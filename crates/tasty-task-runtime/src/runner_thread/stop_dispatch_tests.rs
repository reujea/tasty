//! 메인 스레드가 host 요청을 처리하지 못하는 동안 정지를 요청하면, 러너는 진행 중인 dispatch 하나만
//! 끝내고 멈춘다. 남은 Ready Custom 작업마다 응답 상한을 기다리지 않는다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{OnFailure, TaskCommand, TaskStore};
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::server::IpcCommand;
use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryStore};

use super::RunnerStopObservation;
use crate::runner_host::HOST_DISPATCH_TIMEOUT;
use crate::{TaskScope, TaskService};

const READY_TASKS: usize = 4;

fn custom_tasks(mem: &Arc<Mutex<dyn MemoryStorage>>, seq: &AtomicU64) {
    let mut guard = mem.lock().unwrap();
    let mut store = TaskStore::new(&mut *guard, HOST_OWNER, seq);
    for i in 0..READY_TASKS {
        store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: format!("spawn-{i}"),
                command: TaskCommand::Custom {
                    ipc_method: "fake.spawn".into(),
                    params: serde_json::json!({}),
                    poll: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: serde_json::Value::Null,
                now_ms: 1000,
            })
            .unwrap();
    }
}

#[test]
fn a_stop_during_an_unanswered_dispatch_does_not_wait_for_the_remaining_ready_tasks() {
    let td = tempfile::tempdir().unwrap();
    let mem: Arc<Mutex<dyn MemoryStorage>> = Arc::new(Mutex::new(
        MemoryStore::open(&td.path().join("mem.db")).unwrap(),
    ));
    let host_ipc = Arc::new(OnceLock::new());
    // 메인 스레드가 막혀 요청을 꺼내지 못하는 상태: 받은 요청을 쥐고 답하지 않는다.
    let (tx, rx) = mpsc::channel::<IpcCommand>();
    host_ipc
        .set(HostIpcInjector::new(tx, Arc::new(|| {})))
        .ok()
        .expect("set once");
    let svc = TaskService::new(
        tasty_memory::writer::MemoryAccess::inline(mem.clone()),
        host_ipc,
        Arc::new(crate::completion::fixture::Resolver::default()),
    );
    let scope = TaskScope::new(svc.runner_registry().clone());
    custom_tasks(&mem, scope.agent_seq());

    assert!(svc.runner_start(&scope, 1));
    let first = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("runner sent the first dispatch");
    let requested = Instant::now();
    let receipt = svc.request_stop_workspace(&scope, 1);
    let bound = HOST_DISPATCH_TIMEOUT * (READY_TASKS as u32) + Duration::from_secs(10);
    while receipt.poll() == RunnerStopObservation::Waiting {
        assert!(requested.elapsed() < bound, "runner never stopped");
        std::thread::sleep(Duration::from_millis(10));
    }
    let stopped_after = requested.elapsed();
    drop(first);

    let later: Vec<_> = rx.try_iter().collect();
    assert!(
        later.is_empty(),
        "runner dispatched {} more task(s) after the stop request",
        later.len()
    );
    assert!(
        stopped_after < HOST_DISPATCH_TIMEOUT + Duration::from_secs(2),
        "stop took {stopped_after:?}; it should end with the one dispatch in progress"
    );
}
