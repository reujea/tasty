//! Plugins 모달에서 요청한 작업을 실행한다.

use crate::app::App;
use crate::{plugins_ui, window};

impl App {
    /// Keep the live process and visible state unchanged until the write succeeds.
    fn handle_set_enabled(&mut self, id: String, enabled: bool) {
        let change = tasty_host_plugin::manager::file_config::ConfigChange::Enabled { id, enabled };
        if let Err(error) = self.enqueue_plugin_config(change, |app, result| {
            if let Err(error) = result {
                tracing::warn!(error=%error.message,"plugin toggle failed");
            }
            app.refresh_tool_registry();
            app.refresh_palette_plugin_commands();
            app.refresh_plugin_window_after_file();
        }) {
            tracing::warn!(error=%error.message,"plugin toggle not accepted");
        }
    }

    pub(crate) fn refresh_plugin_window_after_file(&mut self) {
        let snapshot = self.snapshot_plugins();
        if let Some(modal_id) = self.view.active_modal_id()
            && let Some(modal) = self.view.views.get_mut(&modal_id)
            && let Some(view) = modal.as_any_mut().downcast_mut::<window::PluginsView>()
        {
            view.refresh_snapshot(snapshot);
        }
    }

    fn handle_uninstall(&mut self, id: String) {
        if let Err(error) = self.enqueue_plugin_remove(id, |app, result| {
            if let Err(error) = result {
                app.toast_settings_result(error.message, crate::adapters::ui::ToastKind::Error);
            }
        }) {
            self.toast_settings_result(error.message, crate::adapters::ui::ToastKind::Error);
        }
    }

    fn handle_reapprove(&mut self, id: String) {
        let modal = self.view.active_modal_id();
        let display_id = id.clone();
        let admitted = self.enqueue_plugin_reapprove(id, move |app, result| {
            let (message, kind) = match result {
                Ok(_) => (
                    crate::i18n::t_fmt("plugins.attn_reapproved", &display_id),
                    crate::adapters::ui::ToastKind::Success,
                ),
                Err(error) => (
                    crate::i18n::t_fmt("plugins.attn_reapprove_failed", &error.message),
                    crate::adapters::ui::ToastKind::Error,
                ),
            };
            if let Some(modal) = modal.and_then(|id| app.view.views.get_mut(&id))
                && let Some(view) = modal.as_any_mut().downcast_mut::<window::PluginsView>()
            {
                view.push_toast(message, kind);
            } else {
                app.toast_settings_result(message, kind);
            }
        });
        if let Err(error) = admitted {
            self.toast_settings_result(error.message, crate::adapters::ui::ToastKind::Error);
        }
    }

    /// 사용자가 누른 열기가 실패하면 이 창에 띄울 실패 토스트를 돌려준다. 성공은 알리지 않는다.
    fn handle_open_uri(
        &self,
        uri: &str,
        failure_key: &str,
    ) -> Option<(String, crate::adapters::ui::ToastKind)> {
        let opened = crate::terminal_link::open_uri(uri);
        if !opened {
            tracing::warn!("plugins modal: open failed: {uri}");
        }
        crate::terminal_link::open_failure_message(opened, failure_key, uri)
            .map(|message| (message, crate::adapters::ui::ToastKind::Error))
    }

    fn handle_install(
        &mut self,
        src_path: &str,
        choice: &super::install_trust::InstallChoice,
        reviewed: &str,
    ) {
        let modal = self.view.active_modal_id();
        let admitted = self.enqueue_plugin_install(
            src_path.into(),
            choice.clone(),
            Some(reviewed.into()),
            true,
            move |app, result| {
                app.finish_plugin_add(modal, result, true);
            },
        );
        if let Err(error) = admitted {
            self.finish_plugin_add(modal, Err(error), true);
        }
    }

    fn handle_register_unverified(&mut self, id: &str, reviewed: &str) {
        let modal = self.view.active_modal_id();
        let source = self
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.rejected.iter().find(|p| p.id == id))
            .map(|p| p.dir.clone());
        let Some(source) = source else {
            self.finish_plugin_add(
                modal,
                Err(crate::app::plugin_file_lifecycle::Failure::before(
                    "plugin is not waiting for a choice",
                )),
                false,
            );
            return;
        };
        let admitted = self.enqueue_plugin_install(
            source,
            super::install_trust::InstallChoice::AllowUnverified,
            Some(reviewed.into()),
            false,
            move |app, result| app.finish_plugin_add(modal, result, false),
        );
        if let Err(error) = admitted {
            self.finish_plugin_add(modal, Err(error), false);
        }
    }

    fn finish_plugin_add(
        &mut self,
        modal: Option<winit::window::WindowId>,
        result: Result<serde_json::Value, crate::app::plugin_file_lifecycle::Failure>,
        add: bool,
    ) {
        let (ok, message) = match result {
            Ok(body) => (
                true,
                crate::i18n::t_fmt(
                    if add {
                        "plugins.add_installed"
                    } else {
                        "plugins.attn_registered"
                    },
                    body["installed"].as_str().unwrap_or_default(),
                ),
            ),
            Err(error) => (
                false,
                crate::i18n::t_fmt(
                    if add {
                        "plugins.add_install_failed"
                    } else {
                        "plugins.attn_register_failed"
                    },
                    &error.message,
                ),
            ),
        };
        let kind = if ok {
            crate::adapters::ui::ToastKind::Success
        } else {
            crate::adapters::ui::ToastKind::Error
        };
        if let Some(modal) = modal.and_then(|id| self.view.views.get_mut(&id))
            && let Some(view) = modal.as_any_mut().downcast_mut::<window::PluginsView>()
        {
            if add {
                view.finish_add(ok);
            }
            view.push_toast(message, kind);
        } else if !ok {
            self.toast_settings_result(message, kind);
        }
    }

    pub(crate) fn process_plugins_window_actions(&mut self) {
        let Some(modal_id) = self.view.active_modal_id() else {
            return;
        };
        let Some(modal) = self.view.views.get_mut(&modal_id) else {
            return;
        };
        let Some(plugins_window) = modal.as_any_mut().downcast_mut::<window::PluginsView>() else {
            return;
        };
        let actions = std::mem::take(&mut plugins_window.pending_actions);
        if actions.is_empty() {
            return;
        }

        if self.plugin_manager.is_none() {
            return;
        }

        let mut pending_toasts: Vec<(String, crate::adapters::ui::ToastKind)> = Vec::new();
        let mut close_modal = false;
        let mut open_settings_plugin_tab = false;
        // Add 탭의 추가가 끝났는지. 성공하면 입력 화면으로, 실패하면 판정을 남긴 채 돌아간다.

        for action in actions {
            match action {
                plugins_ui::PluginsAction::SetEnabled { id, enabled } => {
                    self.handle_set_enabled(id, enabled);
                }
                plugins_ui::PluginsAction::Uninstall { id } => {
                    self.handle_uninstall(id);
                }
                plugins_ui::PluginsAction::OpenSettings => {
                    close_modal = true;
                    open_settings_plugin_tab = true;
                }
                plugins_ui::PluginsAction::Reapprove { id } => {
                    self.handle_reapprove(id);
                }
                plugins_ui::PluginsAction::RemoveReserved { dir } => {
                    self.handle_remove_reserved(dir);
                }
                plugins_ui::PluginsAction::Close => {
                    close_modal = true;
                }
                plugins_ui::PluginsAction::OpenInstallDir { path } => {
                    pending_toasts.extend(self.handle_open_uri(&path, "toast.open_folder_failed"));
                }
                plugins_ui::PluginsAction::OpenHomepage { url } => {
                    pending_toasts.extend(self.handle_open_uri(&url, "toast.open_link_failed"));
                }
                plugins_ui::PluginsAction::Install {
                    src_path,
                    manifest_sha256,
                } => {
                    self.handle_install(
                        &src_path,
                        &super::install_trust::InstallChoice::VerifiedOnly,
                        &manifest_sha256,
                    );
                }
                plugins_ui::PluginsAction::InstallUnverified {
                    src_path,
                    manifest_sha256,
                } => {
                    self.handle_install(
                        &src_path,
                        &super::install_trust::InstallChoice::AllowUnverified,
                        &manifest_sha256,
                    );
                }
                plugins_ui::PluginsAction::RegisterUnverified {
                    id,
                    manifest_sha256,
                } => {
                    self.handle_register_unverified(&id, &manifest_sha256);
                }
                plugins_ui::PluginsAction::TrustAndInstall {
                    src_path,
                    pubkey_b64,
                    manifest_sha256,
                } => {
                    self.handle_install(
                        &src_path,
                        &super::install_trust::InstallChoice::TrustKey { pubkey_b64 },
                        &manifest_sha256,
                    );
                }
            }
        }

        self.refresh_tool_registry();
        self.refresh_palette_plugin_commands();

        // 한 번에 하나의 모달만 열 수 있어 Plugins를 먼저 닫은 뒤 Settings를 요청한다.
        if close_modal {
            self.close_active_modal();
            if open_settings_plugin_tab {
                self.state.pending_settings_plugin_tab = true;
                crate::shortcuts::send_app_event(&self.view.proxy, crate::AppEvent::OpenSettings);
            }
            return;
        }

        let snapshot = self.snapshot_plugins();
        if let Some(modal) = self.view.views.get_mut(&modal_id)
            && let Some(plugins_window) = modal.as_any_mut().downcast_mut::<window::PluginsView>()
        {
            plugins_window.refresh_snapshot(snapshot);

            for (msg, kind) in pending_toasts {
                plugins_window.push_toast(msg, kind);
            }
        }
    }
}

#[cfg(test)]
mod rfc3339_tests {
    /// UTC 초 단위 형식의 길이·접미사·구분자 위치를 확인한다.
    #[test]
    fn now_rfc3339_is_seconds_z_utc() {
        let s = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        assert_eq!(s.len(), 20, "unexpected length: {s}");
        assert!(s.ends_with('Z'), "must end with Z: {s}");
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
        assert_eq!(s.as_bytes()[10], b'T');
        assert_eq!(s.as_bytes()[13], b':');
        assert_eq!(s.as_bytes()[16], b':');
    }
}
