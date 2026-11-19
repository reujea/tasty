//! Consumer-owned terminal viewport and one-cut content coordinates.
//! The terminal owns history identity, never a user's reading position.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Terminal, TerminalState};

static NEXT_CONTENT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// Identity of a terminal history. Respawn and erase-scrollback invalidate old points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentEpoch {
    serial: u64,
    alternate: bool,
}

impl ContentEpoch {
    fn for_screen(self, alternate: bool) -> Self {
        Self { alternate, ..self }
    }
    pub(crate) fn fresh() -> Self {
        Self {
            serial: NEXT_CONTENT_EPOCH
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("terminal content epoch exhausted"),
            alternate: false,
        }
    }
}

/// Metadata read with rows and modes under the parser's state lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentCut {
    pub epoch: ContentEpoch,
    pub revision: u64,
    /// Stable coordinate of the oldest retained history row.
    pub first_row: usize,
    /// Stable coordinate of the first live grid row.
    pub screen_start: usize,
    pub cols: usize,
    pub rows: usize,
    pub alternate: bool,
}

impl ContentCut {
    pub fn history_len(self) -> usize {
        self.screen_start - self.first_row
    }
    pub fn end_row(self) -> usize {
        self.screen_start.saturating_add(self.rows)
    }
}

#[derive(Debug, Clone, Copy)]
struct Anchor {
    epoch: ContentEpoch,
    row: usize,
}

/// One display owner's reading position. Store this in the View, not Terminal.
/// An absent anchor follows the live grid. Old epochs also resolve to live,
/// including ED3 followed by fresh output in the same parser batch.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalViewport {
    anchor: Option<Anchor>,
}

impl TerminalViewport {
    pub const LIVE: Self = Self { anchor: None };

    pub fn resolve(self, cut: ContentCut) -> ViewportInfo {
        if cut.alternate {
            return ViewportInfo {
                cut,
                top_row: cut.screen_start,
            };
        }
        let top_row = self
            .anchor
            .filter(|a| a.epoch == cut.epoch)
            .map_or(cut.screen_start, |a| {
                a.row.clamp(cut.first_row, cut.screen_start)
            });
        ViewportInfo { cut, top_row }
    }

    pub fn scroll_up(&mut self, cut: ContentCut, rows: usize) {
        if cut.alternate {
            return;
        }
        let current = self.resolve(cut).scroll_offset();
        self.set_scroll_offset(cut, current.saturating_add(rows));
    }
    pub fn scroll_down(&mut self, cut: ContentCut, rows: usize) {
        if cut.alternate {
            return;
        }
        let current = self.resolve(cut).scroll_offset();
        self.set_scroll_offset(cut, current.saturating_sub(rows));
    }
    pub fn scroll_to_bottom(&mut self) {
        self.anchor = None;
    }
    pub fn set_scroll_offset(&mut self, cut: ContentCut, offset: usize) {
        if cut.alternate {
            return;
        }
        let offset = offset.min(cut.history_len());
        self.anchor = (offset != 0).then_some(Anchor {
            epoch: cut.epoch,
            row: cut.screen_start - offset,
        });
    }
}

/// A resolved viewport and its content cut. Pixel coordinates must be converted
/// with this value while row reads use the same locked TerminalReadView.
#[derive(Debug, Clone, Copy)]
pub struct ViewportInfo {
    pub cut: ContentCut,
    pub top_row: usize,
}
impl ViewportInfo {
    pub fn scroll_offset(self) -> usize {
        self.cut.screen_start - self.top_row
    }
}

impl TerminalState {
    pub(crate) fn content_cut(&self) -> ContentCut {
        ContentCut {
            epoch: if self.use_alternate {
                self.alternate_epoch.for_screen(true)
            } else {
                self.scrollback.epoch
            },
            revision: self.content_revision,
            first_row: if self.use_alternate {
                0
            } else {
                self.scrollback.first_row
            },
            screen_start: if self.use_alternate {
                0
            } else {
                self.scrollback
                    .first_row
                    .saturating_add(self.scrollback.total_len())
            },
            cols: self.cols,
            rows: self.rows,
            alternate: self.use_alternate,
        }
    }
}

impl Terminal {
    /// Metadata-only observation. Use with_view for a coordinate followed by row
    /// reads; separate calls cannot promise that the parser has not advanced.
    pub fn content_cut(&self) -> ContentCut {
        self.lock_state().content_cut()
    }

    /// Resolve a consumer-owned anchor and read content in one parser cut.
    pub fn with_view<R>(
        &self,
        viewport: &TerminalViewport,
        read: impl FnOnce(crate::TerminalReadView<'_>) -> R,
    ) -> R {
        let lock_requested = crate::lock_timing::requested();
        let state = self.lock_state();
        let lock_acquired = crate::lock_timing::acquired(lock_requested);
        let view = viewport.resolve(state.content_cut());
        let result = read(crate::TerminalReadView {
            state: &state,
            viewport: view,
        });
        drop(state);
        crate::lock_timing::released(
            crate::lock_timing::LockSite::View,
            lock_requested,
            lock_acquired,
        );
        result
    }

    /// Read the live grid/history without constructing or owning a local View.
    pub fn with_content<R>(&self, read: impl FnOnce(crate::TerminalReadView<'_>) -> R) -> R {
        self.with_view(&TerminalViewport::LIVE, read)
    }
}

impl crate::TerminalReadView<'_> {
    pub fn cut(&self) -> ContentCut {
        self.viewport.cut
    }
    pub fn viewport(&self) -> ViewportInfo {
        self.viewport
    }
    pub fn first_row(&self) -> usize {
        self.viewport.cut.first_row
    }
    pub fn screen_start(&self) -> usize {
        self.viewport.cut.screen_start
    }
    pub fn end_row(&self) -> usize {
        self.viewport.cut.end_row()
    }
    pub fn cols(&self) -> usize {
        self.viewport.cut.cols
    }
    pub fn rows(&self) -> usize {
        self.viewport.cut.rows
    }
    pub fn is_alternate_screen(&self) -> bool {
        self.viewport.cut.alternate
    }
    pub fn find_fake_cursor_cell(&self) -> Option<(usize, usize)> {
        self.state.find_fake_cursor_cell()
    }
    pub fn snapshot_as_vt(&self) -> Vec<u8> {
        self.state.snapshot_as_vt()
    }
    pub fn cached_cwd(&self) -> Option<&std::path::Path> {
        self.state.cached_cwd.as_deref()
    }
    pub fn cursor_position(&self) -> (usize, usize) {
        self.state.surface().cursor_position()
    }
    pub fn mouse_tracking(&self) -> crate::MouseTrackingMode {
        self.state.mouse_tracking()
    }
    pub fn sgr_mouse(&self) -> bool {
        self.state.sgr_mouse()
    }
    pub fn application_cursor_keys(&self) -> bool {
        self.state.application_cursor_keys()
    }
    pub fn screen_text(&self, include_dim: bool) -> String {
        self.state.screen_text(include_dim)
    }
    pub fn screen_text_lines(&self, n: usize, include_dim: bool) -> String {
        self.state.screen_text_lines(n, include_dim)
    }
    pub fn dim_omitted_rows(&self) -> usize {
        self.state.dim_omitted_rows()
    }
    pub fn screen_row(&self, row: usize, include_dim: bool) -> String {
        self.state.screen_row(row, include_dim)
    }
    pub fn screen_lines(&self) -> Vec<termwiz::surface::line::Line> {
        self.state
            .surface()
            .screen_lines()
            .into_iter()
            .map(|l| l.into_owned())
            .collect()
    }
    pub fn scrollback_line_owned(
        &self,
        row: usize,
    ) -> Option<Vec<(String, termwiz::cell::CellAttributes)>> {
        if self.is_alternate_screen() {
            return None;
        }
        self.state
            .scrollback_line_owned(row.checked_sub(self.first_row())?)
    }
    pub fn scrollback_line_wrapped(&self, row: usize) -> Option<bool> {
        if self.is_alternate_screen() {
            return None;
        }
        self.state
            .scrollback_line_wrapped(row.checked_sub(self.first_row())?)
    }
    pub fn scrollback_line_full(&self, row: usize) -> Option<crate::ScrollbackLine> {
        if self.is_alternate_screen() {
            return None;
        }
        self.state
            .scrollback_line_full(row.checked_sub(self.first_row())?)
    }
    /// UI search coordinates share the stable timeline used by selection/links.
    /// The existing Terminal::search remains a live-buffer relative read API.
    pub fn search(
        &self,
        query: &str,
        options: &crate::search::SearchOptions,
    ) -> Result<Vec<crate::search::SearchMatch>, crate::search::SearchError> {
        let mut matches = self
            .state
            .search_buffer(query, options, !self.is_alternate_screen())?;
        for found in &mut matches {
            found.row = found.row.saturating_add(self.first_row());
        }
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> Terminal {
        let mut terminal = Terminal::new_detached(8, 3);
        terminal.feed_bytes(b"0\r\n1\r\n2\r\n3\r\n4");
        terminal
    }
    fn top_text(terminal: &Terminal, viewport: &TerminalViewport) -> String {
        terminal.with_view(viewport, |view| {
            let row = view.viewport().top_row;
            if row < view.screen_start() {
                view.scrollback_line_full(row)
                    .unwrap()
                    .cells()
                    .map(|(s, _)| s)
                    .collect::<String>()
                    .trim_end()
                    .into()
            } else {
                view.screen_row(row - view.screen_start(), true)
            }
        })
    }

    #[test]
    fn two_display_owners_remain_independent_through_output_and_trim() {
        let mut terminal = filled();
        terminal.set_scrollback_limit(3);
        let mut reader = TerminalViewport::default();
        reader.scroll_up(terminal.content_cut(), 1);
        let live = TerminalViewport::default();
        assert_eq!(top_text(&terminal, &reader), "1");
        terminal.feed_bytes(b"\r\n5\r\n6");
        assert_eq!(top_text(&terminal, &reader), "1");
        assert_eq!(top_text(&terminal, &live), "4");
        terminal.feed_bytes(b"\r\n7");
        assert_eq!(
            top_text(&terminal, &reader),
            "2",
            "trim clamps only the removed anchor"
        );
        terminal.with_view(&reader, |view| {
            assert_eq!(view.first_row(), 2);
            assert!(
                view.scrollback_line_owned(1).is_none(),
                "removed IDs cannot name a different row"
            );
            assert_eq!(view.scroll_offset(), 3);
        });
    }

    #[test]
    fn disk_spill_keeps_the_anchor_readable_without_cloning_memory_rows() {
        let mut terminal = filled();
        terminal.enable_disk_scrollback(u32::MAX - 704);
        terminal.set_scrollback_limit(1);
        let mut viewport = TerminalViewport::LIVE;
        viewport.scroll_up(terminal.content_cut(), 2);
        terminal.with_view(&viewport, |view| {
            let disk = view.scrollback_line(view.viewport().top_row).unwrap();
            assert!(matches!(disk, std::borrow::Cow::Owned(_)));
            assert_eq!(disk.cells().next().unwrap().0, "0");
            assert!(matches!(
                view.scrollback_line(1),
                Some(std::borrow::Cow::Borrowed(_))
            ));
        });
        terminal.feed_bytes(b"\r\n5\r\n6\r\n7");
        terminal.with_view(&viewport, |view| {
            assert_eq!(view.first_row(), 0);
            assert_eq!(view.viewport().top_row, 0);
            assert_eq!(
                view.scrollback_line(0).unwrap().cells().next().unwrap().0,
                "0"
            );
        });
    }

    #[test]
    fn ed3_and_new_output_in_one_ingest_never_revive_the_old_anchor() {
        let mut terminal = filled();
        let mut viewport = TerminalViewport::default();
        viewport.scroll_up(terminal.content_cut(), 2);
        let old_epoch = terminal.content_cut().epoch;
        terminal.feed_bytes(b"\x1b[3J\r\n5\r\n6\r\n7\r\n8");
        terminal.with_view(&viewport, |view| {
            assert_ne!(view.cut().epoch, old_epoch);
            assert!(view.scrollback_len() > 0);
            assert_eq!(view.scroll_offset(), 0);
        });
        terminal.feed_bytes(b"\r\n9");
        assert_eq!(viewport.resolve(terminal.content_cut()).scroll_offset(), 0);
    }

    #[test]
    fn alternate_is_live_and_primary_anchor_returns_with_its_original_content() {
        let mut terminal = filled();
        let mut viewport = TerminalViewport::default();
        viewport.scroll_up(terminal.content_cut(), 1);
        let primary = terminal.content_cut();
        terminal.feed_bytes(b"\x1b[?1049hALT\r\n1\r\n2\r\n3");
        let alt = terminal.content_cut();
        assert_ne!(primary.epoch, alt.epoch);
        assert_eq!(viewport.resolve(alt).scroll_offset(), 0);
        viewport.scroll_up(alt, 50);
        terminal.feed_bytes(b"\x1b[?1049l");
        assert_eq!(top_text(&terminal, &viewport), "1");
        assert_eq!(terminal.content_cut().epoch, primary.epoch);
    }

    #[test]
    fn alternate_clear_reentry_invalidates_points_even_inside_one_batch() {
        let mut terminal = filled();
        terminal.feed_bytes(b"\x1b[?1049hOLD");
        let old = terminal.content_cut().epoch;
        terminal.feed_bytes(b"\x1b[?1049l\x1b[?1049hNEW");
        assert_ne!(old, terminal.content_cut().epoch);
        let retained = terminal.content_cut().epoch;
        terminal.feed_bytes(b"\x1b[?47l\x1b[?47h");
        assert_eq!(retained, terminal.content_cut().epoch);
        terminal.feed_bytes(b"\x1b[?1047l\x1b[?1047h");
        assert_eq!(retained, terminal.content_cut().epoch);
        terminal.feed_bytes(b"\x1bc\x1b[?47hRECREATED");
        assert_ne!(retained, terminal.content_cut().epoch);
    }

    #[test]
    fn retained_alternate_rows_and_search_ignore_changes_to_primary_history() {
        let mut terminal = filled();
        terminal.feed_bytes(b"\x1b[?47hALT-KEEP");
        let alt = terminal.content_cut();
        assert_eq!(alt.screen_start, 0);
        terminal.feed_bytes(b"\x1b[?47l\r\nPRIMARY-MATCH\r\n6\r\n7\x1b[?47h");
        terminal.with_content(|view| {
            assert_eq!(view.cut().epoch, alt.epoch);
            assert_eq!(view.viewport().top_row, 0);
            assert_eq!(view.screen_row(0, true), "ALT-KEEP");
            assert!(view.scrollback_line_full(0).is_none());
            let options = crate::search::SearchOptions::default();
            assert_eq!(view.search("ALT-KEEP", &options).unwrap()[0].row, 0);
            assert!(view.search("PRIMARY", &options).unwrap().is_empty());
        });
        assert!(
            !terminal
                .search("PRIMARY", &crate::search::SearchOptions::default())
                .unwrap()
                .is_empty(),
            "the public content search keeps its whole-buffer contract"
        );
    }

    #[test]
    fn resize_shrink_and_grow_keep_the_anchored_row() {
        let mut terminal = filled();
        let mut viewport = TerminalViewport::default();
        viewport.scroll_up(terminal.content_cut(), 1);
        terminal.resize(8, 2);
        assert_eq!(top_text(&terminal, &viewport), "1");
        terminal.resize(8, 4);
        assert_eq!(top_text(&terminal, &viewport), "1");
        viewport.scroll_to_bottom();
        assert_eq!(viewport.resolve(terminal.content_cut()).scroll_offset(), 0);
    }

    #[test]
    fn parser_output_trim_and_resize_cannot_cross_a_reader_cut() {
        use std::sync::{Arc, Barrier, mpsc};
        let terminal = filled();
        let mut viewport = TerminalViewport::default();
        viewport.scroll_up(terminal.content_cut(), 1);
        let state = Arc::clone(&terminal.state);
        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = Arc::clone(&barrier);
        let (tx, rx) = mpsc::channel();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            assert!(matches!(
                state.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            attempt_tx.send(()).unwrap();
            let mut state = state.lock().unwrap();
            state.set_scrollback_limit(2);
            state.ingest(b"\r\n5\r\n6\r\n7");
            state.resize_grid(8, 4);
            tx.send(()).unwrap();
        });
        terminal.with_view(&viewport, |view| {
            let cut = view.cut();
            barrier.wait();
            attempt_rx.recv().unwrap();
            assert!(
                rx.try_recv().is_err(),
                "writer cannot finish while reader holds this cut"
            );
            assert_eq!(view.cut(), cut);
            assert_eq!(view.viewport().top_row, 1);
            assert_eq!(
                view.scrollback_line_full(1)
                    .unwrap()
                    .cells()
                    .next()
                    .unwrap()
                    .0,
                "1"
            );
        });
        worker.join().unwrap();
        rx.recv().unwrap();
        terminal.with_view(&viewport, |view| {
            assert!(view.cut().revision > 1);
            assert!(view.first_row() > 1);
            assert_eq!(view.viewport().top_row, view.first_row());
            assert_eq!(view.rows(), 4);
        });
    }

    #[test]
    fn replacement_terminal_invalidates_an_owners_previous_anchor() {
        let terminal = filled();
        let mut viewport = TerminalViewport::default();
        viewport.scroll_up(terminal.content_cut(), 1);
        let replacement = filled();
        assert_eq!(
            viewport.resolve(replacement.content_cut()).scroll_offset(),
            0
        );
    }
}
