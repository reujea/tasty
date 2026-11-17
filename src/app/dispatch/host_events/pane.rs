//! pane 생성·닫기·분할 이벤트를 전달한다.

use tasty_plugin_protocol::EventScope;
use tasty_plugin_protocol::LifecycleReason;
use tasty_plugin_protocol::events::payloads::{PaneClosed, PaneCreated, PaneSplit, SplitDirection};

use crate::hooks::lua::AutofireCtx;
use crate::plugin::PluginManager;

pub(super) fn emit_created(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    pane_id: u32,
    workspace_id: u32,
) {
    let payload = PaneCreated {
        pane_id,
        parent_pane_group: None,
        workspace_id,
    };
    mgr.emit_host_event("pane.created", &payload, EventScope::System);
    crate::hooks::lua::fire(lua, autofire, "pane.create.post", &payload);
}

pub(super) fn emit_closed(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    pane_id: u32,
) {
    let payload = PaneClosed {
        pane_id,
        reason: LifecycleReason::User,
    };
    mgr.emit_host_event("pane.closed", &payload, EventScope::System);
    crate::hooks::lua::fire(lua, autofire, "pane.delete.post", &payload);
}

pub(super) fn emit_split(
    mgr: &mut PluginManager,
    original_pane: u32,
    new_pane: u32,
    direction: crate::model::SplitDirection,
) {
    let direction = match direction {
        crate::model::SplitDirection::Horizontal => SplitDirection::Horizontal,
        crate::model::SplitDirection::Vertical => SplitDirection::Vertical,
    };
    let payload = PaneSplit {
        original_pane,
        new_pane,
        direction,
    };
    mgr.emit_host_event("pane.split", &payload, EventScope::System);
}
