//! `approval` IPC: read 도메인.

use super::*;
use crate::adapters::ipc::handler::params::{self, p_try};
use crate::core::Core;

pub fn handle_cancel(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => ApprovalId(s.to_string()),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'id'"),
    };
    match core.cancel_approval(engine, &req_id) {
        Ok(change) => {
            persist_record(core, &change.record);
            crate::adapters::ipc::handler::memory::written(core, id, record_to_json(&change.record))
        }
        Err(e) => map_error(id, e),
    }
}

/// `approval.await` 의 실제 본문 — 메인 스레드를 막지 않도록 워커 스레드에서
/// 호출한다. `ipc_dispatch_approval_await` 가 Arc<ApprovalStore> + memory port
/// Arc 를 클론해 thread::spawn 안에서 이 함수를 호출하고, 응답을 `response_tx`
/// 로 보낸다.
///
/// `timeout_ms` 가 0 또는 null 이면 record 의 `timeout_ms` 사용, 그것도 없으면 무한 대기.
pub fn await_blocking(
    store: &ApprovalStore,
    memory: &std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    rpc_id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => ApprovalId(s.to_string()),
        _ => return JsonRpcResponse::invalid_params(rpc_id, "Missing 'id'"),
    };
    let timeout_ms = match p_try!(params::opt_int::<u64>(params, "timeout_ms", &rpc_id)) {
        Some(0) => None,
        Some(v) => Some(v),
        None => store.get(&req_id).and_then(|r| r.request.timeout_ms),
    };
    let outcome = store.await_response(&req_id, timeout_ms);
    // 상태 전이(timeout 자동 전이 포함) 가 있었으면 영속. worker thread 의
    // memory port arc 로 lock 후 한 번에 처리.
    if let Some(record) = store.get(&req_id) {
        persist_record_via_arc(memory, &record);
    }
    match outcome {
        Ok(WaitOutcome::Responded {
            choice,
            by,
            comment,
        }) => JsonRpcResponse::success(
            rpc_id,
            json!({
                "outcome": "responded",
                "choice": choice,
                "by": by,
                "comment": comment,
            }),
        ),
        Ok(WaitOutcome::TimedOut { default_choice }) => JsonRpcResponse::success(
            rpc_id,
            json!({
                "outcome": "timed_out",
                "default_choice": default_choice,
            }),
        ),
        Ok(WaitOutcome::Cancelled) => {
            JsonRpcResponse::success(rpc_id, json!({ "outcome": "cancelled" }))
        }
        Err(e) => map_error(rpc_id, e),
    }
}

/// `approval.await` — 블로킹 대기를 워커로 돌린다. store 선택은 호출자 몫
/// (`agent::task::spawn_task_await` 와 같은 이유).
pub(crate) fn spawn_approval_await(
    store: std::sync::Arc<tasty_approval::ApprovalStore>,
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    rpc_id: Value,
    params: Value,
    response_tx: &std::sync::mpsc::SyncSender<JsonRpcResponse>,
) {
    let response_tx = response_tx.clone();
    std::thread::spawn(move || {
        let resp = await_blocking(&store, &memory, rpc_id, &params);
        tasty_ipc::server::send_response(&response_tx, resp);
    });
}

/// `approval.get` — 단일 record 조회.
pub fn handle_get(
    _core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => ApprovalId(s.to_string()),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'id'"),
    };
    match engine.approval_store.get(&req_id) {
        Some(rec) => JsonRpcResponse::success(id, record_to_json(&rec)),
        None => JsonRpcResponse::success(id, Value::Null),
    }
}

/// `approval.list` — 전체 record. 필터: `state` (pending|responded|timed_out|cancelled|terminal),
/// `workspace_id`.
pub fn handle_list(
    _core: &Core,
    engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let state_filter = params.get("state").and_then(|v| v.as_str());
    let workspace_filter =
        match crate::adapters::ipc::handler::params::optional_u32(params, "workspace_id", &id) {
            Ok(v) => v,
            Err(e) => return e,
        };

    let mut records = engine.approval_store.list();
    if let Some(f) = state_filter {
        records.retain(|r| {
            use tasty_approval::ApprovalState as S;
            match (f, &r.state) {
                ("pending", S::Pending) => true,
                ("responded", S::Responded { .. }) => true,
                ("timed_out", S::TimedOut { .. }) => true,
                ("cancelled", S::Cancelled { .. }) => true,
                ("terminal", s) => s.is_terminal(),
                _ => false,
            }
        });
    }
    if let Some(wid) = workspace_filter {
        records.retain(|r| r.request.workspace_id == Some(wid));
    }
    records.sort_by_key(|r| r.request.created_at);
    let arr: Vec<Value> = records.iter().map(record_to_json).collect();
    JsonRpcResponse::success(id, json!({ "entries": arr, "count": arr.len() }))
}

// ============================================================
// History — memory 의 `tasty.approval.<id>` 키 전체를 시간 기준으로 조회
// ============================================================

/// `approval.history` — 영속 기록 조회. memory 에서 모든 scope 의 approval 항목을
/// 읽어 필터링한다. 필터: `since`/`until` (unix ms, memory updated_at 기준),
/// `workspace_id`, `requester_id`, `decision`, `state`, `limit`.
///
/// 응답: `{ entries: [...], count, returned }`.
pub fn handle_history(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let since = p_try!(params::opt_i64(params, "since", &id));
    let until = p_try!(params::opt_i64(params, "until", &id));
    let workspace_filter =
        match crate::adapters::ipc::handler::params::optional_u32(params, "workspace_id", &id) {
            Ok(v) => v,
            Err(e) => return e,
        };
    let requester_filter = params
        .get("requester_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let decision_filter = params
        .get("decision")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let state_filter = params
        .get("state")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let limit = p_try!(params::opt_int::<usize>(params, "limit", &id));

    let scopes: Vec<String> = match core.with_memory(|s| s.scopes()) {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::error(id, -32603, format!("memory scopes failed: {e}"));
        }
    };

    let mut collected: Vec<ApprovalRecord> = Vec::new();
    for scope_str in scopes {
        let Ok(scope) = Scope::parse(&scope_str) else {
            continue;
        };
        if let Some(wid) = workspace_filter
            && !matches!(scope, Scope::Workspace(s) if s == wid)
        {
            continue;
        }
        let list_opts = tasty_memory::ListOpts {
            prefix: Some(APPROVAL_KEY_PREFIX.to_string()),
            limit: None,
            since,
            until,
            offset: None,
        };
        let Ok(entries) = core.with_memory(|s| s.list(&scope, &list_opts)) else {
            continue;
        };
        for entry in entries {
            // summary 키는 history 결과에서 제외 (3.5 에서 사용).
            if entry.key == "tasty.approval.summary" {
                continue;
            }
            let MemoryValue::Json(v) = entry.value else {
                continue;
            };
            let Ok(record) = serde_json::from_value::<ApprovalRecord>(v) else {
                continue;
            };
            collected.push(record);
        }
    }

    if let Some(ref rid) = requester_filter {
        collected.retain(|r| match &r.request.requester {
            tasty_approval::Requester::User => rid == "user",
            tasty_approval::Requester::Plugin { id } => id == rid,
            tasty_approval::Requester::Agent { id } => id == rid,
        });
    }
    if let Some(ref decision) = decision_filter {
        collected.retain(|r| match &r.state {
            tasty_approval::ApprovalState::Responded { choice, .. } => choice == decision,
            _ => false,
        });
    }
    if let Some(ref sf) = state_filter {
        use tasty_approval::ApprovalState as S;
        collected.retain(|r| {
            matches!(
                (sf.as_str(), &r.state),
                ("pending", S::Pending)
                    | ("responded", S::Responded { .. })
                    | ("timed_out", S::TimedOut { .. })
                    | ("cancelled", S::Cancelled { .. })
            ) || (sf == "terminal" && r.state.is_terminal())
        });
    }

    // 시간 역순 — 최신 응답이 위로.
    collected.sort_by(|a, b| {
        let ta = transition_at(&a.state).unwrap_or(a.request.created_at);
        let tb = transition_at(&b.state).unwrap_or(b.request.created_at);
        tb.cmp(&ta)
    });

    let total = collected.len();
    if let Some(n) = limit {
        collected.truncate(n);
    }
    let returned = collected.len();
    let arr: Vec<Value> = collected.iter().map(record_to_json).collect();
    JsonRpcResponse::success(
        id,
        json!({ "entries": arr, "count": total, "returned": returned }),
    )
}
