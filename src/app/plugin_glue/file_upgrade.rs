use crate::app::{
    App,
    command::{CoreEvent, PluginRegistryChange},
    plugin_file_lifecycle::Failure,
};
use serde_json::Value;
use std::collections::VecDeque;
use tasty_host_plugin::{
    BuiltinUpgradeAction, BuiltinUpgradeItem, BuiltinUpgradeReport,
    builtin::file_upgrade::{self, Entry, Written},
    manager::{FileSwap, TakenRetry},
};

type Finish = Box<dyn FnOnce(&mut App, Result<Value, Failure>)>;
struct Upgrade {
    entries: VecDeque<Entry>,
    items: Vec<BuiltinUpgradeItem>,
    restart: bool,
    applied: bool,
    /// 지금 쓰는 plugin 에서 넘겨받은 자동 재시작 재시도. 기동하지 않고 끝나면 되돌린다.
    held: Option<(String, TakenRetry)>,
    finish: Finish,
}

impl Upgrade {
    fn fail(mut self, app: &mut App, mut error: Failure) {
        // 결과를 아는 실패는 설치본을 바꾸지 않았다. 결과를 모르면 파일이 바뀌었을 수 있다.
        if let Some((id, retry)) = self.held.take()
            && !error.outcome_unknown()
        {
            app.restore_builtin_retry(&id, retry);
        }
        error.applied |= self.applied;
        if !error.details.is_object() {
            error.details = serde_json::json!({});
        }
        error.details["partial"] = serde_json::json!(self.applied);
        error.details["completed_items"] = serde_json::json!(self.items);
        (self.finish)(app, Err(error));
    }
    fn retained_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(Entry::retained_bytes)
            .fold(
                crate::app::file_writer::command::value_bytes(&self.items),
                usize::saturating_add,
            )
            .saturating_add(std::mem::size_of::<Self>())
    }
}
impl App {
    pub(crate) fn enqueue_builtin_upgrade(
        &mut self,
        force: bool,
        restore: Vec<String>,
        all: bool,
        restart: bool,
        finish: impl FnOnce(&mut App, Result<Value, Failure>) + 'static,
    ) -> Result<(), Failure> {
        let root = crate::plugin::plugin_root()
            .ok_or_else(|| Failure::before("plugin directory unavailable"))?;
        let bundle = tasty_host_plugin::builtin::bundle_root();
        let known = tasty_host_plugin::known_plugins::KnownPlugins::path()
            .ok_or_else(|| Failure::before("known-plugins.toml path unavailable"))?;
        let input_bytes = root
            .as_os_str()
            .len()
            .saturating_add(bundle.as_ref().map_or(0, |path| path.as_os_str().len()))
            .saturating_add(known.as_os_str().len())
            .saturating_add(
                restore
                    .iter()
                    .map(String::len)
                    .fold(0usize, usize::saturating_add),
            );
        self.plugin_files.enqueue_sized(input_bytes,move |app| {
            let Some(config) = app.plugin_manager.as_ref().map(|mgr| mgr.config.clone()) else {
                finish(app, Err(Failure::before("plugin manager unavailable")));
                return;
            };
            let bytes = crate::app::file_writer::command::value_bytes(&config)
                + restore.iter().map(String::len).sum::<usize>();
            app.run_plugin_file(
                (bytes,0),
                move || {
                    file_upgrade::prepare(root, bundle, known, config, force, &restore, all)
                        .map_err(|e| e.to_string())
                },
                |result| match result {
                    Ok(plan) => {
                        crate::app::file_writer::command::value_bytes(&plan.config)
                            + plan
                                .entries
                                .iter()
                                .map(Entry::retained_bytes)
                                .fold(0usize, usize::saturating_add)
                    }
                    Err(error) => error.len(),
                },
                |result| {
                    let mut disposition = crate::app::plugin_file_lifecycle::classify_result(result);
                    if let Ok(plan) = result {
                        let failed = plan.entries.iter().any(|entry| matches!(entry, Entry::Done(item) if matches!(item.action, BuiltinUpgradeAction::Failed { .. })));
                        if failed {
                            disposition.outcome = crate::app::file_writer::command::CommandOutcome::KnownFailure;
                            disposition.partial = plan.restored;
                        }
                    }
                    disposition
                },
                move |app, result| match result.and_then(|result| result.map_err(Failure::before)) {
                    Err(error) => finish(app, Err(error)),
                    Ok(plan) => {
                        let applied = plan.restored;
                        if let Some(mgr) = app.plugin_manager.as_mut() {
                            mgr.config = plan.config;
                        }
                        app.next_builtin_upgrade(Upgrade {
                            entries: plan.entries.into(),
                            items: Vec::new(),
                            restart,
                            applied,
                            held: None,
                            finish: Box::new(finish),
                        });
                    }
                },
            );
        })?;
        (self.services.reply_wake())();
        Ok(())
    }

    fn next_builtin_upgrade(&mut self, mut upgrade: Upgrade) {
        let write = loop {
            match upgrade.entries.pop_front() {
                Some(Entry::Done(item)) => upgrade.items.push(item),
                Some(Entry::Write(write)) => break write,
                None => {
                    let report = BuiltinUpgradeReport {
                        items: upgrade.items,
                    };
                    let result = serde_json::to_value(report).map_err(Failure::before);
                    self.refresh_tool_registry();
                    self.refresh_palette_plugin_commands();
                    self.refresh_plugin_window_after_file();
                    (upgrade.finish)(self, result);
                    return;
                }
            }
        };
        let Some(mgr) = self.plugin_manager.as_mut() else {
            (upgrade.finish)(self, Err(Failure::before("plugin manager unavailable")));
            return;
        };
        let FileSwap {
            swapping: swap,
            restart: pending_restart,
            retry,
        } = mgr.begin_file_swap(&write.id, upgrade.restart && write.stops_running());
        upgrade.applied |= swap;
        upgrade.held = retry.map(|retry| (write.id.clone(), retry));
        let id = write.id.clone();
        self.after_plugin_retired(id, move |app, retired| {
            if let Err(mut error) = retired {
                error.saved = Some(false);
                upgrade.fail(app, error);
                return;
            }
            let Some(config) = app.plugin_manager.as_ref().map(|mgr| mgr.config.clone()) else {
                (upgrade.finish)(app, Err(Failure::before("plugin manager unavailable")));
                return;
            };
            app.run_plugin_file(
                (
                    write
                        .retained_bytes()
                        .saturating_add(crate::app::file_writer::command::value_bytes(&config)),
                    upgrade.retained_bytes(),
                ),
                move || file_upgrade::execute(write, config),
                written_bytes,
                |written| crate::app::file_writer::command::CommandDisposition {
                    outcome: if written.uncertain {
                        crate::app::file_writer::command::CommandOutcome::Unknown
                    } else if matches!(written.item.action, BuiltinUpgradeAction::Failed { .. }) {
                        crate::app::file_writer::command::CommandOutcome::KnownFailure
                    } else {
                        crate::app::file_writer::command::CommandOutcome::KnownSuccess
                    },
                    partial: written.changed
                        && matches!(written.item.action, BuiltinUpgradeAction::Failed { .. }),
                },
                move |app, result| match result {
                    Err(error) => {
                        upgrade.fail(app, error);
                    }
                    Ok(written) => {
                        upgrade.applied |= written.changed;
                        let held = upgrade.held.take().map(|(_, retry)| retry);
                        upgrade.items.push(app.apply_builtin_written(
                            written,
                            swap,
                            pending_restart,
                            held,
                        ));
                        app.next_builtin_upgrade(upgrade);
                    }
                },
            );
        });
    }

    fn apply_builtin_written(
        &mut self,
        mut written: Written,
        swap: bool,
        pending_restart: bool,
        retry: Option<TakenRetry>,
    ) -> BuiltinUpgradeItem {
        let stopping = self.plugin_files.stopping();
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return written.item;
        };
        mgr.config = written.config;
        let validated = written.package.is_some();
        // 쓰기 전에 실패해 설치본이 그대로면 넘겨받은 재시도를 되돌린다. 쓰기 결과가 불확실하거나
        // 새 파일이 검증을 통과하지 못했으면 옛 파일로도 띄우지 않는다.
        let unchanged = !written.changed && !written.uncertain && !validated;
        if unchanged
            && !stopping
            && let Some(retry) = retry
        {
            mgr.restore_respawn_retry(&written.item.id, retry);
        }
        if let Some(package) = written.package {
            mgr.replace_committed_package(&written.item.id, Some(package));
        }
        let (restarted, restart_error) = finish_builtin_restart(
            stopping,
            written.uncertain || !validated,
            swap,
            pending_restart,
            || mgr.finish_file_swap(&written.item.id),
        );
        match &mut written.item.action {
            BuiltinUpgradeAction::Upgraded {
                was_restarted,
                restart_error: error,
                ..
            }
            | BuiltinUpgradeAction::Reinstalled {
                was_restarted,
                restart_error: error,
                ..
            } => {
                *was_restarted = restarted;
                *error = restart_error;
            }
            _ => {
                if let Some(error) = restart_error {
                    tracing::warn!(plugin=%written.item.id,error,"plugin restart after file operation failed");
                }
            }
        }
        let version = match &written.item.action {
            BuiltinUpgradeAction::Upgraded { to, .. } => Some(to.clone()),
            BuiltinUpgradeAction::Reinstalled { version, .. } => Some(version.clone()),
            _ => None,
        };
        if written.changed
            && let Some(version) = version
        {
            self.cascade_plugin_events(vec![CoreEvent::PluginRegistryChanged {
                plugin_id: written.item.id.clone(),
                change: PluginRegistryChange::Installed { version },
            }]);
        }
        written.item
    }
}

impl App {
    /// 넘겨받은 재시도를 manager 에 되돌린다. 종료 중이면 띄울 일이 없으므로 버린다.
    fn restore_builtin_retry(&mut self, id: &str, retry: TakenRetry) {
        if self.plugin_files.stopping() {
            return;
        }
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.restore_respawn_retry(id, retry);
        }
    }
}

fn written_bytes(written: &Written) -> usize {
    crate::app::file_writer::command::value_bytes(&written.config)
        .saturating_add(crate::app::file_writer::command::value_bytes(&written.item))
        .saturating_add(written.package.as_ref().map_or(0, |package| {
            format!("{:?}", package.manifest)
                .len()
                .saturating_add(package.dir.as_os_str().len())
        }))
}

fn finish_builtin_restart(
    stopping: bool,
    uncertain: bool,
    swap: bool,
    pending_restart: bool,
    restart: impl FnOnce() -> anyhow::Result<bool>,
) -> (bool, Option<String>) {
    if uncertain || stopping || !(swap || pending_restart) {
        return (false, None);
    }
    match restart() {
        Ok(restarted) => (restarted, None),
        Err(error) => (false, Some(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_report_tracks_actual_success_for_closing_swap_and_pending_combinations() {
        // stopping, uncertain, swap, pending restart, actual successful restart expected
        let cases = [
            (false, false, false, false, false),
            (false, false, false, true, true),
            (false, false, true, false, true),
            (false, false, true, true, true),
            (true, false, false, false, false),
            (true, false, false, true, false),
            (true, false, true, false, false),
            (true, false, true, true, false),
            (false, true, false, false, false),
            (false, true, false, true, false),
            (false, true, true, false, false),
            (false, true, true, true, false),
            (true, true, false, false, false),
            (true, true, false, true, false),
            (true, true, true, false, false),
            (true, true, true, true, false),
        ];
        for (stopping, uncertain, swap, pending, expected) in cases {
            let mut calls = 0;
            let (restarted, error) =
                finish_builtin_restart(stopping, uncertain, swap, pending, || {
                    calls += 1;
                    Ok(true)
                });
            assert_eq!(
                calls,
                usize::from(expected),
                "actual call: {stopping}/{uncertain}/{swap}/{pending}"
            );
            assert_eq!(
                restarted, expected,
                "reported restart: {stopping}/{uncertain}/{swap}/{pending}"
            );
            assert!(error.is_none());
        }
    }
    #[test]
    fn a_closing_host_never_claims_a_skipped_swap_restart() {
        let (restarted, error) = finish_builtin_restart(true, false, true, false, || {
            panic!("restart during shutdown")
        });
        assert!(!restarted);
        assert!(error.is_none());
    }
    #[test]
    fn a_successful_noop_is_not_reported_as_a_restart() {
        assert_eq!(
            finish_builtin_restart(false, false, true, false, || Ok(false)),
            (false, None)
        );
    }
    #[test]
    fn a_failed_actual_restart_is_not_reported_successful() {
        for (swap, pending) in [(true, false), (false, true), (true, true)] {
            let mut calls = 0;
            let (restarted, error) = finish_builtin_restart(false, false, swap, pending, || {
                calls += 1;
                anyhow::bail!("owned restart failure")
            });
            assert_eq!(calls, 1);
            assert!(!restarted);
            assert_eq!(error.as_deref(), Some("owned restart failure"));
        }
    }
    #[test]
    fn written_output_weigh_includes_owned_manifest_and_package_path() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("tasty-plugin.toml"),"manifest_version=1\nid='com.example.weigh'\nname='owned'\nversion='1.0.0'\napi_version='1'\n[entry]\ntype='process'\ncommand='unused'\n").unwrap();
        let mut manifest = crate::plugin::Manifest::load(home.path()).unwrap();
        manifest.name = "x".repeat(1024 * 1024);
        let expected = manifest.name.len() + home.path().as_os_str().len();
        let written = Written {
            item: BuiltinUpgradeItem {
                id: manifest.id.clone(),
                action: BuiltinUpgradeAction::Skipped {
                    installed_version: None,
                    bundle_version: None,
                    reason: "owned weigh fixture".into(),
                },
            },
            package: Some(crate::plugin::PluginPackage {
                dir: home.path().to_path_buf(),
                manifest,
            }),
            config: tasty_host_plugin::PluginsConfig::default(),
            changed: true,
            uncertain: false,
        };
        assert!(
            written_bytes(&written) >= expected,
            "package payload omitted from output weigh"
        );
    }
}

// X11 이벤트 루프로 App 을 세우는 시험이라 Linux 에서만 컴파일한다(preset_shutdown_tests 와 같다).
#[cfg(all(test, target_os = "linux"))]
#[path = "file_upgrade_retry_tests.rs"]
mod retry_tests;
