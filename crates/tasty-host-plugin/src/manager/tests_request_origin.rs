//! 플러그인 전달과 pre/post hook에 원 IPC 요청 번호를 유지하는지 확인한다.
//! 단계별 요청 id가 달라도 호스트 처리와 각 대기 시간을 같은 요청에 기록해야 한다.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tasty_ipc::server::RequestSeq;
use tasty_plugin_manifest::{HookMode, IpcHookDecl};
use tasty_terminal::waker_factory::NoopWakerFactory;

use super::{FinalCaller, PendingRequest, PendingRequestKind, PluginManager};
use crate::PluginPackage;
use crate::process::PluginProcess;
use crate::protocol::PluginResponse;

const OWNER: &str = "com.example.owner";
const EXT: &str = "com.example.ext";

fn mgr_owning(prefix: &str) -> PluginManager {
    let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
        r#"
manifest_version = 1
id = "{OWNER}"
name = "Origin Fixture"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[[contributes.ipc_namespace]]
prefix = "{prefix}"
"#
    ))
    .expect("fixture manifest should parse");
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![PluginPackage {
        dir: PathBuf::from("/nonexistent/origin_fixture"),
        manifest,
    }]);
    mgr
}

fn ok(id: u64) -> PluginResponse {
    PluginResponse {
        id,
        result: Some(serde_json::json!({})),
        error: None,
        error_code: None,
    }
}

/// 대상 플러그인 to에 보낸 대기 항목의 (req_id, origin).
fn pending_to(mgr: &PluginManager, to: &str) -> (u64, Option<RequestSeq>) {
    let found: Vec<_> = mgr
        .pending_requests
        .iter()
        .filter(|(_, p)| p.to == to)
        .map(|(id, p)| (*id, p.origin))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{to} 앞 대기 항목이 하나여야 한다: {found:?}"
    );
    found[0]
}

/// IPC 요청을 plugin namespace 로 넘기면 그 대기 항목이 원 요청의 번호를 든다. 번호가 없는
/// 호출(IPC 큐를 안 지난 것)은 `None` 이다.
#[test]
fn a_forward_puts_the_original_request_seq_on_its_pending_entry() {
    let mut mgr = mgr_owning("orig");
    let (owner, owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    mgr.processes.insert(OWNER.into(), owner);
    let seq = RequestSeq::next();
    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::json!(1),
        tx,
        Some(seq),
    );
    let (req_id, origin) = pending_to(&mgr, OWNER);
    assert_eq!(origin, Some(seq));
    let sent = owner_rx
        .try_recv()
        .expect("target 에 ipc.invoke 가 가야 한다");
    assert_eq!(sent.id, req_id, "plugin 에게 가는 것은 호스트 req_id 다");
    let mut keys: Vec<&str> = sent
        .params
        .as_object()
        .expect("ipc.invoke params 는 객체다")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["caller_plugin_id", "method", "params"],
        "원 요청 번호는 plugin wire 에 싣지 않는다(docs/architecture/ipc-server.md#느린-요청-추적)"
    );

    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.handle_plugin_response(OWNER, ok(req_id));
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::Value::Null,
        tx,
        None,
    );
    assert_eq!(pending_to(&mgr, OWNER).1, None);
}

/// `orig.run` 에 제한 `timeout_ms` 의 pre-hook 을 선언한 확장이 붙은 매니저. 두 plugin 의 요청 수신 통로를 함께 준다.
fn mgr_with_pre_hook(
    timeout_ms: u64,
) -> (
    PluginManager,
    crate::process::RequestTap,
    crate::process::RequestTap,
) {
    let mut mgr = mgr_owning("orig");
    let ext: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
        r#"
manifest_version = 1
id = "{EXT}"
name = "Origin Extension Fixture"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[extends]
plugin_id = "{OWNER}"
version_req = ">=0.0.1"
api_version = "1"

[[extends.pre_ipc]]
method = "orig.run"
mode = "observe"
timeout_ms = {timeout_ms}
"#
    ))
    .expect("extension fixture manifest should parse");
    mgr.packages.push(PluginPackage {
        dir: PathBuf::from("/nonexistent/origin_ext_fixture"),
        manifest: ext,
    });
    let (owner, owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    let (ext_proc, ext_rx) = PluginProcess::stub_with_request_rx(EXT);
    mgr.processes.insert(OWNER.into(), owner);
    mgr.processes.insert(EXT.into(), ext_proc);
    (mgr, owner_rx, ext_rx)
}

/// pre-hook으로 먼저 전달해도 대기 항목은 원 요청 번호를 유지한다.
#[test]
fn a_forward_through_a_pre_hook_puts_the_request_seq_on_the_hook_entry() {
    let (mut mgr, _owner_rx, ext_rx) = mgr_with_pre_hook(60_000);
    let seq = RequestSeq::next();
    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::json!(1),
        tx,
        Some(seq),
    );
    let hook = ext_rx
        .try_recv()
        .expect("확장에 pre-hook 호출이 먼저 가야 한다");
    let (req_id, origin) = pending_to(&mgr, EXT);
    assert_eq!(hook.id, req_id);
    assert_eq!(origin, Some(seq), "pre-hook hop 이 번호를 잃었다");
}

/// 빠른 pre-hook 뒤 느린 target 응답까지 같은 요청에 기록해야 한다.
#[test]
fn a_fast_pre_hook_and_its_slow_target_land_on_one_row_in_order() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, _owner_rx, _ext_rx) = mgr_with_pre_hook(60_000);
    mgr.set_slow_requests(log.clone());
    let seq = RequestSeq::next();
    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::json!(1),
        tx,
        Some(seq),
    );
    log.finish_host(fast_host_leg(seq));
    let (hook_id, _) = pending_to(&mgr, EXT);
    mgr.handle_plugin_response(EXT, ok(hook_id));
    assert!(
        log.snapshot().rows.is_empty(),
        "빠른 pre-hook만 완료했을 때는 요청 로그가 확정되지 않아야 한다"
    );
    let (target_id, _) = pending_to(&mgr, OWNER);
    if let Some(p) = mgr.pending_requests.get_mut(&target_id) {
        p.sent_at = Instant::now() - Duration::from_millis(150);
    }
    mgr.handle_plugin_response(OWNER, ok(target_id));

    let rows = log.snapshot().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].host.is_some(),
        "호스트 처리와 플러그인 대기가 같은 요청에 기록되지 않았다: {rows:?}"
    );
    let hops: Vec<_> = rows[0]
        .plugin_hops
        .iter()
        .map(|h| (h.plugin_id.as_str(), h.host_request_id))
        .collect();
    assert_eq!(hops, [(EXT, hook_id), (OWNER, target_id)]);
}

/// pre-hook → target → post-hook의 각 대기 항목이 같은 원 요청 번호를 유지한다.
#[test]
fn every_hop_of_a_hook_chain_carries_the_same_request_seq() {
    let mut mgr = mgr_owning("orig");
    let (owner, _owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    let (ext, _ext_rx) = PluginProcess::stub_with_request_rx(EXT);
    mgr.processes.insert(OWNER.into(), owner);
    mgr.processes.insert(EXT.into(), ext);
    let seq = RequestSeq::next();
    let decl = IpcHookDecl {
        method: "orig.run".into(),
        modifies: Vec::new(),
        mode: HookMode::Observe,
        timeout_ms: 60_000,
    };
    let (tx, _caller_rx) = mpsc::sync_channel(4);
    mgr.pending_requests.insert(
        70,
        PendingRequest::now(
            EXT,
            PendingRequestKind::ExtensionPreIpcHook {
                target_plugin_id: OWNER.into(),
                extension_plugin_id: EXT.into(),
                method: "orig.run".into(),
                params: serde_json::json!({}),
                pre_hook_mode: HookMode::Observe,
                final_caller: FinalCaller::Local {
                    response_tx: tx,
                    original_id: serde_json::json!(1),
                    origin: Some(seq),
                },
                post_hook: Some(decl),
                deadline: Instant::now() + Duration::from_secs(60),
            },
        )
        .for_request(Some(seq)),
    );

    mgr.handle_plugin_response(EXT, ok(70));
    let (target_id, origin) = pending_to(&mgr, OWNER);
    assert_eq!(origin, Some(seq), "pre-hook → target hop 이 번호를 잃었다");

    mgr.handle_plugin_response(OWNER, ok(target_id));
    let (post_id, origin) = pending_to(&mgr, EXT);
    assert_ne!(post_id, 70);
    assert_eq!(origin, Some(seq), "target → post-hook hop 이 번호를 잃었다");
}

/// 호스트의 빠른 dispatch 처리 시간을 기록한다.
fn fast_host_leg(seq: RequestSeq) -> tasty_telemetry::slow_requests::HostLeg<'static> {
    tasty_telemetry::slow_requests::HostLeg {
        request_seq: seq.get(),
        method: "orig.run",
        caller: tasty_telemetry::slow_requests::CallerKind::Local,
        queue_wait: Duration::from_micros(10),
        host: Duration::from_micros(10),
        outcome: Default::default(),
    }
}

fn forwarded(
    log: &Arc<tasty_telemetry::SlowRequestLog>,
) -> (
    PluginManager,
    RequestSeq,
    mpsc::Receiver<tasty_ipc::protocol::JsonRpcResponse>,
) {
    let mut mgr = mgr_owning("orig");
    mgr.set_slow_requests(log.clone());
    let (owner, _owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    mgr.processes.insert(OWNER.into(), owner);
    let seq = RequestSeq::next();
    let (tx, rx) = mpsc::sync_channel(1);
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::json!(1),
        tx,
        Some(seq),
    );
    // 호스트 dispatch가 빨리 끝난 경우를 만든다.
    log.finish_host(fast_host_leg(seq));
    assert!(
        log.snapshot().rows.is_empty(),
        "호스트 처리 시간만으로는 느린 요청 기준을 넘지 않는다"
    );
    (mgr, seq, rx)
}

/// 늦은 플러그인 응답의 대기 시간을 원 요청에 기록한다.
/// 단계별 req_id는 플러그인이 받은 JSON-RPC id와 같아 로그를 연결할 수 있다.
#[test]
fn a_slow_answer_lands_on_the_original_requests_row() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, seq, _rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    // 실제 대기 대신 전송 시각을 조정해 실행 부하의 영향을 줄인다.
    if let Some(p) = mgr.pending_requests.get_mut(&req_id) {
        p.sent_at = Instant::now() - Duration::from_millis(150);
    }
    mgr.handle_plugin_response(OWNER, ok(req_id));
    let rows = log.snapshot().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].request_seq, seq.get());
    assert!(
        rows[0].host.is_some(),
        "호스트 처리와 플러그인 대기가 같은 요청에 기록되지 않았다"
    );
    let hop = &rows[0].plugin_hops[0];
    assert_eq!(
        (hop.plugin_id.as_str(), hop.host_request_id, hop.outcome),
        (
            OWNER,
            req_id,
            tasty_telemetry::slow_requests::HopOutcome::Ok
        )
    );
    assert!(hop.wait_us >= 150_000, "{hop:?}");
}

/// 만료된 호출은 왕복 시간 분포에서 제외하지만 요청 로그에는 expired로 남긴다.
#[test]
fn an_expired_forward_lands_on_the_row_as_expired() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, seq, rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    mgr.sweep_expired_requests(Instant::now() + Duration::from_secs(3600));
    assert_eq!(
        rx.try_recv()
            .expect("만료가 caller 에 답한다")
            .error
            .map(|e| e.code),
        Some(-32004)
    );
    let rows = log.snapshot().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].request_seq, seq.get());
    assert_eq!(rows[0].plugin_hops[0].host_request_id, req_id);
    assert_eq!(
        rows[0].plugin_hops[0].outcome,
        tasty_telemetry::slow_requests::HopOutcome::Expired
    );
}

/// 빠른 요청은 로그에 남기지 않는다.
#[test]
fn a_fast_answer_leaves_no_row() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, _seq, _rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    mgr.handle_plugin_response(OWNER, ok(req_id));
    assert!(log.snapshot().rows.is_empty());
}

/// 플러그인이 제거되면 대기하던 요청에 cancelled를 기록한다.
#[test]
fn a_forward_cancelled_by_plugin_removal_lands_as_cancelled() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, seq, _rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    if let Some(p) = mgr.pending_requests.get_mut(&req_id) {
        p.sent_at = Instant::now() - Duration::from_millis(150);
    }
    mgr.cancel_pending_namespace_calls(OWNER, "removed");
    let rows = log.snapshot().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].request_seq, seq.get());
    assert_eq!(
        (
            rows[0].plugin_hops[0].host_request_id,
            rows[0].plugin_hops[0].outcome
        ),
        (
            req_id,
            tasty_telemetry::slow_requests::HopOutcome::Cancelled
        )
    );
}

/// pre-hook이 만료돼도 fail-open으로 실행한 target의 대기를 같은 요청에 기록해야 한다.
#[test]
fn an_expired_pre_hook_keeps_the_row_open_for_its_target() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, _owner_rx, _ext_rx) = mgr_with_pre_hook(50);
    mgr.set_slow_requests(log.clone());
    let seq = RequestSeq::next();
    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.forward_namespace_call(
        "orig.run",
        serde_json::json!({}),
        None,
        serde_json::json!(1),
        tx,
        Some(seq),
    );
    log.finish_host(fast_host_leg(seq));
    let (hook_id, _) = pending_to(&mgr, EXT);
    // hook 제한 50ms를 넘기되 느린 요청 기준인 100ms 전에 만료시킨다.
    let sent_at = mgr.pending_requests[&hook_id].sent_at;
    mgr.sweep_expired_requests(sent_at + Duration::from_millis(60));
    assert!(
        log.snapshot().rows.is_empty(),
        "기준 시간 전에 hook이 만료돼도 후속 target 응답 전에는 기록을 확정하지 않는다"
    );
    let (target_id, _) = pending_to(&mgr, OWNER);
    if let Some(p) = mgr.pending_requests.get_mut(&target_id) {
        p.sent_at = Instant::now() - Duration::from_millis(150);
    }
    mgr.handle_plugin_response(OWNER, ok(target_id));

    let rows = log.snapshot().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].host.is_some(),
        "호스트 처리와 플러그인 대기가 같은 요청에 기록되지 않았다: {rows:?}"
    );
    let hops: Vec<_> = rows[0]
        .plugin_hops
        .iter()
        .map(|h| (h.plugin_id.as_str(), h.outcome))
        .collect();
    assert_eq!(
        hops,
        [
            (EXT, tasty_telemetry::slow_requests::HopOutcome::Expired),
            (OWNER, tasty_telemetry::slow_requests::HopOutcome::Ok)
        ]
    );
}

/// 이 범위의 tracing 이벤트를 문자열로 모은다.
fn capture_logs(f: impl FnOnce()) -> String {
    use std::sync::Mutex;
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
    tracing::subscriber::with_default(sub, f);
    let out = buf.0.lock().unwrap().clone();
    String::from_utf8_lossy(&out).into_owned()
}

/// `needle` 을 담은 WARN 줄 하나.
fn warn_line<'a>(logs: &'a str, needle: &str) -> &'a str {
    logs.lines()
        .find(|l| l.contains("WARN") && l.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} 를 담은 WARN 줄이 없다: {logs}"))
}

/// 응답 오류 로그 한 줄에 단계별 req_id와 원 IPC 요청 번호를 함께 기록한다.
#[test]
fn the_error_answer_warning_names_the_host_id_and_the_request_seq_on_one_line() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, seq, _rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    let logs = capture_logs(|| {
        mgr.handle_plugin_response(
            OWNER,
            PluginResponse {
                id: req_id,
                result: None,
                error: Some("boom".into()),
                error_code: None,
            },
        );
    });
    let line = warn_line(&logs, "response error");
    assert!(
        line.contains(&format!("id={req_id}")) && line.contains(&format!("request_seq={seq}")),
        "{line}"
    );
    assert_eq!(logs.matches("WARN").count(), 1, "경고 줄이 늘었다: {logs}");
}

/// namespace 만료 경고 한 줄에도 둘이 함께 있다. caller 에 가는 문구는 그대로다.
#[test]
fn the_expiry_warning_names_the_host_id_and_the_request_seq_on_one_line() {
    let log = Arc::new(tasty_telemetry::SlowRequestLog::default());
    let (mut mgr, seq, rx) = forwarded(&log);
    let (req_id, _) = pending_to(&mgr, OWNER);
    let logs = capture_logs(|| {
        mgr.sweep_expired_requests(Instant::now() + Duration::from_secs(3600));
    });
    let line = warn_line(&logs, "did not answer");
    assert!(
        line.contains(&format!("id={req_id}")) && line.contains(&format!("request_seq={seq}")),
        "{line}"
    );
    let answer = rx.try_recv().expect("caller answer").error.expect("error");
    assert!(
        !answer.message.contains("request_seq"),
        "caller 문구가 바뀌었다: {}",
        answer.message
    );
}

/// 공개 IPC·CLI·다른 plugin 의 일반 namespace 요청은 원격 출처 표지를 실을 수 없다. host 가 넣는
/// 원격 holder call 만 `forward_attach_call` 로 그 표지를 전달한다.
#[test]
fn a_general_namespace_request_cannot_carry_remote_attach_markers() {
    let mut mgr = mgr_owning("orig");
    let (owner, owner_rx) = PluginProcess::stub_with_request_rx(OWNER);
    mgr.processes.insert(OWNER.into(), owner);
    for params in [
        serde_json::json!({ "attach_context": "guess" }),
        serde_json::json!({ "attach_target_surface": 7 }),
        serde_json::json!({ "parent_external": "attach:o1", "child": 0 }),
    ] {
        let (tx, rx) = mpsc::sync_channel(1);
        mgr.forward_namespace_call(
            "orig.run",
            params.clone(),
            None,
            serde_json::json!(1),
            tx,
            None,
        );
        let response = rx.try_recv().expect("refused at once");
        assert_eq!(response.error.map(|e| e.code), Some(-32602), "{params}");
        assert!(
            owner_rx.try_recv().is_err(),
            "{params} must not reach the plugin"
        );
    }
    let (tx, _rx) = mpsc::sync_channel(1);
    mgr.forward_attach_call(
        "orig.run",
        serde_json::json!({ "parent_external": "attach:o1", "attach_context": "c" }),
        None,
        serde_json::json!(1),
        tx,
        None,
    );
    assert!(
        owner_rx.try_recv().is_ok(),
        "the host's remote call reaches the plugin"
    );
}
