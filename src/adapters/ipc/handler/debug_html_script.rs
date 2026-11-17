//! html surface 사용자 탐색의 debug 재현 명령.
//!
//! 주소 입력·뒤로·앞으로·새로고침·중지 조작을 재현하므로 debug 빌드에만 둔다. 스크립트 허용은
//! release 메서드 `surface.html_script_allow`가 맡는다(docs/surfaces/html/index.md).
//! 주소 입력·뒤로·앞으로·새로고침은 release 사용자 경로(`webview_user_nav`)를 그대로 쓴다.

use serde_json::{Value, json};

use crate::ipc::protocol::JsonRpcResponse;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::state::RequestContext;
use crate::webview::WebViewUserNav;

fn find_html_surface<'a>(
    engine: &'a crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    id: &Value,
) -> Result<&'a RemoteSurface, JsonRpcResponse> {
    let Some(surface) = engine.find_surface_by_id(sid) else {
        return Err(JsonRpcResponse::error(
            id.clone(),
            -32000,
            "surface_id not found",
        ));
    };
    match surface.as_any().downcast_ref::<RemoteSurface>() {
        Some(rs) if rs.capabilities.script_gate => Ok(rs),
        _ => Err(JsonRpcResponse::error(
            id.clone(),
            -32000,
            "surface is not an html surface",
        )),
    }
}

/// `debug.webview.history` — 사용자 탐색 조작을 재현한다. 다음 redraw에서 적용한다.
/// `open`은 주소 표시줄 입력처럼 해석을 마친 주소(`file://`·`http(s)://`)만 받는다.
pub(super) fn handle_history(
    state: &mut RequestContext,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let sid = match super::require_surface_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let url = params.get("url").and_then(|v| v.as_str());
    let action = params.get("action").and_then(|v| v.as_str());
    let nav = match (action, url) {
        (Some("open"), Some(url)) if crate::webview::is_navigable_url(url) => {
            Some(WebViewUserNav::Open(url.to_string()))
        }
        (Some("open"), _) => {
            return JsonRpcResponse::invalid_params(
                id,
                "'open' needs a 'url' with a file://, http:// or https:// scheme",
            );
        }
        (Some("back"), _) => Some(WebViewUserNav::Back),
        (Some("forward"), _) => Some(WebViewUserNav::Forward),
        (Some("reload"), _) => Some(WebViewUserNav::Reload),
        (Some("stop"), _) => None,
        _ => {
            return JsonRpcResponse::invalid_params(
                id,
                "'action' must be one of open, back, forward, reload, stop",
            );
        }
    };
    if let Err(e) = find_html_surface(engine, sid, &id) {
        return e;
    }
    match nav {
        Some(nav) => state.webview_user_nav.push((sid, nav)),
        None => state.debug_webview_stop.push(sid),
    }
    JsonRpcResponse::success(id, json!({ "queued": true }))
}
