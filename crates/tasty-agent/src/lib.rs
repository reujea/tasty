//! Tasty Agent — 다중 에이전트 협업 primitive.
//!
//! 1차 시민으로 다음을 제공:
//! - **Task DAG**: 의존성을 가진 task 그래프. state 머신과 사이클 검출.
//! - (후속) Barrier / Semaphore / Lease / Reducer / Rate Limit.
//!
//! 영속은 `tasty-memory` 위에 얹는다 (scope = workspace, key prefix =
//! `tasty.agent.task.<id>`). 본 크레이트는 GUI/IPC와 독립적이며 상태 머신 + 영속
//! 헬퍼 + `reduce_with_custom` 의 기본 runner(`run_custom_shell` — 이 크레이트가
//! 프로세스를 띄우는 유일한 자리이고, 호출자가 자기 함수로 갈아끼울 수 있는
//! 기본값이다)를 담당한다. IPC dispatcher와 task 실행 엔진(run, 옵션 폴링)은
//! 호스트가 본 크레이트의 API를 호출해 조율한다.
#![allow(clippy::result_large_err)]

pub mod barrier;
pub mod lease;
pub mod platform;
pub mod rate_limit;
pub mod reducer;
pub mod runner;
pub mod semaphore;
pub mod task;

use thiserror::Error;

pub use barrier::{Barrier, BarrierState, BarrierStore};
pub use lease::{AcquireAnyOutcome, ElasticSpec, Lease, LeaseMode, LeaseStore};
pub use rate_limit::{ConsumeOutcome, RateLimit, RateLimitStore};
pub use reducer::{
    ReducerInput, extract_paths, reduce_in_process, reduce_with_custom, run_custom_shell,
};
pub use runner::{DispatchHandle, DispatchOutcome, PollOutcome, RunnerLoop, TaskExecutor};
pub use semaphore::{AcquireOutcome, ReleaseOutcome, Semaphore, SemaphoreHolder, SemaphoreStore};
pub use task::{
    DagStateCounts, DagSummary, InlineFallbackSpec, OnFailure, PollSpec, PollSpecRef,
    ReducerStrategy, Task, TaskCommand, TaskGraph, TaskId, TaskResult, TaskState, TaskStore,
    group_tasks_into_dags,
};

/// 본 크레이트의 공용 에러.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("dependency cycle through tasks: {0:?}")]
    DependencyCycle(Vec<TaskId>),
    #[error("unknown dependency task: {0}")]
    UnknownDependency(TaskId),
    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("task already in terminal state: {0}")]
    AlreadyTerminal(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("lease conflict: '{resource}' held by '{holder}'")]
    LeaseConflict { resource: String, holder: String },
    /// pool 모드(`LeaseStore::acquire_any`)에서 fixed candidates 가 전부
    /// 점유 중이거나(elastic 이면) `max_candidates` 상한까지 합성해도 빈
    /// 자리가 없을 때 `LeaseMode::Fail` 에서 반환.
    #[error("lease pool exhausted: none of {candidates:?} available for '{holder}'")]
    LeasePoolExhausted {
        candidates: Vec<String>,
        holder: String,
    },
    /// task 삭제 시 다른 task 가 여전히 참조 중(`depends_on`/`Fallback.task`/
    /// `Reduce.inputs`) — `--cascade`/`--force` 없이는 거부. `referenced_by` 를
    /// 응답 `error.data` 에 실어 호출자가 다음 행동을 정할 수 있게 한다.
    #[error("task {task} is referenced by: {referenced_by:?}")]
    TaskReferenced {
        task: TaskId,
        referenced_by: Vec<TaskId>,
    },
    /// task 삭제 금지 상태는 `Running` 하나뿐 — `cancel` 로 먼저 정리해야 한다.
    /// `--force` 도 이 제약은 뚫지 못한다.
    #[error("cannot delete task {0} while it is Running — cancel it first")]
    TaskRunning(TaskId),
    #[error("memory: {0}")]
    Memory(#[from] tasty_memory::MemoryError),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;

/// 호출자가 준 값(`value`)을 `prefix` 뒤에 붙여 memory 키를 만든다.
///
/// 키 규칙 위반을 memory 층에 맡기면 `MemoryError::InvalidKey` 가 되어 IPC 에서
/// internal(`-32603`)로 나가고, 메시지의 좌표도 접두사를 붙인 **내부 키** 기준이라
/// 호출자는 자기 값의 어디가 틀렸는지 못 읽는다. 그래서 여기서 먼저 재고
/// 입력 오류(`InvalidArgument`)로, 호출자가 준 값 기준 좌표로 돌려준다.
/// 판정과 허용 문자 문구는 `tasty_memory` 의 것을 그대로 쓴다 — 집합을 여기
/// 다시 적지 않는다. 빈 값은 종전대로 통과한다(키가 접두사만으로 유효하다).
pub(crate) fn component_key(prefix: &str, label: &str, value: &str) -> Result<String> {
    let key = format!("{prefix}{value}");
    if let Err(full) = tasty_memory::validate_key(&key) {
        let budget = tasty_memory::MAX_KEY_LEN.saturating_sub(prefix.len());
        // 접두사는 허용 문자만 쓰므로 실패 원인은 값 쪽이다. 문자 위반이면 값을 문자
        // 단위로 다시 재서 값 기준 좌표와 **그 문자 자체**를 싣는다 — `validate_key` 는
        // 바이트를 세고 첫 바이트를 문자로 찍어서 `é` 가 `'Ã'` 로 나온다. 한 문자가
        // 허용되는지는 `validate_key` 에 그 문자만 넘겨 묻는다(집합을 여기 다시 적지 않는다).
        // 문자가 다 허용되면 길이 초과다.
        let mut buf = [0u8; 4];
        let bad = value
            .chars()
            .enumerate()
            .find(|(_, c)| tasty_memory::validate_key(c.encode_utf8(&mut buf)).is_err());
        let reason = match bad {
            Some((i, c)) => format!("invalid char at {i}: {c:?}"),
            None if !value.is_empty() => format!("too long: {} bytes > {budget}", value.len()),
            None => full,
        };
        return Err(AgentError::InvalidArgument(format!(
            "{label} {value:?}: {reason} (allowed: {}; at most {budget} bytes)",
            tasty_memory::KEY_ALLOWED_CHARS
        )));
    }
    Ok(key)
}

#[cfg(test)]
mod component_key_tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use tasty_memory::MemoryStore;

    fn mem() -> MemoryStore {
        MemoryStore::open_in_memory().unwrap()
    }

    /// 호출자 값 기준 좌표로, 입력 오류로 돌아와야 한다 — 내부 키 좌표(접두사 길이
    /// 만큼 밀린 값)나 `Memory` 오류로 새면 IPC 가 `-32603` 을 낸다.
    fn assert_caller_error(err: AgentError, label: &str) {
        let AgentError::InvalidArgument(msg) = &err else {
            panic!("expected InvalidArgument, got {err:?}");
        };
        assert!(msg.starts_with(label), "{msg}");
        assert!(msg.contains("\"v6S\""), "{msg}");
        assert!(msg.contains("invalid char at 2: 'S'"), "{msg}");
    }

    #[test]
    fn semaphore_name_is_judged_in_caller_coordinates() {
        let mut m = mem();
        let mut store = SemaphoreStore::new(&mut m, "_host");
        assert_caller_error(store.create(1, "v6S", 1, 0).unwrap_err(), "semaphore name");
        assert_caller_error(
            store.acquire(1, "v6S", "h", None, 0).unwrap_err(),
            "semaphore name",
        );
        assert_caller_error(store.delete(1, "v6S").unwrap_err(), "semaphore name");
    }

    #[test]
    fn barrier_rate_limit_and_task_ids_are_judged_the_same_way() {
        let mut m = mem();
        assert_caller_error(
            BarrierStore::new(&mut m, "_host")
                .create(1, "v6S", 1, None, 0)
                .unwrap_err(),
            "barrier name",
        );
        assert_caller_error(
            RateLimitStore::new(&mut m, "_host")
                .remove("v6S")
                .unwrap_err(),
            "rate_limit id",
        );
        let seq = AtomicU64::new(0);
        assert_caller_error(
            TaskStore::new(&mut m, "_host", &seq)
                .get(1, &"v6S".to_string())
                .unwrap_err(),
            "task id",
        );
    }

    /// 다바이트 문자는 입력한 그 문자와 문자 단위 좌표로 나온다 — 바이트 좌표 · 첫 바이트를
    /// 문자로 찍은 값(`'Ã'`)이 새면 호출자가 자기 이름에서 그 글자를 못 찾는다.
    #[test]
    fn a_multibyte_char_is_named_as_typed_and_counted_in_chars() {
        let mut m = mem();
        let mut store = SemaphoreStore::new(&mut m, "_host");
        for (name, want) in [
            ("aé", "invalid char at 1: 'é'"),
            ("ab한글", "invalid char at 2: '한'"),
        ] {
            let err = store.create(1, name, 1, 0).unwrap_err();
            let AgentError::InvalidArgument(msg) = &err else {
                panic!("expected InvalidArgument, got {err:?}");
            };
            assert!(msg.contains(want), "{name}: {msg}");
        }
    }

    #[test]
    fn too_long_name_reports_the_budget_left_after_the_prefix() {
        let mut m = mem();
        let mut store = SemaphoreStore::new(&mut m, "_host");
        let budget = tasty_memory::MAX_KEY_LEN - semaphore::SEMAPHORE_KEY_PREFIX.len();
        store.create(1, "a".repeat(budget), 1, 0).unwrap();
        let err = store.create(1, "a".repeat(budget + 1), 1, 0).unwrap_err();
        let AgentError::InvalidArgument(msg) = &err else {
            panic!("expected InvalidArgument, got {err:?}");
        };
        assert!(
            msg.contains(&format!("too long: {} bytes > {budget}", budget + 1)),
            "{msg}"
        );
    }
}
