//! 등록된 IPC method는 그대로 두고 params 값의 자리표시자만 채워 순서대로 요청한다.
//! 스텝 결과는 로그에 남기고 [`SequenceOutcome`]으로 돌려준다. 전달은 출처별 호출자가 정한다.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use super::outcome::{SequenceOutcome, StepResult, StepStatus};
use super::types::IpcCall;
use tasty_ipc::host_call::{HostIpcInjector, InjectError};

/// 한 스텝 응답을 기다릴 시간. 명령 실행을 취소하는 기한은 아니다.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// 호출자가 준비한 body·소문자 헤더·query 값. 여기서는 HTTP 파싱을 하지 않는다.
#[derive(Debug, Clone, Default)]
pub struct SubstitutionContext {
    pub body: Value,
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").expect("valid placeholder regex"))
}

/// 문자열 전체가 하나의 자리표시자일 때만 내부 참조를 반환한다.
fn whole_placeholder(s: &str) -> Option<&str> {
    let inner = s.strip_prefix("${")?.strip_suffix('}')?;
    if inner.contains("${") || inner.contains('}') {
        return None;
    }
    Some(inner)
}

fn resolve_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        cur = match cur {
            Value::Object(map) => map.get(seg)?,
            Value::Array(arr) => {
                let idx: usize = seg.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

fn resolve_ref(reference: &str, ctx: &SubstitutionContext) -> Option<Value> {
    let (scope, path) = reference.split_once('.')?;
    match scope {
        "body" => resolve_json_path(&ctx.body, path).cloned(),
        "header" => ctx
            .headers
            .get(&path.to_ascii_lowercase())
            .map(|s| Value::String(s.clone())),
        "query" => ctx.query.get(path).map(|s| Value::String(s.clone())),
        _ => None,
    }
}

fn render_embedded(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 전체가 자리표시자이면 JSON 타입을 유지하고 없으면 Null이다.
/// 문장 안의 자리표시자는 문자열로 바꾸며 찾지 못한 값은 빈 문자열이다.
fn substitute_string(s: &str, ctx: &SubstitutionContext) -> Value {
    if let Some(inner) = whole_placeholder(s) {
        return resolve_ref(inner, ctx).unwrap_or(Value::Null);
    }
    let replaced = placeholder_re().replace_all(s, |caps: &regex::Captures| {
        resolve_ref(&caps[1], ctx)
            .as_ref()
            .map(render_embedded)
            .unwrap_or_default()
    });
    Value::String(replaced.into_owned())
}

/// params의 문자열 값만 치환한다. 객체 키나 method는 바꾸지 않는다.
pub fn substitute_params(template: &Value, ctx: &SubstitutionContext) -> Value {
    match template {
        Value::String(s) => substitute_string(s, ctx),
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute_params(v, ctx)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), substitute_params(v, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 앞 스텝의 응답 또는 대기 종료 뒤 다음 요청을 보낸다. 실패해도 계속하며 재시도하지 않는다.
/// 응답을 기다리다 포기해도 이미 들어간 명령은 나중에 실행될 수 있다.
/// 따라서 요청 순서는 유지하지만 이전 명령의 완료를 기다린다고 보장하지는 않는다.
pub fn execute_sequence(
    origin: SequenceOrigin,
    injector: &HostIpcInjector,
    calls: &[IpcCall],
    ctx: &SubstitutionContext,
) -> SequenceOutcome {
    let mut outcome = SequenceOutcome::default();
    for (i, call) in calls.iter().enumerate() {
        let params = substitute_params(&call.params, ctx);
        let status = match injector.dispatch_even_if_abandoned(&call.method, params, STEP_TIMEOUT) {
            Ok(_result) => {
                tracing::debug!("{origin} IpcSequence step {i} ({}) ok", call.method);
                StepStatus::Ok
            }
            Err(e) => {
                log_step_failure(origin, i, &call.method, &e);
                StepStatus::from_error(&e)
            }
        };
        outcome.steps.push(StepResult {
            index: i,
            method: call.method.clone(),
            status,
        });
    }
    outcome
}

/// 로그에 남길 호출 출처. 바인딩 허용 여부를 판단하는 TriggerSource와는 별개다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceOrigin {
    Webhook,
    SurfaceHook,
    Dispatch,
}

impl std::fmt::Display for SequenceOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Webhook => "webhook",
            Self::SurfaceHook => "surface hook",
            Self::Dispatch => "hook_handler.dispatch",
        })
    }
}

/// 큐에 넣지 못한 오류와 요청 이후 오류를 구분해 기록한다. 재시도는 하지 않는다.
fn log_step_failure(origin: SequenceOrigin, i: usize, method: &str, e: &InjectError) {
    if e.nothing_ran() {
        tracing::error!(
            "{origin} IpcSequence step {i} ({method}) not run — the host did not queue it: {e}"
        );
    } else {
        tracing::error!("{origin} IpcSequence step {i} ({method}) failed: {e}");
    }
}

/// 셸 프로세스의 진행. 종료 코드는 정보일 뿐 성공·실패 판정에 쓰지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    Started,
    /// 프로세스나 작업 스레드를 띄우지 못했다.
    NotStarted(String),
    /// 종료 코드. 시그널 종료처럼 코드가 없으면 None이다.
    Exited(Option<i32>),
}

/// command·args로 직접 프로세스를 실행하고 작업 스레드에서 종료를 기다린다.
/// 셸을 자동으로 거치지 않는다. 시작·시작 실패·종료를 `report`에 차례로 알린다.
pub fn spawn_shell(
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    report: impl FnMut(ShellEvent) + Send + 'static,
) {
    let report: SharedReport = std::sync::Arc::new(std::sync::Mutex::new(Box::new(report)));
    let in_thread = report.clone();
    let spawned = std::thread::Builder::new()
        .name("hook-shell".into())
        .spawn(move || run_shell(&command, &args, env, &in_thread));
    if let Err(e) = spawned {
        tracing::warn!("hook shell thread spawn failed: {e}");
        send(&report, ShellEvent::NotStarted(e.to_string()));
    }
}

type SharedReport = std::sync::Arc<std::sync::Mutex<Box<dyn FnMut(ShellEvent) + Send>>>;

fn send(report: &SharedReport, event: ShellEvent) {
    let mut report =
        crate::poison::recover_mutex(report.lock(), "hook shell report", &REPORT_POISONED);
    (*report)(event);
}

fn run_shell(command: &str, args: &[String], env: Vec<(String, String)>, report: &SharedReport) {
    match start_child(command, args, env) {
        Ok(child) => {
            send(report, ShellEvent::Started);
            send(report, ShellEvent::Exited(wait_code(command, child)));
        }
        Err(e) => send(report, ShellEvent::NotStarted(e)),
    }
}

fn start_child(
    command: &str,
    args: &[String],
    env: Vec<(String, String)>,
) -> Result<std::process::Child, String> {
    let mut cmd = hook_exec_command(
        command,
        args,
        env,
        tasty_utils::process::process_env_keys_to_strip(),
    );
    tasty_utils::process::hide_console(&mut cmd)
        .spawn()
        .map_err(|e| {
            tracing::warn!("hook shell '{command}' spawn failed: {e}");
            e.to_string()
        })
}

fn wait_code(command: &str, mut child: std::process::Child) -> Option<i32> {
    match child.wait() {
        Ok(status) => {
            tracing::debug!("hook shell '{command}' ran");
            status.code()
        }
        Err(e) => {
            tracing::warn!("hook shell '{command}' wait failed: {e}");
            None
        }
    }
}

static REPORT_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 셸을 거치지 않는 훅 Command를 만든다. `strip` 키는 상속 환경에서 지운다. 터미널 셸과 같은
/// 판정에 바깥 인스턴스의 호출자 신원을 더한 목록이다(`tasty_utils::process::hook_env_keys_to_strip`).
fn hook_exec_command(
    command: &str,
    args: &[String],
    env: Vec<(String, String)>,
    strip: Vec<std::ffi::OsString>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args).envs(env);
    for key in strip {
        cmd.env_remove(key);
    }
    // 호스트 입력을 읽거나 호스트 stdout·stderr에 쓰지 않는다. 출력은 쓰지 않으므로 모으지도 않는다.
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 직접 실행 훅은 호스트의 stdin·stdout·stderr를 물려받지 않는다.
    #[cfg(target_os = "linux")]
    #[test]
    fn hook_exec_does_not_inherit_host_stdio() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("fds");
        // 셸의 stdout은 marker로 돌리므로 stdin(0)과 stderr(2)를 기록한다.
        let script = format!(
            "for n in 0 2; do readlink /proc/$$/fd/$n; done > {}",
            marker.display()
        );
        let (tx, rx) = std::sync::mpsc::channel();
        spawn_shell(
            "sh".into(),
            vec!["-c".into(), script],
            Vec::new(),
            move |event| {
                if let ShellEvent::Exited(_) = event {
                    tx.send(()).expect("report exit");
                }
            },
        );
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the shell exits");
        let fds = std::fs::read_to_string(&marker).expect("marker written");
        assert_eq!(fds.lines().collect::<Vec<_>>(), ["/dev/null", "/dev/null"]);
    }

    /// 직접 실행 훅도 터미널 셸과 같은 판정으로 활성 플러그인이 선언한 세션 키와 CMUX_* 를 지우고,
    /// 바깥 인스턴스의 신원 변수도 지운다.
    #[test]
    fn hook_exec_strips_claude_session_and_cmux_env() {
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
        let cmd = hook_exec_command("true", &[], env, strip);
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

    fn capture_logs(f: impl FnOnce()) -> String {
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf(Arc::new(Mutex::new(Vec::new())));
        let sink = buf.clone();
        let sub = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish();
        // 다른 검사가 먼저 등록한 tracing 콜사이트도 현재 구독자로 수집하도록 캐시를 갱신한다.
        tracing::subscriber::with_default(sub, || {
            tracing::callsite::rebuild_interest_cache();
            f()
        });
        let out = buf.0.lock().unwrap().clone();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn step_logs_name_the_origin_that_fired_the_sequence() {
        // The hook-sequence worker hits the same step log callsites with no subscriber, which can
        // cache "never" for the whole process (docs/dev-guide/unit-test-isolation.md).
        if crate::test_support::rerun_alone(
            module_path!(),
            "step_logs_name_the_origin_that_fired_the_sequence",
        ) {
            return;
        }
        use std::sync::{Arc, mpsc};
        use tasty_ipc::server::IpcCommand;

        for (origin, label) in [
            (SequenceOrigin::Webhook, "webhook"),
            (SequenceOrigin::SurfaceHook, "surface hook"),
            (SequenceOrigin::Dispatch, "hook_handler.dispatch"),
        ] {
            let (tx, rx) = mpsc::channel::<IpcCommand>();
            let injector = HostIpcInjector::new(tx, Arc::new(|| {}));
            let answerer = std::thread::spawn(move || {
                for ok in [true, false] {
                    let cmd = rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("a step reaches the queue");
                    let id = cmd.request.id.clone().unwrap_or(Value::Null);
                    let resp = if ok {
                        tasty_ipc::protocol::JsonRpcResponse::success(id, json!({}))
                    } else {
                        tasty_ipc::protocol::JsonRpcResponse::internal_error(id, "probe refused")
                    };
                    cmd.response_tx.send(resp).expect("answer the step");
                }
            });
            let calls = [
                IpcCall {
                    method: "probe.ok".into(),
                    params: json!({}),
                },
                IpcCall {
                    method: "probe.refused".into(),
                    params: json!({}),
                },
            ];
            let mut outcome = None;
            let logs = capture_logs(|| {
                outcome = Some(execute_sequence(
                    origin,
                    &injector,
                    &calls,
                    &SubstitutionContext::default(),
                ));
            });
            answerer.join().expect("answerer thread");
            let outcome = outcome.expect("the sequence ran");
            assert_eq!(
                outcome.verdict(),
                super::super::outcome::Verdict::Partial,
                "a failed step is not success"
            );
            assert!(matches!(
                outcome.steps[1].status,
                StepStatus::Failed(ref e) if e.contains("probe refused")
            ));
            assert!(
                logs.contains(&format!("{label} IpcSequence step 0 (probe.ok) ok")),
                "{origin:?}: {logs}"
            );
            assert!(
                logs.contains(&format!(
                    "{label} IpcSequence step 1 (probe.refused) failed"
                )),
                "{origin:?}: {logs}"
            );
            if origin != SequenceOrigin::Webhook {
                assert!(!logs.contains("webhook"), "{origin:?}: {logs}");
            }
        }
    }

    fn ctx() -> SubstitutionContext {
        let mut headers = BTreeMap::new();
        headers.insert("x-signature".to_string(), "abc123".to_string());
        let mut query = BTreeMap::new();
        query.insert("token".to_string(), "qtok".to_string());
        SubstitutionContext {
            body: json!({"message": "hi", "nested": {"n": 42}, "arr": [1, 2, 3]}),
            headers,
            query,
        }
    }

    #[test]
    fn whole_placeholder_preserves_type() {
        let out = substitute_params(&json!("${body.nested.n}"), &ctx());
        assert_eq!(out, json!(42));
    }

    #[test]
    fn whole_placeholder_string_value() {
        let out = substitute_params(&json!("${body.message}"), &ctx());
        assert_eq!(out, json!("hi"));
    }

    #[test]
    fn embedded_placeholder_is_textual() {
        let out = substitute_params(&json!("msg: ${body.message}!"), &ctx());
        assert_eq!(out, json!("msg: hi!"));
    }

    #[test]
    fn header_and_query_refs() {
        let out = substitute_params(
            &json!({"sig": "${header.X-Signature}", "tok": "${query.token}"}),
            &ctx(),
        );
        assert_eq!(out, json!({"sig": "abc123", "tok": "qtok"}));
    }

    #[test]
    fn array_index_path() {
        let out = substitute_params(&json!("${body.arr.1}"), &ctx());
        assert_eq!(out, json!(2));
    }

    #[test]
    fn unresolved_ref_is_null_or_empty() {
        assert_eq!(
            substitute_params(&json!("${body.missing}"), &ctx()),
            json!(null)
        );
        assert_eq!(
            substitute_params(&json!("x=${body.missing}"), &ctx()),
            json!("x=")
        );
    }

    #[test]
    fn object_keys_are_never_substituted() {
        let out = substitute_params(&json!({"${body.message}": "v"}), &ctx());
        assert_eq!(out, json!({"${body.message}": "v"}));
    }

    /// 직접 프로세스 실행 경로에서 환경변수가 전달되는지 확인한다.
    #[test]
    fn spawn_shell_exposes_env_to_child() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("env.txt");
        // 셸 확장을 검사하려고 셸 자체를 실행한다. 임시 경로에 공백이 없다는 전제가 있다.
        let (command, args) = if cfg!(windows) {
            (
                "cmd".to_string(),
                vec![
                    "/C".to_string(),
                    format!("echo %TASTY_HOOK_EVENT%> {}", marker.display()),
                ],
            )
        } else {
            (
                "sh".to_string(),
                vec![
                    "-c".to_string(),
                    format!("echo \"$TASTY_HOOK_EVENT\" > {}", marker.display()),
                ],
            )
        };
        spawn_shell(
            command,
            args,
            vec![("TASTY_HOOK_EVENT".to_string(), "user/envtest".to_string())],
            |_| {},
        );
        // 리다이렉트는 쓰기 전에 파일을 만들 수 있으므로 존재가 아닌 내용을 기다린다.
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
            "user/envtest",
            "5초 안에 기대한 환경변수 출력을 읽지 못했다; marker exists={}",
            marker.exists()
        );
    }

    #[test]
    fn nested_object_and_array_recurse() {
        let out = substitute_params(
            &json!({"a": ["${body.message}", {"b": "${body.nested.n}"}]}),
            &ctx(),
        );
        assert_eq!(out, json!({"a": ["hi", {"b": 42}]}));
    }

    #[test]
    fn agent_stream_turn_correlation_sequence_substitutes_only_value_slots() {
        // request_id와 message만 치환하고 등록된 surface 값은 유지하는 입력이다.
        let ctx = SubstitutionContext {
            body: json!({"request_id": "req-8f3a", "prompt": "summarize the build log"}),
            ..Default::default()
        };
        let turn_start_params = substitute_params(
            &json!({"surface": 42, "request_id": "${body.request_id}"}),
            &ctx,
        );
        assert_eq!(
            turn_start_params,
            json!({"surface": 42, "request_id": "req-8f3a"}),
            "request_id 는 값 슬롯에서 치환되고 surface 리터럴은 그대로다"
        );
        let tell_params =
            substitute_params(&json!({"message": "${body.prompt}", "surface": 42}), &ctx);
        assert_eq!(
            tell_params,
            json!({"message": "summarize the build log", "surface": 42})
        );
        let key_shaped = substitute_params(&json!({"${body.request_id}": "x"}), &ctx);
        assert_eq!(
            key_shaped,
            json!({"${body.request_id}": "x"}),
            "key 위치는 절대 치환하지 않는다"
        );
    }
}
