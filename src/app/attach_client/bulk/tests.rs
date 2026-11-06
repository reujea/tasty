use super::*;

use crate::ipc::stream;
use crate::ipc::stream::{STREAM_PROTO, StreamControl, StreamTag};

#[test]
fn bulk_chunk_frames_roundtrip_and_reassembly() {
    let transfer_id = 0xABCD_1234_5678_9F01u64;
    // 여러 프레임에 걸쳐 다시 조립되는지 확인할 크기다.
    let total = BULK_CHUNK_RAW_LEN * 2 + 777;
    let data: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

    let frames = bulk_chunk_frames(transfer_id, &data);
    assert_eq!(frames.len(), 3, "2.5 청크 = 3 파트");

    let mut reassembled = Vec::new();
    for (expected_seq, framed) in frames.iter().enumerate() {
        assert!(framed.len() <= stream::MAX_FRAME_LEN as usize);
        let (tid, seq, part) = stream::decode_bulk_chunk(framed).expect("valid bulk chunk header");
        assert_eq!(tid, transfer_id);
        assert_eq!(seq as usize, expected_seq);
        reassembled.extend_from_slice(part);
    }
    assert_eq!(reassembled, data, "재조립 바이트가 원본과 동일");
}

#[test]
fn bulk_chunk_frames_empty_is_zero_chunks() {
    assert!(bulk_chunk_frames(1, &[]).is_empty());
}

#[test]
fn bulk_chunk_frames_exact_boundary_is_single_chunk() {
    let data = vec![7u8; BULK_CHUNK_RAW_LEN];
    let frames = bulk_chunk_frames(9, &data);
    assert_eq!(frames.len(), 1);
    let (_, seq, part) = stream::decode_bulk_chunk(&frames[0]).unwrap();
    assert_eq!(seq, 0);
    assert_eq!(part.len(), BULK_CHUNK_RAW_LEN);
}

#[test]
fn a_bulk_transfer_declares_loss_notify_and_aborts_on_a_loss_notice() {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (sock, _) = listener.accept().expect("accept");
        let mut writer = sock.try_clone().expect("clone");
        let mut reader = BufReader::new(sock);
        let mut line = String::new();
        reader.read_line(&mut line).expect("stream.open line");
        let ack = serde_json::to_vec(&tasty_ipc::stream::StreamAck {
            ok: true,
            client_id: Some(5),
            proto: STREAM_PROTO,
            error: None,
        })
        .expect("ack");
        stream::write_frame(&mut writer, StreamTag::Control, &ack).expect("write ack");
        let declared = stream::read_frame(&mut reader).expect("declaration");
        let loss = serde_json::to_vec(&StreamControl::Loss { frames: 1 }).expect("loss");
        stream::write_frame(&mut writer, StreamTag::Control, &loss).expect("write loss");
        declared
    });

    let mut conn = open_bulk_connection(BulkEndpoint {
        port,
        remote_ws: 3,
        heartbeat_timeout: stream::HEARTBEAT_TIMEOUT,
    })
    .expect("bulk connection");
    let err = await_bulk_result(
        &mut conn,
        11,
        &tasty_remote::connection::channel().0.epoch(),
    )
    .expect_err("결과를 모르면 성공이 아니다");
    let declared = server.join().expect("server");
    assert_eq!(declared.tag, StreamTag::Control);
    assert!(
        matches!(
            serde_json::from_slice::<StreamControl>(&declared.payload),
            Ok(StreamControl::ClientLossNotify {})
        ),
        "bulk 연결이 손실 통지를 선언해야 한다"
    );
    let msg = err.to_string();
    assert!(msg.contains("aborted"), "중단으로 보고해야 한다: {msg}");
    assert!(
        !msg.starts_with(BULK_REJECT_PREFIX),
        "원격 거부로 보이면 재시도가 막힌다: {msg}"
    );
}

/// `StreamConnection`의 handshake가 소켓 timeout을 다시 걸므로, 연결을 연 뒤의 값을 확인한다.
#[test]
fn the_heartbeat_timeout_is_still_on_the_bulk_socket_after_the_handshake() {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (sock, _) = listener.accept().expect("accept");
        let mut writer = sock.try_clone().expect("clone");
        let mut reader = BufReader::new(sock);
        let mut line = String::new();
        reader.read_line(&mut line).expect("stream.open line");
        let ack = serde_json::to_vec(&tasty_ipc::stream::StreamAck {
            ok: true,
            client_id: Some(5),
            proto: STREAM_PROTO,
            error: None,
        })
        .expect("ack");
        stream::write_frame(&mut writer, StreamTag::Control, &ack).expect("write ack");
        stream::read_frame(&mut reader).expect("declaration")
    });
    let configured = std::time::Duration::from_secs(17);
    let conn = open_bulk_connection(BulkEndpoint {
        port,
        remote_ws: 3,
        heartbeat_timeout: configured,
    })
    .expect("bulk connection");
    let socket = conn.try_clone_writer().expect("socket");
    assert_eq!(socket.read_timeout().expect("read"), Some(configured));
    assert_eq!(socket.write_timeout().expect("write"), Some(configured));
    drop(server.join().expect("server"));
}
