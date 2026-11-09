use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use termwiz::cell::{Blink, CellAttributes, Underline};
use termwiz::color::ColorAttribute;

use crate::scrollback::ScrollbackLine;

/// Disk scrollback magic. Change FORMAT_VERSION when changing the serialized layout.
pub const FILE_MAGIC: &[u8; 4] = b"TSSB";
pub const FORMAT_VERSION: u32 = 4;
/// Oldest layout still decoded. Stored captures are immutable, so an upgrade cannot rewrite them;
/// version 3 records are version 4 records in which no run sets [`FLAG_EXTENDED`].
pub const OLDEST_READABLE_VERSION: u32 = 3;
pub(crate) const HEADER_LEN: u64 = 8; // 4-byte magic + 4-byte version

/// Serialize lines as FILE_MAGIC | FORMAT_VERSION:u32 | { len:u32 | line_bytes }*.
/// The host stores this blob under its data directory.
pub fn serialize_lines(lines: &[ScrollbackLine]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_LEN as usize + lines.len() * 64);
    buf.extend_from_slice(FILE_MAGIC);
    buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    for line in lines {
        let bytes = serialize_line(line);
        let len = bytes.len() as u32;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&bytes);
    }
    buf
}

/// Read a serialized blob. Reject mismatched magic or version; keep complete records before a truncated tail.
pub fn deserialize_lines(data: &[u8]) -> Option<Vec<ScrollbackLine>> {
    if data.len() < HEADER_LEN as usize {
        return None;
    }
    if &data[0..4] != FILE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if !(OLDEST_READABLE_VERSION..=FORMAT_VERSION).contains(&version) {
        return None;
    }
    let mut pos = HEADER_LEN as usize;
    let mut lines = Vec::new();
    while pos + 4 <= data.len() {
        let len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + len > data.len() {
            break;
        }
        lines.push(deserialize_line(&data[pos..pos + len]));
        pos += len;
    }
    Some(lines)
}

/// Disk-backed scrollback storage. Older lines are written to a temp file,
/// while recent lines remain in memory for fast access.
pub struct DiskScrollback {
    file_path: PathBuf,
    /// Records at the front of the file that were dropped but not yet compacted away.
    /// Logical line `i` is physical record `head + i`.
    head: usize,
    /// Byte offsets of each physical record in the file (for random access).
    line_offsets: Vec<u64>,
    /// File size.
    file_size: u64,
    /// Write handle held for the owner's lifetime so appends never reopen the path.
    /// Drop takes it first: Windows cannot remove a file while a handle is open.
    writer: Option<File>,
    /// False after a failed append, when the handle position may be past `file_size`.
    position_known: bool,
    /// Set once a failed compaction dropped retained rows and reported it; cleared when a
    /// compaction succeeds, so a lasting fault is reported once rather than per row.
    loss_reported: bool,
    /// Read-side opens of the backing file, for tests that pin the per-call open count.
    #[cfg(test)]
    read_opens: std::sync::atomic::AtomicUsize,
    /// Compaction copies started, each creating an instance file, for tests that pin their rate.
    #[cfg(test)]
    compaction_copies: std::sync::atomic::AtomicUsize,
    /// `push_lines` calls that wrote at least one line, for tests that pin batching.
    #[cfg(test)]
    push_batches: usize,
    /// Fail the append that would write past this many more bytes, for rollback tests.
    #[cfg(test)]
    fail_append_after: Option<usize>,
    /// Fail every compaction like a full disk, for tests that pin how a lasting fault is handled.
    #[cfg(test)]
    fail_compaction: bool,
}

/// Records are serialized into a buffer of about this size before each write, so a large
/// batch does not hold its whole serialized form in memory.
pub(crate) const WRITE_CHUNK: usize = 1 << 20;

mod reclaim;
pub use reclaim::{
    Liveness, ReclaimReport, process_liveness, reclaim_in, reclaim_orphans, spawn_orphan_reclaim,
};

/// Swap folders under the temp directory: debug builds first, then release builds.
const SUBDIRS: [&str; 2] = ["tasty-scrollback-debug", "tasty-scrollback"];

impl DiskScrollback {
    pub fn new(surface_id: u32) -> std::io::Result<Self> {
        // A replacement may be prepared while the previous instance with the same SID is alive.
        // Each owner therefore gets its own file; create_new also protects files from an older PID.
        let subdir = if cfg!(debug_assertions) {
            SUBDIRS[0]
        } else {
            SUBDIRS[1]
        };
        // reason: shared directory only; create_instance_file uses a unique create_new path per owner.
        let dir = std::env::temp_dir().join(subdir);
        std::fs::create_dir_all(&dir)?;
        let (file_path, mut f) = create_instance_file(&dir, surface_id)?;
        let mut owner = Self {
            file_path,
            head: 0,
            line_offsets: Vec::new(),
            file_size: HEADER_LEN,
            writer: None,
            position_known: true,
            loss_reported: false,
            #[cfg(test)]
            read_opens: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            compaction_copies: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            push_batches: 0,
            #[cfg(test)]
            fail_append_after: None,
            #[cfg(test)]
            fail_compaction: false,
        };
        let initialized = (|| {
            f.write_all(FILE_MAGIC)?;
            f.write_all(&FORMAT_VERSION.to_le_bytes())?;
            f.flush()
        })();
        if let Err(e) = initialized {
            // Close the file before failure drops the path owner, including on Windows.
            drop(f);
            return Err(e);
        }
        owner.writer = Some(f);
        Ok(owner)
    }

    /// Append lines to disk through the held handle. Returns number of lines written.
    ///
    /// All or nothing: on error no line of the batch is recorded, the counters keep their
    /// previous values and the file is cut back to the last complete record, so the next
    /// batch starts where the failed one did.
    pub fn push_lines(&mut self, lines: &[ScrollbackLine]) -> std::io::Result<usize> {
        if lines.is_empty() {
            return Ok(0);
        }
        #[cfg(test)]
        {
            self.push_batches += 1;
        }
        let start = self.file_size;
        let mut offsets = Vec::with_capacity(lines.len());
        let mut written = 0u64;
        let mut buf = Vec::new();
        let result = (|| {
            for line in lines {
                offsets.push(start + written + buf.len() as u64);
                let bytes = serialize_line(line);
                buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&bytes);
                if buf.len() >= WRITE_CHUNK {
                    self.append(&buf)?;
                    written += buf.len() as u64;
                    buf.clear();
                }
            }
            self.append(&buf)?;
            written += buf.len() as u64;
            Ok(())
        })();
        if let Err(e) = result {
            self.discard_from(start);
            return Err(e);
        }
        self.line_offsets.extend(offsets);
        self.file_size = start + written;
        Ok(lines.len())
    }

    fn append(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let file = self
            .writer
            .as_mut()
            .ok_or_else(|| std::io::Error::other("disk scrollback writer is closed"))?;
        if !self.position_known {
            file.seek(SeekFrom::Start(self.file_size))?;
            self.position_known = true;
        }
        #[cfg(test)]
        if let Some(budget) = self.fail_append_after {
            if bytes.len() <= budget {
                self.fail_append_after = Some(budget - bytes.len());
            } else {
                self.fail_append_after = None;
                file.write_all(&bytes[..budget])?;
                return Err(std::io::Error::other("injected append failure"));
            }
        }
        file.write_all(bytes)
    }

    /// Drop bytes past `size` after a failed append. If truncation fails the stale tail
    /// stays, but no offset points into it and the next append seeks back over it.
    fn discard_from(&mut self, size: u64) {
        self.position_known = false;
        if let Some(file) = &self.writer
            && let Err(e) = file.set_len(size)
        {
            tracing::warn!("disk scrollback tail truncation failed: {e}");
        }
    }

    /// Read a line from disk by index.
    pub fn read_line(&self, index: usize) -> std::io::Result<Option<ScrollbackLine>> {
        if index >= self.line_count() {
            return Ok(None);
        }

        let file = self.open_for_read()?;
        let mut reader = BufReader::new(file);
        let offset = self.line_offsets[self.head + index];
        reader.seek(SeekFrom::Start(offset))?;

        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        reader.read_exact(&mut data)?;

        Ok(Some(deserialize_line(&data)))
    }

    /// Visit lines `range` (clamped to the stored count) in order with one open and
    /// one seek. Records are contiguous, so the rest of the range is read sequentially.
    pub fn for_each_in_range(
        &self,
        range: std::ops::Range<usize>,
        mut visit: impl FnMut(usize, ScrollbackLine),
    ) -> std::io::Result<()> {
        let end = range.end.min(self.line_count());
        if range.start >= end {
            return Ok(());
        }
        let mut reader = BufReader::new(self.open_for_read()?);
        reader.seek(SeekFrom::Start(self.line_offsets[self.head + range.start]))?;
        let mut data = Vec::new();
        for index in range.start..end {
            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf)?;
            data.resize(u32::from_le_bytes(len_buf) as usize, 0);
            reader.read_exact(&mut data)?;
            visit(index, deserialize_line(&data));
        }
        Ok(())
    }

    /// Serialized bytes of every stored line, excluding the file header. This equals the
    /// record bytes `serialize_lines` produces for the same lines when no append failed.
    pub fn records_len(&self) -> u64 {
        self.file_size - self.start_offset()
    }

    /// File offset of the oldest retained record, or the end of the file when none is retained.
    fn start_offset(&self) -> u64 {
        self.line_offsets
            .get(self.head)
            .copied()
            .unwrap_or(self.file_size)
    }

    fn open_for_read(&self) -> std::io::Result<File> {
        #[cfg(test)]
        self.read_opens
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        File::open(&self.file_path)
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len() - self.head
    }

    /// Drop the `count` oldest retained lines (clamped to the stored count). Their bytes stay
    /// in the file until enough accumulate for one compaction, so the file holds at most `cap`
    /// plus the compaction threshold records. Returns how many lines were dropped, which is
    /// every retained line when that compaction failed.
    pub fn drop_front(&mut self, count: usize, cap: usize) -> usize {
        let dropped = count.min(self.line_count());
        self.head += dropped;
        if self.head >= compaction_threshold(cap) || self.line_count() == 0 {
            return dropped + self.compact();
        }
        dropped
    }

    /// Move the retained records to a fresh instance file and release the dropped prefix and
    /// its offsets. Returns how many retained lines were given up because that failed.
    fn compact(&mut self) -> usize {
        if self.head == 0 {
            return 0;
        }
        if self.line_count() == 0 {
            if let Err(e) = self.clear() {
                self.restart_after_failure(0, &e);
            }
            return 0;
        }
        match self.copy_retained() {
            Ok((path, writer, base)) => {
                self.adopt_copy(path, writer, base);
                self.loss_reported = false;
                0
            }
            Err(e) => {
                let lost = self.line_count();
                self.restart_after_failure(lost, &e);
                lost
            }
        }
    }

    /// Give up every record after a compaction failed, so a lasting fault (an unlinked file, a
    /// full disk) cannot grow the file or the offset table past the cap. A missing backing file
    /// is replaced by a fresh instance file so later rows read back again; otherwise the file is
    /// cut to its header through the held handle. If neither works, the counters still restart
    /// and the next append overwrites from the header.
    fn restart_after_failure(&mut self, lost: usize, error: &std::io::Error) {
        self.restart_storage();
        self.report_compaction_loss(lost, error);
    }

    /// Empty the backing storage after a failed compaction: replace a missing file, otherwise
    /// cut it to its header, and restart the counters when neither works.
    fn restart_storage(&mut self) {
        let missing = matches!(
            std::fs::metadata(&self.file_path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        );
        let restarted = if missing {
            self.replace_with_fresh_file().or_else(|_| self.clear())
        } else {
            self.clear()
        };
        if let Err(e) = restarted {
            tracing::debug!("disk scrollback restart after a failed compaction failed: {e}");
            self.reset_to_header(false);
        }
    }

    /// Warn once per failure streak; later failures in the same streak only log at debug.
    fn report_compaction_loss(&mut self, lost: usize, error: &std::io::Error) {
        if self.loss_reported {
            tracing::debug!(
                "disk scrollback compaction failed again: {error}; dropped {lost} rows"
            );
        } else {
            self.loss_reported = true;
            tracing::warn!(
                "disk scrollback compaction failed: {error}; dropped the {lost} oldest disk rows"
            );
        }
    }

    /// Start over in a new, empty instance file next to the current path, recreating the folder
    /// if it was removed.
    fn replace_with_fresh_file(&mut self) -> std::io::Result<()> {
        let directory = self
            .file_path
            .parent()
            .ok_or_else(|| std::io::Error::other("scrollback file has no directory"))?
            .to_path_buf();
        std::fs::create_dir_all(&directory)?;
        let (path, mut file) =
            create_instance_file(&directory, instance_surface_id(&self.file_path))?;
        let header = file
            .write_all(FILE_MAGIC)
            .and_then(|()| file.write_all(&FORMAT_VERSION.to_le_bytes()))
            .and_then(|()| file.flush());
        if let Err(e) = header {
            drop(file);
            if let Err(remove) = std::fs::remove_file(&path) {
                tracing::debug!(
                    "partial scrollback file {} removal failed: {remove}",
                    path.display()
                );
            }
            return Err(e);
        }
        drop(self.writer.replace(file));
        let old = std::mem::replace(&mut self.file_path, path);
        if let Err(e) = std::fs::remove_file(&old)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::debug!("old scrollback file {} removal failed: {e}", old.display());
        }
        self.reset_to_header(true);
        Ok(())
    }

    /// Forget every record. `position_known` says whether the handle already sits at the header.
    fn reset_to_header(&mut self, position_known: bool) {
        self.head = 0;
        self.line_offsets.clear();
        self.line_offsets.shrink_to_fit();
        self.file_size = HEADER_LEN;
        self.position_known = position_known;
    }

    /// Switch to the compacted copy whose first record was at `base` in the old file.
    fn adopt_copy(&mut self, path: PathBuf, writer: File, base: u64) {
        drop(self.writer.replace(writer));
        let old = std::mem::replace(&mut self.file_path, path);
        if let Err(e) = std::fs::remove_file(&old)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("old scrollback file {} removal failed: {e}", old.display());
        }
        self.line_offsets.drain(..self.head);
        for offset in &mut self.line_offsets {
            *offset = *offset - base + HEADER_LEN;
        }
        self.line_offsets.shrink_to_fit();
        self.file_size = self.file_size - base + HEADER_LEN;
        self.head = 0;
        self.position_known = true;
    }

    /// Write the header and the retained record bytes to a new instance file next to the
    /// current one. Returns its path, write handle and the old offset of the first record.
    fn copy_retained(&self) -> std::io::Result<(PathBuf, File, u64)> {
        let directory = self
            .file_path
            .parent()
            .ok_or_else(|| std::io::Error::other("scrollback file has no directory"))?;
        let surface_id = instance_surface_id(&self.file_path);
        #[cfg(test)]
        self.compaction_copies
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (path, mut file) = create_instance_file(directory, surface_id)?;
        let base = self.start_offset();
        let copied = (|| {
            #[cfg(test)]
            if self.fail_compaction {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::StorageFull,
                    "injected compaction failure",
                ));
            }
            file.write_all(FILE_MAGIC)?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            let mut reader = self.open_for_read()?;
            reader.seek(SeekFrom::Start(base))?;
            let length = self.file_size - base;
            let moved = std::io::copy(&mut Read::take(reader, length), &mut file)?;
            if moved != length {
                return Err(std::io::Error::other("scrollback file ended early"));
            }
            file.flush()
        })();
        match copied {
            Ok(()) => Ok((path, file, base)),
            Err(e) => {
                drop(file);
                if let Err(remove) = std::fs::remove_file(&path) {
                    tracing::warn!(
                        "partial scrollback file {} removal failed: {remove}",
                        path.display()
                    );
                }
                Err(e)
            }
        }
    }

    /// Discard all on-disk scrollback, truncating the file back to just the
    /// header. Used by ED3 (`CSI 3J`) erase-scrollback.
    pub fn clear(&mut self) -> std::io::Result<()> {
        let file = self
            .writer
            .as_ref()
            .ok_or_else(|| std::io::Error::other("disk scrollback writer is closed"))?;
        // The header stays; the next append seeks to its end.
        file.set_len(HEADER_LEN)?;
        self.reset_to_header(false);
        Ok(())
    }
}

/// Dropped records a file may keep before it is compacted. At least `cap`, so each
/// compaction copies no more than it released and appends stay amortized constant time.
fn compaction_threshold(cap: usize) -> usize {
    cap.max(MIN_COMPACTION_RECORDS)
}

/// Lower bound of [`compaction_threshold`], so a tiny cap does not rewrite the file per line.
pub(crate) const MIN_COMPACTION_RECORDS: usize = 4096;

/// Surface ID encoded in an instance file name (`surface-<pid>-<sid>-<nonce>.scrollback`).
fn instance_surface_id(path: &std::path::Path) -> u32 {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('-').nth(2))
        .and_then(|sid| sid.parse().ok())
        .unwrap_or(0)
}

fn create_instance_file(
    directory: &std::path::Path,
    surface_id: u32,
) -> std::io::Result<(PathBuf, File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_FILE: AtomicU64 = AtomicU64::new(1);
    loop {
        let nonce = NEXT_FILE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| std::io::Error::other("scrollback instance file identity exhausted"))?;
        let path = directory.join(format!(
            "surface-{}-{surface_id}-{nonce}.scrollback",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

impl Drop for DiskScrollback {
    fn drop(&mut self) {
        // Close the write handle first; Windows refuses to remove an open file.
        drop(self.writer.take());
        if let Err(e) = std::fs::remove_file(&self.file_path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::trace!(
                "scrollback temp file {} cleanup failed: {e}",
                self.file_path.display()
            );
        }
    }
}

/// A run with this flag carries the extended byte and the underline color after its flags byte.
const FLAG_EXTENDED: u8 = 32;
const EXT_UNDERLINE_MASK: u8 = 0b0000_0111;
const EXT_BLINK_SHIFT: u32 = 3;
const EXT_BLINK_MASK: u8 = 0b0001_1000;
const EXT_REVERSE: u8 = 32;
const EXT_INVISIBLE: u8 = 64;
const EXT_OVERLINE: u8 = 128;

/// Pack a cell's attributes into the flags byte.
/// bit0=bold, bit1=italic, bit2=underline, bit3=strikethrough, bit4=dim (Intensity::Half),
/// bit5=[`FLAG_EXTENDED`]. Without the extended byte, bit2 means a single underline.
fn attr_flags(attrs: &CellAttributes) -> u8 {
    let mut flags: u8 = 0;
    match attrs.intensity() {
        termwiz::cell::Intensity::Bold => flags |= 1,
        termwiz::cell::Intensity::Half => flags |= 16,
        termwiz::cell::Intensity::Normal => {}
    }
    if attrs.italic() {
        flags |= 2;
    }
    if attrs.underline() != Underline::None {
        flags |= 4;
    }
    if attrs.strikethrough() {
        flags |= 8;
    }
    if extended_attrs(attrs).is_some() {
        flags |= FLAG_EXTENDED;
    }
    flags
}

/// The extended byte, or `None` when the flags byte alone restores the attributes. Most runs
/// have no such attribute and stay as small as in version 3.
fn extended_attrs(attrs: &CellAttributes) -> Option<u8> {
    let underline = match attrs.underline() {
        Underline::None => 0,
        Underline::Single => 1,
        Underline::Double => 2,
        Underline::Curly => 3,
        Underline::Dotted => 4,
        Underline::Dashed => 5,
    };
    let blink = match attrs.blink() {
        Blink::None => 0u8,
        Blink::Slow => 1,
        Blink::Rapid => 2,
    };
    let mut ext = underline | (blink << EXT_BLINK_SHIFT);
    if attrs.reverse() {
        ext |= EXT_REVERSE;
    }
    if attrs.invisible() {
        ext |= EXT_INVISIBLE;
    }
    if attrs.overline() {
        ext |= EXT_OVERLINE;
    }
    let plain_underline = matches!(attrs.underline(), Underline::None | Underline::Single);
    let flags_suffice = plain_underline
        && ext & !EXT_UNDERLINE_MASK == 0
        && attrs.underline_color() == ColorAttribute::Default;
    (!flags_suffice).then_some(ext)
}

fn apply_extended_attrs(attrs: &mut CellAttributes, ext: u8, underline_color: ColorAttribute) {
    attrs.set_underline(match ext & EXT_UNDERLINE_MASK {
        1 => Underline::Single,
        2 => Underline::Double,
        3 => Underline::Curly,
        4 => Underline::Dotted,
        5 => Underline::Dashed,
        _ => Underline::None,
    });
    attrs.set_blink(match (ext & EXT_BLINK_MASK) >> EXT_BLINK_SHIFT {
        1 => Blink::Slow,
        2 => Blink::Rapid,
        _ => Blink::None,
    });
    attrs.set_reverse(ext & EXT_REVERSE != 0);
    attrs.set_invisible(ext & EXT_INVISIBLE != 0);
    attrs.set_overline(ext & EXT_OVERLINE != 0);
    attrs.set_underline_color(underline_color);
}

/// Serialize a scrollback line to bytes, mirroring the in-memory compact form
/// (single text buffer + per-cell lengths + RLE attribute runs).
/// Layout:
///   [wrapped:u8]
///   [text_len:u32][text_bytes]
///   [cell_count:u32][cell_len:u16 × cell_count]
///   [run_count:u32] then per run:
///     [run_len:u32][fg_type:u8][fg_data:0-3][bg_type:u8][bg_data:0-3][flags:u8]
///     and, when flags has FLAG_EXTENDED, [ext:u8][underline_color_type:u8][color_data:0-3]
fn serialize_line(line: &ScrollbackLine) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(if line.wrapped { 1 } else { 0 });

    let text_bytes = line.text.as_bytes();
    buf.extend_from_slice(&(text_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(text_bytes);

    buf.extend_from_slice(&(line.cell_lens.len() as u32).to_le_bytes());
    for &len in &line.cell_lens {
        buf.extend_from_slice(&len.to_le_bytes());
    }

    buf.extend_from_slice(&(line.attr_runs.len() as u32).to_le_bytes());
    for (run_len, attrs) in &line.attr_runs {
        buf.extend_from_slice(&run_len.to_le_bytes());
        serialize_color(&attrs.foreground(), &mut buf);
        serialize_color(&attrs.background(), &mut buf);
        buf.push(attr_flags(attrs));
        if let Some(ext) = extended_attrs(attrs) {
            buf.push(ext);
            serialize_color(&attrs.underline_color(), &mut buf);
        }
    }
    buf
}

/// Bytes `serialize_lines` spends on `line`, including its length prefix, without serializing.
pub fn serialized_record_len(line: &ScrollbackLine) -> usize {
    let runs: usize = line
        .attr_runs
        .iter()
        .map(|(_, attrs)| {
            let extended = extended_attrs(attrs)
                .map(|_| 1 + color_len(&attrs.underline_color()))
                .unwrap_or(0);
            4 + color_len(&attrs.foreground()) + color_len(&attrs.background()) + 1 + extended
        })
        .sum();
    4 + 1 + 4 + line.text.len() + 4 + 2 * line.cell_lens.len() + 4 + runs
}

fn color_len(color: &ColorAttribute) -> usize {
    match color {
        ColorAttribute::Default => 1,
        ColorAttribute::PaletteIndex(_) => 2,
        ColorAttribute::TrueColorWithDefaultFallback(_)
        | ColorAttribute::TrueColorWithPaletteFallback(_, _) => 4,
    }
}

fn serialize_color(color: &ColorAttribute, buf: &mut Vec<u8>) {
    match color {
        ColorAttribute::Default => buf.push(0),
        ColorAttribute::PaletteIndex(idx) => {
            buf.push(1);
            buf.push(*idx);
        }
        ColorAttribute::TrueColorWithDefaultFallback(c)
        | ColorAttribute::TrueColorWithPaletteFallback(c, _) => {
            buf.push(2);
            let (r, g, b, _) = c.to_tuple_rgba();
            buf.push((r * 255.0) as u8);
            buf.push((g * 255.0) as u8);
            buf.push((b * 255.0) as u8);
        }
    }
}

fn deserialize_line(data: &[u8]) -> ScrollbackLine {
    let mut pos = 0;
    if data.len() < 5 {
        return ScrollbackLine::new(Vec::new(), false);
    }

    let wrapped = data[0] != 0;
    pos += 1;

    if pos + 4 > data.len() {
        return ScrollbackLine::new(Vec::new(), wrapped);
    }
    let text_len =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;
    if pos + text_len > data.len() {
        return ScrollbackLine::new(Vec::new(), wrapped);
    }
    let text = String::from_utf8_lossy(&data[pos..pos + text_len]).to_string();
    pos += text_len;

    if pos + 4 > data.len() {
        return ScrollbackLine::from_raw_parts(text, Vec::new(), Vec::new(), wrapped);
    }
    let cell_count =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;
    let mut cell_lens = Vec::with_capacity(cell_count);
    for _ in 0..cell_count {
        if pos + 2 > data.len() {
            break;
        }
        cell_lens.push(u16::from_le_bytes([data[pos], data[pos + 1]]));
        pos += 2;
    }

    let mut attr_runs = Vec::new();
    if pos + 4 <= data.len() {
        let run_count =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        attr_runs.reserve(run_count);
        for _ in 0..run_count {
            if pos + 4 > data.len() {
                break;
            }
            let run_len =
                u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;

            let (fg, advance) = deserialize_color(&data[pos..]);
            pos += advance;
            let (bg, advance) = deserialize_color(&data[pos..]);
            pos += advance;

            if pos >= data.len() {
                break;
            }
            let flags = data[pos];
            pos += 1;

            let mut attrs = CellAttributes::default();
            attrs.set_foreground(fg);
            attrs.set_background(bg);
            if flags & 1 != 0 {
                attrs.set_intensity(termwiz::cell::Intensity::Bold);
            } else if flags & 16 != 0 {
                attrs.set_intensity(termwiz::cell::Intensity::Half);
            }
            if flags & 2 != 0 {
                attrs.set_italic(true);
            }
            if flags & 4 != 0 {
                attrs.set_underline(Underline::Single);
            }
            if flags & 8 != 0 {
                attrs.set_strikethrough(true);
            }
            if flags & FLAG_EXTENDED != 0 {
                let Some(&ext) = data.get(pos) else {
                    attr_runs.push((run_len, attrs));
                    break;
                };
                pos += 1;
                let (underline_color, advance) = deserialize_color(&data[pos..]);
                pos += advance;
                apply_extended_attrs(&mut attrs, ext, underline_color);
            }

            attr_runs.push((run_len, attrs));
        }
    }

    ScrollbackLine::from_raw_parts(text, cell_lens, attr_runs, wrapped)
}

fn deserialize_color(data: &[u8]) -> (ColorAttribute, usize) {
    if data.is_empty() {
        return (ColorAttribute::Default, 0);
    }
    match data[0] {
        0 => (ColorAttribute::Default, 1),
        1 => {
            if data.len() < 2 {
                return (ColorAttribute::Default, 1);
            }
            (ColorAttribute::PaletteIndex(data[1]), 2)
        }
        2 => {
            if data.len() < 4 {
                return (ColorAttribute::Default, 1);
            }
            let c = termwiz::color::SrgbaTuple(
                data[1] as f32 / 255.0,
                data[2] as f32 / 255.0,
                data[3] as f32 / 255.0,
                1.0,
            );
            (ColorAttribute::TrueColorWithDefaultFallback(c), 4)
        }
        _ => (ColorAttribute::Default, 1),
    }
}

#[cfg(test)]
impl DiskScrollback {
    pub(crate) fn backing_file(&self) -> &std::path::Path {
        &self.file_path
    }

    pub(crate) fn compaction_copies(&self) -> usize {
        self.compaction_copies
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn read_opens(&self) -> usize {
        self.read_opens.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn push_batches(&self) -> usize {
        self.push_batches
    }

    pub(crate) fn fail_append_after(&mut self, bytes: usize) {
        self.fail_append_after = Some(bytes);
    }

    pub(crate) fn file_size(&self) -> u64 {
        self.file_size
    }

    pub(crate) fn offsets_capacity(&self) -> usize {
        self.line_offsets.capacity()
    }

    pub(crate) fn physical_records(&self) -> usize {
        self.line_offsets.len()
    }

    pub(crate) fn fail_compaction(&mut self, fail: bool) {
        self.fail_compaction = fail;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termwiz::cell::Intensity;
    use termwiz::color::{ColorSpec, RgbColor};

    fn every_attribute() -> CellAttributes {
        let mut a = CellAttributes::default();
        a.set_underline(Underline::Curly);
        a.set_underline_color(ColorSpec::TrueColor(RgbColor::new_8bpc(10, 20, 30).into()));
        a.set_reverse(true);
        a.set_blink(Blink::Slow);
        a.set_invisible(true);
        a.set_overline(true);
        a
    }

    #[test]
    fn disk_round_trip_keeps_every_drawn_attribute() {
        let a = every_attribute();
        let line = ScrollbackLine::new(vec![("X".to_string(), a.clone())], false);
        let out = deserialize_lines(&serialize_lines(&[line])).unwrap();
        let cells = out[0].to_cells();
        assert_eq!(cells[0].1.underline(), Underline::Curly);
        assert_eq!(cells[0].1.underline_color(), a.underline_color());
        assert!(cells[0].1.reverse());
        assert_eq!(cells[0].1.blink(), Blink::Slow);
        assert!(cells[0].1.invisible());
        assert!(cells[0].1.overline());
    }

    #[test]
    fn each_extended_value_survives_alone() {
        let mut cases = Vec::new();
        for underline in [
            Underline::None,
            Underline::Single,
            Underline::Double,
            Underline::Curly,
            Underline::Dotted,
            Underline::Dashed,
        ] {
            let mut a = CellAttributes::default();
            a.set_underline(underline);
            cases.push(a);
        }
        for blink in [Blink::Slow, Blink::Rapid] {
            let mut a = CellAttributes::default();
            a.set_blink(blink);
            cases.push(a);
        }
        let mut palette_color = CellAttributes::default();
        palette_color.set_underline_color(ColorSpec::PaletteIndex(9));
        cases.push(palette_color);
        for a in cases {
            let out = round_trip(ScrollbackLine::new(vec![("X".into(), a.clone())], false));
            assert_eq!(out.to_cells()[0].1, a);
        }
    }

    /// A DiskScrollback file has the same records as a blob, and its reads ignore the header.
    #[test]
    fn disk_file_keeps_every_drawn_attribute() {
        let a = every_attribute();
        let mut disk = DiskScrollback::new(99174).unwrap();
        disk.push_lines(&[ScrollbackLine::new(vec![("X".into(), a.clone())], false)])
            .unwrap();
        let mut header = [0u8; 8];
        File::open(disk.backing_file())
            .unwrap()
            .read_exact(&mut header)
            .unwrap();
        assert_eq!(&header[4..], &FORMAT_VERSION.to_le_bytes());
        assert_eq!(disk.read_line(0).unwrap().unwrap().to_cells()[0].1, a);
    }

    /// Lines with no extended value keep the version 3 record size; others spend the extra bytes
    /// `serialized_record_len` reports.
    #[test]
    fn record_len_matches_the_serialized_bytes() {
        let mut single = CellAttributes::default();
        single.set_underline(Underline::Single);
        let mut reverse = CellAttributes::default();
        reverse.set_reverse(true);
        let plain_record = 27;
        for (a, expected) in [
            (CellAttributes::default(), plain_record),
            (single, plain_record),
            (reverse, plain_record + 2),
            (every_attribute(), plain_record + 5),
        ] {
            let line = ScrollbackLine::new(vec![("X".into(), a)], false);
            let actual = serialize_lines(std::slice::from_ref(&line)).len() - HEADER_LEN as usize;
            assert_eq!(actual, expected);
            assert_eq!(serialized_record_len(&line), expected);
        }
    }

    /// Written by the version 3 serializer before the format changed. The third cell had a curly
    /// underline and reverse, which version 3 stored only as "underlined".
    const VERSION_3_BLOB: &str = "545353420300000031000000010500000042eab08055030000000100030001\
        000300000001000000010900010100000000020a141e120100000000000c170000000001000000700100000001\
        000100000001000000000000";

    #[test]
    fn version_3_blob_stays_readable() {
        let blob: Vec<u8> = (0..VERSION_3_BLOB.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&VERSION_3_BLOB[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(blob[4], 3);
        let lines = deserialize_lines(&blob).expect("version 3 stays readable");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].wrapped);
        assert!(!lines[1].wrapped);
        let cells = lines[0].to_cells();
        assert_eq!(cells[0].0, "B");
        assert_eq!(cells[0].1.intensity(), Intensity::Bold);
        assert_eq!(cells[0].1.foreground(), ColorAttribute::PaletteIndex(9));
        assert_eq!(cells[1].0, "가");
        assert_eq!(cells[1].1.intensity(), Intensity::Half);
        assert!(cells[1].1.italic());
        let (r, g, b, _) = match cells[1].1.background() {
            ColorAttribute::TrueColorWithDefaultFallback(c) => c.to_tuple_rgba(),
            other => panic!("background {other:?}"),
        };
        assert_eq!([r, g, b].map(|v| (v * 255.0).round() as u8), [10, 20, 30]);
        assert_eq!(cells[2].0, "U");
        assert_eq!(cells[2].1.underline(), Underline::Single);
        assert!(cells[2].1.strikethrough());
        assert!(!cells[2].1.reverse());
        assert_eq!(cells[2].1.blink(), Blink::None);
        assert_eq!(lines[1].to_cells()[0].1, CellAttributes::default());
    }

    #[test]
    fn deserialize_rejects_versions_outside_the_readable_range() {
        for version in [OLDEST_READABLE_VERSION - 1, FORMAT_VERSION + 1] {
            let mut blob = serialize_lines(&[]);
            blob[4..8].copy_from_slice(&version.to_le_bytes());
            assert!(deserialize_lines(&blob).is_none(), "version {version}");
        }
    }

    #[test]
    fn overlapping_same_surface_instances_never_replace_or_remove_each_others_rows() {
        let mut original = DiskScrollback::new(99173).unwrap();
        let old = ScrollbackLine::new(vec![("original".into(), CellAttributes::default())], false);
        original.push_lines(&[old]).unwrap();
        let old_path = original.file_path.clone();
        {
            let mut candidate = DiskScrollback::new(99173).unwrap();
            assert_ne!(candidate.file_path, old_path);
            candidate
                .push_lines(&[ScrollbackLine::new(
                    vec![("candidate".into(), CellAttributes::default())],
                    false,
                )])
                .unwrap();
            assert_eq!(original.read_line(0).unwrap().unwrap().text, "original");
        }
        assert!(old_path.is_file());
        assert_eq!(original.read_line(0).unwrap().unwrap().text, "original");
        let mut replacement = DiskScrollback::new(99173).unwrap();
        replacement
            .push_lines(&[ScrollbackLine::new(
                vec![("replacement".into(), CellAttributes::default())],
                false,
            )])
            .unwrap();
        let replacement_path = replacement.file_path.clone();
        drop(original);
        assert!(!old_path.exists());
        assert!(replacement_path.is_file());
        assert_eq!(
            replacement.read_line(0).unwrap().unwrap().text,
            "replacement"
        );
        replacement
            .push_lines(&[ScrollbackLine::new(
                vec![("later".into(), CellAttributes::default())],
                false,
            )])
            .unwrap();
        assert_eq!(replacement.read_line(1).unwrap().unwrap().text, "later");
        drop(replacement);
        assert!(!replacement_path.exists());
    }

    fn round_trip(input: ScrollbackLine) -> ScrollbackLine {
        let bytes = serialize_line(&input);
        deserialize_line(&bytes)
    }

    #[test]
    fn preserves_intensity_half() {
        let mut a = CellAttributes::default();
        a.set_intensity(Intensity::Half);
        let out = round_trip(ScrollbackLine::new(vec![("D".into(), a)], false));
        let cells = out.to_cells();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].0, "D");
        assert_eq!(cells[0].1.intensity(), Intensity::Half);
        assert!(!out.wrapped);
    }

    #[test]
    fn preserves_intensity_bold() {
        let mut a = CellAttributes::default();
        a.set_intensity(Intensity::Bold);
        let out = round_trip(ScrollbackLine::new(vec![("B".into(), a)], false));
        assert_eq!(out.to_cells()[0].1.intensity(), Intensity::Bold);
    }

    #[test]
    fn preserves_other_attrs_alongside_half() {
        let mut a = CellAttributes::default();
        a.set_intensity(Intensity::Half);
        a.set_italic(true);
        a.set_underline(Underline::Single);
        a.set_strikethrough(true);
        let out = round_trip(ScrollbackLine::new(vec![("X".into(), a)], false));
        let cells = out.to_cells();
        let r = &cells[0].1;
        assert_eq!(r.intensity(), Intensity::Half);
        assert!(r.italic());
        assert_ne!(r.underline(), Underline::None);
        assert!(r.strikethrough());
    }

    #[test]
    fn preserves_wrapped_flag() {
        let a = CellAttributes::default();
        let out = round_trip(ScrollbackLine::new(vec![("W".into(), a)], true));
        assert!(out.wrapped);
        assert_eq!(out.to_cells()[0].0, "W");
    }

    #[test]
    fn preserves_multibyte_unicode_cells() {
        // Check multibyte cell boundaries and attribute runs together.
        let mut bold = CellAttributes::default();
        bold.set_intensity(Intensity::Bold);
        let mut italic = CellAttributes::default();
        italic.set_italic(true);

        let input = ScrollbackLine::new(
            vec![
                ("가".into(), bold.clone()),
                ("나".into(), CellAttributes::default()),
                ("다".into(), CellAttributes::default()),
                ("🦀".into(), italic.clone()),
                ("漢".into(), CellAttributes::default()),
            ],
            true,
        );
        let out = round_trip(input);
        let cells = out.to_cells();

        assert!(out.wrapped);
        assert_eq!(cells.len(), 5);

        assert_eq!(cells[0].0, "가");
        assert_eq!(cells[0].0.len(), 3);
        assert_eq!(cells[1].0, "나");
        assert_eq!(cells[2].0, "다");
        assert_eq!(cells[3].0, "🦀");
        assert_eq!(cells[3].0.len(), 4);
        assert_eq!(cells[4].0, "漢");
        assert_eq!(cells[4].0.len(), 3);

        assert_eq!(cells[0].1.intensity(), Intensity::Bold);
        assert_eq!(cells[1].1.intensity(), Intensity::Normal);
        assert!(!cells[1].1.italic());
        assert!(cells[3].1.italic());
        assert!(!cells[4].1.italic());
    }

    #[test]
    fn batch_serialize_round_trip() {
        let mut bold = CellAttributes::default();
        bold.set_intensity(Intensity::Bold);
        let lines = vec![
            ScrollbackLine::new(vec![("hello".into(), CellAttributes::default())], false),
            ScrollbackLine::new(vec![("world".into(), bold)], true),
        ];
        let bytes = serialize_lines(&lines);
        let out = deserialize_lines(&bytes).expect("deserialize");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].to_cells()[0].0, "hello");
        assert!(!out[0].wrapped);
        let l1 = out[1].to_cells();
        assert_eq!(l1[0].0, "world");
        assert!(out[1].wrapped);
        assert_eq!(l1[0].1.intensity(), Intensity::Bold);
    }

    #[test]
    fn deserialize_rejects_bad_header() {
        assert!(deserialize_lines(&[]).is_none());
        assert!(deserialize_lines(b"XXXX\x01\x00\x00\x00").is_none());
    }

    #[test]
    fn empty_batch_round_trip() {
        let bytes = serialize_lines(&[]);
        let out = deserialize_lines(&bytes).expect("deserialize");
        assert!(out.is_empty());
    }
}
