//! 취소된 시도의 연결 단계는 소켓 기한 설정이나 open 실패를 경고 없이 취소 오류로 끝낸다. 활성 시도는
//! 지금처럼 경고 뒤 진행하거나(attach) 원 오류를 돌려준다(browse).
//!
//! 실제 소켓에서 기한 설정이 실패하는 것은 macOS 뿐이다(양쪽을 닫은 소켓의 옵션 설정을 `EINVAL` 로 거절한다).
//! Linux 는 그 설정이 성공해 open 까지 가므로, 사전 기한 설정 분기는 macOS 의 시험과 설정 결과를 넣는
//! [`arm_timeouts_with`] 시험이 잰다. 로그를 단언하는 시험은 tracing 콜사이트 관심 캐시 때문에 혼자 돈다
//! (`docs/dev-guide/unit-test-isolation.md` "tracing 콜사이트 관심 캐시").

use crate::attempt_seam::{self, Stage};
use crate::outbound::{ATTEMPT_RETIRED, AttemptToken, Remote, retired_or};
use crate::transport::arm_timeouts_with;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 이 범위의 tracing 이벤트를 문자열로 모은다.
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

/// 활성 시도와, 같은 `Remote` 에서 취소한 시도. `Remote` 가 살아 있어야 시도가 유효하다.
fn attempts() -> (Remote, AttemptToken, AttemptToken) {
    let mut remote = Remote::new();
    let active = remote.begin_attempt(None, None).unwrap();
    let retired = remote.begin_attempt(None, None).unwrap();
    remote.cancel_attempt(&retired);
    (remote, active, retired)
}

fn warn_probe(what: &str, e: &std::io::Error) {
    tracing::warn!("probe: failed to set {what}: {e}");
}

fn einval() -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::InvalidInput)
}

#[test]
fn a_failure_on_a_retired_attempt_reads_as_the_cancel() {
    let (_remote, active, retired) = attempts();
    assert_eq!(
        retired_or(Some(&retired), einval()).to_string(),
        ATTEMPT_RETIRED
    );
    assert_ne!(
        retired_or(Some(&active), einval()).to_string(),
        ATTEMPT_RETIRED
    );
    assert_ne!(retired_or(None, einval()).to_string(), ATTEMPT_RETIRED);
}

/// 읽기 기한 실패와 쓰기 기한 실패 두 분기. 활성 시도는 경고 뒤 다음 설정으로 진행하고, 취소된 시도는 그
/// 자리에서 경고 없이 취소 오류로 돌아온다. 활성 시도의 경고가 이 구독자에 닿는 것이 관측 범위의 근거다.
#[test]
fn a_timeout_failure_on_a_retired_attempt_returns_the_cancel_without_a_warning() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "a_timeout_failure_on_a_retired_attempt_returns_the_cancel_without_a_warning",
    ) {
        return;
    }
    let (_remote, active, retired) = attempts();
    for (read_fails, what) in [(true, "read timeout"), (false, "write timeout")] {
        let set_read = || if read_fails { Err(einval()) } else { Ok(()) };
        let set_write = || if read_fails { Ok(()) } else { Err(einval()) };

        let mut wrote = false;
        let logs = capture_logs(|| {
            arm_timeouts_with(
                Some(&active),
                set_read,
                || {
                    wrote = true;
                    set_write()
                },
                warn_probe,
            )
            .expect("an active attempt carries on");
        });
        assert!(
            wrote,
            "{what}: the active attempt went on to the write timeout"
        );
        let warns = warn_lines(&logs);
        assert_eq!(
            warns.len(),
            1,
            "{what}: one warning for the active attempt:\n{logs}"
        );
        assert!(
            warns[0].contains(&format!("failed to set {what}")),
            "{logs}"
        );

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
                warn_probe,
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

/// 연결을 받고 아무것도 보내지 않는 상대. 연결 함수가 소켓 닫기로 끝나므로 상대는 응답하지 않아도 된다.
fn silent_peer() -> (u16, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let mut buf = [0u8; 256];
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        while matches!(std::io::Read::read(&mut &socket, &mut buf), Ok(n) if n > 0) {}
    });
    (port, peer)
}

fn handshake(token: &AttemptToken, port: u16) -> anyhow::Result<()> {
    crate::transport::attach_handshake(port, 3, "retired test", token, Duration::from_secs(5))
        .map(|_| ())
}

/// B-2: 기한 설정 뒤 open 직전에 취소되면 open 실패가 취소 오류가 된다.
#[test]
fn an_attempt_cancelled_before_the_open_ends_the_handshake_as_retired() {
    let mut remote = Remote::new();
    let token = remote.begin_attempt(None, None).unwrap();
    let (port, peer) = silent_peer();
    let cancel = token.clone();
    attempt_seam::arm(Stage::BeforeOpen, move |_| cancel.cancel());
    let error = handshake(&token, port).expect_err("the cancel ends the handshake");
    assert!(attempt_seam::reached(Stage::BeforeOpen));
    assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    peer.join().unwrap();
}

/// B-1: 등록 직후 취소되면 결과는 취소 오류다. macOS 에서는 사전 기한 설정이 실패해 open 전에, 경고 없이
/// 돌아온다. 다른 OS 는 기한 설정이 성공해 open 에서 같은 오류로 돌아온다.
#[test]
fn an_attempt_cancelled_after_registering_ends_the_handshake_as_retired() {
    let mut remote = Remote::new();
    let token = remote.begin_attempt(None, None).unwrap();
    let (port, peer) = silent_peer();
    let cancel = token.clone();
    attempt_seam::arm(Stage::AfterRegister, move |_| cancel.cancel());
    let error = handshake(&token, port).expect_err("the cancel ends the handshake");
    assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    peer.join().unwrap();
}

/// macOS B-1: 같은 기한 설정 실패를 활성 시도(상대 없이 소켓만 닫음)와 취소된 시도로 지난다. 활성 시도는
/// 경고를 남기고 open 까지 가며, 취소된 시도는 open 전에 경고 없이 돌아온다.
#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_cancelled_handshake_stops_at_the_timeout_without_a_warning() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "on_macos_a_cancelled_handshake_stops_at_the_timeout_without_a_warning",
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
    let logs = capture_logs(|| result = handshake(&active, port));
    let error = result.expect_err("the closed socket ends the handshake");
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

    let retired = remote.begin_attempt(None, None).unwrap();
    let (port, peer) = silent_peer();
    let cancel = retired.clone();
    attempt_seam::arm(Stage::AfterRegister, move |_| cancel.cancel());
    let mut result = Ok(());
    let logs = capture_logs(|| result = handshake(&retired, port));
    let error = result.expect_err("the cancel ends the handshake");
    assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    assert!(
        !attempt_seam::reached(Stage::BeforeOpen),
        "it returned at the timeout"
    );
    assert!(warn_lines(&logs).is_empty(), "no warning:\n{logs}");
    peer.join().unwrap();
}

/// macOS browse B-1: 등록 직후 취소되면 읽기 기한 설정 실패가 경고 없이 취소 오류가 된다. 활성 시도는 그
/// 설정의 원 소켓 오류(`EINVAL`)를 그대로 돌려준다. 쓰기 기한 설정 분기는 읽기 설정이 먼저 실패해 닿지 않는다.
#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_cancelled_probe_reports_the_cancel_at_the_timeout() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "on_macos_a_cancelled_probe_reports_the_cancel_at_the_timeout",
    ) {
        return;
    }
    /// macOS 의 `EINVAL`.
    const EINVAL: i32 = 22;
    let probe = |attempt: &AttemptToken, port: u16| {
        crate::browse::probe_method_bound(port, "system.info", serde_json::json!({}), Some(attempt))
    };
    let mut remote = Remote::new();
    let active = remote.begin_attempt(None, None).unwrap();
    let (port, peer) = silent_peer();
    attempt_seam::arm(Stage::AfterRegister, |socket| {
        socket.shutdown(std::net::Shutdown::Both).unwrap();
    });
    let error = probe(&active, port).expect_err("the closed socket ends the probe");
    let socket_error = error
        .downcast_ref::<std::io::Error>()
        .unwrap_or_else(|| panic!("the active attempt keeps the socket error: {error:#}"));
    assert_eq!(
        socket_error.raw_os_error(),
        Some(EINVAL),
        "the read timeout setting's own error: {socket_error}"
    );
    peer.join().unwrap();

    let retired = remote.begin_attempt(None, None).unwrap();
    let (port, peer) = silent_peer();
    let cancel = retired.clone();
    attempt_seam::arm(Stage::AfterRegister, move |_| cancel.cancel());
    let mut result = Ok(serde_json::Value::Null);
    let logs = capture_logs(|| result = probe(&retired, port));
    let error = result.expect_err("the cancel ends the probe");
    assert_eq!(error.to_string(), ATTEMPT_RETIRED);
    assert!(warn_lines(&logs).is_empty(), "no warning:\n{logs}");
    peer.join().unwrap();
}
