//! 다른 스레드가 같은 소켓에서 막혀 읽고 있을 때, 그 reader까지 끝내려고 양쪽을 닫는 함수.
//!
//! Linux와 macOS에서는 `shutdown(Both)`가 다른 스레드의 막힌 `recv`를 바로 돌려보낸다.
//! Windows에서는 돌려보내지 않아 reader가 읽기 기한이나 상대의 데이터까지 막힌다. 그래서
//! Windows에서는 닫은 뒤 그 소켓의 진행 중 I/O에 취소를 요청한다. 막혀 있던 읽기는 오류
//! (관측값 `WSAEINTR` 10004)로 돌아오고, 그 뒤 새로 거는 읽기는 수신 종료라 바로 EOF다.
//! 취소는 진행 중인 쓰기도 끝낼 수 있으므로 reader를 끝내려는 자리에서만 쓴다.
//! 쓰기 쪽만 닫아 상대가 남은 프레임을 다 읽게 하는 자리에는 쓰지 않는다.

use std::io;
use std::net::{Shutdown, TcpStream};

/// `shutdown(Both)`를 하고 그 결과를 그대로 돌려준다. Windows에서는 결과와 상관없이 이어서
/// 이 소켓의 진행 중 I/O에 취소를 요청한다. 취소 결과는 반환값에 섞지 않는다.
/// 취소는 완료를 기다리지 않으므로, 이 함수가 돌아온 시점에 reader가 끝났다는 보장은 없다.
pub fn shutdown_both_and_wake(stream: &TcpStream) -> io::Result<()> {
    let shut = stream.shutdown(Shutdown::Both);
    #[cfg(windows)]
    if let Err(error) = cancel_pending_io(stream) {
        tracing::debug!(%error, "cancelling pending socket I/O after shutdown failed");
    }
    shut
}

/// 취소할 요청이 없었던 경우(`ERROR_NOT_FOUND`)는 성공으로 본다.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "std has no way to cancel a recv blocked in another thread on Windows; the FFI call borrows the socket and is bounded to this function"
)]
fn cancel_pending_io(stream: &TcpStream) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, HANDLE};
    use windows_sys::Win32::System::IO::CancelIoEx;

    let handle = stream.as_raw_socket() as usize as HANDLE;
    // SAFETY: the borrow keeps the socket open for this synchronous call. A SOCKET is a
    // kernel handle and is passed at pointer width. The null OVERLAPPED pointer asks to
    // cancel every pending request on the handle. The call neither takes nor closes it.
    let ok = unsafe { CancelIoEx(handle, std::ptr::null()) };
    if ok != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
        return Ok(());
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::shutdown_both_and_wake;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// 취소로 돌아온 읽기의 OS 코드. Windows의 10004(`WSAEINTR`, 관측값)와
    /// 995(`ERROR_OPERATION_ABORTED`)다. std는 995와 기한 만료 10060을 같은 `TimedOut`으로
    /// 분류하므로 종류가 아니라 코드와 시각으로 가른다.
    const CANCELLED: &[i32] = if cfg!(windows) { &[10_004, 995] } else { &[] };
    const READ_LIMIT: Duration = Duration::from_secs(5);
    /// 기한(5s)보다 충분히 이르다. 이보다 늦은 취소 코드는 깨움으로 보지 않는다.
    const WOKEN_WITHIN: Duration = Duration::from_secs(1);

    #[test]
    fn shutting_both_sides_ends_a_read_already_blocked_in_another_thread() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let peer = TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (socket, _) = listener.accept().expect("accept");
        let reader_copy = socket.try_clone().expect("clone");
        reader_copy
            .set_read_timeout(Some(READ_LIMIT))
            .expect("read timeout");
        let (ready_tx, ready_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut copy = reader_copy;
            let mut byte = [0u8; 1];
            ready_tx.send(()).expect("ready");
            let result = copy.read(&mut byte);
            (Instant::now(), result)
        });
        ready_rx.recv_timeout(READ_LIMIT).expect("reader ready");
        // 준비 신호는 recv 진입의 증거가 아니다. 들어갈 시간을 더 둔다.
        std::thread::sleep(Duration::from_millis(50));

        let called = Instant::now();
        shutdown_both_and_wake(&socket).expect("shutdown");
        let (ended, result) = reader.join().expect("reader thread");
        let took = ended.saturating_duration_since(called);
        match &result {
            Ok(0) => {}
            Err(error)
                if error
                    .raw_os_error()
                    .is_some_and(|code| CANCELLED.contains(&code))
                    && took < WOKEN_WITHIN => {}
            other => panic!("the blocked read was not woken: {other:?} after {took:?}"),
        }
        // 상대는 결과를 본 뒤에 닫는다. 상대의 FIN이 reader를 깨운 것으로 오인하지 않기 위해서다.
        drop(peer);
    }
}
