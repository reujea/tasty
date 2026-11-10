//! Multi-leaf restoration is a single publication. Inputs and identities are fixed before decide.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssemblyDestination {
    Workspace,
    Pane { target: u32, split: SplitSpec },
    Tab { pane: u32, index: usize },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreationAssembly {
    /// Every identity has already been remapped from the capture using durable reservations.
    pub snapshot: ClosedSnapshot,
    pub destination: AssemblyDestination,
    #[serde(deserialize_with = "crate::codec::u32_keyed_map")]
    pub inputs: BTreeMap<u32, DataRef>,
    pub undo: Option<OperationId>,
    /// Existing restore policy can omit unavailable leaves, but never uncertain external effects.
    pub omit_failed: bool,
}
impl CreationAssembly {
    pub fn validate_graph(&self) -> Result<(), Rejection> {
        let bad =
            || Rejection("restore subtree contains inconsistent IDs or parent references".into());
        let snapshot = &self.snapshot;
        if snapshot.version != 1 || snapshot.surfaces.is_empty() {
            return Err(bad());
        }
        let mut seen_surfaces = BTreeSet::new();
        let mut seen_tabs = BTreeSet::new();
        let mut seen_panes = BTreeSet::new();
        for (id, tab) in &snapshot.tabs {
            for surface in tab.layout.leaves() {
                if !seen_surfaces.insert(surface)
                    || snapshot
                        .surfaces
                        .get(&surface)
                        .is_none_or(|leaf| leaf.tab != *id)
                {
                    return Err(bad());
                }
            }
        }
        if seen_surfaces.len() != snapshot.surfaces.len() {
            return Err(bad());
        }
        for (id, pane) in &snapshot.panes {
            if pane.tabs.is_empty() {
                return Err(bad());
            }
            for tab in &pane.tabs {
                if !seen_tabs.insert(*tab)
                    || snapshot.tabs.get(tab).is_none_or(|tab| tab.pane != *id)
                {
                    return Err(bad());
                }
            }
        }
        for (id, workspace) in &snapshot.workspaces {
            for pane in workspace.layout.leaves() {
                if !seen_panes.insert(pane)
                    || snapshot
                        .panes
                        .get(&pane)
                        .is_none_or(|pane| pane.workspace != *id)
                {
                    return Err(bad());
                }
            }
        }
        let valid = match self.destination {
            AssemblyDestination::Workspace => {
                snapshot.root.kind == IdKind::Workspace
                    && snapshot.workspaces.len() == 1
                    && snapshot.workspaces.contains_key(&snapshot.root.id)
                    && seen_panes.len() == snapshot.panes.len()
                    && seen_tabs.len() == snapshot.tabs.len()
            }
            AssemblyDestination::Pane { .. } => {
                snapshot.root.kind == IdKind::Pane
                    && snapshot.workspaces.is_empty()
                    && snapshot.panes.len() == 1
                    && snapshot.panes.contains_key(&snapshot.root.id)
                    && seen_tabs.len() == snapshot.tabs.len()
            }
            AssemblyDestination::Tab { .. } => {
                snapshot.root.kind == IdKind::Tab
                    && snapshot.workspaces.is_empty()
                    && snapshot.panes.is_empty()
                    && snapshot.tabs.len() == 1
                    && snapshot.tabs.contains_key(&snapshot.root.id)
            }
        };
        if !valid || self.reserved_ids().iter().any(|entity| entity.id == 0) {
            return Err(bad());
        }
        Ok(())
    }
    pub fn member(group: &OperationId, surface: u32) -> OperationId {
        OperationId(format!("{}/leaf/{surface}", group.0))
    }
    pub fn target_is_live(&self, model: &JournalModel) -> bool {
        !model.engine_retired
            && match self.destination {
                AssemblyDestination::Workspace => true,
                AssemblyDestination::Pane { target, .. } => model.panes.contains_key(&target),
                AssemblyDestination::Tab { pane, index } => model
                    .panes
                    .get(&pane)
                    .is_some_and(|pane| index <= pane.tabs.len()),
            }
            && self
                .undo
                .as_ref()
                .is_none_or(|id| model.undo_records.iter().any(|record| record.id == *id))
    }
    pub fn reserved_ids(&self) -> Vec<EntityId> {
        [
            (
                IdKind::Workspace,
                self.snapshot.workspaces.keys().copied().collect::<Vec<_>>(),
            ),
            (IdKind::Pane, self.snapshot.panes.keys().copied().collect()),
            (IdKind::Tab, self.snapshot.tabs.keys().copied().collect()),
            (
                IdKind::Surface,
                self.snapshot.surfaces.keys().copied().collect(),
            ),
        ]
        .into_iter()
        .flat_map(|(kind, ids)| ids.into_iter().map(move |id| EntityId { kind, id }))
        .collect()
    }
    pub fn data_refs(&self) -> Vec<DataRef> {
        self.snapshot
            .data_refs()
            .into_iter()
            .chain(self.inputs.values().copied())
            .collect()
    }
    pub fn result(&self, surviving: &BTreeSet<u32>) -> StructuralResult {
        let surface = surviving.iter().next().copied().unwrap_or(0);
        let tab = self
            .snapshot
            .surfaces
            .get(&surface)
            .map(|surface| surface.tab);
        let pane = tab
            .and_then(|tab| self.snapshot.tabs.get(&tab))
            .map(|tab| tab.pane);
        let workspace = pane
            .and_then(|pane| self.snapshot.panes.get(&pane))
            .map(|pane| pane.workspace);
        StructuralResult::Created {
            workspace,
            pane,
            tab,
            surface,
        }
    }
    /// Compile a fixed subtree into ordinary structure facts. Existing owners outside this subtree
    /// are never rebuilt. Missing-kind omission collapses splits and removes empty containers.
    pub fn facts(
        &self,
        model: &JournalModel,
        surviving: &BTreeSet<u32>,
    ) -> Result<Vec<DomainEvent>, Rejection> {
        self.validate_graph()?;
        if !self.target_is_live(model) {
            return Err(Rejection("assembly target was retired".into()));
        }
        let mut tabs = BTreeMap::new();
        for (id, tab) in &self.snapshot.tabs {
            if let Some(layout) = prune(&tab.layout, surviving) {
                let mut tab = tab.clone();
                tab.layout = layout;
                tabs.insert(*id, tab);
            }
        }
        let mut panes = BTreeMap::new();
        for (id, pane) in &self.snapshot.panes {
            let mut pane = pane.clone();
            pane.tabs.retain(|id| tabs.contains_key(id));
            if !pane.tabs.is_empty() {
                panes.insert(*id, pane);
            }
        }
        let pane_ids = panes.keys().copied().collect();
        let mut events = Vec::new();
        match &self.destination {
            AssemblyDestination::Workspace => {
                let workspace = self
                    .snapshot
                    .workspaces
                    .get(&self.snapshot.root.id)
                    .ok_or_else(|| Rejection("assembly workspace root missing".into()))?;
                let Some(layout) = prune(&workspace.layout, &pane_ids) else {
                    return Ok(events);
                };
                let id = self.snapshot.root.id;
                events.push(DomainEvent::WorkspaceCreated {
                    id,
                    name: workspace.name.clone(),
                    category: if model.categories.contains_key(&workspace.category) {
                        workspace.category
                    } else {
                        0
                    },
                    index: model.workspace_order.len(),
                    pane: first(&layout),
                });
                events.push(DomainEvent::WorkspaceDetailsSet {
                    id,
                    subtitle: workspace.subtitle.clone(),
                    description: workspace.description.clone(),
                });
                splits(&layout, &mut |target, pane, split| {
                    events.push(DomainEvent::PaneSplit {
                        target,
                        pane,
                        split,
                    })
                });
                for (key, value) in &workspace.metadata {
                    events.push(DomainEvent::MetadataSet {
                        target: crate::event::MetadataTarget::Workspace(id),
                        key: key.clone(),
                        value: value.clone(),
                    });
                }
            }
            AssemblyDestination::Pane { target, split } => {
                if panes.is_empty() {
                    return Ok(events);
                }
                events.push(DomainEvent::PaneSplit {
                    target: *target,
                    pane: self.snapshot.root.id,
                    split: *split,
                });
            }
            AssemblyDestination::Tab { pane, .. } => {
                if let Some(tab) = tabs.get_mut(&self.snapshot.root.id) {
                    tab.pane = *pane;
                } else {
                    return Ok(events);
                }
                // The destination owns the order; the old capture's pane must not be recreated.
                panes.clear();
                panes.insert(
                    *pane,
                    Pane {
                        workspace: model.panes[pane].workspace,
                        tabs: vec![self.snapshot.root.id],
                    },
                );
            }
        }
        for (pane_id, pane) in &panes {
            for (index, tab_id) in pane.tabs.iter().enumerate() {
                let tab = &tabs[tab_id];
                let surface = first(&tab.layout);
                let spec = |id| {
                    let value = &self.snapshot.surfaces[&id];
                    SurfaceSpec {
                        id,
                        kind: value.kind.clone(),
                        data: value.data,
                    }
                };
                let index = match self.destination {
                    AssemblyDestination::Tab { index, .. } => index,
                    _ => index,
                };
                events.push(DomainEvent::TabCreated {
                    id: *tab_id,
                    pane: *pane_id,
                    index,
                    name: tab.name.clone(),
                    surface: spec(surface),
                });
                events.push(DomainEvent::TabExplicitNameSet {
                    id: *tab_id,
                    name: tab.explicit_name.clone(),
                });
                splits(&tab.layout, &mut |target, surface, split| {
                    events.push(DomainEvent::SurfaceSplit {
                        target,
                        surface: spec(surface),
                        split,
                    })
                });
                for id in tab.layout.leaves() {
                    for (key, value) in &self.snapshot.surfaces[&id].metadata {
                        events.push(DomainEvent::MetadataSet {
                            target: crate::event::MetadataTarget::Surface(id),
                            key: key.clone(),
                            value: value.clone(),
                        });
                    }
                }
            }
        }
        Ok(events)
    }
}
fn first(tree: &SplitTree<u32>) -> u32 {
    match tree {
        SplitTree::Leaf(id) => *id,
        SplitTree::Split { first: child, .. } => first(child),
    }
}
fn splits(tree: &SplitTree<u32>, emit: &mut impl FnMut(u32, u32, SplitSpec)) {
    if let SplitTree::Split {
        direction,
        ratio,
        first: one,
        second: two,
    } = tree
    {
        emit(
            first(one),
            first(two),
            SplitSpec {
                direction: *direction,
                ratio: *ratio,
                placement: Placement::After,
            },
        );
        splits(one, emit);
        splits(two, emit);
    }
}
fn prune(tree: &SplitTree<u32>, keep: &BTreeSet<u32>) -> Option<SplitTree<u32>> {
    match tree {
        SplitTree::Leaf(id) => keep.contains(id).then_some(SplitTree::Leaf(*id)),
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => match (prune(first, keep), prune(second, keep)) {
            (Some(first), Some(second)) => Some(SplitTree::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (one, two) => one.or(two),
        },
    }
}
