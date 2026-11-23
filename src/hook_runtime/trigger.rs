//! 내부 이벤트가 선택한 바인딩을 실행한다. Handler 참조는 등록부의 source·비활성 상태를 검사한다.
//! InlineShell은 이 등록부 조회 없이 셸 명령으로 실행한다.

use tasty_hooks::{HookBinding, HookEvent};

use super::notice::{self, AutoOrigin};
use super::shell_pool::{self, ShellJob};
use super::worker::{Delivery, enqueue_sequence};
use crate::hook_handler::env::{HookShellEnv, build_env};
use crate::hook_handler::exec::{SequenceOrigin, ShellEvent, SubstitutionContext};
use crate::hook_handler::registry::global;
use crate::hook_handler::types::{
    HookHandlerAction, HookHandlerId, IpcCall, TriggerSource, validate_binding,
};
use tasty_ipc::host_call::HostIpcInjector;

/// 실제 수신 이벤트로 payload를 만들고 셸 환경변수와 IPC 값 치환에 함께 사용한다.
/// IPC injector가 없으면 해당 시퀀스는 로그 후 건너뛴다.
pub(super) fn execute_binding(
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
    let origin = AutoOrigin::SurfaceHook { surface_id };
    match binding {
        HookBinding::InlineShell(command) => {
            spawn_shell(command.clone(), Vec::new(), shell_env(), origin)
        }
        HookBinding::Handler(id) => {
            execute_handler_binding(id, injector, &payload, shell_env, origin)
        }
    }
}

fn execute_handler_binding(
    id: &str,
    injector: Option<&HostIpcInjector>,
    payload: &serde_json::Value,
    shell_env: impl FnOnce() -> Vec<(String, String)>,
    origin: AutoOrigin,
) {
    let Some(handler) = global().get(&HookHandlerId(id.to_string())) else {
        tracing::warn!("hook references unknown handler '{id}' — skipped");
        notice::not_run(
            origin,
            crate::i18n::t_fmt("hook_notice.handler_missing", id),
        );
        return;
    };
    if let Err(e) = validate_binding(&handler, TriggerSource::Hook) {
        tracing::warn!("hook handler '{id}' cannot bind to hook trigger: {e} — skipped");
        notice::not_run(
            origin,
            crate::i18n::t_fmt2("hook_notice.handler_refused", id, &e.to_string()),
        );
        return;
    }
    match handler.action {
        HookHandlerAction::ShellCommand { command, args } => {
            spawn_shell(command, args, shell_env(), origin)
        }
        HookHandlerAction::IpcSequence { calls } => {
            execute_ipc_sequence_handler(id, injector, payload, &calls, origin)
        }
    }
}

fn execute_ipc_sequence_handler(
    id: &str,
    injector: Option<&HostIpcInjector>,
    payload: &serde_json::Value,
    calls: &[IpcCall],
    origin: AutoOrigin,
) {
    let Some(inj) = injector else {
        tracing::warn!(
            "hook handler '{id}' is an IpcSequence but no IPC injector is available — skipped"
        );
        notice::not_run(origin, crate::i18n::t_fmt("hook_notice.no_injector", id));
        return;
    };
    let ctx = SubstitutionContext {
        body: payload.clone(),
        ..Default::default()
    };
    // 이 스레드가 host 명령도 처리하므로 응답 대기는 별도 worker가 맡아야 한다.
    let sequence_origin = SequenceOrigin::SurfaceHook;
    if let Err(e) = enqueue_sequence(
        sequence_origin,
        id,
        inj,
        calls,
        ctx,
        Delivery::Notify(origin),
    ) {
        tracing::error!("{sequence_origin} IpcSequence '{id}' not run — {e}");
        notice::not_run(
            origin,
            crate::i18n::t_fmt2("hook_notice.not_queued", id, &e.to_string()),
        );
    }
}

/// 등록 패턴이 아닌 실제 수신 이벤트의 값을 payload에 넣는다.
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

/// command와 args를 공백으로 합쳐 sh/cmd로 실행하도록 공유 실행기에 넣는다. args를 인자별로 escape하지 않는다.
/// 실행기가 받지 않았거나 셸을 띄우지 못하면 알림을 남기며 종료 코드의 성공 여부는 검사하지 않는다.
fn spawn_shell(command: String, args: Vec<String>, env: Vec<(String, String)>, origin: AutoOrigin) {
    submit_hook_shell(shell_pool::shared(), command, args, env, origin);
}

pub(super) fn submit_hook_shell(
    pool: &std::sync::Arc<shell_pool::ShellPool>,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    origin: AutoOrigin,
) {
    let full = if args.is_empty() {
        command
    } else {
        format!("{command} {}", args.join(" "))
    };
    let label = full.clone();
    let submitted = pool.submit(|| {
        let reported = full.clone();
        ShellJob::new(
            full.clone(),
            move || {
                hook_shell_command(
                    &full,
                    env,
                    tasty_utils::process::process_env_keys_to_strip(),
                )
            },
            move |event| {
                if let ShellEvent::NotStarted(e) = event {
                    notice::not_run(
                        origin,
                        crate::i18n::t_fmt2("hook_notice.spawn_failed", &reported, &e),
                    );
                }
            },
        )
    });
    if let Err(reason) = submitted {
        tracing::warn!("hook shell command not run: {reason}; cmd: {label}");
        notice::shell_not_queued(origin, &label, &reason);
    }
}

/// 훅 명령을 sh/cmd로 감싼 Command를 만든다. `strip` 키는 상속 환경에서 지운다. 터미널 셸과 같은
/// 판정에 바깥 인스턴스의 호출자 신원을 더한 목록이다(`tasty_utils::process::hook_env_keys_to_strip`).
fn hook_shell_command(
    full: &str,
    env: Vec<(String, String)>,
    strip: Vec<std::ffi::OsString>,
) -> std::process::Command {
    let mut process = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", full]);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", full]);
        c
    };
    process.envs(env);
    // 호스트 입력을 읽거나 호스트 stdout·stderr에 쓰지 않는다. 한 건의 끝은 셸의 종료다.
    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for key in strip {
        process.env_remove(key);
    }
    // 제한된 PATH에서도 자기 바이너리를 찾도록 현재 실행 파일의 디렉터리를 앞에 붙인다.
    if let Some(path) = tasty_utils::process::path_prepending_self_dir(std::env::var_os("PATH")) {
        process.env("PATH", path);
    }
    process
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 훅 셸은 터미널 셸과 같은 판정으로 활성 플러그인이 선언한 세션 키와 CMUX_* 를 지우고,
    /// 바깥 인스턴스의 신원 변수도 지운다. 나머지는 남긴다.
    #[test]
    fn hook_shell_strips_claude_session_and_cmux_env() {
        use tasty_test_support::strip_env_keys::{
            KEPT, STRIPPED, inherited, install_bundled_plugin_rules,
        };
        use tasty_utils::process::{OUTER_IDENTITY_ENV, hook_env_keys_to_strip};
        install_bundled_plugin_rules();
        let env = KEPT
            .iter()
            .map(|k| ((*k).to_owned(), "1".to_owned()))
            .collect();
        let mut inherited = inherited();
        inherited.extend(
            OUTER_IDENTITY_ENV
                .iter()
                .map(|k| (k.into(), "outer".into())),
        );
        let strip = hook_env_keys_to_strip(inherited);
        let cmd = hook_shell_command("true", env, strip);
        let envs: Vec<_> = cmd.get_envs().collect();
        for key in STRIPPED.iter().chain(OUTER_IDENTITY_ENV) {
            assert!(
                envs.contains(&(std::ffi::OsStr::new(key), None)),
                "{key} 는 지워져야 한다"
            );
        }
        for key in KEPT.iter().filter(|k| !OUTER_IDENTITY_ENV.contains(k)) {
            assert!(
                envs.iter()
                    .any(|(k, v)| *k == std::ffi::OsStr::new(key) && v.is_some()),
                "{key} 는 남아야 한다"
            );
        }
    }

    #[test]
    fn inline_shell_binding_spawns_and_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("hook-ran.txt");
        // 이 검사는 임시 경로에 공백이 없다는 전제로 리다이렉트 문자열을 만든다.
        let cmd = format!("echo ok > {}", marker.display());

        execute_binding(
            &HookBinding::InlineShell(cmd),
            None,
            &HookEvent::Bell,
            &HookEvent::Bell,
            1,
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !marker.exists() {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            marker.exists(),
            "inline shell hook did not create marker file"
        );
    }

    #[test]
    fn shell_binding_receives_hook_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("hook-env.txt");
        // cmd는 > 바로 앞 숫자를 fd로 해석하므로 공백을 두고 출력은 trim한다.
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

        // 실행기 보고(Exited)를 기다려 벽시계 없이 끝을 안다. execute_binding 전체가 아니라 환경변수 전달을 검사한다.
        let env = build_env(&HookShellEnv {
            event: HookEvent::Bell.to_display_string(),
            source: "hook",
            surface_id: Some(42),
            payload: trigger_payload(&HookEvent::Bell, 42),
        });
        run_hook_shell_and_wait(cmd, env);
        let content = std::fs::read_to_string(&marker).expect("marker written");
        assert_eq!(content.trim(), "bell/hook/42");
    }

    /// 훅 셸 명령을 시험 전용 실행기에서 돌리고 셸이 끝날 때까지 기다린다.
    fn run_hook_shell_and_wait(cmd: String, env: Vec<(String, String)>) {
        let pool = shell_pool::ShellPool::new(1, 0);
        let (tx, rx) = std::sync::mpsc::channel();
        pool.submit(|| {
            ShellJob::new(
                "test",
                move || hook_shell_command(&cmd, env, Vec::new()),
                move |event| {
                    if let ShellEvent::Exited(_)
                    | ShellEvent::NotStarted(_)
                    | ShellEvent::WaitFailed(_) = event
                    {
                        tx.send(event).expect("report the end");
                    }
                },
            )
        })
        .expect("accepted");
        let end = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the shell ends");
        assert!(matches!(end, ShellEvent::Exited(_)), "{end:?}");
    }

    /// surface 훅 셸은 호스트 stdio를 물려받지 않고 출력을 모으는 pipe도 쓰지 않는다.
    /// 셸의 fd 0·1·2가 모두 null 장치다. `$$`는 명령 치환 안에서도 셸 자신의 pid다.
    #[cfg(target_os = "linux")]
    #[test]
    fn hook_shell_stdio_is_null() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("fds");
        let cmd = format!(
            "a=$(readlink /proc/$$/fd/0); b=$(readlink /proc/$$/fd/1); c=$(readlink /proc/$$/fd/2); \
             echo \"$a $b $c\" > {}",
            marker.display()
        );
        run_hook_shell_and_wait(cmd, Vec::new());
        let fds = std::fs::read_to_string(&marker).expect("marker written");
        assert_eq!(fds.trim(), "/dev/null /dev/null /dev/null");
    }

    /// 테스트 바이너리를 PATH로 찾아 --list 출력까지 얻는지 확인한다.
    /// 파일은 명령 실행 실패 때도 생성될 수 있어 내용을 확인해야 한다.
    #[test]
    fn inline_shell_resolves_self_binary_via_augmented_path() {
        let exe = std::env::current_exe().expect("current_exe");
        let basename = exe
            .file_name()
            .expect("file_name")
            .to_string_lossy()
            .into_owned();
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("resolved.txt");
        // --list는 목록만 출력하므로 자식에서 검사가 재귀 실행되지 않는다.
        let cmd = format!("{basename} --list > {}", marker.display());

        execute_binding(
            &HookBinding::InlineShell(cmd),
            None,
            &HookEvent::Bell,
            &HookEvent::Bell,
            1,
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut content = String::new();
        assert!(
            tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(25), |_| {
                content = std::fs::read_to_string(&marker).unwrap_or_default();
                !content.trim().is_empty()
            }),
            "self binary was not resolved via augmented PATH — marker empty (command not found leaves a 0-byte redirect file)"
        );
        assert!(
            content.contains(": test"),
            "marker does not contain --list output; got: {content:?}"
        );
    }

    /// 호출 스레드가 반환 뒤 IPC를 처리해 별도 worker에서 응답을 기다리는지 확인한다.
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
            AutoOrigin::SurfaceHook { surface_id: 5 },
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

    /// 응답을 직접 돌려주는 이 입력에서 시퀀스의 요청 순서가 섞이지 않는지 확인한다.
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
            AutoOrigin::SurfaceHook { surface_id: 5 },
        );
        execute_ipc_sequence_handler(
            "user/second",
            Some(&injector),
            &payload,
            &[step("second.a")],
            AutoOrigin::SurfaceHook { surface_id: 5 },
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

        // 파일 생성만으로는 쓰기를 마쳤다고 볼 수 없어 내용을 기다린다.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut content = String::new();
        // 결과는 아래 내용 단언이 메시지와 함께 판정한다.
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(25), |_| {
            content = std::fs::read_to_string(&marker).unwrap_or_default();
            !content.trim().is_empty()
        });
        assert_eq!(
            content.trim(),
            "1",
            "5 초 안에 마커에 exit code 가 안 쓰였다 — 훅이 `TASTY_HOOK_EXIT_CODE` 를 \
             셸 자식에게 전달하지 않으면 여기서 잡힌다"
        );
    }
}
