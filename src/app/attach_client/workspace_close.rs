//! Server close and local disconnect of a mirror workspace. Only the server's reply closes it.

#[cfg(test)]
mod tests;

use super::attempts::MirrorAttempts;
use crate::app::App;
use crate::ipc::stream::StructuralOp;
use crate::view::ui::View as _;
use tasty_remote::client_session::{AttachClientSession, SessionState, WorkspaceClose};

/// 서버 workspace 닫기를 보내지 않는 이유. 보내지 않은 요청은 성공으로 표시하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseRefusal {
    /// 연결이 없거나 재연결 중이라 서버에 닫기를 전달할 수 없다.
    Reconnecting,
    /// 서버가 workspace 닫기 capability를 알리지 않은 구버전이다.
    Unsupported,
    /// 같은 mirror의 닫기 회신을 이미 기다리고 있다.
    Pending,
}

impl CloseRefusal {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Reconnecting => "mirror connection is not live; the server close was not sent",
            Self::Unsupported => "the remote Tasty does not support closing a workspace",
            Self::Pending => "a server close of this workspace is already waiting for its result",
        }
    }

    fn message_key(self) -> &'static str {
        match self {
            Self::Reconnecting => "attach.toast.workspace_close_reconnecting",
            Self::Unsupported => "attach.toast.workspace_close_unsupported",
            Self::Pending => "attach.toast.workspace_close_pending",
        }
    }
}

/// 이 연결로 서버 닫기를 보낼 수 있는지 판정한다. 이미 닫힌 연결도 다시 보내지 않는다.
pub(super) fn refusal_of(session: &AttachClientSession) -> Option<CloseRefusal> {
    if session.state.phase != SessionState::Connected
        || !session.transport.frame_tx.epoch().is_active()
    {
        return Some(CloseRefusal::Reconnecting);
    }
    if !session.transport.workspace_close {
        return Some(CloseRefusal::Unsupported);
    }
    match session.state.workspace_close {
        WorkspaceClose::Idle => None,
        WorkspaceClose::Pending(_) | WorkspaceClose::Closed => Some(CloseRefusal::Pending),
    }
}

/// 준비된 닫기 frame을 실제로 보내기 직전의 판정. 앞서 보낸 닫기가 회신을 기다리거나 이미 성공했으면
/// 보내지 않는다. 보내면 기다리던 op id를 덮어써 그 성공 회신을 잃는다.
pub(crate) fn send_refusal(session: &AttachClientSession) -> Option<CloseRefusal> {
    match session.state.workspace_close {
        WorkspaceClose::Idle => None,
        WorkspaceClose::Pending(_) | WorkspaceClose::Closed => Some(CloseRefusal::Pending),
    }
}

/// 닫기 frame을 보낸 연결이 그 op의 회신을 기다리게 한다.
pub(crate) fn note_sent(session: &mut AttachClientSession, op_id: u64) {
    session.state.workspace_close = WorkspaceClose::Pending(op_id);
}

/// 회신을 받지 못한 채 연결이 끝난 닫기. 서버가 닫았는지 알 수 없다.
pub(super) fn close_result_unknown(session: &AttachClientSession) -> bool {
    matches!(session.state.workspace_close, WorkspaceClose::Pending(_))
}

/// 이 mirror의 재연결을 위해 진행 중인 endpoint·connection 시도를 취소한다. 다른 mirror의 시도는 유지한다.
pub(super) fn cancel_mirror_attempts(
    attempts: &mut MirrorAttempts,
    remote: &mut tasty_remote::outbound::Remote,
    local_workspace: u32,
) -> usize {
    let reason = "the mirror workspace was disconnected by the user";
    let mut cancelled = 0;
    for (token, target) in attempts.endpoint_snapshot() {
        if target.reconnect.as_ref().map(|(id, _)| *id) == Some(local_workspace) {
            target.cancel_run(reason);
            attempts.cancel_endpoint(&token, remote);
            cancelled += 1;
        }
    }
    for (ticket, target) in attempts.connection_snapshot() {
        if target.reconnect.as_ref().map(|(id, _)| *id) == Some(local_workspace) {
            target.cancel_run(reason);
            attempts.cancel_connection(ticket, remote);
            cancelled += 1;
        }
    }
    cancelled
}

impl App {
    /// 서버 workspace 닫기를 보내기 전에 연결 상태를 확인한다. 다른 op에는 None이다.
    pub(crate) fn workspace_close_refusal(&self, op: &StructuralOp) -> Option<CloseRefusal> {
        let StructuralOp::CloseWorkspace { anchor_surface_id } = op else {
            return None;
        };
        match self.remote.sessions.iter().find(|session| {
            session
                .state
                .remote_to_local
                .values()
                .any(|local| local == anchor_surface_id)
        }) {
            Some(session) => refusal_of(session),
            None => Some(CloseRefusal::Reconnecting),
        }
    }

    /// 보내지 않은 서버 닫기를 요청한 창에 알린다. 에이전트 요청은 로그만 남긴다.
    pub(crate) fn report_workspace_close_refusal(
        &mut self,
        engine: crate::runtime::engine_session::EngineId,
        origin: &crate::intent::IntentOrigin,
        refusal: CloseRefusal,
    ) {
        tracing::warn!("remote workspace close refused: {}", refusal.reason());
        if !origin.is_user() {
            return;
        }
        if let Some(context) = self.engines_mut().resolve(engine)
            && let Some(view) = context.view
        {
            context.state.toasts.push(
                crate::i18n::t(refusal.message_key()).to_string(),
                crate::adapters::ui::ToastKind::Warning,
                crate::adapters::ui::ToastScope::Window,
            );
            view.mark_dirty();
        }
    }

    /// 사용자가 mirror 연결을 끊는다. 서버 workspace와 프로세스는 유지한다.
    /// 저장된 원격 행은 형상을 남긴 끊긴 행이 되고 명시적으로 연결할 때까지 자동으로 붙지 않는다.
    /// 시도 중인 행도 시도를 취소하고 같은 상태가 된다. 에이전트 mirror는 로컬 mirror와 세션을 정리한다.
    pub(crate) fn disconnect_mirror_workspace(&mut self, workspace_id: u32) -> bool {
        if self.saved_row_binding(workspace_id).is_some() {
            return self.disconnect_saved_row(workspace_id);
        }
        let Some(index) = self
            .remote
            .sessions
            .iter()
            .position(|session| session.state.local_workspace == workspace_id)
        else {
            tracing::info!("mirror disconnect: workspace {workspace_id} has no attach session");
            return false;
        };
        let cancelled = cancel_mirror_attempts(
            &mut self.state.mirror_attempts,
            &mut self.remote,
            workspace_id,
        );
        if close_result_unknown(&self.remote.sessions[index]) {
            self.toast_mirror_window(
                workspace_id,
                "attach.toast.workspace_close_unknown",
                crate::adapters::ui::ToastKind::Warning,
            );
        }
        let session = self.remote.sessions.remove(index);
        tracing::info!(
            "mirror disconnect: local ws {workspace_id} (remote ws {}, phase {:?}, cancelled attempts {cancelled})",
            session.state.remote_workspace,
            session.state.phase
        );
        self.end_mirror_locally(session);
        true
    }

    /// 저장된 원격 행의 연결을 사용자가 끊는다. 아직 붙지 않은 행이면 바뀔 것이 없지만 시도는 취소한다.
    fn disconnect_saved_row(&mut self, row: u32) -> bool {
        let cancelled =
            cancel_mirror_attempts(&mut self.state.mirror_attempts, &mut self.remote, row);
        // 행 토큰을 바꿔 아직 큐에 넣지 않은 시도도 낡은 것으로 만든다.
        for session in self.engines.all_sessions_mut() {
            if session.core_state.has_workspace(row) {
                session
                    .remote
                    .row_tokens
                    .insert(row, std::sync::Arc::new(()));
            }
        }
        self.remote.active.remove(&row);
        self.remote.user_disconnected.insert(row);
        if let Some(index) = self
            .remote
            .sessions
            .iter()
            .position(|session| session.state.local_workspace == row)
        {
            if close_result_unknown(&self.remote.sessions[index]) {
                self.toast_mirror_window(
                    row,
                    "attach.toast.workspace_close_unknown",
                    crate::adapters::ui::ToastKind::Warning,
                );
            }
            if let Err(error) = self.remote.sessions[index]
                .send_frame(crate::ipc::stream::StreamTag::Detach, Vec::new())
            {
                tracing::debug!(%error, "detach notice could not reach the writer");
            }
            self.enter_disconnected(row);
        }
        tracing::info!(
            "mirror disconnect: saved remote row {row} (cancelled attempts {cancelled})"
        );
        true
    }

    /// 서버 닫기 성공이나 사용자 연결 끊기로 끝난 mirror를 정리한다. 원격발 끊김과 달리 재연결하지 않는다.
    /// 정리로 바뀐 선택을 전환으로 읽지 않도록 직전 활성 workspace를 맞춘다.
    pub(super) fn end_mirror_locally(&mut self, session: AttachClientSession) {
        cancel_mirror_attempts(
            &mut self.state.mirror_attempts,
            &mut self.remote,
            session.state.local_workspace,
        );
        self.cleanup_mirror_workspace(&session, false);
        self.remote.last_active_ws = self
            .focused_pair()
            .and_then(|(main, engine)| {
                engine
                    .core
                    .workspace_at(main.state.active_workspace_index(engine.core))
            })
            .map(|workspace| workspace.id);
        // transport drop이 socket·worker·소유 tunnel을 Remote retirement로 넘긴다.
        drop(session);
    }

    /// 회신 전에 끊긴 닫기는 성공으로 단정하지 않고 결과를 모른다고 알린다.
    pub(super) fn notify_unknown_closes(&mut self, workspaces: Vec<u32>) {
        for workspace in workspaces {
            self.toast_mirror_window(
                workspace,
                "attach.toast.workspace_close_unknown",
                crate::adapters::ui::ToastKind::Warning,
            );
        }
    }

    /// 원격발로 끊긴 세션(`dead`)과 서버가 닫은 세션(`closed`)을 정리한다. 두 목록은 겹치지 않으며
    /// 인덱스는 뒤에서부터 지운다.
    pub(super) fn end_sessions(&mut self, dead: &[usize], closed: &[usize]) {
        let mut ended: Vec<(usize, bool)> = dead
            .iter()
            .map(|&idx| (idx, false))
            .chain(closed.iter().map(|&idx| (idx, true)))
            .collect();
        ended.sort_unstable();
        for &(idx, server_closed) in ended.iter().rev() {
            let session = self.remote.sessions.remove(idx);
            if server_closed {
                self.end_server_closed_mirror(session);
            } else {
                self.cleanup_mirror_workspace(&session, true);
            }
        }
    }

    fn end_server_closed_mirror(&mut self, session: AttachClientSession) {
        let workspace = session.state.local_workspace;
        self.toast_mirror_window(
            workspace,
            "attach.toast.workspace_closed_on_server",
            crate::adapters::ui::ToastKind::Success,
        );
        tracing::info!(
            "attach mirror: local ws {workspace} (remote ws {}) closed on the server",
            session.state.remote_workspace
        );
        self.end_mirror_locally(session);
    }

    /// mirror가 있는 창에 안내한다. 창이 없는 parked mirror는 로그만 남긴다.
    pub(super) fn toast_mirror_window(
        &mut self,
        local_workspace: u32,
        key: &'static str,
        kind: crate::adapters::ui::ToastKind,
    ) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if engine
                .workspaces()
                .into_iter()
                .any(|workspace| workspace.id == local_workspace)
            {
                main.state.toasts.push(
                    crate::i18n::t(key).to_string(),
                    kind,
                    crate::adapters::ui::ToastScope::Window,
                );
                main.mark_dirty();
                return;
            }
        }
        tracing::info!("mirror {local_workspace}: no window for notice {key}");
    }
}
