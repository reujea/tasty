//! 호스트가 응답 대기 상한(5초) 안에 답하지 못할 때의 실행·poll 경계.
//!
//! 시작 뒤 만료는 [`scripted_ipc`](super::super::scripted_ipc) 가 보낸 스레드에서 명령을 시작시키고
//! 시험 본체가 첫 호출이 반환한 뒤 답하거나 닫아 재현한다. 큐 대기 중 만료는 아무도 꺼내지 않아
//! 재현한다.

use std::collections::HashMap;
use std::sync::mpsc;

use serde_json::{Value, json};
use tasty_agent::runner::{DispatchHandle, DispatchOutcome, PollOutcome, TaskExecutor};
use tasty_agent::{OnFailure, PollSpec, PollSpecRef, Task, TaskCommand, TaskState};
use tasty_ipc::dispatch::DispatchStats;
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::{Claim, IpcCommand};

use super::super::scripted_ipc::{self, Step};
use super::super::tests::fresh_ctx;
use super::super::{HostExecutor, RunnerContext};
use super::DISPATCH_OUTCOME_UNKNOWN;

fn install(ctx: &RunnerContext) -> mpsc::Receiver<IpcCommand> {
    let (tx, rx) = mpsc::channel::<IpcCommand>();
    ctx.host_ipc
        .set(HostIpcInjector::new(tx, std::sync::Arc::new(|| {})))
        .ok()
        .expect("set once");
    rx
}

fn reply(cmd: IpcCommand, result: Value) {
    cmd.response_tx
        .send(JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(Value::Null),
            result,
        ))
        .expect("send resp");
}

fn claim(cmd: &IpcCommand) -> Claim {
    cmd.claim(&std::sync::Arc::new(DispatchStats::default()))
}

fn polled(deadline_ms: Option<u64>) -> DispatchHandle {
    DispatchHandle::PolledDispatch {
        workspace_id: 1,
        poll_method: "fake.state".to_string(),
        poll_params: json!({ "surface_id": 9 }),
        state_field: "state".to_string(),
        terminal_states: vec!["idle".to_string()],
        failure_states: vec!["exited".to_string()],
        interval_ms: 1,
        deadline_ms,
    }
}

/// `fake.spawn` 을 보내고 응답의 `surface_id` 로 `fake.state` 를 poll 하는 Custom task.
fn spawn_task() -> Task {
    let mut map_from_response = HashMap::new();
    map_from_response.insert("surface_id".to_string(), "surface_id".to_string());
    Task {
        id: "t-spawn".to_string(),
        workspace_id: 1,
        name: "spawn".into(),
        command: TaskCommand::Custom {
            ipc_method: "fake.spawn".into(),
            params: json!({ "cwd": "/w" }),
            poll: Some(Box::new(PollSpecRef::Inline(PollSpec {
                poll_method: "fake.state".into(),
                map_from_response,
                map_from_request: HashMap::new(),
                state_field: "state".into(),
                terminal_states: vec!["idle".into()],
                failure_states: vec!["exited".into()],
                interval_ms: 1,
                timeout_ms: None,
            }))),
        },
        state: TaskState::Ready,
        depends_on: vec![],
        on_failure: OnFailure::Abort,
        metadata: Value::Null,
        reserved_for_fallback: false,
        contract: None,
        typed_result: None,
        graph_id: None,
        input_snapshot: None,
        accepted: None,
        report_token: None,
        attempt: None,
        route: None,
        skip: None,
        result: None,
        created_at: 0,
        started_at: None,
        finished_at: None,
    }
}

#[test]
fn a_state_poll_that_started_but_answered_past_the_bound_asks_again() {
    let (_td, ctx) = fresh_ctx();
    // 첫 조회는 시작한 채 답하지 않는다. 쥔 명령이 끝까지 살아 있어 채널 종료가 아니라 상한 만료다.
    let ipc = scripted_ipc::install(
        &ctx,
        vec![
            Step::Hold,
            Step::Reply(Box::new(|_| json!({ "state": "idle" }))),
        ],
        false,
    );
    let mut exec = HostExecutor::new(ctx);
    let handle = polled(None);
    match exec.poll(&handle) {
        PollOutcome::Active => {}
        other => panic!("a slow state query must not end the task, got {other:?}"),
    }
    match exec.poll(&handle) {
        PollOutcome::Done(r) => assert_eq!(r.output, Some(json!({ "state": "idle" }))),
        other => panic!("expected Done on the next poll, got {other:?}"),
    }
    ipc.assert_all_steps_used();
    drop(ipc.rx.try_recv().expect("the slow poll was held"));
}

#[test]
fn a_state_poll_that_expired_in_the_queue_asks_again() {
    let (_td, ctx) = fresh_ctx();
    let rx = install(&ctx);
    let mut exec = HostExecutor::new(ctx);
    let handle = polled(None);
    // 아무도 꺼내지 않아 큐에서 상한이 지난다.
    match exec.poll(&handle) {
        PollOutcome::Active => {}
        other => panic!("a queued expiry must not end the task, got {other:?}"),
    }
    let stale = rx.recv().expect("the expired poll was queued");
    assert_eq!(claim(&stale), Claim::Withdrawn);
}

#[test]
fn a_slow_state_poll_past_the_poll_deadline_is_a_poll_timeout() {
    let (_td, ctx) = fresh_ctx();
    let _rx = install(&ctx);
    let mut exec = HostExecutor::new(ctx);
    match exec.poll(&polled(Some(1))) {
        PollOutcome::Failed(e) => {
            assert!(e.contains("poll timeout"), "{e}");
            assert!(e.contains("host_dispatch timeout"), "{e}");
        }
        other => panic!("expected the poll deadline to end it, got {other:?}"),
    }
}

#[test]
fn a_spawn_that_answers_past_the_bound_is_followed_without_sending_it_again() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(
        &ctx,
        vec![
            Step::Hold,
            Step::Reply(Box::new(|state| {
                assert_eq!(state.request.method, "fake.state");
                assert_eq!(state.request.params.get("surface_id"), Some(&json!(9)));
                json!({ "state": "idle" })
            })),
        ],
        false,
    );
    let mut exec = HostExecutor::new(ctx);
    let task = spawn_task();
    let handle = match exec.dispatch(&task) {
        DispatchOutcome::Started(h) => h,
        other => panic!("a started spawn past the bound must not fail the task, got {other:?}"),
    };
    assert!(
        matches!(handle, DispatchHandle::AwaitDispatchAnswer { .. }),
        "{handle:?}"
    );
    assert!(matches!(exec.poll(&handle), PollOutcome::Active));
    let spawn = ipc.rx.try_recv().expect("the spawn was held");
    assert_eq!(spawn.request.method, "fake.spawn");
    reply(spawn, json!({ "surface_id": 9 }));
    // 늦은 응답은 이미 통로에 있다. 다음 poll 이 그것을 받아 state 를 조회한다.
    match exec.poll(&handle) {
        PollOutcome::Done(r) => assert_eq!(r.output, Some(json!({ "state": "idle" }))),
        other => panic!("expected the spawn-wait poll to finish, got {other:?}"),
    }
    ipc.assert_all_steps_used();
    assert!(
        ipc.rx.try_recv().is_err(),
        "the spawn was sent exactly once"
    );
}

#[test]
fn a_spawn_that_expired_in_the_queue_is_retried_on_a_later_tick() {
    let (_td, ctx) = fresh_ctx();
    let rx = install(&ctx);
    let mut exec = HostExecutor::new(ctx);
    let task = spawn_task();
    match exec.dispatch(&task) {
        DispatchOutcome::Deferred => {}
        other => panic!("a spawn that never ran must stay Ready for a retry, got {other:?}"),
    }
    let stale = rx.recv().expect("the expired spawn was queued");
    assert_eq!(
        claim(&stale),
        Claim::Withdrawn,
        "the expired spawn never runs"
    );
}

#[test]
fn a_spawn_whose_channel_closes_after_it_started_is_an_unknown_outcome() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(&ctx, vec![Step::Hold], false);
    let mut exec = HostExecutor::new(ctx);
    let handle = match exec.dispatch(&spawn_task()) {
        DispatchOutcome::Started(h) => h,
        other => panic!("expected Started, got {other:?}"),
    };
    drop(ipc.rx.try_recv().expect("the spawn was held"));
    ipc.assert_all_steps_used();
    match exec.poll(&handle) {
        PollOutcome::Lost(reason) => {
            assert!(reason.starts_with(DISPATCH_OUTCOME_UNKNOWN), "{reason}")
        }
        other => panic!("expected an unknown outcome, got {other:?}"),
    }
}

#[test]
fn an_awaited_answer_this_executor_does_not_hold_is_an_unknown_outcome() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx);
    let handle = DispatchHandle::AwaitDispatchAnswer {
        workspace_id: 1,
        task_id: "t-gone".to_string(),
        ipc_method: "fake.spawn".to_string(),
    };
    match exec.poll(&handle) {
        PollOutcome::Lost(reason) => {
            assert!(reason.starts_with(DISPATCH_OUTCOME_UNKNOWN), "{reason}");
            assert!(reason.contains("fake.spawn"), "{reason}");
        }
        other => panic!("expected an unknown outcome, got {other:?}"),
    }
}
