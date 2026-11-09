//! winit 이벤트 루프와 proxy를 만든다.

use winit::event_loop::{EventLoop, EventLoopProxy};

use crate::AppEvent;

/// 메시지 훅이 이벤트를 보낼 proxy. 훅은 이벤트 루프보다 먼저 만들어지므로 빌드 뒤 채운다.
#[cfg(windows)]
static ACTIVATE_PROXY: std::sync::OnceLock<EventLoopProxy<AppEvent>> = std::sync::OnceLock::new();

/// macOS 기본 메뉴를 끄고 Tasty가 직접 종료·새 창 동작을 등록한다.
/// Windows는 두 번째 프로세스가 보낸 등록 메시지 "활성화"와 WebView로 가는 키를 메시지 훅으로 받는다.
pub(crate) fn build() -> anyhow::Result<(EventLoop<AppEvent>, EventLoopProxy<AppEvent>)> {
    let mut builder = EventLoop::<AppEvent>::with_user_event();
    #[cfg(target_os = "linux")]
    let x_connection = crate::boot::display_backend::configure(&mut builder);
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
        #[cfg(debug_assertions)]
        crate::boot::debug_background::configure_event_loop(&mut builder);
    }
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        // 두 번째 프로세스가 보낸 활성화 등록 메시지와 WebView로 가는 키를 같은 훅이 판정한다.
        let activate = crate::boot::single_instance::service_enabled()
            .then(crate::platform::single_instance_windows::activate_message);
        builder.with_msg_hook(move |msg| {
            if let Some(activate) = activate {
                // SAFETY: winit은 훅에 유효한 MSG 포인터를 넘긴다.
                if unsafe {
                    crate::platform::single_instance_windows::is_activate_message(msg, activate)
                } {
                    match ACTIVATE_PROXY.get() {
                        Some(proxy) => crate::shortcuts::send_app_event(
                            proxy,
                            AppEvent::ExternalActivate(
                                crate::boot::single_instance::ExternalActivation {
                                    evidence: Default::default(),
                                    os_granted: true,
                                },
                            ),
                        ),
                        None => {
                            tracing::warn!("activate message arrived before the event loop proxy")
                        }
                    }
                    return true;
                }
            }
            crate::webview::filter_host_input(msg)
        });
    }
    let event_loop = builder.build()?;
    #[cfg(target_os = "linux")]
    drop(x_connection);
    let proxy = event_loop.create_proxy();
    #[cfg(windows)]
    if ACTIVATE_PROXY.set(proxy.clone()).is_err() {
        tracing::warn!("event loop proxy for the activate message was already set");
    }
    Ok((event_loop, proxy))
}
