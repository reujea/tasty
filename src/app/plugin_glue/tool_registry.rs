//! 현재 플러그인 상태를 도구 메뉴에 반영한다.

use crate::app::App;

impl App {
    /// 플러그인 활성 상태나 ui.tool_item 권한이 바뀌면 갱신한다.
    /// 창 복원 때 옛 목록을 표시하지 않도록 parked 상태도 함께 갱신한다.
    pub(crate) fn refresh_tool_registry(&mut self) {
        let (items, surface_popups) = match self.plugin_manager.as_ref() {
            Some(mgr) => (mgr.plugin_tool_items(), mgr.plugin_surface_popup_ids()),
            None => return,
        };
        for (state, _engine) in self.engines_mut().sessions() {
            state.tool_registry.set_plugin_items(items.clone());
            state
                .tool_registry
                .set_surface_popups(surface_popups.clone());
        }
    }
}
