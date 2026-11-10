//! Logical bootstrap before the first publication ACK. It does not read payloads or create resources.
use super::*;
use crate::SplitTree;
use tasty_model::{PaneNode, SplitNodeId, SurfaceLayout, WorkspaceCategory};

pub fn initialize(core: &mut CoreState, model: &JournalModel) -> Result<()> {
    if !core.local_workspaces.is_empty() {
        return Err("bootstrap cannot replace an already published local tree".into());
    }
    let workspaces = model
        .workspace_order
        .iter()
        .map(|id| {
            let source = model
                .workspaces
                .get(id)
                .ok_or("bootstrap workspace missing")?;
            let layout = pane_tree(model, &source.layout)?;
            let mut workspace = Workspace::new_with_pane(*id, source.name.clone(), Pane::default());
            *workspace.pane_layout_mut() = layout;
            workspace.category = source.category;
            workspace.subtitle = source.subtitle.clone();
            workspace.description = source.description.clone();
            Ok(workspace)
        })
        .collect::<Result<Vec<_>>>()?;
    let categories = model
        .category_order
        .iter()
        .map(|id| {
            Ok(WorkspaceCategory::new(
                *id,
                model
                    .categories
                    .get(id)
                    .ok_or("bootstrap category missing")?
                    .name
                    .clone(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    core.replace_local_workspaces(workspaces);
    core.categories = categories;
    if live::core_canonical(core) != Canonical::of_journal(model, &SkipData) {
        return Err("bootstrap live projection differs from journal structure".into());
    }
    // 원격 행은 로컬 구조 비교 대상이 아니므로 비교한 뒤에 넣는다.
    let mut rows: Vec<_> = model.remote_rows.iter().collect();
    rows.sort_by_key(|(id, row)| (row.index, **id));
    for (id, row) in rows {
        let mut workspace = Workspace::new_remote_row(*id, row.name.clone(), row.binding.clone());
        workspace.subtitle = row.subtitle.clone();
        workspace.description = row.description.clone();
        workspace.category = row.category;
        core.insert_mirror_workspace(row.index, workspace);
    }
    core.committed_structure_revision = model.applied.revision;
    Ok(())
}

fn pane_tree(model: &JournalModel, tree: &SplitTree<u32>) -> Result<PaneNode> {
    match tree {
        SplitTree::Leaf(id) => {
            let source = model.panes.get(id).ok_or("bootstrap pane missing")?;
            let tabs = source
                .tabs
                .iter()
                .map(|id| {
                    let source = model.tabs.get(id).ok_or("bootstrap tab missing")?;
                    Ok(Tab {
                        id: *id,
                        name: source.name.clone(),
                        explicit_name: source.explicit_name.clone(),
                        layout_opt: Some(surface_tree(model, &source.layout)?),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(PaneNode::Leaf(Pane { id: *id, tabs }))
        }
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => Ok(PaneNode::Split {
            direction: *direction,
            ratio: ratio.to_f32(),
            first: Box::new(pane_tree(model, first)?),
            second: Box::new(pane_tree(model, second)?),
        }),
    }
}

fn surface_tree(model: &JournalModel, tree: &SplitTree<u32>) -> Result<SurfaceLayout> {
    match tree {
        SplitTree::Leaf(id) => {
            let source = model.surfaces.get(id).ok_or("bootstrap surface missing")?;
            Ok(SurfaceLayout::Leaf(tasty_model::SurfaceDescriptor {
                id: *id,
                kind: source.kind.clone(),
                activation_generation: source.activation.map(|activation| activation.generation),
            }))
        }
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => Ok(SurfaceLayout::Split {
            node_id: SplitNodeId::allocate(),
            direction: *direction,
            ratio: ratio.to_f32(),
            first: Box::new(surface_tree(model, first)?),
            second: Box::new(surface_tree(model, second)?),
        }),
    }
}
