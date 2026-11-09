//! Whole App and common writer checks; no replacement executor or copied product source.
use super::*;
use crate::app::writer_drain::{WRITER_DRAIN_TIMEOUT, WriterDrain};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
const ID: &str = "com.example.shutdown-files";
pub(crate) fn expired_drain() -> WriterDrain {
    let mut drain = WriterDrain::default();
    let start = Instant::now() - WRITER_DRAIN_TIMEOUT;
    assert!(drain.keep_waiting(1, start, "test pending"));
    assert!(!drain.keep_waiting(1, Instant::now(), "test expired"));
    drain
}
fn attach(app: &mut App) {
    let mut manager = crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        app.services.registries.file_format.clone(),
        app.services.registries.file_handler.clone(),
    );
    let _stub = manager.attach_namespace_stub_for_test(ID, "shutdownfiles");
    app.plugin_manager = Some(manager);
}
fn response(
    app: &mut App,
    id: u64,
) -> (Reply, mpsc::Receiver<tasty_ipc::protocol::JsonRpcResponse>) {
    let reply = Reply::default();
    let (tx, rx) = mpsc::sync_channel(2);
    let deferred = app.plugin_file_reply(serde_json::json!(id), reply.clone());
    app.services.send_handled(&tx, deferred);
    (reply, rx)
}
fn join_file(app: &mut App) {
    let end = Instant::now() + Duration::from_secs(5);
    while app.services.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
pub(crate) fn remove_pipeline(
    make: impl FnOnce() -> App,
    drain: impl FnOnce(&mut App, WriterDrain),
) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    attach(&mut app);
    let directory = tasty_utils::path::tasty_home()
        .unwrap()
        .join("plugins")
        .join(ID);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("owned"), "owned").unwrap();
    let (reply, rx) = response(&mut app, 808);
    let done = reply.clone();
    #[cfg(feature = "gui")]
    app.enqueue_plugin_remove(ID.into(), move |_, result| done.finish(result))
        .unwrap();
    #[cfg(not(feature = "gui"))]
    {
        // The headless product supports config toggles; exercise a real second file command
        // after their receipt without adding the GUI-only removal API to headless.
        app.enqueue_plugin_config(
            tasty_host_plugin::manager::file_config::ConfigChange::Enabled {
                id: ID.into(),
                enabled: false,
            },
            move |app, result| {
                if let Err(error) = result {
                    done.finish(Err(error));
                    return;
                }
                app.after_plugin_retired(ID.into(), move |app, retired| {
                    if let Err(error) = retired {
                        done.finish(Err(error));
                        return;
                    }
                    app.run_plugin_file(
                        (128, 0),
                        move || std::fs::remove_dir_all(directory).map_err(|e| e.to_string()),
                        |_| 8,
                        super::classify_result,
                        move |_, result| {
                            done.finish(
                                result
                                    .and_then(|result| result.map_err(Failure::before))
                                    .map(|()| serde_json::json!({"removed":ID})),
                            )
                        },
                    );
                });
            },
        )
        .unwrap();
    }
    app.stop_plugin_file_producers();
    drain(&mut app, Default::default());
    let answer = rx.try_recv().expect("original pipeline reply missing");
    assert_eq!(answer.id, serde_json::json!(808));
    assert_eq!(
        answer
            .result
            .as_ref()
            .unwrap_or_else(|| panic!("pipeline failed: {answer:?}"))["removed"],
        ID
    );
    assert!(
        !tasty_utils::path::tasty_home()
            .unwrap()
            .join("plugins")
            .join(ID)
            .exists()
    );
    assert_eq!(app.plugin_files_pending(), 0);
    assert!(rx.try_recv().is_err());
    join_file(&mut app);
    cleanup(&mut app);
}
pub(crate) fn final_receipt(
    make: impl FnOnce() -> App,
    drain: impl FnOnce(&mut App, WriterDrain),
    timeout: bool,
) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let (reply, rx) = response(&mut app, 809);
    app.run_plugin_file(
        (1, 0),
        || 11_u64,
        |_| 8,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |_, value| reply.finish(value.map(|value| serde_json::json!({"value":value}))),
    );
    let queue = app.services.file_command_queue().unwrap();
    let _cut = queue
        .flush(tasty_utils::batch_writer::FlushPoint::START)
        .unwrap();
    let (tx, gate) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    let later = timeout.then(|| {
        command::submit(
            Some(&queue),
            1,
            move || {
                entered.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(15)).unwrap();
            },
            |_| 1,
        )
        .unwrap()
    });
    if timeout {
        started.recv_timeout(Duration::from_secs(5)).unwrap();
    } else {
        join_file(&mut app);
    }
    assert!(
        rx.try_recv().is_err(),
        "App continuation still owns the original reply"
    );
    let drain_started = Instant::now();
    drain(
        &mut app,
        if timeout {
            expired_drain()
        } else {
            Default::default()
        },
    );
    if timeout {
        assert!(
            drain_started.elapsed() < WRITER_DRAIN_TIMEOUT / 2,
            "file deadline was reset"
        );
    }
    let answer = rx
        .try_recv()
        .expect("final receipt must precede drain return");
    assert_eq!(answer.id, serde_json::json!(809));
    assert_eq!(answer.result.unwrap()["value"], 11);
    if timeout {
        tx.send(()).unwrap();
    }
    drop(later);
    join_file(&mut app);
    app.collect_plugin_file_shutdown();
    assert!(rx.try_recv().is_err());
    cleanup(&mut app);
}
pub(crate) fn active_unknown_and_queued_refusal(
    make: impl FnOnce() -> App,
    drain: impl FnOnce(&mut App, WriterDrain),
) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let (first, first_rx) = response(&mut app, 810);
    let (second, second_rx) = response(&mut app, 811);
    let (tx, gate) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    app.run_plugin_file(
        (1, 0),
        move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(15)).unwrap();
        },
        |_| 1,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |_, result| first.finish(result.map(|()| serde_json::json!({}))),
    );
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        app.plugin_file_poll_deadline().is_none(),
        "worker receipt uses its wake, not periodic polling"
    );
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_ran = ran.clone();
    app.plugin_files
        .enqueue_sized(1, move |app| {
            app.run_plugin_file(
                (1, 0),
                move || worker_ran.store(true, std::sync::atomic::Ordering::SeqCst),
                |_| 1,
                |_| command::CommandDisposition {
                    outcome: command::CommandOutcome::KnownSuccess,
                    partial: false,
                },
                move |_, result| second.finish(result.map(|()| serde_json::json!({}))),
            )
        })
        .unwrap();
    app.stop_plugin_file_producers();
    let drain_started = Instant::now();
    drain(&mut app, expired_drain());
    assert!(
        drain_started.elapsed() < WRITER_DRAIN_TIMEOUT / 2,
        "pipeline deadline was reset before writer close"
    );
    let first = first_rx
        .try_recv()
        .expect("unknown active caller missing")
        .error
        .unwrap()
        .data
        .unwrap();
    assert_eq!(first["saved"], Value::Null);
    assert_eq!(first["outcome"], "unknown");
    let second = second_rx
        .try_recv()
        .expect("not-started caller missing")
        .error
        .unwrap()
        .data
        .unwrap();
    assert_eq!(second["saved"], false);
    assert_eq!(second["applied"], false);
    assert_eq!(
        app.plugin_files_pending(),
        1,
        "unknown must not become a joined-zero pipeline"
    );
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    tx.send(()).unwrap();
    join_file(&mut app);
    app.collect_plugin_file_shutdown();
    assert_eq!(app.plugin_files_pending(), 1);
    assert!(
        app.plugin_file_poll_deadline().is_none(),
        "terminal Unknown must not spin"
    );
    assert!(first_rx.try_recv().is_err());
    assert!(second_rx.try_recv().is_err());
    cleanup(&mut app);
}

pub(crate) fn deadline_rechecks_ready_receipt(make: impl FnOnce() -> App) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let (reply, rx) = response(&mut app, 812);
    app.run_plugin_file(
        (1, 0),
        || (),
        |_| 1,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |_, result| reply.finish(result.map(|()| serde_json::json!({"confirmed":true}))),
    );
    join_file(&mut app);
    assert!(rx.try_recv().is_err());
    app.stop_plugin_file_producers();
    app.expire_plugin_file_pipelines();
    assert_eq!(rx.try_recv().unwrap().result.unwrap()["confirmed"], true);
    assert_eq!(app.plugin_files_pending(), 0);
    app.collect_plugin_file_shutdown();
    assert!(rx.try_recv().is_err());
    cleanup(&mut app);
}

pub(super) fn cleanup(app: &mut App) {
    app.stop_plugin_file_producers();
    join_file(app);
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let memory = app.services.poll_memory_writer_shutdown();
        #[cfg(feature = "gui")]
        let state = app.services.poll_state_writer_shutdown();
        #[cfg(not(feature = "gui"))]
        let state = 0;
        app.collect_plugin_file_shutdown();
        if memory == 0 && state == 0 {
            break;
        }
        assert!(Instant::now() < end, "owned App test writer not joined");
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(crate) fn retirement_has_an_independent_poll_wake(make: impl FnOnce() -> App) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let (reply, rx) = response(&mut app, 813);
    app.run_plugin_file(
        (1, 0),
        || (),
        |_| 1,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |app, result| {
            assert!(result.is_ok());
            app.after_plugin_retired("owned-retirement".into(), move |_, retired| {
                reply.finish(retired.map(|()| serde_json::json!({"retired":true})))
            });
        },
    );
    join_file(&mut app);
    app.finish_plugin_files();
    let before = Instant::now();
    let deadline = app
        .plugin_file_poll_deadline()
        .expect("retirement must wake even without ordinary timers");
    assert!(deadline >= before);
    assert!(deadline <= Instant::now() + tasty_host_plugin::process::CHILD_EXIT_POLL_INTERVAL);
    app.collect_plugin_file_shutdown();
    assert_eq!(rx.try_recv().unwrap().result.unwrap()["retired"], true);
    assert!(app.plugin_file_poll_deadline().is_none());
    cleanup(&mut app);
}

/// C1 and W are admitted behind a held writer command, so neither can finish until `release`.
struct HeldReceipts {
    engine: crate::runtime::engine_session::EngineSession,
    child: crate::runtime::child_terminal::Ticket,
    webhook: crate::webhook::registry::Receipt,
    held: command::Ticket<()>,
    release: mpsc::Sender<()>,
}
fn hold_child_and_webhook(app: &App) -> HeldReceipts {
    use crate::runtime::child_terminal::ParentKey;
    use crate::webhook::lifetime::{Lifetime, Limit, Persistence};
    app.services.registries.child_terminals.wait_bootstrap();
    let mut engine = shared_engine(app);
    let parent = ParentKey::External("owned-composition".into());
    engine
        .live
        .occupancy
        .acquire_soft(313, parent.clone(), None)
        .unwrap();
    let queue = app.services.file_command_queue().unwrap();
    let (entered, started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let held = command::submit(
        Some(&queue),
        1,
        move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(10)).unwrap();
        },
        |_| 1,
    )
    .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let child = engine
        .runtime
        .child_terminals
        .release_with_effect(parent, 313)
        .unwrap();
    let mut webhook = app.services.webhooks.register(
        vec!["POST".into()],
        None,
        vec![],
        Lifetime {
            persistence: Persistence::Persistent,
            limit: Limit::Unlimited,
        },
        None,
    );
    assert!(child.observation().is_none());
    assert!(webhook.poll().is_none());
    HeldReceipts {
        engine,
        child,
        webhook,
        held,
        release,
    }
}

/// Fixes the per-round order: receipts that finished before a round are collected ahead of L1.
/// Every receipt is ready before the drain starts, so this does not depend on writer timing.
pub(crate) fn ready_receipts_precede_l1_continuation(
    make: impl FnOnce() -> App,
    drain: impl FnOnce(&mut App, crate::runtime::engine_session::EngineSession),
) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let HeldReceipts {
        engine,
        child,
        mut webhook,
        held,
        release,
    } = hold_child_and_webhook(&app);
    let (reply, rx) = response(&mut app, 816);
    app.run_plugin_file(
        (1, 0),
        || 23_u64,
        |_| 8,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |_, result| {
            assert!(
                child
                    .poll()
                    .expect("child live effect must precede L1 continuation")
                    .is_ok()
            );
            assert_eq!(
                webhook
                    .poll()
                    .expect("W receipt must precede L1 continuation")
                    .unwrap()
                    .saved,
                Some(true)
            );
            reply.finish(result.map(|value| serde_json::json!({"value":value})));
        },
    );
    // The writer publishes in admission order, so this later ticket proves C1, W and L1 are ready.
    let queue = app.services.file_command_queue().unwrap();
    let later = command::submit(Some(&queue), 1, || (), |_| 1).unwrap();
    release.send(()).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while later.poll().is_none() {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    assert!(held.poll().unwrap().is_ok());
    assert!(rx.try_recv().is_err());
    drain(&mut app, engine);
    let answer = rx
        .try_recv()
        .expect("original L1 reply must precede final drain return");
    assert_eq!(answer.result.unwrap()["value"], 23);
    cleanup(&mut app);
}

/// The writer runs while the drain collects, so a receipt may reach the round after L1 ran.
/// Nothing may be lost: each original reply still has to arrive before the drain returns.
pub(crate) fn mixed_child_webhook_and_plugin_pipeline(
    make: impl FnOnce() -> App,
    drain: impl FnOnce(&mut App, crate::runtime::engine_session::EngineSession),
) {
    let _home = crate::test_support::IsolatedHome::new();
    let mut app = make();
    let HeldReceipts {
        engine,
        child,
        mut webhook,
        held,
        release,
    } = hold_child_and_webhook(&app);
    let (webhook_tx, webhook_rx) = mpsc::sync_channel(2);
    let parked = app.services.reply_with_file(
        serde_json::json!(815),
        move || webhook.poll(),
        |_, completed| {
            let completed = completed.unwrap_or_else(|error| panic!("W failed {}", error.message));
            let crate::webhook::registry::Value::Registered(outcome) = completed.value else {
                panic!("W registration outcome expected");
            };
            tasty_ipc::protocol::JsonRpcResponse::success(
                serde_json::json!(815),
                serde_json::json!({"saved":completed.saved,"id":outcome.id}),
            )
        },
    );
    app.services.send_handled(&webhook_tx, parked);
    let (reply, rx) = response(&mut app, 814);
    let second_stage = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let executed = second_stage.clone();
    app.run_plugin_file(
        (1, 0),
        || (),
        |_| 1,
        |_| command::CommandDisposition {
            outcome: command::CommandOutcome::KnownSuccess,
            partial: false,
        },
        move |app, result| {
            result.unwrap();
            app.run_plugin_file(
                (1, 0),
                move || {
                    executed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    19_u64
                },
                |_| 8,
                |_| command::CommandDisposition {
                    outcome: command::CommandOutcome::KnownSuccess,
                    partial: false,
                },
                move |_, result| {
                    reply.finish(result.map(|value| serde_json::json!({"value":value})))
                },
            );
        },
    );
    let server = crate::adapters::test::mock_ipc_server::MockIpcServer::new();
    let injector = tasty_ipc::host_call::HostIpcInjector::new(server.tx_clone(), Arc::new(|| {}));
    app.hub.ipc_server = Some(Box::new(server));
    let incoming = std::thread::spawn(move || {
        let rejected = injector.dispatch(
            "notification.create",
            serde_json::json!({"body":"owned-composition"}),
            Duration::from_secs(5),
        );
        assert!(matches!(
            rejected,
            Err(tasty_ipc::host_call::InjectError::Rpc { code: -32000, .. })
        ));
        release.send(()).unwrap();
    });
    assert!(rx.try_recv().is_err());
    assert_eq!(second_stage.load(std::sync::atomic::Ordering::SeqCst), 0);
    app.services.webhooks.begin_shutdown();
    drain(&mut app, engine);
    incoming.join().unwrap();
    assert!(held.poll().unwrap().is_ok());
    let answer = rx
        .try_recv()
        .expect("original L1 reply must precede final drain return");
    assert_eq!(answer.id, serde_json::json!(814));
    assert_eq!(answer.result.unwrap()["value"], 19);
    assert!(rx.try_recv().is_err());
    assert!(
        child
            .poll()
            .expect("child live effect must settle before final drain return")
            .is_ok()
    );
    assert!(child.poll().is_none());
    let saved = webhook_rx
        .try_recv()
        .expect("original W reply must precede final drain return")
        .result
        .unwrap();
    assert_eq!(saved["saved"], true);
    let id = saved["id"].as_str().unwrap();
    assert!(app.services.webhooks.info(id).is_some());
    assert!(webhook_rx.try_recv().is_err());
    assert_eq!(second_stage.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(app.plugin_files_pending(), 0);
    assert_eq!(app.services.webhooks.poll_shutdown(), 0);
    assert!(
        app.plugin_files
            .enqueue_sized(1, |_| panic!("stopped L1 admission executed"))
            .is_err()
    );
    cleanup(&mut app);
}

pub(crate) fn shared_engine(app: &App) -> crate::runtime::engine_session::EngineSession {
    use crate::runtime::engine_session::{EngineSession, EngineSessionSpec};
    EngineSession::for_journal(
        EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker: Arc::new(|| {}),
            shared_ids: None,
            layout_slot: None,
            memory: app.services.memory_access().clone(),
            runner_registry: app.services.tasks.runner_registry().clone(),
        },
        crate::settings::Settings::default(),
        app.services.registries.clone(),
    )
    .unwrap()
}
