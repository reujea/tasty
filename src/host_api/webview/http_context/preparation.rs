//! Run profile I/O on the caller's bounded worker; native receipts and Drop only update memory.
mod admission;
mod namespace;
use super::profile::{self, PreparedProfile, ProfileError};
pub(crate) use namespace::NamespaceRoot;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug)]
pub(crate) enum PrepareError {
    Storage(ProfileError),
    Retired,
    Worker,
    Timeout,
    AdmissionTimeout,
}
impl std::fmt::Display for PrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "private native profile preparation failed: {error}"),
            Self::Retired => f.write_str("native profile preparation retired"),
            Self::Worker => f.write_str("native profile worker unavailable"),
            Self::AdmissionTimeout => f.write_str("native profile namespace admission timed out"),
            Self::Timeout => f.write_str("native profile claim timed out"),
        }
    }
}
pub(crate) const MAX_CLAIM_WAIT: Duration = Duration::from_secs(30);
enum State {
    Pending,
    Ready(Result<PreparedProfile, PrepareError>),
    Taken,
    Cancelled,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cancelled: AtomicBool,
}
impl Shared {
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && (self.current)()
    }
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.ready.notify_all();
        (self.wake)();
    }
}
pub(crate) struct Preparation {
    shared: Arc<Shared>,
}
pub(crate) struct ProfileWork {
    shared: Option<Arc<Shared>>,
    home: NamespaceRoot,
    roots: usize,
    claim_timeout: Duration,
}
impl Preparation {
    pub(crate) fn new(
        home: impl Into<NamespaceRoot>,
        roots: usize,
        claim_timeout: Duration,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> (Self, ProfileWork) {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::Pending),
            ready: Condvar::new(),
            current,
            wake,
            cancelled: AtomicBool::new(false),
        });
        (
            Self {
                shared: shared.clone(),
            },
            ProfileWork {
                shared: Some(shared),
                home: home.into(),
                roots,
                claim_timeout,
            },
        )
    }
    pub(crate) fn poll(&self) -> Option<Result<PreparedProfile, PrepareError>> {
        let Ok(mut state) = self.shared.state.lock() else {
            tracing::error!("private profile preparation poisoned; ownership retained");
            return Some(Err(PrepareError::Worker));
        };
        if !self.shared.active() && !matches!(*state, State::Taken) {
            // Keep the profile in the slot: only the worker may dispose it.
            drop(state);
            self.shared.cancel();
            return Some(Err(PrepareError::Retired));
        }
        if matches!(*state, State::Pending) {
            return None;
        }
        match std::mem::replace(&mut *state, State::Taken) {
            State::Ready(result) => {
                self.shared.ready.notify_all();
                Some(result)
            }
            State::Cancelled => Some(Err(PrepareError::Retired)),
            State::Taken => None,
            State::Pending => unreachable!("pending handled before state move"),
        }
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        let Ok(state) = self.shared.state.lock() else {
            tracing::error!("private profile cancellation poisoned; ownership retained");
            return;
        };
        if !matches!(*state, State::Taken) {
            self.shared.cancelled.store(true, Ordering::Release);
            self.shared.ready.notify_all();
        }
    }
}
impl ProfileWork {
    /// Consumes one slot in an existing bounded worker until native claim/cancellation settles.
    pub(crate) fn run(mut self) {
        let shared = self.shared.take().expect("unconsumed profile work");
        let result = self.prepare(&shared);
        let result = if shared.active() {
            result
        } else {
            discard(result);
            Err(PrepareError::Retired)
        };
        let needs_claim = result.is_ok();
        let Ok(mut state) = shared.state.lock() else {
            tracing::error!("private profile result poisoned");
            discard(result);
            return;
        };
        *state = State::Ready(result);
        drop(state);
        (shared.wake)();
        if !needs_claim {
            return;
        }
        self.await_claim(&shared);
    }
    fn await_claim(&self, shared: &Arc<Shared>) {
        let Ok(mut state) = shared.state.lock() else {
            tracing::error!("private profile claim state poisoned; ownership retained");
            return;
        };
        let deadline = Instant::now() + self.claim_timeout.min(MAX_CLAIM_WAIT);
        loop {
            if matches!(*state, State::Taken) {
                return;
            }
            if Arc::strong_count(shared) == 1 || !shared.active() || Instant::now() >= deadline {
                let expired = Instant::now() >= deadline;
                let next = if expired {
                    State::Ready(Err(PrepareError::Timeout))
                } else {
                    State::Cancelled
                };
                let State::Ready(result) = std::mem::replace(&mut *state, next) else {
                    return;
                };
                drop(state);
                discard(result);
                (shared.wake)();
                return;
            }
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50));
            match shared.ready.wait_timeout(state, wait) {
                Ok((next, _)) => state = next,
                Err(_) => {
                    tracing::error!("private profile claim wait poisoned; ownership retained");
                    return;
                }
            }
        }
    }
    fn prepare(&self, shared: &Shared) -> Result<PreparedProfile, PrepareError> {
        if self.roots == 0 || self.claim_timeout.is_zero() || self.claim_timeout > MAX_CLAIM_WAIT {
            return Err(PrepareError::Storage(ProfileError::Invalid));
        }
        if !shared.active() {
            return Err(PrepareError::Retired);
        }
        // Admission uses one finite budget across both lock acquisitions. Native claim gets
        // its separate, unchanged deadline only after readiness has been published.
        let deadline = Instant::now() + self.claim_timeout;
        let parent = namespace::prepare(&self.home).map_err(PrepareError::Storage)?;
        let report = admission::run(shared, deadline, "sweep", || {
            profile::reclaim_dead(&parent, self.roots)
        })?;
        self.sweep_legacy();
        tracing::debug!(
            inspected = report.inspected,
            removed = report.removed,
            live = report.kept_live,
            uncertain = report.kept_uncertain,
            failed = report.failed,
            limit = report.limit_reached,
            "private native profile sweep completed"
        );
        if !shared.active() {
            return Err(PrepareError::Retired);
        }
        admission::run(shared, deadline, "create", || {
            PreparedProfile::create(&parent, self.roots)
        })
    }
}
impl ProfileWork {
    /// Removes dead roots left in the app-home namespace before the runtime directory was used.
    /// Only the existing dead-owner rules delete; contention or any doubt keeps the data.
    fn sweep_legacy(&self) {
        let Some(legacy) = self.home.legacy() else {
            return;
        };
        match profile::reclaim_dead(&legacy, self.roots) {
            Ok(report) => tracing::debug!(
                removed = report.removed,
                live = report.kept_live,
                uncertain = report.kept_uncertain,
                "legacy private profile namespace swept"
            ),
            Err(error) => {
                tracing::debug!(%error, "legacy private profile namespace kept");
            }
        }
    }
}
impl Drop for ProfileWork {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.take() {
            // reason: uncertain work state is not recovered; the error branch reports poison,
            // and the receipt is still woken after this lock attempt ends.
            if let Ok(mut state) = shared.state.lock() {
                *state = State::Ready(Err(PrepareError::Worker));
            } else {
                tracing::error!("unrun profile work poisoned");
            }
            (shared.wake)();
        }
    }
}
fn discard(result: Result<PreparedProfile, PrepareError>) {
    if let Ok(profile) = result {
        let root = profile.root().to_owned();
        if let Err(error) = profile.discard_unpublished() {
            tracing::warn!(%error,?root,"cancelled unpublished private profile cleanup failed; owned root retained");
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod admission_tests;

#[cfg(test)]
mod runtime_tests;
