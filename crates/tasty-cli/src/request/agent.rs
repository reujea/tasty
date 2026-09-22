//! `tasty agent ...` CLI → JsonRpcRequest 매핑.

use crate::commands::AgentCommands;

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
        TaskList {
            workspace_id,
            state,
        } => {
            let mut p = serde_json::json!({ "workspace_id": *workspace_id });
            // 단일 값은 예전과 똑같이 문자열로 보낸다 — 다중값을 모르는 구버전
            // 호스트에 새 CLI 가 붙어도 `--state running` 은 그대로 동작한다.
            // 2개 이상일 때만 배열로 보낸다(구버전 호스트는 배열을 필터로
            // 인식하지 못해 필터 없이 전체를 반환하는데, 조용히 빈 목록을
            // 내놓는 것보다 낫다).
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
        TaskGet { workspace_id, id } => (
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
            serde_json::json!({ "workspace_id": *workspace_id, "format": format }),
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
            let mut p = serde_json::json!({ "id": id, "format": format });
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            ("agent.dag_get", p)
        }
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
        } => {
            let mut p = serde_json::json!({
                "workspace_id": *workspace_id,
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
                p["exit_code"] = serde_json::json!(*c);
            }
            ("agent.task_set_result", p)
        }
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

/// `TaskCreate` → `agent.task_create` params 조립. `agent_command_to_method_params`
/// 의 인지 복잡도 상한을 넘기지 않도록 그 큰 match 밖으로 뺀 것 — 로직
/// 자체는 이전과 동일하다.
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

/// `--concurrency-limit <name>` 를 `metadata.semaphore.name` 으로 병합한다
/// (`RunnerLoop` dispatch 가 읽는 `task.metadata.semaphore = { name, holder? }`
/// 컨벤션의 CLI 단축 — 값 자체는 raw `--metadata` 로 넣는 것과 동일하다). `None` 이면
/// `metadata_val` 그대로 통과. `--metadata` 가 이미 `semaphore` 키를 담고 있으면
/// 어느 쪽을 취할지 모호하므로 에러로 거부한다.
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
