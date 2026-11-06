//! 연결만 열고 요청을 보내지 않는 상대가 연결 슬롯을 계속 차지하지 못하도록 첫 줄에 기한을 둔다.
//! 첫 줄 이후 요청 간 대기는 호환성을 위해 제한하지 않는다(docs/architecture/ipc-server.md).
//! 읽기마다 남은 시간으로 timeout을 설정해 한 바이트씩 보내도 첫 줄의 기한이 늘어나지 않게 한다.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// 첫 줄의 제한 시간. 연결 슬롯 보호용 값이며 attach heartbeat와 무관하다.
pub(super) const FIRST_LINE_IDLE_TIMEOUT: Duration = Duration::from_secs(20);

/// 디버그 시험은 환경변수로 기한을 줄일 수 있다. 제품값은 FIRST_LINE_IDLE_TIMEOUT이다.
#[cfg(debug_assertions)]
pub(super) fn first_line_idle_timeout() -> Duration {
    super::debug_env_usize("TASTY_DEBUG_IPC_FIRST_LINE_IDLE_MS")
        .map_or(FIRST_LINE_IDLE_TIMEOUT, |ms| {
            Duration::from_millis(ms as u64)
        })
}

#[cfg(not(debug_assertions))]
pub(super) fn first_line_idle_timeout() -> Duration {
    FIRST_LINE_IDLE_TIMEOUT
}

/// 새 소켓 읽기마다 남은 시간으로 timeout을 설정한다. 이미 버퍼에 있는 바이트는 그대로 내준다.
/// 소켓 옵션은 래퍼가 사라져도 남으므로 첫 줄 뒤 clear_read_timeout을 호출해야 한다.
pub(super) struct DeadlineReader<'a> {
    inner: &'a mut BufReader<TcpStream>,
    deadline: Instant,
}

impl<'a> DeadlineReader<'a> {
    pub(super) fn new(inner: &'a mut BufReader<TcpStream>, within: Duration) -> Self {
        Self {
            inner,
            deadline: Instant::now() + within,
        }
    }

    fn arm_remaining(&self) -> std::io::Result<()> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        // 0 은 `set_read_timeout` 이 거절하는 값이라 여기서 기한 만료로 가른다.
        if remaining.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.inner.get_ref().set_read_timeout(Some(remaining))
    }
}

impl Read for DeadlineReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.inner.buffer().is_empty() {
            self.arm_remaining()?;
        }
        self.inner.read(buf)
    }
}

impl BufRead for DeadlineReader<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.inner.buffer().is_empty() {
            self.arm_remaining()?;
        }
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt);
    }
}

/// 이 오류가 읽기 기한 만료인가. 플랫폼마다 `WouldBlock`(Unix) 또는 `TimedOut`(Windows)으로 온다.
pub(super) fn is_idle_expiry(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// timeout을 지우지 못하면 요청 사이의 대기도 제한되므로 연결을 닫는다.
pub(super) fn clear_read_timeout(reader: &BufReader<TcpStream>) -> bool {
    match reader.get_ref().set_read_timeout(None) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("IPC could not clear the first-line read timeout, closing: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    fn pair() -> (TcpStream, BufReader<TcpStream>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let client = TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (server, _) = listener.accept().expect("accept");
        (client, BufReader::new(server))
    }

    fn read_line_within(
        reader: &mut BufReader<TcpStream>,
        within: Duration,
    ) -> std::io::Result<usize> {
        let mut line = String::new();
        DeadlineReader::new(reader, within).read_line(&mut line)
    }

    #[test]
    fn a_silent_peer_expires_at_the_deadline() {
        let (_client, mut reader) = pair();
        let started = Instant::now();
        let err = read_line_within(&mut reader, Duration::from_millis(100)).expect_err("expires");
        assert!(is_idle_expiry(&err), "{err:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the deadline did not bound the read"
        );
    }

    // 조금씩 바이트가 들어와도 첫 줄 전체의 기한은 늘어나면 안 된다.
    #[test]
    fn a_trickling_peer_cannot_stretch_the_deadline() {
        let (mut client, mut reader) = pair();
        let feeder = std::thread::spawn(move || {
            for _ in 0..40 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        let started = Instant::now();
        let err = read_line_within(&mut reader, Duration::from_millis(200)).expect_err("expires");
        assert!(is_idle_expiry(&err), "{err:?}");
        assert!(
            started.elapsed() < Duration::from_millis(900),
            "each byte restarted the timeout — the deadline is per read, not per line ({:?})",
            started.elapsed()
        );
        drop(reader);
        feeder.join().expect("feeder");
    }

    #[test]
    fn a_line_inside_the_deadline_is_read_and_the_timeout_can_be_cleared() {
        let (mut client, mut reader) = pair();
        client.write_all(b"{\"a\":1}\n").expect("write");
        let mut line = String::new();
        DeadlineReader::new(&mut reader, Duration::from_secs(5))
            .read_line(&mut line)
            .expect("read");
        assert_eq!(line, "{\"a\":1}\n");
        assert!(clear_read_timeout(&reader));
        assert_eq!(reader.get_ref().read_timeout().expect("opt"), None);
    }
}
