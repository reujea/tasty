//! 사용자 입력을 재현하는 플러그인 팝업 IPC. 디버그 빌드에서만 제공한다.

#![cfg(debug_assertions)]

use serde_json::json;

use crate::plugin::PluginManager;
use tasty_ipc::protocol::JsonRpcResponse;

pub fn handle_list(mgr: Option<&PluginManager>, id: serde_json::Value) -> JsonRpcResponse {
    let Some(mgr) = mgr else {
        return JsonRpcResponse::success(id, json!({ "contributes": [], "instances": [] }));
    };
    let contributes: Vec<_> = mgr
        .plugin_popup_contributes()
        .into_iter()
        .map(|entry| {
            let trigger = match &entry.contribute.trigger {
                crate::plugin::manifest::PopupTrigger::Event { event_key } => {
                    json!({ "kind": "event", "event_key": event_key })
                }
                crate::plugin::manifest::PopupTrigger::Ipc => json!({ "kind": "ipc" }),
            };
            json!({
                "plugin_id": entry.plugin_id,
                "popup_id": entry.contribute.id,
                "trigger": trigger,
            })
        })
        .collect();
    let instances: Vec<_> = mgr
        .popup_instances()
        .map(|(inst_id, inst)| {
            json!({
                "instance_id": inst_id,
                "plugin_id": inst.plugin_id,
                "popup_id": inst.popup_id,
                // 호스트 팝업과 같은 순번을 써 겹침 순서를 비교할 수 있다.
                "z_seq": inst.z_seq,
                "scope": match inst.contribute.scope {
                    crate::plugin::manifest::PopupScopeDecl::Window => "window",
                    crate::plugin::manifest::PopupScopeDecl::Surface => "surface",
                },
                "scope_surface": inst.scope_surface,
            })
        })
        .collect();
    JsonRpcResponse::success(
        id,
        json!({ "contributes": contributes, "instances": instances }),
    )
}

/// `debug.popup.open` — `{ plugin_id, popup_id, surface_id?, context? }`로 popup 인스턴스 강제 open.
/// surface 선언 popup은 `surface_id`로 대상을 결속한다. 없으면 열지 않고 -32602다.
#[cfg(feature = "gui")]
pub fn handle_open(
    mgr: Option<&mut PluginManager>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(plugin_id) = params.get("plugin_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'plugin_id' parameter");
    };
    let Some(popup_id) = params.get("popup_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'popup_id' parameter");
    };
    let context = params
        .get("context")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let surface_id = match super::params::read_int::<u32>(params, "surface_id") {
        Ok(sid) => sid,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    };
    let Some(mgr) = mgr else {
        return JsonRpcResponse::error(id, -32002, "plugin manager not initialized");
    };
    if surface_id.is_none()
        && crate::plugin_bridge::popup_scope::declared_popup_scope(mgr, plugin_id, popup_id)
            == Some(crate::plugin::manifest::PopupScopeDecl::Surface)
    {
        return JsonRpcResponse::error(
            id,
            -32602,
            format!(
                "popup '{plugin_id}/{popup_id}' is declared with surface scope; surface_id required"
            ),
        );
    }
    match mgr.open_popup_instance(plugin_id, popup_id, context) {
        Some(instance_id) => {
            if let Some(sid) = surface_id {
                mgr.bind_popup_instance_surface(instance_id, sid);
            }
            JsonRpcResponse::success(id, json!({ "instance_id": instance_id }))
        }
        None => JsonRpcResponse::error(
            id,
            -32602,
            format!("popup '{plugin_id}/{popup_id}' not found or plugin not running"),
        ),
    }
}

// 닫기는 App::enqueue_plugin_popup_close를 통해 렌더의 닫기 큐에 넣어야 자식 피커도 정리된다.
// 헤드리스는 목록 조회만 제공한다. 닫기 경로가 없어 열기를 허용하지 않는다.
