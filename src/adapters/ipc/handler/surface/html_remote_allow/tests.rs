use super::*;
use crate::adapters::ipc::handler::surface::html_script::tests::{
    html_surface, state_with_html_tab,
};
use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};

const URL: &str = "file:///docs/a.html";

fn scan(byte: u8) -> Option<ScriptScan> {
    Some(ScriptScan {
        fingerprint: Fingerprint([byte; 32]),
        detection: ScriptDetection::None,
        remote_refs: true,
        remote_scripts: false,
    })
}

fn commit(st: &mut HtmlScriptState, url: &str, scan: Option<ScriptScan>) {
    st.on_host_load_requested();
    st.on_load_started();
    st.on_main_response(url, scan);
    st.on_committed(Some(url));
    st.on_load_finished();
    st.update_banner();
}

/// 요청 하나를 처리하고 포커스·활성 workspace가 그대로인지 확인한다.
fn allow(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    params: Value,
) -> JsonRpcResponse {
    let focus_before = state.focused_surface_id(engine);
    let active_before = state.active_workspace(engine).id;
    let resp = handle_html_remote_allow(&engine.as_ref(), json!(1), &params);
    assert_eq!(state.focused_surface_id(engine), focus_before);
    assert_eq!(state.active_workspace(engine).id, active_before);
    resp
}

fn reason(resp: &JsonRpcResponse) -> String {
    let err = resp.error.as_ref().expect("error");
    assert_eq!(err.code, HTML_SCRIPT_ALLOW_REFUSED, "{}", err.message);
    err.data.as_ref().expect("data")["reason"]
        .as_str()
        .expect("reason")
        .to_string()
}

#[test]
fn allows_remote_content_like_the_notice_button_and_leaves_scripts_alone() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, URL, scan(7)));
    let (script_banner, remote_banner) =
        rs.with_html_script(|st| (st.banner(), st.remote_banner()));

    let r = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": URL, "fingerprint": "07".repeat(32) }),
    )
    .result
    .expect("result");
    assert_eq!(r["result"], "allowed");
    assert_eq!(r["reload"], true);

    let mut by_button = HtmlScriptState::new(true);
    commit(&mut by_button, URL, scan(7));
    by_button.allow_remote_current().expect("button allow");
    rs.with_html_script(|st| {
        assert_eq!(st.remote_allowance(), by_button.remote_allowance());
        assert!(st.allowance().is_none(), "scripts are a separate allowance");
        assert_eq!(st.banner(), script_banner);
        assert_eq!(st.remote_banner(), remote_banner);
        assert!(st.take_reload_request());
    });

    let r = allow(&mut state, &engine, json!({ "surface_id": sid }))
        .result
        .expect("result");
    assert_eq!(r["result"], "already_allowed");
    assert_eq!(r["reload"], false);
    assert!(!rs.with_html_script(|st| st.take_reload_request()));
}

#[test]
fn refusals_name_their_reason_and_change_nothing() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);

    assert_eq!(
        reason(&allow(&mut state, &engine, json!({ "surface_id": sid }))),
        "no_document"
    );
    // 응답 단계 없이 commit된 로컬 문서. http(s) 페이지는 URL로 허용한다(아래 시험).
    rs.with_html_script(|st| commit(st, URL, None));
    assert_eq!(
        reason(&allow(&mut state, &engine, json!({ "surface_id": sid }))),
        "no_fingerprint"
    );
    rs.with_html_script(|st| commit(st, URL, scan(1)));
    assert_eq!(
        reason(&allow(
            &mut state,
            &engine,
            json!({ "surface_id": sid, "fingerprint": "02".repeat(32) })
        )),
        "document_changed"
    );
    rs.with_html_script(|st| {
        st.set_remote_global(true);
    });
    let before = rs.with_html_script(|st| st.clone());
    assert_eq!(
        reason(&allow(&mut state, &engine, json!({ "surface_id": sid }))),
        "globally_allowed"
    );
    assert_eq!(rs.with_html_script(|st| st.clone()), before);
    rs.with_html_script(|st| {
        st.set_remote_global(false);
        st.on_load_started();
        st.on_main_response(URL, scan(1));
    });
    assert_eq!(
        reason(&allow(&mut state, &engine, json!({ "surface_id": sid }))),
        "loading"
    );
    rs.with_html_script(|st| {
        st.on_committed(Some(URL));
        st.on_load_finished();
        st.set_remote_view(tasty_model::html_script::RemoteView::Mirror);
    });
    assert_eq!(
        reason(&allow(&mut state, &engine, json!({ "surface_id": sid }))),
        "unsupported_view"
    );
    assert!(rs.with_html_script(|st| st.remote_allowance().is_none()));
}

#[test]
fn the_query_reports_the_remote_state_without_showing_the_notice() {
    let (_state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, URL, scan(5)));
    let before = rs.with_html_script(|st| st.clone());
    let resp = crate::adapters::ipc::handler::surface::handle_html_script(
        &engine.as_ref(),
        json!(1),
        &json!({ "surface_id": sid }),
    );
    let r = resp.result.expect("result");
    let remote = &r["remote"];
    assert_eq!(remote["global"], false);
    assert_eq!(remote["supported"], true);
    assert_eq!(remote["refs"], true);
    assert_eq!(remote["allowed"], false);
    assert_eq!(remote["effective"], false);
    assert!(remote["banner"]["phase"].is_string());
    assert_eq!(rs.with_html_script(|st| st.clone()), before);
}

/// 지문 없는 `http(s)` 페이지는 URL로만 확인한다. 지문을 주면 그 페이지가 아닌 문서를 가리킨 것이다.
#[test]
fn an_http_page_is_allowed_by_url_only() {
    const PAGE: &str = "https://example.com/guide";
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, PAGE, None));

    for params in [
        json!({ "surface_id": sid, "url": "https://example.com/other" }),
        json!({ "surface_id": sid, "url": PAGE, "fingerprint": "01".repeat(32) }),
    ] {
        assert_eq!(
            reason(&allow(&mut state, &engine, params)),
            "document_changed"
        );
    }
    assert!(rs.with_html_script(|st| st.remote_allowance().is_none()));

    let r = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": PAGE }),
    )
    .result
    .expect("allowed");
    assert_eq!(r["result"], "allowed");
    assert_eq!(r["document"]["fingerprint"], Value::Null);

    let q = crate::adapters::ipc::handler::surface::handle_html_script(
        &engine.as_ref(),
        json!(1),
        &json!({ "surface_id": sid }),
    )
    .result
    .expect("result");
    assert_eq!(q["web_page"], true);
    assert_eq!(q["remote"]["allowance"]["url"], PAGE);
    assert_eq!(q["remote"]["allowance"]["fingerprint"], Value::Null);
    assert!(q["allowance"].is_null(), "scripts are a separate allowance");
}
