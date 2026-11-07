use crate::{
    app::{
        App,
        command::{CoreEvent, PluginRegistryChange},
        plugin_file_lifecycle::Failure,
    },
    plugin,
};
use serde_json::{Value, json};
use tasty_host_plugin::{
    PluginsConfig, file_persistence::ConfigUpdate, known_plugins::KnownPlugins,
    manager::file_config::ConfigChange,
};

type Finish = Box<dyn FnOnce(&mut App, Result<Value, Failure>)>;
impl App {
    pub(crate) fn enqueue_plugin_remove(
        &mut self,
        id: String,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) -> Result<(), Failure> {
        let root =
            plugin::plugin_root().ok_or_else(|| Failure::before("plugin directory unavailable"))?;
        let known = KnownPlugins::path()
            .ok_or_else(|| Failure::before("known-plugins.toml path unavailable"))?;
        self.plugin_files.enqueue_sized(
            id.len()
                .saturating_add(root.as_os_str().len())
                .saturating_add(known.as_os_str().len()),
            move |app| app.start_plugin_remove(id, root, known, Box::new(finish)),
        )?;
        (self.services.reply_wake())();
        Ok(())
    }

    fn start_plugin_remove(
        &mut self,
        id: String,
        root: std::path::PathBuf,
        known: std::path::PathBuf,
        finish: Finish,
    ) {
        self.start_plugin_config(
            ConfigChange::Enabled {
                id: id.clone(),
                enabled: false,
            },
            move |app, result| match result {
                Err(error) => finish(app, Err(error)),
                Ok(_) => app.after_plugin_retired(id.clone(), move |app, retired| match retired {
                    Ok(()) => app.remove_plugin_files(id, root, known, finish),
                    Err(mut error) => {
                        error.saved = Some(false);
                        finish(app, Err(after_disable(error)));
                    }
                }),
            },
        );
    }

    fn remove_plugin_files(
        &mut self,
        id: String,
        root: std::path::PathBuf,
        known: std::path::PathBuf,
        finish: Finish,
    ) {
        let Some(before) = self.plugin_manager.as_ref().map(|mgr| mgr.config.clone()) else {
            finish(
                self,
                Err(after_disable(Failure::before("plugin manager unavailable"))),
            );
            return;
        };
        let bytes = crate::app::file_writer::command::value_bytes(&before) + id.len();
        let remove_id = id.clone();
        self.run_plugin_file((bytes,0),move ||remove_files(&root.join(&remove_id),&known,&remove_id,before),
            |result|match result {Ok(config)=>crate::app::file_writer::command::value_bytes(config),Err(e)=>e.message.len().saturating_add(crate::app::file_writer::command::value_bytes(&e.details))},
            crate::app::plugin_file_lifecycle::classify_failure,
            move |app,result| {
                let result=match result {
                    Err(error)=>Err(after_disable(error)),
                    Ok(Err(error))=>{
                        if error.applied && let Err(cleanup)=app.apply_removed(id,None) {tracing::warn!(error=%cleanup.message,"partial removal cache refresh failed");}
                        Err(after_disable(error))
                    }
                    Ok(Ok(config))=>app.apply_removed(id,Some(config)),
                };
                finish(app,result);
            },
        );
    }

    fn apply_removed(
        &mut self,
        id: String,
        config: Option<PluginsConfig>,
    ) -> Result<Value, Failure> {
        let mgr = crate::app::plugin_file_lifecycle::post_commit::manager_after_files(
            &mut self.plugin_manager,
            crate::app::plugin_file_lifecycle::post_commit::Stage::Removed {
                config_saved: config.is_some(),
            },
        )?;
        if let Some(config) = config {
            mgr.config = config;
        }
        mgr.replace_committed_package(&id, None);
        mgr.command_registry.unregister_plugin(&id);
        crate::i18n::unregister_namespace(&id);
        self.services.registries.plugin_hook_events.unregister(&id);
        self.cascade_plugin_events(vec![CoreEvent::PluginRegistryChanged {
            plugin_id: id.clone(),
            change: PluginRegistryChange::Removed,
        }]);
        self.refresh_tool_registry();
        self.refresh_palette_plugin_commands();
        self.refresh_plugin_window_after_file();
        Ok(json!({"removed":id}))
    }
}

fn remove_files(
    dir: &std::path::Path,
    known: &std::path::Path,
    id: &str,
    before: PluginsConfig,
) -> Result<PluginsConfig, Failure> {
    let mut removed = false;
    let mut cleanup_started = false;
    let result = (|| {
        // A changed path kind must not redirect recursive deletion through another user's link.
        let metadata = std::fs::symlink_metadata(dir)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::other(
                "plugin path is not an owned directory",
            ));
        }
        cleanup_started = true;
        std::fs::remove_dir_all(dir)?;
        removed = true;
        tasty_host_plugin::trust_verdict::forget_unverified_choice_in(known, id)?;
        let mut after = before.clone();
        after.enable(id);
        if tasty_host_plugin::builtin::is_builtin_plugin(id) {
            after.mark_builtin_removed(id);
        }
        let update = ConfigUpdate::between(&before, &after)?.with_enabled_intent(id, true);
        let update = if tasty_host_plugin::builtin::is_builtin_plugin(id) {
            update.with_builtin_removed_intent(id, true)
        } else {
            update
        };
        update.execute()
    })();
    result.map_err(|error| {
        let mut details = json!({"removed":removed,"directory_cleanup_started":cleanup_started});
        if cleanup_started && !removed {
            details["directory_outcome"] = json!("unknown");
            details["outcome"] = json!("unknown");
        }
        Failure {
            message: error.to_string(),
            saved: Some(false),
            applied: removed,
            details,
        }
    })
}

fn after_disable(mut error: Failure) -> Failure {
    error.applied = true;
    if !error.details.is_object() {
        error.details = json!({});
    }
    error.details["disabled"] = json!(true);
    error.details["partial"] = json!(true);
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn recursive_cleanup_error_retains_unknown_directory_outcome_and_known_disable() {
        use std::os::unix::fs::PermissionsExt;
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                if self.0.exists()
                    && let Err(error) =
                        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700))
                {
                    tracing::warn!(%error,"owned fixture mode restore failed");
                }
            }
        }
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join("plugin");
        std::fs::create_dir(&directory).unwrap();
        // One child per level fixes the order: the recursive walk deletes inner/owned,
        // then fails to unlink inner from the read-only parent on every directory order.
        let protected = directory.join("protected");
        std::fs::create_dir(&protected).unwrap();
        let inner = protected.join("inner");
        std::fs::create_dir(&inner).unwrap();
        std::fs::write(inner.join("owned"), "owned").unwrap();
        let _restore = Restore(protected.clone());
        std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o500)).unwrap();
        let error = remove_files(
            &directory,
            &home.path().join("known.toml"),
            "p",
            PluginsConfig::load_from(home.path().join("plugins.toml")),
        )
        .unwrap_err();
        assert!(
            !inner.join("owned").exists(),
            "the real recursive operation already removed one owned file"
        );
        assert!(inner.exists(), "the recursive operation stopped partway");
        let disposition =
            crate::app::plugin_file_lifecycle::classify_failure(&Err::<(), _>(error.clone()));
        assert!(matches!(
            disposition.outcome,
            crate::app::file_writer::command::CommandOutcome::Unknown
        ));
        assert!(
            !disposition.partial,
            "do not invent an observed worker mutation count"
        );
        assert_eq!(error.saved, Some(false));
        assert!(!error.applied);
        assert_eq!(error.details["removed"], false);
        assert_eq!(error.details["directory_outcome"], "unknown");
        let response = after_disable(error).response_data();
        assert_eq!(response["saved"], false);
        assert_eq!(response["applied"], true);
        assert_eq!(response["disabled"], true);
        assert_eq!(response["partial"], true);
        assert_eq!(response["outcome"], "unknown");
    }
    #[test]
    fn failed_final_config_reports_known_partial_removal_and_protects_original() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("plugin");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("owned"), "fixture").unwrap();
        let path = home.path().join("plugins.toml");
        let before = PluginsConfig::load_from(path.clone());
        std::fs::write(&path, "PRIVATE = [[[\n").unwrap();
        let error = remove_files(&dir, &home.path().join("known.toml"), "p", before).unwrap_err();
        assert!(error.applied);
        assert_eq!(error.saved, Some(false));
        assert_eq!(error.details["removed"], true);
        assert!(!error.message.contains("PRIVATE"));
        assert!(!dir.exists());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "PRIVATE = [[[\n");
    }
    #[cfg(unix)]
    #[test]
    fn changed_remove_path_kind_preserves_link_and_foreign_directory() {
        let home = tempfile::tempdir().unwrap();
        let foreign = home.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("keep"), "private").unwrap();
        let dir = home.path().join("plugin");
        std::os::unix::fs::symlink(&foreign, &dir).unwrap();
        let error = remove_files(
            &dir,
            &home.path().join("known.toml"),
            "p",
            PluginsConfig::load_from(home.path().join("plugins.toml")),
        )
        .unwrap_err();
        assert!(!error.applied);
        assert!(
            std::fs::symlink_metadata(&dir)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(foreign.join("keep")).unwrap(),
            "private"
        );
    }
    #[test]
    fn unknown_after_disable_retains_known_effects_and_unknown_file_outcome() {
        let error = after_disable(Failure {
            message: "unknown".into(),
            saved: None,
            applied: false,
            details: Value::Null,
        });
        assert!(error.applied);
        assert_eq!(error.saved, None);
        assert_eq!(error.details["partial"], true);
        assert_eq!(error.response_data()["outcome"], "unknown");
    }
}
