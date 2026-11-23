//! File writer drain after the headless producers have stopped.
use crate::app::{App, writer_drain::WriterDrain};
use crate::runtime::engine_session::EngineSession;

#[cfg(all(test, target_os = "linux"))]
pub(super) fn drain(app: &mut App) {
    drain_with(app, Default::default());
}
pub(super) fn begin(app: &mut App, session: &mut EngineSession) -> WriterDrain {
    let mut drain = Default::default();
    drain_plugin_pipelines(app, &mut drain, &mut || {
        session.borrow_mut().poll_child_registry_effects()
    });
    drain
}
pub(super) fn drain_engine(app: &mut App, session: &mut EngineSession, drain: WriterDrain) {
    drain_with_engine_poll(app, drain, || {
        session.borrow_mut().poll_child_registry_effects()
    });
}
#[cfg(all(test, target_os = "linux"))]
pub(super) fn drain_with(app: &mut App, drain: WriterDrain) {
    drain_with_engine_poll(app, drain, || {});
}
pub(super) fn collect(app: &mut App, poll_engine: &mut impl FnMut()) {
    app.services.poll_file_writer();
    poll_engine();
    app.finish_preset_files();
    app.finish_plugin_files();
    crate::intent::headless::settle_settings_saves(&mut app.services);
    app.services
        .poll_memory_replies(app.plugin_manager.as_mut());
}
fn drain_plugin_pipelines(app: &mut App, drain: &mut WriterDrain, poll_engine: &mut impl FnMut()) {
    app.stop_plugin_file_producers();
    loop {
        reject_new_calls(app.hub.ipc_server.as_deref());
        collect(app, poll_engine);
        let pending = app.plugin_files_pending();
        if !drain.keep_waiting(
            pending,
            std::time::Instant::now(),
            "headless plugin file pipelines exceeded file shutdown deadline",
        ) {
            if pending != 0 {
                app.expire_plugin_file_pipelines();
                collect(app, poll_engine);
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn drain_with_engine_poll(app: &mut App, mut drain: WriterDrain, mut poll_engine: impl FnMut()) {
    drain_plugin_pipelines(app, &mut drain, &mut poll_engine);
    loop {
        reject_new_calls(app.hub.ipc_server.as_deref());
        let remaining = app.services.poll_file_writer_shutdown();
        collect(app, &mut poll_engine);
        if !drain.observe_writer(
            remaining,
            app.services.file_writer_shutdown_report(),
            "file",
            std::time::Instant::now(),
            "headless file writer shutdown timed out; settings it has not saved may be lost",
        ) {
            app.services.registries.child_terminals.finish_shutdown();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

// Match GUI shutdown: a saved HTTP request may still await its injected sequence's answer.
// Reject new work rather than dispatching into already retired engines or abandoning its owner.
fn reject_new_calls(server: Option<&dyn crate::ports::ipc_server::IpcServerPort>) {
    let Some(server) = server else {
        return;
    };
    while let Ok(cmd) = server.try_recv() {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        crate::ipc::server::send_response(
            &cmd.response_tx,
            crate::ipc::protocol::JsonRpcResponse::error(id, -32000, "host is shutting down"),
        );
    }
}

#[cfg(test)]
mod webhook_tests {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    #[test]
    fn a_saved_http_producer_gets_its_injected_call_answered_during_headless_drain() {
        let server = crate::adapters::test::mock_ipc_server::MockIpcServer::new();
        let injector =
            tasty_ipc::host_call::HostIpcInjector::new(server.tx_clone(), Arc::new(|| {}));
        let job = std::thread::spawn(move || {
            injector.dispatch(
                "notification.create",
                serde_json::json!({"body":"owned-shutdown"}),
                Duration::from_secs(5),
            )
        });
        let until = Instant::now() + Duration::from_secs(5);
        // reject_new_calls 는 새 호출에 거절 응답을 보내는 진행이라 기한 뒤 마지막 읽기에서는 부르지 않는다.
        assert!(
            tasty_utils::test_wait::holds_by_every(until, Duration::ZERO, |last| {
                job.is_finished() || {
                    if !last {
                        super::reject_new_calls(Some(&server));
                    }
                    false
                }
            }),
            "HTTP producer was left waiting during drain"
        );
        assert!(
            matches!(job.join().unwrap(),Err(tasty_ipc::host_call::InjectError::Rpc {code:-32000,message}) if message=="host is shutting down")
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod capture_file_tests {
    use crate::app::preset_shutdown_tests::{make_headless, run_case};
    #[test]
    fn app_capture_reply_is_collected_before_final_zero_return() {
        run_case(make_headless, super::drain, false);
    }
    #[test]
    fn app_capture_reply_is_collected_before_timeout_with_later_work() {
        run_case(make_headless, super::drain, true);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod plugin_file_shutdown_tests {
    use crate::app::{
        plugin_file_lifecycle::shutdown_tests as cases, preset_shutdown_tests::make_headless,
    };
    #[test]
    fn plugin_file_shutdown_retirement_wakes_independently() {
        cases::retirement_has_an_independent_poll_wake(make_headless);
    }
    #[test]
    fn plugin_file_shutdown_deadline_rechecks_ready_result() {
        cases::deadline_rechecks_ready_receipt(make_headless);
    }
    #[test]
    fn plugin_file_shutdown_pipeline_keeps_writer_open() {
        cases::remove_pipeline(make_headless, super::drain_with);
    }
    #[test]
    fn plugin_file_shutdown_collects_final_zero() {
        cases::final_receipt(make_headless, super::drain_with, false);
    }
    #[test]
    fn plugin_file_shutdown_collects_ready_reply_on_timeout() {
        cases::final_receipt(make_headless, super::drain_with, true);
    }
    #[test]
    fn plugin_file_shutdown_unknown_and_not_started_reply_once() {
        cases::active_unknown_and_queued_refusal(make_headless, super::drain_with);
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(super) mod composition_tests;
