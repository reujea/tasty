//! Native codec -> ordered client -> actual host/executor -> real loopback POSTs.
use super::*;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use tasty_remote::{
    connection::{QueuedFrame, channel},
    html_http_body::BodyBudget,
    html_http_bridge::{BridgeConfig, BridgeScope, HttpBridge, NativeResponse, NativeWait},
    html_http_client::{ClientLimits, PendingBudget},
    html_http_native::{self, Destination, NativeBudget, NativeLimits},
};
use tokio::io::AsyncWriteExt;
fn config(url: &str, document: u64) -> BridgeConfig {
    let cap = 8_000_000;
    BridgeConfig {
        scope: BridgeScope {
            epoch: 1,
            surface_id: 1,
            binding: 2,
            document,
            source_generation: 1,
            entry_url: url.into(),
            allow_remote_content: false,
        },
        limits: ClientLimits {
            requests: 256,
            upload_bytes: cap,
            encoded_bytes: cap,
            decoded_bytes: cap,
            verify_jobs: 2,
        },
        pending: PendingBudget::new(cap),
        bodies: BodyBudget::new(cap),
    }
}
async fn native(url: &str, body: &[u8], cookie: &str) -> Arc<html_http_native::NativeRequest> {
    let url = reqwest::Url::parse(url).unwrap();
    let (mut browser, mut socket) = tokio::io::duplex(100_000);
    browser
        .write_all(
            format!(
                "POST {} HTTP/1.1\r\nHost: {}:{}\r\nCookie: {}\r\nContent-Length: {}\r\n\r\n",
                url.path(),
                url.host_str().unwrap(),
                url.port().unwrap(),
                cookie,
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    browser.write_all(body).await.unwrap();
    Arc::new(
        html_http_native::read_request(
            &mut socket,
            &Destination::new("http", url.host_str().unwrap(), url.port().unwrap()).unwrap(),
            NativeLimits {
                upload_bytes: 100_000,
                io_timeout: Duration::from_secs(2),
            },
            &NativeBudget::new(1_000_000),
            &|| true,
        )
        .await
        .unwrap(),
    )
}
async fn execute(
    host: &mut Host,
    bridge: &Arc<HttpBridge>,
    rx: &mpsc::Receiver<QueuedFrame>,
    mut wait: NativeWait,
    document: u64,
    sequence: &mut u64,
) -> NativeResponse {
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let mut failures = Vec::new();
    loop {
        bridge.drive(
            || {
                *sequence += 1;
                Some(TransferId {
                    epoch: 1,
                    surface_id: 1,
                    binding: 2,
                    document,
                    request: *sequence,
                    transfer: *sequence,
                })
            },
            |job| {
                job.run();
                Ok(())
            },
        );
        while let Ok(frame) = rx.try_recv() {
            host.send(serde_json::from_slice(&frame.frame.payload).unwrap());
        }
        host.tick();
        while let Ok(frame) = host.rx.try_recv() {
            let reply: HttpReply = serde_json::from_slice(&frame.payload).unwrap();
            if matches!(
                reply,
                HttpReply::Failure { .. } | HttpReply::CommandRejected { .. }
            ) {
                failures.push(describe(&reply));
            }
            bridge.receive(reply);
        }
        if let Poll::Ready(result) =
            Pin::new(&mut wait).poll(&mut Context::from_waker(Waker::noop()))
        {
            return result.unwrap_or_else(|error| {
                panic!("native wait {error:?}; host replies {failures:?}")
            });
        }
        assert!(
            std::time::Instant::now() < until,
            "actual client/executor timeout; host replies {failures:?}"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}
#[tokio::test]
async fn actual_successor_reexecutes_only_current_binary_post_and_resets_document() {
    let fixture = Fixture::navigating();
    fixture.release();
    let original = fixture.url();
    let current = format!("{}final", original);
    let mut host = Host::new(&original);
    let (sender, rx) = channel();
    let bridge = HttpBridge::new(
        config(&original, 3),
        sender.clone(),
        Arc::new(|| true),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("bridge"));
    let mut seq = 0;
    let request = native(&original, b"entry", "native=before").await;
    let response = execute(
        &mut host,
        &bridge,
        &rx,
        bridge.submit(request).unwrap(),
        3,
        &mut seq,
    )
    .await;
    drop(response);
    let request = native(&current, b"\0opaque\xffPOST", "native=before").await;
    let response = execute(
        &mut host,
        &bridge,
        &rx,
        bridge.submit(request.clone()).unwrap(),
        3,
        &mut seq,
    )
    .await;
    assert_eq!(fixture.count(), 2);
    assert_eq!(bridge.snapshot().received, 20);
    // Original first-Begin boundary: a fresh bridge without evidence still refuses current URL.
    let (legacy, legacy_rx) = channel();
    let legacy = HttpBridge::new(
        config(&original, 4),
        legacy,
        Arc::new(|| true),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("legacy"));
    let pending = legacy.submit(request.clone()).unwrap();
    legacy.drive(|| panic!("must not allocate"), Err);
    assert!(pending.await.is_err());
    assert!(legacy_rx.try_recv().is_err());
    let owner = Arc::new(());
    let receipt = bridge
        .capture_reload(&response, &owner, Arc::new(|| true))
        .unwrap();
    assert!(bridge.snapshot().received > 0);
    drop(response);
    let receipt = receipt.retire().unwrap();
    bridge.close();
    assert!(rx.try_recv().is_err());
    let next = HttpBridge::new_successor(
        config(&original, 4),
        sender.clone(),
        Arc::new(|| true),
        Arc::new(|| {}),
        receipt,
        &owner,
        true,
    )
    .unwrap_or_else(|_| panic!("successor"));
    assert_eq!(next.snapshot().received, 0);
    // The actual native reload captures updated cookies but the exact same method/upload bytes.
    let reload = native(&current, b"\0opaque\xffPOST", "native=after").await;
    let response = execute(
        &mut host,
        &next,
        &rx,
        next.submit(reload).unwrap(),
        4,
        &mut seq,
    )
    .await;
    assert_eq!(fixture.count(), 3);
    assert_eq!(response.request.logical_url, current);
    assert_eq!(&*response.request.upload, b"\0opaque\xffPOST");
    assert_eq!(next.snapshot().received, 10);
    assert_eq!(next.snapshot().skipped, 0);
    assert!(!next.snapshot().paused);
    let requests = fixture.requests();
    assert!(requests[1].0.starts_with("POST /final "));
    assert!(requests[2].0.starts_with("POST /final "));
    assert!(
        requests[2]
            .0
            .to_ascii_lowercase()
            .contains("cookie: native=after")
    );
    assert_eq!(requests[1].1, requests[2].1);
    // Old response cannot be reused after accepted successor; no additional upstream POST.
    host.send(HttpRequest::Reuse {
        response: wire::ResponseId {
            request: TransferId {
                document: 3,
                request: 2,
                transfer: 2,
                ..id()
            },
            source_generation: 1,
            serial: 1,
        },
    });
    assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
    assert_eq!(fixture.count(), 3);
    let mut active = next;
    let mut main = response;
    for document in 5..=8 {
        let receipt = active
            .capture_reload(&main, &owner, Arc::new(|| true))
            .unwrap()
            .retire()
            .unwrap();
        active.close();
        drop(main);
        drop(active);
        active = HttpBridge::new_successor(
            config(&original, document),
            sender.clone(),
            Arc::new(|| true),
            Arc::new(|| {}),
            receipt,
            &owner,
            true,
        )
        .unwrap_or_else(|_| panic!("repeated successor"));
        let request = native(&current, b"\0opaque\xffPOST", "native=after").await;
        main = execute(
            &mut host,
            &active,
            &rx,
            active.submit(request).unwrap(),
            document,
            &mut seq,
        )
        .await;
        assert_eq!(active.snapshot().received, 10);
        assert_eq!(
            host.session
                .as_ref()
                .unwrap()
                .remote
                .html_http
                .documents
                .len(),
            1
        );
    }
    assert_eq!(fixture.count(), 7);
    tracing::info!(
        "RELOAD_PROOF: 7 total POSTs = entry + current + 5 explicit Reloads; binary upload preserved; server document owners=1; every fresh document received=10 skipped=0"
    );
    active.close();
}

fn finish(host: &mut Host, response: wire::ResponseId) {
    host.send(HttpRequest::Credit {
        response,
        offset: 0,
        max_bytes: 16,
        allow_large: false,
    });
    assert!(matches!(host.receive(), HttpReply::Chunk { .. }));
    host.send(HttpRequest::Credit {
        response,
        offset: 10,
        max_bytes: 1,
        allow_large: false,
    });
    assert!(matches!(host.receive(), HttpReply::Complete { .. }));
}
#[test]
fn malformed_successors_preserve_live_response_and_do_not_claim_shared_cursor() {
    let fixture = Fixture::new();
    fixture.release();
    let url = fixture.url();
    let mut host = Host::new(&url);
    let response = host.commit(&url, b"owned");
    finish(&mut host, response);
    for mode in 0..5 {
        let mut begin = host.begin(&url, b"owned");
        if let HttpRequest::Begin {
            id,
            method,
            logical_url,
            upload,
            ..
        } = &mut begin
        {
            id.document += 1;
            id.request += 1;
            id.transfer += 1;
            match mode {
                0 => *method = "GET".into(),
                1 => *logical_url = format!("{url}forged"),
                2 => *upload = identity(b"changed"),
                _ => {}
            }
        }
        let mut previous = response;
        if mode == 3 {
            previous.serial += 1;
        }
        if mode == 4 {
            host.send(HttpRequest::Release { response });
        }
        host.send(begin.with_predecessor(previous).unwrap());
        assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
        assert!(
            host.session
                .as_ref()
                .unwrap()
                .remote
                .html_http
                .documents
                .contains_key(&key(41, id()))
        );
        assert_eq!(fixture.count(), 1);
    }
    // Every rejected candidate left the next shared request/transfer values available.
    let mut next = id();
    next.request += 1;
    next.transfer += 1;
    assert!(
        host.hub
            .admit_html_request(41, &host.hub.client_binding(41).unwrap(), next)
    );
}
#[test]
fn successor_does_not_cross_source_grant_or_registration_retirement() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        fixture.release();
        let url = fixture.url();
        let mut host = Host::new(&url);
        let response = host.commit(&url, b"owned");
        finish(&mut host, response);
        if mode == 0 {
            host.session
                .as_ref()
                .unwrap()
                .runtime
                .surfaces
                .get(&1)
                .unwrap()
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                .unwrap()
                .set_webview_url(Some(format!("{url}changed")), None, true);
        }
        if mode == 1 {
            host.session
                .as_mut()
                .unwrap()
                .live
                .occupancy
                .force_detach_workspace(7);
        }
        if mode == 2 {
            host.rx = host.hub.register(41);
            let (tx, rx) = mpsc::channel();
            tx.send(StreamInbound::Frame {
                client_id: 41,
                frame: StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&HtmlRequest::HtmlCapabilities {
                        capabilities: vec![
                            CAPABILITY.into(),
                            VIEWER_CAPABILITY.into(),
                            wire::CAPABILITY.into(),
                            wire::RELOAD_CAPABILITY.into(),
                        ],
                    })
                    .unwrap(),
                ),
            })
            .unwrap();
            host.hub.pump_inbound(&rx);
        }
        let mut begin = host.begin(&url, b"owned");
        if let HttpRequest::Begin { id, .. } = &mut begin {
            id.document += 1;
            id.request += 1;
            id.transfer += 1;
        }
        host.send(begin.with_predecessor(response).unwrap());
        assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
        assert_eq!(fixture.count(), 1);
    }
}

#[test]
fn incomplete_and_cancelled_predecessors_never_authorize_reload() {
    let fixture = Fixture::new();
    let url = fixture.url();
    let mut host = Host::new(&url);
    let previous = host.commit(&url, b"owned");
    let mut begin = host.begin(&url, b"owned");
    if let HttpRequest::Begin { id, .. } = &mut begin {
        id.document += 1;
        id.request += 1;
        id.transfer += 1;
    }
    let request = begin.with_predecessor(previous).unwrap();
    host.send(request.clone());
    assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
    host.send(HttpRequest::Cancel {
        id: id(),
        source_generation: 1,
    });
    assert!(matches!(host.receive(), HttpReply::Cancelled { .. }));
    host.send(request);
    assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
    assert_eq!(fixture.count(), 1);
    fixture.release();
}
#[test]
fn old_peer_cannot_receive_successor_permission_from_ordinary_http_capability() {
    let fixture = Fixture::new();
    fixture.release();
    let url = fixture.url();
    let mut host = Host::new(&url);
    let previous = host.commit(&url, b"owned");
    finish(&mut host, previous);
    let (tx, rx) = mpsc::channel();
    tx.send(StreamInbound::Frame {
        client_id: 41,
        frame: StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&HtmlRequest::HtmlCapabilities {
                capabilities: vec![
                    CAPABILITY.into(),
                    VIEWER_CAPABILITY.into(),
                    wire::CAPABILITY.into(),
                ],
            })
            .unwrap(),
        ),
    })
    .unwrap();
    host.hub.pump_inbound(&rx);
    let mut begin = host.begin(&url, b"owned");
    if let HttpRequest::Begin { id, .. } = &mut begin {
        id.document += 1;
        id.request += 1;
        id.transfer += 1;
    }
    host.send(begin.with_predecessor(previous).unwrap());
    assert!(matches!(
        host.receive(),
        HttpReply::CommandRejected {
            code: wire::FailureCode::UnsupportedScheme,
            ..
        }
    ));
    assert_eq!(fixture.count(), 1);
}
