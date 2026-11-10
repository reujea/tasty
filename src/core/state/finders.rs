use crate::runtime::engine_access::{EngineMut, EngineRef};
impl EngineRef<'_> {
    #[cfg(feature = "gui")]
    pub(crate) fn mirror_workspace_index_for_structural(
        &self,
        intent: &crate::app::command::DomainIntent,
    ) -> Option<usize> {
        mirror_workspace_index_for_structural(self.core, intent)
    }
}
/// Pure target lookup for an App command; no runtime or current View is required.
pub(crate) fn mirror_workspace_index_for_structural(
    core: &crate::core::CoreState,
    intent: &crate::app::command::DomainIntent,
) -> Option<usize> {
    use crate::app::command::DomainIntent as D;
    let ws_idx = match intent {
        D::CloseWorkspace { workspace_id } => core.find_workspace_index_for_id(*workspace_id),
        D::RetireExitedSurface { surface_id, .. } => core
            .find_workspace_index_for_surface(*surface_id)
            .map(|(index, _)| index),
        D::SplitSurface {
            target_surface_id: sid,
            ..
        }
        | D::CloseSurface {
            surface_id: sid, ..
        }
        | D::ConvertSurface {
            surface_id: sid, ..
        } => core.find_workspace_index_for_surface(*sid).map(|(i, _)| i),
        D::MoveSurface {
            source_surface_id,
            target_surface_id,
        } => {
            // 이동·교체는 양쪽 중 하나라도 mirror이면 로컬에서 실행하지 않는다.
            core.find_workspace_index_for_surface(*source_surface_id)
                .map(|(i, _)| i)
                .filter(|&i| core.workspace_at(i).is_some_and(|w| w.mirror))
                .or_else(|| {
                    core.find_workspace_index_for_surface(*target_surface_id)
                        .map(|(i, _)| i)
                })
        }
        D::ReplaceTabWithTab {
            source_tab_id,
            target_tab_id,
        } => {
            // 탭 이동도 양쪽 중 하나라도 mirror이면 로컬에서 실행하지 않는다.
            let ws_of_tab = |tab_id: u32| {
                core.find_pane_for_tab(tab_id)
                    .and_then(|pid| core.find_workspace_index_for_pane(pid))
            };
            ws_of_tab(*source_tab_id)
                .filter(|&i| core.workspace_at(i).is_some_and(|w| w.mirror))
                .or_else(|| ws_of_tab(*target_tab_id))
        }
        D::ReplacePaneWithPane {
            source_pane_id,
            target_pane_id,
        } => {
            // 페인 이동도 양쪽 중 하나라도 mirror이면 로컬에서 실행하지 않는다.
            core.find_workspace_index_for_pane(*source_pane_id)
                .filter(|&i| core.workspace_at(i).is_some_and(|w| w.mirror))
                .or_else(|| core.find_workspace_index_for_pane(*target_pane_id))
        }
        D::SplitPane {
            target_pane_id: pid,
            ..
        }
        | D::CreateTab { pane_id: pid, .. }
        | D::ClosePane { pane_id: pid }
        | D::MoveTab { pane_id: pid, .. } => core.find_workspace_index_for_pane(*pid),
        D::CloseTab { tab_id } => core
            .find_pane_for_tab(*tab_id)
            .and_then(|pid| core.find_workspace_index_for_pane(pid)),
        D::RestoreClosedItem { target_pane_id, .. } => {
            core.find_workspace_index_for_pane((*target_pane_id)?)
        }
        // 이름·카테고리는 원격 트리 구조가 아니다. mirror에서도 로컬에만 적용하며 forward하지 않는다.
        D::SetWorkspaceCategory { .. }
        | D::CreateCategory { .. }
        | D::RenameCategory { .. }
        | D::DeleteCategory { .. } => return None,
        _ => return None,
    }?;
    core.workspace_at(ws_idx)
        .filter(|w| w.mirror)
        .map(|_| ws_idx)
}
impl EngineMut<'_> {
    /// 현재 트리에 없는 자식 등록을 정리한다. 변경이 있으면 저장을 시도한다.
    pub fn reconcile_child_terminals(&mut self) {
        let live = self.live_surface_ids();
        let summary = self
            .runtime
            .child_terminals
            .reconcile_with_live_surfaces(&live);
        if summary.changed() {
            self.runtime.child_terminals.save();
        }
    }
}
