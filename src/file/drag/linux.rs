use super::{Outcome, wayland, x11};
use std::{path::PathBuf, sync::Arc};
use winit::{
    dpi::PhysicalPosition,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
pub(super) enum Session {
    X11(Box<x11::Session>),
    Wayland(wayland::Session),
}
impl Session {
    pub(super) fn start(
        window: &Arc<Window>,
        paths: &[PathBuf],
        wayland: Option<&mut wayland::Context>,
    ) -> Result<Self, String> {
        let bytes = super::payload::uri_list(paths);
        let id = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            RawWindowHandle::Wayland(_) => {
                return wayland
                    .ok_or("native Wayland drag input context unavailable")?
                    .start(bytes)
                    .map(Self::Wayland);
            }
            _ => return Err("unsupported native window backend".into()),
        };
        x11::Session::start(id, bytes)
            .map(|s| Self::X11(Box::new(s)))
            .map_err(|e| e.to_string())
    }
    pub(super) fn is_x11(window: &Window) -> bool {
        window.window_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
            )
        })
    }
    pub(super) fn motion(&mut self, pos: PhysicalPosition<f64>) -> Result<(), String> {
        match self {
            Self::X11(s) => s
                .motion(pos.x as i16, pos.y as i16)
                .map_err(|e| e.to_string()),
            Self::Wayland(_) => Ok(()),
        }
    }
    pub(super) fn release(&mut self) -> Result<(), String> {
        match self {
            Self::X11(s) => s.release().map_err(|e| e.to_string()),
            Self::Wayland(s) => {
                s.release();
                Ok(())
            }
        }
    }
    pub(super) fn poll(&mut self) -> Result<Option<Outcome>, String> {
        match self {
            Self::X11(s) => s.poll().map_err(|e| e.to_string()),
            Self::Wayland(s) => s.poll(),
        }
    }
    pub(super) fn accepted(&self) -> bool {
        match self {
            Self::X11(s) => s.accepted(),
            Self::Wayland(s) => s.accepted(),
        }
    }
}
