//! surface 없이 백그라운드 PTY를 관리한다. 화면·포커스·복원 기록은 만들지 않는다.
//! 동시 개수는 제한하지 않는다. 실행 중인 PTY는 호출자가 spawn에서 유휴 제한을 고른 경우에만 회수한다.
//! 메타데이터를 가진 Pty와 Terminal은 같은 PTY ID의 단일 저장소 항목에 둔다.
//! PTY_ID_BASE 이상의 ID를 사용해 surface ID와 구분하고 회수할 때 내용과 물리 owner를 함께 제거한다.

use super::params::{self, p_try};
use crate::runtime::engine_access::EngineMut;
use std::time::Instant;

use serde_json::{Value, json};

use super::surface::query::{ScreenDiag, with_screen_diagnostics};

use crate::ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;

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

/// 주기 정리와 별도로 spawn/list 접근 시점에도 같은 회수를 적용한다.
/// 주기 타이머를 기다리지 않고 기한이 지난 항목을 응답과 목록에서 빼기 위해서다.
fn lazy_sweep(engine: &mut EngineMut<'_>) {
    // 회수 기록과 알림은 공용 함수가 끝낸다.
    engine.reclaim_standalone_ptys(Instant::now());
}

/// 등록되지 않은 ID의 응답. Tasty가 회수한 ID는 처음부터 없던 ID와 구별되는 이유를 돌려준다.
fn unknown_pty(engine: &EngineMut<'_>, id: Value, pty_id: u32) -> JsonRpcResponse {
    let message = match engine.runtime.terminals.reclaim_record(pty_id) {
        Some(reason) => reason.describe(pty_id),
        None => format!("headless pty {pty_id} not found"),
    };
    JsonRpcResponse::invalid_params(id, message)
}

pub(crate) fn handle_spawn(
    core: &mut crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let idle_timeout_secs = p_try!(params::opt_int::<u64>(params, "idle_timeout_secs", &id));
    if idle_timeout_secs == Some(0) {
        return JsonRpcResponse::invalid_params(id, "idle_timeout_secs must be at least 1");
    }
    lazy_sweep(engine);

    let command = parse_command(params);
    let cwd = optional_str(params, "cwd");
    let owner_agent_id = caller.agent_id().as_str().to_string();

    let now = core.now_instant();
    let pty_id = match engine.runtime.terminals.reserve_pty_id() {
        Ok(pid) => pid,
        Err(error) => return JsonRpcResponse::error(id, -32000, error.to_string()),
    };

    // surface를 만들지 않고 셸만 시작한다. command는 최초 stdin 입력으로 보낸다.
    let cols = engine.runtime.default_cols;
    let rows = engine.runtime.default_rows;
    let sh = crate::core::state::ShellConfig::from_settings(&engine.runtime.settings);
    let waker = engine.make_waker(pty_id);
    let working_dir = cwd.as_deref().map(std::path::Path::new);
    let initial_input = if command.is_empty() {
        None
    } else {
        Some(format!("{}\n", command.join(" ")))
    };
    let (terminal, mut pty) = match tasty_terminal::spawn_terminal(
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
            return JsonRpcResponse::internal_error(id, format!("pty spawn failed: {e}"));
        }
    };
    pty.set_standalone(tasty_terminal::StandalonePty {
        owner_agent_id: owner_agent_id.clone(),
        cwd: cwd.clone(),
        command: command.clone(),
        created_at: now,
        last_activity: now,
        idle_timeout: idle_timeout_secs.map(std::time::Duration::from_secs),
        exited_at: None,
    });
    engine.runtime.terminals.insert(pty_id, terminal, Some(pty));

    JsonRpcResponse::success(
        id,
        json!({
            "pty_id": pty_id,
            "owner_agent_id": owner_agent_id,
            "command": command,
            "cwd": cwd,
            "idle_timeout_secs": idle_timeout_secs,
        }),
    )
}

/// `pty.write` — 실행 중 PTY 에 입력(stdin)을 그대로 보낸다(as-is, 자동 제출 없음 —
/// 호출자가 개행/`\r` 포함). idle 타이머 리셋.
pub(crate) fn handle_write(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let text = match require_str(params, "text", &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if !engine.runtime.terminals.is_standalone(pty_id) {
        return unknown_pty(engine, id, pty_id);
    }
    let Some(terminal) = engine.runtime.terminals.get_mut(pty_id) else {
        return JsonRpcResponse::internal_error(
            id,
            format!("headless pty {pty_id} registry/store desync (terminal missing)"),
        );
    };
    terminal.send_bytes(text.as_bytes());
    engine
        .runtime
        .terminals
        .touch_standalone(pty_id, Instant::now());
    JsonRpcResponse::success(id, json!({ "id": pty_id, "written": text.len() }))
}

/// 현재 화면을 읽고 idle 시간을 갱신한다. lines는 하단 빈 줄을 빼고 마지막 N줄을 고르며
/// 부족하면 스크롤백에서 채운다. show_dim은 기본 false로 dim 셀을 제외한다.
pub(crate) fn handle_read(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !engine.runtime.terminals.is_standalone(pty_id) {
        return unknown_pty(engine, id, pty_id);
    }
    let lines = p_try!(params::opt_int::<usize>(params, "lines", &id));
    let show_dim = params
        .get("show_dim")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
                dim_omitted_rows: if show_dim { 0 } else { t.dim_omitted_rows() },
            }),
        ),
        None => (String::new(), None),
    };
    engine
        .runtime
        .terminals
        .touch_standalone(pty_id, Instant::now());
    JsonRpcResponse::success(
        id,
        with_screen_diagnostics(json!({ "id": pty_id, "text": text }), diag),
    )
}

/// 종료 결과의 현재 상태를 즉시 반환한다. 기다리지는 않으며 idle 시간을 갱신한다.
pub(crate) fn handle_wait(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !engine.runtime.terminals.is_standalone(pty_id) {
        return unknown_pty(engine, id, pty_id);
    }
    if let Some(pty) = engine.runtime.terminals.pty_mut(pty_id) {
        pty.observe_exit();
    }
    engine
        .runtime
        .terminals
        .touch_standalone(pty_id, Instant::now());
    let entry = engine
        .runtime
        .terminals
        .standalone(pty_id)
        .expect("contains() checked above");
    match entry.state().exit() {
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

/// 단일 저장소 항목을 제거해 PTY owner를 종료한다. 비동기 reaper가 소유 child를 회수한다.
/// 이 응답이 자식의 종료 완료를 확인한 것은 아니다.
pub(crate) fn handle_kill(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let pty_id = match require_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !engine.runtime.terminals.is_standalone(pty_id) {
        return unknown_pty(engine, id, pty_id);
    }
    engine.runtime.terminals.remove(pty_id);
    // PTY별 waker 기록도 지워 반복 생성·삭제 시 누적되지 않게 한다.
    if let Some(factory) = engine.runtime.waker_factory.as_ref() {
        factory.forget_surface(pty_id);
    }
    JsonRpcResponse::success(id, json!({ "id": pty_id, "killed": true }))
}

/// 회수 정책을 적용한 뒤 등록된 PTY 전체를 반환한다.
/// owner_phase는 시험 실패 진단에만 사용하므로 공개 응답에 포함하지 않는다.
pub(crate) fn handle_list(engine: &mut EngineMut<'_>, id: Value) -> JsonRpcResponse {
    lazy_sweep(engine);
    let ptys: Vec<Value> = engine
        .runtime
        .terminals
        .standalone_iter()
        .map(|(pty_id, pty)| {
            let e = pty.state().standalone().expect("standalone");
            json!({
                "id": pty_id,
                "owner_agent_id": e.owner_agent_id,
                "cwd": e.cwd,
                "command": e.command,
                "has_exited": pty.state().exit().is_some(),
                "exit_code": pty.state().exit().and_then(|x| x.code),
                "idle_timeout_secs": e.idle_timeout.map(|t| t.as_secs()),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "ptys": ptys }))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use super::*;
    use tasty_terminal::PtyPhase;

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    // Clock이 포함된 Core를 준비하고 TempDir을 호출자에게 넘겨 시험 동안 유지한다.
    pub(crate) fn core() -> (crate::app::services::AppServices, tempfile::TempDir) {
        use std::sync::{Arc, Mutex};

        use tasty_memory::MemoryStorage;
        use tasty_themes::{ThemeStorage, ThemeStore};

        use crate::adapters::test::{
            fake_clock::FakeClock, mock_clipboard::MockClipboard, tmp_home::TmpHome,
        };
        use crate::app::services::builder::AppServicesBuilder;
        use crate::ports::notification_sound::NoopPlayer;

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());
        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let home = TmpHome::new(home_tmp.path().to_path_buf());

        let core = AppServicesBuilder::new()
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_home(Arc::new(home))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test AppServices build");

        (core, home_tmp)
    }

    fn ok(resp: JsonRpcResponse) -> Value {
        resp.result.expect("expected success result")
    }

    /// 종료 신호가 오지 않는 시험을 끝낼 대기 상한. 실패를 숨기려고 늘리지 않는다.
    /// 실패 진단은 종료하지 않는 명령을 보내 별도로 확인한다.
    const EXIT_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

    /// 대상 부재, 기한 소진, 예상보다 일찍 끝난 대기를 구분해 진단한다.
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
                 대상이 없었다({elapsed:?} 만에 반환). 상한({budget:?})을 늘려도 \
                 해결되지 않는 대상 부재 오류다"
            );
        }
        if elapsed >= budget {
            return format!(
                "headless pty {pty_id}의 종료 결과를 제한 시간 {budget:?} 안에 받지 못했다\
                 (경과 {elapsed:?}). {observed}"
            );
        }
        format!(
            "headless pty {pty_id} 대기가 제한 시간 {budget:?} 전에 끝났다\
             (경과 {elapsed:?}). 대상이 남아 있고 결과도 없는 상태에서는 \
             일어나면 안 된다. 대기 경로를 확인한다"
        )
    }

    /// Drive the same nonblocking owner poll as the host; no detached waiter owns the child.
    fn wait_for_exit(engine: &mut EngineMut<'_>, pty_id: u32, sent: &str) -> Value {
        let started = Instant::now();
        loop {
            let result = ok(handle_wait(engine, json!(1), &json!({"id": pty_id})));
            if result["exited"] == true {
                return result;
            }
            let elapsed = started.elapsed();
            if elapsed >= EXIT_WAIT_BUDGET {
                panic!(
                    "{}",
                    exit_wait_failure(
                        pty_id,
                        elapsed,
                        EXIT_WAIT_BUDGET,
                        engine.runtime.terminals.is_standalone(pty_id),
                        &observed_pty_state(&engine.as_ref(), pty_id, sent)
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn owner_phase_note(phase: PtyPhase) -> &'static str {
        match phase {
            PtyPhase::Running => "PTY owner가 실행 중이며 종료 결과 기록은 아직 없다",
            PtyPhase::TerminationRequested => {
                "종료가 요청됐지만 신호 전달이나 회수 완료는 아직 확인되지 않았다"
            }
            PtyPhase::Signalled => "종료 신호가 전달됐지만 결과 기록 완료는 아직 확인되지 않았다",
            PtyPhase::Reaped => {
                "종료 결과가 기록됐다. 대기를 마친 뒤 확인한 상태이므로 결과 기록 시각과 스케줄 지연을 함께 확인한다(docs/dev-guide/self-verification.md)"
            }
            PtyPhase::WaitFailed => "wait 실패로 종료/회수 여부가 불명확하다",
        }
    }

    /// 보낸 입력의 에코 외에 텍스트가 있는지 확인한다.
    /// PTY 라인 디시플린도 에코를 만들므로 에코만으로 셸 실행을 확인할 수 없다.
    fn screen_holds_more_than_our_echo(visible: &str, sent: &str) -> bool {
        let visible = visible.trim();
        if visible.is_empty() {
            return false;
        }
        let echo = sent.trim_matches(|c: char| c.is_whitespace());
        if echo.is_empty() {
            return true;
        }
        !visible.replacen(echo, "", 1).trim().is_empty()
    }

    /// 실패 시 owner 상태와 화면 꼬리를 수집한다. raw 읽기/쓰기 바이트 계수는 없다.
    /// 화면과 에코 비교는 관측 보조 자료이며 자식의 생사나 실행 이력을 보장하지 않는다.
    fn observed_pty_state(
        engine: &crate::runtime::engine_access::EngineRef<'_>,
        pty_id: u32,
        sent: &str,
    ) -> String {
        let owner = match engine.runtime.terminals.standalone(pty_id) {
            Some(e) => owner_phase_note(e.state().observation().phase),
            None => "저장소에 standalone 항목이 없어 owner 상태를 읽을 수 없다",
        };
        let Some(t) = engine.find_terminal_by_id(pty_id) else {
            return format!(
                "관측: [{owner}] · [Terminal 없음: 저장소에 내용 항목이 없어 화면을 읽을 수 없다]"
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
            "현재 화면에 텍스트가 없다. 이 값만으로 실행 여부를 판단할 수 없다"
        } else if screen_holds_more_than_our_echo(visible, sent) {
            "에코와 다른 텍스트가 화면에 있다. 종료 상태는 owner와 함께 확인한다"
        } else {
            "보낸 입력의 에코만 있다. 셸 실행 여부는 확인할 수 없다"
        };
        format!(
            "관측: [{owner}] · [화면 {shell}] (scrollback {sb} 줄, alt-screen {alt}, \
             보낸 것=\"{sent}\", 꼬리=\"{tail}\")",
            sb = t.scrollback_len(),
            alt = t.is_alternate_screen(),
            sent = sent.escape_default(),
            tail = tail.escape_default(),
        )
    }

    #[test]
    fn the_owner_phase_points_at_the_child_or_at_us_but_never_at_both() {
        let ours = [
            owner_phase_note(PtyPhase::Running),
            owner_phase_note(PtyPhase::TerminationRequested),
        ];
        for note in ours {
            assert!(
                !note.contains("자식이 안 죽었다"),
                "owner 상태만으로 자식의 생존 여부를 단정했다: {note}"
            );
        }
        assert!(
            owner_phase_note(PtyPhase::Signalled).contains("결과 기록 완료는 아직 확인되지 않았다")
        );

        // Reaped는 결과 기록 완료만 말한다. 자식 종료 시각을 단정하지 않아야 한다.
        let reaped = owner_phase_note(PtyPhase::Reaped);
        assert!(
            !reaped.contains("대기 쪽"),
            "완료 상태에서 결과 미수신 대기를 원인으로 단정했다: {reaped}"
        );
        assert!(reaped.contains("종료 결과가 기록됐다"), "{reaped}");
        assert!(
            reaped.contains("docs/dev-guide/self-verification.md"),
            "{reaped}"
        );

        let all = [
            owner_phase_note(PtyPhase::Running),
            owner_phase_note(PtyPhase::TerminationRequested),
            owner_phase_note(PtyPhase::Signalled),
            owner_phase_note(PtyPhase::Reaped),
        ];
        let mut seen: Vec<&str> = all.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 4, "owner 상태별 진단이 같아졌다: {all:?}");
    }

    #[test]
    fn the_echo_of_what_we_sent_is_not_evidence_that_the_child_ran() {
        // 실제 실패에서 받은 에코만 있는 화면. 셸 실행의 증거로 해석하면 안 된다.
        assert!(
            !screen_holds_more_than_our_echo("exit 7", "exit 7"),
            "우리가 보낸 것과 같은 화면을 자식의 출력으로 셌다"
        );
        assert!(
            !screen_holds_more_than_our_echo("exit 3", "exit 3\n"),
            "보낸 것의 개행 차이로 에코 판정이 갈렸다"
        );

        assert!(screen_holds_more_than_our_echo(
            "user@host ~ % exit 7",
            "exit 7"
        ));
        assert!(!screen_holds_more_than_our_echo("   ", "exit 7"));
        assert!(screen_holds_more_than_our_echo("$ ", ""));
    }

    #[test]
    fn the_exit_wait_failure_tells_which_of_the_two_events_happened() {
        let budget = std::time::Duration::from_secs(30);

        let gone = exit_wait_failure(7, std::time::Duration::from_millis(1), budget, false, "");
        assert!(gone.contains("레지스트리에 없다"), "{gone}");
        assert!(gone.contains("대상 부재"), "{gone}");

        let spent = exit_wait_failure(7, budget, budget, true, "관측: 화면 빈 채 — X, 꼬리=\"$\"");
        assert!(spent.contains("제한 시간"), "{spent}");
        assert!(
            spent.contains("관측:"),
            "제한 시간 초과 진단에 관측 결과가 없다: {spent}"
        );
        assert!(
            !spent.contains("레지스트리에 없다"),
            "대상이 있는데 없다고 적었다: {spent}"
        );

        let odd = exit_wait_failure(7, std::time::Duration::from_millis(1), budget, true, "");
        assert!(odd.contains("일어나면 안 된다"), "{odd}");
        assert!(
            !odd.contains("레지스트리에 없다") && !odd.contains("안에 받지 못했다"),
            "조기 반환 진단이 대상 부재나 시간 초과 진단과 구분되지 않는다: {odd}"
        );
    }

    #[test]
    fn spawn_write_wait_kill_list_e2e() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let caller = CallerContext::local();

        let resp = handle_spawn(&mut c, &mut e, &caller, json!(1), &json!({}));
        let spawned = ok(resp);
        let pty_id = spawned["pty_id"].as_u64().unwrap() as u32;
        assert!(pty_id >= crate::runtime::terminal_store::PTY_ID_BASE);

        let listed = ok(handle_list(&mut e, json!(2)));
        let arr = listed["ptys"].as_array().unwrap();
        assert!(arr.iter().any(|p| p["id"].as_u64() == Some(pty_id as u64)));

        let w = ok(handle_write(
            &mut e,
            json!(3),
            &json!({ "id": pty_id, "text": "exit 3\n" }),
        ));
        assert_eq!(w["id"].as_u64(), Some(pty_id as u64));

        let exited = wait_for_exit(&mut e, pty_id, "exit 3\n");
        assert_eq!(exited["exit_code"].as_i64(), Some(3));
        assert_eq!(exited["success"], Value::Bool(false));

        let killed = ok(handle_kill(&mut e, json!(4), &json!({ "id": pty_id })));
        assert_eq!(killed["killed"], Value::Bool(true));
        assert!(!e.runtime.terminals.is_standalone(pty_id));
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
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let caller = CallerContext::local();
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
        handle_kill(&mut e, json!(9), &json!({ "id": pty_id }));
    }

    #[test]
    fn spawn_beyond_the_former_limit_succeeds() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let caller = CallerContext::local();
        let mut ids = Vec::new();
        for n in 0..12 {
            let resp = handle_spawn(&mut c, &mut e, &caller, json!(n), &json!({}));
            assert!(
                resp.error.is_none(),
                "spawn {} must not be refused by a concurrency limit: {:?}",
                n + 1,
                resp.error
            );
            ids.push(ok(resp)["pty_id"].as_u64().unwrap());
        }
        let listed = ok(handle_list(&mut e, json!(20)));
        let listed: Vec<u64> = listed["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["id"].as_u64())
            .collect();
        assert!(ids.iter().all(|id| listed.contains(id)), "{listed:?}");
        for pid in ids {
            ok(handle_kill(&mut e, json!(0), &json!({ "id": pid })));
        }
    }

    #[test]
    fn exhausted_id_space_still_refuses_spawn() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        e.runtime.terminals = crate::runtime::terminal_store::TerminalStore::new(
            std::sync::Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX)),
        );
        let resp = handle_spawn(
            &mut c,
            &mut e,
            &CallerContext::local(),
            json!(1),
            &json!({}),
        );
        let err = resp.error.expect("ID exhaustion must refuse");
        assert_eq!(err.code, -32000);
        assert_eq!(err.message, "headless PTY ID space exhausted");
    }

    #[test]
    fn kill_forgets_only_target_waker_gate() {
        use crate::adapters::test::mock_waker_factory::RecordingWakerFactory;
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let factory = RecordingWakerFactory::new();
        let shared: crate::waker::SharedWakerFactory = factory.clone();
        e.runtime.waker_factory = Some(shared);
        let caller = CallerContext::local();

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

        ok(handle_kill(&mut e, json!(3), &json!({ "id": a })));
        assert!(
            factory.forgotten().contains(&a),
            "handle_kill 은 회수하는 pty_id 의 waker 게이트를 정리해야 한다"
        );
        assert!(
            !factory.forgotten().contains(&b),
            "kill 은 대상 pty 의 게이트만 정리(다른 pty 보존)"
        );

        handle_kill(&mut e, json!(4), &json!({ "id": b }));
    }

    #[test]
    fn idle_sweep_forgets_waker_gate() {
        use crate::adapters::test::mock_waker_factory::RecordingWakerFactory;
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let factory = RecordingWakerFactory::new();
        let shared: crate::waker::SharedWakerFactory = factory.clone();
        e.runtime.waker_factory = Some(shared);
        let caller = CallerContext::local();

        let a = ok(handle_spawn(
            &mut c,
            &mut e,
            &caller,
            json!(1),
            &json!({ "idle_timeout_secs": 60 }),
        ))["pty_id"]
            .as_u64()
            .unwrap() as u32;
        assert!(factory.made().contains(&a), "spawn 이 게이트를 만든다");
        expire_idle(&mut e, a);

        ok(handle_list(&mut e, json!(2)));
        assert!(
            !e.runtime.terminals.is_standalone(a),
            "유휴 제한이 지났으므로 접근 시 회수가 a 를 회수해야 한다"
        );
        assert!(
            factory.forgotten().contains(&a),
            "lazy_sweep 은 회수하는 pty_id 의 waker 게이트를 정리해야 한다"
        );
    }

    #[test]
    fn write_read_wait_on_unknown_id_errors() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let bogus = crate::runtime::terminal_store::PTY_ID_BASE + 999;
        assert_eq!(
            handle_write(&mut e, json!(1), &json!({ "id": bogus, "text": "x" }))
                .error
                .expect("unknown id")
                .message,
            format!("headless pty {bogus} not found")
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

    /// 고른 유휴 제한을 이미 지난 것으로 만든다. 접근 시 회수는 실제 현재 시각을 쓰기 때문이다.
    fn expire_idle(engine: &mut EngineMut<'_>, pty_id: u32) {
        engine
            .runtime
            .terminals
            .pty_mut(pty_id)
            .and_then(|pty| pty.standalone_mut())
            .expect("standalone entry")
            .idle_timeout = Some(Duration::ZERO);
    }

    fn spawn_with(
        core: &mut crate::app::services::AppServices,
        engine: &mut EngineMut<'_>,
        params: Value,
    ) -> u32 {
        ok(handle_spawn(
            core,
            engine,
            &CallerContext::local(),
            json!(1),
            &params,
        ))["pty_id"]
            .as_u64()
            .expect("standalone ID") as u32
    }

    fn error_message(resp: JsonRpcResponse) -> String {
        resp.error.expect("expected an error").message
    }

    // 주기 정리도 registry·Terminal·waker를 모두 회수해야 한다.
    #[test]
    fn periodic_sweep_clears_registry_terminal_store_and_waker_gate() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let recorder = std::sync::Arc::new(RecordingWakerFactory::default());
        e.runtime.waker_factory = Some(recorder.clone());
        let pty_id = spawn_with(&mut c, &mut e, json!({ "idle_timeout_secs": 1 }));
        assert!(e.runtime.terminals.is_standalone(pty_id), "registry entry");
        assert!(
            e.runtime.terminals.get(pty_id).is_some(),
            "TerminalStore entry"
        );

        let reaped = e.reclaim_standalone_ptys(Instant::now() + Duration::from_secs(2));

        assert_eq!(reaped, 1, "회수 수");
        assert!(
            !e.runtime.terminals.is_standalone(pty_id),
            "registry 에서 제거"
        );
        assert!(
            e.runtime.terminals.get(pty_id).is_none(),
            "TerminalStore에서도 제거해 PTY master를 닫아야 한다"
        );
        assert_eq!(
            *recorder.forgotten.lock().expect("forgotten poisoned"),
            vec![pty_id],
            "waker dedup 게이트 해제 — 미해제 시 회수마다 게이트가 영구 누적된다"
        );
    }

    // spawn 접근도 주기 정리를 기다리지 않고 같은 회수를 적용한다.
    #[test]
    fn spawn_applies_the_same_reclaim_without_waiting_for_the_timer() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let first = spawn_with(&mut c, &mut e, json!({ "idle_timeout_secs": 60 }));
        expire_idle(&mut e, first);

        let resp = handle_spawn(
            &mut c,
            &mut e,
            &CallerContext::local(),
            json!(1),
            &json!({}),
        );
        let second = ok(resp)["pty_id"].as_u64().unwrap() as u32;
        assert!(
            !e.runtime.terminals.is_standalone(first),
            "spawn 접근의 회수가 빠졌다"
        );
        handle_kill(&mut e, json!(2), &json!({ "id": second }));
    }

    #[test]
    fn a_running_pty_without_a_limit_survives_every_reclaim_path() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let quiet = spawn_with(
            &mut c,
            &mut e,
            json!({ "command": ["exec", "sleep", "600"] }),
        );
        let chatty = spawn_with(
            &mut c,
            &mut e,
            json!({ "command": ["while :; do date; sleep 1; done"] }),
        );
        let later = Instant::now() + Duration::from_secs(390);
        assert_eq!(e.reclaim_standalone_ptys(later), 0);
        assert_eq!(
            e.reclaim_standalone_ptys(Instant::now() + Duration::from_secs(86_400)),
            0
        );
        ok(handle_list(&mut e, json!(2)));
        let spawned = ok(handle_spawn(
            &mut c,
            &mut e,
            &CallerContext::local(),
            json!(3),
            &json!({}),
        ))["pty_id"]
            .as_u64()
            .unwrap() as u32;
        for pty_id in [quiet, chatty] {
            assert!(e.runtime.terminals.is_standalone(pty_id));
            let state = ok(handle_wait(&mut e, json!(4), &json!({ "id": pty_id })));
            assert_eq!(state["exited"], Value::Bool(false));
        }
        for pty_id in [quiet, chatty, spawned] {
            ok(handle_kill(&mut e, json!(5), &json!({ "id": pty_id })));
        }
    }

    #[test]
    fn spawn_rejects_a_zero_idle_timeout() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let (mut c, _home) = core();
        let resp = handle_spawn(
            &mut c,
            &mut e,
            &CallerContext::local(),
            json!(1),
            &json!({ "idle_timeout_secs": 0 }),
        );
        assert_eq!(error_message(resp), "idle_timeout_secs must be at least 1");
        assert_eq!(e.runtime.terminals.standalone_iter().count(), 0);
    }

    #[test]
    fn an_idle_timeout_reclaim_warns_and_answers_with_its_reason() {
        let (_state, mut session) = crate::state::tests::test_state();
        let mut e = session.borrow_mut();
        let (mut c, _home) = core();
        let ws = e.workspace_at(0).expect("fixture workspace").id;
        // spawn and list sweep against the real clock from the fake clock's fixed start, so the
        // limit must be out of reach of real time. The reclaim below alone decides the expiry:
        // last_activity is not later than now, so now + limit is at least the limit past it.
        const LIMIT: Duration = Duration::from_secs(3600);
        let limited = spawn_with(
            &mut c,
            &mut e,
            json!({ "idle_timeout_secs": LIMIT.as_secs() }),
        );
        let unlimited = spawn_with(&mut c, &mut e, json!({}));
        let listed = ok(handle_list(&mut e, json!(2)));
        let row = listed["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"].as_u64() == Some(limited as u64))
            .cloned()
            .expect("listed");
        assert_eq!(row["idle_timeout_secs"].as_u64(), Some(LIMIT.as_secs()));

        assert_eq!(e.reclaim_standalone_ptys(Instant::now() + LIMIT), 1);
        assert!(!e.runtime.terminals.is_standalone(limited));
        assert!(e.runtime.terminals.is_standalone(unlimited));

        let notice = e
            .live
            .notifications
            .all()
            .find(|n| n.body.contains(&limited.to_string()))
            .expect("reclaim notification");
        assert_eq!(
            notice.level,
            crate::notification::NotificationLevel::Warning
        );
        assert_eq!(notice.source_workspace, ws);
        assert_eq!(
            notice.title,
            crate::i18n::t("notification.pty_idle_timeout_title")
        );

        let expected =
            crate::runtime::terminal_store::PtyReclaimReason::IdleTimeout { timeout: LIMIT }
                .describe(limited);
        let params = json!({ "id": limited, "text": "x" });
        for message in [
            error_message(handle_wait(&mut e, json!(3), &params)),
            error_message(handle_read(&mut e, json!(4), &params)),
            error_message(handle_write(&mut e, json!(5), &params)),
            error_message(handle_kill(&mut e, json!(6), &params)),
        ] {
            assert_eq!(message, expected);
            assert!(!message.contains("not found"), "{message}");
        }
        ok(handle_kill(&mut e, json!(7), &json!({ "id": unlimited })));
    }

    #[test]
    fn exited_entry_cleanup_informs_and_answers_with_the_exit_code() {
        let (_state, mut session) = crate::state::tests::test_state();
        let mut e = session.borrow_mut();
        let (mut c, _home) = core();
        let pty_id = spawn_with(&mut c, &mut e, json!({ "command": ["exit", "7"] }));
        let exited = wait_for_exit(&mut e, pty_id, "exit 7");
        assert_eq!(exited["exit_code"].as_i64(), Some(7));
        assert_eq!(
            e.reclaim_standalone_ptys(Instant::now() + Duration::from_secs(3600)),
            0,
            "the first sweep only starts the retention"
        );
        let state = ok(handle_wait(&mut e, json!(2), &json!({ "id": pty_id })));
        assert_eq!(state["exit_code"].as_i64(), Some(7));

        e.runtime.terminals.set_exited_retention(Duration::ZERO);
        ok(handle_list(&mut e, json!(3)));
        assert!(!e.runtime.terminals.is_standalone(pty_id));
        let notice = e
            .live
            .notifications
            .all()
            .find(|n| n.body.contains(&pty_id.to_string()))
            .expect("cleanup notification");
        assert_eq!(notice.level, crate::notification::NotificationLevel::Info);

        let message = error_message(handle_wait(&mut e, json!(4), &json!({ "id": pty_id })));
        assert!(message.contains("exit code 7"), "{message}");
        assert!(!message.contains("not found"), "{message}");
        let bogus = pty_id + 1000;
        assert_eq!(
            error_message(handle_wait(&mut e, json!(5), &json!({ "id": bogus }))),
            format!("headless pty {bogus} not found")
        );
    }
}
