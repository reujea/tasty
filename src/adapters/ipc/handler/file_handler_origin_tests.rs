//! 요청 입구에서 판정한 출처가 파일 식별과 새 탭 생성 요청까지 유지되는지 확인한다.
//! origin을 시험에서 직접 지정하면 입구의 잘못된 사용자 판정을 잡을 수 없다.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::handle_dispatch;
use crate::app::command::DomainIntent;
use crate::file::dispatch::FileDispatchOrigin;
use crate::file::format::DetectorId;
use crate::intent::Intent;

const PLUGIN: &str = "com.example.popup";
const POPUP: u64 = 7;

fn source_fixture() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
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
            name: "source".into(),
            surface: SurfaceSpec {
                id: 1,
                kind: "markdown".into(),
                data: None,
            },
        },
    ]))
}

fn plugin_caller(plugin_id: &str) -> CallerContext {
    CallerContext::Plugin {
        plugin_id: plugin_id.to_string(),
        permissions: Arc::new(HashSet::new()),
    }
}

/// dispatch 요청을 파일 식별 완료와 새 탭 생성 요청까지 적용한다.
/// activated는 확정 입력을 받은 팝업, with_origin_surface는 명시적 대상 pane, mirror는 원격 전달을 준비한다.
fn dispatch_through(
    caller: &CallerContext,
    activated: Option<(&str, u64)>,
    params: serde_json::Value,
    with_origin_surface: bool,
    mirror: bool,
) -> DispatchOutcome {
    dispatch_through_with(caller, activated, &[], params, with_origin_surface, mirror)
}

type DispatchOutcome = (
    FileDispatchOrigin,
    bool,
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
);

/// webview의 navigation 기록도 준비한다. 제스처 여부와 페이지 소유자는 Attempt로 지정한다.
fn dispatch_through_with(
    caller: &CallerContext,
    activated: Option<(&str, u64)>,
    navigated: &[Attempt],
    mut params: serde_json::Value,
    with_origin_surface: bool,
    mirror: bool,
) -> DispatchOutcome {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core();
    let (mut state, mut engine_session) = if mirror {
        crate::state::tests::test_mirror_state()
    } else {
        source_fixture()
    };
    let mut engine = engine_session.borrow_mut();
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
        PLUGIN,
        &[json!({"id":"open", "detector":"origin-test", "priority":0,
            "action":{"kind":"open_surface", "surface_kind":"empty", "param_key":"file"}})],
    );
    if let Some((plugin, instance)) = activated {
        state
            .plugin_popup_user_activated
            .insert(instance, plugin.to_string());
    }
    let pane_id = state.focused_pane_id(&engine);
    assert_eq!(
        state
            .navigation
            .tab_index(engine.find_pane_by_id(pane_id).expect("pane")),
        0
    );
    let view = Arc::new(());
    state.webview_identity = Arc::downgrade(&view);
    let proofs = Arc::new(crate::app::html_runtime::NavigationProofs::default());
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    install_source(&mut engine, sid);
    for attempt in navigated {
        attempt.record(&proofs, &state, &engine, sid);
    }
    if with_origin_surface {
        let sid = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        assert_eq!(engine.find_pane_for_surface(sid), Some(pane_id));
        params["origin_surface_id"] = json!(sid);
    }

    let mut out = crate::ipc::window_port::IntentOutbox::default();
    let mut scope = crate::ipc::request_scope::RequestScope::capture(
        &mut state,
        engine.core,
        Some(proofs.clone()),
    );
    let resp = handle_dispatch(
        &mut out,
        &mut scope,
        &engine.as_ref(),
        caller,
        json!(1),
        params,
    );
    drop(scope);
    assert!(resp.error.is_none(), "dispatch must be accepted: {resp:?}");
    let mut emitted = out.into_vec();
    assert_eq!(emitted.len(), 1);
    let dispatched = emitted.remove(0);
    let intent_is_user = dispatched.origin.is_user();
    let Intent::Domain(DomainIntent::DispatchFile {
        target,
        origin_surface_id,
        dispatch_origin,
        ignore_size_limit,
        ..
    }) = dispatched.body
    else {
        panic!("file_handler.dispatch 가 DispatchFile 외 intent 를 냈다");
    };

    crate::file::dispatch::apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target,
        Some(DetectorId::new("origin-test")),
        origin_surface_id,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin,
            ignore_size_limit,
            run: None,
        },
    );
    // App receives creation requests; the View does not synchronously create/select a tab.
    assert_eq!(engine.find_pane_by_id(pane_id).unwrap().tabs.len(), 1);
    assert_eq!(
        state
            .navigation
            .tab_index(engine.find_pane_by_id(pane_id).unwrap()),
        0
    );
    (
        dispatch_origin,
        intent_is_user,
        state,
        engine_session,
        pane_id,
    )
}

fn dispatch_then_selection(
    caller: &CallerContext,
    activated: Option<(&str, u64)>,
    params: serde_json::Value,
    with_origin_surface: bool,
) -> (FileDispatchOrigin, bool, bool) {
    let (origin, intent_is_user, mut state, _engine_session, pane_id) =
        dispatch_through(caller, activated, params, with_origin_surface, false);
    (
        origin,
        intent_is_user,
        creation_activation(&mut state, pane_id),
    )
}

fn popup_params() -> serde_json::Value {
    json!({ "path": "/tmp/a.md", "owner_popup_instance": POPUP })
}

#[test]
fn a_dispatch_from_the_users_popup_requests_activation_of_the_new_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::User, true, true));
}

#[test]
fn a_dispatch_without_a_popup_requests_background_creation() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        json!({ "path": "/tmp/a.md" }),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

#[test]
fn an_external_caller_cannot_claim_a_popup() {
    let got = dispatch_then_selection(
        &CallerContext::local(),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::Agent, false, false));
}

#[test]
fn a_plugin_cannot_claim_another_plugins_popup() {
    let got = dispatch_then_selection(
        &plugin_caller("com.example.other"),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

#[test]
fn an_untouched_popup_is_not_a_user_action() {
    let got = dispatch_then_selection(&plugin_caller(PLUGIN), None, popup_params(), false);
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

// 대상 pane을 명시해도 사용자 여부는 팝업 입력으로 판정한다.
#[test]
fn a_users_popup_that_names_an_origin_surface_still_requests_activation_of_the_new_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        popup_params(),
        true,
    );
    assert_eq!(got, (FileDispatchOrigin::User, true, true));
}

#[test]
fn an_origin_surface_without_a_popup_requests_background_creation() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        json!({ "path": "/tmp/a.md" }),
        true,
    );
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

const NAV_URL: &str = "about:blank#tasty-nav:link:%2Ftmp%2Fa.md";

/// host 가 plugin [`PLUGIN`] 에 통지한 navigation 시도 하나.
struct Attempt {
    /// 엔진이 그 시도를 사용자 제스처로 봤는가.
    user_gesture: bool,
    /// 그 surface 의 지금 페이지를 쓴 `webview.set_url` 호출자가 [`PLUGIN`] 이었는가.
    owner_wrote_page: bool,
}

fn install_source(engine: &mut crate::runtime::engine_access::EngineMut<'_>, sid: u32) {
    engine.runtime.surfaces.insert(
        sid,
        Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
            sid,
            "markdown",
            PLUGIN.into(),
            "source".into(),
        )),
    );
}

fn creation_activation(state: &mut crate::state::RequestContext, pane_id: u32) -> bool {
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    match &intents[0].body {
        Intent::Domain(DomainIntent::CreateTab {
            pane_id: target,
            activate,
            ..
        }) => {
            assert_eq!(*target, pane_id);
            assert_eq!(*activate, intents[0].origin.is_user());
            *activate
        }
        Intent::NewTab { .. } => intents[0].origin.is_user(),
        other => panic!("expected creation request: {other:?}"),
    }
}

impl Attempt {
    fn record(
        &self,
        proofs: &crate::app::html_runtime::NavigationProofs,
        state: &crate::state::RequestContext,
        engine: &crate::runtime::engine_access::EngineMut<'_>,
        sid: u32,
    ) {
        let remote = engine
            .runtime
            .surfaces
            .get(&sid)
            .unwrap()
            .as_any()
            .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            .unwrap();
        proofs.record(
            &state.webview_identity,
            sid,
            Some(&crate::plugin_bridge::user_navigation::NavigationOwner {
                plugin_id: PLUGIN.into(),
                wrote_page: self.owner_wrote_page,
            }),
            Some(&remote.webview_url),
            &crate::webview::PendingNavigation {
                url: NAV_URL.into(),
                user_gesture: self.user_gesture,
            },
        );
    }
}

/// 소유 plugin 이 쓴 페이지 위에서 엔진이 `user_gesture` 로 본 시도.
fn gesture(user_gesture: bool) -> Attempt {
    Attempt {
        user_gesture,
        owner_wrote_page: true,
    }
}

fn link_params() -> serde_json::Value {
    json!({ "path": "/tmp/a.md", "user_navigation_url": NAV_URL })
}

fn link_then_selection(
    caller: &CallerContext,
    navigated: &[Attempt],
    params: serde_json::Value,
) -> (FileDispatchOrigin, bool, bool) {
    let (origin, intent_is_user, mut state, _owner, pane_id) =
        dispatch_through_with(caller, None, navigated, params, true, false);
    (
        origin,
        intent_is_user,
        creation_activation(&mut state, pane_id),
    )
}

#[test]
fn a_link_the_user_clicked_in_the_plugins_webview_requests_activation_of_the_new_tab() {
    let got = link_then_selection(&plugin_caller(PLUGIN), &[gesture(true)], link_params());
    assert_eq!(got, (FileDispatchOrigin::User, true, true));
}

#[test]
fn a_navigation_the_engine_did_not_see_as_a_gesture_is_not_a_user_action() {
    let got = link_then_selection(&plugin_caller(PLUGIN), &[gesture(false)], link_params());
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

/// 뒤따른 비사용자 navigation이 이전 클릭 기록을 지워야 한다.
#[test]
fn a_non_gesture_attempt_after_an_unused_click_leaves_the_dispatch_unverified() {
    let agents_script = Attempt {
        user_gesture: false,
        owner_wrote_page: false,
    };
    for later in [gesture(false), agents_script] {
        let got = link_then_selection(
            &plugin_caller(PLUGIN),
            &[gesture(true), later],
            link_params(),
        );
        assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
    }
}

/// 다른 호출자가 쓴 페이지의 클릭은 소유 플러그인의 사용자 행동으로 인정하지 않는다.
#[test]
fn a_click_on_a_page_the_owner_did_not_write_is_not_a_user_action() {
    let on_agents_page = Attempt {
        user_gesture: true,
        owner_wrote_page: false,
    };
    let got = link_then_selection(&plugin_caller(PLUGIN), &[on_agents_page], link_params());
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

#[test]
fn a_plugin_cannot_claim_a_navigation_that_never_happened() {
    let got = link_then_selection(&plugin_caller(PLUGIN), &[], link_params());
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

#[test]
fn an_external_caller_cannot_claim_a_webview_navigation() {
    let got = link_then_selection(&CallerContext::local(), &[gesture(true)], link_params());
    assert_eq!(got, (FileDispatchOrigin::Agent, false, false));
}

/// 입구 판정부터 식별 결과 적용까지 이어서 본다. 매칭 핸들러가 없을 때 fallback picker는
/// 증명하지 못한 plugin 중계 요청에만 열리고 외부 IPC 요청에는 열리지 않는다.
#[test]
fn only_an_unverified_plugin_request_opens_the_fallback_picker() {
    for (caller, opens) in [
        (plugin_caller(PLUGIN), true),
        (CallerContext::local(), false),
    ] {
        let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = source_fixture();
        let mut engine = engine_session.borrow_mut();
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let mut scope =
            crate::ipc::request_scope::RequestScope::capture(&mut state, engine.core, None);
        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &caller,
            json!(1),
            json!({ "path": "/tmp/unmatched.unknown" }),
        );
        drop(scope);
        assert!(resp.error.is_none(), "dispatch must be accepted: {resp:?}");
        let Intent::Domain(DomainIntent::DispatchFile {
            target,
            origin_surface_id,
            dispatch_origin,
            ignore_size_limit,
            ..
        }) = out.into_vec().remove(0).body
        else {
            panic!("file_handler.dispatch 가 DispatchFile 외 intent 를 냈다");
        };
        crate::file::dispatch::apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            target,
            None,
            origin_surface_id,
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin,
                ignore_size_limit,
                run: None,
            },
        );
        assert_eq!(
            state.dialogs.file_handler_picker.is_some(),
            opens,
            "{caller:?}"
        );
    }
}

/// 세션 에이전트도 외부 IPC라 팝업이나 navigation 기록이 있어도 picker를 띄울 수 없는 Agent다.
#[test]
fn a_session_agent_caller_stays_an_agent_request() {
    let agent = CallerContext::Agent {
        agent_id: "child:1".to_string(),
        permissions: Arc::new(HashSet::new()),
    };
    let got = dispatch_then_selection(&agent, Some((PLUGIN, POPUP)), popup_params(), false);
    assert_eq!(got, (FileDispatchOrigin::Agent, false, false));
    let got = link_then_selection(&agent, &[gesture(true)], link_params());
    assert_eq!(got, (FileDispatchOrigin::Agent, false, false));
}

#[test]
fn a_plugin_cannot_claim_another_plugins_webview_navigation() {
    let got = link_then_selection(
        &plugin_caller("com.example.other"),
        &[gesture(true)],
        link_params(),
    );
    assert_eq!(got, (FileDispatchOrigin::PluginUnverified, false, false));
}

/// 같은 프레임에 페이지 작성자가 바뀌면 navigation 기록을 버린다. 다음 프레임의 클릭은 허용한다.
#[test]
fn a_click_drained_in_the_frame_the_owner_took_the_page_back_is_not_a_user_action() {
    let (mut state, mut engine_session) = source_fixture();
    let mut engine = engine_session.borrow_mut();
    let view = Arc::new(());
    state.webview_identity = Arc::downgrade(&view);
    let proofs = Arc::new(crate::app::html_runtime::NavigationProofs::default());
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    install_source(&mut engine, sid);
    let mut params = link_params();
    params["origin_surface_id"] = json!(sid);
    let mut origins = Vec::new();
    for (id, owner_took_over) in [(0, true), (1, false)] {
        gesture(true).record(&proofs, &state, &engine, sid);
        proofs.settle_frame(&state.webview_identity, &[sid], &[(sid, owner_took_over)]);
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let mut scope = crate::ipc::request_scope::RequestScope::capture(
            &mut state,
            engine.core,
            Some(proofs.clone()),
        );
        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &plugin_caller(PLUGIN),
            json!(id),
            params.clone(),
        );
        assert!(resp.error.is_none(), "dispatch must be accepted: {resp:?}");
        let emitted = out.into_vec();
        assert_eq!(emitted.len(), 1);
        origins.push(emitted[0].origin.is_user());
    }
    assert_eq!(origins, vec![false, true]);
}

#[test]
fn a_webview_navigation_backs_only_one_dispatch() {
    let (mut state, mut engine_session) = source_fixture();
    let mut engine = engine_session.borrow_mut();
    let view = Arc::new(());
    state.webview_identity = Arc::downgrade(&view);
    let proofs = Arc::new(crate::app::html_runtime::NavigationProofs::default());
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    install_source(&mut engine, sid);
    gesture(true).record(&proofs, &state, &engine, sid);
    let mut params = link_params();
    params["origin_surface_id"] = json!(sid);
    let mut origins = Vec::new();
    for id in 0..2 {
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let mut scope = crate::ipc::request_scope::RequestScope::capture(
            &mut state,
            engine.core,
            Some(proofs.clone()),
        );
        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &plugin_caller(PLUGIN),
            json!(id),
            params.clone(),
        );
        assert!(resp.error.is_none(), "dispatch must be accepted: {resp:?}");
        let emitted = out.into_vec();
        assert_eq!(emitted.len(), 1);
        origins.push(emitted[0].origin.is_user());
    }
    assert_eq!(origins, vec![true, false]);
}

/// mirror origin은 이름으로만 식별하지만 응답 depth는 요청값을 그대로 돌려준다. 응답 필드의 의미는 바꾸지 않는다.
#[test]
fn a_mirror_origin_dispatch_echoes_the_requested_depth() {
    for (mirror, expected) in [(false, "deep"), (true, "deep")] {
        let (mut state, mut engine_session) = if mirror {
            crate::state::tests::test_mirror_state()
        } else {
            source_fixture()
        };
        let engine = engine_session.borrow_mut();
        let sid = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let mut out = crate::ipc::window_port::IntentOutbox::default();
        let mut scope =
            crate::ipc::request_scope::RequestScope::capture(&mut state, engine.core, None);
        let resp = handle_dispatch(
            &mut out,
            &mut scope,
            &engine.as_ref(),
            &CallerContext::local(),
            json!(1),
            json!({"path": "/remote/doc.md", "depth": "deep", "origin_surface_id": sid}),
        );
        let result = resp.result.expect("dispatch accepted");
        assert_eq!(result["depth"], expected, "mirror={mirror}");
    }
}

/// 플러그인 페이지의 사용자 navigation 으로 연 피커는 사용자 요청이라 포커스를 받고 WebView 를 숨긴다.
/// 확인하지 못한 요청과 한 번 쓴 증명은 passive 피커가 된다(docs/design/systems/popup.md#passive-팝업).
#[test]
fn a_picker_opened_from_a_user_navigation_on_the_plugin_page_is_a_user_popup() {
    let opened_by_user = |navigated: Option<Attempt>, reuse: bool| -> bool {
        let (mut state, mut engine_session) = source_fixture();
        let mut engine = engine_session.borrow_mut();
        let view = Arc::new(());
        state.webview_identity = Arc::downgrade(&view);
        let proofs = crate::app::html_runtime::NavigationProofs::default();
        let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
        install_source(&mut engine, sid);
        if let Some(attempt) = navigated {
            attempt.record(&proofs, &state, &engine, sid);
        }
        let caller = CallerContext::Plugin {
            plugin_id: PLUGIN.into(),
            permissions: Arc::new(HashSet::new()),
        };
        let params = json!({ "origin_surface_id": sid, "user_navigation_url": NAV_URL });
        if reuse {
            assert!(proofs.take(
                &state.webview_identity,
                &engine.as_ref(),
                PLUGIN,
                sid,
                NAV_URL
            ));
        }
        let resp = crate::adapters::ipc::handler::file_picker::handle_trigger_with_proofs(
            &mut state,
            &mut engine,
            &caller,
            json!(1),
            &params,
            Some(&proofs),
        );
        assert!(resp.error.is_none(), "picker must open: {resp:?}");
        let opens: Vec<bool> = state
            .take_pending_intents()
            .into_iter()
            .filter(|i| {
                matches!(
                    i.body,
                    Intent::Ui(crate::intent::UiIntent::OpenPopup { .. })
                )
            })
            .map(|i| i.origin.is_user())
            .collect();
        assert_eq!(opens.len(), 1);
        opens[0]
    };
    assert!(opened_by_user(Some(gesture(true)), false));
    assert!(!opened_by_user(Some(gesture(true)), true), "한 번 쓴 증명");
    assert!(
        !opened_by_user(Some(gesture(false)), false),
        "엔진이 사용자 조작으로 보지 않은 시도"
    );
    assert!(!opened_by_user(None, false));
}

/// passive 피커와 그 증명 시험의 공통 준비. 첫 요청은 증명이 없어 passive로 열리고 Intent까지 처리된다.
struct PassivePicker {
    state: crate::state::RequestContext,
    engine_session: crate::runtime::engine_session::EngineSession,
    proofs: crate::app::html_runtime::NavigationProofs,
    sid: u32,
    first_request: u64,
    _view: Arc<()>,
}

fn plugin_caller_of(plugin_id: &str) -> CallerContext {
    CallerContext::Plugin {
        plugin_id: plugin_id.into(),
        permissions: Arc::new(HashSet::new()),
    }
}

impl PassivePicker {
    /// `origin` 을 주지 않으면 소스를 설치한 surface 를 출발지로 삼는다.
    fn opened(origin: Option<u32>, drain: bool) -> Self {
        let (mut state, mut engine_session) = source_fixture();
        for def in crate::adapters::ui::popup::defs::all_defs() {
            state.popups.register_def(def, 1.0);
        }
        let view = Arc::new(());
        state.webview_identity = Arc::downgrade(&view);
        let (sid, first_request) = {
            let mut engine = engine_session.borrow_mut();
            let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
            install_source(&mut engine, sid);
            let resp = crate::adapters::ipc::handler::file_picker::handle_trigger_with_proofs(
                &mut state,
                &mut engine,
                &plugin_caller_of(PLUGIN),
                json!(1),
                &json!({ "origin_surface_id": origin.unwrap_or(sid) }),
                None,
            );
            let first = resp.result.expect("first trigger")["request_id"]
                .as_u64()
                .unwrap();
            (sid, first)
        };
        let mut this = Self {
            state,
            engine_session,
            proofs: crate::app::html_runtime::NavigationProofs::default(),
            sid,
            first_request,
            _view: view,
        };
        if drain {
            this.drain();
            assert!(this.state.popups.is_passive(PICKER));
        }
        this
    }

    fn drain(&mut self) {
        let mut engine = self.engine_session.borrow_mut();
        for intent in self.state.take_pending_intents() {
            if matches!(intent.body, Intent::Ui(_)) {
                crate::intent::popup::handle(&mut self.state, &mut engine, &intent);
            }
        }
    }

    fn prove(&mut self) {
        let engine = self.engine_session.borrow_mut();
        gesture(true).record(&self.proofs, &self.state, &engine, self.sid);
    }

    fn prove_as(&mut self, plugin_id: &str) {
        let engine = self.engine_session.borrow_mut();
        let remote = engine
            .runtime
            .surfaces
            .get(&self.sid)
            .unwrap()
            .as_any()
            .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            .unwrap();
        self.proofs.record(
            &self.state.webview_identity,
            self.sid,
            Some(&crate::plugin_bridge::user_navigation::NavigationOwner {
                plugin_id: plugin_id.into(),
                wrote_page: true,
            }),
            Some(&remote.webview_url),
            &crate::webview::PendingNavigation {
                url: NAV_URL.into(),
                user_gesture: true,
            },
        );
    }

    fn trigger(&mut self, caller: &str, surface: u32) -> JsonRpcResponse {
        let mut engine = self.engine_session.borrow_mut();
        crate::adapters::ipc::handler::file_picker::handle_trigger_with_proofs(
            &mut self.state,
            &mut engine,
            &plugin_caller_of(caller),
            json!(2),
            &json!({ "origin_surface_id": surface, "user_navigation_url": NAV_URL }),
            Some(&self.proofs),
        )
    }

    fn proof_is_kept(&mut self) -> bool {
        let engine = self.engine_session.borrow_mut();
        self.proofs.take(
            &self.state.webview_identity,
            &engine.as_ref(),
            PLUGIN,
            self.sid,
            NAV_URL,
        )
    }

    fn assert_busy(&mut self, resp: &JsonRpcResponse, what: &str) {
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some(-32000), "{what}");
        assert!(self.proof_is_kept(), "{what}: 증명을 소비하지 않는다");
    }

    fn assert_stays_first_request(&self) {
        let requester = self
            .state
            .dialogs
            .file_picker
            .as_ref()
            .and_then(|d| d.requester.as_ref())
            .expect("first requester");
        assert_eq!(requester.plugin_id, PLUGIN);
        assert_eq!(requester.request_id, self.first_request);
    }
}

const PICKER: &str = crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;

#[test]
fn a_proven_press_focuses_the_passive_picker_of_the_same_plugin_and_surface() {
    let mut p = PassivePicker::opened(None, true);
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    assert_eq!(
        resp.result.expect("promoted")["request_id"].as_u64(),
        Some(p.first_request),
        "첫 요청의 번호를 돌려준다"
    );
    assert!(!p.proof_is_kept(), "증명을 소비한다");
    assert!(p.state.popups.is_passive(PICKER), "Intent 처리 전");
    p.drain();
    assert!(p.state.popups.is_focused(PICKER));
    assert!(!p.state.popups.is_passive(PICKER));
    assert!(p.state.has_egui_overlay_open());
    p.assert_stays_first_request();
}

/// 경합으로 둘째 요청이 첫 요청의 열기 Intent보다 먼저 처리돼도 passive로 갇히지 않는다.
#[test]
fn a_proven_press_before_the_open_intent_is_handled_still_focuses_the_picker() {
    let mut p = PassivePicker::opened(None, false);
    assert!(!p.state.popups.is_open(PICKER));
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    assert_eq!(
        resp.result.expect("promoted")["request_id"].as_u64(),
        Some(p.first_request)
    );
    p.drain();
    assert!(p.state.popups.is_focused(PICKER));
    assert!(!p.state.popups.is_passive(PICKER));
}

#[test]
fn a_press_without_a_proof_stays_busy_and_passive() {
    let mut p = PassivePicker::opened(None, true);
    let resp = p.trigger(PLUGIN, p.sid);
    assert_eq!(resp.error.map(|e| e.code), Some(-32000));
    assert!(p.state.take_pending_intents().is_empty());
    assert!(p.state.popups.is_passive(PICKER));
    assert!(!p.state.popups.has_focused());
}

#[test]
fn a_press_of_another_plugin_or_surface_stays_busy_without_consuming_the_proof() {
    let mut p = PassivePicker::opened(None, true);
    p.prove();
    let resp = p.trigger("com.example.other", p.sid);
    p.assert_busy(&resp, "다른 plugin");
    assert!(p.state.popups.is_passive(PICKER));

    // 그 surface 의 소유 plugin 이 다른 경우: 다른 plugin 의 유효한 증명으로 이 피커를 올리지 않는다.
    let mut p = PassivePicker::opened(None, true);
    p.prove_as("com.example.other");
    let resp = p.trigger("com.example.other", p.sid);
    assert_eq!(resp.error.map(|e| e.code), Some(-32000));
    assert!(p.state.popups.is_passive(PICKER));

    let other_origin = u32::MAX;
    let mut p = PassivePicker::opened(Some(other_origin), true);
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "다른 surface 에서 연 피커");
    assert!(p.state.popups.is_passive(PICKER));
}

#[test]
fn a_press_on_a_focused_picker_stays_busy_without_consuming_the_proof() {
    let mut p = PassivePicker::opened(None, true);
    p.prove();
    assert!(p.trigger(PLUGIN, p.sid).result.is_some());
    p.drain();
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "이미 포커스된 피커");
    assert!(p.state.take_pending_intents().is_empty());
}

#[test]
fn a_press_after_the_picker_closed_stays_busy_without_consuming_the_proof() {
    let mut p = PassivePicker::opened(None, true);
    p.state.dialogs.file_picker.as_mut().unwrap().result =
        Some(crate::state::FilePickerResult::Cancelled);
    p.state.popups.close(PICKER);
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "결과 전달을 기다리는 피커");

    // 결과가 채워졌는데 팝업이 아직 닫히지 않은 틈도 같다.
    let mut p = PassivePicker::opened(None, true);
    p.state.dialogs.file_picker.as_mut().unwrap().result =
        Some(crate::state::FilePickerResult::Cancelled);
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "결과가 채워진 피커");

    // 닫기만 처리되고 결과가 아직 기록되지 않은 틈도 같다.
    let mut p = PassivePicker::opened(None, true);
    p.state.popups.close(PICKER);
    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "닫힌 뒤 결과 기록 전");
}

#[test]
fn a_press_while_a_focused_open_is_queued_stays_busy_without_consuming_the_proof() {
    let (mut state, mut engine_session) = source_fixture();
    let view = Arc::new(());
    state.webview_identity = Arc::downgrade(&view);
    let proofs = crate::app::html_runtime::NavigationProofs::default();
    let mut engine = engine_session.borrow_mut();
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    install_source(&mut engine, sid);
    gesture(true).record(&proofs, &state, &engine, sid);
    let params = json!({ "origin_surface_id": sid, "user_navigation_url": NAV_URL });
    let call = |state: &mut crate::state::RequestContext, engine: &mut _| {
        crate::adapters::ipc::handler::file_picker::handle_trigger_with_proofs(
            state,
            engine,
            &plugin_caller_of(PLUGIN),
            json!(1),
            &params,
            Some(&proofs),
        )
    };
    assert!(call(&mut state, &mut engine).result.is_some());
    gesture(true).record(&proofs, &state, &engine, sid);
    let resp = call(&mut state, &mut engine);
    assert_eq!(resp.error.map(|e| e.code), Some(-32000));
    assert!(proofs.take(
        &state.webview_identity,
        &engine.as_ref(),
        PLUGIN,
        sid,
        NAV_URL
    ));
}

#[test]
fn a_press_on_a_passive_picker_hidden_by_its_scope_keeps_the_proof_and_the_other_focus() {
    use crate::adapters::ui::popup::PopupScope;

    const OTHER: &str = "other_popup";
    let mut p = PassivePicker::opened(None, true);
    p.state
        .popups
        .register(crate::adapters::ui::popup::PopupState::new(
            OTHER,
            "Other".to_string(),
            egui::vec2(200.0, 100.0),
        ));
    // 범위 대상은 실제로 있어야 한다. 없으면 draw 가 고아 팝업으로 닫는다.
    let picker_workspace = p.engine_session.borrow_mut().workspace_at(0).unwrap().id;
    p.state.popups.get_mut(PICKER).unwrap().scope = PopupScope::Workspace(picker_workspace);
    p.state.popups.open_with_scope(OTHER, PopupScope::Window);
    p.state.popups.set_focused(OTHER, true);

    // scope_visible 은 draw 만 갱신한다. 실제 프레임을 한 번 그려 범위 밖에 숨긴다.
    let layout = crate::adapters::ui::LayoutContext {
        active_workspace: 0,
        active_workspace_id: picker_workspace + 1,
        pane_rects: vec![],
        surface_rects: vec![],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
    };
    drop(
        egui::Context::default().run(egui::RawInput::default(), |ctx| {
            let engine = p.engine_session.borrow_mut();
            crate::adapters::ui::popup::frame::draw_popup_layer(
                ctx,
                &mut p.state,
                &engine.read(),
                &layout,
            )
        }),
    );
    assert!(!p.state.popups.is_visible_passive(PICKER), "숨은 피커");
    assert!(p.state.popups.is_passive(PICKER), "passive 는 유지된다");

    p.prove();
    let resp = p.trigger(PLUGIN, p.sid);
    p.assert_busy(&resp, "범위 밖에 숨은 피커");
    assert!(p.state.take_pending_intents().is_empty());
    assert!(
        p.state.popups.is_focused(OTHER),
        "보이는 팝업의 포커스 유지"
    );

    // Intent 가 직접 와도 숨은 피커는 올라가지 않고 보이는 팝업의 포커스를 풀지 않는다.
    assert!(!p.state.popups.focus_passive(PICKER));
    assert!(p.state.popups.is_focused(OTHER));
    assert!(p.state.popups.is_passive(PICKER));
}
