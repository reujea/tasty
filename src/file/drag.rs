//! Copy-only outbound file sessions. Starting a session does not report a completed drop.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod desktop;
#[cfg(target_os = "linux")]
mod linux;
mod payload;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_os = "linux")]
use winit::dpi::PhysicalPosition;
use winit::window::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The receiver reported accepting Copy; its filesystem work is owned by the receiver.
    Copied,
    Cancelled,
    /// The peer disappeared or never reported its terminal outcome.
    Unknown,
}

/// A file drag entering a window from another client (native Wayland only; winit reports the
/// X11 one as `WindowEvent::HoveredFile`/`DroppedFile`). Positions are surface-local logical px.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InboundEvent {
    /// The file list is known; the window starts its hover display.
    Hover {
        paths: Vec<PathBuf>,
        at: (f64, f64),
    },
    Motion {
        at: (f64, f64),
    },
    Leave,
    /// The drop target is fixed at `at`.
    Drop {
        paths: Vec<PathBuf>,
        at: (f64, f64),
    },
}

/// The standing data device that receives incoming drags exists only when the process started on
/// native Wayland (no `DISPLAY`, or its X server unreachable). X11 and XWayland keep winit's
/// `DroppedFile` path and create a data device only while dragging out.
#[cfg(target_os = "linux")]
pub(crate) fn wants_inbound(start: Option<crate::boot::display_backend::StartBackend>) -> bool {
    use crate::boot::display_backend::StartBackend;
    matches!(
        start,
        Some(StartBackend::Wayland | StartBackend::WaylandFallback)
    )
}

/// How long a paste may take from the selection to its text field: the read of a foreign
/// selection, and the wait for egui's focus to settle once it has arrived.
pub(crate) const PASTE_LIMIT: Duration = Duration::from_secs(3);

/// The text clipboard of one window on a native Wayland start, carried by the same per-connection
/// data device owner as incoming drags. egui-winit is built without smithay-clipboard and its
/// arboard fallback speaks only X11, so without this handle such windows would have only an
/// in-process clipboard. `new` returns `None` on X11 and XWayland, which keep arboard.
#[cfg(target_os = "linux")]
pub(crate) struct TextSelection(wayland::Clipboard);
#[cfg(target_os = "linux")]
impl TextSelection {
    pub(crate) fn new(
        window: &Window,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Option<Self> {
        let inbound = wants_inbound(crate::boot::display_backend::start().map(|s| s.backend));
        match wayland::Clipboard::new(window, display_owner, inbound) {
            Ok(clipboard) => clipboard.map(Self),
            Err(error) => {
                tracing::warn!(%error, "native Wayland clipboard unavailable");
                None
            }
        }
    }
    /// Makes `text` the system selection.
    pub(crate) fn copy(&mut self, text: String) {
        if let Err(error) = self.0.copy(text) {
            tracing::warn!(%error, "could not set the Wayland clipboard");
        }
    }
    /// Asks for the selection text on behalf of input target `tag`. Our own copy is returned at
    /// once; a foreign one is read in the background and arrives from `poll` with the same tag. A
    /// newer request or `cancel` drops a read still waiting.
    pub(crate) fn request(&mut self, tag: u64) -> Option<String> {
        match self.0.request(tag, wayland::Want::Text) {
            Ok(wayland::Paste::Ready(text)) => Some(text).filter(|t| !t.is_empty()),
            Ok(wayland::Paste::Pending | wayland::Paste::Empty) => None,
            Err(error) => {
                tracing::warn!(%error, "could not read the Wayland clipboard");
                None
            }
        }
    }
    pub(crate) fn cancel(&mut self) {
        self.0.cancel();
    }
    /// Runs the shared queue and returns finished pastes as `(tag, text)`.
    pub(crate) fn poll(&mut self) -> Vec<(u64, String)> {
        self.0.poll().unwrap_or_else(|error| {
            tracing::warn!(%error, "Wayland clipboard queue failed");
            Vec::new()
        })
    }
    /// A paste is being read or copied text is still being written to a receiver.
    pub(crate) fn busy(&self) -> bool {
        self.0.busy()
    }
    /// The selection a window already set up on this thread, for the Core clipboard port. `None`
    /// off the event loop thread and outside a native Wayland start.
    pub(crate) fn on_this_thread() -> Option<Self> {
        wayland::Clipboard::on_this_thread().map(Self)
    }
    /// Makes `text` the selection and also offers `png` as `image/png`.
    pub(crate) fn copy_with_png(&mut self, text: String, png: Vec<u8>) -> Result<(), String> {
        self.0.copy_with_png(text, Some(png))
    }
    /// Like `copy`, for a caller that reports the failure itself (the terminal's copy toast).
    pub(crate) fn copy_checked(&mut self, text: String) -> Result<(), String> {
        self.0.copy(text)
    }
    /// The terminal's paste: text first, a PNG only when the selection has no text. A foreign
    /// selection is read in the background and its result, failures included, comes from
    /// `poll_received`.
    pub(crate) fn request_terminal(&mut self, tag: u64) -> Result<SelectionPaste, String> {
        self.0.request(tag, wayland::Want::TextOrPng)
    }
    /// A text-only paste (an OSC 52 read) whose foreign result comes from `poll_received`.
    pub(crate) fn request_text(&mut self, tag: u64) -> Result<SelectionPaste, String> {
        self.0.request(tag, wayland::Want::Text)
    }
    /// Runs the shared queue and returns this handle's finished reads as `(tag, result)`.
    pub(crate) fn poll_received(&mut self) -> Result<Vec<(u64, SelectionReceived)>, String> {
        self.0.poll_received()
    }
    /// This handle's read is still waiting; false once it finished or a newer paste replaced it.
    pub(crate) fn waiting(&self) -> bool {
        self.0.waiting()
    }
    /// The PNG of a selection this process holds (a published capture); `None` for another app's.
    pub(crate) fn own_png(&mut self) -> Result<Option<std::sync::Arc<[u8]>>, String> {
        self.0.own_png()
    }
}
#[cfg(target_os = "linux")]
pub(crate) use wayland::{Paste as SelectionPaste, Received as SelectionReceived};
/// No other OS has a system clipboard of its own here: egui-winit's stays in use, so no value exists.
#[cfg(not(target_os = "linux"))]
pub(crate) enum TextSelection {}
#[cfg(not(target_os = "linux"))]
impl TextSelection {
    pub(crate) fn new(
        _: &Window,
        _: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Option<Self> {
        None
    }
    pub(crate) fn copy(&mut self, _: String) {
        match *self {}
    }
    pub(crate) fn request(&mut self, _: u64) -> Option<String> {
        match *self {}
    }
    pub(crate) fn cancel(&mut self) {
        match *self {}
    }
    pub(crate) fn poll(&mut self) -> Vec<(u64, String)> {
        match *self {}
    }
    pub(crate) fn busy(&self) -> bool {
        match *self {}
    }
}

pub(crate) struct Context {
    #[cfg(target_os = "linux")]
    wayland: Option<wayland::Context>,
}
impl Context {
    pub(crate) fn new(
        window: Arc<Window>,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Self {
        #[cfg(target_os = "linux")]
        {
            let inbound =
                wants_inbound(crate::boot::display_backend::start().map(|start| start.backend));
            let wayland = match wayland::Context::new(window, display_owner, inbound) {
                Ok(context) => context,
                Err(error) => {
                    tracing::warn!(%error, "native drag context unavailable");
                    None
                }
            };
            Self { wayland }
        }
        #[cfg(not(target_os = "linux"))]
        {
            drop((window, display_owner));
            Self {}
        }
    }
    /// Runs the native drag queue and returns the incoming drag events for this window.
    #[cfg(target_os = "linux")]
    pub(crate) fn poll(&mut self) -> Vec<InboundEvent> {
        let Some(context) = self.wayland.as_mut() else {
            return Vec::new();
        };
        match context.poll() {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!(%error, "native drag input context failed");
                self.wayland = None;
                Vec::new()
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn poll(&mut self) {}
    /// An incoming drag is hovering or being read, or a window has events not yet taken.
    #[cfg(target_os = "linux")]
    pub(crate) fn inbound_pending(&self) -> bool {
        self.wayland
            .as_ref()
            .is_some_and(wayland::Context::inbound_pending)
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) enum StartError {
    Failed(String),
}
#[cfg(target_os = "linux")]
impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(error) => f.write_str(error),
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) struct Session {
    #[cfg(target_os = "linux")]
    backend: linux::Session,
    // Keep the native window/display owner alive until all native resources have been released.
    _window: Arc<Window>,
}
#[cfg(target_os = "linux")]
impl Session {
    pub(crate) fn start(
        window: Arc<Window>,
        paths: &[PathBuf],
        context: &mut Context,
    ) -> Result<Self, StartError> {
        payload::validate(paths).map_err(StartError::Failed)?;
        #[cfg(target_os = "linux")]
        {
            let backend = linux::Session::start(&window, paths, context.wayland.as_mut())
                .map_err(StartError::Failed)?;
            Ok(Self {
                backend,
                _window: window,
            })
        }
    }
    /// Whether a session should exist before the pointer leaves the window.
    #[cfg(target_os = "linux")]
    pub(crate) fn owns_selection_before_leaving(window: &Window) -> bool {
        linux::Session::is_x11(window)
    }
    pub(crate) fn motion(&mut self, position: PhysicalPosition<f64>) -> Result<(), String> {
        self.backend.motion(position)
    }
    pub(crate) fn release(&mut self) -> Result<(), String> {
        self.backend.release()
    }
    pub(crate) fn poll(&mut self) -> Result<Option<Outcome>, String> {
        self.backend.poll()
    }
    pub(crate) fn accepted(&self) -> bool {
        self.backend.accepted()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::wants_inbound;
    use crate::boot::display_backend::StartBackend;
    #[test]
    fn only_a_native_wayland_start_keeps_a_standing_device() {
        assert!(wants_inbound(Some(StartBackend::Wayland)));
        assert!(wants_inbound(Some(StartBackend::WaylandFallback)));
        assert!(!wants_inbound(Some(StartBackend::X11)));
        assert!(!wants_inbound(Some(StartBackend::Unset)));
        assert!(!wants_inbound(None));
    }
}
