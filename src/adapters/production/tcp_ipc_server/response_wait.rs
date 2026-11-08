//! 연결 스레드가 주 스레드의 응답을 기다리는 동안 클라이언트가 떠났는지 함께 본다.
//!
//! 주 스레드가 오래 응답하지 못하면 응답을 기다리는 연결 스레드가 연결 자리를 쥔 채 남는다.
//! 클라이언트가 그사이 죽어 소켓을 닫아도 계속 기다리면 자리가 돌아오지 않아 연결 상한에 닿는다.
//! 그래서 [`PEER_CHECK_INTERVAL`] 마다 소켓을 들여다보고, 상대가 닫았으면 기다리기를 멈춘다.
//!
//! 상대가 닫았는지는 non-blocking `peek` 로 본다. 0 바이트(EOF)나 오류면 닫힌 것이다. 다음 요청을
//! 미리 보낸 클라이언트는 읽을 바이트가 있으므로 살아 있는 것으로 본다. TCP 에서는 쓰기 쪽만 닫고
//! 응답을 기다리는 클라이언트(half-close)와 완전히 닫은 클라이언트를 구분할 수 없다. 앞의 경우도
//! 닫힌 것으로 보므로, 그런 클라이언트는 이 간격보다 오래 걸리는 요청의 응답을 받지 못한다.

use std::net::TcpStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ipc::protocol::JsonRpcResponse;

/// 응답을 기다리는 동안 상대가 닫았는지 확인하는 간격.
pub(super) const PEER_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// 응답 기다리기의 결과.
pub(super) enum Waited {
    Answer(JsonRpcResponse),
    /// 호출자가 정한 대기 기한이 지났다.
    Expired,
    /// 응답 송신 측이 응답 없이 사라졌다.
    SenderDropped,
    /// 기다리는 동안 클라이언트가 연결을 닫았다.
    PeerClosed,
}

/// 응답을 기다린다. `bound` 가 없으면 응답이나 상대의 닫힘이 올 때까지 기다린다.
pub(super) fn wait_for_response(
    resp_rx: &mpsc::Receiver<JsonRpcResponse>,
    bound: Option<Duration>,
    socket: &TcpStream,
) -> Waited {
    let deadline = bound.map(|b| Instant::now() + b);
    loop {
        let slice = match deadline {
            None => PEER_CHECK_INTERVAL,
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Waited::Expired;
                }
                left.min(PEER_CHECK_INTERVAL)
            }
        };
        match resp_rx.recv_timeout(slice) {
            Ok(response) => return Waited::Answer(response),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Waited::SenderDropped,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if peer_closed(socket) {
            return Waited::PeerClosed;
        }
    }
}

/// 상대가 연결을 닫았으면 true. 확인하는 동안만 소켓을 non-blocking 으로 바꾼다.
/// 응답을 기다리는 동안에는 이 스레드 말고 소켓을 읽거나 쓰는 쪽이 없다.
fn peer_closed(socket: &TcpStream) -> bool {
    if let Err(e) = socket.set_nonblocking(true) {
        tracing::warn!("IPC peer check could not switch the socket to non-blocking: {e}");
        return false;
    }
    let closed = peek_shows_closed(socket);
    if let Err(e) = socket.set_nonblocking(false) {
        // blocking 으로 돌리지 못한 소켓으로는 응답을 온전히 쓸 수 없다. 연결을 끝낸다.
        tracing::warn!("IPC peer check could not restore blocking mode: {e}");
        return true;
    }
    closed
}

/// non-blocking 소켓을 한 바이트 들여다본다. EOF 나 읽기 오류면 닫힌 것이다.
fn peek_shows_closed(socket: &TcpStream) -> bool {
    let mut byte = [0u8; 1];
    match socket.peek(&mut byte) {
        Ok(n) => n == 0,
        Err(e) => match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => false,
            _ => {
                tracing::debug!(
                    "IPC peer check read error, treating the connection as closed: {e}"
                );
                true
            }
        },
    }
}
