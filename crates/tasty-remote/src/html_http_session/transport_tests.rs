//! Actual sender queue and TCP writer controls for command advancement and captured retirement.
use super::*;
use crate::{
    client_session::OutFrame,
    connection::{ConnectionSender, channel},
};
use sha2::{Digest, Sha256};
use std::{
    net::{TcpListener, TcpStream},
    time::Duration,
};
use tasty_ipc::{
    html_http::{BodyIdentity, CHUNK_BYTES, HttpRequest},
    stream::{self, StreamTag},
};
fn id() -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 1,
        binding: 1,
        document: 1,
        request: 1,
        transfer: 1,
    }
}
fn setup(sender: &ConnectionSender) -> Arc<CapturedDocument> {
    CapturedDocument::new(
        DocumentKey {
            entry: id(),
            source_generation: 1,
            entry_url: "http://example.test/".into(),
            allow_remote_content: false,
        },
        sender.epoch(),
        ClientLimits {
            requests: 64,
            upload_bytes: 1_000_000,
            encoded_bytes: 1_000_000,
            decoded_bytes: 1_000_000,
            verify_jobs: 1,
        },
        PendingBudget::new(4_000_000),
        BodyBudget::new(4_000_000),
        Arc::new(|| true),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("document"))
}
fn begin(bytes: &[u8]) -> HttpRequest {
    HttpRequest::Begin {
        id: id(),
        source_generation: 1,
        method: "POST".into(),
        logical_url: "http://example.test/".into(),
        headers: vec![],
        upload: BodyIdentity {
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        },
        allow_remote_content: false,
    }
}
#[test]
fn ordered_sender_final_upload_commits_without_a_trailing_ready_and_never_replays() {
    for size in [0, 1, CHUNK_BYTES + 1] {
        let (sender, rx) = channel();
        let doc = setup(&sender);
        let upload = Arc::new(vec![0x9f; size]);
        doc.admit_and_send(&sender, begin(&upload), upload.clone())
            .unwrap();
        let frame = rx.try_recv().unwrap();
        assert!(matches!(
            serde_json::from_slice::<HttpRequest>(&frame.frame.payload).unwrap(),
            HttpRequest::Begin { .. }
        ));
        drop(frame);
        let (mut offset, mut commits) = (0, 0);
        loop {
            assert!(doc.receive(HttpReply::UploadReady {
                id: id(),
                source_generation: 1,
                offset: offset as u64,
                max_bytes: CHUNK_BYTES as u32
            }));
            assert!(doc.pump_commands(&sender));
            for frame in rx.try_iter() {
                match serde_json::from_slice::<HttpRequest>(&frame.frame.payload).unwrap() {
                    HttpRequest::Upload {
                        offset: actual,
                        data_b64,
                        ..
                    } => {
                        assert_eq!(actual, offset as u64);
                        let bytes = tasty_ipc::html_http::decode_chunk(&data_b64).unwrap();
                        assert_eq!(bytes.as_slice(), &upload[offset..offset + bytes.len()]);
                        offset += bytes.len();
                    }
                    HttpRequest::Commit { .. } => commits += 1,
                    _ => panic!("no Begin replay or invented command"),
                }
            }
            if offset == size {
                break;
            }
            assert_eq!(commits, 0);
        }
        assert_eq!(commits, 1);
        assert_eq!(offset, size);
        assert!(!doc.pump_commands(&sender));
        assert!(rx.try_recv().is_err());
    }
}
#[test]
fn wrong_sender_cannot_advance_model_and_failed_enqueue_cannot_be_retried() {
    let (sender, rx) = channel();
    let (other, other_rx) = channel();
    let doc = setup(&sender);
    assert_eq!(
        doc.admit_and_send(&other, begin(&[]), Arc::new(vec![])),
        Err(ClientError::Stale)
    );
    assert!(other_rx.try_recv().is_err());
    assert!(rx.try_recv().is_err());
    assert!(doc.is_current());
    drop(rx);
    assert_eq!(
        doc.admit_and_send(&sender, begin(&[]), Arc::new(vec![])),
        Err(ClientError::Closed)
    );
    assert!(!sender.epoch().is_active());
    assert_eq!(
        doc.admit_and_send(&sender, begin(&[]), Arc::new(vec![])),
        Err(ClientError::Closed)
    );
}
#[test]
fn actual_tcp_writer_drops_retired_http_begin_and_keeps_live_legacy_frames() {
    for terminal_cleanup in [false, true] {
        let (sender, rx) = channel();
        let doc = setup(&sender);
        doc.admit_and_send(&sender, begin(&[]), Arc::new(vec![]))
            .unwrap();
        if terminal_cleanup {
            doc.close_and_send(&sender).unwrap();
            assert_eq!(doc.close_and_send(&sender), Err(ClientError::Stale));
        } else {
            doc.retire();
        }
        sender
            .send(OutFrame {
                tag: StreamTag::Data,
                payload: b"live".to_vec(),
            })
            .unwrap();
        sender
            .send(OutFrame {
                tag: StreamTag::Detach,
                payload: vec![],
            })
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let worker = crate::transport::spawn_attach_write_thread(
            socket,
            rx,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(|| {}),
            " owned HTTP retirement fixture",
        );
        let mut frames = Vec::new();
        loop {
            let frame = stream::read_frame(&mut peer).unwrap();
            let last = frame.tag == StreamTag::Detach;
            frames.push((frame.tag, frame.payload));
            if last {
                break;
            }
        }
        let mut end = [0];
        assert_eq!(std::io::Read::read(&mut peer, &mut end).unwrap(), 0);
        worker.join().unwrap();
        let mut expected = Vec::new();
        if terminal_cleanup {
            expected.push((
                StreamTag::Control,
                HttpRequest::Close {
                    entry: id(),
                    source_generation: 1,
                }
                .frame()
                .unwrap()
                .payload,
            ));
        }
        expected.extend([
            (StreamTag::Data, b"live".to_vec()),
            (StreamTag::Detach, vec![]),
        ]);
        assert_eq!(frames, expected);
        assert!(sender.epoch().is_active());
    }
}

#[test]
fn command_pump_bounds_each_pass_without_losing_the_next_commit() {
    let (sender, rx) = channel();
    let doc = setup(&sender);
    for sequence in 1..=33 {
        let mut request = begin(&[]);
        let mut transfer = id();
        transfer.request = sequence;
        transfer.transfer = sequence;
        if let HttpRequest::Begin { id, .. } = &mut request {
            *id = transfer;
        }
        doc.admit_and_send(&sender, request, Arc::new(vec![]))
            .unwrap();
        assert!(doc.receive(HttpReply::UploadReady {
            id: transfer,
            source_generation: 1,
            offset: 0,
            max_bytes: 1
        }));
    }
    assert_eq!(rx.try_iter().count(), 33);
    assert!(doc.pump_commands(&sender));
    assert_eq!(rx.try_iter().count(), 32);
    assert!(doc.pump_commands(&sender));
    let last = rx.try_recv().unwrap();
    assert!(
        matches!(serde_json::from_slice::<HttpRequest>(&last.frame.payload).unwrap(),HttpRequest::Commit { id,.. } if id.request==33)
    );
    assert!(rx.try_recv().is_err());
    assert!(!doc.pump_commands(&sender));
}

#[test]
fn typed_actions_keep_byte_consent_separate_and_decline_only_the_exact_attempt() {
    let (sender, rx) = channel();
    let (wrong, _other) = channel();
    let doc = setup(&sender);
    doc.admit_and_send(&sender, begin(&[]), Arc::new(vec![]))
        .unwrap();
    assert_eq!(rx.try_iter().count(), 1);
    let mut stale = id();
    stale.document += 1;
    assert!(!doc.apply(&sender, HttpAction::Allow(stale)).unwrap());
    assert!(!doc.apply(&sender, HttpAction::Decline(stale)).unwrap());
    assert_eq!(doc.apply(&wrong, HttpAction::Stop), Err(ClientError::Stale));
    assert!(rx.try_recv().is_err());
    assert!(doc.is_current());
    assert!(doc.apply(&sender, HttpAction::Decline(id())).unwrap());
    assert!(
        matches!(serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),HttpRequest::Cancel {id:actual,source_generation:1} if actual==id())
    );
    assert!(
        !doc.with_current(|model| model.snapshot().receiving)
            .unwrap()
    );
    assert!(!doc.apply(&sender, HttpAction::Allow(id())).unwrap());
    assert!(!doc.apply(&sender, HttpAction::Decline(id())).unwrap());
    assert!(rx.try_recv().is_err());
}
fn verified_empty_response(
    doc: &Arc<CapturedDocument>,
    sender: &ConnectionSender,
    rx: &std::sync::mpsc::Receiver<crate::connection::QueuedFrame>,
) -> (
    tasty_ipc::html_http::ResponseHead,
    BodyIdentity,
    Arc<crate::html_http_body::VerifiedBody>,
) {
    use tasty_ipc::html_http::{ResponseHead, ResponseId};
    doc.admit_and_send(sender, begin(&[]), Arc::new(vec![]))
        .unwrap();
    assert_eq!(rx.try_iter().count(), 1);
    assert!(doc.receive(HttpReply::UploadReady {
        id: id(),
        source_generation: 1,
        offset: 0,
        max_bytes: 1
    }));
    assert!(doc.pump_commands(sender));
    assert!(matches!(
        serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),
        HttpRequest::Commit { .. }
    ));
    let response = ResponseId {
        request: id(),
        source_generation: 1,
        serial: 1,
    };
    let head = ResponseHead {
        id: response,
        logical_url: "http://example.test/".into(),
        status: 200,
        headers: vec![],
        encoded_length: Some(0),
        has_body: true,
    };
    let empty = BodyIdentity {
        bytes: 0,
        sha256: Sha256::digest([]).into(),
    };
    assert!(doc.receive(HttpReply::Head { head: head.clone() }));
    assert!(doc.receive(HttpReply::Complete {
        response,
        encoded: empty,
        decoded: empty
    }));
    doc.with_current(|model| model.take_verify_job())
        .flatten()
        .unwrap()
        .run();
    assert!(doc.with_current(|model| model.poll_verification()).unwrap());
    let original = doc
        .with_current(|model| model.verified(id()))
        .flatten()
        .unwrap();
    (head, empty, original)
}

#[test]
fn typed_reuse_and_release_use_same_verified_owner_without_another_begin_or_decode() {
    let (sender, rx) = channel();
    let doc = setup(&sender);
    let (head, empty, original) = verified_empty_response(&doc, &sender, &rx);
    let response = head.id;
    assert!(doc.apply(&sender, HttpAction::Reuse(response)).unwrap());
    assert!(
        matches!(serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),HttpRequest::Reuse {response:actual} if actual==response)
    );
    assert!(rx.try_recv().is_err());
    assert!(doc.receive(HttpReply::Head { head }));
    assert!(doc.receive(HttpReply::Complete {
        response,
        encoded: empty,
        decoded: empty
    }));
    let reused = doc
        .with_current(|model| model.verified(id()))
        .flatten()
        .unwrap();
    assert!(Arc::ptr_eq(&original, &reused));
    assert!(
        doc.with_current(|model| model.take_verify_job().is_none())
            .unwrap()
    );
    assert!(doc.apply(&sender, HttpAction::Release(response)).unwrap());
    assert!(
        matches!(serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),HttpRequest::Release {response:actual} if actual==response)
    );
    assert!(!doc.apply(&sender, HttpAction::Reuse(response)).unwrap());
    assert!(rx.try_recv().is_err());
}
