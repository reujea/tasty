//! 작업 스레드가 남긴 시스템 알림을 알림 저장소로 옮긴다.

use super::App;
use crate::app::dispatch_domain::DispatchSource;
use crate::runtime::engine_session::EngineId;

impl App {
    /// surface를 지정한 알림은 그 surface의 workspace가 받는다. 나머지와 surface가 사라진 알림은
    /// 포커스 창, 없으면 첫 창의 활성 workspace가 받는다. 창이 없으면 다음 회차까지 그대로 둔다.
    pub(crate) fn poll_system_notices(&mut self) {
        let Some(fallback) = self.system_notice_target() else {
            return;
        };
        for notice in crate::system_notice::take() {
            if notice.source == "settings-file-writer" {
                self.toast_settings_result(
                    notice.body.clone(),
                    crate::adapters::ui::ToastKind::Warning,
                );
            }
            let (engine_id, ws_id, surface_id) = self
                .surface_notice_target(notice.surface_id)
                .map_or((fallback.0, fallback.1, 0), |(e, w)| {
                    (e, w, notice.surface_id)
                });
            let intent = crate::app::command::DomainIntent::PushNotification {
                level: notice.level,
                ws_id,
                surface_id,
                title: notice.title,
                body: notice.body,
                source: notice.source.to_string(),
                merge: crate::app::command::NotificationMerge::Separate,
                terminal_event: false,
            }
            .from_system();
            if let Err(e) = self.dispatch_domain_intent(DispatchSource::Engine(engine_id), intent) {
                tracing::warn!("system notice not stored: {e}");
            }
        }
    }

    fn surface_notice_target(&self, surface_id: u32) -> Option<(EngineId, u32)> {
        if surface_id == 0 {
            return None;
        }
        self.engines.all_sessions().find_map(|session| {
            let engine = session.as_ref();
            let (index, _) = engine.core.find_workspace_index_for_surface(surface_id)?;
            Some((session.id, engine.core.workspace_at(index)?.id))
        })
    }

    fn system_notice_target(&self) -> Option<(EngineId, u32)> {
        let wid = match self.focused_window() {
            Some(_) => self.view.focused_view_id,
            None => self
                .view
                .views
                .iter()
                .find(|(_, w)| w.as_main().is_some())
                .map(|(id, _)| *id),
        }?;
        let (main, engine) = self.engines().window_pair(wid)?;
        let index = main.state.active_workspace_index(engine.core);
        let ws_id = engine.core.workspace_at(index)?.id;
        Some((self.engines.of_window(wid)?, ws_id))
    }
}
