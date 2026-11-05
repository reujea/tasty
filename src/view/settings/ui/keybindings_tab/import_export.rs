//! 단축키 구성을 내보내고, 가져온 파일을 미리 본 뒤 선택한 행을 적용한다.
//! Apply는 호스트·plugin 초안만 바꾸며 설정의 Save가 저장한다(docs/design/policies/key-mapping.md).
//!
//! 계산은 model·labels·view_model·bundle_notices, 화면은 entry·diff_table·notices에 있다.
//! 치수는 갤러리의 kb_import_export와 대조하므로 이 파일에 둔다.

mod bundle_notices;
mod diff_table;
mod entry;
mod labels;
mod model;
mod notices;
mod paint;
mod view_model;

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tasty_host_plugin::keybinding_bundle::{
    BundleWarning, DecodedBundle, PluginShortcutOverrides, encode,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, DrillDown, DrillDownView};

use crate::adapters::ui::icons;
use crate::i18n::{t, t_fmt};
use crate::settings::{KeybindingSettings, Settings};
use crate::settings_ui::PluginShortcutSnapshot;

pub(crate) use model::PluginShortcutDraft;
use model::{Group, RowKey, apply_rows, merged_overrides, row_keys};

use diff_table::diff_table;
use entry::action_row;
use labels::Labels;
use notices::{ExportFailureAction, bundle_notices, dropped_notice, export_failure, parse_failure};
use paint::intro;
use view_model::build_view_model;

// 아래 전용 치수는 이름을 붙인 상수로 유지하며 gallery_copied_dimensions가 갤러리와 대조한다.
// 열 폭·카드 여백·슬롯 높이는 Theme의 `kb-ie-*` 토큰을 쓴다.
/// 그룹 헤더 chevron ↔ 그룹명, 충돌 부제 아이콘 ↔ 문구, 경고 줄 글머리 ↔ 문구 간격 — jsx `gap: 6`.
const GROUP_CHEVRON_GAP: LogicalPx = LogicalPx(6.0);
/// plugin 행 부제의 점 ↔ plugin 이름 간격 — jsx `gap: 5`.
const PLUGIN_DOT_GAP: LogicalPx = LogicalPx(5.0);

/// 내보내기 파일 선택의 결과 키.
pub(crate) const EXPORT_CONSUMER: &str = "keybindings_export";
/// 가져오기 파일 선택의 결과 키.
pub(crate) const IMPORT_CONSUMER: &str = "keybindings_import";

/// 모달을 열 때 host 가 주입하는 plugin 쪽 원본 — export 는 스냅샷이 아니라 이것에서 만든다.
#[derive(Debug, Default, Clone)]
pub struct PluginBundleContext {
    /// `PluginsConfig.keybindings` 전량(비활성·미등록 plugin 포함).
    pub overrides: PluginShortcutOverrides,
    /// 이 환경에 설치된 plugin id — 가져오기가 나머지를 버린다.
    pub installed_plugin_ids: Vec<String>,
    /// plugin id → 표시 이름.
    pub plugin_names: BTreeMap<String, String>,
}

/// 화면이 셸에 요청하는 파일 선택.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportExportRequest {
    Export,
    Import,
}

/// 읽어 들인 번들.
struct Preview {
    file_name: String,
    keybindings: KeybindingSettings,
    overrides: PluginShortcutOverrides,
    /// 이 환경에 없는 plugin 과 버린 override 수.
    dropped: Vec<(String, usize)>,
    /// 경고 블록에 오를 줄(고정 순서).
    notices: Vec<bundle_notices::BundleNotice>,
}

/// 읽지 못한 파일.
struct Failure {
    path: String,
    line: Option<usize>,
}

/// 쓰지 못한 내보내기 — 재시도가 같은 경로를 다시 쓴다.
struct ExportFailure {
    path: PathBuf,
    reason: ExportFailReason,
}

/// 내보내기 오류의 번역 문구를 선택한다. 알려진 오류는 정해진 문구로,
/// 나머지는 공통 문구와 별도 OS 오류 줄로 표시한다.
enum ExportFailReason {
    /// 파일시스템이 읽기 전용이다.
    ReadOnly,
    /// 쓸 권한이 없다.
    PermissionDenied,
    /// 볼륨이 찼다.
    DiskFull,
    /// 알아볼 수 없는 실패 — 구절은 고정이고, 담은 문장은 아래 줄로 나간다.
    Unknown(String),
    OutcomeUnknown,
}

impl ExportFailReason {
    fn of_io(e: &std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::ReadOnlyFilesystem => ExportFailReason::ReadOnly,
            std::io::ErrorKind::PermissionDenied => ExportFailReason::PermissionDenied,
            // StorageFull 대신 공용 raw OS 오류 분류를 사용한다.
            _ if e.raw_os_error() == Some(crate::db::disk_full_os_error()) => {
                ExportFailReason::DiskFull
            }
            _ => ExportFailReason::Unknown(e.to_string()),
        }
    }

    /// 공통 오류 문구 아래에 표시할 OS 오류. 미분류 오류에만 있다.
    fn os_message(&self) -> Option<&str> {
        match self {
            ExportFailReason::Unknown(message) => Some(message.as_str()),
            _ => None,
        }
    }
}

/// 서브탭 상태 — `SettingsUiState` 가 소유한다.
pub struct ImportExportState {
    pending_files: Vec<crate::app::settings_files::SettingsFileRequest>,
    export_observation: crate::app::settings_files::export::Observation,
    preview: Option<Preview>,
    failure: Option<Failure>,
    /// 내보내기 실패 — 떠 있는 동안 Export 버튼이 꺼진다.
    export_failure: Option<ExportFailure>,
    /// 경고 블록의 접힌 줄을 펼쳤는가.
    notices_expanded: bool,
    /// 디자인 기본값: 변경된 행만.
    changed_only: bool,
    collapsed: BTreeSet<Group>,
    deselected: BTreeSet<RowKey>,
    request: Option<ImportExportRequest>,
    toast: Option<String>,
}

impl Default for ImportExportState {
    fn default() -> Self {
        Self {
            pending_files: Vec::new(),
            export_observation: Default::default(),
            preview: None,
            failure: None,
            export_failure: None,
            notices_expanded: false,
            changed_only: true,
            collapsed: BTreeSet::new(),
            deselected: BTreeSet::new(),
            request: None,
            toast: None,
        }
    }
}

impl ImportExportState {
    pub(crate) fn take_file_requests(
        &mut self,
    ) -> Vec<crate::app::settings_files::SettingsFileRequest> {
        std::mem::take(&mut self.pending_files)
    }

    pub(crate) fn accept_file_result(
        &mut self,
        result: crate::app::settings_files::SettingsFileResult,
    ) {
        match result {
            crate::app::settings_files::SettingsFileResult::Export {
                result, observer, ..
            } => self.accept_export(&observer, result),
            crate::app::settings_files::SettingsFileResult::ExportTransport { error, observer } => {
                self.accept_export(&observer, Err(error))
            }
            crate::app::settings_files::SettingsFileResult::Import { path, result } => {
                self.finish_import(&path, result);
            }
        }
    }

    fn accept_export(
        &mut self,
        observer: &std::sync::Weak<()>,
        result: Result<(), crate::app::settings_files::export::ExportError>,
    ) {
        if let Some(path) = self.export_observation.finish(observer) {
            self.finish_export(
                &path,
                result.map_err(|error| match error {
                    crate::app::settings_files::export::ExportError::Io(error) => {
                        ExportFailReason::of_io(&error)
                    }
                    crate::app::settings_files::export::ExportError::Refused(message) => {
                        ExportFailReason::Unknown(message)
                    }
                    crate::app::settings_files::export::ExportError::Unknown => {
                        ExportFailReason::OutcomeUnknown
                    }
                }),
            );
        }
    }

    pub(crate) fn take_request(&mut self) -> Option<ImportExportRequest> {
        self.request.take()
    }

    /// 설정 창 자체 토스트로 올릴 문구(1 회).
    pub(crate) fn take_toast(&mut self) -> Option<String> {
        self.toast.take()
    }

    /// 현재 구성(호스트 draft + 원본 위에 얹은 plugin draft)을 `path` 에 쓴다.
    pub(crate) fn export_to(
        &mut self,
        path: &Path,
        keybindings: &KeybindingSettings,
        ctx: &PluginBundleContext,
        plugin_draft: &PluginShortcutDraft,
    ) {
        let overrides = merged_overrides(&ctx.overrides, plugin_draft);
        match encode(keybindings, &overrides) {
            Ok(text) => {
                let Some(observer) = self.export_observation.begin(path, &text) else {
                    return;
                };
                self.export_failure = None;
                self.toast = None;
                self.pending_files
                    .push(crate::app::settings_files::SettingsFileRequest::Export {
                        path: path.to_path_buf(),
                        text,
                        observer,
                    })
            }
            Err(error) => {
                self.export_observation.clear();
                tracing::error!(%error, "keybinding export encode failed");
                self.finish_export(path, Err(ExportFailReason::Unknown(error.to_string())));
            }
        }
    }

    pub(crate) fn observe_export_draft(
        &mut self,
        keybindings: &KeybindingSettings,
        ctx: &PluginBundleContext,
        draft: &PluginShortcutDraft,
    ) {
        if self.export_observation.pending() {
            match encode(keybindings, &merged_overrides(&ctx.overrides, draft)) {
                Ok(text) => self.export_observation.observe(&text),
                Err(_) => self.export_observation.clear(),
            }
        }
    }

    fn finish_export(&mut self, path: &Path, written: Result<(), ExportFailReason>) {
        match written {
            Ok(()) => {
                self.export_failure = None;
                self.toast = Some(t_fmt(
                    "settings.keybindings.ie_exported_toast",
                    &path.display().to_string(),
                ));
            }
            Err(reason) => {
                self.export_failure = Some(ExportFailure {
                    path: path.to_path_buf(),
                    reason,
                });
            }
        }
    }

    /// 파일을 읽어 미리보기를 만들거나 실패 안내를 표시한다.
    pub(crate) fn import_from(
        &mut self,
        path: &Path,
        settings: &Settings,
        ctx: &PluginBundleContext,
    ) {
        self.preview = None;
        self.failure = None;
        self.pending_files
            .push(crate::app::settings_files::SettingsFileRequest::Import {
                path: path.to_path_buf(),
                plugins: ctx.installed_plugin_ids.clone(),
                scripts: settings
                    .scripts
                    .iter()
                    .map(|script| script.id.clone())
                    .collect(),
            });
    }

    fn finish_import(&mut self, path: &Path, result: Result<DecodedBundle, Option<usize>>) {
        let decoded = match result {
            Ok(decoded) => decoded,
            Err(line) => {
                self.failure = Some(Failure {
                    path: path.display().to_string(),
                    line,
                });
                return;
            }
        };
        let dropped = dropped_plugins(path, &decoded.warnings);
        let mut notices = bundle_notices::bundle_notices(&decoded.warnings);
        notices.extend(bundle_notices::os_reserved_notice(
            &decoded.keybindings,
            &decoded.plugin_keybindings,
            tasty_settings::keybindings::os_keys::ReservedOs::current(),
        ));
        self.preview = Some(Preview {
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            keybindings: decoded.keybindings,
            overrides: decoded.plugin_keybindings,
            dropped,
            notices,
        });
        self.notices_expanded = false;
        self.changed_only = true;
        self.collapsed.clear();
        self.deselected.clear();
    }

    /// 선택한 행을 두 초안에 적용한다.
    fn apply(&mut self, settings: &mut Settings, plugin_draft: &mut PluginShortcutDraft) {
        let Some(preview) = self.preview.as_ref() else {
            return;
        };
        let (kb, ov) = (&preview.keybindings, &preview.overrides);
        let keys: Vec<RowKey> = row_keys(&settings.keybindings, kb, ov)
            .into_iter()
            .filter(|k| !self.deselected.contains(k))
            .collect();
        apply_rows(&keys, &mut settings.keybindings, plugin_draft, kb, ov);
        self.toast = Some(t("settings.keybindings.ie_applied_toast").to_string());
    }
}

/// 설치되지 않은 plugin의 생략 정보를 모은다. 별도 표시 문구가 없는 경고는 로그에 남긴다.
fn dropped_plugins(path: &Path, warnings: &[BundleWarning]) -> Vec<(String, usize)> {
    let mut dropped = Vec::new();
    for w in warnings {
        if let BundleWarning::DroppedUninstalledPlugin {
            plugin_id,
            commands,
        } = w
        {
            dropped.push((plugin_id.clone(), *commands));
        }
        if !bundle_notices::is_shown(w) {
            // 이 경고들의 표시 문구는 디자인이 정하지 않았다 — 자리를 비워 두고 로그만 남긴다.
            tracing::warn!("keybinding import {}: {w}", path.display());
        }
    }
    dropped
}
pub(crate) fn draw_import_export_subtab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut ImportExportState,
    ctx: &PluginBundleContext,
    snapshot: &PluginShortcutSnapshot,
    plugin_draft: &mut PluginShortcutDraft,
) {
    let th = crate::theme::theme();

    let export_clicked = Cell::new(false);
    let import_clicked = Cell::new(false);
    let apply_clicked = Cell::new(false);
    let toggle_clicked = Cell::new(false);
    let choose_another = Cell::new(false);
    let show_more_notices = Cell::new(false);
    let export_action = Cell::new(None::<ExportFailureAction>);

    let current_overrides = merged_overrides(&ctx.overrides, plugin_draft);
    let back_clicked = {
        let labels = Labels {
            general: &settings.general,
            scripts: &settings.scripts,
            snapshot,
            names: &ctx.plugin_names,
        };
        let vm = state.preview.as_ref().map(|p| {
            build_view_model(p, state, &settings.keybindings, &current_overrides, &labels)
        });
        let failed = state.failure.is_some();
        let view = if vm.is_some() || failed {
            DrillDownView::Detail
        } else {
            DrillDownView::List
        };
        let (total, picked) = vm.as_ref().map_or((0, 0), |vm| (vm.total, vm.picked));
        let changed_only = state.changed_only;

        let actions = |ui: &mut egui::Ui, th: &Theme| {
            if failed {
                return;
            }
            if Button::new(t("settings.keybindings.apply_button"))
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Sm)
                .enabled(picked > 0)
                .show(ui, th)
                .clicked()
            {
                apply_clicked.set(true);
            }
            let label = if changed_only {
                t_fmt("settings.keybindings.ie_show_all", &total.to_string())
            } else {
                t("settings.keybindings.ie_changed_only").to_string()
            };
            if Button::new(&label)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, th)
                .clicked()
            {
                toggle_clicked.set(true);
            }
        };

        let failure = state.failure.as_ref();
        let export_fail = state.export_failure.as_ref();
        let notices_expanded = state.notices_expanded;
        let out = DrillDown::new("settings_kb_import_export")
            .view(view)
            .title(t("settings.keybindings.ie_detail_title"))
            .back_label(t("settings.keybindings.preset_back"))
            .show(
                ui,
                &th,
                |ui, th| {
                    egui::Frame::new()
                        .inner_margin(tasty_ui_widgets::margin_sym(th.spacing_lg, th.spacing_md))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                            intro(
                                ui,
                                th,
                                th.measure_md,
                                t("settings.keybindings.ie_entry_intro"),
                            );
                            let mut export_notice = |ui: &mut egui::Ui| {
                                if let Some(f) = export_fail
                                    && let Some(action) = export_failure(ui, th, f)
                                {
                                    export_action.set(Some(action));
                                }
                            };
                            if action_row(
                                ui,
                                th,
                                icons::DOWNLOAD,
                                t("settings.keybindings.ie_export_title"),
                                t("settings.keybindings.ie_export_desc"),
                                t("settings.keybindings.ie_export_button"),
                                ButtonVariant::Secondary,
                                export_fail.is_none() && !state.export_observation.pending(),
                                state
                                    .export_observation
                                    .pending()
                                    .then(|| t("settings.keybindings.ie_export_pending")),
                                export_fail
                                    .map(|_| &mut export_notice as &mut dyn FnMut(&mut egui::Ui)),
                            ) {
                                export_clicked.set(true);
                            }
                            if action_row(
                                ui,
                                th,
                                icons::FILE,
                                t("settings.keybindings.ie_import_title"),
                                t("settings.keybindings.ie_import_desc"),
                                t("settings.keybindings.ie_import_button"),
                                ButtonVariant::Primary,
                                true,
                                None,
                                None,
                            ) {
                                import_clicked.set(true);
                            }
                        });
                },
                |ui, th| {
                    egui::Frame::new()
                        .inner_margin(tasty_ui_widgets::margin_all(th.spacing_lg))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                            if let Some(f) = failure {
                                if parse_failure(ui, th, f) {
                                    choose_another.set(true);
                                }
                                return;
                            }
                            let Some(vm) = vm.as_ref() else {
                                return;
                            };
                            intro(ui, th, th.measure_lg, &vm.intro);
                            if let Some(dropped) = &vm.dropped {
                                dropped_notice(ui, th, dropped);
                            }
                            if !vm.notices.is_empty()
                                && bundle_notices(ui, th, &vm.notices, notices_expanded)
                            {
                                show_more_notices.set(true);
                            }
                            diff_table(
                                ui,
                                th,
                                vm,
                                state.changed_only,
                                &mut state.collapsed,
                                &mut state.deselected,
                            );
                        });
                },
                Some(&actions),
            );
        out.back_clicked
    };

    if export_clicked.get() {
        state.request = Some(ImportExportRequest::Export);
    }
    match export_action.get() {
        Some(ExportFailureAction::Retry) => {
            if let Some(path) = state.export_failure.as_ref().map(|f| f.path.clone()) {
                state.export_to(&path, &settings.keybindings, ctx, plugin_draft);
            }
        }
        Some(ExportFailureAction::ChooseAnother) => {
            state.export_failure = None;
            state.request = Some(ImportExportRequest::Export);
        }
        None => {}
    }
    if show_more_notices.get() {
        state.notices_expanded = true;
    }
    if import_clicked.get() || choose_another.get() {
        state.failure = None;
        state.request = Some(ImportExportRequest::Import);
    }
    if toggle_clicked.get() {
        state.changed_only = !state.changed_only;
    }
    if back_clicked {
        state.preview = None;
        state.failure = None;
    }
    if apply_clicked.get() {
        state.apply(settings, plugin_draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_feedback_waits_for_app_result_and_recovers_after_retry() {
        let mut state = ImportExportState::default();
        let path = PathBuf::from("keybindings.toml");
        state.export_to(
            &path,
            &KeybindingSettings::default(),
            &PluginBundleContext::default(),
            &PluginShortcutDraft::new(),
        );
        let crate::app::settings_files::SettingsFileRequest::Export { observer, .. } =
            state.take_file_requests().pop().unwrap()
        else {
            panic!("export")
        };
        assert!(state.toast.is_none());
        state.accept_file_result(crate::app::settings_files::SettingsFileResult::Export {
            path: path.clone(),
            observer,
            result: Err(crate::app::settings_files::export::ExportError::Io(
                std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            )),
        });
        assert!(matches!(
            state.export_failure.as_ref().unwrap().reason,
            ExportFailReason::PermissionDenied
        ));
        assert!(state.toast.is_none());
        state.export_to(
            &path,
            &KeybindingSettings::default(),
            &PluginBundleContext::default(),
            &PluginShortcutDraft::new(),
        );
        let crate::app::settings_files::SettingsFileRequest::Export { observer, .. } =
            state.take_file_requests().pop().unwrap()
        else {
            panic!("retry")
        };
        state.accept_file_result(crate::app::settings_files::SettingsFileResult::Export {
            path,
            observer,
            result: Ok(()),
        });
        assert!(state.export_failure.is_none());
        assert!(state.take_toast().is_some());
    }

    /// 알려진 오류는 번역 문구만, 미분류 오류는 OS 오류도 표시한다.
    #[test]
    fn each_recognised_cause_takes_its_own_clause_and_only_the_catch_all_carries_the_os_text() {
        use std::io::{Error, ErrorKind};

        let read_only = ExportFailReason::of_io(&Error::from(ErrorKind::ReadOnlyFilesystem));
        assert!(matches!(read_only, ExportFailReason::ReadOnly));
        assert!(read_only.os_message().is_none());

        let denied = ExportFailReason::of_io(&Error::from(ErrorKind::PermissionDenied));
        assert!(matches!(denied, ExportFailReason::PermissionDenied));
        assert!(denied.os_message().is_none());

        let full =
            ExportFailReason::of_io(&Error::from_raw_os_error(crate::db::disk_full_os_error()));
        assert!(
            matches!(full, ExportFailReason::DiskFull),
            "OS의 용량 부족 오류가 DiskFull로 분류되어야 한다"
        );
        assert!(full.os_message().is_none());

        let unknown = ExportFailReason::of_io(&Error::from(ErrorKind::InvalidData));
        assert!(matches!(unknown, ExportFailReason::Unknown(_)));
        assert!(
            unknown.os_message().is_some(),
            "미분류 오류에는 원래 OS 오류 문구가 포함되어야 한다"
        );
    }
}

#[cfg(test)]
mod observation_tests;
