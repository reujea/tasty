//! 키 입력은 포커스를 가진 인스턴스만 받는다. 같은 선언의 인스턴스가 여럿 보일 때 Escape 대상과
//! 포커스 소유를 확인한다.

use super::*;

const ID: PopupId = "focus_owner_test";

fn manager() -> PopupManager {
    let mut mgr = PopupManager::new();
    mgr.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    mgr
}

/// Given 같은 선언의 두 인스턴스가 모두 포커스를 가졌고, When Escape 대상을 고르면,
/// Then 맨 위(나중에 연) 인스턴스다.
#[test]
fn escape_targets_the_topmost_focused_instance() {
    let mut mgr = manager();
    let below = mgr.open(ID, PopupScope::Surface(1)).unwrap();
    let above = mgr.open(ID, PopupScope::Surface(2)).unwrap();
    for iid in [below, above] {
        mgr.set_instance_focused(iid, true);
    }
    for p in &mut mgr.popups {
        p.scope_visible = true;
    }
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(above)
    );
}
