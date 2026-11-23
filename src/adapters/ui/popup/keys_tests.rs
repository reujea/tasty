use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// 한 프레임에 사건을 넣고 `read`가 입력을 읽은 값을 돌려준다.
fn with_input<R>(
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
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

/// 기본 조작 전부. `egui_key`의 갈래가 빠짐없음을 컴파일러가 본다.
const ALL: [BasicKey; 9] = [
    BasicKey::Escape,
    BasicKey::Confirm,
    BasicKey::Expand,
    BasicKey::Up,
    BasicKey::Down,
    BasicKey::Left,
    BasicKey::Right,
    BasicKey::Tab,
    BasicKey::BackTab,
];

/// 이번 프레임에 `basic`의 고정 바인딩이 눌렸는지.
fn pressed(basic: BasicKey, input: &egui::InputState) -> bool {
    tasty_key_match::any_binding_pressed_egui(&basic.owned_bindings(), input, false)
}

/// 이번 프레임의 `basic` 누름을 입력에서 지우고, 있었는지 돌려준다.
fn consume(basic: BasicKey, input: &mut egui::InputState) -> bool {
    tasty_key_match::consume_binding_egui(&basic.owned_bindings(), input, false)
}

/// winit 키 사건이 `basic`의 고정 바인딩인지.
fn matches(basic: BasicKey, key: &Key, mods: ModifiersState) -> bool {
    tasty_key_match::matches_any_binding(&basic.owned_bindings(), key, mods)
}

fn egui_key(basic: BasicKey) -> (egui::Key, egui::Modifiers) {
    match basic {
        BasicKey::Escape => (egui::Key::Escape, egui::Modifiers::NONE),
        BasicKey::Confirm | BasicKey::Expand => (egui::Key::Enter, egui::Modifiers::NONE),
        BasicKey::Up => (egui::Key::ArrowUp, egui::Modifiers::NONE),
        BasicKey::Down => (egui::Key::ArrowDown, egui::Modifiers::NONE),
        BasicKey::Left => (egui::Key::ArrowLeft, egui::Modifiers::NONE),
        BasicKey::Right => (egui::Key::ArrowRight, egui::Modifiers::NONE),
        BasicKey::Tab => (egui::Key::Tab, egui::Modifiers::NONE),
        BasicKey::BackTab => (egui::Key::Tab, egui::Modifiers::SHIFT),
    }
}

#[test]
fn every_basic_key_is_pressed_by_its_own_key() {
    for basic in ALL {
        let (key, mods) = egui_key(basic);
        let hit = with_input(vec![press(key, mods)], mods, |ctx| {
            ctx.input(|i| pressed(basic, i))
        });
        assert!(hit, "{basic:?}");
    }
}

#[test]
fn a_basic_key_is_not_pressed_by_another_key() {
    let hit = with_input(
        vec![press(egui::Key::A, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| ctx.input(|i| ALL.iter().any(|b| pressed(*b, i))),
    );
    assert!(!hit);
}

#[test]
fn space_expands_but_does_not_confirm() {
    let (expand, confirm) = with_input(
        vec![press(egui::Key::Space, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| ctx.input(|i| (pressed(BasicKey::Expand, i), pressed(BasicKey::Confirm, i))),
    );
    assert!(expand);
    assert!(!confirm);
}

/// 검색 띠는 Shift+Enter를 이전 결과로 쓴다. 확정 바인딩과 겹치면 안 된다.
#[test]
fn a_modifier_keeps_the_key_from_matching() {
    for mods in [
        egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
    ] {
        let hit = with_input(vec![press(egui::Key::Enter, mods)], mods, |ctx| {
            ctx.input(|i| pressed(BasicKey::Confirm, i) || pressed(BasicKey::Escape, i))
        });
        assert!(!hit, "{mods:?}");
    }
    let tab = with_input(
        vec![press(egui::Key::Tab, egui::Modifiers::SHIFT)],
        egui::Modifiers::SHIFT,
        |ctx| ctx.input(|i| pressed(BasicKey::Tab, i)),
    );
    assert!(!tab, "Shift+Tab 은 앞 칸이다");
}

#[test]
fn consuming_removes_the_press_for_later_readers() {
    let (first, second) = with_input(
        vec![press(egui::Key::Escape, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| {
            let first = ctx.input_mut(|i| consume(BasicKey::Escape, i));
            let second = ctx.input(|i| pressed(BasicKey::Escape, i));
            (first, second)
        },
    );
    assert!(first);
    assert!(!second);
}

#[test]
fn winit_keys_match_the_same_bindings() {
    let none = ModifiersState::empty();
    assert!(matches(
        BasicKey::Escape,
        &Key::Named(NamedKey::Escape),
        none
    ));
    assert!(matches(
        BasicKey::Confirm,
        &Key::Named(NamedKey::Enter),
        none
    ));
    assert!(matches(
        BasicKey::Expand,
        &Key::Named(NamedKey::Space),
        none
    ));
    assert!(matches(BasicKey::Up, &Key::Named(NamedKey::ArrowUp), none));
    assert!(matches(
        BasicKey::BackTab,
        &Key::Named(NamedKey::Tab),
        ModifiersState::SHIFT
    ));
    assert!(!matches(
        BasicKey::Tab,
        &Key::Named(NamedKey::Tab),
        ModifiersState::SHIFT
    ));
    assert!(!matches(
        BasicKey::Escape,
        &Key::Named(NamedKey::Escape),
        ModifiersState::CONTROL
    ));
}

#[test]
fn every_binding_parses() {
    for basic in ALL {
        for b in basic.bindings() {
            assert!(
                tasty_key_match::parse_binding(b).is_some(),
                "{basic:?}: {b}"
            );
        }
    }
}
