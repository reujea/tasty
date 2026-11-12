//! `agent.task_run` stop 은 메인 스레드에서 러너 join 을 기다리지 않는다. 러너가 메인 스레드의 host
//! 응답을 기다리는 중이어도 바로 응답하고, 러너가 끝나 회수되면 status 가 stopped 로 바뀐다.
//! 러너의 응답 대기 상한을 길게 두어, 첫 dispatch 는 시험이 쥔 장벽을 풀 때까지 끝나지 않는다.
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{OnFailure, TaskCommand, TaskStore};
use tasty_ipc::caller::CallerContext;
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::IpcCommand;
use tasty_memory::{HOST_OWNER, MemoryStorage};

use crate::app::services::AppServices;
use crate::runtime::engine_access::EngineMut;

const READY_TASKS: usize = 4;
/// 핸들러가 돌아오지 않아도 장벽을 푸는 상한. 메인 join 이 되살아나면 이 상한으로 풀린다.
const HOLD_LIMIT: Duration = Duration::from_secs(3);
/// 러너의 응답 대기 상한. 시험이 쥔 요청이 이 상한으로 먼저 풀리지 않게 길게 둔다.
const HELD_DISPATCH_TIMEOUT: Duration = Duration::from_secs(60);

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

/// 첫 요청을 받아 쥐는 보조 스레드. 러너를 시작하기 전에 설치해 요청이 나가는 순간부터 쥔다.
struct Holder {
    /// 첫 요청을 쥐었다는 알림.
    held: mpsc::Receiver<()>,
    /// stop 핸들러를 부르기 직전에 보낸다. 이때부터 [`HOLD_LIMIT`] 을 잰다.
    stopping: mpsc::Sender<()>,
    /// 핸들러가 돌아온 뒤 보낸다. 받으면 쥔 요청에 답한다.
    release: mpsc::Sender<()>,
    /// 받은 요청 채널과, 해제 신호 없이 상한으로 풀었는지를 돌려준다.
    thread: JoinHandle<(mpsc::Receiver<IpcCommand>, bool)>,
}

/// 보조 스레드와 시험 사이의 신호가 끊긴 경우. 단정은 시험 스레드가 하므로 기록만 남긴다.
fn note_if_lost<T, E>(result: Result<T, E>, what: &str) {
    if result.is_err() {
        tracing::warn!("{what}");
    }
}

/// 쥐고 있던 요청에 성공으로 답해 장벽을 푼다.
fn answer_held(cmd: IpcCommand) {
    let id = cmd.request.id.clone().unwrap_or(Value::Null);
    note_if_lost(
        cmd.response_tx
            .send(JsonRpcResponse::success(id, json!({}))),
        "the runner stopped waiting for the held dispatch",
    );
}

fn hold_first(rx: mpsc::Receiver<IpcCommand>) -> Holder {
    let (held_tx, held) = mpsc::channel::<()>();
    let (stopping, stopping_rx) = mpsc::channel::<()>();
    let (release, release_rx) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let cmd = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("runner sent the first dispatch");
        note_if_lost(
            held_tx.send(()),
            "the test stopped waiting for the held dispatch",
        );
        note_if_lost(
            stopping_rx.recv_timeout(HELD_DISPATCH_TIMEOUT),
            "the test never sent stop",
        );
        let timed_out = release_rx.recv_timeout(HOLD_LIMIT).is_err();
        answer_held(cmd);
        (rx, timed_out)
    });
    Holder {
        held,
        stopping,
        release,
        thread,
    }
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
    core.tasks.set_host_dispatch_timeout(HELD_DISPATCH_TIMEOUT);
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

    let holder = hold_first(rx);
    assert_eq!(state(&task_run(&core, &mut engine, "start")), "running");
    holder
        .held
        .recv_timeout(Duration::from_secs(10))
        .expect("holder took the first dispatch");

    holder.stopping.send(()).expect("holder waits for stop");
    let stopped = task_run(&core, &mut engine, "stop");
    // 핸들러가 돌아온 뒤에 장벽을 푼다. 상한으로 풀렸다면 stop 이 러너를 기다린 것이다.
    let answered_before_release = holder.release.send(()).is_ok();
    let (rx, released_by_limit) = holder.thread.join().expect("holder thread");
    assert!(
        answered_before_release && !released_by_limit,
        "stop waited on the main thread until the held dispatch was released"
    );
    assert_eq!(state(&stopped), "stopping", "{stopped}");
    assert_eq!(stopped["running"], json!(false));

    // 메인 루프가 하는 회수. 장벽을 푼 뒤에는 남은 Ready 작업을 보내지 않고 끝난다.
    let deadline = Instant::now() + Duration::from_secs(10);
    while core.tasks.poll_runner_stops() != 0 {
        if let Ok(next) = rx.try_recv() {
            panic!(
                "runner dispatched {} after the stop request",
                next.request.method
            );
        }
        assert!(Instant::now() < deadline, "runner did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
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
        // 다시 시작한 러너도 같은 요청을 보낸다. 꺼내 버리면 러너는 응답 없이 바로 다음으로 간다.
        rx.try_iter().for_each(drop);
        assert!(Instant::now() < deadline, "restarted runner did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn stop_on_a_crashed_runner_answers_stopped() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
    let core = super::super::cli_entry_tests::test_core_builder()
        .with_memory(memory.clone())
        .build()
        .unwrap();
    // 러너 스레드가 첫 dispatch 를 보낸 직후 부르는 waker 가 panic 해 러너가 crash 한다.
    let (tx, _rx) = mpsc::channel::<IpcCommand>();
    core.set_host_ipc_injector(HostIpcInjector::new(
        tx,
        Arc::new(|| panic!("runner crash for the stop test")),
    ));
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    {
        let mut guard = memory.lock().unwrap();
        let seq = AtomicU64::new(0);
        let mut store = TaskStore::new(&mut *guard, HOST_OWNER, &seq);
        store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "spawn".into(),
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

    task_run(&core, &mut engine, "start");
    // crashed 표시는 스레드가 끝나기 전에 선다. 회수하지 않는 조회로 스레드 종료까지 본다.
    let deadline = Instant::now() + Duration::from_secs(10);
    while core.tasks.runner_worker_finished(1) != Some(true) {
        assert!(Instant::now() < deadline, "runner thread did not end");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(state(&task_run(&core, &mut engine, "status")), "crashed");

    let stopped = task_run(&core, &mut engine, "stop");
    assert_eq!(state(&stopped), "stopped", "{stopped}");
    assert_eq!(stopped["crashed"], json!(false), "{stopped}");
}
