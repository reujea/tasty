use serde_json::json;

use super::params::{self, p_try};
use crate::core::structural_exec::{self, Closed, TabCreated};
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_pane_id;

fn require_tab_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    params::opt_int::<u32>(params, "tab_id", id)?.ok_or_else(|| {
        JsonRpcResponse::invalid_params(id.clone(), "Missing required 'tab_id' parameter")
    })
}

pub fn handle_tab_list(
    engine: &crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let pane_id = match require_pane_id(params, &id) {
        Ok(pid) => pid,
        Err(e) => return e,
    };
    let tabs: Vec<_> = if let Some(pane) = engine.find_pane_by_id(pane_id) {
        pane.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let surface = tab.surface();
                let surface_type = surface.type_name();
                let surface_id = surface.surface_id();
                let sids = tab.all_surface_ids();
                let mut entry = json!({
                    "id": tab.id,
                    "name": tab.name,
                    "active": i == pane.active_tab,
                    "type": surface_type,
                    "busy_count": engine.busy_count(&sids),
                });
                if let Some(sid) = surface_id {
                    entry["surface_id"] = json!(sid);
                }
                entry
            })
            .collect()
    } else {
        return JsonRpcResponse::invalid_params(id, format!("Pane {} not found", pane_id));
    };
    JsonRpcResponse::success(id, json!({ "pane_id": pane_id, "tabs": tabs }))
}

pub fn handle_tab_create(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let pane_id = match require_pane_id(params, &id) {
        Ok(pid) => pid,
        Err(e) => return e,
    };

    // 에이전트 경로 — 새 탭은 사용자가 보던 탭을 바꾸지 않는다(ADR-0502).
    match structural_exec::create_tab(core, window, engine, pane_id, params, false) {
        Ok(TabCreated {
            pane_id,
            surface_id,
            tab_count,
            active_tab,
        }) => JsonRpcResponse::success(
            id,
            json!({
                "pane_id": pane_id,
                "surface_id": surface_id,
                "tab_count": tab_count,
                "active_tab": active_tab,
            }),
        ),
        Err(f) => super::structural_failure_response(id, f),
    }
}

pub fn handle_tab_close(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let tab_id = match require_tab_id(params, &id) {
        Ok(tid) => tid,
        Err(e) => return e,
    };

    // Prevent closing a tab that contains the caller — handler 잔존 (caller 정보 params).
    // tab 단위 스코프: 닫기 영향 범위는 그 tab(과 하위 SurfaceGroup)뿐이므로, caller 가
    // 같은 Pane 의 "다른" tab에 있는 경우까지 막으면 안 된다(과잉 차단 버그, Pane 단위
    // 체크였던 이전 구현이 형제 tab 도 잘못 막았다).
    if let Some(caller) = super::caller_surface_id(params)
        && engine.find_tab_for_surface(caller) == Some(tab_id)
    {
        return JsonRpcResponse::invalid_params(
            id,
            "Cannot close a tab that contains your own surface. Use 'tasty close self' instead.",
        );
    }

    match structural_exec::close_tab(core, window, engine, tab_id) {
        Ok(Closed {
            id: tab_id,
            closed: true,
        }) => JsonRpcResponse::success(id, json!({ "closed": true, "tab_id": tab_id })),
        Ok(Closed { id: tab_id, .. }) => JsonRpcResponse::success(
            id,
            json!({
                "closed": false,
                "tab_id": tab_id,
                "reason": "tab not found or cannot close the last tab",
            }),
        ),
        Err(f) => super::structural_failure_response(id, f),
    }
}

pub fn handle_tab_move(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let pane_id = match require_pane_id(params, &id) {
        Ok(pid) => pid,
        Err(e) => return e,
    };
    let from = match p_try!(params::opt_int::<u64>(params, "from_index", &id)) {
        Some(f) => f as usize,
        None => return JsonRpcResponse::invalid_params(id, "Missing 'from_index' parameter"),
    };
    let to = match p_try!(params::opt_int::<u64>(params, "to_index", &id)) {
        Some(t) => t as usize,
        None => return JsonRpcResponse::invalid_params(id, "Missing 'to_index' parameter"),
    };

    match structural_exec::move_tab(core, engine, pane_id, from, to) {
        Ok(moved) => JsonRpcResponse::success(id, json!({ "moved": moved, "pane_id": pane_id })),
        Err(f) => super::structural_failure_response(id, f),
    }
}

// handle_open_markdown / handle_open_explorer removed: use handle_tab_create with type parameter
