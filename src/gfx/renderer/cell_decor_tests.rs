//! 칸이 긋는 선의 판단과 인스턴스. 셀 속성은 실제 SGR 바이트를 터미널에 넣어 얻는다.
//! 프로그램 선의 치수는 시험 전용 값이다. 제품 값은 디자인 규칙에서 정한다.

use super::cell_decor::{
    CellPlace, LineGeometry, LineSpan, ProgramLines, SolidTexel, covered_columns, draws_glyph,
    fill_cell, lines_for_cell, push_over_lines,
};
use super::line_render::scrolled_cell_paint;
use super::*;
use crate::cell_palette::compute_cell_colors;
use tasty_terminal::Terminal;
use tasty_terminal_link::LinkSegment;
use termwiz::cell::CellAttributes;

const CELL_WIDTH: PhysicalPx = PhysicalPx(7.0);
const UNDER: LineSpan = LineSpan {
    top: PhysicalPx(12.0),
    height: PhysicalPx(1.0),
};
const STRIKE: LineSpan = LineSpan {
    top: PhysicalPx(7.0),
    height: PhysicalPx(1.0),
};
const OVER: LineSpan = LineSpan {
    top: PhysicalPx(0.0),
    height: PhysicalPx(1.0),
};
const SOLID: SolidTexel = SolidTexel {
    uv: [0.5, 0.5],
    page: 3,
};

fn rgba(v: f32) -> GpuRgba {
    GpuRgba::dangerously_force_from_array([v, v, v, 1.0])
}
fn bed() -> GpuRgba {
    rgba(0.05)
}
fn cell_ink() -> GpuRgba {
    rgba(0.6)
}
fn ansi() -> [GpuRgb; 16] {
    [GpuRgb::dangerously_force_from_array([0.2, 0.3, 0.4]); 16]
}
fn no_overlays() -> CellOverlays<'static> {
    CellOverlays {
        selection: None,
        selection_ink: None,
        vi_cursor: None,
        link: None,
        search: None,
        preedit: None,
    }
}
/// 시험 전용 치수를 넣은 기하. `program`이 거짓이면 제품처럼 값이 없다.
fn geometry(program: bool) -> LineGeometry {
    LineGeometry::new(
        PhysicalPx(14.0),
        PhysicalPx(11.0),
        program.then_some(ProgramLines {
            underline: UNDER,
            strikethrough: STRIKE,
            overline: OVER,
        }),
    )
}

/// 현재 화면 경로(`fill_surface`)처럼 첫 행의 칸을 읽는다. 넓은 글자는 두 칸이다.
fn screen_cells(bytes: &str, cols: usize) -> Vec<(usize, usize, CellAttributes)> {
    let mut terminal = Terminal::new_detached(cols, 3);
    terminal.feed_bytes(bytes.as_bytes());
    terminal.with_content(|view| {
        view.surface().screen_lines()[0]
            .visible_cells()
            .map(|cell| {
                let wide = cell
                    .str()
                    .chars()
                    .next()
                    .is_some_and(|ch| unicode_width(ch) > 1);
                (
                    cell.cell_index(),
                    if wide { 2 } else { 1 },
                    cell.attrs().clone(),
                )
            })
            .collect()
    })
}

/// 스크롤한 위치의 scrollback 경로(`render_scrollback_line`)처럼 가장 오래된 줄의 칸을 읽는다.
fn scrollback_cells(bytes: &str, cols: usize) -> Vec<(usize, usize, CellAttributes)> {
    let mut terminal = Terminal::new_detached(cols, 2);
    terminal.feed_bytes(format!("{bytes}\r\n\r\n\r\n").as_bytes());
    let lines = terminal.scrollback_lines_all();
    let line = lines.first().expect("a scrollback line");
    let mut col = 0;
    line.cells()
        .map(|(text, attrs)| {
            let width = unicode_width(text.chars().next().unwrap_or(' '));
            let cell = (col, width, attrs.clone());
            col += width;
            cell
        })
        .collect()
}

#[derive(Default)]
struct Emitted {
    bg: Vec<BgInstance>,
    over: Vec<GlyphInstance>,
}

impl Emitted {
    /// 글리프 아래 선의 (열, 위쪽, 높이, 색).
    fn under_lines(&self) -> Vec<(usize, f32, f32, GpuRgba)> {
        self.bg
            .iter()
            .filter(|b| b.line != BgInstance::WHOLE_CELL)
            .map(|b| (b.pos[0] as usize, b.line[0], b.line[1], b.bg_color))
            .collect()
    }
    fn whole_cells(&self) -> usize {
        self.bg
            .iter()
            .filter(|b| b.line == BgInstance::WHOLE_CELL)
            .count()
    }
}

/// 두 경로가 칸마다 하는 일: 칠을 정하고 바탕, 아래 선, 위 선을 낸다.
fn emit(
    cells: &[(usize, usize, CellAttributes)],
    cols: usize,
    overlays: &CellOverlays<'_>,
    geometry: &LineGeometry,
) -> Emitted {
    let mut out = Emitted::default();
    for (col, width, attrs) in cells {
        let paint = paint_cell(*col, 0, attrs, bed(), cell_ink(), &ansi(), None, overlays);
        let place = CellPlace {
            columns: covered_columns(*col, *width, cols),
            row: 0,
            absolute_row: 0,
        };
        let (lines, _) = fill_cell(
            &mut out.bg,
            &place,
            attrs,
            &paint,
            overlays.preedit,
            geometry,
            [0.0, 0.0],
        );
        push_over_lines(
            &mut out.over,
            place.columns,
            0,
            [0.0, 0.0],
            CELL_WIDTH,
            SOLID,
            &lines,
        );
    }
    out
}

fn under_columns(out: &Emitted) -> Vec<usize> {
    out.under_lines().iter().map(|l| l.0).collect()
}

#[test]
fn an_underlined_space_still_gets_a_decoration() {
    let out = emit(
        &screen_cells("\x1b[4ma b\x1b[0m", 10),
        10,
        &no_overlays(),
        &geometry(true),
    );
    assert_eq!(under_columns(&out), vec![0, 1, 2]);
    let (_, top, height, color) = out.under_lines()[1];
    assert_eq!((top, height), (UNDER.top.value(), UNDER.height.value()));
    assert_eq!(color, cell_ink(), "선은 칸의 글자색이다");
}

#[test]
fn a_wide_glyph_decoration_spans_two_cells() {
    let out = emit(
        &screen_cells("\x1b[4m한\x1b[0m", 10),
        10,
        &no_overlays(),
        &geometry(true),
    );
    assert_eq!(under_columns(&out), vec![0, 1]);
    let mut filled: Vec<usize> = out
        .bg
        .iter()
        .filter(|b| b.line == BgInstance::WHOLE_CELL)
        .map(|b| b.pos[0] as usize)
        .collect();
    filled.sort_unstable();
    assert_eq!(filled, (0..10).collect::<Vec<_>>(), "열마다 바탕 하나");
}

#[test]
fn a_wide_glyph_at_the_row_end_is_cut_to_the_last_column() {
    let attrs = screen_cells("\x1b[4m한", 10)[0].2.clone();
    let cols = 5;
    let out = emit(&[(4, 2, attrs)], cols, &no_overlays(), &geometry(true));
    assert_eq!(covered_columns(4, 2, cols), 4..5);
    assert_eq!(under_columns(&out), vec![4]);
}

#[test]
fn a_preedit_cell_gets_no_decoration() {
    let preedit = RenderPreedit {
        text: "가".to_string(),
        anchor: SelectionPoint {
            epoch: tasty_terminal::ContentEpoch::default(),
            col: 0,
            absolute_row: 0,
        },
        bg_color: rgba(0.3),
        fg_color: rgba(0.9),
    };
    let overlays = CellOverlays {
        preedit: Some(&preedit),
        ..no_overlays()
    };
    let out = emit(
        &screen_cells("\x1b[4;9mabc\x1b[0m", 10),
        10,
        &overlays,
        &geometry(true),
    );
    assert_eq!(under_columns(&out), vec![2], "preedit가 0, 1열을 덮는다");
    assert_eq!(out.over.len(), 1);
    assert_eq!(out.over[0].pos[0], 2.0);
}

/// 조합 중에 위로 스크롤해도 preedit 덮개가 그려지므로, 덮인 칸은 원래 글리프와 선을 내지 않는다.
/// 칸마다 (글자, preedit가 덮는지, 선이 없는지)를 본다.
#[test]
fn a_preedit_cell_in_a_scrolled_view_draws_no_glyph_or_line() {
    let preedit = RenderPreedit {
        text: "가".to_string(),
        anchor: SelectionPoint {
            epoch: tasty_terminal::ContentEpoch::default(),
            col: 0,
            absolute_row: 0,
        },
        bg_color: rgba(0.3),
        fg_color: rgba(0.9),
    };
    let overlays = CellOverlays {
        link: Some(&link(rgba(0.7))),
        preedit: Some(&preedit),
        ..no_overlays()
    };
    let mut terminal = Terminal::new_detached(10, 2);
    terminal.feed_bytes(b"abc\r\n\r\n\r\n");
    let lines = terminal.scrollback_lines_all();
    let mut cells = Vec::new();
    for (col, (text, attrs)) in lines
        .first()
        .expect("a scrollback line")
        .cells()
        .take(3)
        .enumerate()
    {
        let place = CellPlace {
            columns: covered_columns(col, 1, 10),
            row: 0,
            absolute_row: 0,
        };
        let (paint, preedit) =
            scrolled_cell_paint(&place, attrs, bed(), cell_ink(), &ansi(), &overlays);
        let mut bg = Vec::new();
        let (lines, covered) = fill_cell(
            &mut bg,
            &place,
            attrs,
            &paint,
            preedit,
            &geometry(true),
            [0.0, 0.0],
        );
        // push_cell은 덮인 칸의 글리프를 내지 않는다.
        cells.push((text.to_string(), covered, lines.is_empty()));
    }
    assert_eq!(
        cells,
        vec![
            ("a".to_string(), true, true),
            ("b".to_string(), true, true),
            ("c".to_string(), false, true),
        ],
        "preedit가 0, 1열을 덮고 링크 밑줄도 그 칸에는 긋지 않는다"
    );
}

fn link(underline: GpuRgba) -> LinkHighlight {
    LinkHighlight {
        epoch: tasty_terminal::ContentEpoch::default(),
        segments: vec![LinkSegment {
            absolute_row: 0,
            start_col: 0,
            end_col: 1,
        }],
        fg: rgba(0.95),
        bg: rgba(0.35),
        underline,
    }
}

#[test]
fn a_link_cell_uses_the_same_decision() {
    let link = link(rgba(0.7));
    let overlays = CellOverlays {
        link: Some(&link),
        ..no_overlays()
    };
    // 값이 없는 제품 기하에서도 링크 밑줄은 나온다.
    let product = geometry(false);
    let out = emit(&screen_cells("ab c", 10), 10, &overlays, &product);
    let lines = out.under_lines();
    assert_eq!(under_columns(&out), vec![0, 1]);
    let span = product.link_underline;
    assert_eq!(
        lines[0],
        (0, span.top.value(), span.height.value(), rgba(0.7))
    );
}

#[test]
fn a_link_replaces_the_program_underline_but_keeps_the_strikethrough() {
    let link = link(rgba(0.7));
    let overlays = CellOverlays {
        link: Some(&link),
        ..no_overlays()
    };
    let out = emit(
        &screen_cells("\x1b[4;9ma\x1b[0m", 10),
        10,
        &overlays,
        &geometry(true),
    );
    let span = geometry(true).link_underline;
    let first: Vec<_> = out.under_lines().into_iter().filter(|l| l.0 == 0).collect();
    assert_eq!(
        first,
        vec![(0, span.top.value(), span.height.value(), rgba(0.7))],
        "한 칸에 밑줄 하나"
    );
    assert_eq!(out.over.len(), 1, "취소선은 남는다");
}

#[test]
fn without_values_program_lines_are_not_drawn() {
    for sgr in [
        "4", "21", "4:3", "4:4", "4:5", "9", "53", "4;58;5;9", "4;9;53",
    ] {
        let cells = screen_cells(&format!("\x1b[{sgr}ma b\x1b[0m"), 10);
        let out = emit(&cells, 10, &no_overlays(), &geometry(false));
        assert!(out.under_lines().is_empty(), "{sgr}");
        assert!(out.over.is_empty(), "{sgr}");
        assert_eq!(out.whole_cells(), cells.len(), "{sgr}: 칸마다 바탕 하나");
    }
}

#[test]
fn a_strikethrough_and_an_overline_lie_over_the_glyph() {
    let out = emit(
        &screen_cells("\x1b[31;9;53m한\x1b[0m", 10),
        10,
        &no_overlays(),
        &geometry(true),
    );
    assert!(out.under_lines().is_empty());
    let red = compute_cell_colors(
        &screen_cells("\x1b[31mx", 10)[0].2,
        bed(),
        cell_ink(),
        &ansi(),
    )
    .1;
    let got: Vec<_> = out
        .over
        .iter()
        .map(|g| (g.pos[0], g.glyph_offset, g.glyph_size, g.fg_color))
        .collect();
    let strike = (
        [0.0, STRIKE.top.value()],
        [CELL_WIDTH.value(), STRIKE.height.value()],
    );
    let over = (
        [0.0, OVER.top.value()],
        [CELL_WIDTH.value(), OVER.height.value()],
    );
    assert_eq!(
        got,
        vec![
            (0.0, strike.0, strike.1, red),
            (0.0, over.0, over.1, red),
            (1.0, strike.0, strike.1, red),
            (1.0, over.0, over.1, red),
        ]
    );
    for g in &out.over {
        assert_eq!(
            (g.uv_offset, g.uv_size, g.page),
            (SOLID.uv, [0.0, 0.0], SOLID.page)
        );
    }
}

#[test]
fn a_default_cell_draws_no_line() {
    let paint = paint_cell(
        0,
        0,
        &CellAttributes::default(),
        bed(),
        cell_ink(),
        &ansi(),
        None,
        &no_overlays(),
    );
    let lines = lines_for_cell(&CellAttributes::default(), &paint, false, &geometry(true));
    assert_eq!(lines.iter().count(), 0);
}

#[test]
fn the_screen_and_the_scrollback_draw_the_same_lines() {
    let row = "\x1b[4ma b\x1b[9m한\x1b[53mz\x1b[0m";
    let screen = emit(&screen_cells(row, 12), 12, &no_overlays(), &geometry(true));
    let history = emit(
        &scrollback_cells(row, 12),
        12,
        &no_overlays(),
        &geometry(true),
    );
    assert_eq!(under_columns(&screen), vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(screen.under_lines(), history.under_lines());
    let over = |e: &Emitted| -> Vec<_> {
        e.over
            .iter()
            .map(|g| (g.pos, g.glyph_offset, g.glyph_size, g.fg_color))
            .collect()
    };
    assert_eq!(over(&screen).len(), 4, "한 두 칸 취소선, z 취소선과 윗줄");
    assert_eq!(over(&screen), over(&history));
}

/// `A\e[8mSECRET\e[28mB\e[41;8mX` 줄의 칸마다 글리프를 내는지.
fn glyph_columns(bytes: &str) -> Vec<(String, bool)> {
    let mut terminal = Terminal::new_detached(20, 3);
    terminal.feed_bytes(bytes.as_bytes());
    terminal.with_content(|view| {
        view.surface().screen_lines()[0]
            .visible_cells()
            .take(9)
            .map(|cell| {
                let text = cell.str().to_string();
                let draws = draws_glyph(&text, cell.attrs(), false);
                (text, draws)
            })
            .collect()
    })
}

#[test]
fn a_hidden_cell_draws_no_glyph_and_sgr_28_shows_the_next_one() {
    let got = glyph_columns("A\x1b[8mSECRET\x1b[28mB\x1b[41;8mX\x1b[0m");
    let shown: String = got
        .iter()
        .map(|(text, draws)| if *draws { text.as_str() } else { "." })
        .collect();
    assert_eq!(shown, "A......B.");
    let texts: String = got.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(texts, "ASECRETBX", "셀 텍스트는 그대로다");
}

#[test]
fn a_hidden_cell_keeps_its_background_and_width() {
    let cells = screen_cells("\x1b[41;8m한X\x1b[0m", 10);
    let out = emit(&cells, 10, &no_overlays(), &geometry(false));
    let shown = emit(
        &screen_cells("\x1b[41m한X\x1b[0m", 10),
        10,
        &no_overlays(),
        &geometry(false),
    );
    assert_eq!(cells[0].1, 2, "넓은 글자는 숨겨도 두 칸이다");
    let bg = |e: &Emitted| -> Vec<_> { e.bg.iter().map(|b| (b.pos, b.bg_color)).collect() };
    assert_eq!(bg(&out), bg(&shown));
}

#[test]
fn a_hidden_cell_draws_no_program_line_but_keeps_the_link_underline() {
    let hidden = emit(
        &screen_cells("\x1b[4;9;53;8mab\x1b[0m", 10),
        10,
        &no_overlays(),
        &geometry(true),
    );
    assert!(hidden.under_lines().is_empty());
    assert!(hidden.over.is_empty());
    let link = link(rgba(0.7));
    let overlays = CellOverlays {
        link: Some(&link),
        ..no_overlays()
    };
    let linked = emit(
        &screen_cells("\x1b[8mab\x1b[0m", 10),
        10,
        &overlays,
        &geometry(true),
    );
    assert_eq!(under_columns(&linked), vec![0, 1]);
}

#[test]
fn a_blank_or_preedit_cell_draws_no_glyph() {
    let attrs = CellAttributes::default();
    assert!(draws_glyph("a", &attrs, false));
    assert!(!draws_glyph(" ", &attrs, false));
    assert!(!draws_glyph("", &attrs, false));
    assert!(!draws_glyph("a", &attrs, true));
}
