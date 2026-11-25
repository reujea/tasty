//! The hold's order, timing and result rules: pointer input behind a settling key, the frame
//! timeout, a paste that leaves the text as it was, a recheck for the owner, and the replay order.
use super::*;

/// Pointer input after a key that waits for earlier input queues behind it, so the earlier input's
/// pass holds only what came before the key and the edit lands where the user was.
#[test]
fn pointer_input_after_a_settling_key_waits() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let now = Instant::now();
    let mut pending = vec![egui::Event::Text("z".into())];
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, now),
        EguiKey::Taken
    );
    assert_eq!(
        ui.judge(&mut state, &Ev::Pointer("click next"), &mut pending, now),
        EguiKey::Taken,
        "a click after the key waits behind it"
    );
    assert_eq!(pending.len(), 1, "the earlier input runs alone");
    ui.pass(&mut state, pending);
    assert!(ui.field.ends_with("zCLIP"), "pasted after z: {}", ui.field);
    let release = released(&mut state);
    assert!(release.replay.is_none());
    assert_eq!(release.queued_events(), vec![Ev::Pointer("click next")]);
}

/// A settling key that then waits for a Wayland read keeps the pointer input queued behind it, and
/// later pointer input queues too instead of overtaking it.
#[test]
fn pointer_input_keeps_its_order_when_a_settled_key_waits_for_a_read() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let mut state = bound();
    let now = Instant::now();
    let mut pending = vec![egui::Event::Text("z".into())];
    ui.judge(&mut state, &press("v"), &mut pending, now);
    assert_eq!(
        ui.judge(&mut state, &Ev::Pointer("first"), &mut pending, now),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert!(state.awaits_text(), "the settled paste waits for the read");
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &Ev::Pointer("second"), &mut pending, now),
        EguiKey::Taken,
        "a later click does not overtake the queued one"
    );
    state.text_arrived(Some("WL".into()), &ui.ctx, &mut pending, now);
    ui.pass(&mut state, pending);
    let release = released(&mut state);
    assert!(release.replay.is_none());
    assert_eq!(
        release.queued_events(),
        vec![Ev::Pointer("first"), Ev::Pointer("second")]
    );
}

/// A window that still draws is never timed out: a frame that comes late runs the edit and its
/// result decides.
#[test]
fn a_late_frame_still_decides() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    ui.judge(&mut state, &press("v"), &mut pending, now);
    state.expire(now + Duration::from_secs(5), &mut pending, || {}, false);
    assert!(state.take_release(EditKeyOwner::EguiOnly).is_none());
    assert_eq!(pending, vec![egui::Event::Paste("CLIP".into())]);
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "helloCLIP");
    assert!(released(&mut state).replay.is_none());
}

/// The frame timeout counts from when the edit went in, not from the key: text a Wayland read
/// returned late gets the full limit before a stopped window gives it up.
#[test]
fn the_frame_timeout_counts_from_the_injection() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let mut state = bound();
    let mut pending = Vec::new();
    let now = Instant::now();
    ui.judge(&mut state, &press("v"), &mut pending, now);
    let arrived = now + Duration::from_secs(2);
    state.text_arrived(Some("WL".into()), &ui.ctx, &mut pending, arrived);
    let almost = arrived + HOST_HOLD_LIMIT - Duration::from_millis(10);
    state.expire(almost, &mut pending, || {}, true);
    assert!(
        state.take_release(EditKeyOwner::EguiOnly).is_none(),
        "the read's own wait does not count"
    );
    state.expire(arrived + HOST_HOLD_LIMIT, &mut pending, || {}, true);
    assert!(pending.is_empty(), "the edit left the input");
    assert_eq!(
        released(&mut state).replay,
        Some((press("v"), EditKey::Paste))
    );
}

/// The same text pasted over itself is a paste that ran: the key is consumed, not replayed (which
/// would paste twice through egui-winit's own Ctrl+V or type `v` for Alt+V).
#[test]
fn the_same_text_pasted_over_itself_consumes_the_key() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.select("field", 0, 5);
    ui.clipboard = ClipText::Text("hello".into());
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "hello");
    assert!(released(&mut state).replay.is_none());
}

/// A password replaced by text of the same length reads the same masked: the paste still ran.
#[test]
fn a_same_length_password_paste_consumes_the_key() {
    let mut ui = Ui::new();
    ui.focus("pw");
    ui.select("pw", 0, 4);
    let mut state = bound();
    let mut pending = Vec::new();
    assert_eq!(
        ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.secret, "CLIP");
    assert!(released(&mut state).replay.is_none());
}

/// A key the owner cannot place yet waits for the earlier input's pass and then goes back to the
/// owner unjudged, first in line. With no earlier input, or no binding, it is free at once.
#[test]
fn an_unsettled_key_goes_back_to_its_owner_after_the_earlier_input() {
    let mut ui = Ui::new();
    ui.frame(Vec::new());
    let mut state = bound();
    let now = Instant::now();
    let judge = |ui: &mut Ui, state: &mut EditKeyState<Ev>, event: &Ev, pending: &mut Vec<_>| {
        let clip = ui.clipboard.clone();
        let mut read = |_tag: u64| clip.clone();
        state.judge_unsettled(
            event,
            EditKeyOwner::Window,
            now,
            KeyCx {
                ctx: &ui.ctx,
                events: pending,
                clipboard: &mut read,
            },
        )
    };
    let mut pending = Vec::new();
    assert_eq!(
        judge(&mut ui, &mut state, &press("v"), &mut pending),
        EguiKey::Free
    );
    let at = egui::pos2(20.0, 15.0);
    let click = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut pending = vec![egui::Event::PointerMoved(at), click(true), click(false)];
    assert_eq!(
        judge(&mut ui, &mut state, &press("q"), &mut pending),
        EguiKey::Free
    );
    assert_eq!(
        judge(&mut ui, &mut state, &press("v"), &mut pending),
        EguiKey::Taken
    );
    assert_eq!(
        judge(&mut ui, &mut state, &press("q"), &mut pending),
        EguiKey::Taken
    );
    ui.pass(&mut state, pending);
    assert_eq!(ui.field, "hello", "the owner judges the key, not the hold");
    let release = state
        .take_release(EditKeyOwner::Window)
        .expect("the key goes back");
    assert!(release.replay.is_none());
    assert_eq!(release.queued_events(), vec![press("v"), press("q")]);
}

/// A window replaying a finished hold gives the released key once, marked with its edit, then the
/// queued events unmarked with their arrival times, and stops when the window closes.
#[test]
fn a_release_replays_in_order_and_stops_when_the_window_closes() {
    let arrived = Instant::now();
    let release = Release::of_events(
        Some((press("c"), EditKey::Copy)),
        vec![Ev::Other("escape"), Ev::Other("after")],
        vec![arrived, arrived],
    );
    let mut seen = Vec::new();
    let closed = replay_release(
        release,
        |closed: &bool| *closed,
        |step| {
            let (event, judged, at) = match step {
                Replayed::Event(event, judged, at) => (event, judged, at),
                #[cfg(debug_assertions)]
                Replayed::Debug(_) => unreachable!("the line holds only window events"),
            };
            seen.push((event.clone(), judged, at));
            event == Ev::Other("escape")
        },
    );
    assert_eq!(closed, Some(true));
    assert_eq!(
        seen,
        vec![
            (press("c"), Some(EditKey::Copy), None),
            (Ev::Other("escape"), None, Some(arrived))
        ]
    );
}

/// An event queued behind a hold keeps the arrival time the window set when it came, and a later
/// arrival keeps its own, so a replay long after still judges by when each one came.
#[test]
fn a_queued_event_keeps_the_arrival_the_window_set() {
    let mut ui = Ui::new();
    ui.focus("field");
    let mut state = bound();
    let now = Instant::now();
    let first = now - Duration::from_millis(300);
    let second = now - Duration::from_millis(100);
    let mut pending = vec![egui::Event::Text("z".into())];
    ui.judge(&mut state, &press("v"), &mut pending, now);
    state.replace_arrival(Some(first));
    ui.judge(&mut state, &Ev::Other("tap"), &mut pending, now);
    state.replace_arrival(Some(second));
    ui.judge(&mut state, &Ev::Other("tap again"), &mut pending, now);
    ui.pass(&mut state, pending);
    let queued = released(&mut state).queued_arrivals();
    assert_eq!(
        queued,
        vec![(Ev::Other("tap"), first), (Ev::Other("tap again"), second)]
    );
}

/// Real window events keep their place behind a held key by their winit variant: a wheel or a
/// pinch after a queued pointer move waits too, so it cannot act where the pointer was before the
/// move. Window state that no result depends on stays free.
#[test]
fn a_wheel_after_a_queued_pointer_move_waits_too() {
    use winit::event::{DeviceId, MouseScrollDelta, TouchPhase};
    let ctx = egui::Context::default();
    let mut state = EditKeyState::<WindowEvent>::default();
    let now = Instant::now();
    let owner = EditKeyOwner::EguiOnly;
    let key = WindowEvent::RedrawRequested;
    state.hold_key(&key, EditKey::Paste, Phase::Settle, now, owner, false);
    let device_id = DeviceId::dummy();
    let moved = WindowEvent::CursorMoved {
        device_id,
        position: (40.0, 40.0).into(),
    };
    let wheel = WindowEvent::MouseWheel {
        device_id,
        delta: MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, -20.0)),
        phase: TouchPhase::Moved,
    };
    let pinch = WindowEvent::PinchGesture {
        device_id,
        delta: 0.1,
        phase: TouchPhase::Moved,
    };
    let resized = WindowEvent::Resized(winit::dpi::PhysicalSize::new(800, 600));
    let mut judged = Vec::new();
    for event in [&moved, &wheel, &pinch, &resized] {
        let mut events = Vec::new();
        let mut clipboard = |_tag: u64| ClipText::Empty;
        let cx = KeyCx {
            ctx: &ctx,
            events: &mut events,
            clipboard: &mut clipboard,
        };
        judged.push(state.judge(event, owner, now, cx));
    }
    assert_eq!(
        judged,
        [
            EguiKey::Taken,
            EguiKey::Taken,
            EguiKey::Taken,
            EguiKey::Free
        ]
    );
    state.resolve_now(Outcome::NotActed);
    let release = state.take_release(owner).expect("released");
    assert_eq!(release.queued_events(), vec![moved, wheel, pinch]);
}

/// A focus loss while a key is held queues behind it like the keys before it. The modifiers it
/// clears stay for the queued key until the focus loss itself replays, so a queued Alt+C is still
/// judged as a copy, and only the input after the focus loss sees them cleared.
#[test]
fn a_focus_loss_while_held_keeps_the_replayed_edit_modifiers() {
    let ctx = egui::Context::default();
    let settings = crate::settings::KeybindingSettings {
        copy: vec!["alt+c".into()],
        ..Default::default()
    };
    let mut state = EditKeyState::<WindowEvent>::default();
    state.set_bindings(Some(EditBindings::from_settings(&settings)));
    state.set_modifiers(alt());
    let now = Instant::now();
    let owner = EditKeyOwner::EguiOnly;
    state.hold_key(
        &WindowEvent::RedrawRequested,
        EditKey::Paste,
        Phase::Settle,
        now,
        owner,
        false,
    );
    // The queued key stands for Alt+C: winit key events cannot be built outside winit.
    let queued_key = WindowEvent::Ime(winit::event::Ime::Commit("alt+c".into()));
    let arrivals = [
        queued_key.clone(),
        WindowEvent::Focused(false),
        WindowEvent::Focused(true),
    ];
    for event in &arrivals {
        let mut events = Vec::new();
        let mut clipboard = |_tag: u64| ClipText::Empty;
        let cx = KeyCx {
            ctx: &ctx,
            events: &mut events,
            clipboard: &mut clipboard,
        };
        // The window runs a free event at once, which applies its modifiers (`GpuState`).
        if state.judge(event, owner, now, cx) == EguiKey::Free
            && let Some(modifiers) = window_modifiers(event)
        {
            state.set_modifiers(modifiers);
        }
    }
    assert_eq!(
        state.modifiers,
        alt(),
        "the held focus loss has not run yet"
    );
    state.resolve_now(Outcome::NotActed);
    let release = state.take_release(owner).expect("released");
    assert_eq!(release.queued_events(), arrivals);
    let mut seen = Vec::new();
    for event in release.queued_events() {
        if event == queued_key {
            let lookup = tasty_key_match::shortcut_lookup_key(
                &Key::Character("c".into()),
                &physical_of(&Key::Character("c".into())),
                state.modifiers,
            );
            let bindings = state.bindings.as_ref().expect("bound");
            seen.push(bindings.binding(&lookup, state.modifiers));
        } else if let Some(modifiers) = window_modifiers(&event) {
            state.set_modifiers(modifiers);
        }
    }
    assert_eq!(seen, [Some(EditKey::Copy)]);
    assert_eq!(state.modifiers, ModifiersState::default());
}

/// A click a Wayland read let through is still unprocessed when the read finishes. Its pass runs
/// first: a click into another field drops the text and the key whatever the read returned, and a
/// click inside the same field still gets the paste.
#[test]
fn a_wayland_read_waits_for_a_click_it_let_through() {
    let click = |at: egui::Pos2| {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![egui::Event::PointerMoved(at), button(true), button(false)]
    };
    let mut ui = Ui::new();
    ui.clipboard = ClipText::Pending;
    let mut state = bound();
    let now = Instant::now();
    for text in [Some("WL".to_string()), Some(String::new()), None] {
        ui.focus("field");
        let mut pending = Vec::new();
        ui.judge(&mut state, &press("v"), &mut pending, now);
        assert_eq!(
            ui.judge(&mut state, &Ev::Pointer("click"), &mut pending, now),
            EguiKey::Free,
            "a click goes on while the read waits"
        );
        pending.extend(click(ui.next_at.center()));
        state.text_arrived(text.clone(), &ui.ctx, &mut pending, now);
        assert!(
            !pending.iter().any(|e| matches!(e, egui::Event::Paste(_))),
            "the paste does not ride the click's pass"
        );
        ui.pass(&mut state, pending);
        assert!(
            released(&mut state).replay.is_none(),
            "focus moved, read {text:?}: dropped"
        );
    }
    assert_eq!(ui.next, "", "nothing reached the clicked field");
    assert_eq!(ui.field, "hello");

    ui.focus("field");
    let mut pending = Vec::new();
    ui.judge(&mut state, &press("v"), &mut pending, now);
    let field_at = ui.next_at.center() - egui::vec2(0.0, ui.next_at.height() + 4.0);
    pending.extend(click(field_at));
    state.text_arrived(Some("WL".into()), &ui.ctx, &mut pending, now);
    ui.pass(&mut state, pending);
    assert!(ui.field.contains("WL"), "pasted in place: {}", ui.field);
    assert!(released(&mut state).replay.is_none());
}

/// A field disabled or no longer drawn in the pass that should run the paste did not run it, even
/// though its stored selection still ends past where the paste would start: the key is replayed.
#[test]
fn a_paste_into_a_field_gone_from_its_pass_replays_the_key() {
    for disabled in [true, false] {
        let mut ui = Ui::new();
        ui.focus("field");
        ui.select("field", 0, 5);
        let mut state = bound();
        let mut pending = Vec::new();
        assert_eq!(
            ui.judge(&mut state, &press("v"), &mut pending, Instant::now()),
            EguiKey::Taken
        );
        if disabled {
            ui.field_enabled = false;
        } else {
            ui.field_drawn = false;
        }
        ui.pass(&mut state, pending);
        assert_eq!(ui.field, "hello");
        assert_eq!(
            released(&mut state).replay,
            Some((press("v"), EditKey::Paste)),
            "disabled {disabled}: the paste did not run"
        );
    }
}

/// The front hold check for one event with `pending` egui input not yet drawn.
fn front(
    state: &mut EditKeyState<Ev>,
    event: &impl LineEntry<Ev>,
    pending: &[egui::Event],
    handed: bool,
) -> bool {
    state.hold_front(event, EditKeyOwner::Window, pending, handed, Instant::now())
}

/// The window's own release after the pass that took `pending`.
fn front_release(ui: &mut Ui, state: &mut EditKeyState<Ev>, pending: Vec<egui::Event>) -> Vec<Ev> {
    ui.pass(state, pending);
    state
        .take_release(EditKeyOwner::Window)
        .expect("the line is released after the pass")
        .queued_events()
}

/// A key after egui input that is not drawn yet (a letter typed into a palette that may close it)
/// waits at the front for the pass that takes that input, with what came after it, and then runs
/// again from the window's input entry.
#[test]
fn a_key_after_unsettled_egui_input_waits_for_its_pass() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    assert!(front(&mut state, &press("a"), &pending, false));
    assert!(front(&mut state, &Ev::Pointer("click"), &pending, false));
    assert!(front(&mut state, &press("b"), &pending, false));
    assert_eq!(
        front_release(&mut ui, &mut state, pending),
        vec![press("a"), Ev::Pointer("click"), press("b")]
    );
}

/// With no input waiting for a pass, a key goes on at once, and pointer input never waits alone.
#[test]
fn a_key_with_no_pending_input_runs_now() {
    let mut state = EditKeyState::<Ev>::default();
    assert!(!front(&mut state, &press("a"), &[], false));
    let pending = vec![egui::Event::Text("z".into())];
    assert!(!front(&mut state, &Ev::Pointer("click"), &pending, false));
    assert!(!state.busy());
}

/// The App plugin step sits after the front check, so a plugin shortcut with nothing waiting runs
/// at once (its waiting counterpart is the front check order, `press_focus_call_sites`).
#[test]
fn a_plugin_shortcut_with_no_pending_input_runs_now() {
    let mut state = EditKeyState::<Ev>::default();
    assert!(!front(&mut state, &press("p"), &[], false));
}

/// A separate window with nothing waiting runs a key at once (its waiting counterpart is the entry
/// order, `press_focus_call_sites`).
#[test]
fn a_separate_window_key_with_no_pending_input_runs_now() {
    let mut state = EditKeyState::<Ev>::default();
    assert!(!front(&mut state, &press("y"), &[], false));
}

/// An Escape the window took out of egui's input for a popup is not in that input, but its pass
/// may close the popup, so the next key waits for it too.
#[test]
fn a_key_after_a_handed_escape_waits_for_its_pass() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    assert!(front(&mut state, &press("a"), &[], true));
    assert_eq!(
        front_release(&mut ui, &mut state, Vec::new()),
        vec![press("a")]
    );
}

/// A pass that took a Tab moves focus only when the next pass begins, so a key right after it
/// waits one more pass even with no input left.
#[test]
fn a_key_after_a_tab_focus_move_waits_for_the_next_pass() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    ui.pass(&mut state, vec![tab()]);
    assert!(front(&mut state, &press("a"), &[], false));
}

/// A pass with no Tab leaves nothing deferred: the next key goes on.
#[test]
fn a_key_after_a_tab_with_no_focus_move_runs_now() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    ui.pass(&mut state, vec![egui::Event::Text("z".into())]);
    assert!(!front(&mut state, &press("a"), &[], false));
}

/// A released line replays from the front: when its first key meets new unsettled input it starts a
/// line again, and the rest of the replay and later keys stay behind it in arrival order.
#[test]
fn a_later_key_stays_behind_a_replayed_key_that_meets_a_new_hold() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    front(&mut state, &press("a"), &pending, false);
    front(&mut state, &press("b"), &pending, false);
    let replay = front_release(&mut ui, &mut state, pending);
    // The first replayed key finds new input (from a key the pass left) and holds again.
    let again = vec![egui::Event::Text("y".into())];
    for event in &replay {
        assert!(front(&mut state, event, &again, false));
    }
    assert!(front(&mut state, &press("c"), &again, false));
    assert_eq!(
        front_release(&mut ui, &mut state, again),
        vec![press("a"), press("b"), press("c")]
    );
}

fn tab() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// The labels of a released line in order: window events by their name, debug injections as text.
#[cfg(debug_assertions)]
fn line_labels(release: Release<Ev>) -> Vec<String> {
    use crate::view::main::debug_input::DebugInput;
    release
        .split()
        .1
        .into_iter()
        .map(|item| match item {
            LineItem::Event(event, _) => format!("{event:?}"),
            LineItem::Debug(DebugInput::Text(text)) => format!("debug {text}"),
            LineItem::Debug(other) => format!("debug {other:?}"),
        })
        .collect()
}

/// [held window key A, debug B, window key C]: the debug injection takes its place in the same line
/// as the window events and the release gives them back in the order they came, so C does not run
/// before B.
#[cfg(debug_assertions)]
#[test]
fn a_debug_input_takes_its_place_in_the_hold_line() {
    use crate::view::main::debug_input::DebugInput;
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    assert!(front(&mut state, &press("a"), &pending, false));
    assert!(
        front(&mut state, &DebugInput::Text("b".into()), &pending, false),
        "the debug input waits in the open line"
    );
    assert!(front(&mut state, &press("c"), &pending, false));
    ui.pass(&mut state, pending);
    let release = state
        .take_release(EditKeyOwner::Window)
        .expect("the line is released after the pass");
    assert_eq!(
        line_labels(release),
        vec![
            format!("{:?}", press("a")),
            "debug b".to_string(),
            format!("{:?}", press("c"))
        ]
    );
}

/// With no line open a debug injection comes back to run through the window's entry now.
#[cfg(debug_assertions)]
#[test]
fn a_debug_input_without_a_line_runs_now() {
    use crate::view::main::debug_input::DebugInput;
    let mut state = EditKeyState::<Ev>::default();
    assert!(!front(
        &mut state,
        &DebugInput::Text("b".into()),
        &[],
        false
    ));
    assert!(!state.busy(), "nothing waits");
}

/// A debug pointer press, as the window entry hands it over.
#[cfg(debug_assertions)]
fn debug_press() -> crate::view::main::debug_input::DebugInput {
    crate::view::main::debug_input::DebugInput::Pointer {
        fx: 0.5,
        fy: 0.5,
        surface_id: None,
        action: crate::view::main::debug_input::InjectPointer::Button {
            button: winit::event::MouseButton::Left,
            pressed: true,
        },
        modifiers: None,
    }
}

/// A debug key press, as the window entry hands it over.
#[cfg(debug_assertions)]
fn debug_key() -> crate::view::main::debug_input::DebugInput {
    crate::view::main::debug_input::DebugInput::Key {
        key: egui::Key::A,
        pressed: true,
        modifiers: None,
    }
}

/// Given a paste waiting for a Wayland read with nothing queued behind it, When a debug pointer
/// press comes, Then it runs now like a real click (the user leaving the field), as the one front
/// judgment (`queues`) reads it.
#[cfg(debug_assertions)]
#[test]
fn a_debug_press_while_a_paste_reads_with_nothing_queued_runs_now() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let mut state = bound();
    let mut pending = Vec::new();
    ui.judge(&mut state, &press("v"), &mut pending, Instant::now());
    assert!(state.awaits_text());
    assert!(!front(&mut state, &debug_press(), &pending, false));
}

/// Given the same read with a key queued behind it, When a debug pointer press comes, Then it waits
/// behind that key, as a real click does.
#[cfg(debug_assertions)]
#[test]
fn a_debug_press_behind_a_queued_key_while_a_paste_reads_waits() {
    let mut ui = Ui::new();
    ui.focus("field");
    ui.clipboard = ClipText::Pending;
    let mut state = bound();
    let mut pending = Vec::new();
    ui.judge(&mut state, &press("v"), &mut pending, Instant::now());
    assert!(front(&mut state, &press("q"), &pending, false));
    assert!(front(&mut state, &debug_press(), &pending, false));
}

/// Given egui input a replayed item left that a pass has not taken (it may move focus or close a
/// popup), When a debug key comes with no line open, Then it starts a settle line like a window key
/// and comes back once that pass ran.
#[cfg(debug_assertions)]
#[test]
fn a_debug_key_after_unsettled_input_waits_for_its_pass() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    assert!(front(&mut state, &debug_key(), &pending, false));
    ui.pass(&mut state, pending);
    let release = state
        .take_release(EditKeyOwner::Window)
        .expect("the line is released after the pass");
    assert!(matches!(
        release.split().1[..],
        [LineItem::Debug(
            crate::view::main::debug_input::DebugInput::Key {
                key: egui::Key::A,
                ..
            }
        )]
    ));
}

/// Given no egui input waiting for a pass, When a debug key comes, Then it runs now.
#[cfg(debug_assertions)]
#[test]
fn a_debug_key_with_no_pending_input_runs_now() {
    let mut state = EditKeyState::<Ev>::default();
    assert!(!front(&mut state, &debug_key(), &[], false));
    assert!(!state.busy());
}

/// Given a settle line whose first key reached the window at t4 and a key behind it that reached it
/// earlier, at t2 (a replay put back ahead of newer input), When the X11 cleanup reads the wait,
/// Then the earliest arrival is the boundary, not the first in line.
#[test]
fn the_earliest_arrival_in_a_line_bounds_the_wait() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    state.replace_arrival(Some(t0 + ms(4)));
    assert!(front(&mut state, &press("a"), &pending, false));
    state.replace_arrival(Some(t0 + ms(2)));
    assert!(front(&mut state, &press("b"), &pending, false));
    state.replace_arrival(None);
    assert_eq!(state.oldest_waiting(), Some(t0 + ms(2)));
}

/// Given a line whose keys came in order, When the cleanup reads the wait, Then it is the first
/// key's arrival.
#[test]
fn an_ordered_line_is_bounded_by_its_first_arrival() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    state.replace_arrival(Some(t0 + ms(2)));
    assert!(front(&mut state, &press("a"), &pending, false));
    state.replace_arrival(Some(t0 + ms(4)));
    assert!(front(&mut state, &press("b"), &pending, false));
    state.replace_arrival(None);
    assert_eq!(state.oldest_waiting(), Some(t0 + ms(2)));
}

/// Given a bound key the owner could not place yet, held for a recheck at its arrival t0 with a key
/// queued behind it at t1, When the earlier input's pass hands it back first in line, Then it keeps
/// t0: the wait is bounded by t0 and its replay reads when it came.
#[test]
fn a_rechecked_key_keeps_its_first_arrival() {
    let mut ui = Ui::new();
    ui.frame(Vec::new());
    let mut state = bound();
    let t0 = Instant::now();
    let t1 = t0 + Duration::from_millis(1);
    let mut pending = vec![egui::Event::Text("z".into())];
    for (event, at) in [(press("v"), t0), (press("q"), t1)] {
        state.replace_arrival(Some(at));
        let clip = ui.clipboard.clone();
        let mut read = |_tag: u64| clip.clone();
        let judged = state.judge_unsettled(
            &event,
            EditKeyOwner::Window,
            at,
            KeyCx {
                ctx: &ui.ctx,
                events: &mut pending,
                clipboard: &mut read,
            },
        );
        assert_eq!(judged, EguiKey::Taken);
    }
    state.replace_arrival(None);
    assert_eq!(state.oldest_waiting(), Some(t0), "the held key waits");
    ui.pass(&mut state, pending);
    assert_eq!(
        state.oldest_waiting(),
        Some(t0),
        "the handed back key keeps its arrival"
    );
    let release = state
        .take_release(EditKeyOwner::Window)
        .expect("the key goes back");
    let arrivals: Vec<_> = release
        .split()
        .1
        .iter()
        .filter_map(LineItem::arrived_at)
        .collect();
    assert_eq!(arrivals, vec![t0, t1]);
}
