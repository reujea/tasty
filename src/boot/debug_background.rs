//! debug 빌드의 배경 실행. `TASTY_DEBUG_BACKGROUND=1`로 띄우면 macOS에서 실행이 끝날 때 앱을
//! 활성화하지 않고, 숨겨 만든 첫 창을 키 창으로 만들지 않은 채 모든 창 뒤에 둔다. 사용자가 쓰는
//! 화면의 앞 창과 키 입력을 가져가지 않고 격리 인스턴스를 검증하기 위한 것이다
//! ([debug IPC](../../docs/dev-guide/debug-ipc.md#배경-실행-tasty_debug_background)).
//! 다른 OS와 변수가 없는 실행은 기존 동작을 유지한다.

#[cfg(target_os = "macos")]
const ENV: &str = "TASTY_DEBUG_BACKGROUND";

/// 이 실행이 배경 실행을 요청했는가. 값이 정확히 `1`일 때만 참이다.
#[cfg(target_os = "macos")]
fn requested() -> bool {
    std::env::var_os(ENV).is_some_and(|v| v == "1")
}

/// macOS에서 실행 완료 때의 앱 활성화(winit 기본값)를 끈다.
#[cfg(target_os = "macos")]
pub(crate) fn configure_event_loop(
    builder: &mut winit::event_loop::EventLoopBuilder<crate::AppEvent>,
) {
    use winit::platform::macos::EventLoopBuilderExtMacOS;
    if requested() {
        builder.with_activate_ignoring_other_apps(false);
    }
}

/// 숨겨 만든 첫 창(정상 부팅·셸 설정·부팅 오류·home 사용 중 화면)을 보인다. 배경 실행이면 macOS에서 키 창으로 만들지 않고 모든 창 뒤에 둔다.
/// 뒤로 보내지 못하면 앞 창을 가져가지 않도록 창을 숨긴 채 두고 오류를 남긴다. IPC는 계속 받는다.
/// 배경 실행이 아니면 `set_visible(true)`로 보인다.
pub(crate) fn show_boot_window(window: &winit::window::Window) {
    #[cfg(target_os = "macos")]
    if requested() {
        if let Err(e) = order_back(window) {
            tracing::error!(
                "debug background launch: ordering the first window back failed ({e}); the window stays hidden"
            );
        }
        return;
    }
    window.set_visible(true);
}

#[cfg(target_os = "macos")]
fn order_back(window: &winit::window::Window) -> Result<(), String> {
    use objc2_app_kit::NSView;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let ns_view_ptr = match window
        .window_handle()
        .map_err(|e| format!("window handle: {e}"))?
        .as_raw()
    {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
        other => return Err(format!("not an AppKit window handle: {other:?}")),
    };
    // SAFETY: ns_view_ptr는 winit이 살려 두는 창의 NSView다. 호출 동안 `&Window`를 빌리고 있고,
    // 이 함수는 부팅 프레임을 그리는 winit 이벤트 루프(main thread)에서만 불린다.
    let ns_view: &NSView = unsafe { &*(ns_view_ptr as *const NSView) };
    let ns_window = ns_view
        .window()
        .ok_or_else(|| "NSView has no NSWindow".to_string())?;
    ns_window.orderBack(None);
    Ok(())
}
