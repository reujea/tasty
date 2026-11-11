//! host가 가져갔지만 Tasty 단축키가 동작하지 않은 키를 WebView2에 다시 넘긴다(Windows).
//!
//! 두 입구가 원래 이벤트를 다르게 붙잡는다.
//! - host 입력 훅: 소비한 메시지(hwnd·message·wParam·lParam)를 보관했다가 `PostMessageW`로 같은 창에 다시 보낸다.
//!   다시 온 메시지는 보낸 목록과 대조해 판정 없이 통과시키고, winit이 Translate·Dispatch한다. WM_CHAR는 그때의
//!   키보드 상태로 만들어지므로 결과 전에 Shift를 뗐으면 다른 글자가 될 수 있다.
//! - `AcceleratorKeyPressed`: 원래 메시지를 얻을 수 없어 DevTools `Input.dispatchKeyEvent`로 같은 키를 합성한다.
//!   이 경로의 키는 Ctrl·Alt 조합과 글자가 아닌 키라 글자를 넣지 않는다(`rawKeyDown`). DOM `key`는 판정 시점의
//!   배열·Shift·CapsLock으로 만든 문자다(`dom_character_key`).
//!
//! 다시 넘긴 키가 WebView2에서 `AcceleratorKeyPressed`로 다시 올라와도 판정하지 않도록 재주입 하나마다 표지를
//! 하나 둔다. 다시 보낸 key-down·key-up의 반송은 그 메시지를 Dispatch하는 동안 바로 온다. 그래서 그 표지는 훅이 그
//! 메시지를 꺼낸 때부터 다음 메시지가 훅에 오기 전까지만 두고, 그 사이 첫 `AcceleratorKeyPressed`에만 쓴다. 키와
//! lParam 상태(scan·extended·Alt·이전 상태·반복 수)가 모두 같아야 반송으로 본다. 합성 키의 표지는 scan code가
//! 없는 이벤트에만 쓴다. 실제 키보드 이벤트는 scan code가 있으므로 훅이 없는 view에서 남은 표지가 실제 누름이나
//! 반복을 삼키지 않는다.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{MSG, PostMessageW};
use windows::core::*;
use winit::keyboard::{Key, ModifiersState, NamedKey, PhysicalKey};

use super::keyboard::{
    current_winit_mods, dom_character_key, vk_to_winit_key, win32_scancode_to_physical,
};
use crate::host_api::webview::key_hold::{HELD_EVENT_LIMIT, KeyHold, RepeatRoute};
use crate::host_api::webview::keys::{KeyDownRoute, WebViewKeySink, route_key_down};

const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYUP: u32 = 0x0105;
/// 다시 보낸 메시지 목록의 상한. 창이 사라져 돌아오지 않은 메시지가 쌓이지 않게 한다. 결과 전에 붙잡을 수 있는
/// 이벤트를 한꺼번에 다시 보내도 훅이 꺼내기 전에 앞 표지가 밀리지 않도록 그 수와 같게 둔다.
const POSTED_LIMIT: usize = HELD_EVENT_LIMIT;
/// 다시 넘긴 키가 `AcceleratorKeyPressed`로 돌아올 수 있는 시간.
const ECHO_LIMIT: Duration = Duration::from_secs(1);

/// DevTools로 합성한 키 하나의 반송 표지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Echo {
    vk: u32,
    up: bool,
    at: Instant,
}

/// 합성한 키의 반송 표지들. 표지 하나는 반송 하나에만 쓰이고 `ECHO_LIMIT` 뒤에 사라진다.
#[derive(Debug, Default)]
struct Echoes(Vec<Echo>);

impl Echoes {
    fn note(&mut self, vk: u32, up: bool, at: Instant) {
        self.0.push(Echo { vk, up, at });
    }

    /// `AcceleratorKeyPressed`로 온 키가 반송이면 표지를 지우고 true다. 합성 키에는 scan code가 없으므로 scan
    /// code가 있는 이벤트(실제 키보드)에는 쓰지 않는다.
    fn take(&mut self, vk: u32, up: bool, scan: u32, now: Instant) -> bool {
        self.0
            .retain(|e| now.saturating_duration_since(e.at) <= ECHO_LIMIT);
        if scan != 0 {
            return false;
        }
        let found = self.0.iter().position(|e| e.vk == vk && e.up == up);
        found.map(|i| self.0.remove(i)).is_some()
    }
}

/// 다시 보낸 메시지의 키와 lParam 상태. `AcceleratorKeyPressed`의 물리 키 상태와 같은 비트 배치다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PostedKey {
    vk: u32,
    up: bool,
    state: u32,
}

impl PostedKey {
    fn of(copy: &MsgCopy) -> Self {
        Self {
            vk: (copy.wparam & 0xffff) as u32,
            up: copy.message == WM_KEYUP || copy.message == WM_SYSKEYUP,
            state: copy.lparam as u32,
        }
    }
}

/// `AcceleratorKeyPressed` 물리 키 상태를 키 메시지 lParam의 비트 배치로 옮긴다.
fn status_state(status: &COREWEBVIEW2_PHYSICAL_KEY_STATUS) -> u32 {
    (status.RepeatCount & 0xffff)
        | (status.ScanCode & 0xff) << 16
        | u32::from(status.IsExtendedKey.as_bool()) << 24
        | u32::from(status.IsMenuKeyDown.as_bool()) << 29
        | u32::from(status.WasKeyDown.as_bool()) << 30
        | u32::from(status.IsKeyReleased.as_bool()) << 31
}

/// 지금 Dispatch 중인 다시 보낸 메시지. 그 반송 하나에만 쓰인다.
#[derive(Debug, Default)]
struct Dispatching(Cell<Option<PostedKey>>);

impl Dispatching {
    fn begin(&self, key: PostedKey) {
        self.0.set(Some(key));
    }

    fn end(&self) {
        self.0.set(None);
    }

    /// 첫 `AcceleratorKeyPressed`가 표지를 비운다. 그 이벤트가 표지의 키·상태와 같으면 반송이다.
    fn take(&self, key: PostedKey) -> bool {
        self.0.take() == Some(key)
    }
}

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
    /// `layout`은 지금 배열이 만드는 문자(`dom_character_key`)다. 없으면 `key`(단축키 비교용 값)에서 만든다.
    pub(super) fn new(
        up: bool,
        vk: u32,
        key: &Key,
        layout: Option<&Key>,
        physical: &PhysicalKey,
        mods: ModifiersState,
        repeat: bool,
    ) -> Self {
        Self {
            up,
            vk,
            key: dom_key(key, layout, mods),
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

/// DOM `KeyboardEvent.key` 값. 배열 문자가 있으면 그것이고, dead key는 "Dead"다. 없으면 단축키 비교용 값에서
/// 만든다. winit의 named key 이름은 DOM 이름과 같다(공백 키만 다르다).
fn dom_key(key: &Key, layout: Option<&Key>, mods: ModifiersState) -> String {
    match layout {
        Some(Key::Character(c)) => return c.to_string(),
        Some(Key::Dead(_)) => return "Dead".into(),
        _ => {}
    }
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

/// 누름·반복·key-up을 짝짓는 물리 키. scan code와 extended 비트로 구분한다(주 Enter와 숫자패드 Enter는 VK가
/// 같다). scan code가 없는 합성 입력만 VK로 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyId {
    scan: u32,
    extended: bool,
    vk: u32,
}

impl KeyId {
    pub(super) fn new(vk: u32, scan: u32, extended: bool) -> Self {
        Self {
            scan,
            extended,
            vk: if scan == 0 { vk } else { 0 },
        }
    }
}

/// 붙잡은 원래 이벤트.
#[derive(Debug)]
pub(super) enum WinKey {
    Msg(MsgCopy),
    Cdp(CdpKey),
}

/// 다시 보낸 창 메시지 목록. 훅이 이 목록의 메시지를 판정 없이 통과시킨다.
#[derive(Debug, Default)]
struct Posted(Vec<MsgCopy>);

impl Posted {
    fn push(&mut self, copy: MsgCopy) {
        self.0.push(copy);
        if self.0.len() > POSTED_LIMIT {
            self.0.remove(0);
        }
    }

    /// 다시 보낸 메시지면 목록에서 지우고 true다.
    fn take(&mut self, copy: &MsgCopy) -> bool {
        let found = self.0.iter().position(|m| m == copy);
        found.map(|i| self.0.remove(i)).is_some()
    }

    /// 보내지 못한 메시지의 표지를 지운다.
    fn forget(&mut self, copy: &MsgCopy) {
        if let Some(i) = self.0.iter().rposition(|m| m == copy) {
            self.0.remove(i);
        }
    }
}

thread_local! {
    static POSTED: RefCell<Posted> = RefCell::new(Posted::default());
    static DISPATCHING: Dispatching = Dispatching::default();
}

/// 훅에 새 메시지가 왔다. 앞서 Dispatch한 다시 보낸 메시지의 반송 표지를 지운다.
pub(super) fn message_arrived() {
    DISPATCHING.with(Dispatching::end);
}

/// 훅이 꺼낸 다시 보낸 메시지를 이제 Dispatch한다. 그 반송을 판정하지 않게 표지를 둔다.
pub(super) fn dispatching_posted(msg: &MSG) {
    let copy = MsgCopy::of(msg);
    DISPATCHING.with(|d| begin_posted(d, &copy));
}

/// key-down과 key-up 모두 표지를 둔다. key-up의 반송이 판정되면 같은 키의 다음 누름에 붙은 결과를 지운다.
fn begin_posted(d: &Dispatching, copy: &MsgCopy) {
    d.begin(PostedKey::of(copy));
}

/// 다시 보낸 메시지면 목록에서 지우고 true다.
pub(super) fn take_posted(msg: &MSG) -> bool {
    let copy = MsgCopy::of(msg);
    POSTED.with(|p| p.borrow_mut().take(&copy))
}

fn post(copy: MsgCopy, surface_id: u32) {
    POSTED.with(|p| p.borrow_mut().push(copy));
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
        POSTED.with(|p| p.borrow_mut().forget(&copy));
        tracing::warn!("WebView surface {surface_id}: re-posting a key message failed: {e}");
    }
}

/// WebView 하나의 붙잡은 키와 `AcceleratorKeyPressed` 반송 표지. GUI 스레드에서만 쓴다.
pub(super) struct KeyReplay {
    surface_id: u32,
    target: ICoreWebView2,
    hold: RefCell<KeyHold<KeyId, WinKey>>,
    echoes: RefCell<Echoes>,
}

impl KeyReplay {
    pub(super) fn new(surface_id: u32, target: ICoreWebView2) -> Self {
        Self {
            surface_id,
            target,
            hold: RefCell::new(KeyHold::default()),
            echoes: RefCell::new(Echoes::default()),
        }
    }

    /// 그 자리에서 정한 첫 누름. 반복이 이 결과를 따른다.
    pub(super) fn decide(&self, key_id: KeyId, pass: bool) {
        self.hold.borrow_mut().decide(key_id, pass);
    }

    pub(super) fn forwarded(&self, id: u64, key_id: KeyId, key: WinKey) {
        if !self.hold.borrow_mut().forwarded(id, key_id, key) {
            tracing::warn!(
                "WebView surface {}: too many keys waiting for a result; dropped the oldest",
                self.surface_id
            );
        }
    }

    pub(super) fn repeat(&self, key_id: KeyId, key: WinKey) -> RepeatRoute<WinKey> {
        self.hold.borrow_mut().repeat(key_id, key)
    }

    /// key-up을 붙잡았으면 true다.
    pub(super) fn release(&self, key_id: KeyId, key: WinKey) -> bool {
        self.hold.borrow_mut().release(key_id, key).is_none()
    }

    /// 다시 넘긴 키가 `AcceleratorKeyPressed`로 돌아왔으면 표지를 지우고 true다.
    pub(super) fn take_echo(
        &self,
        vk: u32,
        up: bool,
        status: &COREWEBVIEW2_PHYSICAL_KEY_STATUS,
    ) -> bool {
        let posted = PostedKey {
            vk,
            up,
            state: status_state(status),
        };
        DISPATCHING.with(|d| d.take(posted))
            || self
                .echoes
                .borrow_mut()
                .take(vk, up, status.ScanCode, Instant::now())
    }

    /// 일련번호 `id`의 결과를 적용한다. 동작하지 않았으면 붙잡은 key-down을 다시 넘기고, 붙잡은 key-up은
    /// 결과와 관계없이 그 뒤에 넘긴다.
    pub(super) fn finish(&self, id: u64, pass: bool) {
        let Some(settled) = self.hold.borrow_mut().finish(id, pass) else {
            return;
        };
        for key in settled.replay.into_iter().chain(settled.release) {
            match key {
                WinKey::Msg(copy) => post(copy, self.surface_id),
                WinKey::Cdp(cdp) => {
                    self.echoes
                        .borrow_mut()
                        .note(cdp.vk, cdp.up, Instant::now());
                    self.dispatch(&cdp);
                }
            }
        }
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
            if replay.take_echo(vk, up, &status) {
                return Ok(());
            }
            let Some(key) = vk_to_winit_key(vk) else {
                return Ok(());
            };
            let extended = status.IsExtendedKey.as_bool();
            let key_id = KeyId::new(vk, status.ScanCode, extended);
            let physical = win32_scancode_to_physical(status.ScanCode, extended);
            let mods = current_winit_mods();
            let repeat = !up && status.WasKeyDown.as_bool();
            let layout = dom_character_key(vk, status.ScanCode);
            let cdp = WinKey::Cdp(CdpKey::new(
                up,
                vk,
                &key,
                layout.as_ref(),
                &physical,
                mods,
                repeat,
            ));
            if up {
                if replay.release(key_id, cdp) {
                    // SAFETY: 위와 같다.
                    unsafe { args.SetHandled(true) }?;
                }
                return Ok(());
            }
            let run_edit = |action| super::run_editing_command(&edit_target, action, surface_id);
            // host 입력 경로의 view도 판정한다. 훅이 소비한 키는 여기로 오지 않고, 훅을 지나지 않은
            // 키(중첩 메시지 루프)는 여기서 받는다(host_input 모듈 설명).
            let consumed = if repeat {
                match replay.repeat(key_id, cdp) {
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
                    KeyDownRoute::Forwarded(id) => replay.forwarded(id, key_id, cdp),
                    KeyDownRoute::Pass => replay.decide(key_id, true),
                    KeyDownRoute::Consumed => replay.decide(key_id, false),
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

/// key-up 메시지(WM_KEYUP·WM_SYSKEYUP)의 물리 키.
pub(super) fn release_key(message: u32, wparam: usize, lparam: isize) -> Option<KeyId> {
    let l = lparam as u32;
    (message == WM_KEYUP || message == WM_SYSKEYUP).then(|| {
        KeyId::new(
            (wparam & 0xffff) as u32,
            (l >> 16) & 0xff,
            l & (1 << 24) != 0,
        )
    })
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
            None,
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
            None,
            &PhysicalKey::Code(KeyCode::F9),
            ModifiersState::empty(),
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&up.params()).unwrap();
        assert_eq!(v["type"], "keyUp");
        assert_eq!(v["key"], "F9");
        assert_eq!(v["code"], "F9");
    }

    /// Given US 배열의 Ctrl+Shift+1(단축키 비교값 "1", 배열 문자 "!"), When 합성한다, Then DOM key는 "!"이고
    /// code는 Digit1이다. dead key는 "Dead"다.
    #[test]
    fn cdp_key_uses_the_layout_character() {
        let bang = Key::Character("!".into());
        let k = CdpKey::new(
            false,
            0x31,
            &Key::Character("1".into()),
            Some(&bang),
            &PhysicalKey::Code(KeyCode::Digit1),
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&k.params()).unwrap();
        assert_eq!(v["key"], "!");
        assert_eq!(v["code"], "Digit1");
        let dead = Key::Dead(Some('^'));
        let k = CdpKey::new(
            false,
            0xDC,
            &Key::Character("\\".into()),
            Some(&dead),
            &PhysicalKey::Code(KeyCode::Backquote),
            ModifiersState::CONTROL,
            false,
        );
        assert_eq!(k.key, "Dead");
    }

    /// Given 훅 없는 view에서 F9를 다시 합성했고 반송은 오지 않았다, When 표지가 남은 동안 실제 F9 반복(scan
    /// 0x43)과 새 누름이 온다, Then 둘 다 표지에 쓰이지 않아 판정을 받는다. scan code 없는 반송은 표지 하나에만
    /// 쓰이고, 표지는 1초 뒤 사라진다.
    #[test]
    fn synthetic_echo_marks_never_take_real_keys() {
        let t = Instant::now();
        let mut e = Echoes::default();
        e.note(0x78, false, t);
        assert!(!e.take(0x78, false, 0x43, t + Duration::from_millis(40)));
        assert!(!e.take(0x78, false, 0x43, t + Duration::from_millis(300)));
        assert!(e.take(0x78, false, 0, t + Duration::from_millis(300)));
        assert!(!e.take(0x78, false, 0, t + Duration::from_millis(310)));
        e.note(0x78, true, t);
        assert!(
            !e.take(0x78, false, 0, t),
            "key-down은 key-up 표지에 쓰이지 않는다"
        );
        assert!(!e.take(0x78, true, 0, t + ECHO_LIMIT + Duration::from_millis(1)));
    }

    fn posted(message: u32, vk: usize, lparam: u32) -> PostedKey {
        PostedKey::of(&MsgCopy {
            hwnd: 1,
            message,
            wparam: vk,
            lparam: lparam as i32 as isize,
        })
    }

    /// Given 훅이 다시 보낸 F9 key-down(lParam 0x00430001)을 Dispatch한다, When 같은 키·상태의
    /// `AcceleratorKeyPressed`가 온다, Then 반송으로 보고 표지를 비운다. 뒤의 같은 이벤트는 판정을 받는다.
    #[test]
    fn a_posted_key_down_echo_is_taken_once_during_its_dispatch() {
        let d = Dispatching::default();
        let f9 = posted(0x0100, 0x78, 0x0043_0001);
        d.begin(f9);
        assert!(d.take(f9));
        assert!(!d.take(f9));
    }

    /// Given 동작하지 않은 R(수식키 없음)을 다시 보냈고 글자 키라 반송이 오지 않았다, When 다음 메시지가 훅에
    /// 온 뒤 중첩 메시지 루프에서 실제 Ctrl+R(같은 물리 키)이 `AcceleratorKeyPressed`로 온다, Then 표지에 쓰이지
    /// 않아 단축키 판정을 받는다. 다른 키가 먼저 오면 표지는 그 이벤트에서 비워진다.
    #[test]
    fn a_leftover_posted_mark_never_takes_another_real_key() {
        let d = Dispatching::default();
        let r = posted(0x0100, 0x52, 0x0013_0001);
        d.begin(r);
        d.end();
        assert!(!d.take(r), "실제 Ctrl+R이 판정을 건너뛰었다");
        d.begin(r);
        assert!(!d.take(posted(0x0100, 0x11, 0x001d_0001)));
        assert!(!d.take(r), "다른 키가 지나간 뒤의 R이 판정을 건너뛰었다");
        // 같은 VK라도 물리 키가 다르면(주 Enter와 숫자패드 Enter) 반송이 아니다.
        d.begin(posted(0x0100, 0x0D, 0x001c_0001));
        assert!(!d.take(posted(0x0100, 0x0D, 0x011c_0001)));
    }

    /// Given 같은 키를 두 번 눌렀고 첫 누름의 key-up을 훅이 다시 보낸다, When 그 key-up이 Dispatch 중에
    /// 반송된다, Then 반송으로 보고 판정하지 않는다(두 번째 누름의 기록에 붙지 않는다). 다음 메시지인 두 번째
    /// key-down이 오면 표지는 비워져 그 누름은 판정을 받는다.
    #[test]
    fn a_posted_key_up_echo_never_reaches_the_next_press() {
        let d = Dispatching::default();
        let up1 = MsgCopy {
            hwnd: 1,
            message: WM_KEYUP,
            wparam: 0x78,
            lparam: 0xc043_0001_u32 as i32 as isize,
        };
        begin_posted(&d, &up1);
        assert!(d.take(PostedKey::of(&up1)), "첫 key-up의 반송이 판정됐다");
        begin_posted(&d, &up1);
        d.end();
        assert!(!d.take(posted(0x0100, 0x78, 0x0043_0001)));
    }

    /// 물리 키 상태 비트 배치는 키 메시지 lParam과 같다. 측정한 반송(F9 누름·뗌, Alt+K)의 값이다.
    #[test]
    fn status_state_matches_the_message_lparam() {
        let st = |repeat, scan, ext: bool, menu: bool, was: bool, rel: bool| {
            status_state(&COREWEBVIEW2_PHYSICAL_KEY_STATUS {
                RepeatCount: repeat,
                ScanCode: scan,
                IsExtendedKey: ext.into(),
                IsMenuKeyDown: menu.into(),
                WasKeyDown: was.into(),
                IsKeyReleased: rel.into(),
            })
        };
        assert_eq!(st(1, 0x43, false, false, false, false), 0x0043_0001);
        assert_eq!(st(1, 0x43, false, false, true, true), 0xc043_0001);
        assert_eq!(st(1, 0x25, false, true, false, false), 0x2025_0001);
        assert_eq!(st(1, 0x1c, true, false, false, false), 0x011c_0001);
    }

    /// Given 물리 키 4개의 첫 누름마다 결과 전 반복 64개를 붙잡았다, When 넷 다 동작하지 않아 메시지 루프가 돌기
    /// 전에 260개를 연달아 다시 보낸다, Then 맨 앞 메시지도 훅이 꺼낼 때 다시 보낸 것으로 알아본다.
    #[test]
    fn posted_marks_cover_every_held_event_sent_at_once() {
        let msg = |vk: usize, n: isize| MsgCopy {
            hwnd: 1,
            message: 0x0100,
            wparam: vk,
            lparam: (n << 16) | 1,
        };
        let mut p = Posted::default();
        for key in 0..4 {
            for n in 0..=64 {
                p.push(msg(0x41 + key, n));
            }
        }
        assert!(p.take(&msg(0x41, 0)), "맨 앞 메시지의 표지가 밀렸다");
        assert!(p.take(&msg(0x44, 64)));
        assert!(!p.take(&msg(0x41, 0)), "표지는 한 번만 쓴다");
    }

    #[test]
    fn release_key_reads_both_key_up_messages() {
        // F9 key-up: scan 0x43. 32비트 lParam을 부호 확장한 값(winit이 넘기는 isize)이다.
        let f9 = 0xffff_ffff_c043_0001_u64 as isize;
        assert_eq!(
            release_key(WM_KEYUP, 0x78, f9),
            Some(KeyId::new(0x78, 0x43, false))
        );
        assert_eq!(
            release_key(WM_SYSKEYUP, 0x78, f9),
            Some(KeyId::new(0x78, 0x43, false))
        );
        assert_eq!(release_key(0x0100, 0x78, f9), None);
    }

    /// Given 주 Enter와 숫자패드 Enter(같은 VK)를 결과 전에 함께 누른다, When 주 Enter를 먼저 뗀다,
    /// Then 그 key-up은 주 Enter의 누름에 붙고 숫자패드 Enter의 반복은 자기 누름에 붙는다.
    #[test]
    fn keys_sharing_a_virtual_key_are_held_per_physical_key() {
        let enter = KeyId::new(0x0D, 0x1C, false);
        let numpad_enter = KeyId::new(0x0D, 0x1C, true);
        assert_ne!(enter, numpad_enter);
        let mut q = KeyHold::<KeyId, &str>::default();
        q.forwarded(1, enter, "enter down");
        q.forwarded(2, numpad_enter, "numpad down");
        assert_eq!(q.release(enter, "enter up"), None);
        assert_eq!(q.repeat(numpad_enter, "numpad repeat"), RepeatRoute::Held);
        let first = q.finish(1, true).expect("enter result");
        assert_eq!(first.replay, vec!["enter down"]);
        assert_eq!(first.release, Some("enter up"));
        let second = q.finish(2, true).expect("numpad result");
        assert_eq!(second.replay, vec!["numpad down", "numpad repeat"]);
        assert_eq!(second.release, None);
        // scan code가 없는 합성 입력은 VK로 구분한다.
        assert_ne!(KeyId::new(0x41, 0, false), KeyId::new(0x42, 0, false));
        assert_eq!(KeyId::new(0x41, 0x1E, false), KeyId::new(0x61, 0x1E, false));
    }
}
