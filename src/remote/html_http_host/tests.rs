mod fixture;
mod reload;
use super::*;
use base64::Engine;
use fixture::Fixture;
use sha2::{Digest, Sha256};
use tasty_ipc::{
    html_http::{BodyIdentity, HttpReply},
    html_transfer::{CAPABILITY, HtmlRequest, VIEWER_CAPABILITY},
    stream::{StreamFrame, StreamTag},
    stream_hub::{SinkReceiver, StreamInbound},
};
fn id() -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 1,
        binding: 2,
        document: 3,
        request: 4,
        transfer: 5,
    }
}
fn identity(body: &[u8]) -> BodyIdentity {
    BodyIdentity {
        bytes: body.len() as u64,
        sha256: Sha256::digest(body).into(),
    }
}
/// HttpReply has no Debug; its wire form names the event and any failure reason.
fn describe(reply: &HttpReply) -> String {
    serde_json::to_string(reply).unwrap()
}
struct Host {
    session: Option<crate::runtime::engine_session::EngineSession>,
    hub: StreamHub,
    rx: SinkReceiver,
    wake: Arc<std::sync::atomic::AtomicUsize>,
}
impl Host {
    fn new(url: &str) -> Self {
        let wake = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let notified = wake.clone();
        let mut session = crate::runtime::engine_session::EngineSession::new(
            80,
            24,
            Arc::new(move || {
                notified.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            }),
        )
        .unwrap();
        let surface = crate::plugin_bridge::remote_surface::RemoteSurface::new(
            1,
            "html",
            "com.tasty.html".into(),
            "fixture".into(),
        );
        surface.set_webview_url(Some(url.into()), None, true);
        session.runtime.surfaces.insert(1, Box::new(surface));
        session
            .live
            .occupancy
            .acquire_workspace(7, &[], &[1], 41)
            .unwrap();
        let hub = StreamHub::new();
        let rx = hub.register(41);
        let (tx, input) = mpsc::channel();
        let cap = HtmlRequest::HtmlCapabilities {
            capabilities: vec![
                CAPABILITY.into(),
                VIEWER_CAPABILITY.into(),
                wire::CAPABILITY.into(),
                wire::HTTPS_CAPABILITY.into(),
                wire::RELOAD_CAPABILITY.into(),
            ],
        };
        tx.send(StreamInbound::Frame {
            client_id: 41,
            frame: StreamFrame::new(StreamTag::Control, serde_json::to_vec(&cap).unwrap()),
        })
        .unwrap();
        session.remote.set_notifier(hub.clone());
        for (client, control) in hub.pump_inbound(&input).html_requests {
            session
                .borrow_mut()
                .handle_html_control(&hub, client, control);
        }
        Self {
            session: Some(session),
            hub,
            rx,
            wake,
        }
    }
    fn begin(&self, url: &str, upload: &[u8]) -> HttpRequest {
        HttpRequest::Begin {
            id: id(),
            source_generation: 1,
            method: "POST".into(),
            logical_url: url.into(),
            headers: vec![],
            upload: identity(upload),
            allow_remote_content: false,
        }
    }
    fn send(&mut self, request: HttpRequest) {
        let (tx, input) = mpsc::channel();
        tx.send(StreamInbound::Frame {
            client_id: 41,
            frame: request.frame().unwrap(),
        })
        .unwrap();
        for (client, control) in self.hub.pump_inbound(&input).html_requests {
            self.session
                .as_mut()
                .unwrap()
                .borrow_mut()
                .handle_html_control(&self.hub, client, control);
        }
    }
    fn tick(&mut self) {
        self.session
            .as_mut()
            .unwrap()
            .borrow_mut()
            .poll_html_transfers();
    }
    #[track_caller]
    fn receive(&mut self) -> HttpReply {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            self.tick();
            if let Ok(frame) = self.rx.try_recv() {
                return serde_json::from_slice(&frame.payload).unwrap();
            }
            assert!(std::time::Instant::now() < until, "host reply timeout");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn commit(&mut self, url: &str, upload: &[u8]) -> wire::ResponseId {
        self.send(self.begin(url, upload));
        assert!(matches!(
            self.receive(),
            HttpReply::UploadReady { offset: 0, .. }
        ));
        if !upload.is_empty() {
            self.send(HttpRequest::Upload {
                id: id(),
                source_generation: 1,
                offset: 0,
                data_b64: base64::engine::general_purpose::STANDARD.encode(upload),
            });
            // The final upload consumes its window; Commit needs no further UploadReady.
        }
        self.send(HttpRequest::Commit {
            id: id(),
            source_generation: 1,
        });
        match self.receive() {
            HttpReply::Head { head } => head.id,
            other => panic!("expected head, got {}", describe(&other)),
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        drop(self.session.take());
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !setup::reaped_for_tests() {
            assert!(
                std::time::Instant::now() < until,
                "owned runtime retirement"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
#[test]
fn actual_host_preserves_binary_post_credit_rejection_and_exact_reuse() {
    let fixture = Fixture::new();
    let url = fixture.url();
    let mut host = Host::new(&url);
    let response = host.commit(&url, b"\0owned\xffPOST");
    let credit = HttpRequest::Credit {
        response,
        offset: 0,
        max_bytes: 16,
        allow_large: false,
    };
    host.send(credit.clone());
    host.send(credit);
    assert!(
        matches!(host.receive(),HttpReply::CommandRejected{response:Some(r),command:wire::CommandKind::Credit,code:wire::FailureCode::InProgress,..} if r==response)
    );
    let baseline = host.wake.load(std::sync::atomic::Ordering::Acquire);
    fixture.release();
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while host.wake.load(std::sync::atomic::Ordering::Acquire) == baseline {
        assert!(
            std::time::Instant::now() < until,
            "backend event must wake the actual engine callback"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let HttpReply::Chunk {
        response: r,
        offset: 0,
        data_b64,
    } = host.receive()
    else {
        panic!("chunk")
    };
    assert_eq!(r, response);
    let body = wire::decode_chunk(&data_b64).unwrap();
    assert_eq!(body, b"owned body");
    host.send(HttpRequest::Credit {
        response,
        offset: body.len() as u64,
        max_bytes: 1,
        allow_large: false,
    });
    let HttpReply::Complete {
        encoded, decoded, ..
    } = host.receive()
    else {
        panic!("complete")
    };
    assert_eq!(encoded, identity(&body));
    assert_eq!(decoded, encoded);
    host.send(HttpRequest::Reuse { response });
    assert!(matches!(host.receive(),HttpReply::Head{head} if head.id==response));
    assert!(
        matches!(host.receive(),HttpReply::Complete{encoded:e,decoded:d,..}if e==encoded&&d==decoded)
    );
    assert_eq!(fixture.count(), 1);
    assert_eq!(fixture.upload(), b"\0owned\xffPOST");
    assert_stop_preserves_reuse(&mut host, &url, response);
    assert_eq!(fixture.count(), 1);
}
fn assert_stop_preserves_reuse(host: &mut Host, url: &str, response: wire::ResponseId) {
    host.send(HttpRequest::Stop {
        entry: id(),
        source_generation: 1,
    });
    let mut new = host.begin(url, &[]);
    if let HttpRequest::Begin { id, .. } = &mut new {
        id.request += 1;
        id.transfer += 1;
    }
    let next = new.id();
    host.send(new);
    assert!(matches!(
        host.receive(),
        HttpReply::CommandRejected {
            code: wire::FailureCode::Closed,
            ..
        }
    ));
    assert!(
        host.hub
            .admit_html_request(41, &host.hub.client_binding(41).unwrap(), next)
    );
    host.send(HttpRequest::Reuse { response });
    assert!(matches!(host.receive(), HttpReply::Head { .. }));
    assert!(matches!(host.receive(), HttpReply::Complete { .. }));
}
#[test]
fn wrong_workspace_source_and_missing_native_capability_never_admit_or_contact() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        let url = fixture.url();
        let mut host = Host::new(&url);
        let mut request = host.begin(&url, &[]);
        if mode == 0 {
            let session = host.session.as_mut().unwrap();
            session.live.occupancy.force_detach_workspace(7);
            session
                .live
                .occupancy
                .acquire_workspace(8, &[], &[99], 41)
                .unwrap();
        }
        if mode == 1
            && let HttpRequest::Begin {
                source_generation, ..
            } = &mut request
        {
            *source_generation = 999;
        }
        if mode == 2 {
            let (tx, rx) = mpsc::channel();
            tx.send(StreamInbound::Frame {
                client_id: 41,
                frame: StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&HtmlRequest::HtmlCapabilities {
                        capabilities: vec![CAPABILITY.into(), wire::CAPABILITY.into()],
                    })
                    .unwrap(),
                ),
            })
            .unwrap();
            host.hub.pump_inbound(&rx);
        }
        host.send(request);
        assert!(matches!(host.receive(), HttpReply::CommandRejected { .. }));
        assert!(
            host.session
                .as_ref()
                .unwrap()
                .remote
                .html_http
                .documents
                .is_empty()
        );
        assert!(
            host.hub
                .admit_html_request(41, &host.hub.client_binding(41).unwrap(), id())
        );
        assert_eq!(fixture.count(), 0);
    }
}
#[test]
fn queued_positive_response_loses_actual_grant_membership_or_source_before_writer() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        let url = fixture.url();
        let mut host = Host::new(&url);
        host.send(host.begin(&url, &[]));
        assert!(matches!(host.receive(), HttpReply::UploadReady { .. }));
        host.send(HttpRequest::Commit {
            id: id(),
            source_generation: 1,
        });
        let until = std::time::Instant::now() + Duration::from_secs(10);
        let frame = loop {
            host.tick();
            if let Ok(frame) = host.rx.recv_held_timeout(Duration::from_millis(1)) {
                break frame;
            }
            assert!(std::time::Instant::now() < until);
        };
        assert!(frame.is_current());
        let reply: HttpReply = serde_json::from_slice(&frame.payload).unwrap();
        assert!(
            matches!(reply, HttpReply::Head { .. }),
            "expected head, got {}",
            describe(&reply)
        );
        let session = host.session.as_mut().unwrap();
        if mode == 0 {
            session
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
            session.live.occupancy.force_detach_workspace(7);
        }
        if mode == 2 {
            session
                .live
                .occupancy
                .acquire_workspace(8, &[], &[], 42)
                .unwrap();
            session.live.occupancy.add_workspace_member(8, 1, false);
        }
        assert!(
            !frame.is_current(),
            "queued body authority must end synchronously"
        );
        drop(frame);
        host.tick();
        assert!(
            host.session
                .as_ref()
                .unwrap()
                .remote
                .html_http
                .documents
                .is_empty()
        );
        fixture.release();
        assert_eq!(fixture.count(), 1);
    }
}

#[test]
fn rejected_document_replacement_preserves_the_accepted_post() {
    let fixture = Fixture::new();
    let url = fixture.url();
    let mut host = Host::new(&url);
    let response = host.commit(&url, &[]);
    let mut replacement = host.begin(&url, &[]);
    if let HttpRequest::Begin { id, .. } = &mut replacement {
        id.document += 1;
        // A new document cannot reuse either registration-scoped freshness cursor.
    }
    host.send(replacement);
    assert!(matches!(
        host.receive(),
        HttpReply::CommandRejected {
            code: wire::FailureCode::Stale,
            ..
        }
    ));
    assert!(
        host.session
            .as_ref()
            .unwrap()
            .remote
            .html_http
            .documents
            .contains_key(&key(41, id()))
    );
    host.send(HttpRequest::Credit {
        response,
        offset: 0,
        max_bytes: 16,
        allow_large: false,
    });
    fixture.release();
    assert!(
        matches!(host.receive(), HttpReply::Chunk { response: actual, .. } if actual == response)
    );
    assert_eq!(fixture.count(), 1);
}
