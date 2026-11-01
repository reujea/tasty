//! Attach the client script gate before the native document starts.
use super::*;

/// html surface에 문서 단위 스크립트 허용을 붙인다(docs/surfaces/html/index.md). 첫 로드 전에 sandbox 값을 맞춘다.
pub(super) fn attach_script_gate(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    wv: &crate::webview::PlatformWebView,
    settings: &crate::webview::HtmlWebViewSettings,
) {
    let Some(rs) = find_remote_surface(view, engine, sid).filter(|rs| rs.kind_static == "html")
    else {
        return;
    };
    // 작업 스레드의 스캔이 끝나면 이 윈도우를 다시 그려 배너를 갱신하고 미룬 결정을 진행한다.
    let proxy = view.proxy.clone();
    let window_id = view.base.winit.id();
    let gate =
        crate::webview::script_gate::ScriptGate::new(sid, std::sync::Arc::clone(&rs.html_script))
            .with_notify(std::sync::Arc::new(move || {
                if proxy
                    .send_event(crate::AppEvent::EguiRepaint { window_id })
                    .is_err()
                {
                    tracing::debug!("html script scan finished after the event loop closed");
                }
            }));
    let gate = if let Some(source) = wv.entry_response_source() {
        gate.with_received_source(source)
    } else {
        gate
    };
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    let gate = if let Some(session) = &rs.http_session {
        gate.with_http_session(session.clone())
    } else {
        gate
    };
    gate.set_sandbox(!settings.javascript_enabled);
    gate.set_remote_global(settings.allow_remote_content);
    wv.attach_script_gate(gate);
}
