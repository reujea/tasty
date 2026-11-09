use std::collections::VecDeque;

use termwiz::cell::CellAttributes;

use crate::TerminalState;
use crate::disk_scrollback;
use termwiz::surface::{Change, Position};

/// A line stored as joined text, per-cell byte lengths, and runs of equal attributes.
/// This avoids retaining a separate String per cell. cells reconstructs borrowed cell views.
/// wrapped marks a presumed soft wrap for copy; capture uses a heuristic and may misclassify a full row.
#[derive(Debug, Clone)]
pub struct ScrollbackLine {
    pub(crate) text: String,
    pub(crate) cell_lens: Vec<u16>,
    pub(crate) attr_runs: Vec<(u32, CellAttributes)>,
    pub wrapped: bool,
}

impl ScrollbackLine {
    /// Compress owned cells in column order.
    pub fn new(cells: Vec<(String, CellAttributes)>, wrapped: bool) -> Self {
        Self::from_cells(cells.iter().map(|(s, a)| (s.as_str(), a)), wrapped)
    }

    /// Build from any iterator of borrowed `(grapheme, attrs)` cells in column
    /// order. This is the allocation-light path — producers can stream cells
    /// straight in without materializing per-cell `String`s.
    pub fn from_cells<'a, I>(cells: I, wrapped: bool) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a CellAttributes)>,
    {
        let mut builder = ScrollbackLineBuilder::new();
        for (s, a) in cells {
            builder.push(s, a);
        }
        builder.finish(wrapped)
    }

    /// Reassemble from raw compact parts (used by disk deserialization). If the
    /// data was truncated such that cells exist with no attribute run, a single
    /// default run is synthesized so [`cells`](Self::cells) stays panic-free and
    /// always has a run to borrow.
    pub(crate) fn from_raw_parts(
        text: String,
        cell_lens: Vec<u16>,
        mut attr_runs: Vec<(u32, CellAttributes)>,
        wrapped: bool,
    ) -> Self {
        if !cell_lens.is_empty() && attr_runs.is_empty() {
            attr_runs.push((cell_lens.len() as u32, CellAttributes::default()));
        }
        Self {
            text,
            cell_lens,
            attr_runs,
            wrapped,
        }
    }

    /// Number of cells (occupied columns) in this line.
    pub fn cell_count(&self) -> usize {
        self.cell_lens.len()
    }

    /// Iterate cells in column order as borrowed `(grapheme, attrs)` pairs.
    /// No per-cell allocation — the grapheme is sliced out of `text` and the
    /// attributes are borrowed from the RLE runs.
    pub fn cells(&self) -> CellsIter<'_> {
        CellsIter {
            text: &self.text,
            byte_pos: 0,
            lens: self.cell_lens.iter(),
            runs: &self.attr_runs,
            run_idx: 0,
            run_remaining: self.attr_runs.first().map(|(n, _)| *n).unwrap_or(0),
        }
    }

    /// Return owned cells for consumers such as search, selection, and IPC.
    pub fn to_cells(&self) -> Vec<(String, CellAttributes)> {
        self.cells()
            .map(|(s, a)| (s.to_string(), a.clone()))
            .collect()
    }

    /// True when no cell shows anything (used to trim trailing blank rows from snapshots).
    pub fn is_blank(&self) -> bool {
        self.cells().all(|(s, attrs)| cell_is_blank(s, attrs))
    }
}

/// True when a cell has no text and no attribute that marks an empty cell. Background color is
/// left out because erasing under a colored pen fills cells with it and those rows stay disposable.
pub(crate) fn cell_is_blank(text: &str, attrs: &CellAttributes) -> bool {
    text.chars().all(char::is_whitespace)
        && attrs.underline() == termwiz::cell::Underline::None
        && !attrs.strikethrough()
        && !attrs.overline()
        && !attrs.reverse()
}

/// Borrowing iterator over a [`ScrollbackLine`]'s cells, yielding
/// `(grapheme, attrs)` in column order without allocating.
pub struct CellsIter<'a> {
    text: &'a str,
    byte_pos: usize,
    lens: std::slice::Iter<'a, u16>,
    runs: &'a [(u32, CellAttributes)],
    run_idx: usize,
    run_remaining: u32,
}

impl<'a> Iterator for CellsIter<'a> {
    type Item = (&'a str, &'a CellAttributes);

    fn next(&mut self) -> Option<Self::Item> {
        let &len = self.lens.next()?;
        let start = self.byte_pos;
        let end = start + len as usize;
        // `get` (not direct slicing) guards against a corrupted disk line whose
        // recorded lengths don't land on char boundaries — fall back to empty.
        let s = self.text.get(start..end).unwrap_or("");
        self.byte_pos = end;
        // Advance past any exhausted runs to the run covering this cell.
        while self.run_remaining == 0 {
            self.run_idx += 1;
            self.run_remaining = self.runs.get(self.run_idx).map(|(n, _)| *n).unwrap_or(1);
        }
        let attrs = self
            .runs
            .get(self.run_idx)
            .map(|(_, a)| a)
            .unwrap_or(&self.runs[0].1);
        self.run_remaining -= 1;
        Some((s, attrs))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.lens.size_hint()
    }
}

impl ExactSizeIterator for CellsIter<'_> {}

/// Incremental builder for a [`ScrollbackLine`]. Cells are pushed one at a time
/// (`grapheme`, `attrs`) so producers can stream borrowed termwiz `CellRef`s —
/// whose `str()`/`attrs()` borrows cannot escape the per-cell scope — directly
/// into the compact form without an intermediate owned `Vec`.
pub struct ScrollbackLineBuilder {
    text: String,
    cell_lens: Vec<u16>,
    attr_runs: Vec<(u32, CellAttributes)>,
}

impl Default for ScrollbackLineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollbackLineBuilder {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cell_lens: Vec::new(),
            attr_runs: Vec::new(),
        }
    }

    /// Append one cell, coalescing into the current attribute run when its
    /// attributes match the previous cell's.
    pub fn push(&mut self, grapheme: &str, attrs: &CellAttributes) {
        self.text.push_str(grapheme);
        self.cell_lens.push(grapheme.len() as u16);
        match self.attr_runs.last_mut() {
            Some((run, a)) if a == attrs => *run += 1,
            _ => self.attr_runs.push((1, attrs.clone())),
        }
    }

    pub fn finish(self, wrapped: bool) -> ScrollbackLine {
        ScrollbackLine {
            text: self.text,
            cell_lens: self.cell_lens,
            attr_runs: self.attr_runs,
            wrapped,
        }
    }
}

/// Scrollback buffer with optional disk-backed storage.
pub(crate) struct Scrollback {
    lines: VecDeque<ScrollbackLine>,
    limit: usize,
    /// Content identity and retained coordinate origin; no display owner state.
    pub(crate) epoch: crate::ContentEpoch,
    pub(crate) first_row: usize,
    disk: Option<disk_scrollback::DiskScrollback>,
    /// Most rows the disk keeps; `None` keeps every spilled row. Rows past it are dropped
    /// oldest first, like the memory limit without disk swap.
    disk_limit: Option<usize>,
}

impl Scrollback {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            limit: 10000,
            epoch: crate::ContentEpoch::fresh(),
            first_row: 0,
            disk: None,
            disk_limit: None,
        }
    }

    /// Set the most rows the disk keeps (`None` = unlimited) and drop the oldest rows past it.
    pub fn set_disk_limit(&mut self, limit: Option<usize>) {
        self.disk_limit = limit;
        self.trim_disk();
    }

    /// Drop the oldest disk rows past the disk limit. Retained rows keep their row IDs.
    fn trim_disk(&mut self) {
        let (Some(cap), Some(ds)) = (self.disk_limit, &mut self.disk) else {
            return;
        };
        let excess = ds.line_count().saturating_sub(cap);
        if excess > 0 {
            let dropped = ds.drop_front(excess, cap);
            self.first_row = self.first_row.saturating_add(dropped);
        }
    }

    /// Set the scrollback buffer limit.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.flush_to_disk();
    }

    /// Enable disk-backed scrollback swap.
    pub fn enable_disk(&mut self, surface_id: u32) {
        if self.disk.is_none() {
            match disk_scrollback::DiskScrollback::new(surface_id) {
                Ok(ds) => self.disk = Some(ds),
                Err(e) => tracing::warn!("failed to create disk scrollback: {e}"),
            }
        }
    }

    /// Push lines oldest first, then spill the overflow to disk as one batch.
    /// The stored rows are the same as pushing the lines one call each, unless the disk
    /// write fails.
    pub fn push_lines(&mut self, lines: impl IntoIterator<Item = ScrollbackLine>) {
        self.lines.extend(lines);
        self.flush_to_disk();
    }

    /// Pop the most recent line from the back (for rows grow restoration).
    pub fn pop_back(&mut self) -> Option<ScrollbackLine> {
        self.lines.pop_back()
    }

    /// Number of lines in memory only (for resize operations).
    pub fn memory_len(&self) -> usize {
        self.lines.len()
    }

    /// Total number of lines (memory + disk, for public API).
    pub fn total_len(&self) -> usize {
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        disk_count + self.lines.len()
    }

    /// Borrow a specific scrollback line by index (0 = oldest, memory only).
    /// For disk-backed lines, use `line_owned()`.
    pub fn line(&self, index: usize) -> Option<&ScrollbackLine> {
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        if index < disk_count {
            None // Disk lines can't be returned as reference — use line_owned()
        } else {
            self.lines.get(index - disk_count)
        }
    }

    /// Get a scrollback line's cells by index, returning owned data.
    /// Works for both memory and disk-backed lines.
    pub fn line_owned(&self, index: usize) -> Option<Vec<(String, CellAttributes)>> {
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        if index < disk_count {
            self.disk
                .as_ref()
                .and_then(|ds| ds.read_line(index).ok().flatten())
                .map(|l| l.to_cells())
        } else {
            self.lines.get(index - disk_count).map(|l| l.to_cells())
        }
    }

    /// Return a complete line, including wrapped, from memory or disk.
    /// Keeping the compact representation avoids expanding and recompressing every cell.
    /// Cloning still copies the text, lengths, and attribute runs; cost depends on line size.
    pub fn line_full(&self, index: usize) -> Option<ScrollbackLine> {
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        if index < disk_count {
            self.disk
                .as_ref()
                .and_then(|ds| ds.read_line(index).ok().flatten())
        } else {
            self.lines.get(index - disk_count).cloned()
        }
    }

    /// Visit lines `range` (clamped to the total) oldest first. Disk rows are read with one
    /// open and sequential reads; a disk read error drops the rest of the disk part of the
    /// range, as per-row reads drop each unreadable row.
    pub fn for_each_line<'s>(
        &'s self,
        range: std::ops::Range<usize>,
        mut visit: impl FnMut(usize, std::borrow::Cow<'s, ScrollbackLine>),
    ) {
        let end = range.end.min(self.total_len());
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        if let Some(ds) = &self.disk
            && range.start < disk_count.min(end)
            && let Err(e) = ds.for_each_in_range(range.start..disk_count.min(end), |i, line| {
                visit(i, std::borrow::Cow::Owned(line))
            })
        {
            tracing::warn!("disk scrollback range read failed: {e}");
        }
        for index in range.start.max(disk_count)..end {
            if let Some(line) = self.lines.get(index - disk_count) {
                visit(index, std::borrow::Cow::Borrowed(line));
            }
        }
    }

    /// Exact length of `serialize_lines` over all scrollback rows, computed without reading
    /// the disk file. Disk rows count from the file size, memory rows from their parts.
    pub fn serialized_len(&self) -> u64 {
        let disk = self.disk.as_ref().map_or(0, |ds| ds.records_len());
        let memory: u64 = self
            .lines
            .iter()
            .map(|line| disk_scrollback::serialized_record_len(line) as u64)
            .sum();
        disk_scrollback::HEADER_LEN + disk + memory
    }

    /// Returns `true` if the line at `index` was soft-wrapped (auto-wrap at
    /// the right edge of the terminal), meaning the next line is a logical
    /// continuation rather than a hard newline.
    pub fn line_wrapped(&self, index: usize) -> Option<bool> {
        let disk_count = self.disk.as_ref().map(|ds| ds.line_count()).unwrap_or(0);
        if index < disk_count {
            self.disk
                .as_ref()
                .and_then(|ds| ds.read_line(index).ok().flatten())
                .map(|l| l.wrapped)
        } else {
            self.lines.get(index - disk_count).map(|l| l.wrapped)
        }
    }

    /// Discard all scrollback (memory + disk) and snap the viewport back to
    /// live. Used by ED3 (`CSI 3J`) erase-scrollback.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.epoch = crate::ContentEpoch::fresh();
        self.first_row = 0;
        if let Some(ds) = &mut self.disk
            && let Err(e) = ds.clear()
        {
            tracing::warn!("disk scrollback clear failed: {e}");
        }
    }

    /// Move lines over the limit to disk in one write, or drop them without disk swap.
    pub(crate) fn flush_to_disk(&mut self) {
        let excess = self.lines.len().saturating_sub(self.limit);
        if excess == 0 {
            return;
        }
        let Some(ds) = &mut self.disk else {
            self.lines.drain(..excess);
            self.first_row = self.first_row.saturating_add(excess);
            return;
        };
        let mut spilled: Vec<ScrollbackLine> = self.lines.drain(..excess).collect();
        if let Some(cap) = self.disk_limit {
            // Rows the cap would drop right after writing are dropped before it, so one large
            // batch never grows the file or the offset table past the cap.
            let unwritten = spilled.len().saturating_sub(cap);
            if unwritten > 0 {
                spilled.drain(..unwritten);
                let disk_rows = ds.line_count();
                let dropped = ds.drop_front(disk_rows, cap);
                self.first_row = self
                    .first_row
                    .saturating_add(dropped)
                    .saturating_add(unwritten);
            }
        }
        if let Err(e) = ds.push_lines(&spilled) {
            // The whole batch is lost between the retained disk and memory ranges; the disk
            // keeps its earlier rows. A new content epoch prevents anchors aliasing rows.
            self.epoch = crate::ContentEpoch::fresh();
            self.first_row = 0;
            tracing::warn!("disk scrollback push of {excess} rows failed: {e}");
            return;
        }
        self.trim_disk();
    }
}

impl TerminalState {
    /// Set the scrollback buffer limit.
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback.set_limit(limit);
        self.content_revision = self.content_revision.wrapping_add(1);
    }

    /// Enable disk-backed scrollback swap for this terminal.
    pub fn enable_disk_scrollback(&mut self, surface_id: u32) {
        self.scrollback.enable_disk(surface_id);
    }

    /// Set the most rows disk swap keeps (`None` = unlimited). Rows past it are dropped oldest
    /// first and the retained rows keep their row IDs.
    pub fn set_disk_scrollback_limit(&mut self, limit: Option<usize>) {
        self.scrollback.set_disk_limit(limit);
        self.content_revision = self.content_revision.wrapping_add(1);
    }

    /// Number of lines in the scrollback buffer (memory + disk).
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.total_len()
    }

    /// Erase all scrollback history (ED3 / `CSI 3J`). Clears the memory + disk
    /// buffers and snaps the user viewport to live. The visible screen and its
    /// `saved_line_tails` (per-row reflow data) are left untouched — ED3 only
    /// touches scrollback. `restorable_scrollback_count` is reset because the
    /// resize "restore debt" lines it counted have just been discarded; leaving
    /// it set would let a later rows-grow wrongly pull fresh scrollback content
    /// back onto the screen.
    pub(crate) fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.restorable_scrollback_count = 0;
    }

    /// Get a specific scrollback line by index (0 = oldest, memory only).
    /// For disk-backed lines, use scrollback_line_owned().
    pub fn scrollback_line(&self, index: usize) -> Option<&crate::ScrollbackLine> {
        self.scrollback.line(index)
    }

    /// Get a scrollback line by index, returning owned data.
    /// Works for both memory and disk-backed lines.
    pub fn scrollback_line_owned(&self, index: usize) -> Option<Vec<(String, CellAttributes)>> {
        self.scrollback.line_owned(index)
    }

    /// Returns whether the scrollback line at `index` ends in a soft wrap
    /// (auto-wrap at the right edge). Used by selection extraction to rejoin
    /// wrapped lines on copy. Returns `None` if `index` is out of range.
    pub fn scrollback_line_wrapped(&self, index: usize) -> Option<bool> {
        self.scrollback.line_wrapped(index)
    }

    /// Get a full scrollback line by index (cells + wrapped flag).
    pub fn scrollback_line_full(&self, index: usize) -> Option<crate::ScrollbackLine> {
        self.scrollback.line_full(index)
    }

    /// 스크롤백을 오래된 순서로 복제한다. Terminal 핸들 경유 시 전체를 한 번의 락으로 처리한다.
    pub fn scrollback_lines_all(&self) -> Vec<crate::ScrollbackLine> {
        let total = self.scrollback_len();
        let mut out = Vec::with_capacity(total);
        self.scrollback
            .for_each_line(0..total, |_, line| out.push(line.into_owned()));
        out
    }

    /// Length `serialize_lines(&self.scrollback_lines_all())` would have, without reading
    /// disk rows. Persistence uses it to reject oversized history before copying it.
    pub fn scrollback_serialized_len(&self) -> u64 {
        self.scrollback.serialized_len()
    }

    /// Snapshot the current visible screen as `ScrollbackLine` records.
    /// Trailing blank rows (모두 공백 / 빈 cell) 은 잘라낸다.
    ///
    /// 사용처: layout 저장 시 scrollback 라인 뒤에 이어 붙여 디스크에 저장한다.
    /// 복원 시 `inject_scrollback` 으로 함께 push 하면 다음 세션에서 위로
    /// 스크롤할 때 이전 화면이 그대로 보인다 (현재 PTY 의 새 prompt 는 그 아래
    /// 에서 시작).
    pub fn screen_snapshot_lines(&self) -> Vec<crate::ScrollbackLine> {
        let surface = self.surface();
        let cols = self.cols;
        let lines = surface.screen_lines();
        let mut result: Vec<crate::ScrollbackLine> = Vec::with_capacity(lines.len());
        for line in lines.iter() {
            let wrapped = Self::line_was_soft_wrapped(line, cols);
            let mut builder = crate::scrollback::ScrollbackLineBuilder::new();
            for cell in line.visible_cells() {
                builder.push(cell.str(), cell.attrs());
            }
            result.push(builder.finish(wrapped));
        }
        // Trim trailing blank rows: cells 가 비거나 모두 whitespace-only.
        while let Some(last) = result.last() {
            if last.is_blank() {
                result.pop();
            } else {
                break;
            }
        }
        result
    }

    /// Inject scrollback lines (oldest first) into this terminal's scrollback buffer.
    /// Used to restore scrollback after recreating a terminal (closed-item / layout restore).
    pub fn inject_scrollback(&mut self, lines: Vec<crate::ScrollbackLine>) {
        self.content_revision = self.content_revision.wrapping_add(1);
        self.scrollback.push_lines(lines);
    }

    /// Pop up to `count` lines from the back of scrollback and draw them at the
    /// top of the visible screen, then place the cursor on the row immediately
    /// below them. Returns the number of lines actually drawn (saturates when
    /// scrollback is shorter than `count` or `count` exceeds available rows).
    ///
    /// 복원 직후 호출용 — 사용자가 "위에 더 있다" 는 사실을 인지하도록 visible
    /// 영역 상단에 옛 라인을 미리 보여주고 새 prompt 가 그 아래에서 시작하게
    /// 만든다. PTY 가 첫 prompt 를 출력하기 전에 호출해야 한다.
    pub fn prefill_visible_from_scrollback(&mut self, count: usize) -> usize {
        self.content_revision = self.content_revision.wrapping_add(1);
        use termwiz::surface::Position;

        let count = count
            .min(self.scrollback.memory_len())
            .min(self.rows.saturating_sub(1));
        if count == 0 {
            return 0;
        }

        let mut to_draw: Vec<crate::ScrollbackLine> = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(line) = self.scrollback.pop_back() {
                to_draw.push(line);
            }
        }
        to_draw.reverse(); // oldest first

        for (row, line) in to_draw.iter().enumerate() {
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

        // The loop above emits `AllAttributes` per restored cell directly on the
        // surface, leaving its pen at the last cell's attrs (a restoration
        // artifact that bypasses `mirror_pen`). Re-apply the logical pen so the
        // surface pen stays aligned with `current_pen`.
        self.primary_surface
            .add_change(Change::AllAttributes(self.current_pen.clone()));

        // Park the cursor on the row right after the prefilled block so the
        // shell's first prompt is emitted there.
        let cursor_y = to_draw.len();
        self.primary_surface.add_change(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(cursor_y),
        });

        to_draw.len()
    }

    /// Capture rows before scrolling. termwiz does not retain a wrap flag here, so a
    /// nonblank rightmost cell is treated as a soft wrap. A hard newline after a full row
    /// can be misclassified and cause copied lines to join.
    pub(crate) fn capture_top_lines(&self, count: usize) -> Vec<crate::scrollback::ScrollbackLine> {
        let surface = self.surface();
        let cols = self.cols;
        let lines = surface.screen_lines();
        let mut result = Vec::new();
        for line in lines.iter().take(count.min(lines.len())) {
            let wrapped = Self::line_was_soft_wrapped(line, cols);
            let mut builder = crate::scrollback::ScrollbackLineBuilder::new();
            for cell in line.visible_cells() {
                builder.push(cell.str(), cell.attrs());
            }
            result.push(builder.finish(wrapped));
        }
        result
    }

    /// Push lines that scrolled off the top into scrollback and keep the
    /// viewport-relative bookkeeping consistent (line tails + scroll offset).
    pub(crate) fn push_scrolled_off(&mut self, captured: Vec<crate::scrollback::ScrollbackLine>) {
        let count = captured.len();
        self.scrollback.push_lines(captured);
        // Shift saved_line_tails: remove top entries that scrolled off
        for _ in 0..count.min(self.saved_line_tails.len()) {
            self.saved_line_tails.remove(0);
        }
    }

    /// Inspect a change and capture scrollback lines before it's applied.
    /// Only the explicit `ScrollRegionUp` path is handled here; the implicit
    /// scroll caused by auto-wrapping text is handled in
    /// [`TerminalState::apply_text_honoring_scroll_region`] because the evicted
    /// rows are produced mid-apply and would not exist in a pre-apply snapshot.
    pub(crate) fn capture_before_scroll(&mut self, change: &Change) {
        if let Change::ScrollRegionUp {
            first_row: 0,
            region_size,
            scroll_count,
        } = change
        {
            // Capture at most region_size rows: a larger scroll count must not copy
            // rows below the region that remain visible.
            let count = (*scroll_count).min(*region_size);
            let captured = self.capture_top_lines(count);
            self.push_scrolled_off(captured);
        }
    }

    /// Apply text within the DECSTBM region and capture rows evicted from the top.
    /// termwiz scrolls the whole screen internally, so split text where scrolling occurs
    /// and capture each row after earlier text has populated it. For a partial region,
    /// scroll explicitly and reposition before printing the next grapheme.
    /// Apply region limits on both screens, but never add alternate-screen rows to history.
    pub(crate) fn apply_text_honoring_scroll_region(&mut self, text: String) {
        // Same bounds the explicit-newline path uses (`perform_index`), so an
        // auto-wrap and an LF at the same row scroll the same rows.
        let (region_top, region_size) = self.scroll_region_params();
        let breaks = self.text_breaks(&text, region_top, region_size);
        if breaks.is_empty() {
            self.surface_mut().add_change(Change::Text(text));
            return;
        }
        // A non-empty break list means the grid has rows, so the region does too.
        let region_bottom = region_top + region_size - 1;

        let mut prev = 0usize;
        for brk in breaks {
            if brk.at > prev {
                self.surface_mut()
                    .add_change(Change::Text(text[prev..brk.at].to_string()));
            }
            match brk.kind {
                BreakKind::SurfaceScroll => {
                    // Capture before termwiz's full-screen scroll, except on the alternate
                    // screen, whose output must not alter primary history or saved tails.
                    if !self.use_alternate {
                        let captured = self.capture_top_lines(1);
                        self.push_scrolled_off(captured);
                    }
                }
                BreakKind::RegionScroll => {
                    let scroll = Change::ScrollRegionUp {
                        first_row: region_top,
                        region_size,
                        scroll_count: 1,
                    };
                    // Only a region anchored at row 0 evicts rows off the top of
                    // the screen; `capture_before_scroll` enforces that, and an
                    // inner region's pushed-out rows must not reach history.
                    if !self.use_alternate {
                        self.capture_before_scroll(&scroll);
                    }
                    self.surface_mut().add_change(scroll);
                    if brk.reset_column {
                        // termwiz's `scroll_region_up` leaves the cursor where it
                        // was, i.e. still parked past the right edge; without this
                        // the withheld grapheme would wrap a second time and land
                        // below the region after all.
                        self.surface_mut().add_change(Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(region_bottom),
                        });
                    }
                }
                BreakKind::Clamp => {
                    // Index outside the margins clamps at the last screen row and
                    // never scrolls (VT100 DECSTBM). Nothing moves; only the
                    // parked column is released so the withheld grapheme
                    // overwrites this row instead of scrolling the screen.
                    if brk.reset_column {
                        self.surface_mut().add_change(Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Relative(0),
                        });
                    }
                }
            }
            prev = brk.at + brk.skip;
        }
        if prev < text.len() {
            self.surface_mut()
                .add_change(Change::Text(text[prev..].to_string()));
        }
    }

    /// Simulate termwiz `print_text` cursor advancement to find every grapheme
    /// whose application moves the cursor off its row, and classify what tasty
    /// must do about it. Mirrors termwiz's deferred-wrap logic exactly (same
    /// grapheme segmentation and column width) so the offsets line up with the
    /// real prints.
    fn text_breaks(&self, text: &str, region_top: usize, region_size: usize) -> Vec<TextBreak> {
        use finl_unicode::grapheme_clusters::Graphemes;

        let mut breaks = Vec::new();
        let width = self.cols;
        let height = self.rows;
        if width == 0 || height == 0 {
            return breaks;
        }
        // `scroll_region_params` never reports an empty region on a non-empty
        // grid — no region means the whole surface, and DECSTBM is normalized to
        // at least one row when it is stored.
        let region_bottom = region_top + region_size - 1;
        let full_screen = region_top == 0 && region_bottom + 1 >= height;
        // Classify the row move the cursor is about to make. `None` = termwiz's
        // own plain "move down one row" is already correct.
        let classify = |ypos: usize| -> Option<BreakKind> {
            if !full_screen && ypos == region_bottom {
                Some(BreakKind::RegionScroll)
            } else if ypos + 1 >= height {
                Some(if full_screen {
                    BreakKind::SurfaceScroll
                } else {
                    BreakKind::Clamp
                })
            } else {
                None
            }
        };

        let (mut xpos, mut ypos) = self.surface().cursor_position();
        let mut byte = 0usize;
        for g in Graphemes::new(text) {
            match g {
                "\r" => xpos = 0,
                "\n" | "\r\n" => {
                    let reset_column = g == "\r\n";
                    match classify(ypos) {
                        // termwiz's `scroll_screen_up` is the right move here, so
                        // the newline is left in the stream (`skip: 0`).
                        Some(BreakKind::SurfaceScroll) => breaks.push(TextBreak {
                            at: byte,
                            skip: 0,
                            reset_column,
                            kind: BreakKind::SurfaceScroll,
                        }),
                        // termwiz would leave the region / scroll the whole grid,
                        // so the newline is consumed here instead.
                        Some(kind) => breaks.push(TextBreak {
                            at: byte,
                            skip: g.len(),
                            reset_column,
                            kind,
                        }),
                        None => ypos += 1,
                    }
                    if reset_column {
                        xpos = 0;
                    }
                }
                _ => {
                    if xpos >= width {
                        match classify(ypos) {
                            Some(kind) => breaks.push(TextBreak {
                                at: byte,
                                skip: 0,
                                reset_column: true,
                                kind,
                            }),
                            None => ypos += 1,
                        }
                        xpos = 0;
                    }
                    xpos += termwiz::cell::grapheme_column_width(g, None).max(1);
                }
            }
            byte += g.len();
        }
        breaks
    }
}

/// One point inside a `Change::Text` where the cursor leaves the row it is on
/// and termwiz's unconditional behavior has to be corrected or observed.
#[derive(Debug, Clone, Copy)]
struct TextBreak {
    /// Byte offset of the grapheme that triggers the move.
    at: usize,
    /// Bytes consumed by the break itself — the newline tasty performs on
    /// termwiz's behalf. `0` for a deferred auto-wrap, whose grapheme is still
    /// printed normally once the cursor has been repositioned.
    skip: usize,
    /// Whether the cursor returns to column 0 (auto-wrap and `\r\n`; a bare
    /// `\n` keeps its column).
    reset_column: bool,
    kind: BreakKind,
}

/// What must happen at a [`TextBreak`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakKind {
    /// Full-screen region with the cursor on the last row: termwiz's own
    /// `scroll_screen_up` is exactly the region scroll, so only the row it is
    /// about to evict needs snapshotting.
    SurfaceScroll,
    /// Cursor on a partial region's bottom row: termwiz would write below the
    /// region, so the region scroll is emitted explicitly instead.
    RegionScroll,
    /// Cursor below the region on the last screen row: neither the region nor
    /// the screen may scroll, so the move is dropped and the cursor stays.
    Clamp,
}

#[cfg(test)]
impl Scrollback {
    pub(crate) fn disk_read_opens(&self) -> usize {
        self.disk.as_ref().map_or(0, |disk| disk.read_opens())
    }

    pub(crate) fn disk_push_batches(&self) -> usize {
        self.disk.as_ref().map_or(0, |disk| disk.push_batches())
    }
}

#[cfg(test)]
#[path = "scrollback_disk_write_tests.rs"]
mod disk_write_tests;

#[cfg(test)]
#[path = "scrollback_disk_read_tests.rs"]
mod disk_read_tests;

#[cfg(test)]
#[path = "scrollback_disk_cap_tests.rs"]
mod disk_cap_tests;

#[cfg(test)]
mod viewport_tests {
    use super::*;

    #[test]
    fn lost_disk_spill_invalidates_the_content_timeline() {
        let id = u32::MAX - 703;
        let mut history = Scrollback::new();
        history.enable_disk(id);
        assert!(history.disk.is_some());
        history.disk.as_mut().unwrap().fail_append_after(0);
        history.set_limit(0);
        let epoch = history.epoch;
        history.push_lines([ScrollbackLine::new(
            vec![("lost".into(), CellAttributes::default())],
            false,
        )]);
        assert_eq!(history.total_len(), 0);
        assert_ne!(history.epoch, epoch);
    }
}
