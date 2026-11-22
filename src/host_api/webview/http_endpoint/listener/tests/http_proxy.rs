//! Real TCP Basic-proxy → HTTP/TLS controls for the WebView2 transport; not WebView2 evidence.
use super::{tls::Pinned, *};
use base64::Engine;
use tokio_rustls::{TlsConnector, rustls};
fn counting(calls: &Arc<AtomicUsize>) -> Handler {
    let seen = calls.clone();
    Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move { Ok(reply(request)) })
    })
}
async fn start(url: &str, handler: Handler, budget: &EndpointBudget) -> Endpoint {
    Endpoint::start(
        Handle::current(),
        config_with(url.into(), handler, Transport::HttpProxy),
        budget.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("endpoint"))
}
fn authorization(endpoint: &Endpoint) -> String {
    let credentials = endpoint.proxy_credentials().unwrap();
    format!(
        "Proxy-Authorization: Basic {}\r\n",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", credentials.user, credentials.password))
    )
}
fn proxy_port(endpoint: &Endpoint) -> u16 {
    url::Url::parse(&endpoint.proxy_uri())
        .unwrap()
        .port()
        .unwrap()
}
/// A refused or completed connection may reset or end without close_notify after its reply.
fn tolerate_close(result: std::io::Result<usize>) {
    if let Err(error) = result {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::UnexpectedEof
            ),
            "{error}"
        );
    }
}
async fn exchange(endpoint: &Endpoint, request: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect(("127.0.0.1", proxy_port(endpoint)))
        .await
        .unwrap();
    socket.write_all(request).await.unwrap();
    socket.shutdown().await.unwrap();
    let mut bytes = Vec::new();
    tolerate_close(socket.read_to_end(&mut bytes).await);
    bytes
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proxy_credentials_describe_only_the_http_transport() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let socks = Endpoint::start(
        Handle::current(),
        config("http://remote.example/doc".into(), counting(&calls)),
        budget.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("endpoint"));
    assert!(socks.proxy_credentials().is_none());
    let http = start("http://remote.example/doc", counting(&calls), &budget).await;
    let credentials = http.proxy_credentials().unwrap();
    assert_eq!(credentials.uri, format!("{}/", http.proxy_uri()));
    assert!(credentials.uri.starts_with("http://127.0.0.1:"));
    assert!(credentials.challenge.starts_with("Basic realm=\"tasty-"));
    assert_eq!(credentials.user, "tasty");
    assert_eq!(credentials.password.len(), 64);
    assert!(!http.proxy_uri().contains(&credentials.password));
    socks.join().await.unwrap();
    http.join().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unauthenticated_request_gets_the_challenge_and_never_reaches_the_handler() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let endpoint = start("http://remote.example/doc", counting(&calls), &budget).await;
    let bytes = exchange(
        &endpoint,
        b"POST http://remote.example/doc HTTP/1.1\r\nHost: remote.example\r\nContent-Length: 3\r\n\r\nabc",
    )
    .await;
    let challenge = format!(
        "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: {}\r\n",
        endpoint.proxy_credentials().unwrap().challenge
    );
    assert!(bytes.starts_with(challenge.as_bytes()));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_absolute_form_post_reaches_the_handler_without_proxy_headers() {
    let sentinel = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    sentinel.set_nonblocking(true).unwrap();
    let port = sentinel.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let handler: Handler = Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            assert_eq!(request.method, "POST");
            assert_eq!(request.upload.as_slice(), b"\0\xffx");
            assert!(request.logical_url.ends_with("/doc?q=1"));
            assert!(request.headers.iter().all(|h| {
                !h.name.eq_ignore_ascii_case("proxy-authorization")
                    && !h.name.eq_ignore_ascii_case("proxy-connection")
            }));
            assert!(
                request
                    .headers
                    .iter()
                    .any(|h| h.name.eq_ignore_ascii_case("cookie") && h.value == b"browser=kept")
            );
            Ok(reply(request))
        })
    });
    let budget = EndpointBudget::new();
    let endpoint = start(&format!("http://127.0.0.1:{port}/doc"), handler, &budget).await;
    let auth = authorization(&endpoint);
    let request = [
        format!(
            "POST http://127.0.0.1:{port}/doc?q=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nProxy-Connection: keep-alive\r\n{auth}Cookie: browser=kept\r\nContent-Length: 3\r\n\r\n"
        )
        .into_bytes(),
        b"\0\xffx".to_vec(),
    ]
    .concat();
    let bytes = exchange(&endpoint, &request).await;
    assert!(bytes.starts_with(b"HTTP/1.1 200 \r\n"));
    assert!(bytes.ends_with(b"REMOTE body"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(
        sentinel.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_origin_and_websocket_routes_are_refused_before_the_handler() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let endpoint = start("http://remote.example/doc", counting(&calls), &budget).await;
    let auth = authorization(&endpoint);
    for request in [
        format!("GET http://other.example/ HTTP/1.1\r\nHost: other.example\r\n{auth}\r\n"),
        format!("CONNECT other.example:443 HTTP/1.1\r\nHost: other.example:443\r\n{auth}\r\n"),
    ] {
        let bytes = exchange(&endpoint, request.as_bytes()).await;
        assert!(
            bytes.starts_with(b"HTTP/1.1 403 Forbidden\r\n"),
            "{request}"
        );
    }
    let tunnel = format!(
        "CONNECT remote.example:80 HTTP/1.1\r\nHost: remote.example:80\r\n{auth}\r\nGET /ws HTTP/1.1\r\nHost: remote.example\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    let bytes = exchange(&endpoint, tunnel.as_bytes()).await;
    assert_eq!(bytes, b"HTTP/1.1 200 Connection Established\r\n\r\n");
    assert_eq!(calls.load(Ordering::Acquire), 0);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_connect_terminates_the_mapped_tls_leaf_before_http() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let handler: Handler = Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            assert_eq!(request.logical_url, "https://remote.example:8443/doc");
            Ok(reply(request))
        })
    });
    let budget = EndpointBudget::new();
    let endpoint = start("https://remote.example:8443/doc", handler, &budget).await;
    let prepared = endpoint
        .prepare_https("https://remote.example:8443/doc")
        .unwrap()
        .await
        .unwrap_or_else(|_| panic!("prepared endpoint authority"));
    let mut socket = TcpStream::connect(("127.0.0.1", proxy_port(&endpoint)))
        .await
        .unwrap();
    socket
        .write_all(
            format!(
                "CONNECT remote.example:8443 HTTP/1.1\r\nHost: remote.example:8443\r\n{}\r\n",
                authorization(&endpoint)
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut established = [0; 39];
    socket.read_exact(&mut established).await.unwrap();
    assert_eq!(&established, b"HTTP/1.1 200 Connection Established\r\n\r\n");
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(Pinned(prepared.leaf())))
    .with_no_client_auth();
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(
            rustls::pki_types::ServerName::try_from("remote.example").unwrap(),
            socket,
        )
        .await
        .unwrap();
    tls.write_all(b"GET /doc HTTP/1.1\r\nHost: remote.example:8443\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = Vec::new();
    tolerate_close(tls.read_to_end(&mut bytes).await);
    assert!(bytes.starts_with(b"HTTP/1.1 200 \r\n"));
    assert!(bytes.ends_with(b"REMOTE body"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    drop(tls);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
}
fn proxy_config(port: u16, handler: Handler) -> EndpointConfig {
    let mut config = config_with(
        "http://remote.example/doc".into(),
        handler,
        Transport::HttpProxy,
    );
    config.port = port;
    config
}
fn port_taken(error: EndpointError) -> bool {
    matches!(
        error,
        EndpointError::Bind {
            kind: std::io::ErrorKind::AddrInUse,
            ..
        }
    )
}
#[derive(Clone, Copy, Debug)]
enum Stage {
    First,
    Successor,
}
/// The stage that failed, the port it asked for (0 for the first endpoint, the inherited port for
/// the successor) and its error.
type Failure = (Stage, u16, EndpointError);
async fn rebind_after_join(
    calls: &Arc<AtomicUsize>,
    budget: &EndpointBudget,
) -> Result<(Endpoint, u16, String), Failure> {
    let first = Endpoint::start(
        Handle::current(),
        proxy_config(0, counting(calls)),
        budget.clone(),
    )
    .await
    .map_err(|error| (Stage::First, 0, error))?;
    let port = first.port();
    assert_eq!(port, proxy_port(&first));
    let old = authorization(&first);
    first.join().await.unwrap();
    let second = Endpoint::start(
        Handle::current(),
        proxy_config(port, counting(calls)),
        budget.clone(),
    )
    .await
    .map_err(|error| (Stage::Successor, port, error))?;
    Ok((second, port, old))
}
/// The released port can be taken by any socket on the host before the successor binds it, as in
/// the product's retained Reload. Only that observed `AddrInUse` starts the pair over, at most
/// three attempts in all.
///
/// The test does not observe who holds the port. A predecessor that never releases it fails all
/// three attempts, but one that keeps it only now and then is hidden when a later attempt gets a
/// new port. Telling the predecessor from another socket would need I/O to whatever listens there,
/// which may never answer, or per-process socket tables that only Linux exposes and that cannot
/// single out the predecessor among the other loopback sockets of this test process.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successor_rebinds_the_joined_address_with_fresh_credentials() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut attempts = Vec::new();
    let (second, port, old) = loop {
        match rebind_after_join(&calls, &budget).await {
            Ok(rebound) => break rebound,
            Err(failure @ (Stage::Successor, _, error)) if port_taken(error) => {
                attempts.push(failure);
                assert_eq!(budget.active_owners(), 0, "{attempts:?}");
                assert!(
                    attempts.len() < 3,
                    "successor bind reported AddrInUse on every attempt: {attempts:?}"
                );
            }
            Err(failure) => {
                attempts.push(failure);
                panic!("successor endpoint: {attempts:?}");
            }
        }
    };
    assert_eq!(second.port(), port);
    assert_ne!(authorization(&second), old);
    let request =
        format!("GET http://remote.example/doc HTTP/1.1\r\nHost: remote.example\r\n{old}\r\n");
    let bytes = exchange(&second, request.as_bytes()).await;
    assert!(bytes.starts_with(b"HTTP/1.1 407 "));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    second.join().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_port_held_by_another_socket_fails_the_bind_as_address_in_use() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let holder = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = holder.local_addr().unwrap().port();
    let error = Endpoint::start(
        Handle::current(),
        proxy_config(port, counting(&calls)),
        budget.clone(),
    )
    .await
    .err()
    .expect("bind on a held port");
    let EndpointError::Bind {
        kind: std::io::ErrorKind::AddrInUse,
        os_error: Some(code),
    } = error
    else {
        panic!("{error:?}");
    };
    assert_eq!(
        std::io::Error::from_raw_os_error(code).kind(),
        std::io::ErrorKind::AddrInUse
    );
    assert_eq!(budget.active_owners(), 0);
    drop(holder);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_alias_tunnel_reaches_the_handler_as_the_remote_loopback_host() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let handler: Handler = Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            assert_eq!(request.logical_url, "http://localhost:8080/doc");
            let header = |name: &str| {
                request
                    .headers
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case(name))
                    .map(|h| h.value.clone())
            };
            assert_eq!(
                header("host").as_deref(),
                Some(b"localhost:8080".as_slice())
            );
            assert_eq!(
                header("origin").as_deref(),
                Some(b"http://localhost:8080".as_slice())
            );
            Ok(reply(request))
        })
    });
    let budget = EndpointBudget::new();
    let mut config = config_with(
        "http://localhost:8080/doc".into(),
        handler,
        Transport::HttpProxy,
    );
    config.loopback_alias = true;
    let endpoint = Endpoint::start(Handle::current(), config, budget.clone())
        .await
        .unwrap_or_else(|_| panic!("endpoint"));
    let auth = authorization(&endpoint);
    let tunnel = format!(
        "CONNECT localhost.tasty.localhost:8080 HTTP/1.1\r\nHost: localhost.tasty.localhost:8080\r\n{auth}\r\nGET /doc HTTP/1.1\r\nHost: localhost.tasty.localhost:8080\r\nOrigin: http://localhost.tasty.localhost:8080\r\n\r\n"
    );
    let bytes = exchange(&endpoint, tunnel.as_bytes()).await;
    assert!(bytes.starts_with(b"HTTP/1.1 200 Connection Established\r\n\r\nHTTP/1.1 200 \r\n"));
    assert!(bytes.ends_with(b"REMOTE body"));
    // Another loopback host is a different origin.
    let other = format!(
        "CONNECT 127-0-0-1.tasty.localhost:8080 HTTP/1.1\r\nHost: 127-0-0-1.tasty.localhost:8080\r\n{auth}\r\n"
    );
    let bytes = exchange(&endpoint, other.as_bytes()).await;
    assert!(bytes.starts_with(b"HTTP/1.1 403 Forbidden\r\n"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    endpoint.join().await.unwrap();
    // Without aliasing the alias is only a foreign host name.
    let endpoint = start("http://localhost:8080/doc", counting(&calls), &budget).await;
    let auth = authorization(&endpoint);
    let bytes = exchange(
        &endpoint,
        format!("CONNECT localhost.tasty.localhost:8080 HTTP/1.1\r\nHost: localhost.tasty.localhost:8080\r\n{auth}\r\n").as_bytes(),
    )
    .await;
    assert!(bytes.starts_with(b"HTTP/1.1 403 Forbidden\r\n"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
}
