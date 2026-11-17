//! 창 없이 처리할 수 있는 debug IPC. GUI와 헤드리스가 공유한다.

use serde_json::Value;

use tasty_ipc::protocol::JsonRpcResponse;

/// Lua 워커에 실행을 예약한다. scheduled 응답은 완료·성공을 보장하지 않는다.
pub(crate) fn lua_eval(
    engine: Option<&crate::hooks::lua::LuaSend<'_>>,
    rpc_id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let source = params.get("source").and_then(|v| v.as_str());
    match (source, engine) {
        (Some(src), Some(eng)) => {
            eng.engine().run_script(src, Some("debug.lua.eval"));
            JsonRpcResponse::success(rpc_id, serde_json::json!({ "scheduled": true }))
        }
        (None, _) => JsonRpcResponse::invalid_params(rpc_id, "Missing 'source'"),
        (_, None) => JsonRpcResponse::error(rpc_id, -32603, "lua engine not initialized"),
    }
}

/// 창을 열지 않고 무대 선언을 조회한다. 제목은 로케일에 의존하지 않도록 번역 키로 반환한다.
pub(crate) fn fullscreen_list(id: Value) -> JsonRpcResponse {
    let stages: Vec<_> = crate::fullscreen_stages::all_metas()
        .iter()
        .map(|m| serde_json::json!({ "id": m.id, "title_key": m.title_key }))
        .collect();
    JsonRpcResponse::success(id, serde_json::json!({ "stages": stages }))
}
