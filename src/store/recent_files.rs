//! 종류별 최근 파일 목록. 창들이 한 캐시를 공유하고, GUI는 state.db writer로 저장한다.
//! 매니페스트의 records_recent를 사용하는 파일 열기 경로와 directory 같은 내장 종류가 기록한다.
//!
//! 중복 비교는 정규화한 경로로 하고 표시·열기에는 원래 경로를 사용한다.
//! 캐시 변경은 바로 보이고(표시 revision) 저장은 writer가 끝낸 뒤 확정 revision으로 따로 센다.
//! 저장 접수·실행이 실패해도 메모리 캐시 변경은 유지된다.

use std::collections::HashMap;
#[cfg(any(feature = "gui", test))]
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// 파일 유형별 최대 보관 개수.
pub(crate) const MAX_ENTRIES: usize = 10;

/// 목록의 한 항목. `opened_at`은 실제로 연 시각(초)이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) path: String,
    pub(crate) opened_at: i64,
}

/// 한 종류의 전체 목록과 그 목록을 만든 표시 revision. 저장 요청 하나가 된다.
#[cfg(any(feature = "gui", test))]
#[derive(Debug, Clone)]
pub(crate) struct RecentSnapshot {
    pub(crate) kind: String,
    pub(crate) revision: u64,
    pub(crate) entries: Vec<Entry>,
}

/// 저장 요청 하나의 끝난 결과.
#[cfg(any(feature = "gui", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecentOutcome {
    /// 이 snapshot이 확정됐다.
    Committed,
    /// 같은 묶음의 뒤 snapshot이 대신 저장됐다. 그 snapshot의 결과가 따로 온다.
    Superseded,
    /// 확정하지 않았거나 확정 여부를 모른다.
    NotCommitted(String),
}

/// 저장소에 snapshot을 넘기는 쪽. 접수만 하고 기다리지 않는다.
#[cfg(any(feature = "gui", test))]
pub(crate) trait RecentSink: Send {
    /// 접수한다. 거절이면 이유를 돌려준다.
    fn submit(&mut self, snapshot: RecentSnapshot) -> Result<(), String>;
    /// 끝난 결과를 꺼낸다. 기다리지 않는다.
    fn reap(&mut self) -> Vec<(String, u64, RecentOutcome)>;
    /// 결과를 아직 꺼내지 않은 접수 수.
    #[cfg(test)]
    fn pending(&self) -> usize;
}

/// 종류별 표시·확정 revision. 확정은 표시를 넘지 않는다.
#[cfg(any(feature = "gui", test))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Revisions {
    pub(crate) visible: u64,
    pub(crate) committed: u64,
}

/// Nonconsuming cache-wide persistence state, independent of queued work age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecentSaveSnapshot {
    pub(crate) dirty: bool,
    pub(crate) dirty_age_us: Option<u64>,
}

// headless 비시험 조합에서는 by_kind만 남아 기본값이 derive와 같다.
#[cfg_attr(not(any(feature = "gui", test)), derive(Default))]
struct Inner {
    /// 종류별 최신순 목록.
    by_kind: HashMap<String, Vec<Entry>>,
    #[cfg(any(feature = "gui", test))]
    revisions: HashMap<String, Revisions>,
    #[cfg(any(feature = "gui", test))]
    sink: Option<Box<dyn RecentSink>>,
    #[cfg(any(feature = "gui", test))]
    clock: Arc<dyn tasty_utils::batch_writer::Clock>,
    #[cfg(any(feature = "gui", test))]
    dirty_since: Option<std::time::Instant>,
}

#[cfg(any(feature = "gui", test))]
impl Default for Inner {
    fn default() -> Self {
        Self {
            by_kind: HashMap::new(),
            #[cfg(any(feature = "gui", test))]
            revisions: HashMap::new(),
            #[cfg(any(feature = "gui", test))]
            sink: None,
            #[cfg(any(feature = "gui", test))]
            clock: Arc::new(tasty_utils::batch_writer::SystemClock),
            #[cfg(any(feature = "gui", test))]
            dirty_since: None,
        }
    }
}

#[derive(Default, Clone)]
pub struct RecentFiles {
    inner: Arc<Mutex<Inner>>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 중복 비교용 경로 키. 파일시스템의 동일 파일 여부를 확인하지는 않는다.
/// verbatim 접두사와 . / .. 표기를 정리하고 Windows에서는 소문자로 바꾼다.
/// 표시·열기에는 원래 경로를 사용한다.
pub(crate) fn dedup_key(path: &str) -> String {
    let stripped = tasty_utils::path::strip_verbatim_prefix(path);
    let normalized = tasty_utils::path::lexically_normalize(Path::new(&stripped));
    let key = normalized.to_string_lossy();
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key.into_owned()
    }
}

/// 최신순 입력에서 같은 정규화 키의 첫 항목을 남기고 나머지를 삭제 대상으로 분류한다.
#[cfg(any(feature = "gui", test))]
fn dedup_rows(rows: Vec<(String, i64)>) -> (Vec<Entry>, Vec<String>) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut kept = Vec::new();
    let mut stale = Vec::new();
    for (path, opened_at) in rows {
        if seen.insert(dedup_key(&path)) {
            kept.push(Entry { path, opened_at });
        } else {
            stale.push(path);
        }
    }
    (kept, stale)
}

impl RecentFiles {
    /// GUI의 state.db가 열려 있으면 모든 창이 공유하는 캐시를, 아니면 새 빈 캐시를 돌려준다.
    pub fn load() -> Self {
        crate::db::shared_recent_files().unwrap_or_default()
    }

    /// 부팅에서 읽은 목록으로 시작하고 변경을 `sink`로 저장하는 캐시.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn with_sink(
        by_kind: HashMap<String, Vec<Entry>>,
        sink: Box<dyn RecentSink>,
    ) -> Self {
        Self::with_clock(
            by_kind,
            sink,
            Arc::new(tasty_utils::batch_writer::SystemClock),
        )
    }

    #[cfg(any(feature = "gui", test))]
    pub(crate) fn with_clock(
        by_kind: HashMap<String, Vec<Entry>>,
        sink: Box<dyn RecentSink>,
        clock: Arc<dyn tasty_utils::batch_writer::Clock>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                by_kind,
                sink: Some(sink),
                clock,
                ..Inner::default()
            })),
        }
    }

    /// Reads cached scalars only. Ready receipts remain owned by settle.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn save_snapshot(&self) -> RecentSaveSnapshot {
        let inner = self.lock();
        RecentSaveSnapshot {
            dirty: inner.dirty_since.is_some(),
            dirty_age_us: inner.dirty_since.map(|since| {
                inner
                    .clock
                    .now()
                    .saturating_duration_since(since)
                    .as_micros()
                    .try_into()
                    .unwrap_or(u64::MAX)
            }),
        }
    }

    /// `kind` 의 최근 목록 스냅샷(최신순). 기록이 없으면 빈 배열.
    pub fn get(&self, kind: &str) -> Vec<String> {
        self.lock()
            .by_kind
            .get(kind)
            .map(|list| list.iter().map(|e| e.path.clone()).collect())
            .unwrap_or_default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        static POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::poison::recover_mutex(self.inner.lock(), "recent files cache", &POISONED)
    }

    /// 종류별 최신 목록에 추가하고 중복·개수 상한을 정리한다. 저장은 접수만 한다.
    pub fn add(&mut self, kind: &str, path: String) {
        let key = dedup_key(&path);
        // 두 창의 캐시 순서와 저장 접수 순서가 뒤바뀌지 않도록 같은 잠금 안에서 접수한다.
        // 이 잠금 안에서는 SQL·디스크·결과 대기를 하지 않는다.
        let mut inner = self.lock();
        let list = inner.by_kind.entry(kind.to_string()).or_default();
        list.retain(|e| dedup_key(&e.path) != key);
        list.insert(
            0,
            Entry {
                path,
                opened_at: now_secs(),
            },
        );
        list.truncate(MAX_ENTRIES);
        #[cfg(any(feature = "gui", test))]
        Self::persist(&mut inner, kind);
    }

    /// 표시 revision을 올리고 그 목록을 저장 접수한다.
    #[cfg(any(feature = "gui", test))]
    fn persist(inner: &mut Inner, kind: &str) {
        let Inner {
            by_kind,
            revisions,
            sink,
            clock,
            dirty_since,
        } = inner;
        let list = by_kind.get(kind).cloned().unwrap_or_default();
        let revision = revisions.entry(kind.to_string()).or_default();
        revision.visible += 1;
        dirty_since.get_or_insert_with(|| clock.now());
        let Some(sink) = sink else {
            tracing::trace!("recent_files save skipped: storage unavailable");
            return;
        };
        let snapshot = RecentSnapshot {
            kind: kind.to_string(),
            revision: revision.visible,
            entries: list,
        };
        if let Err(error) = sink.submit(snapshot) {
            tracing::warn!(kind, %error, "recent files change was not accepted for saving");
        }
    }

    /// 끝난 저장 결과를 거둬 확정 revision을 올린다. 기다리지 않는다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn settle(&self) {
        let mut inner = self.lock();
        let Inner {
            revisions, sink, ..
        } = &mut *inner;
        let Some(sink) = sink else {
            return;
        };
        for (kind, revision, outcome) in sink.reap() {
            match outcome {
                RecentOutcome::Committed => {
                    let r = revisions.entry(kind).or_default();
                    r.committed = r.committed.max(revision);
                }
                RecentOutcome::Superseded => {}
                RecentOutcome::NotCommitted(error) => {
                    tracing::warn!(kind, revision, %error, "recent files change was not saved");
                }
            }
        }
        if revisions.values().all(|r| r.committed >= r.visible) {
            inner.dirty_since = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn revisions(&self, kind: &str) -> Revisions {
        self.lock().revisions.get(kind).copied().unwrap_or_default()
    }

    /// 표시했지만 확정하지 못한 종류와 그 revision.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn unsaved(&self) -> Vec<(String, Revisions)> {
        let mut out: Vec<_> = self
            .lock()
            .revisions
            .iter()
            .filter(|(_, r)| r.committed < r.visible)
            .map(|(k, r)| (k.clone(), *r))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// 결과를 아직 거두지 않은 저장 접수 수.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.lock().sink.as_ref().map_or(0, |s| s.pending())
    }
}

/// 부팅 때 한 번 읽는다. 이전 테이블 이관과 기존 중복 행 정리를 한 transaction으로 하고 종류별
/// 최신 10개를 돌려준다. 실패한 단계는 경고하고 읽은 만큼으로 시작한다.
/// 같은 시각의 행은 나중에 넣은 행을 먼저 둔다.
#[cfg(any(feature = "gui", test))]
pub(crate) fn load_at_boot(conn: &mut rusqlite::Connection) -> HashMap<String, Vec<Entry>> {
    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("recent_files load could not start a transaction: {e}");
            return HashMap::new();
        }
    };
    migrate_recent_markdown(&tx);
    let rows = query_kind_rows(
        &tx,
        "SELECT f.kind, f.path, f.opened_at, o.rank FROM recent_files f
         LEFT JOIN recent_file_order o
           ON o.kind = f.kind AND o.path = f.path AND o.opened_at = f.opened_at
         ORDER BY f.opened_at DESC, f.rowid DESC",
    );
    let mut grouped: HashMap<String, Vec<(String, i64, Option<i64>)>> = HashMap::new();
    for (kind, path, ts, rank) in rows {
        grouped.entry(kind).or_default().push((path, ts, rank));
    }
    let grouped = grouped
        .into_iter()
        .map(|(kind, rows)| (kind, in_list_order(rows)));
    let mut by_kind: HashMap<String, Vec<Entry>> = HashMap::new();
    for (kind, kind_rows) in grouped {
        let (mut kept, stale) = dedup_rows(kind_rows);
        delete_paths(&tx, &kind, &stale);
        kept.truncate(MAX_ENTRIES);
        by_kind.insert(kind, kept);
    }
    if let Err(e) = tx.commit() {
        tracing::warn!("recent_files load cleanup commit failed: {e}");
    }
    by_kind
}

/// 그 종류의 모든 행에 경로·시각이 맞는 순서 정보가 있으면 저장한 목록 순서로, 하나라도 없으면
/// 시각·rowid 최신순(호출자가 넘긴 순서)으로 둔다. 순서 정보가 없는 기존 DB나 이 표를 모르는 이전
/// 바이너리가 행을 바꾼 경우가 뒤쪽이며, 다음 저장이 새 순서를 기록한다.
#[cfg(any(feature = "gui", test))]
fn in_list_order(mut rows: Vec<(String, i64, Option<i64>)>) -> Vec<(String, i64)> {
    if rows.iter().all(|(_, _, rank)| rank.is_some()) {
        rows.sort_by_key(|(_, _, rank)| *rank);
    }
    rows.into_iter().map(|(path, ts, _)| (path, ts)).collect()
}

/// recent_markdown을 이관하고 완료 플래그로 재실행을 막는다. 원래 테이블은 유지한다.
/// 복사나 플래그 기록에 실패하면 다음 로드에서 다시 시도할 수 있다.
#[cfg(any(feature = "gui", test))]
fn migrate_recent_markdown(conn: &rusqlite::Connection) {
    let migrated: bool = conn
        .query_row(
            "SELECT 1 FROM meta WHERE key = 'recent_files_migrated'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if migrated {
        return;
    }
    let has_legacy: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='recent_markdown'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if has_legacy
        && let Err(e) = conn.execute(
            "INSERT OR IGNORE INTO recent_files(kind, path, opened_at)
             SELECT 'markdown', path, opened_at FROM recent_markdown",
            [],
        )
    {
        tracing::warn!("recent_markdown → recent_files migration failed: {e}");
        return;
    }
    if let Err(e) = conn.execute(
        "INSERT OR IGNORE INTO meta(key, value) VALUES('recent_files_migrated', '1')",
        [],
    ) {
        tracing::warn!("recent_files migration flag set failed: {e}");
    }
}

/// 최근 파일 행: kind, path, opened_at, 시각이 맞는 순서 정보의 rank.
#[cfg(any(feature = "gui", test))]
type RecentRow = (String, String, i64, Option<i64>);

#[cfg(any(feature = "gui", test))]
fn query_kind_rows(conn: &rusqlite::Connection, sql: &str) -> Vec<RecentRow> {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("recent_files prepare failed: {e}");
            return Vec::new();
        }
    };
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)));
    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            tracing::warn!("recent_files query failed: {e}");
            Vec::new()
        }
    }
}

/// `paths` 의 행을 `recent_files` 에서 `kind` 별로 삭제. 빈 목록이면 no-op.
#[cfg(any(feature = "gui", test))]
fn delete_paths(conn: &rusqlite::Connection, kind: &str, paths: &[String]) {
    for path in paths {
        if let Err(e) = conn.execute(
            "DELETE FROM recent_files WHERE kind = ?1 AND path = ?2",
            rusqlite::params![kind, path],
        ) {
            tracing::warn!("recent_files delete failed: {e}");
        }
    }
}

/// snapshot 하나를 열린 transaction 안에서 저장한다. 그 종류의 행과 순서 정보를 지우고 목록을
/// 다시 넣으므로 같은 키의 다른 표기 정리와 최신 10개 밖 정리가 함께 끝나고, 디스크가 확정한 목록과
/// 같아진다. opened_at은 연 시각 그대로 두고 목록 순서는 recent_file_order의 rank로 남긴다.
/// 어느 단계든 실패하면 오류를 돌려 호출자가 묶음 전체를 되돌리게 한다.
#[cfg(any(feature = "gui", test))]
pub(crate) fn write_snapshot(
    conn: &rusqlite::Connection,
    snapshot: &RecentSnapshot,
) -> rusqlite::Result<()> {
    let kind = snapshot.kind.as_str();
    conn.execute("DELETE FROM recent_files WHERE kind = ?1", [kind])?;
    conn.execute("DELETE FROM recent_file_order WHERE kind = ?1", [kind])?;
    for (rank, entry) in snapshot.entries.iter().enumerate() {
        conn.execute(
            "INSERT INTO recent_files(kind, path, opened_at) VALUES(?1, ?2, ?3)",
            rusqlite::params![kind, entry.path, entry.opened_at],
        )?;
        conn.execute(
            "INSERT INTO recent_file_order(kind, path, opened_at, rank) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![kind, entry.path, entry.opened_at, rank as i64],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 창마다 받은 핸들이 같은 캐시를 본다. 저장소가 없어도 캐시 변경은 남는다.
    #[test]
    fn handles_cloned_for_windows_share_recent_updates() {
        let mut first = RecentFiles::default();
        let mut second = first.clone();
        first.add("markdown", "/notes/first.md".into());
        assert_eq!(second.get("markdown"), vec!["/notes/first.md"]);
        second.add("markdown", "/notes/second.md".into());
        first.add("markdown", "/notes/./first.md".into());
        assert_eq!(first.get("markdown"), second.get("markdown"));
        assert_eq!(second.get("markdown")[0], "/notes/./first.md");
        assert_eq!(first.revisions("markdown").visible, 3);
        assert_eq!(first.revisions("markdown").committed, 0);
    }

    #[test]
    #[cfg(windows)]
    fn dedup_key_folds_separator_and_case_windows() {
        assert_eq!(dedup_key(r"E:\a\B.md"), dedup_key("E:/a/b.md"),);
        assert_eq!(dedup_key(r"\\?\E:\a\b.md"), dedup_key(r"E:\a\b.md"),);
        assert_eq!(dedup_key(r"E:\a\md\..\b.md"), dedup_key(r"E:\a\b.md"),);
    }

    #[test]
    #[cfg(windows)]
    fn dedup_key_keeps_distinct_files_windows() {
        assert_ne!(dedup_key(r"E:\a\b.md"), dedup_key(r"E:\a\c.md"));
    }

    #[test]
    #[cfg(not(windows))]
    fn dedup_key_folds_normalization_unix() {
        assert_eq!(dedup_key("/a/md/../b.md"), dedup_key("/a/b.md"));
        assert_eq!(dedup_key("/a/./b.md"), dedup_key("/a/b.md"));
        assert_ne!(dedup_key("/a/B.md"), dedup_key("/a/b.md"));
    }

    #[test]
    fn dedup_rows_keeps_latest_per_key() {
        #[cfg(windows)]
        let (raw_new, raw_old, other) = (r"E:\a\b.md", "E:/a/B.md", r"E:\a\c.md");
        #[cfg(not(windows))]
        let (raw_new, raw_old, other) = ("/a/b.md", "/a/./b.md", "/a/c.md");

        let rows = vec![
            (raw_new.to_string(), 300),
            (other.to_string(), 200),
            (raw_old.to_string(), 100),
        ];
        let (kept, stale) = dedup_rows(rows);
        let kept: Vec<_> = kept.into_iter().map(|e| e.path).collect();
        assert_eq!(kept, vec![raw_new.to_string(), other.to_string()]);
        assert_eq!(stale, vec![raw_old.to_string()]);
    }

    #[test]
    fn dedup_rows_noop_when_all_distinct() {
        #[cfg(windows)]
        let (a, b) = (r"E:\a.md", r"E:\b.md");
        #[cfg(not(windows))]
        let (a, b) = ("/a.md", "/b.md");
        let rows = vec![(a.to_string(), 2), (b.to_string(), 1)];
        let (kept, stale) = dedup_rows(rows);
        let kept: Vec<_> = kept.into_iter().map(|e| e.path).collect();
        assert_eq!(kept, vec![a.to_string(), b.to_string()]);
        assert!(stale.is_empty());
    }

    #[test]
    fn get_absent_kind_is_empty() {
        let rf = RecentFiles::default();
        assert!(rf.get("markdown").is_empty());
    }

    #[test]
    fn directory_kind_recent_latest_first_dedup_and_cap() {
        let mut rf = RecentFiles::default();
        #[cfg(windows)]
        let mk = |i: usize| format!(r"E:\dir{i}");
        #[cfg(not(windows))]
        let mk = |i: usize| format!("/dir{i}");

        for i in 0..12 {
            rf.add("directory", mk(i));
        }
        let list = rf.get("directory");
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0], mk(11)); // 마지막 add 가 맨 앞.
        assert_eq!(list[9], mk(2)); // 가장 오래된 유지분.

        rf.add("directory", mk(2));
        let list = rf.get("directory");
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0], mk(2));
        assert_eq!(list.iter().filter(|p| **p == mk(2)).count(), 1);

        assert!(rf.get("markdown").is_empty());
    }

    #[test]
    fn add_and_get_in_memory_dedup() {
        // DB가 없어도 캐시는 갱신돼야 한다.
        let mut rf = RecentFiles::default();
        #[cfg(windows)]
        let (p1, p1_alt, p2) = (r"E:\a\b.md", "E:/a/B.md", r"E:\a\c.md");
        #[cfg(not(windows))]
        let (p1, p1_alt, p2) = ("/a/b.md", "/a/./b.md", "/a/c.md");

        rf.add("markdown", p2.to_string());
        rf.add("markdown", p1.to_string());
        rf.add("markdown", p1_alt.to_string());

        let list = rf.get("markdown");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], p1_alt.to_string());
        assert_eq!(list[1], p2.to_string());
        assert!(rf.get("html").is_empty());
    }
}

#[cfg(test)]
mod dirty_age_tests;
