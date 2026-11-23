use super::*;
use batch_writer::{FlushPoint, ManualClock, SystemClock};

fn finish(parts: &mut FileWriterParts) {
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(holds_by(deadline, || parts.writer.poll_shutdown() == 0));
}

#[test]
fn first_applied_age_survives_updates_old_success_failure_and_retry() {
    let (entered, received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let storage = Arc::new(Gated {
        entered,
        release: Mutex::new(released),
    });
    let clock = Arc::new(ManualClock::default());
    let mut parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut saves = SettingsSaves::with_clock(Some(parts.writer.handle()), storage, clock.clone());
    let initial = saves.snapshot();
    saves.submit(settings(1), None, None);
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    clock.advance(Duration::from_micros(10));
    let first = saves.snapshot();
    saves.submit(settings(2), None, None);
    clock.advance(Duration::from_micros(20));
    let newer = saves.snapshot();
    release.send(true).unwrap();
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(wait_saves(&mut saves, 1)[0].saved.is_ok());
    let older_success = saves.snapshot();
    release.send(false).unwrap();
    assert!(wait_saves(&mut saves, 1)[0].saved.is_err());
    clock.advance(Duration::from_micros(40));
    let failed = saves.snapshot();
    saves.retry_latest_before_shutdown();
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    let retry = saves.snapshot();
    release.send(true).unwrap();
    assert!(wait_saves(&mut saves, 1)[0].saved.is_ok());
    let confirmed = saves.snapshot();
    finish(&mut parts);
    assert_eq!(initial.dirty_age_us, None);
    assert_eq!(first.dirty_age_us, Some(10));
    assert_eq!(newer.dirty_age_us, Some(30));
    assert_eq!((older_success.visible, older_success.committed), (2, 1));
    assert_eq!(older_success.dirty_age_us, Some(30));
    assert_eq!(failed.pending, 0);
    assert_eq!(failed.dirty_age_us, Some(70));
    assert_eq!(retry.dirty_age_us, Some(70));
    assert_eq!(confirmed.dirty_age_us, None);
}

#[test]
fn snapshot_does_not_consume_a_ready_success_receipt() {
    let (entered, received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let storage = Arc::new(Gated {
        entered,
        release: Mutex::new(released),
    });
    let clock = Arc::new(ManualClock::default());
    let mut parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut saves = SettingsSaves::with_clock(Some(parts.writer.handle()), storage, clock.clone());
    saves.submit(settings(1), None, None);
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(true).unwrap();
    let barrier = parts.writer.handle().flush(FlushPoint::START).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(holds_by(deadline, || barrier.try_take().is_some()));
    clock.advance(Duration::from_micros(123));
    let ready = saves.snapshot();
    let repeated = saves.snapshot();
    let results = saves.reap();
    let confirmed = saves.snapshot();
    finish(&mut parts);
    assert_eq!(ready, repeated);
    assert_eq!((ready.visible, ready.committed, ready.pending), (1, 0, 1));
    assert_eq!(ready.dirty_age_us, Some(123));
    assert_eq!(results.len(), 1);
    assert!(results[0].saved.is_ok());
    assert_eq!(confirmed.dirty_age_us, None);
}

struct FailsOnce(std::sync::atomic::AtomicUsize);
impl SettingsStorage for FailsOnce {
    fn load(&self) -> Settings {
        Settings::default()
    }
    fn save(&self, _: &Settings) -> anyhow::Result<()> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("unknown replacement outcome");
        }
        Ok(())
    }
}

#[test]
fn unknown_outcome_keeps_age_until_latest_retry_is_confirmed() {
    let storage = Arc::new(FailsOnce(std::sync::atomic::AtomicUsize::new(0)));
    let clock = Arc::new(ManualClock::default());
    let mut parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut saves = SettingsSaves::with_clock(Some(parts.writer.handle()), storage, clock.clone());
    saves.submit(settings(1), None, None);
    let result = wait_saves(&mut saves, 1);
    clock.advance(Duration::from_micros(321));
    let unknown = saves.snapshot();
    saves.retry_latest_before_shutdown();
    let retry = wait_saves(&mut saves, 1);
    let confirmed = saves.snapshot();
    finish(&mut parts);
    assert!(result[0].saved.as_ref().unwrap_err().contains("unknown"));
    assert_eq!(unknown.pending, 0);
    assert_eq!(unknown.dirty_age_us, Some(321));
    assert!(retry[0].saved.is_ok());
    assert_eq!(confirmed.dirty_age_us, None);
}

struct InlineFailure;
impl SettingsStorage for InlineFailure {
    fn load(&self) -> Settings {
        Settings::default()
    }
    fn save(&self, _: &Settings) -> anyhow::Result<()> {
        anyhow::bail!("replacement refused")
    }
}

#[test]
fn inline_failure_and_closed_admission_remain_dirty_without_pending_work() {
    let storage = Arc::new(InlineFailure);
    let clock = Arc::new(ManualClock::default());
    let mut inline = SettingsSaves::with_clock(None, storage.clone(), clock.clone());
    inline.submit(settings(1), None, None);
    clock.advance(Duration::from_micros(5));
    let failed = inline.snapshot();
    let mut parts = start(
        storage.clone(),
        Limits::default(),
        Arc::new(SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    parts.writer.close();
    let mut closed = SettingsSaves::with_clock(Some(parts.writer.handle()), storage, clock.clone());
    closed.submit(settings(1), None, None);
    clock.advance(Duration::from_micros(7));
    let refused = closed.snapshot();
    finish(&mut parts);
    assert_eq!(failed.pending, 0);
    assert_eq!(failed.dirty_age_us, Some(5));
    assert_eq!(refused.pending, 0);
    assert_eq!(refused.dirty_age_us, Some(7));
    assert!(closed.reap()[0].saved.is_err());
    assert!(inline.reap()[0].saved.is_err());
}

#[test]
fn inline_confirmation_clears_age_and_next_dirty_period_starts_at_zero() {
    let (entered, _received) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let storage = Arc::new(Gated {
        entered,
        release: Mutex::new(released),
    });
    let clock = Arc::new(ManualClock::default());
    let mut saves = SettingsSaves::with_clock(None, storage, clock.clone());
    release.send(true).unwrap();
    saves.submit(settings(1), None, None);
    clock.advance(Duration::from_micros(999));
    assert_eq!(saves.snapshot().dirty_age_us, None);
    release.send(false).unwrap();
    saves.submit(settings(2), None, None);
    assert_eq!(saves.snapshot().dirty_age_us, Some(0));
    clock.advance(Duration::from_micros(13));
    assert_eq!(saves.snapshot().dirty_age_us, Some(13));
    release.send(true).unwrap();
    saves.submit(settings(3), None, None);
    assert_eq!(saves.snapshot().dirty_age_us, None);
    clock.advance(Duration::from_micros(999));
    release.send(false).unwrap();
    saves.submit(settings(4), None, None);
    assert_eq!(saves.snapshot().dirty_age_us, Some(0));
    assert_eq!(
        (saves.snapshot().visible, saves.snapshot().committed),
        (4, 3)
    );
    assert_eq!(saves.reap().len(), 4);
}
