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

/// Given R의 닫는 덮는 팝업이 열려 있고 포커스가 L일 때, When R 범위 안·상자 밖을 처음 누르면, Then 닫히지
/// 않고 그 팝업이 포커스를 받는다. When 포커스가 R인 채로 다시 누르면, Then 닫힌다.
#[test]
fn the_first_press_coming_from_another_scope_is_not_an_outside_click() {
    let mut mgr = manager(true, true);
    let iid = open_r(&mut mgr, false);
    primary(&mut mgr, r_outside(), L);
    assert_eq!(state(&mgr, iid).map(|s| s.0), Some(true));
    let result = primary(&mut mgr, r_outside(), R);
    assert_eq!(result.closed, [R_POPUP]);
    assert!(state(&mgr, iid).is_none());
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
