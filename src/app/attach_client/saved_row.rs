//! 저장된 원격 행의 연결 상태 전환(끊긴 행, 재연결 시간)과 세션 없는 행의 정리.
//! 세션 연결·재연결 자체는 `connection`에 있다.

use super::connection::{remove_mirror_workspace_from_engine, remove_mirror_workspace_from_parked};
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
