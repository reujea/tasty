//! The egui side of the native Wayland clipboard: which keys paste and which output copies.
use super::{HeldPastes, hold_copied_text, moves_focus, paste_key, pastes_for, take_copied_text};
use std::time::{Duration, Instant};
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

/// Given egui output with a copy, When it is applied on any OS, Then the copy leaves the output
/// (egui-winit never writes it) and waits for the window's owner; a later frame without copies
/// keeps it and a newer copy replaces it.
#[test]
fn every_egui_copy_is_held_for_the_copy_door() {
    let mut held = None;
    let mut output = egui::PlatformOutput::default();
    output
        .commands
        .push(egui::OutputCommand::CopyText("field".into()));
    let output = hold_copied_text(&mut held, output);
    assert!(output.commands.is_empty());
    assert_eq!(held.as_deref(), Some("field"));
    let quiet = hold_copied_text(&mut held, egui::PlatformOutput::default());
    assert!(quiet.commands.is_empty());
    assert_eq!(held.as_deref(), Some("field"));
    let mut newer = egui::PlatformOutput::default();
    newer
        .commands
        .push(egui::OutputCommand::CopyText("label".into()));
    let newer = hold_copied_text(&mut held, newer);
    assert!(newer.commands.is_empty());
    assert_eq!(held.as_deref(), Some("label"));
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
fn shift_tab() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
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

fn held(now: Instant) -> HeldPastes {
    let mut held = HeldPastes::default();
    held.add(vec![(7, "text".to_string())], now);
    held
}

#[test]
fn a_paste_waits_out_a_frame_whose_input_can_move_focus() {
    let now = Instant::now();
    for moving in [click(), key(egui::Key::Tab, true), shift_tab()] {
        // Neither the frame that moves focus nor the next one applies it: egui may still be
        // moving focus at the start of that next frame.
        let mut h = held(now);
        assert!(h.frame(std::slice::from_ref(&moving), 7, now).is_empty());
        assert!(h.frame(&[], 7, now).is_empty());
        assert!(!h.is_empty());
        // Focus ended on another widget: dropped.
        assert!(h.frame(&[], 9, now).is_empty());
        assert!(h.is_empty());

        let mut h = held(now);
        h.frame(std::slice::from_ref(&moving), 7, now);
        h.frame(&[], 7, now);
        // Focus stayed on the widget that asked: applied.
        assert_eq!(h.frame(&[], 7, now), ["text"]);
        assert!(h.is_empty());
    }
}

#[test]
fn a_paste_that_arrives_right_after_focus_input_still_waits() {
    let now = Instant::now();
    let mut h = HeldPastes::default();
    h.frame(&[shift_tab()], 7, now);
    h.add(vec![(7, "text".to_string())], now);
    assert!(h.frame(&[], 7, now).is_empty());
    assert_eq!(h.frame(&[], 7, now), ["text"]);
}

#[test]
fn a_paste_held_while_focus_keeps_moving_expires_at_the_paste_limit() {
    let now = Instant::now();
    let mut h = held(now);
    let limit = crate::file::drag::PASTE_LIMIT;
    assert!(
        h.frame(&[click()], 7, now + limit - Duration::from_millis(1))
            .is_empty()
    );
    assert!(!h.is_empty());
    assert!(h.frame(&[click()], 7, now + limit).is_empty());
    assert!(h.is_empty());
}

#[test]
fn only_input_egui_reads_for_focus_holds_a_paste() {
    for event in [
        click(),
        key(egui::Key::Tab, true),
        shift_tab(),
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
    let now = Instant::now();
    let mut h = held(now);
    assert_eq!(h.frame(&[egui::Event::Text("a".into())], 7, now), ["text"]);
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

#[path = "text_selection_frame_tests.rs"]
mod frames;
