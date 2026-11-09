//! 쉬는 대화 탭의 세션을 내린다(유휴 회수) — 언제 내릴지는 `sync_compact::idle_step` 이 정하고, 여기는
//! 내리는 일만 한다.
//!
//! 내리는 길은 손으로 재우기(`agent_session.sleep`)와 같다 — 자식 프로세스를 멈추고 탭에 표시를 켠다.
//! 좌표는 남으므로 다음 입력이 `--resume` 으로 다시 띄운다. 행은 다시 뜰 때까지 `reclaimed` 를 실어,
//! 활동 폴링이 탭의 `SyncState::reclaimed` 를 켜 둔다 — 회수된 세션에는 현행화를 다시 열지 않는다.

use crate::application::surface_link;
use crate::domain::convo::Convo;
use crate::ports::host::SendHost;

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 탭 하나의 세션을 내린다 — 그새 할 일이 생겼으면(`sync_compact::settled`) 그대로 둔다.
    pub(crate) fn idle_reclaim(&mut self, host: &impl SendHost, sid: u32) {
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        if !crate::application::sync_compact::settled(c, self.activity.is_busy(c)) {
            tracing::info!("[agent_session] surface={sid}: 할 일이 생겨 회수를 미룬다");
            return;
        }
        let Some(session) = c.session_id.clone() else {
            return;
        };
        if let Err(e) = self.sup.reclaim(&session) {
            tracing::warn!("[agent_session] surface={sid}: 유휴 회수 실패: {e}");
            return;
        }
        mark(host, sid, c);
        tracing::info!("[agent_session] surface={sid}: 쉬는 세션을 내렸다(다음 입력에 재개)");
    }
}

/// 회수를 탭에 반영한다 — 표시를 못 켜도 회수는 되돌리지 않는다(`ipc::handle_sleep` 과 같은 판단).
fn mark(host: &impl SendHost, sid: u32, c: &mut Convo) {
    c.sync.reclaimed = true;
    if let Err(e) = surface_link::mark_reclaimed(host, sid) {
        tracing::debug!("[agent_session] surface={sid}: 회수 표시 실패: {e}");
    }
}
