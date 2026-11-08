use super::{
    install_trust::{self, InstallChoice, UnverifiedRefused},
    lifecycle,
};
use crate::{
    app::{
        App,
        command::{CoreEvent, PluginRegistryChange},
        plugin_file_lifecycle::Failure,
    },
    plugin::{Manifest, PluginPackage},
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tasty_host_plugin::{
    PluginsConfig, file_persistence::ConfigUpdate, known_plugins::KnownPlugins,
};

struct Installed {
    manifest: Manifest,
    dir: PathBuf,
    config: PluginsConfig,
}

fn failure(error: anyhow::Error) -> Failure {
    let reserved = error.downcast_ref::<tasty_host_plugin::official::ReservedIdError>();
    let details = error
        .downcast_ref::<UnverifiedRefused>()
        .map(|e| json!({"unverified":e.cause.token(),"plugin_id":e.plugin_id}))
        .or_else(|| reserved.map(|e| json!({"reserved_id":e.id,"plugin_id":e.id})))
        .unwrap_or_else(|| json!({}));
    Failure {
        message: error.to_string(),
        saved: Some(false),
        applied: false,
        details,
    }
}

/// 등록 확정 직전의 마지막 예약 id 판정. 설치·재승인 관문을 거친 뒤라 닿지 않아야 한다.
/// 닿으면 명령·번역 등록 전에 멈춘다. 파일·신뢰 DB·plugins.toml은 이미 커밋됐으므로
/// 관리자 부재 실패(`post_commit::Stage::Registration`)와 같이 저장됨·부분 완료로 보고한다.
fn admit_after_files(id: &str) -> Result<(), Failure> {
    tasty_host_plugin::official::admit_plugin_id(id).map_err(|e| Failure {
        message: e.to_string(),
        saved: Some(true),
        applied: false,
        details: json!({
            "files_committed": true,
            "registered": false,
            "partial": true,
            "reserved_id": e.id,
            "plugin_id": e.id,
        }),
    })
}

struct InstallRequest {
    source: PathBuf,
    root: PathBuf,
    known: PathBuf,
    choice: InstallChoice,
    reviewed: Option<String>,
    before: PluginsConfig,
    copy: bool,
}
#[derive(Default)]
struct Cleanup {
    undo: Option<tasty_host_plugin::trust_verdict::TrustRecordUndo>,
    owned: Option<PathBuf>,
}
impl Cleanup {
    fn failed(self, mut error: Failure) -> Failure {
        if let Some(undo) = self.undo
            && let Err(rollback) = undo.restore()
        {
            error.saved = None;
            error.details["rollback_failed"] = json!(true);
            error.message = format!("{}; trust rollback failed: {rollback}", error.message);
        }
        if let Some(dir) = self.owned {
            let cleanup = (|| {
                let metadata = std::fs::symlink_metadata(&dir)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(std::io::Error::other("install cleanup path changed kind"));
                }
                std::fs::remove_dir_all(dir)
            })();
            if let Err(cleanup) = cleanup {
                error.applied = true;
                error.details["cleanup_failed"] = json!(true);
                error.message = format!("{}; install cleanup failed: {cleanup}", error.message);
            }
        }
        error
    }
}
impl InstallRequest {
    fn execute(self) -> Result<Installed, Failure> {
        let mut cleanup = Cleanup::default();
        self.prepare(&mut cleanup)
            .map_err(|error| cleanup.failed(failure(error)))
    }
    fn prepare(&self, cleanup: &mut Cleanup) -> anyhow::Result<Installed> {
        let digest = lifecycle::manifest_digest(&self.source)?;
        if self
            .reviewed
            .as_ref()
            .is_some_and(|reviewed| reviewed != &digest)
        {
            anyhow::bail!(
                "the manifest changed after it was reviewed; verify the folder again. Nothing was installed"
            );
        }
        let manifest =
            Manifest::load(&self.source).map_err(|_| anyhow::anyhow!("invalid plugin manifest"))?;
        crate::plugin_bridge::manifest_validate::validate_bin_extras(&manifest)?;
        let dir = if self.copy {
            self.root.join(&manifest.id)
        } else {
            self.source.clone()
        };
        if self.copy {
            std::fs::create_dir_all(&self.root)?;
            std::fs::create_dir(&dir)
                .map_err(|e| anyhow::anyhow!("plugin destination unavailable: {e}"))?;
            cleanup.owned = Some(dir.clone());
        }
        cleanup.undo = install_trust::authorize_in(
            self.known.clone(),
            &self.source,
            &manifest.id,
            &manifest.permissions,
            &self.choice,
        )?;
        if self.copy {
            lifecycle::copy_dir_recursive(&self.source, &dir)?;
        }
        if lifecycle::manifest_digest(&dir)? != digest {
            anyhow::bail!("plugin manifest changed while installing; verify the folder again");
        }
        let db = KnownPlugins::load_from(self.known.clone())?;
        if !matches!(
            tasty_host_plugin::trust_verdict::judge_with(&dir, &db),
            tasty_host_plugin::trust_verdict::TrustVerdict::Trusted
                | tasty_host_plugin::trust_verdict::TrustVerdict::AcceptedUnverified
        ) {
            anyhow::bail!("plugin did not pass discovery after its trust record was saved");
        }
        let mut after = self.before.clone();
        after.set_granted(&manifest.id, manifest.permissions.clone());
        let config = ConfigUpdate::between(&self.before, &after)?
            .with_grants_intent(&manifest.id, manifest.permissions.clone())
            .execute()?;
        Ok(Installed {
            manifest,
            dir,
            config,
        })
    }
}

impl App {
    pub(crate) fn enqueue_plugin_install(
        &mut self,
        source: PathBuf,
        choice: InstallChoice,
        reviewed: Option<String>,
        copy: bool,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) -> Result<(), Failure> {
        let root = crate::plugin::plugin_root()
            .ok_or_else(|| Failure::before("plugin directory unavailable"))?;
        let known = KnownPlugins::path()
            .ok_or_else(|| Failure::before("known-plugins.toml path unavailable"))?;
        let choice_bytes = match &choice {
            InstallChoice::TrustKey { pubkey_b64 } => pubkey_b64.len(),
            _ => 0,
        };
        let input_bytes = source
            .as_os_str()
            .len()
            .saturating_add(root.as_os_str().len())
            .saturating_add(known.as_os_str().len())
            .saturating_add(reviewed.as_ref().map_or(0, String::len))
            .saturating_add(choice_bytes);
        self.plugin_files.enqueue_sized(input_bytes, move |app| {
            let Some(mgr) = app.plugin_manager.as_ref() else {
                finish(app, Err(Failure::before("plugin manager not initialized")));
                return;
            };
            let before = mgr.config.clone();
            let bytes = crate::app::file_writer::command::value_bytes(&before)
                + source.as_os_str().len()
                + reviewed.as_ref().map_or(0, String::len);
            app.run_plugin_file(
                (bytes, 0),
                move || {
                    InstallRequest {
                        source,
                        root,
                        known,
                        choice,
                        reviewed,
                        before,
                        copy,
                    }
                    .execute()
                },
                |result| match result {
                    Ok(ready) => {
                        crate::app::file_writer::command::value_bytes(&ready.config)
                            + format!("{:?}", ready.manifest).len()
                            + ready.dir.as_os_str().len()
                    }
                    Err(e) => e
                        .message
                        .len()
                        .saturating_add(crate::app::file_writer::command::value_bytes(&e.details)),
                },
                crate::app::plugin_file_lifecycle::classify_failure,
                move |app, result| match result.and_then(|result| result) {
                    Err(error) => finish(app, Err(error)),
                    Ok(ready) => {
                        app.after_plugin_retired(ready.manifest.id.clone(), move |app, retired| {
                            let result = retired
                                .map_err(Failure::after_file_receipt)
                                .and_then(|()| app.apply_installed(ready));
                            finish(app, result);
                        })
                    }
                },
            );
        })?;
        (self.services.reply_wake())();
        Ok(())
    }

    fn apply_installed(&mut self, ready: Installed) -> Result<Value, Failure> {
        let Installed {
            manifest,
            dir,
            config,
        } = ready;
        self.apply_plugin_registration(manifest, dir, config, true)
    }

    pub(super) fn apply_plugin_registration(
        &mut self,
        manifest: Manifest,
        dir: PathBuf,
        config: PluginsConfig,
        installed: bool,
    ) -> Result<Value, Failure> {
        admit_after_files(&manifest.id)?;
        let mgr = crate::app::plugin_file_lifecycle::post_commit::manager_after_files(
            &mut self.plugin_manager,
            crate::app::plugin_file_lifecycle::post_commit::Stage::Registration,
        )?;
        mgr.config = config;
        mgr.replace_committed_package(
            &manifest.id,
            Some(PluginPackage {
                dir: dir.clone(),
                manifest: manifest.clone(),
            }),
        );
        mgr.command_registry.register_plugin(&manifest);
        crate::i18n::register_namespace(&manifest.id, &dir.join(&manifest.lang_dir));
        let mut events = Vec::new();
        if installed {
            events.push(CoreEvent::PluginRegistryChanged {
                plugin_id: manifest.id.clone(),
                change: PluginRegistryChange::Installed {
                    version: manifest.version.clone(),
                },
            });
        }
        if !self.plugin_files.stopping() && !mgr.config.is_disabled(&manifest.id) {
            mgr.enable_committed(&manifest.id).map_err(|e| Failure {
                message: e.to_string(),
                saved: Some(true),
                applied: false,
                details: Value::Null,
            })?;
            events.push(CoreEvent::PluginEnableToggled {
                plugin_id: manifest.id.clone(),
                enabled: true,
            });
        }
        self.cascade_plugin_events(events);
        self.refresh_tool_registry();
        self.refresh_palette_plugin_commands();
        self.refresh_plugin_window_after_file();
        Ok(json!({"installed":manifest.id}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(home: &std::path::Path, choice: InstallChoice) -> InstallRequest {
        let source = home.join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(
            source.join("tasty-plugin.toml"),
            r#"
manifest_version=1
id="com.example.worker-install"
name="owned test"
version="1.0.0"
api_version="1"
permissions=["fs.read"]
[entry]
type="process"
command="unused"
"#,
        )
        .unwrap();
        InstallRequest {
            source,
            root: home.join("plugins"),
            known: home.join("known-plugins.toml"),
            choice,
            reviewed: None,
            before: PluginsConfig::load_from(home.join("plugins.toml")),
            copy: true,
        }
    }
    #[test]
    fn install_worker_confirms_both_files_and_preserves_latest_foreign_edits() {
        let home = tempfile::tempdir().unwrap();
        let request = request(home.path(), InstallChoice::AllowUnverified);
        std::fs::write(
            home.path().join("plugins.toml"),
            "# external\n[foreign]\nvalue='keep'\n",
        )
        .unwrap();
        let installed = std::thread::spawn(move || request.execute())
            .join()
            .unwrap()
            .unwrap();
        assert!(installed.dir.is_dir());
        assert!(
            installed
                .config
                .granted_permissions(&installed.manifest.id)
                .contains("fs.read")
        );
        let known = KnownPlugins::load_from(home.path().join("known-plugins.toml")).unwrap();
        assert!(known.unverified_choice(&installed.manifest.id).is_some());
        let text = std::fs::read_to_string(home.path().join("plugins.toml")).unwrap();
        assert!(text.contains("# external"));
        assert!(text.contains("value='keep'"));
    }
    #[test]
    fn refused_unverified_install_leaves_no_owned_directory_or_trust_approval() {
        let home = tempfile::tempdir().unwrap();
        let request = request(home.path(), InstallChoice::VerifiedOnly);
        let error = request.execute().err().unwrap();
        assert_eq!(error.details["unverified"], "unsigned");
        assert_eq!(error.saved, Some(false));
        assert!(!error.applied);
        assert!(
            !home
                .path()
                .join("plugins/com.example.worker-install")
                .exists()
        );
        assert!(!home.path().join("known-plugins.toml").exists());
    }
    #[test]
    fn an_unknown_reserved_id_is_refused_before_anything_is_written() {
        let home = tempfile::tempdir().unwrap();
        let request = request(home.path(), InstallChoice::AllowUnverified);
        let manifest = request.source.join("tasty-plugin.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            text.replace("com.example.worker-install", "com.tasty.fake"),
        )
        .unwrap();
        let error = request.execute().err().unwrap();
        assert_eq!(error.details["reserved_id"], "com.tasty.fake");
        assert!(
            error.message.contains("reserved com.tasty. prefix"),
            "{}",
            error.message
        );
        assert_eq!(error.saved, Some(false));
        assert!(!error.applied);
        assert!(!home.path().join("plugins").exists());
        assert!(!home.path().join("known-plugins.toml").exists());
        assert!(!home.path().join("plugins.toml").exists());
    }
    #[test]
    fn the_registration_guard_reports_committed_files_and_the_reserved_id() {
        let error = admit_after_files("com.tasty.fake").expect_err("reserved id is refused");
        assert_eq!(error.saved, Some(true));
        assert!(!error.applied);
        let data = error.response_data();
        assert_eq!(data["files_committed"], true);
        assert_eq!(data["registered"], false);
        assert_eq!(data["partial"], true);
        assert_eq!(data["reserved_id"], "com.tasty.fake");
        assert_eq!(data["saved"], true);
        assert!(admit_after_files("com.tasty.markdown").is_ok());
        assert!(admit_after_files("com.example.x").is_ok());
    }
    #[test]
    fn failed_config_restores_owned_trust_and_copy_without_touching_the_source() {
        let home = tempfile::tempdir().unwrap();
        let request = request(home.path(), InstallChoice::AllowUnverified);
        std::fs::write(home.path().join("plugins.toml"), "PRIVATE = [[[\n").unwrap();
        let error = request.execute().err().unwrap();
        assert!(!error.message.contains("PRIVATE"));
        assert_eq!(error.saved, Some(false));
        assert!(!error.applied);
        assert!(
            !home
                .path()
                .join("plugins/com.example.worker-install")
                .exists()
        );
        assert!(home.path().join("source/tasty-plugin.toml").exists());
        assert!(
            KnownPlugins::load_from(home.path().join("known-plugins.toml"))
                .unwrap()
                .unverified_choice("com.example.worker-install")
                .is_none()
        );
    }
    #[test]
    fn an_existing_destination_and_reviewed_manifest_change_are_never_cleaned_up() {
        let home = tempfile::tempdir().unwrap();
        let mut request = request(home.path(), InstallChoice::AllowUnverified);
        let dir = request.root.join("com.example.worker-install");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep"), "foreign").unwrap();
        request.reviewed = Some("different".into());
        assert!(request.execute().is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("keep")).unwrap(),
            "foreign"
        );
        let home = tempfile::tempdir().unwrap();
        let request = super::tests::request(home.path(), InstallChoice::AllowUnverified);
        let dir = request.root.join("com.example.worker-install");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep"), "foreign").unwrap();
        assert!(request.execute().is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("keep")).unwrap(),
            "foreign"
        );
        assert!(!home.path().join("known-plugins.toml").exists());
    }
}
