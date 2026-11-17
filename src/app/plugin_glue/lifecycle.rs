//! 플러그인을 설치·제거하고 권한과 레지스트리를 갱신한다.
//! 결과 이벤트의 후속 처리는 App이 담당한다.

use crate::app::App;
use crate::app::command::CoreEvent;
#[cfg(test)]
use crate::app::command::PluginRegistryChange;
#[cfg(test)]
use crate::plugin::Manifest;
#[cfg(test)]
use crate::plugin::PluginManager;
use crate::plugin::PluginPackage;
use crate::plugin::manifest::SurfaceKindRendering;

pub(super) fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

fn register_plugin_surface_kinds(
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
    plugin_id: &str,
    pkg: &PluginPackage,
    tx: &std::sync::mpsc::Sender<crate::plugin_bridge::host_cmd::HostCmd>,
) -> Vec<CoreEvent> {
    let mut events = Vec::new();
    for decl in &pkg.manifest.surface_kinds {
        if let Some(default) = &decl.default_colors {
            tasty_themes::add_plugin_surface_default(&decl.kind, default.clone());
        }
        let rendering = match decl.rendering {
            SurfaceKindRendering::Remote => {
                crate::plugin_bridge::remote_kind::register_remote_kind(
                    registry,
                    plugin_id,
                    decl,
                    tx.clone(),
                );
                "remote"
            }
            SurfaceKindRendering::Webview => {
                crate::runtime::surface_registry::webview_kind::register_webview_kind(
                    plugin_id,
                    decl,
                    &pkg.manifest.contributes.settings_pages,
                );
                crate::plugin_bridge::remote_kind::register_remote_kind(
                    registry,
                    plugin_id,
                    decl,
                    tx.clone(),
                );
                "webview"
            }
            SurfaceKindRendering::EguiMesh => {
                // 거절된 선언은 등록 이벤트를 내지 않는다. 이유는 등록 함수가 warn으로 남긴다.
                if !crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
                    registry,
                    plugin_id,
                    decl,
                    &pkg.manifest.api_version,
                ) {
                    continue;
                }
                "egui-mesh"
            }
        };
        events.push(CoreEvent::PluginSurfaceKindRegistered {
            plugin_id: plugin_id.to_string(),
            kind: decl.kind.clone(),
            rendering: rendering.to_string(),
        });
    }
    events
}

fn collect_window_declared_events(plugin_id: &str, pkg: &PluginPackage) -> Vec<CoreEvent> {
    let mut events = Vec::new();
    for w in &pkg.manifest.contributes.window {
        tracing::info!(
            "plugin '{}' declared window '{}' (window creation is not implemented here)",
            plugin_id,
            w.id
        );
        events.push(CoreEvent::PluginWindowDeclared {
            plugin_id: plugin_id.to_string(),
            window_id: w.id.clone(),
        });
    }
    events
}

#[cfg(test)]
fn copy_into_plugins(
    src: &std::path::Path,
    dest_root: &std::path::Path,
    dest: &std::path::Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest_root).map_err(|e| anyhow::anyhow!("create dir failed: {e}"))?;
    copy_dir_recursive(src, dest).map_err(|e| anyhow::anyhow!("copy failed: {e}"))
}

/// 이번 설치가 만든 폴더를 지운다. 설치 전 `dest` 가 없었음을 호출자가 확인했다.
#[cfg(test)]
pub(super) fn remove_partial_install(dest: &std::path::Path) {
    if dest.exists()
        && let Err(e) = std::fs::remove_dir_all(dest)
    {
        tracing::error!(
            "removing a failed plugin install at {} failed: {e}",
            dest.display()
        );
    }
}

/// 복사한 플러그인이 패키지 목록에 없는 이유. 신뢰 판정의 거부 사유가 있으면 그것을 쓴다.
#[cfg(test)]
fn not_registered_reason(mgr: &PluginManager, plugin_id: &str) -> String {
    use tasty_host_plugin::discovery::RejectionReason;
    match mgr.rejected.iter().find(|r| r.id == plugin_id) {
        Some(r) => match r.reason {
            RejectionReason::UnknownKey => {
                "it is signed by a key that is not in the trust store".to_string()
            }
            // 거부 기록의 원인은 판정의 설명 문장 그대로다.
            RejectionReason::SignatureInvalid => match &r.cause {
                Some(cause) => cause.clone(),
                None => "the signature could not be verified".to_string(),
            },
        },
        None => "plugin discovery did not load it (see the log for details)".to_string(),
    }
}

/// 패키지 목록에 들어간 플러그인의 명령·번역을 등록하고 비활성 설정이 없으면 실행한다.
#[cfg(test)]
fn register_installed(
    mgr: &mut PluginManager,
    manifest: &Manifest,
    dir: &std::path::Path,
) -> anyhow::Result<Vec<CoreEvent>> {
    mgr.command_registry.register_plugin(manifest);
    let lang_dir = dir.join(&manifest.lang_dir);
    crate::i18n::register_namespace(&manifest.id, &lang_dir);
    // 확장자는 packages에 의존하므로 등록 뒤 다시 계산한다.
    // 아래 enable은 생략될 수 있어 그 호출의 재계산에만 의존할 수 없다.
    mgr.recompute_extensions();
    mgr.debug_assert_extensions_fresh();

    let mut events = vec![CoreEvent::PluginRegistryChanged {
        plugin_id: manifest.id.clone(),
        change: PluginRegistryChange::Installed {
            version: manifest.version.clone(),
        },
    }];

    if !mgr.config.is_disabled(&manifest.id) {
        mgr.enable(&manifest.id)
            .map_err(|e| anyhow::anyhow!("enable after install failed: {e}"))?;
        events.push(CoreEvent::PluginEnableToggled {
            plugin_id: manifest.id.clone(),
            enabled: true,
        });
    }
    Ok(events)
}

/// [`App::plugin_install`] 의 본체. 매니저만 쓰므로 시험이 App 없이 부를 수 있다.
pub(super) fn manifest_digest(dir: &std::path::Path) -> anyhow::Result<String> {
    tasty_host_plugin::trust_verdict::manifest_sha256(dir).map_err(|e| {
        anyhow::anyhow!("invalid plugin at source: cannot read tasty-plugin.toml: {e}")
    })
}

/// `reviewed` 는 화면이 보인 매니페스트의 SHA-256 이다. 있으면 지금 내용과 같아야 설치한다.
#[cfg(test)]
pub(crate) fn install_into(
    mgr: &mut PluginManager,
    src_path: &std::path::Path,
    choice: &super::install_trust::InstallChoice,
    reviewed: Option<&str>,
) -> anyhow::Result<Vec<CoreEvent>> {
    // 판정·선택·권한 승인은 이 내용에 대한 것이다. 화면에서 본 내용과 다르면 설치하지 않는다.
    let judged = manifest_digest(src_path)?;
    if let Some(reviewed) = reviewed
        && reviewed != judged
    {
        anyhow::bail!(
            "the manifest in {} changed after it was reviewed; verify the folder again. Nothing was installed",
            src_path.display()
        );
    }
    let manifest = Manifest::load(src_path)
        .and_then(|m| {
            crate::plugin_bridge::manifest_validate::validate_bin_extras(&m)?;
            Ok(m)
        })
        .map_err(|e| anyhow::anyhow!("invalid plugin at source: {e}"))?;
    let dest_root = crate::plugin::plugin_root()
        .ok_or_else(|| anyhow::anyhow!("could not resolve plugins directory"))?;
    let dest = dest_root.join(&manifest.id);
    if dest.exists() {
        anyhow::bail!(
            "plugin '{}' already installed at {}",
            manifest.id,
            dest.display()
        );
    }
    let undo = super::install_trust::authorize(src_path, &manifest.id, choice)?;
    if let Err(e) = copy_into_plugins(src_path, &dest_root, &dest) {
        remove_partial_install(&dest);
        super::install_trust::undo(undo, &manifest.id);
        return Err(e);
    }
    // 판정한 뒤 복사하는 사이에 원본이 바뀌었으면 판정하지 않은 내용이 설치된 것이다.
    if manifest_digest(&dest).ok().as_deref() != Some(judged.as_str()) {
        remove_partial_install(&dest);
        super::install_trust::undo(undo, &manifest.id);
        anyhow::bail!(
            "the manifest in {} changed while it was being installed; verify the folder again. Nothing was installed",
            src_path.display()
        );
    }

    mgr.refresh_packages();
    if !mgr.packages().iter().any(|p| p.manifest.id == manifest.id) {
        let reason = not_registered_reason(mgr, &manifest.id);
        remove_partial_install(&dest);
        super::install_trust::undo(undo, &manifest.id);
        mgr.refresh_packages();
        anyhow::bail!("plugin '{}' was not registered: {reason}", manifest.id);
    }
    register_installed(mgr, &manifest, &dest)
}

impl App {
    /// hello가 끝난 플러그인의 hook·surface kind를 등록하고 Loaded 이벤트를 전달한다.
    /// surface_registry가 없으면 surface 등록은 건너뛰며 이 hello 결과를 재시도용으로 보관하지 않는다.
    pub(crate) fn finalize_plugin_hello(&mut self, hello_pairs: Vec<(String, String)>) {
        if hello_pairs.is_empty() {
            return;
        }
        // 창이 없어도 같은 process registry에 등록한다.
        let core_registry = self.services.registries.surface_registry.clone();
        // hook 검증은 화면 렌더링과 독립적이어서 surface_registry가 없어도 등록한다.
        let hook_event_registry = self.services.registries.plugin_hook_events.clone();
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        let mut events: Vec<CoreEvent> = Vec::new();

        for (plugin_id, _) in &hello_pairs {
            if let Some(pkg) = mgr.packages().iter().find(|p| &p.manifest.id == plugin_id) {
                let keys: Vec<String> = pkg
                    .manifest
                    .contributes
                    .hook_events
                    .iter()
                    .map(|h| h.key.clone())
                    .collect();
                if !keys.is_empty() {
                    hook_event_registry.register(plugin_id, keys);
                }
            }
        }

        let host_registry = mgr.surface_registry.is_some().then_some(core_registry);
        if let Some(registry) = host_registry {
            let tx = mgr.host_cmd_tx.clone();
            for (plugin_id, version) in &hello_pairs {
                if let Some(pkg) = mgr
                    .packages()
                    .iter()
                    .find(|p| &p.manifest.id == plugin_id)
                    .cloned()
                {
                    events.extend(register_plugin_surface_kinds(
                        &registry, plugin_id, &pkg, &tx,
                    ));
                    events.extend(collect_window_declared_events(plugin_id, &pkg));
                }
                mgr.registered_plugins.insert(plugin_id.clone());
                events.push(CoreEvent::PluginLoaded {
                    plugin_id: plugin_id.clone(),
                    version: version.clone(),
                });
            }
        } else {
            tracing::debug!(
                "plugin manager has no surface_registry; skipping surface registration of {} plugin(s)",
                hello_pairs.len()
            );
            // surface 등록을 생략해도 hello 완료에 대한 Loaded 이벤트는 전달한다.
            for (plugin_id, version) in &hello_pairs {
                events.push(CoreEvent::PluginLoaded {
                    plugin_id: plugin_id.clone(),
                    version: version.clone(),
                });
            }
        }

        self.cascade_plugin_events(events);
    }
}

#[cfg(test)]
mod surface_kind_event_tests {
    use super::*;

    fn package(id: &str, kind: &str) -> PluginPackage {
        let manifest: Manifest = toml::from_str(&format!(
            r#"
manifest_version = 1
id = "{id}"
name = "probe"
version = "0.1.0"
api_version = "{api}"

[entry]
type = "process"
command = "probe"

[[surface_kinds]]
kind = "{kind}"
display_name_i18n_key = "surface.kind.markdown"
rendering = "egui-mesh"
"#,
            api = tasty_plugin_manifest::HOST_API_VERSION,
        ))
        .expect("manifest");
        PluginPackage {
            dir: std::path::PathBuf::new(),
            manifest,
        }
    }

    fn registered_kinds(events: &[CoreEvent]) -> Vec<(String, String)> {
        events
            .iter()
            .filter_map(|e| match e {
                CoreEvent::PluginSurfaceKindRegistered {
                    plugin_id, kind, ..
                } => Some((plugin_id.clone(), kind.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn custom_egui_mesh_kind_registers_and_conflict_emits_no_event() {
        let registry = crate::runtime::surface_registry::SurfaceKindRegistry::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        let first = package("com.example.first", "widget");
        let events = register_plugin_surface_kinds(&registry, "com.example.first", &first, &tx);
        assert_eq!(
            registered_kinds(&events),
            vec![("com.example.first".to_string(), "widget".to_string())]
        );
        assert!(registry.contains("widget"));

        let second = package("com.example.second", "widget");
        let events = register_plugin_surface_kinds(&registry, "com.example.second", &second, &tx);
        assert!(registered_kinds(&events).is_empty());
        let def = registry.get_live("widget").expect("first owner kept");
        assert_eq!(def.source.plugin_id(), Some("com.example.first"));
    }
}

#[cfg(test)]
#[path = "lifecycle_install_tests.rs"]
mod install_tests;
