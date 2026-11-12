//! 훅 대기는 건 회차에만 보고한다 — 늦게 온 옛 회차의 훅이 다음 회차를 끝내지 않는다.
//! 재시작은 완료 쓰기 뒤 끊긴 하류 반영을 마무리한다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use tasty_agent::task::{Completion, TaskGraphSpec};
use tasty_agent::{TaskState, TaskStore};
use tasty_memory::{HOST_OWNER, MemoryStore, MemoryValue, PutOpts, Scope};

use super::{expire_overdue_hook_waits, purge_and_reload_on_restart};
use crate::hook_wait::HookWaitOwner;
use crate::runner_host::{HANDLE_ATTEMPT_FIELD, RunnerContext, handle_key, run_result_key};

fn ctx() -> (tempfile::TempDir, RunnerContext) {
    let td = tempfile::tempdir().unwrap();
    let mem = MemoryStore::open(&td.path().join("mem.db")).unwrap();
    let ctx = RunnerContext {
        scope_stopping: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        memory: tasty_memory::writer::MemoryAccess::inline(Arc::new(Mutex::new(mem))),
        agent_seq: Arc::new(AtomicU64::new(0)),
        host_ipc: Arc::new(OnceLock::new()),
        task_waker_hub: Arc::new(crate::task_waker::TaskWakerHub::new()),
        hook_task_waits: Arc::new(crate::hook_wait::HookTaskWaits::new()),
        agent_turns: Default::default(),
        completion: Arc::new(crate::completion::fixture::Resolver::default()),
        report_limits: Default::default(),
        host_dispatch_timeout: crate::runner_host::HOST_DISPATCH_TIMEOUT,
    };
    (td, ctx)
}

fn with_store<R>(ctx: &RunnerContext, f: impl FnOnce(&mut TaskStore) -> R) -> R {
    ctx.with_store(|mem| f(&mut TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())))
}

fn state_of(ctx: &RunnerContext, id: &str) -> TaskState {
    with_store(ctx, |s| s.get(1, &id.into()).unwrap().unwrap().state)
}

fn state(ctx: &RunnerContext) -> TaskState {
    state_of(ctx, "p")
}

fn register(ctx: &RunnerContext, hook_id: u64, attempt: &str) {
    ctx.hook_task_waits.register_owned(
        hook_id,
        1,
        "p".into(),
        2000,
        HookWaitOwner {
            agent_seq: ctx.agent_seq.clone(),
            completion: ctx.task_waker_hub.clone(),
        },
        Some(attempt.into()),
    );
}

#[test]
fn an_expired_wait_from_an_earlier_attempt_leaves_the_current_run_alone() {
    let (_td, ctx) = ctx();
    with_store(&ctx, |s| {
        let spec: TaskGraphSpec = serde_json::from_value(serde_json::json!({
            "contract_version": 2,
            "tasks": [{"id": "p",
                "command": {"kind": "custom", "ipc_method": "system.ping", "params": {}},
                "output_schema": {"type": "object", "fields": {}}}]
        }))
        .unwrap();
        s.submit_graph(1, spec, 0).unwrap();
        let p = "p".to_string();
        s.set_state(1, &p, TaskState::Running, 1).unwrap();
        s.complete(1, &p, Completion::failed(None, "boom".into()), 2)
            .unwrap();
        s.retry(1, &p, false, 3).unwrap();
        s.set_state(1, &p, TaskState::Running, 4).unwrap();
    });
    register(&ctx, 1, "p#1");
    expire_overdue_hook_waits(&ctx, 5000);
    assert_eq!(
        state(&ctx),
        TaskState::Running,
        "p#1 의 만료가 p#2 를 끝냈다"
    );
    assert!(ctx.hook_task_waits.is_empty());

    register(&ctx, 2, "p#2");
    expire_overdue_hook_waits(&ctx, 5000);
    assert!(matches!(state(&ctx), TaskState::Failed { .. }));
}

#[test]
fn a_restart_releases_consumers_left_waiting_after_the_completion_write() {
    let (_td, ctx) = ctx();
    with_store(&ctx, |s| {
        let spec: TaskGraphSpec = serde_json::from_value(serde_json::json!({
            "contract_version": 2,
            "tasks": [
                {"id": "p",
                 "command": {"kind": "custom", "ipc_method": "system.ping", "params": {}},
                 "output_schema": {"type": "object", "fields": {}}},
                {"id": "c", "depends_on": ["p"],
                 "command": {"kind": "custom", "ipc_method": "system.ping", "params": {}}}]
        }))
        .unwrap();
        s.submit_graph(1, spec, 0).unwrap();
        // 완료 레코드는 썼지만 하류 반영 전에 멈춘 상태를 그대로 심는다.
        let mut p = s.get(1, &"p".into()).unwrap().unwrap();
        p.state = TaskState::Succeeded;
        p.finished_at = Some(1);
        s.put(&p).unwrap();
    });
    assert_eq!(state_of(&ctx, "c"), TaskState::Waiting);
    purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(state_of(&ctx, "c"), TaskState::Ready);
}

/// 실패한 `p#1` 을 재시도해 `p#2` 가 Running 인 상태를 만든다.
fn retried_to_second_attempt(ctx: &RunnerContext) {
    with_store(ctx, |s| {
        let spec: TaskGraphSpec = serde_json::from_value(serde_json::json!({
            "contract_version": 2,
            "tasks": [{"id": "p",
                "command": {"kind": "custom", "ipc_method": "system.ping", "params": {}},
                "output_schema": {"type": "object", "fields": {}}}]
        }))
        .unwrap();
        s.submit_graph(1, spec, 0).unwrap();
        let p = "p".to_string();
        s.set_state(1, &p, TaskState::Running, 1).unwrap();
        s.complete(1, &p, Completion::failed(None, "boom".into()), 2)
            .unwrap();
        s.retry(1, &p, false, 3).unwrap();
        s.set_state(1, &p, TaskState::Running, 4).unwrap();
    });
}

/// 죽은 pid 의 shell handle 을 `attempt` 회차로 저장한다. 보통 존재하지 않는 큰 PID 를 쓴다.
fn put_dead_handle(ctx: &RunnerContext, attempt: &str) {
    let mut v = serde_json::json!({"kind": "shell_process", "data": {"pid": 0xFFFF_FFFEu32}});
    v[HANDLE_ATTEMPT_FIELD] = attempt.into();
    put(ctx, &handle_key("p"), v);
}

fn put(ctx: &RunnerContext, key: &str, v: serde_json::Value) {
    ctx.with_store(|mem| {
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            key,
            &MemoryValue::Json(v),
            &PutOpts::default(),
        )
        .unwrap();
    });
}

fn handle_left(ctx: &RunnerContext) -> bool {
    ctx.with_store(|mem| mem.get(&Scope::Workspace(1), &handle_key("p")).unwrap())
        .is_some()
}

#[test]
fn a_late_restart_report_from_an_earlier_attempt_leaves_the_retried_run_alone() {
    let (_td, ctx) = ctx();
    retried_to_second_attempt(&ctx);

    // 죽은 pid: 옛 회차의 handle 은 지우되 p#2 를 끝내지 않는다.
    put_dead_handle(&ctx, "p#1");
    purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(
        state(&ctx),
        TaskState::Running,
        "p#1 의 복구 보고가 p#2 를 끝냈다"
    );
    assert!(!handle_left(&ctx));

    // 저장된 실행 결과: 같은 규칙.
    put_dead_handle(&ctx, "p#1");
    put(
        &ctx,
        &run_result_key("p"),
        serde_json::json!({"kind": "failed", "error": "old run"}),
    );
    purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(
        state(&ctx),
        TaskState::Running,
        "p#1 의 저장 결과가 p#2 를 끝냈다"
    );
    assert!(!handle_left(&ctx));

    // 지금 회차의 handle 은 그대로 보고한다.
    put_dead_handle(&ctx, "p#2");
    purge_and_reload_on_restart(&ctx, 1);
    assert!(matches!(state(&ctx), TaskState::Unknown { .. }));
    let attempt = with_store(&ctx, |s| s.get(1, &"p".into()).unwrap().unwrap().attempt);
    assert_eq!(attempt.expect("attempt").id, "p#2");
}
