use std::collections::BTreeSet;

use crate::adapters::ipc::handler::params::{self, p_try};
use serde_json::{Value, json};

use crate::core::Core;
use crate::core::agent::graph_view::{collect_graph_edges, on_failure_kind, task_command_kind};
use tasty_agent::task::{TaskCreateOpts, TaskDeleteOpts, TaskPurgeFilter};
use tasty_agent::{
    AgentError, DispatchHandle, OnFailure, PollSpecRef, ReducerStrategy, Task, TaskCommand,
    TaskGraph, TaskId, TaskResult, TaskState, extract_paths, reduce_with_custom, run_custom_shell,
};
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::super::memory::mark_durability;
use super::{agent_err_to_response, escape_dot, now_ms, task_id_param, workspace_id_param};

pub fn handle_task_create(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing required 'name'"),
    };
    let command: TaskCommand = match params.get("command") {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(c) => c,
            Err(e) => {
                return JsonRpcResponse::invalid_params(id, format!("invalid 'command': {e}"));
            }
        },
        None => return JsonRpcResponse::invalid_params(id, "Missing required 'command'"),
    };
    let depends_on: Vec<TaskId> = params
        .get("depends_on")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let on_failure: OnFailure = params
        .get("on_failure")
        .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
        .unwrap_or_default();
    let metadata = params.get("metadata").cloned().unwrap_or(Value::Null);
    // TOCTOU 예약: 이 task 를 곧 다른 main 의 `on_failure.fallback.task` 로
    // 참조할 계획이면 `true` 로 넘긴다 — 그 main 을 만드는 별도 `task-create`
    // 호출 전까지 이 task 가 `Ready`(러너 dispatch 대상)로 노출되지 않고
    // `Waiting` 에 묶인다(`docs/dev-guide/agent-runner.md` "fallback{task}
    // 생성 순서 TOCTOU" 참조). 미지정 시 기본 `false`(기존 동작 그대로).
    let reserved_for_fallback = params
        .get("reserved_for_fallback")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let Err(e) = validate_poll_strategy_refs(&command, &on_failure) {
        return JsonRpcResponse::invalid_params(id, e);
    }
    if let Err(e) = validate_task_output_refs(&command, &depends_on, &on_failure) {
        return JsonRpcResponse::invalid_params(id, e);
    }

    let opts = TaskCreateOpts {
        workspace_id,
        name,
        command,
        depends_on,
        on_failure,
        metadata,
        now_ms: now_ms(),
    };
    mark_durability(
        core,
        match core.task_create(engine, opts, reserved_for_fallback) {
            Ok(task) => match serde_json::to_value(&task) {
                Ok(mut v) => {
                    if let Some(warnings) = fallback_with_deps_warning(&task)
                        && let Some(obj) = v.as_object_mut()
                    {
                        obj.insert("warnings".into(), json!(warnings));
                    }
                    JsonRpcResponse::success(id, v)
                }
                Err(e) => JsonRpcResponse::error(id, -32603, format!("serialize: {e}")),
            },
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

/// `on_failure=Fallback` + 비어있지 않은 `depends_on` 조합은 항상 죽은 설정은
/// 아니지만(이 task 자신이 명령 실행에서 직접 실패하는 Running→Failed 경로에서는
/// 정상 동작), 의존성 실패로 인한 Waiting→Skipped 전이에는 적용되지 않는다
/// (`apply_on_failure`가 `Fallback`에 대해 `None`을 반환) — 착각하기 쉬운 조합이라
/// 생성 자체는 막지 않고 경고만 담아 돌려준다.
fn fallback_with_deps_warning(task: &tasty_agent::Task) -> Option<Vec<&'static str>> {
    if matches!(task.on_failure, OnFailure::Fallback { .. }) && !task.depends_on.is_empty() {
        Some(vec![
            "on_failure=Fallback only takes effect when this task itself fails (Running -> Failed); \
             it has no effect on the Waiting -> Skipped transition caused by a failed dependency (depends_on)",
        ])
    } else {
        None
    }
}

/// `command` 및 `on_failure.Fallback.inline` 이 참조하는 `poll` 이름(완료 판정
/// 전략)을 생성 시점에 검증한다 — task_create 시 미등록 전략 이름을 거부하기
/// 위함이다. 오타를 실행 시점(Custom dispatch)이 아니라 생성 시점에 잡아
/// task 가 Running 에 진입한 뒤에야 실패하는 것을 막는다. `Custom` dispatch 이름
/// 해석(`src/core/agent/runner_host.rs`)과 같은 `resolve_poll_spec` 을 공유한다.
///
/// **범위 주의**: 이 함수는 poll 전략 *이름*만 검증한다. `OnFailure::Fallback.task`
/// 와 `TaskCommand::Reduce.inputs` 가 가리키는 task id 의 존재 검증은 여기가 아니라
/// `core.task_create` → `TaskStore::create`(store 층, `crates/tasty-agent/src/task/
/// store.rs`)가 담당한다 — store 불변식이라 이 handler 를 거치지 않는 호출자에도
/// 적용돼야 하기 때문이다. 두 검증은 서로 다른 거부 사유를 다루므로 겹치지 않는다.
fn validate_poll_strategy_refs(
    command: &TaskCommand,
    on_failure: &OnFailure,
) -> Result<(), String> {
    validate_command_poll_ref(command)?;
    if let OnFailure::Fallback {
        inline: Some(spec), ..
    } = on_failure
    {
        validate_command_poll_ref(&spec.command)?;
        validate_poll_strategy_refs(&spec.command, &spec.on_failure)?;
    }
    Ok(())
}

/// `${task.<id>.output<pointer>}` 참조가 **선언된 의존성 안에만** 있는지 생성
/// 시점에 검증한다.
///
/// 참조한 task 가 `depends_on`(또는 `Reduce.inputs`)에 없으면 그 task 가 아직
/// 끝나기 전에 이 task 가 dispatch 될 수 있다 — 값이 없어 실행 시점에 실패하는
/// race 가 된다. 그 race 를 만들지 않는 방법은 두 가지였다.
///
/// - **거부(채택)**: 사용자가 의존을 명시하게 한다. 그래프가 선언한 그대로 남는다.
/// - 자동으로 엣지 추가: `referenced_task_ids`(참조 무결성)에 새 출처를 더해야
///   하고, 선언하지 않은 의존성이 `agent.task_graph` 렌더와 DAG 그룹핑에 조용히
///   나타난다. 놀라움이 크다.
///
/// 거부를 택했으므로 `crates/tasty-agent/src/task/graph.rs` 의 엣지 정의는
/// 건드리지 않는다 — DAG 도출/그룹핑에 영향이 없다.
///
/// 문법 자체가 깨진 참조(`${task.` 로 시작하는데 형식 불일치)도 여기서 걸린다.
/// 파서는 dispatch 치환과 **같은 모듈**(`core::agent::task_output_ref`)이라 생성
/// 검증과 실행 치환이 문법을 서로 다르게 볼 수 없다.
fn validate_task_output_refs(
    command: &TaskCommand,
    depends_on: &[TaskId],
    on_failure: &OnFailure,
) -> Result<(), String> {
    use crate::core::agent::task_output_ref;

    let mut available: BTreeSet<&str> = depends_on.iter().map(String::as_str).collect();
    if let TaskCommand::Reduce { inputs, .. } = command {
        available.extend(inputs.iter().map(String::as_str));
    }
    for tid in task_output_ref::referenced_tasks(command).map_err(|e| e.0)? {
        if !available.contains(tid.as_str()) {
            return Err(format!(
                "task output reference '{tid}' is not in depends_on; add it so this task runs after '{tid}' finishes"
            ));
        }
    }
    // 인라인 fallback 은 별도 task 로 생성되므로 자기 의존성 기준으로 검증한다 —
    // `depends_on_override` 가 없으면 main 의 depends_on 을 그대로 물려받는다
    // (`crates/tasty-agent/src/task/store.rs`).
    if let OnFailure::Fallback {
        inline: Some(spec), ..
    } = on_failure
    {
        let inherited: Vec<TaskId> = spec
            .depends_on_override
            .clone()
            .unwrap_or_else(|| depends_on.to_vec());
        validate_task_output_refs(&spec.command, &inherited, &spec.on_failure)?;
    }
    Ok(())
}

fn validate_command_poll_ref(command: &TaskCommand) -> Result<(), String> {
    if let TaskCommand::Custom {
        poll: Some(PollSpecRef::Named { strategy }),
        ..
    } = command
    {
        let id = crate::completion_strategy::CompletionStrategyId::new(strategy.clone());
        crate::completion_strategy::global()
            .resolve_poll_spec(&id)
            .map_err(|e| format!("poll strategy '{strategy}': {e}"))?;
    }
    Ok(())
}

// ============================================================
// agent.task_list
// ============================================================

/// state 이름 목록 파라미터를 뽑는다 — `task_list`(`state`) 와
/// `task_purge`(`states`) 가 공유한다.
///
/// 받아들이는 형태는 셋 다 동일하다:
/// - 배열 `["waiting", "ready"]`
/// - 콤마 구분 문자열 `"waiting,ready"`
/// - 단일 문자열 `"waiting"`
///
/// 단수/복수 키 이름도 서로 폴백한다(`state` ↔ `states`). 두 명령의 플래그
/// 이름이 달라서 생기던 혼동 — 콤마 목록을 `task_list --state` 에 넘기면 그
/// 문자열 전체가 하나의 상태 이름으로 취급돼 매칭이 0 건이 되고, 필터가 조용히
/// 무력화되던 — 을 IPC 계층에서 없앤다.
///
/// 빈 값(빈 배열 / 빈 문자열 / 콤마만 있는 문자열)은 `None` 으로 처리해
/// "필터 없음" 이 된다 — 조회 계열(`task_list`)의 의미. 파괴적인
/// `task_purge` 는 빈 값을 "매칭 없음" 으로 봐야 하므로 이 함수 대신
/// [`state_names_param_keep_empty`] 를 쓴다.
fn state_names_param(params: &Value, primary_key: &str) -> Option<Vec<String>> {
    let names = state_names_param_keep_empty(params, primary_key)?;
    (!names.is_empty()).then_some(names)
}

/// [`state_names_param`] 과 같은 파싱을 하되 **키의 존재 여부를 보존한다** —
/// 키가 아예 없거나 `null` 이면 `None`(= 필터 없음), 키는 있는데 파싱 결과가
/// 비면 `Some(vec![])`(= 매칭 없음) 이다.
///
/// 이 구분이 `task_purge` 에는 필수다. `{"states": []}` 를 `None` 으로 접으면
/// "상태 필터 없음" 이 되어 `older_than_ms` 만으로 상태 무관 전체가 삭제
/// 후보가 된다 — "상태를 하나도 안 골랐으니 아무것도 안 지운다" 라는 호출자
/// 기대와 정반대이고, `Core::task_purge` 의 "둘 다 미지정이면 거부" 가드도
/// 우회한다. `Some(vec![])` 는 `plan_sweep` 의 `states.iter().any(..)` 가 항상
/// `false` 라 후보 0 건이 된다.
///
/// 값의 타입이 배열/문자열이 아닌 경우(예: 숫자)도 "키는 있으나 이름 0 개" 로
/// 본다 — 안전한 쪽(아무것도 안 지움)으로 접는다.
fn state_names_param_keep_empty(params: &Value, primary_key: &str) -> Option<Vec<String>> {
    let alias = if primary_key == "states" {
        "state"
    } else {
        "states"
    };
    let raw = params
        .get(primary_key)
        .or_else(|| params.get(alias))
        .filter(|v| !v.is_null())?;

    let names: Vec<String> = match raw {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|x| x.as_str())
            .flat_map(split_state_names)
            .collect(),
        Value::String(s) => split_state_names(s.as_str()),
        _ => Vec::new(),
    };
    Some(names)
}

fn split_state_names(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// state 이름 목록으로 task 를 걸러낸다. `None` 이면 필터 없음(전체 유지).
fn retain_by_state(tasks: &mut Vec<Task>, states: Option<&[String]>) {
    let Some(states) = states else { return };
    tasks.retain(|t| states.iter().any(|s| s == t.state.name()));
}

pub fn handle_task_list(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let state_filter = state_names_param(params, "state");

    match core.task_list(engine, workspace_id) {
        Err(e) => agent_err_to_response(id, e),
        Ok(mut tasks) => {
            retain_by_state(&mut tasks, state_filter.as_deref());
            JsonRpcResponse::success(
                id,
                json!({
                    "total": tasks.len(),
                    "tasks": tasks,
                    "runner": runner_status_json(core, engine, workspace_id),
                }),
            )
        }
    }
}

// ============================================================
// 결정 1/5: "runner 가 꺼져 있다" 를 조회로 드러내기 — 새 메서드 없이
// task_list/task_graph 에 runner 상태를 동반하고, task_get 에 hook_wait 대기
// 상태를 동반한다.
// ============================================================

/// `task_run --action status` 와 동일 형태의 runner 상태 요약. runner thread
/// 유무와 무관하게 store 를 직접 조회하므로, runner 가 꺼져 있어도(즉
/// `running: false` 여도) `ready_count`/`running_count` 는 실제 값을 낸다 —
/// "비-terminal task 는 있는데 아무도 안 돌리고 있다"가 `task_list`/`task_graph`
/// 응답만으로 드러나는 이유.
///
/// 그 조회 자체가 실패하면 카운트는 **`null`** 이고 `store_error` 가 이유를 싣는다.
/// 0 을 돌려주면 "task 가 없다" 와 값이 같아져 위 계약이 거짓이 된다.
/// `list_failures` 는 러너 스레드의 연속 조회 실패 횟수 — `running: true` 인데 이
/// 값이 크면 러너는 살아 있지만 DAG 는 정지 상태다.
fn runner_status_json(core: &Core, engine: &crate::core::CoreState, workspace_id: u32) -> Value {
    let ctx = core.runner_context(engine);
    let status = core.agent_runner_registry().status(&ctx, workspace_id);
    runner_status_value(&status)
}

fn runner_status_value(status: &crate::core::agent::runner_thread::RunnerStatus) -> Value {
    json!({
        "running": status.running,
        "crashed": status.crashed,
        "ready_count": status.ready_count,
        "running_count": status.running_count,
        "store_error": status.store_error,
        "list_failures": status.list_failures,
    })
}

/// task 가 `AwaitExternal` handle 로 외부 신호를 기다리는 중이면 그 사실 +
/// deadline 을 노출한다. `task_get` 이 이 값을 실어야 "그냥 running" 과
/// 구분된다(결정 5) — `AwaitExternal` 의 poll 은 계약상 항상 Active 라 state 만
/// 봐서는 대기 이유를 알 수 없기 때문이다.
fn awaiting_external_json(
    core: &Core,
    engine: &crate::core::CoreState,
    workspace_id: u32,
    task_id: &str,
) -> Option<Value> {
    let ctx = core.runner_context(engine);
    match crate::core::agent::runner_host::load_dispatch_handle(&ctx, workspace_id, task_id)? {
        DispatchHandle::AwaitExternal {
            wait_key,
            deadline_ms,
        } => Some(json!({
            "wait_key": wait_key,
            "deadline_ms": deadline_ms,
        })),
        _ => None,
    }
}

// ============================================================
// agent.task_get
// ============================================================

pub fn handle_task_get(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    match core.task_get(engine, workspace_id, &task_id) {
        Err(e) => agent_err_to_response(id, e),
        Ok(None) => JsonRpcResponse::error(id, -32004, format!("task not found: {task_id}")),
        Ok(Some(t)) => {
            let is_running = matches!(t.state, TaskState::Running);
            let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
            // 결정 5: Running 인데 AwaitExternal 로 외부 신호를 기다리는 task 는
            // "그냥 running" 과 구분되게 대기 정보(+deadline)를 함께 싣는다.
            if is_running
                && let Some(obj) = v.as_object_mut()
                && let Some(info) = awaiting_external_json(core, engine, workspace_id, &task_id)
            {
                obj.insert("awaiting_external".to_string(), info);
            }
            JsonRpcResponse::success(id, v)
        }
    }
}

// ============================================================
// agent.task_cancel
// ============================================================

pub fn handle_task_cancel(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    mark_durability(
        core,
        match core.task_cancel(engine, workspace_id, &task_id, now_ms()) {
            Err(e) => agent_err_to_response(id, e),
            Ok((task, cascaded)) => JsonRpcResponse::success(
                id,
                json!({
                    "task": task,
                    "cascaded": cascaded,
                }),
            ),
        },
    )
}

// ============================================================
// agent.task_retry
// ============================================================

pub fn handle_task_retry(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let reset_downstream = params
        .get("reset_downstream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    mark_durability(
        core,
        match core.task_retry(engine, workspace_id, &task_id, reset_downstream, now_ms()) {
            Err(e) => agent_err_to_response(id, e),
            Ok(task) => match serde_json::to_value(task) {
                Ok(v) => JsonRpcResponse::success(id, v),
                Err(e) => JsonRpcResponse::error(id, -32603, format!("serialize: {e}")),
            },
        },
    )
}

// ============================================================
// agent.task_await — 진짜 blocking (TaskWakerHub 기반)
// ============================================================
//
// J.A.S5: `await_task_blocking` 가 worker thread 안에서 호출되어 (app_methods 의
// `ipc_dispatch_task_await` 분기), 현 state 가 종결이면 즉시 반환, 아니면 hub 에
// waiter 등록 후 recv_timeout. `timeout_ms` 생략 시 [`DEFAULT_TASK_AWAIT_TIMEOUT_MS`]
// (10분)로 대체된다 — 무한 대기가 기본값이면 호출자가 잊고 넘어간 순간 영구 hang
// 이 되므로, 명시적으로 `timeout_ms: 0` 을 넘긴 경우에만 무한 대기를 허용한다
// (approval 과 다른 지점 — task 는 record-level timeout 이 없어 이 IPC 파라미터가
// 유일한 상한이다).
//
// 응답 형식:
//   { outcome: "terminal" | "timed_out" | "not_found",
//     state: "succeeded" | "failed" | ...,  // outcome=terminal 일 때만
//     result: { ... },                        // outcome=terminal 일 때만, 있으면
//   }

/// `timeout_ms` 파라미터 생략 시 기본값(10분). **잠정값** — 근거 있는 측정에서
/// 나온 수치가 아니라 "무한보다는 낫다" 수준의 출발점이다. 실사용 경험이 쌓이면
/// 적정값으로 재조정 필요. 명시적으로 `timeout_ms: 0` 을 넘기면 이 기본값을
/// 우회하고 무한 대기한다(CLI `tasty agent task-await --timeout-ms 0`).
pub const DEFAULT_TASK_AWAIT_TIMEOUT_MS: u64 = 600_000;

pub fn await_task_blocking(
    hub: &crate::core::agent::task_waker::TaskWakerHub,
    memory: &std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    agent_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    rpc_id: Value,
    params: &Value,
) -> JsonRpcResponse {
    use crate::core::agent::task_waker::{AwaitOutcome, TerminalSnapshot};
    use tasty_memory::HOST_OWNER;

    let workspace_id = match workspace_id_param(params, &rpc_id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &rpc_id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    // 생략 시 기본 10분(잠정) — 명시적 0 만 무한 대기로 우회.
    let timeout_ms = Some(
        p_try!(params::opt_int::<u64>(params, "timeout_ms", &rpc_id))
            .unwrap_or(DEFAULT_TASK_AWAIT_TIMEOUT_MS),
    );

    // 1. 현 state snapshot.
    let snap_opt: Option<TerminalSnapshot> = {
        let mut guard = crate::poison::recover_mutex(
            memory.lock(),
            crate::core::MEMORY_WHAT,
            &crate::core::MEMORY_POISONED,
        );
        let store = tasty_agent::TaskStore::new(&mut *guard, HOST_OWNER, agent_seq.as_ref());
        match store.get(workspace_id, &task_id) {
            Ok(Some(t)) => Some(TerminalSnapshot {
                state: t.state,
                result: t.result,
            }),
            Ok(None) => None,
            Err(_) => None,
        }
    };
    let Some(current) = snap_opt else {
        return JsonRpcResponse::success(rpc_id, json!({ "outcome": "not_found" }));
    };

    // 2. blocking await.
    let outcome = hub.await_terminal(workspace_id, &task_id, timeout_ms, current);
    match outcome {
        AwaitOutcome::Terminal(snap) => {
            let mut resp = json!({
                "outcome": "terminal",
                "state": snap.state.name(),
            });
            if let Some(r) = snap.result {
                resp["result"] = serde_json::to_value(r).unwrap_or(Value::Null);
            }
            JsonRpcResponse::success(rpc_id, resp)
        }
        AwaitOutcome::TimedOut => {
            JsonRpcResponse::success(rpc_id, json!({ "outcome": "timed_out" }))
        }
    }
}

/// `agent.task_await` — 블로킹 대기를 워커로 돌린다.
///
/// 호출자가 이미 푼 store 를 받는다. **어느 engine 의 것인지 고르는 일이 조합마다
/// 다르기 때문**이다 — gui 는 창/parked 를 훑고 헤드리스는 하나뿐인 engine 을 쓴다.
/// 고른 뒤에 하는 일은 같으므로 그 뒤만 여기 있다.
pub(crate) fn spawn_task_await(
    hub: std::sync::Arc<crate::core::agent::task_waker::TaskWakerHub>,
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    agent_seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
    rpc_id: Value,
    params: Value,
    response_tx: &std::sync::mpsc::SyncSender<JsonRpcResponse>,
) {
    let response_tx = response_tx.clone();
    std::thread::spawn(move || {
        let resp = await_task_blocking(&hub, &memory, agent_seq, rpc_id, &params);
        tasty_ipc::server::send_response(&response_tx, resp);
    });
}

// ============================================================
// agent.task_graph
// ============================================================

/// Graphviz dot 렌더 — `agent.task_graph` 와 `agent.dag_get` 이 공유한다. 노드 색/엣지
/// 스타일 규칙이 두 벌로 갈라지지 않게 하는 단일 지점이다.
///
/// Reduce.inputs 는 depends_on 과 같은 암묵적 의존성 엣지(사이클 검출 대상,
/// `TaskGraph::dfs_cycle`)라 시각화에도 반영한다. fallback 엣지(사전 존재
/// `Fallback{task}` + 동적 생성 `Fallback{inline}`)는 `referenced_task_ids`(참조 무결성
/// 검증 대상, inline 은 예외)일 뿐 `dfs_cycle` 이 보는 엣지가 아니다 — 사이클 검출
/// 커버리지와 무관하게 관측 목적으로만 함께 그린다(A↔F 상호 fallback 같은 순환도 생성
/// 시점에 막히지 않는다). depends_on 과 구분되게 점선/색으로 표시한다. 수집 규칙은
/// [`collect_graph_edges`] 하나로 dot/json 양쪽이 공유한다.
fn render_graph_dot(tasks: &[Task]) -> String {
    let mut out = String::from("digraph G {\n  rankdir=LR;\n");
    for t in tasks {
        let color = match &t.state {
            TaskState::Ready => "lightblue",
            TaskState::Running => "yellow",
            TaskState::Succeeded => "lightgreen",
            TaskState::Failed { .. } => "salmon",
            TaskState::Cancelled => "gray",
            TaskState::Skipped => "lightgray",
            TaskState::Waiting => "white",
            TaskState::Unknown => "orange",
        };
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\\n{}\", style=filled, fillcolor={}];\n",
            t.id,
            escape_dot(&t.name),
            t.state.name(),
            color
        ));
    }
    for edge in collect_graph_edges(tasks) {
        let (style, color) = match edge.kind {
            "fallback" => ("dashed", "orangered"),
            "reduce" => ("dotted", "blue"),
            _ => {
                out.push_str(&format!("  \"{}\" -> \"{}\";\n", edge.from, edge.to));
                continue;
            }
        };
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [style={}, color={}, label=\"{}\"];\n",
            edge.from, edge.to, style, color, edge.kind
        ));
    }
    out.push_str("}\n");
    out
}

/// json 렌더의 `nodes` — `agent.task_graph` 와 `agent.dag_get` 이 공유한다.
fn render_graph_nodes(tasks: &[Task]) -> Vec<Value> {
    tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "state": t.state.name(),
                "command_kind": task_command_kind(&t.command),
                "on_failure_kind": on_failure_kind(&t.on_failure),
            })
        })
        .collect()
}

/// json 렌더의 `edges` — `agent.task_graph` 와 `agent.dag_get` 이 공유한다.
///
/// depends_on/Fallback.task/Reduce.inputs 는 `referenced_task_ids`(참조 무결성 검증)가
/// 보는 3종 참조고, fallback 엣지에는 동적 생성되는 `Fallback{inline}` 케이스
/// (`metadata.fallback_of` 역참조, 무결성 검증 대상 아님)도 함께 포함된다 — 넷을 한
/// 엣지 리스트에 합치되 `kind` 로 구분해, depends_on 만 보던 시각화가 fallback/reduce
/// 관계를 놓치지 않게 한다. 단, 사이클 검출(`TaskGraph::dfs_cycle`)은 이 중
/// depends_on/Reduce.inputs 만 순회한다 — fallback 엣지는 순회 대상이 아니라서, 예를 들어
/// A→(fallback:F)/F→(fallback:A) 처럼 서로를 fallback 으로 참조하는 순환은 생성 시점에
/// 거부되지 않고 그대로 저장된다. 이 엣지들은 그 순환을 관측 가능하게 만들 뿐,
/// 자동으로 막지는 않는다.
fn render_graph_edges(tasks: &[Task]) -> Vec<Value> {
    collect_graph_edges(tasks)
        .into_iter()
        .map(|edge| json!({"from": edge.from, "to": edge.to, "kind": edge.kind}))
        .collect()
}

pub fn handle_task_graph(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let format = params
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json")
        .to_string();

    let tasks = match core.task_list(engine, workspace_id) {
        Err(e) => return agent_err_to_response(id, e),
        Ok(t) => t,
    };

    // 사이클은 detection 만 — graph 그리기는 그대로 한다.
    let cycle = TaskGraph::build(&tasks).detect_cycles().err();
    let runner = runner_status_json(core, engine, workspace_id);

    match format.as_str() {
        "dot" => JsonRpcResponse::success(
            id,
            json!({
                "format": "dot",
                "dot": render_graph_dot(&tasks),
                "cycle": cycle.as_ref().map(|e| e.to_string()),
                "runner": runner,
            }),
        ),
        _ => {
            let nodes: Vec<Value> = render_graph_nodes(&tasks);
            let edges: Vec<Value> = render_graph_edges(&tasks);
            JsonRpcResponse::success(
                id,
                json!({
                    "format": "json",
                    "nodes": nodes,
                    "edges": edges,
                    "cycle": cycle.as_ref().map(|e| e.to_string()),
                    "runner": runner,
                }),
            )
        }
    }
}

// ============================================================
// agent.dag_list / agent.dag_get
// ============================================================
//
// Tasty 의 영속 모델에 DAG 레코드는 없다 — `tasty_agent::group_tasks_into_dags` 가
// `metadata.dag`(explicit) + 그래프 연결성(derived)에서 도출한다. 근거·규칙은
// `crates/tasty-agent/src/task/dag.rs` 모듈 doc.

/// `workspace_id` 는 **선택**이다 — 생략하면 살아있는 전 workspace 를 순회한다
/// (원칙 3, 포커스 독립성). 잘못된 타입이 오면 조용히 전체 순회로 떨어지지 않고
/// invalid_params 로 거절한다.
fn optional_workspace_id_param(params: &Value, id: &Value) -> Result<Option<u32>, JsonRpcResponse> {
    crate::adapters::ipc::handler::params::optional_u32(params, "workspace_id", id)
}

/// `include_tasks=false`(기본)면 `task_ids` 를 응답에서 뺀다 — 목록 화면은 요약만
/// 쓰는데 큰 DAG 의 id 배열을 매 폴링마다 실어 나를 이유가 없다.
fn dag_summary_json(dag: &tasty_agent::DagSummary, include_tasks: bool) -> Value {
    let mut v = serde_json::to_value(dag).unwrap_or(Value::Null);
    if !include_tasks && let Some(obj) = v.as_object_mut() {
        obj.remove("task_ids");
    }
    v
}

pub fn handle_dag_list(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match optional_workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let include_tasks = params
        .get("include_tasks")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match core.dag_list(engine, workspace_id) {
        Err(e) => agent_err_to_response(id, e),
        Ok(dags) => {
            let rendered: Vec<Value> = dags
                .iter()
                .map(|d| dag_summary_json(d, include_tasks))
                .collect();
            JsonRpcResponse::success(
                id,
                json!({
                    "total": rendered.len(),
                    "dags": rendered,
                    // 열거 범위를 응답에 박아둔다 — "CLI 로 만든 task 가 왜 목록에
                    // 없나" 를 나중에 추적 가능하게 하기 위함. 삭제된 workspace 에
                    // 남은 고아 task 는 뜨지 않는다(부팅 시 자동 GC 의 몫).
                    "scope": "live_workspaces",
                }),
            )
        }
    }
}

/// DAG 부분집합 그래프에서 **사이클만** 뽑는다.
///
/// `detect_cycles()` 는 `depends_on` 이 순회 집합 밖을 가리키면
/// `UnknownDependency` 를 반환하는데(`crates/tasty-agent/src/task/graph.rs`),
/// explicit 그룹(`metadata.dag`)은 그룹 밖 task 를 `depends_on` 할 수 있으므로 그건
/// 이 경로에서 **정상**이다. 그대로 응답 `cycle` 에 실으면 `has_cycle:false` +
/// `cycle:non-null` 로 응답이 자기모순이 되고, 소비 화면이 멀쩡한 DAG 에 "사이클
/// 감지" 를 띄운다. `DagSummary::has_cycle`(`task/dag.rs::summarize`)과 같은 기준으로
/// 좁혀 두 값이 항상 같은 사실을 말하게 한다.
///
/// workspace 전체를 넘기는 `handle_task_graph` 는 이 좁힘이 필요 없다 — 그쪽의
/// `UnknownDependency` 는 진짜 dangling 참조라 그대로 드러내는 게 맞다.
fn subset_cycle(tasks: &[Task]) -> Option<AgentError> {
    match TaskGraph::build(tasks).detect_cycles() {
        Err(e @ AgentError::DependencyCycle(_)) => Some(e),
        _ => None,
    }
}

pub fn handle_dag_get(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match optional_workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let dag_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing required 'id' (dag id)"),
    };
    let format = params
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json")
        .to_string();

    let (dag, tasks) = match core.dag_get(engine, workspace_id, &dag_id) {
        Err(e) => return agent_err_to_response(id, e),
        Ok(None) => {
            return JsonRpcResponse::invalid_params(id, format!("unknown dag id: {dag_id}"));
        }
        Ok(Some(found)) => found,
    };

    // `has_cycle` 은 이미 요약에 있고, 여기서는 사람이 읽는 사유 문자열도 함께 낸다.
    let cycle = subset_cycle(&tasks);
    let runner = runner_status_json(core, engine, dag.workspace_id);
    let summary = dag_summary_json(&dag, true);

    match format.as_str() {
        "dot" => JsonRpcResponse::success(
            id,
            json!({
                "format": "dot",
                "dag": summary,
                "dot": render_graph_dot(&tasks),
                "cycle": cycle.as_ref().map(|e| e.to_string()),
                "runner": runner,
            }),
        ),
        _ => JsonRpcResponse::success(
            id,
            json!({
                "format": "json",
                "dag": summary,
                "nodes": render_graph_nodes(&tasks),
                "edges": render_graph_edges(&tasks),
                "cycle": cycle.as_ref().map(|e| e.to_string()),
                "runner": runner,
            }),
        ),
    }
}

pub fn handle_task_reduce(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let inputs: Vec<TaskId> = match params.get("inputs").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => {
            return JsonRpcResponse::invalid_params(
                id,
                "Missing or empty 'inputs' (array of task ids)",
            );
        }
    };
    let strategy_val = match params.get("strategy") {
        Some(v) => v.clone(),
        None => return JsonRpcResponse::invalid_params(id, "Missing 'strategy'"),
    };
    let strategy: ReducerStrategy = match serde_json::from_value(strategy_val) {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::invalid_params(id, format!("invalid 'strategy': {e}"));
        }
    };
    let extract_path = params.get("extract_path").and_then(|v| v.as_str());

    // 1단계: task 결과 수집 (memory access 안에서). Core 가 lock + store 조립을 담당.
    let collected = match core.task_reduce_collect(engine, workspace_id, &inputs) {
        Err(e) => return agent_err_to_response(id, e),
        Ok(v) => v,
    };

    // 1.5단계: `extract_path` 지정 시 각 input 의 output 에서 그 경로만 추출
    // (예: `/stdout/text`) — `Run` task 의 `{pid,stdout,stderr}` 구조를 모른 채
    // 그대로 합성하면 concat_text/merge_json 결과가 못 쓸 형태가 된다. 경로가
    // 없는 input 은 null 로 대체되고 그 사실이 warnings 에 남는다(조용히 누락
    // 처리하지 않음) — 나머지 input 의 reduce 는 계속 진행한다.
    let (collected, warnings) = extract_paths(&collected, extract_path);

    // 2단계: reducer 실행 (memory lock 바깥에서; custom shell 은 stdin/stdout I/O).
    let result = reduce_with_custom(&strategy, &collected, run_custom_shell);
    match result {
        Ok(value) => JsonRpcResponse::success(id, json!({ "value": value, "warnings": warnings })),
        Err(e) => agent_err_to_response(id, e),
    }
}

// ============================================================
// agent.task_run — workspace 단위 runner thread 시작/중단/상태조회
// ============================================================
//
// action: "start" | "stop" | "status".
// 응답: { running, crashed, ready_count, running_count, store_error, list_failures }.
// store 조회 실패 시 두 카운트는 null 이고 store_error 가 이유를 싣는다.
// start: 이미 실행 중이면 no-op (running=true 그대로).
// stop:  실행 중이면 정지 후 join, 아니면 no-op.
// status: 카운트만 갱신.
pub fn handle_task_run(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let ctx = core.runner_context(engine);
    let registry = core.agent_runner_registry();
    match action {
        "start" => {
            registry.start(ctx.clone(), workspace_id);
        }
        "stop" => {
            registry.stop(workspace_id);
        }
        "status" => {}
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'action': {other} (expected start|stop|status)"),
            );
        }
    }
    let status = registry.status(&ctx, workspace_id);
    JsonRpcResponse::success(id, runner_status_value(&status))
}

// ============================================================
// agent.task_set_result — 외부 호출자가 task 완료 신호를 보내는 진입점
// ============================================================
//
// runner 가 dispatch 하는 task 외에 *수동 / 외부 통합 task* 의 결과 보고용.
// runner thread 는 Core 의 wrapper 를 직접 호출하므로 본 IPC 를 거치지 않는다.
// state 인자: "succeeded" | "failed" (그 외 거부).
pub fn handle_task_set_result(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let state_str = match params.get("state").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return JsonRpcResponse::invalid_params(id, "Missing 'state' ('succeeded' | 'failed')");
        }
    };
    let exit_code = p_try!(
        params::read_signed::<i32>(params, "exit_code")
            .map_err(|msg| JsonRpcResponse::invalid_params(id.clone(), msg))
    );
    let output = params.get("output").cloned();
    let error = params
        .get("error")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let result = TaskResult {
        exit_code,
        output,
        error: error.clone(),
    };

    // 1단계: result 영속.
    if let Err(e) = core.task_set_result(engine, workspace_id, &task_id, result) {
        return agent_err_to_response(id, e);
    }

    // 2단계: state 전이. "failed" 면 TaskState::Failed { error } 로.
    let new_state = match state_str.as_str() {
        "succeeded" => TaskState::Succeeded,
        "failed" => TaskState::Failed {
            error: error.unwrap_or_else(|| "(unspecified)".to_string()),
        },
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'state': {other} (expected 'succeeded' or 'failed')"),
            );
        }
    };

    mark_durability(
        core,
        match core.task_set_state(engine, workspace_id, &task_id, new_state, now_ms()) {
            Err(e) => agent_err_to_response(id, e),
            Ok((task, cascaded)) => JsonRpcResponse::success(
                id,
                json!({
                    "task": task,
                    "cascaded": cascaded,
                }),
            ),
        },
    )
}

// ============================================================
// agent.task_delete
// ============================================================

/// task 삭제 — 참조(depends_on/Fallback.task/Reduce.inputs)가
/// 있으면 기본 거부하고 참조자 목록을 `error.data.referenced_by` 로 반환한다.
/// `cascade` 는 전이적 참조자 전부를 함께 지우고, `force` 는 참조 검사만
/// 우회한다(상태 제약은 못 뚫음 — `Running` 은 항상 거부).
pub fn handle_task_delete(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let cascade = params
        .get("cascade")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let force = params
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    mark_durability(
        core,
        match core.task_delete(
            engine,
            workspace_id,
            &task_id,
            TaskDeleteOpts { cascade, force },
        ) {
            Err(e) => agent_err_to_response(id, e),
            Ok(report) => JsonRpcResponse::success(id, json!({ "deleted": report.deleted })),
        },
    )
}

// ============================================================
// agent.task_purge
// ============================================================

/// `task_purge` 요청 파라미터 → [`TaskPurgeFilter`] (순수 함수라 테스트에서
/// `plan_sweep` 과 직접 조합해 "무엇이 후보가 되는가" 를 확정할 수 있다).
///
/// 상태 목록은 빈 값을 "필터 없음" 으로 접지 않는다 — `{"states": []}` 은
/// "고른 상태가 없음 = 매칭 없음" 이지 "상태 무관 전체" 가 아니다(아래
/// `handle_task_purge` doc 참조 — `task_list` 와 의미가 반대인 이유).
/// `Result` 를 반환하는 이유: 잘못된 `older_than_ms` 를 `None` 으로 만들면 **필터 없음**
/// 과 구별되지 않아, 사용자가 지정한 것보다 **넓은 범위가 삭제 후보**가 된다.
fn purge_filter_from_params(params: &Value, now_ms: u64) -> Result<TaskPurgeFilter, String> {
    Ok(TaskPurgeFilter {
        states: state_names_param_keep_empty(params, "states"),
        older_than_ms: params::read_int::<u64>(params, "older_than_ms")?,
        now_ms,
    })
}

/// task 일괄 삭제 — 상태 이름 목록(`states`, `TaskState::name()`
/// 값들)과 경과시간(`older_than_ms`) 필터로 후보를 고르고, 참조 안전 + `Running`
/// 제외를 지킨 것만 실제로 지운다. 둘 다 생략하면 워크스페이스 전체가 후보가
/// 되어 위험하므로 `core.task_purge` 가 거부한다. `dry_run=true` 면 아무것도
/// 지우지 않고 계획(`deleted`/`retained`)만 반환한다.
///
/// `states` 키가 있는데 이름이 하나도 없으면(`[]` / `""`) "매칭 없음" 이다 —
/// `task_list` 의 "빈 값 = 필터 없음" 과 의미가 반대다. 파괴적 명령이라
/// "상태를 하나도 안 골랐다" 를 "상태 무관 전체" 로 승격시키지 않는다.
pub fn handle_task_purge(
    core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let dry_run = params
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let filter = match purge_filter_from_params(params, now_ms()) {
        Ok(f) => f,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    };
    mark_durability(
        core,
        match core.task_purge(engine, workspace_id, filter, dry_run) {
            Err(e) => agent_err_to_response(id, e),
            Ok(plan) => JsonRpcResponse::success(
                id,
                json!({
                    "deleted": plan.deleted,
                    "retained": plan.retained,
                    "dry_run": dry_run,
                }),
            ),
        },
    )
}

#[cfg(test)]
mod poll_strategy_ref_tests {
    use super::*;
    use tasty_plugin_protocol::host_port::CompletionStrategyRegistryPort;

    /// `crate::completion_strategy::global()` 는 프로세스 전역 싱글턴이라 다른 테스트
    /// 파일과 공유된다 — 이 파일이 쓰는 id 는 충돌 방지를 위해 고유 접두어를 쓴다
    /// (`src/completion_strategy/registry_tests.rs` 의 `cstest-*` 관례와 동일 이유).
    fn install_poll_strategy(plugin_id: &str, short_id: &str) {
        crate::completion_strategy::HostCompletionStrategyPort
            .install_plugin_completion_strategies(
                plugin_id,
                &[json!({
                    "id": short_id,
                    "priority": 100,
                    "spec": {
                        "kind": "poll",
                        "poll_method": format!("{plugin_id}.wait"),
                        "state_field": "state",
                        "terminal_states": ["done"],
                    },
                })],
            );
    }

    // ── `${task.<id>.output<pointer>}` 참조의 생성 시점 검증 ────────────────

    fn tell_with(params: serde_json::Value) -> TaskCommand {
        TaskCommand::Custom {
            ipc_method: "claude.tell".into(),
            params,
            poll: None,
        }
    }

    /// depends_on 에 없는 task 를 참조하면 생성 시점에 거부 — 실행 시점 race 를
    /// 만들지 않는다.
    #[test]
    fn task_create_rejects_output_ref_not_in_depends_on() {
        let command = tell_with(json!({
            "surface_id": "${task.t-a.output/child_surface_id}",
        }));
        let err = validate_task_output_refs(&command, &[], &OnFailure::Abort).unwrap_err();
        assert!(err.contains("t-a"), "{err}");
        assert!(err.contains("depends_on"), "{err}");
    }

    /// depends_on 에 있으면 통과.
    #[test]
    fn task_create_accepts_output_ref_declared_in_depends_on() {
        let command = tell_with(json!({
            "surface_id": "${task.t-a.output/child_surface_id}",
        }));
        assert!(
            validate_task_output_refs(&command, &["t-a".to_string()], &OnFailure::Abort).is_ok()
        );
    }

    /// 여러 참조 중 하나만 빠져도 거부한다 — 부분 통과는 실행 시점 실패로 남는다.
    #[test]
    fn task_create_rejects_when_any_referenced_task_is_undeclared() {
        let command = tell_with(json!({
            "a": "${task.t-a.output/x}",
            "b": "${task.t-b.output/y}",
        }));
        let err = validate_task_output_refs(&command, &["t-a".to_string()], &OnFailure::Abort)
            .unwrap_err();
        assert!(err.contains("t-b"), "{err}");
    }

    /// `Reduce.inputs` 도 의존성 엣지라 참조 출처로 인정된다.
    #[test]
    fn task_create_accepts_output_ref_from_reduce_inputs() {
        // Reduce 자체엔 치환 자리가 없으므로, 인정 범위가 depends_on 에만 묶여
        // 있지 않다는 것만 고정한다.
        let command = TaskCommand::Reduce {
            inputs: vec!["t-a".to_string()],
            strategy: tasty_agent::ReducerStrategy::ConcatText,
        };
        assert!(validate_task_output_refs(&command, &[], &OnFailure::Abort).is_ok());
    }

    /// 인라인 fallback 안의 참조도 재귀 검증한다(poll 전략 검증과 같은 취급).
    #[test]
    fn task_create_validates_output_refs_inside_inline_fallback() {
        let inline = tasty_agent::InlineFallbackSpec {
            name: "fb".into(),
            command: tell_with(json!({ "surface_id": "${task.t-ghost.output/id}" })),
            depends_on_override: None,
            on_failure: OnFailure::Abort,
            metadata: Value::Null,
        };
        let on_failure = OnFailure::Fallback {
            task: None,
            inline: Some(Box::new(inline.clone())),
        };
        // main 의 depends_on 을 물려받는데 거기 t-ghost 가 없다 → 거부.
        let err =
            validate_task_output_refs(&tell_with(Value::Null), &["t-a".to_string()], &on_failure)
                .unwrap_err();
        assert!(err.contains("t-ghost"), "{err}");

        // depends_on_override 로 선언하면 통과.
        let ok_inline = tasty_agent::InlineFallbackSpec {
            depends_on_override: Some(vec!["t-ghost".to_string()]),
            ..inline
        };
        assert!(
            validate_task_output_refs(
                &tell_with(Value::Null),
                &[],
                &OnFailure::Fallback {
                    task: None,
                    inline: Some(Box::new(ok_inline)),
                },
            )
            .is_ok()
        );
    }

    /// 오문법은 참조 검증 단계에서 잡힌다 — dispatch 까지 가지 않는다.
    #[test]
    fn task_create_rejects_malformed_output_placeholder() {
        let command = tell_with(json!({ "x": "${task.t-a.ouput/id}" }));
        let err = validate_task_output_refs(&command, &["t-a".to_string()], &OnFailure::Abort)
            .unwrap_err();
        assert!(err.contains("malformed"), "{err}");
    }

    /// placeholder 가 없는 흔한 command 는 depends_on 이 비어도 그대로 통과한다.
    #[test]
    fn task_create_ignores_commands_without_placeholders() {
        let command = tell_with(json!({ "surface_id": 3, "message": "hi" }));
        assert!(validate_task_output_refs(&command, &[], &OnFailure::Abort).is_ok());
    }

    #[test]
    fn named_poll_ref_to_unregistered_strategy_is_rejected() {
        let command = TaskCommand::Custom {
            ipc_method: "tcpoll1.spawn".into(),
            params: Value::Null,
            poll: Some(PollSpecRef::Named {
                strategy: "tcpoll1/does-not-exist".into(),
            }),
        };
        let err = validate_poll_strategy_refs(&command, &OnFailure::Abort).unwrap_err();
        assert!(err.contains("tcpoll1/does-not-exist"));
    }

    #[test]
    fn named_poll_ref_to_registered_strategy_is_accepted() {
        install_poll_strategy("tcpoll2", "spawn-wait");
        let command = TaskCommand::Custom {
            ipc_method: "tcpoll2.spawn".into(),
            params: Value::Null,
            poll: Some(PollSpecRef::Named {
                strategy: "tcpoll2/spawn-wait".into(),
            }),
        };
        assert!(validate_poll_strategy_refs(&command, &OnFailure::Abort).is_ok());
    }

    #[test]
    fn named_poll_ref_inside_inline_fallback_is_validated() {
        let bad_fallback = TaskCommand::Custom {
            ipc_method: "tcpoll3.spawn".into(),
            params: Value::Null,
            poll: Some(PollSpecRef::Named {
                strategy: "tcpoll3/does-not-exist".into(),
            }),
        };
        let on_failure = OnFailure::Fallback {
            task: None,
            inline: Some(Box::new(tasty_agent::task::InlineFallbackSpec {
                name: "fb".into(),
                command: bad_fallback,
                depends_on_override: None,
                on_failure: OnFailure::Abort,
                metadata: Value::Null,
            })),
        };
        let main_command = TaskCommand::Custom {
            ipc_method: "tcpoll3.main".into(),
            params: Value::Null,
            poll: None,
        };
        let err = validate_poll_strategy_refs(&main_command, &on_failure).unwrap_err();
        assert!(err.contains("tcpoll3/does-not-exist"));
    }

    #[test]
    fn inline_poll_spec_and_no_poll_are_unaffected() {
        let inline_cmd = TaskCommand::Custom {
            ipc_method: "tcpoll4.spawn".into(),
            params: Value::Null,
            poll: Some(PollSpecRef::Inline(tasty_agent::PollSpec {
                poll_method: "tcpoll4.wait".into(),
                map_from_response: Default::default(),
                map_from_request: Default::default(),
                state_field: "state".into(),
                terminal_states: vec!["done".into()],
                failure_states: vec![],
                interval_ms: 500,
                timeout_ms: None,
            })),
        };
        assert!(validate_poll_strategy_refs(&inline_cmd, &OnFailure::Abort).is_ok());

        let no_poll_cmd = TaskCommand::Custom {
            ipc_method: "tcpoll4.spawn".into(),
            params: Value::Null,
            poll: None,
        };
        assert!(validate_poll_strategy_refs(&no_poll_cmd, &OnFailure::Abort).is_ok());
    }
}

#[cfg(test)]
mod graph_edge_tests {
    use super::*;
    use crate::core::agent::graph_view::GraphEdge;

    fn task(id: &str, name: &str, state: TaskState) -> Task {
        Task {
            id: id.to_string(),
            workspace_id: 1,
            name: name.to_string(),
            command: TaskCommand::Run {
                command: vec!["true".to_string()],
                workspace_id: 1,
                cwd: None,
            },
            depends_on: Vec::new(),
            state,
            created_at: 0,
            started_at: None,
            finished_at: None,
            result: None,
            on_failure: OnFailure::Abort,
            metadata: Value::Null,
            reserved_for_fallback: false,
        }
    }

    /// `agent.dag_get` 은 DAG 부분집합만 그래프로 넘긴다 — explicit 그룹
    /// (`metadata.dag`)이 그룹 밖 task 를 `depends_on` 하는 정상 케이스에서
    /// `detect_cycles()` 가 `UnknownDependency` 를 내는데, 그건 사이클이 아니다.
    /// 이걸 응답 `cycle` 에 실으면 `dag.has_cycle:false` 와 모순되는 응답이 나가
    /// 소비 화면이 멀쩡한 DAG 에 "사이클 감지" 를 띄운다. 같은 시나리오를
    /// `crates/tasty-agent/src/task/tests.rs` 의
    /// `explicit_group_with_outside_dependency_is_not_a_cycle` 가 `has_cycle` 쪽에서
    /// 못박고 있고, 이 테스트가 응답 `cycle` 쪽 짝을 맞춘다.
    #[test]
    fn subset_cycle_ignores_dependency_pointing_outside_the_subset() {
        let mut inside = task("t-in", "in", TaskState::Waiting);
        inside.depends_on = vec!["t-out".to_string()];
        let subset = [inside];
        // 좁히지 않은 원본은 실제로 에러를 낸다 — 이 테스트가 공회전이 아님을 못박는다.
        assert!(matches!(
            TaskGraph::build(&subset).detect_cycles(),
            Err(AgentError::UnknownDependency(_))
        ));
        // `t-out` 은 이 부분집합에 없다(= 다른 DAG 소속) — 사이클이 아니다.
        assert!(subset_cycle(&subset).is_none());
    }

    /// 반대로 진짜 사이클은 그대로 실려야 한다.
    #[test]
    fn subset_cycle_reports_a_real_cycle() {
        let mut a = task("t-a", "a", TaskState::Waiting);
        a.depends_on = vec!["t-b".to_string()];
        let mut b = task("t-b", "b", TaskState::Waiting);
        b.depends_on = vec!["t-a".to_string()];
        let cycle = subset_cycle(&[a, b]).expect("cycle detected");
        assert!(matches!(cycle, AgentError::DependencyCycle(_)));
    }

    /// Materialized inline fallback: the fallback task's `metadata.fallback_of` names the main
    /// task. Both `--format dot`/`--format json` render this as a `main -> fallback` edge —
    /// exercised live against a real runner-driven failure via
    /// `tasty agent task-graph --workspace-id N --format dot` (see task report), this test locks
    /// the same shape in at the unit level.
    #[test]
    fn inline_fallback_materialized_produces_main_to_fallback_edge() {
        let main = task(
            "main",
            "main-with-inline-fb",
            TaskState::Failed {
                error: "boom".to_string(),
            },
        );
        let mut fallback = task("fb", "inline-fallback", TaskState::Succeeded);
        fallback.metadata = json!({"fallback_of": "main"});
        let tasks = [main, fallback];
        let edges = collect_graph_edges(&tasks);
        assert_eq!(
            edges.len(),
            1,
            "expected exactly one edge, got: {edges:?}",
            edges = edges_debug(&edges)
        );
        assert_eq!(edges[0].from, "main");
        assert_eq!(edges[0].to, "fb");
        assert_eq!(edges[0].kind, "fallback");
    }

    /// Before the main task fails, `TaskStore::create` hasn't materialized the fallback task yet
    /// — it simply doesn't exist in the task list. No edge to draw, and none should appear.
    #[test]
    fn inline_fallback_not_yet_materialized_has_no_edge() {
        let main = task("main", "main-with-inline-fb", TaskState::Ready);
        let edges = collect_graph_edges(std::slice::from_ref(&main));
        assert!(
            edges.is_empty(),
            "expected no edges, got: {edges:?}",
            edges = edges_debug(&edges)
        );
    }

    /// `fallback_of` pointing at a main task id that's no longer in the task list (deleted,
    /// possible since `referenced_task_ids` — the store's own creation/deletion integrity check
    /// — deliberately excludes this inline reverse-pointer) must not panic and must simply omit
    /// the edge.
    #[test]
    fn inline_fallback_with_deleted_main_is_skipped_without_panic() {
        let mut fallback = task("fb", "inline-fallback", TaskState::Succeeded);
        fallback.metadata = json!({"fallback_of": "main-no-longer-exists"});
        let tasks = [fallback];
        let edges = collect_graph_edges(&tasks);
        assert!(
            edges.is_empty(),
            "expected no edges, got: {edges:?}",
            edges = edges_debug(&edges)
        );
    }

    /// Regression guard for the three pre-existing edge kinds `collect_graph_edges` also now
    /// owns (previously hand-duplicated per dot/json branch): `depends_on`, an explicit
    /// `Fallback{task}`, and `Reduce.inputs`. Also live-verified (see task report) alongside the
    /// inline-fallback scenario in the same running workspace, confirming no regression.
    #[test]
    fn depends_on_explicit_fallback_and_reduce_edges_are_unaffected() {
        let a = task("a", "dep-a", TaskState::Succeeded);
        let mut b = task("b", "dep-b", TaskState::Succeeded);
        b.depends_on = vec!["a".to_string()];

        let fb_target = task("fbtarget", "existing-fb-target", TaskState::Succeeded);
        let mut main = task(
            "main2",
            "main-with-existing-fb",
            TaskState::Failed {
                error: "boom".to_string(),
            },
        );
        main.on_failure = OnFailure::Fallback {
            task: Some("fbtarget".to_string()),
            inline: None,
        };

        let mut reducer = task("reduce", "reduce-ab", TaskState::Succeeded);
        reducer.command = TaskCommand::Reduce {
            inputs: vec!["a".to_string(), "b".to_string()],
            strategy: ReducerStrategy::All,
        };

        let tasks = vec![a, b, fb_target, main, reducer];
        let edges = collect_graph_edges(&tasks);

        let has = |from: &str, to: &str, kind: &str| {
            edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.kind == kind)
        };
        assert!(
            has("a", "b", "depends_on"),
            "got: {edges:?}",
            edges = edges_debug(&edges)
        );
        assert!(
            has("main2", "fbtarget", "fallback"),
            "got: {edges:?}",
            edges = edges_debug(&edges)
        );
        assert!(
            has("a", "reduce", "reduce"),
            "got: {edges:?}",
            edges = edges_debug(&edges)
        );
        assert!(
            has("b", "reduce", "reduce"),
            "got: {edges:?}",
            edges = edges_debug(&edges)
        );
        assert_eq!(
            edges.len(),
            4,
            "got: {edges:?}",
            edges = edges_debug(&edges)
        );
    }

    fn edges_debug(edges: &[GraphEdge<'_>]) -> Vec<(String, String, &'static str)> {
        edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone(), e.kind))
            .collect()
    }
}

#[cfg(test)]
mod state_filter_tests {
    use super::*;

    fn task(id: &str, state: TaskState) -> Task {
        Task {
            id: id.to_string(),
            workspace_id: 1,
            name: id.to_string(),
            command: TaskCommand::Run {
                command: vec!["true".to_string()],
                workspace_id: 1,
                cwd: None,
            },
            depends_on: Vec::new(),
            state,
            created_at: 0,
            started_at: None,
            finished_at: None,
            result: None,
            on_failure: OnFailure::Abort,
            metadata: Value::Null,
            reserved_for_fallback: false,
        }
    }

    fn sample() -> Vec<Task> {
        vec![
            task("w", TaskState::Waiting),
            task("r", TaskState::Ready),
            task("run", TaskState::Running),
            task("ok", TaskState::Succeeded),
        ]
    }

    fn filtered(params: &Value) -> Vec<String> {
        let mut tasks = sample();
        retain_by_state(&mut tasks, state_names_param(params, "state").as_deref());
        tasks.into_iter().map(|t| t.id).collect()
    }

    /// 단일 값은 예전 그대로 — 1개짜리 목록으로 취급된다.
    #[test]
    fn single_state_string_filters_one_state() {
        assert_eq!(filtered(&json!({"state": "running"})), vec!["run"]);
    }

    /// 이 수정의 핵심: 콤마로 이어붙인 여러 state 를 OR 로 매칭한다. 예전에는
    /// `"waiting,ready,running"` 전체가 하나의 상태 이름으로 비교돼 결과가 항상
    /// 비었고, "아직 안 끝난 task 가 있는가" 판정이 통째로 무력화됐다.
    #[test]
    fn comma_separated_states_match_any() {
        assert_eq!(
            filtered(&json!({"state": "waiting,ready,running"})),
            vec!["w", "r", "run"]
        );
        // 공백이 섞여도 동일.
        assert_eq!(
            filtered(&json!({"state": " waiting , running "})),
            vec!["w", "run"]
        );
    }

    /// 배열 형태(`task_purge --states` 가 보내는 모양)도 그대로 받는다.
    #[test]
    fn array_states_match_any() {
        assert_eq!(
            filtered(&json!({"state": ["ready", "succeeded"]})),
            vec!["r", "ok"]
        );
    }

    /// 매칭되는 state 가 하나도 없으면 빈 목록. 필터 없음(전체 반환)과 구분된다.
    #[test]
    fn no_match_yields_empty_list() {
        assert!(filtered(&json!({"state": "failed,cancelled"})).is_empty());
    }

    /// 필터 미지정 / 빈 값은 "필터 없음" — 전체를 그대로 반환한다.
    #[test]
    fn absent_or_empty_filter_keeps_everything() {
        for params in [
            json!({}),
            json!({"state": Value::Null}),
            json!({"state": ""}),
            json!({"state": ","}),
            json!({"state": []}),
        ] {
            assert_eq!(filtered(&params).len(), 4, "params: {params}");
        }
    }

    /// 단수/복수 키를 서로 폴백한다 — `task_list` 에 `states`, `task_purge` 에
    /// `state` 를 보내도 같은 필터로 해석된다.
    #[test]
    fn singular_and_plural_keys_are_interchangeable() {
        let p = json!({"states": "ready,running"});
        assert_eq!(
            state_names_param(&p, "state"),
            Some(vec!["ready".to_string(), "running".to_string()])
        );
        let p = json!({"state": "succeeded"});
        assert_eq!(
            state_names_param(&p, "states"),
            Some(vec!["succeeded".to_string()])
        );
    }
}

/// `task_purge` 의 상태 필터는 `task_list` 와 "빈 값" 의 의미가 다르다 —
/// 조회는 빈 값을 "필터 없음"(전체)으로 접어도 무해하지만, 삭제에서 그렇게
/// 접으면 "상태를 하나도 안 골랐다" 가 "상태 무관 전체 삭제" 로 뒤집힌다.
/// 여기서 파라미터 파싱(`purge_filter_from_params`) + 후보 선정(`plan_sweep`)
/// 을 실제로 조합해 그 경계를 락인한다.
#[cfg(test)]
mod purge_state_filter_tests {
    use std::sync::atomic::AtomicU64;

    use tasty_agent::TaskStore;
    use tasty_agent::task::TaskSweepPlan;
    use tasty_memory::MemoryStorage;

    use super::*;

    /// Ready task 2 개를 `created_at = 1000` 으로 만들어두고, 주어진 purge
    /// 파라미터로 (now_ms = 100_000 기준) 후보가 어떻게 잡히는지 계산한다.
    fn plan(params: &Value) -> TaskSweepPlan {
        let mut mem = tasty_memory::testing::InMemoryStorage::new();
        let seq = AtomicU64::new(0);
        let mem_dyn: &mut dyn MemoryStorage = &mut mem;
        let mut store = TaskStore::new(mem_dyn, "_host", &seq);
        for name in ["a", "b"] {
            store
                .create(TaskCreateOpts {
                    workspace_id: 1,
                    name: name.to_string(),
                    command: TaskCommand::Run {
                        command: vec!["true".to_string()],
                        workspace_id: 1,
                        cwd: None,
                    },
                    depends_on: Vec::new(),
                    on_failure: OnFailure::Abort,
                    metadata: Value::Null,
                    now_ms: 1000,
                })
                .expect("create");
        }
        let filter = purge_filter_from_params(params, 100_000).expect("픽스처 params 는 정상");
        store.plan_sweep(1, &filter).expect("plan_sweep")
    }

    /// 회귀 락인: `{"states": [], "older_than_ms": ...}` 는 아무것도 지우지
    /// 않는다. 빈 목록을 `None` 으로 접던 회귀에서는 상태 무관 전체가 후보가
    /// 되어 경과시간만으로 전부 삭제됐다.
    #[test]
    fn empty_states_array_purges_nothing() {
        let p = plan(&json!({"states": [], "older_than_ms": 1000}));
        assert!(
            p.deleted.is_empty() && p.retained.is_empty(),
            "빈 states 는 매칭 0 건이어야 한다: {p:?}"
        );
    }

    /// 단수 키의 빈 문자열도 동일 — CLI/plugin 이 상태를 안 고른 채 조립하면
    /// 이 모양이 나온다.
    #[test]
    fn empty_state_string_purges_nothing() {
        for params in [
            json!({"state": "", "older_than_ms": 1000}),
            json!({"state": ",", "older_than_ms": 1000}),
            json!({"states": [""], "older_than_ms": 1000}),
        ] {
            let p = plan(&params);
            assert!(
                p.deleted.is_empty() && p.retained.is_empty(),
                "params: {params} → {p:?}"
            );
        }
    }

    /// 반대편 경계: 키가 아예 없으면 예전대로 "상태 필터 없음" 이라
    /// `older_than_ms` 만으로 전부 후보가 된다. 위 케이스와의 차이가
    /// "값이 비었나" 가 아니라 "키가 있나" 임을 확정한다.
    #[test]
    fn absent_states_key_still_means_no_state_filter() {
        let p = plan(&json!({"older_than_ms": 1000}));
        assert_eq!(p.deleted.len(), 2, "{p:?}");
    }

    /// 실제 상태 이름이 있으면 평소대로 지운다(필터가 과하게 잠기지 않았는지).
    #[test]
    fn named_states_still_purge() {
        let p = plan(&json!({"states": ["ready"], "older_than_ms": 1000}));
        assert_eq!(p.deleted.len(), 2, "{p:?}");
        let p = plan(&json!({"states": ["failed"], "older_than_ms": 1000}));
        assert!(p.deleted.is_empty() && p.retained.is_empty(), "{p:?}");
    }

    /// 파싱 계층 자체의 구분 — 키 부재/null 은 `None`, 키가 있는데 이름이 0 개면
    /// `Some(vec![])`. `task_list` 가 쓰는 `state_names_param` 은 둘 다 `None`
    /// 으로 접는(= 필터 없음) 기존 동작 그대로다.
    #[test]
    fn keep_empty_variant_preserves_key_presence() {
        assert_eq!(state_names_param_keep_empty(&json!({}), "states"), None);
        assert_eq!(
            state_names_param_keep_empty(&json!({"states": Value::Null}), "states"),
            None
        );
        assert_eq!(
            state_names_param_keep_empty(&json!({"states": []}), "states"),
            Some(Vec::new())
        );
        assert_eq!(
            state_names_param_keep_empty(&json!({"state": ""}), "states"),
            Some(Vec::new())
        );
        assert_eq!(
            state_names_param_keep_empty(&json!({"states": ["ready"]}), "states"),
            Some(vec!["ready".to_string()])
        );
        // 조회 경로는 그대로 접는다.
        assert_eq!(state_names_param(&json!({"states": []}), "state"), None);
    }
}
