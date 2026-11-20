//! The actual attach synchronization pass drives only its negotiated connection's HTTP owners.
use super::*;
use crate::app::attach_client::tests::test_session_with_frames;
use std::collections::HashMap;
use tasty_ipc::html_http::{BodyIdentity, HttpReply, HttpRequest};
use tasty_remote::{
    html_http_body::BodyBudget, html_http_client::*, html_http_session::CapturedDocument,
};
fn document(sess: &mut AttachClientSession) -> Arc<CapturedDocument> {
    let entry = sess.transport.html.entry_id(42, 1, 1).unwrap();
    let document = CapturedDocument::new(
        DocumentKey {
            entry,
            source_generation: 1,
            entry_url: "http://example.test/".into(),
            allow_remote_content: false,
        },
        sess.transport.frame_tx.epoch(),
        ClientLimits {
            requests: 8,
            upload_bytes: 1000,
            encoded_bytes: 1000,
            decoded_bytes: 1000,
            verify_jobs: 1,
        },
        PendingBudget::new(1_000_000),
        BodyBudget::new(1_000_000),
        Arc::new(|| true),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("captured document"));
    sess.transport
        .html
        .http_documents
        .insert(document.clone())
        .unwrap();
    document
}
#[test]
fn http_sender_actual_sync_pumps_commit_once_on_the_original_connection() {
    let (mut sess, rx) = test_session_with_frames(9000, HashMap::new());
    sess.transport.html.server_http = true;
    let document = document(&mut sess);
    let id = document.entry();
    use sha2::{Digest, Sha256};
    document
        .admit_and_send(
            &sess.transport.frame_tx,
            HttpRequest::Begin {
                id,
                source_generation: 1,
                method: "POST".into(),
                logical_url: "http://example.test/".into(),
                headers: vec![],
                upload: BodyIdentity {
                    bytes: 0,
                    sha256: Sha256::digest([]).into(),
                },
                allow_remote_content: false,
            },
            Arc::new(vec![]),
        )
        .unwrap();
    assert!(matches!(
        serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),
        HttpRequest::Begin { .. }
    ));
    assert!(document.receive(HttpReply::UploadReady {
        id,
        source_generation: 1,
        offset: 0,
        max_bytes: 1
    }));
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    assert!(synchronize(&mut sess, &mut engine));
    assert!(
        matches!(serde_json::from_slice::<HttpRequest>(&rx.try_recv().unwrap().frame.payload).unwrap(),HttpRequest::Commit {id:actual,..} if actual==id)
    );
    assert!(!synchronize(&mut sess, &mut engine));
    assert!(rx.try_recv().is_err());
}
#[test]
fn http_sender_old_peer_and_disconnect_retire_captured_owners_without_sending() {
    let (mut sess, rx) = test_session_with_frames(9000, HashMap::new());
    let first = document(&mut sess);
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    assert!(!synchronize(&mut sess, &mut engine));
    assert!(!first.is_current());
    assert!(rx.try_recv().is_err());
    sess.transport.html.server_http = true;
    let second = document(&mut sess);
    sess.transport
        .end
        .raise(tasty_remote::transport_end::EndReason::Lost);
    assert!(!synchronize(&mut sess, &mut engine));
    assert!(!second.is_current());
    assert!(rx.try_recv().is_err());
}

#[test]
fn http_declaration_requires_server_transport_and_actual_renderer_readiness() {
    let (mut sess, _) = test_session_with_frames(9000, HashMap::new());
    for readiness in [false, true] {
        assert!(
            !viewer_capabilities(&sess, readiness)
                .contains(&tasty_ipc::html_http::CAPABILITY.into())
        );
    }
    sess.transport.html.server_http = true;
    sess.transport.html.server_https = true;
    sess.transport.html.server_http_reload = true;
    let unavailable = viewer_capabilities(&sess, false);
    assert!(unavailable.contains(&VIEWER_CAPABILITY.into()));
    for capability in [
        tasty_ipc::html_http::CAPABILITY,
        tasty_ipc::html_http::HTTPS_CAPABILITY,
        tasty_ipc::html_http::RELOAD_CAPABILITY,
    ] {
        assert!(!unavailable.contains(&capability.into()));
        assert!(viewer_capabilities(&sess, true).contains(&capability.into()));
    }
    sess.transport.html.server_https = false;
    sess.transport.html.server_http_reload = false;
    let http_only = viewer_capabilities(&sess, true);
    assert!(http_only.contains(&tasty_ipc::html_http::CAPABILITY.into()));
    assert!(!http_only.contains(&tasty_ipc::html_http::HTTPS_CAPABILITY.into()));
    assert!(!http_only.contains(&tasty_ipc::html_http::RELOAD_CAPABILITY.into()));
}

#[cfg(target_os = "linux")]
#[test]
fn actual_sync_updates_native_capabilities_without_resetting_the_file_viewer() {
    let (mut sess, rx) = test_session_with_frames(9000, HashMap::new());
    sess.transport.html.server_transfer = true;
    sess.transport.html.server_entry = true;
    sess.transport.html.server_http = true;
    sess.transport.html.server_https = true;
    sess.transport.html.server_http_reload = true;
    let (_, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for readiness in [false, true, false] {
        engine.runtime.http_viewer_available = readiness;
        synchronize(&mut sess, &mut engine);
        let frame = rx.try_recv().expect("capability transition");
        let HtmlRequest::HtmlCapabilities { capabilities } =
            serde_json::from_slice(&frame.frame.payload).unwrap()
        else {
            panic!("actual declaration");
        };
        assert!(capabilities.contains(&CAPABILITY.into()));
        assert!(capabilities.contains(&VIEWER_CAPABILITY.into()));
        assert_eq!(
            capabilities.contains(&tasty_ipc::html_http::CAPABILITY.into()),
            readiness
        );
        synchronize(&mut sess, &mut engine);
        assert!(
            rx.try_recv().is_err(),
            "unchanged availability does not repeat declarations"
        );
    }
}
