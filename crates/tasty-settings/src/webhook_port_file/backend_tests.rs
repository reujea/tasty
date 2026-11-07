use super::*;
use std::fs;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    (dir, path)
}
/// Set only in the child process started by [`rerun_isolated`].
const ISOLATED: &str = "TASTY_SETTINGS_WEBHOOK_LOCK_ISOLATED";

/// Lock release checks fail when a child spawned by another test in this binary inherits the
/// lock file description between fork and exec: flock stays held through that copy until exec
/// closes it. Tests using [`unlocked`] therefore rerun themselves alone in a child process with
/// one test thread. Returns true in the parent after that child passed; false in the child.
fn rerun_isolated(test: &str) -> bool {
    if std::env::var_os(ISOLATED).is_some() {
        return false;
    }
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, module)| module);
    let name = format!("{module}::{test}");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1"])
        .env(ISOLATED, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // "1 passed" also rejects a misspelled name, which would otherwise filter out every test.
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "isolated run of {name} failed: {}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

fn unlocked(path: &Path) {
    assert!(
        std::env::var_os(ISOLATED).is_some(),
        "lock release checks must run through rerun_isolated"
    );
    let target = file_replace::resolve_link(path).unwrap();
    let mut name = target.as_os_str().to_owned();
    name.push(".lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(name))
        .unwrap();
    assert!(
        file.try_lock().is_ok(),
        "section result must release its lock before caller actions"
    );
}
fn secret_doc() -> &'static str {
    "format=2\nport=40000\n[listener]\nkeep=true\n[[webhook]]\nid='fixture'\n[webhook.auth]\ntoken='isolated-auth-payload'\n"
}

#[test]
fn actual_port_changes_preserve_host_auth_and_foreign_sections() {
    if rerun_isolated("actual_port_changes_preserve_host_auth_and_foreign_sections") {
        return;
    }
    let (_dir, path) = fixture();
    fs::write(&path, secret_doc()).unwrap();
    set_port(&path, 41234).unwrap();
    unlocked(&path);
    let table = read_table(&path).unwrap();
    assert_eq!(table["port"].as_integer(), Some(41234));
    assert!(table["webhook"][0]["auth"]["token"].as_str() == Some("isolated-auth-payload"));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    clear_port(&path).unwrap();
    unlocked(&path);
    let table = read_table(&path).unwrap();
    assert!(!table.contains_key("port"));
    assert!(table["webhook"][0]["auth"]["token"].as_str() == Some("isolated-auth-payload"));
}

#[test]
fn invalid_input_keeps_original_and_never_reports_its_payload() {
    if rerun_isolated("invalid_input_keeps_original_and_never_reports_its_payload") {
        return;
    }
    for bytes in [
        b"auth='isolated-auth-payload'\nbroken=[".as_slice(),
        &[0xff, 0xfe],
    ] {
        let (_dir, path) = fixture();
        fs::write(&path, bytes).unwrap();
        for result in [
            set_port(&path, 41000),
            clear_port(&path),
            replace_webhooks(&path, vec![]),
        ] {
            let error = result.unwrap_err().to_string();
            assert!(!error.contains("isolated-auth-payload"));
            assert!(fs::read(&path).unwrap() == bytes);
            unlocked(&path);
        }
        assert_eq!(read_port(&path), None);
        assert!(fs::read(&path).unwrap() == bytes);
        unlocked(&path);
    }
}

#[test]
fn editor_before_locked_read_is_merged_and_editor_after_read_is_preserved() {
    if rerun_isolated("editor_before_locked_read_is_merged_and_editor_after_read_is_preserved") {
        return;
    }
    let (_dir, path) = fixture();
    fs::write(&path, secret_doc()).unwrap();
    // Represents an owned queued section request: no full-file snapshot is retained.
    let queued_section = vec![toml::Value::Table(toml::Table::new())];
    fs::write(&path, "format=2\nport=41000\nexternal='new'\n").unwrap();
    replace_webhooks(&path, queued_section).unwrap();
    let doc = read_table(&path).unwrap();
    assert_eq!(doc["port"].as_integer(), Some(41000));
    assert!(doc["external"].as_str() == Some("new"));
    let mut file = LockedTable::open(&path).unwrap();
    file.table
        .insert("port".into(), toml::Value::Integer(42000));
    let edited = "format=2\nport=43000\nexternal='keep this edit'\n";
    fs::write(&path, edited).unwrap();
    assert!(file.commit().is_err());
    assert!(fs::read_to_string(&path).unwrap() == edited);
    unlocked(&path);
}

#[cfg(unix)]
#[test]
fn secure_replace_preserves_existing_mode_and_uses_private_mode_for_new_files() {
    if rerun_isolated("secure_replace_preserves_existing_mode_and_uses_private_mode_for_new_files")
    {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let (_dir, path) = fixture();
    set_port(&path, 41000).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    replace_webhooks(&path, vec![toml::Value::Table(toml::Table::new())]).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    unlocked(&path);
}

#[cfg(unix)]
#[test]
fn symlink_target_is_shared_and_retargeted_or_dangling_links_are_not_replaced() {
    if rerun_isolated("symlink_target_is_shared_and_retargeted_or_dangling_links_are_not_replaced")
    {
        return;
    }
    use std::os::unix::fs::symlink;
    let (dir, path) = fixture();
    let target = dir.path().join("target.toml");
    let other = dir.path().join("other.toml");
    fs::write(&target, secret_doc()).unwrap();
    fs::write(&other, "format=2\nport=42000\n").unwrap();
    symlink(&target, &path).unwrap();
    set_port(&path, 41000).unwrap();
    assert!(path.is_symlink());
    assert_eq!(
        read_table(&target).unwrap()["port"].as_integer(),
        Some(41000)
    );
    unlocked(&path);
    let file = LockedTable::open(&path).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    assert!(file.commit().is_err());
    assert_eq!(
        read_table(&other).unwrap()["port"].as_integer(),
        Some(42000)
    );
    fs::remove_file(&path).unwrap();
    symlink(dir.path().join("missing"), &path).unwrap();
    assert!(set_port(&path, 45000).is_err());
    assert!(path.is_symlink());
    assert!(!dir.path().join("missing").exists());
}

#[cfg(unix)]
struct ModeRestore(PathBuf, u32);
#[cfg(unix)]
impl Drop for ModeRestore {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&self.0, fs::Permissions::from_mode(self.1)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn read_and_write_access_errors_preserve_original_and_release_the_guard() {
    if rerun_isolated("read_and_write_access_errors_preserve_original_and_release_the_guard") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    // Real unprivileged access checks, not a mocked successful replacement.

    let (_dir, path) = fixture();
    fs::write(&path, secret_doc()).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_ne!(
        fs::metadata(&path).unwrap().uid(),
        0,
        "permission fixture requires an unprivileged test user"
    );
    let restore = ModeRestore(path.clone(), 0o600);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(set_port(&path, 41000).is_err());
    unlocked(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    assert!(replace_webhooks(&path, vec![]).is_err());
    unlocked(&path);
    assert_eq!(read_port(&path), Some(40000));
    assert!(fs::read_to_string(&path).unwrap() == secret_doc());
    drop(restore);
    clear_port(&path).unwrap();
    unlocked(&path);
}

#[cfg(unix)]
#[test]
fn temporary_creation_failure_does_not_replace_or_leave_a_temp_file() {
    if rerun_isolated("temporary_creation_failure_does_not_replace_or_leave_a_temp_file") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;

    let (dir, path) = fixture();
    fs::write(&path, secret_doc()).unwrap();
    drop(file_replace::lock(&path).unwrap()); // Existing lock can be opened in a nonwritable dir.
    let restore = ModeRestore(dir.path().into(), 0o700);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    assert!(set_port(&path, 41000).is_err());
    assert!(fs::read_to_string(&path).unwrap() == secret_doc());
    unlocked(&path);
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        2,
        "only original and lock sidecar remain"
    );
    drop(restore);
    set_port(&path, 41000).unwrap();
    unlocked(&path);
}

#[cfg(unix)]
#[test]
fn legacy_read_keeps_its_in_memory_port_when_migration_cannot_be_saved() {
    if rerun_isolated("legacy_read_keeps_its_in_memory_port_when_migration_cannot_be_saved") {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let (_dir, path) = fixture();
    fs::write(&path, "port=40000\n").unwrap();
    let restore = ModeRestore(path.clone(), 0o600);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(read_port(&path), Some(40000));
    unlocked(&path);
    assert!(fs::read_to_string(&path).unwrap() == "port=40000\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&path, "port=28429\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(read_port(&path), None);
    unlocked(&path);
    assert!(fs::read_to_string(&path).unwrap() == "port=28429\n");
    drop(restore);
}

#[test]
fn explicit_port_mutations_refresh_the_format_marker_but_host_section_does_not() {
    let (_dir, path) = fixture();
    for marker in ["3", "'custom'"] {
        fs::write(
            &path,
            format!("format={marker}\nport=40000\n[listener]\nkeep=true\n"),
        )
        .unwrap();
        replace_webhooks(&path, vec![]).unwrap();
        let table = read_table(&path).unwrap();
        assert!(
            table["format"].as_integer() == Some(3) || table["format"].as_str() == Some("custom")
        );
        set_port(&path, 41000).unwrap();
        assert_eq!(read_table(&path).unwrap()["format"].as_integer(), Some(2));
        fs::write(
            &path,
            format!("format={marker}\nport=40000\n[listener]\nkeep=true\n"),
        )
        .unwrap();
        clear_port(&path).unwrap();
        let table = read_table(&path).unwrap();
        assert_eq!(table["format"].as_integer(), Some(2));
        assert!(!table.contains_key("port"));
        assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    }
}

#[test]
fn a_legacy_file_deleted_while_migration_waits_is_not_recreated() {
    if rerun_isolated("a_legacy_file_deleted_while_migration_waits_is_not_recreated") {
        return;
    }
    use std::time::Duration;
    let (_dir, path) = fixture();
    fs::write(&path, "port=40000\n").unwrap();
    let lock = file_replace::lock(&path).unwrap();
    let (started, start) = std::sync::mpsc::channel();
    let (finished, done) = std::sync::mpsc::channel();
    let source = path.clone();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        finished.send(read_port(&source)).unwrap();
    });
    start.recv_timeout(Duration::from_secs(3)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        done.try_recv().is_err(),
        "legacy migration must wait on the participating lock"
    );
    // A nonparticipating editor removes the file before the waiting reader acquires its lock.
    fs::remove_file(&path).unwrap();
    drop(lock);
    assert_eq!(done.recv_timeout(Duration::from_secs(3)).unwrap(), None);
    worker.join().unwrap();
    assert!(
        !path.exists(),
        "a read must not resurrect an externally deleted configuration"
    );
    unlocked(&path);
}

#[cfg(unix)]
fn legacy_lock_failure(directory_readonly: bool) {
    use std::os::unix::fs::PermissionsExt;
    for (original, mode, expected) in [
        ("port=40000\n", 0o600, Some(40000)),
        ("port=28429\n", 0o600, None),
        ("port=40000\nbroken=[", 0o600, None),
        ("port=40000\n", 0o000, None),
        ("format=2\nport=28429\n", 0o600, Some(DEFAULT_PORT)),
    ] {
        let (dir, path) = fixture();
        fs::write(&path, original).unwrap();
        let file_restore = ModeRestore(path.clone(), 0o600);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        if mode == 0 {
            assert_eq!(
                fs::read(&path).unwrap_err().kind(),
                std::io::ErrorKind::PermissionDenied,
                "negative control must really deny reading"
            );
        } else {
            assert!(fs::read_to_string(&path).unwrap() == original);
        }
        let lock_path = dir.path().join("webhooks.toml.lock");
        let protection = if directory_readonly {
            assert!(!lock_path.exists());
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
            ModeRestore(dir.path().into(), 0o700)
        } else {
            fs::write(&lock_path, b"").unwrap();
            fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o400)).unwrap();
            ModeRestore(lock_path.clone(), 0o600)
        };
        assert!(
            matches!(LockedTable::open(&path), Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied),
            "migration must really fail to create/open its participating lock"
        );
        assert_eq!(read_port(&path), expected);
        assert_eq!(lock_path.exists(), !directory_readonly);
        assert!(set_port(&path, 41000).is_err());
        assert!(replace_webhooks(&path, vec![]).is_err());
        drop(protection);
        drop(file_restore);
        assert!(fs::read_to_string(&path).unwrap() == original);
        // A failed lock acquisition must not retain a guard or poison later normal mutations.
        if original.contains("broken") {
            assert!(set_port(&path, 41000).is_err());
            unlocked(&path);
        } else {
            set_port(&path, 41000).unwrap();
            unlocked(&path);
        }
    }
}

#[cfg(unix)]
#[test]
fn legacy_lock_creation_failure_keeps_read_semantics_without_writing() {
    if rerun_isolated("legacy_lock_creation_failure_keeps_read_semantics_without_writing") {
        return;
    }
    legacy_lock_failure(true);
}

#[cfg(unix)]
#[test]
fn legacy_lock_open_failure_keeps_read_semantics_without_writing() {
    if rerun_isolated("legacy_lock_open_failure_keeps_read_semantics_without_writing") {
        return;
    }
    legacy_lock_failure(false);
}

#[cfg(unix)]
#[test]
fn a_legacy_file_invalidated_while_migration_waits_does_not_reuse_its_old_port() {
    if rerun_isolated("a_legacy_file_invalidated_while_migration_waits_does_not_reuse_its_old_port")
    {
        return;
    }
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    for unreadable in [false, true] {
        let (_dir, path) = fixture();
        fs::write(&path, "port=40000\n").unwrap();
        let restore = ModeRestore(path.clone(), 0o600);
        let lock = file_replace::lock(&path).unwrap();
        let (started, start) = std::sync::mpsc::channel();
        let (finished, done) = std::sync::mpsc::channel();
        let source = path.clone();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            finished.send(read_port(&source)).unwrap();
        });
        start.recv_timeout(Duration::from_secs(3)).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(done.try_recv().is_err(), "migration must wait on its lock");
        if unreadable {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            assert_eq!(
                fs::read(&path).unwrap_err().kind(),
                std::io::ErrorKind::PermissionDenied
            );
        } else {
            fs::write(&path, "port=40000\nbroken=[").unwrap();
        }
        drop(lock);
        assert_eq!(done.recv_timeout(Duration::from_secs(3)).unwrap(), None);
        worker.join().unwrap();
        drop(restore);
        assert!(
            fs::read_to_string(&path).unwrap()
                == if unreadable {
                    "port=40000\n"
                } else {
                    "port=40000\nbroken=["
                }
        );
        unlocked(&path);
    }
}
