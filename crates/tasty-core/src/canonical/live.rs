//! Read live structure without capture, storage, or resource creation.
use super::{
    CanonCategory, CanonData, CanonPane, CanonSurface, CanonTab, CanonTree, CanonWorkspace,
    Canonical, IdMode, direction_name,
};
use crate::CoreState;
use tasty_model::{PaneNode, SurfaceDescriptor, SurfaceLayout};

/// CoreState의 정규 표현. ID는 원래 값이고 surface 저장 자료는 비교하지 않는다.
/// mirror workspace와 선택·파생 값은 뺀다.
pub fn core_canonical(engine: &CoreState) -> Canonical {
    let mut defects = Vec::new();
    let categories = engine
        .categories
        .iter()
        .map(|c| CanonCategory {
            id: c.id,
            name: c.name.clone(),
        })
        .collect();
    let workspaces = engine
        .local_workspaces
        .iter()
        .map(|ws| {
            let mut leaves = Vec::new();
            pane_leaves(ws.pane_layout(), &mut leaves);
            let panes = leaves
                .into_iter()
                .map(|pane| CanonPane {
                    id: pane.id,
                    tabs: pane
                        .tabs
                        .iter()
                        .map(|tab| {
                            let mut surfaces = Vec::new();
                            surface_leaves(tab.layout(), &mut surfaces);
                            CanonTab {
                                id: tab.id,
                                name: tab.name.clone(),
                                explicit_name: tab.explicit_name.clone(),
                                layout: surface_tree(tab.layout(), &mut defects),
                                surfaces: surfaces
                                    .into_iter()
                                    .map(|s| CanonSurface {
                                        id: surface_id(s, &mut defects),
                                        kind: core_kind(s),
                                        data: CanonData::NotCompared,
                                        metadata: Default::default(),
                                    })
                                    .collect(),
                            }
                        })
                        .collect(),
                })
                .collect();
            CanonWorkspace {
                id: ws.id,
                name: ws.name.clone(),
                category: ws.category,
                subtitle: ws.subtitle.clone(),
                description: ws.description.clone(),
                metadata: Default::default(),
                layout: pane_tree(ws.pane_layout()),
                panes,
            }
        })
        .collect();
    Canonical {
        ids: IdMode::Exact,
        categories,
        workspaces,
        defects,
    }
}

/// 저장 형식이 정하는 kind. 대기 중인 terminal은 terminal, plugin 대기는 기다리는 kind다.
pub fn core_kind(surface: &SurfaceDescriptor) -> String {
    surface.kind.clone()
}

fn surface_id(surface: &SurfaceDescriptor, defects: &mut Vec<String>) -> u32 {
    surface.surface_id().unwrap_or_else(|| {
        defects.push(format!("a {} surface has no id", surface.kind()));
        u32::MAX
    })
}

fn pane_leaves<'a>(node: &'a PaneNode, out: &mut Vec<&'a tasty_model::Pane>) {
    match node {
        PaneNode::Leaf(pane) => out.push(pane),
        PaneNode::Split { first, second, .. } => {
            pane_leaves(first, out);
            pane_leaves(second, out);
        }
    }
}

fn surface_leaves<'a>(node: &'a SurfaceLayout, out: &mut Vec<&'a SurfaceDescriptor>) {
    match node {
        SurfaceLayout::Leaf(surface) => out.push(surface),
        SurfaceLayout::Split { first, second, .. } => {
            surface_leaves(first, out);
            surface_leaves(second, out);
        }
    }
}

fn pane_tree(node: &PaneNode) -> CanonTree {
    match node {
        PaneNode::Leaf(pane) => CanonTree::Leaf(pane.id),
        PaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } => CanonTree::Split {
            direction: direction_name(*direction),
            ratio: ratio.to_bits(),
            first: Box::new(pane_tree(first)),
            second: Box::new(pane_tree(second)),
        },
    }
}

/// `focus_second`는 포커스 hint라 담지 않는다.
fn surface_tree(node: &SurfaceLayout, defects: &mut Vec<String>) -> CanonTree {
    match node {
        SurfaceLayout::Leaf(surface) => CanonTree::Leaf(surface_id(surface, defects)),
        SurfaceLayout::Split {
            direction,
            ratio,
            first,
            second,
            ..
        } => CanonTree::Split {
            direction: direction_name(*direction),
            ratio: ratio.to_bits(),
            first: Box::new(surface_tree(first, defects)),
            second: Box::new(surface_tree(second, defects)),
        },
    }
}
