//! 그려진 화면에 노드가 없는 몫에 남긴 버튼 뗌(`MainViewState::drop_input_without_its_node`)이 egui 의 눌림만 풀고
//! 클릭 이력·드래그 끝·드롭을 만들지 않는지 잰다. egui 는 클릭이 된 뗌마다 전역 클릭 시각을 갱신하므로, 다른 화면의
//! 뗌이 클릭으로 세이면 일반 화면의 다음 한 번 클릭이 더블클릭(탐색기 파일 열기)이 된다.

use super::*;

/// 무대를 열어 그린 뒤 무대 가운데(빈 곳) 누름을 무대 패스로 그리고, [종료 Esc, 뗌]으로 무대를 닫는다. 남은 몫은
/// 일반 화면으로 그린다. 뗌 몫을 그린 패스의 egui 시각을 돌려준다.
fn press_on_the_stage_then_exit(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
) -> f64 {
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(s, input, l, r));
    let point = screen().center();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    assert!(stage_or_window_pass(s, input, l, r));
    assert!(
        s.ctx.input(|i| i.pointer.any_down()),
        "무대가 누름을 받았다"
    );
    {
        let engine = s.session.borrow_mut();
        let escape = key_press(egui::Key::Escape);
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, escape)
                .is_some()
        );
    }
    // 창의 무대 종료 키 처리(`try_consume_fullscreen_stage_key`)가 무대를 닫는다.
    assert!(s.state.close_fullscreen_stage());
    input.queue_mut().push(release_at(point));
    while !input.is_empty() {
        assert!(!stage_or_window_pass(s, input, l, r));
    }
    assert!(!s.ctx.input(|i| i.pointer.any_down()), "뗌이 눌림을 풀었다");
    s.ctx.input(|i| i.time)
}

/// L 탐색기의 `pos` 를 한 번 누르고 뗀다(한 묶음, 일반 화면).
fn click(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
    pos: egui::Pos2,
) {
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), pos);
    }
    input.queue_mut().push(release_at(pos));
    while !input.is_empty() {
        assert!(!stage_or_window_pass(s, input, l, r));
    }
}

/// Given 무대가 창을 덮고 L 탐색기가 그 뒤에 있을 때, When 무대 빈 곳 누름을 무대 패스로 그리고 [종료 Esc, 뗌]으로
/// 무대를 닫은 뒤 더블클릭 간격 안에 L 의 banana 줄을 한 번 클릭하면, Then 그 클릭은 한 번 클릭이라 banana 를 고르기만
/// 하고 열지 않는다. 대조: 같은 줄을 실제로 두 번 클릭하면 연다.
#[test]
fn a_click_right_after_a_release_from_the_closed_stage_is_a_single_click() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    // 앞 누름과 더블클릭 간격(0.3초) 밖으로 시간을 보낸다(빈 패스마다 egui 가 1/60초 진행).
    for _ in 0..45 {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    let released = press_on_the_stage_then_exit(&mut s, &mut input, &mut l, &mut r);
    l.actions.clear();
    click(&mut s, &mut input, &mut l, &mut r, banana);
    let gap = s.ctx.input(|i| i.time) - released;
    let double_click = s.ctx.options(|o| o.input_options.max_double_click_delay);
    assert!(
        gap < double_click,
        "뗌과 클릭이 더블클릭 간격 안이다: {gap}"
    );
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
    assert!(
        !l.actions.iter().any(|a| a.contains("OpenFile")),
        "닫힌 무대 몫의 뗌이 클릭으로 세여 한 번 클릭이 파일을 열었다: {:?}",
        l.actions
    );

    for _ in 0..45 {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    click(&mut s, &mut input, &mut l, &mut r, banana);
    click(&mut s, &mut input, &mut l, &mut r, banana);
    assert!(
        l.actions.iter().any(|a| a.contains("OpenFile")),
        "대조: 실제 두 번 클릭은 연다: {:?}",
        l.actions
    );
}

/// Given 일반 화면에서 누른 채 끌기(`dragged`)와 끌어 놓기 payload 가 있을 때, When 무대가 열리고(debug 열기처럼 키
/// 없이) 무대 패스를 그린 뒤 [종료 Esc, 뗌]으로 무대를 닫으면, Then 뗌 몫을 그리는 일반 화면 패스는 payload 를 보지
/// 못하고(드롭 확정 없음) 그 끌기의 `drag_stopped` 도 받지 않으며, 눌림은 풀린다.
#[test]
fn a_release_from_the_closed_stage_neither_stops_a_drag_nor_drops_its_payload() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    let point = surface_rect_l().center();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    while !input.is_empty() {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    let drag = egui::Id::new("pass_focus_release_drag");
    s.ctx.set_dragged_id(drag);
    egui::DragAndDrop::set_payload(&s.ctx, 7_u32);
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    assert_eq!(
        s.ctx.dragged_id(),
        Some(drag),
        "무대 패스 동안 끌기가 남는다"
    );
    {
        let engine = s.session.borrow_mut();
        let escape = key_press(egui::Key::Escape);
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, escape)
                .is_some()
        );
    }
    assert!(s.state.close_fullscreen_stage());
    input.queue_mut().push(release_at(point));
    // 뗌 몫을 그리는 일반 화면 쪽 코드가 보는 값: payload 와 뗌(드롭 확정 조건), drag_stopped.
    let mut seen = Vec::new();
    while !input.is_empty() {
        assert!(!s.state.pass_draws_stage());
        let events = input.take_pass(&mut s.state, &s.ctx);
        let raw = egui::RawInput {
            screen_rect: Some(screen()),
            events,
            ..Default::default()
        };
        drop(s.ctx.run(raw, |ctx| {
            seen.push((
                egui::DragAndDrop::has_payload_of_type::<u32>(ctx),
                ctx.input(|i| i.pointer.primary_released()),
                ctx.drag_stopped_id(),
            ));
        }));
        s.state.close_input_pass(&s.ctx);
    }
    assert!(
        seen.iter().any(|(_, released, _)| *released),
        "뗌 몫을 그렸다: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|(payload, released, _)| *payload && *released),
        "일반 화면이 payload 와 뗌을 함께 보아 드롭을 확정할 수 있다: {seen:?}"
    );
    assert!(
        !seen.iter().any(|(_, _, stopped)| *stopped == Some(drag)),
        "다른 화면의 뗌이 그 끌기의 drag_stopped 를 냈다: {seen:?}"
    );
    assert!(!s.ctx.input(|i| i.pointer.any_down()));
}
