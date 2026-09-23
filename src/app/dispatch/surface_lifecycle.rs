//! Surface close lifecycle 이벤트 broadcast.

use crate::app::App;

impl App {
    /// 모든 윈도우/parked state의 surface close lifecycle 큐를 비우고 구독 plugin에
    /// broadcast한다. `is_user_close` bool → `LifecycleReason` enum 매핑은
    /// 여기서 수행 (state/ 레이어가 plugin/ 의존을 갖지 않게).
    ///
    /// Event Bus 1.0 `surface.closed`로 broadcast. (PR 4에서 옛 `surface.lifecycle`
    /// IPC 폐기. plugin은 `event_subscribe = ["surface.closed"]`로 구독한다.)
    pub(crate) fn dispatch_pending_surface_lifecycle(&mut self) {
        use tasty_plugin_protocol::EventScope;
        use tasty_plugin_protocol::events::LifecycleReason;
        use tasty_plugin_protocol::events::payloads::SurfaceClosed;
        let mut drained: Vec<crate::state::PendingSurfaceClosed> = Vec::new();
        for w in self.view.views.values_mut() {
            if let Some(main) = w.as_main_mut() {
                drained.extend(main.state.take_pending_lifecycle_events());
            }
        }
        for (s, _engine) in &mut self.parked_states {
            drained.extend(s.take_pending_lifecycle_events());
        }
        if drained.is_empty() {
            return;
        }
        let scripts = self.autofire_scripts();
        let lua = self.lua_engine.as_ref();
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        for ev in drained {
            // plugin surface 면 소유 plugin 에 surface.destroy 통지 + host 측
            // RemoteSurfaceEntry/mesh frame 정리 (아니면 no-op). 이 호출이 빠지면
            // plugin 프로세스의 per-surface 상태가 영원히 남는다 (soak S6 실측).
            mgr.destroy_remote_surface(ev.surface_id, ev.kind);
            let bus_reason = if ev.is_user_close {
                LifecycleReason::User
            } else {
                LifecycleReason::Ipc
            };
            let payload = SurfaceClosed {
                surface_id: ev.surface_id,
                kind: ev.kind.map(|s| s.to_string()).unwrap_or_default(),
                reason: bus_reason,
            };
            mgr.emit_host_event("surface.closed", &payload, EventScope::Surface);
            crate::hooks::lua::fire(
                lua,
                crate::hooks::lua::AutofireCtx {
                    scripts: &scripts,
                    guard: &mut self.lua_autofire,
                },
                "surface.delete.post",
                &payload,
            );
        }
    }
}
