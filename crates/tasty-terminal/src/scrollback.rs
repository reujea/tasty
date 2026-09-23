use std::collections::VecDeque;

use termwiz::cell::CellAttributes;

use crate::TerminalState;
use crate::disk_scrollback;
use termwiz::surface::{Change, Position};

/// One scrollback line, stored column-compactly to avoid a heap allocation per
/// cell.
///
/// The naive `Vec<(String, CellAttributes)>` form allocates one `String` plus
/// one `CellAttributes` clone *per column*; a full scrollback (10k lines ×
/// ~180 columns) turns into millions of tiny allocations. Instead a line keeps:
/// - `text`: every cell's grapheme concatenated into a single buffer,
/// - `cell_lens`: the byte length of each cell's grapheme within `text` (so
///   exact per-cell boundaries are preserved, including multi-byte graphemes),
/// - `attr_runs`: run-length-encoded attributes — adjacent cells that share the
///   same `CellAttributes` collapse into one `(run_len, attrs)` entry.
///
/// This makes the allocation count per line constant (3 headers) instead of
/// column-proportional, while reproducing the original cell stream exactly via
/// [`ScrollbackLine::cells`].
///
/// `wrapped == true` means this line was scrolled off because the terminal
/// auto-wrapped at the right edge — i.e. the next line is a logical
/// continuation, not a hard newline. Used for wrap-aware copy.
#[derive(Debug, Clone)]
pub struct ScrollbackLine {
    pub(crate) text: String,
    pub(crate) cell_lens: Vec<u16>,
    pub(crate) attr_runs: Vec<(u32, CellAttributes)>,
    pub wrapped: bool,
}

impl ScrollbackLine {
    /// Build from the legacy owned `Vec<(String, CellAttributes)>` form (cells
    /// in column order). Kept for producers and tests that still hand over owned
    /// cells; the cells are compressed on the way in.
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

    /// Reconstruct the legacy owned `Vec<(String, CellAttributes)>` form. Used
    /// by consumers that still need owned cells (search, selection, IPC); the
    /// allocation is transient, not retained in the scrollback buffer.
    pub fn to_cells(&self) -> Vec<(String, CellAttributes)> {
        self.cells()
            .map(|(s, a)| (s.to_string(), a.clone()))
            .collect()
    }

    /// True when every cell is empty or whitespace-only (used to trim trailing
    /// blank rows from snapshots).
    pub fn is_blank(&self) -> bool {
        self.cells()
            .all(|(s, _)| s.is_empty() || s.chars().all(char::is_whitespace))
    }
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
    /// Current scroll offset (0 = at bottom/live, >0 = scrolled up).
    pub scroll_offset: usize,
    disk: Option<disk_scrollback::DiskScrollback>,
}

impl Scrollback {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            limit: 10000,
            scroll_offset: 0,
            disk: None,
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

    /// Push a line to the back of the scrollback buffer.
    pub fn push_line(&mut self, line: ScrollbackLine) {
        self.lines.push_back(line);
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

    /// Scroll up (towards older content).
    pub fn scroll_up(&mut self, count: usize) {
        let max = self.total_len();
        self.scroll_offset = (self.scroll_offset + count).min(max);
    }

    /// Scroll down (towards newer/live content).
    pub fn scroll_down(&mut self, count: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(count);
    }

    /// Reset scroll position to the bottom (live view).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
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

    /// Get a full line (cells + `wrapped`) by index as an owned
    /// [`ScrollbackLine`]. Works for both memory and disk-backed lines.
    ///
    /// 벌크 캡처(닫은 항목 스냅샷 / layout 영속화)가 라인마다 부르는 경로다.
    /// `line_owned` + `line_wrapped` 를 따로 부르면 두 배로 비싸진다:
    /// 메모리 라인은 `to_cells` 가 cell 마다 `String` 을 새로 할당하고
    /// `CellAttributes` 를 복제한 뒤 곧바로 같은 RLE 표현으로 재압축되고,
    /// 디스크 라인은 같은 인덱스를 두 번 읽어 `File::open` 이 2배가 된다.
    /// 저장 표현이 이미 `ScrollbackLine` 이므로 여기서는 그대로 돌려준다 —
    /// 메모리 라인의 비용은 헤더 3개 복제로 고정된다(cell 수 비례가 아니다).
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
        self.scroll_offset = 0;
        if let Some(ds) = &mut self.disk
            && let Err(e) = ds.clear()
        {
            tracing::warn!("disk scrollback clear failed: {e}");
        }
    }

    /// Flush excess scrollback lines to disk (if disk swap is enabled).
    pub(crate) fn flush_to_disk(&mut self) {
        while self.lines.len() > self.limit {
            if let Some(ds) = &mut self.disk {
                if let Some(line) = self.lines.pop_front()
                    && let Err(e) = ds.push_lines(&[line])
                {
                    tracing::warn!("disk scrollback push failed: {e}");
                }
            } else {
                self.lines.pop_front();
            }
        }
    }
}

impl TerminalState {
    /// Current scroll offset (0 = at bottom/live, >0 = scrolled up).
    pub fn scroll_offset(&self) -> usize {
        self.scrollback.scroll_offset
    }

    /// Set the scrollback buffer limit.
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback.set_limit(limit);
    }

    /// Enable disk-backed scrollback swap for this terminal.
    pub fn enable_disk_scrollback(&mut self, surface_id: u32) {
        self.scrollback.enable_disk(surface_id);
    }

    /// Scroll up (towards older content).
    pub fn scroll_up(&mut self, lines: usize) {
        self.scrollback.scroll_up(lines);
    }

    /// Scroll down (towards newer/live content).
    pub fn scroll_down(&mut self, lines: usize) {
        self.scrollback.scroll_down(lines);
    }

    /// Reset scroll position to the bottom (live view).
    pub fn scroll_to_bottom(&mut self) {
        self.scrollback.scroll_to_bottom();
    }

    /// Set scroll offset directly (for search navigation).
    /// Clamped to [0, scrollback_len].
    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max = self.scrollback.total_len();
        self.scrollback.scroll_offset = offset.min(max);
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

    /// 스크롤백 전량을 인덱스 순서(0 = 가장 오래된 라인)로 회수한다.
    ///
    /// 캡처 전용 벌크 경로다 — 호출자가 `0..scrollback_len()` 을 돌며
    /// [`scrollback_line_full`](Self::scrollback_line_full) 을 부르는 것과 결과는
    /// 같지만, `Terminal` 핸들(`handle.rs`) 경유 시 라인마다 잡히는 state mutex 를 한 번으로
    /// 줄인다. 그 mutex 는 PTY 파서 스레드가 `ingest` 로 잡는 것과 같은 것이라,
    /// 라인당 lock 은 만재 스크롤백에서 파서와 수만 회 경합한다.
    pub fn scrollback_lines_all(&self) -> Vec<crate::ScrollbackLine> {
        let total = self.scrollback_len();
        let mut out = Vec::with_capacity(total);
        for i in 0..total {
            if let Some(line) = self.scrollback.line_full(i) {
                out.push(line);
            }
        }
        out
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
        for line in lines {
            self.scrollback.push_line(line);
        }
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

    /// Capture the top line(s) from the surface before a scroll change is applied.
    ///
    /// Each captured line is tagged with a `wrapped` flag so the next line is
    /// known to be a logical continuation (used by wrap-aware copy). termwiz
    /// `Surface::print_text` does NOT set its own wrap bit when the cursor
    /// runs off the right edge — it just advances `ypos` — so we recover the
    /// flag heuristically: a line is treated as soft-wrapped when its
    /// rightmost cell is occupied by a non-space grapheme. Lines that ended in
    /// a real `\n` almost always have trailing whitespace; lines that wrapped
    /// at the right edge filled the last column. False positives (a hard
    /// newline that happened to fill the row) merge two lines on copy, which
    /// is a strictly better outcome than the prior unconditional `\n` join.
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
        for line in captured {
            self.scrollback.push_line(line);
        }
        // Shift saved_line_tails: remove top entries that scrolled off
        for _ in 0..count.min(self.saved_line_tails.len()) {
            self.saved_line_tails.remove(0);
        }
        // Compensate scroll_offset so the user's viewport stays in place
        if self.scrollback.scroll_offset > 0 {
            self.scrollback.scroll_offset += count;
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
            // termwiz `scroll_region_up` evicts at most `region_size` rows. When
            // `scroll_count` exceeds the region (a top-anchored partial region with
            // an over-sized SU), capturing `scroll_count` rows would also copy rows
            // *below* the region — which stay on screen — producing duplicates in
            // scrollback. Clamp to the region so only genuinely evicted top rows
            // are captured (see ADR/verification: E2). The clamp lives here at the
            // callsite, not in `capture_top_lines`, because the auto-wrap path
            // (`apply_text_honoring_scroll_region`) calls `capture_top_lines(1)`
            // and is region-agnostic.
            let count = (*scroll_count).min(*region_size);
            let captured = self.capture_top_lines(count);
            self.push_scrolled_off(captured);
        }
    }

    /// Apply a `Change::Text` so that it honors the active DECSTBM scroll
    /// region, capturing every row that scrolls off the top on the way.
    ///
    /// termwiz `Surface::print_text` knows nothing about scroll regions: it
    /// advances the cursor one row per auto-wrap / newline and, once past the
    /// **last screen row**, scrolls the *whole* grid internally (no
    /// `ScrollRegionUp` Change) and discards the evicted top line. Two separate
    /// problems follow from that, and this is the single place both are fixed:
    ///
    /// 1. **Scrollback.** Tasty owns the scrollback, so every eviction must be
    ///    observed. The evicted content is generated within the same
    ///    `add_change`, so a pre-apply snapshot is empty — instead the text is
    ///    split at the exact byte offsets where a scroll will occur, each
    ///    segment is applied, and the (now-populated) top row is snapshotted
    ///    right before the scroll consumes it.
    /// 2. **Region containment.** With a partial region the cursor reaching the
    ///    region bottom must scroll *the region*, not walk below it. Left to
    ///    termwiz, an auto-wrapped long line at the region bottom writes over
    ///    the rows the application reserved outside the region (a TUI's input
    ///    bar), and the rows pushed up out of the region are never handed to
    ///    scrollback — the history simply disappears. So the region scroll is
    ///    emitted explicitly here and the offending grapheme is withheld from
    ///    termwiz until the cursor has been repositioned.
    ///
    /// Runs for both screens. On the alternate screen the region containment
    /// still applies, but nothing is captured — alt-screen output must not
    /// leak into the primary screen's history.
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
                    // termwiz performs this scroll itself when the next segment
                    // is applied — snapshot the row it is about to consume.
                    //
                    // The alternate screen is the common case for this branch, not
                    // an exotic one: a full-screen TUI sets no DECSTBM, so every
                    // alt-screen auto-wrap at the last row lands here. Capturing
                    // would read the *alt* surface (`capture_top_lines` follows
                    // `surface()`) into the single primary scrollback, and
                    // `push_scrolled_off` would also shift the primary's
                    // `saved_line_tails` and the user's `scroll_offset`.
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
