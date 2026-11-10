#[cfg(debug_assertions)]
mod debug_tabs;
mod export_status;
mod file_handler_tab;
mod keybindings_tab;
#[cfg(test)]
mod select_probe;
mod tabs;

pub(crate) use crate::view::file_chooser;

use crate::runtime::file_catalog::FormatCatalog as FileFormatRegistry;

use file_handler_tab::{FileHandlerSubTab, draw_file_handler_tab};
use keybindings_tab::{
    EXPORT_CONSUMER, FieldKind, IMPORT_CONSUMER, ImportExportRequest, ImportExportState,
    KeybindingsSubTab, PendingBinding, RecordingSlot, clear_bare_target, draw_keybindings_tab,
    set_bare_target,
};
use tabs::*;

#[cfg(debug_assertions)]
pub(crate) use debug_tabs::DebugTabsApplied;
pub(crate) use keybindings_tab::PluginBundleContext;
pub use keybindings_tab::{KeyCapture, capture_bare_key, capture_winit_key_combo};

use crate::adapters::ui::popup::{PopupManager, PopupState};
use crate::file::format::DetectorId;
use crate::i18n::t;
use crate::plugin::manifest::BindingMode;
use crate::plugin::registry_state::ShortcutOverride;
use crate::runtime::file_catalog::HandlerCatalog as FileHandlerRegistry;
use crate::settings::Settings;
use tasty_host_plugin::SettingsPageEntry;
use tasty_plugin_manifest::SettingsCategory;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::vspace;
use tasty_ui_widgets::{Button, ButtonVariant};

/// L2 사이드바 폭. 디자인 `--tasty-settings-sidebar-width` = 200.
const SETTINGS_SIDEBAR_WIDTH: LogicalPx = LogicalPx(200.0);
/// L1 헤더 밴드 높이. 디자인 header `height: 44`. `size-44` 이지만 이 헤더 높이라는 역할의
/// 토큰이 없어 원시 값을 이름 붙여 둔다.
const SETTINGS_HEADER_HEIGHT: LogicalPx = LogicalPx(44.0);
/// active L1 탭 하단 인디케이터 두께. 디자인 `border-bottom: 2px accent`.
const SETTINGS_TAB_UNDERLINE: LogicalPx = LogicalPx(2.0);
/// L1 탭 사이 간격. 디자인 header `gap: 2`.
const L1_TAB_GAP: LogicalPx = LogicalPx(2.0);
/// 설정 제목과 탭 사이의 구분선 높이.
const SETTINGS_TITLE_DIVIDER_HEIGHT: LogicalPx = LogicalPx(20.0);
/// 구분선 우측 여백. 디자인 jsx:468 `margin: 0 size-14 0 space-sm` 의 size-14.
const SETTINGS_TITLE_DIVIDER_MARGIN_R: LogicalPx = LogicalPx(14.0);
/// 푸터 좌우 패딩. 디자인 footer `padding: space-md size-14` 의 수평값.
const SETTINGS_FOOTER_PAD_X: i8 = 14;

/// plugin 명령의 단축키 표시 자료. 저장된 override가 없으면 매니페스트 기본값을 쓴다.
#[derive(Debug, Clone)]
pub struct PluginShortcutRow {
    pub plugin_id: String,
    pub plugin_name: String,
    pub command_id: String,
    pub title_i18n_key: String,
    pub binding_mode: BindingMode,
    pub manifest_default: Option<String>,
    pub current_override: Option<ShortcutOverride>,
}

#[derive(Debug, Default, Clone)]
pub struct PluginShortcutSnapshot {
    pub rows: Vec<PluginShortcutRow>,
}

/// Sub-tab within the Appearance tab.
///
/// Host-internal variants (`Theme` / `General` / `Display` / `Terminal`) are
/// hardcoded; plugin-contributed pages appear as
/// `Plugin { plugin_id, page_id }` and are resolved against
/// `SettingsUiState::settings_pages` at render time. `page_id` 단독으로는
/// 서로 다른 plugin 이 동일 id 를 contribute 할 경우 충돌하므로
/// `(plugin_id, page_id)` 복합키로 식별한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppearanceSubTab {
    Theme,
    /// 현재 프리셋의 색을 개별 override 하는 picker (theme_overrides 편집).
    Colors,
    General,
    /// UI scale (sm/md/lg) 전용 섹션.
    Display,
    /// app-chrome 테마 (accent / sidebar bg / active tab indicator) 전용 섹션.
    Tasty,
    Terminal,
    /// 내장 Explorer surface의 폰트 설정.
    Explorer,
    /// Plugin-contributed sub-tab. 복합키:
    /// - `plugin_id` = `SettingsPageEntry::plugin_id`
    /// - `page_id` = `SettingsPageContribute::id` (plugin scope 내)
    Plugin {
        plugin_id: String,
        page_id: String,
    },
}

/// Sub-tab within the Plugin tab. Plugin-contributed pages keyed by
/// `(plugin_id, page_id)` — 동일 `page_id` 를 contribute 한 두 plugin 이 충돌하지
/// 않도록 복합키 사용.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginSubTab {
    Plugin { plugin_id: String, page_id: String },
}

/// General 탭의 하위 섹션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneralSubTab {
    General,
    Notifications,
    Accessibility,
    /// 오버레이류(토스트 등) 표시 설정. 현재는 토스트 수명 1행.
    Overlay,
    /// 원격(mirror) 파일 전송 수신측 저장 정책(저장 폴더 + 용량 상한). 백엔드는
    /// `RemoteTransferSettings`.
    RemoteTransfer,
    /// 원격 attach 연결의 무응답 끊김 시간. 백엔드는 `RemoteAttachSettings`.
    RemoteAttach,
    /// macOS의 Alt/Option/Shift 표시 방식. 다른 플랫폼에서는 목록에 넣지 않는다.
    // 이유: 이 variant 를 push 하는 것이 macOS 전용 분기뿐이다(위).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Display,
    /// macOS TCC 권한 상태와 시스템 설정 바로가기.
    // 이유: `Display` 와 같다 — macOS 전용 분기만 push 한다.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MacosPermissions,
}

/// Terminal 탭의 하위 섹션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSubTab {
    General,
    Input,
    MouseCapture,
    Tui,
    Performance,
}

/// Misc 탭의 하위 섹션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiscSubTab {
    Scripts,
    TaskPipeline,
    AnomalyDetection,
}

/// Active L1 tab in the settings window. 디자인 2-level IA 의 상단 7탭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Terminal,
    Appearance,
    Keybindings,
    FileHandler,
    Misc,
    Plugins,
}

/// Persistent state for the settings UI between frames.
pub struct SettingsUiState {
    active_tab: SettingsTab,
    /// Working copy of settings being edited.
    draft: Option<Settings>,
    /// 내장 언어와 언어팩 목록. 처음 그릴 때 읽으며 디렉터리 변경은 창을 다시 열어야 반영된다.
    languages: Option<Vec<crate::i18n::LanguageEntry>>,
    /// Which keybinding field+slot is currently recording input (None = not recording).
    recording_field: Option<RecordingSlot>,
    /// Active sub-tab within keybindings.
    keybindings_sub_tab: KeybindingsSubTab,
    /// Active sub-tab within appearance.
    appearance_sub_tab: AppearanceSubTab,
    /// Active sub-tab within plugin tab. `None` = 등록된 plugin page 가 없거나
    /// 사용자가 아직 어떤 sub-tab 도 선택하지 않은 상태.
    pub(crate) plugin_sub_tab: Option<PluginSubTab>,
    /// Active L2 section within the General L1 tab.
    general_sub_tab: GeneralSubTab,
    /// Active L2 section within the Terminal L1 tab.
    terminal_sub_tab: TerminalSubTab,
    /// Active L2 section within the Misc L1 tab.
    misc_sub_tab: MiscSubTab,
    /// Misc › Scripts 관리 창의 UI-only 상호작용 상태 (add-card / 인라인
    /// rename·remove draft + changed 캐시). 스크립트 데이터 자체는 `draft.scripts`.
    scripts: ScriptsUiState,
    /// L2 사이드바 섹션 필터 텍스트. L1 전환 시 클리어. 7개 L1 탭이 공유한다
    /// (디자인은 한 번에 하나의 L1 만 보이므로 단일 필드로 충분).
    l2_filter: String,
    /// Active sub-tab within FileHandler.
    file_handler_sub_tab: FileHandlerSubTab,
    /// FileHandler 탭의 Extension Mapping draft. None 이면 첫 진입 시 registry 에서 초기화.
    /// 키 = 확장자 (소문자, 점 없음), 값 = 정렬된 detector id 리스트 (빈 리스트 = 클리어).
    pub(crate) extension_priority_draft:
        Option<std::collections::BTreeMap<String, Vec<DetectorId>>>,
    /// Remove·Reset 을 눌러 Save 를 기다리는 확장자와 누르기 전 draft 값(`None` = draft 에 없었다).
    /// Undo 는 이 값으로 그 확장자 하나만 되돌린다. Save·Cancel 때 draft 와 함께 비운다.
    pub(crate) extension_priority_pending:
        std::collections::BTreeMap<String, Option<Vec<DetectorId>>>,
    /// 사용자가 새 확장자 추가 시 입력하는 텍스트 (Extension Mapping sub-tab).
    pub(crate) extension_priority_new_input: String,
    /// FileHandler 탭의 Detectors/Handlers sub-tab 편집 draft. Save 시 registry 에 commit +
    /// 디스크 저장. Cancel 시 폐기.
    pub(crate) fh_edit_draft: file_handler_tab::FileHandlerEditDraft,
    /// 훅 핸들러 변경 초안. Save하면 레지스트리·사용자 설정 파일에 저장하고 Cancel하면 버린다.
    pub(crate) hook_edit_draft: file_handler_tab::HookHandlerEditDraft,
    /// Currently previewed preset name in the Preset sub-tab (None = no preview).
    selected_preset: Option<String>,
    /// Pending keybinding assignment waiting for conflict confirmation.
    pending_binding: Option<PendingBinding>,
    /// Popup manager for settings-window popups (e.g. keybinding conflict).
    popups: PopupManager,
    /// 설정 창 안의 로컬 파일 선택(`file_chooser`). popup 은 위 `popups` 에 등록된다.
    file_chooser: file_chooser::LocalFileChooser,
    /// 충돌 팝업에서 수락/거부 결과를 전달하는 플래그.
    conflict_accepted: bool,
    conflict_cancelled: bool,
    /// Cached system font family list.
    pub font_families: Option<Vec<String>>,
    /// 미리보기에 등록된 글꼴과 글꼴 행이 읽는 판정 결과.
    pub preview_fonts: PreviewFonts,
    /// winit KeyboardInput에서 직접 캡처한 키 조합 (녹화 중일 때 사용).
    pub captured_winit_combo: Option<KeyCapture>,
    /// Plugins 서브탭이 표시할 plugin command snapshot (모달 오픈 시 1회 채워짐).
    pub plugin_shortcuts: PluginShortcutSnapshot,
    /// Plugins 서브탭에서 현재 선택된 plugin id.
    pub plugin_shortcuts_selected: Option<String>,
    /// 사용자가 Plugins 서브탭에서 변경한 override draft.
    /// 키 = (plugin_id, command_id), 값:
    /// - `Some(ShortcutOverride)`: 새 override 적용
    /// - `None`: clear (매니페스트 default로 복귀)
    ///
    /// Save로 닫았을 때만 App이 가져가 PluginsConfig.keybindings와 디스크에 반영한다.
    pub plugin_shortcuts_draft:
        std::collections::BTreeMap<(String, String), Option<ShortcutOverride>>,
    /// Plugin 이 contribute 한 settings page 들의 스냅샷. 모달 오픈 시
    /// host 의 `PluginManager::settings_pages` 에서 복사. 외관 탭의 sub-tab
    /// 합성과 plugin page 렌더링에서 참조한다. 비어 있으면 plugin sub-tab
    /// 자체가 표시되지 않는다 (= dead-setting 미노출 정책).
    pub settings_pages: Vec<SettingsPageEntry>,
    /// Keybindings › Import / Export 서브탭 상태(미리보기 · 행 선택 · 요청).
    import_export: ImportExportState,
    /// 모달 오픈 시 host 가 주입하는 plugin override 원본 · 설치 plugin — export/import 가 쓴다.
    plugin_bundle: PluginBundleContext,
}

impl SettingsUiState {
    pub(crate) fn poll_local_reads(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) -> bool {
        self.file_chooser.poll_local_reads(owner)
    }

    /// 단축키 녹화 중인지 여부.
    pub fn is_recording(&self) -> bool {
        self.recording_field.is_some()
    }

    /// 현재 녹화 중인 슬롯이 quick-switch bare-key 슬롯인지 여부.
    /// (일반 콤보 캡처 vs bare-key 캡처 분기에 사용.)
    pub fn recording_is_bare_key(&self) -> bool {
        matches!(
            &self.recording_field,
            Some(slot) if matches!(slot.field_kind, FieldKind::BareKey(_))
        )
    }

    /// Plugins 모달의 `Configure` 진입점에서 호출 — 첫 진입 탭을 `Plugin` 으로 설정.
    pub fn select_plugin_tab(&mut self) {
        self.active_tab = SettingsTab::Plugins;
    }

    /// file handler picker popup 의 "설정에서 핸들러 등록" 클릭에서 호출 — 첫
    /// 진입 탭을 `FileHandler` 로 설정. `select_plugin_tab` 과 동일하게 release
    /// 빌드에서도 동작하는 일반 기능(디버그 전용 `select_tab_by_key` 와 다름).
    pub fn select_file_handler_tab(&mut self) {
        self.active_tab = SettingsTab::FileHandler;
    }

    /// 부팅 권한 안내의 [권한 설정 열기]에서 호출한다. 첫 화면을 일반 > 권한으로 맞추기
    /// 위해 L1과 L2를 함께 정한다. 권한 L2는 macOS에서만 목록에 들어가므로
    /// (`general_l2_sections`) 다른 OS에서 이 값이 선택되면 본문이 비어 보인다. 다만 이
    /// 함수를 부르는 곳이 macOS 전용 안내 하나뿐이라 그 조합은 실제로 생기지 않는다.
    pub fn select_macos_permissions_tab(&mut self) {
        self.active_tab = SettingsTab::General;
        self.general_sub_tab = GeneralSubTab::MacosPermissions;
        // 권한 화면에 들어오는 것도 스냅샷을 다시 측정하는 시점이다
        // (`apply_l2_select`와 같은 이유).
        crate::macos_permissions::refresh_permission_snapshot();
    }

    /// debug 전용 — 탭 키 문자열로 `active_tab` 을 설정한다 (`debug.settings.open`).
    /// 알 수 없는 키면 `false` 를 반환하고 탭을 바꾸지 않는다.
    #[cfg(debug_assertions)]
    pub fn select_tab_by_key(&mut self, key: &str) -> bool {
        let tab = match key {
            "general" => SettingsTab::General,
            "terminal" => SettingsTab::Terminal,
            "appearance" => SettingsTab::Appearance,
            "keybindings" => SettingsTab::Keybindings,
            // 표시 라벨은 "Handler" 로 일반화됐지만 내부 key 는 FileHandler 유지 —
            // 기존 file_handler 계열 키도 하위호환으로 계속 받는다.
            "handler" | "file_handler" | "file-handler" | "filehandler" => SettingsTab::FileHandler,
            "misc" => SettingsTab::Misc,
            "plugins" => SettingsTab::Plugins,
            _ => return false,
        };
        self.active_tab = tab;
        true
    }

    /// debug 전용 — 현재 활성 L1 탭의 L2 섹션(하위탭)을 키 문자열로 선택한다
    /// (`debug.settings.open` 의 `subtab` 인자). 반드시 [`select_tab_by_key`] 로
    /// L1 을 먼저 정한 뒤 호출한다 — 키는 활성 L1 탭에 종속이다.
    ///
    /// 알 수 없는 키(또는 해당 L1 이 정적 L2 키를 갖지 않는 경우, 예: Plugins 의
    /// 동적 plugin page)면 `false` 를 반환하고 섹션을 바꾸지 않아 L1 기본 L2 가
    /// 유지된다.
    #[cfg(debug_assertions)]
    pub fn select_section_by_key(&mut self, key: &str) -> bool {
        match self.active_tab {
            SettingsTab::General => {
                self.general_sub_tab = match key {
                    "general" => GeneralSubTab::General,
                    "notifications" => GeneralSubTab::Notifications,
                    "accessibility" => GeneralSubTab::Accessibility,
                    "overlay" => GeneralSubTab::Overlay,
                    "remote_transfer" | "remote-transfer" => GeneralSubTab::RemoteTransfer,
                    "remote_attach" | "remote-attach" => GeneralSubTab::RemoteAttach,
                    "display" => GeneralSubTab::Display,
                    "macos_permissions" | "macos-permissions" => GeneralSubTab::MacosPermissions,
                    _ => return false,
                };
                true
            }
            SettingsTab::Terminal => {
                self.terminal_sub_tab = match key {
                    "general" => TerminalSubTab::General,
                    "input" => TerminalSubTab::Input,
                    "mouse_capture" => TerminalSubTab::MouseCapture,
                    "tui" => TerminalSubTab::Tui,
                    "performance" => TerminalSubTab::Performance,
                    _ => return false,
                };
                true
            }
            SettingsTab::Appearance => {
                self.appearance_sub_tab = match key {
                    "theme" => AppearanceSubTab::Theme,
                    "colors" => AppearanceSubTab::Colors,
                    "general" => AppearanceSubTab::General,
                    "display" => AppearanceSubTab::Display,
                    "tasty" => AppearanceSubTab::Tasty,
                    "terminal" => AppearanceSubTab::Terminal,
                    "explorer" => AppearanceSubTab::Explorer,
                    _ => return false,
                };
                true
            }
            SettingsTab::Keybindings => {
                self.keybindings_sub_tab = match key {
                    "general" => KeybindingsSubTab::General,
                    "workspace" => KeybindingsSubTab::Workspace,
                    "pane" => KeybindingsSubTab::Pane,
                    "tab" => KeybindingsSubTab::Tab,
                    "surface" => KeybindingsSubTab::Surface,
                    "clipboard" => KeybindingsSubTab::Clipboard,
                    "zoom" => KeybindingsSubTab::Zoom,
                    "explorer" => KeybindingsSubTab::Explorer,
                    "scripts" => KeybindingsSubTab::Scripts,
                    "preset" => KeybindingsSubTab::Preset,
                    "plugins" => KeybindingsSubTab::Plugins,
                    "import_export" | "import-export" => KeybindingsSubTab::ImportExport,
                    _ => return false,
                };
                true
            }
            SettingsTab::FileHandler => {
                self.file_handler_sub_tab = match key {
                    "extension_mapping" | "extension-mapping" | "extensionmapping" => {
                        FileHandlerSubTab::ExtensionMapping
                    }
                    "detectors" => FileHandlerSubTab::Detectors,
                    "handlers" => FileHandlerSubTab::Handlers,
                    "hook_handlers" | "hook-handlers" | "hookhandlers" => {
                        FileHandlerSubTab::HookHandlers
                    }
                    _ => return false,
                };
                true
            }
            SettingsTab::Misc => {
                self.misc_sub_tab = match key {
                    "scripts" => MiscSubTab::Scripts,
                    "task_pipeline" | "task-pipeline" => MiscSubTab::TaskPipeline,
                    "anomaly_detection" | "anomaly-detection" => MiscSubTab::AnomalyDetection,
                    _ => return false,
                };
                true
            }
            // Plugins L2 는 plugin 이 동적으로 contribute 한 page (복합키
            // `(plugin_id, page_id)`) 라 정적 키로 주소화하지 않는다.
            SettingsTab::Plugins => false,
        }
    }

    pub fn new() -> Self {
        let mut popups = PopupManager::new();
        // 크기는 열 때 `conflict_popup_size` 가 다시 정하므로 등록 값은 첫 프레임 placeholder 다.
        popups.register(
            PopupState::new(
                "keybinding_conflict",
                t("settings.keybindings.conflict_title"),
                egui::vec2(340.0, 120.0),
            )
            .with_close_on_outside_click(false),
        );
        file_chooser::register(&mut popups);
        Self {
            active_tab: SettingsTab::General,
            draft: None,
            languages: None,
            recording_field: None,
            keybindings_sub_tab: KeybindingsSubTab::General,
            appearance_sub_tab: AppearanceSubTab::General,
            plugin_sub_tab: None,
            general_sub_tab: GeneralSubTab::General,
            terminal_sub_tab: TerminalSubTab::General,
            misc_sub_tab: MiscSubTab::Scripts,
            scripts: ScriptsUiState::default(),
            l2_filter: String::new(),
            file_handler_sub_tab: FileHandlerSubTab::ExtensionMapping,
            extension_priority_draft: None,
            extension_priority_pending: Default::default(),
            extension_priority_new_input: String::new(),
            fh_edit_draft: file_handler_tab::FileHandlerEditDraft::default(),
            hook_edit_draft: file_handler_tab::HookHandlerEditDraft::default(),
            selected_preset: None,
            pending_binding: None,
            popups,
            file_chooser: file_chooser::LocalFileChooser::default(),
            conflict_accepted: false,
            conflict_cancelled: false,
            font_families: None,
            preview_fonts: PreviewFonts::default(),
            captured_winit_combo: None,
            plugin_shortcuts: PluginShortcutSnapshot::default(),
            plugin_shortcuts_selected: None,
            plugin_shortcuts_draft: std::collections::BTreeMap::new(),
            settings_pages: Vec::new(),
            import_export: ImportExportState::default(),
            plugin_bundle: PluginBundleContext::default(),
        }
    }

    /// 설정 창 안에서 로컬 파일 선택 popup 을 연다. 결과는 `consumer` 키로
    /// `file_chooser.take_outcome` 해 가져간다. 이미 열려 있던 선택은 취소로 끝난다.
    ///
    /// `title` 을 주면 popup 타이틀과 view 헤더가 모드 기본 문구 대신 그것을 쓴다.
    fn open_file_chooser(
        &mut self,
        consumer: &'static str,
        mode: file_chooser::FileChooserMode,
        filters: Vec<String>,
        title: Option<&'static str>,
    ) {
        self.open_file_chooser_at(consumer, mode, filters, title, None);
    }

    fn open_file_chooser_at(
        &mut self,
        consumer: &'static str,
        mode: file_chooser::FileChooserMode,
        filters: Vec<String>,
        title: Option<&'static str>,
        start: Option<std::path::PathBuf>,
    ) {
        if self.file_chooser.is_pending() {
            return;
        }
        if let Some(start) = start {
            self.file_chooser.begin_at(consumer, mode, filters, start);
        } else {
            self.file_chooser.begin(consumer, mode, filters);
        }
        if let Some(title) = title {
            self.file_chooser.set_title(title);
        }
        let title = self.file_chooser.title();
        if let Some(p) = self.popups.get_mut(file_chooser::FILE_CHOOSER_POPUP_ID) {
            p.size = if self.file_chooser.is_folder() {
                file_chooser::folder_size()
            } else {
                file_chooser::chooser_size()
            };
            p.title = title.to_string();
        }
        // intent-exempt: 설정 창 내부 PopupManager 의 sub-popup open(충돌 팝업과 같은 경로).
        self.popups
            .open_centered_focused(file_chooser::FILE_CHOOSER_POPUP_ID);
    }

    /// Plugin 이 contribute 한 settings page 스냅샷을 주입한다. 모달 오픈 직전에
    /// host App 이 호출한다. 빈 vec 으로 호출하면 plugin sub-tab 이 사라진다.
    pub fn set_settings_pages(&mut self, pages: Vec<SettingsPageEntry>) {
        self.settings_pages = pages;
    }

    /// plugin override 원본 · 설치 plugin 을 주입한다. 모달 오픈 직전에 host App 이 호출한다.
    pub fn set_plugin_bundle_context(&mut self, ctx: PluginBundleContext) {
        self.plugin_bundle = ctx;
    }

    /// 설정 창 자체 토스트로 올릴 가져오기/내보내기 결과 문구(1 회).
    pub(crate) fn take_file_requests(
        &mut self,
    ) -> Vec<crate::app::settings_files::SettingsFileRequest> {
        self.import_export.take_file_requests()
    }

    pub(crate) fn accept_file_result(
        &mut self,
        result: crate::app::settings_files::SettingsFileResult,
    ) {
        self.invalidate_edited_export();
        self.import_export.accept_file_result(result);
    }

    pub fn take_import_export_toast(&mut self) -> Option<String> {
        self.import_export.take_toast()
    }
}

/// draw_settings_panel에 전달하는 데이터와 처리 대상.
pub struct SettingsPanelCtx<'a> {
    pub settings: &'a mut Settings,
    pub ui_state: &'a mut SettingsUiState,
    pub captured_double_tap: &'a mut Option<String>,
    pub file_format: &'a FileFormatRegistry,
    pub file_handler: &'a FileHandlerRegistry,
}

/// 단축키 충돌 팝업에 표시할 안내 문자열을 구성한다. 팝업 draw 와 open 직전
/// 크기 산정 양쪽이 같은 문자열을 쓰도록 단일 출처로 둔다.
fn conflict_message_text(
    pending: &PendingBinding,
    general: &crate::settings::GeneralSettings,
) -> String {
    let conflict_label = if let Some(label) = &pending.conflicting_label {
        label.clone()
    } else {
        let raw = crate::settings::KeybindingSettings::label_key_for(&pending.conflicting_field)
            .map(t)
            .unwrap_or(pending.conflicting_field.as_str());
        raw.trim_end_matches(':').trim().to_string()
    };
    let combo_display =
        crate::settings::KeybindingSettings::format_display(&pending.combo, general);
    crate::i18n::t_fmt2(
        "settings.keybindings.conflict_message",
        &combo_display,
        &conflict_label,
    )
}

/// 폰트·언어·배율에 따라 달라지는 안내문 높이를 측정해 팝업 크기를 정한다.
/// 고정 높이를 쓰면 긴 안내문 아래의 버튼이 잘릴 수 있다.
fn conflict_popup_size(ui: &egui::Ui, th: &Theme, message: String, zoom: f32) -> egui::Vec2 {
    use crate::adapters::ui::popup::content_margin;
    let width = (340.0 * zoom).round();
    let content_w = (LogicalPx(width) - content_margin().scaled(2.0)).max(LogicalPx(1.0));
    let galley = ui.fonts(|f| {
        f.layout(
            message,
            egui::FontId::proportional(th.font_size_body.value()),
            egui::Color32::WHITE, // 측정 전용 — 색은 높이에 무관
            content_w.value(),
        )
    });
    conflict_popup_dims(th, galley.size().y, zoom)
}

/// 측정한 안내문 높이에 타이틀바·여백·버튼 공간을 더한다.
/// 버튼 높이는 배율을 반영한 item_height_interactive로 근사한다.
fn conflict_popup_dims(th: &Theme, label_h: f32, zoom: f32) -> egui::Vec2 {
    use crate::adapters::ui::popup::{content_margin, title_bar_height};
    let width = (340.0 * zoom).round();
    let margin = content_margin();
    let height = title_bar_height()
        + margin
        + LogicalPx(label_h)
        + th.spacing_sm
        + th.item_height_interactive
        + margin
        + th.spacing_xs;
    egui::vec2(width, height.value().round())
}

/// Draw settings directly as a full-window panel (for modal windows).
/// Returns true if Save was clicked, false if Cancel was clicked, None otherwise.
pub fn draw_settings_panel(ctx: &egui::Context, panel: SettingsPanelCtx<'_>) -> Option<bool> {
    let SettingsPanelCtx {
        settings,
        ui_state,
        captured_double_tap,
        file_format,
        file_handler,
    } = panel;
    if ui_state.draft.is_none() {
        ui_state.draft = Some(settings.clone());
        // 새 오픈마다 Scripts 관리 창 상호작용 상태·changed 캐시 리셋(다음 draw 에서
        // 디스크 해시 재계산).
        ui_state.scripts = ScriptsUiState::default();
    }

    // Lazily load system font list on first access
    if ui_state.font_families.is_none() {
        let font_config = crate::font::FontConfig::new(14.0, "");
        ui_state.font_families = Some(font_config.list_families());
    }

    // 언어팩 디렉터리는 처음 한 번만 읽는다.
    if ui_state.languages.is_none() {
        ui_state.languages = Some(crate::i18n::available_languages());
    }

    let mut result = None;
    let th = crate::theme::theme();
    let sep = egui::Stroke::new(th.border_width.value(), th.border_strong().to_egui());

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(th.bg_panel().to_egui()))
        .show(ctx, |ui| {
            // ── L1 헤더 밴드 (디자인 header height44 / bg-sidebar / border-bottom) ──
            let header = egui::TopBottomPanel::top("settings_header")
                .exact_height(SETTINGS_HEADER_HEIGHT.value())
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(th.bg_sidebar().to_egui()))
                .show_inside(ui, |ui| draw_l1_tab_band(ui, &th, ui_state));
            let hr = header.response.rect;
            ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, sep);

            // ── L2 영속 사이드바 (디자인 width200 / bg-sidebar / border-right) ──
            let sections = build_l2_sections(ui_state);
            let l2_placeholder = if ui_state.active_tab == SettingsTab::Plugins {
                t("settings.filter.plugins")
            } else {
                t("settings.filter.sections")
            };
            let side = egui::SidePanel::left("settings_l2_sidebar")
                .exact_width(SETTINGS_SIDEBAR_WIDTH.value())
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(th.bg_sidebar().to_egui()))
                .show_inside(ui, |ui| {
                    draw_l2_sidebar(ui, &th, &sections, &mut ui_state.l2_filter, l2_placeholder)
                });
            let sr = side.response.rect;
            // SidePanel response.rect 은 exact_width 를 넘는 resize handle 영역을
            // 포함하므로 border 는 left + width 에 그린다 (design-parity-notes).
            ui.painter().vline(
                sr.left() + SETTINGS_SIDEBAR_WIDTH.value() - 0.5,
                sr.y_range(),
                sep,
            );
            if let Some(i) = side.inner {
                apply_l2_select(ui_state, &sections[i].select);
            }

            // ── 콘텐츠 컬럼 (스크롤 본문 + 내부 footer) ──
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(th.bg_panel().to_egui()))
                .show_inside(ui, |ui| {
                    // footer (디자인 border-top / justify-flex-end / Cancel ghost + Save primary)
                    let footer = egui::TopBottomPanel::bottom("settings_footer")
                        .resizable(false)
                        .show_separator_line(false)
                        .frame(
                            egui::Frame::NONE
                                .fill(th.bg_panel().to_egui())
                                .inner_margin(egui::Margin {
                                    left: SETTINGS_FOOTER_PAD_X,
                                    right: SETTINGS_FOOTER_PAD_X,
                                    top: th.spacing_md.value() as i8,
                                    bottom: th.spacing_md.value() as i8,
                                }),
                        )
                        .show_inside(ui, |ui| {
                            draw_settings_footer(ui, &th, settings, ui_state, &mut result);
                        });
                    let fr = footer.response.rect;
                    ui.painter().hline(fr.x_range(), fr.top() + 0.5, sep);

                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show_inside(ui, |ui| {
                            let mut draft = ui_state.draft.take().unwrap();

                            // Keybindings › Preset 은 자체 padding + 내부 스크롤을
                            // 가진 DrillDown 이라 표준 패딩/스크롤 래퍼 밖에서
                            // full-bleed 로 그린다 (디자인 settings_window.jsx
                            // `fullBleed`).
                            let full_bleed = ui_state.active_tab == SettingsTab::Keybindings
                                && matches!(
                                    ui_state.keybindings_sub_tab,
                                    KeybindingsSubTab::Preset | KeybindingsSubTab::ImportExport
                                );
                            if full_bleed {
                                draw_active_content(
                                    ui,
                                    &mut draft,
                                    ui_state,
                                    captured_double_tap,
                                    file_format,
                                    file_handler,
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .drag_to_scroll(false)
                                    .show(ui, |ui| {
                                        // 자체 레이아웃을 쓰는 full-bleed 화면을 제외한 공통 너비 제한.
                                        tasty_ui_widgets::settings_content_column(
                                            ui,
                                            th.settings_content_max_width(),
                                            |ui| {
                                                draw_active_content(
                                                    ui,
                                                    &mut draft,
                                                    ui_state,
                                                    captured_double_tap,
                                                    file_format,
                                                    file_handler,
                                                );
                                            },
                                        );
                                    });
                            }

                            // 충돌 감지 시 팝업 열기.
                            // intent-exempt: `ui_state.popups` 는 settings 윈도우 내부의
                            // 별도 PopupManager. host Intent 큐(MainViewState.popups) 와 별개 —
                            // sub-modal 내부 lifecycle 이므로 직접 호출 유지.
                            // 현재 편집 중인 배율로 크기를 먼저 구한 뒤 popups를 변경한다.
                            let conflict_size = if !ui_state.popups.is_open("keybinding_conflict") {
                                ui_state.pending_binding.as_ref().map(|pending| {
                                    let zoom = draft.appearance.ui_scale_factor();
                                    conflict_popup_size(
                                        ui,
                                        &th,
                                        conflict_message_text(pending, &settings.general),
                                        zoom,
                                    )
                                })
                            } else {
                                None
                            };
                            if let Some(size) = conflict_size {
                                if let Some(p) = ui_state.popups.get_mut("keybinding_conflict") {
                                    p.size = size;
                                }
                                ui_state.popups.open_centered_focused("keybinding_conflict");
                            }

                            // 충돌 팝업에서 수락/거부 처리
                            if ui_state.conflict_accepted {
                                ui_state.conflict_accepted = false;
                                if let Some(pending) = ui_state.pending_binding.take() {
                                    // 충돌 제거: 다른 quick-switch 슬롯이면 그 슬롯을 비우고,
                                    // 아니면 일반 콤보 필드에서 해당 바인딩 제거.
                                    if let Some(cb) = pending.conflicting_bare {
                                        clear_bare_target(&mut draft.keybindings, cb);
                                    } else {
                                        draft.keybindings.remove_binding(
                                            &pending.conflicting_field,
                                            pending.conflicting_idx,
                                        );
                                    }
                                    // 타겟 기록: bare-key 슬롯이면 raw 키를 accessor 로,
                                    // 아니면 일반 콤보 필드에 합성 콤보를 기록.
                                    if let Some(bt) = pending.bare_target {
                                        set_bare_target(
                                            &mut draft.keybindings,
                                            bt,
                                            &pending.bare_raw_key,
                                        );
                                    } else {
                                        draft.keybindings.replace_binding_at(
                                            &pending.target_field,
                                            pending.target_idx,
                                            pending.combo,
                                        );
                                    }
                                }
                                // intent-exempt: settings 윈도우 내부 sub-modal close.
                                ui_state.popups.close("keybinding_conflict");
                            }
                            if ui_state.conflict_cancelled {
                                ui_state.conflict_cancelled = false;
                                ui_state.pending_binding = None;
                                // intent-exempt: settings 윈도우 내부 sub-modal close.
                                ui_state.popups.close("keybinding_conflict");
                            }

                            ui_state.draft = Some(draft);
                        });
                });
        });

    // Escape는 가장 위에 열린 팝업 하나만 처리한다.
    let escape_owner = settings_escape_owner(&ui_state.popups);

    // Draw popups (충돌 확인 · 파일 선택)
    let mut chooser_done = false;
    let popup_result = {
        let pending = ui_state.pending_binding.clone();
        let accepted = &mut ui_state.conflict_accepted;
        let cancelled = &mut ui_state.conflict_cancelled;
        let chooser = &mut ui_state.file_chooser;
        let chooser_owns_escape = escape_owner == Some(file_chooser::FILE_CHOOSER_POPUP_ID);
        ui_state.popups.draw(
            ctx,
            &mut |id, ui| {
                if id == file_chooser::FILE_CHOOSER_POPUP_ID {
                    chooser_done |= chooser.draw(ui, &th, chooser_owns_escape);
                    return;
                }
                if id == "keybinding_conflict"
                    && let Some(pending) = &pending
                {
                    // 안내 문자열은 open 직전 크기 산정과 동일 출처를 쓴다
                    // (quick-switch 슬롯 충돌 라벨 처리 포함 — conflict_message_text).
                    ui.label(conflict_message_text(pending, &settings.general));
                    vspace(ui, th.spacing_sm);
                    ui.horizontal(|ui| {
                        if ui.button(t("button.cancel")).clicked() {
                            *cancelled = true;
                        }
                        if ui
                            .button(t("settings.keybindings.conflict_apply"))
                            .clicked()
                        {
                            *accepted = true;
                        }
                    });
                }
            },
            None,
            // settings 모달의 자체 popup 매니저 — plugin popup 과 겹치지 않는다
            // (모달이 뜬 동안 plugin popup 은 입력을 받지 않는다).
            &[],
        )
    };

    // X 버튼으로 충돌 팝업이 닫힌 경우 pending_binding 정리
    if popup_result.closed.contains(&"keybinding_conflict") {
        ui_state.pending_binding = None;
    }

    settle_file_chooser(ui_state, chooser_done, &popup_result.closed);
    apply_file_chooser_outcomes(ui_state);

    // 키보드로 충돌 팝업 수락/거부 — 파일 선택이 그 위에 떠 있으면 키는 그쪽 것이다.
    if ui_state.popups.is_open("keybinding_conflict") && escape_owner == Some("keybinding_conflict")
    {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Y) {
                ui_state.conflict_accepted = true;
            }
            if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::N) {
                ui_state.conflict_cancelled = true;
            }
        });
    }

    result
}

/// 파일 선택 한 프레임의 마무리. 뷰가 끝냈으면 popup 을 닫고, 뷰 밖 경로로 닫혔으면
/// 취소로 남긴다.
fn settle_file_chooser(
    ui_state: &mut SettingsUiState,
    chooser_done: bool,
    closed: &[&'static str],
) {
    if chooser_done {
        // intent-exempt: 설정 창 내부 sub-popup close.
        ui_state.popups.close(file_chooser::FILE_CHOOSER_POPUP_ID);
    }
    let queued = ui_state.popups.take_closed_queue();
    if closed.contains(&file_chooser::FILE_CHOOSER_POPUP_ID)
        || queued.contains(&file_chooser::FILE_CHOOSER_POPUP_ID)
    {
        ui_state.file_chooser.cancel();
    }
}

/// 설정 창 popup 중 Esc 를 받을 하나 — 열린 것 중 z 순서가 가장 위인 것.
fn settings_escape_owner(popups: &PopupManager) -> Option<&'static str> {
    ["keybinding_conflict", file_chooser::FILE_CHOOSER_POPUP_ID]
        .into_iter()
        .filter_map(|id| popups.open_geometry(id).map(|(z, _)| (z, id)))
        .max_by_key(|(z, _)| *z)
        .map(|(_, id)| id)
}

/// 닫힌 파일 선택의 결과를 그것을 연 화면 상태로 돌려준다. 경로의 쓰임은 화면마다 다르다.
fn apply_file_chooser_outcomes(ui_state: &mut SettingsUiState) {
    apply_remote_folder_outcome(ui_state);
    apply_file_chooser_file_outcomes(ui_state);
}

fn apply_remote_folder_outcome(ui_state: &mut SettingsUiState) {
    if let Some(path) = ui_state
        .file_chooser
        .take_outcome("remote_transfer.folder")
        .and_then(file_chooser::FileChooserOutcome::into_first_path)
        && let Some(draft) = ui_state.draft.as_mut()
    {
        draft.remote_transfer.dir = path.to_string_lossy().into_owned();
    }
}

fn apply_file_chooser_file_outcomes(ui_state: &mut SettingsUiState) {
    if let Some(path) = ui_state
        .file_chooser
        .take_outcome(ScriptsUiState::BROWSE_CONSUMER)
        .and_then(file_chooser::FileChooserOutcome::into_first_path)
    {
        ui_state.scripts.apply_browsed_file(&path);
    }
    if let Some(path) = ui_state
        .file_chooser
        .take_outcome(EXPORT_CONSUMER)
        .and_then(file_chooser::FileChooserOutcome::into_first_path)
    {
        match ui_state.draft.as_ref() {
            Some(draft) => ui_state.import_export.export_to(
                &path,
                &draft.keybindings,
                &ui_state.plugin_bundle,
                &ui_state.plugin_shortcuts_draft,
            ),
            None => tracing::warn!("keybinding export: no settings draft to export"),
        }
    }
    if let Some(path) = ui_state
        .file_chooser
        .take_outcome(IMPORT_CONSUMER)
        .and_then(file_chooser::FileChooserOutcome::into_first_path)
    {
        match ui_state.draft.as_ref() {
            Some(draft) => {
                ui_state
                    .import_export
                    .import_from(&path, draft, &ui_state.plugin_bundle)
            }
            None => tracing::warn!("keybinding import: no settings draft to compare against"),
        }
    }
}

// 하위 섹션 목록은 클릭 시 적용할 선택값을 함께 보관한다.

/// L2 사이드바 한 row 가 클릭됐을 때 적용할 sub-tab 선택값.
enum L2Select {
    General(GeneralSubTab),
    Terminal(TerminalSubTab),
    Appearance(AppearanceSubTab),
    Keybindings(KeybindingsSubTab),
    FileHandler(FileHandlerSubTab),
    Misc(MiscSubTab),
    Plugin(PluginSubTab),
}

/// L2 사이드바 한 row 의 표시 모델.
struct L2Section {
    label: String,
    /// plugin-contributed 섹션이면 true → 라벨 앞에 accent-agent dot.
    is_plugin: bool,
    /// 위에 구분선을 긋는다 — 성격이 다른 꼬리 항목(디자인 `KB_L2_SEPARATED`). 필터 중에는
    /// 숨는다(걸러진 목록에서 구분선은 가를 대상이 없다).
    separated: bool,
    selected: bool,
    select: L2Select,
}

/// 현재 활성 L1 탭의 L2 섹션 목록을 만든다. Appearance / Plugins 는 plugin 이
/// contribute 한 page 를 동적으로 합성하고, 사라진 page 를 가리키던 sub-tab 은
/// 여기서 리셋한다.
fn build_l2_sections(ui_state: &mut SettingsUiState) -> Vec<L2Section> {
    match ui_state.active_tab {
        SettingsTab::General => {
            // Display(Alt/Option/Shift 표시 스타일)는 macOS 전용 — 아이콘 글리프
            // 개념이 없는 Windows/Linux 에서는 push 하지 않는다.
            let cur = ui_state.general_sub_tab;
            // 이유: `Display` 를 push 하는 분기가 macOS 에만 있어 다른 OS 에선 `mut` 가 남는다(위).
            #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
            let mut items = vec![
                (GeneralSubTab::General, t("settings.tab.general")),
                (
                    GeneralSubTab::Notifications,
                    t("settings.tab.notifications"),
                ),
                (
                    GeneralSubTab::Accessibility,
                    t("settings.misc.subtab.accessibility"),
                ),
                (GeneralSubTab::Overlay, t("settings.tab.overlay")),
                (
                    GeneralSubTab::RemoteTransfer,
                    t("settings.tab.remote_transfer"),
                ),
                (GeneralSubTab::RemoteAttach, t("settings.tab.remote_attach")),
            ];
            #[cfg(target_os = "macos")]
            items.push((GeneralSubTab::Display, t("settings.tab.display")));
            #[cfg(target_os = "macos")]
            items.push((
                GeneralSubTab::MacosPermissions,
                t("settings.tab.macos_permissions"),
            ));
            items
                .into_iter()
                .map(|(tab, label)| L2Section {
                    label: label.to_string(),
                    is_plugin: false,
                    separated: false,
                    selected: cur == tab,
                    select: L2Select::General(tab),
                })
                .collect()
        }
        SettingsTab::Terminal => {
            let cur = ui_state.terminal_sub_tab;
            [
                (TerminalSubTab::General, t("settings.tab.general")),
                (TerminalSubTab::Input, t("settings.terminal.input")),
                (
                    TerminalSubTab::MouseCapture,
                    t("settings.terminal.mouse_capture"),
                ),
                (TerminalSubTab::Tui, t("settings.terminal.tui")),
                (
                    TerminalSubTab::Performance,
                    t("settings.misc.subtab.performance"),
                ),
            ]
            .into_iter()
            .map(|(tab, label)| L2Section {
                label: label.to_string(),
                is_plugin: false,
                separated: false,
                selected: cur == tab,
                select: L2Select::Terminal(tab),
            })
            .collect()
        }
        SettingsTab::Appearance => build_appearance_sections(ui_state),
        SettingsTab::Keybindings => {
            let cur = ui_state.keybindings_sub_tab;
            [
                (
                    KeybindingsSubTab::General,
                    t("settings.keybindings.subtab.general"),
                ),
                (
                    KeybindingsSubTab::Workspace,
                    t("settings.keybindings.subtab.workspace"),
                ),
                (
                    KeybindingsSubTab::Pane,
                    t("settings.keybindings.subtab.pane"),
                ),
                (KeybindingsSubTab::Tab, t("settings.keybindings.subtab.tab")),
                (
                    KeybindingsSubTab::Surface,
                    t("settings.keybindings.subtab.surface"),
                ),
                (
                    KeybindingsSubTab::Clipboard,
                    t("settings.keybindings.subtab.clipboard"),
                ),
                (
                    KeybindingsSubTab::Zoom,
                    t("settings.keybindings.subtab.zoom"),
                ),
                (
                    KeybindingsSubTab::Explorer,
                    t("settings.keybindings.subtab.explorer"),
                ),
                (
                    KeybindingsSubTab::Scripts,
                    t("settings.keybindings.subtab.scripts"),
                ),
                (
                    KeybindingsSubTab::Preset,
                    t("settings.keybindings.subtab.preset"),
                ),
                (
                    KeybindingsSubTab::Plugins,
                    t("settings.keybindings.subtab.plugins"),
                ),
                (
                    KeybindingsSubTab::ImportExport,
                    t("settings.keybindings.subtab.import_export"),
                ),
            ]
            .into_iter()
            .map(|(tab, label)| L2Section {
                label: label.to_string(),
                is_plugin: false,
                separated: tab == KeybindingsSubTab::ImportExport,
                selected: cur == tab,
                select: L2Select::Keybindings(tab),
            })
            .collect()
        }
        SettingsTab::FileHandler => {
            let cur = ui_state.file_handler_sub_tab;
            [
                (
                    FileHandlerSubTab::ExtensionMapping,
                    t("settings.file_handler.sub.extension_mapping"),
                ),
                (
                    FileHandlerSubTab::Detectors,
                    t("settings.file_handler.sub.detectors"),
                ),
                (
                    FileHandlerSubTab::Handlers,
                    t("settings.file_handler.sub.handlers"),
                ),
                (
                    FileHandlerSubTab::HookHandlers,
                    t("settings.file_handler.sub.hook_handlers"),
                ),
            ]
            .into_iter()
            .map(|(tab, label)| L2Section {
                label: label.to_string(),
                is_plugin: false,
                separated: false,
                selected: cur == tab,
                select: L2Select::FileHandler(tab),
            })
            .collect()
        }
        SettingsTab::Misc => {
            // Scripts 가 최상단.
            let cur = ui_state.misc_sub_tab;
            let mut sections = vec![L2Section {
                label: t("settings.misc.scripts").to_string(),
                is_plugin: false,
                separated: false,
                selected: cur == MiscSubTab::Scripts,
                select: L2Select::Misc(MiscSubTab::Scripts),
            }];
            sections.push(L2Section {
                label: t("settings.misc.subtab.task_pipeline").to_string(),
                is_plugin: false,
                separated: false,
                selected: cur == MiscSubTab::TaskPipeline,
                select: L2Select::Misc(MiscSubTab::TaskPipeline),
            });
            sections.push(L2Section {
                label: t("settings.misc.subtab.anomaly_detection").to_string(),
                is_plugin: false,
                separated: false,
                selected: cur == MiscSubTab::AnomalyDetection,
                select: L2Select::Misc(MiscSubTab::AnomalyDetection),
            });
            sections
        }
        SettingsTab::Plugins => build_plugin_sections(ui_state),
    }
}

/// 고정 Appearance 섹션과 plugin이 제공한 페이지를 합친다.
fn build_appearance_sections(ui_state: &mut SettingsUiState) -> Vec<L2Section> {
    let mut items: Vec<(AppearanceSubTab, String, bool)> = vec![
        (
            AppearanceSubTab::Theme,
            t("settings.appearance.subtab.theme").to_string(),
            false,
        ),
        (
            AppearanceSubTab::Colors,
            t("settings.appearance.subtab.colors").to_string(),
            false,
        ),
        (
            AppearanceSubTab::General,
            t("settings.appearance.subtab.general").to_string(),
            false,
        ),
        (
            AppearanceSubTab::Display,
            t("settings.appearance.subtab.display").to_string(),
            false,
        ),
        (
            AppearanceSubTab::Tasty,
            t("settings.appearance.subtab.tasty").to_string(),
            false,
        ),
        (
            AppearanceSubTab::Terminal,
            t("settings.appearance.subtab.terminal").to_string(),
            false,
        ),
        (
            AppearanceSubTab::Explorer,
            t("settings.appearance.subtab.explorer").to_string(),
            false,
        ),
    ];
    for entry in ui_state
        .settings_pages
        .iter()
        .filter(|e| e.page.category == SettingsCategory::Appearance)
    {
        items.push((
            AppearanceSubTab::Plugin {
                plugin_id: entry.plugin_id.clone(),
                page_id: entry.page.id.clone(),
            },
            t(&entry.page.title_key).to_string(),
            true,
        ));
    }
    // 활성 plugin page 가 사라졌으면 Theme 로 fallback.
    let needs_reset = if let AppearanceSubTab::Plugin {
        plugin_id: ap,
        page_id: pg,
    } = &ui_state.appearance_sub_tab
    {
        !items.iter().any(|(tab, _, _)| {
            matches!(
                tab,
                AppearanceSubTab::Plugin { plugin_id, page_id }
                    if plugin_id == ap && page_id == pg
            )
        })
    } else {
        false
    };
    if needs_reset {
        ui_state.appearance_sub_tab = AppearanceSubTab::Theme;
    }
    let cur = ui_state.appearance_sub_tab.clone();
    items
        .into_iter()
        .map(|(tab, label, is_plugin)| L2Section {
            selected: tab == cur,
            label,
            is_plugin,
            separated: false,
            select: L2Select::Appearance(tab),
        })
        .collect()
}

/// Plugins L2: Plugin category page 만으로 구성. 미선택 상태에서 page 가 있으면
/// 첫 page 를 자동 선택한다 (디자인: 진입 시 L2[t][0] 활성).
fn build_plugin_sections(ui_state: &mut SettingsUiState) -> Vec<L2Section> {
    let pages: Vec<(PluginSubTab, String)> = ui_state
        .settings_pages
        .iter()
        .filter(|e| e.page.category == SettingsCategory::Plugin)
        .map(|e| {
            (
                PluginSubTab::Plugin {
                    plugin_id: e.plugin_id.clone(),
                    page_id: e.page.id.clone(),
                },
                t(&e.page.title_key).to_string(),
            )
        })
        .collect();

    let needs_reset = if let Some(PluginSubTab::Plugin {
        plugin_id: ap,
        page_id: pg,
    }) = ui_state.plugin_sub_tab.as_ref()
    {
        !pages.iter().any(|(tab, _)| {
            matches!(
                tab,
                PluginSubTab::Plugin { plugin_id, page_id }
                    if plugin_id == ap && page_id == pg
            )
        })
    } else {
        false
    };
    if needs_reset {
        ui_state.plugin_sub_tab = None;
    }
    if ui_state.plugin_sub_tab.is_none()
        && let Some((first, _)) = pages.first()
    {
        ui_state.plugin_sub_tab = Some(first.clone());
    }

    let cur = ui_state.plugin_sub_tab.clone();
    pages
        .into_iter()
        .map(|(tab, label)| L2Section {
            selected: cur.as_ref() == Some(&tab),
            label,
            is_plugin: true,
            separated: false,
            select: L2Select::Plugin(tab),
        })
        .collect()
}

/// L2 row 클릭 → 해당 L1 의 sub-tab 상태에 반영.
fn apply_l2_select(ui_state: &mut SettingsUiState, select: &L2Select) {
    match select {
        L2Select::General(v) => {
            ui_state.general_sub_tab = *v;
            // 권한 화면은 상태를 그릴 때마다 측정하지 않고 보관된 스냅샷을 읽는다.
            // 화면에 들어오는 것이 그 갱신 시점 중 하나다. macOS가 아니면 아무 일도
            // 하지 않는다.
            if matches!(v, GeneralSubTab::MacosPermissions) {
                crate::macos_permissions::refresh_permission_snapshot();
            }
        }
        L2Select::Terminal(v) => ui_state.terminal_sub_tab = *v,
        L2Select::Appearance(v) => ui_state.appearance_sub_tab = v.clone(),
        L2Select::Keybindings(v) => {
            ui_state.keybindings_sub_tab = *v;
            // 다른 sub-tab 으로 이동 시 진행 중이던 녹화를 취소.
            ui_state.recording_field = None;
        }
        L2Select::FileHandler(v) => ui_state.file_handler_sub_tab = *v,
        L2Select::Misc(v) => ui_state.misc_sub_tab = *v,
        L2Select::Plugin(v) => ui_state.plugin_sub_tab = Some(v.clone()),
    }
}

// ── L1 헤더 밴드 ──────────────────────────────────────────────────────────

/// 설정 제목과 상위 탭을 그린다. 닫기는 하단 Cancel 또는 OS 닫기 버튼을 사용한다.
fn draw_l1_tab_band(ui: &mut egui::Ui, th: &Theme, ui_state: &mut SettingsUiState) {
    let tabs = [
        (SettingsTab::General, t("settings.tab.general")),
        (SettingsTab::Terminal, t("settings.tab.terminal")),
        (SettingsTab::Appearance, t("settings.tab.appearance")),
        (SettingsTab::Keybindings, t("settings.tab.keybindings")),
        (SettingsTab::FileHandler, t("settings.tab.file_handler")),
        (SettingsTab::Misc, t("settings.tab.misc")),
        (SettingsTab::Plugins, t("settings.tab.plugin")),
    ];
    let prev = ui_state.active_tab;
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: th.spacing_md.value() as i8,
            right: th.spacing_md.value() as i8,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            // 콘텐츠 높이만큼 줄지 않도록 헤더 영역을 잡고 세로 중앙에 배치한다.
            let band_h = ui.available_height();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), band_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    // gap 은 항목마다 명시적으로 add_space — 탭 사이만 2px, 타이틀/구분선
                    // 주변은 디자인 margin(space-sm / size-14) 을 따로 적용한다.
                    ui.spacing_mut().item_spacing.x = 0.0;

                    // 좌측 bold "Settings" 타이틀 (jsx:467, fontSize14 / weight700).
                    ui.label(
                        egui::RichText::new(t("settings.window.title"))
                            .strong()
                            .size(th.font_size_max.value())
                            .color(th.text_primary().to_egui()),
                    );
                    // 타이틀 ↔ 탭 세로 구분선 (jsx:468, width1 h20, margin 좌 space-sm / 우 size-14).
                    ui.add_space(th.spacing_sm.value());
                    let (vrect, _) = ui.allocate_exact_size(
                        egui::vec2(
                            th.border_width.value(),
                            SETTINGS_TITLE_DIVIDER_HEIGHT.value(),
                        ),
                        egui::Sense::hover(),
                    );
                    ui.painter().vline(
                        vrect.center().x,
                        vrect.y_range(),
                        egui::Stroke::new(th.border_width.value(), th.border_strong().to_egui()),
                    );
                    ui.add_space(SETTINGS_TITLE_DIVIDER_MARGIN_R.value());

                    // L1 탭들 (gap 2).
                    for (i, (tab, label)) in tabs.into_iter().enumerate() {
                        if i > 0 {
                            ui.add_space(L1_TAB_GAP.value());
                        }
                        if l1_tab_button(ui, th, label, ui_state.active_tab == tab) {
                            ui_state.active_tab = tab;
                        }
                    }
                },
            );
        });
    // L1 전환 시 L2 필터 초기화 (디자인: pickL1 → setFilter("")).
    if ui_state.active_tab != prev {
        ui_state.l2_filter.clear();
    }
}

/// 헤더 밴드의 한 L1 탭 버튼. 밴드 높이를 가득 채워 active underline 이
/// border-bottom 위치에 정렬되게 한다.
fn l1_tab_button(ui: &mut egui::Ui, th: &Theme, label: &str, active: bool) -> bool {
    let font = egui::FontId::proportional(th.font_size_body.value());
    let pad_x = th.spacing_md.value();
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER);
    let galley_size = galley.size();
    let w = galley_size.x + pad_x * 2.0;
    let h = ui.available_height();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if active {
        th.text_primary().to_egui()
    } else if resp.hovered() {
        th.text_secondary().to_egui()
    } else {
        th.text_muted().to_egui()
    };
    let pos = egui::pos2(
        rect.center().x - galley_size.x * 0.5,
        rect.center().y - galley_size.y * 0.5,
    );
    ui.painter().galley(pos, galley, color);
    if active {
        let thickness = SETTINGS_TAB_UNDERLINE.value();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - thickness * 0.5,
            egui::Stroke::new(thickness, th.accent_primary().to_egui()),
        );
    }
    resp.clicked()
}

// ── L2 사이드바 뷰 ────────────────────────────────────────────────────────

/// 영속 L2 사이드바: 상단 필터 Input(+ border-bottom) + 스크롤 섹션 리스트.
/// 클릭된 섹션 인덱스를 반환한다.
fn draw_l2_sidebar(
    ui: &mut egui::Ui,
    th: &Theme,
    sections: &[L2Section],
    filter: &mut String,
    placeholder: &str,
) -> Option<usize> {
    let mut clicked = None;
    let sep = egui::Stroke::new(th.border_width.value(), th.border_strong().to_egui());
    let pad = th.spacing_sm.value() as i8;

    // 필터 입력 — padding space-sm, 하단 border-bottom separator.
    let filter_resp = egui::Frame::NONE
        .inner_margin(egui::Margin::same(pad))
        .show(ui, |ui| {
            // 디자인 settings_window.jsx:484 — leading 돋보기 아이콘(`icon={ic.search}`).
            tasty_ui_widgets::Input::new()
                .placeholder(placeholder)
                .icon(&|ui, rect, c| {
                    crate::adapters::ui::icons::SEARCH
                        .image(rect.width(), c)
                        .paint_at(ui, rect)
                })
                .show(ui, th, filter);
        });
    let frect = filter_resp.response.rect;
    ui.painter()
        .hline(frect.x_range(), frect.bottom() - 0.5, sep);

    // 섹션 리스트 — 스크롤, padding space-sm.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(pad))
                .show(ui, |ui| {
                    let filter_lc = filter.to_lowercase();
                    let mut any = false;
                    for (i, s) in sections.iter().enumerate() {
                        if !filter_lc.is_empty() && !s.label.to_lowercase().contains(&filter_lc) {
                            continue;
                        }
                        any = true;
                        if s.separated && filter_lc.is_empty() {
                            l2_separator(ui, th);
                        }
                        if sidebar_row(ui, th, &s.label, s.is_plugin, s.selected) {
                            clicked = Some(i);
                        }
                    }
                    // 필터 결과가 없을 때만 안내한다. 섹션 자체가 없으면 본문이 안내한다.
                    if !any && !filter_lc.is_empty() {
                        ui.label(
                            egui::RichText::new(t("settings.filter.no_matches"))
                                .size(th.font_size_caption.value())
                                .color(th.text_muted().to_egui()),
                        );
                    }
                });
        });
    clicked
}

/// L2 구분선 — 디자인 `height: border-width · background: separator · margin: space-sm`.
fn l2_separator(ui: &mut egui::Ui, th: &Theme) {
    let m = th.spacing_sm.value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, th.border_width.value() + m * 2.0),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        (rect.left() + m)..=(rect.right() - m),
        rect.center().y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// L2 사이드바 한 row. selected = surface-active 배경 + radius-sm, plugin row 는
/// 라벨 앞에 accent-agent dot.
fn sidebar_row(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    is_plugin: bool,
    selected: bool,
) -> bool {
    let pad_x = th.spacing_sm.value();
    let pad_y = th.spacing_xs.value();
    let font = egui::FontId::proportional(th.font_size_body.value());
    let row_h = th.font_size_body.value() + pad_y * 2.0;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, row_h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let radius = th.corner_radius_sm.value();
    if selected {
        ui.painter()
            .rect_filled(rect, radius, th.surface_active().to_egui());
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.overlay_hover().to_egui_premultiplied());
    }
    let mut x = rect.left() + pad_x;
    if is_plugin {
        let d = th.status_dot_size.value();
        ui.painter().circle_filled(
            egui::pos2(x + d * 0.5, rect.center().y),
            d * 0.5,
            if selected {
                th.accent_agent_on_active()
            } else {
                th.accent_agent()
            }
            .to_egui(),
        );
        x += d + th.spacing_sm.value();
    }
    let color = if selected {
        th.text_on_active().to_egui()
    } else {
        th.text_muted().to_egui()
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let gy = rect.center().y - galley.size().y * 0.5;
    ui.painter().galley(egui::pos2(x, gy), galley, color);
    resp.clicked()
}

// ── 푸터 ──────────────────────────────────────────────────────────────────

/// 콘텐츠 아래의 Cancel과 Save 버튼.
fn draw_settings_footer(
    ui: &mut egui::Ui,
    th: &Theme,
    settings: &mut Settings,
    ui_state: &mut SettingsUiState,
    result: &mut Option<bool>,
) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        // RTL: 먼저 추가한 위젯이 가장 우측. 디자인 [Cancel] [Save] (Save 우측).
        if Button::new(t("button.save"))
            .variant(ButtonVariant::Primary)
            .show(ui, th)
            .clicked()
        {
            commit_settings_save(settings, ui_state, result);
        }
        if Button::new(t("button.cancel"))
            .variant(ButtonVariant::Ghost)
            .show(ui, th)
            .clicked()
        {
            discard_settings_draft(ui_state, result);
        }
    });
}

/// 설정 초안을 settings 에 반영한다. 핸들러·훅 편집과 전역 Theme 적용은 창을 닫은 뒤
/// `take_execution_edits` 로 회수해 처리한다. 렌더 중에는 THEME 읽기 락을 잡고 있어
/// 여기서 쓰기 락을 잡으면 교착된다.
fn commit_settings_save(
    settings: &mut Settings,
    ui_state: &mut SettingsUiState,
    result: &mut Option<bool>,
) {
    if let Some(draft) = &ui_state.draft {
        *settings = draft.clone();
    }
    *result = Some(true);
}

impl SettingsUiState {
    pub(crate) fn take_execution_edits(
        &mut self,
        clear_scrollback: bool,
    ) -> crate::app::settings_edit::SettingsEdits {
        use crate::app::settings_edit::{RegistryEdit, SettingsEdits};
        let mut registry = Vec::new();
        self.extension_priority_pending.clear();
        if let Some(draft) = self.extension_priority_draft.take() {
            registry.extend(
                draft
                    .into_iter()
                    .map(|(extension, order)| RegistryEdit::Extension { extension, order }),
            );
        }
        registry.extend(std::mem::take(&mut self.fh_edit_draft).into_edits());
        registry.extend(std::mem::take(&mut self.hook_edit_draft).into_edits());
        SettingsEdits {
            registry,
            clear_scrollback,
        }
    }
}

/// Cancel 클릭 시 draft 폐기 — 다음 오픈 시 디스크에서 다시 로드되도록 모든
/// 탭의 편집 draft(extension-priority/file-handler/hook-handler/plugin 단축키)를 지운다.
fn discard_settings_draft(ui_state: &mut SettingsUiState, result: &mut Option<bool>) {
    // plugin 단축키 초안도 취소한다. App은 Save로 닫았을 때만 초안을 적용한다.
    ui_state.plugin_shortcuts_draft.clear();
    ui_state.extension_priority_draft = None;
    ui_state.extension_priority_pending.clear();
    ui_state.fh_edit_draft = file_handler_tab::FileHandlerEditDraft::default();
    ui_state.hook_edit_draft = file_handler_tab::HookHandlerEditDraft::default();
    *result = Some(false);
}

// ── 콘텐츠 디스패치 ───────────────────────────────────────────────────────

/// 활성 L1/L2 에 해당하는 콘텐츠를 그린다. L2 사이드바는 셸이 소유하므로 각 탭
/// draw 는 content-only.
fn draw_active_content(
    ui: &mut egui::Ui,
    draft: &mut Settings,
    ui_state: &mut SettingsUiState,
    captured_double_tap: &mut Option<String>,
    file_format: &FileFormatRegistry,
    file_handler: &FileHandlerRegistry,
) {
    match ui_state.active_tab {
        SettingsTab::General => match ui_state.general_sub_tab {
            GeneralSubTab::General => {
                draw_general_tab(ui, draft, ui_state.languages.as_deref().unwrap_or(&[]))
            }
            GeneralSubTab::Notifications => draw_notifications_tab(ui, draft),
            GeneralSubTab::Accessibility => draw_accessibility_tab(ui, draft),
            GeneralSubTab::Overlay => draw_overlay_tab(ui, draft),
            GeneralSubTab::RemoteTransfer => {
                if draw_remote_transfer_tab(ui, draft) {
                    ui_state.open_file_chooser_at(
                        "remote_transfer.folder",
                        file_chooser::FileChooserMode::Folder,
                        Vec::new(),
                        None,
                        Some(file_chooser::start_directory(&draft.remote_transfer.dir)),
                    );
                }
            }
            GeneralSubTab::RemoteAttach => draw_remote_attach_tab(ui, draft),
            #[cfg(target_os = "macos")]
            GeneralSubTab::Display => draw_general_display_tab(ui, draft),
            #[cfg(target_os = "macos")]
            GeneralSubTab::MacosPermissions => draw_macos_permissions_tab(ui),
            #[cfg(not(target_os = "macos"))]
            GeneralSubTab::Display | GeneralSubTab::MacosPermissions => {
                let th = crate::theme::theme();
                ui.vertical_centered(|ui| {
                    vspace(ui, th.spacing_xl);
                    ui.label(
                        egui::RichText::new(t("settings.misc.empty"))
                            .size(th.font_size_caption.value())
                            .color(th.text_muted().to_egui()),
                    );
                });
            }
        },
        SettingsTab::Terminal => match ui_state.terminal_sub_tab {
            TerminalSubTab::General => draw_terminal_tab(ui, draft),
            TerminalSubTab::Input => draw_terminal_input_tab(ui, draft),
            TerminalSubTab::MouseCapture => draw_terminal_mouse_capture_tab(ui, draft),
            TerminalSubTab::Tui => draw_terminal_tui_tab(ui, draft),
            TerminalSubTab::Performance => draw_performance_tab(ui, draft),
        },
        SettingsTab::Appearance => draw_appearance_tab(
            ui,
            draft,
            &ui_state.appearance_sub_tab,
            &mut ui_state.font_families,
            &mut ui_state.preview_fonts,
            &ui_state.settings_pages,
        ),
        SettingsTab::Keybindings => {
            draw_keybindings_tab(
                ui,
                draft,
                &mut ui_state.recording_field,
                ui_state.keybindings_sub_tab,
                &mut ui_state.selected_preset,
                &mut ui_state.pending_binding,
                captured_double_tap,
                &mut ui_state.captured_winit_combo,
                &ui_state.plugin_shortcuts,
                &mut ui_state.plugin_shortcuts_selected,
                &mut ui_state.plugin_shortcuts_draft,
                &mut ui_state.import_export,
                &ui_state.plugin_bundle,
            );
            match ui_state.import_export.take_request() {
                Some(ImportExportRequest::Export) => {
                    let default_name = format!(
                        "tasty-keybindings-{}.toml",
                        chrono::Local::now().format("%Y-%m-%d")
                    );
                    ui_state.open_file_chooser(
                        EXPORT_CONSUMER,
                        file_chooser::FileChooserMode::Save { default_name },
                        vec!["toml".to_string()],
                        Some(t("settings.keybindings.ie_export_chooser_title")),
                    );
                }
                Some(ImportExportRequest::Import) => ui_state.open_file_chooser(
                    IMPORT_CONSUMER,
                    file_chooser::FileChooserMode::Open,
                    vec!["toml".to_string()],
                    Some(t("settings.keybindings.ie_import_chooser_title")),
                ),
                None => {}
            }
        }
        SettingsTab::FileHandler => draw_file_handler_tab(
            ui,
            ui_state.file_handler_sub_tab,
            &mut ui_state.extension_priority_draft,
            &mut ui_state.extension_priority_pending,
            &mut ui_state.extension_priority_new_input,
            &mut ui_state.fh_edit_draft,
            &mut ui_state.hook_edit_draft,
            &draft.keybindings,
            file_format,
            file_handler,
        ),
        SettingsTab::Misc => draw_misc_content(ui, draft, ui_state),
        SettingsTab::Plugins => draw_plugin_tab(
            ui,
            draft,
            ui_state.plugin_sub_tab.as_ref(),
            &mut ui_state.font_families,
            &mut ui_state.preview_fonts,
            &ui_state.settings_pages,
        ),
    }
}

/// Misc 콘텐츠. Scripts = Lua 스크립트 관리.
fn draw_misc_content(ui: &mut egui::Ui, draft: &mut Settings, ui_state: &mut SettingsUiState) {
    match ui_state.misc_sub_tab {
        MiscSubTab::Scripts => {
            // 관리 창은 바인딩을 편집하지 않는다(Keybindings › Scripts 소유) — bind 버튼은
            // Keybindings › Scripts 로 진입만 한다. 진입 요청은 intent 로 받아 여기서 적용.
            let navigate = draw_scripts_subtab(ui, draft, &mut ui_state.scripts);
            if ui_state.scripts.take_browse_request() {
                ui_state.open_file_chooser(
                    ScriptsUiState::BROWSE_CONSUMER,
                    file_chooser::FileChooserMode::Open,
                    vec!["lua".to_string()],
                    None,
                );
            }
            if navigate {
                ui_state.active_tab = SettingsTab::Keybindings;
                ui_state.keybindings_sub_tab = KeybindingsSubTab::Scripts;
                ui_state.l2_filter.clear();
            }
        }
        MiscSubTab::TaskPipeline => draw_task_pipeline_tab(ui, draft),
        MiscSubTab::AnomalyDetection => draw_anomaly_detection_tab(ui, draft),
    }
}

#[cfg(test)]
mod file_chooser_popup_tests;

#[cfg(all(test, debug_assertions))]
mod tab_key_tests {
    use super::*;

    /// handler와 기존 file_handler 별칭이 같은 탭을 선택한다.
    #[test]
    fn handler_tab_key_aliases() {
        let mut st = SettingsUiState::new();
        for key in ["handler", "file_handler", "file-handler", "filehandler"] {
            st.active_tab = SettingsTab::General;
            assert!(st.select_tab_by_key(key), "key '{key}' should resolve");
            assert_eq!(st.active_tab, SettingsTab::FileHandler);
        }
        assert!(!st.select_tab_by_key("nonexistent"));
    }

    /// Hook Handlers L2 키 매핑 + 미지정 키 거부.
    #[test]
    fn hook_handlers_section_key() {
        let mut st = SettingsUiState::new();
        assert!(st.select_tab_by_key("handler"));
        for key in ["hook_handlers", "hook-handlers", "hookhandlers"] {
            st.file_handler_sub_tab = FileHandlerSubTab::ExtensionMapping;
            assert!(st.select_section_by_key(key), "key '{key}' should resolve");
            assert_eq!(st.file_handler_sub_tab, FileHandlerSubTab::HookHandlers);
        }
        assert!(!st.select_section_by_key("unknown-section"));
    }

    /// 가져오기/내보내기 서브탭 deep-link 키 — 밑줄·하이픈 둘 다.
    #[test]
    fn import_export_section_key() {
        let mut st = SettingsUiState::new();
        assert!(st.select_tab_by_key("keybindings"));
        for key in ["import_export", "import-export"] {
            st.keybindings_sub_tab = KeybindingsSubTab::General;
            assert!(st.select_section_by_key(key), "key '{key}' should resolve");
            assert_eq!(st.keybindings_sub_tab, KeybindingsSubTab::ImportExport);
        }
    }

    /// L2 목록의 마지막이 가져오기/내보내기이고, 그 행만 위에 구분선을 갖는다.
    #[test]
    fn import_export_is_the_separated_last_keybindings_section() {
        let mut st = SettingsUiState::new();
        st.active_tab = SettingsTab::Keybindings;
        let sections = build_l2_sections(&mut st);
        let last = sections.last().expect("sections");
        assert!(matches!(
            last.select,
            L2Select::Keybindings(KeybindingsSubTab::ImportExport)
        ));
        assert_eq!(sections.iter().filter(|s| s.separated).count(), 1);
        assert!(last.separated);
    }

    /// Remote attach 는 Remote transfer 바로 다음 L2 이며 debug 키로 고를 수 있다.
    #[test]
    fn remote_attach_follows_remote_transfer_in_general_sections() {
        let mut st = SettingsUiState::new();
        st.active_tab = SettingsTab::General;
        let sections = build_l2_sections(&mut st);
        let position = |tab: GeneralSubTab| {
            sections
                .iter()
                .position(|s| matches!(s.select, L2Select::General(t) if t == tab))
                .expect("section")
        };
        assert_eq!(
            position(GeneralSubTab::RemoteAttach),
            position(GeneralSubTab::RemoteTransfer) + 1
        );
        for key in ["remote_attach", "remote-attach"] {
            st.general_sub_tab = GeneralSubTab::General;
            assert!(st.select_section_by_key(key), "key '{key}' should resolve");
            assert_eq!(st.general_sub_tab, GeneralSubTab::RemoteAttach);
        }
    }

    /// Cancel 은 plugin 단축키 draft 도 버린다 — 모달 close 가 그 draft 를 회수해 적용하므로.
    #[test]
    fn cancel_discards_the_plugin_shortcut_draft() {
        let mut st = SettingsUiState::new();
        st.plugin_shortcuts_draft
            .insert(("p".into(), "c".into()), None);
        let mut result = None;
        discard_settings_draft(&mut st, &mut result);
        assert!(st.plugin_shortcuts_draft.is_empty());
        assert_eq!(result, Some(false));
    }

    /// 안내문 높이에 맞춰 팝업이 커지고 버튼 공간을 확보하는지 확인한다.
    #[test]
    fn conflict_popup_dims_fits_content() {
        use crate::adapters::ui::popup::title_bar_height;
        let th = crate::theme::theme();
        // 라벨 galley 높이를 3줄분·4줄분으로 흉내 낸다(줄높이는 폰트 상한 이하).
        let line = th.font_size_body.value() * 1.4;
        let three = conflict_popup_dims(&th, line * 3.0, 1.0);
        let four = conflict_popup_dims(&th, line * 4.0, 1.0);
        // 한 줄 더 wrap 되면 팝업이 그만큼 커진다(≈ 한 줄 높이).
        assert!(four.y > three.y, "four.y={} three.y={}", four.y, three.y);
        assert!(
            (four.y - three.y - line).abs() < 1.0,
            "height delta should equal one line: four={} three={} line={}",
            four.y,
            three.y,
            line
        );
        // 라벨이 0 높이여도 최소한 타이틀바 + 버튼행 높이 이상을 확보한다(버튼 clip 방지).
        let floor = title_bar_height() + th.item_height_interactive;
        assert!(
            conflict_popup_dims(&th, 0.0, 1.0).y >= floor.value(),
            "empty-label height must clear title+button floor"
        );
        // 폭은 zoom 을 반영한다(고정 폭 비대칭 제거).
        let base = conflict_popup_dims(&th, line * 3.0, 1.0);
        let zoomed = conflict_popup_dims(&th, line * 3.0, 1.2);
        assert!(zoomed.x > base.x, "zoomed.x={} base.x={}", zoomed.x, base.x);
    }
}
