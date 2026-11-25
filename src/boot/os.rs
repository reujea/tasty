//! OS 콘솔·로그·네이티브 메뉴 콜백을 초기화한다.

#[cfg(feature = "gui")]
use winit::event_loop::EventLoopProxy;

#[cfg(feature = "gui")]
use crate::AppEvent;

pub(crate) fn attach_windows_console_if_needed() {
    // GUI 서브시스템으로 링크하는 조합(src/main.rs)만 부모 콘솔에 붙는다.
    #[cfg(all(windows, not(debug_assertions), feature = "gui"))]
    {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        // SAFETY: 유효한 ATTACH_PARENT_PROCESS 상수로 부모 콘솔 연결을 요청한다.
        unsafe {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "Launching without a parent console (for example from Explorer) makes this call fail, which is expected."
            )]
            let _ = AttachConsole(ATTACH_PARENT_PROCESS); // 부모 콘솔이 없을 수 있어 연결 실패는 무시한다.
        }
    }
}

/// 패닉 기록과 stderr tracing을 초기화한다. 공유 로그 파일은 아직 열지 않는다.
pub(crate) fn init_crash_report() {
    crate::crash_report::init(env!("CARGO_PKG_VERSION"));
}

/// 호스트로 결정한 뒤에만 공유 로그 파일을 연다. CLI가 열면 실행 중 호스트의 로그를 자를 수 있다.
pub(crate) fn enable_host_file_log() {
    crate::crash_report::enable_host_file_log();
}

/// macOS 메뉴 콜백을 App 이벤트로 연결한다. 다른 플랫폼에서는 아무 일도 하지 않는다.
#[cfg(feature = "gui")]
#[allow(unused_variables)]
pub(crate) fn install_macos_delegate(proxy: &EventLoopProxy<AppEvent>) {
    #[cfg(target_os = "macos")]
    {
        let new_window_proxy = proxy.clone();
        let quit_proxy = proxy.clone();
        crate::macos_delegate::store_actions(crate::macos_delegate::DelegateActions {
            new_window: Box::new(move || {
                crate::shortcuts::send_app_event(
                    &new_window_proxy,
                    AppEvent::CreateWindow(crate::app::event::WindowRequestOrigin::User, None),
                );
            }),
            quit: Box::new(move || {
                crate::shortcuts::send_app_event(&quit_proxy, AppEvent::QuitRequested);
            }),
        });
    }
}
