//! `keybindings` 단위 테스트.

use super::*;
use crate::general::GeneralSettings;
use std::collections::HashSet;

#[test]
fn preset_by_name_matches_preset_tasty() {
    let by_name = KeybindingSettings::preset_by_name("Tasty").unwrap();
    let direct = KeybindingSettings::preset_tasty();
    for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        assert_eq!(
            by_name.get_bindings(id),
            direct.get_bindings(id),
            "field {id} mismatch"
        );
    }
    assert!(KeybindingSettings::preset_by_name("Unknown").is_none());
}

#[test]
fn preset_by_name_matches_preset_mac() {
    let by_name = KeybindingSettings::preset_by_name("Mac").unwrap();
    let direct = KeybindingSettings::preset_mac();
    for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        assert_eq!(
            by_name.get_bindings(id),
            direct.get_bindings(id),
            "field {id} mismatch"
        );
    }
}

#[test]
fn preset_by_name_matches_preset_windows() {
    let by_name = KeybindingSettings::preset_by_name("Windows").unwrap();
    let direct = KeybindingSettings::preset_windows();
    for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        assert_eq!(
            by_name.get_bindings(id),
            direct.get_bindings(id),
            "field {id} mismatch"
        );
    }
}

#[test]
fn preset_by_name_matches_preset_linux() {
    let by_name = KeybindingSettings::preset_by_name("Linux").unwrap();
    let direct = KeybindingSettings::preset_linux();
    for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        assert_eq!(
            by_name.get_bindings(id),
            direct.get_bindings(id),
            "field {id} mismatch"
        );
    }
}

#[test]
fn preset_names_lists_all_four() {
    let names = KeybindingSettings::preset_names();
    assert_eq!(names, &["Tasty", "Mac", "Windows", "Linux"]);
}

/// 각 프리셋 내부에 바인딩 충돌이 없는지 검증.
#[test]
fn no_conflicts_within_presets() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            if let Some(bindings) = kb.get_bindings(id) {
                for combo in bindings {
                    if combo.is_empty() {
                        continue;
                    }
                    let conflict = kb.find_conflict(id, combo);
                    assert_eq!(
                        conflict, None,
                        "preset '{name}': field '{id}' combo '{combo}' conflicts with {:?}",
                        conflict
                    );
                }
            }
        }
    }
}

#[test]
fn find_conflict_detects_duplicate_across_fields() {
    let kb = KeybindingSettings::preset_tasty();
    assert_eq!(
        kb.find_conflict("toggle_settings", "ctrl+shift+w"),
        Some(("close_pane", 0))
    );
}

#[test]
fn find_conflict_ignores_self() {
    let kb = KeybindingSettings::preset_tasty();
    assert_eq!(kb.find_conflict("close_pane", "ctrl+shift+w"), None);
}

#[test]
fn find_conflict_empty_combo_is_none() {
    let kb = KeybindingSettings::preset_tasty();
    assert_eq!(kb.find_conflict("close_pane", ""), None);
}

#[test]
fn set_and_get_field_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert!(kb.set_field("new_tab", "alt+x"));
    assert_eq!(kb.get_field("new_tab"), Some("alt+x"));
    assert!(kb.clear_field("new_tab"));
    assert_eq!(kb.get_field("new_tab"), Some(""));
}

#[test]
fn add_remove_binding_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert!(kb.clear_field("copy"));
    assert!(kb.add_binding("copy", "ctrl+c".into()));
    assert!(kb.add_binding("copy", "ctrl+shift+c".into()));
    // 중복 추가는 실패.
    assert!(!kb.add_binding("copy", "ctrl+c".into()));
    assert_eq!(
        kb.get_bindings("copy"),
        Some(&["ctrl+c".to_string(), "ctrl+shift+c".to_string()][..])
    );
    assert!(kb.remove_binding("copy", 0));
    assert_eq!(
        kb.get_bindings("copy"),
        Some(&["ctrl+shift+c".to_string()][..])
    );
    // 범위 밖 idx는 실패.
    assert!(!kb.remove_binding("copy", 5));
}

#[test]
fn add_binding_rejects_empty() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert!(!kb.add_binding("new_tab", String::new()));
}

#[test]
fn set_field_unknown_returns_false() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert!(!kb.set_field("nonexistent", "ctrl+x"));
    assert_eq!(kb.get_field("nonexistent"), None);
}

#[test]
fn general_binding_fields_count() {
    assert_eq!(KeybindingSettings::GENERAL_BINDING_FIELDS.len(), 61);
}

#[test]
fn all_general_fields_have_getters_and_setters() {
    let mut kb = KeybindingSettings::preset_tasty();
    for (id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        assert!(
            kb.get_bindings(id).is_some(),
            "get_bindings missing for {id}"
        );
        assert!(kb.set_field(id, "x"), "set_field missing for {id}");
        assert_eq!(kb.get_field(id), Some("x"));
    }
}

#[test]
fn label_key_for_returns_correct_key() {
    assert_eq!(
        KeybindingSettings::label_key_for("close_pane"),
        Some("settings.keybindings.close_pane_label")
    );
    assert_eq!(
        KeybindingSettings::label_key_for("copy"),
        Some("settings.keybindings.copy_label")
    );
    assert_eq!(KeybindingSettings::label_key_for("nonexistent"), None);
}

/// 0.4 fresh-start: 단일 string 형식은 reject (Vec 필수).
#[test]
fn single_string_keybinding_rejected() {
    let toml_str = r#"new_tab = "alt+x""#;
    let result: Result<KeybindingSettings, _> = toml::from_str(toml_str);
    assert!(
        result.is_err(),
        "single string should be rejected, got: {:?}",
        result
    );
}

// ── format_display: `+` 구분자/키 충돌 해소 ───────────────────────

#[test]
fn format_display_basic() {
    assert_eq!(
        KeybindingSettings::format_display("ctrl+shift+n", &GeneralSettings::default()),
        "Ctrl+Shift+N"
    );
}

#[test]
fn format_display_ctrl_plus_key() {
    // "ctrl++"는 Ctrl + `+` 키. 표시상 "Ctrl++"여야 한다.
    assert_eq!(
        KeybindingSettings::format_display("ctrl++", &GeneralSettings::default()),
        "Ctrl++"
    );
}

#[test]
fn format_display_symbol_aliases() {
    let general = GeneralSettings::default();
    assert_eq!(
        KeybindingSettings::format_display("ctrl+plus", &general),
        "Ctrl++"
    );
    assert_eq!(
        KeybindingSettings::format_display("ctrl+minus", &general),
        "Ctrl+-"
    );
    assert_eq!(
        KeybindingSettings::format_display("ctrl+equals", &general),
        "Ctrl+="
    );
}

#[test]
fn format_display_empty_and_minus() {
    let general = GeneralSettings::default();
    assert_eq!(KeybindingSettings::format_display("", &general), "");
    assert_eq!(
        KeybindingSettings::format_display("ctrl+-", &general),
        "Ctrl+-"
    );
    assert_eq!(
        KeybindingSettings::format_display("ctrl+=", &general),
        "Ctrl+="
    );
}

#[test]
fn format_display_parts_tokenizes() {
    let general = GeneralSettings::default();
    assert_eq!(
        KeybindingSettings::format_display_parts("alt+n", &general),
        vec!["Alt", "N"]
    );
    assert_eq!(
        KeybindingSettings::format_display_parts("ctrl+shift+v", &general),
        vec!["Ctrl", "Shift", "V"]
    );
}

#[test]
fn format_display_parts_plus_key_not_split() {
    // "ctrl++"는 Ctrl + `+`키 — 키캡 2개 [Ctrl][+] 로 분해되어야 한다.
    let general = GeneralSettings::default();
    assert_eq!(
        KeybindingSettings::format_display_parts("ctrl++", &general),
        vec!["Ctrl", "+"]
    );
    assert_eq!(
        KeybindingSettings::format_display_parts("ctrl+plus", &general),
        vec!["Ctrl", "+"]
    );
}

#[test]
fn format_display_parts_empty() {
    assert!(KeybindingSettings::format_display_parts("", &GeneralSettings::default()).is_empty());
}

// ── format_display: macOS 표시 스타일 (alt/option/shift) ───────────────

#[test]
fn format_display_mac_alt_cmd_style() {
    let general = GeneralSettings {
        alt_display_style: "cmd".to_string(),
        ..GeneralSettings::default()
    };
    assert_eq!(
        KeybindingSettings::format_display("alt+n", &general),
        "Cmd+N"
    );
}

#[test]
fn format_display_mac_alt_symbol_style() {
    let general = GeneralSettings {
        alt_display_style: "symbol".to_string(),
        ..GeneralSettings::default()
    };
    assert_eq!(KeybindingSettings::format_display("alt+n", &general), "⌘+N");
}

#[test]
fn format_display_mac_option_symbol_style() {
    let general = GeneralSettings {
        option_display_style: "symbol".to_string(),
        ..GeneralSettings::default()
    };
    assert_eq!(
        KeybindingSettings::format_display("option+n", &general),
        "⌥+N"
    );
}

#[test]
fn format_display_mac_shift_symbol_style() {
    let general = GeneralSettings {
        shift_display_style: "symbol".to_string(),
        ..GeneralSettings::default()
    };
    assert_eq!(
        KeybindingSettings::format_display("shift+n", &general),
        "⇧+N"
    );
}

#[test]
fn format_display_parts_mac_symbol_styles() {
    let general = GeneralSettings {
        alt_display_style: "symbol".to_string(),
        option_display_style: "symbol".to_string(),
        shift_display_style: "symbol".to_string(),
        ..GeneralSettings::default()
    };
    assert_eq!(
        KeybindingSettings::format_display_parts("alt+shift+n", &general),
        vec!["⌘", "⇧", "N"]
    );
}

#[test]
fn format_display_default_style_is_plain_text() {
    // 기본값은 기존 사용자 경험을 그대로 유지해야 한다 (마이그레이션 시 표시 불변).
    let general = GeneralSettings::default();
    assert_eq!(general.alt_display_style, "alt");
    assert_eq!(general.option_display_style, "option");
    assert_eq!(general.shift_display_style, "shift");
    assert_eq!(
        KeybindingSettings::format_display("alt+option+shift+n", &general),
        "Alt+Option+Shift+N"
    );
}

/// TOML에 일부 필드만 있고 나머지가 누락된 경우,
/// 누락된 필드가 preset_tasty() 기본값을 따르는지 확인.
#[test]
fn missing_fields_fall_back_to_preset_not_empty() {
    let toml = r#"new_workspace = ["alt+n"]"#;
    let kb: KeybindingSettings = toml::from_str(toml).unwrap();
    let preset = KeybindingSettings::preset_tasty();

    for (field_id, _label) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        let deserialized = kb.get_bindings(field_id).unwrap();
        let expected = preset.get_bindings(field_id).unwrap();
        assert_eq!(
            deserialized, expected,
            "필드 '{field_id}'가 누락 시 preset 기본값이 아닌 빈 값이 됨"
        );
    }
}

/// 사용자가 설정한 바인딩과 충돌하는 기본값 바인딩이 제거되는지 확인.
#[test]
fn remove_conflicts_from_defaults_strips_conflicting_combos() {
    // zoom_out의 기본값은 ["ctrl+-", "alt+-"].
    // 사용자가 new_tab = ["ctrl+-"]를 설정하면,
    // zoom_out에서 "ctrl+-"가 제거되어야 한다.
    // (예전에는 `image_undo` 로 같은 것을 쟀다 — 그 필드는 실행부가 없어 걷어냈다.)
    let toml = r#"new_tab = ["ctrl+-"]"#;
    let mut kb: KeybindingSettings = toml::from_str(toml).unwrap();
    let existing_keys: HashSet<String> = ["new_tab".to_string()].into_iter().collect();
    kb.remove_conflicts_from_defaults(&existing_keys);

    // new_tab은 사용자 설정이므로 그대로
    assert_eq!(kb.new_tab, vec!["ctrl+-".to_string()]);
    // zoom_out에서 "ctrl+-"가 제거되고 "alt+-"만 남아야 함
    assert!(
        !kb.zoom_out.contains(&"ctrl+-".to_string()),
        "기본값 필드에서 사용자 바인딩과 충돌하는 combo가 제거되지 않음"
    );
    assert!(
        kb.zoom_out.contains(&"alt+-".to_string()),
        "충돌하지 않는 combo까지 제거됨"
    );
}

#[test]
fn script_binding_set_get_remove() {
    let mut kb = KeybindingSettings::default();
    assert!(kb.script_binding_combo("script-0").is_none());
    kb.set_script_binding("script-0", "ctrl+alt+r".to_string());
    assert_eq!(kb.script_binding_combo("script-0"), Some("ctrl+alt+r"));
    // 같은 script_id 재설정은 교체(중복 누적 안 함).
    kb.set_script_binding("script-0", "ctrl+alt+t".to_string());
    assert_eq!(kb.script_binding_combo("script-0"), Some("ctrl+alt+t"));
    assert_eq!(kb.script_bindings.len(), 1);
    // 빈 combo 는 제거.
    kb.set_script_binding("script-0", String::new());
    assert!(kb.script_binding_combo("script-0").is_none());
    // remove.
    kb.set_script_binding("script-1", "ctrl+alt+y".to_string());
    assert!(kb.remove_script_binding("script-1"));
    assert!(!kb.remove_script_binding("script-1"));
}

#[test]
fn combo_conflict_detects_fixed_and_script() {
    let mut kb = KeybindingSettings::default();
    // 고정 액션(new_tab)의 combo 를 잡아 충돌 확인.
    let taken = kb
        .new_tab
        .first()
        .cloned()
        .unwrap_or_else(|| "ctrl+t".to_string());
    assert!(kb.combo_conflict(&taken, None).is_some());
    // 스크립트끼리 충돌.
    kb.set_script_binding("script-0", "ctrl+alt+9".to_string());
    assert_eq!(
        kb.combo_conflict("ctrl+alt+9", None).as_deref(),
        Some("script:script-0")
    );
    // 자기 자신 재바인딩은 충돌 아님(except).
    assert!(kb.combo_conflict("ctrl+alt+9", Some("script-0")).is_none());
    // 미사용 combo 는 충돌 없음.
    assert!(kb.combo_conflict("ctrl+alt+0", None).is_none());
}

#[test]
fn script_bindings_serde_default_when_absent() {
    // script_bindings 키 없는 기존 config 조각 → 빈 목록.
    let kb: KeybindingSettings = toml::from_str("new_tab = [\"ctrl+t\"]").unwrap();
    assert!(kb.script_bindings.is_empty());
}

// ── quick-switch raw 키 필드 (quickswitch-02) ─────────────────────

#[test]
fn preset_tasty_has_vim_style_quick_switch_defaults() {
    let kb = KeybindingSettings::preset_tasty();
    assert_eq!(
        kb.tab_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
    );
    assert_eq!(
        kb.workspace_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );
    assert_eq!(kb.tab_switch_next_key, "l");
    assert_eq!(kb.tab_switch_prev_key, "h");
    assert_eq!(kb.workspace_switch_next_key, "j");
    assert_eq!(kb.workspace_switch_prev_key, "k");
    // 무변경 확인: next_tab/prev_tab 는 quick-switch 와 별개로 빈 채 유지.
    assert!(kb.next_tab.is_empty());
    assert!(kb.prev_tab.is_empty());
}

#[test]
fn missing_new_fields_in_toml_falls_back_to_defaults() {
    // 신규 필드가 없는 구버전 TOML 문자열을 역직렬화 → 필드별 default fn 으로 복원.
    let toml_str = "tab_switch_modifier = \"ctrl\"\nworkspace_switch_modifier = \"alt\"\n";
    let kb: KeybindingSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(
        kb.tab_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
    );
    assert_eq!(
        kb.workspace_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );
    assert_eq!(kb.tab_switch_next_key, "l");
    assert_eq!(kb.tab_switch_prev_key, "h");
    assert_eq!(kb.workspace_switch_next_key, "j");
    assert_eq!(kb.workspace_switch_prev_key, "k");
}

/// 모든 프리셋이 quick-switch 기본값을 동일하게 갖는지(공통 vim 키 적용) 확인.
#[test]
fn all_presets_share_quick_switch_defaults() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        assert_eq!(
            kb.tab_switch_slot_keys,
            ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
            "preset '{name}' tab slot keys"
        );
        assert_eq!(
            kb.workspace_switch_slot_keys,
            ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
            "preset '{name}' workspace slot keys"
        );
        assert_eq!(kb.tab_switch_next_key, "l", "preset '{name}' tab next");
        assert_eq!(kb.tab_switch_prev_key, "h", "preset '{name}' tab prev");
        assert_eq!(
            kb.workspace_switch_next_key, "j",
            "preset '{name}' workspace next"
        );
        assert_eq!(
            kb.workspace_switch_prev_key, "k",
            "preset '{name}' workspace prev"
        );
        assert_eq!(
            kb.category_switch_slot_keys,
            ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
            "preset '{name}' category slot keys"
        );
    }
}

/// 4 프리셋 전부 축별 modifier 기본값이 탭=ctrl / 워크스페이스=alt / 카테고리=ctrl+shift.
#[test]
fn all_presets_share_switch_modifier_defaults() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        assert_eq!(
            kb.tab_switch_modifier, "ctrl",
            "preset '{name}' tab modifier"
        );
        assert_eq!(
            kb.workspace_switch_modifier, "alt",
            "preset '{name}' workspace modifier"
        );
        assert_eq!(
            kb.category_switch_modifier, "ctrl+shift",
            "preset '{name}' category modifier"
        );
    }
}

/// 구 config(카테고리 modifier 필드 없음) 로드 시 serde default 로 `"ctrl+shift"` 채워짐.
/// 마이그레이션이 아니라 로드가 깨지지 않게만 하는 단순 default.
#[test]
fn missing_category_modifier_falls_back_to_ctrl_shift() {
    let toml_str = "tab_switch_modifier = \"ctrl\"\nworkspace_switch_modifier = \"alt\"\n";
    let kb: KeybindingSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(kb.category_switch_modifier, "ctrl+shift");
}

/// index 기반 slot/raw-key accessor round-trip.
#[test]
fn quick_switch_accessors_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();

    // 슬롯 getter 기본값.
    assert_eq!(kb.tab_slot_key(0), Some("1"));
    assert_eq!(kb.tab_slot_key(9), Some("0"));
    assert_eq!(kb.tab_slot_key(10), None); // 범위 밖
    assert_eq!(kb.workspace_slot_key(8), Some("9"));
    assert_eq!(kb.workspace_slot_key(9), None); // 0번 슬롯 없음

    // 슬롯 setter.
    assert!(kb.set_tab_slot_key(4, "q"));
    assert_eq!(kb.tab_slot_key(4), Some("q"));
    assert!(!kb.set_tab_slot_key(10, "q")); // 범위 밖 → false
    assert!(kb.set_workspace_slot_key(0, "z"));
    assert_eq!(kb.workspace_slot_key(0), Some("z"));
    assert!(!kb.set_workspace_slot_key(9, "z"));

    // next/prev getter·setter.
    assert_eq!(kb.tab_next_key(), "l");
    assert_eq!(kb.tab_prev_key(), "h");
    assert_eq!(kb.workspace_next_key(), "j");
    assert_eq!(kb.workspace_prev_key(), "k");
    kb.set_tab_next_key("n");
    kb.set_tab_prev_key("p");
    kb.set_workspace_next_key("d");
    kb.set_workspace_prev_key("u");
    assert_eq!(kb.tab_next_key(), "n");
    assert_eq!(kb.tab_prev_key(), "p");
    assert_eq!(kb.workspace_next_key(), "d");
    assert_eq!(kb.workspace_prev_key(), "u");
}

/// 직렬화 → 역직렬화 후 quick-switch 필드가 보존되는지(round-trip).
#[test]
fn quick_switch_fields_serde_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();
    kb.set_tab_slot_key(0, "q");
    kb.set_workspace_slot_key(0, "z");
    kb.set_tab_next_key("n");
    kb.set_workspace_prev_key("u");

    let serialized = toml::to_string(&kb).unwrap();
    let restored: KeybindingSettings = toml::from_str(&serialized).unwrap();

    assert_eq!(restored.tab_switch_slot_keys, kb.tab_switch_slot_keys);
    assert_eq!(
        restored.workspace_switch_slot_keys,
        kb.workspace_switch_slot_keys
    );
    assert_eq!(restored.tab_switch_next_key, "n");
    assert_eq!(restored.tab_switch_prev_key, kb.tab_switch_prev_key);
    assert_eq!(
        restored.workspace_switch_next_key,
        kb.workspace_switch_next_key
    );
    assert_eq!(restored.workspace_switch_prev_key, "u");
}

/// 신규 필드를 GENERAL_BINDING_FIELDS 에 넣지 않았음을 회귀 방지로 고정.
#[test]
fn quick_switch_fields_not_in_general_bindings() {
    for id in [
        "tab_switch_slot_keys",
        "workspace_switch_slot_keys",
        "tab_switch_next_key",
        "tab_switch_prev_key",
        "workspace_switch_next_key",
        "workspace_switch_prev_key",
        "category_switch_next_key",
        "category_switch_prev_key",
    ] {
        assert!(
            KeybindingSettings::GENERAL_BINDING_FIELDS
                .iter()
                .all(|(fid, _)| *fid != id),
            "{id} 는 콤보가 아닌 raw 키이므로 GENERAL_BINDING_FIELDS 에 없어야 함"
        );
    }
    // count 는 여전히 전체 개수와 같다.
    assert_eq!(KeybindingSettings::GENERAL_BINDING_FIELDS.len(), 61);
}

// ── 카테고리 축 next/prev raw 키 (S-9) ─────────────────────────────

/// 4 프리셋 전부 카테고리 next/prev 기본값이 vim 스타일 `j`/`k` (분석검증 Q1 확정값).
#[test]
fn all_presets_share_category_next_prev_defaults() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        assert_eq!(kb.category_switch_next_key, "j", "preset '{name}' cat next");
        assert_eq!(kb.category_switch_prev_key, "k", "preset '{name}' cat prev");
    }
}

/// 구 config(카테고리 next/prev 필드 없음) 로드 시 default fn 으로 복원.
#[test]
fn missing_category_next_prev_falls_back_to_defaults() {
    let toml_str = "tab_switch_modifier = \"ctrl\"\nworkspace_switch_modifier = \"alt\"\n";
    let kb: KeybindingSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(kb.category_switch_next_key, "j");
    assert_eq!(kb.category_switch_prev_key, "k");
}

/// 카테고리 next/prev accessor round-trip.
#[test]
fn category_next_prev_accessors_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert_eq!(kb.category_next_key(), "j");
    assert_eq!(kb.category_prev_key(), "k");
    kb.set_category_next_key("u");
    kb.set_category_prev_key("p");
    assert_eq!(kb.category_next_key(), "u");
    assert_eq!(kb.category_prev_key(), "p");
}

/// 카테고리 next/prev 직렬화 round-trip.
#[test]
fn category_next_prev_serde_roundtrip() {
    let mut kb = KeybindingSettings::preset_tasty();
    kb.set_category_next_key("u");
    kb.set_category_prev_key("p");
    let serialized = toml::to_string(&kb).unwrap();
    let restored: KeybindingSettings = toml::from_str(&serialized).unwrap();
    assert_eq!(restored.category_switch_next_key, "u");
    assert_eq!(restored.category_switch_prev_key, "p");
}

// ── "개별 지정" sentinel + 역전환 복원 (S-9) ────────────────────────

/// sentinel 의 **문자열 값**을 고정한다(회귀 방지) — 이 값은 축 modifier 필드에 그대로
/// 직렬화되므로, 바뀌면 기존 config 의 "개별 지정" 축이 조용히 풀린다. "4축 조합 파서가
/// 절대 만들 수 없는 문자열" 이라는 동치 쪽은 `parse` 의
/// `individual_sentinel_is_not_a_modifier_combo` 가 `Combo::parse_modifiers` 를 직접 불러
/// 고정한다 — 같은 크레이트라 여기서 문자열 형태만 고정할 이유가 없다.
#[test]
fn individual_switch_modifier_sentinel_value() {
    assert_eq!(KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER, "individual");
}

/// 4 프리셋 전부 기본값은 sentinel 이 아니다(규칙 기반 값만 가짐 — 작업 항목 9).
#[test]
fn presets_never_default_to_individual_modifier() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        for modifier in [
            &kb.tab_switch_modifier,
            &kb.workspace_switch_modifier,
            &kb.category_switch_modifier,
        ] {
            assert_ne!(
                modifier.as_str(),
                KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER,
                "preset '{name}' 축 modifier 가 개별 지정 sentinel 이면 안 됨"
            );
        }
    }
}

/// 개별 지정 → 규칙 기반 역전환 시 각 축의 reset_*_to_defaults 가 정확히 기본값으로
/// 복원하는지(분석검증 Q3).
#[test]
fn reset_to_defaults_restores_each_axis_independently() {
    let mut kb = KeybindingSettings::preset_tasty();
    kb.set_tab_slot_key(0, "ctrl+alt+q");
    kb.set_tab_next_key("shift+f5");
    kb.set_workspace_slot_key(0, "ctrl+shift+z");
    kb.set_category_next_key("alt+ctrl+p");

    kb.reset_tab_switch_to_defaults();
    assert_eq!(
        kb.tab_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
    );
    assert_eq!(kb.tab_next_key(), "l");
    assert_eq!(kb.tab_prev_key(), "h");
    // 워크스페이스/카테고리는 아직 손대지 않았으니 그대로.
    assert_eq!(kb.workspace_slot_key(0), Some("ctrl+shift+z"));
    assert_eq!(kb.category_next_key(), "alt+ctrl+p");

    kb.reset_workspace_switch_to_defaults();
    assert_eq!(
        kb.workspace_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );
    assert_eq!(kb.category_next_key(), "alt+ctrl+p"); // 카테고리는 여전히 그대로.

    kb.reset_category_switch_to_defaults();
    assert_eq!(kb.category_next_key(), "j");
    assert_eq!(kb.category_prev_key(), "k");
    assert_eq!(
        kb.category_switch_slot_keys,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
    );
}

/// 카테고리 next/prev 기본값(`j`/`k`)이 4 프리셋 전체에서 다른 일반 액션과 충돌하지
/// 않는지 확인 — 분석검증 Q1 근거("j"/"k" 무충돌, "u"/"p" 는 실제로는 충돌 없음이지만
/// 원 문서 제안값 "p" 는 toggle_command_palette 와 충돌해 기각됐음을 회귀로 고정).
#[test]
fn category_next_prev_defaults_do_not_conflict_with_presets() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        let next_combo = format!(
            "{}+{}",
            kb.category_switch_modifier, kb.category_switch_next_key
        );
        let prev_combo = format!(
            "{}+{}",
            kb.category_switch_modifier, kb.category_switch_prev_key
        );
        assert_eq!(
            kb.find_conflict("", &next_combo),
            None,
            "preset '{name}': category next combo '{next_combo}' 충돌"
        );
        assert_eq!(
            kb.find_conflict("", &prev_combo),
            None,
            "preset '{name}': category prev combo '{prev_combo}' 충돌"
        );
    }
}

// ── 전체화면 무대 종료 (fullscreen_stage_exit) ─────────────────────

/// 4 프리셋 전부 기본값이 `escape` 다. 무대는 플랫폼 관습이 갈리는 영역이 아니라
/// 프리셋별로 다르게 둘 이유가 없다 — 한 프리셋만 빠져도 그 프리셋 사용자에게는
/// 키보드 종료 수단이 사라진다.
#[test]
fn every_preset_binds_fullscreen_exit_to_escape() {
    for name in KeybindingSettings::preset_names() {
        let kb = KeybindingSettings::preset_by_name(name).unwrap();
        assert_eq!(
            kb.fullscreen_stage_exit,
            vec!["escape".to_string()],
            "preset '{name}' 의 fullscreen_stage_exit 기본값이 escape 가 아니다"
        );
    }
}

/// id → slice / id → `&mut` 양방향 매핑이 둘 다 연결돼 있는지.
///
/// slice 쪽이 빠지면 `find_conflict` 가 이 바인딩을 못 봐 **중복 등록을 허용**하고,
/// `&mut` 쪽이 빠지면 설정 UI 녹화가 값을 못 쓴다. 둘은 서로 다른 match 문이라
/// 한쪽만 추가되는 실수가 실제로 가능하다.
#[test]
fn fullscreen_stage_exit_has_both_crud_directions() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert_eq!(
        kb.get_bindings("fullscreen_stage_exit"),
        Some(&vec!["escape".to_string()][..]),
        "id → slice 매핑이 없다 — 충돌 검사가 이 바인딩을 못 본다"
    );
    assert!(
        kb.set_field("fullscreen_stage_exit", "ctrl+alt+q"),
        "id → &mut 매핑이 없다 — 설정 UI 녹화가 값을 쓸 수 없다"
    );
    assert_eq!(kb.get_field("fullscreen_stage_exit"), Some("ctrl+alt+q"));
    assert_eq!(
        KeybindingSettings::label_key_for("fullscreen_stage_exit"),
        Some("settings.keybindings.fullscreen_stage_exit_label"),
        "GENERAL_BINDING_FIELDS 에 없다 — 설정 UI·충돌 검사 순회에서 통째로 빠진다"
    );
}

/// 충돌 검사가 이 필드를 **양방향으로** 본다 — 이 필드에 남의 combo 를 넣어도,
/// 남의 필드에 이 필드의 combo 를 넣어도 충돌로 잡혀야 한다.
#[test]
fn fullscreen_stage_exit_participates_in_conflict_detection() {
    let mut kb = KeybindingSettings::preset_tasty();
    let new_tab = kb.new_tab.first().cloned().expect("new_tab 기본값");
    assert_eq!(
        kb.find_conflict("fullscreen_stage_exit", &new_tab),
        Some(("new_tab", 0)),
        "이 필드에 다른 액션의 combo 를 넣었는데 충돌로 잡히지 않았다"
    );
    kb.fullscreen_stage_exit = vec!["ctrl+alt+q".to_string()];
    assert_eq!(
        kb.find_conflict("new_tab", "ctrl+alt+q"),
        Some(("fullscreen_stage_exit", 0)),
        "다른 액션이 이 필드의 combo 를 가져갔는데 충돌로 잡히지 않았다"
    );
}

/// 구 config(이 필드 없음) 로드 시 전용 default fn 이 기본 바인딩을 복원한다.
/// struct 레벨 `#[serde(default)]` 만 믿으면 `Vec::default()`(빈 vec)가 들어와
/// 키보드 종료 수단이 조용히 사라진다.
#[test]
fn missing_fullscreen_stage_exit_falls_back_to_escape() {
    let toml_str = r#"new_tab = ["ctrl+t"]"#;
    let kb: KeybindingSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(kb.fullscreen_stage_exit, vec!["escape".to_string()]);
}

/// 사용자가 의도적으로 비운 빈 vec 은 **그대로 보존**된다(기본값으로 되살아나지
/// 않는다). 탈출 수단은 무대 셸의 종료 버튼이 담당한다.
#[test]
fn empty_fullscreen_stage_exit_is_preserved() {
    let toml_str = r#"
new_tab = ["ctrl+t"]
fullscreen_stage_exit = []
"#;
    let kb: KeybindingSettings = toml::from_str(toml_str).unwrap();
    assert!(kb.fullscreen_stage_exit.is_empty());
}

// ── 콤보 필드 ↔ SoT 정합 ──────────────────────────────────────────

/// 직렬화 결과에서 **문자열 배열인 최상위 키**를 뽑아 콤보 필드 목록을 만든다.
///
/// 좌변을 손으로 나열하지 않는 것이 요점이다 — 새 콤보 필드가 늘면 이 목록도 같이
/// 는다. 제외는 둘뿐이고 둘 다 이유가 다르다:
///
/// - `*_switch_slot_keys` 셋은 문자열 배열이지만 콤보가 아니라 raw 키다(이유는 아래
///   `quick_switch_fields_not_in_general_bindings` 와 같다).
/// - `script_bindings` 는 `Vec<ScriptBinding>` 이라 형태부터 다르다(ADR-0031: 스크립트는
///   N 개 동적이라 고정 액션 필드와 별개 표현이다). 기본값이 비어 있으면 "문자열 배열"
///   판정을 공허하게 통과하므로 형태로는 안 갈리고 이름으로 뺀다.
///
/// 기본값이 빈 콤보 필드(`open_explorer` 등)를 놓치지 않으려고 "비어 있지 않은 배열"
/// 로는 거르지 않는다 — 그렇게 하면 이 시험이 가장 잘 빠지는 자리를 못 본다.
fn combo_field_ids(kb: &KeybindingSettings) -> Vec<String> {
    const NOT_COMBO_FIELDS: &[&str] = &[
        "tab_switch_slot_keys",
        "workspace_switch_slot_keys",
        "category_switch_slot_keys",
        "script_bindings",
    ];
    let value = toml::Value::try_from(kb).expect("KeybindingSettings 직렬화");
    let table = value.as_table().expect("최상위는 테이블");
    table
        .iter()
        .filter(|(k, _)| !NOT_COMBO_FIELDS.contains(&k.as_str()))
        .filter(|(_, v)| {
            v.as_array()
                .is_some_and(|a| a.iter().all(|item| item.is_str()))
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// 콤보 필드는 전부 SoT 에 있어야 한다.
///
/// SoT 밖의 콤보 필드는 "동작은 하는데 아무 소비자도 못 보는" 상태가 된다 —
/// webview 키 포워딩(`HostShortcutPolicy`)이 그 콤보를 host 것으로 안 세어 페이지가
/// 먹고, 충돌 검사가 그 필드를 후보로 안 봐서 같은 키가 두 액션에 걸린 채 저장된다.
#[test]
fn every_combo_field_is_in_the_sot() {
    let kb = KeybindingSettings::preset_tasty();
    let missing: Vec<String> = combo_field_ids(&kb)
        .into_iter()
        .filter(|id| {
            KeybindingSettings::GENERAL_BINDING_FIELDS
                .iter()
                .all(|(fid, _)| fid != id)
        })
        .collect();
    assert!(
        missing.is_empty(),
        "콤보 필드인데 GENERAL_BINDING_FIELDS 에 없다: {missing:?}"
    );
}

/// 사용자가 TOML 에 직접 적어 둔 콤보가 이 3필드의 기본값을 이긴다.
///
/// 등록 전에는 이 3필드가 SoT 밖이라 `remove_conflicts_from_defaults` 의 양쪽 순회에서
/// 다 빠져 아무 일도 안 일어났다. 등록하면서 **다른 55개와 같은 취급**을 받게 했다 —
/// 사용자가 명시한 값이 기본값을 이기는 것이 이 함수의 규칙이고, 이 셋만 예외로 두면
/// 같은 콤보가 두 액션에 걸린 채 남는다. 그 대가로 `ctrl+b` 를 이미 다른 데 쓰던
/// 사용자는 업그레이드 첫 로드에서 사이드바 접기 기본값을 잃는다.
#[test]
fn a_user_combo_beats_the_new_sidebar_defaults() {
    let mut kb = KeybindingSettings::preset_tasty();
    assert_eq!(kb.toggle_sidebar_collapse, vec!["ctrl+b".to_string()]);
    kb.new_tab = vec!["ctrl+b".to_string()];

    let existing: HashSet<String> = ["new_tab".to_string()].into_iter().collect();
    kb.remove_conflicts_from_defaults(&existing);

    assert!(
        kb.toggle_sidebar_collapse.is_empty(),
        "사용자가 적은 ctrl+b 가 이겨야 한다: {:?}",
        kb.toggle_sidebar_collapse
    );
    assert_eq!(kb.new_tab, vec!["ctrl+b".to_string()]);
}

/// 걷어낸 필드가 적힌 옛 설정 파일이 그대로 읽힌다.
///
/// `image_undo` / `image_redo` 는 실행부가 없어 제거했다. 이미 그 키가 저장된 설정
/// 파일이 있을 수 있는데, serde 는 모르는 키를 그냥 무시하므로(struct 에
/// `deny_unknown_fields` 를 안 걸었다) 로드가 깨지지 않아야 한다 — 여기서 그것을
/// 값으로 고정한다. 깨지면 사용자가 설정 전체를 잃는다.
#[test]
fn an_old_file_naming_a_removed_field_still_loads() {
    let toml = r#"
new_tab = ["alt+t"]
image_undo = ["ctrl+z"]
image_redo = ["ctrl+shift+z"]
"#;
    let kb: KeybindingSettings = toml::from_str(toml).expect("모르는 키는 무시돼야 한다");
    assert_eq!(kb.new_tab, vec!["alt+t".to_string()]);
}

// ── quick-switch 축 타입 ──────────────────────────────────────────────

use crate::keybindings::crud::{SwitchAxis, SwitchStep};

/// 축이 말하는 슬롯 수는 **실제 배열 길이**여야 한다 — 둘이 갈리면 순회가 슬롯을
/// 빠뜨리거나 범위 밖을 짚는다.
#[test]
fn switch_axis_slot_count_matches_the_array() {
    let kb = KeybindingSettings::default();
    assert_eq!(SwitchAxis::Tab.slot_count(), kb.tab_switch_slot_keys.len());
    assert_eq!(
        SwitchAxis::Workspace.slot_count(),
        kb.workspace_switch_slot_keys.len()
    );
    assert_eq!(
        SwitchAxis::Category.slot_count(),
        kb.category_switch_slot_keys.len()
    );
    // 마지막 슬롯은 있고 그다음은 없다 — 경계가 실제로 그 수다.
    for axis in SwitchAxis::ALL {
        assert!(axis.slot(&kb, axis.slot_count() - 1).is_some());
        assert!(axis.slot(&kb, axis.slot_count()).is_none());
    }
}

#[test]
fn switch_axis_accessors_reach_every_field() {
    let mut kb = KeybindingSettings::default();
    for axis in SwitchAxis::ALL {
        axis.set_modifier(&mut kb, "ctrl+shift");
        assert_eq!(axis.modifier(&kb), "ctrl+shift");
        assert!(!axis.is_individual(&kb));
        axis.set_modifier(&mut kb, KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER);
        assert!(axis.is_individual(&kb));

        assert!(axis.set_slot(&mut kb, 0, "z"));
        assert_eq!(axis.slot(&kb, 0), Some("z"));
        assert!(!axis.set_slot(&mut kb, axis.slot_count(), "z"));

        for step in SwitchStep::ALL {
            axis.set_step(&mut kb, step, "w");
            assert_eq!(axis.step(&kb, step), "w");
        }
    }
    // 축·step 조합이 서로 다른 필드를 짚는지 — 하나로 뭉개지면 위 루프가 못 잡는다.
    let mut kb = KeybindingSettings::default();
    for (i, axis) in SwitchAxis::ALL.into_iter().enumerate() {
        for (j, step) in SwitchStep::ALL.into_iter().enumerate() {
            axis.set_step(&mut kb, step, &format!("k{i}{j}"));
        }
    }
    for (i, axis) in SwitchAxis::ALL.into_iter().enumerate() {
        for (j, step) in SwitchStep::ALL.into_iter().enumerate() {
            assert_eq!(axis.step(&kb, step), format!("k{i}{j}"));
        }
    }
}

/// modifier 필드 id 는 `get_bindings` 가 아는 콤보 필드가 **아니다** — 축 modifier 는
/// 콤보 `Vec` 시스템 밖이라는 사실을 여기서 고정한다.
#[test]
fn switch_axis_modifier_fields_are_outside_the_combo_system() {
    let kb = KeybindingSettings::default();
    for axis in SwitchAxis::ALL {
        let id = axis.modifier_field_id();
        assert!(kb.get_bindings(id).is_none(), "{id}");
        assert!(
            !KeybindingSettings::GENERAL_BINDING_FIELDS
                .iter()
                .any(|(f, _)| *f == id),
            "{id}"
        );
    }
}
