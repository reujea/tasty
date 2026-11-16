//! 변환 입력 팝업 요청을 큐에 넣는 판정. surface 범위 팝업은 대상이 없으면 받지 않는다.

const PLUGIN: &str = "com.example.viewer";
const POPUP_REF: &str = "com.example.viewer/convert";

/// `widget` kind 가 `com.example.viewer/convert` 를 변환 입력 팝업으로 쓰는 mirror 장면.
/// `pending_draft`면 attach 대기 초안을 골라 포커스 surface 가 없게 한다.
fn enqueue(surface_scoped: bool, pending_draft: bool) -> (bool, usize) {
    let (mut state, mut session) = crate::state::tests::test_mirror_state();
    if surface_scoped {
        state
            .tool_registry
            .set_surface_popups([POPUP_REF.to_string()].into_iter().collect());
    }
    let engine = session.borrow_mut();
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "widget",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "egui-mesh",
        "convert_input_popup": "convert",
    }))
    .expect("kind decl");
    assert!(
        crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
            &engine.runtime.surface_registry,
            PLUGIN,
            &decl,
            tasty_plugin_manifest::HOST_API_VERSION,
        )
    );
    let read = engine.read();
    let _draft =
        pending_draft.then(|| crate::state::tests::select_pending_draft(&mut state, &read));
    let queued = state.enqueue_convert_input_popup(&read, "widget", None);
    (queued, state.pending_popup_opens.len())
}

#[test]
fn a_surface_popup_without_a_target_is_not_queued() {
    assert_eq!(enqueue(true, true), (false, 0));
}

#[test]
fn a_surface_popup_with_a_focused_surface_is_queued_for_it() {
    assert_eq!(enqueue(true, false), (true, 1));
}

#[test]
fn a_window_popup_is_queued_without_a_target() {
    assert_eq!(enqueue(false, true), (true, 1));
}
