use super::*;
use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};

/// 플러그인 프로세스 없이 html kind를 등록하고 html 탭을 연다.
pub(in crate::adapters::ipc::handler::surface) fn state_with_html_tab() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let model = crate::state::tests::test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "workspace".into(),
            category: 0,
            index: 0,
            pane: 1,
        },
        E::TabCreated {
            id: 1,
            pane: 1,
            index: 0,
            name: "html".into(),
            surface: SurfaceSpec {
                id: 1,
                kind: "html".into(),
                data: None,
            },
        },
        E::TabCreated {
            id: 2,
            pane: 1,
            index: 1,
            name: "markdown".into(),
            surface: SurfaceSpec {
                id: 2,
                kind: "markdown".into(),
                data: None,
            },
        },
    ]);
    let (state, mut owner) = crate::state::tests::test_state_from_model(model);
    for (sid, kind) in [(1, "html"), (2, "markdown")] {
        owner.runtime.surfaces.insert(
            sid,
            Box::new(RemoteSurface::new(
                sid,
                kind,
                format!("com.tasty.{kind}"),
                kind.into(),
            )),
        );
    }
    (state, owner, 1)
}

fn html_script_of(
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
) -> JsonRpcResponse {
    handle_html_script(&engine.as_ref(), json!(1), &json!({ "surface_id": sid }))
}

pub(in crate::adapters::ipc::handler::surface) fn html_surface<'a>(
    engine: &'a crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
) -> &'a RemoteSurface {
    engine
        .runtime
        .surfaces
        .get(&sid)
        .and_then(|s| s.as_any().downcast_ref::<RemoteSurface>())
        .expect("html remote surface")
}

#[test]
fn reports_detection_and_a_pending_banner_without_changing_it() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| {
        st.on_host_load_requested();
        st.on_load_started();
        st.on_main_response(
            "file:///docs/a.html",
            Some(ScriptScan {
                fingerprint: Fingerprint([0xab; 32]),
                detection: ScriptDetection::Scripts,
                remote_refs: false,
                remote_scripts: false,
            }),
        );
        st.on_committed(Some("file:///docs/a.html"));
        st.on_load_finished();
        st.update_banner();
    });
    let before = rs.with_html_script(|st| st.clone());

    let resp = html_script_of(&engine, sid);
    let r = resp.result.expect("result");
    assert_eq!(r["surface_id"], sid);
    assert_eq!(r["sandbox"], true);
    assert_eq!(r["document"]["url"], "file:///docs/a.html");
    assert_eq!(r["document"]["detection"], "scripts");
    assert_eq!(r["document"]["fingerprint"], "ab".repeat(32));
    assert_eq!(r["allowed"], false);
    assert_eq!(r["javascript"], false);
    assert_eq!(r["loading"], false);
    assert_eq!(r["scanning"], false);
    assert!(r["allowance"].is_null());
    assert_eq!(r["banner"]["phase"], "hidden");
    assert_eq!(r["banner"]["pending_view"], true);
    assert_eq!(r["banner"]["viewed"], false);
    assert_eq!(
        rs.with_html_script(|st| st.clone()),
        before,
        "조회가 상태를 바꿨다"
    );
}

#[test]
fn reports_a_load_before_its_commit_and_the_loading_banner() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    let scan = |b: u8| ScriptScan {
        fingerprint: Fingerprint([b; 32]),
        detection: ScriptDetection::Scripts,
        remote_refs: false,
        remote_scripts: false,
    };
    rs.with_html_script(|st| {
        st.on_user_view();
        st.on_load_started();
        st.on_main_response("file:///docs/a.html", Some(scan(1)));
        st.on_committed(Some("file:///docs/a.html"));
        st.on_load_finished();
        st.update_banner();
        st.on_load_started();
        st.on_main_response("file:///docs/a.html", Some(scan(2)));
        st.update_banner();
    });
    let r = html_script_of(&engine, sid).result.expect("result");
    assert_eq!(r["loading"], true);
    assert_eq!(r["banner"]["phase"], "loading");
    rs.with_html_script(|st| {
        st.on_committed(Some("file:///docs/a.html"));
        st.update_banner();
    });
    let r = html_script_of(&engine, sid).result.expect("result");
    assert_eq!(r["loading"], false);
    assert_eq!(r["banner"]["phase"], "blocked");
}

#[test]
fn reports_a_committed_document_whose_scan_has_not_ended() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    let ticket = rs.with_html_script(|st| {
        st.on_user_view();
        st.on_load_started();
        let (_, ticket) = st.on_main_response_pending("file:///docs/a.html");
        st.on_committed(Some("file:///docs/a.html"));
        st.on_load_finished();
        ticket
    });
    let r = html_script_of(&engine, sid).result.expect("result");
    assert_eq!(r["loading"], false);
    assert_eq!(r["scanning"], true);
    assert!(r["document"]["fingerprint"].is_null());
    rs.with_html_script(|st| {
        st.on_scan_finished(
            ticket,
            Some(ScriptScan {
                fingerprint: Fingerprint([7; 32]),
                detection: ScriptDetection::Scripts,
                remote_refs: false,
                remote_scripts: false,
            }),
        );
        st.update_banner();
    });
    let r = html_script_of(&engine, sid).result.expect("result");
    assert_eq!(r["scanning"], false);
    assert_eq!(r["document"]["detection"], "scripts");
    assert_eq!(r["banner"]["phase"], "blocked");
}

#[test]
fn a_surface_without_a_document_reports_null_document() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let r = html_script_of(&engine, sid).result.expect("result");
    assert!(r["document"].is_null());
    assert_eq!(r["banner"]["phase"], "hidden");
}

#[test]
fn rejects_a_missing_surface_and_a_non_html_surface() {
    let (_state, mut engine_session, _sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let err = html_script_of(&engine, 999_999)
        .error
        .expect("missing surface");
    assert!(err.message.contains("not found"), "{}", err.message);

    let md_sid = 2;
    let err = html_script_of(&engine, md_sid)
        .error
        .expect("markdown surface");
    assert!(
        err.message.contains("not an html surface"),
        "{}",
        err.message
    );
}

fn html_nav_of(engine: &crate::runtime::engine_access::EngineMut<'_>, sid: u32) -> JsonRpcResponse {
    handle_html_nav(&engine.as_ref(), json!(1), &json!({ "surface_id": sid }))
}

#[test]
fn html_nav_reports_the_mirrored_address_and_history() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.set_webview_url(Some("file:///docs/a.html".into()), None, true);
    rs.set_nav_state(NavState::Done);
    assert!(rs.set_webview_nav(tasty_model::WebViewNavInfo {
        url: Some("file:///docs/b.html#top".into()),
        can_go_back: true,
        can_go_forward: false,
    }));

    let r = html_nav_of(&engine, sid).result.expect("result");
    assert_eq!(r["surface_id"], sid);
    assert_eq!(r["url"], "file:///docs/b.html#top");
    assert_eq!(r["requested_url"], "file:///docs/a.html");
    assert_eq!(r["nav_state"], "done");
    assert_eq!(r["can_go_back"], true);
    assert_eq!(r["can_go_forward"], false);
}

#[test]
fn html_nav_of_an_empty_tab_has_no_address_and_no_history() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let r = html_nav_of(&engine, sid).result.expect("result");
    assert!(r["url"].is_null());
    assert!(r["requested_url"].is_null());
    assert_eq!(r["nav_state"], "idle");
    assert_eq!(r["can_go_back"], false);
    assert_eq!(r["can_go_forward"], false);
}

#[test]
fn html_nav_rejects_a_missing_surface_and_a_non_html_surface() {
    let (_state, mut engine_session, _sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let err = html_nav_of(&engine, 999_999)
        .error
        .expect("missing surface");
    assert!(err.message.contains("not found"), "{}", err.message);
    let err = html_nav_of(&engine, 2).error.expect("markdown surface");
    assert!(
        err.message.contains("not an html surface"),
        "{}",
        err.message
    );
}

#[test]
fn an_unchanged_nav_info_does_not_report_a_change() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    let info = tasty_model::WebViewNavInfo {
        url: Some("https://example.com/".into()),
        can_go_back: false,
        can_go_forward: true,
    };
    assert!(rs.set_webview_nav(info.clone()));
    assert!(!rs.set_webview_nav(info));
}
