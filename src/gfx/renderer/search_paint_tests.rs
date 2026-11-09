//! 찾기 매치의 글자색. 셀 속성은 실제 SGR 바이트를 터미널에 넣어 얻는다.

use super::*;
use tasty_terminal::Terminal;
use tasty_terminal::search::SearchMatch;
use termwiz::cell::CellAttributes;

fn rgba(v: f32) -> GpuRgba {
    GpuRgba::dangerously_force_from_array([v, v, v, 1.0])
}

const MATCH_COL: usize = 0;
const ACTIVE_COL: usize = 5;
const OUTSIDE_COL: usize = 3;
const MATCHES: [SearchMatch; 2] = [
    SearchMatch {
        row: 0,
        col_start: MATCH_COL,
        col_end: 2,
    },
    SearchMatch {
        row: 0,
        col_start: ACTIVE_COL,
        col_end: 7,
    },
];

fn default_bg() -> GpuRgba {
    rgba(0.05)
}
fn default_fg() -> GpuRgba {
    rgba(0.6)
}
fn ink() -> GpuRgba {
    rgba(0.8)
}
fn active_ink() -> GpuRgba {
    rgba(0.1)
}
/// 빨강 칸만 기본 글자와 값이 같은 팔레트다.
fn ansi() -> [GpuRgb; 16] {
    let mut ansi = [GpuRgb::dangerously_force_from_array([0.2, 0.3, 0.4]); 16];
    ansi[1] = GpuRgb::dangerously_force_from_array([0.6, 0.6, 0.6]);
    ansi
}

fn highlights() -> SearchHighlights<'static> {
    SearchHighlights {
        epoch: tasty_terminal::ContentEpoch::default(),
        matches: &MATCHES,
        active_index: 1,
        inactive_bg: rgba(0.3),
        active_bg: rgba(0.7),
        active_fg: active_ink(),
        inactive_ink: Some(ink()),
    }
}

fn attrs_of(sgr: &str) -> CellAttributes {
    let mut terminal = Terminal::new_detached(20, 3);
    terminal.feed_bytes(format!("{sgr}x").as_bytes());
    terminal.cell_attrs(0, 0).expect("cell")
}

fn fg_at(sh: &SearchHighlights<'_>, col: usize, attrs: &CellAttributes, replaced: bool) -> GpuRgba {
    let (mut bg, mut fg) = compute_cell_colors(attrs, default_bg(), default_fg(), &ansi());
    sh.paint(col, 0, attrs, replaced, &mut bg, &mut fg);
    fg
}

fn own_fg(attrs: &CellAttributes) -> GpuRgba {
    compute_cell_colors(attrs, default_bg(), default_fg(), &ansi()).1
}

/// 프로그램이 전경색을 고르지 않았으면 기본 글자 셀이다. 색을 되돌린 셀과 bold도 같다.
#[test]
fn cells_without_a_chosen_foreground_read_the_match_ink() {
    for (sgr, why) in [
        ("", "untouched"),
        ("\x1b[31m\x1b[39m", "SGR 39 returns to the default"),
        ("\x1b[31m\x1b[0m", "SGR 0 returns to the default"),
        ("\x1b[1m", "bold"),
        ("\x1b[41m", "a background alone chooses no foreground"),
    ] {
        let attrs = attrs_of(sgr);
        assert_eq!(
            fg_at(&highlights(), MATCH_COL, &attrs, false),
            ink(),
            "{why}"
        );
    }
}

#[test]
fn chosen_colours_keep_their_ink_in_a_match() {
    for (sgr, why) in [
        ("\x1b[32m", "ANSI"),
        ("\x1b[92m", "bright ANSI"),
        ("\x1b[38;5;120m", "256 colour"),
        ("\x1b[38;2;10;200;30m", "true colour"),
        ("\x1b[1;32m", "bold ANSI"),
        ("\x1b[2;32m", "dim ANSI"),
    ] {
        let attrs = attrs_of(sgr);
        assert_ne!(own_fg(&attrs), ink(), "{why}");
        assert_eq!(
            fg_at(&highlights(), MATCH_COL, &attrs, false),
            own_fg(&attrs),
            "{why}"
        );
    }
}

/// 값이 기본 글자와 같아도 프로그램이 고른 색이다.
#[test]
fn a_chosen_colour_equal_to_the_default_ink_is_still_chosen() {
    let v = (default_fg().as_array()[0] * 255.0).round() as u8;
    for sgr in ["\x1b[31m".to_string(), format!("\x1b[38;2;{v};{v};{v}m")] {
        let attrs = attrs_of(&sgr);
        let own = own_fg(&attrs).as_array();
        for (got, want) in own.iter().zip(default_fg().as_array()) {
            assert!((got - want).abs() < 0.01, "{sgr:?}: {own:?}");
        }
        assert_eq!(
            fg_at(&highlights(), MATCH_COL, &attrs, false),
            own_fg(&attrs),
            "{sgr:?}"
        );
    }
}

#[test]
fn a_reversed_cell_keeps_its_ink_in_a_match() {
    for sgr in ["\x1b[7m", "\x1b[7;1m"] {
        let attrs = attrs_of(sgr);
        assert_eq!(
            fg_at(&highlights(), MATCH_COL, &attrs, false),
            default_bg(),
            "{sgr:?}"
        );
    }
}

/// dim은 색을 고르지 않는다. 섞인 색으로 두면 매치 칸에서 1.81이라 읽히지 않는다.
#[test]
fn a_dim_default_cell_reads_the_ink() {
    let attrs = attrs_of("\x1b[2m");
    assert_ne!(own_fg(&attrs), ink());
    assert_eq!(fg_at(&highlights(), MATCH_COL, &attrs, false), ink());
}

/// 현재 매치는 셀 종류에 상관없이 현재 매치 글자다.
#[test]
fn the_active_match_overrides_every_cell() {
    for sgr in ["", "\x1b[1m", "\x1b[2m", "\x1b[32m", "\x1b[7m"] {
        let attrs = attrs_of(sgr);
        assert_eq!(
            fg_at(&highlights(), ACTIVE_COL, &attrs, false),
            active_ink(),
            "{sgr:?}"
        );
    }
}

#[test]
fn a_cell_outside_every_match_is_untouched() {
    let attrs = attrs_of("");
    let (mut bg, mut fg) = (default_bg(), default_fg());
    highlights().paint(OUTSIDE_COL, 0, &attrs, false, &mut bg, &mut fg);
    assert_eq!((bg, fg), (default_bg(), default_fg()));
}

/// 블록 커서·vi 커서·링크가 정한 글자색은 일반 매치가 덮지 않는다. 덮으면 커서 칸 글자가 사라진다.
#[test]
fn ink_chosen_by_a_cursor_or_link_stays_in_a_match() {
    let attrs = attrs_of("");
    assert_eq!(fg_at(&highlights(), MATCH_COL, &attrs, true), default_fg());
}

#[test]
fn a_reversed_screen_keeps_the_cell_ink_in_a_match() {
    let attrs = attrs_of("");
    let sh = highlights().on_screen(true);
    assert_eq!(fg_at(&sh, MATCH_COL, &attrs, false), default_fg());
    assert_eq!(fg_at(&sh, ACTIVE_COL, &attrs, false), active_ink());
    let sh = highlights().on_screen(false);
    assert_eq!(fg_at(&sh, MATCH_COL, &attrs, false), ink());
}

/// mirror는 스냅샷 VT를 다시 파싱한다. 그 뒤에도 셀 종류가 같아야 한다.
#[test]
fn the_cell_kind_survives_the_mirror_snapshot() {
    for (sgr, default) in [
        ("", true),
        ("\x1b[1m", true),
        ("\x1b[2m", true),
        ("\x1b[32m", false),
        ("\x1b[38;2;10;200;30m", false),
        ("\x1b[7m", false),
    ] {
        let mut source = Terminal::new_detached(20, 3);
        source.feed_bytes(format!("{sgr}x").as_bytes());
        let mut mirror = Terminal::new_detached(20, 3);
        mirror.feed_bytes(&source.snapshot_as_vt());
        let attrs = mirror.cell_attrs(0, 0).expect("cell");
        assert_eq!(reads_default_ink(&attrs), default, "{sgr:?}");
    }
}

fn latte() -> tasty_type_appearance::theme::Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(true))
}

/// 내장 테마의 포커스 바탕에서 기본·bold 칸은 값이 그대로이고 비포커스 바탕에서만 달라진다.
#[test]
fn the_ink_equals_the_focused_default_ink_on_the_official_themes() {
    for theme in [tasty_themes::mocha_fallback(), latte()] {
        let term = theme.surface("terminal");
        assert_eq!(theme.text_primary(), term.focused_fg);
        assert_ne!(theme.text_primary(), term.unfocused_fg);
    }
}
