//! CLI를 먼저 처리하고 호스트 실행이면 gui feature에 따라 GUI 또는 헤드리스를 시작한다.
//! GUI 빌드는 --headless를 경고 후 무시한다.

pub(crate) mod cli_routing;
#[cfg(all(feature = "gui", debug_assertions))]
pub(crate) mod debug_background;
#[cfg(all(feature = "gui", debug_assertions))]
pub(crate) mod debug_boot_fault;
#[cfg(feature = "gui")]
pub(crate) mod display_backend;
#[cfg(feature = "gui")]
pub(crate) mod event_loop;
#[cfg(not(feature = "gui"))]
mod file_drain;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_dispatch;
#[cfg(not(feature = "gui"))]
mod headless_memory;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_plugins;
#[cfg(all(test, not(feature = "gui")))]
mod headless_settings_tests;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_stream;
#[cfg(feature = "gui")]
pub(crate) mod home_in_use;
pub(crate) mod locale;
pub(crate) mod locale_font;
#[cfg(not(feature = "gui"))]
mod observer_drain;
pub(crate) mod os;
#[cfg(feature = "gui")]
pub(crate) mod single_instance;
#[cfg(feature = "gui")]
pub(crate) mod trace;
#[cfg(feature = "gui")]
pub(crate) mod waker;
pub(crate) mod wiring;

#[cfg(feature = "gui")]
use crate::App;
#[cfg(not(feature = "gui"))]
use crate::runtime::engine_access::EngineMut;
use crate::{cli, hooks};

fn log_vacuum_result(result: tasty_memory::Result<bool>) {
    match result {
        Ok(true) => tracing::info!("boot memory maintenance: vacuumed memory.db"),
        Ok(false) => {}
        Err(e) => tracing::warn!("boot memory maintenance: vacuum failed: {e}"),
    }
}

fn vacuum_if_needed(store: &mut tasty_memory::MemoryStore, pruned: u64) {
    if pruned == 0 {
        return;
    }
    tracing::info!("boot memory maintenance: pruned {pruned} stale log rows");
    log_vacuum_result(store.vacuum_if_fragmented(10_000));
}

/// VACUUM이 WAL을 늘릴 수 있어 그 뒤에 축소를 요청한다.
/// journal_size_limit은 WAL 재사용 때 적용되며 활성 WAL의 크기 상한은 아니다.
fn truncate_wal(store: &mut tasty_memory::MemoryStore) {
    match store.checkpoint_truncate() {
        Ok(true) => {}
        // 다른 연결 때문에 축소하지 못했어도 부팅은 계속한다.
        Ok(false) => {
            tracing::info!("boot memory maintenance: wal checkpoint was busy; wal left as is")
        }
        Err(e) => tracing::warn!("boot memory maintenance: wal checkpoint failed: {e}"),
    }
}

/// 런타임과 같은 로그 보존 정책을 적용하고 필요하면 VACUUM·WAL 축소를 시도한다.
fn maintain_memory_at_boot(arc: &std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>) {
    let mut store = crate::poison::recover_mutex(
        arc.lock(),
        crate::core::MEMORY_WHAT,
        &crate::core::MEMORY_POISONED,
    );
    // memory 값 상한이 task 결과 몫을 남기지 못하면 부팅 때 한 번 알린다.
    tasty_agent::task::record_limit::warn_if_no_result_room(store.config().entry_max_bytes);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut pruned = 0u64;
    for policy in &crate::store::log_retention::ALL {
        pruned += policy.enforce(&mut *store, now_ms);
    }
    vacuum_if_needed(&mut store, pruned);
    truncate_wal(&mut store);
}

/// main에서 호출하는 프로세스 진입점.
pub fn run() -> anyhow::Result<std::process::ExitCode> {
    // 자식 셸에 활성화 토큰이 새지 않도록 다른 초기화·스레드보다 먼저 읽어 두고 환경에서 지운다.
    #[cfg(feature = "gui")]
    single_instance::evidence::capture_and_clear_env();
    os::attach_windows_console_if_needed();
    os::init_crash_report();

    match cli_routing::parse_or_route()? {
        cli_routing::Routed::AlreadyHandled => Ok(()),
        cli_routing::Routed::Subcommand(cmd, port_file, envelope) => {
            run_subcommand(cmd, port_file, envelope)
        }
        cli_routing::Routed::AugmentedHelp => run_augmented_help(),
        cli_routing::Routed::Gui(cli) => {
            // 로그 파일·memory.db·설정·플러그인·저널을 건드리기 전에 이 홈의 저널 writer 잠금을 선점한다.
            // 잡혀 있으면 저널 worker와 같은 구간 동안 다시 시도한다. 창을 만들기 전이라 멈춘 화면이 없다.
            // 그래도 다른 Tasty가 쥐고 있으면 그 인스턴스의 홈에 부작용을 내지 않는다. release는 실행 중인
            // Tasty에 요청을 넘기고 끝나며(단일 실행), debug는 "홈 사용 중" 오류 화면을 띄운다.
            #[cfg(feature = "gui")]
            let writer_lock = match preempt_home_writer_lock() {
                HomeWriterLock::Acquired(lock, home) => {
                    single_instance::instance_file::publish_started(&home);
                    Some(lock)
                }
                HomeWriterLock::Held(home) if cfg!(debug_assertions) => {
                    return home_in_use::run(&home);
                }
                HomeWriterLock::Held(home) => match single_instance::second::handle_held(&home, cli.webhook_port) {
                    single_instance::second::HeldOutcome::Exit(code) => return Ok(code),
                    single_instance::second::HeldOutcome::BootNormally(lock) => {
                        single_instance::instance_file::publish_started(&home);
                        Some(lock)
                    }
                },
                HomeWriterLock::Unchecked => None,
            };
            // CLI도 같은 바이너리라 호스트로 결정한 뒤 열어야 실행 중 호스트의 로그를 자르지 않는다.
            os::enable_host_file_log();
            // Windows 호스트 종료 시 자식 셸도 정리하도록 job을 만든다. CLI에는 만들지 않는다.
            tasty_reaper::init_host_reaper();
            #[cfg(feature = "gui")]
            {
                if cli.headless {
                    tracing::warn!(
                        "--headless requested in gui build; gui build does not embed headless mode. \
                         Build with --no-default-features to enable headless. Falling back to run_gui."
                    );
                }
                return run_gui(cli, writer_lock);
            }
            #[cfg(not(feature = "gui"))]
            {
                return run_headless(cli);
            }
        }
    }.map(|()| std::process::ExitCode::SUCCESS)
}

fn run_subcommand(
    cmd: cli::Commands,
    port_file: Option<String>,
    envelope: cli::Envelope,
) -> anyhow::Result<()> {
    locale::init();
    cli::run_client_with(cmd, port_file.as_deref(), envelope)
}

fn run_augmented_help() -> anyhow::Result<()> {
    locale::init();
    cli::print_augmented_help()
}

#[cfg(feature = "gui")]
enum HomeWriterLock {
    Acquired(tasty_event_store::WriterLock, std::path::PathBuf),
    /// 다른 프로세스가 이 홈의 저널 writer 잠금을 쥐고 있다.
    Held(std::path::PathBuf),
    /// 홈을 알 수 없거나 잠금을 쓸 수 없다. 저널 worker가 다시 시도하고 그 결과로 판정한다.
    Unchecked,
}

#[cfg(feature = "gui")]
fn preempt_home_writer_lock() -> HomeWriterLock {
    let Some(home) = tasty_utils::path::tasty_home() else {
        return HomeWriterLock::Unchecked;
    };
    // release 단일 실행: PID·시작 시각이 살아 있고 포트가 있는 인스턴스 기록이면 잠금 재시도
    // (`WRITER_LOCK_WAIT`)를 기다리지 않고 바로 넘긴다. 낡은 기록이나 포트 없는 기록은 재시도 경로로 간다.
    if !cfg!(debug_assertions)
        && matches!(
            single_instance::instance_file::read_state(&home),
            single_instance::instance_file::InstanceState::Live(_)
        )
    {
        return HomeWriterLock::Held(home);
    }
    let database = crate::runtime::journal_product::journal_database_path(&home);
    match tasty_event_store::preempt_writer_lock(&database) {
        Ok(tasty_event_store::WriterPreempt::Acquired(lock)) => {
            HomeWriterLock::Acquired(lock, home)
        }
        Ok(tasty_event_store::WriterPreempt::Held) => HomeWriterLock::Held(home),
        Err(e) => {
            tracing::warn!(
                "journal writer lock preempt failed for {}: {e}; the journal worker will retry",
                database.display()
            );
            HomeWriterLock::Unchecked
        }
    }
}

#[cfg(feature = "gui")]
fn run_gui(
    cli: cli::Cli,
    writer_lock: Option<tasty_event_store::WriterLock>,
) -> anyhow::Result<std::process::ExitCode> {
    locale::init();

    let boot_settings = crate::settings::Settings::load();
    // 명시 지정한 웹훅 포트를 열지 못해도 터미널은 시작한다. 원인은 리스너 시작 때 경고 알림으로 남는다.
    let webhook = crate::webhook::prepare(cli.webhook_port, &boot_settings.webhook);

    // 비정상 종료한 프로세스가 남긴 디스크 스크롤백을 설정과 관계없이 백그라운드에서 회수한다.
    tasty_terminal::disk_scrollback::spawn_orphan_reclaim();

    let (event_loop, proxy) = event_loop::build()?;
    // run_app consumes the event loop; shared GPU teardown can outlive every Window.
    // Declare this before App so errors and unwinding also drop GPU owners first.
    #[cfg(target_os = "linux")]
    let display_owner = event_loop.owned_display_handle();
    os::install_macos_delegate(&proxy);

    // 탭의 CWD 표시는 OSC 7을 사용한다. bash는 PROMPT_COMMAND 설정이 필요하다.

    // App을 만들기 전에 메모리 저장소를 열어 Core에 같은 Arc를 전달한다.
    let memory_config = tasty_memory::MemoryConfig {
        entry_max_bytes: boot_settings
            .memory
            .entry_max_mb
            .saturating_mul(1024 * 1024),
        secret_quota_per_owner_bytes: boot_settings
            .memory
            .secret_quota_mb_per_plugin
            .saturating_mul(1024 * 1024),
        regular_quota_total_bytes: boot_settings
            .memory
            .regular_quota_mb_total
            .saturating_mul(1024 * 1024),
    };
    let memory_arc = match tasty_memory::init_with_config(memory_config) {
        Ok(arc) => {
            maintain_memory_at_boot(&arc);
            Some(arc)
        }
        Err(e) => {
            tracing::warn!("memory.db init at boot failed: {e}");
            crate::boot::wiring::memory_fallback_after(&e)
        }
    };

    let screen_proxy = proxy.clone();
    #[cfg(debug_assertions)]
    let injected = debug_boot_fault::take(debug_boot_fault::Stage::AppNew);
    #[cfg(not(debug_assertions))]
    let injected: Option<anyhow::Error> = None;
    let constructed = match injected {
        Some(error) => Err(error),
        None => App::new(
            proxy,
            cli.port_file,
            memory_arc,
            writer_lock,
            #[cfg(debug_assertions)]
            cli.enable_input_simulation,
        ),
    };
    let mut app = match constructed {
        Ok(app) => app,
        Err(error) => {
            // 창이 없는 실패라 stderr만으로는 보이지 않는다(Windows release는 콘솔도 없다).
            let info = app_error_info(&error);
            let code = home_in_use::run_screen_on(event_loop, screen_proxy, info);
            single_instance::instance_file::remove_owned();
            return code;
        }
    };
    app.hub.webhook = Some(webhook);
    hooks::lua::fire(
        app.lua_engine.as_ref(),
        hooks::lua::AutofireCtx {
            scripts: &boot_settings.scripts,
            guard: &mut app.lua_autofire,
            runs: &mut app.lua_runs,
        },
        "tasty.startup.post",
        &serde_json::Value::Null,
    );
    crate::stall_watchdog::spawn();
    event_loop.run_app(&mut app)?;
    let code = app
        .state
        .shutdown
        .as_ref()
        .map_or(0, |state| state.exit_code);
    // 저널 잠금은 App과 함께 놓이므로, 다음 소유자가 낡은 기록을 보지 않게 그 전에 지운다.
    single_instance::instance_file::remove_owned();
    drop_app_with_trace(app);
    #[cfg(target_os = "linux")]
    drop(display_owner);

    Ok(std::process::ExitCode::from(code))
}

/// App 생성이나 첫 화면 조립처럼 터미널을 열기 전 내부 단계의 실패 문구. 같은 내용을 로그에 남긴다.
#[cfg(feature = "gui")]
pub(crate) fn app_error_info(error: &anyhow::Error) -> crate::gpu::BootErrorInfo {
    let title = crate::i18n::t("boot.app_error.title").to_string();
    let body = crate::i18n::t_fmt("boot.app_error.body", &format!("{error:#}"));
    let hint = crate::i18n::t("boot.app_error.hint").to_string();
    tracing::error!("boot failed before the first window: {error:#}\n{title}\n{body}\n{hint}");
    crate::gpu::BootErrorInfo { title, body, hint }
}

/// event_loop 종료 이후 App Drop도 기다릴 수 있어 별도 시간으로 측정한다.
#[cfg(feature = "gui")]
fn drop_app_with_trace(app: App) {
    use std::time::Instant;

    use crate::app::shutdown_trace;

    // 여러 destructor 호출의 시간을 전역 누적기 전후 차이로 합산한다.
    let before = DropTailCounters::snapshot();

    let t_drop = Instant::now();
    drop(app);
    let drop_ms = shutdown_trace::elapsed_ms(t_drop);

    DropTailCounters::snapshot().log_delta(&before);
    tracing::info!(
        target: "tasty::shutdown",
        ms = drop_ms,
        "S5 drop_tail (run_app return -> App drop 완료)"
    );
    if let Some(t0) = shutdown_trace::started_at() {
        tracing::info!(
            target: "tasty::shutdown",
            ms = shutdown_trace::elapsed_ms(t0),
            "shutdown_total_with_drop (사용자 체감 종료 시간)"
        );
    }
    #[cfg(debug_assertions)]
    crate::app::loop_measurement::finish();
}

#[cfg(feature = "gui")]
struct DropTailCounters {
    pty: (std::time::Duration, u64),
    ssh: (std::time::Duration, u64),
}

#[cfg(feature = "gui")]
impl DropTailCounters {
    fn snapshot() -> Self {
        Self {
            pty: tasty_terminal::pty_drop_totals(),
            ssh: tasty_ssh::tunnel_drop_totals(),
        }
    }

    fn log_delta(&self, before: &Self) {
        use crate::app::shutdown_trace::duration_ms;

        tracing::info!(
            target: "tasty::shutdown",
            ms = duration_ms(self.pty.0.saturating_sub(before.pty.0)),
            // layout surface 수가 아니라 실제 PTY를 가진 backend 수다.
            ptys = self.pty.1 - before.pty.1,
            "S5b pty_drop (Pty::drop 합계)"
        );
        tracing::info!(
            target: "tasty::shutdown",
            ms = duration_ms(self.ssh.0.saturating_sub(before.ssh.0)),
            tunnels = self.ssh.1 - before.ssh.1,
            "S5c ssh_tunnel_drop (SshTunnel::drop 합계)"
        );
    }
}

#[cfg(not(feature = "gui"))]
fn run_due_timers(
    app: &mut crate::app::App,
    state: &mut crate::state::RequestContext,
    engine: &mut EngineMut<'_>,
) {
    use std::time::Instant;

    for key in app.timers.drain_due(Instant::now()) {
        match key {
            crate::app::timers::Tick::Busy => {
                // 화면은 없지만 원격 mirror에 상태를 전달해야 한다. 읽는 곳이 없는 StatusBar 브랜치 캐시는 갱신하지 않는다.
                engine.refresh_busy_surfaces();
                engine.forward_busy_activity(&app.stream_hub);
                engine.forward_attention(&app.stream_hub);
                engine.forward_surface_cwd(&app.stream_hub);
                crate::remote::agent_children::tick(&app.stream_hub, &[engine.as_ref()]);
                engine.poll_global_hooks();
                let exec = app.services.hook_executor();
                for event in engine.fire_idle_timeout_hooks(&exec) {
                    engine.enqueue_host_event(event);
                }
                // 플러그인 소켓 입력이 없어도 상태 확인·재시작을 진행하는 주기 경로다.
                headless_plugins::pump_plugins(app, state, engine);
            }
            crate::app::timers::Tick::PtySweep => {
                // 회수 기록과 알림은 공용 함수가 끝낸다.
                engine.reclaim_standalone_ptys(Instant::now());
            }
            crate::app::timers::Tick::CaptureSweep => {
                engine.remote.capture_uploads.sweep_expired(Instant::now());
            }
            crate::app::timers::Tick::LogPrune => {
                let now_ms = u64::try_from(app.services.now_unix_millis()).unwrap_or(0);
                app.services.submit_periodic_prune(now_ms);
            }
        }
    }
    // 두 허브의 최솟값으로 기다렸으므로 플러그인 기한도 함께 처리한다.
    headless_plugins::pump_plugins_if_due(app, state, engine, Instant::now());
}

#[cfg(not(feature = "gui"))]
fn handle_terminal_output(
    app: &mut crate::app::App,
    state: &mut crate::state::RequestContext,
    engine: &mut EngineMut<'_>,
    id: Option<u32>,
) {
    // drain 전에 깨움 중복 방지 표지를 풀어 처리 도중 새 출력의 깨움을 잃지 않게 한다.
    if let Some(factory) = engine.runtime.waker_factory.as_ref() {
        factory.note_drained(id);
    }
    let outcome = match id {
        Some(sid) => app.services.process_pty_output(engine, sid),
        None => {
            let outcome = app.services.process_all_pty_output(engine);
            // 플러그인 수신도 이 기본 waker를 공유하므로 함께 처리한다.
            headless_plugins::pump_plugins(app, state, engine);
            outcome
        }
    };
    for event in outcome.events {
        if let Some((surface, generation)) = event.terminal_binding()
            && !engine
                .runtime
                .terminals
                .matches_generation(surface, generation)
        {
            continue;
        }
        match event {
            crate::app::command::CoreEvent::TerminalProcessExited {
                surface_id,
                generation,
            } => {
                crate::app::process_exit::handle(
                    &mut app.services,
                    state,
                    engine,
                    surface_id,
                    generation,
                );
            }
            crate::app::command::CoreEvent::TerminalCwdChanged { surface_id, .. } => {
                crate::intent::headless::apply_terminal_cwd_changed(engine, surface_id);
            }
            event => fire_terminal_hooks(app, engine, vec![event]),
        }
    }
    crate::intent::headless::drain_pending_host_events(&app.services, engine);
}

/// output-match 훅을 발화하고 HookFired를 큐에 넣는다. PTY 종료는 호출자가 먼저 공용 process_exit 처리로 분기한다.
#[cfg(not(feature = "gui"))]
fn fire_terminal_hooks(
    app: &crate::app::App,
    engine: &mut EngineMut<'_>,
    events: Vec<crate::app::command::CoreEvent>,
) {
    let exec = app.services.hook_executor();
    for event in events {
        let crate::app::command::CoreEvent::TerminalOutputMatch {
            surface_id,
            text,
            generation,
        } = event
        else {
            continue;
        };
        if !engine
            .runtime
            .terminals
            .matches_generation(surface_id, generation)
        {
            continue;
        }
        for fired in engine
            .hooks
            .fire(&exec, surface_id, tasty_hooks::HookEvent::OutputMatch(text))
        {
            engine.enqueue_host_event(fired);
        }
    }
}

/// IPC가 시작됐을 때만 라우터에 의존하는 훅·완료 전략·웹훅을 초기화한다.
#[cfg(not(feature = "gui"))]
fn start_ipc_and_seed(
    app: &mut crate::app::App,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
) {
    let stream_ctx = tasty_ipc::stream_hub::StreamContext {
        hub: app.stream_hub.clone(),
        inbound_tx: app.stream_inbound_tx.clone(),
        waker: app.stream_wake.install(waker.stream_waker()),
        heartbeat_timeout: app.services.attach_heartbeat.clone(),
    };
    let connections = app.services.connections().clone();
    if let Some(lua) = app.lua_engine.as_ref() {
        lua.set_outcome_waker(waker.ipc_waker());
    }
    if let Some(injector) = app
        .hub
        .start_ipc(waker.ipc_waker(), stream_ctx, connections)
    {
        if let Some(server) = app.hub.ipc_server.as_ref() {
            app.services.record_ipc_port(server.port());
        }
        // 웹훅이 참조할 기본 훅 핸들러를 먼저 등록한다.
        crate::hook_handler::install_default_sources();
        // 완료 전략의 notify_via 검증이 훅 핸들러를 참조한다.
        crate::completion_strategy::install_default_sources();
        // 시작 경고는 함수 안에서 알림 채널로 남긴다.
        let reservation = app.hub.webhook.take();
        let _report = app.services.start_webhooks(reservation, injector.clone());
        app.services.set_host_ipc_injector(injector);
    }
}

/// `factory`는 플러그인 매니저와 같은 인스턴스를 받는다.
#[cfg(not(feature = "gui"))]
fn bootstrap_engine(
    app: &mut crate::app::App,
    boot_settings: &crate::settings::Settings,
    factory: crate::waker::SharedWakerFactory,
) -> anyhow::Result<crate::runtime::engine_session::EngineSession> {
    // The engine and file writer share the already normalized boot snapshot.
    app.services
        .submit_settings_save(boot_settings.clone(), None, None);
    app.services.registries.child_terminals.wait_bootstrap();
    let base_waker = factory.make_default_waker();
    // 부팅 때 한 번 전체 슬롯을 기준으로 마이그레이션·scrollback GC를 수행한다.
    crate::core::layout_persistence::migrate_and_gc_on_boot(boot_settings.general.restore_layout);
    if let Some(notice) = layout_persistence_notice(boot_settings.general.restore_layout) {
        tracing::warn!("{notice}");
    }
    // 헤드리스는 슬롯을 점유하지 않으며 레이아웃을 저장·복원하지 않는다.
    let mut engine = crate::runtime::engine_session::EngineSession::for_journal(
        crate::runtime::engine_session::EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker: base_waker,
            shared_ids: None,
            layout_slot: None,
            memory: app.services.memory_access().clone(),
            runner_registry: std::sync::Arc::clone(app.services.tasks.runner_registry()),
        },
        boot_settings.clone(),
        app.services.registries.clone(),
    )?;
    engine.runtime.waker_factory = Some(factory);
    app.journal
        .begin_engine(
            &engine,
            crate::runtime::journal_product::EngineSelection::FreshHeadless,
        )
        .map_err(anyhow::Error::msg)?;
    while engine.journal_binding.is_none() || !app.journal.is_ready(engine.id) {
        app.journal
            .poll_bootstrap(&mut [&mut engine], app.plugin_manager.as_mut())
            .map_err(anyhow::Error::msg)?;
        app.journal
            .poll_restore_bootstrap(&engine)
            .map_err(anyhow::Error::msg)?;
        if !app.journal.is_ready(engine.id) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    crate::app::task_report::apply(&app.services, boot_settings);
    crate::app::anomaly_settings::apply(&app.services, boot_settings);
    // 이전 실행의 agent 상태를 정리하되 작업을 자동 재시작하지는 않는다.
    app.services.tasks.purge_stale_agent_state_on_boot(
        &engine.task_scope,
        &engine
            .core_state
            .workspaces()
            .into_iter()
            .map(|w| w.id)
            .collect::<Vec<_>>(),
    );
    // force-detach 통지에 IPC 서버와 같은 스트림 허브를 사용한다.
    engine.remote.set_notifier(app.stream_hub.clone());
    // 부팅 초기에 보관한 memory.db 대체 사실을 알림 저장소가 생긴 지금 남긴다.
    crate::app::storage_notice::report_memory_fallback(
        &mut engine.borrow_mut(),
        app.services.memory_init_fallback(),
    );
    Ok(engine)
}

/// 기본으로 켜진 레이아웃 복원 설정이 헤드리스에서는 적용되지 않음을 알린다.
#[cfg(not(feature = "gui"))]
fn layout_persistence_notice(restore_layout: bool) -> Option<&'static str> {
    restore_layout.then_some(
        "general.restore_layout is on, but a headless build does not save or restore layouts: \
         workspaces last for this process only (system.info reports layout_slot: null)",
    )
}

#[cfg(not(feature = "gui"))]
enum Wait {
    Event(crate::AppEvent),
    Deadline,
    Disconnected,
}

/// 별도 타이머 스레드 없이 다음 허브 기한까지 recv_timeout으로 기다린다.
#[cfg(not(feature = "gui"))]
fn wait_for_event(
    rx: &std::sync::mpsc::Receiver<crate::AppEvent>,
    deadline: Option<std::time::Instant>,
) -> Wait {
    match deadline {
        Some(at) => {
            match rx.recv_timeout(at.saturating_duration_since(std::time::Instant::now())) {
                Ok(ev) => Wait::Event(ev),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Wait::Deadline,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Wait::Disconnected,
            }
        }
        // 타이머가 없으면 이벤트 수신까지 기다린다.
        None => match rx.recv() {
            Ok(ev) => Wait::Event(ev),
            Err(_) => Wait::Disconnected,
        },
    }
}

#[cfg(not(feature = "gui"))]
fn dispatch_headless_event(
    app: &mut crate::app::App,
    state: &mut crate::state::RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
    event: crate::AppEvent,
) -> std::ops::ControlFlow<()> {
    use crate::AppEvent;
    if matches!(event, AppEvent::JournalReady) {
        return handle_headless_journal_ready(app, state, session, waker);
    }
    if app.journal.is_halted()
        && !matches!(
            event,
            AppEvent::Shutdown | AppEvent::QuitRequested | AppEvent::IpcReady
        )
    {
        return std::ops::ControlFlow::Continue(());
    }
    if app.journal.pauses_observation() && !app.journal.is_halted() {
        if app
            .publication_inputs
            .push(crate::app::publication_input::DeferredEvent::App(event))
            .is_err()
        {
            tracing::error!("headless publication wake queue exceeded its finite event classes");
        }
        return std::ops::ControlFlow::Continue(());
    }
    if matches!(event, AppEvent::IpcReady) {
        waker.note_ipc_drained();
        let flow = headless_dispatch::pump_ipc(app, state, session);
        rewake_if_left(
            flow,
            || ipc_commands_left(&app.services),
            || waker.wake_ipc(),
        );
        return flow;
    }
    let engine = &mut session.borrow_mut();
    match event {
        AppEvent::JournalReady => unreachable!("journal event handled above"),
        AppEvent::Shutdown | AppEvent::QuitRequested => {
            app.state.stopping = true;
            return std::ops::ControlFlow::Break(());
        }
        AppEvent::TerminalOutput(id) => handle_terminal_output(app, state, engine, id),
        AppEvent::IpcReady => unreachable!("IPC event handled above"),
        AppEvent::StreamReady => headless_stream::handle_stream_ready(app, state, engine),
    }
    std::ops::ControlFlow::Continue(())
}

/// 복원·저장 알림을 엔진의 첫 workspace에 넣는다. workspace가 생기기 전이면 다음 차례로 미룬다.
#[cfg(not(feature = "gui"))]
fn deliver_headless_recovery_notices(
    app: &mut crate::app::App,
    session: &mut crate::runtime::engine_session::EngineSession,
) {
    let notices = app.journal.take_notices();
    let Some(workspace) = session
        .core_state
        .local_workspaces()
        .first()
        .map(|ws| ws.id)
    else {
        app.journal.defer_notices(notices);
        return;
    };
    for notice in crate::app::journal::notices::merge_same_level(notices) {
        tracing::warn!("{}: {}", notice.title, notice.body);
        session.live.notifications.add_separate(
            workspace,
            0,
            notice.title,
            notice.body,
            notice.level,
        );
    }
}

/// Publish the committed cut and its replies before replaying deferred observation events.
#[cfg(not(feature = "gui"))]
fn handle_headless_journal_ready(
    app: &mut crate::app::App,
    state: &mut crate::state::RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
) -> std::ops::ControlFlow<()> {
    app.journal
        .update_completion_view(session.id, &session.core_state, &state.navigation);
    if let Err(error) = app
        .journal
        .poll_bootstrap(&mut [session], app.plugin_manager.as_mut())
    {
        tracing::error!("committed structure publication halted: {error}");
    }
    deliver_headless_recovery_notices(app, session);
    if app.journal.is_halted() {
        crate::app::attach_activation::cancel_engine(
            &mut app.pending_server_attaches,
            session.id,
            &mut session.borrow_mut(),
            &app.stream_hub,
        );
    }
    if !app.journal.is_halted() && !app.journal.pauses_observation() {
        for request in app.journal.creation_requests_needing_kind() {
            headless_plugins::ensure_plugin_for_surface_kind(
                app,
                state,
                &mut session.borrow_mut(),
                &request,
            );
        }
        crate::app::attach_activation::poll(
            &mut app.pending_server_attaches,
            &mut app.journal,
            &mut [session],
            &app.stream_hub,
        );
        app.journal
            .resolve_headless_requests(session, state, &app.services);
        app.journal.finish_headless_live_inputs(
            session.id,
            &mut app.services,
            state,
            &mut session.borrow_mut(),
            app.plugin_manager.as_mut(),
        );
        session.borrow_mut().poll_input_submissions();
        for (remote, response) in app.journal.take_remote_results() {
            if remote.engine == session.id {
                crate::app::journal::commands::inbound::deliver_result(
                    remote,
                    response,
                    &mut session.borrow_mut(),
                    app.plugin_manager.as_mut(),
                    &app.stream_hub,
                );
            }
        }
        if !app.journal.take_changed_engines().is_empty() {
            state.reconcile_presentation(&session.core_state);
            session
                .borrow_mut()
                .refresh_attach_presentation(&state.navigation);
        }
        // A server-side close has no incoming stream event to flush its committed delta.
        // Remote results above release their ordering fence before this ordinary observation.
        session.borrow_mut().push_structure_changes();
    }
    app.finish_preset_captures();
    app.journal
        .deliver_plugin_replies(app.plugin_manager.as_mut());
    headless_dispatch::sync_closing_surfaces(app, session);
    while !app.journal.pauses_observation() && !app.journal.is_halted() {
        let Some(crate::app::publication_input::DeferredEvent::App(event)) =
            app.publication_inputs.pop()
        else {
            break;
        };
        let flow = dispatch_headless_event(app, state, session, waker, event);
        if flow.is_break() {
            return flow;
        }
    }
    std::ops::ControlFlow::Continue(())
}

#[cfg(not(feature = "gui"))]
fn rewake_if_left(
    flow: std::ops::ControlFlow<()>,
    commands_left: impl FnOnce() -> bool,
    rewake: impl FnOnce(),
) {
    if flow.is_continue() && commands_left() {
        rewake();
    }
}

/// 아직 꺼내지 않은 명령이 있는지 확인한다. 입장 장부가 없으면 false다.
#[cfg(not(feature = "gui"))]
fn ipc_commands_left(core: &crate::app::services::AppServices) -> bool {
    core.host_ipc_injector
        .get()
        .and_then(|injector| injector.admission())
        .is_some_and(|ledger| ledger.snapshot().queued_commands > 0)
}

/// 자동실행 결과를 알림 대기열에 넣어 같은 회차에 저장한다. headless에는 단축키 실행이 없다.
#[cfg(not(feature = "gui"))]
fn settle_headless_lua_outcomes(app: &mut crate::app::App) {
    if let Some(lua) = app.lua_engine.as_ref() {
        let outcomes = lua.drain_run_outcomes();
        if !outcomes.is_empty() {
            for notice in app.lua_runs.settle(outcomes).notices {
                crate::system_notice::post(notice);
            }
        }
    }
}

#[cfg(not(feature = "gui"))]
fn load_headless_settings() -> crate::settings::Settings {
    let mut settings = crate::settings::Settings::load();
    settings.normalize();
    settings
}

/// gui feature 없는 빌드의 호스트 루프. IPC·PTY·플러그인 이벤트와 타이머를 처리한다.
#[cfg(not(feature = "gui"))]
fn run_headless(cli: cli::Cli) -> anyhow::Result<std::process::ExitCode> {
    use std::sync::mpsc;

    use crate::adapters::production::headless_waker::HeadlessWaker;
    use crate::app::App;

    locale::init();

    let (tx, rx) = mpsc::channel::<crate::AppEvent>();
    let waker = HeadlessWaker::new(tx);

    let boot_settings = load_headless_settings();
    // 명시 지정한 웹훅 포트를 열지 못해도 데몬은 시작한다. 원인은 경고 알림과 webhook.config로 남는다.
    let webhook = crate::webhook::prepare(cli.webhook_port, &boot_settings.webhook);

    let memory_arc = headless_memory::boot_memory(&boot_settings);
    // 비정상 종료한 프로세스가 남긴 디스크 스크롤백을 설정과 관계없이 백그라운드에서 회수한다.
    tasty_terminal::disk_scrollback::spawn_orphan_reclaim();

    let mut app = App::new_headless(waker.journal_waker(), cli.port_file, memory_arc)?;
    app.hub.webhook = Some(webhook);
    // 서버가 받는 첫 연결부터 설정값을 쓰도록 IPC를 열기 전에 반영한다.
    crate::app::attach_heartbeat::apply(&app.services, &boot_settings);
    start_ipc_and_seed(&mut app, &waker);

    // 처음 만든 홈에도 namespace 선언이 있어야 하므로 번들은 부팅 때 설치한다. 프로세스 시작은 지연한다.
    // 엔진이 첫 셸을 만들기 전에 준비해 활성 플러그인의 상속 환경변수 제거 규칙을 그 셸부터 적용한다.
    let factory = waker.waker_factory();
    let registries = headless_plugins::MetadataRegistries {
        file_format: app.services.registries.file_format.clone(),
        file_handler: app.services.registries.file_handler.clone(),
        surface_registry: app.services.registries.surface_registry.clone(),
    };
    headless_plugins::ensure_plugin_manager_metadata_with(&mut app, factory.clone(), registries);
    if let Some(mgr) = app.plugin_manager.as_mut() {
        crate::plugin::install_builtins_if_needed(mgr);
        // 권한 판정과 매니저가 같은 namespace 표를 공유하며 refresh_packages가 내용을 채운다.
        mgr.install_namespace_table_once();
        mgr.refresh_packages();
        mgr.recompute_extensions();
        mgr.publish_shared_tables();
    }

    let mut session = bootstrap_engine(&mut app, &boot_settings, factory)?;
    let engine_id = session.id;
    let mut engine = session.borrow_mut();
    let preset_store = app.services.preset_store.clone();
    let mut state = crate::state::CommandContext::new(&engine.read(), preset_store);
    state.engine_id = Some(engine_id);

    hooks::lua::fire(
        app.lua_engine.as_ref(),
        hooks::lua::AutofireCtx {
            scripts: &boot_settings.scripts,
            guard: &mut app.lua_autofire,
            runs: &mut app.lua_runs,
        },
        "tasty.startup.post",
        &serde_json::Value::Null,
    );

    app.state.started = true;
    tracing::info!("headless daemon ready; PTY pump + IPC dispatch active");

    loop {
        app.services.poll_memory_writer();
        app.services.poll_file_writer();
        engine.poll_child_registry_effects();
        app.finish_preset_files();
        app.finish_plugin_files();
        app.services
            .poll_memory_replies(app.plugin_manager.as_mut());
        crate::intent::headless::settle_settings_saves(&mut app.services);
        if !app.journal.pauses_observation() && !app.gate_queue.is_empty() {
            // 게이트 결과를 기다리던 요청을 이어 간다. 정지된 journal은 진입 경로가 거절한다.
            let flow = headless_dispatch::advance_request_gates(&mut app, &mut state, &mut session);
            engine = session.borrow_mut();
            if flow.is_break() {
                break;
            }
        }
        if !app.journal.is_halted() && !app.journal.pauses_observation() {
            crate::ipc::handler::telemetry_follow_up::run(
                &mut app.services,
                &mut state,
                &mut engine,
            );
            // The service applies/logs results; headless has no popup to receive display updates.
            let _ = app.services.profile_detections.poll();
            engine.poll_attach_subscriptions();
            settle_headless_lua_outcomes(&mut app);
            crate::intent::headless::queue_system_notices(&mut state, &engine);
            crate::intent::headless::drain_pending_intents_in_app(
                &mut app.services,
                &mut state,
                &mut engine,
                &mut app.journal,
            );
            crate::intent::headless::drain_pending_host_events(&app.services, &mut engine);
        }
        // 대기 전에 agent 이벤트를 발행한다. 대기 중 새 항목이 쌓이면 다음 루프에서 전달한다.
        if !app.journal.pauses_observation() {
            let mut agent_events = Vec::new();
            let mut dropped = 0u64;
            crate::app::agent_events::take_from(
                engine.task_scope.event_queue(),
                &mut agent_events,
                &mut dropped,
            );
            crate::app::agent_events::emit(app.plugin_manager.as_mut(), agent_events, dropped);
        }
        app.dispatch_pending_memory_changes();
        let deadline = if app.journal.is_halted() || app.journal.pauses_observation() {
            None
        } else {
            crate::app::timers::min_deadline(
                app.timers.next_deadline(),
                app.plugin_manager.as_ref().and_then(|m| m.next_deadline()),
            )
        };
        let deadline =
            crate::app::timers::min_deadline(deadline, app.journal.cleanup_poll_deadline());
        let deadline = crate::app::timers::min_deadline(deadline, app.runner_stop_poll_deadline());
        let deadline = crate::app::timers::min_deadline(deadline, app.plugin_file_poll_deadline());
        let deadline = if app.journal.is_halted() || app.journal.pauses_observation() {
            deadline
        } else {
            crate::app::timers::min_deadline(deadline, engine.runtime.input_submit_deadline())
        };
        let deadline = if app.journal.is_halted() || app.journal.pauses_observation() {
            deadline
        } else {
            crate::app::timers::min_deadline(
                deadline,
                app.services
                    .profile_detections
                    .has_pending()
                    .then(|| std::time::Instant::now() + std::time::Duration::from_millis(20)),
            )
        };
        let pending = match wait_for_event(&rx, deadline) {
            Wait::Event(ev) => Some(ev),
            Wait::Deadline
                if app.journal.cleanup_poll_deadline().is_some()
                    || engine.runtime.input_submit_deadline().is_some() =>
            {
                Some(crate::AppEvent::JournalReady)
            }
            Wait::Deadline => None,
            Wait::Disconnected => break,
        };

        if !app.journal.is_halted() && !app.journal.pauses_observation() {
            run_due_timers(&mut app, &mut state, &mut engine);
        }

        let Some(event) = pending else {
            continue;
        };
        let flow = dispatch_headless_event(&mut app, &mut state, &mut session, &waker, event);
        engine = session.borrow_mut();
        if flow.is_break() {
            break;
        }
    }
    finish_headless_shutdown(&mut app, &mut session);
    Ok(std::process::ExitCode::SUCCESS)
}

/// A retirement awaiting receipts does not defer Shutdown. Finish it first so its close replies
/// once and records its outcome; one shutdown deadline bounds this wait.
#[cfg(not(feature = "gui"))]
fn drain_headless_publication(
    app: &mut crate::app::App,
    session: &mut crate::runtime::engine_session::EngineSession,
) {
    let mut deadline = None;
    while app
        .journal
        .shutdown_publication_pending(&mut deadline, std::time::Instant::now())
    {
        if let Some(plugins) = app.plugin_manager.as_mut()
            && let Err(error) = plugins.poll_publication_retirements()
        {
            tracing::warn!("headless shutdown could not advance plugin retirements: {error}");
        }
        if let Err(error) = app
            .journal
            .poll_bootstrap(&mut [session], app.plugin_manager.as_mut())
        {
            tracing::error!("committed structure publication halted: {error}");
        }
        app.journal
            .deliver_plugin_replies(app.plugin_manager.as_mut());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Stop admission and retain the original owners while observing shutdown receipts.
#[cfg(not(feature = "gui"))]
fn finish_headless_shutdown(
    app: &mut crate::app::App,
    session: &mut crate::runtime::engine_session::EngineSession,
) {
    app.services.webhooks.begin_shutdown();
    let file_drain = file_drain::begin(app, session);
    // 게이트 결과를 기다리던 요청은 실행하지 않고 종료 오류로 답한다.
    app.gate_queue.reject_all(app.plugin_manager.as_mut());
    drain_headless_publication(app, session);
    app.journal.begin_process_shutdown();
    // Begin stop now; the loop below observes this same retained receipt until its deadline.
    let _ = session.poll_runner_stop(&app.services.tasks);
    app.services.profile_detections.begin_shutdown();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let profiles = app.services.profile_detections.poll_shutdown();
        let runners = session.poll_runner_stop(&app.services.tasks);
        // The control pump processes disposal acknowledgements only. It must never start a new
        // plugin creation or replay input while the publication owner is shutting down.
        let retained = app
            .journal
            .release_halted_resources(session, app.plugin_manager.as_mut());
        let resources = retained
            && crate::runtime::resource_retirement::poll_engine_release(
                session,
                app.plugin_manager.as_mut(),
            );
        let runners_joined = !matches!(runners, tasty_task_runtime::RunnerStopObservation::Waiting);
        if profiles == 0 && runners_joined && resources {
            break;
        }
        if std::time::Instant::now() >= deadline {
            tracing::warn!(
                profiles,
                runners_joined,
                resources_reaped = resources,
                "headless shutdown timed out; unfinished owners have no completion receipt"
            );
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    observer_drain::drain(app, session);
    file_drain::drain_engine(app, session, file_drain);
    drain_headless_memory_writer(app);
}

/// memory.db에 쓰는 producer를 멈춘 뒤 writer의 접수를 막고 마지막 flush와 join을 기다린다.
/// 마지막 묶음의 결과를 기다리던 IPC 요청에도 응답한다.
#[cfg(not(feature = "gui"))]
fn drain_headless_memory_writer(app: &mut crate::app::App) {
    let mut drain = crate::app::writer_drain::WriterDrain::default();
    loop {
        let remaining = app.services.poll_memory_writer_shutdown();
        app.collect_plugin_file_shutdown();
        if !drain.observe_writer(
            remaining,
            app.services.memory_writer_shutdown_report(),
            "memory",
            std::time::Instant::now(),
            "headless memory writer shutdown timed out; requests it has not committed may be lost",
        ) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(all(test, not(feature = "gui")))]
mod tests {
    use super::{layout_persistence_notice, rewake_if_left};
    use std::ops::ControlFlow;

    #[test]
    fn a_restore_layout_setting_is_announced_as_ignored() {
        let notice = layout_persistence_notice(true).expect("켜진 설정은 알려야 한다");
        assert!(notice.contains("restore_layout") && notice.contains("headless"));
        assert!(
            notice.contains("layout_slot: null"),
            "system.info에서 확인할 필드도 안내해야 한다"
        );
        assert_eq!(layout_persistence_notice(false), None);
    }

    #[test]
    fn a_round_that_left_commands_wakes_the_loop_again() {
        let mut woke = 0;
        rewake_if_left(ControlFlow::Continue(()), || true, || woke += 1);
        assert_eq!(woke, 1);
    }

    #[test]
    fn an_empty_or_final_round_does_not_wake_the_loop() {
        let mut woke = 0;
        rewake_if_left(ControlFlow::Continue(()), || false, || woke += 1);
        rewake_if_left(ControlFlow::Break(()), || true, || woke += 1);
        assert_eq!(woke, 0);
    }
}

#[cfg(all(test, not(feature = "gui")))]
mod journal_event_tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Instant;

    #[test]
    fn public_creation_finishes_using_only_real_journal_wakes_and_cleanup_deadlines() {
        let _home = crate::test_support::IsolatedHome::new();
        let (tx, events) = mpsc::channel();
        let waker = crate::adapters::production::headless_waker::HeadlessWaker::new(tx);
        let memory = Arc::new(Mutex::new(
            tasty_memory::MemoryStore::open_in_memory().unwrap(),
        ));
        let mut app =
            crate::app::App::new_headless(waker.journal_waker(), None, Some(memory)).unwrap();
        let mut settings = crate::settings::Settings::default();
        settings.general.shell = "/bin/sh".into();
        settings.general.startup_command = "exec sleep 60".into();
        let mut session = crate::runtime::engine_session::EngineSession::for_journal(
            crate::runtime::engine_session::EngineSessionSpec {
                cols: 80,
                rows: 24,
                waker: waker.waker_factory().make_default_waker(),
                shared_ids: None,
                layout_slot: None,
                memory: app.services.memory_access().clone(),
                runner_registry: Arc::clone(app.services.tasks.runner_registry()),
            },
            settings,
            app.services.registries.clone(),
        )
        .unwrap();
        app.journal
            .begin_engine(
                &session,
                crate::runtime::journal_product::EngineSelection::FreshHeadless,
            )
            .unwrap();
        let mut state = crate::state::CommandContext::new(
            &session.as_ref().read(),
            app.services.preset_store.clone(),
        );
        state.engine_id = Some(session.id);
        // 기다리는 깨움은 모두 저널 worker 가 보낸다. 부하로 worker 의 fsync 가 늦어진 시간은 세지 않고
        // worker 가 잠들어 있던 시간을 센다. 깨움이 와도 되돌리지 않고 두 단계를 합쳐 누적하며, 정리 기한을
        // 기다리는 동안도 worker 는 잠들어 있으므로 센다(docs/dev-guide/unit-test-isolation.md).
        let mut stall =
            crate::runtime::journal_product::stall_budget::StallBudget::new(&app.journal);
        while !app.journal.is_ready(session.id) {
            let event = match events.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => {
                    stall.nap("bootstrap wake");
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => panic!("bootstrap wake: disconnected"),
            };
            assert!(
                dispatch_headless_event(&mut app, &mut state, &mut session, &waker, event)
                    .is_continue()
            );
        }
        let (reply, rx) = mpsc::sync_channel(1);
        let request=serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","method":"workspace.create","params":{"name":"event-driven"},"id":1,"idempotency_key":"event-create"})).unwrap();
        assert!(app.journal.admit_ipc(
            &crate::ipc::server::IpcCommand::new(request, reply),
            &crate::ipc::caller::CallerContext::local()
        ));
        let response = loop {
            if let Ok(response) = rx.try_recv() {
                break response;
            }
            // 정리 기한은 메인 루프의 타이머처럼 지났을 때만 깨움으로 대신한다. 이때도 nap 을 거쳐야 기한이
            // 과거에 머문 채 응답이 오지 않는 정체가 한도 안에서 실패한다.
            let event = match events.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty)
                    if app
                        .journal
                        .cleanup_poll_deadline()
                        .is_some_and(|deadline| Instant::now() >= deadline) =>
                {
                    stall.nap("cleanup deadline wake");
                    crate::AppEvent::JournalReady
                }
                Err(mpsc::TryRecvError::Empty) => {
                    stall.nap("missing event-driven continuation");
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("missing event-driven continuation: disconnected")
                }
            };
            assert!(
                dispatch_headless_event(&mut app, &mut state, &mut session, &waker, event)
                    .is_continue()
            );
        };
        assert!(response.error.is_none(), "{response:?}");
        assert_eq!(response.result.unwrap()["name"], "event-driven");
        assert_eq!(session.core_state.local_workspaces().len(), 2);
        assert!(!app.journal.pauses_observation());
        assert!(!app.journal.is_halted());
    }
}
