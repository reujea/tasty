use super::*;
use crate::app::{plugin_file_lifecycle::shutdown_tests as cases, preset_shutdown_tests::make_gui};
fn start(app: &mut App, file_drain: crate::app::writer_drain::WriterDrain) {
    app.stop_plugin_file_producers();
    app.state.shutdown = Some(ShutdownState {
        exit_code: 0,
        save_final_view: false,
        phase: ShutdownPhase::StoppingPlugins,
        final_view_sequence: None,
        port_scan_deadline: None,
        profile_detection_deadline: None,
        screenshot_deadline: None,
        explorer_files_deadline: None,
        runner_stop_deadline: Instant::now(),
        observer_drain: Default::default(),
        observer_started: None,
        observer_reported: false,
        memory_drain: Default::default(),
        state_drain: Default::default(),
        file_drain,
        journal_deadline: None,
    });
}
fn drain(app: &mut App, file_drain: crate::app::writer_drain::WriterDrain) {
    start(app, file_drain);
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        app.reject_pending_ipc();
        if !app.wait_for_file_writer_shutdown() {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn observer(app: &mut App) {
    start(app, Default::default());
    assert!(!app.wait_for_observer_shutdown());
}
const GUI_CHILD: &str = "TASTY_PLUGIN_FILE_GUI_SHUTDOWN_CHILD";

#[test]
fn plugin_file_shutdown_actual_app_pipeline_final_zero_and_timeout() {
    if let Some(marker) = std::env::var_os(GUI_CHILD) {
        gui_shutdown_body();
        std::fs::write(marker, std::process::id().to_string()).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("gui-completed");
    let name = format!(
        "{}::plugin_file_shutdown_actual_app_pipeline_final_zero_and_timeout",
        module_path!().strip_prefix("tasty::").unwrap()
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name.as_str(), "--exact", "--test-threads=1", "--nocapture"])
        .env(GUI_CHILD, &marker)
        .output()
        .expect("isolated GUI test subprocess");
    assert!(
        output.status.success(),
        "isolated GUI test failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let child: u32 = std::fs::read_to_string(marker)
        .expect("GUI body completion marker missing")
        .parse()
        .unwrap();
    assert_ne!(child, std::process::id());
    tracing::subscriber::with_default(
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .finish(),
        || tracing::info!("L1 isolated GUI body completed in owned child PID {child}"),
    );
}
fn gui_shutdown_body() {
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .finish();
    let _trace = tracing::subscriber::set_default(subscriber);
    let event_loop = crate::x11_test_loop::x11_event_loop::<crate::AppEvent>();
    let make = || make_gui(event_loop.create_proxy());
    let drain_engine = |app: &mut App, session| {
        app.engines
            .insert_pending(session)
            .unwrap_or_else(|_| panic!("owned engine slot"));
        drain(app, Default::default());
    };
    cases::mixed_child_webhook_and_plugin_pipeline(make, drain_engine);
    cases::ready_receipts_precede_l1_continuation(make, drain_engine);
    crate::app::preset_shutdown_tests::run_case(make, observer, false);
    crate::app::preset_shutdown_tests::run_case(make, observer, true);
    cases::remove_pipeline(make, drain);
    cases::final_receipt(make, drain, false);
    cases::final_receipt(make, drain, true);
    cases::active_unknown_and_queued_refusal(make, drain);
    cases::deadline_rechecks_ready_receipt(make);
    cases::retirement_has_an_independent_poll_wake(make);
    crate::app::plugin_file_lifecycle::native_tests::gui_only_consumers(make);
}
