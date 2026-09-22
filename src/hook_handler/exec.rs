//! IpcSequence 실행 코어 — owner 가 고정한 IPC 호출들을 페이로드 값으로 채워
//! 순차 실행한다.
//!
//! ## 불변식 강제
//! - **데이터/흐름 분리**: 치환은 [`substitute_params`] 로 **params 값 노드에만**
//!   적용된다. `IpcCall::method` 는 이 모듈의 어떤 함수에도 인자로 넘어가지 않으므로
//!   페이로드가 method 자리에 도달할 코드 경로가 없다. 객체 key 위치도 치환하지
//!   않는다(값 leaf string 만).
//! - **단방향(fire-and-forget)**: [`execute_sequence`] 는 `()` 를 반환한다. 각 IPC
//!   호출 결과는 내부 로깅에만 쓰이고 호출자(웹훅 ACK 빌더)로 되돌아가지 않는다 —
//!   시그니처상 실행 결과가 ACK 경로로 샐 수 없다.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use super::types::IpcCall;
use tasty_ipc::host_call::{HostIpcInjector, InjectError};

/// IpcSequence 한 스텝의 응답 대기 상한. 메인루프 tick + 핸들러 처리 시간 포함.
const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// 치환 컨텍스트 — HTTP 요청(또는 이벤트)에서 추출한 값들.
///
/// `body` 는 파싱된 JSON(파싱 실패/비-JSON 이면 `Null`), `headers` 는 소문자 정규화
/// 이름→값, `query` 는 쿼리 파라미터 이름→값.
#[derive(Debug, Clone, Default)]
pub struct SubstitutionContext {
    pub body: Value,
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
}

/// `${scope.path}` 플레이스홀더 매처. `scope` ∈ {body, header, query}.
fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").expect("valid placeholder regex"))
}

/// 문자열 전체가 정확히 하나의 `${...}` 인 경우 내부 참조를 반환.
fn whole_placeholder(s: &str) -> Option<&str> {
    let inner = s.strip_prefix("${")?.strip_suffix('}')?;
    // 내부에 추가 `${` 나 `}` 가 있으면 "전체가 단일 플레이스홀더" 가 아니다.
    if inner.contains("${") || inner.contains('}') {
        return None;
    }
    Some(inner)
}

/// JSON path (`a.b.0.c`) 를 따라 값을 찾는다. 객체 key + 배열 인덱스 지원.
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

/// `${scope.path}` 참조를 해소한다. 미해소 시 `None`.
fn resolve_ref(reference: &str, ctx: &SubstitutionContext) -> Option<Value> {
    let (scope, path) = reference.split_once('.')?;
    match scope {
        "body" => resolve_json_path(&ctx.body, path).cloned(),
        // HTTP 헤더 이름은 대소문자 무시 — 소문자 정규화 후 조회.
        "header" => ctx
            .headers
            .get(&path.to_ascii_lowercase())
            .map(|s| Value::String(s.clone())),
        "query" => ctx.query.get(path).map(|s| Value::String(s.clone())),
        _ => None,
    }
}

/// 해소된 값을 문자열 임베드용으로 렌더. 문자열이면 그대로, 그 외는 JSON 표현.
fn render_embedded(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 문자열 leaf 치환. 전체가 단일 `${...}` 이면 해소된 **JSON 타입을 보존**하고,
/// 임베드(`"hello ${body.name}"`)면 텍스트로 치환한다.
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

/// params 템플릿에 페이로드를 치환한다.
///
/// **값 노드(leaf string)에만** `${...}` 를 해석한다. 객체 key 는 절대 건드리지
/// 않으며, method 는 이 함수에 인자로 넘어오지 않는다(데이터/흐름 분리 불변식).
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

/// IpcSequence 를 **fire-and-forget** 로 실행한다.
///
/// 각 스텝의 `method` 는 owner 가 고정한 리터럴(치환 대상 아님)이고, `params` 만
/// 페이로드로 치환한다. IPC 응답은 내부 로깅에만 쓰이고 반환하지 않는다 — 이
/// 함수가 `()` 를 반환하므로 실행 결과가 웹훅 ACK 로 샐 수 없다(단방향 불변식,
/// 이 불변식은 유지한다 — 반환값 추가는 하지 않는다).
///
/// MVP: 한 스텝이 실패해도 다음 스텝을 계속 진행한다(관측만). 조건분기는 후속.
///
/// 실패 로그는 `error!` — 예를 들어 이 스텝이 `agent.task_set_result`
/// 라면, 실패는 곧 "그 task 가 조용히 영원히 끝나지 않는다"는 뜻이다. 반환값이 없어
/// 호출자가 이 실패를 감지할 방법이 없으므로(단방향 불변식), 로그가 유일한 관측
/// 지점이다 — 일상적 경고(`warn!`)로는 운영 중 놓치기 쉽다. push 완료 전략의
/// 필수 timeout(§C-3, 레지스트리 쪽 트랙)이 이 실패로 인한 task 영구 hang 자체의
/// 안전망이고, 이 로그 레벨 변경은 그 안전망이 왜 발동했는지 진단 가능하게 한다.
///
/// 큐 입장 거절(호스트 명령 큐가 밀려 주입을 받지 않음)은 다른 실패와 **다른 문구**로
/// 남긴다 — 그 스텝은 실행되지 않았고(시간 초과와 달리 결과 불명이 아니다), 원인은 스텝이
/// 아니라 호스트의 적체다. 다시 걸지 않는다: 훅 스텝(웹훅 · surface 훅 · idle 훅)은 계속 오는 사건이라
/// 재시도가 곧 적체를 키우는 부하다. 다음 스텝은 그대로 진행한다(위 MVP 정책).
///
/// 스텝은 **기한 없이** 넣는다(`dispatch_even_if_abandoned`) — 스텝 상한에서 물러나도 명령은
/// 큐에 남아 나중에 실행된다. 이 함수를 지나는 훅 스텝 전부(웹훅 · surface 훅 — notification ·
/// bell · output-match · command-completed · process-exit · idle 훅 · 수동 발화)가 그렇다. 그 사건은
/// 다시 오지 않는다 — 웹훅은 밖에서 이미 ACK 됐고, surface 훅의 사건은 한 번 일어나고 끝난다. 그래서
/// 늦게라도 반영되는 쪽이 안 반영되는 쪽보다 낫다(`agent.task_set_result` 스텝이 버려지면 그 task 는
/// 끝나지 않는다). 근거: `docs/adr/0451-a-host-injection-carries-its-wait-as-a-deadline.md`.
///
/// 스텝 로그는 `origin` 을 머리에 단다 — 같은 함수를 세 출처가 부르므로, 없으면 실패한 스텝이 어느
/// 경로에서 왔는지 로그만으로 가를 수 없다.
pub fn execute_sequence(
    origin: SequenceOrigin,
    injector: &HostIpcInjector,
    calls: &[IpcCall],
    ctx: &SubstitutionContext,
) {
    for (i, call) in calls.iter().enumerate() {
        let params = substitute_params(&call.params, ctx);
        match injector.dispatch_even_if_abandoned(&call.method, params, STEP_TIMEOUT) {
            Ok(_result) => {
                tracing::debug!("{origin} IpcSequence step {i} ({}) ok", call.method);
            }
            Err(e) => log_step_failure(origin, i, &call.method, &e),
        }
    }
}

/// IpcSequence 를 누가 발화했는가 — 스텝 로그의 머리말이다. 바인딩 게이트의 입력인
/// [`super::types::TriggerSource`] 와 다르다: 수동 발화는 게이트를 거치지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceOrigin {
    /// 웹훅 리스너 — HTTP 요청마다 제 스레드에서 실행한다.
    Webhook,
    /// surface 훅(notification · bell · output-match · command-completed · process-exit · idle 훅).
    SurfaceHook,
    /// `hook_handler.dispatch` 수동 발화.
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

/// 스텝 실행기 스레드에 쌓여 있을 수 있는 시퀀스 수. 호스트 명령 큐가 받는 주입 명령 수의 상한
/// ([`tasty_ipc::admission::INJECTED_DEPTH_LIMIT`])에 묶는다 — 그것이 바뀌면 이 값도 따라 바뀐다.
/// 한 시퀀스는 스텝이 하나 이상이므로, 이 값이 그 상한 이상이면 호스트 큐가 받아 줬을 만큼의 폭주를
/// 이 자리에서 먼저 거절하지 않는다.
/// 근거: `docs/adr/0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md`.
const PENDING_SEQUENCE_LIMIT: usize = tasty_ipc::admission::INJECTED_DEPTH_LIMIT;

/// 실행기 스레드로 넘기는 시퀀스 한 건 — 제 injector 사본을 들고 간다.
struct SequenceJob {
    origin: SequenceOrigin,
    handler_id: String,
    injector: HostIpcInjector,
    calls: Vec<IpcCall>,
    ctx: SubstitutionContext,
}

/// 스텝 실행기 스레드의 입구. 처음 부를 때 스레드를 하나 띄운다. 못 띄웠으면 `None` 이다.
fn sequence_worker() -> Option<&'static SyncSender<SequenceJob>> {
    static WORKER: OnceLock<Option<SyncSender<SequenceJob>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = sync_channel::<SequenceJob>(PENDING_SEQUENCE_LIMIT);
            let spawned = std::thread::Builder::new()
                .name("hook-sequence".into())
                .spawn(move || {
                    for job in rx {
                        tracing::debug!("{} IpcSequence '{}' running", job.origin, job.handler_id);
                        execute_sequence(job.origin, &job.injector, &job.calls, &job.ctx);
                    }
                });
            match spawned {
                Ok(_) => Some(tx),
                Err(e) => {
                    tracing::error!("hook IpcSequence worker thread spawn failed: {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// IpcSequence 를 **호스트 명령 큐를 비우는 스레드 밖에서** 실행하도록 넘기고 곧바로 돌아온다.
///
/// surface 훅(notification · bell · output-match · command-completed · process-exit · idle 훅)은
/// 호스트 명령 큐를 비우는 바로 그 스레드(GUI 메인 스레드 · headless 루프)에서 발화한다. 거기서
/// [`execute_sequence`] 를 부르면 스텝이 넣은 명령을 꺼낼 스레드가 자기 자신이라 스텝마다 대기
/// 상한([`STEP_TIMEOUT`])까지 서고, 그동안 화면과 모든 IPC 응답이 선다.
///
/// 실행은 전용 스레드 하나가 **넘겨받은 순서대로** 한다 — 한 시퀀스의 스텝은 앞 스텝의 답을 받은 뒤
/// 다음 스텝을 넣고, 먼저 넘겨받은 시퀀스가 먼저 끝난다. surface 훅과 `hook_handler.dispatch` 수동
/// 발화가 이 한 줄을 함께 쓰므로 두 출처의 시퀀스도 스텝 단위로 끼어들지 않는다. 스텝 결과의 기록은
/// [`execute_sequence`] 그대로다(실패는 `error!`, 반환값 없음).
///
/// 실행기에 이미 [`PENDING_SEQUENCE_LIMIT`] 건이 쌓여 있으면 이 시퀀스는 **실행하지 않고** `Err` 를
/// 돌려준다 — 호스트 큐의 입장 거절과 같은 성질이다(스텝이 아니라 적체가 원인이고, 다시 걸지 않는다).
/// 호출자가 `error!` 로 남기고, 답할 호출자가 있는 자리(수동 발화)는 "실행하지 않았다" 로 답한다.
/// 근거: `docs/adr/0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md`,
/// `docs/adr/0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md`.
pub fn enqueue_sequence(
    origin: SequenceOrigin,
    handler_id: &str,
    injector: &HostIpcInjector,
    calls: &[IpcCall],
    ctx: SubstitutionContext,
) -> Result<(), SequenceNotQueued> {
    let worker = sequence_worker().ok_or(SequenceNotQueued::WorkerUnavailable)?;
    let job = SequenceJob {
        origin,
        handler_id: handler_id.to_string(),
        injector: injector.clone(),
        calls: calls.to_vec(),
        ctx,
    };
    worker.try_send(job).map_err(|e| match e {
        TrySendError::Full(_) => SequenceNotQueued::Full,
        TrySendError::Disconnected(_) => SequenceNotQueued::WorkerStopped,
    })
}

/// 실행기에 못 넘긴 시퀀스의 사유 — 그 시퀀스는 실행되지 않았다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceNotQueued {
    /// 대기 시퀀스가 [`PENDING_SEQUENCE_LIMIT`] 에 이르렀다.
    Full,
    /// 실행기 스레드를 못 띄웠다.
    WorkerUnavailable,
    /// 실행기 스레드가 멈췄다.
    WorkerStopped,
}

impl std::fmt::Display for SequenceNotQueued {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "{PENDING_SEQUENCE_LIMIT} sequences are already waiting"),
            Self::WorkerUnavailable => f.write_str("the sequence worker thread is unavailable"),
            Self::WorkerStopped => f.write_str("the sequence worker thread has stopped"),
        }
    }
}

/// 실패한 스텝 하나를 남긴다. 실행되지 않은 실패(큐 입장 거절 · 큐 송신 실패)는 문구를 갈라
/// 시간 초과·handler 거절과 구별되게 한다 — 앞의 것은 스텝이 아니라 호스트의 적체가 원인이다.
fn log_step_failure(origin: SequenceOrigin, i: usize, method: &str, e: &InjectError) {
    if e.nothing_ran() {
        tracing::error!(
            "{origin} IpcSequence step {i} ({method}) not run — the host did not queue it: {e}"
        );
    } else {
        tracing::error!("{origin} IpcSequence step {i} ({method}) failed: {e}");
    }
}

/// `ShellCommand` action 을 **fire-and-forget** 로 실행한다(worker thread spawn).
///
/// `hook_handler.dispatch` 가 셸 핸들러를 수동 발화할 때 쓴다. `tasty-hooks` 의
/// background spawn 을 미러링하되, 구조화된 `command` + `args` 를 셸 경유 없이 직접
/// exec 한다(인젝션 표면 축소). `env` 는 트리거 컨텍스트(`TASTY_HOOK_*`,
/// [`super::env::build_env`]) — 값 전달 전용이며 실행 대상(command)은 owner 소유라
/// 바꾸지 못한다. 실행 결과는 로깅에만 쓰이고 반환하지 않는다 — 응답 경로
/// (ACK/JSON)로 실행 결과가 새지 않는다(단방향 불변식).
pub fn spawn_shell(command: String, args: Vec<String>, env: Vec<(String, String)>) {
    if let Err(e) = std::thread::Builder::new()
        .name("hook-shell".into())
        .spawn(move || {
            let mut cmd = std::process::Command::new(&command);
            cmd.args(&args).envs(env);
            match tasty_utils::process::hide_console(&mut cmd).output() {
                Ok(_) => tracing::debug!("hook shell '{command}' ran"),
                Err(e) => tracing::warn!("hook shell '{command}' spawn failed: {e}"),
            }
        })
    {
        tracing::warn!("hook shell thread spawn failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 이 스코프 동안 이 스레드에서 나가는 tracing 이벤트를 문자열로 모은다.
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
        // 다른 스레드(다른 시험의 실행기 · 수동 발화)가 이 콜사이트를 구독자 없이 먼저 등록하면
        // interest 가 "never" 로 캐시되어 여기서 켠 구독자가 줄을 못 받는다(변이 실행 중 한 번 관측).
        // 구독자를 켠 뒤 캐시를 다시 짓는다.
        tracing::subscriber::with_default(sub, || {
            tracing::callsite::rebuild_interest_cache();
            f()
        });
        let out = buf.0.lock().unwrap().clone();
        String::from_utf8_lossy(&out).into_owned()
    }

    /// 스텝 로그는 시퀀스를 발화한 출처를 찍는다 — 성공한 스텝과 실패한 스텝 둘 다. 예전에는 세
    /// 출처가 모두 `webhook` 으로 찍혀 수동 발화 · surface 훅의 실패를 웹훅 실패로 읽게 했다.
    #[test]
    fn step_logs_name_the_origin_that_fired_the_sequence() {
        use std::sync::{Arc, mpsc};
        use tasty_ipc::server::IpcCommand;

        for (origin, label) in [
            (SequenceOrigin::Webhook, "webhook"),
            (SequenceOrigin::SurfaceHook, "surface hook"),
            (SequenceOrigin::Dispatch, "hook_handler.dispatch"),
        ] {
            let (tx, rx) = mpsc::channel::<IpcCommand>();
            let injector = HostIpcInjector::new(tx, Arc::new(|| {}));
            // 첫 스텝은 성공, 둘째 스텝은 handler 거절로 답한다.
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
            let logs = capture_logs(|| {
                execute_sequence(origin, &injector, &calls, &SubstitutionContext::default())
            });
            answerer.join().expect("answerer thread");
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
        // 전체가 단일 플레이스홀더 → JSON 타입 보존(숫자는 숫자로).
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
        // key 위치의 `${...}` 는 치환하지 않는다(데이터/흐름 분리).
        let out = substitute_params(&json!({"${body.message}": "v"}), &ctx());
        assert_eq!(out, json!({"${body.message}": "v"}));
    }

    /// 직접 exec 경로(`spawn_shell`)가 env 를 자식에 실제로 노출하는지 실 프로세스로
    /// 증명한다 — `hook_handler.dispatch` 셸 발화의 `TASTY_HOOK_*` 회귀 방어.
    #[test]
    fn spawn_shell_exposes_env_to_child() {
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("env.txt");
        // 직접 exec 은 셸 확장이 없으므로 셸 자체를 command 로 준다(공백 없는
        // temp 경로 — trigger.rs 인라인 테스트와 동일 근거로 인용부호 없음).
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
        );
        // **존재가 아니라 내용을 기다린다.** 리다이렉트(`> file`)는 파일을 먼저 만들고
        // 나중에 쓴다 — `exists()` 로 깨면 빈 파일을 읽는 창이 열린다. Windows 의 cmd 는
        // 그 창이 넓어 CI 에서 빈 문자열을 읽고 실패했다. 환경변수가 정말 안 넘어갔다면
        // 빈 값이 아니라 cmd 가 확장하지 못한 `%TASTY_HOOK_EVENT%` 원문이 남는다.
        // `trigger.rs` 의 `shell_binding_receives_command_completed_exit_code_env` 와 같은 대기다.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut content = String::new();
        while Instant::now() < deadline {
            content = std::fs::read_to_string(&marker).unwrap_or_default();
            if !content.trim().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        // 빈 값 하나로는 원인이 셋으로 갈린다 — 마커 존재 여부와 원문 여부를 함께 찍는다.
        assert_eq!(
            content.trim(),
            "user/envtest",
            "marker exists={} — false 면 셸이 안 떴다, 값이 %TASTY_HOOK_EVENT% 원문이면 \
             env 가 안 넘어갔다, true 인데 비어 있으면 5 초 안에 안 쓰였다",
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
        // agent-stream 턴 correlation 웹훅이 거는 시퀀스(문서 등록 예시와 같은 형태):
        // turn_start 의 request_id 와 claude.tell 의 message 만 body 에서 채워지고,
        // method·surface 같은 owner 고정 리터럴은 치환 대상이 아니다(ADR-0046 불변식 1).
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
        // 객체 key 위치의 `${...}` 는 치환되지 않는다(값 leaf 만).
        let key_shaped = substitute_params(&json!({"${body.request_id}": "x"}), &ctx);
        assert_eq!(
            key_shaped,
            json!({"${body.request_id}": "x"}),
            "key 위치는 절대 치환하지 않는다"
        );
    }
}
