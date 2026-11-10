use crate::core::CoreState;
pub(crate) fn presentation(
    core: &CoreState,
    view: Option<crate::core::layout_persistence::import::ImportedView>,
) -> crate::model::RestoredPresentation {
    let view = view.unwrap_or_default();
    let mut result = crate::model::RestoredPresentation {
        active_workspace: view
            .active_workspace
            // 저장된 원격 행도 다시 열 때 선택된 workspace로 돌아온다. 에이전트 mirror는 남지 않는다.
            .filter(|id| {
                core.workspaces().iter().any(|workspace| {
                    workspace.id == *id && (!workspace.mirror || workspace.is_saved_remote_row())
                })
            })
            .or_else(|| {
                core.local_workspaces()
                    .first()
                    .map(|workspace| workspace.id)
            }),
        selection: crate::model::StructurePresentationSnapshot::default(),
    };
    result.selection.panes.extend(view.focused_panes);
    result.selection.selected_tabs.extend(view.active_tabs);
    result.selection.surfaces.extend(view.selected_surfaces);
    result
        .selection
        .collapsed_categories
        .extend(view.collapsed_categories);
    for workspace in core.local_workspaces() {
        for id in workspace.pane_layout().all_pane_ids() {
            if let Some(pane) = workspace.pane_layout().find_pane(id) {
                for tab in &pane.tabs {
                    if let Some(surface) = tab.first_surface_id() {
                        result.selection.surfaces.entry(tab.id).or_insert(surface);
                    }
                    let mut nodes = Vec::new();
                    tab.layout().split_node_ids(&mut nodes);
                    result
                        .selection
                        .split_hints
                        .extend(nodes.into_iter().map(|node| (node, false)));
                }
            }
        }
    }
    result
}
