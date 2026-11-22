//! Outbound socket handshake, reader, writer and heartbeat. No App or View references.
//!
//! 연결의 끝은 전송마다 하나인 [`TransportEnd`]에 둔다. 끝을 본 쪽은 사유를 올리고, 끝을 정하는 쪽
//! 하나가 확정한 뒤 깨운다. 확정에 진 쪽은 깨우지 않는다.
//!
//! | 출구 | 사유 | 확정 |
//! |---|---|---|
//! | reader: 강제 통지 | `Forced`(`workspace_gone`이면 `Lost`) | `ReaderExit` |
//! | reader: Detach·EOF·읽기 오류·panic | `Lost` | `ReaderExit` |
//! | writer: 큐 거절·쓰기 실패·Detach 쓰기 | 쓰기 실패는 `Lost`, 큐 거절은 sender가 `Lost`, Detach는 올리지 않음 | reader를 기다린 뒤 reader, 상한이면 writer가 `Lost`로(`drain_reader_then_settle`) |
//! | writer: sender 전부 사라짐·panic | `Lost` | writer가 바로(`WriterExit`) |
//! | heartbeat | 사유가 오르면 멈춘다 | 확정하지 않음 |
use super::client_session::{FrameSender, MirrorEvent, MirrorOutbox, OutFrame};
use super::transport_end::{EndReason, TransportEnd};
use serde_json::Value;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tasty_ipc::client::StreamConnection;
use tasty_ipc::stream::{self, STREAM_PROTO, StreamControl, StreamTag};
pub fn attach_handshake(
    port: u16,
    workspace: u32,
    log_prefix: &str,
    cancel: &super::outbound::AttemptToken,
    heartbeat_timeout: std::time::Duration,
) -> anyhow::Result<AttachHandshake> {
    if !cancel.is_active() {
        anyhow::bail!("connection attempt retired");
    }
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let sock = TcpStream::connect_timeout(&address, std::time::Duration::from_millis(250))?;
    cancel.register_socket(&sock)?;
    arm_attach_timeouts(&sock, log_prefix, heartbeat_timeout);
    let (mut conn, client_id) = StreamConnection::open_attach_workspace_with_timeout(
        sock,
        STREAM_PROTO,
        workspace,
        heartbeat_timeout,
    )?;
    let first = conn.recv()?;
    if first.tag != StreamTag::Control {
        anyhow::bail!("expected attach Control frame, got {:?}", first.tag);
    }
    let ctrl: Value = serde_json::from_slice(&first.payload)?;
    let (name, surfaces, tree) = parse_attach_descriptor(&ctrl)?;
    // 손실 통지는 연결별 opt-in이다. 구 서버는 이 선언을 모를 수 있으므로
    // 지원을 확인해야 하면 system.info의 ipc.stream.loss-notify capability를 조회한다.
    let declare = serde_json::to_vec(&StreamControl::ClientLossNotify {}).unwrap_or_default();
    if let Err(e) = conn.send(StreamTag::Control, &declare) {
        tracing::warn!("{log_prefix}: 손실 통지 요청을 보내지 못했다: {e}");
    }
    let html = super::html_document::HtmlSession::from_descriptor(&ctrl);
    if html.server_transfer {
        let declare = tasty_ipc::html_transfer::HtmlRequest::HtmlCapabilities {
            capabilities: vec![tasty_ipc::html_transfer::CAPABILITY.into()],
        };
        conn.send(StreamTag::Control, &serde_json::to_vec(&declare)?)?;
    }
    let write_half = conn.try_clone_writer()?;
    Ok(AttachHandshake {
        resize_acks: descriptor_has_capability(&ctrl, stream::RESIZE_ACK_CAPABILITY),
        input_acks: descriptor_has_capability(&ctrl, stream::INPUT_ACK_CAPABILITY),
        html,
        explorer_write: descriptor_has_capability(&ctrl, tasty_ipc::explorer_op::CAPABILITY),
        workspace_close: descriptor_has_capability(&ctrl, stream::WORKSPACE_CLOSE_CAPABILITY),
        agent_children: descriptor_has_capability(&ctrl, tasty_ipc::agent_child::CAPABILITY),
        terminal_read: descriptor_has_capability(&ctrl, tasty_ipc::terminal_read::CAPABILITY),
        file_download: descriptor_has_capability(&ctrl, tasty_ipc::file_download::CAPABILITY),
        inherit_cwd: descriptor_has_capability(&ctrl, stream::INHERIT_CWD_CAPABILITY),
        win32_console_input: stream::descriptor_wants_win32_console_input(&ctrl),
        conn,
        client_id,
        write_half,
        name,
        surfaces,
        tree,
    })
}

/// workspace attach handshake 결과. descriptor의 이름·surface·tree와 연결별 서버 기능을 담는다.
pub struct AttachHandshake {
    pub conn: StreamConnection,
    pub client_id: u32,
    pub write_half: TcpStream,
    pub name: String,
    pub surfaces: Vec<Value>,
    pub tree: Value,
    /// 서버가 모든 크기 요청에 응답한다. 구 서버 descriptor에는 이 기능이 없다.
    pub resize_acks: bool,
    pub input_acks: bool,
    /// 서버가 원격 explorer 파일 작업을 받는다.
    pub explorer_write: bool,
    /// 서버가 점유한 workspace 전체 닫기를 받는다.
    pub workspace_close: bool,
    /// 서버가 원격 자식 에이전트 요청과 완료 통지를 지원한다.
    pub agent_children: bool,
    /// 서버가 holder 의 원본 터미널 읽기를 받는다.
    pub terminal_read: bool,
    /// 서버가 holder 의 원격 파일 하나 가져오기를 받는다.
    pub file_download: bool,
    /// 서버가 생성 요청의 cwd 상속 원본을 따른다.
    pub inherit_cwd: bool,
    /// 서버 PTY가 Windows 콘솔 입력을 받는다. 구 서버는 false(VT)다.
    pub win32_console_input: bool,
    pub html: super::html_document::HtmlSession,
}

/// descriptor의 기능 목록에 `name`이 있는지. 목록이 없는 구 서버는 false다.
pub fn descriptor_has_capability(ctrl: &Value, name: &str) -> bool {
    ctrl.get(stream::DESCRIPTOR_CAPABILITIES)
        .and_then(|v| v.as_array())
        .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(name)))
}

pub fn arm_attach_timeouts(sock: &TcpStream, log_prefix: &str, timeout: std::time::Duration) {
    if let Err(e) = sock.set_read_timeout(Some(timeout)) {
        tracing::warn!("{log_prefix}: failed to set read timeout: {e}");
    }
    if let Err(e) = sock.set_write_timeout(Some(timeout)) {
        tracing::warn!("{log_prefix}: failed to set write timeout: {e}");
    }
}

/// 서버가 workspace attach를 거절했을 때 연결 오류의 머리. 뒤에 서버의 거절 사유가 붙는다.
pub const ATTACH_REJECTED: &str = "workspace attach rejected: ";

/// 서버가 다시 mirror할 수 없는 대상(mirror workspace나 그 surface)이라 거절한 사유.
pub const REJECTED_MIRROR_WORKSPACE: &str = "mirror_workspace";

/// 연결 오류 문자열에서 서버의 거절 사유를 읽는다. 서버 거절이 아니면 `None`이다.
pub fn attach_rejection_reason(error: &str) -> Option<&str> {
    let (_, rest) = error.split_once(ATTACH_REJECTED)?;
    rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .filter(|reason| !reason.is_empty())
}

pub fn parse_attach_descriptor(ctrl: &Value) -> anyhow::Result<(String, Vec<Value>, Value)> {
    match ctrl.get("event").and_then(|v| v.as_str()) {
        Some("attached_workspace") => {}
        Some("attach_error") => {
            let reason = ctrl
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            anyhow::bail!("{ATTACH_REJECTED}{reason}");
        }
        other => anyhow::bail!("unexpected attach control event: {other:?}"),
    }
    let name = ctrl
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("remote")
        .to_string();
    let surfaces = ctrl
        .get("surfaces")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let tree = ctrl.get("tree").cloned().unwrap_or(Value::Null);
    Ok((name, surfaces, tree))
}

/// 쓰기 스레드가 연결의 끝을 확정하는 데 필요한 표지.
///
/// `detach_sent`는 이쪽이 Detach를 보내기 시작했다는 표지다. reader가 뒤따르는 EOF를 원격 원인으로
/// 남기지 않도록 [`AttachReaderEnd`]와 같은 값을 넘긴다.
///
/// `reader_finished`는 같은 연결의 reader가 끝났다는 표지([`AttachReaderEnd::finished`])다.
/// reader가 없는 연결은 처음부터 참인 값을 넘긴다. `reader_wait_cap`은 쓰기 쪽을 닫은 뒤 reader를
/// 기다리는 안전 상한이다([`drain_reader_then_settle`]). 제품 경로는 reader 소켓의 읽기 기한과 같은
/// 값을 넘긴다.
pub struct AttachWriterEnd {
    pub transport_end: Arc<TransportEnd>,
    pub detach_sent: Arc<AtomicBool>,
    pub reader_finished: Arc<AtomicBool>,
    pub reader_wait_cap: std::time::Duration,
    #[cfg(test)]
    pub seams: WriterSeams,
}

/// 시험이 쓰기 스레드의 기다림 순서를 정하는 이음새. 벽시계 경과로 순서를 만들지 않는다.
#[cfg(test)]
#[derive(Default)]
pub struct WriterSeams {
    /// 쓰기 스레드가 reader 기다림에 들어갈 때 신호를 보낸다.
    pub on_wait_enter: Option<std::sync::mpsc::Sender<()>>,
    /// 상한 판정은 이 값이 참이 된 뒤에만 성립한다.
    pub cap_gate: Option<Arc<AtomicBool>>,
}

/// 쓰기 스레드의 출구(설계의 W1~W5).
///
/// - W1: 큐에서 꺼낸 프레임이 Detach가 아니고 끝 사유가 이미 올라 있다(큐 거절, ID 실패, heartbeat
///   송신 실패). 수신 대기 시간 초과에서도 본다. 큐가 계속 차 있어도 빠져나온다.
/// - W2: 쓰기 실패. 사유를 올린 뒤 W1과 같다.
/// - W3: Detach를 썼다. 사유가 올라 있어도 Detach는 쓴다.
/// - W4: 모든 sender가 사라졌다. 바로 확정한다.
/// - W5: panic. [`WriterExit`]가 바로 확정한다.
///
/// W1~W3은 쓰기 쪽을 닫고 reader를 상한까지 기다린다. reader가 먼저 끝나면 reader가 확정했다.
/// 상한에 닿으면 쓰기 스레드가 `raise(Lost)` → settle → `shutdown(Both)` → wake 순서로 끝낸다.
/// 확정에 이긴 쪽만 깨운다.
pub fn spawn_attach_write_thread(
    write_half: TcpStream,
    frame_rx: std::sync::mpsc::Receiver<super::connection::QueuedFrame>,
    link: AttachWriterEnd,
    wake: Arc<dyn Fn() + Send + Sync>,
    log_suffix: &'static str,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // panic guard가 쓰기 핸들 자체를 쥔다. 복제 핸들은 만들 수 없을 때(descriptor 고갈) guard가 소켓을
        // 닫지 못하므로 쓰지 않는다. 스레드는 guard를 거쳐 같은 핸들로 쓴다.
        let mut exit = WriterExit {
            socket: write_half,
            end: link.transport_end.clone(),
            wake: wake.clone(),
        };
        let write_half = &mut exit.socket;
        let end = &link.transport_end;
        loop {
            let queued = match frame_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(queued) => queued,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if end.is_ending() {
                        drain_reader_then_settle(write_half, &link, &wake, log_suffix);
                        return;
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    settle_by_writer(write_half, end, &wake);
                    return;
                }
            };
            if !queued.owner_current() {
                continue;
            }
            let item = queued.frame;
            let detach = item.tag == StreamTag::Detach;
            if !detach && end.is_ending() {
                drain_reader_then_settle(write_half, &link, &wake, log_suffix);
                return;
            }
            if !detach && !queued.epoch.is_active() {
                continue;
            }
            if detach {
                // 쓰기 뒤에는 서버가 바로 닫을 수 있어 reader가 먼저 EOF를 본다.
                link.detach_sent.store(true, Ordering::SeqCst);
            }
            if let Err(e) = stream::write_frame(write_half, item.tag, &item.payload) {
                match write_failure_notice(end, &e, log_suffix) {
                    (true, message) => tracing::warn!("{message}"),
                    (false, message) => tracing::debug!("{message}"),
                }
                end.raise(EndReason::Lost);
                drain_reader_then_settle(write_half, &link, &wake, log_suffix);
                return;
            }
            if detach {
                drain_reader_then_settle(write_half, &link, &wake, log_suffix);
                return;
            }
        }
    })
}

/// 쓰기 실패를 경고로 남길지와 문구. 실패 전에 이미 끝 사유가 올라 있었으면(로컬 해제가 소켓을 닫은 뒤의
/// 정리 Detach 등) 예상한 실패라 debug다. 사유가 없던 연결의 실패만 실제 쓰기 장애로 경고한다.
fn write_failure_notice(
    end: &TransportEnd,
    error: &std::io::Error,
    log_suffix: &str,
) -> (bool, String) {
    if end.is_ending() {
        (
            false,
            format!(
                "attach write thread{log_suffix}: 이미 끝난 연결이라 프레임을 쓰지 못했다: {error}"
            ),
        )
    } else {
        (
            true,
            format!(
                "attach write thread{log_suffix}: 프레임을 쓰지 못해 연결 종료로 처리한다: {error}"
            ),
        )
    }
}

/// 쓰기 스레드가 확정한다. 확정에 이겼을 때만 깨운다. 소켓은 어느 쪽이든 닫는다.
fn settle_by_writer(
    write_half: &TcpStream,
    end: &TransportEnd,
    wake: &Arc<dyn Fn() + Send + Sync>,
) {
    end.raise(EndReason::Lost);
    let won = end.settle();
    if let Err(error) = tasty_utils::socket_wake::shutdown_both_and_wake(write_half) {
        tracing::debug!("remote socket already closed: {error}");
    }
    if won {
        (wake)();
    }
}

/// 쓰기 쪽을 닫고 같은 연결의 reader가 끝나기를 상한까지 기다린 뒤 끝을 확정한다.
///
/// 서버는 쓰기 쪽 EOF로 연결을 정리하고 소켓을 닫으므로, reader는 수신 버퍼에 남은 프레임(서버가
/// 닫기 전에 보낸 `force_detached` 통지 포함)을 다 읽은 뒤 EOF로 끝난다. reader가 상한 전에 끝나면
/// reader가 이미 확정했다. 통지는 reader의 `raise(Forced)`가 확정보다 먼저 일어났을 때만 사유에 든다.
///
/// `cap`은 순서를 정하는 기한이 아니라 안전 상한이다. 상대가 EOF를 처리하지 않은 채 Ping·데이터를
/// 계속 보내는 반쯤 끊긴 연결에서는 reader가 끝나지 않는다. 그때는 상한에서 쓰기 스레드가 `Lost`로
/// 확정한다. 그 뒤 reader가 이미 읽은 통지로 사유를 올려도 확정된 사유는 바뀌지 않는다. 상한은
/// 기다림에 들어간 때부터 잰다. reader가 panic해도 [`ReaderExit`]가 끝 표지를 세우므로 상한까지
/// 기다리지 않는다.
fn drain_reader_then_settle(
    write_half: &TcpStream,
    link: &AttachWriterEnd,
    wake: &Arc<dyn Fn() + Send + Sync>,
    log_suffix: &str,
) {
    let finished = &link.reader_finished;
    if !finished.load(Ordering::Acquire) {
        if let Err(error) = write_half.shutdown(std::net::Shutdown::Write) {
            tracing::debug!("remote socket already closed before the reader drained: {error}");
        }
        #[cfg(test)]
        if let Some(signal) = &link.seams.on_wait_enter
            && signal.send(()).is_err()
        {
            tracing::debug!("writer wait signal receiver is gone");
        }
        let cap = link.reader_wait_cap;
        let deadline = std::time::Instant::now() + cap;
        while !finished.load(Ordering::Acquire) {
            if std::time::Instant::now() >= deadline && cap_released(link) {
                tracing::warn!(
                    "attach write thread{log_suffix}: reader가 {cap:?} 안에 끝나지 않아 끊김으로 확정한다"
                );
                settle_by_writer(write_half, &link.transport_end, wake);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    if let Err(error) = write_half.shutdown(std::net::Shutdown::Both) {
        tracing::debug!("remote socket already closed: {error}");
    }
}

#[cfg(not(test))]
fn cap_released(_link: &AttachWriterEnd) -> bool {
    true
}

#[cfg(test)]
fn cap_released(link: &AttachWriterEnd) -> bool {
    link.seams
        .cap_gate
        .as_ref()
        .is_none_or(|gate| gate.load(Ordering::Acquire))
}

/// 쓰기 스레드가 panic으로 끝나면(W5) 바로 확정하고 소켓을 닫는다. 통지 보존은 포기한다.
/// 깨움은 [`ReaderExit`]와 같은 이유로 `catch_unwind`로 감싸고 payload를 잊는다.
struct WriterExit {
    socket: TcpStream,
    end: Arc<TransportEnd>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Drop for WriterExit {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        self.end.raise(EndReason::Lost);
        let won = self.end.settle();
        if let Err(error) = tasty_utils::socket_wake::shutdown_both_and_wake(&self.socket) {
            tracing::debug!("remote socket already closed: {error}");
        }
        if won {
            wake_while_unwinding(&self.wake, "attach writer");
        }
    }
}

fn wake_while_unwinding(wake: &Arc<dyn Fn() + Send + Sync>, who: &str) {
    if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (wake)())) {
        // payload를 drop하지 않는다. 이 스레드는 이미 unwinding 중이라 payload의 소멸자가 panic하면
        // 프로세스가 abort한다. 스레드가 panic으로 끝날 때 한 번 생기는 작은 누수는 받아들인다.
        std::mem::forget(payload);
        tracing::error!("{who}: the wake callback panicked again while unwinding");
    }
}

/// reader 스레드가 어떻게 끝나든(panic 포함) 확정하고 끝 표지를 세운 뒤 깨운다(R1~R3).
///
/// 순서는 사유 올리기(분기 또는 panic) → settle → `finished` → wake다. 분기는 사유만 올리고 깨우지
/// 않는다. 확정에 이긴 쪽만 깨우므로 쓰기 스레드가 상한에서 먼저 확정했다면 깨우지 않는다.
/// unwinding 중의 깨움은 `catch_unwind`로 감싼다. 깨움 콜백은 호출자가 넘긴 함수라 reader를
/// panic시킨 것이 그 콜백일 수 있다. 다시 panic한 콜백이 drop 밖으로 나가면 프로세스가 abort한다.
struct ReaderExit {
    end: Arc<TransportEnd>,
    finished: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Drop for ReaderExit {
    fn drop(&mut self) {
        let panicked = std::thread::panicking();
        if panicked {
            self.end.raise(EndReason::Lost);
        }
        let won = self.end.settle();
        self.finished.store(true, Ordering::Release);
        if !won {
            return;
        }
        if panicked {
            wake_while_unwinding(&self.wake, "attach reader");
        } else {
            (self.wake)();
        }
    }
}

/// reader가 끝날 때 확정과 원인 기록에 필요한 연결 정보.
pub struct AttachReaderEnd {
    /// 이 연결이 점유한 원격 workspace id. 이쪽 mirror workspace id가 아니다.
    pub remote_workspace: u32,
    /// 이 소켓에 실제로 건 read timeout.
    pub heartbeat_timeout: std::time::Duration,
    /// 이쪽이 Detach를 보내기 시작했다(재동기화·연결 끊기). 쓰기 스레드와 같은 값이다.
    pub detach_sent: Arc<AtomicBool>,
    /// 이 연결의 끝. 서버의 `force_detached` 통지(Final)는 `Forced`로 올린다.
    pub transport_end: Arc<TransportEnd>,
    /// reader 스레드가 끝났다. 확정을 시도한 뒤에 선다. 쓰기 스레드가 쓰기 쪽을 닫은 뒤 이 값을
    /// 기다린다. panic으로 끝나도 [`ReaderExit`]가 세운다.
    pub finished: Arc<AtomicBool>,
    #[cfg(test)]
    pub gate: Option<ReaderGate>,
    /// 시험 전용: reader를 밖에서 보는 관측값([`ReaderProbe`]).
    #[cfg(test)]
    pub probe: Option<ReaderProbe>,
}

/// 시험 전용: 준비된 전송의 reader를 밖에서 본다. `recv_entries`는 handshake 뒤 `conn.recv()`에
/// 들어가기 직전마다 더하는 계수, `finished`는 [`AttachReaderEnd::finished`]의 복제다([`ReaderExit`]가
/// Release로 세운다). `ended_at`은 reader 스레드가 끝 표지를 세우기 직전에 자기 시각을 남긴다.
/// 검사 스레드가 늦게 깨어나도 reader가 언제 끝났는지를 reader 쪽 시각으로 비교하기 위해서다.
/// `first_recv_at`은 첫 `conn.recv()` 직전의 시각이다. 계수보다 먼저 남기므로 계수를 본 쪽은 이 값도 본다.
/// 읽기 기한은 recv 호출부터 재므로 첫 읽기의 기한 만료는 이 시각 + 기한 이후에만 일어난다.
#[cfg(test)]
#[derive(Clone)]
pub struct ReaderProbe {
    pub recv_entries: Arc<std::sync::atomic::AtomicUsize>,
    pub finished: Arc<AtomicBool>,
    pub ended_at: Arc<std::sync::OnceLock<std::time::Instant>>,
    pub first_recv_at: Arc<std::sync::OnceLock<std::time::Instant>>,
}

/// 시험 전용: drop될 때 reader의 끝 시각을 남긴다. [`ReaderExit`]보다 뒤에 선언해 먼저 drop된다.
#[cfg(test)]
struct ReaderEndStamp(Arc<std::sync::OnceLock<std::time::Instant>>);

#[cfg(test)]
impl Drop for ReaderEndStamp {
    fn drop(&mut self) {
        if self.0.set(std::time::Instant::now()).is_err() {
            tracing::debug!("reader end stamp already set");
        }
    }
}

#[cfg(test)]
impl ReaderProbe {
    pub fn entered_recv(&self) -> bool {
        self.recv_entries.load(Ordering::SeqCst) > 0
    }
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
    pub fn ended_at(&self) -> Option<std::time::Instant> {
        self.ended_at.get().copied()
    }
    pub fn first_recv_at(&self) -> Option<std::time::Instant> {
        self.first_recv_at.get().copied()
    }
}

/// 시험 전용: reader가 프레임을 받은 뒤 분기 판별 전에 멈춘다. 도착을 알리고 놓아줄 때까지 기다린다.
/// `release`의 송신 쪽이 사라지면 더 멈추지 않는다.
#[cfg(test)]
pub struct ReaderGate {
    pub arrived: std::sync::mpsc::Sender<()>,
    pub release: std::sync::mpsc::Receiver<()>,
}

impl AttachReaderEnd {
    /// 끊김 사유를 올리고 원인을 남긴다. 이미 다른 쪽이 사유를 올린 연결은 이쪽(정리·쓰기 실패)이
    /// 끝낸 것이라 원격 원인이 아니므로 debug로만 남긴다.
    fn report(&self, error: &anyhow::Error) {
        match self.outcome(error) {
            (true, message) => tracing::warn!("{message}"),
            (false, message) => tracing::debug!("{message}"),
        }
    }

    /// `Lost`를 올리고, 경고로 남길지와 문구를 돌려준다. 이 reader가 처음 사유를 올렸을 때만 경고다.
    fn outcome(&self, error: &anyhow::Error) -> (bool, String) {
        let cause = stream::ReadFailure::of_error(error);
        let workspace = self.remote_workspace;
        let first = !self.transport_end.is_ending();
        self.transport_end.raise(EndReason::Lost);
        if !first {
            (
                false,
                format!(
                    "attach reader thread: remote workspace {workspace} 이미 끝난 연결의 수신 종료({cause:?}): {error}"
                ),
            )
        } else if self.detach_sent.load(Ordering::SeqCst) {
            (
                false,
                format!(
                    "attach reader thread: remote workspace {workspace} 이쪽이 Detach로 끝낸 연결(재동기화 등)의 수신 종료({cause:?}): {error}"
                ),
            )
        } else {
            (
                true,
                format!(
                    "attach reader thread: remote workspace {workspace} 연결 종료를 알린다. 원인 {}: {error}",
                    cause.describe(self.heartbeat_timeout)
                ),
            )
        }
    }

    #[cfg(test)]
    fn pass_gate(&self) {
        if let Some(gate) = &self.gate {
            if gate.arrived.send(()).is_err() {
                tracing::debug!("reader gate arrival receiver is gone");
            }
            if gate.release.recv().is_err() {
                tracing::debug!("reader gate released for good");
            }
        }
    }
}

pub fn spawn_attach_reader_thread(
    mut conn: StreamConnection,
    output: MirrorOutbox,
    frame_tx: FrameSender,
    wake: Arc<dyn Fn() + Send + Sync>,
    decode_control: fn(&[u8]) -> Option<MirrorEvent>,
    end: AttachReaderEnd,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let _exit = ReaderExit {
            end: end.transport_end.clone(),
            finished: end.finished.clone(),
            wake: wake.clone(),
        };
        #[cfg(test)]
        let _stamp = end
            .probe
            .as_ref()
            .map(|probe| ReaderEndStamp(probe.ended_at.clone()));
        let transport_end = &end.transport_end;
        let mut mesh_assembler = tasty_ipc::mesh_stream::MeshFrameAssembler::new();
        loop {
            #[cfg(test)]
            if let Some(probe) = &end.probe {
                // 두 번째 이후의 recv 는 첫 시각을 그대로 둔다.
                if probe.first_recv_at.get().is_none()
                    && probe.first_recv_at.set(std::time::Instant::now()).is_err()
                {
                    tracing::debug!("reader first recv stamp already set");
                }
                probe.recv_entries.fetch_add(1, Ordering::SeqCst);
            }
            match conn.recv() {
                Ok(frame) => {
                    #[cfg(test)]
                    end.pass_gate();
                    match frame.tag {
                        StreamTag::Data => {
                            if let Some((sid, payload)) = stream::decode_mux(&frame.payload) {
                                output.push(MirrorEvent::Data(sid, payload.to_vec()));
                            }
                            (wake)();
                        }
                        StreamTag::Detach => {
                            transport_end.raise(EndReason::Lost);
                            break;
                        }
                        StreamTag::Control => {
                            if let Some(notice) = stream::force_detach_notice(&frame.payload) {
                                transport_end.raise(
                                    if notice == stream::ForceDetachNotice::Final {
                                        EndReason::Forced
                                    } else {
                                        EndReason::Lost
                                    },
                                );
                                break;
                            }
                            if let Ok(StreamControl::InputProcessed { sequence }) =
                                serde_json::from_slice(&frame.payload)
                            {
                                frame_tx.input_processed(sequence);
                                (wake)();
                                continue;
                            }
                            let mirror_ev = decode_control(&frame.payload);
                            if let Some(ev) = mirror_ev
                                && output.push(ev)
                            {
                                (wake)();
                            }
                        }
                        StreamTag::Ping => {}
                        // 완성된 mesh frame만 적용한다. 손상 청크는 폐기하며 이후 full frame으로 복구해야 한다.
                        StreamTag::MeshData => {
                            if let Ok(Some((meta, bytes))) =
                                mesh_assembler.push_chunk(&frame.payload)
                                && output.push(MirrorEvent::Mesh(
                                    meta.surface_id,
                                    meta.generation,
                                    meta.frame_seq,
                                    meta.full_textures,
                                    bytes,
                                ))
                            {
                                (wake)();
                            }
                        }
                    }
                }
                Err(e) => {
                    end.report(&e);
                    break;
                }
            }
        }
        // 확정·끝 표지·깨움은 `_exit`가 한다.
    })
}

/// 송신이 실패하면 `ConnectionSender::fail`이 사유를 올리고 깨운다. 확정은 쓰기 스레드나 reader가 한다.
pub fn spawn_attach_heartbeat_thread(
    raw_frame_tx: FrameSender,
    end: Arc<TransportEnd>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            std::thread::park_timeout(stream::HEARTBEAT_INTERVAL);
            if end.is_ending() {
                break;
            }
            if raw_frame_tx
                .send(OutFrame {
                    tag: StreamTag::Ping,
                    payload: Vec::new(),
                })
                .is_err()
            {
                break;
            }
        }
    })
}

/// The connection owns every I/O worker. Retirement is nonblocking for the GUI: a short-lived
/// reaper closes the exact socket and joins its workers, never a current replacement connection.
#[derive(Clone)]
pub struct RetirementReceipt(std::sync::Arc<std::sync::atomic::AtomicU8>);
/// 회수가 끝났고 worker 가 모두 정상으로 끝났다.
const RETIRED: u8 = 1;
/// 회수가 끝났고 panic 한 worker 가 있었다.
const RETIRE_FAILED: u8 = 2;
/// 회수 스레드가 대기 상한까지 기다렸다. 끝 값에 더해진다.
const RETIRE_WAITED_OUT: u8 = 4;
impl RetirementReceipt {
    pub fn is_done(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }
    pub fn failed(&self) -> bool {
        self.0.load(Ordering::Acquire) & RETIRE_FAILED != 0
    }
    /// 회수 스레드가 대기 상한(250ms)이 지나도록 끝나지 않은 worker 를 보고 소켓을 닫았는지. 회수 스레드가
    /// 관측한 사실이라 worker 스레드의 스케줄에 따라 달라진다. 회수가 끝나기 전에는 false 다.
    pub fn waited_out(&self) -> bool {
        self.0.load(Ordering::Acquire) & RETIRE_WAITED_OUT != 0
    }
}
pub struct ConnectionWorkers {
    receipt: RetirementReceipt,
    control: Option<TcpStream>,
    handles: Vec<std::thread::JoinHandle<()>>,
    tunnel: Option<tasty_ssh::SshTunnel>,
    /// 시험 전용: [`Self::close_socket`]이 소켓을 닫은 스레드. 반납 호출이 자기 스레드에서 닫았는지 본다.
    #[cfg(test)]
    closed_at_once: Arc<std::sync::OnceLock<std::thread::ThreadId>>,
    /// 시험 전용: 참인 동안 회수 스레드가 소켓을 닫지 않고 기다린다([`ReaperCloseHold`]).
    #[cfg(test)]
    reaper_close_held: Arc<AtomicBool>,
}

/// 시험 전용: [`ConnectionWorkers::close_socket`]이 소켓을 닫은 스레드. 닫지 않았으면 `None` 이다.
#[cfg(test)]
pub struct CloseStamp(Arc<std::sync::OnceLock<std::thread::ThreadId>>);

#[cfg(test)]
impl CloseStamp {
    pub fn by(&self) -> Option<std::thread::ThreadId> {
        self.0.get().copied()
    }
}

/// 시험 전용: 회수 스레드의 소켓 닫기를 막아 둔다. drop 하면 놓는다.
#[cfg(test)]
pub struct ReaperCloseHold(Arc<AtomicBool>);

#[cfg(test)]
impl Drop for ReaperCloseHold {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// 시험 전용: 시험이 hold 를 놓을 때까지 기다린다. 놓지 않아도 멈추지 않게 60초에서 그친다.
#[cfg(test)]
fn wait_while_reaper_close_held(held: &AtomicBool) {
    let give_up = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while held.load(Ordering::Acquire) {
        if std::time::Instant::now() >= give_up {
            tracing::debug!("reaper close hold was never released");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

impl ConnectionWorkers {
    pub fn new(control: TcpStream, handles: Vec<std::thread::JoinHandle<()>>) -> Self {
        Self {
            receipt: RetirementReceipt(Arc::new(std::sync::atomic::AtomicU8::new(0))),
            control: Some(control),
            handles,
            tunnel: None,
            #[cfg(test)]
            closed_at_once: Arc::default(),
            #[cfg(test)]
            reaper_close_held: Arc::default(),
        }
    }
    /// 시험 전용: 회수 스레드가 소켓을 닫기 전에 hold 가 풀리기를 기다리게 한다.
    #[cfg(test)]
    pub fn hold_reaper_close(&self) -> ReaperCloseHold {
        self.reaper_close_held.store(true, Ordering::Release);
        ReaperCloseHold(self.reaper_close_held.clone())
    }
    /// 시험 전용: [`Self::close_socket`]이 소켓을 닫은 스레드를 읽는 표지. 연결을 버린 뒤에도 읽는다.
    #[cfg(test)]
    pub fn close_stamp(&self) -> CloseStamp {
        CloseStamp(self.closed_at_once.clone())
    }
    pub fn receipt(&self) -> RetirementReceipt {
        self.receipt.clone()
    }
    pub(crate) fn retire_tunnel(&mut self, tunnel: Option<tasty_ssh::SshTunnel>) {
        self.tunnel = tunnel;
    }
    /// 끊겼다고 판단한 연결의 소켓을 바로 닫는다. 응답이 멈췄던 상대가 돌아오면 이 연결의 끝을 곧바로
    /// 읽고 점유를 놓으므로, 같은 상대에 다시 붙는 새 연결이 already_attached 로 거절되지 않는다.
    /// 작업 스레드 정리는 drop 이 맡는다.
    pub fn close_socket(&self) {
        if let Some(control) = &self.control {
            let shut = tasty_utils::socket_wake::shutdown_both_and_wake(control);
            #[cfg(test)]
            if self
                .closed_at_once
                .set(std::thread::current().id())
                .is_err()
            {
                tracing::debug!("remote socket close stamp already set");
            }
            if let Err(error) = shut {
                tracing::debug!("remote socket already closed: {error}");
            }
        }
    }
}
impl Drop for ConnectionWorkers {
    fn drop(&mut self) {
        let Some(control) = self.control.take() else {
            return;
        };
        let handles = std::mem::take(&mut self.handles);
        let tunnel = self.tunnel.take();
        for handle in &handles {
            handle.thread().unpark();
        }
        let receipt = self.receipt.clone();
        #[cfg(test)]
        let held = self.reaper_close_held.clone();
        std::thread::spawn(move || {
            let waited_out = wait_for_workers(
                || handles.iter().all(std::thread::JoinHandle::is_finished),
                std::time::Duration::from_millis(250),
            );
            #[cfg(test)]
            wait_while_reaper_close_held(&held);
            if let Err(error) = control.shutdown(std::net::Shutdown::Both) {
                tracing::debug!("retired remote socket already closed: {error}");
            }
            // 끝난 worker부터 join해 그 panic 로그를 먼저 남긴다. panic한 reader가 `failed`로 남는 것은
            // 순서가 아니라 [`ReaderExit`] 덕이다. 끝 표지가 서야 reader를 기다리던 writer가 끝나 모든 join이 돌아온다.
            let (done, rest): (Vec<_>, Vec<_>) = handles
                .into_iter()
                .partition(std::thread::JoinHandle::is_finished);
            let mut failed = false;
            for handle in done.into_iter().chain(rest) {
                if handle.join().is_err() {
                    failed = true;
                    tracing::error!("remote I/O worker panicked during retirement");
                }
            }
            failed |= reap_tunnel(tunnel);
            let end = if failed { RETIRE_FAILED } else { RETIRED };
            let waited = if waited_out { RETIRE_WAITED_OUT } else { 0 };
            receipt.0.store(end | waited, Ordering::Release);
        });
    }
}

/// 회수 스레드가 소켓을 닫기 전에 worker를 기다린다. 대기 상한까지 기다렸으면(`true`) 그 판정을
/// 그대로 돌려준다. 기다림을 끝낸 확인과 같은 값이라, 끝낸 직후 worker가 끝나도 상한까지 기다린
/// 사실이 남는다. poll 사이에 worker가 끝난 실제 시각은 알 수 없다.
fn wait_for_workers(mut all_finished: impl FnMut() -> bool, bound: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + bound;
    loop {
        if all_finished() {
            return false;
        }
        if std::time::Instant::now() >= deadline {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Retain ownership of non-socket connection workers (SSH endpoint resolution) through join.
/// Cancellation is supplied by the attempt token; a slow external resolver is reported by the
/// Remote shutdown deadline instead of treating detached execution as completed.
pub(crate) fn join_attempt_workers(handles: Vec<std::thread::JoinHandle<()>>) -> RetirementReceipt {
    let receipt = RetirementReceipt(Arc::new(std::sync::atomic::AtomicU8::new(0)));
    let result = receipt.clone();
    std::thread::spawn(move || {
        let mut failed = false;
        for handle in handles {
            if handle.join().is_err() {
                failed = true;
                tracing::error!("remote endpoint worker panicked");
            }
        }
        result.0.store(
            if failed { RETIRE_FAILED } else { RETIRED },
            Ordering::Release,
        );
    });
    receipt
}

/// A connected socket and its bounded snapshot tail before the application installs a mirror.
/// There is no App/View or local engine lookup here.
pub struct PreparedConnection {
    pub port: u16,
    pub remote_workspace: u32,
    pub client_id: u32,
    pub name: String,
    pub surfaces: Vec<Value>,
    pub tree: Value,
    /// 서버가 모든 크기 요청에 응답한다. 이 연결에서 응답을 기다릴지 정한다.
    pub resize_acks: bool,
    pub transport: super::client_session::ClientTransport,
}
impl PreparedConnection {
    pub fn connect(
        cancel: super::outbound::AttemptToken,
        port: u16,
        workspace: u32,
        tunnel: Option<tasty_ssh::SshTunnel>,
        wake: Arc<dyn Fn() + Send + Sync>,
        decode: fn(&[u8]) -> Option<MirrorEvent>,
        heartbeat_timeout: std::time::Duration,
    ) -> anyhow::Result<Self> {
        let AttachHandshake {
            conn,
            client_id,
            write_half,
            name,
            surfaces,
            tree,
            resize_acks,
            input_acks,
            explorer_write,
            workspace_close,
            agent_children,
            terminal_read,
            file_download,
            inherit_cwd,
            win32_console_input,
            html,
        } = attach_handshake(
            port,
            workspace,
            "remote pending connection",
            &cancel,
            heartbeat_timeout,
        )?;
        let control = write_half.try_clone()?;
        let (frame_tx, frame_rx) = super::connection::channel();
        frame_tx.enable_input_receipts(input_acks);
        frame_tx.set_win32_console_input(win32_console_input);
        let end = Arc::new(TransportEnd::new());
        frame_tx.bind_failure(end.clone(), wake.clone());
        let output = MirrorOutbox::new(frame_tx.epoch());
        let detach_sent = Arc::new(AtomicBool::new(false));
        let reader_finished = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let reader_probe = ReaderProbe {
            recv_entries: Arc::default(),
            finished: reader_finished.clone(),
            ended_at: Arc::default(),
            first_recv_at: Arc::default(),
        };
        let writer = spawn_attach_write_thread(
            write_half,
            frame_rx,
            AttachWriterEnd {
                transport_end: end.clone(),
                detach_sent: detach_sent.clone(),
                reader_finished: reader_finished.clone(),
                reader_wait_cap: heartbeat_timeout,
                #[cfg(test)]
                seams: WriterSeams::default(),
            },
            wake.clone(),
            "",
        );
        let reader = spawn_attach_reader_thread(
            conn,
            output.clone(),
            frame_tx.clone(),
            wake,
            decode,
            AttachReaderEnd {
                remote_workspace: workspace,
                heartbeat_timeout,
                detach_sent,
                transport_end: end.clone(),
                finished: reader_finished,
                #[cfg(test)]
                gate: None,
                #[cfg(test)]
                probe: Some(reader_probe.clone()),
            },
        );
        let heartbeat = spawn_attach_heartbeat_thread(frame_tx.clone(), end.clone());
        let workers = ConnectionWorkers::new(control, vec![writer, reader, heartbeat]);
        Ok(Self {
            port,
            remote_workspace: workspace,
            client_id,
            name,
            surfaces,
            tree,
            resize_acks,
            transport: super::client_session::ClientTransport {
                workers,
                output,
                end,
                frame_tx,
                tunnel,
                explorer_write,
                workspace_close,
                agent_children,
                terminal_read,
                file_download,
                inherit_cwd,
                agent_hello_sent: false,
                explorer_inbox: Vec::new(),
                html,
                #[cfg(test)]
                reader_probe,
            },
        })
    }
}

fn reap_tunnel(tunnel: Option<tasty_ssh::SshTunnel>) -> bool {
    let Some(mut tunnel) = tunnel else {
        return false;
    };
    let failed = match tunnel.terminate_and_reap() {
        Ok(_) => false,
        Err(error) => {
            tracing::error!(%error,"SSH tunnel wait failed");
            true
        }
    };
    drop(tunnel);
    failed
}
pub(crate) fn retire_tunnel(tunnel: tasty_ssh::SshTunnel) -> RetirementReceipt {
    let receipt = RetirementReceipt(Arc::new(std::sync::atomic::AtomicU8::new(0)));
    let completed = receipt.clone();
    std::thread::spawn(move || {
        let failed = reap_tunnel(Some(tunnel));
        completed.0.store(
            if failed { RETIRE_FAILED } else { RETIRED },
            Ordering::Release,
        );
    });
    receipt
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
