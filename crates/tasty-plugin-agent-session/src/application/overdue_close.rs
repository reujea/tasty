//! overdue 턴 닫기의 탭 쪽 — 활동 폴링 · 중지 단추가 판정(`overdue`)을 대화 탭에 반영한다.

use crate::application::session_surface;
use crate::domain::overdue::{close, note_poll};
use crate::ports::host::SendHost;

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 활동 폴링마다 — 데몬은 끝났다는데 탭이 기다리는 overdue 턴을 닫는다.
    pub(crate) fn close_stale_overdue(&mut self, host: &impl SendHost) {
        let mut closed = Vec::new();
        for (&sid, c) in self.convos.iter_mut() {
            let active = self.activity.daemon_active(c);
            if note_poll(c, active) {
                closed.push((sid, close(c)));
            }
        }
        let note = crate::domain::i18n::t("sup.chat.overdue_recovered");
        for (sid, ats) in closed {
            tracing::info!(
                "[agent_session] surface={sid}: 데몬은 턴이 끝났는데 탭이 기다리던 턴 {ats:?} 을 닫고 기록에서 답을 읽는다"
            );
            for at in ats {
                session_surface::patch(
                    host,
                    sid,
                    &session_surface::finish_msg(at, &Ok(note.clone())),
                );
            }
            if let Some(c) = self.convos.get(&sid) {
                self.activity.refresh_status(host, sid, c);
            }
        }
    }
}
