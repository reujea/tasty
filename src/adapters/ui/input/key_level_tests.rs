use super::*;
use crate::settings::KeybindingSettings;

/// 직렬화 최상위 키. 필드를 더하면 자동으로 들어온다.
fn serialized_fields() -> Vec<String> {
    let value = toml::Value::try_from(KeybindingSettings::default()).expect("직렬화");
    value
        .as_table()
        .expect("최상위는 테이블")
        .keys()
        .cloned()
        .collect()
}

/// T-11-2: 직렬화 키 전부가 level 표에 정확히 한 번 있고, 표에 직렬화 키가 아닌 이름이 없다.
#[test]
fn every_serialized_field_has_exactly_one_level() {
    let fields = serialized_fields();
    assert_eq!(fields.len(), 111, "직렬화 키 수가 바뀌었다: {fields:?}");
    for field in &fields {
        let n = KEYBINDING_LEVELS
            .iter()
            .filter(|(name, _)| name == field)
            .count();
        assert_eq!(n, 1, "`{field}` 의 level 행이 {n}개다");
    }
    for (name, _) in KEYBINDING_LEVELS {
        assert!(
            fields.iter().any(|f| f == name),
            "level 표의 `{name}` 은 직렬화 키가 아니다"
        );
    }
}

/// 단축키가 아닌 필드는 끌기 방향 수식키 하나다.
#[test]
fn only_the_drag_flip_modifier_is_not_a_shortcut() {
    let none: Vec<_> = KEYBINDING_LEVELS
        .iter()
        .filter(|(_, level)| level.level().is_none())
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(none, ["explorer_drag_flip_modifier"]);
}

/// 더블탭 입구의 필드는 모두 일반 level을 가진다(더블탭은 같은 필드의 다른 입구다).
#[test]
fn every_double_tap_field_has_a_level() {
    for field in KeybindingSettings::DOUBLE_TAP_FIELDS {
        let level = keybinding_level(field).and_then(FieldLevel::level);
        assert!(level.is_some(), "더블탭 필드 `{field}` 에 level이 없다");
    }
}

/// 숫자 전환의 수식키 모드(수식키 필드)와 개별 키 모드(slot·다음·이전 키 필드)가 축마다 같은 level이다.
#[test]
fn both_numeric_modes_use_the_axis_level() {
    for (axis, prefix) in [
        (NumericAxis::Tab, "tab"),
        (NumericAxis::Workspace, "workspace"),
        (NumericAxis::Category, "category"),
    ] {
        let want = numeric_axis_level(axis);
        for suffix in ["modifier", "slot_keys", "next_key", "prev_key"] {
            let field = format!("{prefix}_switch_{suffix}");
            assert_eq!(
                keybinding_level(&field),
                Some(FieldLevel::Numeric(axis)),
                "`{field}`"
            );
            assert_eq!(
                keybinding_level(&field).and_then(FieldLevel::level),
                Some(want)
            );
        }
    }
    assert_eq!(numeric_axis_level(NumericAxis::Tab), KeyLevel::Tab);
    assert_eq!(
        numeric_axis_level(NumericAxis::Workspace),
        KeyLevel::Workspace
    );
    assert_eq!(
        numeric_axis_level(NumericAxis::Category),
        KeyLevel::Workspace
    );
}

/// plugin 명령: Surface 명령은 Surface, Global 명령은 engine.
#[test]
fn plugin_command_scope_levels() {
    assert_eq!(
        command_scope_level(CommandScope::Surface),
        KeyLevel::Surface
    );
    assert_eq!(command_scope_level(CommandScope::Global), KeyLevel::Engine);
}

/// 경로 level 집합: engine은 늘 있고, 더한 level만 허용한다.
#[test]
fn the_filter_allows_only_levels_on_the_path() {
    let path = PathLevels::engine_only().with(KeyLevel::Window);
    for level in KeyLevel::ALL {
        let want = matches!(level, KeyLevel::Engine | KeyLevel::Window);
        assert_eq!(shortcut_allowed(path, level), want, "{level:?}");
    }
    for level in KeyLevel::ALL {
        assert!(shortcut_allowed(PathLevels::all(), level));
    }
}
