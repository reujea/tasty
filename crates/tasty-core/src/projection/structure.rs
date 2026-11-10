use super::*;
use tasty_model::WorkspaceCategory;

pub(super) fn apply_event(
    engine: &mut CoreState,
    event: &DomainEvent,
    retired: &mut Vec<Retired>,
) -> Result<()> {
    match event {
        DomainEvent::StructureReplaced { replacement, .. } => {
            super::replacement::apply(engine, *replacement, retired)?
        }
        DomainEvent::EngineIncarnationStarted { .. } | DomainEvent::EngineRetired { .. } => {}
        // 저장된 원격 행의 화면 상태는 host가 연결 흐름으로 관리한다. 부팅 때만 행을 만든다.
        DomainEvent::RemoteRowSet { .. } | DomainEvent::RemoteRowRemoved { .. } => {}
        DomainEvent::CategoryCreated { id, name, index } => engine
            .categories
            .insert(*index, WorkspaceCategory::new(*id, name.clone())),
        DomainEvent::CategoryRenamed { id, name } => {
            engine
                .categories
                .iter_mut()
                .find(|category| category.id == *id)
                .ok_or("category missing")?
                .name = name.clone()
        }
        DomainEvent::CategoryMoved { id, index } => {
            let old = engine
                .categories
                .iter()
                .position(|category| category.id == *id)
                .ok_or("category missing")?;
            let category = engine.categories.remove(old);
            engine.categories.insert(*index, category);
        }
        DomainEvent::CategoryClosed { id } => {
            engine.categories.retain(|category| category.id != *id);
            // Mirror category is a local display annotation and cannot retain a deleted category.
            for mirror in &mut engine.mirror_workspaces {
                if mirror.category == *id {
                    mirror.category = 0;
                }
            }
        }
        DomainEvent::WorkspaceCreated {
            id,
            name,
            category,
            index,
            pane,
        } => {
            let mut workspace = Workspace::new_with_pane(
                *id,
                name.clone(),
                Pane {
                    id: *pane,
                    tabs: Vec::new(),
                },
            );
            workspace.category = *category;
            let mut order: Vec<_> = engine
                .local_workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect();
            order.insert(*index, *id);
            engine.push_local_workspace(workspace);
            engine.reorder_local_workspaces(&order)?;
        }
        DomainEvent::WorkspaceRenamed { id, name } => workspace(engine, *id)?.name = name.clone(),
        DomainEvent::WorkspaceDetailsSet {
            id,
            subtitle,
            description,
        } => {
            let workspace = workspace(engine, *id)?;
            workspace.subtitle = subtitle.clone();
            workspace.description = description.clone();
        }
        DomainEvent::WorkspaceAttachMappingSet { .. } => {}
        DomainEvent::WorkspaceMoved {
            id,
            category,
            index,
        } => {
            workspace(engine, *id)?.category = *category;
            let mut order: Vec<_> = engine
                .local_workspaces
                .iter()
                .map(|workspace| workspace.id)
                .filter(|old| old != id)
                .collect();
            order.insert(*index, *id);
            engine.reorder_local_workspaces(&order)?;
        }
        DomainEvent::WorkspaceClosed { id } => {
            let index = engine
                .find_workspace_index_for_id(*id)
                .ok_or("workspace missing")?;
            retired.push(Retired::Workspace(engine.remove_workspace_at(index)));
        }
        DomainEvent::TabCreated {
            id,
            pane: pane_id,
            index,
            name,
            surface,
        } => {
            let leaf = SurfaceDescriptor::new(surface.id, surface.kind.clone());
            pane(engine, *pane_id)?
                .tabs
                .insert(*index, Tab::new_with_surface(*id, name.clone(), leaf));
        }
        DomainEvent::TabRenamed { id, name } => tab(engine, *id)?.name = name.clone(),
        DomainEvent::TabExplicitNameSet { id, name } => {
            tab(engine, *id)?.explicit_name = name.clone()
        }
        DomainEvent::TabMoved {
            id,
            pane: target,
            index,
        } => {
            let moved = layout::detach_tab(engine, *id)?;
            pane(engine, *target)?.tabs.insert(*index, moved);
        }
        DomainEvent::TabClosed { id } => {
            retired.push(Retired::Tab(layout::detach_tab(engine, *id)?))
        }
        DomainEvent::SurfaceConverted { id, kind, .. } => {
            let leaf = SurfaceDescriptor::new(*id, kind.clone());
            let target_tab = engine.find_tab_for_surface(*id).ok_or("surface missing")?;
            let slot = tab(engine, target_tab)?
                .layout_mut()
                .find_leaf_mut(*id)
                .ok_or("surface missing")?;
            retired.push(Retired::Surface(std::mem::replace(slot, leaf)));
        }
        DomainEvent::PaneSplit { .. }
        | DomainEvent::PaneMoved { .. }
        | DomainEvent::PaneClosed { .. }
        | DomainEvent::SurfaceSplit { .. }
        | DomainEvent::SurfaceMoved { .. }
        | DomainEvent::SurfaceClosed { .. }
        | DomainEvent::PaneRatioSet { .. }
        | DomainEvent::SurfaceRatioSet { .. } => layout::apply_event(engine, event, retired)?,
        DomainEvent::UndoRecordAdded { .. }
        | DomainEvent::UndoRecordConsumed { .. }
        | DomainEvent::UndoRecordEvicted { .. }
        | DomainEvent::SurfaceCreationSeeded { .. }
        | DomainEvent::SurfaceSeedImported { .. }
        | DomainEvent::SurfaceDataRecorded { .. }
        | DomainEvent::SurfaceActivationChanged { .. }
        | DomainEvent::OperationPrepared { .. }
        | DomainEvent::OperationResourcePrepared { .. }
        | DomainEvent::OperationAwaitingCleanup { .. }
        | DomainEvent::OperationFinished { .. }
        | DomainEvent::OperationRecoveryObserved { .. }
        | DomainEvent::OperationReconciled { .. } => {}
        DomainEvent::MetadataSet { .. } | DomainEvent::MetadataRemoved { .. } => {
            return Err("service metadata has no structural writer".into());
        }
    }
    Ok(())
}
