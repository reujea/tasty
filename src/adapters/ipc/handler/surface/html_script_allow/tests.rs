use super::*;
use crate::adapters::ipc::handler::surface::html_script::tests::{
    html_surface, state_with_html_tab,
};
use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};

const URL: &str = "file:///docs/a.html";

fn scan(byte: u8) -> Option<ScriptScan> {
    Some(ScriptScan {
        fingerprint: Fingerprint([byte; 32]),
        detection: ScriptDetection::Scripts,
        remote_refs: false,
        remote_scripts: false,
    })
}

/// 사용자가 아직 보지 않은 차단 문서를 commit한다(에이전트가 연 문서와 같은 상태).
fn commit(st: &mut HtmlScriptState, url: &str, scan: Option<ScriptScan>) {
    st.on_host_load_requested();
    st.on_load_started();
    st.on_main_response(url, scan);
    st.on_committed(Some(url));
    st.on_load_finished();
    st.update_banner();
}

/// 요청 하나를 처리하고 포커스·선택이 그대로인지 확인한다.
fn allow(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    params: Value,
) -> JsonRpcResponse {
    let focus_before = state.focused_surface_id(engine);
    let active_before = state.active_workspace(engine).id;
    let resp = handle_html_script_allow(&engine.as_ref(), json!(1), &params);
    assert_eq!(
        state.focused_surface_id(engine),
        focus_before,
        "포커스가 바뀌었다"
    );
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
fn allows_the_queried_document_like_the_banner_button() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, URL, scan(7)));
    let banner_before = rs.with_html_script(|st| st.banner());

    let resp = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": URL, "fingerprint": "07".repeat(32) }),
    );
    let r = resp.result.expect("result");
    assert_eq!(r["result"], "allowed");
    assert_eq!(r["reload"], true);
    assert_eq!(r["document"]["url"], URL);

    // 배너 버튼의 허용과 같은 상태 변화: 기록·JS 값·재로드 요청이 한 번.
    let mut expected = rs.with_html_script(|st| st.clone());
    rs.with_html_script(|st| {
        assert!(st.current_is_allowed());
        assert!(st.take_reload_request());
        assert!(!st.take_reload_request());
    });
    let mut by_button = HtmlScriptState::new(true);
    commit(&mut by_button, URL, scan(7));
    by_button.allow_current().expect("button allow");
    assert_eq!(by_button.allowance(), expected.allowance());
    assert_eq!(by_button.effective_js(), expected.effective_js());
    assert!(expected.take_reload_request());
    assert_eq!(rs.with_html_script(|st| st.banner()), banner_before);
}

#[test]
fn a_second_call_reports_already_allowed_without_reloading() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, URL, scan(7)));
    allow(&mut state, &engine, json!({ "surface_id": sid }));
    rs.with_html_script(|st| st.take_reload_request());
    let r = allow(&mut state, &engine, json!({ "surface_id": sid }))
        .result
        .expect("result");
    assert_eq!(r["result"], "already_allowed");
    assert_eq!(r["reload"], false);
    assert!(!rs.with_html_script(|st| st.take_reload_request()));
}

#[test]
fn a_document_replaced_after_the_query_is_refused() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, URL, scan(1)));
    let queried = "01".repeat(32);
    rs.with_html_script(|st| commit(st, URL, scan(2)));
    let before = rs.with_html_script(|st| st.clone());

    let resp = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": URL, "fingerprint": queried }),
    );
    assert_eq!(reason(&resp), "document_changed");
    let resp = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": "file:///docs/b.html" }),
    );
    assert_eq!(reason(&resp), "document_changed");
    assert_eq!(
        rs.with_html_script(|st| st.clone()),
        before,
        "상태가 바뀌었다"
    );
}

#[test]
fn loading_no_document_and_no_fingerprint_are_told_apart() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);

    let resp = allow(&mut state, &engine, json!({ "surface_id": sid }));
    assert_eq!(reason(&resp), "no_document");

    // 응답 단계 없이 commit된 로컬 문서. http(s) 페이지는 URL로 허용한다(아래 시험).
    rs.with_html_script(|st| commit(st, URL, None));
    let resp = allow(&mut state, &engine, json!({ "surface_id": sid }));
    assert_eq!(reason(&resp), "no_fingerprint");

    rs.with_html_script(|st| {
        commit(st, URL, scan(3));
        st.on_load_started();
        st.on_main_response(URL, scan(4));
    });
    let before = rs.with_html_script(|st| st.clone());
    let resp = allow(&mut state, &engine, json!({ "surface_id": sid }));
    assert_eq!(reason(&resp), "loading");
    assert_eq!(
        rs.with_html_script(|st| st.clone()),
        before,
        "상태가 바뀌었다"
    );
}

#[test]
fn rejects_a_non_html_surface_a_missing_surface_and_bad_arguments() {
    let (mut state, mut engine_session, _sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let err = allow(&mut state, &engine, json!({ "surface_id": 2 }))
        .error
        .expect("markdown surface");
    assert_eq!(err.code, -32602);
    assert!(
        err.message.contains("not an html surface"),
        "{}",
        err.message
    );
    let err = allow(&mut state, &engine, json!({ "surface_id": 999_999 }))
        .error
        .expect("missing surface");
    assert!(err.message.contains("not found"), "{}", err.message);
    let err = allow(&mut state, &engine, json!({ "surface_id": 1, "url": 3 }))
        .error
        .expect("bad url");
    assert_eq!(err.code, -32602);
}

/// `http(s)` 페이지의 스크립트도 URL로만 확인하고 지문 없이 기록한다.
#[test]
fn an_http_page_takes_a_script_allowance_by_url() {
    const PAGE: &str = "https://example.com/guide";
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let rs = html_surface(&engine, sid);
    rs.with_html_script(|st| commit(st, PAGE, None));
    let resp = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": PAGE, "fingerprint": "01".repeat(32) }),
    );
    assert_eq!(reason(&resp), "document_changed");
    let r = allow(
        &mut state,
        &engine,
        json!({ "surface_id": sid, "url": PAGE }),
    )
    .result
    .expect("allowed");
    assert_eq!(r["result"], "allowed");
    let q = crate::adapters::ipc::handler::surface::handle_html_script(
        &engine.as_ref(),
        json!(1),
        &json!({ "surface_id": sid }),
    )
    .result
    .expect("result");
    assert_eq!(q["allowance"]["url"], PAGE);
    assert_eq!(q["allowance"]["fingerprint"], Value::Null);
}
