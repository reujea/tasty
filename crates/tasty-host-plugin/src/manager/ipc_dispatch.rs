//! plugin namespace 호출, extension hook과 응답 전달을 처리한다.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::protocol::{self, IpcCallResult, PluginRequest};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::send_response;
use tasty_plugin_manifest::{HookMode, IpcHookDecl};

use super::{
    FinalCaller, HOOK_FAIL_BACKOFF, HOOK_FAIL_LIMIT, NAMESPACE_CALL_TIMEOUT, PendingPluginCall,
    PendingRequest, PendingRequestKind, PluginManager,
};

impl PluginManager {
    pub fn take_pending_plugin_calls(&mut self) -> Vec<PendingPluginCall> {
        std::mem::take(&mut self.pending_plugin_calls)
            .into_iter()
            .filter(|call| self.plugin_call_is_current(call))
            .collect()
    }

    pub fn plugin_call_is_current(&self, call: &PendingPluginCall) -> bool {
        self.processes
            .get(&call.plugin_id)
            .is_some_and(|process| process.reply_binding().ptr_eq(&call.binding))
    }

    pub fn send_plugin_call_result(
        &mut self,
        call: &PendingPluginCall,
        result: Option<serde_json::Value>,
        error: Option<String>,
        code: Option<i32>,
    ) {
        self.send_bound_ipc_parts(
            &call.plugin_id,
            &call.binding,
            call.call_id,
            result,
            error,
            code,
        );
    }

    pub(super) fn send_bound_ipc_parts(
        &mut self,
        plugin: &str,
        binding: &std::sync::Weak<()>,
        call: u64,
        result: Option<serde_json::Value>,
        error: Option<String>,
        code: Option<i32>,
    ) {
        if self
            .processes
            .get(plugin)
            .is_some_and(|process| process.reply_binding().ptr_eq(binding))
        {
            self.send_ipc_result(plugin, call, result, error, code);
        }
    }

    /// 라우터 결과를 plugin에 전달한다. 원래 JSON-RPC 오류 코드도 유지한다.
    /// 코드 없는 내부 오류에만 None을 사용한다.
    pub fn send_ipc_result(
        &mut self,
        plugin_id: &str,
        call_id: u64,
        result: Option<serde_json::Value>,
        error: Option<String>,
        error_code: Option<i32>,
    ) {
        let req = PluginRequest::new(
            protocol::METHOD_IPC_RESULT,
            serde_json::to_value(IpcCallResult {
                call_id,
                result,
                error,
                error_code,
            })
            .unwrap_or(serde_json::Value::Null),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Some(proc) = self.processes.get(plugin_id)
            && let Err(e) = proc.try_send_request(req)
        {
            tracing::warn!("plugin {plugin_id}: failed to send ipc.result: {e}");
        }
    }

    /// Reject a delayed reply if the original process owner has been replaced or retired.
    /// A true result means the binding matched; existing queue-pressure logging still applies.
    pub fn send_bound_ipc_result(
        &mut self,
        plugin_id: &str,
        binding: &std::sync::Weak<()>,
        call_id: u64,
        response: JsonRpcResponse,
    ) -> bool {
        if !self
            .processes
            .get(plugin_id)
            .is_some_and(|process| process.reply_binding().ptr_eq(binding))
        {
            return false;
        }
        let (result, error, code) = match response.error {
            Some(error) => (None, Some(error.message), Some(error.code)),
            None => (response.result, None, None),
        };
        self.send_ipc_result(plugin_id, call_id, result, error, code);
        true
    }

    /// namespace 소유자와 권한을 확인해 호출을 전달한다. 응답은 response_tx로 돌려준다.
    /// origin은 호스트의 원 요청 번호이며 plugin에는 보내지 않는다.
    /// 번호를 모르거나 중간 file-handler 큐에서 잃었으면 None이다.
    pub fn forward_namespace_call(
        &mut self,
        method: &str,
        params: serde_json::Value,
        caller_plugin_id: Option<&str>,
        original_id: serde_json::Value,
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
        origin: Option<tasty_ipc::server::RequestSeq>,
    ) {
        if let Some(marker) = tasty_ipc::agent_child::reserved_marker(&params) {
            send_response(
                &response_tx,
                JsonRpcResponse::error(original_id, -32602, reserved_message(marker)),
            );
            return;
        }
        self.forward_attach_call(
            method,
            params,
            caller_plugin_id,
            original_id,
            response_tx,
            origin,
        );
    }

    /// 원격 holder call 을 그 namespace 로 전달한다. host 가 원격 출처 표지를 넣은 호출만 이 경로를 쓴다.
    /// 일반 진입점([`Self::forward_namespace_call`])은 같은 표지를 거절한다.
    pub fn forward_attach_call(
        &mut self,
        method: &str,
        params: serde_json::Value,
        caller_plugin_id: Option<&str>,
        original_id: serde_json::Value,
        response_tx: mpsc::SyncSender<JsonRpcResponse>,
        origin: Option<tasty_ipc::server::RequestSeq>,
    ) {
        let plugin_id = match self.validate_namespace_call(method, caller_plugin_id) {
            Ok(id) => id,
            Err((code, msg)) => {
                send_response(
                    &response_tx,
                    JsonRpcResponse::error(original_id, code, &msg),
                );
                return;
            }
        };
        self.dispatch_namespace_or_hook(
            plugin_id,
            method.to_string(),
            params,
            caller_plugin_id.map(str::to_string),
            FinalCaller::Local {
                response_tx,
                original_id,
                origin,
            },
        );
    }

    /// 공통 게이트를 통과한 plugin 호출이 다른 plugin의 namespace 메서드면 소유자에게 전달하고
    /// `true`를 돌려준다. GUI와 헤드리스 런타임이 같은 규칙으로 부른다.
    /// 자기 namespace·소유자 없는 메서드·호스트가 직접 처리하는 메서드(`host_handles`)는
    /// 호스트 처리기로 가도록 `false`다.
    pub fn forward_plugin_call_to_other_namespace(
        &mut self,
        call: &PendingPluginCall,
        host_handles: bool,
    ) -> bool {
        if host_handles || !self.namespace_belongs_to_other(&call.method, &call.plugin_id) {
            return false;
        }
        self.forward_namespace_call_from_plugin(
            &call.method,
            call.params.clone(),
            &call.plugin_id,
            call.call_id,
            &call.binding,
        );
        true
    }

    /// plugin이 보낸 IpcCall이 namespace 메서드인 경우의 forward.
    /// 응답은 caller plugin에 `ipc.result`로 회신한다 (`process_plugin_ipc_calls`와
    /// 같은 방식).
    pub fn forward_namespace_call_from_plugin(
        &mut self,
        method: &str,
        params: serde_json::Value,
        caller_plugin_id: &str,
        call_id: u64,
        binding: &std::sync::Weak<()>,
    ) {
        if !self
            .processes
            .get(caller_plugin_id)
            .is_some_and(|process| process.reply_binding().ptr_eq(binding))
        {
            return;
        }
        let validated = match tasty_ipc::agent_child::reserved_marker(&params) {
            Some(marker) => Err((-32602, reserved_message(marker))),
            None => self.validate_namespace_call(method, Some(caller_plugin_id)),
        };
        let plugin_id = match validated {
            Ok(id) => id,
            Err((code, msg)) => {
                self.send_bound_ipc_parts(
                    caller_plugin_id,
                    binding,
                    call_id,
                    None,
                    Some(msg),
                    Some(code),
                );
                return;
            }
        };
        self.dispatch_namespace_or_hook(
            plugin_id,
            method.to_string(),
            params,
            Some(caller_plugin_id.to_string()),
            FinalCaller::Plugin {
                caller_plugin_id: caller_plugin_id.to_string(),
                call_id,
                binding: binding.clone(),
            },
        );
    }

    /// validate를 통과한 namespace 호출을 hook-aware하게 분기.
    ///
    /// - 활성 extension이 있고 pre-hook이 매칭되면 → extension.invoke_hook 먼저 송신.
    /// - 그 외에는 기존처럼 target에 바로 ipc.invoke 송신. 매칭 post-hook이 있으면
    ///   응답 수신 시 post-hook으로 chain.
    /// - caller가 extension 자신이면 self-loop 방지를 위해 hook을 건너뛴다.
    pub(super) fn dispatch_namespace_or_hook(
        &mut self,
        target_plugin_id: String,
        method: String,
        params: serde_json::Value,
        caller_plugin_id: Option<String>,
        final_caller: FinalCaller,
    ) {
        // Metadata-only startup also needs the configured extension selection.
        self.recompute_extensions();
        let extension_self = match (
            caller_plugin_id.as_deref(),
            self.extensions
                .active_extension_for_target(&target_plugin_id),
        ) {
            (Some(c), Some(e)) => c == e,
            _ => false,
        };

        let active_ext_with_hooks = if extension_self {
            None
        } else {
            self.find_active_ipc_hooks(&target_plugin_id, &method)
        };

        match active_ext_with_hooks {
            Some((ext_id, pre_opt, post_opt)) => {
                // backoff 중인 hook은 우회.
                let pre = pre_opt.filter(|p| !self.is_hook_in_backoff(&ext_id, &p.method));
                let post = post_opt.filter(|p| !self.is_hook_in_backoff(&ext_id, &p.method));

                // Only a hook that will actually run needs its extension process.
                if pre.is_some() || post.is_some() {
                    self.start_one_enabled(&ext_id);
                }
                if let Some(pre) = pre {
                    let payload = serde_json::json!({
                        "method": method,
                        "params": params,
                        "caller_plugin_id": caller_plugin_id,
                    });
                    // 막 띄운 extension 이면 아직 연결 중이다 — 이 시한은 sweep 이 연결
                    // 성사부터 센다(`deadline_from_connection`, docs/dev-guide/plugin-development.md#생명주기-healthcheck--자동-재시작비활성화).
                    let deadline = Instant::now() + Duration::from_millis(pre.timeout_ms as u64);
                    let origin = final_caller.origin();
                    match self.send_extension_invoke_hook(
                        &ext_id,
                        tasty_plugin_protocol::ExtensionHookKind::Ipc,
                        tasty_plugin_protocol::ExtensionHookPhase::Pre,
                        pre.mode,
                        &method,
                        payload,
                    ) {
                        Ok(req_id) => {
                            self.insert_pending(
                                req_id,
                                PendingRequest::now(
                                    ext_id.clone(),
                                    PendingRequestKind::ExtensionPreIpcHook {
                                        target_plugin_id,
                                        extension_plugin_id: ext_id,
                                        method,
                                        params,
                                        pre_hook_mode: pre.mode,
                                        final_caller,
                                        post_hook: post,
                                        deadline,
                                    },
                                )
                                .for_request(origin),
                            );
                        }
                        Err(msg) => {
                            tracing::warn!("pre-hook dispatch failed: {msg}; bypassing hook");
                            self.dispatch_target_invoke(
                                target_plugin_id,
                                method,
                                params,
                                caller_plugin_id.as_deref(),
                                final_caller,
                                post.map(|p| (ext_id.clone(), p)),
                            );
                        }
                    }
                } else if let Some(post) = post {
                    self.dispatch_target_invoke(
                        target_plugin_id,
                        method,
                        params,
                        caller_plugin_id.as_deref(),
                        final_caller,
                        Some((ext_id, post)),
                    );
                } else {
                    self.dispatch_target_invoke(
                        target_plugin_id,
                        method,
                        params,
                        caller_plugin_id.as_deref(),
                        final_caller,
                        None,
                    );
                }
            }
            None => {
                self.dispatch_target_invoke(
                    target_plugin_id,
                    method,
                    params,
                    caller_plugin_id.as_deref(),
                    final_caller,
                    None,
                );
            }
        }
    }

    /// (ext_id, method) 페어가 backoff 중인지 검사. 만료되면 즉시 클리어해서 그 후
    /// 호출은 정상 hook 경로를 탄다.
    pub(super) fn is_hook_in_backoff(&mut self, ext_id: &str, method: &str) -> bool {
        let key = (ext_id.to_string(), method.to_string());
        let now = Instant::now();
        if let Some(state) = self.hook_failures.get(&key)
            && let Some(until) = state.backoff_until
            && now < until
        {
            return true;
        }
        // 만료 시 상태 정리.
        if let Some(state) = self.hook_failures.get_mut(&key)
            && let Some(until) = state.backoff_until
            && now >= until
        {
            state.backoff_until = None;
            state.consecutive_failures = 0;
        }
        false
    }

    /// hook 오류·시간 초과를 기록하고 실패 횟수가 상한에 도달하면 backoff를 시작한다.
    pub(super) fn record_hook_failure(&mut self, ext_id: &str, method: &str) {
        let key = (ext_id.to_string(), method.to_string());
        let state = self.hook_failures.entry(key).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= HOOK_FAIL_LIMIT && state.backoff_until.is_none() {
            state.backoff_until = Some(Instant::now() + HOOK_FAIL_BACKOFF);
            tracing::warn!(
                "extension '{ext_id}' hook on '{method}' entered {}s backoff after {} consecutive failures",
                HOOK_FAIL_BACKOFF.as_secs(),
                state.consecutive_failures
            );
        }
    }

    /// hook이 정상 응답하면 호출. 카운터를 0으로 리셋.
    pub(super) fn record_hook_success(&mut self, ext_id: &str, method: &str) {
        let key = (ext_id.to_string(), method.to_string());
        if let Some(state) = self.hook_failures.get_mut(&key) {
            state.consecutive_failures = 0;
            state.backoff_until = None;
        }
    }

    /// target plugin에 실제 ipc.invoke 송신. post-hook이 주어지면 응답 후 chain.
    pub(super) fn dispatch_target_invoke(
        &mut self,
        target_plugin_id: String,
        method: String,
        params: serde_json::Value,
        caller_plugin_id: Option<&str>,
        final_caller: FinalCaller,
        post_hook: Option<(String, IpcHookDecl)>,
    ) {
        let req_id =
            match self.send_namespace_invoke(&target_plugin_id, &method, &params, caller_plugin_id)
            {
                Ok(id) => id,
                Err(msg) => {
                    self.send_final_error(final_caller, -32003, msg);
                    return;
                }
            };
        let deadline = Instant::now() + NAMESPACE_CALL_TIMEOUT;
        let to = target_plugin_id.clone();
        let origin = final_caller.origin();
        let kind = match (final_caller, post_hook) {
            (
                FinalCaller::Local {
                    response_tx,
                    original_id,
                    origin: _,
                },
                None,
            ) => PendingRequestKind::NamespaceInvoke {
                plugin_id: target_plugin_id,
                response_tx,
                original_id,
                deadline,
            },
            (
                FinalCaller::Plugin {
                    caller_plugin_id,
                    call_id,
                    binding,
                },
                None,
            ) => PendingRequestKind::PluginToPluginNamespace {
                plugin_id: target_plugin_id,
                caller_plugin_id,
                call_id,
                caller_binding: binding,
                deadline,
            },
            (fc, Some((ext_id, decl))) => PendingRequestKind::NamespaceInvokeWithPostHook {
                target_plugin_id,
                method,
                extension_plugin_id: ext_id,
                post_hook_decl: decl,
                final_caller: fc,
                deadline,
            },
        };
        self.insert_pending(req_id, PendingRequest::now(to, kind).for_request(origin));
    }

    /// 활성 extension이 있고 method에 매칭되는 pre/post IPC hook을 검색.
    /// 둘 다 없으면 `None` 반환.
    pub(super) fn find_active_ipc_hooks(
        &self,
        target_plugin_id: &str,
        method: &str,
    ) -> Option<(String, Option<IpcHookDecl>, Option<IpcHookDecl>)> {
        let ext_id = self
            .extensions
            .active_extension_for_target(target_plugin_id)?
            .to_string();
        let pkg = self.packages.iter().find(|p| p.manifest.id == ext_id)?;
        let extends = pkg.manifest.extends.as_ref()?;
        let pre = extends.pre_ipc.iter().find(|h| h.method == method).cloned();
        let post = extends
            .post_ipc
            .iter()
            .find(|h| h.method == method)
            .cloned();
        if pre.is_none() && post.is_none() {
            None
        } else {
            Some((ext_id, pre, post))
        }
    }

    /// extension에 `extension.invoke_hook` 송신. 성공 시 req_id 반환.
    pub(super) fn send_extension_invoke_hook(
        &self,
        extension_plugin_id: &str,
        kind: tasty_plugin_protocol::ExtensionHookKind,
        phase: tasty_plugin_protocol::ExtensionHookPhase,
        mode: HookMode,
        target: &str,
        payload: serde_json::Value,
    ) -> Result<u64, String> {
        let proc = self
            .processes
            .get(extension_plugin_id)
            .ok_or_else(|| format!("extension plugin '{extension_plugin_id}' is not running"))?;
        let mode_str = match mode {
            HookMode::Transform => "transform",
            HookMode::Filter => "filter",
            HookMode::Observe => "observe",
        };
        let kind_str = match kind {
            tasty_plugin_protocol::ExtensionHookKind::Event => "event",
            tasty_plugin_protocol::ExtensionHookKind::Ipc => "ipc",
        };
        let phase_str = match phase {
            tasty_plugin_protocol::ExtensionHookPhase::Pre => "pre",
            tasty_plugin_protocol::ExtensionHookPhase::Post => "post",
        };
        let req_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let req = PluginRequest::new(
            tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK,
            serde_json::json!({
                "kind": kind_str,
                "phase": phase_str,
                "mode": mode_str,
                "target": target,
                "payload": payload,
            }),
            req_id,
        );
        proc.try_send_request(req)
            .map_err(|e| format!("extension '{extension_plugin_id}' send failed: {e}"))?;
        Ok(req_id)
    }

    /// 최종 호출자에게 오류를 보낸다. Local과 plugin 모두 호스트의 오류 코드를 유지한다.
    pub(super) fn send_final_error(
        &mut self,
        final_caller: FinalCaller,
        code: i32,
        message: String,
    ) {
        match final_caller {
            FinalCaller::Local {
                response_tx,
                original_id,
                origin: _,
            } => {
                send_response(
                    &response_tx,
                    JsonRpcResponse::error(original_id, code, &message),
                );
            }
            FinalCaller::Plugin {
                caller_plugin_id,
                call_id,
                binding,
            } => {
                self.send_bound_ipc_parts(
                    &caller_plugin_id,
                    &binding,
                    call_id,
                    None,
                    Some(message),
                    Some(code),
                );
            }
        }
    }

    /// final_caller로 성공 응답 송신.
    pub(super) fn send_final_success(
        &mut self,
        final_caller: FinalCaller,
        result: serde_json::Value,
    ) {
        match final_caller {
            FinalCaller::Local {
                response_tx,
                original_id,
                origin: _,
            } => {
                send_response(&response_tx, JsonRpcResponse::success(original_id, result));
            }
            FinalCaller::Plugin {
                caller_plugin_id,
                call_id,
                binding,
            } => {
                self.send_bound_ipc_parts(
                    &caller_plugin_id,
                    &binding,
                    call_id,
                    Some(result),
                    None,
                    None,
                );
            }
        }
    }

    /// 소유자를 확인하고 자기 namespace 호출을 거절한 뒤 활성 owner만 준비한다. 성공 시 owner id를 반환.
    /// 실패 시 (JSON-RPC code, message) 페어를 반환.
    pub(super) fn validate_namespace_call(
        &mut self,
        method: &str,
        caller_plugin_id: Option<&str>,
    ) -> Result<String, (i32, String)> {
        let plugin_id = self
            .namespaces_read()
            .resolve(method)
            .map(str::to_string)
            .ok_or_else(|| (-32601, format!("method '{method}' not found")))?;
        if let Some(caller) = caller_plugin_id
            && caller == plugin_id
        {
            return Err((
                -32001,
                format!("plugin '{caller}' cannot invoke its own namespace method '{method}'"),
            ));
        }
        // Does not enable, install, or restart an already running owner.
        self.start_one_enabled(&plugin_id);
        if !self.processes.contains_key(&plugin_id) {
            return Err((-32002, format!("plugin '{plugin_id}' is not running")));
        }
        Ok(plugin_id)
    }

    /// target plugin에 `ipc.invoke` 요청을 송신한다. 성공 시 발급된 host→plugin
    /// request id를 반환. caller는 pending_requests에 추적 kind를 직접 삽입한다.
    pub(super) fn send_namespace_invoke(
        &self,
        plugin_id: &str,
        method: &str,
        params: &serde_json::Value,
        caller_plugin_id: Option<&str>,
    ) -> Result<u64, String> {
        let proc = self
            .processes
            .get(plugin_id)
            .ok_or_else(|| format!("plugin '{plugin_id}' is not running"))?;
        let req_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let req = PluginRequest::new(
            tasty_plugin_protocol::ipc_method::METHOD_IPC_INVOKE,
            json!({
                "method": method,
                "params": params,
                "caller_plugin_id": caller_plugin_id,
            }),
            req_id,
        );
        proc.try_send_request(req)
            .map_err(|e| format!("plugin '{plugin_id}' send failed: {e}"))?;
        Ok(req_id)
    }

    /// 죽거나 비활성화된 plugin이 가진 모든 namespace pending에 에러 응답을 보내고
    /// pending에서 제거한다. CLI/Local caller에게는 response_tx로, plugin caller에게는
    /// `ipc.result`로 회신한다.
    pub(super) fn cancel_pending_namespace_calls(&mut self, plugin_id: &str, reason: &str) {
        // 이 plugin 은 치워지는 중이다 — 연속 만료 계수는 다음 기동에 넘기지 않는다.
        self.namespace_expiries.remove(plugin_id);
        self.expired_namespace_calls.remove(plugin_id);
        let to_cancel: Vec<u64> = self
            .pending_requests
            .iter()
            .filter_map(|(id, p)| match &p.kind {
                PendingRequestKind::SurfaceRetire { .. } if p.to == plugin_id => Some(*id),
                PendingRequestKind::NamespaceInvoke { plugin_id: pid, .. }
                | PendingRequestKind::PluginToPluginNamespace { plugin_id: pid, .. }
                | PendingRequestKind::NamespaceInvokeWithPostHook {
                    target_plugin_id: pid,
                    ..
                } if pid == plugin_id => Some(*id),
                PendingRequestKind::ExtensionPreIpcHook {
                    target_plugin_id: pid,
                    extension_plugin_id: epid,
                    ..
                } if pid == plugin_id || epid == plugin_id => Some(*id),
                PendingRequestKind::ExtensionPostIpcHook {
                    extension_plugin_id: epid,
                    ..
                } if epid == plugin_id => Some(*id),
                PendingRequestKind::ExtensionPreEventHook {
                    publisher_plugin_id: pid,
                    extension_plugin_id: epid,
                    ..
                } if pid == plugin_id || epid == plugin_id => Some(*id),
                PendingRequestKind::ExtensionPostEventHook {
                    extension_plugin_id: epid,
                    ..
                } if epid == plugin_id => Some(*id),
                _ => None,
            })
            .collect();
        for id in to_cancel {
            let msg = format!("plugin '{plugin_id}' unavailable: {reason}");
            let removed = self.pending_requests.remove(&id);
            if let Some(p) = &removed {
                // plugin 이 치워지면 사슬은 여기서 끝난다 — 모든 갈래가 caller 에 오류로 답한다.
                self.record_origin_hop(
                    id,
                    p,
                    p.sent_at.elapsed(),
                    tasty_telemetry::slow_requests::HopOutcome::Cancelled,
                    true,
                );
            }
            match removed.map(|p| p.kind) {
                Some(PendingRequestKind::SurfaceRetire {
                    completion,
                    process_binding,
                }) => self.settle_lost_retire_ack(plugin_id, &process_binding, completion, msg),
                Some(PendingRequestKind::NamespaceInvoke {
                    response_tx,
                    original_id,
                    ..
                }) => {
                    send_response(
                        &response_tx,
                        JsonRpcResponse::error(original_id, -32004, &msg),
                    );
                }
                Some(PendingRequestKind::PluginToPluginNamespace {
                    caller_plugin_id,
                    call_id,
                    caller_binding,
                    ..
                }) => {
                    // 바로 위 local 갈래와 같은 `-32004` 를 싣는다. 이 짝이 갈리면
                    // 같은 취소 사건이 caller 종류에 따라 다른 코드로 나간다.
                    self.send_bound_ipc_parts(
                        &caller_plugin_id,
                        &caller_binding,
                        call_id,
                        None,
                        Some(msg),
                        Some(-32004),
                    );
                }
                Some(PendingRequestKind::ExtensionPreIpcHook { final_caller, .. })
                | Some(PendingRequestKind::ExtensionPostIpcHook { final_caller, .. })
                | Some(PendingRequestKind::NamespaceInvokeWithPostHook { final_caller, .. }) => {
                    self.send_final_error(final_caller, -32004, msg);
                }
                Some(PendingRequestKind::ExtensionPreEventHook { .. })
                | Some(PendingRequestKind::ExtensionPostEventHook { .. }) => {
                    // event는 fire-and-forget이라 caller에 회신할 필요 없음.
                }
                _ => {}
            }
        }
        self.reclaim_requests_sent_to(plugin_id, reason);
    }

    /// 해당 plugin에 보낸 나머지 요청도 수신자 기준으로 정리한다.
    /// caller나 deadline이 없는 surface·명령·popup 요청도 포함해 재시작 뒤 남지 않게 한다.
    fn reclaim_requests_sent_to(&mut self, plugin_id: &str, reason: &str) {
        let leftover: Vec<u64> = self
            .pending_requests
            .iter()
            .filter(|(_, p)| p.to == plugin_id)
            .map(|(id, _)| *id)
            .collect();
        if !leftover.is_empty() {
            tracing::debug!(
                "reclaimed {} request(s) sent to plugin '{plugin_id}' ({reason})",
                leftover.len()
            );
        }
        for id in leftover {
            match self.pending_requests.remove(&id).map(|p| p.kind) {
                #[cfg(debug_assertions)]
                Some(PendingRequestKind::DebugExtensionInvokeHook {
                    response_tx,
                    original_id,
                    ..
                }) => {
                    let msg = format!("plugin '{plugin_id}' unavailable: {reason}");
                    send_response(
                        &response_tx,
                        JsonRpcResponse::error(original_id, -32004, &msg),
                    );
                }
                // 회신할 caller 가 없다 — 응답은 호스트 자신의 상태 동기화용이었다.
                _ => {}
            }
        }
    }
}

/// 일반 namespace 요청에 원격 출처 표지가 실렸을 때의 오류.
fn reserved_message(marker: &str) -> String {
    format!("'{marker}' is reserved for remote attach calls that the host runs for a holder")
}
