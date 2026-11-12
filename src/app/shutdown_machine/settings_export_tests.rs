use super::*;
use crate::app::settings_files::{SettingsFileRequest, export::PendingExport};
use std::sync::{Arc, mpsc};

fn state(file_drain: crate::app::writer_drain::WriterDrain) -> ShutdownState {
    ShutdownState {
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
    }
}
#[test]
fn accepted_export_is_collected_at_final_zero_and_timeout_even_after_view_loss() {
    const CHILD_ENV: &str = "TASTY_SETTINGS_EXPORT_GUI_TEST_CHILD";
    const COMPLETE: &str = "final-zero-and-timeout-complete";
    if let Some(marker) = std::env::var_os(CHILD_ENV) {
        run_in_fresh_event_loop();
        std::fs::write(marker, COMPLETE).unwrap();
        return;
    }

    // winit permits only one EventLoop per process, even with one test thread.
    // Keep this App fixture independent of other GUI tests and reap its child.
    let test = format!(
        "{}::accepted_export_is_collected_at_final_zero_and_timeout_even_after_view_loss",
        module_path!().split_once("::").unwrap().1
    );
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("completed");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([test.as_str(), "--exact", "--test-threads=1"])
        .env(CHILD_ENV, &marker)
        .output()
        .expect("run Export App cases in a fresh test process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Export App child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(marker).unwrap_or_default(),
        COMPLETE,
        "Export App child did not finish both cases\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
fn run_in_fresh_event_loop() {
    let event_loop = crate::x11_test_loop::x11_event_loop::<crate::AppEvent>();
    for timeout in [false, true] {
        run_case(event_loop.create_proxy(), timeout);
    }
}
fn run_case(proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>, timeout: bool) {
    let _home = crate::test_support::IsolatedHome::new();
    let home = tasty_utils::path::tasty_home().unwrap();
    let mut app = crate::app::preset_shutdown_tests::make_gui(proxy);
    let path = home.join("export.toml");
    let held = tasty_utils::file_replace::lock(&path).unwrap();
    let life = Arc::new(());
    let request = SettingsFileRequest::Export {
        path: path.clone(),
        text: "owned export".into(),
        observer: Arc::downgrade(&life),
    };
    let queue = app.services.file_command_queue().unwrap();
    let pending = PendingExport::submit(Some(&queue), request).ok().unwrap();
    app.state
        .settings_exports
        .push((winit::window::WindowId::dummy(), pending));
    let _cut = queue
        .flush(tasty_utils::batch_writer::FlushPoint::START)
        .unwrap();
    let (release, released) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    let later = timeout.then(|| {
        crate::app::file_writer::command::submit(
            Some(&queue),
            128,
            move || {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(15)).unwrap();
            },
            |_| 0,
        )
        .unwrap()
    });
    drop(life); // No live View may observe the accepted write.
    assert!(app.state.settings_exports[0].1.poll().is_none());
    assert_ne!(app.services.poll_file_writer_shutdown(), 0);
    drop(held);
    let end = Instant::now() + Duration::from_secs(5);
    if timeout {
        started.recv_timeout(Duration::from_secs(5)).unwrap();
    } else {
        join_file(&mut app, end);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned export");
    assert_eq!(
        app.state.settings_exports.len(),
        1,
        "only App still owns the uncollected export"
    );
    let mut file_drain = crate::app::writer_drain::WriterDrain::default();
    if timeout {
        let now = Instant::now();
        assert!(file_drain.keep_waiting(1, now, "test timeout"));
        assert!(!file_drain.keep_waiting(
            1,
            now + crate::app::writer_drain::WRITER_DRAIN_TIMEOUT,
            "test timeout"
        ));
    }
    app.state.shutdown = Some(state(file_drain));
    assert!(!app.wait_for_file_writer_shutdown());
    assert!(
        app.state.settings_exports.is_empty(),
        "final return skipped the export receipt"
    );
    if timeout {
        release.send(()).unwrap();
    }
    join_file(&mut app, end);
    if let Some(later) = later {
        assert!(later.poll().unwrap().is_ok());
    }
    app.finish_settings_exports();
    assert!(app.state.settings_exports.is_empty());
    join_other(&mut app, end);
}

fn join_file(app: &mut App, end: Instant) {
    while app.services.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
fn join_other(app: &mut App, end: Instant) {
    while app.services.poll_memory_writer_shutdown() != 0
        || app.services.poll_state_writer_shutdown() != 0
    {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
