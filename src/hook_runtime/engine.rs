//! Engine timer adapters provide terminal observations to the hook runtime.
use crate::core::host_event::PendingHostEvent;
use crate::runtime::engine_access::EngineMut;

impl EngineMut<'_> {
    pub(crate) fn poll_global_hooks(&mut self) {
        self.hooks.run_due_global_hooks();
    }
    /// Host event delivery remains the caller's responsibility.
    pub(crate) fn fire_idle_timeout_hooks(
        &mut self,
        exec: &crate::hook_runtime::HookExecutor,
    ) -> Vec<PendingHostEvent> {
        let terminals = &self.runtime.terminals;
        self.hooks
            .fire_idle_timeouts(exec, |sid| terminals.get(sid).map(|t| t.silence_since()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};
    use tasty_hooks::{HookBinding, HookEvent};

    /// Given 2초 전 출력으로 이미 발화한 idle-timeout 훅, When OS 크기 통지 뒤 재그리기 출력이 오면,
    /// Then 그 훅은 다시 발화하지 않고, 새 훅은 재그리기 전 출력부터 센 경과로 발화한다.
    /// 경과·epoch를 마지막 출력 시각으로 세면 앞 훅이 다시 발화하고 새 훅은 발화하지 않는다.
    #[test]
    fn idle_timeout_counts_from_the_output_before_a_resize_redraw() {
        let (_state, mut session) = crate::state::tests::test_state();
        let sid = 7_702;
        let mut terminal = tasty_terminal::Terminal::new_detached(80, 24);
        terminal.feed_bytes_at(b"work", Instant::now() - Duration::from_secs(2));
        session.runtime.terminals.insert(sid, terminal, None);
        let mut engine = session.borrow_mut();
        let inert = || HookBinding::Handler("test.missing-handler".into());
        let first = engine
            .hooks
            .add_surface_hook(sid, HookEvent::IdleTimeout(0), inert(), false);
        let exec = crate::hook_runtime::HookExecutor::new(None);
        assert_eq!(engine.fire_idle_timeout_hooks(&exec).len(), 1);

        let t = engine.runtime.terminals.get_mut(sid).expect("terminal");
        t.note_resize_redraw_at(Instant::now());
        t.feed_bytes(b"\x1b[Hredraw");
        let second = engine
            .hooks
            .add_surface_hook(sid, HookEvent::IdleTimeout(1), inert(), false);
        let fired: Vec<u64> = engine
            .fire_idle_timeout_hooks(&exec)
            .iter()
            .map(|event| match event {
                crate::core::host_event::PendingHostEvent::HookFired { hook_id, .. } => *hook_id,
                other => panic!("HookFired가 아니다: {other:?}"),
            })
            .collect();
        assert_eq!(fired, vec![second], "첫 훅 {first}은 같은 침묵 구간이다");
    }
}
