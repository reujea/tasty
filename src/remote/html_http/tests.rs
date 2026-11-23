mod fixture;
use super::*;
use fixture::Server;
use std::time::{Duration, Instant};
use tasty_ipc::html_transfer::CHUNK_BYTES;
fn limits() -> Limits {
    Limits {
        workers: 2,
        requests_per_document: 16,
        retained_bytes: 8_000_000,
        upload_bytes: 1_000_000,
        encoded_bytes: 1_000_000,
        decoded_bytes: 1_000_000,
        header_bytes: 32_000,
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(3),
        network_time: Duration::from_secs(10),
    }
}
fn backend() -> Backend {
    Backend::new(limits(), tokio::runtime::Handle::current()).unwrap()
}
fn key(document: u64) -> DocumentKey {
    DocumentKey {
        epoch: 1,
        surface_id: 2,
        binding: 3,
        document,
        source_generation: 4,
    }
}
fn id(request: u64, document: u64) -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 2,
        binding: 3,
        document,
        request,
        transfer: request,
    }
}
fn request(b: &Backend, s: &Server, path: &str, n: u64, body: &[u8]) -> Request {
    let mut upload = b.upload();
    let mut offset = 0;
    for c in body.chunks(CHUNK_BYTES) {
        upload.append(offset, c).unwrap();
        offset += c.len() as u64;
    }
    Request {
        id: id(n, 1),
        source_generation: 4,
        method: "POST".into(),
        url: s.url(path),
        headers: vec![],
        upload: upload.finish(),
    }
}
async fn collect(mut op: Operation) -> Result<Arc<StoredResponse>> {
    let mut offset = 0;
    let mut identity = None;
    while let Some(event) = op.events.recv().await {
        match event {
            Event::Head(h) => {
                identity = Some(h.id);
                if h.has_body && h.encoded_length.is_none_or(|n| n <= 100000) {
                    op.credit(Credit {
                        response: h.id,
                        offset,
                        max_bytes: CHUNK_BYTES,
                        allow_large: false,
                    })?;
                }
            }
            Event::Chunk {
                id,
                offset: at,
                bytes,
            } => {
                assert_eq!(at, offset);
                assert!(!bytes.is_empty() && bytes.len() <= CHUNK_BYTES);
                offset += bytes.len() as u64;
                op.credit(Credit {
                    response: id,
                    offset,
                    max_bytes: CHUNK_BYTES,
                    allow_large: offset >= 100_000,
                })?;
            }
            Event::AllowanceRequired { id, .. } => {
                op.credit(Credit {
                    response: id,
                    offset,
                    max_bytes: CHUNK_BYTES,
                    allow_large: true,
                })?;
            }
            Event::Complete(r) => {
                assert_eq!(Some(r.head.id), identity);
                let joined = op.join().await?;
                assert!(Arc::ptr_eq(&r, &joined));
                return Ok(r);
            }
            Event::Failed { error, .. } => {
                assert_eq!(op.join().await.unwrap_err(), error);
                return Err(error);
            }
        }
    }
    op.join().await
}
fn fresh(opened: Opened) -> Operation {
    match opened {
        Opened::Fresh(op) => op,
        Opened::Reused(_) => panic!("fresh operation expected"),
    }
}
#[tokio::test]
async fn binary_multiframe_post_status_headers_and_response_reuse() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let bytes = vec![173; 150_000];
    let r = collect(fresh(d.open(request(&b, &s, "/echo", 1, &bytes)).unwrap()))
        .await
        .unwrap();
    assert_eq!(r.encoded.bytes(), bytes);
    assert_eq!(r.encoded.identity(), r.decoded.identity());
    assert_eq!(s.count(), 1);
    assert!(Arc::ptr_eq(&r, &d.reuse(r.head.id).unwrap()));
    match d.open(request(&b, &s, "/echo", 1, &bytes)).unwrap() {
        Opened::Reused(same) => assert!(Arc::ptr_eq(&same, &r)),
        _ => panic!("POST repeated"),
    };
    assert_eq!(s.count(), 1);
    assert!(matches!(
        d.open(request(&b, &s, "/echo", 1, b"changed")),
        Err(Error::DuplicateMismatch)
    ));
    let status = collect(fresh(d.open(request(&b, &s, "/status", 2, b"")).unwrap()))
        .await
        .unwrap();
    assert_eq!(status.head.status, 418);
    assert_eq!(
        status
            .head
            .headers
            .iter()
            .filter(|h| h.name == "set-cookie")
            .count(),
        2
    );
    assert!(
        status
            .head
            .headers
            .iter()
            .any(|h| h.name == "content-security-policy")
    );
    assert_eq!(
        status
            .head
            .headers
            .iter()
            .filter(|h| h.name == "set-cookie")
            .map(|h| h.value.as_slice())
            .collect::<Vec<_>>(),
        vec![b"first=1".as_slice(), b"second=2".as_slice()]
    );
    let seen = s.seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].body.len(), 150_000);
    assert_eq!(seen[0].path, "/echo");
    assert!(
        !seen[0]
            .headers
            .to_ascii_lowercase()
            .contains("proxy-authorization")
    );
}
#[tokio::test]
async fn redirect_returns_once_and_failed_post_is_not_retried() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let r = collect(fresh(
        d.open(request(&b, &s, "/redirect", 1, b"post")).unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(r.head.status, 302);
    assert_eq!(s.count(), 1);
    assert!(
        r.head
            .headers
            .iter()
            .any(|h| h.name == "location" && h.value == b"/target")
    );
    assert_eq!(
        collect(fresh(
            d.open(request(&b, &s, "/drop", 2, b"only once")).unwrap()
        ))
        .await
        .unwrap_err(),
        Error::Network
    );
    assert_eq!(s.count(), 2);
    assert!(matches!(
        d.open(request(&b, &s, "/drop", 2, b"only once")),
        Err(Error::AlreadyUsed)
    ));
    assert_eq!(s.count(), 2);
}
#[tokio::test]
async fn unknown_100000_and_100001_have_distinct_allowance_boundary() {
    for n in [100000, 100001] {
        let s = Server::new();
        let b = backend();
        let d = b.document(key(1), &s.url("/"), false).unwrap();
        let mut op = fresh(
            d.open(request(&b, &s, &format!("/chunk/{n}"), 1, b""))
                .unwrap(),
        );
        let mut offset = 0;
        let mut paused = 0;
        while let Some(e) = op.events.recv().await {
            match e {
                Event::Head(h) => op
                    .credit(Credit {
                        response: h.id,
                        offset: 0,
                        max_bytes: CHUNK_BYTES,
                        allow_large: false,
                    })
                    .unwrap(),
                Event::Chunk {
                    id,
                    offset: at,
                    bytes,
                } => {
                    assert_eq!(at, offset);
                    assert!(offset + bytes.len() as u64 <= 100000 || paused == 1);
                    offset += bytes.len() as u64;
                    op.credit(Credit {
                        response: id,
                        offset,
                        max_bytes: CHUNK_BYTES,
                        allow_large: paused == 1,
                    })
                    .unwrap()
                }
                Event::AllowanceRequired {
                    id,
                    encoded_at_least,
                    known_length,
                } => {
                    assert_eq!(offset, 100000);
                    assert_eq!(encoded_at_least, 100001);
                    assert_eq!(known_length, None);
                    paused += 1;
                    op.credit(Credit {
                        response: id,
                        offset,
                        max_bytes: CHUNK_BYTES,
                        allow_large: true,
                    })
                    .unwrap()
                }
                Event::Complete(r) => {
                    assert_eq!(r.encoded.bytes().len(), n);
                    break;
                }
                e => panic!("{e:?}"),
            }
        }
        assert_eq!(paused, usize::from(n > 100000));
        assert_eq!(op.join().await.unwrap().encoded.bytes().len(), n);
    }
}
#[tokio::test]
async fn encoded_and_decoded_quotas_are_separate() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let r = collect(fresh(d.open(request(&b, &s, "/gzip", 1, b"")).unwrap()))
        .await
        .unwrap();
    assert!(r.encoded.bytes().len() < 100000);
    assert_eq!(r.decoded.bytes().len(), 200000);
    assert_ne!(r.encoded.identity().sha256, r.decoded.identity().sha256);
    let mut l = limits();
    l.decoded_bytes = 199999;
    let limited = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let doc = limited.document(key(1), &s.url("/"), false).unwrap();
    assert_eq!(
        collect(fresh(
            doc.open(request(&limited, &s, "/gzip", 1, b"")).unwrap()
        ))
        .await
        .unwrap_err(),
        Error::Quota("decoded bytes")
    );
}
#[tokio::test]
async fn cancel_during_blocked_body_read_joins_and_releases_worker() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(d.open(request(&b, &s, "/slow", 1, b"")).unwrap());
    let mut id = None;
    while let Some(e) = op.events.recv().await {
        match e {
            Event::Head(h) => {
                id = Some(h.id);
                op.credit(Credit {
                    response: h.id,
                    offset: 0,
                    max_bytes: 5,
                    allow_large: false,
                })
                .unwrap()
            }
            Event::Chunk { bytes, .. } => {
                assert_eq!(bytes, b"hello");
                break;
            }
            e => panic!("{e:?}"),
        }
    }
    op.credit(Credit {
        response: id.unwrap(),
        offset: 5,
        max_bytes: 5,
        allow_large: false,
    })
    .unwrap();
    let start = Instant::now();
    op.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), op.join())
            .await
            .unwrap()
            .unwrap_err(),
        Error::Cancelled
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(b.available_workers(), 2);
}
#[tokio::test]
async fn policy_stale_credit_and_upload_offsets_fail_closed() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut r = request(&b, &s, "/echo", 1, b"");
    r.url = "wss://127.0.0.1:1".into();
    assert!(matches!(d.open(r), Err(Error::UnsupportedScheme)));
    let mut r = request(&b, &s, "/echo", 1, b"");
    r.url = "http://127.0.0.1:1".into();
    assert!(matches!(
        d.open(r),
        Err(Error::Invalid("cross-origin policy"))
    ));
    let mut r = request(&b, &s, "/echo", 1, b"");
    r.id.document = 2;
    assert!(matches!(d.open(r), Err(Error::Stale)));
    assert_eq!(s.count(), 0);
    let mut upload = b.upload();
    assert!(upload.append(1, b"bad").is_err());
    assert!(upload.append(0, &vec![0; CHUNK_BYTES + 1]).is_err());
    assert_eq!(b.retained_payload_bytes(), 0);
    let mut op = fresh(d.open(request(&b, &s, "/echo", 1, b"value")).unwrap());
    let Event::Head(h) = op.events.recv().await.unwrap() else {
        panic!()
    };
    op.credit(Credit {
        response: h.id,
        offset: 9,
        max_bytes: 4,
        allow_large: false,
    })
    .unwrap();
    assert!(matches!(
        op.events.recv().await,
        Some(Event::Failed {
            error: Error::Invalid(_),
            ..
        })
    ));
    assert!(matches!(op.join().await, Err(Error::Invalid(_))));
}
#[test]
fn accept_encoding_offers_only_decodable_codings() {
    let mut h = vec![
        Header {
            name: "Accept-Encoding".into(),
            value: b"gzip, deflate, br, zstd".to_vec(),
        },
        Header {
            name: "accept".into(),
            value: b"text/html".to_vec(),
        },
    ];
    headers::limit_accept_encoding(&mut h);
    assert_eq!(
        h,
        vec![
            Header {
                name: "accept".into(),
                value: b"text/html".to_vec(),
            },
            Header {
                name: "accept-encoding".into(),
                value: b"gzip, deflate".to_vec(),
            },
        ]
    );
    let mut plain = vec![h[0].clone()];
    headers::limit_accept_encoding(&mut plain);
    assert_eq!(plain, vec![h[0].clone()]);
}
#[test]
fn connection_nominated_headers_and_proxy_credentials_never_cross() {
    let h = vec![
        Header {
            name: "connection".into(),
            value: b"X-Private".to_vec(),
        },
        Header {
            name: "x-private".into(),
            value: b"remove".to_vec(),
        },
        Header {
            name: "proxy-authorization".into(),
            value: b"secret".to_vec(),
        },
        Header {
            name: "origin".into(),
            value: b"http://original".to_vec(),
        },
    ];
    let clean = headers::clean(&h, 1000, true).unwrap();
    assert_eq!(clean, vec![h[3].clone()]);
    assert!(
        headers::clean(
            &[Header {
                name: "connection".into(),
                value: b"Content-Security-Policy".to_vec()
            }],
            1000,
            false
        )
        .is_err()
    );
}

#[tokio::test]
async fn known_large_waits_for_allow_and_decline_never_replays_post() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(
        d.open(request(&b, &s, "/echo", 1, &vec![42; 300000]))
            .unwrap(),
    );
    let Event::Head(h) = op.events.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(h.encoded_length, Some(300000));
    assert!(matches!(
        op.events.recv().await,
        Some(Event::AllowanceRequired {
            known_length: Some(300000),
            ..
        })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), op.events.recv())
            .await
            .is_err()
    );
    assert_eq!(s.count(), 1);
    op.cancel();
    assert_eq!(op.join().await.unwrap_err(), Error::Cancelled);
    assert!(matches!(
        d.open(request(&b, &s, "/echo", 1, &vec![42; 300000])),
        Err(Error::AlreadyUsed)
    ));
    let r = collect(fresh(
        d.open(request(&b, &s, "/echo", 2, &vec![1; 300000]))
            .unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(r.encoded.bytes().len(), 300000);
    assert_eq!(s.count(), 2);
}

#[tokio::test]
async fn registry_worker_and_payload_quotas_release_with_owned_handles() {
    let s = Server::new();
    let mut l = limits();
    l.workers = 1;
    l.requests_per_document = 1;
    let b = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(d.open(request(&b, &s, "/slow", 1, b"")).unwrap());
    let Event::Head(_) = op.events.recv().await.unwrap() else {
        panic!()
    };
    assert!(matches!(
        d.open(request(&b, &s, "/echo", 2, b"")),
        Err(Error::Capacity)
    ));
    drop(d);
    assert_eq!(op.join().await.unwrap_err(), Error::Cancelled);
    assert_eq!(b.available_workers(), 1);
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let r = collect(fresh(
        d.open(request(&b, &s, "/echo", 1, b"payload")).unwrap(),
    ))
    .await
    .unwrap();
    assert_eq!(b.retained_payload_bytes(), 7);
    d.release(r.head.id).unwrap();
    assert!(matches!(d.reuse(r.head.id), Err(Error::Stale)));
    assert_eq!(b.retained_payload_bytes(), 7);
    assert!(matches!(
        d.open(request(&b, &s, "/echo", 1, b"payload")),
        Err(Error::AlreadyUsed)
    ));
    drop(r);
    assert_eq!(b.retained_payload_bytes(), 0);
    let mut l = limits();
    l.retained_bytes = 4;
    l.upload_bytes = 4;
    l.encoded_bytes = 4;
    l.decoded_bytes = 4;
    let limited = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let mut upload = limited.upload();
    upload.append(0, b"1234").unwrap();
    assert_eq!(upload.append(4, b"5"), Err(Error::Quota("upload bytes")));
    drop(upload);
    assert_eq!(limited.retained_payload_bytes(), 0);
}

#[tokio::test]
async fn head_origin_cookie_and_foreign_upload_identity_are_preserved() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut req = request(&b, &s, "/status", 1, b"");
    req.method = "HEAD".into();
    req.headers = vec![
        Header {
            name: "origin".into(),
            value: b"http://logical.invalid".to_vec(),
        },
        Header {
            name: "cookie".into(),
            value: b"session=fixture".to_vec(),
        },
    ];
    let r = collect(fresh(d.open(req).unwrap())).await.unwrap();
    assert_eq!(r.head.status, 418);
    assert!(!r.head.has_body);
    assert!(r.encoded.bytes().is_empty());
    let seen = s.seen.lock().unwrap();
    let h = seen[0].headers.to_ascii_lowercase();
    assert!(h.contains("origin: http://logical.invalid"));
    assert!(h.contains("cookie: session=fixture"));
    drop(seen);
    let foreign = backend();
    assert!(matches!(
        d.open(request(&foreign, &s, "/echo", 2, b"")),
        Err(Error::Invalid("upload backend ownership"))
    ));
    assert_eq!(s.count(), 1);
}

#[tokio::test]
async fn streams_and_stale_response_identity_fail_without_fallback() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    assert_eq!(
        collect(fresh(d.open(request(&b, &s, "/sse", 1, b"")).unwrap()))
            .await
            .unwrap_err(),
        Error::UnsupportedStreaming
    );
    let mut op = fresh(d.open(request(&b, &s, "/echo", 2, b"x")).unwrap());
    let Event::Head(h) = op.events.recv().await.unwrap() else {
        panic!()
    };
    let mut wrong = h.id;
    wrong.source_generation += 1;
    op.credit(Credit {
        response: wrong,
        offset: 0,
        max_bytes: 1,
        allow_large: false,
    })
    .unwrap();
    assert!(matches!(
        op.events.recv().await,
        Some(Event::Failed {
            error: Error::Invalid(_),
            ..
        })
    ));
    assert!(matches!(op.join().await, Err(Error::Invalid(_))));
    finite_network_timeout(&s).await;
    assert_eq!(s.count(), 3);
}

#[tokio::test]
async fn cancelled_join_future_cancels_instead_of_detaching_worker() {
    let s = Server::new();
    let b = backend();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(d.open(request(&b, &s, "/slow", 1, b"")).unwrap());
    assert!(matches!(op.events.recv().await, Some(Event::Head(_))));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), op.join())
            .await
            .is_err()
    );
    let wait_until = std::time::Instant::now() + Duration::from_secs(1);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if b.available_workers() == 2 {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                b.available_workers() == 2,
                "not reached within {:?}: b.available_workers() == 2",
                Duration::from_secs(1)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn consent_pause_is_not_network_timeout() {
    let s = Server::new();
    let configured = limits();
    let outside_both = configured.network_time + configured.read_timeout + Duration::from_secs(1);
    let b = Backend::new(configured, tokio::runtime::Handle::current()).unwrap();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(d.open(request(&b, &s, "/status", 1, b"")).unwrap());
    let Event::Head(h) = op.events.recv().await.unwrap() else {
        panic!()
    };
    // The actual Head is established under the common fixture budgets. Keep the
    // worker runnable while REAL elapsed consent time exceeds BOTH configured
    // deadlines; this also covers the production std::Instant accounting clock.
    let paused_at = Instant::now();
    tokio::time::sleep(outside_both).await;
    assert!(paused_at.elapsed() >= outside_both);
    assert!(matches!(
        op.events.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    op.credit(Credit {
        response: h.id,
        offset: 0,
        max_bytes: CHUNK_BYTES,
        allow_large: false,
    })
    .unwrap();
    let Event::Chunk {
        id,
        offset: 0,
        bytes,
    } = op.events.recv().await.unwrap()
    else {
        panic!()
    };
    assert_eq!(bytes, b"status-body");
    op.credit(Credit {
        response: id,
        offset: bytes.len() as u64,
        max_bytes: CHUNK_BYTES,
        allow_large: false,
    })
    .unwrap();
    assert!(matches!(op.events.recv().await, Some(Event::Complete(_))));
    assert!(op.join().await.is_ok());
}

#[tokio::test]
async fn encoded_and_shared_payload_failures_reclaim_and_tombstone() {
    let s = Server::new();
    let mut l = limits();
    l.encoded_bytes = 4;
    let b = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    assert_eq!(
        collect(fresh(d.open(request(&b, &s, "/chunk/5", 1, b"")).unwrap()))
            .await
            .unwrap_err(),
        Error::Quota("encoded bytes")
    );
    assert_eq!(b.retained_payload_bytes(), 0);
    assert!(matches!(
        d.open(request(&b, &s, "/chunk/5", 1, b"")),
        Err(Error::AlreadyUsed)
    ));
    let mut l = limits();
    l.retained_bytes = 50;
    l.upload_bytes = 50;
    l.encoded_bytes = 50;
    l.decoded_bytes = 50;
    let b = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let d = b.document(key(1), &s.url("/"), false).unwrap();
    assert_eq!(
        collect(fresh(
            d.open(request(&b, &s, "/echo", 1, &[1; 40])).unwrap()
        ))
        .await
        .unwrap_err(),
        Error::Quota("retained bytes")
    );
    assert_eq!(b.retained_payload_bytes(), 0);
    let mut req = request(&b, &s, "/echo", 2, b"");
    req.method = "CONNECT".into();
    assert!(matches!(d.open(req), Err(Error::UnsupportedUpgrade)));
    let mut req = request(&b, &s, "/echo", 2, b"");
    req.headers = vec![Header {
        name: "upgrade".into(),
        value: b"websocket".to_vec(),
    }];
    assert!(matches!(d.open(req), Err(Error::UnsupportedUpgrade)));
    assert_eq!(s.count(), 2);
}

#[tokio::test]
async fn https_origin_policy_rejects_scheme_and_port_changes_before_network() {
    let b = backend();
    let d = b
        .document(key(1), "https://example.invalid/", false)
        .unwrap();
    for url in ["http://example.invalid/", "https://example.invalid:444/"] {
        let r = Request {
            id: id(1, 1),
            source_generation: 4,
            method: "GET".into(),
            url: url.into(),
            headers: vec![],
            upload: b.upload().finish(),
        };
        assert!(matches!(
            d.open(r),
            Err(Error::Invalid("cross-origin policy"))
        ));
    }
    let r = Request {
        id: id(1, 1),
        source_generation: 4,
        method: "GET".into(),
        url: "https://user:secret@example.invalid/".into(),
        headers: vec![],
        upload: b.upload().finish(),
    };
    assert!(matches!(
        d.open(r),
        Err(Error::Invalid(
            "URL userinfo; use native Authorization header"
        ))
    ));
}

// Current-thread runtime: once the receiver consumes this credit, its poll reaches
// the held EOF read before this task runs again. A ready yield prevents idle clock
// auto-advance; a broken credit consumer fails a finite check rather than hanging.
async fn await_credit_consumed(op: &Operation) {
    for _ in 0..10000 {
        if op.credits.capacity() == op.credits.max_capacity() {
            return;
        }
        tokio::task::yield_now().await;
    }
    // 마지막 yield 뒤에 한 번 더 본다. 횟수 한도는 가상 시계 자동 전진을 막는 장치라 그대로 둔다.
    assert_eq!(
        op.credits.capacity(),
        op.credits.max_capacity(),
        "exact EOF credit was not consumed"
    );
}

async fn finite_network_timeout(s: &Server) {
    let mut l = limits();
    let network_budget = l.network_time;
    // Separate the two timers. Virtual time, rather than scheduling before a 40ms Head,
    // proves the finite cumulative network budget expires first.
    l.read_timeout = network_budget * 2;
    let timed = Backend::new(l, tokio::runtime::Handle::current()).unwrap();
    let doc = timed.document(key(1), &s.url("/"), false).unwrap();
    let mut op = fresh(doc.open(request(&timed, s, "/slow", 1, b"")).unwrap());
    let Event::Head(head) = op.events.recv().await.unwrap() else {
        panic!("actual loopback Head")
    };
    op.credit(Credit {
        response: head.id,
        offset: 0,
        max_bytes: CHUNK_BYTES,
        allow_large: false,
    })
    .unwrap();
    let Event::Chunk { bytes, .. } = op.events.recv().await.unwrap() else {
        panic!("actual loopback body")
    };
    assert_eq!(bytes, b"hello");
    tokio::time::pause();
    op.credit(Credit {
        response: head.id,
        offset: bytes.len() as u64,
        max_bytes: CHUNK_BYTES,
        allow_large: false,
    })
    .unwrap();
    await_credit_consumed(&op).await;
    let clock = tokio::time::Instant::now();
    tokio::time::advance(network_budget + Duration::from_secs(1)).await;
    assert!(
        matches!(
            ready_clock_event(&mut op).await,
            Event::Failed {
                error: Error::Timeout,
                ..
            }
        ),
        "network budget must expire before the longer read timer"
    );
    assert!(tokio::time::Instant::now().duration_since(clock) < network_budget * 2);
    tokio::time::resume();
    assert_eq!(op.join().await.unwrap_err(), Error::Timeout);
}

async fn ready_clock_event(op: &mut Operation) -> Event {
    for _ in 0..10000 {
        match op.events.try_recv() {
            Ok(event) => return event,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => tokio::task::yield_now().await,
            Err(error) => panic!("network timeout event channel: {error}"),
        }
    }
    // 마지막 yield 뒤에 한 번 더 본다. 횟수 한도는 가상 시계 자동 전진을 막는 장치라 그대로 둔다.
    match op.events.try_recv() {
        Ok(event) => event,
        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
            panic!("network budget produced no event at the explicitly advanced clock")
        }
        Err(error) => panic!("network timeout event channel: {error}"),
    }
}
