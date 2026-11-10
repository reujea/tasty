//! 로컬 view에 연 top-level `http(s)` 페이지의 원격·스크립트 허용(docs/surfaces/html/index.md
//! "`http(s)` 페이지의 허용").

use super::super::{AllowError, Allowed, BannerPhase, HtmlScriptState, ScriptMarker};
use super::{RemoteAllowError, RemoteMarker, RemoteView};

const B: &str = "http://example.test/b.html";
const C: &str = "http://example.test/c.html";
const FILE: &str = "file:///tmp/a.html";

/// 지문 없는 문서 로드. 게이트의 `respond_without_file`과 같은 순서다.
fn load_page(st: &mut HtmlScriptState, url: &str) {
    st.on_load_started();
    st.on_main_response(url, None);
    st.on_committed(Some(url));
    st.on_load_finished();
}

/// 사용자가 주소 칸·새로고침·뒤로로 연다.
fn user_load(st: &mut HtmlScriptState, url: &str) {
    st.on_user_load_requested();
    load_page(st, url);
}

/// 허용이 요청한 재로드. 호스트는 요청을 꺼내 그대로 다시 로드한다.
fn allow_reload(st: &mut HtmlScriptState, url: &str) {
    assert!(st.take_reload_request(), "allowing requests one reload");
    load_page(st, url);
}

fn opened(sandbox: bool) -> HtmlScriptState {
    let mut st = HtmlScriptState::new(sandbox);
    user_load(&mut st, B);
    st.update_banner();
    st
}

#[test]
fn an_opened_http_document_shows_the_remote_notice_and_accepts_a_url_allowance() {
    let mut st = opened(true);
    assert!(st.current_is_web_page());
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert_eq!(st.remote_marker(), None, "no marker while the notice shows");
    st.allow_remote_current()
        .expect("a page takes a URL allowance");
    assert_eq!(
        st.remote_allowance(),
        Some(&Allowed::Page { url: B.to_string() })
    );
    assert_eq!(st.remote_allowance().and_then(Allowed::fingerprint), None);
    assert_eq!(st.remote_banner_phase(), BannerPhase::Reloading);

    assert!(st.take_reload_request());
    st.on_load_started();
    st.on_main_response(B, None);
    assert!(
        st.document_remote_allowed(),
        "the allow reload opens remote"
    );
    st.on_committed(Some(B));
    st.on_load_finished();
    assert!(st.current_is_remote_allowed());
    assert_eq!(
        st.update_banner(),
        BannerPhase::Blocked,
        "the script notice follows"
    );
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Allowed));

    // 사용자 새로고침은 허용을 풀고 안내를 바로 다시 보인다.
    st.on_user_load_requested();
    st.on_load_started();
    st.on_main_response(B, None);
    assert!(!st.document_remote_allowed());
    st.on_committed(Some(B));
    st.on_load_finished();
    assert!(st.remote_allowance().is_none());
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert_eq!(st.remote_marker(), None);
}

#[test]
fn a_local_file_without_a_fingerprint_is_still_refused() {
    let mut st = HtmlScriptState::new(true);
    load_page(&mut st, FILE);
    assert!(!st.current_is_web_page());
    assert_eq!(
        st.allow_remote_current(),
        Err(RemoteAllowError::Document(AllowError::NoFingerprint))
    );
    assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
}

#[test]
fn a_page_allowance_never_matches_a_file_document_with_the_same_address() {
    let mut st = HtmlScriptState::new(true);
    load_page(&mut st, FILE);
    // 기록을 직접 만들 수 없으므로 판정 규칙만 본다: 페이지 허용은 http(s) 주소에만 맞는다.
    let page = Allowed::Page {
        url: FILE.to_string(),
    };
    assert!(!page.matches_document(st.current().expect("document")));
}

#[test]
fn an_agent_reload_releases_the_allowance() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    allow_reload(&mut st, B);
    assert!(st.current_is_remote_allowed());
    st.on_host_load_requested();
    load_page(&mut st, B);
    assert!(st.remote_allowance().is_none());
    assert!(!st.remote_effectively_allowed());
}

#[test]
fn a_reload_requested_before_the_allow_reload_starts_takes_its_place() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    // 사용자가 허용 재로드보다 먼저 새로고침했다. 그 로드는 허용 재로드가 아니다.
    st.on_user_load_requested();
    load_page(&mut st, B);
    assert!(st.remote_allowance().is_none());
}

#[test]
fn moving_to_another_page_and_back_releases_the_allowance() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    allow_reload(&mut st, B);
    assert!(st.navigation_origin().remote_allowed);
    // 링크로 C.
    load_page(&mut st, C);
    assert!(st.remote_allowance().is_none());
    assert!(!st.current_is_remote_allowed());
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    // 뒤로 B(응답 단계 없는 bfcache 복원 포함).
    st.on_user_load_requested();
    st.on_load_started();
    st.on_committed(Some(B));
    st.on_load_finished();
    assert!(!st.current_is_remote_allowed());
}

#[test]
fn a_redirect_during_the_allow_reload_does_not_carry_the_allowance() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    assert!(st.take_reload_request());
    st.on_load_started();
    st.on_main_response(C, None);
    assert!(!st.document_remote_allowed());
    st.on_committed(Some(C));
    assert!(st.remote_allowance().is_none());
}

#[test]
fn a_commit_without_a_response_releases_the_allowance() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    allow_reload(&mut st, B);
    st.on_load_started();
    st.on_committed(Some(B));
    assert!(st.remote_allowance().is_none());
}

#[test]
fn a_load_that_fails_before_commit_keeps_the_allowance() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow");
    allow_reload(&mut st, B);
    st.on_user_load_requested();
    st.on_load_started();
    st.on_main_response(C, None);
    assert!(!st.document_remote_allowed());
    st.on_load_failed();
    assert_eq!(st.on_load_finished(), Some(false));
    assert!(st.current_is_remote_allowed());
    assert!(st.document_remote_allowed());
}

#[test]
fn allowing_a_page_is_refused_while_loading() {
    let mut st = opened(true);
    st.on_user_load_requested();
    st.on_load_started();
    assert_eq!(
        st.allow_remote_current(),
        Err(RemoteAllowError::Document(AllowError::Loading))
    );
}

#[test]
fn a_dismissed_page_notice_leaves_a_marker_that_reshows_it() {
    let mut st = opened(true);
    st.dismiss_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Blocked));
    assert_eq!(st.banner_phase(), BannerPhase::Hidden, "no script notice");
    assert_eq!(st.marker(), None);
    st.reshow_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert!(st.remote_allowance().is_none(), "the marker does not allow");
}

#[test]
fn the_script_notice_comes_second_and_both_allowances_last_one_showing() {
    let mut st = opened(true);
    assert_eq!(
        st.banner_phase(),
        BannerPhase::Hidden,
        "remote is asked first"
    );
    st.allow_remote_current().expect("allow remote");
    allow_reload(&mut st, B);
    assert!(!st.effective_js(), "remote allow does not run scripts");
    assert!(st.allowance().is_none());
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    assert!(!st.script_banner_needs_remote_body());

    st.allow_current().expect("allow scripts on the page");
    assert_eq!(st.allowance(), Some(&Allowed::Page { url: B.to_string() }));
    assert_eq!(st.banner_phase(), BannerPhase::Reloading);
    allow_reload(&mut st, B);
    assert!(st.effective_js());
    assert!(
        st.current_is_remote_allowed(),
        "the script reload keeps remote"
    );
    assert_eq!(st.marker(), Some(ScriptMarker::Allowed));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);

    // 새로고침은 둘 다 풀고 원격부터 다시 묻는다.
    user_load(&mut st, B);
    assert!(!st.effective_js());
    assert!(!st.current_is_remote_allowed());
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    st.allow_remote_current().expect("allow remote again");
    allow_reload(&mut st, B);
    assert_eq!(
        st.update_banner(),
        BannerPhase::Blocked,
        "scripts asked again"
    );
}

#[test]
fn the_global_remote_switch_asks_for_scripts_at_once() {
    let mut st = HtmlScriptState::new(true);
    st.set_remote_global(true);
    user_load(&mut st, B);
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), None);
}

#[test]
fn without_the_sandbox_there_is_no_script_notice() {
    let mut st = opened(false);
    st.allow_remote_current().expect("allow remote");
    allow_reload(&mut st, B);
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert_eq!(st.marker(), None);
    assert!(st.effective_js());
}

#[test]
fn other_views_take_no_page_allowance() {
    for view in [RemoteView::Http, RemoteView::Mirror] {
        let mut st = HtmlScriptState::new(true);
        st.set_remote_view(view);
        user_load(&mut st, B);
        assert!(!st.current_is_web_page());
        assert_eq!(
            st.allow_remote_current(),
            Err(RemoteAllowError::UnsupportedView)
        );
        assert_eq!(st.allow_current(), Err(AllowError::NoFingerprint));
        assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    }
}

#[test]
fn a_reload_shows_the_page_notice_again_even_after_dismissing_it() {
    let mut st = opened(true);
    st.dismiss_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Blocked));
    user_load(&mut st, B);
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert_eq!(st.remote_marker(), None);
}

#[test]
fn an_agent_opened_page_waits_for_the_user_to_view_it() {
    let mut st = HtmlScriptState::new(true);
    st.on_host_load_requested();
    load_page(&mut st, FILE);
    st.on_user_view();
    st.on_host_load_requested();
    load_page(&mut st, B);
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert!(st.remote_banner().pending_view);
    st.on_user_view();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
}

#[test]
fn the_script_notice_waits_for_the_remote_reload_to_commit() {
    let mut st = opened(true);
    st.allow_remote_current().expect("allow remote");
    assert!(st.take_reload_request());
    // 허용 재로드가 commit되기 전에는 원격 안내(reloading)만 보인다.
    for step in 0..3 {
        match step {
            1 => {
                st.on_load_started();
            }
            2 => {
                st.on_main_response(B, None);
            }
            _ => {}
        }
        assert_eq!(st.update_banner(), BannerPhase::Hidden, "step {step}");
        assert_eq!(st.remote_banner_phase(), BannerPhase::Reloading);
    }
    st.on_committed(Some(B));
    assert_eq!(st.update_banner(), BannerPhase::Blocked, "after the commit");
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    st.on_load_finished();
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}
