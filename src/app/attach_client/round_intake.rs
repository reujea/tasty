//! 출력 회차의 관측 단계. 회차마다 한 번, 설치·이벤트 적용·처리보다 먼저 돈다.
//!
//! 관측은 연결 결과를 거두고, 만료된 재연결 구간을 끝내고, 등록된 연결 시도의 스냅샷을 찍은 뒤 세션
//! 전송의 확정된 끝을 한 번씩 읽는다. 강제 끊김 표지는 여기서만 쓴다. 그래서 같은 회차의 설치 검사와
//! 처리는 같은 관측 결과(표지와 고정 사유)를 본다. 관측 뒤에 확정된 끝은 다음 회차의 몫이다. 관측 뒤
//! 같은 회차에 반납하는 준비된 전송의 끝은 표지 근거로 쓰지 않는다.

use super::pending::PendingMirrorInstall;
use crate::app::App;
use tasty_remote::client_session::{ObservedEnd, SessionState, WorkspaceClose};
use tasty_remote::pending_connection::ConnectionTicket;
use tasty_remote::transport_end::EndReason;

/// 관측 단계가 설치 단계에 넘기는 것.
pub(crate) struct RoundIntake {
    /// 관측 시점에 등록된 연결 시도 전부. 설치 단계는 이 목록만 돈다. 같은 회차의 처리 단계가 새로
    /// 등록한 시도(재동기화 handshake)는 다음 회차의 스냅샷에 든다.
    pub(crate) tickets: Vec<(ConnectionTicket, PendingMirrorInstall)>,
}

impl App {
    /// 회차의 관측 단계. 결과는 설치 단계(`poll_pending_mirror_installs`)에 넘긴다.
    pub(crate) fn observe_round(&mut self) -> RoundIntake {
        self.remote.collect_connections();
        self.end_expired_reconnect_windows();
        let tickets = self.state.mirror_attempts.connection_snapshot();
        let mut forced_rows = Vec::new();
        for session in &mut self.remote.sessions {
            let Some(reason) = session.transport.end.settled() else {
                continue;
            };
            let epoch = session.transport.frame_tx.epoch();
            if session
                .state
                .observed_end
                .as_ref()
                .is_some_and(|end| end.epoch.same(&epoch))
            {
                continue;
            }
            session.state.observed_end = Some(ObservedEnd {
                epoch,
                reason,
                routed: false,
            });
            if reason == EndReason::Forced
                && session.state.phase == SessionState::Connected
                && session.state.workspace_close != WorkspaceClose::Closed
                && let Some(row) = session.state.saved_row
            {
                forced_rows.push(row);
            }
        }
        for row in forced_rows {
            self.mark_force_detached_target(row);
        }
        RoundIntake { tickets }
    }
}
