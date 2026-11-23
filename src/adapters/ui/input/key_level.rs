//! 단축키 후보의 소속 level(계약 K5). 키 경로에 있는 노드의 level 후보만 판정하고, 덮인 노드의 level은 거른다.
//! 이 표는 제품 판정(`route_key`, [`shortcut_allowed`])이 읽는 매핑 그 자체다. 시험은 이 표를 직접 읽는다.
//! 근거는 처리기가 읽는 입력 시점 대상 단위(`command_target_gone`, 구조 명령의 `Unit`)이고, 대상 단위가 없는
//! 명령은 기본 규칙(창 분할·사이드바·도구 메뉴·팝업 열기는 Window, 새 창·종료·설정 열기는 engine)을 따른다.
//! 규칙과 표 설명: `docs/architecture/input-layer.md`의 "단축키 level".

use tasty_plugin_manifest::CommandScope;

/// 단축키 후보가 속한 노드 level. 키 경로는 출발 노드에서 engine까지 이 순서의 역순으로 오른다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum KeyLevel {
    Engine,
    Window,
    Workspace,
    Pane,
    Tab,
    Surface,
}

impl KeyLevel {
    pub(crate) const ALL: [KeyLevel; 6] = [
        KeyLevel::Engine,
        KeyLevel::Window,
        KeyLevel::Workspace,
        KeyLevel::Pane,
        KeyLevel::Tab,
        KeyLevel::Surface,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// 숫자 전환의 축. 축마다 수식키 모드와 개별 키 모드가 있고 두 모드 모두 축의 level을 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericAxis {
    Tab,
    Workspace,
    Category,
}

/// 숫자 전환 축의 level. Tab 축은 Tab, Workspace·Category 축은 workspace를 고르므로 Workspace다.
pub(crate) fn numeric_axis_level(axis: NumericAxis) -> KeyLevel {
    match axis {
        NumericAxis::Tab => KeyLevel::Tab,
        NumericAxis::Workspace | NumericAxis::Category => KeyLevel::Workspace,
    }
}

/// 설정 필드 하나의 소속.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldLevel {
    /// 일반 매칭(더블탭 필드도 여기서 같은 level을 쓴다).
    Level(KeyLevel),
    /// 숫자 전환 필드(수식키·slot 키·다음·이전 키). level은 축의 level이다.
    Numeric(NumericAxis),
    /// 단축키가 아니다. `explorer_drag_flip_modifier`는 끌기 방향을 뒤집는 수식키 값이다.
    NotAShortcut,
}

impl FieldLevel {
    /// 이 필드 후보의 level. 단축키가 아니면 `None`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the level filter and the key path read it from the level filter commit on"
        )
    )]
    pub(crate) fn level(self) -> Option<KeyLevel> {
        match self {
            FieldLevel::Level(level) => Some(level),
            FieldLevel::Numeric(axis) => Some(numeric_axis_level(axis)),
            FieldLevel::NotAShortcut => None,
        }
    }
}

use FieldLevel::{Level as L, Numeric as N};
use KeyLevel::{Engine, Pane, Surface, Tab, Window, Workspace};

/// `KeybindingSettings`의 직렬화 최상위 키마다 소속 level. 키 하나가 정확히 한 번 나온다(시험).
pub(crate) const KEYBINDING_LEVELS: &[(&str, FieldLevel)] = &[
    ("new_workspace", L(Workspace)),
    ("new_tab", L(Pane)),
    ("split_pane_vertical", L(Pane)),
    ("split_pane_horizontal", L(Pane)),
    ("split_surface_vertical", L(Surface)),
    ("split_surface_horizontal", L(Surface)),
    ("toggle_settings", L(Engine)),
    ("toggle_notifications", L(Window)),
    ("toggle_dag_list", L(Workspace)),
    ("open_port_scanner", L(Window)),
    ("open_remote_tool", L(Window)),
    ("open_preset_window", L(Engine)),
    ("open_tutorial", L(Window)),
    ("open_file_picker", L(Workspace)),
    ("close_pane", L(Pane)),
    ("close_surface", L(Surface)),
    ("close_workspace", L(Workspace)),
    ("focus_pane_next", L(Pane)),
    ("focus_pane_prev", L(Pane)),
    ("focus_surface_next", L(Tab)),
    ("focus_surface_prev", L(Tab)),
    ("tab_switch_modifier", N(NumericAxis::Tab)),
    ("workspace_switch_modifier", N(NumericAxis::Workspace)),
    ("category_switch_modifier", N(NumericAxis::Category)),
    ("explorer_drag_flip_modifier", FieldLevel::NotAShortcut),
    ("fullscreen_stage_exit", L(Window)),
    ("code_area_apply", L(Surface)),
    ("code_area_cancel", L(Surface)),
    ("explorer_cursor_up", L(Surface)),
    ("explorer_cursor_down", L(Surface)),
    ("explorer_cursor_left", L(Surface)),
    ("explorer_cursor_right", L(Surface)),
    ("explorer_cursor_home", L(Surface)),
    ("explorer_cursor_end", L(Surface)),
    ("explorer_cursor_page_up", L(Surface)),
    ("explorer_cursor_page_down", L(Surface)),
    ("explorer_extend_up", L(Surface)),
    ("explorer_extend_down", L(Surface)),
    ("explorer_extend_left", L(Surface)),
    ("explorer_extend_right", L(Surface)),
    ("explorer_extend_home", L(Surface)),
    ("explorer_extend_end", L(Surface)),
    ("explorer_extend_page_up", L(Surface)),
    ("explorer_extend_page_down", L(Surface)),
    ("explorer_open", L(Surface)),
    ("explorer_clear_selection", L(Surface)),
    ("toggle_sidebar", L(Window)),
    ("toggle_sidebar_collapse", L(Window)),
    ("toggle_categories_collapsed", L(Window)),
    ("restore_closed", L(Pane)),
    ("quit", L(Engine)),
    ("quit_immediate", L(Engine)),
    ("quit_minimize", L(Window)),
    ("open_markdown", L(Surface)),
    ("open_explorer", L(Pane)),
    ("convert_surface", L(Surface)),
    ("convert_to_markdown", L(Surface)),
    ("convert_to_explorer", L(Surface)),
    ("new_window", L(Engine)),
    ("close_active", L(Tab)),
    ("next_tab", L(Tab)),
    ("prev_tab", L(Tab)),
    ("screenshot_to_clipboard", L(Surface)),
    ("find", L(Surface)),
    ("copy", L(Surface)),
    ("copy_path", L(Surface)),
    ("copy_link", L(Surface)),
    ("cut", L(Surface)),
    ("select_all", L(Surface)),
    ("refresh", L(Surface)),
    ("explorer_refresh", L(Surface)),
    ("explorer_go_up", L(Surface)),
    ("explorer_back", L(Surface)),
    ("explorer_forward", L(Surface)),
    ("explorer_focus_address", L(Surface)),
    ("explorer_toggle_preview", L(Surface)),
    ("explorer_toggle_hidden", L(Surface)),
    ("explorer_properties", L(Surface)),
    ("explorer_new_folder", L(Surface)),
    ("explorer_new_file", L(Surface)),
    ("explorer_rename", L(Surface)),
    ("explorer_trash", L(Surface)),
    ("explorer_undo", L(Surface)),
    ("explorer_redo", L(Surface)),
    ("paste", L(Surface)),
    ("page_undo", L(Surface)),
    ("page_redo", L(Surface)),
    ("zoom_in", L(Surface)),
    ("zoom_out", L(Surface)),
    ("zoom_reset", L(Surface)),
    ("rename_tab", L(Tab)),
    ("rename_workspace", L(Workspace)),
    ("rename_workspace_subtitle", L(Workspace)),
    ("toggle_command_palette", L(Window)),
    ("apply_workspace_preset", L(Workspace)),
    ("apply_tab_preset", L(Tab)),
    ("apply_pane_preset", L(Pane)),
    ("enter_copy_mode", L(Surface)),
    ("minimize_window", L(Window)),
    ("maximize_window", L(Window)),
    ("close_window", L(Window)),
    // 스크립트 매처는 대상을 받지 않고 실행 창 상태에 요청을 쌓는다.
    ("script_bindings", L(Window)),
    ("tab_switch_slot_keys", N(NumericAxis::Tab)),
    ("workspace_switch_slot_keys", N(NumericAxis::Workspace)),
    ("category_switch_slot_keys", N(NumericAxis::Category)),
    ("tab_switch_next_key", N(NumericAxis::Tab)),
    ("tab_switch_prev_key", N(NumericAxis::Tab)),
    ("workspace_switch_next_key", N(NumericAxis::Workspace)),
    ("workspace_switch_prev_key", N(NumericAxis::Workspace)),
    ("category_switch_next_key", N(NumericAxis::Category)),
    ("category_switch_prev_key", N(NumericAxis::Category)),
];

/// 설정 필드의 소속. 표에 없는 이름이면 `None`(시험이 표와 직렬화 키를 맞춘다).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the level filter and the key path read it from the level filter commit on"
    )
)]
pub(crate) fn keybinding_level(field: &str) -> Option<FieldLevel> {
    KEYBINDING_LEVELS
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, level)| *level)
}

/// plugin 명령의 level. Surface 명령은 그 plugin surface의 Surface level, Global 명령은 engine level이다.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the plugin gate reads it from the level filter commit on"
    )
)]
pub(crate) fn command_scope_level(scope: CommandScope) -> KeyLevel {
    match scope {
        CommandScope::Surface => KeyLevel::Surface,
        CommandScope::Global => KeyLevel::Engine,
    }
}

/// 키 경로에 있는 노드들의 level 집합. engine은 늘 경로의 끝이라 들어 있다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PathLevels(u8);

impl PathLevels {
    /// engine만 있는 경로(펼친 팝업·무대).
    pub(crate) fn engine_only() -> Self {
        Self(KeyLevel::Engine.bit())
    }

    /// 덮인 노드가 없는 경로(모든 level).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the key path builder uses it from the level filter commit on"
        )
    )]
    pub(crate) fn all() -> Self {
        Self(KeyLevel::ALL.iter().fold(0, |bits, l| bits | l.bit()))
    }

    pub(crate) fn with(self, level: KeyLevel) -> Self {
        Self(self.0 | level.bit())
    }

    pub(crate) fn contains(self, level: KeyLevel) -> bool {
        self.0 & level.bit() != 0
    }
}

/// 공용 판정: 이 level의 후보를 이 경로에서 판정해도 되는지. 단축키를 실행하는 갈래는 후보 매칭 전에 이
/// 함수로 고른다.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the shortcut branches call it from the level filter commit on"
    )
)]
pub(crate) fn shortcut_allowed(path: PathLevels, level: KeyLevel) -> bool {
    path.contains(level)
}

#[cfg(test)]
#[path = "key_level_tests.rs"]
mod tests;
