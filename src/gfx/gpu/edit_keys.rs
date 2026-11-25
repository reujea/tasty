//! Edit keys (copy, cut, paste) judged before egui-winit sees them. A key bound to an edit in
//! `KeybindingSettings` is consumed only when the edit really happened in egui; otherwise the
//! original key goes to egui-winit (and the window's other handlers) unchanged. When the result is
//! unknown at key time, the key is held, the edit is tried in the next egui pass, and that pass's
//! output decides. Contract: `docs/design/policies/key-mapping.md`, "편집 키 판정".
use std::time::{Duration, Instant};

use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, ModifiersState, PhysicalKey};

use crate::adapters::ui::input::edit_key::{
    self, Capability, EditBindings, EditKey, EditKeyHold, LineItem, Outcome, Release, Verdict,
};

/// A held edit in a window whose frames stopped (minimized or hidden) is taken back out of egui's
/// input and its key replayed once this long has passed since the edit went in. A window that still
/// draws is never timed out: its next frame, however late, runs the edit and decides.
const HOST_HOLD_LIMIT: Duration = Duration::from_secs(1);

/// A native Wayland read that has not answered after the selection's own read limit is given up.
pub(crate) const AWAIT_TEXT_LIMIT: Duration =
    crate::file::drag::PASTE_LIMIT.saturating_add(Duration::from_millis(500));

/// Who replays a released key. egui-only windows (settings, preset, plugins, quit, boot screens,
/// home-in-use) hand every key to egui alone, so the window's GPU state replays there itself. The
/// main window also runs Tasty shortcuts on a passed key, so it takes the release and replays it
/// through its own event path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKeyOwner {
    EguiOnly,
    Window,
}

/// What became of an event offered to [`EditKeyState::judge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiKey {
    /// Not a judged edit key: the event goes on as before.
    Free,
    /// Tasty took the event (a held edit key, or an event queued behind one).
    Taken,
    /// An edit key egui cannot run now. The original key goes on, and later steps must not match it
    /// as an edit binding again.
    Pass(EditKey),
}

/// The clipboard text a paste key would insert, read at key time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClipText {
    Text(String),
    /// Nothing to paste: the clipboard is empty or holds no text, or reading it failed.
    Empty,
    /// A native Wayland read of another app's selection is under way.
    Pending,
}

enum Phase {
    /// Input from before the key is still unprocessed. It runs in a pass of its own first, so the
    /// edit is judged on the focus and selection it leaves and its pass holds the edit alone.
    Settle,
    /// A frame took that earlier input. `again`: it may move focus at the start of the next pass
    /// (see [`defers_focus`]), so one more pass runs before the edit is judged.
    Settling { again: bool },
    /// The earlier input's pass ended: judge the edit again before the next input.
    Rejudge,
    /// Waiting for a native Wayland read asked for the widget tagged `tag`.
    AwaitText { tag: u64 },
    /// The edit event sits in egui's next input.
    Injected,
    /// A frame took the input with the edit; its output decides.
    InPass,
}

/// A window event as the judgment sees it.
pub(crate) trait HeldEvent: Clone {
    /// The pressed key, for a key press.
    fn press(&self) -> Option<(&Key, PhysicalKey)>;
    /// Keys, IME and the modifiers they are read with keep their order behind a held key.
    fn queues_behind_hold(&self) -> bool;
    /// Pointer input. It waits behind a held key too (see [`EditKeyState::judge`]), so a pass the
    /// hold runs carries no click that came after the key.
    fn is_pointer(&self) -> bool;
    /// A key or IME event: it waits for the pass of unprocessed earlier input
    /// ([`EditKeyState::hold_front`]).
    fn is_key(&self) -> bool;
}

impl HeldEvent for WindowEvent {
    fn press(&self) -> Option<(&Key, PhysicalKey)> {
        match self {
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                Some((&event.logical_key, event.physical_key))
            }
            _ => None,
        }
    }

    fn queues_behind_hold(&self) -> bool {
        held_order(self) == HeldOrder::Queue
    }

    fn is_pointer(&self) -> bool {
        held_order(self) == HeldOrder::Pointer
    }

    fn is_key(&self) -> bool {
        matches!(
            self,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldOrder {
    /// Keys, IME, modifiers and the input part of a focus change.
    Queue,
    /// Input that acts where the pointer is: its position, buttons, wheel, touch and gestures, and
    /// files dragged over or dropped at the pointer.
    Pointer,
    /// Window state that no held key's result depends on.
    Free,
}

/// How a window event keeps its place behind a held key. The match names every variant, so a new
/// winit event has to be placed here.
fn held_order(event: &WindowEvent) -> HeldOrder {
    match event {
        // A focus change also clears the modifiers and resets key tracking, so its input part keeps
        // its place. The window state it carries is applied on arrival by the App.
        WindowEvent::KeyboardInput { .. }
        | WindowEvent::Ime(_)
        | WindowEvent::ModifiersChanged(_)
        | WindowEvent::Focused(_) => HeldOrder::Queue,
        WindowEvent::CursorMoved { .. }
        | WindowEvent::CursorEntered { .. }
        | WindowEvent::CursorLeft { .. }
        | WindowEvent::MouseWheel { .. }
        | WindowEvent::MouseInput { .. }
        | WindowEvent::PinchGesture { .. }
        | WindowEvent::PanGesture { .. }
        | WindowEvent::DoubleTapGesture { .. }
        | WindowEvent::RotationGesture { .. }
        | WindowEvent::TouchpadPressure { .. }
        | WindowEvent::Touch(_)
        | WindowEvent::HoveredFile(_)
        | WindowEvent::HoveredFileCancelled
        | WindowEvent::DroppedFile(_) => HeldOrder::Pointer,
        // AxisMotion is a raw device axis with no pointer position; egui-winit and Tasty ignore it.
        WindowEvent::ActivationTokenDone { .. }
        | WindowEvent::Resized(_)
        | WindowEvent::Moved(_)
        | WindowEvent::CloseRequested
        | WindowEvent::Destroyed
        | WindowEvent::AxisMotion { .. }
        | WindowEvent::ScaleFactorChanged { .. }
        | WindowEvent::ThemeChanged(_)
        | WindowEvent::Occluded(_)
        | WindowEvent::RedrawRequested => HeldOrder::Free,
    }
}

/// The modifiers a window event leaves for key judgment: a modifier change gives its state, and a
/// focus loss clears them because a key let go outside the window sends no modifier change. The
/// window applies it when the event runs, so a held focus loss clears them only in its place.
pub(crate) fn window_modifiers(event: &WindowEvent) -> Option<ModifiersState> {
    match event {
        WindowEvent::ModifiersChanged(m) => Some(m.state()),
        WindowEvent::Focused(false) => Some(ModifiersState::default()),
        _ => None,
    }
}

struct HostHold<E> {
    hold: EditKeyHold<E>,
    phase: Phase,
    /// When the key came, then when the edit went into egui's input.
    since: Instant,
    owner: EditKeyOwner,
    /// The owner could not tell whether a text field gets the key before earlier input runs. After
    /// that input's pass the key goes back to the owner unjudged, to be judged on the new state.
    recheck: bool,
    /// Where an injected paste starts, to tell an applied paste from an ignored one.
    paste_at: Option<PasteMark>,
    /// A Wayland read that finished while input it let through was still unprocessed. It is
    /// applied once that input's pass has settled the focus.
    arrived: Option<Arrived>,
    /// Cut points of egui's input ahead of the edit when it went in. Each pass taken before the
    /// edit's own takes one segment ahead of it, so the edit is in the pass that finds none left.
    cuts_ahead: usize,
}

/// A finished Wayland read for the widget tagged `tag`. `text` is `None` when the read failed.
struct Arrived {
    tag: u64,
    text: Option<String>,
}

/// The focused field and the start of its selection (or cursor) when a paste went in.
#[derive(Clone, Copy)]
struct PasteMark {
    id: egui::Id,
    start: usize,
}

/// Keys that came while earlier input was unprocessed, with no edit judged: they wait for the
/// pass that takes that input and then run again from the window's input entry.
struct SettleLine<E> {
    line: Release<E>,
    owner: EditKeyOwner,
    since: Instant,
    /// A pass took the unprocessed input. The line is released when that pass ends.
    taken: bool,
}

/// The edit key judgment of one window.
pub(crate) struct EditKeyState<E = WindowEvent> {
    bindings: Option<EditBindings>,
    modifiers: ModifiersState,
    hold: Option<HostHold<E>>,
    release: Option<(EditKeyOwner, Release<E>)>,
    settle: Option<SettleLine<E>>,
    /// The last frame's input moves focus only when the next pass begins (see [`defers_focus`]).
    /// Until that pass runs, the focus read between frames is not where an edit would go.
    focus_deferred: bool,
    /// Cut points now waiting in the window's egui input (`InputCuts`), kept by the window. An edit
    /// put into the input records it as `cuts_ahead`.
    cuts: usize,
    /// When the window event now being run reached the window (`replace_arrival`). An event queued
    /// behind a hold keeps it for its replay. `None` (no event being run with one) reads as now.
    arrival: Option<Instant>,
}

impl<E> Default for EditKeyState<E> {
    fn default() -> Self {
        Self {
            bindings: None,
            modifiers: ModifiersState::empty(),
            hold: None,
            release: None,
            settle: None,
            focus_deferred: false,
            cuts: 0,
            arrival: None,
        }
    }
}

/// What a key-time judgment reads from egui and the clipboard.
pub(crate) struct KeyCx<'a> {
    pub(crate) ctx: &'a egui::Context,
    /// egui input gathered for the next frame and not yet processed.
    pub(crate) events: &'a mut Vec<egui::Event>,
    /// Reads the clipboard for a paste meant for the widget tagged with the argument.
    pub(crate) clipboard: &'a mut dyn FnMut(u64) -> ClipText,
}

impl<E: HeldEvent> EditKeyState<E> {
    /// The applied bindings this window judges with. `None` judges nothing.
    pub(crate) fn set_bindings(&mut self, bindings: Option<EditBindings>) {
        self.bindings = bindings;
    }

    /// The cut points waiting in the window's egui input before the next judgment or insert. Only
    /// the main window cuts its input; every other window keeps 0.
    pub(crate) fn set_cuts(&mut self, cuts: usize) {
        self.cuts = cuts;
    }

    /// When the window event now being run reached the window: now for a new event, its first
    /// arrival for a replayed one, `None` once it has run. Returns the value it replaces.
    pub(crate) fn replace_arrival(&mut self, at: Option<Instant>) -> Option<Instant> {
        std::mem::replace(&mut self.arrival, at)
    }

    /// [`Self::replace_arrival`]'s value, or now when no event is being run with one.
    pub(crate) fn arrival(&self) -> Instant {
        self.arrival.unwrap_or_else(Instant::now)
    }

    /// The winit modifiers the next key press is judged with.
    pub(crate) fn set_modifiers(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    /// When the earliest window event still waiting in this window's lines (behind a hold, a release
    /// or a settle line) reached the window. `None` when no event waits. Read by the X11 observation
    /// cleanup (`explorer_ime`).
    pub(crate) fn oldest_waiting(&self) -> Option<Instant> {
        let hold = self.hold.as_ref().and_then(|h| h.hold.first_queued_at());
        let release = self.release.as_ref().and_then(|(_, r)| r.first_queued_at());
        let settle = self.settle.as_ref().and_then(|s| s.line.first_queued_at());
        [hold, release, settle].into_iter().flatten().min()
    }

    /// A key is held, or its release still waits for the owner.
    pub(crate) fn busy(&self) -> bool {
        self.hold.is_some() || self.release.is_some() || self.settle.is_some()
    }

    /// The one hold check at the front of a window's input, before any step reads the event. An
    /// event behind a hold, a settle line or a release waiting for its owner takes its place at the
    /// end of that line. A key that comes while earlier input is unsettled ([`Self::unsettled`] on
    /// `events`, or `handed`: an Escape the window took out of egui's input for a popup that has
    /// not drawn it yet) starts a settle line: it runs again from the window's input entry after
    /// the pass that takes that input. True when the event waits.
    pub(crate) fn hold_front(
        &mut self,
        event: &E,
        owner: EditKeyOwner,
        events: &[egui::Event],
        handed: bool,
        now: Instant,
    ) -> bool {
        if self.queues(event) {
            self.queue(event.clone());
            return true;
        }
        if !event.is_key() || !(handed || self.unsettled(events)) {
            return false;
        }
        let mut line = Release {
            replay: None,
            queued: Vec::new(),
        };
        line.push(event.clone(), self.arrival());
        self.settle = Some(SettleLine {
            line,
            owner,
            since: now,
            taken: false,
        });
        true
    }

    pub(crate) fn awaits_text(&self) -> bool {
        matches!(
            self.hold,
            Some(HostHold {
                phase: Phase::AwaitText { .. },
                ..
            })
        )
    }

    /// Judges `event` before egui-winit sees it. Events behind a held key are queued in order.
    pub(crate) fn judge(
        &mut self,
        event: &E,
        owner: EditKeyOwner,
        now: Instant,
        cx: KeyCx<'_>,
    ) -> EguiKey {
        if self.queues(event) {
            self.queue(event.clone());
            return EguiKey::Taken;
        }
        let Some(edit) = self.bound_edit(event) else {
            return EguiKey::Free;
        };
        // Unprocessed input (a click, Tab, Ctrl+A, typed text) may change focus, selection or text
        // before the edit runs. It gets its own pass, then the edit is judged on what it left.
        if self.unsettled(cx.events) {
            self.hold_key(event, edit, Phase::Settle, now, owner, false);
            return EguiKey::Taken;
        }
        let ctx = cx.ctx;
        match start(edit, cx) {
            Start::Pass => EguiKey::Pass(edit),
            Start::Act => EguiKey::Taken,
            Start::Hold(phase) => {
                self.hold_key(event, edit, phase, now, owner, false);
                self.mark_paste(ctx);
                EguiKey::Taken
            }
        }
    }

    /// [`Self::judge`] for a key whose owner cannot tell yet whether a text field gets it: the
    /// focus it reads is from the last frame and input since may have moved it (a click into a
    /// field). With such input pending, a bound edit key waits for that input's pass and then goes
    /// back to the owner unjudged. With none, the owner's reading stands and the key is free.
    pub(crate) fn judge_unsettled(
        &mut self,
        event: &E,
        owner: EditKeyOwner,
        now: Instant,
        cx: KeyCx<'_>,
    ) -> EguiKey {
        if self.queues(event) {
            self.queue(event.clone());
            return EguiKey::Taken;
        }
        match self.bound_edit(event) {
            Some(edit) if self.unsettled(cx.events) => {
                self.hold_key(event, edit, Phase::Settle, now, owner, true);
                EguiKey::Taken
            }
            _ => EguiKey::Free,
        }
    }

    /// Whether `event` waits behind a held key. Keys, IME and modifiers always do. Pointer input
    /// does too, except while a Wayland read is under way with nothing queued yet: there a click
    /// that moves focus is the user leaving the field, and the paste is dropped when the text
    /// arrives. Once anything is queued, pointer input queues as well so it cannot overtake it.
    fn queues(&self, event: &E) -> bool {
        if !self.busy() {
            return false;
        }
        let behind_queued = self.hold.as_ref().is_some_and(|h| h.hold.has_queued());
        event.queues_behind_hold() || event.is_pointer() && (!self.awaits_text() || behind_queued)
    }

    /// The edit `event` presses, by the applied bindings.
    fn bound_edit(&self, event: &E) -> Option<EditKey> {
        let (logical, physical) = event.press()?;
        let bindings = self.bindings.as_ref()?;
        let mods = self.modifiers;
        let lookup = tasty_key_match::shortcut_lookup_key(logical, &physical, mods);
        bindings.binding(&lookup, mods)
    }

    fn hold_key(
        &mut self,
        event: &E,
        edit: EditKey,
        phase: Phase,
        now: Instant,
        owner: EditKeyOwner,
        recheck: bool,
    ) {
        self.hold = Some(HostHold {
            hold: EditKeyHold::new(event.clone(), edit, true),
            phase,
            since: now,
            owner,
            recheck,
            paste_at: None,
            arrived: None,
            cuts_ahead: self.cuts,
        });
    }

    /// Notes where an injected paste starts in the focused field.
    fn mark_paste(&mut self, ctx: &egui::Context) {
        if let Some(held) = self.hold.as_mut()
            && matches!(held.phase, Phase::Injected)
            && held.hold.edit() == EditKey::Paste
        {
            held.paste_at = paste_mark(ctx);
        }
    }

    /// The earlier input's pass ended and the held edit is judged again.
    pub(crate) fn needs_rejudge(&self) -> bool {
        matches!(
            self.hold,
            Some(HostHold {
                phase: Phase::Rejudge,
                ..
            })
        )
    }

    /// Judges a held edit again once the input before it has run: it passes, acts or goes into
    /// egui's input. Input that arrived since gets a pass of its own first. A key held for a
    /// recheck goes back to its owner unjudged instead.
    pub(crate) fn rejudge(&mut self, cx: KeyCx<'_>, now: Instant) {
        let Some(held) = self.hold.as_mut() else {
            return;
        };
        if !matches!(held.phase, Phase::Rejudge) {
            return;
        }
        if held.recheck {
            if let Some(held) = self.hold.take() {
                let mut release = held.hold.resolve(Outcome::NotActed);
                if let Some((event, _)) = release.replay.take() {
                    release
                        .queued
                        .insert(0, LineItem::Event(event, Instant::now()));
                }
                self.add_release(held.owner, release);
            }
            return;
        }
        if has_pending_input(cx.events) {
            held.phase = Phase::Settle;
            return;
        }
        if let Some(Arrived { tag, text }) = held.arrived.take() {
            self.apply_text(tag, text, cx.ctx, cx.events, now);
            return;
        }
        let ctx = cx.ctx;
        match start(held.hold.edit(), cx) {
            Start::Pass => self.resolve_now(Outcome::NotActed),
            Start::Act => self.resolve_now(Outcome::Acted),
            Start::Hold(phase) => {
                held.phase = phase;
                held.since = now;
                held.cuts_ahead = self.cuts;
                self.mark_paste(ctx);
            }
        }
    }

    /// A debug input injection at the front of the window's input, like a window event: while the
    /// line is open it takes its place at the end of that line and `None` is returned; otherwise it
    /// comes back to run now. Its replay runs it through the window's injection entry again.
    #[cfg(debug_assertions)]
    pub(crate) fn hold_debug(
        &mut self,
        input: crate::view::main::debug_input::DebugInput,
    ) -> Option<crate::view::main::debug_input::DebugInput> {
        if !self.busy() {
            return Some(input);
        }
        self.queue_item(LineItem::Debug(input));
        None
    }

    fn queue(&mut self, event: E) {
        let at = self.arrival();
        self.queue_item(LineItem::Event(event, at));
    }

    /// Puts `item` at the end of the open line: the held key's, else the settle line, else the
    /// release still waiting for its owner.
    fn queue_item(&mut self, item: LineItem<E>) {
        if let Some(held) = self.hold.as_mut() {
            held.hold.queue_item(item);
        } else if let Some(settle) = self.settle.as_mut() {
            settle.line.push_item(item);
        } else if let Some((_, release)) = self.release.as_mut() {
            release.push_item(item);
        }
    }

    /// Focus or selection may still change before an edit runs: egui has input it has not
    /// processed, or the last frame's input moves focus when the next pass begins.
    pub(crate) fn unsettled(&self, events: &[egui::Event]) -> bool {
        has_pending_input(events) || self.focus_deferred
    }

    /// A frame took egui's input `events`: an injected edit with no cut point left ahead of it is
    /// now in that pass, and one with cut points ahead lets this pass take one of them. The check
    /// for none left comes before the count goes down. A focus move the input defers to the next
    /// pass is remembered whether or not a key is held.
    pub(crate) fn input_taken(&mut self, events: &[egui::Event]) {
        self.focus_deferred = events.iter().any(defers_focus);
        if let Some(settle) = self.settle.as_mut() {
            settle.taken = true;
        }
        if let Some(held) = self.hold.as_mut() {
            match held.phase {
                Phase::Injected if held.cuts_ahead == 0 => held.phase = Phase::InPass,
                Phase::Injected => held.cuts_ahead -= 1,
                Phase::Settle => {
                    let again = events.iter().any(defers_focus);
                    held.phase = Phase::Settling { again };
                }
                _ => {}
            }
        }
    }

    /// The segment filter dropped the input of the pass just taken (its focus node is not on the
    /// screen it draws). An edit in that pass ends there without replaying its key: its target is
    /// gone, as when the field lost focus. The events queued behind it go back in order.
    pub(crate) fn input_dropped(&mut self) {
        if matches!(
            self.hold,
            Some(HostHold {
                phase: Phase::InPass,
                ..
            })
        ) {
            self.resolve_now(Outcome::TargetGone);
        }
    }

    /// The output of the pass that ran the edit, and the field state it left, decide the held key.
    /// A cut that removed nothing (a read-only field) also drops the copy egui made, so only the
    /// original key's next handler can copy.
    pub(crate) fn after_pass(
        &mut self,
        output: egui::PlatformOutput,
        ctx: &egui::Context,
    ) -> egui::PlatformOutput {
        if let Some(settle) = self.settle.take_if(|s| s.taken) {
            // The keys run again from the window's input entry, which judges them on what the pass
            // left (a focus the pass deferred holds them for one more).
            self.add_release(settle.owner, settle.line);
        }
        if let Some(held) = self.hold.as_mut()
            && let Phase::Settling { again } = held.phase
        {
            held.phase = if again { Phase::Settle } else { Phase::Rejudge };
            return output;
        }
        let Some(held) = self.hold.take_if(|h| matches!(h.phase, Phase::InPass)) else {
            return output;
        };
        let edit = held.hold.edit();
        let outcome = pass_outcome(edit, &output, held.paste_at, ctx);
        let output = if edit == EditKey::Cut && outcome == Outcome::NotActed {
            super::text_selection::take_copied_text(output).0
        } else {
            output
        };
        tracing::debug!(?edit, ?outcome, "held edit key resolved by the egui pass");
        self.release_hold(held, outcome);
        output
    }

    /// The native Wayland read for a held paste finished. `text` is `None` when it failed. If the
    /// user moved focus away from the field the paste was asked for, the text and the key are both
    /// dropped, whatever the read returned. Input the read let through (a click) that egui has not
    /// processed yet may still move focus, so the result waits for that input's pass first.
    pub(crate) fn text_arrived(
        &mut self,
        text: Option<String>,
        ctx: &egui::Context,
        events: &mut Vec<egui::Event>,
        now: Instant,
    ) {
        let Some(held) = self.hold.as_mut() else {
            return;
        };
        let Phase::AwaitText { tag } = held.phase else {
            return;
        };
        if has_pending_input(events) {
            held.arrived = Some(Arrived { tag, text });
            held.phase = Phase::Settle;
            return;
        }
        self.apply_text(tag, text, ctx, events, now);
    }

    /// Applies a finished Wayland read on settled focus: the text goes in for the field it was
    /// asked for, an empty or failed read replays the key, and a field that lost focus drops both.
    fn apply_text(
        &mut self,
        tag: u64,
        text: Option<String>,
        ctx: &egui::Context,
        events: &mut Vec<egui::Event>,
        now: Instant,
    ) {
        let Some(held) = self.hold.as_mut() else {
            return;
        };
        if tag != focus_tag(ctx) {
            tracing::debug!("held paste dropped: its text field lost focus");
            self.resolve_now(Outcome::TargetGone);
            return;
        }
        match text.filter(|t| !t.is_empty()) {
            Some(text) => {
                events.push(egui::Event::Paste(text));
                held.phase = Phase::Injected;
                held.since = now;
                held.cuts_ahead = self.cuts;
                self.mark_paste(ctx);
            }
            None => self.resolve_now(Outcome::NotActed),
        }
    }

    /// Gives up a hold whose result did not come in time. A Wayland read is cancelled past its
    /// limit. A hold waiting for a frame is given up only while `frames_stopped` (the window is
    /// minimized or hidden): an edit still waiting in egui's input is taken back first, so the
    /// replayed key is the only effect. `cancel_read` stops a Wayland read.
    pub(crate) fn expire(
        &mut self,
        now: Instant,
        events: &mut Vec<egui::Event>,
        cancel_read: impl FnOnce(),
        frames_stopped: bool,
    ) {
        if let Some(settle) = self.settle.take_if(|s| {
            frames_stopped && now.saturating_duration_since(s.since) >= HOST_HOLD_LIMIT
        }) {
            // No pass will take the input: the keys go on as they are.
            self.add_release(settle.owner, settle.line);
        }
        let Some(held) = self.hold.as_ref() else {
            return;
        };
        let age = now.saturating_duration_since(held.since);
        let stale = frames_stopped && age >= HOST_HOLD_LIMIT;
        let outcome = match held.phase {
            Phase::AwaitText { .. } if age >= AWAIT_TEXT_LIMIT => {
                cancel_read();
                Outcome::TimedOut
            }
            Phase::Settle | Phase::Settling { .. } | Phase::Rejudge if stale => Outcome::TimedOut,
            Phase::Injected if stale => {
                if retract_edit(events, held.hold.edit()) {
                    Outcome::TimedOut
                } else {
                    Outcome::TargetGone
                }
            }
            _ => return,
        };
        tracing::warn!(edit = ?held.hold.edit(), "held edit key got no result in time");
        self.resolve_now(outcome);
    }

    fn resolve_now(&mut self, outcome: Outcome) {
        if let Some(held) = self.hold.take() {
            self.release_hold(held, outcome);
        }
    }

    fn release_hold(&mut self, held: HostHold<E>, outcome: Outcome) {
        let release = held.hold.resolve(outcome);
        self.add_release(held.owner, release);
    }

    fn add_release(&mut self, owner: EditKeyOwner, release: Release<E>) {
        // A release still waiting for its owner keeps its place: the new one's events come after.
        match self.release.as_mut() {
            Some((_, earlier)) => {
                // The released key is an edit key press; no judgment reads its arrival time.
                let (replay, queued) = release.split();
                if let Some((event, _)) = replay {
                    earlier.push(event, Instant::now());
                }
                for item in queued {
                    earlier.push_item(item);
                }
            }
            None => self.release = Some((owner, release)),
        }
    }

    /// The finished hold for `owner`, if any.
    pub(crate) fn take_release(&mut self, owner: EditKeyOwner) -> Option<Release<E>> {
        self.release
            .take_if(|(o, _)| *o == owner)
            .map(|(_, release)| release)
    }
}

impl super::GpuState {
    /// The applied edit bindings this window judges with. App windows pass the applied settings;
    /// `None` leaves every key to egui-winit.
    pub fn set_edit_bindings(&mut self, bindings: Option<EditBindings>) {
        self.edit_keys.set_bindings(bindings);
    }

    /// When the window event now being run reached the window. The App sets it around each event
    /// and each replayed one and puts the earlier value back after, so a key queued behind a hold
    /// keeps it and double taps and modifier hint holds are judged by arrival, not by when the
    /// event runs. Returns the value it replaces.
    pub fn replace_event_arrival(&mut self, at: Option<Instant>) -> Option<Instant> {
        self.edit_keys.replace_arrival(at)
    }

    /// When the earliest window event still waiting in this window's lines reached the window
    /// (`EditKeyState::oldest_waiting`).
    #[allow(
        dead_code,
        reason = "the only reader, the X11 observation cleanup (`explorer_ime`), is built on Linux only"
    )]
    pub fn oldest_waiting_input(&self) -> Option<Instant> {
        self.edit_keys.oldest_waiting()
    }

    /// [`Self::replace_event_arrival`]'s value (now when the event came by another way).
    pub fn event_arrival(&self) -> Instant {
        self.edit_keys.arrival()
    }

    /// Judges a window event before egui-winit. `owner` says who replays a released key.
    pub fn judge_egui_key(
        &mut self,
        window: &winit::window::Window,
        event: &WindowEvent,
        owner: EditKeyOwner,
    ) -> EguiKey {
        self.expire_edit_hold(frames_stopped(window));
        self.with_key_cx(|state, cx| state.judge(event, owner, Instant::now(), cx))
    }

    /// [`Self::judge_egui_key`] for a key the main window cannot yet place: its focus reading is
    /// from the last frame and egui input since may have moved it. See
    /// [`EditKeyState::judge_unsettled`].
    pub fn judge_unsettled_egui_key(
        &mut self,
        window: &winit::window::Window,
        event: &WindowEvent,
    ) -> EguiKey {
        self.expire_edit_hold(frames_stopped(window));
        self.with_key_cx(|state, cx| {
            state.judge_unsettled(event, EditKeyOwner::Window, Instant::now(), cx)
        })
    }

    /// egui input gathered for the next frame that could still move focus or change a selection.
    pub fn egui_input_pending(&self) -> bool {
        self.edit_keys
            .unsettled(&self.egui_state.egui_input().events)
    }

    /// Runs `f` with this window's judgment state and the egui input gathered for the next frame.
    fn with_key_cx<R>(&mut self, f: impl FnOnce(&mut EditKeyState, KeyCx<'_>) -> R) -> R {
        self.edit_keys.set_cuts(self.egui_input_cuts.len());
        let ctx = self.egui_ctx.clone();
        let mut events = std::mem::take(&mut self.egui_state.egui_input_mut().events);
        let selection = &mut self.text_selection;
        let egui_state = &mut self.egui_state;
        let mut clipboard = |tag: u64| match selection.as_mut() {
            Some(selection) => match selection.request_held_text(tag) {
                Some(Some(text)) => ClipText::Text(text),
                Some(None) => ClipText::Empty,
                None => ClipText::Pending,
            },
            None => egui_state
                .clipboard_text()
                .filter(|t| !t.is_empty())
                .map_or(ClipText::Empty, ClipText::Text),
        };
        let result = f(
            &mut self.edit_keys,
            KeyCx {
                ctx: &ctx,
                events: &mut events,
                clipboard: &mut clipboard,
            },
        );
        let pending = &mut self.egui_state.egui_input_mut().events;
        events.append(pending);
        *pending = events;
        result
    }

    /// [`Self::handle_egui_event`] for a window whose keys go to egui alone. An edit key egui can run
    /// is taken; any other key, and a released one, reaches egui-winit unchanged.
    pub fn handle_egui_event_with_edit_keys(
        &mut self,
        window: &winit::window::Window,
        event: &WindowEvent,
    ) -> (bool, bool) {
        match self.judge_egui_key(window, event, EditKeyOwner::EguiOnly) {
            EguiKey::Taken => (true, true),
            EguiKey::Free | EguiKey::Pass(_) => self.handle_egui_event(window, event),
        }
    }

    /// [`Self::handle_egui_event`] for an egui-only window that also reads keys itself (settings,
    /// quit, preset, plugins). `None` when the judgment took the event: the window skips its own
    /// handling as well and gets the event back from [`Self::take_edit_release`], to run through
    /// [`replay_release`]. `replayed` marks that released key, already judged, which goes on as is.
    /// `edit_target` is false for a key that is no edit's target (a bare Escape goes to its
    /// receiver's Escape handling): it reaches egui-winit unjudged.
    pub fn handle_egui_event_for_window(
        &mut self,
        window: &winit::window::Window,
        event: &WindowEvent,
        replayed: Option<EditKey>,
        edit_target: bool,
    ) -> Option<(bool, bool)> {
        let judged = match replayed {
            Some(edit) => EguiKey::Pass(edit),
            None if edit_target => self.judge_egui_key(window, event, EditKeyOwner::Window),
            None => EguiKey::Free,
        };
        match judged {
            EguiKey::Taken => {
                window.request_redraw();
                None
            }
            EguiKey::Free | EguiKey::Pass(_) => Some(self.handle_egui_event(window, event)),
        }
    }

    /// A finished hold of a window that replays keys itself (the main window, and egui-only windows
    /// with key handling of their own): the key to replay once (not judged again) and the events
    /// queued behind it.
    pub fn take_edit_release(&mut self) -> Option<Release<WindowEvent>> {
        self.edit_keys.take_release(EditKeyOwner::Window)
    }

    /// Before a frame takes egui's input: gives up a stale hold and replays an egui-only release.
    pub(super) fn edit_keys_before_input(&mut self, window: &winit::window::Window) {
        // A frame is being drawn: whatever waits for one gets it now.
        self.expire_edit_hold(false);
        let Some(release) = self.edit_keys.take_release(EditKeyOwner::EguiOnly) else {
            return;
        };
        let (replay, queued) = release.split();
        if let Some((event, _)) = replay {
            self.handle_egui_event(window, &event);
        }
        for item in queued {
            match item {
                LineItem::Event(event, _) => {
                    self.handle_egui_event_with_edit_keys(window, &event);
                }
                // A window with no view has no debug injection entry, so its line holds none.
                #[cfg(debug_assertions)]
                LineItem::Debug(input) => {
                    tracing::warn!(
                        ?input,
                        "a debug input waited in a window with no entry for it"
                    );
                }
            }
        }
    }

    /// After a frame's pass: the held key's result, and a redraw when a key waits to be replayed.
    pub(super) fn edit_keys_after_pass(
        &mut self,
        window: &winit::window::Window,
        output: egui::PlatformOutput,
    ) -> egui::PlatformOutput {
        let output = self.edit_keys.after_pass(output, &self.egui_ctx);
        if self.edit_keys.needs_rejudge() {
            self.with_key_cx(|state, cx| state.rejudge(cx, Instant::now()));
        }
        if self.edit_keys.busy() {
            window.request_redraw();
        }
        output
    }

    /// Hands a finished Wayland read to a paste held for it. False when no paste waits for one.
    pub(super) fn edit_keys_text_arrived(&mut self, text: Option<String>) -> bool {
        if !self.edit_keys.awaits_text() {
            return false;
        }
        self.edit_keys.set_cuts(self.egui_input_cuts.len());
        let events = &mut self.egui_state.egui_input_mut().events;
        self.edit_keys
            .text_arrived(text, &self.egui_ctx, events, Instant::now());
        true
    }

    /// The hold check at the front of this window's input ([`EditKeyState::hold_front`]). `handed`
    /// is whether an Escape taken out of egui's input for a popup has not been drawn yet. True when
    /// the event waits; the window draws to take the input it waits for.
    pub fn hold_window_input(
        &mut self,
        window: &winit::window::Window,
        event: &WindowEvent,
        handed: bool,
    ) -> bool {
        self.expire_edit_hold(frames_stopped(window));
        let events = &self.egui_state.egui_input().events;
        let held =
            self.edit_keys
                .hold_front(event, EditKeyOwner::Window, events, handed, Instant::now());
        if held {
            window.request_redraw();
        }
        held
    }

    pub fn edit_keys_busy(&self) -> bool {
        self.edit_keys.busy()
    }

    fn expire_edit_hold(&mut self, frames_stopped: bool) {
        let selection = &mut self.text_selection;
        let events = &mut self.egui_state.egui_input_mut().events;
        let cancel_read = || {
            if let Some(selection) = selection.as_mut() {
                selection.cancel();
            }
        };
        self.edit_keys
            .expire(Instant::now(), events, cancel_read, frames_stopped);
    }
}

/// The window draws no frames: it is minimized or hidden. winit cannot tell on every platform
/// (Wayland answers neither), and an unknown answer counts as drawing. An occluded window is not
/// told apart either: drawing does not read occlusion (only hang reports record it), and an
/// occluded X11 window still draws.
pub(crate) fn frames_stopped(window: &winit::window::Window) -> bool {
    window.is_minimized() == Some(true) || window.is_visible() == Some(false)
}

/// One step of a finished hold's replay through a window's own event handling.
pub(crate) enum Replayed<E> {
    /// A window event: the released key once, marked with the edit it was judged for (no arrival
    /// time), or a queued event unmarked (judged again) with its arrival time.
    Event(E, Option<EditKey>, Option<Instant>),
    /// A queued debug injection. It runs through the window's injection entry again.
    #[cfg(debug_assertions)]
    Debug(crate::view::main::debug_input::DebugInput),
}

/// Runs a finished hold's line through a window's own event handling, in order: the released key
/// once, then the queued input in the order it came.
/// Stops at the first result `done` accepts (the window closed) and returns it.
pub(crate) fn replay_release<E, A>(
    release: Release<E>,
    done: impl Fn(&A) -> bool,
    mut handle: impl FnMut(Replayed<E>) -> A,
) -> Option<A> {
    let (replay, queued) = release.split();
    let replay = replay.map(|(event, edit)| Replayed::Event(event, Some(edit), None));
    let queued = queued.into_iter().map(|item| match item {
        LineItem::Event(event, at) => Replayed::Event(event, None, Some(at)),
        #[cfg(debug_assertions)]
        LineItem::Debug(input) => Replayed::Debug(input),
    });
    for step in replay.into_iter().chain(queued) {
        let result = handle(step);
        if done(&result) {
            return Some(result);
        }
    }
    None
}

/// What a bound edit does on a settled state: the key passes on, a label copy acts at once, or the
/// key is held with the edit put in egui's input (or a Wayland read started).
enum Start {
    Pass,
    Act,
    Hold(Phase),
}

/// Judges `edit` with no unprocessed input before it, and puts the edit into `cx.events` when held.
fn start(edit: EditKey, cx: KeyCx<'_>) -> Start {
    let tag = focus_tag(cx.ctx);
    let mut clip = None;
    let verdict = edit_key::verdict(Some(edit), |edit| {
        let target = match edit {
            EditKey::Copy | EditKey::Cut => has_selection(cx.ctx),
            EditKey::Paste => text_edit_focused(cx.ctx),
        };
        if !target {
            return Capability::No;
        }
        // egui copies a label selection whenever it has one; a text field may refuse (password).
        if edit == EditKey::Copy && label_copy_only(cx.ctx) {
            return Capability::Yes;
        }
        if edit != EditKey::Paste {
            return Capability::Unknown;
        }
        let text = (cx.clipboard)(tag);
        let capability = if text == ClipText::Empty {
            Capability::No
        } else {
            Capability::Unknown
        };
        clip = Some(text);
        capability
    });
    let edit = match verdict {
        Verdict::NoBinding | Verdict::Pass(_) => return Start::Pass,
        Verdict::Act(edit) => {
            cx.events.push(egui::Event::Copy);
            tracing::debug!(?edit, "edit key acted on a label selection");
            return Start::Act;
        }
        Verdict::Hold(edit) => edit,
    };
    Start::Hold(match (edit, clip) {
        (EditKey::Paste, Some(ClipText::Pending)) => Phase::AwaitText { tag },
        (EditKey::Paste, Some(ClipText::Text(text))) => {
            cx.events.push(egui::Event::Paste(text));
            Phase::Injected
        }
        (EditKey::Copy, _) => {
            cx.events.push(egui::Event::Copy);
            Phase::Injected
        }
        (EditKey::Cut, _) => {
            cx.events.push(egui::Event::Cut);
            Phase::Injected
        }
        (EditKey::Paste, _) => return Start::Pass,
    })
}

/// Input whose focus move egui may apply only at the start of the next pass. egui 0.31 keeps the
/// focus during the pass that reads Shift+Tab and moves it when the next pass begins
/// (`Focus::id_next_frame`), and Tab from the last focusable widget hands focus to the first one in
/// the next pass. The focus read right after that pass is not yet the one an edit would go to.
fn defers_focus(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            ..
        }
    )
}

/// Input egui has not processed that could move focus or change a selection.
fn has_pending_input(events: &[egui::Event]) -> bool {
    events.iter().any(|e| {
        !matches!(
            e,
            egui::Event::PointerMoved(_)
                | egui::Event::MouseMoved(_)
                | egui::Event::PointerGone
                | egui::Event::MouseWheel { .. }
                | egui::Event::Zoom(_)
                | egui::Event::WindowFocused(_)
        )
    })
}

fn focus_tag(ctx: &egui::Context) -> u64 {
    ctx.memory(|m| m.focused()).map_or(0, |id| id.value())
}

/// A focused text field has a non-empty selection, or a label selection exists.
pub(crate) fn has_selection(ctx: &egui::Context) -> bool {
    let field = ctx
        .memory(|m| m.focused())
        .and_then(|id| egui::TextEdit::load_state(ctx, id))
        .and_then(|state| state.cursor.char_range())
        .is_some_and(|range| range.primary != range.secondary);
    field || egui::text_selection::LabelSelectionState::load(ctx).has_selection()
}

/// A label selection exists and no text field has focus, so a copy is the label's alone.
fn label_copy_only(ctx: &egui::Context) -> bool {
    !text_edit_focused(ctx) && egui::text_selection::LabelSelectionState::load(ctx).has_selection()
}

/// The focused widget is a text field.
pub(crate) fn text_edit_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .and_then(|id| egui::TextEdit::load_state(ctx, id))
        .is_some()
}

/// Whether the edit happened in the pass that produced `output`, which ran it alone.
/// - Copy: the pass copied text (a password field copies nothing).
/// - Cut: a text field's text changed. egui reports the text before and after every changed
///   `TextEdit` (masked for passwords); removing a non-empty selection always shortens it, and a
///   read-only field reports a change with equal texts.
/// - Paste: a text field ran an edit in this pass and the marked field's cursor ended past where
///   the paste started (`paste_at`). egui 0.31 reports `ValueChanged` only from a `TextEdit` that
///   is drawn, interactive and focused and whose edit changed something (a read-only field reports
///   it too), so a field disabled or not drawn in this pass, whose stored cursor stays where it
///   was, does not count. egui puts the cursor after the inserted text, and a read-only field
///   leaves it at the start, so this holds even when the new text equals the old (the same text
///   pasted over itself, or a password of the same length). With no mark (the field had no cursor
///   yet, so nothing was selected) a paste lengthens the text, and a text change counts.
fn pass_outcome(
    edit: EditKey,
    output: &egui::PlatformOutput,
    paste_at: Option<PasteMark>,
    ctx: &egui::Context,
) -> Outcome {
    let acted = match edit {
        EditKey::Copy => copied_text(output),
        EditKey::Cut => text_changed(output),
        EditKey::Paste => match paste_at {
            Some(mark) => {
                text_edit_ran(output)
                    && cursor_after(ctx, mark.id).is_some_and(|at| at > mark.start)
            }
            None => text_changed(output),
        },
    };
    if acted {
        Outcome::Acted
    } else {
        Outcome::NotActed
    }
}

fn copied_text(output: &egui::PlatformOutput) -> bool {
    // reason: egui 0.31 still fills the deprecated field for older callers.
    #[allow(deprecated)]
    let legacy = !output.copied_text.is_empty();
    legacy
        || output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
}

/// A text field ran an edit in the pass: egui reports `ValueChanged` for it, with or without a
/// different text.
fn text_edit_ran(output: &egui::PlatformOutput) -> bool {
    output.events.iter().any(|event| {
        matches!(event, egui::output::OutputEvent::ValueChanged(info)
            if info.typ == egui::WidgetType::TextEdit)
    })
}

fn text_changed(output: &egui::PlatformOutput) -> bool {
    output.events.iter().any(|event| {
        let info = event.widget_info();
        info.typ == egui::WidgetType::TextEdit
            && info.prev_text_value.is_some()
            && info.prev_text_value != info.current_text_value
    })
}

/// The focused text field and where its selection (or cursor) starts.
fn paste_mark(ctx: &egui::Context) -> Option<PasteMark> {
    let id = ctx.memory(|m| m.focused())?;
    let range = egui::TextEdit::load_state(ctx, id)?.cursor.char_range()?;
    Some(PasteMark {
        id,
        start: range.primary.index.min(range.secondary.index),
    })
}

/// The cursor position of the text field `id` (its selection's moving end).
fn cursor_after(ctx: &egui::Context, id: egui::Id) -> Option<usize> {
    let range = egui::TextEdit::load_state(ctx, id)?.cursor.char_range()?;
    Some(range.primary.index)
}

/// Takes the injected edit back out of egui's pending input. False when it is gone.
fn retract_edit(events: &mut Vec<egui::Event>, edit: EditKey) -> bool {
    let found = events.iter().rposition(|e| match edit {
        EditKey::Copy => matches!(e, egui::Event::Copy),
        EditKey::Cut => matches!(e, egui::Event::Cut),
        EditKey::Paste => matches!(e, egui::Event::Paste(_)),
    });
    found.map(|i| events.remove(i)).is_some()
}

#[cfg(test)]
#[path = "edit_keys_tests.rs"]
mod tests;
