//! agent task 실행을 가짜 provider 로 결정적으로 시험한다. provider 는 `<p>.state` 에 정해 둔
//! 상태로 답하고 `<p>.tell`·`<p>.spawn` 호출을 기록한다. 턴 보고는 표에 직접 넣는다.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_agent::runner::{DispatchHandle, DispatchOutcome, PollOutcome, TaskExecutor};
use tasty_agent::task::agent::{AgentSession, report};
use tasty_agent::task::contract::TaskContract;
use tasty_agent::task::{FailureCode, TaskCreateOpts};
use tasty_agent::{OnFailure, Task, TaskCommand, TaskState, TaskStore};
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::IpcCommand;
use tasty_memory::HOST_OWNER;

use super::HostExecutor;
use crate::agent_turns::{StartPrompt, TurnEnd, TurnEvent};
use crate::runner_host::tests::fresh_ctx;
use crate::runner_host::{RunnerContext, load_dispatch_handle};

/// 가짜 provider. 상태를 바꾸며 호출을 기록한다.
struct FakeProvider {
    state: Arc<Mutex<String>>,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
}

impl FakeProvider {
    fn install(ctx: &RunnerContext, initial: &str) -> Self {
        Self::install_with(ctx, initial, Duration::ZERO)
    }

    /// `spawn_delay` 만큼 늦게 spawn 에 답한다.
    fn install_with(ctx: &RunnerContext, initial: &str, spawn_delay: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, Arc::new(|| {})))
            .ok()
            .expect("set once");
        let state = Arc::new(Mutex::new(initial.to_string()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (s, c) = (state.clone(), calls.clone());
        std::thread::spawn(move || {
            while let Ok(cmd) = rx.recv_timeout(Duration::from_secs(30)) {
                let method = cmd.request.method.clone();
                c.lock()
                    .unwrap()
                    .push((method.clone(), cmd.request.params.clone()));
                let body = if method.ends_with(".state") {
                    json!({ "state": s.lock().unwrap().clone() })
                } else if method.ends_with(".spawn") {
                    // 늦은 spawn 이 다른 명령의 응답을 막지 않게 따로 답한다.
                    std::thread::spawn(move || {
                        std::thread::sleep(spawn_delay);
                        let id = cmd.request.id.clone().unwrap_or(Value::Null);
                        let body = json!({ "child_surface_id": 42 });
                        if cmd
                            .response_tx
                            .send(JsonRpcResponse::success(id, body))
                            .is_err()
                        {
                            tracing::warn!("spawn reply: the caller is gone");
                        }
                    });
                    continue;
                } else {
                    json!({ "ok": true })
                };
                let id = cmd.request.id.clone().unwrap_or(Value::Null);
                if cmd
                    .response_tx
                    .send(JsonRpcResponse::success(id, body))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self { state, calls }
    }

    fn set(&self, s: &str) {
        *self.state.lock().unwrap() = s.to_string();
    }

    fn sent(&self, suffix: &str) -> Vec<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m.ends_with(suffix))
            .map(|(_, p)| p.clone())
            .collect()
    }
}

/// surface 에 묶인 회차의 지시로 시작한 턴. 프롬프트의 표지가 그 회차 토큰이다.
fn ours(ctx: &RunnerContext, surface: u32) -> TurnEvent {
    let token = ctx.agent_turns.get(surface).expect("bound").token;
    TurnEvent::Started(StartPrompt::Seen(Some(token)))
}

fn contract(v: Value) -> TaskContract {
    serde_json::from_value(v).expect("contract")
}

/// Ready 인 v2 agent task 를 만든다.
fn create(ctx: &RunnerContext, session: AgentSession, c: TaskContract) -> Task {
    create_in(ctx, 1, 1, session, c)
}

/// workspace `ws` 에 task 를 만든다. 세션은 workspace `session_ws` 에 뜬다.
fn create_in(
    ctx: &RunnerContext,
    ws: u32,
    session_ws: u32,
    session: AgentSession,
    c: TaskContract,
) -> Task {
    tasty_test_support::namespace_owners::publish_fixture();
    let seq = ctx.agent_seq.clone();
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, seq.as_ref())
            .create_typed(
                TaskCreateOpts {
                    workspace_id: ws,
                    name: "review".into(),
                    command: TaskCommand::Agent {
                        provider: "claude".into(),
                        workspace_id: session_ws,
                        session,
                        instruction: "review".into(),
                        timeout_ms: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: Value::Null,
                    now_ms: 0,
                },
                c,
            )
            .expect("create")
    })
}

/// runner 처럼 dispatch 뒤 Running 으로 만든다.
fn dispatch(exec: &mut HostExecutor, ctx: &RunnerContext, task: &Task) -> DispatchHandle {
    let DispatchOutcome::Started(h) = exec.dispatch(task) else {
        panic!("dispatch did not start");
    };
    let seq = ctx.agent_seq.clone();
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, seq.as_ref())
            .set_state(task.workspace_id, &task.id, TaskState::Running, 1)
            .expect("running")
    });
    h
}

/// 새 세션 task 의 spawn 이 답해 회차에 묶일 때까지 러너가 드는 handle 을 poll 한다.
/// 이어 간 `AgentTurn`(저장 handle)을 돌려준다.
fn wait_spawned(
    exec: &mut HostExecutor,
    ctx: &RunnerContext,
    task: &Task,
    h: &DispatchHandle,
) -> DispatchHandle {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(turn @ DispatchHandle::AgentTurn { .. }) =
            load_dispatch_handle(ctx, task.workspace_id, &task.id)
        {
            return turn;
        }
        assert!(Instant::now() < until, "the spawn never bound its session");
        assert!(matches!(exec.poll(h), PollOutcome::Active));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn v2() -> TaskContract {
    contract(json!({ "contract_version": 2 }))
}

fn existing() -> AgentSession {
    AgentSession::Existing { surface_id: 7 }
}

/// 기존 세션에 보낸 지시는 회차 표지로 끝난다. 표지가 없는 사용자 턴은 이 회차의 턴이 아니다.
#[test]
fn the_instruction_ends_with_the_attempt_marker_and_a_user_turn_is_not_taken() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    let token = ctx.agent_turns.get(7).expect("bound").token;
    let message = fake.sent(".tell")[0]["message"]
        .as_str()
        .unwrap()
        .to_string();
    let marker = format!("[tasty-task-attempt:{token}]");
    assert!(message.trim_end().ends_with(&marker), "{message}");
    // 사용자가 그 사이 시작한 턴과 그 끝은 받지 않는다.
    ctx.agent_turns
        .report(7, "claude", TurnEvent::Started(StartPrompt::Seen(None)));
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("user's".into()))),
    );
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
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
}

#[test]
fn a_busy_session_gets_no_instruction_until_it_is_idle() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "active");
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert!(
        fake.sent(".tell").is_empty(),
        "사용자의 턴에 끼어들지 않는다"
    );
    fake.set("idle");
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert_eq!(fake.sent(".tell").len(), 1);
    assert_eq!(fake.sent(".tell")[0]["surface_id"], 7);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert_eq!(fake.sent(".tell").len(), 1, "한 번만 보낸다");
}

#[test]
fn an_idle_session_without_a_turn_end_report_does_not_succeed() {
    let (_td, ctx) = fresh_ctx();
    let _fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    for _ in 0..3 {
        assert!(matches!(exec.poll(&h), PollOutcome::Active));
    }
    // 이전 턴의 종료 보고(시작 보고 전)는 이 회차에 적용하지 않는다.
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("old".into()))),
    );
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    ctx.agent_turns.report(7, "claude", ours(&ctx, 7));
    ctx.agent_turns.report(
        7,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("new".into()))),
    );
    let PollOutcome::Done(r) = exec.poll(&h) else {
        panic!("expected done");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "new");
}

#[test]
fn needs_input_is_recorded_as_awaiting_input_and_does_not_finish() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(&ctx, existing(), v2());
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    ctx.agent_turns.report(7, "claude", ours(&ctx, 7));
    fake.set("needs_input");
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    let seq = ctx.agent_seq.clone();
    let stored = ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, seq.as_ref())
            .get(1, &task.id)
            .unwrap()
            .unwrap()
    });
    let link = stored.attempt.unwrap().agent.expect("link");
    assert_eq!(link.surface_id, 7);
    assert!(link.awaiting_input_since.is_some());
    fake.set("exited");
    let PollOutcome::Failed(e) = exec.poll(&h) else {
        panic!("expected failure");
    };
    assert_eq!(
        FailureCode::parse_message(&e),
        Some(FailureCode::AgentExited)
    );
}

#[test]
fn a_second_task_on_the_same_session_waits_for_the_first() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let first = create(&ctx, existing(), v2());
    let second = create(&ctx, existing(), v2());
    let h1 = dispatch(&mut exec, &ctx, &first);
    let h2 = dispatch(&mut exec, &ctx, &second);
    exec.poll(&h1);
    exec.poll(&h2);
    assert_eq!(fake.sent(".tell").len(), 1, "두 번째 지시는 보내지 않는다");
    assert_eq!(ctx.agent_turns.holder(7), Some(first.id.clone()));
    exec.release_permit(&first.id);
    exec.poll(&h2);
    assert_eq!(fake.sent(".tell").len(), 2);
    assert_eq!(ctx.agent_turns.holder(7), Some(second.id.clone()));
}

#[test]
fn a_structured_output_needs_a_submission_and_the_new_session_is_told_how() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "active");
    let mut exec = HostExecutor::new(ctx.clone());
    let c = contract(json!({
        "contract_version": 2,
        "output_schema": { "type": "enum", "values": ["approve", "revise"] }
    }));
    let task = create(
        &ctx,
        AgentSession::New {
            parent_surface: 3,
            cwd: None,
        },
        c,
    );
    let h = dispatch(&mut exec, &ctx, &task);
    wait_spawned(&mut exec, &ctx, &task, &h);
    let spawned = fake.sent(".spawn");
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0]["surface"], 3);
    let prompt = spawned[0]["prompt"].as_str().unwrap();
    assert!(prompt.contains("tasty agent task-submit"), "{prompt}");
    assert!(prompt.contains(&format!("{}#1", task.id)), "{prompt}");
    // 회차 토큰은 회차 id 에서 짐작할 수 없는 값이고 턴 표의 토큰과 같다.
    let token = ctx.agent_turns.get(42).expect("bound").token;
    assert_eq!(token.len(), 32, "{token}");
    assert!(prompt.contains(&format!("--token '{token}'")), "{prompt}");
    assert!(!token.contains(&task.id), "{token}");
    // report 는 사용법을 읽는 명령 한 줄과 이 회차의 주소만 싣는다.
    let stored = ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &task.id)
            .unwrap()
            .unwrap()
    });
    let report = stored.report_token.expect("report token");
    assert_eq!(report.attempt, 1);
    assert_ne!(report.token, token);
    let line =
        tasty_agent::task::agent::report_line(&format!("1/1/agent/{}/{}", report.token, task.id));
    assert!(prompt.contains(&line), "{prompt}");
    assert_eq!(prompt.matches("tasty agent report").count(), 1, "{prompt}");
    // 새 세션의 첫 턴은 spawn 의 지시다. 시작 보고 없이도 종료를 받는다.
    ctx.agent_turns.report(
        42,
        "claude",
        TurnEvent::Ended(TurnEnd::Answer(Some("I think revise".into()))),
    );
    // 제출이 없으므로 보고에는 답만 있다. 결과 확정이 result_missing 으로 끝낸다.
    let PollOutcome::Done(r) = exec.poll(&h) else {
        panic!("expected a report without a submission");
    };
    let out = r.output.unwrap();
    assert!(out.get(report::SUBMITTED).is_none());
    assert_eq!(out[report::FINAL_ANSWER], "I think revise");
}

/// spawn 은 일반 호출의 응답 대기(5초)보다 늦게 답할 수 있다. 그래도 회차에 묶는다.
/// 같은 이름의 task 가 두 workspace 에서 돈다. 한쪽이 끝나 풀려도 다른 쪽의 턴은 이어지고,
/// 각자 자기 세션의 턴으로 끝난다.
#[test]
fn the_same_task_name_in_two_workspaces_keeps_its_own_turn() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec1 = HostExecutor::new(ctx.clone());
    let mut exec2 = HostExecutor::new(ctx.clone());
    let t1 = create_in(&ctx, 1, 1, existing(), v2());
    let mut t2 = t1.clone();
    t2.workspace_id = 2;
    t2.command = TaskCommand::Agent {
        provider: "claude".into(),
        workspace_id: 2,
        session: AgentSession::Existing { surface_id: 9 },
        instruction: "review".into(),
        timeout_ms: None,
    };
    let seq = ctx.agent_seq.clone();
    ctx.with_store(|mem| TaskStore::new(mem, HOST_OWNER, seq.as_ref()).put(&t2))
        .expect("put");
    assert_eq!(t1.id, t2.id);
    let h1 = dispatch(&mut exec1, &ctx, &t1);
    let h2 = dispatch(&mut exec2, &ctx, &t2);
    assert!(matches!(exec1.poll(&h1), PollOutcome::Active));
    assert!(matches!(exec2.poll(&h2), PollOutcome::Active));
    assert_eq!(fake.sent(".tell").len(), 2);
    for (surface, answer) in [(7, "one"), (9, "two")] {
        ctx.agent_turns
            .report(surface, "claude", ours(&ctx, surface));
        ctx.agent_turns.report(
            surface,
            "claude",
            TurnEvent::Ended(TurnEnd::Answer(Some(answer.into()))),
        );
    }
    let PollOutcome::Done(r1) = exec1.poll(&h1) else {
        panic!("ws1 should finish");
    };
    assert_eq!(r1.output.unwrap()[report::FINAL_ANSWER], "one");
    exec1.release_permit(&t1.id);
    assert!(ctx.agent_turns.get(7).is_none());
    let PollOutcome::Done(r2) = exec2.poll(&h2) else {
        panic!("ws2 should still have its turn after ws1 released");
    };
    let out = r2.output.unwrap();
    assert_eq!(out[report::FINAL_ANSWER], "two");
    assert_eq!(out[report::SURFACE_ID], 9);
    assert_eq!(fake.sent(".tell").len(), 2, "다시 보내지 않는다");
}

/// 세션이 다른 workspace 에 떠도 제출 안내와 회차 기록은 task 자신의 workspace 를 쓴다.
#[test]
fn a_session_in_another_workspace_submits_to_the_task_workspace() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "active");
    let mut exec = HostExecutor::new(ctx.clone());
    let c = contract(json!({
        "contract_version": 2,
        "output_schema": { "type": "enum", "values": ["approve", "revise"] }
    }));
    let new = AgentSession::New {
        parent_surface: 3,
        cwd: None,
    };
    let task = create_in(&ctx, 1, 5, new, c);
    let h = dispatch(&mut exec, &ctx, &task);
    let turn = wait_spawned(&mut exec, &ctx, &task, &h);
    let spawned = fake.sent(".spawn");
    assert_eq!(spawned[0]["workspace"], "5");
    let prompt = spawned[0]["prompt"].as_str().unwrap();
    assert!(prompt.contains("--workspace 1 "), "{prompt}");
    let DispatchHandle::AgentTurn { workspace_id, .. } = &turn else {
        panic!("agent handle");
    };
    assert_eq!(*workspace_id, 1);
    assert!(ctx.agent_turns.find(1, &task.id).is_some());
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    let seq = ctx.agent_seq.clone();
    let stored = ctx
        .with_store(|mem| TaskStore::new(mem, HOST_OWNER, seq.as_ref()).get(1, &task.id))
        .expect("get")
        .expect("task");
    let link = stored.attempt.and_then(|a| a.agent).expect("session link");
    assert_eq!(link.surface_id, 42);
}

#[test]
fn a_slow_spawn_still_binds_the_new_session() {
    let (_td, ctx) = fresh_ctx();
    let _fake = FakeProvider::install_with(&ctx, "active", Duration::from_millis(5500));
    let mut exec = HostExecutor::new(ctx.clone());
    let task = create(
        &ctx,
        AgentSession::New {
            parent_surface: 3,
            cwd: None,
        },
        v2(),
    );
    let h = dispatch(&mut exec, &ctx, &task);
    wait_spawned(&mut exec, &ctx, &task, &h);
    assert_eq!(ctx.agent_turns.holder(42), Some(task.id.clone()));
}

#[test]
fn a_restored_handle_without_a_binding_cannot_be_attributed() {
    let (_td, ctx) = fresh_ctx();
    let _fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let h = DispatchHandle::AgentTurn {
        workspace_id: 1,
        task_id: "t".into(),
        attempt_id: "t#1".into(),
        provider: "claude".into(),
        surface_id: 42,
        needs_submission: false,
        deadline_ms: None,
        pending_instruction: None,
        attempt_token: "tok".into(),
    };
    let PollOutcome::Failed(e) = exec.poll(&h) else {
        panic!("expected failure");
    };
    assert_eq!(
        FailureCode::parse_message(&e),
        Some(FailureCode::AgentUnavailable)
    );
}

/// 생성 뒤 provider 자격을 잃은 task 는 실행 직전에 거절하고 원인을 구분해 남긴다.
#[test]
fn an_unsupported_provider_is_refused_at_dispatch() {
    use tasty_test_support::namespace_owners::{DISABLED, NO_PERMISSION};
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    for (provider, reason) in [
        ("other", "no installed plugin owns the 'other' namespace"),
        (DISABLED, "that owns the 'off' namespace is disabled"),
        (
            NO_PERMISSION,
            "does not have the agent.turn_report permission",
        ),
    ] {
        let mut task = create(&ctx, existing(), v2());
        if let TaskCommand::Agent { provider: p, .. } = &mut task.command {
            *p = provider.into();
        }
        let DispatchOutcome::PermanentFail(e) = exec.dispatch(&task) else {
            panic!("expected refusal for {provider}");
        };
        assert_eq!(
            FailureCode::parse_message(&e),
            Some(FailureCode::AgentUnavailable),
            "{e}"
        );
        assert!(e.contains(reason), "{provider}: {e}");
    }
}

/// 고정 목록에 없던 커스텀 provider 도 같은 spawn·tell·state 와 턴 보고로 결과가 확정된다.
#[test]
fn a_custom_provider_plugin_runs_and_its_turn_report_finishes_the_task() {
    let (_td, ctx) = fresh_ctx();
    let fake = FakeProvider::install(&ctx, "idle");
    let mut exec = HostExecutor::new(ctx.clone());
    let mut task = create(&ctx, existing(), v2());
    if let TaskCommand::Agent { provider, .. } = &mut task.command {
        *provider = "foo".into();
    }
    let h = dispatch(&mut exec, &ctx, &task);
    assert!(matches!(exec.poll(&h), PollOutcome::Active));
    assert_eq!(
        fake.sent("foo.tell").len(),
        1,
        "foo.tell must carry the instruction"
    );
    assert!(fake.sent("claude.tell").is_empty());
    ctx.agent_turns.report(7, "foo", ours(&ctx, 7));
    ctx.agent_turns.report(
        7,
        "foo",
        TurnEvent::Ended(TurnEnd::Answer(Some("from foo".into()))),
    );
    let PollOutcome::Done(r) = exec.poll(&h) else {
        panic!("expected done");
    };
    assert_eq!(r.output.unwrap()[report::FINAL_ANSWER], "from foo");
}

#[test]
fn attempt_tokens_are_fresh_for_every_attempt() {
    let a = super::new_attempt_token();
    let b = super::new_attempt_token();
    assert_ne!(a, b);
    assert!(
        a.len() == 32 && a.chars().all(|c| c.is_ascii_hexdigit()),
        "{a}"
    );
}

#[path = "agent_tell_tests.rs"]
mod tell;

#[path = "agent_spawn_tests.rs"]
mod spawn;
