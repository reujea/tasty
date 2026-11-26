//! 로딩 표본에서 move 글리프가 활성 칸에 그려지지 않는지 그린 도형으로 잰다.
//! 앱은 이동 원본이 활성 탭·활성 워크스페이스에 있으면 그 칸에 move 글리프를 두지 않는다.

use crate::catalog::components::painted::{Painted, mocha_latte, paint};
use tasty_type_appearance::theme::Theme;

/// 활성 탭 칸: 위쪽 표시 막대(tab-indicator)에서 탭 높이만큼 내려온 사각형.
fn active_tabs(painted: &Painted, th: &Theme) -> Vec<egui::Rect> {
    let bar_h = th.tab_indicator_width().value();
    painted
        .rects_filled_all(th.tab_indicator().into())
        .into_iter()
        .filter(|bar| (bar.height() - bar_h).abs() < 0.01)
        .map(|bar| {
            egui::Rect::from_min_max(
                bar.min,
                egui::pos2(bar.max.x, bar.min.y + th.tab_height().value()),
            )
        })
        .collect()
}

/// 탭 표본(좁은 탭 포함)의 move 글리프는 활성 탭 칸 밖에 있다.
#[test]
fn the_loading_tab_samples_draw_no_move_glyph_on_an_active_tab() {
    // 표본은 받은 테마와 관계없이 Mocha·Latte를 함께 그린다.
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::tabs(ui, base));
    for (name, th) in mocha_latte() {
        let active = active_tabs(&painted, &th);
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}
