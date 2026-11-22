use super::*;
use std::fs;

// Parallel OS-command fixtures can temporarily inherit lock descriptors until exec.
// This post-release positive observation may retry; held-lock/stale-generation calls never do.
fn cleanup_after_release(publication: &PortFilePublication) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    tasty_utils::test_wait::holds_by(deadline, || publication.cleanup_if_owned().unwrap())
}

#[test]
fn publication_is_complete_before_return_and_cleanup_preserves_a_foreign_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let publication = publish_port_file_to(4242, Some(&path)).unwrap();
    assert_eq!(
        read_port_file_from(Some(path.to_str().unwrap())).unwrap(),
        4242
    );
    fs::write(&path, b"4243").unwrap();
    assert!(!publication.cleanup_if_owned().unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"4243");
    let publication = publish_port_file_to(4244, Some(&path)).unwrap();
    assert!(cleanup_after_release(&publication));
    assert!(!publication.cleanup_if_owned().unwrap());
    assert!(!path.exists());
}

#[test]
fn nonregular_target_and_failed_parent_creation_never_publish() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("directory.port");
    fs::create_dir(&path).unwrap();
    assert!(publish_port_file_to(4242, Some(&path)).is_err());
    let parent = dir.path().join("file");
    fs::write(&parent, b"original").unwrap();
    assert!(write_port_file_to(4242, Some(&parent.join("tasty.port"))).is_err());
    assert_eq!(fs::read(parent).unwrap(), b"original");
    assert!(
        fs::read_dir(dir.path()).unwrap().all(|x| !x
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
}

#[cfg(unix)]
#[test]
fn owner_permissions_readonly_unreadable_and_live_symlink_are_preserved() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("actual");
    write_port_file_to(4242, Some(&target)).unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.path().join("tasty.port");
    symlink("actual", &link).unwrap();
    write_port_file_to(4243, Some(&link)).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), b"4243");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    fs::set_permissions(&target, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(write_port_file_to(4244, Some(&link)).is_err());
    fs::set_permissions(&target, fs::Permissions::from_mode(0o400)).unwrap();
    assert!(write_port_file_to(4244, Some(&link)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"4243");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_file(&target).unwrap();
    assert!(write_port_file_to(4244, Some(&link)).is_err());
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn edit_after_observation_does_not_get_overwritten() {
    use tasty_utils::file_replace::{self, Fingerprint};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    fs::write(&path, b"4242").unwrap();
    let target = file_replace::resolve_link(&path).unwrap();
    let observed = Fingerprint::read(&target).unwrap();
    fs::write(&path, b"4243").unwrap();
    assert!(publish_observed(4244, path.clone(), target, observed).is_err());
    assert_eq!(fs::read(path).unwrap(), b"4243");
}

#[test]
fn publication_generation_same_port_replacement_belongs_to_the_new_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let old = publish_port_file_to(4242, Some(&path)).unwrap();
    let new = publish_port_file_to(4242, Some(&path)).unwrap();
    assert!(
        !old.cleanup_if_owned().unwrap(),
        "old generation must not unlink new file"
    );
    assert_eq!(fs::read(&path).unwrap(), b"4242");
    assert!(cleanup_after_release(&new));
}

#[test]
fn publication_generation_cleanup_leaves_a_participant_locked_file_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let publication = publish_port_file_to(4242, Some(&path)).unwrap();
    let held = tasty_utils::file_replace::lock(&path).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        let result = publication.cleanup_if_owned();
        tx.send(result).unwrap();
        publication
    });
    // Keep the lock held across the observation: a blocking cleanup cannot pass this test.
    let result = rx.recv_timeout(std::time::Duration::from_secs(2));
    drop(held);
    let publication = cleanup.join().unwrap();
    assert!(
        !result
            .expect("cleanup waited for participant lock")
            .unwrap()
    );
    assert_eq!(fs::read(&path).unwrap(), b"4242");
    assert!(cleanup_after_release(&publication));
}

#[test]
fn publication_generation_different_port_and_repeated_recreation_preserve_latest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let old = publish_port_file_to(4242, Some(&path)).unwrap();
    let different = publish_port_file_to(4243, Some(&path)).unwrap();
    assert!(!old.cleanup_if_owned().unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"4243");
    assert!(cleanup_after_release(&different));
    for _ in 0..8 {
        let latest = publish_port_file_to(4242, Some(&path)).unwrap();
        assert!(!old.cleanup_if_owned().unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"4242");
        assert!(cleanup_after_release(&latest));
    }
}

#[test]
fn publication_generation_unknown_identity_never_deletes_even_matching_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let mut publication = publish_port_file_to(4242, Some(&path)).unwrap();
    publication.identity = None;
    assert!(!publication.cleanup_if_owned().unwrap());
    assert_eq!(fs::read(&path).unwrap(), b"4242");
}

#[cfg(unix)]
#[test]
fn publication_generation_live_symlink_keeps_new_publication_and_target_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("actual");
    fs::write(&target, b"4241").unwrap();
    let path = dir.path().join("tasty.port");
    std::os::unix::fs::symlink("actual", &path).unwrap();
    let old = publish_port_file_to(4242, Some(&path)).unwrap();
    let latest = publish_port_file_to(4242, Some(&path)).unwrap();
    assert!(!old.cleanup_if_owned().unwrap());
    assert!(path.is_symlink());
    assert!(cleanup_after_release(&latest));
    assert!(!path.is_symlink());
    assert_eq!(fs::read(target).unwrap(), b"4242");
}

#[cfg(unix)]
#[test]
fn publication_generation_nonregular_replacement_is_preserved_without_opening_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasty.port");
    let publication = publish_port_file_to(4242, Some(&path)).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let cleanup = std::thread::spawn(move || tx.send(publication.cleanup_if_owned()).unwrap());
    let result = rx.recv_timeout(std::time::Duration::from_secs(2));
    if result.is_err() {
        // Unblock a regressed FIFO reader so a failed assertion cannot leave an owned thread.
        let _writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
    }
    cleanup.join().unwrap();
    assert!(
        !result
            .expect("cleanup opened a FIFO and waited for a peer")
            .unwrap()
    );
    use std::os::unix::fs::FileTypeExt;
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
}
