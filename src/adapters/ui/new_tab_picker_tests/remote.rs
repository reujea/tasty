use super::*;
fn prepare() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
    u32,
) {
    let (state, mut session) = crate::state::tests::test_mirror_state();
    let (workspace, pane) = {
        let mut engine = session.borrow_mut();
        let definition = serde_json::from_value(serde_json::json!({"kind":"html", "rendering":"webview", "display_name_i18n_key":"surface.kind.html"})).unwrap();
        let (sender, _receiver) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.runtime.surface_registry,
            "com.tasty.html",
            &definition,
            sender,
        );
        let workspace = engine.workspace_at(0).unwrap();
        let (workspace, pane, surface) = (
            workspace.id,
            workspace.pane_layout().all_pane_ids()[0],
            workspace.all_surface_ids()[0],
        );
        engine.remote.html_entry_hosts.insert(workspace);
        engine.set_mirror_surface_cwd(surface, Some("/server/project".into()));
        (workspace, pane)
    };
    (state, session, workspace, pane)
}
#[test]
fn supported_remote_html_picker_keeps_request_pane_and_remote_cwd() {
    let (mut state, mut session, workspace, pane) = prepare();
    let engine = session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        pane,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let picker = crate::adapters::ui::popup::file_picker::instances::current(&mut state).unwrap();
    assert_eq!(picker.mirror_ws_id, Some(workspace));
    assert_eq!(picker.current_dir, "/server/project");
    let request = stored_request(&state);
    assert_eq!(request.pane_id, pane);
    assert!(request.mirror.is_some());
    state.take_pending_intents();
    apply_outcome(
        &mut state,
        &engine,
        &confirmed(request.request_id, &["/server/project/entry.svg"]),
    );
    assert_eq!(
        create_tabs(&state.take_pending_intents()),
        vec![(
            pane,
            "html",
            &serde_json::json!({"url":"/server/project/entry.svg"})
        )]
    );
}
#[test]
fn replaced_mirror_projection_discards_remote_html_picker_result() {
    let (mut state, mut session, _workspace, pane) = prepare();
    let engine = session.borrow_mut();
    open(
        &mut state,
        &engine.read(),
        pane,
        "html",
        "url",
        HTML_EXTENSIONS,
    );
    let request = stored_request(&state);
    let workspace = engine.workspace_at(0).unwrap();
    let tab = workspace.pane_layout().find_pane(pane).unwrap().tabs[0].id;
    let mut replacement = crate::model::Workspace::new_with_terminal_marker(
        workspace.id,
        "replacement".into(),
        pane,
        tab,
        workspace.all_surface_ids()[0],
    );
    replacement.mirror = true;
    assert!(engine.core.replace_mirror_workspace(replacement).is_ok());
    state.take_pending_intents();
    apply_outcome(
        &mut state,
        &engine,
        &confirmed(request.request_id, &["/server/project/entry.html"]),
    );
    assert!(state.take_pending_intents().is_empty());
    assert!(state.dialogs.file_picker_callers.new_tab.is_empty());
}

/// mirror workspace의 원격 파일 열기는 kind 이름이 아니라 `opens_url`·`mirror_viewer` 선언을 본다.
#[test]
fn a_remote_picker_follows_the_viewer_declarations() {
    let (mut state, mut session, _workspace, pane) = prepare();
    let engine = session.borrow_mut();
    for (kind, extra) in [
        (
            "my-html",
            serde_json::json!({"opens_url": true, "mirror_viewer": true}),
        ),
        ("half-html", serde_json::json!({"mirror_viewer": true})),
    ] {
        let mut decl = serde_json::json!({
            "kind": kind, "rendering": "webview", "display_name_i18n_key": "surface.kind.html"
        });
        decl.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let (sender, _receiver) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.runtime.surface_registry,
            "com.example.web",
            &serde_json::from_value(decl).unwrap(),
            sender,
        );
    }
    open(
        &mut state,
        &engine.read(),
        pane,
        "my-html",
        "url",
        HTML_EXTENSIONS,
    );
    assert!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state).is_some(),
        "declared viewer kind"
    );
    assert_eq!(state.toasts.len(), 0);

    // intent-exempt: 시험 준비로 첫 피커를 결과 없이 치운다.
    state
        .popups
        .close(crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID);
    state.popups.take_closed_queue();
    open(
        &mut state,
        &engine.read(),
        pane,
        "half-html",
        "url",
        HTML_EXTENSIONS,
    );
    assert!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state).is_none(),
        "opens_url not declared"
    );
    assert_eq!(state.toasts.len(), 1);
}
