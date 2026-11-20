//! 키 사건이 포커스 노드를 옮기는 전이(검색 단축키) 뒤 같은 묶음의 키. 전이는 그 키 사건 처리 안에서 적용되고, 뒤
//! 키는 사건 시점의 노드(검색창)로 끊긴다. 노드 차이로 끊은 자리에는 새 노드의 빈 패스가 먼저 서서, 검색창 입력칸이
//! 그 패스에서 egui 키 포커스를 얻고 다음 패스에서 뒤 키를 받는다(`PopupManager::cut_key_frame`).
//!
//! 입력은 창과 같은 끊기·게이트(`WindowInput`), 패스는 egui 패스와 같은 분할(`WindowInput::take_pass`)로 밟는다. 단축키
//! 하나는 창처럼 끊기(`key_focus`) → 단축키 처리(`search_bar::open_or_focus_for`) → 사용자 입력이 포커스를 주며 연 팝업
//! 요청의 즉시 처리(`App::apply_user_focus_opens` 가 `user_focus_open_queued` 로 고르고 대기열을 처리한다) 순서다.

use super::press_cut::{Scene, activate, open_scene, terminal_point_r, type_text};
use super::repro::window_frame_with_find;
use super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, surface_rect_l};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::search_bar::{SEARCH_BAR_POPUP_ID, open_or_focus_for};
use crate::state::focus_node::FocusNode;
use crate::state::input_destination::InputDestination;
use crate::view::main::input_segment::key_focus;

/// 창이 다음 패스 입력을 꺼내 한 프레임을 그린다.
fn pass(s: &mut Scene, input: &mut WindowInput) {
    let events = input.take_pass(&mut s.state, &s.ctx);
    let engine = s.session.borrow_mut();
    window_frame_with_find(
        &s.ctx,
        &mut s.state,
        &engine.read(),
        &s.mgr,
        events,
        &mut s.query,
        false,
    );
}

/// 남은 몫을 모두 그리고 그린 패스 수를 돌려준다.
fn drain(s: &mut Scene, input: &mut WindowInput) -> usize {
    let mut passes = 0;
    while !input.is_empty() {
        pass(s, input);
        passes += 1;
    }
    passes
}

/// `pane` 의 터미널을 눌러 포커스 노드를 그 surface 로 옮기고 그린다.
fn focus_terminal(s: &mut Scene, input: &mut WindowInput, pane: u32, point: egui::Pos2) {
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    activate(s, pane);
    drain(s, input);
    // 검색창 입력칸이 포커스를 놓는 프레임까지 그린다.
    pass(s, input);
}

/// 창이 받은 검색 단축키 하나. 끊기 → 단축키 처리 → 포커스를 주며 연 팝업 요청의 즉시 처리 순서다.
fn find_shortcut(s: &mut Scene, input: &mut WindowInput, sid: u32) {
    let mut engine = s.session.borrow_mut();
    key_focus(&mut s.state, &engine.read(), input);
    open_or_focus_for(&mut s.state, Some(sid), "find");
    if s.state.user_focus_open_queued() {
        for intent in s.state.take_pending_intents() {
            crate::intent::popup::handle(&mut s.state, &mut engine, &intent);
        }
    }
}

/// [k, 검색 단축키, w] 를 한 묶음으로 보내고 k 는 `sid` 터미널(PTY), w 는 그 surface 의 검색창 입력칸에 들어가는지 잰다.
fn k_find_w(s: &mut Scene, input: &mut WindowInput, sid: u32) {
    assert_eq!(
        type_text(s, input, false, "k"),
        None,
        "k 는 egui 로 가지 않는다"
    );
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state
                .input_destination(&engine.read(), s.state.window_holds_terminal_keys()),
            InputDestination::Terminal(sid),
            "k 의 갈래는 터미널(PTY)이다"
        );
    }
    find_shortcut(s, input, sid);
    let search = {
        let engine = s.session.borrow_mut();
        let node = s.state.focus_node(&engine.read());
        let Some(p) = s.state.popups.host_key_owner() else {
            panic!("검색 단축키 뒤 포커스 노드가 검색창이 아니다: {node:?}");
        };
        assert_eq!(p.id, SEARCH_BAR_POPUP_ID);
        assert_eq!(node, FocusNode::Popup(PopupNode::Host(p.instance)));
        p.instance
    };
    assert!(
        type_text(s, input, false, "w").is_some(),
        "w 는 검색창 몫이라 egui 로 간다"
    );
    let passes = drain(s, input);
    assert_eq!(
        s.state.search.query, "w",
        "w 가 검색창 입력칸에 들어간다 (패스 {passes})"
    );
    assert_eq!(
        s.state.popups.instance(search).map(|p| p.scope.clone()),
        Some(crate::adapters::ui::popup::PopupScope::Surface(sid))
    );
}

/// Given R 에 검색창이 열려 있고 포커스 노드가 R 터미널일 때, When 한 묶음에 [k, 검색 단축키, w]가 오면, Then k 는 R
/// 터미널로 가고 단축키가 검색창에 포커스를 즉시 준다. w 는 노드 차이로 끊긴 새 몫이고, 그 앞 빈 패스에서 입력칸이
/// 포커스를 얻어 w 를 받는다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_already_open_search_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    s.state.search.query.clear();
    k_find_w(&mut s, &mut input, SURFACE_R);
}

/// Given 검색창이 R 에 열려 있고 포커스 노드가 L 터미널일 때, When 한 묶음에 [k, L 의 검색 단축키, w]가 오면, Then 단축키의
/// 다시 열기 요청(검색창을 L 로 옮김)이 그 키 처리 안에서 적용되고 w 는 L 의 검색창에 들어간다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_search_field_moved_from_another_surface() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = surface_rect_l().right_bottom() - egui::vec2(20.0, 20.0);
    focus_terminal(&mut s, &mut input, PANE_L, point);
    k_find_w(&mut s, &mut input, SURFACE_L);
}

/// Given 검색창이 열려 있지 않고 포커스 노드가 R 터미널일 때, When 한 묶음에 [k, 검색 단축키, w]가 오면, Then 단축키의
/// 열기 요청이 그 키 처리 안에서 적용되어 새 검색창이 포커스 노드가 되고 w 는 그 입력칸에 들어간다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_newly_opened_search_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    s.state.popups.close(SEARCH_BAR_POPUP_ID);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    assert!(!s.state.popups.is_open(SEARCH_BAR_POPUP_ID));
    s.state.search.query.clear();
    k_find_w(&mut s, &mut input, SURFACE_R);
}

/// Given L 찾기 입력란에 포커스가 없을 때, When 한 묶음에 [L 찾기 입력란 누름, debug 붙여넣기 주입]이 오면, Then 붙여넣기는
/// 키 주입과 같이 누름 뒤에서 끊겨(`debug_input::inject_clipboard`) 누름 패스 다음 패스로 그려지고 입력란에 들어간다.
#[test]
fn a_debug_paste_after_a_press_is_the_next_pass() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.find.center());
    }
    activate(&mut s, PANE_L);
    {
        let engine = s.session.borrow_mut();
        crate::view::main::debug_input::inject_clipboard(
            &mut s.state,
            &engine.read(),
            &mut input,
            egui::Event::Paste("zz".into()),
        );
    }
    let passes = drain(&mut s, &mut input);
    assert_eq!(passes, 2, "누름 패스와 붙여넣기 패스");
    assert_eq!(
        s.query, "zz",
        "붙여넣기가 누름이 포커스를 준 입력란에 들어간다"
    );
}
