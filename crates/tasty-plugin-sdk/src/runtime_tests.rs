//! `runtime_tests` 단위 테스트.
// 테스트 fixture 의 hook 캡처 타입이 깊게 중첩되지만 테스트 한정
// 가독성 문제라 alias 도입 가치가 낮다 — 파일 단위 허용
// (`docs/dev-guide/clippy-policy.md` 참고).
#![allow(clippy::type_complexity)]

use super::*;
use crate::error::PluginError;
use crate::plugin::{IpcMethodCtx, IpcMethodError, SurfaceResult};
use serde_json::json;
use std::io::Write;
use std::sync::{Arc, Mutex};

struct StubPlugin {
    last_ctx: Arc<Mutex<Option<IpcMethodCtx>>>,
    behavior: Behavior,
}

#[derive(Clone)]
enum Behavior {
    Ok(Value),
    Err(IpcMethodError),
}

impl Plugin for StubPlugin {
    fn id(&self) -> &str {
        "test.plugin"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        *self.last_ctx.lock().unwrap() = Some(ctx);
        match self.behavior.clone() {
            Behavior::Ok(v) => Ok(v),
            Behavior::Err(e) => Err(e),
        }
    }
}

struct DefaultPlugin;
impl Plugin for DefaultPlugin {
    fn id(&self) -> &str {
        "test.default"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
}

/// 테스트 전용 dummy HostHandle — 실제 호출하지 않고 ctx에 끼우기만 한다.
fn dummy_host() -> HostHandle {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind localhost");
    let port = listener.local_addr().unwrap().port();
    let accept = std::thread::spawn(move || {
        // 호출자(stream)와 대응하는 server 측 accept — 결과는 즉시 drop.
        let _accepted = listener.accept();
    });
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    accept.join().unwrap();
    let pending: PendingCalls = Arc::new(Mutex::new(HashMap::new()));
    HostHandle::new(Arc::new(Mutex::new(stream)), pending)
}

fn invoke_params(method: &str, params: Value, caller: Option<&str>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("method".into(), Value::String(method.into()));
    obj.insert("params".into(), params);
    if let Some(c) = caller {
        obj.insert("caller_plugin_id".into(), Value::String(c.into()));
    }
    Value::Object(obj)
}

#[test]
fn ipc_invoke_ok_serializes_result() {
    let last = Arc::new(Mutex::new(None));
    let mut plugin = StubPlugin {
        last_ctx: last.clone(),
        behavior: Behavior::Ok(json!({"ok": true, "n": 42})),
    };
    let host = dummy_host();
    let params = invoke_params("codex.spawn", json!({"cwd": "/tmp"}), None);
    let resp = build_response(7, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params, &host));
    assert_eq!(resp.id, 7);
    assert!(resp.error.is_none());
    assert_eq!(resp.result, Some(json!({"ok": true, "n": 42})));

    let ctx = last.lock().unwrap().clone().unwrap();
    assert_eq!(ctx.method, "codex.spawn");
    assert_eq!(ctx.params, json!({"cwd": "/tmp"}));
    assert_eq!(ctx.caller_plugin_id, None);
}

#[test]
fn ipc_invoke_not_found_carries_error_code() {
    let last = Arc::new(Mutex::new(None));
    let mut plugin = StubPlugin {
        last_ctx: last.clone(),
        behavior: Behavior::Err(IpcMethodError::not_found("codex.bogus")),
    };
    let host = dummy_host();
    let params = invoke_params("codex.bogus", json!({}), None);
    let resp = build_response(11, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params, &host));
    assert_eq!(resp.id, 11);
    assert!(resp.result.is_none());
    assert_eq!(resp.error_code, Some(-32601));
}

#[test]
fn ipc_invoke_passes_caller_plugin_id() {
    let last = Arc::new(Mutex::new(None));
    let mut plugin = StubPlugin {
        last_ctx: last.clone(),
        behavior: Behavior::Ok(Value::Null),
    };
    let host = dummy_host();
    let params = invoke_params("codex.spawn", json!({}), Some("com.other.plugin"));
    build_response(1, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params, &host));
    let ctx = last.lock().unwrap().clone().unwrap();
    assert_eq!(ctx.caller_plugin_id.as_deref(), Some("com.other.plugin"));

    let params2 = invoke_params("codex.spawn", json!({}), None);
    build_response(2, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params2, &host));
    let ctx2 = last.lock().unwrap().clone().unwrap();
    assert_eq!(ctx2.caller_plugin_id, None);
}

#[test]
fn ipc_invoke_default_impl_returns_not_implemented() {
    let mut plugin = DefaultPlugin;
    let host = dummy_host();
    let params = invoke_params("codex.spawn", json!({}), None);
    let resp = build_response(3, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params, &host));
    assert_eq!(resp.error_code, Some(-32601));
    assert!(resp.error.unwrap().contains("not implemented"));
}

#[test]
fn ipc_invoke_invalid_params_returns_minus_32602() {
    let mut plugin = DefaultPlugin;
    let host = dummy_host();
    let params = json!({"params": {}});
    let resp = build_response(4, dispatch(&mut plugin, METHOD_IPC_INVOKE, &params, &host));
    assert_eq!(resp.error_code, Some(-32602));
}

#[test]
fn unknown_method_returns_minus_32601() {
    let mut plugin = DefaultPlugin;
    let host = dummy_host();
    let resp = build_response(5, dispatch(&mut plugin, "nonsense", &Value::Null, &host));
    assert_eq!(resp.error_code, Some(-32601));
}

struct OnStartRecorder {
    called: Arc<Mutex<u32>>,
}
impl Plugin for OnStartRecorder {
    fn id(&self) -> &str {
        "test.on_start"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn on_start(&mut self, _host: HostHandle, _bus: crate::bus::BusHandle) {
        *self.called.lock().unwrap() += 1;
    }
}

struct ExtensionStubPlugin {
    last_ctx: Arc<
        Mutex<
            Option<(
                tasty_plugin_protocol::ExtensionHookKind,
                tasty_plugin_protocol::ExtensionHookPhase,
                tasty_plugin_protocol::ExtensionHookMode,
                String,
                Value,
            )>,
        >,
    >,
    outcome: crate::plugin::ExtensionHookOutcome,
}
impl Plugin for ExtensionStubPlugin {
    fn id(&self) -> &str {
        "test.extension"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn handle_extension_hook(
        &mut self,
        ctx: crate::plugin::ExtensionHookCtx,
    ) -> crate::plugin::ExtensionHookOutcome {
        *self.last_ctx.lock().unwrap() =
            Some((ctx.kind, ctx.phase, ctx.mode, ctx.target, ctx.payload));
        self.outcome.clone()
    }
}

#[test]
fn extension_invoke_hook_transform_returns_modified_payload() {
    let last = Arc::new(Mutex::new(None));
    let mut plugin = ExtensionStubPlugin {
        last_ctx: last.clone(),
        outcome: crate::plugin::ExtensionHookOutcome::transformed(json!({"x": 99})),
    };
    let host = dummy_host();
    let params = json!({
        "kind": "ipc",
        "phase": "pre",
        "mode": "transform",
        "target": "com.target/method.foo",
        "payload": {"x": 1},
    });
    let resp = build_response(
        21,
        dispatch(
            &mut plugin,
            tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK,
            &params,
            &host,
        ),
    );
    assert_eq!(resp.id, 21);
    assert!(resp.error.is_none());
    let result = resp.result.expect("result present");
    assert_eq!(result.get("modified_payload"), Some(&json!({"x": 99})));
    assert!(result.get("pass").is_none() || result.get("pass") == Some(&Value::Null));

    let (kind, phase, mode, target, payload) = last.lock().unwrap().clone().unwrap();
    assert_eq!(kind, tasty_plugin_protocol::ExtensionHookKind::Ipc);
    assert_eq!(phase, tasty_plugin_protocol::ExtensionHookPhase::Pre);
    assert_eq!(mode, tasty_plugin_protocol::ExtensionHookMode::Transform);
    assert_eq!(target, "com.target/method.foo");
    assert_eq!(payload, json!({"x": 1}));
}

#[test]
fn extension_invoke_hook_filter_block_returns_pass_false() {
    let last = Arc::new(Mutex::new(None));
    let mut plugin = ExtensionStubPlugin {
        last_ctx: last.clone(),
        outcome: crate::plugin::ExtensionHookOutcome::block(),
    };
    let host = dummy_host();
    let params = json!({
        "kind": "event",
        "phase": "pre",
        "mode": "filter",
        "target": "com.target/event.bar",
        "payload": {},
    });
    let resp = build_response(
        22,
        dispatch(
            &mut plugin,
            tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK,
            &params,
            &host,
        ),
    );
    let result = resp.result.expect("result present");
    assert_eq!(result.get("pass"), Some(&Value::Bool(false)));
}

#[test]
fn extension_invoke_hook_default_impl_returns_pass() {
    let mut plugin = DefaultPlugin;
    let host = dummy_host();
    let params = json!({
        "kind": "ipc",
        "phase": "post",
        "mode": "observe",
        "target": "com.target/method.foo",
        "payload": {},
    });
    let resp = build_response(
        23,
        dispatch(
            &mut plugin,
            tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK,
            &params,
            &host,
        ),
    );
    assert!(resp.error.is_none());
    let result = resp.result.expect("result present");
    // pass() = default Outcome → both fields skipped from serialization.
    assert!(result.as_object().unwrap().is_empty());
}

#[test]
fn extension_invoke_hook_invalid_params_returns_minus_32602() {
    let mut plugin = DefaultPlugin;
    let host = dummy_host();
    let params = json!({"kind": "ipc"}); // missing required fields
    let resp = build_response(
        24,
        dispatch(
            &mut plugin,
            tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK,
            &params,
            &host,
        ),
    );
    assert_eq!(resp.error_code, Some(-32602));
}

/// worker_loop이 dispatch 전에 on_start를 정확히 1회 호출해야 한다.
/// req_rx를 닫아 worker가 즉시 종료하면 on_start만 실행되고 끝.
#[test]
fn worker_loop_invokes_on_start_once_before_dispatch() {
    let called = Arc::new(Mutex::new(0u32));
    let plugin = OnStartRecorder {
        called: called.clone(),
    };
    let host = dummy_host();
    // dummy writer: 어디로도 안 가는 TcpStream 페어
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept = std::thread::spawn(move || {
        let _accepted = listener.accept();
    });
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    accept.join().unwrap();
    let writer = Arc::new(Mutex::new(stream));

    let (tx, rx) = mpsc::channel::<WorkerItem>();
    drop(tx); // queue 즉시 닫기 — worker_loop은 on_start 후 iter()로 빠져나간다.
    let join = std::thread::spawn(move || {
        worker_loop(plugin, rx, writer, host);
    });
    join.join().unwrap();
    assert_eq!(*called.lock().unwrap(), 1);
}

/// self-invoke 는 host 왕복(`ipc.invoke` 프레이밍) 없이 `handle_ipc_method` 로 직접
/// 라우팅되어야 한다 — plugin 자신의 네임스페이스 메서드를 `HostHandle::call`로
/// 부르면 host 의 self-call 미forward 정책 때문에 `-32601`이 나는 문제(`file_watch` 의
/// idle auto-reload 회귀)의 재발 방지 테스트. `caller_plugin_id`는 plugin 자신의
/// id가 채워진다.
#[test]
fn worker_loop_self_invoke_routes_to_handle_ipc_method_directly() {
    let last = Arc::new(Mutex::new(None));
    let plugin = StubPlugin {
        last_ctx: last.clone(),
        behavior: Behavior::Ok(json!({"ok": true})),
    };
    let host = dummy_host();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept = std::thread::spawn(move || {
        let _accepted = listener.accept();
    });
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    accept.join().unwrap();
    let writer = Arc::new(Mutex::new(stream));

    let (tx, rx) = mpsc::channel::<WorkerItem>();
    tx.send(WorkerItem::SelfInvoke {
        method: "markdown.reload".to_string(),
        params: json!({ "surface": 11 }),
    })
    .unwrap();
    drop(tx); // queue 닫기 — worker_loop 이 처리 후 iter()로 빠져나온다.

    let join = std::thread::spawn(move || {
        worker_loop(plugin, rx, writer, host);
    });
    join.join().unwrap();

    let ctx = last
        .lock()
        .unwrap()
        .take()
        .expect("handle_ipc_method should have been invoked by the self-invoke item");
    assert_eq!(ctx.method, "markdown.reload");
    assert_eq!(ctx.params, json!({ "surface": 11 }));
    assert_eq!(ctx.caller_plugin_id.as_deref(), Some("test.plugin"));
}

/// popup.open / popup.closed 라우팅과 콜백 호출 검증.
struct PopupStubPlugin {
    opened: Arc<Mutex<Vec<(String, u64, Value)>>>,
    closed: Arc<Mutex<Vec<(u64, tasty_plugin_protocol::PopupCloseReason)>>>,
}

impl Plugin for PopupStubPlugin {
    fn id(&self) -> &str {
        "test.popup"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn open_popup(
        &mut self,
        ctx: crate::plugin::PopupOpenCtx,
    ) -> tasty_plugin_protocol::PopupOpenResult {
        self.opened
            .lock()
            .unwrap()
            .push((ctx.popup_id, ctx.instance_id, ctx.context));
        tasty_plugin_protocol::PopupOpenResult::default()
    }
    fn on_popup_closed(&mut self, ctx: crate::plugin::PopupClosedCtx) {
        self.closed
            .lock()
            .unwrap()
            .push((ctx.instance_id, ctx.reason));
    }
}

fn make_popup_plugin() -> PopupStubPlugin {
    PopupStubPlugin {
        opened: Arc::new(Mutex::new(Vec::new())),
        closed: Arc::new(Mutex::new(Vec::new())),
    }
}

#[test]
fn popup_open_calls_plugin_with_ctx() {
    let mut plugin = make_popup_plugin();
    let opened = plugin.opened.clone();
    let host = dummy_host();
    let params = json!({
        "popup_id": "search",
        "instance_id": 7,
        "context": {"q": "abc"},
    });
    let resp = build_response(1, dispatch(&mut plugin, METHOD_POPUP_OPEN, &params, &host));
    assert!(resp.error.is_none(), "got error: {:?}", resp.error);

    let opened = opened.lock().unwrap();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].0, "search");
    assert_eq!(opened[0].1, 7);
    assert_eq!(opened[0].2["q"], "abc");
}

#[test]
fn popup_closed_dispatch_invokes_callback() {
    let mut plugin = make_popup_plugin();
    let closed = plugin.closed.clone();
    let host = dummy_host();
    let params = json!({"instance_id": 5, "reason": "outside_click"});
    let resp = build_response(
        3,
        dispatch(&mut plugin, METHOD_POPUP_CLOSED, &params, &host),
    );
    assert!(resp.error.is_none(), "got error: {:?}", resp.error);
    assert_eq!(resp.result, Some(Value::Null));

    let c = closed.lock().unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].0, 5);
    assert_eq!(
        c[0].1,
        tasty_plugin_protocol::PopupCloseReason::OutsideClick
    );
}

/// host 가 포화로 버린 요청 수를 받아 두는 plugin.
struct DropRecorder {
    seen: Arc<Mutex<Vec<u64>>>,
}

impl Plugin for DropRecorder {
    fn id(&self) -> &str {
        "test.drops"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn on_host_dropped_requests(&mut self, dropped: u64) {
        self.seen.lock().unwrap().push(dropped);
    }
}

/// host 가 버린 수는 **dispatch 직전에 한 번만** plugin 에게 전달돼야 한다.
///
/// reader 스레드는 그 수를 `HostHandle` 에 쌓아 두기만 한다 — `&mut plugin` 을 쥔
/// 스레드가 worker 하나뿐이라 꺼내는 자리도 거기다. 꺼낸 뒤 0 이 되지 않으면 같은
/// 수가 요청마다 다시 보고되어, plugin 이 보는 값이 실제로 버려진 수가 아니게 된다.
#[test]
fn worker_loop_reports_host_drops_once_before_dispatch() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let plugin = DropRecorder { seen: seen.clone() };
    let host = dummy_host();
    host.record_dropped_by_host(3);

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept = std::thread::spawn(move || listener.accept().map(|(s, _)| s));
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    // 응답을 쓸 상대를 살려 둔다 — 끊긴 소켓이면 dispatch 의 write 가 먼저 죽는다.
    let _peer = accept.join().unwrap().expect("accept");
    let writer = Arc::new(Mutex::new(stream));

    let (tx, rx) = mpsc::channel::<WorkerItem>();
    for id in 1..=2 {
        tx.send(WorkerItem::Host(PluginRequest::new(
            "test.unknown",
            json!({}),
            id,
        )))
        .unwrap();
    }
    drop(tx);

    let join = std::thread::spawn(move || {
        worker_loop(plugin, rx, writer, host);
    });
    join.join().unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        vec![3],
        "버린 수는 첫 dispatch 직전에 한 번만 보고돼야 한다"
    );
}

/// `on_start` 로 받은 `HostHandle` 을 쥐고 있는 plugin — 번들 plugin 들이 백그라운드
/// 스레드에 host 를 넘겨 두는 형태를 흉내 낸다. 그 클론이 self-invoke sender 를 함께 쥔다.
struct HostKeeper {
    kept: Option<HostHandle>,
}

impl Plugin for HostKeeper {
    fn id(&self) -> &str {
        "test.keeper"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn on_start(&mut self, host: HostHandle, _bus: crate::bus::BusHandle) {
        self.kept = Some(host);
    }
}

/// 인증까지 받아 준 가짜 호스트 쪽 소켓과, 그 호스트에 붙은 `run_with_env` 의 종료 통지.
fn run_against_fake_host() -> (
    TcpStream,
    std::io::BufReader<TcpStream>,
    mpsc::Receiver<std::result::Result<(), String>>,
) {
    run_plugin_against_fake_host(HostKeeper { kept: None })
}

/// [`run_against_fake_host`] 의 본체 — 붙일 plugin 을 받는다.
fn run_plugin_against_fake_host<P: Plugin + Send + 'static>(
    plugin: P,
) -> (
    TcpStream,
    std::io::BufReader<TcpStream>,
    mpsc::Receiver<std::result::Result<(), String>>,
) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let env = PluginEnv {
        plugin_id: "test.keeper".into(),
        host_port: port,
        token: "t".into(),
        host_api_version: "1".into(),
        plugin_dir: None,
        data_dir: None,
        config_path: None,
        log_path: None,
        locale: "en".into(),
        locale_font: None,
        handle_endpoint: None,
    };
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = run_with_env(plugin, &env).map_err(|e| e.to_string());
        if done_tx.send(r).is_err() {
            tracing::debug!("test already gave up waiting for run_with_env");
        }
    });
    let (mut stream, _) = listener.accept().unwrap();
    let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap(); // AuthMessage
    writeln!(stream, "{{\"auth_ack\":{{\"ok\":true}}}}").unwrap();
    stream.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap(); // hello event
    assert!(line.contains("hello"), "expected hello, got {line}");
    (stream, reader, done_rx)
}

/// shutdown 요청을 받으면 `run` 이 돌아와야 한다 — 호스트는 프로세스 종료를 2 s 기다린 뒤
/// 강제 종료한다. worker 큐를 닫는 것만으로는 worker 가 안 끝난다: plugin 이 쥔
/// `HostHandle` 이 self-invoke sender 를 들고 있어서다(`WorkerItem::Stop` 문서).
#[test]
fn run_returns_after_shutdown_even_when_plugin_keeps_host_handle() {
    let (mut stream, mut reader, done_rx) = run_against_fake_host();
    let req = PluginRequest::new(METHOD_SHUTDOWN, json!({}), 99);
    writeln!(stream, "{}", serde_json::to_string(&req).unwrap()).unwrap();
    stream.flush().unwrap();
    let mut ack = String::new();
    reader.read_line(&mut ack).unwrap();
    assert!(
        ack.contains("\"id\":99"),
        "shutdown ack expected, got {ack}"
    );
    // 호스트는 소켓을 쥔 채 프로세스 종료를 기다린다 — 연결을 닫아 주지 않는다.
    let outcome = done_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("run did not return within 1 s of the shutdown request");
    assert_eq!(outcome, Ok(()));
    drop(stream);
}

/// 호스트가 연결을 닫아도 같은 이유로 `run` 이 돌아와야 한다.
#[test]
fn run_returns_after_host_closes_even_when_plugin_keeps_host_handle() {
    let (stream, reader, done_rx) = run_against_fake_host();
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    drop(reader);
    drop(stream);
    let outcome = done_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("run did not return within 1 s of the host closing the connection");
    assert_eq!(outcome, Ok(()));
}

/// ipc 메서드를 받으면 host 를 한 번 부르고, 그 결과(에러 표시)를 시험에 넘기는 plugin.
struct HostCaller {
    outcomes: mpsc::Sender<std::result::Result<Value, String>>,
}

impl Plugin for HostCaller {
    fn id(&self) -> &str {
        "test.caller"
    }
    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        SurfaceResult::default()
    }
    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        let outcome = ctx
            .host
            .call("host.anything", json!({}))
            .map_err(|e| e.to_string());
        if self.outcomes.send(outcome).is_err() {
            tracing::debug!("test stopped listening for host call outcomes");
        }
        Ok(Value::Null)
    }
}

fn send_line(stream: &mut TcpStream, req: &PluginRequest) {
    writeln!(stream, "{}", serde_json::to_string(req).unwrap()).unwrap();
    stream.flush().unwrap();
}

/// shutdown 앞에 쌓인 요청이 host 를 부르면, 결과를 읽어 줄 recv 루프가 이미 끝나
/// 있다. 그 call 이 [`HostHandle::timeout`](60 s)까지 서면 `run` 도 서고 호스트가
/// 2 s 뒤 강제 종료한다 — 그러지 말고 "호스트가 떠났다" 로 곧바로 돌아와야 한다.
///
/// 두 갈래를 함께 본다: 루프가 끝날 때 **이미 기다리던** call(첫 요청)과, 끝난 **뒤에**
/// 시작한 call(둘째 요청 — worker 가 Stop 앞에서 처리한다).
#[test]
fn host_calls_left_at_shutdown_fail_fast_instead_of_timing_out() {
    let (outcome_tx, outcome_rx) = mpsc::channel();
    let (mut stream, mut reader, done_rx) = run_plugin_against_fake_host(HostCaller {
        outcomes: outcome_tx,
    });
    send_line(
        &mut stream,
        &PluginRequest::new(
            METHOD_IPC_INVOKE,
            invoke_params("test.caller.first", json!({}), None),
            1,
        ),
    );
    // worker 가 host 를 불렀다(IpcCall 이 나왔다) — 이 call 은 이제 결과를 기다린다.
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("ipc_call"),
        "expected the host call, got {line}"
    );
    send_line(
        &mut stream,
        &PluginRequest::new(
            METHOD_IPC_INVOKE,
            invoke_params("test.caller.second", json!({}), None),
            2,
        ),
    );
    send_line(
        &mut stream,
        &PluginRequest::new(METHOD_SHUTDOWN, json!({}), 3),
    );

    let wait = std::time::Duration::from_secs(1);
    for which in [
        "first (waiting at shutdown)",
        "second (started after shutdown)",
    ] {
        let outcome = outcome_rx
            .recv_timeout(wait)
            .unwrap_or_else(|_| panic!("the {which} host call did not return within 1 s"));
        assert_eq!(
            outcome,
            Err(PluginError::HostClosed.to_string()),
            "the {which} host call should report that the host left"
        );
    }
    let outcome = done_rx
        .recv_timeout(wait)
        .expect("run did not return within 1 s of the shutdown request");
    assert_eq!(outcome, Ok(()));
    drop(stream);
}

/// 요청 한 줄이 read timeout(50 ms)보다 긴 간격으로 조각나 도착해도 처리돼야 한다.
/// 자르는 자리는 멀티바이트 문자 한가운데 — `read_line` 이라면 앞 조각이 UTF-8 검사로
/// 지워지는 갈래다. 줄이 깨지면 요청이 버려져 응답이 오지 않는다.
#[test]
fn run_handles_a_request_line_split_across_read_timeouts() {
    let (mut stream, mut reader, done_rx) = run_against_fake_host();
    let req = PluginRequest::new(METHOD_PING, json!({ "note": "한글 조각" }), 77);
    let bytes = format!("{}\n", serde_json::to_string(&req).unwrap()).into_bytes();
    let cut = bytes
        .iter()
        .position(|b| *b >= 0x80)
        .expect("the line carries a multibyte character")
        + 1;
    stream.write_all(&bytes[..cut]).unwrap();
    stream.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    stream.write_all(&bytes[cut..]).unwrap();
    stream.flush().unwrap();

    reader
        .get_ref()
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut resp = String::new();
    reader
        .read_line(&mut resp)
        .expect("no response to the split request within 2 s");
    assert!(
        resp.contains("\"id\":77"),
        "response to the split request expected, got {resp}"
    );

    send_line(
        &mut stream,
        &PluginRequest::new(METHOD_SHUTDOWN, json!({}), 78),
    );
    let outcome = done_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("run did not return after shutdown");
    assert_eq!(outcome, Ok(()));
}
