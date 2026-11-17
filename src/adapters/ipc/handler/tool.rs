//! 도구 메뉴의 사용자 클릭을 재현하는 디버그 전용 IPC. release에서는 컴파일하지 않는다.

#![cfg(debug_assertions)]

use crate::runtime::engine_access::EngineMut;
use serde_json::json;

use crate::adapters::ui::tools_menu::ToolRun;
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
/// 항목을 찾지 못하면 invalid_params로 거부. 생성 대기 초안에 걸어 두었으면 그 초안 id를 함께 돌려준다.
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
    let read = engine.read();
    let target = state.command_surface_target(&read);
    match crate::adapters::ui::tools_menu::run_tool(state, &read, &item, target, None, |_| {}) {
        Err(blocked) => JsonRpcResponse::error(
            id,
            -32602,
            format!("tool item '{key}' cannot run: {}", blocked.message()),
        ),
        Ok(ToolRun::Ran) => JsonRpcResponse::success(id, json!({ "invoked": key })),
        Ok(ToolRun::Deferred(pending)) => JsonRpcResponse::success(
            id,
            json!({ "invoked": key, "deferred_pending_id": pending.0 }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::handle_invoke;
    use crate::plugin::manifest::ToolAction;
    use crate::plugin::tool_registry::{ToolItem, ToolSource};

    const SURFACE_POPUP: &str = "com.example.viewer/confirm";

    /// 생성 대기 초안이 대상이면 surface 범위 팝업 도구를 걸어 두고 성공으로 답하며 그 초안 id 를 싣는다.
    #[test]
    fn a_surface_popup_tool_on_a_pending_draft_is_deferred() {
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        state
            .tool_registry
            .set_surface_popups([SURFACE_POPUP.to_string()].into_iter().collect());
        state.tool_registry.set_plugin_items(vec![ToolItem {
            source: ToolSource::Plugin {
                plugin_id: "com.example.viewer".into(),
                tool_id: "t".into(),
            },
            key: "com.example.viewer/t".into(),
            label_i18n_key: String::new(),
            icon: None,
            action: ToolAction::OpenPopup {
                popup_id: SURFACE_POPUP.into(),
            },
            order_hint: 0,
        }]);
        let mut engine = session.borrow_mut();
        let _draft = crate::state::tests::select_pending_draft(&mut state, &engine.read());
        let params = serde_json::json!({ "key": "com.example.viewer/t" });
        let pending = state.attach_pending.entries[0].id;
        let response = handle_invoke(&mut state, &mut engine, serde_json::json!(1), &params);
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(
            response.result,
            Some(serde_json::json!({
                "invoked": "com.example.viewer/t",
                "deferred_pending_id": pending.0,
            }))
        );
        assert!(state.pending_popup_opens.is_empty());
        assert_eq!(state.attach_pending.deferred.waiting_for(pending), 1);
        assert!(state.take_pending_intents().is_empty());
    }

    /// 팔레트로 A 를 고른 뒤 debug.tool.invoke 로 B 를 넣으면 같은 초안에서 A, B 순서로 실행한다.
    #[test]
    fn a_palette_command_then_a_debug_tool_run_in_input_order() {
        use crate::adapters::ui::popup::command_palette::enabled_tests as palette;
        let (mut state, mut session) = palette::palette_with_popup_command();
        state.tool_registry.set_plugin_items(vec![ToolItem {
            source: ToolSource::Plugin {
                plugin_id: "com.example.viewer".into(),
                tool_id: "t".into(),
            },
            key: "com.example.viewer/t".into(),
            label_i18n_key: String::new(),
            icon: None,
            action: ToolAction::OpenPopup {
                popup_id: "com.example.viewer/list".into(),
            },
            order_hint: 0,
        }]);
        let mut engine = session.borrow_mut();
        let read = engine.read();
        let anchor = state.focused_surface_id(&read).unwrap();
        let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
        palette::press_enter_on_confirm(&mut state, &read);
        drop(read);
        let params = serde_json::json!({ "key": "com.example.viewer/t" });
        let response = handle_invoke(&mut state, &mut engine, serde_json::json!(1), &params);
        assert!(response.error.is_none(), "{:?}", response.error);
        let read = engine.read();
        crate::state::attach_pending::deferred::settle_first_created(&mut state, &read, anchor);
        crate::adapters::ui::tools_menu::run_deferred_tools(&mut state, &read);
        let order: Vec<_> = state
            .pending_popup_opens
            .iter()
            .map(|o| o.popup_id.as_str())
            .collect();
        assert_eq!(order, vec!["confirm", "list"]);
    }
}
