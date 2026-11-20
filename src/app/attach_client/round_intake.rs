//! 출력 회차의 관측 단계. 회차마다 한 번, 설치·이벤트 적용·처리보다 먼저 돈다.
//!
//! 관측은 연결 결과를 거두고, 만료된 재연결 구간을 끝내고, 등록된 연결 시도의 스냅샷을 찍은 뒤 세션
//! 전송의 확정된 끝을 한 번씩 읽는다. 강제 끊김 표지는 여기서만 쓴다. 그래서 같은 회차의 설치 검사와
//! 처리는 같은 관측 결과(표지와 고정 사유)를 본다. 관측 뒤에 확정된 끝은 다음 회차의 몫이다. 관측 뒤
//! 같은 회차에 반납하는 준비된 전송의 끝은 표지 근거로 쓰지 않는다.

use super::pending::PendingMirrorInstall;
use crate::app::App;
use std::collections::HashSet;
use tasty_remote::client_session::{ObservedEnd, SessionState, WorkspaceClose};
use tasty_remote::pending_connection::ConnectionTicket;
use tasty_remote::transport_end::EndReason;

/// 관측 단계가 설치 단계에 넘기는 것.
pub(crate) struct RoundIntake {
    /// 관측 시점에 등록된 연결 시도 전부. 설치 단계는 이 목록만 돈다. 같은 회차의 처리 단계가 새로
    /// 등록한 시도(재동기화 handshake)는 다음 회차의 스냅샷에 든다.
    pub(crate) tickets: Vec<(ConnectionTicket, PendingMirrorInstall)>,
    /// 관측 단계가 설치 전 강제 끊김으로 이미 끝낸 시도. 설치 단계는 건너뛴다.
    pub(crate) ended_before_install: HashSet<ConnectionTicket>,
}

/// 준비된 연결이 설치 전에 강제로 끊긴 시도의 run 결과.
pub(crate) const FORCE_DETACHED_BEFORE_INSTALL: &str =
    "the remote workspace was force-detached before the connection was installed";

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
        // 준비된 연결 가운데 지금도 유효한 시도의 전송이 강제 끊김으로 확정됐다. 유효하지 않은 시도는
        // 차단 근거로 삼지 않고 설치 단계의 폐기 경로에 맡긴다.
        let forced_ready: Vec<(ConnectionTicket, PendingMirrorInstall)> = tickets
            .iter()
            .filter(|(ticket, target)| {
                self.remote
                    .prepared_connection(*ticket)
                    .is_some_and(|prepared| {
                        prepared.transport.end.settled() == Some(EndReason::Forced)
                    })
                    && self.mirror_install_target_is_current(target)
            })
            .cloned()
            .collect();
        forced_rows.extend(
            forced_ready
                .iter()
                .filter(|(_, target)| target.binding.is_some())
                .filter_map(|(_, target)| target.row),
        );
        for row in forced_rows {
            self.mark_force_detached_target(row);
        }
        let mut ended_before_install = HashSet::new();
        for (ticket, target) in forced_ready {
            self.end_force_detached_before_install(ticket, &target);
            ended_before_install.insert(ticket);
        }
        RoundIntake {
            tickets,
            ended_before_install,
        }
    }

    /// 준비된 연결이 설치 전에 강제로 끊겼다. 설치하지 않고 연결을 반납하며, 원 run과 그 행에 합류한
    /// run을 같은 실패로 바로 끝낸다. 일반 실패 경로(백오프, 재연결 실패 처리)는 타지 않는다. 옛 세션의
    /// 멈춤과 해제는 같은 회차의 이벤트 적용 뒤 처리 단계가 한다.
    fn end_force_detached_before_install(
        &mut self,
        ticket: ConnectionTicket,
        target: &PendingMirrorInstall,
    ) {
        self.state
            .mirror_attempts
            .cancel_connection(ticket, &mut self.remote);
        tracing::info!(
            row = ?target.row,
            "gui attach: 준비된 연결이 설치 전에 강제로 끊겨 설치하지 않는다"
        );
        target.fail_run(FORCE_DETACHED_BEFORE_INSTALL);
        if let Some(row) = target.row {
            for run in self.row_waits.take_runs(row) {
                crate::request_runs::finish(
                    run,
                    crate::request_runs::RunState::Failed,
                    FORCE_DETACHED_BEFORE_INSTALL,
                    serde_json::json!({ "workspace_id": row }),
                );
            }
            self.remote.active.remove(&row);
        }
        if target.user {
            let reason = crate::i18n::t("cli.attach.force_detached").to_string();
            self.toast_attach_failure(target, &reason);
        }
        if let Some((workspace, epoch)) = &target.reconnect
            && let Some(session) = self.remote.sessions.iter_mut().find(|session| {
                session.state.local_workspace == *workspace
                    && session.transport.frame_tx.epoch().same(epoch)
            })
        {
            session.state.stop_after_events = true;
        }
    }
}
