//! Opaque native capabilities gate original TLS; private probes never enter HTTP dispatch.
pub(super) mod hello;
pub(crate) mod pool;
mod probe;
mod slot;
use super::super::certificates::AuthorityTls;
use super::*;
use slot::Slot;
pub(crate) use slot::{TrustObservation, TrustRequest};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
pub(crate) struct TrustConfig {
    pub timeout: Duration,
    pub wake: Arc<dyn Fn() + Send + Sync>,
}
struct Inner {
    queue: VecDeque<TrustRequest>,
    slots: Vec<std::sync::Weak<Slot>>,
    tasks: JoinSet<()>,
}
pub(super) struct Gate {
    runtime: Handle,
    current: Current,
    budget: EndpointBudget,
    config: TrustConfig,
    closed: AtomicBool,
    inner: Mutex<Inner>,
}
pub(crate) struct NativeTrust(Arc<Gate>);
impl NativeTrust {
    pub(super) fn new(gate: Arc<Gate>) -> Self {
        Self(gate)
    }
    /// Poll only on the native loop. Credentials/capability must never reach page content.
    pub(crate) fn poll(&self) -> Option<TrustRequest> {
        loop {
            if !self.0.current() {
                self.0.close();
                return None;
            }
            // reason: lock() logs poison and closes the gate; None cannot issue a trust capability.
            let request = self.0.lock().ok()?.queue.pop_front()?;
            if request.is_current() {
                return Some(request);
            }
            drop(request);
        }
    }
    /// native loop가 아직 꺼내지 않은 trust 요청이 있는지 본다. 요청은 꺼내지 않는다.
    #[cfg(target_os = "linux")]
    pub(crate) fn has_queued(&self) -> bool {
        self.0.current() && self.0.lock().is_ok_and(|inner| !inner.queue.is_empty())
    }
}
impl Drop for NativeTrust {
    fn drop(&mut self) {
        self.0.close();
    }
}
impl Gate {
    pub(super) fn new(
        runtime: Handle,
        current: Current,
        budget: EndpointBudget,
        config: TrustConfig,
    ) -> Result<Arc<Self>, EndpointError> {
        if config.timeout.is_zero() || config.timeout > Duration::from_secs(30) {
            return Err(EndpointError::Invalid);
        }
        Ok(Arc::new(Self {
            runtime,
            current,
            budget,
            config,
            closed: AtomicBool::new(false),
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                slots: Vec::new(),
                tasks: JoinSet::new(),
            }),
        }))
    }
    fn current(&self) -> bool {
        !self.closed.load(Ordering::Acquire) && (self.current)()
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, EndpointError> {
        self.inner.lock().map_err(|_| {
            self.closed.store(true, Ordering::Release);
            tracing::error!("native TLS trust queue poisoned; trust refused");
            EndpointError::Worker
        })
    }
    pub(super) async fn wait(
        self: &Arc<Self>,
        state: &Arc<State>,
        authority: Authority,
        certificate: Arc<AuthorityTls>,
    ) -> Result<(), EndpointError> {
        if !self.current() {
            return Err(EndpointError::Retired);
        }
        let permit = self.budget.trust()?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| EndpointError::Io)?;
        let port = listener.local_addr().map_err(|_| EndpointError::Io)?.port();
        let slot = Slot::new(
            self,
            state,
            authority,
            certificate,
            port,
            self.config.timeout,
            permit,
        )?;
        // This guard also runs when the original accepted socket task is aborted.
        let _attempt = slot::Attempt(slot.clone());
        {
            let mut inner = self.lock()?;
            if !self.current() {
                return Err(EndpointError::Retired);
            }
            while let Some(result) = inner.tasks.try_join_next() {
                if let Err(error) = result {
                    tracing::warn!(%error,"native TLS probe task failed");
                }
            }
            inner.slots.retain(|slot| slot.strong_count() > 0);
            if inner.slots.len() >= 8 || inner.queue.len() >= 8 {
                return Err(EndpointError::Capacity);
            }
            inner.slots.push(Arc::downgrade(&slot));
            inner.tasks.spawn_on(
                probe::listen(slot.clone(), listener, self.budget.clone()),
                &self.runtime,
            );
            inner.queue.push_back(TrustRequest::new(slot.clone()));
        }
        (self.config.wake)();
        let granted = slot.wait().await;
        slot.cancel();
        // Tell native polling to retire its private probe on deadline or source cancellation.
        (self.config.wake)();
        slot.stopped().await;
        if granted
            && tokio::time::Instant::now() < slot.deadline
            && !slot.failed.load(Ordering::Acquire)
            && self.current()
            && (state.current)()
        {
            Ok(())
        } else {
            Err(EndpointError::Tls)
        }
    }
    pub(super) fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // reason: lock() already logs poison and refuses trust; do not recover uncertain queue ownership.
        let pending = self.lock().ok().map(|mut inner| {
            for slot in inner.slots.iter().filter_map(std::sync::Weak::upgrade) {
                slot.cancel();
            }
            std::mem::take(&mut inner.queue)
        });
        drop(pending);
        (self.config.wake)();
    }
    pub(super) async fn join(&self) -> Result<(), EndpointError> {
        let mut tasks = {
            let mut inner = self.lock()?;
            std::mem::take(&mut inner.tasks)
        };
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                tracing::warn!(%error,"native TLS probe retirement failed");
            }
        }
        Ok(())
    }
}
