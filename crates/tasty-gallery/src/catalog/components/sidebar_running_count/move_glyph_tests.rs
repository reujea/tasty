//! 가장 좁은 행 표본에서 move 글리프가 선택 행에 그려지지 않는지, 거절 표지가 그려지지 않는지
//! 그린 도형으로 잰다. 앱은 이동 원본이 활성 워크스페이스에 있으면 행에 move 글리프를 두지 않고,
//! 거절 표지는 실행 수를 보이지 않는 연결 안 된 저장 원격 행에만 둔다.

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

/// 실행 수를 단 좁은 행에는 거절 표지(attach-refusal-glyph 잉크)가 없다. 같은 잉크 판독이 표지를
/// 찾는지는 표지 하나만 그린 대조로 본다.
#[test]
fn the_narrowest_rows_draw_no_refusal_mark() {
    for (name, th) in mocha_latte() {
        let tint = th.attach_refusal_glyph().to_egui();
        let mark = paint(|ui| {
            tasty_ui_widgets::attach_refusal_mark(ui, &th, "");
        });
        assert!(
            mark.glyphs.iter().any(|(_, c)| *c == tint),
            "{name}: the refusal mark control painted no glyph in its ink"
        );
        let painted = paint(|ui| super::draw_narrowest(ui, &th));
        assert!(
            !painted.move_glyphs(&th).is_empty(),
            "{name}: the narrowest rows painted no move glyph"
        );
        let marks = painted.glyphs.iter().filter(|(_, c)| *c == tint).count();
        assert_eq!(marks, 0, "{name}: refusal marks on the narrowest rows");
    }
}
