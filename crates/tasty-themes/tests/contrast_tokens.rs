//! 내장 테마에서 대비 보정 토큰이 디자인이 잰 색·대비를 내는지 확인한다.
//! 디자인 수치는 WCAG 2.x 상대휘도로 sRGB 값에서 잰 것이다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte.toml");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

fn luminance(c: (u8, u8, u8)) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

fn contrast(a: HexColor, b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(rgb(a)), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn rgb(c: HexColor) -> (u8, u8, u8) {
    (c.r(), c.g(), c.b())
}

#[test]
fn approval_danger_fill_is_the_opaque_mix_the_design_measured() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = latte();
    assert_eq!(rgb(mocha.approval_danger_bg()), (56, 43, 61));
    assert_eq!(rgb(latte.approval_danger_bg()), (236, 214, 222));
    assert_eq!(mocha.approval_danger_bg().a(), 255);
    assert_eq!(latte.approval_danger_bg().a(), 255);
    for (theme, label) in [(&mocha, 9.18), (&latte, 5.80)] {
        let measured = contrast(theme.approval_danger_fg(), rgb(theme.approval_danger_bg()));
        assert!(
            (measured - label).abs() < 0.01,
            "label {measured:.2} ≠ {label}"
        );
    }
    for (theme, edge) in [(&mocha, 5.43), (&latte, 3.52)] {
        let measured = contrast(theme.approval_danger_border(), rgb(theme.surface_raised()));
        assert!(
            (measured - edge).abs() < 0.01,
            "edge {measured:.2} ≠ {edge}"
        );
    }
}

/// 반투명 강조색을 불투명 바탕 위에 sRGB 로 합성한다(CSS color-mix in srgb 와 같다).
fn over(top: HexColor, ground: HexColor) -> (u8, u8, u8) {
    let a = f64::from(top.a()) / 255.0;
    let ch = |t: u8, g: u8| (f64::from(t) * a + f64::from(g) * (1.0 - a)).round() as u8;
    (
        ch(top.r(), ground.r()),
        ch(top.g(), ground.g()),
        ch(top.b(), ground.b()),
    )
}

#[test]
fn active_search_match_ink_meets_the_design_contrast() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = latte();
    assert_eq!(mocha.search_match_active_fg, mocha.bg_panel());
    assert_eq!(latte.search_match_active_fg, latte.text_primary());
    assert_eq!(latte.search_match_active_bg.a(), 0x80);
    // 디자인 수치는 bg-panel 위에서 잰 값이다. 테마 파일은 비율을 알파 바이트로 반올림하므로
    // (70% → 0xb3 = 70.2%) 0.1 까지 차이를 허용하고, 4.5 아래로는 내려가지 않는지 함께 본다.
    for (theme, active, ordinary) in [(&mocha, 6.95, 4.75), (&latte, 4.68, 5.50)] {
        let ground = theme.bg_panel();
        let fill = over(theme.search_match_active_bg, ground);
        let measured = contrast(theme.search_match_active_fg, fill);
        assert!(
            (measured - active).abs() < 0.1 && measured >= 4.5,
            "active {measured:.2} ≠ {active}"
        );
        let fill = over(theme.search_match_bg, ground);
        let measured = contrast(theme.text_primary(), fill);
        assert!(
            (measured - ordinary).abs() < 0.1 && measured >= 4.5,
            "ordinary {measured:.2} ≠ {ordinary}"
        );
    }
}

/// 선택 위 매치는 앱 알파로 합성한 값을 잰다. Latte 현재 매치만 바탕 알파가 0xcc로 오른다.
#[test]
fn selected_search_match_ink_meets_the_design_contrast() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = latte();
    assert_eq!(latte.search_match_active_selected_bg().a(), 0xcc);
    for (theme, ordinary, active) in [(&mocha, 5.08, 9.81), (&latte, 8.95, 8.20)] {
        let ink = theme.search_match_selected_fg();
        let fill = over(theme.search_match_bg, theme.selection_bg);
        let measured = contrast(ink, fill);
        assert!(
            (measured - ordinary).abs() < 0.01,
            "ordinary {measured:.2} ≠ {ordinary}"
        );
        let fill = over(theme.search_match_active_selected_bg(), theme.selection_bg);
        let measured = contrast(ink, fill);
        assert!(
            (measured - active).abs() < 0.01,
            "active {measured:.2} ≠ {active}"
        );
    }
}

/// 내장 Mocha 테마 파일과 코드 기본값이 같은 active 글자색을 쓴다.
#[test]
fn mocha_file_and_fallback_agree_on_the_active_match_ink() {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::MOCHA_TOML_TEXT).expect("mocha.toml");
    let (partial, _) = file.to_partial();
    assert_eq!(
        partial.search_match_active_fg,
        Some(tasty_themes::mocha_fallback_colors().search_match_active_fg)
    );
}

/// 읽기 전용 실제 값과 HTML·Markdown 주소의 idle 값은 같은 상자(surface-raised) 위 일반 글자다.
/// Latte 의 text-secondary 는 그 바탕에서 4.05:1 이라 디자인이 text-secondary-raised 로 올렸다.
#[test]
fn read_only_and_idle_address_values_meet_the_body_text_contrast() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = latte();
    for (theme, design) in [(&mocha, 7.10), (&latte, 5.17)] {
        let ground = rgb(theme.input_readonly_bg());
        for (name, ink) in [
            ("input_readonly_fg", theme.input_readonly_fg()),
            ("fp_folder_path_fg", theme.fp_folder_path_fg()),
            ("html_address_value_fg", theme.html_address_value_fg()),
            ("md_address_value_fg", theme.md_address_value_fg()),
        ] {
            let measured = contrast(ink, ground);
            assert!(
                (measured - design).abs() < 0.01,
                "{name} {measured:.2} ≠ {design}"
            );
        }
        assert_eq!(theme.input_readonly_bg(), theme.state_disabled_fill());
    }
}

/// 성능 탭의 적용 시점 안내는 bg-panel 위 일반 글자(4.5:1 이상), info 글리프는 비텍스트(3:1 이상)다.
#[test]
fn performance_apply_note_meets_text_and_glyph_contrast() {
    let mocha = tasty_themes::mocha_fallback();
    let latte = latte();
    for (theme, text, glyph) in [(&mocha, 9.26, 7.37), (&latte, 5.53, 5.53)] {
        let ground = rgb(theme.bg_panel());
        let measured = contrast(theme.settings_apply_note_fg(), ground);
        assert!(
            (measured - text).abs() < 0.01,
            "note {measured:.2} ≠ {text}"
        );
        let measured = contrast(theme.settings_apply_note_glyph(), ground);
        assert!(
            (measured - glyph).abs() < 0.01,
            "glyph {measured:.2} ≠ {glyph}"
        );
    }
}
