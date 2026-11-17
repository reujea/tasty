//! 연결 대기 결과를 처리한다. 대기는 process::connect의 별도 스레드가 수행한다.
//! 성공 시 실패 기록을 지우고, 실패 시 프로세스를 회수하고 대기 요청에 알린다.
//! 연결 전에 보낸 요청의 제한 시간은 연결 완료부터 계산한다.

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use crate::process::connect::{ConnectOutcome, HANDSHAKE_TIMEOUT};

use super::{FinalCaller, PendingRequestKind, PluginManager};

/// 연결 대기 한도 뒤 결과를 회수할 여유 시간.
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
                    self.end_respawn_streak(&id);
                    self.start_tracker.connected(&id);
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
            let generation = proc.reply_binding();
            self.retire_pending(plugin_id, proc.abandon(), false, generation);
        }
        self.bypass_hooks_sent_to(plugin_id);
        self.forget_plugin_runtime(plugin_id, "plugin did not connect");
        self.on_plugin_spawn_failure(
            plugin_id,
            super::FailureKind::ConnectFailed,
            anyhow::anyhow!(reason),
        );
    }

    /// 같은 deadline으로 모든 진행 중 연결의 결과를 기다린 뒤 처리한다.
    pub(super) fn wait_for_connections(&mut self) {
        let deadline = Instant::now() + self.connection_wait_limit();
        for proc in self.processes.values() {
            proc.wait_connect_settled(deadline);
        }
        self.settle_connections();
    }

    /// 연결 한도에 결과를 회수할 여유 시간을 더한 대기 기준.
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

    /// 연결 전에 보낸 요청의 deadline을 연결 대기 시간만큼 미룬다.
    /// 연결 중이면 None이며 연결 실패는 on_connect_failure에서 처리한다.
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

    /// 연결하지 못한 extension의 hook은 실행되지 않았으므로 우회하고 실패 횟수에 넣지 않는다.
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
