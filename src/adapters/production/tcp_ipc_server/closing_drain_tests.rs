//! 포화 거절(-32062)이 요청 줄을 먼저 보낸 클라이언트에게 RST 없이 닿는지, accept 스레드를 기다리게
//! 하지 않는지 확인한다.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::TcpIpcServer;
use crate::ipc::protocol::JsonRpcResponse;

const PING_LINE: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let client = TcpStream::connect(addr).expect("connect");
    let (server_side, _) = listener.accept().expect("accept");
    (client, server_side)
}

/// 클라이언트가 보낸 바이트가 서버 소켓에 도착할 때까지 기다린다. 거절은 그 뒤에 한다.
fn wait_until_arrived(server_side: &TcpStream) {
    server_side.set_nonblocking(true).expect("nonblocking");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut b = [0u8; 1];
    while !matches!(server_side.peek(&mut b), Ok(n) if n > 0) {
        assert!(
            Instant::now() < deadline,
            "요청 바이트가 서버에 도착하지 않았다"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_saturated_refusal_after_a_sent_line_is_read_and_then_closes_without_reset() {
    let (mut client, server_side) = pair();
    client.write_all(PING_LINE).expect("send");
    wait_until_arrived(&server_side);

    TcpIpcServer::refuse_saturated_connection(server_side);

    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut reader = BufReader::new(client);
    let mut got = String::new();
    reader.read_line(&mut got).expect("거절 응답을 읽어야 한다");
    let resp: JsonRpcResponse = serde_json::from_str(got.trim()).expect("JSON 한 줄");
    assert_eq!(
        resp.error.expect("에러 응답").code,
        crate::ipc::protocol::ERR_CONNECTION_LIMIT_REACHED
    );
    // 읽지 않은 요청을 남긴 채 닫으면 RST 가 가고, Linux 클라이언트는 응답 다음에 연결 재설정을
    // 받는다(Windows 는 응답까지 잃는다). 비운 뒤 닫았으면 정상 종료(EOF)다.
    let mut rest = [0u8; 16];
    match reader.read(&mut rest) {
        Ok(0) => {}
        other => panic!("거절 뒤 정상 종료가 아니다: {other:?}"),
    }
}

#[test]
fn the_saturated_refusal_does_not_wait_for_a_client_that_keeps_sending() {
    let (mut client, server_side) = pair();
    client.write_all(PING_LINE).expect("send");
    wait_until_arrived(&server_side);
    // 거절하는 동안에도 조금씩 계속 보내는 상대. 기다리며 비우는 방식이면 이 상대 때문에 멈춘다.
    let sender = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if client.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    let started = Instant::now();
    TcpIpcServer::refuse_saturated_connection(server_side);
    let took = started.elapsed();

    // 기다리며 비우는 -32060 경로는 0.5초 창을 쓴다. 그 절반 아래면 기다리지 않은 것이다.
    assert!(
        took < Duration::from_millis(250),
        "포화 거절이 accept 스레드를 {took:?} 붙잡았다"
    );
    sender.join().expect("sender");
}

// Linux 는 쓰기 쪽을 먼저 닫기만 해도 응답 뒤 EOF 를 보여 줘서 위 시험으로는 비웠는지 가릴 수 없다.
// Windows 는 닫을 때 남은 수신 데이터로 생기는 RST 에 응답까지 버리므로, 닫기 전에 수신 큐가 비었는지
// 복제한 핸들로 직접 본다. 복제 핸들이 남아 있어 거절 함수가 놓아도 소켓은 아직 닫히지 않는다.
#[test]
fn the_saturated_refusal_reads_away_what_the_client_already_sent() {
    let (mut client, server_side) = pair();
    client.write_all(PING_LINE).expect("send");
    wait_until_arrived(&server_side);
    let probe = server_side.try_clone().expect("clone");

    TcpIpcServer::refuse_saturated_connection(server_side);

    let mut b = [0u8; 1];
    match probe.peek(&mut b) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("닫기 전 수신 큐에 읽지 않은 요청이 남았다: {other:?}"),
    }
}

// -32060: 상한을 넘는 줄을 다 보낸 클라이언트가 거절을 읽은 뒤 연결 재설정이 아니라 정상 종료를 받는다.
// 남는 바이트를 BufReader 버퍼(8 KiB)보다 크게 보내 서버 수신 큐에 읽지 않은 데이터가 남게 한다.
// Linux 는 쓰기 쪽만 닫아도 응답 뒤 EOF 를 보이므로, 닫기 전에 수신 큐가 비었는지도 본다.
#[test]
fn an_oversized_line_refusal_is_read_and_then_closes_without_reset() {
    use crate::ipc::protocol::MAX_REQUEST_LINE_BYTES;
    let (client, server_side) = pair();
    let mut sending = client.try_clone().expect("clone");
    let sender = std::thread::spawn(move || {
        let chunk = vec![b'a'; 64 * 1024];
        let mut left = MAX_REQUEST_LINE_BYTES + 64 * 1024;
        while left > 0 {
            let n = left.min(chunk.len());
            sending.write_all(&chunk[..n]).expect("send");
            left -= n;
        }
        sending.write_all(b"\n").expect("send newline");
    });

    let mut writer = server_side.try_clone().expect("clone");
    let probe = server_side.try_clone().expect("clone");
    let mut reader = BufReader::new(server_side);
    let mut line = String::new();
    let outcome = TcpIpcServer::read_line_capped(&mut reader, &mut line, None);
    assert!(matches!(outcome, super::LineRead::TooLong));
    // 요청을 다 보낸 뒤 거절하는 경우다.
    sender.join().expect("sender");
    TcpIpcServer::refuse_oversized_line(&mut writer, None);
    probe.set_nonblocking(true).expect("nonblocking");
    let mut b = [0u8; 1];
    match probe.peek(&mut b) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("닫기 전 수신 큐에 읽지 않은 요청이 남았다: {other:?}"),
    }
    drop(probe);
    drop(writer);
    drop(reader);

    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut client_reader = BufReader::new(client);
    let mut got = String::new();
    client_reader
        .read_line(&mut got)
        .expect("거절 응답을 읽어야 한다");
    let resp: JsonRpcResponse = serde_json::from_str(got.trim()).expect("JSON 한 줄");
    assert_eq!(
        resp.error.expect("에러 응답").code,
        crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG
    );
    let mut rest = [0u8; 16];
    match client_reader.read(&mut rest) {
        Ok(0) => {}
        other => panic!("거절 뒤 정상 종료가 아니다: {other:?}"),
    }
}
