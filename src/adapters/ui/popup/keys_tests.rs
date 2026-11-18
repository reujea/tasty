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
    for basic in BasicKey::ALL {
        let (key, mods) = egui_key(basic);
        let hit = with_input(vec![press(key, mods)], mods, |ctx| {
            ctx.input(|i| basic.pressed(i, false))
        });
        assert!(hit, "{basic:?}");
    }
}

#[test]
fn a_basic_key_is_not_pressed_by_another_key() {
    let hit = with_input(
        vec![press(egui::Key::A, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| ctx.input(|i| BasicKey::ALL.iter().any(|b| b.pressed(i, false))),
    );
    assert!(!hit);
}

#[test]
fn space_expands_but_does_not_confirm() {
    let (expand, confirm) = with_input(
        vec![press(egui::Key::Space, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| {
            ctx.input(|i| {
                (
                    BasicKey::Expand.pressed(i, false),
                    BasicKey::Confirm.pressed(i, false),
                )
            })
        },
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
            ctx.input(|i| BasicKey::Confirm.pressed(i, false) || BasicKey::Escape.pressed(i, false))
        });
        assert!(!hit, "{mods:?}");
    }
    let tab = with_input(
        vec![press(egui::Key::Tab, egui::Modifiers::SHIFT)],
        egui::Modifiers::SHIFT,
        |ctx| ctx.input(|i| BasicKey::Tab.pressed(i, false)),
    );
    assert!(!tab, "Shift+Tab 은 앞 칸이다");
}

#[test]
fn consuming_removes_the_press_for_later_readers() {
    let (first, second) = with_input(
        vec![press(egui::Key::Escape, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
        |ctx| {
            let first = ctx.input_mut(|i| BasicKey::Escape.consume(i, false));
            let second = ctx.input(|i| BasicKey::Escape.pressed(i, false));
            (first, second)
        },
    );
    assert!(first);
    assert!(!second);
}

#[test]
fn winit_keys_match_the_same_bindings() {
    let none = ModifiersState::empty();
    assert!(BasicKey::Escape.matches(&Key::Named(NamedKey::Escape), none));
    assert!(BasicKey::Confirm.matches(&Key::Named(NamedKey::Enter), none));
    assert!(BasicKey::Expand.matches(&Key::Named(NamedKey::Space), none));
    assert!(BasicKey::Up.matches(&Key::Named(NamedKey::ArrowUp), none));
    assert!(BasicKey::BackTab.matches(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT));
    assert!(!BasicKey::Tab.matches(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT));
    assert!(!BasicKey::Escape.matches(&Key::Named(NamedKey::Escape), ModifiersState::CONTROL));
}

#[test]
fn every_binding_parses() {
    for basic in BasicKey::ALL {
        for b in basic.bindings() {
            assert!(
                tasty_key_match::parse_binding(b).is_some(),
                "{basic:?}: {b}"
            );
        }
    }
}
