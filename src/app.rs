//! `App` — winit `ApplicationHandler` 의 본체. 다중 View + 모달 + 플러그인 매니저 +
//! parked AppState 보관. 메서드는 도메인별 서브모듈로 분산되어 있다.

/// 조합 공용 — 헤드리스 데몬 루프도 이 드레인을 쓴다.
pub(crate) mod agent_events;
#[cfg(feature = "gui")]
pub(crate) mod attach_client;
#[cfg(feature = "gui")]
pub(crate) mod attach_poll;
#[cfg(feature = "gui")]
pub(crate) mod auto_attach;
#[cfg(feature = "gui")]
pub(crate) mod boot_machine;
#[cfg(feature = "gui")]
pub(crate) mod busy;
#[cfg(all(debug_assertions, feature = "gui"))]
pub(crate) mod debug_info;
#[cfg(feature = "gui")]
pub(crate) mod dispatch;
#[cfg(feature = "gui")]
pub(crate) mod dispatch_domain;
#[cfg(not(feature = "gui"))]
#[path = "app/dispatch_domain_stubs.rs"]
pub(crate) mod dispatch_domain;
pub(crate) mod event;
#[cfg(feature = "gui")]
pub(crate) mod event_handler;
#[cfg(feature = "gui")]
mod file_dispatch;
#[cfg(feature = "gui")]
pub(crate) mod global_hooks;
#[cfg(feature = "gui")]
pub(crate) mod idle_hooks;
#[cfg(feature = "gui")]
pub(crate) mod image_upload;
#[cfg(feature = "gui")]
pub(crate) mod ipc;
pub(crate) mod ipc_round;
#[cfg(feature = "gui")]
pub(crate) mod modal;
#[cfg(feature = "gui")]
pub(crate) mod persistence;
#[cfg(feature = "gui")]
pub(crate) mod plugin_glue;
pub(crate) mod process_exit;
#[cfg(feature = "gui")]
pub(crate) mod request_owner;
#[cfg(feature = "gui")]
pub(crate) mod screenshot_capture;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_cascade;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_machine;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_trace;
#[cfg(feature = "gui")]
pub(crate) mod sweeps;
pub(crate) mod timer_report;
pub(crate) mod timers;
#[cfg(feature = "gui")]
pub(crate) mod webview_keys;
#[cfg(feature = "gui")]
pub(crate) mod window_access;
#[cfg(feature = "gui")]
pub(crate) mod window_lifecycle;

#[cfg(feature = "gui")]
use std::sync::Arc;

#[cfg(feature = "gui")]
use winit::event_loop::EventLoopProxy;
#[cfg(feature = "gui")]
use winit::window::{Window, WindowId};

use crate::core::Core;
#[cfg(feature = "gui")]
use crate::gpu::GpuState;
use crate::hub::Hub;
#[cfg(not(feature = "gui"))]
use crate::plugin;
#[cfg(feature = "gui")]
use crate::view::ViewRegistry;
#[cfg(feature = "gui")]
use crate::{AppEvent, plugin, state};

/// GPU 어댑터를 하드웨어·소프트웨어 fallback 모두 못 찾았을 때의 구분 가능한 에러.
/// 호출부(`event_handler`)가 `anyhow::Error::downcast_ref` 로 감지해 panic 대신
/// 사람이 읽을 안내 메시지를 낼 수 있게 한다(`crate::core::MirrorStructuralBlocked`
/// 와 동일한 marker-type 패턴).
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoGpuAdapter;

#[cfg(feature = "gui")]
impl std::fmt::Display for NoGpuAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no compatible GPU adapter found (hardware or software fallback)"
        )
    }
}

#[cfg(feature = "gui")]
impl std::error::Error for NoGpuAdapter {}

#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "some fields are read only by the gui event loop; headless has no reader"
    )
)]
pub(crate) struct App {
    /// 도메인 본체 — `CoreState` 의 mutate 로직을 점진 흡수한 Method wrapper 다수를
    /// 이미 보유한다(수십 개 규모, 계속 증가 중). 잔여 흡수는 계속 진행 중이다.
    pub(crate) core: Core,
    /// 외부 통신 표면. ipc_server, port_file 보유.
    pub(crate) hub: Hub,
    /// Streaming-channel push registry (attach/detach step 1). The IPC accept
    /// threads register/unregister client sinks; the main loop pushes via this.
    pub(crate) stream_hub: tasty_ipc::stream_hub::StreamHub,
    /// Sender cloned into each stream connection so its read thread can route
    /// inbound frames to the main loop.
    pub(crate) stream_inbound_tx: std::sync::mpsc::Sender<tasty_ipc::stream_hub::StreamInbound>,
    /// Receiver drained by the main loop on `AppEvent::StreamReady`.
    pub(crate) stream_inbound_rx: std::sync::mpsc::Receiver<tasty_ipc::stream_hub::StreamInbound>,
    /// GUI 어댑터. proxy, modal/focus 식별자, views HashMap 보유.
    #[cfg(feature = "gui")]
    pub(crate) view: ViewRegistry,
    /// Parked AppStates: preserved when all windows are closed so PTY sessions survive.
    /// Moved into new windows when created, or used directly for IPC.
    #[cfg(feature = "gui")]
    pub(crate) parked_states: Vec<(state::AppState, crate::core::CoreState)>,
    /// 부팅 상태 머신 (`BootPhase`) — 첫 윈도우 부팅 미완 동안 `Some`.
    /// `resumed()` / shell setup 완료가 `begin_boot` 로 시작하고, Ready 도달 시
    /// `finish_boot` 가 take 해 MainView 로 합류한다 (boot_machine.rs).
    #[cfg(feature = "gui")]
    pub(crate) boot: Option<boot_machine::BootState>,
    /// 종료 상태 머신 (`ShutdownPhase`) — 종료 시작부터 `event_loop.exit()` 직전까지
    /// `Some`. 부팅과 대칭으로, 대기 단계 동안 매 프레임 종료 로딩 화면을 그린다
    /// (shutdown_machine.rs).
    #[cfg(feature = "gui")]
    pub(crate) shutdown: Option<shutdown_machine::ShutdownState>,
    // Shell setup mode (before terminal is created)
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_mode: bool,
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_path: String,
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_gpu: Option<GpuState>,
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_window: Option<Arc<Window>>,
    // Boot error mode (엔진 생성 실페인데 GPU·창은 살아있을 때). shell setup 과 같은
    // 구조로 실패 화면을 그린 채 유지하다 사용자가 종료를 누르면 exit(1) 한다.
    // GPU 부재·창 생성 실패는 그릴 수단이 없어 이 경로가 아니다(진단 후 즉시 exit).
    // 근거: ADR-0117 재검토 트리거.
    #[cfg(feature = "gui")]
    pub(crate) boot_error_mode: bool,
    #[cfg(feature = "gui")]
    pub(crate) boot_error_gpu: Option<GpuState>,
    #[cfg(feature = "gui")]
    pub(crate) boot_error_window: Option<Arc<Window>>,
    /// 그릴 진단. 엔진 실패 경로가 설정하고 `drive_boot_frame` 이 이를 보고 boot error
    /// 모드로 전환한다(pending 신호 겸 렌더 소스).
    #[cfg(feature = "gui")]
    pub(crate) boot_error_info: Option<crate::gpu::BootErrorInfo>,
    /// System tray / status item. Must be kept alive for the tray to remain visible.
    /// `None` when the platform tray is unavailable (graceful degradation, ADR-0001).
    #[cfg(all(
        any(windows, target_os = "macos", target_os = "linux"),
        feature = "gui"
    ))]
    pub(crate) tray_icon: Option<tray_icon::TrayIcon>,
    /// Tray menu item IDs for event matching.
    #[cfg(all(
        any(windows, target_os = "macos", target_os = "linux"),
        feature = "gui"
    ))]
    pub(crate) tray_menu_ids: Option<crate::system_tray::TrayMenuIds>,
    /// Modal shake animation state.
    #[cfg(feature = "gui")]
    pub(crate) modal_shake: Option<ModalShake>,
    /// Whether input simulation IPC is enabled (debug builds only).
    #[cfg(debug_assertions)]
    pub(crate) input_simulation_enabled: bool,
    /// Plugin host manager. None until the first AppState is created
    /// (which provides the WakerFactory).
    pub(crate) plugin_manager: Option<plugin::PluginManager>,
    /// 매니저가 **기동까지** 끝났는가 — 번들 설치와 plugin 프로세스 spawn.
    ///
    /// `plugin_manager.is_some()` 로는 이것을 판정할 수 없다. 헤드리스는 조회
    /// 메서드에 답하려고 매니저를 **디스크 읽기만으로** 세우는 경로가 따로 있어서
    /// (`src/boot/headless_plugins.rs`), 매니저가 있어도 아직 아무 plugin 도 안 뜬
    /// 상태가 정상이다. 이 값이 없으면 그 상태에서 기동 요청이 no-op 이 된다.
    ///
    /// 헤드리스 부트스트랩만의 상태라 feature 로 가둔다. gui 는 매니저를 만들 때
    /// 곧바로 설치·기동까지 하므로(`src/app/window_lifecycle.rs` `build_plugin_manager`)
    /// 두 상태가 갈리는 순간이 없다 — 거기에 이 필드를 두면 항상 참인 값을 유지하는
    /// 비용만 남고, 유지를 빠뜨리면 거짓을 말한다.
    #[cfg(not(feature = "gui"))]
    pub(crate) plugin_started: bool,
    /// Sessionwide engine state — workspaces, settings, hooks, registries.
    /// None until the first MainView lifecycle initializes it; Some after.
    pub(crate) core_state: Option<crate::core::CoreState>,
    /// Lua 워커 엔진 (ADR-0031). 부팅 시 1회 생성, VM 은 전용 워커 스레드 소유.
    /// 스크립트는 등록 목록에서 명시 트리거(단축키/자동실행)로만 실행. 초기화 실패 시 None.
    pub(crate) lua_engine: Option<tasty_lua::LuaEngine>,
    /// Lua 자동실행 재진입 가드 — 자동실행 스크립트가 유발한 이벤트의 cascade 재점화
    /// 차단. `about_to_wait` 시작 시 [`checkpoint`](crate::hooks::autofire::AutofireGuard::checkpoint) 1회.
    pub(crate) lua_autofire: crate::hooks::autofire::AutofireGuard,
    /// 중앙 타이머 허브 — 메인 루프의 시간축 주기 작업 전부. gui/headless 실행부가
    /// 매 프레임 `drain_due` 로 due 한 키만 받아 실행한다
    /// (`docs/dev-guide/timer-hub.md`).
    pub(crate) timers: tasty_timer::TimerHub<timers::Tick>,
    /// 허브 데드라인까지만 자고 이벤트 루프를 깨우는 waker 스레드 핸들. 창이 없는
    /// 상태(macOS 최소화 / tray 상주)에서도 `send_event` 경로라 타이머가 계속 돈다 —
    /// `ControlFlow::WaitUntil` 은 창이 있을 때 지연을 줄이는 보조 수단일 뿐이다.
    #[cfg(feature = "gui")]
    pub(crate) timer_waker: tasty_timer::TimerWakerHandle,
    /// IPC 회차의 양보 규칙 — 직전 회차가 끝난 시각을 들고, 사용자 이벤트(`IpcReady`)가
    /// 회차를 돌려도 되는지 정한다(`src/app/ipc.rs` 의 `IpcPacer`).
    #[cfg(feature = "gui")]
    pub(crate) ipc_pacer: crate::app::ipc::IpcPacer,
    /// 현재 열려 있는 `PresetView` 의 winit window id. modeless editor view 는
    /// 엔진 전역 단일 인스턴스 — 같은 명령이 다시 들어오면 새 view 를 만들지 않고
    /// 이 id 의 view 로 포커스만 이동한다.
    #[cfg(feature = "gui")]
    pub(crate) preset_view_id: Option<WindowId>,
    /// map 뒤에 초기 포커스 힌트(X11 `_NET_WM_USER_TIME = 0`)를 지워야 하는 에이전트 창.
    /// winit 이 map 을 알리는 첫 `WindowEvent::Focused(_)` 에서 비운다(ADR-0497). 그 전에 창이
    /// 닫히면 닫힘 경로(`close_main_window` · `close_self_requesting_window`)가 뺀다.
    #[cfg(feature = "gui")]
    pub(crate) pending_focus_hint_clear: std::collections::HashSet<WindowId>,
    /// Plugins 모달의 `Configure` 진입점이 Settings 모달을 열 때, 첫 진입 탭을
    /// `Plugin` 으로 강제하기 위한 1회성 플래그. `open_settings_modal` 이 소비한다.
    #[cfg(feature = "gui")]
    pub(crate) pending_settings_plugin_tab: bool,
    /// file handler picker popup 의 "설정에서 핸들러 등록" 클릭(시스템 전체
    /// handler 0개)이 Settings 모달을 열 때, 첫 진입 탭을 `FileHandler` 로
    /// 강제하기 위한 1회성 플래그. `open_settings_modal` 이 소비한다. release
    /// 빌드에서도 동작하는 일반 기능 — `pending_settings_tab`(debug 전용) 과
    /// 달리 `#[cfg(debug_assertions)]` 로 막혀 있지 않다.
    #[cfg(feature = "gui")]
    pub(crate) pending_settings_file_handler_tab: bool,
    /// `debug.settings.open` 이 지정한 초기 탭 키 (예: `"appearance"`). 다음
    /// `open_settings_modal` 이 1회성으로 소비한다. 설정 모달을 코드로 강제로 여는
    /// 것은 사용자 조작 재현이므로 debug 빌드 전용 (시각 검증 자동화용).
    #[cfg(all(feature = "gui", debug_assertions))]
    pub(crate) pending_settings_tab: Option<String>,
    /// `debug.settings.open` 이 지정한 초기 L2 섹션(하위탭) 키 (예: `"colors"`).
    /// `pending_settings_tab` 으로 L1 을 정한 뒤 `open_settings_modal` 이 1회성으로
    /// 소비한다. 사용자 조작 재현이므로 debug 빌드 전용 (시각 검증 자동화용).
    #[cfg(all(feature = "gui", debug_assertions))]
    pub(crate) pending_settings_subtab: Option<String>,
    /// attach/detach 작업 J — 호스트가 client 로서 점유한 원격 워크스페이스의 mirror
    /// 세션들(연결 reader/입력 forwarder 스레드 + remote↔local id 맵). `Tick::AttachView` 가
    /// 출력 적용/정리에 순회한다.
    #[cfg(feature = "gui")]
    pub(crate) attach_client_sessions: Vec<attach_client::AttachClientSession>,
    /// 단계 7 — 자동 attach 진행 중/완료된 매핑(anchor) 워크스페이스 id 집합. 중복
    /// 트리거 방지(활성화 polling 이 anchor 가 여기 있으면 skip). 세션 정리 시 제거.
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_active: std::collections::HashSet<u32>,
    /// 직전 프레임에 포커스 창의 활성 워크스페이스였던 id(엣지 감지용).
    /// `maybe_trigger_auto_attach` 가 `auto_attach_pending_reactivation` 에 속한
    /// anchor 에 한해, 이 값과 이번 프레임의 활성 ws id 를 비교해 **전환**(재활성화)
    /// 만 트리거로 인정한다 — 아래 `auto_attach_pending_reactivation` 문서 참고.
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_last_active_ws: Option<u32>,
    /// silent disconnect(원격발 EOF/force-detach/heartbeat TTL) 로 방금 정리되어
    /// **재진입 대기 중인** anchor(매핑된 로컬 ws id) 집합. `cleanup_mirror_workspace`
    /// 가 이 정리가 disconnect 경로(사용자가 mirror ws 자체를 닫은 경로가 아니라)임을
    /// 확인했을 때만 여기 넣는다. `maybe_trigger_auto_attach` 는 이 집합에 속한
    /// anchor 만 워크스페이스 전환(엣지, `auto_attach_last_active_ws` 비교)이 있어야
    /// 트리거 후보로 본다 — 이 집합에 없는 anchor(신규 mapping 등)는 기존처럼 활성화
    /// 즉시(레벨) 트리거된다. 트리거에 성공하면 이 집합에서 제거한다.
    ///
    /// 이게 없으면: "disconnect 후 조용한 자동 재연결 억제"를 anchor 워크스페이스가
    /// **활성 상태로 남아있는지 자체**로 판정하게 되는데, 그 기준은 "방금 새로
    /// `attach_mapping` 을 설정한 이미-활성인 워크스페이스"와 구분이 안 된다 — 그러면
    /// 후자(흔한 CLI 시나리오: `tasty set workspace --ssh-profile ...` 를 이미 활성인
    /// 워크스페이스에 실행)도 워크스페이스 전환 전까지 트리거되지 않는 회귀가 생긴다.
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_pending_reactivation: std::collections::HashSet<u32>,
    /// 단계 7 — 자동 attach 워커 스레드 → 메인 루프 결과 채널(SSH 터널/포트 전달).
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_tx: std::sync::mpsc::Sender<auto_attach::AutoAttachOutcome>,
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_rx: std::sync::mpsc::Receiver<auto_attach::AutoAttachOutcome>,
    /// silent disconnect 로 `Reconnecting` 상태가 된 anchor 마다의 backoff 재시도
    /// 스케줄(다음 시도 시각 + 백오프 간격 + 시도 횟수, 상세:
    /// `docs/features/remote-attach/index.md` / `docs/dev-guide/attach-behavior.md#gui-자동-재연결-스코프`).
    /// `auto_attach.rs` 의
    /// `maybe_trigger_reconnect` 가 매 프레임(`poll_auto_attach`) 확인해, 시각이 되면
    /// 워커를 spawn 하고 백오프를 진행시킨다. 재연결 성공/사용자가 mirror 를 닫으면
    /// 해당 anchor 항목을 제거한다.
    #[cfg(feature = "gui")]
    pub(crate) auto_attach_reconnect: std::collections::HashMap<u32, auto_attach::ReconnectSlot>,
    /// 스크린샷→클립보드 캡처 워커 스레드 → 메인 루프 결과 채널.
    #[cfg(feature = "gui")]
    pub(crate) screenshot_capture_tx:
        std::sync::mpsc::Sender<screenshot_capture::ScreenshotCaptureOutcome>,
    #[cfg(feature = "gui")]
    pub(crate) screenshot_capture_rx:
        std::sync::mpsc::Receiver<screenshot_capture::ScreenshotCaptureOutcome>,
    /// mirror 이미지 paste 업로드 워커 스레드 → 메인 루프 결과 채널.
    #[cfg(feature = "gui")]
    pub(crate) image_upload_tx: std::sync::mpsc::Sender<image_upload::ImageUploadOutcome>,
    #[cfg(feature = "gui")]
    pub(crate) image_upload_rx: std::sync::mpsc::Receiver<image_upload::ImageUploadOutcome>,
    /// mirror 파일 전송 진행 이벤트 채널 — 업로드 워커 on_progress → 메인 루프.
    /// 진행 팝업의 determinate bar/바이트/속도를 갱신한다(`drain_transfer_progress`).
    #[cfg(feature = "gui")]
    pub(crate) transfer_progress_tx: std::sync::mpsc::Sender<image_upload::TransferProgressMsg>,
    #[cfg(feature = "gui")]
    pub(crate) transfer_progress_rx: std::sync::mpsc::Receiver<image_upload::TransferProgressMsg>,
    /// 모든 윈도우가 공유하는 wgpu `Instance`. 부트(`App::new`) 시 `Backends::all()`
    /// 로 1회 생성한다. 창마다 `Instance::new`(~50ms) 를 반복하지 않으려고 App 이
    /// 소유 — 모든 surface 가 이 instance 에서 만들어지고 그 수명에 의존하므로
    /// (App 이 모든 창보다 오래 삶) wgpu 의 "동일 instance" 제약도 충족한다.
    #[cfg(feature = "gui")]
    pub(crate) gpu_instance: Arc<wgpu::Instance>,
    /// 공유 wgpu `Adapter`. 첫 윈도우의 surface 로 `request_adapter`(다중 백엔드
    /// 어댑터 열거, ~137ms) 를 1회만 수행해 캐시한다. 이후 창들은 이 adapter 로
    /// 곧장 `request_device` 한다(어댑터 선택은 첫 창과 동일 → 렌더 결과 불변).
    #[cfg(feature = "gui")]
    pub(crate) gpu_adapter: Option<Arc<wgpu::Adapter>>,
}

/// State for the modal window shake animation.
#[cfg(feature = "gui")]
pub(crate) struct ModalShake {
    pub(crate) start: std::time::Instant,
    /// Original window position before shake began.
    pub(crate) origin: winit::dpi::PhysicalPosition<i32>,
}

impl App {
    #[cfg(feature = "gui")]
    pub(crate) fn new(
        proxy: EventLoopProxy<AppEvent>,
        port_file: Option<String>,
        memory: Option<std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>>,
        #[cfg(debug_assertions)] input_simulation_enabled: bool,
    ) -> anyhow::Result<Self> {
        let (stream_inbound_tx, stream_inbound_rx) = std::sync::mpsc::channel();
        let (auto_attach_tx, auto_attach_rx) = std::sync::mpsc::channel();
        let (screenshot_capture_tx, screenshot_capture_rx) = std::sync::mpsc::channel();
        let (image_upload_tx, image_upload_rx) = std::sync::mpsc::channel();
        let (transfer_progress_tx, transfer_progress_rx) = std::sync::mpsc::channel();
        let mut timers = tasty_timer::TimerHub::new();
        timers::register_steady_state(&mut timers, std::time::Instant::now());
        // 데드라인 waker — 고정 주기 ticker 스레드의 대체. 창 유무·플랫폼과 무관하게
        // `send_event` 로 깨우므로 최소화/tray 상주 상태에서도 시간축이 살아 있다.
        let timer_waker = tasty_timer::spawn_timer_waker({
            let proxy = proxy.clone();
            move || proxy.send_event(AppEvent::TimerTick).is_ok()
        });
        Ok(Self {
            core: crate::boot::wiring::build_production_core(memory)?,
            hub: Hub::new(port_file),
            stream_hub: tasty_ipc::stream_hub::StreamHub::new(),
            stream_inbound_tx,
            stream_inbound_rx,
            view: ViewRegistry::new(proxy.clone()),
            parked_states: Vec::new(),
            boot: None,
            shutdown: None,
            shell_setup_mode: false,
            shell_setup_path: String::new(),
            shell_setup_gpu: None,
            shell_setup_window: None,
            boot_error_mode: false,
            boot_error_gpu: None,
            boot_error_window: None,
            boot_error_info: None,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            tray_icon: None,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            tray_menu_ids: None,
            modal_shake: None,
            #[cfg(debug_assertions)]
            input_simulation_enabled,
            plugin_manager: None,
            #[cfg(not(feature = "gui"))]
            plugin_started: false,
            core_state: None,
            lua_engine: crate::hooks::lua::init_engine(),
            lua_autofire: crate::hooks::autofire::AutofireGuard::new(),
            timers,
            timer_waker,
            ipc_pacer: crate::app::ipc::IpcPacer::new({
                let proxy = proxy.clone();
                Box::new(move || {
                    crate::shortcuts::send_app_event(&proxy, AppEvent::IpcReady);
                })
            }),
            preset_view_id: None,
            pending_focus_hint_clear: std::collections::HashSet::new(),
            pending_settings_plugin_tab: false,
            pending_settings_file_handler_tab: false,
            #[cfg(debug_assertions)]
            pending_settings_tab: None,
            #[cfg(debug_assertions)]
            pending_settings_subtab: None,
            attach_client_sessions: Vec::new(),
            auto_attach_active: std::collections::HashSet::new(),
            auto_attach_last_active_ws: None,
            auto_attach_pending_reactivation: std::collections::HashSet::new(),
            auto_attach_tx,
            auto_attach_rx,
            auto_attach_reconnect: std::collections::HashMap::new(),
            screenshot_capture_tx,
            screenshot_capture_rx,
            image_upload_tx,
            image_upload_rx,
            transfer_progress_tx,
            transfer_progress_rx,
            // 공유 wgpu instance — 부트 시 1회. `Backends::all()` 로 백엔드 자동
            // 선택을 유지한다(어댑터는 첫 윈도우 surface 로 지연 생성 → gpu_adapter).
            gpu_instance: Arc::new(wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            })),
            gpu_adapter: None,
        })
    }

    /// Headless 부트 — winit/wgpu/egui 0. mpsc 기반 waker.
    #[cfg(not(feature = "gui"))]
    pub(crate) fn new_headless(
        port_file: Option<String>,
        memory: Option<std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>>,
    ) -> anyhow::Result<Self> {
        let (stream_inbound_tx, stream_inbound_rx) = std::sync::mpsc::channel();
        let mut timers = tasty_timer::TimerHub::new();
        timers::register_steady_state(&mut timers, std::time::Instant::now());
        Ok(Self {
            core: crate::boot::wiring::build_production_core_headless(memory)?,
            hub: Hub::new(port_file),
            stream_hub: tasty_ipc::stream_hub::StreamHub::new(),
            stream_inbound_tx,
            stream_inbound_rx,
            #[cfg(debug_assertions)]
            input_simulation_enabled: false,
            plugin_manager: None,
            #[cfg(not(feature = "gui"))]
            plugin_started: false,
            core_state: None,
            lua_engine: crate::hooks::lua::init_engine(),
            lua_autofire: crate::hooks::autofire::AutofireGuard::new(),
            timers,
        })
    }

    /// CoreState 접근자. 부팅 시 `App.core_state` 에 들어 있다가 첫 MainView
    /// 등록 시 그쪽으로 이동한다. 이 헬퍼는 두 위치 중 살아있는 쪽을 찾아 반환한다.
    /// 어디에도 없으면 panic — 호출 경로가 invariant 를 깬 것.
    #[cfg(feature = "gui")]
    pub(crate) fn core_state(&self) -> &crate::core::CoreState {
        if let Some(e) = self.core_state.as_ref() {
            return e;
        }
        #[cfg(feature = "gui")]
        for w in self.view.views.values() {
            if let Some(main) = w.as_main() {
                return &main.core_state;
            }
        }
        panic!("App.core_state accessed before initialization");
    }

    /// Lua 자동실행 dispatch 용 스크립트 레지스트리 스냅샷 (clone).
    ///
    /// settings 는 CoreState 사본마다 존재하고 설정 apply 시 전 사본에 브로드캐스트
    /// 되므로 살아있는 첫 사본을 읽으면 된다. 아직 어디에도 없으면(부팅 극초반 /
    /// headless 초기) 빈 레지스트리 — 자동실행 no-op. panic 하지 않는 이유:
    /// fire 지점은 CoreState 초기화 이전에도 지나갈 수 있다.
    #[cfg(feature = "gui")]
    pub(crate) fn autofire_scripts(&self) -> tasty_settings::ScriptRegistry {
        if let Some(cs) = self.core_state.as_ref() {
            return cs.settings.scripts.clone();
        }
        #[cfg(feature = "gui")]
        {
            for w in self.view.views.values() {
                if let Some(main) = w.as_main() {
                    return main.core_state.settings.scripts.clone();
                }
            }
            if let Some((_, e)) = self.parked_states.first() {
                return e.settings.scripts.clone();
            }
        }
        tasty_settings::ScriptRegistry::default()
    }

    /// 공유 instance/adapter 를 사용해 per-window `GpuState` 를 생성한다. 6개 윈도우
    /// 오픈 경로(메인창/새창/settings/preset/plugins/quit)가 모두 이 헬퍼를 거친다.
    ///
    /// adapter 는 첫 호출 때 이 윈도우의 surface 를 `compatible_surface` 로 1회만
    /// 생성·캐시한다(현재 코드와 동일한 어댑터 선택 → 렌더 결과 불변). 이후 호출은
    /// 캐시된 adapter 로 곧장 `request_device` 만 수행한다.
    #[cfg(feature = "gui")]
    pub(crate) fn create_gpu_state(
        &mut self,
        window: Arc<Window>,
        appearance: &crate::settings::AppearanceSettings,
    ) -> anyhow::Result<GpuState> {
        let instance = Arc::clone(&self.gpu_instance);
        // 창마다 만들어지는 egui 컨텍스트가 처음부터 같은 노치 거리를 갖게 한다 —
        // 모달은 열릴 때 새로 만들어지므로 이 한 지점이 전부를 덮는다(ADR-0130).
        // 첫 창은 CoreState 보다 먼저 만들어질 수 있어 `core_state()`(없으면 panic)를
        // 쓰지 않는다 — 그때는 기본값이고, 이후 프레임이 설정값으로 덮는다.
        let wheel_line_scroll = self
            .core_state
            .as_ref()
            .map(|cs| cs.settings.general.wheel_line_scroll)
            .or_else(|| {
                self.view.views.values().find_map(|w| {
                    w.as_main()
                        .map(|m| m.core_state.settings.general.wheel_line_scroll)
                })
            })
            .unwrap_or(tasty_settings::DEFAULT_WHEEL_LINE_SCROLL);
        // 전역 Theme 에 실을 런타임 값(배율·모션 감소). `wheel_line_scroll` 과 같은
        // 자리에서 같은 방식으로 찾는다 — 호출부 6 곳이 이 값을 따로 넘기지 않아도
        // 되게 하려는 것이고, 그게 이 값을 빠뜨릴 수 없게 하는 형태다.
        // 배율만은 인자로 받은 `appearance` 에서 온다: 모달 창은 아직 자기 settings
        // 를 CoreState 에 갖고 있지 않은 시점에도 열리는데, 그때 넘어온 appearance 가
        // 그 창이 그릴 배율의 정본이다.
        let theme_runtime = tasty_themes::ThemeRuntime {
            ui_zoom: appearance.ui_scale_factor(),
            ..self
                .core_state
                .as_ref()
                .map(|cs| cs.settings.theme_runtime())
                .or_else(|| {
                    self.view
                        .views
                        .values()
                        .find_map(|w| w.as_main().map(|m| m.core_state.settings.theme_runtime()))
                })
                .unwrap_or_default()
        };
        let proxy = self.view.proxy.clone();
        pollster::block_on(async move {
            if self.gpu_adapter.is_none() {
                // 첫 윈도우: 어댑터 선택용 probe surface 로 request_adapter 1회.
                // probe surface 는 이 스코프에서 drop 되고, 실제 surface 는 아래
                // new_shared 가 다시 생성한다(같은 윈도우, 순차 — 동시 보유 아님).
                let adapter = {
                    let probe = instance.create_surface(window.clone())?;
                    let opts = wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::default(),
                        compatible_surface: Some(&probe),
                        force_fallback_adapter: false,
                    };
                    // 하드웨어 어댑터가 없으면(예: GPU 미탑재 CI/서버, 드라이버 미설치)
                    // 소프트웨어 rasterizer(lavapipe 등)로 한 번 더 시도한다. 그래도
                    // 없으면 NoGpuAdapter 로 호출부(event_handler)가 안내 메시지를
                    // 낼 수 있게 구분 가능한 에러를 반환한다.
                    match instance.request_adapter(&opts).await {
                        Some(a) => a,
                        None => instance
                            .request_adapter(&wgpu::RequestAdapterOptions {
                                force_fallback_adapter: true,
                                ..opts
                            })
                            .await
                            .ok_or_else(|| anyhow::Error::new(NoGpuAdapter))?,
                    }
                };
                self.gpu_adapter = Some(Arc::new(adapter));
            }
            let adapter = Arc::clone(
                self.gpu_adapter
                    .as_ref()
                    .expect("gpu_adapter set above when None"),
            );
            GpuState::new_shared(
                &instance,
                &adapter,
                window,
                appearance,
                theme_runtime,
                wheel_line_scroll,
                proxy,
            )
            .await
        })
    }

    #[cfg(feature = "gui")]
    pub(crate) fn core_state_mut(&mut self) -> &mut crate::core::CoreState {
        if let Some(cs) = self.core_state.as_mut() {
            return cs;
        }
        #[cfg(feature = "gui")]
        for w in self.view.views.values_mut() {
            if let Some(main) = w.as_main_mut() {
                return &mut main.core_state;
            }
        }
        panic!("App.core_state accessed before initialization");
    }
}
