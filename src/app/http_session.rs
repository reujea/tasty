//! App-side lifetime for one actual browser document; no speculative native request is created.
#[cfg(target_os = "linux")]
use crate::webview::http_context::{
    preparation::{Preparation, PrepareError},
    profile::PreparedProfile,
};
use crate::webview::http_endpoint::{
    body::BodySource,
    listener::{
        DocumentIdentity, Endpoint, EndpointConfig, EndpointError, Transport,
        trust::{NativeTrust, TrustConfig},
    },
    runtime::{NativeRuntime, Setup},
};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};
use tasty_remote::{
    connection::ConnectionSender,
    html_http_body::BodyBudget,
    html_http_bridge::{BridgeConfig, BridgeScope, HttpBridge},
    html_http_client::{ClientLimits, PendingBudget},
    html_mirror::{HttpSource, HttpView},
};
mod delivery;
mod native_store;
pub(crate) use native_store::NativeStore;
mod receive;
mod reload;
mod retirement;
#[cfg(any(windows, target_os = "macos"))]
mod script_policy;
pub(crate) use receive::Observation;
pub(crate) use reload::ReloadSetup;
type EndpointReceipt = mpsc::Receiver<Result<(Endpoint, Option<NativeTrust>), EndpointError>>;
type Notices =
    std::collections::HashMap<tasty_ipc::html_transfer::TransferId, (String, Option<u64>, u64)>;
type Current = Arc<dyn Fn() -> bool + Send + Sync>;
/// WebKitGTK authenticates RFC1929 SOCKS. Chromium-based WebView2 only supports HTTP proxy auth,
/// and macOS WebKit answers an HTTP proxy challenge through its navigation delegate.
#[cfg(target_os = "linux")]
const NATIVE_TRANSPORT: Transport = Transport::Socks;
#[cfg(any(windows, target_os = "macos"))]
const NATIVE_TRANSPORT: Transport = Transport::HttpProxy;
struct State {
    setup: Option<Setup>,
    runtime: Option<NativeRuntime>,
    #[cfg(target_os = "linux")]
    preparation: Option<Preparation>,
    endpoint_pending: Option<EndpointReceipt>,
    endpoint: Option<Arc<Endpoint>>,
    trust: Option<NativeTrust>,
}
pub(crate) struct HttpSession {
    pub source: HttpSource,
    pub view: HttpView,
    https: bool,
    pub bridge: Arc<HttpBridge>,
    pub bodies: Arc<BodySource>,
    current: Current,
    reload: reload::ReloadState,
    live: Arc<AtomicBool>,
    closing: AtomicBool,
    failed: AtomicBool,
    joined: Arc<AtomicU8>,
    retaining: AtomicBool,
    pub(crate) native_store: Arc<NativeStore>,
    native_generation: u64,
    final_body: Mutex<Option<Arc<tasty_remote::html_http_body::VerifiedBody>>>,
    state: Mutex<State>,
    wake: Arc<dyn Fn() + Send + Sync>,
    deliveries: Arc<delivery::Deliveries>,
    notices: Mutex<Notices>,
    #[cfg(any(windows, target_os = "macos"))]
    script: script_policy::ScriptPolicy,
    #[cfg(any(windows, target_os = "macos"))]
    me: std::sync::Weak<Self>,
}
impl HttpSession {
    pub(crate) fn new(
        source: HttpSource,
        sender: Arc<ConnectionSender>,
        allow_remote_content: bool,
        https: bool,
        native_store: Arc<NativeStore>,
        wake: Arc<dyn Fn() + Send + Sync>,
        reload: ReloadSetup,
    ) -> Result<Arc<Self>, String> {
        let view = source.new_view().ok_or("retired native HTTP source")?;
        let live = Arc::new(AtomicBool::new(true));
        let native_generation = native_store.generation();
        let current: Current = {
            let view = view.clone();
            let live = live.clone();
            let native_store = native_store.clone();
            Arc::new(move || {
                live.load(Ordering::Acquire)
                    && native_store.is_current(native_generation)
                    && view.is_current()
            })
        };
        let metadata = source.metadata_id();
        let cap = tasty_ipc::admission::QUEUED_BYTES_LIMIT;
        // Shared across all App documents, including separate connections. Resource quotas are
        // independent of each document's smaller encoded consent windows.
        static PENDING: OnceLock<PendingBudget> = OnceLock::new();
        static BODIES: OnceLock<BodyBudget> = OnceLock::new();
        let (bridge, reload) = reload.build(
            BridgeConfig {
                scope: BridgeScope {
                    epoch: metadata.epoch,
                    surface_id: metadata.surface_id,
                    binding: metadata.binding,
                    document: metadata.document,
                    source_generation: source.source_generation(),
                    entry_url: source.entry_url().into(),
                    allow_remote_content,
                },
                limits: ClientLimits {
                    requests: 256,
                    upload_bytes: cap,
                    encoded_bytes: cap,
                    decoded_bytes: cap,
                    verify_jobs: 8,
                },
                pending: PENDING.get_or_init(|| PendingBudget::new(cap)).clone(),
                bodies: BODIES.get_or_init(|| BodyBudget::new(cap)).clone(),
            },
            sender,
            current.clone(),
            wake.clone(),
        )?;
        let bodies = BodySource::new(bridge.clone(), view.clone(), current.clone());
        let setup = Setup::start(current.clone(), wake.clone())
            .map_err(|error| format!("native HTTP runtime: {error:?}"))?;
        Ok(Arc::new_cyclic(|_me| Self {
            #[cfg(any(windows, target_os = "macos"))]
            script: script_policy::ScriptPolicy::default(),
            #[cfg(any(windows, target_os = "macos"))]
            me: _me.clone(),
            source,
            view,
            https,
            bridge,
            bodies,
            current,
            reload,
            live,
            closing: AtomicBool::new(false),
            wake,
            notices: Mutex::default(),
            deliveries: Arc::default(),
            failed: AtomicBool::new(false),
            joined: Arc::new(AtomicU8::new(0)),
            retaining: AtomicBool::new(false),
            native_store,
            native_generation,
            final_body: Mutex::default(),
            state: Mutex::new(State {
                setup: Some(setup),
                runtime: None,
                #[cfg(target_os = "linux")]
                preparation: None,
                endpoint_pending: None,
                endpoint: None,
                trust: None,
            }),
        }))
    }
    pub(crate) fn is_current(&self) -> bool {
        (self.current)()
    }
    pub(crate) fn poll(
        &self,
        wake: Arc<dyn Fn() + Send + Sync>,
        allow_remote_content: bool,
    ) -> Result<(), String> {
        if !self.is_current() {
            return Err("retired native HTTP session".into());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "native HTTP session poisoned")?;
        if state.setup.is_some() && !self.endpoint_admitted()? {
            return Ok(());
        }
        if let Some(setup) = &mut state.setup {
            let Some(runtime) = setup.poll() else {
                return Ok(());
            };
            state.runtime = Some(runtime.map_err(|error| format!("native HTTP setup: {error:?}"))?);
            state.setup = None;
            let runtime = state.runtime.as_ref().expect("ready runtime");
            let id = self.source.metadata_id();
            let config = EndpointConfig {
                transport: NATIVE_TRANSPORT,
                port: self.endpoint_port(),
                loopback_alias: cfg!(target_os = "macos"),
                identity: DocumentIdentity {
                    epoch: id.epoch,
                    surface: id.surface_id,
                    binding: id.binding,
                    document: id.document,
                    source_generation: self.source.source_generation(),
                },
                entry_url: self.source.entry_url().into(),
                allow_remote_content,
                https: self.https,
                current: self.current.clone(),
                limits: tasty_remote::html_http_native::NativeLimits {
                    upload_bytes: tasty_ipc::admission::QUEUED_BYTES_LIMIT,
                    io_timeout: Duration::from_secs(30),
                },
                native_budget: tasty_remote::html_http_native::NativeBudget::new(
                    tasty_ipc::admission::QUEUED_BYTES_LIMIT,
                ),
                handler: self.deliveries.handler(
                    self.bodies.handler(),
                    self.current.clone(),
                    self.local_policy(),
                ),
            };
            let (tx, rx) = mpsc::sync_channel(1);
            let handle = runtime.handle();
            let budget = runtime.endpoint_budget();
            let endpoint_wake = wake.clone();
            drop(runtime.handle().spawn(async move {
                // Only WebKitGTK observes its own TLS failure through the endpoint probe. WebView2
                // compares the presented leaf in its certificate-error event instead.
                let result = if config.https && cfg!(target_os = "linux") {
                    Endpoint::start_with_native_trust(
                        handle,
                        config,
                        budget,
                        TrustConfig {
                            timeout: Duration::from_secs(30),
                            wake: endpoint_wake.clone(),
                        },
                    )
                    .await
                    .map(|(endpoint, trust)| (endpoint, Some(trust)))
                } else {
                    Endpoint::start(handle, config, budget)
                        .await
                        .map(|endpoint| (endpoint, None))
                };
                if let Err(result) = tx.send(result)
                    && let Ok((endpoint, trust)) = result.0
                {
                    drop(trust);
                    if let Err(error) = endpoint.join().await {
                        tracing::warn!(?error, "unclaimed native endpoint retirement failed");
                    }
                }
                endpoint_wake();
            }));
            state.endpoint_pending = Some(rx);
        }
        #[cfg(target_os = "linux")]
        if state.runtime.is_some() && state.preparation.is_none() && !self.native_store.is_live() {
            let runtime = state.runtime.as_ref().expect("ready runtime");
            let (receipt, work) = Preparation::new(
                crate::host_api::webview::http_context::preparation::NamespaceRoot::for_process(
                    tasty_utils::path::tasty_home().ok_or("native HTTP data home unavailable")?,
                ),
                8,
                Duration::from_secs(30),
                runtime.current(),
                wake.clone(),
            );
            runtime
                .spawn_profile(work)
                .map_err(|_| "native HTTP profile capacity")?;
            state.preparation = Some(receipt);
        }
        if let Some(rx) = &state.endpoint_pending {
            match rx.try_recv() {
                Ok(result) => {
                    let (endpoint, trust) =
                        result.map_err(|error| format!("native HTTP endpoint: {error:?}"))?;
                    state.endpoint = Some(Arc::new(endpoint));
                    state.trust = trust;
                    state.endpoint_pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("native HTTP endpoint setup stopped".into());
                }
            }
        }
        Ok(())
    }
    #[cfg(any(windows, target_os = "macos"))]
    fn local_policy(&self) -> Option<delivery::LocalPolicy> {
        let session = self.me.clone();
        Some(Arc::new(move |request| {
            session
                .upgrade()
                .ok_or(EndpointError::Retired)?
                .local_headers(request)
        }))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    fn local_policy(&self) -> Option<delivery::LocalPolicy> {
        None
    }
    #[cfg_attr(
        not(target_os = "linux"),
        expect(
            dead_code,
            reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
        )
    )]
    pub(crate) fn with_native_trust<T>(&self, f: impl FnOnce(&NativeTrust) -> T) -> Option<T> {
        // reason: no trust capability may escape poisoned setup; the owning poll reports and retires it.
        let state = self.state.lock().ok()?;
        state.trust.as_ref().map(f)
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn prepare_probe(&self) -> Result<Preparation, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native probe runtime poisoned")?;
        let runtime = state
            .runtime
            .as_ref()
            .ok_or("native probe runtime unavailable")?;
        let (receipt, work) = Preparation::new(
            crate::host_api::webview::http_context::preparation::NamespaceRoot::for_process(
                tasty_utils::path::tasty_home().ok_or("native probe home unavailable")?,
            ),
            8,
            Duration::from_secs(30),
            runtime.current(),
            self.wake.clone(),
        );
        runtime
            .spawn_profile(work)
            .map_err(|_| "native probe profile capacity")?;
        Ok(receipt)
    }
    #[cfg_attr(
        not(target_os = "linux"),
        expect(
            dead_code,
            reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
        )
    )]
    pub(crate) fn prepare_authority(
        &self,
        url: &str,
    ) -> Result<
        mpsc::Receiver<
            Result<
                crate::webview::http_endpoint::listener::preparation::ReadyAuthority,
                EndpointError,
            >,
        >,
        EndpointError,
    > {
        let state = self.state.lock().map_err(|_| EndpointError::Worker)?;
        let runtime = state.runtime.as_ref().ok_or(EndpointError::Retired)?;
        let endpoint = state.endpoint.as_ref().ok_or(EndpointError::Retired)?;
        let future = endpoint.prepare_https(url)?;
        let (tx, rx) = mpsc::sync_channel(1);
        let wake = self.wake.clone();
        drop(runtime.handle().spawn(async move {
            if tx.send(future.await).is_err() {
                tracing::debug!("native authority receipt retired");
            }
            wake();
        }));
        Ok(rx)
    }
    pub(crate) fn entry_body(&self) -> Option<Arc<tasty_remote::html_http_body::VerifiedBody>> {
        // reason: poisoned final preview storage is unavailable; it cannot authorize native delivery.
        self.deliveries.main().or_else(|| {
            self.bridge
                .document()
                .and_then(|document| {
                    document
                        .with_current(|model| model.verified(document.entry()))
                        .flatten()
                })
                .or_else(|| self.final_body.lock().ok()?.clone())
        })
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn profile(&self) -> Option<Result<PreparedProfile, PrepareError>> {
        // reason: poisoned setup cannot yield a native profile; the owning poll reports and retires it.
        self.state.lock().ok()?.preparation.as_ref()?.poll()
    }
    pub(crate) fn endpoint(&self) -> Option<Arc<Endpoint>> {
        // reason: poisoned setup cannot yield an endpoint; the owning poll reports and retires it.
        self.state.lock().ok()?.endpoint.clone()
    }
    pub(crate) fn drive(
        &self,
        next_id: impl FnMut() -> Option<tasty_ipc::html_transfer::TransferId>,
    ) -> bool {
        let Ok(state) = self.state.lock() else {
            self.close();
            return false;
        };
        let Some(runtime) = &state.runtime else {
            return false;
        };
        self.bridge.drive(next_id, |job| runtime.spawn_verify(job))
    }
    pub(crate) fn request_retained(&self) {
        self.retaining.store(true, Ordering::Release);
    }
    pub(crate) fn cancel_retained(&self) {
        self.native_store.invalidate(self.native_generation);
        self.retaining.store(false, Ordering::Release);
        self.close_reload();
    }
    pub(crate) fn native_owner_current(&self) -> bool {
        self.native_store.is_current(self.native_generation)
    }
    pub(crate) fn retaining(&self) -> bool {
        self.native_owner_current() && self.retaining.load(Ordering::Acquire)
    }
}
impl Drop for HttpSession {
    fn drop(&mut self) {
        self.close();
    }
}
