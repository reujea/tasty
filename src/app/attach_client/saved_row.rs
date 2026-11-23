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

    /// 서버가 강제로 끊은 저장된 원격 행의 연결 대상에 표지를 남긴다. 관측 단계만 부른다. 사용자가
    /// 끊은 행처럼 그 행을 다시 열어도 붙지 않고, 표지는 연결 대상으로 저장해 클라이언트를 다시 시작해도
    /// 남는다. 연결 버튼·메뉴·`remote.connect` 같은 명시적 연결만 표지를 지우고 다시 붙인다.
    pub(super) fn mark_force_detached_target(&mut self, row: u32) {
        self.remote.user_disconnected.insert(row);
        self.mark_force_detached_row(row);
        tracing::info!(
            "gui attach: 서버가 saved remote row {row} 의 점유를 강제로 끊어 재연결하지 않는다"
        );
    }

    /// 처리 단계가 멈추기로 정한 저장 행을 끊긴 행으로 둔다. 재연결·재동기화를 하지 않는다. 옛 전송의
    /// 재동기화 상태도 함께 끝내, 이후 명시적 재연결이 일반 재연결로 등록되게 한다.
    ///
    /// `connect_after`는 멈춤을 미룬 동안 명시적 연결이 왔다는 뜻이다. 그 연결이 시작한 사용자 시도가
    /// 아직 유효하면 끊긴 행으로 바꾸지 않고 재동기화 상태만 끝낸다. 행의 진행 표지와 재연결 슬롯이
    /// 남아 기존 대기·만료·결과 경로가 그 시도를 다룬다. 그런 시도가 없으면 끊긴 행으로 두고 행을
    /// 돌려준다. 호출자가 `connect_after_stop`으로 그 연결을 시작한다.
    pub(super) fn stop_saved_row(&mut self, idx: usize, connect_after: bool) -> Option<u32> {
        let state = &mut self.remote.sessions[idx].state;
        state.resync_pending = None;
        state.resync_awaiting_window = false;
        let row = state.saved_row?;
        if connect_after && self.row_has_live_user_attempt(row) {
            tracing::info!("gui attach: saved remote row {row} 의 멈춤 뒤 명시적 시도를 이어 둔다");
            return None;
        }
        self.enter_disconnected(row);
        connect_after.then_some(row)
    }

    /// 멈춤 뒤에 미룬 명시적 연결을 시작한다. 그 행에 남은 시도(자동 시도 포함)를 먼저 거둬 옛 시도의
    /// 결과가 새 시도의 진행 표지와 슬롯을 바꾸지 않게 한다. 표지는 다시 지우지 않는다. 의사를 남긴
    /// 명시적 연결이 이미 지웠고, 그 뒤에 선 표지는 명시적 시도를 막지 않는다.
    pub(super) fn connect_after_stop(&mut self, row: u32) {
        if self.saved_row_binding(row).is_none() {
            return;
        }
        self.abandon_row_attempts(row, "an explicit connect after the stop replaces it");
        self.remote.pending_reactivation.remove(&row);
        tracing::info!("gui attach: saved remote row {row} 의 멈춤 뒤 미룬 명시적 연결을 시작한다");
        self.restart_disconnected_row(row, None, true);
    }

    /// 행에 아직 유효한 사용자 시도(명시적 연결·원격 팝업)가 있는지.
    fn row_has_live_user_attempt(&self, row: u32) -> bool {
        self.state.mirror_attempts.row_attempts(row).any(|target| {
            (target.explicit || target.user) && self.mirror_install_target_is_current(target)
        })
    }

    /// 명시적 연결이 그 행 세션의 지금 전송에 정한 멈춤에 연결 의사를 남긴다. 다른 전송의 멈춤에는
    /// 남기지 않는다.
    pub(crate) fn note_connect_after_stop(&mut self, row: u32) {
        if let Some(session) = self
            .remote
            .sessions
            .iter_mut()
            .find(|session| session.state.saved_row == Some(row))
        {
            let epoch = session.transport.frame_tx.epoch();
            if let Some(stop) = session
                .state
                .stop_after_events
                .as_mut()
                .filter(|stop| stop.is_for(&epoch))
            {
                stop.connect_after = true;
            }
        }
    }

    /// 멈춤에 남은 연결 의사를 버린다. 멈춤은 그대로 둔다. 멈춤을 적용하기 전에 행이 끊긴 행이 되면
    /// 그 명시적 연결은 결과(만료·거절·다시 막힘·사용자 끊기)를 이미 받은 것이다.
    pub(crate) fn drop_connect_after_stop(&mut self, row: u32) {
        if let Some(stop) = self
            .remote
            .sessions
            .iter_mut()
            .find(|session| session.state.saved_row == Some(row))
            .and_then(|session| session.state.stop_after_events.as_mut())
        {
            stop.connect_after = false;
        }
    }

    /// 처리 단계가 재연결로 보낸 저장 행의 세션. 낡음으로 버린 재동기화의 끝이면 그 행의 시도와 재연결 구간을
    /// 지키는 `end_discarded_resync`로, 아니면 `enter_reconnecting`으로 간다.
    pub(super) fn reconnect_ended_session(&mut self, idx: usize) {
        if super::pending::resync_discard_recorded(&self.remote.sessions[idx]) {
            self.end_discarded_resync(idx);
        } else {
            self.enter_reconnecting(idx);
        }
    }

    /// 설치 단계가 재동기화 연결 시도를 낡음으로 버린 저장 행의 세션을 끝낸다. 재동기화 실패의 자체 완료
    /// (`fail_pending_mirror`)처럼 재연결 중이 되되 그 행의 지금 시도와 재연결 구간을 지킨다. 그 행에 살아 있는
    /// 유효한 다른 시도가 있으면 세션만 재연결 중으로 두고 그 시도가 재연결 구간 안에서 스케줄러에 대체되지
    /// 않게 진행 표지와 슬롯을 갖춘다. 다른 시도가 끝나며 정한 슬롯(백오프)이 있으면 그대로 이어 간다.
    fn end_discarded_resync(&mut self, idx: usize) {
        let state = &mut self.remote.sessions[idx].state;
        state.resync_pending = None;
        state.resync_awaiting_window = false;
        if let Some(end) = state.observed_end.as_mut() {
            end.resync_discarded = false;
        }
        let Some(row) = state.saved_row else {
            return;
        };
        let live = self.row_has_live_attempt(row);
        if !live && !self.remote.reconnect.contains_key(&row) {
            self.enter_reconnecting(idx);
            return;
        }
        let (_, local_workspace) = self.release_session_connection(
            idx,
            tasty_remote::client_session::SessionState::Reconnecting,
        );
        if live {
            self.remote.reconnect.entry(row).or_default();
            self.remote.active.insert(row);
        } else {
            self.remote.active.remove(&row);
            if self.reconnect_window().is_zero() {
                self.enter_disconnected(row);
                return;
            }
        }
        self.mark_row_window_dirty(row);
        tracing::info!(
            "gui attach: mirror workspace {local_workspace} 재동기화 시도 폐기 뒤 재연결 대기(Reconnecting) 진입 (row {row}, live attempt {live})"
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
        self.drop_connect_after_stop(row);
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
    /// 설치 단계의 표지는 같은 회차의 관측 단계가 이미 썼다.
    ///
    /// 행은 끊긴 상태로 남는다. 첫 연결이면 진행 중 표지만 지운다. 세션이 있으면 회차 밖(결과 소비)에서는
    /// 바로 멈추고, 설치 단계에서는 옛 전송의 해제와 멈춤을 같은 회차의 이벤트 적용 뒤로 넘긴다.
    pub(crate) fn drop_attempt_for_force_detached_target(
        &mut self,
        target: &super::pending::PendingMirrorInstall,
        site: DropSite,
    ) -> bool {
        if !self.auto_attempt_blocked(target) {
            return false;
        }
        let Some(row) = target.row else {
            return false;
        };
        tracing::info!("gui attach: 연결 대상이 강제로 끊긴 행 {row}의 진행 중 시도 결과를 버린다");
        target.cancel_run("the saved row's target was force-detached");
        match site {
            DropSite::OutsideRound => {
                self.remote.active.remove(&row);
                self.enter_disconnected(row);
            }
            DropSite::InstallStage => {
                let session = target.reconnect.as_ref().and_then(|(workspace, epoch)| {
                    self.remote.sessions.iter_mut().find(|session| {
                        session.state.local_workspace == *workspace
                            && session.transport.frame_tx.epoch().same(epoch)
                    })
                });
                match session {
                    Some(session) => {
                        session.state.stop_after_events =
                            Some(tasty_remote::client_session::DeferredStop::new(
                                session.transport.frame_tx.epoch(),
                            ))
                    }
                    None => {
                        self.remote.active.remove(&row);
                    }
                }
            }
        }
        true
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

/// 시도를 버리는 판정을 부른 자리.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropSite {
    /// 출력 회차의 설치 단계. 세션의 멈춤은 처리 단계가 한다.
    InstallStage,
    /// 회차 밖(endpoint 결과 소비). 살아 있는 옛 전송이 없어 바로 멈춘다.
    OutsideRound,
}
