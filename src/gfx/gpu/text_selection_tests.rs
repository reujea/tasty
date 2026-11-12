//! The egui side of the native Wayland clipboard: which keys paste and which output copies.
use super::{moves_focus, paste_key, pastes_for, release_pastes, take_copied_text};
use winit::keyboard::{Key, KeyCode, NamedKey, NativeKey, NativeKeyCode, PhysicalKey, SmolStr};

fn ch(c: &str) -> Key {
    Key::Character(SmolStr::new(c))
}
const V: PhysicalKey = PhysicalKey::Code(KeyCode::KeyV);
const B: PhysicalKey = PhysicalKey::Code(KeyCode::KeyB);

#[test]
fn ctrl_v_and_the_paste_key_paste_like_egui_winit() {
    assert!(paste_key(&ch("v"), V, true));
    assert!(paste_key(&ch("V"), V, true));
    assert!(!paste_key(&ch("v"), V, false));
    assert!(paste_key(&Key::Named(NamedKey::Paste), V, false));
    // A Paste key without a logical meaning still pastes; a mapped letter on it does not.
    let paste = PhysicalKey::Code(KeyCode::Paste);
    assert!(paste_key(&Key::Dead(None), paste, false));
    assert!(!paste_key(&ch("x"), paste, false));
}

#[test]
fn the_logical_key_wins_and_a_non_latin_layout_falls_back_to_the_physical_key() {
    // Dvorak: logical v on another position still pastes; logical k on the V position does not.
    assert!(paste_key(&ch("v"), B, true));
    assert!(!paste_key(&ch("k"), V, true));
    // Cyrillic м on the V position maps to no egui key, so the physical V counts.
    assert!(paste_key(&ch("м"), V, true));
    assert!(paste_key(&Key::Dead(None), V, true));
    assert!(!paste_key(
        &Key::Unidentified(NativeKey::Unidentified),
        B,
        true
    ));
    assert!(!paste_key(&Key::Named(NamedKey::ArrowLeft), V, true));
}
#[test]
fn native_codes_do_not_paste() {
    assert!(!paste_key(
        &Key::Unidentified(NativeKey::Unidentified),
        PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
        true
    ));
}

#[test]
fn every_copy_is_taken_and_the_last_text_wins() {
    let mut output = egui::PlatformOutput::default();
    output
        .commands
        .push(egui::OutputCommand::CopyText("first".into()));
    output
        .commands
        .push(egui::OutputCommand::OpenUrl(egui::OpenUrl::same_tab(
            "https://a.example",
        )));
    output
        .commands
        .push(egui::OutputCommand::CopyText("second".into()));
    let (output, copied) = take_copied_text(output);
    assert_eq!(copied.as_deref(), Some("second"));
    assert_eq!(output.commands.len(), 1);
    assert!(matches!(
        output.commands[0],
        egui::OutputCommand::OpenUrl(_)
    ));
}

#[test]
fn the_deprecated_copied_text_is_taken_too() {
    let mut output = egui::PlatformOutput::default();
    // reason: egui-winit still applies this field, so it must not reach egui-winit either.
    #[allow(deprecated)]
    {
        output.copied_text = "legacy".into();
    }
    let (output, copied) = take_copied_text(output);
    assert_eq!(copied.as_deref(), Some("legacy"));
    // reason: as above.
    #[allow(deprecated)]
    let left = output.copied_text.is_empty();
    assert!(left);
    let (_, none) = take_copied_text(egui::PlatformOutput::default());
    assert_eq!(none, None);
}

#[test]
fn a_paste_reaches_only_the_widget_that_asked_while_it_is_still_focused() {
    let finished = vec![(7, "kept".to_string()), (8, "other field".to_string())];
    assert_eq!(pastes_for(finished.clone(), 7), ["kept"]);
    assert!(pastes_for(finished, 9).is_empty());
}

fn click() -> egui::Event {
    egui::Event::PointerButton {
        pos: egui::pos2(1.0, 1.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }
}
fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn a_paste_waits_out_a_frame_whose_input_can_move_focus() {
    for moving in [click(), key(egui::Key::Tab, true)] {
        // The frame that also moves focus applies nothing; the paste is checked again next frame.
        let mut held = vec![(7, "text".to_string())];
        assert!(release_pastes(&mut held, std::slice::from_ref(&moving), 7).is_empty());
        assert_eq!(held.len(), 1);
        // Focus ended on another widget: dropped.
        assert!(release_pastes(&mut held, &[], 9).is_empty());
        assert!(held.is_empty());

        let mut held = vec![(7, "text".to_string())];
        assert!(release_pastes(&mut held, &[moving], 7).is_empty());
        // Focus stayed on the widget that asked: applied.
        assert_eq!(release_pastes(&mut held, &[], 7), ["text"]);
        assert!(held.is_empty());
    }
}

#[test]
fn only_input_egui_reads_for_focus_holds_a_paste() {
    for event in [
        click(),
        key(egui::Key::Tab, true),
        key(egui::Key::ArrowDown, true),
        key(egui::Key::Escape, true),
    ] {
        assert!(moves_focus(&event), "{event:?}");
    }
    let released = egui::Event::PointerButton {
        pos: egui::pos2(1.0, 1.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    };
    for event in [
        released,
        key(egui::Key::Tab, false),
        key(egui::Key::A, true),
        egui::Event::Text("a".into()),
        egui::Event::PointerMoved(egui::pos2(2.0, 2.0)),
    ] {
        assert!(!moves_focus(&event), "{event:?}");
    }
    let mut held = vec![(7, "text".to_string())];
    assert_eq!(
        release_pastes(&mut held, &[egui::Event::Text("a".into())], 7),
        ["text"]
    );
}

/// A frame that took its input past `GpuState::take_egui_input` would apply a paste without the
/// focus check.
#[test]
fn every_egui_frame_takes_its_input_through_the_paste_check() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut callers = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("src is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && std::fs::read_to_string(&path)
                    .expect("source is readable")
                    .contains(concat!("egui_state.", "take_egui_input("))
            {
                callers.push(path.strip_prefix(&root).expect("under src").to_path_buf());
            }
        }
    }
    assert_eq!(
        callers,
        [std::path::PathBuf::from("gfx/gpu/text_selection.rs")]
    );
}
