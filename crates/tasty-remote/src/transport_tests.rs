use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use super::*;

#[test]
fn closing_a_dead_connection_lets_the_peer_see_the_end_at_once() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let workers = ConnectionWorkers::new(ours, Vec::new());

    workers.close_socket();
    let mut buf = [0u8; 1];
    assert_eq!(
        peer.read(&mut buf)
            .expect("the peer reads the end, not a timeout"),
        0
    );
    drop(workers);
}

#[test]
fn reader_receipt_finishes_while_delta_is_restored_ahead_of_data() {
    use std::io::{BufRead, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let handshake = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        stream::write_frame(
            &mut peer,
            StreamTag::Control,
            &serde_json::to_vec(&serde_json::json!({"ok":true,"proto":1,"client_id":7})).unwrap(),
        )
        .unwrap();
        peer
    });
    let (conn, _) = StreamConnection::open(ours, 1).unwrap();
    let mut peer = handshake.join().unwrap();
    let (sender, _queue) = super::super::connection::channel();
    sender.enable_input_receipts(true);
    sender.user_input_dispatched(std::time::Instant::now());
    let output = MirrorOutbox::new(sender.epoch());
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let wake = Arc::new(move || {
        wake_tx.send(()).expect("test receiver remains alive");
    });
    let reader = spawn_attach_reader_thread(
        conn,
        output.clone(),
        sender.clone(),
        wake,
        |_| {
            Some(MirrorEvent::StructuralDelta {
                workspace_id: 1,
                tree: serde_json::json!({}),
                surfaces: Vec::new(),
            })
        },
        AttachReaderEnd {
            remote_workspace: 1,
            heartbeat_timeout: stream::HEARTBEAT_TIMEOUT,
            detach_sent: Arc::new(AtomicBool::new(false)),
            transport_end: Arc::new(TransportEnd::new()),
            finished: Arc::new(AtomicBool::new(false)),
            gate: None,
            probe: None,
        },
    );
    stream::write_frame(&mut peer, StreamTag::Control, b"{}").unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    // The application cannot yet reserve IDs, so its Delta remains at the front.
    output.restore_front(output.drain());
    stream::write_frame(
        &mut peer,
        StreamTag::Control,
        &serde_json::to_vec(&StreamControl::InputProcessed { sequence: 1 }).unwrap(),
    )
    .unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(sender.oldest_unprocessed_input().is_none());
    stream::write_frame(&mut peer, StreamTag::Data, &stream::encode_mux(9, b"after")).unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let events = output.drain();
    assert!(
        matches!(&events[..], [MirrorEvent::StructuralDelta { .. }, MirrorEvent::Data(9, data)] if data == b"after")
    );
    peer.shutdown(std::net::Shutdown::Both).unwrap();
    reader.join().unwrap();
}

#[test]
fn html_transport_is_declared_only_to_servers_advertising_it_and_never_as_viewer() {
    use std::io::{BufRead, BufReader};
    use tasty_ipc::html_transfer::{
        CAPABILITY, HtmlRequest, RESOURCE_CAPABILITY, VIEWER_CAPABILITY,
    };
    for capabilities in [
        None,
        Some(vec![CAPABILITY]),
        Some(vec![VIEWER_CAPABILITY]),
        Some(vec![CAPABILITY, VIEWER_CAPABILITY]),
        Some(vec![CAPABILITY, VIEWER_CAPABILITY, RESOURCE_CAPABILITY]),
    ] {
        let enabled = capabilities
            .as_ref()
            .is_some_and(|v| v.contains(&CAPABILITY));
        let resources = capabilities
            .as_ref()
            .is_some_and(|v| v.contains(&RESOURCE_CAPABILITY));
        let entry = capabilities
            .as_ref()
            .is_some_and(|v| v.contains(&VIEWER_CAPABILITY));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            stream::write_frame(
                &mut socket,
                StreamTag::Control,
                &serde_json::to_vec(&serde_json::json!({"ok":true,"proto":1,"client_id":7}))
                    .unwrap(),
            )
            .unwrap();
            let mut descriptor = serde_json::json!({"event":"attached_workspace","name":"remote","tree":null,"surfaces":[{"remote_id":1,"role":"placeholder","kind":"html"}]});
            if let Some(capabilities) = capabilities {
                descriptor[stream::DESCRIPTOR_CAPABILITIES] = serde_json::json!(capabilities);
            }
            stream::write_frame(
                &mut socket,
                StreamTag::Control,
                &serde_json::to_vec(&descriptor).unwrap(),
            )
            .unwrap();
            let loss = stream::read_frame(&mut reader).unwrap();
            assert!(matches!(
                serde_json::from_slice::<StreamControl>(&loss.payload).unwrap(),
                StreamControl::ClientLossNotify {}
            ));
            let effects = stream::read_frame(&mut reader).unwrap();
            assert!(matches!(
                serde_json::from_slice::<StreamControl>(&effects.payload).unwrap(),
                StreamControl::ClientTerminalEffects {}
            ));
            let next = stream::read_frame(&mut reader);
            if enabled {
                let HtmlRequest::HtmlCapabilities { capabilities } =
                    serde_json::from_slice(&next.unwrap().payload).unwrap()
                else {
                    panic!("expected HTML transport declaration")
                };
                assert_eq!(capabilities, vec![CAPABILITY]);
                assert!(!capabilities.iter().any(|c| c == VIEWER_CAPABILITY));
            } else {
                assert!(
                    next.is_err(),
                    "no declaration to an old or viewer-only peer"
                );
            }
        });
        let mut remote = super::super::outbound::Remote::new();
        let token = remote.begin_attempt(None, None).unwrap();
        let handshake = attach_handshake(
            port,
            1,
            "HTML compatibility test",
            &token,
            stream::HEARTBEAT_TIMEOUT,
        )
        .unwrap();
        assert_eq!(handshake.html.server_transfer, enabled);
        assert_eq!(handshake.html.server_entry, entry);
        assert_eq!(handshake.html.server_resources, resources);
        drop(handshake);
        remote.cancel_attempt(&token);
        peer.join().unwrap();
    }
}

/// `StreamConnection`의 handshake가 소켓 timeout을 다시 걸므로, 연결을 연 뒤의 값을 확인한다.
#[test]
fn the_heartbeat_timeout_is_still_on_the_socket_after_the_handshake() {
    use std::io::{BufRead, BufReader, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        let mut writer = sock.try_clone().unwrap();
        let mut request = String::new();
        BufReader::new(sock).read_line(&mut request).unwrap();
        stream::write_frame(
            &mut writer,
            StreamTag::Control,
            &serde_json::to_vec(&serde_json::json!({"ok":true,"proto":1,"client_id":7})).unwrap(),
        )
        .unwrap();
        stream::write_frame(
            &mut writer,
            StreamTag::Control,
            &serde_json::to_vec(&serde_json::json!({"event":"attached_workspace"})).unwrap(),
        )
        .unwrap();
        writer.flush().unwrap();
        writer
    });
    let mut remote = super::super::outbound::Remote::new();
    let token = remote.begin_attempt(None, None).unwrap();
    let configured = Duration::from_secs(17);
    let handshake =
        attach_handshake(port, 1, "heartbeat setting test", &token, configured).unwrap();
    assert_eq!(
        handshake.write_half.read_timeout().unwrap(),
        Some(configured)
    );
    assert_eq!(
        handshake.write_half.write_timeout().unwrap(),
        Some(configured)
    );
    drop(handshake);
    remote.cancel_attempt(&token);
    drop(peer.join().unwrap());
}

/// 서버 역할의 loopback peer를 띄워 handshake ack까지 보낸 연결을 돌려준다.
fn acked_stream(timeout: Duration) -> (StreamConnection, TcpStream) {
    use std::io::{BufRead, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let handshake = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        stream::write_frame(
            &mut peer,
            StreamTag::Control,
            &serde_json::to_vec(&serde_json::json!({"ok":true,"proto":1,"client_id":7})).unwrap(),
        )
        .unwrap();
        peer
    });
    let (conn, _) =
        StreamConnection::open_attach_workspace_with_timeout(ours, 1, 1, timeout).unwrap();
    (conn, handshake.join().unwrap())
}

#[test]
fn a_silent_peer_past_the_read_timeout_reads_as_a_heartbeat_timeout() {
    let (mut conn, _peer) = acked_stream(Duration::from_millis(200));
    let error = conn
        .recv()
        .expect_err("the silent peer trips the read timeout");
    assert_eq!(
        stream::ReadFailure::of_error(&error),
        stream::ReadFailure::HeartbeatTimeout,
        "{error}"
    );
}

#[test]
fn a_peer_that_closes_the_socket_reads_as_a_peer_close() {
    let (mut conn, peer) = acked_stream(Duration::from_secs(5));
    drop(peer);
    let error = conn.recv().expect_err("the closed peer ends the stream");
    assert_eq!(
        stream::ReadFailure::of_error(&error),
        stream::ReadFailure::PeerClosed,
        "{error}"
    );
}

fn reader_end() -> AttachReaderEnd {
    AttachReaderEnd {
        remote_workspace: 513,
        heartbeat_timeout: Duration::from_secs(45),
        detach_sent: Arc::new(AtomicBool::new(false)),
        transport_end: Arc::new(TransportEnd::new()),
        finished: Arc::new(AtomicBool::new(false)),
        gate: None,
        probe: None,
    }
}

/// 시험용 reader 끝 정보. `end`와 `finished`를 시험이 관찰한다.
fn reader_link(end: &Arc<TransportEnd>, finished: &Arc<AtomicBool>) -> AttachReaderEnd {
    AttachReaderEnd {
        remote_workspace: 1,
        heartbeat_timeout: stream::HEARTBEAT_TIMEOUT,
        detach_sent: Arc::new(AtomicBool::new(false)),
        transport_end: end.clone(),
        finished: finished.clone(),
        gate: None,
        probe: None,
    }
}

/// 시험용 쓰기 스레드 끝 정보. 이음새는 비워 둔다.
fn writer_link(
    end: &Arc<TransportEnd>,
    finished: &Arc<AtomicBool>,
    cap: Duration,
) -> AttachWriterEnd {
    AttachWriterEnd {
        transport_end: end.clone(),
        detach_sent: Arc::new(AtomicBool::new(false)),
        reader_finished: finished.clone(),
        reader_wait_cap: cap,
        seams: WriterSeams::default(),
    }
}

/// 이쪽이 먼저 사유를 올린(정리·쓰기 실패) 뒤의 수신 오류는 원격 원인이 아니라 경고하지 않는다.
#[test]
fn a_read_error_after_our_own_disconnect_is_not_a_warning() {
    let end = reader_end();
    end.transport_end.raise(EndReason::Lost);
    let eof = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    let (warn, message) = end.outcome(&eof);
    assert!(!warn, "{message}");
    assert!(message.contains("PeerClosed"), "{message}");
    assert!(end.transport_end.is_ending());
}

/// 정리 경로: 로컬 해제가 사유를 올리고 소켓을 닫은 뒤 큐에 남은 Detach 의 쓰기 실패는 예상한 실패라
/// debug 다. 사유가 없던 연결의 쓰기 실패는 실제 쓰기 장애라 경고다.
#[test]
fn a_write_failure_after_a_local_release_is_not_a_warning() {
    let broken = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
    let open = TransportEnd::new();
    let (warn, message) = write_failure_notice(&open, &broken, "");
    assert!(warn, "{message}");
    let released = TransportEnd::new();
    released.raise(EndReason::Lost);
    assert!(released.settle());
    let (warn, message) = write_failure_notice(&released, &broken, "");
    assert!(!warn, "{message}");
}

/// 처음 끊김을 알리는 수신 오류는 원인과 실제로 건 timeout을 경고로 남기고 끊김을 세운다.
#[test]
fn the_first_read_error_warns_with_its_cause_and_raises_the_disconnect() {
    let end = reader_end();
    let timeout = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::WouldBlock));
    let (warn, message) = end.outcome(&timeout);
    assert!(warn, "{message}");
    for part in ["remote workspace 513", "45초"] {
        assert!(message.contains(part), "{part} in {message}");
    }
    assert!(end.transport_end.is_ending());
    assert_eq!(end.transport_end.settled(), None, "the reader exit settles");
}

/// 재동기화는 사유를 올리지 않고 Detach 만 보낸다. 그 뒤의 EOF 는 이쪽이 끝낸 연결이라
/// 경고하지 않되, 재attach 가 이어지도록 끊김 사유는 올린다.
#[test]
fn a_read_error_after_our_own_detach_is_not_a_warning_but_still_disconnects() {
    let end = reader_end();
    end.detach_sent.store(true, Ordering::SeqCst);
    let eof = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    let (warn, message) = end.outcome(&eof);
    assert!(!warn, "{message}");
    assert!(message.contains("Detach"), "{message}");
    assert!(end.transport_end.is_ending());
}

/// 쓰기 스레드는 Detach 를 쓰기 전에 표지를 세운다. 쓰기 쪽을 미리 닫아 쓰기가 반드시 실패하게 해
/// 쓰기 뒤에 세우는 순서를 검출한다.
#[test]
fn the_writer_marks_our_detach_before_writing_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_peer, _) = listener.accept().unwrap();
    ours.shutdown(std::net::Shutdown::Write).unwrap();
    let (sender, rx) = super::super::connection::channel();
    sender
        .send(OutFrame {
            tag: StreamTag::Detach,
            payload: Vec::new(),
        })
        .unwrap();
    let mut link = writer_link(
        &Arc::new(TransportEnd::new()),
        &Arc::new(AtomicBool::new(true)),
        Duration::from_secs(60),
    );
    let detach_sent = Arc::new(AtomicBool::new(false));
    link.detach_sent = detach_sent.clone();
    let writer = spawn_attach_write_thread(ours, rx, link, Arc::new(|| {}), " detach order test");
    writer.join().unwrap();
    assert!(detach_sent.load(Ordering::SeqCst));
}

/// 거절 descriptor의 사유를 연결 오류에서 다시 읽을 수 있다. 오류 문장 뒤에 덧붙은 말은 사유가 아니다.
#[test]
fn a_rejection_reason_reads_back_from_the_connection_error() {
    let ctrl = serde_json::json!({"event": "attach_error", "reason": "mirror_workspace"});
    let error = parse_attach_descriptor(&ctrl)
        .expect_err("rejected")
        .to_string();
    assert_eq!(
        attach_rejection_reason(&error),
        Some(REJECTED_MIRROR_WORKSPACE)
    );
    assert_eq!(
        attach_rejection_reason(&format!("the mirror could not be installed: {error}")),
        Some(REJECTED_MIRROR_WORKSPACE)
    );
    assert_eq!(attach_rejection_reason("connection refused"), None);
}

/// 서버가 `force_detached`를 보내고 닫은 연결에서 이쪽 쓰기가 먼저 실패해도, 쓰기 스레드는 확정하지
/// 않고 reader를 기다린다. reader가 통지를 처리해 `Forced`로 확정하고, 확정 뒤의 깨움만 확정된
/// 사유를 본다.
///
/// 순서는 신호로 맞춘다. 너무 큰 프레임으로 송신을 실패시키고, 쓰기 스레드가 쓰기 쪽을 닫은 것(상대가
/// 보는 EOF)을 확인한 뒤에야 상대가 통지를 보내고 reader를 시작한다.
#[test]
fn a_write_failure_waits_for_the_reader_to_see_a_force_detach_notice() {
    use std::io::Read;
    let (write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    // 깨울 때마다 그 순간의 확정 사유를 기록한다. App이 깨어나 읽는 값과 같다.
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let wake: Arc<dyn Fn() + Send + Sync> = {
        let (end, seen) = (end.clone(), seen.clone());
        Arc::new(move || seen.lock().unwrap().push(end.settled()))
    };
    let (sender, rx) = failing_send();
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        writer_link(&end, &finished, Duration::from_secs(60)),
        wake.clone(),
        " force detach race test",
    );
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut rest = Vec::new();
    peer.read_to_end(&mut rest)
        .expect("the writer must close its send side when a write fails");
    assert!(rest.is_empty(), "the oversized frame must not be written");
    assert_eq!(
        end.settled(),
        None,
        "the writer settled before the reader finished"
    );
    stream::write_frame(
        &mut peer,
        StreamTag::Control,
        br#"{"event":"force_detached","reason":"force_detach_workspace"}"#,
    )
    .unwrap();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        wake,
        |_| None,
        reader_link(&end, &finished),
    );
    reader.join().unwrap();
    writer.join().unwrap();
    drop(sender);
    let seen = seen.lock().unwrap();
    let terminal: Vec<_> = seen.iter().flatten().collect();
    assert_eq!(terminal, vec![&EndReason::Forced], "{seen:?}");
    assert_eq!(end.settled(), Some(EndReason::Forced));
}

/// 핸드셰이크까지 마친 연결 한 쌍. 이쪽 쓰기용 사본, reader용 연결, 상대 소켓을 돌려준다.
fn opened_attach_pair() -> (TcpStream, StreamConnection, TcpStream) {
    use std::io::{BufRead, BufReader};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ours = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let handshake = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        stream::write_frame(
            &mut peer,
            StreamTag::Control,
            &serde_json::to_vec(&serde_json::json!({"ok":true,"proto":1,"client_id":7})).unwrap(),
        )
        .unwrap();
        peer
    });
    let write_half = ours.try_clone().unwrap();
    let (conn, _) = StreamConnection::open(ours, 1).unwrap();
    (write_half, conn, handshake.join().unwrap())
}

/// 송신을 반드시 실패시키는 너무 큰 프레임을 넣은 송신 채널.
fn failing_send() -> (
    std::sync::Arc<super::super::connection::ConnectionSender>,
    std::sync::mpsc::Receiver<super::super::connection::QueuedFrame>,
) {
    let (sender, rx) = super::super::connection::channel();
    sender
        .send(OutFrame {
            tag: StreamTag::Control,
            payload: vec![0; stream::MAX_FRAME_LEN as usize + 1],
        })
        .unwrap();
    (sender, rx)
}

fn panicking_decode(_: &[u8]) -> Option<MirrorEvent> {
    panic!("decode panics on purpose");
}

/// reader가 panic으로 끝나도 끝 표지와 끊김 표지가 서서, 송신이 실패한 writer가 상한(60초)을 기다리지 않고
/// 끝나고 끊김이 보인다.
#[test]
fn a_panicking_reader_still_lets_a_failed_writer_report_the_disconnect() {
    let (write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (sender, rx) = failing_send();
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        writer_link(&end, &finished, Duration::from_secs(60)),
        Arc::new(|| {}),
        " reader panic test",
    );
    stream::write_frame(&mut peer, StreamTag::Control, b"{}").unwrap();
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        Arc::new(|| {}),
        panicking_decode,
        reader_link(&end, &finished),
    );
    assert!(reader.join().is_err(), "the reader panicked");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || writer.is_finished()),
        "the writer kept waiting for a reader that panicked"
    );
    writer.join().unwrap();
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(end.settled(), Some(EndReason::Lost));
    drop(sender);
}

/// 깨움 콜백이 panic해 reader가 unwinding하는 중에 guard가 같은 콜백을 다시 불러도 프로세스가
/// abort하지 않고, reader는 panic으로 끝나며 끝 표지와 끊김 표지가 선다. guard가 그 panic을 잡지
/// 않으면 시험 프로세스 전체가 abort한다.
#[test]
fn a_wake_callback_that_panics_again_while_the_reader_unwinds_does_not_abort() {
    let (_write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (sender, _rx) = super::super::connection::channel();
    let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = wakes.clone();
    stream::write_frame(&mut peer, StreamTag::Data, b"").unwrap();
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            panic!("wake panics on purpose");
        }),
        |_| None,
        reader_link(&end, &finished),
    );
    assert!(reader.join().is_err(), "the first wake panicked");
    assert_eq!(wakes.load(Ordering::SeqCst), 2, "the guard woke again");
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(end.settled(), Some(EndReason::Lost));
}

/// 소멸자가 panic하는 payload. unwinding 중에 drop되면 프로세스가 abort한다.
struct PanicsOnDrop;

impl Drop for PanicsOnDrop {
    fn drop(&mut self) {
        panic!("the payload destructor panics on purpose");
    }
}

/// unwinding 중에 다시 panic한 깨움 콜백의 payload가 소멸자에서 panic해도 reader 끝 처리가
/// 프로세스를 abort시키지 않는다. payload를 drop하면 이 시험 실행 파일 전체가 abort한다.
#[test]
fn a_wake_payload_whose_destructor_panics_does_not_abort_the_reader_exit() {
    let (_write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (sender, _rx) = super::super::connection::channel();
    let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = wakes.clone();
    stream::write_frame(&mut peer, StreamTag::Data, b"").unwrap();
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        Arc::new(move || {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("the first wake panics on purpose");
            }
            std::panic::panic_any(PanicsOnDrop);
        }),
        |_| None,
        reader_link(&end, &finished),
    );
    assert!(reader.join().is_err(), "the first wake panicked");
    assert_eq!(wakes.load(Ordering::SeqCst), 2, "the guard woke again");
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(end.settled(), Some(EndReason::Lost));
}

/// reader가 끝나지 않는 연결에서도 송신이 실패한 writer는 상한 뒤 `Lost`로 확정하고 깨운다.
///
/// 기다림의 시작은 상대가 쓰기 쪽 닫힘의 EOF를 본 시각이고, 끝은 깨움 콜백이 불린 시각이다
/// (확정은 그 직전에 한다). 두 시각 모두 그 일을 겪은 스레드가 바로 찍으므로 시험 스레드의
/// 관측 지연이 들어가지 않는다. 남는 것은 두 스레드가 깨어나는 지연이다. 상대 스레드가 EOF를 늦게
/// 보면 잰 값이 줄고, 기다리지 않는 writer가 표지를 세우기 전에 밀려나면 잰 값이 늘어난다. 시험 전용
/// 상한을 1초로 두고 하한을 그 절반(500ms)으로 두어, 어느 쪽이든 지연이 수백 ms를 넘어야 판정이
/// 흔들리게 한다. 상한을 쓰는 실제 값(60초)은 이 시험의 대상이 아니다.
#[test]
fn a_failed_writer_reports_the_disconnect_after_the_wait_cap() {
    use std::io::Read;
    use std::sync::Mutex;
    use std::time::Instant;
    const CAP: Duration = Duration::from_secs(1);
    let (write_half, _conn, mut peer) = opened_attach_pair();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let eof_seen = std::thread::spawn(move || {
        let mut rest = Vec::new();
        peer.read_to_end(&mut rest).map(|_| Instant::now())
    });
    let woke_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let stamp = woke_at.clone();
    let end = Arc::new(TransportEnd::new());
    let (sender, rx) = failing_send();
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        writer_link(&end, &Arc::new(AtomicBool::new(false)), CAP),
        Arc::new(move || {
            stamp.lock().unwrap().get_or_insert_with(Instant::now);
        }),
        " reader wait cap test",
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || writer.is_finished()),
        "the writer must give up waiting for the reader at the cap"
    );
    writer.join().unwrap();
    let eof_at = eof_seen
        .join()
        .unwrap()
        .expect("the peer sees the half-close as EOF");
    let woke_at = woke_at.lock().unwrap().expect("the writer woke the app");
    let waited = woke_at.saturating_duration_since(eof_at);
    assert!(
        waited >= CAP / 2,
        "the disconnect came before the cap: {waited:?}"
    );
    assert_eq!(end.settled(), Some(EndReason::Lost));
    drop(sender);
}

#[path = "transport_exit_tests.rs"]
mod exits;

/// 회수 대기는 기다림을 끝낸 그 확인으로 상한 도달을 정한다. 끝낸 직후 worker가 끝나도(두 번째
/// 확인부터 참) 상한까지 기다린 사실이 남아야 한다.
#[test]
fn the_reaper_wait_keeps_the_judgement_that_ended_it() {
    let mut checks = 0;
    let waited_out = wait_for_workers(
        || {
            checks += 1;
            checks > 1
        },
        Duration::ZERO,
    );
    assert!(waited_out, "the wait ended at its bound");
    assert_eq!(checks, 1, "the judgement is not checked again");
}

#[test]
fn the_reaper_wait_ends_early_when_the_workers_are_finished() {
    assert!(!wait_for_workers(|| true, Duration::from_secs(10)));
}
