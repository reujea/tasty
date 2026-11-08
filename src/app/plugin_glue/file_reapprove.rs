use crate::{
    app::{App, plugin_file_lifecycle::Failure},
    plugin::Manifest,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tasty_host_plugin::{
    PluginsConfig, file_persistence::ConfigUpdate, known_plugins::KnownPlugins,
};

impl App {
    pub(crate) fn enqueue_plugin_reapprove(
        &mut self,
        id: String,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) -> Result<(), Failure> {
        let root = crate::plugin::plugin_root()
            .ok_or_else(|| Failure::before("plugin directory unavailable"))?;
        let known = KnownPlugins::path()
            .ok_or_else(|| Failure::before("known-plugins.toml path unavailable"))?;
        self.plugin_files.enqueue_sized(
            id.len()
                .saturating_add(root.as_os_str().len())
                .saturating_add(known.as_os_str().len()),
            move |app| {
                let Some(before) = app.plugin_manager.as_ref().map(|mgr| mgr.config.clone()) else {
                    finish(app, Err(Failure::before("plugin manager not initialized")));
                    return;
                };
                let dir = root.join(&id);
                let bytes = crate::app::file_writer::command::value_bytes(&before) + id.len();
                app.run_plugin_file(
                    (bytes, 0),
                    move || reapprove(dir, known, &id, before),
                    |result| match result {
                        Ok((manifest, dir, config)) => {
                            format!("{manifest:?}").len()
                                + crate::app::file_writer::command::value_bytes(config)
                                + dir.as_os_str().len()
                        }
                        Err(e) => e.message.len().saturating_add(
                            crate::app::file_writer::command::value_bytes(&e.details),
                        ),
                    },
                    crate::app::plugin_file_lifecycle::classify_failure,
                    move |app, result| match result.and_then(|result| result) {
                        Err(error) => finish(app, Err(error)),
                        Ok((manifest, dir, config)) => {
                            app.after_plugin_retired(manifest.id.clone(), move |app, retired| {
                                let result =
                                    retired.map_err(Failure::after_file_receipt).and_then(|()| {
                                        app.apply_plugin_registration(manifest, dir, config, false)
                                    });
                                finish(app, result);
                            })
                        }
                    },
                );
            },
        )?;
        (self.services.reply_wake())();
        Ok(())
    }
}

fn reapprove(
    dir: PathBuf,
    known: PathBuf,
    id: &str,
    before: PluginsConfig,
) -> Result<(Manifest, PathBuf, PluginsConfig), Failure> {
    let manifest = Manifest::load(&dir).map_err(|_| Failure::before("invalid plugin manifest"))?;
    if manifest.id != id {
        return Err(Failure::before("plugin manifest identity changed"));
    }
    tasty_host_plugin::official::admit_plugin_id(id).map_err(Failure::before)?;
    let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let undo = tasty_host_plugin::trust_verdict::reapprove_permissions_in(
        known.clone(),
        id,
        manifest.permissions.clone(),
        stamp,
    )
    .map_err(Failure::before)?;
    let result = (|| {
        let db = KnownPlugins::load_from(known).map_err(Failure::before)?;
        if !matches!(
            tasty_host_plugin::trust_verdict::judge_with(&dir, &db),
            tasty_host_plugin::trust_verdict::TrustVerdict::Trusted
                | tasty_host_plugin::trust_verdict::TrustVerdict::AcceptedUnverified
        ) {
            return Err(Failure::before(
                "plugin did not pass discovery after reapproval",
            ));
        }
        let mut after = before.clone();
        after.set_granted(id, manifest.permissions.clone());
        after.enable(id);
        ConfigUpdate::between(&before, &after)
            .map(|update| {
                update
                    .with_grants_intent(id, manifest.permissions.clone())
                    .with_enabled_intent(id, true)
            })
            .and_then(ConfigUpdate::execute)
            .map_err(Failure::before)
    })();
    match result {
        Ok(config) => Ok((manifest, dir, config)),
        Err(mut error) => {
            if let Err(rollback) = undo.restore() {
                error.saved = None;
                error.details = json!({"rollback_failed":true});
                error.message = format!("{}; trust rollback failed: {rollback}", error.message);
            }
            Err(error)
        }
    }
}
