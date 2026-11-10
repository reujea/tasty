//! Resolve immutable undo values and reserve all replacement identities on the journal worker.
use super::super::{EngineBinding, ShellRecipe};
use super::*;
use std::collections::BTreeMap;
use tasty_core::{
    AssemblyDestination, ClosedSnapshot, CreationAssembly, DataRef, EntityId, IdKind, Pane,
    SplitTree, Tab, Workspace,
};

pub(super) fn undo(
    executor: &Executor<StructureDecider>,
    admitted: &mut Pending,
    ticket: u64,
    binding: EngineBinding,
    target_pane: Option<u32>,
    scope: Option<u32>,
    shell: ShellRecipe,
) -> Result<ResultValue, String> {
    executor
        .with_state(|_| ())
        .map_err(|error| error.to_string())?;
    let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
    let epoch = inner.epoch;
    if binding.journal_id != inner.store.journal_id() || binding.runtime_epoch != epoch.0 {
        return Err("undo belongs to another runtime".into());
    }
    let model = inner
        .state
        .streams
        .get(&binding.stream)
        .ok_or("undo engine missing")?
        .clone();
    if model.engine_retired || model.engine_incarnation != binding.incarnation {
        return Err("undo engine retired".into());
    }
    let mut selected = None;
    for record in model.undo_records.iter().rev() {
        let bytes = inner
            .store
            .read_payload(tasty_event_store::PayloadRef(record.capture.snapshot.0))
            .map_err(|error| error.to_string())?;
        let snapshot: ClosedSnapshot =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if scope.is_none() || snapshot.origin_workspace == scope {
            selected = Some((record.clone(), snapshot));
            break;
        }
    }
    let Some((record, mut snapshot)) = selected else {
        return Ok(ResultValue::AssemblyResolved {
            stream: binding.stream,
            input: None,
            plan: None,
        });
    };
    let holder = format!("admission/{}/{ticket}", epoch.0);
    // Single-surface undo is restored as a tab, retaining the captured display title.
    // Temporary keys exist only inside this unpublished value and are remapped below.
    if snapshot.root.kind == IdKind::Surface {
        let surface = snapshot.root.id;
        snapshot.root = EntityId {
            kind: IdKind::Tab,
            id: 1,
        };
        snapshot
            .surfaces
            .get_mut(&surface)
            .ok_or("undo surface root missing")?
            .tab = 1;
        snapshot.tabs.insert(
            1,
            Tab {
                pane: 1,
                name: snapshot
                    .tab_name
                    .clone()
                    .unwrap_or_else(|| crate::i18n::t("surface.restore.tab_title_fallback").into()),
                explicit_name: None,
                layout: SplitTree::Leaf(surface),
            },
        );
    }
    let target = target_pane.filter(|pane| model.panes.contains_key(pane));
    if snapshot.root.kind != IdKind::Workspace && target.is_none() {
        if scope.is_some() {
            return Err("scoped undo target is missing".into());
        }
        if snapshot.root.kind == IdKind::Tab {
            let tab = snapshot.root.id;
            snapshot
                .tabs
                .get_mut(&tab)
                .ok_or("undo tab root missing")?
                .pane = 1;
            snapshot.panes.insert(
                1,
                Pane {
                    workspace: 1,
                    tabs: vec![tab],
                },
            );
            snapshot.root = EntityId {
                kind: IdKind::Pane,
                id: 1,
            };
        }
        let pane = snapshot.root.id;
        snapshot
            .panes
            .get_mut(&pane)
            .ok_or("undo pane root missing")?
            .workspace = 1;
        snapshot.workspaces.insert(
            1,
            Workspace {
                name: "Workspace".into(),
                category: 0,
                subtitle: String::new(),
                description: String::new(),
                metadata: Default::default(),
                layout: SplitTree::Leaf(pane),
            },
        );
        snapshot.root = EntityId {
            kind: IdKind::Workspace,
            id: 1,
        };
    }
    reserve_remap(
        &mut inner.store,
        epoch,
        admitted,
        &model,
        &mut snapshot,
        target,
    )?;
    let destination = match snapshot.root.kind {
        IdKind::Workspace => AssemblyDestination::Workspace,
        IdKind::Pane => {
            let caller = target.ok_or("undo pane target missing")?;
            let position = snapshot
                .pane_position
                .as_ref()
                .ok_or("closed pane has no original split position")?;
            let sibling = model
                .panes
                .get(&position.sibling)
                .filter(|sibling| sibling.workspace == model.panes[&caller].workspace)
                .map(|_| position.sibling);
            AssemblyDestination::Pane {
                target: sibling.unwrap_or(caller),
                split: if sibling.is_some() {
                    position.split
                } else {
                    tasty_core::SplitSpec {
                        direction: position.split.direction,
                        ratio: tasty_core::Ratio::from_f32(0.5),
                        placement: tasty_core::Placement::After,
                    }
                },
            }
        }
        IdKind::Tab => {
            let pane = target.ok_or("undo tab target missing")?;
            AssemblyDestination::Tab {
                pane,
                index: model.panes[&pane].tabs.len(),
            }
        }
        _ => return Err("unsupported undo root".into()),
    };
    let mut inputs = BTreeMap::new();
    for (surface, value) in &snapshot.surfaces {
        let mut request =
            crate::runtime::surface_restorer::from_saved(*surface, value, shell.clone());
        if let Some(reference) = request.reference {
            let bytes = inner
                .store
                .read_payload(tasty_event_store::PayloadRef(reference.0))
                .map_err(|error| error.to_string())?;
            crate::runtime::surface_restorer::accept_payload(&mut request, reference, &bytes)?;
        }
        let bytes = serde_json::to_vec(&request.input).map_err(|error| error.to_string())?;
        let input = DataRef(
            inner
                .store
                .put_admission_payload_pinned(epoch, &bytes, &holder)
                .map_err(|error| error.to_string())?
                .0,
        );
        admitted.inputs.push(input);
        inputs.insert(*surface, input);
    }
    if let AssemblyDestination::Tab { pane, .. } = destination {
        snapshot
            .presentation
            .selected_tabs
            .insert(pane, snapshot.root.id);
    }
    let plan = CreationAssembly {
        snapshot,
        destination,
        inputs,
        undo: Some(record.id),
        omit_failed: true,
    };
    plan.validate_graph().map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&plan).map_err(|error| error.to_string())?;
    let input = DataRef(
        inner
            .store
            .put_admission_payload_pinned(epoch, &bytes, &holder)
            .map_err(|error| error.to_string())?
            .0,
    );
    admitted.inputs.push(input);
    Ok(ResultValue::AssemblyResolved {
        stream: binding.stream,
        input: Some(input),
        plan: Some(plan),
    })
}
fn remap_tree(tree: &mut SplitTree<u32>, ids: &BTreeMap<u32, u32>) -> Result<(), String> {
    match tree {
        SplitTree::Leaf(id) => {
            *id = *ids.get(id).ok_or("undo split leaf missing")?;
        }
        SplitTree::Split { first, second, .. } => {
            remap_tree(first, ids)?;
            remap_tree(second, ids)?;
        }
    }
    Ok(())
}

fn reserve_remap(
    store: &mut tasty_event_store::EventStore,
    epoch: tasty_event_store::WriterEpoch,
    admitted: &mut Pending,
    model: &tasty_core::JournalModel,
    snapshot: &mut ClosedSnapshot,
    target: Option<u32>,
) -> Result<BTreeMap<IdKind, BTreeMap<u32, u32>>, String> {
    let mut maps: BTreeMap<IdKind, BTreeMap<u32, u32>> = BTreeMap::new();
    for (kind, ids) in [
        (
            IdKind::Workspace,
            snapshot.workspaces.keys().copied().collect::<Vec<_>>(),
        ),
        (IdKind::Pane, snapshot.panes.keys().copied().collect()),
        (IdKind::Tab, snapshot.tabs.keys().copied().collect()),
        (IdKind::Surface, snapshot.surfaces.keys().copied().collect()),
    ] {
        if ids.is_empty() {
            continue;
        }
        let max = if kind == IdKind::Surface {
            0x7fff_ffff
        } else {
            u32::MAX
        };
        admitted.check_ids(ids.len() as u64)?;
        let range = store
            .reserve_ids(epoch, kind.label(), ids.len() as u64, u64::from(max))
            .map_err(|error| error.to_string())?;
        maps.insert(
            kind,
            ids.into_iter()
                .enumerate()
                .map(|(offset, old)| (old, (range.start + offset as u64) as u32))
                .collect(),
        );
        admitted.reservations.push(range);
    }
    let map = |kind, id| {
        maps.get(&kind)
            .and_then(|map| map.get(&id))
            .copied()
            .ok_or_else(|| format!("undo dangling {}:{id}", kind.label()))
    };
    snapshot.root.id = map(snapshot.root.kind, snapshot.root.id)?;
    snapshot.workspaces = std::mem::take(&mut snapshot.workspaces)
        .into_iter()
        .map(|(id, mut value)| {
            remap_tree(
                &mut value.layout,
                maps.get(&IdKind::Pane).ok_or("undo pane map missing")?,
            )?;
            Ok((map(IdKind::Workspace, id)?, value))
        })
        .collect::<Result<_, String>>()?;
    snapshot.panes = std::mem::take(&mut snapshot.panes)
        .into_iter()
        .map(|(id, mut value)| {
            if maps.contains_key(&IdKind::Workspace) {
                value.workspace = map(IdKind::Workspace, value.workspace)?;
            } else if let Some(target) = target {
                value.workspace = model.panes[&target].workspace;
            }
            value.tabs = value
                .tabs
                .into_iter()
                .map(|id| map(IdKind::Tab, id))
                .collect::<Result<_, _>>()?;
            Ok((map(IdKind::Pane, id)?, value))
        })
        .collect::<Result<_, String>>()?;
    snapshot.tabs = std::mem::take(&mut snapshot.tabs)
        .into_iter()
        .map(|(id, mut value)| {
            value.pane = if maps.contains_key(&IdKind::Pane) {
                map(IdKind::Pane, value.pane)?
            } else {
                target.ok_or("undo tab destination missing")?
            };
            remap_tree(
                &mut value.layout,
                maps.get(&IdKind::Surface)
                    .ok_or("undo surface map missing")?,
            )?;
            Ok((map(IdKind::Tab, id)?, value))
        })
        .collect::<Result<_, String>>()?;
    snapshot.surfaces = std::mem::take(&mut snapshot.surfaces)
        .into_iter()
        .map(|(id, mut value)| {
            value.tab = map(IdKind::Tab, value.tab)?;
            value.activation = None;
            Ok((map(IdKind::Surface, id)?, value))
        })
        .collect::<Result<_, String>>()?;
    let remap_pairs =
        |values: &BTreeMap<u32, u32>, parent: IdKind, child: IdKind| -> BTreeMap<u32, u32> {
            values
                .iter()
                .filter_map(|(one, two)| {
                    Some((
                        maps.get(&parent)?.get(one).copied()?,
                        maps.get(&child)?.get(two).copied()?,
                    ))
                })
                .collect()
        };
    snapshot.presentation = tasty_core::UndoPresentation {
        focused_panes: remap_pairs(
            &snapshot.presentation.focused_panes,
            IdKind::Workspace,
            IdKind::Pane,
        ),
        selected_tabs: remap_pairs(
            &snapshot.presentation.selected_tabs,
            IdKind::Pane,
            IdKind::Tab,
        ),
        selected_surfaces: remap_pairs(
            &snapshot.presentation.selected_surfaces,
            IdKind::Tab,
            IdKind::Surface,
        ),
    };
    Ok(maps)
}

pub(super) fn preset(
    executor: &Executor<StructureDecider>,
    admitted: &mut Pending,
    ticket: u64,
    binding: EngineBinding,
    mut draft: crate::runtime::preset_plan::AssemblyDraft,
) -> Result<ResultValue, String> {
    draft.validate()?;
    executor
        .with_state(|_| ())
        .map_err(|error| error.to_string())?;
    let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
    let epoch = inner.epoch;
    let model = inner
        .state
        .streams
        .get(&binding.stream)
        .ok_or("preset engine missing")?
        .clone();
    if binding.journal_id != inner.store.journal_id()
        || binding.runtime_epoch != epoch.0
        || model.engine_retired
        || model.engine_incarnation != binding.incarnation
    {
        return Err("preset engine binding retired".into());
    }
    let target = match draft.destination {
        AssemblyDestination::Workspace => None,
        AssemblyDestination::Pane { target, .. } => Some(target),
        AssemblyDestination::Tab { pane, .. } => Some(pane),
    };
    let maps = reserve_remap(
        &mut inner.store,
        epoch,
        admitted,
        &model,
        &mut draft.snapshot,
        target,
    )?;
    if let AssemblyDestination::Tab { pane, .. } = draft.destination {
        draft
            .snapshot
            .presentation
            .selected_tabs
            .insert(pane, draft.snapshot.root.id);
    }
    let holder = format!("admission/{}/{ticket}", epoch.0);
    let mut inputs = BTreeMap::new();
    for (label, input) in draft.inputs {
        let surface = maps
            .get(&IdKind::Surface)
            .and_then(|map| map.get(&label))
            .copied()
            .ok_or("preset input label missing")?;
        let bytes = serde_json::to_vec(&input).map_err(|error| error.to_string())?;
        let reference = DataRef(
            inner
                .store
                .put_admission_payload_pinned(epoch, &bytes, &holder)
                .map_err(|error| error.to_string())?
                .0,
        );
        admitted.inputs.push(reference);
        inputs.insert(surface, reference);
    }
    let plan = CreationAssembly {
        snapshot: draft.snapshot,
        destination: draft.destination,
        inputs,
        undo: None,
        omit_failed: draft.omit_failed,
    };
    let bytes = serde_json::to_vec(&plan).map_err(|error| error.to_string())?;
    let input = DataRef(
        inner
            .store
            .put_admission_payload_pinned(epoch, &bytes, &holder)
            .map_err(|error| error.to_string())?
            .0,
    );
    admitted.inputs.push(input);
    Ok(ResultValue::AssemblyResolved {
        stream: binding.stream,
        input: Some(input),
        plan: Some(plan),
    })
}
