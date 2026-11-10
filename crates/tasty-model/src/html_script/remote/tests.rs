use super::super::{Fingerprint, ScriptDetection, ScriptScan};
use super::*;

const A: &str = "file:///docs/a.html";
const B: &str = "file:///docs/b.html";

fn scan(
    byte: u8,
    detection: ScriptDetection,
    remote_refs: bool,
    remote_scripts: bool,
) -> ScriptScan {
    ScriptScan {
        fingerprint: Fingerprint([byte; 32]),
        detection,
        remote_refs,
        remote_scripts,
    }
}

/// 원격 CSS·이미지만 있고 스크립트가 없는 문서.
fn remote_page(byte: u8) -> ScriptScan {
    scan(byte, ScriptDetection::None, true, false)
}

fn plain(byte: u8) -> ScriptScan {
    scan(byte, ScriptDetection::None, false, false)
}

fn load(st: &mut HtmlScriptState, url: &str, s: ScriptScan) {
    st.on_load_started();
    st.on_main_response(url, Some(s));
    st.on_committed(Some(url));
    st.on_load_finished();
}

/// 게이트와 같은 순서의 로드. 허용 기록이 있는 URL만 스캔을 기다리고, 그 밖에는 스캔이 commit 뒤에 끝난다.
fn gate_load(st: &mut HtmlScriptState, url: &str, s: ScriptScan) {
    st.on_load_started();
    if st.needs_scan_before_js(url) {
        st.on_main_response(url, Some(s));
        st.on_committed(Some(url));
    } else {
        let (_, ticket) = st.on_main_response_pending(url);
        st.on_committed(Some(url));
        st.on_scan_finished(ticket, Some(s));
    }
    st.on_load_finished();
}

fn host_load(st: &mut HtmlScriptState, url: &str, s: ScriptScan) {
    st.on_host_load_requested();
    load(st, url, s);
}

fn viewed(s: ScriptScan) -> HtmlScriptState {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, s);
    st.update_banner();
    st
}

#[test]
fn an_agent_opened_document_records_pending_view_until_the_user_selects_it() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, remote_page(1));
    st.on_user_view(); // the selection lands before the first commit only on the first load
    let mut st2 = HtmlScriptState::new(true);
    host_load(&mut st2, A, plain(1));
    host_load(&mut st2, B, remote_page(2));
    st2.update_banner();
    assert_eq!(st2.remote_banner_phase(), BannerPhase::Hidden);
    assert!(st2.remote_banner().pending_view);
    st2.on_user_view();
    st2.update_banner();
    assert_eq!(st2.remote_banner_phase(), BannerPhase::Blocked);
    assert!(!st2.remote_banner().pending_view);
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
}

#[test]
fn no_banner_without_refs_with_the_global_switch_or_in_an_unsupported_view() {
    assert_eq!(viewed(plain(1)).remote_banner_phase(), BannerPhase::Hidden);
    let mut st = viewed(remote_page(1));
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    st.set_remote_global(true);
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), None);
    let mut st = viewed(remote_page(1));
    st.set_remote_view(RemoteView::Mirror);
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(
        st.allow_remote_current(),
        Err(RemoteAllowError::UnsupportedView)
    );
}

#[test]
fn allowing_reloads_and_opens_only_that_document() {
    let mut st = viewed(remote_page(1));
    assert!(!st.document_remote_allowed());
    st.allow_remote_current().unwrap();
    assert!(st.take_reload_request());
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Reloading);
    // The reload's response matches the allowance, so the load is opened before its commit.
    st.on_load_started();
    st.on_main_response(A, Some(remote_page(1)));
    assert!(st.document_remote_allowed(), "decided at the response");
    st.on_committed(Some(A));
    st.on_load_finished();
    st.update_banner();
    assert!(st.current_is_remote_allowed());
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Allowed));
    // Another document: blocked from its response on, allowance released at its commit.
    st.on_load_started();
    st.on_main_response(B, Some(remote_page(2)));
    assert!(!st.document_remote_allowed());
    st.on_committed(Some(B));
    assert!(st.remote_allowance().is_none());
    // Back to A: blocked again.
    load(&mut st, A, remote_page(1));
    assert!(!st.document_remote_allowed());
}

#[test]
fn a_changed_fingerprint_revokes_the_allowance() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    load(&mut st, A, remote_page(1));
    assert!(st.current_is_remote_allowed());
    st.on_load_started();
    st.on_main_response(A, Some(remote_page(9)));
    assert!(!st.document_remote_allowed(), "same URL, other content");
    st.on_committed(Some(A));
    assert!(st.remote_allowance().is_none());
}

#[test]
fn a_load_that_never_commits_returns_to_the_shown_document() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    load(&mut st, A, remote_page(1));
    st.on_load_started();
    st.on_main_response(B, Some(remote_page(2)));
    assert!(!st.document_remote_allowed());
    st.on_load_failed();
    st.on_load_finished();
    assert!(
        st.document_remote_allowed(),
        "the committed document is still A and still allowed"
    );
    assert_eq!(st.remote_marker(), None, "failed loads hide the marker");
}

#[test]
fn an_allowance_waits_for_the_scan_before_deciding() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    assert!(
        st.needs_scan_before_js(A),
        "a remote allowance needs the fingerprint first"
    );
    assert!(!st.needs_scan_before_js(B));
    st.set_remote_global(true);
    assert!(
        st.needs_scan_before_js(A),
        "the allowance outlives a global switch, so its document still waits"
    );
}

#[test]
fn reloading_with_the_global_switch_on_keeps_the_document_allowance() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    gate_load(&mut st, A, remote_page(1));
    st.set_remote_global(true);
    gate_load(&mut st, A, remote_page(1));
    assert!(
        st.remote_allowance().is_some(),
        "the same document reloaded"
    );
    assert!(
        st.set_remote_global(false),
        "still open once the switch is off"
    );
    // A changed fingerprint still revokes it while the switch is on.
    st.set_remote_global(true);
    gate_load(&mut st, A, remote_page(9));
    assert!(st.remote_allowance().is_none());
    assert!(!st.set_remote_global(false));
}

#[test]
fn toggling_the_global_switch_keeps_the_document_allowance() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    load(&mut st, A, remote_page(1));
    assert!(st.set_remote_global(true));
    assert!(
        st.set_remote_global(false),
        "the allowed document stays open"
    );
    let mut st = viewed(remote_page(1));
    assert!(
        !st.set_remote_global(true),
        "the document part stays closed"
    );
    assert_eq!(
        st.allow_remote_current(),
        Err(RemoteAllowError::GloballyAllowed)
    );
}

#[test]
fn allowing_is_refused_while_loading() {
    let mut st = viewed(remote_page(1));
    st.on_load_started();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Loading);
    assert_eq!(
        st.allow_remote_current(),
        Err(RemoteAllowError::Document(AllowError::Loading))
    );
    st.on_main_response(A, Some(remote_page(1)));
    st.on_committed(Some(A));
    st.on_load_finished();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
}

#[test]
fn dismissing_leaves_a_marker_that_reshows_without_allowing() {
    let mut st = viewed(remote_page(1));
    assert_eq!(st.remote_marker(), None, "no marker while the notice shows");
    st.dismiss_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Blocked));
    st.reshow_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert!(st.remote_allowance().is_none());
}

#[test]
fn an_observed_block_without_refs_gives_a_marker_only() {
    let mut st = viewed(plain(1));
    st.on_remote_block_observed();
    st.update_banner();
    assert_eq!(
        st.remote_banner_phase(),
        BannerPhase::Hidden,
        "no late notice"
    );
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Blocked));
    st.reshow_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    st.dismiss_remote_banner();
    st.update_banner();
    assert_eq!(st.remote_marker(), Some(RemoteMarker::Blocked));
    // A new document starts without the observation.
    load(&mut st, B, plain(2));
    assert_eq!(st.remote_marker(), None);
}

#[test]
fn observations_during_a_load_are_ignored() {
    let mut st = viewed(plain(1));
    st.on_load_started();
    st.on_remote_block_observed();
    st.on_main_response(B, Some(plain(2)));
    st.on_committed(Some(B));
    st.on_load_finished();
    assert_eq!(st.remote_marker(), None);
}

#[test]
fn script_and_remote_allowances_are_independent() {
    let mixed = scan(1, ScriptDetection::Scripts, true, true);
    // remote only
    let mut st = viewed(mixed);
    st.allow_remote_current().unwrap();
    load(&mut st, A, mixed);
    assert!(st.current_is_remote_allowed());
    assert!(!st.current_is_allowed());
    assert!(!st.effective_js());
    // script only
    let mut st = viewed(mixed);
    st.allow_current().unwrap();
    load(&mut st, A, mixed);
    assert!(st.current_is_allowed());
    assert!(!st.current_is_remote_allowed());
    assert!(!st.document_remote_allowed());
    // both, one reload each
    let mut st = viewed(mixed);
    st.allow_current().unwrap();
    st.allow_remote_current().unwrap();
    assert!(st.take_reload_request());
    assert!(
        !st.take_reload_request(),
        "both allows share one reload request"
    );
    load(&mut st, A, mixed);
    assert!(st.current_is_allowed() && st.current_is_remote_allowed());
}

#[test]
fn the_script_notice_follows_runnable_scripts_and_picks_its_body() {
    // network scripts only: remote notice only
    let st = viewed(scan(1, ScriptDetection::ScriptsRemoteOnly, true, true));
    assert_eq!(st.banner_phase(), BannerPhase::Hidden);
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    // after the remote allow: the script notice with its plain body
    let mut st = viewed(scan(1, ScriptDetection::ScriptsRemoteOnly, true, true));
    st.allow_remote_current().unwrap();
    load(
        &mut st,
        A,
        scan(1, ScriptDetection::ScriptsRemoteOnly, true, true),
    );
    st.update_banner();
    assert_eq!(st.banner_phase(), BannerPhase::Blocked);
    assert!(!st.script_banner_needs_remote_body());
    // local + network scripts: both notices, the script body names the network scripts
    let st = viewed(scan(1, ScriptDetection::Scripts, true, true));
    assert_eq!(st.banner_phase(), BannerPhase::Blocked);
    assert_eq!(st.remote_banner_phase(), BannerPhase::Blocked);
    assert!(st.script_banner_needs_remote_body());
    // local scripts + remote CSS: both notices, plain script body
    let st = viewed(scan(1, ScriptDetection::Scripts, true, false));
    assert!(!st.script_banner_needs_remote_body());
    // the remote notice closed: plain body
    let mut st = viewed(scan(1, ScriptDetection::Scripts, true, true));
    st.dismiss_remote_banner();
    st.update_banner();
    assert!(!st.script_banner_needs_remote_body());
}

/// view마다 원격 실효값과 원격 전용 스크립트 배너가 그 view의 실제 정책을 따른다.
/// mirror는 전역 설정과 관계없이 원격을 막으므로 원격 전용 스크립트는 실행될 수 없다.
/// HTTP view는 같은 origin 스크립트를 endpoint가 허용하므로 원래대로 스크립트 배너를 띄운다.
#[test]
fn each_view_reports_its_own_remote_policy() {
    let remote_scripts = || scan(1, ScriptDetection::ScriptsRemoteOnly, true, true);
    let case = |view: RemoteView, global: bool| {
        let mut st = HtmlScriptState::new(true);
        st.set_remote_view(view);
        st.set_remote_global(global);
        st.on_user_view();
        host_load(&mut st, A, remote_scripts());
        st.update_banner();
        (st.remote_effectively_allowed(), st.banner_phase())
    };
    assert_eq!(case(RemoteView::Local, false), (false, BannerPhase::Hidden));
    assert!(case(RemoteView::Local, true).0);
    assert_ne!(case(RemoteView::Local, true).1, BannerPhase::Hidden);
    assert_eq!(
        case(RemoteView::Mirror, false),
        (false, BannerPhase::Hidden)
    );
    assert_eq!(case(RemoteView::Mirror, true), (false, BannerPhase::Hidden));
    for global in [false, true] {
        let (effective, phase) = case(RemoteView::Http, global);
        assert_eq!(effective, global);
        assert_ne!(phase, BannerPhase::Hidden, "global {global}");
    }
    let mut st = HtmlScriptState::new(true);
    st.set_remote_view(RemoteView::Mirror);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::Scripts, true, false));
    st.update_banner();
    assert_ne!(
        st.banner_phase(),
        BannerPhase::Hidden,
        "local scripts still ask"
    );
}

/// 허용한 문서에서 연 top-level `http(s)` 탐색은 그 로드 동안 실효값을 따른다. commit되면 다른 문서이므로
/// 허용이 풀리고, 원래 문서로 돌아와도 다시 허용해야 열린다(URL·지문 묶음).
#[test]
fn a_remote_navigation_from_an_allowed_document_releases_the_allowance() {
    let mut st = viewed(remote_page(1));
    st.allow_remote_current().unwrap();
    assert!(st.remote_effectively_allowed());
    gate_load(&mut st, "https://e.example/page", plain(9));
    assert!(!st.current_is_remote_allowed());
    assert!(!st.remote_effectively_allowed());
    gate_load(&mut st, A, remote_page(1));
    assert!(!st.remote_effectively_allowed(), "back to the document");
}

/// top-level 탐색 허용의 근거는 commit된 화면 문서다. 허용한 문서에서 연 원격 로드가 commit되면 근거가
/// 새 문서로 옮겨 가지 않고, commit 없이 실패하면 원래 문서의 근거가 남는다.
#[test]
fn the_navigation_origin_stays_with_the_committed_document() {
    let mut st = viewed(remote_page(1));
    assert!(!st.navigation_origin().remote_allowed);
    st.allow_remote_current().unwrap();
    gate_load(&mut st, A, remote_page(1));
    assert!(st.navigation_origin().remote_allowed);

    st.on_load_started();
    assert!(
        !st.document_remote_allowed(),
        "the load's own requests stay blocked"
    );
    assert!(
        st.navigation_origin().remote_allowed,
        "the document on screen still opens its redirect hops"
    );
    st.on_load_failed();
    st.on_load_finished();
    assert!(
        st.navigation_origin().remote_allowed,
        "a failed load keeps A"
    );
    assert!(st.document_remote_allowed());

    gate_load(&mut st, "http://127.0.0.1:8000/b.html", plain(9));
    let origin = st.navigation_origin();
    assert!(!origin.remote_allowed, "B does not inherit the allowance");
    assert!(origin.remote_document);
    assert!(st.remote_allowance().is_none());
}

#[test]
fn the_global_setting_opens_every_origin_but_not_a_mirror() {
    let mut st = viewed(plain(1));
    st.set_remote_global(true);
    assert!(st.navigation_origin().remote_allowed);
    st.set_remote_view(RemoteView::Mirror);
    assert!(!st.navigation_origin().remote_allowed);
}
