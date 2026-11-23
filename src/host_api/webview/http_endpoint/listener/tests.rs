//! Actual source SOCKS/TCP/HTTP boundary; no browser/App or upstream executor claim.
use super::*;
use sha2::{Digest, Sha256};
use std::{sync::atomic::AtomicUsize, time::Duration};
use tasty_ipc::{
    html_http::{BodyIdentity, Header, HttpRequest, ResponseHead, ResponseId},
    html_transfer::TransferId,
};
use tasty_remote::html_http_body::{self, BodyBudget};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
struct Reply {
    body: Arc<VerifiedBody>,
    request: Arc<NativeRequest>,
}
impl ResponseOwner for Reply {
    fn body(&self) -> &Arc<VerifiedBody> {
        &self.body
    }
    fn request(&self) -> &Arc<NativeRequest> {
        &self.request
    }
}
fn identity() -> DocumentIdentity {
    DocumentIdentity {
        epoch: 1,
        surface: 42,
        binding: 1,
        document: 1,
        source_generation: 1,
    }
}
fn config(url: String, handler: Handler) -> EndpointConfig {
    config_with(url, handler, Transport::Socks)
}
fn config_with(url: String, handler: Handler, transport: Transport) -> EndpointConfig {
    EndpointConfig {
        transport,
        port: 0,
        loopback_alias: false,
        identity: identity(),
        entry_url: url,
        allow_remote_content: false,
        https: true,
        current: Arc::new(|| true),
        limits: NativeLimits {
            upload_bytes: 1_000_000,
            io_timeout: Duration::from_secs(2),
        },
        native_budget: NativeBudget::new(4_000_000),
        handler,
    }
}
fn reply(request: Arc<NativeRequest>) -> Box<dyn ResponseOwner> {
    reply_as(request, 1)
}
fn reply_as(request: Arc<NativeRequest>, generation: u64) -> Box<dyn ResponseOwner> {
    let id = TransferId {
        epoch: 1,
        surface_id: 42,
        binding: 1,
        document: 1,
        request: 1,
        transfer: 1,
    };
    let response = ResponseId {
        request: id,
        source_generation: generation,
        serial: 1,
    };
    let begin = HttpRequest::Begin {
        id,
        source_generation: generation,
        method: request.method.clone(),
        logical_url: request.logical_url.clone(),
        headers: request.headers.clone(),
        upload: request.identity,
        allow_remote_content: false,
    };
    let bytes = Arc::new(b"REMOTE body".to_vec());
    let digest = BodyIdentity {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(&*bytes).into(),
    };
    let head = ResponseHead {
        id: response,
        logical_url: request.logical_url.clone(),
        status: 200,
        headers: vec![
            Header {
                name: "Set-Cookie".into(),
                value: b"one=1".to_vec(),
            },
            Header {
                name: "Set-Cookie".into(),
                value: b"two=2".to_vec(),
            },
            Header {
                name: "X-Bytes".into(),
                value: vec![255],
            },
        ],
        encoded_length: Some(bytes.len() as u64),
        has_body: true,
    };
    let body = html_http_body::verify(
        &begin,
        head,
        bytes,
        (digest, digest),
        &BodyBudget::new(1_000_000),
        1_000_000,
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|_| panic!("verified fixture response"));
    Box::new(Reply { body, request })
}
async fn authenticated(endpoint: &Endpoint, password: Option<&str>) -> TcpStream {
    let proxy = url::Url::parse(&endpoint.proxy_uri()).unwrap();
    let mut socket = TcpStream::connect(("127.0.0.1", proxy.port().unwrap()))
        .await
        .unwrap();
    socket.write_all(&[5, 1, 2]).await.unwrap();
    let mut status = [0; 2];
    socket.read_exact(&mut status).await.unwrap();
    assert_eq!(status, [5, 2]);
    let password = password.unwrap_or_else(|| proxy.password().unwrap());
    let mut auth = vec![1, 5];
    auth.extend_from_slice(b"tasty");
    auth.push(password.len() as u8);
    auth.extend_from_slice(password.as_bytes());
    socket.write_all(&auth).await.unwrap();
    socket.read_exact(&mut status).await.unwrap();
    assert_eq!(
        status,
        if password == proxy.password().unwrap() {
            [1, 0]
        } else {
            [1, 1]
        }
    );
    socket
}
async fn route(socket: &mut TcpStream, host: &str, port: u16) -> [u8; 10] {
    let mut bytes = vec![5, 1, 0, 3, host.len() as u8];
    bytes.extend_from_slice(host.as_bytes());
    bytes.extend_from_slice(&port.to_be_bytes());
    socket.write_all(&bytes).await.unwrap();
    let mut reply = [0; 10];
    socket.read_exact(&mut reply).await.unwrap();
    reply
}
fn close_observation(result: std::io::Result<usize>) -> bool {
    matches!(result, Ok(0))
        || matches!(result,Err(ref error) if matches!(error.kind(),std::io::ErrorKind::ConnectionReset|std::io::ErrorKind::BrokenPipe))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_authenticated_binary_post_preserves_request_and_verified_response_without_target_connect()
 {
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
            assert!(
                request
                    .headers
                    .iter()
                    .any(|h| h.name.eq_ignore_ascii_case("cookie") && h.value == b"browser=kept")
            );
            Ok(reply(request))
        })
    });
    let config = config(format!("http://127.0.0.1:{port}/doc"), handler);
    let native = config.native_budget.clone();
    let budget = EndpointBudget::new();
    let endpoint = Endpoint::start(Handle::current(), config, budget.clone())
        .await
        .unwrap_or_else(|_| panic!("endpoint"));
    let mut socket = authenticated(&endpoint, None).await;
    assert_eq!(route(&mut socket, "127.0.0.1", port).await[1], 0);
    socket.write_all(format!("POST /doc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: browser=kept\r\nContent-Length: 3\r\n\r\n").as_bytes()).await.unwrap();
    socket.write_all(b"\0\xffx").await.unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.starts_with(b"HTTP/1.1 200 \r\n"));
    assert!(bytes.ends_with(b"REMOTE body"));
    assert_eq!(
        bytes.windows(12).filter(|s| *s == b"Set-Cookie: ").count(),
        2
    );
    assert!(bytes.windows(12).any(|s| s == b"X-Bytes: \xff\r\n"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(
        sentinel.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    endpoint.join().await.unwrap();
    assert_eq!(native.used(), 0);
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unauthenticated_bad_token_and_cross_origin_routes_close_before_body_admission() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let handler: Handler = Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move { Ok(reply(request)) })
    });
    let budget = EndpointBudget::new();
    let endpoint = Endpoint::start(
        Handle::current(),
        config("http://remote.example:8080/doc".into(), handler),
        budget.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("endpoint"));
    let proxy = url::Url::parse(&endpoint.proxy_uri()).unwrap();
    let mut none = TcpStream::connect(("127.0.0.1", proxy.port().unwrap()))
        .await
        .unwrap();
    none.write_all(&[5, 1, 0]).await.unwrap();
    let mut status = [0; 2];
    none.read_exact(&mut status).await.unwrap();
    assert_eq!(status, [5, 255]);
    let mut end = [0];
    assert!(close_observation(none.read(&mut end).await));
    let mut bad = authenticated(&endpoint, Some("wrong")).await;
    assert!(close_observation(bad.read(&mut end).await));
    let mut foreign = authenticated(&endpoint, None).await;
    assert_eq!(route(&mut foreign, "other.example", 8080).await[1], 2);
    assert!(close_observation(foreign.read(&mut end).await));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(endpoint.expected_leaf("other.example", 8080).is_none());
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socket_capacity_precedes_authentication_and_retirement_releases_only_after_join() {
    let handler: Handler = Arc::new(|_| Box::pin(async { Err(EndpointError::Body) }));
    let budget = EndpointBudget::new();
    let endpoint = Endpoint::start(
        Handle::current(),
        config("http://remote.example:8080/".into(), handler),
        budget.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("endpoint"));
    let proxy = url::Url::parse(&endpoint.proxy_uri()).unwrap();
    let mut sockets = Vec::new();
    for _ in 0..8 {
        sockets.push(
            TcpStream::connect(("127.0.0.1", proxy.port().unwrap()))
                .await
                .unwrap(),
        );
    }
    let wait_until = std::time::Instant::now() + Duration::from_secs(1);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if budget.active_connections() == 8 {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                budget.active_connections() == 8,
                "not reached within {:?}: budget.active_connections() == 8",
                Duration::from_secs(1)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
    let mut excess = TcpStream::connect(("127.0.0.1", proxy.port().unwrap()))
        .await
        .unwrap();
    let mut byte = [0];
    assert!(close_observation(
        tokio::time::timeout(Duration::from_secs(1), excess.read(&mut byte))
            .await
            .unwrap()
    ));
    assert_eq!(budget.active_connections(), 8);
    endpoint.join().await.unwrap();
    for mut socket in sockets {
        assert!(close_observation(socket.read(&mut byte).await));
    }
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn body_question_wait_does_not_consume_io_deadline_and_close_drops_the_owned_waiter() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let started = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let waiting = started.clone();
    let done = dropped.clone();
    let handler: Handler = Arc::new(move |_| {
        waiting.store(true, Ordering::Release);
        let guard = Dropped(done.clone());
        Box::pin(async move {
            let _guard = guard;
            std::future::pending().await
        })
    });
    let budget = EndpointBudget::new();
    let mut config = config("http://remote.example:8080/doc".into(), handler);
    config.limits.io_timeout = Duration::from_millis(20);
    let native = config.native_budget.clone();
    let endpoint = Endpoint::start(Handle::current(), config, budget.clone())
        .await
        .unwrap_or_else(|_| panic!("endpoint"));
    let mut socket = authenticated(&endpoint, None).await;
    assert_eq!(route(&mut socket, "remote.example", 8080).await[1], 0);
    socket
        .write_all(b"GET /doc HTTP/1.1\r\nHost: remote.example:8080\r\n\r\n")
        .await
        .unwrap();
    let wait_until = std::time::Instant::now() + Duration::from_secs(1);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if started.load(Ordering::Acquire) {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                started.load(Ordering::Acquire),
                "not reached within {:?}: started.load(Ordering::Acquire)",
                Duration::from_secs(1)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(budget.active_connections(), 1);
    assert!(native.used() > 0);
    let mut byte = [0];
    assert_eq!(
        socket.try_read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    endpoint.join().await.unwrap();
    assert!(dropped.load(Ordering::Acquire));
    assert_eq!(native.used(), 0);
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
    assert!(close_observation(socket.read(&mut byte).await));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn endpoint_owner_capacity_and_invalid_metadata_precede_listener_creation() {
    let handler: Handler = Arc::new(|_| Box::pin(async { Err(EndpointError::Body) }));
    let budget = EndpointBudget::new();
    let mut invalid = config("http://remote.example:8080/doc".into(), handler.clone());
    invalid.identity.source_generation = 0;
    assert!(matches!(
        Endpoint::start(Handle::current(), invalid, budget.clone()).await,
        Err(EndpointError::Invalid)
    ));
    assert_eq!(budget.active_owners(), 0);
    let mut owners = Vec::new();
    for _ in 0..8 {
        owners.push(
            Endpoint::start(
                Handle::current(),
                config("http://remote.example:8080/doc".into(), handler.clone()),
                budget.clone(),
            )
            .await
            .unwrap_or_else(|_| panic!("bounded owner")),
        );
    }
    assert_eq!(budget.active_owners(), 8);
    assert!(matches!(
        Endpoint::start(
            Handle::current(),
            config("http://remote.example:8080/doc".into(), handler),
            budget.clone()
        )
        .await,
        Err(EndpointError::Capacity)
    ));
    for owner in owners {
        owner.join().await.unwrap();
    }
    assert_eq!(budget.active_owners(), 0);
    assert_eq!(budget.active_connections(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_body_from_a_different_source_generation_is_rejected_before_status() {
    let handler: Handler = Arc::new(|request| Box::pin(async move { Ok(reply_as(request, 2)) }));
    let budget = EndpointBudget::new();
    let endpoint = Endpoint::start(
        Handle::current(),
        config("http://remote.example:8080/doc".into(), handler),
        budget.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("endpoint"));
    let mut socket = authenticated(&endpoint, None).await;
    assert_eq!(route(&mut socket, "remote.example", 8080).await[1], 0);
    socket
        .write_all(b"GET /doc HTTP/1.1\r\nHost: remote.example:8080\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.is_empty());
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
}

pub(super) mod fault;

mod http_proxy;

mod tls;

mod preparation;

mod trust;
