//! `tasty telemetry ...` CLI → JsonRpcRequest 매핑.

use crate::commands::{TelemetryAnomalyCommands, TelemetryCapCommands, TelemetryCommands};

#[allow(clippy::cognitive_complexity)] // complexity-exempt: CLI enum→(method,params) 평면 match 매핑 — arm 나열, 중첩 없음
pub(super) fn telemetry_command_to_method_params(
    command: &TelemetryCommands,
) -> (&'static str, serde_json::Value) {
    use TelemetryCommands::*;
    match command {
        Record {
            metric,
            value,
            op,
            agent,
            workspace_id,
            tags,
        } => {
            let mut p = serde_json::json!({
                "metric": metric,
                "value": value,
                "op": op,
            });
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(t) = tags {
                match serde_json::from_str::<serde_json::Value>(t) {
                    Ok(v) => p["tags"] = v,
                    Err(e) => {
                        crate::out::errln!(
                            "{}",
                            tasty_i18n::t_fmt("cli.telemetry.tags_not_object", &e.to_string())
                        );
                        std::process::exit(2);
                    }
                }
            }
            ("telemetry.record", p)
        }
        RecordBatch { events } => match serde_json::from_str::<serde_json::Value>(events) {
            Ok(v @ serde_json::Value::Array(_)) => {
                ("telemetry.record_batch", serde_json::json!({ "events": v }))
            }
            // 배열이 아닌 JSON 도 여기서 걸러낸다 — host 는 같은 이유로 거절하지만,
            // 셸에서 오타를 낸 사람에게는 왕복 전에 답하는 편이 낫다.
            Ok(_) => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.telemetry.events_not_array", "not an array")
                );
                std::process::exit(2);
            }
            Err(e) => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.telemetry.events_not_array", &e.to_string())
                );
                std::process::exit(2);
            }
        },
        Summary {
            metric,
            agent,
            workspace_id,
            since,
            until,
        } => {
            let mut p = serde_json::json!({});
            if let Some(m) = metric {
                p["metric"] = serde_json::Value::String(m.clone());
            }
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            ("telemetry.summary", p)
        }
        Timeseries {
            metric,
            agent,
            workspace_id,
            window,
            since,
            until,
        } => {
            let mut p = serde_json::json!({
                "metric": metric,
                "window": window,
            });
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            ("telemetry.timeseries", p)
        }
        Top {
            by,
            limit,
            metric,
            agent,
            workspace_id,
            since,
            until,
        } => {
            let mut p = serde_json::json!({
                "by": by,
                "limit": limit,
            });
            if let Some(m) = metric {
                p["metric"] = serde_json::Value::String(m.clone());
            }
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            ("telemetry.top", p)
        }
        Cap { command } => telemetry_cap_command_to_method_params(command),
        Anomaly { command } => telemetry_anomaly_command_to_method_params(command),
        SessionSummary {
            workspace_id,
            since,
            until,
            format,
            top_n,
        } => {
            let mut p = serde_json::json!({});
            if let Some(w) = workspace_id {
                p["workspace_id"] = serde_json::Value::from(*w);
            }
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            p["format"] = serde_json::Value::String(format.clone());
            if let Some(n) = top_n {
                p["top_n"] = serde_json::Value::from(*n);
            }
            ("telemetry.session_summary", p)
        }
    }
}

pub(super) fn telemetry_anomaly_command_to_method_params(
    command: &TelemetryAnomalyCommands,
) -> (&'static str, serde_json::Value) {
    use TelemetryAnomalyCommands::*;
    match command {
        List {
            agent,
            kind,
            since,
            until,
        } => {
            let mut p = serde_json::json!({});
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            if let Some(k) = kind {
                p["kind"] = serde_json::Value::String(k.clone());
            }
            if let Some(s) = since {
                p["since"] = serde_json::Value::from(*s);
            }
            if let Some(u) = until {
                p["until"] = serde_json::Value::from(*u);
            }
            ("telemetry.anomaly.list", p)
        }
    }
}

pub(super) fn telemetry_cap_command_to_method_params(
    command: &TelemetryCapCommands,
) -> (&'static str, serde_json::Value) {
    use TelemetryCapCommands::*;
    match command {
        Set {
            agent,
            metric,
            threshold,
            window,
            action,
        } => (
            "telemetry.cap.set",
            serde_json::json!({
                "agent": agent,
                "metric": metric,
                "threshold": threshold,
                "window": window,
                "action": action,
            }),
        ),
        List { agent } => {
            let mut p = serde_json::json!({});
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            ("telemetry.cap.list", p)
        }
        Remove { id } => ("telemetry.cap.remove", serde_json::json!({ "id": id })),
        Status { agent } => {
            let mut p = serde_json::json!({});
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            ("telemetry.cap.status", p)
        }
        Reset { id, agent } => {
            if id.is_none() && agent.is_none() {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t("cli.telemetry.cap_reset_target_required")
                );
                std::process::exit(2);
            }
            let mut p = serde_json::json!({});
            if let Some(i) = id {
                p["id"] = serde_json::Value::String(i.clone());
            }
            if let Some(a) = agent {
                p["agent"] = serde_json::Value::String(a.clone());
            }
            ("telemetry.cap.reset", p)
        }
    }
}

// ============================================================
// agent.task_* CLI mapping
// ============================================================
