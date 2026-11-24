#[cfg(test)]
mod command_target_tests;
mod copy_paste;
mod dispatch;
pub(crate) mod double_tap;
mod explorer_keys;
mod keybinding;
/// modifier-hint에 표시할 조합 모델.
pub(crate) mod modifier_hint;
mod numeric;
#[cfg(test)]
mod refresh_tests;
pub(crate) mod structure;
#[cfg(test)]
mod structure_draft_tests;
#[cfg(test)]
mod structure_tests;
pub(crate) mod surface_actions;
mod surface_view_actions;
#[cfg(test)]
mod tests;
mod webview_claims;
mod webview_edit;
mod window_control;
mod zoom;

pub(crate) use explorer_keys::{
    egui_dragging, explorer_cancels_address_check, explorer_takes_clear_selection,
};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use keybinding::CellGeometry;
pub(crate) use webview_claims::webview_shortcut_policy;
pub(crate) use webview_edit::{
    edit_binding_after_clipboard, edit_binding_for_key, webview_edit_for_surface_key,
};
pub(crate) use zoom::run_held_zoom;

use crate::state::command_target::CommandTarget;
pub(crate) use tasty_key_match::{
    any_binding_pressed_egui, consume_binding_egui, matches_any_binding, physical_key_to_logical,
    shortcut_lookup_key, super_held,
};
use winit::event_loop::EventLoopProxy;

/// 이벤트 루프 종료 뒤 입력이 도착할 수 있으므로 전송 실패는 trace로 남긴다.
pub(crate) fn send_app_event(proxy: &EventLoopProxy<crate::AppEvent>, event: crate::AppEvent) {
    if let Err(e) = proxy.send_event(event) {
        tracing::trace!("AppEvent send dropped (event loop closing): {e}");
    }
}

/// 새 workspace는 활성 대상의 카테고리를 상속한다. 빈 engine이면 None으로 normal을 사용한다.
pub(crate) fn focused_workspace_category(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<crate::model::WorkspaceCategoryId> {
    if engine.workspaces().is_empty() {
        return None;
    }
    Some(state.active_workspace(engine).category)
}

/// `close_pane` 액션. 대상 pane 을 닫고, 닫지 못했으면 대상 workspace 를 닫는다.
/// 입력 때 생성 대기 초안이 선택돼 있었으면 닫을 대상이 없으므로 workspace 로 넘어가지 않고 로그만 남긴다.
pub(crate) fn close_pane_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) {
    if ignores_close_for_draft(target, "close_pane") {
        return;
    }
    if !state.close_pane_in(engine, target) {
        close_target_workspace(state, engine, target);
    }
}

/// `new_workspace` 액션. 명령 대상 workspace 의 카테고리에 새 workspace 를 만든다. 대상 workspace 가 없던
/// 입력(workspace 가 하나도 없을 때)은 카테고리 없이 만든다. 입력 때 정한 workspace 가 사라졌으면
/// [`command_target_gone`] 이 먼저 버린다.
pub(crate) fn new_workspace_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) {
    let category = target.find_workspace(engine).map(|ws| ws.category);
    state.dispatch_intent(
        crate::intent::Intent::NewWorkspace {
            kind: None,
            params: serde_json::Value::Null,
            category,
            cwd_source: target.cwd_source(),
        }
        .from_user_shortcut("new_workspace"),
    );
}

/// `close_active` 액션. 대상 탭, pane, workspace 순서로 닫을 수 있는 첫 대상을 닫는다.
/// 입력 때 생성 대기 초안이 선택돼 있었으면 `close_pane` 과 같이 아무것도 닫지 않는다.
pub(crate) fn close_active_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) {
    if ignores_close_for_draft(target, "close_active") {
        return;
    }
    if !state.close_tab_in(engine, target) && !state.close_pane_in(engine, target) {
        close_target_workspace(state, engine, target);
    }
}

/// `rename_tab` 액션. 명령 대상 탭의 이름 바꾸기 팝업을 연다. 키와 팔레트가 함께 쓴다.
pub(crate) fn open_tab_rename(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) {
    if let Some((rename, current_name)) = state.tab_rename_in(engine, target) {
        crate::adapters::ui::rename_popup::open(state, engine, rename, current_name, |intent| {
            intent.from_user_shortcut("rename_tab")
        });
    }
}

/// `close_surface` 액션. 대상 surface, pane, workspace 순서로 닫을 수 있는 첫 대상을 닫는다.
pub(crate) fn close_surface_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) {
    if !state.close_surface_in(engine, target) && !state.close_pane_in(engine, target) {
        close_target_workspace(state, engine, target);
    }
}

/// 대상 workspace 를 사용자 닫기로 닫는다.
pub(crate) fn close_target_workspace(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) -> bool {
    target.workspace_index(engine).is_some_and(|index| {
        state.close_workspace_at(engine, index, crate::state::WorkspaceCloseOrigin::User)
    })
}

/// `action_id` 가 쓰는 입력 시점 대상이 지금 없다. 대상이 없던 명령(`None`)이나 대상 단위가 없는 창 명령은
/// 해당하지 않는다. 탭·surface 단위 명령의 대상이 초안이었으면 그 초안이 남아 있어야 한다. 사라졌으면 info
/// 로그를 남긴다.
pub(crate) fn command_target_gone(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action_id: &str,
    target: &CommandTarget,
) -> bool {
    use crate::state::attach_pending::deferred::CommandSurfaceTarget;
    let draft_gone = target
        .draft
        .is_some_and(|draft| !state.attach_pending.draft_live(engine, draft));
    let gone = match action_id {
        "new_workspace"
        | "close_workspace"
        | "rename_workspace"
        | "rename_workspace_subtitle"
        | "toggle_dag_list"
        | "open_file_picker" => {
            target.workspace.is_some() && target.find_workspace(engine).is_none()
        }
        "new_tab" | "close_pane" | "focus_pane_next" | "focus_pane_prev" | "apply_pane_preset" => {
            target.pane.is_some() && target.find_pane(engine).is_none()
        }
        "close_active" | "rename_tab" | "next_tab" | "prev_tab" | "focus_surface_next"
        | "focus_surface_prev" | "apply_tab_preset" => {
            draft_gone || (target.tab.is_some() && target.find_tab(engine).is_none())
        }
        "close_surface"
        | "screenshot_to_clipboard"
        | "zoom_in"
        | "zoom_out"
        | "zoom_reset"
        | "copy"
        | "cut"
        | "paste"
        | "open_markdown" => {
            draft_gone
                || matches!(target.surface, Some(CommandSurfaceTarget::Surface(sid)) if engine.find_surface_by_id(sid).is_none())
        }
        _ => false,
    };
    if gone {
        tracing::info!("action '{action_id}' dropped: its target {target:?} is gone");
    }
    gone
}

fn ignores_close_for_draft(target: &CommandTarget, action: &str) -> bool {
    if let Some(draft) = target.draft {
        tracing::info!(
            "{action} ignored: pending attach draft {} is selected",
            draft.0
        );
    }
    target.draft.is_some()
}

/// 대상 surface 가 탐색기면 그 id.
fn target_explorer_surface_id(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) -> Option<u32> {
    let sid = target.surface_id()?;
    engine.find_surface_by_id(sid)?.explorer().map(|p| p.id)
}

/// 포커스 surface 의 탐색기. 생성 대기 초안이 선택돼 있으면 포커스 surface 가 없으므로 없다.
fn focused_explorer_panel<'a>(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'a>,
) -> Option<&'a crate::model::ExplorerPanel> {
    let focused = state.focused_surface_id(engine)?;
    engine.find_surface_by_id(focused)?.explorer()
}

pub(crate) fn focused_explorer_surface_id(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<u32> {
    focused_explorer_panel(state, engine).map(|p| p.id)
}

#[cfg(target_os = "linux")]
pub(crate) fn explorer_composition_list_key(
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
) -> bool {
    explorer_keys::list_step(kb, key, mods).is_some()
        || [
            &kb.explorer_open,
            &kb.explorer_clear_selection,
            &kb.explorer_trash,
            &kb.explorer_rename,
        ]
        .into_iter()
        .any(|bindings| matches_any_binding(bindings, key, mods))
}

/// 미리보기 토글·Properties 단축키. 대상 탐색기만 바꾼다.
fn explorer_view_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
    action: &str,
) {
    if let Some(sid) = target_explorer_surface_id(engine, target) {
        surface_actions::explorer_view_on(state, engine, sid, action);
    }
}

/// 뒤로·앞으로·주소창 포커스 단축키. 대상 탐색기만 바꾼다. 주소창 포커스는 다음에 주소창을
/// 그릴 때 준다.
fn explorer_nav_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
    action: &str,
) {
    let Some(sid) = target_explorer_surface_id(engine, target) else {
        return;
    };
    let nav = match action {
        "explorer_back" => crate::explorer_ui::ExplorerAction::GoBack,
        "explorer_forward" => crate::explorer_ui::ExplorerAction::GoForward,
        _ => {
            if let Some(view) = state.explorer_views.get_mut(sid) {
                view.focus_address = true;
            }
            return;
        }
    };
    crate::adapters::ui::egui_panels::apply_explorer_action(state, engine, sid, nav);
}

/// 주소창 포커스 단축키를 대상 html surface의 주소 띠에 준다. 처리했으면 참이다.
fn html_focus_address_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
) -> bool {
    target
        .surface_id()
        .is_some_and(|sid| surface_actions::html_focus_address_on(state, engine, sid))
}

/// 되돌리기·다시 실행 단축키. 대상 탐색기의 이력만 쓴다. mirror explorer 는 파일 작업을
/// 하지 않아 이력이 비어 있다.
fn explorer_history_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
    action: &str,
) {
    let Some(sid) = target_explorer_surface_id(engine, target) else {
        return;
    };
    let (redo, id) = if action == "explorer_redo" {
        (true, "explorer_redo")
    } else {
        (false, "explorer_undo")
    };
    let origin = crate::intent::IntentOrigin::User {
        source: crate::intent::UserSource::Shortcut(id),
    };
    state.explorer_history_step(engine, sid, redo, origin, false);
}

/// 포커스된 탐색기의 주소창이나 이름 입력이 키를 받고 있다. 그동안 목록 단축키는 글자 편집에 양보한다.
pub(crate) fn focused_explorer_typing(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    focused_explorer_surface_id(state, engine)
        .and_then(|sid| state.explorer_views.get(sid))
        .is_some_and(|v| v.text_input_active())
}

/// 대상 surface 에 공통 새로고침 요청을 보낸다. 탐색기 이름·주소 입력 중이면 입력이 우선이라
/// 처리하지 않는다. 대상 surface 가 없으면 false 를 돌려 키를 다른 경로에 남긴다.
pub(super) fn request_target_surface_refresh(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &CommandTarget,
    trigger: crate::intent::RefreshTrigger,
) -> bool {
    if focused_explorer_typing(state, engine) {
        return false;
    }
    let Some(sid) = target.surface_id() else {
        return false;
    };
    request_surface_refresh(state, engine, sid, trigger)
}

/// `sid` 에 공통 새로고침 요청을 보낸다. 대상은 지금의 세대·kind 로 고정해 Intent 로 넘긴다.
/// 탐색기는 host 가 처리자라 여기서 한 번 새로고침한다. surface 가 없으면 false 다.
pub(crate) fn request_surface_refresh(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    sid: u32,
    trigger: crate::intent::RefreshTrigger,
) -> bool {
    let Some(kind) = engine
        .core
        .find_surface_by_id(sid)
        .map(|s| s.kind().to_string())
    else {
        return false;
    };
    let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid) else {
        return false;
    };
    if kind == "explorer" {
        crate::adapters::ui::egui_panels::apply_explorer_action(
            state,
            engine,
            sid,
            crate::explorer_ui::ExplorerAction::Refresh,
        );
    }
    state.dispatch_intent(
        crate::intent::UiIntent::RequestSurfaceRefresh {
            target,
            kind,
            trigger,
        }
        .from_user_shortcut("refresh"),
    );
    true
}
