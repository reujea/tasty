//! App 이 소유권을 놓는 자리의 전송 반납과 시도 취소는 회수 대기 없이 소켓을 바로 닫는다. 이미
//! `recv`에 막힌 reader 도 그 닫기로 끝나야 한다(Windows 는 닫기만으로 막힌 읽기가 돌아오지 않는다).

use super::*;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use tasty_ipc::stream::{self, StreamTag};
use tasty_utils::test_wait::holds_by;

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
/// 커널 진입의 증거는 아니다. 닫기 전에 읽기가 들어가 있었는지는 회차가 끝난 뒤 [`woken`] 이 가른다.
fn wait_until_reader_blocks(probe: &crate::transport::ReaderProbe) {
    assert!(
        holds_by(Instant::now() + READY_LIMIT, || probe.entered_recv()),
        "ready failure: the reader never reached recv (reader finished {})",
        probe.finished()
    );
    std::thread::sleep(Duration::from_millis(50));
}

/// 회차의 reader 가 닫기 전에 이미 `recv` 에 들어가 있었는지.
#[derive(Debug, PartialEq)]
enum Woken {
    /// 막혀 있던 읽기를 닫기가 깨웠다.
    InFlight,
    /// 읽기가 닫힌 뒤에 시작됐을 수 있어 막힌 읽기를 깨웠는지 알 수 없다.
    Undecided,
}

/// Windows 에서 닫기 함수의 취소로 돌아온 읽기의 OS 코드(`tasty_utils::socket_wake` 시험의 관측값
/// `WSAEINTR` 10004 와 `ERROR_OPERATION_ABORTED` 995). std 는 10004 를 `Interrupted` 로 분류하지 않으므로
/// `read_exact` 가 다시 읽지 않고 이 코드가 reader 의 끝 오류로 남는다.
const CANCELLED_READ: &[i32] = &[10_004, 995];
/// 판정할 수 없는 회차를 포함해 최초 회차부터 도는 최대 회차 수.
const RUNS: usize = 5;

/// reader 가 준비 계수를 올린 뒤 밀려 닫기가 먼저 오고 실제 `recv` 가 그 뒤에 시작되면(준비 계수 증가 →
/// reader 지연 → 닫기 → 실제 읽기), 원래 `shutdown` 만 하는 회귀에서도 뒤늦은 읽기가 바로 EOF 로 끝나
/// 판정을 통과한다. Windows 는 막혀 있던 읽기를 취소 코드로 깨우고 닫힌 뒤
/// 새로 건 읽기는 EOF 로 끝내므로, 취소 코드로 끝난 회차만 막힌 읽기를 깨운 것으로 본다. Linux·macOS 는 두
/// 경우 모두 EOF 라 가르지 못한다. 그 OS 의 `shutdown` 은 막힌 읽기도 돌려보내 이 시험이 닫기 함수의 차이를
/// 가르지 않으므로 끝난 회차를 모두 판정한 것으로 본다.
fn woken(probe: &crate::transport::ReaderProbe) -> Woken {
    if !cfg!(windows) {
        return Woken::InFlight;
    }
    match probe.end_os_error() {
        Some(Some(code)) if CANCELLED_READ.contains(&code) => Woken::InFlight,
        _ => Woken::Undecided,
    }
}

/// 회차를 판정할 수 있을 때까지 최초 회차를 포함해 최대 [`RUNS`] 회차를 돈다. 회차 안의 단언은 매 회차
/// 그대로 실패한다. Windows 에서 `recv` 에 막혀 있는지를 직접 볼 수단이 없어 남는 위험이 있다. 정상 구현도
/// reader 가 매 회차 위 순서로 늦게 재개하면 모든 회차가 판정 불가로 끝나 실패한다(회귀 검출이 아니다).
/// 반대로 회귀는 판정 불가 회차로 통과하지 못한다. 판정 불가 회차는 통과로 세지 않기 때문이다.
fn until_decided(mut run: impl FnMut() -> (Woken, Option<Option<i32>>)) {
    let mut ends = Vec::new();
    for _ in 0..RUNS {
        let (woken, end) = run();
        if woken == Woken::InFlight {
            return;
        }
        ends.push(end);
    }
    panic!(
        "undecided, not a regression verdict: in {RUNS} runs no reader ended with an accepted \
         cancellation code {CANCELLED_READ:?}; reader end errors {ends:?}. Each read may have \
         started after the close (a scheduling delay or an environment issue)"
    );
}

fn decided(probe: &crate::transport::ReaderProbe) -> (Woken, Option<Option<i32>>) {
    (woken(probe), probe.end_os_error())
}

/// 상대가 Detach 프레임 없이 EOF 를 받는다. 그 EOF 가 회수 스레드의 닫기보다 먼저인지는 호출자가 정한다
/// (전송을 버리기 전에 부르거나 회수 닫기를 막아 둔 채 부른다). 여기 기한은 닫히지 않은 소켓에서 멈추지
/// 않기 위한 것이다.
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

/// 반납 호출이 돌아올 때 이 스레드에서 소켓을 이미 닫았다. 회수 스레드는 이 표지를 세우지 않고, 닫기를
/// 다른 스레드에 넘기면 표지가 비거나 그 스레드를 가리킨다.
fn assert_closed_by_this_thread(stamp: &crate::transport::CloseStamp) {
    assert_eq!(
        stamp.by(),
        Some(std::thread::current().id()),
        "not closed at once: the return did not close the socket on its own thread"
    );
}

/// 막힌 reader 가 회수 스레드의 닫기 전에 끝났다. 호출자는 전송을 아직 버리지 않았거나 회수 닫기를 막아 둔
/// 채 부른다. 판정은 reader 가 스스로 남긴 끝 시각으로 한다. 상한은 닫기 뒤 5초와, 첫 recv 직전 시각 +
/// 픽스처 읽기 기한 중 이른 쪽이라 첫 읽기의 기한 만료로 끝난 reader 는 통과하지 못한다.
fn assert_reader_released(probe: &crate::transport::ReaderProbe, closed: Instant) {
    let first_recv = probe
        .first_recv_at()
        .expect("the reader stamps its first recv before it counts the entry");
    holds_by(closed + Duration::from_secs(10), || probe.finished());
    let Some(ended) = probe.ended_at() else {
        panic!("blocked reader: the close left the reader in recv");
    };
    let bound = (first_recv + FIXTURE_READ_TIMEOUT).min(closed + Duration::from_secs(5));
    assert!(
        ended < bound,
        "blocked reader: it ended {:?} after the close",
        ended.saturating_duration_since(closed)
    );
}

/// 버린 연결의 worker 가 모두 회수되고 panic 한 것이 없다. 회수 스레드는 모든 worker 를 join 한 뒤에만
/// 완료를 쓴다. 기한은 멈추지 않기 위한 것이다.
fn assert_retired(
    receipt: crate::transport::RetirementReceipt,
    probe: &crate::transport::ReaderProbe,
) {
    assert!(
        holds_by(Instant::now() + Duration::from_secs(10), || receipt
            .is_done()),
        "retirement not done: the workers were never retired (reader finished {})",
        probe.finished()
    );
    assert!(
        !receipt.failed(),
        "failed: a worker panicked (reader finished {})",
        probe.finished()
    );
}

/// 반납은 이 스레드에서 소켓을 닫고, 막힌 reader 는 전송을 버리기 전에(회수 스레드가 없을 때) 끝난다.
#[test]
fn close_locally_closes_the_socket_at_once_and_settles_lost() {
    until_decided(close_locally_run);
}

fn close_locally_run() -> (Woken, Option<Option<i32>>) {
    let (_remote, _token, prepared, mut peer) = prepared_with_peer();
    let transport = prepared.transport;
    let probe = transport.reader_probe.clone();
    wait_until_reader_blocks(&probe);
    let closed = Instant::now();
    transport.close_locally();
    assert_closed_by_this_thread(&transport.workers.close_stamp());
    assert_eq!(
        transport.end.settled(),
        Some(crate::transport_end::EndReason::Lost)
    );
    assert_reader_released(&probe, closed);
    assert_closed_without_detach(&mut peer, &probe);
    let receipt = transport.workers.receipt();
    drop(transport);
    assert_retired(receipt, &probe);
    decided(&probe)
}

/// 설치에 실패한 guard 는 이 스레드에서 소켓을 닫는다. guard 의 drop 은 회수도 시작하므로 회수 스레드의
/// 닫기를 막아 둔 채 reader 의 끝과 상대의 EOF 를 본다.
#[test]
fn a_failed_install_closes_the_taken_connection_at_once() {
    until_decided(failed_install_run);
}

fn failed_install_run() -> (Woken, Option<Option<i32>>) {
    let (_remote, _token, prepared, mut peer) = prepared_with_peer();
    let guard = InstallGuard::new(prepared.transport);
    let end = guard.transport().end.clone();
    let receipt = guard.transport().workers.receipt();
    let stamp = guard.transport().workers.close_stamp();
    let probe = guard.transport().reader_probe.clone();
    let hold = guard.transport().workers.hold_reaper_close();
    wait_until_reader_blocks(&probe);
    let closed = Instant::now();
    drop(guard);
    assert_closed_by_this_thread(&stamp);
    assert_eq!(end.settled(), Some(crate::transport_end::EndReason::Lost));
    assert_reader_released(&probe, closed);
    assert_closed_without_detach(&mut peer, &probe);
    drop(hold);
    assert_retired(receipt, &probe);
    decided(&probe)
}

/// 시도 취소가 등록 소켓을 닫으면 이미 `recv` 에 막힌 reader 가 끝난다. 전송을 drop 하기 전에 본다.
/// drop 의 Detach·writer 닫기·회수 스레드 닫기가 판정을 대신하지 않게 하기 위해서다.
/// 판정은 reader 가 스스로 남긴 끝 시각으로 한다. 상한은 취소 뒤 5초와, reader 가 첫 recv 직전에 남긴 시각 +
/// 픽스처 읽기 기한(30초) 중 이른 쪽이다. 첫 읽기의 기한 만료는 그 시각 + 30초 이후에만 일어나므로, 검사
/// 스레드가 늦게 깨어나거나 준비가 오래 걸려도 그 만료로 끝난 reader 를 취소로 끝난 것으로 보지 않는다.
/// 끝 시각은 끝 표지(`finished`)보다 먼저 서므로, 전송은 끝 표지를 본 뒤에만 버린다.
#[test]
fn cancelling_the_attempt_ends_a_reader_blocked_in_recv() {
    until_decided(cancel_run);
}

fn cancel_run() -> (Woken, Option<Option<i32>>) {
    let (mut remote, token, prepared, _peer) = prepared_with_peer();
    let transport = prepared.transport;
    let probe = transport.reader_probe.clone();
    wait_until_reader_blocks(&probe);
    assert!(
        probe.ended_at().is_none(),
        "the reader ended before the attempt was cancelled"
    );

    let first_recv = probe
        .first_recv_at()
        .expect("the reader stamps its first recv before it counts the entry");
    remote.cancel_attempt(&token);
    let cancelled = Instant::now();
    let deadline = (first_recv + FIXTURE_READ_TIMEOUT).min(cancelled + Duration::from_secs(5));
    // 픽스처 읽기 기한보다 길게 기다려, 끝나지 않던 reader 도 그 기한으로 끝나 끝 시각을 남기게 한다.
    let give_up = cancelled + FIXTURE_READ_TIMEOUT + Duration::from_secs(10);
    holds_by(give_up, || probe.finished());
    // 기한에 닿은 분기도 여기서 끝 시각을 다시 읽는다. 앞서 읽은 값이 낡았을 수 있다.
    let Some(ended) = probe.ended_at() else {
        panic!(
            "the cancelled attempt left its reader blocked in recv past the fixture read timeout"
        );
    };
    assert!(
        ended < deadline,
        "the cancelled attempt left its reader blocked in recv: it ended {:?} after the cancel returned",
        ended.saturating_duration_since(cancelled)
    );
    // 끝 시각을 남긴 reader 는 곧 끝 표지를 세운다. 그 표지를 본 뒤에만 전송을 버린다.
    assert!(
        holds_by(Instant::now() + Duration::from_secs(10), || probe
            .finished()),
        "the reader recorded its end but never set its finished flag"
    );

    let receipt = transport.workers.receipt();
    drop(transport);
    assert_retired(receipt, &probe);
    decided(&probe)
}

/// 반납 없이 버린 전송은 회수 스레드가 대기 상한 뒤 소켓을 닫는다. 그 닫기가 recv 에 막힌 reader 도 끝내야
/// 회수가 읽기 기한(30초)까지 늦어지지 않는다. 상대는 열린 채 응답하지 않고, writer 는 Detach 를 쓴 뒤 쓰기
/// 쪽만 닫고 reader 를 30초까지 기다리므로 먼저 소켓을 닫지 않는다. Linux·macOS 는 `shutdown` 만으로도
/// 막힌 읽기가 돌아오므로 이 시험이 닫기 함수의 차이를 가르는 것은 Windows 뿐이다. 그 차이는 닫기 전에
/// 읽기가 `recv` 에 들어가 있던 회차에서만 드러나므로 [`until_decided`] 로 그런 회차를 판정한다.
#[test]
fn a_dropped_transport_wakes_a_reader_blocked_in_recv() {
    until_decided(dropped_run);
}

fn dropped_run() -> (Woken, Option<Option<i32>>) {
    let (_remote, _token, prepared, _peer) = prepared_with_peer();
    let transport = prepared.transport;
    let probe = transport.reader_probe.clone();
    wait_until_reader_blocks(&probe);
    assert!(
        !probe.finished(),
        "the reader ended before the transport was dropped"
    );
    let receipt = transport.workers.receipt();
    let dropped = Instant::now();
    drop(transport);
    assert_reader_released(&probe, dropped);
    assert_retired(receipt, &probe);
    decided(&probe)
}
