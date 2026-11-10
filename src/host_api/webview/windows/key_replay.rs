//! host가 가져갔지만 Tasty 단축키가 동작하지 않은 키를 WebView2에 다시 넘긴다(Windows).
//!
//! 두 입구가 원래 이벤트를 다르게 붙잡는다.
//! - host 입력 훅: 소비한 메시지(hwnd·message·wParam·lParam)를 보관했다가 `PostMessageW`로 같은 창에 다시 보낸다.
//!   다시 온 메시지는 보낸 목록과 대조해 판정 없이 통과시키고, winit이 Translate·Dispatch한다. WM_CHAR는 그때의
//!   키보드 상태로 만들어지므로 결과 전에 Shift를 뗐으면 다른 글자가 될 수 있다.
//! - `AcceleratorKeyPressed`: 원래 메시지를 얻을 수 없어 DevTools `Input.dispatchKeyEvent`로 같은 키를 합성한다.
//!   이 경로의 키는 Ctrl·Alt 조합과 글자가 아닌 키라 글자를 넣지 않는다(`rawKeyDown`).
//!
//! 다시 넘긴 키가 WebView2에서 `AcceleratorKeyPressed`로 다시 올라와도 판정하지 않도록 그 키(vk, key-up 여부)의
//! 표지를 짧은 시간 둔다. host 입력 경로에서는 실제 키도 훅이 먼저 판정하므로 표지가 실제 키에 쓰여도 결과가 같다.
//! 훅이 없는 view에서는 표지가 남은 동안 같은 키의 실제 누름 하나가 판정 없이 페이지로 갈 수 있다.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{MSG, PostMessageW};
use windows::core::*;
use winit::keyboard::{Key, ModifiersState, NamedKey, PhysicalKey};

use super::keyboard::{current_winit_mods, vk_to_winit_key, win32_scancode_to_physical};
use crate::host_api::webview::key_hold::{KeyHold, RepeatRoute};
use crate::host_api::webview::keys::{KeyDownRoute, WebViewKeySink, route_key_down};

const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYUP: u32 = 0x0105;
/// 다시 보낸 메시지 목록의 상한. 창이 사라져 돌아오지 않은 메시지가 쌓이지 않게 한다.
const POSTED_LIMIT: usize = 256;
/// 다시 넘긴 키가 `AcceleratorKeyPressed`로 돌아올 수 있는 시간.
const ECHO_LIMIT: Duration = Duration::from_secs(1);

/// 훅이 붙잡은 창 메시지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MsgCopy {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
}

impl MsgCopy {
    pub(super) fn of(msg: &MSG) -> Self {
        Self {
            hwnd: msg.hwnd.0 as isize,
            message: msg.message,
            wparam: msg.wParam.0,
            lparam: msg.lParam.0,
        }
    }
}

/// DevTools로 합성할 키.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CdpKey {
    pub up: bool,
    pub vk: u32,
    pub key: String,
    pub code: String,
    pub mods: ModifiersState,
    pub repeat: bool,
}

impl CdpKey {
    pub(super) fn new(
        up: bool,
        vk: u32,
        key: &Key,
        physical: &PhysicalKey,
        mods: ModifiersState,
        repeat: bool,
    ) -> Self {
        Self {
            up,
            vk,
            key: dom_key(key, mods),
            code: dom_code(physical),
            mods,
            repeat,
        }
    }

    /// `Input.dispatchKeyEvent` 인자. modifiers는 CDP 비트(Alt 1, Ctrl 2, Meta 4, Shift 8)다.
    fn params(&self) -> String {
        let mut bits = 0;
        if self.mods.alt_key() {
            bits |= 1;
        }
        if self.mods.control_key() {
            bits |= 2;
        }
        if self.mods.super_key() {
            bits |= 4;
        }
        if self.mods.shift_key() {
            bits |= 8;
        }
        serde_json::json!({
            "type": if self.up { "keyUp" } else { "rawKeyDown" },
            "windowsVirtualKeyCode": self.vk,
            "nativeVirtualKeyCode": self.vk,
            "key": self.key,
            "code": self.code,
            "modifiers": bits,
            "autoRepeat": self.repeat,
        })
        .to_string()
    }
}

/// DOM `KeyboardEvent.key` 값. winit의 named key 이름은 DOM 이름과 같다(공백 키만 다르다).
fn dom_key(key: &Key, mods: ModifiersState) -> String {
    match key {
        Key::Character(c) if mods.shift_key() => c.to_uppercase(),
        Key::Character(c) => c.to_string(),
        Key::Named(NamedKey::Space) => " ".into(),
        Key::Named(named) => format!("{named:?}"),
        _ => "Unidentified".into(),
    }
}

/// DOM `KeyboardEvent.code` 값. winit의 물리 키 이름은 DOM code 이름을 따른다. 모르는 키는 빈 값이다.
fn dom_code(physical: &PhysicalKey) -> String {
    match physical {
        PhysicalKey::Code(code) => format!("{code:?}"),
        PhysicalKey::Unidentified(_) => String::new(),
    }
}

/// 붙잡은 원래 이벤트.
#[derive(Debug)]
pub(super) enum WinKey {
    Msg(MsgCopy),
    Cdp(CdpKey),
}

thread_local! {
    /// 다시 보낸 창 메시지. 훅이 이 목록의 메시지를 판정 없이 통과시킨다.
    static POSTED: RefCell<Vec<MsgCopy>> = const { RefCell::new(Vec::new()) };
}

/// 다시 보낸 메시지면 목록에서 지우고 true다.
pub(super) fn take_posted(msg: &MSG) -> bool {
    let copy = MsgCopy::of(msg);
    POSTED.with(|p| {
        let mut p = p.borrow_mut();
        match p.iter().position(|m| *m == copy) {
            Some(i) => {
                p.remove(i);
                true
            }
            None => false,
        }
    })
}

fn post(copy: MsgCopy, surface_id: u32) {
    POSTED.with(|p| {
        let mut p = p.borrow_mut();
        p.push(copy);
        if p.len() > POSTED_LIMIT {
            p.remove(0);
        }
    });
    // SAFETY: 이 GUI 스레드가 받은 창 핸들에 같은 메시지를 다시 보낸다. 창이 사라졌으면 실패를 돌려준다.
    let sent = unsafe {
        PostMessageW(
            Some(HWND(copy.hwnd as *mut _)),
            copy.message,
            WPARAM(copy.wparam),
            LPARAM(copy.lparam),
        )
    };
    if let Err(e) = sent {
        POSTED.with(|p| {
            let mut p = p.borrow_mut();
            if let Some(i) = p.iter().rposition(|m| *m == copy) {
                p.remove(i);
            }
        });
        tracing::warn!("WebView surface {surface_id}: re-posting a key message failed: {e}");
    }
}

/// WebView 하나의 붙잡은 키와 `AcceleratorKeyPressed` 반송 표지. GUI 스레드에서만 쓴다.
pub(super) struct KeyReplay {
    surface_id: u32,
    target: ICoreWebView2,
    hold: RefCell<KeyHold<u32, WinKey>>,
    /// 다시 넘긴 키(vk, key-up 여부)와 그 시각.
    echoes: RefCell<Vec<(u32, bool, Instant)>>,
}

impl KeyReplay {
    pub(super) fn new(surface_id: u32, target: ICoreWebView2) -> Self {
        Self {
            surface_id,
            target,
            hold: RefCell::new(KeyHold::default()),
            echoes: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn press(&self, vk: u32) {
        self.hold.borrow_mut().press(vk);
    }

    pub(super) fn forwarded(&self, id: u64, vk: u32, key: WinKey) {
        if !self.hold.borrow_mut().forwarded(id, vk, key) {
            tracing::warn!(
                "WebView surface {}: too many keys waiting for a result; dropped the oldest",
                self.surface_id
            );
        }
    }

    pub(super) fn repeat(&self, vk: u32, key: WinKey) -> RepeatRoute<WinKey> {
        self.hold.borrow_mut().repeat(vk, key)
    }

    /// key-up을 붙잡았으면 true다.
    pub(super) fn release(&self, vk: u32, key: WinKey) -> bool {
        self.hold.borrow_mut().release(vk, key).is_none()
    }

    pub(super) fn clear_decisions(&self) {
        self.hold.borrow_mut().clear_decisions();
    }

    /// 다시 넘긴 키가 `AcceleratorKeyPressed`로 돌아왔으면 표지를 지우고 true다.
    pub(super) fn take_echo(&self, vk: u32, up: bool) -> bool {
        let now = Instant::now();
        let mut echoes = self.echoes.borrow_mut();
        echoes.retain(|(_, _, at)| now.saturating_duration_since(*at) <= ECHO_LIMIT);
        match echoes.iter().position(|(v, u, _)| *v == vk && *u == up) {
            Some(i) => {
                echoes.remove(i);
                true
            }
            None => false,
        }
    }

    /// 일련번호 `id`의 결과를 적용한다. 동작하지 않았으면 붙잡은 key-down을 다시 넘기고, 붙잡은 key-up은
    /// 결과와 관계없이 그 뒤에 넘긴다.
    pub(super) fn finish(&self, id: u64, pass: bool) {
        let Some(settled) = self.hold.borrow_mut().finish(id, pass) else {
            return;
        };
        for key in settled.replay.into_iter().chain(settled.release) {
            match key {
                WinKey::Msg(copy) => {
                    let up = copy.message == WM_KEYUP || copy.message == WM_SYSKEYUP;
                    if !up {
                        self.note_echo((copy.wparam & 0xffff) as u32, false);
                    }
                    post(copy, self.surface_id);
                }
                WinKey::Cdp(cdp) => {
                    self.note_echo(cdp.vk, cdp.up);
                    self.dispatch(&cdp);
                }
            }
        }
    }

    fn note_echo(&self, vk: u32, up: bool) {
        self.echoes.borrow_mut().push((vk, up, Instant::now()));
    }

    fn dispatch(&self, cdp: &CdpKey) {
        let surface_id = self.surface_id;
        let params = HSTRING::from(cdp.params());
        let done =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _json| {
                if let Err(e) = result {
                    tracing::warn!("WebView surface {surface_id}: key replay failed: {e}");
                }
                Ok(())
            }));
        // SAFETY: GUI 스레드에서 살아 있는 WebView에 호출하며 HSTRING은 호출 동안 유지된다.
        let sent = unsafe {
            self.target
                .CallDevToolsProtocolMethod(w!("Input.dispatchKeyEvent"), &params, &done)
        };
        if let Err(e) = sent {
            tracing::warn!("WebView surface {surface_id}: key replay failed: {e}");
        }
    }
}

/// `AcceleratorKeyPressed` 처리기. 훅을 지나지 않은 키(옵션 없는 view, 중첩 메시지 루프)를 판정하고, 가져간
/// key-down과 결과 전의 반복·key-up을 합성용 정보로 붙잡는다. 다시 넘긴 키의 반송은 판정하지 않는다.
pub(super) fn accelerator_key_handler(
    sink: std::rc::Rc<dyn WebViewKeySink>,
    edit_target: ICoreWebView2,
    replay: std::rc::Rc<KeyReplay>,
    surface_id: u32,
) -> ICoreWebView2AcceleratorKeyPressedEventHandler {
    AcceleratorKeyPressedEventHandler::create(Box::new(
        move |_sender, args| -> windows::core::Result<()> {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            // SAFETY: WebView2가 콜백 동안 유효한 args를 넘기며 출력 변수는 이 스택의 값이다.
            unsafe { args.KeyEventKind(&mut kind) }?;
            let up = kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_UP
                || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_UP;
            if !up
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
            // SAFETY: 위와 같다.
            unsafe { args.PhysicalKeyStatus(&mut status) }?;
            let mut vk: u32 = 0;
            // SAFETY: 위와 같다.
            unsafe { args.VirtualKey(&mut vk) }?;
            if replay.take_echo(vk, up) {
                return Ok(());
            }
            let Some(key) = vk_to_winit_key(vk) else {
                return Ok(());
            };
            let physical =
                win32_scancode_to_physical(status.ScanCode, status.IsExtendedKey.as_bool());
            let mods = current_winit_mods();
            let repeat = !up && status.WasKeyDown.as_bool();
            let cdp = WinKey::Cdp(CdpKey::new(up, vk, &key, &physical, mods, repeat));
            if up {
                if replay.release(vk, cdp) {
                    // SAFETY: 위와 같다.
                    unsafe { args.SetHandled(true) }?;
                }
                return Ok(());
            }
            let run_edit = |action| super::run_editing_command(&edit_target, action, surface_id);
            // host 입력 경로의 view도 판정한다. 훅이 소비한 키는 여기로 오지 않고, 훅을 지나지 않은
            // 키(중첩 메시지 루프)는 여기서 받는다(host_input 모듈 설명).
            let consumed = if repeat {
                match replay.repeat(vk, cdp) {
                    RepeatRoute::Held | RepeatRoute::Drop => true,
                    RepeatRoute::Pass(_) => false,
                    RepeatRoute::Unknown(_) => {
                        route_key_down(&*sink, surface_id, key, physical, mods, true, run_edit)
                            .consumed()
                    }
                }
            } else {
                let route =
                    route_key_down(&*sink, surface_id, key, physical, mods, false, run_edit);
                match route {
                    KeyDownRoute::Forwarded(id) => replay.forwarded(id, vk, cdp),
                    KeyDownRoute::Pass | KeyDownRoute::Consumed => replay.press(vk),
                }
                route.consumed()
            };
            if consumed {
                // SAFETY: 위와 같다.
                unsafe { args.SetHandled(true) }?;
            }
            Ok(())
        },
    ))
}

/// key-up 메시지(WM_KEYUP·WM_SYSKEYUP)의 vk.
pub(super) fn release_vk(message: u32, wparam: usize) -> Option<u32> {
    (message == WM_KEYUP || message == WM_SYSKEYUP).then_some((wparam & 0xffff) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    /// 합성 키는 페이지가 보는 key·code·수식키를 원래 키에서 채운다.
    #[test]
    fn cdp_params_carry_the_original_key() {
        let k = CdpKey::new(
            false,
            0x43,
            &Key::Character("c".into()),
            &PhysicalKey::Code(KeyCode::KeyC),
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&k.params()).unwrap();
        assert_eq!(v["type"], "rawKeyDown");
        assert_eq!(v["key"], "C");
        assert_eq!(v["code"], "KeyC");
        assert_eq!(v["windowsVirtualKeyCode"], 0x43);
        assert_eq!(v["modifiers"], 10);
        let up = CdpKey::new(
            true,
            0x78,
            &Key::Named(NamedKey::F9),
            &PhysicalKey::Code(KeyCode::F9),
            ModifiersState::empty(),
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&up.params()).unwrap();
        assert_eq!(v["type"], "keyUp");
        assert_eq!(v["key"], "F9");
        assert_eq!(v["code"], "F9");
    }

    #[test]
    fn release_vk_reads_both_key_up_messages() {
        assert_eq!(release_vk(WM_KEYUP, 0x78), Some(0x78));
        assert_eq!(release_vk(WM_SYSKEYUP, 0x43), Some(0x43));
        assert_eq!(release_vk(0x0100, 0x43), None);
    }
}
