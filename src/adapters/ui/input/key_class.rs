//! 키 사건 하나의 경로 분류(계약 E0 단계 4의 입력). 경로 키(egui 키가 있는 누름이고 설정 열기 요청 래치 중·
//! 클립보드 조합·편집 바인딩 키가 아닌 것. bare Esc 는 언제나 경로 키)만 포커스 노드에서 시작한 키 경로(`route_key`)로 수신자를
//! 정하고, 나머지 사건은 그 입구의 지금 경로를 그대로 지난다. egui 키와 클립보드 조합 판정은 egui-winit이 같은
//! 사건을 egui 사건으로 바꾸는 규칙과 같다(시험이 잠금 파일의 egui-winit 소스와 갈래를 대조한다). 규칙:
//! `docs/architecture/input-layer.md`의 "경로 키".

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

use super::key_route::RouteKey;

/// 키 사건 하나의 값. `mods`는 사건 시점 창의 winit 수식키, `egui_mods`는 그 사건을 egui 사건으로 바꿀 egui-winit 이
/// 그 창에 가진 수식키다(클립보드 조합 판정의 입력). 둘은 출처가 달라 갈릴 수 있다(창 포커스를 잃을 때 창 값만
/// 비운다).
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeyEventValue<'a> {
    pub(crate) logical: &'a Key,
    pub(crate) physical: PhysicalKey,
    pub(crate) pressed: bool,
    pub(crate) mods: ModifiersState,
    pub(crate) egui_mods: egui::Modifiers,
}

/// 지금 경로로 남는 사건의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CurrentReason {
    Release,
    /// 설정 열기 요청 래치 중.
    Latched,
    /// egui 키가 없는 누름(수식키 단독, egui 키 없이 글자만 내는 키).
    NoEguiKey,
    /// egui-winit이 `Event::Cut`·`Copy`·`Paste`로 바꾸는 누름.
    Clipboard,
    /// 창 편집 바인딩(복사·잘라내기·붙여넣기)에 맞는 누름.
    EditBinding,
}

/// 키 사건의 분류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyClass {
    Path(RouteKey),
    Current(CurrentReason),
}

/// 키 사건을 가른다. `latched`는 설정 열기 요청 래치, `edit_binding`은 그 누름이 편집 바인딩 키인지(재생하는
/// 원래 키는 보류 때 붙은 편집 표시로 정한다)다.
pub(crate) fn classify_key(
    value: &KeyEventValue<'_>,
    latched: bool,
    edit_binding: bool,
) -> KeyClass {
    let current = KeyClass::Current;
    if !value.pressed {
        return current(CurrentReason::Release);
    }
    if latched {
        return current(CurrentReason::Latched);
    }
    let Some(key) = egui_key(value.logical, value.physical) else {
        return current(CurrentReason::NoEguiKey);
    };
    let route = RouteKey {
        key,
        modifiers: egui_modifiers(value.mods),
        super_held: !cfg!(target_os = "macos") && value.mods.super_key(),
    };
    // bare Esc 는 클립보드·편집 바인딩보다 먼저 경로 키다(편집 바인딩과 겹쳐도 Esc 경로로 간다).
    if route.bare_escape() {
        return KeyClass::Path(route);
    }
    if is_clipboard_combo(key, value.egui_mods) {
        return current(CurrentReason::Clipboard);
    }
    if edit_binding {
        return current(CurrentReason::EditBinding);
    }
    KeyClass::Path(route)
}

/// 이 사건이 수식키 없는 Esc 누름인지(`RouteKey::bare_escape`). 래치와 무관하게 사건 값으로만 정한다. 입력 몫을 끊는
/// 조건 8 이 읽는다.
pub(crate) fn is_bare_escape_press(value: &KeyEventValue<'_>) -> bool {
    value.pressed
        && egui_key(value.logical, value.physical).is_some_and(|key| {
            RouteKey {
                key,
                modifiers: egui_modifiers(value.mods),
                super_held: !cfg!(target_os = "macos") && value.mods.super_key(),
            }
            .bare_escape()
        })
}

/// egui-winit이 이 키 사건에 싣는 egui 키. 논리 키가 먼저이고 없으면 물리 키다.
pub(crate) fn egui_key(logical: &Key, physical: PhysicalKey) -> Option<egui::Key> {
    let logical = match logical {
        Key::Named(named) => key_from_named_key(*named),
        Key::Character(text) => egui::Key::from_name(text.as_str()),
        Key::Unidentified(_) | Key::Dead(_) => None,
    };
    let physical = match physical {
        PhysicalKey::Code(code) => key_from_key_code(code),
        PhysicalKey::Unidentified(_) => None,
    };
    logical.or(physical)
}

/// egui-winit이 수식키 사건에서 정하는 egui 수식키.
pub(crate) fn egui_modifiers(mods: ModifiersState) -> egui::Modifiers {
    let mac = cfg!(target_os = "macos");
    egui::Modifiers {
        alt: mods.alt_key(),
        ctrl: mods.control_key(),
        shift: mods.shift_key(),
        mac_cmd: mac && mods.super_key(),
        command: if mac {
            mods.super_key()
        } else {
            mods.control_key()
        },
    }
}

/// egui-winit이 `Event::Cut`·`Copy`·`Paste`로 바꾸는(또는 빈 클립보드면 사건을 남기지 않는) 누름.
pub(crate) fn is_clipboard_combo(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    is_cut_command(modifiers, key)
        || is_copy_command(modifiers, key)
        || is_paste_command(modifiers, key)
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Cut
        || (modifiers.command && keycode == egui::Key::X)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Copy
        || (modifiers.command && keycode == egui::Key::C)
        || (cfg!(target_os = "windows") && modifiers.ctrl && keycode == egui::Key::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Paste
        || (modifiers.command && keycode == egui::Key::V)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Insert)
}

fn key_from_named_key(named_key: NamedKey) -> Option<egui::Key> {
    use egui::Key;

    Some(match named_key {
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::End => Key::End,
        NamedKey::Home => Key::Home,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::Escape => Key::Escape,
        NamedKey::Cut => Key::Cut,
        NamedKey::Copy => Key::Copy,
        NamedKey::Paste => Key::Paste,
        NamedKey::Space => Key::Space,
        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        NamedKey::F13 => Key::F13,
        NamedKey::F14 => Key::F14,
        NamedKey::F15 => Key::F15,
        NamedKey::F16 => Key::F16,
        NamedKey::F17 => Key::F17,
        NamedKey::F18 => Key::F18,
        NamedKey::F19 => Key::F19,
        NamedKey::F20 => Key::F20,
        NamedKey::F21 => Key::F21,
        NamedKey::F22 => Key::F22,
        NamedKey::F23 => Key::F23,
        NamedKey::F24 => Key::F24,
        NamedKey::F25 => Key::F25,
        NamedKey::F26 => Key::F26,
        NamedKey::F27 => Key::F27,
        NamedKey::F28 => Key::F28,
        NamedKey::F29 => Key::F29,
        NamedKey::F30 => Key::F30,
        NamedKey::F31 => Key::F31,
        NamedKey::F32 => Key::F32,
        NamedKey::F33 => Key::F33,
        NamedKey::F34 => Key::F34,
        NamedKey::F35 => Key::F35,
        _ => return None,
    })
}

fn key_from_key_code(key: KeyCode) -> Option<egui::Key> {
    use egui::Key;

    Some(match key {
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Space => Key::Space,
        KeyCode::Comma => Key::Comma,
        KeyCode::Period => Key::Period,
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => Key::Slash,
        KeyCode::BracketLeft => Key::OpenBracket,
        KeyCode::BracketRight => Key::CloseBracket,
        KeyCode::Backquote => Key::Backtick,
        KeyCode::Quote => Key::Quote,
        KeyCode::Cut => Key::Cut,
        KeyCode::Copy => Key::Copy,
        KeyCode::Paste => Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
        KeyCode::NumpadAdd => Key::Plus,
        KeyCode::Equal => Key::Equals,
        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,
        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,
        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::F25 => Key::F25,
        KeyCode::F26 => Key::F26,
        KeyCode::F27 => Key::F27,
        KeyCode::F28 => Key::F28,
        KeyCode::F29 => Key::F29,
        KeyCode::F30 => Key::F30,
        KeyCode::F31 => Key::F31,
        KeyCode::F32 => Key::F32,
        KeyCode::F33 => Key::F33,
        KeyCode::F34 => Key::F34,
        KeyCode::F35 => Key::F35,
        _ => return None,
    })
}

/// 시험 대역용: egui 키를 창이 받는 winit 논리 키·물리 키로 되돌린다(미국 배열). 표에 없는 키는 `None`이다.
#[cfg(test)]
pub(crate) fn winit_value_of(key: egui::Key) -> Option<(Key, PhysicalKey)> {
    use egui::Key as E;
    let ch = |s: &str| Key::Character(s.into());
    let named = Key::Named;
    let (logical, code) = match key {
        E::A => (ch("a"), KeyCode::KeyA),
        E::B => (ch("b"), KeyCode::KeyB),
        E::C => (ch("c"), KeyCode::KeyC),
        E::D => (ch("d"), KeyCode::KeyD),
        E::E => (ch("e"), KeyCode::KeyE),
        E::F => (ch("f"), KeyCode::KeyF),
        E::G => (ch("g"), KeyCode::KeyG),
        E::H => (ch("h"), KeyCode::KeyH),
        E::I => (ch("i"), KeyCode::KeyI),
        E::J => (ch("j"), KeyCode::KeyJ),
        E::K => (ch("k"), KeyCode::KeyK),
        E::L => (ch("l"), KeyCode::KeyL),
        E::M => (ch("m"), KeyCode::KeyM),
        E::N => (ch("n"), KeyCode::KeyN),
        E::O => (ch("o"), KeyCode::KeyO),
        E::P => (ch("p"), KeyCode::KeyP),
        E::Q => (ch("q"), KeyCode::KeyQ),
        E::R => (ch("r"), KeyCode::KeyR),
        E::S => (ch("s"), KeyCode::KeyS),
        E::T => (ch("t"), KeyCode::KeyT),
        E::U => (ch("u"), KeyCode::KeyU),
        E::V => (ch("v"), KeyCode::KeyV),
        E::W => (ch("w"), KeyCode::KeyW),
        E::X => (ch("x"), KeyCode::KeyX),
        E::Y => (ch("y"), KeyCode::KeyY),
        E::Z => (ch("z"), KeyCode::KeyZ),
        E::Num0 => (ch("0"), KeyCode::Digit0),
        E::Num1 => (ch("1"), KeyCode::Digit1),
        E::Num2 => (ch("2"), KeyCode::Digit2),
        E::Num3 => (ch("3"), KeyCode::Digit3),
        E::Num4 => (ch("4"), KeyCode::Digit4),
        E::Num5 => (ch("5"), KeyCode::Digit5),
        E::Num6 => (ch("6"), KeyCode::Digit6),
        E::Num7 => (ch("7"), KeyCode::Digit7),
        E::Num8 => (ch("8"), KeyCode::Digit8),
        E::Num9 => (ch("9"), KeyCode::Digit9),
        E::F1 => (named(NamedKey::F1), KeyCode::F1),
        E::F2 => (named(NamedKey::F2), KeyCode::F2),
        E::F3 => (named(NamedKey::F3), KeyCode::F3),
        E::F4 => (named(NamedKey::F4), KeyCode::F4),
        E::F5 => (named(NamedKey::F5), KeyCode::F5),
        E::F6 => (named(NamedKey::F6), KeyCode::F6),
        E::F7 => (named(NamedKey::F7), KeyCode::F7),
        E::F8 => (named(NamedKey::F8), KeyCode::F8),
        E::F9 => (named(NamedKey::F9), KeyCode::F9),
        E::F10 => (named(NamedKey::F10), KeyCode::F10),
        E::F11 => (named(NamedKey::F11), KeyCode::F11),
        E::F12 => (named(NamedKey::F12), KeyCode::F12),
        E::ArrowUp => (named(NamedKey::ArrowUp), KeyCode::ArrowUp),
        E::ArrowDown => (named(NamedKey::ArrowDown), KeyCode::ArrowDown),
        E::ArrowLeft => (named(NamedKey::ArrowLeft), KeyCode::ArrowLeft),
        E::ArrowRight => (named(NamedKey::ArrowRight), KeyCode::ArrowRight),
        E::Enter => (named(NamedKey::Enter), KeyCode::Enter),
        E::Escape => (named(NamedKey::Escape), KeyCode::Escape),
        E::Tab => (named(NamedKey::Tab), KeyCode::Tab),
        E::Space => (named(NamedKey::Space), KeyCode::Space),
        E::Backspace => (named(NamedKey::Backspace), KeyCode::Backspace),
        E::Delete => (named(NamedKey::Delete), KeyCode::Delete),
        E::Insert => (named(NamedKey::Insert), KeyCode::Insert),
        E::Home => (named(NamedKey::Home), KeyCode::Home),
        E::End => (named(NamedKey::End), KeyCode::End),
        E::PageUp => (named(NamedKey::PageUp), KeyCode::PageUp),
        E::PageDown => (named(NamedKey::PageDown), KeyCode::PageDown),
        _ => return None,
    };
    Some((logical, PhysicalKey::Code(code)))
}

/// 시험 대역용: egui 수식키를 창의 winit 수식키로 되돌린다. `command`는 Linux·Windows Ctrl, macOS Cmd(Super)다.
/// `super_held`는 비-macOS 실물 Win·Super 다.
#[cfg(test)]
pub(crate) fn winit_mods_of(m: egui::Modifiers, super_held: bool) -> ModifiersState {
    let mut out = ModifiersState::empty();
    let mac = cfg!(target_os = "macos");
    if m.ctrl || (m.command && !mac) {
        out |= ModifiersState::CONTROL;
    }
    if m.shift {
        out |= ModifiersState::SHIFT;
    }
    if m.alt {
        out |= ModifiersState::ALT;
    }
    if m.mac_cmd || (m.command && mac) || (super_held && !mac) {
        out |= ModifiersState::SUPER;
    }
    out
}

#[cfg(test)]
#[path = "key_class_tests.rs"]
mod tests;
