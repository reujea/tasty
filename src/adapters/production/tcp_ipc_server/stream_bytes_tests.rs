use super::*;
use tasty_ipc::{
    stream::StreamFrame,
    stream_hub::{PublicationFence, PushResult, StreamHub},
};
const FULL: usize = stream::MAX_FRAME_LEN as usize + stream::FRAME_HEADER_LEN;

fn sockets() -> (std::net::TcpStream, std::net::TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (server, _) = listener.accept().unwrap();
    (client, server)
}
#[test]
fn actual_socket_writer_skips_retired_document_and_writes_live_frame() {
    let (mut client, server) = sockets();
    let hub = StreamHub::new();
    let rx = hub.register(5);
    let binding = hub.client_binding(5).unwrap();
    let owner = PublicationFence::default();
    let reservation = hub
        .reserve_frame(5, &binding, FULL)
        .unwrap()
        .for_owner(&owner);
    assert_eq!(
        hub.push_reserved(
            reservation,
            StreamFrame::new(StreamTag::Control, b"retired".to_vec())
        ),
        PushResult::Sent
    );
    drop(owner);
    assert_eq!(
        hub.push(5, StreamFrame::new(StreamTag::Control, b"live".to_vec())),
        PushResult::Sent
    );
    assert_eq!(
        hub.push(5, StreamFrame::new(StreamTag::Detach, vec![])),
        PushResult::Sent
    );
    let server_ending = Arc::new(AtomicBool::new(false));
    let writer = TcpIpcServer::spawn_stream_write_thread(server, rx, 1, server_ending.clone());
    assert_eq!(stream::read_frame(&mut client).unwrap().payload, b"live");
    assert_eq!(
        stream::read_frame(&mut client).unwrap().tag,
        StreamTag::Detach
    );
    writer.join().unwrap();
    // 읽기 루프가 뒤따르는 EOF를 정상 종료로 보도록 Detach 송신을 표시한다.
    assert!(server_ending.load(Ordering::Acquire));
    assert!(stream::read_frame(&mut client).is_err());
    assert!(hub.reserve_frame(5, &binding, FULL).is_some());
}
/// client는 끝내는 프레임을 받자마자 닫을 수 있으므로 표지는 쓰기 전에 선다.
/// 쓰기 쪽을 미리 닫아 쓰기가 반드시 실패하게 해, 쓰기 뒤에 세우는 순서를 검출한다.
#[test]
fn the_ending_mark_is_set_before_the_frame_write_starts() {
    let cases = [
        (StreamFrame::new(StreamTag::Detach, vec![]), true),
        (
            StreamFrame::new(
                StreamTag::Control,
                br#"{"event":"force_detached","reason":"local"}"#.to_vec(),
            ),
            true,
        ),
        (
            StreamFrame::new(StreamTag::Control, b"live".to_vec()),
            false,
        ),
    ];
    for (frame, ends) in cases {
        let tag = frame.tag;
        let (_client, server) = sockets();
        server.shutdown(std::net::Shutdown::Write).unwrap();
        let hub = StreamHub::new();
        let rx = hub.register(5);
        assert_eq!(hub.push(5, frame), PushResult::Sent);
        let server_ending = Arc::new(AtomicBool::new(false));
        let writer = TcpIpcServer::spawn_stream_write_thread(server, rx, 1, server_ending.clone());
        writer.join().unwrap();
        assert_eq!(server_ending.load(Ordering::Acquire), ends, "{tag:?}");
    }
}

#[test]
fn actual_socket_writer_discards_old_registration_without_publishing_to_replacement() {
    let (mut client, server) = sockets();
    let hub = StreamHub::new();
    let rx = hub.register(5);
    assert_eq!(
        hub.push(5, StreamFrame::new(StreamTag::Control, b"old".to_vec())),
        PushResult::Sent
    );
    let replacement = hub.register(5);
    let writer =
        TcpIpcServer::spawn_stream_write_thread(server, rx, 1, Arc::new(AtomicBool::new(false)));
    writer.join().unwrap();
    assert!(stream::read_frame(&mut client).is_err());
    assert!(replacement.try_recv().is_err());
    assert!(
        hub.reserve_frame(5, &hub.client_binding(5).unwrap(), FULL)
            .is_some()
    );
}

#[test]
fn actual_protocol_rejection_writes_ack_before_cleanup_without_attach() {
    for proto in [None, Some(stream::STREAM_PROTO + 41)] {
        for _ in 0..16 {
            let (mut client, server) = sockets();
            let (tx, rx) = mpsc::channel();
            let hub = StreamHub::new();
            let ctx = StreamContext {
                hub: hub.clone(),
                inbound_tx: tx,
                waker: Arc::new(|| {}),
                heartbeat_timeout: Default::default(),
            };
            let mut params = serde_json::json!({"target_workspace": 7, "bulk_workspace": 7});
            if let Some(proto) = proto {
                params["proto"] = proto.into();
            }
            let req = serde_json::from_value(serde_json::json!({
                "jsonrpc": "2.0", "method": "stream.open", "params": params, "id": 1
            }))
            .unwrap();
            let reader = BufReader::new(server.try_clone().unwrap());
            // Cleanup has fully returned before the peer reads its final reply.
            TcpIpcServer::handle_stream_connection(reader, server, req, ctx, None);
            let frame = stream::read_frame(&mut client).expect("negative ACK before EOF");
            assert_eq!(frame.tag, StreamTag::Control);
            let ack: StreamAck = serde_json::from_slice(&frame.payload).unwrap();
            assert!(!ack.ok);
            assert_eq!(ack.proto, stream::STREAM_PROTO);
            assert!(
                ack.error
                    .unwrap()
                    .contains(&proto.unwrap_or_default().to_string())
            );
            let id = ack.client_id.unwrap();
            assert!(hub.client_binding(id).is_none());
            assert!(hub.bulk_workspace(id).is_none());
            assert!(
                matches!(rx.try_recv().unwrap(), StreamInbound::Disconnected { client_id } if client_id == id)
            );
            assert!(rx.try_recv().is_err(), "must not dispatch attach");
            assert!(
                stream::read_frame(&mut client).is_err(),
                "connection closed"
            );
        }
    }
}

#[test]
fn rejection_flush_does_not_authorize_retired_or_replaced_frames() {
    for replace in [false, true] {
        let (mut client, server) = sockets();
        let hub = StreamHub::new();
        let rx = hub.register(5);
        let binding = hub.client_binding(5).unwrap();
        let owner = PublicationFence::default();
        let reservation = hub
            .reserve_frame(5, &binding, FULL)
            .unwrap()
            .for_owner(&owner);
        assert_eq!(
            hub.push_reserved(
                reservation,
                StreamFrame::new(StreamTag::Control, b"old".to_vec())
            ),
            PushResult::Sent
        );
        if !replace {
            drop(owner);
        }
        let replacement = replace.then(|| hub.register(5));
        TcpIpcServer::write_stream_rejection(server, rx);
        assert!(stream::read_frame(&mut client).is_err());
        if let Some(replacement) = replacement {
            assert!(replacement.try_recv().is_err());
        }
        let current = hub.client_binding(5).unwrap();
        assert!(
            hub.reserve_frame(5, &current, FULL).is_some(),
            "flush releases its byte lease"
        );
    }
}

#[test]
fn actual_socket_writer_rechecks_authority_before_publishing_queued_frame() {
    let (mut client, server) = sockets();
    let hub = StreamHub::new();
    let rx = hub.register(5);
    let binding = hub.client_binding(5).unwrap();
    let current = Arc::new(AtomicBool::new(true));
    let check = current.clone();
    let reservation = hub
        .reserve_frame(5, &binding, FULL)
        .unwrap()
        .with_guard(Arc::new(move || check.load(Ordering::Acquire)));
    assert_eq!(
        hub.push_reserved(
            reservation,
            StreamFrame::new(StreamTag::Control, b"obsolete authority".to_vec())
        ),
        PushResult::Sent
    );
    current.store(false, Ordering::Release);
    hub.push(5, StreamFrame::new(StreamTag::Control, b"current".to_vec()));
    hub.push(5, StreamFrame::new(StreamTag::Detach, vec![]));
    let writer =
        TcpIpcServer::spawn_stream_write_thread(server, rx, 1, Arc::new(AtomicBool::new(false)));
    assert_eq!(stream::read_frame(&mut client).unwrap().payload, b"current");
    assert_eq!(
        stream::read_frame(&mut client).unwrap().tag,
        StreamTag::Detach
    );
    writer.join().unwrap();
    assert!(stream::read_frame(&mut client).is_err());
    assert!(hub.reserve_frame(5, &binding, FULL).is_some());
}
