//! Plugins 모달에 설치 목록과 확인이 필요한 플러그인을 표시한다.

use crate::app::App;
use crate::{plugin, plugins_ui};

impl App {
    pub(crate) fn snapshot_plugins(&self) -> plugins_ui::PluginsSnapshot {
        let Some(mgr) = self.plugin_manager.as_ref() else {
            return plugins_ui::PluginsSnapshot::default();
        };
        let plugins = mgr
            .packages()
            .iter()
            .map(|pkg| {
                let id = &pkg.manifest.id;
                plugins_ui::PluginEntry {
                    id: id.clone(),
                    name: pkg.manifest.name.clone(),
                    version: pkg.manifest.version.clone(),
                    description: pkg.manifest.description.clone(),
                    authors: pkg.manifest.authors.clone(),
                    homepage: pkg.manifest.homepage.clone(),
                    enabled: !mgr.config.is_disabled(id),
                    running: mgr.is_running(id),
                    health_error: mgr.is_auto_disabled(id),
                    builtin: plugin::is_builtin_plugin(id),
                    surface_kinds: pkg
                        .manifest
                        .surface_kinds
                        .iter()
                        .map(|k| k.kind.clone())
                        .collect(),
                    manifest_permissions: pkg.manifest.permissions.clone(),
                    commands: pkg
                        .manifest
                        .contributes
                        .commands
                        .iter()
                        .map(|cmd| {
                            let keybinding = match mgr.config.shortcut_override(id, &cmd.id) {
                                Some(ov) => {
                                    crate::plugin::registry_state::shortcut_override_display(Some(
                                        ov,
                                    ))
                                }
                                None => cmd.default_keybinding.clone(),
                            };
                            plugins_ui::PluginCommandEntry {
                                title_key: cmd.title_i18n_key.clone(),
                                keybinding,
                            }
                        })
                        .collect(),
                    log_path: mgr.log_path(id).to_string_lossy().into_owned(),
                    install_dir: pkg.dir.to_string_lossy().into_owned(),
                }
            })
            .collect();

        plugins_ui::PluginsSnapshot {
            plugins,
            attention: attention_entries(mgr),
            keycap_style: self.plugin_keycap_style(),
        }
    }
}

impl App {
    /// Plugins 창 키캡 표기. 모든 세션이 같은 설정을 받으므로 첫 엔진의 설정 › 일반 값을 쓴다.
    /// macOS 경로는 `cfg!(target_os = "macos")` 로 고른다.
    fn plugin_keycap_style(&self) -> tasty_ui_widgets::PluginKeycapStyle {
        let macos = cfg!(target_os = "macos");
        match self.engines().windowed_and_parked().next() {
            Some(engine) => {
                let general = &engine.runtime.settings.general;
                tasty_ui_widgets::PluginKeycapStyle::from_setting_names(
                    macos,
                    &general.alt_display_style,
                    &general.option_display_style,
                    &general.shift_display_style,
                )
            }
            None => tasty_ui_widgets::PluginKeycapStyle {
                macos,
                ..Default::default()
            },
        }
    }
}

/// 확인 필요 탭 항목. 사이드바 배지(`PluginManager::attention_count`)와 같은 항목을 센다.
pub(crate) fn attention_entries(mgr: &plugin::PluginManager) -> Vec<plugins_ui::AttentionEntry> {
    // 자동 비활성화된 플러그인은 설치 목록과 확인 필요 목록에 함께 표시한다.
    use crate::plugin::discovery::RejectionReason;
    let mut attention: Vec<plugins_ui::AttentionEntry> = mgr
        .rejected
        .iter()
        .map(|r| {
            let kind = match r.reason {
                RejectionReason::UnknownKey => plugins_ui::AttentionKind::UnknownKey,
                RejectionReason::SignatureInvalid => plugins_ui::AttentionKind::SignatureInvalid,
                RejectionReason::PermissionsChanged => {
                    plugins_ui::AttentionKind::PermissionsChanged
                }
            };
            let (description, homepage) = if kind.shows_manifest_text() {
                (r.description.clone(), r.homepage.clone())
            } else {
                (String::new(), String::new())
            };
            plugins_ui::AttentionEntry {
                id: r.id.clone(),
                name: r.name.clone(),
                version: r.version.clone(),
                authors: r.authors.clone(),
                description,
                homepage,
                builtin: r.builtin,
                kind,
                fingerprint: r.fingerprint.clone(),
                permissions_added: r.permissions_added.clone(),
                permissions_removed: r.permissions_removed.clone(),
                health_detail: None,
                cause: r.cause.clone(),
                unverified: r.unverified,
                manifest_sha256: r.manifest_sha256.clone(),
                folder: None,
            }
        })
        .collect();
    attention.extend(mgr.packages().iter().filter_map(|pkg| {
        let id = &pkg.manifest.id;
        // 사용자가 직접 끈 플러그인은 오류로 표시하지 않는다.
        if !mgr.is_auto_disabled(id) || mgr.config.is_disabled(id) {
            return None;
        }
        Some(plugins_ui::AttentionEntry {
            id: id.clone(),
            name: pkg.manifest.name.clone(),
            version: pkg.manifest.version.clone(),
            authors: pkg.manifest.authors.clone(),
            description: pkg.manifest.description.clone(),
            homepage: pkg.manifest.homepage.clone(),
            builtin: plugin::is_builtin_plugin(id),
            kind: plugins_ui::AttentionKind::HealthError,
            fingerprint: None,
            permissions_added: Vec::new(),
            permissions_removed: Vec::new(),
            health_detail: Some(mgr.log_path(id).to_string_lossy().into_owned()),
            cause: None,
            unverified: None,
            manifest_sha256: String::new(),
            folder: None,
        })
    }));
    attention.extend(mgr.reserved_blocked().iter().map(reserved_entry));
    attention
}

/// 예약 id 폴더 항목. 폴더 이름과 id 만 싣는다. 공식이 아닌 매니페스트는 자기를 Tasty 처럼 적을 수
/// 있어 이름·작성자·홈페이지·설명·버전을 보이지 않는다.
fn reserved_entry(
    blocked: &tasty_host_plugin::discovery::ReservedIdBlocked,
) -> plugins_ui::AttentionEntry {
    plugins_ui::AttentionEntry {
        id: blocked.id.clone(),
        name: super::file_reserved_remove::folder_name(&blocked.dir),
        version: String::new(),
        authors: Vec::new(),
        description: String::new(),
        homepage: String::new(),
        builtin: false,
        kind: plugins_ui::AttentionKind::ReservedId,
        fingerprint: None,
        permissions_added: Vec::new(),
        permissions_removed: Vec::new(),
        health_detail: None,
        cause: None,
        unverified: None,
        manifest_sha256: String::new(),
        folder: Some(blocked.dir.clone()),
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
