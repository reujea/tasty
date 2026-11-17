use super::*;
use crate::adapters::ipc::handler::surface::html_script::tests::{
    html_surface, state_with_html_tab,
};

/// 요청 하나를 처리하고 창에 적용한 뒤 응답과 에이전트 히스토리 큐를 돌려준다.
fn step_on(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
    step: WebViewHistoryStep,
) -> (JsonRpcResponse, Vec<(u32, WebViewHistoryStep)>) {
    let focus_before = state.focused_surface_id(engine);
    let mut scope = crate::ipc::request_scope::RequestScope::capture(state, engine, None);
    let resp = handle_html_history(
        &mut scope,
        &engine.as_ref(),
        json!(1),
        &json!({ "surface_id": sid }),
        step,
    );
    scope.finish().apply(state, engine);
    assert_eq!(
        state.focused_surface_id(engine),
        focus_before,
        "포커스가 바뀌었다"
    );
    assert!(state.webview_user_nav.is_empty(), "사용자 탐색 큐에 넣었다");
    (resp, std::mem::take(&mut state.webview_agent_nav))
}

#[test]
fn queues_each_step_for_that_surface() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    for step in [
        WebViewHistoryStep::Back,
        WebViewHistoryStep::Forward,
        WebViewHistoryStep::Reload,
    ] {
        let (resp, queue) = step_on(&mut state, &engine, sid, step);
        let r = resp.result.expect("result");
        assert_eq!(r["surface_id"], sid);
        assert_eq!(r["queued"], true, "{step:?}");
        assert_eq!(queue, vec![(sid, step)]);
    }
}

/// 관측 사본은 redraw 때만 갱신되어 이동 직후 늦을 수 있다(Linux 실제 실행에서 측정).
/// 사본이 갈 곳 없다고 해도 큐에 넣고, 판정은 적용 시점의 backend 값에 맡긴다.
#[test]
fn a_stale_history_copy_does_not_refuse_the_step() {
    let (mut state, mut engine_session, sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    assert!(
        html_surface(&engine, sid).set_webview_nav(tasty_model::WebViewNavInfo {
            url: Some("file:///docs/b.html".into()),
            can_go_back: false,
            can_go_forward: false,
        })
    );
    let (resp, queue) = step_on(&mut state, &engine, sid, WebViewHistoryStep::Forward);
    assert_eq!(resp.result.expect("result")["queued"], true);
    assert_eq!(queue, vec![(sid, WebViewHistoryStep::Forward)]);
}

#[test]
fn rejects_a_missing_surface_a_non_html_surface_and_no_target() {
    let (mut state, mut engine_session, _sid) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let (resp, queue) = step_on(&mut state, &engine, 999_999, WebViewHistoryStep::Back);
    let err = resp.error.expect("missing surface");
    assert!(err.message.contains("not found"), "{}", err.message);
    assert!(queue.is_empty());

    let (resp, queue) = step_on(&mut state, &engine, 2, WebViewHistoryStep::Reload);
    let err = resp.error.expect("markdown surface");
    assert!(
        err.message.contains("not an html surface"),
        "{}",
        err.message
    );
    assert!(queue.is_empty());

    let mut scope = crate::ipc::request_scope::RequestScope::capture(&mut state, &engine, None);
    let resp = handle_html_history(
        &mut scope,
        &engine.as_ref(),
        json!(1),
        &json!({}),
        WebViewHistoryStep::Back,
    );
    scope.finish().apply(&mut state, &engine);
    assert!(resp.error.is_some(), "surface_id 없이 성공했다");
    assert!(state.webview_agent_nav.is_empty());
}

/// mirror workspace의 html leaf는 콘텐츠 mirror가 없어 client에서 빈 surface다.
/// 원격으로 전달하지 않고 `surface.html_nav`처럼 html이 아니라고 거절한다.
#[test]
fn rejects_a_mirror_placeholder_without_forwarding() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let sid = *state
        .active_workspace(&engine)
        .all_surface_ids()
        .first()
        .expect("fixture surface");
    assert!(engine.is_mirror_surface(sid));
    engine
        .runtime
        .surfaces
        .insert(sid, Box::new(crate::model::EmptySurface::new(sid)));
    assert_eq!(
        engine.find_surface_by_id(sid).map(|s| s.kind()),
        Some("empty")
    );
    let intents_before = state.pending_intents.len();
    let (resp, queue) = step_on(&mut state, &engine, sid, WebViewHistoryStep::Back);
    let err = resp.error.expect("mirror placeholder");
    assert!(
        err.message.contains("not an html surface"),
        "{}",
        err.message
    );
    assert!(queue.is_empty());
    assert_eq!(
        state.pending_intents.len(),
        intents_before,
        "원격 전달 intent가 생겼다"
    );
}

/// 문서 주소·탐색 IPC는 `address_band`를, 스크립트 IPC는 `script_gate`를 선언한 kind를 대상으로 한다.
#[test]
fn navigation_follows_the_address_band_and_scripts_follow_the_gate() {
    use crate::adapters::ipc::handler::surface::{handle_html_nav, handle_html_script};
    use crate::plugin_bridge::remote_surface::RemoteSurface;
    use crate::runtime::surface_registry::KindCapabilities;
    let (mut state, mut engine_session, _) = state_with_html_tab();
    let engine = engine_session.borrow_mut();
    let surface = RemoteSurface::new(5, "my-web", "com.example.web".into(), "web".into())
        .with_capabilities(KindCapabilities {
            address_band: true,
            ..KindCapabilities::default()
        });
    engine.runtime.surfaces.insert(5, Box::new(surface));
    let params = json!({ "surface_id": 5 });
    assert!(
        handle_html_nav(&engine.as_ref(), json!(1), &params)
            .error
            .is_none(),
        "html_nav"
    );
    assert!(
        handle_html_script(&engine.as_ref(), json!(1), &params)
            .error
            .is_some(),
        "html_script"
    );
    let (resp, queue) = step_on(&mut state, &engine, 5, WebViewHistoryStep::Back);
    assert!(resp.error.is_none(), "html_back");
    assert_eq!(queue, vec![(5, WebViewHistoryStep::Back)]);
}
