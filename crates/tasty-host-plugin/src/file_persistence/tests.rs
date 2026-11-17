use super::*;
use crate::{
    known_plugins::KnownPluginEntry,
    registry_state::{PluginsConfig, ShortcutOverride},
};
use std::{sync::mpsc, time::Duration};

fn entry(n: u8) -> KnownPluginEntry {
    KnownPluginEntry {
        pubkey: KnownPluginEntry::encode_pubkey(&[n; 32]),
        trusted_at: "test".into(),
        publisher_fingerprint: String::new(),
    }
}
#[test]
fn independent_config_updates_merge_latest_ids_shortcuts_and_foreign_tables() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    // 옛 `[grants]` 표는 읽지 않지만 저장해도 지우지 않는다.
    std::fs::write(
        &path,
        "[foreign]\nvalue='keep'\n[grants.one]\ngranted=['fs.read']\n",
    )
    .unwrap();
    let a = PluginsConfig::load_from(path.clone());
    let b = a.clone();
    let mut aa = a.clone();
    aa.disable("one");
    let mut bb = b.clone();
    bb.disable("two");
    bb.set_shortcut_override("two", "open", ShortcutOverride::None);
    let update_a = ConfigUpdate::between(&a, &aa).unwrap();
    let update_b = ConfigUpdate::between(&b, &bb).unwrap();
    update_a.execute().unwrap();
    update_b.execute().unwrap();
    let read = PluginsConfig::load_from(path.clone());
    assert!(read.is_disabled("one"));
    assert!(read.is_disabled("two"));
    assert!(matches!(
        read.shortcut_override("two", "open"),
        Some(ShortcutOverride::None)
    ));
    let table = document::read(&path).unwrap();
    assert_eq!(table["foreign"]["value"].as_str(), Some("keep"));
    assert_eq!(
        table["grants"]["one"]["granted"][0].as_str(),
        Some("fs.read")
    );
}
#[test]
fn same_field_external_edit_is_a_conflict_and_invalid_original_stays_protected() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    let before = PluginsConfig::load_from(path.clone());
    let mut after = before.clone();
    after.set_shortcut_override("p", "open", ShortcutOverride::None);
    let update = ConfigUpdate::between(&before, &after).unwrap();
    let external = "[keybindings.p.open]\nmode='key'\nvalue=['F9']\n";
    std::fs::write(&path, external).unwrap();
    assert!(update.execute().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
    std::fs::write(&path, "PRIVATE_CONTENT = [[[\n").unwrap();
    let before = PluginsConfig::load_from(path.clone());
    let mut after = before.clone();
    after.disable("p");
    let error = ConfigUpdate::between(&before, &after).err().unwrap();
    assert!(!error.to_string().contains("PRIVATE_CONTENT"));
    assert!(after.save().is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "PRIVATE_CONTENT = [[[\n"
    );
}
#[test]
fn snapshots_own_their_values_and_wait_outside_the_caller_for_the_common_lock() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    let before = PluginsConfig::load_from(path.clone());
    let mut after = before.clone();
    after.disable("first");
    let update = ConfigUpdate::between(&before, &after).unwrap();
    after.enable("first");
    after.disable("later");
    let lock = tasty_utils::file_replace::lock(&path).unwrap();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || tx.send(update.execute()).unwrap());
    assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
    assert!(after.is_disabled("later"));
    drop(lock);
    rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
    worker.join().unwrap();
    let read = PluginsConfig::load_from(path);
    assert!(read.is_disabled("first"));
    assert!(!read.is_disabled("later"));
}
#[test]
fn trust_updates_and_legacy_observed_saves_do_not_lose_other_callers() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("known-plugins.toml");
    std::fs::write(&path, "[foreign]\nvalue='keep'\n").unwrap();
    let mut a = KnownPlugins::load_from(path.clone()).unwrap();
    let mut b = KnownPlugins::load_from(path.clone()).unwrap();
    a.add("one".into(), entry(1));
    b.add("two".into(), entry(2));
    a.save().unwrap();
    b.save().unwrap();
    update_known(&path, |db| {
        db.add("three".into(), entry(3));
        Ok(())
    })
    .unwrap();
    let read = KnownPlugins::load_from(path.clone()).unwrap();
    assert_eq!(read.plugin_ids().count(), 3);
    assert_eq!(
        document::read(&path).unwrap()["foreign"]["value"].as_str(),
        Some("keep")
    );
    let mut stale = KnownPlugins::load_from(path.clone()).unwrap();
    stale.add("one".into(), entry(4));
    update_known(&path, |db| {
        db.add("one".into(), entry(5));
        Ok(())
    })
    .unwrap();
    assert!(stale.save().is_err());
    assert_eq!(
        KnownPlugins::load_from(path)
            .unwrap()
            .lookup("one")
            .unwrap()
            .pubkey,
        entry(5).pubkey
    );
}
#[test]
fn failed_edit_and_unreadable_original_leave_no_replacement_temp() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("known-plugins.toml");
    std::fs::write(&path, "# original\n").unwrap();
    assert!(
        update_known(&path, |db| {
            db.add("one".into(), entry(1));
            Err::<(), _>(std::io::Error::other("injected"))
        })
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# original\n");
    let directory = home.path().join("directory.toml");
    std::fs::create_dir(&directory).unwrap();
    assert!(update_known(&directory, |_| Ok(())).is_err());
    assert!(
        std::fs::read_dir(home.path()).unwrap().all(|p| !p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
}
#[cfg(unix)]
#[test]
fn aliases_share_updates_and_preserve_symlinks_and_existing_mode() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join("actual.toml");
    let alias = home.path().join("alias.toml");
    std::fs::write(&target, "").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&target, &alias).unwrap();
    update_known(&alias, |db| {
        db.add("one".into(), entry(1));
        Ok(())
    })
    .unwrap();
    update_known(&target, |db| {
        db.add("two".into(), entry(2));
        Ok(())
    })
    .unwrap();
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        KnownPlugins::load_from(alias).unwrap().plugin_ids().count(),
        2
    );
}

#[test]
fn coalesced_snapshots_retain_all_pending_fields_and_can_reverse_an_unsaved_edit() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    let a = PluginsConfig::load_from(path.clone());
    let mut b = a.clone();
    b.disable("one");
    let mut c = b.clone();
    c.disable("two");
    c.enable("one");
    let merged = ConfigUpdate::between(&a, &b)
        .unwrap()
        .coalesce(ConfigUpdate::between(&b, &c).unwrap())
        .unwrap();
    let committed = merged.execute().unwrap();
    assert!(!committed.is_disabled("one"));
    assert!(committed.is_disabled("two"));
    assert!(!PluginsConfig::load_from(path.clone()).is_disabled("one"));
    let unrelated = PluginsConfig::load_from(home.path().join("other.toml"));
    assert!(
        ConfigUpdate::between(&a, &b)
            .unwrap()
            .coalesce(ConfigUpdate::between(&unrelated, &unrelated).unwrap())
            .is_err()
    );
}

#[test]
fn trust_rollback_targets_its_owned_path_and_never_clobbers_a_later_choice() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("known-plugins.toml");
    let undo = crate::trust_verdict::record_trusted_key_in(path.clone(), "one", entry(1)).unwrap();
    update_known(&path, |db| {
        db.add("two".into(), entry(2));
        Ok(())
    })
    .unwrap();
    undo.restore().unwrap();
    let read = KnownPlugins::load_from(path.clone()).unwrap();
    assert!(read.lookup("one").is_none());
    assert!(read.lookup("two").is_some());
    let stale = crate::trust_verdict::record_trusted_key_in(path.clone(), "one", entry(3)).unwrap();
    crate::trust_verdict::record_trusted_key_in(path.clone(), "one", entry(4)).unwrap();
    assert!(stale.restore().is_err());
    assert_eq!(
        KnownPlugins::load_from(path)
            .unwrap()
            .lookup("one")
            .unwrap()
            .pubkey,
        entry(4).pubkey
    );
}

#[test]
fn config_and_trust_edits_preserve_comments_unknown_fields_and_inline_tables() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("plugins.toml");
    std::fs::write(&config,"# heading\n[grants.p] # granted tools\ngranted = ['fs.read'] # caller comment\nfuture = { nested = 7 } # foreign\n\n[foreign]\nvalue = 'keep' # formatting\n").unwrap();
    let before = PluginsConfig::load_from(config.clone());
    let mut after = before.clone();
    after.disable("p");
    ConfigUpdate::between(&before, &after)
        .unwrap()
        .execute()
        .unwrap();
    let text = std::fs::read_to_string(config).unwrap();
    for comment in [
        "# heading",
        "# granted tools",
        "# caller comment",
        "future = { nested = 7 } # foreign",
        "value = 'keep' # formatting",
    ] {
        assert!(text.contains(comment), "{text}");
    }
    let path = home.path().join("known-plugins.toml");
    std::fs::write(&path,"# trust\nunverified = { p = { manifest_sha256 = 'old', cause = 'unsigned', accepted_at = 'then', future = 7 } } # inline\n").unwrap();
    update_known(&path, |db| {
        db.add("other".into(), entry(4));
        Ok(())
    })
    .unwrap();
    update_known(&path, |db| {
        db.accept_unverified(
            "p".into(),
            crate::known_plugins::UnverifiedChoice {
                manifest_sha256: "new".into(),
                cause: "unsigned".into(),
                accepted_at: "now".into(),
            },
        );
        Ok(())
    })
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("new"));
    assert!(text.contains("# trust"));
    assert!(text.contains("future = 7"));
    assert!(text.contains("# inline"));
}

#[test]
fn explicit_commands_override_stale_cache_noops_without_losing_other_actors() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    let cached = PluginsConfig::load_from(path.clone());
    // The command is a no-op in this cache, but not in the latest file.
    let enable = ConfigUpdate::between(&cached, &cached)
        .unwrap()
        .with_enabled_intent("p", true);
    std::fs::write(&path, "# external\n[disabled]\nids=['p','other']\n[grants.p]\ngranted=['fs.read','fs.write']\n[foreign]\nvalue=7\n").unwrap();
    let committed = enable.execute().unwrap();
    assert!(!committed.is_disabled("p"));
    assert!(committed.is_disabled("other"));
    let raw = std::fs::read_to_string(path).unwrap();
    assert!(raw.contains("# external"));
    assert!(raw.contains("value=7"));
    assert!(raw.contains("granted=['fs.read','fs.write']"));
}

#[test]
fn explicit_restore_uses_latest_and_cannot_coalesce() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("plugins.toml");
    let cached = PluginsConfig::load_from(path.clone());
    let plain = || ConfigUpdate::between(&cached, &cached).unwrap();
    let restore = plain().with_restore_builtins_intent(&[], true);
    assert!(restore.clone().coalesce(plain()).is_err());
    assert!(plain().coalesce(restore.clone()).is_err());
    std::fs::write(&path, "[removed_builtins]\nids=['external']\n[grants.p]\ngranted=['fs.write']\n[grants.other]\ngranted=['fs.read']\n").unwrap();
    let config = restore.execute().unwrap();
    assert!(config.removed_builtins.ids.is_empty());
}
