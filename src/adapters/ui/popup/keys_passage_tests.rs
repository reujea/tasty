use super::*;
use crate::adapters::ui::popup::PopupKeyboard;

fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// 한 프레임에 사건을 넣고 그 프레임 안에서 `read`를 부른다.
fn frame<R>(events: Vec<egui::Event>, mut read: impl FnMut(&egui::Context) -> R) -> R {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut out = None;
    let _output = ctx.run(raw, |ctx| out = Some(read(ctx)));
    out.expect("frame ran")
}

fn mixed_input() -> Vec<egui::Event> {
    vec![
        press(egui::Key::Escape, egui::Modifiers::NONE),
        press(egui::Key::Enter, egui::Modifiers::NONE),
        press(egui::Key::ArrowDown, egui::Modifiers::NONE),
        press(egui::Key::Y, egui::Modifiers::NONE),
        egui::Event::Text("y".into()),
    ]
}

#[test]
fn a_popup_that_does_not_receive_keys_sees_nothing() {
    for keyboard in [
        PopupKeyboard::None,
        PopupKeyboard::Keys,
        PopupKeyboard::Text,
    ] {
        let seen = frame(mixed_input(), |ctx| {
            let mut keys = PopupKeys::new(ctx, false, keyboard);
            (
                keys.receives(),
                BasicKey::ALL.iter().any(|b| keys.pressed(*b)),
                keys.consume(BasicKey::Escape),
                keys.consume_key(egui::Modifiers::NONE, egui::Key::Y),
                keys.text(),
                keys.consumed().len(),
            )
        });
        assert_eq!(
            seen,
            (false, false, false, false, String::new(), 0),
            "{keyboard:?}"
        );
    }
}

#[test]
fn a_none_declaration_receives_nothing_even_as_owner() {
    let seen = frame(mixed_input(), |ctx| {
        let keys = PopupKeys::new(ctx, true, PopupKeyboard::None);
        (keys.receives(), keys.pressed(BasicKey::Escape), keys.text())
    });
    assert_eq!(seen, (false, false, String::new()));
}

#[test]
fn a_receiving_popup_sees_its_keys() {
    let seen = frame(mixed_input(), |ctx| {
        let keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys);
        (
            keys.pressed(BasicKey::Escape),
            keys.pressed(BasicKey::Confirm),
            keys.pressed(BasicKey::Down),
            keys.pressed(BasicKey::Up),
        )
    });
    assert_eq!(seen, (true, true, true, false));
}

#[test]
fn consuming_records_the_key_and_hides_it_from_a_second_read() {
    let (first, second, again, consumed) = frame(mixed_input(), |ctx| {
        let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys);
        let first = keys.consume(BasicKey::Escape);
        let second = keys.consume(BasicKey::Escape);
        let again = keys.pressed(BasicKey::Escape);
        let letter = keys.consume_key(egui::Modifiers::NONE, egui::Key::Y);
        assert!(letter);
        (first, second, again, keys.consumed().to_vec())
    });
    assert!(first);
    assert!(!second);
    assert!(!again);
    assert_eq!(
        consumed,
        vec![
            ConsumedKey::Basic(BasicKey::Escape),
            ConsumedKey::Key {
                key: egui::Key::Y,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    );
}

#[test]
fn text_reaches_only_a_text_declaration() {
    let (keys_text, text_text) = frame(mixed_input(), |ctx| {
        (
            PopupKeys::new(ctx, true, PopupKeyboard::Keys).text(),
            PopupKeys::new(ctx, true, PopupKeyboard::Text).text(),
        )
    });
    assert_eq!(keys_text, "");
    assert_eq!(text_text, "y");
}

/// Ctrl·Alt 조합은 팝업이 쓰지 않으므로 소비하지 않고 남긴다(조상으로 올라간다).
#[test]
fn a_command_combination_is_left_for_the_ancestors() {
    for mods in [egui::Modifiers::CTRL, egui::Modifiers::ALT] {
        let (hit, left) = frame(vec![press(egui::Key::Y, mods)], |ctx| {
            let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Text);
            let hit = keys.consume_key(mods, egui::Key::Y);
            let left = ctx.input(|i| i.key_pressed(egui::Key::Y));
            (hit, left)
        });
        assert!(!hit, "{mods:?}");
        assert!(left, "{mods:?}");
    }
}

/// 수식키 없는 소비는 Alt가 붙은 같은 키 누름을 지우지 않는다.
#[test]
fn an_unmodified_consume_leaves_an_alt_press() {
    let left = frame(vec![press(egui::Key::Y, egui::Modifiers::ALT)], |ctx| {
        let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys);
        assert!(!keys.consume_key(egui::Modifiers::NONE, egui::Key::Y));
        ctx.input(|i| i.key_pressed(egui::Key::Y))
    });
    assert!(left);
}

#[test]
fn the_consumption_sets_follow_the_declaration() {
    use egui::{Key, Modifiers};
    let cases = [
        // (선언, 키, 수식키, 소비)
        (PopupKeyboard::None, Key::Escape, Modifiers::NONE, false),
        (PopupKeyboard::Keys, Key::Escape, Modifiers::NONE, true),
        (PopupKeyboard::Keys, Key::Y, Modifiers::NONE, true),
        (PopupKeyboard::Keys, Key::Tab, Modifiers::SHIFT, true),
        (PopupKeyboard::Keys, Key::Y, Modifiers::SHIFT, false),
        (PopupKeyboard::Keys, Key::T, Modifiers::CTRL, false),
        (PopupKeyboard::Text, Key::Y, Modifiers::SHIFT, true),
        (PopupKeyboard::Text, Key::Enter, Modifiers::NONE, true),
        (PopupKeyboard::Text, Key::T, Modifiers::CTRL, false),
        (PopupKeyboard::Text, Key::T, Modifiers::ALT, false),
        (PopupKeyboard::Text, Key::T, Modifiers::MAC_CMD, false),
    ];
    for (keyboard, key, mods, want) in cases {
        assert_eq!(
            consumes_key(keyboard, key, mods, false),
            want,
            "{keyboard:?} {key:?} {mods:?}"
        );
    }
    // 비-macOS의 Win·Super는 egui 수식키에 없어 따로 받는다.
    assert!(!consumes_key(
        PopupKeyboard::Text,
        Key::T,
        Modifiers::NONE,
        true
    ));
    assert!(consumes_text(PopupKeyboard::Text));
    assert!(!consumes_text(PopupKeyboard::Keys));
    assert!(!consumes_text(PopupKeyboard::None));
}

#[test]
fn the_empty_passage_gives_no_keys() {
    let seen = frame(mixed_input(), |ctx| {
        let keys = PopupKeys::none(ctx);
        (keys.receives(), keys.pressed(BasicKey::Escape))
    });
    assert_eq!(seen, (false, false));
}
