//! 칸이 긋는 선(포인터 아래 링크의 밑줄, 프로그램의 밑줄·취소선·윗줄)을 고르고 인스턴스로 낸다.
//! 현재 화면과 스크롤한 위치가 같은 판단을 쓴다. 정본은 디자인 RULES의 "Lines a cell draws"다.
//! 프로그램 선의 기하는 인자로 받고, 없으면 프로그램 선을 내지 않는다.

use std::ops::Range;

use tasty_type_appearance::color::GpuRgba;
use tasty_type_geometry::length::PhysicalPx;
use termwiz::cell::{CellAttributes, Underline};

use super::RenderPreedit;
use super::cell_lines;
use super::cell_paint::CellPaint;
use super::types::{BgInstance, GlyphInstance};

/// 선이 놓이는 층. 밑줄은 글리프 아래 bg 패스, 취소선과 윗줄은 글리프 위 글리프 패스다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LineLayer {
    UnderGlyph,
    OverGlyph,
}

/// 칸 위쪽에서 잰 선의 위쪽 가장자리와 높이. 물리 픽셀이다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct LineSpan {
    pub top: PhysicalPx,
    pub height: PhysicalPx,
}

/// 프로그램 선(SGR 4, 9, 53)의 기하. 밑줄 종류별 모양은 아직 나누지 않는다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct ProgramLines {
    pub underline: LineSpan,
    pub strikethrough: LineSpan,
    pub overline: LineSpan,
}

/// viewport 하나에 쓰는 선의 기하. 칸마다 같으므로 한 번 만든다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct LineGeometry {
    pub link_underline: LineSpan,
    pub program: Option<ProgramLines>,
}

impl LineGeometry {
    /// 링크 밑줄은 셀 높이와 격자 기준선에서 정한다. 프로그램 선은 호출자가 넘긴다.
    pub(super) fn new(
        cell_height: PhysicalPx,
        baseline: PhysicalPx,
        program: Option<ProgramLines>,
    ) -> Self {
        // 선 기하 함수(`cell_lines`)는 정수 행으로 계산한다.
        let height = cell_height.value() as u32;
        let thickness = cell_lines::line_thickness(height);
        let top = cell_lines::underline_top(
            cell_lines::row_under_baseline(baseline.value()),
            thickness,
            height,
            thickness,
        );
        Self {
            link_underline: LineSpan {
                top: PhysicalPx(top as f32),
                height: PhysicalPx(thickness as f32),
            },
            program,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct CellLine {
    pub layer: LineLayer,
    pub span: LineSpan,
    pub color: GpuRgba,
}

/// 한 칸의 선. 밑줄, 취소선, 윗줄을 넘지 않아 할당 없이 담는다.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) struct CellLines {
    lines: [Option<CellLine>; 3],
}

impl CellLines {
    fn push(&mut self, layer: LineLayer, span: LineSpan, color: GpuRgba) {
        if let Some(slot) = self.lines.iter_mut().find(|l| l.is_none()) {
            *slot = Some(CellLine { layer, span, color });
        }
    }

    /// 선은 앞 칸부터 채우므로 첫 칸만 본다.
    pub(super) fn is_empty(&self) -> bool {
        self.lines[0].is_none()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &CellLine> {
        self.lines.iter().flatten()
    }

    pub(super) fn on(&self, layer: LineLayer) -> impl Iterator<Item = &CellLine> {
        self.iter().filter(move |l| l.layer == layer)
    }
}

/// 칸의 속성과 칠에서 선 목록을 낸다. 글자가 없는 칸도 선을 긋는다.
/// preedit가 덮는 칸은 긋지 않는다. 그 위에 preedit 바탕과 글자가 놓인다.
/// 포인터 아래 링크의 밑줄은 프로그램 밑줄 자리를 대신한다. 색은 칠이 정한 링크 밑줄 색이다.
/// 프로그램 선의 색은 칸의 최종 글자색이다.
pub(super) fn lines_for_cell(
    attrs: &CellAttributes,
    paint: &CellPaint,
    covered_by_preedit: bool,
    geometry: &LineGeometry,
) -> CellLines {
    let mut lines = CellLines::default();
    // 선이 없는 칸이 대부분이라 판단 없이 먼저 끝낸다.
    if covered_by_preedit || (paint.underline.is_none() && geometry.program.is_none()) {
        return lines;
    }
    // 숨긴 칸(SGR 8)은 프로그램 선을 긋지 않는다. 링크 밑줄은 프로그램 선이 아니다.
    let program = geometry.program.as_ref().filter(|_| !attrs.invisible());
    if let Some(color) = paint.underline {
        lines.push(LineLayer::UnderGlyph, geometry.link_underline, color);
    } else if let Some(p) = program
        && attrs.underline() != Underline::None
    {
        lines.push(LineLayer::UnderGlyph, p.underline, paint.fg);
    }
    if let Some(p) = program {
        if attrs.strikethrough() {
            lines.push(LineLayer::OverGlyph, p.strikethrough, paint.fg);
        }
        if attrs.overline() {
            lines.push(LineLayer::OverGlyph, p.overline, paint.fg);
        }
    }
    lines
}

/// 칸이 글리프를 내는지. 공백, preedit가 덮는 칸, 숨긴 칸(SGR 8)은 내지 않는다.
/// 숨긴 칸도 바탕과 칸 폭은 그대로이고 셀 텍스트는 복사와 검색에 남는다.
pub(super) fn draws_glyph(text: &str, attrs: &CellAttributes, covered_by_preedit: bool) -> bool {
    !(text.is_empty() || text == " " || covered_by_preedit || attrs.invisible())
}

/// 칸이 덮는 열. 넓은 글자는 두 칸이고 줄 끝에서 잘린다.
pub(super) fn covered_columns(col: usize, width: usize, cols: usize) -> Range<usize> {
    col..(col + width).min(cols)
}

/// 칸이 놓이는 곳. `columns`는 덮는 열이고 넓은 글자는 두 열이다.
pub(super) struct CellPlace {
    pub columns: Range<usize>,
    pub row: usize,
    pub absolute_row: usize,
}

/// 덮는 열마다 바탕과 글리프 아래 선을 bg 인스턴스로 낸다. 선 판단은 공백 칸 검사 앞이다.
/// 칸의 선 목록과 preedit가 이 칸을 덮는지를 돌려준다. 넓은 글자는 왼쪽 칸 기준으로 판단한다.
pub(super) fn fill_cell(
    out: &mut Vec<BgInstance>,
    place: &CellPlace,
    attrs: &CellAttributes,
    paint: &CellPaint,
    preedit: Option<&RenderPreedit>,
    geometry: &LineGeometry,
    viewport_offset: [f32; 2],
) -> (CellLines, bool) {
    let covered_by_preedit =
        preedit.is_some_and(|p| p.covers(place.columns.start, place.absolute_row));
    let lines = lines_for_cell(attrs, paint, covered_by_preedit, geometry);
    for col in place.columns.clone() {
        out.push(BgInstance {
            pos: [col as f32, place.row as f32],
            viewport_offset,
            bg_color: paint.bg,
            line: BgInstance::WHOLE_CELL,
        });
        if lines.is_empty() {
            continue;
        }
        for line in lines.on(LineLayer::UnderGlyph) {
            out.push(BgInstance {
                pos: [col as f32, place.row as f32],
                viewport_offset,
                bg_color: line.color,
                line: [line.span.top.value(), line.span.height.value()],
            });
        }
    }
    (lines, covered_by_preedit)
}

/// 글리프 패스에서 불투명하게 읽히는 아틀라스 한 점. 꽉 찬 블록 글리프의 가운데다.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct SolidTexel {
    pub uv: [f32; 2],
    pub page: u32,
}

/// 덮는 열마다 글리프 위 선을 글리프 인스턴스로 낸다. uv 크기 0이라 모든 픽셀이 한 점을 읽는다.
pub(super) fn push_over_lines(
    out: &mut Vec<GlyphInstance>,
    columns: Range<usize>,
    row: usize,
    viewport_offset: [f32; 2],
    cell_width: PhysicalPx,
    solid: SolidTexel,
    lines: &CellLines,
) {
    for col in columns {
        for line in lines.on(LineLayer::OverGlyph) {
            out.push(GlyphInstance {
                pos: [col as f32, row as f32],
                viewport_offset,
                uv_offset: solid.uv,
                uv_size: [0.0, 0.0],
                fg_color: line.color,
                glyph_offset: [0.0, line.span.top.value()],
                glyph_size: [cell_width.value(), line.span.height.value()],
                page: solid.page,
                _pad: 0,
            });
        }
    }
}
