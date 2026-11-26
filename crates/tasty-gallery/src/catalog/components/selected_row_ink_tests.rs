//! 탐색기 사이드바 표본의 선택 행 글리프가 본체처럼 on-active 잉크로 칠해지는지 그린 도형에서 잰다.

use super::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;

fn themes() -> [(&'static str, Theme); 2] {
    [
        (
            "mocha",
            Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0),
        ),
        ("latte", crate::host_shell::latte_theme()),
    ]
}

/// 글자가 정확히 `label` 인 행마다, 그 글자 앞(왼쪽, 같은 줄 높이)에서 가장 가까운 글리프의 tint.
fn glyphs_before(painted: &Painted, label: &str) -> Vec<egui::Color32> {
    let rows: Vec<_> = painted
        .texts
        .iter()
        .filter(|t| t.text == label)
        .map(|t| t.rect)
        .collect();
    assert!(!rows.is_empty(), "{label:?} 행이 그려지지 않았다");
    rows.iter()
        .map(|line| {
            painted
                .glyphs
                .iter()
                .filter(|(r, _)| {
                    r.right() <= line.left()
                        && r.center().y > line.top()
                        && r.center().y < line.bottom()
                })
                .max_by(|a, b| a.0.right().total_cmp(&b.0.right()))
                .map(|(_, tint)| *tint)
                .unwrap_or_else(|| panic!("{label:?} 앞 글리프가 없다"))
        })
        .collect()
}

/// 트리 폴더 글리프는 선택 행(Downloads)에서 text_on_active, 쉬는 행(Home)에서 tree_row 가 넘기는 잉크다.
#[test]
fn the_explorer_sample_folder_glyph_takes_text_on_active_on_the_selected_row() {
    for (name, th) in themes() {
        let painted = paint(|ui| super::explorer_sidebar::draw_favorites(ui, &th));
        for tint in glyphs_before(&painted, "Downloads") {
            assert_eq!(
                tint,
                th.text_on_active().to_egui(),
                "{name}: 선택 행 폴더 글리프 잉크"
            );
        }
        for tint in glyphs_before(&painted, "Home") {
            assert_eq!(
                tint,
                tasty_ui_widgets::RowState::Rest.glyph(&th).to_egui(),
                "{name}: 쉬는 행 폴더 글리프 잉크"
            );
        }
    }
}
