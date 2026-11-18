//! shortcuts 모듈 단위 테스트 — binding parsing/matching + zoom 단축키.

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey, SmolStr};

use super::physical_key_to_logical;
use crate::view::main::MainView;
use tasty_key_match::{matches_binding, parse_binding};

// macOS에서는 설정의 alt가 Command에 대응하고, 다른 플랫폼에서는 Alt에 대응한다.
#[cfg(target_os = "macos")]
const BINDING_ALT: ModifiersState = ModifiersState::SUPER;
#[cfg(not(target_os = "macos"))]
const BINDING_ALT: ModifiersState = ModifiersState::ALT;

/// macOS 에서 바인딩 `option` 토큰에 대응하는 modifier(Option = winit `ALT`). 다른 OS 의 Win·Super 는 별도 시험이 본다.
#[cfg(target_os = "macos")]
const BINDING_OPTION: ModifiersState = ModifiersState::ALT;

fn mods_ctrl() -> ModifiersState {
    ModifiersState::CONTROL
}
fn mods_ctrl_shift() -> ModifiersState {
    ModifiersState::CONTROL | ModifiersState::SHIFT
}
fn mods_alt() -> ModifiersState {
    BINDING_ALT
}
fn mods_none() -> ModifiersState {
    ModifiersState::empty()
}
fn mods_ctrl_alt() -> ModifiersState {
    ModifiersState::CONTROL | BINDING_ALT
}
fn mods_alt_shift() -> ModifiersState {
    BINDING_ALT | ModifiersState::SHIFT
}
fn k_char(s: &str) -> Key {
    Key::Character(SmolStr::new(s))
}
fn k_named(n: NamedKey) -> Key {
    Key::Named(n)
}

#[test]
fn parse_simple_modifier_plus_key() {
    let p = parse_binding("ctrl+a").unwrap();
    assert!(p.ctrl && !p.shift && !p.alt);
    assert_eq!(p.key, "a");
}

#[test]
fn parse_double_plus_is_plus_key() {
    let p = parse_binding("ctrl++").unwrap();
    assert!(p.ctrl && !p.shift && !p.alt);
    assert_eq!(p.key, "+");
}

#[test]
fn parse_minus_and_equals() {
    assert_eq!(parse_binding("ctrl+-").unwrap().key, "-");
    assert_eq!(parse_binding("ctrl+=").unwrap().key, "=");
}

#[test]
fn parse_plus_alias_is_canonical() {
    let p = parse_binding("ctrl+plus").unwrap();
    assert_eq!(p.key, "plus");
}

#[test]
fn parse_empty_is_rejected() {
    assert!(parse_binding("").is_none());
}

#[test]
fn parse_trailing_plus_is_rejected() {
    assert!(parse_binding("ctrl+").is_none());
}

#[test]
fn parse_modifier_only_is_rejected() {
    assert!(parse_binding("ctrl").is_none());
    assert!(parse_binding("shift").is_none());
    assert!(parse_binding("alt").is_none());
}

#[test]
fn parse_accepts_any_modifier_order() {
    let p1 = parse_binding("ctrl+shift+a").unwrap();
    let p2 = parse_binding("shift+ctrl+a").unwrap();
    assert_eq!((p1.ctrl, p1.shift, p1.key), (true, true, "a"));
    assert_eq!((p2.ctrl, p2.shift, p2.key), (true, true, "a"));
}

#[test]
fn parse_is_case_insensitive_for_modifiers() {
    let p = parse_binding("CTRL+A").unwrap();
    assert!(p.ctrl);
    assert_eq!(p.key, "A");
}

#[test]
fn ctrl_alone_does_not_match_any_binding() {
    let key = k_named(NamedKey::Control);
    for binding in ["ctrl++", "ctrl+=", "ctrl+plus", "ctrl+a", "ctrl+shift+="] {
        assert!(
            !matches_binding(binding, &key, mods_ctrl()),
            "binding {binding:?}가 Ctrl 단독에 매칭되면 안 된다"
        );
    }
}

#[test]
fn shift_alone_does_not_match_any_binding() {
    let key = k_named(NamedKey::Shift);
    assert!(!matches_binding("shift+a", &key, ModifiersState::SHIFT));
}

#[test]
fn alt_alone_does_not_match_any_binding() {
    let key = k_named(NamedKey::Alt);
    assert!(!matches_binding("alt+a", &key, ModifiersState::ALT));
}

#[test]
fn plus_key_matches_ctrl_plus_binding() {
    let key = k_char("+");
    assert!(matches_binding("ctrl++", &key, mods_ctrl()));
}

#[test]
fn plus_alias_matches_plus_character() {
    let key = k_char("+");
    assert!(matches_binding("ctrl+plus", &key, mods_ctrl()));
}

#[test]
fn plus_character_matches_plus_alias_and_literal() {
    let key = k_char("+");
    assert!(matches_binding("ctrl+plus", &key, mods_ctrl()));
    assert!(matches_binding("ctrl++", &key, mods_ctrl()));
}

#[test]
fn equals_key_matches_ctrl_equals_binding() {
    let key = k_char("=");
    assert!(matches_binding("ctrl+=", &key, mods_ctrl()));
    assert!(matches_binding("ctrl+equals", &key, mods_ctrl()));
}

#[test]
fn minus_key_matches_ctrl_minus_binding() {
    let key = k_char("-");
    assert!(matches_binding("ctrl+-", &key, mods_ctrl()));
    assert!(matches_binding("ctrl+minus", &key, mods_ctrl()));
}

#[test]
fn shift_requirement_is_enforced() {
    let key = k_char("+");
    assert!(!matches_binding("ctrl++", &key, mods_ctrl_shift()));
    let eq = k_char("=");
    assert!(matches_binding("ctrl+shift+=", &eq, mods_ctrl_shift()));
    assert!(!matches_binding("ctrl+shift+=", &eq, mods_ctrl()));
}

#[test]
fn letter_matches_both_char_and_control_char() {
    // Ctrl+letter가 0x01-0x1A로 도착해도 매칭.
    let ctrl_a = k_char("\u{1}"); // Ctrl+A = 0x01
    assert!(matches_binding("ctrl+a", &ctrl_a, mods_ctrl()));
    let plain_a = k_char("a");
    assert!(matches_binding("ctrl+a", &plain_a, mods_ctrl()));
}

#[test]
fn no_modifier_binding_does_not_match_when_ctrl_held() {
    let key = k_char("a");
    assert!(matches_binding("a", &key, mods_none()));
    assert!(!matches_binding("a", &key, mods_ctrl()));
}

#[test]
fn empty_binding_never_matches() {
    let key = k_char("a");
    assert!(!matches_binding("", &key, mods_none()));
}

#[test]
fn named_key_without_mapping_never_matches_empty() {
    let key = k_named(NamedKey::Control);
    assert!(!matches_binding("ctrl+a", &key, mods_ctrl()));
}

// IME가 논리 키를 바꿔도 physical_key로 US 키 위치를 판별한다.

fn code(c: KeyCode) -> PhysicalKey {
    PhysicalKey::Code(c)
}

#[test]
fn physical_letters_map_to_lowercase_char() {
    for (kc, expected) in [
        (KeyCode::KeyA, "a"),
        (KeyCode::KeyC, "c"),
        (KeyCode::KeyM, "m"),
        (KeyCode::KeyZ, "z"),
    ] {
        assert_eq!(
            physical_key_to_logical(&code(kc)),
            Some(Key::Character(expected.into())),
            "{kc:?} should map to {expected:?}"
        );
    }
}

#[test]
fn physical_digits_map_to_digit_char() {
    for (kc, expected) in [
        (KeyCode::Digit0, "0"),
        (KeyCode::Digit1, "1"),
        (KeyCode::Digit9, "9"),
    ] {
        assert_eq!(
            physical_key_to_logical(&code(kc)),
            Some(Key::Character(expected.into())),
            "{kc:?} should map to {expected:?}"
        );
    }
}

#[test]
fn physical_punctuation_maps_to_symbol_char() {
    for (kc, expected) in [
        (KeyCode::Minus, "-"),
        (KeyCode::Equal, "="),
        (KeyCode::Slash, "/"),
        (KeyCode::Backslash, "\\"),
    ] {
        assert_eq!(
            physical_key_to_logical(&code(kc)),
            Some(Key::Character(expected.into())),
            "{kc:?} should map to {expected:?}"
        );
    }
}

#[test]
fn non_character_physical_keys_return_none() {
    assert_eq!(physical_key_to_logical(&code(KeyCode::Enter)), None);
    assert_eq!(physical_key_to_logical(&code(KeyCode::Space)), None);
    assert_eq!(physical_key_to_logical(&code(KeyCode::F1)), None);
    assert_eq!(physical_key_to_logical(&code(KeyCode::ArrowUp)), None);
}

#[test]
fn unidentified_physical_key_returns_none() {
    use winit::keyboard::NativeKeyCode;
    assert_eq!(
        physical_key_to_logical(&PhysicalKey::Unidentified(NativeKeyCode::Unidentified)),
        None
    );
}

// 실행 중인 플랫폼의 modifier 매핑을 검증한다.

#[test]
#[cfg(not(target_os = "macos"))]
fn alt_binding_matches_alt_modifier_on_non_macos() {
    let key = k_char("t");
    assert!(matches_binding("alt+t", &key, ModifiersState::ALT));
    assert!(!matches_binding("alt+t", &key, mods_none()));
    assert!(!matches_binding("t", &key, ModifiersState::ALT));
}

#[test]
#[cfg(not(target_os = "macos"))]
fn option_binding_matches_win_super_on_non_macos() {
    let key = k_char("t");
    assert!(matches_binding("option+t", &key, ModifiersState::SUPER));
    assert!(!matches_binding("option+t", &key, ModifiersState::ALT));
    assert!(!matches_binding("option+t", &key, mods_none()));
    assert!(!matches_binding("t", &key, ModifiersState::SUPER));
}

fn fresh_state() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    fixture(false)
}

fn fixture(
    categories: bool,
) -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let mut events = vec![E::CategoryCreated {
        id: 0,
        name: "normal".into(),
        index: 0,
    }];
    if categories {
        for id in 1..=2 {
            events.push(E::CategoryCreated {
                id,
                name: format!("category {id}"),
                index: id as usize,
            });
        }
    }
    for id in 1..=3 {
        events.push(E::WorkspaceCreated {
            id,
            name: format!("workspace {id}"),
            category: if categories { id - 1 } else { 0 },
            index: (id - 1) as usize,
            pane: id,
        });
        for index in 0..3 {
            let tab = id * 10 + index;
            events.push(E::TabCreated {
                id: tab,
                pane: id,
                index: index as usize,
                name: "terminal".into(),
                surface: SurfaceSpec {
                    id: tab,
                    kind: "terminal".into(),
                    data: None,
                },
            });
        }
    }
    let (state, mut engine) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    engine.runtime.settings.general.workspace_categories_enabled = categories;
    engine.runtime.settings.keybindings = crate::settings::KeybindingSettings::default();
    (state, engine)
}

#[test]
fn zoom_shortcuts_queue_relative_changes_without_mutating_settings() {
    use crate::app::engine_action::{SettingsPatch, ZoomChange};
    for (key, expected) in [
        ("=", ZoomChange::In),
        ("-", ZoomChange::Out),
        ("0", ZoomChange::Reset),
    ] {
        let (mut state, mut owner) = fresh_state();
        let engine = owner.borrow_mut();
        engine.runtime.settings.appearance.terminal_font.font_size = Some(20.0);
        let target = state.command_target(&engine.read());
        assert!(MainView::handle_zoom_shortcut(
            &mut state,
            &engine.read(),
            &k_char(key),
            ModifiersState::CONTROL,
            &target
        ));
        assert_eq!(
            engine.runtime.settings.appearance.terminal_font.font_size,
            Some(20.0)
        );
        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        assert!(intents[0].origin.is_user());
        let crate::intent::Intent::PatchSettings(SettingsPatch::FontSize { kind, change }) =
            &intents[0].body
        else {
            panic!("font change request")
        };
        assert!(kind.is_none());
        assert!(matches!(
            (change, expected),
            (ZoomChange::In, ZoomChange::In)
                | (ZoomChange::Out, ZoomChange::Out)
                | (ZoomChange::Reset, ZoomChange::Reset)
        ));
    }
}

#[test]
fn repeated_zoom_input_keeps_each_relative_request() {
    use crate::app::engine_action::{SettingsPatch, ZoomChange};
    let (mut state, mut owner) = fresh_state();
    let engine = owner.borrow_mut();
    for _ in 0..3 {
        let target = state.command_target(&engine.read());
        assert!(MainView::handle_zoom_shortcut(
            &mut state,
            &engine.read(),
            &k_char("="),
            ModifiersState::CONTROL,
            &target
        ));
    }
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 3);
    assert!(intents.iter().all(|intent| matches!(
        &intent.body,
        crate::intent::Intent::PatchSettings(SettingsPatch::FontSize {
            kind: None,
            change: ZoomChange::In
        })
    )));
}

#[test]
fn custom_tab_slot_key_switches_correct_tab() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.goto_tab_in_pane(&engine.read(), 0);
    let mut kb = crate::settings::KeybindingSettings::default();
    kb.set_tab_slot_key(2, "q");
    let target = state.command_target(&engine.read());
    let consumed = MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("q"),
        mods_ctrl(),
        true,  // ctrl
        false, // shift
        false, // alt
        false, // option
        &target,
    );
    assert!(consumed);
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        2
    );
}

#[test]
fn tab_next_prev_keys_cycle_focused_pane_tabs() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.goto_tab_in_pane(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default(); // next="l", prev="h", modifier ctrl
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("l"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        1
    );
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("h"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        0
    );
}

#[test]
fn workspace_next_prev_keys_trigger_category_switch() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.switch_workspace(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default(); // next="j", prev="k", modifier alt
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("j"),
        mods_alt(),
        false,
        false,
        true,  // alt
        false, // option
        &target,
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("k"),
        mods_alt(),
        false,
        false,
        true,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 0);
}

#[test]
fn workspace_slot_key_switches_workspace() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.switch_workspace(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default(); // slot "2" = index 1
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("2"),
        mods_alt(),
        false,
        false,
        true,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
}

#[test]
fn wrong_modifier_and_unbound_key_return_false() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.goto_tab_in_pane(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default();
    let before = state
        .navigation
        .tab_index(state.focused_pane(&engine).unwrap());
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("1"),
        mods_none(),
        false,
        false,
        false,
        false,
        &target
    ));
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("z"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        before
    );
}

#[test]
fn category_combo_routes_to_category_switch() {
    let (mut state, mut engine_session) = fixture(true);
    let engine = engine_session.borrow_mut();
    engine.runtime.settings.general.workspace_categories_enabled = true;
    state.switch_workspace(&engine.read(), 0); // active = ws0 (normal)
    let kb = crate::settings::KeybindingSettings::default(); // cat=ctrl+shift, slot "2"=섹션 index 1
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("2"),
        mods_ctrl_shift(),
        true,  // ctrl
        true,  // shift
        false, // alt
        false, // option
        &target,
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
}

#[test]
fn category_next_prev_keys_cycle_categories() {
    let (mut state, mut engine_session) = fixture(true);
    let engine = engine_session.borrow_mut();
    engine.runtime.settings.general.workspace_categories_enabled = true;
    state.switch_workspace(&engine.read(), 0); // active = ws0 (normal)
    let kb = crate::settings::KeybindingSettings::default();

    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 2);
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("k"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
}

#[test]
fn category_next_prev_keys_noop_when_folders_disabled() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    engine.runtime.settings.general.workspace_categories_enabled = false;
    state.switch_workspace(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default();
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 0);
}

#[test]
fn individual_tab_axis_slot_and_next_prev_dispatch() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.goto_tab_in_pane(&engine.read(), 0);
    let mut kb = crate::settings::KeybindingSettings {
        tab_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.set_tab_slot_key(2, "ctrl+alt+q"); // 3번째 탭 슬롯 = 완전 콤보.
    kb.set_tab_next_key("alt+shift+l");
    kb.set_tab_prev_key("alt+shift+h");

    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("q"),
        mods_ctrl_alt(),
        false,
        false,
        false,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        2
    );
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("h"),
        mods_alt_shift(),
        false,
        true,
        true,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        1
    );
    // 개별 지정은 규칙 기반 대상 조회에서 제외한다.
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("1"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
        &target
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        1
    );
}

#[test]
fn individual_workspace_axis_slot_dispatch() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.switch_workspace(&engine.read(), 0);
    let mut kb = crate::settings::KeybindingSettings {
        workspace_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.set_workspace_slot_key(1, "ctrl+alt+w"); // 2번째 워크스페이스(index 1) = 완전 콤보.

    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("w"),
        mods_ctrl_alt(),
        true,
        false,
        true,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);
}

#[test]
fn individual_category_axis_respects_folders_gate() {
    let (mut state, mut engine_session) = fixture(true);
    let engine = engine_session.borrow_mut();
    engine.runtime.settings.general.workspace_categories_enabled = true;
    state.switch_workspace(&engine.read(), 0);
    let mut kb = crate::settings::KeybindingSettings {
        category_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.category_switch_slot_keys[1] = "ctrl+alt+shift+s".to_string(); // 섹션 index 1.

    let mods = ModifiersState::CONTROL | BINDING_ALT | ModifiersState::SHIFT;
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("s"),
        mods,
        true,
        true,
        true,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 1);

    state.switch_workspace(&engine.read(), 0);
    engine.runtime.settings.general.workspace_categories_enabled = false;
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("s"),
        mods,
        true,
        true,
        true,
        false,
        &target
    ));
    assert_eq!(state.active_workspace_index(&engine), 0);
}

#[test]
fn axis_combos_do_not_cross_route() {
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    state.goto_tab_in_pane(&engine.read(), 0);
    let kb = crate::settings::KeybindingSettings::default();
    let before = state
        .navigation
        .tab_index(state.focused_pane(&engine).unwrap());
    let target = state.command_target(&engine.read());
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("2"),
        mods_ctrl_shift(),
        true,  // ctrl
        true,  // shift
        false, // alt
        false, // option
        &target,
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        before
    );
    let target = state.command_target(&engine.read());
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &engine.read(),
        &kb,
        &k_char("2"),
        mods_ctrl(),
        true,  // ctrl
        false, // shift
        false, // alt
        false, // option
        &target,
    ));
    assert_eq!(
        state
            .navigation
            .tab_index(state.focused_pane(&engine).unwrap()),
        1
    );
}

fn default_new_workspace_key_mods() -> (Key, ModifiersState) {
    let kb = crate::settings::KeybindingSettings::default();
    kb.new_workspace
        .first()
        .and_then(|b| tasty_key_match::parse_binding(b))
        .map(|p| {
            let mut mods = ModifiersState::empty();
            if p.ctrl {
                mods |= ModifiersState::CONTROL;
            }
            if p.shift {
                mods |= ModifiersState::SHIFT;
            }
            if p.alt {
                mods |= BINDING_ALT;
            }
            #[cfg(target_os = "macos")]
            if p.option {
                mods |= BINDING_OPTION;
            }
            (k_char(p.key), mods)
        })
        .expect("default new_workspace binding must parse")
}

#[test]
fn focused_workspace_category_returns_active_workspace_category() {
    let (mut state, mut engine_session) = fixture(true);
    let engine = engine_session.borrow_mut();
    let work = 1;
    state.switch_workspace(&engine.read(), 1);

    assert_eq!(
        super::focused_workspace_category(&state, &engine.read()),
        Some(work)
    );
}

#[test]
fn focused_workspace_category_is_none_when_parked() {
    let (state, mut owner) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
            tasty_core::DomainEvent::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
        ]));
    let engine = owner.borrow_mut();
    assert_eq!(
        super::focused_workspace_category(&state, &engine.read()),
        None
    );
}

#[test]
fn shortcut_new_workspace_inherits_active_category() {
    let (mut state, mut engine_session) = fixture(true);
    let engine = engine_session.borrow_mut();
    let work = 1;
    state.switch_workspace(&engine.read(), 1);

    let kb = crate::settings::KeybindingSettings::default();
    let (key, mods) = default_new_workspace_key_mods();

    let target = state.command_target(&engine.read());
    assert!(MainView::match_create_bindings(
        &mut state,
        &engine.read(),
        &target,
        &kb,
        &key,
        mods
    ));

    let intents = state.take_pending_intents();
    let category = intents.into_iter().find_map(|i| match i.body {
        crate::intent::Intent::NewWorkspace { category, .. } => Some(category),
        _ => None,
    });
    assert_eq!(category, Some(Some(work)));
}

#[test]
fn shortcut_new_workspace_stays_normal_when_categories_off() {
    // 카테고리가 꺼져 있으면 새 워크스페이스도 기본 카테고리를 사용한다.
    let (mut state, mut engine_session) = fresh_state();
    let engine = engine_session.borrow_mut();
    assert!(!engine.runtime.settings.general.workspace_categories_enabled);

    let kb = crate::settings::KeybindingSettings::default();
    let (key, mods) = default_new_workspace_key_mods();

    let target = state.command_target(&engine.read());
    assert!(MainView::match_create_bindings(
        &mut state,
        &engine.read(),
        &target,
        &kb,
        &key,
        mods
    ));

    let intents = state.take_pending_intents();
    let category = intents.into_iter().find_map(|i| match i.body {
        crate::intent::Intent::NewWorkspace { category, .. } => Some(category),
        _ => None,
    });
    assert_eq!(category, Some(Some(crate::model::NORMAL_CATEGORY_ID)));
}

/// match arm에서 액션 문자열을 모은다. guard의 문자열은 액션 등록으로 세지 않는다.
fn match_arm_names(src: &str, fn_name: &str, scrutinee: &str) -> Vec<String> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    let s = Source::new(src);
    let bodies = s.fn_bodies(fn_name);
    assert!(!bodies.is_empty(), "`fn {fn_name}` 를 못 찾았다");
    let head = format!("match {scrutinee} {{");
    let mut out = Vec::new();
    for body in bodies {
        let mut from = body.start;
        while let Some(k) = s.code[from..body.end].find(&head) {
            let open = from + k + head.len() - 1;
            from = open + 1;
            let close = matching_close(&s.code, open).expect("match 블록이 닫히지 않는다");
            let arms = s
                .match_arms(open..close + 1)
                .unwrap_or_else(|e| panic!("`{fn_name}` 의 팔을 못 읽었다 — {e}"));
            for arm in arms {
                for alt in s.alternatives(&arm.pattern) {
                    if let Some(name) = s.plain_string(&alt) {
                        out.push(name.to_string());
                    }
                }
            }
        }
    }
    out
}

fn dispatchable_action_ids() -> Vec<String> {
    const SRC: &str = include_str!("dispatch.rs");
    let mut out = match_arm_names(SRC, "dispatch_action_by_id", "action_id");
    out.extend(webview_edit_routed_ids(SRC));
    out
}

/// match 앞에서 WebView 편집 액션으로 실행되는 ID. 그 분기가 실제로 `run_webview_edit_on`을 부르고
/// 그 함수가 WebView에 명령을 실행할 때만 센다. 둘 중 하나라도 없으면 빈 목록이다.
fn webview_edit_routed_ids(dispatch_src: &str) -> Vec<String> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    const EDIT_SRC: &str = include_str!("webview_edit.rs");
    let edit = Source::new(EDIT_SRC);
    let executes = edit
        .fn_bodies("run_webview_edit_on")
        .iter()
        .any(|b| edit.code[b.clone()].contains(".execute_edit(action)"));

    let s = Source::new(dispatch_src);
    let routes = s.fn_bodies("dispatch_action_by_id").iter().any(|b| {
        let body = &s.code[b.clone()];
        let dispatch_match = body.find("match action_id {").unwrap_or(body.len());
        let head = "WebViewEditAction::from_field_id(action_id) {";
        body[..dispatch_match].find(head).is_some_and(|k| {
            let open = b.start + k + head.len() - 1;
            matching_close(&s.code, open).is_some_and(|close| {
                s.code[open..close].contains("self.run_webview_edit_on(sid, action)")
            })
        })
    });
    if !(executes && routes) {
        return Vec::new();
    }
    crate::webview::WebViewEditAction::ALL
        .into_iter()
        .map(|a| a.field_id().to_string())
        .collect()
}

/// 편집 경로 판정은 실행 호출이 빠진 분기를 세지 않는다.
#[test]
fn webview_edit_route_requires_the_executing_call() {
    const SRC: &str = include_str!("dispatch.rs");
    assert!(webview_edit_routed_ids(SRC).contains(&"page_undo".to_string()));
    let without_call = SRC.replacen(
        "self.run_webview_edit_on(sid, action)",
        "self.has_webview_edit_on(sid, action)",
        1,
    );
    assert_ne!(without_call, SRC, "변이 대상 호출을 찾지 못했다");
    assert!(webview_edit_routed_ids(&without_call).is_empty());
}

// 등록된 액션이 실행 경로에도 있는지 확인한다.
fn double_tap_registered_and_armed() -> (Vec<String>, Vec<String>) {
    const SRC: &str = include_str!("double_tap.rs");
    // 등록 집합은 설정 녹화와 공유하는 상수다. 앞에서 따로 처리하는 두 동작은 실행 함수 밖이다.
    let registered: Vec<String> = crate::settings::KeybindingSettings::DOUBLE_TAP_FIELDS
        .iter()
        .filter(|f| !matches!(**f, "toggle_settings" | "toggle_notifications"))
        .map(|f| f.to_string())
        .collect();
    for special in ["toggle_settings", "toggle_notifications"] {
        assert!(
            SRC.contains(&format!("has_dt(&kb.{special})")),
            "{special} 의 더블탭 처리를 못 찾았다"
        );
    }

    // run_double_tap_ 접두어를 가진 실행 함수를 모두 읽는다.
    let code = tasty_doc_guards::source_text::mask_non_code(SRC);
    let mut names: Vec<&str> = code
        .match_indices("fn run_double_tap_")
        .map(|(at, _)| {
            let rest = &code[at + "fn ".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    assert!(names.len() > 1, "실행 함수를 못 찾았다: {names:?}");
    let armed = names
        .iter()
        .flat_map(|name| match_arm_names(SRC, name, "action"))
        .collect();
    (registered, armed)
}

#[test]
fn every_listed_palette_action_has_an_execution_arm() {
    let runnable = dispatchable_action_ids();
    assert!(
        runnable.len() > 40,
        "arm 을 {}개밖에 못 읽었다 — 파싱이 깨졌다",
        runnable.len()
    );
    let missing: Vec<&str> = crate::state::command_palette::all_commands(&[])
        .iter()
        .filter_map(|c| match c {
            crate::state::command_palette::PaletteCommand::Host { id, .. } => Some(*id),
            crate::state::command_palette::PaletteCommand::Plugin { .. } => None,
        })
        .filter(|id| !runnable.iter().any(|r| r == id))
        .collect();
    assert!(missing.is_empty(), "팔레트에 뜨지만 실행 불가: {missing:?}");
}

#[test]
fn double_tap_registration_and_arms_agree() {
    let (registered, armed) = double_tap_registered_and_armed();
    assert!(
        registered.len() > 20 && armed.len() > 20,
        "등록 {} · arm {} — 파싱이 깨졌다",
        registered.len(),
        armed.len()
    );
    let unarmed: Vec<&String> = registered.iter().filter(|r| !armed.contains(r)).collect();
    let unregistered: Vec<&String> = armed.iter().filter(|a| !registered.contains(a)).collect();
    assert!(
        unarmed.is_empty() && unregistered.is_empty(),
        "등록됐는데 실행 arm 없음: {unarmed:?} · arm 인데 등록 안 됨: {unregistered:?}"
    );
}

/// 팔레트의 `enter_copy_mode` 는 진입 요청만 남긴다. 같은 실행 안에서 소비하지 않으면
/// 요청이 남아 있다가 다음 단축키(Ctrl+F 등)에서 엉뚱하게 복사 모드에 들어간다.
#[test]
fn palette_copy_mode_request_is_consumed_in_the_same_dispatch() {
    use tasty_doc_guards::match_arms::Source;
    const SRC: &str = include_str!("dispatch.rs");
    let s = Source::new(SRC);
    let bodies = s.fn_bodies("dispatch_action_by_id");
    assert_eq!(
        bodies.len(),
        1,
        "`fn dispatch_action_by_id` 를 하나만 찾아야 한다"
    );
    let body = &s.code[bodies[0].clone()];
    let request = body
        .find("pending_enter_copy_mode = true")
        .expect("팔레트가 진입을 요청하는 자리를 찾지 못했다");
    assert!(
        body[request..].contains("self.try_enter_vi_copy_mode(engine)"),
        "요청 뒤에서 같은 실행 안에 진입을 시도해야 한다"
    );
}
