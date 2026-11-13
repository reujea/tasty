//! `webview.post_message` 의 전달 — 대기 큐를 실린 문서에서 돌릴지, 버릴지 정한다.
//! 계약은 docs/design/systems/webview.md 의 "페이지에 보내는 메시지" 절에 있다.

use super::{EngineRef, MainView, find_remote_surface};

/// 지금 실린 문서가 호스트가 이 surface 에 실은 문서인가 — `webview.post_message` 의 전달 조건이다.
///
/// `loaded` 는 호스트가 마지막으로 실은 값이고 `committed` 는 엔진이 알려 준 현재 문서 주소다.
/// 주소로 연 문서는 주소가 같아야 한다(`#fragment` 이동은 같은 문서라 무시한다). raw HTML 로 실은
/// 문서는 비교할 주소가 없으므로 "탐색 가능한 주소로 가 있지 않다"로 가른다 — 그 문서를 떠나는
/// 이동은 어느 플랫폼에서나 `file:`·`http(s):` 주소를 남기고, 바탕 주소(`about:blank` 등)는
/// 플랫폼마다 달라 그 값에 기대지 않는다.
pub(super) fn document_is_the_loaded_one(loaded: Option<&str>, committed: Option<&str>) -> bool {
    use crate::model::html_script::strip_fragment;
    match loaded {
        None => false,
        Some(loaded) if crate::webview::is_navigable_url(loaded) => {
            committed.is_some_and(|now| strip_fragment(now) == strip_fragment(loaded))
        }
        // raw HTML. 떠났으면 탐색 가능한 주소가 남는다.
        Some(_) => !committed.is_some_and(crate::webview::is_navigable_url),
    }
}

/// plugin 이 `webview.post_message` 로 쌓은 메시지 스크립트를 실린 문서에서 돌린다.
/// URL 재동기화 뒤에 돌아 같은 프레임에 문서를 바꿨으면 새 문서가 받는다.
///
/// 버리는 경우가 둘이다(docs/reference/api.md `webview.post_message`):
/// - WebView 가 없다 — 아직 만들지 않았거나(lazy kind) 가려져 해제됐다. 큐를 남기면 호스트
///   메모리에 쌓이고 WebView 가 생기는 순간 밀린 것이 한꺼번에 돈다.
/// - 페이지가 plugin 이 실은 문서가 아닌 곳(사용자가 따라간 링크·외부 사이트)에 가 있다.
///   그대로 돌리면 plugin 이 보낸 데이터를 그 문서가 받는다.
pub(super) fn drain_pending_scripts(view: &MainView, engine: &EngineRef<'_>, all_html_ids: &[u32]) {
    for &sid in all_html_ids {
        let Some(rs) = find_remote_surface(view, engine, sid) else {
            continue;
        };
        let Some(wv) = view.webviews.get(&sid) else {
            let dropped = rs.drop_scripts();
            if dropped > 0 {
                tracing::debug!(
                    "WebView surface {sid}: dropped {dropped} pending message(s); no WebView"
                );
            }
            continue;
        };
        if !document_is_the_loaded_one(
            view.webview_loaded_urls.get(&sid).map(String::as_str),
            wv.nav_info().url.as_deref(),
        ) {
            let dropped = rs.drop_scripts();
            if dropped > 0 {
                tracing::warn!(
                    "WebView surface {sid}: dropped {dropped} pending message(s); \
                     the page is not the document the plugin loaded"
                );
            }
            continue;
        }
        for script in rs.take_scripts() {
            wv.eval_js(&script);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::document_is_the_loaded_one as same_document;

    const A: &str = "file:///docs/a.html";
    const HTML: &str = "<!doctype html><html></html>";

    /// 주소로 연 문서는 그 주소에 머물러 있을 때만 메시지를 받는다.
    #[test]
    fn a_url_document_must_still_be_at_that_url() {
        assert!(same_document(Some(A), Some(A)));
        assert!(
            same_document(Some(A), Some(&format!("{A}#sec"))),
            "같은 문서 안 fragment 이동"
        );
        assert!(!same_document(Some(A), Some("https://example.com/")));
        assert!(!same_document(Some(A), Some("file:///docs/b.html")));
        assert!(!same_document(Some(A), None), "아직 commit 된 문서가 없다");
    }

    /// raw HTML 문서는 비교할 주소가 없다 — 탐색 가능한 주소로 떠났는지로 가른다.
    #[test]
    fn a_raw_html_document_is_left_once_the_page_navigates() {
        assert!(same_document(Some(HTML), None));
        assert!(same_document(Some(HTML), Some("about:blank")));
        assert!(
            same_document(Some(HTML), Some("about:blank#x")),
            "바탕 주소 안 이동은 그대로다"
        );
        assert!(!same_document(Some(HTML), Some("https://example.com/")));
        assert!(!same_document(Some(HTML), Some(A)));
    }

    /// 호스트가 아직 아무 문서도 싣지 않았으면 받을 문서가 없다.
    #[test]
    fn nothing_loaded_means_nothing_to_deliver_to() {
        assert!(!same_document(None, None));
        assert!(!same_document(None, Some(A)));
    }

    /// 판정만 있고 전달 경로가 그것을 부르지 않으면 위 시험은 계속 통과한다.
    /// WebView 가 없을 때와 문서가 다를 때, 둘 다 큐를 버리고 나서야 eval 에 닿는지 본다.
    #[test]
    fn the_drain_consults_both_rules_before_it_evaluates() {
        let src = include_str!("post_message.rs");
        let body = &src[src.find("fn drain_pending_scripts(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        let no_webview = body
            .find("let Some(wv) = view.webviews.get(&sid) else {")
            .expect("missing-WebView branch");
        let origin = body
            .find("if !document_is_the_loaded_one(")
            .expect("origin check");
        let eval = body.find("wv.eval_js(&script)").expect("evaluation");
        assert!(no_webview < eval && origin < eval, "검사가 전달보다 먼저다");
        assert_eq!(
            body.matches("rs.drop_scripts()").count(),
            2,
            "두 경우 모두 큐를 버린다"
        );
    }
}
