//! Task runner — Ready task 를 dispatch 하고 Running task 를 poll 하는 순수 루프.
//!
//! 본 모듈은 *IPC / GUI / OS 와 독립적*. 실제 host-side 동작 (shell process,
//! Custom IPC dispatch, 범용 폴링) 은 [`TaskExecutor`] trait 의 host 측
//! 구현에 위임한다 — 테스트는 mock executor 로 검증.
//!
//! 한 번의 `tick()` 호출이 다음을 수행:
//! 1. Running task 에 대해 `executor.poll(handle)` — Done / Failed 면 완료 보고
//!    ([`Completion`]) 하나로 결과와 종결을 함께 기록한다. 기록이 실패하면 보고를 보관해
//!    다음 tick 에 다시 내고, 그동안 핸들과 permit 을 유지한다.
//! 2. Ready task 에 대해 `executor.dispatch(task)` — handle 보관 + set_state(Running).
//!
//! tick 호출 주기와 핸들 저장·복원은 호스트가 관리한다.
//! 이 루프는 전달받은 running 핸들만 poll하며 없는 핸들을 복원하지 않는다.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::task::attempt::SHRUNK_ERROR_LIMIT;
use crate::task::postprocess::{self, PostprocessCause, PostprocessPhase, PostprocessReport};
use crate::task::{Completion, CompletionOutcome, Task, TaskId, TaskResult, TaskState};
use crate::{AgentError, Result};

/// dispatch / poll 결과를 묶는 핸들. variant 별 의미:
/// - `PolledDispatch`: 범용 비동기 폴링. dispatch 후 `poll_method` 를 terminal 상태
///   도달까지 반복 호출. `poll_params` 는 dispatch 시점에 완성된 폴링 인자.
///   `failure_states` 에 적중하면 `Done` 이 아니라 `Failed` 로 종결한다.
/// - `ShellProcess`: pid 만. host executor 가 `Child` 객체를 별 map 으로 보관.
/// - `ReduceImmediate` / `CustomImmediate`: dispatch 시점에 결과가 즉시 결정.
/// - `ImmediateFail`: dispatch 가 실패. transition 표상 Ready → Failed 직접 전이가
///   불허이므로 *먼저 Running 으로 보낸 후* poll 결과로 Failed 로 흡수하기 위한
///   우회 variant.
/// - `AwaitExternal`: poll 은 관여하지 않는(항상 Active) 외부 push 대기. 종결은
///   host 가 밖에서 store 를 직접 전이시켜 이뤄지고, `tick` 0단계(terminal 흡수)
///   가 다음 tick 에 handle 정리를 담당한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum DispatchHandle {
    /// 범용 폴링 핸들 — dispatch 응답으로 채워진 `poll_params` 로 `poll_method` 를
    /// terminal 상태(`state_field` 가 `terminal_states` 중 하나) 도달까지 반복 호출.
    /// `state_field` 가 `failure_states` 중 하나면 실패로 종결한다(교집합이면 실패 우선).
    PolledDispatch {
        workspace_id: u32,
        poll_method: String,
        poll_params: serde_json::Value,
        state_field: String,
        terminal_states: Vec<String>,
        /// 생략된 기존 영속 레코드는 빈 실패 상태 목록으로 읽는다.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        failure_states: Vec<String>,
        interval_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_ms: Option<u64>,
    },
    ShellProcess {
        pid: u32,
    },
    ReduceImmediate(TaskResult),
    CustomImmediate(TaskResult),
    ImmediateFail(String),
    /// Custom 실행 요청이 시작된 뒤 응답 대기 상한을 넘겼다. 요청을 다시 보내지 않고 executor 가
    /// 보관한 응답 통로에서 늦은 응답을 기다린다. 응답이 오면 executor 가 완료 전략의 handle 로
    /// 이어 간다. 통로는 영속되지 않으므로 재시작 뒤에는 실행 결과 불명(Unknown)으로 끝난다.
    /// agent task 의 새 세션 `<provider>.spawn` 을 기다리는 동안에도 이 handle 이며, 응답 뒤
    /// `AgentTurn` 으로 이어 간다. `<provider>.tell` 이 늦은 응답을 기다리는 동안에는 러너가
    /// `AgentTurn` 을 들고 저장 handle 만 이것으로 바꾼다(재시작 뒤 같은 규칙으로 Unknown).
    AwaitDispatchAnswer {
        workspace_id: u32,
        task_id: TaskId,
        ipc_method: String,
    },
    /// `WaitBarrier` task 의 polling handle — poll 마다 barrier_state 재조회.
    BarrierPoll {
        workspace_id: u32,
        name: String,
    },
    /// 외부 완료 신호를 기다린다. executor의 poll은 Active를 반환하고 호스트가
    /// 결과와 종결 상태를 직접 저장한다. 다음 tick이 핸들과 permit을 정리한다.
    /// wait_key는 호스트가 해석하며 이 크레이트는 의미를 알지 못한다.
    ///
    /// 핸들은 영속되지만 호스트의 hook_wait 매핑은 영속되지 않는다. 재시작 시
    /// 미만료 핸들도 복원해야 이후 외부 상태 전이에서 permit을 정리할 수 있다.
    /// deadline_ms는 Unix epoch 밀리초다. 호스트는 reload에서 만료를 검사하고, 복원한
    /// 미만료 핸들의 기한은 실행 중인 러너의 만료 회수에 다시 건다. 누락된 옛 필드는
    /// 0으로 읽어 만료 처리한다. poll 자체는 기한을 검사하지 않는다.
    AwaitExternal {
        wait_key: String,
        #[serde(default)]
        deadline_ms: u64,
    },
    /// 후처리 `run` 번째 실행을 `not_before_ms` 이후 시작한다. 루프가 직접 다루며
    /// executor 의 poll 을 부르지 않는다. 저장소의 후처리 진행(`Pending`)과 같은 값이다.
    PostprocessPending {
        run: u32,
        not_before_ms: u64,
    },
    /// 실행 중인 후처리 프로세스. host executor 가 프로세스와 출력 수집을 보관한다.
    PostprocessProcess {
        pid: u32,
        run: u32,
    },
    /// 프로세스 없이 결과가 정해진 후처리 실행(시작 실패, 재시작 뒤 결과 불명 등).
    PostprocessResolved(PostprocessReport),
    /// agent 세션의 턴 하나. 호스트의 턴 표(비영속)에 묶인 회차를 poll 마다 확인한다.
    AgentTurn {
        workspace_id: u32,
        task_id: TaskId,
        attempt_id: String,
        provider: String,
        surface_id: u32,
        /// 출력이 같은 회차의 명시 제출을 요구한다(string 이 아닌 출력).
        #[serde(default)]
        needs_submission: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_ms: Option<u64>,
        /// 기존 세션에 아직 보내지 못한 지시. 세션이 다른 회차에 묶였거나 idle 이 아니면
        /// 보내지 않고 다음 poll 에서 다시 본다(같은 세션의 회차를 직렬화한다).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pending_instruction: Option<String>,
        /// 회차마다 새로 만드는 예측할 수 없는 토큰. 지시에 실어 보내고 제출을 받을 때 대조한다.
        /// 다른 회차·다른 호출자의 실수 제출을 막는 값이고 보안 경계가 아니다.
        #[serde(default)]
        attempt_token: String,
    },
}

/// poll 결과.
#[derive(Debug, Clone)]
pub enum PollOutcome {
    Active,
    Done(TaskResult),
    Failed(String),
    /// 실행이 실패로 끝났고 종료 코드·출력이 있다. 결과의 `error` 가 실패 사유다.
    Exited(TaskResult),
    /// 실행은 끝났을 수 있지만 결과를 회수할 수 없다. task 는 Unknown 이 된다.
    Lost(String),
    /// 후처리 실행 하나가 끝났다.
    Postprocessed(PostprocessReport),
}

/// dispatch 의 3-way 결과.
///
/// - `Started(h)` — dispatch 성공, Ready → Running 전이 + handle 보관.
/// - `Deferred` — 이번 tick 의 dispatch 불가 (예: semaphore permit 미점유).
///   state 전이 X, 다음 tick 재시도.
/// - `PermanentFail(e)` — 즉시 실패. 기존 `Err(String)` 흡수 — `ImmediateFail` handle
///   로 wrapping 되어 다음 tick poll 에서 Failed 로 흡수된다.
#[derive(Debug)]
pub enum DispatchOutcome {
    Started(DispatchHandle),
    Deferred,
    PermanentFail(String),
}

/// host 가 구현하는 task 실행기. dispatch 는 *비차단*, poll 은 *비차단 1tick*.
pub trait TaskExecutor {
    /// task 의 command 를 분석해 실제 실행을 시작.
    fn dispatch(&mut self, task: &Task) -> DispatchOutcome;
    /// 핸들의 현재 상태를 1 tick 확인.
    fn poll(&mut self, handle: &DispatchHandle) -> PollOutcome;
    /// task별 조회 상태가 필요한 호스트의 진입점. 기본 구현은 handle 조회에 위임한다.
    fn poll_task(&mut self, _task_id: &TaskId, handle: &DispatchHandle) -> PollOutcome {
        self.poll(handle)
    }
    /// task 가 종결(Succeeded/Failed/Cancelled) 됐을 때 permit 등을 해제.
    /// 기본 구현은 no-op — semaphore 통합이 없는 executor 는 override 불필요.
    fn release_permit(&mut self, _task_id: &TaskId) {}
    /// 저장소에 예약된 후처리 실행을 시작한다([`crate::task::TaskStore::begin_postprocess_run`]
    /// 을 기록한 뒤에만 프로세스를 띄운다). 기본 구현은 후처리를 실행하지 못한다.
    fn start_postprocess(&mut self, _task: &Task, _attempt_id: &str, run: u32) -> DispatchHandle {
        DispatchHandle::PostprocessResolved(PostprocessReport::failed(
            run,
            PostprocessCause::Spawn,
            "this executor does not run postprocess commands",
        ))
    }
    /// 매 tick 처음에 부른다. 종료를 기다리는 자원의 정리 등에 쓴다.
    fn maintain(&mut self) {}
    /// 러너 정지가 요청됐는가. tick 은 Ready 작업을 하나 보내기 전마다 확인하고, 참이면 남은 작업을
    /// 보내지 않는다. dispatch 하나가 호스트 응답을 기다릴 수 있어 tick 끝까지 미루면 정지가 작업 수에
    /// 비례해 늦어진다. 기본 구현은 정지를 모른다.
    fn stop_requested(&self) -> bool {
        false
    }
}

/// 완료 보고 기록의 결과.
#[derive(Debug)]
enum Reported {
    /// 기록했다. 너무 커서 짧은 실패로 바꿔 기록했으면 그 보고다.
    Recorded(Box<Completion>),
    Rejected,
    /// 다시 시도할 오류라 보고를 보관했다.
    Kept,
}

/// `TaskExecutor` 를 들고 task list 를 진행시키는 루프. workspace 1개당 1 instance.
pub struct RunnerLoop<E: TaskExecutor> {
    pub executor: E,
    pub running: HashMap<TaskId, DispatchHandle>,
    /// 기록하지 못한 완료 보고. 다음 tick 에 같은 보고를 다시 낸다.
    pub pending: HashMap<TaskId, Completion>,
    /// 시작했지만 Running 전이를 확정하지 못한 회차. 같은 task 를 다시 시작하지 않고 다음 tick 의
    /// snapshot 으로 이어 간다([`Self::resume_spawned`]).
    pub spawned: HashMap<TaskId, Spawned>,
}

/// 시작했지만 Running 전이를 확정하지 못한 회차.
#[derive(Debug, Clone)]
pub struct Spawned {
    pub handle: DispatchHandle,
    /// 이 회차가 Running 이 되며 가질 회차 id. v1 task 는 없다.
    pub attempt: Option<String>,
}

/// 다시 내도 결과가 달라지지 않는 오류인가. 저장소 오류는 다시 시도한다. 보고 자체가
/// 받아들여지지 않는 오류(다른 회차, 이미 끝남, 없는 task)는 다시 내지 않는다.
pub fn completion_retryable(e: &AgentError) -> bool {
    !matches!(
        e,
        AgentError::CompletionRejected { .. }
            | AgentError::InvalidTransition { .. }
            | AgentError::TaskNotFound(_)
            | AgentError::AlreadyTerminal(_)
    )
}

/// 레코드가 memory 값 상한을 넘어 기록하지 못한 보고를 같은 회차의 짧은 실패로 바꾼다. 같은
/// 보고는 다시 내도 같은 크기라 기록되지 않고 task 가 Running 에 머문다. 출력을 싣지 않은 짧은
/// 보고는 바꾸지 않는다. 바꾼 보고는 출력이 없고 사유가 [`SHRUNK_ERROR_LIMIT`] 안이라 다시
/// 바꾸지 않으므로, 기록을 다시 시도하는 호출자의 재귀는 한 번에 끝난다.
///
/// 후처리 실행 보고는 회차 진행과 맞아야 하므로 같은 실행의 보고로 줄인다
/// ([`Completion::postprocess_too_large_to_store`]). 성공 보고는 먼저 본 작업 결과 사본만 비운
/// 성공으로, 그래도 넘치면 `result_too_large` 실패로 바꾼다. 재귀는 많아야 두 번이다.
pub fn shrink_too_large_completion(e: &AgentError, completion: &Completion) -> Option<Completion> {
    let too_large = matches!(
        e,
        AgentError::Memory(tasty_memory::MemoryError::ValueTooLarge { .. })
    );
    if too_large && completion.postprocess.is_some() {
        return completion.postprocess_too_large_to_store(&e.to_string());
    }
    let carries_bulk = completion.result.output.is_some()
        || completion
            .result
            .error
            .as_ref()
            .is_some_and(|m| m.len() > SHRUNK_ERROR_LIMIT);
    (too_large && carries_bulk && completion.postprocess.is_none())
        .then(|| completion.too_large_to_store(&e.to_string()))
}

impl<E: TaskExecutor> RunnerLoop<E> {
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            running: HashMap::new(),
            pending: HashMap::new(),
            spawned: HashMap::new(),
        }
    }

    /// 완료 보고를 기록한다. 다시 시도할 오류면 보고를 보관한다.
    fn report<FC>(
        &mut self,
        workspace_id: u32,
        task_id: &TaskId,
        completion: Completion,
        now_ms: u64,
        complete: &mut FC,
    ) -> Reported
    where
        FC: FnMut(u32, &TaskId, Completion, u64) -> Result<()>,
    {
        match complete(workspace_id, task_id, completion.clone(), now_ms) {
            Ok(()) => {
                self.pending.remove(task_id);
                Reported::Recorded(Box::new(completion))
            }
            Err(e) if let Some(shrunk) = shrink_too_large_completion(&e, &completion) => {
                tracing::warn!(
                    "runner: {task_id} completion too large to record, settling it as a failure: {e}"
                );
                self.report(workspace_id, task_id, shrunk, now_ms, complete)
            }
            Err(e) => self.unrecorded(task_id, completion, &e),
        }
    }

    /// 기록하지 못한 보고. 다시 시도할 오류면 보관하고, 아니면 버린다.
    fn unrecorded(&mut self, task_id: &TaskId, completion: Completion, e: &AgentError) -> Reported {
        if completion_retryable(e) {
            tracing::warn!("runner: {task_id} completion not recorded, retrying next tick: {e}");
            self.pending.insert(task_id.clone(), completion);
            Reported::Kept
        } else {
            tracing::warn!("runner: {task_id} completion rejected: {e}");
            self.pending.remove(task_id);
            Reported::Rejected
        }
    }

    /// 기록한 보고 뒤 같은 회차에서 이어 갈 후처리 handle. 저장소의 후처리 단계와 같은
    /// 규칙([`postprocess::next_run`])으로 정한다. 없으면 task 가 종결됐다.
    fn continuation(task: &Task, completion: &Completion, now_ms: u64) -> Option<DispatchHandle> {
        let spec = task.contract.as_ref()?.postprocess.as_ref()?;
        match &completion.postprocess {
            None => matches!(completion.outcome, CompletionOutcome::Succeeded).then_some(
                DispatchHandle::PostprocessPending {
                    run: 1,
                    not_before_ms: now_ms,
                },
            ),
            Some(report) => {
                postprocess::next_run(spec, report).map(|run| DispatchHandle::PostprocessPending {
                    run,
                    not_before_ms: now_ms.saturating_add(spec.retry_delay_ms()),
                })
            }
        }
    }

    /// 예약 시각이 된 후처리 실행을 시작한다.
    fn start_postprocess(&mut self, task: &Task, run: u32) {
        let Some(attempt) = task.attempt.as_ref().map(|a| a.id.clone()) else {
            return;
        };
        let handle = self.executor.start_postprocess(task, &attempt, run);
        self.running.insert(task.id.clone(), handle);
    }

    /// 한 tick. 상태 기록은 `set_state`/`complete` 클로저로 호출자가 위임.
    ///
    /// `tasks`: 현재 workspace 의 task snapshot (보통 직전 `task_list` 결과).
    /// `set_state`: `(workspace_id, &task_id, new_state, now_ms)` → `Result<(), AgentError>`.
    /// `complete`: `(workspace_id, &task_id, completion, now_ms)` → `Result<(), AgentError>`.
    /// 결과와 종결을 함께 기록한다([`crate::task::TaskStore::complete`]).
    ///
    /// 에러는 tracing::warn 으로 흡수하고 다음 task 로 진행 (runner thread 가 죽지
    /// 않게 — 사용자가 cancel/retry 로 정리 가능).
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 한 tick 안의 순차 3단계(외부 terminal 흡수 → Running poll → Ready dispatch)가 self.running·self.pending·set_state·complete 클로저를 공유해, 쪼개면 세 함수에 동일 매개변수만 나열되고 흐름 추적이 어려워진다. 중첩은 얕음.
    pub fn tick<FS, FC>(
        &mut self,
        workspace_id: u32,
        now_ms: u64,
        tasks: &[Task],
        mut set_state: FS,
        mut complete: FC,
    ) where
        FS: FnMut(u32, &TaskId, TaskState, u64) -> Result<()>,
        FC: FnMut(u32, &TaskId, Completion, u64) -> Result<()>,
    {
        self.executor.maintain();
        // 외부에서 종결된 작업도 poll 전에 정리해야 permit이 남지 않는다.
        // 결과는 기록됐지만 후속 효과가 끊긴 보고는 같은 보고를 다시 내 마무리한다.
        for task in tasks {
            if !task.state.is_terminal() {
                continue;
            }
            if let Some(c) = self.pending.get(&task.id).cloned()
                && matches!(
                    self.report(workspace_id, &task.id, c, now_ms, &mut complete),
                    Reported::Kept
                )
            {
                continue;
            }
            let held = self.running.remove(&task.id).is_some();
            let spawned = self.spawned.remove(&task.id).is_some();
            if held || spawned {
                self.executor.release_permit(&task.id);
            }
        }
        self.resume_spawned(workspace_id, now_ms, tasks, &mut set_state);

        for task in tasks {
            if !matches!(task.state, TaskState::Running) {
                continue;
            }
            let mut handle = match self.running.get(&task.id) {
                Some(h) => h.clone(),
                None => {
                    // 호스트가 복원하지 않은 핸들은 이 루프에서 처리할 수 없다.
                    continue;
                }
            };
            // 본 작업이 밖에서 완료 보고됐으면(훅·외부 보고) 저장소의 후처리 예약을 따른다.
            if let Some(PostprocessPhase::Pending { run, not_before_ms }) = postprocess_phase(task)
                && !handle.is_postprocess()
                && !self.pending.contains_key(&task.id)
            {
                handle = DispatchHandle::PostprocessPending { run, not_before_ms };
                self.running.insert(task.id.clone(), handle.clone());
            }
            // 기록하지 못한 보고가 있으면 다시 poll 하지 않고 그 보고를 다시 낸다.
            let completion = match self.pending.get(&task.id) {
                Some(c) => c.clone(),
                None => {
                    let attempt = task.attempt.as_ref().map(|a| a.id.clone());
                    let outcome = match &handle {
                        DispatchHandle::PostprocessPending { run, not_before_ms } => {
                            if now_ms >= *not_before_ms {
                                self.start_postprocess(task, *run);
                            }
                            continue;
                        }
                        DispatchHandle::PostprocessResolved(report) => {
                            PollOutcome::Postprocessed(report.clone())
                        }
                        _ => self.executor.poll_task(&task.id, &handle),
                    };
                    match outcome {
                        PollOutcome::Active => continue,
                        PollOutcome::Done(result) => Completion::succeeded(attempt, result),
                        PollOutcome::Failed(err) => Completion::failed(attempt, err),
                        PollOutcome::Exited(result) => Completion::exited(attempt, result),
                        PollOutcome::Lost(reason) => Completion::lost(attempt, reason),
                        PollOutcome::Postprocessed(report) => {
                            Completion::postprocessed(attempt, report)
                        }
                    }
                }
            };
            match self.report(
                workspace_id,
                &task.id,
                completion.clone(),
                now_ms,
                &mut complete,
            ) {
                Reported::Kept => {}
                Reported::Recorded(recorded) => match Self::continuation(task, &recorded, now_ms) {
                    // 같은 회차가 이어진다. permit 은 마지막 종결까지 유지한다.
                    Some(DispatchHandle::PostprocessPending { run, not_before_ms })
                        if now_ms >= not_before_ms =>
                    {
                        self.start_postprocess(task, run);
                    }
                    Some(next) => {
                        self.running.insert(task.id.clone(), next);
                    }
                    None => {
                        self.running.remove(&task.id);
                        self.executor.release_permit(&task.id);
                    }
                },
                Reported::Rejected => {
                    self.running.remove(&task.id);
                    self.executor.release_permit(&task.id);
                }
            }
        }

        for task in tasks {
            if !matches!(task.state, TaskState::Ready) {
                continue;
            }
            if self.running.contains_key(&task.id) || self.spawned.contains_key(&task.id) {
                continue;
            }
            if self.executor.stop_requested() {
                break;
            }
            let handle = match self.executor.dispatch(task) {
                DispatchOutcome::Started(h) => h,
                DispatchOutcome::Deferred => continue,
                DispatchOutcome::PermanentFail(e) => DispatchHandle::ImmediateFail(e),
            };
            // 전이를 확정한 handle 만 지켜본다. 확정하지 못해도 실행은 시작됐으므로 다시 시작하지 않는다.
            match set_state(workspace_id, &task.id, TaskState::Running, now_ms) {
                Ok(()) => {
                    self.running.insert(task.id.clone(), handle);
                }
                Err(e) => {
                    tracing::warn!("set_state(Running) for {} failed: {e}", task.id);
                    let attempt = crate::task::attempt::next_attempt(task, now_ms).map(|a| a.id);
                    self.spawned
                        .insert(task.id.clone(), Spawned { handle, attempt });
                }
            }
        }
    }
}

impl<E: TaskExecutor> RunnerLoop<E> {
    /// Running 을 확정하지 못한 회차를 이번 snapshot 으로 이어 간다. 결과를 모르는 전이가 확정됐으면
    /// (Running) 그 handle 을 지켜보고, 같은 회차로 Ready 면 Running 전이를 다시 시도한다. 회차가 바뀌었거나
    /// 실행 대상이 아니게 됐으면 그 실행을 멈추고 점유를 돌려준다. 종결은 tick 앞부분이 정리한다.
    /// snapshot 에 없는 task 는 목록을 읽지 못했을 수 있어 그대로 둔다.
    fn resume_spawned<FS>(
        &mut self,
        workspace_id: u32,
        now_ms: u64,
        tasks: &[Task],
        set_state: &mut FS,
    ) where
        FS: FnMut(u32, &TaskId, TaskState, u64) -> Result<()>,
    {
        for task in tasks {
            let Some(entry) = self.spawned.get(&task.id) else {
                continue;
            };
            let same_attempt = match task.state {
                TaskState::Running => {
                    task.attempt.as_ref().map(|a| &a.id) == entry.attempt.as_ref()
                }
                TaskState::Ready => {
                    crate::task::attempt::next_attempt(task, now_ms).map(|a| a.id) == entry.attempt
                }
                _ => false,
            };
            if !same_attempt {
                self.spawned.remove(&task.id);
                self.executor.release_permit(&task.id);
                continue;
            }
            if matches!(task.state, TaskState::Ready)
                && let Err(e) = set_state(workspace_id, &task.id, TaskState::Running, now_ms)
            {
                tracing::warn!("set_state(Running) retry for {} failed: {e}", task.id);
                continue;
            }
            if let Some(entry) = self.spawned.remove(&task.id) {
                self.running.insert(task.id.clone(), entry.handle);
            }
        }
    }
}

impl DispatchHandle {
    /// 후처리 단계의 handle 인가.
    pub fn is_postprocess(&self) -> bool {
        matches!(
            self,
            DispatchHandle::PostprocessPending { .. }
                | DispatchHandle::PostprocessProcess { .. }
                | DispatchHandle::PostprocessResolved(_)
        )
    }
}

/// snapshot 의 후처리 단계. 후처리 진행이 없으면 없다.
fn postprocess_phase(task: &Task) -> Option<PostprocessPhase> {
    task.attempt.as_ref()?.postprocess.as_ref().map(|p| p.phase)
}

// ImmediateFail은 호스트 executor의 poll이 Failed로 반환해야 한다.
fn _agent_error_link(_e: AgentError) {}

#[cfg(test)]
mod pending_tests;

#[cfg(test)]
mod postprocess_tests;

#[cfg(test)]
mod spawned_tests;

#[cfg(test)]
mod stop_tests;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::task::{Task, TaskCommand};

    /// Mock executor: dispatch 카운터 + poll 결과 시나리오 큐.
    struct MockExec {
        dispatched: Vec<TaskId>,
        /// task_id -> 미리 정해진 poll outcome 시퀀스.
        polls: HashMap<TaskId, std::collections::VecDeque<PollOutcome>>,
    }

    impl MockExec {
        fn new() -> Self {
            Self {
                dispatched: Vec::new(),
                polls: HashMap::new(),
            }
        }
        fn enq_poll(&mut self, task_id: &str, o: PollOutcome) {
            self.polls
                .entry(task_id.to_string())
                .or_default()
                .push_back(o);
        }
    }

    impl TaskExecutor for MockExec {
        fn dispatch(&mut self, task: &Task) -> DispatchOutcome {
            self.dispatched.push(task.id.clone());
            // 단순 ShellProcess 핸들로 가짜.
            DispatchOutcome::Started(DispatchHandle::ShellProcess { pid: 0 })
        }
        fn poll(&mut self, handle: &DispatchHandle) -> PollOutcome {
            if let DispatchHandle::ImmediateFail(e) = handle {
                return PollOutcome::Failed(e.clone());
            }
            // ShellProcess 만 사용 — task id 와 핸들 매핑은 호출자가 별도 관리해야
            // 하지만 본 테스트는 큐가 1개 task 만 다룬다 가정 → 임의 first.
            for (_id, q) in self.polls.iter_mut() {
                if let Some(o) = q.pop_front() {
                    return o;
                }
            }
            PollOutcome::Active
        }
    }

    fn mk_task(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.to_string(),
            workspace_id: 1,
            name: id.to_string(),
            command: TaskCommand::Run {
                command: vec!["true".into()],
                workspace_id: 1,
                cwd: None as Option<PathBuf>,
            },
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            state: TaskState::Ready,
            created_at: 0,
            started_at: None,
            finished_at: None,
            result: None,
            on_failure: Default::default(),
            metadata: Default::default(),
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
        }
    }

    /// 단순 mock store — Vec<Task> 기반. set_state/set_result 클로저가 본 store 를 mutate.
    struct MockStore {
        tasks: Vec<Task>,
    }
    impl MockStore {
        fn set_state(
            &mut self,
            _ws: u32,
            id: &TaskId,
            new_state: TaskState,
            _now: u64,
        ) -> Result<()> {
            for t in self.tasks.iter_mut() {
                if &t.id == id {
                    t.state = new_state;
                    return Ok(());
                }
            }
            Err(AgentError::TaskNotFound(id.clone()))
        }
        fn complete(&mut self, ws: u32, id: &TaskId, c: Completion, now: u64) -> Result<()> {
            self.set_result(ws, id, c.result)?;
            self.set_state(ws, id, c.outcome.state(), now)
        }
        fn set_result(&mut self, _ws: u32, id: &TaskId, r: TaskResult) -> Result<()> {
            for t in self.tasks.iter_mut() {
                if &t.id == id {
                    t.result = Some(r);
                    return Ok(());
                }
            }
            Err(AgentError::TaskNotFound(id.clone()))
        }
    }

    #[test]
    fn ready_task_dispatches_and_running() {
        let mut runner = RunnerLoop::new(MockExec::new());
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![mk_task("t-1", &[])],
        });

        let snapshot = store.borrow().tasks.clone();
        runner.tick(
            1,
            100,
            &snapshot,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );

        let tasks = &store.borrow().tasks;
        assert_eq!(tasks[0].state, TaskState::Running);
        assert!(runner.running.contains_key("t-1"));
    }

    #[test]
    fn running_task_done_transitions_to_succeeded() {
        let mut exec = MockExec::new();
        exec.enq_poll(
            "t-1",
            PollOutcome::Done(TaskResult {
                exit_code: Some(0),
                output: None,
                error: None,
            }),
        );
        let mut runner = RunnerLoop::new(exec);
        // 직접 running map 에 핸들 박고 task state 도 Running 으로 설정.
        runner
            .running
            .insert("t-1".to_string(), DispatchHandle::ShellProcess { pid: 0 });
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![Task {
                state: TaskState::Running,
                ..mk_task("t-1", &[])
            }],
        });

        let snapshot = store.borrow().tasks.clone();
        runner.tick(
            1,
            200,
            &snapshot,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );

        let tasks = &store.borrow().tasks;
        assert_eq!(tasks[0].state, TaskState::Succeeded);
        assert!(tasks[0].result.is_some());
        assert!(!runner.running.contains_key("t-1"));
    }

    #[test]
    fn immediate_fail_dispatch_is_absorbed_next_tick() {
        struct FailExec;
        impl TaskExecutor for FailExec {
            fn dispatch(&mut self, _t: &Task) -> DispatchOutcome {
                DispatchOutcome::PermanentFail("dispatch error".to_string())
            }
            fn poll(&mut self, h: &DispatchHandle) -> PollOutcome {
                match h {
                    DispatchHandle::ImmediateFail(e) => PollOutcome::Failed(e.clone()),
                    _ => PollOutcome::Active,
                }
            }
        }
        let mut runner = RunnerLoop::new(FailExec);
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![mk_task("t-1", &[])],
        });

        // tick 1: dispatch Err → ImmediateFail handle + Ready→Running.
        let snap1 = store.borrow().tasks.clone();
        runner.tick(
            1,
            100,
            &snap1,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert_eq!(store.borrow().tasks[0].state, TaskState::Running);
        assert!(matches!(
            runner.running.get("t-1"),
            Some(DispatchHandle::ImmediateFail(_))
        ));

        // tick 2: poll → Failed → Running→Failed 전이.
        let snap2 = store.borrow().tasks.clone();
        runner.tick(
            1,
            200,
            &snap2,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(matches!(
            store.borrow().tasks[0].state,
            TaskState::Failed { .. }
        ));
        assert!(store.borrow().tasks[0].result.is_some());
        assert!(!runner.running.contains_key("t-1"));
    }

    #[test]
    fn deferred_dispatch_keeps_task_ready_for_retry() {
        struct ToggleExec {
            calls: u32,
        }
        impl TaskExecutor for ToggleExec {
            fn dispatch(&mut self, _t: &Task) -> DispatchOutcome {
                self.calls += 1;
                if self.calls == 1 {
                    DispatchOutcome::Deferred
                } else {
                    DispatchOutcome::Started(DispatchHandle::ShellProcess { pid: 42 })
                }
            }
            fn poll(&mut self, _h: &DispatchHandle) -> PollOutcome {
                PollOutcome::Active
            }
        }
        let mut runner = RunnerLoop::new(ToggleExec { calls: 0 });
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![mk_task("t-1", &[])],
        });

        // tick 1: Deferred → state 전이 없음, handle 미보관.
        let snap1 = store.borrow().tasks.clone();
        runner.tick(
            1,
            100,
            &snap1,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert_eq!(store.borrow().tasks[0].state, TaskState::Ready);
        assert!(!runner.running.contains_key("t-1"));

        // tick 2: 같은 task 가 다시 dispatch 시도 → Started → Running.
        let snap2 = store.borrow().tasks.clone();
        runner.tick(
            1,
            200,
            &snap2,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert_eq!(store.borrow().tasks[0].state, TaskState::Running);
        assert!(runner.running.contains_key("t-1"));
    }

    #[test]
    fn dispatch_handle_serde_roundtrip_all_variants() {
        let cases = vec![
            DispatchHandle::PolledDispatch {
                workspace_id: 42,
                poll_method: "fake.poll".into(),
                poll_params: serde_json::json!({ "job": "J1" }),
                state_field: "state".into(),
                terminal_states: vec!["done".into()],
                failure_states: vec!["dead".into()],
                interval_ms: 500,
                deadline_ms: Some(1234),
            },
            DispatchHandle::ShellProcess { pid: 12345 },
            DispatchHandle::ReduceImmediate(TaskResult {
                exit_code: Some(0),
                output: Some(serde_json::json!({"merged": "ok"})),
                error: None,
            }),
            DispatchHandle::CustomImmediate(TaskResult {
                exit_code: Some(0),
                output: None,
                error: None,
            }),
            DispatchHandle::ImmediateFail("boom".into()),
            DispatchHandle::BarrierPoll {
                workspace_id: 1,
                name: "b".into(),
            },
            DispatchHandle::AwaitExternal {
                wait_key: "42".into(),
                deadline_ms: 999_999,
            },
        ];
        for h in cases {
            let v = serde_json::to_value(&h).expect("serialize");
            // tag="kind" → object 에 "kind" 필드 존재.
            assert!(v.get("kind").is_some(), "missing 'kind' for {h:?}");
            let back: DispatchHandle = serde_json::from_value(v).expect("deserialize");
            // Debug equality (DispatchHandle 는 PartialEq 안 받음 → format 비교).
            assert_eq!(format!("{h:?}"), format!("{back:?}"));
        }
    }

    /// deadline이 없는 옛 핸들은 0으로 읽어 호스트에서 만료 처리한다.
    #[test]
    fn await_external_old_format_without_deadline_defaults_to_immediately_expired() {
        let old_format = serde_json::json!({
            "kind": "await_external",
            "data": { "wait_key": "hook-1" },
        });
        let handle: DispatchHandle =
            serde_json::from_value(old_format).expect("old format deserialize");
        match handle {
            DispatchHandle::AwaitExternal {
                wait_key,
                deadline_ms,
            } => {
                assert_eq!(wait_key, "hook-1");
                assert_eq!(deadline_ms, 0, "missing field must default to 0 (=expired)");
            }
            other => panic!("expected AwaitExternal, got {other:?}"),
        }
    }

    /// 외부 완료 뒤 다음 tick에서 핸들과 permit이 정리되는지 확인한다.
    #[test]
    fn await_external_stays_active_until_externally_resolved_then_absorbed() {
        struct AwaitExec {
            released: std::cell::RefCell<Vec<TaskId>>,
        }
        impl TaskExecutor for AwaitExec {
            fn dispatch(&mut self, _t: &Task) -> DispatchOutcome {
                DispatchOutcome::Started(DispatchHandle::AwaitExternal {
                    wait_key: "hook-99".into(),
                    deadline_ms: u64::MAX,
                })
            }
            fn poll(&mut self, handle: &DispatchHandle) -> PollOutcome {
                // 계약: AwaitExternal 은 poll 이 절대 종결시키지 않는다.
                assert!(matches!(handle, DispatchHandle::AwaitExternal { .. }));
                PollOutcome::Active
            }
            fn release_permit(&mut self, task_id: &TaskId) {
                self.released.borrow_mut().push(task_id.clone());
            }
        }
        let mut runner = RunnerLoop::new(AwaitExec {
            released: std::cell::RefCell::new(Vec::new()),
        });
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![mk_task("t-1", &[])],
        });

        // tick 1: Ready → Running, AwaitExternal handle 보관.
        let snap1 = store.borrow().tasks.clone();
        runner.tick(
            1,
            100,
            &snap1,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert_eq!(store.borrow().tasks[0].state, TaskState::Running);
        assert!(matches!(
            runner.running.get("t-1"),
            Some(DispatchHandle::AwaitExternal { .. })
        ));

        // tick 2~3: poll 은 항상 Active — Running 유지, handle 잔존.
        for now in [200, 300] {
            let snap = store.borrow().tasks.clone();
            runner.tick(
                1,
                now,
                &snap,
                |ws, id, st, n| store.borrow_mut().set_state(ws, id, st, n),
                |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
            );
            assert_eq!(store.borrow().tasks[0].state, TaskState::Running);
            assert!(runner.running.contains_key("t-1"));
        }
        assert!(runner.executor.released.borrow().is_empty());

        // 외부(hook_id → task_id 매핑 소비 등)가 poll 을 거치지 않고 store 를
        // 직접 Succeeded 로 전이 — `agent.task_set_result` 가 하는 것과 같은 모양의
        // 시나리오를 세운다(그 경로를 부르지는 않는다).
        store
            .borrow_mut()
            .set_result(
                1,
                &"t-1".to_string(),
                TaskResult {
                    exit_code: Some(0),
                    output: None,
                    error: None,
                },
            )
            .expect("set_result");
        store
            .borrow_mut()
            .set_state(1, &"t-1".to_string(), TaskState::Succeeded, 400)
            .expect("set_state");

        // tick 4: 0단계가 이 외부 전이를 흡수 — handle 제거 + release_permit.
        let snap4 = store.borrow().tasks.clone();
        runner.tick(
            1,
            400,
            &snap4,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(!runner.running.contains_key("t-1"));
        assert_eq!(
            runner.executor.released.borrow().clone(),
            vec!["t-1".to_string()]
        );
    }

    #[test]
    fn cancelled_running_task_is_purged_and_permit_released() {
        struct TrackReleases {
            released: std::cell::RefCell<Vec<TaskId>>,
        }
        impl TaskExecutor for TrackReleases {
            fn dispatch(&mut self, _t: &Task) -> DispatchOutcome {
                DispatchOutcome::Started(DispatchHandle::ShellProcess { pid: 1 })
            }
            fn poll(&mut self, _h: &DispatchHandle) -> PollOutcome {
                PollOutcome::Active
            }
            fn release_permit(&mut self, task_id: &TaskId) {
                self.released.borrow_mut().push(task_id.clone());
            }
        }
        let exec = TrackReleases {
            released: std::cell::RefCell::new(Vec::new()),
        };
        let mut runner = RunnerLoop::new(exec);
        runner
            .running
            .insert("t-1".to_string(), DispatchHandle::ShellProcess { pid: 1 });
        // 외부에서 Cancelled 로 직접 전이된 상태 (agent.task_cancel 시나리오).
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![Task {
                state: TaskState::Cancelled,
                ..mk_task("t-1", &[])
            }],
        });
        let snap = store.borrow().tasks.clone();
        runner.tick(
            1,
            300,
            &snap,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(!runner.running.contains_key("t-1"));
        let released = runner.executor.released.borrow().clone();
        assert_eq!(released, vec!["t-1".to_string()]);
    }

    struct TrackReleases {
        released: std::cell::RefCell<Vec<TaskId>>,
    }
    impl TaskExecutor for TrackReleases {
        fn dispatch(&mut self, _t: &Task) -> DispatchOutcome {
            DispatchOutcome::Started(DispatchHandle::ShellProcess { pid: 1 })
        }
        fn poll(&mut self, _h: &DispatchHandle) -> PollOutcome {
            PollOutcome::Active
        }
        fn release_permit(&mut self, task_id: &TaskId) {
            self.released.borrow_mut().push(task_id.clone());
        }
    }

    #[test]
    fn externally_succeeded_running_task_is_purged_and_permit_released() {
        let exec = TrackReleases {
            released: std::cell::RefCell::new(Vec::new()),
        };
        let mut runner = RunnerLoop::new(exec);
        runner
            .running
            .insert("t-1".to_string(), DispatchHandle::ShellProcess { pid: 1 });
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![Task {
                state: TaskState::Succeeded,
                ..mk_task("t-1", &[])
            }],
        });
        let snap = store.borrow().tasks.clone();
        runner.tick(
            1,
            300,
            &snap,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(!runner.running.contains_key("t-1"));
        assert_eq!(
            runner.executor.released.borrow().clone(),
            vec!["t-1".to_string()]
        );
    }

    #[test]
    fn externally_failed_running_task_is_purged_and_permit_released() {
        let exec = TrackReleases {
            released: std::cell::RefCell::new(Vec::new()),
        };
        let mut runner = RunnerLoop::new(exec);
        runner
            .running
            .insert("t-1".to_string(), DispatchHandle::ShellProcess { pid: 1 });
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![Task {
                state: TaskState::Failed {
                    error: "boom".into(),
                },
                ..mk_task("t-1", &[])
            }],
        });
        let snap = store.borrow().tasks.clone();
        runner.tick(
            1,
            300,
            &snap,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(!runner.running.contains_key("t-1"));
        assert_eq!(
            runner.executor.released.borrow().clone(),
            vec!["t-1".to_string()]
        );
    }

    #[test]
    fn terminal_task_without_handle_does_not_call_release_permit() {
        let exec = TrackReleases {
            released: std::cell::RefCell::new(Vec::new()),
        };
        let mut runner = RunnerLoop::new(exec);
        // self.running 에 아무것도 없음 — 예: 재시작 후 Skipped 로 이미 종결된 task.
        let store = std::cell::RefCell::new(MockStore {
            tasks: vec![Task {
                state: TaskState::Skipped,
                ..mk_task("t-1", &[])
            }],
        });
        let snap = store.borrow().tasks.clone();
        runner.tick(
            1,
            300,
            &snap,
            |ws, id, st, now| store.borrow_mut().set_state(ws, id, st, now),
            |ws, id, c, now| store.borrow_mut().complete(ws, id, c, now),
        );
        assert!(runner.executor.released.borrow().is_empty());
    }
}
