//! Sidebar 의 tools 버튼 핸들러 — popup 열기.

use crate::state::MainViewState;

/// 버튼 왼쪽에 맞춰 위로 연다. 위치는 열 때 정해지므로 현재 플러그인 항목까지 포함한 메뉴 크기를 받는다.
pub(crate) fn open_tools_menu(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    btn_rect: egui::Rect,
) {
    let payload = crate::adapters::ui::tools_menu::measure_on_open(ctx, state);
    let menu_size =
        crate::adapters::ui::tools_menu::tools_menu_current_size(state, engine, &payload);
    let pos = egui::pos2(btn_rect.min.x, btn_rect.min.y - menu_size.y);
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup(
            crate::intent::OpenRequest::new(
                "tools_menu",
                crate::model::popup_kind::PopupScope::Window,
            )
            .at(pos)
            .focused()
            .with_payload(payload),
        )
        .from_user_menu("tools_button"),
    );
}
