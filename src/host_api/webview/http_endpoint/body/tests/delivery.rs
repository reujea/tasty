//! Real loopback/bridge response leases; selection is a trusted component actor, not GTK.
use super::*;
use std::collections::VecDeque;

struct Observed {
    first: NativeDelivery,
    second: NativeDelivery,
    body: Arc<VerifiedBody>,
}
#[test]
fn actual_delivery_claim_discard_replacement_and_nonmain_progress_keep_bounded_slots() {
    with_runtime(|runtime| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(deliveries(runtime));
    });
}
async fn delivered(
    endpoint: &Endpoint,
    port: u16,
    observed: &Mutex<VecDeque<Observed>>,
) -> Observed {
    let bytes = tokio::time::timeout(Duration::from_secs(5), post(endpoint, port))
        .await
        .unwrap();
    assert!(bytes.ends_with(BODY));
    observed.lock().unwrap().pop_front().unwrap()
}
fn claimed(observed: Observed) {
    let response = observed
        .first
        .claim()
        .unwrap_or_else(|e| panic!("actual response claim: {e:?}"));
    assert!(Arc::ptr_eq(&response.body, &observed.body));
    assert_eq!(response.request.method, "POST");
    assert_eq!(response.request.upload.as_slice(), UPLOAD);
    assert!(response.body.matches_native(
        response.body.head().id,
        &response.request.method,
        response.request.identity
    ));
    assert!(matches!(
        observed.second.claim(),
        Err(EndpointError::Identity)
    ));
}
async fn deliveries(runtime: NativeRuntime) {
    let counts = Arc::new(Counts::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://localhost:{port}/post");
    let (stop_upstream, rx_stop) = tokio::sync::watch::channel(false);
    let upstream = tokio::spawn(upstream(listener, counts.clone(), rx_stop));
    let (sender, rx) = channel();
    let (mut mirror, view) = mirror(&sender, &url);
    let bridge = bridge(sender, &view);
    let current = Arc::new(AtomicBool::new(true));
    let active = current.clone();
    let source = BodySource::new(
        bridge.clone(),
        view.clone(),
        Arc::new(move || active.load(Ordering::Acquire)),
    );
    let observed = Arc::new(Mutex::new(VecDeque::new()));
    let captured = observed.clone();
    let actual = source.handler();
    let handler: super::super::super::listener::Handler = Arc::new(move |request| {
        let wait = actual(request);
        let captured = captured.clone();
        Box::pin(async move {
            let response = wait.await?;
            captured.lock().unwrap().push_back(Observed {
                first: response
                    .native_delivery()
                    .expect("actual BodySource ticket"),
                second: response
                    .native_delivery()
                    .expect("duplicate capability control"),
                body: response.body().clone(),
            });
            Ok(response)
        })
    });
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
        handler,
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
    nonmain_progress(&endpoint, port, &observed, &counts).await;
    discard_controls(&endpoint, port, &observed, &source).await;
    late_failed_writer(&source, port).await;
    let retired = delivered(&endpoint, port, &observed).await;
    current.store(false, Ordering::Release);
    assert!(matches!(retired.first.claim(), Err(EndpointError::Retired)));
    assert!(!retired.second.discard());
    source.retire();
    reentrant_retirement(&bridge, &view, port).await;
    reentrant_replay(&bridge, &view);
    scope_retirement(&bridge, &view, port).await;
    let direct_budget = replace_view(&bridge, &view, &mut mirror, port).await;
    bridge.close();
    endpoint.join().await.unwrap();
    stop.store(true, Ordering::Release);
    driver.await.unwrap();
    stop_upstream.send_replace(true);
    upstream.await.unwrap();
    mirror.disconnect();
    drop(source);
    drop(bridge);
    drop(observed);
    assert_eq!(budget.active_connections(), 0);
    assert_eq!(budget.active_owners(), 0);
    assert_eq!(native.used(), 0);
    assert_eq!(direct_budget.used(), 0);
}

async fn scope_retirement(
    bridge: &Arc<HttpBridge>,
    view: &tasty_remote::html_mirror::HttpView,
    port: u16,
) {
    for retire in [false, true] {
        let source = BodySource::new(bridge.clone(), view.clone(), Arc::new(|| true));
        let (request, _budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
        let owner = (source.handler())(request)
            .await
            .unwrap_or_else(|_| panic!("response"));
        let ticket = owner.native_delivery().unwrap();
        owner.write_completed(true);
        drop(owner);
        if retire {
            source.retire();
        }
        drop(source);
        assert!(matches!(ticket.claim(), Err(EndpointError::Retired)));
    }
}

async fn nonmain_progress(
    endpoint: &Endpoint,
    port: u16,
    observed: &Mutex<VecDeque<Observed>>,
    counts: &Counts,
) {
    claimed(delivered(endpoint, port, observed).await);
    // Keep every nonmain ticket, without claiming or discarding it. Only the newest can retain
    // a slot after its write; these more-than-eight requests must all make real upstream progress.
    let mut nonmain = Vec::new();
    for _ in 0..12 {
        nonmain.push(delivered(endpoint, port, observed).await);
    }
    assert_eq!(counts.upstream.load(Ordering::Acquire), 13);
    let latest = nonmain.pop().unwrap();
    for stale in nonmain {
        assert!(matches!(stale.first.claim(), Err(EndpointError::Identity)));
        assert!(!stale.second.discard());
    }
    claimed(latest);
}
async fn discard_controls(
    endpoint: &Endpoint,
    port: u16,
    observed: &Mutex<VecDeque<Observed>>,
    source: &Arc<BodySource>,
) {
    let discarded = delivered(endpoint, port, observed).await;
    assert!(discarded.first.discard());
    assert!(matches!(
        discarded.second.claim(),
        Err(EndpointError::Identity)
    ));
    let dropped = delivered(endpoint, port, observed).await;
    drop(dropped.first);
    assert!(matches!(
        dropped.second.claim(),
        Err(EndpointError::Identity)
    ));
    // Cancellation/failed-write paths drop the real boxed owner before a successful write receipt.
    for failed in [false, true] {
        let (request, _budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
        let owner = (source.handler())(request)
            .await
            .unwrap_or_else(|_| panic!("response"));
        let ticket = owner.native_delivery().unwrap();
        if failed {
            owner.write_completed(false);
        }
        drop(owner);
        assert!(matches!(ticket.claim(), Err(EndpointError::Identity)));
    }
}
async fn replace_view(
    bridge: &Arc<HttpBridge>,
    view: &tasty_remote::html_mirror::HttpView,
    mirror: &mut HtmlMirror,
    port: u16,
) -> NativeBudget {
    let transient = BodySource::new(bridge.clone(), view.clone(), Arc::new(|| true));
    let (request, direct_budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
    let owner = (transient.handler())(request)
        .await
        .unwrap_or_else(|_| panic!("response"));
    let stale_view = owner.native_delivery().unwrap();
    owner.write_completed(true);
    drop(owner);
    let _new_view = mirror.http_source().unwrap().new_view().unwrap();
    assert!(matches!(stale_view.claim(), Err(EndpointError::Retired)));
    drop(transient);
    direct_budget
}

async fn late_failed_writer(source: &Arc<BodySource>, port: u16) {
    let (request, _old_budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
    let old = (source.handler())(request)
        .await
        .unwrap_or_else(|_| panic!("response"));
    let stale = old.native_delivery().unwrap();
    let (request, _new_budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
    let replacement = (source.handler())(request)
        .await
        .unwrap_or_else(|_| panic!("response"));
    let current = replacement.native_delivery().unwrap();
    replacement.write_completed(true);
    drop(replacement);
    old.write_completed(false);
    drop(old);
    assert!(matches!(stale.claim(), Err(EndpointError::Identity)));
    assert!(
        current.claim().is_ok(),
        "late old writer failure must not revoke its replacement"
    );
}

struct ReentrantCurrent {
    source: Mutex<std::sync::Weak<BodySource>>,
    remaining: AtomicUsize,
    blocked: AtomicBool,
    retired: AtomicBool,
}
impl ReentrantCurrent {
    fn check(&self) -> bool {
        let Some(source) = self.source.lock().unwrap().upgrade() else {
            return true;
        };
        if matches!(
            source.delivery.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ) || matches!(
            source.replay.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ) {
            self.blocked.store(true, Ordering::Release);
            return false;
        }
        if self
            .remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
            == Ok(1)
        {
            source.retire();
            self.retired.store(true, Ordering::Release);
        }
        true
    }
}
async fn reentrant_retirement(
    bridge: &Arc<HttpBridge>,
    view: &tasty_remote::html_mirror::HttpView,
    port: u16,
) {
    let (source, probe) = reentrant_source(bridge, view, 4);
    let (request, budget) = super::roundtrip::native_request(port, "POST", UPLOAD).await;
    // Fourth currentness check is publication of the real verified NativeResponse. A blocked
    // callback returns instead of hanging, so the old mutex boundary fails deterministically.
    assert!(matches!(
        (source.handler())(request).await,
        Err(EndpointError::Retired)
    ));
    assert!(
        !probe.blocked.load(Ordering::Acquire),
        "currentness callback held delivery lock"
    );
    assert!(
        probe.retired.load(Ordering::Acquire),
        "callback must actually reenter retirement"
    );
    assert!(!source.current());
    assert!(source.delivery.lock().unwrap().is_none());
    let wait_until = std::time::Instant::now() + Duration::from_secs(5);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if budget.used() == 0 {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                budget.used() == 0,
                "not reached within {:?}: budget.used() == 0",
                Duration::from_secs(5)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
}

fn reentrant_source(
    bridge: &Arc<HttpBridge>,
    view: &tasty_remote::html_mirror::HttpView,
    remaining: usize,
) -> (Arc<BodySource>, Arc<ReentrantCurrent>) {
    let probe = Arc::new(ReentrantCurrent {
        source: Mutex::new(std::sync::Weak::new()),
        remaining: AtomicUsize::new(remaining),
        blocked: AtomicBool::new(false),
        retired: AtomicBool::new(false),
    });
    let callback = probe.clone();
    let source = BodySource::new(
        bridge.clone(),
        view.clone(),
        Arc::new(move || callback.check()),
    );
    *probe.source.lock().unwrap() = Arc::downgrade(&source);
    (source, probe)
}
fn reentrant_replay(bridge: &Arc<HttpBridge>, view: &tasty_remote::html_mirror::HttpView) {
    let document = bridge.document().unwrap();
    let body = document
        .with_current(|model| model.verified(document.entry()))
        .flatten()
        .unwrap();
    let (source, probe) = reentrant_source(bridge, view, 2);
    assert!(matches!(
        source.replay_once(body.head().id),
        Err(EndpointError::Retired)
    ));
    assert!(
        !probe.blocked.load(Ordering::Acquire),
        "replay predicate held a source mutex"
    );
    assert!(probe.retired.load(Ordering::Acquire));
    assert!(
        source.replay.lock().unwrap().is_none(),
        "failed publication retained replay permission"
    );
}
