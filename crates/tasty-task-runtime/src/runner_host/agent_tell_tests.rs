//! 기존 세션에 보내는 `<provider>.tell` 이 응답 대기 상한(5초)을 넘길 때의 경계.
//!
//! 모의 IPC([`scripted_ipc`](crate::runner_host::scripted_ipc))는 보낸 스레드에서 `.state` 에 idle 로
//! 바로 답하고, 나머지 명령은 정한 단계대로 시작시켜 넘기거나(시험이 늦게 답하거나 닫는다) 시작시키지
//! 않고 넘긴다(큐 대기 중 만료).

use serde_json::{Value, json};
use tasty_agent::runner::{DispatchHandle, PollOutcome, TaskExecutor};
use tasty_agent::task::FailureCode;
use tasty_agent::task::agent::report;
use tasty_ipc::dispatch::DispatchStats;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::{Claim, IpcCommand};

use super::{create, dispatch, existing, ours, v2};
use crate::agent_turns::{TurnEnd, TurnEvent};
use crate::runner_host::late_answer::DISPATCH_OUTCOME_UNKNOWN;
use crate::runner_host::scripted_ipc::{self, Step};
use crate::runner_host::tests::fresh_ctx;
use crate::runner_host::{HostExecutor, RunnerContext, load_dispatch_handle};

fn claim(cmd: &IpcCommand) -> Claim {
    cmd.claim(&std::sync::Arc::new(DispatchStats::default()))
}

fn reply(cmd: IpcCommand, result: Value) {
    cmd.response_tx
        .send(JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(Value::Null),
            result,
        ))
        .expect("send resp");
}

fn stored(ctx: &RunnerContext, task_id: &str) -> Option<DispatchHandle> {
    load_dispatch_handle(ctx, 1, task_id)
}

/// 지연된 handler 가 결국 지시를 전달했다. 다시 보내지 않고 늦은 응답을 받아 그 턴을 이어 간다.
#[test]
fn a_tell_that_answers_past_the_bound_is_not_sent_again() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(&ctx, vec![Step::Hold], true);
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    match exec.poll(&h) {
        PollOutcome::Active => {}
        other => panic!("a tell that started past the bound must not end the task, got {other:?}"),
    }
    assert_eq!(ctx.agent_turns.holder(7), Some(task.id.clone()));
    assert!(
        matches!(
            stored(&ctx, &task.id),
            Some(DispatchHandle::AwaitDispatchAnswer { .. })
        ),
        "a restart while waiting must see an unanswered request"
    );
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    let tell = ipc.rx.try_recv().expect("the tell was held");
    assert_eq!(tell.request.method, "claude.tell");
    reply(tell, json!({ "ok": true }));
    // 늦은 응답은 이미 통로에 있다. 다음 poll 이 그것을 받아 턴을 이어 간다.
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert!(
        matches!(
            stored(&ctx, &task.id),
            Some(DispatchHandle::AgentTurn { .. })
        ),
        "the late answer was taken"
    );
    ctx.agent_turns.report(7, "claude", ours(&ctx, 7));
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("mine".into()))),
    );
    let PollOutcome::Done(r) = exec.poll(&h) else {
        panic!("expected done");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "mine");
    ipc.assert_all_steps_used();
    assert!(ipc.rx.try_recv().is_err(), "the tell was sent exactly once");
}

/// 큐에서 기다리다 만료된 지시는 실행되지 않았다. 묶음을 풀고 다음 poll 에서 다시 보낸다.
#[test]
fn a_tell_that_expired_in_the_queue_is_sent_again() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(
        &ctx,
        vec![
            Step::Leave,
            Step::Reply(Box::new(|tell| {
                assert_eq!(tell.request.method, "claude.tell");
                json!({ "ok": true })
            })),
        ],
        true,
    );
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    match exec.poll(&h) {
        PollOutcome::Active => {}
        other => panic!("a tell that never ran must not end the task, got {other:?}"),
    }
    let stale = ipc.rx.try_recv().expect("the expired tell was queued");
    assert_eq!(
        claim(&stale),
        Claim::Withdrawn,
        "the expired tell never runs"
    );
    assert_eq!(ctx.agent_turns.holder(7), None, "the binding is released");
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert_eq!(ctx.agent_turns.holder(7), Some(task.id.clone()));
    ctx.agent_turns.report(7, "claude", ours(&ctx, 7));
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("done".into()))),
    );
    let PollOutcome::Done(r) = exec.poll(&h) else {
        panic!("expected done");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "done");
    ipc.assert_all_steps_used();
    assert!(ipc.rx.try_recv().is_err(), "no third tell");
}

/// 시작한 지시의 응답 통로가 답 없이 닫혔다. 전달됐는지 알 수 없어 결과 불명이다.
#[test]
fn a_tell_whose_channel_closes_after_it_started_is_an_unknown_outcome() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(&ctx, vec![Step::Hold], true);
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    drop(ipc.rx.try_recv().expect("the tell was held"));
    ipc.assert_all_steps_used();
    match exec.poll(&h) {
        PollOutcome::Lost(reason) => {
            assert!(reason.starts_with(DISPATCH_OUTCOME_UNKNOWN), "{reason}");
            assert!(reason.contains("claude.tell"), "{reason}");
        }
        other => panic!("expected an unknown outcome, got {other:?}"),
    }
}

/// 늦게 온 응답이 오류면 지시는 전달되지 않았다. 회차를 실패로 끝내고 묶음을 푼다.
#[test]
fn a_tell_that_fails_late_fails_the_attempt() {
    let (_td, ctx) = fresh_ctx();
    let ipc = scripted_ipc::install(&ctx, vec![Step::Hold], true);
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    let tell = ipc.rx.try_recv().expect("the tell was held");
    let id = tell.request.id.clone().unwrap_or(Value::Null);
    tell.response_tx
        .send(JsonRpcResponse::error(id, -32000, "no such session"))
        .expect("send resp");
    ipc.assert_all_steps_used();
    let PollOutcome::Failed(e) = exec.poll(&h) else {
        panic!("expected failure");
    };
    assert_eq!(
        FailureCode::parse_message(&e),
        Some(FailureCode::AgentUnavailable)
    );
    assert_eq!(ctx.agent_turns.holder(7), None);
}
