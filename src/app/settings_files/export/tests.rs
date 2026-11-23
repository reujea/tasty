use super::*;
use crate::app::file_writer::{self, FileWriterParts};
use std::time::{Duration, Instant};
use tasty_utils::batch_writer::{Limits, SystemClock};

fn parts(limits: Limits) -> FileWriterParts {
    file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        limits,
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap()
}
fn request(path: PathBuf, text: &str) -> SettingsFileRequest {
    SettingsFileRequest::Export {
        path,
        text: text.into(),
        observer: Weak::new(),
    }
}
fn collect(pending: &PendingExport) -> SettingsFileResult {
    let end = Instant::now() + Duration::from_secs(5);
    let mut result = None;
    assert!(tasty_utils::test_wait::holds_by(end, || {
        result = pending.poll();
        result.is_some()
    }));
    result.expect("the wait held")
}
fn success(result: SettingsFileResult) {
    assert!(matches!(
        result,
        SettingsFileResult::Export { result: Ok(()), .. }
    ));
}
#[test]
fn held_export_is_nonblocking_and_receipt_retains_credit_without_coalescing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bundle.toml");
    let held = tasty_utils::file_replace::lock(&path).unwrap();
    let mut parts = parts(Limits::default());
    let queue = parts.writer.handle();
    let first = PendingExport::submit(Some(&queue), request(path.clone(), "one"))
        .ok()
        .unwrap();
    let second = PendingExport::submit(Some(&queue), request(path.clone(), "two"))
        .ok()
        .unwrap();
    assert!(first.poll().is_none());
    assert!(second.poll().is_none());
    assert!(!path.exists());
    assert_eq!(parts.writer.stats().held_items, 2);
    assert!(parts.writer.stats().held_bytes > 0);
    drop(held);
    success(collect(&first));
    success(collect(&second));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "two");
    assert!(first.poll().is_none());
    assert_eq!(parts.writer.stats().held_items, 0);
    parts.writer.begin_shutdown();
    while parts.writer.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
    assert_eq!(parts.writer.stats().completed_ok, 2);
}
#[test]
fn count_and_byte_refusals_never_touch_the_destination() {
    for limits in [
        Limits {
            queue_items: 0,
            ..Limits::default()
        },
        Limits {
            queue_bytes: 10,
            ..Limits::default()
        },
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("bundle.toml");
        let parts = parts(limits);
        let result =
            PendingExport::submit(Some(&parts.writer.handle()), request(path.clone(), "value"));
        let result = result.err().expect("refusal");
        assert!(matches!(
            *result,
            SettingsFileResult::ExportTransport {
                error: ExportError::Refused(_),
                ..
            }
        ));
        assert!(!path.exists());
        assert_eq!(parts.writer.stats().admitted, 0);
    }
}
#[test]
fn unavailable_writer_does_not_fall_back_to_inline_execution() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bundle.toml");
    assert!(PendingExport::submit(None, request(path.clone(), "value")).is_err());
    assert!(!path.exists());
}
#[test]
fn unknown_writer_result_is_not_replayed_or_rewritten_as_known_failure() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bundle.toml");
    let mut parts = parts(Limits::default());
    let ticket = command::submit(
        Some(&parts.writer.handle()),
        512,
        || -> SettingsFileResult { panic!("injected completion uncertainty") },
        |_| 128,
    )
    .unwrap();
    let pending = PendingExport {
        observer: Weak::new(),
        ticket,
    };
    assert!(matches!(
        collect(&pending),
        SettingsFileResult::ExportTransport {
            error: ExportError::Unknown,
            ..
        }
    ));
    assert!(!path.exists());
    assert!(pending.poll().is_none());
    assert_eq!(parts.writer.stats().admitted, 1);
    parts.writer.begin_shutdown();
    while parts.writer.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}
#[test]
fn observation_rejects_old_request_after_edit_close_or_reopen() {
    let mut original = Observation::default();
    let old = original
        .begin(std::path::Path::new("owned.toml"), "old")
        .unwrap();
    assert!(
        original
            .begin(std::path::Path::new("owned.toml"), "old")
            .is_none(),
        "pending click must not duplicate"
    );
    original.observe("edited");
    assert!(original.finish(&old).is_none());
    let new = original
        .begin(std::path::Path::new("owned.toml"), "edited")
        .unwrap();
    assert!(original.finish(&old).is_none());
    assert!(original.finish(&new).is_some());
    assert!(original.finish(&new).is_none());
    let closed = original
        .begin(std::path::Path::new("owned.toml"), "closed")
        .unwrap();
    drop(original);
    assert!(closed.upgrade().is_none());
    assert!(Observation::default().finish(&closed).is_none());
}
#[cfg(unix)]
#[test]
fn actual_atomic_replace_failure_keeps_original_and_records_no_success() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bundle.toml");
    std::fs::write(&path, "original").unwrap();
    drop(tasty_utils::file_replace::lock(&path).unwrap());
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = write(&path, "replacement");
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

#[test]
fn completed_unconsumed_receipt_still_accounts_for_owned_output_path() {
    let root = tempfile::tempdir().unwrap();
    let mut path = root.path().join("bundle.toml");
    path.reserve(128 * 1024);
    let capacity = path.capacity() as u64;
    let mut parts = parts(Limits::default());
    let pending = PendingExport::submit(Some(&parts.writer.handle()), request(path, "value"))
        .ok()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    assert!(tasty_utils::test_wait::holds_by(end, || {
        parts.writer.stats().completed_ok != 0
    }));
    assert_eq!(parts.writer.stats().held_items, 1);
    assert!(parts.writer.stats().held_bytes >= capacity);
    success(collect(&pending));
    assert_eq!(parts.writer.stats().held_items, 0);
    parts.writer.begin_shutdown();
    assert!(tasty_utils::test_wait::holds_by(end, || {
        parts.writer.poll_shutdown() == 0
    }));
}
