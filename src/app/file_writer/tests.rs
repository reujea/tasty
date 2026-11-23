use super::*;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};
use tasty_utils::test_wait::{holds_by, holds_by_every};

struct Gated {
    entered: mpsc::Sender<u64>,
    release: Mutex<mpsc::Receiver<bool>>,
}
impl SettingsStorage for Gated {
    fn load(&self) -> Settings {
        Settings::default()
    }
    fn save(&self, settings: &Settings) -> anyhow::Result<()> {
        self.entered.send(settings.remote_transfer.max_mb).unwrap();
        if !self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))?
        {
            anyhow::bail!("replacement failed");
        }
        Ok(())
    }
}
fn settings(value: u64) -> Settings {
    let mut settings = Settings::default();
    settings.remote_transfer.max_mb = value;
    settings
}
fn wait_saves(saves: &mut SettingsSaves, n: usize) -> Vec<SettledSave> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut results = Vec::new();
    // reap 은 끝난 영수증만 거두므로 기한 뒤 관측에서도 거둔 뒤 읽는다.
    assert!(
        holds_by_every(deadline, Duration::ZERO, |last| {
            if last {
                results.extend(saves.reap());
            }
            results.len() >= n || {
                if !last {
                    results.extend(saves.reap());
                }
                false
            }
        }),
        "receipt missing"
    );
    results
}
#[test]
fn older_completion_does_not_clear_newer_dirty_and_failure_keeps_it() {
    let (entered, received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let storage = Arc::new(Gated {
        entered,
        release: Mutex::new(released),
    });
    let mut parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut saves = SettingsSaves::new(Some(parts.writer.handle()), storage);
    saves.submit(settings(1), None, None);
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    saves.submit(settings(2), None, None);
    release.send(true).unwrap();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    assert!(wait_saves(&mut saves, 1)[0].saved.is_ok());
    assert_eq!(saves.unsaved(), Some((2, 1)));
    release.send(false).unwrap();
    assert!(wait_saves(&mut saves, 1)[0].saved.is_err());
    assert_eq!(saves.unsaved(), Some((2, 1)));
    saves.retry_latest_before_shutdown();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    release.send(true).unwrap();
    assert!(wait_saves(&mut saves, 1)[0].saved.is_ok());
    assert_eq!(saves.unsaved(), None);
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(holds_by(deadline, || parts.writer.poll_shutdown() == 0));
    assert_eq!(parts.stats.snapshot().replaced, 2);
    assert_eq!(parts.stats.snapshot().failed, 1);
}
#[test]
fn dynamic_settings_payload_counts_toward_admission_budget() {
    use batch_writer::Request;
    let small = FileRequest::Settings {
        revision: 1,
        settings: Box::new(settings(1)),
    };
    let mut large = settings(2);
    large.remote_transfer.dir = "x".repeat(100_000);
    let large = FileRequest::Settings {
        revision: 2,
        settings: Box::new(large),
    };
    assert!(large.bytes() > small.bytes() + 99_000);
}

#[test]
fn boot_receipt_waits_without_blocking_and_new_window_uses_latest_snapshot() {
    use crate::app::settings_change::{SettingsOutcome, SettingsPoll, pair};
    let (entered, received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let storage = Arc::new(Gated {
        entered,
        release: Mutex::new(released),
    });
    let parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut core = crate::ipc::handler::cli_entry_tests::test_core_builder()
        .with_settings_storage(storage)
        .with_file_writer(parts)
        .build()
        .unwrap();
    let (done, mut waiting) = pair(Arc::new(|| {}));
    let mut final_boot = settings(42);
    final_boot.general.shell = "/confirmed-shell".into();
    final_boot.appearance.theme = "latte".into();
    core.submit_settings_save(final_boot, None, Some(done));
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 42);
    assert!(matches!(waiting.poll(), SettingsPoll::Pending));
    // The disk has not been replaced; the next window still sees the accepted shell and theme.
    let snapshot = core.latest_settings().unwrap();
    assert_eq!(snapshot.general.shell, "/confirmed-shell");
    assert_eq!(snapshot.appearance.theme, "latte");
    core.poll_file_writer();
    assert!(matches!(waiting.poll(), SettingsPoll::Pending));
    release.send(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    // settle_settings_saves 는 끝난 저장 결과만 거둔다. poll 은 Ready 를 한 번만 주므로 결과를 붙잡는다.
    let mut outcome = None;
    assert!(holds_by_every(deadline, Duration::ZERO, |_| {
        crate::intent::headless::settle_settings_saves(&mut core);
        match waiting.poll() {
            SettingsPoll::Pending => false,
            other => {
                outcome = Some(other);
                true
            }
        }
    }));
    match outcome.unwrap() {
        SettingsPoll::Ready(SettingsOutcome::Applied {
            saved, settings, ..
        }) => {
            assert!(saved.is_ok());
            assert_eq!(settings.general.shell, "/confirmed-shell");
            assert_eq!(settings.appearance.theme, "latte");
        }
        _ => panic!("missing boot save outcome"),
    }
    assert!(matches!(waiting.poll(), SettingsPoll::Dropped));
    // holds_by 로 바꾸지 않는다. poll_file_writer_shutdown 은 child cleanup 제출과 webhook 처리를 함께
    // 진행해 기한 뒤 관측에서 부를 수 없고, 그것 없이 종료 완료를 볼 길이 없다.
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

mod dirty_age;
