//! Gate a completion after the consumer's first read, while an earlier wake is pending.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// Only bounds a failing wait. Passing rounds do not depend on it.
const SETTLE_WAIT: Duration = Duration::from_secs(30);

struct GatedReceipt {
    ready: Arc<AtomicBool>,
    gate: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}
impl PendingSave for GatedReceipt {
    fn poll(&self) -> Option<Result<(), String>> {
        let ready_at_read = self.ready.load(Ordering::Acquire);
        if let Some((release, finished)) = self.gate.lock().unwrap().take() {
            release.send(()).unwrap();
            finished.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        ready_at_read.then_some(Ok(()))
    }
}

/// Waits for the startup work queued by `build` and returns the wake counts to measure from.
///
/// The worker runs one cut at a time and notifies after each, so while this command runs every
/// earlier notice has returned. Acknowledging there makes the notice of this command's own batch
/// send a wake, and sending the wake is the last thing a notice does. Receiving it therefore
/// proves that no startup notice can still touch the latch, without timing the writer's window.
fn settle_startup_writes(
    writer: &tasty_utils::batch_writer::Handle<FileExecutor>,
    wake: &tasty_utils::batch_writer::WakeLatch,
    wake_rx: &mpsc::Receiver<()>,
) -> tasty_utils::batch_writer::WakeCounts {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let _ticket = crate::file_command::submit(
        Some(writer),
        0,
        move || {
            entered_tx.send(()).unwrap();
            // An error means the test already failed and dropped the sender.
            release_rx.recv().ok();
        },
        |_| 0,
    )
    .unwrap();
    entered_rx.recv_timeout(SETTLE_WAIT).unwrap();
    wake.acknowledge();
    while wake_rx.try_recv().is_ok() {}
    release_tx.send(()).unwrap();
    wake_rx.recv_timeout(SETTLE_WAIT).unwrap();
    wake.acknowledge();
    wake.counts()
}

fn gated_round(ack_first: bool) -> (bool, (u64, u64, usize), (u64, u64)) {
    let _home = crate::test_support::IsolatedHome::new();
    let dir = tempfile::tempdir().unwrap();
    let (wake_tx, wake_rx) = mpsc::channel();
    let parts = file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        tasty_utils::batch_writer::Limits::default(),
        Arc::new(tasty_utils::batch_writer::SystemClock),
        Arc::new(move || {
            wake_tx.send(()).unwrap();
        }),
    )
    .unwrap();
    let wake = parts.wake.clone();
    let writer = parts.writer.handle();
    let mut services = crate::app::services::builder::AppServicesBuilder::new()
        .with_clock(Arc::new(
            crate::adapters::test::fake_clock::FakeClock::default(),
        ))
        .with_clipboard(Arc::new(
            crate::adapters::test::mock_clipboard::MockClipboard::default(),
        ))
        .with_home(Arc::new(crate::adapters::test::tmp_home::TmpHome::new(
            dir.path().into(),
        )))
        .with_sound_player(Arc::new(crate::ports::notification_sound::NoopPlayer))
        .with_memory(Arc::new(Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        )))
        .with_themes(Arc::new(tasty_themes::ThemeStore::new()))
        .with_preset_store(Arc::new(Mutex::new(
            tasty_presets::PresetStore::load_default(),
        )))
        .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
        .with_file_writer(parts)
        .build()
        .unwrap();
    let before = settle_startup_writes(&writer, &wake, &wake_rx);
    let ready = Arc::new(AtomicBool::new(false));
    let (release_tx, release_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let completed = ready.clone();
    let notify = wake.notifier();
    let worker = std::thread::spawn(move || {
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        completed.store(true, Ordering::Release);
        notify();
        finished_tx.send(()).unwrap();
    });
    let receipt: Box<dyn PendingSave> = Box::new(GatedReceipt {
        ready,
        gate: Mutex::new(Some((release_tx, finished_rx))),
    });
    let slot = Mutex::new(Some(receipt));
    services
        .registries
        .port_favorites
        .install_sink(Arc::new(move |_| Ok(slot.lock().unwrap().take().unwrap())));
    services.registries.port_favorites.update(|list| {
        list.add("127.0.0.1".parse().unwrap(), 48901, "gated".into());
    });
    wake.notifier()(); // A prior completion has already queued this owner turn.
    wake_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    if ack_first {
        services.poll_file_writer(); // Exercise the production ordering.
    } else {
        poll(&services.registries); // Negative control: the previous production ordering.
        wake.acknowledge();
    }
    worker.join().unwrap();
    let queued = wake_rx.try_recv().is_ok();
    if queued {
        services.poll_file_writer();
    }
    let status = services.registries.port_favorites.save_status();
    let counts = wake.counts();
    let deadline = Instant::now() + Duration::from_secs(5);
    while services.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    (
        queued,
        status,
        (
            counts.notified - before.notified,
            counts.woken - before.woken,
        ),
    )
}

#[test]
fn receipt_read_racing_completion_keeps_a_wake_for_the_next_owner_turn() {
    let old = gated_round(false);
    assert_eq!(
        old,
        (false, (1, 0, 1), (2, 1)),
        "negative control must expose the coalesced wake"
    );
    let fixed = gated_round(true);
    assert_eq!(
        fixed,
        (true, (1, 1, 0), (2, 2)),
        "production must progress on the queued completion wake without a timer"
    );
}
