//! 처음 보일 때 만드는 webview kind(`lazy_webview`)의 규칙 — 만들기 전 메시지 처리와 생성 통지.
//! 계약은 docs/design/systems/webview.md 의 "생성 시점" 절에 있다.

use crate::plugin::PluginManager;
use crate::view::MainView;

/// 아직 WebView 가 없는 surface 의 쌓인 메시지를 어떻게 할지.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ScriptsWithoutWebview {
    /// 다음 프레임에 만들어질 WebView 가 받는다.
    Keep,
    /// 처음 보일 때 만드는 kind — 보이기 전까지 끝없이 쌓이므로 버린다. 소유 plugin 은
    /// `surface.webview_created` 를 받고 문서 전체를 다시 싣는다.
    Drop,
}

pub(super) fn scripts_without_webview(lazy: bool) -> ScriptsWithoutWebview {
    if lazy {
        ScriptsWithoutWebview::Drop
    } else {
        ScriptsWithoutWebview::Keep
    }
}

/// lazy kind WebView 를 만든 뒤 소유 plugin 에 `surface.webview_created` 를 보낸다. 만들어지기 전의
/// `webview.post_message` 는 버려졌으므로 plugin 은 이 통지에 문서 전체를 다시 싣는다. 실린 문서
/// 자체는 마지막 `webview.set_url` 값이다.
pub(crate) fn announce_lazy_webviews(view: &mut MainView, plugins: Option<&mut PluginManager>) {
    let created = std::mem::take(&mut view.lazy_webviews_created);
    let Some(mgr) = plugins else {
        return;
    };
    for (surface_id, plugin_id) in created {
        tracing::debug!(
            "WebView surface {surface_id}: lazy webview created, notifying {plugin_id}"
        );
        mgr.emit_host_event_to_plugin(
            &plugin_id,
            WEBVIEW_CREATED_EVENT,
            &serde_json::json!({ "surface_id": surface_id }),
            tasty_plugin_protocol::EventScope::Surface,
        );
    }
}

/// 메모리 절약 — 소유 plugin이 [`RELEASE_HIDDEN_KEY`](crate::runtime::surface_registry::webview_kind::RELEASE_HIDDEN_KEY)
/// 를 켠 lazy kind는 가려진 WebView를 없앤다(렌더링 프로세스도 내려간다). 다시 보이면 생성 경로가 새로
/// 만들고 `surface.webview_created` 를 보내므로 plugin이 문서를 다시 싣는다. 페이지 안의 상태는 사라지므로
/// 남길 것은 plugin이 미리 받아 둔다.
pub(super) fn release_hidden_webviews(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    active_html: &std::collections::HashMap<u32, crate::webview::WebViewBounds>,
    proofs: &crate::app::html_runtime::NavigationProofs,
) {
    let hidden: Vec<u32> = view
        .webviews
        .keys()
        .copied()
        .filter(|sid| !active_html.contains_key(sid))
        .filter(|&sid| {
            super::webview_surface_kind(view, engine, sid)
                .and_then(crate::runtime::surface_registry::webview_kind::webview_settings_source)
                .is_some_and(|source| {
                    crate::host_api::webview::releases_hidden_webview(
                        &source,
                        &engine.runtime.settings,
                    )
                })
        })
        .collect();
    for sid in hidden {
        tracing::debug!("WebView surface {sid}: hidden lazy webview released");
        super::forget_webview(view, sid, proofs);
    }
}

/// lazy kind WebView 생성 통지 키. 받는 plugin 쪽 상수와 같아야 한다.
pub(crate) const WEBVIEW_CREATED_EVENT: &str = "surface.webview_created";

#[cfg(test)]
mod tests {
    use super::{ScriptsWithoutWebview, scripts_without_webview};

    #[test]
    fn a_lazy_kind_drops_messages_until_its_webview_exists() {
        assert_eq!(scripts_without_webview(true), ScriptsWithoutWebview::Drop);
    }

    #[test]
    fn an_eager_kind_keeps_messages_for_the_next_frame() {
        assert_eq!(scripts_without_webview(false), ScriptsWithoutWebview::Keep);
    }

    /// 판정을 실제 drain 이 거치는지 — 규칙만 있고 호출이 빠지면 위 두 검사는 계속 통과한다.
    #[test]
    fn the_drain_and_the_constructor_consult_the_lazy_rules() {
        let src = include_str!("../webview_sync.rs");
        let drain = &src[src.find("fn drain_pending_scripts(").unwrap()..];
        let drain = &drain[..drain.find("\n}\n").unwrap()];
        assert!(drain.contains("scripts_without_webview(lazy)"));
        assert!(drain.contains("rs.take_scripts().len()"));
        let ctor = include_str!("construction.rs");
        let create = &ctor[ctor.find("fn create_missing_webviews(").unwrap()..];
        let create = &create[..create.find("\n}\n").unwrap()];
        let defer = create
            .find("defers_creation(")
            .expect("skip rule is applied");
        let build = create.find("construct(view").expect("constructor call");
        assert!(defer < build, "the skip rule must run before construction");
        assert!(create.contains("view.lazy_webviews_created.push("));
    }

    /// 가려진 WebView 를 없애는 단계가 생성보다 먼저 돈다 — 같은 프레임에 다시 보이면 바로 새로 만든다.
    /// 없앨 때는 stale 정리와 같은 기록을 지워야 다음 생성이 처음 만드는 것과 같다.
    #[test]
    fn hidden_lazy_webviews_are_released_before_creation() {
        let src = include_str!("../webview_sync.rs");
        let sync = &src[src.find("fn sync_webviews(").unwrap()..];
        let release = sync
            .find("lazy::release_hidden_webviews(view, engine, &active_html, proofs);")
            .expect("release step is wired");
        let create = sync.find("create_missing_webviews(view").unwrap();
        assert!(release < create);
        let lazy = include_str!("lazy.rs");
        let body = &lazy[lazy.find("fn release_hidden_webviews(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("!active_html.contains_key(sid)"));
        assert!(body.contains("releases_hidden_webview("));
        assert!(body.contains("super::forget_webview(view, sid, proofs);"));
        assert_eq!(src.matches("view.webviews.remove(&surface);").count(), 1);
    }
}
