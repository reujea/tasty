use super::*;
use base64::Engine;
use tasty_ipc::html_transfer::{HtmlMetadata, HtmlSource};
fn mirror() -> HtmlMirror {
    let (sender, _receiver) = crate::connection::channel();
    HtmlMirror::new(7, 9, sender.epoch())
}
fn id() -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 7,
        binding: 9,
        document: 1,
        request: 1,
        transfer: 1,
    }
}
fn metadata(id: TransferId, size: u64) -> HtmlReply {
    HtmlReply::HtmlMetadata {
        id,
        metadata: HtmlMetadata {
            source: HtmlSource::File {
                path: "/server/remote.html".into(),
                url: "file:///server/remote.html".into(),
            },
            source_generation: 1,
            root: Some("/server".into()),
            cwd: Some("/server".into()),
            size,
            regular: true,
            file: None,
        },
    }
}
fn completed(bytes: &[u8]) -> HtmlMirror {
    let mut m = mirror();
    assert_eq!(m.begin(id()).len(), 1);
    assert!(m.receive(metadata(id(), bytes.len() as u64)));
    m.credit().unwrap();
    m.receive(HtmlReply::HtmlChunk {
        id: id(),
        offset: 0,
        data_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    assert!(m.receive(HtmlReply::HtmlComplete {
        id: id(),
        size: bytes.len() as u64
    }));
    m
}
fn scan(m: &mut HtmlMirror) {
    let (tx, rx) = mpsc::sync_channel(1);
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if tx.send(()).is_err() { /* The test owner may already have retired. */ }
    });
    // The worker may finish before the starting call's own try_recv, so either call may publish.
    if !m.poll_scan(Arc::clone(&notify)) {
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .expect("scan worker notifies after sending its result");
        assert!(m.poll_scan(notify));
    }
    assert!(m.failure.is_none());
    assert!(m.body_store().current().is_some());
}
#[test]
fn reload_still_ends_an_expired_size_question() {
    let mut m = mirror();
    m.begin(id());
    m.receive(metadata(id(), 300_000));
    m.receive(HtmlReply::HtmlFailure {
        id: id(),
        reason: "credit timeout".into(),
    });
    assert!(m.user_reload());
    assert!(!m.allow(id()));
    assert!(m.take_retries().is_empty());
    let retry = TransferId {
        document: 2,
        request: 2,
        transfer: 2,
        ..id()
    };
    assert_eq!(m.begin(retry).len(), 1);
    m.receive(metadata(retry, 300_000));
    assert!(!m.allow(id()));
    assert!(m.allow(retry));
    assert!(m.credit().is_some());
    assert!(!m.receive(HtmlReply::HtmlCancelled { id: id() }));
}
#[test]
fn declined_entry_has_no_receiving_or_late_body() {
    let mut m = mirror();
    m.begin(id());
    m.receive(metadata(id(), 300_000));
    assert!(m.decline(id()).is_some());
    assert!(m.decline(id()).is_none());
    assert!(!m.document.as_ref().unwrap().receiving());
    assert_eq!(m.document.as_ref().unwrap().skipped, 1);
    assert!(!m.receive(HtmlReply::HtmlComplete {
        id: id(),
        size: 300_000
    }));
    assert!(m.body_store().current().is_none());
    assert!(!m.allow(id()));
}
#[test]
fn scan_publishes_received_bytes_and_disconnect_preserves_that_allocation() {
    let mut m = completed(b"<script>globalThis.received=1</script>");
    scan(&mut m);
    let body = Arc::clone(m.body_store().current().unwrap());
    assert_eq!(body.response().source_url(), "file:///server/remote.html");
    assert_eq!(
        body.response().bytes(),
        b"<script>globalThis.received=1</script>"
    );
    m.disconnect();
    assert!(!m.user_reload());
    assert!(m.credit().is_none());
    assert!(Arc::ptr_eq(m.body_store().current().unwrap(), &body));
    assert!(!m.receive(HtmlReply::HtmlFailure {
        id: id(),
        reason: "late".into()
    }));
    assert!(m.failure.is_none());
}
#[test]
fn user_reload_clears_body_and_all_document_budgets() {
    let mut m = completed(b"entry");
    scan(&mut m);
    assert!(m.user_reload());
    assert!(m.body_store().current().is_none());
    let next = TransferId {
        document: 2,
        request: 2,
        transfer: 2,
        ..id()
    };
    m.begin(next);
    let d = m.document.as_ref().unwrap();
    assert_eq!(d.received, 0);
    assert_eq!(d.skipped, 0);
    assert!(d.notices.is_empty());
    assert_eq!(d.resources.len(), 1);
    assert!(!m.receive(HtmlReply::HtmlComplete { id: id(), size: 5 }));
}
#[test]
fn retired_connection_rejects_notice_action_credit_and_new_request() {
    let mut m = mirror();
    m.begin(id());
    m.receive(metadata(id(), 300_000));
    m.connection.retire();
    assert!(!m.allow(id()));
    assert!(m.credit().is_none());
    assert!(!m.user_reload());
    assert!(!m.receive(HtmlReply::HtmlFailure {
        id: id(),
        reason: "late".into()
    }));
}

#[test]
fn late_notice_choices_cannot_cancel_an_already_allowed_entry() {
    let mut m = mirror();
    m.begin(id());
    m.receive(metadata(id(), 300_000));
    assert!(m.allow(id()));
    assert!(m.credit().is_some());
    m.queue_decline(id());
    m.queue_stop(id());
    assert!(m.take_pending().is_empty());
    assert_eq!(m.state(), Some(ReceiveState::Receiving));
    assert_eq!(m.document.as_ref().unwrap().skipped, 0);
}

#[test]
fn reload_retires_old_replies_and_notice_choices_before_the_next_request_is_sent() {
    let mut m = mirror();
    m.begin(id());
    m.receive(metadata(id(), 300_000));
    assert!(m.user_reload());
    assert!(!m.receive(HtmlReply::HtmlFailure {
        id: id(),
        reason: "late".into()
    }));
    assert!(!m.allow(id()));
    assert!(m.decline(id()).is_none());
    assert!(m.credit().is_none());
    assert!(m.failure.is_none());
    assert!(m.needs_request());
}

mod resources;
mod retry;
