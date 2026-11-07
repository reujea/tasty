//! 거절 응답을 보낸 연결을 닫기 전에 상대가 이미 보낸 바이트를 비운다.
//!
//! 읽지 않은 수신 데이터가 남은 소켓을 닫으면 Linux 와 Windows 모두 FIN 대신 RST 를 보낸다. 차이는
//! 받는 쪽이다. Linux 는 RST 전에 도착한 데이터를 먼저 읽게 한 뒤 연결 재설정을 알린다. Windows 는
//! RST 를 받으면 아직 읽지 않은 수신 데이터까지 버리고 `WSAECONNRESET` 을 돌려준다. 그래서 요청을
//! 보내던 Windows 클라이언트는 거절 응답을 읽지 못할 수 있다. macOS CI 에서도 요청을 보내던
//! 클라이언트가 거절 응답 대신 `ECONNRESET` 을 받았다(닫기 전 단계가 없던 때). 받는 쪽이 응답을
//! 버리는 경우에도 성립하도록 macOS 는 Windows 와 같은 경로를 쓴다.
//!
//! 세 경로가 있다.
//! - [`drain_before_close`]: 연결 스레드의 -32060 거절(Linux 밖, 즉 Windows 와 macOS). 쓰기 쪽을 먼저 닫고(FIN) 상대가 보내는
//!   바이트를 짧게 기다리며 읽어 버린다. 시간과 바이트에 상한을 둔다.
//! - [`half_close_and_discard_arrived`]: 연결 스레드의 -32060 거절(Linux). 쓰기 쪽을 먼저 닫고 이미
//!   도착한 바이트만 기다리지 않고 읽어 버린다. 줄 상한을 넘은 요청의 나머지가 수신 큐에 남은 채
//!   닫으면, 요청을 다 보낸 Linux 클라이언트도 응답을 읽은 뒤 연결 재설정을 받는다.
//! - [`discard_arrived_then_close`]: accept 스레드의 -32062 포화 거절(모든 OS). accept 를 막지 않도록
//!   기다리지 않고 이미 도착한 바이트만 읽어 버린다. 닫은 뒤에 도착한 바이트는 여전히 RST 를 부른다.
//!
//! 두 경로 모두 상한을 넘으면 그대로 닫으며, 그때의 RST 는 받아들인다(최선 노력 범위 밖).

use std::io::Read;
use std::net::{Shutdown, TcpStream};
#[cfg(not(target_os = "linux"))]
use std::time::{Duration, Instant};

/// 비우는 전체 시간 상한.
#[cfg(not(target_os = "linux"))]
const DRAIN_WINDOW: Duration = Duration::from_millis(500);
/// 새 바이트가 이만큼 오지 않으면 상대가 보내기를 마친 것으로 보고 멈춘다.
#[cfg(not(target_os = "linux"))]
const DRAIN_IDLE: Duration = Duration::from_millis(50);
/// -32060 경로가 비우는 바이트 상한. 요청 한 줄의 상한과 같다.
const DRAIN_MAX_BYTES: usize = crate::ipc::protocol::MAX_REQUEST_LINE_BYTES;
/// -32062 경로가 비우는 바이트 상한. 과부하 때 accept 스레드에서 읽으므로 요청 줄 상한보다 작게 둔다.
/// 이보다 큰 요청을 이미 보낸 클라이언트는 거절 대신 연결 재설정을 받을 수 있다.
const SATURATED_DISCARD_MAX_BYTES: usize = 1024 * 1024;

#[cfg(not(target_os = "linux"))]
pub(super) fn drain_before_close(stream: &mut TcpStream) {
    if let Err(e) = stream.shutdown(Shutdown::Write) {
        tracing::debug!("IPC refusal half-close failed: {e}");
        return;
    }
    if let Err(e) = stream.set_read_timeout(Some(DRAIN_IDLE)) {
        tracing::debug!("IPC refusal drain could not set a read timeout: {e}");
        return;
    }
    let deadline = Instant::now() + DRAIN_WINDOW;
    let mut drained = 0usize;
    let mut buf = [0u8; 64 * 1024];
    while Instant::now() < deadline && drained < DRAIN_MAX_BYTES {
        match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => drained += n,
            // 기다려도 오지 않으면(시간 초과) 상대가 보내기를 마친 것이다.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return;
            }
            Err(e) => {
                tracing::debug!("IPC refusal drain ended on a read error: {e}");
                return;
            }
        }
    }
    tracing::debug!("IPC refusal drain stopped at its limit after {drained} bytes");
}

/// 포화 거절을 쓴 연결의 쓰기 쪽을 닫고, 이미 도착한 바이트만 읽어 버린 뒤 닫는다.
/// `stream` 은 non-blocking 이어야 한다. 읽을 것이 없으면(`WouldBlock`) 바로 멈추므로 accept 스레드를
/// 기다리게 하지 않는다. 계속 보내는 상대도 [`SATURATED_DISCARD_MAX_BYTES`] 에서 멈춘다.
pub(super) fn discard_arrived_then_close(mut stream: TcpStream) {
    match stream
        .shutdown(Shutdown::Write)
        .and_then(|()| discard_arrived(&mut stream, SATURATED_DISCARD_MAX_BYTES))
    {
        Ok(None) => {}
        Ok(Some(n)) => {
            tracing::debug!("IPC saturation refusal discard stopped at its limit after {n} bytes")
        }
        Err(e) => tracing::debug!("IPC saturation refusal half-close or discard failed: {e}"),
    }
}

/// -32060 거절을 쓴 연결의 쓰기 쪽을 닫고, 이미 도착한 바이트만 기다리지 않고 읽어 버린다(Linux).
/// 소켓은 호출한 쪽이 닫는다. 연결 스레드에서 부르지만 Windows 처럼 기다리지는 않는다.
#[cfg(target_os = "linux")]
pub(super) fn half_close_and_discard_arrived(stream: &mut TcpStream) {
    match stream
        .set_nonblocking(true)
        .and_then(|()| stream.shutdown(Shutdown::Write))
        .and_then(|()| discard_arrived(stream, DRAIN_MAX_BYTES))
    {
        Ok(None) => {}
        Ok(Some(n)) => {
            tracing::debug!("IPC oversize refusal discard stopped at its limit after {n} bytes")
        }
        Err(e) => tracing::debug!("IPC oversize refusal half-close or discard failed: {e}"),
    }
}

/// 기다리지 않고 읽을 수 있는 만큼 읽어 버린다. 상한에서 멈췄으면 읽은 바이트 수를 돌려준다.
fn discard_arrived(stream: &mut TcpStream, limit: usize) -> std::io::Result<Option<usize>> {
    let mut drained = 0usize;
    let mut buf = [0u8; 64 * 1024];
    while drained < limit {
        match stream.read(&mut buf) {
            Ok(0) => return Ok(None),
            Ok(n) => drained += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(Some(drained))
}
