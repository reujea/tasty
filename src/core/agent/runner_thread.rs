//! Workspace 별 runner thread 의 시작/중단/상태를 관리하는 registry.
//!
//! `agent.task_run` IPC handler 가 본 registry 를 호출. 한 workspace 에 동시
//! runner 는 1 개만 — 중복 start 는 no-op.
//!
//! runner thread 본문: `RunnerLoop::tick` 을 500ms 간격으로 호출. tick 안의
//! task_list / set_state / set_result 는 `RunnerContext::with_memory` 의 짧은
//! lock 안에서만 수행 (executor.dispatch / poll 의 plugin IPC 호출은 lock 바깥).
//!
//! R5 완화: tick 본문은 `std::panic::catch_unwind` 로 감싸 panic 시 stop_tx 채널
//! 닫고 thread 종료 — registry 의 status 는 자동으로 false 가 된다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tasty_agent::runner::{DispatchHandle, RunnerLoop};
use tasty_agent::{LeaseStore, SemaphoreStore, TaskId, TaskResult, TaskState, TaskStore};
use tasty_memory::{HOST_OWNER, ListOpts, MemoryValue, Scope};

use super::runner_host::{
    HANDLE_KEY_PREFIX, HostExecutor, RunnerContext, evict_run_result, evict_task_side_keys,
    handle_key, load_run_result,
};
use tasty_agent::runner::PollOutcome;

const TICK_INTERVAL: Duration = Duration::from_millis(500);

/// task snapshot 조회가 연속 실패했을 때 몇 번째 실패를 `error!` 로 올릴지.
/// `TICK_INTERVAL` 이 500ms 라 6 은 약 3 초 — 일시적 lock 경합 한두 번과
/// "store 가 계속 안 읽힌다" 를 가른다.
const STORE_LIST_ERROR_AFTER: u32 = 6;
/// 그 뒤로는 이 주기(약 60 초)로만 다시 남긴다 — tick 마다 찍으면 초당 2 줄이라
/// 로그가 쓸모없어진다.
const STORE_LIST_REPEAT_EVERY: u32 = 120;

/// 연속 `n` 번째 task snapshot 조회 실패를 어떻게 남길지.
///
/// 첫 실패는 `warn!`(일시적일 수 있다), [`STORE_LIST_ERROR_AFTER`] 번째부터는
/// `error!` — 그 시점이면 runner 가 Ready/Running task 를 하나도 진행시키지
/// 못하는 상태가 지속된다는 뜻이고, UI 는 여전히 진행 중으로 보인다
/// (`docs/dev-guide/error-handling.md` 의 "복구 불가, 사용자 작업이 의미를 잃음").
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum StoreListFailureLog {
    Silent,
    Warn,
    Error,
}

fn store_list_failure_log(consecutive: u32) -> StoreListFailureLog {
    if consecutive == 1 {
        return StoreListFailureLog::Warn;
    }
    if consecutive < STORE_LIST_ERROR_AFTER {
        return StoreListFailureLog::Silent;
    }
    if consecutive == STORE_LIST_ERROR_AFTER
        || (consecutive - STORE_LIST_ERROR_AFTER).is_multiple_of(STORE_LIST_REPEAT_EVERY)
    {
        StoreListFailureLog::Error
    } else {
        StoreListFailureLog::Silent
    }
}

struct RunnerControl {
    stop_tx: mpsc::Sender<()>,
    crashed: Arc<AtomicBool>,
    /// tick 머리의 task snapshot 조회가 **연속** 실패한 횟수(성공하면 0). 조회로
    /// "러너가 살아는 있는데 아무것도 못 읽고 있다" 를 드러내기 위해 스레드와
    /// 공유한다 — `running: true` 인데 이 값이 크면 DAG 는 정지 상태다.
    list_failures: Arc<AtomicU32>,
    /// `Option` — `stop` 에서 `take()` 후 join.
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct RunnerStatus {
    pub running: bool,
    pub crashed: bool,
    /// `None` = **셀 수 없었다**(store 조회 실패). 0 과 구분한다 — 조회가 실패했는데
    /// 0 을 돌려주면 "task 가 없다" 와 같은 값이 되어, 이 응답이 계약대로
    /// "정지 상태를 드러내는" 대신 정상으로 보이게 만든다.
    pub ready_count: Option<u32>,
    pub running_count: Option<u32>,
    /// 위 카운트가 `None` 인 이유. 조회에 성공했으면 `None`.
    pub store_error: Option<String>,
    /// 러너 스레드의 연속 조회 실패 횟수(러너가 없으면 0).
    pub list_failures: u32,
}

pub struct RunnerRegistry {
    threads: Mutex<HashMap<u32, RunnerControl>>,
    /// poison 을 이미 보고했는가. `liveness` 는 렌더 경로가 프레임마다 부르므로
    /// 매번 로그를 내면 폭주한다.
    poison_reported: AtomicBool,
}

impl RunnerRegistry {
    /// Poison 된 스레드 맵을 복구한다.
    ///
    /// 맵이 담는 것은 `RunnerControl`(mpsc `Sender` · `Arc<AtomicBool>` ·
    /// `JoinHandle`)뿐이고 임계구역은 조회·삽입·제거밖에 하지 않는다 — 패닉이 나도
    /// 맵의 불변식은 성립한다.
    ///
    /// 사망 범위가 두 겹이라 패닉이 특히 나쁘다.
    ///
    /// - [`Self::liveness`] 는 **렌더 경로**가 프레임마다 부른다(DAG surface 의 러너
    ///   배지). 메인 스레드라 여기서 패닉하면 모든 창의 터미널 세션이 사라진다.
    /// - [`Self::start`] 는 crashed 러너의 **재시작 경로**다. 그 경로가 패닉하면
    ///   `catch_unwind` + `crashed` 플래그로 만들어 둔 자기 복구 설계가 무력해진다.
    ///
    /// 근거 전문은 [`error-handling.md`](../../../docs/dev-guide/error-handling.md)
    /// "락 poison".
    fn lock_recovering(&self) -> std::sync::MutexGuard<'_, HashMap<u32, RunnerControl>> {
        // 첫-1 회 보고를 인라인으로 세워 두었던 자리 — 이제 공용 헬퍼가 같은 일을 한다
        // (`poison_reported` 는 그대로 이 인스턴스의 플래그로 넘긴다).
        crate::poison::recover_mutex(
            self.threads.lock(),
            "runner registry thread map",
            &self.poison_reported,
        )
    }

    pub fn new() -> Self {
        Self {
            threads: Mutex::new(HashMap::new()),
            poison_reported: AtomicBool::new(false),
        }
    }

    /// `true` = 이 호출이 러너를 새로 띄웠다. `false` 는 두 경우를 함께 뜻한다 —
    /// **이미 실행 중**(idempotent, 중복 start 는 no-op)이거나 **스레드 spawn 이
    /// 실패**했거나. 둘을 구분해야 하는 소비자는 이 반환값이 아니라 [`Self::status`]
    /// 를 읽는다(spawn 실패면 등록되지 않으므로 `running: false` 로 드러난다).
    pub fn start(&self, ctx: RunnerContext, workspace_id: u32) -> bool {
        let mut threads = self.lock_recovering();
        if let Some(ctrl) = threads.get(&workspace_id)
            && !ctrl.crashed.load(Ordering::Relaxed)
        {
            return false;
        }
        // crashed 인 경우 정리 후 재시작 허용.
        let (tx, rx) = mpsc::channel::<()>();
        let crashed = Arc::new(AtomicBool::new(false));
        let crashed_thread = crashed.clone();
        let list_failures = Arc::new(AtomicU32::new(0));
        let list_failures_thread = list_failures.clone();
        let ctx_thread = ctx.clone();
        let spawned = thread::Builder::new()
            .name(format!("agent-runner-ws{workspace_id}"))
            .spawn(move || {
                let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_loop(ctx_thread, workspace_id, rx, &list_failures_thread);
                }));
                if panicked.is_err() {
                    crashed_thread.store(true, Ordering::Relaxed);
                    tracing::error!(
                        "agent runner thread for workspace {workspace_id} panicked — \
                         marked crashed. Restart via agent.task_run start."
                    );
                }
            });
        // 스레드 한계·EAGAIN 으로 spawn 이 실패해도 호스트를 죽이지 않는다. 시작하지
        // 못한 것으로 보고(false)하고, crashed 재시작과 같은 경로로 다시 시도할 수 있게
        // 한다 — 등록하지 않으므로 다음 start 가 새로 만든다.
        let join = match spawned {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(
                    "failed to spawn agent-runner thread for workspace {workspace_id}: {e}"
                );
                return false;
            }
        };
        threads.insert(
            workspace_id,
            RunnerControl {
                stop_tx: tx,
                crashed,
                list_failures,
                join: Some(join),
            },
        );
        true
    }

    /// 정지 신호를 보내고 join. 이미 멈춰있으면 false.
    pub fn stop(&self, workspace_id: u32) -> bool {
        let mut threads = self.lock_recovering();
        if let Some(mut ctrl) = threads.remove(&workspace_id) {
            let _ = ctrl.stop_tx.send(()); // thread 가 panic 후 종료 시 Err — 의도적 무시
            if let Some(j) = ctrl.join.take() {
                let _ = j.join(); // catch_unwind 가 panic 흡수 — Err 일 일 거의 없음
            }
            return true;
        }
        false
    }

    /// 러너 스레드의 생사만 — `(running, crashed)`. task 카운트는 세지 않는다.
    ///
    /// [`Self::status`] 가 workspace 전체를 세는 것과 달리, 부분집합(예: DAG 하나)만
    /// 세야 하는 호출자는 카운트를 스스로 만들고 생사만 여기서 물어온다.
    pub fn liveness(&self, workspace_id: u32) -> (bool, bool) {
        let threads = self.lock_recovering();
        match threads.get(&workspace_id) {
            Some(ctrl) => (
                !ctrl.crashed.load(Ordering::Relaxed),
                ctrl.crashed.load(Ordering::Relaxed),
            ),
            None => (false, false),
        }
    }

    /// 현재 상태 + ready/running task 카운트. task list 는 호출자가 별도 제공
    /// (Core 측에 의존성 없음).
    pub fn status(&self, ctx: &RunnerContext, workspace_id: u32) -> RunnerStatus {
        let (running, crashed) = self.liveness(workspace_id);
        let list_failures = {
            let threads = self.threads.lock().expect("RunnerRegistry poisoned");
            threads
                .get(&workspace_id)
                .map_or(0, |c| c.list_failures.load(Ordering::Relaxed))
        };
        match count_ready_running(ctx, workspace_id) {
            Ok((ready_count, running_count)) => RunnerStatus {
                running,
                crashed,
                ready_count: Some(ready_count),
                running_count: Some(running_count),
                store_error: None,
                list_failures,
            },
            Err(e) => RunnerStatus {
                running,
                crashed,
                ready_count: None,
                running_count: None,
                store_error: Some(e),
                list_failures,
            },
        }
    }
}

impl Default for RunnerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// `(ready, running)` 카운트. **조회 실패를 `(0, 0)` 으로 흡수하지 않는다** —
/// 이 값이 `task_list`/`task_graph`/`task_run` 응답의 "정지 상태는 조회로
/// 드러난다" 계약(`docs/dev-guide/agent-runner.md`)을 지탱하므로, 못 읽었을 때
/// 0 을 돌려주면 그 계약이 거짓이 된다.
fn count_ready_running(ctx: &RunnerContext, workspace_id: u32) -> Result<(u32, u32), String> {
    ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let tasks = store.list(workspace_id).map_err(|e| e.to_string())?;
        let r = tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Ready))
            .count() as u32;
        let g = tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Running))
            .count() as u32;
        Ok((r, g))
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// S3 보강 ①: 호스트 재시작 후 *영속된* semaphore holder 정화.
///
/// in-memory `held_permits` 는 재시작 시 비어 있으므로 직전에 점유 중이던
/// permit 이 영구 leak 된다. workspace runner 시작 직전 1 회 수행:
///
/// 1. 해당 workspace 의 모든 Running task 를 load.
/// 2. `metadata.semaphore.holder == task.id` (컨벤션 강제) 인 항목만 정화 대상.
/// 3. 해당 holder 를 store 에서 release.
/// 4. task 자체를 `Failed("host restart")` 로 마감 — handle 유실 시나리오의
///    R3 정책과 일치 (사용자 retry).
fn purge_stale_semaphore_holders(ctx: &RunnerContext, workspace_id: u32) {
    let now = now_ms();
    let candidates: Vec<(String, String, String)> = ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let Ok(tasks) = store.list(workspace_id) else {
            return Vec::new();
        };
        tasks
            .into_iter()
            .filter(|t| matches!(t.state, TaskState::Running))
            .filter_map(|t| {
                let meta = t.metadata.get("semaphore")?.as_object()?;
                let name = meta.get("name")?.as_str()?.to_string();
                let holder = meta
                    .get("holder")
                    .and_then(|v| v.as_str())
                    .unwrap_or(t.id.as_str())
                    .to_string();
                // task.id == holder 컨벤션을 강제: 외부 도구가 임의 holder 로
                // acquire 한 항목은 runner 회수 대상 아님.
                if holder != *t.id.as_str() {
                    return None;
                }
                Some((t.id, name, holder))
            })
            .collect()
    });
    if candidates.is_empty() {
        return;
    }
    ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        {
            let mut sem = SemaphoreStore::new(mem, HOST_OWNER);
            for (_task_id, name, holder) in &candidates {
                let _ = sem.release(workspace_id, name, holder); // idempotent
            }
        }
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        for (task_id, _, _) in &candidates {
            // 정화는 best-effort — task 가 이미 다른 상태로 갔거나 store 가 일시
            // 오류 상태면 다음 사용자 retry 가 처리한다.
            if let Err(e) = store.set_result(
                workspace_id,
                task_id,
                TaskResult {
                    exit_code: None,
                    output: None,
                    error: Some("host restart".to_string()),
                },
            ) {
                tracing::warn!("purge set_result for {task_id} failed: {e}");
            }
            if let Err(e) = store.set_state(
                workspace_id,
                task_id,
                TaskState::Failed {
                    error: "host restart".to_string(),
                },
                now,
            ) {
                tracing::warn!("purge set_state for {task_id} failed: {e}");
            }
        }
    });
    tracing::info!(
        "agent runner ws{workspace_id}: purged {} stale semaphore holder(s) on restart",
        candidates.len()
    );
}

/// S1 보강: 호스트 재시작 후 *영속된* lease holder 정화.
///
/// semaphore purge 와 같은 정책 — `metadata.lease.holder == task.id` 인 Running task 만
/// 정화 대상. 그 외 (외부 도구 또는 사용자 명시 holder) 는 그대로 보존.
fn purge_stale_lease_holders(ctx: &RunnerContext, workspace_id: u32) {
    let now = now_ms();
    let candidates: Vec<(String, String, String)> = ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let Ok(tasks) = store.list(workspace_id) else {
            return Vec::new();
        };
        tasks
            .into_iter()
            .filter(|t| matches!(t.state, TaskState::Running))
            .filter_map(|t| {
                let meta = t.metadata.get("lease")?.as_object()?;
                let resource = meta.get("resource")?.as_str()?.to_string();
                let holder = meta
                    .get("holder")
                    .and_then(|v| v.as_str())
                    .unwrap_or(t.id.as_str())
                    .to_string();
                if holder != *t.id.as_str() {
                    return None;
                }
                Some((t.id, resource, holder))
            })
            .collect()
    });
    if candidates.is_empty() {
        return;
    }
    ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        {
            let mut lstore = LeaseStore::new(mem, HOST_OWNER);
            for (_task_id, resource, holder) in &candidates {
                let _ = lstore.release(workspace_id, resource, holder); // idempotent
            }
        }
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        for (task_id, _, _) in &candidates {
            if let Err(e) = store.set_result(
                workspace_id,
                task_id,
                TaskResult {
                    exit_code: None,
                    output: None,
                    error: Some("host restart".to_string()),
                },
            ) {
                tracing::warn!("purge(lease) set_result for {task_id} failed: {e}");
            }
            if let Err(e) = store.set_state(
                workspace_id,
                task_id,
                TaskState::Failed {
                    error: "host restart".to_string(),
                },
                now,
            ) {
                tracing::warn!("purge(lease) set_state for {task_id} failed: {e}");
            }
        }
    });
    tracing::info!(
        "agent runner ws{workspace_id}: purged {} stale lease holder(s) on restart",
        candidates.len()
    );
}

/// S3: 호스트 재시작 후 영속된 DispatchHandle 을 reload.
///
/// 각 영속 entry 에 대해:
/// - `task state == Running` 이 아닌 항목은 stale → 영속만 제거 (state 변경 X).
/// - `ShellProcess { pid }` : `process_alive::is_alive(pid)` 검사. alive → 복원,
///   dead → K.A-1 `run_result.<task_id>` 영속이 있으면 정확한 exit_code 로 Succeeded /
///   Failed 마감 (precise), 없으면 `Failed("host restart: pid {pid} died (exit_code unknown)")`.
/// - `PolledDispatch` / `BarrierPoll` : 그대로 복원 (insert only). 다음 정상 tick 에서 poll.
///   PolledDispatch 의 첫 poll 이 injector 미준비여도 K.A-2 grace (30s) 안에서는 Active 유지.
/// - `AwaitExternal { deadline_ms, .. }` : `deadline_ms` 가 이미 지났으면 즉시
///   `Failed("...deadline already expired")` 로 마감(구 포맷도 `deadline_ms` 기본값
///   0 이라 이 분기로 걸린다). 아직이면 그대로 복원 — `hook_wait` 매핑은 재시작으로
///   사라졌으므로 훅으로는 못 깨어나고, 다음 reload(다음 재시작) 때 다시 이
///   deadline 판정을 받는다(결정 4 — 계약: 재시작 후 hook_wait 은 비영속).
/// - `Immediate*` / `ImmediateFail` : 영속 대상 아니므로 도달 안 됨 (방어적 evict).
///
/// 반환: 복원할 (task_id, handle) 쌍 — RunnerLoop.running 에 insert.
fn reload_persistent_handles(
    ctx: &RunnerContext,
    workspace_id: u32,
) -> Vec<(TaskId, DispatchHandle)> {
    let now = now_ms();
    let scope = Scope::Workspace(workspace_id);
    let entries = ctx.with_memory(|mem| {
        let opts = ListOpts {
            prefix: Some(HANDLE_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        mem.list(&scope, &opts).unwrap_or_default()
    });
    let mut alive: Vec<(TaskId, DispatchHandle)> = Vec::new();
    let mut dead: Vec<(TaskId, String)> = Vec::new();
    let mut stale: Vec<TaskId> = Vec::new();
    // K.A-1: 직전 host 의 watcher 가 영속한 정확한 종료 결과 — exit_code 포함 마감.
    let mut precise: Vec<(TaskId, PollOutcome)> = Vec::new();

    for e in entries {
        match classify_persisted_handle(ctx, workspace_id, now, e) {
            HandleClassification::Alive(task_id, handle) => alive.push((task_id, handle)),
            HandleClassification::Dead(task_id, err) => dead.push((task_id, err)),
            HandleClassification::Stale(task_id) => stale.push(task_id),
            HandleClassification::Precise(task_id, outcome) => precise.push((task_id, outcome)),
        }
    }

    evict_stale_handles(ctx, &scope, &stale);
    mark_dead_tasks(ctx, workspace_id, &scope, now, &dead);
    finalize_precise_tasks(ctx, workspace_id, &scope, now, &precise);

    if !alive.is_empty() || !dead.is_empty() || !stale.is_empty() || !precise.is_empty() {
        tracing::info!(
            "agent runner ws{workspace_id}: reload handles — alive={}, dead={}, stale={}, precise={}",
            alive.len(),
            dead.len(),
            stale.len(),
            precise.len()
        );
    }
    alive
}

/// [`reload_persistent_handles`] 의 entry 분류 결과.
enum HandleClassification {
    Alive(TaskId, DispatchHandle),
    Dead(TaskId, String),
    Stale(TaskId),
    Precise(TaskId, PollOutcome),
}

/// 영속 entry 하나를 읽어 alive/dead/stale/precise 중 하나로 분류한다. 실제
/// memory/store mutation 은 하지 않는다 — 분류 결과에 따른 일괄 처리는
/// [`evict_stale_handles`]/[`mark_dead_tasks`]/[`finalize_precise_tasks`] 가 담당.
fn classify_persisted_handle(
    ctx: &RunnerContext,
    workspace_id: u32,
    now: u64,
    e: tasty_memory::MemoryEntry,
) -> HandleClassification {
    let task_id = e
        .key
        .strip_prefix(HANDLE_KEY_PREFIX)
        .unwrap_or(&e.key)
        .to_string();
    let MemoryValue::Json(v) = e.value else {
        return HandleClassification::Stale(task_id);
    };
    let handle: DispatchHandle = match serde_json::from_value(v) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("reload handle {task_id} deserialize: {e}");
            return HandleClassification::Stale(task_id);
        }
    };

    // task state 가 Running 이 아니면 영속만 정리. R-1: Running 이 아닌데 handle 영속이
    // 남아 있으면 다음 tick 의 Ready 분기가 *재* dispatch 할 수 있다.
    let state_opt: Option<TaskState> = ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store
            .get(workspace_id, &task_id)
            .ok()
            .flatten()
            .map(|t| t.state)
    });
    if !matches!(state_opt, Some(TaskState::Running)) {
        return HandleClassification::Stale(task_id);
    }

    match &handle {
        DispatchHandle::ShellProcess { pid } => {
            if tasty_agent::platform::process_alive::is_alive(*pid) {
                HandleClassification::Alive(task_id, handle)
            } else if let Some(outcome) = load_run_result(ctx, workspace_id, &task_id) {
                // K.A-1: 직전 host watcher 가 exit_code 까지 영속해 둠 → 정확 마감.
                HandleClassification::Precise(task_id, outcome)
            } else {
                HandleClassification::Dead(
                    task_id,
                    format!("host restart: pid {pid} died (exit_code unknown)"),
                )
            }
        }
        // AwaitExternal 의 deadline 이 이미 지났으면 — `hook_wait` 매핑(재시작 시
        // 비영속)이 사라져 훅으로는 더 이상 깨어날 수 없으므로, handle 에 실린
        // deadline(결정 4)으로 즉시 마감한다. 구 포맷(deadline_ms 없음 → 0)도 이
        // 분기로 걸려 즉시 만료 처리된다.
        DispatchHandle::AwaitExternal { deadline_ms, .. } if *deadline_ms <= now => {
            HandleClassification::Dead(
                task_id,
                "host restart: push completion strategy deadline already expired".to_string(),
            )
        }
        // PolledDispatch/Barrier: 그대로 복원 — 다음 정상 tick 에서 poll.
        // PolledDispatch 의 첫 poll 이 injector 미준비로 실패하면 task=Failed (R3 정책).
        //
        // AwaitExternal(미만료) 도 동형으로 복원한다 — poll 은 항상
        // Active 인 no-op 이지만, 진짜 종결은 `self.running` 과 무관하게
        // store 를 직접 전이시키는 외부 경로가 담당한다. 여기서 복원하지
        // 않으면(= stale 로 evict) 그 외부 완료가 도착했을 때 0단계 terminal
        // 흡수가 handle 을 찾지 못해 `release_permit` 이 누락된다 — 이
        // variant 를 도입한 목적(permit 누수 방지)이 재시작 시나리오에서
        // 깨지는 것이므로 반드시 복원해야 한다. `hook_wait` 매핑은 재시작 시
        // 사라지므로 이 task 는 훅으로는 깨어나지 못하고, 이후 매 tick 의
        // `expire_overdue_hook_waits` 도 이 task 를 더는 추적하지 못한다 —
        // 대신 다음 reload(재-재시작) 때 이 분기가 deadline 으로 마감한다.
        DispatchHandle::PolledDispatch { .. }
        | DispatchHandle::BarrierPoll { .. }
        | DispatchHandle::AwaitExternal { .. } => HandleClassification::Alive(task_id, handle),
        // Immediate* / ImmediateFail 은 영속 대상 아님 — 도달 시 방어적 evict.
        DispatchHandle::ReduceImmediate(_)
        | DispatchHandle::CustomImmediate(_)
        | DispatchHandle::ImmediateFail(_) => HandleClassification::Stale(task_id),
    }
}

/// stale entry: 영속만 제거 (task state 는 건드리지 않음).
fn evict_stale_handles(ctx: &RunnerContext, scope: &Scope, stale: &[TaskId]) {
    if stale.is_empty() {
        return;
    }
    ctx.with_memory(|mem| {
        for tid in stale {
            let _ = mem.delete(HOST_OWNER, scope, &handle_key(tid), None); // best-effort
        }
    });
}

/// dead entry: task=Failed 로 마감 + 영속 evict.
fn mark_dead_tasks(
    ctx: &RunnerContext,
    workspace_id: u32,
    scope: &Scope,
    now: u64,
    dead: &[(TaskId, String)],
) {
    if dead.is_empty() {
        return;
    }
    ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            for (task_id, err) in dead {
                if let Err(e) = store.set_result(
                    workspace_id,
                    task_id,
                    TaskResult {
                        exit_code: None,
                        output: None,
                        error: Some(err.clone()),
                    },
                ) {
                    tracing::warn!("reload mark failed set_result {task_id}: {e}");
                }
                if let Err(e) = store.set_state(
                    workspace_id,
                    task_id,
                    TaskState::Failed { error: err.clone() },
                    now,
                ) {
                    tracing::warn!("reload mark failed set_state {task_id}: {e}");
                }
            }
        }
        for (task_id, _) in dead {
            let _ = mem.delete(HOST_OWNER, scope, &handle_key(task_id), None); // best-effort evict — 실패 시 다음 reload 가 stale 처리
        }
    });
}

/// K.A-1 precise entry: 영속된 exit_code 로 정확히 마감 (Succeeded / Failed 분류).
fn finalize_precise_tasks(
    ctx: &RunnerContext,
    workspace_id: u32,
    scope: &Scope,
    now: u64,
    precise: &[(TaskId, PollOutcome)],
) {
    if precise.is_empty() {
        return;
    }
    ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            for (task_id, outcome) in precise {
                apply_precise_outcome(&mut store, workspace_id, task_id, outcome, now);
            }
        }
        evict_precise_handles(mem, scope, precise);
    });
    for (task_id, _) in precise {
        evict_run_result(ctx, workspace_id, task_id);
    }
}

/// precise entry 하나를 정확한 exit_code 로 마감(Succeeded/Failed). `Active` 는
/// 도달 안 함(watcher 는 종결 시점만 영속) — no-op.
fn apply_precise_outcome(
    store: &mut TaskStore,
    workspace_id: u32,
    task_id: &TaskId,
    outcome: &PollOutcome,
    now: u64,
) {
    let (result, next_state) = match outcome {
        PollOutcome::Done(r) => (r.clone(), TaskState::Succeeded),
        PollOutcome::Failed(err) => (
            TaskResult {
                exit_code: None,
                output: None,
                error: Some(err.clone()),
            },
            TaskState::Failed { error: err.clone() },
        ),
        PollOutcome::Active => return,
    };
    if let Err(e) = store.set_result(workspace_id, task_id, result) {
        tracing::warn!("reload precise set_result {task_id}: {e}");
    }
    if let Err(e) = store.set_state(workspace_id, task_id, next_state, now) {
        tracing::warn!("reload precise set_state {task_id}: {e}");
    }
}

/// handle + run_result 둘 다 evict (정확히 마감된 task 의 잔재 제거).
/// best-effort — 실패 시 다음 reload 가 stale 분기로 정리.
fn evict_precise_handles(
    mem: &mut dyn tasty_memory::MemoryStorage,
    scope: &Scope,
    precise: &[(TaskId, PollOutcome)],
) {
    for (task_id, _) in precise {
        if let Err(e) = mem.delete(HOST_OWNER, scope, &handle_key(task_id), None) {
            tracing::warn!("reload precise evict handle {task_id}: {e}");
        }
    }
}

/// push 완료 전략의 timeout 안전망. `HookTaskWaits` 에
/// 등록된 대기 중 deadline 이 지난 항목을 Failed 로 강제 마감한다. 보고(훅
/// 발화) 유실 시 task 가 영구 Running 에 남는 것을 막는 유일한 장치 — poll 은
/// 여전히 관여하지 않는다(`AwaitExternal` 계약 불변, `runner_host.rs` 의
/// `poll_handle` 참조), 이 sweep 이 "외부에서 store 를 직접 전이" 경로로
/// 종결시킨다.
///
/// `HookTaskWaits` 는 워크스페이스 무관 전역 맵이다 — 어느 workspace 의 runner
/// thread 든 자기 tick 에 편승해 이 sweep 을 돌려도 안전하다: memory store 는
/// `Core` 전역에서 공유되는 같은 `Arc<Mutex<>>`(`TaskStore::set_state` 가
/// workspace_id 를 인자로 받아 정확한 scope 에 쓴다), `sweep_expired` 자체가
/// 원자적 remove 라 여러 thread 가 동시에 돌아도 항목이 중복 처리되지 않는다.
fn expire_overdue_hook_waits(ctx: &RunnerContext, now_ms: u64) {
    let overdue = ctx.hook_task_waits.sweep_expired(now_ms);
    for (workspace_id, task_id) in overdue {
        let error = "push completion strategy timed out waiting for external report".to_string();
        let fire_target = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let result = TaskResult {
                exit_code: None,
                output: None,
                error: Some(error.clone()),
            };
            if let Err(e) = store.set_result(workspace_id, &task_id, result) {
                tracing::warn!("hook wait timeout: set_result {task_id} failed: {e}");
                return None;
            }
            match store.set_state(
                workspace_id,
                &task_id,
                TaskState::Failed {
                    error: error.clone(),
                },
                now_ms,
            ) {
                Ok((task, _downstream)) => Some((task.state, task.result)),
                Err(e) => {
                    tracing::warn!("hook wait timeout: set_state {task_id} failed: {e}");
                    None
                }
            }
        });
        if let Some((state, result)) = fire_target {
            ctx.task_waker_hub.fire(
                workspace_id,
                &task_id,
                crate::core::agent::task_waker::TerminalSnapshot { state, result },
            );
        }
        tracing::warn!(
            "agent task {task_id} (ws {workspace_id}): push completion strategy timed out \
             waiting for external report — marked Failed"
        );
    }
}

/// 재시작 정화 3종 세트 — semaphore/lease holder 회수(+ 해당 task
/// `Failed("host restart")` 마감) 및 persisted `DispatchHandle` reload. 원래
/// `run_loop` 진입부에 있던 로직을 분리한 것 — runner thread 없이도(부팅 경로,
/// [`purge_stale_agent_state_on_boot`]) 호출 가능해야 하기 때문이다.
///
/// 반환값은 reload 로 되살아난 (task_id, handle) 목록 — runner 가 있으면
/// `RunnerLoop.running` 에 삽입해 이어서 poll 한다. 부팅 경로처럼 runner 가
/// 없으면 그냥 버려도 안전하다: `alive` 분류는 부수효과가 없고(핸들을 그대로
/// 반환할 뿐), 다음 수동 `agent.task_run --action start` 가 같은 handle 목록을
/// 다시 reload 해 `RunnerLoop.running` 에 넣는다. `dead`/`stale`/`precise` 분류는
/// 이미 이 호출에서 마감·evict 됐으므로 재호출(수동 start 시 run_loop 진입)해도
/// 대상이 남아있지 않아 no-op — 즉 이 함수는 여러 번 호출해도 안전(idempotent)
/// 하다.
fn purge_and_reload_on_restart(
    ctx: &RunnerContext,
    workspace_id: u32,
) -> Vec<(TaskId, DispatchHandle)> {
    purge_stale_semaphore_holders(ctx, workspace_id);
    purge_stale_lease_holders(ctx, workspace_id);
    reload_persistent_handles(ctx, workspace_id)
}

/// 결정 2: 부팅 시 1회 — 라이브 workspace 전부에 재시작 정화만 적용하고 runner
/// thread 는 켜지 않는다(결정 1: 자동 시작 없음). `workspace_ids` 는 호출자가
/// 라이브 workspace 목록에서 그대로 넘긴다 — task 가 없는 workspace 는 내부
/// 정화 함수들이 candidates 없음으로 조기 반환하므로 별도 필터링 불필요(이미
/// 사라진 workspace id 는애초에 이 목록에 없으므로 자연히 제외된다 — "라이브
/// workspace ∩ task 보유 workspace" 교집합과 동치).
pub(crate) fn purge_stale_agent_state_on_boot(ctx: &RunnerContext, workspace_ids: &[u32]) {
    for &workspace_id in workspace_ids {
        // reload 결과(되살아난 handle)는 버린다 — 이 시점엔 그걸 넘겨받아
        // poll 할 runner 가 없다. 다음 수동 start 가 다시 reload 한다.
        let _ = purge_and_reload_on_restart(ctx, workspace_id);
        // 자동 GC 도 같은 부팅 정화 경로에 얹는다.
        gc_stale_tasks(ctx, workspace_id);
    }
}

/// 자동 GC 임계값(잠정치, provisional). 사용자가 수동으로
/// 지우지 않은 task 는 보통 며칠 안에 확인한다는 추정으로 7일을 잡았다 — 실사용
/// 데이터가 쌓이면 재검토 대상(설정 가능하게 노출하는 것도 후보).
const AGENT_TASK_GC_MIN_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// 부팅 시 정화 경로에 얹는 자동 GC(`docs/dev-guide/agent-runner.md` "자동 GC").
/// `PutOpts.expires_at` 류의
/// memory 자체 TTL 은 쓰지 않는다: task 삭제는 참조 무결성(결정 1)·Running 거부
/// (결정 2) 검사를 반드시 거쳐야 하는데, TTL 만료는 그 검사를 우회한 채 그냥
/// 지워버려 dangling 참조/자원 누수를 재도입하기 때문이다 — 그래서 항상 검증된
/// 경로(`TaskStore::plan_sweep`/`apply_sweep_plan`, `Core::task_purge` 와 동일
/// 로직)만 태운다. 상태를 terminal 로 제한하지 않는 이유는 결정 2 의 근거와
/// 같다 — 방치된 `Waiting` task(예: 미완 Reduce 입력)를 terminal 로 한정하면
/// 영원히 못 지우고, 그게 참조로 자기 입력들을 붙잡아 그 입력들도 GC 대상에서
/// 빠진다. `Running` 은 `plan_sweep` 이 항상 제외하므로 여기서 따로 막지 않는다.
fn gc_stale_tasks(ctx: &RunnerContext, workspace_id: u32) {
    let Some(plan) = gc_plan_sweep(ctx, workspace_id) else {
        return;
    };
    if plan.deleted.is_empty() {
        return;
    }
    if !gc_apply_sweep_plan(ctx, workspace_id, &plan) {
        return;
    }
    for id in &plan.deleted {
        evict_task_side_keys(ctx, workspace_id, id);
    }
    tracing::info!(
        "agent runner ws{workspace_id}: GC swept {} stale task(s), {} retained (still referenced)",
        plan.deleted.len(),
        plan.retained.len()
    );
}

/// [`gc_stale_tasks`] 의 후보 계산 단계. 실패 시 `None`(로그만 남기고 이번
/// workspace 는 건너뜀 — 다음 부팅 때 재시도).
fn gc_plan_sweep(
    ctx: &RunnerContext,
    workspace_id: u32,
) -> Option<tasty_agent::task::TaskSweepPlan> {
    let filter = tasty_agent::task::TaskPurgeFilter {
        states: None,
        older_than_ms: Some(AGENT_TASK_GC_MIN_AGE_MS),
        now_ms: now_ms(),
    };
    let seq = ctx.agent_seq.clone();
    let plan = ctx.with_memory(|mem| {
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.plan_sweep(workspace_id, &filter)
    });
    match plan {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!("agent task GC ws{workspace_id}: plan_sweep failed: {e}");
            None
        }
    }
}

/// [`gc_stale_tasks`] 의 실제 삭제 단계. 성공 여부를 반환(실패 시 로그만).
fn gc_apply_sweep_plan(
    ctx: &RunnerContext,
    workspace_id: u32,
    plan: &tasty_agent::task::TaskSweepPlan,
) -> bool {
    let seq = ctx.agent_seq.clone();
    let res = ctx.with_memory(|mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.apply_sweep_plan(workspace_id, plan)
    });
    match res {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("agent task GC ws{workspace_id}: apply_sweep_plan failed: {e}");
            false
        }
    }
}

/// tick 머리의 task snapshot 조회. 조회 실패를 **빈 목록으로 흡수하지 않고** 로그로
/// 남긴다(`store_list_failures` 는 연속 실패 카운터 — rate-limit 판정에 쓰인다).
///
/// `tick` 은 넘겨받은 슬라이스만 순회하므로(terminal 흡수 · Running poll · Ready
/// dispatch 전부) 빈 목록으로 진행하는 것과 이번 tick 을 건너뛰는 것의 **동작은
/// 완전히 같다** — permit 회수도 snapshot 에서 terminal 로 바뀐 task 를 봤을 때만
/// 일어난다. 그래서 여기서 바꾸는 것은 관측 가능성뿐이고, 흐름은 종전대로 빈
/// snapshot 으로 tick 을 돌린다(다음 tick 에 회복되면 밀린 전이를 한꺼번에 흡수).
fn tick_snapshot(
    ctx: &RunnerContext,
    workspace_id: u32,
    store_list_failures: &AtomicU32,
) -> Vec<tasty_agent::Task> {
    let listed = ctx.with_memory(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.list(workspace_id)
    });
    let e = match listed {
        Ok(tasks) => {
            let prev = store_list_failures.swap(0, Ordering::Relaxed);
            if prev > 0 {
                tracing::info!(
                    "agent runner ws{workspace_id}: task store recovered after {prev} \
                     consecutive list failures"
                );
            }
            return tasks;
        }
        Err(e) => e,
    };
    let n = store_list_failures
        .load(Ordering::Relaxed)
        .saturating_add(1);
    store_list_failures.store(n, Ordering::Relaxed);
    log_store_list_failure(workspace_id, n, &e);
    Vec::new()
}

/// [`tick_snapshot`] 의 실패 로그 — 레벨 판정은 [`store_list_failure_log`], 여기서는
/// 실제 기록만 한다(호출부의 인지 복잡도 상한 때문에 분리).
fn log_store_list_failure(workspace_id: u32, n: u32, e: &dyn std::fmt::Display) {
    match store_list_failure_log(n) {
        StoreListFailureLog::Warn => tracing::warn!(
            "agent runner ws{workspace_id}: task store list failed: {e} \
             — this tick advances no task"
        ),
        StoreListFailureLog::Error => tracing::error!(
            "agent runner ws{workspace_id}: task store list failed {n} times in a row: {e} \
             — the task DAG is stalled (no dispatch, no poll, no permit release) while the UI \
             still shows Ready/Running"
        ),
        StoreListFailureLog::Silent => {}
    }
}

fn run_loop(
    ctx: RunnerContext,
    workspace_id: u32,
    stop_rx: mpsc::Receiver<()>,
    list_failures: &AtomicU32,
) {
    let reloaded = purge_and_reload_on_restart(&ctx, workspace_id);
    let executor = HostExecutor::new(ctx.clone());
    let mut runner = RunnerLoop::new(executor);
    for (task_id, handle) in reloaded {
        runner.running.insert(task_id, handle);
    }
    loop {
        // 0. push 완료 전략 timeout 안전망 — tick 본문보다
        //    먼저 돌려 이번 tick 의 0단계(terminal 흡수)가 방금 Failed 된 task 의
        //    handle 정리 + release_permit 까지 같은 루프에서 마무리하게 한다.
        let now = now_ms();
        expire_overdue_hook_waits(&ctx, now);

        // 1. tick 본문.
        let snapshot = tick_snapshot(&ctx, workspace_id, list_failures);

        let ctx_for_set = ctx.clone();
        let ctx_for_res = ctx.clone();
        runner.tick(
            workspace_id,
            now,
            &snapshot,
            move |ws, id, st, n| {
                let (res, fire_target) = ctx_for_set.with_memory(|mem| {
                    let seq = ctx_for_set.agent_seq.clone();
                    let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
                    match store.set_state(ws, id, st, n) {
                        Ok((task, _downstream)) => {
                            // S5: 종결 전이 시 hub fire 를 위한 snapshot 채취.
                            let fire = if task.state.is_terminal() {
                                Some((task.state.clone(), task.result.clone()))
                            } else {
                                None
                            };
                            (Ok(()), fire)
                        }
                        Err(e) => (Err(e), None),
                    }
                });
                if let Some((state, result)) = fire_target {
                    ctx_for_set.task_waker_hub.fire(
                        ws,
                        id,
                        crate::core::agent::task_waker::TerminalSnapshot { state, result },
                    );
                }
                res
            },
            move |ws, id, r| {
                ctx_for_res.with_memory(|mem| {
                    let seq = ctx_for_res.agent_seq.clone();
                    let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
                    store.set_result(ws, id, r).map(|_| ())
                })
            },
        );

        // 2. sleep + stop check.
        match stop_rx.recv_timeout(TICK_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::runner_host::run_result_key;
    use super::*;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicU64;
    use tasty_agent::task::TaskCreateOpts;
    use tasty_agent::{OnFailure, TaskCommand};
    use tasty_memory::{MemoryStore, PutOpts};

    // 조회 실패 로그가 tick 마다 쏟아지지 않는지(첫 실패 warn → 임계에서 error →
    // 이후 주기적으로만 error). TICK_INTERVAL 이 500ms 라 정책이 무너지면 초당
    // 2 줄이 무한히 쌓인다.
    #[test]
    fn store_list_failure_log_is_rate_limited() {
        assert_eq!(store_list_failure_log(1), StoreListFailureLog::Warn);
        for n in 2..STORE_LIST_ERROR_AFTER {
            assert_eq!(
                store_list_failure_log(n),
                StoreListFailureLog::Silent,
                "{n} 번째 실패는 첫 warn 과 error 임계 사이라 조용해야 한다"
            );
        }
        assert_eq!(
            store_list_failure_log(STORE_LIST_ERROR_AFTER),
            StoreListFailureLog::Error
        );
        // 임계 직후부터 다음 주기 직전까지는 다시 조용하다.
        for n in (STORE_LIST_ERROR_AFTER + 1)..(STORE_LIST_ERROR_AFTER + STORE_LIST_REPEAT_EVERY) {
            assert_eq!(
                store_list_failure_log(n),
                StoreListFailureLog::Silent,
                "{n}"
            );
        }
        assert_eq!(
            store_list_failure_log(STORE_LIST_ERROR_AFTER + STORE_LIST_REPEAT_EVERY),
            StoreListFailureLog::Error
        );
        assert_eq!(
            store_list_failure_log(STORE_LIST_ERROR_AFTER + 2 * STORE_LIST_REPEAT_EVERY),
            StoreListFailureLog::Error
        );
    }

    fn fresh_ctx() -> (tempfile::TempDir, RunnerContext) {
        let td = tempfile::tempdir().unwrap();
        let mem = MemoryStore::open(&td.path().join("mem.db")).unwrap();
        let ctx = RunnerContext {
            memory: Arc::new(Mutex::new(mem)),
            agent_seq: Arc::new(AtomicU64::new(0)),
            host_ipc: Arc::new(OnceLock::new()),
            task_waker_hub: Arc::new(crate::core::agent::task_waker::TaskWakerHub::new()),
            hook_task_waits: Arc::new(crate::core::agent::hook_wait::HookTaskWaits::new()),
        };
        (td, ctx)
    }

    fn put_handle(ctx: &RunnerContext, ws: u32, task_id: &str, handle: &DispatchHandle) {
        ctx.with_memory(|mem| {
            let value = MemoryValue::Json(serde_json::to_value(handle).unwrap());
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(ws),
                &handle_key(task_id),
                &value,
                &PutOpts::default(),
            )
            .unwrap();
        });
    }

    /// J.A.S3: 현 프로세스 pid 로 ShellProcess handle 영속 + reload → 복원.
    /// **배선 테스트** — 정책 함수(`store_list_failure_log`)가 tick 루프에 실제로
    /// 연결돼 있는지. 순수 정책만 검증하면 카운터를 올리지 않는 배선(그래서 `error`
    /// 승격이 영원히 일어나지 않고 회복 로그도 사라지는 상태)이 그대로 통과한다.
    /// 호출부를 옛 `unwrap_or_default()` 로 되돌리는 변이는 dead-code 린트가 잡지만
    /// **컴파일러가 잡는 것은 테스트가 아니다** — 함수를 전부 살려둔 채 배선만
    /// 망가뜨리는 변이(카운터 미증가 · 회복 리셋 제거)에서 이 테스트가 실패해야 한다.
    #[test]
    fn tick_snapshot_counts_consecutive_failures_and_resets_on_recovery() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
        });
        let failures = AtomicU32::new(0);

        // 정상 경로에서는 카운터가 0 을 유지하고 snapshot 이 실제로 온다.
        assert_eq!(tick_snapshot(&ctx, 1, &failures).len(), 1);
        assert_eq!(failures.load(Ordering::Relaxed), 0);

        // 실제 기록된 task 키 자리에 Task 로 역직렬화되지 않는 값을 덮어 조회를
        // 실패시킨다(키 접두사를 테스트에 하드코딩하지 않으려고 런타임에 찾는다).
        let corrupt_key = ctx.with_memory(|mem| {
            let entries = mem
                .list(&Scope::Workspace(1), &ListOpts::default())
                .expect("list");
            let key = entries
                .iter()
                .map(|e| e.key.clone())
                .find(|k| k.contains("task"))
                .expect("task key");
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(1),
                &key,
                &MemoryValue::Json(serde_json::json!({ "not": "a task" })),
                &PutOpts::default(),
            )
            .expect("put");
            key
        });

        // 연속 실패는 **누적**돼야 한다 — 이 값이 `error` 승격 임계를 결정한다.
        for expected in 1..=3u32 {
            assert!(
                tick_snapshot(&ctx, 1, &failures).is_empty(),
                "조회 실패면 빈 snapshot"
            );
            assert_eq!(
                failures.load(Ordering::Relaxed),
                expected,
                "연속 실패가 누적되지 않으면 error 승격이 영원히 일어나지 않는다"
            );
        }

        // 회복하면 0 으로 리셋 — 다음 실패가 다시 1 회째부터 세어진다.
        ctx.with_memory(|mem| {
            mem.delete(HOST_OWNER, &Scope::Workspace(1), &corrupt_key, None)
                .expect("delete");
        });
        assert!(tick_snapshot(&ctx, 1, &failures).is_empty());
        assert_eq!(
            failures.load(Ordering::Relaxed),
            0,
            "회복하면 카운터가 리셋돼야 한다"
        );
    }

    /// store 를 못 읽으면 `status` 는 카운트를 **0 이 아니라 `None`** 으로 낸다.
    /// 0 은 "task 가 없다" 와 값이 같아, `agent-runner.md` 가 계약으로 못박은
    /// "정지 상태는 조회로 드러난다" 가 거짓이 된다.
    #[test]
    fn status_reports_unknown_counts_when_the_task_store_is_unreadable() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
        });
        let registry = RunnerRegistry::new();
        let healthy = registry.status(&ctx, 1);
        assert_eq!(healthy.ready_count, Some(1));
        assert!(healthy.store_error.is_none());

        // 실제로 기록된 task 키를 읽어와 그 자리에 Task 로 역직렬화되지 않는 값을
        // 덮는다 — 키 접두사를 테스트에 하드코딩하지 않으려고 런타임에 찾는다.
        ctx.with_memory(|mem| {
            let entries = mem
                .list(&Scope::Workspace(1), &ListOpts::default())
                .expect("list");
            let key = entries
                .iter()
                .map(|e| e.key.clone())
                .find(|k| k.contains("task"))
                .expect("task key");
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(1),
                &key,
                &MemoryValue::Json(serde_json::json!({ "not": "a task" })),
                &PutOpts::default(),
            )
            .expect("put");
        });

        let broken = registry.status(&ctx, 1);
        assert_eq!(broken.ready_count, None, "못 읽었으면 0 이 아니라 unknown");
        assert_eq!(broken.running_count, None);
        assert!(
            broken.store_error.is_some(),
            "카운트가 없는 이유가 응답에 실려야 한다"
        );
    }

    /// 응답 계약에 실린 `list_failures` 가 **실제 러너 스레드의 카운터를 따라가는지**.
    ///
    /// `tick_snapshot_counts_consecutive_failures_and_resets_on_recovery` 는 정책 함수만
    /// 보고 `status_reports_unknown_counts_when_the_task_store_is_unreadable` 는 러너를
    /// 띄우지 않는다 — 그래서 `status()` 가 이 필드를 항상 0 으로 내는 변이가 잡히지
    /// 않았다(Gate4 A3′). 이 필드는 "러너가 살아는 있는데 아무것도 못 읽고 있다" 를
    /// 조회로 드러내는 유일한 신호라, 배선이 비면 계약이 거짓이 된다.
    #[test]
    fn status_reports_the_running_runner_consecutive_list_failures() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
        });
        // 기록된 task 자리에 Task 로 역직렬화되지 않는 값을 덮어 store.list 를 깨뜨린다
        // (`status_reports_unknown_counts_…` 와 같은 기법 — 키를 하드코딩하지 않는다).
        ctx.with_memory(|mem| {
            let entries = mem
                .list(&Scope::Workspace(1), &ListOpts::default())
                .expect("list");
            let key = entries
                .iter()
                .map(|e| e.key.clone())
                .find(|k| k.contains("task"))
                .expect("task key");
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(1),
                &key,
                &MemoryValue::Json(serde_json::json!({ "not": "a task" })),
                &PutOpts::default(),
            )
            .expect("put");
        });

        let registry = RunnerRegistry::new();
        assert!(registry.start(ctx.clone(), 1), "러너가 새로 떠야 한다");

        // 첫 tick 은 recv_timeout 앞에서 즉시 돈다 — 넉넉히 기다리되(러너 스레드
        // 스케줄링) 무한 대기는 하지 않는다.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut observed = 0;
        while std::time::Instant::now() < deadline {
            let st = registry.status(&ctx, 1);
            if st.list_failures > 0 {
                observed = st.list_failures;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let final_status = registry.status(&ctx, 1);
        registry.stop(1);

        assert!(
            observed > 0,
            "러너가 store 를 못 읽고 있으면 status 가 그 횟수를 드러내야 한다 \
             (running={}, store_error={:?})",
            final_status.running,
            final_status.store_error
        );
        assert!(
            final_status.running,
            "스레드는 살아 있는데 아무것도 못 읽는 상태 — 그게 이 필드가 드러내는 것이다"
        );
    }

    #[test]
    fn reload_persistent_handles_restores_alive_shell_process() {
        let (_td, ctx) = fresh_ctx();
        // task 가 Running 상태여야 reload 대상이 됨.
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });
        let my_pid = std::process::id();
        let handle = DispatchHandle::ShellProcess { pid: my_pid };
        put_handle(&ctx, 1, &task_id, &handle);

        let alive = reload_persistent_handles(&ctx, 1);
        assert_eq!(alive.len(), 1, "live pid should restore");
        assert_eq!(alive[0].0, task_id);
    }

    /// J.A.S3: dead pid → task=Failed("host restart: pid X died") + handle evict.
    #[test]
    fn reload_persistent_handles_marks_dead_pid_as_failed() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });
        // 0xFFFF_FFFE 은 거의 확실히 살아있지 않은 pid.
        let dead_pid: u32 = 0xFFFF_FFFE;
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::ShellProcess { pid: dead_pid },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty(), "dead pid should not restore");

        // task state 확인.
        let final_state: TaskState = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        match final_state {
            TaskState::Failed { error } => assert!(
                error.contains("host restart") && error.contains("died"),
                "unexpected error: {error}"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }

        // handle 도 evict 됐는지.
        let still_there: bool = ctx.with_memory(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "dead pid handle should be evicted");
    }

    fn put_run_result(ctx: &RunnerContext, ws: u32, task_id: &str, value: serde_json::Value) {
        ctx.with_memory(|mem| {
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(ws),
                &run_result_key(task_id),
                &MemoryValue::Json(value),
                &PutOpts::default(),
            )
            .unwrap();
        });
    }

    fn make_running_run_task(ctx: &RunnerContext, ws: u32) -> TaskId {
        ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: ws,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: ws,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store
                .set_state(ws, &t.id, TaskState::Running, 1100)
                .unwrap();
            t.id
        })
    }

    /// K.A.1: dead pid + run_result(done, exit_code=0) → task=Succeeded + precise 마감.
    #[test]
    fn reload_shell_process_with_persisted_done_result_succeeds() {
        let (_td, ctx) = fresh_ctx();
        let task_id = make_running_run_task(&ctx, 1);
        let dead_pid: u32 = 0xFFFF_FFFE;
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::ShellProcess { pid: dead_pid },
        );
        put_run_result(
            &ctx,
            1,
            &task_id,
            serde_json::json!({
                "kind": "done",
                "exit_code": 0,
                "output": { "pid": dead_pid },
                "error": null,
            }),
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty(), "precise should not restore to alive");

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        assert!(
            matches!(task.state, TaskState::Succeeded),
            "got {:?}",
            task.state
        );
        let result = task.result.expect("result present");
        assert_eq!(result.exit_code, Some(0));

        // handle + run_result 모두 evict.
        let handle_present: bool = ctx.with_memory(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!handle_present, "handle evicted after precise");
        let result_present: bool = ctx.with_memory(|mem| {
            mem.get(&Scope::Workspace(1), &run_result_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!result_present, "run_result evicted after precise");
    }

    /// K.A.1: dead pid + run_result(failed) → task=Failed(원본 메시지) + evict.
    #[test]
    fn reload_shell_process_with_persisted_failed_result_marks_failed() {
        let (_td, ctx) = fresh_ctx();
        let task_id = make_running_run_task(&ctx, 1);
        let dead_pid: u32 = 0xFFFF_FFFE;
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::ShellProcess { pid: dead_pid },
        );
        put_run_result(
            &ctx,
            1,
            &task_id,
            serde_json::json!({
                "kind": "failed",
                "error": "Run exited non-zero: code=Some(2)",
            }),
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty());

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        match &task.state {
            TaskState::Failed { error } => assert!(error.contains("non-zero"), "got {error}"),
            other => panic!("expected Failed, got {other:?}"),
        }
        // 기존 unknown 메시지가 아니라 watcher 가 영속한 정확한 메시지여야 함.
        assert!(
            !matches!(&task.state, TaskState::Failed { error } if error.contains("unknown")),
            "should not be 'unknown' message",
        );
    }

    /// K.A.1: dead pid + run_result 없음 → 기존 동작 (unknown Failed).
    #[test]
    fn reload_shell_process_dead_pid_without_run_result_falls_back_to_unknown() {
        let (_td, ctx) = fresh_ctx();
        let task_id = make_running_run_task(&ctx, 1);
        let dead_pid: u32 = 0xFFFF_FFFE;
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::ShellProcess { pid: dead_pid },
        );
        // run_result 없음.

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty());

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        match &task.state {
            TaskState::Failed { error } => {
                assert!(error.contains("exit_code unknown"), "got {error}")
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// J.A.S3: task state != Running 인 handle → stale 처리 (영속만 evict, state X).
    #[test]
    fn reload_persistent_handles_evicts_stale_when_task_not_running() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            // Ready state 유지 (Running 으로 전이 안 함).
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap()
                .id
        });
        let handle = DispatchHandle::ShellProcess {
            pid: std::process::id(),
        };
        put_handle(&ctx, 1, &task_id, &handle);

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty(), "non-Running task handle is stale");

        let still_there: bool = ctx.with_memory(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "stale handle should be evicted");

        // task state 는 그대로 (Ready) — 변경하면 안 됨.
        let state: TaskState = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        assert!(matches!(state, TaskState::Ready), "got {state:?}");
    }

    /// 결정 4: 재시작 reload 시점에 `AwaitExternal.deadline_ms` 가 이미 지났으면
    /// (`hook_wait` 매핑은 재시작으로 사라져 훅으로 못 깨어난다) 즉시 Failed 로
    /// 마감하고 handle 을 evict — 영구 Running 으로 남지 않는다.
    #[test]
    fn reload_persistent_handles_fails_await_external_past_deadline() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Custom {
                        ipc_method: "acme.start".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });
        // now_ms() 는 실제 현재 시각을 쓰므로, 이미 지난 deadline 은 1(unix epoch
        // 근처)로 고정하면 항상 과거다.
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::AwaitExternal {
                wait_key: "hook-1".into(),
                deadline_ms: 1,
            },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(
            alive.is_empty(),
            "past-deadline AwaitExternal must not restore alive"
        );

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        match task.state {
            TaskState::Failed { error } => assert!(
                error.contains("deadline already expired"),
                "unexpected error: {error}"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }

        let still_there: bool = ctx.with_memory(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "expired handle should be evicted");
    }

    /// 결정 4 대칭 케이스: deadline 이 아직 안 지난 `AwaitExternal` 은 그대로
    /// alive 복원(다음 tick 에서 poll — 여전히 Active 인 no-op, 진짜 종결은 외부
    /// 경로 몫).
    #[test]
    fn reload_persistent_handles_restores_await_external_before_deadline() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Custom {
                        ipc_method: "acme.start".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::AwaitExternal {
                wait_key: "hook-2".into(),
                deadline_ms: u64::MAX,
            },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert_eq!(
            alive.len(),
            1,
            "not-yet-expired AwaitExternal should restore"
        );
        assert_eq!(alive[0].0, task_id);

        let state: TaskState = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        assert!(matches!(state, TaskState::Running), "got {state:?}");
    }

    /// push 전략의 timeout 안전망. deadline 이 지난
    /// `hook_task_waits` 항목은 task 를 Failed 로 강제 마감하고 맵에서 제거한다.
    #[test]
    fn expire_overdue_hook_waits_fails_the_task_and_removes_the_entry() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Custom {
                        ipc_method: "acme.start".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });

        ctx.hook_task_waits.register(1, 1, task_id.clone(), 2000);
        expire_overdue_hook_waits(&ctx, 5000);

        // 맵에서 제거됐다 — 재조회는 None.
        assert_eq!(ctx.hook_task_waits.resolve(1), None);

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        assert!(
            matches!(task.state, TaskState::Failed { .. }),
            "got {:?}",
            task.state
        );
        assert!(task.result.is_some());
    }

    /// deadline 이 아직 안 지난 항목은 손대지 않는다.
    #[test]
    fn expire_overdue_hook_waits_leaves_fresh_entries_alone() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Custom {
                        ipc_method: "acme.start".into(),
                        params: serde_json::json!({}),
                        poll: None,
                    },
                    depends_on: vec![],
                    on_failure: OnFailure::Abort,
                    metadata: serde_json::Value::Null,
                    now_ms: 1000,
                })
                .unwrap();
            store.set_state(1, &t.id, TaskState::Running, 1100).unwrap();
            t.id
        });

        ctx.hook_task_waits.register(2, 1, task_id.clone(), 9999);
        expire_overdue_hook_waits(&ctx, 5000);

        // 아직 대기 중 — 제거되지 않았다.
        assert_eq!(ctx.hook_task_waits.resolve(2), Some((1, task_id.clone())));

        let task = ctx.with_memory(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        assert!(matches!(task.state, TaskState::Running));
    }
}

#[cfg(test)]
mod poison_tests {
    use super::*;

    /// 스레드 맵이 poison 돼도 레지스트리는 계속 답한다.
    ///
    /// `.expect()` 이던 시절에는 `liveness` 가 패닉했다 — 그 호출자가 DAG surface 의
    /// 러너 배지를 그리는 **렌더 경로**(메인 스레드, 프레임마다)라 패닉 하나가 모든 창의
    /// 터미널 세션을 함께 죽였다. `start` 는 crashed 러너의 재시작 경로라, 거기서
    /// 패닉하면 `catch_unwind` + `crashed` 로 만들어 둔 자기 복구가 무력해진다.
    #[test]
    fn a_poisoned_thread_map_still_answers_liveness_and_stop() {
        let registry = Arc::new(RunnerRegistry::new());

        let held = Arc::clone(&registry);
        // 패닉시키는 것이 목적이라 join 결과를 아래에서 따로 검사한다.
        let joined = thread::spawn(move || {
            let _guard = held.threads.lock().expect("fresh mutex");
            panic!("a thread dies while holding the runner registry");
        })
        .join();
        assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
        assert!(registry.threads.lock().is_err(), "poison 됐어야 한다");

        // 렌더 경로가 묻는 질문 — 패닉 없이 "러너 없음" 을 답한다.
        assert_eq!(registry.liveness(1), (false, false));
        // 정지 요청도 패닉하지 않는다(등록된 것이 없으니 false).
        assert!(!registry.stop(1));
        assert!(
            registry.poison_reported.load(Ordering::Relaxed),
            "poison 은 보고돼야 한다"
        );
    }
}
