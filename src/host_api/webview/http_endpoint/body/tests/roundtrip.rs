use super::*;
#[test]
fn actual_endpoint_model_server_backend_retains_one_upstream_post_through_explicit_body_replay() {
    with_runtime(|runtime| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(roundtrip(runtime));
    });
}
async fn roundtrip(runtime: NativeRuntime) {
    let counts = Arc::new(Counts::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://localhost:{port}/post");
    let (stop_upstream, rx_stop) = tokio::sync::watch::channel(false);
    let upstream = tokio::spawn(upstream(listener, counts.clone(), rx_stop));
    let (sender, rx) = channel();
    let (mut mirror, view) = mirror(&sender, &url);
    let bridge = bridge(sender, &view);
    let source = BodySource::new(bridge.clone(), view.clone(), Arc::new(|| true));
    let budget = runtime.endpoint_budget();
    let native = NativeBudget::new(4_000_000);
    let current_view = view.clone();
    let config = EndpointConfig {
        transport: crate::host_api::webview::http_endpoint::listener::Transport::Socks,
        port: 0,
        loopback_alias: false,
        identity: DocumentIdentity {
            epoch: 1,
            surface: 42,
            binding: 7,
            document: 1,
            source_generation: 9,
        },
        entry_url: url.clone(),
        allow_remote_content: false,
        https: false,
        current: Arc::new(move || current_view.is_current()),
        limits: NativeLimits {
            upload_bytes: 1_000_000,
            io_timeout: Duration::from_secs(2),
        },
        native_budget: native.clone(),
        handler: source.handler(),
    };
    let handle = runtime.handle();
    let endpoint = handle
        .spawn(Endpoint::start(handle.clone(), config, budget.clone()))
        .await
        .unwrap()
        .unwrap_or_else(|_| panic!("endpoint"));
    let stop = Arc::new(AtomicBool::new(false));
    let driver = tokio::spawn(driver(
        bridge.clone(),
        rx,
        runtime,
        counts.clone(),
        url,
        stop.clone(),
    ));
    let first = initial_bytes(&endpoint, port).await;
    let body = arm_replay(&source, &bridge, &view, &counts);
    confirm_reuse(&endpoint, port, &bridge, &counts, &body, &first).await;
    refuse_duplicate(&endpoint, port, &source, &counts, &body).await;
    refuse_identity(&bridge, &view, &body, port).await;
    assert_eq!(counts.upstream.load(Ordering::Acquire), 1);
    assert_eq!(counts.begins.load(Ordering::Acquire), 1);
    assert_eq!(counts.reuses.load(Ordering::Acquire), 1);
    source.retire();
    assert!(!source.finish_replay(&body));
    assert_eq!(
        source.replay_once(body.head().id),
        Err(EndpointError::Retired)
    );
    bridge.close();
    let wait_until = std::time::Instant::now() + Duration::from_secs(2);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if counts.closes.load(Ordering::Acquire) == 1 {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                counts.closes.load(Ordering::Acquire) == 1,
                "not reached within {:?}: counts.closes.load(Ordering::Acquire) == 1",
                Duration::from_secs(2)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
    stop.store(true, Ordering::Release);
    driver.await.unwrap();
    stop_upstream.send_replace(true);
    upstream.await.unwrap();
    mirror.disconnect();
    drop(bridge);
    assert_eq!(native.used(), 0);
}

async fn initial_bytes(endpoint: &Endpoint, port: u16) -> Vec<u8> {
    let first = tokio::time::timeout(Duration::from_secs(5), post(endpoint, port))
        .await
        .unwrap();
    assert!(first.starts_with(b"HTTP/1.1 200 \r\n"));
    assert!(first.ends_with(BODY));
    assert_eq!(
        first
            .windows(12)
            .filter(|s| s.eq_ignore_ascii_case(b"set-cookie: "))
            .count(),
        2
    );
    assert!(
        !first
            .windows(24)
            .any(|s| s.eq_ignore_ascii_case(b"content-security-policy:"))
    );
    first
}
fn arm_replay(
    source: &BodySource,
    bridge: &HttpBridge,
    view: &tasty_remote::html_mirror::HttpView,
    counts: &Counts,
) -> Arc<VerifiedBody> {
    let document = bridge.document().unwrap();
    let body = document
        .with_current(|model| model.verified(document.entry()))
        .flatten()
        .unwrap();
    assert_eq!(body.encoded(), BODY);
    assert_eq!(body.head().logical_url, view.source().entry_url());
    assert_eq!(counts.upstream.load(Ordering::Acquire), 1);
    let mut wrong = body.head().id;
    wrong.serial += 1;
    assert_eq!(source.replay_once(wrong), Err(EndpointError::Identity));
    source.replay_once(body.head().id).unwrap();
    assert!(!source.finish_replay(&body));
    body
}
async fn confirm_reuse(
    endpoint: &Endpoint,
    port: u16,
    bridge: &HttpBridge,
    counts: &Counts,
    body: &Arc<VerifiedBody>,
    first: &[u8],
) {
    let document = bridge.document().unwrap();
    let second = tokio::time::timeout(Duration::from_secs(5), post(endpoint, port))
        .await
        .unwrap();
    assert_eq!(counts.upstream.load(Ordering::Acquire), 1);
    assert_eq!(second, first);
    assert_eq!(counts.begins.load(Ordering::Acquire), 1);
    assert_eq!(counts.commits.load(Ordering::Acquire), 1);
    assert_eq!(counts.reuses.load(Ordering::Acquire), 1);
    let reused = document
        .with_current(|model| model.verified(document.entry()))
        .flatten()
        .unwrap();
    assert!(Arc::ptr_eq(body, &reused));
    assert_eq!(
        counts.transferred.load(Ordering::Acquire),
        BODY.len() as u64
    );
}
async fn refuse_duplicate(
    endpoint: &Endpoint,
    port: u16,
    source: &BodySource,
    counts: &Counts,
    body: &Arc<VerifiedBody>,
) {
    let duplicate = tokio::time::timeout(Duration::from_secs(5), post(endpoint, port))
        .await
        .unwrap();
    assert!(duplicate.is_empty());
    assert_eq!(counts.upstream.load(Ordering::Acquire), 1);
    assert_eq!(counts.begins.load(Ordering::Acquire), 1);
    let different_owner = copied_body(body);
    assert!(!Arc::ptr_eq(body, &different_owner));
    assert!(!source.finish_replay(&different_owner));
    assert!(source.finish_replay(body));
    assert!(!source.finish_replay(body));
}
async fn refuse_identity(
    bridge: &Arc<HttpBridge>,
    view: &tasty_remote::html_mirror::HttpView,
    body: &Arc<VerifiedBody>,
    port: u16,
) {
    for (method, upload) in [("GET", UPLOAD), ("POST", b"bad".as_slice())] {
        let protected = BodySource::new(bridge.clone(), view.clone(), Arc::new(|| true));
        protected.replay_once(body.head().id).unwrap();
        let handler = protected.handler();
        for (method, upload) in [(method, upload), ("POST", UPLOAD)] {
            let (request, request_budget) = native_request(port, method, upload).await;
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(2), handler(request))
                    .await
                    .unwrap(),
                Err(EndpointError::Identity)
            ));
            assert_eq!(request_budget.used(), 0);
        }
        assert!(!protected.finish_replay(body));
        protected.retire();
    }
}

fn copied_body(body: &Arc<VerifiedBody>) -> Arc<VerifiedBody> {
    use sha2::{Digest, Sha256};
    use tasty_ipc::html_http::BodyIdentity;
    let head = body.head().clone();
    let begin = HttpRequest::Begin {
        id: head.id.request,
        source_generation: head.id.source_generation,
        method: "POST".into(),
        logical_url: head.logical_url.clone(),
        headers: vec![],
        upload: BodyIdentity {
            bytes: UPLOAD.len() as u64,
            sha256: Sha256::digest(UPLOAD).into(),
        },
        allow_remote_content: false,
    };
    tasty_remote::html_http_body::verify(
        &begin,
        head,
        Arc::new(body.encoded().to_vec()),
        (
            BodyIdentity {
                bytes: body.encoded().len() as u64,
                sha256: Sha256::digest(body.encoded()).into(),
            },
            body.decoded_identity(),
        ),
        &BodyBudget::new(1_000_000),
        1_000_000,
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|_| panic!("same representation, different verified owner"))
}

pub(super) async fn native_request(
    port: u16,
    method: &str,
    upload: &[u8],
) -> (Arc<NativeRequest>, NativeBudget) {
    let (mut client, mut server) = tokio::io::duplex(1000);
    client
        .write_all(
            format!(
                "{method} /post HTTP/1.1\r\nHost: localhost:{port}\r\nCookie: native=kept\r\nContent-Length: {}\r\n\r\n",
                upload.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    client.write_all(upload).await.unwrap();
    let budget = NativeBudget::new(1_000_000);
    let request = html_http_native::read_request(
        &mut server,
        &Destination::new("http", "localhost", port).unwrap(),
        NativeLimits {
            upload_bytes: 1000,
            io_timeout: Duration::from_secs(1),
        },
        &budget,
        &|| true,
    )
    .await
    .unwrap();
    (Arc::new(request), budget)
}
