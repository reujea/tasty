mod accessors;
#[cfg(feature = "gui")]
pub(crate) mod attach_pending;
#[cfg(feature = "gui")]
pub(crate) mod branch;
#[cfg(feature = "gui")]
mod detect;
#[cfg(feature = "gui")]
mod dialogs;
#[cfg(feature = "gui")]
mod events;
#[cfg(feature = "gui")]
pub(crate) mod explorer_clipboard_slot;
#[cfg(feature = "gui")]
pub(crate) mod explorer_history;
#[cfg(feature = "gui")]
pub(crate) mod explorer_menu;
#[cfg(feature = "gui")]
mod explorer_ops;
#[cfg(feature = "gui")]
pub(crate) mod explorer_os_clipboard;
#[cfg(feature = "gui")]
mod explorer_path;
#[cfg(feature = "gui")]
mod focus;
#[cfg(all(test, feature = "gui"))]
mod fullscreen_stage_tests;
#[cfg(feature = "gui")]
mod handler_recent;
#[cfg(any(feature = "gui", test))]
mod layout;
#[cfg(feature = "gui")]
pub(crate) mod layout_preview;
pub mod mouse;
pub(crate) mod navigation;
pub(crate) mod pane;
#[cfg(all(test, feature = "gui"))]
mod popup_close_tests;
#[cfg(all(test, feature = "gui"))]
mod popup_ownership_tests;
#[cfg(feature = "gui")]
pub(crate) mod preset_catalog;
#[cfg(feature = "gui")]
mod shell_integration_hint;
#[cfg(any(feature = "gui", test))]
pub(crate) mod surface_focus;
mod tab;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(feature = "gui")]
pub(crate) mod webview_edges;
#[cfg(feature = "gui")]
pub(crate) mod webview_overlay;
mod workspace;

// 팔레트 매칭은 헤드리스 시험에서도 검사한다.
#[cfg(any(feature = "gui", test))]
pub mod command_palette;
#[cfg(feature = "gui")]
pub mod search;
#[cfg(feature = "gui")]
pub use tasty_selection as selection;

pub use crate::core::host_event::PendingHostEvent;
#[cfg(feature = "gui")]
pub use crate::core::host_event::PendingSurfaceClosed;
#[cfg(feature = "gui")]
pub(crate) use dialogs::RenameWaiting;
#[cfg(feature = "gui")]
pub use dialogs::{
    DialogState, FileHandlerPickerData, FileHandlerPickerResult, PendingNativeMenu,
    PendingScriptConfirm, PickerHandlerSummary, RenameTarget, TabDragState, TerminalLinkMenu,
    WsDragState,
};
#[cfg(feature = "gui")]
pub(crate) use dialogs::{
    FilePickerData, FilePickerLocation, FilePickerOutcome, FilePickerRequestId,
    FilePickerRequester, FilePickerResult, FpLoadState, NewTabPickerRequest, OpenFilePickerRequest,
};
#[cfg(feature = "gui")]
pub use events::FocusedSurfaceType;
#[cfg(any(feature = "gui", debug_assertions))]
pub use workspace::WorkspaceCloseOrigin;

#[cfg(feature = "gui")]
use crate::core::CoreState;
#[cfg(feature = "gui")]
use crate::model::LogicalPx;
#[cfg(any(feature = "gui", test))]
use crate::model::PhysicalPx;
use crate::runtime::engine_read::EngineRead;

#[cfg(feature = "gui")]
mod main;
#[cfg(feature = "gui")]
pub(crate) mod terminal_view;
#[cfg(feature = "gui")]
pub use main::MainViewState;
#[cfg(not(feature = "gui"))]
mod command;
#[cfg(not(feature = "gui"))]
pub use command::CommandContext;

// Request adapters share algorithms, while the actual owner types remain
// separate. Neither branch instantiates the other owner's state.
#[cfg(not(feature = "gui"))]
pub(crate) use CommandContext as RequestContext;
#[cfg(feature = "gui")]
pub(crate) use MainViewState as RequestContext;

/// 팝업 열기 요청. context는 플러그인에 보내고 target_surface는 호스트가 scope를 묶을 때만 쓴다.
/// 플러그인 context의 surface_id 같은 키를 호스트 소속 범위로 해석하지 않는다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
pub(crate) struct PendingPopupOpen {
    pub(crate) plugin_id: String,
    pub(crate) popup_id: String,
    pub(crate) context: serde_json::Value,
    pub(crate) target_surface: Option<u32>,
}

/// 파일 드래그 중 누적한 경로와 시작 좌표.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Default)]
pub struct DropHoverState {
    pub(crate) paths: Vec<std::path::PathBuf>,
    /// 드래그 중 이동 이벤트가 없을 수 있어 기록한 시작 좌표(물리 픽셀).
    pub(crate) cursor: Option<(f32, f32)>,
}

/// Explorer에서 복사하거나 잘라낸 경로. 붙여넣기가 소비한다.
#[derive(Clone, Debug)]
#[cfg(feature = "gui")]
pub struct ExplorerClipboard {
    pub(crate) identity: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub paths: Vec<std::path::PathBuf>,
    pub cut: bool,
    /// 경로가 가리키는 파일시스템. 원격 경로 문자열을 로컬 경로로 해석하지 않도록 복사할 때 기록한다.
    pub source: ExplorerPathSource,
    /// 이 목록을 정한 직후 읽은 OS 클립보드. 붙여넣을 때와 다르면 OS 쪽이 더 새롭다. 읽지 못했으면 None.
    pub(crate) os_seen: Option<crate::clipboard::files::OsClip>,
}

/// Explorer 경로의 출처. 같은 문자열이라도 출처가 다르면 다른 파일이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "gui")]
pub enum ExplorerPathSource {
    Local,
    /// mirror workspace에 보이는 원격 호스트의 경로. 값은 로컬 workspace ID다.
    Remote {
        workspace: u32,
    },
}

impl RequestContext {
    /// 기존 engine의 복원된 활성 인덱스와 저장소 핸들로 화면 상태를 초기화한다.
    pub fn new(
        engine: &EngineRead<'_>,
        preset_store: std::sync::Arc<std::sync::Mutex<tasty_presets::PresetStore>>,
    ) -> Self {
        let mut navigation = navigation::NavigationState::default();
        navigation.reconcile(&engine.workspaces());
        #[cfg(not(feature = "gui"))]
        drop(preset_store);
        Self {
            #[cfg(feature = "gui")]
            preset_store: preset_catalog::PresetCatalog::new(preset_store),
            navigation,
            #[cfg(feature = "gui")]
            pending_move: None,
            #[cfg(feature = "gui")]
            tab_bar_scroll: Default::default(),
            #[cfg(feature = "gui")]
            layout_previews: Default::default(),
            #[cfg(feature = "gui")]
            attach_pending: Default::default(),
            #[cfg(feature = "gui")]
            attach_waits: Default::default(),
            #[cfg(feature = "gui")]
            terminal_views: Default::default(),
            #[cfg(any(feature = "gui", debug_assertions, test))]
            category_last_active: std::collections::HashMap::new(),
            #[cfg(not(feature = "gui"))]
            engine_id: None,
            #[cfg(feature = "gui")]
            settings_open_requested: false,
            #[cfg(feature = "gui")]
            plugins_open: false,
            #[cfg(feature = "gui")]
            sidebar_width: engine.settings.appearance.scaled_sidebar_width(),
            #[cfg(feature = "gui")]
            sidebar_visible: true,
            #[cfg(feature = "gui")]
            sidebar_collapsed: false,
            #[cfg(feature = "gui")]
            pending_resize_cursor: None,
            #[cfg(feature = "gui")]
            switch_overlay: None,
            #[cfg(feature = "gui")]
            modifier_hint: crate::adapters::ui::modifier_hint_overlay::ModifierHintRuntime::default(
            ),
            #[cfg(feature = "gui")]
            tutorial: crate::adapters::ui::tutorial::TutorialRuntime::default(),
            #[cfg(feature = "gui")]
            dialogs: DialogState::new(),
            #[cfg(any(feature = "gui", test))]
            tab_bar_height: PhysicalPx(0.0),
            #[cfg(feature = "gui")]
            native_content_rects: Vec::new(),
            #[cfg(feature = "gui")]
            last_focused_surface_id: None,
            #[cfg(feature = "gui")]
            last_active_workspace_id: None,
            #[cfg(feature = "gui")]
            last_focused_tab: None,
            #[cfg(feature = "gui")]
            #[cfg(feature = "gui")]
            popup_hovered: false,
            #[cfg(feature = "gui")]
            plugin_popup_open: false,
            #[cfg(feature = "gui")]
            banner_hovered: false,
            #[cfg(feature = "gui")]
            link_remote_tip: None,
            #[cfg(feature = "gui")]
            modifier_hint_hovered: false,
            #[cfg(feature = "gui")]
            popup_layers: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_layers: Vec::new(),
            #[cfg(feature = "gui")]
            webview_overlay: Default::default(),
            #[cfg(feature = "gui")]
            host_popup_hittest: Vec::new(),
            #[cfg(feature = "gui")]
            popup_escape_owner: None,
            #[cfg(feature = "gui")]
            popup_keyboard_focus: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_hittest: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_placed_scopes: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            banner_layer: None,
            #[cfg(feature = "gui")]
            modifier_hint_layer: None,
            #[cfg(feature = "gui")]
            resize_edge_widget_hovered: false,
            #[cfg(feature = "gui")]
            titlebar_close_rect: None,
            #[cfg(feature = "gui")]
            halt_pointer_inside: false,
            recent_files: crate::recent_files::RecentFiles::load(),
            #[cfg(feature = "gui")]
            popups: {
                let mut pm = crate::adapters::ui::PopupManager::new();
                let ui_zoom = engine.settings.appearance.ui_scale_factor();
                for def in crate::adapters::ui::popup::defs::all_defs() {
                    pm.register_def(def, ui_zoom);
                }
                pm
            },
            #[cfg(feature = "gui")]
            fullscreen_stage: None,
            #[cfg(feature = "gui")]
            stage_closed_queue: Vec::new(),
            #[cfg(feature = "gui")]
            stage_deferred_grid_resync: false,
            #[cfg(feature = "gui")]
            search: crate::search_state::SearchState::new(),
            #[cfg(feature = "gui")]
            port_scan: crate::adapters::ui::popup::port_scanner::PortScanState::Idle,
            #[cfg(feature = "gui")]
            port_favorites_scan: crate::adapters::ui::popup::port_scanner::PortScanState::Idle,
            #[cfg(feature = "gui")]
            command_palette: crate::state::command_palette::CommandPaletteState::default(),
            #[cfg(feature = "gui")]
            toasts: crate::adapters::ui::ToastManager::new(),
            #[cfg(feature = "gui")]
            banners: crate::adapters::ui::BannerManager::new(),
            #[cfg(feature = "gui")]
            branch_cache: branch::BranchCache::default(),
            #[cfg(feature = "gui")]
            attach_notices: Default::default(),
            #[cfg(feature = "gui")]
            restore_notices: Default::default(),
            #[cfg(feature = "gui")]
            attach_size_syncs: Default::default(),
            #[cfg(feature = "gui")]
            shell_integration_hint_shown: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            explorer_views: Default::default(),
            #[cfg(feature = "gui")]
            explorer_clipboard: Default::default(),
            #[cfg(feature = "gui")]
            file_clipboard: Default::default(),
            #[cfg(feature = "gui")]
            explorer_file_requests: Default::default(),
            #[cfg(feature = "gui")]
            script_reads: Default::default(),
            #[cfg(feature = "gui")]
            dag_graph_views: Default::default(),
            #[cfg(feature = "gui")]
            tool_registry: crate::plugin::tool_registry::ToolRegistry::new(),
            #[cfg(feature = "gui")]
            palette_plugin_commands: Vec::new(),
            #[cfg(feature = "gui")]
            pending_plugin_command_invokes: Vec::new(),
            #[cfg(feature = "gui")]
            pending_tool_events: Vec::new(),
            #[cfg(feature = "gui")]
            pending_popup_opens: Vec::new(),
            #[cfg(feature = "gui")]
            pending_handler_ipc: Vec::new(),
            #[cfg(feature = "gui")]
            file_handler_recent: handler_recent::load(),
            #[cfg(feature = "gui")]
            drop_hover: None,
            #[cfg(feature = "gui")]
            pending_file_drops: Vec::new(),
            #[cfg(feature = "gui")]
            pending_drop_position: None,
            #[cfg(feature = "gui")]
            plugin_popup_closes: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_focus_bumps: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_banner_closes: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_popup_regions: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_ime_cursor_area: None,
            #[cfg(feature = "gui")]
            plugin_mesh_popup_forward: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            plugin_popup_user_activated: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            webview_identity: std::sync::Weak::new(),
            #[cfg(feature = "gui")]
            webview_user_nav: Vec::new(),
            #[cfg(feature = "gui")]
            webview_agent_nav: Vec::new(),
            #[cfg(all(feature = "gui", debug_assertions))]
            debug_webview_stop: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_regions: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_forward: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_popup_pending_repaint: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_pending_repaint: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            html_content_insets: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            html_address: Default::default(),
            pending_intents: Vec::new(),
        }
    }

    pub fn dispatch_intent(&mut self, intent: crate::intent::DispatchedIntent) {
        self.pending_intents.push(intent);
    }

    pub fn take_pending_intents(&mut self) -> Vec<crate::intent::DispatchedIntent> {
        std::mem::take(&mut self.pending_intents)
    }

    /// 등록된 convert_input_popup을 열도록 요청한다. 변환 대상이 있으면 context에 담는다.
    /// kind나 팝업 선언을 찾지 못하면 경고를 기록하고 false를 반환한다.
    #[cfg(feature = "gui")]
    pub(crate) fn enqueue_convert_input_popup(
        &mut self,
        engine: &EngineRead<'_>,
        kind: &str,
        convert_surface_id: Option<u32>,
    ) -> bool {
        let Some(popup_ref) = engine
            .surface_registry
            .get(kind)
            .and_then(|d| d.convert_input_popup.clone())
        else {
            tracing::warn!(
                "convert-input popup: kind '{kind}' has no convert_input_popup capability"
            );
            return false;
        };
        let Some((plugin_id, local_id)) = popup_ref.split_once('/') else {
            tracing::warn!("convert-input popup: malformed convert_input_popup '{popup_ref}'");
            return false;
        };
        // 변환 대상이 있으면 포커스와 달라도 그 surface의 cwd를 사용한다.
        let origin = convert_surface_id.or_else(|| self.focused_surface_id(engine));
        let mut context = self.popup_surface_context(engine, origin);
        if let Some(sid) = convert_surface_id {
            context["surface_id"] = serde_json::json!(sid);
        }
        self.pending_popup_opens.push(PendingPopupOpen {
            plugin_id: plugin_id.to_string(),
            popup_id: local_id.to_string(),
            context,
            target_surface: origin,
        });
        true
    }

    /// 도구 메뉴·변환 팝업에 보낼 surface 컨텍스트.
    /// cwd는 inherit_cwd 설정을 따르는 로컬 경로, observed_cwd는 설정과 무관한 로컬 경로다.
    /// 원격 경로는 remote_cwd에 따로 담으며 없으면 해당 관측 키를 생략한다.
    /// origin_surface_id는 대상 ID이고, mirror이면 mirror와 local_surface_id를 추가한다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn popup_surface_context(
        &self,
        engine: &EngineRead<'_>,
        surface_id: Option<u32>,
    ) -> serde_json::Value {
        use crate::core::state::SurfaceCwd;
        let Some(sid) = surface_id else {
            return serde_json::json!({ "cwd": null });
        };
        let cwd = self
            .resolve_inherit_cwd_from_surface(engine, sid)
            .map(|p| p.to_string_lossy().into_owned());
        let mut context = serde_json::json!({ "cwd": cwd, "origin_surface_id": sid });
        match engine.surface_cwd(sid) {
            Some(SurfaceCwd::Local(p)) => {
                context["observed_cwd"] = serde_json::json!(p.to_string_lossy());
            }
            Some(SurfaceCwd::Remote(r)) => {
                context["remote_cwd"] = serde_json::json!(r.as_str());
            }
            None => {}
        }
        if engine.is_mirror_surface(sid) {
            context["mirror"] = serde_json::json!(true);
            context["local_surface_id"] = serde_json::json!(sid);
        }
        context
    }

    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn has_settings_open_request(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.settings_open_requested
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn has_plugin_popup(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.plugin_popup_open
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    /// 텍스트 입력 대화상자가 있는지 반환한다. 헤드리스 debug 조회에서는 false다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub fn has_input_dialog_open(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.dialogs.has_text_input_open()
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    /// 키·IME를 egui로 보내고 터미널 전달을 막을지 판단한다. 두 입력 경로가 같은 조건을 사용한다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn keyboard_overlay_open(&self) -> bool {
        #[cfg(feature = "gui")]
        let address = self.html_address.has_keyboard_focus();
        #[cfg(not(feature = "gui"))]
        let address = false;
        address || self.keyboard_overlay_open_besides_html_address()
    }

    /// html 주소 띠를 뺀 나머지 입력 게이트(설정·입력 대화상자·popup·plugin popup 등)가 열렸는지.
    /// 주소 띠만 포커스를 가졌을 때 공통 새로고침을 그 surface 로 보낼지 정할 때 쓴다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn keyboard_overlay_open_besides_html_address(&self) -> bool {
        #[cfg(feature = "gui")]
        let host_popup_focused = self.popups.has_focused()
            || (self.tutorial.active.is_some() && self.tutorial.keyboard_focus)
            || self.restore_notices.has_keyboard_focus();
        #[cfg(not(feature = "gui"))]
        let host_popup_focused = false;
        keyboard_overlay_open(
            self.has_settings_open_request(),
            self.has_input_dialog_open(),
            host_popup_focused,
            self.has_plugin_popup(),
        )
    }

    /// 파일 피커 요청자의 owner_popup_instance로 열린 자식이 있는지 확인한다.
    #[cfg(feature = "gui")]
    pub(crate) fn plugin_popup_has_open_child(&self, instance_id: u64) -> bool {
        self.dialogs
            .file_picker
            .as_ref()
            .and_then(|d| d.requester.as_ref())
            .and_then(|r| r.owner_popup_instance)
            == Some(instance_id)
    }

    /// 같은 화면 위의 egui 오버레이 때문에 네이티브 WebView를 가려야 하는지 판단한다.
    #[cfg(feature = "gui")]
    pub fn has_egui_overlay_open(&self) -> bool {
        // 모달은 별도 OS 창이므로 열기 요청이나 활성 모달 상태를 이 화면의 가림 조건에 넣지 않는다.
        let open = self.dialogs.has_any_overlay() || self.plugin_popup_open;
        // 무대가 열려도 OS 자식 WebView는 그대로 남으므로 표시를 명시적으로 꺼야 한다.
        open || self.popups.has_visible_open()
            || self.fullscreen_stage.is_some()
            || self.tutorial.active.is_some()
    }

    /// 같은 무대는 유지하고 다른 무대는 닫은 뒤 교체한다. 정의에 없는 ID는 false를 반환한다.
    /// 호출자는 화면을 다시 그리도록 dirty를 설정해야 한다.
    #[cfg(feature = "gui")]
    pub fn open_fullscreen_stage(&mut self, id: &str) -> bool {
        let Some(def) = crate::adapters::ui::fullscreen::defs::find(id) else {
            return false;
        };
        if self
            .fullscreen_stage
            .as_ref()
            .is_some_and(|s| s.id == def.id())
        {
            return true;
        }
        self.close_fullscreen_stage();
        self.fullscreen_stage = Some(crate::adapters::ui::fullscreen::StageState { id: def.id() });
        true
    }

    /// 활성 무대를 제거하고 on_close 처리 큐에 넣는다. 활성 무대가 없으면 false다.
    #[cfg(feature = "gui")]
    pub fn close_fullscreen_stage(&mut self) -> bool {
        match self.fullscreen_stage.take() {
            Some(stage) => {
                self.stage_closed_queue.push(stage.id);
                true
            }
            None => false,
        }
    }

    #[cfg(feature = "gui")]
    pub fn fullscreen_stage_id(&self) -> Option<crate::adapters::ui::fullscreen::StageId> {
        self.fullscreen_stage.as_ref().map(|s| s.id)
    }

    /// 전체화면 무대 표시 여부. 헤드리스 debug 조회에서는 false다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub fn fullscreen_stage_active(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.fullscreen_stage.is_some()
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    /// 닫힌 surface의 화면 전용 cache를 해제한다. 도메인 자원은 engine이 정리한다.
    #[cfg(feature = "gui")]
    pub(crate) fn release_surface_views(&mut self, surface_id: u32) {
        self.terminal_views.remove(surface_id);
        self.explorer_views.drop_view(surface_id);
        self.dag_graph_views.drop_view(surface_id);
        self.shell_integration_hint_shown.remove(&surface_id);
    }

    #[cfg(feature = "gui")]
    pub fn focused_surface_type(&self, engine: &CoreState) -> FocusedSurfaceType {
        let pane = match self.focused_pane(engine) {
            Some(p) => p,
            None => return FocusedSurfaceType::None,
        };
        let tab = match pane.tabs.get(self.navigation.tab_index(pane)) {
            Some(t) => t,
            None => return FocusedSurfaceType::None,
        };

        if let Some(leaf) = tab
            .layout()
            .find_surface(self.navigation.surface_id(tab).unwrap_or(0))
        {
            return Self::surface_to_type(leaf);
        }

        FocusedSurfaceType::None
    }

    #[cfg(feature = "gui")]
    fn surface_to_type(surface: &crate::model::SurfaceDescriptor) -> FocusedSurfaceType {
        match surface.kind() {
            "terminal" => FocusedSurfaceType::Terminal,
            other => FocusedSurfaceType::Kind(other.to_string()),
        }
    }

    /// cwd 상속 설정이 켜져 있으면 포커스된 surface의 로컬 경로를 반환한다.
    /// 원격 mirror의 경로는 로컬 PTY 작업 디렉터리로 사용할 수 없어 제외한다.
    pub(crate) fn resolve_inherit_cwd(
        &self,
        engine: &EngineRead<'_>,
    ) -> Option<std::path::PathBuf> {
        if !engine.settings.general.inherit_cwd || engine.workspaces().is_empty() {
            return None;
        }
        let sid = self.focused_surface_id(engine)?;
        engine.local_surface_cwd(sid)
    }

    /// cwd 상속 설정이 켜져 있으면 지정한 surface의 로컬 경로를 반환한다.
    pub(crate) fn resolve_inherit_cwd_from_surface(
        &self,
        engine: &EngineRead<'_>,
        surface_id: u32,
    ) -> Option<std::path::PathBuf> {
        if !engine.settings.general.inherit_cwd {
            return None;
        }
        engine.local_surface_cwd(surface_id)
    }
}

/// 플러그인 팝업도 egui 입력이 필요하지만 호스트 PopupManager에는 없으므로 별도로 검사한다.
#[cfg(any(feature = "gui", debug_assertions, test))]
pub(crate) fn keyboard_overlay_open(
    settings_open_requested: bool,
    input_dialog_open: bool,
    host_popup_focused: bool,
    plugin_popup_open: bool,
) -> bool {
    settings_open_requested || input_dialog_open || host_popup_focused || plugin_popup_open
}

#[cfg(test)]
mod keyboard_overlay_tests {
    use super::keyboard_overlay_open;

    #[test]
    fn plugin_popup_alone_opens_the_gate() {
        assert!(keyboard_overlay_open(false, false, false, true));
    }

    #[test]
    fn nothing_open_keeps_the_gate_closed() {
        assert!(!keyboard_overlay_open(false, false, false, false));
    }

    #[test]
    fn each_existing_predicate_still_opens_the_gate() {
        assert!(keyboard_overlay_open(true, false, false, false));
        assert!(keyboard_overlay_open(false, true, false, false));
        assert!(keyboard_overlay_open(false, false, true, false));
    }
}

#[cfg(test)]
mod tab_bar_height_seed_tests {
    use super::*;

    // 물리 높이의 미측정 초기값을 논리 토큰과 혼동하지 않아야 한다.
    #[test]
    fn the_seed_is_not_the_logical_token() {
        let (state, mut _engine_session) = super::tests::test_state();
        let _engine = _engine_session.borrow_mut();
        let seeded = state.tab_bar_height;
        assert_eq!(seeded, PhysicalPx(0.0), "측정 전 탭 바 높이는 0이어야 한다");
        assert_ne!(
            seeded.value(),
            tasty_type_appearance::theme::SIZING.tab_bar_height.value(),
            "측정 전 물리 높이에 논리 길이 토큰을 넣으면 안 된다"
        );
    }
}

#[cfg(feature = "gui")]
pub(crate) use main::PendingMove;
