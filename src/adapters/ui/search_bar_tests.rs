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
    // The window's Escape gate runs before the frame that carries the same Escape.
    assert_eq!(
        state
            .popups
            .focused_dismissal_target()
            .map(|(id, _, closes)| (id, closes)),
        Some((SEARCH_BAR_POPUP_ID, false)),
        "the search bar is the only focused popup"
    );
    let mut events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert!(
        state.window_escape(&mut events, Some(0), false),
        "the window handles it"
    );
    assert!(
        events.is_empty(),
        "the window takes the Escape out of egui input"
    );
    assert!(!state.popups.is_focused(SEARCH_BAR_POPUP_ID));
    assert!(
        state.popups.is_open(SEARCH_BAR_POPUP_ID),
        "the winit stage does not close"
    );
    // The popup layer hands that Escape back to the bound bar while its input still has focus.
    escape_frame(&ctx, &mut state, &engine, events);
    assert!(
        !state.popups.is_open(SEARCH_BAR_POPUP_ID),
        "Escape handed to the focused input closes the bar"
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
    // 창이 그 Esc 를 판정해 키 주인 바에 넘긴다. 바 본문은 Esc 를 읽지 않고 중앙 단계가 선언대로 닫는다.
    let mut events = vec![escape_event()];
    assert!(state.window_escape(&mut events, Some(0), false));
    escape_frame(&ctx, &mut state, &engine, events);
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
    // The press moves the focus node before the popup layer decides the key owner, so the click
    // frame already hands the input's egui focus over.
    assert!(
        ctx.memory(|m| m.focused().is_none()),
        "the click frame surrenders the input's focus"
    );
    escape_frame(&ctx, &mut state, &engine, vec![button(false)]);
    assert!(ctx.memory(|m| m.focused().is_none()));
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

/// 다른 surface에서 find를 누르면 하나뿐인 검색창이 그 surface로 옮겨 간다. 옛 검색은 비우고,
/// 닫기 훅은 돌지 않아 옮긴 뒤에 채운 검색이 남는다.
#[test]
fn find_on_another_surface_moves_the_one_bar_without_its_close_hook() {
    let (mut state, mut session) = test_state();
    open_or_focus_for(&mut state, Some(1), "find");
    crate::intent::popup::apply_pending(&mut state, &mut session);
    state.search.query = "needle".into();

    open_or_focus_for(&mut state, Some(2), "find");
    assert!(state.search.query.is_empty());
    assert_eq!(state.search.surface_id, 2);
    state.search.query = "typed".into();
    crate::intent::popup::apply_pending(&mut state, &mut session);

    let bars: Vec<(PopupScope, Option<u32>)> = state
        .popups
        .instances_of(SEARCH_BAR_POPUP_ID)
        .map(|p| (p.scope.clone(), search_target(&p.payload)))
        .collect();
    assert_eq!(bars, vec![(PopupScope::Surface(2), Some(2))]);
    assert!(state.popups.take_closed_ids().is_empty());
    assert_eq!(state.search.query, "typed");
}

fn search_target(payload: &crate::adapters::ui::popup::PopupPayload) -> Option<u32> {
    match payload {
        crate::adapters::ui::popup::PopupPayload::Search { surface_id } => Some(*surface_id),
        _ => None,
    }
}

/// Given 검색창 인스턴스의 대상이 포커스된 터미널과 다른 surface일 때, When 검색어를 입력하면,
/// Then 검색은 포커스된 터미널이 아니라 인스턴스의 대상 surface에 걸린다.
#[test]
fn typing_searches_the_instance_target_not_the_focused_terminal() {
    const TARGET: u32 = 4_242;
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    assert_ne!(state.focused_surface_id(&engine), Some(TARGET));
    let ctx = egui::Context::default();
    state.popups.open_request(
        &crate::intent::OpenRequest::new(
            SEARCH_BAR_POPUP_ID,
            crate::model::popup_kind::PopupScope::Window,
        )
        .at(egui::pos2(100.0, 100.0))
        .focused()
        .with_payload(crate::adapters::ui::popup::PopupPayload::Search { surface_id: TARGET }),
        true,
    );
    for _ in 0..20 {
        escape_frame(&ctx, &mut state, &engine, vec![]);
    }
    escape_frame(
        &ctx,
        &mut state,
        &engine,
        vec![egui::Event::Text("needle".into())],
    );
    assert_eq!(state.search.query, "needle");
    assert_eq!(state.search.surface_id, TARGET);
}

/// 검색어를 넣고 결과 셋 가운데 둘째를 현재로 둔 검색창.
fn bar_on_the_second_of_three(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    open_with_query(ctx, state, engine);
    let at = |row| tasty_terminal::search::SearchMatch {
        row,
        col_start: 0,
        col_end: 6,
    };
    state.search.matches = vec![at(0), at(1), at(2)];
    state.search.current_index = 1;
}

/// 패스 수식키 `modifiers` 로 `events` 한 프레임을 그리고 현재 결과 번호를 돌려준다.
fn index_after(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> usize {
    drop(ctx.run(
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
    state.search.current_index
}

/// 결과 셋 가운데 둘째에 선 검색창에 패스 수식키 `modifiers` 로 `events` 를 넣은 뒤 현재 결과 번호.
fn search_step(
    events: impl FnOnce(&egui::Context) -> Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> usize {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    bar_on_the_second_of_three(&ctx, &mut state, &engine);
    let events = events(&ctx);
    index_after(&ctx, &mut state, &engine, events, modifiers)
}

use crate::adapters::ui::popup::window_input_test_support::{Held, held_press};

/// T-9a-s1: Given 결과 셋 가운데 둘째, When Enter 를 누르면, Then 다음 결과로 간다.
#[test]
fn enter_goes_to_the_next_match() {
    let at = search_step(
        |ctx| vec![held_press(ctx, egui::Key::Enter, None)],
        egui::Modifiers::NONE,
    );
    assert_eq!(at, 2);
}

/// T-9a-s2: Given 결과 셋 가운데 둘째, When Shift+Enter 를 누르면, Then 이전 결과로 간다.
#[test]
fn shift_enter_goes_to_the_previous_match() {
    let at = search_step(
        |ctx| vec![held_press(ctx, egui::Key::Enter, Some(Held::Shift))],
        egui::Modifiers::SHIFT,
    );
    assert_eq!(at, 0);
}

/// T-9a-s3: Given 결과 셋 가운데 둘째, When 한 배치에 Shift+Enter 누름과 Shift 뗌이 와서 패스 수식키가 없으면,
/// Then 그 누름의 수식키로 이전 결과로 한 번 간다.
#[test]
fn shift_enter_uses_its_own_modifiers_after_shift_is_released() {
    let at = search_step(
        |ctx| vec![held_press(ctx, egui::Key::Enter, Some(Held::Shift))],
        egui::Modifiers::NONE,
    );
    assert_eq!(at, 0);
}

/// T-9a-s4: Given 결과 셋 가운데 둘째, When 한 배치에 Enter 둘이 오면, Then 다음 결과로 한 번 간다.
#[test]
fn two_enters_move_once() {
    let at = search_step(
        |ctx| {
            vec![
                held_press(ctx, egui::Key::Enter, None),
                held_press(ctx, egui::Key::Enter, None),
            ]
        },
        egui::Modifiers::NONE,
    );
    assert_eq!(at, 2);
}

/// T-9a-s5: Given 결과 셋 가운데 둘째, When 한 패스에 Enter 와 Shift+Enter 가 오면, Then 이전 결과로 한 번 간다.
#[test]
fn enter_and_shift_enter_in_one_pass_move_back_once() {
    let at = search_step(
        |ctx| {
            vec![
                held_press(ctx, egui::Key::Enter, None),
                held_press(ctx, egui::Key::Enter, Some(Held::Shift)),
            ]
        },
        egui::Modifiers::SHIFT,
    );
    assert_eq!(at, 0);
}

/// T-9a: Given 결과 셋 가운데 둘째, When 수식키 없는 ↑·↓ 를 누르면, Then 이전·다음 결과로 간다.
#[test]
fn bare_arrows_move_between_matches() {
    let up = search_step(
        |ctx| vec![held_press(ctx, egui::Key::ArrowUp, None)],
        egui::Modifiers::NONE,
    );
    let down = search_step(
        |ctx| vec![held_press(ctx, egui::Key::ArrowDown, None)],
        egui::Modifiers::NONE,
    );
    assert_eq!((up, down), (0, 2));
}

/// T-9a: Given 결과 셋 가운데 둘째, When Ctrl·Alt·Cmd(비-macOS Super)+Enter 나 Shift·Ctrl·Alt·Cmd+↑·↓ 를 누르면,
/// Then 결과가 그대로다.
#[test]
fn modified_enter_and_arrows_are_ignored_by_the_search_bar() {
    for held in Held::ALL {
        for key in [egui::Key::Enter, egui::Key::ArrowUp, egui::Key::ArrowDown] {
            if key == egui::Key::Enter && held == Held::Shift {
                continue;
            }
            let at = search_step(
                |ctx| vec![held_press(ctx, key, Some(held))],
                egui::Modifiers::NONE,
            );
            assert_eq!(at, 1, "{held:?} {key:?}");
        }
    }
}

/// `event` 한 프레임을 팝업 층으로 그리고, 그린 뒤 그 누름이 입력에 남았는지 돌려준다.
fn find_frame(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    event: egui::Event,
) -> bool {
    let egui::Event::Key { key, .. } = event else {
        panic!("a key press");
    };
    let mut left = false;
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events: vec![event],
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
            left = ctx.input(|i| {
                i.events.iter().any(
                    |e| matches!(e, egui::Event::Key { key: k, pressed: true, .. } if *k == key),
                )
            });
        },
    ));
    left
}

fn ctrl_f() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::F,
        physical_key: Some(egui::Key::F),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    }
}

/// T-9b-6: Given 입력칸에 포커스가 있는 검색창, When find(Ctrl+F)를 누르면, Then 그 누름을 쓰고 포커스를 터미널로 돌린다.
#[test]
fn find_returns_focus_to_the_terminal() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    open_with_query(&ctx, &mut state, &engine);
    assert!(state.popups.is_focused(SEARCH_BAR_POPUP_ID));
    assert!(
        !find_frame(&ctx, &mut state, &engine, ctrl_f()),
        "find 를 썼다"
    );
    assert!(!state.popups.is_focused(SEARCH_BAR_POPUP_ID));
}

/// T-9b-7: Given 막 연 검색창(입력칸이 아직 포커스를 요청하기 전), When 첫 프레임에 find 를 누르면, Then 검색창은 그
/// 누름을 쓰지 않는다.
#[test]
fn find_before_the_field_has_focus_is_not_taken() {
    let (mut state, mut session) = test_state();
    let session = session.borrow_mut();
    let engine = session.read();
    let ctx = egui::Context::default();
    state.popups.open_at_focused(
        SEARCH_BAR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    assert!(
        find_frame(&ctx, &mut state, &engine, ctrl_f()),
        "누름이 남는다"
    );
    assert!(state.popups.is_open(SEARCH_BAR_POPUP_ID));
}

/// T-9b-8: Given find 바인딩 [F2, Ctrl+F, Alt+F], When 검색창이 없을 때·입력칸에 포커스가 있을 때·다른 surface 를
/// 대상으로 열렸을 때 각 바인딩을 누르면, Then 검색창이 없으면 아무도 쓰지 않고, 있으면 검색창이 그 누름을 쓰고
/// 포커스를 터미널로 돌린다.
#[test]
fn find_bindings_keep_their_meaning() {
    const OTHER: u32 = 4_242;
    let press = |key, modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    // 저장 토큰 alt 는 macOS 에서 Command(egui mac_cmd)이고 다른 OS 에서 Alt 다(tasty_key_match::token_axes_egui).
    // egui Alt 는 macOS 에서 Option 이라 "alt+f" 에 맞지 않는다.
    let alt_token = if cfg!(target_os = "macos") {
        egui::Modifiers {
            mac_cmd: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    } else {
        egui::Modifiers::ALT
    };
    let combos = [
        (egui::Key::F2, egui::Modifiers::NONE),
        (egui::Key::F, egui::Modifiers::CTRL),
        (egui::Key::F, alt_token),
    ];
    for (key, modifiers) in combos {
        for case in ["none", "focused", "other"] {
            let (mut state, mut session) = test_state();
            let session = session.borrow_mut();
            session.runtime.settings.keybindings.find =
                vec!["f2".into(), "ctrl+f".into(), "alt+f".into()];
            let engine = session.read();
            let ctx = egui::Context::default();
            match case {
                "focused" => open_with_query(&ctx, &mut state, &engine),
                "other" => {
                    state.popups.open_request(
                        &crate::intent::OpenRequest::new(
                            SEARCH_BAR_POPUP_ID,
                            crate::model::popup_kind::PopupScope::Window,
                        )
                        .at(egui::pos2(100.0, 100.0))
                        .focused()
                        .with_payload(
                            crate::adapters::ui::popup::PopupPayload::Search { surface_id: OTHER },
                        ),
                        true,
                    );
                    for _ in 0..20 {
                        escape_frame(&ctx, &mut state, &engine, vec![]);
                    }
                }
                _ => {}
            }
            let left = find_frame(&ctx, &mut state, &engine, press(key, modifiers));
            let bar = case != "none";
            assert_eq!(left, !bar, "{case} {key:?} {modifiers:?}");
            assert!(
                !state.popups.is_focused(SEARCH_BAR_POPUP_ID),
                "{case} {key:?} {modifiers:?}"
            );
        }
    }
}

/// 검색 띠 키맵 수신 판정은 본문 읽기(`consume_binding_egui`)가 지우는 누름과 같다. 물리 Alt+F 바인딩은 그
/// 누름의 저장 토큰으로 둔다(`tasty_key_match::token_axes`: macOS 는 `option`, 다른 OS 는 `alt`). 그 바인딩이
/// 이 누름에 실제로 맞는지 시험이 먼저 확인한다.
#[test]
fn the_find_keymap_receives_what_the_body_consumes() {
    let alt_f = if cfg!(target_os = "macos") {
        "option+f"
    } else {
        "alt+f"
    };
    assert!(
        tasty_key_match::binding_matches_egui(
            &[alt_f.to_string()],
            egui::Key::F,
            egui::Modifiers::ALT,
            false,
        ),
        "`{alt_f}` 가 물리 Alt+F 에 맞는다"
    );
    let find = vec!["ctrl+f".to_string(), alt_f.to_string(), "f3".to_string()];
    let mods = [
        egui::Modifiers::NONE,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT,
        egui::Modifiers::CTRL,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    ];
    let mut hits = 0;
    for key in egui::Key::ALL {
        for m in mods {
            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                modifiers: m,
                events: vec![egui::Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: m,
                }],
                ..Default::default()
            };
            ctx.begin_pass(raw);
            let body = ctx.input_mut(|i| tasty_key_match::consume_binding_egui(&find, i, false));
            let _output = ctx.end_pass();
            assert_eq!(
                find_keymap_receives(&find, *key, m, false),
                body,
                "{m:?}+{key:?}"
            );
            hits += usize::from(body);
        }
    }
    assert_eq!(hits, 3);
}
