//! Actual namespace lock admission with populated private roots and an owned worker.
use super::*;
use crate::host_api::webview::http_endpoint::runtime::tests::HANG_LIMIT;
use std::{
    fs::{self, File, OpenOptions},
    os::{fd::AsRawFd, unix::fs::PermissionsExt},
    sync::{
        atomic::AtomicUsize,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
#[derive(Debug, PartialEq)]
enum Event {
    Active,
    Wake,
    Done,
}
#[derive(Clone, Copy)]
enum Phase {
    Sweep,
    Create,
    Unheld,
}
struct HeldAdmission {
    home: tempfile::TempDir,
    lock: Arc<Mutex<Option<File>>>,
    current: Arc<AtomicBool>,
    receipt: Option<Preparation>,
    worker: Option<JoinHandle<()>>,
    events: Receiver<Event>,
}
pub(super) fn hold(parent: &std::path::Path) -> File {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(parent.join(".profile-lock"))
        .unwrap();
    setup_positive(|| {
        // SAFETY: the fixture owns this live descriptor; flock accesses no Rust memory.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        Err(if error.kind() == std::io::ErrorKind::WouldBlock {
            ProfileError::Busy
        } else {
            ProfileError::Io(error)
        })
    });
    lock
}
// Parallel owned fork fixtures can inherit an unrelated setup lock description. Retry
// only typed Busy while seeding this positive fixture, never Capacity or a refusal control.
fn setup_positive<T>(mut operation: impl FnMut() -> Result<T, ProfileError>) -> T {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match operation() {
            Ok(value) => return value,
            Err(ProfileError::Busy) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("positive admission fixture setup: {error}"),
        }
    }
}
impl HeldAdmission {
    fn start(phase: Phase, roots: usize, timeout: Duration) -> Self {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let parent =
            namespace::prepare(&NamespaceRoot::AppHome(home.path().to_path_buf())).unwrap();
        for _ in 0..roots {
            drop(setup_positive(|| PreparedProfile::create(&parent, 8)).into_native());
        }
        let lock = Arc::new(Mutex::new(if matches!(phase, Phase::Sweep) {
            Some(hold(&parent))
        } else {
            None
        }));
        let current = Arc::new(AtomicBool::new(true));
        let (tx, events) = mpsc::channel();
        let active = tx.clone();
        let done = tx.clone();
        let lock_on_gap = lock.clone();
        let current_check = current.clone();
        let calls = AtomicUsize::new(0);
        let (receipt, work) = Preparation::new(
            home.path().to_owned(),
            8,
            timeout,
            Arc::new(move || {
                // The second production current check follows the real sweep's lock release.
                if calls.fetch_add(1, Ordering::AcqRel) == 1 && matches!(phase, Phase::Create) {
                    *lock_on_gap.lock().unwrap() = Some(hold(&parent));
                }
                active.send(Event::Active).unwrap();
                current_check.load(Ordering::Acquire)
            }),
            Arc::new(move || {
                tx.send(Event::Wake).unwrap();
            }),
        );
        Self {
            home,
            lock,
            current,
            receipt: Some(receipt),
            worker: Some(thread::spawn(move || {
                work.run();
                done.send(Event::Done).unwrap();
            })),
            events,
        }
    }
    /// Slow storage has no deadline here: the worker sends every event before it ends, so its end
    /// is read first, and an ended worker with nothing left to read cannot send the one awaited.
    fn event(&self) -> Event {
        let deadline = Instant::now() + HANG_LIMIT;
        loop {
            let ended = self.worker.as_ref().is_none_or(JoinHandle::is_finished);
            match self.events.recv_timeout(Duration::from_millis(1)) {
                Ok(event) => return event,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("profile worker: lost, its events are gone")
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            assert!(!ended, "profile worker: lost, it ended without this event");
            assert!(
                Instant::now() < deadline,
                "profile worker: hung, no event within {HANG_LIMIT:?}"
            );
        }
    }
    fn contended(&self, phase: Phase) {
        assert_eq!(self.event(), Event::Active);
        if matches!(phase, Phase::Create) {
            assert_eq!(self.event(), Event::Active);
        }
        let next = self.event();
        assert_eq!(
            next,
            Event::Active,
            "lock contention published a terminal result instead of waiting: {:?}",
            self.receipt.as_ref().unwrap().poll().map(|r| r.err())
        );
        assert!(self.lock.lock().unwrap().is_some());
        assert!(matches!(
            PreparedProfile::create(&self.home.path().join("native-http-profiles"), 8),
            Err(ProfileError::Busy)
        ));
        assert_eq!(self.roots(), 3);
        assert!(self.receipt.as_ref().unwrap().poll().is_none());
    }
    fn release(&self) {
        drop(self.lock.lock().unwrap().take());
    }
    fn until(&self, wanted: Event) {
        let deadline = Instant::now() + HANG_LIMIT;
        while self.event() != wanted {
            assert!(
                Instant::now() < deadline,
                "profile worker: hung, the awaited event did not come within {HANG_LIMIT:?}"
            );
        }
    }
    fn roots(&self) -> usize {
        fs::read_dir(self.home.path().join("native-http-profiles"))
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().file_name() != ".profile-lock")
            .count()
    }
}
impl Drop for HeldAdmission {
    fn drop(&mut self) {
        drop(self.receipt.take());
        self.release();
        // A test that already failed leaves its worker: a second panic here would abort the run.
        if let Some(worker) = self.worker.take()
            && !thread::panicking()
        {
            super::tests::joined(worker);
        }
    }
}
fn recovery(phase: Phase) {
    let fixture = HeldAdmission::start(phase, 3, MAX_CLAIM_WAIT);
    fixture.contended(phase);
    fixture.release();
    fixture.until(Event::Wake);
    let profile = fixture
        .receipt
        .as_ref()
        .unwrap()
        .poll()
        .unwrap()
        .unwrap_or_else(|error| panic!("admission after release: {error}"));
    assert_eq!(fixture.roots(), 4);
    drop(profile.into_native());
    fixture.until(Event::Done);
}
#[test]
fn held_namespace_lock_admits_real_work_after_release_below_hard_cap() {
    recovery(Phase::Sweep);
}
#[test]
fn lock_acquired_after_real_sweep_does_not_terminally_fail_creation() {
    recovery(Phase::Create);
}
#[test]
fn genuine_eight_root_capacity_is_terminal_and_preserves_live_roots() {
    let fixture = HeldAdmission::start(Phase::Unheld, 8, MAX_CLAIM_WAIT);
    fixture.until(Event::Done);
    assert!(matches!(
        fixture.receipt.as_ref().unwrap().poll(),
        Some(Err(PrepareError::Storage(ProfileError::Capacity)))
    ));
    assert_eq!(fixture.roots(), 8);
    assert!(fixture.receipt.as_ref().unwrap().poll().is_none());
}
#[test]
fn held_lock_has_finite_admission_deadline_and_no_late_allocation() {
    let fixture = HeldAdmission::start(Phase::Sweep, 3, Duration::from_millis(100));
    fixture.contended(Phase::Sweep);
    fixture.until(Event::Done);
    assert!(matches!(
        fixture.receipt.as_ref().unwrap().poll(),
        Some(Err(PrepareError::AdmissionTimeout))
    ));
    fixture.release();
    assert_eq!(fixture.roots(), 3);
}
#[test]
fn dropped_receipt_cancels_wait_before_lock_release() {
    let mut fixture = HeldAdmission::start(Phase::Sweep, 3, Duration::from_secs(30));
    fixture.contended(Phase::Sweep);
    drop(fixture.receipt.take());
    fixture.until(Event::Done);
    assert!(fixture.lock.lock().unwrap().is_some());
    fixture.release();
    assert_eq!(fixture.roots(), 3);
}
#[test]
fn current_only_revocation_cancels_wait_without_condvar_notification() {
    let fixture = HeldAdmission::start(Phase::Sweep, 3, Duration::from_secs(30));
    fixture.contended(Phase::Sweep);
    fixture.current.store(false, Ordering::Release);
    fixture.until(Event::Done);
    assert!(fixture.lock.lock().unwrap().is_some());
    assert!(matches!(
        fixture.receipt.as_ref().unwrap().poll(),
        Some(Err(PrepareError::Retired))
    ));
    fixture.release();
    assert_eq!(fixture.roots(), 3);
}
#[test]
fn poisoned_admission_state_is_not_recovered_or_retried() {
    let fixture = HeldAdmission::start(Phase::Sweep, 3, Duration::from_secs(30));
    fixture.contended(Phase::Sweep);
    let shared = fixture.receipt.as_ref().unwrap().shared.clone();
    assert!(
        thread::spawn(move || {
            let _guard = shared.state.lock().unwrap();
            panic!("owned poison");
        })
        .join()
        .is_err()
    );
    fixture.until(Event::Done);
    assert!(matches!(
        fixture.receipt.as_ref().unwrap().poll(),
        Some(Err(PrepareError::Worker))
    ));
    fixture.release();
    assert_eq!(fixture.roots(), 3);
}

#[test]
fn actual_worker_refuses_unsafe_ancestry_and_symlink_without_modifying_them() {
    for symlink in [false, true] {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, b"retain").unwrap();
        if symlink {
            std::os::unix::fs::symlink(outside.path(), home.path().join("native-http-profiles"))
                .unwrap();
        } else {
            fs::set_permissions(home.path(), fs::Permissions::from_mode(0o775)).unwrap();
        }
        let (receipt, work) = Preparation::new(
            home.path().to_path_buf(),
            8,
            Duration::from_secs(2),
            Arc::new(|| true),
            Arc::new(|| {}),
        );
        thread::spawn(move || work.run()).join().unwrap();
        assert!(matches!(
            receipt.poll(),
            Some(Err(PrepareError::Storage(_)))
        ));
        assert_eq!(fs::read(sentinel).unwrap(), b"retain");
        if symlink {
            assert!(home.path().join("native-http-profiles").is_symlink());
        } else {
            assert_eq!(
                fs::metadata(home.path()).unwrap().permissions().mode() & 0o777,
                0o775
            );
            assert!(!home.path().join("native-http-profiles").exists());
        }
    }
}

#[test]
fn retirement_after_allocation_returns_unpublished_root_to_worker_before_publication() {
    let home = tempfile::tempdir().unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let current = calls.clone();
    let (receipt, work) = Preparation::new(
        home.path().to_path_buf(),
        8,
        Duration::from_secs(2),
        Arc::new(move || current.fetch_add(1, Ordering::AcqRel) < 2),
        Arc::new(|| {}),
    );
    thread::spawn(move || work.run()).join().unwrap();
    assert!(calls.load(Ordering::Acquire) >= 3);
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
    assert_eq!(
        fs::read_dir(home.path().join("native-http-profiles"))
            .unwrap()
            .count(),
        1
    );
}
