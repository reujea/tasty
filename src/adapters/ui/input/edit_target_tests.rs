//! 편집 키의 대상이 단계 4 수신자를 따르는지. 창(winit·GPU)을 띄우지 않고 창이 부르는 같은 판정(`settle_key`)과
//! 대상 함수를 지금 상태로 부른다.

use super::*;
use crate::adapters::ui::input::key_class::KeyEventValue;
use crate::adapters::ui::input::key_nodes::window_key_decision;
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::{PopupEscape, PopupInstanceId, PopupKeyboard, PopupScope};
use crate::runtime::engine_read::EngineRead;
use crate::state::RequestContext;
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

const ALT: ModifiersState = ModifiersState::ALT;

/// 편집 바인딩을 preset 꼴(Alt+C·X·V)로, `new_tab`(Pane)을 Ctrl+F7 로 묶은 장면.
fn scene() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    crate::i18n::init("en");
    let (state, mut session) = crate::state::tests::test_state();
    bind(&mut session, "copy", "alt+c");
    bind(&mut session, "cut", "alt+x");
    bind(&mut session, "paste", "alt+v");
    bind(&mut session, "new_tab", "ctrl+f7");
    (state, session)
}

fn bind(session: &mut crate::runtime::engine_session::EngineSession, field: &str, combo: &str) {
    let engine = session.borrow_mut();
    assert!(
        engine.runtime.settings.keybindings.set_field(field, combo),
        "{field} = {combo}"
    );
}

fn letter(c: &str, code: KeyCode) -> (Key, KeyCode) {
    (Key::Character(c.into()), code)
}

fn command() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    }
}

fn decide_with(
    state: &RequestContext,
    read: &EngineRead<'_>,
    (logical, code): (Key, KeyCode),
    mods: ModifiersState,
    replay_edit: bool,
) -> KeyDecision {
    window_key_decision(
        state,
        read,
        &KeyEventValue {
            logical: &logical,
            physical: PhysicalKey::Code(code),
            pressed: true,
            mods,
        },
        replay_edit,
        false,
    )
}

fn decide(
    state: &RequestContext,
    read: &EngineRead<'_>,
    key: (Key, KeyCode),
    mods: ModifiersState,
) -> KeyDecision {
    decide_with(state, read, key, mods, false)
}

fn receiver(decision: KeyDecision) -> KeyReceiver {
    path_receiver(decision).expect("경로 키")
}

/// 덮지 않는 `Keys` 팝업(알림 목록)을 포커스로 연다. 지금 오버레이 게이트는 열려 있다.
fn open_keys_popup(state: &mut RequestContext) -> PopupInstanceId {
    let iid = state
        .popups
        .open_centered_focused("notifications", PopupScope::Window)
        .expect("열린다");
    assert_eq!(state.popup_focus(), Some(PopupNode::Host(iid)));
    assert!(state.window_keys_to_overlay(), "지금 게이트는 오버레이다");
    iid
}

/// 포커스 surface 의 검색 띠(`Text`)를 포커스로 연다.
fn open_text_popup(state: &mut RequestContext, read: &EngineRead<'_>) -> PopupInstanceId {
    let sid = state.focused_surface_id(read).expect("포커스 surface");
    let iid = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(
                crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID,
                PopupScope::Surface(sid),
            )
            .focused(),
            true,
        )
        .expect("검색 띠가 열린다");
    state.normalize_focus(read);
    iid
}

/// 창 범위 plugin 팝업 하나를 포커스로 둔다. `keyboard` 는 그 팝업 선언이다.
fn place_plugin_popup(state: &mut RequestContext, id: u64, keyboard: PopupKeyboard) {
    let z = tasty_host_plugin::next_popup_z_seq();
    state.plugin_popup_key_candidates.push((id, z));
    state
        .plugin_popup_bound_scopes
        .insert(id, ("p".to_string(), Some(PopupScope::Window)));
    state
        .plugin_popup_key_decls
        .insert(id, (keyboard, PopupEscape::Close));
    state.popups.set_focus(Some(PopupNode::Plugin(id)));
    assert_eq!(state.plugin_popup_key_target(), Some(id));
}

/// 단계 6 의 host 편집 대상. 지금 경로로 남는 사건의 판정은 오버레이 게이트로 둔다.
fn host_target(state: &RequestContext, decision: KeyDecision) -> bool {
    host_edit_target(decision, || false, || state.window_keys_to_overlay())
}

/// T-12b-8: 덮지 않는 `Keys` 팝업 포커스에서 편집 바인딩(Alt+C)은 그 팝업이 쓰지 않아 경로를 올라가고, host egui
/// 편집 대상이 아니다(오버레이 게이트는 열려 있다).
#[test]
fn an_edit_binding_on_a_keys_popup_rises_to_the_focus_path() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_keys_popup(&mut state);
    let decision = decide(&state, &read, letter("c", KeyCode::KeyC), ALT);
    assert!(!receiver(decision).is_egui(), "{decision:?}");
    assert!(!host_target(&state, decision));
}

/// T-12b-15: `Text` 팝업 포커스에서 편집 바인딩은 그 팝업이 받고 host egui 편집 대상이다.
#[test]
fn an_edit_binding_on_a_text_popup_edits_it() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_text_popup(&mut state, &read);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), ALT);
    assert_eq!(receiver(decision), KeyReceiver::Popup(iid));
    assert!(host_target(&state, decision));
}

/// `Keys` plugin 팝업 포커스의 편집 키는 그 팝업에 편집 요청을 보내지 않는다(키 주인이어도 수신자가 아니다).
#[test]
fn an_edit_key_on_a_keys_plugin_popup_sends_no_edit_request() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    place_plugin_popup(&mut state, 42, PopupKeyboard::Keys);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), ALT);
    assert_ne!(receiver(decision), KeyReceiver::PluginPopup(42));
    assert_eq!(
        plugin_popup_edit_target(decision, state.plugin_popup_key_target()),
        None
    );
}

/// `Text` plugin 팝업 포커스의 편집 키는 그 팝업이 받아 편집 요청과 붙여넣기 대상이 그 팝업이다.
#[test]
fn an_edit_key_on_a_text_plugin_popup_asks_that_popup() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    place_plugin_popup(&mut state, 42, PopupKeyboard::Text);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), ALT);
    assert_eq!(receiver(decision), KeyReceiver::PluginPopup(42));
    assert_eq!(
        plugin_popup_edit_target(decision, state.plugin_popup_key_target()),
        Some(42)
    );
}

/// 재생하는 원래 키는 보류 때 판정한 편집이다. 재생 사이에 그 키가 편집 바인딩에서 빠지고 경로의 단축키(`new_tab`)만
/// 남아도 `Text` 팝업이 받는다.
#[test]
fn a_replayed_edit_keeps_its_judgment_after_a_settings_change() {
    let (mut state, mut session) = scene();
    bind(&mut session, "paste", "ctrl+f7");
    let iid = {
        let engine = session.borrow_mut();
        let read = engine.read();
        let iid = open_text_popup(&mut state, &read);
        let held = decide(
            &state,
            &read,
            (Key::Named(NamedKey::F7), KeyCode::F7),
            ModifiersState::CONTROL,
        );
        assert_eq!(receiver(held), KeyReceiver::Popup(iid), "보류 때 판정");
        iid
    };
    bind(&mut session, "paste", "alt+v");
    let engine = session.borrow_mut();
    let read = engine.read();
    let f7 = || (Key::Named(NamedKey::F7), KeyCode::F7);
    assert!(
        matches!(
            receiver(decide(&state, &read, f7(), ModifiersState::CONTROL)),
            KeyReceiver::Shortcut { .. }
        ),
        "새 누름은 바뀐 설정으로 단축키다"
    );
    let replayed = decide_with(&state, &read, f7(), ModifiersState::CONTROL, true);
    assert_eq!(receiver(replayed), KeyReceiver::Popup(iid));
}

/// 설정이 그대로면 재생하는 편집 키도 `Text` 팝업이 받는다.
#[test]
fn a_replayed_edit_without_a_settings_change_edits() {
    let (mut state, mut session) = scene();
    bind(&mut session, "paste", "ctrl+f7");
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_text_popup(&mut state, &read);
    let f7 = (Key::Named(NamedKey::F7), KeyCode::F7);
    let replayed = decide_with(&state, &read, f7, ModifiersState::CONTROL, true);
    assert_eq!(receiver(replayed), KeyReceiver::Popup(iid));
    assert!(host_target(&state, replayed));
}

/// 덮지 않는 `Keys` 팝업 아래 plugin surface 로 올라간 편집 키는 host egui 가 막지 않는다(그 surface 편집 요청으로
/// 간다). 지금 오버레이 게이트는 막는다.
#[test]
fn an_edit_key_rising_from_a_keys_popup_reaches_the_plugin_surface() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_keys_popup(&mut state);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), ALT);
    assert!(!host_egui_takes_edit(
        decision,
        state.window_keys_to_overlay()
    ));
}

/// plugin surface 위 `Text` 팝업 포커스의 Ctrl+V 는 그 팝업이 받고, plugin surface 편집은 host egui 가 막는다.
#[test]
fn ctrl_v_on_a_text_popup_over_a_plugin_surface_pastes_into_the_popup() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_text_popup(&mut state, &read);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), command());
    assert_eq!(receiver(decision), KeyReceiver::Popup(iid));
    assert!(host_egui_takes_edit(
        decision,
        state.window_keys_to_overlay()
    ));
    assert!(host_target(&state, decision));
}

/// 팝업이 없으면 plugin surface 의 Ctrl+V 는 host egui 가 막지 않는다.
#[test]
fn ctrl_v_on_a_plugin_surface_pastes_into_the_surface() {
    let (state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), command());
    assert!(!host_egui_takes_edit(
        decision,
        state.window_keys_to_overlay()
    ));
    assert!(!host_target(&state, decision));
}

/// 클립보드 조합(Ctrl+V, 편집 바인딩 아님)도 `Keys` 팝업에서 다른 편집 바인딩처럼 경로를 올라간다.
#[test]
fn ctrl_v_on_a_keys_popup_rises_like_other_edit_bindings() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_keys_popup(&mut state);
    let decision = decide(&state, &read, letter("v", KeyCode::KeyV), command());
    assert!(!receiver(decision).is_egui(), "{decision:?}");
    assert!(!host_target(&state, decision));
}

/// 클립보드 조합이 아닌 편집 바인딩(Alt+X)도 `Keys` 팝업에서 경로를 올라간다.
#[test]
fn a_non_clipboard_edit_binding_rises_on_a_keys_popup() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_keys_popup(&mut state);
    let decision = decide(&state, &read, letter("x", KeyCode::KeyX), ALT);
    assert!(!receiver(decision).is_egui(), "{decision:?}");
    assert!(!host_target(&state, decision));
}
