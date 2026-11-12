//! 러너 스레드에서 작업을 실행한다. Run은 자식 프로세스, Custom은 IPC와 완료 전략을 사용한다.
//! lease·작업 출력 치환, 실행 handle 보존, 폴링 결과 수집도 담당한다.

mod acquire;
mod agent;
mod agent_spawn;
mod attempt_record;
mod child_env;
mod clock;
mod command_inputs;
mod dispatch;
pub(crate) mod holding_warning;
mod holdings;
mod late_answer;
mod poll;
mod postprocess;
mod record_room;
mod report;
mod run_group;
mod run_result;
mod run_stop;
mod store_keys;
mod ttl_renewal;
mod typed_inputs;
#[cfg(test)]
use run_result::{
    CAPTURE_TAIL_CAP, POLL_FAILURE_SUMMARY_CAP, run_outcome_from_value, run_outcome_to_value,
};
pub(crate) use run_result::{
    RUN_RESULT_LOST, evict_run_result, evict_task_side_keys, evict_task_side_keys_in,
    load_run_result, persist_run_result, shell_outcome_from_status,
};
use run_result::{drain_capped, drain_capped_observed, summarize_poll_response};

pub(crate) use attempt_record::{HANDLE_ATTEMPT_FIELD, dispatch_attempt, handle_value};
pub(crate) use clock::HoldingClock;
use clock::now_ms;
use command_inputs::{substitute_lease_resource, substitute_task_outputs};
use dispatch::DispatchFailure;
pub(crate) use holding_warning::current_holding_warnings;
pub(crate) use holdings::{own_lease, own_semaphore, release_own_holdings, resumes_after_restart};
pub(crate) use late_answer::DISPATCH_OUTCOME_UNKNOWN;
#[cfg(all(test, unix))]
pub(crate) use postprocess::postprocess_result_key;
pub(crate) use postprocess::restored_handle as restored_postprocess_handle;
pub(crate) use report::{SharedReportLimits, append as report_append};
pub(crate) use run_stop::{RunProc, process_of_record, stored_process};
#[cfg(all(test, unix))]
pub(crate) use ttl_renewal::MIN_HOLDING_TTL_MS;
pub(crate) use ttl_renewal::check_holding_ttls;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use serde_json::json;
use tasty_agent::runner::{DispatchHandle, DispatchOutcome, PollOutcome, TaskExecutor};
use tasty_agent::{Task, TaskCommand, TaskId, TaskResult};
use tasty_memory::{HOST_OWNER, MemoryStorage, MemoryValue, PutOpts, Scope};

pub(crate) use store_keys::{
    HANDLE_KEY_PREFIX, handle_key, has_stored_handle, load_dispatch_handle,
    load_dispatch_handle_in, run_result_key,
};

use tasty_ipc::host_call::{HostIpcInjector, InjectError, LateAnswer, LateDispatch};

/// 최초 IPC 요청과 후속 poll 요청에 공통으로 사용하는 응답 대기 시간.
pub(crate) const HOST_DISPATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// injector 미준비 오류의 문구. agent 회차의 오류도 같은 문구를 쓴다.
pub(crate) const INJECTOR_UNINIT_MSG: &str = "host IPC injector not initialized";

/// injector가 아직 준비되지 않은 poll 실패를 잠시 Active로 두는 유예 시간.
pub(crate) const INJECTOR_GRACE_MS: u64 = 30_000;

#[derive(Clone)]
pub(crate) struct RunnerContext {
    pub(crate) scope_stopping: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) memory: tasty_memory::writer::MemoryAccess,
    pub(crate) agent_seq: Arc<AtomicU64>,
    pub(crate) host_ipc: Arc<OnceLock<HostIpcInjector>>,
    /// Core를 거치지 않는 러너의 종료 처리도 대기자를 깨울 수 있도록 같은 hub를 공유한다.
    pub(crate) task_waker_hub: Arc<crate::task_waker::TaskWakerHub>,
    /// 러너가 push 대기를 등록하고 호스트가 훅 결과를 전달하는 공유 매핑.
    pub(crate) hook_task_waits: Arc<crate::hook_wait::HookTaskWaits>,
    pub(crate) agent_turns: Arc<crate::agent_turns::AgentTurns>,
    pub(crate) completion: Arc<dyn crate::completion::CompletionResolver>,
    /// 설정이 정하는 report 상한.
    pub(crate) report_limits: Arc<SharedReportLimits>,
    /// host IPC 요청 하나의 응답 대기 상한. 제품 경로는 [`HOST_DISPATCH_TIMEOUT`]이고 시험만 바꾼다.
    pub(crate) host_dispatch_timeout: Duration,
}

static RUN_RESULT_POISON_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

impl RunnerContext {
    /// The store transition may cancel downstream tasks as well as the requested task.
    /// Notify after releasing the memory lock, through this engine's hub only.
    pub(crate) fn fire_terminal_tasks(
        &self,
        workspace_id: u32,
        _tasks: impl IntoIterator<Item = Task>,
    ) {
        crate::terminal_recovery::reconcile(self, workspace_id);
    }

    /// 본문을 memory writer 순서 안에서 실행하고 결과를 기다린다. 러너 스레드와 그 worker만 부른다.
    /// 확정하지 못하면 결과형의 저장소 오류다([`crate::store_call`]).
    pub(crate) fn with_memory<R: crate::store_call::FromBlocked + Send + 'static>(
        &self,
        f: impl Fn(&mut dyn MemoryStorage) -> R + Send + Sync + 'static,
    ) -> R {
        crate::store_call::blocking(&self.memory, f)
    }

    /// 시험 전용. writer 없는 구성의 저장소를 바로 잠그고 값을 빌리는 본문을 실행한다.
    #[cfg(test)]
    pub(crate) fn with_store<R>(&self, f: impl FnOnce(&mut dyn MemoryStorage) -> R) -> R {
        let mut guard = self
            .memory
            .lock_inline_for_tests()
            .expect("test runner context uses an inline memory route");
        f(&mut *guard)
    }

    /// 결과형이 저장소 오류를 담지 못하는 본문. 확정하지 못하면 [`tasty_memory::writer::Blocked`]다.
    pub(crate) fn store<R: Send + 'static>(
        &self,
        f: impl Fn(&mut dyn MemoryStorage) -> R + Send + Sync + 'static,
    ) -> Result<R, tasty_memory::writer::Blocked> {
        self.memory.run_blocking(0, f)
    }

    /// 큐 입장 거절을 여기서 재시도하면 적체를 늘리므로 오류를 그대로 전달한다.
    pub(crate) fn dispatch_plugin(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, PluginCallError> {
        let inj = self.host_ipc.get().ok_or(PluginCallError::Uninitialized)?;
        inj.dispatch(method, params, self.host_dispatch_timeout)
            .map_err(PluginCallError::Inject)
    }

    /// 시작한 뒤 상한을 넘긴 요청의 응답 통로를 남긴다. 다시 보낼 수 없는 실행 요청에 쓴다.
    pub(crate) fn dispatch_plugin_keeping_late_answer(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, LateCall> {
        let inj = self
            .host_ipc
            .get()
            .ok_or(LateCall::Failed(PluginCallError::Uninitialized))?;
        inj.dispatch_keeping_late_answer(method, params, self.host_dispatch_timeout)
            .map_err(|e| match e {
                LateDispatch::Failed(e) => LateCall::Failed(PluginCallError::Inject(e)),
                LateDispatch::Pending(late) => LateCall::Pending(late),
            })
    }
}

/// 러너의 plugin IPC 호출 실패.
#[derive(Debug)]
pub(crate) enum PluginCallError {
    /// 호스트 IPC injector 가 아직 준비되지 않았다.
    Uninitialized,
    Inject(InjectError),
}

impl PluginCallError {
    /// 호스트가 응답 대기 상한 안에 답하지 못했다(큐 대기 중 만료 또는 실행 중 상한 경과).
    /// 조회처럼 다시 물어도 되는 요청은 다음 poll 에서 다시 묻는다.
    pub(crate) fn is_host_slow(&self) -> bool {
        matches!(
            self,
            PluginCallError::Inject(InjectError::Timeout(_) | InjectError::Expired(_))
        )
    }
}

impl std::fmt::Display for PluginCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginCallError::Uninitialized => f.write_str(INJECTOR_UNINIT_MSG),
            PluginCallError::Inject(e) => e.fmt(f),
        }
    }
}

/// [`RunnerContext::dispatch_plugin_keeping_late_answer`] 의 실패.
pub(crate) enum LateCall {
    Failed(PluginCallError),
    /// 요청은 시작됐고 응답은 아직 올 수 있다.
    Pending(LateAnswer),
}

/// watcher가 자식 종료와 두 출력 파이프의 EOF를 기다린 뒤 결과를 채운다.
/// executor가 사라져도(러너 정지) watcher는 분리되어 계속 기다린다. 취소는 [`run_group`] 으로 끝낸다.
struct ShellChildEntry {
    result: Arc<Mutex<Option<PollOutcome>>>,
    _watcher: thread::JoinHandle<()>,
}

pub(crate) struct HostExecutor {
    ctx: RunnerContext,
    /// Child는 watcher가 소유하고 Clone 가능한 DispatchHandle에는 PID만 남긴다.
    shell_children: HashMap<u32, ShellChildEntry>,
    /// 이 executor가 얻은 permit. 재시작 시 메모리 기록은 없어져 러너 시작 단계에서 저장된 holder를 정리한다.
    held_permits: HashMap<TaskId, (u32, String, String)>,
    /// 이 executor가 얻은 lease. 재시작 정리는 metadata.resource와 task ID holder로 찾은 Running 작업에 한정된다.
    held_leases: HashMap<TaskId, (u32, String, String)>,
    /// workspace가 없는 handle도 삭제할 수 있도록 저장 시 task별 workspace를 기억한다.
    held_handles: HashMap<TaskId, u32>,
    /// 이 executor가 턴 표에 묶은 agent 회차의 workspace. 턴 표는 workspace 사이에 공유된다.
    held_turns: HashMap<TaskId, u32>,
    /// workspace executor의 모든 PolledDispatch가 공유한다. 한 poll이라도 성공하면 유예를 초기화한다.
    injector_grace_deadline_ms: Option<u64>,
    postprocess: postprocess::PostprocessRuns,
    /// task 별로 마지막에 시작한(또는 재시작 뒤 넘겨받은) 프로세스. 끝낼 대상을 PID·시작 시각으로 확인한다.
    run_procs: HashMap<TaskId, RunProc>,
    /// 종결돼 프로세스 묶음을 끝냈고 종료를 기다리는 task. 끝난 것을 확인한 뒤에야 점유를 놓는다.
    stopping_runs: Vec<TaskId>,
    /// TTL 을 둔 lease·permit 의 갱신 상태. `held_leases`·`held_permits` 와 함께 놓는다.
    ttl_renewals: HashMap<(TaskId, ttl_renewal::Holding), ttl_renewal::TtlRenewal>,
    /// TTL 점유와 task 조회 간격·기한에 쓰는 시각.
    holding_clock: HoldingClock,
    /// 상한을 넘긴 Custom 요청의 늦은 응답 대기와, 응답 뒤 이어 간 handle.
    late: late_answer::LateAnswers,
    /// task별 마지막 조회가 끝난 뒤의 다음 조회 시각. 결과나 응답은 공유하지 않는다.
    next_poll_ms: HashMap<TaskId, u64>,
    /// 이 executor를 돌리는 러너의 정지 요청. tick이 dispatch 사이마다 읽는다.
    stop_signal: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl HostExecutor {
    pub(crate) fn new(ctx: RunnerContext) -> Self {
        Self {
            ctx,
            shell_children: HashMap::new(),
            held_permits: HashMap::new(),
            held_leases: HashMap::new(),
            held_handles: HashMap::new(),
            held_turns: HashMap::new(),
            injector_grace_deadline_ms: None,
            postprocess: Default::default(),
            run_procs: HashMap::new(),
            stopping_runs: Vec::new(),
            ttl_renewals: HashMap::new(),
            holding_clock: HoldingClock::system(),
            late: Default::default(),
            next_poll_ms: HashMap::new(),
            stop_signal: None,
        }
    }

    /// 러너의 정지 요청을 `signal` 로 받는다. 정지가 요청되면 같은 tick에서 남은 Ready 작업을 보내지 않는다.
    pub(crate) fn with_stop_signal(mut self, signal: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.stop_signal = Some(signal);
        self
    }

    /// TTL 점유와 task 조회 간격·기한의 시각을 `clock` 으로 정한다.
    // 사용처인 TTL 갱신 시험이 unix 전용이다.
    #[cfg(all(test, unix))]
    pub(crate) fn with_holding_clock(mut self, clock: HoldingClock) -> Self {
        self.holding_clock = clock;
        self
    }

    /// Started를 반환하기 전에 실행 handle을 저장한다. workspace는 handle에 없을 수 있어 별도로 받는다.
    /// 즉시 종료 handle은 저장하지 않는다. 저장이 확정되지 않아도 실행은 시작됐으므로 이 task 를 지켜본다.
    /// 결과를 모르는 저장이 실제로 확정됐다면 남은 handle 정리가 실행 중인 회차를 끝내지 않아야 하고,
    /// 확정되지 않았다면 정리 때의 delete 는 없는 키라 효과가 없다.
    /// `attempt` 는 이 dispatch 가 만들 v2 회차다. 재시작 복구가 이 회차로 보고해 다른 회차를 끝내지 않는다.
    fn persist_handle(
        &mut self,
        ws: u32,
        task_id: &TaskId,
        handle: &DispatchHandle,
        attempt: Option<&str>,
    ) {
        if matches!(
            handle,
            DispatchHandle::ReduceImmediate(_)
                | DispatchHandle::CustomImmediate(_)
                | DispatchHandle::ImmediateFail(_)
        ) {
            return;
        }
        let value = match serde_json::to_value(handle) {
            Ok(v) => run_stop::with_started_at(
                handle_value(v, attempt),
                self.run_procs.get(task_id),
                handle,
            ),
            Err(e) => {
                tracing::warn!("persist handle {task_id} serialize: {e}");
                return;
            }
        };
        let key = handle_key(task_id);
        let res = self.ctx.with_memory(move |mem| {
            mem.put(
                HOST_OWNER,
                &Scope::Workspace(ws),
                &key,
                &value,
                &PutOpts::default(),
            )
        });
        if let Err(e) = res {
            tracing::warn!("persist handle {task_id}: {e}");
        }
        self.held_handles.insert(task_id.clone(), ws);
    }

    /// 이 executor 가 handle 을 저장했거나 넘겨받아 아직 정리하지 않은 task 인가.
    pub(crate) fn watches(&self, task_id: &TaskId) -> bool {
        self.held_handles.contains_key(task_id)
    }

    fn evict_handle(&mut self, task_id: &TaskId) {
        let Some(ws) = self.held_handles.remove(task_id) else {
            return;
        };
        let key = handle_key(task_id);
        let res = self
            .ctx
            .with_memory(move |mem| mem.delete(HOST_OWNER, &Scope::Workspace(ws), &key, None));
        if let Err(e) = res {
            tracing::warn!("evict handle {task_id}: {e}");
        }
        evict_run_result(&self.ctx, ws, task_id);
    }
}

impl HostExecutor {
    /// 회차 시작 결과를 러너의 3갈래로 옮긴다. 시작하지 못했으면 얻은 점유를 돌려준다.
    fn settle_dispatch(
        &mut self,
        task: &Task,
        dispatch_result: Result<DispatchHandle, DispatchFailure>,
    ) -> DispatchOutcome {
        let result = match dispatch_result {
            Ok(h) => {
                let attempt = dispatch_attempt(task);
                self.persist_handle(task.workspace_id, &task.id, &h, attempt.as_deref());
                DispatchOutcome::Started(h)
            }
            Err(DispatchFailure::Permanent(e)) => DispatchOutcome::PermanentFail(e),
            // 회차를 시작하지 않았으므로 얻은 점유를 돌려주고 다음 tick 에 처음부터 다시 얻는다.
            Err(DispatchFailure::NothingRan(e)) => {
                tracing::warn!("agent task {}: {e}; retrying on a later tick", task.id);
                DispatchOutcome::Deferred
            }
        };
        if matches!(
            result,
            DispatchOutcome::PermanentFail(_) | DispatchOutcome::Deferred
        ) {
            self.release_permit(&task.id);
        }
        result
    }
}

impl TaskExecutor for HostExecutor {
    fn dispatch(&mut self, task: &Task) -> DispatchOutcome {
        // 이전 회차의 handle 이 남았으면 그 프로세스가 끝난 것을 아직 확인하지 못했다. 같은 holder id 로
        // 점유를 다시 얻으면 옛 프로세스와 새 회차가 자원을 함께 쓰므로, 정리될 때까지 Ready 로 둔다.
        let (ws, id) = (task.workspace_id, task.id.clone());
        // 읽지 못하면 읽기 실패와 같이 남지 않은 것으로 본다(has_stored_handle).
        if self
            .ctx
            .store(move |mem| has_stored_handle(&*mem, ws, &id))
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "agent task {}: handle read not answered", task.id);
                false
            })
        {
            tracing::debug!(
                "agent task {}: an earlier attempt's handle is still being settled; deferring",
                task.id
            );
            return DispatchOutcome::Deferred;
        }
        // 상한이 생기기 전에 저장된 큰 정의는 실행하지 않는다(완료를 기록할 몫이 없다).
        if let Err(e) = self.record_fits(task, "the definition") {
            return DispatchOutcome::PermanentFail(e);
        }
        match self.try_acquire_lease(task) {
            Ok(None) => {}
            Ok(Some(true)) => {}
            Ok(Some(false)) => return DispatchOutcome::Deferred,
            Err(e) => return DispatchOutcome::PermanentFail(format!("lease: {e}")),
        }
        match self.try_acquire_semaphore(task) {
            Ok(None) => {}
            Ok(Some(true)) => {}
            Ok(Some(false)) => {
                self.release_lease(&task.id);
                return DispatchOutcome::Deferred;
            }
            Err(e) => {
                self.release_lease(&task.id);
                return DispatchOutcome::PermanentFail(format!("semaphore: {e}"));
            }
        }
        // lease를 먼저, 선행 작업 출력을 나중에 치환한다.
        // 출력 데이터 안의 lease 표식을 다시 해석하지 않기 위한 순서다.
        let mut substituted = task.clone();
        if substituted.is_typed() {
            self.issue_report_token(&mut substituted);
        }
        let leased = self.held_leases.get(&task.id).cloned();
        if let Some((_, resource, _)) = &leased {
            substitute_lease_resource(&mut substituted.command, resource);
        }
        let outputs_substituted = match self.collect_task_outputs(task) {
            Ok(outputs) if outputs.is_empty() => Ok(false),
            Ok(outputs) => substitute_task_outputs(&mut substituted.command, &outputs),
            Err(e) => Err(e),
        }
        .and_then(|changed| self.substituted_fits(&substituted, changed));
        let dispatch_result = match outputs_substituted {
            Ok(changed) => {
                // 바뀐 command만 저장해 조회와 실제 실행 인자를 맞춘다.
                if let Some((ws, _, _)) = &leased {
                    self.persist_substituted_command(*ws, &substituted);
                } else if changed {
                    self.persist_substituted_command(task.workspace_id, &substituted);
                }
                // v2 입력은 치환을 마친 뒤 해석해 값이 다시 해석되지 않게 한다.
                self.resolve_typed_inputs(&mut substituted)
                    .map_err(DispatchFailure::from)
                    .and_then(|()| self.dispatch_command(&substituted))
            }
            Err(e) => Err(format!("task output substitution: {e}").into()),
        };
        self.settle_dispatch(task, dispatch_result)
    }

    fn poll(&mut self, handle: &DispatchHandle) -> PollOutcome {
        self.poll_handle(handle)
    }

    fn poll_task(&mut self, task_id: &TaskId, handle: &DispatchHandle) -> PollOutcome {
        self.poll_task_handle(task_id, handle)
    }

    fn release_permit(&mut self, task_id: &TaskId) {
        self.next_poll_ms.remove(task_id);
        // 실행 중인 후처리·Run 은 끝내고, 종료를 확인한 뒤(maintain)에야 자원을 놓는다.
        if !self.postprocess.cancel(task_id) && !self.stop_run(task_id) {
            self.release_resources(task_id);
        }
    }

    fn start_postprocess(&mut self, task: &Task, attempt_id: &str, run: u32) -> DispatchHandle {
        self.start_postprocess_run(task, attempt_id, run)
    }

    fn maintain(&mut self) {
        for task_id in self.postprocess.terminated() {
            self.release_resources(&task_id);
        }
        for task_id in self.stopped_runs() {
            self.release_resources(&task_id);
        }
        self.renew_holdings();
    }

    fn stop_requested(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.ctx.scope_stopping.load(Ordering::Acquire)
            || self
                .stop_signal
                .as_ref()
                .is_some_and(|signal| signal.load(Ordering::Acquire))
    }
}

#[cfg(all(test, unix))]
#[path = "runner_host/poll_interval_tests.rs"]
mod poll_interval_tests;

#[cfg(test)]
#[path = "runner_host/typed_tests.rs"]
mod typed_tests;

#[cfg(test)]
#[path = "runner_host/binding_tests.rs"]
mod binding_tests;

#[cfg(test)]
#[path = "runner_host/record_limit_tests.rs"]
mod record_limit_tests;

// The only test spawns a unix shell command; its helpers would be dead elsewhere.
#[cfg(all(test, unix))]
#[path = "runner_host/acquire_unknown_tests.rs"]
mod acquire_unknown_tests;

#[cfg(test)]
#[path = "runner_host/same_batch_acquire_tests.rs"]
mod same_batch_acquire_tests;

#[cfg(test)]
// 이유: 시험의 반환값 무시는 허용하되 제품 코드의 검사는 유지한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;
    use tasty_memory::MemoryStore;

    pub(super) fn fresh_ctx() -> (tempfile::TempDir, RunnerContext) {
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

    #[test]
    fn persist_handle_round_trip_via_memory_store() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let handle = DispatchHandle::ShellProcess { pid: 4242 };
        exec.persist_handle(1, &"t-test".to_string(), &handle, None);

        let loaded: Option<DispatchHandle> = ctx.with_store(|mem| {
            let entry = mem
                .get(&Scope::Workspace(1), &handle_key("t-test"))
                .ok()??;
            match entry.value {
                MemoryValue::Json(v) => serde_json::from_value(v).ok(),
                _ => None,
            }
        });
        let loaded = loaded.expect("handle loaded");
        match loaded {
            DispatchHandle::ShellProcess { pid } => assert_eq!(pid, 4242),
            other => panic!("expected ShellProcess, got {other:?}"),
        }
    }

    /// 회차 id 는 handle 옆에 저장되고, handle 을 읽는 쪽은 그 키를 무시한다.
    #[test]
    fn a_persisted_handle_keeps_its_attempt_and_still_reads_back() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let handle = DispatchHandle::ShellProcess { pid: 4242 };
        exec.persist_handle(1, &"t-test".to_string(), &handle, Some("t-test#3"));

        let raw = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key("t-test"))
                .unwrap()
                .unwrap()
                .value
        });
        let MemoryValue::Json(raw) = raw else {
            panic!("handle is json");
        };
        assert_eq!(raw[HANDLE_ATTEMPT_FIELD], "t-test#3");
        assert!(matches!(
            load_dispatch_handle(&ctx, 1, "t-test"),
            Some(DispatchHandle::ShellProcess { pid: 4242 })
        ));
    }

    #[test]
    fn persist_handle_skips_immediate_variants() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let immediates = vec![
            DispatchHandle::ImmediateFail("e".into()),
            DispatchHandle::ReduceImmediate(TaskResult {
                exit_code: Some(0),
                output: None,
                error: None,
            }),
            DispatchHandle::CustomImmediate(TaskResult {
                exit_code: Some(0),
                output: None,
                error: None,
            }),
        ];
        for (i, h) in immediates.into_iter().enumerate() {
            let id = format!("t-im-{i}");
            exec.persist_handle(1, &id, &h, None);
            let present: bool = ctx.with_store(|mem| {
                mem.get(&Scope::Workspace(1), &handle_key(&id))
                    .map(|v| v.is_some())
                    .unwrap_or(false)
            });
            assert!(!present, "Immediate handle {h:?} should not persist");
        }
    }

    #[test]
    fn run_outcome_done_serde_round_trip() {
        let outcome = PollOutcome::Done(TaskResult {
            exit_code: Some(0),
            output: Some(json!({ "pid": 1234u32 })),
            error: None,
        });
        let v = run_outcome_to_value(&outcome);
        let back = run_outcome_from_value(&v).expect("round trip");
        match back {
            PollOutcome::Done(r) => {
                assert_eq!(r.exit_code, Some(0));
                assert_eq!(r.output, Some(json!({ "pid": 1234u32 })));
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn run_outcome_failed_serde_round_trip() {
        let outcome = PollOutcome::Failed("Run exited with code 1".into());
        let v = run_outcome_to_value(&outcome);
        let back = run_outcome_from_value(&v).expect("round trip");
        match back {
            PollOutcome::Failed(err) => assert_eq!(err, "Run exited with code 1"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn run_outcome_exited_serde_round_trip() {
        let outcome = PollOutcome::Exited(TaskResult {
            exit_code: Some(5),
            output: Some(json!({"pid": 1, "stdout": {"text": "tail"}})),
            error: Some("Run exited with code 5".into()),
        });
        let v = run_outcome_to_value(&outcome);
        assert_eq!(v["kind"], json!("failed"));
        match run_outcome_from_value(&v).expect("round trip") {
            PollOutcome::Exited(r) => {
                assert_eq!(r.exit_code, Some(5));
                assert_eq!(
                    r.output,
                    Some(json!({"pid": 1, "stdout": {"text": "tail"}}))
                );
                assert_eq!(r.error.as_deref(), Some("Run exited with code 5"));
            }
            other => panic!("expected Exited, got {other:?}"),
        }
    }

    /// 종료 코드·출력을 싣기 전에 저장한 실패 결과는 사유만 있는 실패로 읽는다.
    #[test]
    fn a_failed_run_result_stored_without_exit_code_reads_as_a_plain_failure() {
        let v = json!({"kind": "failed", "error": "Run exited with code 1"});
        assert!(matches!(
            run_outcome_from_value(&v),
            Some(PollOutcome::Failed(e)) if e == "Run exited with code 1"
        ));
        let v = json!({"kind": "failed", "error": "x", "exit_code": null, "output": null});
        assert!(matches!(
            run_outcome_from_value(&v),
            Some(PollOutcome::Failed(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn shell_dispatch_watcher_persists_exit_code_on_success() {
        use tasty_agent::OnFailure;
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let task = Task {
            id: "t-sh-ok".to_string(),
            workspace_id: 1,
            name: "shell-ok".into(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id: 1,
                cwd: None,
            },
            state: tasty_agent::TaskState::Running,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        let outcome = exec.dispatch(&task);
        let handle = match outcome {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        let pid = match handle {
            DispatchHandle::ShellProcess { pid } => pid,
            other => panic!("expected ShellProcess, got {other:?}"),
        };
        let mut final_outcome: Option<PollOutcome> = None;
        for _ in 0..40 {
            match exec.poll(&handle) {
                PollOutcome::Active => std::thread::sleep(Duration::from_millis(50)),
                other => {
                    final_outcome = Some(other);
                    break;
                }
            }
        }
        let outcome = final_outcome.expect("watcher should have completed");
        match outcome {
            PollOutcome::Done(r) => assert_eq!(r.exit_code, Some(0)),
            other => panic!("expected Done, got {other:?}"),
        }
        let loaded = load_run_result(&ctx, 1, &task.id).expect("persisted");
        match loaded {
            PollOutcome::Done(r) => assert_eq!(r.exit_code, Some(0)),
            other => panic!("expected persisted Done, got {other:?}"),
        }
        assert!(!exec.shell_children.contains_key(&pid));
    }

    /// poison 뒤 결과 조회를 검사한다. watcher 쓰기의 실행 타이밍을 재현하는 시험은 아니다.
    #[test]
    fn a_poisoned_run_result_cell_still_delivers_the_outcome() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);

        let cell = Arc::new(Mutex::new(Some(PollOutcome::Done(TaskResult {
            exit_code: Some(0),
            output: None,
            error: None,
        }))));
        let poisoner = cell.clone();
        let _ = thread::spawn(move || {
            let _guard = poisoner.lock().expect("fresh lock");
            panic!("poison the run result cell on purpose");
        })
        .join();
        assert!(
            cell.is_poisoned(),
            "락이 실제로 poison 됐어야 전제가 성립한다"
        );

        let pid = 424242;
        exec.shell_children.insert(
            pid,
            ShellChildEntry {
                result: cell,
                _watcher: thread::spawn(|| {}),
            },
        );

        let handle = DispatchHandle::ShellProcess { pid };
        match exec.poll(&handle) {
            PollOutcome::Done(r) => assert_eq!(r.exit_code, Some(0)),
            other => panic!("poison 된 cell 에서도 결과를 꺼내야 한다, got {other:?}"),
        }
        assert!(!exec.shell_children.contains_key(&pid));
    }

    /// 재시작 뒤 복원한 Run handle(이 executor 의 watcher 가 없음)의 프로세스가 사라지면 종료
    /// 결과를 받을 수 없어 실패가 아니라 결과 불명이다.
    #[test]
    fn a_restored_run_whose_process_is_gone_reports_a_lost_result() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let pid = 0xFFFF_FFFE;
        match exec.poll(&DispatchHandle::ShellProcess { pid }) {
            PollOutcome::Lost(reason) => {
                assert!(reason.starts_with(RUN_RESULT_LOST), "{reason}");
                assert!(reason.contains(&pid.to_string()), "{reason}");
            }
            other => panic!("expected a lost result, got {other:?}"),
        }
    }

    /// 값만 만드는 공용 task 빌더. 실제 프로세스 실행 시험에는 별도 Unix 조건을 둔다.
    fn mk_run_task(id: &str, command: Vec<&str>) -> Task {
        use tasty_agent::OnFailure;
        Task {
            id: id.to_string(),
            workspace_id: 1,
            name: id.to_string(),
            command: TaskCommand::Run {
                command: command.into_iter().map(str::to_string).collect(),
                workspace_id: 1,
                cwd: None,
            },
            state: tasty_agent::TaskState::Running,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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

    /// 지정한 횟수만 폴링한다. sleep 합 외에 poll 자체도 기다릴 수 있어 실제 시간 상한은 아니다.
    #[cfg(unix)]
    fn poll_until_terminal(
        exec: &mut HostExecutor,
        handle: &DispatchHandle,
        max_ticks: u32,
    ) -> PollOutcome {
        for _ in 0..max_ticks {
            match exec.poll(handle) {
                PollOutcome::Active => std::thread::sleep(Duration::from_millis(50)),
                other => return other,
            }
        }
        panic!(
            "task did not reach a terminal state; nominal polling sleep budget: {}ms",
            max_ticks * 50
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_dispatch_captures_stdout_in_output() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let task = mk_run_task("t-sh-capture", vec!["sh", "-c", "echo hello; exit 0"]);
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 40) {
            PollOutcome::Done(r) => {
                assert_eq!(r.exit_code, Some(0));
                let stdout_text = r
                    .output
                    .as_ref()
                    .and_then(|o| o.get("stdout"))
                    .and_then(|s| s.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                assert!(
                    stdout_text.contains("hello"),
                    "expected stdout text to contain 'hello', got {stdout_text:?}"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn shell_dispatch_nonzero_exit_fails_with_exit_code_in_error() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let task = mk_run_task("t-sh-fail", vec!["sh", "-c", "echo out; exit 3"]);
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 40) {
            PollOutcome::Exited(r) => {
                let err = r.error.expect("error");
                assert!(
                    err.starts_with("Run exited with code 3\n"),
                    "expected error to mention exit code 3, got {err}"
                );
                assert_eq!(r.exit_code, Some(3));
                let output = r.output.expect("output");
                assert_eq!(output["stdout"]["text"], json!("out\n"));
            }
            other => panic!("expected Exited, got {other:?}"),
        }
    }

    /// 파이프 용량을 넘는 stdout을 계속 읽어 자식과 wait가 서로 막히지 않는지 확인한다.
    #[cfg(unix)]
    #[test]
    fn shell_dispatch_large_stdout_does_not_deadlock() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let task = mk_run_task("t-sh-bigout", vec!["sh", "-c", "yes | head -c 2000000"]);
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 200) {
            PollOutcome::Done(r) => {
                assert_eq!(r.exit_code, Some(0));
                let stdout = r.output.as_ref().and_then(|o| o.get("stdout")).cloned();
                let truncated = stdout
                    .as_ref()
                    .and_then(|s| s.get("truncated"))
                    .and_then(|t| t.as_bool())
                    .unwrap_or(false);
                assert!(truncated, "2MB stdout should exceed the 64KiB tail cap");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    /// stderr만 차도 자식이 멈출 수 있어 별도로 검사한다.
    #[cfg(unix)]
    #[test]
    fn shell_dispatch_large_stderr_does_not_deadlock() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let task = mk_run_task(
            "t-sh-bigerr",
            vec!["sh", "-c", "yes | head -c 2000000 1>&2"],
        );
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 200) {
            PollOutcome::Done(r) => {
                assert_eq!(r.exit_code, Some(0));
                let stderr = r.output.as_ref().and_then(|o| o.get("stderr")).cloned();
                let truncated = stderr
                    .as_ref()
                    .and_then(|s| s.get("truncated"))
                    .and_then(|t| t.as_bool())
                    .unwrap_or(false);
                assert!(truncated, "2MB stderr should exceed the 64KiB tail cap");
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn drain_capped_truncates_to_tail_and_tracks_dropped_bytes() {
        let total = CAPTURE_TAIL_CAP + 100;
        let mut input = vec![b'a'; total];
        // 끝부분에 다른 바이트를 넣어 앞부분이 아닌 tail이 남았는지 확인한다.
        for b in input.iter_mut().rev().take(100) {
            *b = b'b';
        }
        let result = drain_capped(std::io::Cursor::new(input));
        assert_eq!(result.data.len(), CAPTURE_TAIL_CAP);
        assert!(result.truncated);
        assert_eq!(result.dropped_bytes, 100);
        assert!(result.data.iter().all(|&b| b == b'b' || b == b'a'));
        assert!(result.data.ends_with(&[b'b'; 100]));
    }

    fn mk_polled(method: &str) -> DispatchHandle {
        DispatchHandle::PolledDispatch {
            workspace_id: 1,
            poll_method: method.to_string(),
            poll_params: json!({}),
            state_field: "state".to_string(),
            terminal_states: vec!["done".to_string()],
            failure_states: vec![],
            interval_ms: 1,
            deadline_ms: None,
        }
    }

    /// 지정한 응답을 요청 순서대로 보내는 모의 IPC. 호출자는 반환한 워커를 join한다.
    fn spawn_fake_injector(
        ctx: &RunnerContext,
        responses: Vec<serde_json::Value>,
    ) -> std::thread::JoinHandle<()> {
        use std::sync::mpsc;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        std::thread::spawn(move || {
            for resp in responses {
                let cmd = rx.recv().expect("recv poll request");
                cmd.response_tx
                    .send(JsonRpcResponse::success(
                        cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                        resp,
                    ))
                    .expect("send poll response");
            }
        })
    }

    fn mk_polled_with_states(method: &str, terminal: &[&str], failure: &[&str]) -> DispatchHandle {
        DispatchHandle::PolledDispatch {
            workspace_id: 1,
            poll_method: method.to_string(),
            poll_params: json!({}),
            state_field: "state".to_string(),
            terminal_states: terminal.iter().map(|s| s.to_string()).collect(),
            failure_states: failure.iter().map(|s| s.to_string()).collect(),
            interval_ms: 1,
            deadline_ms: None,
        }
    }

    #[test]
    fn polled_dispatch_failure_state_yields_failed_not_done() {
        let (_td, ctx) = fresh_ctx();
        let worker = spawn_fake_injector(&ctx, vec![json!({ "state": "exited", "why": "killed" })]);
        let mut exec = HostExecutor::new(ctx);
        let handle = mk_polled_with_states("fake.poll", &["idle"], &["exited"]);
        match exec.poll(&handle) {
            PollOutcome::Failed(msg) => {
                assert!(msg.contains("exited"), "{msg}");
                assert!(msg.contains("killed"), "response summary missing: {msg}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        worker.join().unwrap();
    }

    /// 성공·실패 목록이 겹치면 실패를 우선한다.
    #[test]
    fn polled_dispatch_failure_states_take_precedence_over_terminal() {
        let (_td, ctx) = fresh_ctx();
        let worker = spawn_fake_injector(&ctx, vec![json!({ "state": "exited" })]);
        let mut exec = HostExecutor::new(ctx);
        let handle = mk_polled_with_states("fake.poll", &["idle", "exited"], &["exited"]);
        assert!(matches!(exec.poll(&handle), PollOutcome::Failed(_)));
        worker.join().unwrap();
    }

    #[test]
    fn polled_dispatch_without_failure_states_still_succeeds_on_terminal() {
        let (_td, ctx) = fresh_ctx();
        let worker = spawn_fake_injector(&ctx, vec![json!({ "state": "exited" })]);
        let mut exec = HostExecutor::new(ctx);
        let handle = mk_polled_with_states("fake.poll", &["idle", "exited"], &[]);
        assert!(matches!(exec.poll(&handle), PollOutcome::Done(_)));
        worker.join().unwrap();
    }

    #[test]
    fn poll_spec_without_failure_states_deserializes() {
        let spec: tasty_agent::PollSpec = serde_json::from_str(
            r#"{"poll_method":"x","state_field":"state","terminal_states":["idle"]}"#,
        )
        .expect("legacy inline PollSpec should still deserialize");
        assert!(spec.failure_states.is_empty());
        assert_eq!(spec.interval_ms, 500);
    }

    /// 긴 응답을 줄일 때 UTF-8 문자 경계를 지켜야 한다.
    #[test]
    fn poll_failure_summary_truncates_on_char_boundary() {
        let big = "가".repeat(POLL_FAILURE_SUMMARY_CAP);
        let summary = summarize_poll_response(&json!({ "state": "exited", "log": big }));
        assert!(summary.ends_with("...(truncated)"), "{}", &summary[..80]);
        assert!(summary.len() <= POLL_FAILURE_SUMMARY_CAP + "...(truncated)".len());
    }

    #[test]
    fn polled_dispatch_poll_injector_uninit_returns_active_within_grace() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let handle = mk_polled("fake.poll");
        let outcome = exec.poll(&handle);
        assert!(
            matches!(outcome, PollOutcome::Active),
            "expected Active, got {outcome:?}"
        );
        assert!(
            exec.injector_grace_deadline_ms.is_some(),
            "deadline should be set on first uninit"
        );
    }

    #[test]
    fn polled_dispatch_poll_after_grace_expired_returns_failed() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let handle = mk_polled("fake.poll");
        let _first = exec.poll(&handle);
        debug_assert!(matches!(_first, PollOutcome::Active));
        exec.injector_grace_deadline_ms = Some(0);
        let outcome = exec.poll(&handle);
        match outcome {
            PollOutcome::Failed(err) => {
                assert!(err.contains("injector grace expired"), "got {err}");
                assert!(err.contains("fake.poll"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn polled_dispatch_recovers_after_injector_ready() {
        use std::sync::mpsc;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let handle = mk_polled("fake.poll");
        let outcome1 = exec.poll(&handle);
        assert!(matches!(outcome1, PollOutcome::Active));
        assert!(exec.injector_grace_deadline_ms.is_some());

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        let injector = HostIpcInjector::new(tx, waker);
        ctx.host_ipc.set(injector).ok().expect("set once");
        let worker = std::thread::spawn(move || {
            let cmd = rx.recv().expect("recv fake.poll");
            assert_eq!(cmd.request.method, "fake.poll");
            let resp = JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({ "state": "done" }),
            );
            cmd.response_tx.send(resp).expect("send resp");
        });
        let outcome2 = exec.poll(&handle);
        worker.join().unwrap();
        match outcome2 {
            PollOutcome::Done(_) => {}
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(
            exec.injector_grace_deadline_ms.is_none(),
            "deadline should reset after successful dispatch"
        );
    }

    #[test]
    fn polled_dispatch_non_injector_error_fails_immediately() {
        use std::sync::mpsc;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let handle = mk_polled("fake.poll");
        // timeout을 실제로 기다리지 않고 수신자를 닫아 다른 종류의 IPC 실패를 만든다.
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        drop(rx);
        let waker = std::sync::Arc::new(|| {});
        let injector = HostIpcInjector::new(tx, waker);
        ctx.host_ipc.set(injector).ok().expect("set once");
        let outcome = exec.poll(&handle);
        match outcome {
            PollOutcome::Failed(err) => {
                assert!(err.contains("fake.poll"), "got {err}");
                assert!(
                    !err.contains("grace expired"),
                    "non-injector error must not use grace path: {err}"
                );
                assert!(
                    !err.contains("injector not initialized"),
                    "should not be uninit path: {err}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(exec.injector_grace_deadline_ms.is_none());
    }

    #[test]
    fn custom_with_poll_maps_params_and_polls_to_done() {
        use std::collections::HashMap;
        use std::sync::mpsc;
        use tasty_agent::{OnFailure, PollSpec, PollSpecRef};
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv fake.start");
            assert_eq!(start.request.method, "fake.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "job": "J1" }),
                ))
                .expect("send start resp");

            let poll1 = rx.recv().expect("recv fake.poll 1");
            assert_eq!(poll1.request.method, "fake.poll");
            assert_eq!(poll1.request.params.get("surface_id"), Some(&json!(7)));
            assert_eq!(poll1.request.params.get("job"), Some(&json!("J1")));
            poll1
                .response_tx
                .send(JsonRpcResponse::success(
                    poll1.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "state": "running" }),
                ))
                .expect("send poll1 resp");

            let poll2 = rx.recv().expect("recv fake.poll 2");
            poll2
                .response_tx
                .send(JsonRpcResponse::success(
                    poll2.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "state": "done" }),
                ))
                .expect("send poll2 resp");
        });

        let mut map_from_request = HashMap::new();
        map_from_request.insert("surface_id".to_string(), "surface_id".to_string());
        let mut map_from_response = HashMap::new();
        map_from_response.insert("job".to_string(), "job".to_string());
        let task = Task {
            id: "t-poll".to_string(),
            workspace_id: 1,
            name: "poll".into(),
            command: TaskCommand::Custom {
                ipc_method: "fake.start".into(),
                params: json!({ "surface_id": 7 }),
                poll: Some(Box::new(PollSpecRef::Inline(PollSpec {
                    poll_method: "fake.poll".into(),
                    map_from_response,
                    map_from_request,
                    state_field: "state".into(),
                    terminal_states: vec!["done".into()],
                    failure_states: vec![],
                    interval_ms: 1,
                    timeout_ms: None,
                }))),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };

        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match &handle {
            DispatchHandle::PolledDispatch {
                poll_method,
                poll_params,
                ..
            } => {
                assert_eq!(poll_method, "fake.poll");
                assert_eq!(poll_params.get("surface_id"), Some(&json!(7)));
                assert_eq!(poll_params.get("job"), Some(&json!("J1")));
            }
            other => panic!("expected PolledDispatch, got {other:?}"),
        }
        assert!(matches!(exec.poll(&handle), PollOutcome::Active));
        match exec.poll(&handle) {
            PollOutcome::Done(r) => {
                assert_eq!(r.output, Some(json!({ "state": "done" })));
            }
            other => panic!("expected Done, got {other:?}"),
        }
        worker.join().unwrap();
    }

    #[test]
    fn custom_without_poll_is_immediate() {
        use std::sync::mpsc;
        use tasty_agent::OnFailure;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let cmd = rx.recv().expect("recv fake.do");
            cmd.response_tx
                .send(JsonRpcResponse::success(
                    cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "ok": true }),
                ))
                .expect("send resp");
        });
        let task = Task {
            id: "t-imm".to_string(),
            workspace_id: 1,
            name: "imm".into(),
            command: TaskCommand::Custom {
                ipc_method: "fake.do".into(),
                params: json!({}),
                poll: None,
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        worker.join().unwrap();
        match handle {
            DispatchHandle::CustomImmediate(r) => {
                assert_eq!(r.output, Some(json!({ "ok": true })));
            }
            other => panic!("expected CustomImmediate, got {other:?}"),
        }
    }

    #[test]
    fn custom_with_named_poll_strategy_resolves_and_polls() {
        use std::sync::mpsc;
        use tasty_agent::{OnFailure, PollSpecRef};
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, mut ctx) = fresh_ctx();
        ctx.completion = Arc::new(crate::completion::fixture::Resolver {
            strategy: Some(crate::completion::CompletionStrategy {
                id: "rhtest1/wait-done".into(),
                kind: crate::completion::CompletionKind::Poll(
                    serde_json::from_value(json!({
                        "poll_method": "rhtest1.poll",
                        "state_field": "state",
                        "terminal_states": ["done"],
                        "interval_ms": 1,
                    }))
                    .unwrap(),
                ),
            }),
            default_method: None,
        });
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv rhtest1.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({}),
                ))
                .expect("send start resp");
            let poll = rx.recv().expect("recv rhtest1.poll");
            poll.response_tx
                .send(JsonRpcResponse::success(
                    poll.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "state": "done" }),
                ))
                .expect("send poll resp");
        });
        let task = Task {
            id: "t-named".to_string(),
            workspace_id: 1,
            name: "named".into(),
            command: TaskCommand::Custom {
                ipc_method: "rhtest1.start".into(),
                params: json!({}),
                poll: Some(Box::new(PollSpecRef::Named {
                    strategy: "rhtest1/wait-done".into(),
                })),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match exec.poll(&handle) {
            PollOutcome::Done(r) => assert_eq!(r.output, Some(json!({ "state": "done" }))),
            other => panic!("expected Done, got {other:?}"),
        }
        worker.join().unwrap();
    }

    #[test]
    fn custom_with_named_push_strategy_registers_hook_and_awaits_external() {
        use std::sync::mpsc;
        use tasty_agent::{OnFailure, PollSpecRef};
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, mut ctx) = fresh_ctx();
        ctx.completion = Arc::new(crate::completion::fixture::Resolver {
            strategy: Some(crate::completion::CompletionStrategy {
                id: "rhtest-push/wait-done".into(),
                kind: crate::completion::CompletionKind::Push {
                    notify_via: "rhtest-push/notify".into(),
                    timeout_ms: 60_000,
                },
            }),
            default_method: None,
        });
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv rhtest-push.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({}),
                ))
                .expect("send start resp");
            let hook_set = rx.recv().expect("recv hook.set");
            assert_eq!(hook_set.request.method, "hook.set");
            assert_eq!(hook_set.request.params.get("surface_id"), Some(&json!(7)));
            assert_eq!(
                hook_set.request.params.get("event"),
                Some(&json!("command-completed"))
            );
            assert_eq!(
                hook_set.request.params.get("handler"),
                Some(&json!("rhtest-push/notify"))
            );
            assert_eq!(hook_set.request.params.get("once"), Some(&json!(true)));
            hook_set
                .response_tx
                .send(JsonRpcResponse::success(
                    hook_set
                        .request
                        .id
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "hook_id": 999 }),
                ))
                .expect("send hook.set resp");
        });
        let task = Task {
            id: "t-push".to_string(),
            workspace_id: 1,
            name: "push".into(),
            command: TaskCommand::Custom {
                ipc_method: "rhtest-push.start".into(),
                params: json!({ "surface_id": 7 }),
                poll: Some(Box::new(PollSpecRef::Named {
                    strategy: "rhtest-push/wait-done".into(),
                })),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        worker.join().unwrap();
        match &handle {
            DispatchHandle::AwaitExternal {
                wait_key,
                deadline_ms,
            } => {
                assert_eq!(wait_key, "999");
                assert!(
                    *deadline_ms > 0,
                    "deadline must be populated from timeout_ms"
                );
            }
            other => panic!("expected AwaitExternal, got {other:?}"),
        }
        assert!(matches!(exec.poll(&handle), PollOutcome::Active));
        assert_eq!(
            ctx.hook_task_waits.resolve(999),
            Some((1, "t-push".to_string()))
        );
    }

    #[test]
    fn custom_with_push_strategy_missing_surface_id_fails_dispatch() {
        use std::sync::mpsc;
        use tasty_agent::{OnFailure, PollSpecRef};
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, mut ctx) = fresh_ctx();
        ctx.completion = Arc::new(crate::completion::fixture::Resolver {
            strategy: Some(crate::completion::CompletionStrategy {
                id: "rhtest-push2/wait-done".into(),
                kind: crate::completion::CompletionKind::Push {
                    notify_via: "rhtest-push2/notify".into(),
                    timeout_ms: 60_000,
                },
            }),
            default_method: None,
        });
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv rhtest-push2.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({}),
                ))
                .expect("send start resp");
        });
        let task = Task {
            id: "t-push-no-surface".to_string(),
            workspace_id: 1,
            name: "push-no-surface".into(),
            command: TaskCommand::Custom {
                ipc_method: "rhtest-push2.start".into(),
                params: json!({}),
                poll: Some(Box::new(PollSpecRef::Named {
                    strategy: "rhtest-push2/wait-done".into(),
                })),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        match exec.dispatch(&task) {
            DispatchOutcome::PermanentFail(e) => assert!(e.contains("surface_id")),
            other => panic!("expected PermanentFail, got {other:?}"),
        }
        worker.join().unwrap();
    }

    /// 미등록 전략 오류 전에 원래 IPC는 이미 실행된다. 응답 뒤 전략 해석에서 실패하는 순서를 확인한다.
    #[test]
    fn custom_with_unknown_named_poll_strategy_fails_dispatch() {
        use std::sync::mpsc;
        use tasty_agent::{OnFailure, PollSpecRef};
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv rhtest2.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({}),
                ))
                .expect("send start resp");
        });
        let task = Task {
            id: "t-named-missing".to_string(),
            workspace_id: 1,
            name: "named-missing".into(),
            command: TaskCommand::Custom {
                ipc_method: "rhtest2.start".into(),
                params: json!({}),
                poll: Some(Box::new(PollSpecRef::Named {
                    strategy: "rhtest2/does-not-exist".into(),
                })),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        match exec.dispatch(&task) {
            DispatchOutcome::PermanentFail(e) => assert!(e.contains("rhtest2/does-not-exist")),
            other => panic!("expected PermanentFail, got {other:?}"),
        }
        worker.join().unwrap();
    }

    #[test]
    fn custom_without_poll_uses_default_for_method_strategy() {
        use std::sync::mpsc;
        use tasty_agent::OnFailure;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, mut ctx) = fresh_ctx();
        ctx.completion = Arc::new(crate::completion::fixture::Resolver {
            strategy: Some(crate::completion::CompletionStrategy {
                id: "rhtest3/auto-wait".into(),
                kind: crate::completion::CompletionKind::Poll(
                    serde_json::from_value(json!({
                        "poll_method": "rhtest3.poll",
                        "state_field": "state",
                        "terminal_states": ["done"],
                        "interval_ms": 1,
                    }))
                    .unwrap(),
                ),
            }),
            default_method: Some("rhtest3.start".into()),
        });
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let start = rx.recv().expect("recv rhtest3.start");
            start
                .response_tx
                .send(JsonRpcResponse::success(
                    start.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({}),
                ))
                .expect("send start resp");
            let poll = rx.recv().expect("recv rhtest3.poll");
            poll.response_tx
                .send(JsonRpcResponse::success(
                    poll.request.id.clone().unwrap_or(serde_json::Value::Null),
                    serde_json::json!({ "state": "done" }),
                ))
                .expect("send poll resp");
        });
        let task = Task {
            id: "t-default".to_string(),
            workspace_id: 1,
            name: "default".into(),
            command: TaskCommand::Custom {
                ipc_method: "rhtest3.start".into(),
                params: json!({}),
                poll: None,
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: serde_json::Value::Null,
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
        };
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        assert!(matches!(handle, DispatchHandle::PolledDispatch { .. }));
        match exec.poll(&handle) {
            PollOutcome::Done(r) => assert_eq!(r.output, Some(json!({ "state": "done" }))),
            other => panic!("expected Done, got {other:?}"),
        }
        worker.join().unwrap();
    }

    #[test]
    fn evict_handle_removes_from_store() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let task_id = "t-evict".to_string();
        let handle = mk_polled("fake.poll");
        exec.persist_handle(1, &task_id, &handle, None);
        let present_before: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(present_before, "persist should write entry");

        exec.evict_handle(&task_id);
        let present_after: bool = ctx.with_store(|mem| {
            mem.get(&Scope::Workspace(1), &handle_key(&task_id))
                .map(|v| v.is_some())
                .unwrap_or(false)
        });
        assert!(!present_after, "evict should remove entry");
        assert!(!exec.held_handles.contains_key(&task_id));
    }

    #[cfg(unix)]
    #[test]
    fn try_acquire_lease_pool_fixed_distributes_distinct_resources_and_defers() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let candidates = vec!["wt-1".to_string(), "wt-2".to_string(), "wt-3".to_string()];

        let mut got = Vec::new();
        for i in 0..3 {
            let mut task = mk_run_task(&format!("t-pool-{i}"), vec!["true"]);
            task.metadata = json!({ "lease": { "candidates": candidates } });
            assert_eq!(
                exec.try_acquire_lease(&task).unwrap(),
                Some(true),
                "task {i} should acquire a distinct slot"
            );
            got.push(exec.held_leases.get(&task.id).unwrap().1.clone());
        }
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            3,
            "expected 3 distinct resources, got {got:?}"
        );

        let mut task4 = mk_run_task("t-pool-3", vec!["true"]);
        task4.metadata = json!({ "lease": { "candidates": candidates } });
        assert_eq!(exec.try_acquire_lease(&task4).unwrap(), Some(false));
        assert!(!exec.held_leases.contains_key(&task4.id));

        for (resource, _holder) in exec.held_leases.values().map(|(_, r, h)| (r, h)) {
            assert!(
                candidates.contains(resource),
                "fixed mode must never synthesize, got {resource}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn try_acquire_lease_pool_elastic_unbounded_synthesizes_beyond_candidates() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let candidates = vec!["wt-1".to_string(), "wt-2".to_string(), "wt-3".to_string()];

        let mut got = Vec::new();
        for i in 0..5 {
            let mut task = mk_run_task(&format!("t-elastic-{i}"), vec!["true"]);
            task.metadata = json!({ "lease": { "candidates": candidates, "elastic": {} } });
            assert_eq!(
                exec.try_acquire_lease(&task).unwrap(),
                Some(true),
                "task {i} must not wait under unbounded elastic"
            );
            got.push(exec.held_leases.get(&task.id).unwrap().1.clone());
        }
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            5,
            "expected 5 distinct resources, got {got:?}"
        );
        let synthesized_count = got.iter().filter(|r| !candidates.contains(r)).count();
        assert_eq!(synthesized_count, 2, "3 fixed + 2 synthesized");
    }

    #[cfg(unix)]
    #[test]
    fn try_acquire_lease_pool_elastic_max_candidates_caps_synthesis() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let candidates = vec!["wt-1".to_string(), "wt-2".to_string(), "wt-3".to_string()];

        let mut acquired = 0;
        for i in 0..6 {
            let mut task = mk_run_task(&format!("t-cap-{i}"), vec!["true"]);
            task.metadata = json!({
                "lease": { "candidates": candidates, "elastic": { "max_candidates": 4 } }
            });
            if exec.try_acquire_lease(&task).unwrap() == Some(true) {
                acquired += 1;
            }
        }
        assert_eq!(acquired, 4);
    }

    #[cfg(unix)]
    #[test]
    fn try_acquire_lease_resource_sugar_conflicts_with_single_candidate_pool() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);

        let mut task_a = mk_run_task("t-sugar-a", vec!["true"]);
        task_a.metadata = json!({ "lease": { "resource": "single-path" } });
        assert_eq!(exec.try_acquire_lease(&task_a).unwrap(), Some(true));

        let mut task_b = mk_run_task("t-sugar-b", vec!["true"]);
        task_b.metadata = json!({ "lease": { "candidates": ["single-path"] } });
        assert_eq!(
            exec.try_acquire_lease(&task_b).unwrap(),
            Some(false),
            "candidates:['single-path'] must conflict with resource:'single-path' \
             on the same lease key"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dispatch_persists_lease_substituted_cwd_for_task_get() {
        use tasty_agent::TaskStore;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();
        // macOS 임시 경로는 symlink를 포함할 수 있어 pwd의 결과와 비교할 때 canonicalize한다.
        let resolved_path = dir
            .path()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let mut task = mk_run_task("t-persist-cwd", vec!["pwd"]);
        task.metadata = json!({ "lease": { "resource": dir_path } });
        ctx.with_store(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
            store.put(&task).unwrap();
        });

        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };

        let stored = ctx.with_store(|mem| {
            let store = TaskStore::new(mem, HOST_OWNER, ctx.agent_seq.as_ref());
            store.get(1, &task.id).unwrap().unwrap()
        });
        match stored.command {
            TaskCommand::Run { cwd, .. } => {
                assert_eq!(
                    cwd,
                    Some(std::path::PathBuf::from(&dir_path)),
                    "stored task.command.cwd should reflect the acquired lease resource"
                );
            }
            other => panic!("expected Run, got {other:?}"),
        }

        match poll_until_terminal(&mut exec, &handle, 40) {
            PollOutcome::Done(r) => {
                let stdout_text = r
                    .output
                    .as_ref()
                    .and_then(|o| o.get("stdout"))
                    .and_then(|s| s.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                assert_eq!(stdout_text.trim(), resolved_path);
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    fn outputs_of(pairs: &[(&str, serde_json::Value)]) -> HashMap<TaskId, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn custom_params(params: serde_json::Value) -> TaskCommand {
        TaskCommand::Custom {
            ipc_method: "claude.tell".to_string(),
            params,
            poll: None,
        }
    }

    #[test]
    fn task_output_substitution_preserves_number_type() {
        let outputs = outputs_of(&[("t-a", json!({ "child_surface_id": 731 }))]);
        let mut cmd = custom_params(json!({
            "surface_id": "${task.t-a.output/child_surface_id}",
            "message": "hi",
        }));
        assert!(substitute_task_outputs(&mut cmd, &outputs).unwrap());

        let TaskCommand::Custom { params, .. } = &cmd else {
            panic!("expected Custom");
        };
        assert_eq!(params["surface_id"], json!(731));
        assert_ne!(
            params["surface_id"],
            json!("731"),
            "숫자 결과가 문자열로 바뀌었다"
        );
        assert!(
            params["surface_id"].as_u64().is_some(),
            "치환한 surface_id는 u64로 읽을 수 있어야 한다"
        );
        assert_eq!(
            params["message"],
            json!("hi"),
            "참조가 없는 값은 바뀌면 안 된다"
        );
    }

    #[test]
    fn task_output_substitution_preserves_composite_types() {
        let outputs = outputs_of(&[(
            "t-a",
            json!({ "obj": {"k": 1}, "arr": [1, 2], "flag": true }),
        )]);
        let mut cmd = custom_params(json!({
            "o": "${task.t-a.output/obj}",
            "a": "${task.t-a.output/arr}",
            "b": "${task.t-a.output/flag}",
            "whole": "${task.t-a.output}",
        }));
        substitute_task_outputs(&mut cmd, &outputs).unwrap();

        let TaskCommand::Custom { params, .. } = &cmd else {
            panic!("expected Custom");
        };
        assert_eq!(params["o"], json!({"k": 1}));
        assert_eq!(params["a"], json!([1, 2]));
        assert_eq!(params["b"], json!(true));
        assert_eq!(
            params["whole"],
            json!({ "obj": {"k": 1}, "arr": [1, 2], "flag": true })
        );
    }

    #[test]
    fn task_output_substitution_interpolates_within_string() {
        let outputs = outputs_of(&[("t-a", json!({ "pr_number": 42, "who": "zilhak" }))]);
        let mut cmd = custom_params(json!({
            "prompt": "review PR ${task.t-a.output/pr_number}",
            "two": "${task.t-a.output/who} opened #${task.t-a.output/pr_number}",
        }));
        substitute_task_outputs(&mut cmd, &outputs).unwrap();

        let TaskCommand::Custom { params, .. } = &cmd else {
            panic!("expected Custom");
        };
        assert_eq!(params["prompt"], json!("review PR 42"));
        assert_eq!(params["two"], json!("zilhak opened #42"));
    }

    #[test]
    fn task_output_substitution_reaches_nested_json_and_run_args() {
        let outputs = outputs_of(&[("t-a", json!({ "id": 7, "dir": "build" }))]);
        let mut custom = custom_params(json!({ "nested": [{ "deep": "${task.t-a.output/id}" }] }));
        substitute_task_outputs(&mut custom, &outputs).unwrap();
        let TaskCommand::Custom { params, .. } = &custom else {
            panic!("expected Custom");
        };
        assert_eq!(params["nested"][0]["deep"], json!(7));

        let mut run = TaskCommand::Run {
            command: vec!["echo".into(), "id=${task.t-a.output/id}".into()],
            workspace_id: 1,
            cwd: Some("/tmp/${task.t-a.output/dir}".into()),
        };
        substitute_task_outputs(&mut run, &outputs).unwrap();
        let TaskCommand::Run { command, cwd, .. } = &run else {
            panic!("expected Run");
        };
        assert_eq!(command[1], "id=7");
        assert_eq!(cwd.as_deref(), Some(std::path::Path::new("/tmp/build")));
    }

    #[test]
    fn task_output_substitution_rejects_pointer_miss() {
        let outputs = outputs_of(&[("t-a", json!({ "child_surface_id": 731 }))]);
        let mut cmd = custom_params(json!({ "x": "${task.t-a.output/nope}" }));
        let err = substitute_task_outputs(&mut cmd, &outputs).unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    /// 결과에 포함된 다른 표식을 재치환하지 않는다.
    #[test]
    fn task_output_substitution_does_not_rescan_injected_values() {
        let outputs = outputs_of(&[("t-a", json!({ "text": "${task.t-b.output/x}" }))]);
        let mut cmd = custom_params(json!({ "p": "${task.t-a.output/text}" }));
        substitute_task_outputs(&mut cmd, &outputs).unwrap();
        let TaskCommand::Custom { params, .. } = &cmd else {
            panic!("expected Custom");
        };
        assert_eq!(params["p"], json!("${task.t-b.output/x}"));
    }

    #[test]
    fn task_output_substitution_missing_result_is_permanent_fail() {
        use tasty_agent::TaskStore;
        let (_td, ctx) = fresh_ctx();
        let seq = ctx.agent_seq.clone();
        let mut upstream = mk_run_task("t-up", vec!["true"]);
        upstream.result = None;
        ctx.with_store(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.put(&upstream).unwrap();
        });

        let mut exec = HostExecutor::new(ctx);
        let mut task = mk_run_task("t-down", vec!["true"]);
        task.command =
            custom_params(json!({ "surface_id": "${task.t-up.output/child_surface_id}" }));
        task.depends_on = vec!["t-up".to_string()];

        match exec.dispatch(&task) {
            DispatchOutcome::PermanentFail(e) => {
                assert!(e.contains("task output substitution"), "{e}");
                assert!(e.contains("no result output"), "{e}");
            }
            other => panic!("expected PermanentFail, got {other:?}"),
        }
    }

    #[test]
    fn task_output_substitution_unknown_task_is_permanent_fail() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let mut task = mk_run_task("t-down", vec!["true"]);
        task.command = custom_params(json!({ "x": "${task.t-ghost.output/y}" }));

        match exec.dispatch(&task) {
            DispatchOutcome::PermanentFail(e) => assert!(e.contains("task not found"), "{e}"),
            other => panic!("expected PermanentFail, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn dispatch_persists_task_output_substituted_command() {
        use tasty_agent::TaskStore;
        let (_td, ctx) = fresh_ctx();
        let seq = ctx.agent_seq.clone();
        let mut upstream = mk_run_task("t-up", vec!["true"]);
        upstream.result = Some(TaskResult {
            exit_code: Some(0),
            output: Some(json!({ "child_surface_id": 731 })),
            error: None,
        });
        let mut downstream = mk_run_task(
            "t-down",
            vec!["echo", "${task.t-up.output/child_surface_id}"],
        );
        downstream.depends_on = vec!["t-up".to_string()];
        ctx.with_store(|mem| {
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.put(&upstream).unwrap();
            store.put(&downstream).unwrap();
        });

        let mut exec = HostExecutor::new(ctx.clone());
        match exec.dispatch(&downstream) {
            DispatchOutcome::Started(_) => {}
            other => panic!("expected Started, got {other:?}"),
        }

        let stored = ctx.with_store(|mem| {
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.get(1, &"t-down".to_string()).unwrap().unwrap()
        });
        match stored.command {
            TaskCommand::Run { command, .. } => assert_eq!(command[1], "731"),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn dispatch_fills_empty_run_cwd_with_acquired_lease_resource() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();
        // pwd는 symlink를 해석한 경로를 출력한다.
        let resolved_path = dir
            .path()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let mut task = mk_run_task("t-cwd-fill", vec!["pwd"]);
        task.metadata = json!({ "lease": { "resource": dir_path } });

        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 40) {
            PollOutcome::Done(r) => {
                let stdout_text = r
                    .output
                    .as_ref()
                    .and_then(|o| o.get("stdout"))
                    .and_then(|s| s.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                assert_eq!(
                    stdout_text.trim(),
                    resolved_path,
                    "Run.cwd should have been filled with the acquired lease resource"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn dispatch_substitutes_lease_placeholder_in_cwd_and_command_args() {
        use tasty_agent::OnFailure;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let task = Task {
            id: "t-placeholder".to_string(),
            workspace_id: 1,
            name: "placeholder".into(),
            command: TaskCommand::Run {
                command: vec!["echo".into(), "${lease.resource}".into()],
                workspace_id: 1,
                cwd: Some(std::path::PathBuf::from("${lease.resource}")),
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: json!({ "lease": { "resource": dir_path } }),
            result: None,
            created_at: 0,
            started_at: None,
            finished_at: None,
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
        };

        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        match poll_until_terminal(&mut exec, &handle, 40) {
            PollOutcome::Done(r) => {
                let stdout_text = r
                    .output
                    .as_ref()
                    .and_then(|o| o.get("stdout"))
                    .and_then(|s| s.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                assert_eq!(
                    stdout_text.trim(),
                    dir_path,
                    "command arg placeholder should also have been substituted"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_substitutes_lease_placeholder_in_custom_params() {
        use std::sync::mpsc;
        use tasty_agent::OnFailure;
        use tasty_ipc::host_call::HostIpcInjector;
        use tasty_ipc::protocol::JsonRpcResponse;
        use tasty_ipc::server::IpcCommand;

        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx.clone());
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let waker = std::sync::Arc::new(|| {});
        ctx.host_ipc
            .set(HostIpcInjector::new(tx, waker))
            .ok()
            .expect("set once");
        let worker = std::thread::spawn(move || {
            let cmd = rx.recv().expect("recv fake.spawn");
            assert_eq!(cmd.request.params.get("cwd"), Some(&json!("wt-1")));
            cmd.response_tx
                .send(JsonRpcResponse::success(
                    cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                    json!({ "ok": true }),
                ))
                .expect("send resp");
        });
        let task = Task {
            id: "t-custom-lease".to_string(),
            workspace_id: 1,
            name: "custom".into(),
            command: TaskCommand::Custom {
                ipc_method: "fake.spawn".into(),
                params: json!({ "cwd": "${lease.resource}" }),
                poll: None,
            },
            state: tasty_agent::TaskState::Ready,
            depends_on: vec![],
            on_failure: OnFailure::Abort,
            metadata: json!({ "lease": { "candidates": ["wt-1"] } }),
            result: None,
            created_at: 0,
            started_at: None,
            finished_at: None,
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
        };
        let handle = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => h,
            other => panic!("expected Started, got {other:?}"),
        };
        worker.join().unwrap();
        assert!(matches!(handle, DispatchHandle::CustomImmediate(_)));
    }

    #[cfg(unix)]
    #[test]
    fn release_permit_returns_pool_resource_for_next_holder() {
        let (_td, ctx) = fresh_ctx();
        let mut exec = HostExecutor::new(ctx);
        let candidates = vec!["wt-1".to_string()];

        let mut task_a = mk_run_task("t-rel-a", vec!["true"]);
        task_a.metadata = json!({ "lease": { "candidates": candidates } });
        assert_eq!(exec.try_acquire_lease(&task_a).unwrap(), Some(true));

        let mut task_b = mk_run_task("t-rel-b", vec!["true"]);
        task_b.metadata = json!({ "lease": { "candidates": candidates } });
        assert_eq!(exec.try_acquire_lease(&task_b).unwrap(), Some(false));

        exec.release_permit(&task_a.id);
        assert!(!exec.held_leases.contains_key(&task_a.id));

        assert_eq!(exec.try_acquire_lease(&task_b).unwrap(), Some(true));
        assert_eq!(
            exec.held_leases.get(&task_b.id).unwrap().1,
            "wt-1".to_string()
        );
    }
}
