//! Settings 모달을 열고 닫을 때 저장 실패를 알린다.

use std::sync::Arc;

use crate::app::App;
use crate::view;

struct SettingsInitData {
    settings: crate::settings::Settings,
    file_format: Arc<crate::file::format::FileFormatRegistry>,
    file_handler: Arc<crate::file::handler::FileHandlerRegistry>,
    plugin_pages: Vec<tasty_host_plugin::SettingsPageEntry>,
}

impl App {
    pub(crate) fn open_settings_modal(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.view.is_modal_active() {
            return;
        }

        use winit::window::WindowAttributes;

        // 창 크기는 settings-window-width/height 토큰이다. 창 생성 시점의 크기라 UI 배율을 곱하지 않는다.
        use tasty_design_tokens::generated::component::settings::{WINDOW_HEIGHT, WINDOW_WIDTH};
        let size = winit::dpi::LogicalSize::new(WINDOW_WIDTH.value(), WINDOW_HEIGHT.value());
        let mut attrs = WindowAttributes::default()
            .with_title("Tasty Settings")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_visible(false); // Start hidden, show after first render
        if let Some(icon) = crate::app_icon::winit_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        // 생성 실패 시 기존 창은 유지하며 설정 모달 열기만 취소한다.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.notify_window_creation_failed(
                    crate::app::window_lifecycle::WindowCreationTarget::Settings,
                    crate::app::event::WindowRequestOrigin::User,
                    "failed to create settings window",
                    e,
                );
                return;
            }
        };

        let init = self.resolve_settings_init_data();

        let gpu = match self.create_gpu_state(window.clone(), &init.settings.appearance) {
            Ok(g) => g,
            Err(e) => {
                self.notify_window_creation_failed(
                    crate::app::window_lifecycle::WindowCreationTarget::Settings,
                    crate::app::event::WindowRequestOrigin::User,
                    "failed to initialize GPU for settings",
                    e,
                );
                return;
            }
        };

        let modal_window_id = window.id();
        self.settings_edit_owner = Some(crate::app::settings_edit::SettingsEditOwner {
            format: init.file_format.clone(),
            handler: init.file_handler.clone(),
        });
        let clipboard = self.window_clipboard(&window);
        let mut modal = view::SettingsView::new(
            gpu,
            window,
            clipboard,
            init.settings,
            crate::runtime::file_catalog::FormatCatalog::new(init.file_format),
            crate::runtime::file_catalog::HandlerCatalog::new(init.file_handler),
        );
        modal.set_plugin_shortcuts(self.snapshot_plugin_shortcuts());
        modal.set_plugin_bundle_context(self.plugin_bundle_context());
        modal.set_plugin_settings_pages(init.plugin_pages);
        self.apply_pending_tab_overrides(&mut modal);
        crate::view::ui::present_first_frame(&mut modal);
        self.open_modal(
            Box::new(modal),
            modal_window_id,
            crate::view::ModalKind::Settings,
        );
        tracing::info!("opened settings modal {:?}", modal_window_id);
    }

    /// 포커스 창의 설정·레지스트리를 사용한다. 없으면 설정 파일을 읽고 빈 파일 레지스트리를 만든다.
    fn resolve_settings_init_data(&self) -> SettingsInitData {
        let settings = if let Some((_, engine)) = self.focused_pair() {
            engine.runtime.settings.clone()
        } else {
            crate::settings::Settings::load()
        };

        let file_format = self.services.registries.file_format.clone();
        let file_handler = self.services.registries.file_handler.clone();
        let plugin_pages: Vec<tasty_host_plugin::SettingsPageEntry> = self
            .plugin_manager
            .as_ref()
            .map(|mgr| mgr.settings_pages.iter().cloned().collect())
            .unwrap_or_default();

        SettingsInitData {
            settings,
            file_format,
            file_handler,
            plugin_pages,
        }
    }

    /// 플러그인 Configure, 파일 핸들러, 권한 화면 진입 요청과 debug 빌드에서 지정한
    /// 초기 탭·하위 탭을 적용한다.
    fn apply_pending_tab_overrides(&mut self, modal: &mut view::SettingsView) {
        if std::mem::take(&mut self.state.pending_settings_plugin_tab) {
            modal.focus_plugin_tab();
        }
        if std::mem::take(&mut self.state.pending_settings_file_handler_tab) {
            modal.focus_file_handler_tab();
        }
        if std::mem::take(&mut self.state.pending_settings_macos_permissions_tab) {
            modal.focus_macos_permissions_tab();
        }
        #[cfg(debug_assertions)]
        {
            let tab = self.state.pending_settings_tab.take();
            let subtab = self.state.pending_settings_subtab.take();
            crate::app::ipc::warn_unknown_debug_tabs(
                tab.as_deref(),
                subtab.as_deref(),
                modal.apply_debug_tabs(tab.as_deref(), subtab.as_deref()),
            );
        }
    }
}
