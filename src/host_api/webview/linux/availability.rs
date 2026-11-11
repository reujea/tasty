//! Negotiate only the backend accepted by the native constructor, using actual window handles.
use gtk::prelude::ObjectExt;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

pub(crate) fn http_available(window: &(impl HasWindowHandle + HasDisplayHandle)) -> bool {
    let (Ok(window), Ok(display)) = (window.window_handle(), window.display_handle()) else {
        return false;
    };
    if !supported_handles(window.as_raw(), display.as_raw()) {
        return false;
    }
    if let Err(error) = tasty_platform::gtk_init::ensure() {
        tracing::warn!(%error, "native HTTP viewer unavailable: GTK initialization failed");
        return false;
    }
    x11_dl::xlib::Xlib::open().is_ok()
        && gtk::gdk::Display::default().is_some_and(|display| display.is::<gdkx11::X11Display>())
}
fn supported_handles(window: RawWindowHandle, display: RawDisplayHandle) -> bool {
    matches!((window, display), (RawWindowHandle::Xlib(window), RawDisplayHandle::Xlib(display))
        if window.window != 0 && display.display.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::NonNull;
    use winit::raw_window_handle::{
        WaylandDisplayHandle, WaylandWindowHandle, XlibDisplayHandle, XlibWindowHandle,
    };
    #[test]
    fn actual_handle_kinds_must_match_the_x11_factory() {
        // These handles exercise classification only; no native operation receives them.
        let xwindow = RawWindowHandle::Xlib(XlibWindowHandle::new(1));
        let xdisplay = RawDisplayHandle::Xlib(XlibDisplayHandle::new(Some(NonNull::dangling()), 0));
        let wayland = RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()));
        let wayland_display =
            RawDisplayHandle::Wayland(WaylandDisplayHandle::new(NonNull::dangling()));
        assert!(supported_handles(xwindow, xdisplay));
        assert!(!supported_handles(wayland, wayland_display));
        assert!(!supported_handles(wayland, xdisplay));
        assert!(!supported_handles(xwindow, wayland_display));
        assert!(!supported_handles(
            RawWindowHandle::Xlib(XlibWindowHandle::new(0)),
            xdisplay
        ));
        assert!(!supported_handles(
            xwindow,
            RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0))
        ));
    }
}
