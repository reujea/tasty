//! App 이 소유권을 놓는 자리의 전송 반납과 시도 취소는 회수 대기 없이 소켓을 바로 닫는다. 이미
//! `recv`에 막힌 reader 도 그 닫기로 끝나야 한다(Windows 는 닫기만으로 막힌 읽기가 돌아오지 않는다).

use super::*;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use tasty_ipc::stream::{self, StreamTag};

/// 픽스처 연결의 읽기 기한이자 writer 의 reader 대기 상한. 아래 관측 기한(영수증 10초, 취소 뒤
/// 5초)보다 길어서, 첫 읽기의 기한 만료나 그 상한이 닫기 대신 reader 를 끝내 통과시키지 못한다.
const FIXTURE_READ_TIMEOUT: Duration = Duration::from_secs(30);
const READY_LIMIT: Duration = Duration::from_secs(5);

/// handshake 를 마친 상대와 준비된 연결. 시도 토큰의 복제도 돌려준다.
fn prepared_with_peer() -> (
    crate::outbound::Remote,
    crate::outbound::AttemptToken,
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
        token.clone(),
        port,
        3,
        None,
        Arc::new(|| {}),
        |_| None,
        FIXTURE_READ_TIMEOUT,
    )
    .unwrap();
    (remote, token, prepared, peer.join().unwrap())
}

/// reader 가 handshake 뒤 `recv` 에 들어가기를 기다리고 조금 더 둔다. 계수는 recv 직전에 오르므로
/// 커널 진입의 증거는 아니다. 그래서 음성 대조가 정한 판정에서 실패하는 것을 막힘 증거로 쓴다.
fn wait_until_reader_blocks(probe: &crate::transport::ReaderProbe) {
    let deadline = Instant::now() + READY_LIMIT;
    while !probe.entered_recv() {
        assert!(
            Instant::now() < deadline,
            "ready failure: the reader never reached recv (reader finished {})",
            probe.finished()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(50));
}

/// 상대가 Detach 프레임 없이 EOF 를 받는다. 그 EOF 가 회수 대기 전에 왔는지는 벽시계로 재지 않고
/// [`assert_workers_retire_before_the_reaper_wait`] 가 회수 영수증으로 잰다. 회수 스레드도 대기 뒤에
/// 소켓을 닫으므로 여기 기한은 닫히지 않은 소켓에서 멈추지 않기 위한 것이다.
fn assert_closed_without_detach(
    peer: &mut std::net::TcpStream,
    probe: &crate::transport::ReaderProbe,
) {
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    loop {
        match stream::read_frame(peer) {
            Ok(frame) => assert_ne!(
                frame.tag,
                StreamTag::Detach,
                "peer failure: closed with a Detach (reader finished {})",
                probe.finished()
            ),
            Err(error) => {
                assert!(
                    !matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ),
                    "peer failure: the socket stayed open: {error} (reader finished {})",
                    probe.finished()
                );
                break;
            }
        }
    }
}

/// 반납한 연결의 worker 가 회수 스레드의 대기 상한(250ms)보다 먼저 끝난다. 막힌 reader 가 닫기로
/// 끝나지 않았다면 회수 스레드가 상한을 다 기다리거나(`waited_out`), 픽스처 읽기 기한(30초)까지
/// join 에 막혀 아래 10초 안에 완료되지 않는다. 회수 스레드는 모든 worker 를 join 한 뒤에만 완료를
/// 쓰므로 완료 뒤 `waited_out`·`failed` 를 읽는다. 그때의 reader 끝 표지는 join 뒤의 값이라 250ms
/// 시점에 늦은 worker 를 가리키지 않는다.
fn assert_workers_retire_before_the_reaper_wait(
    receipt: crate::transport::RetirementReceipt,
    probe: &crate::transport::ReaderProbe,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !receipt.is_done() {
        assert!(
            Instant::now() < deadline,
            "retirement not done: the workers were never retired (reader finished {})",
            probe.finished()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !receipt.waited_out(),
        "waited out: the reaper waited out its bound for the workers (reader finished {})",
        probe.finished()
    );
    assert!(
        !receipt.failed(),
        "failed: a worker panicked (reader finished {})",
        probe.finished()
    );
}

#[test]
fn close_locally_closes_the_socket_at_once_and_settles_lost() {
    let (_remote, _token, prepared, mut peer) = prepared_with_peer();
    let transport = prepared.transport;
    let probe = transport.reader_probe.clone();
    wait_until_reader_blocks(&probe);
    transport.close_locally();
    assert_eq!(
        transport.end.settled(),
        Some(crate::transport_end::EndReason::Lost)
    );
    assert_closed_without_detach(&mut peer, &probe);
    let receipt = transport.workers.receipt();
    drop(transport);
    assert_workers_retire_before_the_reaper_wait(receipt, &probe);
}

#[test]
fn a_failed_install_closes_the_taken_connection_at_once() {
    let (_remote, _token, prepared, mut peer) = prepared_with_peer();
    let guard = InstallGuard::new(prepared.transport);
    let end = guard.transport().end.clone();
    let receipt = guard.transport().workers.receipt();
    let probe = guard.transport().reader_probe.clone();
    wait_until_reader_blocks(&probe);
    drop(guard);
    assert_eq!(end.settled(), Some(crate::transport_end::EndReason::Lost));
    assert_closed_without_detach(&mut peer, &probe);
    assert_workers_retire_before_the_reaper_wait(receipt, &probe);
}

/// 시도 취소가 등록 소켓을 닫으면 이미 `recv` 에 막힌 reader 가 끝난다. 전송을 drop 하기 전에 본다.
/// drop 의 Detach·writer 닫기·회수 스레드 닫기가 판정을 대신하지 않게 하기 위해서다.
#[test]
fn cancelling_the_attempt_ends_a_reader_blocked_in_recv() {
    let observe_until = Instant::now() + Duration::from_secs(10);
    let (mut remote, token, prepared, _peer) = prepared_with_peer();
    let transport = prepared.transport;
    let probe = transport.reader_probe.clone();
    wait_until_reader_blocks(&probe);
    assert!(
        !probe.finished(),
        "the reader ended before the attempt was cancelled"
    );

    remote.cancel_attempt(&token);
    let deadline = observe_until.min(Instant::now() + Duration::from_secs(5));
    while !probe.finished() {
        assert!(
            Instant::now() < deadline,
            "the cancelled attempt left its reader blocked in recv"
        );
        std::thread::sleep(Duration::from_millis(2));
    }

    let receipt = transport.workers.receipt();
    drop(transport);
    assert_workers_retire_before_the_reaper_wait(receipt, &probe);
}
