//! `agent.task_run` stop 은 메인 스레드에서 러너 join 을 기다리지 않는다. 러너가 메인 스레드의 host
//! 응답을 기다리는 중이어도 바로 응답하고, 메인 루프의 회수 뒤 status 가 stopped 로 바뀐다.
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{OnFailure, TaskCommand, TaskStore};
use tasty_ipc::caller::CallerContext;
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::server::IpcCommand;
use tasty_memory::{HOST_OWNER, MemoryStorage};

use crate::app::services::AppServices;
use crate::runtime::engine_access::EngineMut;

const READY_TASKS: usize = 4;

fn task_run(core: &AppServices, engine: &mut EngineMut<'_>, action: &str) -> Value {
    let response = super::handle_task_run(
        core,
        engine,
        &CallerContext::local(),
        json!(1),
        &json!({ "workspace_id": 1, "action": action }),
    );
    response
        .result
        .unwrap_or_else(|| panic!("task_run {action}: {:?}", response.error))
}

fn state(result: &Value) -> &str {
    result.get("state").and_then(Value::as_str).unwrap_or("?")
}

#[test]
fn stop_answers_while_the_runner_waits_for_the_main_thread_and_status_reports_the_stop() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
    let core = super::super::cli_entry_tests::test_core_builder()
        .with_memory(memory.clone())
        .build()
        .unwrap();
    // 이 시험 스레드가 메인 스레드다. 핸들러를 부르는 동안에는 host 요청을 꺼내 처리하지 못한다.
    let (tx, rx) = mpsc::channel::<IpcCommand>();
    core.set_host_ipc_injector(HostIpcInjector::new(tx, Arc::new(|| {})));
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    {
        let mut guard = memory.lock().unwrap();
        let seq = AtomicU64::new(0);
        let mut store = TaskStore::new(&mut *guard, HOST_OWNER, &seq);
        for i in 0..READY_TASKS {
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: format!("spawn-{i}"),
                    command: TaskCommand::Custom {
                        ipc_method: "fake.spawn".into(),
                        params: json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
        }
    }

    assert_eq!(state(&task_run(&core, &mut engine, "start")), "running");
    let first = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("runner sent the first dispatch");

    let asked = Instant::now();
    let stopped = task_run(&core, &mut engine, "stop");
    let answered_after = asked.elapsed();
    assert!(
        answered_after < Duration::from_secs(1),
        "stop waited {answered_after:?} on the main thread"
    );
    assert_eq!(state(&stopped), "stopping", "{stopped}");
    assert_eq!(stopped["running"], json!(false));

    // 메인 루프가 하는 회수. 진행 중인 dispatch 하나의 응답 상한 안에 끝난다.
    let deadline = Instant::now() + Duration::from_secs(10);
    while core.tasks.poll_runner_stops() != 0 {
        assert!(Instant::now() < deadline, "runner did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(first);
    assert_eq!(state(&task_run(&core, &mut engine, "status")), "stopped");
    assert_eq!(
        rx.try_iter().count(),
        0,
        "runner dispatched more tasks after the stop request"
    );

    assert_eq!(state(&task_run(&core, &mut engine, "start")), "running");
    drop(core.tasks.request_stop_workspace(engine.task_scope, 1));
    let deadline = Instant::now() + Duration::from_secs(10);
    while core.tasks.poll_runner_stops() != 0 {
        // 다시 시작한 러너도 같은 요청을 보낸다. 쌓이지 않게 꺼내 버린다.
        rx.try_iter().for_each(drop);
        assert!(Instant::now() < deadline, "restarted runner did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
}
