use tasty_type_geometry::length::LogicalPx;

use crate::i18n::t;
use crate::plugin::registry_state::ShortcutOverride;
use crate::settings::{GeneralSettings, Settings};
use crate::settings_ui::PluginShortcutSnapshot;

/// 녹화 완료 시 발견된 단축키 충돌의 확인 대기 상태.
#[derive(Debug, Clone)]
pub struct PendingBinding {
    pub target_field: String,
    /// 교체할 (또는 새로 추가할) 대상 인덱스. len()이면 새 추가.
    pub target_idx: usize,
    pub combo: String,
    pub conflicting_field: String,
    pub conflicting_idx: usize,
    // ── quick-switch bare-key 확장 ─────────────────────────────────────
    /// `Some` 이면 이 충돌의 타겟이 quick-switch bare-key 슬롯이다. accept 시
    /// `combo`(합성 표시용) 대신 [`Self::bare_raw_key`] 를 accessor 로 기록한다.
    pub bare_target: Option<BareTarget>,
    /// `bare_target` 이 `Some` 일 때 슬롯에 기록할 raw 키(modifier 없음).
    pub bare_raw_key: String,
    /// 충돌 상대가 **다른 quick-switch 슬롯**이면 `Some`. accept 시 그 슬롯을 비운다
    /// (`conflicting_field`/`conflicting_idx` 는 일반 콤보 필드 전용이므로 슬롯 충돌엔
    /// 쓸 수 없다).
    pub conflicting_bare: Option<BareTarget>,
    /// 팝업에 표시할 충돌 대상 라벨(이미 번역·정리된 문자열). `Some` 이면 팝업이
    /// `label_key_for` 경로 대신 이 값을 그대로 쓴다(슬롯 충돌은 일반 필드 라벨 맵에
    /// 없으므로 필요).
    pub conflicting_label: Option<String>,
}

/// 탭·워크스페이스·카테고리 전환 키 슬롯. 수식키는 각 전환 설정과 조합한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BareTarget {
    /// 탭 quick-switch 슬롯 `idx`(0~9 → 표시 "1번"~"10번").
    TabSlot(usize),
    /// 워크스페이스 quick-switch 슬롯 `idx`(0~8 → 표시 "1번"~"9번").
    WorkspaceSlot(usize),
    /// 카테고리 quick-switch 슬롯 `idx`(0~9 → 표시 "1번"~"10번", reserved normal=1).
    CategorySlot(usize),
    TabNext,
    TabPrev,
    WorkspaceNext,
    WorkspacePrev,
    CategoryNext,
    CategoryPrev,
}

/// 녹화 슬롯이 요구하는 캡처 규칙. 일반 콤보 필드는 modifier 필수([`Combo`]),
/// quick-switch 슬롯은 modifier 금지 bare 키([`BareKey`]) — 단 그 축이 "개별 지정"
/// (`KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER`) 이면 [`IndividualSlot`] 로
/// modifier 포함 자유 콤보를 녹화한다(일반 콤보 필드와 동일 캡처 규칙, 저장 위치만
/// quick-switch 슬롯 필드).
///
/// [`Combo`]: FieldKind::Combo
/// [`BareKey`]: FieldKind::BareKey
/// [`IndividualSlot`]: FieldKind::IndividualSlot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Combo,
    BareKey(BareTarget),
    IndividualSlot(BareTarget),
}

/// Sub-tab within the Keybindings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindingsSubTab {
    General,
    Workspace,
    Pane,
    Tab,
    Surface,
    Clipboard,
    Zoom,
    Explorer,
    Scripts,
    Preset,
    Plugins,
    /// 구성 전량 내보내기 · 파일에서 가져오기(미리보기 후 적용).
    ImportExport,
}

/// 필드의 탭과 표시 순서를 지정한다. 라벨은 `KeybindingSettings::binding_fields`에서 읽는다.
/// 배치를 지정하지 않은 필드는 General 끝에 표시한다.
const ENTRY_PLACEMENT: &[(&str, KeybindingsSubTab, Option<&str>)] = &[
    // General
    ("toggle_settings", KeybindingsSubTab::General, None),
    ("toggle_notifications", KeybindingsSubTab::General, None),
    ("toggle_dag_list", KeybindingsSubTab::General, None),
    ("open_port_scanner", KeybindingsSubTab::General, None),
    ("open_remote_tool", KeybindingsSubTab::General, None),
    ("open_preset_window", KeybindingsSubTab::General, None),
    ("open_tutorial", KeybindingsSubTab::General, None),
    ("open_file_picker", KeybindingsSubTab::General, None),
    ("toggle_command_palette", KeybindingsSubTab::General, None),
    ("toggle_sidebar", KeybindingsSubTab::General, None),
    ("toggle_sidebar_collapse", KeybindingsSubTab::General, None),
    (
        "fullscreen_stage_exit",
        KeybindingsSubTab::General,
        Some("settings.keybindings.fullscreen_stage_exit_desc"),
    ),
    (
        "code_area_apply",
        KeybindingsSubTab::General,
        Some("settings.keybindings.code_area_apply_desc"),
    ),
    (
        "code_area_cancel",
        KeybindingsSubTab::General,
        Some("settings.keybindings.code_area_cancel_desc"),
    ),
    ("restore_closed", KeybindingsSubTab::General, None),
    ("new_window", KeybindingsSubTab::General, None),
    (
        "quit",
        KeybindingsSubTab::General,
        Some("settings.keybindings.quit_desc"),
    ),
    ("quit_immediate", KeybindingsSubTab::General, None),
    ("quit_minimize", KeybindingsSubTab::General, None),
    ("minimize_window", KeybindingsSubTab::General, None),
    ("maximize_window", KeybindingsSubTab::General, None),
    ("close_window", KeybindingsSubTab::General, None),
    // Workspace
    ("new_workspace", KeybindingsSubTab::Workspace, None),
    ("rename_workspace", KeybindingsSubTab::Workspace, None),
    (
        "rename_workspace_subtitle",
        KeybindingsSubTab::Workspace,
        None,
    ),
    (
        "toggle_categories_collapsed",
        KeybindingsSubTab::Workspace,
        None,
    ),
    ("apply_workspace_preset", KeybindingsSubTab::Workspace, None),
    ("close_workspace", KeybindingsSubTab::Workspace, None),
    // Pane
    ("split_pane_vertical", KeybindingsSubTab::Pane, None),
    ("split_pane_horizontal", KeybindingsSubTab::Pane, None),
    ("focus_pane_next", KeybindingsSubTab::Pane, None),
    ("focus_pane_prev", KeybindingsSubTab::Pane, None),
    ("apply_pane_preset", KeybindingsSubTab::Pane, None),
    ("close_pane", KeybindingsSubTab::Pane, None),
    // Tab
    ("new_tab", KeybindingsSubTab::Tab, None),
    ("open_markdown", KeybindingsSubTab::Tab, None),
    ("open_explorer", KeybindingsSubTab::Tab, None),
    ("next_tab", KeybindingsSubTab::Tab, None),
    ("prev_tab", KeybindingsSubTab::Tab, None),
    ("rename_tab", KeybindingsSubTab::Tab, None),
    ("apply_tab_preset", KeybindingsSubTab::Tab, None),
    (
        "close_active",
        KeybindingsSubTab::Tab,
        Some("settings.keybindings.close_active_desc"),
    ),
    // Surface
    ("split_surface_vertical", KeybindingsSubTab::Surface, None),
    ("split_surface_horizontal", KeybindingsSubTab::Surface, None),
    ("focus_surface_next", KeybindingsSubTab::Surface, None),
    ("focus_surface_prev", KeybindingsSubTab::Surface, None),
    ("convert_surface", KeybindingsSubTab::Surface, None),
    ("convert_to_markdown", KeybindingsSubTab::Surface, None),
    ("convert_to_explorer", KeybindingsSubTab::Surface, None),
    (
        "refresh",
        KeybindingsSubTab::Surface,
        Some("settings.keybindings.refresh_desc"),
    ),
    ("find", KeybindingsSubTab::Surface, None),
    ("explorer_focus_address", KeybindingsSubTab::Surface, None),
    ("close_surface", KeybindingsSubTab::Surface, None),
    // Clipboard
    ("copy", KeybindingsSubTab::Clipboard, None),
    ("copy_path", KeybindingsSubTab::Clipboard, None),
    ("copy_link", KeybindingsSubTab::Clipboard, None),
    ("cut", KeybindingsSubTab::Clipboard, None),
    ("select_all", KeybindingsSubTab::Clipboard, None),
    ("paste", KeybindingsSubTab::Clipboard, None),
    ("page_undo", KeybindingsSubTab::Clipboard, None),
    ("page_redo", KeybindingsSubTab::Clipboard, None),
    (
        "screenshot_to_clipboard",
        KeybindingsSubTab::Clipboard,
        None,
    ),
    ("enter_copy_mode", KeybindingsSubTab::Clipboard, None),
    // Zoom
    ("zoom_in", KeybindingsSubTab::Zoom, None),
    ("zoom_out", KeybindingsSubTab::Zoom, None),
    ("zoom_reset", KeybindingsSubTab::Zoom, None),
    // Explorer
    ("explorer_refresh", KeybindingsSubTab::Explorer, None),
    ("explorer_go_up", KeybindingsSubTab::Explorer, None),
    ("explorer_back", KeybindingsSubTab::Explorer, None),
    ("explorer_forward", KeybindingsSubTab::Explorer, None),
    ("explorer_toggle_preview", KeybindingsSubTab::Explorer, None),
    ("explorer_toggle_hidden", KeybindingsSubTab::Explorer, None),
    ("explorer_properties", KeybindingsSubTab::Explorer, None),
    ("explorer_new_folder", KeybindingsSubTab::Explorer, None),
    ("explorer_new_file", KeybindingsSubTab::Explorer, None),
    ("explorer_rename", KeybindingsSubTab::Explorer, None),
    ("explorer_trash", KeybindingsSubTab::Explorer, None),
    ("explorer_open", KeybindingsSubTab::Explorer, None),
    (
        "explorer_clear_selection",
        KeybindingsSubTab::Explorer,
        None,
    ),
    (
        "explorer_cursor_up",
        KeybindingsSubTab::Explorer,
        Some("settings.keybindings.explorer_cursor_desc"),
    ),
    ("explorer_cursor_down", KeybindingsSubTab::Explorer, None),
    ("explorer_cursor_left", KeybindingsSubTab::Explorer, None),
    ("explorer_cursor_right", KeybindingsSubTab::Explorer, None),
    ("explorer_cursor_home", KeybindingsSubTab::Explorer, None),
    ("explorer_cursor_end", KeybindingsSubTab::Explorer, None),
    ("explorer_cursor_page_up", KeybindingsSubTab::Explorer, None),
    (
        "explorer_cursor_page_down",
        KeybindingsSubTab::Explorer,
        None,
    ),
    (
        "explorer_extend_up",
        KeybindingsSubTab::Explorer,
        Some("settings.keybindings.explorer_extend_desc"),
    ),
    ("explorer_extend_down", KeybindingsSubTab::Explorer, None),
    ("explorer_extend_left", KeybindingsSubTab::Explorer, None),
    ("explorer_extend_right", KeybindingsSubTab::Explorer, None),
    ("explorer_extend_home", KeybindingsSubTab::Explorer, None),
    ("explorer_extend_end", KeybindingsSubTab::Explorer, None),
    ("explorer_extend_page_up", KeybindingsSubTab::Explorer, None),
    (
        "explorer_extend_page_down",
        KeybindingsSubTab::Explorer,
        None,
    ),
    ("explorer_undo", KeybindingsSubTab::Explorer, None),
    ("explorer_redo", KeybindingsSubTab::Explorer, None),
];

/// 그 서브탭이 바인딩 엔트리 목록을 그리는가. Scripts/Preset/Plugins/ImportExport 는 자기
/// 화면을 따로 그린다.
fn draws_entries(sub_tab: KeybindingsSubTab) -> bool {
    !matches!(
        sub_tab,
        KeybindingsSubTab::Scripts
            | KeybindingsSubTab::Preset
            | KeybindingsSubTab::Plugins
            | KeybindingsSubTab::ImportExport
    )
}

/// 엔트리 아래에 quick-switch 섹션을 그리는 서브탭과 그 순서.
fn quick_switch_kinds(sub_tab: KeybindingsSubTab) -> &'static [QuickSwitchKind] {
    match sub_tab {
        KeybindingsSubTab::Workspace => &[QuickSwitchKind::Workspace, QuickSwitchKind::Category],
        KeybindingsSubTab::Tab => &[QuickSwitchKind::Tab],
        _ => &[],
    }
}

/// 엔트리 아래 탐색기 드래그 반전 modifier 행을 그리는 서브탭인가.
fn draws_drag_flip(sub_tab: KeybindingsSubTab) -> bool {
    sub_tab == KeybindingsSubTab::General
}

/// 바인딩 행들이 함께 쓰는 표시 조건 — 단축키 표기 방식과 서브탭의 라벨 열 폭.
#[derive(Clone, Copy)]
struct RowLayout<'a> {
    general: &'a GeneralSettings,
    label_col: LogicalPx,
}

/// 서브탭의 라벨 열. 엔트리와 quick-switch 행 라벨 가운데 가장 긴 것을 설정 행과 같은
/// `settings-label-width` … `settings-label-max-width` 로 clamp 한다. 더 긴 라벨은 열 안에서 줄을 바꾼다.
fn subtab_label_column(
    ui: &egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    entries: &[(&str, &str, Option<&str>)],
    kinds: &[QuickSwitchKind],
    drag_flip_row: bool,
) -> LogicalPx {
    let switch_labels: Vec<String> = kinds
        .iter()
        .flat_map(|&kind| quick_switch::row_labels(kind))
        .collect();
    let rows: Vec<SettingsRow<'_>> = entries
        .iter()
        .map(|(_, label_key, desc_key)| {
            let row = SettingsRow::new(t(label_key));
            match desc_key {
                Some(desc) => row.hint(t(desc)),
                None => row,
            }
        })
        .chain(switch_labels.iter().map(|label| SettingsRow::new(label)))
        .chain(drag_flip_row.then(drag_flip::row))
        .collect();
    settings_label_column(ui, th, &rows)
}

/// 정렬 전의 항목 한 줄 — 배치 순서와 (필드 id, 라벨 키, 설명 키).
type PlacedEntry<'a> = (usize, (&'a str, &'a str, Option<&'a str>));

/// 전역·입력칸 바인딩 필드와 라벨을 읽고 배치표에 따라 정렬한다.
/// 배치가 없으면 General 끝에 추가한다.
fn entries_for(
    sub_tab: KeybindingsSubTab,
) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    let mut rows: Vec<PlacedEntry<'_>> = Vec::new();
    for (field_id, label_key) in crate::settings::KeybindingSettings::binding_fields() {
        let placed = ENTRY_PLACEMENT
            .iter()
            .position(|(fid, _, _)| fid == field_id);
        let (order, tab, desc) = match placed {
            // 자체 화면을 그리는 탭은 이 목록을 표시하지 않으므로 배치 대상으로 쓰지 않는다.
            Some(i) if draws_entries(ENTRY_PLACEMENT[i].1) => {
                (i, ENTRY_PLACEMENT[i].1, ENTRY_PLACEMENT[i].2)
            }
            _ => (usize::MAX, KeybindingsSubTab::General, None),
        };
        if tab == sub_tab {
            rows.push((order, (*field_id, *label_key, desc)));
        }
    }
    rows.sort_by_key(|(order, _)| *order);
    rows.into_iter().map(|(_, entry)| entry).collect()
}

/// 녹화 중인 필드 식별자 — 어떤 필드의 어느 슬롯을 기록 중인지.
#[derive(Debug, Clone)]
pub struct RecordingSlot {
    pub field_id: String,
    /// 기존 바인딩 교체 시 인덱스, 새 바인딩 추가 시 `bindings.len()`.
    pub idx: usize,
    /// 이 슬롯의 캡처 규칙. quick-switch 슬롯이면 `BareKey`(modifier 금지).
    pub field_kind: FieldKind,
}

/// Result of key capture attempt.
#[derive(Debug, PartialEq)]
pub enum KeyCapture {
    /// No key pressed yet.
    None,
    /// User pressed Escape — clear the binding.
    Clear,
    /// A valid key combination was captured.
    Combo(String),
}

/// Keybindings 탭 콘텐츠. L2 사이드바(섹션 목록·필터·선택)는 settings 셸이
/// 소유하므로 여기서는 활성 `sub_tab` 의 바인딩 엔트리만 그린다.
#[allow(clippy::too_many_arguments)]
pub fn draw_keybindings_tab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    recording_field: &mut Option<RecordingSlot>,
    sub_tab: KeybindingsSubTab,
    selected_preset: &mut Option<String>,
    pending_binding: &mut Option<PendingBinding>,
    captured_double_tap: &mut Option<String>,
    captured_winit_combo: &mut Option<KeyCapture>,
    plugin_shortcuts: &PluginShortcutSnapshot,
    plugin_shortcuts_selected: &mut Option<String>,
    plugin_shortcuts_draft: &mut std::collections::BTreeMap<
        (String, String),
        Option<ShortcutOverride>,
    >,
    import_export: &mut ImportExportState,
    plugin_bundle: &PluginBundleContext,
) {
    let th = crate::theme::theme();
    let current = sub_tab;

    // winit에서 직접 캡처한 키 조합을 사용. 더블탭은 받는 칸에서만 조합보다 앞선다.
    let captured = double_tap_capture::take_capture(
        ui,
        recording_field,
        captured_double_tap,
        captured_winit_combo,
    );

    if draws_entries(current) {
        let entries = entries_for(current);
        let kinds = quick_switch_kinds(current);
        let layout = RowLayout {
            general: &settings.general,
            label_col: subtab_label_column(ui, &th, &entries, kinds, draws_drag_flip(current)),
        };
        draw_keybinding_entries(
            ui,
            &mut settings.keybindings,
            layout,
            recording_field,
            pending_binding,
            &captured,
            &entries,
        );
        if draws_drag_flip(current) {
            vspace(ui, th.spacing_sm);
            ui.separator();
            vspace(ui, th.spacing_xs);
            drag_flip::draw(
                ui,
                &th,
                &mut settings.keybindings,
                &settings.general,
                layout.label_col,
            );
        }
        for &kind in kinds {
            vspace(ui, th.spacing_sm);
            ui.separator();
            vspace(ui, th.spacing_xs);
            draw_quick_switch_section(
                ui,
                &mut settings.keybindings,
                layout,
                recording_field,
                pending_binding,
                &captured,
                kind,
            );
        }
    }

    match current {
        KeybindingsSubTab::Scripts => {
            draw_script_bindings(ui, settings, recording_field, &captured);
        }
        KeybindingsSubTab::Preset => {
            draw_preset_subtab(
                ui,
                &mut settings.keybindings,
                &settings.general,
                selected_preset,
            );
        }
        KeybindingsSubTab::Plugins => {
            draw_plugins_subtab(
                ui,
                plugin_shortcuts,
                plugin_shortcuts_selected,
                plugin_shortcuts_draft,
                &settings.keybindings,
                &settings.general,
                plugins::PluginRecording {
                    slot: recording_field,
                    pending: pending_binding,
                    captured: &captured,
                },
            );
        }
        KeybindingsSubTab::ImportExport => {
            draw_import_export_subtab(
                ui,
                settings,
                import_export,
                plugin_bundle,
                plugin_shortcuts,
                plugin_shortcuts_draft,
            );
        }
        // 바인딩 엔트리 서브탭은 위에서 그렸다.
        _ => {}
    }

    // Plugins 서브탭의 Custom 키도 녹화 슬롯이라 같은 Esc 안내를 둔다.
    if !matches!(
        current,
        KeybindingsSubTab::Preset | KeybindingsSubTab::ImportExport
    ) {
        vspace(ui, th.spacing_sm);
        ui.label(
            egui::RichText::new(t("settings.keybindings.hint_esc_to_clear"))
                .small()
                .color(th.text_disabled()),
        );
        double_tap_capture::draw_notices(ui, &th, settings, recording_field);
    }
}

/// Preset 서브탭: 좌측 프리셋 목록, 우측 미리보기 테이블 + 적용 버튼.
mod capture;
mod double_tap_capture;
mod drag_flip;
mod entries;
mod entries_scripts;
#[cfg(test)]
mod entries_tests;
mod import_export;
#[cfg(test)]
mod label_width;
mod plugins;
mod preset;
mod quick_switch;

pub use capture::{capture_bare_key, capture_winit_key_combo};
use entries::draw_keybinding_entries;
use entries_scripts::draw_script_bindings;
use import_export::draw_import_export_subtab;
pub(crate) use import_export::{
    EXPORT_CONSUMER, IMPORT_CONSUMER, ImportExportRequest, ImportExportState, PluginBundleContext,
    export_default_name,
};
use plugins::draw_plugins_subtab;
use preset::draw_preset_subtab;
use quick_switch::{QuickSwitchKind, draw_quick_switch_section};
pub use quick_switch::{clear_bare_target, set_bare_target};
use tasty_ui_widgets::{SettingsRow, settings_label_column, vspace};

#[cfg(test)]
mod placement_tests {
    use super::*;

    /// 배치표에 등록된 필드가 바인딩 필드 목록에도 있는지 확인한다.
    #[test]
    fn every_placement_row_points_at_a_real_field() {
        let dangling: Vec<&str> = ENTRY_PLACEMENT
            .iter()
            .map(|(fid, _, _)| *fid)
            .filter(|fid| {
                crate::settings::KeybindingSettings::binding_fields().all(|(sot, _)| sot != fid)
            })
            .collect();
        assert!(
            dangling.is_empty(),
            "바인딩 필드 목록에 없는 필드가 배치표에 있다: {dangling:?}"
        );
    }
}
