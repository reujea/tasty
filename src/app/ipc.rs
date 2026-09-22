//! IPC dispatch loop — `about_to_wait` 에서 매 프레임 한 회차, `IpcReady` 에서는 양보 규칙
//! ([`IpcPacer`])이 허락할 때만 한 회차.
//!
//! 본 모듈은 loop 본체만 담고, 단계별 처리는 sub-module 로 분산.
//!
//! 단계 순서 (먼저 매칭되는 step 에서 종료):
//!
//! 1. `caller_gate`: session_token → CallerContext 결정 + Agent caller 의 `ensure_allowed`
//!    (audit + capability elevation 처리). Local caller 는 그대로 통과.
//! 2. `app_methods`: 호스트 자체 메서드 (system.shutdown /
//!    window.{create,close,focus,list} / plugin.* / approval.await).
//! 3. `debug_methods` (debug 빌드): debug.event_bus.* / debug.extension.invoke_hook /
//!    debug.popup.*.
//! 4. `window_required`: focused window 가 있어야 처리 가능 (surface.ime_*,
//!    debug.info, debug.inject_*). 3·4 는 멱등 키 보존소를 한 번 지나는 한 묶음이다
//!    (`ipc_step_debug_layers`).
//! 5. `routing`: plugin namespace forward → focused window / parked state fallback.

mod app_methods;
mod caller_gate;
#[cfg(debug_assertions)]
mod debug_methods;
mod routing;
mod window_required;

use crate::app::App;

/// 한 step 의 결과. 호출자(loop) 가 다음 동작을 결정한다.
pub(crate) enum IpcStep {
    /// 이 step 에서 처리되지 않음. 다음 step 시도.
    NotHandled,
    /// 처리됨. 다음 cmd 로.
    Handled,
    /// 처리됨 + tool registry 재집계 표시.
    HandledDirty,
    /// `system.shutdown` 만 — loop 즉시 종료, true 반환. debug 빌드 전용.
    #[cfg(debug_assertions)]
    Shutdown,
}

/// gui 에서 IPC 회차가 루프의 나머지(입력 · 타이머 · 렌더)에 차례를 넘기게 하는 규칙.
///
/// 회차는 두 자리에서 돈다 — `about_to_wait`(매 iteration 한 번)와 `IpcReady` 사용자 이벤트.
/// 뒤쪽이 그대로면 지속 부하에서 루프가 `about_to_wait` 에 영영 못 간다. winit 은 한 iteration
/// 안에서 사용자 이벤트를 **큐가 빌 때까지** 꺼내 처리하는데(x11 실측), 생산자는 명령마다 wake 를
/// 한 번 부르고 회차가 답을 주면 곧바로 다음 명령과 wake 를 보낸다. 회차가 도는 동안 wake 가
/// 다시 쌓이므로 그 큐가 비는 순간이 없고, 그동안 타이머도 창 입력도 렌더도 못 돈다.
///
/// 그래서 **사용자 이벤트가 부른 회차는 직전 회차가 끝난 뒤 한 회차 예산이 지났을 때만 돈다.**
/// 그 사이에 온 wake 는 루프를 깨우는 일만 하고, 명령은 곧 올 `about_to_wait` 의 회차가 집는다.
/// 회차가 안 돌면 답이 안 나가 새 명령도 안 오므로 사용자 이벤트 큐는 곧 빈다.
///
/// 예산에서 잘린 회차는 루프를 한 번 깨우고, 그 재깨움은 양보 규칙을 **한 번** 건너뛴다 —
/// `about_to_wait` 한 번 사이에 한 회차까지. 재깨움이 연 회차가 또 잘리면 다음 재깨움은 규칙을
/// 따른다. 한도를 없애면 잘림과 재깨움이 사용자 이벤트 루프 안에서 사슬을 이뤄 기아가 되살아난다
/// (x11 실측, ADR-0413).
///
/// 사용자 이벤트 쪽 회차를 아예 없애지 않는 이유 — 플랫폼 모달 루프(창 크기 조절 · 메뉴 추적
/// 등)가 `about_to_wait` 없이 사용자 이벤트만 전하는 구간이 있으면, 그 구간에서 IPC 를 살리는
/// 것이 이 경로다. 그 구간이 실제로 있는지는 이 머신(x11)에서 잴 수 없다(ADR-0413).
pub(crate) struct IpcPacer {
    last_round_end: Option<std::time::Instant>,
    /// 직전 회차가 예산에서 잘렸고, 그 몫의 재깨움이 아직 회차를 열지 않았다.
    rewake_owed: bool,
    /// 마지막 `about_to_wait` 뒤로 재깨움이 양보 규칙을 건너뛰어 회차를 이미 한 번 열었다.
    rewake_spent: bool,
    /// 루프를 깨우는 수단 — 제품은 `IpcReady` 를 보내고, 시험은 센다.
    wake: Box<dyn Fn()>,
}

impl IpcPacer {
    pub(crate) fn new(wake: Box<dyn Fn()>) -> Self {
        Self {
            last_round_end: None,
            rewake_owed: false,
            rewake_spent: false,
            wake,
        }
    }

    /// 사용자 이벤트가 지금 회차를 돌려도 되는가.
    ///
    /// 잘린 회차가 남긴 재깨움은 양보 규칙을 건너뛴다 — 단 `about_to_wait` 한 번 사이에 한
    /// 번만. 한도가 없으면 재깨움이 연 회차가 또 잘리고 또 재깨워, 사용자 이벤트 루프가 다시
    /// 끝나지 않는다.
    pub(crate) fn event_may_run_round(&mut self, now: std::time::Instant) -> bool {
        if self.rewake_owed && !self.rewake_spent {
            self.rewake_owed = false;
            self.rewake_spent = true;
            return true;
        }
        self.last_round_end.is_none_or(|end| {
            now.saturating_duration_since(end) >= crate::app::ipc_round::ROUND_TIME_BUDGET
        })
    }

    /// `about_to_wait` 에 닿았다 — 루프의 나머지(타이머 · 렌더)가 차례를 받았으므로 재깨움의
    /// 면제를 다시 허락한다.
    pub(crate) fn loop_reached_about_to_wait(&mut self) {
        self.rewake_spent = false;
    }

    /// 회차가 끝났다. 예산에서 잘렸으면 루프를 한 번 깨운다 — 남은 명령의 wake 는 이미
    /// 건너뛴 이벤트로 소비됐을 수 있어서, 안 깨우면 다른 입력이 올 때까지 남는다.
    fn round_ended(&mut self, at: std::time::Instant, end: tasty_ipc::dispatch::RoundEnd) {
        self.last_round_end = Some(at);
        self.rewake_owed = end != tasty_ipc::dispatch::RoundEnd::Drained;
        if self.rewake_owed {
            (self.wake)();
        }
    }
}

impl App {
    /// Process pending IPC commands. Returns true if any commands were processed.
    ///
    /// 한 회차의 규칙(수 예산 · 시간 예산 · 이월)은 [`crate::app::ipc_round`] 가 정한다 —
    /// headless `pump_ipc` 와 같은 것을 쓴다. 회차가 예산에서 멈췄으면 루프를 한 번 더 깨운다 —
    /// 남은 명령의 wake 는 이미 소비됐을 수 있어서, 안 깨우면 다른 입력이 올 때까지 남는다.
    pub(crate) fn process_ipc(&mut self) -> bool {
        let mut round = crate::app::ipc_round::IpcRound::begin();
        let mut processed = false;
        let mut tool_registry_dirty = false;
        // `ipc_server` 빌림은 `next` 호출 안에서 끝난다 — 꺼낸 명령은 owned 라 handler 의
        // 가변 빌림과 안 겹친다.
        while let Some(cmd) = round.next(self.hub.ipc_server.as_deref()) {
            let observed =
                crate::app::ipc_round::CommandObservation::begin(self.core.pressure(), &cmd);
            let step = self.ipc_dispatch_command(cmd);
            observed.finish(self.core.slow_requests());
            match step {
                #[cfg(debug_assertions)]
                IpcStep::Shutdown => {
                    let end = round.finish(self.core.pressure(), self.core.dispatch());
                    self.ipc_pacer.round_ended(std::time::Instant::now(), end);
                    return true;
                }
                IpcStep::HandledDirty => {
                    tool_registry_dirty = true;
                    processed = true;
                }
                IpcStep::Handled => processed = true,
                IpcStep::NotHandled => {}
            }
        }
        let end = round.finish(self.core.pressure(), self.core.dispatch());
        self.ipc_pacer.round_ended(std::time::Instant::now(), end);
        if tool_registry_dirty {
            self.refresh_tool_registry();
            self.refresh_palette_plugin_commands();
        }
        processed
    }

    /// 명령 하나를 단계 순서대로 끝까지 다룬다. 답은 이 안에서 나간다.
    ///
    /// 돌려주는 값은 회차가 알아야 할 것뿐이다 — 처리됐는가, tool registry 를 다시 모아야
    /// 하는가, (debug) 종료하라는 명령이었는가. 아무 단계도 답하지 않았으면 `NotHandled`.
    fn ipc_dispatch_command(&mut self, cmd: crate::ipc::server::IpcCommand) -> IpcStep {
        // 큐 체류 시간은 꺼낸 자리(`CommandObservation::begin`)가 이미 쟀다 — handler 실행
        // 시간과 **따로** 잰다. 합쳐 두면 느린 응답을 보고도 적체인지 handler 비용인지 고를 수 없다.
        // 기한이 큐에서 지났으면 실행하지 않고 답한다 — 게이트보다 앞이다(ADR-0411).
        if !crate::app::ipc_round::claim_or_answer(&cmd, self.core.dispatch()) {
            return IpcStep::Handled;
        }
        let caller = match self.ipc_resolve_caller(&cmd) {
            Some(c) => c,
            None => return IpcStep::Handled,
        };
        let checked = match self.gates_before_routing(&cmd.request, &caller) {
            Ok(checked) => checked,
            Err(response) => {
                crate::ipc::server::send_response(&cmd.response_tx, response);
                return IpcStep::Handled;
            }
        };
        match self.ipc_step_app_methods(&cmd, &caller) {
            #[cfg(debug_assertions)]
            IpcStep::Shutdown => return IpcStep::Shutdown,
            IpcStep::HandledDirty => return IpcStep::HandledDirty,
            IpcStep::Handled => return IpcStep::Handled,
            IpcStep::NotHandled => {}
        }
        if matches!(self.ipc_step_debug_layers(&cmd, &caller), IpcStep::Handled) {
            return IpcStep::Handled;
        }
        if matches!(self.ipc_step_routing(&cmd, &checked), IpcStep::Handled) {
            return IpcStep::Handled;
        }
        IpcStep::NotHandled
    }

    /// step 3·4 — debug step 과 window-required step. 둘 다 release 에서는 아무것도 안 맡는다.
    ///
    /// 멱등 키를 실은 `Mutate` 는 보존소를 먼저 지난다 — app_methods step 과 같은 함수다. 이
    /// step 들은 app_methods step **뒤**라 거기서 연 자리가 닫힌 뒤에 오므로, 여기서 다시 열지
    /// 않으면 입력 주입 · `debug.lua.eval` 이 보존소 없이 실행된다(ADR-0566). 두 step 을 한
    /// 함수로 묶는 이유는 보존소를 한 번 여는 것이다 — 따로 감싸면 첫 step 이 안 맡은 이름을
    /// 둘째가 또 연다.
    #[cfg(debug_assertions)]
    fn ipc_step_debug_layers(
        &mut self,
        cmd: &crate::ipc::server::IpcCommand,
        caller: &crate::ipc::caller::CallerContext,
    ) -> IpcStep {
        if let Some(step) = crate::ipc::handler::idempotency::run_app_layer(
            caller,
            cmd,
            IpcStep::Handled,
            |step| matches!(step, IpcStep::Handled),
            |relayed| self.ipc_step_debug_layers(relayed, caller),
        ) {
            return step;
        }
        if matches!(self.ipc_step_debug(cmd), IpcStep::Handled) {
            return IpcStep::Handled;
        }
        self.ipc_step_window_required(cmd)
    }

    /// release 에는 debug step 이 없고 window-required step 은 아무것도 안 맡는다 — 보존소를
    /// 열 이름이 없다.
    #[cfg(not(debug_assertions))]
    fn ipc_step_debug_layers(
        &mut self,
        cmd: &crate::ipc::server::IpcCommand,
        _caller: &crate::ipc::caller::CallerContext,
    ) -> IpcStep {
        self.ipc_step_window_required(cmd)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use tasty_ipc::dispatch::RoundEnd;

    use super::IpcPacer;
    use crate::app::ipc_round::ROUND_TIME_BUDGET;

    /// 깨운 횟수를 세는 페이서.
    fn counting() -> (IpcPacer, Rc<Cell<u32>>) {
        let wakes = Rc::new(Cell::new(0));
        let seen = Rc::clone(&wakes);
        (
            IpcPacer::new(Box::new(move || seen.set(seen.get() + 1))),
            wakes,
        )
    }

    /// 회차가 한 번도 안 돌았으면 사용자 이벤트가 회차를 연다.
    #[test]
    fn the_first_wake_may_run_a_round() {
        assert!(counting().0.event_may_run_round(Instant::now()));
    }

    /// 직전 회차가 방금 끝났으면 사용자 이벤트는 회차를 안 연다 — 이것이 빠지면 지속 부하에서
    /// winit 의 사용자 이벤트 큐가 비지 않아 `about_to_wait` 가 영영 안 온다.
    #[test]
    fn a_wake_right_after_a_round_yields_to_the_rest_of_the_loop() {
        let (mut pacer, _) = counting();
        let end = Instant::now();
        pacer.round_ended(end, RoundEnd::Drained);
        assert!(!pacer.event_may_run_round(end));
        assert!(!pacer.event_may_run_round(end + ROUND_TIME_BUDGET - Duration::from_millis(1)));
    }

    /// 한 회차 예산이 지나면 다시 연다 — `about_to_wait` 없이 사용자 이벤트만 오는 구간에서도
    /// IPC 가 진척한다.
    #[test]
    fn a_wake_a_budget_after_the_last_round_runs_one() {
        let (mut pacer, _) = counting();
        let end = Instant::now();
        pacer.round_ended(end, RoundEnd::Drained);
        assert!(pacer.event_may_run_round(end + ROUND_TIME_BUDGET));
    }

    /// 예산에서 잘린 회차는 루프를 정확히 한 번 깨우고, 큐를 비운 회차는 안 깨운다.
    #[test]
    fn a_cut_round_wakes_the_loop_once_and_a_drained_round_does_not() {
        let (mut pacer, wakes) = counting();
        let now = Instant::now();
        pacer.round_ended(now, RoundEnd::Drained);
        assert_eq!(
            wakes.get(),
            0,
            "a drained round has nothing left to carry over"
        );
        pacer.round_ended(now, RoundEnd::TimeBudget);
        assert_eq!(wakes.get(), 1);
        pacer.round_ended(now, RoundEnd::CountBudget);
        assert_eq!(wakes.get(), 2, "one wake per cut round");
    }

    /// 잘린 회차의 재깨움은 양보 규칙 안에서도 회차를 연다 — 이것이 없으면 `about_to_wait` 가
    /// 안 오는 구간에서 남은 명령이 다음 입력까지 선다.
    #[test]
    fn the_wake_after_a_cut_round_opens_a_round_inside_the_budget() {
        let (mut pacer, _) = counting();
        let end = Instant::now();
        pacer.round_ended(end, RoundEnd::TimeBudget);
        assert!(pacer.event_may_run_round(end));
        assert!(
            !pacer.event_may_run_round(end),
            "the exemption is spent by the one round it opened"
        );
    }

    /// 면제는 `about_to_wait` 한 번 사이에 한 번이다. 재깨움이 연 회차가 또 잘려도 다음
    /// 재깨움은 양보 규칙을 따른다 — 한도가 없으면 잘림과 재깨움이 사용자 이벤트 루프 안에서
    /// 사슬을 이뤄 x11 의 기아가 되살아난다.
    #[test]
    fn a_chain_of_cut_rounds_yields_until_the_loop_reaches_about_to_wait() {
        let (mut pacer, wakes) = counting();
        let end = Instant::now();
        pacer.round_ended(end, RoundEnd::TimeBudget);
        assert!(pacer.event_may_run_round(end));
        pacer.round_ended(end, RoundEnd::TimeBudget);
        assert_eq!(wakes.get(), 2, "the second cut round still wakes the loop");
        assert!(!pacer.event_may_run_round(end), "but its wake yields");
        pacer.loop_reached_about_to_wait();
        assert!(
            pacer.event_may_run_round(end),
            "after about_to_wait it may skip again"
        );
    }
}
