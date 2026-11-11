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

/// WM_KEYUP이면 그 VK를 돌려준다. 수식키와 함께 떼는 WM_SYSKEYUP은 활성화 키의 뗌이 아니다.
pub(super) fn key_up_vk(message: u32, wparam: usize) -> Option<u32> {
    const WM_KEYUP: u32 = 0x0101;
    (message == WM_KEYUP).then_some((wparam & 0xffff) as u32)
}

/// Ctrl·Alt·Super 없이 누른 글자 키의 논리 값. 이 GUI 스레드의 입력 언어와 키보드 상태(Shift·CapsLock)로
/// 현재 배열이 만드는 문자를 읽는다. 수식키 조합은 이 값을 쓰지 않고 US 물리 키 문자로 비교한다
/// (`tasty_key_match::shortcut_lookup_key`). 읽지 못하면 None이고 호출자는 VK 변환값을 쓴다.
pub(super) fn layout_character_key(vk: u32, scan: u32) -> Option<winit::keyboard::Key> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardState};
    let mut state = [0u8; 256];
    // SAFETY: 256바이트 배열에 이 스레드의 키 상태를 쓰는 조회다.
    if let Err(error) = unsafe { GetKeyboardState(&mut state) } {
        tracing::warn!(%error, "GetKeyboardState failed; the key is compared by its US character");
        return None;
    }
    // SAFETY: 0은 호출 스레드의 입력 언어를 뜻하며 조회만 한다.
    let layout = unsafe { GetKeyboardLayout(0) };
    character_key_in_layout(vk, scan, &state, layout)
}

/// 페이지가 받을 DOM `key` 문자. 단축키 비교용 US 문자와 달리 지금 배열이 Shift·CapsLock·AltGr로 만드는 문자다
/// (US 배열의 Ctrl+Shift+1은 "!"). Ctrl·Alt는 문자를 바꾸지 않으므로 빼고 읽는다. 둘을 함께 누르면 AltGr일 수
/// 있어 그대로 둔다. 글자가 없는 키와 읽지 못한 경우는 None이다.
pub(super) fn dom_character_key(vk: u32, scan: u32) -> Option<winit::keyboard::Key> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardState};
    let mut state = [0u8; 256];
    // SAFETY: 256바이트 배열에 이 스레드의 키 상태를 쓰는 조회다.
    if let Err(error) = unsafe { GetKeyboardState(&mut state) } {
        tracing::warn!(%error, "GetKeyboardState failed; the replayed key keeps its US character");
        return None;
    }
    strip_shortcut_modifiers(&mut state);
    // SAFETY: 0은 호출 스레드의 입력 언어를 뜻하며 조회만 한다.
    let layout = unsafe { GetKeyboardLayout(0) };
    character_key_in_layout(vk, scan, &state, layout)
}

/// 키 상태에서 Ctrl·Alt를 지운다. Ctrl과 Alt가 함께 눌렸으면(AltGr일 수 있다) 그대로 둔다.
pub(super) fn strip_shortcut_modifiers(state: &mut [u8; 256]) {
    const CONTROL: [usize; 3] = [0x11, 0xA2, 0xA3];
    const ALT: [usize; 3] = [0x12, 0xA4, 0xA5];
    let down = |state: &[u8; 256], keys: &[usize]| keys.iter().any(|&k| state[k] & 0x80 != 0);
    if down(state, &CONTROL) && down(state, &ALT) {
        return;
    }
    for k in CONTROL.into_iter().chain(ALT) {
        state[k] &= !0x80;
    }
}

/// `ToUnicodeEx`의 0x4 플래그(Windows 10 1607 이상)는 dead key 상태를 바꾸지 않아 뒤따르는 메시지의 조합이
/// 그대로 남는다. 그보다 오래된 Windows는 이 플래그를 모르므로 dead key 조합이 깨질 수 있다.
const TO_UNICODE_KEEP_KEYBOARD_STATE: u32 = 0x4;

/// 지정한 배열과 키 상태로 VK 하나의 문자를 읽는다. 시험에서 배열을 명시하려고 나눴다.
pub(super) fn character_key_in_layout(
    vk: u32,
    scan: u32,
    state: &[u8; 256],
    layout: windows::Win32::UI::Input::KeyboardAndMouse::HKL,
) -> Option<winit::keyboard::Key> {
    use windows::Win32::UI::Input::KeyboardAndMouse::ToUnicodeEx;
    let mut buf = [0u16; 8];
    // SAFETY: 입력 배열과 출력 버퍼가 호출 동안 살아 있고, 0x4 플래그로 키보드 상태를 바꾸지 않는다.
    let rc = unsafe {
        ToUnicodeEx(
            vk,
            scan,
            state,
            &mut buf,
            TO_UNICODE_KEEP_KEYBOARD_STATE,
            Some(layout),
        )
    };
    key_from_to_unicode(rc, &buf)
}

/// `ToUnicodeEx` 결과를 논리 키로 바꾼다. 음수는 dead key이고 어떤 바인딩과도 맞지 않는 `Key::Dead`가 된다.
/// 0(이 배열에서 문자가 없음)과 제어 문자는 None이다.
pub(super) fn key_from_to_unicode(rc: i32, buf: &[u16]) -> Option<winit::keyboard::Key> {
    use winit::keyboard::Key;
    if rc < 0 {
        let dead = buf
            .first()
            .and_then(|&u| char::from_u32(u32::from(u)))
            .filter(|c| !c.is_control());
        return Some(Key::Dead(dead));
    }
    let len = usize::try_from(rc)
        .ok()
        .filter(|&n| n > 0 && n <= buf.len())?;
    let text = String::from_utf16(&buf[..len]).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    Some(Key::Character(text.into()))
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
    fn key_up_vk_reads_only_key_up_messages() {
        assert_eq!(key_up_vk(0x0101, 0x20), Some(0x20));
        assert_eq!(key_up_vk(0x0100, 0x20), None);
        assert_eq!(key_up_vk(0x0105, 0x20), None);
        assert_eq!(key_up_vk(0x0102, 0x20), None);
    }

    #[test]
    fn to_unicode_results_become_layout_keys() {
        use winit::keyboard::Key;
        let q = [u16::from(b'q'), 0];
        assert_eq!(key_from_to_unicode(1, &q), Some(Key::Character("q".into())));
        let yi = [0x0439u16, 0];
        assert_eq!(
            key_from_to_unicode(1, &yi),
            Some(Key::Character("\u{439}".into()))
        );
        // dead key는 바인딩과 맞지 않는 Dead로 두어 페이지가 조합을 이어 가게 한다.
        let caret = [u16::from(b'^'), 0];
        assert_eq!(key_from_to_unicode(-1, &caret), Some(Key::Dead(Some('^'))));
        assert_eq!(key_from_to_unicode(0, &q), None);
        let ctrl_q = [0x11u16, 0];
        assert_eq!(key_from_to_unicode(1, &ctrl_q), None);
        assert_eq!(key_from_to_unicode(9, &q), None);
    }

    /// 배열을 임시로 불러 확인한다. 활성 배열은 바꾸지 않으며(KLF_ACTIVATE 없음), 시험 전에 없던 배열만
    /// 끝에(panic이어도) 내린다. 불러온 배열은 그동안 세션의 배열 목록에 보이므로 기본 실행에서 뺀다.
    /// 대화형 데스크톱이 없는 세션(SSH의 session 0)에서는 배열을 불러오지 못한다.
    #[test]
    #[ignore = "temporarily loads keyboard layouts into the interactive session; run explicitly"]
    fn plain_keys_follow_the_layout_and_keep_dead_key_state() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyboardLayoutList, HKL, KLF_NOTELLSHELL, LoadKeyboardLayoutW, UnloadKeyboardLayout,
        };
        use winit::keyboard::Key;
        /// 시험이 불러온 배열만 내린다. 단언이 실패해 panic해도 Drop에서 내린다.
        struct Loaded(Option<HKL>);
        impl Drop for Loaded {
            fn drop(&mut self) {
                if let Some(hkl) = self.0.take() {
                    // SAFETY: 이 시험이 불러온 배열만 내린다.
                    if let Err(error) = unsafe { UnloadKeyboardLayout(hkl) } {
                        tracing::warn!(%error, "unloading the test keyboard layout failed");
                    }
                }
            }
        }
        fn with_layout(klid: &str, check: impl FnOnce(HKL)) {
            let mut before = [HKL::default(); 64];
            // SAFETY: 조회만 하며 배열 길이를 함께 넘긴다.
            let n = unsafe { GetKeyboardLayoutList(Some(&mut before)) } as usize;
            let id: Vec<u16> = klid.encode_utf16().chain([0]).collect();
            // SAFETY: NUL 종단 문자열이며 KLF_ACTIVATE 없이 불러 활성 배열을 바꾸지 않는다.
            let hkl =
                unsafe { LoadKeyboardLayoutW(windows::core::PCWSTR(id.as_ptr()), KLF_NOTELLSHELL) }
                    .expect("load layout");
            let _loaded = Loaded((!before[..n].contains(&hkl)).then_some(hkl));
            check(hkl);
        }
        let plain = [0u8; 256];
        let mut shift = [0u8; 256];
        shift[0x10] = 0x80;
        shift[0xA0] = 0x80;
        let ch = |s: &str| Some(Key::Character(s.into()));
        with_layout("00000409", |us| {
            assert_eq!(character_key_in_layout(0xBA, 0x27, &plain, us), ch(";"));
            assert_eq!(character_key_in_layout(0xBA, 0x27, &shift, us), ch(":"));
            assert_eq!(character_key_in_layout(0x51, 0x10, &shift, us), ch("Q"));
        });
        with_layout("00000419", |ru| {
            assert_eq!(
                character_key_in_layout(0x51, 0x10, &plain, ru),
                ch("\u{439}")
            );
            assert_eq!(
                character_key_in_layout(0x51, 0x10, &shift, ru),
                ch("\u{419}")
            );
        });
        with_layout("00000407", |de| {
            assert_eq!(
                character_key_in_layout(0xBA, 0x1A, &plain, de),
                ch("\u{fc}")
            );
            // dead key(^)를 두 번 읽어도 같은 Dead이고, 뒤의 a는 조합되지 않은 a다(상태를 바꾸지 않음).
            for _ in 0..2 {
                assert_eq!(
                    character_key_in_layout(0xDC, 0x29, &plain, de),
                    Some(Key::Dead(Some('^')))
                );
            }
            assert_eq!(character_key_in_layout(0x41, 0x1E, &plain, de), ch("a"));
        });
    }

    /// Given Ctrl+Shift 상태, When DOM 문자용으로 고친다, Then Ctrl만 빠지고 Shift는 남는다.
    /// Given Ctrl+Alt 상태(AltGr일 수 있다), Then 그대로 둔다.
    #[test]
    fn dom_characters_ignore_ctrl_and_alt_but_keep_altgr() {
        let mut ctrl_shift = [0u8; 256];
        ctrl_shift[0x11] = 0x80;
        ctrl_shift[0xA2] = 0x80;
        ctrl_shift[0x10] = 0x80;
        ctrl_shift[0x14] = 0x01;
        strip_shortcut_modifiers(&mut ctrl_shift);
        assert_eq!(ctrl_shift[0x11] & 0x80, 0);
        assert_eq!(ctrl_shift[0xA2] & 0x80, 0);
        assert_eq!(ctrl_shift[0x10], 0x80);
        assert_eq!(ctrl_shift[0x14], 0x01);

        let mut alt = [0u8; 256];
        alt[0x12] = 0x80;
        alt[0xA4] = 0x80;
        strip_shortcut_modifiers(&mut alt);
        assert_eq!(alt[0x12] & 0x80, 0);

        let mut altgr = [0u8; 256];
        altgr[0x11] = 0x80;
        altgr[0xA2] = 0x80;
        altgr[0x12] = 0x80;
        altgr[0xA5] = 0x80;
        let before = altgr;
        strip_shortcut_modifiers(&mut altgr);
        assert_eq!(altgr, before);
    }

    #[test]
    fn ime_process_key_is_not_converted() {
        // IME가 처리하는 키는 VK_PROCESSKEY(0xE5)로 오고 변환표 밖이라 host가 판정하지 않는다.
        assert_eq!(vk_to_winit_key(0xE5), None);
    }
}
