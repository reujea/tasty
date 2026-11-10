use std::path::PathBuf;

use super::{
    NORMAL_CATEGORY_ID, Pane, PaneId, PaneNode, RemoteBinding, SurfaceId, TabId,
    WorkspaceCategoryId, WorkspaceId,
};

/// workspace attach의 surface 분류.
/// - terminals: 실제 터미널 또는 PTY 생성 대기 placeholder.
/// - non_terminals: 전용 mirror 경로가 없어 placeholder로 보낼 surface.
/// - mesh_candidates/content_candidates: trait이 반환한 후보이며 최종 허용 여부는 호스트가 검사한다.
/// - explorers: 활성 내부 탭의 현재 경로로 만든 읽기 전용 원격 탐색 대상.
///
/// 허용되지 않은 후보는 호스트가 placeholder로 처리해야 한다.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AttachSurfaceClass {
    pub terminals: Vec<SurfaceId>,
    pub non_terminals: Vec<SurfaceId>,
    pub mesh_candidates: Vec<(SurfaceId, String, String)>,
    pub explorers: Vec<(SurfaceId, PathBuf)>,
    pub content_candidates: Vec<(SurfaceId, String, String, Option<PathBuf>)>,
}

/// Workspace - one sidebar item. Contains a PaneLayout (binary split tree of Panes).
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub subtitle: String,
    pub description: String,
    /// Always `Some` during normal operation. Temporarily `None` during structural mutations.
    pane_layout_opt: Option<PaneNode>,
    /// 원격 workspace를 보여 주는 client mirror인지. UI 구분 표시에 사용한다.
    pub mirror: bool,
    /// 사용자가 GUI로 붙여 저장되는 원격 행의 연결 대상. mirror에만 있다. 없는 mirror는 에이전트가
    /// 만든 것이라 저장하지 않고 끊기면 정리한다.
    pub remote_row: Option<RemoteBinding>,
    /// 이 워크스페이스가 속한 카테고리(사이드바 폴더) id. 기본값은 예약된
    /// `normal`([`NORMAL_CATEGORY_ID`]). layout.json 으로 영속(`SavedWorkspace.category`).
    pub category: WorkspaceCategoryId,
}

impl Workspace {
    /// Create a workspace with a TerminalSurface marker. Caller must have already
    /// `engine.runtime.terminals.insert(surface_id, terminal)` for the spawned Terminal.
    pub fn new_with_terminal_marker(
        id: WorkspaceId,
        name: String,
        pane_id: PaneId,
        tab_id: TabId,
        surface_id: SurfaceId,
    ) -> Self {
        let pane = Pane::new_with_terminal_marker(pane_id, tab_id, surface_id);
        Self {
            id,
            name,
            subtitle: String::new(),
            description: String::new(),
            pane_layout_opt: Some(PaneNode::Leaf(pane)),
            mirror: false,
            remote_row: None,
            category: NORMAL_CATEGORY_ID,
        }
    }

    /// Access the pane layout (always valid during normal operation).
    /// Panics if called during a structural mutation (between take/put).
    #[track_caller]
    pub fn pane_layout(&self) -> &PaneNode {
        self.pane_layout_opt
            .as_ref()
            .expect("BUG: pane_layout accessed during structural mutation (between take/put)")
    }

    /// Access the pane layout mutably.
    /// Panics if called during a structural mutation (between take/put).
    #[track_caller]
    pub fn pane_layout_mut(&mut self) -> &mut PaneNode {
        self.pane_layout_opt
            .as_mut()
            .expect("BUG: pane_layout accessed during structural mutation (between take/put)")
    }

    /// Remove a pane without making a presentation choice.
    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        self.pane_layout_mut().close_pane(pane_id)
    }

    /// Detach a pane for a structural move. The caller repairs its selection.
    pub fn detach_pane(&mut self, pane_id: PaneId) -> Option<Pane> {
        self.pane_layout_mut().detach_pane(pane_id)
    }

    /// Create a workspace from a pre-built Pane (for non-terminal surface types).
    pub fn new_with_pane(id: WorkspaceId, name: String, pane: Pane) -> Self {
        Self {
            id,
            name,
            subtitle: String::new(),
            description: String::new(),
            pane_layout_opt: Some(PaneNode::Leaf(pane)),
            mirror: false,
            remote_row: None,
            category: NORMAL_CATEGORY_ID,
        }
    }

    /// Create a workspace from a restored pane layout (no PTY creation needed).
    pub fn from_restored(
        id: WorkspaceId,
        name: String,
        subtitle: String,
        pane_layout: PaneNode,
    ) -> Self {
        Self {
            id,
            name,
            subtitle,
            description: String::new(),
            pane_layout_opt: Some(pane_layout),
            mirror: false,
            remote_row: None,
            category: NORMAL_CATEGORY_ID,
        }
    }

    /// 아직 붙지 않은 저장 원격 행. 탭·pane·surface가 없어 화면에는 바탕색과 연결 팝업만 보인다.
    /// 형상이 없음을 나타내는 자리 표시 pane 하나(id 0, 탭 없음)만 가진다.
    pub fn new_remote_row(id: WorkspaceId, name: String, binding: RemoteBinding) -> Self {
        let mut workspace = Self::new_with_pane(id, name, Pane::default());
        workspace.mirror = true;
        workspace.remote_row = Some(binding);
        workspace
    }

    /// GUI로 붙여 저장되는 원격 행인지. 에이전트가 만든 mirror는 false다.
    pub fn is_saved_remote_row(&self) -> bool {
        self.mirror && self.remote_row.is_some()
    }

    /// 형상(탭·surface)이 없는 workspace인지. 아직 붙지 않은 원격 행이 이렇다.
    pub fn is_bare(&self) -> bool {
        self.pane_layout_opt.as_ref().is_some_and(|layout| {
            layout.all_pane_ids().iter().all(|id| {
                layout
                    .find_pane(*id)
                    .is_some_and(|pane| pane.tabs.is_empty())
            })
        })
    }

    /// 카테고리 소속을 변경한다.
    pub fn set_category(&mut self, category: WorkspaceCategoryId) {
        self.category = category;
    }

    /// Collect all surface IDs in this workspace.
    pub fn all_surface_ids(&self) -> Vec<SurfaceId> {
        self.pane_layout().all_surface_ids()
    }

    /// client mirror 복원용 전체 pane·tab·surface 트리. 분할 방향과 비율을 보존한다.
    pub fn to_attach_tree_json(
        &self,
        presentation: &(impl crate::StructurePresentation + ?Sized),
    ) -> serde_json::Value {
        let panes: Vec<_> = self
            .pane_layout()
            .all_pane_ids()
            .iter()
            .filter_map(|&pid| self.pane_layout().find_pane(pid))
            .map(|pane| pane.to_attach_json(presentation))
            .collect();
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "focused_pane": presentation.pane_id(self).unwrap_or(0),
            "panes": panes,
            "pane_layout": self.pane_layout().to_tree_json_full(presentation),
        })
    }

    /// Produce a JSON tree representation of this workspace.
    pub fn to_tree_json(
        &self,
        presentation: &(impl crate::StructurePresentation + ?Sized),
        surface_json: &dyn Fn(SurfaceId) -> serde_json::Value,
    ) -> serde_json::Value {
        let panes: Vec<_> = self
            .pane_layout()
            .all_pane_ids()
            .iter()
            .filter_map(|&pid| self.pane_layout().find_pane(pid))
            .map(|pane| {
                let mut p = pane.to_tree_json(presentation, surface_json);
                p["focused"] = serde_json::json!(Some(pane.id) == presentation.pane_id(self));
                p
            })
            .collect();
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "panes": panes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SplitDirection;

    fn leaf_pane(id: PaneId, tab_id: TabId, surface_id: SurfaceId) -> Pane {
        Pane::new_with_surface(
            id,
            tab_id,
            "Shell".to_string(),
            crate::SurfaceDescriptor::new(surface_id, "empty"),
        )
    }

    #[test]
    fn to_attach_tree_json_includes_pane_layout_with_direction_and_ratio() {
        let pane_layout = PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.3,
            first: Box::new(PaneNode::Leaf(leaf_pane(1, 1, 1))),
            second: Box::new(PaneNode::Leaf(leaf_pane(2, 2, 2))),
        };
        let ws = Workspace::from_restored(9, "test".to_string(), String::new(), pane_layout);
        let json = ws.to_attach_tree_json(&crate::StructurePresentationSnapshot::default());
        assert_eq!(json["pane_layout"]["type"], "Split");
        assert_eq!(json["pane_layout"]["direction"], "vertical");
        assert!((json["pane_layout"]["ratio"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        // 기존 평면 "panes" 필드도 여전히 존재(하위호환)해야 한다.
        assert_eq!(json["panes"].as_array().unwrap().len(), 2);
    }

    /// pane을 세 개 만들어 비포커스 pane 제거 뒤에도 원래 포커스가 유지되는지 구분한다.
    fn three_pane_ws() -> Workspace {
        let pane_layout = PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf(leaf_pane(1, 1, 1))),
            second: Box::new(PaneNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf(leaf_pane(2, 2, 2))),
                second: Box::new(PaneNode::Leaf(leaf_pane(3, 3, 3))),
            }),
        };
        Workspace::from_restored(9, "test".to_string(), String::new(), pane_layout)
    }

    #[test]
    fn closing_an_unfocused_pane_leaves_focus_where_it_was() {
        let mut ws = three_pane_ws();
        assert!(ws.close_pane(1));
        assert!(ws.pane_layout().find_pane(3).is_some());
    }

    #[test]
    fn closing_the_focused_pane_moves_focus_to_the_first_survivor() {
        let mut ws = three_pane_ws();
        assert!(ws.close_pane(3));
        assert!(ws.pane_layout().find_pane(3).is_none());
    }

    /// 없는 pane은 닫지 않았다고 반환하며 포커스를 유지한다.
    #[test]
    fn a_close_that_removed_nothing_reports_it() {
        let mut ws = three_pane_ws();
        assert!(!ws.close_pane(77));
        assert!(ws.pane_layout().find_pane(3).is_some());
    }
}
