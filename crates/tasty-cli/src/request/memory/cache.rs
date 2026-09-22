use crate::commands::MemoryCacheCommands;

use super::read_value_arg;

pub(super) fn memory_cache_command_to_method_params(
    command: &MemoryCacheCommands,
) -> (&'static str, serde_json::Value) {
    use MemoryCacheCommands::*;
    match command {
        Put {
            workspace,
            key,
            value,
            value_b64,
            content_type,
            ttl,
        } => {
            let mut p = serde_json::json!({
                "workspace_id": workspace,
                "key": key,
                "ttl_secs": ttl,
            });
            if let Some(b64) = value_b64.as_deref() {
                p["value_b64"] = serde_json::json!(b64);
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
                    p["value"] = parsed;
                } else {
                    p["value"] = serde_json::Value::String(raw);
                }
            } else {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.memory.put_requires_value", "memory cache put")
                );
                std::process::exit(1);
            }
            if let Some(ct) = content_type.as_deref() {
                p["content_type"] = serde_json::json!(ct);
            }
            ("memory.cache_put", p)
        }
        Get { workspace, key } => (
            "memory.cache_get",
            serde_json::json!({ "workspace_id": workspace, "key": key }),
        ),
        Invalidate { workspace, key } => (
            "memory.cache_invalidate",
            serde_json::json!({ "workspace_id": workspace, "key": key }),
        ),
        Clear { workspace } => (
            "memory.cache_clear",
            serde_json::json!({ "workspace_id": workspace }),
        ),
        List { workspace } => (
            "memory.cache_list",
            serde_json::json!({ "workspace_id": workspace }),
        ),
    }
}
