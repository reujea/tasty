//! Response-aware loopback proxy endpoint. It never opens a connection to the logical target.
mod budget;
mod policy;
pub(crate) mod preparation;
mod request;
#[cfg_attr(
    not(target_os = "linux"),
    expect(
        dead_code,
        reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
    )
)]
pub(crate) mod trust;
use super::{
    certificates::{Authority, Certificates},
    io::{Current, CurrentConnection, Prefixed},
};
use crate::remote::html_http::endpoint::{
    self, EndpointToken,
    proxy::{self, ProxyRealm, ProxyRoute},
};
pub(crate) use budget::EndpointBudget;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tasty_remote::{
    html_http_body::VerifiedBody,
    html_http_native::{NativeBudget, NativeLimits, NativeRequest},
};
use tokio::{
    net::{TcpListener, TcpStream},
    runtime::Handle,
    sync::watch,
    task::{JoinHandle, JoinSet},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointError {
    Invalid,
    Retired,
    Capacity,
    Auth,
    Destination,
    Tls,
    Io,
    /// Binding a loopback listener (the endpoint or a native trust probe) failed; keeps the OS
    /// cause, e.g. `AddrInUse` when another socket took the port a retained Reload inherits.
    Bind {
        kind: std::io::ErrorKind,
        os_error: Option<i32>,
    },
    /// A bound loopback listener could not report its local address; keeps the OS cause.
    LocalAddr {
        kind: std::io::ErrorKind,
        os_error: Option<i32>,
    },
    Body,
    Identity,
    Worker,
}
/// Which loopback listener an open failure belongs to, named in its warning.
#[derive(Clone, Copy, Debug)]
enum ListenerRole {
    Endpoint,
    TrustProbe,
}
fn bind_error(role: ListenerRole, port: u16, error: &std::io::Error) -> EndpointError {
    tracing::warn!(
        listener = ?role,
        port,
        kind = ?error.kind(),
        os_error = ?error.raw_os_error(),
        %error,
        "HTTP endpoint bind failed"
    );
    EndpointError::Bind {
        kind: error.kind(),
        os_error: error.raw_os_error(),
    }
}
fn local_port(
    role: ListenerRole,
    address: std::io::Result<std::net::SocketAddr>,
) -> Result<u16, EndpointError> {
    address.map(|address| address.port()).map_err(|error| {
        tracing::warn!(
            listener = ?role,
            kind = ?error.kind(),
            os_error = ?error.raw_os_error(),
            %error,
            "HTTP endpoint listener address unavailable"
        );
        EndpointError::LocalAddr {
            kind: error.kind(),
            os_error: error.raw_os_error(),
        }
    })
}
pub(crate) trait ResponseOwner: Send {
    fn body(&self) -> &Arc<VerifiedBody>;
    fn request(&self) -> &Arc<NativeRequest>;
    /// Preserve the actual bridge response lease for trusted native main-response selection.
    fn native_delivery(&self) -> Option<super::body::NativeDelivery> {
        None
    }
    fn write_completed(&self, _success: bool) {}
    /// Client-local headers written after the verified head, such as a script-blocking policy.
    fn local_headers(&self) -> &[tasty_ipc::html_http::Header] {
        &[]
    }
}
pub(crate) type ResponseFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn ResponseOwner>, EndpointError>> + Send>>;
pub(crate) type Handler = Arc<dyn Fn(Arc<NativeRequest>) -> ResponseFuture + Send + Sync>;
#[derive(Clone, Copy)]
pub(crate) struct DocumentIdentity {
    pub epoch: u64,
    pub surface: u32,
    pub binding: u64,
    pub document: u64,
    pub source_generation: u64,
}
impl DocumentIdentity {
    fn matches(&self, body: &VerifiedBody) -> bool {
        let id = body.head().id;
        let request = id.request;
        id.serial > 0
            && id.source_generation == self.source_generation
            && (
                request.epoch,
                request.surface_id,
                request.binding,
                request.document,
            ) == (self.epoch, self.surface, self.binding, self.document)
    }
}
/// How the native engine authenticates to the document endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    /// RFC1929 SOCKS5 credentials carried in the proxy URI (WebKitGTK).
    #[cfg_attr(
        not(any(target_os = "linux", test)),
        expect(
            dead_code,
            reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
        )
    )]
    Socks,
    /// Basic HTTP proxy authentication answered by the native auth handler (WebView2).
    HttpProxy,
}
/// Values a native HTTP proxy authentication handler needs; never logged or persisted.
#[cfg_attr(
    not(any(windows, target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the WebView2 and WebKit adapters authenticate through the HTTP proxy front"
    )
)]
pub(crate) struct ProxyCredentials {
    pub uri: String,
    #[cfg_attr(
        all(target_os = "macos", not(test)),
        expect(dead_code, reason = "WebKit reports no proxy realm to compare")
    )]
    pub challenge: String,
    pub user: &'static str,
    pub password: String,
}
pub(crate) struct EndpointConfig {
    pub transport: Transport,
    /// Loopback port to bind; 0 picks a free one. A WebView2 view fixes its proxy address at
    /// creation, so its retained Reload binds the port of the document it replaces.
    pub port: u16,
    /// Read alias hosts back as the remote loopback hosts they stand for (WebKit on macOS).
    pub loopback_alias: bool,
    pub identity: DocumentIdentity,
    pub entry_url: String,
    pub allow_remote_content: bool,
    pub https: bool,
    pub current: Current,
    pub limits: NativeLimits,
    pub native_budget: NativeBudget,
    pub handler: Handler,
}
struct State {
    config: EndpointConfig,
    policy: policy::Policy,
    current: Current,
    stop: Arc<AtomicBool>,
    token: EndpointToken,
    realm: ProxyRealm,
    certificates: Arc<Certificates>,
    budget: EndpointBudget,
    trust: Option<Arc<trust::Gate>>,
}
pub(crate) struct Endpoint {
    state: Arc<State>,
    proxy_uri: String,
    port: u16,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<(), EndpointError>>>,
}
impl Endpoint {
    /// Schedule on the injected runtime, never poll socket setup on the native/engine loop.
    pub(crate) async fn start(
        runtime: Handle,
        config: EndpointConfig,
        budget: EndpointBudget,
    ) -> Result<Self, EndpointError> {
        Self::start_inner(runtime, config, budget, None).await
    }
    pub(crate) async fn start_with_native_trust(
        runtime: Handle,
        config: EndpointConfig,
        budget: EndpointBudget,
        trust: trust::TrustConfig,
    ) -> Result<(Self, trust::NativeTrust), EndpointError> {
        let endpoint = Self::start_inner(runtime, config, budget, Some(trust)).await?;
        let native = trust::NativeTrust::new(
            endpoint
                .state
                .trust
                .as_ref()
                .ok_or(EndpointError::Worker)?
                .clone(),
        );
        Ok((endpoint, native))
    }
    async fn start_inner(
        runtime: Handle,
        config: EndpointConfig,
        budget: EndpointBudget,
        trust: Option<trust::TrustConfig>,
    ) -> Result<Self, EndpointError> {
        // Taken by the thread that first polls this future, before anything can return early.
        #[cfg(test)]
        let fault = tests::fault::take();
        let identity = config.identity;
        if [
            identity.epoch,
            u64::from(identity.surface),
            identity.binding,
            identity.document,
            identity.source_generation,
        ]
        .contains(&0)
            || config.limits.upload_bytes == 0
            || config.limits.upload_bytes > tasty_ipc::admission::QUEUED_BYTES_LIMIT
            || config.limits.io_timeout.is_zero()
        {
            return Err(EndpointError::Invalid);
        }
        if !(config.current)() {
            return Err(EndpointError::Retired);
        }
        let policy = policy::Policy::new(&config)?;
        let owner = budget.owner()?;
        let stop = Arc::new(AtomicBool::new(false));
        let current: Current = {
            let stop = stop.clone();
            let current = config.current.clone();
            Arc::new(move || !stop.load(Ordering::Acquire) && current())
        };
        let mut entropy = [0; 32];
        use rand::TryRngCore;
        rand::rngs::OsRng
            .try_fill_bytes(&mut entropy)
            .map_err(|_| EndpointError::Worker)?;
        let token = EndpointToken::from_random_bytes(entropy);
        let mut entropy = [0; 16];
        rand::rngs::OsRng
            .try_fill_bytes(&mut entropy)
            .map_err(|_| EndpointError::Worker)?;
        let realm = ProxyRealm::from_random_bytes(entropy);
        let listener = TcpListener::bind(("127.0.0.1", config.port))
            .await
            .map_err(|error| bind_error(ListenerRole::Endpoint, config.port, &error))?;
        let address = listener.local_addr();
        #[cfg(test)]
        let address = fault.endpoint_addr(address);
        let port = local_port(ListenerRole::Endpoint, address)?;
        let proxy_uri = match config.transport {
            Transport::Socks => token.proxy_uri(port),
            Transport::HttpProxy => format!("http://127.0.0.1:{port}"),
        };
        if !current() {
            return Err(EndpointError::Retired);
        }
        let certificates =
            Certificates::new(current.clone(), budget.certificates(), runtime.clone());
        let trust = trust
            .map(|config| {
                trust::Gate::new(
                    runtime.clone(),
                    current.clone(),
                    budget.clone(),
                    config,
                    #[cfg(test)]
                    fault,
                )
            })
            .transpose()?;
        let state = Arc::new(State {
            config,
            policy,
            current,
            stop,
            token,
            realm,
            certificates,
            budget,
            trust,
        });
        let (stop, receiver) = watch::channel(false);
        let task = runtime.spawn(listen(state.clone(), listener, receiver, owner));
        Ok(Self {
            state,
            proxy_uri,
            port,
            stop,
            task: Some(task),
        })
    }
    #[cfg_attr(
        not(any(target_os = "linux", test)),
        expect(
            dead_code,
            reason = "only the WebKitGTK adapter retains HTTP reloads and probes native trust"
        )
    )]
    pub(crate) fn proxy_uri(&self) -> String {
        self.proxy_uri.clone()
    }
    #[cfg_attr(
        not(any(windows, target_os = "macos", test)),
        expect(
            dead_code,
            reason = "only the WebView2 and WebKit adapters keep their proxy port across a Reload"
        )
    )]
    pub(crate) fn port(&self) -> u16 {
        self.port
    }
    /// Basic proxy credentials for the HttpProxy transport; None for SOCKS.
    #[cfg_attr(
        not(any(windows, target_os = "macos", test)),
        expect(
            dead_code,
            reason = "only the WebView2 and WebKit adapters authenticate through the HTTP proxy front"
        )
    )]
    pub(crate) fn proxy_credentials(&self) -> Option<ProxyCredentials> {
        if self.state.config.transport != Transport::HttpProxy {
            return None;
        }
        let (user, password) = proxy::credentials(&self.state.token);
        Some(ProxyCredentials {
            uri: format!("{}/", self.proxy_uri),
            challenge: self.state.realm.challenge(),
            user,
            password,
        })
    }
    pub(crate) fn expected_leaf(&self, host: &str, port: u16) -> Option<[u8; 32]> {
        self.state.certificates.expected_leaf(host, port)
    }
    pub(crate) fn close(&self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(trust) = &self.state.trust {
            trust.close();
        }
        self.stop.send_replace(true);
    }
    pub(crate) async fn join(mut self) -> Result<(), EndpointError> {
        self.close();
        if let Some(task) = self.task.take() {
            task.await.map_err(|_| EndpointError::Worker)??;
        }
        Ok(())
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.close();
    }
}
async fn listen(
    state: Arc<State>,
    listener: TcpListener,
    mut stop: watch::Receiver<bool>,
    _owner: budget::Owner,
) -> Result<(), EndpointError> {
    let mut tasks = JoinSet::new();
    while (state.current)() {
        tokio::select! {
            _=stop.changed()=>break,
            result=listener.accept()=>match result {
                Ok((socket,_))=>spawn_connection(&state, &mut tasks, socket),
                Err(error)=>{tracing::warn!(%error,"native mirror listener failed");break;}
            },
            result=tasks.join_next(),if !tasks.is_empty()=>connection_finished(result)
        }
    }
    drop(listener);
    state.certificates.retire();
    if let Some(trust) = &state.trust {
        trust.close();
    }
    retire_connections(&mut tasks).await;
    let result = if let Some(trust) = &state.trust {
        trust.join().await
    } else {
        Ok(())
    };
    state.certificates.settled().await;
    result
}
fn spawn_connection(state: &Arc<State>, tasks: &mut JoinSet<()>, socket: TcpStream) {
    let Ok(permit) = state.budget.socket() else {
        drop(socket);
        return;
    };
    let state = state.clone();
    tasks.spawn(async move {
        let _permit = permit;
        if let Err(error) = connection(state, socket).await {
            tracing::debug!(?error, "native mirror connection ended");
        }
    });
}
fn connection_finished(result: Option<Result<(), tokio::task::JoinError>>) {
    if let Some(Err(error)) = result {
        tracing::warn!(%error, "native mirror connection task failed");
    }
}
async fn retire_connections(tasks: &mut JoinSet<()>) {
    tasks.abort_all();
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result
            && !error.is_cancelled()
        {
            tracing::warn!(%error,"native mirror connection retirement failed");
        }
    }
}
async fn connection(state: Arc<State>, socket: TcpStream) -> Result<(), EndpointError> {
    let deadline = state.config.limits.io_timeout;
    let mut io = CurrentConnection::new(socket, state.current.clone());
    if state.config.transport == Transport::HttpProxy {
        return proxy_connection(state, io).await;
    }
    let route = endpoint::authenticate(&mut io, &state.token, deadline)
        .await
        .map_err(|_| EndpointError::Auth)?;
    let route = Route::new(&state, &route.host, route.port)?;
    if !(state.current)() || !state.policy.candidate(&route.logical) {
        endpoint::reject_route(&mut io)
            .await
            .map_err(|_| EndpointError::Io)?;
        return Err(EndpointError::Destination);
    }
    endpoint::accept_route(&mut io)
        .await
        .map_err(|_| EndpointError::Io)?;
    routed(state, io, route).await
}
/// The authority the native engine addressed and the remote authority it stands for. They
/// differ only for a loopback alias.
#[derive(Clone)]
pub(super) struct Route {
    pub wire: Authority,
    pub logical: Authority,
}
impl Route {
    fn new(state: &State, host: &str, port: u16) -> Result<Self, EndpointError> {
        let wire = Authority::new(host, port).map_err(|_| EndpointError::Destination)?;
        let logical = match state
            .config
            .loopback_alias
            .then(|| super::alias::logical_host(wire.host()))
            .flatten()
        {
            Some(host) => Authority::new(&host, port).map_err(|_| EndpointError::Destination)?,
            None => wire.clone(),
        };
        Ok(Self { wire, logical })
    }
}
async fn proxy_connection(
    state: Arc<State>,
    mut io: CurrentConnection<TcpStream>,
) -> Result<(), EndpointError> {
    let deadline = state.config.limits.io_timeout;
    let route = proxy::authenticate_http(&mut io, &state.token, &state.realm, deadline)
        .await
        .map_err(|_| EndpointError::Auth)?;
    let (route, head) = match route {
        ProxyRoute::Tunnel(route) => (route, None),
        ProxyRoute::Forward { authority, head } => (authority, Some(head)),
    };
    let route = Route::new(&state, &route.host, route.port)?;
    let admitted = match head {
        None => state.policy.candidate(&route.logical),
        Some(_) => state.policy.permits("http", &route.logical),
    };
    if !(state.current)() || !admitted {
        proxy::reject_proxy_route(&mut io)
            .await
            .map_err(|_| EndpointError::Io)?;
        return Err(EndpointError::Destination);
    }
    if let Some(head) = head {
        return request::serve(state, Prefixed::new(head, io), route, "http").await;
    }
    proxy::accept_tunnel(&mut io)
        .await
        .map_err(|_| EndpointError::Io)?;
    routed(state, io, route).await
}
async fn routed(
    state: Arc<State>,
    io: CurrentConnection<TcpStream>,
    route: Route,
) -> Result<(), EndpointError> {
    let deadline = state.config.limits.io_timeout;
    let first = tokio::time::timeout(deadline, io.first_byte())
        .await
        .map_err(|_| EndpointError::Io)?
        .map_err(|_| EndpointError::Io)?;
    let scheme = if first == 22 { "https" } else { "http" };
    if !(state.current)() || !state.policy.permits(scheme, &route.logical) {
        return Err(EndpointError::Destination);
    }
    if scheme == "https" {
        // The leaf names the host the native engine addressed and checks.
        let certificate = state
            .certificates
            .certificate(route.wire.clone())
            .await
            .map_err(|_| EndpointError::Tls)?;
        if let Some(trust) = &state.trust {
            let io = trust::hello::capture(io, &route.wire, deadline).await?;
            trust
                .wait(&state, route.wire.clone(), certificate.clone())
                .await?;
            serve_tls(state, io, route, certificate).await
        } else {
            serve_tls(state, io, route, certificate).await
        }
    } else {
        request::serve(state, io, route, scheme).await
    }
}
async fn serve_tls<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    state: Arc<State>,
    io: S,
    route: Route,
    certificate: Arc<super::certificates::AuthorityTls>,
) -> Result<(), EndpointError> {
    let io = certificate
        .accept(io, state.config.limits.io_timeout)
        .await
        .map_err(|_| EndpointError::Tls)?;
    if !sni_matches(&route.wire, io.server_name()) {
        return Err(EndpointError::Destination);
    }
    request::serve(state, io, route, "https").await
}
fn sni_matches(authority: &Authority, sni: Option<&str>) -> bool {
    match sni {
        None => authority.host().parse::<std::net::IpAddr>().is_ok(),
        Some(host) => Authority::new(host, authority.port())
            .is_ok_and(|supplied| supplied.host() == authority.host()),
    }
}
#[cfg(test)]
mod tests;
