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

/// 취소된 시도의 bulk 연결은 기한 설정이나 open 실패를 취소 오류로 끝낸다. 실제 소켓에서 기한 설정이 실패하는
/// 것은 macOS 뿐이라(양쪽을 닫은 소켓의 옵션 설정을 거절한다) 사전 기한 설정 분기는 macOS 시험과 설정 결과를
/// 넣는 시험(bulk 의 경고 함수를 그대로 넘긴다)이 잰다. 로그를 단언하는 시험은 혼자 돈다
/// (`docs/dev-guide/unit-test-isolation.md` "tracing 콜사이트 관심 캐시").
mod retired_attempt {
    use super::super::{BulkEndpoint, open_bulk_connection_bound, warn_timeout};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tasty_remote::attempt_seam::{self, Stage};
    use tasty_remote::outbound::{ATTEMPT_RETIRED, AttemptToken, Remote};
    use tasty_remote::transport::arm_timeouts_with;

    fn capture_logs(f: impl FnOnce()) -> String {
        #[derive(Clone)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(
            tasty_test_support::is_rerun_alone(),
            "log assertions run alone (rerun_alone)"
        );
        let buf = Buf(Arc::new(Mutex::new(Vec::new())));
        let sink = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        let out = buf.0.lock().unwrap().clone();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn warn_lines(logs: &str) -> Vec<&str> {
        logs.lines().filter(|line| line.contains("WARN")).collect()
    }

    fn einval() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::InvalidInput)
    }

    /// 읽기·쓰기 기한 실패 두 분기를 bulk 의 경고 함수로 지난다. 시도가 없거나 활성이면 경고 뒤 다음 설정으로
    /// 진행하고, 취소된 시도는 그 자리에서 경고 없이 돌아온다. 경고가 이 구독자에 닿는 것이 관측 범위의 근거다.
    #[test]
    fn a_bulk_timeout_failure_on_a_retired_attempt_returns_the_cancel_without_a_warning() {
        if tasty_test_support::rerun_alone(
            module_path!(),
            "a_bulk_timeout_failure_on_a_retired_attempt_returns_the_cancel_without_a_warning",
        ) {
            return;
        }
        let mut remote = Remote::new();
        let active = remote.begin_attempt(None, None).unwrap();
        let retired = remote.begin_attempt(None, None).unwrap();
        remote.cancel_attempt(&retired);
        for (read_fails, what) in [(true, "read timeout"), (false, "write timeout")] {
            let set_read = || if read_fails { Err(einval()) } else { Ok(()) };
            let set_write = || if read_fails { Ok(()) } else { Err(einval()) };
            for attempt in [None, Some(&active)] {
                let mut wrote = false;
                let logs = capture_logs(|| {
                    arm_timeouts_with(
                        attempt,
                        set_read,
                        || {
                            wrote = true;
                            set_write()
                        },
                        warn_timeout,
                    )
                    .expect("carries on");
                });
                assert!(wrote, "{what}: went on to the write timeout");
                let warns = warn_lines(&logs);
                assert_eq!(warns.len(), 1, "{what}: one warning:\n{logs}");
                assert!(
                    warns[0].contains(&format!("failed to set {what}")),
                    "{logs}"
                );
            }
            let mut wrote = false;
            let mut result = Ok(());
            let logs = capture_logs(|| {
                result = arm_timeouts_with(
                    Some(&retired),
                    set_read,
                    || {
                        wrote = true;
                        set_write()
                    },
                    warn_timeout,
                );
            });
            let error = result.expect_err("a retired attempt stops");
            assert_eq!(error.to_string(), ATTEMPT_RETIRED, "{what}");
            assert_eq!(
                wrote, !read_fails,
                "{what}: it stopped at the failed setting"
            );
            assert!(warn_lines(&logs).is_empty(), "{what}: no warning:\n{logs}");
        }
    }

    /// 연결을 받고 아무것도 보내지 않는 상대. 연결 함수가 소켓 닫기로 끝나므로 응답하지 않아도 된다.
    fn silent_peer() -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut buf = [0u8; 256];
            while matches!(std::io::Read::read(&mut &socket, &mut buf), Ok(n) if n > 0) {}
        });
        (port, peer)
    }

    fn open(port: u16, attempt: &AttemptToken) -> anyhow::Result<()> {
        open_bulk_connection_bound(
            BulkEndpoint {
                port,
                remote_ws: 3,
                heartbeat_timeout: Duration::from_secs(5),
            },
            Some(attempt),
        )
        .map(|_| ())
    }

    /// `stage` 에서 시도를 취소하는 bulk 연결의 결과.
    fn cancelled_at(stage: Stage) -> anyhow::Result<()> {
        let remote = Rc::new(RefCell::new(Remote::new()));
        let token = remote.borrow_mut().begin_attempt(None, None).unwrap();
        let (port, peer) = silent_peer();
        let (cancel_remote, cancel_token) = (remote.clone(), token.clone());
        attempt_seam::arm(stage, move |_| {
            cancel_remote.borrow_mut().cancel_attempt(&cancel_token)
        });
        let result = open(port, &token);
        peer.join().unwrap();
        result
    }

    /// B-2: 기한 설정 뒤 open 직전에 취소되면 open 실패가 취소 오류가 된다.
    #[test]
    fn a_bulk_attempt_cancelled_before_the_open_ends_as_retired() {
        let error = cancelled_at(Stage::BeforeOpen).expect_err("the cancel ends the open");
        assert!(attempt_seam::reached(Stage::BeforeOpen));
        assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    }

    /// B-1: 등록 직후 취소되면 결과는 취소 오류다. macOS 는 사전 기한 설정에서, 다른 OS 는 open 에서 돌아온다.
    #[test]
    fn a_bulk_attempt_cancelled_after_registering_ends_as_retired() {
        let error = cancelled_at(Stage::AfterRegister).expect_err("the cancel ends the open");
        assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    }

    /// macOS B-1: 같은 기한 설정 실패를 활성 시도(소켓만 닫음)와 취소된 시도로 지난다. 활성 시도는 경고를
    /// 남기고 open 까지 가며, 취소된 시도는 open 전에 경고 없이 돌아온다.
    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_a_cancelled_bulk_open_stops_at_the_timeout_without_a_warning() {
        if tasty_test_support::rerun_alone(
            module_path!(),
            "on_macos_a_cancelled_bulk_open_stops_at_the_timeout_without_a_warning",
        ) {
            return;
        }
        let mut remote = Remote::new();
        let active = remote.begin_attempt(None, None).unwrap();
        let (port, peer) = silent_peer();
        attempt_seam::arm(Stage::AfterRegister, |socket| {
            socket.shutdown(std::net::Shutdown::Both).unwrap();
        });
        let mut result = Ok(());
        let logs = capture_logs(|| result = open(port, &active));
        let error = result.expect_err("the closed socket ends the open");
        assert_ne!(error.to_string(), ATTEMPT_RETIRED);
        assert!(
            attempt_seam::reached(Stage::BeforeOpen),
            "the active attempt went on"
        );
        assert!(
            warn_lines(&logs)
                .iter()
                .any(|line| line.contains("failed to set read timeout")),
            "the active attempt warned:\n{logs}"
        );
        peer.join().unwrap();

        let logs = capture_logs(|| result = cancelled_at(Stage::AfterRegister));
        let error = result.expect_err("the cancel ends the open");
        assert_eq!(error.to_string(), ATTEMPT_RETIRED);
        assert!(
            !attempt_seam::reached(Stage::BeforeOpen),
            "it returned at the timeout"
        );
        assert!(warn_lines(&logs).is_empty(), "no warning:\n{logs}");
    }
}
