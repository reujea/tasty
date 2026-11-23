//! 엔진별 훅 등록·감시 상태와 발화한 훅의 실행을 소유한다.
//! 여러 엔진이 공유하는 handler 정의 registry는 `hook_handler`에 있고, 여기에는 엔진마다 따로 두는 상태와
//! 바인딩 실행·셸 명령 공유 실행기(`shell_pool`)·IpcSequence worker를 둔다. 실행기·worker와 OS 프로세스는 이 모듈의 private 자원이다.
//! 이름에 global이 붙은 전역 훅도 엔진별 등록이며 프로세스 전역 원본으로 합치지 않는다([Agent task runner](../../docs/dev-guide/agent-runner.md)).

mod engine;
pub(crate) mod global;
pub(crate) mod notice;
pub(crate) mod runs;
pub(crate) mod shell_pool;
mod trigger;
mod worker;

pub(crate) use worker::{Delivery, SequenceNotQueued, enqueue_sequence};

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64};

use tasty_hooks::{FiredHook, HookBinding, HookEvent, HookManager};

use crate::core::host_event::PendingHostEvent;
use global::{GlobalHookManager, HookCondition};

/// 발화한 바인딩을 실행할 때 쓰는 IPC 주입기. 없으면 IpcSequence handler를 건너뛴다.
pub(crate) struct HookExecutor {
    injector: Option<tasty_ipc::host_call::HostIpcInjector>,
}

impl HookExecutor {
    pub(crate) fn new(injector: Option<tasty_ipc::host_call::HostIpcInjector>) -> Self {
        Self { injector }
    }

    /// 바인딩 실행을 시작만 하고 셸 작업의 완료는 기다리지 않는다(셸 작업은 공유 실행기에 넣는다).
    fn run(&self, fired: &FiredHook, surface_id: u32) {
        trigger::execute_binding(
            &fired.binding,
            self.injector.as_ref(),
            &fired.event,
            &fired.received,
            surface_id,
        );
    }
}

/// 자연 발생 이벤트의 HookFired event_kind. 등록 패턴이나 관측 값은 싣지 않는다.
fn observed_kind(event: &HookEvent) -> &'static str {
    match event {
        HookEvent::ProcessExit => "process-exit",
        HookEvent::OutputMatch(_) => "output-match",
        HookEvent::Bell => "bell",
        HookEvent::Notification => "notification",
        HookEvent::IdleTimeout(_) => "idle-timeout",
        HookEvent::CommandCompleted(_) => "command-completed",
        HookEvent::Custom(_) => "custom",
    }
}

/// surface 훅과 전역 훅의 엔진별 등록·감시 상태. ID 카운터는 다른 엔진과 공유한다.
pub(crate) struct HookRuntimeState {
    surface: HookManager,
    global: GlobalHookManager,
}

impl HookRuntimeState {
    pub(crate) fn with_counters(surface_ids: Arc<AtomicU64>, global_ids: Arc<AtomicU32>) -> Self {
        Self {
            surface: HookManager::with_counter(surface_ids),
            global: GlobalHookManager::with_counter(global_ids),
        }
    }

    pub(crate) fn surface_hooks(&self) -> &HookManager {
        &self.surface
    }

    pub(crate) fn global_hooks(&self) -> &GlobalHookManager {
        &self.global
    }

    pub(crate) fn add_surface_hook(
        &mut self,
        surface_id: u32,
        event: HookEvent,
        binding: HookBinding,
        once: bool,
    ) -> u64 {
        self.surface.add_hook(surface_id, event, binding, once)
    }

    pub(crate) fn remove_surface_hook(&mut self, hook_id: u64) -> bool {
        self.surface.remove_hook(hook_id)
    }

    /// 닫힌 surface의 훅을 모두 해제한다. 원격 자식 mirror 면 늦게 오는 종료 통지를 위해 훅을 옮겨 둔다.
    pub(crate) fn forget_surface(&mut self, surface_id: u32) {
        #[cfg(feature = "gui")]
        if crate::remote::agent_children::is_client_child(surface_id) {
            let hooks = self
                .surface
                .list_hooks(Some(surface_id))
                .into_iter()
                .map(|h| (h.event.clone(), h.binding.clone(), h.once))
                .collect();
            crate::remote::agent_children::client_surface_closing(surface_id, hooks);
        }
        self.surface.remove_surface_hooks(surface_id);
    }

    #[cfg(feature = "gui")]
    /// 엔진 밖에 옮겨 둔 훅을 발화한다. 원격 자식 mirror 가 닫힌 뒤 온 이벤트에 쓴다.
    pub(crate) fn fire_detached(
        exec: &HookExecutor,
        manager: &mut HookManager,
        surface_id: u32,
        event: HookEvent,
        event_kind: &str,
    ) -> Vec<PendingHostEvent> {
        let fired = manager.check_and_fire(surface_id, std::slice::from_ref(&event));
        Self::run_fired(exec, fired, surface_id, event_kind, None)
    }

    /// OutputMatch 훅이 있으면 PTY 출력 이벤트를 켜야 한다.
    pub(crate) fn has_output_match_hook(&self, surface_id: u32) -> bool {
        self.surface.has_output_match_hook(surface_id)
    }

    pub(crate) fn add_global_hook(
        &mut self,
        condition: HookCondition,
        command: String,
        label: Option<String>,
    ) -> u32 {
        self.global.add(condition, command, label)
    }

    pub(crate) fn remove_global_hook(&mut self, hook_id: u32) -> bool {
        self.global.remove(hook_id)
    }

    /// 관측한 이벤트에 일치한 훅의 바인딩을 실행하고 호스트에 전달할 HookFired를 돌려준다.
    /// HookFired는 훅이 발화했다는 뜻이며 바인딩의 셸 작업이 끝났다는 뜻이 아니다.
    pub(crate) fn fire(
        &mut self,
        exec: &HookExecutor,
        surface_id: u32,
        event: HookEvent,
    ) -> Vec<PendingHostEvent> {
        let kind = observed_kind(&event).to_string();
        self.fire_as(exec, surface_id, event, kind)
    }

    /// fire와 같지만 event_kind를 호출자가 정한다. 수동 발화는 요청한 이벤트 문자열을 싣는다.
    /// CommandCompleted의 종료 코드는 작업 완료 전략이 성공·실패를 판정하도록 함께 싣는다.
    pub(crate) fn fire_as(
        &mut self,
        exec: &HookExecutor,
        surface_id: u32,
        event: HookEvent,
        event_kind: String,
    ) -> Vec<PendingHostEvent> {
        let exit_code = match &event {
            HookEvent::CommandCompleted(code) => *code,
            _ => None,
        };
        let fired = self
            .surface
            .check_and_fire(surface_id, std::slice::from_ref(&event));
        Self::run_fired(exec, fired, surface_id, &event_kind, exit_code)
    }

    /// 마지막 출력 뒤 경과가 기준을 넘은 IdleTimeout 훅을 발화한다.
    /// last_output_at이 None인 surface는 터미널이 없으므로 건너뛴다.
    pub(crate) fn fire_idle_timeouts(
        &mut self,
        exec: &HookExecutor,
        last_output_at: impl Fn(u32) -> Option<std::time::Instant>,
    ) -> Vec<PendingHostEvent> {
        let surface_ids: std::collections::HashSet<u32> = self
            .surface
            .list_hooks(None)
            .iter()
            .filter(|h| matches!(h.event, HookEvent::IdleTimeout(_)))
            .map(|h| h.surface_id)
            .collect();
        let kind = observed_kind(&HookEvent::IdleTimeout(0));
        let mut out = Vec::new();
        for sid in surface_ids {
            let Some(at) = last_output_at(sid) else {
                continue;
            };
            let fired = self
                .surface
                .check_idle_timeouts(sid, at.elapsed().as_secs(), at);
            out.extend(Self::run_fired(exec, fired, sid, kind, None));
        }
        out
    }

    fn run_fired(
        exec: &HookExecutor,
        fired: Vec<FiredHook>,
        surface_id: u32,
        event_kind: &str,
        exit_code: Option<i32>,
    ) -> Vec<PendingHostEvent> {
        fired
            .iter()
            .map(|f| {
                exec.run(f, surface_id);
                PendingHostEvent::HookFired {
                    hook_id: f.hook_id,
                    event_kind: event_kind.to_string(),
                    surface_id,
                    exit_code,
                }
            })
            .collect()
    }

    /// 조건을 만족한 전역 훅의 셸 명령을 셸 명령 훅과 같은 실행기에 넣는다. 호출 스레드는 기다리지 않는다.
    /// 실행기가 받지 않아도 once 소진과 interval·mtime 기록은 이미 갱신된 상태다. 이전 실행이 남아 있어
    /// 건너뛴 훅의 기록은 `GlobalHookManager::tick`을 따른다.
    pub(crate) fn run_due_global_hooks(&mut self) {
        global::run_due(&mut self.global, shell_pool::shared());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> HookRuntimeState {
        HookRuntimeState::with_counters(Arc::new(AtomicU64::new(0)), Arc::new(AtomicU32::new(0)))
    }

    /// 등록부에 없는 handler는 실행 단계에서 로그만 남기고 건너뛰므로 셸을 띄우지 않는다.
    fn inert() -> HookBinding {
        HookBinding::Handler("test.missing-handler".into())
    }

    fn hook_fired(event: &PendingHostEvent) -> (u64, &str, u32, Option<i32>) {
        match event {
            PendingHostEvent::HookFired {
                hook_id,
                event_kind,
                surface_id,
                exit_code,
            } => (*hook_id, event_kind.as_str(), *surface_id, *exit_code),
            other => panic!("HookFired가 아니다: {other:?}"),
        }
    }

    #[test]
    fn fire_reports_each_matching_hook_with_the_observed_kind() {
        let mut rt = runtime();
        let exec = HookExecutor::new(None);
        let bell = rt.add_surface_hook(7, HookEvent::Bell, inert(), false);
        rt.add_surface_hook(8, HookEvent::Bell, inert(), false);
        rt.add_surface_hook(7, HookEvent::ProcessExit, inert(), false);

        let fired = rt.fire(&exec, 7, HookEvent::Bell);

        assert_eq!(fired.len(), 1);
        assert_eq!(hook_fired(&fired[0]), (bell, "bell", 7, None));
    }

    #[test]
    fn command_completed_carries_the_exit_code_and_once_hooks_fire_once() {
        let mut rt = runtime();
        let exec = HookExecutor::new(None);
        let id = rt.add_surface_hook(3, HookEvent::CommandCompleted(None), inert(), true);

        let first = rt.fire(&exec, 3, HookEvent::CommandCompleted(Some(2)));
        let second = rt.fire(&exec, 3, HookEvent::CommandCompleted(Some(2)));

        assert_eq!(first.len(), 1);
        assert_eq!(hook_fired(&first[0]), (id, "command-completed", 3, Some(2)));
        assert!(second.is_empty(), "once 훅이 다시 발화했다");
    }

    #[test]
    fn fire_as_keeps_the_caller_kind() {
        let mut rt = runtime();
        let exec = HookExecutor::new(None);
        let id = rt.add_surface_hook(1, HookEvent::Custom("x".into()), inert(), false);

        let fired = rt.fire_as(&exec, 1, HookEvent::Custom("x".into()), "x".into());

        assert_eq!(hook_fired(&fired[0]), (id, "x", 1, None));
    }

    #[test]
    fn idle_timeouts_fire_once_per_output_epoch_and_skip_surfaces_without_terminal() {
        let mut rt = runtime();
        let exec = HookExecutor::new(None);
        let id = rt.add_surface_hook(4, HookEvent::IdleTimeout(1), inert(), false);
        rt.add_surface_hook(5, HookEvent::IdleTimeout(1), inert(), false);
        let at = std::time::Instant::now() - std::time::Duration::from_secs(5);
        let last_output = |sid: u32| (sid == 4).then_some(at);

        let first = rt.fire_idle_timeouts(&exec, last_output);
        let second = rt.fire_idle_timeouts(&exec, last_output);

        assert_eq!(first.len(), 1);
        assert_eq!(hook_fired(&first[0]), (id, "idle-timeout", 4, None));
        assert!(second.is_empty(), "같은 출력 시각에서 다시 발화했다");
    }
}
