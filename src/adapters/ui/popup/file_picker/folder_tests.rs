use super::*;
use serde_json::json;

fn open_folder_picker(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    dir: &std::path::Path,
) {
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: "com.example.folder".into(),
        permissions: std::sync::Arc::new(Default::default()),
    };
    let params = json!({"mode": "folder", "location": "local", "start_dir": dir});
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        state,
        engine,
        &caller,
        json!(1),
        &params,
    );
    assert!(response.result.is_some(), "folder mode was refused");
    let d = state.dialogs.file_picker.as_mut().unwrap();
    assert!(d.folder, "mode folder did not open the folder picker");
    d.entries = crate::core::fs_list::read_dir_entries(dir).unwrap();
    d.load = FpLoadState::Loaded;
}

fn confirmed(state: &crate::state::RequestContext) -> Option<Vec<String>> {
    match state.dialogs.file_picker.as_ref()?.result.as_ref()? {
        FilePickerResult::Confirmed { paths } => Some(paths.clone()),
        FilePickerResult::Cancelled => None,
    }
}

#[test]
fn confirming_without_a_selection_returns_the_current_folder() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), b"").unwrap();
    open_folder_picker(&mut state, &mut engine, dir.path());
    assert!(matches!(
        apply_action(&mut state, &engine.read(), FilePickerAction::Confirm),
        PopupAction::Close
    ));
    let current = state
        .dialogs
        .file_picker
        .as_ref()
        .unwrap()
        .current_dir
        .clone();
    assert_eq!(confirmed(&state), Some(vec![current]));
}

#[test]
fn confirming_a_selected_subfolder_returns_it_without_entering() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("proj")).unwrap();
    open_folder_picker(&mut state, &mut engine, dir.path());
    let current = state
        .dialogs
        .file_picker
        .as_ref()
        .unwrap()
        .current_dir
        .clone();
    apply_action(
        &mut state,
        &engine.read(),
        FilePickerAction::Select("proj".into()),
    );
    assert!(matches!(
        apply_action(&mut state, &engine.read(), FilePickerAction::Confirm),
        PopupAction::Close
    ));
    assert_eq!(
        confirmed(&state),
        Some(vec![join_dir(false, &current, "proj")])
    );
}

#[test]
fn a_file_entry_never_confirms_in_folder_mode() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), b"").unwrap();
    open_folder_picker(&mut state, &mut engine, dir.path());
    apply_action(
        &mut state,
        &engine.read(),
        FilePickerAction::ConfirmEntry("a.md".into()),
    );
    assert_eq!(
        confirmed(&state),
        None,
        "a file was confirmed as the folder"
    );
}

#[test]
fn a_loading_folder_cannot_confirm() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    let dir = tempfile::tempdir().unwrap();
    open_folder_picker(&mut state, &mut engine, dir.path());
    state.dialogs.file_picker.as_mut().unwrap().load = FpLoadState::Loading {
        request_id: 0,
        sent_at: std::time::Instant::now(),
    };
    assert!(matches!(
        apply_action(&mut state, &engine.read(), FilePickerAction::Confirm),
        PopupAction::None
    ));
    assert_eq!(confirmed(&state), None);
}
