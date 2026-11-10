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
    let disconnected = Arc::new(AtomicBool::new(false));
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let wake = Arc::new(move || {
        wake_tx.send(()).expect("test receiver remains alive");
    });
    let reader = spawn_attach_reader_thread(
        conn,
        output.clone(),
        sender.clone(),
        disconnected,
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
    }
}

/// 이쪽이 먼저 끊김을 알린(정리·쓰기 실패) 뒤의 수신 오류는 원격 원인이 아니라 경고하지 않는다.
#[test]
fn a_read_error_after_our_own_disconnect_is_not_a_warning() {
    let disconnected = AtomicBool::new(true);
    let eof = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    let (warn, message) = reader_end().outcome(&eof, &disconnected);
    assert!(!warn, "{message}");
    assert!(message.contains("PeerClosed"), "{message}");
    assert!(disconnected.load(Ordering::SeqCst));
}

/// 처음 끊김을 알리는 수신 오류는 원인과 실제로 건 timeout을 경고로 남기고 끊김을 세운다.
#[test]
fn the_first_read_error_warns_with_its_cause_and_raises_the_disconnect() {
    let disconnected = AtomicBool::new(false);
    let timeout = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::WouldBlock));
    let (warn, message) = reader_end().outcome(&timeout, &disconnected);
    assert!(warn, "{message}");
    for part in ["remote workspace 513", "45초"] {
        assert!(message.contains(part), "{part} in {message}");
    }
    assert!(disconnected.load(Ordering::SeqCst));
}
