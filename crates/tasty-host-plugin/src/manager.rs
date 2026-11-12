//! Plugin 프로세스와 등록 정보를 관리한다.
//! GUI 부팅은 활성 plugin을 시작하고 headless는 필요한 시점에 시작할 수 있다.
//! pump가 응답·이벤트·주기 작업을 처리하며 종료 시 shutdown_all로 정리한다.

pub mod file_config;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use tasty_plugin_protocol::SharedBufferId;
use tasty_shm::SharedMemory;

use tasty_plugin_protocol::host_port::SurfaceRegistry;

use crate::handle_channel::HandleListener;
use crate::host_cmd::{HostCmd, SurfaceHandles};
use crate::listener::HostListener;
use crate::process::{PluginProcess, ShutdownBatch};
use crate::protocol::PluginResponse;
use crate::registry_state::PluginsConfig;
use tasty_ipc::ipc_namespace::IpcNamespaceRegistry;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_plugin_manifest::{HookMode, IpcHookDecl, Permission, PluginPackage};

/// host와 plugin 팝업이 함께 사용하는 z-order 순번.
/// 열기·클릭·포커스 시 발급해 두 종류의 팝업 순서를 비교한다.
static NEXT_POPUP_Z_SEQ: AtomicU64 = AtomicU64::new(1);

/// 다음 z-order 순번을 발급한다. host popup 오픈/포커스와 plugin popup 오픈/클릭 양쪽에서
/// 호출된다 — 값 자체의 의미는 없고, 오직 다른 값과의 대소 비교(더 큰 쪽이 위)에만 쓰인다.
pub fn next_popup_z_seq() -> u64 {
    NEXT_POPUP_Z_SEQ.fetch_add(1, Ordering::Relaxed)
}

pub(super) const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(60);
pub(super) const PING_INTERVAL: Duration = Duration::from_secs(15);
/// namespace 응답 대기 한도. healthcheck 기준과 ping 주기 합의 두 배를 사용한다.
/// ping에 답하지만 개별 요청에 답하지 않는 경우도 종료하기 위한 제한이다.
pub(super) const NAMESPACE_CALL_TIMEOUT: Duration =
    Duration::from_secs(2 * (HEALTHCHECK_TIMEOUT.as_secs() + PING_INTERVAL.as_secs()));
/// debug hook 직접 호출의 대기 한도. 매니페스트 hook의 최대 timeout을 사용한다.
#[cfg(debug_assertions)]
pub(super) const DEBUG_HOOK_INVOKE_TIMEOUT: Duration =
    Duration::from_millis(tasty_plugin_manifest::HOOK_TIMEOUT_MS_MAX as u64);
/// namespace 연속 만료가 이 수에 도달하면 다음 ping tick에서 재시작 대상으로 삼는다.
/// hook은 우회할 수 있지만 namespace 호출은 대체 실행이 없어 재시작을 사용한다.
pub(super) const NAMESPACE_EXPIRY_RESTART_LIMIT: u32 = 3;
pub(super) const RESTART_FAILURE_WINDOW: Duration = Duration::from_secs(10);
pub(super) const RESTART_FAILURE_LIMIT: usize = 3;
/// 정상 종료를 기다릴 기간. shutdown_all에서는 같은 deadline을 공유하며 병렬로 기다린다.
/// kill과 프로세스 회수까지 포함한 전체 종료 시간의 상한은 아니다.
pub(super) const PLUGIN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
/// 실행 파일·매니페스트 변경을 확인하는 주기.
pub(super) const AUTO_RELOAD_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// plugin RSS 조회 주기. 조회 비용과 변화 감지 간격을 조절한다.
pub(super) const RSS_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);

/// 매니저 내부 타이머 종류. 호스트는 next_deadline을 자신의 대기 시각과 합친다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginTick {
    /// ping 전송과 마지막 pong 이후 경과 시간에 따른 재시작 판정.
    /// 실제 판정은 호스트가 이 tick을 처리할 때 수행한다.
    Ping,
    /// `RSS_SAMPLE_INTERVAL` 주기 RSS 샘플링.
    Rss,
    /// `AUTO_RELOAD_POLL_INTERVAL` 주기 auto-reload polling. flag off 면 **등록
    /// 자체를 하지 않는다** — 꺼진 기능이 데드라인에 기여하지 않는다.
    AutoReload,
    /// 회수 중인 plugin 이 있을 때만 등록된다 — 끝난 회수를 거두고 미뤄 둔 기동을 한다
    /// (`manager::retire`). 마지막 회수가 끝나면 내려간다.
    Retire,
}

/// IPC 응답을 최종적으로 어디로 회신해야 하는지를 식별. 호스트 외부 caller(CLI/사용자)는
/// `Local`, 다른 plugin이면 `Plugin`.
pub(super) enum FinalCaller {
    Local {
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
        original_id: serde_json::Value,
        /// 원래 호스트 IPC 요청 번호. pre-hook·target·post-hook 대기 항목에 전달한다.
        /// IPC 큐 밖에서 왔거나 file-handler 큐에서 번호를 잃은 요청은 None이다.
        origin: Option<tasty_ipc::server::RequestSeq>,
    },
    Plugin {
        caller_plugin_id: String,
        call_id: u64,
        binding: std::sync::Weak<()>,
    },
}

impl FinalCaller {
    /// 이 회신처를 낳은 IPC 요청의 호스트 번호. plugin 이 부른 호출은 호스트 IPC 큐를 안
    /// 지나 번호가 없다(docs/architecture/ipc-server.md#느린-요청-추적).
    pub(super) fn origin(&self) -> Option<tasty_ipc::server::RequestSeq> {
        match self {
            FinalCaller::Local { origin, .. } => *origin,
            FinalCaller::Plugin { .. } => None,
        }
    }
}

/// 응답을 기다리는 plugin 요청. 종류·수신자·시각을 함께 보관해 종료 시 한 항목으로 정리한다.
pub(super) struct PendingRequest {
    pub(super) kind: PendingRequestKind,
    /// 송신 시각. 응답과 매칭되면 경과 시간을 기록한다.
    pub(super) sent_at: Instant,
    /// 요청을 받은 plugin. 해당 프로세스를 정리할 때 응답 없는 요청도 제거하는 기준이다.
    pub(super) to: String,
    /// 원래 호스트 IPC 요청 번호. hook을 거쳐도 같은 번호를 유지한다.
    /// IPC 밖에서 왔거나 중간 큐에서 번호를 잃었으면 None이다. plugin에는 전송하지 않는다.
    /// 관련 계측: docs/architecture/ipc-server.md#느린-요청-추적.
    pub(super) origin: Option<tasty_ipc::server::RequestSeq>,
}

impl PendingRequest {
    /// 수신자와 현재 송신 시각을 기록한다.
    pub(super) fn now(to: impl Into<String>, kind: PendingRequestKind) -> Self {
        Self {
            kind,
            sent_at: Instant::now(),
            to: to.into(),
            origin: None,
        }
    }

    /// 이 요청을 낳은 IPC 요청의 번호를 붙인다(`None` 이면 그대로).
    pub(super) fn for_request(mut self, origin: Option<tasty_ipc::server::RequestSeq>) -> Self {
        self.origin = origin;
        self
    }
}

/// pending host→plugin request의 종류. 응답 수신 시 어떤 후처리를 할지 식별.
pub(super) enum PendingRequestKind {
    SurfaceCreate {
        surface_id: u32,
        binding: crate::host_cmd::SurfaceBinding,
    },
    SurfaceRestore {
        surface_id: u32,
        binding: crate::host_cmd::SurfaceBinding,
    },
    SurfaceRetire {
        completion: crate::host_cmd::RemoteRetirementCompletion,
        process_binding: std::sync::Weak<()>,
    },
    /// 단축키로 실행한 plugin 명령. SurfaceResult로 표시 이름을 갱신할 수 있다.
    CommandInvoke {
        surface_id: u32,
        binding: Option<crate::host_cmd::SurfaceBinding>,
    },
    /// popup.open IPC 응답 대기. 응답은 [`PopupOpenResult`].
    PopupOpen { instance_id: u64 },
    /// 그 외 (host.hello / ping / 등) — 응답 무시.
    Other,
    /// Client IPC 요청을 plugin namespace로 forward한 경우. plugin이 응답을 주면
    /// 보관한 response_tx로 client에 회신한다.
    NamespaceInvoke {
        plugin_id: String,
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
        original_id: serde_json::Value,
        /// target 응답이 도착해야 하는 시각. 지나면 caller 에 오류로 회신하고 버린다.
        deadline: Instant,
    },
    /// 다른 plugin이 보낸 IpcCall이 namespace 메서드인 경우. target plugin이 응답을
    /// 주면 caller plugin에 `ipc.result`로 회신한다.
    PluginToPluginNamespace {
        /// forward 받은 target plugin (응답을 주는 쪽).
        plugin_id: String,
        /// 호출한 plugin (응답을 받을 쪽).
        caller_plugin_id: String,
        /// caller plugin이 ipc.call 시점에 발급한 call_id.
        call_id: u64,
        caller_binding: std::sync::Weak<()>,
        /// target 응답이 도착해야 하는 시각. 지나면 caller 에 오류로 회신하고 버린다.
        deadline: Instant,
    },
    /// extension의 pre-IPC hook을 dispatch한 뒤 응답 대기. extension이 응답을 주면
    /// (transform이면 payload 교체, filter면 차단 결정) 그 결과로 target plugin에
    /// 실제 ipc.invoke를 보낸다. post-hook이 있으면 함께 전달해 두 번째 phase에 사용.
    ExtensionPreIpcHook {
        target_plugin_id: String,
        extension_plugin_id: String,
        method: String,
        params: serde_json::Value,
        pre_hook_mode: HookMode,
        final_caller: FinalCaller,
        post_hook: Option<IpcHookDecl>,
        /// hook 응답이 도착해야 하는 시각. 지나면 timeout으로 처리.
        deadline: Instant,
    },
    /// extension의 post-IPC hook을 dispatch한 뒤 응답 대기. extension이 응답을 주면
    /// (transform이면 payload 교체, filter면 ignored — post는 차단 무의미)
    /// 그 결과로 caller에 최종 응답.
    ExtensionPostIpcHook {
        extension_plugin_id: String,
        method: String,
        post_hook_mode: HookMode,
        /// target plugin의 응답을 그대로 들고 온 것. Ok면 result, Err면 (msg, code).
        target_outcome: TargetOutcome,
        final_caller: FinalCaller,
        deadline: Instant,
    },
    /// pre-hook 없이 target에 직접 ipc.invoke한 뒤, 매칭 post-hook이 있어
    /// 응답이 오면 post-hook으로 chain해야 하는 경우의 pending.
    NamespaceInvokeWithPostHook {
        target_plugin_id: String,
        method: String,
        extension_plugin_id: String,
        post_hook_decl: IpcHookDecl,
        final_caller: FinalCaller,
        /// target 응답이 도착해야 하는 시각. post-hook 자신의 deadline 이 아니다 —
        /// 그것은 post-hook 을 실제로 보내는 시점에 `timeout_ms` 로 따로 잡는다.
        deadline: Instant,
    },
    /// debug 빌드 한정 — `debug.extension.invoke_hook`이 보낸 hook 응답을 그대로
    /// caller(local CLI)에 회신.
    #[cfg(debug_assertions)]
    DebugExtensionInvokeHook {
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
        original_id: serde_json::Value,
        /// debug hook의 응답 기한. 만료되면 호출자에게 오류를 반환한다.
        deadline: Instant,
    },
    /// extension의 pre-event hook을 dispatch한 뒤 응답 대기. 응답이 오면
    /// (transform이면 envelope.payload 교체, filter면 fan-out 차단)
    /// event_bus.fan_out으로 진행. post_event가 있으면 함께 둔다.
    ExtensionPreEventHook {
        publisher_plugin_id: String,
        extension_plugin_id: String,
        envelope: tasty_plugin_protocol::EventEnvelope,
        pre_hook_mode: HookMode,
        post_hook: Option<tasty_plugin_manifest::EventHookDecl>,
        deadline: Instant,
    },
    /// extension의 post-event hook을 dispatch한 뒤 응답 대기. event는 이미 fan-out 됐으므로
    /// post 응답은 observe로만 의미가 있다 (transform/filter는 ignore).
    ExtensionPostEventHook {
        extension_plugin_id: String,
        event_key: String,
        deadline: Instant,
    },
}

/// 연속 실패 hook의 backoff 윈도우. 3회 연속 실패 후 60초 동안 hook 우회.
pub(super) const HOOK_FAIL_BACKOFF: Duration = Duration::from_secs(60);
pub(super) const HOOK_FAIL_LIMIT: u8 = 3;

/// (ext_id, method) 단위 hook 실패 추적. consecutive_failures가 HOOK_FAIL_LIMIT에 도달하면
/// backoff_until로 설정해 그 동안 hook을 우회한다.
#[derive(Debug, Clone, Default)]
pub(super) struct HookFailureState {
    pub(super) consecutive_failures: u8,
    pub(super) backoff_until: Option<Instant>,
}

/// target plugin의 ipc.invoke 응답 결과. post-hook 진입 시 보존해 둔다.
pub(super) enum TargetOutcome {
    Ok(serde_json::Value),
    Err { message: String, code: i32 },
}

/// extension hook 응답에서 추출한 결정.
pub(super) enum HookOutcome {
    /// payload를 새 값으로 교체 (transform).
    Modified(serde_json::Value),
    /// 차단 (filter).
    Block,
    /// 그 외 — observe, filter pass, transform no-op, 응답 누락, 파싱 실패.
    Pass,
}

/// `ExtensionHookResult` JSON에서 outcome 추출. 에러/누락은 fail-open(`Pass`).
pub(super) fn parse_hook_result(resp: &PluginResponse) -> HookOutcome {
    if resp.error.is_some() {
        return HookOutcome::Pass;
    }
    let result = match &resp.result {
        Some(v) => v,
        None => return HookOutcome::Pass,
    };
    if let Some(v) = result.get("modified_payload")
        && !v.is_null()
    {
        return HookOutcome::Modified(v.clone());
    }
    if let Some(pass) = result.get("pass").and_then(|p| p.as_bool())
        && !pass
    {
        return HookOutcome::Block;
    }
    HookOutcome::Pass
}

pub(super) struct RemoteSurfaceEntry {
    /// 소유 plugin id — surface 닫힘 시 `surface.destroy` 를 이 plugin 에 보낸다.
    pub(super) plugin_id: String,
    pub(super) handles: SurfaceHandles,
    pub(super) publication: RemotePublication,
    /// 원 생성·복원 요청. 원 프로세스가 회수된 뒤 새 프로세스에 다시 게시할 때 쓴다.
    pub(super) origin: reattach::SurfaceOrigin,
}

pub(super) enum RemotePublication {
    NeverSent,
    Sent(std::sync::Weak<()>),
}

/// 최근 수신한 mesh 프레임의 메타데이터. 실제 내용은 공유 버퍼에 있으며 렌더러가 읽는다.
#[derive(Debug, Clone)]
pub struct EguiMeshFrame {
    /// buffer lookup 에 필요한 소유 plugin id.
    pub plugin_id: String,
    /// mesh POD 바이트가 들어있는 shared buffer.
    pub buffer_id: SharedBufferId,
    /// plugin이 알린 footer 세대 번호.
    pub generation: u64,
    /// plugin 렌더 코어의 송신 frame 단조 시퀀스(1부터, buffer 재생성과 무관).
    /// 렌더 prepare 가 `frame_seq == last + 1` 로 textures_delta 체인 연속성을 검증한다.
    /// 구버전 plugin 은 0 → 항상 체인 단절로 취급된다.
    pub frame_seq: u64,
    /// 전체 텍스처 상태를 담은 프레임인지 여부. delta 체인 복구에 사용한다.
    pub full_textures: bool,
    /// `mesh_wire::encode_paint` 가 실제로 만든 바이트 길이(shared buffer 의
    /// power-of-two capacity 가 아니라). attach mesh mirror 가 네트워크로
    /// 정확한 payload 만 내보내는 데 필요. 0 이면 구버전 plugin — consumer 는 버퍼
    /// capacity 전체를 fallback 으로 쓴다.
    pub byte_len: u32,
    /// 이 frame 을 그린 plugin egui pass 의 `PlatformOutput::ime` — IME 를 원하는 위젯이
    /// focus 중이었다면 그 위치(콘텐츠 로컬 논리 포인트). host 가 콘텐츠 영역 origin 을
    /// 더해 창 좌표로 바꾼 뒤 winit `set_ime_cursor_area` 로 OS IME 후보창 위치를 정한다
    /// (`src/view/main.rs` `update_ime_cursor_area`). 편집 위젯이 없으면 `None`.
    /// banner 는 키 입력을 forward 받지 않으므로 늘 `None` 이다.
    pub ime_cursor: Option<tasty_plugin_protocol::ImeCursorWire>,
    /// banner frame에서 plugin이 말줄임한 본문(콘텐츠 로컬 논리 포인트). 호스트가 배너 카드 아래
    /// 툴팁으로 전문을 보인다. surface·popup frame과 구버전 plugin은 `None`이다.
    pub banner_body_cut: Option<tasty_plugin_protocol::BannerBodyCutWire>,
}

pub struct PluginManager {
    packages: Vec<PluginPackage>,
    /// trust gate 에서 거부된 plugin 들 (서명 미신뢰/검증 실패/권한 변경).
    /// `refresh_packages` 가 `packages` 와 함께 갱신한다. UI "확인 필요" 탭 +
    /// 사이드바 경고 배지가 소비. debug 빌드는 trust gate 우회라 항상 비어 있다.
    pub rejected: Vec<crate::discovery::RejectedPlugin>,
    /// 공식 목록에 없는 `com.tasty.*` 폴더. 등록하지 않으며 새로 생긴 항목만 알림 보고로 남긴다.
    reserved: Vec<crate::discovery::ReservedIdBlocked>,
    pub processes: HashMap<String, PluginProcess>,
    pub config: PluginsConfig,
    pub(super) waker: tasty_terminal::waker_factory::SharedWakerFactory,
    pub(super) listener: Option<HostListener>,
    /// 보조 핸들 채널 listener. shared buffer 핸들 전송에 사용. Unix/Windows 양쪽에서
    /// `HandleListener::bind`가 채널을 연다 (Unix=`AF_UNIX` socket, Windows=Named Pipe).
    /// bind 실패 시에만 `None`이 된다.
    pub(super) handle_listener: Option<HandleListener>,
    pub log_dir: PathBuf,
    pub(super) next_request_id: AtomicU64,
    /// plugin 주기 작업 스케줄. `pump(now)` 가 `drain_due` 로 실행하고, 호스트는
    /// [`PluginManager::next_deadline`] 로 자기 대기 계산에 합성한다.
    pub(super) timers: tasty_timer::TimerHub<PluginTick>,
    /// plugin id → 최근 spawn 실패 timestamps. 짧은 시간 내 반복 실패하면 자동 disable.
    pub(super) spawn_failures: HashMap<String, Vec<Instant>>,
    /// 자동 disable되어 사용자가 수동 enable하기 전까지 더 이상 spawn 시도 안 함.
    pub(super) auto_disabled: std::collections::HashSet<String>,
    /// 기동 요청별 결과와 마지막 실패, 호스트 알림으로 넘길 실패 보고.
    pub(super) start_tracker: start_status::StartTracker,
    /// H — auto-reload: plugin id → 마지막 관측한 entry binary mtime.
    pub(super) plugin_binary_mtimes: HashMap<String, SystemTime>,
    /// H — auto-reload: plugin id → 마지막 관측한 manifest version.
    pub(super) plugin_manifest_versions: HashMap<String, String>,
    /// TASTY_PLUGIN_AUTO_RELOAD 설정. 꺼져 있으면 AutoReload 타이머를 등록하지 않는다.
    pub(super) auto_reload_enabled: bool,
    /// hello의 surface kind를 등록할 저장소. 없으면 등록하지 않는다.
    pub surface_registry: Option<Arc<dyn SurfaceRegistry>>,
    /// 이미 registry에 등록된 plugin id (hello를 여러 번 받아도 1회만 등록).
    pub registered_plugins: std::collections::HashSet<String>,
    /// registry create/restore closure가 새 RemoteSurface 등록을 보내는 채널.
    pub host_cmd_tx: Sender<HostCmd>,
    pub(super) host_cmd_rx: Receiver<HostCmd>,
    retirement_control: pump::RetirementControl,
    /// surface_id → RemoteSurface handle. 라이프사이클 동안 유지.
    pub(super) surfaces: HashMap<u32, RemoteSurfaceEntry>,
    /// host → plugin 요청 ID → 종류. 응답 수신 시 후처리 dispatch용.
    pub(super) pending_requests: HashMap<u64, PendingRequest>,
    /// 호스트가 주입한 plugin 왕복 대기 계측. 없으면 기록하지 않아 미측정과 0을 구별한다.
    plugin_wait: Option<Arc<tasty_telemetry::PluginWaitStats>>,
    /// 느린 요청 링(docs/architecture/ipc-server.md#느린-요청-추적). 원 IPC 요청 번호를 든 대기 항목이 끝날 때(응답 · 만료 · 취소)
    /// 그 hop 을 원 요청의 줄에 붙인다. `plugin_wait` 과 같은 이유로 `Option` 이고, `None` 이면
    /// 아무것도 안 남긴다.
    slow_requests: Option<Arc<tasty_telemetry::SlowRequestLog>>,
    /// 프로세스 전체의 plugin 채널 바이트 계측. 모든 매니저가 같은 ChannelLedger를 사용한다.
    pub(super) channel_ledger: Arc<crate::process::channel_bytes::ChannelLedger>,
    /// 각 plugin에 grant된 권한. 매니페스트 + plugins.toml의 granted를 교집합한 결과.
    /// `Arc`로 공유하여 CallerContext가 동시 호출 시 안전.
    plugin_permissions: HashMap<String, Arc<HashSet<Permission>>>,
    /// plugin이 보낸 IpcCall을 호스트의 main loop에서 라우팅 처리하기 위해 모으는 큐.
    /// `App::process_plugin_ipc_calls()`가 매 tick에 비운다.
    pub(super) pending_plugin_calls: Vec<PendingPluginCall>,
    /// plugin이 매니페스트로 선언한 단축키 command 일람. plugin
    /// enable/disable/install/remove 시 갱신됨.
    pub command_registry: super::command_registry::PluginCommandRegistry,
    /// plugin 설정 페이지. 등록 후 설정 화면에서 사용하며 disable·재시작 시 정리한다.
    pub settings_pages: crate::settings_registry::SettingsPageRegistry,
    /// 설치 매니페스트에서 만든 namespace 소유자 목록. 실행 상태와 별도로 관리한다.
    /// tasty-ipc와 같은 Arc를 공유하므로 패키지 갱신은 refresh_packages를 거쳐야 한다.
    ipc_namespaces: Arc<RwLock<IpcNamespaceRegistry>>,
    /// plugin id → (buffer id → 매핑 영역). 호스트가 `host.shared_buffer.create`로
    /// 발급한 영역의 매핑 유지(=OS region keep-alive)와 dirty 수신 시 lookup용.
    /// plugin process가 종료/재시작되면 해당 plugin 슬롯이 통째로 drop되어
    /// 매핑이 해제된다.
    pub(super) plugin_buffers: HashMap<String, HashMap<SharedBufferId, SharedMemory>>,
    /// 이 매니저가 다음에 발급할 공유 버퍼 ID.
    pub(super) next_buffer_id: AtomicU64,
    /// surface별 최근 mesh 메타데이터. plugin 종료·재시작 시 제거한다.
    pub(super) egui_mesh_frames: HashMap<u32, EguiMeshFrame>,
    /// egui-mesh popup instance_id → 최근 paint_frame 메타 (A2). plugin 의
    /// `PopupPaintFrame` 알림마다 갱신되고, 호스트 popup 합성기가 instance_id 로
    /// lookup 한다. popup 이 닫히거나 plugin 이 종료되면 해당 엔트리를 정리한다.
    pub(super) popup_mesh_frames: HashMap<u64, EguiMeshFrame>,
    /// 확장 plugin의 Active·Pending·Disabled·Conflict 상태. hook 전달 대상을 찾는 데 사용한다.
    extensions: super::extension_registry::ExtensionRegistry,
    /// 설치·활성 플러그인의 상속 환경변수 제거 규칙. `recompute_extensions`가 함께 갱신한다.
    inherited_env: std::collections::BTreeMap<String, tasty_utils::process::InheritedEnvRules>,
    /// 설치된 플러그인의 namespace 별 소유 상태(활성·허가 권한). agent provider 자격 판정이 읽는다.
    namespace_owners:
        std::collections::BTreeMap<String, tasty_utils::plugin_namespaces::NamespaceOwner>,
    /// 부팅 매니저만 켠다. 켜져 있으면 위 규칙과 소유 상태를 다른 스레드가 읽는 공용 표에 반영한다.
    publishes_shared_tables: bool,
    /// (ext_id, method) 단위 hook 실패 추적. 3회 연속 실패하면 60초간 backoff.
    pub(super) hook_failures: HashMap<(String, String), HookFailureState>,
    /// plugin별 namespace 연속 만료 횟수. 응답을 받거나 plugin을 정리하면 지운다.
    /// 기록된 만료 ID와 일치하는 늦은 응답도 횟수를 지운다.
    pub(super) namespace_expiries: HashMap<String, u32>,
    /// 최근 만료된 namespace 요청 ID. 늦은 응답을 구별해 만료 횟수를 지우는 데 사용한다.
    /// 저장 개수는 NAMESPACE_EXPIRY_RESTART_LIMIT로 제한한다. hook 응답은 포함하지 않는다.
    pub(super) expired_namespace_calls: HashMap<String, VecDeque<u64>>,
    /// Event Bus 1.0 라우터. 호스트 본문과 plugin 간 broadcast 이벤트를 fan-out.
    pub event_bus: super::event_bus::EventBus,
    /// 호스트가 발화하는 envelope의 `meta.trace_id` 카운터.
    pub(super) event_trace_seq: AtomicU64,
    /// 현재 열려 있는 popup 인스턴스. host가 발급한 `instance_id`를 키로 사용.
    pub(super) popup_instances: HashMap<u64, PopupInstance>,
    /// 다음 popup `instance_id`. 1부터 시작해 단조 증가.
    pub(super) next_popup_instance_id: u64,
    /// surface 범위 이벤트 팝업 중 대상 surface 의 소유 확인을 기다리는 요청.
    /// 매니저는 surface 트리를 모르므로 호스트가 꺼내 확인한 뒤 연다.
    pub(super) pending_surface_popups: Vec<PendingSurfacePopup>,
    /// 현재 열려 있는 banner 인스턴스(A3). host가 발급한 `instance_id`를 키로 사용.
    pub(super) banner_instances: HashMap<u64, BannerInstance>,
    /// 다음 banner `instance_id`. popup 과 별도 카운터, 1부터 단조 증가.
    pub(super) next_banner_instance_id: u64,
    /// egui-mesh banner instance_id → 최근 paint_frame 메타(A3). plugin 의
    /// `BannerPaintFrame` 알림마다 갱신되고, 호스트 banner 합성기가 instance_id 로
    /// lookup 한다. banner 가 닫히거나 plugin 이 종료되면 해당 엔트리를 정리한다.
    pub(super) banner_mesh_frames: HashMap<u64, EguiMeshFrame>,
    /// SurfaceInvalidated로 재그리기를 요청한 surface. pump가 넣고 take_invalidated_surfaces가 가져간다.
    pub(super) invalidated_surfaces: Vec<u32>,
    /// 새 PaintFrame을 받은 surface. 이 frame은 창이 그린 뒤에 도착할 수 있어 호스트가 다시
    /// 그려야 화면에 나온다. pump가 넣고 take_fresh_frame_surfaces가 가져간다.
    pub(super) fresh_frame_surfaces: std::collections::HashSet<u32>,
    /// 입력 없이도 재그리기가 필요한 popup의 요청. pump가 넣고 take_invalidated_popups가 가져간다.
    pub(super) invalidated_popups: Vec<u64>,
    /// 입력 없이도 재그리기가 필요한 banner의 요청. pump가 넣고 take_invalidated_banners가 가져간다.
    pub(super) invalidated_banners: Vec<u64>,
    /// sysinfo 측정 핸들 — tick 마다 새로 만들지 않고 재사용(할당 비용 절감).
    pub(super) sys: sysinfo::System,
    /// 이번 tick의 plugin별 RSS. App이 가져가 이상 탐지·저장·알림을 처리한다.
    pub(super) pending_rss_samples: Vec<(String, u64)>,
    /// 파일 형식 식별 시스템. plugin enable/disable 시 detector 추가/제거.
    /// 호스트 본문이 CoreState 와 같은 Arc 를 공유 (trait object 로 의존성 격리).
    pub file_format: Arc<dyn tasty_plugin_protocol::host_port::FileFormatRegistryPort>,
    /// 파일 핸들러 시스템. plugin enable/disable 시 handler 추가/제거.
    pub file_handler: Arc<dyn tasty_plugin_protocol::host_port::FileHandlerRegistryPort>,
    /// 호스트가 주입한 훅 핸들러 레지스트리. 없으면 기여 등록·해제를 생략한다.
    pub hook_handler: Option<Arc<dyn tasty_plugin_protocol::host_port::HookHandlerRegistryPort>>,
    /// 호스트가 주입한 완료 판정 전략 레지스트리. 없으면 기여 등록·해제를 생략한다.
    pub completion_strategy:
        Option<Arc<dyn tasty_plugin_protocol::host_port::CompletionStrategyRegistryPort>>,
    /// 번역 namespace 등록 인터페이스. 없으면 등록하지 않는다.
    pub i18n_registrar: Option<Arc<dyn tasty_plugin_protocol::host_port::I18nNamespaceRegistrar>>,
    /// 플러그인 자식 프로세스 수명을 호스트에 결박하는 크로스 플랫폼 reaper.
    /// Windows 는 Job Object 핸들을 여기 보유해야 tasty 수명과 KILL_ON_JOB_CLOSE
    /// 가 연동된다. spawn 경로가 prepare/adopt 를 호출. 상세 [`crate::reaper`].
    pub(super) plugin_reaper: crate::reaper::PluginReaper,
    /// 진행 중인 종료 대기. `begin_shutdown_all()` 이 채우고 `poll_shutdown_all()`
    /// 이 비운다. `None` 이면 종료 대기 중이 아니다.
    pub(super) shutdown_batch: Option<ShutdownBatch>,
    /// 무응답 재시작 · disable 로 내려가는 중인 plugin. 회수 대기는 스레드가 하고
    /// 메인 스레드는 [`PluginTick::Retire`] 에서 끝난 것만 거둔다(`manager::retire`).
    pub(super) retiring: HashMap<String, retire::Retiring>,
    /// 회수를 마친 프로세스의 응답 바인딩. 그 프로세스가 만든 surface 를 나중에 닫을 때
    /// 소멸 증거로 쓴다. 프로세스 교체 한 번에 한 항목씩만 늘어난다.
    pub(super) reaped_generations: Vec<std::sync::Weak<()>>,
    /// 보낸 mesh bootstrap 의 게시 상태. 회수한 세대를 아직 가리키는지 판정한다.
    pub(super) mesh_publications: crate::host_cmd::MeshPublicationRegistry,
    /// 호스트가 접수했지만 아직 끝나지 않은 닫기가 회수할 수 있는 surface. 새 프로세스에
    /// 다시 게시하지 않는다(`manager::reattach`).
    pub(super) closing_surfaces: std::collections::BTreeSet<u32>,
}

/// 호스트가 추적 중인 popup 인스턴스 한 건. plugin process가 죽으면 함께 제거된다.
#[derive(Debug, Clone)]
pub struct PopupInstance {
    pub plugin_id: String,
    pub popup_id: String,
    pub contribute: tasty_plugin_manifest::PopupContribute,
    /// host↔plugin popup 통합 z-order 순번(`next_popup_z_seq`). open 시 발급되고, 콘텐츠
    /// 영역 클릭 시(host 쪽 `touch_popup_instance_z`) 갱신된다.
    pub z_seq: u64,
    /// `contribute.scope = "surface"` popup 이 속한 host surface id. popup 을 여는 host
    /// 진입점이 [`PluginManager::bind_popup_instance_surface`] 로 채운다. `None` 이면
    /// 렌더는 선언과 무관하게 window 범위로 다룬다(plugin 이 스스로 연 popup 등).
    pub scope_surface: Option<u32>,
}

/// surface 범위로 선언한 이벤트 팝업을 대상 surface 와 함께 연다는 요청.
/// 호스트는 대상이 `plugin_id` 소유의 플러그인 surface 이고, 플러그인이 발행한 이벤트라면
/// 발행자가 `plugin_id` 일 때만 연다. 그 밖에는 거절한다(docs/design/systems/popup.md).
#[derive(Debug, Clone)]
pub struct PendingSurfacePopup {
    pub plugin_id: String,
    pub popup_id: String,
    pub context: serde_json::Value,
    /// 이벤트를 발행한 플러그인. 호스트가 발행했으면 `None`.
    pub publisher: Option<String>,
    pub surface_id: u32,
}

/// 호스트가 추적 중인 banner 인스턴스 한 건(A3). plugin process가 죽으면 함께 제거된다.
///
/// popup 과 달리 초기 tree 가 없다 — egui-mesh 채널로만 콘텐츠를 그린다. `surface_id`
/// 는 banner 가 도킹된 스코프 surface(D1: plugin 이 소유한 surface 로만 host 가 허용).
#[derive(Debug, Clone)]
pub struct BannerInstance {
    pub plugin_id: String,
    pub banner_id: String,
    pub contribute: tasty_plugin_manifest::BannerContribute,
    /// banner 가 도킹된 surface scope 의 host surface id.
    pub surface_id: u32,
}

/// plugin → host IPC 호출 한 건. 라우팅 후 결과를 plugin에 회신해야 함.
#[derive(Debug, Clone)]
pub struct PendingPluginCall {
    /// Source process captured while collecting the IpcCall, before reload can replace it.
    pub binding: std::sync::Weak<()>,
    pub plugin_id: String,
    pub call_id: u64,
    pub method: String,
    pub params: serde_json::Value,
    pub permissions: Arc<HashSet<Permission>>,
}

/// `[[contributes.popup]]` 한 항목 + 소유 plugin id. 호스트의 popup 라우터가
/// trigger 매칭에 사용하기 위한 평탄 뷰.
#[derive(Debug, Clone)]
pub struct PluginPopupEntry {
    pub plugin_id: String,
    pub contribute: tasty_plugin_manifest::PopupContribute,
}

mod banner;
mod buffer;
mod connect;
mod events;
mod ipc_dispatch;
mod lifecycle;
mod popup;
mod pump;
mod queries;
mod reattach;
mod response;
mod retire;
mod start_status;
pub use start_status::{FailureKind, FailureReport, PluginFailure, StartState, StartStatus};
#[cfg(any(test, feature = "test-support"))]
mod test_support;
#[cfg(any(test, feature = "test-support"))]
pub use test_support::{CallerStub, NamespaceStub};

// 패키지 변경 뒤 확장 상태가 다시 계산되는지 검사한다.
#[cfg(test)]
mod tests_derived_freshness;

// 설치·활성 상태로 상속 환경변수 제거 규칙을 계산·반영하는지 검사한다.
#[cfg(test)]
mod tests_inherited_env;

// 자동 reload의 기준값·변경 감지·교체 검사.
#[cfg(test)]
mod tests_auto_reload;

// surface 범위 이벤트 팝업의 대기열 진입 검사.
#[cfg(test)]
mod tests_surface_popup;

// 원 프로세스 회수 뒤 surface 닫기의 소멸 증거 검사.
#[cfg(test)]
mod tests_retired_surface;

// 원 프로세스 회수 뒤 남은 surface 를 새 프로세스에 다시 게시하는지 검사.
#[cfg(test)]
mod tests_reattach;

// namespace 소유자가 설치 매니페스트에서 계산되는지 검사한다.
#[cfg(test)]
mod tests_namespace_table;

// plugin 주기 작업(PluginTick) 스케줄 — pump(now) 시간 주입 검증.
#[cfg(test)]
mod tests_timers;

// 메인 루프 정지 시간을 ping 무응답으로 세지 않는가(docs/dev-guide/timer-hub.md).
#[cfg(test)]
mod tests_healthcheck;

#[cfg(test)]
mod tests_lifecycle_toggle;

// 만료·취소로 이미 끝난 요청의 늦은 응답이 아무것도 다시 진행시키지 않는가.
#[cfg(test)]
mod tests_late_response;
// 재발화 hop 하한이 dispatch 송신 · 응답 · publish 도착 세 자리에 이어졌는가(docs/reference/event-catalog.md#재발행과-응답).
#[cfg(test)]
mod tests_relay_floor;
// namespace forward 가 정확히 한 번을 약속하지 않는다는 사실의 고정(docs/dev-guide/api-conventions.md#어느-경로에-걸리나--호스트가-아는-이름은-전부-안-plugin-고유-이름만-밖).
#[cfg(test)]
mod tests_forward_idempotency;
// plugin 으로 넘긴 요청의 대기 항목이 hop 마다 원 IPC 요청의 번호를 드는가(docs/architecture/ipc-server.md#느린-요청-추적).
#[cfg(test)]
mod tests_request_origin;

#[cfg(test)]
#[cfg(any(unix, windows))]
mod tests_retire;

#[cfg(test)]
mod tests_connect;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::PluginProcess;

    fn empty_waker() -> tasty_terminal::waker_factory::SharedWakerFactory {
        // headless 환경에서 PluginManager가 사용하는 waker — 실제 wake는 no-op로 충분.
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory)
    }

    /// 프로세스 실행 없이 매니페스트로 namespace 소유자 목록을 구성한다.
    /// 운영 경로와 같은 계산을 사용해 호출 검증을 확인한다.
    fn mgr_with_namespace_owner(owner: &str, prefix: &str) -> PluginManager {
        let manifest_toml = format!(
            r#"
manifest_version = 1
id = "{owner}"
name = "Namespace Owner Fixture"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[[contributes.ipc_namespace]]
prefix = "{prefix}"
"#
        );
        let manifest: tasty_plugin_manifest::Manifest =
            toml::from_str(&manifest_toml).expect("fixture manifest should parse");
        let mut mgr = PluginManager::new(empty_waker());
        mgr.set_packages_for_tests(vec![PluginPackage {
            dir: PathBuf::from("/nonexistent/namespace_owner_fixture"),
            manifest,
        }]);
        mgr
    }

    #[test]
    fn delayed_ipc_result_never_reaches_a_replacement_plugin_process() {
        let mut manager = PluginManager::new(empty_waker());
        let (old, old_rx) = PluginProcess::stub_with_request_rx("same-plugin");
        let binding = old.reply_binding();
        manager.processes.insert("same-plugin".into(), old);
        let (new, new_rx) = PluginProcess::stub_with_request_rx("same-plugin");
        let current = new.reply_binding();
        let _retired = manager.processes.insert("same-plugin".into(), new).unwrap();
        let response = || {
            tasty_ipc::protocol::JsonRpcResponse::success(
                serde_json::Value::Null,
                serde_json::json!({"done":true}),
            )
        };
        assert!(!manager.send_bound_ipc_result("same-plugin", &binding, 7, response()));
        assert!(old_rx.try_recv().is_err());
        assert!(new_rx.try_recv().is_err());
        assert!(manager.send_bound_ipc_result("same-plugin", &current, 8, response()));
        let request = new_rx.try_recv().unwrap();
        assert_eq!(request.method, crate::protocol::METHOD_IPC_RESULT);
        assert_eq!(request.params["call_id"], 8);
        assert_eq!(request.params["result"]["done"], true);
    }

    #[test]
    fn validate_namespace_call_method_not_found() {
        let mut mgr = PluginManager::new(empty_waker());
        let err = mgr
            .validate_namespace_call("nope.method", None)
            .unwrap_err();
        assert_eq!(err.0, -32601);
    }

    #[test]
    fn validate_namespace_call_local_caller_allowed_when_target_running() {
        let mut mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        // process 가짜 entry 삽입 — request 전송은 안 한다, 검증만.
        mgr.processes
            .insert("com.example.codex".into(), stub_process());
        let id = mgr
            .validate_namespace_call("codex.spawn", None)
            .expect("local caller should pass");
        assert_eq!(id, "com.example.codex");
    }

    #[test]
    fn validate_namespace_call_target_not_running() {
        let _home = crate::test_support::HomeEnvGuard::tasty_home();
        let mut mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        mgr.auto_disabled.insert("com.example.codex".into());
        let err = mgr
            .validate_namespace_call("codex.spawn", None)
            .unwrap_err();
        assert_eq!(err.0, -32002);
    }

    #[test]
    fn validate_namespace_call_self_invocation_rejected() {
        let mut mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        mgr.processes
            .insert("com.example.codex".into(), stub_process());
        let err = mgr
            .validate_namespace_call("codex.spawn", Some("com.example.codex"))
            .unwrap_err();
        assert_eq!(err.0, -32001);
        assert!(err.1.contains("its own namespace"));
    }

    #[test]
    fn validate_namespace_call_plugin_caller_without_grant_denied() {
        let mut mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        mgr.processes
            .insert("com.example.codex".into(), stub_process());
        // caller plugin은 ipc.invoke:codex 권한 없음
        mgr.set_plugin_permissions(
            "com.example.helper",
            HashSet::from([Permission::SurfaceRead]),
        );
        let err = mgr
            .validate_namespace_call("codex.spawn", Some("com.example.helper"))
            .unwrap_err();
        assert_eq!(err.0, -32001);
        assert!(err.1.contains("permission_denied"));
        assert!(err.1.contains("ipc.invoke:codex"));
    }

    #[test]
    fn validate_namespace_call_plugin_caller_with_grant_allowed() {
        let mut mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        mgr.processes
            .insert("com.example.codex".into(), stub_process());
        mgr.set_plugin_permissions(
            "com.example.helper",
            HashSet::from([Permission::IpcInvoke("codex".into())]),
        );
        let id = mgr
            .validate_namespace_call("codex.spawn", Some("com.example.helper"))
            .expect("granted caller should pass");
        assert_eq!(id, "com.example.codex");
    }

    /// validate 만 보는 테스트용 stub. PluginProcess는 process.rs의 cfg(test)
    /// 헬퍼를 위임 호출한다.
    fn stub_process() -> PluginProcess {
        PluginProcess::stub_for_test("stub")
    }

    #[cfg(unix)]
    #[test]
    fn create_shared_buffer_rejects_zero_size() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes.insert("com.example.x".into(), stub_process());
        let err = mgr
            .create_shared_buffer_for("com.example.x", 1, 0)
            .unwrap_err();
        assert!(err.contains("size must be > 0"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn create_shared_buffer_rejects_unknown_plugin() {
        let mut mgr = PluginManager::new(empty_waker());
        let err = mgr
            .create_shared_buffer_for("com.example.ghost", 1, 4096)
            .unwrap_err();
        assert!(err.contains("not running"), "got: {err}");
    }

    fn make_response(result: Option<serde_json::Value>, error: Option<&str>) -> PluginResponse {
        PluginResponse {
            id: 1,
            result,
            error: error.map(String::from),
            error_code: None,
        }
    }

    #[test]
    fn parse_hook_result_pass_on_missing_result() {
        let resp = make_response(None, None);
        assert!(matches!(parse_hook_result(&resp), HookOutcome::Pass));
    }

    #[test]
    fn parse_hook_result_pass_on_error() {
        let resp = make_response(Some(serde_json::json!({"pass": false})), Some("boom"));
        assert!(matches!(parse_hook_result(&resp), HookOutcome::Pass));
    }

    #[test]
    fn parse_hook_result_block_on_pass_false() {
        let resp = make_response(Some(serde_json::json!({"pass": false})), None);
        assert!(matches!(parse_hook_result(&resp), HookOutcome::Block));
    }

    #[test]
    fn parse_hook_result_modified_payload_takes_precedence() {
        let resp = make_response(
            Some(serde_json::json!({"modified_payload": {"x": 1}, "pass": false})),
            None,
        );
        match parse_hook_result(&resp) {
            HookOutcome::Modified(v) => assert_eq!(v, serde_json::json!({"x": 1})),
            other => panic!(
                "expected Modified, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn parse_hook_result_pass_when_modified_null() {
        let resp = make_response(Some(serde_json::json!({"modified_payload": null})), None);
        assert!(matches!(parse_hook_result(&resp), HookOutcome::Pass));
    }

    #[test]
    fn record_hook_failure_triggers_backoff_after_limit() {
        let mut mgr = PluginManager::new(empty_waker());
        let ext = "com.example.ext";
        let method = "codex.spawn";
        assert!(!mgr.is_hook_in_backoff(ext, method));
        for _ in 0..HOOK_FAIL_LIMIT {
            mgr.record_hook_failure(ext, method);
        }
        assert!(mgr.is_hook_in_backoff(ext, method));
    }

    #[test]
    fn record_hook_success_resets_counter() {
        let mut mgr = PluginManager::new(empty_waker());
        let ext = "com.example.ext";
        let method = "codex.spawn";
        mgr.record_hook_failure(ext, method);
        mgr.record_hook_failure(ext, method);
        mgr.record_hook_success(ext, method);
        // 추가로 (HOOK_FAIL_LIMIT - 1)회 실패만으로는 backoff 진입 금지.
        for _ in 0..(HOOK_FAIL_LIMIT - 1) {
            mgr.record_hook_failure(ext, method);
        }
        assert!(!mgr.is_hook_in_backoff(ext, method));
    }

    #[test]
    fn find_active_ipc_hooks_returns_none_when_no_extension() {
        let mgr = mgr_with_namespace_owner("com.example.codex", "codex");
        assert!(
            mgr.find_active_ipc_hooks("com.example.codex", "codex.spawn")
                .is_none()
        );
    }

    // plugin 요청의 왕복 대기 시간 기록을 확인한다.
    #[test]
    fn a_matched_response_records_how_long_the_host_waited() {
        let mut mgr = PluginManager::new(empty_waker());
        let stats = Arc::new(tasty_telemetry::PluginWaitStats::default());
        mgr.set_plugin_wait(stats.clone());
        mgr.pending_requests.insert(
            9,
            PendingRequest {
                kind: PendingRequestKind::Other,
                // 실제로 기다린 것이 아니라 **보낸 시각을 뒤로 밀어** 잰다 — 시험이
                // 자기 벽시계를 쓰면 부하에 따라 값이 흔들린다.
                sent_at: Instant::now() - Duration::from_millis(50),
                to: "com.example.x".into(),
                origin: None,
            },
        );

        mgr.handle_plugin_response(
            "com.example.x",
            crate::protocol::PluginResponse {
                id: 9,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );

        let s = stats.snapshot();
        assert_eq!(s.matched, 1, "매칭된 응답 하나가 한 번 세져야 한다");
        assert!(
            s.us_max >= 50_000,
            "요청 전송부터 응답까지의 시간이 기록되어야 한다: {}",
            s.us_max
        );
    }

    // pending에 없는 응답은 시작 시각을 알 수 없으므로 대기 시간 0으로 기록하지 않는다.
    #[test]
    fn an_unmatched_response_is_not_counted_as_a_wait() {
        let mut mgr = PluginManager::new(empty_waker());
        let stats = Arc::new(tasty_telemetry::PluginWaitStats::default());
        mgr.set_plugin_wait(stats.clone());

        mgr.handle_plugin_response(
            "com.example.x",
            crate::protocol::PluginResponse {
                id: 4242,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );

        assert_eq!(stats.snapshot().matched, 0);
        assert_eq!(stats.snapshot().us_mean(), None, "안 쟀으면 None 이다");
    }

    #[test]
    fn find_active_event_hooks_returns_none_when_no_extension() {
        let mgr = PluginManager::new(empty_waker());
        assert!(
            mgr.find_active_event_hooks("com.example.foo", "foo.bar")
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_shared_buffer_rejects_when_handle_listener_missing() {
        // stub mgr는 handle_listener를 bind하지 않으므로 즉시 거절되어야 한다.
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes.insert("com.example.x".into(), stub_process());
        assert!(mgr.handle_listener.is_none());
        let err = mgr
            .create_shared_buffer_for("com.example.x", 1, 4096)
            .unwrap_err();
        assert!(err.contains("handle channel not available"), "got: {err}");
    }

    /// 만료 한 건을 심고 sweep 을 돌린다. `plugin_id` 와 `times` 로 같은 plugin 에
    /// 연속 만료를 만든다.
    fn expire_namespace_calls(mgr: &mut PluginManager, plugin_id: &str, times: u32) {
        expire_namespace_calls_from(mgr, plugin_id, 100, times);
    }

    /// [`expire_namespace_calls`] 에 첫 request id 를 고르게 한 것 — 한 시험에서 만료를
    /// 두 번 쌓을 때 두 번째 묶음이 첫 묶음의 id 를 다시 쓰지 않게 한다.
    fn expire_namespace_calls_from(
        mgr: &mut PluginManager,
        plugin_id: &str,
        first_id: u64,
        times: u32,
    ) {
        for i in 0..times {
            let (tx, _rx) = mpsc::sync_channel(1);
            let id = first_id + i as u64;
            mgr.pending_requests.insert(
                id,
                PendingRequest::now(
                    plugin_id,
                    PendingRequestKind::NamespaceInvoke {
                        plugin_id: plugin_id.into(),
                        response_tx: tx,
                        original_id: serde_json::json!(id),
                        deadline: Instant::now() - Duration::from_secs(1),
                    },
                ),
            );
            mgr.sweep_expired_requests(Instant::now());
        }
    }

    /// 최근 pong이 있어도 namespace 연속 만료가 쌓이면 재시작 대상으로 삼는지 확인한다.
    #[test]
    fn a_plugin_that_only_expires_namespace_calls_is_restarted() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());

        expire_namespace_calls(
            &mut mgr,
            "com.example.silent",
            NAMESPACE_EXPIRY_RESTART_LIMIT - 1,
        );
        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            mgr.processes.contains_key("com.example.silent"),
            "상한 전인데 거둬졌다"
        );

        expire_namespace_calls(&mut mgr, "com.example.silent", 1);
        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            !mgr.processes.contains_key("com.example.silent"),
            "연속 만료가 상한에 닿았는데 재시작이 안 걸렸다"
        );
    }

    /// 재시작 시 등록 상태도 지워 새 hello가 권한과 설정 페이지를 다시 등록할 수 있게 한다.
    #[test]
    fn a_restarted_plugin_is_registered_again_on_its_next_hello() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());
        mgr.registered_plugins.insert("com.example.silent".into());

        expire_namespace_calls(
            &mut mgr,
            "com.example.silent",
            NAMESPACE_EXPIRY_RESTART_LIMIT,
        );
        mgr.restart_unresponsive_plugins(Instant::now());

        assert!(!mgr.processes.contains_key("com.example.silent"));
        assert!(
            !mgr.registered_plugins.contains("com.example.silent"),
            "재시작이 등록 게이트를 안 풀었다 — 다음 hello 가 권한을 다시 못 받는다"
        );
    }

    /// 재시작 후 만료 횟수와 오래된 ID를 지워 같은 이유로 새 프로세스까지 재시작하지 않는다.
    #[test]
    fn a_plugin_restarted_by_the_expiry_streak_is_restarted_once() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());

        expire_namespace_calls(
            &mut mgr,
            "com.example.silent",
            NAMESPACE_EXPIRY_RESTART_LIMIT,
        );
        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            !mgr.processes.contains_key("com.example.silent"),
            "연속 만료가 상한에 닿았는데 재시작이 안 걸렸다"
        );

        // 새 프로세스가 떴다 — 재시작 경로가 스스로 띄우지 않으므로 시험이 대신 둔다.
        mgr.processes
            .insert("com.example.silent".into(), stub_process());
        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            mgr.processes.contains_key("com.example.silent"),
            "재시작된 새 프로세스가 옛 계수로 또 재시작됐다 — 중복 소비"
        );
        assert!(
            !mgr.expired_namespace_calls
                .contains_key("com.example.silent"),
            "옛 프로세스의 거둔 id 가 새 프로세스 앞으로 남았다"
        );
    }

    /// 재시작된 plugin의 요청은 caller나 deadline이 없어도 제거한다.
    /// 다른 plugin에 보낸 요청은 유지한다.
    #[test]
    fn requests_sent_to_a_restarted_plugin_are_reclaimed() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());
        mgr.pending_requests.insert(
            900,
            PendingRequest::now(
                "com.example.silent",
                PendingRequestKind::SurfaceRestore {
                    surface_id: 3,
                    binding: crate::host_cmd::SurfaceHandles {
                        display_name: Arc::default(),
                        snapshot_cache: Arc::default(),
                    }
                    .binding(),
                },
            ),
        );
        mgr.pending_requests.insert(
            901,
            PendingRequest::now("com.example.other", PendingRequestKind::Other),
        );
        assert!(mgr.has_pending_surface_restores());

        expire_namespace_calls(
            &mut mgr,
            "com.example.silent",
            NAMESPACE_EXPIRY_RESTART_LIMIT,
        );
        mgr.restart_unresponsive_plugins(Instant::now());

        assert!(
            !mgr.has_pending_surface_restores(),
            "재시작된 plugin 에게 보낸 surface.restore 가 남았다"
        );
        assert!(!mgr.pending_requests.contains_key(&900));
        assert!(
            mgr.pending_requests.contains_key(&901),
            "다른 plugin 에게 간 요청까지 거뒀다"
        );
    }

    /// namespace 응답을 받으면 연속 만료 횟수를 지운다.
    #[test]
    fn a_namespace_answer_clears_the_expiry_streak() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.slow".into(), stub_process());

        expire_namespace_calls(
            &mut mgr,
            "com.example.slow",
            NAMESPACE_EXPIRY_RESTART_LIMIT - 1,
        );
        assert_eq!(
            mgr.namespace_expiries.get("com.example.slow"),
            Some(&(NAMESPACE_EXPIRY_RESTART_LIMIT - 1))
        );

        // 늦게나마 하나가 답했다.
        let (tx, _rx) = mpsc::sync_channel(1);
        mgr.pending_requests.insert(
            7,
            PendingRequest::now(
                "com.example.slow",
                PendingRequestKind::NamespaceInvoke {
                    plugin_id: "com.example.slow".into(),
                    response_tx: tx,
                    original_id: serde_json::json!(7),
                    deadline: Instant::now() + NAMESPACE_CALL_TIMEOUT,
                },
            ),
        );
        mgr.handle_plugin_response(
            "com.example.slow",
            crate::protocol::PluginResponse {
                id: 7,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );
        assert!(
            !mgr.namespace_expiries.contains_key("com.example.slow"),
            "응답이 왔는데 연속 만료 계수가 남았다"
        );

        expire_namespace_calls(&mut mgr, "com.example.slow", 1);
        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            mgr.processes.contains_key("com.example.slow"),
            "계수가 리셋됐는데 재시작이 걸렸다"
        );
    }

    /// 만료된 요청의 늦은 응답도 기록된 ID와 일치하면 연속 만료 횟수를 지운다.
    #[test]
    fn a_late_namespace_answer_also_clears_the_expiry_streak() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.slow".into(), stub_process());

        expire_namespace_calls(&mut mgr, "com.example.slow", NAMESPACE_EXPIRY_RESTART_LIMIT);
        assert_eq!(
            mgr.namespace_expiries.get("com.example.slow"),
            Some(&NAMESPACE_EXPIRY_RESTART_LIMIT)
        );
        assert!(
            !mgr.pending_requests.contains_key(&100),
            "sweep 이 거두지 않았으면 이 시험은 늦은 응답을 재는 것이 아니다"
        );

        mgr.handle_plugin_response(
            "com.example.slow",
            crate::protocol::PluginResponse {
                id: 100,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );
        assert!(
            !mgr.namespace_expiries.contains_key("com.example.slow"),
            "늦은 namespace 응답이 계수를 안 지웠다"
        );

        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            mgr.processes.contains_key("com.example.slow"),
            "답한 plugin 이 재시작됐다"
        );
    }

    /// 같은 만료 ID의 응답은 한 번만 처리하고 다른 만료 ID는 남긴다.
    #[test]
    fn a_reaped_namespace_id_clears_the_streak_exactly_once() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.echo".into(), stub_process());
        let late = |mgr: &mut PluginManager, id: u64| {
            mgr.handle_plugin_response(
                "com.example.echo",
                crate::protocol::PluginResponse {
                    id,
                    result: Some(serde_json::json!({})),
                    error: None,
                    error_code: None,
                },
            );
        };

        // 상한에 의한 ID 제거와 혼동하지 않도록 상한보다 적게 기록한다.
        expire_namespace_calls_from(&mut mgr, "com.example.echo", 100, 1);
        late(&mut mgr, 100);
        assert!(
            !mgr.namespace_expiries.contains_key("com.example.echo"),
            "처음 온 늦은 응답이 계수를 안 지웠다 — 소비 누락"
        );

        expire_namespace_calls_from(
            &mut mgr,
            "com.example.echo",
            200,
            NAMESPACE_EXPIRY_RESTART_LIMIT - 1,
        );
        late(&mut mgr, 100);
        assert_eq!(
            mgr.namespace_expiries.get("com.example.echo"),
            Some(&(NAMESPACE_EXPIRY_RESTART_LIMIT - 1)),
            "이미 소비된 id 의 재전송이 계수를 또 지웠다 — 중복 소비"
        );

        // 200 을 소비해도 같은 묶음의 201 은 남아야 한다.
        late(&mut mgr, 200);
        expire_namespace_calls_from(&mut mgr, "com.example.echo", 300, 1);
        late(&mut mgr, 201);
        assert!(
            !mgr.namespace_expiries.contains_key("com.example.echo"),
            "한 id 를 소비하면서 다른 거둬진 id 까지 지웠다"
        );

        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            mgr.processes.contains_key("com.example.echo"),
            "답한 plugin 이 재시작됐다"
        );
    }

    /// namespace가 아닌 응답과 기록되지 않은 ID는 연속 만료 횟수를 지우지 않는다.
    #[test]
    fn only_a_namespace_answer_clears_the_expiry_streak() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());
        expire_namespace_calls(
            &mut mgr,
            "com.example.silent",
            NAMESPACE_EXPIRY_RESTART_LIMIT,
        );

        // (1) pending 이 있는 non-namespace 응답.
        mgr.pending_requests.insert(
            500,
            PendingRequest::now("com.example.silent", PendingRequestKind::Other),
        );
        mgr.handle_plugin_response(
            "com.example.silent",
            crate::protocol::PluginResponse {
                id: 500,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );
        assert_eq!(
            mgr.namespace_expiries.get("com.example.silent"),
            Some(&NAMESPACE_EXPIRY_RESTART_LIMIT),
            "namespace 가 아닌 응답이 계수를 지웠다"
        );

        // (2) 기억된 적 없는 id 의 늦은 응답.
        mgr.handle_plugin_response(
            "com.example.silent",
            crate::protocol::PluginResponse {
                id: 9999,
                result: Some(serde_json::json!({})),
                error: None,
                error_code: None,
            },
        );
        assert_eq!(
            mgr.namespace_expiries.get("com.example.silent"),
            Some(&NAMESPACE_EXPIRY_RESTART_LIMIT),
            "namespace 만료로 기억된 적 없는 id 가 계수를 지웠다"
        );

        mgr.restart_unresponsive_plugins(Instant::now());
        assert!(
            !mgr.processes.contains_key("com.example.silent"),
            "계수가 상한인데 재시작이 안 걸렸다"
        );
    }

    /// 호스트가 정한 오류 코드가 plugin 호출자에게도 그대로 전달되는지 확인한다.
    #[test]
    fn a_host_originated_error_code_reaches_a_plugin_caller() {
        let mut mgr = PluginManager::new(empty_waker());
        let (proc, rx) = PluginProcess::stub_with_request_rx("com.example.caller");
        let binding = proc.reply_binding();
        mgr.processes.insert("com.example.caller".into(), proc);
        mgr.send_final_error(
            FinalCaller::Plugin {
                caller_plugin_id: "com.example.caller".into(),
                binding,
                call_id: 9,
            },
            -32004,
            "target did not answer".into(),
        );
        let req = rx
            .try_recv()
            .expect("caller plugin 에 ipc.result 가 가야 한다");
        assert_eq!(req.method, crate::protocol::METHOD_IPC_RESULT);
        let parsed: crate::protocol::IpcCallResult =
            serde_json::from_value(req.params).expect("ipc.result params");
        assert_eq!(parsed.call_id, 9);
        assert_eq!(
            parsed.error_code,
            Some(-32004),
            "호스트가 낸 코드가 plugin 갈래에서 버려졌다"
        );
    }

    /// deadline과 현재 시각을 받아 pending 유지 여부와 응답을 확인한다.
    fn sweep_one_namespace_invoke(
        now: Instant,
        deadline: Instant,
    ) -> (bool, Option<JsonRpcResponse>) {
        let mut mgr = PluginManager::new(empty_waker());
        let (tx, rx) = mpsc::sync_channel(1);
        mgr.pending_requests.insert(
            7,
            PendingRequest::now(
                "com.example.silent",
                PendingRequestKind::NamespaceInvoke {
                    plugin_id: "com.example.silent".into(),
                    response_tx: tx,
                    original_id: serde_json::json!(42),
                    deadline,
                },
            ),
        );
        mgr.sweep_expired_requests(now);
        (mgr.pending_requests.contains_key(&7), rx.try_recv().ok())
    }

    /// 같은 deadline의 앞뒤 시각을 주입해 만료 판정이 달라지는지 확인한다.
    #[test]
    fn the_injected_now_is_what_decides_expiry() {
        let deadline = Instant::now() + Duration::from_secs(3600);
        let (before, resp_before) =
            sweep_one_namespace_invoke(deadline - Duration::from_secs(1), deadline);
        assert!(before, "deadline 이전 시각인데 만료됐다");
        assert!(resp_before.is_none());
        let (after, resp_after) =
            sweep_one_namespace_invoke(deadline + Duration::from_secs(1), deadline);
        assert!(!after, "deadline 이후 시각인데 pending 이 남았다");
        assert!(resp_after.is_some(), "만료 시 caller 에 회신이 가야 한다");
    }

    #[test]
    fn expired_namespace_invoke_answers_its_caller_and_leaves_pending() {
        let (still_pending, resp) =
            sweep_one_namespace_invoke(Instant::now(), Instant::now() - Duration::from_secs(1));
        assert!(!still_pending, "만료된 pending 이 남았다");
        let resp = resp.expect("만료 시 caller 에 회신이 가야 한다");
        assert_eq!(resp.id, serde_json::json!(42));
        let err = resp.error.expect("결과가 아니라 오류여야 한다");
        assert_eq!(err.code, -32004);
        assert!(
            err.message.contains("com.example.silent") && err.message.contains("did not answer"),
            "got: {}",
            err.message
        );
    }

    /// debug hook 직접 호출의 기한 전후에서 응답과 대기 항목 정리를 확인한다.
    #[cfg(debug_assertions)]
    #[test]
    fn an_expired_debug_hook_invoke_answers_its_caller() {
        let mut mgr = PluginManager::new(empty_waker());
        let (tx, rx) = mpsc::sync_channel(1);
        let deadline = Instant::now() + DEBUG_HOOK_INVOKE_TIMEOUT;
        mgr.pending_requests.insert(
            11,
            PendingRequest::now(
                "com.example.ext",
                PendingRequestKind::DebugExtensionInvokeHook {
                    response_tx: tx,
                    original_id: serde_json::json!("dbg"),
                    deadline,
                },
            ),
        );
        mgr.sweep_expired_requests(deadline - Duration::from_millis(1));
        assert!(
            mgr.pending_requests.contains_key(&11),
            "상한 전인데 거둬졌다"
        );
        assert!(rx.try_recv().is_err(), "상한 전에 회신이 갔다");

        mgr.sweep_expired_requests(deadline);
        assert!(
            !mgr.pending_requests.contains_key(&11),
            "만료가 안 거둬졌다"
        );
        let resp = rx.try_recv().expect("만료 시 caller 에 회신이 가야 한다");
        let err = resp.error.expect("결과가 아니라 오류여야 한다");
        assert_eq!(err.code, -32004);
        assert!(
            err.message.contains("debug hook invoke"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn unexpired_namespace_invoke_is_left_alone() {
        // 미래 deadline은 만료되지 않아야 한다. 시간 단위 변환으로 상한이 0이 되는 경우도 검출한다.
        let (still_pending, resp) =
            sweep_one_namespace_invoke(Instant::now(), Instant::now() + NAMESPACE_CALL_TIMEOUT);
        assert!(still_pending, "아직 만료 전인데 pending 이 사라졌다");
        assert!(resp.is_none(), "만료 전에 caller 에 회신이 갔다");
    }
}

#[cfg(test)]
mod tests_namespace_start;
