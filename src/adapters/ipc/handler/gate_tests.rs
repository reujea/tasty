use super::*;

fn split(target: Value) -> JsonRpcRequest {
    serde_json::from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "split",
        "params": { "target_surface": target },
    }))
    .expect("request")
}

#[test]
fn only_a_non_numeric_target_surface_needs_the_nickname_lookup() {
    assert_eq!(
        nickname_wanted(&split(json!("faraway"))).as_deref(),
        Some("faraway")
    );
    for target in [json!(7), json!("7"), json!(""), Value::Null] {
        assert_eq!(nickname_wanted(&split(target.clone())), None, "{target}");
    }
    let mut close = split(json!("faraway"));
    close.method = "surface.close".into();
    assert_eq!(nickname_wanted(&close), None);
}

#[test]
fn a_request_without_a_token_is_local_and_never_reaches_the_writer() {
    let core = super::super::cli_entry_tests::test_core();
    let mut request = split(json!(3));
    request.caller_agent_id = Some("alias_1".into());
    let caller = resolve_caller_waiting(&core, &request).expect("local");
    assert_eq!(caller.agent_id().as_str(), "alias_1");
    assert!(matches!(caller, CallerContext::Local { .. }));
}

// ─── 저장 실패 주입 ───
//
// 실제 SQLite 저장소를 감싼 `FaultyStore`로 writer 경로에서 저장 실패를 낸다. 세션 판정의 실패만 다시
// 넣고 결국 거절하며, 선택 조회(cap)와 기록(rate limit·ipc_calls)의 실패는 요청을 막지 않는다.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};

use tasty_memory::{HOST_OWNER, MemoryValue, PutOpts, Scope};
use tasty_plugin_manifest::Permission;
use tasty_utils::test_wait::holds_by_every;

use super::super::gate_faults::{Faults, FaultyStore, gate_faults};

const AGENT: &str = "child:gate";

fn rig() -> (AppServices, Arc<Faults>) {
    let faults = gate_faults();
    let store = Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    let core = super::super::deferred::tests::writer_core_with(store, usize::MAX);
    (core, faults)
}

fn now() -> u64 {
    telemetry::now_ms()
}

fn issue(core: &AppServices) -> String {
    core.memory_access()
        .run_blocking(0, |mem| {
            tasty_ipc::session::SessionStore::new(mem, HOST_OWNER)
                .issue(AGENT, None, [Permission::SurfaceRead], None, now())
                .map(|(token, _)| token.to_string())
        })
        .expect("writer")
        .expect("issue session")
}

fn with_token(method: &str, token: &str) -> JsonRpcRequest {
    let mut request = split(json!(3));
    request.method = method.into();
    request.params = json!({});
    request.session_token = Some(token.into());
    request
}

fn code(response: &JsonRpcResponse) -> i32 {
    response.error.as_ref().expect("error response").code
}

/// writer를 멈추는 강한 읽기. 혼자 실행되므로 뒤에 넣은 요청은 풀릴 때까지 실행되지 않는다.
struct Hold {
    release: mpsc::SyncSender<()>,
    ticket: MemoryTicket,
}

impl Hold {
    fn new(core: &AppServices) -> Self {
        let (release, held) = mpsc::sync_channel::<()>(1);
        let held = Mutex::new(held);
        let ticket = core
            .memory_access()
            .submit(MemoryRequest::read(0, move |_| {
                let held = held.lock().expect("hold");
                if let Err(mpsc::RecvTimeoutError::Timeout) =
                    held.recv_timeout(std::time::Duration::from_secs(30))
                {
                    panic!("the test hold was never released");
                }
                Ok(())
            }))
            .expect("hold accepted");
        Self { release, ticket }
    }

    fn release(self) {
        let Self {
            release,
            mut ticket,
        } = self;
        drop(release);
        let completion = ticket.wait_timeout(TEST_WAIT).expect("hold finished");
        settle(completion).expect("hold settled");
    }
}

fn put_cap(core: &AppServices, cap: tasty_telemetry::CostCap) {
    core.memory_access()
        .run_blocking(0, move |m| {
            m.put(
                HOST_OWNER,
                &Scope::Global,
                &tasty_telemetry::cap_key(&cap.id),
                &MemoryValue::Json(serde_json::to_value(&cap).expect("cap json")),
                &PutOpts::default(),
            )
        })
        .expect("writer")
        .expect("cap stored");
}

fn triggered_cap(agent: &str, action: tasty_telemetry::CapAction) -> tasty_telemetry::CostCap {
    tasty_telemetry::CostCap {
        id: format!("cap_{}", agent.replace(':', "_")),
        agent: agent.into(),
        metric: "ipc_calls".into(),
        threshold: 1.0,
        window: tasty_telemetry::CapWindow::Total,
        action,
        created_at: 0,
        triggered: Some(tasty_telemetry::CapTriggered { at: 0, value: 1.0 }),
    }
}

fn gate(core: &mut AppServices, request: &JsonRpcRequest, caller: &CallerContext) -> Option<i32> {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    check_request(core, &mut state, &mut engine, request, caller)
        .err()
        .map(|response| code(&response))
}

#[test]
fn a_session_storage_failure_is_submitted_again_until_it_settles() {
    let (core, faults) = rig();
    let token = issue(&core);
    Faults::arm(&faults.key_reads, SESSION_ATTEMPTS as u32 - 1);
    let caller = resolve_caller_waiting(&core, &with_token("surface.list", &token))
        .unwrap_or_else(|r| panic!("a retried session judgement denied the caller: {r:?}"));
    assert_eq!(caller.agent_id().as_str(), AGENT);
    assert_eq!(faults.key_reads.load(Ordering::SeqCst), 0);
}

#[test]
fn every_failed_judgement_falls_back_to_a_read_without_writes() {
    let (core, faults) = rig();
    let token = issue(&core);
    Faults::arm(&faults.key_reads, SESSION_ATTEMPTS as u32);
    let caller = resolve_caller_waiting(&core, &with_token("surface.list", &token))
        .unwrap_or_else(|r| panic!("the read fallback denied the caller: {r:?}"));
    assert_eq!(caller.agent_id().as_str(), AGENT);
}

#[test]
fn a_session_that_cannot_be_read_at_all_is_denied_as_before() {
    let (core, faults) = rig();
    let token = issue(&core);
    Faults::arm(&faults.key_reads, SESSION_ATTEMPTS as u32 + 1);
    let denied = resolve_caller_waiting(&core, &with_token("surface.list", &token))
        .expect_err("an unreadable session must not resolve");
    assert_eq!(code(&denied), -32001);
    // 저장소가 회복되면 같은 토큰을 다시 받는다. 거절은 판정을 지우지 않았다.
    assert!(resolve_caller_waiting(&core, &with_token("surface.list", &token)).is_ok());
}

/// 같은 묶음의 다른 요청이 실제 SQLite 실패(trigger)를 내면 저장소는 묶음 전체를 되돌린다.
#[test]
fn a_failed_neighbour_in_the_same_batch_does_not_deny_the_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("memory.db");
    let store = tasty_memory::MemoryStore::open(&path).expect("file store");
    rusqlite::Connection::open(&path)
        .expect("second connection")
        .execute_batch(
            "CREATE TRIGGER block_neighbour BEFORE INSERT ON memory
             WHEN NEW.key = 'gate_neighbour'
             BEGIN SELECT RAISE(ABORT, 'injected neighbour failure'); END;",
        )
        .expect("trigger");
    let core =
        super::super::deferred::tests::writer_core_with(Arc::new(Mutex::new(store)), usize::MAX);
    let token = issue(&core);
    let request = with_token("surface.list", &token);
    let hold = Hold::new(&core);
    let mut neighbour = core
        .memory_access()
        .submit(MemoryRequest::unit(0, |s| {
            s.put(
                HOST_OWNER,
                &Scope::Global,
                "gate_neighbour",
                &MemoryValue::Text("x".into()),
                &PutOpts::default(),
            )
        }))
        .expect("neighbour accepted");
    let mut progress = GateProgress::default();
    assert!(matches!(
        resolve_caller(&core, &request, &mut progress),
        Step::Pending
    ));
    hold.release();
    let failed = settle(
        neighbour
            .wait_timeout(TEST_WAIT)
            .expect("neighbour finished"),
    );
    assert!(
        matches!(failed, Err(tasty_memory::writer::Failure::Storage(_))),
        "{failed:?}"
    );
    let caller = loop {
        if let Step::Ready(result) = resolve_caller_with(&core, &request, &mut progress, true) {
            break result;
        }
    }
    .unwrap_or_else(|r| panic!("a neighbour's failure denied the session: {r:?}"));
    assert_eq!(caller.agent_id().as_str(), AGENT);
}

#[test]
fn an_unknown_judgement_is_submitted_again_because_it_is_idempotent() {
    let (core, faults) = rig();
    let token = issue(&core);
    Faults::arm(&faults.units_after_body, 1);
    let caller = resolve_caller_waiting(&core, &with_token("surface.list", &token))
        .unwrap_or_else(|r| panic!("an unknown judgement denied the caller: {r:?}"));
    assert_eq!(caller.agent_id().as_str(), AGENT);
    assert_eq!(faults.units_after_body.load(Ordering::SeqCst), 0);
}

#[test]
fn a_closed_writer_refuses_the_session_with_the_shutdown_error() {
    let (mut core, _faults) = rig();
    let token = issue(&core);
    let deadline = std::time::Instant::now() + TEST_WAIT;
    assert!(
        holds_by_every(deadline, std::time::Duration::from_millis(1), |_| {
            core.poll_memory_writer_shutdown() == 0
        }),
        "writer did not stop"
    );
    let refused = resolve_caller_waiting(&core, &with_token("surface.list", &token))
        .expect_err("a closed writer cannot judge the session");
    assert_eq!(code(&refused), -32000);
    assert_eq!(
        refused.error.expect("error").message,
        "host is shutting down"
    );
    // 토큰 없는 Local 요청은 저장소를 보지 않으므로 그대로 통과한다.
    let local = split(json!(3));
    assert!(resolve_caller_waiting(&core, &local).is_ok());
}

#[test]
fn a_cap_read_failure_keeps_the_plugin_call_open() {
    let (mut core, faults) = rig();
    put_cap(
        &core,
        triggered_cap("gate-plugin", tasty_telemetry::CapAction::Pause),
    );
    let plugin = CallerContext::Plugin {
        plugin_id: "gate-plugin".into(),
    };
    let mut request = split(json!(3));
    request.method = "surface.list".into();
    request.params = json!({});
    assert_eq!(gate(&mut core, &request, &plugin), Some(-32007));
    Faults::arm(&faults.prefix_lists, 1);
    assert_eq!(
        gate(&mut core, &request, &plugin),
        None,
        "a cap read failure must not block the call"
    );
    assert_eq!(faults.prefix_lists.load(Ordering::SeqCst), 0);
    assert_eq!(gate(&mut core, &request, &plugin), Some(-32007));
}

#[test]
fn a_failed_record_lets_the_call_through_without_consuming_its_budget() {
    let (mut core, faults) = rig();
    let token = issue(&core);
    core.rate_limit_set(
        AGENT.into(),
        "ipc_calls".into(),
        1,
        3_600_000,
        Some(1),
        now(),
    )
    .expect("budget");
    let request = with_token("surface.list", &token);
    let caller = resolve_caller_waiting(&core, &request).expect("caller");
    Faults::arm(&faults.units_before_body, 1);
    assert_eq!(
        gate(&mut core, &request, &caller),
        None,
        "a record failure must not refuse the call"
    );
    assert_eq!(faults.units_before_body.load(Ordering::SeqCst), 0);
    // 실패한 기록은 토큰을 쓰지 않았다. 남은 한 번을 쓰고 다음 호출이 제한된다.
    assert_eq!(gate(&mut core, &request, &caller), None);
    assert_eq!(gate(&mut core, &request, &caller), Some(-32010));
}

#[test]
fn a_local_alias_is_never_cap_blocked_but_its_calls_trigger_its_caps() {
    let (mut core, _faults) = rig();
    let alias = "alias_gate";
    put_cap(
        &core,
        triggered_cap(alias, tasty_telemetry::CapAction::Pause),
    );
    let notify = tasty_telemetry::CostCap {
        id: "cap_alias_notify".into(),
        triggered: None,
        action: tasty_telemetry::CapAction::Notify,
        ..triggered_cap(alias, tasty_telemetry::CapAction::Notify)
    };
    put_cap(&core, notify);
    let mut request = split(json!(3));
    request.method = "surface.list".into();
    request.params = json!({});
    request.caller_agent_id = Some(alias.into());
    let caller = resolve_caller_waiting(&core, &request).expect("local");
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    assert!(
        check_request(&mut core, &mut state, &mut engine, &request, &caller).is_ok(),
        "Local is not cap blocked"
    );
    let notified = state.pending_intents.iter().any(|intent| {
        matches!(
            &intent.body,
            crate::intent::Intent::Domain(crate::app::command::DomainIntent::PushNotification { title, .. })
                if title.contains("cap_alias_notify") || title.contains("ipc_calls")
        )
    });
    assert!(notified, "the alias call did not trigger its notify cap");
}

// ─── 접수 순서와 종료 ───

fn command(
    id: u64,
    token: Option<&str>,
) -> (
    crate::ipc::server::IpcCommand,
    mpsc::Receiver<JsonRpcResponse>,
) {
    let mut request = split(json!(3));
    request.method = "surface.list".into();
    request.params = json!({});
    request.id = Some(json!(id));
    request.session_token = token.map(str::to_string);
    let (tx, rx) = mpsc::sync_channel(1);
    (crate::ipc::server::IpcCommand::new(request, tx), rx)
}

fn waiting(
    core: &AppServices,
    token: &str,
    id: u64,
) -> (
    crate::app::request_gate::Gated,
    mpsc::Receiver<JsonRpcResponse>,
) {
    let (cmd, rx) = command(id, Some(token));
    let mut progress = GateProgress::default();
    assert!(matches!(
        resolve_caller(core, &cmd.request, &mut progress),
        Step::Pending
    ));
    (crate::app::request_gate::Gated::Ipc { cmd, progress }, rx)
}

fn id_of(gated: &crate::app::request_gate::Gated) -> Value {
    match gated {
        crate::app::request_gate::Gated::Ipc { cmd, .. } => cmd.request.id.clone().expect("id"),
        crate::app::request_gate::Gated::Plugin { request, .. } => request.id.clone().expect("id"),
    }
}

#[test]
fn a_later_request_waits_behind_the_request_whose_gate_is_pending() {
    let (core, _faults) = rig();
    let token = issue(&core);
    let mut queue = crate::app::request_gate::GateQueue::default();
    let hold = Hold::new(&core);
    let (first, _first_rx) = waiting(&core, &token, 1);
    queue.park(first, false);
    // 저장소가 필요 없는 Local 요청도 앞 요청을 앞지르지 않는다.
    let (second, _second_rx) = command(2, None);
    queue.park(
        crate::app::request_gate::Gated::Ipc {
            cmd: second,
            progress: GateProgress::default(),
        },
        false,
    );
    assert!(
        queue.pop_ready(&core).is_none(),
        "the ready second request overtook the first"
    );
    hold.release();
    // pop_ready 는 맨 앞 요청이 기다리던 저장 결과를 거둘 뿐 새로 제출하지 않아 기한 뒤 관측 읽기에서도 부른다.
    let deadline = std::time::Instant::now() + TEST_WAIT;
    let mut front = None;
    assert!(
        holds_by_every(deadline, std::time::Duration::from_millis(1), |_| {
            front = queue.pop_ready(&core);
            front.is_some()
        }),
        "the first gate never finished"
    );
    let front = front.expect("checked above");
    assert_eq!(id_of(&front), json!(1));
    // 다시 기다리게 된 맨 앞 요청은 앞자리로 돌아간다.
    queue.park(front, true);
    assert_eq!(
        id_of(&queue.pop_ready(&core).expect("first again")),
        json!(1)
    );
    assert_eq!(id_of(&queue.pop_ready(&core).expect("second")), json!(2));
    assert!(queue.is_empty());
}

#[test]
fn shutdown_answers_every_waiting_request_without_running_it() {
    let (core, _faults) = rig();
    let token = issue(&core);
    let mut queue = crate::app::request_gate::GateQueue::default();
    let hold = Hold::new(&core);
    let (first, first_rx) = waiting(&core, &token, 1);
    queue.park(first, false);
    let (second, second_rx) = command(2, None);
    queue.park(
        crate::app::request_gate::Gated::Ipc {
            cmd: second,
            progress: GateProgress::default(),
        },
        false,
    );
    assert_eq!(queue.reject_all(None), 2);
    assert!(queue.is_empty());
    for rx in [first_rx, second_rx] {
        let answer = rx.try_recv().expect("answered at shutdown");
        assert_eq!(code(&answer), -32000);
        assert_eq!(
            answer.error.expect("error").message,
            "host is shutting down"
        );
    }
    hold.release();
}
