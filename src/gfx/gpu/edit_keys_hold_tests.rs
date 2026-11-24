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
    assert_eq!(release.queued, vec![Ev::Pointer("click next")]);
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
        release.queued,
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
    assert_eq!(release.queued, vec![press("v"), press("q")]);
}

/// A window replaying a finished hold gives the released key once, marked with its edit, then the
/// queued events unmarked with their arrival times, and stops when the window closes.
#[test]
fn a_release_replays_in_order_and_stops_when_the_window_closes() {
    let arrived = Instant::now();
    let release = Release {
        replay: Some((press("c"), EditKey::Copy)),
        queued: vec![Ev::Other("escape"), Ev::Other("after")],
        queued_at: vec![arrived, arrived],
    };
    let mut seen = Vec::new();
    let closed = replay_release(
        release,
        |closed: &bool| *closed,
        |event, judged, at| {
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
    let (_, queued) = released(&mut state).split();
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
    assert_eq!(release.queued, vec![moved, wheel, pinch]);
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
    assert_eq!(release.queued, arrivals);
    let mut seen = Vec::new();
    for event in release.queued {
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
fn front(state: &mut EditKeyState<Ev>, event: &Ev, pending: &[egui::Event], handed: bool) -> bool {
    state.hold_front(event, EditKeyOwner::Window, pending, handed, Instant::now())
}

/// The window's own release after the pass that took `pending`.
fn front_release(ui: &mut Ui, state: &mut EditKeyState<Ev>, pending: Vec<egui::Event>) -> Vec<Ev> {
    ui.pass(state, pending);
    let (_, queued) = state
        .take_release(EditKeyOwner::Window)
        .expect("the line is released after the pass")
        .split();
    queued.into_iter().map(|(event, _)| event).collect()
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

/// A debug injection (text, key, clipboard) while a key waits in the line waits behind it: it goes
/// into egui's input only after the line has run, so it is not drawn before that key.
#[test]
fn a_debug_text_after_a_held_key_draws_after_it() {
    let mut ui = Ui::new();
    let mut state = EditKeyState::<Ev>::default();
    let pending = vec![egui::Event::Text("z".into())];
    front(&mut state, &press("a"), &pending, false);
    let typed = vec![egui::Event::Text("debug".into())];
    assert_eq!(state.hold_injection(typed.clone()), None);
    assert!(state.take_injections().is_empty(), "the line is still open");
    assert_eq!(
        front_release(&mut ui, &mut state, pending),
        vec![press("a")]
    );
    assert_eq!(state.take_injections(), typed);
}

/// With no line open a debug injection goes into egui's input at once.
#[test]
fn a_debug_text_without_a_hold_draws_now() {
    let mut state = EditKeyState::<Ev>::default();
    let typed = vec![egui::Event::Text("debug".into())];
    assert_eq!(state.hold_injection(typed.clone()), Some(typed));
}
