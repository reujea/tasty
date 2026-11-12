//! 러너가 첫 dispatch 의 host 응답을 기다리는 동안 정지를 요청하면, 그 dispatch 가 끝난 뒤 남은 Ready
//! Custom 작업을 보내지 않고 멈춘다. 응답 대기 상한을 길게 두어, 시험이 첫 요청을 쥐고 있는 동안
//! 러너는 그 dispatch 를 벗어나지 못한다. 시험이 답하면 바로 풀리므로 그 상한을 기다리지 않는다.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use tasty_ipc::protocol::JsonRpcResponse;

use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{OnFailure, TaskCommand, TaskStore};
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::server::IpcCommand;
use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryStore};

use super::RunnerStopObservation;
use crate::{TaskScope, TaskService};

const READY_TASKS: usize = 4;
/// 시험이 쥔 요청이 상한으로 풀리지 않게 하는 러너의 응답 대기 상한. 시험은 이보다 훨씬 먼저 답한다.
const HELD_DISPATCH_TIMEOUT: Duration = Duration::from_secs(60);

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

/// 쥐고 있던 요청에 성공으로 답해 장벽을 푼다.
fn release(cmd: IpcCommand) {
    let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
    cmd.response_tx
        .send(JsonRpcResponse::success(id, serde_json::json!({})))
        .expect("runner waits for the held dispatch");
}

#[test]
fn a_stop_during_a_held_dispatch_sends_none_of_the_remaining_ready_tasks() {
    let td = tempfile::tempdir().unwrap();
    let mem: Arc<Mutex<dyn MemoryStorage>> = Arc::new(Mutex::new(
        MemoryStore::open(&td.path().join("mem.db")).unwrap(),
    ));
    let host_ipc = Arc::new(OnceLock::new());
    // 메인 스레드 역할: 받은 요청을 시험이 쥐고, 풀 때까지 러너는 그 dispatch 에 머문다.
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
    svc.set_host_dispatch_timeout(HELD_DISPATCH_TIMEOUT);

    assert!(svc.runner_start(&scope, 1));
    let held = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("runner sent the first dispatch");
    let receipt = svc.request_stop_workspace(&scope, 1);
    assert_eq!(
        receipt.poll(),
        RunnerStopObservation::Waiting,
        "runner ended while its dispatch was held"
    );
    assert_eq!(svc.runner_status(&scope, 1).state(), "stopping");

    release(held);
    let deadline = Instant::now() + Duration::from_secs(10);
    while receipt.poll() == RunnerStopObservation::Waiting {
        if let Ok(next) = rx.try_recv() {
            panic!(
                "runner dispatched {} after the stop request",
                next.request.method
            );
        }
        assert!(Instant::now() < deadline, "runner did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
    let later: Vec<_> = rx.try_iter().collect();
    assert!(
        later.is_empty(),
        "runner dispatched {} more task(s) after the stop request",
        later.len()
    );
    assert_eq!(svc.runner_status(&scope, 1).state(), "stopped");
}
