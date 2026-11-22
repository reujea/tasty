//! 실제 SQLite 저장소로 승인 기록의 쓰기·조회·재시작 보존을 확인한다. 키 검증이 없는
//! 시험 대역으로는 저장소가 거절하는 키를 잡지 못한다.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tasty_approval::{
    ApprovalChoice, ApprovalId, ApprovalRequest, ApprovalState, ApprovalStore, Requester,
    Responder, Severity, StateChange, WaitOutcome,
};
use tasty_memory::writer::MemoryAccess;
use tasty_memory::{MemoryConfig, MemoryStore, MemoryValue, PutOpts, Scope};

use super::*;

fn request(workspace_id: Option<u32>, timeout_ms: Option<u64>) -> ApprovalRequest {
    ApprovalRequest {
        id: ApprovalId::generate(),
        requester: Requester::Plugin { id: "p".into() },
        workspace_id,
        surface_id: None,
        title: "t".into(),
        body: None,
        choices: vec![ApprovalChoice::approve(), ApprovalChoice::deny()],
        default_choice: None,
        timeout_ms,
        severity: Severity::Info,
        created_at: 0,
        metadata: serde_json::Value::Null,
        host_elevation: None,
    }
}

fn open(path: &Path, config: MemoryConfig) -> (Arc<Mutex<MemoryStore>>, MemoryAccess) {
    let store = Arc::new(Mutex::new(
        MemoryStore::open_with_config(path, config).expect("open the memory db"),
    ));
    let access = MemoryAccess::inline(store.clone());
    (store, access)
}

/// IPC 경로처럼 App이 제출한 기록의 결과를 받아 저장소가 거절하면 실패한다.
fn write(access: &MemoryAccess, change: StateChange) {
    let transition = submit(access, change);
    let id = transition.change.record.request.id;
    let RecordReceipt::Accepted(mut ticket) = transition.record else {
        panic!("record {id} was not submitted: {:?}", transition.record);
    };
    let completion = ticket
        .wait_timeout(Duration::from_secs(5))
        .expect("the write finishes");
    if let Err(why) = tasty_memory::writer::settle(completion) {
        panic!("record {id} was not stored: {why:?}");
    }
}

/// 대문자가 든 생성 ID의 응답·취소·만료 기록이 각 scope에 저장되고, DB를 다시 연 뒤에도
/// 원래 ID와 마지막 상태로 history에 나온다. 같은 scope의 세션 요약은 기록에 섞이지 않는다.
#[test]
fn generated_ids_are_stored_and_read_back_after_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("memory.db");
    let mut expected: Vec<(ApprovalId, Option<u32>, &str)> = Vec::new();
    {
        let (memory, access) = open(&path, MemoryConfig::default());
        let approvals = ApprovalStore::new();
        for workspace in [Some(7), None] {
            if let Some(wid) = workspace {
                memory
                    .lock()
                    .unwrap()
                    .put(
                        tasty_memory::HOST_OWNER,
                        &Scope::Workspace(wid),
                        SUMMARY_KEY,
                        &MemoryValue::Text("summary".into()),
                        &PutOpts {
                            expires_at: None,
                            cas: None,
                        },
                    )
                    .expect("summary put");
            }

            let created = approvals.request(request(workspace, None)).unwrap();
            let id = created.record.request.id.clone();
            assert!(
                id.as_str().bytes().any(|b| b.is_ascii_uppercase()),
                "the generated id carries uppercase letters: {id}"
            );
            write(&access, created);
            let change = approvals
                .respond(&id, "approve".into(), Responder::User, None)
                .unwrap();
            write(&access, change);
            expected.push((id, workspace, "responded"));

            let created = approvals.request(request(workspace, None)).unwrap();
            let id = created.record.request.id.clone();
            write(&access, created);
            write(&access, approvals.cancel(&id).unwrap());
            expected.push((id, workspace, "cancelled"));

            let created = approvals.request(request(workspace, Some(1))).unwrap();
            let id = created.record.request.id.clone();
            write(&access, created);
            assert!(matches!(
                await_approval(&approvals, &access, &id, Some(1)),
                Ok(WaitOutcome::TimedOut { .. })
            ));
            expected.push((id, workspace, "timed_out"));
        }
    }

    let (memory, _access) = open(&path, MemoryConfig::default());
    let mut guard = memory.lock().unwrap();
    let records: HashMap<ApprovalId, ApprovalRecord> =
        stored_records(&mut *guard, None, None, None)
            .expect("history reads every scope")
            .into_iter()
            .map(|r| (r.request.id.clone(), r))
            .collect();
    assert_eq!(records.len(), expected.len(), "{records:?}");
    for (id, workspace, state) in &expected {
        let record = records
            .get(id)
            .unwrap_or_else(|| panic!("{id} is in history"));
        assert_eq!(record.request.workspace_id, *workspace);
        let actual = match record.state {
            ApprovalState::Pending => "pending",
            ApprovalState::Responded { .. } => "responded",
            ApprovalState::Cancelled { .. } => "cancelled",
            ApprovalState::TimedOut { .. } => "timed_out",
        };
        assert_eq!(actual, *state, "{id}");
        let scope = match workspace {
            Some(wid) => Scope::Workspace(*wid),
            None => Scope::Global,
        };
        let key = record_key(id).expect("a generated id has a record key");
        let entry = guard.get(&scope, &key).unwrap();
        // 생성과 마지막 전이 한 번씩만 썼다. 대기 워커는 다른 쪽이 만든 전이를 다시 쓰지 않는다.
        assert_eq!(entry.as_ref().map(|e| e.version), Some(2), "{key}");
        assert!(entry.is_some(), "{key} in {scope:?}");
    }
    let workspace_only = stored_records(&mut *guard, Some(7), None, None).unwrap();
    assert_eq!(workspace_only.len(), 3);
    let summary = guard.get(&Scope::Workspace(7), SUMMARY_KEY).unwrap();
    assert!(matches!(summary.map(|e| e.value), Some(MemoryValue::Text(t)) if t == "summary"));
}

/// 저장소가 기록을 거절해도 이미 끝난 메모리 상태 전이는 그대로 남는다.
#[test]
fn a_refused_write_keeps_the_memory_transition() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (memory, access) = open(
        &dir.path().join("memory.db"),
        MemoryConfig {
            entry_max_bytes: 8,
            ..MemoryConfig::default()
        },
    );
    let approvals = ApprovalStore::new();
    let created = approvals.request(request(Some(7), None)).unwrap();
    let id = created.record.request.id.clone();
    let change = approvals
        .respond(&id, "approve".into(), Responder::User, None)
        .unwrap();
    submit(&access, change).record.detach(&access);
    let expiring = approvals.request(request(None, Some(1))).unwrap();
    let expiring = expiring.record.request.id;
    assert!(matches!(
        await_approval(&approvals, &access, &expiring, Some(1)),
        Ok(WaitOutcome::TimedOut { .. })
    ));

    assert!(matches!(
        approvals.get(&id).unwrap().state,
        ApprovalState::Responded { .. }
    ));
    assert!(matches!(
        approvals.get(&expiring).unwrap().state,
        ApprovalState::TimedOut { .. }
    ));
    let mut guard = memory.lock().unwrap();
    assert!(
        stored_records(&mut *guard, None, None, None)
            .unwrap()
            .is_empty()
    );
}

fn version_of(memory: &Arc<Mutex<MemoryStore>>, scope: &Scope, id: &ApprovalId) -> Option<u64> {
    let key = record_key(id).expect("a generated id has a record key");
    memory
        .lock()
        .unwrap()
        .get(scope, &key)
        .unwrap()
        .map(|e| e.version)
}

/// 여러 대기 워커가 같은 요청의 만료를 기다려도 만료 기록은 한 번만 제출된다.
#[test]
fn concurrent_waiters_record_one_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (memory, access) = open(&dir.path().join("memory.db"), MemoryConfig::default());
    let approvals = Arc::new(ApprovalStore::new());
    let created = approvals.request(request(Some(3), None)).unwrap();
    let id = created.record.request.id.clone();
    write(&access, created);
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let (approvals, access, id) = (approvals.clone(), access.clone(), id.clone());
            std::thread::spawn(move || await_approval(&approvals, &access, &id, Some(30)))
        })
        .collect();
    for waiter in waiters {
        assert!(matches!(
            waiter.join().unwrap(),
            Ok(WaitOutcome::TimedOut { .. })
        ));
    }
    assert_eq!(version_of(&memory, &Scope::Workspace(3), &id), Some(2));
    let stored = stored_records(&mut *memory.lock().unwrap(), Some(3), None, None).unwrap();
    assert!(matches!(stored[0].state, ApprovalState::TimedOut { .. }));
}

/// 대기 워커를 띄운다. 워커가 디렉터리를 함께 가져, 시험이 워커를 회수하지 못하고 실패해도
/// 늦은 만료 쓰기 동안 디렉터리가 남는다. 디렉터리를 먼저 받아 저장소(SQLite 연결)가
/// 디렉터리보다 먼저 해제된다.
fn spawn_waiter(
    dir: &Arc<tempfile::TempDir>,
    approvals: &Arc<ApprovalStore>,
    access: &MemoryAccess,
    id: &ApprovalId,
    wait_ms: u64,
) -> std::thread::JoinHandle<Result<WaitOutcome, ApprovalError>> {
    let (dir, approvals, access, id) = (dir.clone(), approvals.clone(), access.clone(), id.clone());
    std::thread::spawn(move || {
        let _dir = dir;
        let access = access;
        await_approval(&approvals, &access, &id, Some(wait_ms))
    })
}

/// 저장소에서 다시 읽은 승인 기록. history 조회 경로(`stored_records`)로 역직렬화한다.
fn stored_record(memory: &Arc<Mutex<MemoryStore>>, id: &ApprovalId) -> ApprovalRecord {
    stored_records(&mut *memory.lock().unwrap(), None, None, None)
        .unwrap()
        .into_iter()
        .find(|r| r.request.id == *id)
        .expect("stored")
}

/// 저장된 상태 전체가 메모리 상태와 같다.
fn assert_stored_matches_live(
    memory: &Arc<Mutex<MemoryStore>>,
    approvals: &ApprovalStore,
    id: &ApprovalId,
) -> ApprovalState {
    let stored = stored_record(memory, id);
    assert_eq!(stored.state, approvals.get(id).unwrap().state);
    stored.state
}

/// Given 기록한 요청과 시험이 쥔 memory 잠금, When 대기 워커가 만료 전이를 적용하고 늦은
/// 응답이 오면, Then 그 응답은 만료 기록이 저장되기 전에 거절된다. 잠금을 푼 뒤 대기 결과는
/// 만료이고 저장 기록은 만료 하나(version 2)다. 워커는 승인 잠금을 놓은 뒤에야 memory 잠금을
/// 잡으므로, 잠금을 쥐는 동안 상태가 만료로 보이면 만료 전이와 기록 사이에 응답한 것이 된다.
#[test]
fn a_timeout_applied_first_rejects_a_late_response_before_the_write() {
    let dir = Arc::new(tempfile::tempdir().expect("tempdir"));
    let (memory, access) = open(&dir.path().join("memory.db"), MemoryConfig::default());
    let approvals = Arc::new(ApprovalStore::new());
    let created = approvals.request(request(None, None)).unwrap();
    let id = created.record.request.id.clone();
    write(&access, created);
    let key = record_key(&id).expect("a generated id has a record key");
    let held = memory.lock().unwrap();
    let waiter = spawn_waiter(&dir, &approvals, &access, &id, 1);
    // 잠금을 쥔 동안에는 단정하지 않는다. 그 자리의 panic은 memory 잠금을 오염시키고 대기
    // 워커를 회수하지 못한 채 임시 디렉터리를 지운다.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !matches!(
        approvals.get(&id).unwrap().state,
        ApprovalState::TimedOut { .. }
    ) {
        if std::time::Instant::now() >= deadline {
            drop(held);
            panic!("the waiter did not apply the timeout within 10s");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let late = approvals.respond(&id, "approve".into(), Responder::User, None);
    let version_at_late = held.get(&Scope::Global, &key).map(|e| e.map(|e| e.version));
    drop(held);
    let outcome = waiter.join().unwrap().unwrap();
    assert!(
        matches!(late, Err(ApprovalError::AlreadyResponded(_))),
        "{late:?}"
    );
    assert_eq!(version_at_late.unwrap(), Some(1), "no timeout record yet");
    assert!(
        matches!(outcome, WaitOutcome::TimedOut { .. }),
        "{outcome:?}"
    );
    assert_eq!(version_of(&memory, &Scope::Global, &id), Some(2));
    let state = assert_stored_matches_live(&memory, &approvals, &id);
    assert!(matches!(state, ApprovalState::TimedOut { .. }));
}

/// Given 기록한 요청, When 응답이 먼저 적용된 뒤 대기를 부르면, Then 대기 결과는 응답이고
/// 대기 워커는 기록을 쓰지 않는다(반환 직후 version 1). 응답 쪽이 기록하면 version 2다.
#[test]
fn a_response_applied_first_leaves_the_write_to_the_responder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (memory, access) = open(&dir.path().join("memory.db"), MemoryConfig::default());
    let approvals = ApprovalStore::new();
    let created = approvals.request(request(None, None)).unwrap();
    let id = created.record.request.id.clone();
    write(&access, created);
    let change = approvals
        .respond(&id, "approve".into(), Responder::User, None)
        .expect("the request is pending");
    let outcome = await_approval(&approvals, &access, &id, Some(5_000)).unwrap();
    assert!(
        matches!(outcome, WaitOutcome::Responded { .. }),
        "{outcome:?}"
    );
    assert_eq!(version_of(&memory, &Scope::Global, &id), Some(1));
    write(&access, change);
    assert_eq!(version_of(&memory, &Scope::Global, &id), Some(2));
    let state = assert_stored_matches_live(&memory, &approvals, &id);
    assert!(matches!(state, ApprovalState::Responded { .. }));
}

/// Given 기록한 요청과 유한 마감의 대기 워커, When 응답과 만료가 순서를 정하지 않고 겹치면,
/// Then 어느 쪽이 이기든 기록은 승자 하나다. 응답이 받아들여지면 대기 결과는 응답이고, 거절되면
/// 만료다. 저장 기록은 version 2이고 그 상태가 메모리 상태·승자와 같다.
#[test]
fn a_response_and_a_timeout_race_to_one_record() {
    let dir = Arc::new(tempfile::tempdir().expect("tempdir"));
    let (memory, access) = open(&dir.path().join("memory.db"), MemoryConfig::default());
    let approvals = Arc::new(ApprovalStore::new());
    for wait_ms in [5_000, 1] {
        let created = approvals.request(request(None, None)).unwrap();
        let id = created.record.request.id.clone();
        write(&access, created);
        let waiter = spawn_waiter(&dir, &approvals, &access, &id, wait_ms);
        std::thread::sleep(Duration::from_millis(20));
        // 응답이 받아들여지면 대기 회수 전에 기록한다. 오류는 회수한 뒤에 판정한다.
        let response = approvals
            .respond(&id, "approve".into(), Responder::User, None)
            .map(|change| write(&access, change));
        let outcome = waiter.join().unwrap().unwrap();
        let response_won = match response {
            Ok(()) => true,
            Err(ApprovalError::AlreadyResponded(_)) => false,
            Err(other) => panic!("unexpected response error: {other:?}"),
        };
        match (response_won, &outcome) {
            (true, WaitOutcome::Responded { .. }) | (false, WaitOutcome::TimedOut { .. }) => {}
            _ => panic!("response accepted {response_won} but the waiter saw {outcome:?}"),
        }
        assert_eq!(version_of(&memory, &Scope::Global, &id), Some(2));
        let state = assert_stored_matches_live(&memory, &approvals, &id);
        assert_eq!(
            matches!(state, ApprovalState::Responded { .. }),
            response_won,
            "{state:?}"
        );
        assert_eq!(
            matches!(state, ApprovalState::TimedOut { .. }),
            !response_won,
            "{state:?}"
        );
    }
}
