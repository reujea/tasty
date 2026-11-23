use super::*;
use crate::app::{file_consumers, file_writer};
use crate::file::{format::DetectorId, handler::HandlerId};
use std::time::{Duration, Instant};
use tasty_utils::{batch_writer, file_replace};
fn registries() -> (FileFormatRegistry, FileHandlerRegistry) {
    let format = FileFormatRegistry::new();
    format.install_host_defaults(
        "[[detector]]\nid = 'markdown'\nrule = [{kind='extension',values=['md']}]\n",
    );
    let handler = FileHandlerRegistry::new();
    handler.install_host_defaults(
        "[[handler]]\nid='view'\ndetector='markdown'\npriority=50\naction={kind='system'}\n",
    );
    assert!(handler.get(&HandlerId::new("host/view")).is_some());
    (format, handler)
}
fn writer() -> file_writer::FileWriterParts {
    file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap()
}
/// 호출자 조건은 writer join, 통계 읽기, 끝난 저장 영수증만 거두는 rules.poll 이다. 새 작업을 만들지 않아
/// 기한 뒤 관측 읽기에서도 그대로 부른다.
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(tasty_utils::test_wait::holds_by_every(
        deadline,
        Duration::ZERO,
        |_| ready()
    ));
}
fn drain(parts: &mut file_writer::FileWriterParts) {
    wait(|| parts.writer.poll_shutdown() == 0);
}
fn edit(format: &FileFormatRegistry, handler: &FileHandlerRegistry, disabled: bool) {
    format.set_user_detector_disabled(&DetectorId::new("markdown"), disabled);
    format.set_user_extension_priority("md", vec![DetectorId::new("markdown")]);
    handler.set_user_handler_disabled(&HandlerId::new("host/view"), disabled);
}
#[test]
fn combined_registry_burst_preserves_foreign_sections_and_rejects_reload_while_pending() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    std::fs::write(&path, "[foreign]\nvalue='keep'\n").unwrap();
    let (format, handler) = registries();
    let rules = FileRules::load(&path, &format, &handler);
    let mut parts = writer();
    rules.install_sink(file_consumers::sink(parts.writer.handle()));
    let lock = file_replace::lock(&path).unwrap();
    edit(&format, &handler, true);
    rules.submit(&format, &handler);
    wait(|| parts.writer.stats().batches == 1);
    for i in 1..11 {
        edit(&format, &handler, i % 2 == 0);
        rules.submit(&format, &handler);
    }
    assert_eq!(rules.status(), (11, 0, 11));
    assert!(
        rules
            .reload(&format, &handler)
            .unwrap_err()
            .contains("pending")
    );
    drop(lock);
    wait(|| {
        assert!(rules.poll().is_empty());
        rules.status().2 == 0
    });
    assert_eq!(rules.status(), (11, 11, 0));
    assert_eq!(parts.stats.snapshot().replaced, 2);
    assert_eq!(parts.writer.stats().superseded, 9);
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(table["foreign"]["value"].as_str(), Some("keep"));
    assert_eq!(table["detector"][0]["disabled"].as_bool(), Some(true));
    assert_eq!(table["handler"][0]["disabled"].as_bool(), Some(true));
    assert_eq!(
        table["extension_priority"][0]["order"][0].as_str(),
        Some("markdown")
    );
    drain(&mut parts);
}
#[test]
fn external_conflict_remains_dirty_then_explicit_reload_rebases_observation_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    let (format, handler) = registries();
    let rules = FileRules::load(&path, &format, &handler);
    let mut parts = writer();
    rules.install_sink(file_consumers::sink(parts.writer.handle()));
    edit(&format, &handler, true);
    rules.submit(&format, &handler);
    wait(|| {
        assert!(rules.poll().is_empty());
        rules.status().2 == 0
    });
    let external = "[foreign]\nvalue='external'\n";
    std::fs::write(&path, external).unwrap();
    edit(&format, &handler, false);
    rules.submit(&format, &handler);
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(rules.poll());
        rules.status().2 == 0
    });
    assert_eq!(errors.len(), 1);
    assert_eq!(rules.status(), (2, 1, 0));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
    assert!(rules.reload(&format, &handler).unwrap().is_empty());
    assert_eq!(rules.status(), (3, 3, 0));
    assert!(format.export_user_config().is_empty());
    assert!(handler.export_user_config().is_empty());
    edit(&format, &handler, true);
    rules.submit(&format, &handler);
    wait(|| {
        assert!(rules.poll().is_empty());
        rules.status().2 == 0
    });
    assert_eq!(rules.status(), (4, 4, 0));
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(table["foreign"]["value"].as_str(), Some("external"));
    drain(&mut parts);
}
#[test]
fn invalid_original_is_protected_and_invalid_reload_keeps_both_registries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    let invalid = "[[handler]]\nid='host/view'\ndisabled='invalid'\n";
    std::fs::write(&path, invalid).unwrap();
    let (format, handler) = registries();
    let rules = FileRules::load(&path, &format, &handler);
    let mut parts = writer();
    rules.install_sink(file_consumers::sink(parts.writer.handle()));
    edit(&format, &handler, true);
    let old_format = format.export_user_config();
    let old_handler = handler.export_user_config();
    rules.submit(&format, &handler);
    let mut errors = Vec::new();
    wait(|| {
        errors.extend(rules.poll());
        rules.status().2 == 0
    });
    assert_eq!(errors.len(), 1);
    assert_eq!(rules.status(), (1, 0, 0));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    std::fs::write(&path,"[[detector]]\nid='markdown'\ndisabled=false\n[[handler]]\nid='host/view'\ndisabled='invalid'\n").unwrap();
    assert!(rules.reload(&format, &handler).is_err());
    assert_eq!(format.export_user_config(), old_format);
    assert_eq!(handler.export_user_config(), old_handler);
    rules.before_shutdown();
    rules.before_shutdown();
    drain(&mut parts);
    assert_eq!(rules.poll().len(), 1);
    assert_eq!(parts.writer.stats().admitted, 2);
    assert_eq!(rules.finish_shutdown(), Some((1, 0)));
}

#[test]
fn admission_rejection_and_old_completion_keep_latest_dirty_for_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    let (format, handler) = registries();
    let rules = FileRules::load(&path, &format, &handler);
    let mut parts = file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits {
            queue_items: 1,
            ..Default::default()
        },
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    rules.install_sink(file_consumers::sink(parts.writer.handle()));
    let lock = file_replace::lock(&path).unwrap();
    edit(&format, &handler, true);
    rules.submit(&format, &handler);
    wait(|| parts.writer.stats().batches == 1);
    edit(&format, &handler, false);
    rules.submit(&format, &handler);
    assert_eq!(rules.status(), (2, 0, 1));
    assert_eq!(rules.poll().len(), 1);
    drop(lock);
    wait(|| {
        assert!(rules.poll().is_empty());
        rules.status().2 == 0
    });
    assert_eq!(rules.status(), (2, 1, 0));
    let first: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(first["handler"][0]["disabled"].as_bool(), Some(true));
    rules.before_shutdown();
    rules.before_shutdown();
    drain(&mut parts);
    assert!(rules.poll().is_empty());
    assert_eq!(rules.status(), (2, 2, 0));
    assert_eq!(parts.writer.stats().admitted, 2);
    assert_eq!(parts.writer.stats().rejected_saturated, 1);
    let latest: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(latest["handler"][0]["disabled"].as_bool(), Some(false));
    edit(&format, &handler, true);
    rules.submit(&format, &handler); // Probe admission after writer close.
    assert_eq!(rules.poll().len(), 1);
    assert_eq!(rules.status(), (3, 2, 0));
    assert_eq!(rules.finish_shutdown(), Some((3, 2)));
    assert_eq!(rules.finish_shutdown(), None);
}
