//! Plugin 생명주기 매니저.
//!
//! 호스트의 부팅 시 한 번 만들어지고, `App`이 유일한 인스턴스를 보유한다.
//! - 부팅 시 `discover_and_start()`로 `~/.tasty/plugins/`를 스캔하여 활성 plugin 모두 spawn
//! - 매 메인 루프 tick에서 `pump()` 호출 → plugin 알림 처리 + 헬스체크 + 재시작
//! - 종료 시 `shutdown_all()`

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

/// host popup(`PopupManager`)과 plugin popup(`PluginManager::popup_instances`) 사이의
/// z-order 를 판정하는 유일한 공유 기준. 두 매니저가 서로 다른 크레이트에 있어 자료구조를
/// 통합하지 않는 대신, 열리거나 클릭/포커스될 때마다 이 전역 단조증가 순번을 하나씩
/// 받아 각자의 상태(`PopupState.z_seq` / `PopupInstance.z_seq`)에 기록한다 — 값이 큰 쪽이
/// 나중에 열리거나 클릭된 것이므로 항상 위에 그려진다(`docs/design/systems/popup.md` 규칙 7).
static NEXT_POPUP_Z_SEQ: AtomicU64 = AtomicU64::new(1);

/// 다음 z-order 순번을 발급한다. host popup 오픈/포커스와 plugin popup 오픈/클릭 양쪽에서
/// 호출된다 — 값 자체의 의미는 없고, 오직 다른 값과의 대소 비교(더 큰 쪽이 위)에만 쓰인다.
pub fn next_popup_z_seq() -> u64 {
    NEXT_POPUP_Z_SEQ.fetch_add(1, Ordering::Relaxed)
}

pub(super) const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(60);
pub(super) const PING_INTERVAL: Duration = Duration::from_secs(15);
/// namespace 호출 하나가 응답 없이 pending 에 남을 수 있는 상한.
///
/// hook 의 deadline 은 매니페스트가 선언한 `timeout_ms` 에서 오지만(상한 1 초)
/// namespace 호출에는 그런 선언이 없어 값을 여기서 정한다. 이 값은
/// `HEALTHCHECK_TIMEOUT + PING_INTERVAL` 을 **넘겨야** 한다 — 프로세스가 죽거나
/// 굳은 plugin 은 이미 그 상한 안에 healthcheck 가 거두고
/// (`restart_unresponsive_plugins` → `cancel_pending_namespace_calls`) 그 경로가
/// `-32004` 를 돌려주기 때문이다. 더 짧게 잡으면 이미 처리되는 그 경우를 앞질러
/// 회신 시점을 바꾼다. 그래서 그 상한의 두 배로 두고, 여기 걸리는 것은 healthcheck
/// 가 볼 수 없는 경우 — ping 에는 답하면서 이 호출 하나만 영영 안 돌려주는 plugin —
/// 뿐이게 한다.
pub(super) const NAMESPACE_CALL_TIMEOUT: Duration =
    Duration::from_secs(2 * (HEALTHCHECK_TIMEOUT.as_secs() + PING_INTERVAL.as_secs()));
/// debug 한정 `debug.extension.invoke_hook` 한 건의 응답 상한.
///
/// 값을 새로 고르지 않고 매니페스트 검증의 상한에서 가져온다 —
/// [`HOOK_TIMEOUT_MS_MAX`](tasty_plugin_manifest::HOOK_TIMEOUT_MS_MAX) 는 선언된
/// hook 의 `timeout_ms` 가 넘을 수 없는 값이므로, 그보다 더 기다리는 것은 **실제
/// hook 이 할 수 없는 일을 기다리는 것**이다. 이 경로에는 매니페스트 선언이
/// 없어(호출자가 ext_id·phase·payload 를 직접 준다) 값을 어디선가 정해야 하는데,
/// 같은 extension 이 정상 경로에서 받는 상한과 같게 두는 것이 유일하게 파생인 값이다.
#[cfg(debug_assertions)]
pub(super) const DEBUG_HOOK_INVOKE_TIMEOUT: Duration =
    Duration::from_millis(tasty_plugin_manifest::HOOK_TIMEOUT_MS_MAX as u64);
/// 한 plugin 의 namespace 호출이 **연달아** 만료될 수 있는 횟수의 상한. 여기 닿으면
/// 그 plugin 을 healthcheck 무응답과 같은 경로로 재시작한다.
///
/// 만료 한 건은 caller 에 대한 답이지 plugin 에 대한 판정이 아니다 — 한 번은 느렸을
/// 수 있다. 그러나 그 plugin 의 namespace 응답이 **하나도** 안 오는 채로 만료만 쌓이면
/// 호출마다 [`NAMESPACE_CALL_TIMEOUT`] 을 태우고 남는 것은 경고 로그뿐이고, 그 상태는
/// 스스로 끝나지 않는다 — 프로세스는 ping 에 답하므로 healthcheck 가 원리적으로 못 본다.
///
/// **처방이 hook 의 backoff 와 다른 이유**: hook 은 선택적이라 우회가 곧 정상 동작이고,
/// 그래서 실패가 쌓이면 잠시 안 부르는 것이 답이다([`HOOK_FAIL_BACKOFF`]). namespace
/// 호출에는 우회할 대상이 없다 — 같은 처방을 이식하면 "시도조차 않고 즉시 실패" 가 되어
/// 회복한 plugin 이 backoff 동안 **도달 불가**가 된다. 재시작은 그 반대다: 그 자리에서
/// pending 을 전부 거두고(`cancel_pending_namespace_calls`) plugin 을 다시 띄우므로,
/// 다음 호출은 기다림 없이 건강한 프로세스에 닿는다.
///
/// 값 3 은 [`HOOK_FAIL_LIMIT`] 와 같다 — 연속을 우연과 가르는 최소 수. 파생이 아니다.
pub(super) const NAMESPACE_EXPIRY_RESTART_LIMIT: u32 = 3;
pub(super) const RESTART_FAILURE_WINDOW: Duration = Duration::from_secs(10);
pub(super) const RESTART_FAILURE_LIMIT: usize = 3;
/// plugin 하나에 주는 graceful 종료 기회. 초과하면 force kill 한다. 종료 전체
/// (`shutdown_all`)는 이 값을 plugin 마다 직렬로 더하지 않고 겹쳐서 소비하므로,
/// plugin 이 몇 개든 총 대기는 이 값으로 수렴한다.
pub(super) const PLUGIN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
/// H — auto-reload polling 간격. pump tick 안의 자연 debounce — 2초 내
/// 발생한 연속 mtime 변경은 한 번의 swap 으로 흡수된다.
pub(super) const AUTO_RELOAD_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// RssSurge 이상탐지(`docs/features/telemetry/index.md`) — plugin RSS sampling
/// 주기. 너무 짧으면
/// sysinfo 호출 비용이 매 tick 마다 누적되고, 너무 길면 5-샘플 sliding
/// window(`RSS_SURGE_MIN_SAMPLES`)가 실제 급증을 늦게 잡는다.
pub(super) const RSS_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);

/// plugin manager 가 자기 [`TimerHub`](tasty_timer::TimerHub) 에 등록하는 주기 작업 키.
///
/// 이 크레이트는 호스트 `App` 을 모르므로 본체 허브에 직접 등록할 수 없다 — 대신
/// 자기 허브를 소유하고 [`PluginManager::next_deadline`] 만 노출한다. 호스트는 그
/// 값을 자기 데드라인과 `min` 으로 합성한다(`docs/dev-guide/timer-hub.md`
/// "계층을 넘는 허브 합성").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginTick {
    /// `PING_INTERVAL` 주기 ping 송신 + 무응답 plugin 재시작 판정.
    ///
    /// healthcheck 를 별도 tick 으로 두지 않고 여기 합승시켰다 — `HEALTHCHECK_TIMEOUT`
    /// 은 인터벌이 아니라 "마지막 pong 이후 경과" 데드라인 비교라 검사 자체는 아무
    /// tick 에서나 할 수 있고, ping 을 보내는 tick 이 곧 응답을 기대하는 tick 이라
    /// 판정 시점으로 자연스럽다. 결과적으로 **비응답 검출 상한은
    /// `HEALTHCHECK_TIMEOUT + PING_INTERVAL` = 75 초**다(프로세스가 실제로 죽는
    /// 경우는 이 경로가 아니라 event 채널 Disconnected 로 즉시 잡힌다).
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
        /// 이 호출을 낳은 IPC 요청의 호스트 번호. 회신처와 한 몸으로 사슬(pre-hook → target →
        /// post-hook)을 따라가므로, 다음 hop 을 대기 표에 넣는 자리가 여기서 읽어
        /// [`PendingRequest::origin`] 에 복사한다. 번호를 모르는 호출이면 `None` — IPC 큐를 안 지난
        /// 호출과, IPC `file_handler.dispatch` 처럼 큐를 지났지만 파일 핸들러 큐에서 번호를 잃은 호출.
        origin: Option<tasty_ipc::server::RequestSeq>,
    },
    Plugin {
        caller_plugin_id: String,
        call_id: u64,
    },
}

impl FinalCaller {
    /// 이 회신처를 낳은 IPC 요청의 호스트 번호. plugin 이 부른 호출은 호스트 IPC 큐를 안
    /// 지나 번호가 없다(ADR-0436).
    pub(super) fn origin(&self) -> Option<tasty_ipc::server::RequestSeq> {
        match self {
            FinalCaller::Local { origin, .. } => *origin,
            FinalCaller::Plugin { .. } => None,
        }
    }
}

/// 응답을 기다리는 host→plugin request 하나 — **무엇을 기다리는지와 언제 보냈는지**.
///
/// `sent_at` 이 여기 붙는 이유는 이 맵이 요청의 수명을 이미 소유하기 때문이다. 별도
/// 맵에 시각을 두면 응답 없이 사라지는 요청(취소 · deadline 만료 · plugin 종료)마다
/// 두 맵을 같이 지워야 하고, 한 자리만 빠뜨려도 그 맵이 프로세스 수명 동안 자란다.
pub(super) struct PendingRequest {
    pub(super) kind: PendingRequestKind,
    /// 보낸 시각. 응답이 매칭될 때 왕복 대기 시간으로 접힌다.
    pub(super) sent_at: Instant,
    /// 이 요청을 **받은** plugin — 응답을 줄 쪽이다.
    ///
    /// 변종 가운데 절반(`SurfaceCreate` · `SurfaceRestore` · `CommandInvoke` · `PopupOpen`
    /// · `Other`)은 이 값을 안 든다. 그래서 그 plugin 이 치워질 때 그 항목들을 찾을 길이
    /// 없었고, 새 프로세스는 새 id 를 쓰므로 **영영 매칭되지 않는 항목**이 프로세스 수명
    /// 동안 남았다 — 남은 `SurfaceRestore` 는 `has_pending_surface_restores` 까지 참으로
    /// 묶는다. 변종마다 칸을 더하는 대신 여기 하나로 둔다: 받는 쪽은 모든 요청에 있다.
    pub(super) to: String,
    /// 이 plugin 요청을 낳은 **IPC 요청의 호스트 번호**([`tasty_ipc::server::RequestSeq`]).
    ///
    /// IPC 요청을 plugin namespace 로 넘긴 것이면 그 요청의 번호이고, pre/post hook 사슬의 다음
    /// hop 도 같은 값이다 — 사슬이 들고 가는 [`FinalCaller::origin`] 에서 복사하므로 사슬 전체가
    /// 원 요청 하나를 가리킨다. 번호를 모르는 plugin 요청은 `None` 이다 — IPC 요청에서 오지 않은
    /// 것(event.dispatch · surface · plugin 이 부른 namespace 등)과, IPC `file_handler.dispatch` 가
    /// 파일 핸들러 큐를 거쳐 넘긴 것(큐에 옮겨지는 사이 번호가 떨어진다, ADR-0436).
    ///
    /// `to` 와 같은 이유로 변종이 아니라 여기 칸 하나다 — 어느 변종이 이 값을 가질 수 있는지는
    /// 넘기는 쪽이 정하고, 응답·만료·취소는 변종과 무관하게 이 칸을 읽는다. plugin 에게는 안
    /// 간다(ADR-0436): 호스트 쪽 대응표이고, 그 대응을 잇는 것이 호스트 req_id 다.
    pub(super) origin: Option<tasty_ipc::server::RequestSeq>,
}

impl PendingRequest {
    /// `to` 에게 지금 보냈다. `Instant::now()` 를 쓰는 것은 이 크레이트의 기존 관례다 —
    /// `Clock` port 는 본 바이너리의 `Core` 에 있고 여기서는 안 보인다.
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
    },
    SurfaceRestore {
        surface_id: u32,
    },
    /// 단계 G: 단축키 매칭으로 plugin command가 트리거된 경우. 응답은
    /// SurfaceResult 형태로 surface display_name 을 갱신할 수 있다.
    CommandInvoke {
        surface_id: u32,
    },
    /// popup.open IPC 응답 대기. 응답은 [`PopupOpenResult`].
    PopupOpen {
        instance_id: u64,
    },
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
        /// hook 응답이 도착해야 하는 시각. 지나면 caller 에 오류로 회신하고 버린다.
        /// 이 변종만 `response_tx` 를 들고 deadline 이 없었고, 그러면 extension 이
        /// 삼킨 호출 하나가 local CLI 를 영영 세운다 — 선언된 hook 과 달리
        /// 매니페스트가 정해 주는 `timeout_ms` 가 없어 값이 안 붙어 있었던 것이지
        /// 기다려야 할 이유가 있었던 것이 아니다.
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
}

/// egui-mesh surface 의 최근 수신 mesh frame 메타 (A1-S3 수신 라우팅 골격).
///
/// plugin 이 [`tasty_plugin_protocol::PluginEvent::PaintFrame`] 를 보낼 때마다
/// `pump` 가 갱신한다. 렌더 prepare(A1-S5)가 `buffer_id` 로 [`PluginManager::plugin_buffer`]
/// 를 lookup → footer Acquire-load → `mesh_wire::decode_paint` 의 출발점으로 읽는다.
/// 본체(mesh 바이트)는 shared buffer 안에 있고, 이 구조체는 메타만 운반한다.
#[derive(Debug, Clone)]
pub struct EguiMeshFrame {
    /// buffer lookup 에 필요한 소유 plugin id.
    pub plugin_id: String,
    /// mesh POD 바이트가 들어있는 shared buffer.
    pub buffer_id: SharedBufferId,
    /// plugin 이 commit 한 footer generation. host 는 마지막 합성 generation 과
    /// 비교해 변하지 않았으면 재합성을 건너뛴다.
    pub generation: u64,
    /// plugin 렌더 코어의 송신 frame 단조 시퀀스(1부터, buffer 재생성과 무관).
    /// 렌더 prepare 가 `frame_seq == last + 1` 로 textures_delta 체인 연속성을 검증한다.
    /// 구버전 plugin 은 0 → 항상 체인 단절로 취급된다.
    pub frame_seq: u64,
    /// 이 frame 의 textures_delta 가 plugin 의 전체 텍스처 상태를 full image 로
    /// 담고 있는가. true 면 체인 연속성과 무관하게 수락하고 텍스처 상태를 리셋한다.
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
}

pub struct PluginManager {
    packages: Vec<PluginPackage>,
    /// trust gate 에서 거부된 plugin 들 (서명 미신뢰/검증 실패/권한 변경).
    /// `refresh_packages` 가 `packages` 와 함께 갱신한다. UI "확인 필요" 탭 +
    /// 사이드바 경고 배지가 소비. debug 빌드는 trust gate 우회라 항상 비어 있다.
    pub rejected: Vec<crate::discovery::RejectedPlugin>,
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
    /// H — auto-reload: plugin id → 마지막 관측한 entry binary mtime.
    pub(super) plugin_binary_mtimes: HashMap<String, SystemTime>,
    /// H — auto-reload: plugin id → 마지막 관측한 manifest version.
    pub(super) plugin_manifest_versions: HashMap<String, String>,
    /// H — auto-reload 활성 여부. `TASTY_PLUGIN_AUTO_RELOAD` env 로 결정.
    /// false 이면 `PluginTick::AutoReload` 이 등록되지 않아 cost 0.
    pub(super) auto_reload_enabled: bool,
    /// hello 받은 plugin의 surface_kinds를 등록하기 위한 registry 핸들. None이면
    /// registry 등록 동작이 비활성 (헤드리스/테스트).
    pub surface_registry: Option<Arc<dyn SurfaceRegistry>>,
    /// 이미 registry에 등록된 plugin id (hello를 여러 번 받아도 1회만 등록).
    pub registered_plugins: std::collections::HashSet<String>,
    /// registry create/restore closure가 새 RemoteSurface 등록을 보내는 채널.
    pub host_cmd_tx: Sender<HostCmd>,
    pub(super) host_cmd_rx: Receiver<HostCmd>,
    /// surface_id → RemoteSurface handle. 라이프사이클 동안 유지.
    pub(super) surfaces: HashMap<u32, RemoteSurfaceEntry>,
    /// host → plugin 요청 ID → 종류. 응답 수신 시 후처리 dispatch용.
    pub(super) pending_requests: HashMap<u64, PendingRequest>,
    /// host→plugin 왕복 대기 게이지. 호스트가 주입한다 — 이 크레이트는 그것을
    /// 소유하지 않고 올리기만 한다.
    ///
    /// `Option` 인 이유는 주입이 없는 구성이 실재하기 때문이다(이 크레이트의 단위
    /// 시험, 그리고 본 바이너리의 plugin_bridge 시험이 stub registry 로 매니저를
    /// 세운다). 그때 `None` 이면 **아무것도 안 센다** — 0 을 쌓지 않으므로 읽는 쪽이
    /// "안 쟀다" 와 "기다림이 없었다" 를 그대로 가른다.
    plugin_wait: Option<Arc<tasty_telemetry::PluginWaitStats>>,
    /// 느린 요청 링(ADR-0436). 원 IPC 요청 번호를 든 대기 항목이 끝날 때(응답 · 만료 · 취소)
    /// 그 hop 을 원 요청의 줄에 붙인다. `plugin_wait` 과 같은 이유로 `Option` 이고, `None` 이면
    /// 아무것도 안 남긴다.
    slow_requests: Option<Arc<tasty_telemetry::SlowRequestLog>>,
    /// plugin 채널 바이트 장부. 운영 경로는 **프로세스 하나에 하나**다
    /// (`ChannelLedger::process_wide`) — 창마다 매니저를 세워도 합계의 축은 메모리이고
    /// 메모리는 하나다. 이 매니저가 띄우는 모든 plugin 프로세스의 세 채널이 여기에 올라간다.
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
    /// plugin 이 매니페스트로 선언한 `[[contributes.settings_pages]]` sub-page 일람.
    /// plugin hello/manifest 수신 시 등록되고, disable / 재시작 시 정리된다.
    /// 설정 모달의 sub-tab 합성은 본 registry 를 순회 (Step 5).
    pub settings_pages: crate::settings_registry::SettingsPageRegistry,
    /// plugin이 매니페스트로 선언한 IPC namespace prefix 일람. **설치된 매니페스트에서
    /// 유도되며**(ADR-0173) 실행 여부와 무관하다 — 호스트 IPC dispatcher가 namespace
    /// 메서드를 어느 plugin에 forward할지 해결할 때 조회한다. "누가 그 이름의 주인인가"
    /// 와 "지금 떠 있는가" 는 다른 물음이고, 뒤엣것은 `processes` 가 답한다(안 떠 있으면
    /// `-32002`). 이 표는 `packages` 에서 유도되므로 `packages` 를 바꾸는 자리는
    /// [`PluginManager::refresh_packages`] 를 거쳐야 한다.
    ///
    /// **`tasty-ipc` 가 부팅 때 이 `Arc` 를 그대로 받는다** — 사본이 아니라 같은 표다.
    /// 예전에는 `method_meta` 가 자기 `HashMap` 미러를 따로 들었고, 갱신을 한쪽만 하는
    /// 결함이 실제로 났다. 표가 하나면 그 결함이 존재할 자리가 없다.
    ipc_namespaces: Arc<RwLock<IpcNamespaceRegistry>>,
    /// plugin id → (buffer id → 매핑 영역). 호스트가 `host.shared_buffer.create`로
    /// 발급한 영역의 매핑 유지(=OS region keep-alive)와 dirty 수신 시 lookup용.
    /// plugin process가 종료/재시작되면 해당 plugin 슬롯이 통째로 drop되어
    /// 매핑이 해제된다.
    pub(super) plugin_buffers: HashMap<String, HashMap<SharedBufferId, SharedMemory>>,
    /// 호스트 전체에서 단조 증가하는 shared buffer id. plugin 간 충돌 회피 + 디버그
    /// 추적을 단순화하기 위해 글로벌 카운터로 둔다.
    pub(super) next_buffer_id: AtomicU64,
    /// egui-mesh surface_id → 최근 paint_frame 메타 (A1-S3). plugin 의 `PaintFrame`
    /// 알림마다 갱신되고, 렌더 prepare(A1-S5)가 buffer lookup + 디코드 출발점으로 읽는다.
    /// plugin process 가 종료/재시작되면 해당 plugin 의 엔트리를 정리한다 (stale buffer 참조 방지).
    pub(super) egui_mesh_frames: HashMap<u32, EguiMeshFrame>,
    /// egui-mesh popup instance_id → 최근 paint_frame 메타 (A2). plugin 의
    /// `PopupPaintFrame` 알림마다 갱신되고, 호스트 popup 합성기가 instance_id 로
    /// lookup 한다. popup 이 닫히거나 plugin 이 종료되면 해당 엔트리를 정리한다.
    pub(super) popup_mesh_frames: HashMap<u64, EguiMeshFrame>,
    /// Plugin extension 상태 추적. `[extends]` 블록을 선언한 plugin들의
    /// active/pending/disabled/conflict 상태를 보관한다. PR 4/5에서 event/IPC
    /// hook dispatch 시 `active_extension_for_target`을 조회한다.
    extensions: super::extension_registry::ExtensionRegistry,
    /// (ext_id, method) 단위 hook 실패 추적. 3회 연속 실패하면 60초간 backoff.
    pub(super) hook_failures: HashMap<(String, String), HookFailureState>,
    /// plugin 별 **연속** namespace 만료 수. 그 plugin 의 namespace 응답이 하나라도
    /// 도착하면 지운다 — **만료 뒤에 도착한 늦은 응답도 포함한다**(아래
    /// `expired_namespace_calls`). 답하고 있는 plugin 은 아무리 느려도 여기 안 쌓인다.
    /// plugin 이 치워질 때(`cancel_pending_namespace_calls`)도 지운다.
    pub(super) namespace_expiries: HashMap<String, u32>,
    /// plugin 별로 **만료로 거둬진 namespace 호출의 request id**. 그 id 의 응답이
    /// 뒤늦게 도착하면 `namespace_expiries` 를 지우는 근거가 된다 — pending 은 이미
    /// 없으므로 그때는 이 목록만이 "이 응답이 namespace 호출의 것이었나" 를 안다.
    ///
    /// 이것 없이 "id 가 안 맞는 응답" 전체로 계수를 지우면 **늦은 hook 응답**까지
    /// 지우게 되어, namespace 호출만 삼키면서 hook 에만 답하는 plugin 이 판정을
    /// 빠져나간다. 길이는 `NAMESPACE_EXPIRY_RESTART_LIMIT` 로 잘라 무한히 안 자란다
    /// (거기 닿으면 재시작이 일어나고 그 경로가 둘 다 비운다).
    pub(super) expired_namespace_calls: HashMap<String, VecDeque<u64>>,
    /// Event Bus 1.0 라우터. 호스트 본문과 plugin 간 broadcast 이벤트를 fan-out.
    pub event_bus: super::event_bus::EventBus,
    /// 호스트가 발화하는 envelope의 `meta.trace_id` 카운터.
    pub(super) event_trace_seq: AtomicU64,
    /// 현재 열려 있는 popup 인스턴스. host가 발급한 `instance_id`를 키로 사용.
    pub(super) popup_instances: HashMap<u64, PopupInstance>,
    /// 다음 popup `instance_id`. 1부터 시작해 단조 증가.
    pub(super) next_popup_instance_id: u64,
    /// 현재 열려 있는 banner 인스턴스(A3). host가 발급한 `instance_id`를 키로 사용.
    pub(super) banner_instances: HashMap<u64, BannerInstance>,
    /// 다음 banner `instance_id`. popup 과 별도 카운터, 1부터 단조 증가.
    pub(super) next_banner_instance_id: u64,
    /// egui-mesh banner instance_id → 최근 paint_frame 메타(A3). plugin 의
    /// `BannerPaintFrame` 알림마다 갱신되고, 호스트 banner 합성기가 instance_id 로
    /// lookup 한다. banner 가 닫히거나 plugin 이 종료되면 해당 엔트리를 정리한다.
    pub(super) banner_mesh_frames: HashMap<u64, EguiMeshFrame>,
    /// `SurfaceInvalidated`(단계 06) 로 알려진 surface_id 누적 — idle 상태(입력 무)에서
    /// 파일이 바뀐 egui-mesh surface(markdown 등). `pump()` 가 채우고
    /// `take_invalidated_surfaces` 가 드레인한다.
    pub(super) invalidated_surfaces: Vec<u32>,
    /// `PopupInvalidated`(`docs/dev-guide/egui-mesh-channel.md` "popup·banner 대응") 로
    /// 알려진 popup instance_id 누적 — egui
    /// `viewport_output` self-repaint 요청(스크롤 스무딩 등) 처럼 무입력 상태에서
    /// plugin 이 재-forward 를 요청한 egui-mesh popup(git-viewer/clipboard-viewer 등).
    /// `pump()` 가 채우고 `take_invalidated_popups` 가 드레인한다.
    pub(super) invalidated_popups: Vec<u64>,
    /// `BannerInvalidated` 로 알려진 banner instance_id 누적 — 위 popup 칸의 banner
    /// 대응이고 같은 이유로 있다(banner 도 같은 `EguiMeshCore` 를 쓰므로 egui 가
    /// 무입력 재-pass 를 요청할 수 있다). `pump()` 가 채우고
    /// `take_invalidated_banners` 가 드레인한다.
    pub(super) invalidated_banners: Vec<u64>,
    /// sysinfo 측정 핸들 — tick 마다 새로 만들지 않고 재사용(할당 비용 절감).
    pub(super) sys: sysinfo::System,
    /// 이번 sampling tick 에서 모인 (plugin_id, rss_bytes). `pump()` 가 채우고
    /// `take_rss_samples` 가 드레인한다 — `App::about_to_wait` 이 host 가 직접 가진
    /// `CoreState`/`AnomalyDetector` 로 넘겨 검출·영속·알림을 처리한다(본 크레이트는
    /// telemetry anomaly 판정 로직을 모른다, plain data 만 반환).
    pub(super) pending_rss_samples: Vec<(String, u64)>,
    /// 파일 형식 식별 시스템. plugin enable/disable 시 detector 추가/제거.
    /// 호스트 본문이 CoreState 와 같은 Arc 를 공유 (trait object 로 의존성 격리).
    pub file_format: Arc<dyn tasty_plugin_protocol::host_port::FileFormatRegistryPort>,
    /// 파일 핸들러 시스템. plugin enable/disable 시 handler 추가/제거.
    pub file_handler: Arc<dyn tasty_plugin_protocol::host_port::FileHandlerRegistryPort>,
    /// 공유 훅 핸들러 레지스트리(webhook/hook). plugin enable/disable 시
    /// `[[contributes.hook_handler]]` 등록/제거. 호스트가 setter 로 주입하며 None
    /// 이면 skip (headless 부팅 전/test — 훅 핸들러 없이도 코어 동작).
    pub hook_handler: Option<Arc<dyn tasty_plugin_protocol::host_port::HookHandlerRegistryPort>>,
    /// `[[contributes.completion_strategy]]` 등록/제거. 호스트가
    /// setter 로 주입하며 None 이면 skip — hook_handler 와 동일 지위(독립
    /// 레지스트리, 미주입 시 완료 판정 전략 없이도 코어 동작).
    pub completion_strategy:
        Option<Arc<dyn tasty_plugin_protocol::host_port::CompletionStrategyRegistryPort>>,
    /// i18n namespace 등록 trait. None 이면 등록 skip (headless/test).
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
mod response;
mod retire;

// 유도 상태(확장 집합)의 신선도 단정 — 텍스트가 못 보는 "순서" 를 런타임이 본다.
#[cfg(test)]
mod tests_derived_freshness;

// H — plugin 자동 reload (baseline / check_for_updates / auto_reload_one) 테스트.
#[cfg(test)]
mod tests_auto_reload;

// namespace 소유 표가 설치된 매니페스트에서 유도되는가 (옛 mirror 테스트의 새 자리).
#[cfg(test)]
mod tests_namespace_table;

// plugin 주기 작업(PluginTick) 스케줄 — pump(now) 시간 주입 검증.
#[cfg(test)]
mod tests_timers;

#[cfg(test)]
mod tests_lifecycle_toggle;

// 만료·취소로 이미 끝난 요청의 늦은 응답이 아무것도 다시 진행시키지 않는가.
#[cfg(test)]
mod tests_late_response;
// 재발화 hop 하한이 dispatch 송신 · 응답 · publish 도착 세 자리에 이어졌는가(ADR-0406).
#[cfg(test)]
mod tests_relay_floor;
// namespace forward 가 정확히 한 번을 약속하지 않는다는 사실의 고정(ADR-0361).
#[cfg(test)]
mod tests_forward_idempotency;
// plugin 으로 넘긴 요청의 대기 항목이 hop 마다 원 IPC 요청의 번호를 드는가(ADR-0436).
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

    /// validate_namespace_call의 분기를 직접 검증하기 위한 mgr 초기화.
    /// process는 spawn하지 않는다.
    ///
    /// 소유 표를 손으로 채우지 않고 **매니페스트를 놓고 유도를 돌린다** — 그것이
    /// 운영에서 소유가 생기는 유일한 경로이기 때문이다(ADR-0173). 표를 직접 쓰면
    /// 픽스처가 운영에 없는 상태를 만들 수 있고, 그러면 이 테스트가 지키는 것이
    /// 실제 경로와 어긋난다.
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

    // 왕복 대기의 기록 자리. 이 게이지가 없던 때에는 "응답이 느리다" 가 호스트
    // 적체인지 plugin 안의 시간인지 **가릴 값이 없었다.**
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
            "보낸 뒤 흐른 시간이 접혀야 한다: {}",
            s.us_max
        );
    }

    // 끝점이 없는 관측은 안 센다. 이미 만료·취소돼 pending 에 없는 id 의 응답이
    // 그 경우다 — 시작 시각을 모르므로 여기서 0 을 쌓으면 평균이 아래로 끌린다.
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
        for i in 0..times {
            let (tx, _rx) = mpsc::sync_channel(1);
            let id = 100 + i as u64;
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

    /// 연속 만료가 쌓이면 그 plugin 이 재시작 대상이 된다. healthcheck 는 이것을
    /// 원리적으로 못 본다 — stub 은 방금 pong 한 것으로 시작하므로, 여기서 프로세스가
    /// 치워졌다면 판정한 것은 만료 계수뿐이다.
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
        mgr.restart_unresponsive_plugins();
        assert!(
            mgr.processes.contains_key("com.example.silent"),
            "상한 전인데 거둬졌다"
        );

        expire_namespace_calls(&mut mgr, "com.example.silent", 1);
        mgr.restart_unresponsive_plugins();
        assert!(
            !mgr.processes.contains_key("com.example.silent"),
            "연속 만료가 상한에 닿았는데 재시작이 안 걸렸다"
        );
    }

    /// 재시작은 disable · swap 과 **같은 정리**를 거친다 — 등록 게이트까지 푼다.
    ///
    /// 게이트가 안 풀리면 새 프로세스의 hello 가 "이미 등록됨" 으로 읽혀
    /// `register_new_hellos` 가 안 돈다. 그런데 재시작은 그 plugin 의 이벤트 권한
    /// (`event_bus.clear_plugin`)과 설정 sub-page 를 이미 지웠으므로, 재시작된 plugin 은
    /// `event.subscribe` 가 전부 거절되고 설정 탭이 사라진 채로 남는다.
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
        mgr.restart_unresponsive_plugins();

        assert!(!mgr.processes.contains_key("com.example.silent"));
        assert!(
            !mgr.registered_plugins.contains("com.example.silent"),
            "재시작이 등록 게이트를 안 풀었다 — 다음 hello 가 권한을 다시 못 받는다"
        );
    }

    /// 재시작된 plugin **에게 보낸** 요청은 caller 가 없어도 거둬진다.
    ///
    /// 새 프로세스는 새 request id 를 쓰므로 옛 id 의 응답은 다시 안 온다. deadline 도
    /// 없는 변종이라 sweep 도 안 본다 — 여기서 안 거두면 프로세스 수명 동안 남고,
    /// 남은 `SurfaceRestore` 는 `has_pending_surface_restores` 를 영구히 참으로 묶는다.
    /// 다른 plugin 에게 간 요청은 건드리지 않는다.
    #[test]
    fn requests_sent_to_a_restarted_plugin_are_reclaimed() {
        let mut mgr = PluginManager::new(empty_waker());
        mgr.processes
            .insert("com.example.silent".into(), stub_process());
        mgr.pending_requests.insert(
            900,
            PendingRequest::now(
                "com.example.silent",
                PendingRequestKind::SurfaceRestore { surface_id: 3 },
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
        mgr.restart_unresponsive_plugins();

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

    /// namespace 응답이 하나라도 오면 계수가 0 으로 돌아간다 — 답하고 있는 plugin 은
    /// 아무리 느려도 이 판정에 안 걸린다.
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
        mgr.restart_unresponsive_plugins();
        assert!(
            mgr.processes.contains_key("com.example.slow"),
            "계수가 리셋됐는데 재시작이 걸렸다"
        );
    }

    /// **만료 뒤에 도착한** namespace 응답도 계수를 지운다.
    ///
    /// 그 응답이 올 때 pending 은 이미 sweep 이 거둬 없다. 그 자리에서 계수를 안 지우면
    /// `NAMESPACE_CALL_TIMEOUT` 을 조금씩 넘겨 **매번 실제로 답하는** plugin 이 세 번마다
    /// 재시작된다 — 재시작은 느린 것을 빠르게 만들지 못하므로 그 반복은 그 plugin 의
    /// 화면만 주기적으로 없앤다.
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

        mgr.restart_unresponsive_plugins();
        assert!(
            mgr.processes.contains_key("com.example.slow"),
            "답한 plugin 이 재시작됐다"
        );
    }

    /// 계수를 지우는 것은 **namespace 응답뿐**이다 — 두 갈래를 한 자리에서 가른다.
    ///
    /// (1) pending 이 있는 다른 종류의 응답, (2) pending 이 없는데 namespace 만료로
    /// 기억된 id 도 아닌 응답(늦은 hook 응답·잡음). 둘 다 계수를 못 지워야 한다.
    /// 이것이 없으면 namespace 호출만 삼키면서 다른 것에만 답하는 plugin 이 판정을
    /// 빠져나간다.
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

        mgr.restart_unresponsive_plugins();
        assert!(
            !mgr.processes.contains_key("com.example.silent"),
            "계수가 상한인데 재시작이 안 걸렸다"
        );
    }

    /// 호스트가 **스스로 내는** 오류 코드도 plugin 경계를 넘는가. 이 자리가
    /// `None` 을 주면 plugin caller 는 SDK 기본값 `-32000` 을 보고, 같은 사건이
    /// CLI caller 에게는 `-32004` 로 간다 — 코드가 사건이 아니라 누가 물었는지를
    /// 보고하게 된다.
    #[test]
    fn a_host_originated_error_code_reaches_a_plugin_caller() {
        let mut mgr = PluginManager::new(empty_waker());
        let (proc, rx) = PluginProcess::stub_with_request_rx("com.example.caller");
        mgr.processes.insert("com.example.caller".into(), proc);
        mgr.send_final_error(
            FinalCaller::Plugin {
                caller_plugin_id: "com.example.caller".into(),
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

    /// pending 하나를 심고 `now` 시점으로 sweep 을 돌린 결과를 (남았는가, 회신된
    /// 응답) 으로 돌려준다. deadline 만 다르게 주어 만료/미만료 두 갈래를 같은
    /// 자리에서 잰다. **두 시각을 둘 다 인자로 받는 것이 요점이다** — sweep 이
    /// 자기 안에서 `Instant::now()` 를 읽으면 `now` 를 아무 값으로 줘도 판정이
    /// 안 바뀌므로, 이 짝이 시간 주입이 실제로 배선돼 있는지를 가른다.
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

    /// 주입한 시각이 실제로 판정에 쓰이는가 — deadline 을 고정해 두고 `now` 만
    /// 양쪽으로 옮긴다. sweep 이 `Instant::now()` 를 직접 읽으면 두 호출이 같은
    /// 답을 내므로 이 시험이 죽는다.
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

    /// debug 한정 직접 hook 호출도 `response_tx` 를 들고 있어, deadline 이 없으면
    /// extension 이 한 번 삼키는 것만으로 local CLI 가 영영 선다. 상한 직전과
    /// 직후를 같은 자리에서 재서 그 상한이 실제로 걸려 있는지를 가른다.
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
        // 같은 자리에서 deadline 만 미래로 옮긴다 — sweep 이 시각을 실제로 보는지를
        // 이 짝이 가른다(둘 다 통과해야 deadline 비교가 살아 있다는 뜻이다).
        //
        // 이 갈래는 하나를 더 지킨다: `NAMESPACE_CALL_TIMEOUT` 이 0 으로 무너지는
        // 것. 그 값은 두 상수의 `as_secs()` 합에서 오는데 `as_secs()` 는 버림이라,
        // 둘 다 1 초 미만이 되면 상한이 0 이 되고 모든 namespace 호출이 다음 pump
        // 에서 즉시 만료된다. 그러면 여기 deadline 이 곧 `now` 라 이 시험이 빨개진다
        // (변이 실측: 두 상수를 900ms · 500ms 로 내리면 이 시험이 죽는다).
        let (still_pending, resp) =
            sweep_one_namespace_invoke(Instant::now(), Instant::now() + NAMESPACE_CALL_TIMEOUT);
        assert!(still_pending, "아직 만료 전인데 pending 이 사라졌다");
        assert!(resp.is_none(), "만료 전에 caller 에 회신이 갔다");
    }
}

#[cfg(test)]
mod tests_namespace_start;
