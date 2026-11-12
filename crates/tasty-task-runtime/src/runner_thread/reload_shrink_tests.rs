//! 재시작 복구 — 기록하지 못할 만큼 큰 보고는 러너와 같이 한 번 줄여 기록한다.

use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;

use tasty_agent::task::TaskCreateOpts;
use tasty_agent::task::attempt::SHRUNK_ERROR_LIMIT;
use tasty_agent::{OnFailure, TaskCommand};
use tasty_memory::MemoryStore;

use super::*;

fn fresh_ctx() -> (tempfile::TempDir, RunnerContext) {
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

fn store_op<R>(ctx: &RunnerContext, f: impl FnOnce(&mut TaskStore) -> R) -> R {
    ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        f(&mut store)
    })
}

/// 줄바꿈 없는 긴 사유의 재시작 복구 보고는 레코드 상한을 넘는다. 줄인 보고는 사유가 상한
/// 안이라 한 번에 기록되고 handle 을 지워도 된다고 답한다.
#[test]
fn a_reload_failure_with_a_long_single_line_reason_is_stored_after_one_shrink() {
    let (_td, ctx) = fresh_ctx();
    let id = store_op(&ctx, |s| {
        let t = s
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "t".into(),
                command: TaskCommand::Run {
                    command: vec!["true".into()],
                    workspace_id: 1,
                    cwd: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: serde_json::Value::Null,
                now_ms: 1000,
            })
            .unwrap();
        s.set_state(1, &t.id, TaskState::Running, 1001).unwrap();
        t.id
    });
    let report = Completion::failed(None, "e".repeat(1100 * 1024));

    assert!(record_reload_completion(&ctx, 1, &id, report, 1002));

    let t = store_op(&ctx, |s| s.get(1, &id).unwrap().expect("task"));
    let TaskState::Failed { error } = &t.state else {
        panic!("{:?}", t.state);
    };
    assert!(error.len() <= SHRUNK_ERROR_LIMIT, "{}", error.len());
    assert!(
        error.starts_with("eee")
            && error.contains(
                "...(truncated) (the full result could not be stored: memory: value too large"
            ),
        "{error}"
    );
}
