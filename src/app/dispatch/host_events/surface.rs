//! surface 상태 변경 이벤트를 전달한다. 닫기는 dispatch_pending_surface_lifecycle에서 처리한다.

use tasty_plugin_protocol::EventScope;
use tasty_plugin_protocol::events::payloads::{
    SurfaceCreated, SurfaceCreatedBy, SurfaceFocused, SurfaceRefreshRequested, SurfaceTitleChanged,
};

use crate::hooks::lua::AutofireCtx;
use crate::plugin::PluginManager;

pub(super) fn emit_focused(mgr: &mut PluginManager, surface_id: u32, prev_surface_id: Option<u32>) {
    let payload = SurfaceFocused {
        surface_id,
        prev_surface_id,
    };
    mgr.emit_host_event("surface.focused", &payload, EventScope::Surface);
}

pub(super) fn emit_title_changed(mgr: &mut PluginManager, surface_id: u32, title: String) {
    let payload = SurfaceTitleChanged { surface_id, title };
    mgr.emit_host_event("surface.title_changed", &payload, EventScope::Surface);
}

pub(super) fn emit_refresh_requested(
    mgr: &mut PluginManager,
    surface_id: u32,
    kind: String,
    trigger: &'static str,
) {
    let payload = SurfaceRefreshRequested {
        surface_id,
        kind,
        trigger: trigger.to_string(),
    };
    mgr.emit_host_event("surface.refresh_requested", &payload, EventScope::Surface);
}

#[allow(clippy::too_many_arguments)] // reason: host event payload 전체 컨텍스트
pub(super) fn emit_created(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    surface_id: u32,
    kind: &'static str,
    tab_id: u32,
    pane_id: u32,
    workspace_id: u32,
    created_by_plugin: Option<String>,
) {
    let created_by = match created_by_plugin {
        Some(pid) => SurfaceCreatedBy::Agent { source_plugin: pid },
        None => SurfaceCreatedBy::User,
    };
    let payload = SurfaceCreated {
        surface_id,
        kind: kind.to_string(),
        tab_id,
        pane_id,
        workspace_id,
        created_by,
    };
    mgr.emit_host_event("surface.created", &payload, EventScope::Surface);
    crate::hooks::lua::fire(lua, autofire, "surface.create.post", &payload);
}
