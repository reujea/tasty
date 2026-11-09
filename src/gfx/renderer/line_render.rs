use tasty_terminal::ScrollbackLine;
use tasty_type_appearance::color::{GpuRgb, GpuRgba};
use termwiz::cell::CellAttributes;

use tasty_font::GlyphKey;

use super::CellRenderer;
use super::cell_paint::{CellOverlays, CellPaint, paint_cell};
use super::overlay::composite_over;
use super::types::{BgInstance, GlyphInstance};
use tasty_cell_width::unicode_width;

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
        let CellPaint {
            bg: bg_color,
            fg: fg_color,
            underline,
        } = paint_cell(
            col_idx,
            absolute_row,
            attrs,
            default_bg,
            default_fg,
            ansi,
            None,
            &overlays,
        );

        let off = self.current_viewport_offset;

        // Push bg for main cell and continuation cells of wide characters
        for i in 0..width {
            if col_idx + i < cols {
                self.bg_instances.push(BgInstance {
                    pos: [(col_idx + i) as f32, row_idx as f32],
                    viewport_offset: off,
                    bg_color,
                    line: BgInstance::WHOLE_CELL,
                });
                if let Some(color) = underline {
                    self.push_link_underline(col_idx + i, row_idx, color);
                }
            }
        }

        if text.is_empty() || text == " " {
            return;
        }

        let ch = text.chars().next().unwrap();
        let bold = attrs.intensity() == termwiz::cell::Intensity::Bold;
        let italic = attrs.italic();

        let key = GlyphKey { ch, bold, italic };

        if let Some(entry) = self.atlas.get_or_insert(key, &mut self.font_config, queue)
            && entry.width > 0.0
            && entry.height > 0.0
        {
            self.glyph_instances.push(GlyphInstance {
                pos: [col_idx as f32, row_idx as f32],
                viewport_offset: off,
                uv_offset: [entry.uv_x, entry.uv_y],
                uv_size: [entry.uv_w, entry.uv_h],
                fg_color,
                glyph_offset: [entry.offset_x, entry.offset_y],
                glyph_size: [entry.width, entry.height],
                page: entry.page,
                _pad: 0,
            });
        }
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
