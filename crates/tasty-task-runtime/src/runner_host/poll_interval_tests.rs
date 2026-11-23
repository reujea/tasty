//! Task-local polling cadence through the actual runner executor entry point.
use super::scripted_ipc::{self, Scripted, Step};
use super::{HoldingClock, HostExecutor, tests::fresh_ctx};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tasty_agent::runner::{DispatchHandle, PollOutcome, TaskExecutor};
use tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN;

struct Fixture {
    exec: HostExecutor,
    time: Arc<AtomicU64>,
    calls: Arc<AtomicU64>,
    _ipc: Scripted,
    _dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let (dir, ctx) = fresh_ctx();
        let (clock, time) = HoldingClock::manual(1000);
        let calls = Arc::new(AtomicU64::new(0));
        let (count, clock_time) = (calls.clone(), time.clone());
        // 호출 수와 가짜 시계는 응답 종류를 고르기 전에 갱신한다. 성공·오류 응답 모두에 적용된다.
        let ipc = scripted_ipc::install_responder(&ctx, move |cmd| {
            count.fetch_add(1, Ordering::SeqCst);
            clock_time.fetch_add(
                cmd.request.params["duration"].as_u64().unwrap_or(0),
                Ordering::SeqCst,
            );
            match cmd.request.params["error_code"].as_i64() {
                Some(code) if code == i64::from(ERR_EXPIRED_BEFORE_RUN) => Step::ExpiredBeforeRun,
                Some(code) => Step::Error(code as i32, "controlled poll error".into()),
                None => {
                    let state = cmd.request.params["state"].as_str().unwrap_or("running");
                    Step::answer(json!({ "state": state }))
                }
            }
        });
        Self {
            exec: HostExecutor::new(ctx).with_holding_clock(clock),
            time,
            calls,
            _ipc: ipc,
            _dir: dir,
        }
    }
    fn at(&self, ms: u64) {
        self.time.store(ms, Ordering::SeqCst);
    }
    fn count(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
    fn poll(&mut self, task: &str, h: &DispatchHandle) -> PollOutcome {
        self.exec.poll_task(&task.to_string(), h)
    }
}
fn handle(interval_ms: u64) -> DispatchHandle {
    DispatchHandle::PolledDispatch {
        workspace_id: 1,
        poll_method: "fake.state".into(),
        poll_params: json!({}),
        state_field: "state".into(),
        terminal_states: vec!["done".into()],
        failure_states: vec!["failed".into()],
        interval_ms,
        deadline_ms: None,
    }
}
#[test]
fn first_check_is_immediate_and_next_check_obeys_inclusive_due_boundary() {
    let mut f = Fixture::new();
    let h = handle(2000);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 1);
    f.at(2999);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(
        f.count(),
        1,
        "poll must not call the endpoint before its interval is due"
    );
    f.at(3000);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
}

#[test]
fn identical_endpoints_keep_independent_task_intervals() {
    let mut f = Fixture::new();
    let fast = handle(1000);
    let slow = handle(4000);
    assert!(matches!(f.poll("fast", &fast), PollOutcome::Active));
    assert!(matches!(f.poll("slow", &slow), PollOutcome::Active));
    assert_eq!(f.count(), 2, "first checks are not shared");
    f.at(2000);
    assert!(matches!(f.poll("fast", &fast), PollOutcome::Active));
    assert!(matches!(f.poll("slow", &slow), PollOutcome::Active));
    assert_eq!(f.count(), 3);
    f.at(5000);
    assert!(matches!(f.poll("slow", &slow), PollOutcome::Active));
    assert_eq!(f.count(), 4);
}

#[test]
fn deadline_is_processed_without_an_early_endpoint_call() {
    let mut f = Fixture::new();
    let mut h = handle(10_000);
    if let DispatchHandle::PolledDispatch { deadline_ms, .. } = &mut h {
        *deadline_ms = Some(1500);
    }
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    f.at(1499);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    f.at(1500);
    assert!(matches!(f.poll("one", &h), PollOutcome::Failed(e) if e.contains("poll timeout")));
    assert_eq!(f.count(), 1);
}

#[test]
fn slow_requests_and_missed_ticks_wait_from_completion_without_catch_up() {
    let mut f = Fixture::new();
    let mut h = handle(2000);
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"duration":8000});
    }
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.time.load(Ordering::SeqCst), 9000);
    f.at(10_999);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 1);
    f.at(11_000);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
    f.at(200_000);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 3);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 3);
}

#[test]
fn first_due_response_still_wins_over_deadline_and_failure_wins_overlap() {
    let mut f = Fixture::new();
    let mut h = handle(2000);
    if let DispatchHandle::PolledDispatch {
        poll_params,
        deadline_ms,
        ..
    } = &mut h
    {
        *poll_params = json!({"state":"done", "duration":2000});
        *deadline_ms = Some(1000);
    }
    assert!(matches!(f.poll("success", &h), PollOutcome::Done(_)));
    if let DispatchHandle::PolledDispatch {
        poll_params,
        terminal_states,
        ..
    } = &mut h
    {
        *poll_params = json!({"state":"failed"});
        terminal_states.push("failed".into());
    }
    assert!(matches!(f.poll("failure", &h),PollOutcome::Failed(e) if e.contains("failure state")));
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"duration":2000});
    }
    assert!(
        matches!(f.poll("incomplete", &h),PollOutcome::Failed(e) if e.contains("poll timeout"))
    );
    assert_eq!(f.count(), 3);
}

#[test]
fn nonterminal_states_are_not_success_and_result_is_not_cached_between_tasks() {
    let mut f = Fixture::new();
    let mut h = handle(2000);
    for (task, state) in [("a", "stale"), ("b", "needs_input")] {
        if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
            *poll_params = json!({"state":state});
        }
        assert!(matches!(f.poll(task, &h), PollOutcome::Active));
    }
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"state":"done"});
    }
    f.at(2999);
    assert!(matches!(f.poll("a", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
    f.at(3000);
    assert!(matches!(f.poll("a", &h), PollOutcome::Done(_)));
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"state":"needs_input"});
    }
    assert!(matches!(f.poll("b", &h), PollOutcome::Active));
    assert_eq!(f.count(), 4);
}

#[test]
fn release_discards_schedule_before_retry_and_zero_interval_still_calls_once() {
    let mut f = Fixture::new();
    let h = handle(u64::MAX);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    f.exec.release_permit(&"one".into());
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
    f.at(u64::MAX - 1);
    assert!(matches!(f.poll("one", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
    let zero = handle(0);
    for _ in 0..2 {
        assert!(matches!(f.poll("zero", &zero), PollOutcome::Active));
    }
    assert_eq!(f.count(), 4);
}

#[test]
fn late_dispatch_continuation_uses_the_same_task_schedule() {
    let mut f = Fixture::new();
    let task = "late".to_string();
    f.exec.late.continued.insert(task.clone(), handle(2000));
    let h = DispatchHandle::AwaitDispatchAnswer {
        workspace_id: 1,
        task_id: task,
        ipc_method: "fake.start".into(),
    };
    assert!(matches!(f.poll("late", &h), PollOutcome::Active));
    f.at(2999);
    assert!(matches!(f.poll("late", &h), PollOutcome::Active));
    assert_eq!(f.count(), 1);
    f.at(3000);
    assert!(matches!(f.poll("late", &h), PollOutcome::Active));
    assert_eq!(f.count(), 2);
}

#[test]
fn persisted_handle_resumes_in_runner_without_redispatch_and_cancel_skips_poll() {
    use tasty_agent::runner::RunnerLoop;
    use tasty_agent::task::TaskCreateOpts;
    use tasty_agent::{OnFailure, TaskCommand, TaskState, TaskStore};
    use tasty_memory::{HOST_OWNER, MemoryValue, Scope};
    let mut f = Fixture::new();
    let ctx = f.exec.ctx.clone();
    let task = ctx.with_store(|mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
        let task = store
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "resume".into(),
                command: TaskCommand::Custom {
                    ipc_method: "must.not.redispatch".into(),
                    params: json!({}),
                    poll: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata: json!({}),
                now_ms: 1000,
            })
            .unwrap();
        store
            .set_state(1, &task.id, TaskState::Running, 1000)
            .unwrap();
        store.get(1, &task.id).unwrap().unwrap()
    });
    f.exec.persist_handle(1, &task.id, &handle(2000), None);
    assert!(matches!(
        f.poll(&task.id, &handle(2000)),
        PollOutcome::Active
    ));
    let restored = ctx.with_store(|mem| {
        let entry = mem
            .get(&Scope::Workspace(1), &super::handle_key(&task.id))
            .unwrap()
            .unwrap();
        let MemoryValue::Json(value) = entry.value else {
            panic!("JSON handle")
        };
        serde_json::from_value::<DispatchHandle>(value).unwrap()
    });
    let fresh = HostExecutor::new(ctx.clone()).with_holding_clock(f.exec.holding_clock.clone());
    f.exec = fresh;
    f.at(1500);
    let exec = std::mem::replace(&mut f.exec, HostExecutor::new(ctx));
    let mut runner = RunnerLoop::new(exec);
    runner.running.insert(task.id.clone(), restored);
    for now in [1500, 3499, 3500] {
        f.at(now);
        runner.tick(
            1,
            now,
            std::slice::from_ref(&task),
            |_, _, _, _| panic!("no redispatch"),
            |_, _, _, _| panic!("still active"),
        );
        assert_eq!(f.count(), if now == 3500 { 3 } else { 2 });
    }
    let mut cancelled = task;
    cancelled.state = TaskState::Cancelled;
    f.at(5500);
    runner.tick(
        1,
        5500,
        &[cancelled],
        |_, _, _, _| panic!("no redispatch"),
        |_, _, _, _| panic!("no fabricated completion"),
    );
    assert_eq!(f.count(), 3);
    assert!(runner.running.is_empty());
    assert!(runner.executor.next_poll_ms.is_empty());
}

#[test]
fn host_slow_retry_waits_for_interval_while_rpc_error_fails() {
    let mut f = Fixture::new();
    let mut h = handle(2000);
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"error_code":tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN});
    }
    assert!(matches!(f.poll("slow", &h), PollOutcome::Active));
    f.at(2999);
    assert!(matches!(f.poll("slow", &h), PollOutcome::Active));
    assert_eq!(f.count(), 1);
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"state":"done"});
    }
    f.at(3000);
    assert!(matches!(f.poll("slow", &h), PollOutcome::Done(_)));
    assert_eq!(f.count(), 2);
    if let DispatchHandle::PolledDispatch { poll_params, .. } = &mut h {
        *poll_params = json!({"error_code":-32601});
    }
    assert!(
        matches!(f.poll("error",&h),PollOutcome::Failed(e) if e.contains("controlled poll error"))
    );
    assert_eq!(f.count(), 3);
}
