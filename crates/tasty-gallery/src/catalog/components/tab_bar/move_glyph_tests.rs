//! 탭 칸 상태 묶음 표본에서 move 글리프가 활성 탭에 그려지지 않는지 그린 도형으로 잰다.
//! 앱은 이동 원본 서피스가 활성 탭 안에 있으면 탭 칸에 move 글리프를 두지 않는다.

use crate::catalog::components::painted::{mocha_latte, paint};

/// 상태 묶음 표본(좁은 탭 포함)의 move 글리프는 활성 탭 칸 밖에 있다.
#[test]
fn the_status_cluster_samples_draw_no_move_glyph_on_an_active_tab() {
    // 표본은 받은 테마와 관계없이 Mocha·Latte를 함께 그린다.
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::draw_status_cluster(ui, base));
    for (name, th) in mocha_latte() {
        let active = painted.active_tab_cells(&th);
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}
