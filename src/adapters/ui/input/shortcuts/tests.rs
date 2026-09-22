//! shortcuts 모듈 단위 테스트 — binding parsing/matching + zoom 단축키.

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey, SmolStr};

use super::physical_key_to_logical;
use crate::view::main::MainView;
use tasty_key_match::{matches_binding, parse_binding};

/// 바인딩 문자열의 `alt` 토큰이 실제로 요구하는 winit modifier.
///
/// modifier 매핑은 위치 기반이라 macOS 에서 바인딩 `alt` 는 Command(winit
/// `SUPER`) 에, 그 외 플랫폼에서는 Alt 에 대응한다
/// (`docs/design/policies/key-mapping.md`). 아래 헬퍼를 쓰는 테스트들은 매핑
/// 자체가 아니라 축 디스패치를 검증하므로, 타깃에 맞는 modifier 를 돌려준다.
#[cfg(target_os = "macos")]
const BINDING_ALT: ModifiersState = ModifiersState::SUPER;
#[cfg(not(target_os = "macos"))]
const BINDING_ALT: ModifiersState = ModifiersState::ALT;

/// 바인딩 `option` 토큰에 대응하는 modifier — macOS 전용(Option = winit `ALT`).
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

// ── parse_binding 동작 ────────────────────────────────────────────

#[test]
fn parse_simple_modifier_plus_key() {
    let p = parse_binding("ctrl+a").unwrap();
    assert!(p.ctrl && !p.shift && !p.alt);
    assert_eq!(p.key, "a");
}

#[test]
fn parse_double_plus_is_plus_key() {
    // "ctrl++" = Ctrl + `+` 키.
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
    // "ctrl+"처럼 키가 없는 경우.
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

// ── matches_binding: 모디파이어 단독 방어 ─────────────────────────

#[test]
fn ctrl_alone_does_not_match_any_binding() {
    let key = k_named(NamedKey::Control);
    // 어떤 바인딩과도 Ctrl 단독은 매칭되지 않아야 한다.
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

// ── matches_binding: 정상 매칭 경로 ───────────────────────────────

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
    // "ctrl++"는 Shift를 기대하지 않으므로 Ctrl+Shift+<+키>는 매칭 안 됨.
    let key = k_char("+");
    assert!(!matches_binding("ctrl++", &key, mods_ctrl_shift()));
    // 반대로 "ctrl+shift+="는 shift를 요구.
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
    // 가상의 "a" 단독 바인딩 (파서는 허용하지만 의미상 수정자 요구 안 함).
    // Ctrl을 누르고 a를 눌렀는데 바인딩이 "a"뿐이라면 매칭되면 안 됨.
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
    // NamedKey::Control 같이 매핑이 없는 키는 매칭되지 않아야 한다.
    // 과거에는 named_str이 "" 를 반환해서 빈 key_part와 매칭되는 버그가 있었다.
    let key = k_named(NamedKey::Control);
    assert!(!matches_binding("ctrl+a", &key, mods_ctrl()));
}

// ── physical_key_to_logical: IME 조합 중 modifier 폴백 매핑 ─────────
//
// modifier(Ctrl/Cmd/Alt) 가 눌린 동안 IME 가 logical_key 를 조합문자로 덮어써도
// physical key code 로부터 US 레이아웃 기준 base 문자를 복원한다. 이 매핑이
// handle_keyboard_input 의 shortcut_lookup_key / terminal_key / vi_key 폴백을
// 뒷받침한다 — Ctrl+letter, 단축키 매칭, vi 키가 조합문자에 오염되지 않게 한다.

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
    // zoom 단축키(=/-) 등에 쓰이는 기호가 조합 중에도 복원되는지.
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
    // 글자/숫자/기호가 아닌 코드는 None → 호출부가 logical_key 로 폴백한다.
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

// ── matches_binding: 플랫폼 alt/option modifier 매핑 ────────────────
//
// 비-macOS: 바인딩 "alt" → winit alt_key, "option" 바인딩은 절대 불일치.
// (macOS 분기는 이 타깃에서 컴파일되지 않으므로 non-macOS 규칙만 검증.)

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
fn option_binding_never_matches_on_non_macos() {
    let key = k_char("t");
    assert!(!matches_binding("option+t", &key, ModifiersState::ALT));
    assert!(!matches_binding("option+t", &key, mods_none()));
}

// ── handle_zoom_shortcut: surface별 override 갱신 ──────────────────

fn fresh_state() -> (crate::state::AppState, crate::core::CoreState) {
    let waker: crate::terminal::Waker = std::sync::Arc::new(|| {});
    let mut engine = crate::core::CoreState::new(80, 24, waker).unwrap();
    let preset_store = std::sync::Arc::new(std::sync::Mutex::new(
        tasty_presets::PresetStore::load_default(),
    ));
    let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        ));
    let state = crate::state::AppState::new(&mut engine, preset_store, memory);
    (state, engine)
}

#[test]
fn zoom_in_increments_terminal_font_size_override_only() {
    let (mut state, mut engine) = fresh_state();
    // Pin the default so the test is independent of the user's settings file.
    engine.settings.appearance.default_font.font_size = 14.0;
    engine.settings.appearance.terminal_font.font_size = None;
    engine.settings.appearance.plugin_font_overrides.clear();
    let consumed = MainView::handle_zoom_shortcut(
        &mut state,
        &mut engine,
        &k_char("="),
        ModifiersState::CONTROL,
    );
    assert!(consumed);
    let app = &engine.settings.appearance;
    assert_eq!(app.terminal_font.font_size, Some(15.0));
    // Other surfaces remain untouched.
    assert!(!app.plugin_font_overrides.contains_key("markdown"));
    assert!(!app.plugin_font_overrides.contains_key("explorer"));
    // default_font is also untouched.
    assert_eq!(app.default_font.font_size, 14.0);
}

#[test]
fn zoom_out_decrements_terminal_font_size_override() {
    let (mut state, mut engine) = fresh_state();
    engine.settings.appearance.terminal_font.font_size = Some(20.0);
    let consumed = MainView::handle_zoom_shortcut(
        &mut state,
        &mut engine,
        &k_char("-"),
        ModifiersState::CONTROL,
    );
    assert!(consumed);
    assert_eq!(
        engine.settings.appearance.terminal_font.font_size,
        Some(19.0)
    );
}

#[test]
fn zoom_reset_clears_terminal_font_size_override() {
    let (mut state, mut engine) = fresh_state();
    engine.settings.appearance.terminal_font.font_size = Some(20.0);
    let consumed = MainView::handle_zoom_shortcut(
        &mut state,
        &mut engine,
        &k_char("0"),
        ModifiersState::CONTROL,
    );
    assert!(consumed);
    // Reset → override removed (surface returns to default_font).
    assert!(engine.settings.appearance.terminal_font.font_size.is_none());
}

#[test]
fn zoom_in_clamps_at_72px() {
    let (mut state, mut engine) = fresh_state();
    engine.settings.appearance.terminal_font.font_size = Some(71.5);
    MainView::handle_zoom_shortcut(
        &mut state,
        &mut engine,
        &k_char("="),
        ModifiersState::CONTROL,
    );
    assert_eq!(
        engine.settings.appearance.terminal_font.font_size,
        Some(72.0)
    );
}

#[test]
fn zoom_out_clamps_at_6px() {
    let (mut state, mut engine) = fresh_state();
    engine.settings.appearance.terminal_font.font_size = Some(6.5);
    MainView::handle_zoom_shortcut(
        &mut state,
        &mut engine,
        &k_char("-"),
        ModifiersState::CONTROL,
    );
    assert_eq!(
        engine.settings.appearance.terminal_font.font_size,
        Some(6.0)
    );
}

// ── handle_numeric_switch_shortcuts: quick-switch 슬롯/next/prev 배선 ──
//
// 기본 프리셋: tab modifier=ctrl, workspace modifier=alt, tab next/prev="l"/"h",
// workspace next/prev="j"/"k", workspace_categories_enabled=false.

fn add_test_workspace(state: &mut crate::state::AppState, engine: &mut crate::core::CoreState) {
    let event = crate::core::apply_create_workspace_inner(
        engine,
        crate::core::WorkspaceCreationParams::terminal(),
    )
    .unwrap();
    let crate::core::intent::CoreEvent::WorkspaceCreated { index, .. } = event else {
        panic!("apply_create_workspace_inner did not return WorkspaceCreated");
    };
    state.active_workspace = index;
}

#[test]
fn custom_tab_slot_key_switches_correct_tab() {
    let (mut state, mut engine) = fresh_state();
    // focused pane 에 탭 3개 확보 (초기 1 + 2).
    state.add_tab(&mut engine).unwrap();
    state.add_tab(&mut engine).unwrap();
    state.goto_tab_in_pane(&mut engine, 0);
    // 3번째 슬롯(index 2)을 "q" 로 재바인딩.
    let mut kb = crate::settings::KeybindingSettings::default();
    kb.set_tab_slot_key(2, "q");
    // ctrl(기본 tab modifier) + "q" → focused pane 의 3번째 탭(index 2)으로 전환.
    let consumed = MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("q"),
        mods_ctrl(),
        true,  // ctrl
        false, // shift
        false, // alt
        false, // option
    );
    assert!(consumed);
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 2);
}

#[test]
fn tab_next_prev_keys_cycle_focused_pane_tabs() {
    let (mut state, mut engine) = fresh_state();
    state.add_tab(&mut engine).unwrap();
    state.add_tab(&mut engine).unwrap(); // 3 tabs
    state.goto_tab_in_pane(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default(); // next="l", prev="h", modifier ctrl
    // ctrl+l → 다음 탭.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("l"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 1);
    // ctrl+h → 이전 탭.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("h"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 0);
}

#[test]
fn workspace_next_prev_keys_trigger_category_switch() {
    let (mut state, mut engine) = fresh_state();
    add_test_workspace(&mut state, &mut engine); // ws 1
    add_test_workspace(&mut state, &mut engine); // ws 2
    state.switch_workspace(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default(); // next="j", prev="k", modifier alt
    // alt+j → 같은 카테고리(기본 전부 normal) 내 다음 워크스페이스.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("j"),
        mods_alt(),
        false,
        false,
        true,  // alt
        false, // option
    ));
    assert_eq!(state.active_workspace, 1);
    // alt+k → 이전.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("k"),
        mods_alt(),
        false,
        false,
        true,
        false,
    ));
    assert_eq!(state.active_workspace, 0);
}

#[test]
fn workspace_slot_key_switches_workspace() {
    let (mut state, mut engine) = fresh_state();
    add_test_workspace(&mut state, &mut engine); // ws 1
    add_test_workspace(&mut state, &mut engine); // ws 2
    state.switch_workspace(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default(); // slot "2" = index 1
    // alt+"2" → 2번째 워크스페이스(index 1). 카테고리 off → 전역 인덱스.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("2"),
        mods_alt(),
        false,
        false,
        true,
        false,
    ));
    assert_eq!(state.active_workspace, 1);
}

#[test]
fn wrong_modifier_and_unbound_key_return_false() {
    let (mut state, mut engine) = fresh_state();
    state.add_tab(&mut engine).unwrap(); // 2 tabs
    state.goto_tab_in_pane(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default();
    let before = state.focused_pane(&engine).unwrap().active_tab;
    // modifier 없이 "1" → 대상 판정 None → false (맨 키 오검출 없음).
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("1"),
        mods_none(),
        false,
        false,
        false,
        false,
    ));
    // ctrl + "z"(어떤 슬롯/next/prev 도 아님) → false.
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("z"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, before);
}

#[test]
fn category_combo_routes_to_category_switch() {
    // 기본 카테고리 modifier = ctrl+shift(독립 축). ctrl+shift+숫자 → 카테고리 전환.
    let (mut state, mut engine) = fresh_state();
    engine.settings.general.workspace_categories_enabled = true;
    add_test_workspace(&mut state, &mut engine); // ws0 (normal)
    add_test_workspace(&mut state, &mut engine); // ws1
    let cat = engine.create_category("Services").unwrap();
    let ws1_id = engine.workspaces[1].id;
    engine.set_workspace_category(ws1_id, cat).unwrap();
    state.switch_workspace(&mut engine, 0); // active = ws0 (normal)
    let kb = crate::settings::KeybindingSettings::default(); // cat=ctrl+shift, slot "2"=섹션 index 1
    // ctrl+shift+"2" → 섹션 index 1(Services) 로 카테고리 전환 → ws1 착지.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("2"),
        mods_ctrl_shift(),
        true,  // ctrl
        true,  // shift
        false, // alt
        false, // option
    ));
    assert_eq!(state.active_workspace, 1);
}

#[test]
fn category_next_prev_keys_cycle_categories() {
    // 기본 카테고리 modifier = ctrl+shift, next/prev raw 키 = "j"/"k" (S-9).
    let (mut state, mut engine) = fresh_state();
    engine.settings.general.workspace_categories_enabled = true;
    add_test_workspace(&mut state, &mut engine); // ws1
    add_test_workspace(&mut state, &mut engine); // ws2
    let services = engine.create_category("Services").unwrap();
    let extra = engine.create_category("Extra").unwrap();
    let ws1_id = engine.workspaces[1].id;
    let ws2_id = engine.workspaces[2].id;
    engine.set_workspace_category(ws1_id, services).unwrap();
    engine.set_workspace_category(ws2_id, extra).unwrap();
    state.switch_workspace(&mut engine, 0); // active = ws0 (normal)
    let kb = crate::settings::KeybindingSettings::default();

    // ctrl+shift+j → 다음 카테고리(Services) → 미방문이라 first(ws1) 착지.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
    ));
    assert_eq!(state.active_workspace, 1);
    // ctrl+shift+j → 다음 카테고리(Extra) → ws2 착지.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
    ));
    assert_eq!(state.active_workspace, 2);
    // ctrl+shift+k → 이전 카테고리(Services) → ws1 로 복귀.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("k"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
    ));
    assert_eq!(state.active_workspace, 1);
}

#[test]
fn category_next_prev_keys_noop_when_folders_disabled() {
    // folders 기능 off → 카테고리 next/prev 도 슬롯과 동일하게 무시(표시=동작).
    let (mut state, mut engine) = fresh_state();
    engine.settings.general.workspace_categories_enabled = false;
    add_test_workspace(&mut state, &mut engine);
    state.switch_workspace(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default();
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("j"),
        mods_ctrl_shift(),
        true,
        true,
        false,
        false,
    ));
    assert_eq!(state.active_workspace, 0);
}

// ── "개별 지정" 축 디스패치 (S-9) ──────────────────────────────────

#[test]
fn individual_tab_axis_slot_and_next_prev_dispatch() {
    // 탭 축을 개별 지정으로 바꾸고 슬롯/다음/이전에 모디파이어 포함 자유 콤보를 저장.
    let (mut state, mut engine) = fresh_state();
    state.add_tab(&mut engine).unwrap();
    state.add_tab(&mut engine).unwrap(); // 3 tabs
    state.goto_tab_in_pane(&mut engine, 0);
    let mut kb = crate::settings::KeybindingSettings {
        tab_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.set_tab_slot_key(2, "ctrl+alt+q"); // 3번째 탭 슬롯 = 완전 콤보.
    kb.set_tab_next_key("alt+shift+l");
    kb.set_tab_prev_key("alt+shift+h");

    // ctrl+alt+q → 3번째 탭(index 2).
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("q"),
        mods_ctrl_alt(),
        false,
        false,
        false,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 2);
    // alt+shift+h → 이전 탭.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("h"),
        mods_alt_shift(),
        false,
        true,
        true,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 1);
    // 규칙 기반 시절 modifier(ctrl 단독)+슬롯 문자는 이제 안 먹힌다(축이 개별 지정으로
    // 바뀌어 switch_target_for 가 이 축을 절대 반환하지 않음).
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("1"),
        mods_ctrl(),
        true,
        false,
        false,
        false,
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 1);
}

#[test]
fn individual_workspace_axis_slot_dispatch() {
    let (mut state, mut engine) = fresh_state();
    add_test_workspace(&mut state, &mut engine); // ws 1
    add_test_workspace(&mut state, &mut engine); // ws 2
    state.switch_workspace(&mut engine, 0);
    let mut kb = crate::settings::KeybindingSettings {
        workspace_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.set_workspace_slot_key(1, "ctrl+alt+w"); // 2번째 워크스페이스(index 1) = 완전 콤보.

    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("w"),
        mods_ctrl_alt(),
        true,
        false,
        true,
        false,
    ));
    assert_eq!(state.active_workspace, 1);
}

#[test]
fn individual_category_axis_respects_folders_gate() {
    let (mut state, mut engine) = fresh_state();
    engine.settings.general.workspace_categories_enabled = true;
    add_test_workspace(&mut state, &mut engine); // ws0(normal) 이미 있으니 ws1 추가
    let cat = engine.create_category("Services").unwrap();
    let ws1_id = engine.workspaces[1].id;
    engine.set_workspace_category(ws1_id, cat).unwrap();
    state.switch_workspace(&mut engine, 0);
    let mut kb = crate::settings::KeybindingSettings {
        category_switch_modifier: crate::settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            .to_string(),
        ..Default::default()
    };
    kb.category_switch_slot_keys[1] = "ctrl+alt+shift+s".to_string(); // 섹션 index 1.

    let mods = ModifiersState::CONTROL | BINDING_ALT | ModifiersState::SHIFT;
    // folders on → 매칭되어 Services 로 전환.
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("s"),
        mods,
        true,
        true,
        true,
        false,
    ));
    assert_eq!(state.active_workspace, 1);

    // folders off → 개별 지정 콤보가 저장돼 있어도 무시(표시=동작 게이트).
    state.switch_workspace(&mut engine, 0);
    engine.settings.general.workspace_categories_enabled = false;
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("s"),
        mods,
        true,
        true,
        true,
        false,
    ));
    assert_eq!(state.active_workspace, 0);
}

#[test]
fn axis_combos_do_not_cross_route() {
    // ctrl 단독 → Tab, ctrl+shift(카테고리 축) → 탭/워크스페이스로 새지 않음.
    let (mut state, mut engine) = fresh_state();
    state.add_tab(&mut engine).unwrap(); // 2 tabs
    state.goto_tab_in_pane(&mut engine, 0);
    let kb = crate::settings::KeybindingSettings::default();
    let before = state.focused_pane(&engine).unwrap().active_tab;
    // ctrl+shift+"2": categories off → Category 대상이지만 비소비(false), 탭 전환 없음.
    assert!(!MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("2"),
        mods_ctrl_shift(),
        true,  // ctrl
        true,  // shift
        false, // alt
        false, // option
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, before);
    // ctrl 단독+"2" → 탭 전환 정상(2번째 탭 = index 1).
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &mut engine,
        &kb,
        &k_char("2"),
        mods_ctrl(),
        true,  // ctrl
        false, // shift
        false, // alt
        false, // option
    ));
    assert_eq!(state.focused_pane(&engine).unwrap().active_tab, 1);
}

// ── new_workspace 단축키가 현재 활성 카테고리를 계승하는지
// (`docs/features/workspace-category/index.md` 참고) ──────────

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
    let (mut state, mut engine) = fresh_state();
    let work = engine.create_category("Work").unwrap();
    add_test_workspace(&mut state, &mut engine); // ws1, 아직 normal
    let ws1_id = engine.workspaces[1].id;
    engine.set_workspace_category(ws1_id, work).unwrap();
    state.switch_workspace(&mut engine, 1);

    assert_eq!(
        super::focused_workspace_category(&state, &engine),
        Some(work)
    );
}

#[test]
fn focused_workspace_category_is_none_when_parked() {
    let (state, mut engine) = fresh_state();
    engine.workspaces.clear(); // parked 상태 (마지막 윈도우가 닫힌 뒤) 재현.
    assert_eq!(super::focused_workspace_category(&state, &engine), None);
}

#[test]
fn shortcut_new_workspace_inherits_active_category() {
    let (mut state, mut engine) = fresh_state();
    let work = engine.create_category("Work").unwrap();
    add_test_workspace(&mut state, &mut engine); // ws1
    let ws1_id = engine.workspaces[1].id;
    engine.set_workspace_category(ws1_id, work).unwrap();
    state.switch_workspace(&mut engine, 1);

    let kb = crate::settings::KeybindingSettings::default();
    let (key, mods) = default_new_workspace_key_mods();

    assert!(MainView::match_create_bindings(
        &mut state,
        &mut engine,
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
    // 카테고리 토글 off 상태(기본 = normal 하나뿐)에서도 회귀 없이 동작해야 한다 —
    // 활성 워크스페이스가 항상 normal 이므로 Some(NORMAL_CATEGORY_ID) 가 나오고,
    // apply_create_workspace_inner 입장에서는 기존 None 과 동일한 결과(normal)다.
    let (mut state, mut engine) = fresh_state();
    assert!(!engine.settings.general.workspace_categories_enabled);

    let kb = crate::settings::KeybindingSettings::default();
    let (key, mods) = default_new_workspace_key_mods();

    assert!(MainView::match_create_bindings(
        &mut state,
        &mut engine,
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

// ── 목록과 실행 테이블의 대조 ────────────────────────────────────
//
// 이 모듈의 두 시험은 **소스 텍스트를 읽는다.** 재는 대상이 `match` 의 arm 집합인데
// 그것을 값으로 꺼내는 길이 없어서다(arm 을 상수 배열에서 유도하도록 바꾸면 그 배열이
// 또 하나의 사본이 된다 — 이 티켓이 고친 병이 그것이다). 읽는 자리를 함수 본문의 `match`
// 로 좁히고, 팔은 공용 판정기(`tasty_doc_guards::match_arms`)가 뗀다 — 주석·문자열은 구조로
// 안 읽히고, `"a" | "b" =>` 와 guard 가 붙은 팔도 팔이다(줄 앞머리의 `"id" =>` 만 세던
// 판독은 그 둘을 놓쳐 "arm 인데 등록 안 됨" 방향이 조용히 초록이었다).

/// `fn <fn_name>` 정의들 안의 `match <scrutinee> { … }` 팔 이름(따옴표 이름만).
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

/// `dispatch_action_by_id` 의 match arm 이 아는 action_id 집합.
fn dispatchable_action_ids() -> Vec<String> {
    const SRC: &str = include_str!("dispatch.rs");
    match_arm_names(SRC, "dispatch_action_by_id", "action_id")
}

/// `handle_double_tap_shortcut` 의 등록 목록(`bindings_to_check`)과 `run_double_tap_*_action`
/// 의 실행 arm. 둘은 인지 복잡도 때문에 다른 함수로 갈라져 있지만 같은 파일 안이다.
fn double_tap_registered_and_armed() -> (Vec<String>, Vec<String>) {
    const SRC: &str = include_str!("double_tap.rs");
    let list_start = SRC
        .find("let bindings_to_check")
        .expect("bindings_to_check 를 못 찾았다");
    let list_end = SRC[list_start..].find("];").expect("목록 끝") + list_start;
    let registered: Vec<String> = SRC[list_start..list_end]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("(&kb.")?;
            let (_, after) = rest.split_once(", \"")?;
            Some(after.split('"').next()?.to_string())
        })
        .collect();

    // 실행 표는 `run_double_tap_*_action` 여럿에 나뉘어 있다(인지 복잡도 상한 때문이다).
    // 이름을 손으로 적지 않고 그 접두어의 함수를 전부 읽어, 갈래가 늘어도 이 시험이 따라간다.
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

/// 명령 팔레트에 뜨는 액션은 전부 실행 arm 이 있다.
///
/// 목록은 SoT 순회로 자동으로 늘고 실행은 손으로 늘려야 해서, 새 필드가 들어올 때마다
/// 격차가 벌어졌다 — 이 시험을 넣기 직전 값이 **14** 였다(고르면 키캡까지 보여 주고는
/// `tracing::warn!` 한 줄만 남기고 아무 일도 안 했다).
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

/// double-tap 등록 목록과 실행 arm 이 양방향으로 일치한다.
///
/// 두 손 나열이 같은 함수 안에서 갈라져 있어 한쪽만 늘려도 컴파일이 통과했다. 이 시험을
/// 넣기 직전 값은 등록만 1(`open_explorer` — 키를 먹고 아무 일도 안 했다) ·
/// arm 만 4(`restore_closed`/`quit`/`quit_immediate`/`quit_minimize` — 죽은 코드)였다.
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
