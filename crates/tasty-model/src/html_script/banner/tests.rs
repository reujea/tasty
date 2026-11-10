use super::super::{AllowError, Fingerprint, ScriptDetection, ScriptScan};
use super::*;

const A: &str = "file:///docs/a.html";
const B: &str = "file:///docs/b.html";

fn scan(byte: u8, detection: ScriptDetection) -> ScriptScan {
    ScriptScan {
        fingerprint: Fingerprint([byte; 32]),
        detection,
        remote_refs: false,
        remote_scripts: false,
    }
}

fn load(st: &mut HtmlScriptState, url: &str, s: ScriptScan) {
    st.on_load_started();
    st.on_main_response(url, Some(s));
    st.on_committed(Some(url));
    assert_eq!(st.on_load_finished(), None);
}

/// 호스트가 URL을 넣은 로드(plugin·IPC·복원).
fn host_load(st: &mut HtmlScriptState, url: &str, s: ScriptScan) {
    st.on_host_load_requested();
    load(st, url, s);
}

fn scripts(byte: u8) -> ScriptScan {
    scan(byte, ScriptDetection::Scripts)
}

#[test]
fn a_document_nobody_has_viewed_only_records_pending_view() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert!(st.banner().pending_view);
    assert!(!st.banner().shown);
}

#[test]
fn selecting_the_surface_fires_the_pending_banner() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.on_user_view();
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    assert!(st.banner().shown);
    assert!(!st.banner().pending_view);
}

#[test]
fn a_user_open_counts_when_the_surface_was_selected_before_its_first_document() {
    // 사용자가 연 파일은 새 탭이 사용자 선택으로 활성화된 뒤 첫 로드가 온다.
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn a_host_load_into_a_viewed_surface_waits_for_the_next_selection() {
    // 에이전트가 사용자가 보던 surface에 다른 문서를 넣는 경우.
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    assert!(st.banner().viewed);
    host_load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert!(st.banner().pending_view);
}

#[test]
fn an_in_page_navigation_keeps_the_view() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn selecting_the_surface_during_a_load_applies_to_the_committed_document() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    st.on_host_load_requested();
    st.on_load_started();
    st.on_user_view();
    st.on_main_response(B, Some(scripts(2)));
    st.on_committed(Some(B));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn a_selection_before_a_host_load_does_not_carry_into_it() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    st.on_user_view();
    host_load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
}

#[test]
fn no_banner_without_scripts_with_the_sandbox_off_or_once_dismissed() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert!(!st.banner().pending_view);

    let mut st = HtmlScriptState::new(false);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);

    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    st.dismiss_banner();
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    st.reshow_banner();
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn remote_only_scripts_fire_once_remote_content_is_open() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::ScriptsRemoteOnly));
    assert_eq!(
        st.update_banner(),
        BannerPhase::Hidden,
        "network scripts cannot run while remote content is blocked"
    );
    st.set_remote_global(true);
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn allowing_shows_reloading_until_the_reload_commits() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.allow_current().expect("allow");
    assert_eq!(st.update_banner(), BannerPhase::Reloading);
    st.on_load_started();
    st.on_main_response(A, Some(scripts(1)));
    assert_eq!(st.update_banner(), BannerPhase::Reloading);
    st.on_committed(Some(A));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert!(st.banner().shown, "같은 문서라 표지는 남는다");
}

#[test]
fn a_reload_that_never_commits_ends_the_reloading_phase() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.allow_current().expect("allow");
    st.on_load_started();
    assert_eq!(st.on_load_finished(), Some(true));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
}

#[test]
fn allow_waits_while_a_reload_with_changed_content_is_loading() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    // 화면 문서 A를 다시 읽는 중, 응답 단계에서 내용이 바뀐 것이 보였고 아직 commit 전이다.
    st.on_load_started();
    assert_eq!(st.update_banner(), BannerPhase::Loading);
    st.on_main_response(A, Some(scripts(2)));
    assert!(st.loading_before_commit());
    assert_eq!(st.update_banner(), BannerPhase::Loading);
    // 버튼은 비활성이고, 다른 경로로 허용을 불러도 기록하지 않는다.
    assert_eq!(st.allow_current(), Err(AllowError::Loading));
    assert!(st.allowance().is_none());
    assert!(!st.take_reload_request());
    // commit 뒤에는 새 내용의 문서가 화면 문서가 되고, 스크립트가 있어 다시 허용할 수 있다.
    st.on_committed(Some(A));
    assert!(!st.loading_before_commit());
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    st.allow_current().expect("allow after commit");
    assert_eq!(
        st.allowance().and_then(|a| a.fingerprint()),
        Some(Fingerprint([2; 32])),
        "허용은 commit된 새 내용에 묶인다"
    );
}

#[test]
fn a_load_that_ends_without_a_commit_returns_to_blocked() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.on_load_started();
    assert_eq!(st.update_banner(), BannerPhase::Loading);
    assert_eq!(st.on_load_finished(), Some(false));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    st.allow_current().expect("allow once the load is gone");
}

#[test]
fn another_document_drops_the_banner_and_decides_again() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.dismiss_banner();
    // 사용자가 보던 문서의 링크로 다른 문서에 간다. 닫힘은 풀리고 다시 판정한다.
    load(&mut st, B, scripts(2));
    assert!(!st.banner().dismissed);
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn a_fragment_move_or_a_same_content_reload_keeps_the_flags() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    st.dismiss_banner();
    host_load(&mut st, &format!("{A}#part"), scripts(1));
    assert!(st.banner().dismissed);
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
}

#[test]
fn turning_the_sandbox_off_hides_a_shown_banner() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    st.set_sandbox(false);
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
}

#[test]
fn the_marker_follows_dismiss_allow_and_the_document() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scripts(1));
    st.update_banner();
    assert_eq!(st.marker(), None, "배너가 떠 있는 동안에는 표지가 없다");
    st.dismiss_banner();
    assert_eq!(st.marker(), Some(ScriptMarker::Blocked));
    st.reshow_banner();
    assert_eq!(st.marker(), None);
    st.allow_current().expect("allow");
    assert_eq!(st.marker(), Some(ScriptMarker::Allowed));
    load(&mut st, B, scripts(2));
    assert_eq!(st.marker(), None, "다른 문서는 허용을 잇지 않는다");
}

#[test]
fn no_marker_without_scripts_or_with_the_sandbox_off() {
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    st.dismiss_banner();
    assert_eq!(st.marker(), None);
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scripts(1));
    st.dismiss_banner();
    st.set_sandbox(false);
    assert_eq!(st.marker(), None);
}

#[test]
fn a_failed_load_hides_the_banner_and_the_marker_until_a_document_commits() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    load(&mut st, A, scripts(1));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);

    st.on_load_started();
    st.on_load_failed();
    assert_eq!(st.on_load_finished(), Some(false));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert_eq!(st.marker(), None);

    st.on_load_started();
    assert_eq!(
        st.update_banner(),
        BannerPhase::Hidden,
        "commit 전 재로드는 실패 상태를 유지한다"
    );
    st.on_main_response(A, Some(scripts(1)));
    st.on_committed(Some(A));
    assert_eq!(st.on_load_finished(), None);
    assert!(!st.load_failed());
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn a_failed_load_hides_the_marker_of_a_dismissed_or_allowed_document() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    load(&mut st, A, scripts(1));
    st.dismiss_banner();
    assert_eq!(st.marker(), Some(ScriptMarker::Blocked));
    st.on_load_failed();
    assert_eq!(st.marker(), None);
    load(&mut st, A, scripts(1));
    assert_eq!(
        st.marker(),
        Some(ScriptMarker::Blocked),
        "같은 문서가 다시 commit되면 닫힘 표지를 다시 정한다"
    );

    st.allow_current().expect("allow");
    load(&mut st, A, scripts(1));
    assert_eq!(st.marker(), Some(ScriptMarker::Allowed));
    st.on_load_failed();
    assert_eq!(st.marker(), None);
}

#[test]
fn a_user_load_into_an_unviewed_surface_fires_on_commit() {
    // 주소 입력·뒤로·앞으로·새로고침은 사용자가 그 surface를 보고 있는 조작이다.
    let mut st = HtmlScriptState::new(true);
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    st.on_user_load_requested();
    load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
    assert!(st.banner().shown);
}

#[test]
fn a_user_load_overrides_a_host_load_mark_left_before_it() {
    // 호스트 로드 요청 뒤 commit 전에 사용자가 다시 탐색하면 사용자 로드로 본다.
    let mut st = HtmlScriptState::new(true);
    st.on_user_view();
    host_load(&mut st, A, scan(1, ScriptDetection::None));
    st.on_host_load_requested();
    st.on_user_load_requested();
    load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Blocked);
}

#[test]
fn a_host_load_after_a_committed_user_load_waits_again() {
    let mut st = HtmlScriptState::new(true);
    st.on_user_load_requested();
    load(&mut st, A, scan(1, ScriptDetection::None));
    host_load(&mut st, B, scripts(2));
    assert_eq!(st.update_banner(), BannerPhase::Hidden);
    assert!(st.banner().pending_view);
}
