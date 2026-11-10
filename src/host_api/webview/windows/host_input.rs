//! WebView2 `AllowHostInputProcessing` 키 경로.
//!
//! 이 옵션으로 만든 controller는 키보드 포커스 창(`Chrome_WidgetWin_0`)을 host GUI 스레드에 둔다.
//! 그래서 페이지로 갈 키 메시지가 winit 메시지 루프를 먼저 지나고, winit 메시지 훅이
//! Translate·Dispatch 전에 소비 여부를 정할 수 있다. `AcceleratorKeyPressed`는 Ctrl·Alt가 없는
//! 글자 키를 올리지 않으므로 그 키의 바인딩은 이 경로로만 받는다.
//! 옵션을 쓸 수 없으면(옛 runtime의 인터페이스 부재, 생성 실패) 옵션 없이 다시 만들고
//! 그 view는 기존 `AcceleratorKeyPressed` 경로만 쓴다.
//!
//! 옵션을 켠 view도 `AcceleratorKeyPressed` 판정을 유지한다. 훅이 소비한 key-down은 Dispatch되지 않아
//! WebView2가 그 키로 이벤트를 올리지 않는다. 훅이 통과시킨 키는 같은 판정을 다시 받지만, 판정은 소비하지 않을
//! 때 아무 상태도 바꾸지 않으므로 같은 결과(통과)가 된다. 훅을 지나지 않은 키(`wait_with_pump` 같은 중첩 메시지
//! 루프, 다른 스레드가 보낸 메시지)는 이 이벤트가 Ctrl·Alt 조합과 글자가 아닌 키를 대신 판정한다.
//!
//! host 큐로 가져간 key-down은 원래 메시지를 붙잡았다가 Tasty 단축키가 동작하지 않았으면 다시 보낸다
//! (`super::key_replay`). 결과 전에 온 자동 반복과 key-up도 함께 붙잡는다.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::{
    IsChild, MSG, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
};
use windows::core::*;

use super::key_replay::{KeyReplay, MsgCopy, WinKey, release_vk, take_posted};
use super::keyboard::{
    current_winit_mods, key_down_message, key_up_vk, layout_character_key, vk_to_winit_key,
    win32_scancode_to_physical,
};
use crate::host_api::webview::click_gesture::{ClickGesture, proves_activation};
use crate::host_api::webview::key_hold::RepeatRoute;
use crate::host_api::webview::keys::{KeyDownRoute, WebViewKeySink, route_key_down};

struct Route {
    root: HWND,
    surface_id: u32,
    sink: Rc<dyn WebViewKeySink>,
    target: ICoreWebView2,
    click: Rc<ClickGesture>,
    replay: Rc<KeyReplay>,
}

thread_local! {
    /// host 입력 경로로 만든 WebView. winit 메시지 훅과 같은 GUI 스레드에서만 쓴다.
    static ROUTES: RefCell<Vec<Route>> = const { RefCell::new(Vec::new()) };
}

/// 등록을 지우는 표지. WebView가 사라질 때 함께 버린다.
pub(super) struct RouteGuard {
    root: HWND,
    click: Rc<ClickGesture>,
}

impl RouteGuard {
    /// 이 WebView가 받은 OS 입력. 같은 문서 이동의 사용자 제스처 근거다.
    pub(super) fn click(&self) -> Rc<ClickGesture> {
        self.click.clone()
    }

    /// 호스트가 로드·이동을 시작했다. 남은 입력이 그 탐색을 증명하지 않게 한다.
    pub(super) fn cancel_click(&self) {
        self.click.cancel();
    }
}

impl Drop for RouteGuard {
    fn drop(&mut self) {
        let root = self.root;
        ROUTES.with(|routes| routes.borrow_mut().retain(|r| r.root != root));
    }
}

/// `root`(TASTY_WEBVIEW 자식 창)와 그 자손에 온 키를 이 surface의 키로 처리하게 등록한다.
pub(super) fn register(
    root: HWND,
    surface_id: u32,
    sink: Rc<dyn WebViewKeySink>,
    target: ICoreWebView2,
    replay: Rc<KeyReplay>,
) -> RouteGuard {
    let click = Rc::new(ClickGesture::default());
    ROUTES.with(|routes| {
        let mut routes = routes.borrow_mut();
        routes.retain(|r| r.root != root);
        routes.push(Route {
            root,
            surface_id,
            sink,
            target,
            click: click.clone(),
            replay,
        });
    });
    RouteGuard { root, click }
}

/// 등록한 WebView(`root`와 그 자손)로 가는 메시지의 경로를 찾는다.
fn route_of<T>(hwnd: HWND, pick: impl FnOnce(&Route) -> T) -> Option<T> {
    if hwnd.is_invalid() {
        return None;
    }
    ROUTES.with(|routes| {
        routes
            .borrow()
            .iter()
            .find(|r| {
                // SAFETY: 두 핸들 모두 이 스레드가 받은 값이며 IsChild는 조회만 한다.
                r.root == hwnd || unsafe { IsChild(r.root, hwnd) }.as_bool()
            })
            .map(pick)
    })
}

/// WebView로 가는 왼쪽 버튼 누름·뗌을 클릭으로 기록한다. 메시지는 소비하지 않는다.
/// 페이지 스크립트가 만든 이벤트는 창 메시지가 아니므로 여기에 오지 않는다.
fn note_click(msg: &MSG) {
    let press = match msg.message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => true,
        WM_LBUTTONUP => false,
        _ => return,
    };
    if let Some(click) = route_of(msg.hwnd, |r| r.click.clone()) {
        if press {
            click.press(Instant::now());
        } else {
            click.release(Instant::now());
        }
    }
}

/// WebView로 가는 key-up을 그 키의 누름 증명에 알린다. 메시지는 소비하지 않는다.
fn note_key_up(msg: &MSG) {
    if let Some(vk) = key_up_vk(msg.message, msg.wParam.0)
        && let Some(click) = route_of(msg.hwnd, |r| r.click.clone())
    {
        click.key_release(vk, Instant::now());
    }
}

/// 결과를 기다리는 누름의 key-up이면 붙잡는다. 붙잡았으면 true이고 winit은 그 메시지를 보내지 않는다.
fn hold_key_up(msg: &MSG) -> bool {
    let Some(vk) = release_vk(msg.message, msg.wParam.0) else {
        return false;
    };
    route_of(msg.hwnd, |r| r.replay.clone())
        .is_some_and(|replay| replay.release(vk, WinKey::Msg(MsgCopy::of(msg))))
}

/// winit 메시지 훅. 등록한 WebView로 가는 key-down을 소비했으면 true를 돌려주고, winit은 그 메시지를
/// Translate·Dispatch하지 않는다(그래서 WM_CHAR도 생기지 않는다). IME 처리 키(VK_PROCESSKEY)·
/// 변환표 밖 키는 그대로 보낸다. key-up은 결과를 기다리는 누름의 것만 붙잡는다. 다시 보낸 메시지는 판정하지 않는다.
pub(crate) fn filter_message(msg: *const c_void) -> bool {
    if msg.is_null() {
        return false;
    }
    // SAFETY: winit은 훅에 PeekMessage가 채운 유효한 MSG 포인터를 넘긴다.
    let msg = unsafe { &*msg.cast::<MSG>() };
    note_click(msg);
    note_key_up(msg);
    if take_posted(msg) {
        return false;
    }
    if hold_key_up(msg) {
        return true;
    }
    let Some(km) = key_down_message(msg.message, msg.wParam.0, msg.lParam.0) else {
        return false;
    };
    // 판정 중 콜백이 등록부를 바꿔도 되도록 빌린 상태를 풀고 부른다.
    let route = route_of(msg.hwnd, |r| {
        (
            r.surface_id,
            r.sink.clone(),
            r.target.clone(),
            r.click.clone(),
            r.replay.clone(),
        )
    });
    let Some((surface_id, sink, target, click, replay)) = route else {
        return false;
    };
    let Some(key) = vk_to_winit_key(km.vk) else {
        return false;
    };
    let mods = current_winit_mods();
    // Ctrl·Alt·Super 없는 글자 키는 다른 경로처럼 현재 배열의 문자로 비교한다. 수식키 조합은 US 물리 키 문자다.
    let key = if matches!(key, winit::keyboard::Key::Character(_))
        && !(mods.control_key() || mods.alt_key() || mods.super_key())
    {
        layout_character_key(km.vk, km.scan).unwrap_or(key)
    } else {
        key
    };
    let physical = win32_scancode_to_physical(km.scan, km.extended);
    let copy = WinKey::Msg(MsgCopy::of(msg));
    let consumed = if km.repeat {
        match replay.repeat(km.vk, copy) {
            RepeatRoute::Held | RepeatRoute::Drop => true,
            RepeatRoute::Pass(_) => false,
            RepeatRoute::Unknown(_) => {
                route_key_down(&*sink, surface_id, key, physical, mods, true, |action| {
                    super::run_editing_command(&target, action, surface_id)
                })
                .consumed()
            }
        }
    } else {
        let route = route_key_down(&*sink, surface_id, key, physical, mods, false, |action| {
            super::run_editing_command(&target, action, surface_id)
        });
        match route {
            KeyDownRoute::Forwarded(id) => replay.forwarded(id, km.vk, copy),
            KeyDownRoute::Pass | KeyDownRoute::Consumed => replay.press(km.vk),
        }
        route.consumed()
    };
    // 이 함수가 돌아간 뒤에 메시지가 Dispatch되므로 페이지가 키를 보기 전에 기록된다.
    if proves_activation(km.vk, mods, consumed) {
        click.key_press(km.vk, Instant::now());
    }
    consumed
}

/// controller를 만들고 host 입력 경로를 썼는지 함께 돌려준다. 옵션을 쓸 수 없거나 그 생성이 실패하면
/// 옵션 없이 한 번 더 만든다. `in_private`는 entry·HTTP view의 요구다. 별도 user-data 폴더와 함께
/// 로컬 WebView 브라우저 문맥을 나누고, 끝난 mirror 세션이 브라우징 프로필을 남기지 않게 한다.
///
/// # Safety
/// COM을 초기화한 GUI 스레드에서 살아 있는 environment와 자식 창 `hwnd`로 부른다.
pub(super) unsafe fn create_controller(
    env: &ICoreWebView2Environment,
    hwnd: HWND,
    in_private: bool,
    surface_id: u32,
) -> std::result::Result<(ICoreWebView2Controller, bool), String> {
    #[cfg(debug_assertions)]
    if super::debug_host_input::host_input_disabled() {
        tracing::warn!("WebView surface {surface_id}: host input processing is off for debugging");
        // SAFETY: 호출자 계약을 그대로 넘긴다.
        return unsafe { create_once(env, hwnd, in_private, false) }
            .map(|controller| (controller, false));
    }
    // SAFETY: 호출자 계약을 그대로 넘긴다.
    match unsafe { create_once(env, hwnd, in_private, true) } {
        Ok(controller) => return Ok((controller, true)),
        Err(error) => tracing::warn!(
            "WebView surface {surface_id}: host input processing is unavailable ({error}); \
             character keys without Ctrl or Alt stay with the page"
        ),
    }
    // SAFETY: 위와 같다.
    unsafe { create_once(env, hwnd, in_private, false) }.map(|controller| (controller, false))
}

/// 옵션이 필요하면 `CreateCoreWebView2ControllerWithOptions`, 아니면 일반 생성으로 한 번 만든다.
unsafe fn create_once(
    env: &ICoreWebView2Environment,
    hwnd: HWND,
    in_private: bool,
    host_input: bool,
) -> std::result::Result<ICoreWebView2Controller, String> {
    let (tx, rx) = mpsc::channel();
    let callback =
        CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |hr, controller| {
            if let Err(error) = tx.send((hr, controller)) {
                tracing::warn!(%error, "WebView2 controller handoff failed");
            }
            Ok(())
        }));
    let started = if in_private || host_input {
        let env10 = env
            .cast::<ICoreWebView2Environment10>()
            .map_err(|e| format!("ICoreWebView2Environment10 is unavailable: {e}"))?;
        // SAFETY: COM 아파트 스레드에서 살아 있는 environment로 옵션을 만들고 설정한다.
        let options = unsafe { env10.CreateCoreWebView2ControllerOptions() }
            .map_err(|e| format!("CreateControllerOptions failed: {e}"))?;
        if in_private {
            // SAFETY: 위에서 만든 옵션 객체다.
            unsafe { options.SetIsInPrivateModeEnabled(true) }
                .map_err(|e| format!("SetIsInPrivateModeEnabled failed: {e}"))?;
        }
        if host_input {
            let options4 = options
                .cast::<ICoreWebView2ControllerOptions4>()
                .map_err(|e| format!("ICoreWebView2ControllerOptions4 is unavailable: {e}"))?;
            // SAFETY: 같은 옵션 객체의 다른 인터페이스다.
            unsafe { options4.SetAllowHostInputProcessing(true) }
                .map_err(|e| format!("SetAllowHostInputProcessing failed: {e}"))?;
        }
        // SAFETY: hwnd는 호출자가 만든 살아 있는 자식 창이고 callback은 완료까지 유지된다.
        unsafe { env10.CreateCoreWebView2ControllerWithOptions(hwnd, &options, &callback) }
    } else {
        // SAFETY: 위와 같다.
        unsafe { env.CreateCoreWebView2Controller(hwnd, &callback) }
    };
    started.map_err(|e| format!("CreateController failed: {e}"))?;
    let (hr, controller) =
        webview2_com::wait_with_pump(rx).map_err(|e| format!("Controller wait failed: {e}"))?;
    match (hr, controller) {
        (Ok(()), Some(controller)) => Ok(controller),
        (hr, _) => Err(format!("controller creation completed with {hr:?}")),
    }
}
