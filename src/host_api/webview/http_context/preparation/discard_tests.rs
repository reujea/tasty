//! Unpublished cleanup on its own worker while something else holds the namespace lock.
use super::admission_tests::hold;
use super::tests::{joined, private_home, request, roots, woken};
use super::*;
use crate::host_api::webview::http_endpoint::runtime::tests::HANG_LIMIT;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{OnceLock, atomic::AtomicUsize, mpsc},
    thread::{self, JoinHandle},
};

/// Counts what the worker's removal attempts answered.
#[derive(Default)]
pub(crate) struct DiscardAttempts {
    busy: AtomicUsize,
    refused: AtomicUsize,
    /// Any error other than Busy ends the cleanup, so the first one is the one that kept the root.
    first_refusal: OnceLock<String>,
}
impl DiscardAttempts {
    pub(super) fn record(&self, attempt: &Result<(), ProfileError>) {
        match attempt {
            Err(ProfileError::Busy) => &self.busy,
            Err(error) => {
                self.first_refusal.get_or_init(|| error.to_string());
                &self.refused
            }
            Ok(()) => return,
        }
        .fetch_add(1, Ordering::AcqRel);
    }
    pub(crate) fn busy(&self) -> usize {
        self.busy.load(Ordering::Acquire)
    }
    pub(crate) fn refused(&self) -> usize {
        self.refused.load(Ordering::Acquire)
    }
    pub(crate) fn first_refusal(&self) -> Option<&str> {
        self.first_refusal.get().map(String::as_str)
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
    /// Keeps a published success from starting its claim deadline until the returned sender drops,
    /// so no claim can expire before the test holds the lock. The sender carries no value it could
    /// send, so dropping it is the only release.
    pub(crate) fn with_claim_gate(mut self) -> (Self, mpsc::Sender<std::convert::Infallible>) {
        let (open, gate) = mpsc::channel();
        self.claim_gate = Some(gate);
        (self, open)
    }
}
impl DiscardAttempts {
    /// What the cleanup was refused with, for the failure messages of a cleanup that met no Busy
    /// or kept its root.
    pub(crate) fn refusals(&self) -> String {
        format!(
            "{} refused attempt(s), first: {:?}",
            self.refused(),
            self.first_refusal()
        )
    }
}
impl Preparation {
    /// Whether the published result is a prepared profile, read without taking it: a poll would
    /// claim the profile and leave no unpublished cleanup to observe. The error names what was
    /// published instead.
    pub(crate) fn prepared(&self) -> Result<(), String> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| "poisoned state".to_owned())?;
        match &*state {
            State::Ready(Ok(_)) => Ok(()),
            State::Ready(Err(error)) => Err(error.to_string()),
            State::Pending => Err("nothing published".into()),
            State::Taken => Err("already taken".into()),
            State::Cancelled => Err("cancelled".into()),
        }
    }
}

fn namespace(home: &Path) -> PathBuf {
    home.join("native-http-profiles")
}
/// Waits for a real Busy, or for the worker to have ended: a release before the first attempt would
/// pass without any retry.
fn first_busy(attempts: &DiscardAttempts, worker: &JoinHandle<()>) {
    let deadline = Instant::now() + HANG_LIMIT;
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || attempts.busy() != 0 || worker.is_finished()),
        "profile worker: hung, no cleanup attempt within {HANG_LIMIT:?}"
    );
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
    let (work, gate) = work.with_claim_gate();
    let worker = thread::spawn(move || work.run());
    woken(&wake, &worker);
    let lock = hold(&namespace(home.path()));
    current.store(false, Ordering::Release);
    drop(gate);
    first_busy(&attempts, &worker);
    // Nothing that holds the lock can end here, so an answer must not depend on the wait.
    assert!(
        receipt.shared.state.try_lock().is_ok(),
        "the cleanup waits with the receipt's state locked"
    );
    assert!(matches!(receipt.poll(), Some(Err(PrepareError::Retired))));
    drop(lock);
    joined(worker);
    // A worker that ended before any Busy cannot meet the lock after its release either.
    assert!(
        attempts.busy() > 0,
        "the worker ended with no Busy seen: {}; with none refused, one possible cause is a \
         preparation that failed after it left the lock file",
        attempts.refusals()
    );
    assert!(
        roots(home.path()).is_empty(),
        "the cleanup gave up on the lock: {}",
        attempts.refusals()
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
    // holds_by_every 로 바꾸지 않는다. 판정 대상은 매 시도가 새로 만드는 경합 결과라, 기한 뒤에 새 시도
    // 없이 관측할 상태가 없다.
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
