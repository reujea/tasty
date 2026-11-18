//! 상태 술어가 여닫는 팝업은 술어 값이 바뀔 때만 열고 닫는다. 연 팝업은 포커스 없이 passive 다.

use super::open_when::DrivenOpen;
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
    mgr.follow_open_when(
        DRIVEN_ID,
        Some(DrivenOpen::at(PopupScope::Workspace(1))),
        |_| PopupPayload::None,
    );
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(1)));
    assert!(mgr.is_passive(DRIVEN_ID));
    assert!(!mgr.is_focused(DRIVEN_ID));

    mgr.follow_open_when(
        DRIVEN_ID,
        Some(DrivenOpen::at(PopupScope::Workspace(2))),
        |_| PopupPayload::None,
    );
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(2)));

    mgr.follow_open_when(DRIVEN_ID, None, |_| PopupPayload::None);
    assert!(!mgr.is_open(DRIVEN_ID));
    assert_eq!(mgr.take_closed_queue().len(), 1);
}

/// Given 술어로 연 팝업을 다른 경로로 닫음, When 같은 술어 값이 계속 오면, Then 다시 열지 않는다.
/// When 값이 바뀌면 Then 다시 연다.
#[test]
fn a_popup_closed_elsewhere_stays_closed_until_the_predicate_changes() {
    let mut mgr = manager();
    mgr.follow_open_when(
        DRIVEN_ID,
        Some(DrivenOpen::at(PopupScope::Workspace(1))),
        |_| PopupPayload::None,
    );
    mgr.close(DRIVEN_ID);
    mgr.follow_open_when(
        DRIVEN_ID,
        Some(DrivenOpen::at(PopupScope::Workspace(1))),
        |_| PopupPayload::None,
    );
    assert!(!mgr.is_open(DRIVEN_ID));

    mgr.follow_open_when(
        DRIVEN_ID,
        Some(DrivenOpen::at(PopupScope::Workspace(2))),
        |_| PopupPayload::None,
    );
    assert_eq!(shown_scope(&mgr), Some(PopupScope::Workspace(2)));
}

/// Given 술어가 덮지 않는 모양으로 연 팝업, When 같은 범위에서 덮는 모양으로 바뀌면, Then 앞 인스턴스를 닫고
/// 덮는 인스턴스를 새로 연다. 덮는 모양은 modal 그림자와 포인터를 받는 scrim 이고 덮지 않는 모양은 그 반대다.
#[test]
fn a_changed_cover_closes_the_shown_instance_and_opens_the_other_look() {
    let mut mgr = manager();
    let open = |covers| DrivenOpen {
        scope: PopupScope::Workspace(1),
        covers_scope: Some(covers),
    };
    mgr.follow_open_when(DRIVEN_ID, Some(open(false)), |_| PopupPayload::None);
    let bare = mgr.instances_of(DRIVEN_ID).next().unwrap();
    let bare_id = bare.instance;
    assert!(!bare.look.covers_scope);
    assert_eq!(bare.look.shadow, PopupShadow::Popover);
    assert_eq!(bare.look.scrim_input, ScrimInput::VisualOnly);

    mgr.follow_open_when(DRIVEN_ID, Some(open(true)), |_| PopupPayload::None);
    let closed = mgr.take_closed_queue();
    assert_eq!(
        closed.iter().map(|c| c.instance).collect::<Vec<_>>(),
        [bare_id]
    );
    let covering = mgr.instances_of(DRIVEN_ID).next().unwrap();
    assert_ne!(covering.instance, bare_id);
    assert!(covering.look.covers_scope);
    assert_eq!(covering.look.shadow, PopupShadow::Modal);
    assert_eq!(covering.look.scrim_input, ScrimInput::TakesPointer);
    assert!(mgr.is_passive(DRIVEN_ID));

    mgr.follow_open_when(DRIVEN_ID, None, |_| PopupPayload::None);
    assert!(
        !mgr.covers_scope(DRIVEN_ID),
        "a closed instance returns to its declared look"
    );
}
