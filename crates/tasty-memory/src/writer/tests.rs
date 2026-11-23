use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tasty_utils::batch_writer::{
    Completion, FlushFailure, FlushPoint, Limits, ManualClock, SubmitError, SystemClock,
};

use super::*;
use crate::{HOST_OWNER, PutOpts, Scope};
use tasty_utils::test_wait::{holds_by, holds_by_every};

const WAIT: Duration = Duration::from_secs(10);

fn text(s: &str) -> MemoryValue {
    MemoryValue::Text(s.into())
}

fn put(owner: &str, key: &str, value: &str, cas: Option<u64>) -> WriteOp {
    WriteOp::Put {
        owner: owner.into(),
        scope: Scope::Global,
        key: key.into(),
        value: text(value),
        opts: PutOpts {
            expires_at: None,
            cas,
        },
    }
}

/// 묶음 실행 전에 멈출 수 있는 공유 저장소 접근. 운영 경로와 같은 공유 잠금을 쓴다.
struct Gated {
    shared: WriterStore,
    open: Arc<(Mutex<bool>, Condvar)>,
}

impl StoreAccess for Gated {
    fn with_store<R>(&mut self, f: impl FnOnce(&mut dyn MemoryStorage) -> R) -> R {
        let (lock, cv) = &*self.open;
        let mut open = lock.lock().unwrap();
        while !*open {
            open = cv.wait(open).unwrap();
        }
        drop(open);
        self.shared.with_store(f)
    }
}

struct Rig {
    writer: MemoryWriter<Gated>,
    handle: MemoryHandle<Gated>,
    store: Arc<Mutex<MemoryStore>>,
    open: Arc<(Mutex<bool>, Condvar)>,
    clock: Arc<ManualClock>,
    changes: Arc<ChangeFeed>,
}

impl Rig {
    fn release(&self) {
        *self.open.0.lock().unwrap() = true;
        self.open.1.notify_all();
    }
    fn commits(&self) -> u64 {
        self.store.lock().unwrap().db_latency().snapshot().commits
    }
    fn tick(&self) {
        self.clock.advance(Duration::from_millis(2));
    }
    fn shutdown(mut self) {
        self.release();
        self.writer.begin_shutdown();
        let deadline = Instant::now() + WAIT;
        // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
        assert!(
            holds_by_every(deadline, Duration::from_millis(1), |_| self
                .writer
                .poll_shutdown()
                == 0),
            "writer did not join"
        );
    }
}

fn rig_with(store: MemoryStore, limits: Limits, open: bool) -> Rig {
    let store = Arc::new(Mutex::new(store));
    let dyn_store: Arc<Mutex<dyn MemoryStorage>> = store.clone();
    let gate = Arc::new((Mutex::new(open), Condvar::new()));
    let clock = Arc::new(ManualClock::default());
    let changes = Arc::new(ChangeFeed::default());
    let writer = spawn(
        Gated {
            shared: WriterStore::shared(dyn_store),
            open: gate.clone(),
        },
        changes.clone(),
        limits,
        clock.clone(),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.handle();
    Rig {
        writer,
        handle,
        store,
        open: gate,
        clock,
        changes,
    }
}

fn file_store(dir: &Path) -> MemoryStore {
    MemoryStore::open(&dir.join("memory.db")).unwrap()
}

fn done(c: MemoryCompletion) -> std::result::Result<MemoryOutput, WriterError> {
    match c {
        Completion::Done(r) => r,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn version(c: MemoryCompletion) -> u64 {
    match done(c) {
        Ok(MemoryOutput::Written(WriteOutput::Version(v))) => v,
        other => panic!("expected a version, got {other:?}"),
    }
}

fn rejection(c: MemoryCompletion) -> &'static str {
    match done(c) {
        Err(WriterError::Rejected { error, .. }) => match *error {
            MemoryError::CasConflict { .. } => "cas",
            MemoryError::OwnedByOther { .. } => "owner",
            MemoryError::NotFound { .. } => "not_found",
            _ => "other",
        },
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
fn accepted_requests_wait_for_one_commit_and_survive_a_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let rig = rig_with(file_store(tmp.path()), Limits::default(), false);
    let before = rig.commits();
    let started = Instant::now();
    let receipts: Vec<_> = (0..100)
        .map(|i| {
            rig.handle
                .submit(MemoryRequest::Write(put(
                    HOST_OWNER,
                    &format!("k{i}"),
                    "v",
                    None,
                )))
                .unwrap()
        })
        .collect();
    let submit_time = started.elapsed();
    assert!(submit_time < Duration::from_secs(1), "{submit_time:?}");
    rig.tick();
    std::thread::sleep(Duration::from_millis(20));
    assert!(receipts.iter().all(|r| r.try_take().is_none()));
    assert_eq!(
        rig.commits(),
        before,
        "nothing commits while the worker is held"
    );

    rig.release();
    for r in &receipts {
        assert_eq!(version(r.wait_timeout(WAIT).unwrap()), 1);
    }
    assert_eq!(rig.commits(), before + 1, "a hundred requests, one commit");
    assert_eq!(rig.handle.stats().batches, 1);
    rig.shutdown();

    let reopened = file_store(tmp.path());
    assert_eq!(reopened.count(&Scope::Global, Some("k")).unwrap(), 100);
}

#[test]
fn requests_in_one_batch_are_judged_in_order_and_rejections_stay_alone() {
    let rig = rig_with(
        MemoryStore::open_in_memory().unwrap(),
        Limits::default(),
        false,
    );
    let submit = |op| rig.handle.submit(MemoryRequest::Write(op)).unwrap();
    let a = submit(put("com.a", "k", "1", None));
    let b = submit(put("com.a", "k", "2", Some(1)));
    let stale = submit(put("com.a", "k", "3", Some(1)));
    let other = submit(put("com.b", "k", "4", None));
    let atomic = rig
        .handle
        .submit(MemoryRequest::Batch {
            mode: BatchMode::Atomic,
            ops: vec![
                put("com.a", "x", "1", None),
                put("com.a", "k", "5", Some(9)),
            ],
        })
        .unwrap();
    let after = submit(put("com.a", "k", "6", Some(2)));
    rig.tick();
    rig.release();
    assert_eq!(version(a.wait_timeout(WAIT).unwrap()), 1);
    assert_eq!(version(b.wait_timeout(WAIT).unwrap()), 2);
    assert_eq!(rejection(stale.wait_timeout(WAIT).unwrap()), "cas");
    assert_eq!(rejection(other.wait_timeout(WAIT).unwrap()), "owner");
    match done(atomic.wait_timeout(WAIT).unwrap()) {
        Err(WriterError::Rejected { index, .. }) => assert_eq!(index, Some(1)),
        other => panic!("{other:?}"),
    }
    assert_eq!(version(after.wait_timeout(WAIT).unwrap()), 3);
    {
        let mut s = rig.store.lock().unwrap();
        assert!(
            s.get(&Scope::Global, "x").unwrap().is_none(),
            "the atomic batch rolls back alone"
        );
        assert!(s.take_pending_changes().is_empty(), "the writer moved them");
        let changes = rig.changes.take();
        assert_eq!(
            changes.len(),
            3,
            "rejected requests leave no change: {changes:?}"
        );
        assert_eq!(s.db_latency().snapshot().commits, 1);
    }
    rig.shutdown();
}

#[test]
fn a_compound_operation_reads_judges_and_writes_without_a_second_submit() {
    let rig = rig_with(
        MemoryStore::open_in_memory().unwrap(),
        Limits::default(),
        false,
    );
    // permit 하나짜리 semaphore: 비어 있을 때만 점유한다.
    let acquire = |who: &'static str| {
        MemoryRequest::unit(64, move |s| {
            if s.count(&Scope::Global, Some("permit."))? >= 1 {
                return Ok(false);
            }
            s.put(
                HOST_OWNER,
                &Scope::Global,
                &format!("permit.{who}"),
                &text(who),
                &PutOpts::default(),
            )?;
            Ok(true)
        })
    };
    let first = rig.handle.submit(acquire("a")).unwrap();
    let second = rig.handle.submit(acquire("b")).unwrap();
    rig.tick();
    rig.release();
    let got = |r: &batch_writer::Receipt<MemoryCompletion>| {
        done(r.wait_timeout(WAIT).unwrap())
            .unwrap()
            .into_value::<bool>()
            .unwrap()
    };
    assert!(got(&first));
    assert!(!got(&second), "the second sees the first in the same batch");
    assert_eq!(rig.handle.stats().batches, 1);
    rig.shutdown();
}

#[test]
fn a_strong_read_sees_every_request_accepted_before_it_after_their_commit() {
    let rig = rig_with(
        MemoryStore::open_in_memory().unwrap(),
        Limits::default(),
        false,
    );
    let w = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
        .unwrap();
    let r = rig
        .handle
        .submit(MemoryRequest::read(64, |s| {
            Ok(s.get(&Scope::Global, "k")?.map(|e| e.version))
        }))
        .unwrap();
    rig.release();
    assert_eq!(version(w.wait_timeout(WAIT).unwrap()), 1);
    let seen = done(r.wait_timeout(WAIT).unwrap())
        .unwrap()
        .into_value::<Option<u64>>()
        .unwrap();
    assert_eq!(seen, Some(1));
    assert_eq!(rig.handle.stats().batches, 2, "the read closed the window");
    assert_eq!(rig.commits(), 1, "the read itself commits nothing");
    rig.shutdown();
}

#[test]
fn a_storage_failure_fails_every_request_of_the_batch_and_publishes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = file_store(tmp.path());
    let pages: i64 = store
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    store
        .conn
        .query_row(&format!("PRAGMA max_page_count={}", pages + 2), [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    let rig = rig_with(store, Limits::default(), false);
    let big = "x".repeat(64 * 1024);
    let small = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "small", "1", None)))
        .unwrap();
    let huge = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "big", &big, None)))
        .unwrap();
    rig.tick();
    rig.release();
    for r in [&small, &huge] {
        match r.wait_timeout(WAIT).unwrap() {
            Completion::Failed(WriterError::Storage(e)) => {
                assert_eq!(e.storage_failure(), Some(StorageFailure::DiskFull))
            }
            other => panic!("{other:?}"),
        }
    }
    {
        let mut s = rig.store.lock().unwrap();
        assert!(s.get(&Scope::Global, "small").unwrap().is_none());
        assert!(s.take_pending_changes().is_empty());
        assert_eq!(s.db_latency().snapshot().commits, 0);
    }
    let flush = rig.handle.flush(FlushPoint::START).unwrap();
    assert!(matches!(
        flush.wait_timeout(WAIT).unwrap(),
        Err(FlushFailure::NotClean { failed: 2, .. })
    ));
    rig.shutdown();
}

#[test]
fn a_failure_no_request_saw_is_reported_as_unknown_and_not_run_again() {
    let store = MemoryStore::open_in_memory().unwrap();
    crate::batch::plant_unreadable_row_for_tests(&store);
    let rig = rig_with(store, Limits::default(), false);
    let runs = Arc::new(Mutex::new(0));
    let counted = runs.clone();
    let swallowing = rig
        .handle
        .submit(MemoryRequest::unit(0, move |s| {
            *counted.lock().unwrap() += 1;
            s.put(
                HOST_OWNER,
                &Scope::Global,
                "a",
                &text("1"),
                &PutOpts::default(),
            )?;
            // 조회 실패를 삼켜도 단위는 깨진다. writer는 원인 요청을 알 수 없다.
            Ok(s.get(&Scope::Global, "unreadable").is_err())
        }))
        .unwrap();
    rig.tick();
    rig.release();
    assert!(matches!(
        swallowing.wait_timeout(WAIT).unwrap(),
        Completion::Unknown(WriterError::Storage(_))
    ));
    assert_eq!(*runs.lock().unwrap(), 1);
    assert!(
        rig.store
            .lock()
            .unwrap()
            .get(&Scope::Global, "a")
            .unwrap()
            .is_none()
    );
    rig.shutdown();
}

#[test]
fn a_panicking_unit_fails_alone_and_the_rest_of_its_batch_commits_once() {
    let rig = rig_with(
        MemoryStore::open_in_memory().unwrap(),
        Limits::default(),
        false,
    );
    let a = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "a", "1", None)))
        .unwrap();
    let bad = rig
        .handle
        .submit(MemoryRequest::unit(0, |s| -> Result<()> {
            s.put(
                HOST_OWNER,
                &Scope::Global,
                "bad",
                &text("1"),
                &PutOpts::default(),
            )?;
            panic!("unit panic for a test")
        }))
        .unwrap();
    let c = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "a", "2", Some(1))))
        .unwrap();
    rig.tick();
    rig.release();
    assert!(matches!(
        bad.wait_timeout(WAIT).unwrap(),
        Completion::Failed(WriterError::Panicked)
    ));
    assert_eq!(version(a.wait_timeout(WAIT).unwrap()), 1);
    assert_eq!(
        version(c.wait_timeout(WAIT).unwrap()),
        2,
        "run again from scratch, not on top of the rolled-back attempt"
    );
    let mut s = rig.store.lock().unwrap();
    assert!(s.get(&Scope::Global, "bad").unwrap().is_none());
    assert_eq!(s.db_latency().snapshot().commits, 1);
    assert!(s.take_pending_changes().is_empty(), "the writer moved them");
    assert_eq!(rig.changes.take().len(), 2);
    drop(s);
    rig.shutdown();
}

fn lock_holder(path: &Path) -> rusqlite::Connection {
    let holder = rusqlite::Connection::open(path).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    holder
}

#[test]
fn busy_is_retried_until_the_lock_frees_and_then_commits_once() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory.db");
    let store = MemoryStore::open(&path).unwrap();
    store.conn.busy_timeout(Duration::ZERO).unwrap();
    let holder = lock_holder(&path);
    let store = Arc::new(Mutex::new(store));
    let dyn_store: Arc<Mutex<dyn MemoryStorage>> = store.clone();
    let writer = spawn(
        WriterStore::shared(dyn_store),
        Arc::default(),
        Limits {
            retry_for: Duration::from_secs(5),
            ..Limits::default()
        },
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.handle();
    let r = handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
        .unwrap();
    let deadline = Instant::now() + WAIT;
    assert!(holds_by(deadline, || handle.stats().retries >= 2));
    assert!(r.try_take().is_none());
    holder.execute_batch("ROLLBACK").unwrap();
    assert_eq!(version(r.wait_timeout(WAIT).unwrap()), 1);
    assert_eq!(store.lock().unwrap().db_latency().snapshot().commits, 1);
    drop(writer);
}

#[test]
fn busy_past_the_retry_window_fails_and_the_next_request_still_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory.db");
    let store = MemoryStore::open(&path).unwrap();
    store.conn.busy_timeout(Duration::ZERO).unwrap();
    let holder = lock_holder(&path);
    let rig = rig_with(
        store,
        Limits {
            retry_for: Duration::from_millis(30),
            ..Limits::default()
        },
        true,
    );
    let r = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
        .unwrap();
    rig.tick();
    match r.wait_timeout(WAIT).unwrap() {
        Completion::Failed(WriterError::Storage(e)) => {
            assert_eq!(e.storage_failure(), Some(StorageFailure::Busy))
        }
        other => panic!("{other:?}"),
    }
    holder.execute_batch("ROLLBACK").unwrap();
    let next = rig
        .handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
        .unwrap();
    rig.tick();
    assert_eq!(version(next.wait_timeout(WAIT).unwrap()), 1);
    rig.shutdown();
}

#[test]
fn a_dropped_receipt_still_commits_and_shutdown_drains_before_join() {
    let tmp = tempfile::tempdir().unwrap();
    let mut rig = rig_with(file_store(tmp.path()), Limits::default(), false);
    drop(
        rig.handle
            .submit(MemoryRequest::Write(put(HOST_OWNER, "kept", "1", None)))
            .unwrap(),
    );
    rig.writer.begin_shutdown();
    assert!(matches!(
        rig.handle
            .submit(MemoryRequest::Write(put(HOST_OWNER, "late", "1", None))),
        Err(SubmitError::Closed)
    ));
    assert_eq!(
        rig.writer.poll_shutdown(),
        1,
        "held worker is not joined yet"
    );
    rig.shutdown();
    let reopened = file_store(tmp.path());
    assert!(reopened.get(&Scope::Global, "kept").unwrap().is_some());
    assert!(reopened.get(&Scope::Global, "late").unwrap().is_none());
}

#[test]
fn the_production_access_serializes_with_direct_lock_holders() {
    let store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let writer = spawn(
        WriterStore::shared(store.clone()),
        Arc::default(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.handle();
    let guard = store.lock().unwrap();
    let r = handle
        .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
        .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert!(
        r.try_take().is_none(),
        "the worker waits for the shared lock"
    );
    drop(guard);
    assert_eq!(version(r.wait_timeout(WAIT).unwrap()), 1);
    drop(writer);
}

/// writer 없는 구성은 같은 실행 규칙으로 바로 실행하고, 두 경로 모두 commit된 변경을 버퍼로 옮긴다.
#[test]
fn both_routes_settle_the_same_way_and_feed_committed_changes() {
    let inline_store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let inline = MemoryAccess::inline(inline_store.clone());
    assert!(!inline.is_queued());
    let queued_store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let changes = Arc::new(ChangeFeed::default());
    let mut writer = spawn(
        WriterStore::shared(queued_store),
        changes.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let queued = MemoryAccess::writer(writer.handle(), changes);
    assert!(queued.is_queued());
    for access in [&inline, &queued] {
        let mut ok = access
            .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "1", None)))
            .unwrap();
        let mut stale = access
            .submit(MemoryRequest::Write(put(HOST_OWNER, "k", "2", Some(9))))
            .unwrap();
        match settle(ok.wait_timeout(WAIT).unwrap()) {
            Ok(MemoryOutput::Written(WriteOutput::Version(1))) => {}
            other => panic!("{other:?}"),
        }
        match settle(stale.wait_timeout(WAIT).unwrap()) {
            Err(Failure::Rejected { error, .. }) => {
                assert!(matches!(*error, crate::MemoryError::CasConflict { .. }))
            }
            other => panic!("{other:?}"),
        }
        assert!(ok.try_take().is_none(), "a result is taken once");
        let changes = access.changes().take();
        assert_eq!(changes.len(), 1, "only the committed put: {changes:?}");
        // 기다리는 실행도 같은 결과와 같은 거절을 돌려준다.
        match access.run_and_wait(MemoryRequest::Write(put(HOST_OWNER, "w", "1", None)), WAIT) {
            Ok(MemoryOutput::Written(WriteOutput::Version(1))) => {}
            other => panic!("{other:?}"),
        }
        match access.run_and_wait(
            MemoryRequest::Write(put(HOST_OWNER, "w", "2", Some(9))),
            WAIT,
        ) {
            Err(crate::writer::Waited::Failed(Failure::Rejected { .. })) => {}
            other => panic!("{other:?}"),
        }
        access.changes().take();
    }
    assert!(
        inline_store
            .lock()
            .unwrap()
            .take_pending_changes()
            .is_empty()
    );
    writer.begin_shutdown();
    let deadline = Instant::now() + WAIT;
    // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
    assert!(
        holds_by_every(deadline, Duration::from_millis(1), |_| writer
            .poll_shutdown()
            == 0),
        "writer did not join"
    );
}

/// writer가 소유한 저장소는 writer와 함께 닫힌다. 다른 소비자 없이 접수한 쓰기가 남고, worker가
/// 끝나면 연결을 닫아 WAL 파일이 정리된다.
#[test]
fn an_owned_store_closes_with_its_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory.db");
    let store = MemoryStore::open(&path).unwrap();
    let changes = Arc::new(ChangeFeed::default());
    let mut writer = spawn(
        WriterStore::owned(store),
        changes.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let access = MemoryAccess::writer(writer.handle(), changes);
    let mut first = access
        .submit(MemoryRequest::Write(put(HOST_OWNER, "owned", "1", None)))
        .unwrap();
    assert_eq!(version(first.wait_timeout(WAIT).unwrap()), 1);
    let wal = dir.path().join("memory.db-wal");
    assert!(wal.exists(), "the open connection keeps its WAL");
    writer.begin_shutdown();
    let deadline = Instant::now() + WAIT;
    // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
    assert!(
        holds_by_every(deadline, Duration::from_millis(1), |_| writer
            .poll_shutdown()
            == 0),
        "writer did not join"
    );
    drop(writer);
    assert!(!wal.exists(), "the store closed with its writer");
    let reopened = MemoryStore::open(&path).unwrap();
    let entry = reopened.get(&Scope::Global, "owned").unwrap().unwrap();
    assert_eq!(entry.version, 1);
}

fn queued_access(
    store: Arc<Mutex<dyn MemoryStorage>>,
    limits: Limits,
) -> (MemoryWriter<WriterStore>, MemoryAccess) {
    let changes = Arc::new(ChangeFeed::default());
    let writer = spawn(
        WriterStore::shared(store),
        changes.clone(),
        limits,
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let access = MemoryAccess::writer(writer.handle(), changes);
    (writer, access)
}

/// 본문은 writer 순서 안에서 실행되고, 호출은 writer가 본문을 실행할 때까지 돌아오지 않는다.
#[test]
fn a_blocking_body_runs_on_the_writer_and_the_call_waits_for_it() {
    let store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let (writer, access) = queued_access(store.clone(), Limits::default());
    let guard = store.lock().unwrap();
    let returned = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = returned.clone();
    let caller = std::thread::spawn(move || {
        let key = String::from("borrowed");
        let value = access
            .run_blocking(0, move |s| {
                s.put(
                    HOST_OWNER,
                    &Scope::Global,
                    &key,
                    &text("v"),
                    &PutOpts::default(),
                )
                .map(|v| (key.len(), v))
            })
            .unwrap();
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        value
    });
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !returned.load(std::sync::atomic::Ordering::SeqCst),
        "the call waits while the writer cannot run the body"
    );
    drop(guard);
    assert_eq!(caller.join().unwrap().unwrap(), (8, 1));
    assert!(
        store
            .lock()
            .unwrap()
            .get(&Scope::Global, "borrowed")
            .unwrap()
            .is_some()
    );
    drop(writer);
}

/// 다른 요청의 저장 실패로 묶음이 되돌려지면 본문을 새 요청으로 다시 넣어 혼자 확정한다.
#[test]
fn a_body_failed_by_a_neighbour_is_submitted_again_and_commits_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let file = file_store(tmp.path());
    let pages: i64 = file
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    file.conn
        .query_row(&format!("PRAGMA max_page_count={}", pages + 2), [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    let store: Arc<Mutex<dyn MemoryStorage>> = Arc::new(Mutex::new(file));
    let limits = Limits {
        max_delay: Duration::from_millis(300),
        ..Limits::default()
    };
    let (writer, access) = queued_access(store.clone(), limits);
    let big = "x".repeat(64 * 1024);
    let mut huge = access
        .submit(MemoryRequest::Write(put(HOST_OWNER, "big", &big, None)))
        .unwrap();
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = runs.clone();
    let stored = access.run_blocking(0, move |s| {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        s.put(
            HOST_OWNER,
            &Scope::Global,
            "small",
            &text("1"),
            &PutOpts::default(),
        )
    });
    assert!(matches!(
        settle(huge.wait_timeout(WAIT).unwrap()),
        Err(Failure::Storage(_))
    ));
    assert_eq!(stored.unwrap().unwrap(), 1);
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 2);
    drop(writer);
}

/// 결과 불명은 다시 넣지 않는다. 확정됐을 수 있는 판단을 두 번 확정하지 않기 위해서다.
#[test]
fn an_unknown_outcome_is_returned_without_running_the_body_again() {
    let store = MemoryStore::open_in_memory().unwrap();
    crate::batch::plant_unreadable_row_for_tests(&store);
    let store: Arc<Mutex<dyn MemoryStorage>> = Arc::new(Mutex::new(store));
    let (writer, access) = queued_access(store.clone(), Limits::default());
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = runs.clone();
    let outcome = access.run_blocking(0, move |s| {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        s.put(
            HOST_OWNER,
            &Scope::Global,
            "a",
            &text("1"),
            &PutOpts::default(),
        )
        .unwrap();
        s.get(&Scope::Global, "unreadable").is_err()
    });
    assert!(
        matches!(outcome, Err(Blocked::Unknown(Some(_)))),
        "{outcome:?}"
    );
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
    drop(writer);
}

/// 접수를 멈춘 writer는 본문을 실행하지 않고 거절을 돌려준다.
#[test]
fn a_closed_writer_refuses_a_blocking_body_without_running_it() {
    let store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let (mut writer, access) = queued_access(store, Limits::default());
    writer.begin_shutdown();
    let deadline = Instant::now() + WAIT;
    // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
    assert!(
        holds_by_every(deadline, Duration::from_millis(1), |_| writer
            .poll_shutdown()
            == 0),
        "writer did not join"
    );
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = ran.clone();
    let outcome = access.run_blocking(0, move |_| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst)
    });
    assert!(
        matches!(outcome, Err(Blocked::Refused(SubmitError::Closed))),
        "{outcome:?}"
    );
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
}

/// 결과를 거두는 스레드로 표시한 스레드는 writer 결과를 기다리지 못한다.
#[test]
fn a_result_reaping_thread_may_not_wait_for_the_writer() {
    let store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    let (writer, access) = queued_access(store.clone(), Limits::default());
    let inline = MemoryAccess::inline(store);
    let blocking = access.clone();
    let waited = std::thread::spawn(move || {
        mark_result_reaping_thread();
        blocking.run_blocking(0, |_| ())
    })
    .join();
    let unit = std::thread::spawn(move || {
        mark_result_reaping_thread();
        access.run_and_wait(MemoryRequest::unit(0, |_| Ok(())), WAIT)
    })
    .join();
    if cfg!(debug_assertions) {
        assert!(waited.is_err() && unit.is_err());
    }
    // 바로 실행하는 구성은 다른 스레드를 기다리지 않아 검사하지 않는다.
    let inline_ok = std::thread::spawn(move || {
        mark_result_reaping_thread();
        inline.run_blocking(0, |_| 7)
    })
    .join();
    assert_eq!(inline_ok.unwrap().unwrap(), 7);
    assert!(!is_result_reaping_thread());
    drop(writer);
}

#[test]
fn memory_writer_alias_exposes_nonconsuming_shutdown_observation() {
    use tasty_utils::batch_writer::ShutdownObservation;
    let mut rig = rig_with(
        MemoryStore::open_in_memory().unwrap(),
        Limits::default(),
        true,
    );
    assert_eq!(
        rig.writer.shutdown_observation(),
        ShutdownObservation::NotStarted
    );
    rig.writer.begin_shutdown();
    let deadline = Instant::now() + WAIT;
    // poll 은 끝난 worker 를 join 하기만 한다(종료는 앞에서 시작했다). 마지막 읽기로 쓴다.
    assert!(holds_by_every(deadline, Duration::from_millis(1), |_| rig
        .writer
        .poll_shutdown()
        == 0));
    let observed = rig.writer.shutdown_observation();
    assert!(matches!(
        observed,
        ShutdownObservation::Joined {
            panicked: false,
            flush: Ok(_)
        }
    ));
    assert_eq!(rig.writer.shutdown_observation(), observed);
}
