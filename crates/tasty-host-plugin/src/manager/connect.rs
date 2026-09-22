//! 연결 대기의 결과를 거둔다 — 연결 대기 자체는 `process::connect` 가 메인 스레드 밖에서 한다.
//!
//! 기동은 자식을 띄우자마자 `processes` 에 넣는다. 그래서 이 모듈이 할 일은 둘이다 —
//! 연결이 성사되면 연속 실패 기록을 지우고, 끝내 안 오면 예전 spawn 실패와 같은 갈래
//! (`plugin.error` · 연속 실패 누적 · 자동 비활성)로 보낸다. 근거·대안은 ADR-0469.

use std::time::Instant;

use crate::process::connect::{ConnectOutcome, HANDSHAKE_TIMEOUT};

use super::PluginManager;

/// 전체 기동이 연결을 기다려 주는 한도의 여유 — 연결 대기 스레드는 연결 한도
/// ([`HANDSHAKE_TIMEOUT`])에서 스스로 실패를 적으므로, 그 결과를 받을 만큼만 더 기다린다.
const SETTLE_MARGIN: std::time::Duration = std::time::Duration::from_millis(500);

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
                ConnectOutcome::Connected(waited) => {
                    tracing::info!(
                        plugin_id = id,
                        ms = waited.as_secs_f64() * 1000.0,
                        "plugin connected"
                    );
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
        self.forget_plugin_runtime(plugin_id, "plugin did not connect");
        self.on_plugin_spawn_failure(plugin_id, anyhow::anyhow!(reason));
    }

    /// 지금 연결 중인 plugin 전부의 결과가 날 때까지 기다린 뒤 거둔다 — 전체 기동
    /// (`discover_and_start`)이 부른다. 연결 대기는 plugin 마다 스레드라 겹치므로 전체
    /// 대기는 가장 느린 하나로 수렴한다.
    pub(super) fn wait_for_connections(&mut self) {
        let handshake = self
            .listener
            .as_ref()
            .map_or(HANDSHAKE_TIMEOUT, |l| l.handshake_timeout());
        let deadline = Instant::now() + handshake + SETTLE_MARGIN;
        for proc in self.processes.values() {
            proc.wait_connect_settled(deadline);
        }
        self.settle_connections();
    }
}
