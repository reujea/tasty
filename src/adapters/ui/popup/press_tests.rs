//! 누름 하나를 범위 경로로 나누는 규칙을 관리자 층에서 확인한다. L(surface 1)과 R(surface 2)을 나란히 둔다.

use super::super::*;
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::layout_context::SurfaceOwner;
use crate::adapters::ui::popup::route::{RoutedPress, path_at};
use crate::intent::OpenRequest;

const R_POPUP: PopupId = "press_test_r";
const WIN_POPUP: PopupId = "press_test_window";
const L: u32 = 1;
const R: u32 = 2;

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
}

fn l_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 600.0))
}

fn r_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(400.0, 600.0))
}

fn layout() -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![(10, l_rect()), (20, r_rect())],
        surface_rects: vec![(L, l_rect()), (R, r_rect())],
        active_tabs: vec![],
        active_tab_ids: vec![(10, 100), (20, 200)],
        workspace_rect: Some(screen()),
        surface_owners: vec![
            SurfaceOwner {
                surface: L,
                tab: 100,
                pane: 10,
            },
            SurfaceOwner {
                surface: R,
                tab: 200,
                pane: 20,
            },
        ],
        tab_content_rects: vec![(10, l_rect()), (20, r_rect())],
    }
}

/// 팝업 상자 위치. R 안 위쪽에 둔다. 범위 안·상자 밖 자리는 `r_outside()`다.
fn r_box_at() -> egui::Pos2 {
    egui::pos2(450.0, 40.0)
}

fn r_outside() -> egui::Pos2 {
    egui::pos2(700.0, 500.0)
}

fn l_point() -> egui::Pos2 {
    l_rect().center()
}

/// `covers`·`close`를 정한 R 범위 팝업과 창 범위 팝업을 등록한 관리자.
fn manager(covers: bool, close: bool) -> PopupManager {
    let mut mgr = PopupManager::new();
    let mut r = PopupState::new(R_POPUP, "r", egui::vec2(200.0, 100.0), PopupScope::Window)
        .with_instances(PopupInstances::PerScopeTarget);
    r.look = r.look.covering(covers);
    r.declared_look = r.look;
    r.close_on_outside_click = close;
    mgr.register(r);
    let mut w = PopupState::new(WIN_POPUP, "w", egui::vec2(200.0, 100.0), PopupScope::Window);
    w.look = w.look.covering(true);
    w.declared_look = w.look;
    w.close_on_outside_click = true;
    w.sticky_focus = true;
    mgr.register(w);
    mgr
}

fn open_r(mgr: &mut PopupManager, focused: bool) -> PopupInstanceId {
    let mut req = OpenRequest::new(R_POPUP, PopupScope::Surface(R)).at(r_box_at());
    if focused {
        req = req.focused();
    }
    mgr.open_request(&req, focused).unwrap()
}

/// 누르기 직전 포커스가 `focus`(surface)였을 때 `pos`를 `button`으로 누른 프레임 하나.
fn press(
    mgr: &mut PopupManager,
    pos: egui::Pos2,
    focus: u32,
    button: egui::PointerButton,
) -> PopupDrawResult {
    let layout = layout();
    let focus_point = if focus == L { l_point() } else { r_outside() };
    let routed = RoutedPress {
        pos,
        path: path_at(&layout, pos),
        focus_before: path_at(&layout, focus_point),
        button,
    };
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events: vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    let mut result = None;
    drop(ctx.run(raw, |ctx| {
        result = Some(mgr.draw_routed(ctx, &mut |_, _| {}, Some(&layout), &[], Some(&routed)));
    }));
    result.unwrap()
}

fn primary(mgr: &mut PopupManager, pos: egui::Pos2, focus: u32) -> PopupDrawResult {
    press(mgr, pos, focus, egui::PointerButton::Primary)
}

fn state(mgr: &PopupManager, iid: PopupInstanceId) -> Option<(bool, egui::Pos2, u64)> {
    mgr.instance(iid)
        .filter(|p| p.open)
        .map(|p| (p.focused, p.pos, p.z_seq))
}

/// Given R의 덮는·바깥 클릭 닫기 팝업이 포커스를 가졌을 때, When L을 누르면, Then 열림·포커스·위치·순번이
/// 그대로다(범위 밖 누름은 무반응).
#[test]
fn a_press_outside_the_scope_leaves_the_popup_alone() {
    let mut mgr = manager(true, true);
    let iid = open_r(&mut mgr, true);
    let before = state(&mgr, iid);
    primary(&mut mgr, l_point(), L);
    assert_eq!(state(&mgr, iid), before);
}

/// Given R의 덮지 않는 팝업(바깥 클릭은 포커스 해제)이 포커스일 때, When L을 누르면, Then 바뀌지 않는다.
/// When R 본문을 누르면, Then 열린 채 포커스만 잃는다.
#[test]
fn a_non_covering_popup_loses_focus_only_to_a_press_in_its_scope() {
    let mut mgr = manager(false, false);
    let iid = open_r(&mut mgr, true);
    primary(&mut mgr, l_point(), L);
    assert_eq!(state(&mgr, iid).map(|s| s.0), Some(true));
    primary(&mut mgr, r_outside(), R);
    assert_eq!(state(&mgr, iid).map(|s| s.0), Some(false));
}

/// Given 창을 덮는 닫기 팝업(명령 팔레트 꼴)이 열려 있을 때, When L이나 R을 누르면, Then 닫힌다.
#[test]
fn a_window_popup_closes_on_a_press_anywhere() {
    for point in [l_point(), r_outside()] {
        let mut mgr = manager(true, true);
        mgr.open_request(
            &OpenRequest::new(WIN_POPUP, PopupScope::Window).focused(),
            true,
        )
        .unwrap();
        let result = primary(&mut mgr, point, L);
        assert_eq!(result.closed, [WIN_POPUP], "{point:?}");
    }
}

/// Given 덮고 바깥 클릭이 포커스 해제인 R 팝업이 포커스일 때, When R 범위 안·상자 밖을 누르면, Then 포커스를
/// 유지한다(D-b).
#[test]
fn a_covering_popup_that_does_not_close_keeps_the_focus() {
    let mut mgr = manager(true, false);
    let iid = open_r(&mut mgr, true);
    primary(&mut mgr, r_outside(), R);
    assert_eq!(state(&mgr, iid).map(|s| s.0), Some(true));
}

/// Given passive로 열린 덮는 R 팝업일 때, When R 범위 안·상자 밖을 누르면, Then 팝업이 포커스를 받고 passive가
/// 풀린다.
#[test]
fn a_press_in_the_scope_of_a_passive_covering_popup_focuses_it() {
    let mut mgr = manager(true, false);
    let iid = open_r(&mut mgr, false);
    assert!(mgr.instance(iid).unwrap().passive);
    primary(&mut mgr, r_outside(), L);
    let p = mgr.instance(iid).unwrap();
    assert!(p.focused && !p.passive);
}

/// Given L과 R에 같은 선언의 팝업 A·B가 열려 B가 포커스일 때, When A 안을 누르면, Then B의 포커스·위치는
/// 그대로다(다른 범위 경로의 팝업은 건드리지 않는다).
#[test]
fn a_press_inside_one_popup_leaves_a_popup_in_another_scope_focused() {
    let mut mgr = manager(false, false);
    let a = mgr
        .open_request(
            &OpenRequest::new(R_POPUP, PopupScope::Surface(L))
                .at(egui::pos2(50.0, 40.0))
                .focused(),
            true,
        )
        .unwrap();
    let mut other = PopupState::new(
        "press_test_other",
        "o",
        egui::vec2(200.0, 100.0),
        PopupScope::Window,
    )
    .with_instances(PopupInstances::PerScopeTarget);
    other.look = other.look.covering(false);
    mgr.register(other);
    let b = mgr
        .open_request(
            &OpenRequest::new("press_test_other", PopupScope::Surface(R))
                .at(r_box_at())
                .focused(),
            true,
        )
        .unwrap();
    let before = state(&mgr, b);
    primary(&mut mgr, egui::pos2(60.0, 60.0), L);
    assert_eq!(state(&mgr, a).map(|s| s.0), Some(true));
    assert_eq!(
        state(&mgr, b).map(|s| (s.0, s.1)),
        before.map(|s| (s.0, s.1))
    );
}

/// Given R의 닫는 덮는 팝업일 때, When 오른쪽·가운데 버튼으로 L을 누르면, Then 그대로다. When 포커스가 R인 채로
/// R 범위 안을 누르면, Then 닫힌다(왼쪽 버튼과 같은 판정).
#[test]
fn secondary_and_middle_presses_follow_the_same_routing() {
    for button in [egui::PointerButton::Secondary, egui::PointerButton::Middle] {
        let mut mgr = manager(true, true);
        let iid = open_r(&mut mgr, true);
        press(&mut mgr, l_point(), L, button);
        assert!(state(&mgr, iid).is_some(), "{button:?}");
        let result = press(&mut mgr, r_outside(), R, button);
        assert_eq!(result.closed, [R_POPUP], "{button:?}");
    }
}

/// Given 창을 덮는 팝업이 R 팝업 위에 열려 있을 때, When R 범위 안을 누르면, Then 덮인 R 팝업은 바깥 누름을
/// 받지 않고 위 팝업만 닫힌다.
#[test]
fn a_covered_popup_gets_no_outside_press() {
    let mut mgr = manager(true, true);
    let iid = open_r(&mut mgr, true);
    mgr.open_request(
        &OpenRequest::new(WIN_POPUP, PopupScope::Window).focused(),
        true,
    )
    .unwrap();
    let result = primary(&mut mgr, r_outside(), R);
    assert_eq!(result.closed, [WIN_POPUP]);
    assert!(state(&mgr, iid).is_some());
}

/// Given R을 덮는 팝업이 있을 때, Then 그 덮는 사각형이 결과에 담겨 포인터 대상 찾기가 쓴다.
#[test]
fn the_cover_of_a_covering_popup_is_reported() {
    let mut mgr = manager(true, false);
    open_r(&mut mgr, true);
    let result = primary(&mut mgr, l_point(), L);
    assert_eq!(
        result.covers.iter().map(|c| c.rect).collect::<Vec<_>>(),
        [r_rect()]
    );
}

/// 덮는 팝업과 아래 버튼 하나를 같은 프레임에 그리고, 그 버튼이 눌렸는지 돌려준다.
fn frame_with_button(
    mgr: &mut PopupManager,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    button_rect: egui::Rect,
) -> bool {
    let layout = layout();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let mut clicked = false;
    drop(ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            clicked = ui.put(button_rect, egui::Button::new("under")).clicked();
        });
        mgr.draw(ctx, &mut |_, _| {}, Some(&layout), &[]);
    }));
    clicked
}

/// Given R을 덮는 팝업이 열려 있을 때, When R 범위 안·상자 밖의 아래 위젯을 누르고 떼면, Then 그 위젯은
/// 눌리지 않는다. L의 위젯은 눌린다(창 범위의 egui 위젯도 덮인 범위에서는 포인터를 받지 않는다).
#[test]
fn a_widget_under_a_cover_gets_no_click() {
    let mut mgr = manager(true, false);
    open_r(&mut mgr, true);
    let ctx = egui::Context::default();
    let click = |pos: egui::Pos2| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let under_r = egui::Rect::from_center_size(r_outside(), egui::vec2(80.0, 30.0));
    let under_l = egui::Rect::from_center_size(l_point(), egui::vec2(80.0, 30.0));
    // 첫 프레임은 레이어를 만든다. egui는 직전 프레임 레이어로 판정한다.
    frame_with_button(&mut mgr, &ctx, vec![], under_r);
    assert!(!frame_with_button(
        &mut mgr,
        &ctx,
        click(r_outside()),
        under_r
    ));
    frame_with_button(&mut mgr, &ctx, vec![], under_l);
    assert!(frame_with_button(&mut mgr, &ctx, click(l_point()), under_l));
}

/// Given R을 덮는 팝업, When 제목줄을 눌러 끌고 놓으면, Then 팝업이 옮겨진다(덮는 범위를 막는 레이어가
/// 콘텐츠 밖 상자 자리의 끌기를 가져가지 않는다).
#[test]
fn a_covering_popup_still_moves_by_its_title_bar() {
    let mut mgr = manager(true, false);
    let iid = open_r(&mut mgr, true);
    let ctx = egui::Context::default();
    let spare = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame_with_button(&mut mgr, &ctx, vec![], spare);
    let start = mgr.instance(iid).unwrap().pos;
    let grab = start + egui::vec2(60.0, 5.0);
    let to = grab + egui::vec2(-30.0, 40.0);
    frame_with_button(
        &mut mgr,
        &ctx,
        vec![egui::Event::PointerMoved(grab), button(grab, true)],
        spare,
    );
    frame_with_button(&mut mgr, &ctx, vec![egui::Event::PointerMoved(to)], spare);
    frame_with_button(
        &mut mgr,
        &ctx,
        vec![egui::Event::PointerMoved(to), button(to, false)],
        spare,
    );
    assert_eq!(
        mgr.instance(iid).unwrap().pos,
        start + egui::vec2(-30.0, 40.0)
    );
}

/// host·plugin 공용 판정: 같은 입력이면 인스턴스 종류와 무관하게 같은 결과다. 열린 자식이 있으면 닫는 대신
/// 덮는 팝업은 포커스를 받고 덮지 않는 팝업은 포커스를 푼다.
#[test]
fn one_outside_press_rule_decides_for_any_popup() {
    use super::{OutsidePress, OutsideTarget, outside_press};
    let layout = layout();
    let r_scope = PopupScope::Surface(R);
    let routed = |pos: egui::Pos2, focus: egui::Pos2| RoutedPress {
        pos,
        path: path_at(&layout, pos),
        focus_before: path_at(&layout, focus),
        button: egui::PointerButton::Primary,
    };
    let target = |cover: bool, child: bool| OutsideTarget {
        placement_scope: &r_scope,
        logical_scope: &r_scope,
        z_seq: 5,
        box_rect: Some(egui::Rect::from_min_size(
            r_box_at(),
            egui::vec2(200.0, 100.0),
        )),
        cover_rect: cover.then_some(r_rect()),
        closes_on_outside_click: true,
        has_open_child: child,
    };
    let second = routed(r_outside(), r_outside());
    let first = routed(r_outside(), l_point());
    assert_eq!(
        outside_press(&target(true, false), &second, &[]),
        OutsidePress::Close
    );
    assert_eq!(
        outside_press(&target(true, false), &first, &[]),
        OutsidePress::Focus
    );
    assert_eq!(
        outside_press(&target(false, false), &first, &[]),
        OutsidePress::None
    );
    assert_eq!(
        outside_press(&target(true, true), &second, &[]),
        OutsidePress::Focus
    );
    assert_eq!(
        outside_press(&target(false, true), &second, &[]),
        OutsidePress::Unfocus
    );
    // 상자 안, 범위 밖, 위 팝업에 덮인 자리는 아무 일도 없다.
    let in_box = routed(r_box_at() + egui::vec2(1.0, 1.0), r_outside());
    assert_eq!(
        outside_press(&target(true, false), &in_box, &[]),
        OutsidePress::None
    );
    let in_l = routed(l_point(), r_outside());
    assert_eq!(
        outside_press(&target(true, false), &in_l, &[]),
        OutsidePress::None
    );
    let higher = [super::Occluder {
        rect: r_rect(),
        z_seq: 6,
    }];
    assert_eq!(
        outside_press(&target(true, false), &second, &higher),
        OutsidePress::None
    );
}
