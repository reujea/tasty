//! `tasty agent ...` CLI → JsonRpcRequest 매핑.

use crate::commands::{AgentCommands, ReportCommands};
use crate::format::graph_table::ipc_format;

pub(super) fn agent_command_to_method_params(
    command: &AgentCommands,
) -> (&'static str, serde_json::Value) {
    use AgentCommands::*;
    match command {
        TaskCreate {
            workspace_id,
            name,
            command: cmd_spec,
            depends_on,
            on_failure,
            metadata,
            concurrency_limit,
            reserved_for_fallback,
        } => build_task_create_params(
            *workspace_id,
            name,
            cmd_spec,
            depends_on,
            on_failure,
            metadata.as_deref(),
            concurrency_limit.as_deref(),
            *reserved_for_fallback,
        ),
        TaskGraphSubmit {
            workspace_id,
            graph,
            dry_run,
        } => build_task_graph_params(*workspace_id, graph, *dry_run),
        TaskList {
            workspace_id,
            state,
            ..
        } => {
            let mut p = serde_json::json!({ "workspace_id": *workspace_id });
            // 구 호스트와의 단일값 호환을 위해 하나면 문자열, 여러 개면 배열로 보낸다.
            // 배열을 모르는 구 호스트는 다중값 필터를 적용하지 않고 전체를 반환한다.
            match state.len() {
                0 => {}
                1 => p["state"] = serde_json::Value::String(state[0].clone()),
                _ => {
                    p["state"] = serde_json::Value::Array(
                        state
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    );
                }
            }
            ("agent.task_list", p)
        }
        TaskGet {
            workspace_id, id, ..
        } => (
            "agent.task_get",
            serde_json::json!({ "workspace_id": *workspace_id, "id": id }),
        ),
        TaskAwait {
            workspace_id,
            id,
            timeout_ms,
        } => {
            let mut p = serde_json::json!({ "workspace_id": *workspace_id, "id": id });
            if let Some(t) = timeout_ms {
                p["timeout_ms"] = serde_json::Value::from(*t);
            }
            ("agent.task_await", p)
        }
        TaskCancel { workspace_id, id } => (
            "agent.task_cancel",
            serde_json::json!({ "workspace_id": *workspace_id, "id": id }),
        ),
        TaskRetry {
            workspace_id,
            id,
            reset_downstream,
        } => (
            "agent.task_retry",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "id": id,
                "reset_downstream": *reset_downstream,
            }),
        ),
        TaskGraph {
            workspace_id,
            format,
        } => (
            "agent.task_graph",
            serde_json::json!({ "workspace_id": *workspace_id, "format": ipc_format(format) }),
        ),
        DagList {
            workspace_id,
            include_tasks,
        } => {
            let mut p = serde_json::json!({ "include_tasks": *include_tasks });
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            ("agent.dag_list", p)
        }
        DagGet {
            id,
            workspace_id,
            format,
        } => {
            let mut p = serde_json::json!({ "id": id, "format": ipc_format(format) });
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            ("agent.dag_get", p)
        }
        DagReport {
            dag,
            workspace_id,
            task,
            attempt,
            include_raw,
        } => dag_report_params(dag, *workspace_id, task.as_deref(), *attempt, *include_raw),
        Report { command } => report_params(command),
        TaskRun {
            workspace_id,
            action,
        } => (
            "agent.task_run",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "action": action.as_str(),
            }),
        ),
        TaskSetResult {
            workspace_id,
            id,
            state,
            output,
            error,
            exit_code,
            attempt_id,
        } => (
            "agent.task_set_result",
            task_set_result_params(
                *workspace_id,
                id,
                state,
                output.as_deref(),
                error.as_deref(),
                *exit_code,
                attempt_id.as_deref(),
            ),
        ),
        TaskSubmit {
            workspace_id,
            id,
            attempt_id,
            token,
            output,
        } => (
            "agent.task_submit_result",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "id": id,
                "attempt_id": attempt_id,
                "token": token,
                "output": parse_inline_or_file_json(output, "--output"),
            }),
        ),
        TaskDelete {
            workspace_id,
            id,
            cascade,
            force,
        } => (
            "agent.task_delete",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "id": id,
                "cascade": *cascade,
                "force": *force,
            }),
        ),
        TaskPurge {
            workspace_id,
            states,
            older_than_ms,
            dry_run,
        } => {
            if states.is_empty() && older_than_ms.is_none() {
                crate::out::errln!("{}", tasty_i18n::t("cli.agent.purge_filter_required"));
                std::process::exit(1);
            }
            let mut p = serde_json::json!({
                "workspace_id": *workspace_id,
                "dry_run": *dry_run,
            });
            if !states.is_empty() {
                p["states"] = serde_json::Value::Array(
                    states
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                );
            }
            if let Some(t) = older_than_ms {
                p["older_than_ms"] = serde_json::Value::from(*t);
            }
            ("agent.task_purge", p)
        }
        BarrierCreate {
            workspace_id,
            name,
            count_required,
            timeout_ms,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": *workspace_id,
                "name": name,
                "count_required": *count_required,
            });
            if let Some(t) = timeout_ms {
                p["timeout_ms"] = serde_json::Value::from(*t);
            }
            ("agent.barrier_create", p)
        }
        BarrierSignal { workspace_id, name } => (
            "agent.barrier_signal",
            serde_json::json!({ "workspace_id": *workspace_id, "name": name }),
        ),
        BarrierAwait { workspace_id, name } => (
            "agent.barrier_await",
            serde_json::json!({ "workspace_id": *workspace_id, "name": name }),
        ),
        BarrierState { workspace_id, name } => (
            "agent.barrier_state",
            serde_json::json!({ "workspace_id": *workspace_id, "name": name }),
        ),
        SemaphoreCreate {
            workspace_id,
            name,
            permits,
        } => (
            "agent.semaphore_create",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "name": name,
                "permits": *permits,
            }),
        ),
        SemaphoreSetPermits {
            workspace_id,
            name,
            permits,
        } => (
            "agent.semaphore_set_permits",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "name": name,
                "permits": *permits,
            }),
        ),
        SemaphoreAcquire {
            workspace_id,
            name,
            holder,
            ttl_ms,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": *workspace_id,
                "name": name,
                "holder": holder,
            });
            if let Some(t) = ttl_ms {
                p["ttl_ms"] = serde_json::Value::from(*t);
            }
            ("agent.semaphore_acquire", p)
        }
        SemaphoreRelease {
            workspace_id,
            name,
            holder,
        } => (
            "agent.semaphore_release",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "name": name,
                "holder": holder,
            }),
        ),
        BarrierList { workspace_id } => (
            "agent.barrier_list",
            serde_json::json!({ "workspace_id": *workspace_id }),
        ),
        BarrierDelete { workspace_id, name } => (
            "agent.barrier_delete",
            serde_json::json!({ "workspace_id": *workspace_id, "name": name }),
        ),
        SemaphoreList { workspace_id } => (
            "agent.semaphore_list",
            serde_json::json!({ "workspace_id": *workspace_id }),
        ),
        SemaphoreDelete { workspace_id, name } => (
            "agent.semaphore_delete",
            serde_json::json!({ "workspace_id": *workspace_id, "name": name }),
        ),
        LeaseAcquire {
            workspace_id,
            resource,
            holder,
            ttl_ms,
            mode,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": *workspace_id,
                "resource": resource,
                "holder": holder,
                "mode": mode,
            });
            if let Some(t) = ttl_ms {
                p["ttl_ms"] = serde_json::Value::from(*t);
            }
            ("agent.lease_acquire", p)
        }
        LeaseRelease {
            workspace_id,
            resource,
            holder,
        } => (
            "agent.lease_release",
            serde_json::json!({
                "workspace_id": *workspace_id,
                "resource": resource,
                "holder": holder,
            }),
        ),
        LeaseList { workspace_id } => (
            "agent.lease_list",
            serde_json::json!({ "workspace_id": *workspace_id }),
        ),
        TaskReduce {
            workspace_id,
            inputs,
            strategy,
            extract_path,
        } => {
            if inputs.is_empty() {
                crate::out::errln!("{}", tasty_i18n::t("cli.agent.inputs_empty"));
                std::process::exit(1);
            }
            let strategy_val = parse_reducer_strategy(strategy);
            let mut params = serde_json::json!({
                "workspace_id": *workspace_id,
                "inputs": inputs,
                "strategy": strategy_val,
            });
            if let Some(path) = extract_path {
                params["extract_path"] = serde_json::json!(path);
            }
            ("agent.task_reduce", params)
        }
        RateLimitSet {
            agent,
            metric,
            limit,
            per_ms,
            burst,
        } => {
            let mut p = serde_json::json!({
                "agent": agent,
                "metric": metric,
                "limit": *limit,
                "per_ms": *per_ms,
            });
            if let Some(b) = burst {
                p["burst"] = serde_json::json!(*b);
            }
            ("agent.rate_limit_set", p)
        }
        RateLimitList => ("agent.rate_limit_list", serde_json::json!({})),
        RateLimitRemove { id } => ("agent.rate_limit_remove", serde_json::json!({ "id": id })),
        RateLimitStatus { agent, metric } => {
            let mut p = serde_json::json!({});
            if let Some(a) = agent {
                p["agent"] = serde_json::json!(a);
            }
            if let Some(m) = metric {
                p["metric"] = serde_json::json!(m);
            }
            ("agent.rate_limit_status", p)
        }
    }
}

#[allow(clippy::too_many_arguments)] // CLI 인자 하나당 파라미터 하나 — 묶으면 오히려 추적이 어려워짐
fn build_task_create_params(
    workspace_id: u32,
    name: &str,
    cmd_spec: &str,
    depends_on: &[String],
    on_failure: &str,
    metadata: Option<&str>,
    concurrency_limit: Option<&str>,
    reserved_for_fallback: bool,
) -> (&'static str, serde_json::Value) {
    let mut command_val = parse_inline_or_file_json(cmd_spec, "--command");
    let warnings = inject_command_workspace_id(&mut command_val, workspace_id);
    let metadata_val = metadata
        .map(|s| parse_inline_or_file_json(s, "--metadata"))
        .unwrap_or(serde_json::Value::Null);
    let metadata_val = apply_concurrency_limit(metadata_val, concurrency_limit);
    let on_failure_val = parse_on_failure(on_failure);

    let mut p = serde_json::json!({
        "workspace_id": workspace_id,
        "name": name,
        "command": command_val,
    });
    if !depends_on.is_empty() {
        p["depends_on"] = serde_json::Value::Array(
            depends_on
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        );
    }
    p["on_failure"] = on_failure_val;
    if !metadata_val.is_null() {
        p["metadata"] = metadata_val;
    }
    if reserved_for_fallback {
        p["reserved_for_fallback"] = serde_json::Value::Bool(true);
    }
    if !warnings.is_empty() {
        p[super::CLI_WARNINGS_PARAMS_KEY] = serde_json::Value::Array(
            warnings
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
    }
    ("agent.task_create", p)
}

/// `workspace_id` 필드를 가진 command 종류. 그래프 제출은 다른 종류의 command 에 이 키가
/// 있으면 모르는 키로 거절한다.
const COMMAND_KINDS_WITH_WORKSPACE_ID: [&str; 2] = ["run", "agent"];

/// 그래프 안의 run·agent command(on_failure 의 inline fallback 포함)에 `workspace_id` 가
/// 없으면 `--workspace-id` 를 채운다. 다른 종류의 command 는 그대로 보낸다.
fn build_task_graph_params(
    workspace_id: u32,
    graph: &str,
    dry_run: bool,
) -> (&'static str, serde_json::Value) {
    let mut graph_val = parse_inline_or_file_json(graph, "--graph");
    let mut warnings = Vec::new();
    if let Some(tasks) = graph_val.get_mut("tasks").and_then(|t| t.as_array_mut()) {
        for task in tasks {
            for pointer in ["/command", "/on_failure/inline/command"] {
                let Some(command) = task.pointer_mut(pointer) else {
                    continue;
                };
                let kind = command.get("kind").and_then(|k| k.as_str());
                if kind.is_some_and(|k| COMMAND_KINDS_WITH_WORKSPACE_ID.contains(&k)) {
                    warnings.extend(inject_command_workspace_id(command, workspace_id));
                }
            }
        }
    }
    let mut p = serde_json::json!({ "workspace_id": workspace_id, "graph": graph_val });
    if !warnings.is_empty() {
        p[super::CLI_WARNINGS_PARAMS_KEY] = serde_json::Value::Array(
            warnings
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
    }
    let method = if dry_run {
        "agent.task_graph_validate"
    } else {
        "agent.task_graph_submit"
    };
    (method, p)
}

/// `--strategy` 파싱:
/// - `first_success` / `all` / `merge_json` / `concat_text` → `{ "kind": "<x>" }`
/// - `custom:<command>` → `{ "kind": "custom", "command": "<command>" }`
fn parse_reducer_strategy(s: &str) -> serde_json::Value {
    if let Some(cmd) = s.strip_prefix("custom:") {
        serde_json::json!({ "kind": "custom", "command": cmd })
    } else {
        serde_json::json!({ "kind": s })
    }
}

/// `--command` JSON 에 `workspace_id` 가 없으면 `--workspace-id` 플래그 값을 채워
/// 넣고, 이미 있는데 플래그 값과 다르면 플래그 값으로 덮어쓰며 경고 문구를 반환한다
/// (플래그 값이 항상 우선 — 에러로 거부하지 않음).
fn inject_command_workspace_id(
    command_val: &mut serde_json::Value,
    flag_workspace_id: u32,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(obj) = command_val.as_object_mut() else {
        return warnings;
    };
    match obj.get("workspace_id") {
        None => {
            obj.insert(
                "workspace_id".to_string(),
                serde_json::json!(flag_workspace_id),
            );
        }
        Some(v) if v.as_u64() == Some(flag_workspace_id as u64) => {}
        Some(v) => {
            warnings.push(tasty_i18n::t_args(
                "cli.agent.command_workspace_id_overridden",
                &[
                    &v.to_string(),
                    &flag_workspace_id.to_string(),
                    &flag_workspace_id.to_string(),
                ],
            ));
            obj.insert(
                "workspace_id".to_string(),
                serde_json::json!(flag_workspace_id),
            );
        }
    }
    warnings
}

/// `--command` 같은 인자: 인라인 JSON 또는 `@path/to/file.json`.
fn parse_inline_or_file_json(s: &str, flag: &str) -> serde_json::Value {
    let json_text = if let Some(path) = s.strip_prefix('@') {
        match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_args(
                        "cli.agent.flag_file_read_failed",
                        &[path, flag, &e.to_string()]
                    )
                );
                std::process::exit(1);
            }
        }
    } else {
        s.to_string()
    };
    match serde_json::from_str(&json_text) {
        Ok(v) => v,
        Err(e) => {
            crate::out::errln!(
                "{}",
                tasty_i18n::t_fmt2("cli.agent.flag_not_json", flag, &e.to_string())
            );
            std::process::exit(1);
        }
    }
}

/// concurrency-limit를 metadata.semaphore.name에 넣는다. 지정하지 않으면 원래 metadata를 유지한다.
/// metadata에 semaphore가 이미 있으면 두 설정 중 하나를 임의로 고르지 않고 거절한다.
fn apply_concurrency_limit(
    metadata_val: serde_json::Value,
    concurrency_limit: Option<&str>,
) -> serde_json::Value {
    let Some(name) = concurrency_limit else {
        return metadata_val;
    };
    let mut obj = match metadata_val {
        serde_json::Value::Null => serde_json::Map::new(),
        serde_json::Value::Object(map) => map,
        _ => {
            crate::out::errln!("{}", tasty_i18n::t("cli.agent.metadata_not_object"));
            std::process::exit(1);
        }
    };
    if obj.contains_key("semaphore") {
        crate::out::errln!("{}", tasty_i18n::t("cli.agent.metadata_semaphore_conflict"));
        std::process::exit(1);
    }
    obj.insert("semaphore".to_string(), serde_json::json!({ "name": name }));
    serde_json::Value::Object(obj)
}

/// 선택 인자는 준 것만 싣는다.
fn task_set_result_params(
    workspace_id: u32,
    id: &str,
    state: &str,
    output: Option<&str>,
    error: Option<&str>,
    exit_code: Option<i32>,
    attempt_id: Option<&str>,
) -> serde_json::Value {
    let mut p = serde_json::json!({
        "workspace_id": workspace_id,
        "id": id,
        "state": state,
    });
    if let Some(o) = output {
        p["output"] = parse_inline_or_file_json(o, "--output");
    }
    if let Some(e) = error {
        p["error"] = serde_json::json!(e);
    }
    if let Some(c) = exit_code {
        p["exit_code"] = serde_json::json!(c);
    }
    if let Some(a) = attempt_id {
        p["attempt_id"] = serde_json::json!(a);
    }
    p
}

/// `--on-failure` 인자 파싱:
/// - `abort` → `{ "kind": "abort" }`
/// - `continue_downstream` → `{ "kind": "continue_downstream" }`
/// - `fallback:<task_id>` → `{ "kind": "fallback", "task": "<task_id>" }`
fn parse_on_failure(s: &str) -> serde_json::Value {
    if let Some(task) = s.strip_prefix("fallback:") {
        serde_json::json!({ "kind": "fallback", "task": task })
    } else {
        serde_json::json!({ "kind": s })
    }
}

fn dag_report_params(
    dag: &str,
    workspace_id: Option<u32>,
    task: Option<&str>,
    attempt: Option<u32>,
    include_raw: bool,
) -> (&'static str, serde_json::Value) {
    let mut p = serde_json::json!({ "id": dag, "include_raw": include_raw });
    if let Some(w) = workspace_id {
        p["workspace_id"] = serde_json::Value::from(w);
    }
    if let Some(t) = task {
        p["task"] = serde_json::Value::from(t);
    }
    if let Some(a) = attempt {
        p["attempt"] = serde_json::Value::from(a);
    }
    ("agent.dag_report", p)
}

/// `report append` 는 주소가 없거나 틀리면 통신 전에 끝낸다. `usage` 는 `dispatch.rs` 가 통신 없이 처리한다.
fn report_params(command: &ReportCommands) -> (&'static str, serde_json::Value) {
    let ReportCommands::Append { address, text } = command else {
        return ("report.noop", serde_json::json!({}));
    };
    let address = match address {
        Some(a) => a.clone(),
        None => std::env::var(tasty_agent::task::report::REPORT_ENV).unwrap_or_else(|_| {
            crate::out::errln!("{}", tasty_i18n::t("cli.agent.report_address_missing"));
            std::process::exit(1);
        }),
    };
    if let Err(e) = tasty_agent::task::report::ReportAddress::parse(&address) {
        crate::out::errln!(
            "{}",
            tasty_i18n::t_fmt("cli.agent.report_address_invalid", &e)
        );
        std::process::exit(1);
    }
    (
        "agent.report_append",
        serde_json::json!({ "address": address, "text": text }),
    )
}
