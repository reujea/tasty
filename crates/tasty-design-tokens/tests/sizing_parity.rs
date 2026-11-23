//! SEMANTIC_DIM_TO_THEME_FIELD의 대응표를 따라 SIZING과 DTCG 값을 비교한다.
//! 불일치가 나면 임의로 값을 맞추지 말고 디자인의 의도와 변경 내용을 확인한다.

use tasty_design_tokens::DTCG_JSON;
use tasty_design_tokens::dtcg::{self, SEMANTIC_DIM_TO_THEME_FIELD, ThemeMode};
use tasty_type_appearance::theme::SIZING;

/// `SIZING` 필드명 문자열 → 실제 값(px 필드는 `.0` 로 f32 추출).
/// `SEMANTIC_DIM_TO_THEME_FIELD` 에 새 필드가 추가되면 여기 match arm 도 추가할 것.
fn sizing_value(field: &str) -> f32 {
    match field {
        "spacing_xs" => SIZING.spacing_xs.0,
        "spacing_sm" => SIZING.spacing_sm.0,
        "spacing_md" => SIZING.spacing_md.0,
        "spacing_lg" => SIZING.spacing_lg.0,
        "spacing_xl" => SIZING.spacing_xl.0,
        "border_width" => SIZING.border_width.0,
        "focus_ring_width" => SIZING.focus_ring_width.0,
        "focus_ring_offset" => SIZING.focus_ring_offset.0,
        "corner_radius" => SIZING.corner_radius.0,
        "corner_radius_sm" => SIZING.corner_radius_sm.0,
        "item_height_tree" => SIZING.item_height_tree.0,
        "item_height_interactive" => SIZING.item_height_interactive.0,
        "item_height_tab" => SIZING.item_height_tab.0,
        "tab_width" => SIZING.tab_width.0,
        "font_size_micro" => SIZING.font_size_micro.0,
        "font_size_caption" => SIZING.font_size_caption.0,
        "font_size_body" => SIZING.font_size_body.0,
        "font_size_heading" => SIZING.font_size_heading.0,
        "font_size_max" => SIZING.font_size_max.0,
        "font_size_brand_display" => SIZING.font_size_brand_display.0,
        "font_size_prose_h1" => SIZING.font_size_prose_h1.0,
        "font_size_term_sm" => SIZING.font_size_term_sm.0,
        "font_size_term" => SIZING.font_size_term.0,
        "font_size_term_lg" => SIZING.font_size_term_lg.0,
        "icon_glyph_size_xs" => SIZING.icon_glyph_size_xs.0,
        "icon_glyph_size_sm" => SIZING.icon_glyph_size_sm.0,
        "icon_glyph_size_md" => SIZING.icon_glyph_size_md.0,
        "icon_glyph_size_lg" => SIZING.icon_glyph_size_lg.0,
        "measure_sm" => SIZING.measure_sm.0,
        "measure_md" => SIZING.measure_md.0,
        "measure_lg" => SIZING.measure_lg.0,
        "measure_xl" => SIZING.measure_xl.0,
        "field_width_xs" => SIZING.field_width_xs.0,
        "field_width_color" => SIZING.field_width_color.0,
        "field_width_md" => SIZING.field_width_md.0,
        "field_width_lg" => SIZING.field_width_lg.0,
        "status_bar_height" => SIZING.status_bar_height.0,
        "toolbar_height" => SIZING.toolbar_height.0,
        "titlebar_height" => SIZING.titlebar_height.0,
        "overlay_top_offset" => SIZING.overlay_top_offset.0,
        "label_detail_gap" => SIZING.label_detail_gap.0,
        "selection_edge_width" => SIZING.selection_edge_width.0,
        "tint_edge_width" => SIZING.tint_edge_width.0,
        "border_dash" => SIZING.border_dash.0,
        "border_dash_gap" => SIZING.border_dash_gap.0,
        "status_dot_ring_width" => SIZING.status_dot_ring_width.0,
        "sidebar_logo_size" => SIZING.sidebar_logo_size.0,
        "sidebar_logo_collapsed_size" => SIZING.sidebar_logo_collapsed_size.0,
        "sidebar_wordmark_font_size" => SIZING.sidebar_wordmark_font_size.0,
        "sidebar_section_heading_font_size" => SIZING.sidebar_section_heading_font_size.0,
        "sidebar_button_label_font_size" => SIZING.sidebar_button_label_font_size.0,
        "sidebar_collapsed_slot_width" => SIZING.sidebar_collapsed_slot_width.0,
        "sidebar_collapsed_icon_height" => SIZING.sidebar_collapsed_icon_height.0,
        "sidebar_collapsed_workspace_height" => SIZING.sidebar_collapsed_workspace_height.0,
        "traffic_size" => SIZING.traffic_size.0,
        "caption_width" => SIZING.caption_width.0,
        "window_button_size" => SIZING.window_button_size.0,
        "toast_max_width" => SIZING.toast_max_width.0,
        "toast_accent_width" => SIZING.toast_accent_width.0,
        "status_dot_size" => SIZING.status_dot_size.0,
        "spinner_size" => SIZING.spinner_size.0,
        "tab_bar_height" => SIZING.tab_bar_height.0,
        "tab_bar_label_font_size" => SIZING.tab_bar_label_font_size.0,
        other => panic!(
            "sizing_parity: SEMANTIC_DIM_TO_THEME_FIELD 의 필드 `{other}` 가 \
             sizing_value() 에 없음 — match arm 을 추가할 것"
        ),
    }
}

/// 별칭을 해석한 최종 문자열(`"8px"` / `"1.6"` / `"9999px"`)에서 숫자만 파싱.
fn terminal_number(raw: &str) -> f32 {
    let stripped = raw.strip_suffix("px").unwrap_or(raw);
    stripped
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("최종 값이 숫자가 아님: {raw}"))
}

/// `SEMANTIC_DIM_TO_THEME_FIELD` 표 전체를 데이터로 순회하는 정합 가드.
#[test]
fn sizing_matches_dim_tokens() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    for (path, field) in SEMANTIC_DIM_TO_THEME_FIELD {
        let terminal = set
            .resolve(path, ThemeMode::Mocha)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let expected = terminal_number(&terminal);
        let actual = sizing_value(field);
        assert_eq!(
            actual, expected,
            "SIZING.{field} ({actual}) != {path} ({expected})"
        );
    }
}

/// 배율에 무관한 tint 비율을 DTCG 값과 비교한다.
/// type-appearance는 design-tokens에 의존할 수 없어 상수를 직접 정의한다.
#[test]
fn tint_alphas_match_tokens() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    for (path, actual) in [
        (
            "semantic.tint-fill-alpha",
            tasty_type_appearance::theme::TINT_FILL_ALPHA,
        ),
        (
            "semantic.tint-border-alpha",
            tasty_type_appearance::theme::TINT_BORDER_ALPHA,
        ),
    ] {
        let terminal = set
            .resolve(path, ThemeMode::Mocha)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        let expected = terminal_number(&terminal);
        assert_eq!(actual, expected, "{path} drift (소스 {actual})");
    }
}

/// 상태바 글리프 크기의 component → semantic → primitive 연결을 확인한다.
/// 최종 값만 비교하면 중간 계층을 건너뛴 참조를 놓치므로 각 연결을 따로 검사한다.
#[test]
fn the_statusbar_glyph_size_chain_stands_on_three_tiers() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");

    let component = set
        .get("component.statusbar-glyph-size")
        .expect("component.statusbar-glyph-size 가 없다");
    assert_eq!(component.ty, "dimension", "크기 role 인데 $type 이 다르다");
    assert_eq!(
        dtcg::alias_target(&component.value),
        Some("semantic.icon-size-xs"),
        "component가 semantic을 거치지 않는다"
    );

    let semantic = set
        .get("semantic.icon-size-xs")
        .expect("semantic.icon-size-xs 가 없다");
    assert_eq!(
        dtcg::alias_target(&semantic.value),
        Some("primitive.size-12"),
        "semantic 이 primitive 를 안 거친다"
    );

    let primitive = set
        .get("primitive.size-12")
        .expect("primitive.size-12 가 없다");
    assert_eq!(primitive.value, "12px", "최종 값이 12px가 아니다");

    // 치수는 색 테마에 따라 달라지지 않는다.
    for mode in [ThemeMode::Mocha, ThemeMode::Latte] {
        assert_eq!(
            set.resolve("component.statusbar-glyph-size", mode)
                .unwrap_or_else(|e| panic!("{mode:?}: {e}")),
            "12px",
            "{mode:?} 에서 글리프 크기가 다르다 — 치수는 테마에 의존하지 않는다"
        );
    }
}
