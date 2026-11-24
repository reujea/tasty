//! Outbound mirror session ownership. App applies received values to explicit engine/View targets.
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tasty_ipc::stream::StreamTag;
use tasty_model::Workspace;
/// Host-independent remote directory values; the application chooses its local picker cache.
#[derive(Clone)]
pub struct RemoteDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub ext: String,
    /// OS 가 숨김으로 표시했는가. 옛 server 는 싣지 않아 거짓이다.
    pub os_hidden: bool,
    /// 링크 상태. 옛 server 는 싣지 않아 링크 아님이다.
    pub link: RemoteLink,
}

/// 원격 목록 항목의 링크 상태. 링크 구분을 싣지 않는 옛 server 의 항목과 모르는 값은 `NotALink` 다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemoteLink {
    #[default]
    NotALink,
    /// 대상이 있다. 종류·크기·수정 시각은 대상의 값이다.
    Valid,
    /// 대상이 없다. 크기·수정 시각은 링크 자신의 값이다.
    Broken,
}
/// 출력과 resize를 같은 버퍼에 도착 순서대로 담아 올바른 크기의 그리드에 적용한다.
pub enum MirrorEvent {
    /// 셋째 값은 reader가 받은 시각이다. 적용이 밀려도 출력 시각은 받은 때로 센다.
    Data(u32, Vec<u8>, std::time::Instant),
    /// 넷째 값은 reader가 받은 시각이다. 재그리기 구간을 받은 때부터 잰다.
    Resize(u32, usize, usize, std::time::Instant),
    /// 서버가 적용하지 않은 크기 요청. 값은 그 요청의 것이다.
    ResizeRejected(u32, usize, usize),
    /// 로컬 PTY가 없는 mirror의 busy 상태는 서버에서 받는다.
    Activity(u32, bool),
    /// None이면 원격 attention 해제다. 로컬 완료 감지와 별도로 반영한다.
    Attention(u32, Option<tasty_ipc::stream::AttentionKindWire>),
    /// 서버가 넘긴 알림음 한 번. 원격 surface ID이며 0은 workspace 알림이다.
    NotificationSound(u32),
    /// 원격 cwd. 로컬 파일시스템 경로로 사용하지 않으며 None이면 값을 지운다.
    Cwd(u32, Option<String>),
    /// 원격 전경 프로세스 이름. None이면 값을 지운다.
    Foreground(u32, Option<String>),
    /// 구조 변경 실패. 에이전트 요청은 로그, 사용자 요청은 toast로 알린다.
    StructuralFailed(u64, Option<String>),
    /// 사용자 요청의 포커스 의도를 뒤따르는 StructuralDelta에 전달할 op_id.
    /// 셋째 값은 닫은 항목 기록에 내용 없이 남긴 terminal 수다.
    StructuralSucceeded(u64, Option<u32>, u32),
    /// 원격의 전체 구조. 기존 surface의 로컬 ID·터미널은 가능한 한 재사용한다.
    StructuralDelta {
        workspace_id: u32,
        tree: Value,
        surfaces: Vec<Value>,
    },
    /// capture_result 커스텀 이벤트. 경로는 원격 파일시스템의 경로다. `ok` 는 경로 텍스트를 원격
    /// 클립보드에 쓴 것이고, 저장만 하고 쓰지 못했으면 `ok` 가 false 인 채 `path` 가 온다.
    CaptureResult {
        upload_id: Option<u64>,
        ok: bool,
        path: Option<String>,
        reason: Option<String>,
        /// 이미지도 함께 게시했는가. None 이면 이 필드를 싣지 않는 옛 서버라 경로만 쓴 것이다.
        image: Option<bool>,
        image_reason: Option<String>,
    },
    /// list_dir_result 커스텀 이벤트. dir은 서버가 반환한 절대경로다.
    ListDirResult {
        request_id: u64,
        ok: bool,
        dir: Option<String>,
        entries: Option<Vec<RemoteDirEntry>>,
        /// 서버가 프레임 크기 때문에 목록을 잘랐으면 사용자에게 알린다.
        truncated: bool,
        reason: Option<String>,
        /// 폴더가 있는 디스크의 서버 쪽 식별값. 같은 값이면 같은 디스크다. 옛 서버는 싣지 않는다.
        volume: Option<u64>,
    },
    /// 재조립된 mesh frame. bytes는 서버가 footer를 제거한 payload다.
    Mesh(u32, u64, u64, bool, Vec<u8>),
    /// git_query_result의 kind별 JSON은 호스트가 해석하지 않고 플러그인으로 전달한다.
    GitQueryResult {
        request_id: u64,
        ok: bool,
        kind: String,
        data: Option<Value>,
        truncated: bool,
        reason: Option<String>,
    },
    /// markdown_content_result의 surface_id는 원격 ID다. 성공은 file/source, 실패는 reason을 담는다.
    MarkdownContentResult {
        request_id: u64,
        surface_id: u32,
        ok: bool,
        file: Option<String>,
        source: Option<String>,
        truncated: bool,
        reason: Option<String>,
    },
    /// 다른 점유 대상의 문서 신호도 올 수 있어 이 세션의 원격 surface ID인지 확인해야 한다.
    MarkdownChanged {
        surface_id: u32,
    },
    /// 원격 explorer 파일 작업의 진행·결과(`explorer_op_progress`·`explorer_op_result`).
    /// 작업 엔진을 가진 호스트가 해석하므로 JSON 그대로 전달한다.
    ExplorerOp(Value),
    /// 원격 자식 에이전트의 출처 확인·실행 결과·완료 통지. 부모 registry 를 가진 호스트가 처리한다.
    AgentChild(tasty_ipc::agent_child::AgentServerMsg),
    /// holder 가 요청한 원본 터미널 읽기의 결과. 대기 응답을 가진 호스트가 연결 세대를 확인해 적용한다.
    TerminalRead {
        request_id: u64,
        outcome: tasty_ipc::terminal_read::ReadOutcome,
    },
    /// holder 가 요청한 원격 파일 조각·정보. 다운로드 상태를 가진 호스트가 연결 세대를 확인해 적용한다.
    FileDownload {
        request_id: u64,
        outcome: tasty_ipc::file_download::DownloadOutcome,
    },
    Html(tasty_ipc::html_transfer::HtmlReply),
    Http(tasty_ipc::html_http::HttpReply),
    /// 서버 전송 손실. 데이터가 연속이라는 가정을 버리고 재동기화를 요청한다.
    Desynced {
        frames: u64,
    },
}

/// Incoming values have one arrival order and a bounded retained-byte budget. On overflow the
/// connection is explicitly desynchronized; dropping content never masquerades as a continuous cut.
mod outbox {
    use super::MirrorEvent;
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Pending {
        events: Vec<MirrorEvent>,
        bytes: usize,
        /// `events` 가운데 알림음 수. 알림음은 한도와 재동기화에 들지 않는다.
        sounds: usize,
    }
    /// 쌓아 둘 알림음의 최대 수. 넘친 알림음은 화면 재동기화 없이 버린다.
    const MAX_PENDING_SOUNDS: usize = 64;
    fn is_sound(event: &MirrorEvent) -> bool {
        matches!(event, MirrorEvent::NotificationSound(_))
    }
    /// 앞에서부터 알림음을 상한까지만 남기고 남긴 수를 돌려준다.
    fn cap_sounds(events: &mut Vec<MirrorEvent>) -> usize {
        let mut kept = 0;
        events.retain(|event| {
            if !is_sound(event) {
                return true;
            }
            kept += 1;
            kept <= MAX_PENDING_SOUNDS
        });
        kept.min(MAX_PENDING_SOUNDS)
    }
    #[derive(Clone)]
    pub struct MirrorOutbox {
        epoch: crate::connection::ConnectionEpoch,
        pending: Arc<Mutex<Pending>>,
    }
    impl MirrorOutbox {
        pub fn new(epoch: crate::connection::ConnectionEpoch) -> Self {
            Self {
                epoch,
                pending: Arc::new(Mutex::new(Pending::default())),
            }
        }
        pub fn push(&self, event: MirrorEvent) -> bool {
            if !self.epoch.is_active() {
                return false;
            }
            let Ok(mut pending) = self.pending.lock() else {
                if !super::MIRROR_OUTBOX_PUSH_DROPPED
                    .swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    tracing::error!("remote inbox poisoned; arrival discarded");
                }
                return false;
            };
            if !self.epoch.is_active() {
                return false;
            }
            // 알림음은 버려도 되는 프레임이다. 다른 이벤트의 자리를 쓰지 않고, 넘치면 그것만 버린다.
            if is_sound(&event) {
                if pending.sounds < MAX_PENDING_SOUNDS {
                    pending.sounds += 1;
                    pending.events.push(event);
                }
                return true;
            }
            let bytes = event.retained_bytes();
            if pending.events.len() - pending.sounds >= 1024
                || pending.bytes.saturating_add(bytes) > tasty_ipc::admission::QUEUED_BYTES_LIMIT
            {
                let frames = (pending.events.len() - pending.sounds) as u64 + 1;
                pending.events.clear();
                pending.sounds = 0;
                pending.bytes = std::mem::size_of::<MirrorEvent>();
                pending.events.push(MirrorEvent::Desynced { frames });
            } else {
                pending.bytes += bytes;
                pending.events.push(event);
            }
            true
        }
        /// A blocked structural delta and its complete tail precede arrivals received meanwhile.
        /// If preserving the combined tail exceeds the same budget, report loss explicitly.
        pub fn restore_front(&self, mut events: Vec<MirrorEvent>) {
            if !self.epoch.is_active() {
                return;
            }
            let mut pending = tasty_utils::poison::recover_mutex(
                self.pending.lock(),
                super::MIRROR_OUTBOX_WHAT,
                &super::MIRROR_OUTBOX_POISONED,
            );
            let bytes = events
                .iter()
                .filter(|event| !is_sound(event))
                .fold(0usize, |sum, event| {
                    sum.saturating_add(event.retained_bytes())
                });
            let counted = events.iter().filter(|event| !is_sound(event)).count()
                + (pending.events.len() - pending.sounds);
            if counted > 1024
                || bytes.saturating_add(pending.bytes) > tasty_ipc::admission::QUEUED_BYTES_LIMIT
            {
                pending.events.clear();
                pending.events.push(MirrorEvent::Desynced {
                    frames: counted as u64,
                });
                pending.sounds = 0;
                pending.bytes = std::mem::size_of::<MirrorEvent>();
            } else {
                events.append(&mut pending.events);
                pending.sounds = cap_sounds(&mut events);
                pending.events = events;
                pending.bytes += bytes;
            }
        }
        /// 아직 적용하지 않은 이벤트가 남았는지. ID 예약을 기다려 되돌린 이벤트도 포함한다.
        pub fn has_pending(&self) -> bool {
            !tasty_utils::poison::recover_mutex(
                self.pending.lock(),
                super::MIRROR_OUTBOX_WHAT,
                &super::MIRROR_OUTBOX_POISONED,
            )
            .events
            .is_empty()
        }
        pub fn drain(&self) -> Vec<MirrorEvent> {
            let mut pending = tasty_utils::poison::recover_mutex(
                self.pending.lock(),
                super::MIRROR_OUTBOX_WHAT,
                &super::MIRROR_OUTBOX_POISONED,
            );
            pending.bytes = 0;
            pending.sounds = 0;
            if !self.epoch.is_active() {
                pending.events.clear();
                return Vec::new();
            }
            std::mem::take(&mut pending.events)
        }
    }
}

pub use outbox::MirrorOutbox;

/// 입력·heartbeat·제어 프레임을 단일 writer 스레드로 전달한다.
pub struct OutFrame {
    pub tag: StreamTag,
    pub payload: Vec<u8>,
}

pub type FrameSender = Arc<super::connection::ConnectionSender>;
pub type SharedFrameSender = FrameSender;

const MIRROR_OUTBOX_WHAT: &str = "attach mirror outbox";
static MIRROR_OUTBOX_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// poison 복구와 이벤트 폐기는 서로 다른 진단이므로 각각 한 번씩 기록한다.
static MIRROR_OUTBOX_PUSH_DROPPED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Reconnecting에서는 mirror와 scrollback을 남기고 연결만 교체한다.
/// Disconnected는 저장된 원격 행이 형상을 남긴 채 연결 없이 기다리는 상태다(재연결 시간을 넘겼거나
/// 사용자가 끊었다). 완전히 닫힌 세션은 목록에서 제거하므로 Closed 상태는 두지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Connected,
    Reconnecting,
    Disconnected,
}

/// 성공한 사용자 요청의 다음 delta에 한 번 적용할 로컬 포커스 의도.
#[derive(Debug, Clone)]
pub enum PendingOpFocus {
    NewResource,
    /// 옛 포커스가 사라졌으면 우선순위 순서의 원격 ID 후보 중 남은 surface를 선택한다.
    Close {
        candidates: Vec<u32>,
    },
}

/// 회신을 기다리는 에이전트 요청 ID. 회신이나 재연결 때 제거한다.
#[derive(Debug, Default)]
pub struct AgentRequests {
    /// forward 한 구조 op 의 op_id(`PendingStructuralForward::silent_failure`).
    pub structural: HashSet<u64>,
    /// 원격 markdown 원문 요청의 request_id(`PendingMarkdownContentForward::agent_origin`).
    markdown: HashSet<u64>,
}

impl AgentRequests {
    pub fn note_structural(&mut self, agent_origin: bool, op_id: u64) {
        if agent_origin {
            self.structural.insert(op_id);
        }
    }

    pub fn forget_structural(&mut self, op_id: u64) {
        self.structural.remove(&op_id);
    }

    pub fn note_markdown(&mut self, agent_origin: bool, request_id: u64) {
        if agent_origin {
            self.markdown.insert(request_id);
        }
    }

    pub fn take_markdown(&mut self, request_id: u64) -> bool {
        self.markdown.remove(&request_id)
    }

    pub fn clear(&mut self) {
        self.structural.clear();
        self.markdown.clear();
    }
}

#[derive(Default)]
pub struct MirrorStructureIds {
    pub panes: HashMap<u32, u32>,
    pub remote_tabs: HashMap<u32, u32>,
}

impl MirrorStructureIds {
    pub fn retain_workspace(&mut self, workspace: &Workspace) {
        let pane_ids = workspace.pane_layout().all_pane_ids();
        self.panes.retain(|_, local| pane_ids.contains(local));
        self.remote_tabs.retain(|_, local| {
            pane_ids.iter().any(|id| {
                workspace
                    .pane_layout()
                    .find_pane(*id)
                    .is_some_and(|pane| pane.tabs.iter().any(|tab| tab.id == *local))
            })
        });
    }
}

pub struct AttachClientSession {
    pub state: ClientSessionState,
    pub transport: ClientTransport,
}
pub struct ClientTransport {
    pub workers: crate::transport::ConnectionWorkers,
    pub output: MirrorOutbox,
    /// 이 연결의 끝. reader·쓰기 스레드·로컬 해제 가운데 하나만 확정한다. App 은 회차 관측에서만
    /// 확정된 사유를 읽고, I/O 가능 여부는 `is_ending()`으로 본다.
    pub end: Arc<crate::transport_end::TransportEnd>,
    pub frame_tx: SharedFrameSender,
    pub tunnel: Option<tasty_ssh::SshTunnel>,
    /// 서버가 원격 explorer 파일 작업을 받는다. 구 서버 descriptor 에는 이 기능이 없다.
    pub explorer_write: bool,
    /// 서버가 점유한 workspace 전체 닫기를 받는다. 구 서버 descriptor 에는 이 기능이 없다.
    pub workspace_close: bool,
    /// 서버가 원격 자식 에이전트 요청과 완료 통지를 받는다. 구 서버 descriptor 에는 이 기능이 없다.
    pub agent_children: bool,
    /// 서버가 holder 의 원본 터미널 읽기를 받는다. 구 서버 descriptor 에는 이 기능이 없다.
    pub terminal_read: bool,
    /// 서버가 원격 파일 하나 가져오기를 받는다. 구 서버 descriptor 에는 이 기능이 없다.
    pub file_download: bool,
    /// 서버가 생성 요청의 cwd 상속 원본(`inherit_cwd`)을 따른다. 구 서버는 필드를 무시하므로 "상속 안 함"이
    /// 지켜지지 않는다. client 는 이 값과 관계없이 원본을 싣는다.
    pub inherit_cwd: bool,
    /// 서버가 터미널 유래 알림을 직접 만들어 그 소리를 넘긴다(`NOTIFICATION_SOUND_CAPABILITY`). 참이면
    /// mirror가 같은 바이트로 만든 터미널 유래 알림은 소리 없이 기록한다.
    pub notification_sounds: bool,
    /// 이 연결로 출처 hello 를 보냈다. 재연결하면 새 transport 라 다시 보낸다.
    pub agent_hello_sent: bool,
    /// 받은 원격 explorer 작업 통지. 작업 상태를 가진 App 이 도착 순서대로 꺼낸다.
    pub explorer_inbox: Vec<Value>,
    pub html: crate::html_document::HtmlSession,
    #[cfg(test)]
    pub reader_probe: crate::transport::ReaderProbe,
}
pub struct ClientSessionState {
    pub structure_ids: MirrorStructureIds,
    pub local_workspace: u32,
    /// 원격 surface ID를 로컬 mirror ID로 바꾼다.
    pub remote_to_local: HashMap<u32, u32>,
    /// 읽기·쓰기 실패나 종료 통지를 메인 루프의 정리 경로에 전달한다.
    /// 재연결 때 sender를 교체하며 입력 forwarder는 같은 공유 핸들을 유지한다.
    pub phase: SessionState,
    // 이유: 서버 세션 ID를 보관한다. 현재 이 필드를 읽는 경로는 없다.
    #[allow(dead_code)]
    pub client_id: u32,
    /// bulk 연결이 기존 점유에 연결할 원격 workspace ID.
    pub remote_workspace: u32,
    /// bulk 연결도 대화형 attach의 로컬 포트와 SSH 터널을 사용한다.
    pub bulk_port: u16,
    /// 세션이 살아 있는 동안 터널을 유지한다. 직접 loopback 연결은 None.
    #[allow(dead_code)]
    /// 저장된 원격 행이면 그 행 ID(`local_workspace`와 같다). 에이전트 mirror는 None이다.
    pub saved_row: Option<u32>,
    pub op_seq: u64,
    /// 사용자 요청별 포커스 의도. 성공 회신에서 꺼내 다음 delta에 적용한다.
    /// 현재 실패 회신에서는 제거하지 않으며 재연결이나 세션 제거 때 정리된다.
    pub pending_op_focus: HashMap<u64, PendingOpFocus>,
    /// 에이전트 요청의 회신은 사용자 toast 대신 로그로 알린다.
    pub agent_requests: AgentRequests,
    /// 성공 회신 뒤 다음 StructuralDelta가 한 번 소비할 포커스 의도.
    pub next_delta_focus: Option<PendingOpFocus>,
    /// 원격 surface별 크기 요청의 중복 전송 방지·응답 대기·실패 목록. 재연결 때 비운다.
    pub resize_sync: crate::resize_sync::ResizeSync,
    /// 이 연결의 loopback 엔드포인트(`127.0.0.1:<로컬 포트>`). 호스트 표시에는 쓰지 않는다([`Self::host_text`]).
    pub remote_label: String,
    /// 이번 회차에 받아 아직 재생하지 않은 알림음 수. App이 출력 회차 끝에 꺼내 재생한다.
    pub notification_sounds: u32,
    /// 연결할 때 고른 `tasty-attach` 프로필 이름. 연결 시점 값이라 재연결해도, 연결 뒤 프로필을
    /// 바꾸거나 지워도 바꾸지 않는다.
    pub profile: Option<String>,
    /// 응답에 소비자 정보가 없어 요청별로 기록한다. None은 File Picker, Some은 explorer surface ID다.
    pub pending_list_dir_consumers: HashMap<u64, Option<u32>>,
    /// 회신을 기다리는 원격 git 조회와 그 결과를 받을 plugin. 회신을 받으면 지운다.
    /// 재연결 뒤에도 남겨, 세션을 닫을 때 기다리던 plugin에 취소를 보낸다.
    pub git_query_requesters: HashMap<u64, String>,
    /// 회신을 기다리는 캡처 업로드와 그 요청을 만든 View. 회신은 이 View에만 알리고,
    /// 목록에 없는 upload_id 의 회신(중복·지연)은 안내하지 않는다.
    pub capture_uploads: HashMap<u64, std::sync::Weak<()>>,
    /// 로컬 markdown surface ID. mirror 교체·삭제는 일반 lifecycle 큐를 거치지 않아 직접 destroy를 보낸다.
    pub markdown_locals: HashSet<u32>,
    pub html_locals: HashSet<u32>,
    /// 손실 뒤 재attach를 기다리는 동안 통지된 프레임 수.
    /// 서버가 점유 해제를 큐에 넣은 뒤 소켓을 닫으므로 Detach 후 EOF를 기다려 새 attach를 보낸다.
    pub resync_pending: Option<u64>,
    /// parked 상태에서는 재attach할 창이 없어 Detach 전송을 미룬다. 창이 생기면 이어서 처리한다.
    pub resync_awaiting_window: bool,
    /// 이 연결로 보낸 서버 workspace 닫기의 진행.
    pub workspace_close: WorkspaceClose,
    /// 회차 관측이 이 세션의 전송에서 읽은 확정된 끝. 전송(epoch)마다 한 번만 적고, 처리 단계가 한 번만
    /// 분류한다. 새 전송을 설치하면 None으로 시작한다.
    pub observed_end: Option<ObservedEnd>,
    /// 관측·설치 단계가 이 세션을 멈추기로 정한 멈춤. 옛 전송의 해제와 멈춤은 같은 회차의 이벤트
    /// 적용과 닫기 판정 뒤로 넘긴다. 멈춤은 정한 전송의 몫이다. 사용자가 연결을 놓거나 새 전송을 설치하면
    /// 끝나고, 처리 단계는 지금 전송의 멈춤만 적용한다. 처리 단계가 멈출 때 지운다.
    pub stop_after_events: Option<DeferredStop>,
}

/// 관측·설치 단계가 정한 멈춤. 정한 순간의 전송에 묶인다.
#[derive(Clone)]
pub struct DeferredStop {
    pub epoch: super::connection::ConnectionEpoch,
    /// 멈춤을 미룬 동안 그 행에 명시적 연결이 왔다. 처리 단계는 멈춤을 적용한 뒤 그 연결을 잇는다.
    /// 이 멈춤의 일부라 멈춤이 끝나거나 새 멈춤으로 바뀌면 함께 사라진다.
    pub connect_after: bool,
}

impl DeferredStop {
    pub fn new(epoch: super::connection::ConnectionEpoch) -> Self {
        Self {
            epoch,
            connect_after: false,
        }
    }

    /// 이 멈춤이 `epoch` 전송의 것인지.
    pub fn is_for(&self, epoch: &super::connection::ConnectionEpoch) -> bool {
        self.epoch.same(epoch)
    }
}

/// 회차 관측이 기록한 전송의 끝.
#[derive(Clone)]
pub struct ObservedEnd {
    pub epoch: super::connection::ConnectionEpoch,
    pub reason: crate::transport_end::EndReason,
    /// 처리 단계가 이 끝을 이미 분류했다. 같은 끝을 다시 분류하지 않는다.
    pub routed: bool,
    /// 이 끝으로 시작한 재동기화의 연결 시도를 설치 단계가 낡음으로 버렸다. 처리 단계가 같은 회차의 이벤트를
    /// 적용한 뒤 재동기화를 끝낸다(저장 행은 재연결, 에이전트 mirror는 정리). 닫기 회신이 있을 수 있는 이벤트가
    /// 남으면 다음 회차로 넘긴다. 새 전송을 설치하면 끝 기록과 함께 사라지고, 사용자가 연결을 놓으면 지운다.
    pub resync_discarded: bool,
}

/// 서버 workspace 닫기 요청의 진행. 한 연결에서 하나만 기다린다.
/// 성공 회신을 받은 연결만 `Closed`가 되며, 회신 전 단절은 결과를 모르는 것으로 본다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkspaceClose {
    #[default]
    Idle,
    /// 보낸 op_id의 회신을 기다린다.
    Pending(u64),
    /// 보낸 op_id의 회신을 기다리던 전송이 끝나 결과를 모른다고 이미 알렸다. 같은 요청을 다시 알리지
    /// 않는다. 늦게 적용되는 그 op의 회신은 `Pending`처럼 반영한다. 새 전송을 설치하면 `Idle`이 된다.
    Unanswered(u64),
    /// 서버가 workspace를 닫았다고 회신했다. 이 mirror는 재연결하지 않고 정리한다.
    Closed,
}

impl WorkspaceClose {
    /// 구조 변경 회신을 반영한다. 기다리던 op가 아니면 바뀌지 않는다.
    pub fn settle(&mut self, op_id: u64, ok: bool) {
        if matches!(*self, WorkspaceClose::Pending(id) | WorkspaceClose::Unanswered(id) if id == op_id)
        {
            *self = if ok {
                WorkspaceClose::Closed
            } else {
                WorkspaceClose::Idle
            };
        }
    }
}

impl AttachClientSession {
    /// 손실 복구를 위해 옛 연결에 Detach를 보냈다면 다음 EOF는 재attach로 처리한다.
    pub fn resync_released(&self) -> bool {
        self.state.resync_pending.is_some() && !self.state.resync_awaiting_window
    }

    pub fn state(&self) -> SessionState {
        self.state.phase
    }

    pub fn saved_row(&self) -> Option<u32> {
        self.state.saved_row
    }

    /// 현재 sender로 큐에 넣는다. poison 상태에서는 sender를 회수해 사용한다.
    pub fn send_frame(
        &self,
        tag: StreamTag,
        payload: Vec<u8>,
    ) -> Result<(), std::sync::mpsc::SendError<OutFrame>> {
        self.transport.frame_tx.send(OutFrame { tag, payload })
    }
}

impl Drop for ClientTransport {
    fn drop(&mut self) {
        // Closing the old socket is distinct from replaying input into another connection.
        if let Err(error) = self.frame_tx.send(OutFrame {
            tag: StreamTag::Detach,
            payload: Vec::new(),
        }) {
            tracing::debug!("remote detach queue already closed: {error}");
        }
        self.workers.retire_tunnel(self.tunnel.take());
        self.end.raise(crate::transport_end::EndReason::Lost);
        self.end.settle();
        self.frame_tx.retire();
    }
}

/// 설치 중인 새 전송. 설치가 끝나 꺼내지 않고 버려지면(설치 실패) 그 자리에서 끝을 확정하고 소켓을
/// 바로 닫는다. 그냥 버리면 회수 스레드가 worker 를 최대 250ms 기다린 뒤에야 닫는다.
pub struct InstallGuard(Option<ClientTransport>);

impl InstallGuard {
    pub fn new(transport: ClientTransport) -> Self {
        Self(Some(transport))
    }

    pub fn transport(&self) -> &ClientTransport {
        self.0
            .as_ref()
            .expect("the transport stays until it is installed")
    }

    /// 설치에 성공해 전송을 넘긴다.
    pub fn into_installed(mut self) -> ClientTransport {
        self.0.take().expect("the transport is installed once")
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if let Some(transport) = &self.0 {
            transport.close_locally();
        }
    }
}

impl ClientTransport {
    /// App 이 이 연결의 소유권을 놓는 자리에서 부른다. 끝을 `Lost`로 확정하고 소켓을 바로 닫는다.
    /// 이미 다른 쪽이 확정했다면 그 사유가 남는다. 이 끝은 회차 관측의 분류 대상이 아니다.
    pub fn close_locally(&self) {
        self.end.raise(crate::transport_end::EndReason::Lost);
        self.end.settle();
        self.frame_tx.retire();
        self.workers.close_socket();
    }
}

impl MirrorEvent {
    fn retained_bytes(&self) -> usize {
        fn text(value: &Option<String>) -> usize {
            value.as_ref().map_or(0, String::capacity)
        }
        fn json(value: &Value) -> usize {
            std::mem::size_of::<Value>().saturating_add(match value {
                Value::String(value) => value.capacity(),
                Value::Array(values) => values.iter().fold(
                    values
                        .capacity()
                        .saturating_mul(std::mem::size_of::<Value>()),
                    |sum, value| sum.saturating_add(json(value)),
                ),
                Value::Object(values) => {
                    values
                        .iter()
                        .fold(values.len().saturating_mul(128), |sum, (key, value)| {
                            sum.saturating_add(key.capacity())
                                .saturating_add(json(value))
                        })
                }
                _ => 0,
            })
        }
        std::mem::size_of::<Self>().saturating_add(match self {
            Self::Data(_, bytes, _) | Self::Mesh(_, _, _, _, bytes) => bytes.capacity(),
            Self::Cwd(_, value) | Self::Foreground(_, value) | Self::StructuralFailed(_, value) => {
                text(value)
            }
            Self::StructuralDelta { tree, surfaces, .. } => surfaces.iter().fold(
                json(tree).saturating_add(
                    surfaces
                        .capacity()
                        .saturating_mul(std::mem::size_of::<Value>()),
                ),
                |sum, value| sum.saturating_add(json(value)),
            ),
            Self::CaptureResult {
                path,
                reason,
                image_reason,
                ..
            } => text(path)
                .saturating_add(text(reason))
                .saturating_add(text(image_reason)),
            Self::ExplorerOp(value) => json(value),
            Self::AgentChild(msg) => serde_json::to_value(msg).map_or(0, |value| json(&value)),
            Self::TerminalRead { outcome, .. } => {
                serde_json::to_value(outcome).map_or(0, |value| json(&value))
            }
            Self::FileDownload { outcome, .. } => match outcome {
                tasty_ipc::file_download::DownloadOutcome::Chunk { data_b64, .. } => {
                    data_b64.capacity()
                }
                tasty_ipc::file_download::DownloadOutcome::Stat { name, .. } => name.capacity(),
                tasty_ipc::file_download::DownloadOutcome::Refused { .. } => 0,
            },
            Self::Html(reply) => reply.retained_bytes(),
            Self::Http(reply) => reply.retained_bytes(),
            Self::ListDirResult {
                dir,
                entries,
                reason,
                ..
            } => text(dir)
                .saturating_add(text(reason))
                .saturating_add(entries.as_ref().map_or(0, |entries| {
                    entries.iter().fold(
                        entries
                            .capacity()
                            .saturating_mul(std::mem::size_of::<RemoteDirEntry>()),
                        |sum, entry| {
                            sum.saturating_add(entry.name.capacity())
                                .saturating_add(entry.ext.capacity())
                        },
                    )
                })),
            Self::GitQueryResult {
                kind, data, reason, ..
            } => kind
                .capacity()
                .saturating_add(data.as_ref().map_or(0, json))
                .saturating_add(text(reason)),
            Self::MarkdownContentResult {
                file,
                source,
                reason,
                ..
            } => text(file)
                .saturating_add(text(source))
                .saturating_add(text(reason)),
            _ => 0,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RetiredDocuments {
    pub markdown: Vec<u32>,
    pub html: Vec<u32>,
}
impl ClientSessionState {
    /// 원격 호스트 표시(상태바·핸들러 picker·링크 툴팁·File Picker 배지·탐색기)에 쓰는 글자. 프로필 이름이고, 프로필 이름이
    /// 없는 attach는 빈 값이다. 빈 값을 받은 표지는 호스트 대신 `attach.mirror_pill_label` 낱말을 그린다.
    pub fn host_text(&self) -> String {
        self.profile.clone().unwrap_or_default()
    }

    /// Return retired plugin document IDs while retaining exactly the new projection's ownership.
    pub fn replace_document_ids(
        &mut self,
        markdown: HashSet<u32>,
        html: HashSet<u32>,
    ) -> RetiredDocuments {
        fn replace(current: &mut HashSet<u32>, next: HashSet<u32>) -> Vec<u32> {
            let removed = current.difference(&next).copied().collect();
            *current = next;
            removed
        }
        RetiredDocuments {
            markdown: replace(&mut self.markdown_locals, markdown),
            html: replace(&mut self.html_locals, html),
        }
    }
}

#[cfg(test)]
#[path = "close_locally_tests.rs"]
mod close_locally_tests;

#[cfg(test)]
mod retained_bytes_tests {
    use super::MirrorEvent;

    /// 수신 버퍼 예산은 이름 문자열도 센다. 빠지면 큰 이름이 예산을 우회한다.
    #[test]
    fn a_foreground_name_counts_toward_the_inbox_budget() {
        let base = MirrorEvent::Foreground(1, None).retained_bytes();
        let name = "x".repeat(4096);
        let capacity = name.capacity();
        assert_eq!(
            MirrorEvent::Foreground(1, Some(name)).retained_bytes(),
            base + capacity
        );
    }
}

#[cfg(test)]
mod outbox_tests {
    use super::{MirrorEvent, MirrorOutbox};

    /// Given 서버가 넘긴 알림음과 출력, When 수신 큐가 한도까지 찬다, Then 알림음은 출력의 자리를 쓰지 않고
    /// 재동기화를 만들지 않으며 넘치면 그것만 버린다.
    #[test]
    fn notification_sounds_neither_take_output_room_nor_desync() {
        let (sender, _queue) = crate::connection::channel();
        let outbox = MirrorOutbox::new(sender.epoch());
        for _ in 0..1024 {
            assert!(outbox.push(MirrorEvent::NotificationSound(1)));
        }
        for _ in 0..1024 {
            assert!(outbox.push(MirrorEvent::Data(1, vec![b'x'], std::time::Instant::now())));
        }
        assert!(outbox.push(MirrorEvent::NotificationSound(1)));
        let drained = outbox.drain();
        let count = |pick: fn(&MirrorEvent) -> bool| drained.iter().filter(|e| pick(e)).count();
        assert_eq!(count(|e| matches!(e, MirrorEvent::Desynced { .. })), 0);
        assert_eq!(count(|e| matches!(e, MirrorEvent::Data(..))), 1024);
        assert_eq!(
            count(|e| matches!(e, MirrorEvent::NotificationSound(_))),
            64
        );
    }

    /// Given 되돌려 넣을 묶음과 그사이 도착한 것이 각각 알림음 상한만큼 있다, When 되돌려 넣는다, Then 합친
    /// 큐도 상한을 넘지 않고 앞(먼저 받은) 알림음과 출력 순서를 남긴다.
    #[test]
    fn restoring_the_front_keeps_the_sound_limit() {
        let (sender, _queue) = crate::connection::channel();
        let outbox = MirrorOutbox::new(sender.epoch());
        let mut front = vec![MirrorEvent::Data(
            1,
            b"a".to_vec(),
            std::time::Instant::now(),
        )];
        front.extend((0..64).map(|_| MirrorEvent::NotificationSound(1)));
        for _ in 0..64 {
            assert!(outbox.push(MirrorEvent::NotificationSound(2)));
        }
        assert!(outbox.push(MirrorEvent::Data(
            1,
            b"b".to_vec(),
            std::time::Instant::now()
        )));
        outbox.restore_front(front);
        // 상한이 다시 차 있으므로 새 알림음은 버려진다.
        assert!(outbox.push(MirrorEvent::NotificationSound(3)));
        let drained = outbox.drain();
        let sounds: Vec<u32> = drained
            .iter()
            .filter_map(|e| match e {
                MirrorEvent::NotificationSound(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(sounds, vec![1; 64]);
        let data: Vec<&[u8]> = drained
            .iter()
            .filter_map(|e| match e {
                MirrorEvent::Data(_, bytes, _) => Some(bytes.as_slice()),
                _ => None,
            })
            .collect();
        assert_eq!(data, vec![b"a".as_slice(), b"b".as_slice()]);
    }
}
