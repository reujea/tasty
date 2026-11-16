//! Exercise the real popup draw lifecycle, including its temporarily moved manager.
use super::*;
use crate::model::{PhysicalPx, PhysicalRect};
use crate::state::tests::test_state;

#[test]
fn search_input_focus_text_and_invalid_edge_survive_popup_draw() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let ctx = egui::Context::default();
    state.popups.open_at_focused(
        SEARCH_BAR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    let frame = |state: &mut MainViewState, events, modifiers| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                crate::adapters::ui::draw_popups(
                    ctx,
                    state,
                    &engine.read(),
                    &[],
                    PhysicalRect {
                        x: PhysicalPx(0.0),
                        y: PhysicalPx(0.0),
                        width: PhysicalPx(1280.0),
                        height: PhysicalPx(720.0),
                    },
                    1.0,
                );
            },
        )
    };
    for _ in 0..20 {
        drop(frame(&mut state, vec![], egui::Modifiers::NONE));
    }
    assert!(
        ctx.memory(|m| m.focused().is_some()),
        "open requests text focus without a click"
    );
    drop(frame(
        &mut state,
        vec![egui::Event::Text("needle".into())],
        egui::Modifiers::NONE,
    ));
    assert_eq!(state.search.query, "needle");
    state.search.last_error = Some("invalid regex".into());
    let focused = frame(&mut state, vec![], egui::Modifiers::NONE);
    let red = crate::theme::theme().input_border_invalid().to_egui();
    fn count_red(shape: &egui::Shape, red: egui::Color32) -> usize {
        match shape {
            egui::Shape::Rect(r) if r.stroke.color == red => 1,
            egui::Shape::Vec(children) => children.iter().map(|s| count_red(s, red)).sum(),
            _ => 0,
        }
    }
    let red_boxes = |out: &egui::FullOutput| {
        out.shapes
            .iter()
            .map(|s| count_red(&s.shape, red))
            .sum::<usize>()
    };
    assert_eq!(red_boxes(&focused), 2, "invalid focused edge and ring");
    let ctrl = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    drop(frame(
        &mut state,
        vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: Some(egui::Key::F),
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
        ctrl,
    ));
    assert!(state.popups.is_open(SEARCH_BAR_POPUP_ID));
    assert!(
        !state.popups.is_focused(SEARCH_BAR_POPUP_ID),
        "find releases the restored manager's focus"
    );
    let blurred = frame(&mut state, vec![], egui::Modifiers::NONE);
    assert!(ctx.memory(|m| m.focused().is_none()));
    assert_eq!(state.search.query, "needle");
    assert_eq!(
        red_boxes(&blurred),
        1,
        "invalid edge remains without a ring"
    );
}

#[test]
fn search_input_has_canonical_height_and_unclipped_ring_in_the_sized_shell() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let ctx = egui::Context::default();
    state.popups.open_at_focused(
        SEARCH_BAR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    // An undersized stale instance must be repaired by the registered per-frame sizer.
    state.popups.get_mut(SEARCH_BAR_POPUP_ID).unwrap().size.y = 1.0;
    let mut last = None;
    for _ in 0..20 {
        last = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                ..Default::default()
            },
            |ctx| {
                crate::adapters::ui::draw_popups(
                    ctx,
                    &mut state,
                    &engine.read(),
                    &[],
                    PhysicalRect {
                        x: PhysicalPx(0.0),
                        y: PhysicalPx(0.0),
                        width: PhysicalPx(1280.0),
                        height: PhysicalPx(720.0),
                    },
                    1.0,
                );
            },
        ));
    }
    let theme = crate::theme::theme();
    let out = last.unwrap();
    let (input, clip) = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(rect) if rect.fill == theme.input_raised_bg().to_egui() => {
                Some((rect.rect, s.clip_rect))
            }
            _ => None,
        })
        .expect("actual search Input fill");
    let popup = state.popups.get_mut(SEARCH_BAR_POPUP_ID).unwrap();
    let margin = crate::adapters::ui::popup::content_margin().value();
    assert_eq!(input.height(), theme.input_height().value());
    assert_eq!(popup.size.y - 2.0 * margin, theme.input_height().value());
    // Input's focus ring uses an expanded rect and an outside stroke, both one border token.
    assert!(clip.contains_rect(input.expand(2.0 * theme.border_width.value())));
}

/// Runs one egui frame through the real popup draw path used by the app.
fn escape_frame(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    events: Vec<egui::Event>,
) {
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            crate::adapters::ui::draw_popups(
                ctx,
                state,
                engine,
                &[],
                PhysicalRect {
                    x: PhysicalPx(0.0),
                    y: PhysicalPx(0.0),
                    width: PhysicalPx(1280.0),
                    height: PhysicalPx(720.0),
                },
                1.0,
            );
        },
    ));
}

fn escape_event() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Opens the bar with keyboard focus on its input and types a query.
fn open_with_query(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.popups.open_at_focused(
        SEARCH_BAR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    for _ in 0..20 {
        escape_frame(ctx, state, engine, vec![]);
    }
    assert!(ctx.memory(|m| m.focused().is_some()), "input took focus");
    escape_frame(ctx, state, engine, vec![egui::Event::Text("needle".into())]);
    assert_eq!(state.search.query, "needle");
}

#[test]
fn escape_closes_the_bar_after_the_winit_stage_released_its_popup_focus() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    open_with_query(&ctx, &mut state, &engine);
    // The winit Escape stage runs before the frame that carries the same Escape. These are the
    // two calls of try_consume_escape_key in src/view/main/keyboard.rs; change them together.
    assert_eq!(
        state.popups.focused_dismissal_target(),
        Some((SEARCH_BAR_POPUP_ID, false)),
        "the search bar is the only focused popup"
    );
    if let Some((id, closes)) = state.popups.focused_dismissal_target() {
        state.popups.set_focused(id, false);
        assert!(!closes);
    }
    assert!(!state.popups.is_focused(SEARCH_BAR_POPUP_ID));
    assert!(
        state.popups.is_open(SEARCH_BAR_POPUP_ID),
        "the winit stage does not close"
    );
    escape_frame(&ctx, &mut state, &engine, vec![escape_event()]);
    assert!(
        !state.popups.is_open(SEARCH_BAR_POPUP_ID),
        "Escape that ended the input's focus closes the bar"
    );
    assert!(state.search.query.is_empty(), "highlights are cleared");
}

#[test]
fn escape_closes_the_bar_while_its_popup_keeps_focus() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    open_with_query(&ctx, &mut state, &engine);
    escape_frame(&ctx, &mut state, &engine, vec![escape_event()]);
    assert!(!state.popups.is_open(SEARCH_BAR_POPUP_ID));
    assert!(state.search.query.is_empty());
}

#[test]
fn escape_after_a_terminal_click_keeps_the_bar_open() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    open_with_query(&ctx, &mut state, &engine);
    let outside = egui::pos2(900.0, 600.0);
    let button = |pressed| egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    escape_frame(
        &ctx,
        &mut state,
        &engine,
        vec![egui::Event::PointerMoved(outside), button(true)],
    );
    assert!(!state.popups.is_focused(SEARCH_BAR_POPUP_ID));
    // The draw callback still reads the pre-draw focus snapshot on the click frame and takes the
    // input's egui focus back; the following frame must hand it over.
    assert!(
        ctx.memory(|m| m.focused().is_some()),
        "the click frame leaves the input focused"
    );
    escape_frame(&ctx, &mut state, &engine, vec![button(false)]);
    assert!(
        ctx.memory(|m| m.focused().is_none()),
        "the next frame surrenders the input's focus"
    );
    // Known limit: an Escape on the frame right after the click frame would still close the bar,
    // because no frame has surrendered the focus yet. Key bubbling from the focus position removes it.
    escape_frame(&ctx, &mut state, &engine, vec![escape_event()]);
    assert!(state.popups.is_open(SEARCH_BAR_POPUP_ID));
    assert_eq!(state.search.query, "needle");
}

#[test]
fn escape_after_the_find_shortcut_keeps_the_bar_open() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    open_with_query(&ctx, &mut state, &engine);
    let ctrl = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::F,
                physical_key: Some(egui::Key::F),
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
            modifiers: ctrl,
            ..Default::default()
        },
        |ctx| {
            crate::adapters::ui::draw_popups(
                ctx,
                &mut state,
                &engine,
                &[],
                PhysicalRect {
                    x: PhysicalPx(0.0),
                    y: PhysicalPx(0.0),
                    width: PhysicalPx(1280.0),
                    height: PhysicalPx(720.0),
                },
                1.0,
            );
        },
    ));
    assert!(!state.popups.is_focused(SEARCH_BAR_POPUP_ID));
    escape_frame(&ctx, &mut state, &engine, vec![]);
    escape_frame(&ctx, &mut state, &engine, vec![escape_event()]);
    assert!(state.popups.is_open(SEARCH_BAR_POPUP_ID));
    assert_eq!(state.search.query, "needle");
}
