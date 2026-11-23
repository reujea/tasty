use super::*;
use crate::adapters::ui::input::key_level::{KEYBINDING_LEVELS, PathLevels, keybinding_level};

/// 모든 단축키 필드에 바인딩 하나씩을 준 설정. 숫자 전환은 수식키 모드다.
fn every_field_bound() -> KeybindingSettings {
    let mut kb = KeybindingSettings::default();
    for (field, level) in KEYBINDING_LEVELS {
        if matches!(level, FieldLevel::Level(_)) && *field != "script_bindings" {
            assert!(kb.set_field(field, "ctrl+alt+shift+f12"), "`{field}`");
        }
    }
    kb.script_bindings
        .push(tasty_settings::keybindings::ScriptBinding {
            combo: "ctrl+alt+shift+f11".to_string(),
            script_id: "s".to_string(),
        });
    kb
}

fn bound(kb: &KeybindingSettings, field: &str) -> bool {
    if field == "script_bindings" {
        return !kb.script_bindings.is_empty();
    }
    kb.get_bindings(field).is_some_and(|b| !b.is_empty())
}

/// 경로에 없는 level의 필드는 비고, 경로에 있는 level의 필드는 그대로다.
#[test]
fn the_path_keybindings_keep_only_fields_on_the_path() {
    let kb = every_field_bound();
    let path = PathLevels::engine_only().with(KeyLevel::Window);
    let filtered = path_keybindings(&kb, Some(path));
    let mut checked = 0;
    for (field, level) in KEYBINDING_LEVELS {
        let FieldLevel::Level(l) = level else {
            continue;
        };
        assert!(bound(&kb, field), "`{field}` 픽스처에 바인딩이 없다");
        assert_eq!(
            bound(&filtered, field),
            matches!(l, KeyLevel::Engine | KeyLevel::Window),
            "`{field}` ({l:?})"
        );
        checked += 1;
    }
    assert!(checked > 90, "{checked}개만 봤다");
}

/// 래치 중(경로 없음)에는 아무것도 거르지 않는다.
#[test]
fn no_path_keeps_every_field() {
    let kb = every_field_bound();
    assert_eq!(path_keybindings(&kb, None), kb);
}

/// 경로에 없는 숫자 전환 축은 두 모드(수식키·개별 키)가 함께 꺼진다. 경로에 있는 축은 그대로다.
#[test]
fn a_numeric_axis_off_the_path_turns_both_modes_off() {
    let kb = KeybindingSettings {
        tab_switch_modifier: "ctrl".to_string(),
        workspace_switch_modifier: "alt".to_string(),
        tab_switch_next_key: "ctrl+f9".to_string(),
        workspace_switch_next_key: "ctrl+f10".to_string(),
        ..Default::default()
    };
    let path = PathLevels::engine_only()
        .with(KeyLevel::Window)
        .with(KeyLevel::Workspace);
    let filtered = path_keybindings(&kb, Some(path));
    assert_eq!(
        filtered.tab_switch_modifier,
        KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
    );
    assert!(filtered.tab_switch_slot_keys.iter().all(String::is_empty));
    assert!(filtered.tab_switch_next_key.is_empty());
    assert_eq!(filtered.workspace_switch_modifier, "alt");
    assert_eq!(filtered.workspace_switch_next_key, "ctrl+f10");
}

/// T-12a-1: 스크립트 단축키는 Window level이다. Window가 없는 경로에서는 걸러진다.
#[test]
fn a_script_shortcut_is_a_window_candidate() {
    assert_eq!(
        keybinding_level("script_bindings"),
        Some(FieldLevel::Level(KeyLevel::Window))
    );
    let window = PathLevels::engine_only().with(KeyLevel::Window);
    assert!(allows(Some(window), KeyLevel::Window));
    assert!(!allows(Some(PathLevels::engine_only()), KeyLevel::Window));
    let kb = every_field_bound();
    assert!(
        path_keybindings(&kb, Some(PathLevels::engine_only()))
            .script_bindings
            .is_empty()
    );
    assert!(
        !path_keybindings(&kb, Some(window))
            .script_bindings
            .is_empty()
    );
}

/// 경로가 없으면(래치 중) 어느 level도 거르지 않는다.
#[test]
fn no_path_allows_every_level() {
    for level in KeyLevel::ALL {
        assert!(allows(None, level));
    }
    let kb = every_field_bound();
    assert_eq!(path_keybindings(&kb, None).new_tab, kb.new_tab);
}

/// 덮는 범위 아래 level은 빼고, 덮지 않으면 붙은 범위부터다.
#[test]
fn the_scope_levels_start_at_the_scope_or_its_parent() {
    let pane = from_level(KeyLevel::Pane, true);
    for level in KeyLevel::ALL {
        assert_eq!(
            pane.contains(level),
            !matches!(level, KeyLevel::Tab | KeyLevel::Surface),
            "{level:?}"
        );
    }
    let above_pane = from_level(KeyLevel::Pane, false);
    assert!(!above_pane.contains(KeyLevel::Pane));
    assert!(above_pane.contains(KeyLevel::Workspace));
    assert_eq!(
        from_level(KeyLevel::Window, false),
        PathLevels::engine_only()
    );
    assert_eq!(from_level(KeyLevel::Surface, true), PathLevels::all());
}

/// 포커스 surface 출발 경로는 모든 level, 래치 중이면 경로가 없다.
#[test]
fn a_surface_focus_path_has_every_level_and_the_latch_has_none() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert!(matches!(
        state.focus_node(&read),
        FocusNode::Surface(Some(_))
    ));
    assert_eq!(focus_path_levels(&state, &read), Some(PathLevels::all()));
    state.settings_open_requested = true;
    assert_eq!(state.focus_node(&read), FocusNode::SettingsRequest);
    assert_eq!(focus_path_levels(&state, &read), None);
}

/// 다른 surface 의 검색 띠: 명령 대상은 모델 포커스 surface(pane 20 의 201)이고 포커스 팝업은 pane 10 의 surface 101
/// 에 붙어 있다. 경로는 두 자리가 함께 가진 노드(Workspace·Window)뿐이라, 명령 대상에서 실행될 Surface·Tab·Pane
/// 단축키(`close_surface` 등)는 걸러진다.
#[cfg(feature = "gui")]
#[test]
fn a_popup_on_another_surface_keeps_only_the_shared_levels() {
    use crate::adapters::ui::popup::PopupScope;
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert_eq!(state.focused_surface_id(&read), Some(201));
    let iid = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(101))
                .focused(),
            true,
        )
        .expect("검색 띠가 열린다");
    state.normalize_focus(&read);
    assert_eq!(state.focused_surface_id(&read), Some(201));
    assert_eq!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(iid))
    );
    let path = focus_path_levels(&state, &read).expect("경로");
    for level in [KeyLevel::Surface, KeyLevel::Tab, KeyLevel::Pane] {
        assert!(!path.contains(level), "{level:?}");
    }
    assert!(path.contains(KeyLevel::Workspace));
    let kb = KeybindingSettings {
        close_surface: vec!["ctrl+f9".into()],
        ..Default::default()
    };
    assert!(path_keybindings(&kb, Some(path)).close_surface.is_empty());
}
