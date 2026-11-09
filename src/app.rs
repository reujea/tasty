//! `App`은 winit 이벤트 처리, 창·모달·플러그인, 창 없이 보존하는 세션 상태를 관리한다.

/// GUI와 헤드리스에서 공통으로 이벤트 큐를 비운다.
pub(crate) mod agent_events;
pub(crate) mod approval_grant;
pub(crate) mod approval_records;
pub(crate) mod attach_activation;
#[cfg(feature = "gui")]
pub(crate) mod attach_client;
#[cfg(feature = "gui")]
mod attach_input;
#[cfg(feature = "gui")]
pub(crate) mod attach_poll;
#[cfg(feature = "gui")]
pub(crate) mod auto_attach;
#[cfg(feature = "gui")]
pub(crate) mod boot_machine;
#[cfg(feature = "gui")]
pub(crate) mod busy;
pub(crate) mod command;
pub(crate) mod creation_intent;
#[cfg(feature = "gui")]
pub(crate) mod dag_reads;
#[cfg(all(debug_assertions, feature = "gui"))]
pub(crate) mod debug_info;
#[cfg(feature = "gui")]
pub(crate) mod dispatch;
#[cfg(feature = "gui")]
pub(crate) mod dispatch_domain;
#[cfg(feature = "gui")]
mod egui_repaint;
#[cfg(feature = "gui")]
pub(crate) mod engine_registry;
pub(crate) mod event;
#[cfg(feature = "gui")]
pub(crate) mod event_handler;
pub(crate) mod file_consumers;
#[cfg(feature = "gui")]
mod file_dispatch;
pub(crate) mod file_writer;
#[cfg(feature = "gui")]
pub(crate) mod font_rejection;
#[cfg(feature = "gui")]
pub(crate) mod global_hooks;
#[cfg(feature = "gui")]
mod halt_frame;
#[cfg(feature = "gui")]
pub(crate) mod idle_hooks;
#[cfg(feature = "gui")]
pub(crate) mod image_jobs;
#[cfg(feature = "gui")]
pub(crate) mod image_upload;
#[cfg(feature = "gui")]
pub(crate) mod ipc;
pub(crate) mod ipc_round;
pub(crate) mod journal;
#[cfg(feature = "gui")]
pub(crate) mod local_reads;
#[cfg(all(debug_assertions, feature = "gui"))]
pub(crate) mod loop_measurement;
mod memory_changes;
#[cfg(feature = "gui")]
pub(crate) mod modal;
#[cfg(feature = "gui")]
mod offview_window_events;
#[cfg(feature = "gui")]
pub(crate) mod persistence;
pub(crate) mod plugin_events;
pub(crate) mod plugin_failure_notice;
pub(crate) mod plugin_file_lifecycle;
#[cfg(feature = "gui")]
pub(crate) mod plugin_glue;
pub(crate) mod plugin_lifecycle;
pub(crate) mod preset_commands;
pub(crate) mod process_exit;
pub(crate) mod publication_input;
#[cfg(feature = "gui")]
mod recovery_notices;
pub(crate) mod request_gate;
#[cfg(feature = "gui")]
pub(crate) mod request_owner;
#[cfg(feature = "gui")]
pub(crate) mod screenshot_capture;
pub(crate) mod services;
pub(crate) mod settings_change;
#[cfg(feature = "gui")]
mod shell_integration_hint_poll;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_cascade;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_machine;
#[cfg(feature = "gui")]
mod shutdown_observers;
#[cfg(feature = "gui")]
pub(crate) mod shutdown_trace;
pub(crate) mod state;
pub(crate) mod storage_notice;
#[cfg(feature = "gui")]
pub(crate) mod stream_round;
#[cfg(feature = "gui")]
pub(crate) mod sweeps;
pub(crate) mod timer_report;
pub(crate) mod timers;
#[cfg(feature = "gui")]
pub(crate) mod tutorial_progress;
#[cfg(feature = "gui")]
pub(crate) mod webview_keys;
#[cfg(feature = "gui")]
pub(crate) mod window_access;
#[cfg(feature = "gui")]
pub(crate) mod window_lifecycle;
pub(crate) mod writer_drain;

#[cfg(feature = "gui")]
mod external_activation;

#[cfg(feature = "gui")]
use std::sync::Arc;

#[cfg(feature = "gui")]
use winit::event_loop::EventLoopProxy;
#[cfg(feature = "gui")]
use winit::window::{Window, WindowId};

use crate::app::services::AppServices;
#[cfg(feature = "gui")]
use crate::gpu::GpuState;
use crate::hub::Hub;
#[cfg(not(feature = "gui"))]
use crate::plugin;
#[cfg(feature = "gui")]
use crate::view::ViewRegistry;
#[cfg(feature = "gui")]
use crate::{AppEvent, plugin};

/// GPU 어댑터를 찾지 못한 오류. 호출자가 downcast하여 사용자에게 안내한다.
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

pub(crate) struct App {
    pub(crate) journal: journal::JournalApplication,
    pub(crate) preset_io: preset_capture::PendingPresetFiles,
    #[cfg(feature = "gui")]
    pub(crate) remote_io: remote_tool_files::Pending,
    #[cfg(feature = "gui")]
    pub(crate) settings_edit_owner: Option<settings_edit::SettingsEditOwner>,
    #[cfg(feature = "gui")]
    pub(crate) port_scans: port_scans::PortScans,
    #[cfg(feature = "gui")]
    pub(crate) explorer_files: explorer_files::ExplorerFiles,
    #[cfg(feature = "gui")]
    pub(crate) local_reads: local_reads::LocalReads,
    #[cfg(feature = "gui")]
    pub(crate) dag_reads: dag_reads::DagReads,
    #[cfg(feature = "gui")]
    pub(crate) tutorial_progress: tutorial_progress::TutorialProgress,
    /// state.db writer가 결과를 냈을 때 메인 루프를 깨운다. 부팅이 writer를 시작할 때 넘긴다.
    #[cfg(feature = "gui")]
    pub(crate) state_db_wake: Arc<dyn Fn() + Send + Sync>,
    pub(crate) pending_server_attaches: Vec<attach_activation::PendingAttach>,
    /// 받을 workspace가 아직 없어 전달을 미룬 복원·저장 알림.
    #[cfg(feature = "gui")]
    pub(crate) pending_recovery_notices: Vec<journal::notices::RecoveryNotice>,
    /// 사용자에게 알린 거절된 글꼴 값. 프로세스 수명 동안 유지한다.
    #[cfg(feature = "gui")]
    pub(crate) font_rejections: font_rejection::FontRejectionNotices,
    /// 마지막으로 띄운 오른쪽 위 복원 안내 id.
    #[cfg(feature = "gui")]
    pub(crate) next_restore_notice_id: u64,
    pub(crate) publication_inputs: publication_input::PublicationInputs,
    /// 요청 게이트의 저장 결과를 기다리는 IPC 명령·plugin 호출. 접수 순서대로 하나씩 진행한다.
    pub(crate) gate_queue: request_gate::GateQueue,
    pub(crate) services: AppServices,
    pub(crate) state: state::AppState,
    pub(crate) hub: Hub,
    /// IPC 연결 스레드가 수신자를 등록하고 메인 루프가 출력을 전송한다.
    pub(crate) stream_hub: tasty_ipc::stream_hub::StreamHub,
    /// Sender cloned into each stream connection so its read thread can route
    /// inbound frames to the main loop.
    pub(crate) stream_inbound_tx: std::sync::mpsc::Sender<tasty_ipc::stream_hub::StreamInbound>,
    /// Receiver drained by the main loop on `AppEvent::StreamReady`.
    pub(crate) stream_inbound_rx: std::sync::mpsc::Receiver<tasty_ipc::stream_hub::StreamInbound>,
    /// 처리 전인 StreamReady를 하나로 합친다. 메인 루프가 스트림 회차를 열 때 표시를 내린다.
    pub(crate) stream_wake: tasty_ipc::stream_hub::StreamWakeGate,
    /// attach holder 가 요청한 원격 explorer 파일 작업. 연결마다 하나씩 실행한다.
    pub(crate) remote_file_jobs: file_jobs::remote::RemoteJobs,
    #[cfg(feature = "gui")]
    pub(crate) view: ViewRegistry,
    /// 모든 engine과 창·parked·임시 관계. 창을 모두 닫아도 engine과 PTY는 여기 남는다.
    /// `view` 바로 뒤에 두어 종료 때 모든 창 View가 먼저, 그 뒤 engine이 drop된다.
    #[cfg(feature = "gui")]
    pub(crate) engines: engine_registry::EngineRegistry,
    /// 첫 창의 부팅 중에만 보유한다. 완료 시 상태를 MainView로 옮긴다.
    #[cfg(feature = "gui")]
    pub(crate) boot: Option<boot_machine::BootResources>,
    #[cfg(feature = "gui")]
    pub(crate) pending_window: Option<window_lifecycle::PendingWindow>,
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_gpu: Option<GpuState>,
    #[cfg(feature = "gui")]
    pub(crate) shell_setup_window: Option<Arc<Window>>,
    #[cfg(feature = "gui")]
    pub(crate) boot_error_gpu: Option<GpuState>,
    #[cfg(feature = "gui")]
    pub(crate) boot_error_window: Option<Arc<Window>>,
    /// System tray / status item. Must be kept alive for the tray to remain visible.
    /// `None` when the platform tray is unavailable (graceful degradation, docs/architecture/shutdown-sequence.md).
    #[cfg(all(
        any(windows, target_os = "macos", target_os = "linux"),
        feature = "gui"
    ))]
    pub(crate) tray_icon: Option<tray_icon::TrayIcon>,
    #[cfg(all(
        any(windows, target_os = "macos", target_os = "linux"),
        feature = "gui"
    ))]
    pub(crate) tray_menu_ids: Option<crate::system_tray::TrayMenuIds>,
    #[cfg(feature = "gui")]
    pub(crate) modal_shake: Option<ModalShake>,
    pub(crate) plugin_manager: Option<plugin::PluginManager>,
    pub(crate) plugin_files: plugin_file_lifecycle::PluginFiles,
    /// VM은 전용 워커 스레드가 소유한다. 초기화 실패 시 None.
    pub(crate) lua_engine: Option<tasty_lua::LuaEngine>,
    /// 자동실행 스크립트가 일으킨 이벤트로 같은 스크립트를 다시 실행하지 않게 한다.
    /// `about_to_wait` 시작 시 [`checkpoint`](crate::hooks::autofire::AutofireGuard::checkpoint)로 회차를 갱신한다.
    pub(crate) lua_autofire: crate::hooks::autofire::AutofireGuard,
    /// 결과를 기다리는 Lua 실행의 시작 경로. 단축키 결과는 원 창 토스트, 자동실행 결과는 알림으로 보낸다.
    pub(crate) lua_runs: crate::hooks::lua_runs::LuaRuns,
    #[cfg(feature = "gui")]
    pub(crate) scripts_after_save: Vec<(
        winit::window::WindowId,
        crate::app::settings_change::SettingsWait,
        String,
        String,
    )>,
    /// GUI와 헤드리스의 주기 작업을 예약한다. docs/dev-guide/timer-hub.md 참조.
    pub(crate) timers: tasty_timer::TimerHub<timers::Tick>,
    /// 창이 없거나 최소화되어도 send_event로 타이머 처리를 깨운다.
    #[cfg(feature = "gui")]
    pub(crate) timer_waker: tasty_timer::TimerWakerHandle,
    /// IPC 회차 사이에 대기 시간을 두어 타이머·사용자 이벤트 처리가 계속되게 한다.
    #[cfg(feature = "gui")]
    pub(crate) ipc_pacer: crate::app::ipc::IpcPacer,
    /// 스트림 회차 사이에도 같은 간격을 두어 입력·렌더·IPC가 차례를 받게 한다.
    #[cfg(feature = "gui")]
    pub(crate) stream_pacer: crate::app::stream_round::StreamPacer,
    /// mirror 연결의 읽기 스레드가 공유하는 깨움 표시. 처리 전인 AttachClientData를 하나로 합친다.
    #[cfg(feature = "gui")]
    pub(crate) attach_client_wake: tasty_ipc::stream_hub::StreamWakeGate,
    #[cfg(feature = "gui")]
    pub(crate) attach_client_pacer: crate::app::stream_round::StreamPacer,
    /// 첫 Focused 이벤트 뒤 X11 초기 포커스 힌트를 지울 에이전트 창.
    /// 그 전에 닫힌 창은 닫기 경로에서 제거한다.
    #[cfg(feature = "gui")]
    pub(crate) pending_focus_hint_clear: std::collections::HashSet<WindowId>,
    /// 다음에 만드는 창에 넣을 실행기 활성화 토큰(외부 활성화 요청에 창이 없을 때).
    #[cfg(feature = "gui")]
    pub(crate) next_window_activation:
        Option<crate::boot::single_instance::evidence::LaunchEvidence>,
    /// 만드는 중인 창이 등록되면 OS 활성화를 요청할 외부 요청.
    #[cfg(feature = "gui")]
    pub(crate) pending_external_activation:
        Option<crate::boot::single_instance::ExternalActivation>,
    #[cfg(feature = "gui")]
    pub(crate) remote: tasty_remote::outbound::Remote,
    /// 스크린샷→클립보드 캡처 워커 스레드 → 메인 루프 결과 채널.
    #[cfg(feature = "gui")]
    pub(crate) screenshot_workers: screenshot_capture::ScreenshotWorkers,
    #[cfg(feature = "gui")]
    pub(crate) screenshot_capture_tx:
        std::sync::mpsc::Sender<screenshot_capture::ScreenshotCaptureOutcome>,
    #[cfg(feature = "gui")]
    pub(crate) screenshot_capture_rx:
        std::sync::mpsc::Receiver<screenshot_capture::ScreenshotCaptureOutcome>,
    /// 이미지 파일 저장처럼 이벤트 루프를 멈추는 작업을 실행하는 worker.
    #[cfg(feature = "gui")]
    pub(crate) image_jobs: image_jobs::ImageWorker<image_jobs::ImageJobDone>,
    /// mirror 이미지 paste 업로드 워커 스레드 → 메인 루프 결과 채널.
    #[cfg(feature = "gui")]
    pub(crate) image_upload_tx: std::sync::mpsc::Sender<image_upload::ImageUploadOutcome>,
    #[cfg(feature = "gui")]
    pub(crate) image_upload_rx: std::sync::mpsc::Receiver<image_upload::ImageUploadOutcome>,
    /// 업로드 워커의 진행 정보를 메인 루프로 전달한다.
    #[cfg(feature = "gui")]
    pub(crate) transfer_progress_tx: std::sync::mpsc::Sender<image_upload::TransferProgressMsg>,
    #[cfg(feature = "gui")]
    pub(crate) transfer_progress_rx: std::sync::mpsc::Receiver<image_upload::TransferProgressMsg>,
    /// 창마다 Instance를 만들지 않도록 공유한다. 모든 surface보다 오래 유지한다.
    #[cfg(feature = "gui")]
    pub(crate) gpu_instance: Arc<wgpu::Instance>,
    /// 첫 창의 surface로 선택한 어댑터를 다른 창에서도 재사용한다.
    #[cfg(feature = "gui")]
    pub(crate) gpu_adapter: Option<Arc<wgpu::Adapter>>,
    /// 다른 윈도우에 마지막으로 알린 공용 목록 원본 리비전(Explorer 즐겨찾기, 포트 즐겨찾기).
    #[cfg(feature = "gui")]
    pub(crate) shared_lists_announced: [u64; 2],
}

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
        journal_writer_lock: Option<tasty_event_store::WriterLock>,
        #[cfg(debug_assertions)] input_simulation_enabled: bool,
    ) -> anyhow::Result<Self> {
        let (stream_inbound_tx, stream_inbound_rx) = std::sync::mpsc::channel();
        let (screenshot_capture_tx, screenshot_capture_rx) = std::sync::mpsc::channel();
        let (image_upload_tx, image_upload_rx) = std::sync::mpsc::channel();
        let (transfer_progress_tx, transfer_progress_rx) = std::sync::mpsc::channel();
        let mut timers = tasty_timer::TimerHub::new();
        timers::register_steady_state(&mut timers, std::time::Instant::now());
        let timer_waker = tasty_timer::spawn_timer_waker({
            let proxy = proxy.clone();
            move || proxy.send_event(AppEvent::TimerTick).is_ok()
        });
        let journal = journal::JournalApplication::new_with_writer_lock(
            {
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::JournalReady).is_err() {
                        tracing::trace!("journal wake after GUI shutdown");
                    }
                })
            },
            journal_writer_lock,
        )?;
        let journal_metrics = journal.metrics_handle();
        Ok(Self {
            journal,
            pending_server_attaches: Vec::new(),
            preset_io: Default::default(),
            remote_io: Default::default(),
            pending_recovery_notices: Vec::new(),
            #[cfg(feature = "gui")]
            font_rejections: Default::default(),
            #[cfg(feature = "gui")]
            next_restore_notice_id: 0,
            settings_edit_owner: None,
            port_scans: Default::default(),
            #[cfg(feature = "gui")]
            explorer_files: Default::default(),
            tutorial_progress: Default::default(),
            state_db_wake: {
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::TimerTick).is_err() {
                        tracing::debug!("state writer wake after shutdown");
                    }
                })
            },
            dag_reads: dag_reads::DagReads::new({
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::TimerTick).is_err() {
                        tracing::debug!("DAG read wake after shutdown");
                    }
                })
            }),
            local_reads: local_reads::LocalReads::new({
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::TimerTick).is_err() {
                        tracing::debug!("local read wake after shutdown");
                    }
                })
            }),
            publication_inputs: Default::default(),
            gate_queue: Default::default(),
            services: crate::boot::wiring::build_production_core(memory, {
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::TimerTick).is_err() {
                        tracing::debug!("memory writer wake after shutdown");
                    }
                })
            })?
            .with_journal_metrics(journal_metrics),
            state: state::AppState {
                #[cfg(debug_assertions)]
                input_simulation_enabled,
                ..Default::default()
            },
            hub: Hub::new(port_file),
            stream_hub: tasty_ipc::stream_hub::StreamHub::new(),
            stream_inbound_tx,
            stream_inbound_rx,
            stream_wake: Default::default(),
            remote_file_jobs: Default::default(),
            view: ViewRegistry::new(proxy.clone()),
            engines: engine_registry::EngineRegistry::default(),
            boot: None,
            pending_window: None,
            shell_setup_gpu: None,
            shell_setup_window: None,
            boot_error_gpu: None,
            boot_error_window: None,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            tray_icon: None,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            tray_menu_ids: None,
            modal_shake: None,
            plugin_manager: None,
            plugin_files: Default::default(),
            lua_engine: crate::hooks::lua::init_engine(),
            lua_autofire: crate::hooks::autofire::AutofireGuard::new(),
            lua_runs: crate::hooks::lua_runs::LuaRuns::default(),
            #[cfg(feature = "gui")]
            scripts_after_save: Vec::new(),
            timers,
            timer_waker,
            ipc_pacer: crate::app::ipc::IpcPacer::new({
                let proxy = proxy.clone();
                Box::new(move || {
                    crate::shortcuts::send_app_event(&proxy, AppEvent::IpcReady);
                })
            }),
            stream_pacer: Default::default(),
            attach_client_wake: {
                let gate = tasty_ipc::stream_hub::StreamWakeGate::new();
                let proxy = proxy.clone();
                gate.connect(std::sync::Arc::new(move || {
                    if let Err(error) = proxy.send_event(AppEvent::AttachClientData) {
                        tracing::debug!("remote wake after event loop closed: {error}");
                    }
                }));
                gate
            },
            attach_client_pacer: Default::default(),
            pending_focus_hint_clear: std::collections::HashSet::new(),
            next_window_activation: None,
            pending_external_activation: None,
            remote: tasty_remote::outbound::Remote::new(),
            screenshot_workers: Default::default(),
            screenshot_capture_tx,
            screenshot_capture_rx,
            image_jobs: image_jobs::ImageWorker::new({
                let proxy = proxy.clone();
                Arc::new(move || {
                    if proxy.send_event(AppEvent::TimerTick).is_err() {
                        tracing::debug!("image job wake after shutdown");
                    }
                })
            }),
            image_upload_tx,
            image_upload_rx,
            transfer_progress_tx,
            transfer_progress_rx,
            gpu_instance: Arc::new(new_gpu_instance()),
            gpu_adapter: None,
            shared_lists_announced: [0, 0],
        })
    }

    /// GUI 자원 없이 mpsc waker로 시작한다.
    #[cfg(not(feature = "gui"))]
    pub(crate) fn new_headless(
        journal_wake: std::sync::Arc<dyn Fn() + Send + Sync>,
        port_file: Option<String>,
        memory: Option<std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>>,
    ) -> anyhow::Result<Self> {
        let (stream_inbound_tx, stream_inbound_rx) = std::sync::mpsc::channel();
        let mut timers = tasty_timer::TimerHub::new();
        timers::register_steady_state(&mut timers, std::time::Instant::now());
        let journal = journal::JournalApplication::new(journal_wake.clone())?;
        let journal_metrics = journal.metrics_handle();
        Ok(Self {
            journal,
            pending_server_attaches: Vec::new(),
            preset_io: Default::default(),
            publication_inputs: Default::default(),
            gate_queue: Default::default(),
            services: crate::boot::wiring::build_production_core_headless(memory, journal_wake)?
                .with_journal_metrics(journal_metrics),
            state: state::AppState::default(),
            hub: Hub::new(port_file),
            stream_hub: tasty_ipc::stream_hub::StreamHub::new(),
            stream_inbound_tx,
            stream_inbound_rx,
            stream_wake: Default::default(),
            remote_file_jobs: Default::default(),
            plugin_manager: None,
            plugin_files: Default::default(),
            lua_engine: crate::hooks::lua::init_engine(),
            lua_autofire: crate::hooks::autofire::AutofireGuard::new(),
            lua_runs: crate::hooks::lua_runs::LuaRuns::default(),
            #[cfg(feature = "gui")]
            scripts_after_save: Vec::new(),
            timers,
        })
    }

    /// 자동실행은 CoreState 초기화 전에도 호출될 수 있어 그때는 빈 레지스트리를 반환한다.
    #[cfg(feature = "gui")]
    pub(crate) fn autofire_scripts(&self) -> tasty_settings::ScriptRegistry {
        let engines = self.engines();
        engines
            .primary()
            .or_else(|| engines.parked().next())
            .map(|e| e.runtime.settings.scripts.clone())
            .unwrap_or_default()
    }

    /// 창별 GPU 상태를 만들되 instance와 첫 창에서 선택한 adapter는 공유한다.
    #[cfg(feature = "gui")]
    pub(crate) fn create_gpu_state(
        &mut self,
        window: Arc<Window>,
        appearance: &crate::settings::AppearanceSettings,
    ) -> anyhow::Result<GpuState> {
        let instance = Arc::clone(&self.gpu_instance);
        // 첫 창은 CoreState보다 먼저 만들어질 수 있어 없으면 기본 휠 거리를 사용한다.
        let wheel_line_scroll = self
            .engines()
            .primary()
            .map(|cs| cs.runtime.settings.general.wheel_line_scroll)
            .unwrap_or(tasty_settings::DEFAULT_WHEEL_LINE_SCROLL);
        // 모달의 CoreState가 아직 없을 수 있으므로 배율은 이 창의 appearance를 사용한다.
        let theme_runtime = tasty_themes::ThemeRuntime {
            ui_zoom: appearance.ui_scale_factor(),
            ..self
                .engines()
                .primary()
                .map(|cs| cs.runtime.settings.theme_runtime())
                .unwrap_or_default()
        };
        let proxy = self.view.proxy.clone();
        pollster::block_on(async move {
            if self.gpu_adapter.is_none() {
                let adapter = select_gpu_adapter(&instance, &window).await?;
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
}

/// 모든 창이 공유하는 wgpu instance를 만든다.
#[cfg(feature = "gui")]
pub(crate) fn new_gpu_instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    })
}

/// 창에 맞는 GPU adapter를 고른다. 하드웨어 선택에 실패하면 소프트웨어 adapter도 시도하고,
/// 그것도 없으면 [`NoGpuAdapter`]로 실패한다.
#[cfg(feature = "gui")]
pub(crate) async fn select_gpu_adapter(
    instance: &wgpu::Instance,
    window: &Arc<Window>,
) -> anyhow::Result<wgpu::Adapter> {
    // 어댑터 선택용 surface는 실제 surface를 만들기 전에 해제한다.
    let probe = instance.create_surface(window.clone())?;
    let opts = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: Some(&probe),
        force_fallback_adapter: false,
    };
    match instance.request_adapter(&opts).await {
        Some(adapter) => Ok(adapter),
        None => instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: true,
                ..opts
            })
            .await
            .ok_or_else(|| anyhow::Error::new(NoGpuAdapter)),
    }
}

pub(crate) mod anomaly_settings;
pub(crate) mod attach_heartbeat;
pub(crate) mod task_completion;
pub(crate) mod task_report;
pub(crate) mod terminal_input;

#[cfg(feature = "gui")]
pub(crate) mod engine_action;

#[cfg(feature = "gui")]
pub(crate) mod frame_effect;

#[cfg(feature = "gui")]
mod view_frame;

#[cfg(feature = "gui")]
mod plugin_surface_dirty;
#[cfg(feature = "gui")]
mod view_mesh;
#[cfg(feature = "gui")]
pub(crate) use view_mesh::MeshBootstrap;

#[cfg(feature = "gui")]
mod explorer_action;
#[cfg(feature = "gui")]
pub(crate) mod explorer_hidden;
#[cfg(feature = "gui")]
pub(crate) mod requests;
#[cfg(feature = "gui")]
mod surface_poll;

pub(crate) mod telemetry;

#[cfg(feature = "gui")]
pub(crate) mod settings_edit;

#[cfg(feature = "gui")]
pub(crate) mod remote_browser;

#[cfg(feature = "gui")]
pub(crate) mod plugin_display;

#[cfg(feature = "gui")]
mod preset_editor;
#[cfg(feature = "gui")]
pub(crate) mod settings_files;

mod preset_capture;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use preset_capture::shutdown_tests as preset_shutdown_tests;

#[cfg(feature = "gui")]
pub(crate) mod explorer_files;
pub(crate) mod file_jobs;
#[cfg(feature = "gui")]
mod port_scans;

#[cfg(feature = "gui")]
pub(crate) mod html_runtime;
#[cfg(all(
    feature = "gui",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub(crate) mod http_session;
#[cfg(feature = "gui")]
pub(crate) mod webview_sync;

pub(crate) mod remote_file_commands;
#[cfg(feature = "gui")]
pub(crate) mod remote_tool_files;
#[cfg(feature = "gui")]
mod system_notices;

impl App {
    /// Reap only workers whose stop has been requested; never wait for a running thread here.
    pub(crate) fn runner_stop_poll_deadline(&self) -> Option<std::time::Instant> {
        (self.services.tasks.poll_runner_stops() != 0)
            .then(|| std::time::Instant::now() + std::time::Duration::from_millis(10))
    }
}
