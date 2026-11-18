//! 단축키 도움말이 창 범위 팝업 인스턴스로 열리고, 포커스를 받지 않으며, 옮긴 자리를 저장하는 규칙.

use super::*;
use crate::state::tests::test_state;

const ID: PopupId = MODIFIER_HINT_POPUP_ID;

/// Ctrl을 표시 지연보다 오래 누른 상태로 둔다.
fn hold_ctrl_long(state: &mut MainViewState) {
    state.modifier_hint.update_hold(true, false, false, false);
    state
        .modifier_hint
        .debug_backdate(std::time::Duration::from_millis(5000));
}

enum Pointer {
    Press(egui::Pos2),
    MoveTo(egui::Pos2),
    Release(egui::Pos2),
    /// 보조 버튼을 그 자리에서 누르거나(true) 뗀다(false).
    Secondary(egui::Pos2, bool),
    Idle,
}

/// 팝업 층의 순서(sync → 그리기 → 놓은 자리 저장)를 한 프레임 돈다. 도움말은 실제 콘텐츠를 그리고
/// 다른 팝업은 콘텐츠 없이 그린다.
fn frame(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pointer: Pointer,
) -> Vec<egui::LayerId> {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    match pointer {
        Pointer::Press(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, true));
        }
        Pointer::MoveTo(pos) => raw.events.push(egui::Event::PointerMoved(pos)),
        Pointer::Release(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, false));
        }
        Pointer::Secondary(pos, pressed) => raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }),
        Pointer::Idle => {}
    }
    let mut layers = Vec::new();
    drop(ctx.run(raw, |ctx| {
        sync(ctx, state, engine);
        let mut popups =
            std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
        let drawn = popups.draw(
            ctx,
            &mut |cx, ui| {
                if cx.id == ID {
                    draw_hint_popup(ui, state, engine, cx);
                }
            },
            None,
            &[],
        );
        state.popups = popups;
        persist_released(state, engine, &drawn.released);
        layers = drawn.layers;
    }));
    layers
}

fn hint_rect(state: &MainViewState) -> egui::Rect {
    state.popups.visible_rect(ID).expect("the hint is open")
}

/// Given 홀드하지 않은 상태, When 표시 지연보다 오래 누르면, Then 도움말이 창 범위 인스턴스로 저장 자리
/// (저장값이 없으면 화면 왼쪽 아래)에 열린다. 포커스와 Esc 주인이 되지 않고 plugin 팝업과 견줄 순번도 없다.
/// 겹친 native WebView를 숨기거나 키보드 포커스를 회수하지 않는다.
/// When 키를 모두 떼면 Then 닫힌다.
#[test]
fn the_hint_is_a_window_instance_that_never_takes_focus() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Pointer::Idle);
    assert!(!state.popups.is_open(ID));

    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);

    assert_eq!(state.popups.open_scope(ID), Some(&PopupScope::Window));
    assert_eq!(state.popups.instances_of(ID).count(), 1);
    assert!(state.popups.instances_of(ID).all(|p| !p.focused));
    assert!(state.popups.topmost_visible_open(None).is_none());
    assert_eq!(state.popups.max_open_z_seq(), None);
    let want = {
        let th = crate::theme::theme();
        default_rect(ctx.screen_rect(), &th)
    };
    assert_eq!(hint_rect(&state), want);
    assert!(!state.has_egui_overlay_open());
    assert!(
        state.popups.visible_passive_rects().is_empty(),
        "the hint must not hide the WebView under it"
    );

    state.modifier_hint.update_hold(false, false, false, false);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    assert!(!state.popups.is_open(ID));
}

/// Given 열린 도움말, When 위쪽 띠(키캡·"held" 문구 위 포함)를 눌러 끌어 옮긴 뒤 놓으면, Then 놓은 프레임에
/// 옮긴 자리와 크기를 설정에 저장하는 사용자 의도가 하나 나간다. 끄는 동안에는 저장하지 않는다.
/// 띠 안의 글자가 누름을 가져가면 옮겨지지 않으므로 띠의 여러 지점에서 확인한다.
#[test]
fn dragging_the_hint_persists_geometry_on_release() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Pointer::Idle);
    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    drop(state.take_pending_intents());

    let strip = drag_strip(state.popups.instances_of(ID).next().expect("open"));
    let offsets: Vec<f32> = (0..)
        .map(|i| 4.0 + 6.0 * i as f32)
        .take_while(|x| *x < strip.width() - 4.0)
        .collect();
    assert!(offsets.len() > 10, "the strip is sampled densely");
    for dx in offsets {
        let start = hint_rect(&state);
        let grab = egui::pos2(start.left() + dx, start.top() + strip.height() / 2.0);
        let to = grab + egui::vec2(6.0, -2.0);
        frame(&ctx, &mut state, &read, Pointer::Press(grab));
        frame(&ctx, &mut state, &read, Pointer::MoveTo(to));
        assert!(
            state.take_pending_intents().is_empty(),
            "nothing saved mid-drag"
        );
        frame(&ctx, &mut state, &read, Pointer::Release(to));
        frame(&ctx, &mut state, &read, Pointer::Idle);

        let moved = start.translate(egui::vec2(6.0, -2.0));
        assert_eq!(hint_rect(&state), moved, "grabbed {dx}px into the strip");
        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1, "grabbed {dx}px into the strip");
        let crate::intent::Intent::Domain(crate::app::command::DomainIntent::UpdateSettings(saved)) =
            &intents[0].body
        else {
            panic!("expected UpdateSettings, got {:?}", intents[0].body);
        };
        assert_eq!(
            saved.modifier_hint.pos,
            Some((LogicalPx(moved.left()), LogicalPx(moved.top())))
        );
        assert_eq!(
            saved.modifier_hint.size,
            Some((LogicalPx(moved.width()), LogicalPx(moved.height())))
        );
    }
}

/// Given 위쪽 띠를 왼쪽 버튼으로 잡아 끄는 중, When 보조 버튼을 눌렀다 떼면, Then 끌기가 끝나지 않고 저장도
/// 하지 않는다. When 이어서 옮긴 뒤 왼쪽 버튼을 놓으면 Then 옮긴 전체 거리로 한 번 저장한다.
#[test]
fn a_secondary_click_does_not_end_the_hint_drag() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Pointer::Idle);
    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    drop(state.take_pending_intents());

    let strip = drag_strip(state.popups.instances_of(ID).next().expect("open"));
    let start = hint_rect(&state);
    let grab = egui::pos2(start.left() + 4.0, start.top() + strip.height() / 2.0);
    let mid = grab + egui::vec2(6.0, -2.0);
    let end = mid + egui::vec2(10.0, 4.0);
    frame(&ctx, &mut state, &read, Pointer::Press(grab));
    frame(&ctx, &mut state, &read, Pointer::MoveTo(mid));
    frame(&ctx, &mut state, &read, Pointer::Secondary(mid, true));
    frame(&ctx, &mut state, &read, Pointer::Secondary(mid, false));
    assert!(
        state.take_pending_intents().is_empty(),
        "a secondary release must not end the drag"
    );
    frame(&ctx, &mut state, &read, Pointer::MoveTo(end));
    frame(&ctx, &mut state, &read, Pointer::Release(end));
    frame(&ctx, &mut state, &read, Pointer::Idle);

    assert_eq!(hint_rect(&state), start.translate(end - grab));
    assert_eq!(state.take_pending_intents().len(), 1);
}

/// Given 먼저 연 알림 팝업과 그 뒤에 열린 도움말이 겹칠 때, Then 겹친 곳의 맨 위는 알림 팝업이다.
/// When 도움말의 겹치지 않은 곳을 눌러도, Then 알림 팝업이 위에 남고 포커스도 그대로다.
#[test]
fn the_hint_stays_below_other_popups() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Pointer::Idle);
    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    let hint = hint_rect(&state);
    state.modifier_hint.update_hold(false, false, false, false);
    frame(&ctx, &mut state, &read, Pointer::Idle);

    state.popups.open_request(
        &crate::intent::OpenRequest::new("notifications", PopupScope::Window)
            .at(egui::pos2(hint.center().x, hint.top()))
            .focused(),
        true,
    );
    frame(&ctx, &mut state, &read, Pointer::Idle);
    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    let layers = frame(&ctx, &mut state, &read, Pointer::Idle);
    assert_eq!(layers.len(), 1, "only the notifications layer takes focus");
    let notifications = state.popups.visible_rect("notifications").expect("open");
    let hint = hint_rect(&state);
    let overlap = hint.intersect(notifications);
    assert!(overlap.is_positive(), "the two popups overlap");
    assert_eq!(ctx.layer_id_at(overlap.center()), Some(layers[0]));

    let bare = egui::pos2(hint.left() + 4.0, hint.center().y);
    assert!(!notifications.contains(bare));
    frame(&ctx, &mut state, &read, Pointer::Press(bare));
    frame(&ctx, &mut state, &read, Pointer::Release(bare));
    let layers = frame(&ctx, &mut state, &read, Pointer::Idle);
    assert_eq!(ctx.layer_id_at(overlap.center()), Some(layers[0]));
    assert!(state.popups.is_focused("notifications"));
}

/// Given 목록이 넘쳐 세로 스크롤바가 오른쪽 아래 그립과 겹친 도움말, When 그립을 눌러 끌고 놓으면,
/// Then 목록이 스크롤되지 않고 오른쪽·아래 변이 포인터를 따라가며, 놓은 프레임에 새 크기를 저장한다.
#[test]
fn dragging_the_grip_resizes_the_hint_over_its_scrollbar() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, &read, Pointer::Idle);
    hold_ctrl_long(&mut state);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    // 처음 크기의 최소값으로 줄여 목록이 반드시 넘치게 한다.
    let min = {
        let th = crate::theme::theme();
        egui::vec2(
            th.modhint_min_width().value(),
            th.modhint_min_height().value(),
        )
    };
    let iid = state.popups.instances_of(ID).next().expect("open").instance;
    if let Some(p) = state.popups.instance_mut(iid) {
        p.size = min;
    }
    frame(&ctx, &mut state, &read, Pointer::Idle);
    frame(&ctx, &mut state, &read, Pointer::Idle);
    drop(state.take_pending_intents());

    let start = hint_rect(&state);
    let grip = crate::theme::theme().modhint_grip_size().value();
    let grab = start.max - egui::Vec2::splat(grip / 2.0);
    let to = grab + egui::vec2(30.0, 20.0);
    frame(&ctx, &mut state, &read, Pointer::Press(grab));
    frame(
        &ctx,
        &mut state,
        &read,
        Pointer::MoveTo(grab + egui::vec2(15.0, 10.0)),
    );
    frame(&ctx, &mut state, &read, Pointer::MoveTo(to));
    frame(&ctx, &mut state, &read, Pointer::Release(to));
    frame(&ctx, &mut state, &read, Pointer::Idle);

    // 크기 조절은 잡은 변을 포인터 위치로 옮긴다.
    let resized = egui::Rect::from_min_max(start.min, to);
    assert_eq!(hint_rect(&state), resized);
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    let crate::intent::Intent::Domain(crate::app::command::DomainIntent::UpdateSettings(saved)) =
        &intents[0].body
    else {
        panic!("expected UpdateSettings, got {:?}", intents[0].body);
    };
    assert_eq!(
        saved.modifier_hint.size,
        Some((LogicalPx(resized.width()), LogicalPx(resized.height())))
    );
}
