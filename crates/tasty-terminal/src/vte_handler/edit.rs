//! ED/EL erases include the cursor cell and preserve cursor position and the drawing pen.
//!
//! A pending wrap is represented by cx == cols, beyond the last visible column.
//! For erase ranges, that means the last column. Absolute(cols) clamps to cols - 1,
//! so restoring the cursor with an absolute move would lose the pending wrap.
//! Instead, erase the last cell by printing a space with the erase attributes; this
//! advances the cursor back to cols. All six ED/EL branches preserve that state.
//!
//! This depends on termwiz storing pending wrap in its cursor column. If wrap gets a
//! separate flag, cursor preservation alone will no longer determine that flag's state.
//!
//! Erased cells use default attributes with the current background color. The drawing
//! pen is restored afterward because termwiz's clear operations reset it. ED2 also
//! homes the cursor; EL2 moves to column zero here to erase the full line. Both need
//! cursor restoration. See docs/surfaces/terminal/index.md#스크롤-영역과-소거.

use termwiz::cell::{CellAttributes, unicode_column_width};
use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{Edit, EraseInDisplay, EraseInLine};
use termwiz::surface::{Change, Position};

use crate::TerminalState;

/// Number of cells from column zero through the cursor. Pending wrap includes the full row.
fn erase_span_to_cursor(cx: usize, cols: usize) -> usize {
    (cx + 1).min(cols)
}

/// Restore the cursor after erasing. Pending wrap needs no absolute move: printing
/// through the last cell restores it, while Absolute(cols) would clear it.
fn restore_cursor_column(cx: usize, cy: usize, cols: usize) -> Option<Change> {
    (cx < cols).then_some(Change::CursorPosition {
        x: Position::Absolute(cx),
        y: Position::Absolute(cy),
    })
}

/// Erase the last cell and restore pending wrap by printing with erase_attrs.
/// The caller must restore the previous drawing pen afterward.
fn park_on_erased_last_cell(pen: CellAttributes, cy: usize, cols: usize) -> Vec<Change> {
    vec![
        Change::CursorPosition {
            x: Position::Absolute(cols.saturating_sub(1)),
            y: Position::Absolute(cy),
        },
        Change::AllAttributes(pen),
        Change::Text(" ".to_string()),
    ]
}

impl TerminalState {
    /// Erased cells retain the current background but reset other attributes.
    fn erase_attrs(&self) -> CellAttributes {
        CellAttributes::default()
            .set_background(self.current_pen.background())
            .clone()
    }

    /// 소거가 칸을 채울 배경색 — termwiz 지우기 primitive 의 인자.
    ///
    /// primitive 는 `CellAttributes::default().set_background(color)` 로 칸을 채우므로
    /// 이 한 값이 [`Self::erase_attrs`] 와 같은 결과를 낸다.
    fn erase_color(&self) -> ColorAttribute {
        self.current_pen.background()
    }

    /// Restore the drawing pen after erasing. Cell erase attributes and subsequent
    /// text attributes must be checked separately.
    fn restore_pen(&self) -> Change {
        Change::AllAttributes(self.current_pen.clone())
    }

    pub(crate) fn map_edit(&mut self, edit: Edit) -> Vec<Change> {
        // 소거 pen 은 **소거 시작 시점**의 것이다. 아래 갈래들이 내는 `Clear*` 는
        // 적용될 때 `mirror_pen` 을 통해 `current_pen` 을 바꾸므로, 한 번 읽어 둔다.
        let erase_color = self.erase_color();
        let erase_attrs = self.erase_attrs();
        let restore_pen = self.restore_pen();
        match edit {
            Edit::EraseInDisplay(mode) => match mode {
                EraseInDisplay::EraseToEndOfDisplay => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    // primitive 는 커서 행을 `xpos..width` 로 자르므로 걸친 커서
                    // (`xpos == cols`)에서는 커서 행이 빈 구간이 된다 — 아래 행들은
                    // 이 한 줄이 지우고, 커서가 올라앉은 마지막 칸만 따로 찍는다.
                    let mut changes = vec![Change::ClearToEndOfScreen(erase_color)];
                    if cx >= cols {
                        changes.extend(park_on_erased_last_cell(erase_attrs, cy, cols));
                    }
                    changes.push(restore_pen);
                    changes
                }
                EraseInDisplay::EraseToStartOfDisplay => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    let mut changes = Vec::new();
                    for row in 0..cy {
                        changes.push(Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(row),
                        });
                        changes.push(Change::ClearToEndOfLine(erase_color));
                    }
                    changes.push(Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(cy),
                    });
                    // Clear operations changed the pen; restore the erase attributes before printing this row.
                    changes.push(Change::AllAttributes(erase_attrs));
                    changes.push(Change::Text(" ".repeat(erase_span_to_cursor(cx, cols))));
                    changes.extend(restore_cursor_column(cx, cy, cols));
                    changes.push(restore_pen);
                    changes
                }
                EraseInDisplay::EraseDisplay => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    let mut changes = vec![Change::ClearScreen(erase_color)];
                    // `ClearScreen` 은 커서를 홈으로 보낸다 — ED 는 커서를 움직이지
                    // 않으므로 되돌린다. `clear` 류가 보내는 `ESC [ H ESC [ 2J` 는
                    // 먼저 홈으로 가므로 이 복원에 영향을 받지 않는다.
                    if cx < cols {
                        changes.push(Change::CursorPosition {
                            x: Position::Absolute(cx),
                            y: Position::Absolute(cy),
                        });
                    } else {
                        changes.extend(park_on_erased_last_cell(erase_attrs, cy, cols));
                    }
                    changes.push(restore_pen);
                    changes
                }
                EraseInDisplay::EraseScrollback => {
                    // ED3: erase scrollback history only — the visible screen is
                    // preserved (no Change emitted). `clear` 같은 명령이 보내는
                    // `\x1b[3J\x1b[2J` 류에서 ED2 가 화면을, ED3 가 스크롤백을 지운다.
                    self.clear_scrollback();
                    vec![]
                }
            },
            Edit::EraseInLine(mode) => match mode {
                EraseInLine::EraseToEndOfLine => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    // 걸친 커서에서 primitive 의 범위는 빈 구간이다(위 ED0 참조) —
                    // 지울 것은 커서가 올라앉은 마지막 칸 하나뿐이라 그것만 찍는다.
                    let mut changes = if cx >= cols {
                        park_on_erased_last_cell(erase_attrs, cy, cols)
                    } else {
                        vec![Change::ClearToEndOfLine(erase_color)]
                    };
                    changes.push(restore_pen);
                    changes
                }
                EraseInLine::EraseToStartOfLine => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    let mut changes = vec![
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(cy),
                        },
                        Change::AllAttributes(erase_attrs),
                        Change::Text(" ".repeat(erase_span_to_cursor(cx, cols))),
                    ];
                    changes.extend(restore_cursor_column(cx, cy, cols));
                    changes.push(restore_pen);
                    changes
                }
                EraseInLine::EraseLine => {
                    let (cx, cy) = self.surface().cursor_position();
                    let (cols, _rows) = self.surface().dimensions();
                    let mut changes = vec![
                        Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(cy),
                        },
                        Change::ClearToEndOfLine(erase_color),
                    ];
                    // 0 열로 옮긴 것은 지우기 위한 것이므로 되돌린다 — ED/EL 은
                    // 커서를 움직이지 않는다.
                    if cx < cols {
                        changes.push(Change::CursorPosition {
                            x: Position::Absolute(cx),
                            y: Position::Absolute(cy),
                        });
                    } else {
                        changes.extend(park_on_erased_last_cell(erase_attrs, cy, cols));
                    }
                    changes.push(restore_pen);
                    changes
                }
            },
            Edit::ScrollUp(n) => {
                let (first_row, region_size) = self.scroll_region_params();
                vec![Change::ScrollRegionUp {
                    first_row,
                    region_size,
                    scroll_count: n as usize,
                }]
            }
            Edit::ScrollDown(n) => {
                let (first_row, region_size) = self.scroll_region_params();
                vec![Change::ScrollRegionDown {
                    first_row,
                    region_size,
                    scroll_count: n as usize,
                }]
            }
            Edit::DeleteCharacter(n) => {
                let (cx, cy) = self.surface().cursor_position();
                let (cols, _rows) = self.surface().dimensions();
                let remaining = cols.saturating_sub(cx);
                let n = (n as usize).min(remaining);
                if n == 0 {
                    return vec![];
                }
                let mut kept = self.read_cells_from_surface(cy, cx, cols);
                // Drop n columns worth of cells; a wide cell crossing the boundary goes whole.
                let mut skip_cols = 0;
                let skip_cells = kept
                    .iter()
                    .take_while(|(_, _, width)| {
                        let skip = skip_cols < n;
                        skip_cols += width;
                        skip
                    })
                    .count();
                kept.drain(..skip_cells);
                self.rewrite_from_cursor(cx, cy, remaining, 0, kept)
            }
            Edit::InsertCharacter(n) => self.insert_blank_changes(n as usize),
            Edit::DeleteLine(n) => {
                let (_cx, cy) = self.surface().cursor_position();
                let (first_row, region_size) = self.scroll_region_params();
                let effective_first = cy.max(first_row);
                let effective_size = (first_row + region_size).saturating_sub(effective_first);
                if effective_size == 0 {
                    return vec![];
                }
                vec![
                    Change::ScrollRegionUp {
                        first_row: effective_first,
                        region_size: effective_size,
                        scroll_count: n as usize,
                    },
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(cy),
                    },
                ]
            }
            Edit::InsertLine(n) => {
                let (_cx, cy) = self.surface().cursor_position();
                let (first_row, region_size) = self.scroll_region_params();
                let effective_first = cy.max(first_row);
                let effective_size = (first_row + region_size).saturating_sub(effective_first);
                if effective_size == 0 {
                    return vec![];
                }
                vec![
                    Change::ScrollRegionDown {
                        first_row: effective_first,
                        region_size: effective_size,
                        scroll_count: n as usize,
                    },
                    Change::CursorPosition {
                        x: Position::Absolute(0),
                        y: Position::Absolute(cy),
                    },
                ]
            }
            Edit::EraseCharacter(n) => {
                let (cx, cy) = self.surface().cursor_position();
                let (cols, _rows) = self.surface().dimensions();
                let n = (n as usize).min(cols.saturating_sub(cx));
                if n == 0 {
                    return vec![];
                }
                vec![
                    Change::CursorPosition {
                        x: Position::Absolute(cx),
                        y: Position::Absolute(cy),
                    },
                    Change::Text(" ".repeat(n)),
                    Change::CursorPosition {
                        x: Position::Absolute(cx),
                        y: Position::Absolute(cy),
                    },
                ]
            }
            Edit::Repeat(n) => {
                // REP (CSI b): repeat the last printed character n times. We track
                // the last printed grapheme in `last_print` (set in
                // action_to_changes). Clamp the count to the grid area to bound
                // allocation against a hostile parameter.
                let Some(ch) = self.last_print.clone() else {
                    return vec![];
                };
                let count = (n.max(1) as usize).min(self.cols.saturating_mul(self.rows));
                let text = ch.repeat(count);
                let width = unicode_column_width(&text, None);
                self.print_with_insert_mode(width, text)
            }
        }
    }

    /// Insert `n` blank columns at the cursor, shifting existing line content to
    /// the right (ICH semantics). The cursor stays at its original position.
    /// Shared by the ICH edit op and by IRM (insert-mode) printing. Returns an
    /// empty change list when there is no room to shift.
    pub(crate) fn insert_blank_changes(&self, n: usize) -> Vec<Change> {
        let (cx, cy) = self.surface().cursor_position();
        let (cols, _rows) = self.surface().dimensions();
        let remaining = cols.saturating_sub(cx);
        let n = n.min(remaining);
        if n == 0 {
            return vec![];
        }
        // Existing cells that still fit after the n blank columns; the rest fall off the edge.
        let mut used_cols = n;
        let mut kept = self.read_cells_from_surface(cy, cx, cols);
        let fitting = kept
            .iter()
            .take_while(|(_, _, width)| {
                used_cols += width;
                used_cols <= remaining
            })
            .count();
        kept.truncate(fitting);
        self.rewrite_from_cursor(cx, cy, remaining, n, kept)
    }

    /// Rewrite `span` columns from the cursor: `leading` erased cells, then `cells` with their own
    /// attributes, then erased cells to the end of the span. The cursor and pen stay as they were.
    fn rewrite_from_cursor(
        &self,
        cx: usize,
        cy: usize,
        span: usize,
        leading: usize,
        cells: Vec<(String, CellAttributes, usize)>,
    ) -> Vec<Change> {
        let cursor = Change::CursorPosition {
            x: Position::Absolute(cx),
            y: Position::Absolute(cy),
        };
        let erased = self.erase_attrs();
        let mut changes = vec![cursor.clone()];
        let mut run_attrs = erased.clone();
        let mut run_text = " ".repeat(leading);
        let mut used_cols = leading;
        for (text, attrs, width) in cells {
            if attrs != run_attrs {
                flush_run(&mut changes, &run_attrs, &mut run_text);
                run_attrs = attrs;
            }
            run_text.push_str(&text);
            used_cols += width;
        }
        if used_cols < span && erased != run_attrs {
            flush_run(&mut changes, &run_attrs, &mut run_text);
            run_attrs = erased;
        }
        run_text.push_str(&" ".repeat(span.saturating_sub(used_cols)));
        flush_run(&mut changes, &run_attrs, &mut run_text);
        changes.push(self.restore_pen());
        changes.push(cursor);
        changes
    }
}

/// Queue `text` written with `attrs`, leaving `text` empty for the next run.
fn flush_run(changes: &mut Vec<Change>, attrs: &CellAttributes, text: &mut String) {
    if !text.is_empty() {
        changes.push(Change::AllAttributes(attrs.clone()));
        changes.push(Change::Text(std::mem::take(text)));
    }
}
