//! 실제 프로세스 강제 종료 뒤의 journal 상태.
//!
//! 각 시험은 자기 시험 바이너리를 `--exact <시험 이름>`과 환경변수로 다시 실행한다. 자식은 지정
//! 지점까지 진행한 뒤 도달 표시 파일을 남기고 `std::process::abort()`로 끝난다. 부모는 자식이
//! 그 지점에 도달했는지와 비정상 종료했는지를 확인한 뒤 journal을 다시 열어 상태를 판정한다.
//! drop 뒤 재오픈과 달리 소멸자·잠금 해제·연결 종료 없이 프로세스가 사라지는 경우를 재현한다.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use tasty_event_store::{
    ActivationClaim, CommandKey, CommandLookup, CommandStatus, CommitOutcome, CommitRequest,
    EffectState, EffectTransition, EventStore, ExpectedRevision, NewCommand, NewEffect, NewEvent,
    OpaquePayload, SCHEMA_VERSION, StoreError, StreamAppend, StreamId, WriterEpoch,
};

const JOURNAL: &str = "journal-crash";
/// 자식으로 실행할 시험 이름. 없으면 부모로 동작한다.
const ENV_CHILD: &str = "TASTY_EVENT_STORE_CRASH_CHILD";
const ENV_DIR: &str = "TASTY_EVENT_STORE_CRASH_DIR";
/// 한 시험이 자식을 여럿 띄울 때 구분하는 이름.
const ENV_ROLE: &str = "TASTY_EVENT_STORE_CRASH_ROLE";
const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

// ---- 자식 실행과 판정 도우미 ----

/// 이 프로세스가 `test`의 자식이면 작업 디렉터리를 돌려준다.
fn child_dir(test: &str) -> Option<PathBuf> {
    (std::env::var(ENV_CHILD).ok()? == test)
        .then(|| PathBuf::from(std::env::var_os(ENV_DIR).expect("child needs its directory")))
}

fn role() -> String {
    std::env::var(ENV_ROLE).unwrap_or_default()
}

fn db_path(dir: &Path) -> PathBuf {
    dir.join("journal.db")
}

fn marker(dir: &Path, role: &str) -> PathBuf {
    dir.join(format!("reached-{role}"))
}

/// 도달 표시를 남기고 소멸자 없이 프로세스를 끝낸다.
fn reach_and_abort(dir: &Path, note: &str) -> ! {
    std::fs::write(marker(dir, &role()), note).expect("write marker");
    std::process::abort();
}

/// 자식의 stdout·stderr 를 모으는 파일. 부모 시험의 출력에 자식의 줄이 섞이지 않게 한다.
fn child_log(dir: &Path, role: &str) -> PathBuf {
    dir.join(format!("child{role}.out"))
}

fn spawn_child(test: &str, dir: &Path, role: &str) -> Child {
    let out = std::fs::File::create(child_log(dir, role)).expect("child log");
    Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", test, "--test-threads=1", "--nocapture"])
        .env(ENV_CHILD, test)
        .env(ENV_DIR, dir)
        .env(ENV_ROLE, role)
        .stdout(out.try_clone().expect("child log handle"))
        .stderr(out)
        .spawn()
        .expect("spawn child")
}

fn wait_child(mut child: Child) -> ExitStatus {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    // 거짓을 읽은 뒤 기한이 지났으면 한 번 더 거두고 판정한다. try_wait 는 끝난 자식을 거두기만 한다.
    loop {
        if let Some(status) = child.try_wait().expect("wait child") {
            return status;
        }
        if Instant::now() >= deadline {
            if let Some(status) = child.try_wait().expect("wait child") {
                return status;
            }
            child.kill().expect("kill stuck child");
            panic!("child did not finish within {CHILD_TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 자식이 표시 지점에 도달한 뒤 abort로 끝났는지 확인하고 표시 내용을 돌려준다.
/// 표시가 없으면 자식이 그 전에 실패한 것이다.
fn assert_aborted_at_marker(status: ExitStatus, dir: &Path, role: &str) -> String {
    let log = || std::fs::read_to_string(child_log(dir, role)).unwrap_or_default();
    let note = std::fs::read_to_string(marker(dir, role)).unwrap_or_else(|_| {
        panic!(
            "child {role:?} did not reach its point: {status:?}\nchild output:\n{}",
            log()
        )
    });
    assert!(
        !status.success(),
        "child {role:?} was expected to abort\nchild output:\n{}",
        log()
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            status.signal(),
            Some(6),
            "child {role:?} ended by SIGABRT\nchild output:\n{}",
            log()
        );
    }
    note
}

fn run_child_to_abort(test: &str, dir: &Path) {
    let status = wait_child(spawn_child(test, dir, ""));
    assert_aborted_at_marker(status, dir, "");
}

/// 강제 종료 뒤 새 프로세스처럼 다시 연다. OS가 자식의 잠금을 풀었어야 writer가 된다.
fn reopen_as_writer(dir: &Path) -> (EventStore, WriterEpoch) {
    let mut store = EventStore::open(&db_path(dir), JOURNAL).expect("reopen after crash");
    let epoch = store.acquire_writer().expect("lock is released by the OS");
    (store, epoch)
}

fn integrity_ok(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("raw connection");
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity check");
    assert_eq!(result, "ok");
}

// ---- 기록 도우미 ----

fn payload(tag: &str, bytes: &[u8]) -> OpaquePayload {
    OpaquePayload {
        type_tag: tag.to_owned(),
        schema_version: 1,
        bytes: bytes.to_vec(),
    }
}

fn event(id: &str) -> NewEvent {
    NewEvent {
        event_id: id.to_owned(),
        payload: payload("CrashEvent", id.as_bytes()),
        recorded_at_ms: 0,
        causation_id: None,
        actor: "agent".to_owned(),
        origin: "crash-test".to_owned(),
        payload_refs: Vec::new(),
    }
}

fn append(stream: &str, ids: &[&str]) -> StreamAppend {
    StreamAppend {
        stream_id: StreamId::new(stream),
        expected: ExpectedRevision::Any,
        events: ids.iter().map(|id| event(id)).collect(),
    }
}

fn key(k: &str) -> CommandKey {
    CommandKey {
        caller_scope: "agent-1".to_owned(),
        idempotency_key: k.to_owned(),
    }
}

/// 명령·두 stream의 이벤트·effect를 함께 담은 요청.
fn full_request(epoch: WriterEpoch, n: u32) -> CommitRequest {
    let mut request = CommitRequest::new(epoch);
    request.command = Some(NewCommand {
        command_id: format!("cmd-{n}"),
        key: Some(key(&format!("k-{n}"))),
        request_digest: format!("digest-{n}").into_bytes(),
        resolved: b"target=pane:1".to_vec(),
        status: CommandStatus::Completed,
        response: Some(b"ok".to_vec()),
    });
    request.appends.push(append(
        "engine-a",
        &[&format!("a-{n}-1"), &format!("a-{n}-2")],
    ));
    request
        .appends
        .push(append("engine-b", &[&format!("b-{n}-1")]));
    request.effects.push(NewEffect {
        effect_id: format!("fx-{n}"),
        operation_id: format!("op-{n}"),
        resource_generation: 1,
        claim_kind: tasty_event_store::ClaimKind::Activation,
        payload: payload("SurfaceCreate", b"{}"),
        initial: EffectState::Pending,
    });
    request
}

fn commit(store: &mut EventStore, request: &CommitRequest) {
    match store.commit(request).expect("commit") {
        CommitOutcome::Committed { batch: Some(_) } => {}
        other => panic!("unexpected outcome {other:?}"),
    }
}

fn open_writer(path: &Path) -> (EventStore, WriterEpoch) {
    let mut store = EventStore::open(path, JOURNAL).expect("open journal");
    let epoch = store.acquire_writer().expect("acquire writer");
    (store, epoch)
}

fn stream(id: &str) -> StreamId {
    StreamId::new(id)
}

// ---- a. commit 직후 강제 종료 ----

#[test]
fn abort_right_after_commit_keeps_the_whole_batch() {
    const TEST: &str = "abort_right_after_commit_keeps_the_whole_batch";
    if let Some(dir) = child_dir(TEST) {
        let (mut store, epoch) = open_writer(&db_path(&dir));
        commit(&mut store, &full_request(epoch, 1));
        reach_and_abort(&dir, "committed");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    run_child_to_abort(TEST, dir.path());
    integrity_ok(&db_path(dir.path()));

    let (mut store, epoch) = reopen_as_writer(dir.path());
    let batches = store.read_batches_after(None, 10).expect("batches");
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].events.len(), 3);
    assert_eq!(batches[0].command_id.as_deref(), Some("cmd-1"));
    assert_eq!(
        store.stream_revision(&stream("engine-a")).expect("a"),
        Some(2)
    );
    assert_eq!(
        store.stream_revision(&stream("engine-b")).expect("b"),
        Some(1)
    );

    let CommandLookup::Hit(record) = store
        .lookup_command(&key("k-1"), b"digest-1")
        .expect("lookup")
    else {
        panic!("command identity must survive the crash");
    };
    assert_eq!(record.batch_ids, vec![batches[0].cut.batch_id]);
    assert_eq!(record.response.as_deref(), Some(&b"ok"[..]));

    let effect = store.effect("fx-1").expect("effect").expect("fx-1");
    assert_eq!(effect.state, EffectState::Pending);
    assert_eq!(effect.cause_batch_id, Some(batches[0].cut.batch_id));
    assert_eq!(effect.command_id.as_deref(), Some("cmd-1"));

    // 새 writer가 같은 요청을 다시 보내도 기존 기록을 돌려받고 batch를 더 만들지 않는다.
    let outcome = store
        .commit(&full_request(epoch, 1))
        .expect("resubmit after the crash");
    assert!(
        matches!(outcome, CommitOutcome::Duplicate(ref r) if r.command_id == "cmd-1"),
        "{outcome:?}"
    );
    assert_eq!(
        store.read_batches_after(None, 10).expect("batches").len(),
        1
    );
    assert_eq!(
        store.stream_revision(&stream("engine-a")).expect("a"),
        Some(2)
    );
}

// ---- b. commit 전 강제 종료 ----

#[test]
fn abort_before_commit_leaves_nothing_of_the_prepared_batch() {
    const TEST: &str = "abort_before_commit_leaves_nothing_of_the_prepared_batch";
    if let Some(dir) = child_dir(TEST) {
        let (mut store, epoch) = open_writer(&db_path(&dir));
        commit(&mut store, &full_request(epoch, 1));
        let prepared = full_request(epoch, 2);
        assert_eq!(prepared.appends.len(), 2);
        reach_and_abort(&dir, "prepared");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    run_child_to_abort(TEST, dir.path());
    integrity_ok(&db_path(dir.path()));

    let (store, _) = reopen_as_writer(dir.path());
    let batches = store.read_batches_after(None, 10).expect("batches");
    assert_eq!(batches.len(), 1, "only the committed batch remains");
    assert_eq!(batches[0].command_id.as_deref(), Some("cmd-1"));
    assert_eq!(
        store.stream_revision(&stream("engine-a")).expect("a"),
        Some(2)
    );
    assert_eq!(
        store
            .lookup_command(&key("k-2"), b"digest-2")
            .expect("lookup"),
        CommandLookup::Miss
    );
    assert_eq!(store.command("cmd-2").expect("cmd-2"), None);
    assert_eq!(store.effect("fx-2").expect("fx-2"), None);
}

// ---- c. effect Running 기록 직후 강제 종료 ----

fn claim() -> ActivationClaim {
    ActivationClaim {
        engine_id: "engine-a".to_owned(),
        surface_id: Some("surface-1".to_owned()),
        runtime_epoch: 1,
        activation_generation: 1,
    }
}

fn transition(from: EffectState, to: EffectState) -> EffectTransition {
    EffectTransition {
        effect_id: "fx-1".to_owned(),
        from,
        to,
        resource_generation: 1,
        attempt: None,
        claim: None,
        result: None,
    }
}

#[test]
fn abort_after_running_leaves_a_running_attempt_for_reconciliation() {
    const TEST: &str = "abort_after_running_leaves_a_running_attempt_for_reconciliation";
    if let Some(dir) = child_dir(TEST) {
        let (mut store, epoch) = open_writer(&db_path(&dir));
        commit(&mut store, &full_request(epoch, 1));
        let running = EffectTransition {
            claim: Some(tasty_event_store::EffectClaim::Activation(claim())),
            ..transition(EffectState::Pending, EffectState::Running)
        };
        store.transition_effect(epoch, &running).expect("running");
        reach_and_abort(&dir, &epoch.0.to_string());
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let status = wait_child(spawn_child(TEST, dir.path(), ""));
    let child_epoch = WriterEpoch(
        assert_aborted_at_marker(status, dir.path(), "")
            .parse()
            .expect("child epoch"),
    );
    integrity_ok(&db_path(dir.path()));

    let (mut store, epoch) = reopen_as_writer(dir.path());
    assert!(epoch > child_epoch, "the new writer fences the crashed one");

    let running = store
        .effects_in_state(EffectState::Running)
        .expect("running");
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].effect_id, "fx-1");
    assert_eq!(running[0].attempt, 1);
    let attempts = store.effect_attempts("fx-1").expect("attempts");
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].writer_epoch, child_epoch);
    assert_eq!(
        attempts[0].claim,
        tasty_event_store::EffectClaim::Activation(claim())
    );
    assert_eq!(attempts[0].outcome, None);

    // 결과를 모르는 attempt를 Uncertain으로 닫고, 실행되지 않았다는 증거로 Cancelled로 닫는다.
    store
        .transition_effect(
            epoch,
            &EffectTransition {
                attempt: Some(1),
                ..transition(EffectState::Running, EffectState::Uncertain)
            },
        )
        .expect("mark uncertain");
    store
        .transition_effect(
            epoch,
            &EffectTransition {
                result: Some(b"no surface-1 process".to_vec()),
                ..transition(EffectState::Uncertain, EffectState::Cancelled)
            },
        )
        .expect("reconcile");
    let attempts = store.effect_attempts("fx-1").expect("attempts");
    assert_eq!(attempts[0].outcome, Some(EffectState::Uncertain));
    assert_eq!(attempts[0].reconciled_outcome, Some(EffectState::Cancelled));
    assert_eq!(
        store.effect("fx-1").expect("fx").expect("fx-1").state,
        EffectState::Cancelled
    );
}

// ---- d. 두 프로세스의 동시 첫 open ----

const GO_FILE: &str = "go";

fn wait_for_go(dir: &Path) {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    // 거짓을 읽은 뒤 기한이 지났으면 한 번 더 읽고 판정한다.
    while !dir.join(GO_FILE).exists() {
        if Instant::now() >= deadline {
            assert!(dir.join(GO_FILE).exists(), "parent never said go");
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn two_processes_opening_an_empty_file_at_once_leave_a_valid_journal() {
    const TEST: &str = "two_processes_opening_an_empty_file_at_once_leave_a_valid_journal";
    if let Some(dir) = child_dir(TEST) {
        wait_for_go(&dir);
        let mut store = EventStore::open(&db_path(&dir), JOURNAL).expect("first open");
        let note = match store.acquire_writer() {
            Ok(epoch) => {
                let mut request = CommitRequest::new(epoch);
                let id = format!("from-{}", role());
                request
                    .appends
                    .push(append(&format!("child-{}", role()), &[&id]));
                commit(&mut store, &request);
                "writer".to_owned()
            }
            Err(StoreError::WriterLocked) => "locked".to_owned(),
            Err(other) => panic!("unexpected error {other:?}"),
        };
        reach_and_abort(&dir, &note);
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let children: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|role| (role, spawn_child(TEST, dir.path(), role)))
        .collect();
    std::fs::write(dir.path().join(GO_FILE), b"").expect("go");
    let notes: Vec<_> = children
        .into_iter()
        .map(|(role, child)| {
            let status = wait_child(child);
            (role, assert_aborted_at_marker(status, dir.path(), role))
        })
        .collect();
    assert!(
        notes.iter().any(|(_, note)| note == "writer"),
        "at least one child becomes the writer: {notes:?}"
    );

    let path = db_path(dir.path());
    integrity_ok(&path);
    let conn = rusqlite::Connection::open(&path).expect("raw connection");
    let versions: Vec<u32> = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("versions")
        .collect::<Result<_, _>>()
        .expect("read versions");
    assert_eq!(versions, (1..=SCHEMA_VERSION).collect::<Vec<_>>());

    let (store, _) = reopen_as_writer(dir.path());
    for (role, note) in &notes {
        let expected = (note == "writer").then_some(1);
        assert_eq!(
            store
                .stream_revision(&stream(&format!("child-{role}")))
                .expect("revision"),
            expected,
            "child {role} reported {note}"
        );
    }
}

#[test]
fn abort_before_group_commit_leaves_none_of_the_staged_requests() {
    const TEST: &str = "abort_before_group_commit_leaves_none_of_the_staged_requests";
    if let Some(dir) = child_dir(TEST) {
        let (mut store, epoch) = open_writer(&db_path(&dir));
        let mut group = store.begin_commit_group(epoch).expect("group");
        for n in 1..=3 {
            group.append(&full_request(epoch, n)).expect("stage");
        }
        reach_and_abort(&dir, "staged before physical commit");
    }
    let dir = tempfile::tempdir().unwrap();
    run_child_to_abort(TEST, dir.path());
    integrity_ok(&db_path(dir.path()));
    let (store, _) = reopen_as_writer(dir.path());
    assert!(store.read_batches_after(None, 10).unwrap().is_empty());
    for n in 1..=3 {
        assert!(store.command(&format!("cmd-{n}")).unwrap().is_none());
        assert!(store.effect(&format!("fx-{n}")).unwrap().is_none());
    }
}

#[test]
fn abort_after_group_commit_keeps_each_original_batch_response_and_effect() {
    const TEST: &str = "abort_after_group_commit_keeps_each_original_batch_response_and_effect";
    if let Some(dir) = child_dir(TEST) {
        let (mut store, epoch) = open_writer(&db_path(&dir));
        let before = store.commit_statistics();
        let mut group = store.begin_commit_group(epoch).expect("group");
        for n in 1..=3 {
            group.append(&full_request(epoch, n)).expect("stage");
        }
        group.finish().expect("physical commit");
        assert_eq!(
            store.commit_statistics().physical_commits - before.physical_commits,
            1
        );
        reach_and_abort(&dir, "three logical batches in one physical commit");
    }
    let dir = tempfile::tempdir().unwrap();
    run_child_to_abort(TEST, dir.path());
    integrity_ok(&db_path(dir.path()));
    let (mut store, epoch) = reopen_as_writer(dir.path());
    let batches = store.read_batches_after(None, 10).unwrap();
    assert_eq!(batches.len(), 3);
    for n in 1..=3 {
        let saved = store.command(&format!("cmd-{n}")).unwrap().unwrap();
        assert_eq!(saved.response.as_deref(), Some(b"ok".as_slice()));
        assert_eq!(saved.batch_ids, [batches[n as usize - 1].cut.batch_id]);
        let effect = store.effect(&format!("fx-{n}")).unwrap().unwrap();
        assert_eq!(
            effect.cause_batch_id,
            Some(batches[n as usize - 1].cut.batch_id)
        );
        assert_eq!(effect.state, EffectState::Pending);
        assert_eq!(effect.attempt, 0);
        assert!(matches!(
            store.commit(&full_request(epoch, n)),
            Ok(CommitOutcome::Duplicate(_))
        ));
    }
    assert_eq!(store.read_batches_after(None, 10).unwrap().len(), 3);
}
