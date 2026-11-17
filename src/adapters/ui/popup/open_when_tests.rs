//! 상태 술어가 여닫는 팝업은 술어 값이 바뀔 때만 열고 닫는다. 연 팝업은 포커스 없이 passive 다.

use super::*;

const DRIVEN_ID: PopupId = "open_when_test";

fn manager() -> PopupManager {
    let mut mgr = PopupManager::new();
    mgr.register(PopupState::new(
        DRIVEN_ID,
        "test",
        egui::vec2(200.0, 100.0),
        PopupScope::Window,
    ));
    mgr
}

fn shown_scope(mgr: &PopupManager) -> Option<PopupScope> {
    mgr.instances_of(DRIVEN_ID).next().map(|p| p.scope.clone())
}

/// Given 술어가 Workspace(1) 을 돌려줌, Then 포커스 없이 연다. When 같은 값이 다시 오면 Then 그대로다.
/// When Workspace(2) 가 오면 Then 그 범위로 옮긴다. When None 이 오면 Then 닫는다.
#[test]
fn the_popup_follows_the_predicate_and_opens_without_focus() {
    let mut mgr = manager();
    mgr.follow_open_when(DRIVEN_ID, Some(PopupScope::Workspace(1)));
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(1)));
    assert!(mgr.is_passive(DRIVEN_ID));
    assert!(!mgr.is_focused(DRIVEN_ID));

    mgr.follow_open_when(DRIVEN_ID, Some(PopupScope::Workspace(2)));
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(2)));

    mgr.follow_open_when(DRIVEN_ID, None);
    assert!(!mgr.is_open(DRIVEN_ID));
    assert_eq!(mgr.take_closed_queue().len(), 1);
}

/// Given 술어로 연 팝업을 다른 경로로 닫음, When 같은 술어 값이 계속 오면, Then 다시 열지 않는다.
/// When 값이 바뀌면 Then 다시 연다.
#[test]
fn a_popup_closed_elsewhere_stays_closed_until_the_predicate_changes() {
    let mut mgr = manager();
    mgr.follow_open_when(DRIVEN_ID, Some(PopupScope::Workspace(1)));
    mgr.close(DRIVEN_ID);
    mgr.follow_open_when(DRIVEN_ID, Some(PopupScope::Workspace(1)));
    assert!(!mgr.is_open(DRIVEN_ID));

    mgr.follow_open_when(DRIVEN_ID, Some(PopupScope::Workspace(2)));
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(2)));
}
