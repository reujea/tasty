//! 하나만 유지하는 modeless PresetView의 생성·복원·닫기.

use std::sync::Arc;

use winit::window::WindowId;

use crate::app::App;
use crate::view;

impl App {
    /// 이미 열려 있으면 새 창 대신 기존 창에 포커스를 준다.
    pub(crate) fn open_preset_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(id) = self.state.preset_view_id {
            if let Some(w) = self.view.views.get(&id) {
                w.base().winit.focus_window();
                return;
            }
            self.state.preset_view_id = None;
        }

        let attrs = Self::preset_window_attributes();
        let window = match Self::create_window_or_warn(event_loop, attrs) {
            Some(w) => w,
            None => return,
        };

        let appearance = self.focused_appearance_or_disk();
        // 단축키는 창을 열 때 읽어 두므로 설정 변경은 다시 열 때 반영된다.
        let keybindings = self.focused_keybindings_or_disk();
        let gpu = match self.create_gpu_state(window.clone(), &appearance) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("failed to init GPU for preset window: {e}");
                return;
            }
        };

        let drafts = {
            let store = crate::poison::recover_mutex(
                self.services.preset_store.lock(),
                crate::core::PRESET_STORE_WHAT,
                &crate::core::PRESET_STORE_POISONED,
            );
            crate::app::preset_editor::capture(&store)
        };
        let registry = Some(crate::runtime::kind_catalog::KindCatalog::new(
            self.services.registries.surface_registry.clone(),
        ));
        let window_id = window.id();
        let mut preset = {
            let clipboard = self.window_clipboard(&window);
            view::PresetView::new(gpu, window, clipboard, drafts, registry, keybindings)
        };
        crate::view::ui::present_first_frame(&mut preset);
        self.view.views.insert(window_id, Box::new(preset));
        self.state.preset_view_id = Some(window_id);
        tracing::info!("opened preset window {:?}", window_id);
    }

    fn preset_window_attributes() -> winit::window::WindowAttributes {
        use winit::window::WindowAttributes;
        let mut attrs = WindowAttributes::default()
            .with_title(crate::i18n::t("preset.window.title"))
            .with_inner_size(winit::dpi::LogicalSize::new(960, 640))
            .with_min_inner_size(winit::dpi::LogicalSize::new(760, 480))
            .with_visible(false);
        if let Some(icon) = crate::app_icon::winit_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }
        attrs
    }

    fn focused_appearance_or_disk(&self) -> crate::settings::AppearanceSettings {
        self.focused_pair()
            .map(|(_, engine)| engine.runtime.settings.appearance.clone())
            .unwrap_or_else(|| crate::settings::Settings::load().appearance)
    }

    fn focused_keybindings_or_disk(&self) -> crate::settings::KeybindingSettings {
        self.focused_pair()
            .map(|(_, engine)| engine.runtime.settings.keybindings.clone())
            .unwrap_or_else(|| crate::settings::Settings::load().keybindings)
    }

    fn create_window_or_warn(
        event_loop: &winit::event_loop::ActiveEventLoop,
        attrs: winit::window::WindowAttributes,
    ) -> Option<Arc<winit::window::Window>> {
        match event_loop.create_window(attrs) {
            Ok(w) => Some(Arc::new(w)),
            Err(e) => {
                tracing::warn!("failed to create preset window: {e}");
                None
            }
        }
    }

    pub(crate) fn on_preset_window_closed(&mut self, window_id: WindowId) {
        if self.state.preset_view_id != Some(window_id) {
            return;
        }
        self.state.preset_view_id = None;
        self.view.views.remove(&window_id);
    }

    /// 도구 메뉴나 저장 결과가 요청한 편집기를 열고 선택할 항목을 반영한다.
    pub(crate) fn process_pending_open_preset_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        let mut request_open = false;
        let mut pending_selection: Option<(tasty_presets::PresetKind, String)> = None;
        for w in self.main_windows_iter_mut() {
            if w.state.dialogs.pending_open_preset_window {
                w.state.dialogs.pending_open_preset_window = false;
                request_open = true;
            }
            if let Some(sel) = w.state.dialogs.pending_preset_window_selection.take() {
                pending_selection = Some(sel);
                request_open = true;
            }
        }
        if !request_open {
            return;
        }
        self.open_preset_window(event_loop);
        if let Some((kind, name)) = pending_selection
            && let Some(pwid) = self.state.preset_view_id
            && let Some(pw) = self
                .view
                .views
                .get_mut(&pwid)
                .and_then(|w| w.as_any_mut().downcast_mut::<view::PresetView>())
        {
            pw.select(kind, name);
        }
    }
}
