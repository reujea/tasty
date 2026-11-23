use super::*;
use crate::connection::channel;
use tasty_ipc::html_http::{CommandKind, FailureCode, HttpRequest};
fn id(surface: u32, document: u64) -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: surface,
        binding: 1,
        document,
        request: document,
        transfer: document,
    }
}
fn captured(surface: u32, document: u64, current: Arc<AtomicBool>) -> Arc<CapturedDocument> {
    let (sender, _rx) = channel();
    CapturedDocument::new(
        DocumentKey {
            entry: id(surface, document),
            source_generation: 1,
            entry_url: "http://example.test/".into(),
            allow_remote_content: false,
        },
        sender.epoch(),
        ClientLimits {
            requests: 8,
            upload_bytes: 1_000_000,
            encoded_bytes: 1_000_000,
            decoded_bytes: 1_000_000,
            verify_jobs: 2,
        },
        PendingBudget::new(4_000_000),
        BodyBudget::new(4_000_000),
        Arc::new(move || current.load(Ordering::Acquire)),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("captured document"))
}
fn rejection(id: TransferId) -> HttpReply {
    HttpReply::CommandRejected {
        id,
        source_generation: 1,
        response: None,
        command: CommandKind::Begin,
        code: FailureCode::Stale,
        reason: "Stale".into(),
    }
}
#[test]
fn captured_source_retirement_and_table_drop_revoke_native_held_clones() {
    let current = Arc::new(AtomicBool::new(true));
    let document = captured(1, 1, current.clone());
    let mut documents = HttpDocuments::default();
    documents.insert(document.clone()).unwrap();
    assert!(document.with_current(|model| model.snapshot()).is_some());
    current.store(false, Ordering::Release);
    assert!(document.with_current(|_| ()).is_none());
    assert!(!document.is_current());
    let next = captured(1, 2, Arc::new(AtomicBool::new(true)));
    documents.insert(next.clone()).unwrap();
    drop(documents);
    assert!(!next.is_current());
    assert!(next.with_current(|_| ()).is_none());
}
#[test]
fn duplicate_registration_and_rejected_replacement_preserve_current_model() {
    let document = captured(1, 2, Arc::new(AtomicBool::new(true)));
    let mut documents = HttpDocuments::default();
    documents.insert(document.clone()).unwrap();
    documents.insert(document.clone()).unwrap();
    assert!(document.is_current());
    let older = captured(1, 1, Arc::new(AtomicBool::new(true)));
    assert_eq!(documents.insert(older), Err(ClientError::AlreadyUsed));
    assert!(document.is_current());
    let next = captured(1, 3, Arc::new(AtomicBool::new(true)));
    documents.insert(next.clone()).unwrap();
    assert!(!document.is_current());
    assert!(next.is_current());
}
#[test]
fn finite_table_and_full_identity_drop_unknown_replies_without_fallback() {
    let mut documents = HttpDocuments::default();
    for surface in 1..=8 {
        documents
            .insert(captured(surface, 1, Arc::new(AtomicBool::new(true))))
            .unwrap();
    }
    assert_eq!(
        documents.insert(captured(9, 1, Arc::new(AtomicBool::new(true)))),
        Err(ClientError::Quota)
    );
    assert_eq!(documents.snapshot().len(), 8);
    assert!(!documents.receive(rejection(id(99, 1))));
    assert!(!documents.receive(rejection(id(1, 99))));
    documents.retain(|remote| remote != 1);
    assert!(documents.get(1).is_none());
    assert_eq!(documents.snapshot().len(), 7);
}
#[test]
fn poisoned_model_is_retired_without_recovering_partially_mutated_state() {
    let document = captured(1, 1, Arc::new(AtomicBool::new(true)));
    let worker = document.clone();
    assert!(
        std::thread::spawn(move || worker.with_current(|_| panic!("fixture poison")))
            .join()
            .is_err()
    );
    assert!(!document.receive(rejection(id(1, 1))));
    assert!(!document.is_current());
    document.retire();
}
#[test]
fn retiring_one_document_preserves_another_model_on_the_same_connection_epoch() {
    let (sender, _rx) = channel();
    let epoch = sender.epoch();
    let make = |surface| {
        CapturedDocument::new(
            DocumentKey {
                entry: id(surface, 1),
                source_generation: 1,
                entry_url: "http://example.test/".into(),
                allow_remote_content: false,
            },
            epoch.clone(),
            ClientLimits {
                requests: 8,
                upload_bytes: 1_000,
                encoded_bytes: 1_000,
                decoded_bytes: 1_000,
                verify_jobs: 1,
            },
            PendingBudget::new(1_000_000),
            BodyBudget::new(1_000_000),
            Arc::new(|| true),
            Arc::new(|| {}),
        )
        .unwrap_or_else(|_| panic!("document"))
    };
    let first = make(1);
    let second = make(2);
    first.retire();
    assert!(!first.is_current());
    assert!(second.is_current());
    assert!(epoch.is_active());
    sender.retire();
    assert!(!second.is_current());
}
#[test]
fn command_rejection_routes_to_exact_model_without_rewriting_as_terminal_failure() {
    use sha2::{Digest, Sha256};
    let document = captured(1, 1, Arc::new(AtomicBool::new(true)));
    let begin = HttpRequest::Begin {
        id: id(1, 1),
        source_generation: 1,
        method: "POST".into(),
        logical_url: "http://example.test/".into(),
        headers: vec![],
        upload: tasty_ipc::html_http::BodyIdentity {
            bytes: 0,
            sha256: Sha256::digest([]).into(),
        },
        allow_remote_content: false,
    };
    assert!(
        document
            .with_current(|model| model.admit(begin, Arc::new(Vec::new())).is_ok())
            .unwrap()
    );
    assert!(
        document
            .with_current(|model| model.receive(HttpReply::UploadReady {
                id: id(1, 1),
                source_generation: 1,
                offset: 0,
                max_bytes: 1
            }))
            .unwrap()
    );
    assert!(
        document
            .with_current(|model| matches!(model.next_command(), Some(HttpRequest::Commit { .. })))
            .unwrap()
    );
    let mut documents = HttpDocuments::default();
    documents.insert(document.clone()).unwrap();
    assert!(!documents.receive(rejection(id(1, 1))));
    // A rejected duplicate Begin cannot tombstone an admitted upload or invent a retry.
    assert_eq!(
        document
            .with_current(|model| model.state(id(1, 1)))
            .flatten(),
        Some(crate::html_http_client::ReceiveState::WaitingHead)
    );
    assert!(
        document
            .with_current(|model| model.next_command().is_none())
            .unwrap()
    );
}

#[test]
fn actual_outbox_preserves_http_arrival_order_and_accounts_reply_capacity() {
    use crate::client_session::{MirrorEvent, MirrorOutbox};
    let (sender, _rx) = channel();
    let outbox = MirrorOutbox::new(sender.epoch());
    assert!(outbox.push(MirrorEvent::Data(
        1,
        b"before".to_vec(),
        std::time::Instant::now()
    )));
    assert!(outbox.push(MirrorEvent::Http(rejection(id(1, 1)))));
    assert!(outbox.push(MirrorEvent::Data(
        1,
        b"after".to_vec(),
        std::time::Instant::now()
    )));
    assert!(
        matches!(&outbox.drain()[..], [MirrorEvent::Data(_,a,_),MirrorEvent::Http(HttpReply::CommandRejected { .. }),MirrorEvent::Data(_,b,_)] if a == b"before" && b == b"after")
    );
    for _ in 0..32 {
        let mut reason = String::with_capacity(2 * 1024 * 1024);
        reason.push_str("Pending");
        let mut reply = rejection(id(1, 1));
        if let HttpReply::CommandRejected { reason: value, .. } = &mut reply {
            *value = reason;
        }
        assert!(outbox.push(MirrorEvent::Http(reply)));
    }
    assert!(matches!(
        &outbox.drain()[..],
        [MirrorEvent::Desynced { frames: 32 }]
    ));
    sender.retire();
    assert!(!outbox.push(MirrorEvent::Http(rejection(id(1, 1)))));
}
