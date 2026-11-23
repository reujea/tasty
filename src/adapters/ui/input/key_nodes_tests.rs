//! 단계 4(경로 키의 수신자)와 그 값을 읽는 게이트(K9 `window_feeds_key`, 터미널 게이트 `window_key_holds_terminal`,
//! App plugin 게이트 `plugin_key_gate_open`). 창(winit·GPU)을 띄우지 않고 창이 부르는 같은 함수를 지금 상태로 부른다.
//! 단축키는 기본값과 겹치지 않는 조합으로 묶는다.

use super::*;
use crate::adapters::ui::input::key_level::PathLevels;
use crate::adapters::ui::input::key_route::{KeyEnd, KeyReceiver};
use crate::adapters::ui::popup::{PopupInstanceId, PopupScope};
use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
use crate::state::RequestContext;
use crate::state::focus_node::FocusNode;
use winit::keyboard::{KeyCode, NamedKey, PhysicalKey};

/// `toggle_settings`(engine) = Ctrl+F5, `toggle_notifications`(Window) = Ctrl+F6, `new_tab`(Pane) = Ctrl+F7,
/// `fullscreen_stage_exit` = Ctrl+F8 로 묶은 장면.
fn scene() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    crate::i18n::init("en");
    let (state, mut session) = crate::state::tests::test_state();
    bind(&mut session, "toggle_settings", "ctrl+f5");
    bind(&mut session, "toggle_notifications", "ctrl+f6");
    bind(&mut session, "new_tab", "ctrl+f7");
    bind(&mut session, "fullscreen_stage_exit", "ctrl+f8");
    (state, session)
}

fn bind(session: &mut crate::runtime::engine_session::EngineSession, field: &str, combo: &str) {
    let engine = session.borrow_mut();
    assert!(
        engine.runtime.settings.keybindings.set_field(field, combo),
        "{field} = {combo}"
    );
}

fn f_key(n: u8) -> (Key, KeyCode) {
    let (named, code) = match n {
        5 => (NamedKey::F5, KeyCode::F5),
        6 => (NamedKey::F6, KeyCode::F6),
        7 => (NamedKey::F7, KeyCode::F7),
        8 => (NamedKey::F8, KeyCode::F8),
        9 => (NamedKey::F9, KeyCode::F9),
        _ => unreachable!(),
    };
    (Key::Named(named), code)
}

fn escape() -> (Key, KeyCode) {
    (Key::Named(NamedKey::Escape), KeyCode::Escape)
}

fn letter(c: &str, code: KeyCode) -> (Key, KeyCode) {
    (Key::Character(c.into()), code)
}

/// 창이 이 누름에 내리는 단계 4 판정(포커스 surface 는 egui 화면이 아니다).
fn decide(
    state: &MainViewState,
    read: &EngineRead<'_>,
    (logical, code): (Key, KeyCode),
    mods: ModifiersState,
) -> KeyDecision {
    window_key_decision(
        state,
        read,
        &KeyEventValue {
            logical: &logical,
            physical: PhysicalKey::Code(code),
            pressed: true,
            mods,
            egui_mods: crate::adapters::ui::input::key_class::egui_modifiers(mods),
        },
        false,
        false,
    )
}

fn routed(d: KeyDecision) -> Routed {
    match d {
        KeyDecision::Path(r) => r,
        KeyDecision::Current => panic!("경로 키가 아니다"),
    }
}

fn gate(
    state: &MainViewState,
    read: &EngineRead<'_>,
    (logical, code): (Key, KeyCode),
    mods: ModifiersState,
) -> bool {
    plugin_key_gate_open(
        state,
        read,
        &KeyEventValue {
            logical: &logical,
            physical: PhysicalKey::Code(code),
            pressed: true,
            mods,
            egui_mods: crate::adapters::ui::input::key_class::egui_modifiers(mods),
        },
    )
}

const CTRL: ModifiersState = ModifiersState::CONTROL;
const NONE: ModifiersState = ModifiersState::empty();

fn open_focused(state: &mut RequestContext, id: &'static str) -> PopupInstanceId {
    let iid = state
        .popups
        .open_centered_focused(id, PopupScope::Window)
        .expect("열린다");
    assert_eq!(
        state.popup_focus(),
        Some(crate::adapters::ui::popup::focus::PopupNode::Host(iid))
    );
    iid
}

fn open_search(state: &mut RequestContext, read: &EngineRead<'_>) -> (PopupInstanceId, u32) {
    let sid = state.focused_surface_id(read).expect("포커스 surface");
    let iid = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(sid))
                .focused(),
            true,
        )
        .expect("검색 띠가 열린다");
    state.normalize_focus(read);
    assert_eq!(
        state.focus_node(read),
        FocusNode::Popup(crate::adapters::ui::popup::focus::PopupNode::Host(iid))
    );
    (iid, sid)
}

/// T-12b-1: 덮지 않는 팝업(알림 목록, `Keys`) 포커스 + 그 팝업이 쓰지 않는 Window 단축키 → 단축키 단계가 받는다.
#[test]
fn an_uncovering_popup_passes_an_unused_window_shortcut_up() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    assert!(!state.popups.instance(iid).unwrap().covers());
    let r = routed(decide(&state, &read, f_key(6), CTRL));
    assert_eq!(r.receiver, KeyReceiver::Shortcut { end: KeyEnd::Drop });
    assert!(r.levels.contains(KeyLevel::Window));
    // 팝업이 쓰는 키(수식키 없는 ↓)는 팝업이 받는다(대조).
    let down = (Key::Named(NamedKey::ArrowDown), KeyCode::ArrowDown);
    assert_eq!(
        routed(decide(&state, &read, down, NONE)).receiver,
        KeyReceiver::Popup(iid)
    );
}

/// T-12b-1: 덮는 팝업(command_palette) 포커스 + 덮인 범위의 단축키(new_tab, Pane) → 단축키가 아니다. 같은 덮음 + engine
/// 단축키(toggle_settings) → 단축키 단계가 받는다.
#[test]
fn a_covering_popup_keeps_covered_shortcuts_off_the_path() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "command_palette");
    assert!(state.popups.instance(iid).unwrap().covers());
    let covered = routed(decide(&state, &read, f_key(7), CTRL));
    assert!(!covered.levels.contains(KeyLevel::Pane));
    assert!(!covered.levels.contains(KeyLevel::Window));
    assert_eq!(covered.receiver, KeyReceiver::Popup(iid));
    assert_eq!(
        routed(decide(&state, &read, f_key(5), CTRL)).receiver,
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// T-12b-1: 설정 열기 요청 래치 중 키는 지금 경로다(단계 4를 지나지 않는다).
#[test]
fn a_key_under_the_settings_latch_stays_on_the_current_path() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert!(matches!(
        decide(&state, &read, f_key(5), CTRL),
        KeyDecision::Path(_)
    ));
    state.settings_open_requested = true;
    assert_eq!(decide(&state, &read, f_key(5), CTRL), KeyDecision::Current);
}

/// T-12b-2 K9: 단축키·끝 목적지 수신자로 끝난 경로 키는 egui 큐에 싣지 않고, egui 수신자로 끝난 키만 싣는다.
/// 지금 경로로 남는 사건은 지금 게이트 값이다.
#[test]
fn only_keys_ending_at_an_egui_receiver_are_fed_to_egui() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    let up = decide(&state, &read, f_key(6), CTRL);
    assert_eq!(state.window_feeds_key(up, false, false), None);
    assert!(!state.window_key_holds_terminal(up));
    let down = (Key::Named(NamedKey::ArrowDown), KeyCode::ArrowDown);
    let taken = decide(&state, &read, down, NONE);
    assert_eq!(routed(taken).receiver, KeyReceiver::Popup(iid));
    assert_eq!(state.window_feeds_key(taken, false, false), Some(false));
    assert!(state.window_key_holds_terminal(taken));
    assert_eq!(
        state.window_feeds_key(KeyDecision::Current, false, false),
        state.window_feeds_egui(false, false)
    );
}

/// T-12b-2: 터미널 포커스 + 글자 → 그 surface(끝 목적지)이고 egui 로 가지 않는다.
#[test]
fn a_letter_on_a_terminal_ends_at_that_surface() {
    let (state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let sid = state.focused_surface_id(&read).expect("터미널");
    let d = decide(&state, &read, letter("a", KeyCode::KeyA), NONE);
    assert_eq!(routed(d).receiver, KeyReceiver::Surface(sid));
    assert!(routed(d).levels == PathLevels::all());
    let mut state = state;
    assert_eq!(state.window_feeds_key(d, false, false), None);
    assert!(!state.window_key_holds_terminal(d));
}

/// T-12b-3: 수식키 Esc 는 보통 키로 경로를 오른다. 덮지 않는 `Keys` 팝업 + Shift+Esc 에 묶은 Window 단축키 → 단축키.
/// 터미널 + Shift+Esc → PTY(그 surface).
#[test]
fn a_modified_escape_goes_up_the_key_path() {
    let (mut state, mut session) = scene();
    bind(&mut session, "toggle_notifications", "shift+escape");
    let engine = session.borrow_mut();
    let read = engine.read();
    let sid = state.focused_surface_id(&read).expect("터미널");
    assert_eq!(
        routed(decide(&state, &read, escape(), ModifiersState::SHIFT)).receiver,
        KeyReceiver::Shortcut {
            end: KeyEnd::Surface(sid)
        }
    );
    open_focused(&mut state, "notifications");
    assert_eq!(
        routed(decide(&state, &read, escape(), ModifiersState::SHIFT)).receiver,
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// T-12b-3: search_bar(`Text`) 포커스 + Ctrl+Esc 에 묶은 Surface 단축키 → 단축키(`Text` 명령 키 양보). 같은 팝업 +
/// Shift+Esc → 팝업이 받는다(`Text` 가 소비). 터미널 + 묶지 않은 Shift+Esc → 그 surface.
#[test]
fn a_text_popup_yields_a_bound_ctrl_escape_and_takes_shift_escape() {
    let (mut state, mut session) = scene();
    bind(&mut session, "close_surface", "ctrl+escape");
    let engine = session.borrow_mut();
    let read = engine.read();
    let sid = state.focused_surface_id(&read).expect("터미널");
    assert_eq!(
        routed(decide(&state, &read, escape(), ModifiersState::SHIFT)).receiver,
        KeyReceiver::Surface(sid)
    );
    let (iid, _) = open_search(&mut state, &read);
    assert_eq!(
        routed(decide(&state, &read, escape(), CTRL)).receiver,
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(
        routed(decide(&state, &read, escape(), ModifiersState::SHIFT)).receiver,
        KeyReceiver::Popup(iid)
    );
}

/// T-Esc-1: 수식키 없는 Esc 는 어느 포커스에서도 지금 경로다.
#[test]
fn a_bare_escape_stays_on_the_current_path() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(decide(&state, &read, escape(), NONE), KeyDecision::Current);
    open_focused(&mut state, "notifications");
    assert_eq!(decide(&state, &read, escape(), NONE), KeyDecision::Current);
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(decide(&state, &read, escape(), NONE), KeyDecision::Current);
}

/// T-12b-4: 무대 + 종료 키 → 무대 노드가 받는다(engine 후보보다 먼저). 무대 + engine 단축키 → 단축키. 무대 + Window
/// 단축키 → 단축키가 아니다(경로는 engine 만이라 무대 내용이 받는다).
#[test]
fn a_stage_takes_its_exit_key_and_passes_engine_shortcuts() {
    let (mut state, mut session) = scene();
    bind(&mut session, "toggle_settings", "ctrl+f8");
    let engine = session.borrow_mut();
    let read = engine.read();
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(state.focus_node(&read), FocusNode::Stage);
    let exit = routed(decide(&state, &read, f_key(8), CTRL));
    assert_eq!(exit.receiver, KeyReceiver::Stage);
    assert_eq!(exit.levels, PathLevels::engine_only());
    bind(&mut session, "toggle_settings", "ctrl+f5");
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(
        routed(decide(&state, &read, f_key(5), CTRL)).receiver,
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    // 경로에 그 키의 단축키가 없어 무대 내용(`Text`)이 명령 키를 받는다. Window 단축키는 돌지 않는다.
    assert_eq!(
        routed(decide(&state, &read, f_key(6), CTRL)).receiver,
        KeyReceiver::Stage
    );
}

/// T-12b-6 (다)·(라): App plugin 게이트. search_bar 포커스 + 팝업이 쓰지 않는 키 → 열림(지금 게이트는 닫힘). 무대 +
/// 무대 종료 키 → 닫힘. 무대 + 다른 키 → 열림. 래치 중 키는 지금 게이트(닫힘).
#[test]
fn the_plugin_gate_opens_for_keys_the_start_node_does_not_end() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_search(&mut state, &read);
    assert!(state.window_keys_to_overlay(), "지금 게이트는 닫혀 있다");
    assert!(gate(&state, &read, f_key(9), CTRL));
    assert!(
        !gate(&state, &read, letter("a", KeyCode::KeyA), NONE),
        "검색 입력칸이 쓰는 글자는 닫힘"
    );
    state.settings_open_requested = true;
    assert!(!gate(&state, &read, f_key(9), CTRL));
    state.settings_open_requested = false;
    state.popups.close(SEARCH_BAR_POPUP_ID);
    assert!(state.open_fullscreen_stage("blank"));
    assert!(!gate(&state, &read, f_key(8), CTRL), "무대 종료 키");
    assert!(gate(&state, &read, f_key(9), CTRL), "무대가 쓰지 않는 키");
}

/// T-12b-6: 터미널 포커스면 plugin 명령이 surface 내용보다 먼저다(출발 노드가 범위 노드면 열림).
#[test]
fn the_plugin_gate_is_open_on_a_terminal() {
    let (state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert!(gate(&state, &read, letter("a", KeyCode::KeyA), NONE));
    assert!(gate(&state, &read, f_key(9), CTRL));
}

/// T-12b-14: 노드 키맵. search_bar 포커스 + find 키(그 surface 의 Surface 단축키이기도 하다) → 검색 띠가 받는다.
#[test]
fn the_search_bar_keymap_takes_the_find_key() {
    let (mut state, mut session) = scene();
    bind(&mut session, "find", "ctrl+f9");
    let engine = session.borrow_mut();
    let read = engine.read();
    let (iid, _) = open_search(&mut state, &read);
    let r = routed(decide(&state, &read, f_key(9), CTRL));
    assert_eq!(r.receiver, KeyReceiver::Popup(iid));
    assert!(r.levels.contains(KeyLevel::Surface), "find 는 경로에 있다");
}

/// T-12b-17: `Text` 내용 명령 키. command_palette + Ctrl+A(경로에 같은 키 단축키 없음) → 팝업. 같은 팝업 + 경로에 남는
/// level(engine)의 단축키 키 → 단축키. `Keys` 팝업 + 소비하지 않는 명령 키 → 위로.
#[test]
fn a_text_popup_takes_a_command_key_only_when_no_shortcut_on_the_path_uses_it() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let palette = open_focused(&mut state, "command_palette");
    let select_all = letter("a", KeyCode::KeyA);
    assert_eq!(
        routed(decide(&state, &read, select_all.clone(), CTRL)).receiver,
        KeyReceiver::Popup(palette)
    );
    bind(&mut session, "toggle_settings", "ctrl+a");
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(
        routed(decide(&state, &read, select_all, CTRL)).receiver,
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    state.popups.close("command_palette");
    open_focused(&mut state, "notifications");
    assert_eq!(
        routed(decide(&state, &read, f_key(9), CTRL)).receiver,
        KeyReceiver::Drop
    );
}

/// 편집 바인딩 키·클립보드 조합·egui 키 없는 누름·뗌·재생 편집 표시는 지금 경로다.
#[test]
fn edit_clipboard_and_non_egui_keys_stay_on_the_current_path() {
    let (state, mut session) = scene();
    bind(&mut session, "copy", "ctrl+f9");
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(decide(&state, &read, f_key(9), CTRL), KeyDecision::Current);
    let command = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        CTRL
    };
    assert_eq!(
        decide(&state, &read, letter("v", KeyCode::KeyV), command),
        KeyDecision::Current
    );
    assert_eq!(
        decide(
            &state,
            &read,
            (Key::Named(NamedKey::Shift), KeyCode::ShiftLeft),
            ModifiersState::SHIFT
        ),
        KeyDecision::Current
    );
    let (logical, code) = f_key(6);
    let value = KeyEventValue {
        logical: &logical,
        physical: PhysicalKey::Code(code),
        pressed: true,
        mods: CTRL,
        egui_mods: crate::adapters::ui::input::key_class::egui_modifiers(CTRL),
    };
    assert!(matches!(
        window_key_decision(&state, &read, &value, false, false),
        KeyDecision::Path(_)
    ));
    assert_eq!(
        window_key_decision(&state, &read, &value, true, false),
        KeyDecision::Current,
        "재생하는 원래 키의 편집 표시"
    );
    let released = KeyEventValue {
        pressed: false,
        ..value
    };
    assert_eq!(
        window_key_decision(&state, &read, &released, false, false),
        KeyDecision::Current
    );
}

/// 입력 시점 결속: 창이 지금 받는 키를 팝업 키 주인에 묶었으면 그 주인의 포커스를 풀었어도 출발 노드는 그 팝업이다.
#[test]
fn a_bound_key_owner_is_the_start_node() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    assert!(state.bind_window_key_owner());
    state.popups.set_focus(None);
    state.normalize_focus(&read);
    assert!(matches!(state.focus_node(&read), FocusNode::Surface(_)));
    assert_eq!(
        crate::adapters::ui::input::key_path::key_start_node(&state, &read),
        FocusNode::Popup(crate::adapters::ui::popup::focus::PopupNode::Host(iid))
    );
    let down = (Key::Named(NamedKey::ArrowDown), KeyCode::ArrowDown);
    assert_eq!(
        routed(decide(&state, &read, down, NONE)).receiver,
        KeyReceiver::Popup(iid)
    );
}

/// T-12b-7: 전달 키 분류. 보통 단축키 키는 경로 키, bare Esc·클립보드 조합·편집 바인딩 키는 지금 경로다.
#[test]
fn forwarded_keys_are_classified_like_window_keys() {
    let (_state, mut session) = scene();
    bind(&mut session, "copy", "ctrl+f9");
    let engine = session.borrow_mut();
    let kb = &engine.read().settings.keybindings;
    let class = |key: Key, mods| forwarded_key_class(kb, &key, mods);
    assert_eq!(class(Key::Character("b".into()), CTRL), ForwardedKey::Path);
    assert_eq!(class(f_key(6).0, CTRL), ForwardedKey::Path);
    assert_eq!(class(escape().0, NONE), ForwardedKey::Current);
    assert_eq!(class(escape().0, ModifiersState::SHIFT), ForwardedKey::Path);
    assert_eq!(
        class(f_key(9).0, CTRL),
        ForwardedKey::Current,
        "편집 바인딩 키"
    );
    let command = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        CTRL
    };
    assert_eq!(
        class(Key::Character("c".into()), command),
        ForwardedKey::Current,
        "클립보드 조합"
    );
}

/// T-12b-7: 받은 surface 경로. 다른 곳(Window 범위)에 덮지 않는 팝업이 포커스여도 받은 surface 의 경로 키는 막히지
/// 않는다(지금 게이트는 막는다). 받은 surface 를 덮는 팝업이 있으면 막힌다. 지금 경로로 남는 키는 지금 게이트다.
#[test]
fn a_forwarded_path_key_is_blocked_only_when_its_surface_is_covered() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let sid = state.focused_surface_id(&read).expect("surface");
    assert!(!received_key_blocked(
        &state,
        &read,
        ForwardedKey::Path,
        sid
    ));
    open_focused(&mut state, "notifications");
    assert!(state.window_keys_to_overlay(), "지금 게이트는 막는다");
    assert!(!received_key_blocked(
        &state,
        &read,
        ForwardedKey::Path,
        sid
    ));
    assert!(received_key_blocked(
        &state,
        &read,
        ForwardedKey::Current,
        sid
    ));
    state.popups.close("notifications");
    let palette = open_focused(&mut state, "command_palette");
    assert!(state.popups.instance(palette).unwrap().covers());
    assert!(received_key_blocked(&state, &read, ForwardedKey::Path, sid));
}

/// T-12b-6 (라) 진입 직후: 팝업에 키를 묶은 몫에서 무대를 열면 무대가 출발 노드다. 그 몫을 그리기 전 무대 종료 키는
/// App plugin 게이트를 닫아 무대 종료 단계로 간다(무대 노드가 결속보다 먼저다).
#[test]
fn a_stage_opened_over_a_bound_popup_is_the_start_node() {
    let (mut state, mut session) = scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_search(&mut state, &read);
    assert!(state.bind_window_key_owner());
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(
        crate::adapters::ui::input::key_path::key_start_node(&state, &read),
        FocusNode::Stage
    );
    assert!(!gate(&state, &read, f_key(8), CTRL), "무대 종료 키");
    assert_eq!(
        routed(decide(&state, &read, f_key(8), CTRL)).receiver,
        KeyReceiver::Stage
    );
}
