//! Execution obligations derived from a committed local batch, never polled from a View baseline.
use super::engine_access::EngineMut;
impl EngineMut<'_> {
    /// Project transport obligations only after the entire structural batch has been installed.
    /// Sending remains in the ordinary observation pump, behind its publication/result barrier.
    pub(crate) fn observe_committed_structure(
        &mut self,
        before: &tasty_core::JournalModel,
        events: &[tasty_core::RecordedEvent],
    ) {
        use tasty_core::DomainEvent as E;
        let structural = events.iter().any(|record| {
            matches!(
                record.event,
                E::StructureReplaced { .. }
                    | E::WorkspaceCreated { .. }
                    | E::WorkspaceRenamed { .. }
                    | E::WorkspaceDetailsSet { .. }
                    | E::WorkspaceMoved { .. }
                    | E::WorkspaceClosed { .. }
                    | E::PaneSplit { .. }
                    | E::PaneMoved { .. }
                    | E::PaneClosed { .. }
                    | E::TabCreated { .. }
                    | E::TabRenamed { .. }
                    | E::TabExplicitNameSet { .. }
                    | E::TabMoved { .. }
                    | E::TabClosed { .. }
                    | E::SurfaceSplit { .. }
                    | E::SurfaceMoved { .. }
                    | E::SurfaceClosed { .. }
                    | E::SurfaceConverted { .. }
                    | E::SurfaceActivationChanged { .. }
                    | E::PaneRatioSet { .. }
                    | E::SurfaceRatioSet { .. }
            )
        });
        if !structural {
            return;
        }
        for workspace in &before.workspace_order {
            if !self
                .core
                .local_workspaces()
                .iter()
                .any(|current| current.id == *workspace)
            {
                // The registry retains stop ownership even after this workspace leaves the tree.
                let _ = self.task_scope.request_stop_workspace(*workspace);
            }
        }
        for (tab, old) in &before.tabs {
            if let Some(pane) = self.core.find_pane_for_tab(*tab)
                && pane != old.pane
            {
                self.runtime.pending_host_events.push(
                    crate::core::host_event::PendingHostEvent::TabMoved {
                        tab_id: *tab,
                        from_pane: old.pane,
                        to_pane: pane,
                    },
                );
            }
        }
        let workspaces = affected_workspaces(before, self.core, events);
        for workspace in workspaces {
            if self.live.occupancy.workspace_holder(workspace).is_some() {
                self.remote.mark_structure_changed(workspace);
            }
        }
        for record in events {
            let E::SurfaceActivationChanged { id, activation, .. } = &record.event else {
                continue;
            };
            if activation.phase != tasty_core::ActivationPhase::Ready {
                continue;
            }
            let Some(workspace) = self
                .find_workspace_index_for_surface(*id)
                .and_then(|(index, _)| self.workspace_at(index))
                .map(|workspace| workspace.id)
            else {
                continue;
            };
            if self.live.occupancy.workspace_holder(workspace).is_none() {
                continue;
            }
            let generation = self.runtime.terminals.generation(*id);
            self.live
                .occupancy
                .add_workspace_member(workspace, *id, generation.is_some());
            if let Some(generation) = generation {
                self.remote
                    .pending_workspace_taps
                    .insert(*id, (workspace, generation));
            }
        }
    }
}

impl EngineMut<'_> {
    /// Remote replacement preserves IDs when the server keeps a tab. Initial snapshot insertion
    /// has no predecessor and therefore emits no historical movement event.
    #[cfg(feature = "gui")]
    pub(crate) fn replace_mirror_workspace(
        &mut self,
        workspace: crate::model::Workspace,
    ) -> Result<(), crate::model::Workspace> {
        let old = self
            .core
            .mirror_workspaces()
            .iter()
            .find(|old| old.id == workspace.id)
            .map(|old| {
                old.pane_layout()
                    .all_pane_ids()
                    .into_iter()
                    .filter_map(|pane| old.pane_layout().find_pane(pane))
                    .flat_map(|pane| pane.tabs.iter().map(move |tab| (tab.id, pane.id)))
                    .collect::<std::collections::HashMap<_, _>>()
            })
            .unwrap_or_default();
        let moved: Vec<_> = workspace
            .pane_layout()
            .all_pane_ids()
            .into_iter()
            .filter_map(|pane| workspace.pane_layout().find_pane(pane))
            .flat_map(|pane| {
                pane.tabs.iter().filter_map(|tab| {
                    old.get(&tab.id)
                        .filter(|from| **from != pane.id)
                        .map(|from| crate::core::host_event::PendingHostEvent::TabMoved {
                            tab_id: tab.id,
                            from_pane: *from,
                            to_pane: pane.id,
                        })
                })
            })
            .collect();
        self.core.replace_mirror_workspace(workspace)?;
        self.runtime.pending_host_events.extend(moved);
        Ok(())
    }
}

/// Resolve both sides of a move or deletion without rebuilding every holder's wire tree.
fn affected_workspaces(
    before: &tasty_core::JournalModel,
    after: &tasty_core::CoreState,
    events: &[tasty_core::RecordedEvent],
) -> std::collections::BTreeSet<u32> {
    use tasty_core::{DomainEvent as E, EntityId, IdKind as K};
    let mut entities = Vec::new();
    for record in events {
        let mut add = |kind, id| entities.push(EntityId { kind, id });
        match &record.event {
            E::WorkspaceCreated { id, .. }
            | E::WorkspaceRenamed { id, .. }
            | E::WorkspaceDetailsSet { id, .. }
            | E::WorkspaceMoved { id, .. }
            | E::WorkspaceClosed { id } => add(K::Workspace, *id),
            E::PaneRatioSet { workspace, .. } => add(K::Workspace, *workspace),
            E::PaneSplit { target, pane, .. } => {
                add(K::Pane, *target);
                add(K::Pane, *pane);
            }
            E::PaneMoved { id, target, .. } => {
                add(K::Pane, *id);
                add(K::Pane, *target);
            }
            E::PaneClosed { id } => add(K::Pane, *id),
            E::TabCreated { id, pane, .. } | E::TabMoved { id, pane, .. } => {
                add(K::Tab, *id);
                add(K::Pane, *pane);
            }
            E::TabRenamed { id, .. } | E::TabExplicitNameSet { id, .. } | E::TabClosed { id } => {
                add(K::Tab, *id)
            }
            E::SurfaceRatioSet { tab, .. } => add(K::Tab, *tab),
            E::SurfaceSplit {
                target, surface, ..
            } => {
                add(K::Surface, *target);
                add(K::Surface, surface.id);
            }
            E::SurfaceMoved { id, target, .. } => {
                add(K::Surface, *id);
                add(K::Surface, *target);
            }
            E::SurfaceClosed { id }
            | E::SurfaceConverted { id, .. }
            | E::SurfaceActivationChanged { id, .. } => add(K::Surface, *id),
            E::StructureReplaced {
                replacement,
                removed,
            } => {
                entities.extend([replacement.source, replacement.target]);
                entities.extend(removed.iter().copied());
            }
            E::UndoRecordAdded { .. }
            | E::UndoRecordConsumed { .. }
            | E::UndoRecordEvicted { .. }
            | E::EngineIncarnationStarted { .. }
            | E::EngineRetired { .. }
            | E::CategoryCreated { .. }
            | E::CategoryRenamed { .. }
            | E::CategoryMoved { .. }
            | E::CategoryClosed { .. }
            | E::WorkspaceAttachMappingSet { .. }
            | E::RemoteRowSet { .. }
            | E::RemoteRowRemoved { .. }
            | E::SurfaceCreationSeeded { .. }
            | E::SurfaceSeedImported { .. }
            | E::SurfaceDataRecorded { .. }
            | E::OperationPrepared { .. }
            | E::OperationResourcePrepared { .. }
            | E::OperationAwaitingCleanup { .. }
            | E::OperationFinished { .. }
            | E::OperationRecoveryObserved { .. }
            | E::OperationReconciled { .. }
            | E::MetadataSet { .. }
            | E::MetadataRemoved { .. } => {}
        }
    }
    let old_pane = |id| before.panes.get(&id).map(|pane| pane.workspace);
    let old_tab = |id| before.tabs.get(&id).and_then(|tab| old_pane(tab.pane));
    let new_pane = |id| {
        after
            .find_workspace_index_for_pane(id)
            .and_then(|index| after.workspace_at(index))
            .map(|workspace| workspace.id)
    };
    let mut affected = std::collections::BTreeSet::new();
    for entity in entities {
        let (old, new) = match entity.kind {
            K::Workspace => (Some(entity.id), Some(entity.id)),
            K::Pane => (old_pane(entity.id), new_pane(entity.id)),
            K::Tab => (
                old_tab(entity.id),
                after.find_pane_for_tab(entity.id).and_then(new_pane),
            ),
            K::Surface => (
                before
                    .surfaces
                    .get(&entity.id)
                    .and_then(|surface| old_tab(surface.tab)),
                after
                    .find_workspace_index_for_surface(entity.id)
                    .and_then(|(index, _)| after.workspace_at(index))
                    .map(|workspace| workspace.id),
            ),
            K::Category => (None, None),
        };
        affected.extend(old.into_iter().chain(new));
    }
    affected
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_core::{DomainEvent as E, JournalModel};
    fn scopes(events: Vec<E>) -> std::collections::BTreeSet<u32> {
        let mut setup = vec![E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        }];
        for id in 1..=3 {
            setup.push(E::WorkspaceCreated {
                id,
                name: format!("{id}"),
                category: 0,
                index: (id - 1) as usize,
                pane: id,
            });
            setup.push(E::TabCreated {
                id,
                pane: id,
                index: 0,
                name: "tab".into(),
                surface: tasty_core::SurfaceSpec {
                    id,
                    kind: "empty".into(),
                    data: None,
                },
            });
        }
        setup.push(E::TabCreated {
            id: 11,
            pane: 1,
            index: 1,
            name: "remaining".into(),
            surface: tasty_core::SurfaceSpec {
                id: 11,
                kind: "empty".into(),
                data: None,
            },
        });
        let before = crate::state::tests::test_model(setup);
        let records: Vec<_> = events
            .into_iter()
            .enumerate()
            .map(|(i, event)| tasty_core::RecordedEvent {
                revision: before.applied.revision.unwrap() + i as u64 + 1,
                event,
            })
            .collect();
        let mut model: JournalModel = before.clone();
        tasty_core::evolve(
            &mut model,
            &tasty_core::DomainBatch {
                batch_id: 2,
                events: records.clone(),
            },
        )
        .unwrap();
        let mut after = tasty_core::CoreState::new_base();
        tasty_core::projection::bootstrap::initialize(&mut after, &model).unwrap();
        affected_workspaces(&before, &after, &records)
    }
    #[test]
    fn rename_does_not_dirty_unrelated_held_workspaces() {
        assert_eq!(
            scopes(vec![E::TabRenamed {
                id: 1,
                name: "new".into()
            }]),
            [1].into()
        );
    }
    #[test]
    fn move_marks_both_workspaces_and_close_keeps_the_old_owner() {
        assert_eq!(
            scopes(vec![E::TabMoved {
                id: 1,
                pane: 2,
                index: 1
            }]),
            [1, 2].into()
        );
        assert_eq!(scopes(vec![E::WorkspaceClosed { id: 1 }]), [1].into());
    }
}
