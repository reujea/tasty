use serde_json::{Value, json};

use crate::adapters::ipc::handler::params::{self, p_try};
use crate::core::Core;
use tasty_agent::LeaseMode;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::super::memory::mark_durability;
use super::{agent_err_to_response, now_ms, workspace_id_param};

fn resource_param(params: &Value, id: &Value) -> Result<String, JsonRpcResponse> {
    params
        .get("resource")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), "Missing or empty 'resource'"))
}

fn holder_param(params: &Value, id: &Value) -> Result<String, JsonRpcResponse> {
    params
        .get("holder")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), "Missing or empty 'holder'"))
}

fn serialize<T: serde::Serialize>(id: Value, value: T) -> JsonRpcResponse {
    match serde_json::to_value(value) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, -32603, format!("serialize: {e}")),
    }
}

pub fn handle_lease_acquire(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let resource = match resource_param(params, &id) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let holder = match holder_param(params, &id) {
        Ok(h) => h,
        Err(e) => return e,
    };
    let ttl_ms = p_try!(params::opt_int::<u64>(params, "ttl_ms", &id));
    let mode = match params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("fail")
    {
        "fail" => LeaseMode::Fail,
        "block" => LeaseMode::Block,
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("'mode' must be 'fail' or 'block', got '{other}'"),
            );
        }
    };
    mark_durability(
        core,
        match core.lease_acquire(workspace_id, &resource, &holder, ttl_ms, mode, now_ms()) {
            Ok(outcome) => serialize(id, outcome),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_lease_release(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let resource = match resource_param(params, &id) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let holder = match holder_param(params, &id) {
        Ok(h) => h,
        Err(e) => return e,
    };
    mark_durability(
        core,
        match core.lease_release(workspace_id, &resource, &holder) {
            Ok(outcome) => serialize(id, outcome),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_lease_list(
    core: &Core,
    _engine: &mut crate::core::CoreState,
    _caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    match core.lease_list(workspace_id, now_ms()) {
        Ok(leases) => {
            JsonRpcResponse::success(id, json!({ "total": leases.len(), "leases": leases }))
        }
        Err(e) => agent_err_to_response(id, e),
    }
}

// ============================================================
// agent.task_reduce — 다른 task 결과 합성
// ============================================================
