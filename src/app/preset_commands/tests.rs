use super::*;
use crate::app::file_writer::{self, FileWriterParts};
use std::time::{Duration, Instant};
use tasty_presets::{
    PresetPane, PresetPaneNode, PresetSurface, PresetSurfaceLayout, PresetTab, WorkspacePreset,
};
use tasty_utils::batch_writer::{Limits, SystemClock};
use tasty_utils::test_wait::{holds_by, holds_by_every};

pub(crate) fn value(subtitle: &str) -> OwnedPreset {
    OwnedPreset::Workspace(WorkspacePreset {
        name: "ignored".into(),
        subtitle: subtitle.into(),
        description: String::new(),
        layout: PresetPaneNode::Leaf {
            pane: PresetPane {
                active_tab: 0,
                tabs: vec![PresetTab {
                    explicit_name: None,
                    layout: PresetSurfaceLayout::Leaf {
                        surface: PresetSurface {
                            id: None,
                            kind: "terminal".into(),
                            cwd: None,
                            startup_command: None,
                            params: serde_json::Value::Null,
                        },
                    },
                }],
            },
        },
    })
}
struct Fixture {
    root: tempfile::TempDir,
    cache: Arc<Mutex<PresetStore>>,
    commands: PresetCommands,
    parts: FileWriterParts,
}
impl Fixture {
    fn new(limits: Limits) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("workspace")).unwrap();
        let cache = Arc::new(Mutex::new(PresetStore::load_from(root.path().into())));
        let parts = file_writer::start(
            Arc::new(tasty_settings::FileSettingsStorage),
            limits,
            Arc::new(SystemClock),
            Arc::new(|| {}),
        )
        .unwrap();
        let commands = PresetCommands::new(cache.clone(), Some(parts.writer.handle()));
        Self {
            root,
            cache,
            commands,
            parts,
        }
    }
    fn save(&self, name: Option<&str>, overwrite: bool, subtitle: &str) -> Ticket<SaveOutcome> {
        self.commands.save(
            "a".into(),
            name.map(str::to_owned),
            overwrite,
            value(subtitle),
        )
    }
    fn collect<T>(&self, ticket: &Ticket<T>) -> Result<T, PresetMutationError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        // commands.poll 은 끝난 항목만 거두므로 기한 뒤 관측에서도 거둔 뒤 읽는다.
        let mut result = None;
        assert!(
            holds_by_every(deadline, Duration::ZERO, |_| {
                self.commands.poll();
                result = ticket.poll();
                result.is_some()
            }),
            "command receipt missing"
        );
        result.expect("the wait held")
    }
    fn drain(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        self.parts.writer.close();
        // commands.poll 은 거두기, writer poll_shutdown 은 끝난 worker join 이라 새 작업이 없다.
        assert!(
            holds_by_every(deadline, Duration::ZERO, |_| {
                self.commands.poll();
                self.parts.writer.poll_shutdown() == 0
            }),
            "writer failed to stop"
        );
        self.commands.poll();
    }
}
#[test]
fn ordered_commands_publish_cache_before_receipt_and_do_not_coalesce() {
    let mut f = Fixture::new(Limits::default());
    let held = tasty_utils::file_replace::lock(&f.root.path().join("workspace/a.toml")).unwrap();
    let first = f.save(Some("a"), false, "one");
    let second = f.save(Some("a"), true, "two");
    // intent-exempt: exercise queued rename ordering itself and verify its receipt, cache and disk.
    let renamed = f
        .commands
        .rename(PresetKind::Workspace, "a".into(), "b".into());
    // intent-exempt: exercise queued delete ordering itself and verify its receipt, cache and disk.
    let removed = f.commands.delete(PresetKind::Workspace, "b".into());
    let third = f.save(None, false, "three");
    let fourth = f.save(None, false, "four");
    assert!(first.poll().is_none());
    assert!(
        f.cache
            .lock()
            .unwrap()
            .list(PresetKind::Workspace)
            .is_empty()
    );
    assert_eq!(f.commands.status().pending, 6);
    drop(held);
    assert!(matches!(f.collect(&first).unwrap(), SaveOutcome::Saved(name) if name == "a"));
    f.collect(&second).unwrap();
    f.collect(&renamed).unwrap();
    f.collect(&removed).unwrap();
    assert!(matches!(f.collect(&third).unwrap(), SaveOutcome::Saved(name) if name == "a"));
    assert!(matches!(f.collect(&fourth).unwrap(), SaveOutcome::Saved(name) if name == "a-2"));
    let cache = f.cache.lock().unwrap();
    assert_eq!(cache.list(PresetKind::Workspace), ["a", "a-2"]);
    assert_eq!(cache.get_workspace("a").unwrap().subtitle, "three");
    assert_eq!(cache.get_workspace("a-2").unwrap().subtitle, "four");
    drop(cache);
    let disk = PresetStore::load_from(f.root.path().into());
    assert_eq!(disk.get_workspace("a").unwrap().subtitle, "three");
    assert!(!f.root.path().join("workspace/b.toml").exists());
    f.drain();
    assert_eq!(f.parts.writer.stats().superseded, 0);
    assert_ordered_command_measurements(&f);
}
#[test]
fn queue_full_and_closed_rejections_never_mutate_files_or_cache() {
    let mut f = Fixture::new(Limits {
        queue_items: 1,
        ..Limits::default()
    });
    let held = tasty_utils::file_replace::lock(&f.root.path().join("workspace/a.toml")).unwrap();
    let first = f.save(Some("a"), false, "one");
    let rejected = f.save(Some("rejected"), false, "two");
    assert!(matches!(
        rejected.poll(),
        Some(Err(PresetMutationError::Transport { unknown: false, .. }))
    ));
    f.parts.writer.close();
    let closed = f.save(Some("closed"), false, "three");
    assert!(closed.poll().unwrap().is_err());
    assert!(
        f.cache
            .lock()
            .unwrap()
            .list(PresetKind::Workspace)
            .is_empty()
    );
    drop(held);
    f.collect(&first).unwrap();
    f.drain();
    assert_eq!(f.commands.status().accepted, 1);
    assert_eq!(f.commands.status().rejected, 2);
    assert_eq!(f.cache.lock().unwrap().list(PresetKind::Workspace), ["a"]);
    assert!(!f.root.path().join("workspace/rejected.toml").exists());
    assert!(!f.root.path().join("workspace/closed.toml").exists());
}
#[test]
fn external_edit_failure_preserves_original_cache_and_external_bytes() {
    let mut f = Fixture::new(Limits::default());
    f.collect(&f.save(Some("a"), false, "original")).unwrap();
    let path = f.root.path().join("workspace/a.toml");
    let held = tasty_utils::file_replace::lock(&path).unwrap();
    let ticket = f.save(Some("a"), true, "discarded");
    std::fs::write(&path, "# externally changed\n").unwrap();
    assert!(ticket.poll().is_none());
    drop(held);
    assert!(matches!(
        f.collect(&ticket),
        Err(PresetMutationError::Store(_))
    ));
    assert_eq!(
        f.cache.lock().unwrap().get_workspace("a").unwrap().subtitle,
        "original"
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "# externally changed\n"
    );
    f.drain();
    let stats = f.parts.stats.snapshot();
    assert_eq!(
        stats.operations.replace.attempts, 1,
        "validation failure must not fabricate a replacement attempt"
    );
    assert_eq!(stats.commands.known_failure, 1);
}
#[test]
fn dropped_observer_does_not_cancel_accepted_save_or_cache_publication() {
    let mut f = Fixture::new(Limits::default());
    let held = tasty_utils::file_replace::lock(&f.root.path().join("workspace/a.toml")).unwrap();
    drop(f.save(Some("a"), false, "saved"));
    assert_eq!(f.commands.status().pending, 1);
    drop(held);
    f.drain();
    assert_eq!(
        f.cache.lock().unwrap().get_workspace("a").unwrap().subtitle,
        "saved"
    );
    assert_eq!(f.commands.status().completed, 1);
}
#[test]
fn panic_after_side_effect_is_unknown_and_never_replayed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut f = Fixture::new(Limits::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let actual = calls.clone();
    let marker = f.root.path().join("side-effect");
    let output = marker.clone();
    let ticket = f.commands.submit::<()>(128, move |_| {
        actual.fetch_add(1, Ordering::Relaxed);
        tasty_utils::file_replace::replace(&output, b"committed").unwrap();
        panic!("injected after actual side effect");
    });
    assert!(matches!(
        f.collect(&ticket),
        Err(PresetMutationError::Transport { unknown: true, .. })
    ));
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "committed");
    let next = f.save(Some("a"), false, "must not run on poisoned worker");
    assert!(matches!(
        f.collect(&next),
        Err(PresetMutationError::Transport { unknown: false, .. })
    ));
    f.drain();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(f.commands.status().unknown, 1);
    let stats = f.parts.stats.snapshot();
    assert_eq!(stats.operations.replace.succeeded, 1);
    assert_eq!(
        stats.operations.replace.unknown, 0,
        "post-replace panic cannot erase a known successful physical operation"
    );
    assert_eq!(stats.commands.unknown, 1);
    assert_eq!(stats.commands.known_failure, 1);
    assert!(
        f.cache
            .lock()
            .unwrap()
            .list(PresetKind::Workspace)
            .is_empty()
    );
}
#[test]
fn dynamic_payload_is_rejected_by_byte_budget_before_execution() {
    let mut f = Fixture::new(Limits {
        queue_bytes: 4096,
        ..Limits::default()
    });
    let ticket = f.save(Some("a"), false, &"x".repeat(20_000));
    assert!(matches!(
        ticket.poll(),
        Some(Err(PresetMutationError::Transport { unknown: false, .. }))
    ));
    assert_eq!(f.commands.status().rejected, 1);
    assert!(!f.root.path().join("workspace/a.toml").exists());
    f.drain();
}
#[test]
fn committed_rename_with_missing_cache_source_reports_saved_cache_failure() {
    let mut f = Fixture::new(Limits::default());
    f.collect(&f.save(Some("a"), false, "source")).unwrap();
    let held = tasty_utils::file_replace::lock(&f.root.path().join("workspace/a.toml")).unwrap();
    // intent-exempt: exercise committed rename with a missing cache source and inspect saved failure.
    let ticket = f
        .commands
        .rename(PresetKind::Workspace, "a".into(), "b".into());
    // Simulate a broken independent cache mutator; the disk commit must not become a false success.
    f.cache
        .lock()
        .unwrap()
        .apply_change(PresetChange::Removed {
            kind: PresetKind::Workspace,
            name: "a".into(),
        })
        .unwrap();
    drop(held);
    assert!(matches!(
        f.collect(&ticket),
        Err(PresetMutationError::Cache {
            saved: true,
            partial: false,
            ..
        })
    ));
    assert!(f.root.path().join("workspace/b.toml").is_file());
    assert!(!f.root.path().join("workspace/a.toml").exists());
    assert!(f.cache.lock().unwrap().get_workspace("b").is_none());
    f.drain();
}
#[test]
fn actual_success_then_failed_cleanup_is_partial_not_a_single_failed_replace() {
    let mut f = Fixture::new(Limits::default());
    let destination = f.root.path().join("destination");
    let blocked = f.root.path().join("cannot-unlink-directory");
    std::fs::create_dir(&blocked).unwrap();
    let output = destination.clone();
    let ticket = f.commands.submit::<()>(128, move |_| {
        tasty_utils::file_replace::replace(&output, b"committed").unwrap();
        let error = tasty_utils::file_replace::remove(&blocked).unwrap_err();
        let mut outcome = Outcome::new(
            Vec::new(),
            Err(PresetMutationError::Transport {
                message: error.to_string(),
                unknown: false,
            }),
        );
        outcome.partial = true;
        outcome
    });
    assert!(f.collect(&ticket).is_err());
    f.drain();
    assert_eq!(std::fs::read(destination).unwrap(), b"committed");
    let stats = f.parts.stats.snapshot();
    assert_eq!(
        (
            stats.operations.replace.succeeded,
            stats.operations.replace.failed
        ),
        (1, 0)
    );
    assert_eq!(
        (
            stats.operations.delete.attempts,
            stats.operations.delete.failed
        ),
        (1, 1)
    );
    assert_eq!(stats.commands.known_failure, 1);
    assert_eq!(stats.commands.partial, 1);
}
#[test]
fn opaque_successful_output_is_unclassified_until_backend_supplies_a_classifier() {
    let mut f = Fixture::new(Limits::default());
    let path = f.root.path().join("opaque");
    let queue = f.parts.writer.handle();
    let ticket = command::submit(
        Some(&queue),
        128,
        move || {
            tasty_utils::file_replace::replace(&path, b"actual").unwrap();
            false // An opaque value is not evidence of backend business success or failure.
        },
        |_| 1,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut result = None;
    assert!(holds_by(deadline, || {
        result = ticket.poll();
        result.is_some()
    }));
    assert!(!result.unwrap().unwrap());
    f.drain();
    let stats = f.parts.stats.snapshot();
    assert_eq!(stats.operations.replace.succeeded, 1);
    assert_eq!(stats.commands.unclassified, 1);
    assert_eq!(stats.commands.known_success, 0);
    assert_eq!(stats.commands.known_failure, 0);
}

fn assert_ordered_command_measurements(f: &Fixture) {
    let stats = f.parts.stats.snapshot();
    assert_eq!(stats.replaced, 0);
    assert_eq!(
        (
            stats.operations.replace.attempts,
            stats.operations.replace.succeeded
        ),
        (5, 5)
    );
    assert_eq!(
        (
            stats.operations.delete.attempts,
            stats.operations.delete.succeeded
        ),
        (2, 2)
    );
    assert!(stats.operations.replace.payload_bytes > 0);
    assert_eq!(
        stats.operations.replace.payload_bytes,
        stats.operations.replace.published_bytes
    );
    assert_eq!(stats.commands.known_success, 6);
    assert_eq!(stats.commands.known_failure, 0);
    assert_eq!(stats.commands.unclassified, 0);
    assert_eq!(f.commands.status().accepted, 6);
    assert_eq!(f.commands.status().completed, 6);
}
