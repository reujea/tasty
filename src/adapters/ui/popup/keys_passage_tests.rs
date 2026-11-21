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
fn frame<R>(events: Vec<egui::Event>, read: impl FnMut(&egui::Context) -> R) -> R {
    frame_with(egui::Modifiers::NONE, events, read)
}

/// 프레임 수식키(`RawInput.modifiers`)를 정한 한 프레임.
fn frame_with<R>(
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
    mut read: impl FnMut(&egui::Context) -> R,
) -> R {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        events,
        modifiers,
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
            let mut keys = PopupKeys::new(ctx, false, keyboard, &EditBindings::default());
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
        let keys = PopupKeys::new(ctx, true, PopupKeyboard::None, &EditBindings::default());
        (keys.receives(), keys.pressed(BasicKey::Escape), keys.text())
    });
    assert_eq!(seen, (false, false, String::new()));
}

#[test]
fn a_receiving_popup_sees_its_keys() {
    let seen = frame(mixed_input(), |ctx| {
        let keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys, &EditBindings::default());
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
        let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys, &EditBindings::default());
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
            PopupKeys::new(ctx, true, PopupKeyboard::Keys, &EditBindings::default()).text(),
            PopupKeys::new(ctx, true, PopupKeyboard::Text, &EditBindings::default()).text(),
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
            let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Text, &EditBindings::default());
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
        let mut keys = PopupKeys::new(ctx, true, PopupKeyboard::Keys, &EditBindings::default());
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
            consumes_key(keyboard, key, mods, false, &EditBindings::default()),
            want,
            "{keyboard:?} {key:?} {mods:?}"
        );
    }
    // 비-macOS의 Win·Super는 egui 수식키에 없어 따로 받는다.
    assert!(!consumes_key(
        PopupKeyboard::Text,
        Key::T,
        Modifiers::NONE,
        true,
        &EditBindings::default()
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

/// 복사·잘라내기·붙여넣기 목록을 통째로 정한 편집 바인딩. 기본 설정에 기대지 않는다.
fn edits(copy: &[&str], cut: &[&str], paste: &[&str]) -> EditBindings {
    let own = |l: &[&str]| l.iter().map(|b| (*b).to_string()).collect::<Vec<_>>();
    let mut kb = crate::settings::KeybindingSettings::default();
    kb.copy = own(copy);
    kb.cut = own(cut);
    kb.paste = own(paste);
    EditBindings::from_settings(&kb)
}

const CTRL_SHIFT: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: false,
};

/// `Text` 팝업에서 한 번 소비하고, 그 뒤 같은 키 누름이 남았는지.
fn consume_once(
    keyboard: PopupKeyboard,
    edits: &EditBindings,
    frame_mods: egui::Modifiers,
    event_mods: egui::Modifiers,
    call_mods: egui::Modifiers,
    key: egui::Key,
) -> (bool, bool) {
    frame_with(frame_mods, vec![press(key, event_mods)], |ctx| {
        let mut keys = PopupKeys::new(ctx, true, keyboard, edits);
        let hit = keys.consume_key(call_mods, key);
        let left = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, .. } if *k == key))
        });
        (hit, left)
    })
}

/// 편집 바인딩(복사·잘라내기·붙여넣기)은 수식키가 붙어도 `Text` 팝업이 소비한다. 세 동작을 하나씩 본다.
#[test]
fn a_text_popup_consumes_each_edit_binding() {
    use egui::{Key, Modifiers};
    for (name, bindings, key) in [
        ("paste", edits(&[], &[], &["ctrl+v"]), Key::V),
        ("copy", edits(&["ctrl+c"], &[], &[]), Key::C),
        ("cut", edits(&[], &["ctrl+x"], &[]), Key::X),
    ] {
        let (hit, left) = consume_once(
            PopupKeyboard::Text,
            &bindings,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Modifiers::CTRL,
            key,
        );
        assert!(hit, "{name} 소비");
        assert!(!left, "{name} 사건이 지워짐");
    }
}

/// 소비 집합은 그때 적용된 설정의 바인딩을 따른다.
#[test]
fn a_rebound_paste_follows_the_settings() {
    use egui::{Key, Modifiers};
    let bindings = edits(&[], &[], &["ctrl+shift+y"]);
    assert_eq!(
        consume_once(
            PopupKeyboard::Text,
            &bindings,
            CTRL_SHIFT,
            CTRL_SHIFT,
            CTRL_SHIFT,
            Key::Y
        ),
        (true, false),
        "바꾼 바인딩은 소비"
    );
    assert_eq!(
        consume_once(
            PopupKeyboard::Text,
            &bindings,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Key::V
        ),
        (false, true),
        "옛 기본 붙여넣기는 소비하지 않고 남긴다"
    );
}

/// 판정은 `consume_key`가 받은 수식키로 한다. 프레임 수식키는 보지 않는다.
#[test]
fn the_edit_judgment_reads_the_given_modifiers_not_the_frame() {
    use egui::{Key, Modifiers};
    let bindings = edits(&[], &[], &["ctrl+v"]);
    assert_eq!(
        consume_once(
            PopupKeyboard::Text,
            &bindings,
            Modifiers::CTRL,
            CTRL_SHIFT,
            CTRL_SHIFT,
            Key::V
        ),
        (false, true),
        "(가) 프레임 Ctrl, 사건·호출 Ctrl+Shift+V"
    );
    assert_eq!(
        consume_once(
            PopupKeyboard::Text,
            &bindings,
            CTRL_SHIFT,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Key::V
        ),
        (true, false),
        "(나) 프레임 Ctrl+Shift, 사건·호출 Ctrl+V"
    );
}

/// 편집 바인딩이 아닌 Ctrl 조합은 `Text` 팝업도 소비하지 않는다.
#[test]
fn a_text_popup_leaves_a_command_that_is_not_an_edit_binding() {
    use egui::{Key, Modifiers};
    let bindings = edits(&[], &[], &["ctrl+v"]);
    assert_eq!(
        consume_once(
            PopupKeyboard::Text,
            &bindings,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Key::Y
        ),
        (false, true)
    );
}

/// `Keys` 팝업은 편집 조합을 먹지 않고 위로 보낸다. 수식키 없는 키를 편집 바인딩으로 저장한 경우도 같다.
#[test]
fn a_keys_popup_leaves_edit_bindings() {
    use egui::{Key, Modifiers};
    assert_eq!(
        consume_once(
            PopupKeyboard::Keys,
            &edits(&[], &[], &["ctrl+v"]),
            Modifiers::CTRL,
            Modifiers::CTRL,
            Modifiers::CTRL,
            Key::V
        ),
        (false, true),
        "Ctrl+V"
    );
    let none = Modifiers::NONE;
    assert_eq!(
        consume_once(
            PopupKeyboard::Keys,
            &edits(&[], &[], &["v"]),
            none,
            none,
            none,
            Key::V
        ),
        (false, true),
        "paste [v]"
    );
    assert_eq!(
        consume_once(
            PopupKeyboard::Keys,
            &edits(&[], &[], &["ctrl+v"]),
            none,
            none,
            none,
            Key::V
        ),
        (true, false),
        "paste [ctrl+v]이면 같은 v를 소비"
    );
}
