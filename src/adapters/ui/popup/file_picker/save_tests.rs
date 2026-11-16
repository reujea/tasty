use super::*;
use serde_json::json;

#[test]
fn plugin_save_picker_returns_one_path_without_writing_and_preserves_admission() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("existing.png"), b"unchanged").unwrap();
    std::fs::create_dir(dir.path().join("folder.png")).unwrap();
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: "com.tasty.image".into(),
        permissions: std::sync::Arc::new(Default::default()),
    };
    let origin = state.focused_surface_id(&engine.read());
    let params = json!({"mode":"save", "location":"local", "default_name":"new.png", "filters":["png"], "start_dir":dir.path(), "origin_surface_id": origin});
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &params,
    );
    let id = response.result.unwrap()["request_id"].as_u64().unwrap();
    check_opened_state(state.dialogs.file_picker.as_ref().unwrap());
    assert!(
        matches!(
            apply_action(&mut state, &engine.read(), FilePickerAction::Confirm),
            PopupAction::None
        ),
        "loading cannot confirm"
    );
    let d = state.dialogs.file_picker.as_mut().unwrap();
    d.entries = crate::core::fs_list::read_dir_entries(dir.path()).unwrap();
    d.load = FpLoadState::Loaded;
    state.take_pending_intents();
    assert!(matches!(
        apply_action(
            &mut state,
            &engine.read(),
            FilePickerAction::ConfirmEntry("existing.png".into())
        ),
        PopupAction::None
    ));
    let d = state.dialogs.file_picker.as_ref().unwrap();
    assert_eq!(d.save_name.as_deref(), Some("existing.png"));
    assert!(save::overwrites("existing.png", &d.entries, &d.filters));
    check_rejected_names(&mut state, &engine.read());
    apply_action(
        &mut state,
        &engine.read(),
        FilePickerAction::EditName(" new.png ".into()),
    );
    assert!(matches!(
        apply_action(&mut state, &engine.read(), FilePickerAction::Confirm),
        PopupAction::Close
    ));
    let busy = crate::adapters::ipc::handler::file_picker::handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(2),
        &params,
    );
    assert_eq!(busy.error.unwrap().code, -32000);
    let (outcome, requester) = crate::app::dispatch::file_picker::take_outcome(&mut state).unwrap();
    assert_eq!(outcome.request_id.0, id);
    assert_eq!(requester.unwrap().request_id, id);
    assert_eq!(
        outcome.paths,
        vec![dir.path().join("new.png").to_string_lossy().into_owned()]
    );
    assert!(!outcome.cancelled);
    assert!(!dir.path().join("new.png").exists());
    assert_eq!(
        std::fs::read(dir.path().join("existing.png")).unwrap(),
        b"unchanged"
    );
    assert!(state.take_pending_intents().is_empty());
    assert!(crate::app::dispatch::file_picker::take_outcome(&mut state).is_none());
}

fn check_rejected_names(state: &mut MainViewState, engine: &EngineRead<'_>) {
    for name in [
        "",
        "..",
        "../escape.png",
        "a/b.png",
        "a\\b.png",
        "folder.png",
    ] {
        apply_action(state, engine, FilePickerAction::EditName(name.into()));
        assert!(
            matches!(
                apply_action(state, engine, FilePickerAction::Confirm),
                PopupAction::None
            ),
            "{name}"
        );
    }
}

fn check_opened_state(d: &crate::state::FilePickerData) {
    assert_eq!(d.save_name.as_deref(), Some("new.png"));
    assert!(d.mirror_ws_id.is_none());
    assert_eq!(d.requester.as_ref().unwrap().plugin_id, "com.tasty.image");
    assert!(d.requester.as_ref().unwrap().owner_popup_instance.is_none());
}
