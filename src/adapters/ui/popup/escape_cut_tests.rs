//! 수식키 없는 Esc 누름의 입력 몫(조건 8). 앞 경계: 지금 몫에 키 입력·누름·결속·넘긴 Esc 가 있으면 Esc 앞에서 끊는다.
//! 뒤 경계: egui 입력에 실은 Esc 는 그 몫을 끝내 다음 사건이 새 몫이다(`MainViewState::mark_escape_segment`). egui 가
//! 아닌 수신자(터미널)로 간 Esc 는 큐·결속을 남기지 않아 뒤 경계도 없다. 창 사건 입구는 `window_input_test_support`
//! 가 창과 같은 함수로 밟는다(끊기 → 단계 4 판정 → egui 게이트 → 뒤 경계 표시).

use super::press_cut::{activate, drain, open_scene, pass, terminal_point_r, type_text};
use super::{PANE_R, SURFACE_R};
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::state::focus_node::FocusNode;

fn escape_event() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 창 Esc 처리(`apply_escape_decision`)를 거치지 않는 bare Esc 누름 하나. 창 Esc 처리가 노드를 옮기지 않는 경우(선언 Ignore)와
/// 같은 사건 입구다. 게이트 결과를 돌려준다.
fn escape_key(s: &mut super::press_cut::Scene, input: &mut WindowInput) -> Option<bool> {
    let engine = s.session.borrow_mut();
    input.key_events(&mut s.state, &engine.read(), false, vec![escape_event()])
}

/// 창 Esc 처리까지 거치는 bare Esc 누름 하나(`WindowInput::escape`). 창이 처리했으면 참이다.
fn window_escape(s: &mut super::press_cut::Scene, input: &mut WindowInput) -> bool {
    let engine = s.session.borrow_mut();
    input.escape(&s.ctx, &mut s.state, &engine.read(), true)
}

/// Given R 검색창이 키 주인일 때, When 한 묶음에 [글자 x, bare Esc]가 오면, Then 노드도 누름도 바뀌지 않았지만
/// Esc 앞에서 끊어 x 몫이 먼저 그려지고 x 는 검색어에 들어간다. Esc 몫이 검색창을 닫는다.
#[test]
fn a_letter_then_escape_on_one_popup_cut_before_the_escape() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert_eq!(type_text(&mut s, &mut input, false, "x"), Some(true));
    assert_eq!(input.cut_count(), 0, "글자 앞에서는 끊지 않는다");
    assert!(window_escape(&mut s, &mut input), "창이 Esc 를 처리한다");
    assert_eq!(input.cut_count(), 1, "Esc 앞에서 끊는다");
    pass(&mut s, &mut input);
    assert_eq!(s.state.search.query, "x", "x 몫이 먼저 그려진다");
    assert!(s.state.popups.instance(s.search).is_some_and(|p| p.open));
    drain(&mut s, &mut input);
    assert!(
        !s.state.popups.instance(s.search).is_some_and(|p| p.open),
        "Esc 몫이 검색창을 닫는다"
    );
}

/// Given R 검색창이 키 주인일 때, When bare Esc 만 오면, Then 끊지 않고 그 Esc 가 검색창을 닫는다.
#[test]
fn an_escape_alone_on_a_popup_closes_it() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert!(window_escape(&mut s, &mut input), "창이 Esc 를 처리한다");
    assert_eq!(input.cut_count(), 0, "빈 몫의 Esc 는 끊지 않는다");
    // 창이 처리한 Esc 는 egui 입력에서 빠져 큐가 비어도 창은 그 Esc 를 실은 패스를 그린다.
    pass(&mut s, &mut input);
    assert!(input.is_empty());
    assert!(
        !s.state.popups.instance(s.search).is_some_and(|p| p.open),
        "Esc 가 검색창을 닫는다"
    );
}

/// Given R 검색창이 키 주인일 때, When 한 묶음에 [bare Esc, 글자 x]가 오고 Esc 가 노드를 옮기지 않으면,
/// Then egui 에 실은 Esc 가 그 몫을 끝내 x 앞에서 끊는다.
#[test]
fn a_key_after_an_escape_on_the_same_popup_starts_a_new_segment() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert!(
        escape_key(&mut s, &mut input).is_some(),
        "Esc 는 egui 로 간다"
    );
    assert_eq!(input.cut_count(), 0);
    assert_eq!(type_text(&mut s, &mut input, false, "x"), Some(true));
    assert_eq!(input.cut_count(), 1, "Esc 뒤 글자는 새 몫이다");
}

/// Given R 검색창이 키 주인이고 지금 몫이 비었을 때, When bare Esc 가 오면, Then 끊지 않는다.
#[test]
fn an_escape_into_an_empty_segment_does_not_cut() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert!(
        escape_key(&mut s, &mut input).is_some(),
        "Esc 는 egui 로 간다"
    );
    assert_eq!(input.cut_count(), 0, "빈 몫의 Esc 는 끊지 않는다");
}

/// Given R 터미널이 포커스 노드일 때, When 한 묶음에 [bare Esc, 글자 x]가 오면, Then 둘 다 터미널로 가고
/// egui 큐·결속을 남기지 않으므로 Esc 뒤에서 끊지 않는다.
#[test]
fn a_key_after_an_escape_to_the_terminal_shares_its_segment() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let point = terminal_point_r(&s);
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    activate(&mut s, PANE_R);
    drain(&mut s, &mut input);
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_R)),
            "준비: R 터미널이 포커스 노드다"
        );
    }
    assert_eq!(input.cut_count(), 0);
    assert_eq!(escape_key(&mut s, &mut input), None, "Esc 는 터미널로 간다");
    assert_eq!(
        type_text(&mut s, &mut input, false, "x"),
        None,
        "x 는 터미널로 간다"
    );
    assert_eq!(input.cut_count(), 0, "터미널로 간 Esc 뒤에서 끊지 않는다");
}

/// Given 선언 `Close` 인 R 검색창이 포커스 노드일 때, When bare Esc 가 오면, Then 사건 시점 수신자가 그
/// 검색창이고(Ignore 가 아니라 건너뛰지 않는다) 창의 Esc 적용이 처리해 그 Esc 가 검색창을 닫는다.
#[test]
fn escape_on_a_close_popup_closes_it() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        let read = engine.read();
        let egui_surface =
            crate::adapters::ui::input::key_nodes::focused_surface_takes_egui(&s.state, &read);
        let decision = crate::adapters::ui::input::key_nodes::bare_escape_decision(
            &s.state,
            &read,
            egui_surface,
        );
        assert!(
            matches!(
                decision,
                crate::adapters::ui::input::key_nodes::KeyDecision::Path(routed)
                    if routed.receiver
                        == crate::adapters::ui::input::key_route::KeyReceiver::Popup(s.search)
            ),
            "수신자는 검색창이다: {decision:?}"
        );
    }
    assert!(window_escape(&mut s, &mut input), "창이 Esc 를 처리한다");
    pass(&mut s, &mut input);
    assert!(
        !s.state.popups.instance(s.search).is_some_and(|p| p.open),
        "Esc 가 검색창을 닫는다"
    );
}

/// 창이 winit 키 사건으로 받은 Escape 자리 누름 하나(`WindowInput::winit_escape`). 창이 처리했으면 참이다.
fn winit_escape(
    s: &mut super::press_cut::Scene,
    input: &mut WindowInput,
    logical: winit::keyboard::Key,
) -> bool {
    let engine = s.session.borrow_mut();
    input.winit_escape(&s.ctx, &mut s.state, &engine.read(), logical)
}

/// Given R 검색창이 포커스 노드일 때, When 논리 키를 모르는 배열에서 물리 Escape 자리를 수식키 없이 누르면,
/// Then 단계 11 입구도 단계 4 와 같은 판정(논리 키가 없으면 물리 키)으로 그 누름을 bare Esc 로 보아 창이 처리하고 그
/// Esc 가 검색창을 닫는다.
#[test]
fn a_physical_only_escape_closes_the_focused_popup() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert!(
        winit_escape(
            &mut s,
            &mut input,
            winit::keyboard::Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
        ),
        "창이 물리 Escape 자리 누름을 처리한다"
    );
    pass(&mut s, &mut input);
    assert!(
        !s.state.popups.instance(s.search).is_some_and(|p| p.open),
        "그 Esc 가 검색창을 닫는다"
    );
}

/// Given R 검색창이 포커스 노드일 때, When 논리 키 Escape 를 수식키 없이 누르면, Then 창이 처리하고 그 Esc 가
/// 검색창을 닫는다.
#[test]
fn a_logical_escape_closes_the_focused_popup() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert!(
        winit_escape(
            &mut s,
            &mut input,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape),
        ),
        "창이 Esc 를 처리한다"
    );
    pass(&mut s, &mut input);
    assert!(
        !s.state.popups.instance(s.search).is_some_and(|p| p.open),
        "그 Esc 가 검색창을 닫는다"
    );
}
