//! 처음 보일 때 만드는 WebView — 만들어졌다는 통지에 문서를 다시 싣는다.
//!
//! 매니페스트의 `lazy_webview = true` 로 호스트는 이 탭이 처음 화면에 나올 때 WebView 를 만든다.
//! 복원된 탭 대부분은 한 번도 안 보이므로 탭마다 뜨던 렌더링 프로세스가 그만큼 줄어든다. 대신 만들어지기
//! 전에 보낸 `webview.post_message`(증분)는 호스트가 버린다. 증분을 되살리지 않고 지금 대화로 문서
//! 전체를 다시 싣는다 — 대화 상태는 이 plugin 이 들고 있으므로 같은 화면이 나온다.

use serde_json::Value;

use crate::ports::host::HostCall;

/// 호스트가 lazy WebView 를 만든 직후 kind 소유 plugin 에만 보내는 이벤트 키.
pub const WEBVIEW_CREATED_EVENT: &str = "surface.webview_created";

/// 통지가 가리키는 surface.
pub fn surface_of(payload: &Value) -> Option<u32> {
    payload
        .get("surface_id")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// WebView 가 새로 생겼다 — 그 탭의 지금 문서를 싣는다. 모르는 탭이면(이미 닫힘) 그냥 둔다.
    pub(crate) fn webview_created(&self, host: &impl HostCall, payload: &Value) {
        let Some(sid) = surface_of(payload) else {
            tracing::warn!("[agent_session] {WEBVIEW_CREATED_EVENT} 모양이 틀리다: {payload}");
            return;
        };
        let Some(c) = self.convos.get(&sid) else {
            tracing::debug!("[agent_session] surface={sid}: 모르는 탭의 WebView 생성 통지");
            return;
        };
        tracing::info!("[agent_session] surface={sid}: WebView 가 생겨 문서를 다시 싣는다");
        crate::application::session_surface::push(
            host,
            &self.sup,
            sid,
            c,
            self.activity.is_busy(c),
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::application::surface_link::tests_support::FakeHost;

    #[test]
    fn a_created_webview_gets_the_whole_document_again() {
        let mut plugin = crate::test_support::TestPlugin::default();
        plugin
            .convos
            .insert(7, crate::domain::convo::Convo::default());
        let host = FakeHost::new();
        plugin.webview_created(&host, &json!({ "surface_id": 7 }));
        assert_eq!(host.set_url_calls(), 1, "문서를 다시 싣지 않았다");
    }

    #[test]
    fn an_unknown_or_malformed_notice_loads_nothing() {
        let mut plugin = crate::test_support::TestPlugin::default();
        plugin
            .convos
            .insert(7, crate::domain::convo::Convo::default());
        let host = FakeHost::new();
        plugin.webview_created(&host, &json!({ "surface_id": 8 }));
        plugin.webview_created(&host, &json!({ "surface": 7 }));
        plugin.webview_created(&host, &json!({ "surface_id": -1 }));
        assert_eq!(host.set_url_calls(), 0);
    }

    /// 이벤트 키가 호스트 상수와 같아야 통지가 닿는다 — 갈리면 위 검사는 통과해도 실물은 안 그린다.
    #[test]
    fn the_event_key_matches_the_host() {
        let host = include_str!("../../../../src/app/webview_sync/lazy.rs");
        assert!(host.contains(&format!(
            "pub(crate) const WEBVIEW_CREATED_EVENT: &str = \"{WEBVIEW_CREATED_EVENT}\";"
        )));
    }

    /// `on_event` 가 이 키를 받아 여기로 넘겨야 한다 — 처리기만 있고 배선이 빠지면 위 검사는 통과한다.
    #[test]
    fn on_event_routes_the_notice_here() {
        let main = crate::test_support::wiring_source();
        let body = &main[main.find("fn on_host_event(").unwrap()..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        assert!(body.contains("lazy_webview::WEBVIEW_CREATED_EVENT"));
        assert!(body.contains("self.webview_created("));
    }

    /// 매니페스트가 켜 둬야 호스트가 처음 보일 때 만든다.
    #[test]
    fn the_manifest_opts_in() {
        let manifest = include_str!("../../tasty-plugin.toml");
        assert!(manifest.contains("\nlazy_webview = true\n"));
    }
}
