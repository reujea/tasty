//! 도구 메뉴의 popup 열기 요청과 생성 대기 초안에 걸어 둔 명령을 플러그인 매니저로 전달한다.

use crate::app::App;

impl App {
    pub(crate) fn dispatch_pending_popup_opens(&mut self) {
        let mut drained: Vec<crate::state::PendingPopupOpen> = Vec::new();
        let mut notices = Vec::new();
        for (s, engine) in self.engines_mut().sessions() {
            // 생성이 끝난 초안에 걸어 둔 명령을 먼저 실행해 그 팝업 요청도 이번에 연다.
            notices.extend(crate::adapters::ui::tools_menu::run_deferred_tools(
                s,
                &engine.read(),
            ));
            drained.append(&mut s.pending_popup_opens);
        }
        if drained.is_empty() && notices.is_empty() {
            return;
        }
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        for notice in notices {
            crate::plugin_bridge::key_dispatch::emit_command_invoked(
                mgr,
                &notice.plugin_id,
                &notice.command_id,
                notice.surface_id,
            );
        }
        for open in drained {
            // 큐에 넣는 쪽이 먼저 거르지만, 큐에 넣은 뒤 선언이 바뀐 경우를 위해 여기서도 거른다.
            if crate::plugin_bridge::popup_scope::lacks_required_target(
                crate::plugin_bridge::popup_scope::declared_popup_scope(
                    mgr,
                    &open.plugin_id,
                    &open.popup_id,
                ),
                open.target_surface,
            ) {
                tracing::warn!(
                    "popup '{}/{}' not opened: it needs a surface target and none is focused",
                    open.plugin_id,
                    open.popup_id
                );
                continue;
            }
            let Some(instance_id) =
                mgr.open_popup_instance(&open.plugin_id, &open.popup_id, open.context)
            else {
                continue;
            };
            // 대상 surface 를 결속한다. 범위는 선언과 결속 대상으로 렌더 경로가 정한다.
            if let Some(sid) = open.target_surface {
                mgr.bind_popup_instance_surface(instance_id, sid);
            }
        }
    }
}
