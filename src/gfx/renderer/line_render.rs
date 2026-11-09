use tasty_terminal::ScrollbackLine;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;

use super::CellRenderer;
use super::RenderPreedit;
use super::cell_decor::{CellPlace, covered_columns};
use super::cell_paint::{CellOverlays, CellPaint, paint_cell};
use super::overlay::composite_over;
use super::types::BgInstance;
use tasty_cell_width::unicode_width;

/// 스크롤한 위치의 칸 칠과, 칸이 덮이는지 판단할 preedit를 정한다. GPU 자원 없이 판단만 한다.
pub(super) fn scrolled_cell_paint<'a>(
    place: &CellPlace,
    attrs: &CellAttributes,
    default_bg: GpuRgba,
    default_fg: GpuRgba,
    ansi: &[GpuRgb; 16],
    overlays: &CellOverlays<'a>,
) -> (CellPaint, Option<&'a RenderPreedit>) {
    let paint = paint_cell(
        place.columns.start,
        place.absolute_row,
        attrs,
        default_bg,
        default_fg,
        ansi,
        None,
        overlays,
    );
    // 스크롤한 위치는 preedit 덮개를 보지 않는다. 덮인 칸도 원래 글리프와 선을 낸다.
    (paint, None)
}

impl CellRenderer {
    /// Render a single cell into instance buffers (shared logic for both line types).
    #[allow(clippy::too_many_arguments)]
    fn render_cell(
        &mut self,
        col_idx: usize,
        row_idx: usize,
        text: &str,
        attrs: &CellAttributes,
        width: usize,
        cols: usize,
        default_bg: GpuRgba,
        default_fg: GpuRgba,
        ansi: &[GpuRgb; 16],
        queue: &wgpu::Queue,
        absolute_row: usize,
        overlays: CellOverlays<'_>,
    ) {
        let place = CellPlace {
            columns: covered_columns(col_idx, width, cols),
            row: row_idx,
            absolute_row,
        };
        let (paint, preedit) =
            scrolled_cell_paint(&place, attrs, default_bg, default_fg, ansi, &overlays);
        let overlays = CellOverlays {
            preedit,
            ..overlays
        };
        self.push_cell(place, text, attrs, paint, &overlays, queue);
    }

    /// Render a single scrollback line (compact `ScrollbackLine`).
    /// Cells are streamed via the borrowing `cells()` iterator — no per-cell
    /// allocation. Fills remaining columns with default_bg.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_scrollback_line(
        &mut self,
        line: &ScrollbackLine,
        row_idx: usize,
        cols: usize,
        default_bg: GpuRgba,
        default_fg: GpuRgba,
        ansi: &[GpuRgb; 16],
        queue: &wgpu::Queue,
        absolute_row: usize,
        overlays: CellOverlays<'_>,
    ) {
        let mut col_idx: usize = 0;
        for (text, attrs) in line.cells() {
            if col_idx >= cols {
                break;
            }
            let ch = text.chars().next().unwrap_or(' ');
            let width = unicode_width(ch);
            self.render_cell(
                col_idx,
                row_idx,
                text,
                attrs,
                width,
                cols,
                default_bg,
                default_fg,
                ansi,
                queue,
                absolute_row,
                overlays,
            );
            col_idx += width;
        }
        // Fill remaining columns with default_bg (vi cursor 가 trailing 영역에 있으면 강조).
        let off = self.current_viewport_offset;
        for c in col_idx..cols {
            let bg_color = overlays
                .vi_cursor
                .filter(|(pt, ..)| pt.col == c && pt.absolute_row == absolute_row)
                .map(|(_, cursor_bg, _)| composite_over(*cursor_bg, default_bg))
                .unwrap_or(default_bg);
            self.bg_instances.push(BgInstance {
                pos: [c as f32, row_idx as f32],
                viewport_offset: off,
                bg_color,
                line: BgInstance::WHOLE_CELL,
            });
        }
    }

    /// Render a single surface line (from termwiz screen_lines).
    /// Fills remaining columns with default_bg.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_surface_line(
        &mut self,
        line: &termwiz::surface::line::Line,
        row_idx: usize,
        cols: usize,
        default_bg: GpuRgba,
        default_fg: GpuRgba,
        ansi: &[GpuRgb; 16],
        queue: &wgpu::Queue,
        absolute_row: usize,
        overlays: CellOverlays<'_>,
    ) {
        let mut last_col = 0usize;
        for cell_ref in line.visible_cells() {
            let col_idx = cell_ref.cell_index();
            if col_idx >= cols {
                break;
            }
            let text = cell_ref.str();
            let ch = text.chars().next().unwrap_or(' ');
            let width = if !text.is_empty() {
                unicode_width(ch)
            } else {
                1
            };
            self.render_cell(
                col_idx,
                row_idx,
                text,
                cell_ref.attrs(),
                width,
                cols,
                default_bg,
                default_fg,
                ansi,
                queue,
                absolute_row,
                overlays,
            );
            last_col = col_idx + width;
        }
        // Fill remaining columns with default_bg (vi cursor 가 trailing 영역에 있으면 강조).
        let off = self.current_viewport_offset;
        for c in last_col..cols {
            let bg_color = overlays
                .vi_cursor
                .filter(|(pt, ..)| pt.col == c && pt.absolute_row == absolute_row)
                .map(|(_, cursor_bg, _)| composite_over(*cursor_bg, default_bg))
                .unwrap_or(default_bg);
            self.bg_instances.push(BgInstance {
                pos: [c as f32, row_idx as f32],
                viewport_offset: off,
                bg_color,
                line: BgInstance::WHOLE_CELL,
            });
        }
    }
}
