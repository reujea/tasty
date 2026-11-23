//! Generated key/SPKI and actual local TLS wire controls; no WebKit challenge or upstream proof.
use super::*;
use std::time::Duration;
use tasty_ipc::html_transfer::TransferId;
use tasty_remote::{
    connection::channel, html_http_body::BodyBudget, html_http_client::*,
    html_http_session::CapturedDocument,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn current(document: &Arc<CapturedDocument>) -> Arc<dyn Fn() -> bool + Send + Sync> {
    let owner = Arc::downgrade(document);
    Arc::new(move || owner.upgrade().is_some_and(|owner| owner.is_current()))
}
fn document(generation: u64) -> Arc<CapturedDocument> {
    let (sender, _rx) = channel();
    CapturedDocument::new(
        DocumentKey {
            entry: TransferId {
                epoch: 1,
                surface_id: 1,
                binding: 1,
                document: generation,
                request: 1,
                transfer: 1,
            },
            source_generation: 1,
            entry_url: "https://mirror.example/".into(),
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
        PendingBudget::new(1_000_000),
        BodyBudget::new(1_000_000),
        Arc::new(|| true),
        Arc::new(|| {}),
    )
    .unwrap_or_else(|_| panic!("document"))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn document_and_authority_get_genuinely_fresh_keys_and_same_authority_reuses_one_owner() {
    let document = document(1);
    let map = Certificates::new(
        current(&document),
        CertificateBudget::new(),
        Handle::current(),
    );
    let authority = Authority::new("Mirror.Example", 443).unwrap();
    let (first, same) = tokio::join!(
        map.certificate(authority.clone()),
        map.certificate(authority.clone())
    );
    let first = first.unwrap_or_else(|_| panic!("first certificate"));
    let same = same.unwrap_or_else(|_| panic!("same certificate"));
    assert!(Arc::ptr_eq(&first, &same));
    assert_eq!(map.expected_leaf("mirror.example", 443), Some(first.leaf()));
    let other_port = map
        .certificate(Authority::new("mirror.example", 8443).unwrap())
        .await
        .unwrap_or_else(|_| panic!("other port"));
    assert_ne!(first.leaf(), other_port.leaf());
    assert_ne!(first.spki(), other_port.spki());
    let next = document_for_next();
    let next_map = Certificates::new(current(&next), CertificateBudget::new(), Handle::current());
    let changed = next_map
        .certificate(authority)
        .await
        .unwrap_or_else(|_| panic!("next document"));
    assert_ne!(first.leaf(), changed.leaf());
    assert_ne!(first.spki(), changed.spki());
    document.retire();
    assert!(map.expected_leaf("mirror.example", 443).is_none());
    assert!(matches!(
        map.certificate(Authority::new("mirror.example", 443).unwrap())
            .await,
        Err(CertificateError::Retired)
    ));
    assert!(next_map.current());
}
fn document_for_next() -> Arc<CapturedDocument> {
    document(2)
}
#[tokio::test]
async fn retired_generation_and_invalid_authority_never_publish_a_pin() {
    let document = document(1);
    let map = Certificates::new(
        current(&document),
        CertificateBudget::new(),
        Handle::current(),
    );
    for (host, port) in [
        ("", 443),
        ("*.example", 443),
        ("example", 0),
        ("user@example", 443),
    ] {
        assert!(Authority::new(host, port).is_err());
    }
    assert_eq!(Authority::new("[::1]", 8443).unwrap().host(), "::1");
    map.retire();
    assert!(!map.current());
    assert!(map.expected_leaf("mirror.example", 443).is_none());
    assert!(matches!(
        map.certificate(Authority::new("mirror.example", 443).unwrap())
            .await,
        Err(CertificateError::Retired)
    ));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_tls_socket_presents_the_generated_mapped_leaf_with_original_sni() {
    use tokio_rustls::{TlsConnector, rustls};
    let document = document(1);
    let map = Certificates::new(
        current(&document),
        CertificateBudget::new(),
        Handle::current(),
    );
    let leaf = map
        .certificate(Authority::new("mirror.example", 443).unwrap())
        .await
        .unwrap_or_else(|_| panic!("certificate"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let tls = leaf.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = tls.accept(stream, Duration::from_secs(2)).await.unwrap();
        assert_eq!(stream.server_name(), Some("mirror.example"));
        let mut byte = [0];
        stream.read_exact(&mut byte).await.unwrap();
        assert_eq!(byte, [7]);
        stream.write_all(&[9]).await.unwrap();
    });
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(leaf.der.clone()))
        .unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let mut socket = connector
        .connect(
            rustls::pki_types::ServerName::try_from("mirror.example").unwrap(),
            socket,
        )
        .await
        .unwrap();
    let actual = socket
        .get_ref()
        .1
        .peer_certificates()
        .unwrap()
        .first()
        .unwrap();
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(actual)),
        map.expected_leaf("mirror.example", 443).unwrap()
    );
    socket.write_all(&[7]).await.unwrap();
    let mut byte = [0];
    socket.read_exact(&mut byte).await.unwrap();
    assert_eq!(byte, [9]);
    drop(socket);
    server.await.unwrap();
}

#[test]
fn old_document_jobs_keep_shared_capacity_until_actual_job_drop() {
    use std::sync::{Condvar, Mutex, mpsc};
    struct Release(Arc<(Mutex<bool>, Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            let (lock, ready) = &*self.0;
            *lock.lock().unwrap() = true;
            ready.notify_all();
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let fence = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Release(fence.clone());
    let (entered, waiting) = mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        let (lock, ready) = &*fence;
        let mut open = lock.lock().unwrap();
        entered.send(()).unwrap();
        while !*open {
            open = ready.wait(open).unwrap();
        }
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    runtime.block_on(async {
        let old_document = document(1);
        let budget = CertificateBudget::new();
        let old = Certificates::new(current(&old_document), budget.clone(), Handle::current());
        let mut waiters = Vec::new();
        for port in 4400..4408 {
            let map = old.clone();
            waiters.push(tokio::spawn(async move {
                map.certificate(Authority::new("mirror.example", port).unwrap())
                    .await
            }));
        }
        let wait_until = std::time::Instant::now() + Duration::from_secs(2);
        // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
        loop {
            if budget.generators() == 8 {
                break;
            }
            if std::time::Instant::now() >= wait_until {
                assert!(
                    budget.generators() == 8,
                    "not reached within {:?}: budget.generators() == 8",
                    Duration::from_secs(2)
                );
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(budget.reserved(), 8 * CERTIFICATE_BYTES);
        let next_document = document(2);
        let next = Certificates::new(current(&next_document), budget.clone(), Handle::current());
        assert!(matches!(
            next.certificate(Authority::new("mirror.example", 443).unwrap())
                .await,
            Err(CertificateError::Capacity)
        ));
        assert!(next.slots.lock().unwrap().is_empty());
        assert_eq!(budget.reserved(), 8 * CERTIFICATE_BYTES);
        old.retire();
        for waiter in waiters {
            assert!(matches!(
                waiter.await.unwrap(),
                Err(CertificateError::Retired)
            ));
        }
        assert_eq!(
            budget.generators(),
            8,
            "retired futures cannot release queued crypto-job permits"
        );
        drop(release);
        tokio::time::timeout(Duration::from_secs(2), old.settled())
            .await
            .unwrap();
        assert_eq!(budget.generators(), 0);
        drop(old);
        assert_eq!(budget.reserved(), 0);
        let retained = next
            .certificate(Authority::new("mirror.example", 443).unwrap())
            .await
            .unwrap_or_else(|_| panic!("capacity restored after old jobs ended"));
        next.retire();
        next.settled().await;
        drop(next);
        assert_eq!(
            budget.reserved(),
            CERTIFICATE_BYTES,
            "native-held owner retains its certificate reservation"
        );
        drop(retained);
        assert_eq!(budget.reserved(), 0);
    });
    runtime.block_on(blocker).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_owned_tls_stream_keeps_key_lease_and_stops_raw_writes_after_retirement() {
    use std::{
        pin::Pin,
        sync::atomic::{AtomicBool, AtomicUsize},
        task::{Context, Poll},
    };
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_rustls::{TlsConnector, rustls};
    struct Socket {
        io: tokio::net::TcpStream,
        live: Arc<AtomicBool>,
        after: Arc<AtomicUsize>,
    }
    impl AsyncRead for Socket {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.io).poll_read(cx, buf)
        }
    }
    impl AsyncWrite for Socket {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            if !self.live.load(Ordering::Acquire) {
                self.after.fetch_add(1, Ordering::AcqRel);
            }
            Pin::new(&mut self.io).poll_write(cx, bytes)
        }
        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.io).poll_flush(cx)
        }
        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.io).poll_shutdown(cx)
        }
    }
    let document = document(1);
    let budget = CertificateBudget::new();
    let map = Certificates::new(current(&document), budget.clone(), Handle::current());
    let leaf = map
        .certificate(Authority::new("mirror.example", 443).unwrap())
        .await
        .unwrap_or_else(|_| panic!("certificate"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let live = Arc::new(AtomicBool::new(true));
    let after = Arc::new(AtomicUsize::new(0));
    let signal = live.clone();
    let writes = after.clone();
    let tls = leaf.clone();
    let (resume, wait) = tokio::sync::oneshot::channel();
    // The [7] record can reach the client before flush runs, so retirement waits for flushed.
    let (flushed, written) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = tls
            .accept(
                Socket {
                    io: stream,
                    live: signal,
                    after: writes,
                },
                Duration::from_secs(2),
            )
            .await
            .unwrap_or_else(|_| panic!("TLS accept"));
        drop(tls);
        stream.write_all(&[7]).await.unwrap();
        stream.flush().await.unwrap();
        flushed.send(()).unwrap();
        wait.await.unwrap();
        let error = stream.write_all(&[99]).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionAborted);
        drop(stream);
    });
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(leaf.der.clone()))
        .unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let mut socket = connector
        .connect(
            rustls::pki_types::ServerName::try_from("mirror.example").unwrap(),
            socket,
        )
        .await
        .unwrap();
    let mut byte = [0];
    socket.read_exact(&mut byte).await.unwrap();
    assert_eq!(byte, [7]);
    written.await.unwrap();
    drop(leaf);
    map.retire();
    map.settled().await;
    drop(map);
    live.store(false, Ordering::Release);
    assert_eq!(budget.reserved(), CERTIFICATE_BYTES);
    resume.send(()).unwrap();
    server.await.unwrap();
    assert_eq!(after.load(Ordering::Acquire), 0);
    assert_eq!(budget.reserved(), 0);
    let mut late = Vec::new();
    let result = socket.read_to_end(&mut late).await;
    if let Err(error) = result {
        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
    }
    assert!(late.is_empty());
}
