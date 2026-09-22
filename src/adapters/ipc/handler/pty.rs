//! `pty.*` IPC 핸들러 — headless PTY primitive 의 IPC/CLI 표면
//! (`docs/adr/0050-headless-pty-primitive.md`, `docs/features/headless-pty/index.md`).
//!
//! 에이전트가 **Surface(Tab) 없이** 백그라운드에서 굴리는 1 회성 PTY 를 spawn/write/
//! read/wait/kill/list 한다. 상위 `child_terminal`(`terminal.*`) 은 자식 터미널
//! *surface* 를 만들어 GUI 에 노출하지만, 여기 PTY 는 Surface 트리를 전혀 건드리지
//! 않는다 — 렌더되지 않고, 포커스/닫은-항목 히스토리/선택에 닿지 않는다(identity.md
//! 원칙 1). 좀비 누적은 [`PtyRegistry`](crate::core::pty_registry) 의 동시 개수 상한
//! + idle TTL 로 막는다.
//!
//! **두 store 정합**: 메타데이터·exit-code cell 은 `engine.pty_registry`, 실제 headless
//! `Terminal` 은 `engine.terminals`(`TerminalStore`)에 **같은 pty id** 로 보관한다(pty id
//! 는 [`PTY_ID_BASE`](crate::core::pty_registry::PTY_ID_BASE) 이상 disjoint 범위에서
//! 발급되어 surface id 와 충돌하지 않는다). 어느 한 쪽만 지우면 누수/좀비가
//! 되므로 kill/sweep 은 **항상 두 store 를 함께** 정리한다.

use super::params::{self, p_try};
use std::time::Instant;

use serde_json::{Value, json};

use super::surface::query::{ScreenDiag, with_screen_diagnostics};

use crate::core::CoreState;
use crate::core::pty_registry::{PtySpawnError, PtySpawnSpec};
use crate::ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

// ───── 파라미터 헬퍼 ─────

use super::params::require_u32;

fn require_str(params: &Value, key: &str, id: &Value) -> Result<String, JsonRpcResponse> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), format!("missing '{key}'")))
}

fn optional_str(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(String::from)
}

/// `command` 파라미터(문자열 배열)를 토큰 벡터로. 미지정/빈 배열이면 빈 vec — bare shell.
fn parse_command(params: &Value) -> Vec<String> {
    params
        .get("command")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// `pty.spawn`/`pty.list` 접근 시점의 lazy sweep — `reconcile_with_live_surfaces`
/// (child_terminal) 와 동형의 "접근 시 self-heal" 패턴.
///
/// **주기 타이머(`Tick::PtySweep`)가 생긴 뒤에도 이 lazy 경로는 남는다.** `pty.spawn`
/// **직전에** 도는 덕분에 동시 개수 상한(기본 8) 판정이 정확해지기 때문이다 — 죽은
/// 항목을 먼저 치우고 나서 상한을 본다. 주기 타이머로 *대체*하면 "실제로는 idle 인
/// PTY 때문에 spawn 이 상한 초과로 실패" 하는 회귀가 생긴다. 두 경로가 같은
/// [`CoreState::sweep_idle_ptys`] 를 부르므로 idempotent 하고 후처리도 동일하다
/// (`docs/adr/0050-headless-pty-primitive.md`).
fn lazy_sweep(engine: &mut CoreState) {
    // 반환 id 는 여기서 쓰지 않는다 — 회수 후처리는 공용 함수가 이미 끝냈다.
    let _ = engine.sweep_idle_ptys(Instant::now());
}

// ───── 핸들러 ─────

/// `pty.spawn` — headless PTY 를 띄우고 pty id 를 반환한다. 상한 초과 시
/// [`PtySpawnError::LimitReached`] 를 IPC 에러로 변환한다(panic 하지 않음).
pub(crate) fn handle_spawn(
    core: &mut crate::core::Core,
    engine: &mut CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    lazy_sweep(engine);

    let command = parse_command(params);
    let cwd = optional_str(params, "cwd");
    let owner_agent_id = caller.agent_id().as_str().to_string();

    // 1) registry 등록(상한 게이트). 실패하면 아무 자원도 만들지 않고 즉시 반환.
    // Clock port 경유 — outbound port 실제 소비 경로.
    let now = core.now_instant();
    let pty_id = match engine.pty_registry.register(
        PtySpawnSpec {
            owner_agent_id: owner_agent_id.clone(),
            cwd: cwd.clone(),
            command: command.clone(),
        },
        now,
    ) {
        Ok(pid) => pid,
        Err(e @ PtySpawnError::LimitReached { .. }) => {
            return JsonRpcResponse::error(id, -32000, e.to_string());
        }
    };

    // 2) 실제 headless Terminal 생성(Surface/트리 삽입 없이 PTY 셸만). Surface 터미널
    //    spawn 경로(mod.rs apply_split_pane)와 동일한 ShellConfig/waker 배선을 쓰되,
    //    command 가 주어지면 initial_input 으로 즉시 실행한다(terminal.spawn 이 tab
    //    생성 후 command 를 send 하는 것과 동형 — 여기서는 첫 stdin 바이트로 주입).
    let cols = engine.default_cols;
    let rows = engine.default_rows;
    let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
    let waker = engine.make_waker(pty_id);
    let working_dir = cwd.as_deref().map(std::path::Path::new);
    let initial_input = if command.is_empty() {
        None
    } else {
        Some(format!("{}\n", command.join(" ")))
    };
    let terminal = match tasty_terminal::Terminal::new(
        tasty_terminal::TerminalConfig {
            cols,
            rows,
            shell: sh.shell_ref(),
            args: &sh.args_ref(),
            extra_env: &sh.envs_ref(),
            surface_id: pty_id,
            working_dir,
            initial_input: initial_input.as_deref(),
        },
        waker,
    ) {
        Ok(t) => t,
        Err(e) => {
            // 롤백: registry 항목을 되돌려 상한 슬롯을 회복한다.
            engine.pty_registry.remove(pty_id);
            return JsonRpcResponse::internal_error(id, format!("pty spawn failed: {e}"));
        }
    };
    engine.terminals.insert(pty_id, terminal);

    // 3) exit-code watcher 배선: waitable child 를 Terminal 에서 넘겨받아
    //    `child.wait()` 로 진짜 종료코드를 잡는다(18-a `attach_exit_watcher`).
    //    take_child 이후 Terminal 의 자체 exit 감지·Drop kill 은 이 child 에 적용되지
    //    않으므로, kill 은 Terminal drop 의 PTY master close(SIGHUP)로 처리한다.
    if let Some(mut child) = engine
        .terminals
        .get_mut(pty_id)
        .and_then(|t| t.take_child())
    {
        engine
            .pty_registry
            .attach_exit_watcher(pty_id, move || match child.wait() {
                Ok(status) => crate::core::pty_registry::PtyExit::from_status(
                    Some(status.exit_code() as i32),
                    status.success(),
                ),
                Err(e) => {
                    tracing::warn!("headless pty {pty_id} child.wait failed: {e}");
                    crate::core::pty_registry::PtyExit::from_status(None, false)
                }
            });
    }

    JsonRpcResponse::success(
        id,
        json!({
            "pty_id": pty_id,
            "owner_agent_id": owner_agent_id,
            "command": command,
            "cwd": cwd,
        }),
    )
}

/// `pty.write` — 실행 중 PTY 에 입력(stdin)을 그대로 보낸다(as-is, 자동 제출 없음 —
/// 호출자가 개행/`\r` 포함). idle 타이머 리셋.
pub(crate) fn handle_write(engine: &mut CoreState, id: Value, params: &Value) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let text = match require_str(params, "text", &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if !engine.pty_registry.contains(pty_id) {
        return JsonRpcResponse::invalid_params(id, format!("headless pty {pty_id} not found"));
    }
    let Some(terminal) = engine.terminals.get_mut(pty_id) else {
        return JsonRpcResponse::internal_error(
            id,
            format!("headless pty {pty_id} registry/store desync (terminal missing)"),
        );
    };
    terminal.send_bytes(text.as_bytes());
    engine.pty_registry.touch(pty_id, Instant::now());
    JsonRpcResponse::success(id, json!({ "id": pty_id, "written": text.len() }))
}

/// `pty.read` — PTY 의 현재 화면 텍스트를 읽는다(optional `lines`=**마지막 N 줄** —
/// 하단 공백 행은 건너뛰고 모자라면 스크롤백에서 채운다).
/// `surface.screen_text` 와 동일 추출 경로. idle 타이머 리셋.
/// `show_dim`(기본 false): dim(ghost-suggestion, 예: Claude Code 자동완성 제안) 셀을
/// 결과에 포함할지 — 기본은 제외해 실제 입력된 텍스트만 반환한다.
pub(crate) fn handle_read(engine: &mut CoreState, id: Value, params: &Value) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !engine.pty_registry.contains(pty_id) {
        return JsonRpcResponse::invalid_params(id, format!("headless pty {pty_id} not found"));
    }
    let lines = p_try!(params::opt_int::<usize>(params, "lines", &id));
    let show_dim = params
        .get("show_dim")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // `surface.screen_text` 와 **같은 추출**이므로 진단 필드도 같이 낸다. 한쪽 문에만
    // 달면 같은 질문("왜 N 보다 적게 왔나")이 어느 문으로 물었느냐에 따라 답이 있기도
    // 없기도 하다.
    let found = engine.find_terminal_by_id(pty_id);
    let (text, diag) = match found {
        Some(t) => (
            match lines {
                Some(n) => t.screen_text_lines(n, show_dim),
                None => t.screen_text(show_dim),
            },
            Some(ScreenDiag {
                scrollback_len: t.scrollback_len(),
                alt_screen: t.is_alternate_screen(),
            }),
        ),
        None => (String::new(), None),
    };
    engine.pty_registry.touch(pty_id, Instant::now());
    JsonRpcResponse::success(
        id,
        with_screen_diagnostics(json!({ "id": pty_id, "text": text }), diag),
    )
}

/// `pty.wait` — 폴링(즉시 반환, blocking 아님). exit-watcher 가 채운 exit cell 을
/// 조회해 종료 여부·exit code 를 반환한다. Surface 라이브 트리가 아니라
/// `PtyEntry::exit()` 로 판정한다(headless 라 Surface 자체가 없음). 활발히 폴링 중인
/// PTY 가 idle-sweep 에 회수되지 않도록 idle 타이머를 리셋한다.
pub(crate) fn handle_wait(engine: &mut CoreState, id: Value, params: &Value) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !engine.pty_registry.contains(pty_id) {
        return JsonRpcResponse::invalid_params(id, format!("headless pty {pty_id} not found"));
    }
    engine.pty_registry.touch(pty_id, Instant::now());
    let entry = engine
        .pty_registry
        .get(pty_id)
        .expect("contains() checked above");
    match entry.exit() {
        Some(exit) => JsonRpcResponse::success(
            id,
            json!({
                "id": pty_id,
                "exited": true,
                "exit_code": exit.code,
                "success": exit.success,
            }),
        ),
        None => JsonRpcResponse::success(id, json!({ "id": pty_id, "exited": false })),
    }
}

/// `pty.kill` — 프로세스를 종료하고 두 store 에서 회수한다. Surface 를 닫는 게 아니라
/// (headless 라 Surface 가 없다) PTY 프로세스만 종료한다: Terminal 을 store 에서
/// 제거하면 drop 시 PTY master 가 닫히며 자식에 SIGHUP 이 가고, exit-watcher 스레드가
/// `child.wait()` 로 reap 한다.
pub(crate) fn handle_kill(engine: &mut CoreState, id: Value, params: &Value) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let had_entry = engine.pty_registry.remove(pty_id).is_some();
    // Terminal drop → PTY master close → 자식 SIGHUP. watcher 가 reap.
    let had_terminal = engine.terminals.remove(pty_id).is_some();
    if !had_entry && !had_terminal {
        return JsonRpcResponse::invalid_params(id, format!("headless pty {pty_id} not found"));
    }
    // waker dedup 게이트 제거(pty_id 키) — 미제거 시 kill 마다 게이트 영구 누적(누수).
    if let Some(factory) = engine.waker_factory.as_ref() {
        factory.forget_surface(pty_id);
    }
    JsonRpcResponse::success(id, json!({ "id": pty_id, "killed": true }))
}

/// `pty.attach_surface` — headless PTY 를 실제 Surface(Tab) 로 **승격(adopt)** 한다.
/// `pane_id`(어느 Pane 에 붙일지) + `id`(pty id) 를 받아 `AdoptTerminal` intent 로
/// 실행한다. spawn/write/read/wait/kill/list 와 달리 이건 진짜 Surface 를 새로
/// 만들므로(권한 `[SurfaceWrite, TerminalSpawn]`) `tab.create` 와 동일한
/// `cascade_tab_created`(tab.created/surface.created host event)를 발화해야 GUI 가
/// 그 Tab 을 렌더한다. 승격 후 그 pty id 는 registry 에서 빠져 `pty.list` 에 더 이상
/// 나타나지 않는다(같은 Terminal 인스턴스가 surface_id 키로 옮겨짐 — 상태 보존).
pub(crate) fn handle_attach_surface(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut CoreState,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let pane_id = match require_u32(params, "pane_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };

    // 검증: 대상 headless PTY 가 살아있고 pane 이 존재해야 한다(깔끔한 에러 메시지).
    match engine.pty_registry.get(pty_id) {
        None => {
            return JsonRpcResponse::invalid_params(id, format!("headless pty {pty_id} not found"));
        }
        Some(entry) if entry.has_exited() => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("headless pty {pty_id} already exited"),
            );
        }
        Some(_) => {}
    }
    if engine.find_pane_by_id(pane_id).is_none() {
        return JsonRpcResponse::invalid_params(id, format!("pane {pane_id} not found"));
    }

    let intent = crate::core::intent::DomainIntent::AdoptTerminal { pane_id, pty_id };
    let events = match core.apply(engine, intent) {
        Ok(events) => events,
        Err(e) => return super::structural_apply_error(id, &e),
    };

    let Some(crate::core::intent::CoreEvent::TabCreated {
        pane_id,
        tab_id,
        surface_id,
        ..
    }) = events.into_iter().next()
    else {
        return JsonRpcResponse::internal_error(id, "Core::apply returned no TabCreated event");
    };

    // tab.create 와 동형 cascade — tab.created/surface.created host event enqueue +
    // polling baseline 동기화. 이 호출을 빠뜨리면 데이터만 옮겨지고 화면엔 안 뜬다.
    crate::core::structural_cascade::cascade_tab_created(
        window, engine, pane_id, tab_id, surface_id,
    );

    JsonRpcResponse::success(
        id,
        json!({
            "pane_id": pane_id,
            "tab_id": tab_id,
            "surface_id": surface_id,
        }),
    )
}

/// `pty.list` — 살아있는 headless PTY 전체 목록. **포커스 독립성**: 필터 없이 전 목록을
/// 무조건 반환한다. 접근 시점에 idle TTL 을 lazy sweep 한다.
/// 등록된 headless PTY 목록. 각 항목이 노출하는 것은 **소비자가 있는 값**뿐이다.
///
/// ★ `PtyEntry::watch_phase`([`crate::core::pty_registry::WatchPhase`])는 여기 **일부러**
/// 안 싣는다. 읽는 쪽이 `#[cfg(test)]` 뿐이라 다음 사람이 죽은 코드로 읽기 쉬운데, 죽은
/// 코드가 아니라 **아직 소비자를 안 만든 것**이다. 판단은 이렇다 — 그 값을 응답에 실으면
/// IPC 계약이 한 칸 늘고, 계약은 한 번 늘면 되돌리기가 비싸다. 지금 그 값이 답하는 물음은
/// "대기가 상한을 다 썼을 때 무엇을 봐야 하나" 하나뿐이고, 그 물음은 실패문이 자기 자리에서
/// 답한다(`exit_wait_failure` → `observed_pty_state`). 즉 **밖으로 내보낼 이유가 아직 없다.**
///
/// 내보낼 이유가 생기는 자리는 하나다: 진단이 아니라 **운영**이 그 값을 물을 때 — 예컨대
/// 에이전트가 "이 PTY 가 왜 안 끝나나" 를 프로세스 밖에서 판정해야 할 때. 그때 이 주석을
/// 지우고 필드를 더하면서 `watch_phase` 의 `#[cfg(test)]` 게이트를 함께 벗긴다(같은 파일의
/// `PtyRegistry::wait_for_exit` 가 쓰는 정책과 같다: 소비자가 생길 때 벗긴다).
pub(crate) fn handle_list(engine: &mut CoreState, id: Value) -> JsonRpcResponse {
    lazy_sweep(engine);
    let ptys: Vec<Value> = engine
        .pty_registry
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "owner_agent_id": e.owner_agent_id,
                "cwd": e.cwd,
                "command": e.command,
                "has_exited": e.has_exited(),
                "exit_code": e.exit().and_then(|x| x.code),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "ptys": ptys }))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::core::pty_registry::WatchPhase;

    fn engine() -> CoreState {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        CoreState::new(80, 24, waker).expect("engine")
    }

    /// `handle_spawn` 이 소비하는 `Core::now_instant`(Clock port) 용 최소
    /// fixture. `TempDir` 은 호출자가 명명된 binding 으로 받아 즉시 drop 되지 않게 한다.
    fn core() -> (crate::core::Core, tempfile::TempDir) {
        use std::sync::{Arc, Mutex};

        use tasty_memory::MemoryStorage;
        use tasty_themes::{ThemeStorage, ThemeStore};

        use crate::adapters::test::{
            fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
            mock_process::MockProcessSpawner, tmp_home::TmpHome,
        };
        use crate::core::builder::CoreBuilder;
        use crate::ports::notification_sound::NoopPlayer;

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());
        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let home = TmpHome::new(home_tmp.path().to_path_buf());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(home))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core build");

        (core, home_tmp)
    }

    fn ok(resp: JsonRpcResponse) -> Value {
        resp.result.expect("expected success result")
    }

    /// 종료 대기의 상한. **경주 예산이 아니라 안전망**이다 — 올려서 통과시키지 마라.
    /// 근거(정상 구간과 이 값 사이에 표본이 없다)와 재검토 조건은 ADR-0211.
    ///
    /// **실패 갈래를 재현하는 법**: 이 상수를 잠깐 2 초로 줄이고, 아래 e2e 의 write 를
    /// 종료를 부르지 않는 것(`echo hi` 등)으로 바꿔 그 시험 하나만 돌린다. 예산을 다 쓰고
    /// 죽으며 `exit_wait_failure` 가 watcher 위상과 화면 관측을 찍는다. 확인 뒤 둘 다
    /// 되돌린다 — 이 진단문은 실패 갈래에서만 도므로, 태워 보지 않으면 틀린 채로 남는다
    /// (그 일이 실제로 있었다: 2026-09-06 macOS 빨강에서 이 자리가 반대 결론을 적었다).
    const EXIT_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

    /// `wait_for_exit` 가 `None` 을 낸 **두 사건**을 갈라 문장으로 만든다.
    ///
    /// **왜 가르나.** `PtyRegistry::wait_for_exit` 는 예산을 다 썼을 때도, 그런 id 가
    /// 레지스트리에 아예 없을 때도(`entries.get(&id)?`) 똑같이 `None` 을 낸다. 종전
    /// 문구는 그 둘 모두에 "did not exit within timeout" 이라고 적었다 — 실측으로
    /// 없는 id 는 **1.16 ms** 만에 그 문구를 냈다. 부하 의존 타임아웃으로 읽히는
    /// 빨강이 사실은 부하와 무관한 사건일 수 있고, 그때 상한 인상은 처방이 아니다.
    ///
    /// 가르는 값은 **경과**다. 순수 함수라 아래 단위 테스트가 세 방향을 다 찌른다.
    fn exit_wait_failure(
        pty_id: u32,
        elapsed: std::time::Duration,
        budget: std::time::Duration,
        still_registered: bool,
        observed: &str,
    ) -> String {
        if !still_registered {
            return format!(
                "headless pty {pty_id} 가 레지스트리에 없다 — 기다린 것이 아니라 \
                 대상이 없었다({elapsed:?} 만에 반환). 상한({budget:?}) 인상은 \
                 이 사건의 처방이 아니다"
            );
        }
        if elapsed >= budget {
            // 재현 불가능한 러너(macOS CI)에서 이 갈래가 걸리면 붙어서 못 본다. 그래서
            // 실패 자리가 스스로 갈리게 관측 사실을 함께 싣는다(정상 경로에선 안 꺼낸다).
            return format!(
                "headless pty {pty_id} 가 예산 {budget:?} 안에 종료하지 않았다\
                 (경과 {elapsed:?}). exit-watcher 가 cell 을 못 채웠거나 자식이 \
                 실제로 안 죽었다 — {observed}"
            );
        }
        format!(
            "headless pty {pty_id} 대기가 예산 {budget:?} 을 다 쓰지 않고 끝났다\
             (경과 {elapsed:?}). Condvar 루프는 남은 시간이 0 일 때만 빠져나오므로 \
             이 조합은 일어나면 안 된다 — 상한이 아니라 대기 자체를 봐라"
        )
    }

    /// exit-watcher 의 종료 신호를 기다려 종료 정보를 반환한다 — 고정 간격 폴링이 아니라
    /// `Condvar` 대기라, 러너 부하로 스케줄이 밀려도 종료 즉시 반환한다(ADR-0129 형태 C 근본).
    /// 상한은 신호가 영영 안 올 때만 걸리는 안전망이라 넉넉히 둔다(정상 경로는 수 ms).
    /// `sent` 는 그 pty 로 우리가 보낸 글자 — 실패 갈래에서 화면의 에코를 빼는 데 쓴다.
    fn wait_for_exit(engine: &mut CoreState, pty_id: u32, sent: &str) -> Value {
        let started = std::time::Instant::now();
        match engine.pty_registry.wait_for_exit(pty_id, EXIT_WAIT_BUDGET) {
            Some(exit) => json!({
                "id": pty_id,
                "exited": true,
                "exit_code": exit.code,
                "success": exit.success,
            }),
            None => {
                let elapsed = started.elapsed();
                let still_registered = engine.pty_registry.contains(pty_id);
                // 실패 갈래에서만 관측을 꺼낸다 — Some(exit) 정상 경로는 이 줄에 안 온다.
                let observed = observed_pty_state(engine, pty_id, sent);
                panic!(
                    "{}",
                    exit_wait_failure(
                        pty_id,
                        elapsed,
                        EXIT_WAIT_BUDGET,
                        still_registered,
                        &observed
                    )
                )
            }
        }
    }

    /// exit-watcher 의 위상을 한 구절로. **`exit()` 가 `None` 인 이유**를 가르는 값이다.
    ///
    /// 화면만 보던 때는 이 갈림길이 아예 안 보였다 — `None` 하나가 "자식이 안 죽었다" 와
    /// "우리가 잡을 자리에 못 갔다" 를 같은 얼굴로 냈다. 뒤쪽 둘(`NotAttached`·`Spawned`)은
    /// 자식의 생사와 무관한 우리 쪽 사건이라, 그때 자식을 들여다보면 헛짚는다.
    ///
    /// `Reaped` 의 처방은 한 번 틀렸다가 고쳤다. 처음엔 "대기 쪽을 봐라" 였는데, 그 문장은
    /// **표본 0 인 추측**이었다. 재 보니 그 조합이 나는 경로는 하나뿐이다 — 대기가 상한을 다
    /// 써 마지막 검사를 마친 **뒤에** 자식이 끝나는 것. 대기가 놓치는 경로는 없다: 채우는
    /// 쪽과 깨는 쪽이 같은 락 안이고, 대기는 만료로 깬 뒤에도 cell 을 다시 검사한다 —
    /// **깨우기가 통째로 없어도 안 놓친다**(`crate::core::pty_registry` 의
    /// `a_fill_is_never_lost_even_if_the_wakeup_never_comes` 가 100 회에 유실 0 을 잰다).
    /// 그래서 이 갈래는 대기가 아니라 **늦은 자식**을 가리킨다 —
    /// ADR-0211 이 "느려서 못 잡았다" 가 사실이 되는 경우로 예상한 그것이다.
    fn watch_phase_note(phase: WatchPhase) -> &'static str {
        match phase {
            WatchPhase::NotAttached => {
                "watcher 를 못 걸었다 — cell 은 영영 안 찬다(자식의 생사와 무관한 우리 쪽 사건)"
            }
            WatchPhase::Spawned => {
                "watcher 스레드는 떴는데 wait 에 못 들어갔다 — 자식이 아니라 스케줄을 봐라"
            }
            WatchPhase::Waiting => "watcher 가 wait 에 들어가 아직 안 돌아왔다 — 자식이 안 죽었다",
            WatchPhase::Reaped => {
                "watcher 가 결과를 채웠다 — 자식이 끝나긴 했고 상한을 막 넘겨 늦게 끝났다\
                 (채우는 것과 깨는 것이 같은 락 안이라 대기가 놓친 것이 아니다). \
                 '느려서 못 잡았다' 가 사실이 되는 유일한 갈래다 — 상한을 다시 재라(ADR-0211)"
            }
        }
    }

    /// 화면에 남은 것이 **우리가 보낸 글자의 에코뿐**인지 가른다.
    ///
    /// **왜 이 물음이어야 하나.** PTY 의 에코는 자식이 아니라 라인 디시플린이 낸다 —
    /// 자식이 셸이 아니어도, tty 를 한 글자도 읽지 않아도 master 로 되돌아온다.
    /// 리눅스 실측(2026-09-08): 자식을 `sleep 60` 으로 두고 master 에 `exit 7\r` 을 쓰면
    /// master 에서 `exit 7\r\n` 8 B 가 그대로 되읽힌다. 그래서 "화면에 내용이 있다" 는
    /// **셸이 떴다는 증거가 못 된다.** 에코를 뺀 나머지가 남을 때만 자식이 무엇인가
    /// 뱉었다고 말할 수 있다.
    fn screen_holds_more_than_our_echo(visible: &str, sent: &str) -> bool {
        let visible = visible.trim();
        if visible.is_empty() {
            return false;
        }
        let echo = sent.trim_matches(|c: char| c.is_whitespace());
        if echo.is_empty() {
            // 보낸 것이 없으면 화면의 무엇이든 자식이 낸 것이다.
            return true;
        }
        !visible.replacen(echo, "", 1).trim().is_empty()
    }

    /// 실패 갈래 진단 — pty 화면에서 **관측된 사실**을 한 줄로. 정상 경로에선 호출되지
    /// 않으므로 이 비용은 실패 때만 든다.
    ///
    /// 붙일 후보 셋 중 실제로 꺼낼 수 있는 것만 담는다. registry·terminal 어디도 마스터에서
    /// 읽은 **raw 바이트 수**나 write **누적 바이트**를 들지 않아, ① 의 정확한 바이트 수와
    /// ③(write 바이트)은 못 꺼낸다. 대신 ② 화면 꼬리와, ① 의 근사(에코 밖의 출력이 있는가 +
    /// scrollback 줄 수)를 싣는다 — "자식이 한 글자도 안 뱉었나" 대 "뱉었는데 안 죽나" 를
    /// 이 값으로 가른다. `sent` 는 그 pty 로 우리가 보낸 글자다(에코를 빼는 데 쓴다).
    fn observed_pty_state(engine: &CoreState, pty_id: u32, sent: &str) -> String {
        // 위상이 먼저다 — 화면은 자식이 뭘 했는지의 근사지만, 위상은 우리가 잡을 자리에
        // 갔는지를 직접 말한다. 엔트리가 없으면 그것도 관측이다.
        let watcher = match engine.pty_registry.get(pty_id) {
            Some(e) => watch_phase_note(e.watch_phase()),
            None => "registry 에 엔트리가 없다 — 위상을 못 읽었다",
        };
        let Some(t) = engine.find_terminal_by_id(pty_id) else {
            return format!(
                "관측: [{watcher}] · [terminal 없음 — registry/store desync 라 화면을 못 읽었다]"
            );
        };
        let screen = t.screen_text(false);
        let visible = screen.trim_end();
        let tail: String = visible
            .chars()
            .rev()
            .take(48)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        let shell = if visible.trim().is_empty() {
            "빈 채 — 자식이 한 글자도 안 뱉었고 에코조차 없다(write 가 안 갔거나 exec 미기동)"
        } else if screen_holds_more_than_our_echo(visible, sent) {
            "에코 밖의 출력이 있다 — 자식이 무엇인가 뱉었다(뜬 쪽. 안 죽는 이유를 봐라)"
        } else {
            "우리가 보낸 것의 에코뿐 — 자식이 떴다는 증거가 아니다(에코는 라인 디시플린이 낸다)"
        };
        format!(
            "관측: [{watcher}] · [화면 {shell}] (scrollback {sb} 줄, alt-screen {alt}, \
             보낸 것=\"{sent}\", 꼬리=\"{tail}\")",
            sb = t.scrollback_len(),
            alt = t.is_alternate_screen(),
            sent = sent.escape_default(),
            tail = tail.escape_default(),
        )
    }

    #[test]
    fn the_watcher_phase_points_at_the_child_or_at_us_but_never_at_both() {
        // 이 진단문의 값은 "무엇을 다음에 열어야 하나" 를 한 방향으로 가리키는 것이다.
        // 앞의 둘은 우리 쪽(자식을 들여다보면 헛짚는다), 뒤의 둘은 자식 쪽·대기 쪽이다.
        let ours = [
            watch_phase_note(WatchPhase::NotAttached),
            watch_phase_note(WatchPhase::Spawned),
        ];
        for note in ours {
            assert!(
                !note.contains("자식이 안 죽었다"),
                "우리 쪽 사건인데 자식을 지목했다: {note}"
            );
        }
        assert!(watch_phase_note(WatchPhase::Waiting).contains("자식이 안 죽었다"));

        // `Reaped` 의 처방은 측정으로 한 번 뒤집혔다. 도달 경로가 "늦은 자식" 하나뿐이고
        // 대기가 놓치는 경로는 없다는 것을 `pty_registry` 의 두 시험이 잰다 — 그래서 이
        // 문장은 대기를 지목하면 안 되고, ADR-0211 의 재검토 조건으로 보내야 한다.
        let reaped = watch_phase_note(WatchPhase::Reaped);
        assert!(
            !reaped.contains("대기 쪽"),
            "표본 0 이던 옛 처방(대기 쪽)이 되살아났다: {reaped}"
        );
        assert!(reaped.contains("늦게 끝났다"), "{reaped}");
        assert!(reaped.contains("ADR-0211"), "{reaped}");

        // 네 위상이 서로 다른 문장을 낸다 — 하나로 뭉치면 가르는 값이 아니다.
        let all = [
            watch_phase_note(WatchPhase::NotAttached),
            watch_phase_note(WatchPhase::Spawned),
            watch_phase_note(WatchPhase::Waiting),
            watch_phase_note(WatchPhase::Reaped),
        ];
        let mut seen: Vec<&str> = all.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4, "위상 문장이 겹친다: {all:?}");
    }

    #[test]
    fn the_echo_of_what_we_sent_is_not_evidence_that_the_child_ran() {
        // 픽스처는 이 함수의 상수가 아니라 **2026-09-06 macOS 실패 로그가 실제로 찍은
        // 화면 꼬리**다(run 34020997495, `spawn_with_command_captures_exit_code`).
        // 그때 진단문은 이 화면을 보고 "셸은 떴다" 고 적었다 — 그 문장이 틀렸다.
        assert!(
            !screen_holds_more_than_our_echo("exit 7", "exit 7"),
            "우리가 보낸 것과 같은 화면을 자식의 출력으로 셌다"
        );
        assert!(
            !screen_holds_more_than_our_echo("exit 3", "exit 3\n"),
            "보낸 것의 개행 차이로 에코 판정이 갈렸다"
        );

        // 에코 밖이 있으면 그때는 자식이 뱉은 것이다.
        assert!(screen_holds_more_than_our_echo(
            "user@host ~ % exit 7",
            "exit 7"
        ));
        // 빈 화면은 어느 쪽도 아니다 — 호출자가 먼저 가른다.
        assert!(!screen_holds_more_than_our_echo("   ", "exit 7"));
        // 보낸 것이 없으면 화면의 무엇이든 자식이 낸 것이다.
        assert!(screen_holds_more_than_our_echo("$ ", ""));
    }

    #[test]
    fn the_exit_wait_failure_tells_which_of_the_two_events_happened() {
        let budget = std::time::Duration::from_secs(30);

        // 대상이 없다 — 경과가 짧고, 상한 인상이 처방이 아니라고 적힌다.
        let gone = exit_wait_failure(7, std::time::Duration::from_millis(1), budget, false, "");
        assert!(gone.contains("레지스트리에 없다"), "{gone}");
        assert!(gone.contains("처방이 아니다"), "{gone}");

        // 양방향 — 예산을 다 쓴 쪽은 그 문장을 쓰지 않는다. 그리고 이 갈래는 관측을 담는다
        // (재현 불가 러너에서 스스로 갈리게).
        let spent = exit_wait_failure(7, budget, budget, true, "관측: 화면 빈 채 — X, 꼬리=\"$\"");
        assert!(spent.contains("예산"), "{spent}");
        assert!(
            spent.contains("관측:"),
            "예산 갈래가 관측을 안 담았다: {spent}"
        );
        assert!(
            !spent.contains("레지스트리에 없다"),
            "대상이 있는데 없다고 적었다: {spent}"
        );

        // 세 번째 갈래 — 있는데 예산도 안 썼다. 일어나면 안 되는 조합이라 그렇게 적는다.
        let odd = exit_wait_failure(7, std::time::Duration::from_millis(1), budget, true, "");
        assert!(odd.contains("일어나면 안 된다"), "{odd}");
        assert!(
            !odd.contains("레지스트리에 없다") && !odd.contains("안에 종료하지 않았다"),
            "세 갈래가 안 갈렸다: {odd}"
        );
    }

    #[test]
    fn spawn_write_wait_kill_list_e2e() {
        let mut e = engine();
        let (mut c, _home) = core();
        let caller = CallerContext::Local;

        // spawn: bare shell (command 없음). pty id 는 disjoint 고범위.
        let resp = handle_spawn(&mut c, &mut e, &caller, json!(1), &json!({}));
        let spawned = ok(resp);
        let pty_id = spawned["pty_id"].as_u64().unwrap() as u32;
        assert!(pty_id >= crate::core::pty_registry::PTY_ID_BASE);

        // list: 방금 만든 PTY 가 보인다(필터 없이 전체 반환).
        let listed = ok(handle_list(&mut e, json!(2)));
        let arr = listed["ptys"].as_array().unwrap();
        assert!(arr.iter().any(|p| p["id"].as_u64() == Some(pty_id as u64)));

        // write: 셸에 `exit 3` 을 보내 종료를 유발(명령 실행 경로 검증).
        let w = ok(handle_write(
            &mut e,
            json!(3),
            &json!({ "id": pty_id, "text": "exit 3\n" }),
        ));
        assert_eq!(w["id"].as_u64(), Some(pty_id as u64));

        // wait: 실제 exit code 3 을 exit-watcher 가 잡아야 한다.
        let exited = wait_for_exit(&mut e, pty_id, "exit 3\n");
        assert_eq!(exited["exit_code"].as_i64(), Some(3));
        assert_eq!(exited["success"], Value::Bool(false));

        // kill: 두 store 에서 회수 → list 에 안 보인다.
        let killed = ok(handle_kill(&mut e, json!(4), &json!({ "id": pty_id })));
        assert_eq!(killed["killed"], Value::Bool(true));
        assert!(!e.pty_registry.contains(pty_id));
        assert!(e.find_terminal_by_id(pty_id).is_none());
        let listed2 = ok(handle_list(&mut e, json!(5)));
        assert!(
            listed2["ptys"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["id"].as_u64() != Some(pty_id as u64))
        );
    }

    #[test]
    fn spawn_with_command_captures_exit_code() {
        // command 를 initial_input 으로 즉시 실행하는 경로 + exit-code 캡처 검증.
        let mut e = engine();
        let (mut c, _home) = core();
        let caller = CallerContext::Local;
        let resp = handle_spawn(
            &mut c,
            &mut e,
            &caller,
            json!(1),
            &json!({ "command": ["exit", "7"] }),
        );
        let pty_id = ok(resp)["pty_id"].as_u64().unwrap() as u32;
        let exited = wait_for_exit(&mut e, pty_id, "exit 7");
        assert_eq!(exited["exit_code"].as_i64(), Some(7));
        // 정리: 살아있는 PTY 종료(응답은 확인 불필요).
        handle_kill(&mut e, json!(9), &json!({ "id": pty_id }));
    }

    #[test]
    fn spawn_beyond_limit_returns_error() {
        let mut e = engine();
        let (mut c, _home) = core();
        // 상한을 2 로 낮춰 3 번째 spawn 이 LimitReached 로 실패하는지 확인.
        e.pty_registry = crate::core::pty_registry::PtyRegistry::with_limits(
            2,
            crate::core::pty_registry::DEFAULT_IDLE_TTL,
        );
        let caller = CallerContext::Local;
        let a = handle_spawn(&mut c, &mut e, &caller, json!(1), &json!({}));
        let b = handle_spawn(&mut c, &mut e, &caller, json!(2), &json!({}));
        assert!(a.result.is_some());
        assert!(b.result.is_some());
        let resp = handle_spawn(&mut c, &mut e, &caller, json!(3), &json!({}));
        assert!(
            resp.error.is_some(),
            "3rd spawn must fail with LimitReached"
        );
        let err = resp.error.unwrap();
        assert!(
            err.message.contains("limit"),
            "error should mention limit: {}",
            err.message
        );
        // 정리: 살아있는 두 PTY 종료(응답은 확인 불필요).
        for pid in e.pty_registry.ids() {
            handle_kill(&mut e, json!(0), &json!({ "id": pid }));
        }
    }

    /// 회귀(waker dedup 게이트 누수): `pty.kill` 은 회수하는 pty_id 의 waker 게이트를
    /// 반드시 `forget_surface` 로 정리해야 한다(대상 pty 만 — 다른 pty 게이트는 보존).
    #[test]
    fn kill_forgets_only_target_waker_gate() {
        use crate::adapters::test::mock_waker_factory::RecordingWakerFactory;
        let mut e = engine();
        let (mut c, _home) = core();
        let factory = RecordingWakerFactory::new();
        let shared: crate::waker::SharedWakerFactory = factory.clone();
        e.waker_factory = Some(shared);
        let caller = CallerContext::Local;

        // spawn 2 개 — 각 pty_id 로 targeted 게이트가 생성된다(make_waker → factory).
        let a = ok(handle_spawn(&mut c, &mut e, &caller, json!(1), &json!({})))["pty_id"]
            .as_u64()
            .unwrap() as u32;
        let b = ok(handle_spawn(&mut c, &mut e, &caller, json!(2), &json!({})))["pty_id"]
            .as_u64()
            .unwrap() as u32;
        assert!(
            factory.made().contains(&a) && factory.made().contains(&b),
            "spawn 은 pty_id 별 targeted 게이트를 만든다"
        );

        // kill a → forget_surface(a) 호출, b 게이트는 건드리지 않는다.
        ok(handle_kill(&mut e, json!(3), &json!({ "id": a })));
        assert!(
            factory.forgotten().contains(&a),
            "handle_kill 은 회수하는 pty_id 의 waker 게이트를 정리해야 한다"
        );
        assert!(
            !factory.forgotten().contains(&b),
            "kill 은 대상 pty 의 게이트만 정리(다른 pty 보존)"
        );

        // 정리: 남은 b 종료.
        handle_kill(&mut e, json!(4), &json!({ "id": b }));
    }

    /// 회귀(waker dedup 게이트 누수): idle TTL sweep(`lazy_sweep`)도 회수하는 pty_id 의
    /// waker 게이트를 `forget_surface` 로 정리해야 한다. TTL 0 으로 즉시 만료시킨다.
    #[test]
    fn idle_sweep_forgets_waker_gate() {
        use crate::adapters::test::mock_waker_factory::RecordingWakerFactory;
        let mut e = engine();
        let (mut c, _home) = core();
        // TTL 0: 다음 접근(lazy_sweep) 시 touch 안 된 headless PTY 는 즉시 만료 회수.
        e.pty_registry =
            crate::core::pty_registry::PtyRegistry::with_limits(8, std::time::Duration::ZERO);
        let factory = RecordingWakerFactory::new();
        let shared: crate::waker::SharedWakerFactory = factory.clone();
        e.waker_factory = Some(shared);
        let caller = CallerContext::Local;

        let a = ok(handle_spawn(&mut c, &mut e, &caller, json!(1), &json!({})))["pty_id"]
            .as_u64()
            .unwrap() as u32;
        assert!(factory.made().contains(&a), "spawn 이 게이트를 만든다");

        // handle_list → lazy_sweep → a 가 idle 만료로 회수되며 게이트도 정리된다.
        ok(handle_list(&mut e, json!(2)));
        assert!(
            !e.pty_registry.contains(a),
            "TTL 0 이므로 sweep 이 a 를 회수해야 한다"
        );
        assert!(
            factory.forgotten().contains(&a),
            "lazy_sweep 은 회수하는 pty_id 의 waker 게이트를 정리해야 한다"
        );
    }

    #[test]
    fn write_read_wait_on_unknown_id_errors() {
        let mut e = engine();
        let bogus = crate::core::pty_registry::PTY_ID_BASE + 999;
        assert!(
            handle_write(&mut e, json!(1), &json!({ "id": bogus, "text": "x" }))
                .error
                .is_some()
        );
        assert!(
            handle_read(&mut e, json!(1), &json!({ "id": bogus }))
                .error
                .is_some()
        );
        assert!(
            handle_wait(&mut e, json!(1), &json!({ "id": bogus }))
                .error
                .is_some()
        );
        assert!(
            handle_kill(&mut e, json!(1), &json!({ "id": bogus }))
                .error
                .is_some()
        );
    }

    // ───── 주기 sweep 경로 (ADR-0050 "좀비 회수 시점") ─────

    /// `forget_surface` 호출을 기록하는 waker factory — 회수 시 waker dedup 게이트가
    /// 실제로 해제되는지 관측한다(미해제 시 sweep 마다 게이트 영구 누적 = 누수).
    #[derive(Default)]
    struct RecordingWakerFactory {
        forgotten: std::sync::Mutex<Vec<u32>>,
    }

    impl tasty_terminal::waker_factory::WakerFactory for RecordingWakerFactory {
        fn make_targeted_waker(&self, _surface_id: u32) -> tasty_terminal::Waker {
            std::sync::Arc::new(|| {})
        }
        fn make_default_waker(&self) -> tasty_terminal::Waker {
            std::sync::Arc::new(|| {})
        }
        fn note_drained(&self, _surface_id: Option<u32>) {}
        fn forget_surface(&self, surface_id: u32) {
            self.forgotten
                .lock()
                .expect("forgotten poisoned")
                .push(surface_id);
        }
    }

    /// TTL 을 아주 짧게 주입한 registry. 실시간 5분을 기다리지 않고 만료를 재현한다
    /// (`PtyRegistry::with_limits`).
    fn short_ttl_registry(max: usize) -> crate::core::pty_registry::PtyRegistry {
        crate::core::pty_registry::PtyRegistry::with_limits(max, Duration::from_millis(1))
    }

    /// 주기 경로(`Tick::PtySweep` 실행부)가 부르는 [`CoreState::sweep_idle_ptys`] 가
    /// **lazy 와 동일한 후처리**를 한다 — 세 가지를 한 묶음으로 정리해야 두 store 정합이
    /// 깨지지 않는다(ADR-0050: "어느 한 쪽만 지우면 누수/좀비").
    ///
    /// 두 경로가 같은 함수를 부르므로 후처리가 갈라질 수 없다는 것이 이 구조의 핵심이고,
    /// 이 테스트는 그 함수가 실제로 세 가지를 다 하는지를 고정한다.
    #[test]
    fn periodic_sweep_clears_registry_terminal_store_and_waker_gate() {
        let mut e = engine();
        let (mut c, _home) = core();
        let recorder = std::sync::Arc::new(RecordingWakerFactory::default());
        e.waker_factory = Some(recorder.clone());
        e.pty_registry = short_ttl_registry(8);

        let spawned = ok(handle_spawn(
            &mut c,
            &mut e,
            &CallerContext::Local,
            json!(1),
            &json!({}),
        ));
        let pty_id = spawned["pty_id"].as_u64().expect("pty_id") as u32;
        assert!(e.pty_registry.contains(pty_id), "registry entry");
        assert!(e.terminals.get(pty_id).is_some(), "TerminalStore entry");

        // TTL(1ms) 경과 — 접근(`pty.*`) 없이 주기 경로만 돈다.
        std::thread::sleep(Duration::from_millis(5));
        let reaped = e.sweep_idle_ptys(Instant::now());

        assert_eq!(reaped, vec![pty_id], "회수 id");
        assert!(!e.pty_registry.contains(pty_id), "registry 에서 제거");
        assert!(
            e.terminals.get(pty_id).is_none(),
            "TerminalStore 에서도 제거 — 남으면 자식이 SIGHUP 을 못 받아 좀비가 된다"
        );
        assert_eq!(
            *recorder.forgotten.lock().expect("forgotten poisoned"),
            vec![pty_id],
            "waker dedup 게이트 해제 — 미해제 시 회수마다 게이트가 영구 누적된다"
        );
    }

    /// **회귀 방어** — 주기 타이머가 생겼다고 lazy 경로를 없애면 안 된다.
    ///
    /// `lazy_sweep` 은 `pty.spawn` **직전에** 돌아 동시 개수 상한 판정을 정확하게
    /// 유지한다. 제거하면 "실제로는 idle 이라 곧 회수될 PTY 때문에 spawn 이 상한 초과로
    /// 실패" 하는 회귀가 생긴다 — 주기 타이머는 최대 `interval + slack` 뒤에나 도는데
    /// spawn 은 지금 성공해야 한다.
    #[test]
    fn spawn_still_reclaims_idle_slots_before_checking_the_limit() {
        let mut e = engine();
        let (mut c, _home) = core();
        // 상한 1 — 이미 꽉 찼지만 그 항목은 idle 만료 상태다.
        e.pty_registry = short_ttl_registry(1);
        e.pty_registry
            .register(
                crate::core::pty_registry::PtySpawnSpec {
                    owner_agent_id: "agent-1".into(),
                    cwd: None,
                    command: vec!["sleep".into(), "3600".into()],
                },
                Instant::now(),
            )
            .expect("첫 등록은 상한 안");
        std::thread::sleep(Duration::from_millis(5));

        let resp = handle_spawn(&mut c, &mut e, &CallerContext::Local, json!(1), &json!({}));
        assert!(
            resp.error.is_none(),
            "lazy sweep 이 먼저 돌아 슬롯을 회수해야 한다 (상한 초과 실패 = lazy 가 제거된 것): {:?}",
            resp.error
        );
    }
}
