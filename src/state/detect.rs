//! 폴링으로 surface·워크스페이스·탭 포커스와 pane 간 탭 이동을 감지한다.

use super::{PendingHostEvent, RequestContext};
use crate::core::CoreState;

impl RequestContext {
    /// plugin `surface.focused`로 갈 이벤트. 화면에 낸 프레임에서 host 키 포커스가 surface로 들어갈 때마다 하나다
    /// (`MainViewState::take_key_focus_entries`). 키를 가진 팝업이 닫혀 같은 surface로 돌아오면 앞 값 없이 다시 낸다.
    /// 키 포커스가 팝업으로 가는 전이는 이 이벤트로 알리지 않는다(새 surface가 없다).
    pub fn detect_focus_change(&mut self) -> Vec<PendingHostEvent> {
        self.take_key_focus_entries()
            .into_iter()
            .map(
                |(surface_id, prev_surface_id)| PendingHostEvent::SurfaceFocused {
                    surface_id,
                    prev_surface_id,
                },
            )
            .collect()
    }

    /// 선택 surface가 바뀌었으면 새 선택 surface. 탭 제목의 OSC 제목을 그 surface 것으로 다시 읽는 데 쓴다.
    /// 키 포커스와 무관하게 선택을 따른다(팝업 아래에서 선택이 바뀌어도 제목은 바뀐다).
    pub fn detect_selection_change(&mut self, engine: &CoreState) -> Option<u32> {
        let current = self.focused_surface_id(engine);
        if current == self.last_focused_surface_id {
            return None;
        }
        self.last_focused_surface_id = current;
        current
    }

    pub fn detect_workspace_activation(&mut self, engine: &CoreState) -> Vec<PendingHostEvent> {
        let mut events = Vec::new();
        let current = engine
            .workspace_at(self.active_workspace_index(engine))
            .map(|w| w.id);
        if current == self.last_active_workspace_id {
            return events;
        }
        let prev = self.last_active_workspace_id;
        self.last_active_workspace_id = current;
        if let Some(workspace_id) = current {
            events.push(PendingHostEvent::WorkspaceActivated {
                workspace_id,
                prev_workspace_id: prev,
            });
        }
        events
    }

    pub fn detect_tab_focus_change(&mut self, engine: &CoreState) -> Vec<PendingHostEvent> {
        let mut events = Vec::new();
        let current = self.focused_pane(engine).and_then(|pane| {
            pane.tabs
                .get(self.navigation.tab_index(pane))
                .map(|tab| (pane.id, tab.id))
        });
        if current == self.last_focused_tab {
            return events;
        }
        let prev_tab_id = self.last_focused_tab.map(|(_, tab_id)| tab_id);
        self.last_focused_tab = current;
        if let Some((pane_id, tab_id)) = current {
            events.push(PendingHostEvent::TabFocused {
                tab_id,
                pane_id,
                prev_tab_id,
            });
        }
        events
    }
}
