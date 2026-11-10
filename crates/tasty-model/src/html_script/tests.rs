use super::*;

const A: &str = "file:///docs/a.html";
const C: &str = "file:///docs/c.html";

fn scan(byte: u8) -> ScriptScan {
    ScriptScan {
        fingerprint: Fingerprint([byte; 32]),
        detection: ScriptDetection::Scripts,
        remote_refs: false,
        remote_scripts: false,
    }
}

/// STARTED → RESPONSE → COMMITTED → FINISHED 순서의 main frame 로드. 단계별 JS 값을 돌려준다.
fn load(st: &mut HtmlScriptState, url: &str, s: Option<ScriptScan>) -> [bool; 3] {
    let started = st.on_load_started();
    let response = st.on_main_response(url, s);
    let committed = st.on_committed(Some(url));
    assert_eq!(
        st.on_load_finished(),
        None,
        "commit이 있었으면 복원하지 않는다"
    );
    [started, response, committed]
}

fn allowed_a() -> HtmlScriptState {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.allow_current().expect("allow");
    assert!(st.take_reload_request());
    assert_eq!(load(&mut st, A, Some(scan(1))), [false, true, true]);
    st
}

#[test]
fn a_first_load_under_the_sandbox_keeps_javascript_off() {
    let mut st = HtmlScriptState::new(true);
    assert_eq!(load(&mut st, A, Some(scan(1))), [false, false, false]);
    assert_eq!(st.current_detection(), Some(ScriptDetection::Scripts));
    assert!(st.allowance().is_none());
}

#[test]
fn allowing_records_the_current_document_and_requests_one_reload() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.allow_current().expect("allow");
    assert_eq!(
        st.allowance(),
        Some(&Allowed::Document(Allowance {
            url: A.to_string(),
            fingerprint: Fingerprint([1; 32]),
        }))
    );
    assert!(st.take_reload_request());
    assert!(!st.take_reload_request(), "요청은 한 번만 꺼낸다");
}

#[test]
fn the_allowed_document_runs_after_the_reload_and_on_later_reloads() {
    let mut st = allowed_a();
    assert!(st.current_is_allowed());
    // 사용자 재로드(F5)도 같은 문서면 허용을 유지한다.
    assert_eq!(load(&mut st, A, Some(scan(1))), [false, true, true]);
    assert!(st.allowance().is_some());
}

#[test]
fn a_fragment_in_the_url_does_not_make_another_document() {
    let mut st = allowed_a();
    assert_eq!(
        load(&mut st, "file:///docs/a.html#section", Some(scan(1))),
        [false, true, true]
    );
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(A));
}

#[test]
fn another_document_clears_the_allowance_and_coming_back_needs_a_new_allow() {
    let mut st = allowed_a();
    assert_eq!(load(&mut st, C, Some(scan(2))), [false, false, false]);
    assert!(st.allowance().is_none());
    assert_eq!(load(&mut st, A, Some(scan(1))), [false, false, false]);
}

#[test]
fn a_changed_file_is_another_document() {
    let mut st = allowed_a();
    assert_eq!(load(&mut st, A, Some(scan(9))), [false, false, false]);
    assert!(st.allowance().is_none());
}

#[test]
fn a_load_that_ends_without_a_commit_restores_the_allowed_document() {
    let mut st = allowed_a();
    // 없는 파일: 응답 단계 없이 실패한다.
    assert!(!st.on_load_started());
    assert_eq!(st.on_load_finished(), Some(true));
    assert!(st.allowance().is_some());
    // 다운로드: main frame 응답은 있지만 commit 없이 끝난다.
    assert!(!st.on_load_started());
    assert!(!st.on_main_response("file:///docs/d.gz", None));
    assert_eq!(st.on_load_finished(), Some(true));
    assert!(st.current_is_allowed());
}

/// process 종료 콜백과 실패 콜백이 함께 오는 백엔드에서 두 번째 종료는 아무것도 바꾸지 않는다.
#[test]
fn a_second_end_of_the_same_load_changes_nothing() {
    let mut st = allowed_a();
    st.on_load_started();
    assert!(!st.on_main_response(C, Some(scan(2))));
    assert_eq!(st.on_load_finished(), Some(true));
    assert!(!st.loading_before_commit());
    assert_eq!(st.on_load_finished(), None);
    assert!(!st.loading_before_commit());
    assert!(st.current_is_allowed());
    assert_eq!(st.allow_current(), Ok(()));
}

#[test]
fn a_load_that_ends_without_a_commit_keeps_an_unallowed_document_off() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.on_load_started();
    assert_eq!(st.on_load_finished(), Some(false));
}

#[test]
fn a_commit_without_a_response_clears_the_allowance_and_cannot_be_allowed() {
    let mut st = allowed_a();
    st.on_load_started();
    assert!(!st.on_committed(Some("file:///docs/c.html#top")));
    assert!(st.allowance().is_none());
    assert_eq!(
        st.current(),
        Some(&DocumentRecord {
            url: C.to_string(),
            scan: None,
        })
    );
    assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
}

#[test]
fn the_pending_response_does_not_leak_into_the_next_load() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, C, Some(scan(2)));
    load(&mut st, A, Some(scan(1)));
    st.allow_current().expect("allow");
    // 허용 재로드: 응답 단계에서 일치한다.
    st.on_load_started();
    assert!(st.on_main_response(A, Some(scan(1))));
    st.on_committed(Some(A));
    st.on_load_finished();
    // 뒤로·앞으로 가기가 응답 단계 없이 commit된다.
    st.on_load_started();
    st.on_committed(Some(C));
    st.on_load_finished();
    st.on_load_started();
    assert!(!st.on_committed(Some(A)));
    st.on_load_finished();
    assert!(st.allowance().is_none());
    // 그 뒤 commit 없는 로드는 JS를 복원하지 않는다.
    st.on_load_started();
    assert_eq!(st.on_load_finished(), Some(false));
}

#[test]
fn a_new_load_drops_the_pending_response_even_without_a_finish() {
    let mut st = allowed_a();
    // 허용 재로드가 응답 단계에서 일치한 뒤, 종료 신호 없이 다음 로드가 시작된다.
    st.on_load_started();
    assert!(st.on_main_response(A, Some(scan(1))));
    st.on_load_started();
    // 새 로드는 응답 단계 없이 다른 문서로 commit된다. 앞 로드의 일치가 넘어오면 안 된다.
    assert!(!st.on_committed(Some(C)));
    assert!(st.allowance().is_none());
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(C));
}

#[test]
fn an_allowed_document_on_screen_does_not_turn_javascript_on_for_a_load_in_flight() {
    let mut st = allowed_a();
    st.on_load_started();
    assert!(!st.effective_js(), "응답 단계 전에는 끈다");
    st.on_main_response(C, Some(scan(2)));
    assert!(!st.effective_js());
    // 로드 중에 sandbox를 다시 켜도 화면 문서의 허용을 새 문서에 적용하지 않는다.
    assert!(!st.set_sandbox(true));
}

#[test]
fn turning_the_sandbox_off_runs_every_document() {
    let mut st = HtmlScriptState::new(false);
    assert_eq!(load(&mut st, A, Some(scan(1))), [true, true, true]);
    assert!(!st.set_sandbox(true));
    assert!(st.set_sandbox(false));
}

#[test]
fn allowing_needs_a_committed_document_with_a_fingerprint() {
    let mut st = HtmlScriptState::new(true);
    assert_eq!(st.allow_current(), Err(AllowError::NoDocument));
    load(&mut st, "about:blank", None);
    assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
    assert!(!st.take_reload_request());
}

#[test]
fn banner_flags_reset_for_a_new_document_and_survive_a_reload() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.banner_mut().dismissed = true;
    load(&mut st, A, Some(scan(1)));
    assert!(
        st.banner().dismissed,
        "같은 문서의 재로드는 표지를 유지한다"
    );
    load(&mut st, C, Some(scan(2)));
    assert_eq!(st.banner(), BannerFlags::default());
}

#[test]
fn strip_fragment_keeps_the_query() {
    assert_eq!(strip_fragment("file:///a.html?x=1#y"), "file:///a.html?x=1");
    assert_eq!(strip_fragment("file:///a.html"), "file:///a.html");
}

/// STARTED → RESPONSE(스캔은 다른 스레드) → COMMITTED → FINISHED. 스캔 결과는 호출자가 따로 넘긴다.
fn load_with_pending_scan(st: &mut HtmlScriptState, url: &str) -> ScanTicket {
    st.on_load_started();
    let (js, ticket) = st.on_main_response_pending(url);
    assert!(!js, "sandbox가 켜져 있으면 판정 전 JS는 꺼져 있다");
    assert!(!st.on_committed(Some(url)));
    assert_eq!(st.on_load_finished(), None);
    ticket
}

#[test]
fn a_scan_that_ends_after_the_commit_attaches_to_that_document() {
    let mut st = HtmlScriptState::new(true);
    let ticket = load_with_pending_scan(&mut st, A);
    assert!(st.scan_pending());
    assert!(st.awaiting_verdict());
    assert_eq!(st.current_detection(), None);
    assert_eq!(
        st.allow_current(),
        Err(AllowError::Loading),
        "지문 전에는 허용하지 않는다"
    );
    assert_eq!(st.on_scan_finished(ticket, Some(scan(1))), Some(false));
    assert!(!st.scan_pending());
    assert_eq!(st.current_detection(), Some(ScriptDetection::Scripts));
    st.allow_current().expect("allow after the scan");
    assert_eq!(
        st.allowance().and_then(|a| a.fingerprint()),
        Some(Fingerprint([1; 32]))
    );
}

#[test]
fn a_scan_that_ends_before_the_commit_is_used_by_the_commit() {
    let mut st = HtmlScriptState::new(true);
    st.on_load_started();
    let (_, ticket) = st.on_main_response_pending(A);
    assert_eq!(st.on_scan_finished(ticket, Some(scan(2))), Some(false));
    assert!(!st.on_committed(Some(A)));
    assert!(!st.scan_pending());
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(2)));
}

#[test]
fn a_late_scan_from_a_replaced_load_is_dropped() {
    let mut st = HtmlScriptState::new(true);
    // 스캔 중 새 로드가 시작됐다.
    st.on_load_started();
    let (_, old) = st.on_main_response_pending(A);
    st.on_load_started();
    let (_, new) = st.on_main_response_pending(C);
    assert_eq!(st.on_scan_finished(old, Some(scan(1))), None);
    assert!(!st.on_committed(Some(C)));
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(C));
    assert_eq!(
        st.current_detection(),
        None,
        "낡은 결과가 새 문서에 붙지 않는다"
    );
    // 다른 문서가 commit된 뒤 도착한 결과도 버린다.
    let later = load_with_pending_scan(&mut st, A);
    assert_eq!(st.on_scan_finished(new, Some(scan(3))), None);
    assert_eq!(st.on_scan_finished(later, Some(scan(4))), Some(false));
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(4)));
}

#[test]
fn a_scan_of_a_failed_load_approves_nothing() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, C, Some(scan(9)));
    st.on_load_started();
    let (_, ticket) = st.on_main_response_pending(A);
    st.on_load_failed();
    assert_eq!(st.on_load_finished(), Some(false));
    assert_eq!(st.on_scan_finished(ticket, Some(scan(1))), None);
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(C));
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(9)));
}

#[test]
fn a_same_url_reload_keeps_the_banner_only_when_the_content_is_unchanged() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.on_user_view();
    st.update_banner();
    st.dismiss_banner();
    // 같은 내용: 스캔 중에도 이전 문서의 감지와 닫힘이 남고, 결과 뒤에도 유지된다.
    let same = load_with_pending_scan(&mut st, A);
    assert_eq!(st.display_detection(), Some(ScriptDetection::Scripts));
    assert!(st.banner().dismissed);
    assert_eq!(st.marker(), Some(ScriptMarker::Blocked));
    st.on_scan_finished(same, Some(scan(1)));
    assert!(st.banner().dismissed);
    // 바뀐 내용: 결과가 오면 새 문서로 보고 닫힘을 푼다.
    let changed = load_with_pending_scan(&mut st, A);
    assert!(st.banner().dismissed, "결과 전에는 잠정 유지");
    st.on_scan_finished(changed, Some(scan(2)));
    assert!(!st.banner().dismissed);
    assert!(
        st.banner().viewed,
        "사용자가 보던 문서의 재로드는 본 것으로 한다"
    );
}

#[test]
fn the_banner_stays_in_the_loading_phase_until_the_scan_ends() {
    let mut st = HtmlScriptState::new(true);
    load(&mut st, A, Some(scan(1)));
    st.on_user_view();
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    let ticket = load_with_pending_scan(&mut st, A);
    assert_eq!(st.update_banner(), BannerPhase::Loading);
    st.on_scan_finished(ticket, Some(scan(1)));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn only_an_allowed_url_needs_the_scan_before_javascript() {
    let mut st = allowed_a();
    assert!(st.needs_scan_before_js(&format!("{A}#x")));
    assert!(!st.needs_scan_before_js(C));
    st.set_sandbox(false);
    assert!(
        !st.needs_scan_before_js(A),
        "sandbox가 꺼지면 결과와 관계없이 켠다"
    );
    assert!(!HtmlScriptState::new(true).needs_scan_before_js(A));
}

/// 결정을 미룬 응답: 허용한 내용과 같으면 commit 전에 켜고, 바뀌었으면 켜지 않는다.
#[test]
fn a_deferred_decision_turns_javascript_on_only_for_the_allowed_content() {
    let mut st = allowed_a();
    st.on_load_started();
    let (js, ticket) = st.on_main_response_pending(A);
    assert!(!js);
    assert_eq!(st.on_scan_finished(ticket, Some(scan(1))), Some(true));
    assert!(st.on_committed(Some(A)));
    assert!(st.current_is_allowed());
    st.on_load_finished();
    st.on_load_started();
    let (_, ticket) = st.on_main_response_pending(A);
    assert_eq!(st.on_scan_finished(ticket, Some(scan(2))), Some(false));
    assert!(!st.on_committed(Some(A)));
    assert!(
        st.allowance().is_none(),
        "내용이 바뀌면 이전 허용을 이어받지 않는다"
    );
}

#[test]
fn a_read_failure_leaves_the_document_without_a_fingerprint() {
    let mut st = HtmlScriptState::new(true);
    let ticket = load_with_pending_scan(&mut st, A);
    assert_eq!(st.on_scan_finished(ticket, None), Some(false));
    assert!(!st.scan_pending());
    assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
}

#[test]
fn without_the_sandbox_a_pending_scan_keeps_javascript_on() {
    let mut st = HtmlScriptState::new(false);
    st.on_load_started();
    let (js, ticket) = st.on_main_response_pending(A);
    assert!(js);
    assert!(st.on_committed(Some(A)));
    assert_eq!(st.on_scan_finished(ticket, Some(scan(1))), Some(true));
}

/// commit A → A 스캔 대기 → 새 로드 B 시작. 늦은 A 결과는 화면 문서 A에만 붙고 B에는 붙지 않는다.
#[test]
fn a_scan_of_the_shown_document_survives_a_new_load_but_never_reaches_it() {
    let mut st = HtmlScriptState::new(true);
    let a = load_with_pending_scan(&mut st, A);
    st.on_load_started();
    let (_, b) = st.on_main_response_pending(C);
    assert!(st.awaiting_verdict());
    assert_eq!(st.allow_current(), Err(AllowError::Loading));
    assert_eq!(st.on_scan_finished(a, Some(scan(1))), Some(false));
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(A));
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(1)));
    assert!(st.scan_pending(), "B의 스캔은 여전히 대기");
    assert!(!st.on_committed(Some(C)));
    assert_eq!(st.current().and_then(|d| d.scan), None);
    assert_eq!(st.on_scan_finished(b, Some(scan(2))), Some(false));
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(2)));
}

/// commit A → A 스캔 대기 → 새 로드 B 시작 → B가 commit된 뒤 늦은 A 결과는 버린다.
#[test]
fn a_late_scan_after_the_next_commit_is_dropped() {
    let mut st = HtmlScriptState::new(true);
    let a = load_with_pending_scan(&mut st, A);
    let b = load_with_pending_scan(&mut st, C);
    assert_eq!(st.on_scan_finished(a, Some(scan(1))), None);
    assert_eq!(st.current().map(|d| d.url.as_str()), Some(C));
    assert_eq!(st.current().and_then(|d| d.scan), None);
    assert_eq!(st.on_scan_finished(b, Some(scan(2))), Some(false));
}

/// commit A 뒤 그 로드가 실패하면 늦은 A 결과로 허용 가능한 지문을 만들지 않는다.
#[test]
fn a_late_scan_of_a_load_that_failed_after_its_commit_is_dropped() {
    let mut st = HtmlScriptState::new(true);
    st.on_load_started();
    let (_, a) = st.on_main_response_pending(A);
    st.on_committed(Some(A));
    st.on_load_failed();
    st.on_load_finished();
    assert!(!st.scan_pending());
    assert_eq!(st.on_scan_finished(a, Some(scan(1))), None);
    assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
}

/// 새 로드 B가 commit 전에 실패해도 화면 문서 A의 대기는 A의 것이므로 유지한다.
#[test]
fn a_failure_of_the_next_load_keeps_the_shown_documents_scan() {
    let mut st = HtmlScriptState::new(true);
    let a = load_with_pending_scan(&mut st, A);
    st.on_load_started();
    let (_, b) = st.on_main_response_pending(C);
    st.on_load_failed();
    st.on_load_finished();
    assert_eq!(st.on_scan_finished(b, Some(scan(2))), None);
    assert_eq!(st.on_scan_finished(a, Some(scan(1))), Some(false));
    assert_eq!(st.current().and_then(|d| d.scan), Some(scan(1)));
}
