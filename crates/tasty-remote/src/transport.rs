//! Outbound socket handshake, reader, writer and heartbeat. No App or View references.
use super::client_session::{FrameSender, MirrorEvent, MirrorOutbox, OutFrame};
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

pub fn parse_attach_descriptor(ctrl: &Value) -> anyhow::Result<(String, Vec<Value>, Value)> {
    match ctrl.get("event").and_then(|v| v.as_str()) {
        Some("attached_workspace") => {}
        Some("attach_error") => {
            let reason = ctrl
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            anyhow::bail!("workspace attach rejected: {reason}");
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

/// `detach_sent`는 이쪽이 Detach를 보내기 시작했다는 표지다. reader가 뒤따르는 EOF를 원격 원인으로
/// 남기지 않도록 [`AttachReaderEnd`]와 같은 값을 넘긴다.
pub fn spawn_attach_write_thread(
    write_half: TcpStream,
    frame_rx: std::sync::mpsc::Receiver<super::connection::QueuedFrame>,
    disconnected: Arc<AtomicBool>,
    detach_sent: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
    log_suffix: &'static str,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut write_half = write_half;
        loop {
            let queued = match frame_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(queued) => queued,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    if !disconnected.load(Ordering::Acquire) =>
                {
                    continue;
                }
                Err(_) => break,
            };
            if !queued.owner_current() {
                continue;
            }
            let item = queued.frame;
            let detach = item.tag == StreamTag::Detach;
            if !detach && (!queued.epoch.is_active() || disconnected.load(Ordering::Acquire)) {
                continue;
            }
            if detach {
                // 쓰기 뒤에는 이 스레드가 소켓을 닫고 서버도 바로 닫을 수 있어 reader가 먼저 EOF를 본다.
                detach_sent.store(true, Ordering::SeqCst);
            }
            if !write_attach_frame(&mut write_half, &item, &disconnected, &wake, log_suffix) {
                break;
            }
        }
    })
}

// The queue loop owns epoch admission and byte charges; this step owns socket I/O completion.
fn write_attach_frame(
    write_half: &mut TcpStream,
    item: &OutFrame,
    disconnected: &AtomicBool,
    wake: &Arc<dyn Fn() + Send + Sync>,
    log_suffix: &str,
) -> bool {
    if let Err(e) = stream::write_frame(write_half, item.tag, &item.payload) {
        tracing::warn!(
            "attach write thread{log_suffix}: 프레임을 쓰지 못해 연결 종료로 처리한다: {e}"
        );
        disconnected.store(true, Ordering::SeqCst);
        (wake)();
        return false;
    }
    if item.tag == StreamTag::Detach {
        if let Err(error) = write_half.shutdown(std::net::Shutdown::Both) {
            tracing::debug!("remote socket already closed: {error}");
        }
        return false;
    }
    true
}

/// reader가 수신 오류로 끝날 때 원인을 남기는 데 필요한 연결 정보.
pub struct AttachReaderEnd {
    /// 이 연결이 점유한 원격 workspace id. 이쪽 mirror workspace id가 아니다.
    pub remote_workspace: u32,
    /// 이 소켓에 실제로 건 read timeout.
    pub heartbeat_timeout: std::time::Duration,
    /// 이쪽이 Detach를 보내기 시작했다(재동기화·연결 끊기). 쓰기 스레드와 같은 값이다.
    pub detach_sent: Arc<AtomicBool>,
}

impl AttachReaderEnd {
    /// 끊김을 알리고 원인을 남긴다. 이미 끊김을 알렸다면 이 소켓은 이쪽(정리·쓰기 실패)이
    /// 닫은 것이라 원격 원인이 아니므로 debug로만 남긴다.
    fn report(&self, error: &anyhow::Error, disconnected: &AtomicBool) {
        match self.outcome(error, disconnected) {
            (true, message) => tracing::warn!("{message}"),
            (false, message) => tracing::debug!("{message}"),
        }
    }

    /// `disconnected`를 세우고, 경고로 남길지와 문구를 돌려준다.
    fn outcome(&self, error: &anyhow::Error, disconnected: &AtomicBool) -> (bool, String) {
        let cause = stream::ReadFailure::of_error(error);
        let workspace = self.remote_workspace;
        if disconnected.swap(true, Ordering::SeqCst) {
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
}

pub fn spawn_attach_reader_thread(
    mut conn: StreamConnection,
    output: MirrorOutbox,
    frame_tx: FrameSender,
    disconnected: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
    decode_control: fn(&[u8]) -> Option<MirrorEvent>,
    end: AttachReaderEnd,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut mesh_assembler = tasty_ipc::mesh_stream::MeshFrameAssembler::new();
        loop {
            match conn.recv() {
                Ok(frame) => match frame.tag {
                    StreamTag::Data => {
                        if let Some((sid, payload)) = stream::decode_mux(&frame.payload) {
                            output.push(MirrorEvent::Data(sid, payload.to_vec()));
                        }
                        (wake)();
                    }
                    StreamTag::Detach => {
                        disconnected.store(true, Ordering::SeqCst);
                        (wake)();
                        break;
                    }
                    StreamTag::Control => {
                        if String::from_utf8_lossy(&frame.payload).contains("force_detached") {
                            disconnected.store(true, Ordering::SeqCst);
                            (wake)();
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
                        if let Ok(Some((meta, bytes))) = mesh_assembler.push_chunk(&frame.payload)
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
                },
                Err(e) => {
                    end.report(&e, &disconnected);
                    (wake)();
                    break;
                }
            }
        }
    })
}

pub fn spawn_attach_heartbeat_thread(
    raw_frame_tx: FrameSender,
    disconnected: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            std::thread::park_timeout(stream::HEARTBEAT_INTERVAL);
            if disconnected.load(Ordering::SeqCst) {
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
impl RetirementReceipt {
    pub fn is_done(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }
    pub fn failed(&self) -> bool {
        self.0.load(Ordering::Acquire) == 2
    }
}
pub struct ConnectionWorkers {
    receipt: RetirementReceipt,
    control: Option<TcpStream>,
    handles: Vec<std::thread::JoinHandle<()>>,
    tunnel: Option<tasty_ssh::SshTunnel>,
}
impl ConnectionWorkers {
    pub fn new(control: TcpStream, handles: Vec<std::thread::JoinHandle<()>>) -> Self {
        Self {
            receipt: RetirementReceipt(Arc::new(std::sync::atomic::AtomicU8::new(0))),
            control: Some(control),
            handles,
            tunnel: None,
        }
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
        if let Some(control) = &self.control
            && let Err(error) = control.shutdown(std::net::Shutdown::Both)
        {
            tracing::debug!("remote socket already closed: {error}");
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
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            while !handles.iter().all(std::thread::JoinHandle::is_finished)
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if let Err(error) = control.shutdown(std::net::Shutdown::Both) {
                tracing::debug!("retired remote socket already closed: {error}");
            }
            let mut failed = false;
            for handle in handles {
                if handle.join().is_err() {
                    failed = true;
                    tracing::error!("remote I/O worker panicked during retirement");
                }
            }
            failed |= reap_tunnel(tunnel);
            receipt
                .0
                .store(if failed { 2 } else { 1 }, Ordering::Release);
        });
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
        result
            .0
            .store(if failed { 2 } else { 1 }, Ordering::Release);
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
        let disconnected = Arc::new(AtomicBool::new(false));
        frame_tx.bind_failure(disconnected.clone(), wake.clone());
        let output = MirrorOutbox::new(frame_tx.epoch());
        let detach_sent = Arc::new(AtomicBool::new(false));
        let writer = spawn_attach_write_thread(
            write_half,
            frame_rx,
            disconnected.clone(),
            detach_sent.clone(),
            wake.clone(),
            "",
        );
        let reader = spawn_attach_reader_thread(
            conn,
            output.clone(),
            frame_tx.clone(),
            disconnected.clone(),
            wake,
            decode,
            AttachReaderEnd {
                remote_workspace: workspace,
                heartbeat_timeout,
                detach_sent,
            },
        );
        let heartbeat = spawn_attach_heartbeat_thread(frame_tx.clone(), disconnected.clone());
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
                disconnected,
                frame_tx,
                tunnel,
                explorer_write,
                workspace_close,
                agent_children,
                terminal_read,
                agent_hello_sent: false,
                explorer_inbox: Vec::new(),
                html,
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
        completed
            .0
            .store(if failed { 2 } else { 1 }, Ordering::Release);
    });
    receipt
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
