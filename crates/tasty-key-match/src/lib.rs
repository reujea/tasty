//! 바인딩 문자열을 winit/egui 키 이벤트와 대조한다. 단축키와 webview 키 전달이 같은 규칙을 쓴다.
//! 문자열 파싱은 tasty_settings::keybindings::parse가 맡고, 이 크레이트는 플랫폼별 매칭을 담당한다.

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

#[cfg(feature = "egui-input")]
use tasty_settings::keybindings::parse::ParsedBinding;
pub use tasty_settings::keybindings::parse::bindings_equivalent;
pub use tasty_settings::keybindings::parse::parse_binding;

pub fn matches_any_binding(bindings: &[String], key: &Key, mods: ModifiersState) -> bool {
    bindings.iter().any(|b| matches_binding(b, key, mods))
}

#[cfg(feature = "egui-input")]
/// egui 입력(`InputState`) 기준으로 바인딩 목록 중 하나라도 이번 프레임에 눌렸는지
/// 판정한다. winit 단축키 경로가 닿지 않는 egui 위젯(검색 바 등) 안에서
/// `KeybindingSettings` 바인딩을 그대로 매칭하기 위한 진입점.
/// egui 는 비-macOS 의 Win·Super 를 모르므로 `super_held` 는 [`super_held`] 로 구해 넘긴다.
pub fn any_binding_pressed_egui(
    bindings: &[String],
    input: &egui::InputState,
    super_held: bool,
) -> bool {
    bindings
        .iter()
        .any(|b| binding_pressed_egui(b, input, super_held))
}

#[cfg(feature = "egui-input")]
/// 단일 바인딩 문자열이 egui 입력에서 이번 프레임에 눌렸는지 판정.
fn binding_pressed_egui(binding: &str, input: &egui::InputState, super_held: bool) -> bool {
    let Some(parsed) = parse_binding(binding) else {
        return false;
    };
    if !egui_modifiers_match(&parsed, &input.modifiers, super_held) {
        return false;
    }
    match token_to_egui_key(&parsed.key.to_ascii_lowercase()) {
        Some(key) => input.key_pressed(key),
        None => false,
    }
}

#[cfg(feature = "egui-input")]
/// 이번 프레임의 키 누름 중 바인딩 목록과 맞는 것을 입력에서 지우고, 하나라도 있었는지 돌려준다.
/// 여러 줄 입력의 확정 키처럼 같은 키가 위젯 기본 동작(줄바꿈)으로 처리되면 안 될 때 위젯보다 먼저
/// 부른다. 판정은 각 이벤트가 가진 modifier 로 하며 규칙은 [`any_binding_pressed_egui`] 와 같다.
///
/// 지운 키 누름 바로 뒤의 글자 이벤트도 함께 지운다. egui-winit 은 ctrl·command 가 없는 키 누름
/// (Alt, 비-macOS 의 Super, macOS 의 Option)마다 그 키의 글자를 `Key` 바로 뒤 `Text` 로 넣으므로,
/// 남겨 두면 바인딩이 동작하면서 입력칸에 글자도 들어간다. 맞지 않은 키(AltGr 로 친 `@` 등)의
/// 글자는 건드리지 않는다.
pub fn consume_binding_egui(
    bindings: &[String],
    input: &mut egui::InputState,
    super_held: bool,
) -> bool {
    let wanted: Vec<_> = bindings
        .iter()
        .filter_map(|b| parse_binding(b))
        .filter_map(|p| Some((token_to_egui_key(&p.key.to_ascii_lowercase())?, p)))
        .collect();
    let matches = |e: &egui::Event| match e {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => wanted
            .iter()
            .any(|(k, p)| k == key && egui_modifiers_match(p, modifiers, super_held)),
        _ => false,
    };
    let mut hit = false;
    let mut i = 0;
    while i < input.events.len() {
        if !matches(&input.events[i]) {
            i += 1;
            continue;
        }
        input.events.remove(i);
        hit = true;
        if matches!(input.events.get(i), Some(egui::Event::Text(_))) {
            input.events.remove(i);
        }
    }
    hit
}

#[cfg(feature = "egui-input")]
/// 키 누름 하나(키와 그 누름의 수정자)가 바인딩 목록 가운데 하나와 맞는지 돌려준다. 입력을 바꾸지 않는다.
/// 판정은 [`consume_binding_egui`] 가 지울 `Key` 사건을 고르는 것과 같다.
pub fn binding_matches_egui(
    bindings: &[String],
    key: egui::Key,
    modifiers: egui::Modifiers,
    super_held: bool,
) -> bool {
    bindings.iter().filter_map(|b| parse_binding(b)).any(|p| {
        token_to_egui_key(&p.key.to_ascii_lowercase()) == Some(key)
            && egui_modifiers_match(&p, &modifiers, super_held)
    })
}

#[cfg(feature = "egui-input")]
/// modifier 매핑은 winit 경로(`matches_binding`)와 같은 [`token_axes_egui`] 규칙을 따른다.
fn egui_modifiers_match(
    parsed: &ParsedBinding<'_>,
    mods: &egui::Modifiers,
    super_held: bool,
) -> bool {
    let (alt, option) = token_axes_egui(mods, super_held);
    mods.ctrl == parsed.ctrl
        && mods.shift == parsed.shift
        && alt == parsed.alt
        && option == parsed.option
}

/// 저장 토큰 `alt`·`option` 에 해당하는 수정자가 눌렸는지 돌려준다. 하단 수정자 열의 위치로
/// 정한다. macOS 는 `alt` = Command(winit super), `option` = Option(winit alt)이고, 다른 OS 는
/// `alt` = Alt, `option` = 같은 위치의 Win·Super 다.
pub fn token_axes(mods: ModifiersState) -> (bool, bool) {
    if cfg!(target_os = "macos") {
        (mods.super_key(), mods.alt_key())
    } else {
        (mods.alt_key(), mods.super_key())
    }
}

#[cfg(feature = "egui-input")]
/// [`token_axes`] 의 egui 판. egui 는 비-macOS 에서 Win·Super 를 담지 않으므로(egui-winit 이
/// `mac_cmd` 를 macOS 에서만 채운다) 그 값은 `super_held` 로 받는다.
pub fn token_axes_egui(mods: &egui::Modifiers, super_held: bool) -> (bool, bool) {
    if cfg!(target_os = "macos") {
        (mods.mac_cmd, mods.alt)
    } else {
        (mods.alt, super_held)
    }
}

#[cfg(feature = "egui-input")]
fn super_held_id() -> egui::Id {
    egui::Id::new("tasty_key_match.super_held")
}

#[cfg(feature = "egui-input")]
/// winit 의 `ModifiersChanged` 마다 불러 Win·Super(macOS Command) 상태를 egui 문맥에 남긴다.
/// egui 입력 안에서 판정하는 단축키·전환 표시·드래그 반전이 [`super_held`] 로 읽는다.
pub fn note_modifiers(ctx: &egui::Context, mods: ModifiersState) {
    let held = mods.super_key();
    ctx.data_mut(|d| d.insert_temp(super_held_id(), held));
}

#[cfg(feature = "egui-input")]
/// [`note_modifiers`] 가 마지막으로 남긴 Win·Super 상태. 기록이 없으면 false 다.
/// `ctx.input` 안에서 부르면 문맥 잠금이 겹치므로 그 밖에서 구해 넘긴다.
pub fn super_held(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(super_held_id())).unwrap_or(false)
}

/// 바인딩 키 토큰(소문자)을 egui `Key` 로 변환. named/function 토큰은 명시 매핑하고,
/// 글자·숫자·기호는 egui `Key::from_name` 에 위임한다 (대문자 폴백 포함).
#[cfg(feature = "egui-input")]
fn token_to_egui_key(token: &str) -> Option<egui::Key> {
    use egui::Key;
    Some(match token {
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "escape" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "enter" => Key::Enter,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "insert" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        _ => {
            return Key::from_name(token).or_else(|| Key::from_name(&token.to_ascii_uppercase()));
        }
    })
}

/// ctrl/alt/option 중 하나 이상을 요구하는지 확인한다. shift 단독은 false다.
/// webview가 페이지에 남길 키와 호스트로 전달할 키를 구분할 때 사용한다.
pub fn binding_has_modifier(binding: &str) -> bool {
    match parse_binding(binding) {
        Some(p) => p.ctrl || p.alt || p.option,
        None => false,
    }
}

/// 키 토큰이 F1~F24 기능키인지 확인한다. modifier 는 보지 않는다. 문자·IME 입력을 만들지 않는 키라
/// webview 가 보조키 없는 조합 중 예외적으로 host 로 넘길 수 있는 범위를 정할 때 쓴다.
pub fn binding_is_function_key(binding: &str) -> bool {
    let Some(parsed) = parse_binding(binding) else {
        return false;
    };
    let token = parsed.key.to_ascii_lowercase();
    token
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
        && NAMED_KEY_TOKENS.iter().any(|(_, name)| *name == token)
}

/// Parse a binding string like "ctrl+shift+n" and check if it matches
/// the given key + modifiers. Returns false for empty bindings.
pub fn matches_binding(binding: &str, key: &Key, mods: ModifiersState) -> bool {
    let Some(parsed) = parse_binding(binding) else {
        return false;
    };

    // modifier 자체를 누른 이벤트는 단축키를 실행하지 않는다.
    if let Key::Named(n) = key
        && matches!(
            n,
            NamedKey::Control
                | NamedKey::Shift
                | NamedKey::Alt
                | NamedKey::Super
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::Fn
                | NamedKey::FnLock
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
                | NamedKey::Symbol
                | NamedKey::SymbolLock
        )
    {
        return false;
    }

    // 수정자는 정확히 같아야 한다. alt·option 축은 키 위치로 정한다(`token_axes`).
    let (alt, option) = token_axes(mods);
    if mods.control_key() != parsed.ctrl
        || mods.shift_key() != parsed.shift
        || alt != parsed.alt
        || option != parsed.option
    {
        return false;
    }

    let key_lower = parsed.key.to_ascii_lowercase();
    match key {
        Key::Character(c) => {
            let ch = c.to_lowercase();
            if key_matches_token(&ch, &key_lower) {
                return true;
            }
            // Ctrl+letter may arrive as control character (0x01-0x1A).
            // Convert back to the letter for matching.
            if parsed.ctrl && c.len() == 1 {
                let byte = c.as_bytes()[0];
                if (1..=26).contains(&byte) {
                    let letter = ((byte - 1) + b'a') as char;
                    return letter.to_string() == key_lower;
                }
            }
            false
        }
        Key::Named(named) => match named_key_to_string(named) {
            Some(named_str) => named_str == key_lower,
            None => false,
        },
        _ => false,
    }
}

/// 입력 문자(`character`, 이미 lowercase)가 바인딩 키 토큰(`token`)과 동일한 키를
/// 의미하는지 판정. `"plus"↔"+"`, `"minus"↔"-"`, `"equals"↔"="` 등 심볼 이름을
/// 양쪽 모두에서 받도록 별칭 매칭을 수행한다.
fn key_matches_token(character: &str, token: &str) -> bool {
    if character == token {
        return true;
    }
    matches!(
        (character, token),
        ("+", "plus")
            | ("plus", "+")
            | ("-", "minus")
            | ("minus", "-")
            | ("=", "equals")
            | ("equals", "=")
    )
}

/// 바인딩 키 토큰으로 쓰는 named key 이름. 매칭과 [`binding_key_recognized`] 가 같은 표를 읽는다.
const NAMED_KEY_TOKENS: &[(NamedKey, &str)] = &[
    (NamedKey::Tab, "tab"),
    (NamedKey::Space, "space"),
    (NamedKey::Enter, "enter"),
    (NamedKey::Backspace, "backspace"),
    (NamedKey::Delete, "delete"),
    (NamedKey::Insert, "insert"),
    (NamedKey::Home, "home"),
    (NamedKey::End, "end"),
    (NamedKey::PageUp, "pageup"),
    (NamedKey::PageDown, "pagedown"),
    (NamedKey::ArrowUp, "up"),
    (NamedKey::ArrowDown, "down"),
    (NamedKey::ArrowLeft, "left"),
    (NamedKey::ArrowRight, "right"),
    (NamedKey::F1, "f1"),
    (NamedKey::F2, "f2"),
    (NamedKey::F3, "f3"),
    (NamedKey::F4, "f4"),
    (NamedKey::F5, "f5"),
    (NamedKey::F6, "f6"),
    (NamedKey::F7, "f7"),
    (NamedKey::F8, "f8"),
    (NamedKey::F9, "f9"),
    (NamedKey::F10, "f10"),
    (NamedKey::F11, "f11"),
    (NamedKey::F12, "f12"),
    (NamedKey::F13, "f13"),
    (NamedKey::F14, "f14"),
    (NamedKey::F15, "f15"),
    (NamedKey::F16, "f16"),
    (NamedKey::F17, "f17"),
    (NamedKey::F18, "f18"),
    (NamedKey::F19, "f19"),
    (NamedKey::F20, "f20"),
    (NamedKey::F21, "f21"),
    (NamedKey::F22, "f22"),
    (NamedKey::F23, "f23"),
    (NamedKey::F24, "f24"),
    (NamedKey::Escape, "escape"),
];

/// 바인딩 키 토큰으로 쓰는 이름 키 이름 전부. 다른 표(OS 메뉴 key equivalent 등)가 같은 이름 집합을
/// 다루는지 시험에서 대조할 때 쓴다.
pub fn named_key_tokens() -> impl Iterator<Item = &'static str> {
    NAMED_KEY_TOKENS.iter().map(|(_, name)| *name)
}

fn named_key_to_string(key: &NamedKey) -> Option<&'static str> {
    NAMED_KEY_TOKENS
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, name)| *name)
}

/// 바인딩의 키 토큰이 어떤 키 입력과도 맞을 수 있는지. 문자 하나, 심볼 별칭(`plus`·`minus`·`equals`),
/// [`NAMED_KEY_TOKENS`] 의 이름만 받는다. `ctrl+shft+h` 처럼 modifier 를 잘못 쓰면 남은 `shft+h` 가
/// 키 토큰이 되어 어떤 입력과도 맞지 않으므로 false다. 빈 문자열과 modifier 만 있는 표기도 false다.
pub fn binding_key_recognized(binding: &str) -> bool {
    let Some(parsed) = parse_binding(binding) else {
        return false;
    };
    let token = parsed.key.to_ascii_lowercase();
    token.chars().count() == 1
        || matches!(token.as_str(), "plus" | "minus" | "equals")
        || NAMED_KEY_TOKENS.iter().any(|(_, name)| *name == token)
}

/// 텍스트로 넣은 키(설정 파일·plugin override)의 해석 문제.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKeyProblem {
    /// 저장 토큰 대신 OS 키 이름(`cmd`·`super`·`win`·`meta`)을 적었다.
    OsKeyName,
    /// 그 밖의 이유로 [`binding_key_recognized`] 가 받지 않는다.
    Unrecognized,
}

/// `keys` 중 해석하지 못하는 첫 키의 위치와 이유. OS 키 이름이 있으면 일반 해석 실패보다
/// 먼저 그 이유를 고른다 — 어떻게 적어야 하는지 안내할 수 있기 때문이다.
pub fn first_text_key_problem<S: AsRef<str>>(keys: &[S]) -> Option<(usize, TextKeyProblem)> {
    keys.iter().enumerate().find_map(|(i, k)| {
        let k = k.as_ref();
        if binding_key_recognized(k) {
            None
        } else if tasty_settings::keybindings::os_keys::uses_os_key_name(k) {
            Some((i, TextKeyProblem::OsKeyName))
        } else {
            Some((i, TextKeyProblem::Unrecognized))
        }
    })
}

/// 단축키를 찾을 때 쓸 키를 정한다. 단축키 경로(본 창·plugin·webview)가 같은 규칙을 쓴다.
///
/// - 물리 키가 F13~F24 인데 논리 키가 이름 키가 아니면 그 F 키로 본다. macOS winit 은 F21~F24 의
///   논리 키를 AppKit 사설 영역 문자(`U+F718` 등)로 올리고 물리 키만 `KeyCode::F21` 로 준다.
/// - ctrl·alt·super 가 눌렸으면 IME 가 바꾼 논리 문자 대신 물리 키의 US 배열 문자를 쓴다
///   ([`physical_key_to_logical`]).
pub fn shortcut_lookup_key(logical: &Key, physical: &PhysicalKey, mods: ModifiersState) -> Key {
    if !matches!(logical, Key::Named(_))
        && let Some(named) = physical_function_key(physical)
    {
        return Key::Named(named);
    }
    if (mods.control_key() || mods.super_key() || mods.alt_key())
        && let Some(key) = physical_key_to_logical(physical)
    {
        return key;
    }
    logical.clone()
}

/// 물리 키 F13~F24 의 이름 키. 논리 키가 그 키로 오지 않는 플랫폼의 폴백이다.
fn physical_function_key(physical: &PhysicalKey) -> Option<NamedKey> {
    let PhysicalKey::Code(code) = physical else {
        return None;
    };
    Some(match code {
        KeyCode::F13 => NamedKey::F13,
        KeyCode::F14 => NamedKey::F14,
        KeyCode::F15 => NamedKey::F15,
        KeyCode::F16 => NamedKey::F16,
        KeyCode::F17 => NamedKey::F17,
        KeyCode::F18 => NamedKey::F18,
        KeyCode::F19 => NamedKey::F19,
        KeyCode::F20 => NamedKey::F20,
        KeyCode::F21 => NamedKey::F21,
        KeyCode::F22 => NamedKey::F22,
        KeyCode::F23 => NamedKey::F23,
        KeyCode::F24 => NamedKey::F24,
        _ => return None,
    })
}

/// Convert a physical key code to a Key::Character for shortcut matching.
/// On macOS, when IME is composing (e.g. Korean), logical_key may contain
/// the composed character (e.g. "ㅇ" instead of "d"). This function extracts
/// the intended key from the physical key code.
pub fn physical_key_to_logical(physical: &PhysicalKey) -> Option<Key> {
    let code = match physical {
        PhysicalKey::Code(c) => c,
        _ => return None,
    };
    let ch: &str = match code {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Backquote => "`",
        KeyCode::Backslash => "\\",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        _ => return None,
    };
    Some(Key::Character(ch.into()))
}

#[cfg(all(test, feature = "egui-input"))]
mod egui_tests {
    use super::*;

    fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// 맞는 키 누름만 지운다. modifier 가 다르거나 바인딩에 없는 키는 위젯에 그대로 간다.
    #[test]
    fn consume_removes_only_the_matching_press() {
        let ctx = egui::Context::default();
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: !cfg!(target_os = "macos"),
            ..Default::default()
        };
        let mut hit = None;
        let mut left = Vec::new();
        let output = ctx.run(
            egui::RawInput {
                events: vec![
                    press(egui::Key::Enter, egui::Modifiers::NONE),
                    press(egui::Key::Enter, ctrl),
                    press(egui::Key::A, ctrl),
                ],
                ..Default::default()
            },
            |ctx| {
                ctx.input_mut(|i| {
                    hit = Some(consume_binding_egui(&["ctrl+enter".into()], i, false));
                    left = i.events.clone();
                });
            },
        );
        drop(output);
        assert_eq!(hit, Some(true));
        assert_eq!(
            left,
            vec![
                press(egui::Key::Enter, egui::Modifiers::NONE),
                press(egui::Key::A, ctrl)
            ]
        );
    }

    fn consume_events(bindings: &[&str], events: Vec<egui::Event>) -> (bool, Vec<egui::Event>) {
        let ctx = egui::Context::default();
        let bindings: Vec<String> = bindings.iter().map(|b| (*b).into()).collect();
        let (mut hit, mut left) = (false, Vec::new());
        let output = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                ctx.input_mut(|i| {
                    hit = consume_binding_egui(&bindings, i, false);
                    left = i.events.clone();
                });
            },
        );
        drop(output);
        (hit, left)
    }

    fn text(s: &str) -> egui::Event {
        egui::Event::Text(s.into())
    }

    /// 키 누름 하나의 판정은 `consume_binding_egui` 가 지우는 `Key` 사건과 같다.
    #[test]
    fn the_single_press_match_agrees_with_consume() {
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: !cfg!(target_os = "macos"),
            ..Default::default()
        };
        let ctrl_shift = egui::Modifiers {
            shift: true,
            ..ctrl
        };
        let bindings = ["ctrl+v", "ctrl+shift+y", "v"];
        let events = vec![
            press(egui::Key::V, ctrl),
            press(egui::Key::V, egui::Modifiers::NONE),
            press(egui::Key::V, ctrl_shift),
            press(egui::Key::Y, ctrl_shift),
            press(egui::Key::Y, ctrl),
            press(egui::Key::C, ctrl),
        ];
        let owned: Vec<String> = bindings.iter().map(|b| (*b).into()).collect();
        let mut matched = 0;
        for event in events {
            let egui::Event::Key { key, modifiers, .. } = &event else {
                unreachable!("키 누름만 넣었다");
            };
            let judged = binding_matches_egui(&owned, *key, *modifiers, false);
            let (removed, _) = consume_events(&bindings, vec![event.clone()]);
            assert_eq!(judged, removed, "{event:?}");
            matched += usize::from(judged);
        }
        assert_eq!(matched, 3);
    }

    /// 맞는 키 누름 바로 뒤의 글자도 지운다. 앞뒤로 친 다른 글자는 남는다.
    #[test]
    fn consume_also_removes_the_text_of_the_matching_press() {
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        // macOS 에서 `option` 이 egui alt 다. 다른 OS 에서는 `alt` 가 egui alt 다.
        let binding = if cfg!(target_os = "macos") {
            "option+f"
        } else {
            "alt+f"
        };
        let none = egui::Modifiers::NONE;
        let (hit, left) = consume_events(
            &[binding],
            vec![
                press(egui::Key::A, none),
                text("a"),
                press(egui::Key::F, alt),
                text("f"),
                press(egui::Key::B, none),
                text("b"),
            ],
        );
        assert!(hit);
        assert_eq!(
            left,
            vec![
                press(egui::Key::A, none),
                text("a"),
                press(egui::Key::B, none),
                text("b"),
            ]
        );
    }

    /// 맞지 않은 키의 글자는 남는다. Windows 의 AltGr 는 ctrl+alt 로 오고 그 글자(`@`)가 뒤따른다.
    /// `alt+q` 바인딩은 ctrl 이 더 눌린 그 누름과 맞지 않으므로 글자 입력이 깨지지 않는다.
    #[test]
    fn an_altgr_glyph_that_matches_no_binding_is_kept() {
        let altgr = egui::Modifiers {
            alt: true,
            ctrl: true,
            command: !cfg!(target_os = "macos"),
            ..Default::default()
        };
        let events = vec![press(egui::Key::Q, altgr), text("@")];
        let (hit, left) = consume_events(&["alt+q", "option+q"], events.clone());
        assert_eq!((hit, left), (false, events));
    }

    /// 지운 키 누름 바로 뒤가 글자가 아니면 그 뒤의 글자는 다른 키의 것이므로 남긴다.
    #[test]
    fn a_text_after_another_event_is_kept() {
        let none = egui::Modifiers::NONE;
        let events = vec![
            press(egui::Key::Enter, none),
            press(egui::Key::A, none),
            text("a"),
        ];
        let (hit, left) = consume_events(&["enter"], events);
        assert!(hit);
        assert_eq!(left, vec![press(egui::Key::A, none), text("a")]);
    }

    /// F13~F24 는 egui 키 이름으로도 찾는다(egui 경로의 단축키가 winit 경로와 같은 키를 받는다).
    #[test]
    fn high_function_keys_map_to_egui_keys() {
        assert_eq!(token_to_egui_key("f13"), Some(egui::Key::F13));
        assert_eq!(token_to_egui_key("f24"), Some(egui::Key::F24));
    }

    /// 빈 목록과 맞지 않는 바인딩은 아무것도 지우지 않는다.
    #[test]
    fn consume_without_a_match_keeps_every_event() {
        let ctx = egui::Context::default();
        let mut hit = None;
        let mut n = 0;
        let output = ctx.run(
            egui::RawInput {
                events: vec![press(egui::Key::Escape, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                ctx.input_mut(|i| {
                    hit = Some(
                        consume_binding_egui(&[], i, false)
                            || consume_binding_egui(&["ctrl+enter".into()], i, false),
                    );
                    n = i.events.len();
                });
            },
        );
        drop(output);
        assert_eq!((hit, n), (Some(false), 1));
    }

    /// egui 판정도 winit 과 같은 위치 규칙을 쓴다. 비-macOS 의 Win·Super 는 `super_held` 로 온다.
    #[test]
    fn egui_option_axis_follows_the_key_position() {
        let none = egui::Modifiers::NONE;
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        if cfg!(target_os = "macos") {
            assert_eq!(token_axes_egui(&alt, false), (false, true));
            assert_eq!(token_axes_egui(&none, true), (false, false));
        } else {
            assert_eq!(token_axes_egui(&none, true), (false, true));
            assert_eq!(token_axes_egui(&alt, false), (true, false));
        }
    }

    /// `note_modifiers` 로 남긴 Win·Super 상태를 `super_held` 가 읽고, egui 판정에 넘기면
    /// option 바인딩이 맞는다.
    #[test]
    fn noted_super_key_reaches_the_egui_option_binding() {
        let ctx = egui::Context::default();
        assert!(!super_held(&ctx));
        note_modifiers(&ctx, ModifiersState::SUPER);
        assert!(super_held(&ctx));
        let held = super_held(&ctx);
        let mut hit = None;
        let output = ctx.run(
            egui::RawInput {
                events: vec![press(egui::Key::K, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                ctx.input(|i| hit = Some(any_binding_pressed_egui(&["option+k".into()], i, held)));
            },
        );
        drop(output);
        // macOS 의 Super 는 Command(`alt`)이므로 option 바인딩에 맞지 않는다.
        assert_eq!(hit, Some(!cfg!(target_os = "macos")));
        note_modifiers(&ctx, ModifiersState::empty());
        assert!(!super_held(&ctx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_bad_key_says_whether_an_os_key_name_was_written() {
        assert_eq!(first_text_key_problem(&["ctrl+k", "option+k"]), None);
        assert_eq!(
            first_text_key_problem(&["ctrl+k", "cmd+k", "ctrl+shft+h"]),
            Some((1, TextKeyProblem::OsKeyName))
        );
        assert_eq!(
            first_text_key_problem(&["ctrl+shft+h", "super+k"]),
            Some((0, TextKeyProblem::Unrecognized))
        );
    }

    #[test]
    fn recognized_keys_are_the_ones_the_matcher_can_match() {
        for b in [
            "ctrl+shift+h",
            "ctrl+f5",
            "alt+escape",
            "ctrl+plus",
            "ctrl++",
            "option+x",
            "Ctrl+PageUp",
            "f13",
            "ctrl+F24",
        ] {
            assert!(binding_key_recognized(b), "{b}");
        }
        // OS 키 이름은 OS 마다 위치가 달라 저장 토큰으로 받지 않는다(key-mapping.md).
        for b in [
            "super+k",
            "win+k",
            "meta+k",
            "command+k",
            "ctrl+shft+h",
            "cmd+k",
            "f25",
            "ctrl+",
            "ctrl",
            "",
            "ctrl+hello",
        ] {
            assert!(!binding_key_recognized(b), "{b}");
        }
    }

    /// 비-macOS 의 Win·Super 는 `option` 이고 Alt 는 `alt` 다. macOS 는 Option 이 `option`,
    /// Command(winit super)가 `alt` 로 바뀌지 않는다.
    #[test]
    fn option_matches_the_key_at_the_option_position() {
        let k = Key::Character("k".into());
        let (option_key, alt_key) = if cfg!(target_os = "macos") {
            (ModifiersState::ALT, ModifiersState::SUPER)
        } else {
            (ModifiersState::SUPER, ModifiersState::ALT)
        };
        assert!(matches_binding("option+k", &k, option_key));
        assert!(!matches_binding("option+k", &k, alt_key));
        assert!(!matches_binding("k", &k, option_key));
        assert!(matches_binding("alt+k", &k, alt_key));
        assert!(!matches_binding("alt+k", &k, option_key));
        assert!(matches_binding(
            "ctrl+option+shift+k",
            &k,
            option_key | ModifiersState::CONTROL | ModifiersState::SHIFT
        ));
        assert_eq!(token_axes(option_key), (false, true));
        assert_eq!(token_axes(alt_key), (true, false));
    }

    #[test]
    fn high_function_keys_match_with_and_without_modifiers() {
        let f13 = Key::Named(NamedKey::F13);
        assert!(matches_binding("f13", &f13, ModifiersState::empty()));
        assert!(matches_binding("ctrl+f13", &f13, ModifiersState::CONTROL));
        assert!(!matches_binding("f13", &f13, ModifiersState::CONTROL));
        assert!(!matches_binding("f14", &f13, ModifiersState::empty()));
        assert!(matches_binding(
            "shift+F24",
            &Key::Named(NamedKey::F24),
            ModifiersState::SHIFT
        ));
    }

    /// macOS winit 이 F21 을 논리 키 `Character("\u{F718}")` + 물리 키 `F21` 로 올려도 `f21` 바인딩에 맞는다.
    #[test]
    fn a_private_use_logical_key_on_a_physical_f21_matches_f21() {
        let logical = Key::Character("\u{F718}".into());
        let physical = PhysicalKey::Code(KeyCode::F21);
        for mods in [ModifiersState::empty(), ModifiersState::CONTROL] {
            let key = shortcut_lookup_key(&logical, &physical, mods);
            assert_eq!(key, Key::Named(NamedKey::F21), "{mods:?}");
        }
        let key = shortcut_lookup_key(&logical, &physical, ModifiersState::empty());
        assert!(matches_binding("f21", &key, ModifiersState::empty()));
        let key = shortcut_lookup_key(&logical, &physical, ModifiersState::CONTROL);
        assert!(matches_binding("ctrl+f21", &key, ModifiersState::CONTROL));
        assert!(!matches_binding("f21", &logical, ModifiersState::empty()));
    }

    /// 폴백은 논리 키가 이름 키가 아닐 때만이고, 수식키 조합의 US 문자 대체는 그대로다.
    #[test]
    fn lookup_key_keeps_named_logical_keys_and_the_us_letter_fallback() {
        let named = Key::Named(NamedKey::Escape);
        let f13 = PhysicalKey::Code(KeyCode::F13);
        assert_eq!(
            shortcut_lookup_key(&named, &f13, ModifiersState::empty()),
            named
        );
        let hangul = Key::Character("ㅇ".into());
        let d = PhysicalKey::Code(KeyCode::KeyD);
        assert_eq!(
            shortcut_lookup_key(&hangul, &d, ModifiersState::CONTROL),
            Key::Character("d".into())
        );
        assert_eq!(
            shortcut_lookup_key(&hangul, &d, ModifiersState::empty()),
            hangul
        );
        for code in [KeyCode::F12, KeyCode::F25] {
            let key = Key::Character("x".into());
            assert_eq!(
                shortcut_lookup_key(&key, &PhysicalKey::Code(code), ModifiersState::empty()),
                key,
                "{code:?}"
            );
        }
    }

    #[test]
    fn every_named_token_matches_its_key() {
        for (named, name) in NAMED_KEY_TOKENS {
            assert!(
                matches_binding(name, &Key::Named(*named), ModifiersState::empty()),
                "{name}"
            );
        }
    }

    /// 기능키만 참이다. 문자·이름 키·잘못된 표기는 거짓이고 modifier 는 판정에 쓰지 않는다.
    #[test]
    fn only_f1_to_f24_count_as_function_keys() {
        for b in ["f5", "F5", "f1", "f24", "shift+f5", "ctrl+f12"] {
            assert!(binding_is_function_key(b), "{b}");
        }
        for b in ["f", "f0", "f25", "r", "ctrl+r", "enter", "escape", "", "fx"] {
            assert!(!binding_is_function_key(b), "{b}");
        }
    }
}
