//! 스트림 연결의 쓰기 스레드. sink 큐를 소켓에 쓰고 끝나면 소켓을 닫는다.

use super::*;

impl TcpIpcServer {
    /// 읽지 않는 client에 막힌 쓰기도 읽기 timeout과 같은 기한에 끝낸다. 기한 동안 한 바이트도
    /// 받지 않는 client는 읽기 쪽의 생존 판정으로도 끊긴 연결이다. 이 기한이 없으면 sink를 지워도
    /// 막힌 writer가 깨지 않아 연결 정리(`finish_stream_connection`의 join)가 끝나지 않는다.
    pub(super) fn arm_stream_write_timeout(writer: &std::net::TcpStream, timeout: Duration) {
        if let Err(e) = writer.set_write_timeout(Some(timeout)) {
            tracing::warn!("stream client: failed to set write timeout: {e}");
        }
    }

    /// 서버가 끝내는 연결의 프레임. client는 Detach와 그보다 먼저 오는 `force_detached` Control
    /// 어느 쪽을 받아도 연결을 닫는다(`notify_detached`).
    pub(super) fn frame_ends_connection(tag: StreamTag, payload: &[u8]) -> bool {
        match tag {
            StreamTag::Detach => true,
            StreamTag::Control => stream::force_detach_notice(payload).is_some(),
            _ => false,
        }
    }

    /// 프레임 하나를 쓴다. 쓰기를 이어 가면 true, 실패했거나 Detach를 썼으면 false다.
    /// client는 연결을 끝내는 프레임을 받자마자 닫을 수 있다. 표지를 쓰기 전에 세워야 읽기 쪽이
    /// 뒤따르는 EOF를 먼저 처리해도 정상 종료로 본다. 끝내는 프레임을 끝까지 쓴 경우에만
    /// `wrote_ending`을 세운다. 소켓을 어떻게 닫을지는 표지가 아니라 이 값으로 정한다.
    pub(super) fn write_stream_frame(
        w: &mut std::net::TcpStream,
        tag: StreamTag,
        payload: &[u8],
        client_id: StreamClientId,
        server_ending: &AtomicBool,
        wrote_ending: &mut bool,
    ) -> bool {
        let ends = Self::frame_ends_connection(tag, payload);
        if ends {
            server_ending.store(true, Ordering::Release);
        }
        if let Err(e) = stream::write_frame(w, tag, payload) {
            tracing::warn!(
                "stream client {client_id}: {tag:?} 프레임 송신 실패로 쓰기를 멈춘다(수신 timeout과 별개인 송신 오류): {e}"
            );
            return false;
        }
        *wrote_ending |= ends;
        tag != StreamTag::Detach
    }

    /// 실제 데이터가 없을 때 주기 Ping을 보내 상대의 읽기 timeout을 갱신한다.
    /// 서버가 연결을 끝내는 프레임은 쓰기 전에 `server_ending`을 세워 읽기 쪽이 뒤따르는 EOF를
    /// 정상 종료로 본다.
    pub(super) fn spawn_stream_write_thread(
        writer: std::net::TcpStream,
        sink_rx: tasty_ipc::stream_hub::SinkReceiver,
        client_id: StreamClientId,
        server_ending: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        let mut w = writer;
        thread::spawn(move || {
            let mut wrote_ending = false;
            loop {
                match sink_rx.recv_held_timeout(stream::HEARTBEAT_INTERVAL) {
                    Ok(frame) => {
                        if !frame.is_current() {
                            continue;
                        }
                        if !Self::write_stream_frame(
                            &mut w,
                            frame.tag,
                            &frame.payload,
                            client_id,
                            &server_ending,
                            &mut wrote_ending,
                        ) {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Err(e) = stream::write_frame(&mut w, StreamTag::Ping, &[]) {
                            tracing::warn!(
                                "stream client {client_id}: 하트비트 Ping 송신 실패로 쓰기를 멈춘다(수신 timeout과 별개인 송신 오류): {e}"
                            );
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break, // unregister 됨
                }
            }
            Self::close_after_writer(&w, client_id, wrote_ending);
        })
    }

    /// writer가 끝나면 이 연결은 더 쓸 수 없으므로 소켓을 닫는다.
    /// 끝내는 프레임을 끝까지 쓴 연결은 쓰기 쪽만 닫는다. client가 통지를 다 읽은 뒤 EOF를 보고,
    /// 읽기 쪽은 `run_stream_read_loop`가 그 뒤 첫 프레임이나 client의 EOF에서 끝낸다.
    /// 그 밖의 연결(sink 해제, 끝내는 프레임을 포함한 송신 실패)은 양쪽을 닫는다. 읽기 스레드가
    /// 들고 있는 사본도 깨워야 client가 EOF를 보고 읽기 루프도 끝난다.
    pub(super) fn close_after_writer(
        w: &std::net::TcpStream,
        client_id: StreamClientId,
        wrote_ending: bool,
    ) {
        let how = if wrote_ending {
            std::net::Shutdown::Write
        } else {
            std::net::Shutdown::Both
        };
        if let Err(e) = w.shutdown(how) {
            tracing::debug!("stream client {client_id}: 연결 종료({how:?}) 실패: {e}");
        }
    }
}
