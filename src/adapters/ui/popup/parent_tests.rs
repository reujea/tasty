//! 어느 host 팝업 인스턴스든 자식을 가질 수 있다. 자식은 부모의 범위를 따르고, 열린 자식이 있는 부모는
//! 바깥 클릭으로 닫히지 않는다. 부모가 닫히면 자식도 닫힌다.

use super::*;
use crate::intent::OpenRequest;

const PARENT_ID: PopupId = "parent_test_parent";
const CHILD_ID: PopupId = "parent_test_child";

fn manager() -> PopupManager {
    let mut mgr = PopupManager::new();
    for id in [PARENT_ID, CHILD_ID] {
        mgr.register(
            PopupState::new(id, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
                .with_close_on_outside_click(true)
                .with_instances(PopupInstances::PerScopeTarget),
        );
    }
    mgr
}

fn open_child(
    mgr: &mut PopupManager,
    parent: PopupInstanceId,
    scope: PopupScope,
) -> PopupInstanceId {
    let mut request = OpenRequest::new(CHILD_ID, scope).at(egui::pos2(520.0, 520.0));
    request.parent = Some(parent);
    mgr.open_request(&request, true).expect("child opens")
}

fn click_far_outside(mgr: &mut PopupManager) {
    let ctx = egui::Context::default();
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    raw.events.push(egui::Event::PointerButton {
        pos: egui::pos2(10.0, 10.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    drop(ctx.run(raw, |ctx| {
        mgr.draw(ctx, &mut |_, _| {}, None, &[]);
    }));
}

/// Given Surface(3) 범위의 부모, When 요청 범위가 Window 인 자식을 그 부모로 열면, Then 자식은 Surface(3) 범위다.
#[test]
fn a_child_takes_the_scope_of_its_parent() {
    let mut mgr = manager();
    let parent = mgr
        .open_request(&OpenRequest::new(PARENT_ID, PopupScope::Surface(3)), true)
        .unwrap();
    let child = open_child(&mut mgr, parent, PopupScope::Window);
    assert_eq!(mgr.instance(child).unwrap().scope, PopupScope::Surface(3));
    assert_eq!(mgr.instance(child).unwrap().parent, Some(parent));
}

/// 부모가 이미 닫혔으면 요청 범위를 그대로 쓴다.
#[test]
fn a_child_of_a_closed_parent_keeps_the_requested_scope() {
    let mut mgr = manager();
    let parent = mgr
        .open_request(&OpenRequest::new(PARENT_ID, PopupScope::Surface(3)), true)
        .unwrap();
    mgr.close_instance(parent, PopupCloseReason::Command);
    let child = open_child(&mut mgr, parent, PopupScope::Pane(7));
    assert_eq!(mgr.instance(child).unwrap().scope, PopupScope::Pane(7));
}

/// Given 바깥 클릭에 닫히는 부모와 자식, When 둘 다의 바깥을 누르면, Then 자식만 닫히고 부모는 남는다.
/// When 한 번 더 누르면, Then 자식이 없는 부모가 닫힌다.
#[test]
fn an_outside_click_closes_the_child_but_keeps_its_parent() {
    let mut mgr = manager();
    let mut request = OpenRequest::new(PARENT_ID, PopupScope::Window).at(egui::pos2(500.0, 500.0));
    request.focus = true;
    let parent = mgr.open_request(&request, true).unwrap();
    let child = open_child(&mut mgr, parent, PopupScope::Window);

    click_far_outside(&mut mgr);
    let closed: Vec<_> = mgr
        .take_closed_queue()
        .into_iter()
        .map(|c| (c.instance, c.reason))
        .collect();
    assert_eq!(closed, [(child, PopupCloseReason::OutsideClick)]);
    assert!(mgr.instance(parent).is_some(), "부모는 남는다");

    click_far_outside(&mut mgr);
    let closed: Vec<_> = mgr
        .take_closed_queue()
        .into_iter()
        .map(|c| (c.instance, c.reason))
        .collect();
    assert_eq!(closed, [(parent, PopupCloseReason::OutsideClick)]);
}
