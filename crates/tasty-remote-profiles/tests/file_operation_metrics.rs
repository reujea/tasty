//! Actual backend operations inside the same bounded observation scope used by FileExecutor.
use std::{fs, sync::Arc};
#[cfg(unix)]
use tasty_doc_guards::floored_walk::{CountedOn, Floor, entries_with_floor};
use tasty_remote_profiles::{
    ManagedCleanup, MutationStage, Passkey, PasskeyCommand, Passkeys, RemoteFileStore,
};
use tasty_utils::file_replace::{FileOperationMetrics, observe_operations};

fn inline(name: &str) -> PasskeyCommand {
    PasskeyCommand::Inline {
        name: name.into(),
        secret: "owned-fixture-value".into(),
    }
}
fn metrics() -> Arc<FileOperationMetrics> {
    Arc::new(FileOperationMetrics::default())
}

#[test]
fn private_secret_list_and_user_unlink_have_separate_exact_physical_counts() {
    let root = tempfile::tempdir().unwrap();
    let store = RemoteFileStore::at(root.path());
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    let saved = store.passkey(inline("key")).unwrap();
    assert!(saved.secret_written.is_some());
    let key = root.path().join("passkeys/key");
    let list = root.path().join("passkeys.toml");
    let bytes = fs::metadata(&key).unwrap().len() + fs::metadata(&list).unwrap().len();
    let first = counts.snapshot();
    assert_eq!(first.replace.attempts, 2);
    assert_eq!(first.replace.succeeded, 2);
    assert_eq!(first.replace.payload_bytes, bytes);
    assert_eq!(first.replace.published_bytes, bytes);
    assert_eq!(first.delete.attempts, 0);
    let removed = store
        .passkey(PasskeyCommand::Remove { name: "key".into() })
        .unwrap();
    assert!(removed.removed);
    assert_eq!(removed.cleanup, ManagedCleanup::Removed);
    let final_counts = counts.snapshot();
    assert_eq!(final_counts.replace.succeeded, 3);
    assert_eq!(
        final_counts.replace.published_bytes,
        bytes + fs::metadata(list).unwrap().len()
    );
    assert_eq!(final_counts.delete.attempts, 1);
    assert_eq!(final_counts.delete.succeeded, 1);
    assert!(!key.exists());
}

#[cfg(unix)]
fn private_temp_failure_fixture(inject: impl FnOnce(&std::path::Path)) {
    let root = tempfile::tempdir().unwrap();
    let secrets = root.path().join("passkeys");
    fs::create_dir(&secrets).unwrap();
    // The destination and its .lock fit NAME_MAX=255; the private temp suffix does not.
    let name = "x".repeat(250);
    let original = secrets.join(&name);
    fs::write(&original, b"preserved fixture").unwrap();
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    let error = RemoteFileStore::at(root.path())
        .passkey(inline(&name))
        .unwrap_err();
    assert_eq!(error.stage, MutationStage::SecretWrite);
    assert!(error.secret_written.is_none());
    let snapshot = counts.snapshot();
    assert_eq!(snapshot.replace.attempts, 1);
    assert_eq!(snapshot.replace.failed, 1);
    assert_eq!(snapshot.replace.succeeded, 0);
    assert_eq!(snapshot.replace.unknown, 0);
    assert_eq!(snapshot.replace.published_bytes, 0);
    assert_eq!(snapshot.replace.errors[4], 1);
    assert_eq!(snapshot.delete.attempts, 0);
    assert_eq!(fs::read(&original).unwrap(), b"preserved fixture");
    inject(&secrets);
    let files = entries_with_floor(
        &secrets,
        &secrets,
        &Floor {
            min: 2,
            measured: 2,
            measured_on: "2026-10-24",
            counted_on: CountedOn::SyntheticTree,
            why_this_gap: "The owned fixture contains exactly the preserved secret and its participant lock; no missing file is acceptable.",
        },
        2,
    )
    .unwrap();
    assert_eq!(files.len(), 2);
    let names: std::collections::BTreeSet<_> = files.iter().map(|file| file.rel.as_str()).collect();
    assert_eq!(names, [name.as_str(), &format!("{name}.lock")].into());
}

#[cfg(unix)]
#[test]
fn private_temp_creation_failure_is_an_observed_failed_attempt_without_user_unlink() {
    private_temp_failure_fixture(|_| {});
}

#[cfg(unix)]
#[test]
#[should_panic(expected = "direct-entry limit")]
fn private_temp_failure_rejects_an_extra_residual_directory() {
    private_temp_failure_fixture(|secrets| {
        fs::create_dir(secrets.join("unexpected-residual-directory")).unwrap();
    });
}

#[cfg(unix)]
#[test]
#[should_panic(expected = "direct-entry limit")]
fn private_temp_failure_rejects_an_extra_residual_symlink() {
    private_temp_failure_fixture(|secrets| {
        std::os::unix::fs::symlink("missing-target", secrets.join("unexpected-residual-link"))
            .unwrap();
    });
}

#[test]
fn malformed_registry_refusal_never_fabricates_a_secret_replacement() {
    let root = tempfile::tempdir().unwrap();
    let list = root.path().join("passkeys.toml");
    fs::write(&list, b"broken=[").unwrap();
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    let error = RemoteFileStore::at(root.path())
        .passkey(inline("key"))
        .unwrap_err();
    assert_eq!(error.stage, MutationStage::PasskeyRead);
    assert!(error.secret_written.is_none());
    assert_eq!(counts.snapshot().replace.attempts, 0);
    assert_eq!(counts.snapshot().delete.attempts, 0);
    assert_eq!(fs::read(list).unwrap(), b"broken=[");
    assert!(!root.path().join("passkeys/key").exists());
}

#[test]
fn managed_cleanup_failure_is_measured_after_the_known_list_commit() {
    let root = tempfile::tempdir().unwrap();
    let store = RemoteFileStore::at(root.path());
    store.passkey(inline("key")).unwrap();
    let key = root.path().join("passkeys/key");
    fs::remove_file(&key).unwrap();
    fs::create_dir(&key).unwrap();
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    let result = store
        .passkey(PasskeyCommand::Remove { name: "key".into() })
        .unwrap();
    assert!(result.removed);
    assert!(matches!(result.cleanup, ManagedCleanup::Failed(_)));
    let snapshot = counts.snapshot();
    assert_eq!(snapshot.replace.succeeded, 1);
    assert_eq!(snapshot.delete.attempts, 1);
    assert_eq!(snapshot.delete.failed, 1);
    assert_eq!(snapshot.delete.succeeded, 0);
    assert!(key.is_dir());
    let list: Passkeys =
        toml::from_str(&fs::read_to_string(root.path().join("passkeys.toml")).unwrap()).unwrap();
    assert!(list.passkeys.is_empty());
}

#[cfg(unix)]
#[test]
fn secret_success_then_list_failure_keeps_typed_partial_fields_and_exact_metrics() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let store = RemoteFileStore::at(root.path());
    store
        .passkey(PasskeyCommand::Path {
            name: "old".into(),
            path: "unused".into(),
        })
        .unwrap();
    let list = root.path().join("passkeys.toml");
    let original = fs::read(&list).unwrap();
    fs::set_permissions(&list, fs::Permissions::from_mode(0o400)).unwrap();
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    let result = store.passkey(inline("orphan"));
    // Restore fixture permissions before inspecting the expected failure.
    fs::set_permissions(&list, fs::Permissions::from_mode(0o600)).unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.stage, MutationStage::PasskeySave);
    assert!(error.secret_written.is_some());
    assert!(!error.passkeys_saved);
    assert_eq!(fs::read(list).unwrap(), original);
    let key = root.path().join("passkeys/orphan");
    assert_eq!(
        fs::metadata(&key).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let snapshot = counts.snapshot();
    assert_eq!(snapshot.replace.attempts, 2);
    assert_eq!(snapshot.replace.succeeded, 1);
    assert_eq!(snapshot.replace.failed, 1);
    assert_eq!(snapshot.replace.errors[1], 1);
    assert_eq!(
        snapshot.replace.published_bytes,
        fs::metadata(key).unwrap().len()
    );
    assert_eq!(snapshot.delete.attempts, 0);
}

#[test]
fn legacy_managed_remove_uses_the_same_user_unlink_observer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("key");
    fs::write(&path, b"owned fixture").unwrap();
    let mut keys = Passkeys {
        version: 1,
        passkeys: vec![Passkey {
            name: "key".into(),
            kind: "inline".into(),
            path: path.to_string_lossy().into(),
        }],
    };
    let counts = metrics();
    let _scope = observe_operations(counts.clone());
    assert!(keys.remove("key"));
    assert_eq!(counts.snapshot().delete.succeeded, 1);
    assert_eq!(counts.snapshot().replace.attempts, 0);
    assert!(!path.exists());
}
