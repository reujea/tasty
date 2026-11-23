//! Real TLS wire controls with an opaque synthetic native actor; no GTK evidence is claimed.
use super::super::trust::{NativeTrust, TrustConfig, TrustObservation, TrustRequest};
use super::*;
use tokio_rustls::{TlsConnector, rustls};
fn client(leaf: Option<[u8; 32]>) -> Arc<rustls::ClientConfig> {
    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap();
    Arc::new(match leaf {
        Some(leaf) => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(tls::Pinned(leaf)))
            .with_no_client_auth(),
        None => builder
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth(),
    })
}
async fn start(
    timeout: Duration,
    budget: EndpointBudget,
    active: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
) -> (Endpoint, NativeTrust) {
    let handler: Handler = Arc::new(move |request| {
        calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            assert_eq!(request.upload.as_slice(), b"\0\xffx");
            Ok(reply(request))
        })
    });
    let mut settings = config("https://remote.example:8443/doc".into(), handler);
    settings.current = Arc::new(move || active.load(Ordering::Acquire));
    Endpoint::start_with_native_trust(
        Handle::current(),
        settings,
        budget,
        TrustConfig {
            timeout,
            wake: Arc::new(|| {}),
        },
    )
    .await
    .unwrap_or_else(|_| panic!("trust endpoint"))
}
async fn request(native: &NativeTrust) -> TrustRequest {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(request) = native.poll() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap()
}
async fn hello(endpoint: &Endpoint, sni: &'static str) -> (TcpStream, rustls::ClientConnection) {
    let mut socket = authenticated(endpoint, None).await;
    assert_eq!(route(&mut socket, "remote.example", 8443).await[1], 0);
    let mut client = rustls::ClientConnection::new(
        client(endpoint.expected_leaf("remote.example", 8443)),
        rustls::pki_types::ServerName::try_from(sni).unwrap(),
    )
    .unwrap();
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).unwrap();
    socket.write_all(&bytes).await.unwrap();
    (socket, client)
}
async fn proxy(uri: &str, password: Option<&str>) -> TcpStream {
    let uri = url::Url::parse(uri).unwrap();
    let mut socket = TcpStream::connect(("127.0.0.1", uri.port().unwrap()))
        .await
        .unwrap();
    socket.write_all(&[5, 1, 2]).await.unwrap();
    let mut status = [0; 2];
    socket.read_exact(&mut status).await.unwrap();
    assert_eq!(status, [5, 2]);
    let pass = password.unwrap_or_else(|| uri.password().unwrap());
    let mut auth = vec![1, 5];
    auth.extend_from_slice(b"tasty");
    auth.push(pass.len() as u8);
    auth.extend_from_slice(pass.as_bytes());
    socket.write_all(&auth).await.unwrap();
    socket.read_exact(&mut status).await.unwrap();
    assert_eq!(
        status,
        if pass == uri.password().unwrap() {
            [1, 0]
        } else {
            [1, 1]
        }
    );
    socket
}
async fn probe(request: &TrustRequest, trusted: bool) {
    let mut socket = proxy(&request.probe_proxy_uri().unwrap(), None).await;
    assert_eq!(
        route(&mut socket, request.host(), request.port()).await[1],
        0
    );
    let result = TlsConnector::from(client(trusted.then(|| request.leaf())))
        .connect(
            rustls::pki_types::ServerName::try_from(request.host().to_owned()).unwrap(),
            socket,
        )
        .await;
    if trusted {
        let mut stream = result.unwrap();
        // Unexpectedly trusted probe sends a full binary HTTP request: it must never dispatch.
        let sent=stream.write_all(b"POST /doc HTTP/1.1\r\nHost: remote.example:8443\r\nContent-Length: 3\r\n\r\n\0\xffx").await;
        if let Err(error) = sent {
            tracing::debug!(%error,"TLS-only probe closed before HTTP write");
        }
        let mut byte = [0];
        assert!(matches!(stream.read(&mut byte).await, Ok(0) | Err(_)));
    } else {
        let error = result.expect_err("self-signed issuer rejected");
        let Some(rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer)) =
            error
                .get_ref()
                .and_then(|e| e.downcast_ref::<rustls::Error>())
        else {
            panic!("actual unknown issuer: {error:?}")
        };
    }
}
fn observation(request: &TrustRequest) -> TrustObservation {
    TrustObservation {
        host: request.host().into(),
        port: request.port(),
        leaf: request.leaf(),
        unknown_ca_only: true,
    }
}
async fn closed(socket: &mut TcpStream) {
    let mut byte = [0];
    assert!(close_observation(
        tokio::time::timeout(Duration::from_secs(1), socket.read(&mut byte))
            .await
            .unwrap()
    ));
}
async fn finish(mut socket: TcpStream, mut client: rustls::ClientConnection) -> Vec<u8> {
    let mut response = Vec::new();
    let mut posted = false;
    tokio::time::timeout(Duration::from_secs(2),async {
        loop {
            if !client.is_handshaking() && !posted { use std::io::Write;client.writer().write_all(b"POST /doc HTTP/1.1\r\nHost: remote.example:8443\r\nContent-Length: 3\r\n\r\n\0\xffx").unwrap();posted=true; }
            let mut bytes=Vec::new();while client.wants_write(){client.write_tls(&mut bytes).unwrap();}socket.write_all(&bytes).await.unwrap();
            let mut bytes=[0;8192];let count=socket.read(&mut bytes).await.unwrap();assert!(count>0);client.read_tls(&mut std::io::Cursor::new(&bytes[..count])).unwrap();client.process_new_packets().unwrap();
            use std::io::Read;let mut plain=[0;8192];loop{match client.reader().read(&mut plain){Ok(0)=>break,Ok(n)=>response.extend_from_slice(&plain[..n]),Err(error)if error.kind()==std::io::ErrorKind::WouldBlock=>break,Err(error)=>panic!("TLS response {error}")}}
            if response.ends_with(b"REMOTE body"){return;}
        }
    }).await.unwrap();
    response
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_has_zero_tls_before_native_ack_and_post_survives_private_unknown_ca_probe() {
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        calls.clone(),
    )
    .await;
    let ready = endpoint
        .prepare_https("https://remote.example:8443/doc")
        .unwrap()
        .await
        .unwrap_or_else(|_| panic!("pin"));
    let (mut socket, client) = hello(&endpoint, "remote.example").await;
    let request = request(&native).await;
    assert_eq!(request.identity().unwrap().document, 1);
    assert_eq!(request.leaf(), ready.leaf());
    let mut byte = [0];
    assert!(
        tokio::time::timeout(Duration::from_millis(30), socket.read(&mut byte))
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    probe(&request, false).await;
    assert!(request.ack(observation(&request)));
    assert!(!request.ack(observation(&request)));
    assert!(!request.reject());
    let response = finish(socket, client).await;
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_probes(), 0);
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_trust(), 1);
    drop(request);
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unexpectedly_trusted_probe_binary_http_never_dispatches_or_releases_original_without_ack()
{
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        calls.clone(),
    )
    .await;
    let (mut socket, _) = hello(&endpoint, "remote.example").await;
    let request = request(&native).await;
    probe(&request, true).await;
    let mut byte = [0];
    assert!(
        tokio::time::timeout(Duration::from_millis(30), socket.read(&mut byte))
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(request.reject());
    closed(&mut socket).await;
    endpoint.join().await.unwrap();
    drop(request);
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_wrong_leaf_authority_and_error_flags_are_terminal_once_only() {
    for mode in 0..5 {
        let budget = EndpointBudget::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let (endpoint, native) = start(
            Duration::from_secs(3),
            budget.clone(),
            Arc::new(AtomicBool::new(true)),
            calls.clone(),
        )
        .await;
        let (mut socket, _) = hello(&endpoint, "remote.example").await;
        let request = request(&native).await;
        if mode > 0 {
            probe(&request, false).await;
        }
        let mut observed = observation(&request);
        match mode {
            1 => observed.leaf[0] ^= 1,
            2 => observed.host = "foreign.example".into(),
            3 => observed.port = 443,
            4 => observed.unknown_ca_only = false,
            _ => {}
        }
        assert!(!request.ack(observed));
        assert!(!request.ack(observation(&request)));
        closed(&mut socket).await;
        assert_eq!(calls.load(Ordering::Acquire), 0);
        endpoint.join().await.unwrap();
        drop(request);
        assert_eq!(budget.active_trust(), 0);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retirement_deadline_inbox_drop_and_unanswered_drop_cancel_original_and_late_ack() {
    for mode in 0..4 {
        let budget = EndpointBudget::new();
        let active = Arc::new(AtomicBool::new(true));
        let calls = Arc::new(AtomicUsize::new(0));
        let (endpoint, native) = start(
            if mode == 0 {
                Duration::from_millis(120)
            } else {
                Duration::from_secs(3)
            },
            budget.clone(),
            active.clone(),
            calls.clone(),
        )
        .await;
        let (mut socket, _) = hello(&endpoint, "remote.example").await;
        let request = request(&native).await;
        match mode {
            0 => {}
            1 => active.store(false, Ordering::Release),
            2 => drop(native),
            _ => {
                drop(request);
                closed(&mut socket).await;
                endpoint.join().await.unwrap();
                assert_eq!(budget.active_trust(), 0);
                continue;
            }
        }
        closed(&mut socket).await;
        assert!(!request.ack(observation(&request)));
        assert_eq!(calls.load(Ordering::Acquire), 0);
        endpoint.join().await.unwrap();
        assert_eq!(budget.active_trust(), 1);
        drop(request);
        assert_eq!(budget.active_trust(), 0);
        assert_eq!(budget.active_probes(), 0);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn probe_credentials_exact_origin_and_sni_refuse_before_certificate() {
    let budget = EndpointBudget::new();
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let (mut original, _) = hello(&endpoint, "remote.example").await;
    let request = request(&native).await;
    let uri = request.probe_proxy_uri().unwrap();
    let main = url::Url::parse(&endpoint.proxy_uri()).unwrap();
    let mut wrong = proxy(&uri, main.password()).await;
    closed(&mut wrong).await;
    let mut wrong = proxy(&uri, None).await;
    assert_eq!(route(&mut wrong, "foreign.example", 8443).await[1], 2);
    closed(&mut wrong).await;
    let mut wrong = proxy(&uri, None).await;
    assert_eq!(route(&mut wrong, "remote.example", 8443).await[1], 0);
    let mut client = rustls::ClientConnection::new(
        client(Some(request.leaf())),
        rustls::pki_types::ServerName::try_from("foreign.example").unwrap(),
    )
    .unwrap();
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).unwrap();
    wrong.write_all(&bytes).await.unwrap();
    closed(&mut wrong).await;
    assert!(!request.ack(observation(&request)));
    closed(&mut original).await;
    endpoint.join().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eight_originals_do_not_starve_probes_and_retired_native_requests_keep_shared_quota() {
    let budget = EndpointBudget::new();
    let (endpoint, native) = start(
        Duration::from_secs(5),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let mut sockets = Vec::new();
    let mut requests = Vec::new();
    for _ in 0..8 {
        sockets.push(hello(&endpoint, "remote.example").await.0);
        requests.push(request(&native).await);
    }
    assert_eq!(budget.active_connections(), 8);
    assert_eq!(budget.active_trust(), 8);
    probe(&requests[0], false).await;
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_probes(), 0);
    assert_eq!(budget.active_trust(), 8);
    let (next, inbox) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let (mut socket, _) = hello(&next, "remote.example").await;
    closed(&mut socket).await;
    assert!(inbox.poll().is_none());
    drop(requests);
    assert_eq!(budget.active_trust(), 0);
    let (mut socket, _) = hello(&next, "remote.example").await;
    let fresh = request(&inbox).await;
    assert!(fresh.reject());
    closed(&mut socket).await;
    next.join().await.unwrap();
    drop(fresh);
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trust_mode_keeps_plain_http_dispatch_and_inbox_empty() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let handler: Handler = Arc::new(move |request| {
        seen.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move { Ok(reply(request)) })
    });
    let budget = EndpointBudget::new();
    let (endpoint, native) = Endpoint::start_with_native_trust(
        Handle::current(),
        config("http://remote.example:8080/doc".into(), handler),
        budget.clone(),
        TrustConfig {
            timeout: Duration::from_secs(2),
            wake: Arc::new(|| {}),
        },
    )
    .await
    .unwrap_or_else(|_| panic!("plain trust endpoint"));
    let mut socket = authenticated(&endpoint, None).await;
    assert_eq!(route(&mut socket, "remote.example", 8080).await[1], 0);
    socket
        .write_all(b"GET /doc HTTP/1.1\r\nHost: remote.example:8080\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await.unwrap();
    assert!(bytes.ends_with(b"REMOTE body"));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert!(native.poll().is_none());
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn probe_preauth_cap_is_separate_and_old_original_sni_cannot_enqueue() {
    let budget = EndpointBudget::new();
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let (mut foreign, _) = hello(&endpoint, "foreign.example").await;
    closed(&mut foreign).await;
    assert!(native.poll().is_none());
    let (mut original, _) = hello(&endpoint, "remote.example").await;
    let request = request(&native).await;
    let uri = url::Url::parse(&request.probe_proxy_uri().unwrap()).unwrap();
    let mut probes = Vec::new();
    for _ in 0..8 {
        probes.push(
            TcpStream::connect(("127.0.0.1", uri.port().unwrap()))
                .await
                .unwrap(),
        );
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while budget.active_probes() != 8 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(budget.active_connections(), 1);
    let mut ninth = TcpStream::connect(("127.0.0.1", uri.port().unwrap()))
        .await
        .unwrap();
    closed(&mut ninth).await;
    assert_eq!(budget.active_probes(), 8);
    assert!(request.reject());
    closed(&mut original).await;
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_probes(), 0);
    assert_eq!(budget.active_connections(), 0);
    drop(request);
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test]
async fn bounded_client_hello_recording_refuses_more_than_64k_without_server_bytes() {
    let (mut client, server) = tokio::io::duplex(131_072);
    // A valid-looking fragmented handshake claiming a huge body cannot grow the replay buffer.
    let mut bytes = Vec::new();
    for index in 0..5 {
        bytes.extend_from_slice(&[22, 3, 3, 0x40, 0]);
        let mut record = vec![0; 16_384];
        if index == 0 {
            record[..4].copy_from_slice(&[1, 1, 0, 0]);
        }
        bytes.extend_from_slice(&record);
    }
    client.write_all(&bytes).await.unwrap();
    let result = super::super::trust::hello::capture(
        server,
        &super::super::Authority::new("remote.example", 8443).unwrap(),
        Duration::from_millis(100),
    )
    .await;
    assert!(result.is_err());
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert!(reply.is_empty());
}

mod causes;
mod pool;
