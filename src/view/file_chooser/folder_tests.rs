use super::*;

fn loaded(mode: FileChooserMode) -> LocalFileChooser {
    let mut ch = LocalFileChooser::default();
    ch.begin_at(
        "folder.test",
        mode,
        vec!["png".into()],
        PathBuf::from("/fixture"),
    );
    let s = ch.session.as_mut().unwrap();
    s.query = None;
    s.accept_read(Ok(vec![
        DirEntryInfo {
            name: "sub".into(),
            is_dir: true,
            ..Default::default()
        },
        DirEntryInfo {
            name: "file.txt".into(),
            ..Default::default()
        },
    ]));
    ch
}
#[test]
fn folder_confirm_uses_selected_or_current_and_never_enters() {
    let mut ch = loaded(FileChooserMode::Folder);
    let s = ch.session.as_mut().unwrap();
    assert!(
        s.visible(&s.entries[1]),
        "folder mode ignores extension filters"
    );
    assert_eq!(
        s.apply(FilePickerAction::Confirm),
        Some(FileChooserOutcome::confirmed(PathBuf::from("/fixture")))
    );
    s.apply(FilePickerAction::Select("sub".into()));
    assert_eq!(
        s.apply(FilePickerAction::Confirm),
        Some(FileChooserOutcome::confirmed(
            PathBuf::from("/fixture").join("sub")
        ))
    );
    assert_eq!(s.current_dir, PathBuf::from("/fixture"));
    s.apply(FilePickerAction::Select("file.txt".into()));
    assert_eq!(s.selected, ["sub"]);
    assert_eq!(
        s.apply(FilePickerAction::ConfirmEntry("file.txt".into())),
        None
    );
    s.apply(FilePickerAction::Select("sub".into()));
    assert!(s.selected.is_empty());
    s.apply(FilePickerAction::Select("sub".into()));
    s.apply(FilePickerAction::ClearSelection);
    assert!(s.selected.is_empty());
}
#[test]
fn folder_double_click_enters_and_unread_or_failed_directory_cannot_confirm() {
    let mut ch = loaded(FileChooserMode::Folder);
    let s = ch.session.as_mut().unwrap();
    s.apply(FilePickerAction::NavigateInto("sub".into()));
    assert_eq!(s.current_dir, PathBuf::from("/fixture").join("sub"));
    assert!(s.selected.is_empty());
    assert_eq!(s.apply(FilePickerAction::Confirm), None);
    s.query = None;
    s.accept_read(Err(std::io::Error::from(
        std::io::ErrorKind::PermissionDenied,
    )));
    assert_eq!(s.apply(FilePickerAction::Confirm), None);
    s.accept_read(Ok(vec![]));
    assert_eq!(
        s.apply(FilePickerAction::Confirm),
        Some(FileChooserOutcome::confirmed(
            PathBuf::from("/fixture").join("sub")
        ))
    );
}
#[test]
fn folder_outcome_is_consumer_owned_and_consumed_once_including_cancel() {
    let mut ch = loaded(FileChooserMode::Folder);
    let result = ch
        .session
        .as_mut()
        .unwrap()
        .apply(FilePickerAction::Confirm)
        .unwrap();
    ch.finish(result.clone());
    assert!(ch.take_outcome("different").is_none());
    assert_eq!(ch.take_outcome("folder.test"), Some(result));
    assert!(ch.take_outcome("folder.test").is_none());
    ch = loaded(FileChooserMode::Folder);
    ch.cancel();
    assert_eq!(
        ch.take_outcome("folder.test"),
        Some(FileChooserOutcome::Cancelled)
    );
    assert!(ch.take_outcome("folder.test").is_none());
}

fn frame(ch: &mut LocalFileChooser, ctx: &egui::Context, key: egui::Key) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(480.0, 360.0),
        )),
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    drop(ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ch.draw(ui, &crate::theme::theme(), true);
        });
    }));
}
#[test]
fn folder_keyboard_skips_files_and_enter_confirms_while_right_enters() {
    let ctx = egui::Context::default();
    let mut ch = loaded(FileChooserMode::Folder);
    frame(&mut ch, &ctx, egui::Key::ArrowDown);
    frame(&mut ch, &ctx, egui::Key::ArrowDown);
    assert_eq!(ch.session.as_ref().unwrap().selected, ["sub"]);
    frame(&mut ch, &ctx, egui::Key::Enter);
    assert_eq!(
        ch.take_outcome("folder.test"),
        Some(FileChooserOutcome::confirmed(
            PathBuf::from("/fixture").join("sub")
        ))
    );
    ch = loaded(FileChooserMode::Folder);
    frame(&mut ch, &ctx, egui::Key::ArrowDown);
    frame(&mut ch, &ctx, egui::Key::ArrowRight);
    assert_eq!(
        ch.session.as_ref().unwrap().current_dir,
        PathBuf::from("/fixture").join("sub")
    );
    assert!(ch.take_outcome("folder.test").is_none());
    frame(&mut ch, &ctx, egui::Key::Escape);
    assert_eq!(
        ch.take_outcome("folder.test"),
        Some(FileChooserOutcome::Cancelled)
    );
}
