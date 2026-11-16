//! `debug.webview.snapshot` — native WebView의 그림과 문서 계산값을 파일로 남긴다.
//! `ui.screenshot`은 구조상 WebView를 담지 않고, OS 화면 캡처는 macOS에서 화면 기록 권한이 필요하다.
//! 이 메서드는 WebView 자체에 그림을 요청하므로 권한 없이 동작한다. 지금은 macOS 백엔드만 구현했다.

use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};

impl App {
    pub(super) fn ipc_handle_debug_webview_snapshot(&mut self, cmd: &IpcCommand) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let response = match self.debug_webview_snapshot(&cmd.request.params, &id) {
            Ok(body) => JsonRpcResponse::success(id, body),
            Err(response) => response,
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    fn debug_webview_snapshot(
        &mut self,
        params: &serde_json::Value,
        id: &serde_json::Value,
    ) -> Result<serde_json::Value, JsonRpcResponse> {
        let sid = params::require_u32(params, "surface_id", id)?;
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|p| std::path::Path::new(p).is_absolute())
            .ok_or_else(|| {
                JsonRpcResponse::invalid_params(id.clone(), "'path' must be an absolute path")
            })?;
        let Some(webview) = self
            .view
            .views
            .values()
            .filter_map(|v| v.as_main())
            .find_map(|main| main.webviews.get(&sid))
        else {
            return Err(JsonRpcResponse::error(
                id.clone(),
                -32000,
                format!("surface {sid} has no native WebView"),
            ));
        };
        snapshot(webview, std::path::Path::new(path), id)?;
        Ok(serde_json::json!({
            "surface_id": sid,
            "png": format!("{path}.png"),
            "json": format!("{path}.json"),
        }))
    }
}

#[cfg(target_os = "macos")]
fn snapshot(
    webview: &crate::webview::PlatformWebView,
    base: &std::path::Path,
    _id: &serde_json::Value,
) -> Result<(), JsonRpcResponse> {
    webview.debug_snapshot(base);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn snapshot(
    _webview: &crate::webview::PlatformWebView,
    _base: &std::path::Path,
    id: &serde_json::Value,
) -> Result<(), JsonRpcResponse> {
    Err(JsonRpcResponse::error(
        id.clone(),
        -32000,
        "debug.webview.snapshot is implemented only on macOS",
    ))
}
