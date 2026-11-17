//! html surface의 히스토리 이동(`surface.html_back`·`surface.html_forward`·`surface.html_reload`).
//!
//! 에이전트 경로다. 대상은 surface ID로만 받고 포커스·선택을 바꾸지 않는다. 이동은 이 창의
//! 큐에 넣고 다음 redraw의 WebView 동기화가 호스트 출처 로드로 실행한다.
//! 갈 곳이 있는지는 여기서 판정하지 않는다. 호스트의 관측 사본(`webview_nav`)은 redraw 때만
//! 갱신되어 이동 직후 늦을 수 있으므로, 적용할 때 backend 값으로 판정해 갈 곳이 없으면 버린다.

use serde_json::{Value, json};
use tasty_ipc::protocol::JsonRpcResponse;

use super::html_script::{address_band, find_html};
use crate::ipc::window_port::IpcWindow;
use crate::webview::WebViewHistoryStep;

pub(crate) fn handle_html_history(
    window: &mut dyn IpcWindow,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: Value,
    params: &Value,
    step: WebViewHistoryStep,
) -> JsonRpcResponse {
    let (surface_id, _) = match find_html(engine, &id, params, address_band) {
        Ok(found) => found,
        Err(e) => return e,
    };
    window.enqueue_webview_history(surface_id, step);
    JsonRpcResponse::success(id, json!({ "surface_id": surface_id, "queued": true }))
}

#[cfg(test)]
mod tests;
