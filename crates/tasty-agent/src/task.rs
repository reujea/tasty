//! Task primitive — DAG, 상태 머신, memory 영속.
//!
//! 의도적으로 IPC/GUI와 독립적이다. 호스트는 본 모듈의 API를 호출해 task 모델을
//! 영속하고, 별도 스케줄러가 `Ready` 상태 task를 실제로 실행한다 (`Run`은 Surface
//! 없는 bare subprocess spawn + stdout/stderr 캡처, `Custom`은 임의 IPC dispatch —
//! 옵션 폴링 포함, `Reduce`는 본 크레이트 내부 reducer로). 실행 완료 신호가 들어오면
//! 호스트가 `TaskStore::set_state`로 진행시키고 downstream의 readiness가 갱신된다.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tasty_utils::id::WorkspaceId;

/// Task의 고유 식별자. 형식 `t-<timestamp_ms>-<seq>` (예: `t-1716800000123-7`).
/// 호스트가 본 크레이트 외부에서 임의 문자열로 생성해도 무방하지만, 본 모듈의
/// 헬퍼는 위 형식으로 발급한다.
pub type TaskId = String;

/// Task의 state 머신.
///
/// 변환 규칙:
/// - `Waiting → Ready` (의존성 모두 Succeeded)
/// - `Waiting → Skipped` (의존성 중 하나 Failed, downstream skip 모드)
/// - `Waiting → Cancelled` (사용자/abort)
/// - `Ready → Running` (스케줄러가 실행 시작)
/// - `Ready → Cancelled`
/// - `Running → Succeeded`
/// - `Running → Failed`
/// - `Running → Cancelled`
/// - `Unknown → Ready` (사용자 명시 retry)
/// - `Unknown → Cancelled`
///
/// 재시작 후 `Running` 상태였던 task는 호스트가 `Unknown`으로 표시한다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskState {
    Waiting,
    Ready,
    Running,
    Succeeded,
    Failed { error: String },
    Cancelled,
    Skipped,
    Unknown,
}

impl TaskState {
    pub fn name(&self) -> &'static str {
        match self {
            TaskState::Waiting => "waiting",
            TaskState::Ready => "ready",
            TaskState::Running => "running",
            TaskState::Succeeded => "succeeded",
            TaskState::Failed { .. } => "failed",
            TaskState::Cancelled => "cancelled",
            TaskState::Skipped => "skipped",
            TaskState::Unknown => "unknown",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Succeeded
                | TaskState::Failed { .. }
                | TaskState::Cancelled
                | TaskState::Skipped
        )
    }
}

/// Task가 실패했을 때 downstream 처리 정책.
///
/// **어느 쪽 task에 설정하느냐에 따라 의미가 반대로 갈린다** — 같은 필드가
/// 방향이 다른 두 관계를 표현하기 때문이다:
/// - `Abort`/`ContinueDownstream`은 **의존하는 쪽(downstream)** 자신에 설정해야
///   한다. `apply_on_failure`가 평가하는 건 실패한 upstream이 아니라, 그
///   upstream에 의존해 지금 `Skipped`로 전이하려는 downstream 자신의
///   `on_failure`다(`task/store.rs`의 `cascade_downstream`이
///   `apply_on_failure(&d_task, ...)`로 downstream을 넘긴다). 실패할 upstream
///   쪽에 설정하면 아무 효과가 없다.
/// - `Fallback`은 **실패할 수 있는 쪽(upstream)** 자신에 설정해야 한다.
///   `set_state`가 그 task 자신이 `Failed`로 전이하는 순간 자기 `on_failure`를
///   보고 fallback을 승격시킨다. downstream 쪽에 설정하면, downstream이
///   의존성 실패로 `Skipped`행 판정을 받아도 `apply_on_failure`는 `None`을
///   반환할 뿐이라(아래) 아무 전이도 일어나지 않고 `Waiting`에 영구히 멈춘다 —
///   경고 로그조차 남지 않는다.
///
/// 정리: Abort/ContinueDownstream → downstream에 설정. Fallback → upstream에
/// 설정.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum OnFailure {
    /// downstream 모두 `Skipped`로.
    #[default]
    Abort,
    /// downstream을 정상 진행 (의존성 실패를 성공처럼 취급).
    ContinueDownstream,
    /// main 이 `Failed` 로 전이해야만 도는 대체 경로. 그 fallback 이 Succeed 하면
    /// downstream 이 정상 진행.
    ///
    /// 두 형식 지원 (둘 중 정확히 하나):
    /// - `task`: 사전에 존재하는 task id (기존 동작). main 이 `Failed` 가 되기
    ///   전까지는 **dormant** 하다 — `depends_on` 이 비어 있어도 곧장 `Ready`
    ///   로 뜨지 않는다(`TaskGraph::evaluate_readiness` 의 dormant 판정,
    ///   `TaskStore::create` 가 main 생성 시점에 이미 `Ready` 였던 대상을
    ///   `Waiting` 으로 소급 정정). main 이 `Succeeded`/`Cancelled`/`Skipped`
    ///   로 끝나 다시는 `Failed` 가 될 일이 없어지면(`set_state`), 그 fallback
    ///   은 `Waiting` 에 영구 잔류하지 않고 `Skipped` 로 마감된다.
    /// - `inline`: 동적 spec — main 이 Failed 가 되는 set_state 분기에서
    ///   `TaskStore::create` 가 새 fallback task 를 생성. `metadata.fallback_of`
    ///   에 main.id 가 자동 주입되어 idempotency 보장 + downstream 재평가 추적.
    ///   task 자체가 실패 시점에야 생기므로 dormant 상태를 거치지 않는다.
    Fallback {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<TaskId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inline: Option<Box<InlineFallbackSpec>>,
    },
}

/// `OnFailure::Fallback { inline: Some(...) }` 의 동적 생성 spec.
///
/// `depends_on_override` 가 `None` 이면 main task 의 `depends_on` 을 그대로 복사한다.
/// 사용자가 inline 의 `on_failure` 를 또 `Fallback{inline=...}` 으로 nested 하는
/// 경우는 runtime 검증 안 함 — 무한 재귀 방지는 사용자 책임 (max depth 1 권고).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineFallbackSpec {
    pub name: String,
    pub command: TaskCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on_override: Option<Vec<TaskId>>,
    #[serde(default)]
    pub on_failure: OnFailure,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Task가 실행할 동작.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskCommand {
    /// Surface(Tab) 없는 bare subprocess 로 일반 명령 실행. stdout/stderr 를 각각
    /// 마지막 64KiB(tail)까지 캡처해 `TaskResult.output`(성공 시) 또는 에러 메시지
    /// (실패 시)에 싣는다 — tty 가 필요한 명령은 지원하지 않는다(그건 `pty.*`의 몫).
    Run {
        command: Vec<String>,
        workspace_id: WorkspaceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,
    },
    /// 임의 IPC 메서드 호출 위임 (호출자가 해당 메서드의 권한을 보유해야 함).
    ///
    /// `poll` 이 `None` 이면 dispatch 응답으로 즉시 종결한다(`CustomImmediate`).
    /// `Some(..)` 이면 dispatch 후 terminal 상태 도달까지 `poll` 사양대로 폴링한다
    /// — 코어가 모르는 임의 비동기 작업(예: 자식 에이전트 spawn 후 idle 까지 대기)을
    /// 어떤 플러그인이든 표현할 수 있는 범용 메커니즘.
    Custom {
        ipc_method: String,
        #[serde(default)]
        params: serde_json::Value,
        /// 인라인 `PollSpec` 또는 등록된 완료 판정 전략 이름 — PollSpec 이 이름
        /// 참조를 받도록 확장한 형태다. `None` 이면 결정 6(기본 전략)이
        /// 적용될 수 있다 — host 가 `ipc_method` 에 매칭되는 `default_for_methods`
        /// 전략을 찾아 대신 사용한다. 매칭되는 기본 전략도 없으면 기존 동작
        /// (dispatch 응답 즉시 `Succeeded`, `CustomImmediate`) 을 유지한다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        poll: Option<PollSpecRef>,
    },
    /// 다른 task의 결과를 합성.
    Reduce {
        inputs: Vec<TaskId>,
        strategy: ReducerStrategy,
    },
    /// 명시적 barrier 대기. barrier 가 Closed 되면 Succeeded, TimedOut 이면 Failed.
    /// timeout 은 barrier 자체의 `timeout_ms` 가 단일 출처.
    WaitBarrier { name: String },
}

/// 범용 폴링 사양 — `TaskCommand::Custom { poll: Some(..) }` 에서 사용.
///
/// dispatch IPC 호출 후, 응답의 `state_field` 가 `terminal_states` 중 하나에 도달할
/// 때까지 `poll_method` 를 반복 호출한다. 필드 의미는 plugin manifest 의
/// `CompletionStrategyDecl`/`AutoWaitDecl`/`PollingDecl`(CLI auto_wait 경로)와
/// 동형으로 맞춘다 — 폴링 semantics 를 CLI/agent 양쪽에서 통일하기 위함. 두 곳의
/// 필드 대응은 (본 크레이트가 `tasty-plugin-manifest` 를 의존할 수 없어) 주석만으로
/// 지켜지는 불변식이 아니라, host 측 변환 함수
/// `completion_strategy_to_poll_spec()`(`src/core/agent/completion_strategy.rs`)
/// 와 그 옆의 단위테스트가 필드 대응을 컴파일/테스트 타임에 강제한다 — 필드가
/// 갈라지면 그 변환 함수/테스트가 깨진다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollSpec {
    /// 폴링 시 호출할 IPC method (예: `"<plugin>.wait"`).
    pub poll_method: String,
    /// dispatch 응답 키 → poll params 키 매핑 (예: `{"child_index":"child_index"}`).
    /// 응답이 원 요청보다 우선해 poll params 를 채운다.
    #[serde(default)]
    pub map_from_response: HashMap<String, String>,
    /// 원 dispatch params 키 → poll params 키 매핑 (예: `{"surface_id":"surface_id"}`).
    #[serde(default)]
    pub map_from_request: HashMap<String, String>,
    /// 응답에서 상태를 읽을 필드명 (예: `"state"`).
    pub state_field: String,
    /// terminal 로 간주할 상태값 목록 (예: `["idle","needs_input"]`).
    /// 비-terminal 상태는 모두 계속 폴링(Active)으로 취급한다.
    pub terminal_states: Vec<String>,
    /// **실패**로 간주할 상태값 목록 (예: `["exited"]`). 적중하면 task 가
    /// `Succeeded` 가 아니라 `Failed` 로 종결되어 `OnFailure`(abort /
    /// continue_downstream / fallback)가 비로소 동작한다 — 이 목록이 비어 있으면
    /// 폴링 완료 판정은 terminal 도달을 전부 성공으로 읽는다.
    ///
    /// `terminal_states` 와 같은 값이 양쪽에 들어가면 **이쪽이 이긴다**(판정은
    /// `src/core/agent/runner_host.rs`) — 실패를 성공으로 읽는 쪽이 그 반대보다
    /// 위험하기 때문이다.
    ///
    /// 생략 가능하다. 이 필드 도입 이전에 영속된 DAG JSON(`TaskCommand::Custom.poll`
    /// 의 인라인 `PollSpec`)이 그대로 역직렬화돼야 하므로 필수 필드로 만들지
    /// 않는다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failure_states: Vec<String>,
    /// 폴링 간격 (ms). 핸들에 보존된다. 기본 500ms — 생략해도 역직렬화가 실패하지
    /// 않는다(이전에는 필수 필드였다).
    #[serde(default = "default_poll_interval_ms")]
    pub interval_ms: u64,
    /// 전체 폴링 timeout (ms). `None` 이면 무한 대기.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

fn default_poll_interval_ms() -> u64 {
    500
}

/// `TaskCommand::Custom.poll` 이 받아들이는 두 형태 — 인라인 사양 또는 이름 참조
/// (PollSpec 이 이름 참조를 받도록 확장, 인라인 형태 하위호환 유지).
///
/// `#[serde(untagged)]`: `{"strategy": "<이름>"}` 형태(다른 필드 없음)는 `Named` 로,
/// 그 외(기존 인라인 `PollSpec` 의 필수 필드들 — `poll_method`/`state_field`/
/// `terminal_states` 등)는 `Inline` 으로 해석된다. `PollSpec` 에는 `strategy` 라는
/// 필드가 없으므로 두 형태는 구조적으로 겹치지 않는다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PollSpecRef {
    /// 등록된 완료 판정 전략 이름(레지스트리 id, `<owner>/<short>`)을 참조.
    /// 해석은 host(`src/core/agent/runner_host.rs`)가 완료 판정 전략 레지스트리로
    /// 수행한다 — 이 크레이트는 레지스트리를 모른다(코어가 특정 에이전트를 모르는
    /// 것과 같은 층위 분리, `docs/dev-guide/agent-runner.md`).
    Named { strategy: String },
    /// 기존 인라인 형태. 하위호환 — 기존 DAG JSON 은 그대로 동작한다.
    Inline(PollSpec),
}

/// Reducer 합성 전략.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReducerStrategy {
    FirstSuccess,
    All,
    MergeJson,
    ConcatText,
    /// shell 명령으로 결과 배열 stdin 전달, stdout이 최종.
    Custom {
        command: String,
    },
}

/// Task 실행 결과. `Succeeded`/`Failed` 상태에서만 채워진다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    /// 종료 코드 (Run의 exit_code 등).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// 실행 산출물 (Custom의 IPC 응답, 폴링 종료 시 마지막 응답 등). `Run`(성공 시)은
    /// `{"pid", "stdout": {"text","truncated","dropped_bytes"}, "stderr": {..}}` 형태 —
    /// 각 스트림은 마지막 64KiB(tail)만 보관하고, 상한을 넘긴 만큼만 `truncated`/
    /// `dropped_bytes` 로 알린다. `Run` 실패 시엔 이 필드가 아니라 `error` 에 캡처
    /// 결과가 실린다(`TaskExecutor::poll` 계약상 `Failed` 는 문자열 하나만 나른다).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// 에러 사유 (Failed 상태에서만).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 영속되는 Task 한 레코드.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub command: TaskCommand,
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    pub state: TaskState,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResult>,
    #[serde(default)]
    pub on_failure: OnFailure,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// `TaskStore::create_reserved_for_fallback`로 생성된 task 가 아직 어떤
    /// main 의 `OnFailure::Fallback{task}` 로도 연결되지 않은 동안 `true`.
    /// `TaskGraph::dormant_as_pending_fallback` 이 이 값을 최우선으로 존중해
    /// readiness 평가 자체를 보류시킨다 — `Ready` 로 단 한 번도 노출되지
    /// 않으므로 러너가 dispatch 할 수 없다. main 이 이 task 를 `Fallback{task}`
    /// 로 참조하는 `create()` 호출의 소급 정정 블록이 그 순간 `false` 로
    /// 해제하고, 그 뒤로는 (참조하는 main 이 실제로 존재/미종결) 이라는 정상
    /// dormant 판정이 이어받는다. `#[serde(default)]` 는 이 필드 도입 이전에
    /// 영속된 task 를 `false`(비예약, 기존 동작 그대로)로 채운다.
    #[serde(default)]
    pub reserved_for_fallback: bool,
}

/// 워크스페이스에 속한 task들의 그래프 뷰. 사이클 검출, downstream 계산용.
pub(super) const TASK_KEY_PREFIX: &str = "tasty.agent.task.";

pub(super) fn task_key(id: &TaskId) -> crate::Result<String> {
    crate::component_key(TASK_KEY_PREFIX, "task id", id)
}

/// `MemoryStore` 위에 얹은 Task 영속 + state 머신.
///
/// 본 store는 빌려 쓰는 형태. 호스트가 `MemoryStore`의 lock을 잡은 상태에서
/// 임시로 wrap해 호출한다.
pub(super) fn is_valid_transition(from: &TaskState, to: &TaskState) -> bool {
    use TaskState::*;
    match (from, to) {
        (Waiting, Ready)
        | (Waiting, Skipped)
        | (Waiting, Cancelled)
        | (Ready, Running)
        | (Ready, Cancelled)
        | (Running, Succeeded)
        | (Running, Failed { .. })
        | (Running, Cancelled)
        | (Unknown, Ready)
        | (Unknown, Cancelled)
        | (Unknown, Waiting) => true,
        // retry 경로는 별도 메서드에서 처리. 직접 set_state로는 거부.
        _ => false,
    }
}

/// on_failure 정책에 따라 downstream task의 목표 상태를 결정.
/// 호출 시점 `task`는 dep 중 하나가 실패해 `evaluate_readiness`가 `Skipped`를
/// 반환한 상태로 가정한다.
pub(super) fn apply_on_failure(task: &Task, _all: &[Task]) -> Option<TaskState> {
    match &task.on_failure {
        OnFailure::Abort => Some(TaskState::Skipped),
        OnFailure::ContinueDownstream => Some(TaskState::Ready),
        // Fallback은 호스트가 fallback task를 별도 트리거해야 함. 본 task는 일단
        // Waiting을 유지하고, fallback이 Succeed하면 그때 호스트가 다시 평가.
        OnFailure::Fallback { .. } => None,
    }
}

pub mod dag;
mod graph;
mod store;

pub use dag::{DagStateCounts, DagSummary, group_tasks_into_dags};
pub use graph::*;
pub use store::*;

#[cfg(test)]
#[path = "task/tests.rs"]
mod tests;
