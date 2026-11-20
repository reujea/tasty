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

/// namespace 요청이 만료됐을 때 호출자에게 보낼 메시지.
fn namespace_timeout_message(plugin_id: &str) -> String {
    format!(
        "plugin '{plugin_id}' did not answer within {}s",
        NAMESPACE_CALL_TIMEOUT.as_secs()
    )
}

/// 원 IPC 요청 번호. IPC 외부에서 시작했거나 전달 중 번호가 없어졌으면 none.
fn request_seq_label(origin: Option<tasty_ipc::server::RequestSeq>) -> String {
    origin.map_or_else(|| "none".to_string(), |seq| seq.to_string())
}

// 표시 이름·스냅샷 슬롯의 poison을 각각 처음 한 번만 알린다.
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

    /// 호스트와 공유할 플러그인 왕복 대기 통계를 주입한다.
    pub fn set_plugin_wait(&mut self, stats: Arc<tasty_telemetry::PluginWaitStats>) {
        self.plugin_wait = Some(stats);
    }

    /// 플러그인 채널의 대기 바이트·상한·거절·대기 통계를 읽는다.
    /// system.pressure에는 아직 이 값을 포함하지 않는다.
    pub fn channel_bytes(&self) -> crate::process::channel_bytes::ChannelBytesSnapshot {
        self.channel_ledger.snapshot()
    }

    /// 호스트 처리와 플러그인 경유 시간을 같은 요청에 기록할 공용 로그를 주입한다.
    pub fn set_slow_requests(&mut self, log: Arc<tasty_telemetry::SlowRequestLog>) {
        self.slow_requests = Some(log);
    }

    /// 플러그인 요청을 기다리기 전에 원 IPC 요청의 기록을 연다.
    /// 호스트 처리만 빨랐더라도 플러그인 응답까지 집계할 수 있어야 한다.
    pub(super) fn insert_pending(&mut self, req_id: u64, pending: PendingRequest) {
        if let (Some(log), Some(seq)) = (&self.slow_requests, pending.origin) {
            log.note_forwarded(seq.get());
        }
        self.pending_requests.insert(req_id, pending);
    }

    /// 원 IPC 요청 번호가 있으면 플러그인 경유 시간을 기록한다.
    /// last는 이후 hook이나 target 호출이 이어지지 않는 마지막 단계인지 나타낸다.
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

    /// 매칭된 응답의 왕복 대기 시간을 기록한다.
    fn record_plugin_wait(&self, waited: std::time::Duration) {
        if let Some(stats) = &self.plugin_wait {
            stats.record(waited);
        }
    }

    pub(super) fn handle_plugin_response(&mut self, plugin_id: &str, resp: PluginResponse) {
        if self.pending_requests.get(&resp.id).is_some_and(|pending| {
            matches!(pending.kind, PendingRequestKind::SurfaceRetire { .. })
                && pending.to != plugin_id
        }) {
            tracing::warn!("remote retirement response came from another plugin");
            return;
        }
        let pending = self.pending_requests.remove(&resp.id);
        self.record_response_observation(plugin_id, &resp, pending.as_ref());
        let kind = match pending {
            Some(p) => p.kind,
            None => {
                // event.dispatch 응답은 pending 대신 EventBus의 재발행 추적 기록에서 처리한다.
                if self.event_bus.note_dispatch_answered(plugin_id, resp.id) {
                    return;
                }
                // id 가 안 맞는 응답 — 이미 만료·취소돼 거둬진 것이다.
                self.settle_late_response(plugin_id, resp.id);
                return;
            }
        };
        // namespace 응답은 연속 만료 기록을 지운다. pong이나 다른 요청의 응답은 제외한다.
        if matches!(
            kind,
            PendingRequestKind::NamespaceInvoke { .. }
                | PendingRequestKind::PluginToPluginNamespace { .. }
                | PendingRequestKind::NamespaceInvokeWithPostHook { .. }
        ) {
            self.namespace_expiries.remove(plugin_id);
        }
        match kind {
            PendingRequestKind::SurfaceCreate {
                surface_id,
                binding,
            }
            | PendingRequestKind::SurfaceRestore {
                surface_id,
                binding,
            } => {
                self.apply_surface_response(plugin_id, surface_id, &binding, resp.result);
            }
            PendingRequestKind::CommandInvoke {
                surface_id,
                binding,
            } => {
                if let Some(binding) = binding {
                    self.apply_surface_response(plugin_id, surface_id, &binding, resp.result);
                }
            }
            PendingRequestKind::SurfaceRetire {
                completion,
                process_binding,
            } => {
                self.finish_surface_retirement(plugin_id, completion, process_binding, resp);
            }
            PendingRequestKind::Other => {}
            PendingRequestKind::PopupOpen { instance_id } => match resp.error {
                Some(error) => self.fail_popup_open(plugin_id, instance_id, &error),
                None => handle_popup_open_response(plugin_id, instance_id, resp.result),
            },
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
                caller_binding,
                deadline: _,
            } => {
                // 플러그인 호출자에게도 원 오류 코드를 전달한다.
                self.send_bound_ipc_parts(
                    &caller_plugin_id,
                    &caller_binding,
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

    // Observe only the matched wait, but log both matched and late error responses.
    fn record_response_observation(
        &self,
        plugin_id: &str,
        resp: &PluginResponse,
        pending: Option<&PendingRequest>,
    ) {
        // 이미 만료·취소된 요청처럼 매칭되지 않은 응답은 왕복 시간에 넣지 않는다.
        if let Some(p) = pending {
            let waited = p.sent_at.elapsed();
            self.record_plugin_wait(waited);
            let outcome = if resp.error.is_some() {
                tasty_telemetry::slow_requests::HopOutcome::Error
            } else {
                tasty_telemetry::slow_requests::HopOutcome::Ok
            };
            // pre-hook 또는 post-hook이 남은 target 응답이면 후속 처리가 이어진다.
            let last = !matches!(
                p.kind,
                PendingRequestKind::ExtensionPreIpcHook { .. }
                    | PendingRequestKind::NamespaceInvokeWithPostHook { .. }
            );
            self.record_origin_hop(resp.id, p, waited, outcome, last);
        }
        if let Some(err) = &resp.error {
            // 플러그인 로그와 호스트 요청을 함께 추적할 수 있도록 원 요청 번호도 남긴다.
            tracing::warn!(
                "plugin '{plugin_id}' response error (id={}, request_seq={}): {err}",
                resp.id,
                request_seq_label(pending.and_then(|p| p.origin))
            );
        }
    }

    // Only an ACK from the captured live process completes this retirement receipt.
    fn finish_surface_retirement(
        &self,
        plugin_id: &str,
        completion: crate::host_cmd::RemoteRetirementCompletion,
        process_binding: std::sync::Weak<()>,
        resp: PluginResponse,
    ) {
        let current = self
            .processes
            .get(plugin_id)
            .map(|process| process.reply_binding());
        if current
            .as_ref()
            .is_none_or(|current| !current.ptr_eq(&process_binding))
            || process_binding.upgrade().is_none()
        {
            completion.finish(Err(
                "destruction response belongs to a retired plugin process".into(),
            ));
        } else if let Some(error) = resp.error {
            completion.finish(Err(format!("plugin rejected surface destruction: {error}")));
        } else {
            completion.finish(Ok(()));
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
        binding: &crate::host_cmd::SurfaceBinding,
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
        if entry.plugin_id != plugin_id || !binding.matches(&entry.handles) {
            return;
        }
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

    /// pump가 받은 시각으로 만료 요청을 처리한다.
    /// hook은 원래 흐름을 계속 진행하고, namespace 호출은 오류를 반환한다.
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

    /// 연결된 플러그인에 보낸 hook·namespace 요청 중 deadline을 넘긴 id를 모은다.
    /// 연결 대기 중에는 deadline_from_connection이 만료를 유예한다.
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

    /// 만료된 hook은 원래 흐름을 진행하고 실패를 기록한다.
    /// namespace 호출은 호출자에게 -32004를 반환한다.
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

    /// namespace 호출의 만료는 호출자 종류에 관계없이 -32004로 회신한다.
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
                caller_binding,
                deadline: _,
            } => {
                let msg = self.note_namespace_expiry(&plugin_id, id, origin);
                self.send_bound_ipc_parts(
                    &caller_plugin_id,
                    &caller_binding,
                    call_id,
                    None,
                    Some(msg),
                    Some(-32004),
                );
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
            // 디버그 직접 호출은 이어갈 원래 요청이 없으므로 오류로 회신한다.
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

    /// namespace 만료를 세고 회신 메시지를 만든다.
    /// 로그에는 플러그인 요청 id와 원 IPC 요청 번호를 함께 남긴다.
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

    /// 늦은 응답을 식별할 요청 id를 재시작 기준 개수만큼 보관한다.
    /// 그보다 오래된 id는 버리므로 모든 늦은 응답을 기억하지는 않는다.
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

    /// pending에 없는 응답을 처리한다. 이미 끝난 요청의 hook·target 호출이나 회신을
    /// 반복하지 않으며, 만료된 hook의 실패 기록도 되돌리지 않는다.
    /// 기억하고 있는 namespace 요청의 늦은 응답이면 연속 만료 기록만 지운다.
    /// ping처럼 처음부터 pending에 넣지 않은 응답도 오므로 개별 로그는 trace로 남긴다.
    fn settle_late_response(&mut self, plugin_id: &str, resp_id: u64) {
        tracing::trace!(
            "plugin '{plugin_id}' answered request {resp_id} that has no pending entry \
             (settled, or never tracked) — discarded"
        );
        self.clear_streak_if_late_namespace_answer(plugin_id, resp_id);
    }

    /// 기억하고 있는 만료 요청의 늦은 응답이면 연속 만료 기록을 지운다.
    /// 느리더라도 응답하는 플러그인의 불필요한 재시작을 줄이기 위한 처리다.
    fn clear_streak_if_late_namespace_answer(&mut self, plugin_id: &str, resp_id: u64) {
        let Some(ids) = self.expired_namespace_calls.get_mut(plugin_id) else {
            return;
        };
        if let Some(pos) = ids.iter().position(|id| *id == resp_id) {
            ids.remove(pos);
            self.namespace_expiries.remove(plugin_id);
        }
    }

    /// namespace 연속 만료를 센다. 실제 재시작은 다음 ping tick에서 판정한다.
    /// 재시작도 pending을 순회하므로 이 만료 처리 중에는 실행하지 않는다.
    fn record_namespace_expiry(&mut self, plugin_id: &str) {
        let n = self
            .namespace_expiries
            .entry(plugin_id.to_string())
            .or_insert(0);
        *n = n.saturating_add(1);
        if *n >= super::NAMESPACE_EXPIRY_RESTART_LIMIT {
            tracing::error!(
                "plugin '{plugin_id}' let {n} namespace calls expire in a row without answering \
                 any of them — restart threshold reached; checked on the next ping tick"
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

#[cfg(test)]
mod binding_tests {
    use super::super::RemoteSurfaceEntry;
    use super::*;
    use crate::host_cmd::SurfaceHandles;

    fn handles(name: &str) -> SurfaceHandles {
        SurfaceHandles {
            display_name: Arc::new(std::sync::Mutex::new(name.into())),
            snapshot_cache: Arc::default(),
        }
    }

    #[test]
    fn a_late_create_response_cannot_mutate_a_same_id_same_plugin_replacement() {
        let mut manager =
            PluginManager::new(Arc::new(tasty_terminal::waker_factory::NoopWakerFactory));
        let old = handles("old");
        let fresh = handles("fresh");
        let plugin = "com.test.kind";
        manager.surfaces.insert(
            7,
            RemoteSurfaceEntry {
                plugin_id: plugin.into(),
                handles: fresh.clone(),
                publication: super::super::RemotePublication::NeverSent,
                origin: super::super::reattach::SurfaceOrigin::create(serde_json::json!({})),
            },
        );
        for (id, binding, name) in [(1, old.binding(), "stale"), (2, fresh.binding(), "current")] {
            manager.pending_requests.insert(
                id,
                PendingRequest::now(
                    plugin,
                    PendingRequestKind::SurfaceCreate {
                        surface_id: 7,
                        binding,
                    },
                ),
            );
            manager.handle_plugin_response(plugin, PluginResponse { id, result: Some(serde_json::json!({"surface_id":7, "display_name":name, "snapshot":{"name":name}})), error:None, error_code:None });
            assert_eq!(*old.display_name.lock().unwrap(), "old");
            assert_eq!(
                *fresh.display_name.lock().unwrap(),
                if id == 1 { "fresh" } else { "current" }
            );
            assert_eq!(
                *fresh.snapshot_cache.lock().unwrap(),
                (id == 2).then(|| serde_json::json!({"name":"current"}))
            );
        }
    }
}
