use std::sync::atomic::{AtomicU64, Ordering};

use crate::output_origins::{ChunkOrigin, OriginSpans};

/// Raw output retention limit (1 MiB). A read cannot cover more raw bytes than this.
/// max_bytes may request less; decoded text can be longer than the raw bytes.
pub const OUTPUT_RETENTION_MAX_BYTES: usize = 1_048_576;

/// Disambiguates two streams created inside the same nanosecond.
static NEXT_STREAM_SEQ: AtomicU64 = AtomicU64::new(0);

/// Combine a timestamp and a process counter to identify an output stream.
/// A terminal respawn keeps its surface ID but gets a new stream token, so an old
/// cursor cannot silently read the replacement's output. The counter distinguishes
/// streams created at the same timestamp; the timestamp reduces reuse across restarts.
fn mint_stream_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = NEXT_STREAM_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{seq:x}")
}

/// Where a read starts.
pub enum OutputCursor {
    /// The terminal's shared server-held mark. If trimming has passed it, report skipped bytes.
    /// Without a mark, start at the oldest retained byte and report skipped=0 even if
    /// earlier output was trimmed. Consumers needing their own loss history should use At.
    Mark,
    /// An absolute position the consumer holds. The server keeps no state for
    /// it, so two consumers reading this way never move each other.
    At(u64),
}

/// What a consumer asks for.
pub struct OutputReadRequest {
    pub from: OutputCursor,
    /// Upper bound on the **raw** bytes this read covers — not on the length of
    /// the text that comes back. Clamped to `1..=OUTPUT_RETENTION_MAX_BYTES`.
    pub max_bytes: usize,
    pub strip_ansi: bool,
    /// The stream the cursor was taken from. `Some` makes a stale cursor an
    /// error instead of a silent read of another stream's bytes.
    pub expect_stream: Option<String>,
}

/// One output read. Cursor positions count raw bytes, not text.len(): lossy UTF-8
/// decoding can expand bytes, while ANSI stripping removes them. Continue at next_cursor.
#[derive(Debug)]
pub struct OutputRead {
    pub text: String,
    /// Raw bytes this read covered.
    pub raw_bytes: usize,
    /// Where this read actually started. Differs from the requested position
    /// exactly when `skipped` is not zero.
    pub cursor: u64,
    /// Next raw byte position. At retention_end the reader has caught up and reads
    /// empty output until more bytes arrive. This position alone does not indicate process exit.
    pub next_cursor: u64,
    /// Oldest retained position.
    pub retention_start: u64,
    /// One past the newest retained position.
    pub retention_end: u64,
    /// Raw bytes that fell out of retention between the requested position and
    /// where the read had to start. Not zero means output was lost for good.
    pub skipped: u64,
    /// This stream's token. Carry it back with the next cursor.
    pub stream: String,
}

/// One scanner-cursor read: the same continuity values as [`OutputRead`] plus, when
/// requested and decidable, where each byte of `read.text` came from.
#[derive(Debug)]
pub struct ScanRead {
    pub read: OutputRead,
    /// `None` when origins were not requested, when no retained byte of this read has
    /// a decidable origin, or when the description would exceed
    /// [`crate::TEXT_ORIGINS_MAX_ENTRIES`] entries.
    pub origins: Option<crate::TextOrigins>,
}

/// What the scanner read copies under the terminal state lock. Decoding, ANSI
/// stripping and the origin mapping run on it after the lock is released.
pub(crate) struct ScanSnapshot {
    raw: Vec<u8>,
    spans: Option<Vec<(u64, ChunkOrigin)>>,
    cursor: u64,
    skipped: u64,
    retention_start: u64,
    retention_end: u64,
    stream: String,
}

impl ScanSnapshot {
    /// Build the text the same way the other reads do (one lossy decode, then
    /// `strip_ansi` when asked) and map its bytes to the snapshot's origins.
    pub(crate) fn finish(self, strip_ansi: bool) -> ScanRead {
        let decoded = String::from_utf8_lossy(&self.raw).to_string();
        let origins = self.spans.as_deref().and_then(|spans| {
            crate::text_origins::map_text_origins(
                &self.raw,
                self.cursor,
                spans,
                &decoded,
                strip_ansi,
                self.skipped > 0,
            )
        });
        let text = if strip_ansi {
            tasty_ansi::strip_ansi(&decoded)
        } else {
            decoded
        };
        let raw_bytes = self.raw.len();
        ScanRead {
            read: OutputRead {
                text,
                raw_bytes,
                cursor: self.cursor,
                next_cursor: self.cursor + raw_bytes as u64,
                retention_start: self.retention_start,
                retention_end: self.retention_end,
                skipped: self.skipped,
                stream: self.stream,
            },
            origins,
        }
    }
}

/// The supplied cursor cannot identify a valid continuation in this stream.
#[derive(Debug, PartialEq, Eq)]
pub enum OutputReadError {
    /// A position beyond the output produced so far. Rejected rather than treated as an empty read.
    AheadOfStream { cursor: u64, retention_end: u64 },
    /// The position was taken from a different stream — the surface id was
    /// reused, or the terminal was respawned under it.
    StreamMismatch { expected: String, actual: String },
}

/// Raw PTY output with three independent ways to select the start:
/// - read_mark: one explicit server-held mark per terminal; reads do not move it.
/// - scan_mark: a scanner cursor advanced by each read, separate from read_mark.
/// - OutputCursor::At: a consumer-owned position; the server stores no cursor for it.
///
/// Positions count raw bytes from stream creation. Trimming moves base, not saved marks,
/// so reads can report the gap between a mark and the oldest retained byte.
pub(crate) struct OutputBuffer {
    buffer: Vec<u8>,
    /// Absolute position of `buffer[0]`.
    base: u64,
    read_mark: Option<u64>,
    scan_mark: u64,
    stream: String,
    /// 스캔 읽기가 아직 읽지 않은 범위 `[max(base, scan_mark), end)`의 청크 출처.
    origins: OriginSpans,
}

impl OutputBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            base: 0,
            read_mark: None,
            scan_mark: 0,
            stream: mint_stream_id(),
            origins: OriginSpans::default(),
        }
    }

    /// Append raw bytes and trim to the retention size.
    ///
    /// No mark is adjusted: the marks are absolute and only `base` moves.
    /// `origin` is how the chunk was classified when it arrived (see `output_origins`).
    pub fn append(&mut self, data: &[u8], origin: ChunkOrigin) {
        self.origins.push(self.end(), origin);
        self.buffer.extend_from_slice(data);
        if self.buffer.len() > OUTPUT_RETENTION_MAX_BYTES {
            let excess = self.buffer.len() - OUTPUT_RETENTION_MAX_BYTES;
            self.buffer.drain(..excess);
            self.base += excess as u64;
            self.origins.drop_before(self.base.max(self.scan_mark));
        }
        self.origins.enforce_cap();
    }

    /// Decide the chunks that arrived while an OS resize was being applied.
    pub(crate) fn settle_pending_origins(&mut self, to: ChunkOrigin) {
        self.origins.settle_pending(to);
    }

    /// Issue a new token when mirror output is no longer continuous, such as after
    /// transport loss or a fresh attach snapshot. Keep retained bytes and positions.
    /// Readers supplying the old token get a mismatch; tokenless readers do not detect the gap.
    pub fn renew_stream(&mut self) {
        self.stream = mint_stream_id();
    }

    /// Set a read mark at the current end of the buffer.
    pub fn set_mark(&mut self) {
        self.read_mark = Some(self.end());
    }

    /// Compatibility text read from the mark, or the oldest retained byte if unset.
    /// Use read to obtain skipped-byte information as well.
    pub fn read_since_mark(&self, strip_ansi: bool) -> String {
        self.read_from(self.mark_position(), OUTPUT_RETENTION_MAX_BYTES, strip_ansi)
            .text
    }

    /// Answer a read from a position, refusing the positions that cannot be a
    /// continuation of what this stream gave out.
    pub fn read(&self, req: &OutputReadRequest) -> Result<OutputRead, OutputReadError> {
        if let Some(expected) = &req.expect_stream
            && expected != &self.stream
        {
            return Err(OutputReadError::StreamMismatch {
                expected: expected.clone(),
                actual: self.stream.clone(),
            });
        }
        let requested = match req.from {
            OutputCursor::Mark => self.mark_position(),
            OutputCursor::At(at) => at,
        };
        if requested > self.end() {
            return Err(OutputReadError::AheadOfStream {
                cursor: requested,
                retention_end: self.end(),
            });
        }
        Ok(self.read_from(
            requested,
            req.max_bytes.clamp(1, OUTPUT_RETENTION_MAX_BYTES),
            req.strip_ansi,
        ))
    }

    /// Copy what the scanner cursor has not read yet and advance the cursor in one
    /// operation so appended output cannot fall between those steps. The start may
    /// split an escape sequence or UTF-8 character. ANSI stripping can leave a partial
    /// sequence; lossy decoding replaces split characters. Consumers needing intact
    /// sequences must preserve and decode raw bytes themselves.
    ///
    /// `skipped` counts from the cursor as it was, so bytes trimmed past an unread
    /// cursor are reported. `with_origins` copies the chunk origins of the range.
    pub(crate) fn take_scan_snapshot(&mut self, with_origins: bool) -> ScanSnapshot {
        let start = self.scan_mark.max(self.base);
        let end = self.end();
        let snapshot = ScanSnapshot {
            raw: self.buffer[(start - self.base) as usize..].to_vec(),
            spans: with_origins.then(|| self.origins.slice(start, end)),
            cursor: start,
            skipped: start - self.scan_mark,
            retention_start: self.base,
            retention_end: end,
            stream: self.stream.clone(),
        };
        self.scan_mark = end;
        self.origins.drop_before(end);
        snapshot
    }

    /// One past the newest retained position.
    fn end(&self) -> u64 {
        self.base + self.buffer.len() as u64
    }

    /// Where a mark read starts asking from. A mark trimming has passed is left
    /// where it is so that [`Self::read`] can measure the gap.
    fn mark_position(&self) -> u64 {
        self.read_mark.unwrap_or(self.base)
    }

    /// The read itself. Infallible: the requested position is clamped into the
    /// retained region and the gap is reported rather than refused.
    fn read_from(&self, requested: u64, max_bytes: usize, strip_ansi: bool) -> OutputRead {
        let start = requested.clamp(self.base, self.end());
        let off = (start - self.base) as usize;
        let take = cut_before_split_char(&self.buffer[off..], max_bytes);
        let text = String::from_utf8_lossy(&self.buffer[off..off + take]).to_string();
        OutputRead {
            text: if strip_ansi {
                tasty_ansi::strip_ansi(&text)
            } else {
                text
            },
            raw_bytes: take,
            cursor: start,
            next_cursor: start + take as u64,
            retention_start: self.base,
            retention_end: self.end(),
            skipped: start.saturating_sub(requested),
            stream: self.stream.clone(),
        }
    }
}

/// Shorten the read by up to three bytes to avoid ending inside a UTF-8 character.
/// If that would return nothing, keep the requested size so a small cap still makes progress.
fn cut_before_split_char(bytes: &[u8], max: usize) -> usize {
    if max >= bytes.len() {
        return bytes.len();
    }
    let mut cut = max;
    let mut backed_off = 0;
    while cut > 0 && backed_off < 3 && bytes[cut] & 0b1100_0000 == 0b1000_0000 {
        cut -= 1;
        backed_off += 1;
    }
    if cut == 0 { max } else { cut }
}

#[cfg(test)]
mod tests {
    use super::{
        OUTPUT_RETENTION_MAX_BYTES, OutputBuffer, OutputCursor, OutputReadError, OutputReadRequest,
        mint_stream_id,
    };

    impl OutputBuffer {
        /// Append output that arrived outside any resize redraw window.
        fn append_normal(&mut self, data: &[u8]) {
            self.append(data, crate::output_origins::ChunkOrigin::Normal);
        }

        /// The scanner read's text, as `surface.read_since_scan_mark` returns it.
        fn take_since_scan_mark(&mut self, strip_ansi: bool) -> String {
            self.take_scan_snapshot(true).finish(strip_ansi).read.text
        }
    }

    /// Given a buffer filled to the retention limit with 4096 alternating origin spans,
    /// When a 512-byte chunk of the other origin trims the first two spans away, Then the
    /// 4095 spans left are under the span cap and keep their decided origins.
    #[test]
    fn trimming_runs_before_the_span_cap_so_decided_origins_survive() {
        use crate::TextOrigin;
        use crate::output_origins::{ChunkOrigin, MAX_ORIGIN_SPANS};
        let mut b = OutputBuffer::new();
        for i in 0..MAX_ORIGIN_SPANS {
            let origin = if i % 2 == 0 {
                ChunkOrigin::Normal
            } else {
                ChunkOrigin::Redraw
            };
            b.append(&[b'a'; 256], origin);
        }
        b.append(&[b'b'; 512], ChunkOrigin::Normal);
        let read = b.take_scan_snapshot(true).finish(false);
        assert_eq!(read.read.skipped, 512);
        let origins = read
            .origins
            .expect("4095 ranges and one gap fit the entry cap");
        assert_eq!(origins.ranges.len(), MAX_ORIGIN_SPANS - 1);
        assert!(
            origins
                .ranges
                .iter()
                .all(|r| r.origin != TextOrigin::Unknown)
        );
    }

    /// Push `n` bytes of filler whose content is irrelevant to the assertion.
    fn filler(n: usize) -> Vec<u8> {
        vec![b'.'; n]
    }

    /// Helper for the cursor form: ask from an absolute position with no cap
    /// and no stream check.
    fn at(b: &OutputBuffer, cursor: u64) -> super::OutputRead {
        b.read(&OutputReadRequest {
            from: OutputCursor::At(cursor),
            max_bytes: OUTPUT_RETENTION_MAX_BYTES,
            strip_ansi: false,
            expect_stream: None,
        })
        .expect("a position inside the stream is answerable")
    }

    /// Helper for the mark form.
    fn from_mark(b: &OutputBuffer) -> super::OutputRead {
        b.read(&OutputReadRequest {
            from: OutputCursor::Mark,
            max_bytes: OUTPUT_RETENTION_MAX_BYTES,
            strip_ansi: false,
            expect_stream: None,
        })
        .expect("the mark is never ahead of the stream")
    }

    /// Renewing the stream turns a position taken before it into a mismatch
    /// for a reader that carries the token, and changes nothing for one that
    /// does not: positions keep counting and the retained bytes stay.
    #[test]
    fn a_renewed_stream_refuses_the_old_token_but_keeps_counting_positions() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"before");
        let first = at(&b, 0);
        assert_eq!(first.text, "before");

        b.renew_stream();
        b.append_normal(b"after");

        let refused = b.read(&OutputReadRequest {
            from: OutputCursor::At(first.next_cursor),
            max_bytes: OUTPUT_RETENTION_MAX_BYTES,
            strip_ansi: false,
            expect_stream: Some(first.stream.clone()),
        });
        match refused {
            Err(OutputReadError::StreamMismatch { expected, actual }) => {
                assert_eq!(expected, first.stream);
                assert_ne!(actual, first.stream, "the token did not change");
            }
            other => panic!("a read across the renewal must be refused: {other:?}"),
        }

        let tokenless = at(&b, first.next_cursor);
        assert_eq!(
            tokenless.text, "after",
            "a reader without the token sees positions continue as before"
        );
        assert_ne!(tokenless.stream, first.stream);
    }

    #[test]
    fn a_read_mark_the_trim_passed_still_reads_the_whole_buffer_and_now_says_what_it_lost() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"head");
        b.set_mark();
        b.append_normal(b"tail");
        assert_eq!(b.read_since_mark(false), "tail");
        assert_eq!(from_mark(&b).skipped, 0);

        // Overflow by more than the mark's offset, so the trim passes the mark.
        b.append_normal(&filler(OUTPUT_RETENTION_MAX_BYTES));
        assert_eq!(
            b.read_since_mark(false).len(),
            OUTPUT_RETENTION_MAX_BYTES,
            "the text is what it always was — the whole retained buffer"
        );
        let read = from_mark(&b);
        assert_eq!(
            read.skipped, 4,
            "'tail' sat between the mark and the retained window and is gone \
             for good — that is what the read now says instead of starting \
             over in silence"
        );
        assert_eq!(read.cursor, read.retention_start);
    }

    #[test]
    fn a_read_mark_outside_the_trimmed_region_shifts_by_the_trimmed_amount() {
        let mut b = OutputBuffer::new();
        b.append_normal(&filler(OUTPUT_RETENTION_MAX_BYTES));
        b.set_mark();
        b.append_normal(b"after");
        // Trimmed 5 bytes off the front; the mark sat at OUTPUT_RETENTION_MAX_BYTES, well
        // past that, so it survives and still points just before "after".
        assert_eq!(b.read_since_mark(false), "after");
    }

    #[test]
    fn the_scan_cursor_advances_so_each_read_returns_only_what_arrived_since() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"first");
        assert_eq!(b.take_since_scan_mark(false), "first");
        assert_eq!(
            b.take_since_scan_mark(false),
            "",
            "nothing was appended between the two reads"
        );
        b.append_normal(b"second");
        assert_eq!(b.take_since_scan_mark(false), "second");
    }

    #[test]
    fn the_two_cursors_do_not_move_each_other() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"before");
        b.take_since_scan_mark(false);

        // The agent sets its mark; the scan cursor must not follow it.
        b.append_normal(b"between");
        b.set_mark();
        b.append_normal(b"after");
        assert_eq!(
            b.take_since_scan_mark(false),
            "betweenafter",
            "set_mark moved the scan cursor — the scanner would have lost \
             'between'"
        );

        // And the scan read must not have moved the agent's mark.
        assert_eq!(b.read_since_mark(false), "after");
    }

    #[test]
    fn trim_pulls_the_scan_cursor_back_instead_of_dropping_it() {
        let mut b = OutputBuffer::new();
        b.append_normal(&filler(OUTPUT_RETENTION_MAX_BYTES));
        b.take_since_scan_mark(false);
        b.append_normal(b"tail");
        assert_eq!(
            b.take_since_scan_mark(false),
            "tail",
            "the cursor was clamped to the retained region, so only the newly \
             appended bytes come back"
        );
    }

    #[test]
    fn strip_ansi_applies_to_the_scan_read_as_well() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"\x1b[31mred\x1b[0m");
        assert_eq!(b.take_since_scan_mark(true), "red");
    }

    #[test]
    fn two_consumer_held_cursors_do_not_move_each_other_or_either_server_mark() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"one");
        let a1 = at(&b, 0);
        assert_eq!(a1.text, "one");
        b.append_normal(b"two");

        // B reads the same region A already read. A's position is unaffected
        // because the buffer holds nothing on either consumer's behalf.
        let b1 = at(&b, 0);
        assert_eq!(b1.text, "onetwo");
        let a2 = at(&b, a1.next_cursor);
        assert_eq!(a2.text, "two", "B's read did not move A on");

        // And neither touched the two server-held cursors.
        assert_eq!(b.read_since_mark(false), "onetwo");
        assert_eq!(b.take_since_scan_mark(false), "onetwo");
    }

    #[test]
    fn a_cursor_the_trim_passed_reports_the_gap_instead_of_starting_over_in_silence() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"lost");
        let cursor = at(&b, 0).cursor;
        b.append_normal(&filler(OUTPUT_RETENTION_MAX_BYTES));

        let read = at(&b, cursor);
        assert_eq!(
            read.skipped, 4,
            "the four bytes of 'lost' are gone for good"
        );
        assert_eq!(read.cursor, read.retention_start);
        assert_eq!(read.retention_start, 4);
        assert_eq!(read.retention_end, 4 + OUTPUT_RETENTION_MAX_BYTES as u64);
    }

    #[test]
    fn a_cursor_past_the_end_of_the_stream_is_refused_rather_than_answered_empty() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"abc");
        let err = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(9),
                max_bytes: OUTPUT_RETENTION_MAX_BYTES,
                strip_ansi: false,
                expect_stream: None,
            })
            .expect_err(
                "answering empty would let the consumer wait for a \
                 continuation that, once the terminal produces six more bytes, \
                 is unrelated to anything it saw",
            );
        assert_eq!(
            err,
            OutputReadError::AheadOfStream {
                cursor: 9,
                retention_end: 3
            }
        );
        // The end itself is not past the end: that is "caught up".
        let caught_up = at(&b, 3);
        assert_eq!(caught_up.text, "");
        assert_eq!(caught_up.next_cursor, caught_up.retention_end);
    }

    #[test]
    fn a_cursor_carried_over_from_another_stream_is_refused() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"abc");
        let other = OutputBuffer::new();
        assert_ne!(
            b.stream, other.stream,
            "two buffers built back to back must not share a token — a reused \
             surface id would otherwise read as the same stream"
        );

        let err = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: OUTPUT_RETENTION_MAX_BYTES,
                strip_ansi: false,
                expect_stream: Some(other.stream.clone()),
            })
            .expect_err("the token names a stream this buffer is not");
        assert_eq!(
            err,
            OutputReadError::StreamMismatch {
                expected: other.stream.clone(),
                actual: b.stream.clone(),
            }
        );

        // The same position with this stream's own token is answerable.
        assert_eq!(
            b.read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: OUTPUT_RETENTION_MAX_BYTES,
                strip_ansi: false,
                expect_stream: Some(b.stream.clone()),
            })
            .expect("own token")
            .text,
            "abc"
        );
    }

    #[test]
    fn max_bytes_caps_the_raw_region_and_the_next_cursor_counts_raw_bytes() {
        let mut b = OutputBuffer::new();
        b.append_normal("\x1b[31mred\x1b[0m".as_bytes());
        let read = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: 8,
                strip_ansi: true,
                expect_stream: None,
            })
            .expect("inside the stream");
        assert_eq!(read.raw_bytes, 8);
        assert_eq!(read.next_cursor, 8);
        assert_eq!(
            read.text, "red",
            "stripping removed bytes, so the text is shorter than the region"
        );
        assert_ne!(
            read.text.len(),
            read.raw_bytes,
            "a consumer advancing by text.len() would desynchronise — this is \
             why next_cursor is a separate number"
        );
    }

    #[test]
    fn a_cap_landing_mid_character_backs_off_so_neither_side_loses_it() {
        let mut b = OutputBuffer::new();
        // 'ㄱ' is three bytes; ask for four so the cap lands inside the second.
        b.append_normal("ㄱㄴ".as_bytes());
        let first = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: 4,
                strip_ansi: false,
                expect_stream: None,
            })
            .expect("inside the stream");
        assert_eq!(first.text, "ㄱ");
        assert_eq!(first.raw_bytes, 3, "backed off one byte");
        assert_eq!(at(&b, first.next_cursor).text, "ㄴ");
    }

    #[test]
    fn a_cap_smaller_than_the_next_character_still_makes_progress() {
        let mut b = OutputBuffer::new();
        b.append_normal("ㄱ".as_bytes());
        let read = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: 1,
                strip_ansi: false,
                expect_stream: None,
            })
            .expect("inside the stream");
        assert_eq!(
            read.raw_bytes, 1,
            "backing off would take nothing, and a read that takes nothing \
             while bytes are waiting never finishes"
        );
        assert_eq!(read.next_cursor, 1);
    }

    #[test]
    fn max_bytes_is_clamped_at_both_ends() {
        let mut b = OutputBuffer::new();
        b.append_normal(b"abcdef");
        let zero = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: 0,
                strip_ansi: false,
                expect_stream: None,
            })
            .expect("inside the stream");
        assert_eq!(
            zero.raw_bytes, 1,
            "0 would make every read empty and the consumer would never advance"
        );
        let huge = b
            .read(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: usize::MAX,
                strip_ansi: false,
                expect_stream: None,
            })
            .expect("inside the stream");
        assert_eq!(huge.raw_bytes, 6, "nothing beyond what is retained exists");
    }

    #[test]
    fn positions_keep_counting_across_a_trim() {
        let mut b = OutputBuffer::new();
        b.append_normal(&filler(OUTPUT_RETENTION_MAX_BYTES));
        assert_eq!(at(&b, 0).retention_start, 0);
        b.append_normal(b"tail");
        let read = at(&b, 4);
        assert_eq!(read.retention_start, 4, "the window moved, not the numbers");
        assert_eq!(
            read.retention_end,
            4 + OUTPUT_RETENTION_MAX_BYTES as u64,
            "a position that once named a retained byte never names a different \
             one later"
        );
    }

    #[test]
    fn the_stream_token_does_not_rest_on_the_clock_alone() {
        // A process counter distinguishes tokens even when the timestamp is identical.
        // This test checks that component; it does not simulate a host restart.
        let tail = |id: String| id.split_once('-').map(|(_, t)| t.to_string());
        let a = tail(mint_stream_id()).expect("표지에 시계 뒤 마디가 있다");
        let b = tail(mint_stream_id()).expect("표지에 시계 뒤 마디가 있다");
        assert_ne!(a, b, "같은 나노초에 서면 두 표지가 같아진다");
    }
}
