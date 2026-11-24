//! 복원 안내가 창 범위 팝업 인스턴스로 열리고, 포커스를 받지 않으며, 안내 열에 쌓이고, 바깥 클릭에 남는 규칙.

use super::*;
use crate::state::tests::test_state;

fn entry(id: u64, path: &str) -> RestoreNoticeEntry {
    RestoreNoticeEntry {
        id,
        title: "title".into(),
        cause: "cause".into(),
        label: "label".into(),
        path: path.into(),
        body: "body".into(),
    }
}

fn term_rect() -> PhysicalRect {
    PhysicalRect {
        x: crate::model::PhysicalPx(0.0),
        y: crate::model::PhysicalPx(0.0),
        width: crate::model::PhysicalPx(1600.0),
        height: crate::model::PhysicalPx(900.0),
    }
}

enum Input {
    Idle,
    Press(egui::Pos2),
    Release(egui::Pos2),
    Key(egui::Key),
}

/// 팝업 층을 실제 순서(열 자리 → 그리기 → 닫기 훅)로 한 프레임 돈다.
fn frame(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    input: Input,
) {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        )),
        ..Default::default()
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    match input {
        Input::Idle => {}
        Input::Press(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, true));
        }
        Input::Release(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, false));
        }
        Input::Key(key) => raw.events.push(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }),
    }
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, engine, &[], term_rect(), 1.0);
    }));
}

fn notice_rects(state: &MainViewState) -> Vec<egui::Rect> {
    state
        .popups
        .instances_of(RESTORE_NOTICE_POPUP_ID)
        .map(|p| p.popup_rect())
        .collect()
}

/// Given 안내 둘, Then 둘 다 시스템 사건으로 연 passive 창 범위 인스턴스이고 포커스·Esc 주인이 되지 않으며
/// plugin 팝업과 견줄 순번도 없다. 같은 안내는 다시 쌓지 않는다. 그린 뒤 안내 열에 연 순서대로 위에서부터
/// 간격을 두고 쌓인다.
#[test]
fn restore_notices_are_window_multi_passive_noncovering_instances() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    assert!(push(&mut state, entry(1, "/a")));
    assert!(
        !push(&mut state, entry(2, "/a")),
        "the same notice stacks once"
    );
    assert!(push(&mut state, entry(3, "/b")));
    assert!(has_undrawn(&state));
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    assert!(!has_undrawn(&state));

    assert_eq!(count(&state), 2);
    for p in state.popups.instances_of(RESTORE_NOTICE_POPUP_ID) {
        assert_eq!(p.scope, PopupScope::Window);
        assert!(!state.popups.is_instance_focused(p.instance));
    }
    assert!(state.popups.is_passive(RESTORE_NOTICE_POPUP_ID));
    assert!(!state.popups.covers_scope(RESTORE_NOTICE_POPUP_ID));
    assert!(state.popups.topmost_visible_open(None).is_none());
    assert_eq!(state.popups.max_open_z_seq(), None);

    let rects = notice_rects(&state);
    let gap = crate::theme::theme().notice_column_gap().value();
    assert!(rects[0].height() > 1.0, "the measured height is used");
    assert_eq!(rects[1].top(), rects[0].bottom() + gap);
    assert_eq!(rects[0].right(), rects[1].right());
    assert_eq!(state.restore_notices.column, Some(rects[0].union(rects[1])));
}

/// Given 그려진 안내 둘, Then native WebView는 숨기거나 키보드 포커스를 회수하지 않고 카드 자리에만 구멍을 낸다.
#[test]
fn restore_notices_leave_the_webview_alone() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    push(&mut state, entry(1, "/a"));
    push(&mut state, entry(3, "/b"));
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    let rects = notice_rects(&state);
    assert_eq!(rects.len(), 2);
    assert!(
        !state.has_egui_overlay_open(),
        "a notice must not take the WebView keyboard focus back"
    );
    assert!(
        state.popups.visible_passive_rects().is_empty(),
        "a notice must not hide the WebView under it"
    );
    assert_eq!(state.popups.quiet_popup_rects(), rects);
}

/// Given 사용자 경로로 연 안내(포커스를 받지 않는 선언), Then 출처와 관계없이 WebView 처리는 같다.
#[test]
fn a_quiet_popup_opened_by_the_user_still_leaves_the_webview_alone() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(RESTORE_NOTICE_POPUP_ID, PopupScope::Window)
                .with_payload(PopupPayload::RestoreNotice(Box::new(NoticePayload {
                    entry: entry(1, "/a"),
                    height: None,
                    drawn: false,
                    focused_inside: false,
                }))),
            true,
        )
        .expect("opens");
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    assert!(!state.popups.is_passive(RESTORE_NOTICE_POPUP_ID));
    assert!(!state.popups.has_visible_open());
    assert!(!state.has_egui_overlay_open());
    assert!(state.popups.visible_passive_rects().is_empty());
    assert_eq!(state.popups.quiet_popup_rects(), notice_rects(&state));
}

/// Given 그려진 안내, Then 그 카드 위의 포인터는 포인터 대상 찾기에서 팝업 노드다(아래 pane을 활성화하지
/// 않는다). When 안내를 눌러도 Then 팝업 포커스가 생기지 않는다. When 안내 바깥을 눌러도 Then 안내가 남는다.
#[test]
fn a_click_on_a_notice_does_not_activate_the_pane_below() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    push(&mut state, entry(1, "/a"));
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    let card = notice_rects(&state)[0];
    let inside = card.center();
    assert!(state.popups.quiet_popup_covers(inside));
    // 창의 포인터 처리는 직전 프레임에 그린 팝업 노드로 대상을 찾는다. 카드 위는 팝업 것이고 카드 왼쪽 바깥은 아니다.
    assert!(state.popup_owns_point(inside));
    assert!(!state.popup_owns_point(egui::pos2(card.min.x - 1.0, inside.y)));
    frame(&ctx, &mut state, &read, Input::Press(inside));
    frame(&ctx, &mut state, &read, Input::Release(inside));
    assert!(!state.popups.has_focused());
    assert_eq!(count(&state), 1);
}

#[test]
fn outside_click_keeps_the_notice() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    push(&mut state, entry(1, "/a"));
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    let outside = egui::pos2(100.0, 600.0);
    assert!(!notice_rects(&state)[0].contains(outside));
    frame(&ctx, &mut state, &read, Input::Press(outside));
    frame(&ctx, &mut state, &read, Input::Release(outside));
    frame(&ctx, &mut state, &read, Input::Key(egui::Key::Escape));
    assert_eq!(count(&state), 1);
    assert!(state.restore_notices.take_acknowledged().is_empty());
}

/// Given 나타나기만 한 안내, Then 키 게이트가 열리지 않아 키는 포커스된 surface로 간다. When 안내 안쪽을 누르면
/// Then 안쪽 컨트롤이 키를 받고, When Esc를 누르면 Then 그 안내만 닫히고 id가 확인 응답으로 남는다.
#[test]
fn typing_with_a_notice_shown_goes_to_the_focused_surface() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    push(&mut state, entry(1, "/a"));
    push(&mut state, entry(2, "/b"));
    for _ in 0..3 {
        frame(&ctx, &mut state, &read, Input::Idle);
    }
    assert!(!has_keyboard_focus(&state.popups));
    assert!(!state.keyboard_overlay_open_besides_html_address());

    let first = notice_rects(&state)[0];
    let body = egui::pos2(first.center().x, first.bottom() - 4.0);
    frame(&ctx, &mut state, &read, Input::Press(body));
    frame(&ctx, &mut state, &read, Input::Release(body));
    assert!(has_keyboard_focus(&state.popups));
    assert!(state.keyboard_overlay_open_besides_html_address());

    frame(&ctx, &mut state, &read, Input::Key(egui::Key::Escape));
    frame(&ctx, &mut state, &read, Input::Idle);
    assert_eq!(count(&state), 1);
    assert_eq!(state.restore_notices.take_acknowledged(), vec![1]);
    assert!(!has_keyboard_focus(&state.popups));
}

/// 한 프레임을 `terminal` 범위로 돈다. 열이 검색 막대와 가로로 겹치는 좁은 창을 만든다.
fn frame_in(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    terminal: PhysicalRect,
) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(terminal.width.value(), terminal.height.value()),
        )),
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, engine, &[], terminal, 1.0);
    }));
}

/// Given 좁은 창, When 검색 막대를 처음 띄운 프레임, Then attach 대기 칩이 잇는 열의 위쪽은 그 프레임에 그린
/// 막대 아래다(막대의 직전 자리로 잰 열이 아니다).
#[test]
fn the_chip_column_starts_below_a_search_bar_opened_this_frame() {
    let (mut state, mut session) = test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    let narrow = PhysicalRect {
        width: crate::model::PhysicalPx(520.0),
        height: crate::model::PhysicalPx(400.0),
        ..term_rect()
    };
    frame_in(&ctx, &mut state, &read, narrow);
    state.popups.open_request(
        &crate::intent::OpenRequest::new(
            crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID,
            PopupScope::Window,
        )
        .at_top(),
        true,
    );
    let before = notice_column(
        narrow,
        state.tab_bar_height,
        1.0,
        &state.popups.reserved_top_bands(),
    );
    frame_in(&ctx, &mut state, &read, narrow);
    let bar = state.popups.reserved_top_bands();
    assert_eq!(bar.len(), 1, "the search bar is drawn at the top");
    let bar = bar[0];
    let column = crate::adapters::ui::to_egui_rect(narrow, 1.0);
    assert!(
        bar.right() > before.rect.left() && bar.left() < column.right(),
        "the bar {bar:?} spans the column {:?}",
        before.rect
    );
    assert!(
        before.rect.top() < bar.bottom(),
        "the column measured before drawing ({:?}) ends above the drawn bar {bar:?}",
        before.rect
    );
    let top = state
        .restore_notices
        .column_top
        .expect("measured after drawing");
    assert!(
        top.value() >= bar.bottom(),
        "chip column top {top:?} is below the drawn bar {bar:?}"
    );
}
