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

/// 실제 연결은 읽기 스레드가 소켓 사본을 들고 있어 writer가 끝나도 소켓이 남는다.
/// 서버가 끝낸 연결이면 writer가 쓰기 쪽을 닫아 client가 통지 뒤 EOF를 본다.
#[test]
fn a_server_ended_connection_reaches_the_client_as_eof_while_the_reader_holds_the_socket() {
    let (mut client, server) = sockets();
    let reader_copy = server.try_clone().unwrap();
    let hub = StreamHub::new();
    let rx = hub.register(5);
    for frame in [
        StreamFrame::new(
            StreamTag::Control,
            br#"{"event":"force_detached","reason":"force_detach"}"#.to_vec(),
        ),
        StreamFrame::new(StreamTag::Detach, vec![]),
    ] {
        assert_eq!(hub.push(5, frame), PushResult::Sent);
    }
    let writer =
        TcpIpcServer::spawn_stream_write_thread(server, rx, 1, Arc::new(AtomicBool::new(false)));
    assert_eq!(
        stream::read_frame(&mut client).unwrap().tag,
        StreamTag::Control
    );
    assert_eq!(
        stream::read_frame(&mut client).unwrap().tag,
        StreamTag::Detach
    );
    writer.join().unwrap();
    let after = stream::read_frame(&mut client).unwrap_err();
    assert_eq!(after.kind(), std::io::ErrorKind::UnexpectedEof, "{after}");
    drop(reader_copy);
}

/// 통지를 보낸 뒤 client가 닫지 않고 계속 보내도 읽기 루프가 끝나고 프레임을 넘기지 않는다.
#[test]
fn the_read_loop_ends_on_the_first_frame_after_the_server_ended_the_connection() {
    let (mut client, server) = sockets();
    let (inbound_tx, inbound_rx) = mpsc::channel();
    let ctx = StreamContext {
        hub: StreamHub::new(),
        inbound_tx,
        waker: Arc::new(|| {}),
        heartbeat_timeout: Default::default(),
    };
    let end = StreamReadEnd {
        client_id: 5,
        peer: None,
        read_timeout: Duration::from_secs(30),
        server_ending: Arc::new(AtomicBool::new(true)),
    };
    stream::write_frame(&mut client, StreamTag::Data, b"after").unwrap();
    let done = std::thread::spawn(move || {
        let mut reader = BufReader::new(server);
        TcpIpcServer::run_stream_read_loop(&ctx, &end, &mut reader);
    });
    let started = std::time::Instant::now();
    done.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(inbound_rx.try_recv().is_err());
}

/// 강제 끊김을 실제 writer와 소켓으로 끝까지 잰다. 읽기 스레드가 소켓 사본을 들고 있고,
/// 연결 정리(`finish_stream_connection`의 `unregister`)가 writer보다 먼저 와도
/// 통지를 넣었다고 답한 연결은 그 통지를 소켓에 쓰고, 못 넣은 연결은 곧바로 닫혀야 한다.
/// `blocked_reader`면 writer를 돌리기 전에 서버 읽기 스레드 사본에서 읽기를 걸어 둔다([`BlockedRead`]).
/// 양쪽을 닫는 경로에서만 쓴다. 쓰기 쪽만 닫는 경로는 읽기 쪽을 열어 두므로 그 읽기가 기한까지 남는다.
/// 돌려주는 값은 응답의 `notified`, client가 받은 태그, client 읽기가 끝난 오류, 걸어 둔 읽기,
/// 읽기 스레드가 들고 있는 소켓 사본, 그리고 아직 열어 둔 client 소켓이다.
fn force_detach_over_a_socket(
    free_slots: usize,
    blocked_reader: bool,
) -> (
    bool,
    Vec<StreamTag>,
    std::io::Error,
    Option<BlockedRead>,
    std::net::TcpStream,
    std::net::TcpStream,
) {
    let (mut client, server) = sockets();
    let reader_copy = server.try_clone().unwrap();
    let blocked = blocked_reader.then(|| BlockedRead::start(&server));
    let hub = StreamHub::new();
    let holder = hub.alloc_id();
    let rx = hub.register(holder);
    while hub.push(holder, StreamFrame::new(StreamTag::Data, vec![0])) == PushResult::Sent {}
    for _ in 0..free_slots {
        rx.recv().unwrap();
    }
    let (_state, mut session) = crate::state::tests::test_state();
    let notified = {
        let mut engine = session.borrow_mut();
        engine.remote.set_notifier(hub.clone());
        engine.live.occupancy.acquire(10, holder).unwrap();
        engine.force_detach(10).expect("held").notified
    };
    // 읽기 루프가 먼저 끝나 정리가 sink를 지운 순서를 만든다.
    hub.unregister(holder);
    let writer =
        TcpIpcServer::spawn_stream_write_thread(server, rx, 1, Arc::new(AtomicBool::new(false)));
    let mut tags = Vec::new();
    let end = loop {
        match stream::read_frame(&mut client) {
            Ok(frame) => tags.push(frame.tag),
            Err(error) => break error,
        }
    };
    writer.join().unwrap();
    (notified, tags, end, blocked, reader_copy, client)
}

/// 큐가 가득 차 통지를 못 넣으면 남은 큐를 버리고 곧바로 닫는다. client는 heartbeat 기한이 아니라
/// 바로 EOF를 본다(`sockets()`의 2초 읽기 기한 안).
#[test]
fn a_force_detach_that_could_not_queue_the_notice_closes_the_socket_at_once() {
    let (notified, tags, end, blocked, _reader_copy, _client) = force_detach_over_a_socket(0, true);
    assert!(!notified);
    assert!(tags.is_empty(), "{tags:?}");
    assert_eq!(end.kind(), std::io::ErrorKind::UnexpectedEof, "{end}");
    // client를 열어 둔 채로, 이미 막혀 있던 서버 읽기도 돌아와야 읽기 루프가 끝난다(`Shutdown::Both`).
    blocked.expect("started").assert_woken();
}

/// 자리가 하나만 남아 Control만 들어가도 `notified: true`면 그 통지는 소켓에 쓰이고 연결은 닫힌다.
#[test]
fn a_queued_force_detach_notice_reaches_the_socket_even_after_the_sink_is_released() {
    let (notified, tags, end, _blocked, _reader_copy, _client) =
        force_detach_over_a_socket(1, false);
    assert!(notified);
    assert_eq!(tags.last(), Some(&StreamTag::Control), "{:?}", tags.len());
    assert!(tags[..tags.len() - 1].iter().all(|t| *t == StreamTag::Data));
    assert_eq!(end.kind(), std::io::ErrorKind::UnexpectedEof, "{end}");
}

/// 자리가 넉넉하면 통지와 Detach가 모두 쓰인다. 정리가 먼저 와도 Detach가 버려지지 않는다.
#[test]
fn a_force_detach_with_room_writes_the_notice_and_the_detach_before_eof() {
    let (notified, tags, end, _blocked, _reader_copy, _client) =
        force_detach_over_a_socket(8, false);
    assert!(notified);
    assert_eq!(
        tags[tags.len() - 2..],
        [StreamTag::Control, StreamTag::Detach]
    );
    assert_eq!(end.kind(), std::io::ErrorKind::UnexpectedEof, "{end}");
}

/// 읽지 않는 client에 막힌 writer도 쓰기 기한에서 끝나야 연결 정리가 기다리다 멈추지 않는다.
#[test]
fn the_stream_write_timeout_is_armed_on_the_socket() {
    let (_client, server) = sockets();
    TcpIpcServer::arm_stream_write_timeout(&server, Duration::from_secs(45));
    assert_eq!(
        server.write_timeout().unwrap(),
        Some(Duration::from_secs(45))
    );
}

/// writer가 닫기 전에 서버 읽기 스레드의 소켓 사본에서 이미 막혀 있는 읽기. 3초 읽기 기한은 소켓을
/// 닫기 전에 건다. macOS는 양쪽이 닫힌 소켓의 옵션 설정을 `EINVAL`로 거절하기 때문이다.
/// 읽기 직전 준비 신호를 받고 50ms를 더 둔다. 신호는 recv 진입의 증거가 아니다.
struct BlockedRead {
    reader: std::thread::JoinHandle<(Duration, std::io::Result<usize>)>,
}

impl BlockedRead {
    const LIMIT: Duration = Duration::from_secs(3);
    /// 이보다 늦게 온 취소 코드는 기한 근처라 깨움으로 보지 않는다.
    const WOKEN_WITHIN: Duration = Duration::from_secs(2);
    /// 취소로 돌아온 읽기의 OS 코드: Windows의 10004(`WSAEINTR`, 관측값)와 995
    /// (`ERROR_OPERATION_ABORTED`). std는 995와 기한 만료 10060을 같은 `TimedOut`으로 분류하므로
    /// 종류가 아니라 코드와 시각으로 가른다.
    const CANCELLED: &[i32] = if cfg!(windows) { &[10_004, 995] } else { &[] };

    fn start(server: &std::net::TcpStream) -> Self {
        let mut copy = server.try_clone().unwrap();
        copy.set_read_timeout(Some(Self::LIMIT)).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut byte = [0u8; 1];
            ready_tx.send(()).unwrap();
            let started = std::time::Instant::now();
            let result = copy.read(&mut byte);
            (started.elapsed(), result)
        });
        ready_rx.recv_timeout(Self::LIMIT).expect("reader ready");
        std::thread::sleep(Duration::from_millis(50));
        Self { reader }
    }

    /// 걸어 둔 읽기가 EOF(`Ok(0)`)로, 또는 기한보다 이르게 취소 코드로 돌아왔다. 기한 만료와 그 밖의
    /// 오류·바이트 수신은 깨어나지 않은 것이다.
    fn assert_woken(self) {
        let (took, result) = self.reader.join().unwrap();
        match &result {
            Ok(0) => {}
            Err(e)
                if e.raw_os_error()
                    .is_some_and(|code| Self::CANCELLED.contains(&code))
                    && took < Self::WOKEN_WITHIN => {}
            other => panic!("the blocked server read was not woken: {other:?} after {took:?}"),
        }
    }
}

/// 끝내는 Control의 쓰기 자체가 실패하면 통지는 쓰이지 않았으므로 쓰기 쪽만 닫지 않고 양쪽을 닫는다.
/// 쓰기 쪽을 미리 닫아 쓰기를 실패시키고, 이미 막혀 있던 서버 읽기가 돌아오는지로 `Shutdown::Both` 를 잰다.
#[test]
fn a_failed_ending_frame_write_closes_both_sides() {
    let (_client, server) = sockets();
    let blocked = BlockedRead::start(&server);
    server.shutdown(std::net::Shutdown::Write).unwrap();
    let hub = StreamHub::new();
    let rx = hub.register(5);
    assert_eq!(
        hub.push(
            5,
            StreamFrame::new(
                StreamTag::Control,
                br#"{"event":"force_detached","reason":"force_detach"}"#.to_vec(),
            )
        ),
        PushResult::Sent
    );
    let server_ending = Arc::new(AtomicBool::new(false));
    let writer = TcpIpcServer::spawn_stream_write_thread(server, rx, 1, server_ending.clone());
    writer.join().unwrap();
    assert!(
        server_ending.load(Ordering::Acquire),
        "marked before the write"
    );
    blocked.assert_woken();
}
