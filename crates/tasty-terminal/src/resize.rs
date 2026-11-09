//! Resize 처리 — grid 크기 변경(`TerminalState`)이 먼저이고, OS 크기 알림은 `Pty`에
//! 예약한 뒤 다음 `process()`의 강제 flush 또는 throttle된 `flush_pty_resize` 호출에서
//! 적용한다 (docs/surfaces/terminal/index.md#vte-에뮬레이션).

use termwiz::cell::CellAttributes;
use termwiz::surface::Change;

use crate::{Terminal, TerminalState};

impl TerminalState {
    /// Resize the grid (surfaces, scroll region, line tails). Returns true if the
    /// dimensions actually changed (the caller then schedules a PTY notify).
    pub(crate) fn resize_grid(&mut self, cols: usize, rows: usize) -> bool {
        if self.cols == cols && self.rows == rows {
            return false;
        }

        self.content_revision = self.content_revision.wrapping_add(1);
        let old_cols = self.cols;
        let old_rows = self.rows;

        if cols != old_cols && !self.use_alternate {
            self.save_or_restore_line_tails(old_cols, cols, rows);
        }

        // Handle rows shrink BEFORE resize (need to capture lines before they're lost)
        if rows < old_rows && !self.use_alternate {
            self.handle_rows_shrink(rows, old_rows);
        }

        let old_cursor = self.primary_surface.cursor_position();

        self.cols = cols;
        self.rows = rows;
        // Column changes reset custom tab stops to the default grid.
        if cols != old_cols {
            self.tab_stops = crate::default_tab_stops(cols);
        }
        self.primary_surface.resize(cols, rows);
        if let Some(alt) = &mut self.alternate_surface {
            alt.resize(cols, rows);
        }

        // Handle rows grow AFTER resize (surface now has room for ScrollRegionDown)
        let mut rows_restored = 0usize;
        if rows > old_rows && !self.use_alternate {
            rows_restored = self.handle_rows_grow(rows, old_rows);
        }

        if cols > old_cols && !self.use_alternate {
            self.restore_tails_to_surface(old_cols, cols);
        }

        if !self.use_alternate {
            use termwiz::surface::Position;
            let cursor_y = (old_cursor.1 + rows_restored).min(rows.saturating_sub(1));
            let cursor_x = old_cursor.0.min(cols.saturating_sub(1));
            self.primary_surface.add_change(Change::CursorPosition {
                x: Position::Absolute(cursor_x),
                y: Position::Absolute(cursor_y),
            });
            // Restoring cells bypasses mirror_pen and leaves the surface pen at the last cell.
            // Restore the logical pen before processing subsequent text or SGR changes.
            self.primary_surface
                .add_change(Change::AllAttributes(self.current_pen.clone()));
        }

        self.scroll_region = None;
        // Notify resize subscribers. Their bounded queues can drop an update.
        self.fan_out_resize(cols, rows);
        self.fan_out_attach_resize(cols, rows);
        true
    }

    /// When rows shrink, capture top lines to scrollback so the cursor stays
    /// near the bottom, mimicking xterm/Alacritty behavior.
    pub(crate) fn handle_rows_shrink(&mut self, new_rows: usize, old_rows: usize) {
        let (_, cursor_y) = self.primary_surface.cursor_position();
        let rows_to_remove = old_rows - new_rows;

        let lines = self.primary_surface.screen_lines();
        let mut blank_below = 0;
        for i in ((cursor_y + 1)..old_rows).rev() {
            if i < lines.len() && Self::is_line_blank(&lines[i]) {
                blank_below += 1;
            } else {
                break;
            }
        }

        let lines_to_scroll = rows_to_remove.saturating_sub(blank_below);
        if lines_to_scroll > 0 {
            let captured = self.capture_top_lines(lines_to_scroll);
            let count = captured.len();
            self.scrollback.push_lines(captured);
            // These lines are owed back to a symmetric grow.
            self.restorable_scrollback_count += count;
            for _ in 0..count.min(self.saved_line_tails.len()) {
                self.saved_line_tails.remove(0);
            }

            self.primary_surface.add_change(Change::ScrollRegionUp {
                first_row: 0,
                region_size: old_rows,
                scroll_count: count,
            });
        }
    }

    /// When rows grow, restore lines from scrollback to the top of the screen.
    /// Called AFTER primary_surface.resize() so the surface already has room.
    /// Returns the number of lines restored (for cursor offset calculation).
    pub(crate) fn handle_rows_grow(&mut self, new_rows: usize, old_rows: usize) -> usize {
        use termwiz::surface::Position;

        let rows_added = new_rows - old_rows;
        // Only restore lines that were pushed by a prior shrink.
        let restore_count = rows_added
            .min(self.scrollback.memory_len())
            .min(self.restorable_scrollback_count);

        if restore_count == 0 {
            return 0;
        }

        let mut to_restore: Vec<crate::scrollback::ScrollbackLine> = Vec::new();
        for _ in 0..restore_count {
            if let Some(line) = self.scrollback.pop_back() {
                to_restore.push(line);
            }
        }
        let actual_restored = to_restore.len();
        self.restorable_scrollback_count -= actual_restored;
        to_restore.reverse(); // oldest first

        self.primary_surface.add_change(Change::ScrollRegionDown {
            first_row: 0,
            region_size: new_rows,
            scroll_count: actual_restored,
        });

        for (row, line) in to_restore.iter().enumerate() {
            self.primary_surface.add_change(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(row),
            });
            for (text, attrs) in line.cells() {
                self.primary_surface
                    .add_change(Change::AllAttributes(attrs.clone()));
                self.primary_surface
                    .add_change(Change::Text(text.to_string()));
            }
        }

        if !self.saved_line_tails.is_empty() {
            let mut shifted = vec![Vec::new(); actual_restored];
            shifted.append(&mut self.saved_line_tails);
            self.saved_line_tails = shifted;
        }

        actual_restored
    }

    /// Check if a line is visually blank (see [`crate::scrollback::cell_is_blank`]).
    pub(crate) fn is_line_blank(line: &termwiz::surface::line::Line) -> bool {
        line.visible_cells()
            .all(|cell| crate::scrollback::cell_is_blank(cell.str(), cell.attrs()))
    }

    /// Heuristic: returns true when the line's rightmost cell is occupied by a
    /// non-space grapheme — the signature of a soft-wrap.
    pub(crate) fn line_was_soft_wrapped(line: &termwiz::surface::line::Line, cols: usize) -> bool {
        if cols == 0 {
            return false;
        }
        for cell in line.visible_cells() {
            let idx = cell.cell_index();
            let width = cell.width().max(1);
            // A cell that occupies the rightmost column has its right edge at `cols`.
            if idx + width == cols {
                let s = cell.str();
                return !s.is_empty() && s.trim() != "";
            }
        }
        false
    }

    /// Before termwiz truncates lines, capture cells that would be lost (cols
    /// shrinking) or merge saved tails back when cols grow.
    pub(crate) fn save_or_restore_line_tails(
        &mut self,
        old_cols: usize,
        new_cols: usize,
        new_rows: usize,
    ) {
        let lines = self.primary_surface.screen_lines();
        let line_count = lines.len();

        if self.saved_line_tails.len() < line_count {
            self.saved_line_tails.resize(line_count, Vec::new());
        }

        if new_cols < old_cols {
            for (i, line) in lines.iter().enumerate() {
                let mut tail_cells: Vec<(String, CellAttributes)> = line
                    .visible_cells()
                    .filter(|cell| cell.cell_index() >= new_cols)
                    .map(|cell| (cell.str().to_string(), cell.attrs().clone()))
                    .collect();
                // Prepend to any previously saved tail for this line
                if !self.saved_line_tails[i].is_empty() {
                    tail_cells.append(&mut self.saved_line_tails[i]);
                }
                self.saved_line_tails[i] = tail_cells;
            }
        }

        self.saved_line_tails.truncate(new_rows);
    }

    /// After termwiz Surface::resize expanded cols, write back saved tail cells.
    pub(crate) fn restore_tails_to_surface(&mut self, old_cols: usize, new_cols: usize) {
        use termwiz::surface::Position;

        let restore_count = new_cols - old_cols;

        for (row, tail) in self.saved_line_tails.iter_mut().enumerate() {
            if tail.is_empty() {
                continue;
            }
            let cells_to_restore = restore_count.min(tail.len());
            let restored: Vec<(String, CellAttributes)> = tail.drain(..cells_to_restore).collect();

            self.primary_surface.add_change(Change::CursorPosition {
                x: Position::Absolute(old_cols),
                y: Position::Absolute(row),
            });
            for (text, attrs) in restored {
                self.primary_surface
                    .add_change(Change::AllAttributes(attrs));
                self.primary_surface.add_change(Change::Text(text));
            }
        }
    }
}

impl Terminal {
    /// Change the grid and publish its resize tap under the content lock. The owner
    /// subsequently schedules this size on the matching Pty; a tap is not an OS ACK.
    pub fn resize(&mut self, cols: usize, rows: usize) -> bool {
        if self.cached_dims == (cols, rows) {
            return false;
        }
        let changed = self.lock_state().resize_grid(cols, rows);
        self.cached_dims = (cols, rows);
        changed
    }

    /// attach client의 크기 요청에 쓴다. 크기가 바뀌면 [`Terminal::resize`]와 같다.
    /// 이미 그 크기면 현재 grid 크기를 attach tap에 넣어 요청자가 출력과 같은 순서로 응답을 받게 한다.
    /// 반환값은 grid가 바뀌었는지이며, 바뀐 경우에만 호출자가 PTY 크기 알림을 예약한다.
    pub fn resize_or_confirm_attach(&mut self, cols: usize, rows: usize) -> bool {
        if self.resize(cols, rows) {
            return true;
        }
        let mut state = self.lock_state();
        let (cols, rows) = (state.cols, state.rows);
        state.fan_out_attach_resize(cols, rows);
        false
    }
}
