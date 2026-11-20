//! 저장된 원격 행의 연결 상태 전환(끊긴 행, 재연결 시간)과 세션 없는 행의 정리.
//! 세션 연결·재연결 자체는 `connection`에 있다.

use super::connection::{remove_mirror_workspace_from_engine, remove_mirror_workspace_from_parked};
use super::round_observation::RoundObservation;
use crate::app::App;
use crate::runtime::engine_session::EngineId;
use crate::view::ui::View as _;
use std::collections::HashMap;
use tasty_remote::client_session::SessionState;

impl App {
    pub(crate) fn reconnect_window(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.services
                .reconnect_window_secs
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// 행의 진행 중인 재연결 시도(endpoint 해석, 연결, 설치를 기다리는 준비된 연결)를 취소하고
    /// 행 토큰을 바꿔 아직 큐에 넣지 않은 시도와 늦게 도착한 성공도 낡은 것으로 만든다.
    pub(crate) fn abandon_row_attempts(&mut self, row: u32, reason: &str) -> usize {
        let cancelled = super::workspace_close::cancel_mirror_attempts_for(
            &mut self.state.mirror_attempts,
            &mut self.remote,
            row,
            reason,
        );
        for session in self.engines.all_sessions_mut() {
            if session.core_state.has_workspace(row) {
                session
                    .remote
                    .row_tokens
                    .insert(row, std::sync::Arc::new(()));
            }
        }
        self.remote.active.remove(&row);
        cancelled
    }

    /// 서버가 강제로 끊은 저장된 원격 행. 재연결 구간을 열지 않고 끊긴 행으로 두며, 사용자가 끊은
    /// 행처럼 표지를 남겨 그 행을 다시 열어도 붙지 않는다. 표지는 연결 대상으로 저장해 클라이언트를
    /// 다시 시작해도 남는다. 연결 버튼·메뉴·`remote.connect` 같은 명시적 연결만 표지를 지우고 다시 붙인다.
    pub(super) fn stop_after_force_detach(&mut self, idx: usize) {
        let Some(row) = self.remote.sessions[idx].state.saved_row else {
            return;
        };
        self.remote.user_disconnected.insert(row);
        self.mark_force_detached_row(row);
        self.enter_disconnected(row);
        tracing::info!(
            "gui attach: 서버가 saved remote row {row} 의 점유를 강제로 끊어 재연결하지 않는다"
        );
    }

    /// 저장된 원격 행을 형상을 남긴 끊긴 행으로 둔다. 연결이 남아 있으면 먼저 놓는다.
    /// 자동 재연결은 멈추고, 그 행을 다시 열거나 연결 버튼으로 다시 붙는다.
    pub(crate) fn enter_disconnected(&mut self, row: u32) {
        let Some(idx) = self
            .remote
            .sessions
            .iter()
            .position(|session| session.state.saved_row == Some(row))
        else {
            return;
        };
        if self.remote.sessions[idx].state.phase == SessionState::Connected {
            self.release_session_connection(idx, SessionState::Disconnected);
        }
        self.remote.sessions[idx].state.phase = SessionState::Disconnected;
        self.remote.active.remove(&row);
        self.remote.reconnect.entry(row).or_default().given_up = true;
        self.mark_row_window_dirty(row);
        tracing::info!("gui attach: saved remote row {row} 끊긴 상태로 전환");
    }

    /// 같은 연결 대상의 다른 행이 강제로 끊겨 표지가 저장된 뒤 도착한 시도 결과를 버린다. 예약된
    /// 재연결은 `maybe_trigger_reconnect`가 거두지만, 이미 시작한 시도(첫 연결 포함)의 결과는 그 검사를
    /// 지나지 않고 결과 소비(`apply_auto_attach_outcome`)와 설치(`poll_pending_mirror_installs`)로 온다.
    /// 명시적 연결과 원격 팝업으로 시작한 시도는 사용자가 고른 연결이라 그대로 둔다. 버렸으면 true다.
    /// 행은 끊긴 상태로 남는다. 세션이 있으면 `enter_disconnected`로 멈추고, 첫 연결이면 진행 중
    /// 표지만 지운다. 표지가 있는 동안 그 행을 다시 열어도 붙지 않는다.
    pub(crate) fn drop_attempt_for_force_detached_target(
        &mut self,
        target: &super::pending::PendingMirrorInstall,
    ) -> bool {
        let Some(row) = target.row else {
            return false;
        };
        if target.explicit || target.user {
            return false;
        }
        if !self.row_user_disconnected(row) && !self.target_force_detached_this_round(row) {
            return false;
        }
        tracing::info!("gui attach: 연결 대상이 강제로 끊긴 행 {row}의 진행 중 시도 결과를 버린다");
        target.cancel_run("the saved row's target was force-detached");
        self.remote.active.remove(&row);
        self.enter_disconnected(row);
        true
    }

    /// 같은 연결 대상의 다른 저장 행이 강제로 끊겼지만 아직 끝을 분류하지 않아 표지가 저장되지 않았는지.
    /// 끝 분류는 Connected 세션만 다루고, 분류한 세션은 Disconnected로 남긴 채 전송 표지를 지우지 않는다.
    /// 그래서 Connected 세션만 본다. 분류가 끝난 옛 표지를 보면 명시적 연결이 저장 표지를 지운 뒤의
    /// 자동 재연결도 계속 버리게 된다.
    /// `apply_attach_client_output`은 대기 연결 설치를 세션 끝 분류(`stop_after_force_detach`가 표지를
    /// 저장)보다 먼저 한다. 회차 순서를 바꾸면 같은 회차의 설치 여부를 보는 끝 분류(`installing_epoch`)도
    /// 달라지므로, 순서는 두고 설치 검사가 회차 시작의 관측값(`RoundObservation`)을 본다. 끝 분류도 같은
    /// 관측값을 쓰므로 이번 회차에 강제 끊김으로 분류될 행이 곧 여기서 보는 행이다. 회차 밖(endpoint 결과
    /// 소비)에서는 지금 값을 관측한다.
    fn target_force_detached_this_round(&self, row: u32) -> bool {
        let Some(binding) = self.saved_row_binding(row) else {
            return false;
        };
        let same_target = |other: u32| self.saved_row_binding(other).as_ref() == Some(&binding);
        match &self.state.round_observation {
            Some(round) => round.force_detached_rows().any(same_target),
            None => RoundObservation::observe(&self.remote.sessions)
                .force_detached_rows()
                .any(same_target),
        }
    }

    fn mark_row_window_dirty(&mut self, row: u32) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if engine.workspaces().into_iter().any(|ws| ws.id == row) {
                main.mark_dirty();
                break;
            }
        }
    }

    /// 세션 없이 남은 저장된 원격 행(아직 붙지 않았거나 첫 연결에 실패한 행)을 지운다.
    pub(super) fn remove_unattached_row(&mut self, row: u32) {
        let engine = self.saved_remote_row_engine(row);
        let row_binding = self.saved_row_binding(row);
        let none = HashMap::new();
        let mut removed = false;
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            if remove_mirror_workspace_from_engine(&mut engine, &mut main.state, row, &none) {
                main.mark_dirty();
                removed = true;
                break;
            }
        }
        if !removed {
            remove_mirror_workspace_from_parked(self.engines_mut(), row, &none);
        }
        if let Some(engine) = engine {
            self.journal.admit_remote_row(engine, row, true);
        }
        crate::app::auto_attach::forget_row(&mut self.remote, row);
        if let Some(binding) = row_binding {
            self.forget_removed_row_mark(&binding);
        }
    }

    /// 이 workspace를 저장된 원격 행으로 가진 engine.
    pub(crate) fn saved_remote_row_engine(&self, workspace: u32) -> Option<EngineId> {
        self.engines.all_sessions().find_map(|session| {
            let core = &session.core_state;
            core.find_workspace_index_for_id(workspace)
                .and_then(|index| core.workspace_at(index))
                .is_some_and(|ws| ws.is_saved_remote_row())
                .then_some(session.id)
        })
    }
}
