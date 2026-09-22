use crate::commands::MemorySecretCommands;

use super::{read_value_arg, require_scope, resolve_scope, ttl_to_expires_at};

pub(super) fn memory_secret_command_to_method_params(
    command: &MemorySecretCommands,
) -> (&'static str, serde_json::Value) {
    use MemorySecretCommands::*;
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
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                    params["value"] = parsed;
                } else {
                    params["value"] = serde_json::Value::String(raw);
                }
            } else {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.memory.put_requires_value", "memory secret put")
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
            ("memory.secret.put", params)
        }
        Get { scope, key } => {
            let scope_token = require_scope(scope);
            (
                "memory.secret.get",
                serde_json::json!({ "scope": scope_token, "key": key }),
            )
        }
        Delete { scope, key, cas } => {
            let scope_token = require_scope(scope);
            let mut p = serde_json::json!({ "scope": scope_token, "key": key });
            if let Some(c) = cas {
                p["cas"] = serde_json::json!(c);
            }
            ("memory.secret.delete", p)
        }
        Exists { scope, key } => {
            let scope_token = require_scope(scope);
            (
                "memory.secret.exists",
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
            ("memory.secret.list", p)
        }
        Count { scope, prefix } => {
            let scope_token = require_scope(scope);
            let mut p = serde_json::json!({ "scope": scope_token });
            if let Some(pre) = prefix {
                p["prefix"] = serde_json::json!(pre);
            }
            ("memory.secret.count", p)
        }
        Scopes => ("memory.secret.scopes", serde_json::json!({})),
        Stats { scope } => {
            let mut p = serde_json::json!({});
            if let Some(tok) = resolve_scope(scope) {
                p["scope"] = serde_json::json!(tok);
            }
            ("memory.secret.stats", p)
        }
    }
}
