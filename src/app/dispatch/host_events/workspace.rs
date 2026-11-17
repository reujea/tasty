//! workspace 상태·구조 변경 이벤트를 전달한다.

use tasty_plugin_protocol::EventScope;
use tasty_plugin_protocol::LifecycleReason;
use tasty_plugin_protocol::events::payloads::{
    WorkspaceActivated, WorkspaceClosed, WorkspaceCreated, WorkspaceRenamed,
};

use winit::window::WindowId;

use crate::core::CoreState;
use crate::hooks::lua::AutofireCtx;
use crate::plugin::PluginManager;

/// workspace.created payload의 window_id. [`created_window`]로만 만들 수 있어
/// 발행 경로가 발행 시점 조회를 건너뛰고 값을 넣을 수 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CreatedWindow(u64);

#[cfg(test)]
impl CreatedWindow {
    pub(super) fn id(self) -> u64 {
        self.0
    }
}

/// 발행 시점에 workspace를 가진 창. 창이 없는 engine(parked)에 있거나 이미 닫혔으면 0이다.
/// windows는 `App::find_main_with_workspace`가 보는 것과 같은 창 engine 목록이다.
pub(super) fn created_window<'a>(
    windows: impl IntoIterator<Item = (WindowId, &'a CoreState)>,
    workspace_id: u32,
) -> CreatedWindow {
    let owner = windows
        .into_iter()
        .find(|(_, engine)| engine.has_workspace(workspace_id))
        .map(|(wid, _)| u64::from(wid));
    CreatedWindow(owner.unwrap_or(0))
}

pub(super) fn emit_activated(
    mgr: &mut PluginManager,
    workspace_id: u32,
    prev_workspace_id: Option<u32>,
) {
    let payload = WorkspaceActivated {
        workspace_id,
        prev_workspace_id,
    };
    mgr.emit_host_event("workspace.activated", &payload, EventScope::System);
}

pub(super) struct RenameEvent {
    pub workspace_id: u32,
    pub name: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    /// GUI에서 직접 이름을 바꾼 경우다. IPC 변경은 포함하지 않는다.
    pub user_direct: bool,
}

pub(super) fn emit_renamed(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    ev: RenameEvent,
) {
    let RenameEvent {
        workspace_id,
        name,
        subtitle,
        description,
        user_direct,
    } = ev;
    let payload = WorkspaceRenamed {
        workspace_id,
        name,
        subtitle,
        description,
    };
    mgr.emit_host_event("workspace.renamed", &payload, EventScope::System);
    if user_direct {
        crate::hooks::lua::fire(lua, autofire, "workspace.change.post", &payload);
    }
}

pub(super) fn emit_created(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    workspace_id: u32,
    window: CreatedWindow,
    name: String,
) {
    let payload = WorkspaceCreated {
        workspace_id,
        window_id: window.0,
        name,
    };
    mgr.emit_host_event("workspace.created", &payload, EventScope::System);
    crate::hooks::lua::fire(lua, autofire, "workspace.create.post", &payload);
}

pub(super) fn emit_closed(
    mgr: &mut PluginManager,
    lua: Option<&crate::hooks::lua::LuaSend<'_>>,
    autofire: AutofireCtx<'_>,
    workspace_id: u32,
) {
    let payload = WorkspaceClosed {
        workspace_id,
        reason: LifecycleReason::User,
    };
    mgr.emit_host_event("workspace.closed", &payload, EventScope::System);
    crate::hooks::lua::fire(lua, autofire, "workspace.delete.post", &payload);
}
