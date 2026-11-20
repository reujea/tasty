//! 앞 몫의 패스가 그리는 중에 화면을 바꾸면(무대 종료 버튼, 팝업 무대 버튼) 남은 몫과 그 뒤에 붙은 입력이 다른 화면에
//! 가지 않는지, 몫 폐기가 뺀 누름의 뗌이 새 화면에 외톨이로 들지 않는지 잰다. 그리기 중 전환은 패스 콜백에서 무대를
//! 여닫아 모사한다(`stage_or_window_pass_with`). 종료 버튼(`draw_exit_button`)은 비공개라 누르지 않는다.

use super::*;

/// L 탐색기가 포커스 노드이고 모든 버튼이 올라간 장면.
fn scene() -> (Scene, Explorer, Explorer, WindowInput) {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
        input
            .queue_mut()
            .push(release_at(surface_rect_l().center()));
        drain(&mut s, &mut input, &mut bodies);
    }
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "버튼이 모두 올라갔다"
    );
    (s, l, r, input)
}

/// 누름 하나(창의 누름 입구, 끊기와 누름 스냅샷을 거친다).
fn press(s: &mut Scene, input: &mut WindowInput, pos: egui::Pos2) {
    let engine = s.session.borrow_mut();
    input.press(&mut s.state, &engine.read(), pos);
}

/// 키 누름 하나(창의 키 입구). `egui_surface` 는 포커스 surface 가 egui 로 입력을 받는지다.
fn key(s: &mut Scene, input: &mut WindowInput, egui_surface: bool, k: egui::Key) -> Option<bool> {
    let engine = s.session.borrow_mut();
    input.key(&mut s.state, &engine.read(), egui_surface, key_press(k))
}

fn moved(pos: egui::Pos2) -> Event {
    Event::PointerMoved(pos)
}

fn wheel() -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -40.0),
        modifiers: egui::Modifiers::NONE,
    }
}

fn is_press(e: &Event) -> bool {
    matches!(e, Event::PointerButton { pressed: true, .. })
}

fn is_release(e: &Event) -> bool {
    matches!(e, Event::PointerButton { pressed: false, .. })
}

/// 남은 패스를 모두 그린다. 패스마다 무대였는지와 실제로 든 사건이다.
fn drain_passes(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
) -> Vec<(bool, Vec<Event>)> {
    let mut passes = Vec::new();
    while !input.is_empty() {
        passes.push(stage_or_window_pass_with(s, input, l, r, &mut |_| {}));
    }
    passes
}

/// 앞 클릭과 더블클릭 간격(0.3초) 밖으로 시간을 보낸다(빈 패스마다 egui 가 1/60초 진행).
fn wait_out_double_click(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
) {
    for _ in 0..45 {
        stage_or_window_pass(s, input, l, r);
    }
}

/// L 의 빈 곳(항목 줄 아래).
fn empty_l() -> egui::Pos2 {
    let rect = surface_rect_l();
    egui::pos2(rect.left() + 24.0, rect.bottom() - 24.0)
}

/// Given 무대가 창을 덮을 때, When 한 묶음에 [무대 누름·뗌(몫 A), 키 K, 누름 P(뒤 L 의 banana 줄 자리)]가 오고 A 를
/// 그리는 패스가 무대를 닫으면(무대 종료 버튼), Then 무대에서 든 P 는 뒤 일반 패스에 들지 않아 L 이 banana 를 고르지
/// 않고 egui 는 그 버튼을 눌린 채 갖지 않는다.
#[test]
fn a_press_left_after_the_stage_closed_while_drawing_stays_off_the_window() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    let point = screen().center();
    press(&mut s, &mut input, point);
    input.queue_mut().push(release_at(point));
    assert!(key(&mut s, &mut input, false, egui::Key::F9).is_some());
    press(&mut s, &mut input, banana);

    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage, "A 는 무대로 그린다");
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.iter().all(|(stage, _)| !stage));
    assert!(
        !rest.iter().any(|(_, e)| e.iter().any(is_press)),
        "무대에서 든 누름이 일반 패스에 들었다: {rest:?}"
    );
    assert!(!l.selected().contains("banana"), "L: {:?}", l.selected());
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "버튼이 눌린 채 남았다"
    );
}

/// Given 일반 화면, When 한 묶음에 [일반 화면 누름·뗌(몫 A), 키 K, 누름 P]가 오고 A 를 그리는 패스가 무대를
/// 열면(팝업 무대 버튼), Then 전환 앞 일반 화면에서 든 P 는 무대 패스 사건에 없다.
#[test]
fn a_press_left_after_the_stage_opened_while_drawing_stays_off_the_stage() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    let a = empty_l();
    press(&mut s, &mut input, a);
    input.queue_mut().push(release_at(a));
    assert!(key(&mut s, &mut input, true, egui::Key::F9).is_some());
    press(&mut s, &mut input, banana);

    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.open_fullscreen_stage("blank"));
    });
    assert!(!stage, "A 는 일반 화면으로 그린다");
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.iter().all(|(stage, _)| *stage));
    assert!(
        !rest.iter().any(|(_, e)| e.iter().any(is_press)),
        "일반 화면에서 든 누름이 무대 패스에 들었다: {rest:?}"
    );
}

/// 그리기 중 전환 뒤 다음 묶음의 키 K2 와 이동이 어느 화면에 가는지. 앞 몫 A·B·C 의 원시 길이는 3·1·2 다.
/// `close` 면 무대에서 A 를 그리며 닫고(무대→일반), 아니면 일반 화면에서 A 를 그리며 연다(일반→무대).
fn next_batch_after_a_drawing_transition(close: bool) {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    let a = if close {
        assert!(s.state.open_fullscreen_stage("blank"));
        assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
        screen().center()
    } else {
        empty_l()
    };
    press(&mut s, &mut input, a);
    input.queue_mut().push(release_at(a));
    assert!(key(&mut s, &mut input, !close, egui::Key::F9).is_some());
    press(&mut s, &mut input, banana);
    assert_eq!(input.queue_mut().len(), 6, "A·B·C 의 원시 길이 3·1·2");

    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        if close {
            assert!(st.close_fullscreen_stage());
        } else {
            assert!(st.open_fullscreen_stage("blank"));
        }
    });
    assert_eq!(stage, close);
    assert_eq!(s.state.pass_draws_stage(), !close, "입력 직전 화면");
    let m = egui::pos2(surface_rect_l().center().x, surface_rect_l().top() + 60.0);
    assert!(
        key(&mut s, &mut input, close, egui::Key::F10).is_some(),
        "K2 의 키 게이트"
    );
    input.queue_mut().push(moved(m));
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.iter().all(|(stage, _)| *stage != close));
    assert!(
        !rest.iter().any(|(_, e)| e.iter().any(is_press)),
        "P 가 전환 뒤 화면에 들었다: {rest:?}"
    );
    // B·C 는 빠지고 K2 의 몫은 봉인한 자리(남은 원시 길이 3)에서 시작한다. K2 는 추가 끊기를 만들지 않는다.
    assert_eq!(rest.len(), 3, "B·C·K2 몫의 패스: {rest:?}");
    assert!(
        matches!(
            rest[2].1.as_slice(),
            [Event::Key { key: egui::Key::F10, pressed: true, .. }, Event::PointerMoved(p)] if *p == m
        ),
        "K2 몫: {:?}",
        rest[2].1
    );
    assert!(rest[..2].iter().all(|(_, e)| {
        !e.iter()
            .any(|e| matches!(e, Event::Key { pressed: true, .. }))
    }));
    assert_eq!(
        s.ctx.input(|i| i.pointer.hover_pos()),
        Some(m),
        "이동이 그 화면에 들었다"
    );
}

/// Given 무대, When [무대 누름·뗌, 키, 누름 P] 의 A 를 그리며 무대를 닫고 다음 묶음에 키 K2·이동이 오면, Then K2 와
/// 이동은 일반 화면 패스에 들고 P 는 빠진다.
#[test]
fn the_next_batch_after_the_stage_closed_while_drawing_reaches_the_window() {
    next_batch_after_a_drawing_transition(true);
}

/// Given 일반 화면, When [누름·뗌, 키, 누름 P] 의 A 를 그리며 무대를 열고 다음 묶음에 키 K2·이동이 오면, Then K2 와
/// 이동은 무대 패스에 들고 P 는 빠진다.
#[test]
fn the_next_batch_after_the_stage_opened_while_drawing_reaches_the_stage() {
    next_batch_after_a_drawing_transition(false);
}

/// Given 무대, When [무대 종료 버튼 클릭 A, 무대 키 K] 만 오고 A 를 그리며 무대를 닫은 뒤 다음 묶음에 키 없이 이동과
/// 휠이 오면, Then 그 이동·휠은 일반 패스에 들어 그 화면의 hover 가 된다(K 의 무대 몫에 붙어 빠지지 않는다).
#[test]
fn a_move_and_wheel_after_the_stage_closed_while_drawing_reach_the_window() {
    let (mut s, mut l, mut r, mut input) = scene();
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    let point = screen().center();
    press(&mut s, &mut input, point);
    input.queue_mut().push(release_at(point));
    assert!(key(&mut s, &mut input, false, egui::Key::F9).is_some());
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage);
    let m = egui::pos2(surface_rect_l().center().x, surface_rect_l().top() + 60.0);
    input.queue_mut().push(moved(m));
    input.queue_mut().push(wheel());
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.iter().all(|(stage, _)| !stage));
    assert!(
        rest.iter().any(|(_, e)| e.contains(&moved(m))
            && e.iter().any(|e| matches!(e, Event::MouseWheel { .. }))),
        "이동·휠이 일반 패스에 들지 않았다: {rest:?}"
    );
    assert_eq!(s.ctx.input(|i| i.pointer.hover_pos()), Some(m));
}

/// 무대 누름과 [키, 종료 Esc] 를 한 묶음에 넣고 무대를 닫아 그 몫들을 일반 화면으로 그린다(누름이 빠진다).
fn drop_a_stage_press(s: &mut Scene, input: &mut WindowInput, l: &mut Explorer, r: &mut Explorer) {
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(s, input, l, r));
    press(s, input, screen().center());
    assert!(key(s, input, false, egui::Key::F9).is_some());
    assert!(key(s, input, false, egui::Key::Escape).is_some());
    // 창의 무대 종료 키 처리(`try_consume_fullscreen_stage_key`)가 무대를 닫는다.
    assert!(s.state.close_fullscreen_stage());
    let rest = drain_passes(s, input, l, r);
    assert!(
        rest.iter()
            .all(|(stage, e)| !stage && !e.iter().any(is_press))
    );
}

/// Given 버튼이 모두 올라간 상태, When [무대 누름, 키, 종료 Esc] 로 무대를 닫아 그 누름이 빠진 뒤 다음 묶음에 같은
/// 버튼의 뗌이 오면, Then 새 화면 패스의 실제 사건에 그 뗌이 없고 `button_released` 가 거짓이다.
#[test]
fn the_release_of_a_dropped_stage_press_does_not_reach_the_window() {
    let (mut s, mut l, mut r, mut input) = scene();
    drop_a_stage_press(&mut s, &mut input, &mut l, &mut r);
    input.queue_mut().push(release_at(screen().center()));
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(
        !rest.iter().any(|(_, e)| e.iter().any(is_release)),
        "외톨이 뗌이 새 화면에 들었다: {rest:?}"
    );
    assert!(
        !s.ctx
            .input(|i| i.pointer.button_released(egui::PointerButton::Primary))
    );
}

/// Given 오른쪽 버튼이 일반 화면에서 눌려 egui 의 클릭 조건을 채운 상태, When 같은 순서로 왼쪽 누름이 빠진 뒤 왼쪽
/// 뗌이 오면, Then 그 외톨이 뗌은 `button_clicked` 를 만들지 않는다.
#[test]
fn the_release_of_a_dropped_stage_press_makes_no_click_while_another_button_is_down() {
    let (mut s, mut l, mut r, mut input) = scene();
    let at = empty_l();
    input.queue_mut().push(moved(at));
    input.queue_mut().push(Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Default::default(),
    });
    drop(drain_passes(&mut s, &mut input, &mut l, &mut r));
    assert!(
        s.ctx
            .input(|i| i.pointer.button_down(egui::PointerButton::Secondary))
    );
    drop_a_stage_press(&mut s, &mut input, &mut l, &mut r);
    input.queue_mut().push(release_at(screen().center()));
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(!rest.is_empty());
    assert!(
        !s.ctx
            .input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)),
        "외톨이 뗌이 클릭이 됐다: {rest:?}"
    );
}

/// Given 실제 왼쪽 누름을 일반 화면 패스에서 처리해 egui 가 눌린 채일 때, When 같은 버튼의 다른 누름(debug 누름)이
/// 빠지는 몫에 들고 그 뒤 실제 뗌이 오면, Then 그 뗌은 버리지 않아 egui 의 눌림이 풀린다(눌림 상태 확인이 먼저다).
#[test]
fn a_release_after_a_dropped_press_still_ends_the_press_egui_holds() {
    let (mut s, mut l, mut r, mut input) = scene();
    press(&mut s, &mut input, empty_l());
    drop(drain_passes(&mut s, &mut input, &mut l, &mut r));
    assert!(s.ctx.input(|i| i.pointer.any_down()), "실제 누름을 그렸다");
    drop_a_stage_press(&mut s, &mut input, &mut l, &mut r);
    input.queue_mut().push(release_at(empty_l()));
    drop(drain_passes(&mut s, &mut input, &mut l, &mut r));
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "뗌이 눌림을 풀지 못했다"
    );
}

/// Given 실제 왼쪽 누름을 그려 egui 가 눌린 채일 때, When 같은 버튼의 누름과 뗌이 함께 빠지는 무대 몫에 들면, Then 그
/// 뗌은 egui 의 눌림을 푼다.
#[test]
fn a_release_in_the_same_dropped_segment_as_a_press_ends_the_press_egui_holds() {
    let (mut s, mut l, mut r, mut input) = scene();
    press(&mut s, &mut input, empty_l());
    drop(drain_passes(&mut s, &mut input, &mut l, &mut r));
    assert!(s.ctx.input(|i| i.pointer.any_down()));
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(key(&mut s, &mut input, false, egui::Key::F9).is_some());
    press(&mut s, &mut input, screen().center());
    input.queue_mut().push(release_at(screen().center()));
    assert!(key(&mut s, &mut input, false, egui::Key::Escape).is_some());
    assert!(s.state.close_fullscreen_stage());
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.iter().all(|(stage, _)| !stage));
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "뗌이 눌림을 풀지 못했다"
    );
}

/// Given 무대, When [무대 종료 버튼 클릭 A, 무대 키 K] 뒤 그리기 전에 무대를 닫고 일반 화면 누름 P(banana)를 넣은 뒤
/// 같은 무대를 다시 열고 A 를 그리며 닫으면, Then 일반 화면에서 든 P 는 일반 패스에 들어 L 이 banana 를 고른다(남은 몫을
/// 모두 전환 앞 것으로 보지 않는다).
#[test]
fn a_press_made_on_the_window_between_stage_toggles_reaches_the_window() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    let point = screen().center();
    press(&mut s, &mut input, point);
    input.queue_mut().push(release_at(point));
    assert!(key(&mut s, &mut input, false, egui::Key::F9).is_some());
    assert!(s.state.close_fullscreen_stage());
    press(&mut s, &mut input, banana);
    input.queue_mut().push(release_at(banana));
    assert!(s.state.open_fullscreen_stage("blank"));
    let (stage, _) = stage_or_window_pass_with(&mut s, &mut input, &mut l, &mut r, &mut |st| {
        assert!(st.close_fullscreen_stage());
    });
    assert!(stage);
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(
        rest.iter()
            .any(|(stage, e)| !stage && e.iter().any(is_press))
    );
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// 대조: 같은 화면의 [클릭, 키, 누름] 은 모두 그 화면에 간다. 누름 뒤 키는 앞·뒤 두 패스로 나뉜다.
#[test]
fn a_click_a_key_and_a_press_on_one_screen_all_reach_it() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    let a = empty_l();
    press(&mut s, &mut input, a);
    input.queue_mut().push(release_at(a));
    assert!(key(&mut s, &mut input, true, egui::Key::F9).is_some());
    press(&mut s, &mut input, banana);
    input.queue_mut().push(release_at(banana));
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(rest.len() >= 2, "누름 뒤 키는 두 패스로 나뉜다: {rest:?}");
    assert_eq!(
        rest.iter()
            .map(|(_, e)| e.iter().filter(|e| is_press(e)).count())
            .sum::<usize>(),
        2
    );
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// 대조: 무대 누름이 빠진 뒤 그 버튼으로 일반 화면을 다시 누르면 그 누름의 뗌은 정상 클릭이다.
#[test]
fn a_fresh_click_after_a_dropped_stage_press_is_a_normal_click() {
    let (mut s, mut l, mut r, mut input) = scene();
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    drop_a_stage_press(&mut s, &mut input, &mut l, &mut r);
    wait_out_double_click(&mut s, &mut input, &mut l, &mut r);
    l.actions.clear();
    press(&mut s, &mut input, banana);
    input.queue_mut().push(release_at(banana));
    let rest = drain_passes(&mut s, &mut input, &mut l, &mut r);
    assert!(
        rest.iter().any(|(_, e)| e.iter().any(is_release)),
        "{rest:?}"
    );
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
    assert!(
        !l.actions.iter().any(|a| a.contains("OpenFile")),
        "한 번 클릭이 파일을 열었다: {:?}",
        l.actions
    );
    assert!(!s.ctx.input(|i| i.pointer.any_down()));
}
