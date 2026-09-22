//! Plugin → host 응답 처리. `pump` 에서 매 tick 호출되는 `drain_plugin_responses`
//! 와 그 dispatch 로 호출되는 pre/post hook 응답 처리, `sweep_expired_requests` 까지
//! 포함.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::protocol::{self, PluginResponse, SurfaceResult};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::send_response;
use tasty_plugin_manifest::{EventHookDecl, HookMode, IpcHookDecl};

use std::sync::Arc;

use super::{
    FinalCaller, HookOutcome, NAMESPACE_CALL_TIMEOUT, PendingRequest, PendingRequestKind,
    PluginManager, TargetOutcome, parse_hook_result,
};

/// 만료된 namespace 호출이 caller 에게 돌려주는 사유. 세 변종이 같은 문장을 쓴다 —
/// caller 입장에서는 같은 일(기다리던 plugin 응답이 끝내 안 왔다)이다.
fn namespace_timeout_message(plugin_id: &str) -> String {
    format!(
        "plugin '{plugin_id}' did not answer within {}s",
        NAMESPACE_CALL_TIMEOUT.as_secs()
    )
}

/// 경고 줄에 싣는 원 요청 번호. 번호를 모르는 plugin 요청이면 `none` 이다(IPC 요청에서 오지 않은
/// 것, 그리고 파일 핸들러 큐를 거쳐 번호를 잃은 것).
fn request_seq_label(origin: Option<tasty_ipc::server::RequestSeq>) -> String {
    origin.map_or_else(|| "none".to_string(), |seq| seq.to_string())
}

// surface handle 슬롯 락의 poison 보고 플래그(각 첫 1 회만). 둘 다 값 슬롯(String ·
// Option<Value>)이라 락을 든 채 죽어도 불변식이 성하다 — 복구가 맞다. 조용히 삼키면
// 라벨/스냅샷이 갱신 없이 stale 로 남는데 그 사실이 어디에도 안 남는다.
static DISPLAY_NAME_POISONED: AtomicBool = AtomicBool::new(false);
const DISPLAY_NAME_WHAT: &str = "surface display-name slot";
static SNAPSHOT_CACHE_POISONED: AtomicBool = AtomicBool::new(false);
const SNAPSHOT_CACHE_WHAT: &str = "surface snapshot cache slot";

impl PluginManager {
    pub(super) fn drain_plugin_responses(&mut self) {
        let plugin_ids: Vec<String> = self.processes.keys().cloned().collect();
        for plugin_id in plugin_ids {
            // Drain all responses without holding a borrow on `self.processes`.
            let mut responses: Vec<PluginResponse> = Vec::new();
            if let Some(proc) = self.processes.get(&plugin_id) {
                while let Ok(resp) = proc.resp_rx.try_recv() {
                    responses.push(resp);
                }
            }
            for resp in responses {
                self.handle_plugin_response(&plugin_id, resp);
            }
        }
    }

    /// 왕복 대기 게이지를 주입한다. 호스트가 `Core` 쪽 압력 게이지와 **같은 축으로**
    /// 읽을 수 있도록 같은 프로세스의 한 인스턴스를 공유한다.
    pub fn set_plugin_wait(&mut self, stats: Arc<tasty_telemetry::PluginWaitStats>) {
        self.plugin_wait = Some(stats);
    }

    /// plugin 채널에 지금 쌓인 바이트와 상한·거절·대기 누계(ADR-0360).
    ///
    /// 값을 만드는 데까지다 — IPC/CLI 로 내보내는 자리(`system.pressure`)는 아직 이 값을
    /// 안 읽는다. 장부가 프로세스 하나라서 어느 매니저에서 불러도 같은 값이 나온다.
    pub fn channel_bytes(&self) -> crate::process::channel_bytes::ChannelBytesSnapshot {
        self.channel_ledger.snapshot()
    }

    /// 느린 요청 링을 주입한다(ADR-0436). `set_plugin_wait` 과 같이 프로세스의 한 인스턴스를
    /// 호스트 dispatch 루프와 나눠 든다 — 호스트 몫과 plugin hop 이 같은 줄에 이어지려면 둘이
    /// 같은 링을 봐야 한다.
    pub fn set_slow_requests(&mut self, log: Arc<tasty_telemetry::SlowRequestLog>) {
        self.slow_requests = Some(log);
    }

    /// 원 IPC 요청 번호를 든 plugin 요청을 대기 표에 넣는다 — 링에 그 요청의 자리를 먼저 연다.
    /// hop 이 끝나기 전에 호스트 몫이 채워지므로(forward 는 dispatch 안에서 일어난다) 자리가
    /// 먼저 있어야 호스트 몫이 문턱 아래여도 버려지지 않고 hop 을 기다린다.
    pub(super) fn insert_pending(&mut self, req_id: u64, pending: PendingRequest) {
        if let (Some(log), Some(seq)) = (&self.slow_requests, pending.origin) {
            log.note_forwarded(seq.get());
        }
        self.pending_requests.insert(req_id, pending);
    }

    /// 원 IPC 요청 번호를 든 대기 항목 하나가 끝났다 — 그 hop 을 링의 줄에 붙인다. 번호가 없는
    /// 항목(IPC 요청에서 오지 않았거나 파일 핸들러 큐에서 번호를 잃은 것)은 남기지 않는다.
    ///
    /// `last` 는 사슬이 이 hop 에서 끝나는가다. pre-hook 은 늘 target 으로 이어지고(응답이든
    /// fail-open 이든), post-hook 이 걸린 target 은 응답이 오면 post-hook 으로 이어진다.
    pub(super) fn record_origin_hop(
        &self,
        req_id: u64,
        pending: &PendingRequest,
        waited: std::time::Duration,
        outcome: tasty_telemetry::slow_requests::HopOutcome,
        last: bool,
    ) {
        let (Some(log), Some(seq)) = (&self.slow_requests, pending.origin) else {
            return;
        };
        log.finish_plugin_hop(
            seq.get(),
            tasty_telemetry::slow_requests::PluginHop {
                plugin_id: pending.to.clone(),
                host_request_id: req_id,
                wait_us: u64::try_from(waited.as_micros()).unwrap_or(u64::MAX),
                outcome,
            },
            last,
        );
    }

    /// 응답 하나가 매칭됐다 — 보낸 뒤 흐른 시간을 게이지에 접는다.
    fn record_plugin_wait(&self, waited: std::time::Duration) {
        if let Some(stats) = &self.plugin_wait {
            stats.record(waited);
        }
    }

    pub(super) fn handle_plugin_response(&mut self, plugin_id: &str, resp: PluginResponse) {
        let pending = self.pending_requests.remove(&resp.id);
        // 왕복 대기 — 여기가 모든 plugin 응답이 지나는 한 자리다. 매칭되지 않은
        // 응답(이미 만료·취소된 id)은 재지 않는다: 잰 값의 끝점이 없다.
        if let Some(p) = &pending {
            let waited = p.sent_at.elapsed();
            self.record_plugin_wait(waited);
            let outcome = if resp.error.is_some() {
                tasty_telemetry::slow_requests::HopOutcome::Error
            } else {
                tasty_telemetry::slow_requests::HopOutcome::Ok
            };
            // 응답이 사슬의 다음 hop 을 부르는 두 종류 — 그 밖은 여기서 끝난다.
            let last = !matches!(
                p.kind,
                PendingRequestKind::ExtensionPreIpcHook { .. }
                    | PendingRequestKind::NamespaceInvokeWithPostHook { .. }
            );
            self.record_origin_hop(resp.id, p, waited, outcome, last);
        }
        if let Some(err) = &resp.error {
            // 원 IPC 요청 번호를 같은 줄에 싣는다 — plugin 로그의 id(= 호스트 req_id)에서 호스트
            // 쪽 원 요청으로 되짚는 열쇠다(ADR-0436). 새 줄을 만들지 않는다.
            tracing::warn!(
                "plugin '{plugin_id}' response error (id={}, request_seq={}): {err}",
                resp.id,
                request_seq_label(pending.as_ref().and_then(|p| p.origin))
            );
        }
        let kind = match pending {
            Some(p) => p.kind,
            None => {
                // `event.dispatch` 의 응답은 pending 을 안 만든다(호스트가 기다리지 않는다) —
                // 대신 버스가 재발화 hop 하한을 위해 따로 기록하고, plugin 은 응답해야 한다
                // (ADR-0406). 그 기록이면 여기서 끝난다.
                if self.event_bus.note_dispatch_answered(plugin_id, resp.id) {
                    return;
                }
                // id 가 안 맞는 응답 — 이미 만료·취소돼 거둬진 것이다.
                self.settle_late_response(plugin_id, resp.id);
                return;
            }
        };
        // namespace 응답이 하나라도 오면 그 plugin 의 연속 만료 계수를 지운다 —
        // 답하고 있는 plugin 은 느려도 재시작 판정에 안 걸린다. pong 은 애초에
        // pending 을 안 만들어(`PluginProcess::ping`) 위 `None` 갈래로 빠지고, 거기서도
        // namespace 로 기억된 id 가 아니라 안 지운다: ping 에만 답하는 plugin 을
        // 가리려는 것이 계수의 목적이므로, pong 이 계수를 지우면 계수가 영영 안 찬다.
        // 이 `matches!` 가 실제로 거르는 것은 `SurfaceCreate`·`PopupOpen`·`Other`
        // 같은 다른 pending 종류다.
        if matches!(
            kind,
            PendingRequestKind::NamespaceInvoke { .. }
                | PendingRequestKind::PluginToPluginNamespace { .. }
                | PendingRequestKind::NamespaceInvokeWithPostHook { .. }
        ) {
            self.namespace_expiries.remove(plugin_id);
        }
        match kind {
            PendingRequestKind::SurfaceCreate { surface_id }
            | PendingRequestKind::SurfaceRestore { surface_id }
            | PendingRequestKind::CommandInvoke { surface_id } => {
                self.apply_surface_response(plugin_id, surface_id, resp.result);
            }
            PendingRequestKind::Other => {}
            PendingRequestKind::PopupOpen { instance_id } => {
                handle_popup_open_response(plugin_id, instance_id, resp.result);
            }
            PendingRequestKind::NamespaceInvoke {
                plugin_id: _,
                response_tx,
                original_id,
                deadline: _,
            } => {
                send_namespace_result(
                    &response_tx,
                    original_id,
                    resp.error,
                    resp.error_code,
                    resp.result,
                );
            }
            PendingRequestKind::PluginToPluginNamespace {
                plugin_id: _,
                caller_plugin_id,
                call_id,
                deadline: _,
            } => {
                // plugin caller에는 ipc.result로 회신. 코드도 함께 간다 — 같은 함수의
                // namespace 갈래(위)가 이미 `resp.error_code` 를 쓰고 있었고, 이쪽만
                // 버리면 plugin 을 거쳐 나온 응답이 전부 `-32000` 이 된다.
                self.send_ipc_result(
                    &caller_plugin_id,
                    call_id,
                    resp.result,
                    resp.error,
                    resp.error_code,
                );
            }
            PendingRequestKind::ExtensionPreIpcHook {
                target_plugin_id,
                extension_plugin_id,
                method,
                params,
                pre_hook_mode,
                final_caller,
                post_hook,
                deadline: _,
            } => {
                self.handle_pre_ipc_hook_response(
                    extension_plugin_id,
                    target_plugin_id,
                    method,
                    params,
                    pre_hook_mode,
                    final_caller,
                    post_hook,
                    resp,
                );
            }
            PendingRequestKind::NamespaceInvokeWithPostHook {
                target_plugin_id: _,
                method,
                extension_plugin_id,
                post_hook_decl,
                final_caller,
                deadline: _,
            } => {
                self.handle_target_response_with_post_hook(
                    extension_plugin_id,
                    method,
                    post_hook_decl,
                    final_caller,
                    resp,
                );
            }
            PendingRequestKind::ExtensionPostIpcHook {
                extension_plugin_id,
                method,
                post_hook_mode,
                target_outcome,
                final_caller,
                deadline: _,
            } => {
                self.record_hook_outcome(&extension_plugin_id, &method, resp.error.is_some());
                self.handle_post_ipc_hook_response(
                    post_hook_mode,
                    target_outcome,
                    final_caller,
                    resp,
                );
            }
            PendingRequestKind::ExtensionPreEventHook {
                publisher_plugin_id,
                extension_plugin_id,
                envelope,
                pre_hook_mode,
                post_hook,
                deadline: _,
            } => {
                self.handle_pre_event_hook_response(
                    publisher_plugin_id,
                    extension_plugin_id,
                    envelope,
                    pre_hook_mode,
                    post_hook,
                    resp,
                );
            }
            PendingRequestKind::ExtensionPostEventHook {
                extension_plugin_id,
                event_key,
                deadline: _,
            } => {
                self.record_hook_outcome(&extension_plugin_id, &event_key, resp.error.is_some());
                // post-event는 결과를 무시 (이미 fan-out 완료).
            }
            #[cfg(debug_assertions)]
            PendingRequestKind::DebugExtensionInvokeHook {
                response_tx,
                original_id,
                deadline: _,
            } => {
                send_namespace_result(
                    &response_tx,
                    original_id,
                    resp.error,
                    resp.error_code,
                    resp.result,
                );
            }
        }
    }

    /// post-hook 응답의 성공/실패를 hook 실패 카운터에 반영하는 공통 스텝
    /// (ExtensionPostIpcHook/ExtensionPostEventHook 두 arm 이 공유).
    fn record_hook_outcome(&mut self, extension_id: &str, key: &str, failed: bool) {
        if failed {
            self.record_hook_failure(extension_id, key);
        } else {
            self.record_hook_success(extension_id, key);
        }
    }

    /// `SurfaceCreate`/`SurfaceRestore`/`CommandInvoke` 공통 응답 처리 —
    /// display_name/snapshot 동기화. 결과 없음/디코드 실패/surface 미존재는 조용히 skip.
    fn apply_surface_response(
        &mut self,
        plugin_id: &str,
        surface_id: u32,
        result: Option<serde_json::Value>,
    ) {
        let Some(result_value) = result else {
            return;
        };
        let parsed: SurfaceResult = match serde_json::from_value(result_value) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("plugin '{plugin_id}' surface response decode error: {e}");
                return;
            }
        };
        let Some(entry) = self.surfaces.get(&surface_id) else {
            return;
        };
        if let Some(name) = parsed.display_name {
            *tasty_utils::poison::recover_mutex(
                entry.handles.display_name.lock(),
                DISPLAY_NAME_WHAT,
                &DISPLAY_NAME_POISONED,
            ) = name;
        }
        if let Some(snapshot) = parsed.snapshot {
            *tasty_utils::poison::recover_mutex(
                entry.handles.snapshot_cache.lock(),
                SNAPSHOT_CACHE_WHAT,
                &SNAPSHOT_CACHE_POISONED,
            ) = Some(snapshot);
        }
    }

    /// debug 빌드 한정 — extension에 직접 hook을 송신하고 응답을 caller에 회신.
    /// 테스트 도구. 정상 트래픽(IPC/event)이 아니라 디버그 path.
    #[cfg(debug_assertions)]
    #[allow(clippy::too_many_arguments)] // reason: debug IPC 시그니처와 1:1 매핑
    pub fn debug_invoke_extension_hook(
        &mut self,
        extension_id: &str,
        kind: tasty_plugin_protocol::ExtensionHookKind,
        phase: tasty_plugin_protocol::ExtensionHookPhase,
        mode: HookMode,
        target: &str,
        payload: serde_json::Value,
        original_id: serde_json::Value,
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
    ) {
        if !self.processes.contains_key(extension_id) {
            send_response(
                &response_tx,
                JsonRpcResponse::error(
                    original_id,
                    -32002,
                    format!("extension '{extension_id}' is not running"),
                ),
            );
            return;
        }
        match self.send_extension_invoke_hook(extension_id, kind, phase, mode, target, payload) {
            Ok(req_id) => {
                self.pending_requests.insert(
                    req_id,
                    PendingRequest::now(
                        extension_id,
                        PendingRequestKind::DebugExtensionInvokeHook {
                            response_tx,
                            original_id,
                            deadline: Instant::now() + super::DEBUG_HOOK_INVOKE_TIMEOUT,
                        },
                    ),
                );
            }
            Err(msg) => {
                send_response(
                    &response_tx,
                    JsonRpcResponse::error(original_id, -32003, &msg),
                );
            }
        }
    }

    /// pre-event-hook 응답을 처리. mode에 따라 transform(payload 교체) / filter(차단) / observe.
    pub(super) fn handle_pre_event_hook_response(
        &mut self,
        publisher_plugin_id: String,
        extension_plugin_id: String,
        mut envelope: tasty_plugin_protocol::EventEnvelope,
        mode: HookMode,
        post_hook: Option<EventHookDecl>,
        resp: PluginResponse,
    ) {
        if resp.error.is_some() {
            self.record_hook_failure(&extension_plugin_id, &envelope.key);
        } else {
            self.record_hook_success(&extension_plugin_id, &envelope.key);
        }
        let outcome = parse_hook_result(&resp);

        if matches!(mode, HookMode::Filter) && matches!(outcome, HookOutcome::Block) {
            tracing::info!(
                "extension '{extension_plugin_id}' filtered event '{}' from '{publisher_plugin_id}'",
                envelope.key
            );
            return;
        }
        if let (HookMode::Transform, HookOutcome::Modified(new_payload)) = (mode, outcome) {
            envelope.payload = new_payload;
        }
        self.fan_out_then_post(
            &publisher_plugin_id,
            envelope,
            extension_plugin_id,
            post_hook,
        );
    }

    /// pre-hook 응답을 처리. mode에 따라 transform/filter/observe 적용 후 target에 forward.
    #[allow(clippy::too_many_arguments)] // reason: IPC pre-hook 컨텍스트 전체 전달
    pub(super) fn handle_pre_ipc_hook_response(
        &mut self,
        extension_plugin_id: String,
        target_plugin_id: String,
        method: String,
        original_params: serde_json::Value,
        mode: HookMode,
        final_caller: FinalCaller,
        post_hook: Option<IpcHookDecl>,
        resp: PluginResponse,
    ) {
        if resp.error.is_some() {
            self.record_hook_failure(&extension_plugin_id, &method);
        } else {
            self.record_hook_success(&extension_plugin_id, &method);
        }
        // hook 응답에서 outcome 추출. 에러/누락은 fail-open(original payload로 진행).
        let outcome = parse_hook_result(&resp);
        let post_pair = post_hook.map(|p| (extension_plugin_id.clone(), p));

        // payload는 hook 호출 시 {method, params, caller}로 래핑했으므로,
        // transform이면 wrapper의 params 필드를 다시 꺼내 사용한다.
        let final_params = match (mode, &outcome) {
            (HookMode::Transform, HookOutcome::Modified(v)) => {
                v.get("params").cloned().unwrap_or(original_params)
            }
            _ => original_params,
        };

        // filter mode에서 block이면 호출 자체 차단.
        if matches!(mode, HookMode::Filter) && matches!(outcome, HookOutcome::Block) {
            let msg = format!("extension '{extension_plugin_id}' filtered out method '{method}'");
            // post-hook이 있어도 filter block이면 target 호출이 일어나지 않으므로 post도 skip.
            self.send_final_error(final_caller, -32001, msg);
            return;
        }

        // observe/transform/filter-pass → target에 정상 invoke.
        self.dispatch_target_invoke(
            target_plugin_id,
            method,
            final_params,
            None, // caller_plugin_id는 검증을 이미 통과했으므로 target 측 caller 표시는 필요시만.
            final_caller,
            post_pair,
        );
    }

    /// target plugin 응답을 받았을 때 post-hook으로 chain.
    pub(super) fn handle_target_response_with_post_hook(
        &mut self,
        extension_plugin_id: String,
        method: String,
        post_hook_decl: IpcHookDecl,
        final_caller: FinalCaller,
        resp: PluginResponse,
    ) {
        let target_outcome = if let Some(err) = resp.error.clone() {
            TargetOutcome::Err {
                message: err,
                code: resp.error_code.unwrap_or(-32000),
            }
        } else {
            TargetOutcome::Ok(resp.result.clone().unwrap_or(serde_json::Value::Null))
        };

        // post-hook payload: target의 result (에러면 null 전달).
        let payload = match &target_outcome {
            TargetOutcome::Ok(v) => v.clone(),
            TargetOutcome::Err { .. } => serde_json::Value::Null,
        };

        if self.is_hook_in_backoff(&extension_plugin_id, &method) {
            self.finalize_target_outcome(final_caller, target_outcome);
            return;
        }
        let deadline = Instant::now() + Duration::from_millis(post_hook_decl.timeout_ms as u64);
        let origin = final_caller.origin();
        match self.send_extension_invoke_hook(
            &extension_plugin_id,
            tasty_plugin_protocol::ExtensionHookKind::Ipc,
            tasty_plugin_protocol::ExtensionHookPhase::Post,
            post_hook_decl.mode,
            &method,
            payload,
        ) {
            Ok(req_id) => {
                self.insert_pending(
                    req_id,
                    PendingRequest::now(
                        extension_plugin_id.clone(),
                        PendingRequestKind::ExtensionPostIpcHook {
                            extension_plugin_id,
                            method,
                            post_hook_mode: post_hook_decl.mode,
                            target_outcome,
                            final_caller,
                            deadline,
                        },
                    )
                    .for_request(origin),
                );
            }
            Err(msg) => {
                tracing::warn!("post-hook dispatch failed: {msg}; bypassing");
                self.finalize_target_outcome(final_caller, target_outcome);
            }
        }
    }

    /// deadline 을 넘긴 pending 요청을 sweep 한다. hook 은 fail-open(원래 흐름을
    /// 그대로 진행)으로, namespace 호출은 caller 에 오류 회신으로 끝난다 — 후자는
    /// "진행" 할 원본 흐름이 없다(target 응답 자체가 목적이었다).
    ///
    /// `now` 는 [`PluginManager::pump`] 가 받은 프레임 기준시각이다. 여기서
    /// `Instant::now()` 를 직접 읽으면 호출자가 넘긴 시각과 이 함수가 보는 시각이
    /// 갈리고, 그 순간 **시험이 시간을 주입할 자리가 없어진다** — deadline 을
    /// 과거/미래로 두는 것 말고는 만료를 만들 방법이 없고, 그 방식으로는 "만료가
    /// 한 번 일어난 뒤 같은 pending 이 다시 안 만료된다" 같은 시간 축의 성질을
    /// 못 잰다. pump 가 자기 타이머 판정에 쓰는 시각과 같은 값을 쓰는 것이기도 하다.
    pub(super) fn sweep_expired_requests(&mut self, now: Instant) {
        let expired = self.collect_expired_request_ids(now);
        for id in expired {
            if let Some(p) = self.pending_requests.remove(&id) {
                // pre-hook 만료는 fail-open 으로 target 을 부른다 — 사슬이 이어진다.
                let last = !matches!(p.kind, PendingRequestKind::ExtensionPreIpcHook { .. });
                self.record_origin_hop(
                    id,
                    &p,
                    now.saturating_duration_since(p.sent_at),
                    tasty_telemetry::slow_requests::HopOutcome::Expired,
                    last,
                );
                self.expire_pending_request(id, p.kind, p.origin);
            }
        }
    }

    /// 현재 pending 중인 요청 가운데 `now` 시점 deadline 을 넘긴 request id 목록.
    /// deadline 을 든 변종만 본다 — 4 종 hook(pre/post × ipc/event) 과 3 종
    /// namespace 호출.
    ///
    /// 시한은 받는 plugin 의 연결 성사부터 센다([`Self::deadline_from_connection`]) —
    /// 연결 중인 plugin 에 보낸 요청은 아직 만료되지 않는다.
    fn collect_expired_request_ids(&self, now: Instant) -> Vec<u64> {
        let expired = |p: &PendingRequest, deadline: Instant| {
            self.deadline_from_connection(&p.to, p.sent_at, deadline)
                .is_some_and(|d| now >= d)
        };
        self.pending_requests
            .iter()
            .filter_map(|(id, p)| match &p.kind {
                PendingRequestKind::ExtensionPreIpcHook { deadline, .. }
                | PendingRequestKind::ExtensionPostIpcHook { deadline, .. }
                | PendingRequestKind::ExtensionPreEventHook { deadline, .. }
                | PendingRequestKind::ExtensionPostEventHook { deadline, .. }
                | PendingRequestKind::NamespaceInvoke { deadline, .. }
                | PendingRequestKind::PluginToPluginNamespace { deadline, .. }
                | PendingRequestKind::NamespaceInvokeWithPostHook { deadline, .. } => {
                    expired(p, *deadline).then_some(*id)
                }
                #[cfg(debug_assertions)]
                PendingRequestKind::DebugExtensionInvokeHook { deadline, .. } => {
                    expired(p, *deadline).then_some(*id)
                }
                _ => None,
            })
            .collect()
    }

    /// 타임아웃된 요청 한 건을 종결한다. hook 은 fail-open — target/publisher 는
    /// 원본 payload 로 그대로 진행시키고 해당 extension 은 실패로 기록한다.
    /// namespace 호출은 기다리던 응답이 곧 목적이라 진행시킬 것이 없다 — plugin 이
    /// 사라졌을 때(`cancel_pending_namespace_calls`)와 같은 모양으로 caller 에
    /// `-32004` 를 회신한다.
    fn expire_pending_request(
        &mut self,
        id: u64,
        kind: PendingRequestKind,
        origin: Option<tasty_ipc::server::RequestSeq>,
    ) {
        match kind {
            PendingRequestKind::ExtensionPreIpcHook {
                target_plugin_id,
                extension_plugin_id,
                method,
                params,
                pre_hook_mode: _,
                final_caller,
                post_hook,
                deadline: _,
            } => self.fail_open_pre_ipc_hook(
                target_plugin_id,
                extension_plugin_id,
                method,
                params,
                final_caller,
                post_hook,
            ),
            PendingRequestKind::ExtensionPostIpcHook {
                extension_plugin_id,
                method,
                post_hook_mode: _,
                target_outcome,
                final_caller,
                deadline: _,
            } => self.fail_open_post_ipc_hook(
                extension_plugin_id,
                method,
                target_outcome,
                final_caller,
            ),
            PendingRequestKind::ExtensionPreEventHook {
                publisher_plugin_id,
                extension_plugin_id,
                envelope,
                pre_hook_mode: _,
                post_hook,
                deadline: _,
            } => self.fail_open_pre_event_hook(
                publisher_plugin_id,
                extension_plugin_id,
                envelope,
                post_hook,
            ),
            PendingRequestKind::ExtensionPostEventHook {
                extension_plugin_id,
                event_key,
                deadline: _,
            } => self.fail_open_post_event_hook(extension_plugin_id, event_key),
            other => self.expire_pending_answer(id, other, origin),
        }
    }

    /// 회신이 목적인 pending 의 만료 — 진행시킬 원본 흐름이 없어 fail-open 이 없다.
    ///
    /// caller 종류(local `response_tx` · plugin `ipc.result` · post-hook 이 걸린
    /// `send_final_error`)만 다르고 싣는 코드는 셋 다 `-32004` 로 같다
    /// (`docs/adr/0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md`).
    fn expire_pending_answer(
        &mut self,
        id: u64,
        kind: PendingRequestKind,
        origin: Option<tasty_ipc::server::RequestSeq>,
    ) {
        match kind {
            PendingRequestKind::NamespaceInvoke {
                plugin_id,
                response_tx,
                original_id,
                deadline: _,
            } => {
                let msg = self.note_namespace_expiry(&plugin_id, id, origin);
                send_response(
                    &response_tx,
                    JsonRpcResponse::error(original_id, -32004, &msg),
                );
            }
            PendingRequestKind::PluginToPluginNamespace {
                plugin_id,
                caller_plugin_id,
                call_id,
                deadline: _,
            } => {
                let msg = self.note_namespace_expiry(&plugin_id, id, origin);
                // 바로 위 local 갈래와 같은 `-32004`. 만료는 caller 종류와 무관한
                // 같은 사건이다.
                self.send_ipc_result(&caller_plugin_id, call_id, None, Some(msg), Some(-32004));
            }
            PendingRequestKind::NamespaceInvokeWithPostHook {
                target_plugin_id,
                method: _,
                extension_plugin_id: _,
                post_hook_decl: _,
                final_caller,
                deadline: _,
            } => {
                let msg = self.note_namespace_expiry(&target_plugin_id, id, origin);
                self.send_final_error(final_caller, -32004, msg);
            }
            // debug 한정 직접 hook 호출도 `response_tx` 를 들고 있어 회신이 목적이다.
            // 진행시킬 원본 흐름이 없으므로 namespace 만료와 같은 모양으로 끝낸다.
            #[cfg(debug_assertions)]
            PendingRequestKind::DebugExtensionInvokeHook {
                response_tx,
                original_id,
                deadline: _,
            } => {
                let msg = format!(
                    "extension did not answer the debug hook invoke within {}ms",
                    super::DEBUG_HOOK_INVOKE_TIMEOUT.as_millis()
                );
                tracing::warn!("{msg}");
                send_response(
                    &response_tx,
                    JsonRpcResponse::error(original_id, -32004, &msg),
                );
            }
            _ => {}
        }
    }

    /// namespace 만료 한 건을 기록한다 — 경고를 남기고 연속 계수에 더한 뒤,
    /// caller 에 실을 문구를 돌려준다. 세 caller 갈래가 같은 문구·같은 계수를
    /// 쓰므로 그 셋을 여기 한 자리로 모은다.
    ///
    /// 경고에는 호스트 req_id 와 원 IPC 요청 번호를 **같은 줄에** 더한다 — caller 에 가는 문구
    /// (`msg`)는 그대로다. 둘은 plugin 쪽 로그(req_id)와 호스트 쪽 원 요청을 잇는 열쇠다(ADR-0436).
    fn note_namespace_expiry(
        &mut self,
        plugin_id: &str,
        req_id: u64,
        origin: Option<tasty_ipc::server::RequestSeq>,
    ) -> String {
        let msg = namespace_timeout_message(plugin_id);
        tracing::warn!(
            "{msg} (id={req_id}, request_seq={})",
            request_seq_label(origin)
        );
        self.record_namespace_expiry(plugin_id);
        self.remember_expired_namespace_call(plugin_id, req_id);
        msg
    }

    /// 거둬진 namespace 호출의 id 를 그 plugin 앞으로 적어 둔다. 뒤늦게 그 id 의
    /// 응답이 오면 [`Self::clear_streak_if_late_namespace_answer`] 가 계수를 지운다.
    /// 길이는 계수 상한만큼만 들고 오래된 것부터 버린다 — 상한에 닿으면 재시작이
    /// 일어나고 그 경로가 이 목록도 비우므로, 이 자름이 판정을 무르게 하지 않는다.
    fn remember_expired_namespace_call(&mut self, plugin_id: &str, req_id: u64) {
        let ids = self
            .expired_namespace_calls
            .entry(plugin_id.to_string())
            .or_default();
        ids.push_back(req_id);
        while ids.len() > super::NAMESPACE_EXPIRY_RESTART_LIMIT as usize {
            ids.pop_front();
        }
    }

    /// 이미 거둬진 요청의 **늦은 응답**을 처리하는 유일한 자리.
    ///
    /// 요청이 pending 에서 빠지는 길은 셋이다 — 응답 매칭 · deadline 만료(sweep) · plugin
    /// 이 치워짐(취소). 뒤 둘은 **그 자리에서 이미 끝을 냈다**: namespace 호출은 caller 에
    /// `-32004` 를 회신했고, hook 은 fail-open 으로 원래 흐름을 진행시켰다. 그래서 늦게
    /// 온 응답이 할 일은 **아무것도 진행시키지 않는 것**이고, 변종마다 그 이유가 있다.
    ///
    /// - **target 응답이 늦으면 post-hook 을 안 부른다.** post-hook 은 caller 에게 갈 결과를
    ///   바꾸는 단계인데 caller 는 이미 `-32004` 를 받았다. 부르면 extension 이 결과를 못
    ///   바꾸는 호출을 받고 **효과만 남는다**(post-hook 이 기록·전송을 하는 extension 이면
    ///   caller 가 실패로 본 호출에 대해 그 효과가 난다).
    /// - **pre-hook 응답이 늦으면 target 을 다시 안 부른다.** fail-open 이 원본 payload 로
    ///   이미 불렀다 — 다시 부르면 같은 호출이 두 번 실행된다.
    /// - **post-hook 응답이 늦으면 결과를 다시 안 보낸다.** fail-open 이 target 결과를 이미
    ///   보냈다 — caller 는 같은 id 에 답을 두 번 받는다.
    /// - **hook 실패 계수를 되돌리지 않는다.** 만료가 이미 실패로 셌고, backoff 가 묻는
    ///   것은 "제때 답하는가" 라 늦은 답은 여전히 실패다.
    ///
    /// 남는 일은 하나다 — namespace 호출의 늦은 답은 연속 만료 계수를 지운다(ADR-0311
    /// 2026-09-20 보강: 늦어도 답한 것은 답한 것이다). 이 정책의 근거와 대안은
    /// ADR-0311 의 2026-09-21 보강에 있다.
    ///
    /// ★ **이 자리에 오는 것이 늦은 응답만은 아니다.** pending 에 애초에 안 들어가는
    /// 요청(ping · `event.dispatch` 같은 알림성 요청)에도 plugin 은 답하고, 그 답도 id 가
    /// 안 맞아 여기로 온다. id 만으로는 둘을 못 가르고, 뒤엣것은 정상 운용에서 늘 난다 —
    /// 그래서 한 건마다 남기는 기록은 trace 에 둔다(debug 에 두면 ping 마다 한 줄이 쌓인다).
    fn settle_late_response(&mut self, plugin_id: &str, resp_id: u64) {
        tracing::trace!(
            "plugin '{plugin_id}' answered request {resp_id} that has no pending entry \
             (settled, or never tracked) — discarded"
        );
        self.clear_streak_if_late_namespace_answer(plugin_id, resp_id);
    }

    /// 만료로 이미 거둬진 namespace 호출의 **늦은 응답**이면 그 plugin 의 연속 계수를
    /// 지운다.
    ///
    /// 늦어도 답한 것은 답한 것이다. 이것이 없으면 `NAMESPACE_CALL_TIMEOUT` 을 조금씩
    /// 넘겨 답하는 plugin 이 — 매번 실제로 응답을 보내는데도 — 세 번마다 재시작된다.
    /// 재시작은 느린 것을 빠르게 만들지 못하므로 그 반복은 그 plugin 의 surface·popup
    /// 만 주기적으로 없앨 뿐이다. 계수가 가리려는 것은 **아무것도 안 답하는** plugin 이다.
    fn clear_streak_if_late_namespace_answer(&mut self, plugin_id: &str, resp_id: u64) {
        let Some(ids) = self.expired_namespace_calls.get_mut(plugin_id) else {
            return;
        };
        if let Some(pos) = ids.iter().position(|id| *id == resp_id) {
            ids.remove(pos);
            self.namespace_expiries.remove(plugin_id);
        }
    }

    /// namespace 만료 한 건을 그 target plugin 의 연속 계수에 더한다.
    ///
    /// 여기서 바로 재시작하지 않는다 — 이 함수는 sweep 루프 한가운데서 불리고,
    /// 재시작은 `pending_requests` 를 다시 훑어 거둔다. 판정은 다음 ping tick 의
    /// `restart_unresponsive_plugins` 가 healthcheck 와 같은 자리에서 한다.
    fn record_namespace_expiry(&mut self, plugin_id: &str) {
        let n = self
            .namespace_expiries
            .entry(plugin_id.to_string())
            .or_insert(0);
        *n = n.saturating_add(1);
        if *n >= super::NAMESPACE_EXPIRY_RESTART_LIMIT {
            tracing::error!(
                "plugin '{plugin_id}' let {n} namespace calls expire in a row without answering \
                 any of them — it will be restarted"
            );
        }
    }

    fn fail_open_pre_ipc_hook(
        &mut self,
        target_plugin_id: String,
        extension_plugin_id: String,
        method: String,
        params: serde_json::Value,
        final_caller: FinalCaller,
        post_hook: Option<IpcHookDecl>,
    ) {
        tracing::warn!(
            "pre-hook timeout: ext='{extension_plugin_id}' method='{method}' — fail-open"
        );
        self.record_hook_failure(&extension_plugin_id, &method);
        let post_pair = post_hook.map(|p| (extension_plugin_id.clone(), p));
        self.dispatch_target_invoke(
            target_plugin_id,
            method,
            params,
            None,
            final_caller,
            post_pair,
        );
    }

    fn fail_open_post_ipc_hook(
        &mut self,
        extension_plugin_id: String,
        method: String,
        target_outcome: TargetOutcome,
        final_caller: FinalCaller,
    ) {
        tracing::warn!(
            "post-hook timeout: ext='{extension_plugin_id}' method='{method}' — fail-open"
        );
        self.record_hook_failure(&extension_plugin_id, &method);
        self.finalize_target_outcome(final_caller, target_outcome);
    }

    fn fail_open_pre_event_hook(
        &mut self,
        publisher_plugin_id: String,
        extension_plugin_id: String,
        envelope: tasty_plugin_protocol::EventEnvelope,
        post_hook: Option<EventHookDecl>,
    ) {
        tracing::warn!(
            "pre-event-hook timeout: ext='{extension_plugin_id}' event='{}' — fail-open",
            envelope.key
        );
        self.record_hook_failure(&extension_plugin_id, &envelope.key);
        self.fan_out_then_post(
            &publisher_plugin_id,
            envelope,
            extension_plugin_id,
            post_hook,
        );
    }

    fn fail_open_post_event_hook(&mut self, extension_plugin_id: String, event_key: String) {
        tracing::warn!("post-event-hook timeout: ext='{extension_plugin_id}' event='{event_key}'");
        self.record_hook_failure(&extension_plugin_id, &event_key);
    }

    /// post-hook 응답을 처리. transform이면 result 교체, 그 외는 원 target 응답 사용.
    pub(super) fn handle_post_ipc_hook_response(
        &mut self,
        mode: HookMode,
        target_outcome: TargetOutcome,
        final_caller: FinalCaller,
        resp: PluginResponse,
    ) {
        let outcome = parse_hook_result(&resp);
        let final_outcome = match (mode, outcome, target_outcome) {
            (HookMode::Transform, HookOutcome::Modified(v), _) => TargetOutcome::Ok(v),
            (_, _, original) => original,
        };
        self.finalize_target_outcome(final_caller, final_outcome);
    }

    pub(super) fn finalize_target_outcome(
        &mut self,
        final_caller: FinalCaller,
        outcome: TargetOutcome,
    ) {
        match outcome {
            TargetOutcome::Ok(v) => self.send_final_success(final_caller, v),
            TargetOutcome::Err { message, code } => {
                self.send_final_error(final_caller, code, message);
            }
        }
    }
}

/// `PopupOpen` 응답 처리 — egui-mesh popup 은 open 응답에 별도 콘텐츠 계약이
/// 없다. 디코드만 검증하고 성공은 무시한다.
fn handle_popup_open_response(
    plugin_id: &str,
    instance_id: u64,
    result: Option<serde_json::Value>,
) {
    let Some(result_value) = result else {
        return;
    };
    if let Err(e) = serde_json::from_value::<protocol::PopupOpenResult>(result_value) {
        tracing::warn!(
            "plugin '{plugin_id}' popup.open response decode error (instance {instance_id}): {e}"
        );
    }
}

/// `NamespaceInvoke`/`DebugExtensionInvokeHook` 공통 응답 회신 — plugin 응답을
/// `JsonRpcResponse` 로 매핑해 caller 의 `response_tx` 에 그대로 전달한다.
fn send_namespace_result(
    response_tx: &mpsc::SyncSender<JsonRpcResponse>,
    original_id: serde_json::Value,
    error: Option<String>,
    error_code: Option<i32>,
    result: Option<serde_json::Value>,
) {
    let response = if let Some(err) = error {
        let code = error_code.unwrap_or(-32000);
        JsonRpcResponse::error(original_id, code, &err)
    } else {
        JsonRpcResponse::success(original_id, result.unwrap_or(serde_json::Value::Null))
    };
    send_response(response_tx, response);
}
