//! html surface 상태 조회.
//!
//! - `surface.html_script`: 스크립트·원격 콘텐츠의 감지·허용·안내 상태(docs/surfaces/html/index.md). 배너 표지를 갱신하는
//!   `update_banner`가 아니라 `banner_phase`를 부르므로 조회가 배너를 띄우거나 사용자 열람으로
//!   기록되지 않는다.
//! - `surface.html_nav`: commit된 문서 주소와 히스토리 이동 가능 여부. 호스트가 프레임마다 옮긴
//!   backend 관측값의 사본을 읽는다.
//!
//! 둘 다 읽기만 한다.

use serde_json::{Value, json};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_model::html_script::HtmlScriptState;

use super::require_surface_id;
use crate::model::{NavState, Surface};
use crate::plugin_bridge::remote_surface::RemoteSurface;

/// params의 surface_id로 html surface를 찾는다. 없거나 html이 아니면 invalid params로 답한다.
pub(super) fn find_html<'a>(
    engine: &'a crate::runtime::engine_access::EngineRef<'_>,
    id: &Value,
    params: &Value,
) -> Result<(u32, &'a RemoteSurface), JsonRpcResponse> {
    let surface_id = require_surface_id(params, id)?;
    let Some(surface) = engine.find_surface_by_id(surface_id) else {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("Surface {surface_id} not found"),
        ));
    };
    surface
        .as_any()
        .downcast_ref::<RemoteSurface>()
        .filter(|rs| rs.kind_static == "html")
        .map(|rs| (surface_id, rs))
        .ok_or_else(|| {
            JsonRpcResponse::invalid_params(
                id.clone(),
                format!("Surface {surface_id} is not an html surface"),
            )
        })
}

pub(crate) fn handle_html_script(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let (surface_id, rs) = match find_html(engine, &id, params) {
        Ok(found) => found,
        Err(e) => return e,
    };
    let mut result = rs.with_html_script(|st| state_json(st));
    result["surface_id"] = json!(surface_id);
    JsonRpcResponse::success(id, result)
}

pub(crate) fn handle_html_nav(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let (surface_id, rs) = match find_html(engine, &id, params) {
        Ok(found) => found,
        Err(e) => return e,
    };
    let info = rs.webview_nav();
    let nav_state = match rs.nav_state() {
        NavState::Idle => "idle",
        NavState::Loading => "loading",
        NavState::Done => "done",
        NavState::Failed => "failed",
    };
    JsonRpcResponse::success(
        id,
        json!({
            "surface_id": surface_id,
            "url": info.url,
            "requested_url": rs.webview_url(),
            "nav_state": nav_state,
            "can_go_back": info.can_go_back,
            "can_go_forward": info.can_go_forward,
        }),
    )
}

fn state_json(st: &HtmlScriptState) -> Value {
    let document = st.current().map(|doc| {
        json!({
            "url": doc.url,
            "detection": doc.scan.map(|s| s.detection.as_str()),
            "fingerprint": doc.scan.map(|s| s.fingerprint.to_hex()),
        })
    });
    let allowance = st.allowance().map(|a| {
        json!({
            "url": a.url(),
            "fingerprint": a.fingerprint().map(|f| f.to_hex()),
        })
    });
    let flags = st.banner();
    json!({
        "sandbox": st.sandbox(),
        "document": document,
        "allowance": allowance,
        "allowed": st.current_is_allowed(),
        "web_page": st.current_is_web_page(),
        "javascript": st.effective_js(),
        "loading": st.loading_before_commit(),
        "scanning": st.scan_pending(),
        "banner": {
            "phase": st.banner_phase().as_str(),
            "viewed": flags.viewed,
            "pending_view": flags.pending_view,
            "shown": flags.shown,
            "dismissed": flags.dismissed,
        },
        "remote": remote_json(st),
    })
}

/// 원격 콘텐츠 차단 상태. 조회만 하며 안내 표지를 바꾸지 않는다.
fn remote_json(st: &HtmlScriptState) -> Value {
    let allowance = st.remote_allowance().map(|a| {
        json!({
            "url": a.url(),
            "fingerprint": a.fingerprint().map(|f| f.to_hex()),
        })
    });
    let flags = st.remote_banner();
    let scan = st.current().and_then(|doc| doc.scan);
    json!({
        "global": st.remote_global(),
        "supported": st.remote_supported(),
        "view": st.remote_view().as_str(),
        "refs": scan.map(|s| s.remote_refs),
        "remote_scripts": scan.map(|s| s.remote_scripts),
        "observed": st.remote_observed(),
        "allowance": allowance,
        "allowed": st.current_is_remote_allowed(),
        "effective": st.remote_effectively_allowed(),
        "marker": st.remote_marker().map(|m| m.as_str()),
        "banner": {
            "phase": st.remote_banner_phase().as_str(),
            "viewed": flags.viewed,
            "pending_view": flags.pending_view,
            "shown": flags.shown,
            "dismissed": flags.dismissed,
        },
    })
}

#[cfg(test)]
pub(super) mod tests;
