//! 관리자가 본문에 넘기는 키 통로(`PopupCx::keys`). `draw_routed`를 egui 프레임 안에서 부르고 본문 자리에 탐침을
//! 넘겨, 통로가 키를 주는지와 본문 문이 열렸는지(본문 안에서 키 사건이 보이는지)를 인스턴스마다 기록한다.

use super::*;
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::input::edit_key::EditBindings;
use crate::intent::OpenRequest;

const ID: PopupId = "cx_keys_test";
const OWNER: PopupId = "cx_keys_owner";

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

fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// 한 프레임을 그리고, 본문이 불린 인스턴스마다 탐침 결과를 돌려준다.
fn frame<T>(
    mgr: &mut PopupManager,
    edits: &EditBindings,
    events: Vec<egui::Event>,
    mut probe: impl FnMut(&mut PopupCx<'_>, &mut egui::Ui) -> T,
) -> Vec<(PopupInstanceId, T)> {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events,
        ..Default::default()
    };
    let layout = layout_showing(&[1, 2]);
    let mut seen = Vec::new();
    drop(ctx.run(raw, |ctx| {
        mgr.draw_routed(
            ctx,
            &mut |cx, ui| {
                let out = probe(cx, ui);
                seen.push((cx.instance, out));
            },
            Some(&layout),
            edits,
            &[],
            None,
        );
    }));
    seen
}

/// 통로가 키를 주는지와 본문 문이 열렸는지(본문 안에서 키 사건이 보이는지).
fn gate(cx: &mut PopupCx<'_>, ui: &mut egui::Ui) -> (bool, bool) {
    let open = ui.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::Key { .. }))
    });
    (cx.keys.receives(), open)
}

fn enter() -> Vec<egui::Event> {
    vec![key(egui::Key::Enter, egui::Modifiers::NONE)]
}

fn of<T: Copy>(seen: &[(PopupInstanceId, T)], iid: PopupInstanceId) -> T {
    seen.iter()
        .find(|(i, _)| *i == iid)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("{iid:?} 의 본문이 불리지 않았다: {:?}", seen.len()))
}

/// 선언 없이 만든(선언 기본 `Keys`) 키 주인 후보.
fn owner_state() -> PopupState {
    PopupState::new(OWNER, "owner", egui::vec2(200.0, 100.0), PopupScope::Window)
}

fn restore_notice_def() -> &'static PopupDef {
    defs::find(crate::adapters::ui::restore_notice::RESTORE_NOTICE_POPUP_ID)
        .expect("restore notice is declared")
}

/// Given 같은 정의(선언 `Keys`)의 인스턴스 둘이 두 범위에 열렸고 나중에 연 것이 키 주인일 때, When 키를 넣고 그리면,
/// Then 두 본문이 모두 불리고 키 주인만 통로가 키를 준다.
#[test]
fn of_two_instances_of_one_definition_only_the_key_owner_receives_keys() {
    let mut mgr = PopupManager::new();
    mgr.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
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
    for iid in [first, second] {
        assert_eq!(mgr.instance(iid).unwrap().keyboard, PopupKeyboard::Keys);
    }
    let seen = frame(&mut mgr, &EditBindings::default(), enter(), gate);
    assert_eq!(seen.len(), 2, "두 본문이 모두 불린다");
    assert!(
        !of(&seen, first).0,
        "키 주인이 아닌 인스턴스는 키를 받지 않는다"
    );
    assert!(of(&seen, second).0, "키 주인은 키를 받는다");
}

/// Given 키 주인이 없는 프레임에 복원 안내(선언 `keyboard: None`)만 열렸을 때, When 키를 넣고 그리면, Then 본문 문은
/// 열려 카드 안 위젯이 키를 보지만 통로는 키를 주지 않는다.
#[test]
fn a_restore_notice_without_a_key_owner_sees_keys_through_its_gate_but_its_passage_gives_none() {
    let mut mgr = PopupManager::new();
    mgr.register_def(restore_notice_def(), 1.0);
    let iid = mgr
        .open_request(
            &OpenRequest::new(restore_notice_def().id, PopupScope::Window),
            false,
        )
        .expect("restore notice opens");
    assert!(!mgr.has_focused(), "키 주인이 없는 프레임이다");
    let (receives, open) = of(
        &frame(&mut mgr, &EditBindings::default(), enter(), gate),
        iid,
    );
    assert!(open, "본문 문이 열려 키 사건이 보인다");
    assert!(!receives, "선언이 None이라 통로는 키를 주지 않는다");
}

/// Given 복원 안내와 다른 키 주인(선언 기본 `Keys`)이 함께 열렸을 때, When 키를 넣고 그리면, Then 복원 안내의 본문 문은
/// 닫혀 키 사건이 보이지 않고 통로도 키를 주지 않는다.
#[test]
fn a_restore_notice_beside_a_key_owner_has_its_gate_closed_and_no_keys() {
    let mut mgr = PopupManager::new();
    mgr.register_def(restore_notice_def(), 1.0);
    mgr.register(owner_state());
    let notice = mgr
        .open_request(
            &OpenRequest::new(restore_notice_def().id, PopupScope::Window),
            false,
        )
        .expect("restore notice opens");
    let owner = mgr
        .open_request(&OpenRequest::new(OWNER, PopupScope::Window).focused(), true)
        .unwrap();
    let seen = frame(&mut mgr, &EditBindings::default(), enter(), gate);
    assert_eq!(of(&seen, owner), (true, true), "다른 키 주인이 키를 받는다");
    assert_eq!(
        of(&seen, notice),
        (false, false),
        "복원 안내는 본문 문이 닫히고 통로도 키를 주지 않는다"
    );
}

/// 정의 하나를 연 관리자. `focused`면 포커스를 주며 열고, `with_owner`면 다른 키 주인을 함께 연다.
fn one_definition(
    def: &PopupDef,
    focused: bool,
    with_owner: bool,
) -> (PopupManager, PopupInstanceId) {
    let mut mgr = PopupManager::new();
    mgr.register_def(def, 1.0);
    mgr.register(owner_state());
    let request = OpenRequest::new(def.id, PopupScope::Window);
    let request = if focused { request.focused() } else { request };
    let iid = mgr
        .open_request(&request, focused)
        .unwrap_or_else(|| panic!("{} opens in the window scope", def.id));
    if with_owner {
        mgr.open_request(&OpenRequest::new(OWNER, PopupScope::Window).focused(), true)
            .unwrap();
    }
    (mgr, iid)
}

/// Given 모든 정의의 인스턴스를 (스스로 포커스를 받음 / 다른 키 주인이 있음 / 키 주인이 없음) 세 프레임으로 그릴 때,
/// Then 통로가 키를 주는 것은 본문 문이 열렸고 선언 `keyboard`가 `None`이 아닐 때와 같다.
#[test]
fn every_definition_receives_keys_exactly_when_its_gate_is_open_and_it_declares_keys() {
    let mut open_frames = 0;
    let mut closed_frames = 0;
    for def in defs::all_defs() {
        for (focused, with_owner) in [(true, false), (false, true), (false, false)] {
            let (mut mgr, iid) = one_definition(def, focused, with_owner);
            let (receives, open) = of(
                &frame(&mut mgr, &EditBindings::default(), enter(), gate),
                iid,
            );
            let declares_keys = def.decl.keyboard != PopupKeyboard::None;
            assert_eq!(
                receives,
                open && declares_keys,
                "{} focused={focused} with_owner={with_owner}: receives={receives} open={open}",
                def.id
            );
            if open {
                open_frames += 1;
            } else {
                closed_frames += 1;
            }
        }
    }
    assert!(
        open_frames > 0 && closed_frames > 0,
        "열린 문과 닫힌 문을 모두 쟀다"
    );
}

/// Given 선언 `Text` 인스턴스가 키 주인이고 Ctrl+V가 눌렸을 때, When paste `["ctrl+v"]` 조합을 `draw_routed`에 넘기면,
/// Then 본문 통로의 `consume_key`가 그 키를 소비한다. When 같은 프레임에 빈 조합을 넘기면, Then 소비하지 않는다.
#[test]
fn the_manager_hands_the_given_edit_bindings_to_the_passage() {
    let consumes = |edits: &EditBindings| {
        let mut mgr = PopupManager::new();
        let decl = PopupDecl {
            keyboard: PopupKeyboard::Text,
            ..PopupDecl::new(PopupScopeKind::Window, popup_size(egui::vec2(200.0, 100.0)))
        };
        mgr.register(PopupState::declared(
            ID,
            "t",
            &decl,
            None,
            PopupScope::Window,
        ));
        let iid = mgr
            .open_request(&OpenRequest::new(ID, PopupScope::Window).focused(), true)
            .unwrap();
        let seen = frame(
            &mut mgr,
            edits,
            vec![key(egui::Key::V, egui::Modifiers::CTRL)],
            |cx, _| {
                (
                    cx.keys.receives(),
                    cx.keys.consume_key(egui::Modifiers::CTRL, egui::Key::V),
                )
            },
        );
        of(&seen, iid)
    };
    let mut kb = crate::settings::KeybindingSettings::default();
    kb.copy = Vec::new();
    kb.cut = Vec::new();
    kb.paste = vec!["ctrl+v".to_string()];
    assert_eq!(
        consumes(&EditBindings::from_settings(&kb)),
        (true, true),
        "받은 paste 조합으로 Ctrl+V를 소비한다"
    );
    assert_eq!(
        consumes(&EditBindings::default()),
        (true, false),
        "빈 조합이면 Ctrl+V는 소비 집합 밖이다"
    );
}

/// Given 같은 정의의 인스턴스 둘이 열렸고 host 키 주인이 나중에 연 것일 때, When host 팝업 층만 그리면(창 결정 없음),
/// Then 본문 문은 `host_key_owner`가 고른 인스턴스만 열린다.
#[test]
fn with_a_key_owner_only_the_host_key_owner_body_gate_opens() {
    let mut mgr = PopupManager::new();
    mgr.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
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
    let seen = frame(&mut mgr, &EditBindings::default(), enter(), gate);
    assert_eq!(
        mgr.host_key_owner().map(|p| p.instance),
        Some(second),
        "전제: host 키 주인은 나중에 연 인스턴스"
    );
    assert!(
        !of(&seen, first).1,
        "키 주인이 아닌 인스턴스의 본문 문은 닫힌다"
    );
    assert!(of(&seen, second).1, "키 주인의 본문 문은 열린다");
}
