//! 탐색기가 포커스일 때 bare Esc 의 단계 4 수신자. 탐색기가 그 Esc 를 쓰면(주소 확인 대기, 고른 항목) 단축키 단계가
//! 받고, 쓰지 않으면 범위 노드에 붙은 Window 팝업(알림 목록)이 받는다. 사용 판정은 실행부
//! (`MainView::cancel_explorer_address_check`·`explorer_key_clear_selection`)의 거절 조건과 같은 함수다.

use super::*;
use crate::adapters::ui::input::key_route::KeyReceiver;
use crate::adapters::ui::popup::{PopupInstanceId, PopupScope};
use crate::state::RequestContext;
use winit::keyboard::{KeyCode, NamedKey, PhysicalKey};

struct Scene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    sid: u32,
    root: std::path::PathBuf,
    notifications: PopupInstanceId,
}

/// 기본 설정(`explorer_clear_selection` = Escape)의 탐색기 surface 가 포커스이고, 알림 목록이 포커스 없이 Window 범위에
/// 열린 장면. 고른 항목도 기다리는 주소 확인도 없다.
fn scene() -> Scene {
    crate::i18n::init("en");
    let (mut state, mut session, sid, root) = crate::state::tests::explorer_fixture(false);
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let notifications = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            false,
        )
        .expect("알림 목록이 열린다");
    let engine = session.borrow_mut();
    let read = engine.read();
    state.normalize_focus(&read);
    let panel = crate::model::ExplorerPanel::new(sid, root.clone());
    state.explorer_views.get_or_init(&panel, None);
    assert_eq!(
        crate::shortcuts::focused_explorer_surface_id(&state, &read),
        Some(sid),
        "탐색기가 포커스다"
    );
    assert!(
        !state.popups.is_instance_focused(notifications),
        "알림 목록은 포커스가 없다"
    );
    assert!(
        read.settings
            .keybindings
            .explorer_clear_selection
            .iter()
            .any(|c| c.eq_ignore_ascii_case("escape")),
        "기본 설정의 선택 해제는 Escape 다"
    );
    Scene {
        state,
        session,
        sid,
        root,
        notifications,
    }
}

/// 창이 탐색기(egui 로 입력을 받는 surface)에서 받은 bare Esc 누름의 단계 4 수신자.
fn escape_receiver(s: &mut Scene) -> KeyReceiver {
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let logical = Key::Named(NamedKey::Escape);
    let value = KeyEventValue {
        logical: &logical,
        physical: PhysicalKey::Code(KeyCode::Escape),
        pressed: true,
        mods: ModifiersState::empty(),
    };
    let egui_surface = focused_surface_takes_egui(&s.state, &read);
    assert!(egui_surface, "탐색기는 egui 로 입력을 받는다");
    match window_key_decision(&s.state, &read, &value, false, egui_surface) {
        KeyDecision::Path(routed) => routed.receiver,
        KeyDecision::Current => panic!("bare Esc 는 경로 키다"),
    }
}

fn view(s: &mut Scene) -> &mut crate::adapters::ui::surface::explorer::view::ExplorerView {
    s.state.explorer_views.get_mut(s.sid).expect("탐색기 view")
}

/// T-12b-10(M-12b-8·13 F). Given 기본 설정에서 고른 항목이 없는 탐색기가 포커스이고 알림 목록이 Window 범위에 붙어
/// 열려 있을 때, When bare Esc 를 누르면, Then 탐색기는 그 Esc 를 쓰지 않으므로 선택 해제 단축키가 후보가 아니고 Esc 는
/// 붙은 Window 팝업(알림 목록)이 받는다.
#[test]
fn escape_on_an_explorer_with_nothing_selected_reaches_the_attached_window_popup() {
    let mut s = scene();
    let engine = s.session.borrow_mut();
    assert!(
        !crate::shortcuts::explorer_takes_clear_selection(&s.state, &engine.read()),
        "고른 항목이 없으면 선택 해제를 쓰지 않는다"
    );
    assert_eq!(
        escape_receiver(&mut s),
        KeyReceiver::Popup(s.notifications),
        "쓰지 않는 Esc 는 붙은 알림 목록이 받는다"
    );
}

/// M-12b-8 P. Given 같은 장면에서 탐색기에 고른 항목이 있을 때, When bare Esc 를 누르면, Then 탐색기가 그 Esc 를 쓰므로
/// 단축키 단계(선택 해제)가 받고 알림 목록은 받지 않는다. 실행부도 같은 판정으로 거절하지 않는다.
#[test]
fn escape_used_by_explorer_clears_its_selection() {
    let mut s = scene();
    let picked = s.root.join("a.txt");
    view(&mut s).select_only(&picked);
    let engine = s.session.borrow_mut();
    assert!(crate::shortcuts::explorer_takes_clear_selection(
        &s.state,
        &engine.read()
    ));
    assert!(matches!(
        escape_receiver(&mut s),
        KeyReceiver::Shortcut { .. }
    ));
    s.state.key_event_dragging = true;
    assert_eq!(
        escape_receiver(&mut s),
        KeyReceiver::Popup(s.notifications),
        "끄는 중이면 탐색기가 그 Esc 를 쓰지 않는다(끌기 취소)"
    );
}

/// M-12b-11 F. Given 고른 항목이 없는 탐색기가 원격 주소 확인을 기다릴 때, When bare Esc 를 누르면, Then 주소 확인
/// 취소(고정 Esc)가 surface level 단축키 후보라 단축키 단계가 받고 알림 목록은 받지 않는다.
#[test]
fn escape_cancels_a_pending_explorer_address_check() {
    let mut s = scene();
    view(&mut s).wait_address_for_test();
    let engine = s.session.borrow_mut();
    assert!(crate::shortcuts::explorer_cancels_address_check(
        &s.state,
        &engine.read(),
        &Key::Named(NamedKey::Escape),
        ModifiersState::empty(),
    ));
    assert!(!crate::shortcuts::explorer_takes_clear_selection(
        &s.state,
        &engine.read()
    ));
    assert!(matches!(
        escape_receiver(&mut s),
        KeyReceiver::Shortcut { .. }
    ));
}

/// Given 같은 장면에 범위를 덮지 않는 plugin 팝업이 알림 목록보다 위(z)에 Window 범위로 포커스 없이 떠 있을 때, When bare
/// Esc 를 누르면, Then 붙은 팝업은 host·plugin 을 한 z 순서로 보므로 그 plugin 팝업이 받는다.
#[test]
fn escape_reaches_an_attached_plugin_popup_above_the_host_one() {
    let mut s = scene();
    let z = tasty_host_plugin::next_popup_z_seq();
    s.state.plugin_popup_key_candidates.push((42, z));
    s.state
        .plugin_popup_bound_scopes
        .insert(42, ("p".to_string(), Some(PopupScope::Window)));
    s.state.plugin_popup_key_decls.insert(
        42,
        (
            crate::adapters::ui::popup::PopupKeyboard::Keys,
            crate::adapters::ui::popup::PopupEscape::Close,
        ),
    );
    assert_eq!(escape_receiver(&mut s), KeyReceiver::PluginPopup(42));
}
