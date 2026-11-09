//! 첫 창의 부팅을 단계별로 진행하며 대기 중에도 이벤트 루프에 제어를 돌려준다.
//! 일반 부팅은 첫 로딩 프레임을 그린 뒤 창을 표시한다. shell setup에서 넘어온 창은 이미 보인다.
//! 부팅 중 사용자 입력은 처리하지 않고 AppEvent는 완료 후 순서대로 재생한다.
//! 레이아웃 복원 전에 다른 변경이 끼지 않게 하며 IPC 서버도 완료 단계에서 시작한다.
//! RedrawRequested가 오지 않아도 about_to_wait에서 다음 단계를 진행하도록 예약한다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::app::App;
use crate::gpu::GpuState;

/// about_to_wait에서 다음 부팅 단계를 예약하는 간격.
pub(crate) const BOOT_FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// 레이아웃에 필요한 플러그인 kind 등록을 기다리는 상한. 동기 부팅 경로와 공유한다.
/// 기한이 지나도 복원은 진행하며 미등록 kind는 deferred placeholder로 남긴다.
/// 대기를 늘리면 첫 화면의 placeholder를 줄일 수 있지만 부팅도 늦어진다.
/// docs/features/layout-persistence/index.md의 Plugin surface 복원을 따른다.
pub(crate) const PLUGIN_WAIT_DEADLINE: Duration = Duration::from_millis(300);

use crate::app::state::{BootPhase, BootProgress};

pub(crate) struct BootResources {
    pub(crate) window: Arc<Window>,
    pub(crate) gpu: GpuState,
    engine_worker: Option<
        std::sync::mpsc::Receiver<
            anyhow::Result<(
                crate::runtime::engine_session::EngineSession,
                crate::plugin::PluginManager,
            )>,
        >,
    >,
    pub(crate) pending_events: Vec<crate::AppEvent>,
    settings_save: Option<crate::app::settings_change::SettingsWait>,
}

/// 이미 준비된 창과 GPU로 엔진 생성 오류를 보여 주며 같은 내용을 로그에도 남긴다.
fn boot_engine_error_info(err: &anyhow::Error) -> crate::gpu::BootErrorInfo {
    let title = crate::i18n::t("boot.engine_error.title").to_string();
    let body = crate::i18n::t_fmt("boot.engine_error.body", &err.to_string());
    let hint = crate::i18n::t("boot.engine_error.hint").to_string();
    tracing::error!("boot engine creation failed: {err:#}\n{title}\n{body}\n{hint}");
    crate::gpu::BootErrorInfo { title, body, hint }
}

/// 첫 workspace가 만들어지지 않은 엔진의 오류. 실패 이유를 알면 함께 보여 준다.
fn first_workspace_error(reason: Option<String>) -> String {
    match reason {
        Some(reason) => format!("the first workspace was not created: {reason}"),
        None => "the first workspace was not created".into(),
    }
}

/// 저널을 열지 못한 첫 엔진이 임시 저장소로 계속할지. 홈 사용 중이면 다른 인스턴스의 원본을
/// 보호하는 정상 제한이라 부팅 오류 화면에 남고, 임시 저장소마저 실패했으면 물러날 곳이 없다.
fn journal_failure_falls_back(home_in_use: bool, ephemeral: bool) -> bool {
    !home_in_use && !ephemeral
}

/// 저널 실패 문구. `home_in_use`는 worker가 "다른 프로세스가 같은 홈을 쓰고 있음"으로 보낸
/// 실패일 때의 홈 경로다. 오류 문자열을 해석하지 않는다.
fn journal_error_info(
    home_in_use: Option<&std::path::Path>,
    err: &anyhow::Error,
) -> crate::gpu::BootErrorInfo {
    match home_in_use {
        Some(home) => {
            tracing::error!("structure journal refused: {err:#}");
            crate::boot::home_in_use::error_info(home)
        }
        None => {
            tracing::error!("structure journal bootstrap failed: {err:#}");
            crate::gpu::BootErrorInfo {
                title: crate::i18n::t("boot.journal_error.title").to_owned(),
                body: crate::i18n::t_fmt("boot.journal_error.body", &err.to_string()),
                hint: crate::i18n::t("boot.journal_error.hint").to_owned(),
            }
        }
    }
}

impl App {
    /// 저널을 열지 못한 이유가 "다른 프로세스가 같은 홈을 쓰고 있음"이면 그 문구를, 아니면 엔진
    /// 오류 문구를 고른다. 오류 문자열이 아니라 worker가 보낸 종류로 판정한다.
    fn boot_journal_error_info(&self, err: &anyhow::Error) -> crate::gpu::BootErrorInfo {
        let home = self
            .journal
            .home_in_use()
            .then(tasty_utils::path::tasty_home)
            .flatten();
        journal_error_info(home.as_deref(), err)
    }

    /// 테마를 적용하고 첫 로딩 프레임을 그린다. hidden으로 만든 창만 표시를 전환한다.
    pub(crate) fn begin_boot(
        &mut self,
        window: Arc<Window>,
        gpu: GpuState,
        mut settings: crate::settings::Settings,
        boot_t0: Instant,
        window_hidden: bool,
    ) {
        let settings_origin = settings.origin;
        let (db_init_error, invalid_theme_name) = self.init_boot_db_and_theme(&mut settings);

        // 엔진이 슬롯을 읽기 전에 레이아웃 마이그레이션과 전체 슬롯의 scrollback GC를 수행한다.
        crate::core::layout_persistence::migrate_and_gc_on_boot(settings.general.restore_layout);

        let (saved, waiting) = crate::app::settings_change::pair(self.services.reply_wake());
        self.services
            .submit_settings_save(settings.clone(), None, Some(saved));
        let mut boot = BootResources {
            window,
            gpu,
            engine_worker: None,
            pending_events: Vec::new(),
            settings_save: Some(waiting),
        };
        let progress = BootProgress {
            settings,
            settings_origin,
            phase: BootPhase::GpuInit,
            boot_t0,
            db_init_error,
            invalid_theme_name,
            restored_idx: None,
            journal_plugins_waited: false,
        };
        Self::present_first_boot_frame(&mut boot, &progress, boot_t0, window_hidden);
        self.boot = Some(boot);
        self.state.boot = Some(progress);
    }

    /// 첫 로딩 프레임부터 사용자 테마를 쓰도록 적용한다. state.db도 엔진 생성 전에 준비한다.
    fn init_boot_db_and_theme(
        &mut self,
        settings: &mut crate::settings::Settings,
    ) -> (Option<crate::db::DbInitError>, Option<String>) {
        let t_db_theme = Instant::now();
        let db_init_error = self.start_state_db();
        let invalid_theme_name = crate::app::window_lifecycle::boot_apply_theme(settings);
        tracing::info!(
            target: "tasty::boot",
            ms = t_db_theme.elapsed().as_secs_f64() * 1000.0,
            "T2.5 db_theme (begin_boot enter -> first loading frame)"
        );
        (db_init_error, invalid_theme_name)
    }

    /// state.db를 실행당 한 번 열고 연결을 writer에 넘긴 뒤 공유 핸들을 공개한다. writer를 시작하지
    /// 못한 것도 초기화 실패로 알리며 그때는 공유 핸들을 공개하지 않는다.
    fn start_state_db(&mut self) -> Option<crate::db::DbInitError> {
        let db = match crate::db::init() {
            crate::db::InitOutcome::Opened(db) => db,
            crate::db::InitOutcome::Failed(error) => return Some(error),
            crate::db::InitOutcome::Repeated => return None,
        };
        match crate::db::writer::start(
            db,
            tasty_utils::batch_writer::Limits::default(),
            Arc::new(tasty_utils::batch_writer::SystemClock),
            self.state_db_wake.clone(),
        ) {
            Ok((parts, shared)) => {
                self.services.install_state_writer(parts);
                crate::db::publish(shared);
                None
            }
            Err(error) => {
                tracing::warn!(
                    "state.db writer failed to start: {error} — continuing without recent files and tutorial progress"
                );
                Some(crate::db::DbInitError::Other(format!(
                    "state writer could not start: {error}"
                )))
            }
        }
    }

    /// hidden 창은 RedrawRequested를 못 받을 수 있어 즉시 그린다.
    /// 렌더 실패 시에도 창을 표시하므로 OS 기본 배경이 잠깐 보일 수 있다.
    fn present_first_boot_frame(
        boot: &mut BootResources,
        progress: &BootProgress,
        boot_t0: Instant,
        window_hidden: bool,
    ) {
        let phase_key = crate::gpu::loading::boot_phase_text_key(&progress.phase);
        if let Err(e) = boot.gpu.render_loading(&boot.window, phase_key) {
            tracing::warn!("boot loading first frame render failed: {e} — showing window anyway");
        }
        if window_hidden {
            #[cfg(debug_assertions)]
            crate::boot::debug_background::show_boot_window(&boot.window);
            #[cfg(not(debug_assertions))]
            boot.window.set_visible(true);
            tracing::info!(
                target: "tasty::boot",
                ms = boot_t0.elapsed().as_secs_f64() * 1000.0,
                "T2.9 window_visible (boot start -> set_visible(true) after first loading frame)"
            );
        }
        boot.window.request_redraw();
    }

    /// boot를 꺼내서 처리하므로 처리 중 다시 호출되면 아무 일도 하지 않는다.
    pub(crate) fn drive_boot_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(mut boot) = self.boot.take() else {
            return;
        };
        let Some(mut progress) = self.state.boot.take() else {
            self.boot = Some(boot);
            return;
        };
        let ready = self.boot_step(&mut boot, &mut progress);
        // 엔진 생성 실패는 준비된 창과 GPU를 오류 화면에 넘겨 알린다.
        if self.state.boot_error_info.is_some() {
            self.enter_boot_error_mode(boot.window, boot.gpu);
            return;
        }
        if ready {
            self.finish_boot(boot, progress, event_loop);
            return;
        }
        let phase_key = crate::gpu::loading::boot_phase_text_key(&progress.phase);
        match boot.gpu.render_loading(&boot.window, phase_key) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // 다음 프레임에 다시 그릴 수 있도록 surface를 재구성한다.
                boot.gpu.resize(boot.window.inner_size());
            }
            Err(e) => {
                let msg = format!("boot loading frame render error: {e}");
                tracing::warn!("{}", msg);
                crate::crash_report::record_error(&msg);
            }
        }
        boot.window.request_redraw();
        self.boot = Some(boot);
        self.state.boot = Some(progress);
    }

    /// 한 단계를 진행하고 부팅 완료 여부를 반환한다.
    fn boot_step(&mut self, boot: &mut BootResources, progress: &mut BootProgress) -> bool {
        if matches!(progress.phase, BootPhase::GpuInit) {
            if !self.boot_settings_ready(boot) {
                return false;
            }
            boot.engine_worker = Some(self.spawn_engine_worker(boot, progress));
            progress.phase = BootPhase::WaitingEngine {
                started: Instant::now(),
                frames: 0,
            };
            return false;
        }
        if matches!(progress.phase, BootPhase::WaitingEngine { .. }) {
            return self.boot_step_waiting_engine(boot, progress);
        }
        if matches!(progress.phase, BootPhase::WaitingJournal) {
            return self.boot_step_waiting_journal(boot, progress);
        }
        if matches!(progress.phase, BootPhase::WaitingPlugins { .. }) {
            return self.boot_step_waiting_plugins(boot, progress);
        }
        self.boot_step_restoring_layout(boot, progress)
    }

    /// Poll the final snapshot without blocking loading frames or starting settings producers.
    fn boot_settings_ready(&mut self, boot: &mut BootResources) -> bool {
        self.services.poll_file_writer();
        self.settle_settings_saves();
        let Some(waiting) = boot.settings_save.as_mut() else {
            return true;
        };
        match waiting.poll() {
            crate::app::settings_change::SettingsPoll::Pending => {
                tracing::debug!(target: "tasty::boot", "boot settings write pending; yielding to loading frame");
                return false;
            }
            crate::app::settings_change::SettingsPoll::Ready(_) => {}
            crate::app::settings_change::SettingsPoll::Dropped => {
                tracing::warn!("boot settings save ended without a result");
            }
        }
        boot.settings_save = None;
        true
    }

    fn boot_step_waiting_engine(
        &mut self,
        boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let BootPhase::WaitingEngine { started, frames } = &mut progress.phase else {
            unreachable!("boot_step_waiting_engine called outside WaitingEngine phase");
        };
        *frames += 1;
        let received = boot
            .engine_worker
            .as_ref()
            .ok_or(std::sync::mpsc::TryRecvError::Disconnected)
            .and_then(|rx| rx.try_recv());
        match received {
            Ok(Ok((engine, mgr))) => {
                boot.engine_worker.take();
                let wait_ms = started.elapsed().as_secs_f64() * 1000.0;
                let frames = *frames;
                self.install_pending_engine(engine);
                self.plugin_manager = Some(mgr);
                tracing::info!(
                    target: "tasty::boot",
                    ms = wait_ms,
                    frames,
                    "T2.7 engine_wait (워커 체류; frames = 그동안 돈 로딩 프레임 스텝 수)"
                );
                self.boot_transition_after_engine(boot, progress)
            }
            Ok(Err(e)) => {
                boot.engine_worker.take();
                self.state.boot_error_info = Some(boot_engine_error_info(&e));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                if !self.services.registries.child_terminals.ready() {
                    return false;
                }
                // 스레드 생성 실패나 워커 패닉으로 결과가 없으면 동기로 다시 시도한다.
                tracing::error!("boot engine worker channel disconnected — synchronous fallback");
                if let Err(e) = self.ensure_engine_and_plugins(
                    &boot.gpu,
                    progress.settings.appearance.scaled_sidebar_width(),
                ) {
                    self.state.boot_error_info = Some(boot_engine_error_info(&e));
                    return false;
                }
                self.boot_transition_after_engine(boot, progress)
            }
        }
    }

    fn boot_step_waiting_plugins(
        &mut self,
        _boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let BootPhase::WaitingPlugins {
            started,
            deadline,
            needed,
        } = &mut progress.phase
        else {
            unreachable!("boot_step_waiting_plugins called outside WaitingPlugins phase");
        };
        let satisfied = self.boot_pump_step_plugins_registered(needed);
        // 기한이 지나도 복원을 진행하며 미등록 kind는 placeholder로 남긴다.
        if satisfied || Instant::now() >= *deadline {
            tracing::info!(
                target: "tasty::boot",
                ms = started.elapsed().as_secs_f64() * 1000.0,
                reason = if satisfied { "satisfied" } else { "deadline" },
                deadline_ms = PLUGIN_WAIT_DEADLINE.as_millis() as u64,
                "T4 layout_wait_plugins"
            );
            progress.journal_plugins_waited = true;
            progress.phase = BootPhase::WaitingJournal;
        }
        false
    }

    fn boot_step_restoring_layout(
        &mut self,
        _boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let BootPhase::RestoringLayout { started, deadline } = &mut progress.phase else {
            unreachable!("boot_step_restoring_layout called outside RestoringLayout phase");
        };
        let done = self.boot_pump_step_remote_restores_done();
        if done || Instant::now() >= *deadline {
            tracing::info!(
                target: "tasty::boot",
                ms = started.elapsed().as_secs_f64() * 1000.0,
                reason = if done { "satisfied" } else { "deadline" },
                "T6 remote_surface_wait (deadline 500ms)"
            );
            true
        } else {
            false
        }
    }

    /// 동기 부팅과 같은 초기화 함수를 워커에서 실행한다.
    /// 스레드를 만들지 못하면 송신자가 해제돼 다음 수신 시 동기 재시도로 넘어간다.
    fn spawn_engine_worker(
        &self,
        boot: &BootResources,
        progress: &BootProgress,
    ) -> std::sync::mpsc::Receiver<
        anyhow::Result<(
            crate::runtime::engine_session::EngineSession,
            crate::plugin::PluginManager,
        )>,
    > {
        let (cols, rows) = crate::app::window_lifecycle::boot_grid_size(
            &boot.gpu,
            progress.settings.appearance.scaled_sidebar_width(),
        );
        let factory: crate::waker::SharedWakerFactory = Arc::new(
            crate::waker_factory_winit::WinitWakerFactory::new(self.view.proxy.clone()),
        );
        let proxy = self.view.proxy.clone();
        let memory_access = self.services.memory_access().clone();
        let runner_registry = Arc::clone(self.services.tasks.runner_registry());
        let gauges = self.services.plugin_gauges();
        let registries = self.services.registries.clone();
        // 점유 중인 슬롯을 확인해야 하므로 메인 스레드에서 선택해 워커로 전달한다.
        let layout_slot = self.claim_free_layout_slot();
        // 첫 engine 이 자식 관계를 읽기 전에 어느 창에도 돌아오지 않을 항목을 지운다.
        registries
            .child_terminals
            .prepare(!progress.settings.general.restore_layout);
        #[cfg(debug_assertions)]
        let input_simulation_enabled = self.state.input_simulation_enabled;
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("tasty-boot-engine".into())
            .spawn(move || {
                registries.child_terminals.wait_bootstrap();
                let result = crate::app::window_lifecycle::build_engine_and_plugins(
                    crate::app::window_lifecycle::FirstBootEngine {
                        cols,
                        rows,
                        factory,
                        proxy,
                        memory_access,
                        runner_registry,
                        layout_slot,
                        registries,
                        #[cfg(debug_assertions)]
                        input_simulation_enabled,
                    },
                    gauges,
                );
                if tx.send(result).is_err() {
                    // 수신자가 없으면 결과를 drop하며 플러그인 자식 정리도 Drop에 맡긴다.
                    tracing::warn!("boot engine worker: receiver dropped; discarding init result");
                }
            });
        if let Err(e) = spawned {
            tracing::error!("boot engine worker spawn failed: {e} — synchronous fallback");
        }
        rx
    }

    fn boot_transition_after_engine(
        &mut self,
        _boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let id = self
            .engines
            .pending_id()
            .expect("boot engine was installed");
        let session = self.engines.session_mut(id).expect("pending engine exists");
        let slot = session
            .persistence
            .slot
            .expect("GUI engine has a layout slot");
        let resume = session.runtime.settings.general.restore_layout;
        if let Err(error) = self.journal.begin_first_gui_engine(
            session,
            crate::runtime::journal_product::EngineSelection::Slot { slot, resume },
        ) && !self.boot_fall_back_to_ephemeral(id, &error)
        {
            self.state.boot_error_info =
                Some(self.boot_journal_error_info(&anyhow::anyhow!(error)));
            return false;
        }
        progress.phase = BootPhase::WaitingJournal;
        false
    }

    /// 원본 저널을 열지 못한 첫 엔진을 임시 저장소로 다시 연다. 홈 사용 중(다른 인스턴스의 원본
    /// 보호)이거나 이미 임시 저장소인데도 실패했으면 물러나지 않고 false다.
    fn boot_fall_back_to_ephemeral(
        &mut self,
        id: crate::runtime::engine_session::EngineId,
        failure: &str,
    ) -> bool {
        if !journal_failure_falls_back(self.journal.home_in_use(), self.journal.is_ephemeral()) {
            return false;
        }
        let session = self.engines.session_mut(id).expect("pending engine exists");
        match self
            .journal
            .poll_boot_recovery(session, self.plugin_manager.as_mut(), failure)
        {
            Ok(()) => true,
            Err(error) => {
                tracing::error!("temporary structure journal recovery failed: {error:#}");
                false
            }
        }
    }

    fn boot_step_waiting_journal(
        &mut self,
        boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let id = self.engines.pending_id().expect("pending bootstrap engine");
        let session = self.engines.session_mut(id).expect("pending engine exists");
        if let Err(error) = self
            .journal
            .poll_bootstrap(&mut [session], self.plugin_manager.as_mut())
        {
            if self.boot_fall_back_to_ephemeral(id, &error) {
                return false;
            }
            self.state.boot_error_info =
                Some(self.boot_journal_error_info(&anyhow::anyhow!(error)));
            return false;
        }
        if session.journal_binding.is_some() && !progress.journal_plugins_waited {
            let needed = self.boot_required_plugin_kinds();
            if !needed.is_empty() {
                let now = Instant::now();
                progress.phase = BootPhase::WaitingPlugins {
                    started: now,
                    deadline: now + PLUGIN_WAIT_DEADLINE,
                    needed,
                };
                return false;
            }
            progress.journal_plugins_waited = true;
        }
        let session = self.engines.session_mut(id).expect("pending engine exists");
        if let Err(error) = self.journal.poll_restore_bootstrap(session) {
            if self.boot_fall_back_to_ephemeral(id, &error) {
                return false;
            }
            self.state.boot_error_info =
                Some(self.boot_journal_error_info(&anyhow::anyhow!(error)));
            return false;
        }
        if session.journal_binding.is_none() || !self.journal.is_ready(id) {
            return false;
        }
        // A failed or uncertain first creation leaves the engine ready without a workspace,
        // which no View can show. Report it like other engine startup failures.
        if session.core_state.workspaces().is_empty() {
            let error = first_workspace_error(self.journal.take_first_workspace_failure(id));
            if self.boot_fall_back_to_ephemeral(id, &error) {
                return false;
            }
            self.state.boot_error_info = Some(boot_engine_error_info(&anyhow::anyhow!(error)));
            return false;
        }
        self.boot_transition_after_journal(boot, progress)
    }

    fn boot_transition_after_journal(
        &mut self,
        _boot: &mut BootResources,
        progress: &mut BootProgress,
    ) -> bool {
        let id = self.engines.pending_id().expect("pending bootstrap engine");
        progress.restored_idx = self.journal.take_restored_presentation(id);
        let now = Instant::now();
        progress.phase = BootPhase::RestoringLayout {
            started: now,
            deadline: now + Duration::from_millis(500),
        };
        false
    }

    /// 종료 전에 결과를 회수해야 할 부팅 워커가 있는지 확인한다.
    /// 회수 전에 프로세스를 끝내면 PluginManager의 Drop이 실행되지 않아 자식이 남을 수 있다.
    pub(super) fn boot_engine_worker_pending(&self) -> bool {
        self.boot
            .as_ref()
            .is_some_and(|boot| boot.engine_worker.is_some())
    }

    /// 종료 단계에서 블로킹 없이 결과를 받는다. 대기 기한은 호출자가 관리한다.
    /// Ok(None)은 결과 대기 중이거나 엔진 생성 실패, Err는 채널 단절 또는 회수 대상 부재다.
    pub(super) fn try_recv_boot_engine_worker(
        &mut self,
    ) -> Result<
        Option<(
            crate::runtime::engine_session::EngineSession,
            crate::plugin::PluginManager,
        )>,
        (),
    > {
        let Some(boot) = self.boot.as_mut() else {
            return Err(());
        };
        let Some(rx) = boot.engine_worker.as_ref() else {
            return Err(());
        };
        match rx.try_recv() {
            Ok(Ok(payload)) => Ok(Some(payload)),
            Ok(Err(_)) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("boot engine worker not reclaimed before exit: disconnected");
                Err(())
            }
        }
    }

    /// 창·IPC를 등록하고 시작 이벤트를 알린 뒤 부팅 중 보류한 이벤트를 재생한다.
    fn finish_boot(
        &mut self,
        boot: BootResources,
        progress: BootProgress,
        event_loop: &ActiveEventLoop,
    ) {
        let BootResources {
            window,
            gpu,
            pending_events,
            engine_worker: _,
            settings_save: _,
        } = boot;
        let BootProgress {
            settings,
            settings_origin,
            phase: _,
            boot_t0,
            db_init_error,
            invalid_theme_name,
            restored_idx,
            journal_plugins_waited: _,
        } = progress;
        self.state.started = true;
        #[cfg(debug_assertions)]
        let assembled = match crate::boot::debug_boot_fault::take(
            crate::boot::debug_boot_fault::Stage::Assemble,
        ) {
            Some(error) => Err(error.to_string()),
            None => self.assemble_app_state(restored_idx),
        };
        #[cfg(not(debug_assertions))]
        let assembled = self.assemble_app_state(restored_idx);
        let mut state = match assembled {
            Ok(state) => state,
            Err(error) => {
                // 첫 화면을 만들 수 없으면 계속할 세션이 없다. 준비된 부팅 창에 원인을 보이고,
                // 사용자가 닫으면 실패 코드로 끝낸다(부팅 오류 화면의 종료 경로).
                tracing::error!("cannot register the initial committed engine: {error}");
                let error = anyhow::anyhow!("cannot assemble the first window: {error}");
                self.state.boot_error_info = Some(crate::boot::app_error_info(&error));
                self.enter_boot_error_mode(window, gpu);
                return;
            }
        };
        Self::report_boot_init_errors(&mut state, invalid_theme_name);
        Self::report_boot_notices(&mut state.toasts, &mut self.font_rejections, &settings);
        // 서버가 받는 첫 연결부터 설정값을 쓰도록 IPC를 열기 전에 반영한다.
        crate::app::attach_heartbeat::apply(&self.services, &settings);
        self.start_boot_ipc_and_webhooks();
        Self::report_persistence_incidents(settings_origin, &mut state);

        let engine = self
            .engines
            .pending_id()
            .expect("pending engine must be present to register a main window");
        let mut core_state = self
            .engines
            .get_mut(engine)
            .expect("pending engine must be present to register a main window");
        // 두 저장소 실패는 앱을 멈추지 않고 첫 engine의 알림으로 한 번 남긴다.
        if let Some(error) = db_init_error.as_ref() {
            crate::app::storage_notice::report_state_db_unavailable(&mut core_state, error);
        }
        crate::app::storage_notice::report_memory_fallback(
            &mut core_state,
            self.services.memory_init_fallback(),
        );
        // force-detach 통지를 IPC와 같은 스트림으로 보낸다.
        core_state.remote.set_notifier(self.stream_hub.clone());
        crate::app::task_report::apply(&self.services, &core_state.runtime.settings);
        crate::app::anomaly_settings::apply(&self.services, &core_state.runtime.settings);
        // 첫 창을 노출하기 전에 이전 실행의 에이전트 작업 상태를 정리하며 자동 실행은 하지 않는다.
        self.services.tasks.purge_stale_agent_state_on_boot(
            core_state.task_scope,
            &core_state
                .workspaces()
                .into_iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
        );
        Self::report_missing_permissions(&mut state);
        self.register_window(
            gpu,
            state,
            engine,
            window.clone(),
            crate::app::event::WindowRequestOrigin::User,
        );
        self.emit_startup_complete_event();

        tracing::info!(
            target: "tasty::boot",
            ms = boot_t0.elapsed().as_secs_f64() * 1000.0,
            "boot_total (boot start -> Ready; T2.5~T6 + 미계측 잔여 합)"
        );
        crate::boot::trace::mark_resumed_done();

        window.request_redraw();

        // TerminalOutput 이벤트를 보류한 동안 닫혀 있던 wake 중복 방지 상태를 풀 수 있도록
        // 엔진이 views에 등록된 뒤 이벤트를 재생한다.
        for ev in pending_events {
            use winit::application::ApplicationHandler;
            self.user_event(event_loop, ev);
        }
    }

    /// 부팅 카드를 쌓는다. 첫 overlay 프레임보다 먼저 쌓이므로 저장된 수명을 여기서 넣는다.
    pub(super) fn report_boot_notices(
        toasts: &mut crate::adapters::ui::ToastManager,
        font_rejections: &mut crate::app::font_rejection::FontRejectionNotices,
        settings: &crate::settings::Settings,
    ) {
        toasts.set_lifetime_ms(settings.overlay.toast_duration_ms);
        Self::report_locale_fallback(toasts);
        for value in font_rejections.at_boot(&settings.appearance) {
            toasts.push(
                crate::i18n::t_fmt_fit("toast.font_rejected", &value),
                crate::adapters::ui::ToastKind::Warning,
                crate::adapters::ui::ToastScope::Window,
            );
        }
    }

    /// 언어팩 폴백을 한 번 알린다. 설정값은 유지하며 로더가 남긴 경고 로그는 반복하지 않는다.
    fn report_locale_fallback(toasts: &mut crate::adapters::ui::ToastManager) {
        if let Some(msg) =
            crate::i18n::load_report().and_then(crate::i18n::LoadReport::user_warning)
        {
            toasts.push(
                msg,
                crate::adapters::ui::ToastKind::Warning,
                crate::adapters::ui::ToastScope::Window,
            );
        }
        // 폰트 실패는 언어 폴백과 별개다. 긴 경로만 줄여 조치 안내가 잘리지 않게 한다.
        if let Some(detail) = crate::boot::locale::font_warning() {
            toasts.push(
                crate::i18n::t_fmt_fit("i18n.warn.font_unresolved", &detail),
                crate::adapters::ui::ToastKind::Warning,
                crate::adapters::ui::ToastScope::Window,
            );
        }
    }

    /// 잘못된 테마 이름을 모달로 알린다. state.db 실패는 engine 알림으로 따로 남긴다.
    fn report_boot_init_errors(
        state: &mut crate::state::MainViewState,
        invalid_theme_name: Option<String>,
    ) {
        if let Some(invalid) = invalid_theme_name {
            crate::adapters::ui::info_modal::show_info_modal(
                state,
                crate::adapters::ui::info_modal::InfoModal {
                    title: crate::i18n::t("theme_error.title").to_string(),
                    body: crate::i18n::t_fmt("theme_error.body", &invalid),
                    extra_buttons: Vec::new(),
                    emphasis: false,
                },
            );
        }
    }

    /// macOS에서 확인할 수 있는 권한 중 하나라도 허용되지 않았으면 안내 모달을 띄운다.
    /// 부팅 직후에 권한을 자동으로 요청하지 않으므로, 사용자가 권한 화면을 찾아가는
    /// 경로는 이 안내다. 사용자가 권한을 회수할 수 있어 부팅할 때마다 다시 확인하며,
    /// 안내를 띄웠다는 사실은 저장하지 않는다. 어떤 권한을 보는지와 그 한계는
    /// `crates/tasty-platform/src/macos_permissions.rs`의 `should_show_permission_notice`에
    /// 있다. macOS가 아니면 아무 일도 하지 않는다.
    fn report_missing_permissions(state: &mut crate::state::MainViewState) {
        if !crate::macos_permissions::wants_permission_notice() {
            return;
        }
        crate::adapters::ui::info_modal::show_info_modal(
            state,
            crate::adapters::ui::info_modal::InfoModal {
                title: crate::i18n::t("macos_permissions.notice.title").to_string(),
                body: crate::macos_permissions::permission_notice_body(
                    crate::macos_permissions::notice_inputs(),
                    crate::i18n::t,
                ),
                extra_buttons: permission_notice_buttons(),
                emphasis: true,
            },
        );
    }

    /// 설정·레이아웃을 읽거나 보존하지 못한 사실과 복구에 필요한 정보를 알린다.
    fn report_persistence_incidents(
        settings_origin: tasty_settings::SettingsOrigin,
        state: &mut crate::state::MainViewState,
    ) {
        use crate::adapters::ui::{ToastKind, ToastScope};
        use tasty_settings::SettingsOrigin;

        let mut warn = |msg: String| {
            state
                .toasts
                .push(msg, ToastKind::Warning, ToastScope::Window);
        };
        // 테마 저장으로 파일이 복구됐어도 부팅 때 처음 발견한 문제는 안내해야 한다.
        match settings_origin {
            SettingsOrigin::Clean => {}
            SettingsOrigin::Unparsable => {
                let path = tasty_settings::Settings::config_path().unwrap_or_default();
                let shown = tasty_utils::path::tilde_abbreviate(&path);
                // 파일이 여전히 해석되지 않으면 백업에 성공했다고 안내하지 않는다.
                let key = if tasty_settings::Settings::file_is_unparsable(&path) {
                    "persistence.warn.settings_unparsable_blocked"
                } else {
                    "persistence.warn.settings_unparsable"
                };
                // 경로만 줄여 문장 끝의 조치 안내를 남긴다.
                warn(crate::i18n::t_fmt_fit(key, &shown));
            }
            SettingsOrigin::ProtectedUnreadable => {
                warn(crate::i18n::t("persistence.warn.settings_locked").to_string());
            }
        }
    }

    fn start_boot_ipc_and_webhooks(&mut self) {
        let ipc_proxy = self.view.proxy.clone();
        let ipc_waker: crate::ipc::server::IpcWaker = std::sync::Arc::new(move || {
            crate::shortcuts::send_app_event(&ipc_proxy, crate::AppEvent::IpcReady);
        });
        let stream_proxy = self.view.proxy.clone();
        let stream_waker: crate::ipc::server::IpcWaker = std::sync::Arc::new(move || {
            crate::shortcuts::send_app_event(&stream_proxy, crate::AppEvent::StreamReady);
        });
        let stream_ctx = tasty_ipc::stream_hub::StreamContext {
            hub: self.stream_hub.clone(),
            inbound_tx: self.stream_inbound_tx.clone(),
            waker: self.stream_wake.install(stream_waker),
            heartbeat_timeout: self.services.attach_heartbeat.clone(),
        };
        let connections = self.services.connections().clone();
        // Lua 실행 결과도 IPC와 같은 깨우기로 메인 루프에 전달한다.
        if let Some(lua) = self.lua_engine.as_ref() {
            lua.set_outcome_waker(ipc_waker.clone());
        }
        if let Some(injector) = self.hub.start_ipc(ipc_waker, stream_ctx, connections) {
            if let Some(server) = self.hub.ipc_server.as_ref() {
                self.services.record_ipc_port(server.port());
            }
            self.start_single_instance_service();
            // 리스너가 참조할 훅 레지스트리를 먼저 채운다. 웹훅 시작 실패는 비치명적 경고 알림으로 남긴다.
            crate::hook_handler::install_default_sources();
            crate::completion_strategy::install_default_sources();
            let reservation = self.hub.webhook.take();
            // 경고는 토스트가 아니라 알림으로 남긴다. 첫 창이 등록된 뒤 그 활성 workspace에 저장된다.
            let _report = self.services.start_webhooks(reservation, injector.clone());
            self.services.set_host_ipc_injector(injector);
        }
    }

    /// IPC가 열린 뒤 인스턴스 파일에 포트를 쓰고, 두 번째 프로세스의 활성화 요청을 받을 경로를 연다.
    fn start_single_instance_service(&mut self) {
        let Some(port) = self.services.ports.ipc_port() else {
            return;
        };
        crate::boot::single_instance::instance_file::publish_port(port);
        // Windows 는 이벤트 루프의 메시지 훅으로 받으므로 여기서 여는 경로는 Linux D-Bus 뿐이다.
        #[cfg(target_os = "linux")]
        if crate::boot::single_instance::service_enabled()
            && let Some(home) = tasty_utils::path::tasty_home()
        {
            let proxy = self.view.proxy.clone();
            crate::boot::single_instance::dbus::start_service(
                &home,
                Box::new(move |evidence| {
                    crate::shortcuts::send_app_event(
                        &proxy,
                        crate::AppEvent::ExternalActivate(
                            crate::boot::single_instance::ExternalActivation {
                                evidence,
                                os_granted: false,
                            },
                        ),
                    );
                }),
            );
        }
    }

    fn emit_startup_complete_event(&mut self) {
        if let Some(mgr) = self.plugin_manager.as_mut() {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::SystemStartupComplete;
            mgr.emit_host_event(
                "system.startup_complete",
                &SystemStartupComplete::default(),
                EventScope::System,
            );
        }
    }

    /// 복원 중 사용자 입력은 소비하되 OS 포커스는 egui에 보존한다.
    pub(crate) fn handle_boot_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        match event {
            WindowEvent::RedrawRequested => self.drive_boot_frame(event_loop),
            WindowEvent::Resized(size) => {
                if let Some(boot) = self.boot.as_mut() {
                    boot.gpu.resize(size);
                }
            }
            WindowEvent::Focused(_) => {
                if let Some(boot) = self.boot.as_mut() {
                    // Focus may arrive while the loading frame is visible, before MainView exists.
                    // Dropping it leaves egui unfocused until the next OS focus transition.
                    boot.gpu.handle_egui_event(&boot.window, &event);
                }
            }
            WindowEvent::CloseRequested => {
                // 종료 상태 머신이 부팅 워커와 플러그인 자식도 회수한다.
                self.begin_shutdown(event_loop);
            }
            _ => {}
        }
    }
}

/// 권한 안내 모달에 붙는 버튼. 시스템 설정이 아니라 Tasty의 권한 화면으로 보낸다.
#[cfg(all(target_os = "macos", feature = "gui"))]
fn permission_notice_buttons() -> Vec<crate::adapters::ui::info_modal::InfoModalButton> {
    vec![crate::adapters::ui::info_modal::InfoModalButton {
        label: crate::i18n::t("macos_permissions.notice.open_settings").to_string(),
        action: crate::adapters::ui::info_modal::InfoModalButtonAction::OpenPermissionSettings,
    }]
}

#[cfg(not(all(target_os = "macos", feature = "gui")))]
fn permission_notice_buttons() -> Vec<crate::adapters::ui::info_modal::InfoModalButton> {
    Vec::new()
}

#[cfg(test)]
mod boot_error_tests {
    use super::*;

    /// 번역 초기화 전후 모두 서로 다른 제목·본문·조치 안내를 사용해야 한다.
    #[test]
    fn a_home_in_use_failure_names_the_home_instead_of_the_shell() {
        crate::i18n::init("en");
        let home = std::path::Path::new("/tmp/tasty-home-in-use-example");
        let err = anyhow::anyhow!("another store holds the journal writer lock");
        let info = journal_error_info(Some(home), &err);
        assert_eq!(info.title, crate::i18n::t("boot.home_in_use.title"));
        assert_eq!(info.hint, crate::i18n::t("boot.home_in_use.hint"));
        assert!(info.body.contains(&home.display().to_string()));
        assert_ne!(info.title, crate::i18n::t("boot.engine_error.title"));
        assert_ne!(info.hint, crate::i18n::t("boot.engine_error.hint"));
    }

    #[test]
    fn other_journal_failures_describe_storage_instead_of_the_shell() {
        crate::i18n::init("en");
        // 같은 문자열이어도 종류가 홈 사용 중이 아니면 엔진 오류 문구다(문자열로 판정하지 않는다).
        let err = anyhow::anyhow!("another store holds the journal writer lock");
        let info = journal_error_info(None, &err);
        assert_eq!(info.title, crate::i18n::t("boot.journal_error.title"));
    }

    /// App 생성·첫 화면 조립 실패 문구는 원인을 담고 셸 설정 안내(엔진 오류)와 구별된다.
    #[test]
    fn app_error_info_names_the_cause_and_differs_from_the_shell_hint() {
        crate::i18n::init("en");
        let info = crate::boot::app_error_info(&anyhow::anyhow!("data home unavailable"));
        assert_eq!(info.title, crate::i18n::t("boot.app_error.title"));
        assert!(info.body.contains("data home unavailable"));
        assert_ne!(info.hint, crate::i18n::t("boot.engine_error.hint"));
        assert_ne!(info.title, info.body);
    }

    #[test]
    fn only_a_journal_that_is_not_held_elsewhere_falls_back_once() {
        assert!(journal_failure_falls_back(false, false));
        assert!(!journal_failure_falls_back(true, false), "home in use");
        assert!(
            !journal_failure_falls_back(false, true),
            "already temporary"
        );
    }

    #[test]
    fn engine_error_info_reads_three_distinct_diagnostics() {
        let info = boot_engine_error_info(&anyhow::anyhow!("shell not found: /bad/path"));
        assert!(!info.title.is_empty(), "title must be present");
        assert!(!info.body.is_empty(), "body must be present");
        assert!(!info.hint.is_empty(), "hint must be present");
        assert_ne!(info.title, info.body, "title and body must differ");
        assert_ne!(info.body, info.hint, "body and hint must differ");
        assert_ne!(info.title, info.hint, "title and hint must differ");
    }
}
