//! permit 획득이 결과 불명으로 끝나면 그 회차에는 실행하지 않고 같은 holder 의 점유를 돌려준다.
//! 다음 회차가 다시 얻어 한 번만 실행한다. 저장 실패는 실제 저장소에서 주입한다.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::runner::{DispatchOutcome, TaskExecutor};
use tasty_agent::semaphore::SEMAPHORE_KEY_PREFIX;
use tasty_agent::{OnFailure, SemaphoreStore, Task, TaskCommand, TaskState};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::{HOST_OWNER, MemoryStorage};

use super::{HostExecutor, RunnerContext};

fn ctx_on(memory: Arc<Mutex<dyn MemoryStorage>>) -> RunnerContext {
    RunnerContext {
        scope_stopping: Arc::new(AtomicBool::new(false)),
        memory: tasty_memory::writer::MemoryAccess::inline(memory),
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

fn task(marker: &std::path::Path) -> Task {
    Task {
        id: "t-gpu".to_string(),
        workspace_id: 1,
        name: "gpu".into(),
        command: TaskCommand::Run {
            command: vec![
                "sh".into(),
                "-c".into(),
                format!("echo run >> {}", marker.display()),
            ],
            workspace_id: 1,
            cwd: None,
        },
        state: TaskState::Running,
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

fn holders(memory: &Arc<Mutex<dyn MemoryStorage>>) -> usize {
    let mut guard = memory.lock().unwrap();
    SemaphoreStore::new(&mut *guard, HOST_OWNER)
        .get(1, "gpu")
        .unwrap()
        .expect("semaphore exists")
        .holders
        .len()
}

fn runs(marker: &std::path::Path) -> usize {
    std::fs::read_to_string(marker)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[test]
fn an_unknown_permit_acquire_spawns_nothing_and_the_next_tick_runs_once() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("runs");
    let faults = Arc::new(Faults::default().writing(SEMAPHORE_KEY_PREFIX));
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    {
        let mut guard = memory.lock().unwrap();
        SemaphoreStore::new(&mut *guard, HOST_OWNER)
            .create(1, "gpu", 1, 1000)
            .unwrap();
    }
    let mut exec = HostExecutor::new(ctx_on(memory.clone()));
    let task = task(&marker);

    Faults::arm(&faults.units_after_writing, 1);
    assert!(matches!(exec.dispatch(&task), DispatchOutcome::Deferred));
    assert_eq!(
        faults.units_after_writing.load(Ordering::SeqCst),
        0,
        "the injected unknown outcome was not reached"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        runs(&marker),
        0,
        "nothing runs before the permit is confirmed"
    );
    assert_eq!(
        holders(&memory),
        0,
        "the unconfirmed holding was given back"
    );

    assert!(matches!(exec.dispatch(&task), DispatchOutcome::Started(_)));
    let deadline = Instant::now() + Duration::from_secs(10);
    while runs(&marker) == 0 {
        assert!(
            Instant::now() < deadline,
            "the confirmed attempt did not run"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(runs(&marker), 1, "one attempt runs once");
    assert_eq!(holders(&memory), 1);
}
