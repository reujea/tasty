use super::*;

const A: &str = "file:///docs/a.html";
const B: &str = "http://127.0.0.1:8000/b.html";
const HOP: &str = "http://127.0.0.1:8000/redirect?to=b";

const BLOCKED_LOCAL: Origin = Origin {
    remote_allowed: false,
    remote_document: false,
};
const ALLOWED_LOCAL: Origin = Origin {
    remote_allowed: true,
    remote_document: false,
};
const BLOCKED_REMOTE: Origin = Origin {
    remote_allowed: false,
    remote_document: true,
};

#[test]
fn a_page_navigation_from_a_blocked_local_document_stays_closed() {
    let mut t = TopLevelAdmission::default();
    assert!(!t.start(B, StartKind::Other, false, BLOCKED_LOCAL));
    assert!(
        !t.start(B, StartKind::Other, true, BLOCKED_LOCAL),
        "a click too"
    );
    assert!(!t.admitting());
    assert!(!t.redirect(B), "no open load to follow");
}

#[test]
fn an_allowed_document_opens_its_navigation_and_every_redirect_hop_until_commit() {
    let mut t = TopLevelAdmission::default();
    assert!(t.start(HOP, StartKind::Other, false, ALLOWED_LOCAL));
    assert!(t.admits(HOP));
    assert!(t.redirect(B));
    assert!(t.admits(B));
    assert!(!t.admits(HOP), "the hop moved on");
    t.end();
    assert!(!t.admitting());
    assert!(
        !t.redirect("http://127.0.0.1:8000/c.html"),
        "closed at commit"
    );
}

#[test]
fn a_designated_url_opens_once_whatever_the_document_on_screen() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Url("HTTP://127.0.0.1:8000".into()));
    assert!(
        !t.start(A, StartKind::Reload, false, BLOCKED_LOCAL),
        "a reload is not the designated open"
    );
    assert!(t.start(
        "http://127.0.0.1:8000/",
        StartKind::Other,
        false,
        BLOCKED_LOCAL
    ));
    t.end();
    assert!(
        !t.start(
            "http://127.0.0.1:8000/",
            StartKind::Other,
            false,
            BLOCKED_LOCAL
        ),
        "the designation was consumed"
    );
}

#[test]
fn a_designation_for_another_url_does_not_open_a_page_navigation() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Url(B.into()));
    assert!(!t.start(HOP, StartKind::Other, false, BLOCKED_LOCAL));
    assert!(
        t.start(B, StartKind::Other, false, BLOCKED_LOCAL),
        "still waiting for its own start"
    );
}

#[test]
fn reload_and_history_designations_match_their_kind() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Reload);
    assert!(!t.start(B, StartKind::History, false, BLOCKED_REMOTE));
    assert!(t.start(B, StartKind::Reload, false, BLOCKED_REMOTE));
    t.designate(Designation::History);
    assert!(t.start(B, StartKind::History, false, BLOCKED_REMOTE));
    t.designate(Designation::Next);
    assert!(t.start(HOP, StartKind::Other, false, BLOCKED_LOCAL));
}

#[test]
fn a_click_in_a_remote_document_opens_but_its_own_navigation_does_not() {
    let mut t = TopLevelAdmission::default();
    assert!(
        !t.start(HOP, StartKind::Other, false, BLOCKED_REMOTE),
        "meta refresh"
    );
    assert!(
        t.start(HOP, StartKind::Other, true, BLOCKED_REMOTE),
        "link click"
    );
}

#[test]
fn a_closed_navigation_does_not_drop_the_open_load() {
    let mut t = TopLevelAdmission::default();
    assert!(t.start(B, StartKind::Other, false, ALLOWED_LOCAL));
    assert!(!t.start(HOP, StartKind::Other, false, BLOCKED_LOCAL));
    assert!(t.admits(B));
}

#[test]
fn a_local_target_opens_nothing_remote() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Url(A.into()));
    assert!(t.start(A, StartKind::Other, false, BLOCKED_LOCAL));
    assert!(!t.admitting(), "only http(s) targets are recorded");
}

#[test]
fn same_target_ignores_the_fragment_and_case_of_scheme_and_host() {
    assert!(same_target("http://Host:80/a#x", "http://host:80/a"));
    assert!(same_target("https://h", "https://h/"));
    assert!(same_target("https://h?q=1", "https://h/?q=1"));
    assert!(
        !same_target("https://h/A", "https://h/a"),
        "the path keeps its case"
    );
    assert!(!same_target("https://h/a", "https://h/b"));
}

#[test]
fn is_remote_url_reads_only_the_http_schemes() {
    assert!(is_remote_url("HTTPS://x"));
    assert!(is_remote_url("http://x"));
    assert!(!is_remote_url("file:///x"));
    assert!(!is_remote_url("about:blank"));
    assert!(!is_remote_url("http"));
}

#[test]
fn same_target_follows_whatwg_normalization() {
    assert!(
        same_target("http://h:80/a", "http://h/a"),
        "default http port"
    );
    assert!(
        same_target("https://h:443/a", "https://h/a"),
        "default https port"
    );
    assert!(
        !same_target("http://h:8080/a", "http://h/a"),
        "other ports stay"
    );
    assert!(
        same_target("http://h/a/../b/./c", "http://h/b/c"),
        "dot path"
    );
    assert!(
        same_target("http://bücher.example/", "http://xn--bcher-kva.example/"),
        "IDN host"
    );
    assert!(
        same_target(
            "http://h/한글?q=값",
            "http://h/%ED%95%9C%EA%B8%80?q=%EA%B0%92"
        ),
        "non-ASCII path and query"
    );
}

#[test]
fn a_designated_url_matches_the_engine_form_of_the_same_address() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Url(
        "http://Bücher.example:80/x/../y#top".into(),
    ));
    assert!(t.start(
        "http://xn--bcher-kva.example/y",
        StartKind::Other,
        false,
        BLOCKED_LOCAL
    ));
}

#[test]
fn a_subframe_navigation_to_the_admitted_url_gets_no_document_exception() {
    let mut t = TopLevelAdmission::default();
    let mut frames = SubframeNavigations::default();
    t.designate(Designation::Next);
    assert!(t.start(B, StartKind::Other, false, BLOCKED_LOCAL));
    assert!(t.admits_document(B, &frames), "the main request");
    frames.hop(Some(7), HOP);
    assert!(t.admits_document(B, &frames), "a frame going elsewhere");
    frames.hop(Some(7), "http://127.0.0.1:8000/b.html#frag");
    assert!(
        !t.admits_document(B, &frames),
        "the same URL from a subframe is refused"
    );
    frames.end(7);
    assert!(t.admits_document(B, &frames));
    frames.hop(Some(8), B);
    frames.clear();
    assert!(
        t.admits_document(B, &frames),
        "frames of the old document are gone"
    );
}

#[test]
fn a_withdrawn_designation_does_not_open_a_later_page_navigation() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Next);
    t.withdraw(&Designation::Next);
    assert!(
        !t.start(B, StartKind::Other, false, BLOCKED_REMOTE),
        "meta refresh after a failed Navigate"
    );
}

#[test]
fn withdrawing_leaves_a_newer_designation() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Next);
    t.designate(Designation::Reload);
    t.withdraw(&Designation::Next);
    assert!(t.start(B, StartKind::Reload, false, BLOCKED_REMOTE));
}

#[test]
fn a_history_change_without_a_start_drops_a_next_designation() {
    let mut t = TopLevelAdmission::default();
    t.designate(Designation::Next);
    t.drop_next();
    assert!(
        !t.start(B, StartKind::Other, false, BLOCKED_REMOTE),
        "the page's later automatic navigation stays closed"
    );
    t.designate(Designation::History);
    t.drop_next();
    assert!(
        t.start(B, StartKind::History, false, BLOCKED_REMOTE),
        "only Next waits for any start"
    );
}

#[test]
fn unidentified_subframe_navigations_keep_every_address_until_commit() {
    let mut t = TopLevelAdmission::default();
    let mut frames = SubframeNavigations::default();
    t.designate(Designation::Next);
    assert!(t.start(B, StartKind::Other, false, BLOCKED_LOCAL));
    frames.hop(None, B);
    frames.hop(None, HOP);
    assert!(
        !t.admits_document(B, &frames),
        "a later unidentified navigation does not replace the first address"
    );
    assert!(frames.claims(HOP));
    frames.end(u64::MAX);
    assert!(!t.admits_document(B, &frames), "no id can end it");
    frames.clear();
    assert!(t.admits_document(B, &frames), "main commit empties it");
}
