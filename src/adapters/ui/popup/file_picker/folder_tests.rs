use super::*;
use serde_json::json;

fn open_folder_picker(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    dir: &std::path::Path,
) {
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: "com.example.folder".into(),
    };
    let origin = state.focused_surface_id(&engine.read());
    let params = json!({"mode": "folder", "location": "local", "start_dir": dir, "origin_surface_id": origin});
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        state,
        engine,
        &caller,
        json!(1),
        &params,
    );
    assert!(response.result.is_some(), "folder mode was refused");
    let d = crate::adapters::ui::popup::file_picker::instances::current_mut(state).unwrap();
    assert!(d.folder, "mode folder did not open the folder picker");
    d.entries = crate::core::fs_list::read_dir_entries(dir).unwrap();
    d.load = FpLoadState::Loaded;
}

fn confirmed(state: &crate::state::RequestContext) -> Option<Vec<String>> {
    match crate::adapters::ui::popup::file_picker::instances::latest(state)?
        .result
        .as_ref()?
    {
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
        crate::adapters::ui::popup::file_picker::instances::apply_to_current(
            &mut state,
            &engine.read(),
            FilePickerAction::Confirm
        ),
        PopupAction::Close
    ));
    let current = crate::adapters::ui::popup::file_picker::instances::current(&mut state)
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
    let current = crate::adapters::ui::popup::file_picker::instances::current(&mut state)
        .unwrap()
        .current_dir
        .clone();
    crate::adapters::ui::popup::file_picker::instances::apply_to_current(
        &mut state,
        &engine.read(),
        FilePickerAction::Select("proj".into()),
    );
    assert!(matches!(
        crate::adapters::ui::popup::file_picker::instances::apply_to_current(
            &mut state,
            &engine.read(),
            FilePickerAction::Confirm
        ),
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
    crate::adapters::ui::popup::file_picker::instances::apply_to_current(
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
    crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state)
        .unwrap()
        .load = FpLoadState::Loading {
        request_id: 0,
        sent_at: std::time::Instant::now(),
    };
    assert!(matches!(
        crate::adapters::ui::popup::file_picker::instances::apply_to_current(
            &mut state,
            &engine.read(),
            FilePickerAction::Confirm
        ),
        PopupAction::None
    ));
    assert_eq!(confirmed(&state), None);
}

// ── 폴더 키(노드 키맵 읽기). 수식키는 egui 매처처럼 패턴에 없는 Shift·Alt 를 허용한다. ──

fn entries() -> Vec<FilePickerEntryView> {
    ["proj", "src"]
        .into_iter()
        .map(|name| FilePickerEntryView {
            name: name.into(),
            is_dir: true,
            size_display: String::new(),
            modified_display: String::new(),
        })
        .collect()
}

/// `mode` 로 폴더 목록 보기의 키만 한 프레임 읽는다. `dropdown` 이면 그리기 전에 egui 팝업 하나가 열려 있다.
fn folder_key(
    mode: FilePickerMode<'_>,
    dropdown: bool,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<FilePickerAction> {
    let th = crate::theme::theme();
    let entries = entries();
    let selected: Vec<String> = Vec::new();
    let props = FilePickerProps {
        theme: &th,
        remote_host: None,
        crumbs: &[],
        state: FpViewState::Loaded,
        entries: &entries,
        selected: &selected,
        mode,
        escape: false,
        title_label: "",
        name_field_label: "",
        name_placeholder: "",
        cancel_label: "",
        confirm_label: "",
        overwrite_warning: "",
        hidden_folders_one: "",
        hidden_folders_many: "",
        folder_not_save_target: "",
        folder_open_enters: "",
        empty_label: "",
        loading_label: "",
        loading_body_local: "",
        loading_body_remote: "",
        error_perm_title: "",
        error_perm_retry: "",
        error_conn_title: "",
        error_conn_reconnect: "",
        col_name: "",
        col_size: "",
        col_modified: "",
        filters: &[],
        filter_showing: "",
    };
    let ctx = egui::Context::default();
    if dropdown {
        ctx.memory_mut(|m| m.open_popup(egui::Id::new("folder-test-dropdown")));
    }
    let mut action = None;
    drop(ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut keys = crate::adapters::ui::popup::keys::PopupKeys::new(
                    ctx,
                    true,
                    crate::adapters::ui::popup::PopupKeyboard::Text,
                    &crate::adapters::ui::input::edit_key::EditBindings::default(),
                );
                action = folder::keyboard(ui, &props, &mut keys);
            });
        },
    ));
    action
}

fn folder_mode() -> FilePickerMode<'static> {
    FilePickerMode::Folder {
        path: "/w",
        hint: "",
        file_tooltip: "",
    }
}

/// T-9b-1: Given 폴더 모드 목록, When Alt+↑ 를 누르면, Then 상위 폴더로 간다.
#[test]
fn alt_up_goes_to_the_parent_folder() {
    assert_eq!(
        folder_key(
            folder_mode(),
            false,
            egui::Key::ArrowUp,
            egui::Modifiers::ALT
        ),
        Some(FilePickerAction::NavigateUp)
    );
}

/// T-9b-2: Given 폴더 모드 목록, When Alt+Shift+↑ 를 누르면, Then 상위 폴더로 간다.
#[test]
fn alt_shift_up_goes_to_the_parent_folder() {
    assert_eq!(
        folder_key(
            folder_mode(),
            false,
            egui::Key::ArrowUp,
            egui::Modifiers::ALT | egui::Modifiers::SHIFT
        ),
        Some(FilePickerAction::NavigateUp)
    );
}

/// T-9b-3: Given 폴더 모드 목록, When Shift+Enter 를 누르면, Then 지금 폴더를 확정한다.
#[test]
fn shift_enter_opens_the_folder() {
    assert_eq!(
        folder_key(
            folder_mode(),
            false,
            egui::Key::Enter,
            egui::Modifiers::SHIFT
        ),
        Some(FilePickerAction::Confirm)
    );
}

/// T-9b-4: Given 일반 파일 모드 목록, When Alt+↑ 를 누르면, Then 아무 동작도 없다.
#[test]
fn alt_up_in_file_mode_does_nothing() {
    let open = FilePickerMode::Open { selection_text: "" };
    assert_eq!(
        folder_key(open, false, egui::Key::ArrowUp, egui::Modifiers::ALT),
        None
    );
}

/// T-9b-5: Given egui 드롭다운이 열린 폴더 모드 목록, When Alt+↑ 를 누르면, Then 아무 동작도 없다.
#[test]
fn alt_up_with_a_dropdown_open_does_nothing() {
    assert_eq!(
        folder_key(
            folder_mode(),
            true,
            egui::Key::ArrowUp,
            egui::Modifiers::ALT
        ),
        None
    );
}

/// T-12b-14 [폴더 피커 + Alt+↑]: Given 폴더 피커가 포커스이고 Alt+↑ 가 탐색기 상위 폴더 단축키(`explorer_go_up`)이기도
/// 할 때, When 창이 Alt+↑ 를 받으면, Then 창 키 경로가 그 키를 피커에 싣고 그 몫을 그린 피커가 상위 폴더로 간다.
#[test]
fn a_window_alt_up_on_the_folder_picker_goes_to_the_parent_folder() {
    use crate::adapters::ui::popup::window_input_test_support::{WindowInput, layer_frame};
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("child");
    std::fs::create_dir(&child).unwrap();
    {
        let mut engine = session.borrow_mut();
        assert!(
            engine
                .runtime
                .settings
                .keybindings
                .explorer_go_up
                .iter()
                .any(|b| b == "alt+up")
        );
        open_folder_picker(&mut state, &mut engine, &child);
    }
    let engine = session.borrow_mut();
    let read = engine.read();
    state.normalize_focus(&read);
    let ctx = egui::Context::default();
    layer_frame(&ctx, &mut state, &read, Vec::new());
    let before = crate::adapters::ui::popup::file_picker::instances::current(&mut state)
        .unwrap()
        .current_dir
        .clone();
    let mut input = WindowInput::default();
    let up = egui::Event::Key {
        key: egui::Key::ArrowUp,
        physical_key: Some(egui::Key::ArrowUp),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::ALT,
    };
    assert!(
        input.key(&mut state, &read, false, up).is_some(),
        "Alt+↑ 는 피커에 실린다"
    );
    let events = input.take_pass(&mut state, &ctx);
    layer_frame(&ctx, &mut state, &read, events);
    let after = crate::adapters::ui::popup::file_picker::instances::current(&mut state)
        .unwrap()
        .current_dir
        .clone();
    assert_ne!(after, before, "상위 폴더로 갔다");
    assert_eq!(
        std::path::Path::new(&after).canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}
