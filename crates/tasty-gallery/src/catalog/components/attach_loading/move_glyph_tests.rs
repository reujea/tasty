//! 로딩 표본에서 move 글리프가 활성 칸에 그려지지 않는지 그린 도형으로 잰다.
//! 앱은 이동 원본이 활성 탭·활성 워크스페이스에 있으면 그 칸에 move 글리프를 두지 않는다.

use crate::catalog::components::painted::{mocha_latte, paint};

/// 탭 표본(좁은 탭 포함)의 move 글리프는 활성 탭 칸 밖에 있다.
#[test]
fn the_loading_tab_samples_draw_no_move_glyph_on_an_active_tab() {
    // 표본은 받은 테마와 관계없이 Mocha·Latte를 함께 그린다.
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::tabs(ui, base));
    for (name, th) in mocha_latte() {
        let active = painted.active_tab_cells(&th);
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}

/// 레일 아바타 표본(네 모서리 표본 포함)의 move 칩 글리프는 활성 아바타(overlay-active 채움) 밖에 있다.
#[test]
fn the_loading_rail_samples_draw_no_move_chip_on_an_active_avatar() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let active = painted.rects_filled_all(th.overlay_active().to_egui_premultiplied());
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}
