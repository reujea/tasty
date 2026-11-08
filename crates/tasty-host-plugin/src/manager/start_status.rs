//! 기동 요청의 결과, 마지막 실패와 호스트 알림으로 넘길 실패 보고.
//!
//! 기동마다 세대를 붙인다. 연결 결과는 그 프로세스를 띄운 세대에만 기록하므로, 앞 요청의 늦은
//! 실패가 뒤 요청의 결과를 덮지 않는다. 기동 중에 비활성화·교체되면 그 세대는 취소로 끝난다.
//! 같은 plugin 의 연속 실패는 실패 창(`RESTART_FAILURE_WINDOW`) 안에서 첫 실패 한 건만 보고하고,
//! 자동 비활성화는 따로 한 건 보고한다.

use std::collections::HashMap;
use std::time::Instant;

use super::{PluginManager, RESTART_FAILURE_WINDOW};

/// 한 기동 요청의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartState {
    /// 프로세스를 띄웠거나 띄울 차례이고 연결 결과가 아직 없다.
    Starting,
    /// 핸드셰이크까지 끝났다.
    Connected,
    /// 띄우지 못했거나 연결 전에 끝났다.
    Failed,
    /// 결과가 나기 전에 비활성화·교체로 거뒀다.
    Cancelled,
}

impl StartState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Connected => "connected",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// 실패 종류. `plugin.show` 의 `kind` 값이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// 프로세스를 띄우지 못했다.
    SpawnFailed,
    /// 프로세스가 핸드셰이크 전에 끝났거나 시간 안에 연결하지 않았다.
    ConnectFailed,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpawnFailed => "spawn_failed",
            Self::ConnectFailed => "connect_failed",
        }
    }
}

/// 실패 한 건. `generation` 은 실패한 기동의 세대다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailure {
    pub kind: FailureKind,
    pub message: String,
    pub generation: u64,
}

/// 가장 최근 기동 요청의 세대와 상태. 실패로 끝났으면 그 실패를 싣는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartStatus {
    pub generation: u64,
    pub state: StartState,
    pub failure: Option<PluginFailure>,
}

/// 호스트가 알림으로 남길 실패.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReport {
    /// 기동 실패. 실패 창 안의 이어지는 실패는 보고하지 않는다.
    Failed {
        plugin_id: String,
        failure: PluginFailure,
    },
    /// 실패가 쌓여 자동 비활성화했다. 사용자가 다시 켤 때까지 띄우지 않는다.
    AutoDisabled {
        plugin_id: String,
        failure: PluginFailure,
        failures: usize,
    },
    /// 공식 목록에 없는 `com.tasty.*` 폴더를 등록하지 않았다. `reason`은 판정 문장이다.
    ReservedId { plugin_id: String, reason: String },
}

#[derive(Debug, Default)]
pub(crate) struct StartTracker {
    next_generation: u64,
    starts: HashMap<String, StartStatus>,
    /// 떠 있지만 연결 결과가 없는 프로세스를 띄운 세대.
    connecting: HashMap<String, u64>,
    last_failure: HashMap<String, PluginFailure>,
    reported_at: HashMap<String, Instant>,
    reports: Vec<FailureReport>,
}

impl StartTracker {
    /// 기동을 시작한다. 아직 띄우지 않은 기동(회수 대기)이 있으면 그 세대를 이어 쓴다.
    pub(super) fn begin(&mut self, plugin_id: &str) -> u64 {
        if let Some(s) = self.starts.get(plugin_id)
            && s.state == StartState::Starting
            && !self.connecting.contains_key(plugin_id)
        {
            return s.generation;
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        self.starts.insert(
            plugin_id.to_string(),
            StartStatus {
                generation,
                state: StartState::Starting,
                failure: None,
            },
        );
        generation
    }

    pub(super) fn spawned(&mut self, plugin_id: &str, generation: u64) {
        self.connecting.insert(plugin_id.to_string(), generation);
    }

    pub(super) fn connected(&mut self, plugin_id: &str) {
        let Some(generation) = self.connecting.remove(plugin_id) else {
            return;
        };
        self.reported_at.remove(plugin_id);
        if let Some(s) = self.starts.get_mut(plugin_id)
            && s.generation == generation
            && s.state == StartState::Starting
        {
            s.state = StartState::Connected;
        }
    }

    /// 기동 실패를 기록한다. 띄운 프로세스의 실패면 그 세대에, 띄우기 전 실패면 진행 중 세대에 둔다.
    /// `auto_disabled` 는 이 실패로 자동 비활성화했을 때의 누적 실패 수다.
    pub(super) fn failed(
        &mut self,
        plugin_id: &str,
        kind: FailureKind,
        message: String,
        auto_disabled: Option<usize>,
        now: Instant,
    ) {
        let generation = match self.connecting.remove(plugin_id) {
            Some(g) => g,
            None => self.starts.get(plugin_id).map_or(0, |s| s.generation),
        };
        let failure = PluginFailure {
            kind,
            message,
            generation,
        };
        if let Some(s) = self.starts.get_mut(plugin_id)
            && s.generation == generation
            && s.state == StartState::Starting
        {
            s.state = StartState::Failed;
            s.failure = Some(failure.clone());
        }
        self.last_failure
            .insert(plugin_id.to_string(), failure.clone());
        let plugin_id = plugin_id.to_string();
        if let Some(failures) = auto_disabled {
            self.reported_at.remove(&plugin_id);
            self.reports.push(FailureReport::AutoDisabled {
                plugin_id,
                failure,
                failures,
            });
            return;
        }
        let recent = self
            .reported_at
            .get(&plugin_id)
            .is_some_and(|t| now.duration_since(*t) < RESTART_FAILURE_WINDOW);
        if !recent {
            self.reported_at.insert(plugin_id.clone(), now);
            self.reports
                .push(FailureReport::Failed { plugin_id, failure });
        }
    }

    /// 결과가 나기 전에 거둔 기동을 취소로 끝낸다.
    pub(super) fn cancelled(&mut self, plugin_id: &str) {
        self.connecting.remove(plugin_id);
        if let Some(s) = self.starts.get_mut(plugin_id)
            && s.state == StartState::Starting
        {
            s.state = StartState::Cancelled;
        }
    }

    /// 기동과 무관한 보고를 같은 알림 대기열에 넣는다.
    pub(super) fn report(&mut self, report: FailureReport) {
        self.reports.push(report);
    }

    /// 사용자가 다시 켜면 이전 알림 묶음을 끝낸다.
    pub(super) fn reset_reporting(&mut self, plugin_id: &str) {
        self.reported_at.remove(plugin_id);
    }
}

impl PluginManager {
    /// 가장 최근 기동 요청의 세대와 결과. 기동한 적이 없으면 `None`.
    pub fn start_status(&self, plugin_id: &str) -> Option<&StartStatus> {
        self.start_tracker.starts.get(plugin_id)
    }

    /// 이 plugin 의 마지막 기동 실패. 이후 연결에 성공해도 남는다.
    pub fn last_failure(&self, plugin_id: &str) -> Option<&PluginFailure> {
        self.start_tracker.last_failure.get(plugin_id)
    }

    /// 아직 알리지 않은 실패 보고. 호스트가 알림 저장소를 쓸 수 있을 때만 꺼낸다.
    pub fn take_failure_reports(&mut self) -> Vec<FailureReport> {
        std::mem::take(&mut self.start_tracker.reports)
    }

    /// 알릴 실패 보고가 있는가. 호스트는 알림을 남길 곳이 없으면 꺼내지 않고 둔다.
    pub fn has_failure_reports(&self) -> bool {
        !self.start_tracker.reports.is_empty()
    }

    /// 호스트 시험이 매니저의 실패 보고를 직접 쌓는다.
    #[cfg(any(test, feature = "test-support"))]
    pub fn push_failure_report_for_tests(&mut self, report: FailureReport) {
        self.start_tracker.reports.push(report);
    }
}

#[cfg(test)]
#[path = "start_status_tests.rs"]
mod tests;
