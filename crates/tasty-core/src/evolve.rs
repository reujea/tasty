//! pure evolve. batch 하나를 모델 사본에 모두 적용한 뒤에만 교체하므로 일부만 반영되지 않는다.
//!
//! 이벤트는 decide가 확정한 사실이다. 참조가 맞지 않으면 추측해 고치지 않고 오류로 중단한다.
//! 입력은 저장 batch가 아니라 해석을 마친 [`DomainBatch`]다.

mod lifecycle;

use std::collections::BTreeMap;

use tasty_model::{PaneId, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId};

use crate::event::{DomainBatch, DomainEvent, MetadataTarget, SplitSpec, SurfaceSpec};
use crate::ids::{BatchId, IdKind, Revision};
use crate::model::{Category, JournalModel, Pane, RemoveLeaf, SplitTree, Surface, Tab, Workspace};

#[derive(Debug, thiserror::Error)]
pub enum EvolveError {
    #[error("batch {got} is not after the last applied batch {last}")]
    StaleBatch { last: BatchId, got: BatchId },

    #[error("structure revision {got} does not follow {expected}")]
    RevisionGap { expected: Revision, got: Revision },

    #[error("{0} does not exist")]
    Missing(String),

    #[error("{0} already exists")]
    Duplicate(String),

    #[error("index {index} is out of range for {len} items")]
    IndexOutOfRange { index: usize, len: usize },

    #[error("{0} is the last node of its layout")]
    LastLeaf(String),

    #[error("category:{0} still has workspaces")]
    CategoryNotEmpty(WorkspaceCategoryId),

    #[error("{0} cannot be moved next to itself")]
    SelfTarget(String),

    #[error("metadata key {key} is not set on {target}")]
    MissingKey { target: String, key: String },

    #[error("invalid committed fact: {0}")]
    InvalidFact(String),
}

type Result<T> = std::result::Result<T, EvolveError>;

/// batch의 구조 이벤트를 순서대로 적용한다. 오류이면 모델은 호출 전 그대로다.
pub fn evolve(model: &mut JournalModel, batch: &DomainBatch) -> Result<()> {
    check_batch(model, batch.batch_id)?;
    if batch.events.is_empty() {
        model.applied.batch = Some(batch.batch_id);
        return Ok(());
    }
    *model = evolve_owned(model.clone(), batch)?;
    Ok(())
}

pub(crate) fn check_batch(model: &JournalModel, batch_id: BatchId) -> Result<()> {
    if let Some(last) = model.applied.batch
        && batch_id <= last
    {
        return Err(EvolveError::StaleBatch {
            last,
            got: batch_id,
        });
    }
    Ok(())
}

/// The caller owns a private candidate; a failed candidate is discarded without another clone.
pub(crate) fn evolve_owned(mut next: JournalModel, batch: &DomainBatch) -> Result<JournalModel> {
    check_batch(&next, batch.batch_id)?;
    for recorded in &batch.events {
        let expected = next.applied.revision.unwrap_or(0) + 1;
        if recorded.revision != expected {
            return Err(EvolveError::RevisionGap {
                expected,
                got: recorded.revision,
            });
        }
        apply(&mut next, recorded.event.clone())?;
        next.applied.revision = Some(recorded.revision);
    }
    next.applied.batch = Some(batch.batch_id);
    Ok(next)
}

fn apply(m: &mut JournalModel, event: DomainEvent) -> Result<()> {
    match event {
        DomainEvent::EngineIncarnationStarted { previous, current } => {
            if m.engine_incarnation != previous || previous.checked_add(1) != Some(current) {
                return Err(EvolveError::InvalidFact(
                    "engine incarnation did not advance from its predecessor".into(),
                ));
            }
            m.engine_incarnation = current;
            m.engine_retired = false;
            Ok(())
        }
        DomainEvent::EngineRetired { incarnation } => {
            if m.engine_incarnation != incarnation || incarnation == 0 {
                return Err(EvolveError::InvalidFact(
                    "retirement belongs to another engine incarnation".into(),
                ));
            }
            m.engine_retired = true;
            Ok(())
        }
        DomainEvent::StructureReplaced {
            replacement,
            removed,
        } => {
            let (after, actual) = replacement
                .simulate(m)
                .map_err(|error| EvolveError::InvalidFact(error.to_string()))?;
            if actual != removed {
                return Err(EvolveError::InvalidFact(
                    "replacement removal set differs from its fact".into(),
                ));
            }
            *m = after;
            Ok(())
        }
        DomainEvent::UndoRecordAdded { record } => {
            let owns = m
                .operations
                .get(&record.id)
                .and_then(|operation| operation.retirement.as_ref())
                .is_some_and(|plan| {
                    (plan.is_user_close || plan.remote_user_close)
                        && plan.target == record.target
                        && plan.undo.as_ref() == Some(&record.capture)
                });
            if !owns
                || m.undo_records.iter().any(|old| old.id == record.id)
                || record.capture.snapshot.0 == 0
                || m.undo_records.len() >= 10
            {
                return Err(EvolveError::InvalidFact("invalid undo record".into()));
            }
            m.undo_records.push(record);
            Ok(())
        }
        DomainEvent::UndoRecordConsumed { id } | DomainEvent::UndoRecordEvicted { id } => {
            let index = m
                .undo_records
                .iter()
                .position(|record| record.id == id)
                .ok_or_else(|| EvolveError::Missing(format!("undo:{}", id.0)))?;
            m.undo_records.remove(index);
            Ok(())
        }
        DomainEvent::CategoryCreated { id, name, index } => create_category(m, id, name, index),
        DomainEvent::CategoryRenamed { id, name } => {
            get_mut(&mut m.categories, IdKind::Category, id)?.name = name;
            Ok(())
        }
        DomainEvent::CategoryMoved { id, index } => {
            move_in(&mut m.category_order, IdKind::Category, id, index)
        }
        DomainEvent::CategoryClosed { id } => close_category(m, id),
        DomainEvent::WorkspaceCreated {
            id,
            name,
            category,
            index,
            pane,
        } => create_workspace(m, id, name, category, index, pane),
        DomainEvent::WorkspaceRenamed { id, name } => {
            get_mut(&mut m.workspaces, IdKind::Workspace, id)?.name = name;
            Ok(())
        }
        DomainEvent::WorkspaceDetailsSet {
            id,
            subtitle,
            description,
        } => {
            let workspace = get_mut(&mut m.workspaces, IdKind::Workspace, id)?;
            workspace.subtitle = subtitle;
            workspace.description = description;
            Ok(())
        }
        // 옛 매핑 기록은 읽고 버린다.
        DomainEvent::WorkspaceAttachMappingSet { .. } => Ok(()),
        DomainEvent::RemoteRowSet { id, row } => {
            if m.workspaces.contains_key(&id) {
                return Err(EvolveError::Duplicate(format!("workspace:{id}")));
            }
            m.remote_rows.insert(id, row);
            Ok(())
        }
        DomainEvent::RemoteRowRemoved { id } => {
            remove(&mut m.remote_rows, IdKind::Workspace, id)?;
            Ok(())
        }
        DomainEvent::WorkspaceMoved {
            id,
            category,
            index,
        } => move_workspace(m, id, category, index),
        DomainEvent::WorkspaceClosed { id } => close_workspace(m, id),
        DomainEvent::PaneSplit {
            target,
            pane,
            split,
        } => split_pane(m, target, pane, split),
        DomainEvent::PaneMoved { id, target, split } => move_pane(m, id, target, split),
        DomainEvent::PaneClosed { id } => close_pane(m, id),
        DomainEvent::TabCreated {
            id,
            pane,
            index,
            name,
            surface,
        } => create_tab(m, id, pane, index, name, surface),
        DomainEvent::TabRenamed { id, name } => {
            get_mut(&mut m.tabs, IdKind::Tab, id)?.name = name;
            Ok(())
        }
        DomainEvent::TabExplicitNameSet { id, name } => {
            get_mut(&mut m.tabs, IdKind::Tab, id)?.explicit_name = name;
            Ok(())
        }
        DomainEvent::TabMoved { id, pane, index } => move_tab(m, id, pane, index),
        DomainEvent::TabClosed { id } => close_tab(m, id),
        DomainEvent::SurfaceSplit {
            target,
            surface,
            split,
        } => split_surface(m, target, surface, split),
        DomainEvent::SurfaceMoved { id, target, split } => move_surface(m, id, target, split),
        DomainEvent::SurfaceClosed { id } => close_surface(m, id),
        DomainEvent::SurfaceConverted { id, kind, data } => {
            let surface = get_mut(&mut m.surfaces, IdKind::Surface, id)?;
            surface.kind = kind;
            surface.data = data;
            surface.content_generation = 0;
            surface.snapshot_schema = 0;
            Ok(())
        }
        DomainEvent::SurfaceCreationSeeded {
            id,
            activation_generation,
            input,
        } => {
            let surface = get_mut(&mut m.surfaces, IdKind::Surface, id)?;
            if input.0 == 0
                || surface.activation.map(|activation| activation.generation)
                    != Some(activation_generation)
            {
                return Err(EvolveError::InvalidFact(
                    "creation seed belongs to an earlier activation".into(),
                ));
            }
            surface.creation_seed = Some(input);
            Ok(())
        }
        DomainEvent::SurfaceSeedImported { id, input } => {
            let surface = get_mut(&mut m.surfaces, IdKind::Surface, id)?;
            if input.0 == 0 || surface.activation.is_some() || surface.creation_seed.is_some() {
                return Err(EvolveError::InvalidFact(
                    "import seed requires an inactive surface without a seed".into(),
                ));
            }
            surface.creation_seed = Some(input);
            Ok(())
        }
        DomainEvent::SurfaceDataRecorded {
            id,
            activation_generation,
            content_generation,
            snapshot_schema,
            data,
        } => {
            let surface = get_mut(&mut m.surfaces, IdKind::Surface, id)?;
            if surface.activation.map(|a| a.generation) != activation_generation
                || content_generation <= surface.content_generation
                || snapshot_schema == 0
            {
                return Err(EvolveError::InvalidFact(
                    "stale surface content generation".into(),
                ));
            }
            surface.data = Some(data);
            surface.content_generation = content_generation;
            surface.snapshot_schema = snapshot_schema;
            Ok(())
        }
        DomainEvent::SurfaceActivationChanged {
            id,
            previous_generation,
            activation,
        } => lifecycle::activation(m, id, previous_generation, activation),
        DomainEvent::PaneRatioSet {
            workspace,
            path,
            ratio,
        } => {
            let layout = &mut get_mut(&mut m.workspaces, IdKind::Workspace, workspace)?.layout;
            lifecycle::ratio(layout, &path, ratio)
        }
        DomainEvent::SurfaceRatioSet { tab, path, ratio } => {
            let layout = &mut get_mut(&mut m.tabs, IdKind::Tab, tab)?.layout;
            lifecycle::ratio(layout, &path, ratio)
        }
        DomainEvent::OperationPrepared { operation } => lifecycle::prepare(m, *operation),
        DomainEvent::OperationResourcePrepared { id, data, deferred } => {
            lifecycle::prepared(m, id, data, deferred)
        }
        DomainEvent::OperationAwaitingCleanup {
            id,
            outcome,
            cleanup,
            prepared_data,
            deferred,
        } => lifecycle::await_cleanup(m, id, outcome, cleanup, prepared_data, deferred),
        DomainEvent::OperationFinished { id, outcome } => lifecycle::finish(m, id, outcome, None),
        DomainEvent::OperationRecoveryObserved { id, evidence } => {
            let operation = m
                .operations
                .get_mut(&id)
                .ok_or_else(|| EvolveError::Missing(format!("operation:{}", id.0)))?;
            if evidence.0 == 0
                || operation.reconciliation_evidence.is_some()
                || !matches!(
                    operation.outcome,
                    Some(
                        crate::OperationOutcome::Cancelled { .. }
                            | crate::OperationOutcome::Uncertain { .. }
                            | crate::OperationOutcome::Superseded { .. }
                    )
                )
            {
                return Err(EvolveError::InvalidFact(
                    "invalid recovery observation".into(),
                ));
            }
            operation.reconciliation_evidence = Some(evidence);
            Ok(())
        }
        DomainEvent::OperationReconciled {
            id,
            outcome,
            evidence,
        } => lifecycle::finish(m, id, outcome, Some(evidence)),
        DomainEvent::MetadataSet { target, key, value } => {
            metadata_mut(m, target)?.insert(key, value);
            Ok(())
        }
        DomainEvent::MetadataRemoved { target, key } => {
            match metadata_mut(m, target)?.remove(&key) {
                Some(_) => Ok(()),
                None => Err(EvolveError::MissingKey {
                    target: target_name(target),
                    key,
                }),
            }
        }
    }
}

fn create_category(
    m: &mut JournalModel,
    id: WorkspaceCategoryId,
    name: String,
    index: usize,
) -> Result<()> {
    ensure_absent(&m.categories, IdKind::Category, id)?;
    insert_at(&mut m.category_order, index, id)?;
    m.categories.insert(id, Category { name });
    Ok(())
}

fn close_category(m: &mut JournalModel, id: WorkspaceCategoryId) -> Result<()> {
    get(&m.categories, IdKind::Category, id)?;
    if m.workspaces.values().any(|w| w.category == id) {
        return Err(EvolveError::CategoryNotEmpty(id));
    }
    m.categories.remove(&id);
    m.category_order.retain(|c| *c != id);
    // 원격 행은 카테고리 닫기를 막지 않고 기본 카테고리로 돌아간다.
    for row in m.remote_rows.values_mut().filter(|row| row.category == id) {
        row.category = tasty_model::NORMAL_CATEGORY_ID;
    }
    Ok(())
}

fn create_workspace(
    m: &mut JournalModel,
    id: WorkspaceId,
    name: String,
    category: WorkspaceCategoryId,
    index: usize,
    pane: PaneId,
) -> Result<()> {
    ensure_absent(&m.workspaces, IdKind::Workspace, id)?;
    ensure_absent(&m.panes, IdKind::Pane, pane)?;
    get(&m.categories, IdKind::Category, category)?;
    insert_at(&mut m.workspace_order, index, id)?;
    m.workspaces.insert(
        id,
        Workspace {
            name,
            category,
            layout: SplitTree::Leaf(pane),
            subtitle: String::new(),
            description: String::new(),
            metadata: Default::default(),
        },
    );
    m.panes.insert(
        pane,
        Pane {
            workspace: id,
            tabs: Vec::new(),
        },
    );
    Ok(())
}

fn move_workspace(
    m: &mut JournalModel,
    id: WorkspaceId,
    category: WorkspaceCategoryId,
    index: usize,
) -> Result<()> {
    get(&m.categories, IdKind::Category, category)?;
    get_mut(&mut m.workspaces, IdKind::Workspace, id)?.category = category;
    move_in(&mut m.workspace_order, IdKind::Workspace, id, index)
}

fn close_workspace(m: &mut JournalModel, id: WorkspaceId) -> Result<()> {
    let workspace = remove(&mut m.workspaces, IdKind::Workspace, id)?;
    m.workspace_order.retain(|w| *w != id);
    for pane in workspace.layout.leaves() {
        drop_pane(m, pane)?;
    }
    Ok(())
}

fn split_pane(m: &mut JournalModel, target: PaneId, pane: PaneId, split: SplitSpec) -> Result<()> {
    ensure_absent(&m.panes, IdKind::Pane, pane)?;
    let workspace = get(&m.panes, IdKind::Pane, target)?.workspace;
    attach_pane(m, workspace, target, pane, split)?;
    m.panes.insert(
        pane,
        Pane {
            workspace,
            tabs: Vec::new(),
        },
    );
    Ok(())
}

fn move_pane(m: &mut JournalModel, id: PaneId, target: PaneId, split: SplitSpec) -> Result<()> {
    if id == target {
        return Err(EvolveError::SelfTarget(name(IdKind::Pane, id)));
    }
    let from = get(&m.panes, IdKind::Pane, id)?.workspace;
    let to = get(&m.panes, IdKind::Pane, target)?.workspace;
    detach_pane(m, from, id)?;
    attach_pane(m, to, target, id, split)?;
    get_mut(&mut m.panes, IdKind::Pane, id)?.workspace = to;
    Ok(())
}

fn close_pane(m: &mut JournalModel, id: PaneId) -> Result<()> {
    let workspace = get(&m.panes, IdKind::Pane, id)?.workspace;
    detach_pane(m, workspace, id)?;
    drop_pane(m, id)
}

fn attach_pane(
    m: &mut JournalModel,
    workspace: WorkspaceId,
    target: PaneId,
    pane: PaneId,
    split: SplitSpec,
) -> Result<()> {
    let layout = &mut get_mut(&mut m.workspaces, IdKind::Workspace, workspace)?.layout;
    if layout.split_leaf(target, pane, split.direction, split.ratio, split.placement) {
        Ok(())
    } else {
        Err(EvolveError::Missing(format!(
            "pane:{target} in workspace:{workspace}"
        )))
    }
}

fn detach_pane(m: &mut JournalModel, workspace: WorkspaceId, pane: PaneId) -> Result<()> {
    let layout = &mut get_mut(&mut m.workspaces, IdKind::Workspace, workspace)?.layout;
    removed(layout.remove_leaf(pane), name(IdKind::Pane, pane))
}

/// pane과 소속 tab·surface를 지운다. 배치에서는 호출자가 이미 뺐다.
fn drop_pane(m: &mut JournalModel, id: PaneId) -> Result<()> {
    let pane = remove(&mut m.panes, IdKind::Pane, id)?;
    for tab in pane.tabs {
        drop_tab(m, tab)?;
    }
    Ok(())
}

fn create_tab(
    m: &mut JournalModel,
    id: TabId,
    pane: PaneId,
    index: usize,
    name: String,
    surface: SurfaceSpec,
) -> Result<()> {
    ensure_absent(&m.tabs, IdKind::Tab, id)?;
    ensure_absent(&m.surfaces, IdKind::Surface, surface.id)?;
    insert_at(
        &mut get_mut(&mut m.panes, IdKind::Pane, pane)?.tabs,
        index,
        id,
    )?;
    m.tabs.insert(
        id,
        Tab {
            pane,
            name,
            explicit_name: None,
            layout: SplitTree::Leaf(surface.id),
        },
    );
    insert_surface(m, id, surface);
    Ok(())
}

fn move_tab(m: &mut JournalModel, id: TabId, pane: PaneId, index: usize) -> Result<()> {
    let from = get(&m.tabs, IdKind::Tab, id)?.pane;
    get(&m.panes, IdKind::Pane, pane)?;
    get_mut(&mut m.panes, IdKind::Pane, from)?
        .tabs
        .retain(|t| *t != id);
    insert_at(
        &mut get_mut(&mut m.panes, IdKind::Pane, pane)?.tabs,
        index,
        id,
    )?;
    get_mut(&mut m.tabs, IdKind::Tab, id)?.pane = pane;
    Ok(())
}

fn close_tab(m: &mut JournalModel, id: TabId) -> Result<()> {
    let pane = get(&m.tabs, IdKind::Tab, id)?.pane;
    get_mut(&mut m.panes, IdKind::Pane, pane)?
        .tabs
        .retain(|t| *t != id);
    drop_tab(m, id)
}

fn drop_tab(m: &mut JournalModel, id: TabId) -> Result<()> {
    let tab = remove(&mut m.tabs, IdKind::Tab, id)?;
    for surface in tab.layout.leaves() {
        remove(&mut m.surfaces, IdKind::Surface, surface)?;
    }
    Ok(())
}

fn split_surface(
    m: &mut JournalModel,
    target: SurfaceId,
    surface: SurfaceSpec,
    split: SplitSpec,
) -> Result<()> {
    ensure_absent(&m.surfaces, IdKind::Surface, surface.id)?;
    let tab = get(&m.surfaces, IdKind::Surface, target)?.tab;
    attach_surface(m, tab, target, surface.id, split)?;
    insert_surface(m, tab, surface);
    Ok(())
}

fn move_surface(
    m: &mut JournalModel,
    id: SurfaceId,
    target: SurfaceId,
    split: SplitSpec,
) -> Result<()> {
    if id == target {
        return Err(EvolveError::SelfTarget(name(IdKind::Surface, id)));
    }
    let from = get(&m.surfaces, IdKind::Surface, id)?.tab;
    let to = get(&m.surfaces, IdKind::Surface, target)?.tab;
    let layout = &mut get_mut(&mut m.tabs, IdKind::Tab, from)?.layout;
    removed(layout.remove_leaf(id), name(IdKind::Surface, id))?;
    attach_surface(m, to, target, id, split)?;
    get_mut(&mut m.surfaces, IdKind::Surface, id)?.tab = to;
    Ok(())
}

fn close_surface(m: &mut JournalModel, id: SurfaceId) -> Result<()> {
    let tab = get(&m.surfaces, IdKind::Surface, id)?.tab;
    let layout = &mut get_mut(&mut m.tabs, IdKind::Tab, tab)?.layout;
    removed(layout.remove_leaf(id), name(IdKind::Surface, id))?;
    m.surfaces.remove(&id);
    Ok(())
}

fn attach_surface(
    m: &mut JournalModel,
    tab: TabId,
    target: SurfaceId,
    surface: SurfaceId,
    split: SplitSpec,
) -> Result<()> {
    let layout = &mut get_mut(&mut m.tabs, IdKind::Tab, tab)?.layout;
    if layout.split_leaf(
        target,
        surface,
        split.direction,
        split.ratio,
        split.placement,
    ) {
        Ok(())
    } else {
        Err(EvolveError::Missing(format!(
            "surface:{target} in tab:{tab}"
        )))
    }
}

fn insert_surface(m: &mut JournalModel, tab: TabId, spec: SurfaceSpec) {
    m.surfaces.insert(
        spec.id,
        Surface {
            tab,
            kind: spec.kind,
            data: spec.data,
            creation_seed: None,
            metadata: Default::default(),
            activation: None,
            content_generation: 0,
            snapshot_schema: 0,
        },
    );
}

fn metadata_mut(
    m: &mut JournalModel,
    target: MetadataTarget,
) -> Result<&mut BTreeMap<String, String>> {
    Ok(match target {
        MetadataTarget::Workspace(id) => {
            &mut get_mut(&mut m.workspaces, IdKind::Workspace, id)?.metadata
        }
        MetadataTarget::Surface(id) => &mut get_mut(&mut m.surfaces, IdKind::Surface, id)?.metadata,
    })
}

fn target_name(target: MetadataTarget) -> String {
    match target {
        MetadataTarget::Workspace(id) => name(IdKind::Workspace, id),
        MetadataTarget::Surface(id) => name(IdKind::Surface, id),
    }
}

fn removed(result: RemoveLeaf, name: String) -> Result<()> {
    match result {
        RemoveLeaf::Removed => Ok(()),
        RemoveLeaf::NotFound => Err(EvolveError::Missing(name)),
        RemoveLeaf::LastLeaf => Err(EvolveError::LastLeaf(name)),
    }
}

type Map<V> = BTreeMap<u32, V>;

fn name(kind: IdKind, id: u32) -> String {
    format!("{}:{id}", kind.label())
}

fn get<V>(map: &Map<V>, kind: IdKind, id: u32) -> Result<&V> {
    map.get(&id)
        .ok_or_else(|| EvolveError::Missing(name(kind, id)))
}

fn get_mut<V>(map: &mut Map<V>, kind: IdKind, id: u32) -> Result<&mut V> {
    map.get_mut(&id)
        .ok_or_else(|| EvolveError::Missing(name(kind, id)))
}

fn remove<V>(map: &mut Map<V>, kind: IdKind, id: u32) -> Result<V> {
    map.remove(&id)
        .ok_or_else(|| EvolveError::Missing(name(kind, id)))
}

fn ensure_absent<V>(map: &Map<V>, kind: IdKind, id: u32) -> Result<()> {
    if map.contains_key(&id) {
        Err(EvolveError::Duplicate(name(kind, id)))
    } else {
        Ok(())
    }
}

fn insert_at<T>(list: &mut Vec<T>, index: usize, item: T) -> Result<()> {
    if index > list.len() {
        return Err(EvolveError::IndexOutOfRange {
            index,
            len: list.len(),
        });
    }
    list.insert(index, item);
    Ok(())
}

/// 목록에서 빼서 `index`에 다시 넣는다. `index`는 뺀 뒤 목록 기준이다.
fn move_in(list: &mut Vec<u32>, kind: IdKind, item: u32, index: usize) -> Result<()> {
    let Some(pos) = list.iter().position(|x| *x == item) else {
        return Err(EvolveError::Missing(name(kind, item)));
    };
    list.remove(pos);
    insert_at(list, index, item)
}
