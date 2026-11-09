//! The actual model job is consumed by the native worker pool before verified publication.
use super::tests::{ready, reaped, serial, start, wait_result};
use super::*;
use sha2::{Digest, Sha256};
use tasty_ipc::{
    html_http::{BodyIdentity, HttpReply, HttpRequest, ResponseHead, ResponseId},
    html_transfer::TransferId,
};
use tasty_remote::{connection::channel, html_http_body::BodyBudget, html_http_client::*};
fn make_job() -> (
    ClientDocument,
    VerifyJob,
    TransferId,
    BodyBudget,
    PendingBudget,
) {
    let (sender, _rx) = channel();
    let id = TransferId {
        epoch: 1,
        surface_id: 42,
        binding: 1,
        document: 1,
        request: 1,
        transfer: 1,
    };
    let body = BodyBudget::new(1_000_000);
    let pending = PendingBudget::new(1_000_000);
    let mut model = ClientDocument::new(
        DocumentKey {
            entry: id,
            source_generation: 1,
            entry_url: "https://remote.example/".into(),
            allow_remote_content: false,
        },
        sender.epoch(),
        ClientLimits {
            requests: 8,
            upload_bytes: 1000,
            encoded_bytes: 1000,
            decoded_bytes: 1000,
            verify_jobs: 1,
        },
        pending.clone(),
        body.clone(),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("model"));
    let empty = BodyIdentity {
        bytes: 0,
        sha256: Sha256::digest([]).into(),
    };
    model
        .admit(
            HttpRequest::Begin {
                id,
                source_generation: 1,
                method: "GET".into(),
                logical_url: "https://remote.example/".into(),
                headers: vec![],
                upload: empty,
                allow_remote_content: false,
            },
            Arc::new(vec![]),
        )
        .unwrap();
    assert!(model.receive(HttpReply::UploadReady {
        id,
        source_generation: 1,
        offset: 0,
        max_bytes: 1
    }));
    assert!(matches!(
        model.next_command(),
        Some(HttpRequest::Commit { .. })
    ));
    let response = ResponseId {
        request: id,
        source_generation: 1,
        serial: 1,
    };
    assert!(model.receive(HttpReply::Head {
        head: ResponseHead {
            id: response,
            logical_url: "https://remote.example/".into(),
            status: 200,
            headers: vec![],
            encoded_length: Some(0),
            has_body: true
        }
    }));
    assert!(model.receive(HttpReply::Complete {
        response,
        encoded: empty,
        decoded: empty
    }));
    let job = model.take_verify_job().unwrap();
    (model, job, id, body, pending)
}
#[test]
fn actual_verify_job_finishes_on_pool_and_retired_submission_returns_same_owned_job() {
    let _serial = serial();
    let mut setup = start(Arc::new(|| true));
    let runtime = ready(&mut setup);
    let (mut model, job, id, body, pending) = make_job();
    assert!(runtime.spawn_verify(job).is_ok());
    wait_result(
        "verify job",
        || runtime.parts.jobs.available_permits() == 8,
        || model.poll_verification().then_some(()),
    );
    let verified = model.verified(id).unwrap();
    assert_eq!(verified.encoded(), b"");
    drop(verified);
    model.close();
    drop(model);
    drop(runtime);
    drop(setup);
    reaped();
    assert_eq!(body.used(), 0);
    assert_eq!(pending.used(), 0);
    let active = Arc::new(AtomicBool::new(true));
    let source = active.clone();
    let mut setup = start(Arc::new(move || source.load(Ordering::Acquire)));
    let runtime = ready(&mut setup);
    let (mut model, job, id, body, pending) = make_job();
    active.store(false, Ordering::Release);
    let job = runtime
        .spawn_verify(job)
        .expect_err("retired work returned");
    assert_eq!(model.state(id), Some(ReceiveState::Verifying));
    drop(job);
    assert!(model.poll_verification());
    assert_eq!(model.state(id), Some(ReceiveState::Failed));
    drop(model);
    drop(runtime);
    drop(setup);
    reaped();
    assert_eq!(body.used(), 0);
    assert_eq!(pending.used(), 0);
}
