//! Unpublished cleanup on its own worker while something else holds the namespace lock.
use super::admission_tests::hold;
use super::tests::{joined, private_home, request, roots, woken};
use super::*;
use crate::host_api::webview::http_endpoint::runtime::tests::HANG_LIMIT;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicUsize,
    thread::{self, JoinHandle},
};

/// Counts what the worker's removal attempts answered.
#[derive(Default)]
pub(crate) struct DiscardAttempts {
    busy: AtomicUsize,
    refused: AtomicUsize,
}
impl DiscardAttempts {
    pub(super) fn record(&self, attempt: &Result<(), ProfileError>) {
        match attempt {
            Err(ProfileError::Busy) => &self.busy,
            Err(_) => &self.refused,
            Ok(()) => return,
        }
        .fetch_add(1, Ordering::AcqRel);
    }
    pub(crate) fn busy(&self) -> usize {
        self.busy.load(Ordering::Acquire)
    }
}
impl ProfileWork {
    pub(crate) fn discard_attempts(&self) -> Arc<DiscardAttempts> {
        self.discard_attempts.clone()
    }
    fn with_discard_wait(mut self, wait: Duration) -> Self {
        self.discard_wait = wait;
        self
    }
}

fn namespace(home: &Path) -> PathBuf {
    home.join("native-http-profiles")
}
/// Waits for a real Busy, or for the worker to have ended: a release before the first attempt would
/// pass without any retry.
fn first_busy(attempts: &DiscardAttempts, worker: &JoinHandle<()>) {
    let deadline = Instant::now() + HANG_LIMIT;
    while attempts.busy() == 0 && !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "profile worker: hung, no cleanup attempt within {HANG_LIMIT:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_dropped_receipt_cleanup_waits_out_a_held_lock() {
    let home = private_home();
    let (receipt, work, wake) =
        request(home.path(), Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let attempts = work.discard_attempts();
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let lock = hold(&namespace(home.path()));
    drop(receipt);
    first_busy(&attempts, &worker);
    drop(lock);
    joined(worker);
    assert!(attempts.busy() > 0, "the cleanup never met the held lock");
    assert!(
        roots(home.path()).is_empty(),
        "the cleanup gave up on the lock"
    );
}

#[test]
fn a_stale_receipt_answers_while_its_worker_waits_for_the_lock() {
    let home = private_home();
    let current = Arc::new(AtomicBool::new(true));
    let (receipt, work, wake) = request(home.path(), current.clone(), MAX_CLAIM_WAIT);
    let attempts = work.discard_attempts();
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let lock = hold(&namespace(home.path()));
    current.store(false, Ordering::Release);
    first_busy(&attempts, &worker);
    // Nothing that holds the lock can end here, so an answer must not depend on the wait.
    assert!(
        receipt.shared.state.try_lock().is_ok(),
        "the cleanup waits with the receipt's state locked"
    );
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
    drop(lock);
    joined(worker);
    assert!(
        roots(home.path()).is_empty(),
        "the cleanup gave up on the lock"
    );
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
}

#[test]
fn a_lock_held_past_the_wait_keeps_the_root_and_its_record() {
    let home = private_home();
    let (receipt, work, wake) =
        request(home.path(), Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let attempts = work.discard_attempts();
    let work = work.with_discard_wait(Duration::from_millis(20));
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let lock = hold(&namespace(home.path()));
    drop(receipt);
    joined(worker);
    assert!(attempts.busy() > 0);
    let left = roots(home.path());
    assert_eq!(left.len(), 1);
    assert!(left[0].join("owner.json").is_file());
    drop(lock);
}

#[test]
fn a_refused_cleanup_is_not_tried_again() {
    let home = private_home();
    let (receipt, work, wake) =
        request(home.path(), Arc::new(AtomicBool::new(true)), MAX_CLAIM_WAIT);
    let attempts = work.discard_attempts();
    // Long enough that retrying a refusal would show up, short enough to end if it does.
    let work = work.with_discard_wait(Duration::from_millis(200));
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let left = roots(home.path());
    let record = left[0].join("owner.json");
    let mut changed: serde_json::Value =
        serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    changed["kind"] = "not-this-owner".into();
    let changed = serde_json::to_vec(&changed).unwrap();
    fs::write(&record, &changed).unwrap();
    drop(receipt);
    joined(worker);
    // A parallel child can still answer Busy first; only the refusal count is exact.
    assert_eq!(attempts.refused.load(Ordering::Acquire), 1);
    assert_eq!(roots(home.path()), left);
    assert_eq!(fs::read(&record).unwrap(), changed);
}

/// The receipt learns of an expired claim while the cleanup still waits for the lock: the wake does
/// not follow the removal. A claim budget that ran out before this test held the lock, or an
/// admission that never reached the claim, leaves nothing to observe, so that attempt starts over.
#[test]
fn an_expired_claim_wakes_its_receipt_before_the_cleanup_ends() {
    let deadline = Instant::now() + HANG_LIMIT;
    loop {
        assert!(
            Instant::now() < deadline,
            "no attempt held the lock before the claim expired within {HANG_LIMIT:?}"
        );
        let home = private_home();
        let (receipt, work, wake) = request(
            home.path(),
            Arc::new(AtomicBool::new(true)),
            Duration::from_millis(200),
        );
        let worker = thread::spawn(move || work.run());
        woken(&wake, &worker);
        if roots(home.path()).is_empty() {
            joined(worker);
            continue;
        }
        let lock = hold(&namespace(home.path()));
        woken(&wake, &worker);
        if roots(home.path()).is_empty() {
            drop(lock);
            joined(worker);
            continue;
        }
        assert!(
            !worker.is_finished(),
            "the expired claim woke its receipt only after the cleanup ended"
        );
        assert!(matches!(receipt.poll(), Some(Err(PrepareError::Timeout))));
        drop(lock);
        joined(worker);
        assert!(roots(home.path()).is_empty());
        return;
    }
}
