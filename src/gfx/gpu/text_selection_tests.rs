//! The egui side of the native Wayland clipboard: which keys paste and which output copies.
use super::{paste_key, pastes_for, take_copied_text};
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
