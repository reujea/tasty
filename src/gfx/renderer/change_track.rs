//! 직전 프레임과 비교해 프레임마다 바뀐 surface·칸·인스턴스 바이트 수를 센다.
//! perf 계측이 켜진 프레임에서만 칸 서명과 인스턴스 사본을 만든다. 꺼진 프레임은 보관한 사본도 버린다.
//! 칸 비교는 인스턴스가 아니라 칸 좌표별 (문자열, 속성, 적용된 색) 서명으로 한다. 칸 하나가 내는
//! 인스턴스 수는 글리프·선·커서에 따라 달라지고, 글리프 인스턴스의 atlas 좌표는 내용이 같아도
//! eviction으로 바뀌기 때문이다. 인스턴스 바이트 차이는 업로드를 줄일 수 있는 양을 따로 보여 준다.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use tasty_cell_width::unicode_width;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;
use termwiz::surface::Surface;

use super::cell_decor::covered_columns;
use super::cell_paint::{CellOverlays, cursor_paint_at, paint_cell};
use super::overlay::composite_over;
use super::{CursorPaint, RenderPreedit};

/// 한 프레임에서 직전 프레임과 달라진 양.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameChanges {
    /// 터미널 내용 revision이 바뀌었거나 직전 프레임에 그리지 않은 surface 수.
    pub surfaces: u32,
    /// 서명이 달라진 칸 수. 격자 크기가 바뀌었거나 새로 그린 surface는 모든 칸을 센다.
    pub cells: u32,
    /// 같은 순번의 인스턴스와 바이트가 다른 인스턴스의 바이트 합. 길이가 늘거나 줄어든 몫도 더한다.
    pub instance_bytes: u64,
}

/// surface 하나의 한 프레임 기록.
struct SurfaceFrame {
    revision: u64,
    cols: usize,
    rows: usize,
    cells: Vec<u64>,
    bg_bytes: Vec<u8>,
    glyph_bytes: Vec<u8>,
}

/// 렌더러가 칸을 낼 때마다 서명을 받는 추적기.
#[derive(Default)]
pub(crate) struct ChangeTracker {
    enabled: bool,
    current: Option<(u32, SurfaceFrame)>,
    previous: HashMap<u32, SurfaceFrame>,
    next: HashMap<u32, SurfaceFrame>,
    frame: FrameChanges,
}

impl ChangeTracker {
    /// 프레임을 시작한다. 꺼져 있으면 직전 프레임 사본을 버리고 이 프레임에서 아무것도 보관하지 않는다.
    pub(crate) fn begin_frame(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.current = None;
        self.next.clear();
        self.frame = FrameChanges::default();
        if !enabled {
            self.previous = HashMap::new();
            self.next = HashMap::new();
        }
    }

    /// 이 surface의 칸 서명을 받기 시작한다. 꺼진 프레임에서는 아무것도 하지 않는다.
    pub(crate) fn begin_surface(&mut self, id: u32, revision: u64, cols: usize, rows: usize) {
        if !self.enabled {
            return;
        }
        self.current = Some((
            id,
            SurfaceFrame {
                revision,
                cols,
                rows,
                cells: vec![0; cols.saturating_mul(rows)],
                bg_bytes: Vec::new(),
                glyph_bytes: Vec::new(),
            },
        ));
    }

    /// 칸 서명을 받는 중인가. 서명 계산 자체를 건너뛰는 데 쓴다.
    #[inline]
    pub(crate) fn recording(&self) -> bool {
        self.current.is_some()
    }

    /// `row` 줄의 `columns` 칸에 서명을 더한다. 한 칸에 여러 층(글자, 커서 덮개, 조합 중 글자)이
    /// 겹치면 순서대로 섞는다.
    pub(crate) fn note(&mut self, columns: Range<usize>, row: usize, signature: u64) {
        let Some((_, frame)) = self.current.as_mut() else {
            return;
        };
        if row >= frame.rows {
            return;
        }
        let end = columns.end.min(frame.cols);
        for col in columns.start..end {
            let slot = &mut frame.cells[row * frame.cols + col];
            *slot = slot.rotate_left(17) ^ signature;
        }
    }

    /// surface 하나를 끝내고 직전 프레임과 비교한다. `bg`·`glyph`는 이 surface가 낸 인스턴스 바이트다.
    pub(crate) fn end_surface(&mut self, bg: &[u8], glyph: &[u8], instance_sizes: (usize, usize)) {
        let Some((id, mut frame)) = self.current.take() else {
            return;
        };
        frame.bg_bytes.extend_from_slice(bg);
        frame.glyph_bytes.extend_from_slice(glyph);
        let changes = compare(self.previous.get(&id), &frame, instance_sizes);
        self.frame.surfaces += changes.surfaces;
        self.frame.cells = self.frame.cells.saturating_add(changes.cells);
        self.frame.instance_bytes = self
            .frame
            .instance_bytes
            .saturating_add(changes.instance_bytes);
        self.next.insert(id, frame);
    }

    /// 프레임을 끝낸다. 이번에 그린 surface만 다음 비교 대상으로 남긴다. 꺼진 프레임은 `None`이다.
    pub(crate) fn finish_frame(&mut self) -> Option<FrameChanges> {
        self.current = None;
        if !self.enabled {
            return None;
        }
        std::mem::swap(&mut self.previous, &mut self.next);
        self.next.clear();
        Some(self.frame)
    }

    /// 보관 중인 직전 프레임 surface 수. 꺼진 상태에서 사본이 없는지 시험이 확인한다.
    #[cfg(test)]
    pub(crate) fn retained_surfaces(&self) -> usize {
        self.previous.len() + self.next.len() + usize::from(self.current.is_some())
    }
}

fn compare(
    previous: Option<&SurfaceFrame>,
    frame: &SurfaceFrame,
    (bg_size, glyph_size): (usize, usize),
) -> FrameChanges {
    let total_cells = u32::try_from(frame.cells.len()).unwrap_or(u32::MAX);
    let Some(prev) = previous else {
        return FrameChanges {
            surfaces: 1,
            cells: total_cells,
            instance_bytes: (frame.bg_bytes.len() + frame.glyph_bytes.len()) as u64,
        };
    };
    let cells = if prev.cols == frame.cols && prev.rows == frame.rows {
        let changed = prev
            .cells
            .iter()
            .zip(&frame.cells)
            .filter(|(a, b)| a != b)
            .count();
        u32::try_from(changed).unwrap_or(u32::MAX)
    } else {
        total_cells
    };
    FrameChanges {
        surfaces: u32::from(prev.revision != frame.revision),
        cells,
        instance_bytes: changed_bytes(&prev.bg_bytes, &frame.bg_bytes, bg_size)
            + changed_bytes(&prev.glyph_bytes, &frame.glyph_bytes, glyph_size),
    }
}

/// 같은 순번 인스턴스끼리 비교해 다른 인스턴스의 바이트와 길이 차이를 더한다.
fn changed_bytes(prev: &[u8], cur: &[u8], instance_size: usize) -> u64 {
    if instance_size == 0 {
        return 0;
    }
    let differing = prev
        .chunks_exact(instance_size)
        .zip(cur.chunks_exact(instance_size))
        .filter(|(a, b)| a != b)
        .count();
    (differing * instance_size + prev.len().abs_diff(cur.len())) as u64
}

/// 글자가 있는 칸의 서명. 칠한 색은 선택·검색·커서 같은 덮개를 이미 반영한다.
pub(crate) fn cell_signature(
    text: &str,
    attrs: &CellAttributes,
    bg: GpuRgba,
    fg: GpuRgba,
    underline: Option<GpuRgba>,
) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    text.hash(&mut h);
    (attrs.intensity() as u8).hash(&mut h);
    (attrs.underline() as u8).hash(&mut h);
    (attrs.blink() as u8).hash(&mut h);
    attrs.italic().hash(&mut h);
    attrs.strikethrough().hash(&mut h);
    attrs.overline().hash(&mut h);
    attrs.invisible().hash(&mut h);
    attrs.reverse().hash(&mut h);
    attrs.underline_color().hash(&mut h);
    hash_color(&mut h, bg);
    hash_color(&mut h, fg);
    underline.is_some().hash(&mut h);
    if let Some(color) = underline {
        hash_color(&mut h, color);
    }
    h.finish()
}

/// 글자 없이 바탕만 칠한 칸이나 덮개 글자의 서명.
pub(crate) fn overlay_signature(ch: Option<char>, bg: Option<GpuRgba>, fg: Option<GpuRgba>) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    ch.hash(&mut h);
    for color in [bg, fg] {
        color.is_some().hash(&mut h);
        if let Some(color) = color {
            hash_color(&mut h, color);
        }
    }
    h.finish()
}

fn hash_color(h: &mut impl Hasher, color: GpuRgba) {
    for channel in color.as_array() {
        channel.to_bits().hash(h);
    }
}

/// 칸 서명을 계산할 viewport 값. 렌더러가 칠할 때와 같은 값을 넘긴다.
#[derive(Clone, Copy)]
pub(super) struct SignInput<'a> {
    pub default_bg: GpuRgba,
    pub default_fg: GpuRgba,
    pub ansi: &'a [GpuRgb; 16],
    /// 현재 화면에서만 있다. 스크롤한 위치는 커서를 그리지 않는다.
    pub cursor: Option<(usize, usize, bool, CursorPaint)>,
    pub overlays: CellOverlays<'a>,
    pub cols: usize,
    pub rows: usize,
}

/// 현재 화면 전체의 칸 서명을 낸다. 렌더러의 현재 화면 경로와 같은 칠 함수를 쓴다.
pub(super) fn sign_live_screen(
    tracker: &mut ChangeTracker,
    input: &SignInput<'_>,
    surface: &Surface,
    row_offset: usize,
) {
    for (row, line) in surface.screen_lines().iter().enumerate().take(input.rows) {
        let mut signer = RowSigner::new(tracker, input, row, row_offset + row);
        for cell in line.visible_cells() {
            if !signer.cell(cell.cell_index(), cell.str(), cell.attrs()) {
                break;
            }
        }
        signer.finish();
    }
}

/// 한 줄의 칸 서명을 낸다. 글자가 없는 뒤쪽 칸은 [`RowSigner::finish`]가 렌더러처럼 바탕 색만 센다.
pub(super) struct RowSigner<'t, 'i> {
    tracker: &'t mut ChangeTracker,
    input: &'i SignInput<'i>,
    row: usize,
    absolute_row: usize,
    last_col: usize,
}

impl<'t, 'i> RowSigner<'t, 'i> {
    pub(super) fn new(
        tracker: &'t mut ChangeTracker,
        input: &'i SignInput<'i>,
        row: usize,
        absolute_row: usize,
    ) -> Self {
        Self {
            tracker,
            input,
            row,
            absolute_row,
            last_col: 0,
        }
    }

    /// 글자가 있는 칸 하나. 격자 밖이면 거짓이고 호출자는 줄을 끝낸다.
    pub(super) fn cell(&mut self, col: usize, text: &str, attrs: &CellAttributes) -> bool {
        let input = self.input;
        if col >= input.cols {
            return false;
        }
        let paint = paint_cell(
            col,
            self.absolute_row,
            attrs,
            input.default_bg,
            input.default_fg,
            input.ansi,
            cursor_paint_at(input.cursor, col, self.row),
            &input.overlays,
        );
        let wide = text.chars().next().is_some_and(|ch| unicode_width(ch) > 1);
        let columns = covered_columns(col, if wide { 2 } else { 1 }, input.cols);
        self.last_col = columns.end;
        let signature = cell_signature(text, attrs, paint.bg, paint.fg, paint.underline);
        self.tracker.note(columns, self.row, signature);
        true
    }

    /// 마지막 글자 뒤의 빈 칸을 바탕 색으로 센다.
    pub(super) fn finish(self) {
        let input = self.input;
        let row = self.row;
        for col in self.last_col..input.cols {
            let block_cursor = matches!(
                input.cursor,
                Some((cx, cy, _, CursorPaint::Block)) if row == cy && col == cx
            );
            let mut bg = if block_cursor {
                input.default_fg
            } else {
                input.default_bg
            };
            if let Some((pt, cursor_bg, _)) = input.overlays.vi_cursor
                && pt.col == col
                && pt.absolute_row == self.absolute_row
            {
                bg = composite_over(*cursor_bg, bg);
            }
            self.tracker
                .note(col..col + 1, row, overlay_signature(None, Some(bg), None));
        }
    }
}

/// 칸 위에 따로 그리는 막대·밑줄 커서와 조합 중 글자의 서명을 더한다.
pub(super) fn sign_overlays(
    tracker: &mut ChangeTracker,
    input: &SignInput<'_>,
    preedit: Option<&RenderPreedit>,
    top_row: usize,
) {
    if let Some((cx, cy, wide, shape)) = input.cursor
        && let Some(glyph) = shape.overlay_glyph()
    {
        let span = if shape == CursorPaint::Underline && wide {
            2
        } else {
            1
        };
        let signature = overlay_signature(Some(glyph), None, Some(input.default_fg));
        tracker.note(cx..cx + span, cy, signature);
    }
    let Some(preedit) = preedit else {
        return;
    };
    let Some(row) = preedit.anchor.absolute_row.checked_sub(top_row) else {
        return;
    };
    let mut col = preedit.anchor.col;
    for ch in preedit.text.chars() {
        let width = unicode_width(ch);
        let signature = overlay_signature(Some(ch), Some(preedit.bg_color), Some(preedit.fg_color));
        tracker.note(col..col + width.max(1), row, signature);
        col += width;
    }
}
