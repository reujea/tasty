//! 숨긴 칸(SGR 8)과 앱 오버레이. 디자인 RULES "Hidden cells (SGR 8)": 오버레이는 바탕만 칠하고
//! 숨긴 글자는 어느 오버레이에서도 드러내지 않는다. 바탕은 같은 칸에 숨김이 없을 때와 같다.
//! 색은 렌더 호출부가 테마에서 읽는 역할 그대로 쓴다(Mocha·Latte, 포커스·비포커스).
//!
//! 이미 다른 시험이 고정한 것:
//! - 숨긴 칸 + hover 링크의 밑줄이 남는다: `cell_decor_tests::a_hidden_cell_draws_no_program_line_but_keeps_the_link_underline`.
//!   링크 칸의 밑줄 색이 링크 입력의 밑줄 색이고 커서 칸은 커서 잉크다: `cell_paint_tests::a_cursor_cell_in_a_link_stays_a_cursor_cell_with_a_line_in_its_ink`.
//! - SGR 28 뒤 칸은 다시 글리프를 낸다: `cell_decor_tests::a_hidden_cell_draws_no_glyph_and_sgr_28_shows_the_next_one`.

use super::cell_decor::{
    CellPlace, LineGeometry, LineLayer, LineSpan, ProgramLines, covered_columns, draws_glyph,
    fill_cell,
};
use super::cell_paint_tests::official_themes;
use super::*;
use crate::cell_palette::compute_cell_colors;
use tasty_selection::SelectionMode;
use tasty_terminal::Terminal;
use tasty_terminal::search::SearchMatch;
use tasty_terminal_link::LinkSegment;
use tasty_type_appearance::theme::Theme;
use termwiz::cell::CellAttributes;

const ORDINARY_COL: usize = 0;
const ACTIVE_COL: usize = 2;
const PLAIN_COL: usize = 6;
const MATCHES: [SearchMatch; 2] = [
    SearchMatch {
        row: 0,
        col_start: 0,
        col_end: 2,
    },
    SearchMatch {
        row: 0,
        col_start: 2,
        col_end: 4,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Overlay {
    BlockCursor,
    ViCursor,
    Selection,
    OrdinaryMatch,
    ActiveMatch,
    SelectionOrdinaryMatch,
    SelectionActiveMatch,
    HoverLink,
}

const OVERLAYS: [Overlay; 8] = [
    Overlay::BlockCursor,
    Overlay::ViCursor,
    Overlay::Selection,
    Overlay::OrdinaryMatch,
    Overlay::ActiveMatch,
    Overlay::SelectionOrdinaryMatch,
    Overlay::SelectionActiveMatch,
    Overlay::HoverLink,
];

fn point(col: usize) -> SelectionPoint {
    SelectionPoint {
        epoch: tasty_terminal::ContentEpoch::default(),
        col,
        absolute_row: 0,
    }
}

/// 첫 행의 칸 텍스트와 속성.
fn row_cells(bytes: &str) -> Vec<(String, CellAttributes)> {
    let mut terminal = Terminal::new_detached(20, 3);
    terminal.feed_bytes(bytes.as_bytes());
    terminal.with_content(|view| {
        view.surface().screen_lines()[0]
            .visible_cells()
            .take(8)
            .map(|cell| (cell.str().to_string(), cell.attrs().clone()))
            .collect()
    })
}

/// 렌더 호출부(`render_pass.rs`, `view/main/mouse.rs`)가 테마에서 만드는 입력.
struct ThemeInputs {
    bg: GpuRgba,
    fg: GpuRgba,
    ansi: [GpuRgb; 16],
    selection: (NormalizedSelection, GpuRgba, GpuRgba),
    vi: (SelectionPoint, GpuRgba, GpuRgba),
    link: LinkHighlight,
    search: SearchHighlights<'static>,
}

fn inputs(theme: &Theme, focused: bool, overlay: Overlay) -> ThemeInputs {
    let surface = theme.surface("terminal");
    let (bg, fg) = if focused {
        (surface.focused_bg, surface.focused_fg)
    } else {
        (surface.unfocused_bg, surface.unfocused_fg)
    };
    let (start, end) = match overlay {
        Overlay::SelectionOrdinaryMatch | Overlay::SelectionActiveMatch => (0, 4),
        _ => (PLAIN_COL - 1, PLAIN_COL + 1),
    };
    ThemeInputs {
        bg: bg.to_gpu_rgba(),
        fg: fg.to_gpu_rgba(),
        ansi: theme.ansi_palette(),
        selection: (
            NormalizedSelection {
                start: point(start),
                end: point(end),
                mode: SelectionMode::Normal,
            },
            theme.selection_bg.to_gpu_rgba(),
            theme.text_on_active().to_gpu_rgba(),
        ),
        vi: (
            point(PLAIN_COL),
            theme.vi_cursor_bg.to_gpu_rgba(),
            theme.vi_cursor_fg().to_gpu_rgba(),
        ),
        link: LinkHighlight {
            epoch: tasty_terminal::ContentEpoch::default(),
            segments: vec![LinkSegment {
                absolute_row: 0,
                start_col: PLAIN_COL,
                end_col: PLAIN_COL + 1,
            }],
            fg: theme.text_on_active().to_gpu_rgba(),
            bg: theme.selection_bg.to_gpu_rgba(),
            underline: theme.accent_primary_raised().to_gpu_rgba(),
        },
        search: SearchHighlights::for_theme(
            theme,
            focused,
            tasty_terminal::ContentEpoch::default(),
            &MATCHES,
            1,
        ),
    }
}

fn overlay_col(overlay: Overlay) -> usize {
    match overlay {
        Overlay::OrdinaryMatch | Overlay::SelectionOrdinaryMatch => ORDINARY_COL,
        Overlay::ActiveMatch | Overlay::SelectionActiveMatch => ACTIVE_COL,
        _ => PLAIN_COL,
    }
}

/// 오버레이 하나를 건 칸의 칠. 렌더 경로처럼 화면 반전이 없으면 선택 잉크를 넘긴다.
fn paint_under(attrs: &CellAttributes, overlay: Overlay, input: &ThemeInputs) -> CellPaint {
    let selection = matches!(
        overlay,
        Overlay::Selection | Overlay::SelectionOrdinaryMatch | Overlay::SelectionActiveMatch
    )
    .then_some(&input.selection);
    let search = matches!(
        overlay,
        Overlay::OrdinaryMatch
            | Overlay::ActiveMatch
            | Overlay::SelectionOrdinaryMatch
            | Overlay::SelectionActiveMatch
    )
    .then_some(&input.search);
    let overlays = CellOverlays {
        selection,
        selection_ink: selection.map(|(.., ink)| *ink),
        vi_cursor: (overlay == Overlay::ViCursor).then_some(&input.vi),
        link: (overlay == Overlay::HoverLink).then_some(&input.link),
        search,
        preedit: None,
    };
    paint_cell(
        overlay_col(overlay),
        0,
        attrs,
        input.bg,
        input.fg,
        &input.ansi,
        (overlay == Overlay::BlockCursor).then_some(CursorPaint::Block),
        &overlays,
    )
}

/// 표: 테마 × 포커스 × 오버레이. 블록 커서는 포커스에만 있다.
fn each_case(mut check: impl FnMut(&str, bool, Overlay, &ThemeInputs)) {
    for (name, theme) in ["Mocha", "Latte"].into_iter().zip(official_themes()) {
        let surface = theme.surface("terminal");
        assert_ne!(surface.focused_bg, surface.unfocused_bg, "{name}");
        for focused in [true, false] {
            for overlay in OVERLAYS {
                if overlay == Overlay::BlockCursor && !focused {
                    continue;
                }
                check(name, focused, overlay, &inputs(&theme, focused, overlay));
            }
        }
    }
}

#[test]
fn a_hidden_cell_under_every_overlay_draws_no_glyph_and_keeps_the_plain_fill() {
    let hidden = row_cells("\x1b[8mABCDEFGH\x1b[0m");
    let shown = row_cells("ABCDEFGH");
    let mut cases = 0;
    each_case(|name, focused, overlay, input| {
        let col = overlay_col(overlay);
        let (text, attrs) = &hidden[col];
        assert!(attrs.invisible());
        let paint = paint_under(attrs, overlay, input);
        let mut bg = Vec::new();
        let place = CellPlace {
            columns: covered_columns(col, 1, 20),
            row: 0,
            absolute_row: 0,
        };
        let geometry = LineGeometry::new(PhysicalPx(14.0), PhysicalPx(11.0), None);
        let (_, covered) = fill_cell(&mut bg, &place, attrs, &paint, None, &geometry, [0.0; 2]);
        let case = format!("{name} focused={focused} {overlay:?}");
        assert!(!draws_glyph(text, attrs, covered), "{case}: 글리프 0");
        let plain = paint_under(&shown[col].1, overlay, input);
        assert!(draws_glyph(&shown[col].0, &shown[col].1, false), "{case}");
        assert_eq!(paint.bg, plain.bg, "{case}: 바탕은 숨김이 없을 때와 같다");
        assert_eq!(bg[0].bg_color, plain.bg, "{case}: 낸 바탕 인스턴스");
        cases += 1;
    });
    assert_eq!(cases, 2 * (8 + 7));
}

#[test]
fn a_block_cursor_on_a_hidden_cell_fills_with_the_cell_ink() {
    for sgr in ["", "31;", "41;", "2;", "7;"] {
        let hidden = row_cells(&format!("\x1b[{sgr}8mABCDEFGH\x1b[0m"));
        for (name, theme) in ["Mocha", "Latte"].into_iter().zip(official_themes()) {
            let input = inputs(&theme, true, Overlay::BlockCursor);
            let attrs = &hidden[PLAIN_COL].1;
            let ink = compute_cell_colors(attrs, input.bg, input.fg, &input.ansi).1;
            let paint = paint_under(attrs, Overlay::BlockCursor, &input);
            assert_eq!(paint.bg, ink, "{name} {sgr:?}: 맞바꾼 바탕은 칸의 잉크");
        }
    }
}

#[test]
fn a_hidden_link_cell_draws_no_program_line() {
    for (name, theme) in ["Mocha", "Latte"].into_iter().zip(official_themes()) {
        for focused in [true, false] {
            let input = inputs(&theme, focused, Overlay::HoverLink);
            let geometry = LineGeometry::new(
                PhysicalPx(14.0),
                PhysicalPx(11.0),
                Some(ProgramLines {
                    underline: LineSpan {
                        top: PhysicalPx(12.0),
                        height: PhysicalPx(1.0),
                    },
                    strikethrough: LineSpan {
                        top: PhysicalPx(7.0),
                        height: PhysicalPx(1.0),
                    },
                    overline: LineSpan {
                        top: PhysicalPx(0.0),
                        height: PhysicalPx(1.0),
                    },
                }),
            );
            let hidden = row_cells("\x1b[4;9;53;8mABCDEFGH\x1b[0m");
            let attrs = &hidden[PLAIN_COL].1;
            let paint = paint_under(attrs, Overlay::HoverLink, &input);
            let lines = super::cell_decor::lines_for_cell(attrs, &paint, false, &geometry);
            let got: Vec<_> = lines.iter().map(|l| (l.layer, l.span, l.color)).collect();
            assert_eq!(
                got,
                vec![(
                    LineLayer::UnderGlyph,
                    geometry.link_underline,
                    theme.accent_primary_raised().to_gpu_rgba()
                )],
                "{name} focused={focused}: 링크 밑줄 하나, 취소선·윗줄 없음"
            );
        }
    }
}
