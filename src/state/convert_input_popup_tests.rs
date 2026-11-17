//! 변환 입력 팝업 요청을 큐에 넣는 판정. surface 범위 팝업은 대상이 없으면 받지 않는다.
//! 입력 시점 대상이 attach 생성 대기 초안이면 선언 범위와 관계없이 걸어 두고 생성된 surface 에서 연다.

const PLUGIN: &str = "com.example.viewer";
const POPUP_REF: &str = "com.example.viewer/convert";

/// `widget` kind 가 `com.example.viewer/convert` 를 변환 입력 팝업으로 쓰는 mirror 장면.
fn scene(
    surface_scoped: bool,
) -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    let (mut state, mut session) = crate::state::tests::test_mirror_state();
    if surface_scoped {
        state
            .tool_registry
            .set_surface_popups([POPUP_REF.to_string()].into_iter().collect());
    }
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "widget",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "egui-mesh",
        "convert_input_popup": "convert",
    }))
    .expect("kind decl");
    assert!(
        crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
            &session.borrow_mut().runtime.surface_registry,
            PLUGIN,
            &decl,
            tasty_plugin_manifest::HOST_API_VERSION,
        )
    );
    (state, session)
}

/// 포커스 surface 가 있을 때 요청한 결과와 큐 길이.
fn enqueue_focused(surface_scoped: bool) -> (bool, usize) {
    let (mut state, mut session) = scene(surface_scoped);
    let engine = session.borrow_mut();
    let read = engine.read();
    let queued = state.enqueue_convert_input_popup(&read, "widget", None);
    (queued, state.pending_popup_opens.len())
}

#[test]
fn a_surface_popup_with_a_focused_surface_is_queued_for_it() {
    assert_eq!(enqueue_focused(true), (true, 1));
}

#[test]
fn a_window_popup_is_queued_without_a_target() {
    assert_eq!(enqueue_focused(false), (true, 1));
}

/// 초안 선택 중 요청하면 큐에 넣지 않고 그 초안에 걸고, 생성된 surface 를 문맥·대상으로 연다.
/// 새 탭을 만드는 입력이라 제자리 변환 표시(`surface_id`)는 없다. 선언 범위와 관계없이 같다.
fn draft_request_opens_on_the_created_surface(surface_scoped: bool) {
    let (mut state, mut session) = scene(surface_scoped);
    let engine = session.borrow_mut();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).unwrap();
    let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
    let pending = state.attach_pending.entries[0].id;
    assert!(state.enqueue_convert_input_popup(&read, "widget", None));
    assert!(state.pending_popup_opens.is_empty());
    assert_eq!(state.attach_pending.deferred.waiting_for(pending), 1);
    crate::state::attach_pending::deferred::settle_first_created(&mut state, &read, anchor);
    let notices = crate::adapters::ui::tools_menu::run_deferred_tools(&mut state, &read);
    assert!(notices.is_empty());
    assert_eq!(state.pending_popup_opens.len(), 1);
    let open = &state.pending_popup_opens[0];
    assert_eq!(
        (open.plugin_id.as_str(), open.popup_id.as_str()),
        (PLUGIN, "convert")
    );
    assert_eq!(open.target_surface, Some(anchor));
    assert_eq!(open.context["origin_surface_id"], serde_json::json!(anchor));
    assert!(open.context.get("surface_id").is_none());
}

#[test]
fn a_surface_popup_for_a_draft_opens_on_the_created_surface() {
    draft_request_opens_on_the_created_surface(true);
}

#[test]
fn a_window_popup_for_a_draft_opens_on_the_created_surface() {
    draft_request_opens_on_the_created_surface(false);
}

/// 생성이 실패해 초안이 정리되면 걸어 둔 요청도 열지 않고 버린다.
#[test]
fn a_draft_request_is_dropped_when_the_creation_fails() {
    let (mut state, mut session) = scene(true);
    let engine = session.borrow_mut();
    let read = engine.read();
    let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
    let pending = state.attach_pending.entries[0].id;
    assert!(state.enqueue_convert_input_popup(&read, "widget", None));
    let epoch = state.attach_pending.entries[0].epoch.clone();
    state.attach_pending.entries[0].receipt.bind(0);
    state.attach_pending.result(&epoch, 0, None, false);
    state.attach_pending.retain(&read);
    assert_eq!(state.attach_pending.deferred.waiting_for(pending), 0);
    assert!(crate::adapters::ui::tools_menu::run_deferred_tools(&mut state, &read).is_empty());
    assert!(state.pending_popup_opens.is_empty());
}

/// 변환 입력 팝업이 없는 kind 는 초안에도 걸지 않는다.
#[test]
fn a_kind_without_a_convert_input_popup_is_not_held_for_a_draft() {
    let (mut state, mut session) = scene(true);
    let engine = session.borrow_mut();
    let read = engine.read();
    let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
    let pending = state.attach_pending.entries[0].id;
    assert!(!state.enqueue_convert_input_popup(&read, "no-such-kind", None));
    assert_eq!(state.attach_pending.deferred.waiting_for(pending), 0);
}
