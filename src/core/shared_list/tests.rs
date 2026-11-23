use super::*;
use crate::app::{file_consumers, file_writer};
use crate::core::port_favorites::PortFavorites;
use std::time::{Duration, Instant};
use tasty_utils::{batch_writer, file_replace};

fn writer() -> file_writer::FileWriterParts {
    file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap()
}
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, ready),
        "file receipt not completed"
    );
}
fn add(source: &SharedList<PortFavorites>, port: u16) {
    source.update(|list| list.add("127.0.0.1".parse().unwrap(), port, port.to_string()));
}
fn drain(parts: &mut file_writer::FileWriterParts) {
    wait(|| parts.writer.poll_shutdown() == 0);
}

#[test]
fn blocked_file_does_not_hold_list_lock_and_burst_keeps_latest_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("favorites.toml");
    let source = SharedList::<PortFavorites>::load_at(path.clone());
    let mut parts = writer();
    source.install_sink(file_consumers::sink(parts.writer.handle()));
    let lock = file_replace::lock(&path).unwrap();
    add(&source, 1000);
    wait(|| parts.writer.stats().batches == 1);
    for port in 1001..1011 {
        add(&source, port);
    }
    assert_eq!(source.copy().1.items.len(), 11);
    assert_eq!(source.save_status(), (11, 0, 11));
    assert!(source.poll().is_empty());
    assert!(!path.exists());
    drop(lock);
    wait(|| {
        assert!(source.poll().is_empty());
        source.save_status().2 == 0
    });
    assert_eq!(source.save_status(), (11, 11, 0));
    assert_eq!(parts.stats.snapshot().replaced, 2);
    assert_eq!(parts.writer.stats().superseded, 9);
    let loaded: PortFavorites = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(loaded.items.len(), 11);
    assert_eq!(loaded.items.last().unwrap().port, 1010);
    drain(&mut parts);
}

#[test]
fn external_change_preserves_file_and_dirty_then_shutdown_retries_latest_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("favorites.toml");
    let source = SharedList::<PortFavorites>::load_at(path.clone());
    let mut parts = writer();
    source.install_sink(file_consumers::sink(parts.writer.handle()));
    add(&source, 1000);
    wait(|| {
        source.poll();
        source.save_status().2 == 0
    });
    let original = std::fs::read(&path).unwrap();
    let external = b"# external\n";
    std::fs::write(&path, external).unwrap();
    add(&source, 1001);
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(source.poll());
        source.save_status().2 == 0
    });
    assert_eq!(errors.len(), 1);
    assert_eq!(source.save_status(), (2, 1, 0));
    assert_eq!(std::fs::read(&path).unwrap(), external);
    assert_eq!(source.copy().1.items.len(), 2);
    std::fs::write(&path, original).unwrap(); // Restore only this fixture's external edit.
    source.retry_latest_before_shutdown();
    source.retry_latest_before_shutdown();
    drain(&mut parts);
    assert!(source.poll().is_empty());
    assert_eq!(source.save_status(), (2, 2, 0));
    assert_eq!(parts.writer.stats().admitted, 3);
    let loaded: PortFavorites = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(loaded.items.len(), 2);
}

#[test]
fn corrupt_original_and_closed_queue_remain_unsaved_without_losing_visible_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("favorites.toml");
    std::fs::write(&path, b"not = [valid").unwrap();
    let source = SharedList::<PortFavorites>::load_at(path.clone());
    let mut parts = writer();
    source.install_sink(file_consumers::sink(parts.writer.handle()));
    add(&source, 1000);
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(source.poll());
        source.save_status().2 == 0
    });
    assert_eq!(errors.len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), b"not = [valid");
    drain(&mut parts);
    add(&source, 1001);
    assert_eq!(source.poll().len(), 1);
    assert_eq!(source.save_status(), (2, 0, 0));
    assert_eq!(source.copy().1.items.len(), 2);
}

#[test]
fn saturated_submission_keeps_newer_dirty_after_older_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("favorites.toml");
    let source = SharedList::<PortFavorites>::load_at(path.clone());
    let limits = batch_writer::Limits {
        queue_items: 1,
        ..Default::default()
    };
    let mut parts = file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        limits,
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    source.install_sink(file_consumers::sink(parts.writer.handle()));
    let lock = file_replace::lock(&path).unwrap();
    add(&source, 1000);
    wait(|| parts.writer.stats().batches == 1);
    add(&source, 1001);
    assert_eq!(source.poll().len(), 1);
    assert_eq!(source.save_status(), (2, 0, 1));
    drop(lock);
    wait(|| {
        assert!(source.poll().is_empty());
        source.save_status().2 == 0
    });
    assert_eq!(source.save_status(), (2, 1, 0));
    source.retry_latest_before_shutdown();
    drain(&mut parts);
    assert!(source.poll().is_empty());
    assert_eq!(source.save_status(), (2, 2, 0));
    let loaded: PortFavorites = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(loaded.items.len(), 2);
}

#[cfg(unix)]
#[test]
fn snapshot_preserves_symlink_and_permissions_and_rejects_retargeting() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("actual.toml");
    std::fs::write(&target, "").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    let path = dir.path().join("favorites.toml");
    symlink(&target, &path).unwrap();
    let source = SharedList::<PortFavorites>::load_at(path.clone());
    let mut parts = writer();
    source.install_sink(file_consumers::sink(parts.writer.handle()));
    add(&source, 1000);
    wait(|| {
        assert!(source.poll().is_empty());
        source.save_status().2 == 0
    });
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let other = dir.path().join("other.toml");
    std::fs::write(&other, "# other\n").unwrap();
    std::fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    add(&source, 1001);
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(source.poll());
        source.save_status().2 == 0
    });
    assert_eq!(errors.len(), 1);
    assert_eq!(std::fs::read_to_string(other).unwrap(), "# other\n");
    assert_eq!(source.save_status(), (2, 1, 0));
    drain(&mut parts);
}
