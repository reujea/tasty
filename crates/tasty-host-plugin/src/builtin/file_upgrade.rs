//! Owned file half of runtime builtin upgrades. Live generations remain on the host loop.
mod source;

use super::*;
use crate::file_persistence::ConfigUpdate;

pub struct Plan {
    pub config: PluginsConfig,
    pub entries: Vec<Entry>,
    pub restored: bool,
}
impl Entry {
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::Done(item) => serde_json::to_vec(item).map_or(usize::MAX, |value| value.len()),
            Self::Write(write) => write.retained_bytes(),
        }
    }
}
pub enum Entry {
    Done(BuiltinUpgradeItem),
    Write(Write),
}
pub struct Write {
    pub id: String,
    source: PathBuf,
    destination: PathBuf,
    kind: Kind,
    action: BuiltinUpgradeAction,
    prepared: Box<source::PreparedSource>,
}
enum Kind {
    Install,
    Content,
    Overwrite,
}
impl Write {
    pub fn stops_running(&self) -> bool {
        matches!(self.kind, Kind::Overwrite)
    }
    pub fn retained_bytes(&self) -> usize {
        self.id
            .len()
            .saturating_add(self.source.as_os_str().len())
            .saturating_add(self.destination.as_os_str().len())
            .saturating_add(
                serde_json::to_vec(&self.action).map_or(usize::MAX, |value| value.len()),
            )
            .saturating_add(self.prepared.retained_bytes())
            .saturating_add(std::mem::size_of::<Self>())
    }
}
pub struct Written {
    pub item: BuiltinUpgradeItem,
    pub package: Option<crate::PluginPackage>,
    pub config: PluginsConfig,
    pub changed: bool,
    /// A directory helper failed after entering a potentially mutating operation.
    pub uncertain: bool,
}

/// Capture paths before entering the worker; no worker-thread home resolution is needed.
pub fn prepare(
    root: PathBuf,
    bundle: Option<PathBuf>,
    known: PathBuf,
    before: PluginsConfig,
    force: bool,
    restore: &[String],
    restore_all: bool,
) -> std::io::Result<Plan> {
    let known_path = known;
    let known = crate::known_plugins::KnownPlugins::load_from(known_path.clone())?;
    let mut after = before.clone();
    if restore_all {
        after.clear_removed_builtins();
    } else {
        for id in restore {
            after.unmark_builtin_removed(id);
        }
    }
    let (config, restored) = ConfigUpdate::between(&before, &after)?
        .with_restore_builtins_intent(restore, restore_all)
        .execute_with_change()?;
    let entries = BUILTINS
        .iter()
        .map(|spec| {
            plan_entry(
                spec,
                &root,
                bundle.as_deref(),
                (&known_path, &known),
                &config,
                force,
            )
        })
        .collect();
    Ok(Plan {
        config,
        entries,
        restored,
    })
}

fn plan_entry(
    spec: &BuiltinSpec,
    root: &Path,
    bundle: Option<&Path>,
    observation: (&Path, &crate::known_plugins::KnownPlugins),
    config: &PluginsConfig,
    force: bool,
) -> Entry {
    let dest = root.join(spec.id);
    if config.is_builtin_removed(spec.id) {
        return Entry::Done(BuiltinUpgradeItem {
            id: spec.id.into(),
            action: BuiltinUpgradeAction::Skipped {
                installed_version: read_installed_version(&dest).map(|v| v.to_string()),
                bundle_version: None,
                reason: "user-removed".into(),
            },
        });
    }
    let Some(source) = bundle.map(|b| b.join(spec.id)).filter(|p| p.is_dir()) else {
        return Entry::Done(not_in_bundle_item(spec));
    };
    let (known_path, known) = observation;
    let prepared = match source::PreparedSource::capture(&source, known_path, known, spec.id) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Entry::Done(BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Failed {
                    reason: error.to_string(),
                },
            });
        }
    };
    if let Err(item) = verify_trust(spec, &source, &dest, known) {
        return Entry::Done(item);
    }
    let installed = read_installed_version(&dest);
    let version = read_bundle_version(&source);
    let new_version = version.as_ref().map(|v| v.to_string()).unwrap_or_default();
    let (kind, action) = if !dest.exists() {
        (
            Kind::Install,
            BuiltinUpgradeAction::Reinstalled {
                version: new_version,
                was_restarted: false,
                restart_error: None,
            },
        )
    } else {
        match decide_builtin_upgrade(installed.as_ref(), version.as_ref(), force) {
            BuiltinUpgradeDecision::Skip => {
                return Entry::Done(BuiltinUpgradeItem {
                    id: spec.id.into(),
                    action: BuiltinUpgradeAction::Skipped {
                        installed_version: installed.map(|v| v.to_string()),
                        bundle_version: version.map(|v| v.to_string()),
                        reason: "installed >= bundle".into(),
                    },
                });
            }
            BuiltinUpgradeDecision::ResyncSameVersion => {
                let action = BuiltinUpgradeAction::Skipped {
                    installed_version: installed.map(|v| v.to_string()),
                    bundle_version: version.map(|v| v.to_string()),
                    reason: if sync_probe::sync_would_touch(&source, &dest) {
                        "same-version (content resync: files rewritten)"
                    } else {
                        return Entry::Done(BuiltinUpgradeItem {
                            id: spec.id.into(),
                            action: BuiltinUpgradeAction::Skipped {
                                installed_version: read_installed_version(&dest)
                                    .map(|v| v.to_string()),
                                bundle_version: read_bundle_version(&source).map(|v| v.to_string()),
                                reason: "same-version (content resync: nothing to write)".into(),
                            },
                        });
                    }
                    .into(),
                };
                (Kind::Content, action)
            }
            BuiltinUpgradeDecision::UpgradeVersion { from, to } => (
                Kind::Overwrite,
                BuiltinUpgradeAction::Upgraded {
                    from: from.to_string(),
                    to: to.to_string(),
                    was_restarted: false,
                    restart_error: None,
                },
            ),
            BuiltinUpgradeDecision::ForceOverwrite => (
                Kind::Overwrite,
                BuiltinUpgradeAction::Reinstalled {
                    version: new_version,
                    was_restarted: false,
                    restart_error: None,
                },
            ),
        }
    };
    Entry::Write(Write {
        id: spec.id.into(),
        source,
        destination: dest,
        kind,
        action,
        prepared: Box::new(prepared),
    })
}

/// Failed file/config work remains a per-plugin failure, retaining any known file mutation.
pub fn execute(write: Write, before: PluginsConfig) -> Written {
    execute_inner(write, before, || {})
}
fn execute_inner(write: Write, before: PluginsConfig, after_copy: impl FnOnce()) -> Written {
    let mut changed = false;
    let mut uncertain = false;
    let mut package = None;
    let mut config = before.clone();
    let result = (|| {
        validate_prepared(&write, &write.source)?;
        let parent = write
            .destination
            .parent()
            .ok_or_else(|| std::io::Error::other("plugin directory unavailable"))?;
        std::fs::create_dir_all(parent)?;
        if let Ok(metadata) = std::fs::symlink_metadata(&write.destination)
            && (!metadata.is_dir() || metadata.file_type().is_symlink())
        {
            return Err(std::io::Error::other(
                "plugin destination changed path kind",
            ));
        }
        match write.kind {
            Kind::Install => {
                std::fs::create_dir(&write.destination)?;
                changed = true;
                copy_dir_recursive(&write.source, &write.destination)
                    .inspect_err(|_| uncertain = true)?;
            }
            Kind::Content => {
                changed = sync_dir_by_content(&write.source, &write.destination)
                    .inspect_err(|_| uncertain = true)?;
            }
            Kind::Overwrite => {
                overwrite_builtin_dir(&write.source, &write.destination)
                    .inspect_err(|_| uncertain = true)?;
                changed = true;
            }
        }
        after_copy();
        let manifest = validate_prepared(&write, &write.destination)?;
        let mut after = before.clone();
        for token in &manifest.permissions {
            after.grant(&write.id, token);
        }
        let mut update = ConfigUpdate::between(&before, &after)?;
        for token in &manifest.permissions {
            update = update.with_permission_intent(&write.id, token, true);
        }
        config = update.execute()?;
        package = Some(crate::PluginPackage {
            dir: write.destination.clone(),
            manifest,
        });
        Ok::<(), std::io::Error>(())
    })();
    let action = match result {
        Ok(()) => write.action,
        Err(error) => BuiltinUpgradeAction::Failed {
            reason: if uncertain {
                format!("{error}; directory outcome unknown; automatic restart suppressed")
            } else {
                error.to_string()
            },
        },
    };
    Written {
        item: BuiltinUpgradeItem {
            id: write.id,
            action,
        },
        package,
        config,
        changed,
        uncertain,
    }
}

fn validate_prepared(write: &Write, copied: &Path) -> std::io::Result<Manifest> {
    let known = write.prepared.validate(&write.source, &write.id)?;
    write.prepared.validate_manifest(copied)?;
    let spec = BUILTINS
        .iter()
        .find(|spec| spec.id == write.id)
        .ok_or_else(|| std::io::Error::other("prepared builtin identity unavailable"))?;
    let manifest = Manifest::load(copied)
        .map_err(|_| std::io::Error::other("prepared builtin manifest unavailable"))?;
    if manifest.id != write.id {
        return Err(std::io::Error::other("prepared builtin identity changed"));
    }
    verify_trust(spec, copied, &write.destination, &known)
        .map_err(|_| std::io::Error::other("prepared builtin trust verification failed"))?;
    Ok(manifest)
}

pub(super) fn verify_trust(
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
    known: &crate::known_plugins::KnownPlugins,
) -> Result<(), BuiltinUpgradeItem> {
    // 폴더 이름과 다른 id를 선언한 번들은 서명과 관계없이 복사하지 않는다(모든 빌드).
    if let Some(reason) = crate::official::bundle_identity_mismatch(spec.id, src) {
        return Err(BuiltinUpgradeItem {
            id: spec.id.into(),
            action: BuiltinUpgradeAction::Failed { reason },
        });
    }
    #[cfg(not(debug_assertions))]
    {
        use crate::bundle_sig::TrustDecision;
        match crate::bundle_sig::verify_bundle_signature_with(src, known) {
            Ok(TrustDecision::Trusted) => Ok(()),
            Ok(TrustDecision::Untrusted {
                plugin_id,
                fingerprint,
                reason,
                ..
            }) => {
                // builtin 업데이트는 신뢰되지 않은 서명을 거절한다. 여기서 승인 UI를 열지 않는다.
                tracing::warn!(
                    "builtin '{}' is untrusted (reason: {reason:?}, fp: {fingerprint})",
                    plugin_id
                );
                Err(BuiltinUpgradeItem {
                    id: spec.id.into(),
                    action: BuiltinUpgradeAction::Skipped {
                        installed_version: read_installed_version(dest).map(|v| v.to_string()),
                        bundle_version: None,
                        reason: format!("untrusted: {reason:?}"),
                    },
                })
            }
            Err(e) => {
                tracing::warn!("builtin '{}' bundle signature check failed: {e}", spec.id);
                Err(BuiltinUpgradeItem {
                    id: spec.id.into(),
                    action: BuiltinUpgradeAction::Skipped {
                        installed_version: read_installed_version(dest).map(|v| v.to_string()),
                        bundle_version: None,
                        reason: format!("signature-invalid: {e}"),
                    },
                })
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        // debug 빌드는 서명 검증 결과를 로그로만 남기고 설치를 건너뛰지 않으므로
        // 설치 대상 경로를 읽을 일이 없다 — release 갈래에서만 쓰인다.
        let _ = dest;
        match crate::bundle_sig::verify_bundle_signature_with(src, known) {
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(
                    "dev bundle '{}' signature check: {e} (debug build = ignored)",
                    spec.id
                );
            }
        }
        Ok(())
    }
}

/// Boot/synchronous callers publish only the latest receipt-confirmed grants and flags.
pub(super) fn persist_boot_config(
    mgr: &mut PluginManager,
    before: PluginsConfig,
) -> std::io::Result<()> {
    match crate::file_persistence::ConfigUpdate::between(&before, &mgr.config)
        .and_then(crate::file_persistence::ConfigUpdate::execute)
    {
        Ok(committed) => {
            mgr.config = committed;
            mgr.recompute_extensions();
            Ok(())
        }
        Err(error) => {
            mgr.config = before;
            mgr.recompute_extensions();
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) {
        let dir = root.join("com.tasty.html");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("tasty-plugin.toml"),
            r#"
manifest_version=1
id="com.tasty.html"
name="test"
version="1.0.0"
api_version="1"
permissions=["fs.read"]
[entry]
type="process"
command="unused"
"#,
        )
        .unwrap();
        std::fs::write(dir.join("unused"), "owned fixture").unwrap();
    }
    fn retirement_gap_plan(home: &Path) -> (Write, PluginsConfig) {
        let bundle = home.join("bundle");
        fixture(&bundle);
        let plan = prepare(
            home.join("installed"),
            Some(bundle),
            home.join("known.toml"),
            PluginsConfig::load_from(home.join("plugins.toml")),
            false,
            &[],
            false,
        )
        .unwrap();
        let write = plan
            .entries
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Write(write) if write.id == "com.tasty.html" => Some(write),
                _ => None,
            })
            .unwrap();
        (write, plan.config)
    }
    #[test]
    fn retirement_gap_source_manifest_change_cannot_publish_new_grants_or_package() {
        let home = tempfile::tempdir().unwrap();
        let (write, config) = retirement_gap_plan(home.path());
        let path = home.path().join("bundle/com.tasty.html/tasty-plugin.toml");
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("fs.read", "fs.write");
        std::fs::write(path, text).unwrap();
        let written = execute(write, config);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(written.package.is_none());
        assert!(!written.changed);
        assert!(!written.uncertain);
        assert!(
            !written
                .config
                .granted_permissions("com.tasty.html")
                .contains("fs.write")
        );
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }
    #[test]
    fn retirement_gap_same_plugin_trust_change_cannot_publish_package() {
        let home = tempfile::tempdir().unwrap();
        let (write, config) = retirement_gap_plan(home.path());
        crate::file_persistence::update_known(&home.path().join("known.toml"), |known| {
            known.add(
                "com.tasty.html".into(),
                crate::known_plugins::KnownPluginEntry {
                    pubkey: crate::known_plugins::KnownPluginEntry::encode_pubkey(&[7; 32]),
                    permissions: vec!["fs.read".into()],
                    trusted_at: "owned fixture".into(),
                    publisher_fingerprint: String::new(),
                },
            );
            Ok(())
        })
        .unwrap();
        let written = execute(write, config);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(written.package.is_none());
        assert!(!written.changed);
        assert!(!written.uncertain);
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }
    #[test]
    fn retirement_gap_signature_change_cannot_publish_package() {
        let home = tempfile::tempdir().unwrap();
        let (write, config) = retirement_gap_plan(home.path());
        std::fs::write(
            home.path()
                .join("bundle/com.tasty.html/tasty-plugin.toml.sig"),
            [0; 64],
        )
        .unwrap();
        let written = execute(write, config);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(!written.changed);
        assert!(written.package.is_none());
    }
    #[test]
    fn retirement_gap_unrelated_trust_and_unsigned_binary_changes_keep_existing_policy() {
        let home = tempfile::tempdir().unwrap();
        let (write, config) = retirement_gap_plan(home.path());
        std::fs::write(home.path().join("known.toml"), "[foreign]\nrevision=2\n").unwrap();
        std::fs::write(
            home.path().join("bundle/com.tasty.html/unused"),
            "binary outside manifest signature scope",
        )
        .unwrap();
        let written = execute(write, config);
        assert!(written.package.is_some());
        assert!(!written.uncertain);
    }
    #[test]
    fn retirement_gap_post_copy_manifest_change_blocks_grants_and_package() {
        let home = tempfile::tempdir().unwrap();
        let (write, config) = retirement_gap_plan(home.path());
        let path = home
            .path()
            .join("installed/com.tasty.html/tasty-plugin.toml");
        let written = execute_inner(write, config, move || {
            let text = std::fs::read_to_string(&path)
                .unwrap()
                .replace("fs.read", "fs.write");
            std::fs::write(path, text).unwrap();
        });
        assert!(written.changed);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(written.package.is_none());
        assert!(
            written
                .config
                .granted_permissions("com.tasty.html")
                .is_empty()
        );
    }
    #[test]
    fn stale_asset_unlink_is_observed_but_recursive_directory_cleanup_is_not() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join("source");
        let destination = home.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("stale"), "owned fixture").unwrap();
        let metrics =
            std::sync::Arc::new(tasty_utils::file_replace::FileOperationMetrics::default());
        let _scope = tasty_utils::file_replace::observe_operations(metrics.clone());
        prune_dest_not_in_src(&source, &destination).unwrap();
        assert!(!destination.join("stale").exists());
        std::fs::remove_dir_all(&destination).unwrap();
        let counts = metrics.snapshot();
        assert_eq!(counts.delete.attempts, 1);
        assert_eq!(counts.delete.succeeded, 1);
        assert_eq!(counts.replace.attempts, 0);
    }
    #[test]
    fn protected_trust_failure_precedes_restore_and_latest_restore_is_observed() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("plugins.toml");
        let known = home.path().join("known.toml");
        let config = PluginsConfig::load_from(path.clone());
        let external = "# external\n[removed_builtins]\nids=['com.tasty.html']\n";
        std::fs::write(&path, external).unwrap();
        std::fs::write(&known, "PRIVATE = [[[\n").unwrap();
        let request = || {
            prepare(
                home.path().join("plugins"),
                None,
                known.clone(),
                config.clone(),
                false,
                &[],
                true,
            )
        };
        let error = request().err().unwrap();
        assert!(!error.to_string().contains("PRIVATE"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
        std::fs::write(&known, "").unwrap();
        let plan = request().unwrap();
        assert!(plan.restored);
        assert!(plan.config.removed_builtins.ids.is_empty());
        assert!(!request().unwrap().restored);
    }
    #[test]
    fn directory_helper_failure_is_unknown_without_inventing_known_mutations() {
        let home = tempfile::tempdir().unwrap();
        let destination = home.path().join("installed");
        std::fs::create_dir(&destination).unwrap();
        let bundle = home.path().join("bundle");
        fixture(&bundle);
        std::fs::create_dir(destination.join("unused")).unwrap();
        let known_path = home.path().join("known.toml");
        let prepared = source::PreparedSource::capture(
            &bundle.join("com.tasty.html"),
            &known_path,
            &crate::known_plugins::KnownPlugins::default(),
            "com.tasty.html",
        )
        .unwrap();
        let write = Write {
            id: "com.tasty.html".into(),
            source: bundle.join("com.tasty.html"),
            destination,
            kind: Kind::Content,
            action: BuiltinUpgradeAction::Reinstalled {
                version: "1".into(),
                was_restarted: false,
                restart_error: None,
            },
            prepared: Box::new(prepared),
        };
        let written = execute(
            write,
            PluginsConfig::load_from(home.path().join("plugins.toml")),
        );
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(written.uncertain);
        assert!(!written.changed);
        assert!(written.package.is_none());
    }
    #[test]
    fn runtime_file_upgrade_has_no_live_manager_and_noop_plan_has_no_write() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("installed");
        let bundle = home.path().join("bundle");
        fixture(&bundle);
        let path = home.path().join("plugins.toml");
        let known = home.path().join("known-plugins.toml");
        let before = PluginsConfig::load_from(path.clone());
        let plan = prepare(
            root.clone(),
            Some(bundle.clone()),
            known.clone(),
            before,
            false,
            &[],
            false,
        )
        .unwrap();
        assert!(!root.exists());
        assert!(!path.exists());
        let write = plan
            .entries
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Write(write) if write.id == "com.tasty.html" => Some(write),
                _ => None,
            })
            .unwrap();
        let written = execute(write, plan.config);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Reinstalled { .. }
        ));
        assert!(written.changed);
        assert!(written.package.is_some());
        assert!(
            written
                .config
                .granted_permissions("com.tasty.html")
                .contains("fs.read")
        );
        let bytes = std::fs::read(&path).unwrap();
        let plan = prepare(root, Some(bundle), known, written.config, false, &[], false).unwrap();
        assert!(
            plan.entries
                .iter()
                .all(|entry| matches!(entry, Entry::Done(_)))
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn boot_barrier_keeps_failed_grants_out_of_live_config_and_imports_latest_flags() {
        let home = crate::test_support::HomeEnvGuard::tasty_home();
        let mut mgr = PluginManager::new(std::sync::Arc::new(
            tasty_terminal::waker_factory::NoopWakerFactory,
        ));
        let before = mgr.config.clone();
        mgr.config.grant("p", "fs.read");
        std::fs::write(home.path().join("plugins.toml"), "PRIVATE = [[[\n").unwrap();
        assert!(persist_boot_config(&mut mgr, before).is_err());
        assert!(mgr.config.granted_permissions("p").is_empty());
        std::fs::write(
            home.path().join("plugins.toml"),
            "[disabled]\nids=['external']\n",
        )
        .unwrap();
        let before = mgr.config.clone();
        mgr.config.grant("p", "fs.read");
        persist_boot_config(&mut mgr, before).unwrap();
        assert!(mgr.config.is_disabled("external"));
        assert!(mgr.config.granted_permissions("p").contains("fs.read"));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_file_upgrade_refuses_a_changed_destination_link() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("installed");
        let bundle = home.path().join("bundle");
        fixture(&bundle);
        let plan = prepare(
            root.clone(),
            Some(bundle),
            home.path().join("known.toml"),
            PluginsConfig::load_from(home.path().join("plugins.toml")),
            false,
            &[],
            false,
        )
        .unwrap();
        let write = plan
            .entries
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Write(write) => Some(write),
                _ => None,
            })
            .unwrap();
        let foreign = home.path().join("foreign");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("keep"), "private").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&foreign, root.join("com.tasty.html")).unwrap();
        let written = execute(write, plan.config);
        assert!(matches!(
            written.item.action,
            BuiltinUpgradeAction::Failed { .. }
        ));
        assert!(!written.changed);
        assert_eq!(
            std::fs::read_to_string(foreign.join("keep")).unwrap(),
            "private"
        );
    }
}
