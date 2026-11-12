//! 터널 너머 원격 포트에 리스너가 없는지 판정한다.
//!
//! `ssh -L`은 로컬 리스너를 먼저 열기 때문에 원격 포트가 비어 있어도 터널 수립은 성공한다. 원격 포트가
//! 비어 있다는 사실은 첫 연결 때만 드러난다. ssh가 그 연결의 채널을 열지 못하고 stderr에
//! `channel N: open failed: connect failed: <원격의 사유>`를 쓴 뒤 로컬 연결을 닫는다. 로컬에서는
//! EOF(Linux·macOS)나 연결 재설정(Windows os error 10054)으로 보인다.
//!
//! 판정 근거는 둘 다 필요하다. stderr 줄의 `open failed: connect failed` 부분은 ssh 클라이언트가
//! 채널 열기 거절 사유 코드(SSH2_OPEN_CONNECT_FAILED)를 고정 문자열로 바꾼 것이라 OS·로케일과
//! 관계없다. 그 뒤의 사유 문구(`Connection refused` 등)는 원격 OS가 만들어 OS마다 다르므로 보지 않는다.
//! 포워드 대상이 원격의 loopback이라 그 연결 실패는 그 포트에 받을 프로세스가 없다는 뜻이다.
//! 리스너는 있는데 IPC 응답이 이상한 경우는 stderr 줄이 없으므로 이 판정에 걸리지 않는다.

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// IPC 실패 뒤 stderr 줄이 도착하기를 기다리는 상한. ssh는 거절 줄을 쓴 뒤 로컬 연결을 닫지만
/// 읽기 스레드가 그 줄을 처리하기 전에 호출자가 실패를 볼 수 있다.
const VERDICT_GRACE: Duration = Duration::from_millis(500);

/// ssh stderr 한 줄이 원격 포트의 연결 실패로 채널을 열지 못했다는 보고인지.
pub(crate) fn is_channel_connect_failure(line: &str) -> bool {
    line.trim_start().starts_with("channel ") && line.contains("open failed: connect failed")
}

/// 연결이 응답 없이 끊겼음을 나타내는 입출력 오류 종류인지.
fn is_dropped_kind(kind: std::io::ErrorKind) -> bool {
    use std::io::ErrorKind as K;
    matches!(
        kind,
        K::UnexpectedEof | K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe
    )
}

/// 오류 사슬에 연결 끊김(EOF·재설정·중단·파이프 끊김)이 있는지.
pub fn is_dropped_connection(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|io| is_dropped_kind(io.kind()))
}

/// 두 근거로 리스너 없음을 정한다. 연결이 끊겼고 ssh가 채널 연결 실패를 보고했어야 한다.
pub(crate) fn listener_absent(channel_connect_failed: bool, connection_dropped: bool) -> bool {
    channel_connect_failed && connection_dropped
}

/// 응답 없이 연결이 닫혔을 때 쓰는 오류. 사슬에서 [`is_dropped_connection`]이 알아본다.
pub fn connection_closed_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        message.into(),
    ))
}

/// stderr 바이트를 줄로 모아 채널 연결 실패 줄을 찾는다.
#[derive(Default)]
struct LineScanner {
    line: Vec<u8>,
}

impl LineScanner {
    /// 한 줄이 이 길이를 넘으면 뒤는 버린다. 판정할 줄은 짧다.
    const MAX_LINE: usize = 1024;

    fn feed(&mut self, bytes: &[u8], seen: &AtomicBool) {
        for &b in bytes {
            if b == b'\n' {
                self.finish_line(seen);
            } else if self.line.len() < Self::MAX_LINE {
                self.line.push(b);
            }
        }
    }

    fn finish_line(&mut self, seen: &AtomicBool) {
        if is_channel_connect_failure(&String::from_utf8_lossy(&self.line)) {
            seen.store(true, Ordering::Release);
        }
        self.line.clear();
    }
}

/// 터널 ssh의 stderr를 사용자 stderr로 그대로 넘기면서 채널 연결 실패 줄을 표시한다. stderr가 닫힐
/// 때(ssh 종료)까지 읽는다. 넘기기에 실패해도 읽기는 계속한다. 멈추면 ssh가 파이프에서 막힌다.
pub(crate) fn relay_stderr(mut from: impl Read, mut to: impl Write, seen: &AtomicBool) {
    let mut buf = [0u8; 4096];
    let mut scanner = LineScanner::default();
    let mut relaying = true;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::debug!(%e, "ssh tunnel stderr read ended");
                break;
            }
        };
        if relaying && let Err(e) = to.write_all(&buf[..n]).and_then(|()| to.flush()) {
            tracing::debug!(%e, "ssh tunnel stderr relay stopped");
            relaying = false;
        }
        scanner.feed(&buf[..n], seen);
    }
    scanner.finish_line(seen);
}

/// 실패가 연결 끊김이면 stderr 표시를 잠시 기다려 리스너 없음을 정한다.
pub(crate) fn judge_after_failure(seen: &Arc<AtomicBool>, failure: &anyhow::Error) -> bool {
    let dropped = is_dropped_connection(failure);
    if !dropped {
        return false;
    }
    let deadline = Instant::now() + VERDICT_GRACE;
    loop {
        let refused = seen.load(Ordering::Acquire);
        if refused || Instant::now() >= deadline {
            return listener_absent(refused, dropped);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 원격 포트에 리스너가 없다는 판정. 표시 문구는 원인 추정(원격 tasty 미실행 또는 오래된 포트 파일)을 담는다.
#[derive(Debug)]
pub struct RemoteListenerAbsent {
    pub remote_port: u16,
}

impl std::fmt::Display for RemoteListenerAbsent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            tasty_i18n::t_fmt("ssh.tunnel.listener_absent", &self.remote_port.to_string())
        )
    }
}

impl std::error::Error for RemoteListenerAbsent {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_side_part_of_the_open_failure_line_is_matched() {
        for line in [
            "channel 1: open failed: connect failed: Connection refused",
            "channel 2: open failed: connect failed: Connection refused\r",
            // 원격 Windows 등 사유 문구가 다른 OS.
            "channel 3: open failed: connect failed: No connection could be made",
            "channel 4: open failed: connect failed: ",
        ] {
            assert!(is_channel_connect_failure(line), "{line}");
        }
        for line in [
            "channel 1: open failed: administratively prohibited: open failed",
            "Warning: Permanently added 'host' to the list of known hosts.",
            "connect failed: Connection refused",
            "",
        ] {
            assert!(!is_channel_connect_failure(line), "{line}");
        }
    }

    #[test]
    fn dropped_connections_include_eof_reset_abort_and_broken_pipe() {
        use std::io::ErrorKind as K;
        for kind in [
            K::UnexpectedEof,
            K::ConnectionReset,
            K::ConnectionAborted,
            K::BrokenPipe,
        ] {
            let e = anyhow::Error::new(std::io::Error::from(kind)).context("read failed");
            assert!(is_dropped_connection(&e), "{kind:?}");
        }
        for kind in [K::TimedOut, K::WouldBlock, K::ConnectionRefused] {
            let e = anyhow::Error::new(std::io::Error::from(kind));
            assert!(!is_dropped_connection(&e), "{kind:?}");
        }
        assert!(is_dropped_connection(&connection_closed_error("eof")));
        assert!(!is_dropped_connection(&anyhow::anyhow!(
            "IPC error response (code=-32601)"
        )));
    }

    #[test]
    fn a_listener_is_absent_only_with_both_signals() {
        assert!(listener_absent(true, true));
        assert!(!listener_absent(true, false));
        assert!(!listener_absent(false, true));
        assert!(!listener_absent(false, false));
    }

    /// 넘긴 바이트는 원문 그대로이고, 줄이 여러 read에 걸쳐 나뉘어도 알아본다.
    #[test]
    fn the_relay_forwards_everything_and_marks_a_split_failure_line() {
        struct Chunks(Vec<&'static [u8]>);
        impl Read for Chunks {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0.is_empty() {
                    return Ok(0);
                }
                let chunk = self.0.remove(0);
                buf[..chunk.len()].copy_from_slice(chunk);
                Ok(chunk.len())
            }
        }
        let input: Vec<&'static [u8]> = vec![
            b"Warning: added host\nchannel 1: open fa",
            b"iled: connect failed: Connection refused\n",
        ];
        let expected: Vec<u8> = input.concat();
        let seen = AtomicBool::new(false);
        let mut out = Vec::new();
        relay_stderr(Chunks(input), &mut out, &seen);
        assert_eq!(out, expected);
        assert!(seen.load(Ordering::Acquire));

        let seen = AtomicBool::new(false);
        let mut out = Vec::new();
        relay_stderr(
            Chunks(vec![b"Warning: added host\n" as &[u8]]),
            &mut out,
            &seen,
        );
        assert!(!seen.load(Ordering::Acquire));
    }

    #[test]
    fn the_verdict_needs_a_dropped_connection_and_the_marked_line() {
        let marked = Arc::new(AtomicBool::new(true));
        let unmarked = Arc::new(AtomicBool::new(false));
        let dropped = connection_closed_error("eof");
        let other = anyhow::anyhow!("parse failed");
        assert!(judge_after_failure(&marked, &dropped));
        assert!(!judge_after_failure(&marked, &other));
        assert!(!judge_after_failure(&unmarked, &dropped));
    }
}
