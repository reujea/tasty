//! 새 세션의 `<provider>.spawn` 을 기다리는 동안 러너가 다른 task 를 진행하는지 본다.

use std::time::{Duration, Instant};

use tasty_agent::runner::{DispatchHandle, DispatchOutcome, PollOutcome, TaskExecutor};
use tasty_agent::task::FailureCode;
use tasty_agent::task::agent::{AgentSession, report};

use super::{FakeProvider, create, dispatch, existing, ours, v2};
use crate::agent_turns::{TurnEnd, TurnEvent};
use crate::runner_host::tests::fresh_ctx;
use crate::runner_host::{HostExecutor, load_dispatch_handle};

fn new_session() -> AgentSession {
    AgentSession::New {
        parent_surface: 3,
        cwd: None,
    }
}

/// spawn 이 답하기 전에도 dispatch 는 바로 돌아오고, 같은 executor 의 다른 task 가 진행된다.
#[test]
fn a_slow_spawn_does_not_hold_up_other_tasks() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install_with(&ctx, "idle", Duration::from_secs(2));
    let mut exec = HostExecutor::new(ctx.clone());
    let spawning = create(&ctx, new_session(), v2());
    let started = Instant::now();
    let DispatchOutcome::Started(spawn_handle) = exec.dispatch(&spawning) else {
        panic!("dispatch did not start");
    };
    assert!(
        started.elapsed() < Duration::from_millis(1000),
        "dispatch waited {:?} for the spawn",
        started.elapsed()
    );
    // 다른 task 는 spawn 의 응답을 기다리지 않고 지시를 보낸다.
    let other = create(&ctx, existing(), v2());
    let other_handle = dispatch(&mut exec, &ctx, &other);
    assert!(matches!(exec.poll(&other_handle), PollOutcome::Active));
    assert_eq!(fake.sent(".tell").len(), 1);
    assert_eq!(
        ctx.agent_turns.holder(42),
        None,
        "the spawn has not answered"
    );
    assert!(matches!(exec.poll(&spawn_handle), PollOutcome::Active));

    let until = Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(10), |_| ctx
            .agent_turns
            .holder(42)
            .is_some()),
        "the spawn never bound its session"
    );
    assert_eq!(ctx.agent_turns.holder(42), Some(spawning.id.clone()));
    ctx.agent_turns.report(
        42,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("spawned".into()))),
    );
    // poll 은 끝난 spawn 의 결과를 거두기만 한다. 마지막 읽기로 쓴다.
    let mut outcome = PollOutcome::Active;
    assert!(
        tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(10), |_| {
            outcome = exec.poll(&spawn_handle);
            !matches!(outcome, PollOutcome::Active)
        }),
        "the spawn task never finished"
    );
    let PollOutcome::Done(r) = outcome else {
        panic!("expected done, got {outcome:?}");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "spawned");
    assert_eq!(fake.sent(".spawn").len(), 1, "spawn 은 한 번만 보낸다");

    ctx.agent_turns.report(7, "claude", ours(&ctx, 7));
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("told".into()))),
    );
    let PollOutcome::Done(r) = exec.poll(&other_handle) else {
        panic!("expected the other task to finish");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "told");
}

fn wait_closed(fake: &FakeProvider) -> Vec<serde_json::Value> {
    let until = Instant::now() + Duration::from_secs(5);
    while fake.sent("surface.close").is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    fake.sent("surface.close")
}

/// 상한까지 답하지 않은 spawn 은 task 를 끝내고, 그 뒤에 뜬 세션은 task 가 닫는다.
#[test]
fn a_session_that_starts_after_the_task_gave_up_is_closed() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install_with(&ctx, "idle", Duration::from_millis(300));
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, new_session(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    exec.late
        .spawns
        .get_mut(&task.id)
        .expect("waiting spawn")
        .give_up_at = Instant::now();
    let PollOutcome::Failed(e) = exec.poll(&h) else {
        panic!("expected the task to give up");
    };
    assert_eq!(
        FailureCode::parse_message(&e),
        Some(FailureCode::AgentUnavailable)
    );
    assert!(e.contains("no reply within 30s"), "{e}");
    let closed = wait_closed(&fake);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["surface_id"], 42);
    assert_eq!(ctx.agent_turns.holder(42), None);
}

/// spawn 을 기다리는 동안 취소하면 뒤에 뜬 세션을 닫는다.
#[test]
fn a_session_that_starts_after_the_task_was_cancelled_is_closed() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install_with(&ctx, "idle", Duration::from_millis(300));
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, new_session(), v2());
    dispatch(&mut exec, &ctx, &task);
    exec.release_permit(&task.id);
    let closed = wait_closed(&fake);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["surface_id"], 42);
    assert_eq!(ctx.agent_turns.holder(42), None);
}

/// 응답이 이미 와서 묶인 뒤 넘겨받기 전에 취소해도 묶음을 풀고 세션을 닫는다.
#[test]
fn a_bound_session_not_yet_taken_is_closed_on_cancel() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, new_session(), v2());
    dispatch(&mut exec, &ctx, &task);
    let until = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by_every(until, Duration::from_millis(5), |_| ctx
            .agent_turns
            .holder(42)
            .is_some()),
        "the spawn never bound its session"
    );
    exec.release_permit(&task.id);
    assert_eq!(ctx.agent_turns.holder(42), None);
    let closed = wait_closed(&fake);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["surface_id"], 42);
}

/// spawn 을 기다리는 동안의 저장 handle 은 응답 대기다. 재시작하면 다시 보내지 않고 결과 불명이다.
#[test]
fn a_restart_while_the_spawn_is_waiting_sees_an_unanswered_request() {
    let (_td, ctx) = fresh_ctx();
    let _fake = FakeProvider::install_with(&ctx, "idle", Duration::from_secs(2));
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, new_session(), v2());
    dispatch(&mut exec, &ctx, &task);
    match load_dispatch_handle(&ctx, 1, &task.id) {
        Some(DispatchHandle::AwaitDispatchAnswer { ipc_method, .. }) => {
            assert_eq!(ipc_method, "claude.spawn")
        }
        other => panic!("expected a stored answer wait, got {other:?}"),
    }
}

/// 러너가 멈춰 executor 가 사라져도 뒤에 뜬 세션을 닫는다.
#[test]
fn a_session_that_starts_after_the_runner_stopped_is_closed() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install_with(&ctx, "idle", Duration::from_millis(300));
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, new_session(), v2());
    dispatch(&mut exec, &ctx, &task);
    drop(exec);
    let closed = wait_closed(&fake);
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["surface_id"], 42);
    assert_eq!(ctx.agent_turns.holder(42), None);
}
