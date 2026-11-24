//! egui integration: the key-time judgment and the pass that decides a held key, on a real
//! `egui::Context` with an editable field, a read-only field, a password field and a label.
use std::time::{Duration, Instant};

use super::*;
use winit::keyboard::{KeyCode, SmolStr};

/// A test event: a key press or any other event that queues behind a hold.
#[derive(Clone, Debug, PartialEq)]
enum Ev {
    Press(Key),
    Other(&'static str),
    Pointer(&'static str),
}

impl HeldEvent for Ev {
    fn press(&self) -> Option<(&Key, PhysicalKey)> {
        match self {
            Ev::Press(key) => Some((key, physical_of(key))),
            Ev::Other(_) | Ev::Pointer(_) => None,
        }
    }
    fn queues_behind_hold(&self) -> bool {
        !matches!(self, Ev::Pointer(_))
    }
    fn is_pointer(&self) -> bool {
        matches!(self, Ev::Pointer(_))
    }
    fn is_key(&self) -> bool {
        !matches!(self, Ev::Pointer(_))
    }
}

/// The key position under a letter, as the US layout puts it. Shortcuts with Alt match by position.
fn physical_of(key: &Key) -> PhysicalKey {
    let code = match key {
        Key::Character(c) if c == "c" => KeyCode::KeyC,
        Key::Character(c) if c == "v" => KeyCode::KeyV,
        Key::Character(c) if c == "x" => KeyCode::KeyX,
        _ => KeyCode::KeyQ,
    };
    PhysicalKey::Code(code)
}

fn press(c: &str) -> Ev {
    Ev::Press(Key::Character(SmolStr::new(c)))
}

/// The modifier the settings' `alt` token stands for on this OS (Command on macOS), found through
/// the binding lookup itself.
fn alt() -> ModifiersState {
    let settings = crate::settings::KeybindingSettings {
        paste: vec!["alt+v".into()],
        ..Default::default()
    };
    let bindings = EditBindings::from_settings(&settings);
    let v = Key::Character(SmolStr::new("v"));
    [ModifiersState::ALT, ModifiersState::SUPER]
        .into_iter()
        .find(|m| bindings.binding(&v, *m) == Some(EditKey::Paste))
        .expect("the alt token maps to one modifier")
}

fn id(name: &str) -> egui::Id {
    egui::Id::new(name)
}

struct Ui {
    ctx: egui::Context,
    field: String,
    next: String,
    secret: String,
    clipboard: ClipText,
    label: egui::Rect,
    /// Where the second field was drawn, to click into it.
    next_at: egui::Rect,
    /// The first field is drawn, and takes input when drawn.
    field_drawn: bool,
    field_enabled: bool,
}

impl Ui {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            field: "hello".into(),
            next: String::new(),
            secret: "pass".into(),
            clipboard: ClipText::Text("CLIP".into()),
            label: egui::Rect::NOTHING,
            next_at: egui::Rect::NOTHING,
            field_drawn: true,
            field_enabled: true,
        }
    }

    /// One frame with `events`. The field sits at the top so a click at (5, 5) lands in it.
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::PlatformOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let (field, next, secret) = (&mut self.field, &mut self.next, &mut self.secret);
        let (label, next_at) = (&mut self.label, &mut self.next_at);
        let (field_drawn, field_enabled) = (self.field_drawn, self.field_enabled);
        self.ctx
            .run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if field_drawn {
                        ui.add(
                            egui::TextEdit::singleline(field)
                                .id(id("field"))
                                .interactive(field_enabled),
                        );
                    }
                    *next_at = ui.add(egui::TextEdit::singleline(next).id(id("next"))).rect;
                    let mut read_only: &str = "mirror://address";
                    ui.add(egui::TextEdit::singleline(&mut read_only).id(id("ro")));
                    ui.add(
                        egui::TextEdit::singleline(secret)
                            .password(true)
                            .id(id("pw")),
                    );
                    *label = ui.label("label text").rect;
                });
            })
            .platform_output
    }

    fn focus(&mut self, name: &str) {
        self.ctx.memory_mut(|m| m.request_focus(id(name)));
        self.frame(Vec::new());
        self.frame(Vec::new());
    }

    fn select(&mut self, name: &str, from: usize, to: usize) {
        use egui::text::{CCursor, CCursorRange};
        let mut state = egui::TextEdit::load_state(&self.ctx, id(name)).expect("field state");
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(from),
            CCursor::new(to),
        )));
        state.store(&self.ctx, id(name));
    }

    /// Judges `event` with `pending` as the not-yet-processed input of the next frame.
    fn judge(
        &mut self,
        state: &mut EditKeyState<Ev>,
        event: &Ev,
        pending: &mut Vec<egui::Event>,
        now: Instant,
    ) -> EguiKey {
        let clip = self.clipboard.clone();
        let mut read = |_tag: u64| clip.clone();
        state.judge(
            event,
            EditKeyOwner::EguiOnly,
            now,
            KeyCx {
                ctx: &self.ctx,
                events: pending,
                clipboard: &mut read,
            },
        )
    }

    /// Runs the frame that takes `pending` and lets its output decide the held key. A hold that
    /// waited for earlier input is judged again after that pass, as `GpuState` does, and the edit
    /// it puts in the input runs in the next frame. A hold that asks for one more pass (a focus
    /// move egui applies when the next pass begins) gets an empty frame first.
    fn pass(&mut self, state: &mut EditKeyState<Ev>, pending: Vec<egui::Event>) -> Option<String> {
        state.input_taken(&pending);
        let output = self.frame(pending);
        let output = state.after_pass(output, &self.ctx);
        let copied = super::super::text_selection::take_copied_text(output).1;
        if matches!(
            state.hold,
            Some(HostHold {
                phase: Phase::Settle,
                ..
            })
        ) {
            return self.pass(state, Vec::new()).or(copied);
        }
        if !state.needs_rejudge() {
            return copied;
        }
        let clip = self.clipboard.clone();
        let mut read = |_tag: u64| clip.clone();
        let mut next = Vec::new();
        state.rejudge(
            KeyCx {
                ctx: &self.ctx,
                events: &mut next,
                clipboard: &mut read,
            },
            Instant::now(),
        );
        let later = self.pass(state, next);
        later.or(copied)
    }
}

fn bound() -> EditKeyState<Ev> {
    let settings = crate::settings::KeybindingSettings {
        copy: vec!["alt+c".into()],
        cut: vec!["alt+x".into()],
        paste: vec!["alt+v".into()],
        ..Default::default()
    };
    let mut state = EditKeyState::default();
    state.set_bindings(Some(EditBindings::from_settings(&settings)));
    state.set_modifiers(alt());
    state
}

fn released(state: &mut EditKeyState<Ev>) -> Release<Ev> {
    state
        .take_release(EditKeyOwner::EguiOnly)
        .expect("the held key is released")
}

/// A bound paste into a focused field with text on the clipboard is held, inserted once, and the
/// key is consumed: nothing is replayed, so egui-winit never types `v`.
#[test]
fn a_bound_paste_inserts_once_and_consumes_the_key() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(pending, vec![egui::Event::Paste("CLIP".into())]);
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "helloCLIP");
    let release = released(&mut state);
    assert!(release.replay.is_none(), "an acted paste consumes the key");
}

/// An empty clipboard passes the key at key time; nothing is held.
#[test]
fn an_empty_clipboard_passes_the_paste_key() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Empty;
    let mut state = bound();
    let mut pending = Vec::new();
    let got = ui.judge(&mut state, &press("v"), &mut pending, Instant::now());
    assert_eq!(got, EguiKey::Pass(EditKey::Paste));
    assert!(pending.is_empty());
    assert!(!state.busy());
}

/// Copy without a selection, and paste without a focused field, pass at key time.
#[test]
fn nothing_to_act_on_passes_at_key_time() {
    let mut ui = Ui::new();
    ui.frame(Vec::new());
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, now),
        EguiKey::Pass(EditKey::Copy)
    );
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Pass(EditKey::Paste)
    );
    ui.focus("field");
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, now),
        EguiKey::Pass(EditKey::Copy),
        "a focused field without a selection has nothing to copy"
    );
    assert!(pending.is_empty());
}

/// A key without a binding is left alone, and so is every key when no bindings are set.
#[test]
fn an_unbound_key_is_free() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("q"), &mut pending, now),
        EguiKey::Free
    );
    state.set_bindings(None);
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Free
    );
    assert!(pending.is_empty());
}

/// A copy with a selection is held and acts: the copied text comes out of the pass once.
#[test]
fn a_bound_copy_with_a_selection_copies() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.select("field", 0, 4);
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    let copied = ui.pass(&mut state, pending);
    assert_eq!(copied.as_deref(), Some("hell"));
    assert!(released(&mut state).replay.is_none());
}

/// The read-only field (a mirror address): a cut removes nothing, so the key is replayed and the
/// copy egui made is dropped; a copy there copies once.
#[test]
fn a_read_only_field_passes_cut_and_copies_copy() {
    let mut ui = Ui::new();
    ui.focus("ro");
    ui.select("ro", 0, 6);
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("x"), &mut pending, now),
        EguiKey::Taken
    );
    let copied = ui.pass(&mut state, pending);
    assert_eq!(copied, None, "a cut that removed nothing makes no copy");
    let release = released(&mut state);
    assert_eq!(release.replay, Some((press("x"), EditKey::Cut)));

    let mut pending = Vec::new();
    ui.select("ro", 0, 6);
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(ui.pass(&mut state, pending).as_deref(), Some("mirror"));
    assert!(released(&mut state).replay.is_none());
}

/// A paste into a read-only field changes nothing and the key is replayed once.
#[test]
fn a_paste_into_a_read_only_field_replays_the_key() {
    let mut ui = Ui::new();
    ui.focus("ro");
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(
        released(&mut state).replay,
        Some((press("v"), EditKey::Paste))
    );
}

/// A password field never copies, so a copy there is replayed.
#[test]
fn a_password_field_replays_copy() {
    let mut ui = Ui::new();
    ui.focus("pw");
    ui.select("pw", 0, 4);
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    assert_eq!(ui.pass(&mut state, pending), None);
    assert_eq!(
        released(&mut state).replay,
        Some((press("c"), EditKey::Copy))
    );
}

/// A click into the field and a bound paste before the same frame: the paste is held, not passed
/// on the stale focus, and lands in the clicked field.
#[test]
fn a_click_then_paste_lands_in_the_clicked_field() {
    let mut ui = Ui::new();
    ui.frame(Vec::new());
    let mut state = bound();
    let at = egui::pos2(20.0, 15.0);
    let click = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut pending = vec![egui::Event::PointerMoved(at), click(true), click(false)];
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken,
        "unprocessed input keeps the key from passing on the old focus"
    );
    ui.pass(&mut state, pending);
    assert!(
        ui.field.contains("CLIP") && ui.field.len() == "helloCLIP".len(),
        "pasted once at the click: {}",
        ui.field
    );
    assert!(released(&mut state).replay.is_none());
}

/// Ctrl+A then a bound copy before the same frame: the copy sees the new selection.
#[test]
fn select_all_then_copy_copies_the_new_selection() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = vec![egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }];
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    assert_eq!(ui.pass(&mut state, pending).as_deref(), Some("hello"));
    assert!(released(&mut state).replay.is_none());
}

/// Tab then a bound paste before the same frame: the paste follows the focus Tab moved.
#[test]
fn tab_then_paste_follows_the_new_focus() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = vec![egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.next, "CLIP");
    assert_eq!(ui.field, "hello");
    assert!(released(&mut state).replay.is_none());
}

/// Shift+Tab then a bound paste before the same frame: egui moves focus back only when the next
/// pass begins, and the paste is judged on that focus, not on the field Shift+Tab left.
#[test]
fn shift_tab_then_paste_follows_the_focus_of_the_next_pass() {
    let mut ui = Ui::new();
    ui.focus("next");
    let mut state = bound();
    let mut pending = vec![egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }];
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "helloCLIP");
    assert_eq!(ui.next, "");
    assert!(
        released(&mut state).replay.is_none(),
        "the paste ran in the field Shift+Tab moved to"
    );
}

/// A Tab from `from` whose pass already ran, then a bound paste before the next frame: egui moves
/// focus only when that next pass begins, so the paste waits for it and lands in `field`.
fn paste_after_a_finished_tab_pass(from: &str, shift: bool) {
    let mut ui = Ui::new();
    ui.focus(from);
    let mut state = bound();
    let tab = egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        },
    };
    ui.pass(&mut state, vec![tab]);
    assert!(!state.busy(), "the Tab ran with no key held");
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken,
        "the key waits for the pass that moves focus"
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "helloCLIP", "pasted where focus went");
    assert_eq!(ui.next, "");
    assert!(released(&mut state).replay.is_none(), "consumed");
}

/// Shift+Tab back to the previous field, then a paste after a frame boundary.
#[test]
fn a_paste_after_a_finished_shift_tab_pass_follows_the_deferred_focus() {
    paste_after_a_finished_tab_pass("next", true);
}

/// Tab from the last field wraps to the first one in the next pass, then a paste after a frame
/// boundary.
#[test]
fn a_paste_after_a_finished_wrapping_tab_pass_follows_the_deferred_focus() {
    paste_after_a_finished_tab_pass("pw", false);
}

/// Events after a held key wait behind it in order; a second bound key queues instead of being
/// judged on its own.
#[test]
fn later_events_queue_behind_the_held_key() {
    let mut ui = Ui::new();
    ui.focus("ro");
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(
        ui.judge(&mut state, &Ev::Other("release"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(pending.len(), 1, "queued events add no egui input");
    ui.pass(&mut state, pending);
    let release = released(&mut state);
    assert_eq!(release.replay, Some((press("v"), EditKey::Paste)));
    assert_eq!(release.queued, vec![Ev::Other("release"), press("c")]);
}

/// A held edit in a window whose frames stopped is taken back out of the input and its key
/// replayed once the limit has passed.
#[test]
fn a_stalled_hold_retracts_the_edit_and_replays() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    state.expire(now + Duration::from_millis(10), &mut pending, || {}, true);
    assert!(state.take_release(EditKeyOwner::EguiOnly).is_none());
    state.expire(now + HOST_HOLD_LIMIT, &mut pending, || {}, true);
    assert!(pending.is_empty(), "the edit left the input");
    assert_eq!(
        released(&mut state).replay,
        Some((press("v"), EditKey::Paste))
    );
}

/// A native Wayland read in flight holds the paste. Text for the same field is pasted; an empty or
/// failed read replays the key; a field that lost focus drops both whatever the read returned; a
/// read that never ends is cancelled and replayed.
#[test]
fn a_pending_wayland_read_holds_the_paste() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let now = Instant::now();

    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    assert!(state.awaits_text() && pending.is_empty());
    state.text_arrived(Some("WL".into()), &ui.ctx, &mut pending, now);
    assert_eq!(pending, vec![egui::Event::Paste("WL".into())]);
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "helloWL");
    assert!(released(&mut state).replay.is_none());

    for text in [None, Some(String::new())] {
        let mut pending = Vec::new();
        ui.judge(&mut state, &press("v"), &mut pending, now);
        state.text_arrived(text, &ui.ctx, &mut pending, now);
        assert!(pending.is_empty());
        assert_eq!(
            released(&mut state).replay,
            Some((press("v"), EditKey::Paste))
        );
    }

    for text in [Some("WL".to_string()), Some(String::new()), None] {
        ui.focus("field");
        let mut pending = Vec::new();
        ui.judge(&mut state, &press("v"), &mut pending, now);
        ui.focus("next");
        state.text_arrived(text.clone(), &ui.ctx, &mut pending, now);
        assert!(pending.is_empty());
        assert!(
            released(&mut state).replay.is_none(),
            "focus moved, read {text:?}: dropped"
        );
    }
    assert_eq!(ui.next, "", "nothing reached the field focus moved to");

    ui.focus("field");
    let mut pending = Vec::new();
    ui.judge(&mut state, &press("v"), &mut pending, now);
    let mut cancelled = false;
    state.expire(
        now + AWAIT_TEXT_LIMIT,
        &mut pending,
        || cancelled = true,
        false,
    );
    assert!(cancelled);
    assert_eq!(
        released(&mut state).replay,
        Some((press("v"), EditKey::Paste))
    );
}

/// A label selection with no field focused copies at key time without a hold.
#[test]
fn a_label_selection_copy_acts_at_once() {
    let mut ui = Ui::new();
    ui.frame(Vec::new());
    // Drag across the label to select it.
    let start = ui.label.left_center() + egui::vec2(1.0, 0.0);
    let end = ui.label.right_center() - egui::vec2(1.0, 0.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    ui.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
    ui.frame(vec![egui::Event::PointerMoved(end)]);
    ui.frame(vec![button(end, false)]);
    if !egui::text_selection::LabelSelectionState::load(&ui.ctx).has_selection() {
        // The label's place depends on egui's default style; fail loudly rather than pass empty.
        panic!("the drag did not select the label");
    }
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("c"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    assert_eq!(pending, vec![egui::Event::Copy]);
    assert!(!state.busy(), "an act holds nothing");
    let output = ui.frame(pending);
    let copied = super::super::text_selection::take_copied_text(output).1;
    assert!(copied.is_some_and(|t| !t.is_empty()));
}

/// Text typed before a bound cut in the same frame: the typed text runs first and the cut is judged
/// on what it left. With no selection the cut does nothing, so the typed change is not counted as
/// the cut's and the key goes on.
#[test]
fn typed_text_before_a_cut_is_not_the_cut_s_change() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = vec![egui::Event::Text("z".into())];
    assert_eq!(
        ui.judge(&mut state, &press("x"), &mut pending, Instant::now()),
        EguiKey::Taken,
        "unprocessed input holds the key until it has run"
    );
    assert_eq!(
        pending.len(),
        1,
        "nothing is injected before the earlier input runs"
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.field.matches('z').count(), 1, "typed once: {}", ui.field);
    assert_eq!(
        released(&mut state).replay,
        Some((press("x"), EditKey::Cut)),
        "a cut with nothing selected passes its key on"
    );
}

/// Pointer input waits behind a key whose edit is in egui's input, so a click (a copy button) never
/// shares the edit's pass and a cut that did nothing drops only its own copy. Before an edit goes
/// in, pointer input runs as usual.
#[test]
fn pointer_input_waits_while_the_edit_is_in_the_input() {
    let mut ui = Ui::new();
    ui.focus("ro");
    ui.select("ro", 0, 6);
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    assert_eq!(
        ui.judge(&mut state, &press("x"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(
        ui.judge(&mut state, &Ev::Pointer("click"), &mut pending, now),
        EguiKey::Taken,
        "a click behind the injected cut waits"
    );
    ui.pass(&mut state, pending);
    let release = released(&mut state);
    assert_eq!(release.replay, Some((press("x"), EditKey::Cut)));
    assert_eq!(release.queued, vec![Ev::Pointer("click")]);

    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(
        ui.judge(&mut state, &Ev::Pointer("move"), &mut pending, now),
        EguiKey::Free,
        "while a Wayland read waits, a click that moves focus is the user leaving the field"
    );
}

#[path = "edit_keys_hold_tests.rs"]
mod hold;
