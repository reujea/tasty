//! 남는 눌림(`state::held_keys`). egui 에 실은 누름의 뗌은 포커스가 옮겨도 egui 로 가고, OS 포커스 상실과 모달
//! 진입은 그 자리에서 눌림을 푼다. 입력은 창과 같은 끊기·게이트(`WindowInput`), 패스는 egui 패스와 같은 분할로
//! 밟고 눌림은 창 egui 의 `keys_down` 으로 본다.

use egui::{Event, Key, Modifiers};

use super::press_cut::{Scene, drain, open_scene};
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
use crate::state::focus_node::FocusNode;

fn key_a(pressed: bool) -> Event {
    Event::Key {
        key: Key::A,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// 창이 받은 A 누름·뗌 하나(`WindowInput::key`). egui 게이트 결과다.
fn send_a(s: &mut Scene, input: &mut WindowInput, pressed: bool) -> Option<bool> {
    let engine = s.session.borrow_mut();
    input.key(&mut s.state, &engine.read(), false, key_a(pressed))
}

fn a_held(s: &Scene) -> bool {
    s.ctx.input(|i| i.keys_down.contains(&Key::A))
}

/// 검색창(`Text`) 포커스에서 A 를 눌러 그린 장면. egui 는 A 를 눌린 채로 안다.
fn held_in_search(s: &mut Scene, input: &mut WindowInput) {
    assert!(send_a(s, input, true).is_some(), "검색창이 받는다");
    drain(s, input);
    assert!(a_held(s));
}

/// 단축키가 검색창을 닫아 포커스 노드가 터미널로 옮긴다.
fn close_search_by_shortcut(s: &mut Scene) {
    s.state.popups.close(SEARCH_BAR_POPUP_ID);
    let engine = s.session.borrow_mut();
    s.state.normalize_focus(&engine.read());
    assert!(matches!(
        s.state.focus_node(&engine.read()),
        FocusNode::Surface(_)
    ));
}

/// 검색창에서 누른 키의 뗌은 단축키가 포커스를 터미널로 옮긴 뒤에도 egui 로 가 눌림을 푼다.
#[test]
fn a_release_after_a_focus_moving_shortcut_reaches_the_press_receiver() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    close_search_by_shortcut(&mut s);
    assert!(
        send_a(&mut s, &mut input, false).is_some(),
        "짝 누름의 수신자"
    );
    drain(&mut s, &mut input);
    assert!(!a_held(&s));
}

/// 같은 수신자의 뗌은 지금처럼 egui 로 간다.
#[test]
fn a_release_on_the_same_receiver_is_unchanged() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    assert!(send_a(&mut s, &mut input, false).is_some());
    drain(&mut s, &mut input);
    assert!(!a_held(&s));
}

/// egui 에 싣지 않은 누름(터미널)의 뗌은 그 뒤 팝업이 포커스여도 egui 로 가지 않는다.
#[test]
fn a_release_of_a_terminal_press_stays_off_egui() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    close_search_by_shortcut(&mut s);
    assert_eq!(send_a(&mut s, &mut input, true), None, "터미널이 받는다");
    s.state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(
                SEARCH_BAR_POPUP_ID,
                crate::adapters::ui::popup::PopupScope::Surface(super::SURFACE_R),
            )
            .focused(),
            true,
        )
        .expect("검색창이 다시 열린다");
    {
        let engine = s.session.borrow_mut();
        s.state.normalize_focus(&engine.read());
    }
    assert_eq!(send_a(&mut s, &mut input, false), None);
}

/// 창이 OS 포커스를 잃었다가 되찾는다(`MainView` 의 `Focused` 처리). 푸는 뗌은 창 egui 입력에 넣는다.
fn window_focus(s: &mut Scene, input: &mut WindowInput, focused: bool) {
    let releases = s.state.release_on_window_focus(focused);
    input.queue_mut().extend(releases);
    drain(s, input);
}

/// OS 포커스 상실은 합성 짝뗌을 기다리지 않고 egui 의 눌림을 푼다.
#[test]
fn a_window_focus_loss_leaves_no_held_key_in_the_receiver() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    window_focus(&mut s, &mut input, false);
    assert!(!a_held(&s));
}

/// 포커스를 되찾은 뒤 첫 누름은 검색창이 받고 그 뗌도 짝을 찾는다.
#[test]
fn the_first_press_after_focus_returns_is_handled() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    window_focus(&mut s, &mut input, false);
    window_focus(&mut s, &mut input, true);
    assert!(send_a(&mut s, &mut input, true).is_some());
    drain(&mut s, &mut input);
    assert!(a_held(&s));
    assert!(send_a(&mut s, &mut input, false).is_some());
    drain(&mut s, &mut input);
    assert!(!a_held(&s));
}

/// 모달이 열린다(`App::open_modal`). 푸는 뗌은 창 egui 입력에 넣는다.
fn modal_entry(s: &mut Scene, input: &mut WindowInput) {
    let releases = s.state.release_on_modal_entry();
    input.queue_mut().extend(releases);
    drain(s, input);
}

/// 모달이 열리기 전에 누른 키는 모달 진입 때 풀린다(모달 중 뗌은 막힌다).
#[test]
fn a_key_held_before_a_modal_opens_is_released_on_entry() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    modal_entry(&mut s, &mut input);
    assert!(!a_held(&s));
}

/// 모달이 닫힌 뒤 첫 누름은 검색창이 받고 그 뗌도 짝을 찾는다.
#[test]
fn the_first_press_after_the_modal_closes_is_handled() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    held_in_search(&mut s, &mut input);
    modal_entry(&mut s, &mut input);
    assert!(send_a(&mut s, &mut input, true).is_some());
    drain(&mut s, &mut input);
    assert!(a_held(&s));
    assert!(send_a(&mut s, &mut input, false).is_some());
    drain(&mut s, &mut input);
    assert!(!a_held(&s));
}

/// plugin 팝업이 받은 누름의 뗌은 포커스가 옮겨도 그 팝업 입력에 직접 붙고 egui 로 싣지 않는다. 포커스 상실도 같다.
#[test]
fn a_plugin_popup_press_is_released_on_that_popup() {
    let mut s = open_scene(|_| {});
    let fed = s
        .state
        .pair_key_release(Some((Key::A, true)), Some(42), Some(true));
    assert_eq!(fed, Some(true));
    assert_eq!(
        s.state.pair_key_release(Some((Key::A, false)), None, None),
        None,
        "the release goes to that popup alone"
    );
    assert_eq!(s.state.plugin_popup_key_releases, vec![(42, Key::A)]);
    s.state.plugin_popup_key_releases.clear();
    s.state
        .pair_key_release(Some((Key::B, true)), Some(42), Some(true));
    assert_eq!(s.state.release_on_window_focus(false).len(), 1);
    assert_eq!(s.state.plugin_popup_key_releases, vec![(42, Key::B)]);
}

/// Given plugin 팝업 42 가 받은 A 누름이 남아 있고 키를 받는 plugin 팝업이 43 으로 옮겼다, When A 를 떼면, Then 그 뗌은
/// 누름을 받은 42 에만 가고 지금 수신자 43 의 egui 입력으로 싣지 않는다.
#[test]
fn a_plugin_popup_release_does_not_reach_the_new_receiver() {
    let mut s = open_scene(|_| {});
    s.state
        .pair_key_release(Some((Key::A, true)), Some(42), Some(true));
    let fed = s
        .state
        .pair_key_release(Some((Key::A, false)), Some(43), Some(true));
    assert_eq!(fed, None, "the release is not loaded for 43");
    assert_eq!(s.state.plugin_popup_key_releases, vec![(42, Key::A)]);
}

/// Given host 팝업이 받은 A 누름이 남아 있다, When 키 주인이 바뀐 뒤 A 를 떼면, Then 뗌은 egui 로 간다.
#[test]
fn a_host_press_release_still_goes_to_egui() {
    let mut s = open_scene(|_| {});
    s.state
        .pair_key_release(Some((Key::A, true)), None, Some(true));
    let fed = s
        .state
        .pair_key_release(Some((Key::A, false)), Some(43), None);
    assert_eq!(fed, Some(false));
    assert!(s.state.plugin_popup_key_releases.is_empty());
}
