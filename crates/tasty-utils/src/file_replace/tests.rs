use super::*;

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

#[test]
fn a_new_file_is_created_and_an_existing_one_is_replaced_without_leftovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    replace(&path, b"a = 1\n").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"a = 1\n");
    replace(&path, b"a = 2\n").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"a = 2\n");
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn a_failed_replace_keeps_the_original_and_removes_its_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, b"original").unwrap();
    // 대상 이름이 디렉터리라 이름 바꾸기가 실패한다.
    let blocked = dir.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("inside"), b"x").unwrap();
    assert!(replace(&blocked, b"new").is_err());
    assert!(blocked.is_dir());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn a_missing_directory_is_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent").join("config.toml");
    let error = replace(&path, b"x").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!path.exists());
}

#[test]
fn a_new_destination_can_inherit_source_security_without_changing_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("destination");
    fs::write(&source, b"source").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    }
    replace_preserving(&target, b"destination", &source).unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(&target).unwrap(), b"destination");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let absent = dir.path().join("not-created");
    assert!(replace_preserving(&absent, b"private", &dir.path().join("missing-source")).is_err());
    assert!(!absent.exists());
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn existing_destination_keeps_its_own_security_when_a_source_is_provided() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("destination");
    fs::write(&source, b"source").unwrap();
    fs::write(&target, b"old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    }
    replace_preserving(&target, b"new", &source).unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(&target).unwrap(), b"new");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(leftovers(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn the_existing_mode_is_kept_and_a_new_secret_starts_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hooks.toml");
    fs::write(&path, b"old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    replace(&path, b"new").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );

    let secret = dir.path().join("passkey");
    replace_with(
        &secret,
        b"secret",
        Options {
            new_mode: Some(0o600),
            sync: true,
        },
    )
    .unwrap();
    assert_eq!(
        fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn a_link_keeps_pointing_at_the_replaced_file_and_a_dangling_link_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let real_dir = dir.path().join("dotfiles");
    fs::create_dir(&real_dir).unwrap();
    let real = real_dir.join("config.toml");
    fs::write(&real, b"old").unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink("dotfiles/config.toml", &link).unwrap();

    replace(&link, b"new").unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&real).unwrap(), b"new");
    assert!(leftovers(&real_dir).is_empty());

    let dangling = dir.path().join("dangling.toml");
    std::os::unix::fs::symlink("dotfiles/absent.toml", &dangling).unwrap();
    let error = replace(&dangling, b"x").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!real_dir.join("absent.toml").exists());
}

#[test]
fn the_lock_file_serializes_writers_across_replacements() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("remote-profiles.toml");
    let held = lock(&path).unwrap();
    replace(&path, b"first").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let other = path.clone();
    let waiter = std::thread::spawn(move || {
        let _second = lock(&other).unwrap();
        tx.send(()).unwrap();
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "the second writer waits while the first holds the lock"
    );
    drop(held);
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("the second writer gets the lock once it is released");
    waiter.join().unwrap();
}

#[test]
fn a_fingerprint_sees_content_changes_and_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    assert_eq!(Fingerprint::read(&path).unwrap(), Fingerprint::Missing);
    fs::write(&path, b"a").unwrap();
    let first = Fingerprint::read(&path).unwrap();
    assert_eq!(first, Fingerprint::of_bytes(b"a"));
    fs::write(&path, b"b").unwrap();
    assert_ne!(Fingerprint::read(&path).unwrap(), first);
}

#[cfg(unix)]
#[test]
fn an_existing_secret_is_private_before_any_payload_is_written() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret");
    fs::write(&path, b"old").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    replace_using(&path, Options::default(), |file| {
        assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
        assert_eq!(file.metadata()?.len(), 0);
        file.write_all(b"private new bytes")
    })
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"private new bytes");
}

#[cfg(unix)]
#[test]
fn rename_does_not_bypass_existing_file_write_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("readonly");
    fs::write(&path, b"original").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let direct = OpenOptions::new().write(true).open(&path);
    // Privileged test runners can write read-only files; compare the actual prior access result.
    if direct.is_err() {
        assert_eq!(
            replace(&path, b"new").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(leftovers(dir.path()).is_empty());
    } else {
        replace(&path, b"new").unwrap();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn owner_group_extended_attributes_and_acl_survive_before_and_after_writing() {
    use rustix::fs::{XattrFlags, fgetxattr, fsetxattr};
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("controlled");
    fs::write(&path, b"old").unwrap();
    let source = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let before = source.metadata().unwrap();
    fsetxattr(&source, "user.test", b"kept", XattrFlags::empty()).unwrap();
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permissions, id) in [
        (1u16, 6u16, u32::MAX),
        (2, 0, before.uid() + 1),
        (4, 0, u32::MAX),
        (16, 0, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend(tag.to_le_bytes());
        acl.extend(permissions.to_le_bytes());
        acl.extend(id.to_le_bytes());
    }
    fsetxattr(
        &source,
        "system.posix_acl_access",
        &acl,
        XattrFlags::empty(),
    )
    .unwrap();
    let check = |file: &File| {
        let meta = file.metadata().unwrap();
        assert_eq!((meta.uid(), meta.gid()), (before.uid(), before.gid()));
        let mut value = vec![0u8; acl.len()];
        let len = fgetxattr(file, "system.posix_acl_access", value.as_mut_slice()).unwrap();
        assert_eq!(&value[..len], acl.as_slice());
        let len = fgetxattr(file, "user.test", value.as_mut_slice()).unwrap();
        assert_eq!(&value[..len], b"kept");
    };
    replace_using(&path, Options::default(), |file| {
        check(file);
        assert_eq!(file.metadata()?.len(), 0);
        file.write_all(b"new")
    })
    .unwrap();
    check(&File::open(&path).unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"new");
    let destination = dir.path().join("new-destination");
    replace_using_source(&destination, Options::default(), Some(&path), |file| {
        check(file);
        assert_eq!(file.metadata()?.len(), 0);
        file.write_all(b"copy")
    })
    .unwrap();
    check(&File::open(&destination).unwrap());
    assert_eq!(fs::read(&destination).unwrap(), b"copy");
    assert_eq!(fs::read(&path).unwrap(), b"new");
}

#[cfg(windows)]
#[test]
fn a_windows_readonly_target_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("readonly");
    fs::write(&path, b"original").unwrap();
    let original_permission = fs::metadata(&path).unwrap().permissions();
    let mut permission = original_permission.clone();
    permission.set_readonly(true);
    fs::set_permissions(&path, permission).unwrap();
    assert!(replace(&path, b"new").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original");
    fs::set_permissions(&path, original_permission).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn macos_custom_acl_and_extended_attribute_survive_replacement() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("controlled");
    fs::write(&path, b"old").unwrap();
    let identity = Command::new("id").arg("-un").output().unwrap();
    assert!(identity.status.success());
    let user = String::from_utf8(identity.stdout).unwrap();
    let entry = format!(
        "user:{} allow read,write,readattr,writeattr,readextattr,writeextattr,readsecurity",
        user.trim()
    );
    assert!(
        Command::new("chmod")
            .args(["+a", &entry])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("xattr")
            .args(["-w", "user.test", "kept"])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let acl = |path: &Path| {
        let output = Command::new("ls").arg("-le").arg(path).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let before = acl(&path);
    assert!(!before.is_empty());
    replace(&path, b"new").unwrap();
    assert_eq!(acl(&path), before);
    let output = Command::new("xattr")
        .args(["-p", "user.test"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "kept");
}

#[test]
fn nonblocking_participant_lock_uses_the_blocking_namespace_and_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    // Other parallel tests spawn OS commands: inherited descriptors can briefly retain a
    // released lock until exec. Only this test's post-release observation retries; the product
    // try_lock and the held-lock cleanup test must still return after a single attempt.
    let acquire_released = || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut result = None;
        // 잠금 획득이 곧 기다리는 결과다. 기한 뒤 마지막 시도도 같은 획득이라 판정을 바꾸지 않는다.
        assert!(
            crate::test_wait::holds_by_every(deadline, std::time::Duration::ZERO, |_| {
                result = try_lock(&path).unwrap();
                result.is_some()
            }),
            "released lock stayed held"
        );
        result
    };
    let held = lock(&path).unwrap();
    assert!(try_lock(&path).unwrap().is_none());
    drop(held);
    let acquired = acquire_released().expect("released participant lock");
    assert!(try_lock(&path).unwrap().is_none());
    drop(acquired);
    assert!(acquire_released().is_some());
    let bad = dir.path().join("missing/child.toml");
    assert!(try_lock(&bad).is_err(), "path errors are not contention");
}
