//! workspace별 러너 스레드를 시작·정지·조회한다. 중복 시작은 생략한다.
//! 각 tick의 저장소 접근은 락 안에서, IPC 실행·poll은 락 밖에서 수행한다.
//! run_loop의 패닉을 잡아 crashed로 표시하며 다음 start가 재시작할 수 있다.

#[cfg(test)]
mod attempt_tests;
mod completion_reports;
mod restart_holders;
pub(crate) mod settle;
use completion_reports::CompletionReports;
use restart_holders::{purge_stale_lease_holders, purge_stale_semaphore_holders};
pub(crate) use settle::{settle_ended_task, settle_ended_tasks_in_background, stored_handle_ids};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tasty_agent::runner::{
    DispatchHandle, RunnerLoop, completion_retryable, shrink_too_large_completion,
};
use tasty_agent::task::Completion;
use tasty_agent::{TaskId, TaskState, TaskStore};
use tasty_memory::{HOST_OWNER, ListOpts, MemoryValue, Scope};

use super::runner_host::{
    DISPATCH_OUTCOME_UNKNOWN, HANDLE_ATTEMPT_FIELD, HANDLE_KEY_PREFIX, HostExecutor,
    RUN_RESULT_LOST, RunnerContext, evict_run_result, evict_task_side_keys, handle_key,
    load_run_result, process_of_record, release_own_holdings, restored_postprocess_handle,
    resumes_after_restart,
};
use tasty_agent::runner::PollOutcome;
use tasty_agent::task::postprocess::PostprocessCause;

// The join slot only transfers an owned handle; recovery never treats a missing handle as joined.
static JOIN_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

pub(crate) const TICK_INTERVAL: Duration = Duration::from_millis(500);

/// 조회가 반복 실패할 때 error로 올릴 횟수. tick 소요가 달라질 수 있어 시간 상한은 아니다.
const STORE_LIST_ERROR_AFTER: u32 = 6;
/// error 승격 뒤 반복 로그를 줄이는 실패 횟수 간격.
const STORE_LIST_REPEAT_EVERY: u32 = 120;

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
    owner: std::sync::Weak<crate::task_waker::TaskWakerHub>,
    stop_tx: mpsc::Sender<()>,
    /// 정지 요청 표시. 러너 스레드의 executor도 같은 값을 읽어 tick 안에서 남은 dispatch를 멈춘다.
    stopping: Arc<AtomicBool>,
    crashed: Arc<AtomicBool>,
    list_failures: Arc<AtomicU32>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
    joined: std::sync::atomic::AtomicU8,
}
impl RunnerControl {
    fn request_stop(&self) {
        if !self.stopping.swap(true, Ordering::AcqRel) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = self.stop_tx.send(()); // A finished receiver already needs no stop signal.
        }
    }
    fn observe_join(&self) -> RunnerStopObservation {
        let mut slot = match self.join.try_lock() {
            Ok(slot) => slot,
            Err(std::sync::TryLockError::WouldBlock) => return RunnerStopObservation::Waiting,
            Err(std::sync::TryLockError::Poisoned(poison)) => {
                tasty_utils::poison::recover_poisoned(
                    poison,
                    "task runner join slot",
                    &JOIN_POISON_REPORTED,
                )
            }
        };
        if let Some(handle) = slot.as_ref() {
            if !handle.is_finished() {
                return RunnerStopObservation::Waiting;
            }
            let failed = slot.take().expect("join handle present").join().is_err()
                || self.crashed.load(Ordering::Acquire);
            if failed {
                tracing::warn!("task runner joined after worker failure");
            }
            self.joined
                .store(if failed { 2 } else { 1 }, Ordering::Release);
        }
        match self.joined.load(Ordering::Acquire) {
            1 => RunnerStopObservation::Joined,
            2 => RunnerStopObservation::WorkerFailed,
            _ => RunnerStopObservation::Waiting,
        }
    }
    #[cfg(test)]
    fn join_blocking(&self) {
        let mut slot = tasty_utils::poison::recover_mutex(
            self.join.lock(),
            "task runner join slot",
            &JOIN_POISON_REPORTED,
        );
        if let Some(handle) = slot.take() {
            let failed = handle.join().is_err() || self.crashed.load(Ordering::Acquire);
            if failed {
                tracing::warn!("task runner joined after worker failure");
            }
            self.joined
                .store(if failed { 2 } else { 1 }, Ordering::Release);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerStopObservation {
    Waiting,
    Joined,
    WorkerFailed,
}
/// Exact controls captured when stop was requested. A later workspace owner is never included.
#[derive(Clone)]
pub struct RunnerStopReceipt {
    registry: std::sync::Weak<RunnerRegistry>,
    controls: Vec<(u32, Arc<RunnerControl>)>,
}
impl RunnerStopReceipt {
    pub fn poll(&self) -> RunnerStopObservation {
        let mut waiting = false;
        let mut failed = false;
        for (workspace, control) in &self.controls {
            let observed = control.observe_join();
            waiting |= observed == RunnerStopObservation::Waiting;
            failed |= observed == RunnerStopObservation::WorkerFailed;
            if observed != RunnerStopObservation::Waiting
                && let Some(registry) = self.registry.upgrade()
            {
                registry.unregister(*workspace, control);
            }
        }
        if waiting {
            RunnerStopObservation::Waiting
        } else if failed {
            RunnerStopObservation::WorkerFailed
        } else {
            RunnerStopObservation::Joined
        }
    }
}
#[derive(Debug, Clone)]
pub struct RunnerStatus {
    pub running: bool,
    pub crashed: bool,
    /// 정지를 요청했지만 러너 스레드가 아직 끝나지 않았다. 아직 보내지 않은 Ready 작업은 보내지 않지만
    /// 진행 중인 dispatch·poll·후처리 정리가 끝날 때까지 걸릴 수 있다. 회수 여부가 아니라 스레드 종료가 기준이다.
    pub stopping: bool,
    pub ready_count: Option<u32>,
    pub running_count: Option<u32>,
    pub store_error: Option<String>,
    pub list_failures: u32,
}
impl RunnerStatus {
    /// `running`·`stopping`·`stopped`·`crashed` 중 하나. crash 가 가장 앞선다.
    pub fn state(&self) -> &'static str {
        if self.crashed {
            "crashed"
        } else if self.running {
            "running"
        } else if self.stopping {
            "stopping"
        } else {
            "stopped"
        }
    }
}
pub struct RunnerRegistry {
    threads: Mutex<HashMap<u32, Arc<RunnerControl>>>,
    poison_reported: AtomicBool,
}
impl RunnerRegistry {
    fn lock_recovering(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Arc<RunnerControl>>> {
        tasty_utils::poison::recover_mutex(
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
    fn unregister(&self, workspace: u32, control: &Arc<RunnerControl>) {
        let mut threads = self.lock_recovering();
        if threads
            .get(&workspace)
            .is_some_and(|current| Arc::ptr_eq(current, control))
        {
            threads.remove(&workspace);
        }
    }
    pub(crate) fn request_stop(
        self: &Arc<Self>,
        owner: &Arc<crate::task_waker::TaskWakerHub>,
        workspace: Option<u32>,
    ) -> RunnerStopReceipt {
        let controls = self
            .lock_recovering()
            .iter()
            .filter(|(id, control)| {
                workspace.is_none_or(|workspace| workspace == **id)
                    && control.owner.ptr_eq(&Arc::downgrade(owner))
            })
            .map(|(id, control)| (*id, control.clone()))
            .collect::<Vec<_>>();
        for (_, control) in &controls {
            control.request_stop();
        }
        RunnerStopReceipt {
            registry: Arc::downgrade(self),
            controls,
        }
    }
    /// Reap only stopped workers; crashed entries remain visible until stop or explicit restart.
    pub(crate) fn poll_stops(&self) -> usize {
        let controls = self
            .lock_recovering()
            .iter()
            .filter(|(_, control)| control.stopping.load(Ordering::Acquire))
            .map(|(id, control)| (*id, control.clone()))
            .collect::<Vec<_>>();
        let mut remaining = 0;
        for (workspace, control) in controls {
            if control.observe_join() == RunnerStopObservation::Waiting {
                remaining += 1;
            } else {
                self.unregister(workspace, &control);
            }
        }
        remaining
    }
    pub(crate) fn start(&self, ctx: RunnerContext, workspace_id: u32) -> bool {
        let mut threads = self.lock_recovering();
        if ctx.scope_stopping.load(Ordering::Acquire) {
            return false;
        }
        if let Some(control) = threads.get(&workspace_id) {
            // A stopping old owner still owns its dispatch/poll until the exact worker has joined.
            if !control.crashed.load(Ordering::Acquire) && !control.stopping.load(Ordering::Acquire)
            {
                return false;
            }
            if control.observe_join() == RunnerStopObservation::Waiting {
                return false;
            }
        }
        threads.remove(&workspace_id);
        let (stop_tx, stop_rx) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = stopping.clone();
        let crashed = Arc::new(AtomicBool::new(false));
        let thread_crashed = crashed.clone();
        let list_failures = Arc::new(AtomicU32::new(0));
        let thread_failures = list_failures.clone();
        let owner = Arc::downgrade(&ctx.task_waker_hub);
        let spawned = thread::Builder::new()
            .name(format!("agent-runner-ws{workspace_id}"))
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_loop(
                        ctx,
                        workspace_id,
                        stop_rx,
                        thread_stopping,
                        &thread_failures,
                    )
                }))
                .is_err()
                {
                    thread_crashed.store(true, Ordering::Release);
                    tracing::error!(
                        workspace_id,
                        "task runner panicked; explicit restart is required"
                    );
                }
            });
        let join = match spawned {
            Ok(join) => join,
            Err(error) => {
                tracing::error!(%error,workspace_id,"task runner spawn failed");
                return false;
            }
        };
        threads.insert(
            workspace_id,
            Arc::new(RunnerControl {
                owner,
                stop_tx,
                stopping,
                crashed,
                list_failures,
                join: Mutex::new(Some(join)),
                joined: std::sync::atomic::AtomicU8::new(0),
            }),
        );
        true
    }
    /// 시험 전용 동기 정지. 제품 경로는 메인 스레드를 막지 않도록 [`Self::request_stop`] 의 receipt 와
    /// [`Self::poll_stops`] 를 쓴다. 러너 tick 이 메인 스레드의 host 응답을 기다릴 수 있어서다.
    #[cfg(test)]
    pub(crate) fn stop(&self, workspace: u32) -> bool {
        let control = self.lock_recovering().get(&workspace).cloned();
        if let Some(control) = control {
            control.request_stop();
            control.join_blocking();
            self.unregister(workspace, &control);
            true
        } else {
            false
        }
    }
    /// 시험 전용. workspace 러너 스레드가 실제로 끝났는가. 등록부에서 지우지 않고 join 하지도 않는다.
    /// 등록된 러너가 없으면 `None`, 이미 join 했으면 `Some(true)`다.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn worker_finished(&self, workspace: u32) -> Option<bool> {
        let control = self.lock_recovering().get(&workspace).cloned()?;
        let slot = control.join.lock().ok()?;
        Some(slot.as_ref().is_none_or(|handle| handle.is_finished()))
    }
    /// workspace 에 멈추지 않은 러너가 있는가. 어느 engine 의 러너인지는 보지 않는다.
    pub(crate) fn has_live_runner(&self, workspace: u32) -> bool {
        self.lock_recovering()
            .get(&workspace)
            .is_some_and(|control| {
                !control.stopping.load(Ordering::Acquire)
                    && !control.crashed.load(Ordering::Acquire)
            })
    }
    pub(crate) fn scoped_liveness(
        &self,
        owner: &Arc<crate::task_waker::TaskWakerHub>,
        workspace: u32,
    ) -> (bool, bool) {
        self.lock_recovering()
            .get(&workspace)
            .filter(|control| control.owner.ptr_eq(&Arc::downgrade(owner)))
            .map_or((false, false), |control| {
                let crashed = control.crashed.load(Ordering::Acquire);
                (
                    !control.stopping.load(Ordering::Acquire) && !crashed,
                    crashed,
                )
            })
    }
    /// 정지를 요청한 원 범위의 러너가 아직 끝나지 않았는가. join 을 기다리지 않는다.
    fn scoped_stopping(
        &self,
        owner: &Arc<crate::task_waker::TaskWakerHub>,
        workspace: u32,
    ) -> bool {
        let control = self
            .lock_recovering()
            .get(&workspace)
            .filter(|control| control.owner.ptr_eq(&Arc::downgrade(owner)))
            .cloned();
        control.is_some_and(|control| {
            control.stopping.load(Ordering::Acquire)
                && control.observe_join() == RunnerStopObservation::Waiting
        })
    }
    pub(crate) fn status(&self, ctx: &RunnerContext, workspace: u32) -> RunnerStatus {
        self.status_with(
            &ctx.task_waker_hub,
            workspace,
            count_ready_running(ctx, workspace),
        )
    }

    /// 저장소에서 센 Ready·Running 수에 러너 실행 여부를 더한다.
    pub(crate) fn status_with(
        &self,
        hub: &Arc<crate::task_waker::TaskWakerHub>,
        workspace: u32,
        counts: Result<(u32, u32), String>,
    ) -> RunnerStatus {
        let (running, crashed) = self.scoped_liveness(hub, workspace);
        let stopping = self.scoped_stopping(hub, workspace);
        let list_failures = self
            .lock_recovering()
            .get(&workspace)
            .filter(|control| control.owner.ptr_eq(&Arc::downgrade(hub)))
            .map_or(0, |control| control.list_failures.load(Ordering::Relaxed));
        match counts {
            Ok((ready, running_count)) => RunnerStatus {
                running,
                crashed,
                stopping,
                ready_count: Some(ready),
                running_count: Some(running_count),
                store_error: None,
                list_failures,
            },
            Err(error) => RunnerStatus {
                running,
                crashed,
                stopping,
                ready_count: None,
                running_count: None,
                store_error: Some(error),
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
/// 서비스 소유자가 사라질 때(앱 종료) runner 스레드가 끝나기를 기다리는 상한. runner 는
/// 끝나면서 진행 중인 후처리의 프로세스 그룹을 끝내고 보고를 저장한다(최대 3초). 그 일이
/// 프로세스 종료보다 먼저 끝나도록 기다리되, 멈춘 runner 가 종료를 막지 않게 상한을 둔다.
const SHUTDOWN_JOIN_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

impl Drop for RunnerRegistry {
    fn drop(&mut self) {
        let threads = self.threads.get_mut().unwrap_or_else(|poison| {
            tasty_utils::poison::recover_poisoned(
                poison,
                "runner registry thread map",
                &self.poison_reported,
            )
        });
        for control in threads.values() {
            control.request_stop();
        }
        let waiting = || {
            threads
                .values()
                .filter(|control| control.observe_join() == RunnerStopObservation::Waiting)
                .count()
        };
        let deadline = std::time::Instant::now() + SHUTDOWN_JOIN_WAIT;
        let mut remaining = waiting();
        while remaining != 0 && std::time::Instant::now() < deadline {
            thread::sleep(std::time::Duration::from_millis(5));
            remaining = waiting();
        }
        if remaining != 0 {
            tracing::warn!(
                remaining,
                "task runners still unjoined at service owner drop"
            );
        }
    }
}

/// 조회 실패를 task가 없는 것으로 표시하지 않도록 오류를 그대로 반환한다.
fn count_ready_running(ctx: &RunnerContext, workspace_id: u32) -> Result<(u32, u32), String> {
    let seq = ctx.agent_seq.clone();
    ctx.with_memory(move |mem| count_ready_running_in(mem, &seq, workspace_id))
}

pub(crate) fn count_ready_running_in(
    mem: &mut dyn tasty_memory::MemoryStorage,
    seq: &std::sync::atomic::AtomicU64,
    workspace_id: u32,
) -> Result<(u32, u32), String> {
    {
        let store = TaskStore::new(mem, HOST_OWNER, seq);
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
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 저장된 handle을 분류해 복원하거나 정리한다.
/// ShellProcess는 PID가 없을 때만 저장된 종료 결과를 확인한다. PID 생존은 같은 자식임을 보장하지 않는다.
/// PolledDispatch·BarrierPoll은 복원하며 즉시 완료 handle은 삭제 대상으로 본다.
/// AwaitExternal은 reload 당시 만료됐으면 실패 처리하고, 미만료면 복원하면서 기한을 공유 대기에 다시 건다.
fn reload_persistent_handles(
    ctx: &RunnerContext,
    workspace_id: u32,
) -> Vec<(TaskId, DispatchHandle)> {
    let now = now_ms();
    let scope = Scope::Workspace(workspace_id);
    let listed = scope.clone();
    let entries = ctx.with_memory(move |mem| {
        let opts = ListOpts {
            prefix: Some(HANDLE_KEY_PREFIX.to_string()),
            ..Default::default()
        };
        mem.list(&listed, &opts).unwrap_or_default()
    });
    let mut alive: Vec<(TaskId, DispatchHandle)> = Vec::new();
    let mut dead: Vec<(TaskId, Option<String>, String)> = Vec::new();
    let mut stale: Vec<TaskId> = Vec::new();
    let mut settled: Vec<tasty_agent::Task> = Vec::new();
    let mut precise: Vec<(TaskId, Option<String>, PollOutcome)> = Vec::new();

    for e in entries {
        match classify_persisted_handle(ctx, workspace_id, now, e) {
            HandleClassification::Alive(task_id, handle) => alive.push((task_id, handle)),
            HandleClassification::AwaitingExternal(task_id, attempt, handle) => {
                restore_external_deadline(ctx, workspace_id, &task_id, attempt, &handle);
                alive.push((task_id, handle));
            }
            HandleClassification::Dead(task_id, attempt, err) => dead.push((task_id, attempt, err)),
            HandleClassification::Stale(task_id) => stale.push(task_id),
            HandleClassification::NotRunning(task) => settled.push(*task),
            HandleClassification::Precise(task_id, attempt, outcome) => {
                precise.push((task_id, attempt, outcome))
            }
        }
    }

    evict_stale_handles(ctx, &scope, &stale);
    for task in &settled {
        settle_ended_task(ctx, workspace_id, task);
    }
    mark_dead_tasks(ctx, workspace_id, &scope, now, &dead);
    finalize_precise_tasks(ctx, workspace_id, &scope, now, &precise);

    if !alive.is_empty()
        || !dead.is_empty()
        || !stale.is_empty()
        || !settled.is_empty()
        || !precise.is_empty()
    {
        tracing::info!(
            "agent runner ws{workspace_id}: reload handles — alive={}, dead={}, stale={}, ended={}, precise={}",
            alive.len(),
            dead.len(),
            stale.len(),
            settled.len(),
            precise.len()
        );
    }
    alive
}

/// `Dead`·`Precise` 의 회차는 handle 을 저장한 dispatch 의 회차다(v1·옛 레코드는 없음).
enum HandleClassification {
    Alive(TaskId, DispatchHandle),
    /// 미만료 AwaitExternal. 복원하면서 저장된 기한을 다시 검사 대상으로 등록한다.
    AwaitingExternal(TaskId, Option<String>, DispatchHandle),
    Dead(TaskId, Option<String>, String),
    Stale(TaskId),
    /// handle 이 남았는데 task 는 이미 Running 이 아니다(러너가 꺼진 동안 취소 등). 남은 프로세스를
    /// 끝내고 종료를 확인한 뒤 handle 을 지우고 그 task 가 자기 id 로 쥔 점유도 반환한다
    /// ([`settle_ended_task`]). 반환할 러너가 그때 없었기 때문이다.
    NotRunning(Box<tasty_agent::Task>),
    Precise(TaskId, Option<String>, PollOutcome),
}

/// handle별 정리 계획을 만든다. 저장소 변경은 분류를 모은 뒤 별도 함수에서 수행한다.
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
    let proc = process_of_record(&v);
    let attempt = v
        .get(HANDLE_ATTEMPT_FIELD)
        .and_then(|a| a.as_str())
        .map(str::to_string);
    let handle: DispatchHandle = match serde_json::from_value(v) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("reload handle {task_id} deserialize: {e}");
            return HandleClassification::Stale(task_id);
        }
    };

    // task 가 없으면(조회 오류 포함) handle 만 지운다. Running 이 아니면 쥔 점유도 반환한다.
    let (seq, id) = (ctx.agent_seq.clone(), task_id.clone());
    let task_opt = ctx.with_memory(move |mem| {
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.get(workspace_id, &id).ok().flatten()
    });
    let Some(task) = task_opt else {
        return HandleClassification::Stale(task_id);
    };
    if !matches!(task.state, TaskState::Running) {
        return HandleClassification::NotRunning(Box::new(task));
    }
    // 본 작업을 마친 후처리 단계는 저장된 handle 이 아니라 회차의 진행으로 복원한다.
    if let Some(h) = restored_postprocess_handle(ctx, workspace_id, &task, &handle, proc) {
        // 결과 불명은 다시 실행하지 않는 종결이라 Run 의 결과 불명처럼 지금 확정하고 점유를 반환한다.
        // 저장된 보고는 재시도로 이어질 수 있어 러너가 처리한다.
        if let DispatchHandle::PostprocessResolved(report) = &h
            && report.cause() == Some(PostprocessCause::OutcomeUnknown)
        {
            return HandleClassification::Precise(
                task_id,
                attempt,
                PollOutcome::Postprocessed(report.clone()),
            );
        }
        return HandleClassification::Alive(task_id, h);
    }

    match &handle {
        DispatchHandle::ShellProcess { pid } => {
            // 저장한 시작 시각과 같아야 같은 프로세스다(PID 는 다시 쓰일 수 있다).
            if proc.is_some_and(|p| p.is_running()) {
                HandleClassification::Alive(task_id, handle)
            } else if let Some(outcome) = load_run_result(ctx, workspace_id, &task_id) {
                HandleClassification::Precise(task_id, attempt, outcome)
            } else {
                // 저장된 종료 결과가 없으면 그 종료 코드를 받을 방법이 없다(부모가 아니다). 실패로
                // 꾸미지 않고 결과 불명으로 둔다.
                HandleClassification::Precise(
                    task_id,
                    attempt,
                    PollOutcome::Lost(format!(
                        "{RUN_RESULT_LOST}: pid {pid} ended before the runner resumed watching it and no exit status was saved"
                    )),
                )
            }
        }
        // 비영속 hook 매핑은 사라졌으므로 저장된 기한으로 만료를 판단한다. 옛 누락 필드는 0이다.
        DispatchHandle::AwaitExternal { deadline_ms, .. } if *deadline_ms <= now => {
            HandleClassification::Dead(
                task_id,
                attempt,
                "host restart: push completion strategy deadline already expired".to_string(),
            )
        }
        // 미만료 AwaitExternal도 외부 상태 변경 뒤 점유를 정리할 수 있도록 복원한다.
        // hook ID는 재시작 뒤 다시 발급될 수 있어 훅 매핑은 되살리지 않고 기한만 다시 건다.
        DispatchHandle::AwaitExternal { .. } => {
            HandleClassification::AwaitingExternal(task_id, attempt, handle)
        }
        DispatchHandle::PolledDispatch { .. } | DispatchHandle::BarrierPoll { .. } => {
            HandleClassification::Alive(task_id, handle)
        }
        DispatchHandle::ReduceImmediate(_)
        | DispatchHandle::CustomImmediate(_)
        | DispatchHandle::ImmediateFail(_) => HandleClassification::Stale(task_id),
        DispatchHandle::PostprocessPending { .. }
        | DispatchHandle::PostprocessProcess { .. }
        | DispatchHandle::PostprocessResolved(_) => HandleClassification::Alive(task_id, handle),
        // 응답 통로는 영속하지 않는다. 요청은 이미 시작됐으므로 다시 보내지 않고 결과 불명으로 둔다.
        DispatchHandle::AwaitDispatchAnswer { ipc_method, .. } => HandleClassification::Precise(
            task_id,
            attempt,
            PollOutcome::Lost(format!(
                "{DISPATCH_OUTCOME_UNKNOWN}: '{ipc_method}' had started but had not answered \
                 before the host restarted; check whether it took effect before retrying"
            )),
        ),
        // 턴 표는 영속하지 않는다. 재시작 사이에 끝난 턴의 답변을 이 회차에 귀속할 수 없다.
        // 세션은 그대로 두고 회차만 실패로 정리한다.
        DispatchHandle::AgentTurn { surface_id, .. } => HandleClassification::Dead(
            task_id,
            attempt,
            tasty_agent::task::FailureCode::AgentUnavailable.message(format!(
                "host restart: the turn on surface {surface_id} can no longer be attributed to this attempt"
            )),
        ),
    }
}

/// stale handle만 삭제하고 task 상태는 바꾸지 않는다.
fn evict_stale_handles(ctx: &RunnerContext, scope: &Scope, stale: &[TaskId]) {
    if stale.is_empty() {
        return;
    }
    let (scope, stale) = (scope.clone(), stale.to_vec());
    ctx.with_memory(move |mem| {
        for tid in &stale {
            mem.delete(HOST_OWNER, &scope, &handle_key(tid), None).unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to evict a stale task handle; reload will retry");
            });
        }
    });
}

fn mark_dead_tasks(
    ctx: &RunnerContext,
    workspace_id: u32,
    scope: &Scope,
    now: u64,
    dead: &[(TaskId, Option<String>, String)],
) {
    for (task_id, attempt, err) in dead {
        let completion = Completion::failed(attempt.clone(), err.clone());
        if record_reload_completion(ctx, workspace_id, task_id, completion, now) {
            evict_handle(ctx, scope, task_id);
        }
    }
}

/// 재시작 복구의 완료 보고를 기록한다. 기록하지 못했으면 handle 을 남겨 다음 reload 가 다시
/// 보고하게 `false` 를 돌려준다. 받아들여지지 않는 보고는 다시 내도 같아 handle 을 지운다.
fn record_reload_completion(
    ctx: &RunnerContext,
    workspace_id: u32,
    task_id: &TaskId,
    completion: Completion,
    now: u64,
) -> bool {
    match ctx.complete_task(workspace_id, task_id, completion.clone(), now) {
        Ok(_) => true,
        Err(e) if let Some(shrunk) = shrink_too_large_completion(&e, &completion) => {
            tracing::warn!(
                "reload completion for {task_id} too large to record, settling it as a failure: {e}"
            );
            record_reload_completion(ctx, workspace_id, task_id, shrunk, now)
        }
        Err(e) => reload_completion_unrecorded(task_id, &e),
    }
}

/// 기록하지 못한 재시작 복구 보고. 다시 시도할 오류면 handle 을 남기도록 `false` 다.
fn reload_completion_unrecorded(task_id: &TaskId, e: &tasty_agent::AgentError) -> bool {
    if completion_retryable(e) {
        tracing::warn!("reload completion for {task_id} not recorded; kept for retry: {e}");
        false
    } else {
        tracing::warn!("reload completion for {task_id} rejected: {e}");
        true
    }
}

fn evict_handle(ctx: &RunnerContext, scope: &Scope, task_id: &TaskId) {
    let (scope, key) = (scope.clone(), handle_key(task_id));
    ctx.with_memory(move |mem| {
        mem.delete(HOST_OWNER, &scope, &key, None)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to evict a finished task handle; reload will retry");
            });
    });
}

fn finalize_precise_tasks(
    ctx: &RunnerContext,
    workspace_id: u32,
    scope: &Scope,
    now: u64,
    precise: &[(TaskId, Option<String>, PollOutcome)],
) {
    for (task_id, attempt, outcome) in precise {
        let completion = match outcome {
            PollOutcome::Done(r) => Completion::succeeded(attempt.clone(), r.clone()),
            PollOutcome::Failed(err) => Completion::failed(attempt.clone(), err.clone()),
            PollOutcome::Exited(r) => Completion::exited(attempt.clone(), r.clone()),
            PollOutcome::Lost(reason) => Completion::lost(attempt.clone(), reason.clone()),
            PollOutcome::Postprocessed(r) => Completion::postprocessed(attempt.clone(), r.clone()),
            // 저장된 결과가 종결이 아니면 이전처럼 handle 만 지운다.
            PollOutcome::Active => {
                evict_handle(ctx, scope, task_id);
                continue;
            }
        };
        // 확정 전의 task 가 쥔 점유. 부팅 정리가 이 회차의 점유를 남겨 두었으므로(살아 있었을
        // 수 있는 Run) 끝난 것을 확인한 지금 반환한다.
        let held = load_task(ctx, workspace_id, task_id);
        if record_reload_completion(ctx, workspace_id, task_id, completion, now) {
            if let Some(task) = held {
                ctx.with_memory(move |mem| release_own_holdings(mem, workspace_id, &task));
            }
            evict_handle(ctx, scope, task_id);
            evict_run_result(ctx, workspace_id, task_id);
        }
    }
}

/// 복원한 Run·후처리가 부팅 정리가 남긴 점유를 이어받는가. 읽지 못하면 이어받지 않는다.
fn resumes(ctx: &RunnerContext, workspace_id: u32, task: &tasty_agent::Task) -> bool {
    let task = task.clone();
    ctx.store(move |mem| resumes_after_restart(mem, workspace_id, &task))
        .unwrap_or_else(|error| {
            tracing::warn!(%error, workspace_id, "restored holdings not checked");
            false
        })
}

fn load_task(ctx: &RunnerContext, workspace_id: u32, task_id: &str) -> Option<tasty_agent::Task> {
    let (seq, id) = (ctx.agent_seq.clone(), task_id.to_string());
    ctx.with_memory(move |mem| {
        TaskStore::new(mem, HOST_OWNER, seq.as_ref())
            .get(workspace_id, &id)
            .ok()
            .flatten()
    })
}

/// 복원한 AwaitExternal의 저장된 기한을 공유 대기에 건다. 기한 검사는 hook 대기와 같이
/// 실행 중인 러너의 tick이 맡고, 만료 처리도 같은 함수가 한다.
fn restore_external_deadline(
    ctx: &RunnerContext,
    workspace_id: u32,
    task_id: &TaskId,
    attempt: Option<String>,
    handle: &DispatchHandle,
) {
    let DispatchHandle::AwaitExternal { deadline_ms, .. } = handle else {
        return;
    };
    ctx.hook_task_waits.register_restored(
        workspace_id,
        task_id.clone(),
        *deadline_ms,
        crate::hook_wait::HookWaitOwner {
            agent_seq: ctx.agent_seq.clone(),
            completion: ctx.task_waker_hub.clone(),
        },
        attempt,
    );
}

/// 복원한 기한이 대표하는 회차가 아직 실행 중인지 본다. 기한 전에 외부 상태 변경으로 끝났거나
/// 새 회차가 시작됐으면 시간 초과로 보고하지 않는다.
fn restored_wait_still_running(
    ctx: &RunnerContext,
    workspace_id: u32,
    task_id: &TaskId,
    attempt: Option<&str>,
) -> bool {
    let running = load_task(ctx, workspace_id, task_id).is_some_and(|task| {
        matches!(task.state, TaskState::Running)
            && attempt.is_none_or(|a| task.attempt.as_ref().is_some_and(|cur| cur.id == a))
    });
    if !running {
        tracing::debug!(
            "agent task {task_id} (ws {workspace_id}): restored push wait ended before its deadline"
        );
    }
    running
}

/// 대기 매핑을 먼저 회수한 뒤 만료 작업을 실패로 표시하고 대기자를 깨운다.
/// 저장 실패는 경고하며 제거한 매핑을 재등록하지 않는다. 재시작 뒤 복원한 대기는 기한만으로 회수한다.
/// 공유 매핑의 제거가 락 안에서 이뤄져 여러 workspace 러너가 같은 항목을 중복 회수하지 않는다.
fn expire_overdue_hook_waits(ctx: &RunnerContext, now_ms: u64) {
    for wait in ctx.hook_task_waits.take_expired(now_ms) {
        expire_hook_wait(ctx, wait, now_ms);
    }
}

/// 회수한 대기 하나를 시간 초과 실패로 끝낸다. 복원한 대기는 그 회차가 아직 실행 중일 때만 끝낸다.
fn expire_hook_wait(ctx: &RunnerContext, wait: crate::hook_wait::ExpiredWait, now_ms: u64) {
    let workspace_id = wait.workspace;
    let task_id = wait.task;
    if wait.restored
        && !restored_wait_still_running(ctx, workspace_id, &task_id, wait.attempt.as_deref())
    {
        return;
    }
    // 다른 engine 이 건 대기는 그 engine 의 id 발급기와 대기자 hub 로 끝낸다.
    let mut owner_ctx = ctx.clone();
    if let Some(owner) = wait.owner {
        owner_ctx.agent_seq = owner.agent_seq;
        owner_ctx.task_waker_hub = owner.completion;
    }
    let error = "push completion strategy timed out waiting for external report".to_string();
    let completion = Completion::failed(wait.attempt, error);
    if let Err(e) = owner_ctx.complete_task(workspace_id, &task_id, completion, now_ms) {
        tracing::warn!("hook wait timeout: completion for {task_id} not recorded: {e}");
    }
    tracing::warn!(
        "agent task {task_id} (ws {workspace_id}): push completion strategy timed out \
         waiting for external report; failure update attempted"
    );
}

/// 부팅 또는 러너 시작 때 점유 정리, handle 복원, Waiting task 의 readiness 재평가를 수행한다.
/// 부팅에서는 반환 handle을 버리고 수동 러너 시작 때 다시 읽는다. 저장 실패 항목은 남을 수 있다.
fn purge_and_reload_on_restart(
    ctx: &RunnerContext,
    workspace_id: u32,
) -> Vec<(TaskId, DispatchHandle)> {
    crate::terminal_recovery::reconcile(ctx, workspace_id);
    purge_stale_semaphore_holders(ctx, workspace_id);
    purge_stale_lease_holders(ctx, workspace_id);
    let reloaded = reload_persistent_handles(ctx, workspace_id);
    resettle_waiting_tasks(ctx, workspace_id);
    crate::terminal_recovery::reconcile(ctx, workspace_id);
    reloaded
}

/// 완료 쓰기 뒤 하류 반영 전에 멈춘 task 를 마무리한다. 복원한 handle 의 보고를 먼저 기록한 뒤
/// 평가해야 그 보고로 풀리는 하류도 같은 번에 맞춰진다.
fn resettle_waiting_tasks(ctx: &RunnerContext, workspace_id: u32) {
    let seq = ctx.agent_seq.clone();
    let now = now_ms();
    let changed = ctx.with_memory(move |mem| {
        TaskStore::new(mem, HOST_OWNER, seq.as_ref()).resettle_waiting(workspace_id, now)
    });
    match changed {
        Ok(tasks) => ctx.fire_terminal_tasks(workspace_id, tasks),
        Err(e) => tracing::warn!("agent runner ws{workspace_id}: readiness resettle failed: {e}"),
    }
}

/// 전달받은 live workspace를 정리하되 러너 스레드를 자동으로 시작하지 않는다.
pub(crate) fn purge_stale_agent_state_on_boot(ctx: &RunnerContext, workspace_ids: &[u32]) {
    for &workspace_id in workspace_ids {
        // 아직 러너가 없어 복원 결과는 사용하지 않는다. 수동 시작 때 다시 읽는다.
        let _ = purge_and_reload_on_restart(ctx, workspace_id);
        gc_stale_tasks(ctx, workspace_id);
    }
}

/// 사용자가 며칠 안에 결과를 확인한다는 가정으로 정한 잠정 보존 기간. 실사용 측정에 근거한 값은 아니다.
const AGENT_TASK_GC_MIN_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// memory TTL 대신 TaskStore의 참조·Running 보호 규칙을 거쳐 오래된 작업을 지운다.
/// 완료 상태만 고르면 방치된 Waiting 작업과 그 입력을 계속 남기므로 상태 필터는 두지 않는다.
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
    // 이전 회차의 종료를 확인하는 중인 task 는 지우지 않는다. 지우면 handle 이 사라져 점유를 반환할
    // 정리가 없어진다.
    let plan = ctx.with_memory(move |mem| {
        let keep = stored_handle_ids(&*mem, workspace_id);
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.plan_sweep(workspace_id, &filter, &keep)
    });
    match plan {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!("agent task GC ws{workspace_id}: plan_sweep failed: {e}");
            None
        }
    }
}

fn gc_apply_sweep_plan(
    ctx: &RunnerContext,
    workspace_id: u32,
    plan: &tasty_agent::task::TaskSweepPlan,
) -> bool {
    let (seq, plan) = (ctx.agent_seq.clone(), plan.clone());
    let res = ctx.with_memory(move |mem| {
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.apply_sweep_plan(workspace_id, &plan)
    });
    match res {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("agent task GC ws{workspace_id}: apply_sweep_plan failed: {e}");
            false
        }
    }
}

/// snapshot 조회 실패를 세고 로그를 남긴 뒤 빈 목록으로 tick을 호출한다.
/// 목록 기반 dispatch·poll·점유 해제는 건너뛰지만 앞서 실행하는 hook 만료 처리는 별개다.
fn tick_snapshot(
    ctx: &RunnerContext,
    workspace_id: u32,
    store_list_failures: &AtomicU32,
) -> Vec<tasty_agent::Task> {
    let seq = ctx.agent_seq.clone();
    let listed = ctx.with_memory(move |mem| {
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

fn log_store_list_failure(workspace_id: u32, n: u32, e: &dyn std::fmt::Display) {
    match store_list_failure_log(n) {
        StoreListFailureLog::Warn => tracing::warn!(
            "agent runner ws{workspace_id}: task store list failed: {e} \
             — snapshot-based dispatch, polling, and permit release skipped"
        ),
        StoreListFailureLog::Error => tracing::error!(
            "agent runner ws{workspace_id}: task store list failed {n} times in a row: {e} \
             — snapshot-based dispatch, polling, and permit release remain blocked"
        ),
        StoreListFailureLog::Silent => {}
    }
}

fn run_loop(
    ctx: RunnerContext,
    workspace_id: u32,
    stop_rx: mpsc::Receiver<()>,
    stopping: Arc<AtomicBool>,
    list_failures: &AtomicU32,
) {
    if ctx.scope_stopping.load(Ordering::Acquire)
        || !matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty))
    {
        return;
    }
    let reloaded = purge_and_reload_on_restart(&ctx, workspace_id);
    let executor = HostExecutor::new(ctx.clone()).with_stop_signal(stopping);
    let mut runner = RunnerLoop::new(executor);
    let mut reports = CompletionReports::default();
    for (task_id, handle) in reloaded {
        // 복원한 Run·후처리는 부팅 정리가 남겨 둔 점유를 쥐고 있다. 끝날 때 반환하도록 넘겨받는다.
        if let Some(task) = load_task(&ctx, workspace_id, &task_id)
            && resumes(&ctx, workspace_id, &task)
        {
            runner.executor.adopt_restored_run(workspace_id, &task);
        }
        runner.running.insert(task_id, handle);
    }
    loop {
        if ctx.scope_stopping.load(Ordering::Acquire)
            || !matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty))
        {
            break;
        }
        // 만료 작업의 종결 상태를 이번 snapshot에서도 보고 handle·점유를 정리할 수 있도록 먼저 처리한다.
        let now = now_ms();
        expire_overdue_hook_waits(&ctx, now);

        // 러너가 지켜보지 않는 남은 handle(취소된 회차 등)의 종료를 다시 확인한다.
        settle::settle_unwatched_handles(&ctx, workspace_id, |id| runner.executor.watches(id));

        let snapshot = tick_snapshot(&ctx, workspace_id, list_failures);

        let ctx_for_set = ctx.clone();
        let ctx_for_res = ctx.clone();
        let reports = &mut reports;
        runner.tick(
            workspace_id,
            now,
            &snapshot,
            move |ws, id, st, n| {
                let (seq, task_id) = (ctx_for_set.agent_seq.clone(), id.clone());
                let (res, fire_target) = ctx_for_set
                    .store(move |mem| {
                        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
                        match store.set_state(ws, &task_id, st.clone(), n) {
                            Ok((task, downstream)) => (
                                Ok(()),
                                std::iter::once(task).chain(downstream).collect::<Vec<_>>(),
                            ),
                            Err(e) => (Err(e), Vec::new()),
                        }
                    })
                    .unwrap_or_else(|blocked| {
                        (
                            Err(tasty_agent::AgentError::Memory(blocked.to_memory_error())),
                            Vec::new(),
                        )
                    });
                ctx_for_set.fire_terminal_tasks(ws, fire_target);
                res
            },
            move |ws, id, c, n| reports.report(&ctx_for_res, ws, id, c, n),
        );

        match stop_rx.recv_timeout(TICK_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => continue,
        }
    }
}

#[cfg(test)]
#[path = "terminal_execution_tests.rs"]
mod terminal_execution_tests;

#[cfg(test)]
mod tests {
    use super::super::runner_host::run_result_key;
    use super::*;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicU64;
    use tasty_agent::task::TaskCreateOpts;
    use tasty_agent::{OnFailure, TaskCommand};
    use tasty_memory::{MemoryStore, PutOpts};

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

    fn put_handle(ctx: &RunnerContext, ws: u32, task_id: &str, handle: &DispatchHandle) {
        ctx.with_store(|mem| {
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

    /// 실제 저장소 값을 손상시켜 snapshot 조회 실패 누적과 회복 시 초기화를 확인한다.
    #[test]
    fn tick_snapshot_counts_consecutive_failures_and_resets_on_recovery() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_store(|mem| {
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

        assert_eq!(tick_snapshot(&ctx, 1, &failures).len(), 1);
        assert_eq!(failures.load(Ordering::Relaxed), 0);

        // 키 형식을 복제하지 않고 실제 저장된 task 키의 값만 손상시킨다.
        let corrupt_key = ctx.with_store(|mem| {
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

        for expected in 1..=3u32 {
            assert!(
                tick_snapshot(&ctx, 1, &failures).is_empty(),
                "조회 실패면 빈 snapshot"
            );
            assert_eq!(
                failures.load(Ordering::Relaxed),
                expected,
                "error 승격에 사용하는 연속 실패 수가 누적되지 않았다"
            );
        }

        ctx.with_store(|mem| {
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

    #[test]
    fn status_reports_unknown_counts_when_the_task_store_is_unreadable() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_store(|mem| {
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

        ctx.with_store(|mem| {
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

    /// 실제 러너 스레드가 올린 조회 실패 수를 status가 반환하는지 확인한다.
    #[test]
    fn status_reports_the_running_runner_consecutive_list_failures() {
        let (_td, ctx) = fresh_ctx();
        ctx.with_store(|mem| {
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
        // 실제 task 키의 값을 역직렬화할 수 없는 데이터로 바꾼다.
        ctx.with_store(|mem| {
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
            "조회 실패와 별개로 러너 스레드는 실행 중이어야 한다"
        );
    }

    #[test]
    fn reload_persistent_handles_restores_alive_shell_process() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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

    #[test]
    fn reload_persistent_handles_marks_dead_pid_as_unknown() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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
        // 보통 존재하지 않는 큰 PID를 쓴다. 환경에 따라 사용 중일 가능성은 있다.
        let dead_pid: u32 = 0xFFFF_FFFE;
        put_handle(
            &ctx,
            1,
            &task_id,
            &DispatchHandle::ShellProcess { pid: dead_pid },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty(), "dead pid should not restore");

        let final_state: TaskState = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        match final_state {
            TaskState::Unknown {
                reason: Some(reason),
            } => assert!(
                reason.starts_with(RUN_RESULT_LOST)
                    && reason.contains("before the runner resumed watching it"),
                "unexpected reason: {reason}"
            ),
            other => panic!("expected Unknown, got {other:?}"),
        }

        let still_there: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "dead pid handle should be evicted");
    }

    fn put_run_result(ctx: &RunnerContext, ws: u32, task_id: &str, value: serde_json::Value) {
        ctx.with_store(|mem| {
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
        ctx.with_store(|mem| {
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

        let task = ctx.with_store(|mem| {
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

        let handle_present: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!handle_present, "handle evicted after precise");
        let result_present: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &run_result_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!result_present, "run_result evicted after precise");
    }

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
                "error": "Run exited with code 2",
            }),
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty());

        let task = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        match &task.state {
            TaskState::Failed { error } => {
                assert!(error.contains("exited with code 2"), "got {error}")
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(
            !matches!(&task.state, TaskState::Failed { error } if error.contains("unknown")),
            "should not be 'unknown' message",
        );
    }

    /// 실패한 Run 의 저장된 결과에 종료 코드·출력이 있으면 재시작 뒤 확정한 결과에도 남는다.
    #[test]
    fn reload_shell_process_with_a_failed_result_keeps_its_exit_code_and_output() {
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
                "error": "Run exited with code 5",
                "exit_code": 5,
                "output": {"pid": 1, "stdout": {"text": "tail"}},
            }),
        );

        assert!(reload_persistent_handles(&ctx, 1).is_empty());
        let task = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        assert!(matches!(&task.state, TaskState::Failed { error } if error.contains("code 5")));
        let result = task.result.as_ref().expect("result");
        assert_eq!(result.exit_code, Some(5));
        assert_eq!(
            result.output.as_ref().unwrap()["stdout"]["text"],
            serde_json::json!("tail")
        );
    }

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

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty());

        let task = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap()
        });
        // 종료 결과를 받을 수 없으므로 실패로 꾸미지 않고 결과 불명으로 둔다.
        match &task.state {
            TaskState::Unknown {
                reason: Some(reason),
            } => assert!(reason.starts_with(RUN_RESULT_LOST), "got {reason}"),
            other => panic!("expected Unknown, got {other:?}"),
        }
        let handle_left = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .unwrap()
                .is_some()
        });
        assert!(
            !handle_left,
            "the handle is dropped once the task is unknown"
        );
    }

    #[test]
    fn reload_persistent_handles_evicts_stale_when_task_not_running() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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
                .unwrap()
                .id
        });
        let handle = DispatchHandle::ShellProcess {
            pid: std::process::id(),
        };
        put_handle(&ctx, 1, &task_id, &handle);

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(alive.is_empty(), "non-Running task handle is stale");

        let still_there: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "stale handle should be evicted");

        let state: TaskState = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        assert!(matches!(state, TaskState::Ready), "got {state:?}");
    }

    /// reload 순간 이미 지난 AwaitExternal 기한은 실패로 처리한다.
    #[test]
    fn reload_persistent_handles_fails_await_external_past_deadline() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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
                wait_key: "hook-1".into(),
                deadline_ms: 1,
            },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(
            alive.is_empty(),
            "past-deadline AwaitExternal must not restore alive"
        );

        let task = ctx.with_store(|mem| {
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

        let still_there: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!still_there, "expired handle should be evicted");
    }

    /// reload 순간 미만료인 AwaitExternal은 복원한다. 이후 기한 만료는 restored_await_external_tests가 확인한다.
    #[test]
    fn reload_persistent_handles_restores_await_external_before_deadline() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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

        let state: TaskState = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        assert!(matches!(state, TaskState::Running), "got {state:?}");
    }

    /// 응답 통로는 재시작을 넘지 못한다. 이미 시작한 실행 요청을 다시 보내지 않고 결과 불명으로 둔다.
    #[test]
    fn reload_persistent_handles_marks_an_awaited_dispatch_answer_unknown() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let t = store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: "t".into(),
                    command: TaskCommand::Custom {
                        ipc_method: "acme.spawn".into(),
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
            &DispatchHandle::AwaitDispatchAnswer {
                workspace_id: 1,
                task_id: task_id.clone(),
                ipc_method: "acme.spawn".into(),
            },
        );

        let alive = reload_persistent_handles(&ctx, 1);
        assert!(
            alive.is_empty(),
            "the answer channel does not survive a restart"
        );

        let state: TaskState = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &task_id).unwrap().unwrap().state
        });
        match state {
            TaskState::Unknown {
                reason: Some(reason),
            } => assert!(
                reason.starts_with(DISPATCH_OUTCOME_UNKNOWN) && reason.contains("acme.spawn"),
                "unexpected reason: {reason}"
            ),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn expire_overdue_hook_waits_fails_the_task_and_removes_the_entry() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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

        assert_eq!(ctx.hook_task_waits.resolve(1), None);

        let task = ctx.with_store(|mem| {
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

    #[test]
    fn global_hook_expiry_notifies_the_origin_engine_and_its_downstream() {
        use crate::event_feed::{AgentEvent, AgentEventQueue};
        use crate::hook_wait::HookWaitOwner;
        use crate::task_waker::TaskWakerHub;

        let (_td, mut origin) = fresh_ctx();
        let origin_feed = Arc::new(AgentEventQueue::new());
        origin.task_waker_hub = Arc::new(TaskWakerHub::with_feed(origin_feed.clone()));
        let mut sweeping = origin.clone();
        let sweeping_feed = Arc::new(AgentEventQueue::new());
        sweeping.task_waker_hub = Arc::new(TaskWakerHub::with_feed(sweeping_feed.clone()));
        sweeping.agent_seq = Arc::new(AtomicU64::new(500));
        let (parent, child) = origin.with_store(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, origin.agent_seq.as_ref());
            let opts = |name: &str, depends_on| TaskCreateOpts {
                workspace_id: 7,
                name: name.into(),
                command: TaskCommand::Custom {
                    ipc_method: "acme.start".into(),
                    params: serde_json::json!({}),
                    poll: None,
                },
                depends_on,
                on_failure: OnFailure::Abort,
                metadata: serde_json::Value::Null,
                now_ms: 1000,
            };
            let parent = store.create(opts("parent", vec![])).unwrap();
            let child = store
                .create(opts("child", vec![parent.id.clone()]))
                .unwrap();
            store
                .set_state(7, &parent.id, TaskState::Running, 1100)
                .unwrap();
            (parent.id, child.id)
        });
        origin.hook_task_waits.register_owned(
            1,
            7,
            parent.clone(),
            2000,
            HookWaitOwner {
                agent_seq: origin.agent_seq.clone(),
                completion: origin.task_waker_hub.clone(),
            },
            None,
        );
        // No origin runner is running. A process-wide sweep still expires its wait.
        expire_overdue_hook_waits(&sweeping, 5000);
        let (events, dropped) = origin_feed.take_pending();
        assert_eq!(dropped, 0);
        assert_eq!(events.len(), 2);
        let finished = |id: &str, want: &str| {
            events.iter().any(|e| {
                matches!(e, AgentEvent::TaskFinished { workspace_id: 7, task_id, state, revision: Some(_), .. }
                    if task_id == id && *state == want)
            })
        };
        assert!(finished(&parent, "failed"), "{events:?}");
        assert!(finished(&child, "skipped"), "{events:?}");
        assert!(sweeping_feed.take_pending().0.is_empty());
        expire_overdue_hook_waits(&sweeping, 5000);
        assert!(origin_feed.take_pending().0.is_empty());
    }

    #[test]
    fn expire_overdue_hook_waits_leaves_fresh_entries_alone() {
        let (_td, ctx) = fresh_ctx();
        let task_id = ctx.with_store(|mem| {
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

        assert_eq!(ctx.hook_task_waits.resolve(2), Some((1, task_id.clone())));

        let task = ctx.with_store(|mem| {
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

    /// poison 뒤 liveness·stop은 복구하며 경고 표지가 기록되는지 확인한다. status의 별도 lock은 검사하지 않는다.
    #[test]
    fn a_poisoned_thread_map_still_answers_liveness_and_stop() {
        let registry = Arc::new(RunnerRegistry::new());

        let held = Arc::clone(&registry);
        let joined = thread::spawn(move || {
            let _guard = held.threads.lock().expect("fresh mutex");
            panic!("a thread dies while holding the runner registry");
        })
        .join();
        assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
        assert!(registry.threads.lock().is_err(), "poison 됐어야 한다");

        assert_eq!(
            registry.scoped_liveness(&Arc::new(crate::task_waker::TaskWakerHub::new()), 1),
            (false, false)
        );
        assert!(!registry.stop(1));
        assert!(
            registry.poison_reported.load(Ordering::Relaxed),
            "poison 은 보고돼야 한다"
        );
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    fn controlled(
        owner: &Arc<crate::task_waker::TaskWakerHub>,
    ) -> (Arc<RunnerControl>, mpsc::Sender<()>) {
        let (release_tx, release_rx) = mpsc::channel();
        let (stop_tx, _stop_rx) = mpsc::channel();
        let join = thread::spawn(move || {
            release_rx.recv().expect("release test worker");
        });
        (
            Arc::new(RunnerControl {
                owner: Arc::downgrade(owner),
                stop_tx,
                stopping: Arc::new(AtomicBool::new(false)),
                crashed: Arc::new(AtomicBool::new(false)),
                list_failures: Arc::new(AtomicU32::new(0)),
                join: Mutex::new(Some(join)),
                joined: std::sync::atomic::AtomicU8::new(0),
            }),
            release_tx,
        )
    }
    #[test]
    fn stop_matches_scope_and_waits_for_the_exact_worker() {
        let registry = Arc::new(RunnerRegistry::new());
        let origin = Arc::new(crate::task_waker::TaskWakerHub::new());
        let other = Arc::new(crate::task_waker::TaskWakerHub::new());
        let (control, release) = controlled(&origin);
        registry.lock_recovering().insert(7, control.clone());
        assert_eq!(
            registry.request_stop(&other, Some(7)).poll(),
            RunnerStopObservation::Joined
        );
        assert!(!control.stopping.load(Ordering::Acquire));
        let receipt = registry.request_stop(&origin, Some(7));
        assert_eq!(receipt.poll(), RunnerStopObservation::Waiting);
        assert!(registry.lock_recovering().contains_key(&7));
        release.send(()).unwrap();
        control.join_blocking();
        assert_eq!(receipt.poll(), RunnerStopObservation::Joined);
        assert!(!registry.lock_recovering().contains_key(&7));
    }
    #[test]
    fn an_old_receipt_cannot_unregister_a_successor_and_poll_never_waits_on_join_lock() {
        let registry = Arc::new(RunnerRegistry::new());
        let owner = Arc::new(crate::task_waker::TaskWakerHub::new());
        let (old, release) = controlled(&owner);
        registry.lock_recovering().insert(7, old.clone());
        let receipt = registry.request_stop(&owner, Some(7));
        {
            let _joining = old.join.lock().unwrap();
            assert_eq!(receipt.poll(), RunnerStopObservation::Waiting);
        }
        release.send(()).unwrap();
        old.join_blocking();
        let (successor, release) = controlled(&owner);
        registry.lock_recovering().insert(7, successor.clone());
        assert_eq!(receipt.poll(), RunnerStopObservation::Joined);
        assert!(Arc::ptr_eq(
            registry.lock_recovering().get(&7).unwrap(),
            &successor
        ));
        assert!(!successor.stopping.load(Ordering::Acquire));
        release.send(()).unwrap();
        successor.join_blocking();
    }
}

#[cfg(all(test, unix))]
#[path = "runner_thread/postprocess_reload_tests.rs"]
mod postprocess_reload_tests;

#[cfg(all(test, unix))]
#[path = "runner_thread/cancel_stop_tests.rs"]
mod cancel_stop_tests;

#[cfg(all(test, unix))]
#[path = "runner_thread/ttl_renewal_tests.rs"]
mod ttl_renewal_tests;

#[cfg(test)]
#[path = "runner_thread/reload_shrink_tests.rs"]
mod reload_shrink_tests;

#[cfg(test)]
#[path = "runner_thread/restored_await_external_tests.rs"]
mod restored_await_external_tests;

#[cfg(all(test, unix))]
#[path = "runner_thread/post_spawn_tests.rs"]
mod post_spawn_tests;

#[cfg(test)]
#[path = "runner_thread/unanswered_completion_tests.rs"]
mod unanswered_completion_tests;

#[cfg(test)]
#[path = "runner_thread/stop_dispatch_tests.rs"]
mod stop_dispatch_tests;
