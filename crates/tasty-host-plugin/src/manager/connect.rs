//! 연결 대기의 결과를 거둔다 — 연결 대기 자체는 `process::connect` 가 메인 스레드 밖에서 한다.
//!
//! 기동은 자식을 띄우자마자 `processes` 에 넣는다. 그래서 이 모듈이 할 일은 셋이다 —
//! 연결이 성사되면 연속 실패 기록을 지우고, 끝내 안 오면 예전 spawn 실패와 같은 갈래
//! (`plugin.error` · 연속 실패 누적 · 자동 비활성, 그 extension 에 보낸 hook 은 hook 없이
//! 진행)로 보내고, 연결 전에 보낸 요청의 시한을 연결 성사부터 세게 한다. 근거·대안은 ADR-0505.

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use crate::process::connect::{ConnectOutcome, HANDSHAKE_TIMEOUT};

use super::{FinalCaller, PendingRequestKind, PluginManager};

/// 전체 기동이 연결을 기다려 주는 한도의 여유 — 연결 대기 스레드는 연결 한도
/// ([`HANDSHAKE_TIMEOUT`])에서 스스로 실패를 적으므로, 그 결과를 받을 만큼만 더 기다린다.
const SETTLE_MARGIN: Duration = Duration::from_millis(500);

impl PluginManager {
    /// 연결 대기가 끝난 plugin 을 거둔다 — 기다리지 않는다. pump 가 매번 부른다.
    pub(super) fn settle_connections(&mut self) {
        let outcomes: Vec<(String, ConnectOutcome)> = self
            .processes
            .iter()
            .filter_map(|(id, p)| p.take_connect_outcome().map(|o| (id.clone(), o)))
            .collect();
        for (id, outcome) in outcomes {
            match outcome {
                ConnectOutcome::Connected { at, waited } => {
                    tracing::info!(
                        plugin_id = id,
                        ms = waited.as_secs_f64() * 1000.0,
                        "plugin connected"
                    );
                    if let Some(proc) = self.processes.get_mut(&id) {
                        proc.mark_connected(at);
                    }
                    self.spawn_failures.remove(&id);
                }
                ConnectOutcome::Failed(reason) => self.on_connect_failure(&id, reason),
            }
        }
    }

    /// 연결이 끝내 안 온 plugin 을 내리고 기동 실패로 센다.
    ///
    /// 프로세스는 요청 없이 곧바로 kill 한다 — 읽을 소켓이 없다. kill 과 회수는 회수
    /// 스레드가 한다(메인 스레드가 커널 안에서 멈춘 자식을 `wait` 하지 않게). 연결을
    /// 기다리던 사이 쌓인 namespace 호출은 caller 에 오류로 회신한다.
    fn on_connect_failure(&mut self, plugin_id: &str, reason: String) {
        if let Some(proc) = self.processes.remove(plugin_id) {
            self.retire_pending(plugin_id, proc.abandon(), false);
        }
        self.bypass_hooks_sent_to(plugin_id);
        self.forget_plugin_runtime(plugin_id, "plugin did not connect");
        self.on_plugin_spawn_failure(plugin_id, anyhow::anyhow!(reason));
    }

    /// 지금 연결 중인 plugin 전부의 결과가 날 때까지 기다린 뒤 거둔다 — 전체 기동
    /// (`discover_and_start`)이 부른다. 연결 대기는 plugin 마다 스레드라 겹치므로 전체
    /// 대기는 가장 느린 하나로 수렴한다.
    pub(super) fn wait_for_connections(&mut self) {
        let deadline = Instant::now() + self.connection_wait_limit();
        for proc in self.processes.values() {
            proc.wait_connect_settled(deadline);
        }
        self.settle_connections();
    }

    /// 연결 결과가 나기를 기다려 줄 최대 시간 — 연결 한도에 그 결과를 받을 여유를 더한
    /// 것이다. 연결 대기 스레드가 한도에서 스스로 실패를 적으므로 이보다 오래 연결 중으로
    /// 남는 plugin 은 없다. 예전에 spawn 안에서 막히던 시간과 같은 값이다.
    pub fn connection_wait_limit(&self) -> Duration {
        self.listener
            .as_ref()
            .map_or(HANDSHAKE_TIMEOUT, |l| l.handshake_timeout())
            + SETTLE_MARGIN
    }

    /// 이 plugin 이 떠 있지만 아직 연결 결과가 안 거둬졌는가.
    pub fn is_connecting(&self, plugin_id: &str) -> bool {
        self.processes
            .get(plugin_id)
            .is_some_and(|p| p.connected_at().is_none())
    }

    /// 요청 하나의 시한을 **연결 성사부터** 센 값. 받는 plugin 이 아직 연결 중이면 `None`
    /// (아직 만료를 따지지 않는다).
    ///
    /// 예전에는 기동이 연결까지 막혔으므로 기동 직후 보낸 요청의 시한은 연결 뒤에 섰다.
    /// 지금은 기동이 곧바로 돌아오므로 그대로 두면 짧은 시한(pre-hook 의 `timeout_ms`)이
    /// 연결 시간까지 떠안는다. 그래서 연결 전에 보낸 요청은 연결까지 기다린 만큼 시한을
    /// 민다 — 시한의 **길이**는 그대로다. 연결에 끝내 실패하면 그 요청은 시한이 아니라
    /// 연결 실패 갈래([`Self::on_connect_failure`])에서 끝난다. 근거는 ADR-0505.
    pub(super) fn deadline_from_connection(
        &self,
        to: &str,
        sent_at: Instant,
        deadline: Instant,
    ) -> Option<Instant> {
        let Some(proc) = self.processes.get(to) else {
            return Some(deadline);
        };
        let connected_at = proc.connected_at()?;
        Some(deadline + connected_at.saturating_duration_since(sent_at))
    }

    /// 연결하지 못한 extension 에 보낸 hook 을 **보낸 적 없는 것으로** 진행시킨다 — hook
    /// 송신이 실패했을 때(`… dispatch failed; bypassing`)와 같은 갈래다. 예전에는 extension
    /// 기동이 연결 실패로 끝나 hook 송신 자체가 실패했으므로 원래 흐름이 hook 없이 이어졌다.
    /// 그 hook 은 실행된 적이 없으므로 연속 실패로 세지 않는다.
    fn bypass_hooks_sent_to(&mut self, plugin_id: &str) {
        let ids: Vec<u64> = self
            .pending_requests
            .iter()
            .filter(|(_, p)| {
                p.to == plugin_id
                    && matches!(
                        p.kind,
                        PendingRequestKind::ExtensionPreIpcHook { .. }
                            | PendingRequestKind::ExtensionPostIpcHook { .. }
                            | PendingRequestKind::ExtensionPreEventHook { .. }
                            | PendingRequestKind::ExtensionPostEventHook { .. }
                    )
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some(p) = self.pending_requests.remove(&id) else {
                continue;
            };
            let last = !matches!(p.kind, PendingRequestKind::ExtensionPreIpcHook { .. });
            self.record_origin_hop(
                id,
                &p,
                p.sent_at.elapsed(),
                tasty_telemetry::slow_requests::HopOutcome::Cancelled,
                last,
            );
            tracing::warn!("extension '{plugin_id}' did not connect; bypassing its hook");
            match p.kind {
                PendingRequestKind::ExtensionPreIpcHook {
                    target_plugin_id,
                    extension_plugin_id,
                    method,
                    params,
                    final_caller,
                    post_hook,
                    ..
                } => {
                    // 송신 실패 갈래(`ipc_dispatch.rs`)가 target 에 싣는 호출 plugin id 를 그대로
                    // 싣는다 — plugin 이 부른 호출이면 그 id, 로컬 호출이면 없다.
                    let caller_plugin_id = match &final_caller {
                        FinalCaller::Plugin {
                            caller_plugin_id, ..
                        } => Some(caller_plugin_id.clone()),
                        FinalCaller::Local { .. } => None,
                    };
                    self.dispatch_target_invoke(
                        target_plugin_id,
                        method,
                        params,
                        caller_plugin_id.as_deref(),
                        final_caller,
                        post_hook.map(|p| (extension_plugin_id, p)),
                    )
                }
                PendingRequestKind::ExtensionPostIpcHook {
                    target_outcome,
                    final_caller,
                    ..
                } => self.finalize_target_outcome(final_caller, target_outcome),
                PendingRequestKind::ExtensionPreEventHook {
                    publisher_plugin_id,
                    extension_plugin_id,
                    envelope,
                    post_hook,
                    ..
                } => self.fan_out_then_post(
                    &publisher_plugin_id,
                    envelope,
                    extension_plugin_id,
                    post_hook,
                ),
                _ => {}
            }
        }
    }
}
