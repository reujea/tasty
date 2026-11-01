//! Native key conversion shared by the platform callbacks.

/// extended 키는 0xE000과 scancode를 합쳐 winit 물리 키로 변환한다.
pub(super) fn win32_scancode_to_physical(
    scancode: u32,
    extended: bool,
) -> winit::keyboard::PhysicalKey {
    use winit::platform::scancode::PhysicalKeyExtScancode;
    let ex = (scancode & 0xff) | if extended { 0xe000 } else { 0 };
    winit::keyboard::PhysicalKey::from_scancode(ex)
}

/// host 메시지 루프에서 본 key-down 하나. lParam의 scan code(16~23비트), extended(24비트),
/// 직전 눌림(30비트, 자동 반복)을 풀어 둔다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyDownMessage {
    pub vk: u32,
    pub scan: u32,
    pub extended: bool,
    pub repeat: bool,
}

/// WM_KEYDOWN·WM_SYSKEYDOWN만 풀어 돌려준다. key-up과 문자 메시지는 None이다.
pub(super) fn key_down_message(
    message: u32,
    wparam: usize,
    lparam: isize,
) -> Option<KeyDownMessage> {
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    if message != WM_KEYDOWN && message != WM_SYSKEYDOWN {
        return None;
    }
    let l = lparam as u32;
    Some(KeyDownMessage {
        vk: (wparam & 0xffff) as u32,
        scan: (l >> 16) & 0xff,
        extended: l & (1 << 24) != 0,
        repeat: l & (1 << 30) != 0,
    })
}

/// callback 인자에 없는 modifier 상태를 현재 스레드의 키보드 상태에서 읽는다.
pub(super) fn current_winit_mods() -> winit::keyboard::ModifiersState {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use winit::keyboard::ModifiersState;
    let down = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| -> bool {
        // SAFETY: GetKeyState 는 호출 스레드의 키보드 상태만 읽는 순수 조회 API 다.
        (unsafe { GetKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
    };
    let mut mods = ModifiersState::empty();
    mods.set(ModifiersState::CONTROL, down(VK_CONTROL));
    mods.set(ModifiersState::SHIFT, down(VK_SHIFT));
    mods.set(ModifiersState::ALT, down(VK_MENU));
    mods.set(ModifiersState::SUPER, down(VK_LWIN) || down(VK_RWIN));
    mods
}

/// 알려진 named key와 문자·숫자·기호만 변환한다. 나머지는 None이다.
pub(super) fn vk_to_winit_key(vk: u32) -> Option<winit::keyboard::Key> {
    use winit::keyboard::{Key, NamedKey};
    let named = match vk as u16 {
        0x09 => NamedKey::Tab,
        0x0D => NamedKey::Enter,
        0x08 => NamedKey::Backspace,
        0x2E => NamedKey::Delete,
        0x2D => NamedKey::Insert,
        0x24 => NamedKey::Home,
        0x23 => NamedKey::End,
        0x21 => NamedKey::PageUp,
        0x22 => NamedKey::PageDown,
        0x26 => NamedKey::ArrowUp,
        0x28 => NamedKey::ArrowDown,
        0x25 => NamedKey::ArrowLeft,
        0x27 => NamedKey::ArrowRight,
        0x1B => NamedKey::Escape,
        0x20 => NamedKey::Space,
        0x70 => NamedKey::F1,
        0x71 => NamedKey::F2,
        0x72 => NamedKey::F3,
        0x73 => NamedKey::F4,
        0x74 => NamedKey::F5,
        0x75 => NamedKey::F6,
        0x76 => NamedKey::F7,
        0x77 => NamedKey::F8,
        0x78 => NamedKey::F9,
        0x79 => NamedKey::F10,
        0x7A => NamedKey::F11,
        0x7B => NamedKey::F12,
        0x7C => NamedKey::F13,
        0x7D => NamedKey::F14,
        0x7E => NamedKey::F15,
        0x7F => NamedKey::F16,
        0x80 => NamedKey::F17,
        0x81 => NamedKey::F18,
        0x82 => NamedKey::F19,
        0x83 => NamedKey::F20,
        0x84 => NamedKey::F21,
        0x85 => NamedKey::F22,
        0x86 => NamedKey::F23,
        0x87 => NamedKey::F24,
        c @ 0x41..=0x5A => {
            return Some(Key::Character(((c as u8 + 32) as char).to_string().into()));
        }
        c @ 0x30..=0x39 => return Some(Key::Character(((c as u8) as char).to_string().into())),
        c @ 0x60..=0x69 => {
            return Some(Key::Character(
                ((c as u8 - 0x60 + b'0') as char).to_string().into(),
            ));
        }
        0xBB | 0x6B => return Some(Key::Character("=".into())), // OEM_PLUS / numpad ADD
        0xBD | 0x6D => return Some(Key::Character("-".into())), // OEM_MINUS / numpad SUBTRACT
        0xBC => return Some(Key::Character(",".into())),
        0xBE => return Some(Key::Character(".".into())),
        0xBF => return Some(Key::Character("/".into())),
        0xC0 => return Some(Key::Character("`".into())),
        0xDB => return Some(Key::Character("[".into())),
        0xDC => return Some(Key::Character("\\".into())),
        0xDD => return Some(Key::Character("]".into())),
        0xDE => return Some(Key::Character("'".into())),
        0xBA => return Some(Key::Character(";".into())),
        _ => return None,
    };
    Some(Key::Named(named))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_down_message_reads_scan_extended_and_repeat() {
        // Q 첫 누름: scan 0x10, 반복 아님.
        assert_eq!(
            key_down_message(0x0100, 0x51, 0x0010_0001),
            Some(KeyDownMessage {
                vk: 0x51,
                scan: 0x10,
                extended: false,
                repeat: false
            })
        );
        // 자동 반복(30비트).
        assert_eq!(
            key_down_message(0x0100, 0x51, 0x4010_0001).map(|m| m.repeat),
            Some(true)
        );
        // Alt 조합은 WM_SYSKEYDOWN, extended 화살표(24비트).
        assert_eq!(
            key_down_message(0x0104, 0x25, 0x214B_0001),
            Some(KeyDownMessage {
                vk: 0x25,
                scan: 0x4B,
                extended: true,
                repeat: false
            })
        );
        // 32비트 lParam을 부호 확장한 key-up 값(winit이 넘기는 isize)도 key-down이 아니다.
        assert_eq!(
            key_down_message(0x0101, 0x51, 0xffff_ffff_c010_0001_u64 as isize),
            None
        );
        // WM_CHAR·WM_SYSKEYUP도 건너뛴다.
        assert_eq!(key_down_message(0x0102, 0x71, 0x0010_0001), None);
        assert_eq!(key_down_message(0x0105, 0x51, 0x0010_0001), None);
    }

    #[test]
    fn ime_process_key_is_not_converted() {
        // IME가 처리하는 키는 VK_PROCESSKEY(0xE5)로 오고 변환표 밖이라 host가 판정하지 않는다.
        assert_eq!(vk_to_winit_key(0xE5), None);
    }
}
