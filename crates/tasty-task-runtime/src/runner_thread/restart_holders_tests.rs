//! 재시작 뒤 Run 의 점유: 프로세스가 살아 있으면 유지하고, 끝난 것을 확인한 뒤 반환한다.
//! 감시할 Run 이 없는 점유는 이전처럼 부팅 정리가 반환하고 실패로 끝낸다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use tasty_agent::runner::TaskExecutor;
use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{
    LeaseMode, LeaseStore, OnFailure, SemaphoreStore, TaskCommand, TaskState, TaskStore,
};
use tasty_memory::{HOST_OWNER, MemoryStore, MemoryValue, PutOpts, Scope};

use super::super::purge_and_reload_on_restart;
use crate::runner_host::{HostExecutor, RunnerContext, handle_key, run_result_key};

const DEAD_PID: u32 = 0xFFFF_FFFE;

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

/// Running run task 하나. `metadata` 로 점유를 정하고 그 점유를 실제로 얻어 둔다.
fn running_holder(ctx: &RunnerContext, metadata: serde_json::Value) -> String {
    ctx.with_store(|mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
        let t = store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "semhold".into(),
                command: TaskCommand::Run {
                    command: vec!["sleep".into(), "40".into()],
                    workspace_id: 1,
                    cwd: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata,
                now_ms: 1000,
            })
            .unwrap();
        store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
        t.id
    })
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

fn put_run_handle(ctx: &RunnerContext, task_id: &str, pid: u32) {
    put(
        ctx,
        &handle_key(task_id),
        serde_json::json!({"kind": "shell_process", "data": {"pid": pid}}),
    );
}

fn state_of(ctx: &RunnerContext, id: &str) -> TaskState {
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &id.to_string())
            .unwrap()
            .unwrap()
            .state
    })
}

fn gpu_with_one_permit(ctx: &RunnerContext, holder: &str) {
    ctx.with_store(|mem| {
        let mut sem = SemaphoreStore::new(mem, HOST_OWNER);
        sem.create(1, "gpu", 1, 1000).unwrap();
        assert!(sem.acquire(1, "gpu", holder, None, 1000).unwrap().acquired);
    });
}

fn gpu_holders(ctx: &RunnerContext) -> Vec<String> {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .get(1, "gpu")
            .unwrap()
            .unwrap()
            .holders
            .into_iter()
            .map(|h| h.id)
            .collect()
    })
}

fn another_task_gets_gpu(ctx: &RunnerContext) -> bool {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .acquire(1, "gpu", "sem2", None, 2000)
            .unwrap()
            .acquired
    })
}

/// 리뷰 재현: permit 1 자원을 쥔 run 이 재시작 뒤에도 살아 있으면 다른 task 가 그 permit 을
/// 얻으면 안 된다. 프로세스가 끝나 결과를 회수할 수 없으면 결과 불명이 되고 그때 반환한다.
#[test]
fn a_live_run_keeps_its_permit_until_it_ends_after_a_restart() {
    let (_td, ctx) = ctx();
    let id = running_holder(&ctx, serde_json::json!({"semaphore": {"name": "gpu"}}));
    gpu_with_one_permit(&ctx, &id);
    put_run_handle(&ctx, &id, std::process::id());

    // 부팅 정리와 러너 시작 정리 둘 다 같은 판정을 한다.
    for _ in 0..2 {
        let restored = purge_and_reload_on_restart(&ctx, 1);
        assert_eq!(restored.len(), 1, "살아 있는 Run 을 복원한다");
        assert_eq!(state_of(&ctx, &id), TaskState::Running);
        assert_eq!(gpu_holders(&ctx), vec![id.clone()]);
    }
    assert!(
        !another_task_gets_gpu(&ctx),
        "살아 있는 Run 의 permit 을 넘겼다"
    );

    // 러너가 감시하기 전에 프로세스가 끝났고 저장된 종료 결과가 없다.
    put_run_handle(&ctx, &id, DEAD_PID);
    assert!(purge_and_reload_on_restart(&ctx, 1).is_empty());
    assert!(matches!(state_of(&ctx, &id), TaskState::Unknown { .. }));
    assert!(
        gpu_holders(&ctx).is_empty(),
        "끝난 Run 의 permit 을 반환한다"
    );
    assert!(another_task_gets_gpu(&ctx));
}

#[test]
fn an_ended_run_with_a_saved_result_finishes_and_returns_its_permit() {
    let (_td, ctx) = ctx();
    let id = running_holder(&ctx, serde_json::json!({"semaphore": {"name": "gpu"}}));
    gpu_with_one_permit(&ctx, &id);
    put_run_handle(&ctx, &id, DEAD_PID);
    put(
        &ctx,
        &run_result_key(&id),
        serde_json::json!({"kind": "done", "exit_code": 0}),
    );

    purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(state_of(&ctx, &id), TaskState::Succeeded);
    assert!(gpu_holders(&ctx).is_empty());
}

/// 감시할 Run handle 이 없으면(다른 dispatch 종류, handle 없음) 이전처럼 반환하고 실패로 끝낸다.
#[test]
fn a_holder_without_a_run_to_watch_is_still_failed_and_released() {
    let (_td, ctx) = ctx();
    let id = running_holder(&ctx, serde_json::json!({"semaphore": {"name": "gpu"}}));
    gpu_with_one_permit(&ctx, &id);

    purge_and_reload_on_restart(&ctx, 1);
    assert!(matches!(state_of(&ctx, &id), TaskState::Failed { error } if error == "host restart"));
    assert!(gpu_holders(&ctx).is_empty());
}

/// lease 도 같다. 살아 있는 동안 TTL 이 지나 다른 holder 가 그 자원을 얻었다면, 끝난 Run 의
/// 반환은 그 holder 의 lease 를 건드리지 않는다.
#[test]
fn a_restored_run_returns_only_a_lease_it_still_holds() {
    let (_td, ctx) = ctx();
    let id = running_holder(
        &ctx,
        serde_json::json!({"lease": {"resource": "db", "ttl_ms": 1}}),
    );
    let lease_holder = |ctx: &RunnerContext| {
        ctx.with_store(|mem| {
            LeaseStore::new(mem, HOST_OWNER)
                .get(1, "db")
                .unwrap()
                .map(|l| l.holder)
        })
    };
    ctx.with_store(|mem| {
        let acquired = LeaseStore::new(mem, HOST_OWNER)
            .acquire(1, "db", &id, None, LeaseMode::Fail, 1000)
            .unwrap()
            .acquired;
        assert!(acquired);
    });
    put_run_handle(&ctx, &id, std::process::id());
    purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(state_of(&ctx, &id), TaskState::Running);
    assert_eq!(lease_holder(&ctx), Some(id.clone()));

    // TTL 이 지나 다른 holder 가 얻은 경우(자원의 TTL 은 재시작과 무관하게 적용된다).
    ctx.with_store(|mem| {
        let mut leases = LeaseStore::new(mem, HOST_OWNER);
        leases.release(1, "db", &id).unwrap();
        let acquired = leases
            .acquire(1, "db", "other", None, LeaseMode::Fail, 2000)
            .unwrap()
            .acquired;
        assert!(acquired);
    });
    put_run_handle(&ctx, &id, DEAD_PID);
    purge_and_reload_on_restart(&ctx, 1);
    assert!(matches!(state_of(&ctx, &id), TaskState::Unknown { .. }));
    assert_eq!(lease_holder(&ctx), Some("other".to_string()));
}

/// 러너가 복원한 Run 을 감시하다 끝나면 executor 가 넘겨받은 permit 을 반환한다.
#[test]
fn the_runner_returns_the_permit_of_a_run_it_adopted() {
    let (_td, ctx) = ctx();
    let id = running_holder(&ctx, serde_json::json!({"semaphore": {"name": "gpu"}}));
    gpu_with_one_permit(&ctx, &id);
    let task = ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &id)
            .unwrap()
            .unwrap()
    });
    let mut exec = HostExecutor::new(ctx.clone());
    exec.adopt_restored_run(1, &task);
    exec.release_permit(&id);
    assert!(gpu_holders(&ctx).is_empty());
}

fn cancel(ctx: &RunnerContext, id: &str) {
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .set_state(1, &id.to_string(), TaskState::Cancelled, 3000)
            .unwrap();
    });
}

/// 점유(semaphore·lease)를 쥔 Running run 하나. 프로세스는 살아 있다.
fn running_run_with_holdings(ctx: &RunnerContext) -> String {
    let id = running_holder(
        ctx,
        serde_json::json!({"semaphore": {"name": "gpu"}, "lease": {"resource": "db"}}),
    );
    gpu_with_one_permit(ctx, &id);
    ctx.with_store(|mem| {
        let acquired = LeaseStore::new(mem, HOST_OWNER)
            .acquire(1, "db", &id, None, LeaseMode::Fail, 1000)
            .unwrap()
            .acquired;
        assert!(acquired);
    });
    put_run_handle(ctx, &id, std::process::id());
    id
}

fn lease_holder(ctx: &RunnerContext) -> Option<String> {
    ctx.with_store(|mem| {
        LeaseStore::new(mem, HOST_OWNER)
            .get(1, "db")
            .unwrap()
            .map(|l| l.holder)
    })
}

fn handle_left(ctx: &RunnerContext, id: &str) -> bool {
    ctx.with_store(|mem| {
        mem.get(&Scope::Workspace(1), &handle_key(id))
            .unwrap()
            .is_some()
    })
}

/// 재시작 뒤 러너를 켜기 전에 취소한 run: 다음 부팅 정리가 남은 점유를 반환한다.
#[test]
fn a_run_cancelled_while_no_runner_watched_it_returns_its_holdings_on_restart() {
    let (_td, ctx) = ctx();
    let id = running_run_with_holdings(&ctx);
    // 재시작: 살아 있는 Run 이라 점유를 유지한다.
    super::super::purge_stale_agent_state_on_boot(&ctx, &[1]);
    assert_eq!(gpu_holders(&ctx), vec![id.clone()]);

    cancel(&ctx, &id);
    super::super::purge_stale_agent_state_on_boot(&ctx, &[1]);
    assert!(
        gpu_holders(&ctx).is_empty(),
        "취소한 run 의 permit 이 남았다"
    );
    assert_eq!(lease_holder(&ctx), None, "취소한 run 의 lease 가 남았다");
    assert!(!handle_left(&ctx, &id));
    assert!(another_task_gets_gpu(&ctx));
}

/// 러너를 멈춘 동안 취소한 run: 러너를 다시 켤 때 정리가 남은 점유를 반환한다.
#[test]
fn a_run_cancelled_while_the_runner_was_stopped_returns_its_holdings_on_start() {
    let (_td, ctx) = ctx();
    let id = running_run_with_holdings(&ctx);
    cancel(&ctx, &id);

    let registry = crate::runner_thread::RunnerRegistry::new();
    assert!(registry.start(ctx.clone(), 1));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !gpu_holders(&ctx).is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    registry.stop(1);
    assert!(
        gpu_holders(&ctx).is_empty(),
        "취소한 run 의 permit 이 남았다"
    );
    assert_eq!(lease_holder(&ctx), None, "취소한 run 의 lease 가 남았다");
    assert!(!handle_left(&ctx, &id));
}
