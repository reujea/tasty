//! 결과를 받지 못한 완료 보고가 실제로는 확정된 경우. 러너가 같은 보고를 다시 내 같은 보고로 받아들여지면
//! 후속 확인 읽기로 대기자를 깨우며 재전송도 사건을 중복하지 않는다. 결과 불명은 실제 저장소에서 주입한다.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tasty_agent::task::{Completion, TaskGraphSpec};
use tasty_agent::{TaskResult, TaskState, TaskStore};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::{HOST_OWNER, MemoryStorage};

use super::CompletionReports;
use crate::event_feed::{AgentEvent, AgentEventQueue};
use crate::runner_host::RunnerContext;
use crate::task_waker::{AwaitOutcome, TaskWakerHub, TerminalSnapshot};

fn setup() -> (Arc<Faults>, RunnerContext, Arc<AgentEventQueue>) {
    let faults = Arc::new(Faults::default().writing("tasty.agent.typed_task."));
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    let feed = Arc::new(AgentEventQueue::new());
    let ctx = RunnerContext {
        scope_stopping: Arc::new(AtomicBool::new(false)),
        memory: tasty_memory::writer::MemoryAccess::inline(memory),
        agent_seq: Arc::new(AtomicU64::new(0)),
        host_ipc: Arc::new(OnceLock::new()),
        task_waker_hub: Arc::new(TaskWakerHub::with_feed(feed.clone())),
        hook_task_waits: Arc::new(crate::hook_wait::HookTaskWaits::new()),
        agent_turns: Default::default(),
        completion: Arc::new(crate::completion::fixture::Resolver::default()),
        report_limits: Default::default(),
        host_dispatch_timeout: crate::runner_host::HOST_DISPATCH_TIMEOUT,
    };
    ctx.with_store(|mem| {
        let mut s = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
        let spec: TaskGraphSpec = serde_json::from_value(serde_json::json!({
            "contract_version": 2,
            "tasks": [{"id": "p",
                "command": {"kind": "custom", "ipc_method": "system.ping", "params": {}},
                "output_schema": {"type": "object", "fields": {}}}]
        }))
        .unwrap();
        s.submit_graph(1, spec, 0).unwrap();
        s.set_state(1, &"p".to_string(), TaskState::Running, 1)
            .unwrap();
    });
    (faults, ctx, feed)
}

fn state(ctx: &RunnerContext) -> TaskState {
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &"p".to_string())
            .unwrap()
            .unwrap()
            .state
    })
}

fn finished(feed: &AgentEventQueue) -> usize {
    feed.take_pending()
        .0
        .iter()
        .filter(|e| matches!(e, AgentEvent::TaskFinished { task_id, .. } if task_id == "p"))
        .count()
}

fn done() -> Completion {
    Completion::succeeded(
        Some("p#1".into()),
        TaskResult {
            exit_code: None,
            output: Some(serde_json::json!({})),
            error: None,
        },
    )
}

#[test]
fn a_completion_that_committed_without_an_answer_is_confirmed_before_resend() {
    let (faults, ctx, feed) = setup();
    // 대기자는 등록한 뒤 Running 을 읽고 기다린다. 등록을 마친 다음에 보고한다.
    let hub = ctx.task_waker_hub.clone();
    let (registered_tx, registered) = std::sync::mpsc::channel();
    let running = ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .get(1, &"p".to_string())
            .unwrap()
            .unwrap()
    });
    let waiter = std::thread::spawn(move || {
        hub.await_terminal(1, &"p".to_string(), Some(10_000), move || {
            registered_tx.send(()).unwrap();
            Some(TerminalSnapshot::of(&running, None))
        })
    });
    registered.recv().unwrap();
    let mut reports = CompletionReports::default();

    Faults::arm(&faults.units_after_writing, 1);
    assert!(reports.report(&ctx, 1, &"p".into(), done(), 10).is_err());
    assert_eq!(
        faults.units_after_writing.load(Ordering::SeqCst),
        0,
        "주입이 닿지 않았다"
    );
    assert_eq!(state(&ctx), TaskState::Succeeded, "보고가 확정되지 않았다");
    assert_eq!(
        finished(&feed),
        1,
        "후속 읽기로 확인한 종결을 복구해야 한다"
    );

    reports
        .report(&ctx, 1, &"p".into(), done(), 20)
        .expect("the resent report is the same report");
    assert_eq!(
        finished(&feed),
        0,
        "재전송이 종결을 중복해서 알리면 안 된다"
    );
    assert!(matches!(waiter.join().unwrap(), AwaitOutcome::Terminal(_)));

    // 답을 받은 뒤의 같은 보고는 다시 알리지 않는다.
    reports.report(&ctx, 1, &"p".into(), done(), 30).unwrap();
    assert_eq!(finished(&feed), 0);
}

#[test]
fn an_answered_completion_resent_by_the_runner_is_not_announced_twice() {
    let (_faults, ctx, feed) = setup();
    let mut reports = CompletionReports::default();
    reports.report(&ctx, 1, &"p".into(), done(), 10).unwrap();
    assert_eq!(finished(&feed), 1);
    reports.report(&ctx, 1, &"p".into(), done(), 20).unwrap();
    assert_eq!(finished(&feed), 0);
}
