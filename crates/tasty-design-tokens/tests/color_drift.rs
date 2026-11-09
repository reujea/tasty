//! DTCG와 내장 Mocha·Latte 테마의 색을 비교한다.
//! ThemeColors에 대응하는 hex primitive만 검사한다. alpha, 절대색, 브랜드색,
//! OS 고정 색은 대응 필드가 없어 제외한다.

use tasty_design_tokens::DTCG_JSON;
use tasty_design_tokens::dtcg::{self, ThemeMode};
use tasty_themes::{LATTE_TOML_TEXT, MOCHA_TOML_TEXT, ThemeFile, mocha_fallback_colors};
use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

#[test]
fn primitive_colors_match_embedded_themes() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    let mocha = mocha_fallback_colors();
    let latte_file = ThemeFile::parse(LATTE_TOML_TEXT).expect("latte.toml must parse");
    let (latte, is_light) = latte_file.to_partial();
    assert_eq!(is_light, Some(true), "latte.toml must declare is_light");

    // (DTCG primitive 이름, mocha 기준값, latte 기준값)
    let pairs: &[(&str, HexColor, Option<HexColor>)] = &[
        // ── neutral ramp 12단 (neutral-0 = 최심 배경 … neutral-1100 = 최강 전경) ──
        ("color-neutral-0", mocha.crust, latte.crust),
        ("color-neutral-100", mocha.mantle, latte.mantle),
        ("color-neutral-200", mocha.base, latte.base),
        ("color-neutral-300", mocha.surface0, latte.surface0),
        ("color-neutral-400", mocha.surface1, latte.surface1),
        ("color-neutral-500", mocha.surface2, latte.surface2),
        ("color-neutral-600", mocha.overlay0, latte.overlay0),
        ("color-neutral-700", mocha.overlay1, latte.overlay1),
        ("color-neutral-800", mocha.overlay2, latte.overlay2),
        ("color-neutral-900", mocha.subtext0, latte.subtext0),
        ("color-neutral-1000", mocha.subtext1, latte.subtext1),
        ("color-neutral-1100", mocha.text, latte.text),
        // ── accent hue 13종 (catppuccin — 테마당 hue 별 1값, ramp 없음) ──
        ("color-blue", mocha.blue, latte.blue),
        ("color-green", mocha.green, latte.green),
        ("color-red", mocha.red, latte.red),
        ("color-yellow", mocha.yellow, latte.yellow),
        ("color-peach", mocha.peach, latte.peach),
        ("color-mauve", mocha.mauve, latte.mauve),
        ("color-teal", mocha.teal, latte.teal),
        ("color-sky", mocha.sky, latte.sky),
        ("color-lavender", mocha.lavender, latte.lavender),
        ("color-flamingo", mocha.flamingo, latte.flamingo),
        ("color-pink", mocha.pink, latte.pink),
        ("color-maroon", mocha.maroon, latte.maroon),
        ("color-rosewater", mocha.rosewater, latte.rosewater),
    ];

    for (name, mocha_expected, latte_expected) in pairs {
        let path = format!("primitive.{name}");

        let mocha_raw = set
            .resolve(&path, ThemeMode::Mocha)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let mocha_actual = HexColor::from_hex(&mocha_raw)
            .unwrap_or_else(|| panic!("{path}: mocha 값이 hex 가 아님: {mocha_raw}"));
        assert_eq!(
            mocha_actual, *mocha_expected,
            "{path} mocha drift ({mocha_raw})"
        );

        let latte_raw = set
            .resolve(&path, ThemeMode::Latte)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let latte_actual = HexColor::from_hex(&latte_raw)
            .unwrap_or_else(|| panic!("{path}: latte 값이 hex 가 아님: {latte_raw}"));
        assert_eq!(
            Some(latte_actual),
            *latte_expected,
            "{path} latte drift ({latte_raw})"
        );
    }
}

/// 생성 상수는 Mocha 값만 담으므로, 흐린 노드는 Theme 접근자로 Latte 값까지 읽어야 한다.
#[test]
fn node_dim_opacity_follows_the_token_in_both_modes() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    let build = |light: bool| Theme::with_colors(mocha_fallback_colors(), light);
    for (mode, light) in [(ThemeMode::Mocha, false), (ThemeMode::Latte, true)] {
        let token: f32 = set
            .resolve("component.dag-node-dim-opacity", mode)
            .expect("token must resolve")
            .parse()
            .expect("opacity must be a number");
        assert_eq!(build(light).node_dim_opacity(), token);
    }
}

/// 내장 테마의 placeholder는 입력 상자 위 일반 글자 등급을 넘는 subtext다.
/// 사용자 설정에서는 별도 필드로 바꿀 수 있다.
#[test]
fn placeholder_uses_the_subtext_band_in_shipped_themes() {
    let mocha = mocha_fallback_colors();
    assert_eq!(mocha.placeholder, HexColor::from_hex("#a6adc8").unwrap());
    let latte_file = ThemeFile::parse(LATTE_TOML_TEXT).expect("latte.toml must parse");
    let (latte, _) = latte_file.to_partial();
    assert_eq!(latte.placeholder, HexColor::from_hex("#5c5f77"));
}

/// 내장 테마의 표면 전경은 focused_fg=text, unfocused_fg=text-muted를 따른다.
/// text-muted는 Mocha에서 subtext0, Latte에서 subtext1이다.
/// 배경은 표면 종류마다 관계가 달라 이 시험에서 비교하지 않는다.
#[test]
fn surface_foregrounds_track_palette_in_shipped_themes() {
    for (name, toml, light) in [
        ("mocha", MOCHA_TOML_TEXT, false),
        ("latte", LATTE_TOML_TEXT, true),
    ] {
        let file = ThemeFile::parse(toml).unwrap_or_else(|e| panic!("{name}.toml: {e}"));
        let text = file
            .palette
            .text
            .unwrap_or_else(|| panic!("{name}.toml: palette.text 없음"));
        let palette_muted = if light {
            file.palette.subtext1
        } else {
            file.palette.subtext0
        }
        .unwrap_or_else(|| panic!("{name}.toml: text-muted의 palette 값 없음"));
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        let muted = Theme::with_colors(colors, light).text_muted();
        assert_eq!(
            muted, palette_muted,
            "{name} text-muted가 palette의 약속한 칸과 다르다"
        );

        assert!(
            !file.surfaces.is_empty(),
            "{name}.toml 에 [surfaces.*] 가 하나도 없다 — 비교할 표면이 없다"
        );
        for (kind, surface) in &file.surfaces {
            assert_eq!(
                surface.focused_fg,
                Some(text),
                "{name}.toml [surfaces.{kind}].focused_fg != palette.text ({text:?})"
            );
            assert_eq!(
                surface.unfocused_fg,
                Some(muted),
                "{name}.toml [surfaces.{kind}].unfocused_fg != text-muted ({muted:?})"
            );
        }
    }
}

/// WCAG 2.x 상대 명도. sRGB 역감마 문턱은 이 계산식의 0.03928을 사용한다.
fn relative_luminance(c: HexColor) -> f64 {
    let channel = |v: u8| {
        let x = f64::from(v) / 255.0;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

fn contrast_ratio(a: HexColor, b: HexColor) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// 패널 위 프레임 선은 border-strong보다 대비가 높아야 한다.
/// Mocha에서는 더 밝고 Latte에서는 더 어두우므로 명도 부등호도 반대인지 확인한다.
#[test]
fn the_frame_line_outranks_the_strong_border_by_contrast_not_lightness() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    let hex = |path: &str, mode| {
        let raw = set
            .resolve(path, mode)
            .unwrap_or_else(|e| panic!("{path} ({mode:?}): {e:?}"));
        HexColor::from_hex(&raw).unwrap_or_else(|| panic!("{path}: hex 가 아님: {raw}"))
    };

    // (모드, 이름, border-strong 대비, border-frame 대비)
    for (mode, name, want_before, want_after) in [
        (ThemeMode::Mocha, "mocha", 1.80_f64, 2.46_f64),
        (ThemeMode::Latte, "latte", 1.61, 1.91),
    ] {
        let panel = hex("semantic.bg-panel", mode);
        let strong = hex("semantic.border-strong", mode);
        let frame = hex("semantic.border-frame", mode);

        assert_ne!(
            frame, strong,
            "{name}: border-frame과 border-strong의 색이 같아졌다"
        );

        let before = contrast_ratio(strong, panel);
        let after = contrast_ratio(frame, panel);
        assert!(
            (before - want_before).abs() < 0.005,
            "{name}: border-strong 대비가 {before:.4}로 기대값 {want_before}와 다르다"
        );
        assert!(
            (after - want_after).abs() < 0.005,
            "{name}: border-frame 대비가 {after:.4}로 기대값 {want_after}와 다르다"
        );
        assert!(
            after > before,
            "{name}: border-frame의 패널 대비가 border-strong보다 높지 않다 ({before:.4} → {after:.4})"
        );
    }

    // 명도 부등호는 두 테마에서 **반대**여야 한다 — 위 doc 주석의 근거.
    let lighter_in_mocha = relative_luminance(hex("semantic.border-frame", ThemeMode::Mocha))
        > relative_luminance(hex("semantic.border-strong", ThemeMode::Mocha));
    let lighter_in_latte = relative_luminance(hex("semantic.border-frame", ThemeMode::Latte))
        > relative_luminance(hex("semantic.border-strong", ThemeMode::Latte));
    assert!(
        lighter_in_mocha && !lighter_in_latte,
        "명도 부등호가 두 테마에서 같은 방향이 됐다 (mocha frame 이 더 밝다={lighter_in_mocha} · \
         latte frame 이 더 밝다={lighter_in_latte}) — 프레임 색 선택의 근거를 다시 확인해야 한다"
    );
}

/// 실제 배지 접근자의 대비를 검사하므로 alias를 전역 강조색으로 되돌리면 Latte에서 실패한다.
#[test]
fn notification_warning_badge_is_readable_in_both_shipped_themes() {
    for (name, toml, light) in [
        ("mocha", MOCHA_TOML_TEXT, false),
        ("latte", LATTE_TOML_TEXT, true),
    ] {
        let file = ThemeFile::parse(toml).expect("embedded theme must parse");
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        let th = Theme::with_colors(colors, light);
        let bg = th.notification_badge_warning_bg();
        let fg = th.notification_badge_warning_fg();
        let ratio = contrast_ratio(bg, fg);
        assert!(
            ratio >= 4.5,
            "{name} warning badge contrast {ratio:.4} < 4.5"
        );
        assert_eq!(
            th.notification_item_warning_glyph(),
            th.accent_warning_glyph()
        );
        assert_eq!(th.notification_badge_error_fg(), th.text_on_accent());
        assert_eq!(th.notification_badge_info_fg(), th.text_on_accent());
        if !light {
            assert_eq!(bg, th.accent_warning());
            assert_eq!(fg, th.text_on_accent());
        }
    }
}

/// 수기로 옮긴 Latte color-mix가 디자인 측 CSS의 계산값과 같고 Mocha는 원래 역할을 유지하는지 본다.
/// 대비 하한은 평상 상태에서 화면에 머무는 글자 4.5:1, 글리프 3:1이다.
#[test]
fn state_ink_matches_the_design_values_and_clears_contrast() {
    let build = |toml: &str, light: bool| {
        let file = ThemeFile::parse(toml).expect("embedded theme must parse");
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, light)
    };
    let latte = build(LATTE_TOML_TEXT, true);
    let mocha = build(MOCHA_TOML_TEXT, false);

    assert_latte_state_ink(&latte);
    assert_mocha_state_ink(&mocha);
    for (name, th) in [("mocha", &mocha), ("latte", &latte)] {
        assert_ink_on_active(name, th);
        assert_ink_on_raised(name, th);
    }
}

fn hex(s: &str) -> HexColor {
    HexColor::from_hex(s).expect("valid hex")
}

fn assert_latte_state_ink(latte: &Theme) {
    assert_eq!(latte.surface_on_raised(), hex("#eff1f5"));
    assert_eq!(latte.text_on_active(), hex("#35374a"));
    assert_eq!(latte.accent_primary_on_active(), hex("#14429f"));
    assert_eq!(latte.accent_warning_raised(), hex("#70470f"));
    assert_eq!(latte.accent_attention_raised(), hex("#8c3706"));
    assert_eq!(latte.accent_info_raised(), hex("#025b7e"));
    assert_eq!(latte.text_muted(), hex("#5c5f77"));
    assert_eq!(latte.accent_warning_on_active(), hex("#64400d"));
    assert_eq!(latte.accent_success_on_active(), hex("#235818"));
    assert_eq!(latte.accent_danger_on_active(), hex("#930b28"));
    assert_eq!(latte.accent_agent_on_active(), hex("#5f28a7"));
    assert_eq!(latte.accent_remote_on_active(), hex("#025373"));
    assert_eq!(latte.accent_attached_raised(), hex("#3f4a8b"));
    assert_eq!(latte.accent_attached_on_active(), hex("#39447f"));
}

fn assert_mocha_state_ink(mocha: &Theme) {
    assert_eq!(mocha.surface_on_raised(), hex("#45475a"));
    assert_eq!(mocha.text_on_active(), hex("#cdd6f4"));
    assert_eq!(mocha.accent_primary_on_active(), hex("#b2cefc"));
    assert_eq!(mocha.accent_warning_raised(), hex("#f9e2af"));
    assert_eq!(mocha.accent_attention_raised(), hex("#fab387"));
    assert_eq!(mocha.accent_info_raised(), hex("#89dceb"));
    assert_eq!(mocha.text_muted(), hex("#a6adc8"));
    assert_eq!(mocha.accent_warning_on_active(), hex("#f9e2af"));
    assert_eq!(mocha.accent_success_on_active(), hex("#a6e3a1"));
    assert_eq!(mocha.accent_danger_on_active(), hex("#f8b9cb"));
    assert_eq!(mocha.accent_agent_on_active(), hex("#dbc1f9"));
    assert_eq!(mocha.accent_remote_on_active(), hex("#89dceb"));
    assert_eq!(mocha.accent_attached_raised(), hex("#b4befe"));
    assert_eq!(mocha.accent_attached_on_active(), hex("#bfc8fe"));
}

/// 찾기 적중 쌍은 바탕과 무관한 불투명 값이다. 디자인 대비표의 값과 같아야 한다.
#[test]
fn markdown_find_pairs_match_the_design_values() {
    let build = |toml: &str, light: bool| {
        let file = ThemeFile::parse(toml).expect("embedded theme must parse");
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, light)
    };
    let cases = [
        (
            "mocha",
            build(MOCHA_TOML_TEXT, false),
            ["#f9e2af", "#11111b", "#89b4fa", "#11111b"],
        ),
        (
            "latte",
            build(LATTE_TOML_TEXT, true),
            ["#efc78e", "#4c4f69", "#1e66f5", "#ffffff"],
        ),
    ];
    for (name, th, [hit_bg, hit_fg, current_bg, current_fg]) in cases {
        assert_eq!(th.md_find_hit_bg(), hex(hit_bg), "{name} hit bg");
        assert_eq!(th.md_find_hit_fg(), hex(hit_fg), "{name} hit fg");
        assert_eq!(
            th.md_find_current_bg(),
            hex(current_bg),
            "{name} current bg"
        );
        assert_eq!(
            th.md_find_current_fg(),
            hex(current_fg),
            "{name} current fg"
        );
    }
}

/// 터미널과 markdown의 바탕이 포커스 바탕 쌍 하나를 쓰고, Latte만 다른 역할이 정본 값과 같은지 본다.
#[test]
fn surface_bed_pair_and_latte_remaps_match_the_design_values() {
    let build = |toml: &str, light: bool| {
        let file = ThemeFile::parse(toml).expect("embedded theme must parse");
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, light)
    };
    let cases = [
        (
            "mocha",
            build(MOCHA_TOML_TEXT, false),
            ["#000000", "#1e1e2e", "#94e2d5", "#1e1e2e", "#181825"],
        ),
        (
            "latte",
            build(LATTE_TOML_TEXT, true),
            ["#ffffff", "#dce0e8", "#147c82", "#000000", "#e6e9ef"],
        ),
    ];
    for (name, th, [focused, unfocused, data_strong, match_fg, row]) in cases {
        assert_eq!(th.surface_bed_focused(), hex(focused), "{name} focused bed");
        assert_eq!(
            th.surface_bed_unfocused(),
            hex(unfocused),
            "{name} unfocused bed"
        );
        for kind in ["terminal", "markdown"] {
            assert_eq!(th.surface(kind).focused_bg, hex(focused), "{name} {kind}");
            assert_eq!(
                th.surface(kind).unfocused_bg,
                hex(unfocused),
                "{name} {kind}"
            );
        }
        assert_eq!(th.accent_data_strong(), hex(data_strong), "{name} data");
        assert_eq!(th.dag_edge_binding(), hex(data_strong), "{name} binding");
        assert_eq!(
            th.search_match_active_unfocused_fg(),
            hex(match_fg),
            "{name} match ink"
        );
        assert_eq!(th.md_table_row_bg_unfocused(), hex(row), "{name} row");
    }
}

fn assert_ink_on_active(name: &str, th: &Theme) {
    let active = th.surface_active();
    for (text, ink) in [
        ("text", th.text_on_active()),
        ("match run", th.explorer_match_fg_active()),
        ("kbd plus", th.kbd_plus_fg_active()),
    ] {
        assert!(
            contrast_ratio(active, ink) >= 4.5,
            "{name} {text} on active"
        );
    }
    for (glyph, ink) in [
        ("primary", th.accent_primary_on_active()),
        ("warning", th.accent_warning_on_active()),
        ("success", th.accent_success_on_active()),
        ("danger", th.accent_danger_on_active()),
        ("agent", th.accent_agent_on_active()),
        ("remote", th.accent_remote_on_active()),
        ("attached", th.accent_attached_on_active()),
        ("completion dot", th.status_dot_completion_active()),
    ] {
        assert!(
            contrast_ratio(active, ink) >= 3.0,
            "{name} {glyph} glyph on active"
        );
    }
}

fn assert_ink_on_raised(name: &str, th: &Theme) {
    assert!(
        contrast_ratio(th.bg_app(), th.text_muted()) >= 4.5,
        "{name} muted on bg-app"
    );
    let raised = th.tag_bg();
    for (tag, ink) in [
        ("default", th.tag_fg()),
        ("success", th.tag_success_fg()),
        ("warning", th.tag_warning_fg()),
        ("attention", th.tag_attention_fg()),
        ("danger", th.tag_danger_fg()),
        ("info", th.tag_info_fg()),
        ("attached", th.tag_attached_fg()),
    ] {
        assert!(contrast_ratio(raised, ink) >= 4.5, "{name} tag {tag}");
    }
    assert!(
        contrast_ratio(th.surface_raised(), th.kbd_fg()) >= 4.5,
        "{name} kbd"
    );
    assert!(
        contrast_ratio(th.banner_button_bg(), th.text_primary()) >= 4.5,
        "{name} banner button"
    );
}

/// 생성기는 strokeStyle을 상수로 만들지 않으므로 앱이 잘라낸 항목 이름에 이탤릭을 직접 건다.
/// 디자인이 값을 바꾸면 그 코드(`tasty_ui_widgets::italicize_name`)도 함께 바꾼다.
#[test]
fn cut_pending_font_style_is_the_italic_the_app_applies() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    for mode in [ThemeMode::Mocha, ThemeMode::Latte] {
        assert_eq!(
            set.resolve("component.cut-pending-font-style", mode)
                .expect("token must resolve"),
            "italic"
        );
    }
}

/// 선택 위 매치의 새 역할. 키가 없는 바탕은 사용자 테마의 현재 매치 바탕을 따른다.
#[test]
fn selected_match_roles_match_the_design_values() {
    let build = |toml: &str, light: bool| {
        let file = ThemeFile::parse(toml).expect("embedded theme must parse");
        let (partial, _) = file.to_partial();
        let mut colors = mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, light)
    };
    let mocha = build(MOCHA_TOML_TEXT, false);
    let latte = build(LATTE_TOML_TEXT, true);
    assert_eq!(mocha.search_match_selected_fg(), hex("#11111b"));
    assert_eq!(latte.search_match_selected_fg(), hex("#000000"));
    assert_eq!(
        mocha.search_match_active_selected_bg(),
        mocha.search_match_active_bg
    );
    assert_eq!(latte.search_match_active_selected_bg(), hex("#df8e1dcc"));

    let mut colors = mocha_fallback_colors();
    colors.search_match_active_bg = hex("#12345680");
    let user = Theme::with_colors(colors, true);
    assert_eq!(user.search_match_active_selected_bg(), hex("#12345680"));
}
