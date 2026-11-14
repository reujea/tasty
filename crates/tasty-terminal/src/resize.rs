//! Resize 처리 — grid 크기 변경(`TerminalState`)이 먼저이고, OS 크기 알림은 `Pty`에
//! 예약한 뒤 다음 `process()`의 강제 flush 또는 throttle된 `flush_pty_resize` 호출에서
//! 적용한다 (docs/surfaces/terminal/index.md#vte-에뮬레이션).

use termwiz::cell::CellAttributes;
use termwiz::surface::Change;

use crate::{Terminal, TerminalState};

/// A cell borrowed from a scrollback row, in column order.
type BorrowedCell<'a> = (&'a str, &'a CellAttributes);
/// An owned cell as kept in a [`LineTail`].
type OwnedCell = (String, CellAttributes);

/// Cells of one screen row that do not fit in the current width, kept so a
/// later widen can put them back. `cells` are contiguous graphemes in column
/// order whose first one starts at `start_col`.
#[derive(Clone, Debug, Default)]
pub(crate) struct LineTail {
    pub(crate) start_col: usize,
    pub(crate) cells: Vec<OwnedCell>,
}

impl LineTail {
    pub(crate) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

/// Column width of one grapheme as termwiz prints it.
fn grapheme_width(text: &str) -> usize {
    termwiz::cell::grapheme_column_width(text, None).max(1)
}

/// The padding a row keeps after its content: a default-attribute ASCII space
/// or an empty cell. Other whitespace (NBSP, U+3000) is content the program printed.
fn is_default_padding(text: &str, attrs: &CellAttributes) -> bool {
    (text.is_empty() || text == " ") && *attrs == CellAttributes::default()
}

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
            self.save_or_restore_line_tails(old_cols, cols);
        }

        // Handle rows shrink BEFORE resize (need to capture lines before they're lost)
        if rows < old_rows && !self.use_alternate {
            self.handle_rows_shrink(rows, old_rows);
        }
        // Tails are indexed by screen row; rows past the new bottom are gone.
        self.saved_line_tails.truncate(rows);

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
            self.restore_tails_to_surface(cols);
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
            let mut captured = self.capture_top_lines(lines_to_scroll);
            let count = captured.len();
            // A row going to scrollback takes its saved tail with it, so the
            // whole row survives instead of only the part that fit on screen.
            let tails: Vec<LineTail> = self
                .saved_line_tails
                .drain(..count.min(self.saved_line_tails.len()))
                .collect();
            for (line, tail) in captured.iter_mut().zip(tails) {
                if !tail.is_empty() {
                    *line = Self::join_tail(line, tail);
                }
            }
            self.scrollback.push_lines(captured);
            // These lines are owed back to a symmetric grow.
            self.restorable_scrollback_count += count;

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

        let cols = self.cols;
        let mut restored_tails = Vec::with_capacity(actual_restored);
        for (row, line) in to_restore.iter().enumerate() {
            self.primary_surface.add_change(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(row),
            });
            let (fits, tail) = Self::split_restored_row(line, cols);
            for (text, attrs) in fits {
                self.primary_surface
                    .add_change(Change::AllAttributes(attrs.clone()));
                self.primary_surface
                    .add_change(Change::Text(text.to_string()));
            }
            restored_tails.push(tail);
        }

        let any_restored_tail = restored_tails.iter().any(|tail| !tail.is_empty());
        if any_restored_tail || !self.saved_line_tails.is_empty() {
            restored_tails.append(&mut self.saved_line_tails);
            self.saved_line_tails = restored_tails;
        }

        actual_restored
    }

    /// Split a scrollback row into the cells that fit in `cols` and the overflow
    /// tail. The row keeps the width it had when it scrolled off, so writing it
    /// whole into a narrower grid would soft-wrap over the rows below. Trailing
    /// padding is dropped; a wide grapheme crossing the right edge moves to the
    /// tail whole, and the tail starts at that grapheme's column.
    fn split_restored_row(
        line: &crate::scrollback::ScrollbackLine,
        cols: usize,
    ) -> (Vec<BorrowedCell<'_>>, LineTail) {
        let mut cells: Vec<BorrowedCell<'_>> = line.cells().collect();
        while cells
            .last()
            .is_some_and(|(text, attrs)| is_default_padding(text, attrs))
        {
            cells.pop();
        }

        let mut col = 0usize;
        let mut split_at = cells.len();
        for (i, (text, _)) in cells.iter().enumerate() {
            let width = grapheme_width(text);
            if col + width > cols {
                split_at = i;
                break;
            }
            col += width;
        }
        let tail = LineTail {
            start_col: col,
            cells: cells[split_at..]
                .iter()
                .map(|(text, attrs)| ((*text).to_string(), (*attrs).clone()))
                .collect(),
        };
        cells.truncate(split_at);
        (cells, tail)
    }

    /// Rebuild a captured row with its saved tail: the row's cells left of
    /// `tail.start_col`, blank padding up to it, then the tail cells. The soft
    /// wrap guess was made against the narrower width, so a row whose tail holds
    /// content is stored as not wrapped.
    fn join_tail(
        line: &crate::scrollback::ScrollbackLine,
        tail: LineTail,
    ) -> crate::scrollback::ScrollbackLine {
        let blank_attrs = CellAttributes::default();
        let mut builder = crate::scrollback::ScrollbackLineBuilder::new();
        let mut col = 0usize;
        for (text, attrs) in line.cells() {
            let width = grapheme_width(text);
            if col + width > tail.start_col {
                break;
            }
            builder.push(text, attrs);
            col += width;
        }
        while col < tail.start_col {
            builder.push(" ", &blank_attrs);
            col += 1;
        }
        let tail_has_content = tail
            .cells
            .iter()
            .any(|(text, attrs)| !is_default_padding(text, attrs));
        for (text, attrs) in &tail.cells {
            builder.push(text, attrs);
        }
        builder.finish(line.wrapped && !tail_has_content)
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

    /// Before termwiz truncates lines on a cols shrink, move the cells that would
    /// be lost to each row's tail, ahead of what the tail already holds. Also
    /// gives every screen row a tail slot.
    pub(crate) fn save_or_restore_line_tails(&mut self, old_cols: usize, new_cols: usize) {
        let lines = self.primary_surface.screen_lines();
        let line_count = lines.len();

        if self.saved_line_tails.len() < line_count {
            self.saved_line_tails
                .resize(line_count, LineTail::default());
        }

        if new_cols < old_cols {
            for (i, line) in lines.iter().enumerate() {
                let saved = std::mem::take(&mut self.saved_line_tails[i]);
                // Cells right of an existing tail's start are blank on screen;
                // the tail already holds what belongs there.
                let limit = if saved.is_empty() {
                    usize::MAX
                } else {
                    saved.start_col
                };
                let mut start_col = None;
                let mut cells: Vec<OwnedCell> = Vec::new();
                for cell in line.visible_cells() {
                    let idx = cell.cell_index();
                    if idx < new_cols || idx >= limit {
                        continue;
                    }
                    start_col.get_or_insert(idx);
                    cells.push((cell.str().to_string(), cell.attrs().clone()));
                }
                let start_col = start_col.unwrap_or(if saved.is_empty() {
                    new_cols
                } else {
                    saved.start_col
                });
                cells.extend(saved.cells);
                self.saved_line_tails[i] = LineTail { start_col, cells };
            }
        }
    }

    /// After termwiz Surface::resize expanded cols, write back the saved tail
    /// cells that now fit, each at its own column. Graphemes are counted by
    /// column width so a wide one is never written across the right edge.
    pub(crate) fn restore_tails_to_surface(&mut self, new_cols: usize) {
        use termwiz::surface::Position;

        for (row, tail) in self.saved_line_tails.iter_mut().enumerate() {
            if tail.is_empty() {
                continue;
            }
            let mut col = tail.start_col;
            let mut take = 0usize;
            for (text, _) in &tail.cells {
                let width = grapheme_width(text);
                if col + width > new_cols {
                    break;
                }
                col += width;
                take += 1;
            }
            if take == 0 {
                continue;
            }

            self.primary_surface.add_change(Change::CursorPosition {
                x: Position::Absolute(tail.start_col),
                y: Position::Absolute(row),
            });
            for (text, attrs) in tail.cells.drain(..take) {
                self.primary_surface
                    .add_change(Change::AllAttributes(attrs));
                self.primary_surface.add_change(Change::Text(text));
            }
            tail.start_col = col;
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
