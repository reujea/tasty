//! 저장된 원격 행의 연결과 재연결을 처리한다.
//! SSH 연결 준비는 워커가 맡고 메인 루프가 결과를 mirror 세션에 적용한다.
//! 아직 붙지 않은 행은 그 행이 보이면(포커스된 창의 활성 workspace) 연결하고, 연결이 끊긴 행은
//! 재연결 시간 동안 예약 시각마다 다시 시도한 뒤 형상을 남긴 끊긴 행이 된다.
//! docs/dev-guide/attach-behavior.md#gui-자동-재연결-스코프 참조.

mod notice;
// X11 이벤트 루프로 App 을 만드는 시험이라 Linux 에서만 컴파일한다(preset_shutdown_tests 와 같다).
#[cfg(all(test, target_os = "linux"))]
mod reconnect_window_tests;
pub(crate) mod row_use;
// X11 이벤트 루프로 App 을 만드는 시험이라 Linux 에서만 컴파일한다(preset_shutdown_tests 와 같다).
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod test_rig;

use std::time::Instant;

use tasty_remote_profiles::{Passkeys, RemoteProfiles};
use tasty_ssh::{self as ssh, PortMode, SshTunnel};

use crate::app::App;
use crate::model::RemoteBinding;
use crate::view::ui::View as _;
use tasty_remote::client_session::SessionState;
use tasty_remote::outbound::{AutoAttachOutcome, ReconnectSlot};
use tasty_remote::refusal::RowNoticeKind;

pub(crate) use notice::connect_failure_kind;

fn reconnect_due(slot: Option<&ReconnectSlot>, now: Instant) -> bool {
    match slot {
        Some(slot) if slot.given_up => false,
        Some(slot) => now >= slot.next_attempt,
        None => true,
    }
}

/// 다음 재연결을 예약할 시각. 슬롯이 없는 첫 시도는 현재 프레임에서 처리한다.
fn reconnect_wakeup_at(slot: Option<&ReconnectSlot>) -> Option<Instant> {
    match slot {
        Some(slot) if slot.given_up => None,
        Some(slot) => Some(slot.next_attempt),
        None => None,
    }
}

/// 재연결 구간이 시작된 뒤 재연결 시간이 지났는지.
fn reconnect_window_elapsed(
    slot: &ReconnectSlot,
    now: Instant,
    window: std::time::Duration,
) -> bool {
    now.saturating_duration_since(slot.started) >= window
}

/// 사용자 대기 갈래 L4(권고 a): 아직 붙지 않은 저장된 원격 행이 보이게 되면 연결한다. 재시작 뒤
/// 복원된 활성 행, 창 포커스 전환, 이웃 행 선택도 여기 든다. b로 정해지면 `edge_now`(사용자가
/// 그 행으로 옮긴 프레임)만 true를 돌려준다.
fn visible_row_connects(_edge_now: bool) -> bool {
    true
}

/// 사용자 대기 갈래 L1(권고 a): 이 실행 중 사용자가 연결을 끊은 행은 명시적인 연결 전까지 자동으로
/// 붙지 않는다. b로 정해지면 다음에 그 행을 열 때(`edge_now`) 다시 붙는다.
fn user_disconnect_holds(user_disconnected: bool, _edge_now: bool) -> bool {
    user_disconnected
}

impl App {
    /// 활성 workspace의 전환을 한 번 계산해 연결과 재연결이 같은 전환을 보게 한다.
    pub(crate) fn poll_auto_attach(&mut self) {
        let prev_active = self.remote.last_active_ws;
        let current_ws_id = self
            .focused_pair()
            .and_then(|(main, engine)| {
                engine
                    .core
                    .workspace_at(main.state.active_workspace_index(engine.core))
            })
            .map(|ws| ws.id);
        self.remote.last_active_ws = current_ws_id;

        self.maybe_connect_visible_row(current_ws_id, prev_active);
        self.maybe_trigger_reconnect(current_ws_id, prev_active);
        self.drain_auto_attach_results();
        self.sync_mapping_notices();
        self.publish_row_states();
    }

    /// 타이머만 등록·해제한다. 중단 상태를 담은 재연결 슬롯은 보존한다.
    pub(crate) fn sync_reconnect_timers(&mut self, now: Instant) {
        let mut wakeups = self.reconnect_wakeups();
        // 연결을 기다리는 요청의 상한에도 깨워 시간 초과로 답한다.
        for (row, deadline) in self.row_waits.deadlines() {
            match wakeups.iter_mut().find(|(r, _)| *r == row) {
                Some((_, at)) => *at = (*at).min(deadline),
                None => wakeups.push((row, deadline)),
            }
        }
        crate::app::timers::sync_reconnect_timers(&mut self.timers, &wakeups, now);
    }

    /// 재연결 중인 행마다 깨울 시각. 재연결 시간이 끝나는 시각에는 시도가 진행 중이어도 깨워
    /// 끊긴 행으로 바꾼다. 워커가 떠 있는 행의 다음 시도는 결과 이벤트(`AutoAttachReady`)가 깨운다.
    fn reconnect_wakeups(&self) -> Vec<(u32, Instant)> {
        let window = self.reconnect_window();
        self.remote
            .sessions
            .iter()
            .filter(|s| s.state() == SessionState::Reconnecting)
            .filter_map(|s| s.saved_row())
            .filter_map(|row| {
                let slot = self.remote.reconnect.get(&row);
                let deadline = slot.map(|slot| slot.started + window);
                let next = reconnect_wakeup_at(slot).filter(|_| !self.remote.active.contains(&row));
                match (next, deadline) {
                    (Some(next), Some(deadline)) => Some((row, next.min(deadline))),
                    (next, deadline) => next.or(deadline).map(|at| (row, at)),
                }
            })
            .collect()
    }

    /// 저장된 원격 행의 연결 대상. 창과 parked engine을 모두 본다.
    pub(crate) fn saved_row_binding(&self, row: u32) -> Option<RemoteBinding> {
        self.engines.all_sessions().find_map(|session| {
            let core = &session.core_state;
            core.find_workspace_index_for_id(row)
                .and_then(|index| core.workspace_at(index))
                .and_then(|ws| ws.remote_row.clone())
        })
    }

    /// 같은 연결 대상의 저장된 원격 행. 창과 parked engine을 모두 본다.
    pub(crate) fn saved_row_for_binding(&self, binding: &RemoteBinding) -> Option<u32> {
        self.engines.all_sessions().find_map(|session| {
            session
                .core_state
                .mirror_workspaces()
                .iter()
                .find(|ws| ws.remote_row.as_ref() == Some(binding))
                .map(|ws| ws.id)
        })
    }

    fn row_session_phase(&self, row: u32) -> Option<SessionState> {
        self.remote
            .sessions
            .iter()
            .find(|session| session.state.saved_row == Some(row))
            .map(|session| session.state.phase)
    }

    /// 포커스된 창의 활성 workspace가 붙지 않은 저장된 원격 행이면 연결을 시작한다.
    /// 재연결 시간을 넘겨 끊긴 행은 사용자가 그 행으로 다시 옮겼을 때만 다시 붙는다.
    fn maybe_connect_visible_row(&mut self, current_ws_id: Option<u32>, prev_active: Option<u32>) {
        let Some(row) = current_ws_id else {
            return;
        };
        if self.saved_row_binding(row).is_none() || self.remote.active.contains(&row) {
            return;
        }
        let edge_now = is_reactivation_edge(current_ws_id, prev_active);
        let user_disconnected = self.remote.user_disconnected.contains(&row);
        if user_disconnect_holds(user_disconnected, edge_now) {
            return;
        }
        match self.row_session_phase(row) {
            Some(SessionState::Connected | SessionState::Reconnecting) => {}
            Some(SessionState::Disconnected) => {
                // 다른 곳에서 닫힌 행은 다시 붙을 곳이 없어 옮겨 와도 시도하지 않는다.
                if edge_now && !self.row_closed_elsewhere(row) {
                    self.connect_saved_row(row, true);
                }
            }
            None => {
                // 실패한 행은 사용자가 다시 그 행으로 옮기거나 연결 버튼을 눌러야 다시 시도한다.
                // 원격 workspace 가 없다고 확인된 행은 이번 실행에서 다시 시도하지 않는다.
                let waiting = self.remote.pending_reactivation.contains(&row);
                if visible_row_connects(edge_now)
                    && (!waiting || edge_now)
                    && !self.row_closed_elsewhere(row)
                {
                    self.connect_saved_row(row, edge_now);
                }
            }
        }
    }

    /// 저장된 원격 행의 연결을 시작한다. 연결된 행이나 시도 중인 행은 그대로 둔다.
    /// 형상이 남은 끊긴 행은 새 재연결 구간을 시작하고, 아직 붙지 않은 행은 첫 연결을 한다.
    /// 사용자의 명시적 연결(팝업·메뉴)도 이 함수를 쓰며 그때는 사용자가 끊은 표지를 먼저 지운다.
    pub(crate) fn connect_saved_row(&mut self, row: u32, from_user: bool) {
        self.connect_saved_row_with_run(row, from_user, None);
    }

    /// `run`이 있으면 이 연결의 결과를 그 run에 남긴다(`remote.attach`가 기존 행에 합류할 때).
    pub(crate) fn connect_saved_row_with_run(
        &mut self,
        row: u32,
        from_user: bool,
        run: Option<u64>,
    ) {
        if from_user {
            // 사용자가 다시 연결하면 앞서 끊은 뜻은 끝난다. 다음 단절은 자동 재연결과 시간 만료를 따른다.
            self.remote.user_disconnected.remove(&row);
        }
        if self.remote.active.contains(&row) {
            run.into_iter().for_each(crate::request_runs::forget);
            return;
        }
        let Some(binding) = self.saved_row_binding(row) else {
            run.into_iter().for_each(crate::request_runs::forget);
            return;
        };
        self.ensure_row_token(row);
        self.remote.pending_reactivation.remove(&row);
        match self.row_session_phase(row) {
            // 재동기화(Resync) 중인 행은 이미 다시 붙는 중이라 run 을 그 결과에 합류시킨다.
            Some(SessionState::Connected) if !self.row_link_live(row) => {
                self.row_waits.join_run(row, run);
            }
            Some(SessionState::Connected) => {
                run.into_iter().for_each(crate::request_runs::forget);
            }
            // 사용자의 명시적 연결만 새 재연결 구간을 시작한다. 요청은 지금 구간의 예약 시도에 합류해
            // 시간 창과 백오프를 그대로 둔다.
            Some(SessionState::Reconnecting) if from_user => {
                self.remote.reconnect.insert(row, ReconnectSlot::new());
                self.trigger_reconnect_row(row, None, None, run);
            }
            Some(SessionState::Reconnecting) => {
                self.row_waits.join_run(row, run);
            }
            Some(SessionState::Disconnected) => {
                if let Some(session) = self
                    .remote
                    .sessions
                    .iter_mut()
                    .find(|session| session.state.saved_row == Some(row))
                {
                    session.state.phase = SessionState::Reconnecting;
                }
                self.remote.reconnect.insert(row, ReconnectSlot::new());
                self.trigger_reconnect_row(row, None, None, run);
            }
            None => self.start_first_connect(row, binding, run),
        }
    }

    /// 행의 연결 토큰이 없으면 만든다. 토큰이 없으면 시작한 시도가 바로 낡은 것으로 버려진다.
    pub(crate) fn ensure_row_token(&mut self, row: u32) {
        for session in self.engines.all_sessions_mut() {
            if session.core_state.has_workspace(row) {
                session
                    .remote
                    .row_tokens
                    .entry(row)
                    .or_insert_with(|| std::sync::Arc::new(()));
            }
        }
    }

    fn start_first_connect(&mut self, row: u32, binding: RemoteBinding, run: Option<u64>) {
        let attempt = match self.remote.begin_attempt(Some(row), Some(binding.clone())) {
            Ok(attempt) => attempt,
            Err(error) => {
                run.into_iter().for_each(crate::request_runs::forget);
                tracing::warn!("{error}");
                return;
            }
        };
        let endpoint_target = match self.mirror_install_target(None, Some(row), None, false) {
            Ok(target) => target.with_run(run),
            Err(error) => {
                run.into_iter().for_each(crate::request_runs::forget);
                self.remote.finish_attempt(&attempt);
                tracing::warn!("remote target unavailable: {error}");
                return;
            }
        };
        self.state
            .mirror_attempts
            .register_endpoint(attempt.clone(), endpoint_target);
        self.remote.active.insert(row);
        let spawned = self.spawn_endpoint_attempt(
            attempt,
            row,
            binding.remote_workspace,
            binding.profile,
            false,
        );
        if let Err(error) = spawned {
            self.reject_endpoint_start(row, error, false);
        }
    }

    /// 재연결 중인 행을 예약 시각이나 사용자가 그 행으로 돌아왔을 때 다시 붙인다.
    /// 재연결 시간을 넘기면 끊긴 행으로 바꾼다.
    fn maybe_trigger_reconnect(&mut self, current_ws_id: Option<u32>, prev_active: Option<u32>) {
        self.end_expired_reconnect_windows();
        let rows: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .filter(|s| s.state() == SessionState::Reconnecting)
            .filter_map(|s| s.saved_row())
            .collect();
        for row in rows {
            if self.remote.active.contains(&row) {
                continue;
            }
            self.trigger_reconnect_row(row, current_ws_id, prev_active, None);
        }
    }

    /// 재연결 시간이 지난 재연결 중인 행을 끊긴 행으로 바꾼다. 진행 중인 시도와 park한 창에서 설치를
    /// 기다리는 연결도 시간 창과 함께 끝낸다. 결과를 받거나 설치하기 전에도 불러, 시간이 지난 뒤
    /// 준비된 연결이 이번 루프의 만료 검사보다 먼저 설치되지 않게 한다.
    pub(crate) fn end_expired_reconnect_windows(&mut self) {
        let window = self.reconnect_window();
        let now = Instant::now();
        let expired: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .filter(|s| s.state() == SessionState::Reconnecting)
            .filter_map(|s| s.saved_row())
            .filter(|row| {
                self.remote
                    .reconnect
                    .get(row)
                    .is_some_and(|slot| reconnect_window_elapsed(slot, now, window))
            })
            .collect();
        for row in expired {
            self.abandon_row_attempts(row, "the reconnect window ran out");
            self.enter_disconnected(row);
            self.notify_reconnect_giveup(row);
        }
    }

    /// `current_ws_id`가 None이면 예약 시각과 관계없이 지금 시도한다(명시적 연결).
    fn trigger_reconnect_row(
        &mut self,
        row: u32,
        current_ws_id: Option<u32>,
        prev_active: Option<u32>,
        run: Option<u64>,
    ) {
        // 아래에서 시도를 시작하지 않고 끝나면 run을 결과 없이 둔다.
        let forget = |run: Option<u64>| run.into_iter().for_each(crate::request_runs::forget);
        if self.remote.active.contains(&row) {
            forget(run);
            return;
        }
        let edge_now = current_ws_id.is_none()
            || (current_ws_id == Some(row) && is_reactivation_edge(current_ws_id, prev_active));
        let due = reconnect_due(self.remote.reconnect.get(&row), Instant::now());
        if !edge_now && !due {
            forget(run);
            return;
        }
        // 대기 중 행이 닫혔을 수 있으므로 다시 읽는다.
        let Some(binding) = self.saved_row_binding(row) else {
            forget(run);
            forget_row(&mut self.remote, row);
            return;
        };
        let attempt = match self.remote.begin_attempt(Some(row), Some(binding.clone())) {
            Ok(attempt) => attempt,
            Err(error) => {
                forget(run);
                tracing::warn!("{error}");
                return;
            }
        };
        let index = self
            .remote
            .sessions
            .iter()
            .position(|session| session.state.saved_row == Some(row));
        let endpoint_target = match self.mirror_install_target(None, Some(row), index, false) {
            Ok(target) => target.with_run(run),
            Err(error) => {
                forget(run);
                self.remote.finish_attempt(&attempt);
                tracing::warn!("remote reconnect target unavailable: {error}");
                return;
            }
        };
        self.state
            .mirror_attempts
            .register_endpoint(attempt.clone(), endpoint_target);
        self.remote.active.insert(row);
        let spawned = self.spawn_endpoint_attempt(
            attempt,
            row,
            binding.remote_workspace,
            binding.profile,
            true,
        );
        if let Err(error) = spawned {
            self.reject_endpoint_start(row, error, true);
        }
    }

    fn reject_endpoint_start(&mut self, row: u32, error: String, reconnect: bool) {
        self.state.mirror_attempts.discard_inactive_endpoints();
        self.remote.active.remove(&row);
        let stage = if reconnect { "reconnect" } else { "endpoint" };
        tracing::warn!("remote {stage} start rejected: {error}");
    }

    // Both entry paths have already fixed the original target and registered the attempt.
    fn spawn_endpoint_attempt(
        &mut self,
        attempt: tasty_remote::outbound::AttemptToken,
        row: u32,
        remote_ws: u32,
        profile: String,
        is_reconnect: bool,
    ) -> Result<(), String> {
        let tx = self.remote.tx.clone();
        let proxy = self.view.proxy.clone();
        self.remote.spawn_attempt(attempt.clone(), move || {
            let result = resolve_endpoint_bound(&profile, &attempt);
            let outcome = AutoAttachOutcome {
                attempt,
                row: Some(row),
                remote_ws,
                result,
                is_reconnect,
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = tx.send(outcome); // 수신자가 종료되면 결과를 전달할 수 없다.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = proxy.send_event(crate::app::event::AppEvent::AutoAttachReady); // 이벤트 루프 종료 시 깨움 실패를 무시한다.
        })
    }

    pub(crate) fn drain_auto_attach_results(&mut self) {
        self.remote.reap_attempts();
        self.end_expired_reconnect_windows();
        let stale: Vec<_> = self
            .state
            .mirror_attempts
            .endpoint_snapshot()
            .into_iter()
            .filter(|(_, target)| !self.mirror_install_target_is_current(target))
            .map(|(token, target)| (token, target.row))
            .collect();
        for (token, row) in stale {
            self.state
                .mirror_attempts
                .cancel_endpoint(&token, &mut self.remote);
            if let Some(row) = row {
                self.remote.active.remove(&row);
            }
        }
        self.state.mirror_attempts.discard_inactive_endpoints();
        while let Ok(outcome) = self.remote.rx.try_recv() {
            self.apply_auto_attach_outcome(outcome);
        }
    }

    fn take_current_endpoint_target(
        &mut self,
        outcome: &AutoAttachOutcome,
    ) -> Option<crate::app::attach_client::pending::PendingMirrorInstall> {
        let Some(target) = self.state.mirror_attempts.take_endpoint(&outcome.attempt) else {
            self.remote.finish_attempt(&outcome.attempt);
            return None;
        };
        if !self.mirror_install_target_is_current(&target) {
            target.cancel_run(crate::app::attach_client::pending::TARGET_DISCARDED);
            if let Some(retired) = self.remote.finish_attempt(&outcome.attempt)
                && let Some(row) = retired.row
            {
                self.remote.active.remove(&row);
            }
            return None;
        }
        Some(target)
    }

    fn accept_auto_attach_outcome(
        &mut self,
        outcome: &AutoAttachOutcome,
    ) -> Option<(
        crate::app::attach_client::pending::PendingMirrorInstall,
        tasty_remote::outbound::AttemptRecord,
    )> {
        let target = self.take_current_endpoint_target(outcome)?;
        let Some(accepted) = self.remote.finish_attempt(&outcome.attempt) else {
            tracing::debug!("discarding result from retired remote connection attempt");
            target.cancel_run("the attempt was replaced or cancelled");
            return None;
        };
        self.accepted_row_is_current(outcome, &accepted, &target)
            .then_some((target, accepted))
    }

    /// 결과가 시도한 행의 것이고 그 행의 연결 대상이 그대로인지. 아니면 run을 끝낸다.
    fn accepted_row_is_current(
        &mut self,
        outcome: &AutoAttachOutcome,
        accepted: &tasty_remote::outbound::AttemptRecord,
        target: &crate::app::attach_client::pending::PendingMirrorInstall,
    ) -> bool {
        if accepted.row != outcome.row {
            tracing::error!("remote outcome belongs to another row");
            target.fail_run("the attach result belongs to another workspace");
            return false;
        }
        if let Some(row) = accepted.row
            && self.saved_row_binding(row) != accepted.binding
        {
            self.remote.active.remove(&row);
            tracing::debug!("discarding SSH result after the saved row changed");
            target.cancel_run("the saved remote row changed");
            return false;
        }
        true
    }

    fn apply_auto_attach_outcome(&mut self, outcome: AutoAttachOutcome) {
        // Keep the accepted attempt owner through result dispatch, as before extraction.
        let Some((target, accepted)) = self.accept_auto_attach_outcome(&outcome) else {
            self.remote.discard_endpoint_outcome(outcome);
            return;
        };
        let AutoAttachOutcome {
            attempt: _,
            row,
            remote_ws,
            result,
            is_reconnect,
        } = outcome;
        match result {
            Ok((tunnel, port)) => {
                self.handle_auto_attach_connected(
                    target,
                    &accepted,
                    remote_ws,
                    is_reconnect,
                    tunnel,
                    port,
                );
            }
            Err(e) => {
                target.fail_run(format!("the attach endpoint could not be resolved: {e:#}"));
                if let Some(this) = e.downcast_ref::<tasty_remote::self_instance::ThisInstance>() {
                    tracing::warn!(
                        "SSH attach 대상이 이 인스턴스 자신이라 거절했다 (row {row:?}, remote ws {remote_ws}, reconnect={is_reconnect})"
                    );
                    self.record_self_refusal(&accepted, remote_ws, is_reconnect, this.port, true);
                    return;
                }
                tracing::warn!(
                    "attach 엔드포인트 해석 실패 (row {row:?}, remote ws {remote_ws}, reconnect={is_reconnect}): {e}"
                );
                if let (Some(row), Some(binding)) = (row, accepted.binding.as_ref()) {
                    self.remote.active.remove(&row);
                    let kind = notice::resolve_failure_kind(&e);
                    if is_reconnect {
                        if notice::reconnect_keeps_resolve_notice(&kind) {
                            self.remote.refusals.note(row, kind, binding);
                        }
                        self.on_reconnect_attempt_failed(row, &e);
                    } else {
                        self.on_first_connect_failed(row, binding, &e);
                    }
                }
            }
        }
    }

    fn handle_auto_attach_connected(
        &mut self,
        target: crate::app::attach_client::pending::PendingMirrorInstall,
        accepted: &tasty_remote::outbound::AttemptRecord,
        remote_ws: u32,
        is_reconnect: bool,
        tunnel: Option<SshTunnel>,
        port: u16,
    ) {
        let row = accepted.row;
        let run = target.clone();
        // 자기 포트는 debug·release 모두 연결을 시도하기 전에 거절한다.
        let Some(attach_result) = self.queue_endpoint_mirror(target, port, remote_ws, tunnel)
        else {
            run.fail_run(format!("attach target port {port} is this instance"));
            self.record_self_refusal(accepted, remote_ws, is_reconnect, port, false);
            return;
        };
        match attach_result {
            // 접수는 성공 설치가 아니다. 실패 이력은 pending 설치 성공 때만 해제한다.
            Ok(()) => {}
            Err(e) => {
                run.fail_run(format!("the mirror connection could not start: {e}"));
                tracing::warn!(
                    "attach mirror 실패 (row {row:?}, remote ws {remote_ws}, reconnect={is_reconnect}): {e}"
                );
                if let (Some(row), Some(binding)) = (row, accepted.binding.as_ref()) {
                    self.remote.active.remove(&row);
                    if is_reconnect {
                        self.on_reconnect_attempt_failed(row, &e);
                    } else {
                        self.on_first_connect_failed(row, binding, &e);
                    }
                }
            }
        }
    }

    /// 거절을 IPC로 조회할 수 있게 남기고, 같은 대상을 매 프레임 다시 해석하지 않게 한다.
    /// 첫 연결은 그 행으로 다시 옮길 때까지 기다리고, 재연결은 다른 재연결 실패와 같은 백오프를 따른다.
    fn record_self_refusal(
        &mut self,
        accepted: &tasty_remote::outbound::AttemptRecord,
        remote_ws: u32,
        is_reconnect: bool,
        port: u16,
        via_ssh: bool,
    ) {
        self.remote
            .refusals
            .record(tasty_remote::refusal::AttachRefusal {
                attempt: accepted.token.id(),
                row_workspace: accepted.row,
                remote_workspace: remote_ws,
                port,
                via_ssh,
                reconnect: is_reconnect,
            });
        let (Some(row), Some(binding)) = (accepted.row, accepted.binding.as_ref()) else {
            return;
        };
        self.remote.active.remove(&row);
        let error = anyhow::anyhow!("attach target is this instance's own port {port}");
        if is_reconnect {
            self.remote
                .refusals
                .note(row, RowNoticeKind::SelfInstance, binding);
            self.on_reconnect_attempt_failed(row, &error);
        } else {
            self.remote
                .refusals
                .note(row, RowNoticeKind::SelfInstance, binding);
            self.remote.pending_reactivation.insert(row);
        }
    }

    /// 첫 연결이 실패하면 이유를 행 안내로 남기고, 사용자가 그 행으로 다시 옮기거나 연결 버튼을 누를
    /// 때까지 다시 시도하지 않는다. 다시 시도해도 같은 실패가 될 수 있어 SSH 접속을 반복하지 않는다.
    pub(crate) fn on_first_connect_failed(
        &mut self,
        row: u32,
        binding: &RemoteBinding,
        err: &anyhow::Error,
    ) {
        let kind = notice::resolve_failure_kind(err);
        let kind = connect_failure_kind(&err.to_string()).unwrap_or(kind);
        self.remote.refusals.note(row, kind, binding);
        self.remote.active.remove(&row);
        self.remote.pending_reactivation.insert(row);
    }

    /// 재연결 실패는 지수 백오프로 다시 시도한다. 다른 곳에서 사용 중이거나 원격 workspace가 없거나
    /// mirror 체인이면 다시 시도해도 같은 거절이므로 바로 끊긴 행으로 두고 이유를 남긴다.
    pub(super) fn on_reconnect_attempt_failed(&mut self, row: u32, err: &anyhow::Error) {
        let permanent = connect_failure_kind(&err.to_string());
        if let Some(kind) = permanent {
            if let Some(binding) = self.saved_row_binding(row) {
                self.remote.refusals.note(row, kind, &binding);
            }
            self.enter_disconnected(row);
            return;
        }
        let slot = self.remote.reconnect.entry(row).or_default();
        slot.attempts += 1;
        let base = slot.backoff.current();
        slot.backoff.advance();
        use rand::Rng;
        let jitter = 0.8 + rand::rng().random::<f64>() * 0.4;
        slot.next_attempt = Instant::now() + base.mul_f64(jitter);
    }

    /// 재연결 시간이 지나 재연결을 멈췄다. toast 는 없고 행 표지와 연결 팝업이 알린다.
    /// 그 행을 다시 열면 다시 시도한다.
    fn notify_reconnect_giveup(&mut self, row: u32) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if engine.find_workspace_index_for_id(row).is_some() {
                main.mark_dirty();
            }
        }
        tracing::warn!("attach 재연결 중단 (row {row}) — 재연결 시간이 지나 끊긴 행으로 전환");
    }
}

/// 워커 스레드에서 프로필과 SSH 터널 또는 loopback 포트를 준비한다.
fn resolve_endpoint_bound(
    profile: &str,
    attempt: &tasty_remote::outbound::AttemptToken,
) -> anyhow::Result<(Option<SshTunnel>, u16)> {
    resolve_endpoint_bound_with(attempt, || resolve_endpoint(profile))
}

/// 해석 함수를 받아 취소 등록·자기 판정을 붙인다. 시험은 SSH 없이 해석 결과를 넣는다.
fn resolve_endpoint_bound_with<T>(
    attempt: &tasty_remote::outbound::AttemptToken,
    resolve: impl FnOnce() -> anyhow::Result<(Option<T>, u16)>,
) -> anyhow::Result<(Option<T>, u16)> {
    let cancel = tasty_ssh::SshCancel::new();
    attempt
        .register_ssh(cancel.clone())
        .map_err(anyhow::Error::msg)?;
    let _scope = cancel.scope();
    let result = resolve()?;
    if !attempt.is_active() {
        anyhow::bail!("remote endpoint attempt cancelled");
    }
    tasty_remote::self_instance::refuse_this_instance(result.0.is_some(), result.1, Some(attempt))?;
    Ok(result)
}

fn resolve_endpoint(profile: &str) -> anyhow::Result<(Option<SshTunnel>, u16)> {
    let profiles = RemoteProfiles::load();
    let passkeys = Passkeys::load();
    let p = profiles
        .get(profile)
        .ok_or_else(|| notice::ProfileNotFound(profile.to_owned()))?;
    let (ssh_target, remote_tasty, port_mode, port_file) =
        ssh::resolve_attach_target(p, &profiles, &passkeys)?;

    if let Some(port) = parse_loopback_port(&ssh_target.destination) {
        return Ok((None, port));
    }

    let ssh = ssh::resolve_ssh_path();
    let mode = PortMode::parse(&port_mode)?;
    // TASTY_SSH_VERIFY가 설정된 검증 환경에서는 accept-new를 사용한다. 없으면 host key 옵션을
    // 넘기지 않고 사용자 ssh 설정을 따른다.
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);
    let remote_port = ssh::discover_remote_port(
        &ssh,
        &ssh_target,
        &remote_tasty,
        mode,
        verify,
        debug,
        port_file.as_deref(),
    )?;
    let tunnel = SshTunnel::establish(&ssh, &ssh_target, remote_port, verify)?;
    let local_port = tunnel.local_port;
    Ok((Some(tunnel), local_port))
}

/// 행의 연결 시도·재연결·실패 기록을 모두 지운다. 행이 닫히거나 정리될 때 쓴다.
pub(crate) fn forget_row(remote: &mut tasty_remote::outbound::Remote, row: u32) {
    remote.active.remove(&row);
    remote.reconnect.remove(&row);
    remote.pending_reactivation.remove(&row);
    remote.user_disconnected.remove(&row);
    remote.refusals.clear_notice(row);
}

/// 연결에 성공한 행의 재연결 기록을 지운다.
pub(crate) fn forget_row_backoff(remote: &mut tasty_remote::outbound::Remote, row: u32) {
    remote.reconnect.remove(&row);
    remote.pending_reactivation.remove(&row);
}

fn is_reactivation_edge(current: Option<u32>, previous: Option<u32>) -> bool {
    current.is_some() && current != previous
}

/// `127.0.0.1:PORT` / `localhost:PORT` / `[::1]:PORT` 면 PORT 를 돌려준다(loopback 직결).
/// 그 외(원격 호스트/alias)는 None → SSH 터널 경로.
fn parse_loopback_port(dest: &str) -> Option<u16> {
    let (host, port_str) = if let Some(rest) = dest.strip_prefix("[::1]:") {
        ("::1", rest)
    } else {
        let (h, p) = dest.rsplit_once(':')?;
        (h, p)
    };
    let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
    if !is_loopback {
        return None;
    }
    port_str.parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_up_slot_schedules_no_wakeup_but_survives() {
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = Instant::now() + std::time::Duration::from_secs(5);
        slot.given_up = true;
        assert_eq!(reconnect_wakeup_at(Some(&slot)), None);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn active_backoff_schedules_its_next_attempt() {
        let at = Instant::now() + std::time::Duration::from_secs(5);
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = at;
        assert_eq!(reconnect_wakeup_at(Some(&slot)), Some(at));
    }

    #[test]
    fn absent_slot_schedules_no_wakeup_because_it_is_already_due() {
        assert_eq!(reconnect_wakeup_at(None), None);
        assert!(reconnect_due(None, Instant::now()));
    }

    #[test]
    fn reactivation_edge_only_on_transition() {
        assert!(is_reactivation_edge(Some(1), None));
        assert!(!is_reactivation_edge(Some(1), Some(1)));
        assert!(is_reactivation_edge(Some(2), Some(1)));
        assert!(!is_reactivation_edge(None, Some(1)));
    }

    #[test]
    fn the_reconnect_window_counts_from_the_start_of_the_disconnect() {
        let mut slot = ReconnectSlot::new();
        let start = slot.started;
        let window = std::time::Duration::from_secs(300);
        assert!(!reconnect_window_elapsed(&slot, start, window));
        assert!(!reconnect_window_elapsed(
            &slot,
            start + std::time::Duration::from_secs(299),
            window
        ));
        assert!(reconnect_window_elapsed(
            &slot,
            start + std::time::Duration::from_secs(300),
            window
        ));
        // 백오프로 다음 시도가 밀려도 시작 시각은 그대로다.
        slot.next_attempt = start + std::time::Duration::from_secs(900);
        assert!(reconnect_window_elapsed(
            &slot,
            start + std::time::Duration::from_secs(301),
            window
        ));
        assert!(reconnect_window_elapsed(
            &slot,
            start,
            std::time::Duration::ZERO
        ));
    }

    /// 사용자 대기 갈래 L1·L4의 권고안: 보이는 행은 연결하고, 사용자가 끊은 행은 다시 열어도 붙지 않는다.
    #[test]
    fn pending_user_decisions_follow_the_recommended_options() {
        assert!(visible_row_connects(false));
        assert!(visible_row_connects(true));
        assert!(user_disconnect_holds(true, true));
        assert!(!user_disconnect_holds(false, false));
    }

    #[test]
    fn loopback_ports_parsed() {
        assert_eq!(parse_loopback_port("127.0.0.1:45123"), Some(45123));
        assert_eq!(parse_loopback_port("localhost:8080"), Some(8080));
        assert_eq!(parse_loopback_port("[::1]:9000"), Some(9000));
    }

    #[test]
    fn non_loopback_is_none() {
        assert_eq!(parse_loopback_port("gx10"), None);
        assert_eq!(parse_loopback_port("user@host"), None);
        assert_eq!(parse_loopback_port("192.168.0.10:45123"), None);
        assert_eq!(parse_loopback_port("example.com:22"), None);
    }

    #[test]
    fn reconnect_due_when_no_slot_yet() {
        assert!(reconnect_due(None, Instant::now()));
    }

    #[test]
    fn reconnect_due_respects_backoff_next_attempt() {
        let mut slot = ReconnectSlot::new();
        slot.next_attempt = Instant::now() + std::time::Duration::from_secs(10);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
        slot.next_attempt = Instant::now() - std::time::Duration::from_secs(1);
        assert!(reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn reconnect_due_is_false_when_given_up_even_past_next_attempt() {
        let mut slot = ReconnectSlot::new();
        slot.given_up = true;
        slot.next_attempt = Instant::now() - std::time::Duration::from_secs(1);
        assert!(!reconnect_due(Some(&slot), Instant::now()));
    }

    #[test]
    fn automatic_attach_asks_a_tunnel_peer_and_refuses_this_instance() {
        let (port, server) = fake_peer::serve_once(
            serde_json::json!({ "instance_id": tasty_ipc::instance::instance_id() }),
        );
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        let error = resolve_endpoint_bound_with(&attempt, || Ok((Some(()), port)))
            .expect_err("this instance");
        assert!(server.join().expect("server").contains("\"system.info\""));
        assert!(
            error
                .downcast_ref::<tasty_remote::self_instance::ThisInstance>()
                .is_some(),
            "{error}"
        );
    }

    /// 프로필 파일 해석이 실제로 돌려주는 오류가 거절 안내의 `ProfileMissing` 으로 분류되는지 본다.
    /// 오류를 감싸는 단계가 생기면 안내가 `Unresolved` 로 바뀌어 배너의 이유가 달라진다.
    #[test]
    fn a_missing_profile_mapping_resolves_to_the_profile_missing_notice() {
        let name = format!(
            "no-such-profile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        let Err(error) = resolve_endpoint_bound(&name, &attempt) else {
            panic!("a profile that does not exist cannot resolve");
        };
        assert_eq!(
            notice::resolve_failure_kind(&error),
            RowNoticeKind::ProfileMissing,
            "{error}"
        );
        assert!(error.to_string().contains(&name), "{error}");
    }

    #[test]
    fn automatic_attach_does_not_ask_a_direct_endpoint() {
        let mut remote = tasty_remote::outbound::Remote::new();
        let attempt = remote.begin_attempt(None, None).expect("attempt");
        // 아무도 듣지 않는 포트라도 터널이 아니면 묻지 않는다.
        let port = fake_peer::free_port();
        let resolved = resolve_endpoint_bound_with(&attempt, || Ok((None::<()>, port)));
        assert_eq!(resolved.expect("direct endpoint").1, port);
    }
}

/// SSH 경로의 자기 판정 배선을 시험하는 가짜 `system.info` 상대.
#[cfg(test)]
pub(crate) mod fake_peer {
    use std::io::{BufRead, BufReader, Write};

    /// 요청 한 줄을 읽고 주어진 결과 한 줄을 돌려준다. 받은 요청 줄을 돌려준다.
    pub(crate) fn serve_once(result: serde_json::Value) -> (u16, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).expect("read");
            let reply = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result });
            writeln!(&stream, "{reply}").expect("write");
            line
        });
        (port, handle)
    }

    /// 바인드했다 놓은 포트. 묻지 않는 경로가 연결하면 거절돼 드러난다.
    pub(crate) fn free_port() -> u16 {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .expect("free port")
            .port()
    }
}
