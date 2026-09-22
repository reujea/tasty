//! IPC dispatch: 외부 client → plugin namespace forward, plugin → plugin namespace
//! forward, extension hook 진입 routing, hook backoff/실패 카운터, 최종 응답
//! 송신 helper.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::protocol::{self, IpcCallResult, PluginRequest};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::send_response;
use tasty_plugin_manifest::{HookMode, IpcHookDecl, Permission};

use super::{
    FinalCaller, HOOK_FAIL_BACKOFF, HOOK_FAIL_LIMIT, NAMESPACE_CALL_TIMEOUT, PendingPluginCall,
    PendingRequest, PendingRequestKind, PluginManager,
};

impl PluginManager {
    pub fn take_pending_plugin_calls(&mut self) -> Vec<PendingPluginCall> {
        std::mem::take(&mut self.pending_plugin_calls)
    }

    /// 라우터가 처리한 결과를 plugin에 송신.
    ///
    /// `error_code` 는 호스트가 준 JSON-RPC 코드다. **버리면 plugin 을 거쳐 나온 응답이
    /// 전부 `-32000`(server error)이 된다** — 호스트가 "인자를 고쳐라"(`-32602`)로
    /// 거절한 것까지 그렇게 되어, 호출자가 재시도 정책을 반대로 고른다.
    /// 코드가 없는 실패(문자열만 있는 내부 경로)는 `None` 을 준다.
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

    /// 호스트 본문이 새 envelope를 발화. 호스트는 모든 namespace에 publish 가능.
    /// 매칭되는 모든 plugin 구독자에게 `event.dispatch` 송신.
    ///
    /// `origin` 은 이 호출을 낳은 IPC 요청의 호스트 번호다 — 큐에서 꺼낸 명령을 넘기는 자리는
    /// 그 명령의 [`tasty_ipc::server::IpcCommand::request_seq`] 를 준다. 번호를 모르는 호출은
    /// `None` 이다 — 파일 핸들러 큐를 거친 forward 는 IPC `file_handler.dispatch` 에서 왔더라도
    /// 큐로 옮겨지는 사이에 번호를 잃어 `None` 이다. 번호는 plugin 에게 안 가고 대기 중 표에만
    /// 실린다.
    pub fn forward_namespace_call(
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

    /// plugin이 보낸 IpcCall이 namespace 메서드인 경우의 forward.
    /// 응답은 caller plugin에 `ipc.result`로 회신한다 (`process_plugin_ipc_calls`와
    /// 같은 방식).
    pub fn forward_namespace_call_from_plugin(
        &mut self,
        method: &str,
        params: serde_json::Value,
        caller_plugin_id: &str,
        call_id: u64,
    ) {
        let plugin_id = match self.validate_namespace_call(method, Some(caller_plugin_id)) {
            Ok(id) => id,
            Err((code, msg)) => {
                self.send_ipc_result(caller_plugin_id, call_id, None, Some(msg), Some(code));
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
                    // 성사부터 센다(`deadline_from_connection`, ADR-0505).
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

    /// hook 응답에서 에러/타임아웃이 발생했을 때 호출. 연속 실패 카운터를 증가시키고
    /// 임계를 넘으면 backoff 시작.
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
                },
                None,
            ) => PendingRequestKind::PluginToPluginNamespace {
                plugin_id: target_plugin_id,
                caller_plugin_id,
                call_id,
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

    /// final_caller로 에러 응답 송신.
    ///
    /// **두 갈래가 같은 `code` 를 싣는다.** 한동안 plugin 갈래만 코드를 버렸는데,
    /// 그러면 같은 사건(예: target 이 안 돌려줬다 `-32004`)이 caller 가 CLI 냐 다른
    /// plugin 이냐에 따라 `-32004` 와 `-32000` 으로 갈렸다 — 코드가 사건이 아니라
    /// **누가 물었는가**를 보고했다. `docs/adr/0171-a-host-error-code-survives-the-plugin-boundary.md`
    /// 이 정한 축(호스트가 준 코드는 plugin 경계를 넘어 살아남는다)과 같고, 그 ADR
    /// 이 센 일곱 자리는 호스트가 *되받은* 코드였다. 여기 것은 호스트가 *스스로 내는*
    /// 코드다.
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
            } => {
                self.send_ipc_result(&caller_plugin_id, call_id, None, Some(message), Some(code));
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
            } => {
                self.send_ipc_result(&caller_plugin_id, call_id, Some(result), None, None);
            }
        }
    }

    /// 소유·caller 권한을 확인한 뒤 활성 owner만 준비한다. 성공 시 owner id를 반환.
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
        if let Some(caller) = caller_plugin_id {
            if caller == plugin_id {
                return Err((
                    -32001,
                    format!("plugin '{caller}' cannot invoke its own namespace method '{method}'"),
                ));
            }
            let prefix = method.split('.').next().unwrap_or("");
            let required = Permission::IpcInvoke(prefix.to_string());
            let allowed = self
                .plugin_permissions
                .get(caller)
                .map(|set| set.contains(&required))
                .unwrap_or(false);
            if !allowed {
                return Err((
                    -32001,
                    format!(
                        "permission_denied: plugin '{caller}' lacks 'ipc.invoke:{prefix}' \
                         permission for namespace method '{method}'"
                    ),
                ));
            }
        }
        // Does not enable, install, grant, or restart an already running owner.
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
                    ..
                }) => {
                    // 바로 위 local 갈래와 같은 `-32004` 를 싣는다. 이 짝이 갈리면
                    // 같은 취소 사건이 caller 종류에 따라 다른 코드로 나간다.
                    self.send_ipc_result(&caller_plugin_id, call_id, None, Some(msg), Some(-32004));
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

    /// 위 취소가 못 보는 나머지 — **그 plugin 에게 보냈는데** 회신할 caller 가 따로
    /// 없는 요청(`SurfaceCreate` · `SurfaceRestore` · `CommandInvoke` · `PopupOpen` ·
    /// `Other`)과 debug hook 호출을 거둔다.
    ///
    /// 거두지 않으면 영영 안 끝난다. 새 프로세스는 새 request id 를 쓰므로 옛 id 의
    /// 응답은 다시 오지 않고, deadline 도 없는 변종이라 sweep 도 안 본다. 그 항목들이
    /// 재시작마다 쌓이고, 남은 `SurfaceRestore` 는 `has_pending_surface_restores` 를
    /// 영구히 참으로 묶는다.
    ///
    /// 받는 쪽으로 찾는다(`PendingRequest::to`) — 위 취소는 *무엇을 위한* 요청인가
    /// (target) 로 찾으므로, 둘을 합쳐야 이 plugin 이 얽힌 요청이 전부 덮인다.
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
