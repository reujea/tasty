//! 포트·탐색기 표본의 빈 즐겨찾기 별이 본체처럼 투명도 배수 없이 glyph_dim 으로 칠해지는지 그린 도형에서 잰다.

use super::painted::{Painted, paint};
use tasty_type_appearance::theme::Theme;

fn mocha() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// `label` 글자 줄 앞(왼쪽, 같은 줄 높이)의 별 글리프 tint.
fn star_before(painted: &Painted, th: &Theme, label: &str) -> egui::Color32 {
    let line = painted.text(label).rect;
    let sz = th.icon_glyph_size_sm.value();
    painted
        .glyphs
        .iter()
        .find(|(r, _)| {
            (r.width() - sz).abs() < 0.5
                && r.right() <= line.left()
                && r.center().y > line.top()
                && r.center().y < line.bottom()
        })
        .map(|(_, tint)| *tint)
        .unwrap_or_else(|| panic!("{label:?} 앞 별 글리프가 없다"))
}

/// 포트 표본의 빈 즐겨찾기 별은 glyph_dim 이다.
#[test]
fn the_ports_sample_empty_favorites_star_is_glyph_dim() {
    for (name, th) in [
        ("mocha", mocha()),
        ("latte", crate::host_shell::latte_theme()),
    ] {
        let painted = paint(|ui| super::port_scanner::draw(ui, &th));
        let tint = star_before(
            &painted,
            &th,
            crate::i18n::t("port_scanner.favorites_empty"),
        );
        assert_eq!(
            tint,
            th.glyph_dim().to_egui(),
            "{name}: 포트 표본 빈 즐겨찾기 별 tint 가 glyph_dim 이 아니다"
        );
    }
}

/// 탐색기 표본의 빈 즐겨찾기 별은 glyph_dim 이다.
#[test]
fn the_explorer_sample_empty_favorites_star_is_glyph_dim() {
    for (name, th) in [
        ("mocha", mocha()),
        ("latte", crate::host_shell::latte_theme()),
    ] {
        let painted = paint(|ui| super::explorer_sidebar::draw_favorites(ui, &th));
        let tint = star_before(
            &painted,
            &th,
            crate::i18n::t_lang("en", "explorer.sidebar.favorites_empty"),
        );
        assert_eq!(
            tint,
            th.glyph_dim().to_egui(),
            "{name}: 탐색기 표본 빈 즐겨찾기 별 tint 가 glyph_dim 이 아니다"
        );
    }
}

/// 탐색기 표본의 빈 즐겨찾기 안내 줄은 메뉴 이름을 본체 메뉴 줄과 같은 내장 영어 lang 값으로 text_muted 로 그리고,
/// Meta hint 행은 정본처럼 사이드바의 남은 폭 안에서 감긴다고 적는다.
#[test]
fn the_explorer_sample_hint_names_the_menu_label_from_lang() {
    let th = mocha();
    let painted = paint(|ui| super::explorer_sidebar::draw_favorites(ui, &th));
    let label = crate::i18n::t_lang("en", "explorer.context_menu.add_to_favorites");
    let menu = painted
        .texts
        .iter()
        .find(|t| t.text == label)
        .unwrap_or_else(|| panic!("메뉴 이름 {label:?} 이 안내 줄에 그려지지 않았다"));
    assert_eq!(menu.color, th.text_muted().to_egui(), "메뉴 이름 잉크");
    painted.text("Wraps within the available width of the sidebar");
}
