//! App 이 소유권을 놓는 자리의 전송 반납은 회수 대기 없이 소켓을 바로 닫는다.

use super::*;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use tasty_ipc::stream::{self, StreamTag};

/// handshake 를 마친 상대와 준비된 연결.
fn prepared_with_peer() -> (
    crate::outbound::Remote,
    crate::transport::PreparedConnection,
    std::net::TcpStream,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(socket.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        for control in [
            serde_json::json!({"ok": true, "proto": 1, "client_id": 7}),
            serde_json::json!({"event": "attached_workspace", "name": "remote", "tree": null, "surfaces": []}),
        ] {
            stream::write_frame(
                &mut socket,
                StreamTag::Control,
                &serde_json::to_vec(&control).unwrap(),
            )
            .unwrap();
        }
        socket
    });
    // 시도는 `Remote` 가 살아 있는 동안만 유효하다.
    let mut remote = crate::outbound::Remote::new();
    let token = remote.begin_attempt(None, None).unwrap();
    let prepared = crate::transport::PreparedConnection::connect(
        token,
        port,
        3,
        None,
        Arc::new(|| {}),
        |_| None,
        Duration::from_secs(10),
    )
    .unwrap();
    (remote, prepared, peer.join().unwrap())
}

/// 상대가 Detach 프레임 없이 EOF 를 받는다. 그 EOF 가 회수 대기 전에 왔는지는 벽시계로 재지 않고
/// [`assert_workers_retire_before_the_reaper_wait`] 가 회수 영수증으로 잰다. 회수 스레드도 대기 뒤에
/// 소켓을 닫으므로 여기 기한은 닫히지 않은 소켓에서 멈추지 않기 위한 것이다.
fn assert_closed_without_detach(peer: &mut std::net::TcpStream) {
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    loop {
        match stream::read_frame(peer) {
            Ok(frame) => assert_ne!(frame.tag, StreamTag::Detach, "closed without a Detach"),
            Err(error) => {
                assert!(
                    !matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ),
                    "the socket stayed open: {error}"
                );
                break;
            }
        }
    }
}

/// 반납한 연결의 worker 가 회수 스레드의 대기 상한(250ms)보다 먼저 끝난다. 소켓을 닫지 않았다면
/// reader 가 읽기 기한(10초)까지 막혀 회수 스레드가 상한을 다 기다린 뒤 소켓을 닫는다. 그 여부는 회수
/// 스레드가 영수증에 남기므로([`crate::transport::RetirementReceipt::waited_out`]) 검사 스레드가
/// 밀려도 판정이 바뀌지 않는다. 회수 완료를 기다리는 기한은 멈추지 않기 위한 것이다.
fn assert_workers_retire_before_the_reaper_wait(receipt: crate::transport::RetirementReceipt) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !receipt.is_done() {
        assert!(Instant::now() < deadline, "the workers were never retired");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !receipt.waited_out(),
        "the reaper waited out its bound for the workers"
    );
}

#[test]
fn close_locally_closes_the_socket_at_once_and_settles_lost() {
    let (_remote, prepared, mut peer) = prepared_with_peer();
    let transport = prepared.transport;
    transport.close_locally();
    assert_eq!(
        transport.end.settled(),
        Some(crate::transport_end::EndReason::Lost)
    );
    assert_closed_without_detach(&mut peer);
    let receipt = transport.workers.receipt();
    drop(transport);
    assert_workers_retire_before_the_reaper_wait(receipt);
}

#[test]
fn a_failed_install_closes_the_taken_connection_at_once() {
    let (_remote, prepared, mut peer) = prepared_with_peer();
    let guard = InstallGuard::new(prepared.transport);
    let end = guard.transport().end.clone();
    let receipt = guard.transport().workers.receipt();
    drop(guard);
    assert_eq!(end.settled(), Some(crate::transport_end::EndReason::Lost));
    assert_closed_without_detach(&mut peer);
    assert_workers_retire_before_the_reaper_wait(receipt);
}
