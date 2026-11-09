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
            let wayland = match wayland::Context::new(window, display_owner) {
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
    pub(crate) fn poll(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(context) = self.wayland.as_mut()
            && let Err(error) = context.poll()
        {
            tracing::warn!(%error, "native drag input context failed");
            self.wayland = None;
        }
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
