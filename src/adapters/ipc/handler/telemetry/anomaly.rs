//! 이상 탐지 기록·조회·알림. 보존은 store::log_retention의 공통 정책을 따른다(docs/design/systems/storage.md).

use crate::adapters::ipc::handler::params::{self, p_try};
use serde_json::{Value, json};
use tasty_memory::{ListOpts, MemoryValue, Scope};
use tasty_telemetry::{ANOMALY_KEY_PREFIX, Anomaly, AnomalyKind};

use crate::app::services::AppServices;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

pub(super) use crate::app::telemetry::persist_anomaly;

pub(in crate::adapters::ipc::handler) fn fire_anomaly_notification(
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    anomaly: &Anomaly,
) {
    if !crate::app::anomaly_settings::should_notify(
        &engine.runtime.settings.anomaly_detection,
        anomaly.kind,
    ) {
        return;
    }
    let Some(ws) = engine.workspace_at(window.active_workspace_index(engine)) else {
        return;
    };
    let ws_id = ws.id;
    let title = format!(
        "이상 탐지: {} ({})",
        anomaly.kind.as_token(),
        anomaly.subject
    );
    // CallBurst/SlowLoop는 시간·건수, RssSurge는 샘플 수·RSS로 설명한다.
    let body = match anomaly.kind {
        AnomalyKind::CallBurst | AnomalyKind::SlowLoop => {
            let count = anomaly
                .detail
                .get("count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let window_ms = anomaly
                .detail
                .get("window_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!(
                "agent={} subject={} count={} ({}s 윈도우, anomaly={})",
                anomaly.agent,
                anomaly.subject,
                count,
                window_ms / 1000,
                anomaly.id,
            )
        }
        AnomalyKind::RssSurge => {
            let latest = anomaly
                .detail
                .get("latest_rss_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let min_samples = anomaly
                .detail
                .get("min_samples")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!(
                "agent={} subject={} latest_rss_bytes={} ({}개 샘플 연속 증가, anomaly={})",
                anomaly.agent, anomaly.subject, latest, min_samples, anomaly.id,
            )
        }
    };
    let _ = engine; // 옛 직접 add 경로 제거 — cascade 가 라우팅 + add + host event 일괄.
    out.push(
        crate::app::command::DomainIntent::PushNotification {
            level: crate::notification::NotificationLevel::Warning,
            ws_id,
            surface_id: 0,
            title,
            body,
            source: "telemetry.anomaly".to_string(),
            merge: crate::app::command::NotificationMerge::Coalesce,
            stream_echo: false,
        }
        .from_system(),
    );
}

/// `telemetry.anomaly.list` — 영속된 anomaly 레코드 조회. 필터: `agent`, `kind`,
/// `since`, `until` (unix ms). 응답은 `detected_at` 오름차순.
pub fn handle_anomaly_list(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_filter = params
        .get("agent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let kind_filter = params
        .get("kind")
        .and_then(|v| v.as_str())
        .map(String::from);
    let since = p_try!(params::opt_int::<u64>(params, "since", &id));
    let until = p_try!(params::opt_int::<u64>(params, "until", &id));

    let list_opts = ListOpts {
        prefix: Some(ANOMALY_KEY_PREFIX.to_string()),
        limit: None,
        since: since.map(|v| v as i64),
        until: until.map(|v| v as i64),
        offset: None,
    };
    core.reply_store(
        id,
        crate::adapters::ipc::handler::deferred::params_bytes(params),
        move |s| s.list(&Scope::Global, &list_opts),
        move |_, id, entries| match entries {
            Ok(entries) => anomaly_response(id, entries, agent_filter, kind_filter, since, until),
            Err(e) => JsonRpcResponse::error(id, -32603, format!("memory list failed: {e}")),
        },
    )
}

fn anomaly_response(
    id: Value,
    entries: Vec<tasty_memory::MemoryEntry>,
    agent_filter: Option<String>,
    kind_filter: Option<String>,
    since: Option<u64>,
    until: Option<u64>,
) -> JsonRpcResponse {
    let mut out: Vec<Anomaly> = Vec::with_capacity(entries.len());
    for entry in entries {
        let MemoryValue::Json(v) = entry.value else {
            continue;
        };
        let Ok(a) = serde_json::from_value::<Anomaly>(v) else {
            continue;
        };
        if let Some(ref agent) = agent_filter
            && &a.agent != agent
        {
            continue;
        }
        if let Some(ref k) = kind_filter
            && a.kind.as_token() != k
        {
            continue;
        }
        if let Some(s) = since
            && a.detected_at < s
        {
            continue;
        }
        if let Some(u) = until
            && a.detected_at >= u
        {
            continue;
        }
        out.push(a);
    }
    out.sort_by_key(|a| a.detected_at);
    let arr: Vec<Value> = out
        .iter()
        .map(|a| serde_json::to_value(a).unwrap_or(Value::Null))
        .collect();
    JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
}
