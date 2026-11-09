//! Per-View gesture ownership. No native callback can mutate a different View or Surface.
use super::MainView;
use crate::runtime::{engine_read::EngineRead, surface_binding::SurfaceBinding};
use std::path::PathBuf;
use winit::event::{ElementState, MouseButton, WindowEvent};

#[derive(Default)]
pub(crate) struct Gesture {
    context: Option<crate::file::drag::Context>,
    held: bool,
    handed_off: bool,
    source: Option<(SurfaceBinding, Vec<PathBuf>)>,
    #[cfg(target_os = "linux")]
    native: Option<crate::file::drag::Session>,
    /// X11 session opened while the pointer is still inside. Not handed off yet.
    #[cfg(target_os = "linux")]
    armed: Option<(Vec<PathBuf>, crate::file::drag::Session)>,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    desktop: Option<crate::file::drag::desktop::Handle>,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pending: Option<crate::file::drag::desktop::Request>,
    #[cfg(target_os = "macos")]
    event: Option<crate::file::drag::desktop::GestureEvent>,
}
impl Gesture {
    pub(crate) fn new(
        window: std::sync::Arc<winit::window::Window>,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Self {
        Self {
            context: Some(crate::file::drag::Context::new(window, display_owner)),
            ..Self::default()
        }
    }
}
impl MainView {
    pub(crate) fn has_outbound_drag(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.file_drag.native.is_some()
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.file_drag.desktop.is_some()
        }
    }

    pub(super) fn cancel_outbound_drag(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.file_drag.native = None;
            self.file_drag.armed = None;
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.file_drag.desktop = None;
            self.file_drag.pending = None;
        }
        self.file_drag.source = None;
        self.base
            .winit
            .set_cursor(winit::window::CursorIcon::Default);
    }

    pub(crate) fn poll_outbound_drag(&mut self, engine: &EngineRead<'_>) {
        if let Some(context) = self.file_drag.context.as_mut() {
            context.poll();
        }
        if self.state.fullscreen_stage_active()
            || self.state.keyboard_overlay_open()
            || self.base.state.close_requested
            || self
                .file_drag
                .source
                .as_ref()
                .is_some_and(|(binding, _)| !binding.current_in(engine))
        {
            self.cancel_outbound_drag();
            return;
        }
        #[cfg(target_os = "linux")]
        if let Some(native) = self.file_drag.native.as_mut() {
            match native.poll() {
                Ok(Some(outcome)) => {
                    tracing::debug!(?outcome, "outbound file drag ended");
                    self.cancel_outbound_drag();
                    self.file_drag.held = false;
                    self.file_drag.handed_off = false;
                }
                Ok(None) => self.base.winit.set_cursor(if native.accepted() {
                    winit::window::CursorIcon::Copy
                } else {
                    winit::window::CursorIcon::NoDrop
                }),
                Err(error) => self.outbound_failed(error),
            }
        }
        if !self.has_outbound_drag() && self.file_drag.held && !self.file_drag.handed_off {
            // The payload is created by Explorer after its existing threshold and selection logic.
            self.file_drag.source = crate::explorer_ui::view::drag::outbound_payload(
                &self.base.gpu.egui_ctx,
                &self.state.explorer_views,
            )
            .and_then(|(sid, paths)| {
                SurfaceBinding::capture(engine, sid).map(|binding| (binding, paths))
            });
            #[cfg(target_os = "linux")]
            self.arm_outbound_drag();
        }
    }

    /// GNOME bridges an X11 drag to Wayland windows only when `XdndSelection` changes owner while
    /// the pointer is over an X11 window, so the owner must be set before the pointer leaves.
    /// An internal drag is unaffected: the session stays silent until it is handed off.
    #[cfg(target_os = "linux")]
    fn arm_outbound_drag(&mut self) {
        let Some((_, paths)) = self.file_drag.source.as_ref() else {
            self.file_drag.armed = None;
            return;
        };
        if self
            .file_drag
            .armed
            .as_ref()
            .is_some_and(|(armed, _)| armed == paths)
        {
            return;
        }
        self.file_drag.armed = None;
        if !crate::file::drag::Session::owns_selection_before_leaving(&self.base.winit) {
            return;
        }
        let paths = paths.clone();
        match crate::file::drag::Session::start(
            self.base.winit.clone(),
            &paths,
            self.file_drag.context.as_mut().expect("View drag context"),
        ) {
            Ok(session) => self.file_drag.armed = Some((paths, session)),
            // Leaving the window tries again and reports a failure to the user there.
            Err(error) => tracing::debug!(%error, "could not arm the outbound file drag"),
        }
    }

    fn outbound_failed(&mut self, error: String) {
        tracing::warn!(%error, "outbound file drag failed");
        self.cancel_outbound_drag();
        self.state.toasts.push_info(
            crate::i18n::t("explorer.drag.native_unavailable"),
            crate::adapters::ui::ToastScope::Window,
        );
        self.base.state.dirty = true;
        self.base.winit.request_redraw();
    }

    pub(super) fn outbound_drag_event(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
        modal: bool,
    ) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(event) = crate::file::drag::desktop::GestureEvent::capture(&self.base.winit) {
            self.file_drag.event = Some(event);
        }
        if modal {
            self.cancel_outbound_drag();
        }
        if matches!(
            event,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            }
        ) {
            if self.has_outbound_drag() {
                return true;
            }
            #[cfg(target_os = "macos")]
            {
                self.file_drag.event =
                    crate::file::drag::desktop::GestureEvent::capture(&self.base.winit);
            }
            self.file_drag.held = true;
            self.file_drag.handed_off = false;
            self.file_drag.source = None;
            #[cfg(target_os = "linux")]
            {
                self.file_drag.armed = None;
            }
        }
        if self.has_outbound_drag()
            && let WindowEvent::KeyboardInput { event, .. } = event
            && event.state == ElementState::Pressed
            && crate::shortcuts::matches_any_binding(
                &engine.settings.keybindings.explorer_clear_selection,
                &crate::shortcuts::shortcut_lookup_key(
                    &event.logical_key,
                    &event.physical_key,
                    self.base.state.modifiers,
                ),
                self.base.state.modifiers,
            )
        {
            self.cancel_outbound_drag();
            return true;
        }
        let outside = match event {
            WindowEvent::CursorLeft { .. } => true,
            WindowEvent::CursorMoved { position, .. } => {
                let size = self.base.winit.inner_size();
                position.x < 0.0
                    || position.y < 0.0
                    || position.x >= f64::from(size.width)
                    || position.y >= f64::from(size.height)
            }
            _ => false,
        };
        if outside && self.file_drag.held && !self.file_drag.handed_off && !modal {
            self.poll_outbound_drag(engine);
            if let Some((binding, paths)) = self.file_drag.source.as_ref()
                && binding.current_in(engine)
            {
                self.file_drag.handed_off = true;
                // Linux sessions are asynchronous and dispatch only their own queues.
                #[cfg(target_os = "linux")]
                match match self.file_drag.armed.take() {
                    Some((armed, session)) if &armed == paths => Ok(session),
                    _ => crate::file::drag::Session::start(
                        self.base.winit.clone(),
                        paths,
                        self.file_drag.context.as_mut().expect("View drag context"),
                    ),
                } {
                    Ok(session) => self.file_drag.native = Some(session),
                    Err(error) => self.outbound_failed(error.to_string()),
                }
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    let (request, handle) = crate::file::drag::desktop::Request::new(
                        self.base.winit.clone(),
                        paths.clone(),
                    );
                    #[cfg(target_os = "macos")]
                    let request = {
                        let mut request = request;
                        request.event = self.file_drag.event.take();
                        request
                    };
                    self.file_drag.desktop = Some(handle);
                    self.file_drag.pending = Some(request);
                }
                self.base.gpu.release_drag_pointer();
                self.left_mouse_down = false;
                self.base.winit.request_redraw();
            }
        }
        let released = matches!(
            event,
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            }
        );
        if released {
            self.file_drag.held = false;
            #[cfg(target_os = "linux")]
            {
                self.file_drag.armed = None;
            }
        }
        if !self.file_drag.handed_off {
            return false;
        }
        #[cfg(target_os = "linux")]
        if let Some(native) = self.file_drag.native.as_mut() {
            let result = match event {
                WindowEvent::CursorMoved { position, .. } => native.motion(*position),
                _ if released => native.release(),
                _ => Ok(()),
            };
            if let Err(error) = result {
                self.outbound_failed(error);
            }
        }
        // A return to the source stays native until the next press, never an internal release.
        let consumed = matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorLeft { .. }
                | WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    ..
                }
        );
        if released && !self.has_outbound_drag() {
            self.file_drag.handed_off = false;
        }
        consumed
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl MainView {
    pub(crate) fn take_outbound_request(&mut self) -> Option<crate::file::drag::desktop::Request> {
        self.file_drag.pending.take()
    }
    pub(crate) fn finish_outbound_request(
        &mut self,
        id: u64,
        result: Result<crate::file::drag::Outcome, String>,
        engine: &EngineRead<'_>,
    ) {
        if self
            .file_drag
            .desktop
            .as_ref()
            .is_none_or(|handle| handle.id != id)
        {
            return;
        }
        if self
            .file_drag
            .source
            .as_ref()
            .is_none_or(|(binding, _)| !binding.current_in(engine))
        {
            self.cancel_outbound_drag();
            return;
        }
        match result {
            Ok(outcome) => {
                tracing::debug!(?outcome, "outbound file drag ended");
                self.cancel_outbound_drag();
            }
            Err(error) => self.outbound_failed(error),
        }
        self.file_drag.held = false;
        self.file_drag.handed_off = false;
    }
}
