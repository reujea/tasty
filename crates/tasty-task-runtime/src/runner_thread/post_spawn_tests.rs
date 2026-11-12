//! 시작한 뒤 handle 저장이나 Running 전이를 확정하지 못한 회차. 같은 회차를 다시 시작하지 않고, 확정된
//! 전이는 그 handle 로 끝까지 지켜보며, 점유는 종결 뒤 한 번 돌려준다. 저장 실패는 실제 저장소에서 주입한다.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::runner::RunnerLoop;
use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{OnFailure, SemaphoreStore, TaskCommand, TaskState, TaskStore};
use tasty_memory::testing::faults::{Faults, FaultyStore};
use tasty_memory::{HOST_OWNER, MemoryStorage};

use crate::runner_host::{HANDLE_KEY_PREFIX, HostExecutor, RunnerContext};

const WAIT: Duration = Duration::from_secs(10);

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

struct Fixture {
    memory: Arc<Mutex<dyn MemoryStorage>>,
    faults: Arc<Faults>,
    ctx: RunnerContext,
    runner: RunnerLoop<HostExecutor>,
    marker: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

/// 시작·끝을 표식 파일에 한 줄씩 쓰는 Run task 하나와 크기 1 semaphore.
fn fixture(faults: Faults) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("runs");
    let faults = Arc::new(faults);
    let memory: Arc<Mutex<dyn MemoryStorage>> =
        Arc::new(Mutex::new(FaultyStore::new(faults.clone())));
    let ctx = ctx_on(memory.clone());
    {
        let mut guard = memory.lock().unwrap();
        TaskStore::new(&mut *guard, HOST_OWNER, &ctx.agent_seq)
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "gpu".into(),
                command: TaskCommand::Run {
                    command: vec![
                        "sh".into(),
                        "-c".into(),
                        format!(
                            "echo start >> {m}; sleep 0.3; echo end >> {m}",
                            m = marker.display()
                        ),
                    ],
                    workspace_id: 1,
                    cwd: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: serde_json::json!({"semaphore": {"name": "gpu"}}),
                now_ms: 1000,
            })
            .unwrap();
        SemaphoreStore::new(&mut *guard, HOST_OWNER)
            .create(1, "gpu", 1, 1000)
            .unwrap();
    }
    let runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    Fixture {
        memory,
        faults,
        ctx,
        runner,
        marker,
        _dir: dir,
    }
}

/// `run_loop` 한 바퀴와 같은 순서로 돈다. Running 전이는 `running_fails` 가 남은 동안 답 없이 실패한다.
fn tick(f: &mut Fixture, now: u64, running_fails: &AtomicU32) {
    let Fixture { ctx, runner, .. } = f;
    super::settle::settle_unwatched_handles(ctx, 1, |id| runner.executor.watches(id));
    let seq = ctx.agent_seq.clone();
    let snapshot = ctx
        .store(move |mem| TaskStore::new(mem, HOST_OWNER, seq.as_ref()).list(1))
        .expect("list answered")
        .expect("list");
    let (for_set, for_res) = (ctx.clone(), ctx.clone());
    runner.tick(
        1,
        now,
        &snapshot,
        move |ws, id, st, n| {
            if matches!(st, TaskState::Running)
                && running_fails
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |k| k.checked_sub(1))
                    .is_ok()
            {
                return Err(tasty_agent::AgentError::InvalidArgument(
                    "running transition not answered".into(),
                ));
            }
            let (seq, id) = (for_set.agent_seq.clone(), id.clone());
            for_set
                .store(move |mem| {
                    TaskStore::new(mem, HOST_OWNER, seq.as_ref())
                        .set_state(ws, &id, st.clone(), n)
                        .map(|_| ())
                })
                .unwrap_or_else(|blocked| {
                    Err(tasty_agent::AgentError::Memory(blocked.to_memory_error()))
                })
        },
        move |ws, id, c, n| for_res.complete_task(ws, id, c, n).map(|_| ()),
    );
}

fn lines(marker: &Path, word: &str) -> usize {
    std::fs::read_to_string(marker)
        .map(|s| s.lines().filter(|l| *l == word).count())
        .unwrap_or(0)
}

fn holders(f: &Fixture) -> usize {
    let mut guard = f.memory.lock().unwrap();
    SemaphoreStore::new(&mut *guard, HOST_OWNER)
        .get(1, "gpu")
        .unwrap()
        .expect("semaphore exists")
        .holders
        .len()
}

fn state(f: &Fixture) -> TaskState {
    let mut guard = f.memory.lock().unwrap();
    let mut tasks = TaskStore::new(&mut *guard, HOST_OWNER, &f.ctx.agent_seq)
        .list(1)
        .unwrap();
    tasks.pop().expect("task").state
}

/// 종결과 점유 반환까지 tick 을 돈다.
fn run_to_end(f: &mut Fixture, mut now: u64) {
    let none = AtomicU32::new(0);
    let deadline = Instant::now() + WAIT;
    while !(state(f).is_terminal() && holders(f) == 0) {
        assert!(Instant::now() < deadline, "끝나지 않았다: {:?}", state(f));
        now += 100;
        tick(f, now, &none);
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// handle 저장이 실제로는 확정됐는데 결과 불명으로 끝나고(`units_after_writing` 은 본문 오류 뒤에도
/// 앞선 쓰기를 commit 한다) Running 전이도 두 번 답을 받지 못한다. 같은 회차를 다시 시작하지 않고, 남은
/// handle 정리가 실행 중인 프로세스를 끝내지 않으며, 전이가 확정되면 그 프로세스를 끝까지 지켜본 뒤
/// 점유를 돌려준다.
#[test]
fn an_unanswered_handle_save_and_running_transition_start_the_attempt_once() {
    let mut f = fixture(Faults::default().writing(HANDLE_KEY_PREFIX));
    Faults::arm(&f.faults.units_after_writing, 1);
    let fails = AtomicU32::new(2);
    tick(&mut f, 1100, &fails);
    assert_eq!(
        f.faults.units_after_writing.load(Ordering::SeqCst),
        0,
        "주입이 닿지 않았다"
    );
    // Running 이 확정되기 전 tick 에서 남은 handle 정리가 돈다.
    tick(&mut f, 1200, &fails);
    assert_eq!(
        fails.load(Ordering::SeqCst),
        0,
        "Running 전이를 다시 시도하지 않았다"
    );
    assert_eq!(state(&f), TaskState::Ready);
    assert_eq!(holders(&f), 1, "실행 중인 회차의 점유를 놓았다");

    run_to_end(&mut f, 1300);
    assert_eq!(
        state(&f),
        TaskState::Succeeded,
        "실행 중인 프로세스를 끝냈다"
    );
    assert_eq!(lines(&f.marker, "start"), 1, "같은 회차를 다시 시작했다");
    assert_eq!(
        lines(&f.marker, "end"),
        1,
        "시작한 프로세스를 끝까지 지켜보지 않았다"
    );
}

/// Running 전이가 실제로는 확정됐는데 결과 불명으로 끝난다. 다음 tick 이 그 handle 을 지켜봐 완료를
/// 보고하고 점유를 돌려준다.
#[test]
fn a_running_transition_that_committed_without_an_answer_is_polled_to_the_end() {
    let mut f = fixture(Faults::default().writing("tasty.agent.task."));
    Faults::arm(&f.faults.units_after_writing, 1);
    tick(&mut f, 1100, &AtomicU32::new(0));
    assert_eq!(
        f.faults.units_after_writing.load(Ordering::SeqCst),
        0,
        "주입이 닿지 않았다"
    );
    assert_eq!(state(&f), TaskState::Running, "전이가 확정되지 않았다");
    assert_eq!(holders(&f), 1);

    run_to_end(&mut f, 1200);
    assert_eq!(state(&f), TaskState::Succeeded);
    assert_eq!(lines(&f.marker, "start"), 1, "같은 회차를 다시 시작했다");
    assert_eq!(lines(&f.marker, "end"), 1);
}
