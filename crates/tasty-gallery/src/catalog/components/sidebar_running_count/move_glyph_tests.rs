//! 가장 좁은 행 표본에서 move 글리프가 선택 행에 그려지지 않는지 그린 도형으로 잰다.
//! 앱은 이동 원본이 활성 워크스페이스에 있으면 행에 move 글리프를 두지 않는다.

use tasty_ui_widgets::RowState;

use crate::catalog::components::painted::{mocha_latte, paint};

/// 쉬는 행은 move 글리프를 그리고, 선택 행(surface-active 채움)은 그리지 않는다.
#[test]
fn the_narrowest_selected_row_draws_no_move_glyph() {
    for (name, th) in mocha_latte() {
        let painted = paint(|ui| super::draw_narrowest(ui, &th));
        let selected = painted.rects_filled_all(RowState::Selected.fill(&th));
        painted.assert_move_glyphs_outside(&th, &selected, name);
    }
}
