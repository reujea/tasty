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
