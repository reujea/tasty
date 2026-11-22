//! egui's text clipboard. Copies never reach egui-winit: `apply_egui_output` takes every copy out
//! of egui's output and keeps the text until the window's owner passes it to the copy door
//! (`crate::clipboard::copy`), which writes through the window's `TerminalClipboard` and shows the
//! result. Pastes on a native Wayland start come from the window's
//! [`crate::file::drag::TextSelection`], because egui-winit is built without smithay-clipboard (its
//! second data device would compete with the standing one for incoming drags) and its arboard
//! fallback speaks only X11. X11, XWayland and the other OSes keep egui-winit's paste: there the
//! window has no `TextSelection` and the paste paths below fall through to egui-winit.
use super::GpuState;
use super::plugin_paste_read::{PluginClip, PluginPasteEnd};
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

    /// The clipboard text a paste key sent to a plugin text field would insert, read at key time.
    /// A native Wayland read of another app's selection is waited for only when `wait` (the
    /// field's plugin replies with edit outcomes): it gets a tag of its own and its result comes
    /// from [`Self::take_plugin_paste_arrivals`]. Otherwise it is cancelled and reads as empty.
    pub(crate) fn plugin_paste(&mut self, wait: bool) -> PluginClip {
        let tag = if wait {
            self.plugin_reads.next_tag()
        } else {
            self.focus_tag()
        };
        let Some(selection) = self.text_selection.as_mut() else {
            return match self.egui_state.clipboard_text().filter(|t| !t.is_empty()) {
                Some(text) => PluginClip::Text(text),
                None => PluginClip::Empty,
            };
        };
        match selection.request_held_text(tag) {
            Some(Some(text)) => PluginClip::Text(text),
            Some(None) => PluginClip::Empty,
            None if wait => {
                self.plugin_reads.started(tag);
                PluginClip::Pending(tag)
            }
            None => {
                selection.cancel();
                PluginClip::Empty
            }
        }
    }

    /// Stops waiting for the plugin paste read `tag`, cancelling it when it is still being read.
    /// Its result, if it comes late, is dropped.
    pub(crate) fn cancel_plugin_paste(&mut self, tag: u64) {
        if self.plugin_reads.cancel(tag)
            && let Some(selection) = self.text_selection.as_mut()
            && selection.waiting()
        {
            selection.cancel();
        }
    }

    /// Plugin paste reads that ended since the last call, for App to send or replay.
    pub(crate) fn take_plugin_paste_arrivals(&mut self) -> Vec<(u64, PluginPasteEnd)> {
        self.plugin_reads.take_arrived()
    }

    /// The plugin popup that receives this window's keys, set by the window before keys reach egui
    /// and before each egui frame. A paste asked while it is set is meant for that popup.
    pub fn set_plugin_paste_target(&mut self, instance: Option<u64>) {
        self.plugin_paste_target = instance;
    }

    /// The input target a paste is meant for: the plugin popup receiving keys, else the widget
    /// focused when it was asked.
    pub(super) fn focus_tag(&self) -> u64 {
        if let Some(instance) = self.plugin_paste_target {
            return plugin_popup_tag(instance);
        }
        self.egui_ctx
            .memory(|m| m.focused())
            .map_or(0, |id| id.value())
    }

    /// Hands egui's output to egui-winit after taking out the copied text, which waits for
    /// [`Self::take_egui_copy`]. A newer copy replaces one not yet taken.
    pub(super) fn apply_egui_output(&mut self, window: &Window, output: egui::PlatformOutput) {
        let output = self.edit_keys_after_pass(window, output);
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
        let finished = if self.edit_keys.awaits_text() || self.plugin_reads.wants_failures() {
            selection.poll_with_failures()
        } else {
            selection
                .poll()
                .into_iter()
                .map(|(tag, text)| (tag, Some(text)))
                .collect()
        };
        // A plugin field's read goes to App before the host's: its text must not also reach a host
        // field or egui's `Paste` through `HeldPastes`.
        let finished = self.plugin_reads.sort(finished, selection.waiting());
        let mut pastes = Vec::new();
        for (tag, text) in finished {
            if !self.edit_keys_text_arrived(text.clone())
                && let Some(text) = text
            {
                pastes.push((tag, text));
            }
        }
        self.held_pastes.add(pastes, std::time::Instant::now());
        !self.held_pastes.is_empty() || self.edit_keys_busy() || self.plugin_reads.busy()
    }

    /// The input of this window's next egui frame. Every frame takes its input here, so each
    /// frame's focus input is counted and a waiting paste is checked against the focus at the
    /// moment egui will apply it.
    pub fn take_egui_input(&mut self, window: &Window) -> egui::RawInput {
        self.held_pastes.set_find_fields(&[]);
        self.take_pass_input(window, false)
    }

    /// `take_egui_input` 과 같다. `main_pass` 면 끊은 자리부터는 그다음 패스 몫으로 남긴다. 끊은 자리마다 메인 창
    /// 상태의 입력 몫·누름 스냅샷이 하나씩 쌓이므로, 나누기는 그것을 함께 꺼내는 `run_main_pass` 만 한다.
    pub(super) fn take_pass_input(&mut self, window: &Window, main_pass: bool) -> egui::RawInput {
        self.edit_keys_before_input(window);
        let mut raw_input = self.egui_state.take_egui_input(window);
        if main_pass
            && self.egui_input_cuts.split_pass(
                &mut raw_input.events,
                &mut self.egui_state.egui_input_mut().events,
            )
        {
            self.egui_ctx.request_repaint();
        }
        self.edit_keys.input_taken(&raw_input.events);
        self.pass_escape_marks = std::mem::take(&mut self.loaded_escape_marks);
        let focused = self.focus_tag();
        if self.plugin_paste_target.is_some()
            && self
                .held_pastes
                .hidden_focus_input(&raw_input.events, focused)
        {
            self.cancel_pending_paste();
        }
        let pastes = self
            .held_pastes
            .frame(&raw_input.events, focused, std::time::Instant::now());
        raw_input
            .events
            .extend(pastes.into_iter().map(egui::Event::Paste));
        raw_input
    }

    /// 마지막으로 꺼낸 입력이 적재한 Esc 누름마다 사건 시점 수식키. 관리자가 있는 별도 창 `render` 가 입력을 꺼낸 바로
    /// 뒤 부른다.
    pub fn take_window_escape_marks(&mut self) -> Vec<tasty_key_match::KeyMods> {
        std::mem::take(&mut self.pass_escape_marks)
    }

    /// The Wayland selection needs polling: a paste is being read or copied text is being sent.
    pub fn text_selection_busy(&self) -> bool {
        self.text_selection
            .as_ref()
            .is_some_and(crate::file::drag::TextSelection::busy)
            || !self.held_pastes.is_empty()
            || self.edit_keys.awaits_text()
            || self.plugin_reads.busy()
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
    /// The explorer Find fields the main window's last normal pass drew. A settled paste for one of
    /// them goes in only after the pass's segment filter, in a pass that gives the field its keys
    /// ([`Self::receive_find`]).
    find_fields: Vec<u64>,
}

impl Default for HeldPastes {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            quiet: SETTLE_FRAMES,
            find_fields: Vec::new(),
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

    /// The host cannot see which text field inside a plugin popup has focus, so input that can
    /// move focus ends every paste meant for that popup (`tag`). True when `events` had such input;
    /// the caller then also cancels the reads still in flight.
    pub(crate) fn hidden_focus_input(&mut self, events: &[egui::Event], tag: u64) -> bool {
        if !events.iter().any(moves_focus) {
            return false;
        }
        self.items.retain(|(asked, _, _)| {
            let kept = *asked != tag;
            if !kept {
                tracing::debug!("clipboard paste dropped: focus input reached its plugin popup");
            }
            kept
        });
        true
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
        let mut finished = Vec::new();
        for (tag, text, arrived) in std::mem::take(&mut self.items) {
            if tag == focused && self.find_fields.contains(&tag) {
                self.items.push((tag, text, arrived));
            } else {
                finished.push((tag, text));
            }
        }
        pastes_for(finished, focused)
    }

    /// The Find fields of the pass about to be taken (the main window's, empty for any other).
    pub(crate) fn set_find_fields(&mut self, fields: &[u64]) {
        self.find_fields.clear();
        self.find_fields.extend_from_slice(fields);
    }

    /// After the segment filter: the settled pastes for the focused Find field, when `open` (the
    /// pass draws the normal screen and its surfaces take keys). Otherwise they stay for a later
    /// pass. [`Self::frame`] has already dropped stale ones and those for another field.
    pub(crate) fn receive_find(&mut self, focused: u64, open: bool) -> Vec<String> {
        if !open || self.quiet < SETTLE_FRAMES || !self.find_fields.contains(&focused) {
            return Vec::new();
        }
        let mut taken = Vec::new();
        self.items.retain(|(tag, text, _)| {
            let take = *tag == focused;
            if take {
                taken.push(text.clone());
            }
            !take
        });
        taken
    }
}

/// The paste target tag of a plugin popup instance. Its text fields are not host egui widgets, so
/// the popup gets a tag of its own: a paste asked for one popup never reaches another.
pub(crate) fn plugin_popup_tag(instance: u64) -> u64 {
    egui::Id::new(("plugin_popup_paste", instance)).value()
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
