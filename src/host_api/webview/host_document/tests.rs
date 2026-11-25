use super::*;

/// 호스트가 보낸 새 탐색의 시작. 주소 끝의 로드 번호를 머리글로 단다(`host_document_url`).
fn start(docs: &mut HostDocuments, uri: &str, nav: u64) -> StartDecision {
    let load = uri
        .rsplit('/')
        .next()
        .and_then(|last| last.split('#').next())
        .and_then(parse_host_load);
    docs.navigation_started(uri, Some(nav), StartKind::Other, false, load)
}

/// 페이지가 스스로 시작한 새 탐색(머리글 없음).
fn page(docs: &mut HostDocuments, uri: &str, nav: u64) -> StartDecision {
    docs.navigation_started(uri, Some(nav), StartKind::Other, false, None)
}

/// 지금 main 문서 요청에 본문을 실은 응답을 넘긴다(Windows 배선은 `SetResponse` 성공 뒤 `served`를 부른다).
fn serve(docs: &mut HostDocuments, uri: &str) -> Option<Arc<str>> {
    let body = docs.respond(uri, true, true, false);
    if body.is_some() {
        docs.served();
    }
    body
}

/// WebView2가 Reload로 보고한 시작.
fn reload(docs: &mut HostDocuments, uri: &str, nav: u64) -> StartDecision {
    docs.navigation_started(uri, Some(nav), StartKind::Reload, false, None)
}

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
    assert_eq!(start(&mut docs, &url, 10), StartDecision::HostLoad);
    assert_eq!(serve(&mut docs, &url).as_deref(), Some("<p>one</p>"));
    docs.navigation_completed(Some(10), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn requests_other_than_the_main_document_get_no_body() {
    let mut docs = HostDocuments::default();
    let url = host_document_url(1, 1, 1);
    docs.sent(1, url.clone(), body("x"));
    start(&mut docs, &url, 10);
    assert!(docs.respond(&url, false, true, false).is_none(), "POST");
    assert!(
        docs.respond(&url, true, false, false).is_none(),
        "하위 리소스"
    );
    assert!(docs.respond(&url, true, true, true).is_none(), "subframe");
    assert!(
        serve(&mut docs, &host_document_url(1, 1, 2)).is_none(),
        "다른 load"
    );
    assert!(
        serve(&mut docs, &host_document_url(2, 1, 1)).is_none(),
        "다른 surface"
    );
}

/// 표에 있어도 지금 main 탐색의 주소가 아니면 본문을 주지 않는다(iframe이 같은 주소를 열 때).
#[test]
fn a_document_in_the_table_is_not_served_outside_its_main_navigation() {
    let mut docs = HostDocuments::default();
    let url = host_document_url(1, 1, 1);
    docs.sent(1, url.clone(), body("x"));
    assert!(serve(&mut docs, &url).is_none(), "시작 전");
    start(&mut docs, &url, 10);
    serve(&mut docs, &url);
    docs.navigation_completed(Some(10), true);
    assert!(serve(&mut docs, &url).is_none(), "탐색이 끝난 뒤");
}

#[test]
fn an_unknown_reserved_navigation_is_cancelled() {
    let mut docs = HostDocuments::default();
    let known = host_document_url(1, 1, 1);
    docs.sent(1, known, body("x"));
    assert_eq!(
        start(
            &mut docs,
            "http://v1-1.tasty-host-document.invalid/notes.md",
            1
        ),
        StartDecision::Cancel
    );
    assert_eq!(
        start(&mut docs, "https://example.com/", 2),
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
    start(&mut docs, &a, 10);
    assert_eq!(serve(&mut docs, &a).as_deref(), Some("a"));
    start(&mut docs, &b, 11);
    assert_eq!(serve(&mut docs, &b).as_deref(), Some("b"));
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
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    docs.navigation_completed(Some(11), false);
    assert_eq!(docs.loads(), vec![(1, true)]);
    assert_eq!(
        reload(&mut docs, &format!("{a}#top"), 12),
        StartDecision::Reload,
        "commit된 문서의 다시 읽기(fragment가 붙어도 같은 문서)"
    );
    assert_eq!(serve(&mut docs, &a).as_deref(), Some("a"));
    docs.navigation_completed(Some(12), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

/// 본문을 보내지 않은 성공(오류 페이지 등)은 보이는 문서를 바꾸지 않는다.
#[test]
fn a_success_without_a_served_body_does_not_replace_the_shown_document() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    docs.navigation_completed(Some(11), true);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn a_shown_ordinary_document_retires_the_host_document() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    start(&mut docs, "about:blank", 11);
    docs.navigation_completed(Some(11), true);
    assert!(docs.loads().is_empty());
}

#[test]
fn an_ended_renderer_drops_documents_in_flight() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    docs.renderer_ended();
    assert_eq!(docs.loads(), vec![(1, true)]);
    assert!(serve(&mut docs, &b).is_none());
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
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.committed(Some(10), false);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    docs.navigation_completed(Some(10), false);
    docs.navigation_completed(Some(11), false);
    assert_eq!(docs.loads(), vec![(1, true)]);
}

#[test]
fn an_error_page_commit_keeps_the_previous_document() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.committed(Some(10), false);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    serve(&mut docs, &b);
    docs.committed(Some(11), true);
    assert_eq!(docs.loads(), vec![(1, true), (2, false)]);
}

#[test]
fn an_ordinary_commit_retires_the_host_document() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.committed(Some(10), false);
    start(&mut docs, "about:blank", 11);
    docs.committed(Some(11), false);
    assert!(docs.loads().is_empty());
}

/// 본문을 로컬로 보내지 못한 예약 주소 탐색은 성공으로 끝나도 실패로 보고한다.
#[test]
fn a_reserved_navigation_without_a_local_answer_reports_failure() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    // 본문을 꺼냈지만 실은 응답을 넘기지 못했다(200 생성이나 SetResponse 실패).
    docs.respond(&a, true, true, false);
    assert!(docs.navigation_completed(Some(10), true));
    assert!(docs.loads().is_empty());
    let b = host_document_url(1, 1, 2);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    serve(&mut docs, &b);
    assert!(!docs.navigation_completed(Some(11), true));
    start(&mut docs, "https://example.com/", 12);
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
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
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

/// 표에 있는 주소라도 호스트가 보낸 다시 싣기와 commit된 문서의 Reload가 아니면 본문을 받지 못한다.
#[test]
fn only_the_host_retry_and_a_reload_reach_a_reserved_address() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    assert_eq!(
        docs.navigation_started(&a, Some(9), StartKind::Other, true, Some(1)),
        StartDecision::Cancel,
        "보낸 주소로의 리다이렉트"
    );
    assert_eq!(
        reload(&mut docs, &a, 9),
        StartDecision::Cancel,
        "아직 보이지 않은 문서의 Reload"
    );
    assert_eq!(start(&mut docs, &a, 10), StartDecision::HostLoad);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    assert_eq!(
        start(&mut docs, &a, 11),
        StartDecision::Cancel,
        "보이는 문서로 페이지가 스스로 가는 새 탐색"
    );
    assert!(serve(&mut docs, &a).is_none());
    assert_eq!(
        docs.navigation_started(&a, Some(12), StartKind::History, false, Some(1)),
        StartDecision::Cancel,
        "이력 이동"
    );
    assert_eq!(
        docs.navigation_started(&a, Some(13), StartKind::Reload, true, Some(1)),
        StartDecision::Cancel,
        "Reload의 리다이렉트"
    );
    assert_eq!(reload(&mut docs, &a, 14), StartDecision::Reload);
    assert_eq!(serve(&mut docs, &a).as_deref(), Some("a"));
    docs.sent(2, b.clone(), body("b"));
    assert_eq!(start(&mut docs, &b, 15), StartDecision::HostLoad);
    assert_eq!(
        start(&mut docs, &b, 16),
        StartDecision::Cancel,
        "보낸 문서가 이미 묶인 뒤 같은 주소의 두 번째 새 탐색"
    );
    // 취소한 16은 요청을 내지 않는다. 문서 요청은 받은 15의 것이라 본문을 받는다.
    assert_eq!(serve(&mut docs, &b).as_deref(), Some("b"));
}

/// 200 응답을 만들지 못해 404를 보낸 탐색은 성공으로 끝나도 실패이고 보이던 문서를 지우지 않는다.
#[test]
fn a_body_that_never_left_keeps_the_previous_document() {
    let mut docs = HostDocuments::default();
    let (a, b) = (host_document_url(1, 1, 1), host_document_url(1, 1, 2));
    docs.sent(1, a.clone(), body("a"));
    start(&mut docs, &a, 10);
    serve(&mut docs, &a);
    docs.navigation_completed(Some(10), true);
    docs.sent(2, b.clone(), body("b"));
    start(&mut docs, &b, 11);
    assert!(docs.respond(&b, true, true, false).is_some());
    docs.committed(Some(11), false);
    assert!(docs.navigation_completed(Some(11), true));
    assert_eq!(docs.loads(), vec![(1, true)]);
    assert_eq!(reload(&mut docs, &a, 12), StartDecision::Reload);
}

/// 호스트는 `load_url`로 예약 주소를 보내지 않는다(다시 싣기만 그 주소를 연다).
#[test]
fn the_host_never_loads_a_reserved_address_by_url() {
    assert!(!host_may_load_url(
        "http://v1-1.tasty-host-document.invalid/notes.md"
    ));
    assert!(!host_may_load_url(&host_document_url(1, 1, 2)));
    assert!(host_may_load_url("https://example.com/notes.md"));
}

/// 호스트 다시 싣기의 시작보다 먼저 페이지가 같은 주소로 가는 탐색을 시작해도 본문은 호스트 탐색만 받는다.
#[test]
fn a_page_navigation_ahead_of_the_host_retry_gets_no_body() {
    let mut docs = HostDocuments::default();
    let a = host_document_url(1, 1, 1);
    docs.sent(1, a.clone(), body("a"));
    assert_eq!(page(&mut docs, &a, 9), StartDecision::Cancel);
    assert!(serve(&mut docs, &a).is_none());
    assert_eq!(
        docs.navigation_started(&a, None, StartKind::Other, false, Some(1)),
        StartDecision::Cancel,
        "탐색 ID를 읽지 못한 시작은 묶지 않는다"
    );
    assert_eq!(
        docs.navigation_started(&a, Some(10), StartKind::Other, false, Some(2)),
        StartDecision::Cancel,
        "다른 로드의 머리글"
    );
    assert_eq!(start(&mut docs, &a, 11), StartDecision::HostLoad);
    assert_eq!(serve(&mut docs, &a).as_deref(), Some("a"));
}

/// 다시 싣기 요청의 머리글 줄은 시작 처리기가 읽는 이름과 값으로 나뉜다.
#[test]
fn the_host_load_header_round_trips() {
    let line = host_load_header(7);
    let (name, value) = line.split_once(':').unwrap();
    assert_eq!(name, HOST_LOAD_HEADER);
    assert_eq!(parse_host_load(value), Some(7));
    assert_eq!(parse_host_load("seven"), None);
}

/// 받은 호스트 다시 싣기가 진행 중일 때 같은 주소로 온 페이지 탐색을 취소해도 그 다시 싣기의 기록은 남는다.
#[test]
fn a_cancelled_page_navigation_keeps_the_admitted_host_retry() {
    let mut docs = HostDocuments::default();
    let b = host_document_url(1, 1, 2);
    docs.sent(2, b.clone(), body("b"));
    assert_eq!(start(&mut docs, &b, 15), StartDecision::HostLoad);
    assert_eq!(page(&mut docs, &b, 16), StartDecision::Cancel);
    assert_eq!(serve(&mut docs, &b).as_deref(), Some("b"));
    assert!(!docs.navigation_completed(Some(15), true));
    assert_eq!(docs.loads(), vec![(2, true)]);
}

/// 다시 싣기를 보낸 뒤 그 시작의 머리글을 읽지 못하면 본문을 주지 않고, 그 로드가 기다리는 시작으로 본다.
/// 머리글이 없는 시작(페이지 탐색)은 그렇게 보지 않는다.
#[test]
fn an_unreadable_header_counts_only_while_the_retry_awaits_its_start() {
    let mut docs = HostDocuments::default();
    let b = host_document_url(1, 1, 2);
    docs.sent(2, b.clone(), body("b"));
    assert!(docs.awaiting_start(2));
    assert!(!docs.awaiting_start(3));
    let unreadable = HostLoadHeader::Unreadable;
    assert_eq!(
        docs.navigation_started(&b, Some(15), StartKind::Other, false, unreadable.load()),
        StartDecision::Cancel
    );
    assert!(serve(&mut docs, &b).is_none());
    assert!(sent_for_current_load(unreadable, 2, docs.awaiting_start(2)));
    assert!(!sent_for_current_load(
        HostLoadHeader::Absent,
        2,
        docs.awaiting_start(2)
    ));
    assert!(!sent_for_current_load(
        unreadable,
        3,
        docs.awaiting_start(3)
    ));
    // 다시 싣기가 묶인 뒤에는 읽지 못한 시작을 그 로드의 것으로 보지 않는다.
    assert_eq!(start(&mut docs, &b, 16), StartDecision::HostLoad);
    assert!(!docs.awaiting_start(2));
    assert!(!sent_for_current_load(
        unreadable,
        2,
        docs.awaiting_start(2)
    ));
}

/// 탐색 ID를 읽지 못한 Reload는 보이는 문서의 주소라도 본문을 받지 못한다.
#[test]
fn a_reload_without_a_navigation_id_gets_no_body() {
    let mut docs = HostDocuments::default();
    let b = host_document_url(1, 1, 2);
    docs.sent(2, b.clone(), body("b"));
    assert_eq!(start(&mut docs, &b, 15), StartDecision::HostLoad);
    assert_eq!(serve(&mut docs, &b).as_deref(), Some("b"));
    docs.committed(Some(15), false);
    assert!(!docs.navigation_completed(Some(15), true));
    assert_eq!(
        docs.navigation_started(&b, None, StartKind::Reload, false, None),
        StartDecision::Cancel
    );
    assert!(serve(&mut docs, &b).is_none());
    assert_eq!(reload(&mut docs, &b, 16), StartDecision::Reload);
    assert_eq!(serve(&mut docs, &b).as_deref(), Some("b"));
}
