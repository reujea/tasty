//! `tasty plugin ...` CLI → JsonRpcRequest 매핑.

use crate::commands::PluginCommands;

pub(super) fn plugin_command_to_method_params(
    command: &PluginCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        PluginCommands::List => ("plugin.list", serde_json::json!({})),
        PluginCommands::Show { id } => ("plugin.show", serde_json::json!({ "id": id })),
        PluginCommands::Install {
            path,
            allow_unverified,
        } => (
            "plugin.install",
            serde_json::json!({ "path": path, "allow_unverified": allow_unverified }),
        ),
        PluginCommands::Remove { id, ignore_busy } => (
            "plugin.remove",
            serde_json::json!({ "id": id, "ignore_busy": ignore_busy }),
        ),
        PluginCommands::UpgradeBuiltins {
            force,
            restore_removed,
            restore_all,
            restart_running,
            ignore_busy,
        } => (
            "plugin.upgrade_builtins",
            serde_json::json!({
                "force": force,
                "restore_removed": restore_removed,
                "restore_all": restore_all,
                "restart_running": restart_running,
                "ignore_busy": ignore_busy,
            }),
        ),
        PluginCommands::Enable { id, .. } => ("plugin.enable", serde_json::json!({ "id": id })),
        PluginCommands::Disable { id, ignore_busy } => (
            "plugin.disable",
            serde_json::json!({ "id": id, "ignore_busy": ignore_busy }),
        ),
        // Logs는 IPC를 거치지 않음 — run_client에서 special-case로 처리.
        PluginCommands::Logs { .. } => ("plugin.list", serde_json::json!({})),
        // Doctor도 IPC를 거치지 않음 — manifest 를 로컬에서 직접 읽는다.
        PluginCommands::Doctor { .. } => ("plugin.list", serde_json::json!({})),
        PluginCommands::Permissions { id } => {
            ("plugin.permissions", serde_json::json!({ "id": id }))
        }
        PluginCommands::Grant { id, permission } => (
            "plugin.grant",
            serde_json::json!({ "id": id, "permission": permission }),
        ),
        PluginCommands::Revoke { id, permission } => (
            "plugin.revoke",
            serde_json::json!({ "id": id, "permission": permission }),
        ),
        PluginCommands::GrantAgentPermission {
            agent,
            permission,
            ttl,
        } => (
            "plugin.grant_agent_permission",
            serde_json::json!({
                "agent_id": agent,
                "permission": permission,
                "ttl_secs": ttl,
            }),
        ),
        PluginCommands::RevokeAgentPermission { agent, permission } => (
            "plugin.revoke_agent_permission",
            serde_json::json!({
                "agent_id": agent,
                "permission": permission,
            }),
        ),
        PluginCommands::ListAgentPermissions { agent } => (
            "plugin.list_agent_permissions",
            serde_json::json!({ "agent_id": agent }),
        ),
        PluginCommands::RequestPermission {
            agent,
            permission,
            reason,
        } => (
            "plugin.request_permission",
            serde_json::json!({
                "agent_id": agent,
                "permission": permission,
                "reason": reason,
            }),
        ),
        PluginCommands::Extension { command } => match command {
            crate::ExtensionCommands::List => ("plugin.extension.list", serde_json::json!({})),
        },
        PluginCommands::AuditQuery {
            caller_kind,
            caller_id,
            method_prefix,
            decision,
            since_ms,
            until_ms,
            limit,
        } => {
            let mut p = serde_json::Map::new();
            if let Some(v) = caller_kind {
                p.insert("caller_kind".into(), serde_json::json!(v));
            }
            if let Some(v) = caller_id {
                p.insert("caller_id".into(), serde_json::json!(v));
            }
            if let Some(v) = method_prefix {
                p.insert("method_prefix".into(), serde_json::json!(v));
            }
            if let Some(v) = decision {
                p.insert("decision".into(), serde_json::json!(v));
            }
            if let Some(v) = since_ms {
                p.insert("since_ms".into(), serde_json::json!(v));
            }
            if let Some(v) = until_ms {
                p.insert("until_ms".into(), serde_json::json!(v));
            }
            if let Some(v) = limit {
                p.insert("limit".into(), serde_json::json!(v));
            }
            ("plugin.audit_query", serde_json::Value::Object(p))
        }
        PluginCommands::AuditSummary {
            caller_kind,
            caller_id,
            method_prefix,
            decision,
            since_ms,
            until_ms,
            top_n,
        } => {
            let mut p = serde_json::Map::new();
            if let Some(v) = caller_kind {
                p.insert("caller_kind".into(), serde_json::json!(v));
            }
            if let Some(v) = caller_id {
                p.insert("caller_id".into(), serde_json::json!(v));
            }
            if let Some(v) = method_prefix {
                p.insert("method_prefix".into(), serde_json::json!(v));
            }
            if let Some(v) = decision {
                p.insert("decision".into(), serde_json::json!(v));
            }
            if let Some(v) = since_ms {
                p.insert("since_ms".into(), serde_json::json!(v));
            }
            if let Some(v) = until_ms {
                p.insert("until_ms".into(), serde_json::json!(v));
            }
            if let Some(v) = top_n {
                p.insert("top_n".into(), serde_json::json!(v));
            }
            ("plugin.audit_summary", serde_json::Value::Object(p))
        }
        PluginCommands::AuditClear { before_ms } => {
            let mut p = serde_json::Map::new();
            if let Some(v) = before_ms {
                p.insert("before_ms".into(), serde_json::json!(v));
            }
            ("plugin.audit_clear", serde_json::Value::Object(p))
        }
        // audit-follow는 ClientDriven에서 plugin.audit_follow IPC를 반복 호출한다.
        PluginCommands::AuditFollow { .. } => ("plugin.list", serde_json::json!({})),
    }
}
