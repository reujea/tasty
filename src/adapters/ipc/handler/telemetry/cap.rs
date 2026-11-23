//! 사용량 상한(cap)의 등록·조회와 초과 시 처리를 담당한다.

use serde_json::{Value, json};
use tasty_memory::{ListOpts, MemoryValue, PutOpts, Scope};
use tasty_telemetry::{
    CAP_KEY_PREFIX, CapAction, CapWindow, CostCap, TelemetryEvent, cap_key, summarize_events,
    validate_agent_id, validate_metric,
};

use crate::app::services::AppServices;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::now_ms;
use super::query::{QueryFilter, collect_events};
use crate::adapters::ipc::handler::deferred::params_bytes;

pub(super) fn generate_cap_id(core: &AppServices) -> String {
    let ts = now_ms();
    let seq = core.telemetry_seq.next();
    format!("cap_{ts:013}{seq:04}", ts = ts, seq = seq % 10_000)
}

/// 모든 cap 을 memory 에서 읽어온다. cap 은 global scope 에만 저장.
pub(crate) fn load_all_caps(
    store: &dyn tasty_memory::MemoryStorage,
) -> std::result::Result<Vec<CostCap>, String> {
    let list_opts = ListOpts {
        prefix: Some(CAP_KEY_PREFIX.to_string()),
        limit: None,
        since: None,
        until: None,
        offset: None,
    };
    let entries = store
        .list(&Scope::Global, &list_opts)
        .map_err(|e| format!("memory list failed: {e}"))?;
    let mut out = Vec::new();
    for entry in entries {
        let MemoryValue::Json(v) = entry.value else {
            continue;
        };
        if let Ok(cap) = serde_json::from_value::<CostCap>(v) {
            out.push(cap);
        }
    }
    Ok(out)
}

pub(super) fn save_cap(
    store: &mut dyn tasty_memory::MemoryStorage,
    cap: &CostCap,
) -> std::result::Result<(), String> {
    let key = cap_key(&cap.id);
    let value = MemoryValue::Json(serde_json::to_value(cap).map_err(|e| e.to_string())?);
    let opts = PutOpts {
        expires_at: None,
        cas: None,
    };
    store
        .put(
            tasty_memory::HOST_OWNER,
            &Scope::Global,
            &key,
            &value,
            &opts,
        )
        .map(|_| ())
        .map_err(|e| format!("memory put failed: {e}"))
}

pub(super) fn cap_to_json(cap: &CostCap) -> Value {
    serde_json::to_value(cap).unwrap_or(Value::Null)
}

pub fn handle_cap_set(
    core: &AppServices,
    _engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent = match params.get("agent").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'agent'"),
    };
    if let Err(e) = validate_agent_id(&agent) {
        return JsonRpcResponse::invalid_params(id, e.to_string());
    }
    let metric = match params.get("metric").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'metric'"),
    };
    if let Err(e) = validate_metric(&metric) {
        return JsonRpcResponse::invalid_params(id, e.to_string());
    }
    let threshold = match crate::adapters::ipc::handler::params::opt_f64(params, "threshold", &id) {
        Ok(Some(t)) if t > 0.0 => t,
        Err(e) => return e,
        _ => {
            return JsonRpcResponse::invalid_params(id, "'threshold' must be a positive number");
        }
    };
    let window_str = params
        .get("window")
        .and_then(|v| v.as_str())
        .unwrap_or("total");
    let window = match window_str.parse::<CapWindow>() {
        Ok(w) => w,
        Err(_) => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'window' '{window_str}' (total|1h|1d)"),
            );
        }
    };
    let action_str = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("notify");
    let action = match action_str.parse::<CapAction>() {
        Ok(a) => a,
        Err(_) => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'action' '{action_str}' (pause|require_approval|notify)"),
            );
        }
    };

    let cap = CostCap {
        id: generate_cap_id(core),
        agent,
        metric,
        threshold,
        window,
        action,
        created_at: now_ms(),
        triggered: None,
    };
    let saved = cap.clone();
    core.reply_store(
        id,
        params_bytes(params),
        move |s| save_cap(s, &saved),
        move |core, id, result| match result {
            Ok(()) => crate::adapters::ipc::handler::memory::written(core, id, cap_to_json(&cap)),
            Err(e) => JsonRpcResponse::error(id, -32603, e),
        },
    )
}

/// `telemetry.cap.list` — 전체 cap. 필터: `agent`.
pub fn handle_cap_list(
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
    core.reply_store(
        id,
        params_bytes(params),
        |s| load_all_caps(s),
        move |_, id, caps| {
            let mut caps = match caps {
                Ok(c) => c,
                Err(e) => return JsonRpcResponse::error(id, -32603, e),
            };
            if let Some(ref a) = agent_filter {
                caps.retain(|c| &c.agent == a);
            }
            caps.sort_by_key(|a| a.created_at);
            let arr: Vec<Value> = caps.iter().map(cap_to_json).collect();
            JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
        },
    )
}

pub fn handle_cap_remove(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let cap_id_str = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'id'"),
    };
    let key = cap_key(&cap_id_str);
    core.reply_store(
        id,
        params_bytes(params),
        move |s| s.delete(tasty_memory::HOST_OWNER, &Scope::Global, &key, None),
        move |core, id, result| match result {
            Ok(()) => crate::adapters::ipc::handler::memory::written(
                core,
                id,
                json!({ "removed": true, "id": cap_id_str }),
            ),
            Err(tasty_memory::MemoryError::NotFound { .. }) => {
                JsonRpcResponse::error(id, -32004, format!("not_found: {cap_id_str}"))
            }
            Err(e) => JsonRpcResponse::error(id, -32603, format!("memory delete failed: {e}")),
        },
    )
}

/// agent/metric/window의 이벤트를 집계한다. Set은 값을 교체하고 Inc/Dec는 더하거나 뺀다.
pub(super) fn compute_current_value(
    store: &dyn tasty_memory::MemoryStorage,
    cap: &CostCap,
) -> std::result::Result<f64, String> {
    let now = now_ms();
    let (since, until) = match cap.window.span_ms() {
        Some(span) => (Some(now.saturating_sub(span)), Some(now)),
        None => (None, None),
    };
    let filter = QueryFilter {
        metric: Some(cap.metric.clone()),
        agent: Some(cap.agent.clone()),
        workspace_id: None,
        since,
        until,
    };
    let events = collect_events(store, &filter)?;
    if events.is_empty() {
        return Ok(0.0);
    }
    let summaries = summarize_events(events);
    // cap은 agent 전체에 적용하므로 workspace별로 나누지 않는다.
    let sum: f64 = summaries.iter().map(|s| s.sum).sum();
    Ok(sum)
}

/// `telemetry.cap.status` — agent 별 cap 들의 현재 값/임계/triggered 상태.
pub fn handle_cap_status(
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
    // cap 목록과 각 cap 의 현재 값을 같은 writer 순서에서 읽는다.
    core.reply_store(
        id,
        params_bytes(params),
        move |s| {
            let caps = load_all_caps(s)?;
            let mut out = Vec::new();
            for cap in caps {
                if let Some(ref a) = agent_filter
                    && &cap.agent != a
                {
                    continue;
                }
                match compute_current_value(s, &cap) {
                    Ok(current) => out.push((cap, current)),
                    Err(e) => tracing::warn!("cap status: compute failed for {}: {e}", cap.id),
                }
            }
            Ok(out)
        },
        |_, id, found: std::result::Result<Vec<(CostCap, f64)>, String>| match found {
            Ok(found) => cap_status_response(id, found),
            Err(e) => JsonRpcResponse::error(id, -32603, e),
        },
    )
}

fn cap_status_response(id: Value, found: Vec<(CostCap, f64)>) -> JsonRpcResponse {
    let mut out: Vec<Value> = Vec::new();
    for (cap, current) in &found {
        let (cap, current) = (cap, *current);
        let ratio = if cap.threshold > 0.0 {
            current / cap.threshold
        } else {
            0.0
        };
        let mut entry = serde_json::to_value(cap).unwrap_or(Value::Null);
        if let Some(obj) = entry.as_object_mut() {
            obj.insert(
                "current_value".into(),
                Value::from(serde_json::Number::from_f64(current).unwrap_or_else(|| 0.into())),
            );
            obj.insert(
                "ratio".into(),
                Value::from(serde_json::Number::from_f64(ratio).unwrap_or_else(|| 0.into())),
            );
        }
        out.push(entry);
    }
    JsonRpcResponse::success(id, json!({ "entries": out, "count": out.len() }))
}

/// `telemetry.cap.reset` — `triggered` 상태 제거. `id` 또는 `agent` 둘 중 하나 필수.
pub fn handle_cap_reset(
    core: &AppServices,
    _engine: &crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let by_id = params.get("id").and_then(|v| v.as_str()).map(String::from);
    let by_agent = params
        .get("agent")
        .and_then(|v| v.as_str())
        .map(String::from);
    if by_id.is_none() && by_agent.is_none() {
        return JsonRpcResponse::invalid_params(id, "Provide 'id' or 'agent'");
    }
    core.reply_store(
        id,
        params_bytes(params),
        move |s| {
            let mut caps = load_all_caps(s)?;
            let mut reset_ids: Vec<String> = Vec::new();
            for cap in caps.iter_mut() {
                let matches = match (&by_id, &by_agent) {
                    (Some(i), _) => &cap.id == i,
                    (None, Some(a)) => &cap.agent == a,
                    _ => false,
                };
                if !matches || cap.triggered.is_none() {
                    continue;
                }
                cap.triggered = None;
                if let Err(e) = save_cap(s, cap) {
                    tracing::warn!("cap reset: save failed for {}: {e}", cap.id);
                    continue;
                }
                reset_ids.push(cap.id.clone());
            }
            Ok(reset_ids)
        },
        |core, id, reset: std::result::Result<Vec<String>, String>| match reset {
            Ok(reset_ids) => crate::adapters::ipc::handler::memory::written(
                core,
                id,
                json!({ "reset_ids": reset_ids, "count": reset_ids.len() }),
            ),
            Err(e) => JsonRpcResponse::error(id, -32603, e),
        },
    )
}

pub(super) fn cap_matches_untriggered(cap: &CostCap, ev: &TelemetryEvent) -> bool {
    cap.agent == ev.agent && cap.metric == ev.metric && cap.triggered.is_none()
}

/// Notify는 알림, RequireApproval은 승인 요청, Pause는 알림을 만든다.
/// Pause/RequireApproval의 이후 IPC 차단은 check_cap_block이 담당한다.
pub(super) fn fire_cap_action(
    core: &mut AppServices,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    cap: &CostCap,
    current: f64,
) {
    match cap.action {
        CapAction::Notify => fire_notify(window, out, engine, cap, current),
        CapAction::RequireApproval => {
            fire_require_approval(core, window, out, engine, cap, current)
        }
        CapAction::Pause => {
            // 차단은 진입 검사에서 하고 여기서는 사용자에게 이유를 알린다.
            fire_notify(window, out, engine, cap, current);
            tracing::info!(
                "cap triggered (action {:?}): cap={} agent={} metric={} value={} threshold={}",
                cap.action,
                cap.id,
                cap.agent,
                cap.metric,
                current,
                cap.threshold,
            );
        }
    }
}

/// 처음 상한에 도달하면 승인을 요청한다. 승인 후 cap.reset으로 해제해야 호출을 재개한다.
/// triggered가 있는 동안 재발행하지 않는다. reset 후 다시 상한에 도달하면 새로 요청한다.
pub(super) fn fire_require_approval(
    core: &mut AppServices,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    cap: &CostCap,
    current: f64,
) {
    let ws_id = engine
        .workspace_at(window.active_workspace_index(engine))
        .map(|w| w.id);
    let title = format!("Cap '{}' — 승인 필요", cap.metric);
    let body = format!(
        "agent={} metric={} value={} ≥ threshold={} (window={:?}, cap={}). \
         승인하면 `tasty telemetry cap reset --id {}` 으로 해제하세요.",
        cap.agent, cap.metric, current, cap.threshold, cap.window, cap.id, cap.id,
    );
    let req = tasty_approval::ApprovalRequest {
        id: tasty_approval::ApprovalId::generate(),
        requester: tasty_approval::Requester::Agent {
            id: tasty_memory::HOST_OWNER.to_string(),
        },
        workspace_id: ws_id,
        surface_id: None,
        title,
        body: Some(body),
        choices: Vec::new(),
        default_choice: None,
        timeout_ms: None,
        severity: tasty_approval::Severity::Warn,
        created_at: 0,
        metadata: serde_json::json!({
            "source": "telemetry.cap",
            "cap_id": cap.id,
            "agent": cap.agent,
            "metric": cap.metric,
            "value": current,
            "threshold": cap.threshold,
        }),
        host_elevation: None,
    };
    match core.request_approval(req) {
        Ok(transition) => {
            transition.record.detach(core.memory_access());
            let change = transition.change;
            // 먼저 발생한 알림보다 승인 팝업이 앞서지 않도록 outbox를 먼저 창 큐로 옮긴다.
            window.enqueue_intents(std::mem::take(out));
            #[cfg(feature = "gui")]
            window.enqueue_approval_popup(engine, &change.record);
            tracing::info!(
                "cap require_approval: issued approval id={} for cap={}",
                change.record.request.id.as_str(),
                cap.id,
            );
        }
        Err(e) => {
            tracing::warn!(
                "cap require_approval: approval.request failed for cap={}: {e}",
                cap.id
            );
        }
    }
}

/// IPC를 거치지 않고 활성 workspace에 알림 intent를 추가한다.
pub(super) fn fire_notify(
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    cap: &CostCap,
    current: f64,
) {
    let Some(ws) = engine.workspace_at(window.active_workspace_index(engine)) else {
        tracing::warn!("cap notify: no active workspace, skipping cap {}", cap.id);
        return;
    };
    let ws_id = ws.id;
    let title = format!("Cap '{}' 임계 도달", cap.metric);
    let body = format!(
        "agent={} metric={} value={} ≥ threshold={} (window={:?}, cap={})",
        cap.agent, cap.metric, current, cap.threshold, cap.window, cap.id,
    );
    out.push(
        crate::app::command::DomainIntent::PushNotification {
            level: crate::notification::NotificationLevel::Warning,
            ws_id,
            surface_id: 0,
            title,
            body,
            source: "telemetry.cap".to_string(),
            merge: crate::app::command::NotificationMerge::Coalesce,
            stream_echo: false,
        }
        .from_system(),
    );
}
