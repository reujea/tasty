//! JournalApplication이 모은 복원·저장 사실을 알림 항목으로 옮긴다.
use crate::app::App;
use crate::app::journal::notices::{self, RecoveryNotice};
use crate::notification::NotificationLevel;

/// 알림 항목의 출처 표시. 사용자가 보는 출처 이름이 아니라 구분용 값이다.
const SOURCE: &str = "tasty.recovery";

impl App {
    /// 쌓인 알림을 받을 엔진의 첫 workspace에 넣는다. 아직 workspace가 없거나 창에 붙기 전인 엔진의
    /// 알림은 다음 차례로 미룬다.
    pub(crate) fn deliver_recovery_notices(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_recovery_notices);
        pending.extend(self.journal.take_notices());
        if pending.is_empty() {
            return;
        }
        let fallback = self
            .view
            .views
            .keys()
            .find_map(|window| self.engines.of_window(*window));
        for notice in notices::merge_same_level(pending) {
            let workspace = notice
                .engine
                .or(fallback)
                .and_then(|id| self.engines.get(id))
                .and_then(|engine| engine.core.local_workspaces().first().map(|ws| ws.id));
            // 알림 전달은 창이나 parked 엔진의 workspace만 찾는다. 새 창에 붙기 전인 엔진의 알림은
            // 그 창이 생길 때까지 미룬다.
            let Some(workspace) = workspace.filter(|id| {
                crate::app::window_access::engines_mut!(self)
                    .sessions()
                    .any(|(_, engine)| engine.has_workspace(*id))
            }) else {
                self.pending_recovery_notices.push(notice);
                continue;
            };
            log(&notice);
            if let Some(popup) = &notice.popup {
                self.show_restore_notice(workspace, popup);
            }
            self.cascade_notification_pushed(crate::app::command::NotificationPush {
                ws_id: workspace,
                surface_id: 0,
                title: notice.title,
                body: notice.body,
                source: SOURCE.into(),
                level: notice.level,
                merge: crate::app::command::NotificationMerge::Separate,
                terminal_event: false,
            });
        }
    }

    /// workspace가 있는 창의 오른쪽 위 안내 열에 비포커스 안내를 띄운다. parked 엔진이면 그 엔진이
    /// 창에 붙을 때까지 띄우지 않는다. 오류 알림 항목은 따로 남으므로 안내를 잃지 않는다.
    fn show_restore_notice(&mut self, workspace: u32, popup: &notices::NoticePopup) {
        self.next_restore_notice_id = self.next_restore_notice_id.wrapping_add(1);
        let entry = crate::adapters::ui::restore_notice::RestoreNoticeEntry {
            id: self.next_restore_notice_id,
            title: popup.title().to_owned(),
            cause: popup.cause.text().to_owned(),
            label: popup.label().to_owned(),
            path: popup.path.clone(),
            body: self.notice_body(popup),
        };
        if let Some((state, _)) = crate::app::window_access::engines_mut!(self)
            .sessions()
            .find(|(_, engine)| engine.has_workspace(workspace))
        {
            crate::adapters::ui::restore_notice::push(state, entry);
        }
    }

    /// 안내 본문. halt 중에는 알림 영역을 열 수 없으므로 halt 사유를 안내에 함께 보인다.
    fn notice_body(&self, popup: &notices::NoticePopup) -> String {
        match (popup.variant, self.journal.halt_reason()) {
            (notices::NoticeVariant::Halt, Some(reason)) => format!(
                "{}

{reason}",
                popup.body()
            ),
            _ => popup.body().to_owned(),
        }
    }

    /// 사용자가 닫은 안내를 거둔다. 안내를 닫아도 원본·저장 상태는 바뀌지 않는다.
    pub(crate) fn collect_restore_notice_acknowledgements(&mut self) {
        for (state, _) in crate::app::window_access::engines_mut!(self).sessions() {
            for id in state.restore_notices.take_acknowledged() {
                tracing::info!(id, "restore notice dismissed");
            }
        }
    }
}

fn log(notice: &RecoveryNotice) {
    match notice.level {
        NotificationLevel::Error => tracing::error!("{}: {}", notice.title, notice.body),
        _ => tracing::warn!("{}: {}", notice.title, notice.body),
    }
}
