//! 창의 키 처리(Esc 판정 `escape_gate`, 키 입력의 입력 시점 결속 `bind_window_key_owner`)를 거친 키를 실은 프레임은
//! 창이 정한 키 주인 하나만 그 키를 소비한다. 본문, 접힌 보기 버튼의 Enter·Space, 접기 버튼이 모두 그 값을 쓴다.

use super::*;
use crate::adapters::ui::LayoutContext;
use crate::intent::OpenRequest;
use crate::state::{EscapeGate, RequestContext};

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
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !seen.contains(&cx.instance) {
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
    _session: crate::runtime::engine_session::EngineSession,
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
        _session: session,
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
        _session,
        ctx,
        g,
        x,
    } = open_scene(|_| {});
    assert_eq!(
        state.escape_gate(),
        EscapeGate::Consumed {
            drop_from_egui: false
        }
    );
    assert!(
        !state.popups.instance(x).unwrap().focused,
        "창이 X 의 포커스를 푼다"
    );
    let seen = frame(&ctx, &mut state, vec![key(egui::Key::Escape), enter()]);
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
        _session,
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
