mod correction;
mod fixture;
mod retirement;
mod security;
use super::*;
use fixture::Fixture;
use sha2::{Digest, Sha256};
use std::time::Duration;
fn limits() -> core::Limits {
    core::Limits {
        workers: 2,
        requests_per_document: 16,
        retained_bytes: 8_000_000,
        upload_bytes: 1_000_000,
        encoded_bytes: 1_000_000,
        decoded_bytes: 1_000_000,
        header_bytes: 65_536,
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        network_time: Duration::from_secs(5),
    }
}
fn id(n: u64) -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 2,
        binding: 3,
        document: 4,
        request: n,
        transfer: n,
    }
}
fn setup(url: &str) -> (Server, OwnerToken, core::Backend) {
    let l = limits();
    let b = core::Backend::new(l.clone(), tokio::runtime::Handle::current()).unwrap();
    let mut s = Server::new(b.clone(), 2, &l).unwrap();
    let t = s
        .admit(Capture {
            entry: id(1),
            source_generation: 7,
            entry_url: url.into(),
            allow_remote_content: false,
        })
        .unwrap();
    (s, t, b)
}
fn begin(n: u64, url: &str, body: &[u8]) -> wire::HttpRequest {
    wire::HttpRequest::Begin {
        id: id(n),
        source_generation: 7,
        method: "POST".into(),
        logical_url: url.into(),
        headers: vec![],
        upload: wire::BodyIdentity {
            bytes: body.len() as u64,
            sha256: Sha256::digest(body).into(),
        },
        allow_remote_content: false,
    }
}
fn handle(s: &mut Server, t: &OwnerToken, r: wire::HttpRequest) -> core::Result<()> {
    s.handle(t, |_| true, r)
}
async fn reply(s: &mut Server, t: &OwnerToken) -> wire::HttpReply {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(f) = s.poll(t, |_| true, MAX_FRAME_LEN as usize + 5).unwrap() {
                return serde_json::from_slice(&f.payload).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap()
}
async fn start(s: &mut Server, t: &OwnerToken, n: u64, url: &str, body: &[u8]) {
    handle(s, t, begin(n, url, body)).unwrap();
    assert!(matches!(
        reply(s, t).await,
        wire::HttpReply::UploadReady { offset: 0, .. }
    ));
    for (i, c) in body.chunks(wire::CHUNK_BYTES).enumerate() {
        handle(
            s,
            t,
            wire::HttpRequest::Upload {
                id: id(n),
                source_generation: 7,
                offset: (i * wire::CHUNK_BYTES) as u64,
                data_b64: encode(c),
            },
        )
        .unwrap();
        if (i + 1) * wire::CHUNK_BYTES < body.len() {
            assert!(matches!(
                reply(s, t).await,
                wire::HttpReply::UploadReady { .. }
            ));
        }
    }
    handle(
        s,
        t,
        wire::HttpRequest::Commit {
            id: id(n),
            source_generation: 7,
        },
    )
    .unwrap();
}
async fn collect(
    s: &mut Server,
    t: &OwnerToken,
) -> (wire::ResponseId, Vec<u8>, wire::BodyIdentity, usize) {
    let mut r = None;
    let mut bytes = Vec::new();
    let mut asked = 0;
    let mut allowed = false;
    loop {
        match reply(s, t).await {
            wire::HttpReply::Head { head } => {
                r = Some(head.id);
                if head.has_body && head.encoded_length.is_none_or(|n| n <= wire::AUTO_BYTES) {
                    grant(s, t, head.id, 0, false).unwrap();
                }
            }
            wire::HttpReply::AllowanceRequired { response, .. } => {
                asked += 1;
                allowed = true;
                grant(s, t, response, bytes.len() as u64, true).unwrap();
            }
            wire::HttpReply::Chunk {
                response,
                offset,
                data_b64,
            } => {
                assert_eq!(offset, bytes.len() as u64);
                bytes.extend(wire::decode_chunk(&data_b64).unwrap());
                grant(s, t, response, bytes.len() as u64, allowed).unwrap();
            }
            wire::HttpReply::Complete {
                response,
                encoded,
                decoded,
            } => {
                assert_eq!(Some(response), r);
                assert_eq!(encoded.bytes, bytes.len() as u64);
                assert_eq!(encoded.sha256, <[u8; 32]>::from(Sha256::digest(&bytes)));
                return (response, bytes, decoded, asked);
            }
            wire::HttpReply::Failure { code, reason, .. } => {
                panic!("unexpected {code:?}: {reason}")
            }
            _ => panic!("unexpected reply"),
        }
    }
}
fn grant(
    s: &mut Server,
    t: &OwnerToken,
    r: wire::ResponseId,
    offset: u64,
    allow: bool,
) -> core::Result<()> {
    let n = if !allow && offset < wire::AUTO_BYTES {
        (wire::AUTO_BYTES - offset).min(wire::CHUNK_BYTES as u64) as u32
    } else {
        wire::CHUNK_BYTES as u32
    };
    handle(
        s,
        t,
        wire::HttpRequest::Credit {
            response: r,
            offset,
            max_bytes: n,
            allow_large: allow,
        },
    )
}
#[tokio::test]
async fn post_upload_commit_reuse_stop_and_release_are_single_network_request() {
    let f = Fixture::new(12_345, false);
    let url = f.url();
    let (s, t, b) = setup(&url);
    let mut s = s;
    let body = vec![127; 70_000];
    handle(&mut s, &t, begin(1, &url, &body)).unwrap();
    assert_eq!(f.count(), 0);
    assert_eq!(
        handle(&mut s, &t, begin(1, &url, &body)),
        Err(core::Error::InProgress)
    );
    assert!(
        handle(
            &mut s,
            &t,
            wire::HttpRequest::Commit {
                id: id(1),
                source_generation: 7
            }
        )
        .is_err()
    );
    assert_eq!(f.count(), 0);
    assert!(matches!(
        reply(&mut s, &t).await,
        wire::HttpReply::UploadReady {
            max_bytes: 49152,
            ..
        }
    ));
    assert!(
        handle(
            &mut s,
            &t,
            wire::HttpRequest::Upload {
                id: id(1),
                source_generation: 7,
                offset: 1,
                data_b64: encode(&body[..49152])
            }
        )
        .is_err()
    );
    handle(
        &mut s,
        &t,
        wire::HttpRequest::Upload {
            id: id(1),
            source_generation: 7,
            offset: 0,
            data_b64: encode(&body[..49152]),
        },
    )
    .unwrap();
    // The next ready window must actually be published before the sender can consume it.
    assert!(
        handle(
            &mut s,
            &t,
            wire::HttpRequest::Upload {
                id: id(1),
                source_generation: 7,
                offset: 49152,
                data_b64: encode(&body[49152..])
            }
        )
        .is_err()
    );
    reply(&mut s, &t).await;
    handle(
        &mut s,
        &t,
        wire::HttpRequest::Upload {
            id: id(1),
            source_generation: 7,
            offset: 49152,
            data_b64: encode(&body[49152..]),
        },
    )
    .unwrap();
    assert_eq!(f.count(), 0);
    handle(
        &mut s,
        &t,
        wire::HttpRequest::Commit {
            id: id(1),
            source_generation: 7,
        },
    )
    .unwrap();
    let (r, bytes, _, _) = collect(&mut s, &t).await;
    assert_eq!(bytes.len(), 12_345);
    assert_eq!(f.count(), 1);
    assert_eq!(f.body(), body);
    handle(
        &mut s,
        &t,
        wire::HttpRequest::Stop {
            entry: id(1),
            source_generation: 7,
        },
    )
    .unwrap();
    handle(&mut s, &t, wire::HttpRequest::Reuse { response: r }).unwrap();
    assert!(matches!(
        reply(&mut s, &t).await,
        wire::HttpReply::Head { .. }
    ));
    assert!(matches!(
        reply(&mut s, &t).await,
        wire::HttpReply::Complete { .. }
    ));
    assert_eq!(f.count(), 1);
    assert_eq!(s.transferred_bytes(&t).unwrap(), 12_345);
    handle(&mut s, &t, wire::HttpRequest::Release { response: r }).unwrap();
    assert!(handle(&mut s, &t, wire::HttpRequest::Reuse { response: r }).is_err());
    assert_eq!(
        handle(&mut s, &t, begin(1, &url, &body)),
        Err(core::Error::AlreadyUsed)
    );
    assert_eq!(
        handle(&mut s, &t, begin(2, &url, &[])),
        Err(core::Error::Closed)
    );
    s.retire(&t);
    wait_workers(&b).await;
    assert_eq!(b.retained_payload_bytes(), 0);
}
#[tokio::test]
async fn empty_upload_and_unknown_exact_threshold_eof_do_not_request_allow() {
    let f = Fixture::new(100000, true);
    let url = f.url();
    let (mut s, t, b) = setup(&url);
    start(&mut s, &t, 1, &url, &[]).await;
    let (_, bytes, decoded, asked) = collect(&mut s, &t).await;
    assert_eq!(bytes.len(), 100000);
    assert_eq!(decoded.bytes, 100000);
    assert_eq!(asked, 0);
    assert_eq!(s.transferred_bytes(&t).unwrap(), 100000);
    s.retire(&t);
    wait_workers(&b).await;
}
#[tokio::test]
async fn unknown_threshold_and_known_large_allowance_resume_exact_identity() {
    for chunked in [true, false] {
        let f = Fixture::new(100001, chunked);
        let url = f.url();
        let (mut s, t, b) = setup(&url);
        start(&mut s, &t, 1, &url, &[]).await;
        let (_, bytes, _, asked) = collect(&mut s, &t).await;
        assert_eq!(asked, 1);
        assert_eq!(bytes.len(), 100001);
        assert_eq!(s.transferred_bytes(&t).unwrap(), 100001);
        s.retire(&t);
        wait_workers(&b).await;
    }
}
async fn wait_workers(b: &core::Backend) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while b.available_workers() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
