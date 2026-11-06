//! Send bounded capture and bulk payloads on their original connection epoch.

#[cfg(test)]
mod tests;

use super::forward::RemoteTarget;
use super::wire::send_control_frame;
use crate::app::App;
use crate::ipc::client::StreamConnection;
use crate::ipc::stream;
use crate::ipc::stream::{STREAM_PROTO, StreamControl, StreamTag};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};

/// 원격은 client_id로도 구별하므로 업로드 ID는 프로세스 안에서 구별되면 된다.
static NEXT_CAPTURE_UPLOAD_ID: AtomicU64 = AtomicU64::new(1);

fn next_capture_upload_id() -> u64 {
    NEXT_CAPTURE_UPLOAD_ID.fetch_add(1, Ordering::Relaxed)
}

/// bulk ID도 원격 client_id와 함께 사용한다.
static NEXT_BULK_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

fn next_bulk_transfer_id() -> u64 {
    NEXT_BULK_TRANSFER_ID.fetch_add(1, Ordering::Relaxed)
}

/// binary sub-header를 더해도 프레임 상한을 넘지 않는 payload 크기.
const BULK_CHUNK_RAW_LEN: usize = stream::MAX_FRAME_LEN as usize - stream::BULK_CHUNK_HEADER_LEN;

/// 이미지 업로드 결과 처리가 원격 정책 거절과 전송 오류를 구별하는 접두사다.
/// 거절이면 Dismiss만, 나머지 오류면 Retry를 제공하므로 문구를 임의로 바꾸지 않는다.
pub(crate) const BULK_REJECT_PREFIX: &str = "remote rejected bulk upload: ";

/// base64 팽창과 JSON 오버헤드를 포함해 Control 프레임 상한 안에 들도록 여유를 둔다.
const CAPTURE_CHUNK_RAW_LEN: usize = 700 * 1024;

impl App {
    /// 캡처를 원격에 올리고 원격 클립보드에 경로를 넣도록 요청한다.
    /// StreamControl enum 밖의 capture_chunk/capture_commit 이벤트를 사용한다.
    /// commit 을 보낸 업로드만 `origin` View 와 함께 회신 대기 목록에 넣는다.
    pub(crate) fn forward_capture_to_remote_clipboard(
        &mut self,
        target: &RemoteTarget,
        file_name: &str,
        bytes: &[u8],
        origin: std::sync::Weak<()>,
    ) -> anyhow::Result<()> {
        if !self.remote_target_is_current(target) {
            anyhow::bail!("capture connection retired before delivery");
        }
        let frame_tx = target.sender.clone();
        let upload_id = next_capture_upload_id();

        use base64::Engine as _;
        let chunks: Vec<&[u8]> = if bytes.is_empty() {
            vec![&[][..]]
        } else {
            bytes.chunks(CAPTURE_CHUNK_RAW_LEN).collect()
        };
        let total = chunks.len() as u32;
        for (seq, chunk) in chunks.into_iter().enumerate() {
            let msg = serde_json::json!({
                "event": "capture_chunk",
                "upload_id": upload_id,
                "seq": seq as u32,
                "total": total,
                "data_b64": base64::engine::general_purpose::STANDARD.encode(chunk),
            });
            send_control_frame(&frame_tx, &msg)?;
        }
        let commit = serde_json::json!({
            "event": "capture_commit",
            "upload_id": upload_id,
            "file_name": file_name,
        });
        send_control_frame(&frame_tx, &commit)?;
        if let Some(session) = self
            .remote
            .sessions
            .iter_mut()
            .find(|session| session.state.local_workspace == target.workspace)
        {
            session.state.capture_uploads.insert(upload_id, origin);
        }
        Ok(())
    }
}

/// 파일을 transfer_id·seq 헤더가 있는 청크로 나눈다. 빈 입력은 청크 없이 begin·commit만 보낸다.
fn bulk_chunk_frames(transfer_id: u64, bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .chunks(BULK_CHUNK_RAW_LEN)
        .enumerate()
        .map(|(seq, part)| stream::encode_bulk_chunk(transfer_id, seq as u32, part))
        .collect()
}

/// bulk 채널이 붙을 곳과 소켓에 걸 무응답 제한. 제한은 전송을 시작할 때의 설정값이다.
#[derive(Clone, Copy)]
pub(crate) struct BulkEndpoint {
    pub port: u16,
    pub remote_ws: u32,
    pub heartbeat_timeout: std::time::Duration,
}

/// 기존 workspace 점유에 연결된 bulk 채널을 연다. timeout 설정 실패는 경고만 남긴다.
#[cfg(test)]
fn open_bulk_connection(endpoint: BulkEndpoint) -> anyhow::Result<StreamConnection> {
    open_bulk_connection_bound(endpoint, None)
}
fn open_bulk_connection_bound(
    endpoint: BulkEndpoint,
    attempt: Option<&tasty_remote::outbound::AttemptToken>,
) -> anyhow::Result<StreamConnection> {
    let BulkEndpoint {
        port,
        remote_ws,
        heartbeat_timeout,
    } = endpoint;
    let sock = TcpStream::connect(("127.0.0.1", port))?;
    if let Some(attempt) = attempt {
        attempt.register_socket(&sock)?;
    }
    if let Err(e) = sock.set_read_timeout(Some(heartbeat_timeout)) {
        tracing::warn!("bulk upload: failed to set read timeout: {e}");
    }
    if let Err(e) = sock.set_write_timeout(Some(heartbeat_timeout)) {
        tracing::warn!("bulk upload: failed to set write timeout: {e}");
    }
    let (mut conn, _client_id) =
        StreamConnection::open_bulk_with_timeout(sock, STREAM_PROTO, remote_ws, heartbeat_timeout)?;
    declare_bulk_loss_notify(&mut conn);
    Ok(conn)
}

/// 결과가 유실되면 기다리기만 하지 않도록 손실 통지를 요청한다. 구 서버는 무시할 수 있다.
fn declare_bulk_loss_notify(conn: &mut StreamConnection) {
    match serde_json::to_vec(&StreamControl::ClientLossNotify {}) {
        Ok(declare) => {
            if let Err(e) = conn.send(StreamTag::Control, &declare) {
                tracing::warn!("bulk upload: loss-notify declaration was not sent: {e}");
            }
        }
        Err(e) => tracing::warn!("bulk upload: loss-notify declaration did not serialize: {e}"),
    }
}

/// begin·청크·commit을 순서대로 보내고 시작 및 각 청크 뒤에 누적 전송 바이트를 알린다.
fn send_bulk_payload(
    conn: &mut StreamConnection,
    transfer_id: u64,
    file_name: &str,
    bytes: &[u8],
    on_progress: impl Fn(u64, u64),
    epoch: &tasty_remote::connection::ConnectionEpoch,
) -> anyhow::Result<()> {
    let begin = StreamControl::BulkBegin {
        transfer_id,
        filename: file_name.to_string(),
        total_size: bytes.len() as u64,
    };
    ensure_bulk_epoch(epoch)?;
    conn.send(StreamTag::Control, &serde_json::to_vec(&begin)?)?;

    let total = bytes.len() as u64;
    on_progress(0, total);
    let mut sent: u64 = 0;
    for framed in bulk_chunk_frames(transfer_id, bytes) {
        let part_len = (framed.len() - stream::BULK_CHUNK_HEADER_LEN) as u64;
        ensure_bulk_epoch(epoch)?;
        conn.send(StreamTag::Data, &framed)?;
        sent += part_len;
        on_progress(sent, total);
    }

    let commit = StreamControl::BulkCommit { transfer_id };
    ensure_bulk_epoch(epoch)?;
    conn.send(StreamTag::Control, &serde_json::to_vec(&commit)?)?;
    Ok(())
}

/// 해당 transfer_id의 결과를 기다린다. Ping이나 다른 응답은 무시하므로 전체 대기 기한은 없다.
/// 소켓 timeout이 설정됐다면 개별 읽기에 적용된다.
fn await_bulk_result(
    conn: &mut StreamConnection,
    transfer_id: u64,
    epoch: &tasty_remote::connection::ConnectionEpoch,
) -> anyhow::Result<String> {
    loop {
        ensure_bulk_epoch(epoch)?;
        let frame = conn.recv()?;
        match frame.tag {
            StreamTag::Control => {
                match serde_json::from_slice::<StreamControl>(&frame.payload) {
                    Ok(StreamControl::BulkResult {
                        transfer_id: tid,
                        ok,
                        path,
                        reason,
                    }) if tid == transfer_id => {
                        if let Err(e) = conn.detach() {
                            tracing::debug!("bulk upload: detach after result failed: {e}");
                        }
                        return if ok {
                            path.ok_or_else(|| {
                                anyhow::anyhow!("bulk result ok but carried no path")
                            })
                        } else {
                            Err(anyhow::anyhow!(
                                "{BULK_REJECT_PREFIX}{}",
                                reason.unwrap_or_else(|| "unknown".to_string())
                            ))
                        };
                    }
                    // 서버가 저장했는지 알 수 없어 자동 재시도하지 않는다.
                    // 정책 거절 접두사는 붙이지 않아 사용자가 재시도를 선택할 수 있게 한다.
                    Ok(StreamControl::Loss { frames }) => {
                        if let Err(e) = conn.detach() {
                            tracing::debug!("bulk upload: detach after a loss notice failed: {e}");
                        }
                        anyhow::bail!(
                            "bulk upload aborted: the remote dropped {frames} frame(s) of this transfer's result channel, so whether the file was saved is unknown"
                        );
                    }
                    _ => {}
                }
            }
            StreamTag::Detach => {
                anyhow::bail!("remote detached before delivering bulk result");
            }
            _ => {}
        }
    }
}

/// 원격 업로드 전체를 동기로 수행한다. on_progress는 시작과 각 청크 전송 뒤 호출한다.
pub(crate) fn upload_file_over_bulk(
    endpoint: BulkEndpoint,
    file_name: &str,
    bytes: &[u8],
    on_progress: impl Fn(u64, u64),
    epoch: &tasty_remote::connection::ConnectionEpoch,
    attempt: &tasty_remote::outbound::AttemptToken,
) -> anyhow::Result<String> {
    ensure_bulk_epoch(epoch)?;
    let transfer_id = next_bulk_transfer_id();
    let mut conn = open_bulk_connection_bound(endpoint, Some(attempt))?;
    send_bulk_payload(&mut conn, transfer_id, file_name, bytes, on_progress, epoch)?;
    await_bulk_result(&mut conn, transfer_id, epoch)
}

fn ensure_bulk_epoch(epoch: &tasty_remote::connection::ConnectionEpoch) -> anyhow::Result<()> {
    if epoch.is_active() {
        Ok(())
    } else {
        anyhow::bail!("bulk connection retired; remote save outcome may be unknown")
    }
}
