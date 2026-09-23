//! `App` 의 윈도우 라이프사이클 메서드.
//!
//! - `create_app_state`: GPU 상태 + 사이드바 폭으로부터 새 `AppState` 를 만든다.
//!   첫 호출 시 plugin manager 도 초기화하며, `pending_layout_restore` 가 있으면
//!   plugin 등록을 짧게 기다린 뒤 layout 을 복원한다.
//! - `register_window`: 만들어진 `MainView` 를 hash 에 등록 + (사용자 발화면) focused 로
//!   설정 + `window.created` host event / lua hook 발화.
//! - `create_new_window`: 다중 윈도우용 — 새 winit window + GPU + AppState (parked 우선) + 모달 안내.
//!
//! 첫 부팅(첫 윈도우)은 이 모듈의 동기 함수 대신 부팅 상태 머신
//! (`boot_machine.rs` — `begin_boot` → phase 스텝 → `finish_boot`)이 담당하며,
//! 여기의 `build_engine_and_plugins`(워커 본문) / `boot_pump_step_*` /
//! `boot_apply_pending_layout_restore` / `assemble_app_state` 를 공유한다.
//! `ensure_engine_and_plugins` 는 동기 wrapper — 동기 경로(다중 창)와 부팅
//! 머신의 워커 실패 fallback 이 쓴다.

use std::sync::Arc;

use winit::window::Window;

use crate::app::App;
use crate::app::event::WindowRequestOrigin;
use crate::gpu::GpuState;
use crate::{plugin, window};

/// 부팅 흐름의 테마 적용. `tasty-themes` 의 디스크 초기화 + 전역 Theme 설치를 한 단계로 묶는다.
///
/// 반환: `settings.appearance.theme` 가 디스크/캐시에 없어 mocha 로 fallback 된 경우 원래 요청 id.
/// (호출자가 InfoModal 로 사용자에게 알린다.)
/// `boot_apply_theme` 의 3단계(first_run_init/sync_builtin_themes/rescan)가
/// 반복하는 "실패해도 계속 진행 + warn 로그" 패턴을 통합.
fn warn_on_theme_err<T, E: std::fmt::Display>(step: &str, result: Result<T, E>) {
    if let Err(e) = result {
        tracing::warn!("{step} failed: {e}");
    }
}

pub(super) fn boot_apply_theme(settings: &mut tasty_settings::Settings) -> Option<String> {
    let appearance = &mut settings.appearance;
    warn_on_theme_err("themes first_run_init", tasty_themes::first_run_init());
    // 빌트인 테마(앱 소유)를 임베드 정본과 동기화 — 옛 스키마/색의 디스크 복사본을
    // 갱신한다. mocha 정본 보장도 겸한다(ensure_mocha_exists 의 상위 집합).
    warn_on_theme_err("sync_builtin_themes", tasty_themes::sync_builtin_themes());
    warn_on_theme_err("themes rescan", tasty_themes::rescan());
    let requested = appearance.theme.clone();
    tasty_themes::apply_theme(appearance, &requested);
    // 설정에서 오는 런타임 값(배율·모션 감소)을 실어 부팅 직후 steady state 도
    // 올바른 상태로 설치한다.
    tasty_themes::install_global_with_runtime(&settings.appearance, settings.theme_runtime());
    if settings.appearance.theme != requested {
        Some(requested)
    } else {
        None
    }
}

/// 부팅 시 터미널 그리드 크기 계산 — GPU cell metrics + 사이드바 폭 의존이라
/// 메인 스레드 몫이다. `ensure_engine_and_plugins`(동기)와 부팅 상태 머신의
/// 워커 spawn(cols/rows 를 정수 2개로 뽑아 전달)이 공유한다.
pub(super) fn boot_grid_size(
    gpu: &GpuState,
    sidebar_width: tasty_type_geometry::length::LogicalPx,
) -> (usize, usize) {
    let sf = gpu.scale_factor();
    let size = gpu.size();
    let sidebar_w = sidebar_width.to_physical(sf);
    let terminal_rect = crate::model::PhysicalRect {
        x: sidebar_w,
        y: tasty_type_geometry::length::PhysicalPx(0.0),
        width: (tasty_type_geometry::length::PhysicalPx(size.width as f32) - sidebar_w)
            .max(tasty_type_geometry::length::PhysicalPx(1.0)),
        height: tasty_type_geometry::length::PhysicalPx(size.height as f32),
    };
    gpu.grid_size_for_rect(&terminal_rect)
}

/// 엔진(CoreState)+plugin manager 원자 초기화(T2.6·T3)의 App-free 본문 —
/// **첫 부팅 전용** (두 번째 창의 글로벌 Arc 공유 분기는
/// `ensure_engine_and_plugins` 에 남아 있다). `App` 참조가 없어 부팅 상태
/// 머신이 워커 스레드에서 실행하며(`boot_machine.rs` 의 WaitingEngine),
/// 동기 wrapper 의 첫 부팅 분기도 같은 본문을 쓴다.
///
/// 실패: `CoreState::new_with_ids`(셸 spawn 등) 실패를 `Err` 로 반환한다. 워커에서
/// 돌 때는 그 `Err` 가 결과 채널로 전달돼 `WaitingEngine` 이 진단 후 정상 종료하고,
/// 동기 wrapper 경로는 caller 로 전파된다(패닉시키지 않는다).
pub(super) fn build_engine_and_plugins(
    cols: usize,
    rows: usize,
    factory: crate::waker::SharedWakerFactory,
    proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>,
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    layout_slot: crate::core::layout_persistence::LayoutSlotId,
    // 프로세스 게이지 핸들. `Core` 를 통째로 넘기지 않는 것이 이 본문이 App-free 인
    // 이유이고, `Arc` 두 개의 묶음은 그 성질을 안 깬다.
    gauges: crate::core::PluginGauges,
    #[cfg(debug_assertions)] input_simulation_enabled: bool,
) -> anyhow::Result<(crate::core::CoreState, plugin::PluginManager)> {
    let engine = build_core_state_first_boot(
        cols,
        rows,
        factory.clone(),
        proxy,
        memory,
        layout_slot,
        #[cfg(debug_assertions)]
        input_simulation_enabled,
    )?;
    let mgr = build_plugin_manager(factory, &engine, gauges);
    Ok((engine, mgr))
}

/// 첫 부팅의 CoreState 생성 (T2.6 계측 포함). 공유 source 없음 —
/// `CoreState::new` 의 기본 Arc 사용. preset_store 는 Core 가 유일 owner
/// 이며 engine 에는 더 이상 없다.
fn build_core_state_first_boot(
    cols: usize,
    rows: usize,
    factory: crate::waker::SharedWakerFactory,
    proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>,
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    layout_slot: crate::core::layout_persistence::LayoutSlotId,
    #[cfg(debug_assertions)] input_simulation_enabled: bool,
) -> anyhow::Result<crate::core::CoreState> {
    // 레이아웃 슬롯 로드 등 디스크 I/O 포함 — T2↔T3 갭의 두 번째 기여자라 별도
    // 계측 (첫 번째는 begin_boot 의 db+theme). scrollback orphan GC 는 여기 없다 —
    // 전 슬롯 union 으로 부팅 1 회만 돈다(`begin_boot` 초입).
    let t_engine = std::time::Instant::now();
    let waker: crate::terminal::Waker = factory.make_default_waker();
    // engine 생성 실패(= 사용자 shell 경로 오타·PTY/fd 고갈 등)를 패닉으로 올리지
    // 않고 caller 로 반환한다. 부팅은 진단을 로그로 내고 실패 화면을 그렸다가 사용자가
    // 닫을 때 종료하고, 새 창은 그 창만 취소한다 — 사용자 조작발이면 InfoModal 로 안내,
    // 에이전트 IPC 발이면 요청자에게 응답 에러로만 돌려준다(`notify_window_creation_failed`).
    let mut engine =
        crate::core::CoreState::new_with_ids(cols, rows, waker, None, Some(layout_slot), memory)?;
    engine.waker_factory = Some(factory);
    // 첫 부팅 — identify_worker 는 App proxy 가 필요.
    engine.identify_worker = Some(Arc::new(crate::identify_worker::IdentifyWorker::new(
        engine.file_format.clone(),
        proxy,
    )));
    #[cfg(debug_assertions)]
    {
        engine.input_simulation_enabled = input_simulation_enabled;
    }
    tracing::info!(
        target: "tasty::boot",
        ms = t_engine.elapsed().as_secs_f64() * 1000.0,
        "T2.6 engine_init (CoreState::new_with_ids + layout slot load)"
    );
    Ok(engine)
}

/// PluginManager 생성 + builtin 설치·discovery·spawn (T3a·T3b 계측 포함).
/// engine 의 registry Arc 들을 공유해 만든다 — App-free.
fn build_plugin_manager(
    factory: crate::waker::SharedWakerFactory,
    engine: &crate::core::CoreState,
    gauges: crate::core::PluginGauges,
) -> plugin::PluginManager {
    let mut mgr = plugin::PluginManager::with_registries(
        factory,
        engine.file_format.clone(),
        engine.file_handler.clone(),
    );
    // 이 자리를 빠뜨리면 매니저가 아무것도 안 세고, `system.pressure` 의 왕복 덩어리가
    // `matched: 0` 으로 남는다 — 그 0 은 "plugin 을 안 기다렸다" 처럼 읽힌다. 느린 요청
    // 링도 같다: 빠뜨리면 호스트 몫만 남고 plugin 대기가 원 요청 줄에 안 붙는다.
    mgr.set_plugin_wait(gauges.plugin_wait);
    mgr.set_slow_requests(gauges.slow_requests);
    mgr.set_surface_registry(engine.surface_registry.clone());
    mgr.set_i18n_registrar(std::sync::Arc::new(crate::i18n::BinI18nRegistrar));
    // 공유 훅 핸들러 레지스트리(전역 싱글턴) port 주입 — plugin enable/disable
    // 시 `[[contributes.hook_handler]]` 를 등록/해제한다(S11).
    mgr.set_hook_handler_registry(std::sync::Arc::new(
        crate::hook_handler::HostHookHandlerPort,
    ));
    // 완료 판정 전략 레지스트리(전역 싱글턴) port 주입 — plugin enable/disable
    // 시 `[[contributes.completion_strategy]]` 를 등록/해제한다.
    mgr.set_completion_strategy_registry(std::sync::Arc::new(
        crate::completion_strategy::HostCompletionStrategyPort,
    ));
    // T3 은 discovery 와 spawn 을 나눠 찍는다 — 4부 escalate 확정 시 어느
    // 쪽이 병목인지 판단하기 위함.
    let t3 = std::time::Instant::now();
    plugin::install_builtins_if_needed(&mut mgr);
    // 소유 표를 해소하는 crate 에 **같은 표**를 넘긴다 — 사본이 아니다.
    // 이 설치가 없으면 `method_meta` 는 어떤 plugin prefix 도 모르는 상태로 남고,
    // 그러면 plugin namespace 메서드가 권한 검사에서 "모르는 메서드" 가 된다.
    // 설치 자체는 1 회이고, 표의 내용은 그 뒤 `refresh_packages` 가 채운다.
    mgr.install_namespace_table_once();
    mgr.refresh_packages();
    tracing::info!(
        target: "tasty::boot",
        ms = t3.elapsed().as_secs_f64() * 1000.0,
        "T3a plugin_discovery (install_builtins + refresh_packages)"
    );
    let t3b = std::time::Instant::now();
    mgr.discover_and_start();
    tracing::info!(
        target: "tasty::boot",
        ms = t3b.elapsed().as_secs_f64() * 1000.0,
        total_ms = t3.elapsed().as_secs_f64() * 1000.0,
        "T3b plugin_spawn (discover_and_start; total_ms = T3 전체)"
    );
    mgr
}

/// 어떤 창을 열려다 실패했는지 — 안내 문구를 고르는 유일한 기준이다.
///
/// 지점마다 문구가 달라야 한다: 설정 창이 안 열렸는데 "새 창을 열 수 없습니다" 가 뜨면
/// 그냥 틀린 안내다. 종료 확인 모달은 여기 없다 — 그 실패는 안내가 아니라
/// `begin_shutdown` 폴백으로 처리한다(ADR-0117).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowCreationTarget {
    NewWindow,
    Settings,
    Plugins,
}

impl WindowCreationTarget {
    fn title_key(self) -> &'static str {
        match self {
            Self::NewWindow => "window_error.new_window.title",
            Self::Settings => "window_error.settings.title",
            Self::Plugins => "window_error.plugins.title",
        }
    }

    fn body_key(self) -> &'static str {
        match self {
            Self::NewWindow => "window_error.new_window.body",
            Self::Settings => "window_error.settings.body",
            Self::Plugins => "window_error.plugins.body",
        }
    }
}

impl App {
    /// Create an AppState from a GPU state, computing grid size from the sidebar width.
    ///
    /// 동기 경로 (다중 창 `create_new_window` 등). 첫 부팅은 이 함수 대신 부팅
    /// 상태 머신(`boot_machine.rs`)이 같은 하위 단계(`ensure_engine_and_plugins` /
    /// `boot_pump_step_*` / `boot_apply_pending_layout_restore` / `assemble_app_state`)
    /// 를 프레임 단위로 나눠 태운다 — 의미론은 이 함수와 동일해야 한다.
    pub(crate) fn create_app_state(
        &mut self,
        gpu: &GpuState,
        sidebar_width: tasty_type_geometry::length::LogicalPx,
    ) -> anyhow::Result<crate::state::AppState> {
        self.ensure_engine_and_plugins(gpu, sidebar_width)?;

        // pending_layout_restore 가 있으면: wait-for-plugin loop 를 거쳐 등록
        // 대기 → `DomainIntent::ApplyPendingLayoutRestore` 발화. Intent 본문
        // (Core::apply) 안에서 take + restore + restored_active_workspace 추출이
        // 한 번에 일어난다 — caller 는 events 만 검사.
        //
        // Intent 큐 우회 직접 apply — bootstrap context (main loop 진입 전) 라
        // 큐 drain 이 일어나지 않는다.
        let restored_idx_after_layout = if self.core_state().pending_layout_restore.is_some() {
            self.boot_wait_for_required_plugin_kinds();
            let restored = self.boot_apply_pending_layout_restore();
            self.boot_wait_for_remote_surface_restores();
            restored
        } else {
            None
        };

        // 복원이 워크스페이스를 하나도 만들지 못한 경우의 안전망. 복원 예정이면
        // `new_with_ids` 가 기본 워크스페이스를 만들지 않으므로 여기서 메운다.
        // 복원이 정상이면 no-op.
        let bootstrapped = match self.core_state.as_mut() {
            Some(engine) => Self::bootstrap_workspace_if_empty(&mut self.core, engine),
            None => None,
        };

        Ok(self.assemble_app_state(bootstrapped.or(restored_idx_after_layout)))
    }

    /// 엔진(CoreState)·plugin manager 의 원자 초기화 — `create_app_state` 선두 절반.
    /// 두 블록 모두 `is_none` 가드라 재호출은 no-op (다중 창 경로 안전).
    ///
    /// 동기 wrapper — 첫 부팅 본문은 App-free `build_engine_and_plugins`(부팅
    /// 상태 머신의 워커와 동일 본문)로 위임하고, 여기는 두 번째 main window 의
    /// 글로벌 Arc 공유 분기 + self 장착을 담당한다. 호출자: 동기 경로
    /// (`create_app_state` / `create_new_window`) + 부팅 상태 머신의 워커
    /// disconnect fallback (T2.6·T3 계측 포함).
    pub(super) fn ensure_engine_and_plugins(
        &mut self,
        gpu: &GpuState,
        sidebar_width: tasty_type_geometry::length::LogicalPx,
    ) -> anyhow::Result<()> {
        let (cols, rows) = boot_grid_size(gpu, sidebar_width);
        let factory: crate::waker::SharedWakerFactory = Arc::new(
            crate::waker_factory_winit::WinitWakerFactory::new(self.view.proxy.clone()),
        );
        // `claim_free_layout_slot` 은 `&self` 를 빌리므로 `&mut self` 아래에서 쓰기
        // 전에 값으로 먼저 뽑아둔다. 점유는 아래 engine 생성으로 확정된다.
        let layout_slot = self.claim_free_layout_slot();

        // CoreState를 App 직속에 1회 init.
        if self.core_state.is_none() {
            // 두 번째 main window 생성 시: 첫 engine 의 글로벌 Arc 들을 공유한다.
            // surface_registry 는 plugin_manager 가 첫 부팅 시 set 한 것과 같은
            // Arc 여야 plugin 이 register 한 surface kind 가 두 번째 윈도우에서도
            // 보임. file_format / file_handler 도 동일 — plugin contribute 한
            // file 동작이 두번째 윈도우에서 누락 안 되도록.
            //
            // 첫 부팅 시점에는 source 없음 → App-free 본문
            // (`build_core_state_first_boot`)이 CoreState::new 의 기본 Arc 사용.
            let shared = self.any_main_engine().map(|src| {
                (
                    src.surface_registry.clone(),
                    src.file_format.clone(),
                    src.file_handler.clone(),
                    src.identify_worker.clone(),
                    src.approval_store.clone(),
                    src.telemetry_seq.clone(),
                    src.anomaly_detector.clone(),
                    src.agent_seq.clone(),
                    src.next_ids.clone(),
                )
            });

            let engine = if let Some((
                surface_registry,
                file_format,
                file_handler,
                identify_worker,
                approval_store,
                telemetry_seq,
                anomaly_detector,
                agent_seq,
                next_ids,
            )) = shared
            {
                // 레이아웃 슬롯 로드 등 디스크 I/O 포함 (T2.6). scrollback orphan GC 는
                // 여기서 하지 않는다 — 전 슬롯 union 으로 부팅 1 회만 돈다
                // (`layout_persistence::migrate_and_gc_on_boot`, `begin_boot` 초입).
                let t_engine = std::time::Instant::now();
                // IdGenerator 는 CoreState::new 시점에 default workspace 만들면서
                // 첫 ID 들 발급하므로, **생성 전에** source 의 next_ids 를 주입해야
                // workspace_id/pane_id/tab_id/surface_id 충돌이 안 난다.
                let waker: crate::terminal::Waker = factory.make_default_waker();
                let mut engine = crate::core::CoreState::new_with_ids(
                    cols,
                    rows,
                    waker,
                    Some(next_ids),
                    Some(layout_slot),
                    self.core.memory_arc(),
                )?;
                engine.waker_factory = Some(factory.clone());
                engine.surface_registry = surface_registry;
                engine.file_format = file_format;
                engine.file_handler = file_handler;
                engine.identify_worker = identify_worker;
                engine.approval_store = approval_store;
                engine.telemetry_seq = telemetry_seq;
                engine.anomaly_detector = anomaly_detector;
                engine.agent_seq = agent_seq;
                #[cfg(debug_assertions)]
                {
                    engine.input_simulation_enabled = self.input_simulation_enabled;
                }
                tracing::info!(
                    target: "tasty::boot",
                    ms = t_engine.elapsed().as_secs_f64() * 1000.0,
                    "T2.6 engine_init (CoreState::new_with_ids + layout slot load)"
                );
                engine
            } else {
                build_core_state_first_boot(
                    cols,
                    rows,
                    factory.clone(),
                    self.view.proxy.clone(),
                    self.core.memory_arc(),
                    layout_slot,
                    #[cfg(debug_assertions)]
                    self.input_simulation_enabled,
                )?
            };
            self.core_state = Some(engine);
        }

        if self.plugin_manager.is_none() {
            let gauges = self.core.plugin_gauges();
            let mgr = build_plugin_manager(factory, self.core_state(), gauges);
            self.plugin_manager = Some(mgr);
        }
        Ok(())
    }

    /// `DomainIntent::ApplyPendingLayoutRestore` 1회 apply (T5 계측 포함).
    /// take + restore + restored_active_workspace 추출은 Intent 본문(Core::apply)
    /// 안에서 한 번에 일어난다 — 단일 take 보장. 반환: 복원된 활성 workspace idx.
    pub(super) fn boot_apply_pending_layout_restore(&mut self) -> Option<usize> {
        let t5 = std::time::Instant::now();
        let engine = self
            .core_state
            .as_mut()
            .expect("core_state must be initialized before layout restore");
        let restored = match self.core.apply(
            engine,
            crate::core::intent::DomainIntent::ApplyPendingLayoutRestore,
        ) {
            Ok(events) => events.into_iter().find_map(|e| {
                if let crate::core::intent::CoreEvent::LayoutRestored {
                    restored: true,
                    active_workspace,
                } = e
                {
                    tracing::info!("Layout restored from slot file (deferred)");
                    active_workspace
                } else {
                    None
                }
            }),
            Err(e) => {
                tracing::warn!("ApplyPendingLayoutRestore failed: {e}");
                None
            }
        };
        tracing::info!(
            target: "tasty::boot",
            ms = t5.elapsed().as_secs_f64() * 1000.0,
            "T5 layout_apply (ApplyPendingLayoutRestore)"
        );
        restored
    }

    /// AppState 조립 — `create_app_state` 의 마지막 절반. 부팅 상태 머신의 Ready
    /// 합류(finish_boot)와 동기 경로가 공유한다.
    pub(super) fn assemble_app_state(
        &mut self,
        restored_idx_after_layout: Option<usize>,
    ) -> crate::state::AppState {
        let preset_store = self.core.preset_store.clone();
        let memory = self.core.memory_arc();
        let mut state = crate::state::AppState::new(self.core_state_mut(), preset_store, memory);
        if let Some(restored_idx) = restored_idx_after_layout {
            state.switch_workspace(self.core_state_mut(), restored_idx);
        }
        if let Some(mgr) = self.plugin_manager.as_ref() {
            state
                .tool_registry
                .set_plugin_items(mgr.plugin_tool_items());
            // 바로 위 tool_registry 와 같은 시점·같은 이유의 초기 populate — 이후 라이프사이클 변경은
            // `App::refresh_palette_plugin_commands`(같은 `tool_registry_dirty`
            // 트리거)가 갱신하지만, 첫 창 조립 시점엔 그 경로를 거치지 않으므로
            // 여기서 한 번 채워야 새로 뜬 창의 팔레트가 처음부터 plugin 명령을 본다.
            state.palette_plugin_commands = mgr.plugin_palette_commands();
        }
        state
    }

    /// wait-for-plugin: pending layout restore 가 요구하는 plugin surface kind
    /// 들이 등록될 때까지 pump + sleep 폴링 (deadline 300ms).
    /// `required_plugin_kinds` 만 peek (take 안 함) — Intent 본문이 단일 take 를
    /// 보장. T4 부팅 계측 포함 (탈출 사유: satisfied / deadline).
    fn boot_wait_for_required_plugin_kinds(&mut self) {
        use crate::app::boot_machine::PLUGIN_WAIT_DEADLINE;
        use std::time::{Duration, Instant};
        let needed = self.boot_required_plugin_kinds();
        let t4 = Instant::now();
        let deadline = t4 + PLUGIN_WAIT_DEADLINE;
        let mut t4_reason = "deadline";
        while Instant::now() < deadline {
            if self.boot_pump_step_plugins_registered(&needed) {
                t4_reason = "satisfied";
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        tracing::info!(
            target: "tasty::boot",
            ms = t4.elapsed().as_secs_f64() * 1000.0,
            reason = t4_reason,
            deadline_ms = PLUGIN_WAIT_DEADLINE.as_millis() as u64,
            "T4 layout_wait_plugins"
        );
    }

    /// pending layout restore 가 요구하는 plugin surface kind 목록 peek (take 안 함).
    pub(super) fn boot_required_plugin_kinds(&self) -> Vec<String> {
        self.core_state()
            .pending_layout_restore
            .as_ref()
            .map(|s| s.required_plugin_kinds())
            .unwrap_or_default()
    }

    /// wait-for-plugin 1스텝: pump → `finalize_plugin_hello` → 필요 kind 전수 등록
    /// 확인. 동기 루프(위)와 부팅 상태 머신의 WaitingPlugins 스텝이 공유한다.
    /// 반환: 필요 kind 가 전부 등록됐는가 (satisfied).
    pub(super) fn boot_pump_step_plugins_registered(&mut self, needed: &[String]) -> bool {
        let hello_pairs = if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.pump(std::time::Instant::now())
        } else {
            Vec::new()
        };
        self.finalize_plugin_hello(hello_pairs);
        let engine = self.core_state();
        needed
            .iter()
            .all(|k| engine.surface_registry.get_live(k).is_some())
    }

    /// ApplyPendingLayoutRestore 가 RemoteSurface 들을 생성하고
    /// `HostCmd::RemoteSurfaceRestored` 를 큐잉했다. pump 를 추가로 돌려
    /// 송신 → plugin 응답 round-trip 이 끝날 때까지 대기한다. 이게 끝나야
    /// RemoteSurface 의 snapshot_cache 가 plugin 의 최신 값으로 갱신된
    /// 상태로 main loop 에 진입 — 사용자 동작 race 가 사라진다. carry 값이
    /// 이미 안전망 역할을 하므로 (1) 레이아웃 슬롯 오염은 이 wait 와 무관하게
    /// 차단된 상태이고, 이 wait 는 부팅 직후 사용자 동작이 응답으로 덮어
    /// 씌워지는 깜박임/덮어쓰기를 추가로 방지하는 목적.
    ///
    /// deadline: plugin 이 panic/hang 등으로 영영 응답 안 보내는 케이스
    /// 보호. 초과해도 (1) carry 덕에 layout 손상은 없음.
    /// T6 부팅 계측 포함 (탈출 사유: satisfied / deadline).
    fn boot_wait_for_remote_surface_restores(&mut self) {
        use std::time::{Duration, Instant};
        let t6 = Instant::now();
        let deadline = t6 + Duration::from_millis(500);
        let mut t6_reason = "deadline";
        while Instant::now() < deadline {
            if self.boot_pump_step_remote_restores_done() {
                t6_reason = "satisfied";
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        tracing::info!(
            target: "tasty::boot",
            ms = t6.elapsed().as_secs_f64() * 1000.0,
            reason = t6_reason,
            "T6 remote_surface_wait (deadline 500ms)"
        );
    }

    /// RemoteSurface 복원 round-trip 대기 1스텝. **pump 는 조건 확인 전 무조건
    /// 1회** (1차 스텝과 미묘하게 다름 — hello 가 비어도 pump 가 send/recv 를
    /// 진행시켜야 round-trip 이 끝난다). 동기 루프(위)와 부팅 상태 머신의
    /// RestoringLayout 스텝이 공유한다. 반환: 더 이상 pending 이 없는가.
    pub(super) fn boot_pump_step_remote_restores_done(&mut self) -> bool {
        let still_pending = if let Some(mgr) = self.plugin_manager.as_mut() {
            let hello_pairs = mgr.pump(std::time::Instant::now());
            if !hello_pairs.is_empty() {
                self.finalize_plugin_hello(hello_pairs);
            }
            self.plugin_manager
                .as_ref()
                .is_some_and(|m| m.has_pending_surface_restores())
        } else {
            false
        };
        !still_pending
    }

    /// Register a MainView. 사용자 발화 창이면 focused 로 설정하고, 에이전트 발화 창이면
    /// focused 를 그대로 둔다([`focus_after_register`]).
    pub(crate) fn register_window(
        &mut self,
        gpu: GpuState,
        state: crate::state::AppState,
        core_state: crate::core::CoreState,
        window: Arc<Window>,
        origin: WindowRequestOrigin,
    ) {
        let window_id = window.id();
        let main =
            window::main::MainView::new(gpu, state, core_state, window, self.view.proxy.clone());
        self.view.views.insert(window_id, Box::new(main));
        self.view.focused_view_id =
            focus_after_register(self.view.focused_view_id, window_id, origin);
        let scripts = self.autofire_scripts();
        if let Some(mgr) = self.plugin_manager.as_mut() {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::{WindowCreated, WindowModality};
            let payload = WindowCreated {
                window_id: u64::from(window_id),
                kind: "main".to_string(),
                modality: WindowModality::Modeless,
            };
            mgr.emit_host_event("window.created", &payload, EventScope::System);
            crate::hooks::lua::fire(
                self.lua_engine.as_ref(),
                crate::hooks::lua::AutofireCtx {
                    scripts: &scripts,
                    guard: &mut self.lua_autofire,
                },
                "window.create.post",
                &payload,
            );
        }
    }

    /// focused main 창의 winit 창(에이전트 창을 그 뒤에 둘 기준). 없으면 `None`.
    fn focused_main_winit(&self) -> Option<Arc<Window>> {
        let id = self.view.focused_view_id?;
        let view = self.view.views.get(&id)?;
        view.as_main().map(|_| view.base().winit.clone())
    }

    /// Create a new window with its own terminal.
    /// 새 창을 만든다. 성공하면 새 창의 `WindowId`, 실패하면 사람이 읽을 원인 문자열을
    /// 돌려준다 — IPC 요청자(`AppEvent::CreateWindow` 의 완료 채널)가 이 결과를 그대로
    /// 응답에 싣는다(ADR-0122). 사용자 경로(menu/tray)는 완료 채널이 없어 반환값을 쓰지
    /// 않고, 실패 안내는 `notify_window_creation_failed` 가 모달로 띄운다.
    pub(crate) fn create_new_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        origin: WindowRequestOrigin,
    ) -> Result<winit::window::WindowId, String> {
        use winit::window::WindowAttributes;

        let title = if cfg!(debug_assertions) {
            "Tasty (Debug)"
        } else {
            "Tasty"
        };
        let mut attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            .with_min_inner_size(winit::dpi::LogicalSize::new(640, 480));
        if let Some(icon) = crate::app_icon::winit_window_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }
        // CSD: macOS 는 fullsize-content-view(네이티브 신호등 유지). 그 외 OS no-op.
        attrs = crate::platform::window_chrome::apply_csd_attributes(attrs);
        // 에이전트가 만든 창은 숨긴 채 만들어 등록 뒤 사용자 창 뒤에 보인다(원칙 1·3,
        // ADR-0497). 사용자 창은 지금까지와 같다.
        attrs = origin_window_attributes(attrs, origin);

        // 새 창 생성 실패는 패닉이 아니다 — 이미 떠 있는 창들의 세션을 죽이지 않도록,
        // 기존 창에 안내를 띄우고 새 창만 취소한다.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                return Err(self.notify_window_creation_failed(
                    WindowCreationTarget::NewWindow,
                    origin,
                    "failed to create new window",
                    e,
                ));
            }
        };
        window.set_ime_allowed(true);

        // Windows 절전(suspend/resume) 감지 — WM_POWERBROADCAST 후킹. resume 시
        // 죽은 ConPTY 자식 정리 + 살아있는 자식 wake nudge (ADR-0017). power
        // broadcast 는 시스템 전역이라 어느 윈도우든 받으므로, 창마다 설치해 두면
        // ≥1 개 창이 살아있는 한 동작한다 (resume 헬스 패스는 idempotent).
        #[cfg(windows)]
        {
            let proxy = self.view.proxy.clone();
            crate::platform::power_windows::install_resume_hook(
                &window,
                Box::new(move || {
                    crate::shortcuts::send_app_event(&proxy, crate::AppEvent::SystemResumed);
                }),
            );
        }

        // state.db 초기화. 실패하면 InfoModal로 안내 후 종료(Exit 1).
        // create_app_state 이전에 호출해야 plugin/recent_files 등이 정상 동작.
        let db_init_error = crate::db::init().err();

        let (settings, invalid_theme_name) = boot_load_and_normalize_settings();
        // GPU 초기화 실패(어댑터 부재 등) — 부팅 경로처럼 안내 후 창을 취소한다.
        // 부팅과 달리 종료하지 않는다(기존 창이 계속 그려져야 한다). window 는 여기서
        // 반환하며 drop 돼 OS 창이 닫힌다.
        let gpu = match self.create_gpu_state(window.clone(), &settings.appearance) {
            Ok(g) => g,
            Err(e) => {
                return Err(self.notify_window_creation_failed(
                    WindowCreationTarget::NewWindow,
                    origin,
                    "failed to initialize GPU for new window",
                    e,
                ));
            }
        };

        // 엔진 생성 실패의 유일한 `?` 는 셸 spawn 이다 — 사용자 `config.toml` 의 shell
        // 경로 오타·PTY/fd 고갈이 여기로 온다. 패닉시키면 실행 중인 모든 세션이 사라지므로,
        // 안내 후 새 창만 취소한다(window·gpu 는 반환하며 drop).
        let (mut state, mut core_state) =
            match self.acquire_app_state_and_engine(&gpu, settings.appearance.sidebar_width) {
                Ok(pair) => pair,
                Err(e) => {
                    return Err(self.notify_window_creation_failed(
                        WindowCreationTarget::NewWindow,
                        origin,
                        "failed to create engine for new window",
                        e,
                    ));
                }
            };
        self.ensure_at_least_one_workspace(&mut core_state, &mut state);

        // DB 초기화 실패 알림. 가장 먼저 푸시해서 큐 head에 둠 → [확인] 시 Exit(1).
        if let Some(err) = db_init_error {
            crate::adapters::ui::info_modal::show_info_modal(
                &mut state,
                build_db_init_error_modal(&err),
            );
        }

        // Theme fallback 알림 (잘못된 theme 이름이었던 경우).
        if let Some(invalid) = invalid_theme_name {
            crate::adapters::ui::info_modal::show_info_modal(
                &mut state,
                build_theme_fallback_modal(&invalid),
            );
        }

        // register_window 가 window 을 consume 하므로 id 를 먼저 캡처 — IPC 요청자에게
        // 돌려줄 window_id 다(ADR-0122). window.list 와 동일한 u64 변환을 쓴다.
        let window_id = window.id();
        // 에이전트 창을 둘 자리 — 사용자가 보던 창. register_window 전에 잡는다.
        let behind = matches!(origin, WindowRequestOrigin::Agent)
            .then(|| (window.clone(), self.focused_main_winit()));
        self.register_window(gpu, state, core_state, window, origin);
        if let Some((window, anchor)) = behind {
            show_agent_window(&window, anchor.as_deref());
            self.pending_focus_hint_clear.insert(window_id);
        }
        tracing::info!("created new window {window_id:?} ({origin:?})");
        Ok(window_id)
    }

    /// 창(또는 설정/플러그인/종료 모달) 생성 실패를 기존 창에 안내한다. 새 창은
    /// 그릴 수 없으므로(엔진·GPU 가 없다) 안내를 **이미 떠 있는** focused 창에 띄운다
    /// — 그 창의 세션은 그대로 살아 있다. 띄울 창이 하나도 없으면(첫 부팅 이전 등)
    /// 로그만 남긴다 — 그 경우는 부팅 경로가 별도로 종료를 판단한다.
    /// 창(또는 설정/플러그인/종료 모달) 생성 실패를 처리하고, 요청자에게 돌려줄 사람이
    /// 읽을 원인 문자열을 반환한다.
    ///
    /// `User`(메뉴·단축키·tray) 는 방금 그 조작의 결과이므로 **이미 떠 있는** 창에
    /// InfoModal 로 알린다(실패한 창은 엔진·GPU 가 없어 그릴 수 없다). `Agent`(IPC) 발
    /// 실패는 화면을 건드리지 않는다 — 반환한 문자열이 완료 채널로 요청자에게 간다.
    /// 예전엔 Agent 도 toast 로 알렸으나, 사용자가 요청하지도 않은 일의 실패 통지가
    /// 화면에 뜨는 것 자체가 원칙 1 위반이고 동기 응답이 생긴 지금은 불필요하다
    /// (ADR-0117 재검토, ADR-0122).
    pub(super) fn notify_window_creation_failed(
        &mut self,
        target: WindowCreationTarget,
        origin: crate::app::event::WindowRequestOrigin,
        context: &str,
        err: impl std::fmt::Display,
    ) -> String {
        use crate::app::event::WindowRequestOrigin;

        tracing::error!("{context}: {err}");
        let body = crate::i18n::t_fmt(target.body_key(), &err.to_string());

        // Agent 발 실패는 요청자에게 IPC 응답으로만 돌려준다 — 사용자 화면 무변경.
        if matches!(origin, WindowRequestOrigin::Agent) {
            return body;
        }

        // User: 이미 떠 있는 창에 모달로 안내. 메인 창이 하나도 없으면(첫 부팅 이전 등)
        // 로그만 남긴다 — 그 경우는 부팅 경로가 별도로 종료를 판단한다.
        if let Some(view) = self.notice_window_mut() {
            let modal = crate::adapters::ui::info_modal::InfoModal {
                title: crate::i18n::t(target.title_key()).to_string(),
                body: body.clone(),
                on_close: crate::adapters::ui::info_modal::InfoModalAction::Continue,
                extra_buttons: Vec::new(),
            };
            crate::adapters::ui::info_modal::show_info_modal(&mut view.state, modal);
        } else {
            tracing::error!(
                "no main window to surface the window-creation failure notice ({context})"
            );
        }
        body
    }

    /// `create_new_window` 지원 — parked state 가 있으면 재사용, 없으면
    /// `create_app_state`(+ `App.core_state` take)로 새로 만든다.
    fn acquire_app_state_and_engine(
        &mut self,
        gpu: &GpuState,
        sidebar_width: tasty_type_geometry::length::LogicalPx,
    ) -> anyhow::Result<(crate::state::AppState, crate::core::CoreState)> {
        // Reuse parked state if available (restoring previous session)
        let (state, parked_engine) = if !self.parked_states.is_empty() {
            let parked = self.parked_states.remove(0);
            tracing::info!(
                "restoring parked state, {} remaining",
                self.parked_states.len()
            );
            let (st, eng) = parked;
            (st, Some(eng))
        } else {
            let st = self.create_app_state(gpu, sidebar_width)?;
            (st, None)
        };

        // 새 윈도우의 engine: parked 가 있으면 그쪽을 재사용, 없으면 App.core_state
        // 를 take. create_app_state 가 항상 self.core_state 를 set 하므로
        // 두 번째 main window 생성 시에도 새 engine 이 만들어져 들어와 있음
        // (글로벌 Arc 들은 첫 engine 과 공유 — create_app_state 의 shared 분기 참조).
        //
        // parked engine 의 `layout_slot` 은 **재배정하지 않는다**. parked 는 이미
        // 슬롯을 들고 있던 engine 을 통째로 되살리는 것이라, 새 슬롯을 주면 자기
        // 레이아웃을 놔두고 남의 슬롯 파일을 덮어쓴다. 위 분기에서 parked 가 있으면
        // `create_app_state`(→ `ensure_engine_and_plugins`) 자체를 타지 않으므로
        // 중복 배정도 일어나지 않는다.
        let core_state = match parked_engine {
            Some(e) => e,
            None => self
                .core_state
                .take()
                .expect("App.core_state must be present to register a main window"),
        };
        Ok((state, core_state))
    }

    /// `create_new_window` 지원 — 새 윈도우의 engine 이 워크스페이스가 하나도 없으면
    /// (parked 재사용이 아닌 신규 생성 경로) 기본 워크스페이스를 부트스트랩한다.
    fn ensure_at_least_one_workspace(
        &mut self,
        core_state: &mut crate::core::CoreState,
        state: &mut crate::state::AppState,
    ) {
        if let Some(idx) = Self::bootstrap_workspace_if_empty(&mut self.core, core_state) {
            state.active_workspace = idx;
        }
    }

    /// 워크스페이스가 하나도 없는 engine 에 기본 워크스페이스를 만든다. 이미 있으면
    /// no-op 으로 `None` 을 반환하고, 만들었으면 그 인덱스를 반환한다.
    ///
    /// **복원 실패 안전망.** `CoreState::new_with_ids` 는 복원 예정이면 기본
    /// 워크스페이스를 만들지 않는데(만들면 PTY 가 회수 불가로 남는다),
    /// `SavedLayout::restore` 는 워크스페이스를 하나도 복원하지 못하면
    /// `engine.workspaces` 를 건드리지 않고 `false` 를 반환한다. 그 경우 워크스페이스
    /// 0개인 창이 뜨므로, 복원 적용 지점 **양쪽**(동기 `create_app_state`, 부팅 상태
    /// 머신 `finish_boot`)이 직후에 이걸 부른다.
    ///
    /// `&mut self` 가 아니라 `core`/`engine` 을 따로 받는다 — 호출자가
    /// `self.core_state` 를 빌린 상태에서 `self.core` 를 함께 써야 하기 때문
    /// (서로 다른 필드라 분리 대여가 필요하다).
    pub(super) fn bootstrap_workspace_if_empty(
        core: &mut crate::core::Core,
        engine: &mut crate::core::CoreState,
    ) -> Option<usize> {
        if !engine.workspaces.is_empty() {
            return None;
        }
        match core.create_default_workspace(engine) {
            Ok(idx) => Some(idx),
            Err(e) => {
                tracing::error!("bootstrap workspace failed: {e}");
                None
            }
        }
    }
}

/// `create_new_window` 지원 — 설정 로드+정규화+저장, theme 적용까지 한 단계로 묶는다
/// (`resumed()`의 `App::boot_load_normalized_settings`와 로직 유사하나, 이쪽은 theme
/// 적용까지 포함해 완전히 동일하진 않다).
fn boot_load_and_normalize_settings() -> (crate::settings::Settings, Option<String>) {
    let mut settings = crate::settings::Settings::load();
    // 모든 enum-like 필드 정규화. invalid 가 있었으면 즉시 파일에 반영해서
    // 다음 부팅에 같은 popup / 잘못된 동작이 재발하지 않게 한다.
    let normalize_report = settings.normalize();
    if normalize_report.changed
        && let Err(e) = settings.save()
    {
        tracing::warn!("failed to persist normalized settings: {e}");
    }

    // memory.db 는 boot 가 App::new 이전에 초기화한다.

    // Apply theme via tasty-themes (first-run init, fallback, partial accumulation, global install).
    let invalid_theme_name = boot_apply_theme(&mut settings);
    if (invalid_theme_name.is_some() || normalize_report.changed)
        && let Err(e) = settings.save()
    {
        tracing::warn!("failed to persist settings after theme apply: {e}");
    }
    (settings, invalid_theme_name)
}

/// `create_new_window` 지원 — state.db 초기화 실패 안내 모달을 만든다(pure builder).
fn build_db_init_error_modal(
    err: &crate::db::DbInitError,
) -> crate::adapters::ui::info_modal::InfoModal {
    tracing::error!("state.db init failed: {err}");
    let (key, args) = err.user_message_i18n();
    let body = match args.len() {
        0 => crate::i18n::t(key).to_string(),
        1 => crate::i18n::t_fmt(key, &args[0]),
        _ => crate::i18n::t_fmt2(key, &args[0], &args[1]),
    };
    crate::adapters::ui::info_modal::InfoModal {
        title: crate::i18n::t("db_error.title").to_string(),
        body,
        on_close: crate::adapters::ui::info_modal::InfoModalAction::Exit(1),
        extra_buttons: Vec::new(),
    }
}

/// `create_new_window` 지원 — theme fallback(잘못된 theme 이름) 안내 모달을 만든다
/// (pure builder).
fn build_theme_fallback_modal(
    invalid_theme_name: &str,
) -> crate::adapters::ui::info_modal::InfoModal {
    crate::adapters::ui::info_modal::InfoModal {
        title: crate::i18n::t("theme_error.title").to_string(),
        body: crate::i18n::t_fmt("theme_error.body", invalid_theme_name),
        on_close: crate::adapters::ui::info_modal::InfoModalAction::Continue,
        extra_buttons: Vec::new(),
    }
}

/// 새 main 창을 등록한 뒤 `focused_view_id` 가 가리킬 창.
///
/// 사용자 발화(메뉴 · 단축키 · dock · tray · 부팅)는 방금 그 조작의 결과이므로 새 창으로
/// 옮긴다. 에이전트 발화(`window.create` / `view.create`)는 옮기지 않는다 — 대상 없는 IPC
/// 요청이 떨어지는 자리가 사용자가 보던 창에서 바뀌면 안 된다(원칙 1·3, ADR-0497).
/// 에이전트 창도 가리키던 창이 없을 때(main 창이 하나도 없던 상태)는 새 창을 잡는다 —
/// 그때는 빼앗을 사용자 포커스가 없다. 사용자가 그 창을 직접 고르면
/// `WindowEvent::Focused(true)` 경로가 옮긴다.
pub(crate) fn focus_after_register(
    current: Option<winit::window::WindowId>,
    registered: winit::window::WindowId,
    origin: WindowRequestOrigin,
) -> Option<winit::window::WindowId> {
    match origin {
        WindowRequestOrigin::User => Some(registered),
        WindowRequestOrigin::Agent => current.or(Some(registered)),
    }
}

/// 창 생성 속성의 발화 주체 갈래. 사용자 창은 활성화된 채 보이게 만든다(winit 기본값과
/// 같다). 에이전트 창은 활성화하지 않고 **숨긴 채** 만든다 — 보이는 것은 등록 뒤
/// [`show_agent_window`] 가 사용자 창 뒤에 한다(ADR-0497).
pub(crate) fn origin_window_attributes(
    attrs: winit::window::WindowAttributes,
    origin: WindowRequestOrigin,
) -> winit::window::WindowAttributes {
    match origin {
        WindowRequestOrigin::User => attrs.with_active(true),
        WindowRequestOrigin::Agent => attrs.with_active(false).with_visible(false),
    }
}

/// 숨겨 만든 에이전트 창을 `anchor`(사용자가 보던 창) 뒤에, 키 포커스 없이 보인다.
/// 네이티브 호출이 실패하면 창 생성을 실패시키지 않고 경고 뒤 winit 기본 경로로 보인다.
fn show_agent_window(window: &Window, anchor: Option<&Window>) {
    if let Err(e) = crate::platform::window_stacking::show_behind(window, anchor) {
        tracing::warn!(
            "agent window: showing behind the user's window failed ({e}) — showing it the default way"
        );
        window.set_visible(true);
    }
    window.request_redraw();
}

#[cfg(test)]
mod register_focus_tests;
