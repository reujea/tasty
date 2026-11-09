//! 모달은 일반 view 맵에 두고 ViewRegistry의 활성 모달로 구별하며 한 번에 하나만 활성화한다.
//! 종료 확인은 다른 모달과 달리 열기 실패 때도 종료를 계속한다.
//! 공통 첫 프레임 처리는 present_first_frame에 두고 각 창의 실패 정책은 호출부에 남긴다.

pub(crate) mod plugins;
pub(crate) mod preset;
pub(crate) mod quit;
pub(crate) mod settings;
pub(crate) mod shake;

use winit::window::WindowId;

use crate::app::App;
use crate::view;
use crate::view::ModalKind;
use crate::view::ui::View as _;

impl App {
    pub(crate) fn open_modal(
        &mut self,
        modal: Box<dyn crate::view::ui::View>,
        window_id: WindowId,
        kind: ModalKind,
    ) {
        self.view.views.insert(window_id, modal);
        self.view.set_active_modal(window_id, kind);
    }

    pub(crate) fn close_active_modal(&mut self) {
        let Some(active) = self.view.take_active_modal() else {
            return;
        };
        let Some(mut modal) = self.view.views.remove(&active.id) else {
            return;
        };
        if let Some(settings_modal) = modal.as_any_mut().downcast_mut::<view::SettingsView>() {
            // 창이 열린 동안 다른 경로가 바꾼 설정을 저장이 되돌리지 않도록 필드 단위로 병합한다.
            let new_settings = match self.engines().windowed_and_parked().next() {
                Some(current) => {
                    let merged = crate::settings::merge::merge_settings_edit(
                        settings_modal.opened_settings(),
                        &settings_modal.settings,
                        &current.runtime.settings,
                    );
                    if !merged.conflicts.is_empty() {
                        tracing::warn!(
                            fields = ?merged.conflicts,
                            "settings save: fields also changed elsewhere while the window was open; keeping the window's values"
                        );
                    }
                    merged.settings
                }
                None => settings_modal.settings.clone(),
            };
            let plugin_draft = settings_modal.take_plugin_shortcut_draft();
            // Only footer Save returns execution edits; Cancel leaves application services unchanged.
            let execution_edits = settings_modal.take_execution_edits();

            if execution_edits.is_some() {
                let origin =
                    crate::app::command::DomainIntent::UpdateSettings(new_settings.clone())
                        .from_user_menu(crate::app::font_rejection::SETTINGS_SAVE_MENU)
                        .origin;
                self.cascade_settings_updated(new_settings, &origin, None);
            }

            for main in self.main_windows_iter_mut() {
                main.state.settings_open_requested = false;
            }
            self.apply_plugin_shortcut_draft(plugin_draft);
            let owner = self.settings_edit_owner.take();
            if let (Some(owner), Some(edits)) = (owner, execution_edits) {
                self.apply_settings_edits(owner, edits);
            }
        } else if modal.as_any().is::<view::PluginsView>() {
            for main in self.main_windows_iter_mut() {
                main.state.plugins_open = false;
                main.mark_dirty();
            }
        }
    }
}
