//! `approval` IPC: summary 도메인.

use super::*;
use crate::adapters::ipc::handler::params::require_u32;
use crate::core::Core;

pub fn handle_summary_set(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match require_u32(params, "workspace_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let content = match params.get("content").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing 'content' (string)"),
    };
    let scope = Scope::Workspace(workspace_id);
    let value = MemoryValue::Text(content);
    let opts = PutOpts {
        expires_at: None,
        cas: None,
    };
    match core.with_memory(|s| s.put(tasty_memory::HOST_OWNER, &scope, SUMMARY_KEY, &value, &opts))
    {
        Ok(_) => crate::adapters::ipc::handler::memory::written(
            core,
            id,
            json!({ "workspace_id": workspace_id }),
        ),
        Err(e) => JsonRpcResponse::error(id, -32603, format!("summary set failed: {e}")),
    }
}

/// `approval.summary.get` — workspace 의 요약을 반환. 없으면 `content: null`.
pub fn handle_summary_get(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match require_u32(params, "workspace_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let scope = Scope::Workspace(workspace_id);
    match core.with_memory(|s| s.get(&scope, SUMMARY_KEY)) {
        Ok(Some(e)) => {
            let content = match e.value {
                MemoryValue::Text(t) => Some(t),
                MemoryValue::Json(v) => v.as_str().map(str::to_string),
                _ => None,
            };
            JsonRpcResponse::success(
                id,
                json!({
                    "workspace_id": workspace_id,
                    "content": content,
                    "updated_at": e.updated_at,
                }),
            )
        }
        Ok(None) => {
            JsonRpcResponse::success(id, json!({ "workspace_id": workspace_id, "content": null }))
        }
        Err(e) => JsonRpcResponse::error(id, -32603, format!("summary get failed: {e}")),
    }
}
