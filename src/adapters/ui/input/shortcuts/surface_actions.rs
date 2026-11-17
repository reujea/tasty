//! surface 하나를 대상으로 하는 내장 액션. 대상 surface 는 호출자가 입력 시점에 정해 넘기며 여기서
//! 포커스를 다시 읽지 않는다.

use crate::intent::{OpenPopupMode, UiIntent};
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;

/// `sid` 의 검색을 연다. 탐색기는 자기 Find 바, 터미널은 검색창이다. 다른 kind 는 처리하지 않고 false 다.
pub(crate) fn find_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
    source: &'static str,
) -> bool {
    match engine.find_surface_by_id(sid).map(|s| s.kind()) {
        Some("explorer") => state.open_explorer_find(sid),
        Some("terminal") => {
            crate::adapters::ui::search_bar::open_or_focus_for(state, Some(sid), source)
        }
        _ => return false,
    }
    true
}

/// `sid` 를 변환할 kind 를 고르는 popup 을 연다.
pub(crate) fn open_convert_popup(state: &mut MainViewState, sid: u32, source: &'static str) {
    state.dialogs.convert_popup = Some(sid);
    state.dialogs.convert_popup_selected = None;
    state.dispatch_intent(
        UiIntent::OpenPopup {
            id: "convert_surface",
            mode: OpenPopupMode::CenteredFocused(crate::adapters::ui::popup::PopupScope::Surface(
                sid,
            )),
        }
        .from_user_shortcut(source),
    );
}

/// `sid` 를 탐색기로 바꾸도록 요청한다.
pub(crate) fn convert_to_explorer(state: &mut MainViewState, sid: u32, source: &'static str) {
    state.dispatch_intent(
        crate::intent::Intent::ConvertSurface {
            surface_id: sid,
            target: crate::intent::ConvertTarget::Kind {
                cwd: None,
                kind: "explorer".to_string(),
                params: serde_json::json!({}),
            },
        }
        .from_user_shortcut(source),
    );
}

/// 탐색기 `sid` 의 미리보기·숨김 파일 토글과 Properties. `sid` 가 탐색기가 아니면 아무것도 하지 않는다.
pub(crate) fn explorer_view_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    sid: u32,
    action: &str,
) {
    let Some(panel) = engine.find_surface_by_id(sid).and_then(|s| s.explorer()) else {
        return;
    };
    let cwd = panel.current_root().to_path_buf();
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
