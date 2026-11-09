//! 처음 보일 때 만드는 webview kind(`lazy_webview`)의 규칙 — 생성 통지와 가려진 WebView 해제.
//! 계약은 docs/design/systems/webview.md 의 "생성 시점" 절에 있다.

use crate::plugin::PluginManager;
use crate::view::MainView;

/// lazy kind WebView 를 만든 뒤 소유 plugin 에 `surface.webview_created` 를 보낸다. 실린 문서는
/// 마지막 `webview.set_url` 값이다 — plugin 은 이 통지에 맞춰 페이지 상태를 다시 맞춘다.
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
            // 설정 주인은 kind 등록 plugin 이 아니라 surface 소유 plugin 이다.
            let Some(owner) =
                super::find_remote_surface(view, engine, sid).map(|rs| rs.plugin_id.clone())
            else {
                return false;
            };
            super::webview_surface_kind(view, engine, sid)
                .and_then(crate::runtime::surface_registry::webview_kind::webview_settings_source)
                .is_some_and(|source| {
                    crate::host_api::webview::releases_hidden_webview(
                        &source,
                        &owner,
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

/// `surface.webview_created` 를 받을 plugin — surface 를 소유한 plugin 이다. kind 등록은 다른
/// plugin 이 같은 이름으로 덮어쓸 수 있으므로(경고만 남는다) 등록자를 수신자로 삼으면 B 가 A 의
/// kind 를 덮어써 A 의 통지를 가로챌 수 있다. `owner` 는 `RemoteSurface.plugin_id`, `registrant`
/// 는 kind 선언을 올린 plugin 이다. lazy kind 가 아니면 통지 자체가 없다.
pub(super) fn creation_event_recipient(
    lazy: bool,
    owner: Option<&str>,
    registrant: Option<&str>,
) -> Option<String> {
    // 등록자는 일부러 쓰지 않는다 — 손에 쥐고도 고르지 않는 것이 이 규칙의 내용이다.
    let _ = registrant;
    lazy.then(|| owner.map(str::to_owned)).flatten()
}

/// lazy kind WebView 생성 통지 키. 받는 plugin 쪽 상수와 같아야 한다.
pub(crate) const WEBVIEW_CREATED_EVENT: &str = "surface.webview_created";

#[cfg(test)]
mod tests {
    /// 생성 단계가 lazy 규칙을 거치는지 — 규칙만 있고 호출이 빠지면 판정 시험은 계속 통과한다.
    #[test]
    fn the_constructor_consults_the_lazy_rules() {
        let ctor = include_str!("construction.rs");
        let create = &ctor[ctor.find("fn create_missing_webviews(").unwrap()..];
        let create = &create[..create.find("\n}\n").unwrap()];
        let defer = create
            .find("defers_creation(")
            .expect("skip rule is applied");
        let build = create.find("construct(view").expect("constructor call");
        assert!(defer < build, "the skip rule must run before construction");
        assert!(create.contains("view.lazy_webviews_created.push("));
        // 통지 대상은 kind 를 등록한 plugin 이 아니라 surface 소유 plugin 이다. 판정은
        // creation_event_recipient 가 하고, 거기에 소유자가 owner 자리로 들어가야 한다.
        assert!(
            create.contains(
                "let owner = find_remote_surface(view, engine, sid).map(|rs| rs.plugin_id.clone());"
            ),
            "the owner must come from RemoteSurface.plugin_id"
        );
        assert!(
            create.contains("super::lazy::creation_event_recipient(\n                lazy,\n                owner.as_deref(),"),
            "the recipient rule decides, with the surface owner in the owner position"
        );
    }

    /// 통지는 surface 소유자에게 간다 — kind 를 덮어쓴 plugin 이 아니라. B 가 A 의 kind 이름을
    /// lazy 로 덮어써도 `surface.webview_created` 는 A 에게 가야 A 가 문서를 다시 싣는다.
    #[test]
    fn the_surface_owner_receives_the_creation_event_not_the_kind_registrant() {
        let owner = "com.example.owner";
        let squatter = "com.example.squatter";
        assert_eq!(
            super::creation_event_recipient(true, Some(owner), Some(squatter)),
            Some(owner.to_owned()),
            "덮어쓴 plugin 이 아니라 소유자가 받는다"
        );
        // 등록자가 소유자와 같아도 결과는 소유자다.
        assert_eq!(
            super::creation_event_recipient(true, Some(owner), Some(owner)),
            Some(owner.to_owned())
        );
        // lazy kind 가 아니면 통지 자체가 없다.
        assert_eq!(
            super::creation_event_recipient(false, Some(owner), Some(squatter)),
            None,
            "lazy 가 아닌 kind 는 생성 통지를 내지 않는다"
        );
        // 소유자를 못 찾으면(plugin surface 가 아니면) 보낼 곳이 없다 — 등록자로 대신하지 않는다.
        assert_eq!(
            super::creation_event_recipient(true, None, Some(squatter)),
            None,
            "소유자가 없으면 등록자에게 보내지 않는다"
        );
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
        // 해제 설정의 주인도 surface 소유 plugin 이다.
        assert!(
            body.contains("rs.plugin_id.clone()"),
            "owner comes from the surface"
        );
        assert!(body.contains("super::forget_webview(view, sid, proofs);"));
        assert_eq!(src.matches("view.webviews.remove(&surface);").count(), 1);
    }
}
