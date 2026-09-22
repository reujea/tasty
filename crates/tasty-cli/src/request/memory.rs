//! `tasty memory ...` CLI → JsonRpcRequest 매핑 + 공용 scope/value/TTL 헬퍼.

use crate::commands::{MemoryCommands, memory::ScopeArgs};

/// scope 선택자 → scope 토큰. 아무것도 안 주면 `None`.
///
/// 인자가 [`ScopeArgs`] 한 덩어리인 것이 요점이다 — 여섯 값을 따로 받으면 호출부마다
/// 여섯 줄이 되고, 그 여섯 줄이 자리 수만큼(16) 복제된 것이 이 모듈의 원래 모습이었다.
pub(super) fn resolve_scope(a: &ScopeArgs) -> Option<String> {
    if let Some(s) = a.scope.as_deref() {
        return Some(s.to_string());
    }
    if let Some(id) = a.surface {
        return Some(format!("surface:{id}"));
    }
    if let Some(id) = a.workspace {
        return Some(format!("workspace:{id}"));
    }
    if let Some(id) = a.window {
        return Some(format!("window:{id}"));
    }
    if let Some(u) = a.account.as_deref() {
        return Some(format!("account:{u}"));
    }
    if a.global {
        return Some("global".to_string());
    }
    None
}

/// `@path` 접두를 가진 value는 파일에서 UTF-8 텍스트로 읽어온다. 이외에는 그대로.
pub(super) fn read_value_arg(value: &str) -> std::io::Result<String> {
    if let Some(path) = value.strip_prefix('@') {
        std::fs::read_to_string(path)
    } else {
        Ok(value.to_string())
    }
}

#[allow(clippy::cognitive_complexity)] // complexity-exempt: CLI enum→(method,params) 평면 match 매핑 — arm 나열, 중첩 없음
pub(super) fn memory_command_to_method_params(
    command: &MemoryCommands,
) -> (&'static str, serde_json::Value) {
    use MemoryCommands::*;
    match command {
        Put {
            scope,
            key,
            value,
            value_b64,
            content_type,
            ttl,
            expires_at,
            cas,
        } => {
            let scope_token = require_scope(scope);
            let mut params = serde_json::json!({
                "scope": scope_token,
                "key": key,
            });
            if let Some(b64) = value_b64.as_deref() {
                params["value_b64"] = serde_json::json!(b64);
            } else if let Some(v) = value.as_deref() {
                let raw = match read_value_arg(v) {
                    Ok(s) => s,
                    Err(e) => {
                        crate::out::errln!(
                            "{}",
                            tasty_i18n::t_fmt("cli.memory.value_file_read_failed", &e.to_string())
                        );
                        std::process::exit(1);
                    }
                };
                // JSON으로 파싱되면 JSON value, 아니면 string으로 보존.
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                    params["value"] = parsed;
                } else {
                    params["value"] = serde_json::Value::String(raw);
                }
            } else {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.memory.put_requires_value", "memory put")
                );
                std::process::exit(1);
            }
            if let Some(ct) = content_type.as_deref() {
                params["content_type"] = serde_json::json!(ct);
            }
            if let Some(t) = expires_at {
                params["expires_at"] = serde_json::json!(t);
            } else if let Some(secs) = ttl {
                params["expires_at"] = serde_json::json!(ttl_to_expires_at(*secs));
            }
            if let Some(v) = cas {
                params["cas"] = serde_json::json!(v);
            }
            ("memory.put", params)
        }
        Get { scope, key } => {
            let scope_token = require_scope(scope);
            (
                "memory.get",
                serde_json::json!({ "scope": scope_token, "key": key }),
            )
        }
        Delete { scope, key, cas } => {
            let scope_token = require_scope(scope);
            let mut p = serde_json::json!({ "scope": scope_token, "key": key });
            if let Some(c) = cas {
                p["cas"] = serde_json::json!(c);
            }
            ("memory.delete", p)
        }
        Exists { scope, key } => {
            let scope_token = require_scope(scope);
            (
                "memory.exists",
                serde_json::json!({ "scope": scope_token, "key": key }),
            )
        }
        List {
            scope,
            prefix,
            limit,
            since,
            until,
            offset,
        } => {
            let scope_token = require_scope(scope);
            let mut p = serde_json::json!({ "scope": scope_token });
            if let Some(pre) = prefix {
                p["prefix"] = serde_json::json!(pre);
            }
            if let Some(l) = limit {
                p["limit"] = serde_json::json!(l);
            }
            if let Some(s) = since {
                p["since"] = serde_json::json!(s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::json!(u);
            }
            if let Some(o) = offset {
                p["offset"] = serde_json::json!(o);
            }
            ("memory.list", p)
        }
        Query {
            scope,
            path,
            equals,
            prefix,
            limit,
            since,
            until,
            offset,
        } => {
            let scope_token = require_scope(scope);
            // `--equals` 는 JSON 리터럴로 파싱; 실패하면 문자열 그대로.
            let equals_val: serde_json::Value = match serde_json::from_str(equals) {
                Ok(v) => v,
                Err(_) => serde_json::Value::String(equals.clone()),
            };
            let mut p = serde_json::json!({
                "scope": scope_token,
                "path": path,
                "equals": equals_val,
            });
            if let Some(pre) = prefix {
                p["prefix"] = serde_json::json!(pre);
            }
            if let Some(l) = limit {
                p["limit"] = serde_json::json!(l);
            }
            if let Some(s) = since {
                p["since"] = serde_json::json!(s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::json!(u);
            }
            if let Some(o) = offset {
                p["offset"] = serde_json::json!(o);
            }
            ("memory.query", p)
        }
        Export { scope } => {
            let mut p = serde_json::json!({});
            if let Some(tok) = resolve_scope(scope) {
                p["scope"] = serde_json::json!(tok);
            }
            ("memory.export", p)
        }
        Import { file, replace } => {
            let raw = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt2("cli.memory.import_read_failed", file, &e.to_string())
                    );
                    std::process::exit(1);
                }
            };
            let parsed: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt2("cli.memory.import_not_json", file, &e.to_string())
                    );
                    std::process::exit(1);
                }
            };
            // 입력은 배열이거나 `{ "entries": [...] }` 형태 둘 다 허용.
            let entries = if parsed.is_array() {
                parsed
            } else if let Some(arr) = parsed.get("entries") {
                arr.clone()
            } else {
                crate::out::errln!("{}", tasty_i18n::t_fmt("cli.memory.import_bad_shape", file));
                std::process::exit(1);
            };
            (
                "memory.import",
                serde_json::json!({ "entries": entries, "replace": replace }),
            )
        }
        Count { scope, prefix } => {
            let scope_token = require_scope(scope);
            let mut p = serde_json::json!({ "scope": scope_token });
            if let Some(pre) = prefix {
                p["prefix"] = serde_json::json!(pre);
            }
            ("memory.count", p)
        }
        Scopes => ("memory.scopes", serde_json::json!({})),
        Stats { scope } => {
            let mut p = serde_json::json!({});
            if let Some(tok) = resolve_scope(scope) {
                p["scope"] = serde_json::json!(tok);
            }
            ("memory.stats", p)
        }
        Gc => ("memory.gc", serde_json::json!({})),
        Secret { command } => memory_secret_command_to_method_params(command),
        Bb { command } => memory_bb_command_to_method_params(command),
        Plan { command } => memory_plan_command_to_method_params(command),
        Cache { command } => memory_cache_command_to_method_params(command),
        Goal { command } => memory_goal_command_to_method_params(command),
    }
}

pub(super) fn ttl_to_expires_at(secs: u64) -> i64 {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let add_ms = secs.saturating_mul(1000).min(i64::MAX as u64) as i64;
    now_ms.saturating_add(add_ms)
}

/// [`resolve_scope`] 와 같되, scope 가 없으면 메시지를 내고 종료한다.
pub(super) fn require_scope(a: &ScopeArgs) -> String {
    match resolve_scope(a) {
        Some(s) => s,
        None => {
            crate::out::errln!("{}", tasty_i18n::t("cli.memory.scope_required"));
            std::process::exit(1);
        }
    }
}

mod bb;
mod cache;
mod goal;
mod plan;
mod secret;

use bb::memory_bb_command_to_method_params;
use cache::memory_cache_command_to_method_params;
use goal::memory_goal_command_to_method_params;
use plan::memory_plan_command_to_method_params;
use secret::memory_secret_command_to_method_params;

/// `memory list` ↔ `memory secret list` 가 같은 인자에 같은 params 를 낸다.
///
/// 이 자리의 결함은 "갈렸다" 가 아니라 **처음부터 덜 복제됐고 아무도 안 봤다** 였다 —
/// `memory.secret.list` 핸들러는 `since`/`until`/`offset` 을 처음부터 읽는데 CLI 의 secret
/// 경로는 셋을 한 번도 보낸 적이 없다. 코드를 공유하는 것으로는 그 형태가 안 잡힌다(덜
/// 복제된 쪽에 맞춰 공유물이 쓰였을 것이다). 잡히는 것은 **두 자리에 같은 것을 넣어 보고
/// 나온 것을 대조하는** 술어뿐이다.
#[cfg(test)]
mod list_filter_parity {
    use clap::Parser;

    /// 두 계열이 함께 받아야 하는 list 필터 전부. 값까지 준다.
    const LIST_ARGS: [&str; 11] = [
        "--global", "--prefix", "p", "--limit", "3", "--since", "5", "--until", "9", "--offset",
        "2",
    ];

    fn list_params(secret: bool) -> serde_json::Value {
        let mut argv = vec!["tasty", "memory"];
        if secret {
            argv.push("secret");
        }
        argv.push("list");
        argv.extend(LIST_ARGS);
        let cli = crate::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("`{}` 파싱 실패:\n{e}", argv.join(" ")));
        match cli.command.expect("서브커맨드가 있어야 한다") {
            crate::Commands::Memory { command } => {
                super::memory_command_to_method_params(&command).1
            }
            _ => unreachable!("memory 서브커맨드가 아니다"),
        }
    }

    #[test]
    fn the_two_list_commands_take_the_same_filters_and_send_the_same_params() {
        assert_eq!(
            list_params(false),
            list_params(true),
            "`memory list` 와 `memory secret list` 가 같은 인자에 다른 params 를 낸다. \
             두 계열의 핸들러는 짝마다 같은 키를 읽으므로, CLI 한쪽만 자라면 서버는 받는데 \
             CLI 로는 닿을 길이 없는 자리가 생긴다."
        );
    }

    #[test]
    fn every_list_filter_reaches_the_params_with_its_value() {
        let p = list_params(true);
        for (k, v) in [("limit", 3), ("since", 5), ("until", 9), ("offset", 2)] {
            assert_eq!(
                p.get(k).and_then(serde_json::Value::as_i64),
                Some(v),
                "`--{k}` 가 params 에 안 실린다: {p}"
            );
        }
        assert_eq!(p.get("prefix").and_then(|v| v.as_str()), Some("p"));
    }
}
