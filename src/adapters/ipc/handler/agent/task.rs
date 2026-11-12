use std::collections::BTreeSet;

use crate::adapters::ipc::handler::params::{self, p_try};
use crate::runtime::engine_access::EngineMut;
use serde_json::{Value, json};

use crate::app::services::AppServices;
use tasty_agent::task::{
    Completion, CompletionOutcome, TaskCreateOpts, TaskDeleteOpts, TaskPurgeFilter,
};
use tasty_agent::{
    DispatchHandle, OnFailure, PollSpecRef, ReducerStrategy, Task, TaskCommand, TaskId, TaskResult,
    TaskState, extract_paths, reduce_with_custom, run_custom_shell,
};
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_task_runtime::task::{task_phase, task_phase_with_handle};

use super::super::memory::mark_durability;
use super::task_graph_view::{dag_response, dag_summary_json, graph_response};
use super::{agent_err_to_response, now_ms, reply_task, task_id_param, workspace_id_param};

pub fn handle_task_create(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    // 다른 작업의 fallback으로 등록하기 전 실행되지 않도록 Waiting 상태로 예약한다.
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_create(&scope, opts.clone(), reserved_for_fallback),
        |core, id, task| {
            mark_durability(
                core,
                match serde_json::to_value(&task) {
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
            )
        },
    )
}

/// Fallback은 작업 자체의 실행 실패에만 적용된다. 의존성이 실패하면 자기 fallback을
/// 실행하지 않고 Waiting에 남으므로, 생성은 허용하되 이 차이를 경고한다.
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

/// 실행을 시작하기 전에 poll 전략 이름을 검증한다. 인라인 fallback에도 적용한다.
/// 다른 작업 ID의 존재 여부는 핸들러를 거치지 않는 호출에도 적용되도록 TaskStore에서 검사한다.
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

/// 출력 참조의 작업이 선언된 의존성에 있어야 결과가 준비된 뒤 실행할 수 있다.
/// 의존성을 자동 추가하지 않고 누락을 거절한다. 문법 검사도 실행 시 치환과 같은 파서를 쓴다.
fn validate_task_output_refs(
    command: &TaskCommand,
    depends_on: &[TaskId],
    on_failure: &OnFailure,
) -> Result<(), String> {
    use tasty_task_runtime::task_output_ref;

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
    // 인라인 fallback은 별도 작업이다. override가 없으면 본 작업의 의존성을 물려받는다.
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
    if let TaskCommand::Custom { poll: Some(p), .. } = command
        && let PollSpecRef::Named { strategy } = p.as_ref()
    {
        let id = crate::completion_strategy::CompletionStrategyId::new(strategy.clone());
        crate::completion_strategy::global()
            .resolve_poll_spec(&id)
            .map_err(|e| format!("poll strategy '{strategy}': {e}"))?;
    }
    Ok(())
}

/// 배열·콤마 구분 문자열·단일 문자열을 받고 state/states를 서로 별칭으로 쓴다.
/// 조회에서 빈 값은 필터 없음이다. 삭제는 빈 값을 보존하는 state_names_param_keep_empty를 쓴다.
fn state_names_param(params: &Value, primary_key: &str) -> Option<Vec<String>> {
    let names = state_names_param_keep_empty(params, primary_key)?;
    (!names.is_empty()).then_some(names)
}

/// 키 부재나 null은 필터 없음, 빈 배열·문자열은 매칭 없음으로 구분한다.
/// 잘못된 타입도 매칭 없음으로 처리한다. 빈 선택을 전체 삭제로 해석하지 않기 위해서다.
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
    core: &AppServices,
    engine: &mut EngineMut<'_>,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let state_filter = state_names_param(params, "state");

    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| {
            let list = tx.task_list(&scope, workspace_id)?;
            let handles = tx.stored_handle_ids(workspace_id);
            let runner = tx.runner_status(&scope, workspace_id);
            Ok((list, handles, runner))
        },
        move |_, id, (mut tasks, handles, runner)| {
            retain_by_state(&mut tasks, state_filter.as_deref());
            let rows: Vec<Value> = tasks.iter().map(|t| task_with_phase(t, &handles)).collect();
            JsonRpcResponse::success(
                id,
                json!({
                    "total": rows.len(),
                    "tasks": rows,
                    "runner": runner_status_value(&runner),
                }),
            )
        },
    )
}

/// 목록의 task 하나. `task_get` 과 같은 `phase` 를 싣는다([`task_phase`]).
fn task_with_phase(task: &Task, handles: &std::collections::HashSet<TaskId>) -> Value {
    let mut v = serde_json::to_value(task).unwrap_or(Value::Null);
    if let (Some(phase), Some(obj)) = (task_phase(task, handles), v.as_object_mut()) {
        obj.insert("phase".to_string(), Value::from(phase));
    }
    v
}

/// 러너가 꺼져 있어도 저장소를 조회해 실제 작업 수를 반환한다.
/// 조회 실패 시 카운트는 0이 아니라 null이며 store_error에 원인이 담긴다.
/// list_failures는 러너의 연속 조회 실패 횟수다. 스레드가 살아 있어도 작업이 진행되지 않을 수 있다.
fn runner_status_value(status: &tasty_task_runtime::RunnerStatus) -> Value {
    json!({
        "running": status.running,
        "crashed": status.crashed,
        "state": status.state(),
        "ready_count": status.ready_count,
        "running_count": status.running_count,
        "store_error": status.store_error,
        "list_failures": status.list_failures,
    })
}

/// AwaitExternal은 상태만 보면 Running이므로 대기 중인 신호와 기한을 함께 반환한다.
fn awaiting_external_json(handle: Option<DispatchHandle>) -> Option<Value> {
    match handle? {
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

pub fn handle_task_get(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| {
            let Some((t, revision)) = tx.task_get_with_revision(&scope, workspace_id, &task_id)?
            else {
                return Ok(Err(task_id.clone()));
            };
            let has_handle = tx.has_stored_handle(workspace_id, &task_id);
            let holding_warnings = tx.task_holding_warnings(&scope, workspace_id, &task_id);
            let handle = if matches!(t.state, TaskState::Running) {
                tx.dispatch_handle(workspace_id, &task_id)
            } else {
                None
            };
            Ok(Ok(StoredTask {
                task: t,
                revision,
                has_handle,
                holding_warnings,
                handle,
            }))
        },
        |_, id, found| match found {
            Err(task_id) => {
                JsonRpcResponse::error(id, -32004, format!("task not found: {task_id}"))
            }
            Ok(stored) => stored_task_response(id, stored),
        },
    )
}

/// `task_get` 이 writer 순서에서 한 번에 읽은 값.
struct StoredTask {
    task: Task,
    revision: Option<u64>,
    has_handle: bool,
    holding_warnings: Vec<Value>,
    handle: Option<DispatchHandle>,
}

fn stored_task_response(id: Value, stored: StoredTask) -> JsonRpcResponse {
    let StoredTask {
        task: t,
        revision,
        has_handle,
        holding_warnings,
        handle,
    } = stored;
    {
        {
            let phase = task_phase_with_handle(&t, has_handle);
            let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
            // 러너가 TTL 을 갱신하지 못해 lease·permit 을 잃은 기록. 없으면 키를 싣지 않는다.
            if !holding_warnings.is_empty()
                && let Some(obj) = v.as_object_mut()
            {
                obj.insert(
                    "holding_warnings".to_string(),
                    Value::from(holding_warnings),
                );
            }
            if let (Some(phase), Some(obj)) = (phase, v.as_object_mut()) {
                obj.insert("phase".to_string(), Value::from(phase));
            }
            if let (Some(revision), Some(obj)) = (revision, v.as_object_mut()) {
                obj.insert("revision".to_string(), Value::from(revision));
            }
            if let Some(obj) = v.as_object_mut()
                && let Some(info) = awaiting_external_json(handle)
            {
                obj.insert("awaiting_external".to_string(), info);
            }
            JsonRpcResponse::success(id, v)
        }
    }
}

pub fn handle_task_cancel(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_cancel(&scope, workspace_id, &task_id, now_ms()),
        |core, id, (task, cascaded)| {
            mark_durability(
                core,
                JsonRpcResponse::success(
                    id,
                    json!({
                        "task": task,
                        "cascaded": cascaded,
                    }),
                ),
            )
        },
    )
}

pub fn handle_task_retry(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_retry(&scope, workspace_id, &task_id, reset_downstream, now_ms()),
        |core, id, task| {
            mark_durability(
                core,
                match serde_json::to_value(task) {
                    Ok(v) => JsonRpcResponse::success(id, v),
                    Err(e) => JsonRpcResponse::error(id, -32603, format!("serialize: {e}")),
                },
            )
        },
    )
}

/// 생략 시 기본 대기 시간은 10분이다. 측정으로 정한 값은 아니므로 사용 경험에 따라 재검토한다.
/// timeout_ms를 명시적으로 0으로 지정하면 무한 대기한다.
pub const DEFAULT_TASK_AWAIT_TIMEOUT_MS: u64 = 600_000;

pub fn await_task_blocking(
    awaiter: &tasty_task_runtime::TaskAwaiter,
    rpc_id: Value,
    params: &Value,
) -> JsonRpcResponse {
    use tasty_task_runtime::task_waker::AwaitOutcome;

    let workspace_id = match workspace_id_param(params, &rpc_id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &rpc_id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let timeout_ms = Some(
        p_try!(params::opt_int::<u64>(params, "timeout_ms", &rpc_id))
            .unwrap_or(DEFAULT_TASK_AWAIT_TIMEOUT_MS),
    );

    let outcome = awaiter.await_terminal(workspace_id, &task_id, timeout_ms);
    match outcome {
        AwaitOutcome::Terminal(snap) => {
            let mut resp = json!({
                "outcome": "terminal",
                "state": snap.state.name(),
            });
            if let Some(r) = snap.result {
                resp["result"] = serde_json::to_value(r).unwrap_or(Value::Null);
            }
            if let Some(attempt_id) = snap.attempt_id {
                resp["attempt_id"] = Value::from(attempt_id);
            }
            if let Some(revision) = snap.revision {
                resp["revision"] = Value::from(revision);
            }
            if let Some(skip) = snap.skip {
                resp["skip"] = serde_json::to_value(skip).unwrap_or(Value::Null);
            }
            JsonRpcResponse::success(rpc_id, resp)
        }
        AwaitOutcome::TimedOut => {
            JsonRpcResponse::success(rpc_id, json!({ "outcome": "timed_out" }))
        }
        AwaitOutcome::NotFound => {
            JsonRpcResponse::success(rpc_id, json!({ "outcome": "not_found" }))
        }
    }
}

/// 요청 workspace를 가진 engine이 없으면 다른 허브에서 기다리지 않고 라우터와 같은 대상 없음 오류로 답한다.
pub(crate) fn unowned_await_workspace(rpc_id: Value, workspace_id: u32) -> JsonRpcResponse {
    JsonRpcResponse::invalid_params(
        rpc_id,
        crate::core::request_target::unowned_target_message(
            crate::core::request_target::ResourceId {
                kind: crate::core::request_target::Kind::Workspace,
                id: u64::from(workspace_id),
            },
            "agent.task_await",
        ),
    )
}

/// 메인 루프를 막지 않도록 워커에서 완료를 기다린다.
/// GUI와 헤드리스의 engine 선택 방식이 달라, 호출자가 소유 engine의 대기 계약을 먼저 고른다.
pub(crate) fn spawn_task_await(
    awaiter: tasty_task_runtime::TaskAwaiter,
    rpc_id: Value,
    params: Value,
    response_tx: &std::sync::mpsc::SyncSender<JsonRpcResponse>,
) {
    let response_tx = response_tx.clone();
    std::thread::spawn(move || {
        let resp = await_task_blocking(&awaiter, rpc_id, &params);
        tasty_ipc::server::send_response(&response_tx, resp);
    });
}

pub fn handle_task_graph(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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

    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| {
            let list = tx.task_list(&scope, workspace_id)?;
            Ok((list, tx.runner_status(&scope, workspace_id)))
        },
        move |_, id, (tasks, runner)| {
            graph_response(id, &format, &tasks, runner_status_value(&runner))
        },
    )
}

/// workspace_id를 생략하면 현재 모든 workspace를 조회한다. 잘못된 타입은 거절한다.
fn optional_workspace_id_param(params: &Value, id: &Value) -> Result<Option<u32>, JsonRpcResponse> {
    crate::adapters::ipc::handler::params::optional_u32(params, "workspace_id", id)
}

pub fn handle_dag_list(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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

    let scope = engine.task_scope.clone();
    let workspaces = crate::app::task_completion::dag_workspaces(
        engine
            .workspaces()
            .into_iter()
            .map(|workspace| workspace.id),
        workspace_id,
    );
    reply_task(
        core,
        id,
        params,
        move |tx| tx.dag_list(&scope, &workspaces),
        move |_, id, dags: Vec<tasty_agent::DagSummary>| {
            let rendered: Vec<Value> = dags
                .iter()
                .map(|d| dag_summary_json(d, include_tasks))
                .collect();
            JsonRpcResponse::success(
                id,
                json!({
                    "total": rendered.len(),
                    "dags": rendered,
                    // 삭제된 workspace에 남은 작업은 이 목록에 포함되지 않는다.
                    "scope": "live_workspaces",
                }),
            )
        },
    )
}

pub fn handle_dag_get(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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

    let scope = engine.task_scope.clone();
    let workspaces = crate::app::task_completion::dag_workspaces(
        engine
            .workspaces()
            .into_iter()
            .map(|workspace| workspace.id),
        workspace_id,
    );
    reply_task(
        core,
        id,
        params,
        move |tx| {
            Ok(match tx.dag_get(&scope, &workspaces, &dag_id)? {
                Some((dag, list)) => {
                    let runner = tx.runner_status(&scope, dag.workspace_id);
                    Ok((dag, list, runner))
                }
                None => Err(dag_id.clone()),
            })
        },
        move |_, id, found| match found {
            Err(dag_id) => JsonRpcResponse::invalid_params(id, format!("unknown dag id: {dag_id}")),
            Ok((dag, tasks, runner)) => {
                dag_response(id, &format, &dag, &tasks, runner_status_value(&runner))
            }
        },
    )
}

pub fn handle_task_reduce(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let extract_path = params
        .get("extract_path")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_reduce_collect(&scope, workspace_id, &inputs),
        move |_, id, collected| {
            // 경로가 없는 입력도 버리지 않고 null과 경고로 남긴 뒤 나머지를 합친다.
            let (collected, warnings) = extract_paths(&collected, extract_path.as_deref());

            // custom shell의 입출력 동안 저장소 잠금을 잡지 않는다.
            let result = reduce_with_custom(&strategy, &collected, run_custom_shell);
            match result {
                Ok(value) => {
                    JsonRpcResponse::success(id, json!({ "value": value, "warnings": warnings }))
                }
                Err(e) => agent_err_to_response(id, e),
            }
        },
    )
}

/// workspace 러너를 시작·중지하거나 상태를 조회한다. 이미 시작/중지된 경우는 그대로 둔다.
/// stop 은 정지를 요청하고 바로 응답한다. 러너가 아직 끝나지 않았으면 `state` 가 `stopping` 이다.
/// 상태 조회 실패 시 카운트는 null이고 store_error에 원인을 담는다.
pub fn handle_task_run(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    match action {
        "start" => {
            core.tasks.runner_start(engine.task_scope, workspace_id);
        }
        "stop" => {
            // 메인 스레드에서 러너 join 을 기다리지 않는다. 러너 tick 은 Custom dispatch 마다 메인 스레드의
            // 응답을 기다리므로, 여기서 기다리면 서로를 기다린다. poll 은 끝난 worker 만 join 하는
            // 비차단 확인이라, 이미 끝난 worker(crash 포함)는 여기서 회수되어 응답이 stopped 가 된다.
            // 아직 도는 worker 는 끝날 때까지 state "stopping" 이고 메인 루프의 poll_runner_stops 가 회수한다.
            core.tasks
                .request_stop_workspace(engine.task_scope, workspace_id)
                .poll();
        }
        "status" => {}
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'action': {other} (expected start|stop|status)"),
            );
        }
    }
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| Ok(tx.runner_status(&scope, workspace_id)),
        |_, id, status| JsonRpcResponse::success(id, runner_status_value(&status)),
    )
}

/// 외부에서 작업 결과를 보고하는 진입점. 러너도 같은 완료 경로([`TaskStore::complete`])를 쓴다.
/// 결과와 종결을 한 번에 기록하므로 인자가 틀리면 아무것도 쓰지 않는다. 같은 회차에 같은 보고를
/// 다시 내면 `duplicate: true` 로 같은 응답을 돌려준다.
///
/// [`TaskStore::complete`]: tasty_agent::TaskStore::complete
pub fn handle_task_set_result(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let attempt_id = match params.get("attempt_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(a)) => Some(a.clone()),
        Some(_) => return JsonRpcResponse::invalid_params(id, "'attempt_id' must be a string"),
    };
    let outcome = match state_str.as_str() {
        "succeeded" => CompletionOutcome::Succeeded,
        "failed" => CompletionOutcome::Failed {
            error: error.clone().unwrap_or_else(|| "(unspecified)".to_string()),
        },
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'state': {other} (expected 'succeeded' or 'failed')"),
            );
        }
    };
    let completion = Completion {
        attempt_id,
        result: TaskResult {
            exit_code,
            output,
            error,
        },
        outcome,
        // 외부 보고는 본 작업의 결과다. 후처리가 있으면 이 보고 뒤에 후처리가 실행된다.
        postprocess: None,
        main_copy_dropped: false,
    };

    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_complete(&scope, workspace_id, &task_id, completion.clone(), now_ms()),
        |core, id, receipt| {
            mark_durability(
                core,
                JsonRpcResponse::success(
                    id,
                    json!({
                        "task": receipt.task,
                        "cascaded": receipt.transitioned,
                        "duplicate": receipt.duplicate,
                    }),
                ),
            )
        },
    )
}

/// 참조가 있으면 거절하고 error.data.referenced_by에 참조자를 반환한다.
/// cascade는 참조자를 함께 지우고 force는 참조 검사만 생략한다. Running 삭제는 항상 거절한다.
pub fn handle_task_delete(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| {
            tx.task_delete(
                &scope,
                workspace_id,
                &task_id,
                TaskDeleteOpts { cascade, force },
            )
        },
        |core, id, report| {
            mark_durability(
                core,
                JsonRpcResponse::success(id, json!({ "deleted": report.deleted })),
            )
        },
    )
}

/// 빈 states는 매칭 없음으로 보존한다. 잘못된 older_than_ms는 거절한다.
/// 두 값을 필터 없음으로 처리하면 의도보다 넓은 범위를 지울 수 있다.
fn purge_filter_from_params(params: &Value, now_ms: u64) -> Result<TaskPurgeFilter, String> {
    Ok(TaskPurgeFilter {
        states: state_names_param_keep_empty(params, "states"),
        older_than_ms: params::read_int::<u64>(params, "older_than_ms")?,
        now_ms,
    })
}

/// 상태·경과시간 필터로 고른 작업 중 참조 검사와 Running 제외 조건을 만족하는 것만 지운다.
/// 두 필터를 모두 생략하면 거절한다. dry_run은 deleted/retained 계획만 반환한다.
pub fn handle_task_purge(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
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
    let scope = engine.task_scope.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_purge(&scope, workspace_id, filter.clone(), dry_run),
        move |core, id, plan| {
            mark_durability(
                core,
                JsonRpcResponse::success(
                    id,
                    json!({
                        "deleted": plan.deleted,
                        "retained": plan.retained,
                        "skipped_count": plan.skipped.len(),
                        "skipped": plan.skipped,
                        "dry_run": dry_run,
                    }),
                ),
            )
        },
    )
}

#[cfg(test)]
mod poll_strategy_ref_tests {
    use super::*;
    use tasty_plugin_protocol::host_port::CompletionStrategyRegistryPort;

    // 전역 레지스트리를 다른 시험과 공유하므로 고유한 ID 접두어를 쓴다.
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

    fn tell_with(params: serde_json::Value) -> TaskCommand {
        TaskCommand::Custom {
            ipc_method: "claude.tell".into(),
            params,
            poll: None,
        }
    }

    #[test]
    fn task_create_rejects_output_ref_not_in_depends_on() {
        let command = tell_with(json!({
            "surface_id": "${task.t-a.output/child_surface_id}",
        }));
        let err = validate_task_output_refs(&command, &[], &OnFailure::Abort).unwrap_err();
        assert!(err.contains("t-a"), "{err}");
        assert!(err.contains("depends_on"), "{err}");
    }

    #[test]
    fn task_create_accepts_output_ref_declared_in_depends_on() {
        let command = tell_with(json!({
            "surface_id": "${task.t-a.output/child_surface_id}",
        }));
        assert!(
            validate_task_output_refs(&command, &["t-a".to_string()], &OnFailure::Abort).is_ok()
        );
    }

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

    #[test]
    fn task_create_accepts_output_ref_from_reduce_inputs() {
        // Reduce에는 치환할 값이 없지만 inputs를 의존성으로 허용하는지는 확인한다.
        let command = TaskCommand::Reduce {
            inputs: vec!["t-a".to_string()],
            strategy: tasty_agent::ReducerStrategy::ConcatText,
        };
        assert!(validate_task_output_refs(&command, &[], &OnFailure::Abort).is_ok());
    }

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
        let err =
            validate_task_output_refs(&tell_with(Value::Null), &["t-a".to_string()], &on_failure)
                .unwrap_err();
        assert!(err.contains("t-ghost"), "{err}");

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

    #[test]
    fn task_create_rejects_malformed_output_placeholder() {
        let command = tell_with(json!({ "x": "${task.t-a.ouput/id}" }));
        let err = validate_task_output_refs(&command, &["t-a".to_string()], &OnFailure::Abort)
            .unwrap_err();
        assert!(err.contains("malformed"), "{err}");
    }

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
            poll: Some(Box::new(PollSpecRef::Named {
                strategy: "tcpoll1/does-not-exist".into(),
            })),
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
            poll: Some(Box::new(PollSpecRef::Named {
                strategy: "tcpoll2/spawn-wait".into(),
            })),
        };
        assert!(validate_poll_strategy_refs(&command, &OnFailure::Abort).is_ok());
    }

    #[test]
    fn named_poll_ref_inside_inline_fallback_is_validated() {
        let bad_fallback = TaskCommand::Custom {
            ipc_method: "tcpoll3.spawn".into(),
            params: Value::Null,
            poll: Some(Box::new(PollSpecRef::Named {
                strategy: "tcpoll3/does-not-exist".into(),
            })),
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
            poll: Some(Box::new(PollSpecRef::Inline(tasty_agent::PollSpec {
                poll_method: "tcpoll4.wait".into(),
                map_from_response: Default::default(),
                map_from_request: Default::default(),
                state_field: "state".into(),
                terminal_states: vec!["done".into()],
                failure_states: vec![],
                interval_ms: 500,
                timeout_ms: None,
            }))),
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
    use super::super::task_graph_view::*;
    use super::*;
    use tasty_agent::{AgentError, TaskGraph};
    use tasty_task_runtime::graph_view::GraphEdge;
    use tasty_task_runtime::graph_view::{collect_graph_edges, drawn_edges};

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

    // 그룹 밖 의존성은 DAG의 사이클이 아니다. has_cycle과 cycle 응답이 일치해야 한다.
    #[test]
    fn subset_cycle_ignores_dependency_pointing_outside_the_subset() {
        let mut inside = task("t-in", "in", TaskState::Waiting);
        inside.depends_on = vec!["t-out".to_string()];
        let subset = [inside];
        assert!(matches!(
            TaskGraph::build(&subset).detect_cycles(),
            Err(AgentError::UnknownDependency(_))
        ));
        assert!(subset_cycle(&subset).is_none());
    }

    #[test]
    fn subset_cycle_reports_a_real_cycle() {
        let mut a = task("t-a", "a", TaskState::Waiting);
        a.depends_on = vec!["t-b".to_string()];
        let mut b = task("t-b", "b", TaskState::Waiting);
        b.depends_on = vec!["t-a".to_string()];
        let cycle = subset_cycle(&[a, b]).expect("cycle detected");
        assert!(matches!(cycle, AgentError::DependencyCycle(_)));
    }

    // 생성된 인라인 fallback은 metadata.fallback_of를 통해 원래 작업과 연결된다.
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

    // 작업이 실패하기 전에는 인라인 fallback이 아직 생성되지 않아 표시할 엣지도 없다.
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

    // fallback_of는 참조 무결성 검사 대상이 아니므로 원래 작업이 먼저 삭제될 수 있다.
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

    /// 전이 엣지는 대상마다 하나이며, 경로를 고른 뒤 선택 상태를 싣는다. 선택되지 않은
    /// 노드는 skip 이유를 함께 낸다.
    #[test]
    fn transition_edges_carry_their_selection() {
        let mut review = task("review", "review", TaskState::Waiting);
        review.contract = Some(
            serde_json::from_value(json!({"contract_version": 2,
                "transitions": {"cases": [
                    {"when": {"compare": {"path": "", "op": "eq", "value": 1}}, "to": ["ship"]},
                    {"when": {"compare": {"path": "", "op": "eq", "value": 2}}, "to": ["ship", "fix"]}],
                    "no_match": "finish"}}))
            .expect("contract"),
        );
        let ship = task("ship", "ship", TaskState::Waiting);
        let mut fix = task("fix", "fix", TaskState::Waiting);
        let pending = [review.clone(), ship.clone(), fix.clone()];
        let edges = render_graph_edges(&pending);
        assert_eq!(
            edges,
            vec![
                json!({"from": "review", "to": "ship", "kind": "transition", "selection": "pending"}),
                json!({"from": "review", "to": "fix", "kind": "transition", "selection": "pending"}),
            ]
        );

        review.state = TaskState::Succeeded;
        review.route = Some(tasty_agent::RouteDecision {
            attempt_id: Some("review#1".into()),
            matched: vec![0],
            otherwise: false,
            selected: vec!["ship".into()],
        });
        fix.state = TaskState::Skipped;
        fix.skip = Some(tasty_agent::SkipReason::BranchNotSelected);
        let decided = [review, ship, fix];
        let selections: Vec<Value> = render_graph_edges(&decided)
            .into_iter()
            .map(|e| e["selection"].clone())
            .collect();
        assert_eq!(selections, vec![json!("selected"), json!("not_selected")]);
        let nodes = render_graph_nodes(&decided);
        assert_eq!(nodes[2]["skip"], json!({"reason": "branch_not_selected"}));
        assert_eq!(nodes[1]["skip"], Value::Null);
        let dot = render_graph_dot(&decided);
        assert!(
            dot.contains(
                "\"fix\" [label=\"fix\\nnot selected\", style=\"filled,dashed\", fillcolor=lightgray];"
            ),
            "{dot}"
        );
        let mut cut = decided[2].clone();
        cut.skip = Some(tasty_agent::SkipReason::UpstreamUnavailable {
            source: "review".into(),
            source_state: "failed".into(),
        });
        assert!(
            render_graph_dot(std::slice::from_ref(&cut))
                .contains("\"fix\" [label=\"fix\\nskipped\", style=filled, fillcolor=lightgray];"),
            "an upstream skip keeps the skipped label"
        );
        assert!(
            dot.contains(
                "\"review\" -> \"fix\" [style=dashed, color=\"#6a5acd66\", fontcolor=gray, label=\"not_selected\"];"
            ),
            "{dot}"
        );
        assert!(
            dot.contains(
                "\"review\" -> \"ship\" [style=dashed, color=slateblue, penwidth=2, label=\"selected\"];"
            ),
            "{dot}"
        );
    }

    /// 같은 원본을 depends_on 과 binding 으로 함께 가리키면 binding 한 줄만 나온다. one_of 는
    /// 원본마다 한 줄이다.
    #[test]
    fn binding_replaces_depends_on_only_where_the_edge_is_drawn() {
        let a = task("a", "a", TaskState::Succeeded);
        let b = task("b", "b", TaskState::Succeeded);
        let c = task("c", "c", TaskState::Succeeded);
        let mut join = task("join", "join", TaskState::Waiting);
        join.depends_on = vec!["a".to_string(), "c".to_string()];
        join.contract = Some(
            serde_json::from_value(json!({"contract_version": 2,
                "input_schema": {"type": "object", "fields": {
                    "x": {"type": "json"}, "y": {"type": "json"}}},
                "bindings": {
                    "x": {"from_task": "a"},
                    "y": {"one_of": [{"from_task": "a"}, {"from_task": "b"}]}}}))
            .expect("contract"),
        );
        let tasks = [a, b, c, join];
        let want = |from: &str, kind: &'static str| (from.to_string(), "join".to_string(), kind);
        let mut edges = edges_debug(&collect_graph_edges(&tasks));
        edges.sort();
        assert_eq!(
            edges,
            vec![
                want("a", "binding"),
                want("a", "depends_on"),
                want("b", "binding"),
                want("c", "depends_on"),
            ],
            "the JSON edge list keeps both declarations"
        );
        let mut drawn = edges_debug(&drawn_edges(collect_graph_edges(&tasks)));
        drawn.sort();
        assert_eq!(
            drawn,
            vec![
                want("a", "binding"),
                want("b", "binding"),
                want("c", "depends_on"),
            ]
        );
        let dot = render_graph_dot(&tasks);
        assert!(
            dot.contains("\"a\" -> \"join\" [style=dashed, color=darkcyan, label=\"binding\"];"),
            "{dot}"
        );
        assert!(dot.contains("\"c\" -> \"join\";"), "{dot}");
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

    #[test]
    fn single_state_string_filters_one_state() {
        assert_eq!(filtered(&json!({"state": "running"})), vec!["run"]);
    }

    #[test]
    fn comma_separated_states_match_any() {
        assert_eq!(
            filtered(&json!({"state": "waiting,ready,running"})),
            vec!["w", "r", "run"]
        );
        assert_eq!(
            filtered(&json!({"state": " waiting , running "})),
            vec!["w", "run"]
        );
    }

    #[test]
    fn array_states_match_any() {
        assert_eq!(
            filtered(&json!({"state": ["ready", "succeeded"]})),
            vec!["r", "ok"]
        );
    }

    #[test]
    fn no_match_yields_empty_list() {
        assert!(filtered(&json!({"state": "failed,cancelled"})).is_empty());
    }

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

// 빈 상태 필터가 전체 삭제로 해석되지 않는지 실제 파싱과 plan_sweep을 함께 검사한다.
#[cfg(test)]
mod purge_state_filter_tests {
    use std::sync::atomic::AtomicU64;

    use tasty_agent::TaskStore;
    use tasty_agent::task::TaskSweepPlan;
    use tasty_memory::MemoryStorage;

    use super::*;

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
        store
            .plan_sweep(1, &filter, &Default::default())
            .expect("plan_sweep")
    }

    #[test]
    fn empty_states_array_purges_nothing() {
        let p = plan(&json!({"states": [], "older_than_ms": 1000}));
        assert!(
            p.deleted.is_empty() && p.retained.is_empty(),
            "빈 states 는 매칭 0 건이어야 한다: {p:?}"
        );
    }

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

    #[test]
    fn absent_states_key_still_means_no_state_filter() {
        let p = plan(&json!({"older_than_ms": 1000}));
        assert_eq!(p.deleted.len(), 2, "{p:?}");
    }

    #[test]
    fn named_states_still_purge() {
        let p = plan(&json!({"states": ["ready"], "older_than_ms": 1000}));
        assert_eq!(p.deleted.len(), 2, "{p:?}");
        let p = plan(&json!({"states": ["failed"], "older_than_ms": 1000}));
        assert!(p.deleted.is_empty() && p.retained.is_empty(), "{p:?}");
    }

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
        assert_eq!(state_names_param(&json!({"states": []}), "state"), None);
    }
}
