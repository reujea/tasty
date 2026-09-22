//! `tasty approval ...` CLI → JsonRpcRequest 매핑.

use crate::commands::ApprovalCommands;

#[allow(clippy::cognitive_complexity)] // complexity-exempt: CLI enum→(method,params) 평면 match 매핑 — arm 나열, 중첩 없음
pub(super) fn approval_command_to_method_params(
    command: &ApprovalCommands,
) -> (&'static str, serde_json::Value) {
    use ApprovalCommands::*;
    match command {
        Request {
            title,
            body,
            choices,
            default_choice,
            timeout_ms,
            severity,
            workspace_id,
            surface_id,
            metadata,
        } => {
            let mut p = serde_json::json!({ "title": title });
            if let Some(b) = body {
                p["body"] = serde_json::Value::String(b.clone());
            }
            if let Some(raw) = choices {
                let arr: Vec<serde_json::Value> = raw
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|spec| {
                        let mut parts = spec.split(':');
                        let key = parts.next().unwrap_or("").to_string();
                        let label = parts
                            .next()
                            .map(str::to_string)
                            .unwrap_or_else(|| key.clone());
                        let destructive = matches!(parts.next(), Some("1") | Some("true"));
                        serde_json::json!({
                            "key": key,
                            "label": label,
                            "destructive": destructive,
                        })
                    })
                    .collect();
                p["choices"] = serde_json::Value::Array(arr);
            }
            if let Some(d) = default_choice {
                p["default_choice"] = serde_json::Value::String(d.clone());
            }
            if let Some(t) = timeout_ms {
                p["timeout_ms"] = serde_json::Value::from(*t);
            }
            if let Some(s) = severity {
                p["severity"] = serde_json::Value::String(s.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(s) = surface_id {
                p["surface_id"] = serde_json::Value::from(*s);
            }
            if let Some(m) = metadata {
                match serde_json::from_str::<serde_json::Value>(m) {
                    Ok(v) => p["metadata"] = v,
                    Err(e) => {
                        crate::out::errln!(
                            "{}",
                            tasty_i18n::t_fmt("cli.approval.metadata_not_json", &e.to_string())
                        );
                        std::process::exit(2);
                    }
                }
            }
            ("approval.request", p)
        }
        Respond {
            id,
            choice,
            comment,
        } => {
            let mut p = serde_json::json!({ "id": id, "choice": choice });
            if let Some(c) = comment {
                p["comment"] = serde_json::Value::String(c.clone());
            }
            ("approval.respond", p)
        }
        Cancel { id } => ("approval.cancel", serde_json::json!({ "id": id })),
        Await { id, timeout_ms } => {
            let mut p = serde_json::json!({ "id": id });
            if let Some(t) = timeout_ms {
                p["timeout_ms"] = serde_json::Value::from(*t);
            }
            ("approval.await", p)
        }
        Get { id } => ("approval.get", serde_json::json!({ "id": id })),
        List {
            state,
            workspace_id,
        } => {
            let mut p = serde_json::json!({});
            if let Some(s) = state {
                p["state"] = serde_json::Value::String(s.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            ("approval.list", p)
        }
        History {
            since,
            until,
            workspace_id,
            requester_id,
            decision,
            state,
            limit,
        } => {
            let mut p = serde_json::json!({});
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(r) = requester_id {
                p["requester_id"] = serde_json::Value::String(r.clone());
            }
            if let Some(d) = decision {
                p["decision"] = serde_json::Value::String(d.clone());
            }
            if let Some(s) = state {
                p["state"] = serde_json::Value::String(s.clone());
            }
            if let Some(l) = limit {
                p["limit"] = serde_json::Value::from(*l);
            }
            ("approval.history", p)
        }
        Summary { command } => {
            use crate::ApprovalSummaryCommands::*;
            match command {
                Set {
                    workspace_id,
                    content,
                } => {
                    let resolved = if let Some(path) = content.strip_prefix('@') {
                        match std::fs::read_to_string(path) {
                            Ok(s) => s,
                            Err(e) => {
                                crate::out::errln!(
                                    "{}",
                                    tasty_i18n::t_fmt2(
                                        "cli.approval.content_file_read_failed",
                                        path,
                                        &e.to_string()
                                    )
                                );
                                std::process::exit(2);
                            }
                        }
                    } else {
                        content.clone()
                    };
                    (
                        "approval.summary.set",
                        serde_json::json!({ "workspace_id": *workspace_id, "content": resolved }),
                    )
                }
                Get { workspace_id } => (
                    "approval.summary.get",
                    serde_json::json!({ "workspace_id": *workspace_id }),
                ),
            }
        }
    }
}
