//! 도구 메뉴의 사용자 클릭을 재현하는 디버그 전용 IPC. release에서는 컴파일하지 않는다.

#![cfg(debug_assertions)]

use crate::runtime::engine_access::EngineMut;
use serde_json::json;

use crate::plugin::manifest::ToolAction;
use crate::plugin::tool_registry::ToolSource;
use crate::state::RequestContext;
use tasty_ipc::protocol::JsonRpcResponse;

/// `debug.tool.list` — 현재 도구 메뉴에 표시되는 모든 항목을 정렬된 순서로 반환.
pub fn handle_list(
    state: &RequestContext,
    _engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let items: Vec<_> = state
        .tool_registry
        .visible_items()
        .into_iter()
        .map(|item| {
            let ToolSource::Plugin { plugin_id, tool_id } = &item.source;
            let source = json!({
                "kind": "plugin",
                "plugin_id": plugin_id,
                "tool_id": tool_id,
            });
            let action = match &item.action {
                ToolAction::Event { event_key } => {
                    json!({ "kind": "event", "event_key": event_key })
                }
                ToolAction::OpenSurface { surface_kind } => {
                    json!({ "kind": "open_surface", "surface_kind": surface_kind })
                }
                ToolAction::OpenPopup { popup_id } => {
                    json!({ "kind": "open_popup", "popup_id": popup_id })
                }
            };
            json!({
                "key": item.key,
                "source": source,
                "label_i18n_key": item.label_i18n_key,
                "icon": item.icon,
                "action": action,
                "order_hint": item.order_hint,
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "items": items }))
}

/// `debug.tool.invoke` — `params.key`로 항목을 찾아 사용자 클릭과 동일한 동작을 수행.
/// 항목을 찾지 못하면 invalid_params로 거부.
pub fn handle_invoke(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(key) = params.get("key").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'key' parameter");
    };
    let Some(item) = state.tool_registry.find(key) else {
        return JsonRpcResponse::error(id, -32602, format!("tool item '{key}' not found"));
    };
    if let Err(blocked) = crate::adapters::ui::tools_menu::invoke_tool(state, &engine.read(), &item)
    {
        return JsonRpcResponse::error(
            id,
            -32602,
            format!("tool item '{key}' cannot run: {}", blocked.message()),
        );
    }
    JsonRpcResponse::success(id, json!({ "invoked": key }))
}
