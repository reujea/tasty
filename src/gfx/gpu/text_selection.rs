//! egui's text clipboard. Copies never reach egui-winit: `apply_egui_output` takes every copy out
//! of egui's output and keeps the text until the window's owner passes it to the copy door
//! (`crate::clipboard::copy`), which writes through the window's `TerminalClipboard` and shows the
//! result. Pastes on a native Wayland start come from the window's
//! [`crate::file::drag::TextSelection`], because egui-winit is built without smithay-clipboard (its
//! second data device would compete with the standing one for incoming drags) and its arboard
//! fallback speaks only X11. X11, XWayland and the other OSes keep egui-winit's paste: there the
//! window has no `TextSelection` and the paste paths below fall through to egui-winit.
use super::GpuState;
use winit::{event::WindowEvent, window::Window};

impl GpuState {
    /// Gives this window the native Wayland clipboard when the start judgment chose native
    /// Wayland. Only App windows get one: App's loop polls it (`App::poll_text_selections`).
    pub(crate) fn attach_text_selection(
        &mut self,
        window: &Window,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) {
        self.text_selection = crate::file::drag::TextSelection::new(window, display_owner);
    }

    /// Before egui sees `event`: a paste key starts reading the Wayland selection for the focused
    /// widget. egui-winit then finds its in-process clipboard empty and adds nothing itself.
    pub(super) fn route_paste_key(&mut self, event: &WindowEvent) {
        if self.text_selection.is_some()
            && is_paste_key(event, self.egui_state.egui_input().modifiers.command)
            && let Some(text) = self.request_selection_text()
        {
            self.egui_state
                .egui_input_mut()
                .events
                .push(egui::Event::Paste(text));
        }
    }

    /// Text to paste now. On a native Wayland start a foreign selection is read in the background
    /// and arrives later as a paste event for the widget focused now, so this returns `None`.
    pub(super) fn clipboard_text_now(&mut self) -> Option<String> {
        if self.text_selection.is_some() {
            return self.request_selection_text();
        }
        self.egui_state.clipboard_text()
    }

    fn request_selection_text(&mut self) -> Option<String> {
        let tag = self.focus_tag();
        self.text_selection.as_mut()?.request(tag)
    }

    /// Drops a paste still being read, when its key press was rebound to another action.
    pub fn cancel_pending_paste(&mut self) {
        if let Some(selection) = self.text_selection.as_mut() {
            selection.cancel();
        }
    }

    /// The input target a paste is meant for: the widget focused when it was asked.
    fn focus_tag(&self) -> u64 {
        self.egui_ctx
            .memory(|m| m.focused())
            .map_or(0, |id| id.value())
    }

    /// Hands egui's output to egui-winit after taking out the copied text, which waits for
    /// [`Self::take_egui_copy`]. A newer copy replaces one not yet taken.
    pub(super) fn apply_egui_output(&mut self, window: &Window, output: egui::PlatformOutput) {
        let output = hold_copied_text(&mut self.egui_copy, output);
        self.egui_state.handle_platform_output(window, output);
    }

    /// The text an egui widget of this window copied since the last call. The window's owner
    /// writes it through the copy door.
    pub fn take_egui_copy(&mut self) -> Option<String> {
        self.egui_copy.take()
    }

    /// An egui copy waits for [`Self::take_egui_copy`].
    pub fn has_egui_copy(&self) -> bool {
        self.egui_copy.is_some()
    }

    /// Collects finished pastes for the next egui frame. True while one waits and the window
    /// should redraw.
    pub fn poll_text_selection(&mut self) -> bool {
        let Some(selection) = self.text_selection.as_mut() else {
            return false;
        };
        self.held_pastes
            .add(selection.poll(), std::time::Instant::now());
        !self.held_pastes.is_empty()
    }

    /// The input of this window's next egui frame. Every frame takes its input here, so each
    /// frame's focus input is counted and a waiting paste is checked against the focus at the
    /// moment egui will apply it.
    pub fn take_egui_input(&mut self, window: &Window) -> egui::RawInput {
        let mut raw_input = self.egui_state.take_egui_input(window);
        let focused = self.focus_tag();
        let pastes = self
            .held_pastes
            .frame(&raw_input.events, focused, std::time::Instant::now());
        raw_input
            .events
            .extend(pastes.into_iter().map(egui::Event::Paste));
        raw_input
    }

    /// The Wayland selection needs polling: a paste is being read or copied text is being sent.
    pub fn text_selection_busy(&self) -> bool {
        self.text_selection
            .as_ref()
            .is_some_and(crate::file::drag::TextSelection::busy)
            || !self.held_pastes.is_empty()
    }
}

/// Frames without focus input after which egui's focus is settled. A pointer press, Tab or an
/// arrow moves focus during its own frame, but Shift+Tab only schedules the move for the start of
/// the next frame (egui 0.31 `id_next_frame`, not readable from outside egui). So the frame with
/// the input and the one after it must both pass before the focus read between frames is the one
/// egui applies a paste to.
const SETTLE_FRAMES: u8 = 2;

/// Finished pastes waiting for egui's focus to settle, each with its target tag and arrival time.
pub(crate) struct HeldPastes {
    items: Vec<(u64, String, std::time::Instant)>,
    /// Frames since the last one with focus input, up to `SETTLE_FRAMES`.
    quiet: u8,
}

impl Default for HeldPastes {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            quiet: SETTLE_FRAMES,
        }
    }
}

impl HeldPastes {
    pub(crate) fn add(&mut self, finished: Vec<(u64, String)>, now: std::time::Instant) {
        self.items
            .extend(finished.into_iter().map(|(tag, text)| (tag, text, now)));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// One egui frame with `events`, `focused` being the widget focused before it. Returns the
    /// pastes this frame may apply: none until two frames have gone by without focus input, then
    /// those still meant for `focused`. A paste that waits past `PASTE_LIMIT` is dropped.
    pub(crate) fn frame(
        &mut self,
        events: &[egui::Event],
        focused: u64,
        now: std::time::Instant,
    ) -> Vec<String> {
        self.quiet = if events.iter().any(moves_focus) {
            0
        } else {
            (self.quiet + 1).min(SETTLE_FRAMES)
        };
        self.items.retain(|(_, _, arrived)| {
            let fresh = now.duration_since(*arrived) < crate::file::drag::PASTE_LIMIT;
            if !fresh {
                tracing::debug!("clipboard paste dropped: focus kept moving");
            }
            fresh
        });
        if self.quiet < SETTLE_FRAMES || self.items.is_empty() {
            return Vec::new();
        }
        let finished = std::mem::take(&mut self.items)
            .into_iter()
            .map(|(tag, text, _)| (tag, text))
            .collect();
        pastes_for(finished, focused)
    }
}

/// Input egui turns into a focus change during the frame: a pressed pointer button, and the keys
/// egui's memory reads for focus movement (Tab, Shift+Tab, the arrows, Escape).
pub(crate) fn moves_focus(event: &egui::Event) -> bool {
    use egui::Key;
    match event {
        egui::Event::PointerButton { pressed, .. } => *pressed,
        egui::Event::Key { key, pressed, .. } => {
            *pressed
                && matches!(
                    key,
                    Key::Tab
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Escape
                )
        }
        _ => false,
    }
}

/// The finished pastes still meant for the widget focused now. One asked for a widget that has
/// since lost focus is dropped rather than typed into another field.
pub(crate) fn pastes_for(finished: Vec<(u64, String)>, focused: u64) -> Vec<String> {
    finished
        .into_iter()
        .filter_map(|(asked, text)| {
            if asked == focused {
                return Some(text);
            }
            tracing::debug!("clipboard paste dropped: its text field lost focus");
            None
        })
        .collect()
}

/// Whether `event` is a pressed key egui-winit treats as its paste command.
fn is_paste_key(event: &WindowEvent, command: bool) -> bool {
    matches!(event, WindowEvent::KeyboardInput { event, .. }
        if event.state == winit::event::ElementState::Pressed
            && paste_key(&event.logical_key, event.physical_key, command))
}

/// egui-winit's `is_paste_command` on the logical key, else the physical one. `command` is egui's
/// command modifier, Ctrl on Linux. A named logical key other than Paste never falls back to the
/// physical key here; egui does that only for named keys it cannot map, none of which sit on the
/// V position.
pub(crate) fn paste_key(
    logical: &winit::keyboard::Key,
    physical: winit::keyboard::PhysicalKey,
    command: bool,
) -> bool {
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
    let key = match logical {
        Key::Named(NamedKey::Paste) => Some(egui::Key::Paste),
        Key::Named(_) => return false,
        Key::Character(name) => egui::Key::from_name(name.as_str()),
        Key::Unidentified(_) | Key::Dead(_) => None,
    }
    .or(match physical {
        PhysicalKey::Code(KeyCode::Paste) => Some(egui::Key::Paste),
        PhysicalKey::Code(KeyCode::KeyV) => Some(egui::Key::V),
        _ => None,
    });
    key == Some(egui::Key::Paste) || (command && key == Some(egui::Key::V))
}

/// Moves every copy out of `output` into `held`. A copy still held stays unless a newer one replaces
/// it, so a frame without copies never drops one the owner has not taken yet.
pub(crate) fn hold_copied_text(
    held: &mut Option<String>,
    output: egui::PlatformOutput,
) -> egui::PlatformOutput {
    let (output, copied) = take_copied_text(output);
    if copied.is_some() {
        *held = copied;
    }
    output
}

/// Removes every copy request from `output` and returns the last copied text.
pub(crate) fn take_copied_text(
    mut output: egui::PlatformOutput,
) -> (egui::PlatformOutput, Option<String>) {
    let mut copied = None;
    output.commands.retain(|command| match command {
        egui::OutputCommand::CopyText(text) => {
            copied = Some(text.clone());
            false
        }
        _ => true,
    });
    // reason: egui 0.31 still fills the deprecated field for older callers; egui-winit reads it too.
    #[allow(deprecated)]
    let legacy = std::mem::take(&mut output.copied_text);
    if !legacy.is_empty() {
        copied = Some(legacy);
    }
    (output, copied)
}

#[cfg(test)]
#[path = "text_selection_tests.rs"]
mod tests;
