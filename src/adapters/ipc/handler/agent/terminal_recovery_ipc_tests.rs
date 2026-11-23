//! Real task IPC adapters keep the unknown response while notifying confirmed terminal state.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::json;
use tasty_agent::{Task, TaskState, TaskStore};
use tasty_ipc::caller::CallerContext;
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::{HOST_OWNER, MemoryStorage};
use tasty_task_runtime::event_feed::AgentEvent;
use tasty_task_runtime::task_waker::{AwaitOutcome, TerminalSnapshot};

#[test]
fn ipc_cancel_commit_without_response_wakes_timeout_zero_and_does_not_claim_success() {
    exercise(false);
}

#[test]
fn queued_ipc_cancel_recovers_after_deferred_unknown_response() {
    exercise(true);
}

fn exercise(queued: bool) {
    let _home = crate::test_support::TastyHomeGuard::new();
    let faults = Arc::new(Faults::default().writing("tasty.agent.task."));
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    let mut core = if queued {
        super::super::deferred::tests::writer_core_with(memory.clone(), usize::MAX)
    } else {
        super::super::cli_entry_tests::test_core_builder()
            .with_memory(memory.clone())
            .build()
            .unwrap()
    };
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let task: Task = serde_json::from_value(json!({
        "id":"terminal-ipc", "workspace_id":1, "name":"isolated IPC", "created_at":0,
        "command":{"kind":"custom", "ipc_method":"system.ping", "params":{}},
        "state":{"kind":"running"}, "on_failure":{"kind":"abort"}
    }))
    .unwrap();
    {
        let mut guard = memory.lock().unwrap();
        TaskStore::new(&mut *guard, HOST_OWNER, &AtomicU64::new(0))
            .put(&task)
            .unwrap();
    }
    let hub = engine.task_scope.waker_hub().clone();
    let cleanup = hub.clone();
    let running = task.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = hub.await_terminal(1, &running.id, Some(0), || {
            ready_tx.send(()).unwrap();
            Some(TerminalSnapshot::of(&running, Some(1)))
        });
        drop(done_tx.send(outcome));
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    Faults::arm(&faults.units_after_writing, 1);
    let params = json!({"workspace_id":1,"id":task.id});
    let response = cancel(&core, &mut engine, &params, queued);
    assert_eq!(faults.units_after_writing.load(Ordering::SeqCst), 0);
    assert!(
        response.result.is_none(),
        "unknown commit must not return success: {response:?}"
    );
    let error = response.error.expect("unsettled IPC error");
    assert_eq!(
        error
            .data
            .as_ref()
            .and_then(|d| d.get("outcome"))
            .and_then(|v| v.as_str()),
        Some("unknown")
    );
    match done_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(AwaitOutcome::Terminal(s)) => assert_eq!(s.state, TaskState::Cancelled),
        other => {
            cleanup.fire(1, &task.id, TerminalSnapshot::of(&task, None));
            panic!("IPC recovery did not wake timeout-zero waiter: {other:?}");
        }
    }
    let first = engine.task_scope.event_queue().take_pending().0;
    assert_eq!(
        first
            .iter()
            .filter(|e| matches!(
                e,
                AgentEvent::TaskFinished {
                    terminal_id: Some(_),
                    ..
                }
            ))
            .count(),
        1
    );
    let _response = cancel(&core, &mut engine, &params, queued);
    assert!(engine.task_scope.event_queue().take_pending().0.is_empty());
    if queued {
        // 종료 poll 은 끝난 writer worker 를 join 만 하므로 기한 뒤 관측 읽기에서도 부른다.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        assert!(tasty_utils::test_wait::holds_by_every(
            deadline,
            Duration::from_millis(1),
            |_| core.poll_memory_writer_shutdown() == 0
        ));
    }
}

fn cancel(
    core: &crate::app::services::AppServices,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    params: &serde_json::Value,
    queued: bool,
) -> tasty_ipc::protocol::JsonRpcResponse {
    let (release, blocked) = mpsc::channel();
    let _gate = queued.then(|| {
        core.memory_access()
            .submit(tasty_memory::writer::MemoryRequest::read(0, move |_| {
                blocked.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(())
            }))
            .unwrap()
    });
    let response =
        super::handle_task_cancel(core, engine, &CallerContext::local(), json!(1), params);
    if !queued {
        return response;
    }
    release.send(()).unwrap();
    barrier(core);
    let response = core.answer_parked_for_test();
    barrier(core);
    core.memory_access().reap_detached();
    response
}

fn barrier(core: &crate::app::services::AppServices) {
    let mut ticket = core
        .memory_access()
        .submit(tasty_memory::writer::MemoryRequest::read(0, |_| Ok(())))
        .unwrap();
    let completion = ticket
        .wait_timeout(Duration::from_secs(5))
        .expect("strong read barrier");
    tasty_memory::writer::settle(completion).expect("read after the unknown write");
}
