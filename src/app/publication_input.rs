//! Transient event-loop inputs held while authorized resources and their final facts are published.
//! Raw input is never journalled. A resumed event must still address the same View and resource.
use crate::AppEvent;
use std::collections::VecDeque;

const MAX_EVENTS: usize = 256;
const MAX_BYTES: usize = tasty_ipc::admission::QUEUED_BYTES_LIMIT;

pub(crate) enum DeferredEvent {
    App(AppEvent),
    #[cfg(feature = "gui")]
    Window {
        window: winit::window::WindowId,
        event: winit::event::WindowEvent,
        target: Option<InputTarget>,
    },
}

#[cfg(feature = "gui")]
#[derive(Clone)]
pub(crate) struct InputTarget {
    engine: crate::runtime::engine_session::EngineId,
    surface: Option<u32>,
    resource: Option<tasty_terminal::ResourceGeneration>,
    remote: Option<crate::plugin_bridge::host_cmd::SurfaceBinding>,
    revision: Option<u64>,
    activation: Option<u64>,
    host_focus: Option<(Option<crate::model::popup_kind::PopupId>, Option<egui::Id>)>,
}

#[derive(Default)]
pub(crate) struct PublicationInputs {
    events: VecDeque<(DeferredEvent, usize)>,
    bytes: usize,
    pub rejected: usize,
    #[cfg(feature = "gui")]
    targets: std::collections::HashMap<winit::window::WindowId, InputTarget>,
    #[cfg(feature = "gui")]
    rejected_windows: std::collections::HashSet<winit::window::WindowId>,
    #[cfg(feature = "gui")]
    reset_all_gestures: bool,
}

impl PublicationInputs {
    pub fn push(&mut self, mut event: DeferredEvent) -> Result<(), DeferredEvent> {
        if matches!(event, DeferredEvent::App(AppEvent::TerminalOutput(_))) {
            event = DeferredEvent::App(AppEvent::TerminalOutput(None));
        }
        // Wake hints carry no payload. Their source queue/Terminal retains all data in arrival order.
        let app_event = match &event {
            DeferredEvent::App(event) => Some(event),
            #[cfg(feature = "gui")]
            _ => None,
        };
        if let Some(event) = app_event {
            let duplicate = self.events.iter().any(|(old, _)| match old {
                DeferredEvent::App(old) => wake_class(old)
                    .zip(wake_class(event))
                    .is_some_and(|(a, b)| a == b),
                #[cfg(feature = "gui")]
                _ => false,
            });
            if duplicate {
                return Ok(());
            }
        }
        #[cfg(feature="gui")]
        if let DeferredEvent::Window {window,event:winit::event::WindowEvent::RedrawRequested,..}=&event
            && self.events.iter().any(|(old,_)|matches!(old,DeferredEvent::Window {window:old,event:winit::event::WindowEvent::RedrawRequested,..} if old==window)) {
                return Ok(());
            }
        let weight = weight(&event);
        if self.events.len() >= MAX_EVENTS || self.bytes.saturating_add(weight) > MAX_BYTES {
            self.rejected = self.rejected.saturating_add(1);
            #[cfg(feature = "gui")]
            if let DeferredEvent::Window { window, .. } = &event {
                if self.rejected_windows.len() < MAX_EVENTS {
                    self.rejected_windows.insert(*window);
                } else {
                    self.reset_all_gestures = true;
                }
            }
            return Err(event);
        }
        self.bytes += weight;
        self.events.push_back((event, weight));
        Ok(())
    }
    pub fn pop(&mut self) -> Option<DeferredEvent> {
        let (event, weight) = self.events.pop_front()?;
        self.bytes -= weight;
        Some(event)
    }
}

fn wake_class(event: &AppEvent) -> Option<u8> {
    match event {
        AppEvent::TerminalOutput(_) => Some(0),
        AppEvent::IpcReady => Some(1),
        AppEvent::StreamReady => Some(2),
        AppEvent::JournalReady => Some(3),
        #[cfg(feature = "gui")]
        AppEvent::AttachClientData => Some(4),
        #[cfg(feature = "gui")]
        AppEvent::AutoAttachReady => Some(5),
        #[cfg(feature = "gui")]
        AppEvent::TimerTick => Some(6),
        _ => None,
    }
}

fn weight(event: &DeferredEvent) -> usize {
    let base = std::mem::size_of::<DeferredEvent>();
    match event {
        #[cfg(feature = "gui")]
        DeferredEvent::App(AppEvent::CreateWindow(_, completion)) => {
            base + completion
                .as_ref()
                .map_or(0, crate::app::event::IpcCompletion::retained_bytes)
        }
        #[cfg(feature = "gui")]
        DeferredEvent::App(AppEvent::RunLuaScript { source, name, .. }) => {
            base + source.len() + name.len()
        }
        #[cfg(feature = "gui")]
        DeferredEvent::App(AppEvent::IdentifyDone {
            target, detector, ..
        }) => base + format!("{target:?}{detector:?}").len(),
        #[cfg(feature = "gui")]
        DeferredEvent::Window { event, .. } => {
            base + match event {
                winit::event::WindowEvent::Ime(
                    winit::event::Ime::Commit(text) | winit::event::Ime::Preedit(text, _),
                ) => text.len(),
                winit::event::WindowEvent::KeyboardInput { event, .. } => {
                    event.text.as_ref().map_or(0, |text| text.len())
                        + match &event.logical_key {
                            winit::keyboard::Key::Character(text) => text.len(),
                            _ => 0,
                        }
                }
                winit::event::WindowEvent::DroppedFile(path)
                | winit::event::WindowEvent::HoveredFile(path) => path.as_os_str().len(),
                _ => 0,
            }
        }
        _ => base,
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn capture_publication_input(
        &self,
        window: winit::window::WindowId,
        event: &winit::event::WindowEvent,
    ) -> Option<InputTarget> {
        use winit::event::WindowEvent as E;
        // Resize/redraw/close/focus belong to the OS window, not its selected surface.
        if !matches!(
            event,
            E::KeyboardInput { .. }
                | E::Ime(_)
                | E::MouseInput { .. }
                | E::MouseWheel { .. }
                | E::CursorMoved { .. }
                | E::Touch(_)
                | E::DroppedFile(_)
        ) {
            return None;
        }
        self.publication_inputs
            .targets
            .get(&window)
            .cloned()
            .or_else(|| self.live_input_target(window))
    }

    fn live_input_target(&self, window: winit::window::WindowId) -> Option<InputTarget> {
        let engine = self.engines.of_window(window)?;
        let view = self.view.views.get(&window)?.as_main()?;
        let owner = self.engines.get(engine)?;
        let surface = view.state.focused_surface_id(owner.core);
        let resource = surface.and_then(|id| {
            self.journal
                .input_generation(engine, id)
                .unwrap_or_else(|| owner.runtime.terminals.generation(id))
        });
        let remote = surface
            .and_then(|id| owner.find_surface_by_id(id))
            .and_then(|surface| {
                surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            })
            .map(|surface| surface.handles().binding());
        Some(InputTarget {
            engine,
            surface,
            resource,
            remote,
            revision: owner.core.committed_structure_revision(),
            host_focus: host_keyboard_focus(view),
            activation: surface.and_then(|id| {
                owner
                    .core
                    .find_surface_by_id(id)
                    .and_then(|surface| surface.activation_generation)
            }),
        })
    }

    pub(crate) fn publication_input_is_current(
        &self,
        window: winit::window::WindowId,
        event: &winit::event::WindowEvent,
        target: &InputTarget,
    ) -> bool {
        if self.engines.of_window(window) != Some(target.engine) {
            return false;
        }
        let Some(view) = self.view.views.get(&window).and_then(|view| view.as_main()) else {
            return false;
        };
        let Some(owner) = self.engines.get(target.engine) else {
            return false;
        };
        if matches!(
            event,
            winit::event::WindowEvent::KeyboardInput { .. } | winit::event::WindowEvent::Ime(_)
        ) && target.host_focus.is_some()
        {
            return host_keyboard_focus(view) == target.host_focus;
        }
        if view.state.focused_surface_id(owner.core) != target.surface {
            return false;
        }
        if target
            .surface
            .and_then(|id| owner.runtime.terminals.generation(id))
            != target.resource
        {
            return false;
        }
        if let Some(binding) = &target.remote {
            let Some(remote) = target
                .surface
                .and_then(|id| owner.find_surface_by_id(id))
                .and_then(|surface| {
                    surface
                        .as_any()
                        .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                })
            else {
                return false;
            };
            if !binding.matches(&remote.handles()) {
                return false;
            }
        }
        let geometry = matches!(
            event,
            winit::event::WindowEvent::MouseInput { .. }
                | winit::event::WindowEvent::MouseWheel { .. }
                | winit::event::WindowEvent::CursorMoved { .. }
                | winit::event::WindowEvent::Touch(_)
                | winit::event::WindowEvent::DroppedFile(_)
        );
        if target.surface.and_then(|id| {
            owner
                .core
                .find_surface_by_id(id)
                .and_then(|surface| surface.activation_generation)
        }) != target.activation
        {
            return false;
        }
        if geometry {
            return owner.core.committed_structure_revision() == target.revision;
        }
        true
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn capture_published_input_targets(&mut self) {
        if self.journal.pauses_observation() {
            return;
        }
        self.publication_inputs.targets = self
            .view
            .views
            .keys()
            .filter_map(|window| {
                self.live_input_target(*window)
                    .map(|target| (*window, target))
            })
            .collect();
    }

    pub(crate) fn defer_publication_event(&mut self, event: DeferredEvent) {
        if let Err(event) = self.publication_inputs.push(event) {
            if let DeferredEvent::App(AppEvent::CreateWindow(_, Some(completion))) = event {
                completion.reply_err(
                    crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL,
                    "publication input queue is full",
                );
            }
            tracing::warn!("publication input queue rejected an event at its capacity limit");
        }
    }

    pub(crate) fn resume_publication_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        use winit::application::ApplicationHandler;
        while !self.journal.pauses_observation()
            && !self.journal.is_halted()
            && self.state.shutdown.is_none()
        {
            let Some(event) = self.publication_inputs.pop() else {
                break;
            };
            match event {
                DeferredEvent::App(event) => self.user_event(event_loop, event),
                DeferredEvent::Window {
                    window,
                    event,
                    target,
                } => {
                    if target.as_ref().is_none_or(|target| {
                        self.publication_input_is_current(window, &event, target)
                    }) {
                        self.window_event(event_loop, window, event);
                    } else {
                        self.cancel_publication_gesture(window);
                        tracing::debug!(
                            "discarding transient input for a retired View or resource binding"
                        );
                    }
                }
            }
        }
        if !self.journal.pauses_observation() {
            self.publication_inputs.targets.clear();
        }
        let rejected = std::mem::take(&mut self.publication_inputs.rejected);
        if rejected > 0 {
            let windows = if std::mem::take(&mut self.publication_inputs.reset_all_gestures) {
                self.view.views.keys().copied().collect()
            } else {
                std::mem::take(&mut self.publication_inputs.rejected_windows)
            };
            for window in windows {
                self.cancel_publication_gesture(window);
            }
            for view in self.view.views.values_mut() {
                // An overflow cannot leave a modifier latched from a discarded release event.
                view.base_mut().state.modifiers = winit::keyboard::ModifiersState::empty();
                if let Some(main) = view.as_main_mut() {
                    main.state.toasts.push_info(
                        crate::i18n::t("toast.input_queue_full").to_string(),
                        crate::model::toast_kind::ToastScope::Window,
                    );
                }
            }
        }
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    fn cancel_publication_gesture(&mut self, window: winit::window::WindowId) {
        let Some(main) = self
            .view
            .views
            .get_mut(&window)
            .and_then(|view| view.as_main_mut())
        else {
            return;
        };
        if let Some(drag) = main.dragging_divider.take() {
            main.state.layout_previews.cancel(drag.sequence);
        }
        main.left_mouse_down = false;
        main.left_select_bypass = false;
        main.report_buttons_down.clear();
        main.link_click_consumed = false;
        main.right_link_press = None;
        main.last_mouse_report_cell = None;
        main.hovered_link = None;
        main.state.popups.cancel_pointer_interactions();
        main.state.pending_resize_cursor = None;
        if let Some(selection) = &mut main.text_selection {
            selection.dragging = false;
        }
        main.ime_preedit = None;
        main.ime_cursor_advance = 0;
        main.base.state.dirty = true;
    }
}

#[cfg(feature = "gui")]
fn host_keyboard_focus(
    view: &crate::view::main::MainView,
) -> Option<(Option<crate::model::popup_kind::PopupId>, Option<egui::Id>)> {
    (view.state.popups.has_focused()
        || view.state.has_input_dialog_open()
        || view.state.plugin_popup_open
        || view.state.tutorial.keyboard_focus)
        .then(|| {
            (
                view.state
                    .popups
                    .focused_dismissal_target()
                    .map(|(id, _, _)| id),
                view.base.gpu.egui_ctx.memory(|memory| memory.focused()),
            )
        })
}
