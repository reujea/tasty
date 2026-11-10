//! stream.open을 첫 줄로 보내 JSON-RPC 연결을 바이너리 스트림으로 전환한다.
//! 프레임은 [tag:u8][len:u32 BE][payload] 형식이다. 점유·mirror 처리는 상위 계층이 맡는다.

mod read_failure;

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use read_failure::ReadFailure;

/// Method name whose presence as the first JSON-RPC line upgrades a connection
/// to the streaming channel.
pub const STREAM_OPEN_METHOD: &str = "stream.open";

/// handshake에서 동일한 값인지 비교하는 프로토콜 버전. 변경하면 기존 peer 연결이 거절된다.
/// 추가 기능은 capability와 ClientLossNotify 같은 별도 선언으로 협상한다.
pub const STREAM_PROTO: u32 = 1;

/// 서버가 모든 ClientResize에 Resize 또는 ResizeRejected로 응답한다는 capability 이름.
/// system.info에 알리고 workspace attach descriptor의 [`DESCRIPTOR_CAPABILITIES`]에도 싣는다.
/// client는 연결마다 descriptor를 보고 응답을 기다릴지 정한다.
pub const RESIZE_ACK_CAPABILITY: &str = "ipc.stream.resize-ack";
/// [`RESIZE_ACK_CAPABILITY`]의 버전.
pub const RESIZE_ACK_VERSION: u32 = 1;
/// Main-loop receipt of all Data/MeshInput frames preceding an input checkpoint.
/// This does not acknowledge shell consumption, command completion, or rendering.
pub const INPUT_ACK_CAPABILITY: &str = "ipc.stream.input-ack";
/// 서버가 [`StructuralOp::CloseWorkspace`]로 점유한 workspace 전체를 닫고 결과를 회신한다는 capability 이름.
/// 이 기능이 없는 구 서버는 모르는 op를 회신 없이 버리므로 client는 보내지 않고 미지원으로 알린다.
pub const WORKSPACE_CLOSE_CAPABILITY: &str = "ipc.stream.workspace-close";
/// workspace attach descriptor에서 이 연결의 서버 기능 이름 목록을 담는 키.
/// 구 서버는 이 키를 보내지 않으므로 없으면 빈 목록으로 읽는다.
pub const DESCRIPTOR_CAPABILITIES: &str = "stream_capabilities";
/// workspace attach descriptor에서 서버 PTY가 읽는 키 입력 형식을 담는 키.
/// 값은 [`PTY_INPUT_WIN32_CONSOLE`] 또는 [`PTY_INPUT_VT`]다. 구 서버는 이 키를 보내지 않으므로
/// 없으면 VT로 읽는다.
pub const DESCRIPTOR_PTY_INPUT: &str = "pty_input";
/// ConPTY: 수식키가 붙은 줄바꿈 키를 win32-input 시퀀스로 받아야 하는 Windows 콘솔.
pub const PTY_INPUT_WIN32_CONSOLE: &str = "win32-console";
/// Unix PTY: CSI-u와 제어문자를 그대로 응용 프로그램에 전달한다.
pub const PTY_INPUT_VT: &str = "vt";

/// 이 빌드의 PTY 입력 형식. 서버가 descriptor에 싣는다.
pub fn local_pty_input() -> &'static str {
    if cfg!(windows) {
        PTY_INPUT_WIN32_CONSOLE
    } else {
        PTY_INPUT_VT
    }
}

/// descriptor가 Windows 콘솔 입력을 알리는지. 키가 없거나 모르는 값이면 false(VT)다.
pub fn descriptor_wants_win32_console_input(descriptor: &serde_json::Value) -> bool {
    descriptor
        .get(DESCRIPTOR_PTY_INPUT)
        .and_then(|v| v.as_str())
        == Some(PTY_INPUT_WIN32_CONSOLE)
}

/// Frame header length: 1-byte tag + 4-byte big-endian payload length.
pub const FRAME_HEADER_LEN: usize = 5;

/// 프레임 payload 상한(1 MiB). 초과하면 할당 전에 거절하고 연결을 끝낸다.
pub const MAX_FRAME_LEN: u32 = 1 << 20;

/// Idle interval between application-level [`StreamTag::Ping`] heartbeats sent
/// by either peer's write side while no other frame has gone out. Real traffic
/// (Data/Control) counts as liveness too — a peer only falls back to sending a
/// bare Ping once this long has passed with nothing else to send.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Default socket read timeout applied to attach streams. Six heartbeats' worth
/// of slack so transient jitter or a brief network stall doesn't trip a false
/// disconnect — a peer is declared dead only after missing several heartbeats
/// in a row.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);

/// New attach connections read their read timeout from this value once. Clones
/// share it, so a settings change reaches later connections while sockets that
/// are already open keep the timeout they were given.
#[derive(Clone, Debug)]
pub struct SharedHeartbeatTimeout(Arc<AtomicU64>);

impl Default for SharedHeartbeatTimeout {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(HEARTBEAT_TIMEOUT.as_secs())))
    }
}

impl SharedHeartbeatTimeout {
    pub fn get(&self) -> Duration {
        Duration::from_secs(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, timeout: Duration) {
        self.0.store(timeout.as_secs(), Ordering::Relaxed);
    }
}

/// Frame type tag (first byte of every frame).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum StreamTag {
    /// PTY 출력 또는 echo 등의 바이트 payload.
    Data = 0,
    /// handshake와 세션 제어용 UTF-8 JSON.
    Control = 1,
    /// Application-level keepalive. Sent with an empty payload by either peer's
    /// write side when idle for [`HEARTBEAT_INTERVAL`]; receiving *any* frame
    /// (including this one) resets the reader's [`HEARTBEAT_TIMEOUT`] socket
    /// read timeout, so no explicit handling is needed on receipt.
    Ping = 2,
    /// Graceful close signal (empty payload), either direction.
    Detach = 3,
    /// Opaque plugin egui-mesh frame bytes, chunked (`crate::mesh_stream`). Kept
    /// distinct from [`StreamTag::Data`] (which carries PTY bytes, optionally
    /// surface-muxed) so a mesh consumer never has to disambiguate mesh chunks
    /// from terminal output at the demux layer — attach mesh mirror
    /// (`docs/dev-guide/attach-behavior.md` "mesh mirror 채널").
    MeshData = 4,
}

impl StreamTag {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Data),
            1 => Some(Self::Control),
            2 => Some(Self::Ping),
            3 => Some(Self::Detach),
            4 => Some(Self::MeshData),
            _ => None,
        }
    }
}

/// One streaming frame (tag + payload). Shared by the server push sink and the
/// client transport.
#[derive(Clone, Debug)]
pub struct StreamFrame {
    pub tag: StreamTag,
    pub payload: Vec<u8>,
}

impl StreamFrame {
    pub fn new(tag: StreamTag, payload: Vec<u8>) -> Self {
        Self { tag, payload }
    }
}

/// `params` of the `stream.open` handshake request.
#[derive(Debug, Serialize, Deserialize)]
pub struct StreamOpenParams {
    #[serde(default)]
    pub proto: u32,
    /// attach할 surface ID. 지정하면 handshake 뒤 점유와 초기 snapshot 전송을 요청한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    /// attach할 workspace ID. 터미널별 Data에는 surface ID prefix가 붙는다.
    /// target과 함께 지정하면 서버가 거절한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace: Option<u32>,
    /// bulk 파일 연결을 묶을 workspace ID. 대화형 holder가 되지 않으며 Data는 파일 청크다.
    /// 서버는 해당 workspace에 활성 holder가 있어야 전송을 허용한다. 다른 target과 상호 배타다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bulk_workspace: Option<u32>,
}

/// workspace Data에 4바이트 big-endian surface ID를 붙인다. 단일 surface 연결에는 쓰지 않는다.
pub fn encode_mux(surface_id: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&surface_id.to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

/// [`encode_mux`] 의 역연산. 4바이트 미만이면 `None`(잘린 프레임).
pub fn decode_mux(buf: &[u8]) -> Option<(u32, &[u8])> {
    if buf.len() < 4 {
        return None;
    }
    let sid = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    Some((sid, &buf[4..]))
}

/// bulk 파일 전송(docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그)의 `Data` 프레임 sub-header 길이 = `[transfer_id: u64 BE][seq: u32 BE]`.
pub const BULK_CHUNK_HEADER_LEN: usize = 12;

/// bulk 파일 청크 `Data` 프레임 인코딩. 페이로드 앞에 12바이트 binary sub-header
/// (`[transfer_id: u64 BE][seq: u32 BE]`)를 붙여 raw 파일 바이트를 실어 나른다.
/// `encode_mux`(surface 다중화)와 달리 transfer/seq 를 실으며, base64 를 쓰지 않는다.
/// `seq` 는 진단·검증용(TCP 는 연결당 순서 보장이라 재정렬에 쓰지 않는다 — 캡처
/// 업로드와 동일 근거).
pub fn encode_bulk_chunk(transfer_id: u64, seq: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(BULK_CHUNK_HEADER_LEN + bytes.len());
    out.extend_from_slice(&transfer_id.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

/// [`encode_bulk_chunk`] 의 역연산. 12바이트(sub-header) 미만이면 `None`(잘린 프레임).
/// 반환: `(transfer_id, seq, 파일 바이트 슬라이스)`.
pub fn decode_bulk_chunk(buf: &[u8]) -> Option<(u64, u32, &[u8])> {
    if buf.len() < BULK_CHUNK_HEADER_LEN {
        return None;
    }
    let transfer_id = u64::from_be_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ]);
    let seq = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
    Some((transfer_id, seq, &buf[BULK_CHUNK_HEADER_LEN..]))
}

/// Control payload the server sends immediately after a successful upgrade.
#[derive(Debug, Serialize, Deserialize)]
pub struct StreamAck {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<u32>,
    #[serde(default)]
    pub proto: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 세션 중 전달하는 event 태그 기반 Control 메시지. handshake descriptor는 별도 JSON이다.
/// 모르는 variant는 역직렬화에 실패하고 소비자가 무시한다. Loss는 출력의 연속성을 바꾸므로
/// ClientLossNotify를 선언한 클라이언트에만 보낸다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StreamControl {
    /// Client→server: an ordered boundary after previously queued Data/MeshInput.
    InputCheckpoint { sequence: u64 },
    /// Server→client: the main loop dispatched the input preceding this boundary.
    /// Rejected input still follows its existing error path; receipt is not success.
    InputProcessed { sequence: u64 },
    /// 서버→클라이언트: 원격 PTY에 적용된 크기. 클라이언트는 로컬 요청값이 아니라
    /// 이 응답으로 mirror 크기를 바꿔 원격 reflow 결과와 맞춘다.
    Resize {
        /// Remote surface id. The client maps it to its local mirror surface id
        /// (workspace attach) or applies it to its sole mirror (surface attach).
        surface_id: u32,
        cols: usize,
        rows: usize,
    },
    /// 서버→클라이언트: 원격 surface의 busy/idle 상태. mirror의 활동 표시에 사용한다.
    /// 서버는 1Hz 조회에서 바뀐 값을 보낸다. 송신 전에 변화 캐시가 갱신되므로
    /// 버려진 프레임을 다음 tick이 같은 값으로 다시 보내지는 않는다.
    Activity {
        /// Remote surface id, resolved the same way as [`StreamControl::Resize`].
        surface_id: u32,
        busy: bool,
    },
    /// 클라이언트→서버: 로컬 mirror pane에 맞는 PTY 크기를 요청한다.
    /// 서버가 holder를 확인하고 resize한 뒤 Resize로 확정 크기를 보낸다.
    /// [`RESIZE_ACK_CAPABILITY`]를 알리는 서버는 모든 요청에 응답한다. 크기가 이미 같으면 출력과 같은
    /// 순서로 현재 크기의 Resize를, 대상·점유가 맞지 않으면 ResizeRejected를 보낸다.
    /// 그 capability가 없는 구 서버는 크기가 같을 때나 거절할 때 응답하지 않는다.
    ClientResize {
        /// Remote surface id (the client maps its local mirror id to this before
        /// sending). The server resolves the enclosing workspace and verifies the
        /// requesting client is its attach holder before applying.
        surface_id: u32,
        cols: usize,
        rows: usize,
    },
    /// 서버→클라이언트: ClientResize를 적용하지 않았다. 대상이 terminal이 아니거나 없거나,
    /// 요청한 연결이 그 workspace의 holder가 아닐 때다. 크기를 바꾸지 않으므로 출력 순서와 무관하게
    /// 바로 보낸다. 값은 거절한 요청의 것이며 client는 이것으로 기다리던 요청을 찾는다.
    /// 구 client는 이 variant를 모르므로 역직렬화에 실패해 무시한다.
    ResizeRejected {
        /// Remote surface id named by the rejected request.
        surface_id: u32,
        cols: usize,
        rows: usize,
    },
    /// 클라이언트→서버: 사용자가 mirror surface를 확인해 attention을 지웠다는 통지.
    /// 실제 레코드가 제거됐을 때만 보내며 holder를 확인한 서버가 원격 상태도 지운다.
    /// 서버의 Attention(None)을 이미 빈 mirror에 적용해도 제거가 없어 다시 통지하지 않는다.
    /// 별도 응답은 없다.
    ClientAttentionClear {
        /// Remote surface id whose attention record should be dropped. The server
        /// resolves the enclosing workspace and verifies the requesting client is
        /// its attach holder before applying.
        surface_id: u32,
    },
    /// 클라이언트→서버: mirror의 구조 변경을 원격에서 실행하도록 요청한다.
    /// anchor는 원격 surface ID다. 서버가 자신의 트리에서 pane/tab/workspace를 찾고
    /// holder를 확인한 뒤 같은 op_id의 StructuralResult를 보낸다.
    StructuralOp {
        /// Client-assigned monotonic id, echoed back in the result so the client
        /// can correlate the reply (and toast on failure).
        op_id: u64,
        op: StructuralOp,
        /// 사용자 또는 에이전트 요청인지 구분한다. 서버의 복원 기록과 탭 선택에 사용한다.
        /// 이전 client와의 호환을 위해 필드 생략은 User다. 새 client는 항상 보낸다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<ForwardOrigin>,
    },
    /// 서버→클라이언트: 구조 변경 결과. 거절 시 reason을 보내며 해당 작업은 적용하지 않는다.
    /// 예를 들어 원격에 등록되지 않은 surface 종류는 만들 수 없다.
    StructuralResult {
        op_id: u64,
        /// Exact resource created by this operation, when the server knows it.
        /// Old peers omit/ignore this field; never infer an arbitrary ID from a merged snapshot.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        created_surface_id: Option<u32>,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// 성공한 원격 사용자 닫기에서 큐 예산을 넘어 닫은 항목 기록에 내용 없이 남긴 terminal 수.
        /// 요청한 사용자가 경고를 받는다. idempotent replay 회신과 옛 서버는 보내지 않는다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        history_omitted: Option<u32>,
    },
    /// 서버→클라이언트: 구조 변경 후 workspace 전체 트리와 surface descriptor.
    /// handshake와 같은 형식이며, 클라이언트는 살아남은 터미널의 로컬 ID·스크롤백을 유지한다.
    /// 전달된 op의 결과라면 성공 StructuralResult 다음에 보내고, 서버 자체 구조 변화도 이 형식을 쓴다.
    StructuralDelta {
        /// Remote workspace id (the client maps it to its local mirror
        /// workspace; a mirror session hosts exactly one workspace so this is
        /// mainly for validation/diagnostics).
        workspace_id: u32,
        /// Post-op full workspace tree (`to_attach_tree_json`, same shape as the
        /// handshake `tree`).
        tree: serde_json::Value,
        /// Post-op per-surface descriptors (same shape as the handshake
        /// `surfaces`: `{remote_id, role, cols, rows}` for terminals /
        /// `{remote_id, role, kind}` for placeholders).
        surfaces: Vec<serde_json::Value>,
    },
    /// bulk 파일 전송(docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그)의 control-plane 시작 메시지. 전용 bulk 연결에서
    /// 실제 파일 바이트(`Data` 프레임, [`encode_bulk_chunk`])에 앞서 파일명·총 크기를
    /// 알린다. 서버는 `total_size` 를 사전 용량 승인의 입력으로 쓰고, `transfer_id`
    /// 단위로 청크를 누적한다. 저장 dir 결정·경로 회신은 `commit` 에서 확정.
    ///
    /// Direction: **client→server**.
    BulkBegin {
        transfer_id: u64,
        filename: String,
        total_size: u64,
    },
    /// bulk 전송 완료 신호. 서버는 누적 바이트를 파일로 저장 확정하고
    /// [`StreamControl::BulkResult`] 로 원격 절대경로(또는 실패사유)를 회신한다.
    ///
    /// Direction: **client→server**.
    BulkCommit { transfer_id: u64 },
    /// bulk 전송 결과. `ok=true` 면 `path` 에 원격 파일시스템 절대경로, `ok=false` 면
    /// `reason` 에 실패사유(용량 초과·미인가·저장 실패 등). 소비자(mirror 터미널 이미지 붙여넣기 업로드 · 전송 진행/실패 팝업)가 이 경로를
    /// 대화형 스트림에 삽입하거나 진행 UI 에 표시한다.
    ///
    /// Direction: **server→client**.
    BulkResult {
        transfer_id: u64,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// 클라이언트→서버: mirror pane의 크기·배율·테마·포커스.
    /// 첫 메시지가 mesh 구독을 시작하고 이후 변경은 원격 플러그인의 set_context에 반영한다.
    /// 입력 묶음은 별도 MeshInput으로 전달한다.
    MeshContext {
        /// Remote surface id (client-mapped, like every other mirror message).
        surface_id: u32,
        /// Physical pixel width of the client's local mirror pane.
        width_px: u32,
        /// Physical pixel height of the client's local mirror pane.
        height_px: u32,
        /// Logical→physical scale (egui `ScreenDescriptor.pixels_per_point`).
        pixels_per_point: f32,
        /// The client's own resolved theme — the mirror should visually match
        /// what the *attach client* is displaying, not the (possibly headless,
        /// possibly differently-themed) server.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theme: Option<tasty_plugin_protocol::protocol::ThemeWire>,
        /// Whether the local mirror pane currently has keyboard focus.
        #[serde(default)]
        focused: bool,
    },
    /// 클라이언트→서버: RawInputWire 입력 묶음. PointerGone·포커스·modifier도 포함한다.
    /// 별도 응답 없이 원격 repaint의 MeshData로 화면 결과를 받는다.
    MeshInput {
        surface_id: u32,
        input: tasty_plugin_protocol::protocol::RawInputWire,
    },
    /// 클라이언트→서버: frame_seq가 끊겼을 때 전체 텍스처를 다시 요청한다.
    /// 서버는 다음 set_context에 need_full_textures를 켠다. 별도 ack 대신
    /// 해당 surface의 full_textures=true인 MeshData가 도착했는지 확인한다.
    MeshFullResendRequest { surface_id: u32 },
    /// 서버→클라이언트: surface 소유 인스턴스의 attention 상태. None은 해제를 뜻한다.
    /// mirror는 이 값을 AttentionStore에 반영한다. 서버는 바뀐 값만 보내므로
    /// 유실된 프레임이 다음 tick에 자동 재전송되거나 client의 임의 상태 변경이 복구되지는 않는다.
    Attention {
        /// Remote surface id, resolved the same way as [`StreamControl::Resize`].
        surface_id: u32,
        /// The attention kind now recorded on the remote, or `None` if cleared.
        kind: Option<AttentionKindWire>,
    },
    /// 서버→클라이언트: 원격이 확인한 cwd. None이면 mirror의 저장값도 지운다.
    /// 원격 경로이므로 로컬 파일 작업에 사용하지 않는다. 서버의 inherit_cwd와 무관하게
    /// 관측값을 보내며 실행 시 그 설정의 적용 여부는 소비자가 판단한다.
    /// 값이나 holder가 바뀔 때 보내며 유실 후 다음 tick의 재전송은 보장하지 않는다.
    Cwd {
        /// Remote surface id, resolved the same way as [`StreamControl::Resize`].
        surface_id: u32,
        /// The remote path, or `None` if the remote cwd is unknown.
        cwd: Option<String>,
    },
    /// 서버→클라이언트: mesh mirror가 불가능한 surface의 오류.
    /// 대상 부재·지원 kind 아님·플러그인 미실행 등의 reason을 보낸다.
    MeshError { surface_id: u32, reason: String },
    /// 서버→클라이언트: 이 연결의 push 큐에서 프레임이 유실됐다.
    /// ClientLossNotify를 선언한 연결에만 보내며 새 출력보다 먼저 큐에 넣는다.
    /// 유실 뒤에 선언하면 그전의 누적 손실도 알린다. 여러 surface가 같은 연결을 쓰므로 surface ID는 없다.
    /// 복구는 PTY·상태·mesh·bulk 소비자가 각자의 규칙으로 처리한다.
    Loss {
        /// Frames dropped for this connection since the previous `Loss` frame (or
        /// since the connection opened, if this is the first). Counts frames, not
        /// bytes — the sink is a frame queue and the bytes behind a dropped frame
        /// are gone before anyone counts them.
        frames: u64,
    },
    /// 클라이언트→서버: Loss 통지를 받을 수 있다고 선언한다. 반복해도 같고 해제 기능은 없다.
    /// 선언 전에는 손실을 조용히 처리한다. 지원하지 않는 구 서버는 이 variant를 무시한다.
    /// 서버 지원 여부는 system.info의 ipc.stream.loss-notify로 확인한다.
    /// STREAM_PROTO는 동등 비교이므로 이 추가 기능 때문에 버전을 올리지 않는다.
    ClientLossNotify {},
}

/// The concrete structural operation carried by [`StreamControl::StructuralOp`].
/// Every variant is anchored on remote surface id(s); the server resolves the
/// enclosing pane/tab/workspace from its authoritative tree, so the client never
/// needs to track remote pane/tab ids (it only maps surfaces).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StructuralOp {
    /// Split a surface within its tab. Anchor = the surface being split.
    SplitSurface {
        surface_id: u32,
        direction: SplitAxis,
        /// New surface kind (`"terminal"` or a registered plugin kind).
        #[serde(default = "default_terminal_kind")]
        surface_kind: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    /// Split the pane containing the anchor surface.
    SplitPane {
        anchor_surface_id: u32,
        direction: SplitAxis,
        #[serde(default = "default_terminal_kind")]
        surface_kind: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    /// Add a new tab to the pane containing the anchor surface.
    NewTab {
        anchor_surface_id: u32,
        #[serde(default = "default_terminal_kind")]
        surface_kind: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    /// Close a surface (cascading up to tab/pane/workspace as usual).
    CloseSurface { surface_id: u32 },
    /// Close the tab containing the anchor surface.
    CloseTab { anchor_surface_id: u32 },
    /// Close the pane containing the anchor surface.
    ClosePane { anchor_surface_id: u32 },
    /// Reorder a tab within the pane containing the anchor surface.
    MoveTab {
        anchor_surface_id: u32,
        from_index: usize,
        to_index: usize,
    },
    /// Convert a surface to a different kind in place.
    ConvertSurface {
        surface_id: u32,
        #[serde(default = "default_terminal_kind")]
        surface_kind: String,
        #[serde(default)]
        params: serde_json::Value,
        /// 변환 대상이 사용할 원격 cwd. 플랫폼에 의존하지 않는 경로 문자열이다.
        /// client가 모르면 None으로 보내고 서버가 원래 surface에서 찾는다.
        #[serde(default)]
        cwd: Option<String>,
    },
    /// Move a live surface onto another surface's slot. On the wire both ids are
    /// remote ids of the held workspace; the server rejects a target outside it.
    MoveSurface {
        source_surface_id: u32,
        target_surface_id: u32,
    },
    /// anchor의 workspace 복원 스택에서 마지막 항목을 복원한다.
    /// 클라이언트가 종류·내용을 정하지 않으며 서버가 저장된 스크롤백과 PTY를 복원한다.
    RestoreClosedItem { anchor_surface_id: u32 },
    /// Close the whole held workspace that contains the anchor surface on the server,
    /// ending its PTYs. Sent only to a server whose descriptor carries
    /// [`WORKSPACE_CLOSE_CAPABILITY`]; an older server drops unknown ops without a reply.
    CloseWorkspace { anchor_surface_id: u32 },
}

/// 전달된 구조 변경의 호출 주체. 서버의 복원 기록과 사용자 탭 선택에 사용한다.
/// 생략/null은 옛 client와 같은 User다. 모르는 값은 프레임을 거절하지 않고 Agent로 읽어
/// 사용자 상태를 변경하는 권한을 주지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardOrigin {
    /// The client's user, by hand (shortcut, button, context menu).
    User,
    /// An agent driving the client over IPC/CLI — and any origin this build
    /// does not know (see the type's doc).
    Agent,
}

impl<'de> Deserialize<'de> for ForwardOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some("user") => ForwardOrigin::User,
            _ => ForwardOrigin::Agent,
        })
    }
}

impl ForwardOrigin {
    /// The origin a received op stands for. Absence is `User` because that is
    /// what every client before the field meant — see the field's doc on
    /// [`StreamControl::StructuralOp`].
    pub fn of_wire(field: Option<ForwardOrigin>) -> ForwardOrigin {
        field.unwrap_or(ForwardOrigin::User)
    }
}

/// 원격 복원 스택이 비었음을 구분하는 프로토콜 값. 사용자 문구가 아니므로 번역하지 않는다.
pub const STRUCTURAL_REASON_RESTORE_EMPTY: &str = "restore_empty";

/// 원격에 없는 파일로 markdown surface 를 바꾸려 할 때 거절 사유의 앞부분. 뒤에 원격 경로가 붙는다.
/// 로컬 `markdown.navigate` 의 거절 문구와 같고, client 는 이 앞부분으로 "파일 없음" 실패를 구분한다.
pub const STRUCTURAL_REASON_PATH_NOT_FOUND_PREFIX: &str = "path not found: ";

impl StructuralOp {
    /// The remote surface id this op is anchored on. The server uses it to locate
    /// the enclosing workspace and verify the requesting client is its attach
    /// holder before executing (only the occupier may mutate the workspace).
    pub fn anchor_surface_id(&self) -> u32 {
        match self {
            StructuralOp::SplitSurface { surface_id, .. }
            | StructuralOp::CloseSurface { surface_id }
            | StructuralOp::ConvertSurface { surface_id, .. } => *surface_id,
            StructuralOp::SplitPane {
                anchor_surface_id, ..
            }
            | StructuralOp::NewTab {
                anchor_surface_id, ..
            }
            | StructuralOp::CloseTab { anchor_surface_id }
            | StructuralOp::ClosePane { anchor_surface_id }
            | StructuralOp::RestoreClosedItem { anchor_surface_id }
            | StructuralOp::CloseWorkspace { anchor_surface_id }
            | StructuralOp::MoveTab {
                anchor_surface_id, ..
            } => *anchor_surface_id,
            StructuralOp::MoveSurface {
                source_surface_id, ..
            } => *source_surface_id,
        }
    }

    /// The op's wire tag (`kind`) — the same string serde writes. The server
    /// names a forwarded op by it when the op points at something that is no
    /// longer there, the way an IPC rejection names the request's method.
    pub fn wire_kind(&self) -> &'static str {
        match self {
            StructuralOp::SplitSurface { .. } => "split_surface",
            StructuralOp::SplitPane { .. } => "split_pane",
            StructuralOp::NewTab { .. } => "new_tab",
            StructuralOp::CloseSurface { .. } => "close_surface",
            StructuralOp::CloseTab { .. } => "close_tab",
            StructuralOp::ClosePane { .. } => "close_pane",
            StructuralOp::MoveTab { .. } => "move_tab",
            StructuralOp::ConvertSurface { .. } => "convert_surface",
            StructuralOp::MoveSurface { .. } => "move_surface",
            StructuralOp::RestoreClosedItem { .. } => "restore_closed_item",
            StructuralOp::CloseWorkspace { .. } => "close_workspace",
        }
    }

    /// Return a copy with the anchor surface id replaced. The mirror client builds
    /// an op with a **local** anchor id (the only id it knows at the block point),
    /// then swaps in the mapped **remote** id before sending. For `MoveSurface`
    /// the anchor is the source; the target is also a local id and must be
    /// replaced separately with [`with_move_target_surface_id`](Self::with_move_target_surface_id).
    pub fn with_anchor_surface_id(&self, remote: u32) -> StructuralOp {
        let mut cloned = self.clone();
        match &mut cloned {
            StructuralOp::SplitSurface { surface_id, .. }
            | StructuralOp::CloseSurface { surface_id }
            | StructuralOp::ConvertSurface { surface_id, .. } => *surface_id = remote,
            StructuralOp::SplitPane {
                anchor_surface_id, ..
            }
            | StructuralOp::NewTab {
                anchor_surface_id, ..
            }
            | StructuralOp::CloseTab { anchor_surface_id }
            | StructuralOp::ClosePane { anchor_surface_id }
            | StructuralOp::RestoreClosedItem { anchor_surface_id }
            | StructuralOp::CloseWorkspace { anchor_surface_id }
            | StructuralOp::MoveTab {
                anchor_surface_id, ..
            } => *anchor_surface_id = remote,
            StructuralOp::MoveSurface {
                source_surface_id, ..
            } => *source_surface_id = remote,
        }
        cloned
    }

    /// The second surface a `MoveSurface` names (its target). Other ops name only the anchor.
    pub fn move_target_surface_id(&self) -> Option<u32> {
        match self {
            StructuralOp::MoveSurface {
                target_surface_id, ..
            } => Some(*target_surface_id),
            _ => None,
        }
    }

    /// Return a copy with the `MoveSurface` target replaced by the mapped **remote**
    /// id. Other ops are returned unchanged.
    pub fn with_move_target_surface_id(&self, remote: u32) -> StructuralOp {
        let mut cloned = self.clone();
        if let StructuralOp::MoveSurface {
            target_surface_id, ..
        } = &mut cloned
        {
            *target_surface_id = remote;
        }
        cloned
    }
}

/// 호스트 AttentionKind와 변환하는 wire 타입. surface.completion의
/// completion/needs_input 문자열과 같은 이름을 사용한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKindWire {
    /// Work finished — completion IPC/CLI, OSC 133 command completion, toast.
    Completion,
    /// Waiting on the user — Claude plugin `notification`/`pre-tool-use` hooks.
    NeedsInput,
}

/// Split direction carried over the wire. Maps to the host `SplitDirection` /
/// the IPC `"vertical"`/`"horizontal"` convention on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAxis {
    Vertical,
    Horizontal,
}

impl SplitAxis {
    /// The IPC/`SplitDirection` string form (`handle_split` parses this).
    pub fn as_ipc_str(self) -> &'static str {
        match self {
            SplitAxis::Vertical => "vertical",
            SplitAxis::Horizontal => "horizontal",
        }
    }
}

fn default_terminal_kind() -> String {
    "terminal".to_string()
}

/// 헤더와 payload를 한 버퍼에 모아 write_all을 한 번 호출한 뒤 flush한다.
/// 소켓의 TCP_NODELAY와 함께 작은 프레임의 분할 전송 지연을 줄인다.
/// write_all 내부에서 부분 쓰기를 재시도할 수 있어 단일 syscall을 보장하지는 않는다.
pub fn write_frame<W: Write>(w: &mut W, tag: StreamTag, payload: &[u8]) -> io::Result<()> {
    let len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame payload exceeds u32"))?;
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame payload exceeds MAX_FRAME_LEN",
        ));
    }
    let mut buf = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    buf.push(tag as u8);
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf)?;
    w.flush()
}

/// EOF·알 수 없는 tag·초과 길이·읽기 timeout은 오류다.
/// 헤더나 payload를 일부 읽은 뒤 오류가 나면 같은 연결에서 재시도하지 않는다.
/// read_exact가 소비한 바이트를 복원할 수 없어 프레임 경계가 어긋날 수 있다.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<StreamFrame> {
    let mut hdr = [0u8; 5];
    r.read_exact(&mut hdr)?;
    let tag = StreamTag::from_u8(hdr[0])
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unknown stream tag"))?;
    let len = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]);
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds MAX_FRAME_LEN",
        ));
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    Ok(StreamFrame { tag, payload })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_descriptor_without_pty_input_reads_as_vt() {
        // 구 서버는 키를 보내지 않는다. Windows 서버를 VT로 오판하는 쪽만 남고 Unix 서버는 맞는다.
        assert!(!descriptor_wants_win32_console_input(
            &serde_json::json!({})
        ));
        assert!(!descriptor_wants_win32_console_input(
            &serde_json::json!({ DESCRIPTOR_PTY_INPUT: "future" })
        ));
        assert!(!descriptor_wants_win32_console_input(
            &serde_json::json!({ DESCRIPTOR_PTY_INPUT: PTY_INPUT_VT })
        ));
        assert!(descriptor_wants_win32_console_input(
            &serde_json::json!({ DESCRIPTOR_PTY_INPUT: PTY_INPUT_WIN32_CONSOLE })
        ));
    }

    #[test]
    fn frame_roundtrip() {
        for (tag, payload) in [
            (StreamTag::Data, b"hello world".to_vec()),
            (StreamTag::Control, br#"{"ok":true}"#.to_vec()),
            (StreamTag::Detach, Vec::new()),
            (StreamTag::Ping, vec![0u8; 1000]),
        ] {
            let mut buf = Vec::new();
            write_frame(&mut buf, tag, &payload).unwrap();
            let mut cur = Cursor::new(buf);
            let frame = read_frame(&mut cur).unwrap();
            assert_eq!(frame.tag, tag);
            assert_eq!(frame.payload, payload);
        }
    }

    /// 전체 입력을 받는 시험 writer에서 헤더·payload를 나눠 쓰지 않는지 확인한다.
    #[test]
    fn write_frame_emits_one_write_call() {
        struct CountingWriter {
            writes: usize,
            buf: Vec<u8>,
        }
        impl Write for CountingWriter {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                self.buf.extend_from_slice(data);
                Ok(data.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        for payload in [Vec::new(), b"hi".to_vec(), vec![7u8; 4096]] {
            let mut w = CountingWriter {
                writes: 0,
                buf: Vec::new(),
            };
            write_frame(&mut w, StreamTag::Data, &payload).unwrap();
            assert_eq!(
                w.writes,
                1,
                "payload {} 바이트가 {} 번의 write 로 쪼개졌다",
                payload.len(),
                w.writes
            );
            assert_eq!(w.buf.len(), FRAME_HEADER_LEN + payload.len());
        }
    }

    #[test]
    fn heartbeat_timeout_has_jitter_margin_over_interval() {
        // 한 번의 지연으로 끊기지 않도록 heartbeat보다 충분히 긴 timeout을 유지한다.
        assert!(HEARTBEAT_TIMEOUT >= HEARTBEAT_INTERVAL * 2);
        assert_eq!(HEARTBEAT_TIMEOUT, HEARTBEAT_INTERVAL * 6);
    }

    #[test]
    fn ping_frame_has_empty_payload_roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, StreamTag::Ping, &[]).unwrap();
        let mut cur = Cursor::new(buf);
        let frame = read_frame(&mut cur).unwrap();
        assert_eq!(frame.tag, StreamTag::Ping);
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn tag_from_u8() {
        assert_eq!(StreamTag::from_u8(0), Some(StreamTag::Data));
        assert_eq!(StreamTag::from_u8(3), Some(StreamTag::Detach));
        assert_eq!(StreamTag::from_u8(9), None);
    }

    #[test]
    fn read_rejects_unknown_tag() {
        // tag=9 (invalid), len=0
        let bytes = vec![9u8, 0, 0, 0, 0];
        let mut cur = Cursor::new(bytes);
        assert!(read_frame(&mut cur).is_err());
    }

    #[test]
    fn read_rejects_oversize_len() {
        // tag=0, len=MAX_FRAME_LEN+1 — must reject before allocating.
        let len = MAX_FRAME_LEN + 1;
        let mut bytes = vec![0u8];
        bytes.extend_from_slice(&len.to_be_bytes());
        let mut cur = Cursor::new(bytes);
        let err = read_frame(&mut cur).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_eof_is_err() {
        let mut cur = Cursor::new(Vec::new());
        assert!(read_frame(&mut cur).is_err());
    }

    #[test]
    fn write_rejects_oversize_payload() {
        let big = vec![0u8; (MAX_FRAME_LEN + 1) as usize];
        let mut buf = Vec::new();
        assert!(write_frame(&mut buf, StreamTag::Data, &big).is_err());
    }

    #[test]
    fn mux_roundtrip() {
        let enc = encode_mux(42, b"hello");
        assert_eq!(&enc[..4], &42u32.to_be_bytes());
        let (sid, rest) = decode_mux(&enc).unwrap();
        assert_eq!(sid, 42);
        assert_eq!(rest, b"hello");
        let empty = encode_mux(7, b"");
        let (sid2, rest2) = decode_mux(&empty).unwrap();
        assert_eq!(sid2, 7);
        assert!(rest2.is_empty());
    }

    #[test]
    fn decode_mux_rejects_truncated() {
        assert!(decode_mux(&[0u8, 1, 2]).is_none());
        assert!(decode_mux(&[]).is_none());
    }

    #[test]
    fn bulk_chunk_roundtrip() {
        let enc = encode_bulk_chunk(0x0102_0304_0506_0708, 42, b"payload");
        assert_eq!(&enc[..8], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&enc[8..12], &42u32.to_be_bytes());
        let (tid, seq, rest) = decode_bulk_chunk(&enc).unwrap();
        assert_eq!(tid, 0x0102_0304_0506_0708);
        assert_eq!(seq, 42);
        assert_eq!(rest, b"payload");
        let empty = encode_bulk_chunk(7, 0, b"");
        let (tid2, seq2, rest2) = decode_bulk_chunk(&empty).unwrap();
        assert_eq!(tid2, 7);
        assert_eq!(seq2, 0);
        assert!(rest2.is_empty());
    }

    #[test]
    fn decode_bulk_chunk_rejects_truncated() {
        assert!(decode_bulk_chunk(&[]).is_none());
        assert!(decode_bulk_chunk(&[0u8; 11]).is_none());
        assert!(decode_bulk_chunk(&[0u8; 12]).is_some());
    }

    #[test]
    fn stream_control_bulk_begin_commit_result_roundtrip() {
        let begin = StreamControl::BulkBegin {
            transfer_id: 9,
            filename: "img.png".to_string(),
            total_size: 123_456,
        };
        let s = serde_json::to_string(&begin).unwrap();
        assert!(s.contains(r#""event":"bulk_begin""#));
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), begin);

        let commit = StreamControl::BulkCommit { transfer_id: 9 };
        let s = serde_json::to_string(&commit).unwrap();
        assert!(s.contains(r#""event":"bulk_commit""#));
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), commit);

        let ok = StreamControl::BulkResult {
            transfer_id: 9,
            ok: true,
            path: Some("/home/u/.tasty/transfers/img.png".to_string()),
            reason: None,
        };
        let s = serde_json::to_string(&ok).unwrap();
        assert!(s.contains(r#""event":"bulk_result""#));
        assert!(!s.contains("reason")); // skipped when None
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), ok);

        let fail = StreamControl::BulkResult {
            transfer_id: 9,
            ok: false,
            path: None,
            reason: Some("bound workspace has no active holder".to_string()),
        };
        let s = serde_json::to_string(&fail).unwrap();
        assert!(!s.contains("path")); // skipped when None
        assert!(s.contains("no active holder"));
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), fail);
    }

    #[test]
    fn bulk_events_are_distinct_from_capture_and_structural() {
        // A bulk_begin must not be misread as a foreign event and vice-versa —
        // the `event` tag keeps the mid-session control messages disjoint.
        let begin = serde_json::to_string(&StreamControl::BulkBegin {
            transfer_id: 1,
            filename: "x".to_string(),
            total_size: 0,
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&begin).unwrap(),
            StreamControl::BulkBegin { .. }
        ));
        // The screenshot capture-upload events are NOT StreamControl variants — they must
        // still fail to parse as one (bulk added no accidental collision).
        for capture in [
            r#"{"event":"capture_chunk","upload_id":1,"seq":0,"total":1,"data_b64":"AA=="}"#,
            r#"{"event":"capture_commit","upload_id":1,"file_name":"x.png"}"#,
        ] {
            assert!(serde_json::from_str::<StreamControl>(capture).is_err());
        }
    }

    #[test]
    fn stream_control_resize_roundtrip() {
        let msg = StreamControl::Resize {
            surface_id: 7,
            cols: 157,
            rows: 45,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"resize""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_activity_roundtrip() {
        let msg = StreamControl::Activity {
            surface_id: 9,
            busy: true,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"activity""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_cwd_roundtrip_including_cleared() {
        for cwd in [Some("/srv/proj".to_string()), None] {
            let msg = StreamControl::Cwd { surface_id: 9, cwd };
            let s = serde_json::to_string(&msg).unwrap();
            assert!(s.contains(r#""event":"cwd""#));
            let back: StreamControl = serde_json::from_str(&s).unwrap();
            assert_eq!(back, msg);
        }
        let cleared = r#"{"event":"cwd","surface_id":3,"cwd":null}"#;
        assert_eq!(
            serde_json::from_str::<StreamControl>(cleared).unwrap(),
            StreamControl::Cwd {
                surface_id: 3,
                cwd: None
            }
        );
    }

    #[test]
    fn stream_control_client_resize_roundtrip() {
        let msg = StreamControl::ClientResize {
            surface_id: 12,
            cols: 203,
            rows: 57,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"client_resize""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_resize_rejected_roundtrip() {
        let msg = StreamControl::ResizeRejected {
            surface_id: 12,
            cols: 203,
            rows: 57,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"resize_rejected""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    /// 거절 회신을 모르는 구 client의 enum에서는 역직렬화가 실패해 그 client가 무시한다.
    #[test]
    fn resize_rejected_fails_to_decode_in_a_client_without_the_variant() {
        #[derive(Debug, serde::Deserialize)]
        #[serde(tag = "event", rename_all = "snake_case")]
        #[allow(dead_code)] // 이유: 구 client의 enum 모양만 흉내 낸다. 필드는 읽지 않는다.
        enum OlderControl {
            Resize {
                surface_id: u32,
                cols: usize,
                rows: usize,
            },
        }
        let s = serde_json::to_string(&StreamControl::ResizeRejected {
            surface_id: 1,
            cols: 80,
            rows: 24,
        })
        .unwrap();
        assert!(serde_json::from_str::<OlderControl>(&s).is_err());
    }

    #[test]
    fn client_resize_and_resize_are_distinct_events() {
        let client = serde_json::to_string(&StreamControl::ClientResize {
            surface_id: 1,
            cols: 80,
            rows: 24,
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&client).unwrap(),
            StreamControl::ClientResize { .. }
        ));
        let server = serde_json::to_string(&StreamControl::Resize {
            surface_id: 1,
            cols: 80,
            rows: 24,
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&server).unwrap(),
            StreamControl::Resize { .. }
        ));
    }

    #[test]
    fn stream_control_ignores_foreign_events() {
        // Handshake/other events must NOT deserialize as a StreamControl variant —
        // the client relies on this to skip payloads it doesn't handle.
        for foreign in [
            r#"{"event":"attached","surface_id":1,"cols":80,"rows":24}"#,
            r#"{"event":"attach_error","reason":"x"}"#,
            r#"{"event":"force_detached"}"#,
        ] {
            assert!(serde_json::from_str::<StreamControl>(foreign).is_err());
        }
    }

    #[test]
    fn stream_control_structural_op_roundtrip() {
        let msg = StreamControl::StructuralOp {
            op_id: 42,
            op: StructuralOp::SplitSurface {
                surface_id: 7,
                direction: SplitAxis::Horizontal,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            origin: Some(ForwardOrigin::User),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"structural_op""#));
        assert!(s.contains(r#""kind":"split_surface""#));
        assert!(s.contains(r#""direction":"horizontal""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_structural_op_all_kinds_roundtrip() {
        let ops = [
            StructuralOp::SplitPane {
                anchor_surface_id: 3,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({"a": 1}),
            },
            StructuralOp::NewTab {
                anchor_surface_id: 4,
                surface_kind: "markdown".to_string(),
                params: serde_json::json!({"file": "/x"}),
            },
            StructuralOp::CloseSurface { surface_id: 5 },
            StructuralOp::CloseTab {
                anchor_surface_id: 6,
            },
            StructuralOp::ClosePane {
                anchor_surface_id: 7,
            },
            StructuralOp::MoveTab {
                anchor_surface_id: 8,
                from_index: 0,
                to_index: 2,
            },
            StructuralOp::ConvertSurface {
                surface_id: 9,
                surface_kind: "image".to_string(),
                params: serde_json::json!({}),
                cwd: Some("/tmp/proj".to_string()),
            },
            StructuralOp::MoveSurface {
                source_surface_id: 10,
                target_surface_id: 11,
            },
        ];
        for op in ops {
            let msg = StreamControl::StructuralOp {
                op_id: 1,
                op: op.clone(),
                origin: Some(ForwardOrigin::Agent),
            };
            let s = serde_json::to_string(&msg).unwrap();
            let back: StreamControl = serde_json::from_str(&s).unwrap();
            assert_eq!(back, msg, "roundtrip failed for {op:?}");
        }
    }

    /// origin을 보내지 않는 client의 close도 User로 읽어 기존 복원 동작을 유지한다.
    #[test]
    fn a_structural_op_without_origin_is_a_users_op() {
        let raw =
            r#"{"event":"structural_op","op_id":5,"op":{"kind":"close_surface","surface_id":3}}"#;
        let StreamControl::StructuralOp { op_id, op, origin } =
            serde_json::from_str::<StreamControl>(raw).unwrap()
        else {
            panic!("expected structural_op");
        };
        assert_eq!(
            (op_id, op),
            (5, StructuralOp::CloseSurface { surface_id: 3 })
        );
        assert_eq!(origin, None);
        assert_eq!(ForwardOrigin::of_wire(origin), ForwardOrigin::User);
    }

    /// 알려진 variant의 새 필드는 구 parser가 무시하되 프레임 전체를 버리지 않는다.
    #[test]
    fn the_origin_field_is_spelled_and_ignored_as_documented() {
        let msg = StreamControl::StructuralOp {
            op_id: 1,
            op: StructuralOp::CloseTab {
                anchor_surface_id: 2,
            },
            origin: Some(ForwardOrigin::Agent),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""origin":"agent""#), "{s}");
        let unknown_key =
            r#"{"event":"client_resize","surface_id":1,"cols":80,"rows":24,"origin":"agent"}"#;
        assert!(serde_json::from_str::<StreamControl>(unknown_key).is_ok());
    }

    /// 모르는 origin은 Agent, 생략/null은 User로 읽는다.
    #[test]
    fn an_unknown_origin_keeps_the_frame_and_reads_as_agent() {
        let with = |origin: &str| {
            format!(
                r#"{{"event":"structural_op","op_id":9,"op":{{"kind":"close_surface","surface_id":3}},"origin":{origin}}}"#
            )
        };
        let origin_of = |raw: &str| match serde_json::from_str::<StreamControl>(raw) {
            Ok(StreamControl::StructuralOp {
                op_id: 9, origin, ..
            }) => ForwardOrigin::of_wire(origin),
            other => panic!("the frame must parse as the same structural_op: {other:?}"),
        };
        assert_eq!(origin_of(&with(r#""plugin""#)), ForwardOrigin::Agent);
        assert_eq!(origin_of(&with("7")), ForwardOrigin::Agent);
        assert_eq!(origin_of(&with(r#"{"by":"x"}"#)), ForwardOrigin::Agent);
        assert_eq!(origin_of(&with(r#""user""#)), ForwardOrigin::User);
        assert_eq!(origin_of(&with(r#""agent""#)), ForwardOrigin::Agent);
        assert_eq!(origin_of(&with("null")), ForwardOrigin::User);
        let absent =
            r#"{"event":"structural_op","op_id":9,"op":{"kind":"close_surface","surface_id":3}}"#;
        assert_eq!(origin_of(absent), ForwardOrigin::User);
    }

    /// `wire_kind` is a second spelling of the serde tag — it must not drift.
    #[test]
    fn wire_kind_is_the_serde_tag() {
        let ops = [
            StructuralOp::SplitSurface {
                surface_id: 1,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            StructuralOp::SplitPane {
                anchor_surface_id: 1,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            StructuralOp::NewTab {
                anchor_surface_id: 1,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            StructuralOp::CloseSurface { surface_id: 1 },
            StructuralOp::CloseTab {
                anchor_surface_id: 1,
            },
            StructuralOp::ClosePane {
                anchor_surface_id: 1,
            },
            StructuralOp::MoveTab {
                anchor_surface_id: 1,
                from_index: 0,
                to_index: 1,
            },
            StructuralOp::ConvertSurface {
                surface_id: 1,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
                cwd: None,
            },
            StructuralOp::MoveSurface {
                source_surface_id: 1,
                target_surface_id: 2,
            },
            StructuralOp::RestoreClosedItem {
                anchor_surface_id: 1,
            },
            StructuralOp::CloseWorkspace {
                anchor_surface_id: 1,
            },
        ];
        for op in ops {
            let v = serde_json::to_value(&op).unwrap();
            assert_eq!(v["kind"], op.wire_kind(), "{op:?}");
        }
    }

    #[test]
    fn structural_op_defaults_terminal_kind_and_empty_params() {
        // Client may omit surface_kind/params for a plain terminal split.
        let raw = r#"{"kind":"split_surface","surface_id":1,"direction":"vertical"}"#;
        let op: StructuralOp = serde_json::from_str(raw).unwrap();
        match op {
            StructuralOp::SplitSurface {
                surface_kind,
                params,
                ..
            } => {
                assert_eq!(surface_kind, "terminal");
                assert_eq!(params, serde_json::Value::Null);
            }
            _ => panic!("expected split_surface"),
        }
    }

    /// wire 하위호환: `cwd` 키를 보내지 않는 구버전 client 의 convert op 도
    /// 그대로 역직렬화되고 `cwd: None` 이 된다(서버가 자체 resolve 로 폴백).
    #[test]
    fn convert_surface_without_cwd_key_deserializes_to_none() {
        let raw =
            r#"{"kind":"convert_surface","surface_id":7,"surface_kind":"explorer","params":{}}"#;
        let op: StructuralOp = serde_json::from_str(raw).unwrap();
        match op {
            StructuralOp::ConvertSurface {
                cwd, surface_id, ..
            } => {
                assert_eq!(surface_id, 7);
                assert!(cwd.is_none());
            }
            other => panic!("expected convert_surface, got {other:?}"),
        }
    }

    #[test]
    fn stream_control_structural_result_roundtrip() {
        let ok = StreamControl::StructuralResult {
            created_surface_id: None,
            op_id: 9,
            ok: true,
            reason: None,
            history_omitted: None,
        };
        let s = serde_json::to_string(&ok).unwrap();
        assert!(s.contains(r#""event":"structural_result""#));
        assert!(!s.contains("reason")); // skipped when None
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), ok);

        let fail = StreamControl::StructuralResult {
            created_surface_id: None,
            op_id: 9,
            ok: false,
            reason: Some("unsupported kind: markdown".to_string()),
            history_omitted: None,
        };
        let s = serde_json::to_string(&fail).unwrap();
        assert!(s.contains("unsupported kind"));
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), fail);

        // An older client ignores the count; an older server never sends it.
        let omitted = StreamControl::StructuralResult {
            created_surface_id: None,
            op_id: 9,
            ok: true,
            reason: None,
            history_omitted: Some(2),
        };
        let s = serde_json::to_string(&omitted).unwrap();
        assert!(s.contains(r#""history_omitted":2"#), "{s}");
        assert_eq!(serde_json::from_str::<StreamControl>(&s).unwrap(), omitted);
        let older: StreamControl =
            serde_json::from_str(r#"{"event":"structural_result","op_id":9,"ok":true}"#).unwrap();
        assert!(matches!(
            older,
            StreamControl::StructuralResult {
                history_omitted: None,
                ..
            }
        ));
    }

    #[test]
    fn stream_control_structural_delta_roundtrip() {
        let msg = StreamControl::StructuralDelta {
            workspace_id: 3,
            tree: serde_json::json!({
                "panes": [{ "id": 1, "tabs": [] }],
                "focused_pane": 1,
            }),
            surfaces: vec![
                serde_json::json!({"remote_id": 10, "role": "terminal", "cols": 80, "rows": 24}),
                serde_json::json!({"remote_id": 11, "role": "placeholder", "kind": "markdown"}),
            ],
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"structural_delta""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn structural_delta_not_confused_with_other_events() {
        let delta = serde_json::to_string(&StreamControl::StructuralDelta {
            workspace_id: 1,
            tree: serde_json::Value::Null,
            surfaces: vec![],
        })
        .unwrap();
        match serde_json::from_str::<StreamControl>(&delta).unwrap() {
            StreamControl::StructuralDelta { workspace_id, .. } => assert_eq!(workspace_id, 1),
            other => panic!("expected StructuralDelta, got {other:?}"),
        }
        assert!(matches!(
            serde_json::from_str::<StreamControl>(
                r#"{"event":"resize","surface_id":1,"cols":80,"rows":24}"#
            )
            .unwrap(),
            StreamControl::Resize { .. }
        ));
    }

    #[test]
    fn structural_op_anchor_surface_id() {
        assert_eq!(
            StructuralOp::CloseSurface { surface_id: 5 }.anchor_surface_id(),
            5
        );
        assert_eq!(
            StructuralOp::ClosePane {
                anchor_surface_id: 9
            }
            .anchor_surface_id(),
            9
        );
        assert_eq!(
            StructuralOp::MoveSurface {
                source_surface_id: 3,
                target_surface_id: 4
            }
            .anchor_surface_id(),
            3
        );
    }

    #[test]
    fn structural_op_with_anchor_surface_id() {
        let local = StructuralOp::SplitPane {
            anchor_surface_id: 5, // local mirror id
            direction: SplitAxis::Vertical,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let remote = local.with_anchor_surface_id(100);
        assert_eq!(remote.anchor_surface_id(), 100);
        let mv = StructuralOp::MoveSurface {
            source_surface_id: 1,
            target_surface_id: 2,
        }
        .with_anchor_surface_id(9);
        match mv {
            StructuralOp::MoveSurface {
                source_surface_id,
                target_surface_id,
            } => {
                assert_eq!(source_surface_id, 9);
                assert_eq!(
                    target_surface_id, 2,
                    "anchor 치환은 target 을 바꾸지 않는다"
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(mv.move_target_surface_id(), Some(2));
        let mv = mv.with_move_target_surface_id(11);
        assert_eq!(mv.move_target_surface_id(), Some(11));
        assert_eq!(mv.anchor_surface_id(), 9);
        assert_eq!(remote.move_target_surface_id(), None);
        assert_eq!(remote.with_move_target_surface_id(11), remote);
    }

    #[test]
    fn split_axis_ipc_str() {
        assert_eq!(SplitAxis::Vertical.as_ipc_str(), "vertical");
        assert_eq!(SplitAxis::Horizontal.as_ipc_str(), "horizontal");
    }

    #[test]
    fn stream_control_mesh_context_roundtrip() {
        use tasty_plugin_protocol::protocol::ThemeWire;
        use tasty_type_appearance::theme::ThemeColors;

        // 큰 ThemeColors 객체가 json! 재귀 한도를 넘지 않도록 JSON 문자열로 만든다.
        const COLORS_JSON: &str = r##"{
            "crust":"#11111b","mantle":"#181825","base":"#1e1e2e","surface0":"#313244",
            "surface1":"#45475a","surface2":"#585b70","overlay0":"#6c7086","overlay1":"#7f849c",
            "overlay2":"#9399b2","text":"#cdd6f4","subtext1":"#bac2de","subtext0":"#a6adc8",
            "placeholder":"#9399b2","blue":"#89b4fa","green":"#a6e3a1","red":"#f38ba8",
            "yellow":"#f9e2af","peach":"#fab387","mauve":"#cba6f7","teal":"#94e2d5",
            "sky":"#89dceb","lavender":"#b4befe","flamingo":"#f2cdcd","pink":"#f5c2e7",
            "maroon":"#eba0ac","rosewater":"#f5e0dc","selection_bg":"#585b70",
            "vi_cursor_bg":"#f9e2af","search_match_bg":"#f9e2af","search_match_active_bg":"#fab387","search_match_active_fg":"#1e1e2e",
            "ansi_black":"#45475a","ansi_red":"#f38ba8","ansi_green":"#a6e3a1","ansi_yellow":"#f9e2af",
            "ansi_blue":"#89b4fa","ansi_magenta":"#f5c2e7","ansi_cyan":"#94e2d5","ansi_white":"#bac2de",
            "ansi_bright_black":"#585b70","ansi_bright_red":"#f38ba8","ansi_bright_green":"#a6e3a1",
            "ansi_bright_yellow":"#f9e2af","ansi_bright_blue":"#89b4fa","ansi_bright_magenta":"#f5c2e7",
            "ansi_bright_cyan":"#94e2d5","ansi_bright_white":"#a6adc8"
        }"##;
        let colors: ThemeColors = serde_json::from_str(COLORS_JSON).unwrap();

        let msg = StreamControl::MeshContext {
            surface_id: 5,
            width_px: 800,
            height_px: 600,
            pixels_per_point: 2.0,
            theme: Some(ThemeWire {
                colors,
                is_light: false,
                ui_zoom: 1.0,
            }),
            focused: true,
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"mesh_context""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);

        let no_theme = StreamControl::MeshContext {
            surface_id: 6,
            width_px: 100,
            height_px: 100,
            pixels_per_point: 1.0,
            theme: None,
            focused: false,
        };
        let s2 = serde_json::to_string(&no_theme).unwrap();
        assert!(!s2.contains("theme"));
        assert_eq!(
            serde_json::from_str::<StreamControl>(&s2).unwrap(),
            no_theme
        );
    }

    #[test]
    fn stream_control_mesh_input_roundtrip() {
        use tasty_plugin_protocol::protocol::{RawInputEventWire, RawInputWire};

        let msg = StreamControl::MeshInput {
            surface_id: 8,
            input: RawInputWire {
                time: Some(1.5),
                focused: true,
                modifiers: Default::default(),
                events: vec![
                    RawInputEventWire::PointerMoved { x: 10.0, y: 20.0 },
                    RawInputEventWire::PointerGone,
                ],
            },
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"mesh_input""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_mesh_full_resend_request_roundtrip() {
        let msg = StreamControl::MeshFullResendRequest { surface_id: 3 };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"mesh_full_resend_request""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn stream_control_mesh_error_roundtrip() {
        let msg = StreamControl::MeshError {
            surface_id: 11,
            reason: "surface is not egui-mesh whitelisted".to_string(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"mesh_error""#));
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn mesh_events_are_distinct_from_each_other_and_existing_events() {
        let ctx = serde_json::to_string(&StreamControl::MeshContext {
            surface_id: 1,
            width_px: 1,
            height_px: 1,
            pixels_per_point: 1.0,
            theme: None,
            focused: false,
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&ctx).unwrap(),
            StreamControl::MeshContext { .. }
        ));
        let resend =
            serde_json::to_string(&StreamControl::MeshFullResendRequest { surface_id: 1 }).unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&resend).unwrap(),
            StreamControl::MeshFullResendRequest { .. }
        ));
        let resize = serde_json::to_string(&StreamControl::Resize {
            surface_id: 1,
            cols: 80,
            rows: 24,
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_str::<StreamControl>(&resize).unwrap(),
            StreamControl::Resize { .. }
        ));
    }

    #[test]
    fn open_params_target_workspace_roundtrip() {
        let p = StreamOpenParams {
            proto: 1,
            target: None,
            target_workspace: Some(9),
            bulk_workspace: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: StreamOpenParams = serde_json::from_str(&s).unwrap();
        assert_eq!(back.target_workspace, Some(9));
        assert_eq!(back.target, None);
        assert_eq!(back.bulk_workspace, None);
        let old: StreamOpenParams = serde_json::from_str(r#"{"proto":1}"#).unwrap();
        assert_eq!(old.target_workspace, None);
        assert_eq!(old.bulk_workspace, None);
    }

    #[test]
    fn open_params_bulk_workspace_roundtrip_and_backward_compat() {
        let p = StreamOpenParams {
            proto: 1,
            target: None,
            target_workspace: None,
            bulk_workspace: Some(7),
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains(r#""bulk_workspace":7"#));
        let back: StreamOpenParams = serde_json::from_str(&s).unwrap();
        assert_eq!(back.bulk_workspace, Some(7));
        assert_eq!(back.target, None);
        assert_eq!(back.target_workspace, None);
        let old: StreamOpenParams =
            serde_json::from_str(r#"{"proto":1,"target_workspace":3}"#).unwrap();
        assert_eq!(old.bulk_workspace, None);
        assert_eq!(old.target_workspace, Some(3));
    }

    #[test]
    fn stream_control_loss_roundtrip() {
        let msg = StreamControl::Loss { frames: 1734 };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"loss""#), "{s}");
        assert!(s.contains(r#""frames":1734"#), "{s}");
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn client_loss_notify_roundtrip() {
        let msg = StreamControl::ClientLossNotify {};
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""event":"client_loss_notify""#), "{s}");
        let back: StreamControl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, msg);
    }

    /// 모르는 event의 파싱 실패를 확인한다. 구 peer도 Loss를 무시하므로 명시적 선언이 필요하다.
    #[test]
    fn an_unknown_event_fails_to_parse_so_a_missed_loss_reads_as_no_loss() {
        let from_a_newer_build = r#"{"event":"a_variant_that_does_not_exist_here","frames":9}"#;
        assert!(serde_json::from_str::<StreamControl>(from_a_newer_build).is_err());
    }

    /// 추가 Loss 기능 때문에 기존 peer와의 handshake 버전이 바뀌지 않았는지 확인한다.
    #[test]
    fn adding_a_control_variant_does_not_move_the_handshake_version() {
        assert_eq!(STREAM_PROTO, 1);
    }
}
