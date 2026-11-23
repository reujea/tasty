use super::*;
use crate::app::{file_consumers, file_writer};
use crate::hook_handler::{HookHandlerId, registry::UserHookHandlerUpsertDecl};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tasty_utils::{batch_writer, file_replace};
fn writer(items: usize) -> file_writer::FileWriterParts {
    file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits {
            queue_items: items,
            ..Default::default()
        },
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap()
}
fn wait(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    assert!(tasty_utils::test_wait::holds_by(end, ready));
}
fn patch_with(reg: &HookHandlerRegistry, priority: i32, params: serde_json::Value) {
    reg.upsert_user_handler(UserHookHandlerUpsertDecl {
        id: "user/test".into(),
        source: Some(crate::hook_handler::HookSource::Any),
        priority: Some(priority),
        display_name_i18n_key: None,
        disabled: None,
        action: Some(
            serde_json::from_value(serde_json::json!({
                "kind":"ipc_sequence", "calls":[{"method":"test", "params":params}]
            }))
            .unwrap(),
        ),
    })
    .unwrap();
}
fn patch(reg: &HookHandlerRegistry, priority: i32) {
    patch_with(reg, priority, serde_json::json!({"nested": null}));
}
#[test]
fn gated_burst_keeps_receipts_pending_then_saves_latest_and_foreign_section() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hooks.toml");
    std::fs::write(&path, "[foreign]\nvalue='keep'\n").unwrap();
    let reg = HookHandlerRegistry::new();
    reg.install_user_config(&path);
    let mut writer = writer(256);
    let sink = file_consumers::sink(writer.writer.handle());
    let lock = file_replace::lock(&path).unwrap();
    patch(&reg, 1);
    let (_, first) = reg.submit_file(Some(sink.clone()), false).unwrap();
    wait(|| writer.writer.stats().batches == 1);
    let mut tickets = vec![first];
    for priority in 2..=11 {
        patch(&reg, priority);
        tickets.push(reg.submit_file(Some(sink.clone()), false).unwrap().1);
    }
    assert!(tickets.iter().all(|t| t.poll().is_none()));
    assert_eq!(reg.file_status(), Some((11, 0, 11)));
    assert!(reg.reload_file().unwrap_err().contains("pending"));
    assert_eq!(
        reg.get(&HookHandlerId::new("user/test")).unwrap().priority,
        11
    );
    drop(lock);
    wait(|| {
        assert!(reg.poll_file().is_empty());
        reg.file_status().unwrap().2 == 0
    });
    assert!(tickets.iter().all(|t| t.poll() == Some(Ok(()))));
    assert_eq!(reg.file_status(), Some((11, 11, 0)));
    assert_eq!(writer.stats.snapshot().replaced, 2);
    assert_eq!(writer.writer.stats().superseded, 9);
    let text = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(table["foreign"]["value"].as_str(), Some("keep"));
    let loaded = HookHandlerRegistry::new();
    loaded.install_user_config(&path);
    assert_eq!(
        loaded
            .get(&HookHandlerId::new("user/test"))
            .unwrap()
            .priority,
        11
    );
    assert_eq!(reg.export_user_config(), loaded.export_user_config());
    wait(|| writer.writer.poll_shutdown() == 0);
}
#[test]
fn rejected_latest_remains_dirty_and_shutdown_retries_once_without_duplicate_response() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hooks.toml");
    let reg = HookHandlerRegistry::new();
    reg.install_user_config(&path);
    let mut writer = writer(1);
    let sink = file_consumers::sink(writer.writer.handle());
    let lock = file_replace::lock(&path).unwrap();
    patch(&reg, 1);
    let first = reg.submit_file(Some(sink.clone()), false).unwrap().1;
    wait(|| writer.writer.stats().batches == 1);
    patch(&reg, 2);
    let last = reg.submit_file(Some(sink), false).unwrap().1;
    assert!(last.poll().unwrap().is_err());
    assert!(reg.poll_file().is_empty());
    drop(lock);
    wait(|| {
        reg.poll_file();
        first.poll().is_some()
    });
    assert_eq!(reg.file_status(), Some((2, 1, 0)));
    reg.before_file_shutdown();
    reg.before_file_shutdown();
    wait(|| writer.writer.poll_shutdown() == 0);
    assert!(reg.poll_file().is_empty());
    assert_eq!(reg.file_status(), Some((2, 2, 0)));
    assert!(last.poll().unwrap().is_err());
    assert_eq!(writer.writer.stats().admitted, 2);
    assert_eq!(reg.finish_file_shutdown(), None);
}
#[test]
fn corrupt_original_and_external_edit_are_protected_and_failed_reload_keeps_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hooks.toml");
    let invalid = "[[handler]]\nid='user/test'\npriority='bad'\n";
    std::fs::write(&path, invalid).unwrap();
    let reg = HookHandlerRegistry::new();
    reg.install_user_config(&path);
    let mut writer = writer(256);
    let sink = file_consumers::sink(writer.writer.handle());
    patch(&reg, 1);
    let ticket = reg.submit_file(Some(sink.clone()), true).unwrap().1;
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(reg.poll_file());
        ticket.poll().is_some()
    });
    assert_eq!(errors.len(), 1);
    assert!(ticket.poll().unwrap().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    assert!(reg.reload_file().is_err());
    assert_eq!(
        reg.get(&HookHandlerId::new("user/test")).unwrap().priority,
        1
    );
    std::fs::write(&path, "[foreign]\nvalue='reloaded'\n").unwrap();
    reg.reload_file().unwrap();
    patch(&reg, 2);
    let ticket = reg.submit_file(Some(sink.clone()), false).unwrap().1;
    wait(|| {
        assert!(reg.poll_file().is_empty());
        ticket.poll().is_some()
    });
    assert_eq!(ticket.poll(), Some(Ok(())));
    std::fs::write(&path, "# external\n").unwrap();
    patch(&reg, 3);
    let ticket = reg.submit_file(Some(sink), false).unwrap().1;
    wait(|| {
        assert!(reg.poll_file().is_empty());
        ticket.poll().is_some()
    });
    assert!(ticket.poll().unwrap().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# external\n");
    wait(|| writer.writer.poll_shutdown() == 0);
    assert_eq!(reg.finish_file_shutdown(), Some((4, 3)));
    assert_eq!(reg.finish_file_shutdown(), None);
}
#[test]
fn non_toml_action_is_reported_instead_of_silently_omitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hooks.toml");
    let reg = HookHandlerRegistry::new();
    reg.install_user_config(&path);
    patch_with(&reg, 1, serde_json::json!({"number":u64::MAX}));
    let mut writer = writer(256);
    let sink = file_consumers::sink(writer.writer.handle());
    let ticket = reg.submit_file(Some(sink), false).unwrap().1;
    assert!(ticket.poll().unwrap().is_err());
    assert!(reg.poll_file().is_empty());
    assert!(!path.exists());
    assert_eq!(writer.writer.stats().admitted, 0);
    assert_eq!(reg.file_status(), Some((1, 0, 0)));
    wait(|| writer.writer.poll_shutdown() == 0);
}
