use termwiz::cell::{CellAttributes, unicode_column_width};
use termwiz::escape::{Action, ControlCode};
use termwiz::surface::Change;

use super::{TerminalEvent, TerminalEventKind, TerminalState};

impl TerminalState {
    /// Convert a parsed VT action into Surface changes.
    pub(crate) fn action_to_changes(&mut self, action: Action) -> Vec<Change> {
        match action {
            Action::Print(c) => {
                let text = self.apply_charset(&c.to_string());
                self.emit_output_text(|| text.clone());
                self.last_print = Some(text.clone());
                self.print_with_insert_mode(unicode_column_width(&text, None), text)
            }
            Action::PrintString(s) => {
                let text = self.apply_charset(&s);
                self.emit_output_text(|| text.clone());
                if let Some(last) = text.chars().last() {
                    self.last_print = Some(last.to_string());
                }
                let width = unicode_column_width(&text, None);
                self.print_with_insert_mode(width, text)
            }
            Action::Control(code) => {
                if matches!(code, ControlCode::LineFeed) {
                    self.emit_output_text(|| "\n".to_string());
                }
                self.map_control(code)
            }
            Action::CSI(csi) => self.map_csi(csi),
            Action::Esc(esc) => self.map_esc(esc),
            Action::OperatingSystemCommand(osc) => {
                self.map_osc(*osc);
                vec![]
            }
            // XtGetTcap (DCS + q ... ST): termcap/terminfo capability query.
            // Answered with a "currently not supported" reply (see handler).
            Action::XtGetTcap(names) => {
                self.handle_xtgettcap(&names);
                vec![]
            }
            _ => vec![],
        }
    }

    /// Whether the charset currently invoked into GL is the DEC line-drawing set.
    pub(crate) fn active_charset_line_drawing(&self) -> bool {
        if self.charset_active_g1 {
            self.charset_g1_line_drawing
        } else {
            self.charset_g0_line_drawing
        }
    }

    /// Translate printed text through the active charset. When the DEC special
    /// line-drawing set is invoked, ASCII bytes `0x60..=0x7e` map to box-drawing
    /// glyphs; everything else passes through. Fast path (no allocation beyond
    /// the clone) when line drawing is inactive.
    pub(crate) fn apply_charset(&self, text: &str) -> String {
        if !self.active_charset_line_drawing() {
            return text.to_string();
        }
        text.chars()
            .map(|c| dec_line_drawing_glyph(c).unwrap_or(c))
            .collect()
    }

    /// Build the changes for printing `text` (column width `width`). When IRM
    /// (insert mode) is active, existing cells are shifted right by `width`
    /// columns first (ICH semantics) so the glyphs are inserted rather than
    /// overwriting. The shift Change is emitted before the Text, keeping the
    /// Print on its normal (non-scrolling) path.
    fn print_with_insert_mode(&self, width: usize, text: String) -> Vec<Change> {
        if !self.insert_mode {
            return vec![Change::Text(text)];
        }
        let mut changes = self.insert_blank_changes(width);
        changes.push(Change::Text(text));
        changes
    }

    /// Push an `OutputAppended` event for the observer router. The text is
    /// built lazily so a disabled gate (no observers — the common case) costs
    /// no allocation. `surface_id = 0` here; the host fills it on
    /// `collect_events`.
    fn emit_output_text(&mut self, make_text: impl FnOnce() -> String) {
        if !self.emit_output_events {
            return;
        }
        let text = make_text();
        if text.is_empty() {
            return;
        }
        self.events.push(TerminalEvent {
            generation: self.connection.generation(),
            surface_id: 0,
            kind: TerminalEventKind::OutputAppended { text },
        });
    }

    /// Cells of `row` from `start_col` up to `end_col` with their attributes and column widths.
    /// A gap between stored cells is a default blank. Stops at the last stored cell.
    pub(crate) fn read_cells_from_surface(
        &self,
        row: usize,
        start_col: usize,
        end_col: usize,
    ) -> Vec<(String, CellAttributes, usize)> {
        let surface = self.surface();
        let lines = surface.screen_lines();
        let mut cells = Vec::new();
        let Some(line) = lines.get(row) else {
            return cells;
        };
        let mut next_col = start_col;
        for cell in line.visible_cells() {
            let idx = cell.cell_index();
            if idx >= end_col {
                break;
            }
            if idx < start_col {
                continue;
            }
            for _ in next_col..idx {
                cells.push((" ".to_string(), CellAttributes::default(), 1));
            }
            let width = cell.width().max(1);
            cells.push((cell.str().to_string(), cell.attrs().clone(), width));
            next_col = idx + width;
        }
        cells
    }

    /// Column of the next tab stop strictly to the right of `col`. Falls back to
    /// the last column when no further stop exists (HT never wraps to the next
    /// line; it clamps at the right margin).
    pub(crate) fn next_tab_stop(&self, col: usize) -> usize {
        let last = self.cols.saturating_sub(1);
        for c in (col + 1)..self.cols {
            if self.tab_stops.get(c).copied().unwrap_or(false) {
                return c;
            }
        }
        last
    }

    /// Column of the previous tab stop strictly to the left of `col`. Falls back
    /// to column 0 (CBT clamps at the left margin).
    pub(crate) fn prev_tab_stop(&self, col: usize) -> usize {
        for c in (0..col).rev() {
            if self.tab_stops.get(c).copied().unwrap_or(false) {
                return c;
            }
        }
        0
    }

    /// Set a tab stop at `col` (HTS).
    pub(crate) fn set_tab_stop(&mut self, col: usize) {
        if let Some(slot) = self.tab_stops.get_mut(col) {
            *slot = true;
        }
    }

    /// Clear the tab stop at `col` (TBC 0).
    pub(crate) fn clear_tab_stop(&mut self, col: usize) {
        if let Some(slot) = self.tab_stops.get_mut(col) {
            *slot = false;
        }
    }

    /// Clear every tab stop (TBC 3).
    pub(crate) fn clear_all_tab_stops(&mut self) {
        self.tab_stops.iter_mut().for_each(|s| *s = false);
    }

    /// Scroll-region row bounds `(top, bottom)`, 0-based inclusive. Full screen
    /// when no region is set.
    pub(crate) fn region_bounds(&self) -> (usize, usize) {
        match self.scroll_region {
            Some((top, bottom)) => (top, bottom),
            None => (0, self.rows.saturating_sub(1)),
        }
    }

    /// Translate a logical (0-based) absolute row to a physical surface row,
    /// honoring DECOM origin mode. In origin mode the row is offset by the
    /// region top and clamped to the region bottom; otherwise it is unchanged.
    pub(crate) fn resolve_origin_row(&self, logical_row: usize) -> usize {
        if self.origin_mode {
            let (top, bottom) = self.region_bounds();
            (top + logical_row).min(bottom)
        } else {
            logical_row
        }
    }

    /// Home row for cursor positioning: region top in origin mode, else 0.
    pub(crate) fn origin_home_row(&self) -> usize {
        if self.origin_mode {
            self.region_bounds().0
        } else {
            0
        }
    }

    /// Get scroll region parameters for ScrollRegionUp/Down changes.
    pub(crate) fn scroll_region_params(&self) -> (usize, usize) {
        match self.scroll_region {
            Some((top, bottom)) => {
                let size = bottom.saturating_sub(top) + 1;
                (top, size)
            }
            None => {
                let (_cols, rows) = self.surface().dimensions();
                (0, rows)
            }
        }
    }
}

/// Map an ASCII byte to its DEC special graphics (line-drawing) glyph. Covers
/// the standard `0x60..=0x7e` range; returns `None` for anything outside it
/// (which prints unchanged). Reference: VT100 special graphics charset.
fn dec_line_drawing_glyph(c: char) -> Option<char> {
    Some(match c {
        '`' => '◆',
        'a' => '▒',
        'b' => '\u{2409}', // HT symbol
        'c' => '\u{240c}', // FF symbol
        'd' => '\u{240d}', // CR symbol
        'e' => '\u{240a}', // LF symbol
        'f' => '°',
        'g' => '±',
        'h' => '\u{2424}', // NL symbol
        'i' => '\u{240b}', // VT symbol
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => return None,
    })
}

mod control;
mod cursor;
mod edit;
mod esc;
mod osc;
