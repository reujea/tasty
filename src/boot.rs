//! `fn main` 부팅 시퀀스 오케스트레이션.
//!
//! `run()` 이 단일 진입점. 내부 단계 순서:
//!
//! 1. OS 보정 (Windows console attach, crash_report::init — panic hook + stderr tracing)
//! 2. CLI 라우팅 결정 (`cli_routing::parse_or_route`)
//! 3. 결정에 따라 mode helper 호출:
//!    - `AlreadyHandled` → Ok(())
//!    - `Subcommand` → i18n init + `cli::run_client_with`
//!    - `AugmentedHelp` → i18n init + `cli::print_augmented_help`
//!    - `Gui` → 공유 로그 파일 개방(`os::enable_host_file_log`) + i18n init +
//!      event loop / background threads / App / event_loop.run_app
//!      (gui 빌드 — `--headless` 는 warn 한 줄 뒤 무시된다) — 또는 `run_headless`
//!      (headless 빌드. 헤드리스 여부는 플래그가 아니라 `gui` feature 가 정한다)

pub(crate) mod cli_routing;
#[cfg(feature = "gui")]
pub(crate) mod event_loop;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_dispatch;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_plugins;
#[cfg(not(feature = "gui"))]
pub(crate) mod headless_stream;
pub(crate) mod locale;
pub(crate) mod locale_font;
pub(crate) mod os;
#[cfg(feature = "gui")]
pub(crate) mod trace;
#[cfg(feature = "gui")]
pub(crate) mod waker;
pub(crate) mod wiring;

#[cfg(feature = "gui")]
use crate::App;
use crate::{cli, hooks};

fn log_vacuum_result(result: tasty_memory::Result<bool>) {
    match result {
        Ok(true) => tracing::info!("boot memory maintenance: vacuumed memory.db"),
        Ok(false) => {}
        Err(e) => tracing::warn!("boot memory maintenance: vacuum failed: {e}"),
    }
}

/// 대량 삭제 직후에만 압축(freelist 가 클 때) — `pruned == 0` 이면 평소 부팅처럼 no-op.
fn vacuum_if_needed(store: &mut tasty_memory::MemoryStore, pruned: u64) {
    if pruned == 0 {
        return;
    }
    tracing::info!("boot memory maintenance: pruned {pruned} stale log rows");
    log_vacuum_result(store.vacuum_if_fragmented(10_000));
}

/// 이미 비대해진 WAL 회수 — `journal_size_limit`(`tasty_memory::WAL_SIZE_LIMIT_BYTES`)
/// 은 되감기 때만 작동해 커지는 것 자체는 못 막으므로, 기존 인스턴스의 큰 WAL 은
/// 되감기를 한 번 강제해야 줄어든다.
///
/// **VACUUM 뒤에** 부른다: VACUUM 은 DB 전체를 다시 쓰므로 그 자체로 WAL 을 크게
/// 부풀린다. 순서를 뒤집으면 잘라낸 직후 다시 커진 채로 부팅이 끝난다.
fn truncate_wal(store: &mut tasty_memory::MemoryStore) {
    match store.checkpoint_truncate() {
        Ok(true) => {}
        // busy — 다른 커넥션이 읽는 중이라 이번엔 못 줄였다. 다음 부팅에 다시 시도되고
        // 그 사이에도 pragma 가 상한을 지키므로 정보 수준으로만 남긴다.
        Ok(false) => {
            tracing::info!("boot memory maintenance: wal checkpoint was busy; wal left as is")
        }
        Err(e) => tracing::warn!("boot memory maintenance: wal checkpoint failed: {e}"),
    }
}

/// boot 시 1회 memory.db 위생 정리.
///
/// audit/telemetry 는 append-only 로그라 `memory` 테이블을 무한 채운다(per-IPC audit
/// 가 수십만 행 누적). put 은 이제 O(1)(전체 스캔 제거)이라 성능 목적은 아니며, 무한
/// 누적으로 인한 디스크 증가와 1GB regular quota 도달을 막는 retention 이다. 정책은
/// `store::log_retention` 이 소유하고 런타임 append 경로와 공유한다 — 부팅
/// 경로만 있으면 재시작 전까지 무제한으로 자란다(그게 원래 상태였다). 조용히(이벤트
/// 없이) 삭제 후 단편화가 크면 1회 VACUUM 으로 회수하며, 최초 1회만 대량(수십만 행)
/// 삭제로 ~2s 소요될 수 있고 이후 부팅은 초과분만 정리한다.
fn maintain_memory_at_boot(arc: &std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>) {
    // 상한 값은 `store::log_retention` 이 단독으로 소유한다 — 런타임 집행
    // 경로가 같은 테이블을 읽는다. 여기에 숫자를 다시 적으면 두 경로가 갈린다.
    let mut store = crate::poison::recover_mutex(
        arc.lock(),
        crate::core::MEMORY_WHAT,
        &crate::core::MEMORY_POISONED,
    );
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

/// 프로세스 진입점. `src/main.rs` 가 부르는 유일한 lib 항목이라 `pub` 이다 —
/// 그 밖의 모듈은 여전히 `pub(crate)` 다.
pub fn run() -> anyhow::Result<()> {
    os::attach_windows_console_if_needed();
    os::init_crash_report();

    match cli_routing::parse_or_route()? {
        cli_routing::Routed::AlreadyHandled => Ok(()),
        cli_routing::Routed::Subcommand(cmd, port_file, envelope) => {
            run_subcommand(cmd, port_file, envelope)
        }
        cli_routing::Routed::AugmentedHelp => run_augmented_help(),
        cli_routing::Routed::Gui(cli) => {
            // 공유 로그 파일은 host 만 연다(= 여기서 연다). CLI 클라이언트도 같은
            // 바이너리라, 역할 판정 전에 열면 CLI 를 한 번 돌릴 때마다 실행 중인
            // host 의 로그가 truncate 된다.
            os::enable_host_file_log();
            // 호스트(터미널·plugin 을 spawn 하는 프로세스)에서만 자식 결박 job 을 생성한다.
            // CLI client / augmented-help 경로는 터미널을 띄우지 않으므로 제외. 이 job 이
            // tasty 프로세스 사망 시 자식 셸 트리를 함께 정리한다(비-Windows 는 no-op).
            tasty_reaper::init_host_reaper();
            #[cfg(feature = "gui")]
            {
                if cli.headless {
                    tracing::warn!(
                        "--headless requested in gui build; gui build does not embed headless mode. \
                         Build with --no-default-features to enable headless. Falling back to run_gui."
                    );
                }
                run_gui(cli)
            }
            #[cfg(not(feature = "gui"))]
            {
                run_headless(cli)
            }
        }
    }
}

/// `cli.command.is_some()` — i18n 후 client mode 진입.
fn run_subcommand(
    cmd: cli::Commands,
    port_file: Option<String>,
    envelope: cli::Envelope,
) -> anyhow::Result<()> {
    locale::init();
    cli::run_client_with(cmd, port_file.as_deref(), envelope)
}

/// `TASTY_SURFACE_ID` + `!cli.launch` — i18n 후 augmented help 출력.
fn run_augmented_help() -> anyhow::Result<()> {
    locale::init();
    cli::print_augmented_help()
}

/// 본 GUI 부트.
#[cfg(feature = "gui")]
fn run_gui(cli: cli::Cli) -> anyhow::Result<()> {
    locale::init();

    let (event_loop, proxy) = event_loop::build()?;
    os::install_macos_delegate(&proxy);

    // CWD는 OSC 7 시퀀스에만 의존한다. 모든 플랫폼 공통.
    // zsh/fish는 기본 지원, bash는 PROMPT_COMMAND 설정 필요.

    // Settings 와 Memory store 를 App 생성 *이전* 에 초기화.
    // Core 가 처음부터 실 Memory store 의 Arc 를 보유한다. 글로벌 STORE 싱글톤은
    // 폐기됨 — Arc 가 유일한 store handle.
    let boot_settings = crate::settings::Settings::load();
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

    let mut app = App::new(
        proxy,
        cli.port_file,
        memory_arc,
        #[cfg(debug_assertions)]
        cli.enable_input_simulation,
    )?;
    hooks::lua::fire(
        app.lua_engine.as_ref(),
        hooks::lua::AutofireCtx {
            scripts: &boot_settings.scripts,
            guard: &mut app.lua_autofire,
        },
        "tasty.startup.post",
        &serde_json::Value::Null,
    );
    // 이벤트 루프 stall 관측 시작 — 펌프가 멎으면 이 스레드만 살아남아 로그를 남긴다.
    crate::stall_watchdog::spawn();
    event_loop.run_app(&mut app)?;
    drop_app_with_trace(app);

    Ok(())
}

/// `event_loop.run_app` 반환 후의 **Drop tail** 계측 (S5 계열).
///
/// `app` 을 스코프 끝 암묵 drop 에 맡기면 계측을 끼울 자리가 없어 명시 drop 으로
/// 전환했다(동작은 동일 — drop 시점이 같은 함수의 몇 줄 앞으로 당겨질 뿐이다).
///
/// 이 구간이 중요한 이유: `event_loop.exit()` 시점에 창은 이미 사라졌는데 블로킹
/// destructor(LuaEngine join / PluginProcess wait / SshTunnel wait / PTY kill)가
/// 여기서 직렬로 돈다. 즉 **종료 화면으로 덮을 수 없는 체감 시간**이며,
/// `shutdown_total` 만 재면 통째로 놓친다.
#[cfg(feature = "gui")]
fn drop_app_with_trace(app: App) {
    use std::time::Instant;

    use crate::app::shutdown_trace;

    // Drop tail 내부 세분(S5b/S5c)은 destructor 가 surface·세션 수만큼 반복돼
    // 개별 로그로는 읽기 어렵다. 크레이트별 전역 누적기의 **전후 델타**로 잰다.
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
}

/// Drop tail 세분 계측(S5b/S5c)의 스냅샷 — 크레이트별 전역 누적기 값.
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

    /// `before` 대비 증가분을 S5b/S5c 로 찍는다.
    fn log_delta(&self, before: &Self) {
        use crate::app::shutdown_trace::duration_ms;

        tracing::info!(
            target: "tasty::shutdown",
            ms = duration_ms(self.pty.0.saturating_sub(before.pty.0)),
            // 필드명이 S3 의 `surfaces` 와 다른 이유: 여기서 세는 건 **PTY 를 실제로
            // 가진 backend** 라 layout 상의 surface 수와 일치하지 않는다(child
            // terminal / headless PTY 는 layout 밖에도 있고, PTY 없는 surface 도 있다).
            ptys = self.pty.1 - before.pty.1,
            "S5b pty_drop (PtyBackend::drop 합계)"
        );
        tracing::info!(
            target: "tasty::shutdown",
            ms = duration_ms(self.ssh.0.saturating_sub(before.ssh.0)),
            tunnels = self.ssh.1 - before.ssh.1,
            "S5c ssh_tunnel_drop (SshTunnel::drop 합계)"
        );
    }
}

/// 시간축 — 이번 바퀴에 due 한 타이머 키를 전부 실행하고, plugin 허브의 데드라인이
/// 지났으면 그쪽도 거둔다. gui `about_to_wait` 의 drain 블록과 동형이며, 각 arm 이
/// 무엇의 headless 등가인지는 arm 주석에 있다.
#[cfg(not(feature = "gui"))]
fn run_due_timers(
    app: &mut crate::app::App,
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
) {
    use std::time::Instant;

    for key in app.timers.drain_due(Instant::now()) {
        match key {
            crate::app::timers::Tick::Busy => {
                // 렌더가 없어 로컬 redraw 는 무의미하지만(반환값 무시), attach
                // client 로의 busy forward 는 headless 가 원격 attach 의 주
                // 시나리오라 필수 — gui `app/busy.rs` 의 `poll_busy_states` 와
                // 동형(엔진 1 개라 순회 불필요).
                // StatusBar 브랜치 캐시(`core/state/branch.rs`)는 **의도적으로**
                // 여기에 배선하지 않는다 — headless 는 StatusBar 를 렌더하지 않아
                // 읽는 쪽이 없고(그래서 캐시 자체가 `gui` feature 게이트다), 갱신하면
                // 읽히지도 않을 `.git/HEAD` 를 초당 한 번 여는 것이 된다.
                engine.refresh_busy_surfaces();
                engine.forward_busy_activity(&app.stream_hub);
                // attention forward 도 같은 tick(gui `app/busy.rs` 와 동형).
                // headless 가 원격 attach 의 주 시나리오라 이 배선이 없으면
                // mirror 는 서버 attention 을 영원히 못 받는다.
                engine.forward_attention(&app.stream_hub);
                // cwd forward 도 같은 tick — 빠지면 headless 서버를 mirror 하는 client 는
                // OSC 7 을 안 쏘는 셸의 cwd 를 영영 못 받는다.
                engine.forward_surface_cwd(&app.stream_hub);
                // 글로벌 훅 — gui `app/global_hooks.rs` 의 `poll_global_hooks` 와
                // 동형(엔진 1 개라 순회 불필요).
                engine.poll_global_hooks();
                // IdleTimeout 훅 — gui `app/idle_hooks.rs` 의
                // `poll_idle_timeout_hooks` 와 동형(엔진 1 개라 순회 불필요).
                // 바인딩 실행 + host event enqueue 는 여기서 직접 한다(엔진
                // 레이어는 순수 조회만 함 — `CoreState::poll_idle_timeout_hooks`).
                let injector = app.core.host_ipc_injector.get().cloned();
                for (surface_id, f) in engine.poll_idle_timeout_hooks() {
                    crate::hook_handler::trigger::execute_binding(
                        &f.binding,
                        injector.as_ref(),
                        &f.event,
                        &f.received,
                        surface_id,
                    );
                    state.enqueue_host_event(crate::state::PendingHostEvent::HookFired {
                        hook_id: f.hook_id,
                        event_kind: "idle-timeout".to_string(),
                        surface_id,
                        exit_code: None,
                    });
                }
                // plugin 소켓이 조용해도 healthcheck/재시작 타이머가 진행되도록
                // 1Hz 안전망으로 편승(주 wake 경로는 TerminalOutput(None)).
                headless_plugins::pump_plugins(app, state, engine);
            }
            // TTL 정리 3종 — gui `app/sweeps.rs` 와 동형(엔진 1 개라 순회 불필요).
            // 접근 시점 lazy 경로를 대체하지 않고 보완한다
            // (`docs/adr/0050-headless-pty-primitive.md` "좀비 회수 시점").
            // headless 야말로 이 보완이 가장 필요한 실행 형태다 — GUI 조작이
            // 아예 없어 lazy 를 굴릴 사용자 접근 자체가 없다.
            crate::app::timers::Tick::PtySweep => {
                // 반환 id 는 쓰지 않는다 — 두 store 회수까지 공용 함수가 끝냈다.
                let _ = engine.sweep_idle_ptys(Instant::now());
            }
            crate::app::timers::Tick::CaptureSweep => {
                engine.capture_uploads.sweep_expired(Instant::now());
            }
            crate::app::timers::Tick::LogPrune => {
                let now_ms = u64::try_from(app.core.now_unix_millis()).unwrap_or(0);
                app.core.with_memory(|mem| {
                    crate::store::log_retention::maybe_prune(mem, now_ms);
                });
            }
        }
    }
    // plugin 허브의 시간축 — 루프는 두 허브의 `min` 에 깨어나므로 여기서 plugin 쪽도
    // 거둔다. 빠지면 plugin 데드라인이 과거에 남아 대기가 0 이 되고 루프가 다음
    // `Tick::Busy` 까지 헛돈다(gui 는 `about_to_wait` 가 깨어날 때마다 `mgr.pump`).
    headless_plugins::pump_plugins_if_due(app, state, engine, Instant::now());
}

/// PTY 출력 wake 처리 — dedup 게이트 해제 후 대상 surface(또는 전체)를 drain 한다.
#[cfg(not(feature = "gui"))]
fn handle_terminal_output(
    app: &mut crate::app::App,
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
    id: Option<u32>,
) {
    // Early reset: drain 직전에 dedup 게이트를 풀어 경합 wake 유실 방지
    // (research §8). headless 는 단일 engine 이라 순회 불필요.
    if let Some(factory) = engine.waker_factory.as_ref() {
        factory.note_drained(id);
    }
    // Targeted wake 는 해당 surface 만, default wake 는 전체 drain.
    // View-independent output hooks and explicit PTY exit lifecycle run in both hosts.
    let outcome = match id {
        Some(sid) => app.core.process_pty_output(engine, sid), // targeted: 해당 surface 만 drain
        None => {
            let outcome = app.core.process_all_pty_output(engine); // default: 전체 drain
            // plugin 프로세스 수신 스레드도 이 default waker 를 공유한다
            // (headless_plugins 모듈 주석 참조) — hello 응답/PaintFrame 등
            // plugin 이벤트도 이 wake 로 도착하므로 여기서 함께 pump.
            headless_plugins::pump_plugins(app, state, engine);
            outcome
        }
    };
    for event in outcome.events {
        if let crate::core::intent::CoreEvent::TerminalProcessExited { surface_id } = event {
            crate::app::process_exit::handle(&mut app.core, state, engine, surface_id);
        } else {
            fire_terminal_hooks(app, state, engine, vec![event]);
        }
    }
    crate::intent::headless::drain_pending_host_events(&app.core, state, engine);
}

/// PTY drain의 output-match와 process-exit 훅을 발화한다.
///
/// gui 는 `App::cascade_terminal_output_match`(`app/dispatch_domain.rs`)가 하는
/// 일이고, headless 에는 그 cascade 층이 없다(`app/dispatch_domain_stubs.rs`).
/// stub 의 근거는 "cascade 는 View 의 모든 window 에 broadcast 하는 것" 인데
/// **훅 실행은 view 와 무관한 부수효과**라 그 근거가 닿지 않는다 — 같은 stub
/// 파일에 묶여 함께 죽어 있었다. `tasty set hook --event output-match:...` 는
/// CLI 로 노출된 에이전트 기능이므로 headless 에서도 동작해야 한다
/// (`docs/identity.md` 원칙 2).
/// process-exit도 GUI의 종료 cascade와 같이 알리고 surface를 정리한다.
/// 종료 binding은 먼저 모으고 surface를 닫은 뒤 실행해, 완료 알림이 죽은
/// surface를 살아 있다고 보고 형제 hook을 다시 등록하지 않게 한다.
///
/// 이 output-match 함수는 기존처럼 직접 훅 실행만 수행한다. process-exit는
/// 공용 종료 처리에서 HookFired를 enqueue하며, 위 PTY drain이 headless의
/// host-event 소비자를 호출해 task waiter를 처리한다. view/plugin broadcast는
/// headless 소비 범위에 포함하지 않는다.
#[cfg(not(feature = "gui"))]
fn fire_terminal_hooks(
    app: &crate::app::App,
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
    events: Vec<crate::core::intent::CoreEvent>,
) {
    let injector = app.core.host_ipc_injector.get().cloned();
    for event in events {
        let (surface_id, hook_event, exited) = match event {
            crate::core::intent::CoreEvent::TerminalOutputMatch { surface_id, text } => {
                (surface_id, tasty_hooks::HookEvent::OutputMatch(text), false)
            }
            crate::core::intent::CoreEvent::TerminalProcessExited { surface_id } => {
                (surface_id, tasty_hooks::HookEvent::ProcessExit, true)
            }
            _ => continue,
        };
        let fired = engine
            .hook_manager
            .check_and_fire(surface_id, &[hook_event]);
        if exited {
            // intent-exempt: headless PTY 종료 이벤트의 cascade — GUI와 같은 종료 정리 경계다.
            state.close_surface_by_id_no_snapshot(engine, surface_id, true);
        }
        for f in fired {
            crate::hook_handler::trigger::execute_binding(
                &f.binding,
                injector.as_ref(),
                &f.event,
                &f.received,
                surface_id,
            );
        }
    }
}

/// 부팅 시 memory.db 초기화 — 설정의 상한/쿼터를 반영하고 유지보수를 1 회 돌린다.
/// 실패해도 데몬은 뜬다(memory 없는 상태로 계속) — 반환 `None` 이 그 상태다.
#[cfg(not(feature = "gui"))]
fn boot_memory(
    boot_settings: &crate::settings::Settings,
) -> Option<std::sync::Arc<std::sync::Mutex<tasty_memory::MemoryStore>>> {
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
    memory_arc
}

/// IPC accept 스레드를 띄우고, 그 라우터를 필요로 하는 전역 레지스트리를 시드한다.
/// `start_ipc` 가 injector 를 돌려주지 않으면(IPC 미기동) 시드도 하지 않는다 —
/// 시드 대상이 전부 IPC 라우터를 전제하기 때문이다.
#[cfg(not(feature = "gui"))]
fn start_ipc_and_seed(
    app: &mut crate::app::App,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
) {
    let stream_ctx = tasty_ipc::stream_hub::StreamContext {
        hub: app.stream_hub.clone(),
        inbound_tx: app.stream_inbound_tx.clone(),
        waker: waker.stream_waker(),
    };
    let connections = app.core.connections().clone();
    if let Some(injector) = app
        .hub
        .start_ipc(waker.ipc_waker(), stream_ctx, connections)
    {
        // 웹훅 리스너 init (headless). start_ipc 이후 = (B)IPC 처리 가능. config 는
        // 아래 CoreState::new_with_ids 에서 로드되지만 리스너는 IPC 라우터만
        // 필요하므로 이 시점 주입으로 충분. headless 엔 init_app_state 가 없어 여기서 호출.
        // headless 는 toast UI 가 없으므로 포트 미설정/bind 실패 경고는 리스너 내부
        // `tracing::warn!` 로만 노출된다(S8) — 반환 report 는 여기서 소비하지 않는다.
        //
        // 공유 훅 핸들러 레지스트리 시드(host embedded 기본값 + user config). 웹훅
        // 바인딩·`hook_handler.*` 조회가 이 전역 레지스트리를 보므로 리스너 init 전에
        // 채운다(plugin contribution 은 이후 discover_and_start 에서 병합).
        crate::hook_handler::install_default_sources();
        // 완료 판정 전략 레지스트리 시드 — 훅 핸들러와 대칭 위치.
        // notify_via 참조 무결성 검증이 훅 핸들러 레지스트리를 보므로 그 뒤에 둔다.
        crate::completion_strategy::install_default_sources();
        // 반환 report 는 여기서 소비하지 않는다 — headless 엔 toast UI 가 없어
        // 포트 미설정/bind 실패는 리스너 내부 `tracing::warn!` 로만 노출된다.
        let _ = crate::webhook::init_from_config(injector.clone());
        app.core.set_host_ipc_injector(injector);
    }
}

/// Engine 부트스트랩 — gui 가 첫 MainView 생성 시 하는 일의 headless 등가.
#[cfg(not(feature = "gui"))]
fn bootstrap_engine(
    app: &mut crate::app::App,
    boot_settings: &crate::settings::Settings,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
) -> anyhow::Result<crate::core::CoreState> {
    let factory = waker.waker_factory();
    let base_waker = factory.make_default_waker();
    // gui 의 `begin_boot` 과 같은 부팅 1 회 훅 — 레거시 `layout.json` 마이그레이션 +
    // 전 슬롯 union scrollback GC. `new_with_ids` 가 슬롯을 읽기 전이어야 한다.
    crate::core::layout_persistence::migrate_and_gc_on_boot(boot_settings.general.restore_layout);
    let mut engine =
        // 슬롯 `None` — headless 는 레이아웃 복원을 적용하지 않으므로(위 "0-C")
        // 어떤 슬롯도 점유하지 않고 로드·저장 모두 하지 않는다.
        crate::core::CoreState::new_with_ids(80, 24, base_waker, None, None, app.core.memory_arc())?;
    engine.waker_factory = Some(factory);
    // agent task runner 재시작 정화(결정 2) — 자동 시작은 하지 않는다(결정 1).
    // CoreState 확보 직후, 어떤 client 도 아직 붙기 전에 1 회만 수행.
    app.core.purge_stale_agent_state_on_boot(&engine);
    app.core.inject_agent_runner_registry(&engine);
    // attach/detach 단계 3: force-detach 통지가 stream client 로 push 되도록 IPC
    // 서버와 동일한 StreamHub 를 attach registry 에 주입.
    engine.attach.set_notifier(app.stream_hub.clone());
    Ok(engine)
}

/// 한 바퀴의 대기 결과.
#[cfg(not(feature = "gui"))]
enum Wait {
    /// 이벤트가 도착했다.
    Event(crate::AppEvent),
    /// 타이머 데드라인에 도달했다 — 이번 바퀴는 타이머만 돌린다.
    Deadline,
    /// 송신단이 전부 사라졌다 — 루프를 끝낸다.
    Disconnected,
}

/// 데드라인 인지 수신 — gui 의 `about_to_wait` 와 대칭이다. gui 는 waker 스레드가
/// 이벤트 루프를 깨우지만, headless 는 메인 루프가 직접 `recv_timeout` 으로 허브
/// 데드라인을 지키므로 wake 신호를 위한 ticker 스레드가 아예 필요 없다.
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
        // 등록된 타이머가 없다 — 깨울 이유가 없으므로 무기한 블로킹.
        None => match rx.recv() {
            Ok(ev) => Wait::Event(ev),
            Err(_) => Wait::Disconnected,
        },
    }
}

/// 도착한 이벤트 하나를 처리한다. `Break` 면 메인 루프를 끝낸다.
#[cfg(not(feature = "gui"))]
fn dispatch_headless_event(
    app: &mut crate::app::App,
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
    waker: &crate::adapters::production::headless_waker::HeadlessWaker,
    event: crate::AppEvent,
) -> std::ops::ControlFlow<()> {
    use crate::AppEvent;
    match event {
        AppEvent::Shutdown | AppEvent::QuitRequested => return std::ops::ControlFlow::Break(()),
        AppEvent::TerminalOutput(id) => handle_terminal_output(app, state, engine, id),
        // pump 가 `system.shutdown` 을 받으면 break 를 돌려준다 — 데몬을 멈추는
        // 유일한 IPC 경로다(gui 의 winit proxy 에 대응).
        AppEvent::IpcReady => {
            // 채널에는 `IpcReady` 가 하나만 선다(`HeadlessWaker` 의 게이트). 회차를 열기
            // 전에 풀어, 회차 도중 든 명령이 다음 것을 세우게 한다.
            waker.note_ipc_drained();
            let flow = headless_dispatch::pump_ipc(app, state, engine);
            // 회차가 예산에서 멈춰 명령이 남았으면 다시 깨운다 — 그 명령들의 wake 는 게이트에
            // 막혀 사라졌다. 채널 꼬리에 붙으므로 그 사이의 PTY·plugin wake 가 먼저 돈다.
            rewake_if_left(flow, || ipc_commands_left(&app.core), || waker.wake_ipc());
            return flow;
        }
        AppEvent::StreamReady => headless_stream::handle_stream_ready(app, state, engine),
    }
    std::ops::ControlFlow::Continue(())
}

/// 회차가 명령을 남기고 끝났으면(`Break` 가 아니고 큐가 안 비었으면) 한 번 다시 깨운다.
/// 큐가 빈 회차는 깨우지 않으므로 헛도는 루프가 없다. 판정을 떼어 `App` 없이 시험한다.
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

/// 명령 큐에 아직 안 꺼낸 명령이 있는가 — 입장 장부의 지금 값으로 답한다(명령을 꺼낼 때
/// 장부에서 빠진다). 장부가 없으면(IPC 서버가 안 뜬 조립) `IpcReady` 자체가 안 온다.
#[cfg(not(feature = "gui"))]
fn ipc_commands_left(core: &crate::core::Core) -> bool {
    core.host_ipc_injector
        .get()
        .and_then(|injector| injector.admission())
        .is_some_and(|ledger| ledger.snapshot().queued_commands > 0)
}

/// Headless 부트. winit / wgpu / egui 가 없는 빌드 (`--no-default-features`) 전용.
///
/// 시퀀스:
/// 1. `mpsc::channel::<AppEvent>` 생성 + `HeadlessWaker` 로 IPC/PTY waker 발급
/// 2. Settings/Memory store 초기화 (gui 와 동일 정책)
/// 3. `App::new_headless` 로 Core+Hub+plugin_manager 초기화
/// 4. `hub.start_ipc(ipc_waker, stream_ctx, connections)` — accept 스레드 분리
///    (+ 스트림 승격 경로). 셋째 인자는 `Core` 가 들고 있는 연결 자리 게이지의 핸들이다
/// 5. 데드라인 인지 수신 loop — 중앙 타이머 허브의 `next_deadline()` 까지만
///    `recv_timeout` 으로 기다리고, 매 바퀴 due 한 타이머 키를 실행한다.
///    Shutdown / QuitRequested 수신 시 break (`docs/dev-guide/timer-hub.md`)
#[cfg(not(feature = "gui"))]
fn run_headless(cli: cli::Cli) -> anyhow::Result<()> {
    use std::sync::mpsc;

    use crate::adapters::production::headless_waker::HeadlessWaker;
    use crate::app::App;

    locale::init();

    let (tx, rx) = mpsc::channel::<crate::AppEvent>();
    let waker = HeadlessWaker::new(tx);

    let boot_settings = crate::settings::Settings::load();
    let memory_arc = boot_memory(&boot_settings);

    let mut app = App::new_headless(cli.port_file, memory_arc)?;
    start_ipc_and_seed(&mut app, &waker);

    let mut engine = bootstrap_engine(&mut app, &boot_settings, &waker)?;
    let preset_store = app.core.preset_store.clone();
    let memory = app.core.memory_arc();
    let mut state = crate::state::AppState::new(&mut engine, preset_store, memory);

    hooks::lua::fire(
        app.lua_engine.as_ref(),
        hooks::lua::AutofireCtx {
            scripts: &boot_settings.scripts,
            guard: &mut app.lua_autofire,
        },
        "tasty.startup.post",
        &serde_json::Value::Null,
    );

    // 번들 plugin 을 **설치**한다 — 띄우지는 않는다. gui 가 창을 만들 때
    // `install_builtins_if_needed` 를 부르는 것과 같은 자리이며, 헤드리스에만 이
    // 호출이 없으면 갓 만든 홈은 package 0 인 채로 남는다(`plugin.list` 가 0 을
    // 답하고 어떤 plugin 메서드도 소속이 안 잡힌다).
    //
    // 예전에는 이 설치가 "호스트가 모르는 이름을 처음 부를 때" 딸려 왔다 — 즉
    // **오타 하나가 plugin 을 설치·기동**했다. 소속 판정을 매니페스트로 옮기면서
    // (ADR-0173) 그 우연한 트리거가 사라졌으므로, 설치는 제 자리인 부팅으로 온다.
    // 기동은 여전히 지연이다: 여기서 프로세스는 하나도 안 뜬다.
    headless_plugins::ensure_plugin_manager_metadata(&mut app, &engine);
    if let Some(mgr) = app.plugin_manager.as_mut() {
        crate::plugin::install_builtins_if_needed(mgr);
        // 소유 표를 해소하는 crate 에 **같은 표**를 넘긴다 — 사본이 아니다.
        // 이 설치가 없으면 `method_meta` 는 어떤 plugin prefix 도 모르는 상태로 남고,
        // 그러면 plugin namespace 메서드가 권한 검사에서 "모르는 메서드" 가 된다.
        // 설치 자체는 1 회이고, 표의 내용은 그 뒤 `refresh_packages` 가 채운다.
        mgr.install_namespace_table_once();
        mgr.refresh_packages();
    }

    tracing::info!("headless daemon ready; PTY pump + IPC dispatch active");

    loop {
        // 블로킹 대기에 들어가기 전에 Intent 큐를 비운다. 정상 경로에서는 발화 지점
        // (IPC / plugin 호출)이 이미 응답 전에 drain 하므로 여기서는 비어 있지만,
        // 앞으로 다른 발화점이 생겨도 큐가 프로세스 수명 동안 쌓이지 않게 하는
        // 최종 방어선이다 — `docs/adr/0111-headless-drains-the-intent-queue.md`.
        crate::intent::headless::drain_pending_intents(&mut app.core, &mut state, &mut engine);
        crate::intent::headless::drain_pending_host_events(&app.core, &mut state, &engine);
        // agent 사건 큐도 같은 자리에서 비운다. 이것이 없으면 `events.fetch` 는
        // 이 조합에서도 정상 응답하는데 링이 영영 비어 있다 — 오류가 아니라
        // **조용한 빈 답**이라 소비자가 구분하지 못한다. 꺼내고 내보내는 규칙은
        // gui 의 `about_to_wait` 드레인과 **같은 함수**를 쓴다
        // (`crate::app::agent_events`) — 두 벌로 두면 한쪽만 고쳐진다.
        //
        // 자리는 대기 **직전**이다. `events.fetch` 요청이 도착해 루프를 깨우면 다음
        // 바퀴가 그때까지의 사건을 발행하고 대기 중인 조회가 조건 변수로 깨어난다.
        // 사건이 그 뒤에 생기고 다른 입력이 하나도 없으면, 이 조합에는 큐가 루프를
        // 깨우는 경로가 없으므로 발행은 그 조회의 `wait_ms` 가 만료돼 소비자가 다시
        // 물을 때까지 밀린다 — 폴링 한 주기의 지연이고 사건은 안 사라진다(위치가
        // 그대로 남는다).
        {
            let mut agent_events = Vec::new();
            let mut dropped = 0u64;
            crate::app::agent_events::take_from(
                &engine.agent_event_queue,
                &mut agent_events,
                &mut dropped,
            );
            crate::app::agent_events::emit(app.plugin_manager.as_mut(), agent_events, dropped);
        }
        // plugin manager 는 자기 허브를 따로 소유한다 — 대기 계산은 min 으로 합성.
        let deadline = crate::app::timers::min_deadline(
            app.timers.next_deadline(),
            app.plugin_manager.as_ref().and_then(|m| m.next_deadline()),
        );
        let pending = match wait_for_event(&rx, deadline) {
            Wait::Event(ev) => Some(ev),
            Wait::Deadline => None,
            Wait::Disconnected => break,
        };

        // 시간축 — due 한 타이머 키 실행. gui `about_to_wait` 의 drain 블록과 동형.
        run_due_timers(&mut app, &mut state, &mut engine);

        let Some(event) = pending else {
            continue;
        };
        if dispatch_headless_event(&mut app, &mut state, &mut engine, &waker, event).is_break() {
            break;
        }
    }
    Ok(())
}

#[cfg(all(test, not(feature = "gui")))]
mod tests {
    use super::rewake_if_left;
    use std::ops::ControlFlow;

    /// 예산에서 멈춰 명령이 남은 회차는 루프를 다시 깨운다 — 안 깨우면 남은 명령은 다른
    /// 입력이 올 때까지 선다(그 명령들의 wake 는 게이트에 접혀 사라졌다).
    #[test]
    fn a_round_that_left_commands_wakes_the_loop_again() {
        let mut woke = 0;
        rewake_if_left(ControlFlow::Continue(()), || true, || woke += 1);
        assert_eq!(woke, 1);
    }

    /// 큐가 빈 회차와 데몬을 멈추는 회차는 깨우지 않는다 — 앞은 헛도는 루프, 뒤는 멈추는
    /// 데몬에 대한 헛 wake 다.
    #[test]
    fn an_empty_or_final_round_does_not_wake_the_loop() {
        let mut woke = 0;
        rewake_if_left(ControlFlow::Continue(()), || false, || woke += 1);
        rewake_if_left(ControlFlow::Break(()), || true, || woke += 1);
        assert_eq!(woke, 0);
    }
}
