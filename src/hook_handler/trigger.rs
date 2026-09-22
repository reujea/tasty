//! 내부 이벤트(hook) 트리거 → 바인딩 실행 (S9).
//!
//! `tasty-hooks` 는 leaf 크레이트라 공유 훅 핸들러 레지스트리를 볼 수 없어
//! (surface, event) 매칭만 하고 [`HookBinding`] 을 돌려준다. 실제 실행 —
//! 레지스트리 조회 + `source` 게이트(hook 수용 여부) + ShellCommand/IpcSequence
//! 분기 — 는 여기서 한다.
//!
//! ## source 게이트
//! 트리거 출처는 항상 [`TriggerSource::Hook`]. [`validate_binding`] 이 핸들러의
//! `source` 가 `Hook | Any` 인지 확인한다 — `Webhook` 전용 핸들러는 hook 트리거로
//! 실행되지 않는다(§3.1 셸/흐름 분리의 역방향 게이트).
//!
//! ## 하위호환
//! [`HookBinding::InlineShell`] 은 옛 `hook.set --command` 의 인라인 셸을 그대로
//! 실행한다(익명 hook 핸들러). 옛 `tasty-hooks::check_and_fire` 가 하던 셸 spawn 을
//! 여기로 옮긴 것이라 동작이 동일하다.

use tasty_hooks::{HookBinding, HookEvent};

use super::env::{HookShellEnv, build_env};
use super::exec::{SubstitutionContext, enqueue_sequence};
use super::registry::global;
use super::types::{HookHandlerAction, HookHandlerId, IpcCall, TriggerSource, validate_binding};
use tasty_ipc::host_call::HostIpcInjector;

/// 발사된 훅의 바인딩을 실행한다 (fire-and-forget).
///
/// `injector` 는 IpcSequence 핸들러 실행에만 필요하다 — 없으면 IpcSequence 는
/// 건너뛰고 warn 을 남긴다(셸은 injector 불요). `event`(등록 이벤트) + `surface_id`
/// 는 셸 핸들러 자식 프로세스에 `TASTY_HOOK_*` env 로 노출되는 트리거 컨텍스트다
/// ([`super::env`]). `received`(실제 관측 이벤트, [`tasty_hooks::FiredHook::received`])
/// 로부터 단일 payload `Value` 를 여기서 한 번 조립해 셸(`build_env`
/// 의 `payload`)과 IpcSequence(`SubstitutionContext.body`) 양쪽에 같은 소스로
/// 공급한다 — 두 경로가 각자 빈 값을 공급하던 것을 하나로 모은다.
pub fn execute_binding(
    binding: &HookBinding,
    injector: Option<&HostIpcInjector>,
    event: &HookEvent,
    received: &HookEvent,
    surface_id: u32,
) {
    let payload = trigger_payload(received, surface_id);
    let shell_env = || {
        build_env(&HookShellEnv {
            event: event.to_display_string(),
            source: "hook",
            surface_id: Some(surface_id),
            payload: payload.clone(),
        })
    };
    match binding {
        // 하위호환: 익명 셸 핸들러. 옛 check_and_fire 의 셸 spawn 과 동일 동작.
        HookBinding::InlineShell(command) => {
            // fire-and-forget: 프로덕션은 자식 완료를 기다리지 않는다(UI 블록 방지). 반환된
            // JoinHandle 은 버린다 — 스레드는 detach 되어 계속 돈다. 테스트만 이 핸들을 받아 join 한다.
            let _ = spawn_shell(command.clone(), Vec::new(), shell_env());
        }
        // 레지스트리 핸들러 참조 — 조회 + source 게이트 후 action 분기.
        HookBinding::Handler(id) => execute_handler_binding(id, injector, &payload, shell_env),
    }
}

/// [`HookBinding::Handler`] 참조 실행 — 레지스트리 조회 + `source` 게이트 +
/// ShellCommand/IpcSequence 분기. `execute_binding` 에서 분리(cognitive complexity
/// 게이트, payload 조립 추가로 초과).
fn execute_handler_binding(
    id: &str,
    injector: Option<&HostIpcInjector>,
    payload: &serde_json::Value,
    shell_env: impl FnOnce() -> Vec<(String, String)>,
) {
    let Some(handler) = global().get(&HookHandlerId(id.to_string())) else {
        tracing::warn!("hook references unknown handler '{id}' — skipped");
        return;
    };
    if let Err(e) = validate_binding(&handler, TriggerSource::Hook) {
        tracing::warn!("hook handler '{id}' cannot bind to hook trigger: {e} — skipped");
        return;
    }
    match handler.action {
        HookHandlerAction::ShellCommand { command, args } => {
            // fire-and-forget: InlineShell arm 과 같다 — 반환된 JoinHandle 은 버린다(스레드는
            // detach 되어 계속 돈다). 프로덕션은 hook 자식 완료를 기다리지 않는다.
            let _ = spawn_shell(command, args, shell_env());
        }
        HookHandlerAction::IpcSequence { calls } => {
            execute_ipc_sequence_handler(id, injector, payload, &calls)
        }
    }
}

/// `execute_handler_binding` 의 `IpcSequence` 분기 — injector 부재 게이트 +
/// 실행. 별도 함수로 뺀 것 자체가 cognitive complexity 완화 목적(중첩 match 제거).
fn execute_ipc_sequence_handler(
    id: &str,
    injector: Option<&HostIpcInjector>,
    payload: &serde_json::Value,
    calls: &[IpcCall],
) {
    let Some(inj) = injector else {
        tracing::warn!(
            "hook handler '{id}' is an IpcSequence but no IPC injector is available — skipped"
        );
        return;
    };
    let ctx = SubstitutionContext {
        body: payload.clone(),
        ..Default::default()
    };
    // 이 함수는 호스트 명령 큐를 비우는 스레드(GUI 메인 · headless 루프)에서 불린다 — 스텝의 답을
    // 여기서 기다리면 그 스레드가 스텝마다 대기 상한까지 선다. 실행기 스레드에 넘기고 돌아온다.
    enqueue_sequence(id, inj, calls, ctx);
}

/// 훅 트리거 payload 조립 — 셸 env(`TASTY_HOOK_*`)와 IpcSequence(`${body.*}`) 양쪽이
/// 같은 소스에서 파생되는 단일 지점. `surface_id` 는 모든 이벤트에
/// 공통, 그 외 키는 `received` 의 실제 관측값에서 이벤트별로 채운다 — 등록 패턴이
/// 아니라 실제 수신값이어야 하는 이유는 [`tasty_hooks::FiredHook::received`] 참조.
fn trigger_payload(received: &HookEvent, surface_id: u32) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("surface_id".to_string(), serde_json::json!(surface_id));
    match received {
        HookEvent::CommandCompleted(exit_code) => {
            obj.insert("exit_code".to_string(), serde_json::json!(exit_code));
        }
        HookEvent::OutputMatch(text) => {
            obj.insert("matched_text".to_string(), serde_json::json!(text));
        }
        HookEvent::IdleTimeout(elapsed_secs) => {
            obj.insert(
                "idle_elapsed_secs".to_string(),
                serde_json::json!(elapsed_secs),
            );
        }
        HookEvent::Custom(name) => {
            obj.insert("custom_event".to_string(), serde_json::json!(name));
        }
        HookEvent::ProcessExit | HookEvent::Bell | HookEvent::Notification => {}
    }
    serde_json::Value::Object(obj)
}

/// 셸 명령을 백그라운드 스레드에서 fire-and-forget 실행. spawn 실패는 warn.
///
/// `args` 가 있으면 명령 문자열 뒤에 공백 join 해 붙인다(레지스트리 ShellCommand
/// 용 — 인라인 셸은 항상 args 없음). `env` 는 트리거 컨텍스트(`TASTY_HOOK_*`) —
/// 값 전달 전용이며 실행 대상은 바꾸지 못한다.
fn spawn_shell(
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let full = if args.is_empty() {
            command
        } else {
            format!("{command} {}", args.join(" "))
        };
        let mut process = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", &full]);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &full]);
            c
        };
        process.envs(env);
        // 패키징된 macOS `.app`(LaunchServices 경유 실행)은 PATH 를
        // `/usr/bin:/bin:/usr/sbin:/sbin` 으로 제한한다. 이 최소 PATH 를 그대로
        // 상속하면 `tasty` 자기 자신을 재호출하는 hook 커맨드(notify-done 등)가
        // `command not found`(exit 127)로 조용히 실패한다. 실행 중인 바이너리
        // 자신의 디렉토리를 PATH 맨 앞에 붙여 self 재호출을 항상 해결한다.
        // Terminal::new(PTY 셸)와 동일한 보강을 공유 헬퍼로 적용한다.
        if let Some(path) = tasty_utils::process::path_prepending_self_dir(std::env::var_os("PATH"))
        {
            process.env("PATH", path);
        }
        if let Err(e) = tasty_utils::process::hide_console(&mut process).output() {
            tracing::warn!("hook shell command spawn failed: {e}; cmd: {full}");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 하위호환(익명 셸) 경로가 실제로 셸을 spawn 해 명령을 실행함을 증명한다 —
    /// 옛 `tasty-hooks::check_and_fire` 가 하던 셸 실행이 본체로 옮겨온 뒤에도
    /// 동일하게 동작하는지(ProcessExit 훅 등 셸 훅 회귀 방어) 검증한다.
    #[test]
    fn inline_shell_binding_spawns_and_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("hook-ran.txt");
        // 리다이렉트(`>`)는 cmd/sh 양쪽에서 동일 문법. 경로에 인용부호를 넣지
        // 않는다 — cmd 는 std 의 백슬래시-인용 이스케이프를 이해하지 못한다(셸
        // 호출은 옛 tasty-hooks 와 동일하며, 여기선 공백 없는 temp 경로를 쓴다).
        let cmd = format!("echo ok > {}", marker.display());

        execute_binding(
            &HookBinding::InlineShell(cmd),
            None,
            &HookEvent::Bell,
            &HookEvent::Bell,
            1,
        );

        // fire-and-forget 백그라운드 스레드 — 파일 생성까지 폴링.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !marker.exists() {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            marker.exists(),
            "inline shell hook did not create marker file"
        );
    }

    /// 셸 훅 자식 프로세스가 `TASTY_HOOK_*` env 를 실제로 받는지 실 spawn 으로
    /// 증명한다 — cmd(`%VAR%`)/sh(`$VAR`) 각자의 확장 문법으로 값을 파일에 남긴다.
    #[test]
    fn shell_binding_receives_hook_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("hook-env.txt");
        // cmd 함정: `>` 직전 문자가 숫자면 fd 리다이렉트로 파싱된다(`42> f` = stderr).
        // 리다이렉트 앞에 공백을 두고, 결과의 trailing space 는 trim 으로 흡수한다.
        let cmd = if cfg!(windows) {
            format!(
                "echo %TASTY_HOOK_EVENT%/%TASTY_HOOK_SOURCE%/%TASTY_HOOK_SURFACE_ID% > {}",
                marker.display()
            )
        } else {
            format!(
                "echo \"$TASTY_HOOK_EVENT/$TASTY_HOOK_SOURCE/$TASTY_HOOK_SURFACE_ID\" > {}",
                marker.display()
            )
        };

        // 벽시계 마감(5s 폴링) 대신 자식 완료를 기다린다 — spawn_shell 의 스레드는 자식
        // 프로세스를 .output() 으로 끝까지 기다린 뒤 종료하므로, 그 JoinHandle 을 join 하면
        // marker 가 확실히 쓰인 뒤 반환한다. 옛 5s 데드라인은 부하 높은 러너(특히 Windows
        // cmd spawn)에서 확률적으로 넘겨 marker 미존재 → expect panic 이었다(형태 C).
        // execute_binding 의 InlineShell 분기와 동일한 배선을 재조립한다(build_env(shell env)
        // → spawn_shell): 이 테스트가 검증하는 것은 자식이 TASTY_HOOK_* env 를 받는가이고,
        // build_env(pub) 가 그 env 를 만든다. execute_binding 은 JoinHandle 을 안 돌려주므로
        // (프로덕션 fire-and-forget) 완료를 기다리려면 spawn_shell 을 직접 부른다.
        let env = build_env(&HookShellEnv {
            event: HookEvent::Bell.to_display_string(),
            source: "hook",
            surface_id: Some(42),
            payload: trigger_payload(&HookEvent::Bell, 42),
        });
        spawn_shell(cmd, Vec::new(), env)
            .join()
            .expect("shell thread joined");
        let content = std::fs::read_to_string(&marker).expect("marker written");
        assert_eq!(content.trim(), "bell/hook/42");
    }

    /// end-to-end: self-binary 디렉토리가 없는 PATH 환경에서도 hook 셸 커맨드가
    /// `tasty` 자기 자신(여기선 테스트 바이너리)을 basename 으로 호출해 해결됨을
    /// 증명한다 — `spawn_shell` 이 PATH 를 self-dir 로 보강하기 때문.
    ///
    /// **marker 내용을 검증한다(존재 여부가 아니라).** 셸은 리다이렉션(`>`)을 명령
    /// 실행 전에 처리(O_CREAT|O_TRUNC)하므로, basename 이 `command not found`(exit
    /// 127)여도 marker 파일 자체는 0바이트로 생성된다. 존재만 보면 해결 실패를
    /// 못 잡는 vacuous pass 가 된다. 실제 stdout(`--list` 는 `<name>: test` 라인들을
    /// 출력)이 marker 에 담겼는지까지 확인해야 self 해결 성공을 진짜로 증명한다.
    #[test]
    fn inline_shell_resolves_self_binary_via_augmented_path() {
        let exe = std::env::current_exe().expect("current_exe");
        // file_name 은 windows 에서 `.exe` 확장자를 포함해 그대로 호출 가능하다.
        let basename = exe
            .file_name()
            .expect("file_name")
            .to_string_lossy()
            .into_owned();
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("resolved.txt");
        // 테스트 바이너리를 basename 으로 호출(→ PATH 해결 필요). `--list` 는
        // 테스트를 실행하지 않고 목록(`<name>: test`)만 stdout 으로 출력 후 즉시
        // 종료하므로 재귀가 없고, 해결 성공 시 marker 에 실제 내용이 담긴다.
        // basename 은 해시만 포함(공백 없음)이라 인용부호 불필요 — 기존 테스트 관례.
        let cmd = format!("{basename} --list > {}", marker.display());

        execute_binding(
            &HookBinding::InlineShell(cmd),
            None,
            &HookEvent::Bell,
            &HookEvent::Bell,
            1,
        );

        // 존재가 아니라 실제 stdout 이 담길 때까지 폴링한다. 해결 실패 시 marker 는
        // 0바이트인 채로 남아 deadline 까지 조건을 못 채운다.
        let deadline = Instant::now() + Duration::from_secs(10);
        let content = loop {
            let c = std::fs::read_to_string(&marker).unwrap_or_default();
            if !c.trim().is_empty() || Instant::now() >= deadline {
                break c;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(
            !content.trim().is_empty(),
            "self binary was not resolved via augmented PATH — marker empty (command not found leaves a 0-byte redirect file)"
        );
        // `--list` 출력 형식(`<test name>: test`)의 존재로 실제 실행을 확증한다.
        assert!(
            content.contains(": test"),
            "marker does not contain --list output; got: {content:?}"
        );
    }

    /// surface 훅의 IpcSequence 는 부른 스레드에서 스텝의 답을 기다리지 않는다. 그 스레드가 곧
    /// 호스트 명령 큐를 비우는 스레드라, 기다리면 스텝마다 대기 상한(10 s)까지 선다. 여기서는 부른
    /// 스레드(이 시험)가 큐를 비우는 쪽을 맡는다 — 돌아온 뒤에야 첫 스텝을 받을 수 있다.
    /// 스텝은 순서대로, 앞 스텝의 답을 받은 뒤에 다음 스텝이 들어온다.
    #[test]
    fn an_ipc_sequence_hook_returns_before_its_steps_are_answered_and_runs_them_in_order() {
        use std::sync::{Arc, mpsc};
        use tasty_ipc::server::IpcCommand;

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, Arc::new(|| {}));
        let calls = vec![
            IpcCall {
                method: "probe.first".into(),
                params: serde_json::json!({"surface": "${body.surface_id}"}),
            },
            IpcCall {
                method: "probe.second".into(),
                params: serde_json::json!({}),
            },
        ];

        let started = Instant::now();
        execute_ipc_sequence_handler(
            "user/probe",
            Some(&injector),
            &trigger_payload(&HookEvent::Notification, 5),
            &calls,
        );
        let returned_after = started.elapsed();
        assert!(
            returned_after < Duration::from_secs(2),
            "the hook waited {returned_after:?} for its steps on the thread that drains the queue"
        );

        let answer = |cmd: IpcCommand| {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            cmd.response_tx
                .send(tasty_ipc::protocol::JsonRpcResponse::success(
                    id,
                    serde_json::json!({}),
                ))
                .expect("answer the step");
        };
        let first = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the first step reaches the queue");
        assert_eq!(first.request.method, "probe.first");
        assert_eq!(first.request.params, serde_json::json!({"surface": 5}));
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "the second step must wait for the first step's answer"
        );
        answer(first);
        let second = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the second step reaches the queue after the first is answered");
        assert_eq!(second.request.method, "probe.second");
        answer(second);
    }

    /// 잇달아 발화한 두 훅의 시퀀스는 끼어들지 않는다 — 첫 시퀀스의 모든 스텝이 둘째 시퀀스의 첫
    /// 스텝보다 먼저 큐에 들어간다. 시퀀스마다 스레드를 띄우면(ADR-0498 대안 A) 둘째 시퀀스의 첫
    /// 스텝이 첫 시퀀스의 첫 스텝 답을 기다리지 않고 들어온다.
    #[test]
    fn ipc_sequence_hooks_fired_back_to_back_do_not_interleave_their_steps() {
        use std::sync::{Arc, mpsc};
        use tasty_ipc::server::IpcCommand;

        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, Arc::new(|| {}));
        let step = |method: &str| IpcCall {
            method: method.into(),
            params: serde_json::json!({}),
        };
        let payload = trigger_payload(&HookEvent::Notification, 5);
        execute_ipc_sequence_handler(
            "user/first",
            Some(&injector),
            &payload,
            &[step("first.a"), step("first.b")],
        );
        execute_ipc_sequence_handler(
            "user/second",
            Some(&injector),
            &payload,
            &[step("second.a")],
        );

        let answer = |cmd: IpcCommand| {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            cmd.response_tx
                .send(tasty_ipc::protocol::JsonRpcResponse::success(
                    id,
                    serde_json::json!({}),
                ))
                .expect("answer the step");
        };
        let mut arrived = Vec::new();
        for _ in 0..3 {
            let cmd = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("a step reaches the queue");
            arrived.push(cmd.request.method.clone());
            assert!(
                rx.recv_timeout(Duration::from_millis(200)).is_err(),
                "a step reached the queue while {:?} was unanswered (arrived so far: {arrived:?})",
                cmd.request.method
            );
            answer(cmd);
        }
        assert_eq!(arrived, ["first.a", "first.b", "second.a"]);
    }

    /// 알 수 없는 핸들러 id 참조는 조용히 무시(패닉 없음).
    #[test]
    fn unknown_handler_reference_is_noop() {
        execute_binding(
            &HookBinding::Handler("user/does-not-exist".into()),
            None,
            &HookEvent::Bell,
            &HookEvent::Bell,
            1,
        );
    }

    #[test]
    fn trigger_payload_carries_command_completed_exit_code() {
        let payload = trigger_payload(&HookEvent::CommandCompleted(Some(1)), 7);
        assert_eq!(payload["surface_id"], serde_json::json!(7));
        assert_eq!(payload["exit_code"], serde_json::json!(1));
    }

    #[test]
    fn trigger_payload_carries_output_match_text() {
        let payload = trigger_payload(&HookEvent::OutputMatch("boom detected".into()), 3);
        assert_eq!(payload["matched_text"], serde_json::json!("boom detected"));
    }

    #[test]
    fn trigger_payload_carries_idle_elapsed_secs() {
        let payload = trigger_payload(&HookEvent::IdleTimeout(42), 9);
        assert_eq!(payload["idle_elapsed_secs"], serde_json::json!(42));
    }

    /// exit code 가 셸 자식 프로세스 env 까지 실제로 도달하는지 end-to-end 로
    /// 증명한다 — payload 조립 결함(payload 가 항상 Null 이라 `$TASTY_HOOK_EXIT_CODE`
    /// 가 존재하지 않던 문제)의 회귀 방어.
    #[test]
    fn shell_binding_receives_command_completed_exit_code_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("exit-code.txt");
        let cmd = if cfg!(windows) {
            format!("echo %TASTY_HOOK_EXIT_CODE% > {}", marker.display())
        } else {
            format!("echo \"$TASTY_HOOK_EXIT_CODE\" > {}", marker.display())
        };

        execute_binding(
            &HookBinding::InlineShell(cmd),
            None,
            &HookEvent::CommandCompleted(None),
            &HookEvent::CommandCompleted(Some(1)),
            1,
        );

        // **존재가 아니라 내용을 기다린다.** 리다이렉트(`> file`)는 파일을 먼저 만들고
        // 나중에 쓴다 — `exists()` 로 깨면 빈 파일을 읽는 창이 열린다. 실제로 그 창에
        // 걸려 Windows 잡이 간헐적으로 빨갰다(같은 회차의 다른 8 회는 통과 —
        // 환경변수가 정말 비어 있었다면 매번 실패한다).
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut content = String::new();
        while Instant::now() < deadline {
            content = std::fs::read_to_string(&marker).unwrap_or_default();
            if !content.trim().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(
            content.trim(),
            "1",
            "5 초 안에 마커에 exit code 가 안 쓰였다 — 훅이 `TASTY_HOOK_EXIT_CODE` 를 \
             셸 자식에게 전달하지 않으면 여기서 잡힌다"
        );
    }
}
