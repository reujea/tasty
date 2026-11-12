//! 재시작 복구 — 시작을 기록한 후처리는 다시 실행하지 않고, 예약만 된 실행은 이어서 실행한다.

use serde_json::{Value, json};
use tasty_agent::runner::RunnerLoop;
use tasty_agent::task::postprocess::{PostprocessOutcome, PostprocessReport};
use tasty_agent::task::{Completion, TaskGraphSpec};
use tasty_memory::{MemoryValue, PutOpts};

use super::super::runner_host::{handle_value, postprocess_result_key};
use super::*;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use tasty_agent::{Task, TaskResult};
use tasty_memory::MemoryStore;

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

const JUDGE: &str = "judge";

fn store_op<R>(ctx: &RunnerContext, f: impl FnOnce(&mut TaskStore) -> R) -> R {
    ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        f(&mut store)
    })
}

fn judge(ctx: &RunnerContext) -> Task {
    store_op(ctx, |s| {
        s.get(1, &JUDGE.to_string()).unwrap().expect("task")
    })
}

/// 본 작업까지 끝낸 judge 를 만든다. 후처리는 실행되면 `log` 에 한 줄을 남긴다.
fn after_main(ctx: &RunnerContext, log: &std::path::Path, retry: Value) -> String {
    let script = format!("echo pp >> '{}'; printf true", log.display());
    let mut postprocess = json!({"command": ["sh", "-c", script], "timeout_ms": 10000});
    if !retry.is_null() {
        postprocess["retry"] = retry;
    }
    let spec: TaskGraphSpec = serde_json::from_value(json!({
        "contract_version": 2,
        "tasks": [{"id": JUDGE, "output_schema": {"type": "boolean"}, "postprocess": postprocess,
                   "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]
    }))
    .expect("graph");
    let id = JUDGE.to_string();
    let attempt = store_op(ctx, |s| {
        s.submit_graph(1, spec, 0).unwrap();
        s.set_state(1, &id, TaskState::Running, 1).unwrap();
        s.get(1, &id).unwrap().unwrap().attempt.unwrap().id
    });
    let main = TaskResult {
        exit_code: Some(0),
        output: None,
        error: None,
    };
    store_op(ctx, |s| {
        s.complete(
            1,
            &id,
            Completion::succeeded(Some(attempt.clone()), main),
            2,
        )
        .unwrap()
    });
    // 이전 호스트가 저장한 본 작업 handle. 복원은 이 handle 이 아니라 회차의 진행을 따른다.
    let handle = handle_value(
        serde_json::to_value(DispatchHandle::ShellProcess { pid: 0xFFFF_FFFE }).unwrap(),
        Some(&attempt),
    );
    let MemoryValue::Json(handle) = handle else {
        panic!("handle value is JSON");
    };
    put(ctx, &handle_key(&id), handle);
    attempt
}

fn put(ctx: &RunnerContext, key: &str, value: Value) {
    ctx.with_store(|mem| {
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            key,
            &MemoryValue::Json(value),
            &PutOpts::default(),
        )
        .unwrap()
    });
}

fn begin(ctx: &RunnerContext, attempt: &str, run: u32) {
    store_op(ctx, |s| {
        s.begin_postprocess_run(1, &JUDGE.to_string(), attempt, run, 3)
            .unwrap()
    });
}

/// 재시작한 runner 로 judge 가 끝날 때까지 돌린다.
fn restart_and_finish(ctx: &RunnerContext) -> Task {
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    for (task_id, handle) in reload_persistent_handles(ctx, 1) {
        runner.running.insert(task_id, handle);
    }
    for _ in 0..500 {
        let snapshot = store_op(ctx, |s| s.list(1).unwrap());
        let (a, b) = (ctx.clone(), ctx.clone());
        runner.tick(
            1,
            now_ms(),
            &snapshot,
            move |ws, id, st, n| store_op(&a, |s| s.set_state(ws, id, st, n).map(|_| ())),
            move |ws, id, c, n| store_op(&b, |s| s.complete(ws, id, c, n).map(|_| ())),
        );
        let t = judge(ctx);
        if t.state.is_terminal() {
            return t;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("judge did not finish: {:?}", judge(ctx).state);
}

fn runs(log: &std::path::Path) -> usize {
    std::fs::read_to_string(log)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[test]
fn a_started_run_without_a_recorded_result_is_not_run_again() {
    let (td, ctx) = fresh_ctx();
    let log = td.path().join("pp.log");
    // 재시도가 남아 있어도 결과 불명은 다시 실행하지 않는다.
    let attempt = after_main(&ctx, &log, json!({"max_retries": 3}));
    begin(&ctx, &attempt, 1);
    let t = restart_and_finish(&ctx);
    assert!(
        matches!(&t.state, TaskState::Failed { error } if error.contains("outcome_unknown")),
        "{:?}",
        t.state
    );
    assert_eq!(runs(&log), 0);
}

#[test]
fn a_started_run_with_a_recorded_result_finishes_with_that_result() {
    let (td, ctx) = fresh_ctx();
    let log = td.path().join("pp.log");
    let attempt = after_main(&ctx, &log, Value::Null);
    begin(&ctx, &attempt, 1);
    let report = PostprocessReport {
        run: 1,
        exit_code: Some(0),
        stderr: None,
        stderr_truncated: false,
        outcome: PostprocessOutcome::Collected {
            stdout: json!(false),
        },
    };
    put(
        &ctx,
        &postprocess_result_key(JUDGE),
        json!({"attempt_id": attempt, "report": report}),
    );
    let t = restart_and_finish(&ctx);
    assert_eq!(t.state, TaskState::Succeeded, "{:?}", t.result);
    let typed = t.typed_result.unwrap();
    assert_eq!(serde_json::to_value(&typed.output).unwrap(), json!(false));
    assert_eq!(runs(&log), 0);
}

#[test]
fn a_result_from_another_attempt_is_not_used() {
    let (td, ctx) = fresh_ctx();
    let log = td.path().join("pp.log");
    let attempt = after_main(&ctx, &log, Value::Null);
    begin(&ctx, &attempt, 1);
    let report = PostprocessReport {
        run: 1,
        exit_code: Some(0),
        stderr: None,
        stderr_truncated: false,
        outcome: PostprocessOutcome::Collected {
            stdout: json!(true),
        },
    };
    put(
        &ctx,
        &postprocess_result_key(JUDGE),
        json!({"attempt_id": format!("{JUDGE}#99"), "report": report}),
    );
    let t = restart_and_finish(&ctx);
    assert!(matches!(&t.state, TaskState::Failed { error } if error.contains("outcome_unknown")));
}

#[test]
fn a_scheduled_run_that_never_started_runs_once_after_the_restart() {
    let (td, ctx) = fresh_ctx();
    let log = td.path().join("pp.log");
    after_main(&ctx, &log, Value::Null);
    let t = restart_and_finish(&ctx);
    assert_eq!(t.state, TaskState::Succeeded, "{:?}", t.result);
    assert_eq!(runs(&log), 1);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn gone(pid: i32) -> bool {
    crate::process_gone_tests_support::gone(pid as u32)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn dropping_the_registry_waits_until_the_postprocess_is_stopped_and_recorded() {
    let (td, ctx) = fresh_ctx();
    let pidfile = td.path().join("child.pid");
    let pid_s = pidfile.display().to_string();
    let script = format!("sleep 30 & echo $! > '{pid_s}'.tmp; mv '{pid_s}'.tmp '{pid_s}'; wait");
    let spec: TaskGraphSpec = serde_json::from_value(json!({
        "contract_version": 2,
        "tasks": [{"id": JUDGE,
                   "postprocess": {"command": ["sh", "-c", script], "timeout_ms": 60000,
                                   "retry": {"max_retries": 3}},
                   "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]
    }))
    .expect("graph");
    store_op(&ctx, |s| s.submit_graph(1, spec, 0).unwrap());
    let registry = RunnerRegistry::new();
    assert!(registry.start(ctx.clone(), 1));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !pidfile.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "postprocess did not start: {:?}",
            judge(&ctx).state
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let child: i32 = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // 앱 종료 때 서비스 소유자가 사라지는 경로. 돌아온 뒤에는 프로세스 종료가 이어져도 된다.
    drop(registry);
    assert!(
        gone(child),
        "postprocess grandchild {child} survived the shutdown"
    );
    let stored = ctx.with_store(|mem| {
        mem.get(&Scope::Workspace(1), &postprocess_result_key(JUDGE))
            .unwrap()
            .map(|e| e.value)
    });
    let Some(MemoryValue::Json(stored)) = stored else {
        panic!("no postprocess report was recorded: {stored:?}");
    };
    assert_eq!(
        stored["report"]["outcome"]["cause"],
        json!("cancelled"),
        "{stored}"
    );
}

/// 세마포어 gpu(permit 1)를 쥔 채 후처리 실행 1 을 시작한 judge. 이전 호스트가 저장한 후처리
/// handle 은 `pid`·`started_at` 를 가리킨다.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn holding_judge_in_postprocess(ctx: &RunnerContext, pid: u32, started_at: u64) {
    let spec: TaskGraphSpec = serde_json::from_value(json!({
        "contract_version": 2,
        "tasks": [{"id": JUDGE, "output_schema": {"type": "boolean"},
                   "metadata": {"semaphore": {"name": "gpu"}},
                   "postprocess": {"command": ["true"], "timeout_ms": 10000,
                                   "retry": {"max_retries": 3}},
                   "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]
    }))
    .expect("graph");
    let id = JUDGE.to_string();
    let attempt = store_op(ctx, |s| {
        s.submit_graph(1, spec, 0).unwrap();
        s.set_state(1, &id, TaskState::Running, 1).unwrap();
        s.get(1, &id).unwrap().unwrap().attempt.unwrap().id
    });
    let main = TaskResult {
        exit_code: Some(0),
        output: None,
        error: None,
    };
    store_op(ctx, |s| {
        s.complete(
            1,
            &id,
            Completion::succeeded(Some(attempt.clone()), main),
            2,
        )
        .unwrap()
    });
    begin(ctx, &attempt, 1);
    ctx.with_store(|mem| {
        let mut sem = tasty_agent::SemaphoreStore::new(mem, HOST_OWNER);
        sem.create(1, "gpu", 1, 1).unwrap();
        assert!(sem.acquire(1, "gpu", JUDGE, None, 1).unwrap().acquired);
    });
    let handle = handle_value(
        serde_json::to_value(DispatchHandle::PostprocessProcess { pid, run: 1 }).unwrap(),
        Some(&attempt),
    );
    let MemoryValue::Json(mut handle) = handle else {
        panic!("handle value is JSON");
    };
    handle["started_at"] = json!(started_at);
    put(ctx, &handle_key(&id), handle);
}

fn gpu_holders(ctx: &RunnerContext) -> Vec<String> {
    ctx.with_store(|mem| {
        tasty_agent::SemaphoreStore::new(mem, HOST_OWNER)
            .get(1, "gpu")
            .unwrap()
            .unwrap()
            .holders
            .into_iter()
            .map(|h| h.id)
            .collect()
    })
}

/// 이전 호스트가 띄운 후처리처럼 새 프로세스 그룹에서 오래 도는 프로세스. 회수는 별도 스레드가
/// 하고, 그 스레드를 돌려준다. 끝낸 뒤 join 하면 회수까지 끝난 것이 확정된다.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_postprocess_like() -> (
    u32,
    u64,
    std::thread::JoinHandle<std::io::Result<std::process::ExitStatus>>,
) {
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    let started_at = tasty_agent::platform::process_start::start_time(pid).unwrap();
    let reaper = std::thread::spawn(move || child.wait());
    (pid, started_at, reaper)
}

/// 러너 시작처럼 정리·복원한 뒤 복원한 회차를 넘겨받은 runner.
fn restarted_runner(ctx: &RunnerContext) -> RunnerLoop<HostExecutor> {
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    for (task_id, handle) in purge_and_reload_on_restart(ctx, 1) {
        if let Some(task) = load_task(ctx, 1, &task_id)
            && ctx.with_store(|mem| resumes_after_restart(mem, 1, &task))
        {
            runner.executor.adopt_restored_run(1, &task);
        }
        runner.running.insert(task_id, handle);
    }
    runner
}

fn tick(ctx: &RunnerContext, runner: &mut RunnerLoop<HostExecutor>) {
    let snapshot = store_op(ctx, |s| s.list(1).unwrap());
    let (a, b) = (ctx.clone(), ctx.clone());
    runner.tick(
        1,
        now_ms(),
        &snapshot,
        move |ws, id, st, n| store_op(&a, |s| s.set_state(ws, id, st, n).map(|_| ())),
        move |ws, id, c, n| store_op(&b, |s| s.complete(ws, id, c, n).map(|_| ())),
    );
}

fn tick_until(
    ctx: &RunnerContext,
    runner: &mut RunnerLoop<HostExecutor>,
    what: &str,
    done: impl Fn() -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: {:?}",
            judge(ctx).state
        );
        tick(ctx, runner);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// 재시작 뒤에도 살아 있는 후처리는 끝날 때까지 permit 을 쥔다. 끝나면 결과를 받을 수 없어 결과
/// 불명으로 끝나고 그때 반환한다. 다시 실행하지 않는다.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_live_postprocess_keeps_its_permit_across_a_restart_until_it_ends() {
    let (_td, ctx) = fresh_ctx();
    let (pid, started_at, reaper) = spawn_postprocess_like();
    holding_judge_in_postprocess(&ctx, pid, started_at);

    let mut runner = restarted_runner(&ctx);
    for _ in 0..5 {
        tick(&ctx, &mut runner);
    }
    assert_eq!(judge(&ctx).state, TaskState::Running);
    assert_eq!(
        gpu_holders(&ctx),
        vec![JUDGE.to_string()],
        "살아 있는 후처리의 permit 을 풀었다"
    );

    // SAFETY: 이 시험이 띄운 프로세스 그룹이다.
    assert_eq!(unsafe { libc::kill(-(pid as i32), libc::SIGKILL) }, 0);
    // 회수 전 종료 상태(좀비)는 러너가 살아 있다고 본다. 회수가 끝난 뒤부터 tick 을 센다.
    reaper.join().unwrap().unwrap();
    tick_until(&ctx, &mut runner, "unknown after the end", || {
        judge(&ctx).state.is_terminal()
    });
    let state = judge(&ctx).state;
    assert!(
        matches!(&state, TaskState::Failed { error } if error.contains("outcome_unknown") && error.contains("ended after a host restart")),
        "{state:?}"
    );
    tick_until(&ctx, &mut runner, "permit returned", || {
        gpu_holders(&ctx).is_empty()
    });
}

/// 재시작 동안 끝난 후처리(저장된 보고 없음)는 부팅 정리가 결과 불명으로 끝내고 permit 을 반환한다.
/// 점유를 쥐었다는 이유로 `host restart` 실패가 되지 않는다.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_postprocess_that_ended_during_the_restart_ends_unknown_at_the_cleanup() {
    let (_td, ctx) = fresh_ctx();
    let (pid, started_at, reaper) = spawn_postprocess_like();
    // SAFETY: 이 시험이 띄운 프로세스 그룹이다.
    assert_eq!(unsafe { libc::kill(-(pid as i32), libc::SIGKILL) }, 0);
    // 부팅 정리는 회수 전 종료 상태(좀비)도 살아 있다고 본다(`run_group::is_running`). 좀비가 된 것만
    // 보고 정리를 부르면 회수가 늦을 때 task 가 Running 에 남으므로 회수 스레드를 join 한다.
    reaper.join().unwrap().unwrap();
    holding_judge_in_postprocess(&ctx, pid, started_at);

    purge_stale_agent_state_on_boot(&ctx, &[1]);
    let state = judge(&ctx).state;
    assert!(
        matches!(&state, TaskState::Failed { error } if error.contains("outcome_unknown")),
        "{state:?}"
    );
    assert!(gpu_holders(&ctx).is_empty());
}

/// Linux 에서 호스트가 비정상 종료하면 리더만 PDEATHSIG 로 끝나고 리더가 띄운 프로세스는 남는다.
/// 재시작 판정은 리더만 보므로 task 는 결과 불명으로 끝나고 permit 을 반환한다. 남은 그룹 구성원은
/// 정리하지 않는다(docs/dev-guide/agent-runner.md 의 비정상 종료 한계).
#[cfg(target_os = "linux")]
#[test]
fn a_dead_leader_ends_unknown_even_while_its_group_member_lives() {
    use std::os::unix::process::CommandExt;
    let (td, ctx) = fresh_ctx();
    let pidfile = td.path().join("member.pid");
    let pid_s = pidfile.display().to_string();
    let mut leader = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "sleep 60 & echo $! > '{pid_s}'.tmp; mv '{pid_s}'.tmp '{pid_s}'; wait"
        ))
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = leader.id();
    let started_at = tasty_agent::platform::process_start::start_time(pid).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let member: i32 = loop {
        if let Some(p) = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            break p;
        }
        assert!(std::time::Instant::now() < deadline, "member never started");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    // 비정상 종료 때 PDEATHSIG 가 리더에게만 보내는 신호를 흉내 낸다.
    // SAFETY: 이 시험이 띄운 리더에게만 보낸다.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    leader.wait().unwrap();
    holding_judge_in_postprocess(&ctx, pid, started_at);

    purge_stale_agent_state_on_boot(&ctx, &[1]);
    let state = judge(&ctx).state;
    assert!(
        matches!(&state, TaskState::Failed { error } if error.contains("outcome_unknown")),
        "{state:?}"
    );
    assert!(gpu_holders(&ctx).is_empty());
    // 신호는 비동기로 처리되므로 정리가 보낸 신호가 있었다면 드러날 시간을 둔다.
    let watch_until = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let mut member_left = !gone(member);
    while member_left && std::time::Instant::now() < watch_until {
        std::thread::sleep(std::time::Duration::from_millis(10));
        member_left = !gone(member);
    }
    // SAFETY: 이 시험의 리더가 띄운 프로세스다.
    unsafe { libc::kill(member, libc::SIGKILL) };
    assert!(member_left, "재시작 판정이 남은 그룹 구성원을 끝냈다");
}

/// 재시작 뒤 넘겨받은 살아 있는 후처리를 취소하면 그 그룹을 끝내고, 끝난 것을 확인한 뒤 반환한다.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn cancelling_a_restored_postprocess_kills_it_before_its_permit_returns() {
    let (_td, ctx) = fresh_ctx();
    // 종료 확인은 러너가 tick 마다 한다. 회수 스레드는 기다리지 않는다.
    let (pid, started_at, _reaper) = spawn_postprocess_like();
    holding_judge_in_postprocess(&ctx, pid, started_at);
    let mut runner = restarted_runner(&ctx);
    tick(&ctx, &mut runner);
    store_op(&ctx, |s| s.cancel(1, &JUDGE.to_string(), 5).unwrap());
    tick_until(&ctx, &mut runner, "permit returned", || {
        gpu_holders(&ctx).is_empty()
    });
    assert!(gone(pid as i32), "취소한 후처리가 남았다");
}

/// gpu(permit 1)를 쥔 채 후처리를 기다리는 judge. `retry_wait` 이면 실행 1 이 실패해 실행 2 를
/// 기다리고(저장된 handle 은 끝난 실행 1 의 후처리 handle), 아니면 본 작업만 끝났다(저장된 handle 은
/// 본 작업의 Run handle). 후처리는 실행되면 `log` 에 한 줄을 남기고 true 를 낸다.
fn holding_judge_pending(ctx: &RunnerContext, log: &std::path::Path, retry_wait: bool) {
    let script = format!("echo pp >> '{}'; printf true", log.display());
    let spec: TaskGraphSpec = serde_json::from_value(json!({
        "contract_version": 2,
        "tasks": [{"id": JUDGE, "output_schema": {"type": "boolean"},
                   "metadata": {"semaphore": {"name": "gpu"}},
                   "postprocess": {"command": ["sh", "-c", script], "timeout_ms": 10000,
                                   "retry": {"max_retries": 3, "delay_ms": 200}},
                   "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]
    }))
    .expect("graph");
    let id = JUDGE.to_string();
    let attempt = store_op(ctx, |s| {
        s.submit_graph(1, spec, 0).unwrap();
        s.set_state(1, &id, TaskState::Running, 1).unwrap();
        s.get(1, &id).unwrap().unwrap().attempt.unwrap().id
    });
    let main = TaskResult {
        exit_code: Some(0),
        output: None,
        error: None,
    };
    store_op(ctx, |s| {
        s.complete(
            1,
            &id,
            Completion::succeeded(Some(attempt.clone()), main),
            2,
        )
        .unwrap()
    });
    ctx.with_store(|mem| {
        let mut sem = tasty_agent::SemaphoreStore::new(mem, HOST_OWNER);
        sem.create(1, "gpu", 1, 1).unwrap();
        assert!(sem.acquire(1, "gpu", JUDGE, None, 1).unwrap().acquired);
    });
    let handle = if retry_wait {
        begin(ctx, &attempt, 1);
        let failed = PostprocessReport::failed(
            1,
            tasty_agent::task::postprocess::PostprocessCause::NonzeroExit,
            "exited with code 1",
        );
        store_op(ctx, |s| {
            s.complete(
                1,
                &id,
                Completion::postprocessed(Some(attempt.clone()), failed),
                now_ms(),
            )
            .unwrap()
        });
        DispatchHandle::PostprocessProcess {
            pid: 0xFFFF_FFFE,
            run: 1,
        }
    } else {
        DispatchHandle::ShellProcess { pid: 0xFFFF_FFFE }
    };
    let phase = judge(ctx).attempt.unwrap().postprocess.unwrap().phase;
    assert!(
        matches!(phase, tasty_agent::task::postprocess::PostprocessPhase::Pending { run, .. } if run == if retry_wait { 2 } else { 1 }),
        "{phase:?}"
    );
    let MemoryValue::Json(handle) =
        handle_value(serde_json::to_value(handle).unwrap(), Some(&attempt))
    else {
        panic!("handle value is JSON");
    };
    put(ctx, &handle_key(&id), handle);
}

/// 후처리를 기다리던 회차는 점유를 쥔 채 재시작을 넘어 예약대로 후처리를 실행하고, 끝난 뒤 반환한다.
/// 점유를 쥐었다는 이유로 `host restart` 실패가 되지 않는다.
#[test]
fn a_pending_postprocess_keeps_its_permit_and_runs_as_scheduled_after_a_restart() {
    for retry_wait in [false, true] {
        let (td, ctx) = fresh_ctx();
        let log = td.path().join("pp.log");
        holding_judge_pending(&ctx, &log, retry_wait);

        purge_stale_agent_state_on_boot(&ctx, &[1]);
        assert_eq!(
            judge(&ctx).state,
            TaskState::Running,
            "retry_wait={retry_wait}"
        );
        assert_eq!(gpu_holders(&ctx), vec![JUDGE.to_string()]);

        let mut runner = restarted_runner(&ctx);
        tick_until(&ctx, &mut runner, "postprocess ran", || {
            judge(&ctx).state.is_terminal()
        });
        assert_eq!(
            judge(&ctx).state,
            TaskState::Succeeded,
            "retry_wait={retry_wait}"
        );
        assert_eq!(runs(&log), 1);
        tick_until(&ctx, &mut runner, "permit returned", || {
            gpu_holders(&ctx).is_empty()
        });
    }
}
