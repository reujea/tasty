//! 겹친 상태의 칸 전체 색. 셀 속성은 실제 SGR 바이트를 터미널에 넣어 얻는다.

use super::*;
use crate::cell_palette::compute_cell_colors;
use tasty_selection::SelectionMode;
use tasty_terminal::Terminal;
use termwiz::cell::CellAttributes;

fn rgba(v: f32) -> GpuRgba {
    GpuRgba::dangerously_force_from_array([v, v, v, 1.0])
}
fn bed() -> GpuRgba {
    rgba(0.05)
}
fn cell_ink() -> GpuRgba {
    rgba(0.6)
}
fn sel_bg() -> GpuRgba {
    rgba(0.35)
}
fn sel_ink() -> GpuRgba {
    rgba(0.9)
}
fn ansi() -> [GpuRgb; 16] {
    [GpuRgb::dangerously_force_from_array([0.2, 0.3, 0.4]); 16]
}
fn attrs_of(sgr: &str) -> CellAttributes {
    let mut terminal = Terminal::new_detached(20, 3);
    terminal.feed_bytes(format!("{sgr}x").as_bytes());
    terminal.cell_attrs(0, 0).expect("cell")
}
fn point(col: usize) -> SelectionPoint {
    SelectionPoint {
        epoch: tasty_terminal::ContentEpoch::default(),
        col,
        absolute_row: 0,
    }
}
fn selection() -> (NormalizedSelection, GpuRgba, GpuRgba) {
    (
        NormalizedSelection {
            start: point(0),
            end: point(5),
            mode: SelectionMode::Normal,
        },
        sel_bg(),
        sel_ink(),
    )
}
fn selected(attrs: &CellAttributes, block_cursor: bool, ink: bool) -> (GpuRgba, GpuRgba) {
    let sel = selection();
    paint_cell(
        0,
        0,
        attrs,
        bed(),
        cell_ink(),
        &ansi(),
        block_cursor,
        &CellOverlays {
            selection: Some(&sel),
            selection_ink: ink.then(sel_ink),
            vi_cursor: None,
            link: None,
            search: None,
        },
    )
}

/// bold와 dim은 색을 고르지 않는다. dim을 섞인 색으로 두면 선택 칸에서 1.26 · 1.10이다.
#[test]
fn a_selected_default_cell_reads_the_selection_ink() {
    for sgr in ["", "\x1b[1m", "\x1b[2m", "\x1b[31m\x1b[39m", "\x1b[41m"] {
        assert_eq!(
            selected(&attrs_of(sgr), false, true).1,
            sel_ink(),
            "{sgr:?}"
        );
    }
}

#[test]
fn a_selected_cell_with_its_own_ink_keeps_it() {
    for sgr in [
        "\x1b[32m",
        "\x1b[2;32m",
        "\x1b[38;5;120m",
        "\x1b[38;2;10;200;30m",
        "\x1b[7m",
    ] {
        let attrs = attrs_of(sgr);
        let own = compute_cell_colors(&attrs, bed(), cell_ink(), &ansi()).1;
        assert_ne!(own, sel_ink(), "{sgr:?}");
        assert_eq!(selected(&attrs, false, true).1, own, "{sgr:?}");
    }
}

#[test]
fn a_selected_cell_lies_on_the_selection_fill() {
    assert_eq!(selected(&attrs_of(""), false, true).0, sel_bg());
}

#[test]
fn a_block_cursor_cell_keeps_its_swapped_ink_in_a_selection() {
    assert_eq!(selected(&attrs_of(""), true, true).1, bed());
}

/// 반전 화면의 기본 글자는 바탕 색이다. 선택 잉크는 화면 값에서 빠진다.
#[test]
fn a_reversed_screen_drops_the_selection_ink_only_when_the_setting_allows_it() {
    let mut terminal = Terminal::new_detached(20, 3);
    terminal.feed_bytes(b"\x1b[?5hx");
    terminal.with_content(|view| {
        let paint =
            |enabled| screen_paint(&view, enabled, None, Some(sel_ink()), bed(), cell_ink());
        assert_eq!(paint(true).selection_ink, None);
        assert_eq!(paint(false).selection_ink, Some(sel_ink()));
    });
    assert_eq!(selected(&attrs_of(""), false, false).1, cell_ink());
}

/// 비포커스 바탕에서도 값이 같은 근거다. 바탕이 불투명해 아래 바탕이 글자에 닿지 않는다.
#[test]
fn the_selection_fill_is_opaque_on_the_official_themes() {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    let latte = tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(true));
    for theme in [tasty_themes::mocha_fallback(), latte] {
        assert_eq!(theme.selection_bg.to_gpu_rgba().as_array()[3], 1.0);
        assert_eq!(theme.vi_cursor_bg.to_gpu_rgba().as_array()[3], 1.0);
    }
}
