//! `tasty preset` / `tasty file-handler` CLI → JsonRpcRequest 매핑.
//!
//! `read_json_file_or_stdin` 는 preset save 의 file 인자 처리에 사용.

use crate::commands::{
    CompletionStrategyCommands, FileHandlerCommands, HookHandlerCommands, PresetCommands,
};

pub(super) fn file_handler_command_to_method_params(
    command: &FileHandlerCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        FileHandlerCommands::Reload => ("file_handler.reload", serde_json::Value::Null),
        FileHandlerCommands::Dispatch {
            path,
            depth,
            origin_surface,
            ignore_size_limit,
        } => (
            "file_handler.dispatch",
            serde_json::json!({
                "path": path,
                "depth": depth,
                "origin_surface_id": origin_surface,
                "ignore_size_limit": ignore_size_limit,
            }),
        ),
    }
}

pub(super) fn completion_strategy_command_to_method_params(
    command: &CompletionStrategyCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        CompletionStrategyCommands::List => ("completion_strategy.list", serde_json::json!({})),
    }
}

pub(super) fn hook_handler_command_to_method_params(
    command: &HookHandlerCommands,
) -> (&'static str, serde_json::Value) {
    use HookHandlerCommands as H;
    match command {
        H::List => ("hook_handler.list", serde_json::json!({})),
        H::Get { id } => ("hook_handler.get", serde_json::json!({ "id": id })),
        H::Upsert {
            id,
            source,
            priority,
            display_name_key,
            disabled,
            action,
            calls,
        } => {
            // --calls 는 --action 의 IpcSequence 축약. 둘은 clap 이 배타로 막는다.
            let action_value = match (action, calls) {
                (Some(a), _) => Some(json_or_raw(a)),
                (None, Some(c)) => Some(serde_json::json!({
                    "kind": "ipc_sequence",
                    "calls": json_or_raw(c),
                })),
                (None, None) => None,
            };
            (
                "hook_handler.upsert",
                serde_json::json!({
                    "id": id,
                    "source": source,
                    "priority": priority,
                    "display_name_i18n_key": display_name_key,
                    "disabled": disabled,
                    "action": action_value,
                }),
            )
        }
        H::Remove { id } => ("hook_handler.remove", serde_json::json!({ "id": id })),
        H::Reload => ("hook_handler.reload", serde_json::Value::Null),
        H::Dispatch {
            id,
            body,
            header,
            query,
        } => {
            // body/header/query 는 JSON 문자열 → Value 파싱(서버가 치환 컨텍스트로 사용).
            let parse = |s: &Option<String>| {
                s.as_deref()
                    .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
            };
            (
                "hook_handler.dispatch",
                serde_json::json!({
                    "id": id,
                    "body": parse(body),
                    "headers": parse(header),
                    "query": parse(query),
                }),
            )
        }
    }
}

/// JSON 문자열을 파싱하되, **못 읽으면 조용히 버리지 않고 원문 문자열 그대로 넘긴다.**
///
/// `.ok()` 로 떨어뜨리면 그 자리가 "미지정" 이 되고, `--action` 오타 하나가 아무것도
/// 안 고친 upsert 를 성공으로 만든다. 문자열로 넘기면 서버의 스키마 검증이 그 자리에서
/// 거부하므로, 잘못 적은 것이 값으로 드러난다.
fn json_or_raw(s: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(s)
        .unwrap_or_else(|_| serde_json::Value::String(s.to_string()))
}

/// Read --file (or "-" for stdin) and parse as JSON.
pub(super) fn read_json_file_or_stdin(path: &str) -> Result<serde_json::Value, String> {
    use std::io::Read;
    let raw = if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| tasty_i18n::t_fmt("cli.preset.stdin_read_failed", &e.to_string()))?;
        buf
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| tasty_i18n::t_fmt("cli.preset.file_read_failed", &e.to_string()))?
    };
    serde_json::from_str(&raw).map_err(|e| tasty_i18n::t_fmt("cli.preset.not_json", &e.to_string()))
}

pub(super) fn preset_command_to_method_params(
    command: &PresetCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        PresetCommands::List { kind } => ("preset.list", serde_json::json!({ "kind": kind })),
        PresetCommands::Get { kind, name } => (
            "preset.get",
            serde_json::json!({ "kind": kind, "name": name }),
        ),
        PresetCommands::Save {
            kind,
            name,
            file,
            overwrite,
        } => {
            let data = match read_json_file_or_stdin(file) {
                Ok(v) => v,
                Err(e) => {
                    crate::out::errln!("{}", tasty_i18n::t_fmt("cli.preset.save_read_failed", &e));
                    std::process::exit(1);
                }
            };
            (
                "preset.save",
                serde_json::json!({
                    "kind": kind,
                    "name": name,
                    "data": data,
                    "overwrite": overwrite,
                }),
            )
        }
        PresetCommands::Delete { kind, name } => (
            "preset.delete",
            serde_json::json!({ "kind": kind, "name": name }),
        ),
        PresetCommands::Rename { kind, from, to } => (
            "preset.rename",
            serde_json::json!({ "kind": kind, "from": from, "to": to }),
        ),
        PresetCommands::Capture {
            kind,
            source_id,
            name,
        } => (
            "preset.capture",
            serde_json::json!({
                "kind": kind,
                "source_id": source_id,
                "name": name,
            }),
        ),
        PresetCommands::Apply {
            kind,
            name,
            target_pane,
            target_workspace,
        } => (
            "preset.apply",
            serde_json::json!({
                "kind": kind,
                "name": name,
                "target_pane_id": target_pane,
                "target_workspace_id": target_workspace,
            }),
        ),
    }
}
