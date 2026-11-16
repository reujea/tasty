//! 입력 이벤트가 난 범위 경로(window → workspace → pane → tab → surface)를 계산한다.
//! 팝업이 이벤트에 반응할지는 그 팝업의 범위가 이 경로에 있는지로 정한다.
//! 포인터는 좌표가 든 가장 깊은 노드까지, 키보드는 포커스된 surface 와 그 조상을 경로로 쓴다.

use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::PopupScope;
use crate::core::CoreState;
use crate::state::MainViewState;

/// 이벤트가 속한 범위 노드들. 비어 있는 단계는 이벤트가 그 깊이까지 내려가지 않았다는 뜻이다.
/// 경로는 이 모듈의 계산 함수로만 만든다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct ScopePath {
    workspace: Option<u32>,
    pane: Option<u32>,
    tab: Option<u32>,
    surface: Option<u32>,
}

impl ScopePath {
    /// `scope` 가 이 경로 위에 있는지. `Window` 는 모든 경로의 뿌리라 항상 참이다.
    pub(crate) fn contains(&self, scope: &PopupScope) -> bool {
        match scope {
            PopupScope::Window => true,
            PopupScope::Workspace(id) => self.workspace == Some(*id),
            PopupScope::Pane(id) => self.pane == Some(*id),
            PopupScope::Tab(id) => self.tab == Some(*id),
            PopupScope::Surface(id) => self.surface == Some(*id),
        }
    }
}

/// 좌표 `pos` 의 범위 경로. surface 안이면 surface 와 그 조상을, 탭 바처럼 pane 안이지만 surface 밖이면
/// pane 과 workspace 를, workspace 작업 영역 밖(사이드바·상태바)이면 window 만 담는다.
/// Workspace 는 작업 영역(`workspace_rect`) 안일 때만 담는다. 작업 영역이 없는 창은 담지 않는다.
pub(crate) fn path_at(layout: &LayoutContext, pos: egui::Pos2) -> ScopePath {
    let workspace = layout
        .workspace_rect
        .is_some_and(|r| r.contains(pos))
        .then_some(layout.active_workspace_id);
    if let Some(&(surface, _)) = layout.surface_rects.iter().find(|(_, r)| r.contains(pos)) {
        let owner = layout.surface_owners.iter().find(|o| o.surface == surface);
        return ScopePath {
            workspace,
            pane: owner.map(|o| o.pane),
            tab: owner.map(|o| o.tab),
            surface: Some(surface),
        };
    }
    if let Some(&(pane, _)) = layout.pane_rects.iter().find(|(_, r)| r.contains(pos)) {
        return ScopePath {
            workspace,
            pane: Some(pane),
            ..ScopePath::default()
        };
    }
    ScopePath {
        workspace,
        ..ScopePath::default()
    }
}

/// 포커스된 surface 와 그 조상의 범위 경로. `EngineRead` 는 `CoreState` 로 역참조되므로 두 쪽에서 부를 수
/// 있다. 포커스된 surface 가 없으면 활성 workspace 까지만 담는다.
pub(crate) fn path_of_focus(state: &MainViewState, core: &CoreState) -> ScopePath {
    if core.workspaces().is_empty() {
        return ScopePath::default();
    }
    let Some(surface) = state.focused_surface_id(core) else {
        return ScopePath {
            workspace: Some(state.active_workspace(core).id),
            ..ScopePath::default()
        };
    };
    let workspace = core
        .find_workspace_index_for_surface(surface)
        .and_then(|(index, _)| core.workspace_at(index))
        .map(|ws| ws.id);
    ScopePath {
        workspace,
        pane: core.find_pane_for_surface(surface),
        tab: core.find_tab_for_surface(surface),
        surface: Some(surface),
    }
}

/// 키 입력이 갈 곳.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyTarget {
    /// 이 surface 가 키를 받는다.
    Surface(u32),
    /// 팝업·대화상자 같은 오버레이가 키를 받는다.
    Overlay,
}

/// 지금 입력 경로가 쓰는 키 게이트를 그대로 감싼다. 게이트가 열려 있거나 전체화면 무대가 떠 있으면
/// `Overlay`, 아니면 포커스된 surface 다. 무대는 창의 키 처리가 게이트보다 먼저 소비하므로 같은 답을
/// 낸다. 포커스된 surface 가 없으면 `None` 이다.
pub(crate) fn keyboard_target(state: &MainViewState, core: &CoreState) -> Option<KeyTarget> {
    if state.keyboard_overlay_open() || state.fullscreen_stage_active() {
        return Some(KeyTarget::Overlay);
    }
    state.focused_surface_id(core).map(KeyTarget::Surface)
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
