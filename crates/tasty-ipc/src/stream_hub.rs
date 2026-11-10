//! 연결마다 유한 push 큐를 둔다. 큐가 가득 차면 프레임을 버리고 연속 유실이 한도를 넘으면 끊는다.
//! 소켓 I/O는 호스트 adapter가 맡고 이 허브는 채널·수신 메시지 분류만 담당한다.
//! 별도 인증 토큰은 없으며 SSH와 loopback 연결을 신뢰 경계로 사용한다.

mod bytes;
mod html;
mod wake;
pub use bytes::{FrameReservation, HeldFrame, PublicationFence};
pub use html::HtmlControl;
pub use wake::{PUMP_MESSAGE_BUDGET, StreamWakeGate};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, Weak};

use crate::server::IpcWaker;
use crate::stream::StreamFrame;

/// Bounded capacity of each per-client push sink. A client whose sink fills up
/// has frames dropped — the main loop never blocks on a slow consumer.
const SINK_CAP: usize = 1024;

/// Consecutive dropped frames after which a lagging client is force-disconnected.
const LAG_LIMIT: u32 = 64;

/// Identifier of an upgraded streaming connection.
pub type StreamClientId = u32;

/// One message routed from a stream connection to the main loop.
pub enum StreamInbound {
    /// A frame received from a stream client.
    Frame {
        client_id: StreamClientId,
        frame: StreamFrame,
    },
    /// surface attach 요청. 엔진에 접근할 수 있는 메인 루프에서 점유·snapshot·tap을 처리한다.
    AttachRequest {
        client_id: StreamClientId,
        target_surface_id: u32,
    },
    /// workspace attach 요청. 터미널과 비터미널의 표현은 메인 루프가 구성한다.
    AttachWorkspaceRequest {
        client_id: StreamClientId,
        target_workspace_id: u32,
    },
    /// 연결이 끝났다. 메인 루프가 해당 client의 점유를 해제한다.
    Disconnected { client_id: StreamClientId },
}

/// 한 번의 pump_inbound가 분류한 메시지. 실제 엔진 처리는 메인 루프가 수행한다.
#[derive(Default)]
pub struct PumpOutcome {
    /// Clients whose connections closed — release their attach locks.
    pub disconnected: Vec<StreamClientId>,
    /// `(client_id, target_surface_id)` attach requests.
    pub attach_requests: Vec<(StreamClientId, u32)>,
    /// `(client_id, target_workspace_id)` workspace attach requests (step 6).
    pub workspace_attach_requests: Vec<(StreamClientId, u32)>,
    /// Input and receipt boundaries in wire order, including across input kinds.
    pub input_events: Vec<(StreamClientId, InputEvent)>,
    /// 원격 구조 변경 요청. 메인 루프가 holder를 확인하고 도메인 함수를 호출한다.
    /// origin은 이미 ForwardOrigin::of_wire로 해석됐으며 생략은 User다.
    pub structural_ops: Vec<(
        StreamClientId,
        u64,
        crate::stream::StructuralOp,
        crate::stream::ForwardOrigin,
    )>,
    /// 원격 PTY 크기 요청. 메인 루프가 holder를 확인하고 resize한다. 크기 확정은 기존 tap이 통지한다.
    pub resize_requests: Vec<(StreamClientId, u32, usize, usize)>,
    /// mirror 사용자의 attention 해제 요청. holder를 확인한 뒤 원격 상태를 지운다.
    /// 이후 attention 변경 통지가 mirror에 반영된다.
    pub attention_clear_requests: Vec<(StreamClientId, u32)>,
    /// `(client_id, surface_id)` explicit full-texture-resend requests
    /// ([`StreamControl::MeshFullResendRequest`](crate::stream::StreamControl)),
    /// e.g. after a client-side decode error or reconnect.
    pub mesh_full_resend_requests: Vec<(StreamClientId, u32)>,
    /// capture 업로드 제어 메시지. StreamControl과 다른 event 태그를 쓰며 그 파싱 실패 뒤 확인한다.
    pub capture_uploads: Vec<(StreamClientId, CaptureUploadMsg)>,
    /// 원격 디렉터리 조회. 같은 Control 채널에서 capture 메시지 다음으로 파싱한다.
    pub list_dir_requests: Vec<(StreamClientId, ListDirRequestMsg)>,
    /// 원격 Git 조회. 별도의 event 태그를 가진 Control JSON이다.
    pub git_query_requests: Vec<(StreamClientId, GitQueryRequestMsg)>,
    /// 원격 markdown 원문 조회. 별도의 event 태그를 가진 Control JSON이다.
    pub markdown_content_requests: Vec<(StreamClientId, MarkdownContentRequestMsg)>,
    /// File and HTTP commands in one wire order, including credit and cancellation.
    pub html_requests: Vec<(StreamClientId, HtmlControl)>,
    /// 원격 explorer 파일 작업의 요청·답·취소. 별도의 event 태그를 가진 Control JSON이다.
    pub explorer_ops: Vec<(StreamClientId, crate::explorer_op::ExplorerOpMsg)>,
    /// 원격 자식 에이전트의 출처 선언·실행 요청·통지 응답. 별도의 event 태그를 가진 Control JSON이다.
    pub agent_children: Vec<(StreamClientId, crate::agent_child::AgentClientMsg)>,
    /// holder 의 원본 터미널 읽기. 별도의 event 태그를 가진 Control JSON이다.
    pub terminal_reads: Vec<(StreamClientId, crate::terminal_read::TerminalReadMsg)>,
    /// holder 의 원격 파일 하나 가져오기. 별도의 event 태그를 가진 Control JSON이다.
    pub file_downloads: Vec<(StreamClientId, crate::file_download::FileDownloadMsg)>,
    /// bulk begin·chunk·commit을 도착 순서대로 보관한다. 태그별로 나누면 같은 배치의
    /// chunk를 begin보다 먼저 처리할 수 있다. 결속 workspace는 연결의 bulk_workspace에서 찾는다.
    pub bulk_events: Vec<(StreamClientId, BulkEvent)>,
}

/// Main-loop input dispatch order. A checkpoint never covers later input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Data(Vec<u8>),
    Mesh {
        surface_id: u32,
        input: tasty_plugin_protocol::protocol::RawInputWire,
    },
    Checkpoint(u64),
    Context(MeshInputContext),
}
/// Context must precede the MeshInput that uses it, even in the same pump batch.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshInputContext {
    pub surface_id: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub pixels_per_point: f32,
    pub theme: Option<tasty_plugin_protocol::protocol::ThemeWire>,
    pub focused: bool,
}

/// Control의 begin/commit과 Data의 chunk를 한 순서로 처리하기 위한 이벤트.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkEvent {
    /// 전송 시작 — 파일명·총 크기 통지. `total_size` 는 수신측 사전 용량 승인(`begin_bulk_transfer`)의 입력.
    Begin {
        transfer_id: u64,
        filename: String,
        total_size: u64,
    },
    /// 파일 청크 — bulk 연결 Data 프레임에서 뜯은 raw 바이트. `seq` 는 진단용(TCP 는
    /// 연결당 순서 보장이라 재정렬에 쓰지 않는다).
    Chunk {
        transfer_id: u64,
        seq: u32,
        bytes: Vec<u8>,
    },
    /// 전송 완료 — 서버가 저장 확정 후 `BulkResult` 회신.
    Commit { transfer_id: u64 },
}

/// Screenshot→remote-clipboard mid-session control messages. See
/// [`PumpOutcome::capture_uploads`] doc for why this lives outside `StreamControl`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CaptureUploadMsg {
    /// One chunk of base64-encoded file bytes. `seq`/`total` are carried for
    /// diagnostics only — chunks are appended in arrival order (TCP is ordered
    /// per-connection), not reordered by `seq`.
    CaptureChunk {
        upload_id: u64,
        seq: u32,
        total: u32,
        data_b64: String,
    },
    /// Marks the upload complete — the main loop finalizes (write file + set the
    /// local clipboard to its path) and replies with a `capture_result` event.
    CaptureCommit { upload_id: u64, file_name: String },
}

/// attach holder가 원격 디렉터리를 조회하는 제어 메시지.
/// 일반 플러그인 IPC의 FsRead 검사가 아니라 해당 연결의 workspace 점유를 확인한다.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ListDirRequestMsg {
    /// `dir` empty means "use the remote home directory" (server-side convention).
    ListDirRequest { request_id: u64, dir: String },
}

/// attach holder의 원격 Git 조회. client_holds_workspace로 점유를 확인한다.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GitQueryRequestMsg {
    GitQueryRequest {
        request_id: u64,
        /// **원격** surface id — 서버가 자기 실제 PTY(`Terminal::get_cwd`)로 cwd 를
        /// 직접 resolve 한다(mirror 의 OSC 7 재생 의존 없음). `worktree_path` 가
        /// 있으면 이 필드는 무시된다.
        surface_id: u32,
        kind: GitQueryKind,
        /// worktree 전환/새로고침 — 이전 응답이 돌려준 opaque 서버 경로를 그대로
        /// echo. `None` 이면 `surface_id` 의 cwd 로 새로 discover.
        #[serde(default)]
        worktree_path: Option<String>,
        /// `kind = Diff` 전용 — 대상 파일의 repo-relative 경로.
        #[serde(default)]
        diff_path: Option<String>,
    },
}

/// attach holder의 원격 markdown 원문 조회. 디렉터리 조회와 같은 점유 조건을 적용한다.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MarkdownContentRequestMsg {
    MarkdownContentRequest {
        request_id: u64,
        /// **원격** surface id — 서버가 자기 트리에서 그 surface 를 찾아 content
        /// mirror 대상인지 확인하고 그 파일을 읽는다.
        surface_id: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitQueryKind {
    Snapshot,
    Diff,
}

impl GitQueryKind {
    /// 서버 응답과 클라이언트 요청이 공유하는 kind 문자열.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            GitQueryKind::Snapshot => "snapshot",
            GitQueryKind::Diff => "diff",
        }
    }
}

/// Per-client push sink held in the registry.
struct StreamSink {
    html_transfer: bool,
    html_viewer: bool,
    html_http: bool,
    html_https: bool,
    html_http_reload: bool,
    html_admission: Option<(u64, u64)>,
    binding: Arc<()>,
    registration: u128,
    tx: SyncSender<HeldFrame>,
    bytes: Arc<AtomicUsize>,
    /// Consecutive dropped-frame count (reset on a successful send).
    lag: u32,
    /// ClientLossNotify를 받아 손실 통지를 활성화했는지. 선언 전에는 조용히 버린다.
    loss_notify: bool,
    /// 마지막 Loss 이후 유실한 프레임 수. 선언 전에도 세며 통지를 큐에 넣은 뒤에만 초기화한다.
    pending_loss: u64,
    /// loss_notify && pending_loss > 0의 캐시. 대기 중인 통지가 없으면
    /// 수신자가 sink 맵을 잠그지 않도록 두며 sync_owed에서 갱신한다.
    owes_notice: Arc<AtomicBool>,
    /// 큐에 있고 writer가 아직 꺼내지 않은 프레임 수. 넣기 전에 늘리고 실패하면 되돌린다.
    /// 연결별로 세고 살아 있는 연결만 합해 끊긴 연결의 잔량이 통계에 남지 않게 한다.
    queued: Arc<AtomicU64>,
}

/// writer가 큐에서 꺼낼 때 backlog를 줄이고 대기 중인 Loss를 다시 넣는 수신 인터페이스.
pub struct SinkReceiver {
    rx: Receiver<HeldFrame>,
    queued: Arc<AtomicU64>,
    id: StreamClientId,
    owes_notice: Arc<AtomicBool>,
    /// 허브의 sink 맵. `Weak` 인 이유: 수신 끝이 맵을 붙들면 허브가 사라진 뒤에도 맵이 남는다.
    /// 맵에 든 `SyncSender` 가 살아 있으면 이 채널이 안 끊겨 write 스레드가 종료를 못 본다.
    sinks: Weak<Mutex<HashMap<StreamClientId, StreamSink>>>,
}

impl SinkReceiver {
    /// 한 프레임을 꺼낸 직후 backlog를 줄이고 밀린 Loss를 넣는다.
    /// 유실 전 프레임 뒤에 통지가 놓이며 다음 push나 inbound를 기다릴 필요가 없다.
    fn took(&self, frame: HeldFrame) -> HeldFrame {
        self.queued.fetch_sub(1, Ordering::Relaxed);
        if self.owes_notice.load(Ordering::Acquire) {
            self.repay_now();
        }
        frame
    }

    fn repay_now(&self) {
        let Some(sinks) = self.sinks.upgrade() else {
            return; // 허브가 사라졌다 — 이 연결도 곧 끝난다.
        };
        let mut sinks =
            tasty_utils::poison::recover_mutex(sinks.lock(), SINKS_WHAT, &SINKS_POISONED);
        if let Some(sink) = sinks.get_mut(&self.id) {
            StreamHub::repay_pending_loss(sink);
        }
    }

    /// [`Receiver::recv_timeout`] 과 같다.
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<StreamFrame, mpsc::RecvTimeoutError> {
        self.rx
            .recv_timeout(timeout)
            .map(|f| self.took(f).into_frame())
    }

    /// [`Receiver::try_recv`] 와 같다.
    pub fn try_recv(&self) -> Result<StreamFrame, mpsc::TryRecvError> {
        self.rx.try_recv().map(|f| self.took(f).into_frame())
    }

    /// [`Receiver::recv`] 와 같다.
    pub fn recv(&self) -> Result<StreamFrame, mpsc::RecvError> {
        self.rx.recv().map(|f| self.took(f).into_frame())
    }
}

/// Outcome of a [`StreamHub::push`] attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum PushResult {
    /// Frame queued for the client's write thread.
    Sent,
    /// No such client (already disconnected).
    Unknown,
    /// Sink full — frame dropped, client still connected.
    Dropped,
    /// Client exceeded the lag limit and was disconnected.
    Disconnected,
}

/// 누적 유실 프레임 수와 지연으로 끊은 연결 수.
/// 전송 성공 때 초기화하는 연속 유실 수 lag와 구분한다.
#[derive(Debug, Default)]
struct StreamLoss {
    frames: AtomicU64,
    lagged_out: AtomicU64,
}

/// 누적 유실 두 값과 현재 backlog. backlog만 큐 소비·연결 해제에 따라 줄어든다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamLossSnapshot {
    /// 연결이 살아 있는데 sink 가 차서 버린 프레임 수 — **조용한 손실**.
    pub frames_dropped: u64,
    /// lag 한도를 넘겨 끊은 연결 수. 그 마지막 프레임도 `frames_dropped` 에 든다.
    pub clients_lagged_out: u64,
    /// 지금 살아 있는 연결들의 sink 에 쌓여 있고 아직 write 스레드가 안 가져간 프레임 수의
    /// 합. 한 연결의 몫은 [`SINK_CAPACITY`] 를 넘지 못한다.
    pub backlog: u64,
}

/// 진단 응답에 제공하는 연결 하나의 큐 용량.
pub const SINK_CAPACITY: usize = SINK_CAP;

/// Context handed to each accepted connection so it can register a stream sink,
/// forward inbound frames, and wake the main loop. Cloneable + `Send`.
#[derive(Clone)]
pub struct StreamContext {
    pub hub: StreamHub,
    pub inbound_tx: Sender<StreamInbound>,
    pub waker: IpcWaker,
    /// 새 연결이 소켓에 걸 무응답 제한. 설정 반영 경로가 갱신한다.
    pub heartbeat_timeout: crate::stream::SharedHeartbeatTimeout,
}

/// Shared registry of stream-client push sinks. Cloneable (internal `Arc`): the
/// IPC accept threads register/unregister, the main loop pushes.
#[derive(Clone)]
pub struct StreamHub {
    sinks: Arc<Mutex<HashMap<StreamClientId, StreamSink>>>,
    next_id: Arc<AtomicU32>,
    /// bulk 연결과 workspace의 매핑. Data를 파일 청크로 분류하고
    /// begin/commit 때 해당 workspace에 holder가 있는지 확인하는 데 쓴다.
    bulk_bindings: Arc<Mutex<HashMap<StreamClientId, u32>>>,
    /// 누적 손실. 클론이 공유한다 — 허브는 accept 스레드마다 클론되므로 `Arc` 밖에
    /// 두면 스레드마다 다른 수를 센다.
    loss: Arc<StreamLoss>,
}

impl Default for StreamHub {
    fn default() -> Self {
        Self::new()
    }
}

/// 메모리 맵의 poison을 각각 한 번 보고하고 복구한다. 조회 실패를 미등록 연결로 취급하지 않는다.
static SINKS_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BULK_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const SINKS_WHAT: &str = "stream hub sink map";
const BULK_WHAT: &str = "stream hub bulk binding map";

impl StreamHub {
    pub fn new() -> Self {
        Self {
            sinks: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU32::new(1)),
            bulk_bindings: Arc::new(Mutex::new(HashMap::new())),
            loss: Arc::new(StreamLoss::default()),
        }
    }

    /// Allocate a fresh client id (monotonic, mirrors `IdGenerator`).
    pub fn alloc_id(&self) -> StreamClientId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a client's push sink. Returns the receiving end the connection's
    /// write thread drains to the socket.
    pub fn register(&self, id: StreamClientId) -> SinkReceiver {
        let (tx, rx) = mpsc::sync_channel(SINK_CAP);
        let queued = Arc::new(AtomicU64::new(0));
        let owes_notice = Arc::new(AtomicBool::new(false));
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED).insert(
            id,
            StreamSink {
                html_transfer: false,
                html_viewer: false,
                html_http: false,
                html_https: false,
                html_http_reload: false,
                html_admission: None,
                binding: Arc::new(()),
                registration: rand::random(),
                tx,
                bytes: Arc::new(AtomicUsize::new(0)),
                lag: 0,
                loss_notify: false,
                pending_loss: 0,
                owes_notice: owes_notice.clone(),
                queued: queued.clone(),
            },
        );
        SinkReceiver {
            rx,
            queued,
            id,
            owes_notice,
            sinks: Arc::downgrade(&self.sinks),
        }
    }

    /// A reply lease identifies this registration, including when a numeric ID is registered again.
    pub fn client_identity(&self, id: StreamClientId) -> Option<(u128, Weak<()>)> {
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
            .get(&id)
            .map(|sink| (sink.registration, Arc::downgrade(&sink.binding)))
    }
    pub fn client_binding(&self, id: StreamClientId) -> Option<Weak<()>> {
        self.client_identity(id).map(|(_, binding)| binding)
    }
    pub fn matches_client_binding(&self, id: StreamClientId, binding: &Weak<()>) -> bool {
        self.client_binding(id)
            .is_some_and(|current| current.ptr_eq(binding))
    }

    /// Both peers must explicitly support transport. This never enables an HTML role.
    pub fn html_transfer_enabled(&self, id: StreamClientId) -> bool {
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
            .get(&id)
            .is_some_and(|sink| sink.html_transfer)
    }
    pub fn html_viewer_enabled(&self, id: StreamClientId) -> bool {
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
            .get(&id)
            .is_some_and(|sink| sink.html_transfer && sink.html_viewer)
    }
    fn set_html_capabilities(&self, id: StreamClientId, capabilities: &[String]) {
        if let Some(sink) =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
                .get_mut(&id)
        {
            sink.html_transfer = capabilities
                .iter()
                .any(|c| c == crate::html_transfer::CAPABILITY);
            sink.html_http = capabilities
                .iter()
                .any(|c| c == crate::html_http::CAPABILITY);
            sink.html_http_reload = capabilities
                .iter()
                .any(|c| c == crate::html_http::RELOAD_CAPABILITY);
            sink.html_https = capabilities
                .iter()
                .any(|c| c == crate::html_http::HTTPS_CAPABILITY);
            sink.html_viewer = capabilities
                .iter()
                .any(|c| c == crate::html_transfer::VIEWER_CAPABILITY);
        }
    }

    /// ClientLossNotify 선언을 기록한다. 반복 호출은 같으며 이미 끊긴 연결은 무시한다.
    pub fn enable_loss_notify(&self, id: StreamClientId) {
        if let Some(sink) =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
                .get_mut(&id)
        {
            sink.loss_notify = true;
            Self::sync_owed(sink);
        }
    }

    /// Drop a client's sink (its write thread then exits when the sender drops).
    /// Idempotent. bulk 결속(있으면)도 함께 청소한다.
    pub fn unregister(&self, id: StreamClientId) {
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
            .remove(&id);
        tasty_utils::poison::recover_mutex(self.bulk_bindings.lock(), BULK_WHAT, &BULK_POISONED)
            .remove(&id);
    }

    /// Remove only the original registration, including its bulk binding.
    pub fn unregister_bound(&self, id: StreamClientId, binding: &Weak<()>) -> bool {
        let mut sinks =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED);
        if !sinks
            .get(&id)
            .is_some_and(|sink| Arc::downgrade(&sink.binding).ptr_eq(binding))
        {
            return false;
        }
        sinks.remove(&id);
        tasty_utils::poison::recover_mutex(self.bulk_bindings.lock(), BULK_WHAT, &BULK_POISONED)
            .remove(&id);
        true
    }

    /// bulk 전송 전용 연결(docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그)로 태깅한다. 핸드셰이크의 `bulk_workspace` 를
    /// 결속 workspace 로 기록하며, 이 등록은 [`register`](Self::register)와 read 루프
    /// 시작 사이(같은 accept 스레드)에서 이뤄지므로 이후 pump 되는 모든 프레임에서
    /// [`bulk_workspace`](Self::bulk_workspace)로 조회된다.
    pub fn register_bulk(&self, id: StreamClientId, workspace_id: u32) {
        tasty_utils::poison::recover_mutex(self.bulk_bindings.lock(), BULK_WHAT, &BULK_POISONED)
            .insert(id, workspace_id);
    }

    /// 이 연결이 bulk 전용이면 결속 workspace_id, 아니면 `None`. pump_inbound 의
    /// Data 분류(파일 청크 vs PTY 입력)와 begin/commit 인가에서 참조한다.
    pub fn bulk_workspace(&self, id: StreamClientId) -> Option<u32> {
        tasty_utils::poison::recover_mutex(self.bulk_bindings.lock(), BULK_WHAT, &BULK_POISONED)
            .get(&id)
            .copied()
    }

    /// Push a frame to one client. Non-blocking: a full sink drops the frame and,
    /// past [`LAG_LIMIT`] consecutive drops, disconnects the client.
    pub fn push(&self, id: StreamClientId, frame: StreamFrame) -> PushResult {
        self.push_to(id, None, frame, None)
    }

    /// Registration comparison and enqueue share the same sink lock.
    pub fn push_bound(
        &self,
        id: StreamClientId,
        binding: &Weak<()>,
        frame: StreamFrame,
    ) -> PushResult {
        self.push_to(id, Some(binding), frame, None)
    }

    fn push_to(
        &self,
        id: StreamClientId,
        binding: Option<&Weak<()>>,
        frame: StreamFrame,
        reservation: Option<FrameReservation>,
    ) -> PushResult {
        let mut sinks =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED);
        let Some(sink) = sinks.get_mut(&id) else {
            return PushResult::Unknown;
        };
        if binding.is_some_and(|binding| !Arc::downgrade(&sink.binding).ptr_eq(binding)) {
            return PushResult::Unknown;
        }
        let held = match reservation {
            Some(reservation) => match reservation.attach(id, sink, frame) {
                Ok(held) => held,
                Err(_) => return PushResult::Unknown,
            },
            None => match HeldFrame::charge(frame, sink) {
                Ok(held) => held,
                Err(_) => return PushResult::Dropped,
            },
        };
        // 큐가 가득 차 생긴 손실이므로 Loss도 바로 넣지 못할 수 있다.
        // pending_loss를 보존하고 수신자가 자리를 비울 때 또는 다음 push 전에 다시 넣는다.
        // 통지가 큐에 들어가기 전에는 누계를 초기화하지 않는다.
        Self::repay_pending_loss(sink);
        match Self::enqueue_held(sink, held) {
            Ok(()) => {
                sink.lag = 0;
                PushResult::Sent
            }
            Err(TrySendError::Full(_)) => {
                // 연결을 끊는 경우에도 마지막 프레임은 유실됐으므로 분기 전에 센다.
                self.loss.frames.fetch_add(1, Ordering::Relaxed);
                sink.pending_loss += 1;
                Self::sync_owed(sink);
                sink.lag += 1;
                if sink.lag >= LAG_LIMIT {
                    sinks.remove(&id); // sender dropped → write thread exits
                    self.loss.lagged_out.fetch_add(1, Ordering::Relaxed);
                    PushResult::Disconnected
                } else {
                    PushResult::Dropped
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                sinks.remove(&id);
                PushResult::Unknown
            }
        }
    }

    /// 수신자의 dequeue 직후, 새 push 직전, inbound 처리 끝에 밀린 Loss를 넣는다.
    /// 선언 전이거나 큐가 가득 찼으면 누계를 유지한다. 통지를 우선 넣어 바로 뒤의
    /// 데이터가 다시 버려져도 다음 Loss로 알린다. 통지 성공은 소비자의 진척이 아니므로 lag를 초기화하지 않는다.
    fn repay_pending_loss(sink: &mut StreamSink) {
        if !Self::owes(sink) {
            return;
        }
        let notice = crate::stream::StreamControl::Loss {
            frames: sink.pending_loss,
        };
        let Ok(payload) = serde_json::to_vec(&notice) else {
            return; // 빚을 유지한다 — 다음 기회에 다시 만든다.
        };
        if Self::try_enqueue(
            sink,
            StreamFrame::new(crate::stream::StreamTag::Control, payload),
        )
        .is_ok()
        {
            sink.pending_loss = 0;
            Self::sync_owed(sink);
        }
    }

    /// 손실 통지를 선언했고 아직 알리지 않은 유실이 있는지 확인한다.
    fn owes(sink: &StreamSink) -> bool {
        sink.loss_notify && sink.pending_loss != 0
    }

    /// [`StreamSink::owes_notice`] 를 참값에 맞춘다. `loss_notify` · `pending_loss` 를 바꾸는
    /// 자리마다 부른다.
    fn sync_owed(sink: &StreamSink) {
        sink.owes_notice.store(Self::owes(sink), Ordering::Release);
    }

    /// sink 에 프레임 하나를 넣고 [`StreamSink::queued`] 를 맞춘다.
    ///
    /// **넣기 전에** 올리는 이유: 성공 뒤에 올리면 write 스레드가 그 사이에 꺼내 먼저
    /// 내릴 수 있고, 그러면 카운터가 0 아래로 내려갔다가 돌아온다(`u64` 라 wrap 한다).
    fn try_enqueue(sink: &StreamSink, frame: StreamFrame) -> Result<(), TrySendError<HeldFrame>> {
        let held = HeldFrame::charge(frame, sink).map_err(TrySendError::Full)?;
        Self::enqueue_held(sink, held)
    }
    fn enqueue_held(sink: &StreamSink, frame: HeldFrame) -> Result<(), TrySendError<HeldFrame>> {
        sink.queued.fetch_add(1, Ordering::Relaxed);
        let sent = sink.tx.try_send(frame);
        if sent.is_err() {
            sink.queued.fetch_sub(1, Ordering::Relaxed);
        }
        sent
    }

    /// 선언 전에 유실됐고 이미 큐를 비운 연결도 이번 inbound 처리 뒤 Loss를 받을 수 있게 한다.
    fn repay_all_pending_loss(&self) {
        let mut sinks =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED);
        for sink in sinks.values_mut() {
            Self::repay_pending_loss(sink);
        }
    }

    /// 누적 유실과 살아 있는 연결들의 현재 backlog를 읽는다. push 결과를 무시한 호출의 손실도 포함한다.
    pub fn loss(&self) -> StreamLossSnapshot {
        let backlog =
            tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED)
                .values()
                .map(|s| s.queued.load(Ordering::Relaxed))
                .sum();
        StreamLossSnapshot {
            frames_dropped: self.loss.frames.load(Ordering::Relaxed),
            clients_lagged_out: self.loss.lagged_out.load(Ordering::Relaxed),
            backlog,
        }
    }

    /// Called by the GUI/headless main loop at the ordered input boundary, never by
    /// the socket reader or heartbeat writer. A full sink follows normal loss handling.
    pub fn acknowledge_input(&self, client: StreamClientId, sequence: u64) {
        let payload =
            serde_json::to_vec(&crate::stream::StreamControl::InputProcessed { sequence })
                .expect("input receipt contains only an integer");
        // A full/closed sink already owns disconnect handling; never replay the receipt.
        let _ = self.push(
            client,
            StreamFrame::new(crate::stream::StreamTag::Control, payload),
        );
    }

    /// Number of currently connected stream clients.
    pub fn client_count(&self) -> usize {
        tasty_utils::poison::recover_mutex(self.sinks.lock(), SINKS_WHAT, &SINKS_POISONED).len()
    }

    /// 수신 메시지를 종류별로 분류한다. 엔진 접근과 점유·입력 처리 자체는 메인 루프가 맡는다.
    /// 일반 Data는 input_events, bulk 연결의 Data는 bulk_events로 반환한다.
    pub fn pump_inbound(&self, inbound_rx: &Receiver<StreamInbound>) -> PumpOutcome {
        self.pump_inbound_budgeted(inbound_rx, usize::MAX).0
    }

    /// 메시지를 최대 `budget`개까지만 꺼낸다. 남은 메시지는 채널에 그대로 둔다.
    /// 두 번째 값은 예산에 닿았는지다. true이면 채널에 메시지가 남았을 수 있다.
    pub fn pump_inbound_budgeted(
        &self,
        inbound_rx: &Receiver<StreamInbound>,
        budget: usize,
    ) -> (PumpOutcome, bool) {
        let mut out = PumpOutcome::default();
        let mut taken = 0usize;
        while taken < budget
            && let Ok(msg) = inbound_rx.try_recv()
        {
            taken += 1;
            match msg {
                StreamInbound::Disconnected { client_id } => out.disconnected.push(client_id),
                StreamInbound::AttachRequest {
                    client_id,
                    target_surface_id,
                } => out.attach_requests.push((client_id, target_surface_id)),
                StreamInbound::AttachWorkspaceRequest {
                    client_id,
                    target_workspace_id,
                } => out
                    .workspace_attach_requests
                    .push((client_id, target_workspace_id)),
                StreamInbound::Frame { client_id, frame } => {
                    // 같은 Data 태그라도 bulk 연결이면 파일 청크로 분류한다.
                    let bulk_ws = self.bulk_workspace(client_id);
                    match frame.tag {
                        crate::stream::StreamTag::Data if bulk_ws.is_some() => {
                            match crate::stream::decode_bulk_chunk(&frame.payload) {
                                Some((transfer_id, seq, data)) => {
                                    out.bulk_events.push((
                                        client_id,
                                        BulkEvent::Chunk {
                                            transfer_id,
                                            seq,
                                            bytes: data.to_vec(),
                                        },
                                    ));
                                }
                                None => tracing::warn!(
                                    "bulk transfer: truncated chunk frame from client {client_id} (< sub-header) — dropping"
                                ),
                            }
                        }
                        crate::stream::StreamTag::Data => {
                            out.input_events
                                .push((client_id, InputEvent::Data(frame.payload)));
                        }
                        crate::stream::StreamTag::Control => {
                            // client→server 제어 메시지만 분류한다. 다른 형태는 별도 event parser로 확인한다.
                            match serde_json::from_slice(&frame.payload) {
                                Ok(crate::stream::StreamControl::StructuralOp {
                                    op_id,
                                    op,
                                    origin,
                                }) => out.structural_ops.push((
                                    client_id,
                                    op_id,
                                    op,
                                    crate::stream::ForwardOrigin::of_wire(origin),
                                )),
                                Ok(crate::stream::StreamControl::ClientResize {
                                    surface_id,
                                    cols,
                                    rows,
                                }) => {
                                    out.resize_requests
                                        .push((client_id, surface_id, cols, rows));
                                }
                                Ok(crate::stream::StreamControl::ClientAttentionClear {
                                    surface_id,
                                }) => {
                                    out.attention_clear_requests.push((client_id, surface_id));
                                }
                                Ok(crate::stream::StreamControl::BulkBegin {
                                    transfer_id,
                                    filename,
                                    total_size,
                                }) => out.bulk_events.push((
                                    client_id,
                                    BulkEvent::Begin {
                                        transfer_id,
                                        filename,
                                        total_size,
                                    },
                                )),
                                Ok(crate::stream::StreamControl::BulkCommit { transfer_id }) => out
                                    .bulk_events
                                    .push((client_id, BulkEvent::Commit { transfer_id })),
                                Ok(crate::stream::StreamControl::MeshContext {
                                    surface_id,
                                    width_px,
                                    height_px,
                                    pixels_per_point,
                                    theme,
                                    focused,
                                }) => {
                                    out.input_events.push((
                                        client_id,
                                        InputEvent::Context(MeshInputContext {
                                            surface_id,
                                            width_px,
                                            height_px,
                                            pixels_per_point,
                                            theme,
                                            focused,
                                        }),
                                    ));
                                }
                                Ok(crate::stream::StreamControl::MeshFullResendRequest {
                                    surface_id,
                                }) => {
                                    out.mesh_full_resend_requests.push((client_id, surface_id));
                                }
                                Ok(crate::stream::StreamControl::MeshInput {
                                    surface_id,
                                    input,
                                }) => {
                                    out.input_events
                                        .push((client_id, InputEvent::Mesh { surface_id, input }));
                                }
                                Ok(crate::stream::StreamControl::InputCheckpoint { sequence }) => {
                                    out.input_events
                                        .push((client_id, InputEvent::Checkpoint(sequence)));
                                }
                                // 연결별 선언이므로 허브에서 바로 기록한다.
                                Ok(crate::stream::StreamControl::ClientLossNotify {}) => {
                                    self.enable_loss_notify(client_id);
                                }
                                Ok(_) => {}
                                Err(_) => {
                                    self.collect_other_control(client_id, &frame.payload, &mut out)
                                }
                            }
                        }
                        // Ping/Detach에는 엔진에 전달할 입력 payload가 없다.
                        _ => {}
                    }
                }
            }
        }
        // 이 배치의 선언(`ClientLossNotify`)까지 반영한 뒤에 갚는다.
        self.repay_all_pending_loss();
        (out, taken >= budget)
    }

    /// `StreamControl` 이 아닌 제어 메시지를 형태별로 분류한다. 맞는 형태가 없으면 버린다.
    fn collect_other_control(
        &self,
        client_id: StreamClientId,
        payload: &[u8],
        out: &mut PumpOutcome,
    ) {
        if self.collect_html_request(client_id, payload, out) {
            // The HTML helper owns capability declarations and request order.
        } else if let Ok(msg) = serde_json::from_slice::<CaptureUploadMsg>(payload) {
            out.capture_uploads.push((client_id, msg));
        } else if let Ok(msg) = serde_json::from_slice::<ListDirRequestMsg>(payload) {
            out.list_dir_requests.push((client_id, msg));
        } else if let Ok(msg) = serde_json::from_slice::<GitQueryRequestMsg>(payload) {
            out.git_query_requests.push((client_id, msg));
        } else if let Ok(msg) = serde_json::from_slice::<MarkdownContentRequestMsg>(payload) {
            out.markdown_content_requests.push((client_id, msg));
        } else if let Ok(msg) = serde_json::from_slice::<crate::explorer_op::ExplorerOpMsg>(payload)
        {
            out.explorer_ops.push((client_id, msg));
        } else if let Ok(msg) =
            serde_json::from_slice::<crate::agent_child::AgentClientMsg>(payload)
        {
            out.agent_children.push((client_id, msg));
        } else if let Ok(msg) =
            serde_json::from_slice::<crate::terminal_read::TerminalReadMsg>(payload)
        {
            out.terminal_reads.push((client_id, msg));
        } else if let Ok(msg) =
            serde_json::from_slice::<crate::file_download::FileDownloadMsg>(payload)
        {
            out.file_downloads.push((client_id, msg));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::StreamTag;

    fn frame(tag: StreamTag, p: &[u8]) -> StreamFrame {
        StreamFrame::new(tag, p.to_vec())
    }

    #[test]
    fn html_capabilities_are_opt_in_per_registration_and_keep_credit_order() {
        use crate::html_transfer::{CAPABILITY, HtmlRequest, TransferId};
        let hub = StreamHub::new();
        let _sink = hub.register(5);
        let (tx, rx) = mpsc::channel();
        let id = TransferId {
            epoch: 1,
            surface_id: 1,
            binding: 1,
            document: 1,
            request: 1,
            transfer: 1,
        };
        let send = |msg| {
            tx.send(StreamInbound::Frame {
                client_id: 5,
                frame: frame(StreamTag::Control, &serde_json::to_vec(&msg).unwrap()),
            })
            .unwrap()
        };
        send(HtmlRequest::HtmlEntryRequest { id });
        assert!(hub.pump_inbound(&rx).html_requests.is_empty());
        send(HtmlRequest::HtmlCapabilities {
            capabilities: vec![CAPABILITY.into()],
        });
        send(HtmlRequest::HtmlEntryRequest { id });
        send(HtmlRequest::HtmlRead {
            id,
            offset: 0,
            allow: false,
        });
        send(HtmlRequest::HtmlCancel { id });
        let out = hub.pump_inbound(&rx);
        assert_eq!(out.html_requests.len(), 4);
        assert!(matches!(
            out.html_requests[0].1,
            HtmlControl::File(HtmlRequest::HtmlCapabilities { .. })
        ));
        assert!(matches!(
            out.html_requests[1].1,
            HtmlControl::File(HtmlRequest::HtmlEntryRequest { .. })
        ));
        assert!(matches!(
            out.html_requests[2].1,
            HtmlControl::File(HtmlRequest::HtmlRead { .. })
        ));
        assert!(matches!(
            out.html_requests[3].1,
            HtmlControl::File(HtmlRequest::HtmlCancel { .. })
        ));
        let _replacement = hub.register(5);
        assert!(!hub.html_transfer_enabled(5));
    }

    #[test]
    fn alloc_id_is_monotonic() {
        let hub = StreamHub::new();
        assert_eq!(hub.alloc_id(), 1);
        assert_eq!(hub.alloc_id(), 2);
        assert_eq!(hub.alloc_id(), 3);
    }

    #[test]
    fn html_viewer_requires_both_capabilities_and_a_live_registration() {
        use crate::html_transfer::{CAPABILITY, HtmlRequest, VIEWER_CAPABILITY};
        let hub = StreamHub::new();
        let _sink = hub.register(51);
        let (tx, rx) = mpsc::channel();
        let declare = |capabilities: Vec<String>| {
            tx.send(StreamInbound::Frame {
                client_id: 51,
                frame: frame(
                    StreamTag::Control,
                    &serde_json::to_vec(&HtmlRequest::HtmlCapabilities { capabilities }).unwrap(),
                ),
            })
            .unwrap();
            hub.pump_inbound(&rx)
        };
        assert!(!hub.html_viewer_enabled(51));
        declare(vec![VIEWER_CAPABILITY.into()]);
        assert!(!hub.html_viewer_enabled(51));
        declare(vec![CAPABILITY.into(), VIEWER_CAPABILITY.into()]);
        assert!(hub.html_viewer_enabled(51));
        let _replacement = hub.register(51);
        assert!(!hub.html_viewer_enabled(51));
        assert!(!hub.html_transfer_enabled(51));
    }

    #[test]
    fn push_unknown_client() {
        let hub = StreamHub::new();
        assert_eq!(
            hub.push(42, frame(StreamTag::Data, b"x")),
            PushResult::Unknown
        );
    }

    #[test]
    fn register_push_receive() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        assert_eq!(hub.client_count(), 1);
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"hi")),
            PushResult::Sent
        );
        let got = rx.recv().unwrap();
        assert_eq!(got.tag, StreamTag::Data);
        assert_eq!(got.payload, b"hi");
        hub.unregister(id);
        assert_eq!(hub.client_count(), 0);
    }

    #[test]
    fn slow_client_drops_then_disconnects() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let _rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        let mut saw_disconnect = false;
        for _ in 0..LAG_LIMIT {
            match hub.push(id, frame(StreamTag::Data, b"x")) {
                PushResult::Dropped => {}
                PushResult::Disconnected => {
                    saw_disconnect = true;
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(saw_disconnect);
        assert_eq!(hub.client_count(), 0);
    }

    #[test]
    fn a_single_drop_survives_the_success_that_follows_it() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        assert_eq!(
            hub.loss(),
            StreamLossSnapshot::default(),
            "시작이 0 이 아니다"
        );

        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        assert_eq!(hub.loss().frames_dropped, 0, "성공만 했는데 손실을 셌다");

        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"lost")),
            PushResult::Dropped
        );
        assert_eq!(hub.loss().frames_dropped, 1);
        assert_eq!(hub.loss().clients_lagged_out, 0, "끊지도 않았는데 셌다");

        rx.recv().expect("한 장은 이미 큐에 있다");
        assert_eq!(hub.push(id, frame(StreamTag::Data, b"y")), PushResult::Sent);

        assert_eq!(
            hub.loss().frames_dropped,
            1,
            "성공이 누적 손실을 지웠다 — lag 을 그대로 노출한 것과 같다"
        );
    }

    #[test]
    fn the_frame_that_triggers_the_disconnect_is_counted_as_lost_too() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let _rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        let mut refused = 0;
        loop {
            match hub.push(id, frame(StreamTag::Data, b"x")) {
                PushResult::Dropped => refused += 1,
                PushResult::Disconnected => {
                    refused += 1;
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        let loss = hub.loss();
        assert_eq!(loss.frames_dropped, refused, "끊은 갈래의 한 장을 안 셌다");
        assert_eq!(loss.clients_lagged_out, 1);
    }

    #[test]
    fn a_client_that_never_declares_sees_no_extra_frame() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        for _ in 0..3 {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"gap")),
                PushResult::Dropped
            );
        }
        rx.recv().expect("한 장 비운다");
        rx.recv().expect("두 장 비운다");
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"post")),
            PushResult::Sent
        );

        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            drained.iter().all(|f| f.tag == StreamTag::Data),
            "선언하지 않았는데 Control 프레임이 섞였다"
        );
        assert_eq!(drained.len(), SINK_CAP - 1, "큐 길이가 달라졌다");
    }

    #[test]
    fn the_notice_lands_between_the_last_survivor_and_the_first_frame_after_the_gap() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        hub.enable_loss_notify(id);

        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        for _ in 0..3 {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"gap")),
                PushResult::Dropped
            );
        }
        rx.recv().expect("한 장 비운다");
        rx.recv().expect("두 장 비운다");
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"post")),
            PushResult::Sent
        );

        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let notice_at = drained
            .iter()
            .position(|f| f.tag == StreamTag::Control)
            .expect("통지가 큐에 없다");
        assert_eq!(
            notice_at,
            drained.len() - 2,
            "통지가 공백 지점이 아니라 다른 자리에 있다"
        );
        assert_eq!(drained[notice_at + 1].payload, b"post".to_vec());
        assert!(
            drained[..notice_at].iter().all(|f| f.payload == b"pre"),
            "통지 앞에 공백 뒤 프레임이 섞였다"
        );
        let parsed: crate::stream::StreamControl =
            serde_json::from_slice(&drained[notice_at].payload).expect("통지 파싱");
        assert_eq!(
            parsed,
            crate::stream::StreamControl::Loss { frames: 3 },
            "잃은 수가 안 맞는다"
        );
    }

    #[test]
    fn a_notice_that_cannot_be_sent_is_kept_as_debt_and_paid_in_full_later() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        hub.enable_loss_notify(id);

        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        for _ in 0..2 {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"gap")),
                PushResult::Dropped
            );
        }
        rx.recv().expect("한 장 비운다");
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );

        rx.recv().expect("소비자가 계속 읽는다");
        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let notice = drained
            .iter()
            .find(|f| f.tag == StreamTag::Control)
            .expect("통지가 결국에도 안 왔다");
        let parsed: crate::stream::StreamControl =
            serde_json::from_slice(&notice.payload).expect("통지 파싱");
        assert_eq!(
            parsed,
            crate::stream::StreamControl::Loss { frames: 2 },
            "통지가 태워지기 전에 빚이 지워졌다"
        );
    }

    #[test]
    fn sending_the_notice_does_not_count_as_the_consumer_keeping_up() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        hub.enable_loss_notify(id);

        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        for _ in 0..(LAG_LIMIT - 1) {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"x")),
                PushResult::Dropped
            );
        }
        rx.recv().expect("한 칸 비운다 — 통지가 그 칸을 쓴다");
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"x")),
            PushResult::Disconnected,
            "통지 성공이 lag 을 되돌렸다 — 느린 소비자가 한도를 피한다"
        );
    }

    #[test]
    fn pump_inbound_records_the_loss_notify_declaration() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        let (tx, inbound_rx) = mpsc::channel();
        let payload =
            serde_json::to_vec(&crate::stream::StreamControl::ClientLossNotify {}).unwrap();
        tx.send(StreamInbound::Frame {
            client_id: id,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert!(
            out.input_events.is_empty() && out.structural_ops.is_empty(),
            "선언이 다른 갈래로 샜다"
        );

        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );
        rx.recv().expect("한 장 비운다");
        rx.recv().expect("두 장 비운다");
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"post")),
            PushResult::Sent
        );
        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            drained.iter().any(|f| f.tag == StreamTag::Control),
            "pump 로 들어온 선언이 안 기록됐다"
        );
    }

    #[test]
    fn a_clone_of_the_hub_counts_into_the_same_total() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let _rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        let other = hub.clone();
        assert_eq!(
            other.push(id, frame(StreamTag::Data, b"lost")),
            PushResult::Dropped
        );
        assert_eq!(hub.loss().frames_dropped, 1, "클론이 자기 수를 따로 셌다");
    }

    #[test]
    fn backlog_rises_with_queued_frames_and_falls_as_the_writer_takes_them() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..3 {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        assert_eq!(hub.loss().backlog, 3);
        rx.recv().expect("한 장");
        rx.try_recv().expect("두 장");
        assert_eq!(hub.loss().backlog, 1, "꺼낸 만큼 안 내려갔다");
        rx.recv_timeout(std::time::Duration::from_millis(10))
            .expect("세 장");
        let loss = hub.loss();
        assert_eq!(loss.backlog, 0);
        assert_eq!(loss.frames_dropped, 0, "backlog 이 손실 누계로 샜다");
    }

    #[test]
    fn a_refused_frame_does_not_count_as_backlog() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let _rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(hub.push(id, frame(StreamTag::Data, b"x")), PushResult::Sent);
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"lost")),
            PushResult::Dropped
        );
        assert_eq!(hub.loss().backlog, SINK_CAPACITY as u64);
        assert_eq!(hub.loss().frames_dropped, 1);
    }

    #[test]
    fn a_disconnected_client_leaves_nothing_in_the_backlog() {
        let hub = StreamHub::new();
        let gone = hub.alloc_id();
        let stays = hub.alloc_id();
        let _gone_rx = hub.register(gone);
        let _stays_rx = hub.register(stays);
        for _ in 0..5 {
            assert_eq!(
                hub.push(gone, frame(StreamTag::Data, b"x")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(stays, frame(StreamTag::Data, b"y")),
            PushResult::Sent
        );
        assert_eq!(hub.loss().backlog, 6);
        hub.unregister(gone);
        assert_eq!(hub.loss().backlog, 1, "끊긴 연결의 큐가 합에 남았다");
    }

    #[test]
    fn a_notice_follows_the_last_survivor_with_nothing_pushed_and_nothing_sent_after_the_loss() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        hub.enable_loss_notify(id);
        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );
        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            drained.len(),
            SINK_CAP + 1,
            "공백 앞을 다 읽었는데 통지가 안 왔다"
        );
        assert!(
            drained[..SINK_CAP].iter().all(|f| f.payload == b"pre"),
            "통지가 공백 앞 프레임 사이에 끼었다"
        );
        let last = &drained[SINK_CAP];
        assert_eq!(last.tag, StreamTag::Control);
        let notice: crate::stream::StreamControl =
            serde_json::from_slice(&last.payload).expect("StreamControl");
        assert_eq!(notice, crate::stream::StreamControl::Loss { frames: 1 });
        assert_eq!(hub.loss().backlog, 0, "꺼낸 통지가 backlog 에 남았다");
    }

    #[test]
    fn a_declaration_that_arrives_after_the_loss_is_answered_in_the_same_inbound_batch() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );
        let before: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(before.len(), SINK_CAP);
        assert!(
            before.iter().all(|f| f.tag == StreamTag::Data),
            "선언 전인데 통지가 나갔다"
        );

        let (tx, inbound_rx) = mpsc::channel();
        let declare =
            serde_json::to_vec(&crate::stream::StreamControl::ClientLossNotify {}).unwrap();
        tx.send(StreamInbound::Frame {
            client_id: id,
            frame: frame(StreamTag::Control, &declare),
        })
        .unwrap();
        hub.pump_inbound(&inbound_rx);

        let after = rx.try_recv().expect("선언한 배치에서 통지가 나가야 한다");
        assert_eq!(after.tag, StreamTag::Control);
        let notice: crate::stream::StreamControl =
            serde_json::from_slice(&after.payload).expect("StreamControl");
        assert!(
            matches!(notice, crate::stream::StreamControl::Loss { frames: 1 }),
            "선언 전에 센 공백을 말해야 한다: {notice:?}"
        );
        tx.send(StreamInbound::Frame {
            client_id: id,
            frame: frame(StreamTag::Ping, b""),
        })
        .unwrap();
        hub.pump_inbound(&inbound_rx);
        assert!(rx.try_recv().is_err(), "갚은 통지가 되풀이됐다");
    }

    #[test]
    fn a_declaration_between_the_loss_and_the_drain_is_repaid_by_the_write_thread() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );
        hub.enable_loss_notify(id);
        let drained: Vec<StreamFrame> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            drained.len(),
            SINK_CAP + 1,
            "선언 뒤 공백 앞을 다 읽었는데 통지가 안 왔다"
        );
        let last = &drained[SINK_CAP];
        assert_eq!(last.tag, StreamTag::Control);
        let notice: crate::stream::StreamControl =
            serde_json::from_slice(&last.payload).expect("StreamControl");
        assert_eq!(notice, crate::stream::StreamControl::Loss { frames: 1 });
    }

    #[test]
    fn pump_inbound_adds_nothing_for_a_client_that_never_declared() {
        let hub = StreamHub::new();
        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..SINK_CAP {
            assert_eq!(
                hub.push(id, frame(StreamTag::Data, b"pre")),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, frame(StreamTag::Data, b"gap")),
            PushResult::Dropped
        );
        let drained = std::iter::from_fn(|| rx.try_recv().ok()).count();
        assert_eq!(drained, SINK_CAP);
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::Frame {
            client_id: id,
            frame: frame(StreamTag::Ping, b""),
        })
        .unwrap();
        hub.pump_inbound(&inbound_rx);
        assert!(rx.try_recv().is_err(), "선언 안 한 연결에 통지를 넣었다");
    }

    #[test]
    fn input_checkpoints_keep_their_exact_wire_prefix_across_batches() {
        let hub = StreamHub::new();
        let (tx, rx) = mpsc::channel();
        let send = |tag, payload| {
            tx.send(StreamInbound::Frame {
                client_id: 7,
                frame: StreamFrame::new(tag, payload),
            })
            .unwrap()
        };
        send(StreamTag::Data, b"A".to_vec());
        send(
            StreamTag::Control,
            serde_json::to_vec(&crate::stream::StreamControl::InputCheckpoint { sequence: 1 })
                .unwrap(),
        );
        send(StreamTag::Data, b"B".to_vec());
        assert_eq!(
            hub.pump_inbound(&rx).input_events,
            vec![
                (7, InputEvent::Data(b"A".to_vec())),
                (7, InputEvent::Checkpoint(1)),
                (7, InputEvent::Data(b"B".to_vec()))
            ]
        );
        send(
            StreamTag::Control,
            serde_json::to_vec(&crate::stream::StreamControl::InputCheckpoint { sequence: 2 })
                .unwrap(),
        );
        assert_eq!(
            hub.pump_inbound(&rx).input_events,
            vec![(7, InputEvent::Checkpoint(2))]
        );
    }

    #[test]
    fn input_checkpoint_preserves_mesh_and_other_surface_order() {
        let hub = StreamHub::new();
        let (tx, rx) = mpsc::channel();
        let input = tasty_plugin_protocol::protocol::RawInputWire {
            time: None,
            focused: true,
            modifiers: Default::default(),
            events: vec![],
        };
        let enqueue = |event| {
            tx.send(StreamInbound::Frame {
                client_id: 7,
                frame: StreamFrame::new(StreamTag::Control, serde_json::to_vec(&event).unwrap()),
            })
            .unwrap()
        };
        enqueue(crate::stream::StreamControl::MeshInput {
            surface_id: 10,
            input: input.clone(),
        });
        enqueue(crate::stream::StreamControl::InputCheckpoint { sequence: 1 });
        enqueue(crate::stream::StreamControl::MeshInput {
            surface_id: 20,
            input: input.clone(),
        });
        assert_eq!(
            hub.pump_inbound(&rx).input_events,
            vec![
                (
                    7,
                    InputEvent::Mesh {
                        surface_id: 10,
                        input: input.clone()
                    }
                ),
                (7, InputEvent::Checkpoint(1)),
                (
                    7,
                    InputEvent::Mesh {
                        surface_id: 20,
                        input
                    }
                ),
            ]
        );
        enqueue(crate::stream::StreamControl::InputCheckpoint { sequence: 2 });
        assert_eq!(
            hub.pump_inbound(&rx).input_events,
            vec![(7, InputEvent::Checkpoint(2))]
        );
    }

    #[test]
    fn pump_inbound_classifies_data_as_input() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::Frame {
            client_id: 5,
            frame: frame(StreamTag::Data, b"echo me"),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert!(out.disconnected.is_empty());
        assert!(out.attach_requests.is_empty());
        assert_eq!(
            out.input_events,
            vec![(5u32, InputEvent::Data(b"echo me".to_vec()))]
        );
    }

    #[test]
    fn pump_inbound_classifies_attach_requests() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::AttachRequest {
            client_id: 3,
            target_surface_id: 42,
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.attach_requests, vec![(3u32, 42u32)]);
    }

    #[test]
    fn pump_inbound_classifies_workspace_attach_requests() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::AttachWorkspaceRequest {
            client_id: 4,
            target_workspace_id: 8,
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.workspace_attach_requests, vec![(4u32, 8u32)]);
        assert!(out.attach_requests.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_structural_op() {
        use crate::stream::{SplitAxis, StreamControl, StructuralOp};
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let op = StructuralOp::SplitSurface {
            surface_id: 12,
            direction: SplitAxis::Vertical,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let payload = serde_json::to_vec(&StreamControl::StructuralOp {
            op_id: 3,
            op: op.clone(),
            origin: Some(crate::stream::ForwardOrigin::Agent),
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 8,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(
            out.structural_ops,
            vec![(8u32, 3u64, op, crate::stream::ForwardOrigin::Agent)]
        );
        assert!(out.input_events.is_empty());
    }

    #[test]
    fn pump_inbound_ignores_unknown_control() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let payload = serde_json::to_vec(&crate::stream::StreamControl::Resize {
            surface_id: 1,
            cols: 80,
            rows: 24,
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 8,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert!(out.structural_ops.is_empty());
        assert!(out.resize_requests.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_client_resize() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let payload = serde_json::to_vec(&crate::stream::StreamControl::ClientResize {
            surface_id: 12,
            cols: 203,
            rows: 57,
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 8,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.resize_requests, vec![(8u32, 12u32, 203usize, 57usize)]);
        assert!(out.structural_ops.is_empty());
        assert!(out.input_events.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_mesh_context() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let payload = serde_json::to_vec(&crate::stream::StreamControl::MeshContext {
            surface_id: 7,
            width_px: 800,
            height_px: 600,
            pixels_per_point: 2.0,
            theme: None,
            focused: true,
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 9,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(
            out.input_events,
            vec![(
                9,
                InputEvent::Context(MeshInputContext {
                    surface_id: 7,
                    width_px: 800,
                    height_px: 600,
                    pixels_per_point: 2.0,
                    theme: None,
                    focused: true
                })
            )]
        );
        assert!(out.resize_requests.is_empty());
        assert!(out.structural_ops.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_mesh_full_resend_request() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let payload = serde_json::to_vec(&crate::stream::StreamControl::MeshFullResendRequest {
            surface_id: 7,
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 9,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.mesh_full_resend_requests, vec![(9u32, 7u32)]);
        assert!(
            !out.input_events
                .iter()
                .any(|(_, event)| matches!(event, InputEvent::Context(_)))
        );
    }

    #[test]
    fn pump_inbound_classifies_mesh_input() {
        use tasty_plugin_protocol::protocol::{ModifiersWire, RawInputEventWire, RawInputWire};
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let input = RawInputWire {
            time: None,
            focused: true,
            modifiers: ModifiersWire {
                shift: true,
                ..Default::default()
            },
            events: vec![RawInputEventWire::PointerMoved { x: 3.0, y: 4.0 }],
        };
        let payload = serde_json::to_vec(&crate::stream::StreamControl::MeshInput {
            surface_id: 7,
            input: input.clone(),
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 9,
            frame: frame(StreamTag::Control, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(
            out.input_events,
            vec![(
                9u32,
                InputEvent::Mesh {
                    surface_id: 7,
                    input
                }
            )]
        );
        assert!(
            !out.input_events
                .iter()
                .any(|(_, event)| matches!(event, InputEvent::Context(_)))
        );
        assert!(out.mesh_full_resend_requests.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_capture_chunk_and_commit() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let chunk = serde_json::json!({
            "event": "capture_chunk",
            "upload_id": 42,
            "seq": 0,
            "total": 1,
            "data_b64": "aGVsbG8=",
        });
        let commit = serde_json::json!({
            "event": "capture_commit",
            "upload_id": 42,
            "file_name": "screenshot-1.png",
        });
        tx.send(StreamInbound::Frame {
            client_id: 5,
            frame: frame(StreamTag::Control, chunk.to_string().as_bytes()),
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 5,
            frame: frame(StreamTag::Control, commit.to_string().as_bytes()),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.capture_uploads.len(), 2);
        match &out.capture_uploads[0] {
            (5, CaptureUploadMsg::CaptureChunk { upload_id, .. }) => assert_eq!(*upload_id, 42),
            other => panic!("expected CaptureChunk, got {other:?}"),
        }
        match &out.capture_uploads[1] {
            (
                5,
                CaptureUploadMsg::CaptureCommit {
                    upload_id,
                    file_name,
                },
            ) => {
                assert_eq!(*upload_id, 42);
                assert_eq!(file_name, "screenshot-1.png");
            }
            other => panic!("expected CaptureCommit, got {other:?}"),
        }
        assert!(out.structural_ops.is_empty());
        assert!(out.resize_requests.is_empty());
        assert!(out.input_events.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_explorer_op_messages() {
        use crate::explorer_op::ExplorerOpMsg;
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        for msg in [
            serde_json::json!({"event": "explorer_op_request", "op_id": 1, "op": {"kind": "trash", "paths": []}}),
            serde_json::json!({"event": "explorer_op_cancel", "op_id": 1}),
        ] {
            tx.send(StreamInbound::Frame {
                client_id: 4,
                frame: frame(StreamTag::Control, msg.to_string().as_bytes()),
            })
            .unwrap();
        }
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.explorer_ops.len(), 2);
        assert!(matches!(
            out.explorer_ops[0],
            (4, ExplorerOpMsg::ExplorerOpRequest { op_id: 1, .. })
        ));
        assert_eq!(
            out.explorer_ops[1],
            (4, ExplorerOpMsg::ExplorerOpCancel { op_id: 1 })
        );
        assert!(out.list_dir_requests.is_empty());
        assert!(out.markdown_content_requests.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_list_dir_request() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let req = serde_json::json!({
            "event": "list_dir_request",
            "request_id": 7,
            "dir": "/tmp",
        });
        tx.send(StreamInbound::Frame {
            client_id: 3,
            frame: frame(StreamTag::Control, req.to_string().as_bytes()),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.list_dir_requests.len(), 1);
        match &out.list_dir_requests[0] {
            (3, ListDirRequestMsg::ListDirRequest { request_id, dir }) => {
                assert_eq!(*request_id, 7);
                assert_eq!(dir, "/tmp");
            }
            other => panic!("expected ListDirRequest from client 3, got {other:?}"),
        }
        assert!(out.capture_uploads.is_empty());
        assert!(out.structural_ops.is_empty());
        assert!(out.resize_requests.is_empty());
        assert!(out.input_events.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_terminal_read_request() {
        use crate::terminal_read::{ReadOp, TerminalReadMsg};
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let req = TerminalReadMsg::TerminalReadRequest {
            request_id: 4,
            surface_id: 42,
            op: ReadOp::Position,
        };
        tx.send(StreamInbound::Frame {
            client_id: 3,
            frame: frame(StreamTag::Control, &serde_json::to_vec(&req).unwrap()),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.terminal_reads, vec![(3, req)]);
        assert!(out.git_query_requests.is_empty());
        assert!(out.agent_children.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_git_query_request() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        let req = serde_json::json!({
            "event": "git_query_request",
            "request_id": 9,
            "surface_id": 42,
            "kind": "snapshot",
        });
        tx.send(StreamInbound::Frame {
            client_id: 3,
            frame: frame(StreamTag::Control, req.to_string().as_bytes()),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.git_query_requests.len(), 1);
        match &out.git_query_requests[0] {
            (
                3,
                GitQueryRequestMsg::GitQueryRequest {
                    request_id,
                    surface_id,
                    kind,
                    worktree_path,
                    diff_path,
                },
            ) => {
                assert_eq!(*request_id, 9);
                assert_eq!(*surface_id, 42);
                assert_eq!(*kind, GitQueryKind::Snapshot);
                assert_eq!(*worktree_path, None);
                assert_eq!(*diff_path, None);
            }
            other => panic!("expected GitQueryRequest from client 3, got {other:?}"),
        }
        assert!(out.list_dir_requests.is_empty());
        assert!(out.capture_uploads.is_empty());
    }

    #[test]
    fn bulk_binding_register_and_unregister() {
        let hub = StreamHub::new();
        assert_eq!(hub.bulk_workspace(5), None);
        hub.register_bulk(5, 42);
        assert_eq!(hub.bulk_workspace(5), Some(42));
        assert_eq!(hub.bulk_workspace(6), None);
        hub.unregister(5);
        assert_eq!(hub.bulk_workspace(5), None);
    }

    #[test]
    fn pump_inbound_bulk_connection_data_is_chunk_not_input() {
        let hub = StreamHub::new();
        hub.register_bulk(7, 3); // client 7 = bulk 연결(ws 3 결속)
        let (tx, inbound_rx) = mpsc::channel();
        let payload = crate::stream::encode_bulk_chunk(99, 2, b"filebytes");
        tx.send(StreamInbound::Frame {
            client_id: 7,
            frame: frame(StreamTag::Data, &payload),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(
            out.bulk_events,
            vec![(
                7u32,
                BulkEvent::Chunk {
                    transfer_id: 99,
                    seq: 2,
                    bytes: b"filebytes".to_vec(),
                }
            )]
        );
        assert!(out.input_events.is_empty());
    }

    #[test]
    fn pump_inbound_non_bulk_data_still_input() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::Frame {
            client_id: 8,
            frame: frame(StreamTag::Data, b"keystrokes"),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(
            out.input_events,
            vec![(8u32, InputEvent::Data(b"keystrokes".to_vec()))]
        );
        assert!(out.bulk_events.is_empty());
    }

    #[test]
    fn pump_inbound_classifies_bulk_begin_and_commit() {
        use crate::stream::StreamControl;
        let hub = StreamHub::new();
        hub.register_bulk(9, 1);
        let (tx, inbound_rx) = mpsc::channel();
        let begin = serde_json::to_vec(&StreamControl::BulkBegin {
            transfer_id: 100,
            filename: "img.png".to_string(),
            total_size: 2048,
        })
        .unwrap();
        let commit = serde_json::to_vec(&StreamControl::BulkCommit { transfer_id: 100 }).unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 9,
            frame: frame(StreamTag::Control, &begin),
        })
        .unwrap();
        tx.send(StreamInbound::Frame {
            client_id: 9,
            frame: frame(StreamTag::Control, &commit),
        })
        .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.bulk_events.len(), 2);
        assert_eq!(
            out.bulk_events[0],
            (
                9u32,
                BulkEvent::Begin {
                    transfer_id: 100,
                    filename: "img.png".to_string(),
                    total_size: 2048,
                }
            )
        );
        assert_eq!(
            out.bulk_events[1],
            (9u32, BulkEvent::Commit { transfer_id: 100 })
        );
        assert!(out.structural_ops.is_empty());
        assert!(out.capture_uploads.is_empty());
    }

    #[test]
    fn pump_inbound_preserves_bulk_begin_chunk_commit_order() {
        use crate::stream::{StreamControl, encode_bulk_chunk};
        let hub = StreamHub::new();
        hub.register_bulk(5, 2);
        let (tx, inbound_rx) = mpsc::channel();
        let begin = serde_json::to_vec(&StreamControl::BulkBegin {
            transfer_id: 7,
            filename: "f.bin".to_string(),
            total_size: 6,
        })
        .unwrap();
        let commit = serde_json::to_vec(&StreamControl::BulkCommit { transfer_id: 7 }).unwrap();
        for f in [
            frame(StreamTag::Control, &begin),
            frame(StreamTag::Data, &encode_bulk_chunk(7, 0, b"abc")),
            frame(StreamTag::Data, &encode_bulk_chunk(7, 1, b"def")),
            frame(StreamTag::Control, &commit),
        ] {
            tx.send(StreamInbound::Frame {
                client_id: 5,
                frame: f,
            })
            .unwrap();
        }
        let out = hub.pump_inbound(&inbound_rx);
        let events: Vec<&BulkEvent> = out.bulk_events.iter().map(|(_, e)| e).collect();
        assert!(matches!(events[0], BulkEvent::Begin { transfer_id: 7, .. }));
        assert!(matches!(
            events[1],
            BulkEvent::Chunk {
                transfer_id: 7,
                seq: 0,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            BulkEvent::Chunk {
                transfer_id: 7,
                seq: 1,
                ..
            }
        ));
        assert!(matches!(events[3], BulkEvent::Commit { transfer_id: 7 }));
    }

    #[test]
    fn pump_inbound_reports_disconnects() {
        let hub = StreamHub::new();
        let (tx, inbound_rx) = mpsc::channel();
        tx.send(StreamInbound::Disconnected { client_id: 7 })
            .unwrap();
        tx.send(StreamInbound::Disconnected { client_id: 9 })
            .unwrap();
        let out = hub.pump_inbound(&inbound_rx);
        assert_eq!(out.disconnected, vec![7, 9]);
    }
}
