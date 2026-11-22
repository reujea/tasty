use super::*;
use std::sync::{Arc, Mutex, mpsc};

#[test]
fn invalid_file_boot_uses_and_persists_one_normalized_snapshot() {
    // The production file worker needs a process-wide synthetic home, not a thread override.
    let _home = crate::test_support::TastyHomeGuard::new();
    let path = crate::settings::Settings::config_path().unwrap();
    std::fs::write(
        &path,
        "[anomaly_detection]\nslow_loop_threshold=1\nrss_surge_samples=3\nrss_surge_min_mib=0\nrss_surge_notify=false\nslow_loop_notify=true\ncall_burst_notify=false\n",
    )
    .unwrap();
    let settings = load_headless_settings();
    assert_eq!(
        settings.anomaly_detection,
        tasty_settings::AnomalyDetectionSettings::default()
    );
    let (tx, _events) = mpsc::channel();
    let waker = crate::adapters::production::headless_waker::HeadlessWaker::new(tx);
    let memory = Arc::new(Mutex::new(
        tasty_memory::MemoryStore::open_in_memory().unwrap(),
    ));
    let mut app =
        crate::app::App::new_headless(waker.journal_waker(), None, Some(memory), None).unwrap();
    let mut session = bootstrap_engine(&mut app, &settings, waker.waker_factory()).unwrap();
    assert_eq!(
        session.runtime.settings.anomaly_detection,
        settings.anomaly_detection
    );
    for i in 0..5 {
        assert!(
            app.services
                .anomaly_detector
                .record_rss_sample("small", 100 + i, i, i)
                .is_none()
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.services.poll_file_writer();
        let persisted: crate::settings::Settings =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        if persisted.anomaly_detection == settings.anomaly_detection {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "normalized file was not saved"
        );
        std::thread::yield_now();
    }
    let mut next = session.runtime.settings.clone();
    next.general.language = "ko".into();
    assert!(
        next.validate_runtime().is_ok(),
        "an unrelated live edit remains valid"
    );
    let mut state = crate::state::CommandContext::new(
        &session.as_ref().read(),
        app.services.preset_store.clone(),
    );
    crate::intent::headless::apply_one_for_test(
        &mut app.services,
        &mut state,
        &mut session.borrow_mut(),
        crate::app::command::DomainIntent::UpdateSettings(next).from_agent_ipc(),
    );
    assert_eq!(session.runtime.settings.general.language, "ko");
    finish_headless_shutdown(&mut app, &mut session);
}
