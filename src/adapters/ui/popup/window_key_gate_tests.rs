//! 창의 키 처리(egui 전달 게이트 `window_feeds_egui`, Esc 처리 `window_escape`, 터미널 게이트
//! `window_holds_terminal_keys`)를 거친 키를 실은 프레임은 창이 정한 키 주인 하나만 그 키를 소비한다. 본문, 접힌
//! 보기 버튼의 Enter·Space, 접기 버튼이 모두 그 값을 쓴다. 창 호출부가 이 함수들을 그대로 쓰는지는
//! `source_guards::window_key_gate_call_sites` 가 본다.

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
/// 판정 함수로 밟는다. egui 로 보낼 키만 `pending`(그 키를 실은 프레임의 egui 입력)에 싣고, Esc 는 창의 Esc 처리를
/// 거친다. 터미널(PTY)로 가면 참이다.
fn window_key(
    state: &mut RequestContext,
    pending: &mut Vec<egui::Event>,
    event: egui::Event,
) -> bool {
    if state.window_feeds_egui(false, true).is_some() {
        pending.push(event.clone());
    }
    if super::body_keys::is_escape(&event) && state.window_escape(pending) {
        return false;
    }
    !state.window_holds_terminal_keys()
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
    let layout = layout();
    let mut seen = Vec::new();
    let raw = egui::RawInput {
        screen_rect: Some(rect(0.0, 0.0, 1200.0, 700.0)),
        events,
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        state.popups.draw(
            ctx,
            &mut |cx, ui| {
                if ui.input(|i| i.key_pressed(read)) && !seen.contains(&cx.instance) {
                    seen.push(cx.instance);
                }
            },
            Some(&layout),
            &[],
        );
    }));
    seen
}

struct Scene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    ctx: egui::Context,
    /// surface 2 범위의 접힌 팝업. 포커스를 가진다.
    g: PopupInstanceId,
    /// surface 1 범위의 키 주인(맨 위, 포커스). 바깥 클릭에 닫히지 않는다.
    x: PopupInstanceId,
}

fn open(state: &mut RequestContext, scope: u32, focused: bool) -> PopupInstanceId {
    let req = OpenRequest::new(ID, PopupScope::Surface(scope));
    let req = if focused { req.focused() } else { req };
    state.popups.open_request(&req, focused).unwrap()
}

/// G(surface 2, 접힘, 포커스) → X(surface 1, 포커스) → `extra` 순서로 연다. X 가 키 주인이다.
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
    assert!(gp.collapsed() && gp.focused, "G 는 접힌 채 포커스를 가진다");
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

/// Given 키 주인 X 와 다른 범위에 포커스를 가진 접힌 G 가 있을 때, When 창이 Esc 로 X 의 포커스를 풀고 같은 프레임에
/// Enter 가 함께 오면, Then X 본문 하나만 Enter 를 읽고 G 의 접힌 보기 버튼은 Enter 로 펼쳐지지 않는다.
#[test]
fn a_key_after_the_window_released_the_owner_reaches_only_that_owner() {
    let Scene {
        mut state,
        session: _session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    let mut pending = Vec::new();
    assert!(!window_key(
        &mut state,
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(
        !state.popups.instance(x).unwrap().focused,
        "창이 X 의 포커스를 푼다"
    );
    assert!(
        !window_key(&mut state, &mut pending, enter()),
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
        state.popups.instance(a).unwrap().focused,
        "누름이 A 에 포커스를 준다"
    );
    assert_eq!(seen, [x], "Enter 는 X 본문만 읽는다");
    assert!(state.popups.instance(g).unwrap().collapsed());

    assert!(state.bind_window_key_owner());
    let seen = frame(&ctx, &mut state, vec![enter()]);
    assert_eq!(seen, [a], "다음 키부터 A 가 받는다");
}

/// Given 키 주인 X 가 바깥 클릭에 닫히는 팝업이고 포커스를 받지 않는 팝업 N(복원 안내 같은)이 함께 보일 때, When
/// 창이 Esc 로 X 를 닫고 그 닫기가 그리기 전에 적용되면, Then 그 Esc 는 처리한 X 에서 멈춘다. 키 주인이 사라졌어도
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
        state.popups.instance_mut(iid).unwrap().arrange.takes_focus = false;
        n = Some(iid);
    });
    let n = n.unwrap();
    state.popups.instance_mut(x).unwrap().close_on_outside_click = true;
    assert!(frame_reading(&ctx, &mut state, Vec::new(), egui::Key::Escape).is_empty());
    let mut pending = Vec::new();
    assert!(!window_key(
        &mut state,
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(pending.is_empty(), "닫은 주인의 Esc 는 egui 입력에서 뺀다");
    // 창의 닫기 요청은 그리기 전에 적용된다. 아래에서는 Esc 가 egui 에 남았다고 쳐도 N 이 읽지 않는지 본다.
    {
        let mut engine = session.borrow_mut();
        for intent in state.take_pending_intents() {
            crate::intent::popup::handle(&mut state, &mut engine, &intent);
        }
    }
    assert!(
        !state.popups.instance(x).is_some_and(|p| p.open),
        "창이 X 를 닫는다"
    );
    let seen = frame_reading(
        &ctx,
        &mut state,
        vec![key(egui::Key::Escape)],
        egui::Key::Escape,
    );
    assert!(
        seen.is_empty(),
        "아래 N({n:?})은 그 Esc 를 읽지 않는다: {seen:?}"
    );
}

/// Given 키 주인 X 와 다른 범위에 포커스를 가진 G(바깥 클릭에 닫힘)가 있을 때, When 그리기 전에 Esc 가 두 번 오면,
/// Then 창은 첫 Esc 로 X 만 처리하고 두 번째 Esc 는 이번 프레임 키 주인 X 것으로 끝낸다. G 는 포커스와 열림을
/// 그대로 둔다.
#[test]
fn a_second_escape_before_the_draw_stays_with_the_bound_owner() {
    let Scene {
        mut state,
        session: _session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    state.popups.instance_mut(g).unwrap().close_on_outside_click = true;
    let mut pending = Vec::new();
    assert!(!window_key(
        &mut state,
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(!state.popups.instance(x).unwrap().focused);
    assert!(
        !window_key(&mut state, &mut pending, key(egui::Key::Escape)),
        "두 번째 Esc 도 X 것이다"
    );
    assert!(state.take_pending_intents().is_empty(), "G 를 닫지 않는다");
    let gp = state.popups.instance(g).unwrap();
    assert!(gp.open && gp.focused, "G 는 포커스·열림 그대로다");
    assert!(!window_key(&mut state, &mut pending, enter()));
    assert_eq!(pending, [enter()], "두 Esc 는 egui 입력에서 빠진다");
    let seen = frame(&ctx, &mut state, pending);
    assert_eq!(seen, [x], "그 프레임 키는 X 본문만 읽는다");
}

/// Given 팝업이 X 하나뿐일 때, When 창이 Esc 로 X 의 포커스를 푼 뒤 같은 프레임에 Enter 가 오면, Then 창의 터미널
/// 게이트(`keyboard_overlay_open`)는 그 프레임 동안 닫혀 있어 Enter 가 터미널로 가지 않는다. 그 프레임을 그린 뒤에는
/// 열린다.
#[test]
fn a_key_after_the_window_released_the_only_popup_does_not_reach_the_terminal() {
    crate::i18n::init("en");
    let (mut state, _session) = crate::state::tests::test_state();
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
        &mut state,
        &mut pending,
        key(egui::Key::Escape)
    ));
    assert!(!state.popups.has_focused(), "X 의 포커스가 풀렸다");
    assert!(
        !window_key(&mut state, &mut pending, enter()),
        "같은 프레임의 Enter 는 터미널로 가지 않는다"
    );
    assert_eq!(pending, [enter()], "그 Enter 는 egui 로 간다");
    let seen = frame(&ctx, &mut state, pending);
    assert_eq!(seen, [x], "그 Enter 는 X 본문이 읽는다");
    let mut pending = Vec::new();
    assert!(
        window_key(&mut state, &mut pending, enter()),
        "다음 키부터 터미널로 간다"
    );
    assert!(pending.is_empty(), "다음 키는 egui 로 가지 않는다");
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
    state.popups.instance_mut(g).unwrap().focused = false;
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
        assert!(!window_key(&mut state, &mut events, key(egui::Key::Escape)));
        {
            let mut engine = session.borrow_mut();
            for intent in state.take_pending_intents() {
                crate::intent::popup::handle(&mut state, &mut engine, &intent);
            }
        }
        // 같은 프레임에 이어 온 글자도 창이 묶은 키 주인 것이다. 찾기 입력란에 들어가지 않는다.
        assert!(!window_key(
            &mut state,
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
