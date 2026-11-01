//! WebView2 `AllowHostInputProcessing` 키 경로.
//!
//! 이 옵션으로 만든 controller는 키보드 포커스 창(`Chrome_WidgetWin_0`)을 host GUI 스레드에 둔다.
//! 그래서 페이지로 갈 키 메시지가 winit 메시지 루프를 먼저 지나고, winit 메시지 훅이
//! Translate·Dispatch 전에 소비 여부를 정할 수 있다. `AcceleratorKeyPressed`는 Ctrl·Alt가 없는
//! 글자 키를 올리지 않으므로 그 키의 바인딩은 이 경로로만 받는다.
//! 옵션을 쓸 수 없으면(옛 runtime의 인터페이스 부재, 생성 실패) 옵션 없이 다시 만들고
//! 그 view는 기존 `AcceleratorKeyPressed` 경로를 쓴다.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::mpsc;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::{IsChild, MSG};
use windows::core::*;

use super::keyboard::{
    current_winit_mods, key_down_message, vk_to_winit_key, win32_scancode_to_physical,
};
use crate::host_api::webview::keys::WebViewKeySink;

/// 키를 받은 그 WebView에서 사용자 바인딩을 처리한다. 소비했으면 true이고 페이지·엔진에 보내지 않는다.
/// 자동 반복은 첫 누름의 결정을 따르며 다시 실행하지 않는다. 두 키 경로가 같은 순서를 쓴다.
pub(super) fn route_key(
    sink: &dyn WebViewKeySink,
    target: &ICoreWebView2,
    surface_id: u32,
    key: winit::keyboard::Key,
    physical: winit::keyboard::PhysicalKey,
    mods: winit::keyboard::ModifiersState,
    repeat: bool,
) -> bool {
    if repeat {
        return sink.consumes_repeat(surface_id, &key, &physical, mods);
    }
    if sink.blocks_edit_key(&key, &physical, mods) {
        return true;
    }
    // 사용자 편집 바인딩은 이 WebView의 포커스 frame에 엔진 편집 명령으로 실행한다.
    if let Some(action) = sink.edit_action(&key, &physical, mods) {
        super::run_editing_command(target, action, surface_id);
        return true;
    }
    sink.capture_key(surface_id, key, physical, mods)
}

struct Route {
    root: HWND,
    surface_id: u32,
    sink: Rc<dyn WebViewKeySink>,
    target: ICoreWebView2,
}

thread_local! {
    /// host 입력 경로로 만든 WebView. winit 메시지 훅과 같은 GUI 스레드에서만 쓴다.
    static ROUTES: RefCell<Vec<Route>> = const { RefCell::new(Vec::new()) };
}

/// 등록을 지우는 표지. WebView가 사라질 때 함께 버린다.
pub(super) struct RouteGuard {
    root: HWND,
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
) -> RouteGuard {
    ROUTES.with(|routes| {
        let mut routes = routes.borrow_mut();
        routes.retain(|r| r.root != root);
        routes.push(Route {
            root,
            surface_id,
            sink,
            target,
        });
    });
    RouteGuard { root }
}

/// winit 메시지 훅. 등록한 WebView로 가는 key-down을 소비했으면 true를 돌려주고, winit은 그 메시지를
/// Translate·Dispatch하지 않는다(그래서 WM_CHAR도 생기지 않는다). key-up·IME 처리 키(VK_PROCESSKEY)·
/// 변환표 밖 키는 그대로 보낸다.
pub(crate) fn filter_message(msg: *const c_void) -> bool {
    if msg.is_null() {
        return false;
    }
    // SAFETY: winit은 훅에 PeekMessage가 채운 유효한 MSG 포인터를 넘긴다.
    let msg = unsafe { &*msg.cast::<MSG>() };
    let Some(km) = key_down_message(msg.message, msg.wParam.0, msg.lParam.0) else {
        return false;
    };
    if msg.hwnd.is_invalid() {
        return false;
    }
    // 판정 중 콜백이 등록부를 바꿔도 되도록 빌린 상태를 풀고 부른다.
    let route = ROUTES.with(|routes| {
        routes
            .borrow()
            .iter()
            .find(|r| {
                // SAFETY: 두 핸들 모두 이 스레드가 받은 값이며 IsChild는 조회만 한다.
                r.root == msg.hwnd || unsafe { IsChild(r.root, msg.hwnd) }.as_bool()
            })
            .map(|r| (r.surface_id, r.sink.clone(), r.target.clone()))
    });
    let Some((surface_id, sink, target)) = route else {
        return false;
    };
    let Some(key) = vk_to_winit_key(km.vk) else {
        return false;
    };
    let physical = win32_scancode_to_physical(km.scan, km.extended);
    route_key(
        &*sink,
        &target,
        surface_id,
        key,
        physical,
        current_winit_mods(),
        km.repeat,
    )
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
