//! 사용자가 앱을 다시 실행했을 때, OS가 붙여 준 활성화 증거로 **이미 있는** 창의 활성화를 OS에 요청한다.
//!
//! 포커스를 주는 Tasty API는 없으므로(docs/design/policies/focus.md) Tasty는 증거를 OS에 전달할 뿐이고 실제로 창을
//! 앞으로 올릴지는 창 관리자·컴포지터·포그라운드 잠금이 정한다. 호출자는 증거가 있는 외부 요청에서만
//! 이 모듈을 부른다. winit `focus_window()`는 쓰지 않는다. X11에서는 타임스탬프 없이 요청하고,
//! Windows에서는 Alt 키를 주입해 포그라운드 잠금을 우회하기 때문이다.
//!
//! - **X11**: 창에 `_NET_STARTUP_ID`를 걸고, startup id의 `_TIME<ts>`를 사용자 조작 시각으로 실어
//!   EWMH `_NET_ACTIVE_WINDOW`를 루트에 보낸다(source indication 1 = 응용). 이어서 startup-notification
//!   "remove" 메시지로 실행기의 대기 표시를 끝낸다. winit의 Xlib 연결을 그대로 쓴다.
//! - **Wayland**: 두 번째 실행이 받은 xdg-activation 토큰으로 기존 창의 surface에 `xdg_activation_v1.activate`를
//!   보낸다(winit 포크의 `WindowExtWayland::activate_with_token`). 토큰이 없거나 컴포지터가
//!   `xdg_activation_v1`을 제공하지 않으면 [`ActivationError::WaylandUnsupported`]다.
//! - **Windows**: 대상 HWND에 `SetForegroundWindow`. 두 번째 프로세스가 `AllowSetForegroundWindow`로
//!   넘긴 권한을 쓴다. 권한이 없으면 OS가 작업 표시줄 깜빡임으로 바꾼다.
//! - **macOS**: Finder·Dock 실행은 LaunchServices가 reopen으로 처리하므로 이 경로를 쓰지 않는다.

use winit::window::Window;

/// 실행기가 넘긴 활성화 증거. 값은 로그에 남기지 않는다.
#[derive(Clone, Default)]
pub struct ActivationEvidence {
    pub wayland_token: Option<String>,
    pub x11_startup_id: Option<String>,
    /// startup id의 `_TIME` 타임스탬프. 없으면 0(CurrentTime)으로 요청한다.
    pub x11_timestamp: Option<u32>,
}

/// 요청 결과. 요청을 보냈다는 뜻이며 OS가 받아들였는지는 알 수 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationRequested {
    X11ActiveWindow,
    WaylandXdgActivation,
    WindowsSetForeground,
}

/// 활성화를 요청하지 못한 이유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationError {
    /// Wayland 창인데 토큰이 없거나 컴포지터가 `xdg_activation_v1`을 제공하지 않는다.
    /// 기존 창을 앞으로 가져올 수 없으므로 호출자는 새 창 경로로 넘긴다.
    WaylandUnsupported(String),
    /// 그 밖의 실패. 로그만 남긴다.
    Other(String),
}

impl std::fmt::Display for ActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WaylandUnsupported(why) | Self::Other(why) => f.write_str(why),
        }
    }
}

/// `window`의 활성화를 OS에 요청한다.
pub fn request_activation(
    window: &Window,
    evidence: &ActivationEvidence,
) -> Result<ActivationRequested, ActivationError> {
    imp::request_activation(window, evidence)
}

/// 창 없이 startup-notification "remove" 메시지만 보낸다. 실행 중인 Tasty에 증거를 넘기지 못한
/// 두 번째 프로세스가 실행기의 대기 커서를 끝낼 때 쓴다. X11이 아니면 아무것도 하지 않는다.
pub fn x11_startup_complete(startup_id: &str) -> Result<(), String> {
    imp::x11_startup_complete(startup_id)
}

/// startup-notification 메시지 본문. 값의 공백·따옴표·역슬래시는 역슬래시로 감싼다.
pub fn startup_remove_message(startup_id: &str) -> String {
    let mut quoted = String::with_capacity(startup_id.len() + 2);
    quoted.push('"');
    for c in startup_id.chars() {
        if matches!(c, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    format!("remove: ID={quoted}")
}

/// X11 ClientMessage 하나에 실리는 바이트 수(format 8, 20바이트).
pub const STARTUP_INFO_CHUNK: usize = 20;

/// 메시지를 NUL로 끝내고 20바이트씩 나눈다. 마지막 조각은 0으로 채운다.
pub fn startup_info_chunks(message: &str) -> Vec<[u8; STARTUP_INFO_CHUNK]> {
    let mut bytes = message.as_bytes().to_vec();
    bytes.push(0);
    bytes
        .chunks(STARTUP_INFO_CHUNK)
        .map(|chunk| {
            let mut out = [0u8; STARTUP_INFO_CHUNK];
            out[..chunk.len()].copy_from_slice(chunk);
            out
        })
        .collect()
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::c_ulong;
    use std::os::raw::c_long;

    use winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use winit::window::Window;
    use x11_dl::xlib;

    use super::{ActivationError, ActivationEvidence, ActivationRequested};

    /// EWMH source indication 1 = 응용. 타임스탬프로 창 관리자의 포커스 빼앗기 방지 판정을 받는다.
    const SOURCE_APPLICATION: c_long = 1;

    fn xlib_window(window: &Window) -> Result<Option<c_ulong>, String> {
        match window
            .window_handle()
            .map_err(|e| format!("window handle: {e}"))?
            .as_raw()
        {
            RawWindowHandle::Xlib(h) => Ok(Some(h.window)),
            _ => Ok(None),
        }
    }

    fn xlib_display(window: &Window) -> Result<*mut xlib::Display, String> {
        match window
            .display_handle()
            .map_err(|e| format!("display handle: {e}"))?
            .as_raw()
        {
            RawDisplayHandle::Xlib(h) => h
                .display
                .map(|p| p.as_ptr() as *mut xlib::Display)
                .ok_or_else(|| "Xlib display handle is empty".to_string()),
            other => Err(format!("not an Xlib display handle: {other:?}")),
        }
    }

    fn intern(x: &xlib::Xlib, dpy: *mut xlib::Display, name: &std::ffi::CStr) -> xlib::Atom {
        // SAFETY: dpy 는 살아 있는 Xlib 연결이고 name 은 NUL 종단 문자열이다.
        unsafe { (x.XInternAtom)(dpy, name.as_ptr(), xlib::False) }
    }

    fn set_startup_id(x: &xlib::Xlib, dpy: *mut xlib::Display, win: c_ulong, id: &str) {
        let atom = intern(x, dpy, c"_NET_STARTUP_ID");
        let utf8 = intern(x, dpy, c"UTF8_STRING");
        // SAFETY: format 8 의 id.len() 바이트를 가리키며 win 은 살아 있는 창이다.
        unsafe {
            (x.XChangeProperty)(
                dpy,
                win,
                atom,
                utf8,
                8,
                xlib::PropModeReplace,
                id.as_ptr(),
                id.len() as i32,
            )
        };
    }

    fn send_to_root(
        x: &xlib::Xlib,
        dpy: *mut xlib::Display,
        message: xlib::XClientMessageEvent,
        mask: c_long,
    ) -> Result<(), String> {
        let mut event = xlib::XEvent {
            client_message: message,
        };
        // SAFETY: dpy 는 살아 있는 연결이다.
        let root = unsafe { (x.XDefaultRootWindow)(dpy) };
        // SAFETY: event 는 완전히 채운 ClientMessage 이고 root 는 같은 연결의 루트 창이다.
        let status = unsafe { (x.XSendEvent)(dpy, root, xlib::False, mask, &mut event) };
        if status == 0 {
            return Err("XSendEvent failed".to_string());
        }
        Ok(())
    }

    fn request_active_window(
        x: &xlib::Xlib,
        dpy: *mut xlib::Display,
        win: c_ulong,
        timestamp: u32,
    ) -> Result<(), String> {
        let mut data = xlib::ClientMessageData::new();
        data.set_long(0, SOURCE_APPLICATION);
        data.set_long(1, timestamp as c_long);
        data.set_long(2, 0);
        let message = xlib::XClientMessageEvent {
            type_: xlib::ClientMessage,
            serial: 0,
            send_event: xlib::True,
            display: dpy,
            window: win,
            message_type: intern(x, dpy, c"_NET_ACTIVE_WINDOW"),
            format: 32,
            data,
        };
        send_to_root(
            x,
            dpy,
            message,
            xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask,
        )
    }

    /// startup-notification 메시지를 `_NET_STARTUP_INFO_BEGIN` 뒤 `_NET_STARTUP_INFO` 조각으로 보낸다.
    fn send_startup_remove(
        x: &xlib::Xlib,
        dpy: *mut xlib::Display,
        sender: c_ulong,
        id: &str,
    ) -> Result<(), String> {
        let begin = intern(x, dpy, c"_NET_STARTUP_INFO_BEGIN");
        let more = intern(x, dpy, c"_NET_STARTUP_INFO");
        let message = super::startup_remove_message(id);
        for (n, chunk) in super::startup_info_chunks(&message).iter().enumerate() {
            let mut data = xlib::ClientMessageData::new();
            for (i, byte) in chunk.iter().enumerate() {
                data.set_byte(i, *byte as std::os::raw::c_char);
            }
            let event = xlib::XClientMessageEvent {
                type_: xlib::ClientMessage,
                serial: 0,
                send_event: xlib::True,
                display: dpy,
                window: sender,
                message_type: if n == 0 { begin } else { more },
                format: 8,
                data,
            };
            send_to_root(x, dpy, event, xlib::PropertyChangeMask)?;
        }
        Ok(())
    }

    /// 두 번째 실행이 받은 토큰으로 기존 Wayland 창의 활성화를 요청한다.
    fn request_wayland_activation(
        window: &Window,
        evidence: &ActivationEvidence,
    ) -> Result<ActivationRequested, ActivationError> {
        use winit::platform::wayland::WindowExtWayland;
        use winit::window::ActivationToken;

        let Some(token) = &evidence.wayland_token else {
            return Err(ActivationError::WaylandUnsupported(
                "no xdg-activation token came with the launch".to_string(),
            ));
        };
        window
            .activate_with_token(ActivationToken::from_raw(token.clone()))
            .map(|()| ActivationRequested::WaylandXdgActivation)
            .map_err(|e| {
                ActivationError::WaylandUnsupported(format!(
                    "the compositor does not offer xdg_activation_v1: {e}"
                ))
            })
    }

    pub(super) fn request_activation(
        window: &Window,
        evidence: &ActivationEvidence,
    ) -> Result<ActivationRequested, ActivationError> {
        let Some(win) = xlib_window(window).map_err(ActivationError::Other)? else {
            return request_wayland_activation(window, evidence);
        };
        x11_request_activation(window, win, evidence).map_err(ActivationError::Other)
    }

    fn x11_request_activation(
        window: &Window,
        win: c_ulong,
        evidence: &ActivationEvidence,
    ) -> Result<ActivationRequested, String> {
        let dpy = xlib_display(window)?;
        let x = xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
        if let Some(id) = &evidence.x11_startup_id {
            set_startup_id(&x, dpy, win, id);
        }
        let result = request_active_window(&x, dpy, win, evidence.x11_timestamp.unwrap_or(0));
        if let Some(id) = &evidence.x11_startup_id
            && let Err(e) = send_startup_remove(&x, dpy, win, id)
        {
            tracing::warn!("startup-notification remove failed: {e}");
        }
        // SAFETY: 같은 연결의 요청 버퍼를 비운다.
        unsafe { (x.XFlush)(dpy) };
        result.map(|()| ActivationRequested::X11ActiveWindow)
    }

    pub(super) fn x11_startup_complete(startup_id: &str) -> Result<(), String> {
        let x = xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
        // SAFETY: DISPLAY 환경변수의 디스플레이를 연다. 실패하면 null 이다.
        let dpy = unsafe { (x.XOpenDisplay)(std::ptr::null()) };
        if dpy.is_null() {
            return Err("cannot open the X display".to_string());
        }
        // SAFETY: dpy 는 위에서 연 연결이다.
        let root = unsafe { (x.XDefaultRootWindow)(dpy) };
        // SAFETY: 열린 연결에서 1x1 창을 만든다. 메시지의 보낸 창으로만 쓰고 map 하지 않는다.
        let sender = unsafe { (x.XCreateSimpleWindow)(dpy, root, 0, 0, 1, 1, 0, 0, 0) };
        let result = send_startup_remove(&x, dpy, sender, startup_id);
        // SAFETY: 위에서 만든 창을 정리한다.
        unsafe { (x.XDestroyWindow)(dpy, sender) };
        // SAFETY: 위에서 연 연결을 닫는다. XCloseDisplay 가 남은 요청을 보낸다.
        unsafe { (x.XCloseDisplay)(dpy) };
        result
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::{ActivationError, ActivationEvidence, ActivationRequested};

    pub(super) fn request_activation(
        window: &Window,
        _evidence: &ActivationEvidence,
    ) -> Result<ActivationRequested, ActivationError> {
        let hwnd = match window
            .window_handle()
            .map_err(|e| ActivationError::Other(format!("window handle: {e}")))?
            .as_raw()
        {
            RawWindowHandle::Win32(h) => HWND(h.hwnd.get() as *mut std::ffi::c_void),
            other => {
                return Err(ActivationError::Other(format!(
                    "not a Win32 window handle: {other:?}"
                )));
            }
        };
        // SAFETY: hwnd 는 winit 이 만든 살아 있는 창이다. 거절은 FALSE 반환이며 부작용이 없다.
        let granted = unsafe { SetForegroundWindow(hwnd) }.as_bool();
        if !granted {
            tracing::info!("SetForegroundWindow was refused by the foreground lock");
        }
        Ok(ActivationRequested::WindowsSetForeground)
    }

    pub(super) fn x11_startup_complete(_startup_id: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use winit::window::Window;

    use super::{ActivationError, ActivationEvidence, ActivationRequested};

    pub(super) fn request_activation(
        _window: &Window,
        _evidence: &ActivationEvidence,
    ) -> Result<ActivationRequested, ActivationError> {
        Err(ActivationError::Other(
            "activation by a launch evidence is not used on this platform".to_string(),
        ))
    }

    pub(super) fn x11_startup_complete(_startup_id: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remove_message_quotes_special_characters() {
        assert_eq!(startup_remove_message("a b"), r#"remove: ID="a b""#);
        assert_eq!(startup_remove_message(r#"q"\"#), r#"remove: ID="q\"\\""#);
    }

    #[test]
    fn the_message_is_nul_terminated_and_split_into_padded_chunks() {
        let chunks = startup_info_chunks(&"x".repeat(25));
        assert_eq!(chunks.len(), 2);
        assert_eq!(&chunks[0], &[b'x'; 20]);
        assert_eq!(&chunks[1][..5], b"xxxxx");
        assert!(chunks[1][5..].iter().all(|b| *b == 0));
        // 정확히 20바이트면 NUL 하나를 위한 조각이 더 붙는다.
        let exact = startup_info_chunks(&"y".repeat(20));
        assert_eq!(exact.len(), 2);
        assert_eq!(exact[1], [0u8; 20]);
    }
}
