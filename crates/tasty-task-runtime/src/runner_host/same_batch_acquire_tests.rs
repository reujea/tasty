//! 같은 writer 묶음에 들어간 두 러너의 permit 획득. 뒤 요청은 앞 요청의 반영을 보고 판정하므로 크기 1
//! semaphore 는 한쪽만 얻는다. 운영과 같은 writer 와 `try_acquire_semaphore` 경로를 쓴다.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::{OnFailure, SemaphoreStore, Task, TaskCommand, TaskState};
use tasty_memory::writer::{ChangeFeed, MemoryAccess, MemoryRequest, WriterStore};
use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryStore};
use tasty_utils::batch_writer::{Limits, SystemClock};

use super::{HostExecutor, RunnerContext};

const WAIT: Duration = Duration::from_secs(10);

fn ctx_on(memory: MemoryAccess) -> RunnerContext {
    RunnerContext {
        scope_stopping: Arc::new(AtomicBool::new(false)),
        memory,
        agent_seq: Arc::new(AtomicU64::new(0)),
        host_ipc: Arc::new(OnceLock::new()),
        task_waker_hub: Arc::new(crate::task_waker::TaskWakerHub::new()),
        hook_task_waits: Arc::new(crate::hook_wait::HookTaskWaits::new()),
        agent_turns: Default::default(),
        completion: Arc::new(crate::completion::fixture::Resolver::default()),
        report_limits: Default::default(),
        host_dispatch_timeout: crate::runner_host::HOST_DISPATCH_TIMEOUT,
    }
}

fn task(id: &str) -> Task {
    Task {
        id: id.to_string(),
        workspace_id: 1,
        name: id.into(),
        command: TaskCommand::Run {
            command: vec!["true".into()],
            workspace_id: 1,
            cwd: None,
        },
        state: TaskState::Ready,
        depends_on: vec![],
        on_failure: OnFailure::Abort,
        metadata: serde_json::json!({"semaphore": {"name": "gpu"}}),
        reserved_for_fallback: false,
        contract: None,
        typed_result: None,
        graph_id: None,
        input_snapshot: None,
        accepted: None,
        report_token: None,
        attempt: None,
        route: None,
        skip: None,
        result: None,
        created_at: 0,
        started_at: None,
        finished_at: None,
    }
}

type Gate = Arc<(Mutex<bool>, Condvar)>;

fn open(gate: &Gate) {
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
}

#[test]
fn two_acquires_in_one_batch_grant_a_single_permit_once() {
    let store: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(MemoryStore::open_in_memory().unwrap()));
    {
        let mut guard = store.lock().unwrap();
        SemaphoreStore::new(&mut *guard, HOST_OWNER)
            .create(1, "gpu", 1, 1000)
            .unwrap();
    }
    let changes = Arc::new(ChangeFeed::default());
    let mut writer = tasty_memory::writer::spawn(
        WriterStore::shared(store.clone()),
        changes.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let handle = writer.handle();
    let access = MemoryAccess::writer(handle.clone(), changes);

    // 앞 묶음을 실행 중에 붙잡아 두 획득이 그 뒤 한 묶음으로 모이게 한다.
    let gate: Gate = Arc::new((Mutex::new(false), Condvar::new()));
    let entered = Arc::new(AtomicBool::new(false));
    let (g, e) = (gate.clone(), entered.clone());
    let mut blocker = access
        .submit(MemoryRequest::unit(0, move |_| {
            e.store(true, Ordering::SeqCst);
            let mut opened = g.0.lock().unwrap();
            while !*opened {
                opened = g.1.wait(opened).unwrap();
            }
            Ok(())
        }))
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while !entered.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "the blocking batch did not start"
        );
        std::thread::sleep(Duration::from_millis(1));
    }

    let racers: Vec<_> = ["t-a", "t-b"]
        .into_iter()
        .map(|id| {
            let ctx = ctx_on(access.clone());
            std::thread::spawn(move || {
                let mut exec = HostExecutor::new(ctx);
                exec.try_acquire_semaphore(&task(id))
            })
        })
        .collect();
    while handle.stats().admitted < 3 {
        assert!(Instant::now() < deadline, "the acquires were not admitted");
        std::thread::sleep(Duration::from_millis(1));
    }
    open(&gate);
    assert!(blocker.wait_timeout(WAIT).is_some());
    let got: Vec<Option<bool>> = racers
        .into_iter()
        .map(|t| t.join().unwrap().expect("acquire answered"))
        .collect();

    let stats = handle.stats();
    assert_eq!(
        stats.batches, 2,
        "the acquires did not share one batch: {stats:?}"
    );
    assert_eq!(stats.batch_size_max, 2);
    assert_eq!(
        got.iter().filter(|g| **g == Some(true)).count(),
        1,
        "a single permit went to {got:?}"
    );
    assert_eq!(
        got.iter().filter(|g| **g == Some(false)).count(),
        1,
        "{got:?}"
    );
    {
        let mut guard = store.lock().unwrap();
        let holders = SemaphoreStore::new(&mut *guard, HOST_OWNER)
            .get(1, "gpu")
            .unwrap()
            .unwrap()
            .holders;
        assert_eq!(holders.len(), 1);
    }

    writer.begin_shutdown();
    while writer.poll_shutdown() != 0 {
        assert!(Instant::now() < deadline + WAIT, "writer did not join");
        std::thread::sleep(Duration::from_millis(1));
    }
}
