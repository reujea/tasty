use serde_json::{Value, json};

use crate::adapters::ipc::handler::params::{self, p_try};
use crate::core::Core;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::super::memory::mark_durability;
use super::{agent_err_to_response, name_param, now_ms, workspace_id_param};

fn serialize<T: serde::Serialize>(id: Value, value: T) -> JsonRpcResponse {
    match serde_json::to_value(value) {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, -32603, format!("serialize: {e}")),
    }
}

pub fn handle_semaphore_create(
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
    let name = match name_param(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let permits = match p_try!(params::opt_int::<u64>(params, "permits", &id)) {
        Some(c) if c <= u32::MAX as u64 => c as u32,
        _ => {
            return JsonRpcResponse::invalid_params(
                id,
                "Missing or invalid 'permits' (must be u32 >= 1)",
            );
        }
    };
    mark_durability(
        core,
        match core.semaphore_create(workspace_id, name, permits, now_ms()) {
            Ok(s) => serialize(id, s),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_semaphore_set_permits(
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
    let name = match name_param(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let permits = match p_try!(params::opt_int::<u64>(params, "permits", &id)) {
        Some(c) if c <= u32::MAX as u64 => c as u32,
        _ => {
            return JsonRpcResponse::invalid_params(
                id,
                "Missing or invalid 'permits' (must be u32 >= 1)",
            );
        }
    };
    mark_durability(
        core,
        match core.semaphore_set_permits(workspace_id, &name, permits, now_ms()) {
            Ok(s) => serialize(id, s),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_semaphore_acquire(
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
    let name = match name_param(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let holder = match params.get("holder").and_then(|v| v.as_str()) {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing or empty 'holder'"),
    };
    let ttl_ms = match params::opt_int::<u64>(params, "ttl_ms", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    mark_durability(
        core,
        match core.semaphore_acquire(workspace_id, &name, &holder, ttl_ms, now_ms()) {
            Ok(outcome) => serialize(id, outcome),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_semaphore_release(
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
    let name = match name_param(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let holder = match params.get("holder").and_then(|v| v.as_str()) {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing or empty 'holder'"),
    };
    mark_durability(
        core,
        match core.semaphore_release(workspace_id, &name, &holder) {
            Ok(outcome) => serialize(id, outcome),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

pub fn handle_semaphore_list(
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
    match core.semaphore_list(workspace_id, now_ms()) {
        Ok(semaphores) => JsonRpcResponse::success(
            id,
            json!({ "total": semaphores.len(), "semaphores": semaphores }),
        ),
        Err(e) => agent_err_to_response(id, e),
    }
}

pub fn handle_semaphore_delete(
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
    let name = match name_param(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    mark_durability(
        core,
        match core.semaphore_delete(workspace_id, &name) {
            Ok(()) => JsonRpcResponse::success(id, json!({ "deleted": true })),
            Err(e) => agent_err_to_response(id, e),
        },
    )
}

// ============================================================
// agent.lease_*
// ============================================================
