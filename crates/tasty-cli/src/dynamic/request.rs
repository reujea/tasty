//! clap 매칭 결과를 JSON-RPC 메서드와 인자로 변환한다.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use clap::ArgMatches;
use serde_json::{Map, Value};

use tasty_ipc::protocol::JsonRpcRequest;
use tasty_plugin_manifest::{
    AutoWaitDecl, CliArgType, CliOutputDecl, CompletionStrategyDecl, PollingDecl,
};

use super::stdin::{extract_value, merge_stdin_params, read_stdin_json};
use super::{AutoWaitPlan, PluginCliEntry};
use crate::caller_context::CallerContextSource;

/// 이 요청의 응답을 자식 표로 찍을지. 매니페스트가 선언한 메서드만 대상이고 `--format` 값을 검사한다.
pub fn wants_child_table(entries: &[PluginCliEntry], request: &JsonRpcRequest) -> Result<bool> {
    let declared = entries
        .iter()
        .flat_map(|entry| &entry.cli.subcommands)
        .any(|sub| {
            sub.ipc_method == request.method && sub.output == Some(CliOutputDecl::ChildTable)
        });
    if !declared {
        return Ok(false);
    }
    crate::format::child_table::wants_table(&request.params)
}

/// 매칭된 `ArgMatches`에서 plugin이 어떤 메서드를 호출할지 해석.
/// 호스트 정적 서브커맨드와 충돌하지 않는 plugin 최상위 이름에 한해 진행한다.
///
/// 반환의 두 번째 값은 manifest 가 선언한 polling 사양 (있으면). caller 가
/// `Some(polling)` 일 때 *terminal_states 도달까지 반복 IPC 호출* 한다.
///
/// `remote_target` 은 요청의 실행 위치(`workspace` 숫자 id·`pane`)가 attach mirror 인지 host 에 묻는다.
/// 원격 실행을 선언한 메서드에 경로 인자와 실행 위치가 있을 때만 부른다.
pub fn matches_to_request(
    entries: &[PluginCliEntry],
    caller_contexts: &[CallerContextSource],
    matches: &ArgMatches,
    remote_target: &mut dyn FnMut(&Map<String, Value>) -> bool,
) -> Result<(JsonRpcRequest, Option<PollingDecl>, Option<AutoWaitPlan>)> {
    let (top_name, top_sub) = matches
        .subcommand()
        .ok_or_else(|| anyhow!("{}", tasty_i18n::t("cli.plugin_cli.no_subcommand")))?;
    let entry = entries
        .iter()
        .find(|e| e.cli.name == top_name)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                tasty_i18n::t_fmt("cli.plugin_cli.not_plugin_command", top_name)
            )
        })?;
    let (sub_name, sub_args) = top_sub.subcommand().ok_or_else(|| {
        anyhow!(
            "{}",
            tasty_i18n::t_fmt("cli.plugin_cli.subcommand_required", top_name)
        )
    })?;
    let sub_decl = entry
        .cli
        .subcommands
        .iter()
        .find(|s| s.name == sub_name)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                tasty_i18n::t_fmt2("cli.plugin_cli.unknown_subcommand", top_name, sub_name)
            )
        })?;
    let group = entry.cli.arg_groups.get(&sub_decl.args);

    let mut params = Map::new();
    if let Some(g) = group {
        let mut path_args = Vec::new();
        for arg in g.positional.iter().chain(g.flags.iter()) {
            if let Some(v) = extract_value(sub_args, arg)? {
                if matches!(arg.ty, CliArgType::String) && v.is_string() && arg.path_kind.is_some()
                {
                    path_args.push(arg);
                }
                params.insert(arg.name.clone(), v);
            }
        }
        // 실행 위치가 attach mirror 면 경로는 실행하는 서버 기준이라 이 머신에서 바꾸거나 검사하지 않는다.
        // 그 값은 그대로 보내고 서버 쪽 plugin 이 검사한다. 그 밖에는 호출자 cwd 기준 절대 경로로 바꾸고
        // 존재(+종류)를 검사한다.
        // 원격 실행을 선언한 메서드만 묻는다. 다른 명령의 `--workspace` 는 이름일 수 있어 mirror 로 보지 않는다.
        let remote =
            !path_args.is_empty() && entry.remote_methods.contains(&sub_decl.ipc_method) && {
                let query = placement_query(&params);
                !query.is_empty() && remote_target(&query)
            };
        if !remote {
            for arg in path_args {
                let Some(raw) = params.get(&arg.name).and_then(Value::as_str) else {
                    continue;
                };
                let normalized = normalize_path_arg(arg, raw)?;
                params.insert(arg.name.clone(), normalized);
            }
        }
        // `stdin_json = true` 인 서브커맨드는 (stdin 이 TTY 가 아닐 때) stdin 의
        // JSON 한 덩이를 읽어 CLI 로 명시되지 않은 params 필드를 채운다.
        // Claude Code 처럼 hook payload 를 stdin JSON 으로 전달하는 외부 시스템
        // 연동용. CLI 로 직접 지정된 값이 항상 우선.
        if sub_decl.stdin_json
            && let Some(stdin_json) = read_stdin_json()
        {
            merge_stdin_params(&mut params, g, &stdin_json)?;
        }
    }

    // 명시하지 않은 plugin의 surface(u32)·caller_surface 인자는 TASTY_SURFACE_ID로 채운다.
    // 플러그인이 선언한 호출자가 있으면 그 선언에 따라 채우지 않고 호출자 문맥을 싣는다.
    let declares = |name: &str| {
        group.is_some_and(|g| {
            g.flags
                .iter()
                .chain(g.positional.iter())
                .any(|a| a.name == name && matches!(a.ty, CliArgType::U32))
        })
    };
    crate::caller_context::apply_to_plugin_params(
        &mut params,
        declares,
        &crate::caller_context::current(caller_contexts),
        &entry.strict_caller_contexts,
        std::env::var("TASTY_SURFACE_ID").ok().as_deref(),
    )
    .map_err(|message| anyhow!("{message}"))?;

    // CLI의 이름 기반 전략 조회는 아직 연결되지 않았다. 빈 목록으로 넘겨
    // 이름 참조는 unknown strategy로 거절하고 인라인 polling만 처리한다.
    let available_strategies: HashMap<String, CompletionStrategyDecl> = HashMap::new();
    let auto_wait_plan = sub_decl
        .auto_wait
        .as_ref()
        .map(|aw| build_auto_wait_plan(aw, &params, &available_strategies))
        .transpose()?;

    Ok((
        JsonRpcRequest {
            caller_agent_id: tasty_ipc::protocol::caller_agent_id_from_env(),
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            method: sub_decl.ipc_method.clone(),
            params: Value::Object(params),
            id: Some(Value::from(1)),
            session_token: std::env::var("TASTY_SESSION_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
        },
        sub_decl.polling.clone(),
        auto_wait_plan,
    ))
}

/// 요청의 실행 위치. workspace 이름은 mirror 를 가리킬 수 없으므로 숫자 id 만 싣는다.
pub(crate) fn placement_query(params: &Map<String, Value>) -> Map<String, Value> {
    let mut query = Map::new();
    if let Some(ws) = params
        .get("workspace")
        .and_then(|w| w.as_u64().or_else(|| w.as_str()?.parse().ok()))
    {
        query.insert("workspace".into(), Value::from(ws));
    }
    if let Some(pane) = params.get("pane").and_then(Value::as_u64) {
        query.insert("pane".into(), Value::from(pane));
    }
    query
}

/// `path_kind = "directory"`/`"file"`/`"url_or_file"` 인 값을 호출자 cwd 기준 절대 경로로 바꾸고 검사한다.
/// `"url_or_file"` 은 `scheme://` 값을 그대로 두고 나머지를 `"file"` 처럼 다룬다.
fn normalize_path_arg(arg: &tasty_plugin_manifest::CliArg, raw: &str) -> Result<Value> {
    let normalized = match arg.path_kind.as_deref() {
        Some("directory") => crate::cwd_resolve::normalize_cwd_arg(raw),
        Some("file") => crate::cwd_resolve::normalize_file_arg(raw),
        Some("url_or_file") => crate::cwd_resolve::normalize_url_or_file_arg(raw),
        _ => return Ok(Value::String(raw.to_owned())),
    };
    normalized.map(Value::String).map_err(|e| {
        anyhow!(
            "{}",
            tasty_i18n::t_fmt2("cli.plugin_cli.arg_invalid", &arg.name, &e.to_string())
        )
    })
}

/// 인라인 polling 또는 등록된 이름을 PollingDecl로 해석한다.
/// 매니페스트 검증은 둘 중 하나만 허용하며, 이름이 목록에 없으면 오류를 반환한다.
pub(super) fn resolve_auto_wait_polling(
    aw: &AutoWaitDecl,
    available_strategies: &HashMap<String, CompletionStrategyDecl>,
) -> Result<PollingDecl> {
    if let Some(polling) = &aw.polling {
        return Ok(polling.clone());
    }
    let strategy = aw.strategy.as_ref().ok_or_else(|| {
        anyhow!(
            "{}",
            tasty_i18n::t_fmt("cli.plugin_cli.auto_wait_no_mode", &aw.method)
        )
    })?;
    available_strategies
        .get(strategy)
        .map(CompletionStrategyDecl::to_polling_decl)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                tasty_i18n::t_fmt2(
                    "cli.plugin_cli.auto_wait_unknown_strategy",
                    &aw.method,
                    strategy
                )
            )
        })
}

/// `AutoWaitDecl` 와 1 차 요청 params 로 실행 계획을 구성한다.
/// `--no-wait` (params 의 `no_wait_field` 가 true 인 경우) 면 `skipped = true`.
pub(super) fn build_auto_wait_plan(
    aw: &AutoWaitDecl,
    request_params: &Map<String, Value>,
    available_strategies: &HashMap<String, CompletionStrategyDecl>,
) -> Result<AutoWaitPlan> {
    let skipped = request_params
        .get(&aw.no_wait_field)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let polling = resolve_auto_wait_polling(aw, available_strategies)?;
    Ok(AutoWaitPlan {
        method: aw.method.clone(),
        polling,
        map_from_response: aw.map_from_response.clone(),
        map_from_request: aw.map_from_request.clone(),
        timeout_field: aw.timeout_field.clone(),
        request_params: request_params.clone(),
        skipped,
    })
}
