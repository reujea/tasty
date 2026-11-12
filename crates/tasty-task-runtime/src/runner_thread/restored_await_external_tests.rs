//! 재시작 뒤 복원한 AwaitExternal은 hook 매핑 없이도 같은 프로세스에서 기한이 지나면
//! 재시작 없는 경로와 같은 시간 초과 실패로 끝난다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use tasty_agent::runner::DispatchHandle;
use tasty_agent::task::{Completion, TaskCreateOpts};
use tasty_agent::{OnFailure, Task, TaskCommand, TaskResult, TaskState, TaskStore};
use tasty_memory::{HOST_OWNER, MemoryStore, PutOpts, Scope};

use super::{expire_overdue_hook_waits, now_ms, purge_and_reload_on_restart};
use crate::runner_host::{RunnerContext, handle_key, handle_value};
use crate::task_waker::{AwaitOutcome, TaskWakerHub, TerminalSnapshot};

const TIMEOUT_ERROR: &str = "push completion strategy timed out waiting for external report";

fn ctx_with(
    hook_task_waits: Arc<crate::hook_wait::HookTaskWaits>,
) -> (tempfile::TempDir, RunnerContext) {
    let td = tempfile::tempdir().unwrap();
    let mem = MemoryStore::open(&td.path().join("mem.db")).unwrap();
    let ctx = RunnerContext {
        scope_stopping: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        memory: tasty_memory::writer::MemoryAccess::inline(Arc::new(Mutex::new(mem))),
        agent_seq: Arc::new(AtomicU64::new(0)),
        host_ipc: Arc::new(OnceLock::new()),
        task_waker_hub: Arc::new(TaskWakerHub::new()),
        hook_task_waits,
        agent_turns: Default::default(),
        completion: Arc::new(crate::completion::fixture::Resolver::default()),
        report_limits: Default::default(),
        host_dispatch_timeout: crate::runner_host::HOST_DISPATCH_TIMEOUT,
    };
    (td, ctx)
}

fn ctx() -> (tempfile::TempDir, RunnerContext) {
    ctx_with(Arc::new(crate::hook_wait::HookTaskWaits::new()))
}

fn task(ctx: &RunnerContext, id: &str) -> Task {
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &id.to_string())
            .unwrap()
            .unwrap()
    })
}

/// 실행 중인 push 대기 task와 재시작 전에 저장된 handle. 반환값은 (task id, 기한).
fn running_push_wait(ctx: &RunnerContext) -> (String, u64) {
    let id = ctx.with_store(|mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
        let t = store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "push".into(),
                command: TaskCommand::Custom {
                    ipc_method: "acme.start".into(),
                    params: serde_json::json!({}),
                    poll: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: serde_json::Value::Null,
                now_ms: 1000,
            })
            .unwrap();
        store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
        t.id
    });
    let attempt = task(ctx, &id).attempt.map(|a| a.id);
    let deadline = now_ms() + 60_000;
    let handle = DispatchHandle::AwaitExternal {
        wait_key: "7".into(),
        deadline_ms: deadline,
    };
    let value = handle_value(serde_json::to_value(&handle).unwrap(), attempt.as_deref());
    ctx.with_store(|mem| {
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            &handle_key(&id),
            &value,
            &PutOpts::default(),
        )
        .unwrap();
    });
    (id, deadline)
}

fn failed_error(t: &Task) -> String {
    match &t.state {
        TaskState::Failed { error } => error.clone(),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_restored_push_wait_times_out_in_the_same_process_and_wakes_its_waiter() {
    let (_td, ctx) = ctx();
    let (id, deadline) = running_push_wait(&ctx);

    // 부팅 정리와 수동 러너 시작이 모두 reload 한다. 두 번 해도 기한은 하나다.
    for _ in 0..2 {
        assert_eq!(purge_and_reload_on_restart(&ctx, 1).len(), 1);
    }
    assert_eq!(ctx.hook_task_waits.len(), 1);

    let hub = ctx.task_waker_hub.clone();
    let running = TerminalSnapshot::of(&task(&ctx, &id), None);
    let waited_id = id.clone();
    let waiter = std::thread::spawn(move || {
        hub.await_terminal(1, &waited_id, Some(10_000), || Some(running))
    });
    // 만료 전에 대기자 등록을 기다린다. 등록 뒤의 조회는 Running 스냅샷이라 종결은 만료 경로의
    // wake로만 전달된다.
    let registered_by = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while ctx.task_waker_hub.waiter_count(1, &id) == 0 {
        assert!(
            std::time::Instant::now() < registered_by,
            "대기자가 등록되지 않았다"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    expire_overdue_hook_waits(&ctx, deadline - 1);
    assert_eq!(
        task(&ctx, &id).state,
        TaskState::Running,
        "기한 전에 끝냈다"
    );

    expire_overdue_hook_waits(&ctx, deadline);
    let t = task(&ctx, &id);
    assert!(
        failed_error(&t).contains(TIMEOUT_ERROR),
        "재시작 없는 경로와 같은 문구여야 한다: {:?}",
        t.state
    );
    match waiter.join().unwrap() {
        AwaitOutcome::Terminal(snap) => {
            assert!(matches!(snap.state, TaskState::Failed { .. }), "{snap:?}")
        }
        other => panic!("task-await 가 종결을 받지 못했다: {other:?}"),
    }
    assert!(ctx.hook_task_waits.is_empty());
}

/// 같은 공유 대기를 보는 두 workspace 러너가 차례로 회수해도 한 번만 종결한다.
#[test]
fn two_runners_sharing_the_waits_finish_a_restored_wait_once() {
    let (_td, a) = ctx();
    let (id, deadline) = running_push_wait(&a);
    purge_and_reload_on_restart(&a, 1);
    let mut b = a.clone();
    b.task_waker_hub = Arc::new(TaskWakerHub::new());

    expire_overdue_hook_waits(&b, deadline);
    let first = task(&a, &id);
    failed_error(&first);
    let revision = a.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, a.agent_seq.as_ref())
            .revision(1, &id)
            .unwrap()
    });

    expire_overdue_hook_waits(&a, deadline + 1);
    let after = a.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, a.agent_seq.as_ref())
            .revision(1, &id)
            .unwrap()
    });
    assert_eq!(revision, after, "두 번째 러너가 같은 task 를 다시 기록했다");
    assert_eq!(task(&a, &id).finished_at, first.finished_at);
}

/// 기한 전에 외부 보고로 끝난 task 는 기한이 지나도 그 결과를 유지한다.
#[test]
fn an_external_report_before_the_deadline_is_kept() {
    let (_td, ctx) = ctx();
    let (id, deadline) = running_push_wait(&ctx);
    purge_and_reload_on_restart(&ctx, 1);

    let attempt = task(&ctx, &id).attempt.map(|a| a.id);
    let done = Completion::succeeded(
        attempt,
        TaskResult {
            exit_code: Some(0),
            output: None,
            error: None,
        },
    );
    ctx.complete_task(1, &id, done, deadline - 10).unwrap();
    assert_eq!(task(&ctx, &id).state, TaskState::Succeeded);

    expire_overdue_hook_waits(&ctx, deadline);
    assert_eq!(task(&ctx, &id).state, TaskState::Succeeded);
    assert!(ctx.hook_task_waits.is_empty());
}
