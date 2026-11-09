use crate::i18n::t;
use crate::settings::Settings;
use tasty_host_plugin::SettingsPageEntry;

use super::appearance::{draw_plugin_settings_page, find_plugin_settings_entry};

/// Plugin 탭 콘텐츠. L2 사이드바(plugin page 합성·필터·선택)는 settings 셸이
/// 소유하므로 여기서는 활성 `sub_tab` 의 page 콘텐츠만 그린다. `None` 이거나
/// page 가 사라진 경우 안내 메시지만 표시.
pub fn draw_plugin_tab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    sub_tab: Option<&crate::settings_ui::PluginSubTab>,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut super::font_rows::PreviewFonts,
    settings_pages: &[SettingsPageEntry],
) {
    use crate::settings_ui::PluginSubTab;
    match sub_tab {
        Some(PluginSubTab::Plugin { plugin_id, page_id }) => {
            if let Some(entry) = find_plugin_settings_entry(settings_pages, plugin_id, page_id) {
                draw_plugin_settings_page(
                    ui,
                    settings,
                    font_families,
                    preview_fonts,
                    plugin_id,
                    &entry.page,
                );
            } else {
                ui.label(t("settings.plugin.empty"));
            }
        }
        None => {
            ui.label(t("settings.plugin.empty"));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings_ui::{PluginSubTab, SettingsUiState};

    #[test]
    fn plugin_sub_tab_default_is_none() {
        let s = SettingsUiState::new();
        assert!(s.plugin_sub_tab.is_none());
    }

    #[test]
    fn plugin_sub_tab_variant_uses_composite_key() {
        // 동일 page_id 라도 plugin_id 가 다르면 다른 variant 로 식별.
        let a = PluginSubTab::Plugin {
            plugin_id: "alpha".into(),
            page_id: "main".into(),
        };
        let b = PluginSubTab::Plugin {
            plugin_id: "beta".into(),
            page_id: "main".into(),
        };
        assert_ne!(a, b);
    }
}
