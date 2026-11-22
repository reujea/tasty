use super::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use tasty_host_plugin::manager::file_config::ConfigChange;

const ID: &str = "com.example.file-receipt";
fn app() -> App {
    let mut app = App::new_headless(Arc::new(|| {}), None, None, None).unwrap();
    let mut mgr = crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let _stub = mgr.attach_namespace_stub_for_test(ID, "filereceipt");
    app.plugin_manager = Some(mgr);
    app
}
fn pump(app: &mut App, reply: &Reply) -> Result<Value, Failure> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.services.poll_file_writer();
        app.finish_plugin_files();
        if let Some(result) = reply.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "plugin file reply pending");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn disable() -> ConfigChange {
    ConfigChange::Enabled {
        id: ID.into(),
        enabled: false,
    }
}

#[test]
fn held_file_lock_keeps_runtime_and_original_reply_pending_until_receipt() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    let path = tasty_utils::path::tasty_home()
        .unwrap()
        .join("plugins.toml");
    let lock = tasty_utils::file_replace::lock(&path).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(2);
    let response = crate::ipc::handler::plugin::dispatch_lifecycle_toggle(
        &mut app,
        "plugin.disable",
        serde_json::json!(73),
        &serde_json::json!({"id":ID}),
    )
    .unwrap();
    app.services.send_handled(&tx, response);
    app.finish_plugin_files();
    std::thread::sleep(Duration::from_millis(20));
    app.finish_plugin_files();
    assert!(rx.try_recv().is_err());
    assert!(
        app.plugin_files.budget.bytes() > 0,
        "active pipeline remains charged while file lock delays receipt"
    );
    let mgr = app.plugin_manager.as_ref().unwrap();
    assert!(mgr.is_running(ID));
    assert!(!mgr.config.is_disabled(ID));
    drop(lock);
    let deadline = Instant::now() + Duration::from_secs(5);
    let response = loop {
        app.services.poll_file_writer();
        app.finish_plugin_files();
        app.services
            .poll_memory_replies(app.plugin_manager.as_mut());
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(response.id, serde_json::json!(73));
    assert!(response.error.is_none());
    assert_eq!(response.result.unwrap()["disabled"], ID);
    assert_eq!(
        app.plugin_files.budget.bytes(),
        0,
        "completed receipt/callback releases its retained reservation"
    );
    let mgr = app.plugin_manager.as_ref().unwrap();
    assert!(!mgr.is_running(ID));
    assert!(mgr.config.is_disabled(ID));
    for _ in 0..3 {
        app.finish_plugin_files();
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn failed_file_write_does_not_retire_or_mutate_live_plugin() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    let path = tasty_utils::path::tasty_home()
        .unwrap()
        .join("plugins.toml");
    std::fs::write(&path, "PRIVATE = [[[\n").unwrap();
    let reply = Reply::default();
    let done = reply.clone();
    app.enqueue_plugin_config(disable(), move |_, result| done.finish(result))
        .unwrap();
    let failure = pump(&mut app, &reply).unwrap_err();
    assert_eq!(failure.saved, Some(false));
    assert!(!failure.applied);
    assert!(!failure.message.contains("PRIVATE"));
    let mgr = app.plugin_manager.as_ref().unwrap();
    assert!(mgr.is_running(ID));
    assert!(!mgr.config.is_disabled(ID));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "PRIVATE = [[[\n");
}

#[test]
fn unknown_worker_outcome_never_applies_or_replays_and_finishes_once() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let reply = Reply::default();
    let done = reply.clone();
    app.run_plugin_file(
        (1, 0),
        move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            panic!("injected file outcome loss");
        },
        |_: &()| 0,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |_, result| done.finish(result.map(|_| serde_json::json!({"applied":true}))),
    );
    let failure = pump(&mut app, &reply).unwrap_err();
    assert_eq!(failure.saved, None);
    assert!(!failure.applied);
    for _ in 0..3 {
        app.finish_plugin_files();
        assert!(reply.poll().is_none());
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(app.plugin_manager.as_ref().unwrap().is_running(ID));
}

#[test]
fn ordered_callers_keep_their_own_completion_and_later_preparation_sees_commit() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let first = Reply::default();
    let second = Reply::default();
    for (sequence, reply) in [(1, first.clone()), (2, second.clone())] {
        let observed = observed.clone();
        app.enqueue_plugin_config(disable(), move |app, result| {
            assert!(app.plugin_manager.as_ref().unwrap().config.is_disabled(ID));
            observed.borrow_mut().push(sequence);
            reply.finish(result);
        })
        .unwrap();
    }
    pump(&mut app, &first).unwrap();
    pump(&mut app, &second).unwrap();
    assert_eq!(*observed.borrow(), vec![1, 2]);
    assert_eq!(app.plugin_files_pending(), 0);
}

#[test]
fn shutdown_admission_rejection_has_no_file_or_runtime_effect() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    app.stop_plugin_file_producers();
    let error = app
        .enqueue_plugin_config(disable(), |_, _| panic!("rejected work ran"))
        .unwrap_err();
    assert_eq!(error.saved, Some(false));
    app.finish_plugin_files();
    assert_eq!(app.plugin_files_pending(), 0);
    assert!(app.plugin_manager.as_ref().unwrap().is_running(ID));
    assert!(
        !tasty_utils::path::tasty_home()
            .unwrap()
            .join("plugins.toml")
            .exists()
    );
}

#[test]
fn missing_manager_preserves_toggle_error_and_original_rpc_id() {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    app.plugin_manager = None;
    for method in ["plugin.enable", "plugin.disable"] {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let response = crate::ipc::handler::plugin::dispatch_lifecycle_toggle(
            &mut app,
            method,
            serde_json::json!(99),
            &serde_json::json!({"id":ID}),
        )
        .unwrap();
        app.services.send_handled(&tx, response);
        app.finish_plugin_files();
        app.services.poll_memory_replies(None);
        let response = rx.try_recv().unwrap();
        assert_eq!(response.id, serde_json::json!(99));
        let error = response.error.unwrap();
        assert_eq!(error.code, -32000);
        assert_eq!(
            error.message,
            format!(
                "{} failed: plugin manager not initialized",
                method.trim_start_matches("plugin.")
            )
        );
    }
}

#[test]
fn post_commit_registration_loss_preserves_saved_state_and_original_reply_once() {
    post_commit_reply(false, true);
}
#[test]
fn post_commit_removal_loss_preserves_saved_and_prior_effects_in_original_reply_once() {
    post_commit_reply(true, true);
}
#[test]
fn post_commit_partial_removal_loss_preserves_unsaved_final_config_and_prior_effects() {
    post_commit_reply(true, false);
}
fn post_commit_reply(removed: bool, config_saved: bool) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = app();
    let home = tasty_utils::path::tasty_home().unwrap();
    let path = home.join("plugins.toml");
    let before = tasty_host_plugin::PluginsConfig::load_from(path.clone());
    let mut after = before.clone();
    after.disable("persisted-stage");
    let update =
        tasty_host_plugin::file_persistence::ConfigUpdate::between(&before, &after).unwrap();
    let directory = home.join("owned-post-commit");
    if removed {
        std::fs::create_dir(&directory).unwrap();
    }
    if !config_saved {
        std::fs::write(&path, "PRIVATE = [[[\n").unwrap();
    }
    let reply = Reply::default();
    let done = reply.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(2);
    let original_id = serde_json::json!("post-commit-original");
    let response = app.plugin_file_reply(original_id.clone(), reply);
    app.services.send_handled(&tx, response);
    app.plugin_manager = None;
    let work_dir = directory.clone();
    app.run_plugin_file(
        (128, 0),
        move || {
            if removed {
                std::fs::remove_dir_all(&work_dir).unwrap();
            } else {
                std::fs::create_dir(&work_dir).unwrap();
            }
            update.execute().is_ok()
        },
        |_| 1,
        move |saved| command::CommandDisposition {
            outcome: if *saved {
                command::CommandOutcome::KnownSuccess
            } else {
                command::CommandOutcome::KnownFailure
            },
            partial: removed && !*saved,
        },
        move |app, result| {
            let result = result.and_then(|saved| {
                let stage = if removed {
                    post_commit::Stage::Removed {
                        config_saved: saved,
                    }
                } else {
                    post_commit::Stage::Registration
                };
                post_commit::manager_after_files(&mut app.plugin_manager, stage)
                    .map(|_| serde_json::json!({}))
            });
            done.finish(result);
        },
    );
    assert!(rx.try_recv().is_err());
    let deadline = Instant::now() + Duration::from_secs(5);
    let response = loop {
        app.services.poll_file_writer();
        app.finish_plugin_files();
        app.services.poll_memory_replies(None);
        if let Ok(response) = rx.try_recv() {
            break response;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(response.id, original_id);
    let error = response.error.unwrap();
    assert_eq!(error.code, -32000);
    let data = error.data.unwrap();
    assert_eq!(data["saved"], config_saved);
    assert_eq!(data["applied"], removed);
    assert_eq!(data["partial"], true);
    assert_eq!(data["files_committed"], config_saved);
    assert_eq!(directory.exists(), !removed);
    for _ in 0..3 {
        app.services.poll_file_writer();
        app.finish_plugin_files();
        app.services.poll_memory_replies(None);
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn secondary_queue_cannot_retain_sixty_four_mebibytes_before_writer_admission() {
    let mut files = PluginFiles::default();
    let payload_size = 1024 * 1024;
    let mut accepted = 0;
    for _ in 0..64 {
        let payload = vec![7_u8; payload_size];
        if files
            .enqueue_sized(payload.len(), move |_| {
                drop(std::hint::black_box(payload));
            })
            .is_err()
        {
            break;
        }
        accepted += 1;
    }
    assert!(accepted > 0);
    assert!(
        accepted * payload_size <= tasty_utils::batch_writer::Limits::default().queue_bytes,
        "secondary queue retained {} payload bytes before writer admission",
        accepted * payload_size
    );
}

#[test]
fn discarded_secondary_queue_releases_payload_reservations() {
    let mut files = PluginFiles::default();
    let payload = vec![9_u8; 1024 * 1024];
    files
        .enqueue_sized(payload.len(), move |_| drop(std::hint::black_box(payload)))
        .unwrap();
    assert!(files.budget.bytes() >= 1024 * 1024);
    files.queue.clear();
    assert_eq!(files.budget.bytes(), 0);
}
