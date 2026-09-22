//! 연결 전에 보낸 요청의 시한은 연결 성사부터 센다는 것, 그리고 연결하지 못한 extension 에
//! 보낸 hook 은 보낸 적 없는 것으로 진행된다는 것(ADR-0505).
//!
//! 프로세스는 stub 이다 — 연결 상태만 손으로 바꾸고, 시각은 sweep 에 주입한다.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use tasty_ipc::protocol::JsonRpcResponse;
use tasty_plugin_manifest::HookMode;

use super::super::{
    FinalCaller, NAMESPACE_CALL_TIMEOUT, PendingRequest, PendingRequestKind, PluginManager,
};
use crate::process::{PluginProcess, RequestTap};
use crate::protocol::PluginResponse;
use tasty_terminal::waker_factory::NoopWakerFactory;

const EXT: &str = "com.example.ext";
const TARGET: &str = "com.example.target";
const PRE_HOOK_TIMEOUT: Duration = Duration::from_millis(50);

fn manager() -> PluginManager {
    PluginManager::new(Arc::new(NoopWakerFactory))
}

/// 아직 연결 중인 프로세스.
fn connecting(id: &str) -> (PluginProcess, RequestTap) {
    let (mut proc, tap) = PluginProcess::stub_with_request_rx(id);
    proc.mark_connecting_for_test();
    (proc, tap)
}

fn ok(id: u64) -> PluginResponse {
    PluginResponse {
        id,
        result: Some(serde_json::json!({})),
        error: None,
        error_code: None,
    }
}

/// `EXT` 에 보낸 pre-hook 한 건(`TARGET` 의 `target.run` 앞). 시한은 `PRE_HOOK_TIMEOUT`.
fn pre_hook(sent_at: Instant) -> (PendingRequest, mpsc::Receiver<JsonRpcResponse>) {
    let (tx, rx) = mpsc::sync_channel(1);
    let mut p = PendingRequest::now(
        EXT,
        PendingRequestKind::ExtensionPreIpcHook {
            target_plugin_id: TARGET.into(),
            extension_plugin_id: EXT.into(),
            method: "target.run".into(),
            params: serde_json::json!({}),
            pre_hook_mode: HookMode::Observe,
            final_caller: FinalCaller::Local {
                response_tx: tx,
                original_id: serde_json::json!(1),
                origin: None,
            },
            post_hook: None,
            deadline: sent_at + PRE_HOOK_TIMEOUT,
        },
    );
    p.sent_at = sent_at;
    (p, rx)
}

/// 막 띄운 extension 이 pre-hook 시한보다 오래 걸려 연결해도 그 첫 hook 은 만료되지 않고,
/// 연결 뒤 답하면 target 까지 이어진다.
///
/// 시한을 보낸 시각부터 세면 연결을 기다리는 사이 sweep 이 fail-open 으로 hook 을 버리고
/// extension 을 실패로 센다 — 막 띄운 extension 의 첫 hook 이 늘 그렇게 된다.
#[test]
fn a_pre_hook_sent_before_the_extension_connects_is_timed_from_the_connection() {
    let mut mgr = manager();
    let (ext, _ext_tap) = connecting(EXT);
    let (target, target_tap) = PluginProcess::stub_with_request_rx(TARGET);
    mgr.processes.insert(EXT.into(), ext);
    mgr.processes.insert(TARGET.into(), target);
    let sent = Instant::now();
    let (p, _rx) = pre_hook(sent);
    mgr.pending_requests.insert(7, p);

    // 연결이 시한의 스무 배 걸린다.
    let connected = sent + PRE_HOOK_TIMEOUT * 20;
    mgr.sweep_expired_requests(connected - Duration::from_millis(1));
    assert!(
        mgr.pending_requests.contains_key(&7),
        "연결 중인 extension 에 보낸 hook 이 만료됐다"
    );
    mgr.processes
        .get_mut(EXT)
        .expect("ext")
        .mark_connected(connected);
    mgr.sweep_expired_requests(connected + PRE_HOOK_TIMEOUT / 2);
    assert!(
        mgr.pending_requests.contains_key(&7),
        "연결 뒤 시한이 다 차기 전에 만료됐다"
    );

    mgr.handle_plugin_response(EXT, ok(7));
    assert!(mgr.hook_failures.is_empty(), "답한 hook 이 실패로 세였다");
    let forwarded = target_tap
        .try_recv()
        .expect("hook 을 지나 target 에 닿는다");
    assert_eq!(forwarded.method, "ipc.invoke");
}

/// 연결 뒤에도 답이 없으면 시한의 **길이**만큼 지나 예전처럼 만료된다 — 시한을 미루기만
/// 하고 없애지 않는다.
#[test]
fn a_pre_hook_still_expires_one_timeout_after_the_connection() {
    let mut mgr = manager();
    let (ext, _ext_tap) = connecting(EXT);
    let (target, _target_tap) = PluginProcess::stub_with_request_rx(TARGET);
    mgr.processes.insert(EXT.into(), ext);
    mgr.processes.insert(TARGET.into(), target);
    let sent = Instant::now();
    let (p, _rx) = pre_hook(sent);
    mgr.pending_requests.insert(7, p);

    let connected = sent + PRE_HOOK_TIMEOUT * 20;
    mgr.processes
        .get_mut(EXT)
        .expect("ext")
        .mark_connected(connected);
    mgr.sweep_expired_requests(connected + PRE_HOOK_TIMEOUT);
    assert!(
        !mgr.pending_requests.contains_key(&7),
        "연결 뒤 시한이 지났는데 안 만료됐다"
    );
}

/// namespace 호출도 같은 규칙이다 — owner 가 시한보다 오래 걸려 연결해도 첫 호출이 성공한다.
#[test]
fn a_namespace_call_sent_before_the_owner_connects_is_timed_from_the_connection() {
    let mut mgr = manager();
    let (owner, _tap) = connecting(TARGET);
    mgr.processes.insert(TARGET.into(), owner);
    let sent = Instant::now();
    let (tx, rx) = mpsc::sync_channel(1);
    let mut p = PendingRequest::now(
        TARGET,
        PendingRequestKind::NamespaceInvoke {
            plugin_id: TARGET.into(),
            response_tx: tx,
            original_id: serde_json::json!(3),
            deadline: sent + NAMESPACE_CALL_TIMEOUT,
        },
    );
    p.sent_at = sent;
    mgr.pending_requests.insert(9, p);

    let connected = sent + NAMESPACE_CALL_TIMEOUT + Duration::from_secs(1);
    mgr.sweep_expired_requests(connected - Duration::from_millis(1));
    assert!(
        rx.try_recv().is_err(),
        "연결 중인 owner 에 보낸 호출이 만료로 회신됐다"
    );
    mgr.processes
        .get_mut(TARGET)
        .expect("owner")
        .mark_connected(connected);
    mgr.sweep_expired_requests(connected + NAMESPACE_CALL_TIMEOUT - Duration::from_secs(1));

    mgr.handle_plugin_response(TARGET, ok(9));
    let answer = rx.try_recv().expect("caller 가 답을 받는다");
    assert!(answer.error.is_none(), "첫 호출이 실패했다: {answer:?}");
}

/// 연결에 끝내 실패한 extension 에 보낸 pre-hook 은 예전 기동 실패 때처럼 hook 없이 target 으로
/// 간다 — caller 에 오류를 돌려주지 않고, 실행된 적 없는 hook 을 실패로 세지 않는다.
#[test]
fn a_pre_hook_to_an_extension_that_never_connects_is_bypassed() {
    let mut mgr = manager();
    let (ext, _ext_tap) = connecting(EXT);
    let (target, target_tap) = PluginProcess::stub_with_request_rx(TARGET);
    mgr.processes.insert(EXT.into(), ext);
    mgr.processes.insert(TARGET.into(), target);
    let (p, rx) = pre_hook(Instant::now());
    mgr.pending_requests.insert(7, p);

    mgr.on_connect_failure(EXT, "did not connect".into());

    assert!(
        rx.try_recv().is_err(),
        "hook 을 건너뛰지 않고 caller 에 오류를 돌려줬다"
    );
    assert!(
        mgr.hook_failures.is_empty(),
        "실행된 적 없는 hook 을 실패로 셌다"
    );
    let forwarded = target_tap.try_recv().expect("hook 없이 target 에 닿는다");
    assert_eq!(forwarded.method, "ipc.invoke");
}

/// plugin 이 부른 호출이면 건너뛴 hook 뒤 target 에 가는 요청에 그 plugin id 가 실린다 — hook
/// 송신이 실패했을 때의 갈래와 같은 값이다. 로컬 호출이면 싣지 않는다.
#[test]
fn a_bypassed_pre_hook_carries_the_calling_plugin_to_the_target() {
    const CALLER: &str = "com.example.caller";
    let mut mgr = manager();
    let (ext, _ext_tap) = connecting(EXT);
    let (target, target_tap) = PluginProcess::stub_with_request_rx(TARGET);
    mgr.processes.insert(EXT.into(), ext);
    mgr.processes.insert(TARGET.into(), target);
    let (mut from_plugin, _rx) = pre_hook(Instant::now());
    if let PendingRequestKind::ExtensionPreIpcHook { final_caller, .. } = &mut from_plugin.kind {
        *final_caller = FinalCaller::Plugin {
            caller_plugin_id: CALLER.into(),
            call_id: 5,
        };
    }
    mgr.pending_requests.insert(7, from_plugin);
    let (local, _local_rx) = pre_hook(Instant::now());
    mgr.pending_requests.insert(8, local);

    mgr.on_connect_failure(EXT, "did not connect".into());

    let mut callers: Vec<serde_json::Value> = std::iter::from_fn(|| target_tap.try_recv().ok())
        .map(|req| req.params["caller_plugin_id"].clone())
        .collect();
    callers.sort_by_key(|v| v.is_null());
    assert_eq!(
        callers,
        vec![serde_json::json!(CALLER), serde_json::Value::Null],
        "target 에 실린 호출 plugin id 가 송신 실패 갈래와 다르다"
    );
}
