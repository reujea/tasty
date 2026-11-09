//! Construct and destroy native HTTP runtimes on bounded workers; native owners only poll memory.
mod retirement;
use super::{io::Current, listener::EndpointBudget};
#[cfg(target_os = "linux")]
use crate::host_api::webview::http_context::preparation::ProfileWork;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
use tasty_remote::html_http_client::VerifyJob;
use tokio::{runtime::Handle, sync::Semaphore};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeError {
    Capacity,
    Retired,
    Worker,
}
struct State {
    live: AtomicBool,
    current: Current,
}
impl State {
    fn current(&self) -> bool {
        self.live.load(Ordering::Acquire) && (self.current)()
    }
}
struct Parts {
    handle: Handle,
    jobs: Arc<Semaphore>,
    endpoints: EndpointBudget,
}
struct Owner {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    state: Arc<State>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.state.live.store(false, Ordering::Release);
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            retirement::retain(thread);
        }
    }
}
pub(crate) struct Setup {
    result: mpsc::Receiver<Result<Parts, RuntimeError>>,
    owner: Option<Owner>,
}
pub(crate) struct NativeRuntime {
    parts: Parts,
    _owner: Owner,
}
impl Setup {
    pub(crate) fn start(
        current: Current,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, RuntimeError> {
        if !current() {
            return Err(RuntimeError::Retired);
        }
        let permit = retirement::reserve()?;
        let state = Arc::new(State {
            live: AtomicBool::new(true),
            current,
        });
        let worker = state.clone();
        let (tx, result) = mpsc::sync_channel(1);
        let (stop, shutdown) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("html-native-setup".into())
            .spawn(move || {
                let _permit = permit;
                setup_worker(worker, tx, shutdown, wake);
            })
            .map_err(|_| RuntimeError::Worker)?;
        Ok(Self {
            result,
            owner: Some(Owner {
                stop: Some(stop),
                thread: Some(thread),
                state,
            }),
        })
    }
    pub(crate) fn poll(&mut self) -> Option<Result<NativeRuntime, RuntimeError>> {
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(RuntimeError::Worker),
        };
        let owner = self.owner.take()?;
        Some(match result {
            Ok(parts) if owner.state.current() => Ok(NativeRuntime {
                parts,
                _owner: owner,
            }),
            Ok(_) => Err(RuntimeError::Retired),
            Err(error) => Err(error),
        })
    }
}
fn setup_worker(
    worker: Arc<State>,
    tx: mpsc::SyncSender<Result<Parts, RuntimeError>>,
    shutdown: mpsc::Receiver<()>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            setup_failed(error, tx, &wake);
            return;
        }
    };
    let parts = if worker.current() {
        Ok(Parts {
            handle: runtime.handle().clone(),
            jobs: Arc::new(Semaphore::new(8)),
            endpoints: EndpointBudget::new(),
        })
    } else {
        Err(RuntimeError::Retired)
    };
    let success = parts.is_ok();
    let delivered = tx.send(parts).is_ok();
    wake();
    wait_for_shutdown(success, delivered, shutdown);
    worker.live.store(false, Ordering::Release);
    // Runtime Drop may await actual blocking profile/verification work. This owned
    // setup worker is the only place that waits; permits survive until termination.
    drop(runtime);
}
fn setup_failed(
    error: std::io::Error,
    tx: mpsc::SyncSender<Result<Parts, RuntimeError>>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) {
    tracing::warn!(%error, "native HTTP runtime setup failed");
    if tx.send(Err(RuntimeError::Worker)).is_err() {
        tracing::debug!("native runtime setup receiver retired");
    }
    wake();
}
fn wait_for_shutdown(success: bool, delivered: bool, shutdown: mpsc::Receiver<()>) {
    if success && delivered && shutdown.recv().is_ok() {
        tracing::debug!("native runtime explicitly stopped");
    }
}
impl NativeRuntime {
    pub(crate) fn handle(&self) -> Handle {
        self.parts.handle.clone()
    }
    pub(crate) fn endpoint_budget(&self) -> EndpointBudget {
        self.parts.endpoints.clone()
    }
    #[cfg_attr(
        not(any(target_os = "linux", test)),
        expect(
            dead_code,
            reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
        )
    )]
    pub(crate) fn current(&self) -> Current {
        let state = self._owner.state.clone();
        Arc::new(move || state.current())
    }
    fn submit<T: Send + 'static>(&self, work: T, execute: fn(T)) -> Result<(), T> {
        if !self._owner.state.current() {
            return Err(work);
        }
        let Ok(permit) = self.parts.jobs.clone().try_acquire_owned() else {
            return Err(work);
        };
        let state = self._owner.state.clone();
        // The finite job owns its permit until actual completion, even after native retirement.
        drop(self.parts.handle.spawn_blocking(move || {
            let _permit = permit;
            if state.current() {
                execute(work);
            }
        }));
        Ok(())
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn spawn_profile(&self, work: ProfileWork) -> Result<(), ProfileWork> {
        self.submit(work, ProfileWork::run)
    }
    pub(crate) fn spawn_verify(&self, work: VerifyJob) -> Result<(), VerifyJob> {
        self.submit(work, VerifyJob::run)
    }
}
#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod tests_verify;
