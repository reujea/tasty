use crate::app::command::{DomainIntent, SendPayload};
use crate::model::PhysicalRect;
use crate::runtime::engine_read::EngineRead;
use crate::selection::{self, SelectionMode, SelectionPoint, TextSelection};
use crate::view::ui::View;

use super::MainView;

impl MainView {
    /// Extend the current selection (or create one from last click) to the given position.
    /// Used for Shift+Click range selection.
    pub(super) fn extend_selection(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &PhysicalRect,
    ) {
        if let Some((point, surface_id)) = self.mouse_to_grid(&engine.as_ref(), x, y, terminal_rect)
        {
            if let Some(sel) = &mut self.text_selection {
                // Existing selection: keep anchor, move cursor
                if sel.surface_id == surface_id && sel.anchor.epoch == point.epoch {
                    sel.cursor = point;
                    sel.mode = SelectionMode::Normal;
                    sel.dragging = true;
                }
            } else if let Some(anchor) = self.last_click_pos.filter(|p| p.epoch == point.epoch) {
                // No selection but have a previous click position: use it as anchor
                self.text_selection = Some(TextSelection {
                    anchor,
                    cursor: point,
                    mode: SelectionMode::Normal,
                    surface_id,
                    dragging: true,
                });
            }
            self.mark_dirty();
        }
    }

    /// Start a new text selection from the given pixel position.
    pub(super) fn start_selection(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &PhysicalRect,
    ) {
        if let Some((point, surface_id, word_bounds, cols)) =
            self.mouse_selection_context(&engine.as_ref(), x, y, terminal_rect)
        {
            // Detect multi-click
            let now = std::time::Instant::now();
            let same_pos = self
                .last_click_pos
                .is_some_and(|previous| previous == point);
            let within_time = self
                .last_click_time
                .is_some_and(|t| now.duration_since(t).as_millis() < 400);
            if same_pos && within_time {
                self.click_count = (self.click_count + 1).min(3);
            } else {
                self.click_count = 1;
            }
            self.last_click_time = Some(now);
            self.last_click_pos = Some(point);

            let (mode, dragging) = match self.click_count {
                2 => (SelectionMode::Word, false),
                3 => {
                    self.click_count = 0; // Reset after triple
                    (SelectionMode::Line, false)
                }
                _ => (SelectionMode::Normal, true),
            };

            // For word/line mode, expand anchor/cursor
            let (anchor, cursor) = match mode {
                SelectionMode::Word => {
                    let (start_col, end_col) = word_bounds;
                    (
                        SelectionPoint {
                            col: start_col,
                            ..point
                        },
                        SelectionPoint {
                            col: end_col,
                            ..point
                        },
                    )
                }
                SelectionMode::Line => (
                    SelectionPoint { col: 0, ..point },
                    SelectionPoint {
                        col: cols.saturating_sub(1),
                        ..point
                    },
                ),
                SelectionMode::Normal | SelectionMode::Block => {
                    // Clear any existing selection on single click. Block mode
                    // is never produced by mouse click — defensive default.
                    (point, point)
                }
            };

            self.text_selection = Some(TextSelection {
                anchor,
                cursor,
                mode,
                surface_id,
                dragging,
            });
            self.mark_dirty();
        } else {
            // Clicked outside terminal — clear selection

            self.text_selection = None;
        }
    }

    /// Find word boundaries around the given column in the given absolute row.
    fn find_word_bounds(
        view: &tasty_terminal::TerminalReadView<'_>,
        point: SelectionPoint,
    ) -> (usize, usize) {
        let col = point.col;
        let absolute_row = point.absolute_row;
        let row_text: Vec<(String, usize)> = if absolute_row < view.screen_start() {
            let Some(line) = view.scrollback_line_full(absolute_row) else {
                return (col, col);
            };
            let mut result = Vec::new();
            let mut c = 0;
            for (text, _) in line.cells() {
                result.push((text.to_string(), c));
                c += tasty_cell_width::unicode_width(text.chars().next().unwrap_or(' '));
            }
            result
        } else {
            let lines = view.surface().screen_lines();
            let Some(line) = lines.get(absolute_row - view.screen_start()) else {
                return (col, col);
            };
            line.visible_cells()
                .map(|cell| (cell.str().to_string(), cell.cell_index()))
                .collect()
        };

        // Find which cell the col is in
        let is_word_char = |s: &str| -> bool {
            s.chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        };

        // Find the cell at col
        let target_idx = row_text
            .iter()
            .position(|(_, c)| *c >= col)
            .unwrap_or(row_text.len().saturating_sub(1));
        if row_text.is_empty() {
            return (col, col);
        }
        let target_idx = target_idx.min(row_text.len() - 1);
        let word = is_word_char(&row_text[target_idx].0);

        // Expand left
        let mut start = target_idx;
        while start > 0 && is_word_char(&row_text[start - 1].0) == word {
            start -= 1;
        }
        // Expand right
        let mut end = target_idx;
        while end + 1 < row_text.len() && is_word_char(&row_text[end + 1].0) == word {
            end += 1;
        }

        let start_col = row_text[start].1;
        let end_text = &row_text[end].0;
        let end_ch = end_text.chars().next().unwrap_or(' ');
        let end_col = row_text[end].1 + tasty_cell_width::unicode_width(end_ch) - 1;
        (start_col, end_col)
    }

    /// Move the terminal cursor to the clicked position using the click_cursor module.
    pub(super) fn move_cursor_to_click(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &PhysicalRect,
    ) {
        if !engine.settings.general.click_to_move_cursor {
            return;
        }

        // Only move cursor when a shell is in the foreground.
        // Non-shell programs have their own cursor semantics that don't match
        // terminal grid positions, so arrow-key injection would be incorrect.
        let is_shell = self
            .state
            .focused_surface_id(engine)
            .and_then(|sid| engine.foreground_process_name_now(sid))
            .map(|name| crate::click_cursor::is_shell_process(&name))
            .unwrap_or(false);
        if !is_shell {
            return;
        }

        let surface_id = match self.state.focused_surface_id(engine) {
            Some(sid) => sid,
            None => return,
        };

        // Commit any in-progress IME composition before moving cursor.
        // preedit text 도 Intent 큐로 — Intent FIFO 라 arrow 보다 먼저 처리.
        if self.ime_preedit.is_some() {
            super::ime::flush_preedit(self, engine);
        }

        let terminal = match self.state.focused_terminal(&engine.as_ref()) {
            Some(t) => t,
            None => return,
        };

        // Use the actual content rect (after tab bar) instead of the raw pane rect
        let surface_rect = match self.state.focused_surface_rect(
            engine,
            *terminal_rect,
            self.base.gpu.scale_factor(),
        ) {
            Some(r) => r,
            None => return,
        };
        let position =
            terminal.with_view(&self.state.terminal_views.get(engine, surface_id), |view| {
                let region = crate::click_cursor::EditableRegion::from_terminal(&view)?;
                let (cols, rows) = view.dimensions();
                let (click_col, click_row) = crate::click_cursor::pixel_to_grid(
                    x,
                    y,
                    &surface_rect,
                    self.base.gpu.cell_width(),
                    self.base.gpu.cell_height(),
                    cols,
                    rows,
                );
                let (click_row, click_col) = region.clamp(click_row, click_col)?;
                if click_row == region.cursor_row && click_col == region.cursor_col {
                    return None;
                }
                let going_right = (click_row, click_col) > (region.cursor_row, region.cursor_col);
                let count = crate::click_cursor::count_arrows(
                    &view,
                    region.cursor_row,
                    region.cursor_col,
                    click_row,
                    click_col,
                    cols,
                );
                (count != 0).then_some((going_right, count, view.application_cursor_keys()))
            });
        let Some((going_right, arrow_count, app_cursor)) = position else {
            return;
        };
        let arrow: &'static [u8] = if going_right {
            if app_cursor { b"\x1bOC" } else { b"\x1b[C" }
        } else if app_cursor {
            b"\x1bOD"
        } else {
            b"\x1b[D"
        };

        // arrow_count 만큼 sequence 를 한 Vec<u8> 에 concat 후 1 Intent 발행
        // (큐 폭증 회피).
        let mut bytes = Vec::with_capacity(arrow.len() * arrow_count);
        for _ in 0..arrow_count {
            bytes.extend_from_slice(arrow);
        }
        self.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Bytes(bytes),
            }
            .from_user_shortcut("click_cursor"),
        );
    }

    /// Convert mouse physical coordinates to a grid SelectionPoint for the focused terminal.
    pub(super) fn mouse_to_grid(
        &self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &PhysicalRect,
    ) -> Option<(SelectionPoint, u32)> {
        self.mouse_selection_context(engine, x, y, terminal_rect)
            .map(|(point, sid, _, _)| (point, sid))
    }

    fn mouse_selection_context(
        &self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &PhysicalRect,
    ) -> Option<(SelectionPoint, u32, (usize, usize), usize)> {
        let surface_id = self.state.focused_surface_id(engine)?;
        // hard 점유(readonly)면 mirror, 아니면 live — 실제 렌더되는 것과 동일 대상을
        // 참조해야 좌표 변환이 화면과 일치한다(docs/dev-guide/attach-behavior.md).
        let terminal = engine.visible_terminal(surface_id)?;
        // Use the actual content rect (after tab bar) instead of the raw pane rect
        let surface_rect = self.state.focused_surface_rect(
            engine,
            *terminal_rect,
            self.base.gpu.scale_factor(),
        )?;
        terminal.with_view(&self.state.terminal_views.get(engine, surface_id), |view| {
            let point = selection::pixel_to_grid(
                x,
                y,
                &surface_rect,
                self.base.gpu.cell_width(),
                self.base.gpu.cell_height(),
                view.viewport(),
            );
            Some((
                point,
                surface_id,
                Self::find_word_bounds(&view, point),
                view.cols(),
            ))
        })
    }

    /// 현재 선택을 복사하고 선택 범위는 유지한다. 우클릭 복사는 포커스와 무관하다.
    /// Ctrl+C의 포커스 일치 검사는 호출자인 handle_copy_shortcut에서 한다.
    pub fn copy_selection_to_clipboard(&mut self, engine: &EngineRead<'_>) -> bool {
        self.copy_selection_with_hint(engine, Vec::new())
    }

    /// [`Self::copy_selection_to_clipboard`]와 같고 토스트에 `hint` 키캡을 붙인다.
    /// 우클릭 메뉴처럼 키보드가 아닌 경로에서 부른다.
    pub fn copy_selection_with_hint(&mut self, engine: &EngineRead<'_>, hint: Vec<String>) -> bool {
        let selection = self.text_selection.clone();
        copy_selection(engine, selection.as_ref(), false, |text, sid| {
            self.copy_text(
                text,
                false,
                "toast.copied",
                hint,
                crate::adapters::ui::ToastScope::Surface(sid),
            )
        })
    }

    /// Copy with hard newlines replaced by spaces, retaining the selection.
    pub fn copy_selection_no_newline(&mut self, engine: &EngineRead<'_>) -> bool {
        let selection = self.text_selection.clone();
        copy_selection(engine, selection.as_ref(), true, |text, sid| {
            self.copy_text(
                text,
                false,
                "toast.copied",
                Vec::new(),
                crate::adapters::ui::ToastScope::Surface(sid),
            )
        })
    }
}

/// A selected Copy consumes input even when the clipboard write fails: false would send SIGINT.
fn copy_selection(
    engine: &EngineRead<'_>,
    selection: Option<&TextSelection>,
    no_newline: bool,
    write: impl FnOnce(&str, u32) -> bool,
) -> bool {
    let Some(sel) = selection.filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(terminal) = engine.visible_terminal(sel.surface_id) else {
        return false;
    };
    let text = selection::extract_selected_text(terminal, sel);
    if !text.is_empty() {
        let text = if no_newline {
            text.replace('\n', " ")
        } else {
            text
        };
        // Write success affects feedback, never input routing.
        write(&text, sel.surface_id);
    }
    true
}

/// 선택한 surface와 포커스가 같은지 확인한다. Ctrl+C에만 적용하며 우클릭 복사는 제외한다.
pub(crate) fn selection_matches_focus(selection_surface_id: u32, focused: Option<u32>) -> bool {
    focused == Some(selection_surface_id)
}

/// 선택이 있고 포커스된 surface에 속하는지 확인한다.
pub(crate) fn should_copy_via_focused_selection(
    selection_surface_id: Option<u32>,
    focused: Option<u32>,
) -> bool {
    selection_surface_id
        .map(|sid| selection_matches_focus(sid, focused))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{selection_matches_focus, should_copy_via_focused_selection};

    #[test]
    fn clipboard_failure_consumes_copy_and_keeps_selection() {
        use super::*;
        let (mut state, mut session) = crate::state::tests::test_state();
        let sid = session.read().workspace_at(0).unwrap().all_surface_ids()[0];
        session
            .borrow_mut()
            .find_terminal_by_id_mut(sid)
            .unwrap()
            .feed_bytes(b"fixture");
        let engine = session.read();
        let epoch = engine
            .visible_terminal(sid)
            .unwrap()
            .with_content(|v| v.cut().epoch);
        let selection = TextSelection {
            surface_id: sid,
            anchor: SelectionPoint {
                epoch,
                col: 0,
                absolute_row: 0,
            },
            cursor: SelectionPoint {
                epoch,
                col: 6,
                absolute_row: 0,
            },
            mode: SelectionMode::Normal,
            dragging: false,
        };
        let mut clipboard = crate::clipboard::terminal::TerminalClipboard::unavailable(
            crate::clipboard::error::ClipboardError::BackendUnavailable("fixture".into()),
        );
        let mut writes = 0;
        assert!(copy_selection(
            &engine,
            Some(&selection),
            false,
            |text, target| {
                writes += 1;
                assert_eq!(target, sid);
                assert_eq!(text, "fixture");
                state.clipboard_copy_feedback(
                    clipboard.set_text(text),
                    "toast.copied",
                    Vec::new(),
                    crate::adapters::ui::ToastScope::Surface(sid),
                )
            }
        ));
        assert_eq!(writes, 1);
        assert_eq!(selection.cursor.col, 6);
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("toast.clipboard_unavailable")]
        );
        assert!(!copy_selection(&engine, None, false, |_, _| panic!(
            "no selection"
        )));
    }

    #[test]
    fn matches_when_focused_surface_owns_the_selection() {
        assert!(selection_matches_focus(5, Some(5)));
    }

    #[test]
    fn does_not_match_after_focus_moves_to_another_surface() {
        // 터미널 A(5)에서 드래그 선택 후 Explorer B(2)로 포커스 이동 — A 의 선택은 여전히
        // `text_selection` 에 남아있지만 더 이상 포커스와 일치하지 않는다.
        assert!(!selection_matches_focus(5, Some(2)));
    }

    #[test]
    fn does_not_match_when_nothing_is_focused() {
        assert!(!selection_matches_focus(5, None));
    }

    #[test]
    fn copy_shortcut_uses_selection_when_focus_matches() {
        assert!(should_copy_via_focused_selection(Some(5), Some(5)));
    }

    #[test]
    fn copy_shortcut_skips_stale_selection_after_focus_moves() {
        // 다른 surface에 포커스가 있으면 그 surface의 복사 처리를 사용한다.
        assert!(!should_copy_via_focused_selection(Some(5), Some(2)));
    }

    #[test]
    fn copy_shortcut_skips_when_no_selection_exists() {
        // 선택이 없으면 false다.
        assert!(!should_copy_via_focused_selection(None, Some(2)));
    }
}
