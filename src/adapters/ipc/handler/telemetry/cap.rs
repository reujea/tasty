//! `telemetry.cap.*` — cost cap (예산) 관리 + cap 평가/발동.

use serde_json::{Value, json};
use tasty_memory::{ListOpts, MemoryValue, PutOpts, Scope};
use tasty_telemetry::{
    CAP_KEY_PREFIX, CapAction, CapWindow, CostCap, TelemetryEvent, cap_key, summarize_events,
    validate_agent_id, validate_metric,
};

use crate::core::Core;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::now_ms;
use super::query::{QueryFilter, collect_events};

pub(super) fn generate_cap_id(engine: &mut crate::core::CoreState) -> String {
    let ts = now_ms();
    let seq = engine.telemetry_seq.next();
    format!("cap_{ts:013}{seq:04}", ts = ts, seq = seq % 10_000)
}

/// 모든 cap 을 memory 에서 읽어온다. cap 은 global scope 에만 저장.
pub(super) fn load_all_caps(core: &Core) -> std::result::Result<Vec<CostCap>, String> {
    let list_opts = ListOpts {
        prefix: Some(CAP_KEY_PREFIX.to_string()),
        limit: None,
        since: None,
        until: None,
        offset: None,
    };
    let entries = core
        .with_memory(|s| s.list(&Scope::Global, &list_opts))
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

pub(super) fn save_cap(core: &Core, cap: &CostCap) -> std::result::Result<(), String> {
    let key = cap_key(&cap.id);
    let value = MemoryValue::Json(serde_json::to_value(cap).map_err(|e| e.to_string())?);
    let opts = PutOpts {
        expires_at: None,
        cas: None,
    };
    core.with_memory(|s| {
        s.put(
            tasty_memory::HOST_OWNER,
            &Scope::Global,
            &key,
            &value,
            &opts,
        )
    })
    .map(|_| ())
    .map_err(|e| format!("memory put failed: {e}"))
}

pub(super) fn cap_to_json(cap: &CostCap) -> Value {
    serde_json::to_value(cap).unwrap_or(Value::Null)
}

/// `telemetry.cap.set` — cap 등록.
pub fn handle_cap_set(
    core: &Core,
    engine: &mut crate::core::CoreState,
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
        id: generate_cap_id(engine),
        agent,
        metric,
        threshold,
        window,
        action,
        created_at: now_ms(),
        triggered: None,
    };
    if let Err(e) = save_cap(core, &cap) {
        return JsonRpcResponse::error(id, -32603, e);
    }
    crate::adapters::ipc::handler::memory::written(core, id, cap_to_json(&cap))
}

/// `telemetry.cap.list` — 전체 cap. 필터: `agent`.
pub fn handle_cap_list(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_filter = params
        .get("agent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let mut caps = match load_all_caps(core) {
        Ok(c) => c,
        Err(e) => return JsonRpcResponse::error(id, -32603, e),
    };
    if let Some(ref a) = agent_filter {
        caps.retain(|c| &c.agent == a);
    }
    caps.sort_by_key(|a| a.created_at);
    let arr: Vec<Value> = caps.iter().map(cap_to_json).collect();
    JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
}

/// `telemetry.cap.remove` — cap 삭제.
pub fn handle_cap_remove(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let cap_id_str = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'id'"),
    };
    let key = cap_key(&cap_id_str);
    let result =
        core.with_memory(|s| s.delete(tasty_memory::HOST_OWNER, &Scope::Global, &key, None));
    match result {
        Ok(()) => crate::adapters::ipc::handler::memory::written(
            core,
            id,
            json!({ "removed": true, "id": cap_id_str }),
        ),
        Err(tasty_memory::MemoryError::NotFound { .. }) => {
            JsonRpcResponse::error(id, -32004, format!("not_found: {cap_id_str}"))
        }
        Err(e) => JsonRpcResponse::error(id, -32603, format!("memory delete failed: {e}")),
    }
}

/// agent + metric + window 의 현재 누적값을 raw events 에서 즉시 집계.
///
/// `Op::Set` 은 sum 을 통째 교체. `Op::Inc/Dec` 는 누적. 4.1 의 `summarize_events`
/// 와 동일 정책.
pub(super) fn compute_current_value(
    core: &Core,
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
    let events = collect_events(core, &filter)?;
    if events.is_empty() {
        return Ok(0.0);
    }
    let summaries = summarize_events(events);
    // metric/agent 가 단일이면 결과는 단일 entry. workspace_id 분리는 무시 — cap 은 agent 전체.
    let sum: f64 = summaries.iter().map(|s| s.sum).sum();
    Ok(sum)
}

/// `telemetry.cap.status` — agent 별 cap 들의 현재 값/임계/triggered 상태.
pub fn handle_cap_status(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_filter = params
        .get("agent")
        .and_then(|v| v.as_str())
        .map(String::from);
    let caps = match load_all_caps(core) {
        Ok(c) => c,
        Err(e) => return JsonRpcResponse::error(id, -32603, e),
    };
    let mut out: Vec<Value> = Vec::new();
    for cap in &caps {
        if let Some(ref a) = agent_filter
            && &cap.agent != a
        {
            continue;
        }
        let current = match compute_current_value(core, cap) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("cap status: compute failed for {}: {e}", cap.id);
                continue;
            }
        };
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
    core: &Core,
    _engine: &mut crate::core::CoreState,
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
    let mut caps = match load_all_caps(core) {
        Ok(c) => c,
        Err(e) => return JsonRpcResponse::error(id, -32603, e),
    };
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
        if let Err(e) = save_cap(core, cap) {
            tracing::warn!("cap reset: save failed for {}: {e}", cap.id);
            continue;
        }
        reset_ids.push(cap.id.clone());
    }
    crate::adapters::ipc::handler::memory::written(
        core,
        id,
        json!({ "reset_ids": reset_ids, "count": reset_ids.len() }),
    )
}

// ============================================================
// Cap 평가 / 액션 발화
// ============================================================

/// 매 record 후 호출되는 best-effort 후크. agent+metric 이 일치하고 아직
/// triggered 되지 않은 cap 들을 검사해 임계를 넘으면 `triggered` 마크 + 액션 발화.
///
/// 모든 실패는 warn 로그로만 — record 자체의 응답에는 영향이 없다.
///
/// `Notify`/`RequireApproval`/`Pause` 모두 여기서 발화된다 — `Pause`/`RequireApproval`
/// 의 실제 IPC 차단은 호출 전 [`check_cap_block`](super::check_cap_block) 이 담당한다.
pub(super) fn evaluate_caps_after_record(
    core: &mut Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    ev: &TelemetryEvent,
) {
    let caps = match load_all_caps(core) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cap eval: load failed: {e}");
            return;
        }
    };
    for mut cap in caps {
        if !cap_matches_untriggered(&cap, ev) {
            continue;
        }
        try_trigger_cap(core, window, out, engine, &mut cap);
    }
}

/// `ev` 와 agent+metric 이 일치하고 아직 triggered 되지 않은 cap 인가.
fn cap_matches_untriggered(cap: &CostCap, ev: &TelemetryEvent) -> bool {
    cap.agent == ev.agent && cap.metric == ev.metric && cap.triggered.is_none()
}

/// 현재 값을 계산해 임계를 넘었으면 `triggered` 마크 + 저장 + 액션 발화.
fn try_trigger_cap(
    core: &mut Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    cap: &mut CostCap,
) {
    let current = match compute_current_value(core, cap) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("cap eval: compute failed for {}: {e}", cap.id);
            return;
        }
    };
    if current < cap.threshold {
        return;
    }
    // 임계 초과 — triggered 마크 후 액션 발화.
    cap.triggered = Some(tasty_telemetry::CapTriggered {
        at: now_ms(),
        value: current,
    });
    if let Err(e) = save_cap(core, cap) {
        tracing::warn!("cap eval: save failed for {}: {e}", cap.id);
        return;
    }
    fire_cap_action(core, window, out, engine, cap, current);
}

/// cap 액션을 실제 시스템으로 발화. `Notify` 는 알림만; `RequireApproval` 은
/// approval.request 자동 발행 + IPC 차단; `Pause` 는 알림 + IPC 차단(차단 자체는
/// `check_cap_block` 이 담당) — memory 상의 `triggered` 필드는 이미 기록됐으므로
/// status 조회로도 확인 가능하다.
pub(super) fn fire_cap_action(
    core: &mut Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    cap: &CostCap,
    current: f64,
) {
    match cap.action {
        CapAction::Notify => fire_notify(window, out, engine, cap, current),
        CapAction::RequireApproval => {
            fire_require_approval(core, window, out, engine, cap, current)
        }
        CapAction::Pause => {
            // 차단은 dispatcher 의 check_cap_block 이 담당. 여기서는 사용자에게
            // 사실을 알리는 알림만 함께 띄운다 — 차단된 plugin 이 침묵 속에 멈춰
            // 보이지 않도록.
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

/// `RequireApproval` 액션: cap 이 처음 triggered 되는 시점에
/// host 가 approval.request 를 자동 발행한다. 이후 plugin 의 모든 IPC 는
/// `check_cap_block` 이 거부 — 사용자는 popup 에서 승인 후 `cap.reset` 으로
/// triggered 를 풀어야 plugin 이 재개된다 (또는 거부 후 그대로 둠).
///
/// 매 호출마다 새 approval 을 발행하지 않고, **cap-당 1회**만 발행 (triggered 가
/// 비어 있을 때만 fire_cap_action 가 호출되므로 자연스럽게 단발). 추가 발행이
/// 필요하면 `cap.reset` 후 다음 record 가 임계를 다시 넘을 때 fire 된다.
pub(super) fn fire_require_approval(
    core: &mut Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    cap: &CostCap,
    current: f64,
) {
    let ws_id = engine
        .workspaces
        .get(window.active_workspace_index())
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
    };
    match core.request_approval(engine, req) {
        Ok(change) => {
            crate::ipc::handler::approval::persist_record(core, &change.record);
            // 승인 팝업은 창 큐에 즉시 들어간다. 같은 평가에서 먼저 발화한 cap 의 알림이
            // 출구에 남아 있으면 팝업이 그것을 앞지르므로, 출구를 먼저 창 큐로 옮겨 발화
            // 순서대로 쌓이게 한다(ADR-0471).
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

/// `Notify` 액션: 활성 워크스페이스에 notification 추가 + host event enqueue.
/// notification.create 핸들러의 단순 경로와 동등하나 IPC 를 거치지 않는다.
pub(super) fn fire_notify(
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    cap: &CostCap,
    current: f64,
) {
    let Some(ws) = engine.workspaces.get(window.active_workspace_index()) else {
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
        crate::core::intent::DomainIntent::PushNotification {
            ws_id,
            surface_id: 0,
            title,
            body,
            source: "telemetry.cap".to_string(),
        }
        .from_system(),
    );
}

// ============================================================
// Anomaly
// ============================================================
