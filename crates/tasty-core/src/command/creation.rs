use super::*;
use crate::{
    Activation, ActivationPhase, CreationDestination as Destination, CreationPlan, Operation,
    OperationOutcome, PreparationResult, StructuralEffect,
};

type Result<T> = std::result::Result<T, Rejection>;

pub(super) fn decide(
    model: &JournalModel,
    command: &StructuralCommand,
) -> Result<StructuralDecision> {
    match command {
        StructuralCommand::PrepareCreation {
            operation,
            command_id,
            input,
            plan,
        } => {
            validate_target(model, plan)?;
            if model.operations.values().any(|pending| {
                pending
                    .creation
                    .as_ref()
                    .is_some_and(|creation| creation.surface.id == plan.surface.id)
                    && (pending.outcome.is_none()
                        || matches!(pending.outcome, Some(OperationOutcome::Uncertain { .. })))
            }) {
                return Err(Rejection("surface has an unresolved creation owner".into()));
            }
            if plan.surface.kind.is_empty() {
                return Err(Rejection("surface kind must not be empty".into()));
            }
            let generation = model
                .activation_high_water
                .get(&plan.surface.id)
                .copied()
                .unwrap_or(0)
                .max(
                    model
                        .surfaces
                        .get(&plan.surface.id)
                        .and_then(|surface| surface.activation)
                        .map_or(0, |activation| activation.generation),
                )
                .checked_add(1)
                .ok_or_else(|| Rejection("activation generation exhausted".into()))?;
            let operation = Operation {
                id: operation.clone(),
                command_id: command_id.clone(),
                engine_incarnation: model.engine_incarnation,
                creation: Some(plan.clone()),
                assembly: None,
                retirement: None,
                forward: false,
                targets: plan.targets(),
                reserved: plan.reserved_ids(),
                input: *input,
                activation_generation: generation,
                outcome: None,
                pending_outcome: None,
                cleanup: None,
                prepared_data: None,
                prepared_deferred: false,
                resource_prepared: false,
                reconciliation_evidence: None,
            };
            Ok(StructuralDecision {
                events: vec![DomainEvent::OperationPrepared {
                    operation: Box::new(operation.clone()),
                }],
                effects: vec![StructuralEffect::PrepareSurface {
                    operation: operation.id.clone(),
                    input: *input,
                    surface: plan.surface.id,
                    kind: plan.surface.kind.clone(),
                    activation_generation: generation,
                }],
                result: StructuralResult::Pending {
                    operation: operation.id,
                },
                completed_command: None,
            })
        }
        StructuralCommand::FinishCreation {
            operation: id,
            result,
        } => {
            let operation = model
                .operations
                .get(id)
                .ok_or_else(|| Rejection("preparation operation not found".into()))?;
            if operation.outcome.is_some()
                || operation.pending_outcome.is_some()
                || operation.resource_prepared
            {
                return Err(Rejection("preparation already has a result".into()));
            }
            let plan = operation
                .creation
                .as_ref()
                .ok_or_else(|| Rejection("operation is not a creation".into()))?;
            if matches!(plan.destination, Destination::Assembly { .. }) {
                return super::assembly::prepared_member(model, operation, result);
            }
            if let PreparationResult::Failed { reason } = result {
                return Ok(failed(operation, plan, reason.clone(), false));
            }
            if operation.engine_incarnation != model.engine_incarnation {
                return Ok(failed(
                    operation,
                    plan,
                    "engine incarnation changed during preparation".into(),
                    true,
                ));
            }
            if let Err(error) = validate_target(model, plan) {
                return Ok(failed(operation, plan, error.0, true));
            }
            let (data, deferred) = match result {
                PreparationResult::Ready { data } => (data, false),
                PreparationResult::Deferred { data } => (data, true),
                _ => unreachable!(),
            };
            let mut events = Vec::new();
            let previous_activation = match &plan.destination {
                Destination::Convert {
                    previous_activation,
                    ..
                }
                | Destination::Restore {
                    previous_activation,
                    ..
                } => Some(*previous_activation),
                _ => None,
            };
            events.push(DomainEvent::OperationAwaitingCleanup {
                id: id.clone(),
                outcome: OperationOutcome::Succeeded,
                cleanup: crate::CleanupPlan::InstallPrepared {
                    surface: plan.surface.id,
                    previous_activation,
                },
                prepared_data: *data,
                deferred,
            });
            Ok(StructuralDecision {
                events,
                completed_command: None,
                effects: Vec::new(),
                result: StructuralResult::Pending {
                    operation: id.clone(),
                },
            })
        }
        StructuralCommand::MarkPreparationUncertain {
            operation: id,
            reason,
        } => {
            let operation = model
                .operations
                .get(id)
                .ok_or_else(|| Rejection("uncertain operation not found".into()))?;
            if operation.outcome.is_some() {
                return Err(Rejection("operation already has an outcome".into()));
            }
            Ok(StructuralDecision {
                events: vec![DomainEvent::OperationFinished {
                    id: id.clone(),
                    outcome: OperationOutcome::Uncertain {
                        reason: reason.clone(),
                    },
                }],
                effects: Vec::new(),
                result: StructuralResult::Pending {
                    operation: id.clone(),
                },
                completed_command: None,
            })
        }
        StructuralCommand::FinishCleanup { operation: id } => {
            let operation = model
                .operations
                .get(id)
                .ok_or_else(|| Rejection("cleanup operation not found".into()))?;
            if operation.outcome.is_some() {
                return Err(Rejection("cleanup already finished".into()));
            }
            let outcome = operation
                .pending_outcome
                .clone()
                .ok_or_else(|| Rejection("operation is not awaiting cleanup".into()))?;
            if matches!(
                operation.creation.as_ref().map(|plan| &plan.destination),
                Some(Destination::Assembly { .. })
            ) {
                return super::assembly::settle(model, operation, outcome);
            }
            let result = match &outcome {
                OperationOutcome::Succeeded => operation
                    .creation
                    .as_ref()
                    .ok_or_else(|| Rejection("creation plan missing".into()))?
                    .created_result(),
                OperationOutcome::Failed { reason }
                | OperationOutcome::Cancelled { reason }
                | OperationOutcome::Superseded { reason }
                | OperationOutcome::Uncertain { reason } => StructuralResult::Failed {
                    reason: reason.clone(),
                },
            };
            let mut events = Vec::new();
            if matches!(
                operation.cleanup,
                Some(crate::CleanupPlan::InstallPrepared { .. })
            ) {
                let mut plan = operation
                    .creation
                    .as_ref()
                    .ok_or_else(|| Rejection("creation plan missing".into()))?
                    .clone();
                validate_target(model, &plan)?;
                if operation.engine_incarnation != model.engine_incarnation {
                    return Err(Rejection(
                        "engine incarnation changed while installation was authorized".into(),
                    ));
                }
                plan.surface.data = operation.prepared_data;
                events = creation_events(model, &plan);
                let previous_generation = model
                    .surfaces
                    .get(&plan.surface.id)
                    .and_then(|surface| surface.activation.map(|activation| activation.generation));
                events.push(DomainEvent::SurfaceActivationChanged {
                    id: plan.surface.id,
                    previous_generation,
                    activation: Activation {
                        generation: operation.activation_generation,
                        phase: if operation.prepared_deferred {
                            ActivationPhase::Deferred
                        } else {
                            ActivationPhase::Ready
                        },
                    },
                });
                if !matches!(plan.destination, Destination::Restore { .. }) {
                    events.push(DomainEvent::SurfaceCreationSeeded {
                        id: plan.surface.id,
                        activation_generation: operation.activation_generation,
                        input: operation.input,
                    });
                }
            }
            events.push(DomainEvent::OperationFinished {
                id: id.clone(),
                outcome,
            });
            Ok(StructuralDecision {
                events,
                effects: Vec::new(),
                completed_command: Some(operation.command_id.clone()),
                result,
            })
        }
        StructuralCommand::RejectInstallation {
            operation: id,
            reason,
        } => {
            let operation = model
                .operations
                .get(id)
                .ok_or_else(|| Rejection("installation operation missing".into()))?;
            if operation.outcome.is_some()
                || !matches!(
                    operation.cleanup,
                    Some(crate::CleanupPlan::InstallPrepared { .. })
                )
            {
                return Err(Rejection(
                    "installation is not waiting for publication".into(),
                ));
            }
            if matches!(
                operation.creation.as_ref().map(|plan| &plan.destination),
                Some(Destination::Assembly { .. })
            ) {
                return super::assembly::settle(
                    model,
                    operation,
                    OperationOutcome::Failed {
                        reason: reason.clone(),
                    },
                );
            }
            Ok(StructuralDecision {
                events: vec![DomainEvent::OperationFinished {
                    id: id.clone(),
                    outcome: OperationOutcome::Failed {
                        reason: reason.clone(),
                    },
                }],
                effects: Vec::new(),
                completed_command: Some(operation.command_id.clone()),
                result: StructuralResult::Failed {
                    reason: reason.clone(),
                },
            })
        }
        StructuralCommand::CancelUnstartedCreation {
            operation: id,
            reason,
        } => {
            let operation = model
                .operations
                .get(id)
                .ok_or_else(|| Rejection("preparation operation not found".into()))?;
            if operation.outcome.is_some()
                || operation.pending_outcome.is_some()
                || operation.resource_prepared
            {
                return Err(Rejection("operation is no longer unstarted".into()));
            }
            if matches!(
                operation.creation.as_ref().map(|plan| &plan.destination),
                Some(Destination::Assembly { .. })
            ) {
                return super::assembly::cancel_unstarted_member(model, operation, reason);
            }
            Ok(StructuralDecision {
                events: vec![DomainEvent::OperationFinished {
                    id: id.clone(),
                    outcome: OperationOutcome::Cancelled {
                        reason: reason.clone(),
                    },
                }],
                effects: Vec::new(),
                completed_command: Some(operation.command_id.clone()),
                result: StructuralResult::Failed {
                    reason: reason.clone(),
                },
            })
        }
        _ => unreachable!("creation commands are dispatched explicitly"),
    }
}

/// Reuse terminal creation rules only after the adapter supplied exact-owner receipt evidence.
/// The scratch model merely permits those rules to run; only reconciliation facts touch history.
pub(super) fn reconcile(
    model: &JournalModel,
    id: &crate::OperationId,
    evidence: crate::DataRef,
    discarded: Option<&str>,
) -> Result<StructuralDecision> {
    let previous = model
        .operations
        .get(id)
        .ok_or_else(|| Rejection("preparation reconciliation operation missing".into()))?;
    let plan = previous
        .creation
        .as_ref()
        .ok_or_else(|| Rejection("reconciliation operation is not a creation".into()))?;
    if evidence.0 == 0 || !matches!(previous.outcome, Some(OperationOutcome::Uncertain { .. })) {
        return Err(Rejection(
            "preparation reconciliation requires an uncertain owner and evidence".into(),
        ));
    }
    let mut resumed = model.clone();
    resumed
        .operations
        .get_mut(id)
        .ok_or_else(|| Rejection("preparation owner disappeared".into()))?
        .outcome = None;
    if let Destination::Assembly { operation: group } = &plan.destination {
        let coordinator = resumed
            .operations
            .get_mut(group)
            .ok_or_else(|| Rejection("assembly coordinator missing".into()))?;
        if matches!(
            coordinator.outcome,
            Some(OperationOutcome::Uncertain { .. })
        ) {
            coordinator.outcome = None;
        }
    }
    let command = match discarded {
        Some(reason) => StructuralCommand::RejectInstallation {
            operation: id.clone(),
            reason: reason.to_owned(),
        },
        None => StructuralCommand::FinishCleanup {
            operation: id.clone(),
        },
    };
    let mut decision = decide(&resumed, &command)?;
    let mut events = Vec::with_capacity(decision.events.len());
    for event in decision.events {
        match event {
            DomainEvent::OperationFinished { id, outcome }
                if model.operations.get(&id).is_some_and(|operation| {
                    matches!(operation.outcome, Some(OperationOutcome::Uncertain { .. }))
                }) =>
            {
                // Unresolved peers/target changes do not invent a second reconciliation.
                if !matches!(outcome, OperationOutcome::Uncertain { .. }) {
                    events.push(DomainEvent::OperationReconciled {
                        id,
                        outcome,
                        evidence,
                    });
                }
            }
            event => events.push(event),
        }
    }
    decision.events = events;
    Ok(decision)
}

fn validate_target(model: &JournalModel, plan: &CreationPlan) -> Result<()> {
    if model.engine_retired || !plan.target_is_live(model) {
        return Err(Rejection(
            "creation target no longer exists or its activation changed".into(),
        ));
    }
    Ok(())
}

fn failed(
    operation: &Operation,
    plan: &CreationPlan,
    reason: String,
    discard: bool,
) -> StructuralDecision {
    let outcome = if discard {
        OperationOutcome::Cancelled {
            reason: reason.clone(),
        }
    } else {
        OperationOutcome::Failed {
            reason: reason.clone(),
        }
    };
    StructuralDecision {
        events: vec![if discard {
            DomainEvent::OperationAwaitingCleanup {
                id: operation.id.clone(),
                outcome,
                cleanup: crate::CleanupPlan::DiscardPrepared {
                    surface: plan.surface.id,
                    activation_generation: operation.activation_generation,
                },
                prepared_data: None,
                deferred: false,
            }
        } else {
            DomainEvent::OperationFinished {
                id: operation.id.clone(),
                outcome,
            }
        }],
        effects: Vec::new(),
        completed_command: (!discard).then(|| operation.command_id.clone()),
        result: StructuralResult::Failed { reason },
    }
}

fn creation_events(model: &JournalModel, plan: &CreationPlan) -> Vec<DomainEvent> {
    let mut events = Vec::new();
    let tab = match &plan.destination {
        Destination::Workspace {
            workspace,
            pane,
            tab,
            name,
            category,
            subtitle,
            description,
        } => {
            events.push(DomainEvent::WorkspaceCreated {
                id: *workspace,
                name: name.clone(),
                category: *category,
                index: model.workspace_order.len(),
                pane: *pane,
            });
            events.push(DomainEvent::WorkspaceDetailsSet {
                id: *workspace,
                subtitle: subtitle.clone(),
                description: description.clone(),
            });
            Some((*pane, *tab, 0))
        }
        Destination::Tab { pane, tab, index } => Some((*pane, *tab, *index)),
        Destination::Pane {
            target,
            pane,
            tab,
            split,
        } => {
            events.push(DomainEvent::PaneSplit {
                target: *target,
                pane: *pane,
                split: *split,
            });
            Some((*pane, *tab, 0))
        }
        Destination::Split { target, split } => {
            events.push(DomainEvent::SurfaceSplit {
                target: *target,
                surface: plan.surface.clone(),
                split: *split,
            });
            None
        }
        Destination::Restore { .. } | Destination::Assembly { .. } => None,
        Destination::Convert {
            surface,
            explicit_name,
            ..
        } => {
            events.push(DomainEvent::SurfaceConverted {
                id: *surface,
                kind: plan.surface.kind.clone(),
                data: plan.surface.data,
            });
            if let Some(name) = explicit_name
                && let Some(tab) = model
                    .surfaces
                    .get(surface)
                    .and_then(|surface| model.tabs.get(&surface.tab))
                && matches!(tab.layout, crate::SplitTree::Leaf(_))
            {
                events.push(DomainEvent::TabExplicitNameSet {
                    id: model.surfaces[surface].tab,
                    name: name.clone(),
                });
            }
            None
        }
    };
    if let Some((pane, id, index)) = tab {
        events.push(DomainEvent::TabCreated {
            id,
            pane,
            index,
            name: plan.tab_name.clone(),
            surface: plan.surface.clone(),
        });
        if let Some(name) = &plan.explicit_name {
            events.push(DomainEvent::TabExplicitNameSet {
                id,
                name: Some(name.clone()),
            });
        }
    }
    events
}
