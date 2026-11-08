//! 플러그인 매니저의 기동·연결 실패, 자동 비활성화, 예약 id 거부를 알림으로 남긴다. GUI와 헤드리스가 같이 쓴다.
//!
//! 매니저는 실패 보고를 쌓아 두고 호스트가 꺼낸다. 알림을 남길 workspace가 없으면 꺼내지 않으므로
//! 첫 창·워크스페이스가 준비되기 전 부팅 중 실패도 버려지지 않는다. 사용자가 끈 plugin은 기동하지
//! 않으므로 보고가 생기지 않는다. 묶음 기준은 매니저(`start_status`)가 정한다.

use tasty_host_plugin::manager::{FailureKind, FailureReport};

use crate::notification::NotificationLevel;
use crate::plugin::PluginManager;
use crate::runtime::engine_access::EngineMut;

/// 쌓인 실패 보고를 이 engine의 알림으로 옮긴다. 옮긴 건수를 돌려준다.
pub(crate) fn report_plugin_failures(mgr: &mut PluginManager, engine: &mut EngineMut<'_>) -> usize {
    if !mgr.has_failure_reports() {
        return 0;
    }
    let ws_id = engine.workspace_at(0).map(|ws| ws.id);
    deliver(mgr, engine, ws_id)
}

fn deliver(mgr: &mut PluginManager, engine: &mut EngineMut<'_>, ws_id: Option<u32>) -> usize {
    let Some(ws_id) = ws_id else {
        // 알림 저장소의 출처가 없다. 보고는 매니저에 남겨 다음 pump 에서 옮긴다.
        return 0;
    };
    let reports = mgr.take_failure_reports();
    let count = reports.len();
    for report in reports {
        let (level, title, body) = notice(&report);
        // 서로 다른 plugin 의 실패나 같은 plugin 의 실패와 자동 비활성화는 각각 다른 사건이다.
        // 병합 창 안에 이어 나와도 합치지 않아 제목과 본문을 잃지 않는다.
        let id =
            engine
                .live
                .notifications
                .add_separate(ws_id, 0, title.clone(), body.clone(), level);
        engine.enqueue_host_event(crate::state::PendingHostEvent::NotificationCreated {
            id,
            level,
            title,
            body,
            source: "plugin".to_string(),
        });
    }
    count
}

/// GUI pump 경로. 포커스된 창의 engine, 없으면 첫 engine에 남긴다.
#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn report_plugin_failures(&mut self) {
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        if !mgr.has_failure_reports() {
            return;
        }
        let focused = self
            .view
            .focused_view_id
            .filter(|wid| self.engines.of_window(*wid).is_some());
        match focused {
            Some(wid) => {
                if let Some(mut engine) = self.engines.window_engine_mut(wid) {
                    report_plugin_failures(mgr, &mut engine);
                }
            }
            None => {
                if let Some(session) = self.engines.all_sessions_mut().next() {
                    report_plugin_failures(mgr, &mut session.borrow_mut());
                }
            }
        }
    }
}

/// 한 번의 실패는 경고, 실패가 쌓여 자동으로 끈 것은 오류다. plugin 은 ID로 가리킨다
/// (매니페스트 문구를 싣지 않는다).
fn notice(report: &FailureReport) -> (NotificationLevel, String, String) {
    match report {
        FailureReport::Failed { plugin_id, failure } => {
            let body_key = match failure.kind {
                FailureKind::SpawnFailed => "plugin_notice.spawn_failed_body",
                FailureKind::ConnectFailed => "plugin_notice.connect_failed_body",
            };
            (
                NotificationLevel::Warning,
                crate::i18n::t_fmt("plugin_notice.start_failed_title", plugin_id),
                tasty_i18n::t_args(body_key, &[&failure.message, plugin_id]),
            )
        }
        // 디자인을 정하기 전이라 기동 실패 제목을 쓰고 본문에 판정 문장을 그대로 싣는다.
        FailureReport::ReservedId { plugin_id, reason } => (
            NotificationLevel::Warning,
            crate::i18n::t_fmt("plugin_notice.start_failed_title", plugin_id),
            reason.clone(),
        ),
        FailureReport::AutoDisabled {
            plugin_id,
            failure,
            failures,
        } => (
            NotificationLevel::Error,
            crate::i18n::t_fmt("plugin_notice.auto_disabled_title", plugin_id),
            tasty_i18n::t_args(
                "plugin_notice.auto_disabled_body",
                &[&failures.to_string(), &failure.message, plugin_id],
            ),
        ),
    }
}

#[cfg(test)]
#[path = "plugin_failure_notice_tests.rs"]
mod tests;
