use super::*;
use tasty_ipc::stream_hub::{SinkReceiver, StreamInbound};
fn id() -> TransferId {
    TransferId {
        epoch: 1,
        surface_id: 1,
        binding: 8,
        document: 1,
        request: 1,
        transfer: 1,
    }
}
fn capabilities(hub: &StreamHub, client: u32) {
    let (tx, rx) = mpsc::channel();
    tx.send(StreamInbound::Frame {
        client_id: client,
        frame: tasty_ipc::stream::StreamFrame::new(
            tasty_ipc::stream::StreamTag::Control,
            serde_json::to_vec(&HtmlRequest::HtmlCapabilities {
                capabilities: vec![tasty_ipc::html_transfer::CAPABILITY.into()],
            })
            .unwrap(),
        ),
    })
    .unwrap();
    hub.pump_inbound(&rx);
}
fn reply(rx: &SinkReceiver) -> HtmlReply {
    let frame = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(frame.payload.len() <= tasty_ipc::stream::MAX_FRAME_LEN as usize);
    serde_json::from_slice(&frame.payload).unwrap()
}
struct Fixture {
    hub: StreamHub,
    rx: SinkReceiver,
    tx: SyncSender<HtmlRequest>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    source: Arc<Mutex<tasty_remote::html_source::HtmlSourceState>>,
}
impl Fixture {
    fn new(path: &std::path::Path) -> Self {
        Self::url(path.to_string_lossy().as_ref(), Duration::from_secs(30))
    }
    fn url(url: &str, timeout: Duration) -> Self {
        let hub = StreamHub::new();
        let rx = hub.register(41);
        capabilities(&hub, 41);
        let source = Arc::new(Mutex::new(
            tasty_remote::html_source::HtmlSourceState::default(),
        ));
        source.lock().unwrap().set(Some(url), None);
        let typed = source.lock().unwrap().source.clone().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, commands) = mpsc::sync_channel(1);
        let context = Context {
            hub: hub.clone(),
            client: 41,
            binding: hub.client_binding(41).unwrap(),
            source_state: Arc::downgrade(&source),
            generation: 1,
            cancel: cancel.clone(),
            id: id(),
            idle_timeout: timeout,
            lease: resources::Lease::entry(),
        };
        let thread = std::thread::spawn(move || context.run(typed, None, commands));
        Self {
            hub,
            rx,
            tx,
            cancel,
            thread: Some(thread),
            source,
        }
    }
    fn read(&self, offset: u64, allow: bool) {
        self.tx
            .send(HtmlRequest::HtmlRead {
                id: id(),
                offset,
                allow,
            })
            .unwrap();
    }
    fn join(&mut self) {
        self.thread.take().unwrap().join().unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
#[test]
fn metadata_first_large_file_exact_bytes_and_bounded_frames() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.html");
    let expected: Vec<u8> = (0..1_500_000).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &expected).unwrap();
    let mut f = Fixture::new(&path);
    assert!(matches!(
        reply(&f.rx),
        HtmlReply::HtmlMetadata {
            metadata: HtmlMetadata {
                size: 1_500_000,
                regular: true,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        f.rx.recv_timeout(Duration::from_millis(30)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let mut bytes = Vec::new();
    let mut chunks = 0;
    loop {
        f.read(bytes.len() as u64, true);
        match reply(&f.rx) {
            HtmlReply::HtmlChunk {
                offset, data_b64, ..
            } => {
                assert_eq!(offset, bytes.len() as u64);
                bytes.extend(
                    base64::engine::general_purpose::STANDARD
                        .decode(data_b64)
                        .unwrap(),
                );
                chunks += 1;
            }
            other => panic!("{other:?}"),
        };
        if bytes.len() == expected.len() {
            break;
        }
    }
    assert!(matches!(
        reply(&f.rx),
        HtmlReply::HtmlComplete {
            size: 1_500_000,
            ..
        }
    ));
    assert_eq!(chunks, 31);
    assert_eq!(Sha256::digest(&bytes), Sha256::digest(&expected));
    f.join();
}
#[test]
fn large_without_allowance_fails_and_source_replacement_invalidates_allowance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.html");
    std::fs::write(&path, vec![b'a'; 100_001]).unwrap();
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    f.read(0, false);
    assert!(matches!(reply(&f.rx), HtmlReply::HtmlFailure { .. }));
    f.join();
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    let replacement = dir.path().join("b.html");
    std::fs::write(&replacement, vec![b'b'; 100_001]).unwrap();
    std::fs::rename(replacement, &path).unwrap();
    f.read(0, true);
    assert!(matches!(reply(&f.rx), HtmlReply::HtmlFailure { .. }));
    f.join();
}
#[test]
fn cancel_generation_close_timeout_and_reused_connection_release_owned_worker() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.html");
    std::fs::write(&path, b"abc").unwrap();
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    f.cancel.store(true, Ordering::Release);
    assert!(matches!(reply(&f.rx), HtmlReply::HtmlCancelled { .. }));
    f.join();
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    f.source
        .lock()
        .unwrap()
        .set(Some("https://remote.example/"), None);
    assert!(matches!(reply(&f.rx), HtmlReply::HtmlCancelled { .. }));
    f.join();
    let mut f = Fixture::url(path.to_str().unwrap(), Duration::from_millis(10));
    reply(&f.rx);
    assert!(
        matches!(reply(&f.rx),HtmlReply::HtmlFailure {reason,..} if reason.contains("timeout"))
    );
    f.join();
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    let new = f.hub.register(41);
    f.join();
    assert!(matches!(
        new.recv_timeout(Duration::from_millis(30)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let mut f = Fixture::new(&path);
    reply(&f.rx);
    let original = std::mem::take(&mut f.source);
    drop(original);
    f.hub.unregister(41);
    f.join();
}
#[test]
fn expired_size_question_is_served_by_a_fresh_allowed_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.html");
    let expected = vec![b'z'; 300_000];
    std::fs::write(&path, &expected).unwrap();
    let mut f = Fixture::url(path.to_str().unwrap(), Duration::from_millis(10));
    assert!(matches!(reply(&f.rx), HtmlReply::HtmlMetadata { .. }));
    assert!(
        matches!(reply(&f.rx),HtmlReply::HtmlFailure {reason,..} if reason.contains("timeout"))
    );
    f.join();
    // The client keeps the question; its consent starts a new worker that reads to the end.
    let mut f = Fixture::new(&path);
    assert!(matches!(
        reply(&f.rx),
        HtmlReply::HtmlMetadata {
            metadata: HtmlMetadata { size: 300_000, .. },
            ..
        }
    ));
    let mut received = 0u64;
    loop {
        f.read(received, true);
        match reply(&f.rx) {
            HtmlReply::HtmlChunk { data_b64, .. } => {
                received += base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .unwrap()
                    .len() as u64;
            }
            other => panic!("{other:?}"),
        }
        if received == expected.len() as u64 {
            break;
        }
    }
    assert!(matches!(
        reply(&f.rx),
        HtmlReply::HtmlComplete { size: 300_000, .. }
    ));
    f.join();
}
#[test]
fn http_metadata_is_typed_and_never_read_as_file() {
    let mut f = Fixture::url("http://127.0.0.1:9/not-a-file", Duration::from_secs(1));
    assert!(matches!(
        reply(&f.rx),
        HtmlReply::HtmlMetadata {
            metadata: HtmlMetadata {
                source: HtmlSource::Http { .. },
                regular: false,
                root: None,
                ..
            },
            ..
        }
    ));
    assert!(matches!(reply(&f.rx),HtmlReply::HtmlFailure {reason,..} if reason.contains("proxy")));
    f.join();
}
#[test]
fn source_create_restore_requested_url_and_native_commit_are_distinct() {
    // Source snapshots are pure path values; no fixture file is opened in this test.
    let parent = std::path::PathBuf::from(if cfg!(windows) {
        r"C:\remote-html-source"
    } else {
        "/remote-html-source"
    });
    let first = parent.join("first.html");
    let restored_path = parent.join("restored.html");
    let rs = RemoteSurface::new(1, "html", "com.tasty.html".into(), "html".into())
        .with_capabilities(
            crate::runtime::surface_registry::KindCapabilities::undeclared(
                "html",
                "com.tasty.html",
            ),
        )
        .with_initial_file(&serde_json::json!({"url":first}));
    assert_eq!(
        rs.html_source_info().0,
        tasty_remote::html_source::classify(first.to_str(), None)
    );
    assert_eq!(rs.source_cwd(), Some(parent.clone()));
    let restored = RemoteSurface::new(2, "html", "com.tasty.html".into(), "html".into())
        .with_capabilities(
            crate::runtime::surface_registry::KindCapabilities::undeclared(
                "html",
                "com.tasty.html",
            ),
        );
    restored.cache_snapshot(serde_json::json!({"url":restored_path}));
    assert_eq!(restored.source_cwd(), Some(parent));
    assert_eq!(
        restored.html_source_info().0,
        tasty_remote::html_source::classify(restored_path.to_str(), None)
    );
    restored.set_webview_url(Some("https://localhost/new".into()), None, true);
    let requested = restored.html_source_info();
    restored.cache_snapshot(serde_json::json!({"url":first}));
    assert_eq!(restored.html_source_info(), requested);
    #[cfg(feature = "gui")]
    {
        restored.set_webview_nav(tasty_model::WebViewNavInfo::default());
        assert_eq!(restored.html_source_info(), requested);
    }
    restored.set_webview_url(Some("plain text body".into()), None, true);
    assert_eq!(restored.html_source_info().0, HtmlSource::Inline);
    assert_eq!(restored.source_cwd(), None);
}

#[test]
fn process_worker_budget_survives_engine_retirement_until_actual_join() {
    let permit = WorkerPermit::acquire().unwrap();
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::spawn(move || rx.recv().unwrap());
    crate::poison::recover_mutex(RETIRED.lock(), "retired HTML workers", &RETIRED_POISONED)
        .push((thread, permit));
    let slots: Vec<_> = (0..7).map(|_| WorkerPermit::acquire().unwrap()).collect();
    assert!(WorkerPermit::acquire().is_none());
    tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while WORKER_COUNT.load(Ordering::Acquire) == 8 && Instant::now() < deadline {
        reap_retired();
        std::thread::yield_now();
    }
    assert_eq!(WORKER_COUNT.load(Ordering::Acquire), 7);
    let extra = WorkerPermit::acquire().unwrap();
    drop(extra);
    drop(slots);
    assert_eq!(WORKER_COUNT.load(Ordering::Acquire), 0);
}

#[test]
fn resource_lease_reuses_root_and_cancel_fences_every_child() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.html");
    std::fs::write(&index, b"entry").unwrap();
    std::fs::write(dir.path().join("style.css"), b"body{color:blue}").unwrap();
    let lease = resources::Lease::entry();
    let source = tasty_remote::html_source::classify(index.to_str(), None);
    let (_, _) = lease.open(index.to_str().unwrap(), source.clone()).unwrap();
    assert!(lease.unchanged());
    let mut child = lease.clone();
    child.set_test_relative("style.css");
    let (resource, metadata_source) = child.open(index.to_str().unwrap(), source.clone()).unwrap();
    assert_eq!(resource.file.metadata().unwrap().len(), 16);
    assert!(matches!(metadata_source,HtmlSource::File{path,..} if path.ends_with("style.css")));
    lease.cancel.store(true, Ordering::Release);
    assert!(!child.unchanged());
    assert!(child.open(index.to_str().unwrap(), source).is_err());
}

#[cfg(feature = "gui")]
#[test]
fn resource_registry_fences_parent_source_binding_and_replacement() {
    let hub = StreamHub::new();
    let _receiver = hub.register(41);
    let (_state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    engine
        .live
        .occupancy
        .acquire_workspace(1, &[], &[], 41)
        .unwrap();
    let owner = TransferOwner::capture(&engine.as_ref(), &hub, 41, false).unwrap();
    let source = Arc::new(Mutex::new(
        tasty_remote::html_source::HtmlSourceState::default(),
    ));
    let mut roots = resources::Roots::default();
    let entry = id();
    let child = TransferId {
        request: 2,
        transfer: 2,
        ..entry
    };
    let lease = roots
        .lease(
            41,
            entry,
            &HtmlRequest::HtmlEntryRequest { id: entry },
            owner.clone(),
            Arc::downgrade(&source),
            1,
        )
        .unwrap();
    let request = HtmlRequest::HtmlResourceRequest {
        id: child,
        entry,
        source_generation: 1,
        relative: "theme.css".into(),
        query: None,
    };
    assert!(
        roots
            .lease(
                41,
                child,
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                1
            )
            .is_ok()
    );
    assert!(
        roots
            .lease(
                41,
                child,
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                2
            )
            .is_err()
    );
    assert!(
        roots
            .lease(
                41,
                TransferId {
                    binding: 9,
                    ..child
                },
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                1
            )
            .is_err()
    );
    let replaced_source = Arc::new(Mutex::new(
        tasty_remote::html_source::HtmlSourceState::default(),
    ));
    assert!(
        roots
            .lease(
                41,
                child,
                &request,
                owner.clone(),
                Arc::downgrade(&replaced_source),
                1
            )
            .is_err()
    );
    roots.cancel_entry(41, child);
    assert!(!lease.cancel.load(Ordering::Acquire));
    roots.cancel_entry(41, entry);
    assert!(lease.cancel.load(Ordering::Acquire));
    assert!(
        roots
            .lease(
                41,
                child,
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                1
            )
            .is_err()
    );
    let replacement = TransferId {
        document: 2,
        request: 3,
        transfer: 3,
        ..entry
    };
    let fresh = roots
        .lease(
            41,
            replacement,
            &HtmlRequest::HtmlEntryRequest { id: replacement },
            owner.clone(),
            Arc::downgrade(&source),
            1,
        )
        .unwrap();
    assert!(
        roots
            .lease(41, child, &request, owner, Arc::downgrade(&source), 1)
            .is_err()
    );
    drop(roots);
    assert!(fresh.cancel.load(Ordering::Acquire));
}

#[cfg(feature = "gui")]
#[test]
fn navigation_retires_old_children_and_reload_reauthorizes_current_root() {
    let hub = StreamHub::new();
    let _rx = hub.register(41);
    let (_state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    engine
        .live
        .occupancy
        .acquire_workspace(1, &[], &[], 41)
        .unwrap();
    let owner = TransferOwner::capture(&engine.as_ref(), &hub, 41, false).unwrap();
    let source = Arc::new(Mutex::new(
        tasty_remote::html_source::HtmlSourceState::default(),
    ));
    let mut roots = resources::Roots::default();
    let first = id();
    let original = roots
        .lease(
            41,
            first,
            &HtmlRequest::HtmlEntryRequest { id: first },
            owner.clone(),
            Arc::downgrade(&source),
            1,
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.html");
    std::fs::write(&path, b"original").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/next.html"), b"next").unwrap();
    let typed = tasty_remote::html_source::classify(path.to_str(), None);
    original
        .open(path.to_str().unwrap(), typed.clone())
        .unwrap();
    let next = TransferId {
        document: 2,
        request: 2,
        transfer: 2,
        ..first
    };
    let request = HtmlRequest::HtmlNavigateRequest {
        id: next,
        entry: first,
        source_generation: 1,
        relative: "sub/next.html".into(),
        query: Some("q=1".into()),
        fragment: Some("anchor".into()),
    };
    assert!(
        roots
            .lease(
                41,
                first,
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                1
            )
            .is_err()
    );
    let moved = roots
        .lease(
            41,
            next,
            &request,
            owner.clone(),
            Arc::downgrade(&source),
            1,
        )
        .unwrap();
    assert!(original.cancel.load(Ordering::Acquire));
    assert!(
        original
            .open(path.to_str().unwrap(), typed.clone())
            .is_err()
    );
    let (file, provenance) = moved.open(path.to_str().unwrap(), typed.clone()).unwrap();
    assert_eq!(file.file.metadata().unwrap().len(), 4);
    assert!(
        matches!(provenance,HtmlSource::File { url,.. } if url.ends_with("/sub/next.html?q=1#anchor"))
    );
    let metadata = moved.file_metadata(path.to_str().unwrap()).unwrap();
    assert_eq!(metadata.relative, "sub/next.html");
    assert_eq!(metadata.mime, "text/html");
    assert!(
        roots
            .lease(
                41,
                next,
                &request,
                owner.clone(),
                Arc::downgrade(&source),
                1
            )
            .is_err()
    );
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, b"replaced original").unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert!(!moved.unchanged());
    let reload = TransferId {
        document: 3,
        request: 3,
        transfer: 3,
        ..first
    };
    let request = HtmlRequest::HtmlFileEntryRequest {
        id: reload,
        relative: "sub/next.html".into(),
        query: None,
        fragment: None,
    };
    let fresh = roots
        .lease(41, reload, &request, owner, Arc::downgrade(&source), 1)
        .unwrap();
    assert!(moved.cancel.load(Ordering::Acquire));
    assert!(fresh.open(path.to_str().unwrap(), typed).is_ok());
    assert!(fresh.unchanged());
}
