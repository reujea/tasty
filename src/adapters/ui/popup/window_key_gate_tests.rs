//! 창의 키 처리(egui 전달 게이트 `window_feeds_key`, Esc 처리 `window_escape`, 터미널 게이트
//! `window_key_holds_terminal`)를 거친 키를 실은 프레임은 창이 정한 키 주인 하나만 그 키를 소비한다. 본문, 접힌
//! 보기 버튼의 Enter·Space, 접기 버튼이 모두 그 값을 쓴다.
//!
//! 이 시험들은 창(winit·GPU)을 띄우지 않고 창과 같은 판정 함수로 키를 보낸다. 판정 함수 안의 변이는 이 장면 시험이
//! 잡는다. 창 호출부(`view::main::feed_egui_keyboard`, `keyboard::handle_keyboard_input`)가 그 함수를 거치지 않고
//! 키를 보내는 우회는 이 시험이 잡지 못하고 소스 가드 `source_guards::window_key_gate_call_sites` 가 잡는다.

use super::*;
use crate::adapters::ui::LayoutContext;
use crate::intent::OpenRequest;
use crate::state::RequestContext;

const ID: PopupId = "window_key_gate_test";

fn rect(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
}

/// surface 1 은 넓고, surface 2 는 팝업이 접히는 좁은 범위, surface 3 은 넓다.
fn layout() -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects: vec![
            (1, rect(0.0, 0.0, 600.0, 700.0)),
            (2, rect(600.0, 0.0, 120.0, 80.0)),
            (3, rect(600.0, 100.0, 600.0, 600.0)),
        ],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    }
}

/// 창이 키 사건 하나를 받는 경로(`view::main::feed_egui_keyboard` → `keyboard::handle_keyboard_input`)를 창과 같은
/// 판정 함수(단계 4 판정, K9 게이트 `window_feeds_key`, 터미널 게이트 `window_key_holds_terminal`)로 밟는다. egui 로 보낼 키만 `pending`(그 키를 실은 프레임의 egui 입력)에 싣는다. egui 로 보내지 않은 Esc
/// 뗌은 그 몫의 패스를 닫을 때 egui 의 Escape 눌림 기록을 풀도록 남긴다(`MainView::note_unfed_key`). 뗌은 처리하지 않고, Esc 누름은 창의 Esc 처리를
/// 거친다(`ctx` 의 직전 패스 Escape 눌림을 넘긴다). 터미널(PTY)로 가면 참이다. 창 호출부를 부르지 않으므로 호출부
/// 우회는 소스 가드 몫이다.
fn window_key(
    ctx: &egui::Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pending: &mut Vec<egui::Event>,
    event: egui::Event,
) -> bool {
    // 글자 사건은 그 키 누름이 만든다.
    let (escape, pressed) = match &event {
        egui::Event::Key { key, pressed, .. } => (*key == egui::Key::Escape, *pressed),
        _ => (false, true),
    };
    let loaded_from = pending.len();
    let decision =
        super::window_input_test_support::window_key_decision_of(state, engine, &event, false);
    if state.window_feeds_key(decision, false, true).is_some() {
        pending.push(event.clone());
    } else if escape && !pressed {
        state.popups.note_unfed_escape_release();
    }
    if !pressed {
        return false;
    }
    if escape
        && state.window_escape(
            pending,
            Some(loaded_from),
            ctx.input(|i| i.key_down(egui::Key::Escape)),
        )
    {
        return false;
    }
    !state.window_key_holds_terminal(decision)
}

fn enter() -> egui::Event {
    key(egui::Key::Enter)
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 창의 한 프레임: 이번 프레임 범위·표시 판정 → 키 주인 결정 → host 팝업 그리기(`frame::draw_popup_layer` 순서).
/// 본문에서 Enter 를 읽은 인스턴스를 돌려준다.
fn frame(
    ctx: &egui::Context,
    state: &mut RequestContext,
    events: Vec<egui::Event>,
) -> Vec<PopupInstanceId> {
    frame_reading(ctx, state, events, egui::Key::Enter)
}

/// [`frame`]과 같고, 본문에서 `read` 키를 읽은 인스턴스를 돌려준다.
fn frame_reading(
    ctx: &egui::Context,
    state: &mut RequestContext,
    events: Vec<egui::Event>,
    read: egui::Key,
) -> Vec<PopupInstanceId> {
    frame_calls(ctx, state, events, read).1
}

/// [`frame_reading`]과 같고, 본문 콜백이 불린 인스턴스도 함께 돌려준다(불림, 키를 읽음).
fn frame_calls(
    ctx: &egui::Context,
    state: &mut RequestContext,
    events: Vec<egui::Event>,
    read: egui::Key,
) -> (Vec<PopupInstanceId>, Vec<PopupInstanceId>) {
    let layout = layout();
    let mut called = Vec::new();
    let mut seen = Vec::new();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events,
        ..Default::default()
    };
    state
        .popups
        .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
    drop(ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(
            ctx,
            &mut |cx, ui| {
                if !called.contains(&cx.instance) {
                    called.push(cx.instance);
                }
                if ui.input(|i| i.key_pressed(read)) && !seen.contains(&cx.instance) {
                    seen.push(cx.instance);
                }
            },
            Some(&layout),
            &[],
        );
    }));
    state.close_input_pass(ctx);
    (called, seen)
}

struct Scene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    ctx: egui::Context,
    /// surface 2 범위의 접힌 팝업. 포커스를 주며 열었지만 X 를 연 뒤로 포커스 위치가 아니다.
    g: PopupInstanceId,
    /// surface 1 범위의 키 주인(맨 위, 포커스). 바깥 클릭에 닫히지 않는다.
    x: PopupInstanceId,
}

fn open(state: &mut RequestContext, scope: u32, focused: bool) -> PopupInstanceId {
    let req = OpenRequest::new(ID, PopupScope::Surface(scope));
    let req = if focused { req.focused() } else { req };
    state.popups.open_request(&req, focused).unwrap()
}

/// G(surface 2, 접힘, 포커스) → X(surface 1, 포커스) → `extra` 순서로 연다. 포커스 위치는 창에 하나라 X 가 키
/// 주인이고 G 는 열린 채 포커스가 없다.
fn open_scene(extra: impl FnOnce(&mut RequestContext)) -> Scene {
    crate::i18n::init("en");
    let (mut state, session) = crate::state::tests::test_state();
    state.popups.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let g = open(&mut state, 2, true);
    let x = open(&mut state, 1, true);
    extra(&mut state);
    // 필요 크기를 고정한다. G 만 surface 2 범위보다 커서 접힌다.
    for p in state.popups.popups.iter_mut() {
        p.close_on_outside_click = false;
        p.arrange.fit = super::PopupFit::Collapse;
        p.size_user_overridden = true;
        let needed = if p.instance == g {
            (400.0, 300.0)
        } else {
            (240.0, 200.0)
        };
        p.set_needed_size(egui::vec2(needed.0, needed.1));
    }
    let ctx = egui::Context::default();
    assert!(frame(&ctx, &mut state, Vec::new()).is_empty());
    let gp = state.popups.instance(g).unwrap();
    assert!(
        gp.open && gp.collapsed() && !state.popups.is_instance_focused(g),
        "G 는 접힌 채 열려 있고 포커스가 없다"
    );
    assert_eq!(
        state.popups.focused_dismissal_target().map(|t| t.1),
        Some(x),
        "X 가 키 주인이다"
    );
    Scene {
        state,
        session,
        ctx,
        g,
        x,
    }
}

/// Given 키 주인 X 와 다른 범위에 열린 접힌 G 가 있을 때, When 창이 Esc 로 X 의 포커스를 풀고 같은 프레임에
/// Enter 가 함께 오면, Then X 본문 하나만 Enter 를 읽고 G 의 접힌 보기 버튼은 Enter 로 펼쳐지지 않는다.
#[test]
fn a_key_after_the_window_released_the_owner_reaches_only_that_owner() {
    let Scene {
        mut state,
        mut session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(
        !state.popups.is_instance_focused(x),
        "창이 X 의 포커스를 푼다"
    );
    assert!(
        !window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut pending,
            enter()
        ),
        "같은 프레임의 Enter 는 터미널로 가지 않는다"
    );
    // 창은 그 Esc 를 egui 입력에서 뺐다. 같은 프레임의 Enter 만 egui 로 간다.
    assert_eq!(pending, [enter()]);
    let seen = frame(&ctx, &mut state, pending);
    assert_eq!(seen, [x], "Enter 는 X 본문만 읽는다");
    assert!(
        state.popups.instance(g).unwrap().collapsed(),
        "G 의 접힌 보기 버튼은 Enter 를 받지 않는다"
    );
}

/// Given 키 주인 X 와 그 위에 포커스 없는 A(다른 범위)가 있을 때, When A 를 누른 프레임에 Enter 가 오면, Then 그
/// 프레임 키 주인은 창이 입력 시점에 묶은 X(누름을 반영하기 전 값)라 X 본문 하나만 Enter 를 읽는다. 누름은 그리기
/// 앞부분에서 A 에 포커스를 주고(포커스 표시는 A), A 는 다음 키부터 받는다.
#[test]
fn a_key_in_the_frame_of_a_press_reaches_only_the_owner_before_the_press() {
    let mut a = None;
    let Scene {
        mut state,
        session: _session,
        ctx,
        g,
        x,
    } = open_scene(|state| a = Some(open(state, 3, false)));
    let a = a.unwrap();
    let at = state.popups.instance(a).unwrap().content_rect().center();
    assert!(
        state.bind_window_key_owner(),
        "창이 Enter 의 키 주인을 묶는다"
    );
    let seen = frame(
        &ctx,
        &mut state,
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            enter(),
        ],
    );
    assert!(
        state.popups.is_instance_focused(a),
        "누름이 A 에 포커스를 준다"
    );
    assert_eq!(seen, [x], "Enter 는 X 본문만 읽는다");
    assert!(state.popups.instance(g).unwrap().collapsed());

    assert!(state.bind_window_key_owner());
    let seen = frame(&ctx, &mut state, vec![enter()]);
    assert_eq!(seen, [a], "다음 키부터 A 가 받는다");
}

/// Given 키 주인 X 가 바깥 클릭에 닫히는 팝업이고 포커스를 받지 않는 팝업 N(복원 안내 같은)이 함께 보일 때, When
/// 창이 Esc 를 처리한 뒤 그리기 전에 X 가 닫히면, Then 그 Esc 는 처리한 X 에서 멈춘다. 키 주인이 사라졌어도
/// 그 프레임은 키 주인이 있는 프레임이라 N 의 본문은 그 Esc 를 읽지 않는다.
#[test]
fn an_escape_bound_to_an_owner_that_closed_does_not_reach_a_popup_below() {
    let mut n = None;
    let Scene {
        mut state,
        mut session,
        ctx,
        g: _,
        x,
    } = open_scene(|state| {
        let iid = open(state, 3, false);
        let p = state.popups.instance_mut(iid).unwrap();
        // 포커스를 받지 않는 선언과 같게 두 값을 함께 바꾼다(`takes_focus == (keyboard != None)`).
        p.arrange.takes_focus = false;
        p.keyboard = super::PopupKeyboard::None;
        n = Some(iid);
    });
    let n = n.unwrap();
    state.popups.instance_mut(x).unwrap().close_on_outside_click = true;
    assert!(frame_reading(&ctx, &mut state, Vec::new(), egui::Key::Escape).is_empty());
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(pending.is_empty(), "닫은 주인의 Esc 는 egui 입력에서 뺀다");
    // 창 Esc 는 닫기 의도를 만들지 않는다. 그 Esc 뒤·그리기 전에 X 를 따로 닫는다. 아래에서는 Esc 가 egui 에
    // 남았다고 쳐도 N 이 읽지 않는지 본다.
    state
        .popups
        .close_instance(x, super::PopupCloseReason::Command);
    assert!(
        !state.popups.instance(x).is_some_and(|p| p.open),
        "X 가 닫혔다"
    );
    let (called, seen) = frame_calls(
        &ctx,
        &mut state,
        vec![key(egui::Key::Escape)],
        egui::Key::Escape,
    );
    assert!(called.contains(&n), "N({n:?})의 본문이 불렸다: {called:?}");
    assert!(!seen.contains(&n), "N({n:?})은 그 Esc 를 읽지 않는다");
    assert!(
        seen.is_empty(),
        "아래 N({n:?})은 그 Esc 를 읽지 않는다: {seen:?}"
    );
}

/// Given 키 주인 X 와 다른 범위에 열린 G(바깥 클릭에 닫힘)가 있을 때, When 그리기 전에 Esc 가 두 번 오면, Then
/// 창은 첫 Esc 로 X 만 처리하고 두 번째 Esc 는 이번 프레임 키 주인 X 것으로 끝낸다. G 는 열린 채고 포커스를 얻지
/// 않는다.
#[test]
fn a_second_escape_before_the_draw_stays_with_the_bound_owner() {
    let Scene {
        mut state,
        mut session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    state.popups.instance_mut(g).unwrap().close_on_outside_click = true;
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(!state.popups.is_instance_focused(x));
    assert!(
        !window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut pending,
            key(egui::Key::Escape)
        ),
        "두 번째 Esc 도 X 것이다"
    );
    assert!(state.take_pending_intents().is_empty(), "G 를 닫지 않는다");
    let gp = state.popups.instance(g).unwrap();
    assert!(
        gp.open && !state.popups.is_instance_focused(g),
        "G 는 열린 채고 포커스를 얻지 않는다"
    );
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        enter()
    ));
    assert_eq!(pending, [enter()], "두 Esc 는 egui 입력에서 빠진다");
    let seen = frame(&ctx, &mut state, pending);
    assert_eq!(seen, [x], "그 프레임 키는 X 본문만 읽는다");
}

/// Given 팝업이 X 하나뿐일 때, When 창이 Esc 로 X 의 포커스를 푼 뒤 같은 프레임에 Enter 가 오면, Then 창의 터미널
/// 게이트(`window_key_holds_terminal`)는 그 프레임 동안 닫혀 있어 Enter 가 터미널로 가지 않는다. 그 프레임을 그린
/// 뒤에는 열린다. 판정 함수(`window_feeds_key`·`window_key_holds_terminal`)의 변이는 이 시험이 잡고, 창 호출부가
/// 그 함수를 우회하는 변이는 `source_guards::window_key_gate_call_sites` 가 잡는다.
#[test]
fn a_key_after_the_window_released_the_only_popup_does_not_reach_the_terminal() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    state.popups.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let x = open(&mut state, 1, true);
    {
        let p = state.popups.instance_mut(x).unwrap();
        p.close_on_outside_click = false;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(240.0, 200.0));
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, Vec::new());
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(!state.popups.has_focused(), "X 의 포커스가 풀렸다");
    assert!(
        !window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut pending,
            enter()
        ),
        "같은 프레임의 Enter 는 터미널로 가지 않는다"
    );
    assert_eq!(pending, [enter()], "그 Enter 는 egui 로 간다");
    let seen = frame(&ctx, &mut state, pending);
    assert_eq!(seen, [x], "그 Enter 는 X 본문이 읽는다");
    let mut pending = Vec::new();
    assert!(
        window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut pending,
            enter()
        ),
        "다음 키부터 터미널로 간다"
    );
    assert!(pending.is_empty(), "다음 키는 egui 로 가지 않는다");
}

/// 포커스된 host 팝업 X(Esc 선언 Close)가 키 주인인 장면. X 가 키 주인임을 먼저 단정한다.
fn focused_close_owner() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    egui::Context,
    PopupInstanceId,
) {
    crate::i18n::init("en");
    let (mut state, session) = crate::state::tests::test_state();
    state.popups.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let x = open(&mut state, 1, true);
    {
        let p = state.popups.instance_mut(x).unwrap();
        p.close_on_outside_click = false;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(240.0, 200.0));
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, Vec::new());
    assert_eq!(
        state.popups.focused_dismissal_target().map(|t| t.1),
        Some(x),
        "X 가 키 주인이다"
    );
    (state, session, ctx, x)
}

/// 수식키 Esc 는 창 Esc 가 아니다: 창이 처리하지 않고(Pass), X(`Keys`)가 소비하지 않는 수식키 조합이라 키 경로를
/// 올라가 egui 큐에 실리지 않는다. X 는 열린 채 포커스를 유지한다.
fn modified_escape_is_not_a_window_escape(modifiers: egui::Modifiers) {
    use crate::adapters::ui::popup::window_input_test_support::WindowInput;
    let (mut state, mut session, ctx, x) = focused_close_owner();
    let engine = session.borrow_mut();
    let mut input = WindowInput::default();
    let handled = input.escape_with(&ctx, &mut state, &engine.read(), true, modifiers);
    assert!(
        !input.escape_fed(),
        "그 누름은 egui 큐에 실리지 않는다: {modifiers:?}"
    );
    assert!(!handled, "창이 처리하지 않는다: {modifiers:?}");
    assert!(
        !input
            .queue_mut()
            .iter()
            .any(super::body_keys::is_escape_press),
        "egui 큐에 Esc 누름이 없다: {modifiers:?}"
    );
    assert!(state.popups.instance(x).is_some_and(|p| p.open));
    assert!(
        state.popups.is_instance_focused(x),
        "X 는 포커스를 유지한다"
    );
}

/// T-4a3-15a: Shift+Esc 는 창 Esc 가 아니다(공유 판정 `tasty_key_match::is_window_escape` 는 정확 일치).
#[test]
fn shift_escape_is_not_a_window_escape() {
    modified_escape_is_not_a_window_escape(egui::Modifiers::SHIFT);
}

/// T-4a3-15b: Ctrl+Esc 도 창 Esc 가 아니다.
#[test]
fn ctrl_escape_is_not_a_window_escape() {
    modified_escape_is_not_a_window_escape(egui::Modifiers::CTRL);
}

/// T-4a3-15c: Alt+Esc 도 창 Esc 가 아니다.
#[test]
fn alt_escape_is_not_a_window_escape() {
    modified_escape_is_not_a_window_escape(egui::Modifiers::ALT);
}

/// T-4a3-15d: 수식키 없는 Esc 는 창이 처리하고 이번에 실린 그 사건을 egui 큐에서 뺀다.
#[test]
fn bare_escape_is_a_window_escape() {
    use crate::adapters::ui::popup::window_input_test_support::WindowInput;
    let (mut state, mut session, ctx, _x) = focused_close_owner();
    let engine = session.borrow_mut();
    let mut input = WindowInput::default();
    let handled = input.escape_with(
        &ctx,
        &mut state,
        &engine.read(),
        true,
        egui::Modifiers::NONE,
    );
    assert!(input.escape_fed(), "그 누름이 egui 큐에 실렸다");
    assert!(handled, "창이 처리한다");
    assert!(
        !input
            .queue_mut()
            .iter()
            .any(super::body_keys::is_escape_press),
        "창이 처리한 누름은 egui 큐에서 빠졌다"
    );
}

/// T-4a3-15e: 이번 사건이 큐에 실리지 않았으면(적재 범위가 비었다) 창은 앞 사건으로 남은 Esc 누름을 빼지 않는다.
#[test]
fn an_unloaded_escape_leaves_an_older_press_in_the_queue() {
    let (mut state, _session, _ctx, _x) = focused_close_owner();
    let mut pending = vec![key(egui::Key::Escape)];
    let loaded_from = Some(pending.len());
    assert!(
        state.window_escape(&mut pending, loaded_from, false),
        "키 주인이 있어 창이 처리한다"
    );
    assert_eq!(
        pending
            .iter()
            .filter(|e| super::body_keys::is_escape_press(e))
            .count(),
        1,
        "앞 사건으로 남은 누름은 그대로다"
    );
}

/// T-4a3-8: Given 덮지 않는 검색 바가 키 주인일 때, When 한 창 사건 묶음 [Esc, 글자] 가 오면(창이 Esc 를 egui 입력에서
/// 빼고 사건 시점에 포커스를 푼다), Then 다음 글자는 아래 surface(터미널)로 가고 egui 입력에 실리지 않는다.
#[test]
fn next_key_after_escape_on_a_non_covering_popup_goes_below() {
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    state.popups.open_at_focused(
        SEARCH_BAR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        egui::pos2(100.0, 100.0),
    );
    let screen = crate::model::PhysicalRect {
        x: crate::model::PhysicalPx(0.0),
        y: crate::model::PhysicalPx(0.0),
        width: crate::model::PhysicalPx(1280.0),
        height: crate::model::PhysicalPx(720.0),
    };
    for _ in 0..3 {
        drop(ctx.run(
            egui::RawInput {
                screen_rect: Some(rect(0.0, 0.0, 1280.0, 720.0)),
                ..Default::default()
            },
            |ctx| {
                drop(crate::adapters::ui::draw_popups(
                    ctx,
                    &mut state,
                    &read,
                    &[],
                    screen,
                    1.0,
                ));
            },
        ));
        state.close_input_pass(&ctx);
    }
    assert!(
        state.popups.is_focused(SEARCH_BAR_POPUP_ID),
        "검색 바가 키 주인이다"
    );
    assert!(
        !state.popups.covers_scope(SEARCH_BAR_POPUP_ID),
        "덮지 않는 팝업이다"
    );
    // 한 묶음: [Esc, 글자]. redraw 없이 창이 차례로 받는다(사건마다 끊기 → egui 게이트 → 창 판정).
    let mut input = crate::adapters::ui::popup::window_input_test_support::WindowInput::default();
    assert!(
        input.escape(&ctx, &mut state, &read, true),
        "창이 Esc 를 처리하고 사건 시점에 포커스를 푼다"
    );
    assert!(
        input.queue_mut().is_empty(),
        "창이 그 Esc 를 egui 입력에서 뺐다"
    );
    assert_eq!(input.cut_count(), 0, "Esc 까지는 한 몫이다");
    assert_eq!(
        input.key(&mut state, &read, false, key(egui::Key::A)),
        None,
        "다음 글자는 egui 로 가지 않는다"
    );
    // 끊은 자리는 글자 앞이다. 빈 준비 패스 자리가 하나 더 생길 수 있다.
    assert!(
        input.cut_count() >= 1,
        "포커스 노드가 바뀌어 글자 앞에서 몫을 끊는다"
    );
    // 창의 뒤 단계는 그 사건에 내린 판정을 읽는다(다시 계산하지 않는다).
    assert!(
        !state.window_key_holds_terminal(input.last_decision()),
        "다음 글자는 아래 surface(터미널)로 간다"
    );
    assert!(
        !matches!(
            state.focus_node(&read),
            crate::state::focus_node::FocusNode::Popup(_)
        ),
        "포커스 노드가 팝업이 아니다"
    );
}

/// 이번 프레임 키 주인의 egui 사건을 버튼이 가져가는지 본다. `focus` 위젯에 egui 키 포커스를 주고 Enter 를 넣는다.
fn enter_on_focused(ctx: &egui::Context, state: &mut RequestContext, focus: egui::Id) {
    ctx.memory_mut(|m| m.request_focus(focus));
    frame(ctx, state, Vec::new());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(focus),
        "위젯이 키 포커스를 가진다"
    );
    assert!(state.bind_window_key_owner());
    frame(ctx, state, vec![enter()]);
}

/// Given 다른 범위 팝업 X 가 키 주인이고 접힌 G 의 보기 버튼에 egui 키 포커스가 남았을 때, When Enter 가 오면, Then
/// G 는 펼쳐지지 않는다(키로 생긴 클릭은 키 주인 버튼만 받는다).
#[test]
fn enter_on_a_focused_collapsed_button_of_another_scope_does_not_expand_it() {
    let Scene {
        mut state,
        session: _session,
        ctx,
        g,
        x: _,
    } = open_scene(|_| {});
    // G 의 보기 버튼에 이를 때까지 Tab 으로 egui 키 포커스를 옮긴다.
    let button = state
        .popups
        .instance(g)
        .unwrap()
        .collapsed_frame()
        .expect("G 는 접힌 프레임이 있다")
        .rect;
    let mut focused = None;
    for _ in 0..8 {
        frame(&ctx, &mut state, vec![key(egui::Key::Tab)]);
        focused = ctx.memory(|m| m.focused()).filter(|id| {
            ctx.read_response(*id)
                .is_some_and(|r| button.contains_rect(r.rect))
        });
        if focused.is_some() {
            break;
        }
    }
    let focused = focused.expect("Tab 으로 G 의 보기 버튼이 키 포커스를 얻는다");
    enter_on_focused(&ctx, &mut state, focused);
    assert!(
        state.popups.instance(g).unwrap().collapsed(),
        "G 는 접힌 채다"
    );
}

/// Given 다른 범위 팝업 X 가 키 주인이고 펼친 G 의 제목줄 접기 버튼에 egui 키 포커스가 남았을 때, When Enter 가
/// 오면, Then G 는 다시 접히지 않는다.
#[test]
fn enter_on_a_focused_collapse_button_of_another_scope_does_not_refold_it() {
    let Scene {
        mut state,
        session: _session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    state.popups.expand(g);
    // 펼치면 G 가 포커스·맨 앞을 얻는다. 키 주인을 X 로 되돌린다.
    state.popups.set_instance_focused(x, true);
    frame(&ctx, &mut state, Vec::new());
    assert_eq!(
        state.popups.focused_dismissal_target().map(|t| t.1),
        Some(x)
    );
    assert!(state.popups.instance(g).unwrap().expanded());
    enter_on_focused(
        &ctx,
        &mut state,
        egui::Id::new("popup.collapse_button").with(g.0),
    );
    assert!(
        state.popups.instance(g).unwrap().expanded(),
        "G 는 펼친 채다"
    );
}

/// 팝업 층보다 먼저 그리는 화면(탐색기 찾기 입력란)과 팝업 층을 창의 한 프레임 순서대로 그린다
/// (`egui_bridge::run_egui_frame`: 화면은 `surfaces_take_keys` 문 안에서 그린다).
fn surface_frame(
    ctx: &egui::Context,
    state: &mut RequestContext,
    query: &mut String,
    events: Vec<egui::Event>,
    focus: bool,
) -> (tasty_ui_widgets::ExplorerFindEvents, bool) {
    let layout = layout();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events,
        ..Default::default()
    };
    let th = crate::theme::theme();
    let mut out = tasty_ui_widgets::ExplorerFindEvents::default();
    let mut escape_after = false;
    drop(ctx.run(raw, |ctx| {
        let take = state.surfaces_take_keys();
        super::body_keys::with_body_keys(ctx, take, || {
            egui::Area::new(egui::Id::new("explorer_find_test"))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_width(400.0);
                    out = tasty_ui_widgets::explorer_find_bar(
                        ui,
                        &th,
                        tasty_ui_widgets::ExplorerFindBar {
                            query,
                            subfolders: None,
                            status: tasty_ui_widgets::ExplorerFindStatus::Text(""),
                            labels: tasty_ui_widgets::ExplorerFindLabels {
                                placeholder: "",
                                subfolders: "",
                                close: "",
                            },
                            focus,
                        },
                    );
                });
        });
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(ctx, &mut |_, _| {}, Some(&layout), &[]);
        // 팝업 층 뒤에 그리는 것(plugin 팝업 렌더 등)이 볼 Esc 가 남았는지.
        escape_after = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        ..
                    }
                )
            })
        });
    }));
    (out, escape_after)
}

/// Given 탐색기 찾기 입력란에 egui 키 포커스와 검색어가 있을 때, When 창이 Esc 로 팝업 키 주인의 포커스를 풀거나
/// 닫으면, Then 찾기 입력란은 그 Esc 를 보지 않아 검색어·포커스가 그대로다.
#[test]
fn an_escape_the_popup_handled_does_not_reach_a_surface_field() {
    for closes in [false, true] {
        crate::i18n::init("en");
        let (mut state, mut session) = crate::state::tests::test_state();
        state.popups.register(
            PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
                .with_instances(PopupInstances::PerScopeTarget),
        );
        let ctx = egui::Context::default();
        let mut query = String::from("abc");
        // 처음 여는 찾기 바처럼 입력란에 egui 키 포커스를 준다.
        surface_frame(&ctx, &mut state, &mut query, Vec::new(), true);
        let (focused, _) = surface_frame(&ctx, &mut state, &mut query, Vec::new(), false);
        assert!(focused.field_focused, "입력란이 키 포커스를 얻는다");
        let x = open(&mut state, 3, true);
        {
            let p = state.popups.instance_mut(x).unwrap();
            p.close_on_outside_click = closes;
            p.size_user_overridden = true;
            p.set_needed_size(egui::vec2(240.0, 200.0));
        }
        let (before, _) = surface_frame(&ctx, &mut state, &mut query, Vec::new(), false);
        assert!(before.field_focused, "찾기 입력란이 키 포커스를 가진다");
        let mut events = Vec::new();
        assert!(!window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut events,
            key(egui::Key::Escape)
        ));
        {
            let mut engine = session.borrow_mut();
            for intent in state.take_pending_intents() {
                crate::intent::popup::handle(&mut state, &mut engine, &intent);
            }
        }
        // 같은 프레임에 이어 온 글자도 창이 묶은 키 주인 것이다. 찾기 입력란에 들어가지 않는다.
        assert!(!window_key(
            &ctx,
            &mut state,
            &session.borrow_mut().read(),
            &mut events,
            egui::Event::Text("z".into())
        ));
        let (after, escape_after) = surface_frame(&ctx, &mut state, &mut query, events, false);
        assert!(
            !escape_after,
            "팝업 층 뒤에 그리는 것은 그 Esc 를 보지 않는다(closes={closes})"
        );
        assert!(
            !after.escape,
            "찾기 입력란은 Esc 를 보지 않는다(closes={closes})"
        );
        assert!(after.field_focused, "포커스가 그대로다(closes={closes})");
        assert_eq!(query, "abc", "검색어가 그대로다(closes={closes})");
    }
}

fn escape_release() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// [`frame`]과 같은 창의 한 프레임. 본문마다 본 Escape 사건(인스턴스, 눌림, 반복)을 순서대로 돌려준다.
fn frame_escapes(
    ctx: &egui::Context,
    state: &mut RequestContext,
    events: Vec<egui::Event>,
) -> Vec<(PopupInstanceId, bool, bool)> {
    let layout = layout();
    let mut seen = Vec::new();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events,
        ..Default::default()
    };
    state
        .popups
        .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
    drop(ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(
            ctx,
            &mut |cx, ui| {
                ui.input(|i| {
                    for e in &i.events {
                        if let egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed,
                            repeat,
                            ..
                        } = e
                        {
                            seen.push((cx.instance, *pressed, *repeat));
                        }
                    }
                })
            },
            Some(&layout),
            &[],
        );
    }));
    state.close_input_pass(ctx);
    seen
}

/// Given 키 주인 X 와 다른 범위에 열린 G 가 있을 때, When 한 프레임에 Esc 누름 → 뗌 → 누름이 오면, Then 창은
/// 첫 누름으로 X 의 포커스를 풀어 X 에 넘기고 뗌은 egui 입력에 남긴다. 두 번째 누름은 이번 프레임 키 주인 X 것으로
/// 끝나 누구에게도 가지 않는다. X 본문은 첫 누름과 뗌을 그 순서로 보고, 다음 프레임에 Escape 가 눌린 채 남지 않는다.
#[test]
fn a_release_between_two_escapes_reaches_the_owner_after_the_first_press() {
    let Scene {
        mut state,
        mut session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(!state.popups.is_instance_focused(x));
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        escape_release()
    ));
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert_eq!(pending, [escape_release()], "누름 둘은 빠지고 뗌은 남는다");
    let seen = frame_escapes(&ctx, &mut state, pending);
    assert_eq!(
        seen,
        [(x, true, false), (x, false, false)],
        "X 본문만 첫 누름 → 뗌 순서로 본다"
    );
    assert!(
        state.popups.instance(g).unwrap().open && !state.popups.is_instance_focused(g),
        "G 는 열린 채고 포커스를 얻지 않는다"
    );
    assert!(!ctx.input(|i| i.key_down(egui::Key::Escape)));
    assert!(frame_escapes(&ctx, &mut state, Vec::new()).is_empty());
    assert!(
        !ctx.input(|i| i.key_down(egui::Key::Escape)),
        "다음 프레임에 Escape 가 눌린 채 남지 않는다"
    );
}

/// Given 팝업이 X 하나뿐일 때, When 창이 첫 Esc 누름으로 X 의 포커스를 풀어 X 에 넘기고 그 뗌이 다음 프레임에 터미널
/// 경로(egui 로 보내지 않음)로 가면, Then 그 뗌이 실린 몫의 패스를 닫을 때 egui 의 Escape 눌림 기록이 풀린다. X 에 다시 포커스를 주고 새로 누른 Esc 는
/// 반복이 아니다.
#[test]
fn an_escape_release_that_went_to_the_terminal_does_not_make_the_next_press_a_repeat() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    state.popups.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let x = open(&mut state, 1, true);
    {
        let p = state.popups.instance_mut(x).unwrap();
        p.close_on_outside_click = false;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(240.0, 200.0));
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, Vec::new());
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    let seen = frame_escapes(&ctx, &mut state, pending);
    assert_eq!(seen, [(x, true, false)]);
    assert!(
        ctx.input(|i| i.key_down(egui::Key::Escape)),
        "넘긴 누름은 egui 처럼 눌림으로 남는다"
    );

    let mut pending = Vec::new();
    assert!(!state.popups.has_focused(), "키 오버레이가 없다");
    window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        escape_release(),
    );
    assert!(pending.is_empty(), "그 뗌은 egui 로 가지 않는다");
    assert!(frame_escapes(&ctx, &mut state, pending).is_empty());
    assert!(
        !ctx.input(|i| i.key_down(egui::Key::Escape)),
        "그 뗌이 실린 몫의 패스를 닫으면 egui 의 Escape 눌림 기록이 풀린다"
    );

    state.popups.set_instance_focused(x, true);
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    let seen = frame_escapes(&ctx, &mut state, pending);
    assert_eq!(seen, [(x, true, false)], "새 누름은 반복이 아니다");
}

/// J5(시험용 덮는 팝업). Given 알림 목록이 열려 있고 창을 덮는 passive Window 범위 시험용 팝업이 그 위에 있을 때, When
/// 글자와 Esc 를 창의 키 경로로 보내면, Then 덮는 팝업이 포커스 노드라 글자와 첫 Esc 는 그 팝업 본문이 읽고 덮인 알림
/// 목록은 그 Esc 로 닫히지 않는다. 실제 파일 피커 본문으로 잰 같은 장면은
/// `adapters::ipc::handler::file_picker` 의 범위 시험(`file_picker_scope_tests.rs`)에 있다.
#[test]
fn a_covering_window_popup_takes_the_first_escape_before_an_open_notification_list() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    for def in super::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state.popups.register(PopupState::new(
        ID,
        "test",
        egui::vec2(200.0, 100.0),
        PopupScope::Window,
    ));
    state
        .popups
        .open_request(&OpenRequest::new("notifications", PopupScope::Window), true)
        .unwrap();
    let mut req = OpenRequest::new(ID, PopupScope::Window).centered();
    req.covers_scope = Some(true);
    let picker = state.popups.open_request(&req, false).unwrap();
    for p in state
        .popups
        .popups
        .iter_mut()
        .filter(|p| p.instance == picker)
    {
        p.close_on_outside_click = false;
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut state, Vec::new());
    let engine = session.borrow_mut();
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(picker)),
        "덮는 passive 팝업이 포커스 노드다"
    );

    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &engine.read(),
        &mut pending,
        egui::Event::Text("a".into())
    ));
    let layout = layout();
    let mut texts = Vec::new();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events: pending,
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(
            ctx,
            &mut |cx, ui| {
                let typed = ui.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Text(t) if t == "a"))
                });
                if typed {
                    texts.push(cx.instance);
                }
            },
            Some(&layout),
            &[],
        );
    }));
    assert_eq!(texts, [picker], "글자는 덮는 팝업 본문이 읽는다");

    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &engine.read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert_eq!(
        frame_reading(&ctx, &mut state, pending, egui::Key::Escape),
        [picker],
        "첫 Esc 는 덮는 팝업 본문이 읽는다"
    );
    assert!(
        state.popups.is_open("notifications"),
        "덮인 알림 목록은 그 Esc 로 닫히지 않는다"
    );
    let closes_notifications = state.pending_intents.iter().any(|i| {
        matches!(
            &i.body,
            crate::intent::Intent::Ui(crate::intent::UiIntent::ClosePopup {
                id: "notifications"
            })
        )
    });
    assert!(!closes_notifications, "알림 목록 닫기 요청이 없다");
}

/// [`frame_escapes`]와 같은 창의 한 프레임. 배치한 팝업을 다음 누름의 포인터 대상으로 남긴다(`frame::draw_popup_layer`
/// 처럼). 본문마다 본 Escape 사건(인스턴스, 눌림, 반복)을 돌려준다.
fn frame_escapes_with_hits(
    ctx: &egui::Context,
    state: &mut RequestContext,
    events: Vec<egui::Event>,
) -> Vec<(PopupInstanceId, bool, bool)> {
    let layout = layout();
    let mut seen = Vec::new();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events,
        ..Default::default()
    };
    let mut hits = Vec::new();
    state
        .popups
        .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
    drop(ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        let result = state.popups.draw(
            ctx,
            &mut |cx, ui| {
                ui.input(|i| {
                    for e in &i.events {
                        if let egui::Event::Key {
                            key: egui::Key::Escape,
                            pressed,
                            repeat,
                            ..
                        } = e
                        {
                            seen.push((cx.instance, *pressed, *repeat));
                        }
                    }
                })
            },
            Some(&layout),
            &[],
        );
        hits = result.hit_rects;
    }));
    state.host_popup_hittest = hits;
    state.close_input_pass(ctx);
    seen
}

/// (e) Given 키 주인 X 가 직전 패스에서 Esc 누름을 받아 egui 가 Escape 를 눌린 채로 볼 때, When 한 묶음에 [Esc 뗌, 다른
/// 범위 팝업 Y 누름, 새 Esc 누름]이 오면, Then 창은 Y 누름과 그 뒤 Esc 에서 끊고, 뗌을 실은 앞 패스 뒤의 패스에서 Y 가
/// 받는 새 Esc 는 반복이 아니다(뒤 몫의 반복 판정은 앞 몫의 뗌을 반영한 상태에서 시작한다).
#[test]
fn a_new_escape_after_a_press_that_cut_the_frame_is_not_a_repeat() {
    use crate::adapters::ui::popup::window_input_test_support::WindowInput;
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    state.popups.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let y = open(&mut state, 3, false);
    let x = open(&mut state, 1, true);
    for p in state.popups.popups.iter_mut() {
        p.close_on_outside_click = false;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(240.0, 200.0));
    }
    let ctx = egui::Context::default();
    frame_escapes_with_hits(&ctx, &mut state, Vec::new());
    let mut pending = Vec::new();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert_eq!(
        frame_escapes_with_hits(&ctx, &mut state, pending),
        [(x, true, false)]
    );
    assert!(
        ctx.input(|i| i.key_down(egui::Key::Escape)),
        "넘긴 누름은 egui 처럼 눌림으로 남는다"
    );
    state.popups.set_instance_focused(x, true);
    let y_at = state
        .host_popup_hittest
        .iter()
        .map(|o| o.rect)
        .find(|r| rect(600.0, 100.0, 600.0, 600.0).contains(r.center()))
        .expect("Y 가 그려진다")
        .center();

    let mut input = WindowInput::default();
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        input.queue_mut(),
        escape_release()
    ));
    {
        let engine = session.borrow_mut();
        input.press(&mut state, &engine.read(), y_at);
    }
    assert!(
        state.popups.is_instance_focused(y),
        "누름이 Y 로 포커스를 옮긴다"
    );
    {
        // 창은 누름 뒤 키를 받기 전에 그 키 자리에서 끊는다(`MainView::cut_input_segment`).
        let engine = session.borrow_mut();
        crate::view::main::input_segment::key_focus(&mut state, &engine.read(), &mut input);
    }
    assert!(!window_key(
        &ctx,
        &mut state,
        &session.borrow_mut().read(),
        input.queue_mut(),
        key(egui::Key::Escape)
    ));
    let first = input.take_pass(&mut state, &ctx);
    assert_eq!(
        frame_escapes_with_hits(&ctx, &mut state, first),
        [(x, false, false)],
        "앞 패스는 X 가 뗌을 본다"
    );
    assert!(!ctx.input(|i| i.key_down(egui::Key::Escape)));
    let press = input.take_pass(&mut state, &ctx);
    assert_eq!(frame_escapes_with_hits(&ctx, &mut state, press), []);
    let second = input.take_pass(&mut state, &ctx);
    assert_eq!(
        frame_escapes_with_hits(&ctx, &mut state, second),
        [(y, true, false)],
        "새 Esc 는 Y 가 받고 반복이 아니다"
    );
    assert!(input.is_empty());
}

/// [`frame`]과 같은 순서로 한 프레임을 그리고 팝업 포커스 막대 수를 센다.
fn focus_bars(ctx: &egui::Context, state: &mut RequestContext) -> usize {
    let layout = layout();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        ..Default::default()
    };
    state
        .popups
        .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
    let out = ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(ctx, &mut |_, _| {}, Some(&layout), &[]);
    });
    state.close_input_pass(ctx);
    super::focus_bar_count(&out)
}

/// 좁은 범위 키 주인은 자기 막대 하나를 켠다. 창 위로 펼친 팝업과 그 팝업이 연 자식이 키를 가지면 어느 팝업도
/// 막대를 그리지 않고 키 포커스 surface도 없어 창 안에 켜진 표지가 없다(정본 RULES 4절 "Shown over the window").
#[test]
fn a_popup_shown_over_the_window_and_its_child_light_nothing() {
    const CHILD: PopupId = "window_key_gate_test_child";
    let Scene {
        mut state,
        mut session,
        ctx,
        g,
        x,
    } = open_scene(|state| {
        state.popups.register(PopupState::new(
            CHILD,
            "test",
            egui::vec2(120.0, 80.0),
            PopupScope::Window,
        ));
    });
    assert_eq!(focus_bars(&ctx, &mut state), 1, "the narrow owner X");
    assert_eq!(state.key_focus_surface(&session.borrow_mut().read()), None);

    state.popups.close_instance(x, PopupCloseReason::Command);
    state.popups.expand(g);
    let mut request = OpenRequest::new(CHILD, PopupScope::Surface(2)).focused();
    request.parent = Some(g);
    let child = state.popups.open_request(&request, true).expect("child");
    focus_bars(&ctx, &mut state);
    assert!(state.popups.instance(g).unwrap().expanded());
    assert_eq!(
        state.popups.instance(child).unwrap().placement_scope(),
        PopupScope::Window,
        "the child is shown over the window with its parent"
    );
    assert_eq!(
        state.popups.focused_dismissal_target().map(|t| t.1),
        Some(child)
    );
    assert_eq!(focus_bars(&ctx, &mut state), 0, "nothing lit");
    assert_eq!(state.key_focus_surface(&session.borrow_mut().read()), None);
}
