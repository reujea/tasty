use super::*;
use crate::{SplitTree, Workspace};

type Result<T> = std::result::Result<T, Rejection>;

pub(super) fn decide(m: &JournalModel, cmd: &StructuralCommand) -> Result<StructuralDecision> {
    match cmd {
        StructuralCommand::ResetCategories
        | StructuralCommand::CreateCategory { .. }
        | StructuralCommand::RenameCategory { .. }
        | StructuralCommand::DeleteCategory { .. }
        | StructuralCommand::ReorderCategory { .. } => category(m, cmd),
        StructuralCommand::SetWorkspaceCategory { .. }
        | StructuralCommand::UpdateWorkspaceMeta { .. }
        | StructuralCommand::SetWorkspaceAttachMapping { .. }
        | StructuralCommand::MoveWorkspace { .. } => workspace_change(m, cmd),
        StructuralCommand::SetRemoteRow { .. } | StructuralCommand::RemoveRemoteRow { .. } => {
            remote_row(m, cmd)
        }
        StructuralCommand::RenameTab { .. }
        | StructuralCommand::MoveTab { .. }
        | StructuralCommand::SetPaneRatio { .. }
        | StructuralCommand::SetSurfaceRatio { .. } => tab_or_ratio(m, cmd),
        StructuralCommand::ReconcileRetirement { .. }
        | StructuralCommand::RecoverOperation { .. }
        | StructuralCommand::Replace { .. }
        | StructuralCommand::PrepareForward { .. }
        | StructuralCommand::FinishForward { .. }
        | StructuralCommand::PrepareAssembly { .. }
        | StructuralCommand::RecordCapture { .. }
        | StructuralCommand::Close { .. }
        | StructuralCommand::FinishRetirement { .. }
        | StructuralCommand::ReconcilePreparation { .. }
        | StructuralCommand::RetireEngine { .. }
        | StructuralCommand::OpenEngine { .. }
        | StructuralCommand::PrepareCreation { .. }
        | StructuralCommand::FinishCreation { .. }
        | StructuralCommand::FinishCleanup { .. }
        | StructuralCommand::CancelUnstartedCreation { .. }
        | StructuralCommand::RejectInstallation { .. }
        | StructuralCommand::MarkPreparationUncertain { .. } => {
            unreachable!("command family is dispatched before metadata")
        }
    }
}

fn category(m: &JournalModel, cmd: &StructuralCommand) -> Result<StructuralDecision> {
    let mut events = Vec::new();
    let result = match cmd {
        StructuralCommand::ResetCategories => {
            for (index, workspace) in m.workspace_order.iter().enumerate() {
                if m.workspaces[workspace].category != 0 {
                    events.push(DomainEvent::WorkspaceMoved {
                        id: *workspace,
                        category: 0,
                        index,
                    });
                }
            }
            events.extend(
                m.category_order
                    .iter()
                    .filter(|id| **id != 0)
                    .map(|id| DomainEvent::CategoryClosed { id: *id }),
            );
            StructuralResult::Updated
        }
        StructuralCommand::CreateCategory { reserved_id, name } => {
            if *reserved_id == 0 || m.categories.contains_key(reserved_id) {
                return Err(Rejection("category ID was not freshly reserved".into()));
            }
            let name = tasty_model::validate_new_category_name(
                name,
                m.categories.values().map(|c| c.name.as_str()),
            )
            .map_err(|e| Rejection(e.to_string()))?;
            events.push(DomainEvent::CategoryCreated {
                id: *reserved_id,
                name,
                index: m.category_order.len(),
            });
            StructuralResult::CreatedCategory { id: *reserved_id }
        }
        StructuralCommand::RenameCategory { id, name } => {
            mutable_category(m, *id)?;
            let others = m
                .categories
                .iter()
                .filter(|(key, _)| **key != *id)
                .map(|(_, c)| c.name.as_str());
            let name = tasty_model::validate_rename_category_name(name, others)
                .map_err(|e| Rejection(e.to_string()))?;
            events.push(DomainEvent::CategoryRenamed { id: *id, name });
            StructuralResult::Updated
        }
        StructuralCommand::DeleteCategory { id } => {
            mutable_category(m, *id)?;
            for (index, wsid) in m.workspace_order.iter().enumerate() {
                if m.workspaces[wsid].category == *id {
                    events.push(DomainEvent::WorkspaceMoved {
                        id: *wsid,
                        category: 0,
                        index,
                    });
                }
            }
            events.push(DomainEvent::CategoryClosed { id: *id });
            StructuralResult::Updated
        }
        StructuralCommand::ReorderCategory { id, to_index } => {
            if *id == 0 || *to_index == 0 {
                return Err(Rejection(
                    "the 'normal' category is fixed at position 0".into(),
                ));
            }
            let from = m
                .category_order
                .iter()
                .position(|x| x == id)
                .ok_or_else(|| Rejection("category index out of range".into()))?;
            if *to_index >= m.category_order.len() {
                return Err(Rejection("category index out of range".into()));
            }
            if from != *to_index {
                events.push(DomainEvent::CategoryMoved {
                    id: *id,
                    index: *to_index,
                });
            }
            StructuralResult::Updated
        }
        _ => unreachable!("command family is dispatched above"),
    };
    Ok(StructuralDecision {
        events,
        result,
        effects: Vec::new(),
        completed_command: None,
    })
}

fn workspace_change(m: &JournalModel, cmd: &StructuralCommand) -> Result<StructuralDecision> {
    let mut events = Vec::new();
    let result = match cmd {
        StructuralCommand::SetWorkspaceCategory {
            workspace_id,
            category,
        } => {
            if !m.categories.contains_key(category) {
                return Err(Rejection("category not found".into()));
            }
            let index = m
                .workspace_order
                .iter()
                .position(|id| id == workspace_id)
                .ok_or_else(|| Rejection("workspace not found".into()))?;
            events.push(DomainEvent::WorkspaceMoved {
                id: *workspace_id,
                category: *category,
                index,
            });
            StructuralResult::Updated
        }
        StructuralCommand::UpdateWorkspaceMeta {
            workspace_id,
            name,
            subtitle,
            description,
        } => {
            let ws = workspace(m, *workspace_id)?;
            if let Some(name) = name {
                events.push(DomainEvent::WorkspaceRenamed {
                    id: *workspace_id,
                    name: name.clone(),
                });
            }
            if subtitle.is_some() || description.is_some() {
                events.push(DomainEvent::WorkspaceDetailsSet {
                    id: *workspace_id,
                    subtitle: subtitle.clone().unwrap_or_else(|| ws.subtitle.clone()),
                    description: description
                        .clone()
                        .unwrap_or_else(|| ws.description.clone()),
                });
            }
            StructuralResult::Updated
        }
        StructuralCommand::SetWorkspaceAttachMapping {
            workspace_id,
            mapping,
        } => {
            workspace(m, *workspace_id)?;
            events.push(DomainEvent::WorkspaceAttachMappingSet {
                id: *workspace_id,
                mapping: mapping.clone(),
            });
            StructuralResult::Updated
        }
        StructuralCommand::MoveWorkspace {
            workspace_id,
            to_index,
        } => {
            let from = m.workspace_order.iter().position(|id| id == workspace_id);
            let moved =
                from.is_some_and(|from| from != *to_index && *to_index < m.workspace_order.len());
            if moved {
                events.push(DomainEvent::WorkspaceMoved {
                    id: *workspace_id,
                    category: m.workspaces[workspace_id].category,
                    index: *to_index,
                });
            }
            StructuralResult::Moved { moved }
        }
        _ => unreachable!("command family is dispatched above"),
    };
    Ok(StructuralDecision {
        events,
        result,
        effects: Vec::new(),
        completed_command: None,
    })
}

fn tab_or_ratio(m: &JournalModel, cmd: &StructuralCommand) -> Result<StructuralDecision> {
    let mut events = Vec::new();
    let result = match cmd {
        StructuralCommand::RenameTab { tab_id, name } => {
            if !m.tabs.contains_key(tab_id) {
                return Err(Rejection(format!("Tab id {tab_id} not found")));
            }
            events.push(DomainEvent::TabExplicitNameSet {
                id: *tab_id,
                name: name.clone(),
            });
            StructuralResult::Updated
        }
        StructuralCommand::MoveTab {
            pane_id,
            tab_id,
            to_index,
        } => {
            let moved = m.panes.get(pane_id).is_some_and(|pane| {
                pane.tabs
                    .iter()
                    .position(|id| id == tab_id)
                    .is_some_and(|from| from != *to_index && *to_index < pane.tabs.len())
            });
            if moved {
                events.push(DomainEvent::TabMoved {
                    id: *tab_id,
                    pane: *pane_id,
                    index: *to_index,
                });
            }
            StructuralResult::Moved { moved }
        }
        StructuralCommand::SetPaneRatio {
            workspace_id,
            path,
            expected_leaves,
            expected_revision,
            ratio,
        } => {
            check_revision(m, *expected_revision)?;
            check_split(&workspace(m, *workspace_id)?.layout, path, expected_leaves)?;
            events.push(DomainEvent::PaneRatioSet {
                workspace: *workspace_id,
                path: path.clone(),
                ratio: *ratio,
            });
            StructuralResult::Updated
        }
        StructuralCommand::SetSurfaceRatio {
            tab_id,
            path,
            expected_leaves,
            expected_revision,
            ratio,
        } => {
            let tab = m
                .tabs
                .get(tab_id)
                .ok_or_else(|| Rejection(format!("Tab id {tab_id} not found")))?;
            check_revision(m, *expected_revision)?;
            check_split(&tab.layout, path, expected_leaves)?;
            events.push(DomainEvent::SurfaceRatioSet {
                tab: *tab_id,
                path: path.clone(),
                ratio: *ratio,
            });
            StructuralResult::Updated
        }
        _ => unreachable!("command family is dispatched above"),
    };
    Ok(StructuralDecision {
        events,
        result,
        effects: Vec::new(),
        completed_command: None,
    })
}

fn mutable_category(m: &JournalModel, id: u32) -> Result<()> {
    if id == 0 {
        return Err(Rejection(
            "the 'normal' category cannot be renamed or deleted".into(),
        ));
    }
    if !m.categories.contains_key(&id) {
        return Err(Rejection("category not found".into()));
    }
    Ok(())
}

fn workspace(m: &JournalModel, id: u32) -> Result<&Workspace> {
    m.workspaces
        .get(&id)
        .ok_or_else(|| Rejection(format!("Workspace id {id} not found")))
}

fn check_split(tree: &SplitTree<u32>, path: &[bool], expected: &[u32]) -> Result<()> {
    let mut node = tree;
    for second in path {
        node = match node {
            SplitTree::Split {
                first,
                second: other,
                ..
            } => {
                if *second {
                    other
                } else {
                    first
                }
            }
            SplitTree::Leaf(_) => return Err(Rejection("split target no longer exists".into())),
        };
    }
    if matches!(node, SplitTree::Leaf(_)) || node.leaves() != expected {
        return Err(Rejection(
            "split target changed before the command was committed".into(),
        ));
    }
    Ok(())
}

fn check_revision(m: &JournalModel, expected: u64) -> Result<()> {
    if m.applied.revision != Some(expected) {
        return Err(Rejection("split changed since the drag began".into()));
    }
    Ok(())
}

fn remote_row(m: &JournalModel, cmd: &StructuralCommand) -> Result<StructuralDecision> {
    let mut events = Vec::new();
    match cmd {
        StructuralCommand::SetRemoteRow { id, row } => {
            if m.workspaces.contains_key(id) {
                return Err(Rejection(format!(
                    "workspace {id} is local and cannot be a remote row"
                )));
            }
            let mut row = row.clone();
            if !m.categories.contains_key(&row.category) {
                row.category = tasty_model::NORMAL_CATEGORY_ID;
            }
            if m.remote_rows.get(id) != Some(&row) {
                events.push(DomainEvent::RemoteRowSet { id: *id, row });
            }
        }
        StructuralCommand::RemoveRemoteRow { id } => {
            if m.remote_rows.contains_key(id) {
                events.push(DomainEvent::RemoteRowRemoved { id: *id });
            }
        }
        _ => unreachable!("remote row command family"),
    }
    Ok(StructuralDecision {
        events,
        effects: Vec::new(),
        result: StructuralResult::Updated,
        completed_command: None,
    })
}
