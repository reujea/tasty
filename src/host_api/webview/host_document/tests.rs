use super::*;

const E_FAIL: i32 = 0x8000_4005_u32 as i32;

fn big() -> String {
    // UTF-16 바이트로 2MiB를 넘는 문서. mermaid 번들(3.5MB)을 넣은 markdown이 이 크기다.
    "a".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2 + 1)
}

fn body(s: &str) -> Arc<str> {
    Arc::from(s)
}

#[test]
fn only_an_oversized_invalid_argument_rejection_of_the_current_load_is_retried() {
    let html = big();
    assert!(retry_via_host_document(E_INVALIDARG, &html, 3, 3, true));
    assert!(
        !retry_via_host_document(E_FAIL, &html, 3, 3, true),
        "다른 거절 이유는 다시 싣지 않는다"
    );
    assert!(
        !retry_via_host_document(E_INVALIDARG, &html, 4, 3, true),
        "지난 로드는 다시 싣지 않는다"
    );
    assert!(
        !retry_via_host_document(E_INVALIDARG, &html, 3, 3, false),
        "허용하지 않은 view는 다시 싣지 않는다"
    );
    let at_limit = "a".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2);
    assert!(
        !retry_via_host_document(E_INVALIDARG, &at_limit, 3, 3, true),
        "상한 안의 문서는 다른 이유로 거절된 것이다"
    );
}

#[test]
fn the_cheap_size_filter_never_drops_a_retried_document() {
    for html in [
        big(),
        "가".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2 + 1),
        "a".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2),
    ] {
        if retry_via_host_document(E_INVALIDARG, &html, 1, 1, true) {
            assert!(may_exceed_limit(&html));
        }
    }
    assert!(!may_exceed_limit("<p>small</p>"));
}

#[test]
fn the_size_counts_utf16_bytes() {
    // 한글은 UTF-8 3바이트지만 UTF-16 2바이트다. 상한은 UTF-16 기준이다.
    let korean = "가".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2);
    assert!(!retry_via_host_document(E_INVALIDARG, &korean, 1, 1, true));
    let korean = "가".repeat(NAVIGATE_TO_STRING_MAX_BYTES / 2 + 1);
    assert!(retry_via_host_document(E_INVALIDARG, &korean, 1, 1, true));
}

#[test]
fn reserved_addresses_are_recognised_by_host_only() {
    let url = host_document_url(7, 2, 5);
    assert_eq!(url, "http://v7-2.tasty-host-document.invalid/5");
    assert!(is_reserved(&url));
    assert!(is_reserved(
        "https://V1-1.Tasty-Host-Document.Invalid./x?y#z"
    ));
    assert!(is_reserved("http://u@v1-1.tasty-host-document.invalid:80/"));
    assert!(!is_reserved(
        "http://example.com/tasty-host-document.invalid"
    ));
    assert!(!is_reserved("about:blank#tasty-nav:browse:1"));
    assert!(!is_reserved("file:///v1-1.tasty-host-document.invalid/1"));
    assert!(!is_reserved(
        "http://tasty-host-document.invalid.example.com/"
    ));
}

/// host가 보낸 문서는 그 main 탐색의 문서 요청에만 응답하고, 성공하면 보이는 문서가 된다.
#[test]
fn a_sent_document_answers_its_main_navigation_and_becomes_shown() {
    let mut docs = HostDocuments::default();
    let url = host_document_url(1, 1, 1);
    docs.sent(1, url.clone(), body("<p>one</p>"));
    assert_eq!(
        docs.navigation_started(&url, Some(10)),
        StartDecision::HostLoad
    );
    assert_eq!(
        docs.respond(&url, true, true, false).as_deref(),
        Some("<p>one</p>")
    );
    docs.navigation_completed(Some(10), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn requests_other_than_the_main_document_get_no_body() {
    let mut docs = HostDocuments::default();
    let url = host_document_url(1, 1, 1);
    docs.sent(1, url.clone(), body("x"));
    docs.navigation_started(&url, Some(10));
    assert!(docs.respond(&url, false, true, false).is_none(), "POST");
    assert!(
        docs.respond(&url, true, false, false).is_none(),
        "하위 리소스"
    );
    assert!(docs.respond(&url, true, true, true).is_none(), "subframe");
    assert!(
        docs.respond(&host_document_url(1, 1, 2), true, true, false)
            .is_none(),
        "다른 load"
    );
    assert!(
        docs.respond(&host_document_url(2, 1, 1), true, true, false)
            .is_none(),
        "다른 surface"
    );
}

/// 표에 있어도 지금 main 탐색의 주소가 아니면 본문을 주지 않는다(iframe이 같은 주소를 열 때).
#[test]
fn a_document_in_the_table_is_not_served_outside_its_main_navigation() {
    let mut docs = HostDocuments::default();
    let url = host_document_url(1, 1, 1);
    docs.sent(1, url.clone(), body("x"));
    assert!(docs.respond(&url, true, true, false).is_none(), "시작 전");
    docs.navigation_started(&url, Some(10));
    docs.respond(&url, true, true, false);
    docs.navigation_completed(Some(10), true);
    assert!(
        docs.respond(&url, true, true, false).is_none(),
        "탐색이 끝난 뒤"
    );
}

#[test]
fn an_unknown_reserved_navigation_is_cancelled() {
    let mut docs = HostDocuments::default();
    let known = host_document_url(1, 1, 1);
    docs.sent(1, known.clone(), body("x"));
    assert!(docs.knows(&format!("{known}#top")));
    assert!(!docs.knows("http://v1-1.tasty-host-document.invalid/notes.md"));
    assert_eq!(
        docs.navigation_started("http://v1-1.tasty-host-document.invalid/notes.md", Some(1)),
        StartDecision::Cancel
    );
    assert_eq!(
        docs.navigation_started("https://example.com/", Some(2)),
        StartDecision::NotReserved
    );
}

/// 장벽에 붙잡혔다가 차례로 나가는 두 로드는 각자 자기 본문을 갖는다.
#[test]
fn two_queued_loads_keep_their_own_bodies() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&a, Some(10));
    assert_eq!(docs.respond(&a, true, true, false).as_deref(), Some("a"));
    docs.navigation_started(&b, Some(11));
    assert_eq!(docs.respond(&b, true, true, false).as_deref(), Some("b"));
    // 대신된 앞 탐색의 늦은 실패는 그 문서만 지운다.
    docs.navigation_completed(Some(10), false);
    assert_eq!(docs.loads(), vec![(2, false)]);
    docs.navigation_completed(Some(11), true);
    assert_eq!(docs.loads(), vec![(2, true)]);
}

#[test]
fn an_immediately_rejected_retry_leaves_no_entry() {
    let mut docs = HostDocuments::default();
    docs.sent(1, host_document_url(1, 1, 1), body("x"));
    docs.rejected(1);
    assert!(docs.loads().is_empty());
}

/// 새 문서가 실패해도 직전에 보이던 문서는 남아 다시 읽기에 응답한다.
#[test]
fn a_failed_new_load_keeps_the_shown_document_for_reload() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.navigation_completed(Some(11), false);
    assert_eq!(docs.loads(), vec![(1, true)]);
    assert_eq!(
        docs.navigation_started(&format!("{a}#top"), Some(12)),
        StartDecision::Reload,
        "commit된 문서의 다시 읽기(fragment가 붙어도 같은 문서)"
    );
    assert_eq!(docs.respond(&a, true, true, false).as_deref(), Some("a"));
    docs.navigation_completed(Some(12), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

/// 본문을 보내지 않은 성공(오류 페이지 등)은 보이는 문서를 바꾸지 않는다.
#[test]
fn a_success_without_a_served_body_does_not_replace_the_shown_document() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.navigation_completed(Some(11), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn a_shown_ordinary_document_retires_the_host_document() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.navigation_completed(Some(10), true);
    docs.navigation_started("about:blank", Some(11));
    docs.navigation_completed(Some(11), true);
    assert!(docs.loads().is_empty());
}

#[test]
fn an_ended_renderer_drops_documents_in_flight() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.renderer_ended();
    assert_eq!(docs.loads(), vec![(1, true)]);
    assert!(docs.respond(&b, true, true, false).is_none());
}

/// 같은 문서 fragment 이동의 주소 판정은 host 주소에서도 `#` 앞만 본다.
#[test]
fn a_fragment_move_in_a_host_document_is_reported() {
    let url = host_document_url(1, 1, 1);
    let moved = format!("{url}#tasty-nav:browse:1");
    let nav = crate::host_api::webview::same_document::from_href_result(
        &serde_json::to_string(&moved).unwrap(),
        true,
        &[],
    )
    .unwrap();
    assert_eq!(nav.url, moved);
    assert!(tasty_model::html_script::top_level::same_target(
        &moved, &url
    ));
}

/// commit한 문서는 그 완료가 대신된 뒤 새 로드가 commit 전에 실패해도 남는다.
#[test]
fn a_committed_document_survives_a_superseded_completion_and_a_failed_successor() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.committed(Some(10), false);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.navigation_completed(Some(10), false);
    docs.navigation_completed(Some(11), false);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn an_error_page_commit_keeps_the_previous_document() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.committed(Some(10), false);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.respond(&b, true, true, false);
    docs.committed(Some(11), true);
    assert_eq!(docs.loads(), vec![(1, true), (2, false)]);
}

#[test]
fn an_ordinary_commit_retires_the_host_document() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.committed(Some(10), false);
    docs.navigation_started("about:blank", Some(11));
    docs.committed(Some(11), false);
    assert!(docs.loads().is_empty());
}

/// 본문을 로컬로 보내지 못한 예약 주소 탐색은 성공으로 끝나도 실패로 보고한다.
#[test]
fn a_reserved_navigation_without_a_local_answer_reports_failure() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.response_failed();
    assert!(docs.navigation_completed(Some(10), true));
    assert!(docs.loads().is_empty());
    let b = host_document_url(1, 1, 2);
    docs.sent(2, b.clone(), body("b"));
    docs.navigation_started(&b, Some(11));
    docs.respond(&b, true, true, false);
    assert!(!docs.navigation_completed(Some(11), true));
    docs.navigation_started("https://example.com/", Some(12));
    assert!(!docs.navigation_completed(Some(12), true));
}

#[test]
fn a_navigation_cancelled_before_its_start_is_recognised_once() {
    let mut docs = HostDocuments::default();
    docs.cancelled(Some(7));
    assert!(!docs.take_cancelled(Some(8)));
    assert!(docs.take_cancelled(Some(7)));
    assert!(!docs.take_cancelled(Some(7)));
    docs.cancelled(None);
    assert!(
        !docs.take_cancelled(None),
        "ID를 모르는 취소는 기록하지 않는다"
    );
}

#[test]
fn an_ended_browser_empties_the_table() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    docs.navigation_started(&a, Some(10));
    docs.respond(&a, true, true, false);
    docs.committed(Some(10), false);
    docs.browser_ended();
    assert!(docs.loads().is_empty());
}

#[test]
fn the_filters_cover_every_reserved_address() {
    for url in [
        host_document_url(1, 1, 1),
        "https://v1-1.tasty-host-document.invalid/1".to_string(),
        "http://v1-1.tasty-host-document.invalid:8080/1".to_string(),
    ] {
        assert!(is_reserved(&url));
        assert!(
            RESERVED_FILTERS
                .iter()
                .any(|f| url.starts_with(f.split('*').next().unwrap())
                    && url.contains(".tasty-host-document.invalid")),
            "{url}"
        );
    }
}
