mod copy_paste;
mod dispatch;
mod double_tap;
mod explorer_keys;
mod keybinding;
/// modifier-hint에 표시할 조합 모델.
pub(crate) mod modifier_hint;
mod numeric;
#[cfg(test)]
mod refresh_tests;
#[cfg(test)]
mod tests;
mod webview_claims;
mod webview_edit;
mod window_control;
mod zoom;

pub(crate) use webview_claims::webview_shortcut_policy;
pub(crate) use webview_edit::{edit_binding_after_clipboard, edit_binding_for_key};

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

/// `close_pane` 액션. 활성 pane 을 닫고, 닫지 못했으면 활성 workspace 를 닫는다.
/// 생성 대기 초안이 선택돼 있으면 닫을 대상이 없으므로 workspace 로 넘어가지 않고 로그만 남긴다.
pub(crate) fn close_pane_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if ignores_close_for_draft(state, engine, "close_pane") {
        return;
    }
    if !state.close_active_pane(engine) {
        state.close_active_workspace(engine);
    }
}

/// `close_active` 액션. 활성 탭, pane, workspace 순서로 닫을 수 있는 첫 대상을 닫는다.
/// 생성 대기 초안이 선택돼 있으면 `close_pane` 과 같이 아무것도 닫지 않는다.
pub(crate) fn close_active_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if ignores_close_for_draft(state, engine, "close_active") {
        return;
    }
    if !state.close_active_tab(engine) && !state.close_active_pane(engine) {
        state.close_active_workspace(engine);
    }
}

fn ignores_close_for_draft(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: &str,
) -> bool {
    let draft = state.attach_pending.selected(engine, &state.navigation);
    if let Some(entry) = draft {
        tracing::info!(
            "{action} ignored: pending attach draft {} is selected",
            entry.id.0
        );
    }
    draft.is_some()
}

fn focused_explorer_panel<'a>(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'a>,
) -> Option<&'a crate::model::ExplorerPanel> {
    let pane = state.focused_pane(engine)?;
    let tab = pane.tabs.get(state.navigation.tab_index(pane))?;
    let focused = state.navigation.surface_id(tab).unwrap_or(0);
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

/// 미리보기 토글·Properties 단축키. 포커스된 탐색기만 바꾼다.
fn explorer_view_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: &str,
) {
    let Some(panel) = focused_explorer_panel(state, engine) else {
        return;
    };
    let (sid, cwd) = (panel.id, panel.current_root().to_path_buf());
    if action == "explorer_toggle_preview" {
        if let Some(view) = state.explorer_views.get_mut(sid) {
            view.preview.toggle();
        }
        return;
    }
    if action == "explorer_toggle_hidden" {
        if let Some(view) = state.explorer_views.get_mut(sid) {
            view.toggle_hidden();
        }
        return;
    }
    crate::adapters::ui::popup::explorer_properties::open_for_shortcut(
        state,
        engine.is_mirror_surface(sid),
        sid,
        &cwd,
    );
}

/// 뒤로·앞으로·주소창 포커스 단축키. 포커스된 탐색기만 바꾼다. 주소창 포커스는 다음에 주소창을
/// 그릴 때 준다.
fn explorer_nav_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: &str,
) {
    let Some(sid) = focused_explorer_surface_id(state, engine) else {
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

/// 주소창 포커스 단축키를 포커스된 html surface의 주소 띠에 준다. 처리했으면 참이다.
/// 띠를 숨기는 mirror workspace의 html leaf는 처리하지 않는다. 포커스는 다음에 띠를 그릴 때 준다.
fn html_focus_address_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    let Some(sid) = state.focused_pane(engine).and_then(|pane| {
        let tab = pane.tabs.get(state.navigation.tab_index(pane))?;
        state.navigation.surface_id(tab)
    }) else {
        return false;
    };
    let html = engine
        .find_surface_by_id(sid)
        .and_then(|s| s.remote_webview())
        .is_some_and(|w| w.capabilities().address_band);
    if !html || engine.is_mirror_surface(sid) {
        return false;
    }
    state.html_address.request_focus(sid);
    true
}

/// 되돌리기·다시 실행 단축키. 포커스된 탐색기의 이력만 쓴다. mirror explorer 는 파일 작업을
/// 하지 않아 이력이 비어 있다.
fn explorer_history_shortcut(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    action: &str,
) {
    let Some(sid) = focused_explorer_panel(state, engine).map(|p| p.id) else {
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

/// 포커스된 surface 에 공통 새로고침 요청을 보낸다. 탐색기 이름·주소 입력 중이면 입력이 우선이라
/// 처리하지 않는다. 대상 surface 가 없으면 false 를 돌려 키를 다른 경로에 남긴다.
pub(super) fn request_focused_surface_refresh(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    trigger: crate::intent::RefreshTrigger,
) -> bool {
    if focused_explorer_typing(state, engine) {
        return false;
    }
    let Some(sid) = state.focused_surface_id(engine) else {
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

/// 키보드 붙여넣기는 포커스된 탐색기의 현재 폴더를 대상으로 한다.
fn focused_explorer_cwd(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<std::path::PathBuf> {
    focused_explorer_panel(state, engine).map(|p| p.current_root().to_path_buf())
}
