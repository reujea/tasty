//! 러너가 지켜보는 task 의 lease·semaphore TTL 은 task 가 살아 있는 동안 갱신된다. 갱신하지 않으면
//! TTL 이 지난 뒤 다른 holder 가 같은 자원을 가져간다.
//!
//! 러너의 점유 시각은 시험이 진행시키는 시계([`HoldingClock::manual`])가 정하고, 다른 holder 의 획득도
//! 같은 시각으로 시도한다. 부하로 tick 이 늦게 와도 갱신 주기와 만료 판정이 실제 시각에 끌려가지 않는다.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tasty_agent::runner::{DispatchOutcome, TaskExecutor};
use tasty_agent::task::TaskCreateOpts;
use tasty_agent::{
    LeaseMode, LeaseStore, OnFailure, SemaphoreStore, TaskCommand, TaskState, TaskStore,
};
use tasty_memory::{HOST_OWNER, MemoryStore, MemoryValue, PutOpts, Scope};

use crate::runner_host::{HoldingClock, HostExecutor, RunnerContext, handle_key};

const TTL_MS: u64 = 300;
/// 빈 Run cwd 는 lease 자원으로 채워지므로(`${lease.resource}`) 있는 디렉터리를 자원으로 쓴다.
const DB: &str = "/";

fn ctx() -> (tempfile::TempDir, RunnerContext) {
    let td = tempfile::tempdir().unwrap();
    let mem = MemoryStore::open(&td.path().join("mem.db")).unwrap();
    let ctx = RunnerContext {
        scope_stopping: Arc::new(AtomicBool::new(false)),
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

fn sleeper(ctx: &RunnerContext) -> tasty_agent::Task {
    sleeper_with(
        ctx,
        serde_json::json!({"lease": {"resource": DB, "ttl_ms": TTL_MS}}),
    )
}

fn sleeper_with(ctx: &RunnerContext, metadata: serde_json::Value) -> tasty_agent::Task {
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .create(TaskCreateOpts {
                workspace_id: 1,
                name: "sleeper".into(),
                command: TaskCommand::Run {
                    command: vec!["sleep".into(), "30".into()],
                    workspace_id: 1,
                    cwd: None,
                },
                depends_on: vec![],
                on_failure: OnFailure::Abort,
                metadata,
                now_ms: 1000,
            })
            .unwrap()
    })
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// 러너와 시험이 함께 보는 시계를 단 executor. 실제 시각에서 출발해 시험이 움직일 때만 간다.
fn executor(ctx: &RunnerContext) -> (HostExecutor, Arc<AtomicU64>) {
    let (clock, at) = HoldingClock::manual(now());
    (HostExecutor::new(ctx.clone()).with_holding_clock(clock), at)
}

fn other_gets_db(ctx: &RunnerContext, at: u64) -> bool {
    ctx.with_store(|mem| {
        LeaseStore::new(mem, HOST_OWNER)
            .acquire(1, DB, "other", None, LeaseMode::Block, at)
            .unwrap()
            .acquired
    })
}

fn db_holder(ctx: &RunnerContext) -> Option<String> {
    ctx.with_store(|mem| {
        LeaseStore::new(mem, HOST_OWNER)
            .get(1, DB)
            .unwrap()
            .map(|l| l.holder)
    })
}

/// 시계를 tick 하나(TTL 의 1/6)씩 `for_ms` 만큼 진행하며 러너가 tick 마다 부르는 maintain 을 부른다.
/// `before_tick` 은 시각이 움직인 뒤 그 tick 의 maintain 전에 그 시각으로 불린다. 다른 holder 가
/// 끼어들 수 있는 가장 늦은 순간이다.
fn keep_ticking(
    exec: &mut HostExecutor,
    at: &AtomicU64,
    for_ms: u64,
    mut before_tick: impl FnMut(u64),
) {
    let step = TTL_MS / 6;
    for _ in 0..for_ms / step {
        before_tick(at.fetch_add(step, Ordering::SeqCst) + step);
        exec.maintain();
    }
}

/// 이 시각에 lease 가 만료됐는지. 갱신이 늦으면 다음 tick 전에 만료된다.
fn db_expired(ctx: &RunnerContext, at: u64) -> bool {
    ctx.with_store(|mem| {
        LeaseStore::new(mem, HOST_OWNER)
            .get(1, DB)
            .unwrap()
            .is_none_or(|l| l.is_expired(at))
    })
}

#[test]
fn a_running_task_keeps_its_ttl_lease_while_the_runner_watches_it() {
    let (_td, ctx) = ctx();
    let task = sleeper(&ctx);
    let (mut exec, at) = executor(&ctx);
    let outcome = exec.dispatch(&task);
    let DispatchOutcome::Started(_) = outcome else {
        panic!("dispatch: {outcome:?}");
    };
    keep_ticking(&mut exec, &at, TTL_MS * 4, |now| {
        assert!(
            !db_expired(&ctx, now),
            "{now}: tick 사이에 lease 가 만료됐다"
        )
    });
    assert!(
        !other_gets_db(&ctx, at.load(Ordering::SeqCst)),
        "살아 있는 task 의 lease 를 TTL 뒤 다른 holder 가 가져갔다"
    );
    assert_eq!(db_holder(&ctx), Some(task.id.clone()));
    let acquired_at = ctx.with_store(|mem| {
        LeaseStore::new(mem, HOST_OWNER)
            .get(1, DB)
            .unwrap()
            .unwrap()
            .acquired_at
    });
    assert!(
        acquired_at < at.load(Ordering::SeqCst) - TTL_MS,
        "갱신이 획득 시각을 바꿨다"
    );

    // 끝나면(취소) 프로세스를 끝내고 반환한다. 그 뒤에는 갱신하지 않는다.
    exec.release_permit(&task.id);
    let deadline = Instant::now() + Duration::from_secs(10);
    while db_holder(&ctx).is_some() && Instant::now() < deadline {
        exec.maintain();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(db_holder(&ctx), None);
}

/// 재시작 뒤 넘겨받은 Run 의 lease 도 갱신한다. 내려가 있던 동안 이미 만료 시각이 지났어도
/// 아무도 가져가지 않았으면 다시 늦춘다.
#[test]
fn a_restored_run_keeps_its_ttl_lease_after_a_restart() {
    let (_td, ctx) = ctx();
    let task = sleeper(&ctx);
    let id = task.id.clone();
    // 이전 호스트가 쥔 lease. 만료 시각은 이미 지났다.
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .set_state(1, &id, TaskState::Running, 1100)
            .unwrap();
        assert!(
            LeaseStore::new(mem, HOST_OWNER)
                .acquire(
                    1,
                    DB,
                    &id,
                    Some(TTL_MS),
                    LeaseMode::Fail,
                    now() - 10 * TTL_MS
                )
                .unwrap()
                .acquired
        );
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            &handle_key(&id),
            &MemoryValue::Json(serde_json::json!({
                "kind": "shell_process", "data": {"pid": std::process::id()}})),
            &PutOpts::default(),
        )
        .unwrap();
    });
    let (mut exec, at) = executor(&ctx);
    let restored = super::purge_and_reload_on_restart(&ctx, 1);
    assert_eq!(restored.len(), 1);
    let task = super::load_task(&ctx, 1, &id).unwrap();
    exec.adopt_restored_run(1, &task);
    exec.maintain();
    keep_ticking(&mut exec, &at, TTL_MS * 3, |now| {
        assert!(
            !db_expired(&ctx, now),
            "{now}: tick 사이에 lease 가 만료됐다"
        )
    });
    assert!(
        !other_gets_db(&ctx, at.load(Ordering::SeqCst)),
        "복원한 Run 의 lease 를 다른 holder 가 가져갔다"
    );
    assert_eq!(db_holder(&ctx), Some(id));
}

fn gpu_task(ctx: &RunnerContext) -> tasty_agent::Task {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .create(1, "gpu", 1, 1000)
            .unwrap()
    });
    sleeper_with(
        ctx,
        serde_json::json!({"semaphore": {"name": "gpu", "ttl_ms": TTL_MS}}),
    )
}

fn other_gets_gpu(ctx: &RunnerContext, at: u64) -> bool {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .acquire(1, "gpu", "other", None, at)
            .unwrap()
            .acquired
    })
}

/// 이 시각에 permit 이 만료됐는지.
fn gpu_expired(ctx: &RunnerContext, at: u64) -> bool {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .get(1, "gpu")
            .unwrap()
            .unwrap()
            .holders
            .first()
            .is_none_or(|h| h.is_expired(at))
    })
}

fn gpu_holder(ctx: &RunnerContext) -> Vec<(String, Option<u64>)> {
    ctx.with_store(|mem| {
        SemaphoreStore::new(mem, HOST_OWNER)
            .get(1, "gpu")
            .unwrap()
            .unwrap()
            .holders
            .into_iter()
            .map(|h| (h.id, h.acquired_at))
            .collect()
    })
}

#[test]
fn a_running_task_keeps_its_ttl_permit_while_the_runner_watches_it() {
    let (_td, ctx) = ctx();
    let task = gpu_task(&ctx);
    let (mut exec, at) = executor(&ctx);
    let outcome = exec.dispatch(&task);
    let DispatchOutcome::Started(_) = outcome else {
        panic!("dispatch: {outcome:?}");
    };
    keep_ticking(&mut exec, &at, TTL_MS * 4, |now| {
        assert!(
            !gpu_expired(&ctx, now),
            "{now}: tick 사이에 permit 이 만료됐다"
        )
    });
    assert!(
        !other_gets_gpu(&ctx, at.load(Ordering::SeqCst)),
        "살아 있는 task 의 permit 을 TTL 뒤 다른 holder 가 가져갔다"
    );
    let holders = gpu_holder(&ctx);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].0, task.id);
    assert!(
        holders[0].1.unwrap() < at.load(Ordering::SeqCst) - TTL_MS,
        "갱신이 획득 시각을 바꿨다"
    );

    exec.release_permit(&task.id);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !gpu_holder(&ctx).is_empty() && Instant::now() < deadline {
        exec.maintain();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(gpu_holder(&ctx).is_empty());
}

#[test]
fn a_restored_run_keeps_its_ttl_permit_after_a_restart() {
    let (_td, ctx) = ctx();
    let task = gpu_task(&ctx);
    let id = task.id.clone();
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .set_state(1, &id, TaskState::Running, 1100)
            .unwrap();
        assert!(
            SemaphoreStore::new(mem, HOST_OWNER)
                .acquire(1, "gpu", &id, Some(TTL_MS), now() - 10 * TTL_MS)
                .unwrap()
                .acquired
        );
        mem.put(
            HOST_OWNER,
            &Scope::Workspace(1),
            &handle_key(&id),
            &MemoryValue::Json(serde_json::json!({
                "kind": "shell_process", "data": {"pid": std::process::id()}})),
            &PutOpts::default(),
        )
        .unwrap();
    });
    let (mut exec, at) = executor(&ctx);
    assert_eq!(super::purge_and_reload_on_restart(&ctx, 1).len(), 1);
    let task = super::load_task(&ctx, 1, &id).unwrap();
    exec.adopt_restored_run(1, &task);
    exec.maintain();
    keep_ticking(&mut exec, &at, TTL_MS * 3, |now| {
        assert!(
            !gpu_expired(&ctx, now),
            "{now}: tick 사이에 permit 이 만료됐다"
        )
    });
    assert!(
        !other_gets_gpu(&ctx, at.load(Ordering::SeqCst)),
        "복원한 Run 의 permit 을 다른 holder 가 가져갔다"
    );
    assert_eq!(gpu_holder(&ctx)[0].0, id);
}

fn service(ctx: &RunnerContext) -> (crate::TaskService, crate::TaskScope) {
    let svc = crate::TaskService::new(
        ctx.memory.clone(),
        Arc::new(OnceLock::new()),
        Arc::new(crate::completion::fixture::Resolver::default()),
    );
    let scope = crate::TaskScope::new(svc.runner_registry().clone());
    (svc, scope)
}

fn create_opts(metadata: serde_json::Value, on_failure: OnFailure) -> TaskCreateOpts {
    TaskCreateOpts {
        workspace_id: 1,
        name: "t".into(),
        command: TaskCommand::Run {
            command: vec!["true".into()],
            workspace_id: 1,
            cwd: None,
        },
        depends_on: vec![],
        on_failure,
        metadata,
        now_ms: 1000,
    }
}

/// tick 두 번 안에 갱신하지 못할 만큼 짧은 TTL 은 생성·제출 때 거절한다(-32602 로 나가는 InvalidArgument).
#[test]
fn a_holding_ttl_shorter_than_two_ticks_is_rejected_at_create_and_submit() {
    let (_td, ctx) = ctx();
    let (svc, scope) = service(&ctx);
    let min = crate::runner_host::MIN_HOLDING_TTL_MS;
    assert_eq!(min, 1000);
    for meta in [
        serde_json::json!({"lease": {"resource": DB, "ttl_ms": min - 1}}),
        serde_json::json!({"semaphore": {"name": "gpu", "ttl_ms": 300}}),
    ] {
        let err = svc
            .task_create(&scope, create_opts(meta, OnFailure::Abort), false)
            .unwrap_err();
        assert!(
            matches!(err, tasty_agent::AgentError::InvalidArgument(_)),
            "{err:?}"
        );
    }
    let inline = OnFailure::Fallback {
        task: None,
        inline: Some(Box::new(tasty_agent::task::InlineFallbackSpec {
            name: "fb".into(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id: 1,
                cwd: None,
            },
            depends_on_override: None,
            on_failure: OnFailure::Abort,
            metadata: serde_json::json!({"lease": {"resource": DB, "ttl_ms": 10}}),
        })),
    };
    assert!(matches!(
        svc.task_create(&scope, create_opts(serde_json::json!({}), inline), false),
        Err(tasty_agent::AgentError::InvalidArgument(_))
    ));
    svc.task_create(
        &scope,
        create_opts(
            serde_json::json!({"lease": {"resource": DB, "ttl_ms": min}}),
            OnFailure::Abort,
        ),
        false,
    )
    .expect("하한과 같은 TTL 은 받는다");

    let spec: tasty_agent::task::TaskGraphSpec = serde_json::from_value(serde_json::json!({
        "contract_version": 2,
        "tasks": [{"id": "g", "command": {"kind": "run", "workspace_id": 1, "command": ["true"]},
                   "metadata": {"semaphore": {"name": "gpu", "ttl_ms": 500}}}]}))
    .unwrap();
    let Err(err) = svc.task_graph_submit(&scope, 1, spec, false, 2000) else {
        panic!("짧은 TTL 의 그래프를 받았다");
    };
    assert!(
        matches!(err, tasty_agent::AgentError::InvalidArgument(_)),
        "{err:?}"
    );
}

/// 갱신 전에 TTL 이 지나 다른 holder 가 자원을 가져가면 task 조회에 경고가 남는다.
#[test]
fn a_lease_lost_before_renewal_is_reported_on_the_task() {
    let (_td, ctx) = ctx();
    let (svc, scope) = service(&ctx);
    let task = sleeper(&ctx);
    let (mut exec, at) = executor(&ctx);
    let outcome = exec.dispatch(&task);
    let DispatchOutcome::Started(_) = outcome else {
        panic!("dispatch: {outcome:?}");
    };
    assert!(svc.task_holding_warnings(&scope, 1, &task.id).is_empty());
    // 러너가 tick 을 건너뛴 사이 TTL 이 지나 다른 holder 가 얻는다.
    at.fetch_add(TTL_MS + 50, Ordering::SeqCst);
    assert!(other_gets_db(&ctx, at.load(Ordering::SeqCst)));
    exec.maintain();
    let warnings = svc.task_holding_warnings(&scope, 1, &task.id);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0]["kind"], "lease");
    assert_eq!(warnings[0]["name"], DB);
    assert_eq!(db_holder(&ctx).as_deref(), Some("other"));
    // 잃은 점유는 다시 갱신하지 않아 기록이 늘지 않는다.
    keep_ticking(&mut exec, &at, TTL_MS, |_| {});
    assert_eq!(svc.task_holding_warnings(&scope, 1, &task.id).len(), 1);
    exec.release_permit(&task.id);
}

/// 기록은 그때의 회차 id 를 담고, 조회는 지금 회차의 것만 보인다. retry 로 연 새 회차에 이전 회차의
/// 경고가 섞이지 않는다.
#[test]
fn holding_warnings_show_only_the_current_attempts_records() {
    let (_td, ctx) = ctx();
    let (svc, scope) = service(&ctx);
    let id = "g".to_string();
    let spec: tasty_agent::task::TaskGraphSpec = serde_json::from_value(serde_json::json!({
        "contract_version": 2,
        "tasks": [{"id": id, "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}]}))
    .unwrap();
    let running = |now| {
        ctx.with_store(|mem| {
            TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
                .set_state(1, &id, TaskState::Running, now)
                .unwrap()
                .0
                .attempt
                .unwrap()
                .id
        })
    };
    ctx.with_store(|mem| {
        TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref())
            .submit_graph(1, spec, 0)
            .unwrap();
    });
    let first = running(10);
    let record = || {
        crate::runner_host::holding_warning::record_holding_warning(
            &ctx,
            1,
            &id,
            serde_json::json!({"kind": "lease", "name": DB, "message": "lost"}),
        )
    };
    record();
    let shown = svc.task_holding_warnings(&scope, 1, &id);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0]["attempt_id"], first.as_str());

    ctx.with_store(|mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
        store
            .set_state(1, &id, TaskState::Failed { error: "x".into() }, 20)
            .unwrap();
        store.retry(1, &id, false, 30).unwrap();
    });
    let second = running(40);
    assert_ne!(first, second);
    assert!(
        svc.task_holding_warnings(&scope, 1, &id).is_empty(),
        "이전 회차의 경고가 새 회차에 보인다"
    );
    record();
    let shown = svc.task_holding_warnings(&scope, 1, &id);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0]["attempt_id"], second.as_str());
}
