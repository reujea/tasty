//! 키 입력은 포커스를 가진 맨 위 인스턴스가 받는다. 같은 선언의 인스턴스가 여러 범위에 보일 때 Escape 대상과
//! 포커스 소유를 확인한다. 다른 범위의 인스턴스는 포커스를 서로 빼앗지 않는다.

use super::*;
use crate::adapters::ui::LayoutContext;
use crate::intent::OpenRequest;

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
    mgr.open(ID, PopupScope::Surface(1)).unwrap();
    let above = mgr.open(ID, PopupScope::Surface(2)).unwrap();
    for p in &mut mgr.popups {
        p.focused = true;
        p.scope_visible = true;
    }
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(above)
    );
}

/// Given 한 범위에 포커스를 가진 인스턴스가 있고, When 다른 범위에 같은 선언을 포커스를 주며 열면,
/// Then 다른 범위의 인스턴스는 포커스를 잃지 않고, 키는 새(맨 위) 인스턴스가 받는다.
#[test]
fn a_focused_open_leaves_the_focus_of_a_sibling_in_another_scope() {
    let mut mgr = manager();
    let first = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(1)).focused(),
            true,
        )
        .unwrap();
    let second = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(2)).focused(),
            true,
        )
        .unwrap();
    assert!(mgr.instance(first).unwrap().focused);
    assert!(mgr.instance(second).unwrap().focused);
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(second)
    );
}

/// 보이는 surface만 담은 배치. 목록에 없는 surface 범위의 팝업은 숨는다.
fn layout_showing(surfaces: &[u32]) -> LayoutContext {
    let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects: surfaces.iter().map(|&s| (s, full)).collect(),
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    }
}

/// `surfaces`만 보이는 배치로 한 프레임을 그린다. `press`가 있으면 그 위치를 누른다.
fn frame(mgr: &mut PopupManager, surfaces: &[u32], press: Option<egui::Pos2>) {
    let ctx = egui::Context::default();
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    if let Some(pos) = press {
        raw.events.push(egui::Event::PointerMoved(pos));
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    }
    let layout = layout_showing(surfaces);
    drop(ctx.run(raw, |ctx| {
        mgr.draw(ctx, &mut |_, _| {}, Some(&layout), &[]);
    }));
}

fn focused_instances(mgr: &PopupManager) -> Vec<PopupInstanceId> {
    mgr.popups
        .iter()
        .filter(|p| p.id == ID && p.open && p.focused)
        .map(|p| p.instance)
        .collect()
}

/// Given B를 먼저, A를 나중에 포커스를 주며 열어 둘 다 포커스를 갖고 A가 키 주인이며 A의 범위가 숨었을 때, When B를 누르면, Then A의 범위가
/// 다시 보여도 다른 범위의 A는 포커스를 그대로 갖고, B가 맨 앞이라 키 주인이다.
#[test]
fn a_click_leaves_a_hidden_sibling_focused_and_makes_the_clicked_one_the_key_owner() {
    let mut mgr = manager();
    let b = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(2))
                .at(egui::pos2(600.0, 300.0))
                .focused(),
            true,
        )
        .unwrap();
    let a = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(1))
                .at(egui::pos2(100.0, 300.0))
                .focused(),
            true,
        )
        .unwrap();
    assert_eq!(focused_instances(&mgr), [b, a]);
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(a)
    );
    frame(&mut mgr, &[2], Some(egui::pos2(650.0, 350.0)));
    frame(&mut mgr, &[1, 2], None);
    assert!(focused_instances(&mgr).contains(&b));
    assert!(focused_instances(&mgr).contains(&a));
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(b)
    );
}

/// Given passive로 연 B와 포커스를 가진 A가 있고 A의 범위가 숨었을 때, When B를 passive 승격하면,
/// Then A의 범위가 다시 보여도 다른 범위의 A는 포커스를 그대로 갖고, B가 맨 앞이라 키 주인이다.
#[test]
fn a_passive_promotion_leaves_a_hidden_sibling_focused_and_makes_the_promoted_one_the_key_owner() {
    let mut mgr = manager();
    let b = mgr
        .open_request(&OpenRequest::new(ID, PopupScope::Surface(2)), false)
        .unwrap();
    let a = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(1)).focused(),
            true,
        )
        .unwrap();
    assert_eq!(focused_instances(&mgr), [a]);
    frame(&mut mgr, &[2], None);
    assert!(mgr.focus_passive(ID, &|_| {
        crate::adapters::ui::popup::route::ScopePath::window_for_test()
    }));
    frame(&mut mgr, &[1, 2], None);
    assert!(focused_instances(&mgr).contains(&b));
    assert!(focused_instances(&mgr).contains(&a));
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(b)
    );
}

/// 두 범위가 모두 보이는 배치로 키 하나를 넣은 프레임을 그리고, 본문이 egui 입력에서 키를 본 인스턴스를 돌려준다.
fn instances_reading_keys(mgr: &mut PopupManager) -> Vec<PopupInstanceId> {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events: vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let layout = layout_showing(&[1, 2]);
    let mut readers = Vec::new();
    drop(ctx.run(raw, |ctx| {
        mgr.draw(
            ctx,
            &mut |cx, ui| {
                // 본문이 키 주인 표시를 보지 않고 egui 입력을 바로 읽어도 키 주인 본문만 키를 본다.
                let sees = ui.input(|i| i.key_pressed(egui::Key::Enter));
                assert_eq!(sees, cx.focused, "키를 보는 본문은 키 주인 표시와 같다");
                if sees {
                    readers.push(cx.instance);
                }
            },
            Some(&layout),
            &[],
        );
    }));
    readers
}

/// Given 두 범위에 포커스를 유지한 host 팝업이 하나씩 있을 때, When 키를 넣으면, Then 본문은 키 주인(맨 위) 하나만
/// 그 키를 처리한다. When plugin 팝업이 키 주인이면, Then 어느 host 본문도 키를 받지 않는다.
#[test]
fn only_the_key_owner_body_reads_keys_while_other_scopes_keep_focus() {
    let mut mgr = manager();
    let first = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(1)).focused(),
            true,
        )
        .unwrap();
    let second = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(2)).focused(),
            true,
        )
        .unwrap();
    assert_eq!(focused_instances(&mgr), [first, second]);
    assert_eq!(instances_reading_keys(&mut mgr), [second]);
    mgr.set_plugin_holds_keys(true);
    assert!(instances_reading_keys(&mut mgr).is_empty());
}

/// Given 두 범위에 포커스를 유지한 host 팝업이 하나씩 있을 때, When 창이 키 주인(위)의 포커스를 풀고 그 키를 입력
/// 시점의 키 주인에 묶으면, Then 같은 키를 실은 프레임에서는 묶인 팝업 본문만 그 키를 읽고(하위 화면 되돌아가기)
/// 새로 키 주인이 된 아래 팝업은 읽지 않는다. When 묶인 팝업이 그사이 닫혔으면, Then 아무 본문도 읽지 않는다.
#[test]
fn a_key_the_window_handled_reaches_only_the_owner_at_input_time() {
    let mut mgr = manager();
    let first = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(1)).focused(),
            true,
        )
        .unwrap();
    let second = mgr
        .open_request(
            &OpenRequest::new(ID, PopupScope::Surface(2)).focused(),
            true,
        )
        .unwrap();
    let (_, owner, _) = mgr.focused_dismissal_target().unwrap();
    assert_eq!(owner, second);
    mgr.set_instance_focused(owner, false);
    mgr.bind_input_key_owner(owner);
    assert_eq!(instances_reading_keys(&mut mgr), [second]);
    assert_eq!(
        instances_reading_keys(&mut mgr),
        [first],
        "다음 키부터는 새 키 주인이 받는다"
    );

    mgr.set_instance_focused(second, true);
    let (_, owner, _) = mgr.focused_dismissal_target().unwrap();
    assert_eq!(owner, second);
    mgr.bind_input_key_owner(owner);
    mgr.close_instance(owner, super::PopupCloseReason::Escape);
    assert!(
        instances_reading_keys(&mut mgr).is_empty(),
        "포커스가 남은 아래 팝업도 닫힌 팝업의 키를 읽지 않는다"
    );
    assert_eq!(instances_reading_keys(&mut mgr), [first]);
}
