//! surface 닫기 이벤트를 플러그인에 전달한다.

use crate::app::App;

impl App {
    /// 모든 창·parked 상태의 닫기 큐를 처리한다. state가 plugin 타입에 의존하지 않도록
    /// 사용자 닫기 여부를 여기서 LifecycleReason으로 변환한다.
    pub(crate) fn dispatch_pending_surface_lifecycle(&mut self) {
        use tasty_plugin_protocol::EventScope;
        use tasty_plugin_protocol::events::LifecycleReason;
        use tasty_plugin_protocol::events::payloads::SurfaceClosed;
        let mut drained: Vec<crate::state::PendingSurfaceClosed> = Vec::new();
        for session in self.engines.all_sessions_mut() {
            let pending = std::mem::take(&mut session.runtime.pending_plugin_retirements);
            for (surface, binding) in pending {
                if !binding.is_alive() {
                    continue;
                }
                match self.plugin_manager.as_mut().map(|manager| {
                    manager.enqueue_bound_remote_retirement(surface, binding.clone())
                }) {
                    Some(Ok(())) => {}
                    Some(Err(error)) => {
                        tracing::error!(surface, "retaining failed plugin retirement: {error}");
                        session
                            .runtime
                            .pending_plugin_retirements
                            .push((surface, binding));
                    }
                    None => session
                        .runtime
                        .pending_plugin_retirements
                        .push((surface, binding)),
                }
            }
            // Required owner disposal is not replaced by sending its informational close event.
            if session.runtime.pending_plugin_retirements.is_empty() {
                drained.append(&mut session.runtime.pending_lifecycle_events);
            }
        }
        if drained.is_empty() {
            return;
        }
        let scripts = self.autofire_scripts();
        let lua_send = crate::app::dispatch::lua_commands::lua_send!(self);
        let lua = lua_send.as_ref();
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        for ev in drained {
            let bus_reason = if ev.is_user_close {
                LifecycleReason::User
            } else {
                LifecycleReason::Ipc
            };
            let payload = SurfaceClosed {
                surface_id: ev.surface_id,
                kind: ev.kind.unwrap_or_default(),
                reason: bus_reason,
            };
            mgr.emit_host_event("surface.closed", &payload, EventScope::Surface);
            crate::hooks::lua::fire(
                lua,
                crate::hooks::lua::AutofireCtx {
                    scripts: &scripts,
                    guard: &mut self.lua_autofire,
                    runs: &mut self.lua_runs,
                },
                "surface.delete.post",
                &payload,
            );
        }
    }
}
