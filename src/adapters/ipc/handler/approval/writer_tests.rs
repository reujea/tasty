//! writer 스레드를 거치는 승인 기록 제출 경계. IPC는 기록이 끝난 뒤 응답하고, GUI 응답은 기록을
//! 기다리지 않는다. 두 경로 모두 App 기능이 전이마다 기록을 한 번 제출한다.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tasty_approval::{ApprovalId, ApprovalState, Responder};
use tasty_memory::{MemoryConfig, MemoryStore, Scope};
use tasty_utils::test_wait::holds_by_every;

use crate::adapters::ipc::handler::deferred::tests::{make_request, run_request, writer_core_with};
use crate::app::approval_records::{RecordReceipt, record_key};

fn store(config: MemoryConfig) -> Arc<Mutex<MemoryStore>> {
    Arc::new(Mutex::new(
        MemoryStore::open_in_memory_with_config(config).expect("store"),
    ))
}

fn version(memory: &Arc<Mutex<MemoryStore>>, scope: &Scope, id: &ApprovalId) -> Option<u64> {
    let key = record_key(id).expect("generated id");
    memory
        .lock()
        .unwrap()
        .get(scope, &key)
        .unwrap()
        .map(|e| e.version)
}

/// approval.request·respond·cancel은 writer가 기록을 commit한 뒤 응답하고, 응답 시점에는 그
/// 전이의 기록이 이미 저장소에 있다. 응답 형식은 바뀌지 않는다.
#[test]
fn ipc_transitions_answer_after_their_record_commits() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let memory = store(MemoryConfig::default());
    let mut core = writer_core_with(memory.clone(), usize::MAX);
    let scope = Scope::Workspace(9);
    let request = |n| {
        make_request(
            n,
            "approval.request",
            json!({"title": "t", "workspace_id": 9}),
            None,
        )
    };

    let (requested, later) = run_request(&mut core, &request(1));
    assert!(later, "the reply waits for the writer");
    let id = ApprovalId(
        requested.result.expect("request")["id"]
            .as_str()
            .unwrap()
            .into(),
    );
    assert_eq!(version(&memory, &scope, &id), Some(1));
    let (responded, later) = run_request(
        &mut core,
        &make_request(
            2,
            "approval.respond",
            json!({"id": id.as_str(), "choice": "approve"}),
            None,
        ),
    );
    assert!(later);
    assert_eq!(
        responded.result.expect("respond")["state"]["state"],
        "responded"
    );
    assert_eq!(version(&memory, &scope, &id), Some(2));

    let (requested, _) = run_request(&mut core, &request(3));
    let id = ApprovalId(
        requested.result.expect("request")["id"]
            .as_str()
            .unwrap()
            .into(),
    );
    let (cancelled, later) = run_request(
        &mut core,
        &make_request(4, "approval.cancel", json!({"id": id.as_str()}), None),
    );
    assert!(later);
    assert_eq!(
        cancelled.result.expect("cancel")["state"]["state"],
        "cancelled"
    );
    assert_eq!(version(&memory, &scope, &id), Some(2));
}

/// 저장소가 기록을 거절해도 IPC 응답은 성공 그대로이고 메모리 상태 전이는 남는다.
#[test]
fn a_refused_record_keeps_the_ipc_reply_and_the_memory_state() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let memory = store(MemoryConfig {
        entry_max_bytes: 8,
        ..MemoryConfig::default()
    });
    let mut core = writer_core_with(memory.clone(), usize::MAX);
    let (requested, later) = run_request(
        &mut core,
        &make_request(
            1,
            "approval.request",
            json!({"title": "t", "workspace_id": 9}),
            None,
        ),
    );
    assert!(later);
    let id = ApprovalId(
        requested.result.expect("request")["id"]
            .as_str()
            .unwrap()
            .into(),
    );
    let (responded, _) = run_request(
        &mut core,
        &make_request(
            2,
            "approval.respond",
            json!({"id": id.as_str(), "choice": "deny"}),
            None,
        ),
    );
    assert_eq!(
        responded.result.expect("respond")["state"]["state"],
        "responded"
    );
    assert!(matches!(
        core.approval_store.get(&id).unwrap().state,
        ApprovalState::Responded { .. }
    ));
    assert_eq!(version(&memory, &Scope::Workspace(9), &id), None);
}

/// GUI 응답 경로는 App이 제출한 기록을 메인 루프 수거에 넘기고 바로 돌아온다. 기록은 popup이나
/// View가 아니라 App의 memory 핸들이 거두며, 수거 뒤 저장소에 남는다.
#[test]
fn a_gui_response_hands_its_record_to_the_main_loop() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let memory = store(MemoryConfig::default());
    let mut core = writer_core_with(memory.clone(), usize::MAX);
    let request = tasty_approval::ApprovalRequest {
        id: ApprovalId::generate(),
        requester: tasty_approval::Requester::Plugin { id: "p".into() },
        workspace_id: None,
        surface_id: None,
        title: "t".into(),
        body: None,
        choices: Vec::new(),
        default_choice: None,
        timeout_ms: None,
        severity: tasty_approval::Severity::Info,
        created_at: 0,
        metadata: serde_json::Value::Null,
        host_elevation: None,
    };
    let requested = core.request_approval(request).expect("request");
    let id = requested.change.record.request.id.clone();
    requested.record.detach(core.memory_access());

    let responded = core
        .respond_approval(&id, "approve".into(), Responder::User, None)
        .expect("respond");
    assert!(matches!(responded.record, RecordReceipt::Accepted(_)));
    responded.record.detach(core.memory_access());
    assert!(core.memory_access().detached_len() >= 1);
    assert!(matches!(
        core.approval_store.get(&id).unwrap().state,
        ApprovalState::Responded { .. }
    ));

    // reap_detached 는 끝난 티켓만 꺼낸다(이 시험의 두 기록은 완료 콜백이 없다). 새 작업을 만들지 않아
    // 기한 뒤 관측 읽기에서도 먼저 거둔다. 거두지 않으면 끝난 기록이 목록에 남아 보이지 않는다.
    let deadline = Instant::now() + Duration::from_secs(30);
    assert!(
        holds_by_every(deadline, Duration::from_millis(1), |last| {
            let access = core.memory_access();
            if last {
                access.reap_detached();
            }
            if access.detached_len() == 0 {
                return true;
            }
            if !last {
                access.reap_detached();
            }
            false
        }),
        "the detached record never landed"
    );
    assert_eq!(version(&memory, &Scope::Global, &id), Some(2));
}
