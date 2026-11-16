//! 도구 메뉴의 popup 열기 요청을 플러그인 매니저로 전달한다.

use crate::app::App;

impl App {
    pub(crate) fn dispatch_pending_popup_opens(&mut self) {
        let mut drained: Vec<crate::state::PendingPopupOpen> = Vec::new();
        for (s, _engine) in self.engines_mut().sessions() {
            drained.append(&mut s.pending_popup_opens);
        }
        if drained.is_empty() {
            return;
        }
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        for open in drained {
            // surface 선언 팝업은 대상 surface 가 없으면 창 범위로 바꾸지 않고 열지 않는다.
            if open.target_surface.is_none()
                && crate::plugin_bridge::popup_scope::declared_popup_scope(
                    mgr,
                    &open.plugin_id,
                    &open.popup_id,
                ) == Some(tasty_plugin_manifest::PopupScopeDecl::Surface)
            {
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
