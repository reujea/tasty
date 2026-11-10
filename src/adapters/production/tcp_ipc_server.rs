//! TCP에서 JSON-RPC 요청을 받아 메인 루프의 명령 큐로 전달한다.
//! 전송 타입은 crate::ipc::server에 두고 서버 구현만 이 모듈이 담당한다.

#[cfg(test)]
mod stream_bytes_tests;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use tasty_telemetry::ConnectionStats;

use crate::ipc::port_file;
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::ipc::server::{IpcCommand, IpcWaker};
use crate::ipc::stream::{self, StreamAck, StreamFrame, StreamTag};
use crate::ports::ipc_server::IpcServerPort;
use tasty_ipc::admission::{
    CommandAdmission, INJECTED_DEPTH_LIMIT, Origin, QUEUED_BYTES_LIMIT, QueueLimits,
};
use tasty_ipc::stream_hub::{StreamClientId, StreamContext, StreamInbound};

mod accept_clock;
mod closing_drain;
#[cfg(test)]
mod closing_drain_tests;
mod first_line;
mod response_wait;

use tasty_ipc::protocol::MAX_REQUEST_LINE_BYTES;

/// 정상 줄·EOF·크기 초과·기한 만료·읽기 실패를 구분한다.
enum LineRead {
    /// 개행까지 한 줄을 읽었다.
    Line,
    /// peer 가 연결을 닫았다.
    Eof,
    /// 개행 없이 [`MAX_REQUEST_LINE_BYTES`] 를 채웠다 — 거절을 응답으로 알리고 닫는다.
    TooLong,
    /// 첫 줄 읽기의 기한이 지났다.
    Idle,
    /// 소켓 오류 또는 비-UTF-8.
    Failed,
}

/// 요청 하나를 처리한 뒤 연결을 어떻게 할지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Served {
    /// 다음 요청을 읽는다.
    Keep,
    /// 연결을 끝낸다.
    Close,
    /// 응답을 기다리는 동안 클라이언트가 닫았다. 이미 받아 둔 요청을 응답 없이 큐에 넣고 끝낸다.
    PeerLeft,
}

impl Served {
    fn kept(keep: bool) -> Self {
        if keep { Self::Keep } else { Self::Close }
    }

    #[cfg(test)]
    fn keeps_connection(self) -> bool {
        self == Self::Keep
    }
}

/// 요청을 큐에 넣지 못한 이유.
enum EnqueueRefused {
    /// 명령 큐 입장 상한에 걸렸다. 실행하지 않았다.
    QueueFull(String),
    /// 주 스레드가 큐를 닫았다.
    MainGone,
}

/// 연결마다 스레드가 생기므로 동시 연결 수를 제한한다.
/// 번들 플러그인·attach·mesh·단기 CLI 연결에 여유를 둔 값이며 실측으로 보정한 상한은 아니다.
/// 정상 사용에서 포화되면 system.pressure의 connections와 거절 로그로 재검토한다.
pub(crate) const MAX_CONCURRENT_CONNECTIONS: usize = 256;

/// 한 회차의 처리 건수를 제한해 다른 이벤트도 처리할 수 있게 한다.
/// 연결 상한과 같은 값을 사용하며 시간 예산도 별도로 적용한다(docs/architecture/ipc-server.md).
/// 큐 길이의 상한은 아니다. 끊긴 클라이언트의 요청은 자리가 돌아온 뒤에도 큐에 남으므로
/// 소켓 요청이 연결 수보다 많이 쌓일 수 있고, 그 양은 명령 큐 입장 상한이 막는다.
/// 남은 명령은 헤드리스의 재깨움 또는 GUI IpcPacer로 다음 회차에서 처리한다.
pub(crate) const DRAIN_BUDGET_PER_ROUND: usize = MAX_CONCURRENT_CONNECTIONS;

/// 응답 소켓의 쓰기 timeout. 응답을 읽지 않는 상대 때문에 연결 스레드가 계속 막히지 않게 한다.
/// attach heartbeat와 무관한 요청-응답 연결의 상한이다. 개별 소켓 쓰기에 적용되며
/// 응답 전체를 쓰는 절차의 절대 마감시간은 아니다.
const RESPONSE_WRITE_TIMEOUT: Duration = Duration::from_secs(20);

// 최대 요청 두 건은 대기할 수 있고 연결수×줄크기보다는 작도록 큐 바이트 한도의 관계를 확인한다.
const _: () = assert!(QUEUED_BYTES_LIMIT >= 2 * MAX_REQUEST_LINE_BYTES);
const _: () = assert!(QUEUED_BYTES_LIMIT < MAX_CONCURRENT_CONNECTIONS * MAX_REQUEST_LINE_BYTES);
// 주입 깊이: 한 dispatch 회차가 주입 적체를 한 번에 비울 수 있어야 한다.
const _: () = assert!(INJECTED_DEPTH_LIMIT <= DRAIN_BUDGET_PER_ROUND);

/// 큐 송신자와 입장 기록을 묶어 용량 검사 없는 전송을 피한다.
struct CommandQueue {
    tx: mpsc::Sender<IpcCommand>,
    admission: Arc<CommandAdmission>,
}

/// 격리 시험에서 제품 한도를 채우지 않고 거절을 확인하도록 디버그 환경변수로 제한을 조정한다.
#[cfg(debug_assertions)]
fn queue_limits() -> QueueLimits {
    QueueLimits {
        queued_bytes: debug_env_usize("TASTY_DEBUG_IPC_QUEUE_BYTES").unwrap_or(QUEUED_BYTES_LIMIT),
        injected_depth: debug_env_usize("TASTY_DEBUG_IPC_INJECT_DEPTH")
            .unwrap_or(INJECTED_DEPTH_LIMIT),
    }
}

#[cfg(not(debug_assertions))]
fn queue_limits() -> QueueLimits {
    QueueLimits::DEFAULT
}

/// debug 전용 상한 덮어쓰기 값. 없으면 `None`, 숫자가 아니면 경고를 남기고 `None`.
#[cfg(debug_assertions)]
fn debug_env_usize(name: &str) -> Option<usize> {
    let raw = std::env::var(name).ok()?;
    match raw.trim().parse::<usize>() {
        Ok(v) => {
            tracing::warn!("{name}={v} overrides an IPC admission bound (debug build)");
            Some(v)
        }
        Err(e) => {
            tracing::warn!("{name}={raw:?} is not a number, ignored: {e}");
            None
        }
    }
}

/// 연결 스레드 종료 시 Drop으로 슬롯을 반납한다.
/// 계수는 외부에서 system.pressure로 읽을 수 있도록 주입된 ConnectionStats에 저장한다.
struct ConnectionSlot {
    stats: Arc<ConnectionStats>,
}

impl ConnectionSlot {
    /// 상한 안에서 연결 슬롯을 확보한다. 포화 진입은 warn, 반복 거절은 debug로 남긴다.
    /// 로그 수준과 무관하게 거절 횟수는 모두 집계하며 다시 연결을 수락하면 포화 로그 플래그를 초기화한다.
    fn try_acquire(stats: &Arc<ConnectionStats>, saturated: &AtomicBool) -> Option<Self> {
        let limit = MAX_CONCURRENT_CONNECTIONS as u64;
        if stats.try_open(limit).is_none() {
            if saturated.swap(true, Ordering::Relaxed) {
                tracing::debug!("IPC connection refused (still at {MAX_CONCURRENT_CONNECTIONS})");
            } else {
                tracing::warn!(
                    "IPC connection refused — {MAX_CONCURRENT_CONNECTIONS} concurrent connections already live"
                );
            }
            return None;
        }
        saturated.store(false, Ordering::Relaxed);
        Some(Self {
            stats: stats.clone(),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.stats.close();
    }
}

/// 스트리밍 핸드셰이크 params 에서 추출한 attach 대상(surface/workspace 중 하나).
struct StreamHandshake {
    /// client 가 선언한 스트림 프로토콜 버전. `STREAM_PROTO` 와 다르면 attach 를
    /// **dispatch 하지 않는다** — 상세는 `validate_stream_proto`.
    proto: u32,
    attach_target: Option<u32>,
    attach_workspace: Option<u32>,
}

/// 스트림 읽기 루프가 끝난 원인을 남길 때 필요한 연결 정보.
struct StreamReadEnd {
    client_id: StreamClientId,
    peer: Option<std::net::SocketAddr>,
    /// 이 소켓에 실제로 건 read timeout.
    read_timeout: Duration,
    /// 서버가 Detach를 먼저 보냈다. 그 뒤의 EOF는 client가 응답으로 닫은 것이다.
    detach_sent: Arc<AtomicBool>,
}

impl StreamReadEnd {
    /// 경고로 남길지와 문구. 서버가 먼저 Detach를 보낸 연결의 오류는 정상 종료의 뒷정리다.
    fn read_error_report(&self, error: &std::io::Error) -> (bool, String) {
        let cause = stream::ReadFailure::of_io(error);
        let message = format!(
            "stream client {} from {:?}: 읽기 루프가 끝났다. 원인 {} ({cause:?}): {error}",
            self.client_id,
            self.peer,
            cause.describe(self.read_timeout)
        );
        (!self.detach_sent.load(Ordering::Acquire), message)
    }

    fn log_read_error(&self, error: &std::io::Error) {
        match self.read_error_report(error) {
            (true, message) => tracing::warn!("{message}"),
            (false, message) => tracing::debug!("{message} (Detach 송신 뒤)"),
        }
    }
}

/// loopback의 동적 포트에서 수신하고 포트 파일로 주소를 알리는 IPC 서버.
pub struct TcpIpcServer {
    command_rx: mpsc::Receiver<IpcCommand>,
    /// Sender 사본 — host→plugin sync dispatch 시 외부 thread 가 직접 push.
    command_tx: mpsc::Sender<IpcCommand>,
    port: u16,
    /// Shutdown flag to signal the accept thread to stop.
    shutdown: Arc<AtomicBool>,
    /// Completed publication owned by this listener, used for guarded teardown.
    publication: Option<port_file::PortFilePublication>,
    /// 명령 큐의 입장 장부. 소켓 경로와 호스트 주입기(`HostIpcInjector`)가 같은 것을 든다.
    admission: Arc<CommandAdmission>,
}

impl TcpIpcServer {
    /// 명령 적재 후 waker로 메인 루프를 깨운다. 연결 계수는 호출자가 준 저장소를 공유한다.
    pub fn start_with_port_file(
        port_file_override: Option<String>,
        waker: Option<IpcWaker>,
        stream_ctx: StreamContext,
        connections: Arc<ConnectionStats>,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();

        tracing::info!("IPC server listening on 127.0.0.1:{}", port);

        let custom_port_file = port_file_override.map(std::path::PathBuf::from);

        listener.set_nonblocking(true)?;
        let publication = Self::clear_notify_then_publish_port(
            Self::notify_dir_to_clear(tasty_utils::path::tasty_home(), custom_port_file.as_deref()),
            port,
            custom_port_file.as_deref(),
        )?;

        // 채널 자체는 무제한이다. 앞단에서 바이트·개수 제한을 검사해 대기 대신 거절한다.
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let admission = CommandAdmission::new(queue_limits());
        let shutdown = Arc::new(AtomicBool::new(false));

        let shutdown_clone = shutdown.clone();
        let accept_tx = cmd_tx.clone();
        let accept_admission = admission.clone();
        let first_line_idle = first_line::first_line_idle_timeout();
        // 연결 스레드에서도 슬롯을 반납하므로 계수는 공유한다. 포화 플래그는 로그에만 쓴다.
        let saturated = Arc::new(AtomicBool::new(false));
        let accept = thread::Builder::new()
            .name("ipc-accept".into())
            .spawn(move || {
                // accept 전 대기는 요청 큐에서 보이지 않으므로 연결 계수에 대기 상한을 기록한다.
                let mut clock = accept_clock::AcceptClock::new(std::time::Instant::now());
                loop {
                    if shutdown_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => {
                            connections.record_accept_wait(clock.bound(std::time::Instant::now()));
                            let Some(slot) = ConnectionSlot::try_acquire(&connections, &saturated)
                            else {
                                Self::refuse_saturated_connection(stream);
                                continue;
                            };
                            let queue = CommandQueue {
                                tx: accept_tx.clone(),
                                admission: accept_admission.clone(),
                            };
                            let waker = waker.clone();
                            let stream_ctx = stream_ctx.clone();
                            thread::spawn(move || {
                                Self::handle_connection(
                                    stream,
                                    queue,
                                    waker,
                                    stream_ctx,
                                    slot,
                                    first_line_idle,
                                );
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            clock.saw_empty(std::time::Instant::now());
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => {
                            tracing::warn!("IPC accept error: {}", e);
                            break;
                        }
                    }
                }
            });

        if let Err(error) = accept {
            if let Err(cleanup) = publication.cleanup_if_owned() {
                tracing::warn!(%cleanup, "failed to clean up port after accept spawn failure");
            }
            return Err(error.into());
        }

        Ok(Self {
            command_rx: cmd_rx,
            command_tx: cmd_tx,
            port,
            shutdown,
            publication: Some(publication),
            admission,
        })
    }

    pub(crate) fn admission(&self) -> Arc<CommandAdmission> {
        self.admission.clone()
    }

    /// 이전 완료 로그를 정리한 뒤 포트 파일을 쓴다. 먼저 주소를 공개하면
    /// 새로 기록된 완료 로그를 부팅 정리가 지울 수 있다.
    /// 홈을 모르거나 포트 파일이 데이터 루트 밖이면 로그 정리는 건너뛴다.
    fn clear_notify_then_publish_port(
        notify_dir: Option<std::path::PathBuf>,
        port: u16,
        custom_port_file: Option<&std::path::Path>,
    ) -> Result<port_file::PortFilePublication> {
        Self::clear_notify_then_publish(notify_dir, || {
            port_file::publish_port_file_to(port, custom_port_file)
        })
    }

    /// 포트 파일이 데이터 루트에 있을 때만 그 루트의 notify를 정리한다.
    /// 외부 경로를 지정한 호스트가 같은 홈을 쓰는 다른 호스트의 로그를 지우지 않게 한다.
    /// 로그 writer의 경로는 데이터 루트이므로 포트 파일 옆 notify로 대체하지 않는다.
    /// 경로를 정규화할 수 없으면 입력 경로 그대로 비교한다(docs/features/child-terminal/index.md).
    fn notify_dir_to_clear(
        data_root: Option<std::path::PathBuf>,
        custom_port_file: Option<&std::path::Path>,
    ) -> Option<std::path::PathBuf> {
        let root = data_root?;
        if let Some(port_file) = custom_port_file {
            let port_dir = match port_file.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => dir,
                _ => std::path::Path::new("."),
            };
            let same = match (
                std::fs::canonicalize(port_dir),
                std::fs::canonicalize(&root),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => port_dir == root.as_path(),
            };
            if !same {
                tracing::info!(
                    "port file {} is outside the data root {} — leaving {}/notify alone, \
                     another host may own it",
                    port_file.display(),
                    root.display(),
                    root.display()
                );
                return None;
            }
        }
        Some(root.join("notify"))
    }

    /// 정리 후 발행한다. 시험은 발행 시점에 정리가 끝났는지 확인한다.
    fn clear_notify_then_publish<T, F>(
        notify_dir: Option<std::path::PathBuf>,
        publish: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        if let Some(dir) = notify_dir {
            Self::clear_notify_dir(&dir);
        }
        publish()
    }

    /// notify를 삭제한다. 없는 경우는 정상이며 다른 오류는 기록한다. 다음 쓰기가 다시 생성한다.
    fn clear_notify_dir(dir: &std::path::Path) {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!("failed to clear notify dir {}: {}", dir.display(), e);
            }
        }
    }

    /// 연결 슬롯은 인자로 소유해 함수가 어떤 경로로 끝나도 반납한다.
    fn handle_connection(
        stream: std::net::TcpStream,
        queue: CommandQueue,
        waker: Option<IpcWaker>,
        stream_ctx: StreamContext,
        _slot: ConnectionSlot,
        first_line_idle: Duration,
    ) {
        let Some((mut reader, mut writer, peer)) = Self::prepare_stream(stream) else {
            return;
        };

        // Read the first line manually so the BufReader retains any bytes
        // buffered after it. On a streaming-channel upgrade those buffered bytes
        // are the start of the binary frames following the handshake line.
        let mut line = String::new();
        match Self::read_first_line(&mut reader, &mut line, peer, first_line_idle) {
            // 첫 줄의 기한은 여기까지다 — 걷지 못하면 요청 사이의 쉼에 상한이 남으므로 닫는다.
            LineRead::Line if !first_line::clear_read_timeout(&reader) => return,
            LineRead::Line => {}
            LineRead::Idle => {
                Self::refuse_idle_first_line(&mut writer, peer, first_line_idle);
                return;
            }
            LineRead::TooLong => {
                Self::refuse_oversized_line(&mut writer, peer);
                return;
            }
            LineRead::Eof | LineRead::Failed => return,
        }

        // Streaming upgrade: first line is `{"method":"stream.open",...}`. The
        // connection leaves the request-response model and becomes a framed
        // bidirectional pipe.
        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(line.trim())
            && req.method == stream::STREAM_OPEN_METHOD
        {
            Self::handle_stream_connection(reader, writer, req, stream_ctx, peer);
            return;
        }

        Self::arm_response_write_timeout(&writer);
        Self::run_request_response_loop(&mut reader, &mut writer, &mut line, &queue, &waker, peer);

        tracing::debug!("IPC client disconnected from {:?}", peer);
    }

    /// blocking 모드로 바꾸고 reader/writer 소켓 사본을 만든다. 실패하면 로그 후 종료한다.
    fn prepare_stream(
        stream: std::net::TcpStream,
    ) -> Option<(
        BufReader<std::net::TcpStream>,
        std::net::TcpStream,
        Option<std::net::SocketAddr>,
    )> {
        let peer = stream.peer_addr().ok();
        tracing::debug!("IPC client connected from {:?}", peer);

        if !Self::configure_socket(&stream) {
            return None;
        }

        let reader = BufReader::new(match stream.try_clone() {
            Ok(s) => s,
            Err(_) => return None,
        });
        let writer = stream;
        Some((reader, writer, peer))
    }

    /// 연결별 I/O는 blocking으로 사용한다. 전환 실패 시 종료한다.
    /// 작은 요청·프레임의 전송 지연을 줄이려고 TCP_NODELAY를 설정하며 실패는 경고만 한다.
    fn configure_socket(stream: &std::net::TcpStream) -> bool {
        if let Err(e) = stream.set_nonblocking(false) {
            tracing::warn!("Failed to set stream to blocking mode: {}", e);
            return false;
        }
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("Failed to set TCP_NODELAY on IPC stream: {}", e);
        }
        true
    }

    /// 일반 RPC 응답에만 쓰기 timeout을 설정한다. 스트림 업그레이드의 전용 writer에는 적용하지 않는다.
    /// 업그레이드하면 전용 writer가 이 핸들을 그대로 넘겨받으므로 업그레이드 여부를 정한 뒤 설정한다.
    /// 설정 실패는 기록하고 timeout 없이 계속한다.
    fn arm_response_write_timeout(writer: &std::net::TcpStream) {
        if let Err(e) = writer.set_write_timeout(Some(RESPONSE_WRITE_TIMEOUT)) {
            tracing::warn!("Failed to set IPC response write timeout: {e}");
        }
    }

    /// 크기 한도 안에서 한 줄을 읽는다. 초과 시 응답과 연결 종료는 호출자가 담당한다.
    /// 나머지 바이트가 소켓에 남으므로 같은 연결에서 다음 요청으로 읽으면 안 된다.
    /// BufRead 인자로 받아 크기 판정은 실제 소켓 없이도 시험할 수 있다.
    fn read_line_capped<R: BufRead>(
        reader: &mut R,
        line: &mut String,
        peer: Option<std::net::SocketAddr>,
    ) -> LineRead {
        match reader
            .by_ref()
            .take(MAX_REQUEST_LINE_BYTES as u64)
            .read_line(line)
        {
            Ok(0) => LineRead::Eof,
            Ok(n) if n == MAX_REQUEST_LINE_BYTES && !line.ends_with('\n') => {
                tracing::warn!(
                    "IPC request line from {:?} exceeded {} bytes — refusing the line and closing the connection",
                    peer,
                    MAX_REQUEST_LINE_BYTES
                );
                LineRead::TooLong
            }
            Ok(_) => LineRead::Line,
            Err(e) if first_line::is_idle_expiry(&e) => LineRead::Idle,
            Err(e) => {
                tracing::warn!("IPC read error from {:?}: {}", peer, e);
                LineRead::Failed
            }
        }
    }

    /// 첫 줄만 읽어 업그레이드 뒤 이진 프레임이 될 버퍼를 보존한다.
    /// 크기 초과와 기한 만료는 연결을 끝내기 전에 거절 응답이 필요하다.
    fn read_first_line(
        reader: &mut BufReader<std::net::TcpStream>,
        line: &mut String,
        peer: Option<std::net::SocketAddr>,
        within: Duration,
    ) -> LineRead {
        let outcome = Self::read_line_capped(
            &mut first_line::DeadlineReader::new(reader, within),
            line,
            peer,
        );
        if matches!(outcome, LineRead::Eof) {
            tracing::debug!("IPC client disconnected (eof) {:?}", peer);
        }
        outcome
    }

    fn run_request_response_loop(
        reader: &mut BufReader<std::net::TcpStream>,
        writer: &mut std::net::TcpStream,
        line: &mut String,
        queue: &CommandQueue,
        waker: &Option<IpcWaker>,
        peer: Option<std::net::SocketAddr>,
    ) {
        let mut served = Self::process_request_line(line, queue, waker, writer, peer);
        while served == Served::Keep {
            line.clear();
            match Self::read_line_capped(reader, line, peer) {
                LineRead::Line => {}
                LineRead::TooLong => {
                    Self::refuse_oversized_line(writer, peer);
                    return;
                }
                LineRead::Eof | LineRead::Failed | LineRead::Idle => return,
            }
            served = Self::process_request_line(line, queue, waker, writer, peer);
        }
        if served == Served::PeerLeft {
            response_wait::enqueue_left_requests(reader, line, queue, waker, peer);
        }
    }

    /// 스트림 writer는 push를 보내고 현재 스레드는 수신 프레임을 메인 루프로 넘긴다.
    /// SSH와 loopback 접근을 신뢰하며 핸드셰이크 session_token은 검사하지 않는다(docs/design/systems/memory.md).
    fn handle_stream_connection(
        mut reader: BufReader<std::net::TcpStream>,
        writer: std::net::TcpStream,
        req: JsonRpcRequest,
        ctx: StreamContext,
        peer: Option<std::net::SocketAddr>,
    ) {
        let read_timeout = ctx.heartbeat_timeout.get();
        Self::arm_stream_read_timeout(&reader, read_timeout);

        let client_id = ctx.hub.alloc_id();
        let sink_rx = ctx.hub.register(client_id);
        tracing::debug!("stream client {} upgraded from {:?}", client_id, peer);

        let handshake = Self::parse_stream_handshake(&ctx, client_id, req);
        if !Self::validate_stream_proto(&ctx, client_id, &handshake, peer) {
            // No async writer or attach exists yet. Flush this final ACK while its
            // original registration is live; unregister must not authorize old queues.
            Self::write_stream_rejection(writer, sink_rx);
            Self::finish_stream_connection(&ctx, client_id, None, peer);
            return;
        }
        let detach_sent = Arc::new(AtomicBool::new(false));
        let write_handle =
            Self::spawn_stream_write_thread(writer, sink_rx, client_id, detach_sent.clone());
        Self::push_stream_ack(&ctx, client_id);
        Self::dispatch_stream_attach(&ctx, client_id, &handshake);
        let end = StreamReadEnd {
            client_id,
            peer,
            read_timeout,
            detach_sent,
        };
        Self::run_stream_read_loop(&ctx, &end, &mut reader);
        Self::finish_stream_connection(&ctx, client_id, Some(write_handle), peer);
    }

    /// FIN/RST 없는 단절을 읽기 timeout으로 감지한다. 유휴 연결도 주기 Ping으로 유지한다.
    fn arm_stream_read_timeout(reader: &BufReader<std::net::TcpStream>, timeout: Duration) {
        if let Err(e) = reader.get_ref().set_read_timeout(Some(timeout)) {
            tracing::warn!("stream client: failed to set read timeout: {e}");
        }
    }

    /// 프로토콜이 다르면 ok:false ack를 보내고 attach 전에 거절한다.
    /// 사용할 수 없는 클라이언트가 점유를 먼저 잡아 정상 접속을 막지 않게 하기 위해서다(docs/dev-guide/attach-behavior.md).
    fn validate_stream_proto(
        ctx: &StreamContext,
        client_id: StreamClientId,
        handshake: &StreamHandshake,
        peer: Option<std::net::SocketAddr>,
    ) -> bool {
        if handshake.proto == stream::STREAM_PROTO {
            return true;
        }
        tracing::warn!(
            "stream client {client_id} from {peer:?}: proto {} != server {} — attach 를 dispatch 하지 않고 거절합니다(점유 미획득).",
            handshake.proto,
            stream::STREAM_PROTO,
        );
        let ack = StreamAck {
            ok: false,
            client_id: Some(client_id),
            proto: stream::STREAM_PROTO,
            error: Some(format!(
                "unsupported stream proto {} (server speaks {})",
                handshake.proto,
                stream::STREAM_PROTO
            )),
        };
        let ack_bytes = serde_json::to_vec(&ack).unwrap_or_default();
        let _ = ctx // best-effort 거절 ack — client 가 이미 끊겼으면 무해(연결은 바로 정리된다).
            .hub
            .push(client_id, StreamFrame::new(StreamTag::Control, ack_bytes));
        false
    }

    /// Only the rejecting connection thread writes here, before any async writer exists.
    /// The held frame retains its byte lease and must still belong to the live registration.
    fn write_stream_rejection(
        mut writer: std::net::TcpStream,
        sink_rx: tasty_ipc::stream_hub::SinkReceiver,
    ) {
        if let Err(error) = writer.set_write_timeout(Some(RESPONSE_WRITE_TIMEOUT)) {
            tracing::warn!(%error, "failed to bound stream rejection write");
            return;
        }
        if let Ok(frame) = sink_rx.recv_held_timeout(Duration::ZERO)
            && frame.tag == StreamTag::Control
            && frame.is_current()
            && let Err(error) = stream::write_frame(&mut writer, frame.tag, &frame.payload)
        {
            tracing::debug!(%error, "stream rejection peer disconnected before ACK");
        }
    }

    /// Successful handshake ack uses the single asynchronous socket writer.
    fn push_stream_ack(ctx: &StreamContext, client_id: StreamClientId) {
        let ack = StreamAck {
            ok: true,
            client_id: Some(client_id),
            proto: stream::STREAM_PROTO,
            error: None,
        };
        let ack_bytes = serde_json::to_vec(&ack).unwrap_or_default();
        let _ = ctx // best-effort ack push — PushResult(Result 아님) 무시: client 끊겼으면 무해.
            .hub
            .push(client_id, StreamFrame::new(StreamTag::Control, ack_bytes));
    }

    /// sink 등록을 해제하고 writer 종료를 기다린 뒤 메인 루프에 연결 종료를 알린다.
    fn finish_stream_connection(
        ctx: &StreamContext,
        client_id: StreamClientId,
        write_handle: Option<thread::JoinHandle<()>>,
        peer: Option<std::net::SocketAddr>,
    ) {
        ctx.hub.unregister(client_id); // drops the sink sender → write thread exits
        if write_handle.is_some_and(|handle| handle.join().is_err()) {
            tracing::warn!("IPC writer thread panicked during connection cleanup");
        }
        // 메인 루프가 남아 있으면 이 연결의 점유를 해제한다. 이미 종료됐으면 통지 실패를 무시한다.
        if ctx
            .inbound_tx
            .send(StreamInbound::Disconnected { client_id })
            .is_ok()
        {
            (ctx.waker)();
        }
        tracing::debug!("stream client {} disconnected from {:?}", client_id, peer);
    }

    /// surface/workspace attach 대상 또는 bulk 전용 연결을 구분한다.
    /// bulk는 점유하지 않고 파일 청크를 나르며 첫 프레임 전에 hub에 종류를 등록한다.
    fn parse_stream_handshake(
        ctx: &StreamContext,
        client_id: StreamClientId,
        req: JsonRpcRequest,
    ) -> StreamHandshake {
        let open_params = serde_json::from_value::<stream::StreamOpenParams>(req.params).ok();
        let attach_target = open_params.as_ref().and_then(|p| p.target);
        let attach_workspace = open_params.as_ref().and_then(|p| p.target_workspace);
        let bulk_workspace = open_params.as_ref().and_then(|p| p.bulk_workspace);
        if let Some(ws) = bulk_workspace {
            ctx.hub.register_bulk(client_id, ws);
        }
        StreamHandshake {
            proto: open_params.as_ref().map(|p| p.proto).unwrap_or_default(),
            attach_target,
            attach_workspace,
        }
    }

    /// 실제 데이터가 없을 때 주기 Ping을 보내 상대의 읽기 timeout을 갱신한다.
    /// Detach를 보냈으면 `detach_sent`를 세워 읽기 쪽이 뒤따르는 EOF를 정상 종료로 본다.
    fn spawn_stream_write_thread(
        writer: std::net::TcpStream,
        sink_rx: tasty_ipc::stream_hub::SinkReceiver,
        client_id: StreamClientId,
        detach_sent: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        let mut w = writer;
        thread::spawn(move || {
            loop {
                match sink_rx.recv_held_timeout(stream::HEARTBEAT_INTERVAL) {
                    Ok(frame) => {
                        if !frame.is_current() {
                            continue;
                        }
                        if let Err(e) = stream::write_frame(&mut w, frame.tag, &frame.payload) {
                            tracing::warn!(
                                "stream client {client_id}: {:?} 프레임 송신 실패로 쓰기를 멈춘다(수신 timeout과 별개인 송신 오류): {e}",
                                frame.tag
                            );
                            break;
                        }
                        if frame.tag == StreamTag::Detach {
                            detach_sent.store(true, Ordering::Release);
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
        })
    }

    /// 점유·스냅샷·출력 tap은 엔진을 소유한 메인 루프에 맡긴다. 결과는 별도 Control로 보낸다.
    /// surface와 workspace가 모두 지정되면 workspace가 우선이다.
    fn dispatch_stream_attach(
        ctx: &StreamContext,
        client_id: StreamClientId,
        handshake: &StreamHandshake,
    ) {
        if let Some(target_workspace_id) = handshake.attach_workspace {
            if ctx
                .inbound_tx
                .send(StreamInbound::AttachWorkspaceRequest {
                    client_id,
                    target_workspace_id,
                })
                .is_ok()
            {
                (ctx.waker)();
            }
        } else if let Some(target_surface_id) = handshake.attach_target
            && ctx
                .inbound_tx
                .send(StreamInbound::AttachRequest {
                    client_id,
                    target_surface_id,
                })
                .is_ok()
        {
            (ctx.waker)();
        }
    }

    /// 수신 프레임을 메인 루프로 넘겨 입력·크기 변경 등을 처리한다.
    /// 읽기 오류로 끝나면 원인을 분류해 남긴다. client의 Detach와 메인 루프 소멸은 정상 종료다.
    fn run_stream_read_loop(
        ctx: &StreamContext,
        end: &StreamReadEnd,
        reader: &mut BufReader<std::net::TcpStream>,
    ) {
        let client_id = end.client_id;
        loop {
            match stream::read_frame(reader) {
                Ok(frame) if frame.tag == StreamTag::Detach => {
                    tracing::debug!("stream client {client_id}: client가 Detach로 연결을 끝냈다");
                    break;
                }
                Ok(frame) => {
                    if ctx
                        .inbound_tx
                        .send(StreamInbound::Frame { client_id, frame })
                        .is_err()
                    {
                        break; // main loop gone
                    }
                    (ctx.waker)();
                }
                Err(e) => {
                    end.log_read_error(&e);
                    break;
                }
            }
        }
    }

    /// Handle one request line of a request-response connection. Empty and
    /// unparseable lines keep the connection; send/recv/write failures close it.
    fn process_request_line(
        line: &str,
        queue: &CommandQueue,
        waker: &Option<IpcWaker>,
        writer: &mut std::net::TcpStream,
        peer: Option<std::net::SocketAddr>,
    ) -> Served {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Served::Keep;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                return Served::kept(Self::send_parse_error(writer, e));
            }
        };

        Self::dispatch_and_await(request, trimmed.len(), queue, waker, writer, peer)
    }

    /// JSON 한 줄을 쓰고 flush 를 시도한다(write 가 실패해도 flush 는 그대로
    /// 시도된다 — 두 결과를 각각 반환해 호출자가 로깅/제어흐름을 결정한다).
    fn write_json_line(
        writer: &mut std::net::TcpStream,
        json: &str,
    ) -> (std::io::Result<()>, std::io::Result<()>) {
        let write_result = writeln!(writer, "{}", json);
        let flush_result = writer.flush();
        (write_result, flush_result)
    }

    /// 파싱 오류를 응답한다. 쓰기/flush가 실패하면 잘린 JSON에 다음 응답이 붙지 않도록 연결을 닫는다.
    fn send_parse_error(writer: &mut std::net::TcpStream, e: serde_json::Error) -> bool {
        let err_resp = JsonRpcResponse::error(
            serde_json::Value::Null,
            -32700,
            format!("Parse error: {}", e),
        );
        let json = serde_json::to_string(&err_resp).unwrap();
        let (write_result, flush_result) = Self::write_json_line(writer, &json);
        if let Err(e) = write_result {
            tracing::warn!("IPC parse-error response write failed: {e}");
            return false;
        }
        if let Err(e) = flush_result {
            tracing::warn!("IPC parse-error response flush failed: {e}");
            return false;
        }
        true
    }

    /// accept 스레드를 막지 않도록 non-blocking 쓰기 한 번으로 포화 거절을 시도하고 닫는다.
    /// 닫기 전에 이미 도착한 요청 바이트를 기다리지 않고 비워 RST 가 응답을 지우지 않게 한다.
    /// 부분 전송이나 실패, 비운 뒤 도착한 바이트가 있으면 거절 사유 전달을 보장하지 못한다.
    /// 업그레이드 판별 전이므로 스트림 클라이언트는 이 JSON을 프레임 오류로 읽을 수 있다(docs/architecture/ipc-server.md).
    /// 포화 진입의 warn은 슬롯 검사에서 이미 남기므로 여기서는 debug로 기록한다.
    fn refuse_saturated_connection(stream: std::net::TcpStream) {
        if let Err(e) = stream.set_nonblocking(true) {
            tracing::debug!("IPC saturation refusal could not go non-blocking: {e}");
            return;
        }
        let line = Self::saturation_refusal_line();
        let mut w = stream;
        // 한 번만 쓴다. `write_all` 은 부분 전송에서 다시 시도하므로 여기서는 안 쓴다.
        Self::log_saturation_refusal_write(w.write(line.as_bytes()), line.len());
        closing_drain::discard_arrived_then_close(w);
    }

    fn saturation_refusal_line() -> String {
        let resp = JsonRpcResponse::error(
            serde_json::Value::Null,
            crate::ipc::protocol::ERR_CONNECTION_LIMIT_REACHED,
            format!(
                "the server already holds {MAX_CONCURRENT_CONNECTIONS} concurrent connections \
                 and did not read this one — nothing ran, so retry it as is"
            ),
        );
        format!("{}\n", serde_json::to_string(&resp).unwrap())
    }

    fn log_saturation_refusal_write(result: std::io::Result<usize>, len: usize) {
        match result {
            Ok(n) if n == len => {}
            Ok(n) => tracing::debug!("IPC saturation refusal only partly sent ({n}/{len})"),
            Err(e) => tracing::debug!("IPC saturation refusal not sent: {e}"),
        }
    }

    /// 줄 크기 초과를 알린 뒤 연결을 끝낸다. 남은 바이트를 새 요청으로 읽지 않게 한다.
    /// 완전한 JSON이 아니므로 id는 null이다. 첫 줄에서는 업그레이드 판별 전이어서
    /// 응답용 write timeout을 여기서 설정한다.
    fn refuse_oversized_line(writer: &mut std::net::TcpStream, peer: Option<std::net::SocketAddr>) {
        Self::arm_response_write_timeout(writer);
        let resp = JsonRpcResponse::error(
            serde_json::Value::Null,
            crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG,
            format!(
                "request line exceeded {MAX_REQUEST_LINE_BYTES} bytes — the connection is closed"
            ),
        );
        let json = serde_json::to_string(&resp).unwrap();
        let (write_result, flush_result) = Self::write_json_line(writer, &json);
        if let Err(e) = write_result {
            tracing::debug!("IPC oversize refusal write failed for {peer:?}: {e}");
        }
        if let Err(e) = flush_result {
            tracing::debug!("IPC oversize refusal flush failed for {peer:?}: {e}");
        }
        #[cfg(not(target_os = "linux"))]
        closing_drain::drain_before_close(writer);
        #[cfg(target_os = "linux")]
        closing_drain::half_close_and_discard_arrived(writer);
    }

    /// 첫 줄 기한 만료를 알리고 연결을 끝낸다. 미완성 줄이므로 id는 null이다.
    /// 상대가 응답도 읽지 않을 수 있어 write timeout을 먼저 설정한다.
    fn refuse_idle_first_line(
        writer: &mut std::net::TcpStream,
        peer: Option<std::net::SocketAddr>,
        within: Duration,
    ) {
        tracing::warn!(
            "IPC client {peer:?} sent no complete request line within {within:?} — closing the connection"
        );
        Self::arm_response_write_timeout(writer);
        let resp = JsonRpcResponse::error(
            serde_json::Value::Null,
            crate::ipc::protocol::ERR_FIRST_LINE_IDLE,
            format!(
                "no complete request line arrived within {} ms of connecting — nothing ran; \
                 the connection is closed, send the request right after connecting",
                within.as_millis()
            ),
        );
        Self::write_final_line(writer, &resp, peer);
    }

    fn write_final_line(
        writer: &mut std::net::TcpStream,
        resp: &JsonRpcResponse,
        peer: Option<std::net::SocketAddr>,
    ) {
        let json = serde_json::to_string(resp).unwrap();
        let (write_result, flush_result) = Self::write_json_line(writer, &json);
        if let Err(e) = write_result {
            tracing::debug!("IPC closing refusal write failed for {peer:?}: {e}");
        }
        if let Err(e) = flush_result {
            tracing::debug!("IPC closing refusal flush failed for {peer:?}: {e}");
        }
    }

    /// 명령을 메인 루프로 보내고 연결 스레드에서 응답을 기다린다. 반환값은 연결 유지 여부다.
    /// 기다리는 동안 연결 슬롯을 유지한다. response_timeout_ms가 없거나 0이면 응답이 오거나
    /// 클라이언트가 연결을 닫을 때까지 기다린다. 응답 송신자가 사라져도 대기가 끝난다.
    /// 큐 용량 거절은 실행하지 않았다는 응답을 보내며 같은 연결을 유지한다.
    fn dispatch_and_await(
        request: JsonRpcRequest,
        wire_bytes: usize,
        queue: &CommandQueue,
        waker: &Option<IpcWaker>,
        writer: &mut std::net::TcpStream,
        peer: Option<std::net::SocketAddr>,
    ) -> Served {
        let rpc_id = request.id.clone().unwrap_or(serde_json::Value::Null);
        let wait_bound = request
            .response_timeout_ms
            .filter(|ms| *ms > 0)
            .map(Duration::from_millis);

        match Self::enqueue_request(request, wire_bytes, queue, waker) {
            Ok((resp_rx, lifecycle)) => Self::await_dispatch_response(
                &resp_rx, wait_bound, &lifecycle, rpc_id, writer, peer,
            ),
            Err(EnqueueRefused::QueueFull(reason)) => {
                Served::kept(Self::answer_queue_full(writer, rpc_id, &reason, peer))
            }
            Err(EnqueueRefused::MainGone) => Served::Close,
        }
    }

    /// 입장 판정을 거쳐 요청을 큐에 넣고 주 스레드를 깨운다. 응답 수신 측과 수명 핸들을 돌려준다.
    fn enqueue_request(
        request: JsonRpcRequest,
        wire_bytes: usize,
        queue: &CommandQueue,
        waker: &Option<IpcWaker>,
    ) -> Result<
        (
            mpsc::Receiver<JsonRpcResponse>,
            tasty_ipc::server::LifecycleHandle,
        ),
        EnqueueRefused,
    > {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        let mut cmd = IpcCommand::with_wire_bytes(request, resp_tx, wire_bytes);
        if let Err(refusal) = cmd.admit(&queue.admission, Origin::Socket) {
            return Err(EnqueueRefused::QueueFull(refusal.to_string()));
        }
        let lifecycle = cmd.lifecycle();

        if queue.tx.send(cmd).is_err() {
            tracing::warn!("IPC cmd_tx.send failed (main thread shut down?)");
            return Err(EnqueueRefused::MainGone);
        }

        lifecycle.mark_enqueued();
        if let Some(waker) = waker {
            waker();
        }
        Ok((resp_rx, lifecycle))
    }

    /// 응답 대기 기한이 지나도 연결은 유지한다. 실행 전이면 취소하고 시작됐다면 결과 불명으로 답한다.
    /// 기다리는 동안 클라이언트가 닫으면 대기를 끝내 자리를 돌려준다. 이미 큐에 넣은 요청은 그대로 두며
    /// 그 응답은 버려진다. 클라이언트가 미리 보낸 나머지 요청은 호출자가 응답 없이 큐에 넣는다.
    fn await_dispatch_response(
        resp_rx: &mpsc::Receiver<JsonRpcResponse>,
        wait_bound: Option<Duration>,
        lifecycle: &tasty_ipc::server::LifecycleHandle,
        rpc_id: serde_json::Value,
        writer: &mut std::net::TcpStream,
        peer: Option<std::net::SocketAddr>,
    ) -> Served {
        let response = match response_wait::wait_for_response(resp_rx, wait_bound, writer) {
            response_wait::Waited::Answer(response) => response,
            // 만료 순간 명령이 아직 큐에 있었으면 실행되지 않게 막고 "실행 안 됨" 으로
            // 답한다(docs/architecture/ipc-server.md). 이미 시작됐으면 종전 그대로 결과 불명이다.
            response_wait::Waited::Expired => {
                let bound = wait_bound.unwrap_or_default();
                match lifecycle.withdraw() {
                    tasty_ipc::server::Withdraw::NotRun => {
                        tasty_ipc::server::expired_before_run_response(rpc_id, bound)
                    }
                    tasty_ipc::server::Withdraw::Started => {
                        Self::wait_expired_response(rpc_id, bound, peer)
                    }
                }
            }
            response_wait::Waited::SenderDropped => {
                tracing::warn!("IPC resp_rx.recv failed: response_tx dropped without sending");
                return Served::Close;
            }
            response_wait::Waited::PeerClosed => {
                tracing::debug!(
                    "IPC client {peer:?} closed the connection while its request waited for the host"
                );
                return Served::PeerLeft;
            }
        };
        lifecycle.record_answer(&response);
        Served::kept(Self::write_dispatch_response(writer, &response, peer))
    }

    /// 시작된 요청의 대기 기한이 지났다. 실행은 계속될 수 있어 결과 불명으로 답한다.
    /// 이전 응답 채널이 끊겨 늦은 결과가 소켓에 섞이지 않으므로 연결은 유지한다.
    /// 중복 효과를 피하려면 재전송 전에 상태를 조회해야 한다.
    fn wait_expired_response(
        rpc_id: serde_json::Value,
        bound: Duration,
        peer: Option<std::net::SocketAddr>,
    ) -> JsonRpcResponse {
        tracing::warn!(
            "IPC response wait of {:?} expired for {:?} — the request may still be running",
            bound,
            peer
        );
        JsonRpcResponse::error(
            rpc_id,
            crate::ipc::protocol::ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN,
            format!(
                "the caller's response timeout of {} ms expired before the host answered; \
                 whether the request ran is unknown — read the state before resending it",
                bound.as_millis()
            ),
        )
    }

    /// 큐가 찬 요청은 실행하지 않고 거절한다. 연결은 유지해 나중에 다시 시도할 수 있다.
    fn answer_queue_full(
        writer: &mut std::net::TcpStream,
        rpc_id: serde_json::Value,
        reason: &str,
        peer: Option<std::net::SocketAddr>,
    ) -> bool {
        tracing::debug!("IPC request from {peer:?} refused at the command queue: {reason}");
        let resp = JsonRpcResponse::error(
            rpc_id,
            crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL,
            format!("{reason} — nothing ran; retry it as is after a pause"),
        );
        Self::write_dispatch_response(writer, &resp, peer)
    }

    fn write_dispatch_response(
        writer: &mut std::net::TcpStream,
        response: &JsonRpcResponse,
        peer: Option<std::net::SocketAddr>,
    ) -> bool {
        let json = serde_json::to_string(response).unwrap();
        let (write_result, flush_result) = Self::write_json_line(writer, &json);
        if !Self::log_dispatch_write_result(write_result, flush_result) {
            return false;
        }
        tracing::debug!("IPC response sent for {:?}", peer);
        true
    }

    fn log_dispatch_write_result(
        write_result: std::io::Result<()>,
        flush_result: std::io::Result<()>,
    ) -> bool {
        if let Err(e) = write_result {
            tracing::warn!("IPC write error: {}", e);
            return false;
        }
        if let Err(e) = flush_result {
            tracing::warn!("IPC flush error: {}", e);
            return false;
        }
        true
    }
}

impl IpcServerPort for TcpIpcServer {
    /// 큐에서 꺼내면 입장 용량을 반납한다. 실행 중인 명령까지 대기량으로 세지 않는다.
    fn try_recv(&self) -> Result<IpcCommand, mpsc::TryRecvError> {
        let mut cmd = self.command_rx.try_recv()?;
        cmd.mark_dequeued();
        Ok(cmd)
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn command_sender(&self) -> mpsc::Sender<IpcCommand> {
        self.command_tx.clone()
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    fn read(input: &[u8]) -> (LineRead, String) {
        let mut reader = BufReader::new(input);
        let mut line = String::new();
        let outcome = TcpIpcServer::read_line_capped(&mut reader, &mut line, None);
        (outcome, line)
    }

    #[test]
    fn a_normal_line_is_read_whole() {
        let (outcome, line) = read(b"{\"method\":\"system.info\"}\nnext\n");
        assert!(matches!(outcome, LineRead::Line));
        assert_eq!(line, "{\"method\":\"system.info\"}\n");
    }

    #[test]
    fn eof_is_told_apart_from_a_refusal() {
        let (outcome, line) = read(b"");
        assert!(matches!(outcome, LineRead::Eof));
        assert!(line.is_empty());
    }

    #[test]
    fn a_line_longer_than_the_cap_is_refused() {
        let input = vec![b'a'; MAX_REQUEST_LINE_BYTES + 1];
        let (outcome, _) = read(&input);
        assert!(matches!(outcome, LineRead::TooLong));
    }

    #[test]
    fn a_line_that_ends_exactly_at_the_cap_is_not_refused() {
        let mut input = vec![b'a'; MAX_REQUEST_LINE_BYTES - 1];
        input.push(b'\n');
        assert_eq!(input.len(), MAX_REQUEST_LINE_BYTES);
        let (outcome, line) = read(&input);
        assert!(
            matches!(outcome, LineRead::Line),
            "상한에서 개행으로 끝나는 줄은 초과가 아니다"
        );
        assert_eq!(line.len(), MAX_REQUEST_LINE_BYTES);
    }

    // 거절 응답의 바이트를 확인한다. 실제 accept 루프에 상한+1개 연결을 여는 시험은 아니다.
    #[test]
    fn a_refused_connection_is_told_why_before_it_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let (server_side, _) = listener.accept().expect("accept");

        TcpIpcServer::refuse_saturated_connection(server_side);

        let mut got = String::new();
        BufReader::new(client)
            .read_line(&mut got)
            .expect("거절 응답을 읽어야 한다");
        assert!(
            !got.is_empty(),
            "연결이 그냥 닫혔다 — 거절이 응답으로 안 왔다"
        );
        let resp: JsonRpcResponse =
            serde_json::from_str(got.trim()).expect("JSON 한 줄이어야 한다");
        let err = resp.error.expect("에러 응답이어야 한다");
        assert_eq!(
            err.code,
            crate::ipc::protocol::ERR_CONNECTION_LIMIT_REACHED,
            "연결 상한 거절이 전송 계층 코드로 안 왔다"
        );
        assert!(
            err.message
                .contains(&MAX_CONCURRENT_CONNECTIONS.to_string()),
            "거절 문구가 상한 값을 안 싣는다: {}",
            err.message
        );
    }

    // 명령을 시작한 상태로 응답 채널을 유지해 실행 후 timeout을 재현한다.
    // 시작 전 만료는 다른 오류여야 한다.
    fn test_queue(tx: mpsc::Sender<IpcCommand>, limits: QueueLimits) -> CommandQueue {
        CommandQueue {
            tx,
            admission: CommandAdmission::new(limits),
        }
    }

    #[test]
    fn a_wait_past_the_callers_bound_answers_unknown_outcome() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");

        let (cmd_tx, cmd_rx) = mpsc::channel::<IpcCommand>();
        let queue = test_queue(cmd_tx, QueueLimits::DEFAULT);
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.list".to_string(),
            params: serde_json::Value::Null,
            id: Some(serde_json::json!(7)),
            session_token: None,
            response_timeout_ms: Some(50),
            idempotency_key: None,
        };
        let (held_tx, held_rx) = mpsc::channel();
        let taker = std::thread::spawn(move || {
            let cmd = cmd_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("the request reaches the queue");
            let stats = Arc::new(tasty_ipc::dispatch::DispatchStats::default());
            assert_eq!(cmd.claim(&stats), tasty_ipc::server::Claim::Run);
            let cell = cmd.outcome_cell();
            // 받는 쪽이 이미 실패해 사라졌을 수 있다 — 그때는 시험이 다른 실패문으로 끝난다.
            let _ = held_tx.send(cmd);
            cell
        });

        // 회귀가 무한 대기로 남지 않도록 시험 자체에도 완료 기한을 둔다.
        let (done_tx, done_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let kept =
                TcpIpcServer::dispatch_and_await(request, 0, &queue, &None, &mut server_side, None)
                    .keeps_connection();
            // 받는 쪽이 이미 실패해 사라졌을 수 있다 — 그때는 시험이 다른 실패문으로
            // 끝나므로 이 전송의 실패에 대해 할 일이 없다.
            let _ = done_tx.send(kept);
        });
        let kept = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("호출자가 실은 상한이 안 걸렸다 — 기다림이 안 끝났다");
        assert!(kept, "만료는 연결을 끊는 사건이 아니다");
        waiter.join().expect("waiter");
        let cell = taker.join().expect("taker");
        drop(held_rx);
        assert_eq!(
            cell.get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error {
                code: crate::ipc::protocol::ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN
            }),
            "the slow-request row reads the answer the waiter sent (docs/architecture/ipc-server.md)"
        );

        let mut got = String::new();
        BufReader::new(client)
            .read_line(&mut got)
            .expect("만료 응답을 읽어야 한다");
        let resp: JsonRpcResponse =
            serde_json::from_str(got.trim()).expect("JSON 한 줄이어야 한다");
        assert_eq!(resp.id, serde_json::json!(7), "요청의 id 를 안 돌려줬다");
        let err = resp.error.expect("에러 응답이어야 한다");
        assert_eq!(
            err.code,
            crate::ipc::protocol::ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN,
            "만료가 '결과 불명' 코드로 안 왔다"
        );
        assert!(
            err.message.contains("unknown"),
            "문구가 결과 불명을 말하지 않는다: {}",
            err.message
        );
    }

    #[test]
    fn a_wait_that_ends_while_queued_answers_not_run_and_the_command_is_not_run_later() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");

        let (cmd_tx, cmd_rx) = mpsc::channel::<IpcCommand>();
        let queue = test_queue(cmd_tx, QueueLimits::DEFAULT);
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.create".to_string(),
            params: serde_json::Value::Null,
            id: Some(serde_json::json!(9)),
            session_token: None,
            response_timeout_ms: Some(30),
            idempotency_key: None,
        };
        let kept =
            TcpIpcServer::dispatch_and_await(request, 0, &queue, &None, &mut server_side, None)
                .keeps_connection();
        assert!(kept, "만료는 연결을 끊는 사건이 아니다");

        let queued = cmd_rx
            .try_recv()
            .expect("the request was queued and nobody took it");
        assert_eq!(
            queued.claim(&Arc::new(tasty_ipc::dispatch::DispatchStats::default())),
            tasty_ipc::server::Claim::Withdrawn,
            "the waiter withdrew it, so taking it now must not run it"
        );
        assert_eq!(
            queued.outcome_cell().get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error {
                code: crate::ipc::protocol::ERR_EXPIRED_BEFORE_RUN
            }),
            "the slow-request row reads the answer the waiter sent (docs/architecture/ipc-server.md)"
        );

        drop(server_side);
        let mut got = String::new();
        BufReader::new(client)
            .read_line(&mut got)
            .expect("the expiry answer");
        let resp: JsonRpcResponse = serde_json::from_str(got.trim()).expect("one JSON line");
        assert_eq!(resp.id, serde_json::json!(9));
        let err = resp.error.expect("an error response");
        assert_eq!(err.code, crate::ipc::protocol::ERR_EXPIRED_BEFORE_RUN);
        assert!(err.message.contains("did not run"), "{}", err.message);
    }

    // 명령을 계속 보관해도 큐에서 꺼낸 순간 대기 용량을 반납해야 한다.
    #[test]
    fn a_dequeued_command_stops_counting_while_it_is_still_held() {
        let (tx, rx) = mpsc::channel();
        let admission = CommandAdmission::new(QueueLimits::DEFAULT);
        let server = TcpIpcServer {
            command_rx: rx,
            command_tx: tx.clone(),
            port: 0,
            shutdown: Arc::new(AtomicBool::new(false)),
            // This fabricated server owns no port publication.
            publication: None,
            admission: admission.clone(),
        };
        let (resp_tx, _resp_rx) = mpsc::sync_channel(1);
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.list".to_string(),
            params: serde_json::Value::Null,
            id: Some(serde_json::json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let mut cmd = IpcCommand::with_wire_bytes(request, resp_tx, 100);
        cmd.admit(&admission, Origin::Socket)
            .expect("an empty queue takes one");
        tx.send(cmd).expect("queue");
        assert_eq!(admission.snapshot().queued_bytes, 100);

        let held = server.try_recv().expect("dequeue");
        let snap = admission.snapshot();
        assert_eq!(
            (snap.queued_bytes, snap.queued_commands),
            (0, 0),
            "a command taken off the queue still counts — the share is returned on drop, not on dequeue"
        );
        drop(held);
    }

    // 다른 한도는 유지하고 큐 바이트 한도만 낮춰 거절과 용량 반환 뒤 재시도를 확인한다.
    #[test]
    fn a_request_past_the_queue_bytes_limit_is_refused_and_the_connection_kept() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let queue = test_queue(
            cmd_tx,
            QueueLimits {
                queued_bytes: 10,
                injected_depth: INJECTED_DEPTH_LIMIT,
            },
        );
        let request = |id: u64| JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.list".to_string(),
            params: serde_json::Value::Null,
            id: Some(serde_json::json!(id)),
            session_token: None,
            response_timeout_ms: Some(30),
            idempotency_key: None,
        };

        let held = queue
            .admission
            .admit(8, Origin::Socket)
            .expect("an empty queue takes one");
        let kept =
            TcpIpcServer::dispatch_and_await(request(3), 8, &queue, &None, &mut server_side, None)
                .keeps_connection();
        assert!(kept, "a queue refusal must not close the connection");
        assert!(
            cmd_rx.try_recv().is_err(),
            "the refused request reached the queue"
        );

        drop(held);
        let kept =
            TcpIpcServer::dispatch_and_await(request(4), 8, &queue, &None, &mut server_side, None)
                .keeps_connection();
        assert!(kept);
        assert!(
            cmd_rx.try_recv().is_ok(),
            "after the share came back the request must be queued"
        );

        drop(server_side);
        let mut lines = BufReader::new(client).lines();
        let first: JsonRpcResponse =
            serde_json::from_str(&lines.next().expect("line").expect("read")).expect("json");
        assert_eq!(first.id, serde_json::json!(3));
        let err = first.error.expect("refusal is an error response");
        assert_eq!(err.code, crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL);
        assert!(err.message.contains("nothing ran"), "{}", err.message);
        let second: JsonRpcResponse =
            serde_json::from_str(&lines.next().expect("line").expect("read")).expect("json");
        assert_eq!(
            second.error.expect("nobody answers the queued one").code,
            crate::ipc::protocol::ERR_EXPIRED_BEFORE_RUN,
            "the second request was queued and never taken, so its wait expired before it ran"
        );
        let snap = queue.admission.snapshot();
        assert_eq!(snap.refused_bytes, 1);
    }

    // 무기한 대기 전체를 증명하지는 못하므로, 제한 없는 요청이 짧은 시간 안에 끝나지 않는지 확인한다.
    #[test]
    fn a_request_without_a_bound_is_still_waited_on() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let _client = std::net::TcpStream::connect(addr).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let queue = test_queue(cmd_tx, QueueLimits::DEFAULT);
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "workspace.list".to_string(),
            params: serde_json::Value::Null,
            id: Some(serde_json::json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };

        let (done_tx, done_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let kept =
                TcpIpcServer::dispatch_and_await(request, 0, &queue, &None, &mut server_side, None)
                    .keeps_connection();
            // 받는 쪽이 이미 실패해 사라졌을 수 있다 — 그때는 시험이 다른 실패문으로
            // 끝나므로 이 전송의 실패에 대해 할 일이 없다.
            let _ = done_tx.send(kept);
        });

        if let Ok(kept) = done_rx.recv_timeout(Duration::from_millis(300)) {
            panic!("상한을 안 실었는데 기다림이 끝났다 (반환값 {kept}) — 기본값이 상한이 됐다");
        }

        // 명령을 집어 응답 통로를 버리면 기다림이 풀린다. 스레드를 매달아 두지 않는다.
        drop(cmd_rx);
        waiter.join().expect("waiter");
    }

    // 실제 요청 루프에서 크기 초과 응답을 확인한다. 첫 빈 줄은 dispatch 없이 다음 줄로 넘어가게 한다.
    #[test]
    fn an_oversized_line_is_refused_with_a_code_not_a_bare_close() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let client = std::net::TcpStream::connect(addr).expect("connect");
        let (server_side, _) = listener.accept().expect("accept");

        // 큰 입력의 송신은 별도 스레드에서 한다. 서버가 닫히면 남은 쓰기는 오류로 끝난다.
        let mut client_w = client.try_clone().expect("clone");
        std::thread::spawn(move || {
            let blob = vec![b'x'; MAX_REQUEST_LINE_BYTES + 16];
            // 실패가 정상 종료다 — 서버가 상한에서 읽기를 멈추면 남은 바이트는 갈 곳이
            // 없고, 시험이 끝나며 소켓이 닫히면 이 쓰기가 오류로 풀린다. 그 오류에
            // 대해 할 일이 없다.
            let _ = client_w.write_all(&blob);
        });

        let mut reader = BufReader::new(server_side.try_clone().expect("clone"));
        let mut writer = server_side;
        let (cmd_tx, _cmd_rx) = mpsc::channel();
        let queue = test_queue(cmd_tx, QueueLimits::DEFAULT);
        let mut first = String::new();
        TcpIpcServer::run_request_response_loop(
            &mut reader,
            &mut writer,
            &mut first,
            &queue,
            &None,
            None,
        );

        // 서버를 닫아 거절이 없어도 client 읽기가 EOF로 끝나게 한다.
        drop(reader);
        drop(writer);

        let mut got = String::new();
        BufReader::new(client)
            .read_line(&mut got)
            .expect("거절 응답을 읽어야 한다");
        assert!(
            !got.is_empty(),
            "연결이 그냥 닫혔다 — 거절이 응답으로 안 왔다"
        );
        let resp: JsonRpcResponse =
            serde_json::from_str(got.trim()).expect("JSON 한 줄이어야 한다");
        let err = resp.error.expect("에러 응답이어야 한다");
        assert_eq!(
            err.code,
            crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG,
            "줄 상한 거절이 전송 계층 코드로 안 왔다"
        );
        assert!(
            err.message.contains(&MAX_REQUEST_LINE_BYTES.to_string()),
            "거절 문구가 상한 값을 안 싣는다: {}",
            err.message
        );
    }

    // 실제 소켓에 설정된 timeout을 다시 읽는다. 송신 버퍼를 채워 timeout 발생까지 검증하지는 않는다.
    #[test]
    fn the_response_write_timeout_is_actually_on_the_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let _client = std::net::TcpStream::connect(addr).expect("connect");
        let (server_side, _) = listener.accept().expect("accept");

        assert_eq!(
            server_side.write_timeout().expect("read back"),
            None,
            "새 소켓에는 상한이 없어야 한다 — 이 시험의 전제"
        );

        TcpIpcServer::arm_response_write_timeout(&server_side);

        assert_eq!(
            server_side.write_timeout().expect("read back"),
            Some(RESPONSE_WRITE_TIMEOUT),
            "쓰기 상한이 소켓에 안 걸렸다"
        );
    }

    fn slots() -> (Arc<ConnectionStats>, AtomicBool) {
        (Arc::new(ConnectionStats::default()), AtomicBool::new(false))
    }

    /// `handle_connection` 을 실제 소켓 쌍 위에서 돌린다. 끝나면 `done` 에 신호가 온다.
    fn serve_one(
        first_line_idle: Duration,
    ) -> (
        std::net::TcpStream,
        Arc<ConnectionStats>,
        mpsc::Receiver<()>,
        mpsc::Receiver<IpcCommand>,
    ) {
        let (client, stats, done, cmd_rx, _admission) = serve_one_watching_queue(first_line_idle);
        (client, stats, done, cmd_rx)
    }

    /// [`serve_one`] 과 같되 명령 큐 입장 기록도 돌려준다.
    fn serve_one_watching_queue(
        first_line_idle: Duration,
    ) -> (
        std::net::TcpStream,
        Arc<ConnectionStats>,
        mpsc::Receiver<()>,
        mpsc::Receiver<IpcCommand>,
        Arc<CommandAdmission>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let client =
            std::net::TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (server_side, _) = listener.accept().expect("accept");
        let (stats, sat) = slots();
        let slot = ConnectionSlot::try_acquire(&stats, &sat).expect("seat");
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let queue = test_queue(cmd_tx, QueueLimits::DEFAULT);
        let admission = queue.admission.clone();
        let (inbound_tx, _inbound_rx) = mpsc::channel();
        let ctx = StreamContext {
            hub: tasty_ipc::stream_hub::StreamHub::new(),
            inbound_tx,
            waker: Arc::new(|| {}),
            heartbeat_timeout: Default::default(),
        };
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            TcpIpcServer::handle_connection(server_side, queue, None, ctx, slot, first_line_idle);
            // 받는 쪽이 이미 실패해 사라졌을 수 있다 — 그때 시험은 다른 실패문으로 끝난다.
            let _ = done_tx.send(());
        });
        (client, stats, done_rx, cmd_rx, admission)
    }

    const WAITING_REQUEST: &[u8] =
        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"workspace.list\"}\n";

    // 주 스레드가 응답하지 않는 동안 클라이언트가 요청을 보내고 끊는다. 연결 스레드는 응답을
    // 기다리지 않고 끝나 자리를 돌려줘야 한다. 그러지 않으면 이런 클라이언트가 쌓여 상한에 닿는다.
    #[test]
    fn a_client_that_leaves_while_the_host_is_stalled_returns_its_seat() {
        let (mut client, stats, done, cmd_rx) = serve_one(Duration::from_secs(20));
        client.write_all(WAITING_REQUEST).expect("send");
        // 명령은 큐에 들어갔지만 아무도 처리하지 않는다 — 응답하지 못하는 주 스레드.
        let cmd = cmd_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the request reached the queue");
        drop(client);
        done.recv_timeout(Duration::from_secs(10)).expect(
            "the connection thread kept waiting for the host after its client left — the seat is never returned",
        );
        assert_eq!(stats.snapshot().live, 0, "the seat was not returned");
        // 늦게 처리돼도 응답은 받을 곳이 없을 뿐이다.
        tasty_ipc::server::send_response(
            &cmd.response_tx,
            JsonRpcResponse::success(serde_json::json!(1), serde_json::json!([])),
        );
    }

    // 두 요청을 한 번에 쓰고 닫은 클라이언트. 두 줄은 서버 reader 버퍼에 함께 들어가 소켓에는 EOF 만
    // 남는다. 앞 요청의 응답이 확인 간격을 넘겨도 뒤 요청은 응답 없이 큐에 들어가야 한다(끊긴 클라이언트의
    // 요청도 실행한다는 계약). 끝에 개행 없는 줄도 종전 루프처럼 한 요청으로 읽는다. 해석할 수 없는 줄은
    // 건너뛴다. 큐 입장 기록은 들어간 명령 수를 세고, 연결 자리는 돌아온다.
    #[test]
    fn requests_sent_ahead_by_a_client_that_left_still_reach_the_queue() {
        let (mut client, stats, done, cmd_rx, admission) =
            serve_one_watching_queue(Duration::from_secs(20));
        client
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"workspace.list\"}\n\
                  {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"workspace.list\"}\n\
                  not json\n\
                  {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"workspace.list\"}",
            )
            .expect("send");
        drop(client);
        let first = cmd_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the first request reached the queue");
        assert_eq!(first.request.id, Some(serde_json::json!(1)));
        done.recv_timeout(Duration::from_secs(10))
            .expect("the connection thread did not end after its client left");
        assert_eq!(stats.snapshot().live, 0, "the seat was not returned");
        let left: Vec<IpcCommand> = cmd_rx.try_iter().collect();
        let ids: Vec<_> = left.iter().map(|c| c.request.id.clone()).collect();
        assert_eq!(
            ids,
            vec![Some(serde_json::json!(2)), Some(serde_json::json!(3))],
            "requests the client sent ahead were not queued"
        );
        assert_eq!(admission.snapshot().queued_commands, 3);
        drop(first);
        drop(left);
        assert_eq!(admission.snapshot().queued_commands, 0);
    }

    // 응답이 늦어도 연결을 연 채 기다리는 클라이언트는 응답을 받는다. 상대 확인 간격을 두 번 넘겨 답한다.
    #[test]
    fn a_client_that_stays_gets_a_late_answer() {
        let (mut client, stats, done, cmd_rx) = serve_one(Duration::from_secs(20));
        client.write_all(WAITING_REQUEST).expect("send");
        let cmd = cmd_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the request reached the queue");
        std::thread::sleep(response_wait::PEER_CHECK_INTERVAL * 2 + Duration::from_millis(200));
        assert!(
            done.try_recv().is_err(),
            "the connection ended while its client was still waiting"
        );
        assert_eq!(stats.snapshot().live, 1);
        tasty_ipc::server::send_response(
            &cmd.response_tx,
            JsonRpcResponse::success(serde_json::json!(1), serde_json::json!("late")),
        );
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut got = String::new();
        BufReader::new(client).read_line(&mut got).expect("read");
        let resp: JsonRpcResponse = serde_json::from_str(got.trim()).expect("one JSON line");
        assert_eq!(resp.result, Some(serde_json::json!("late")));
    }

    #[test]
    fn a_connection_that_never_sends_a_line_is_told_why_and_its_seat_returned() {
        let (client, stats, done, _cmd_rx) = serve_one(Duration::from_millis(100));
        assert_eq!(stats.snapshot().live, 1);
        done.recv_timeout(Duration::from_secs(5))
            .expect("the connection thread never ended — the first line has no deadline");
        assert_eq!(stats.snapshot().live, 0, "the seat was not returned");

        let mut got = String::new();
        BufReader::new(client).read_line(&mut got).expect("read");
        let resp: JsonRpcResponse =
            serde_json::from_str(got.trim()).expect("one JSON line, not a bare close");
        let err = resp.error.expect("error response");
        assert_eq!(err.code, crate::ipc::protocol::ERR_FIRST_LINE_IDLE);
        assert!(err.message.contains("nothing ran"), "{}", err.message);
    }

    #[test]
    fn a_pause_after_the_first_line_does_not_close_the_connection() {
        let (mut client, stats, done, _cmd_rx) = serve_one(Duration::from_millis(100));
        client.write_all(b"\n").expect("first line");
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            done.try_recv().is_err(),
            "the connection closed during a pause after its first line"
        );
        assert_eq!(stats.snapshot().live, 1);

        client.write_all(b"{not json\n").expect("second line");
        let mut got = String::new();
        BufReader::new(client.try_clone().expect("clone"))
            .read_line(&mut got)
            .expect("read");
        let resp: JsonRpcResponse = serde_json::from_str(got.trim()).expect("json");
        assert_eq!(resp.error.expect("error").code, -32700);
        drop(client);
        done.recv_timeout(Duration::from_secs(5))
            .expect("ends on EOF");
    }

    #[test]
    fn a_slot_is_returned_when_it_drops() {
        let (live, sat) = slots();
        {
            let _held = ConnectionSlot::try_acquire(&live, &sat).expect("첫 자리는 잡힌다");
            assert_eq!(live.snapshot().live, 1);
        }
        assert_eq!(live.snapshot().live, 0, "Drop 이 자리를 반납해야 한다");
    }

    #[test]
    fn a_refusal_does_not_leak_the_count() {
        let (live, sat) = slots();
        let held: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| ConnectionSlot::try_acquire(&live, &sat).expect("상한까지는 잡힌다"))
            .collect();
        assert_eq!(live.snapshot().live, MAX_CONCURRENT_CONNECTIONS as u64);

        assert!(
            ConnectionSlot::try_acquire(&live, &sat).is_none(),
            "상한을 넘는 연결은 거절된다"
        );
        assert_eq!(
            live.snapshot().live,
            MAX_CONCURRENT_CONNECTIONS as u64,
            "거절은 계수를 그대로 두어야 한다"
        );
        drop(held);
        assert_eq!(live.snapshot().live, 0);
    }

    #[test]
    fn a_returned_slot_lets_the_next_one_in() {
        let (live, sat) = slots();
        let mut held: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| ConnectionSlot::try_acquire(&live, &sat).expect("상한까지는 잡힌다"))
            .collect();
        assert!(ConnectionSlot::try_acquire(&live, &sat).is_none());

        held.pop();
        assert!(
            ConnectionSlot::try_acquire(&live, &sat).is_some(),
            "반납된 자리로 다음 연결이 들어와야 한다"
        );
    }

    // 슬롯 계수가 외부에서 읽는 공용 계측에 반영되는지 확인한다.
    #[test]
    fn the_seat_count_lands_in_the_gauge_the_diagnostic_reads() {
        let (gauge, sat) = slots();
        let held = ConnectionSlot::try_acquire(&gauge, &sat).expect("첫 자리는 잡힌다");
        let during = gauge.snapshot();
        drop(held);
        let after = gauge.snapshot();

        assert_eq!(during.live, 1, "붙어 있는 동안은 게이지가 1 이어야 한다");
        assert_eq!(after.live, 0, "반납되면 0 으로 돌아온다");
        assert_eq!(after.accepted, 1, "누계는 반납해도 안 내려간다");
        assert_eq!(after.live_max, 1, "최댓값이 그 순간을 남긴다");
        assert_eq!(after.refused_saturated, 0);
    }

    // 반복 로그를 줄여도 거절 횟수는 모두 집계해야 한다.
    #[test]
    fn a_folded_log_line_does_not_fold_the_refusal_count() {
        let (gauge, sat) = slots();
        let _held: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| ConnectionSlot::try_acquire(&gauge, &sat).expect("상한까지는 잡힌다"))
            .collect();
        for _ in 0..3 {
            assert!(ConnectionSlot::try_acquire(&gauge, &sat).is_none());
        }
        assert_eq!(
            gauge.snapshot().refused_saturated,
            3,
            "세 번 거절했으면 세 번 세야 한다 — 로그가 한 줄이어도"
        );
    }

    // 전송 한도는 실제 부팅에서 사용하는 기본 entry_max_mb의 JSON escape 최악 크기와 비교한다.
    // 이 시험은 사용자가 바꾼 설정값까지 확인하지 않는다.
    #[test]
    fn the_cap_clears_the_largest_payload_the_store_accepts() {
        let entry_max_bytes = tasty_settings::MemorySettings::default()
            .entry_max_mb
            .saturating_mul(1024 * 1024) as usize;
        let worst_case_on_the_wire = entry_max_bytes * 6;
        assert!(
            MAX_REQUEST_LINE_BYTES >= worst_case_on_the_wire,
            "상한 {MAX_REQUEST_LINE_BYTES} 가 저장소 기본 cap {entry_max_bytes} 의 escape \
             최악치 {worst_case_on_the_wire} 보다 작다 — 정상 요청을 거절하게 된다"
        );
    }
}

#[cfg(test)]
mod notify_cleanup_tests {
    use super::*;

    #[test]
    fn without_a_port_file_override_the_data_root_notify_dir_is_cleared() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            TcpIpcServer::notify_dir_to_clear(Some(tmp.path().to_path_buf()), None),
            Some(tmp.path().join("notify"))
        );
    }

    #[test]
    fn a_port_file_inside_the_data_root_keeps_the_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let port_file = tmp.path().join("other-name.port");
        assert_eq!(
            TcpIpcServer::notify_dir_to_clear(Some(tmp.path().to_path_buf()), Some(&port_file)),
            Some(tmp.path().join("notify"))
        );
    }

    // 다른 포트 경로의 인스턴스가 원래 데이터 루트의 알림 로그를 지우지 않아야 한다.
    #[test]
    fn a_port_file_outside_the_data_root_leaves_the_notify_dir_alone() {
        let home = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let live = home.path().join("notify").join("9.log");
        std::fs::create_dir_all(live.parent().unwrap()).unwrap();
        std::fs::write(&live, b"surface 9 task complete (via spawn)\n").unwrap();
        let port_file = elsewhere.path().join("tasty.port");

        let target =
            TcpIpcServer::notify_dir_to_clear(Some(home.path().to_path_buf()), Some(&port_file));
        assert_eq!(
            target, None,
            "데이터 루트 밖의 포트 파일인데 청소 대상이 나왔다"
        );

        TcpIpcServer::clear_notify_then_publish_port(target, 4244, Some(&port_file))
            .expect("port file write");
        assert!(port_file.exists(), "포트 파일은 그래도 써야 한다");
        assert!(
            live.exists(),
            "다른 호스트의 살아 있는 완료 로그가 지워졌다"
        );
    }

    #[test]
    fn an_unresolved_data_root_has_nothing_to_clear() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            TcpIpcServer::notify_dir_to_clear(None, Some(&tmp.path().join("tasty.port"))),
            None
        );
    }

    #[test]
    fn clear_removes_populated_notify_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let notify = tmp.path().join("notify");
        std::fs::create_dir_all(&notify).unwrap();
        for id in ["11.log", "42.log", "1337.log"] {
            std::fs::write(notify.join(id), b"spawn done\n").unwrap();
        }
        assert!(notify.exists());

        TcpIpcServer::clear_notify_dir(&notify);

        assert!(!notify.exists(), "notify dir should be removed");
    }

    #[test]
    fn clear_is_noop_when_dir_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let notify = tmp.path().join("notify");
        assert!(!notify.exists());

        TcpIpcServer::clear_notify_dir(&notify);

        assert!(!notify.exists());
    }

    // 정리와 포트 파일 발행이 모두 끝난 뒤의 상태를 확인한다. 순서는 아래 시험이 별도로 검사한다.
    #[test]
    fn the_notify_dir_is_cleared_before_the_port_file_is_published() {
        let tmp = tempfile::tempdir().unwrap();
        let notify = tmp.path().join("notify");
        std::fs::create_dir_all(&notify).unwrap();
        for i in 0..512 {
            std::fs::write(notify.join(format!("{i}.log")), b"surface done\n").unwrap();
        }
        let port_file = tmp.path().join("tasty.port");

        TcpIpcServer::clear_notify_then_publish_port(Some(notify.clone()), 4242, Some(&port_file))
            .expect("port file write");

        assert!(port_file.exists(), "포트 파일이 쓰여야 한다");
        assert!(
            !notify.exists(),
            "포트 파일이 보이는 시점에 notify/ 는 이미 치워져 있어야 한다"
        );
    }

    // 두 작업이 끝난 뒤 결과만으로는 순서를 모르므로 발행하는 바로 그 시점에 검사한다.
    #[test]
    fn the_cleanup_has_already_finished_when_the_publish_step_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let notify = tmp.path().join("notify");
        std::fs::create_dir_all(&notify).unwrap();
        for i in 0..512 {
            std::fs::write(notify.join(format!("{i}.log")), b"surface done\n").unwrap();
        }

        let mut notify_seen_at_publish = None;
        TcpIpcServer::clear_notify_then_publish(Some(notify.clone()), || {
            notify_seen_at_publish = Some(notify.exists());
            Ok(())
        })
        .expect("publish");

        assert_eq!(
            notify_seen_at_publish,
            Some(false),
            "발행 단계가 도는 시점에 notify/ 가 아직 남아 있었다 — 청소가 뒤로 밀렸다"
        );
    }

    #[test]
    fn failed_publication_keeps_the_original_and_does_not_report_startup_success() {
        let tmp = tempfile::tempdir().unwrap();
        let notify = tmp.path().join("notify");
        std::fs::create_dir(&notify).unwrap();
        std::fs::write(notify.join("1.log"), b"stale").unwrap();
        let path = tmp.path().join("tasty.port");
        std::fs::create_dir(&path).unwrap();
        assert!(
            TcpIpcServer::clear_notify_then_publish_port(Some(notify.clone()), 4242, Some(&path))
                .is_err()
        );
        assert!(!notify.exists());
        assert!(path.is_dir());
    }

    #[test]
    fn an_unresolved_home_skips_the_cleanup_and_still_publishes_the_port() {
        let tmp = tempfile::tempdir().unwrap();
        let port_file = tmp.path().join("tasty.port");

        TcpIpcServer::clear_notify_then_publish_port(None, 4243, Some(&port_file))
            .expect("port file write");

        assert!(port_file.exists());
    }
}

impl Drop for TcpIpcServer {
    fn drop(&mut self) {
        // 포트 파일이 남는 시간을 확인하기 위해 Drop 종료 시간을 기록한다.
        let t_drop = std::time::Instant::now();
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(publication) = &self.publication
            && let Err(error) = publication.cleanup_if_owned()
        {
            tracing::trace!(%error, "port publication cleanup failed");
        }
        tracing::info!(
            target: "tasty::shutdown",
            ms = t_drop.elapsed().as_secs_f64() * 1000.0,
            "S5d ipc_server_drop (accept stop + port file 제거)"
        );
    }
}

#[cfg(test)]
mod handshake_tests {
    use std::sync::Arc;
    use std::sync::mpsc;

    use super::*;
    use tasty_ipc::stream_hub::StreamHub;

    fn ctx() -> (StreamContext, mpsc::Receiver<StreamInbound>) {
        let (inbound_tx, inbound_rx) = mpsc::channel();
        let ctx = StreamContext {
            hub: StreamHub::new(),
            inbound_tx,
            waker: Arc::new(|| {}),
            heartbeat_timeout: Default::default(),
        };
        (ctx, inbound_rx)
    }

    /// 연결 스레드가 서버 쪽 소켓을 소유한다. 시험은 client 쪽에서 연결이 끊기는지로 읽기 기한을 본다.
    /// 소켓 사본으로 기한을 읽는 방법은 쓰지 않는다. Windows 에서는 사본이 원래 핸들에 건 기한을
    /// 돌려주지 않는다(CI 에서 None 을 받았다).
    fn accept_stream(listener: &std::net::TcpListener, ctx: &StreamContext) -> std::net::TcpStream {
        let client =
            std::net::TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (server, peer) = listener.accept().expect("accept");
        let reader = BufReader::new(server.try_clone().expect("clone"));
        let req = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "stream.open",
            "params": {"proto": stream::STREAM_PROTO},
            "id": 1,
        }))
        .expect("request");
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            TcpIpcServer::handle_stream_connection(reader, server, req, ctx, Some(peer));
        });
        client
    }

    /// ack 는 읽기 기한을 건 뒤에 나가므로, ack 를 받았으면 그 연결의 기한은 정해졌다.
    fn read_ack(client: &mut std::net::TcpStream) {
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("client timeout");
        let frame = stream::read_frame(client).expect("ack");
        assert_eq!(frame.tag, StreamTag::Control);
    }

    /// 서버가 `within` 안에 연결을 닫는지 본다. 그 사이에 오는 Ping 등 프레임은 건너뛴다.
    fn closed_within(client: &mut std::net::TcpStream, within: Duration) -> bool {
        let deadline = std::time::Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            client.set_read_timeout(Some(left)).expect("client timeout");
            match stream::read_frame(client) {
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return false;
                }
                Err(_) => return true,
            }
        }
    }

    // client 는 handshake 뒤 아무것도 보내지 않는다. 서버는 그 연결에 건 읽기 기한이 지나면 연결을 닫는다.
    // 기본값(30초)이 아니라 설정값(1초)이 걸려야 먼저 연 연결이 10초 안에 닫힌다.
    #[test]
    fn a_settings_change_reaches_only_connections_opened_after_it() {
        let (ctx, _rx) = ctx();
        ctx.heartbeat_timeout.set(Duration::from_secs(1));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");

        let mut first = accept_stream(&listener, &ctx);
        read_ack(&mut first);

        ctx.heartbeat_timeout.set(Duration::from_secs(60));
        let mut second = accept_stream(&listener, &ctx);
        read_ack(&mut second);

        assert!(
            closed_within(&mut first, Duration::from_secs(10)),
            "먼저 연 연결이 열 때의 설정(1초)으로 끊기지 않았다 — 이미 열린 연결의 제한은 설정 변경으로 바뀌지 않는다"
        );
        // 두 연결은 거의 같은 때 열렸다. 뒤 연결도 1초를 받았다면 앞 연결이 닫힌 뒤 2초 안에 닫힌다.
        assert!(
            !closed_within(&mut second, Duration::from_secs(2)),
            "설정 변경 뒤에 연 연결이 바뀐 설정(60초)을 받지 않았다"
        );
    }

    #[test]
    fn matching_proto_passes_without_pushing_a_rejection() {
        let (ctx, _rx) = ctx();
        let client_id = ctx.hub.alloc_id();
        let sink = ctx.hub.register(client_id);
        let hs = StreamHandshake {
            proto: stream::STREAM_PROTO,
            attach_target: None,
            attach_workspace: Some(7),
        };

        assert!(TcpIpcServer::validate_stream_proto(
            &ctx, client_id, &hs, None
        ));
        assert!(
            sink.try_recv().is_err(),
            "정상 proto 에는 거절 ack 가 나가면 안 된다"
        );
    }

    #[test]
    fn mismatched_proto_is_rejected_with_an_error_ack() {
        let (ctx, _rx) = ctx();
        let client_id = ctx.hub.alloc_id();
        let sink = ctx.hub.register(client_id);
        let hs = StreamHandshake {
            proto: stream::STREAM_PROTO + 41,
            attach_target: None,
            attach_workspace: Some(7),
        };

        assert!(!TcpIpcServer::validate_stream_proto(
            &ctx, client_id, &hs, None
        ));

        let frame = sink.try_recv().expect("거절 ack 가 sink 에 실려야 한다");
        assert_eq!(frame.tag, StreamTag::Control);
        let ack: StreamAck = serde_json::from_slice(&frame.payload).expect("ack 역직렬화");
        assert!(!ack.ok, "거절은 ok:false 로 표현된다: {ack:?}");
        assert_eq!(
            ack.proto,
            stream::STREAM_PROTO,
            "client 가 서버 버전을 알 수 있어야 한다"
        );
        let err = ack.error.expect("거절 사유가 실려야 한다");
        assert!(
            err.contains(&(stream::STREAM_PROTO + 41).to_string()),
            "사유에 client 가 보낸 값이 있어야 한다: {err}"
        );
    }

    #[test]
    fn missing_proto_field_defaults_to_zero_and_is_rejected() {
        let params = serde_json::json!({ "target_workspace": 7 });
        let parsed: stream::StreamOpenParams = serde_json::from_value(params).expect("parse");
        assert_eq!(parsed.proto, 0, "생략된 proto 는 0 이다");

        let (ctx, _rx) = ctx();
        let client_id = ctx.hub.alloc_id();
        let _sink = ctx.hub.register(client_id);
        let hs = StreamHandshake {
            proto: parsed.proto,
            attach_target: None,
            attach_workspace: parsed.target_workspace,
        };
        assert!(!TcpIpcServer::validate_stream_proto(
            &ctx, client_id, &hs, None
        ));
    }
}

#[cfg(test)]
mod read_end_tests {
    use super::*;

    fn end(detach_sent: bool) -> StreamReadEnd {
        StreamReadEnd {
            client_id: 42,
            peer: Some(std::net::SocketAddr::from(([127, 0, 0, 1], 5555))),
            read_timeout: Duration::from_secs(45),
            detach_sent: Arc::new(AtomicBool::new(detach_sent)),
        }
    }

    #[test]
    fn a_read_timeout_is_warned_with_the_client_peer_and_armed_seconds() {
        let (warn, message) =
            end(false).read_error_report(&std::io::Error::from(std::io::ErrorKind::WouldBlock));
        assert!(warn);
        for part in [
            "stream client 42",
            "127.0.0.1:5555",
            "45초",
            "HeartbeatTimeout",
        ] {
            assert!(message.contains(part), "{part} in {message}");
        }
    }

    #[test]
    fn an_eof_after_the_server_sent_detach_is_not_a_warning() {
        let eof = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        let (warn, message) = end(true).read_error_report(&eof);
        assert!(!warn);
        assert!(message.contains("PeerClosed"), "{message}");
        assert!(end(false).read_error_report(&eof).0);
    }
}
