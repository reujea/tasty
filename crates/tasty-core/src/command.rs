//! Structural inputs contain fixed identities and values, never live services or an ID allocator.

mod assembly;
mod bootstrap;
mod creation;
mod forward;
mod metadata;
mod replacement;
pub(crate) mod retirement;

use serde::{Deserialize, Serialize};
use tasty_model::WorkspaceAttachMapping;

use crate::{DomainEvent, JournalModel, Ratio};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StructuralCommand {
    ReconcilePreparation {
        operation: crate::OperationId,
        evidence: crate::DataRef,
        discarded: Option<String>,
    },
    ReconcileRetirement {
        operation: crate::OperationId,
        evidence: crate::DataRef,
    },
    RecoverOperation {
        operation: crate::OperationId,
        outcome: crate::OperationOutcome,
        evidence: crate::DataRef,
    },
    Replace {
        operation: crate::OperationId,
        command_id: String,
        input: crate::DataRef,
        replacement: crate::Replacement,
        expected: Vec<crate::RetiredSurface>,
    },
    PrepareForward {
        operation: crate::OperationId,
        command_id: String,
        input: crate::DataRef,
    },
    FinishForward {
        operation: crate::OperationId,
        outcome: crate::OperationOutcome,
    },
    PrepareAssembly {
        operation: crate::OperationId,
        command_id: String,
        input: crate::DataRef,
        plan: crate::CreationAssembly,
    },
    RecordCapture {
        surface: u32,
        kind: String,
        activation: Option<u64>,
        content_generation: u64,
        snapshot_schema: u32,
        data: crate::DataRef,
    },
    Close {
        operation: crate::OperationId,
        command_id: String,
        input: crate::DataRef,
        target: crate::CloseTarget,
        expected: Vec<crate::RetiredSurface>,
        undo: Option<crate::UndoCapture>,
        is_user_close: bool,
        #[serde(default)]
        remote_user_close: bool,
    },
    FinishRetirement {
        operation: crate::OperationId,
        outcome: crate::OperationOutcome,
    },
    RetireEngine {
        expected_incarnation: u64,
    },
    OpenEngine {
        expected_incarnation: u64,
        reset_structure: bool,
        normal_category_name: String,
    },
    PrepareCreation {
        operation: crate::OperationId,
        command_id: String,
        input: crate::DataRef,
        plan: crate::CreationPlan,
    },
    CancelUnstartedCreation {
        operation: crate::OperationId,
        reason: String,
    },
    RejectInstallation {
        operation: crate::OperationId,
        reason: String,
    },
    FinishCleanup {
        operation: crate::OperationId,
    },
    MarkPreparationUncertain {
        operation: crate::OperationId,
        reason: String,
    },
    FinishCreation {
        operation: crate::OperationId,
        result: crate::PreparationResult,
    },
    ResetCategories,
    CreateCategory {
        reserved_id: u32,
        name: String,
    },
    RenameCategory {
        id: u32,
        name: String,
    },
    DeleteCategory {
        id: u32,
    },
    ReorderCategory {
        id: u32,
        to_index: usize,
    },
    SetWorkspaceCategory {
        workspace_id: u32,
        category: u32,
    },
    UpdateWorkspaceMeta {
        workspace_id: u32,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
    },
    SetWorkspaceAttachMapping {
        workspace_id: u32,
        mapping: Option<WorkspaceAttachMapping>,
    },
    MoveWorkspace {
        workspace_id: u32,
        to_index: usize,
    },
    /// 저장된 원격 행을 만들거나 값을 바꾼다. 없는 카테고리는 기본 카테고리로 저장한다.
    SetRemoteRow {
        id: u32,
        row: crate::model::RemoteRow,
    },
    /// 저장된 원격 행을 지운다. 없는 행이면 이벤트를 만들지 않는다.
    RemoveRemoteRow {
        id: u32,
    },
    RenameTab {
        tab_id: u32,
        name: Option<String>,
    },
    MoveTab {
        pane_id: u32,
        tab_id: u32,
        to_index: usize,
    },
    SetPaneRatio {
        workspace_id: u32,
        path: Vec<bool>,
        expected_leaves: Vec<u32>,
        expected_revision: u64,
        ratio: Ratio,
    },
    SetSurfaceRatio {
        tab_id: u32,
        path: Vec<bool>,
        expected_leaves: Vec<u32>,
        expected_revision: u64,
        ratio: Ratio,
    },
}

impl StructuralCommand {
    /// Fresh identities required by this command. Reservation is an admission task, never decide I/O.
    pub fn reserved_ids(&self) -> Vec<crate::EntityId> {
        match self {
            Self::PrepareCreation { plan, .. } => plan.reserved_ids(),
            Self::PrepareAssembly { plan, .. } => plan.reserved_ids(),
            Self::CreateCategory { reserved_id, .. } => vec![crate::EntityId {
                kind: crate::IdKind::Category,
                id: *reserved_id,
            }],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructuralResult {
    Closed {
        closed: bool,
    },
    EngineOpened {
        incarnation: u64,
    },
    Pending {
        operation: crate::OperationId,
    },
    Created {
        workspace: Option<u32>,
        pane: Option<u32>,
        tab: Option<u32>,
        surface: u32,
    },
    Failed {
        reason: String,
    },
    Updated,
    CreatedCategory {
        id: u32,
    },
    Moved {
        moved: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuralDecision {
    pub events: Vec<DomainEvent>,
    pub result: StructuralResult,
    pub effects: Vec<crate::StructuralEffect>,
    pub completed_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Rejection(pub String);

/// Decide on immutable structure and validate the complete candidate without publishing it.
pub fn decide_structure(
    model: &JournalModel,
    command: &StructuralCommand,
) -> Result<StructuralDecision, Rejection> {
    if model.engine_retired
        && !matches!(
            command,
            StructuralCommand::ReconcilePreparation { .. }
                | StructuralCommand::ReconcileRetirement { .. }
                | StructuralCommand::RecoverOperation { .. }
                | StructuralCommand::OpenEngine { .. }
                | StructuralCommand::RetireEngine { .. }
                | StructuralCommand::FinishRetirement { .. }
                | StructuralCommand::FinishForward { .. }
                | StructuralCommand::FinishCreation { .. }
                | StructuralCommand::FinishCleanup { .. }
                | StructuralCommand::RejectInstallation { .. }
                | StructuralCommand::MarkPreparationUncertain { .. }
                | StructuralCommand::CancelUnstartedCreation { .. }
        )
    {
        return Err(Rejection("engine binding has been retired".into()));
    }
    let decision = match command {
        StructuralCommand::ReconcilePreparation {
            operation,
            evidence,
            discarded,
        } => creation::reconcile(model, operation, *evidence, discarded.as_deref())?,
        StructuralCommand::ReconcileRetirement {
            operation,
            evidence,
        } => {
            let previous = model
                .operations
                .get(operation)
                .ok_or_else(|| Rejection("reconciliation operation missing".into()))?;
            if previous.retirement.is_none()
                || !matches!(
                    previous.outcome,
                    Some(crate::OperationOutcome::Uncertain { .. })
                )
                || evidence.0 == 0
            {
                return Err(Rejection(
                    "retirement reconciliation needs an uncertain cleanup and evidence".into(),
                ));
            }
            StructuralDecision {
                events: vec![DomainEvent::OperationReconciled {
                    id: operation.clone(),
                    outcome: crate::OperationOutcome::Succeeded,
                    evidence: *evidence,
                }],
                effects: Vec::new(),
                result: StructuralResult::Closed { closed: true },
                completed_command: Some(previous.command_id.clone()),
            }
        }
        StructuralCommand::RecoverOperation {
            operation,
            outcome,
            evidence,
        } => {
            let previous = model
                .operations
                .get(operation)
                .ok_or_else(|| Rejection("recovery operation missing".into()))?;
            if previous.outcome.is_some()
                || evidence.0 == 0
                || !matches!(
                    outcome,
                    crate::OperationOutcome::Cancelled { .. }
                        | crate::OperationOutcome::Uncertain { .. }
                        | crate::OperationOutcome::Superseded { .. }
                )
            {
                return Err(Rejection(
                    "recovery observation cannot invent a successful execution".into(),
                ));
            }
            StructuralDecision {
                events: vec![
                    DomainEvent::OperationFinished {
                        id: operation.clone(),
                        outcome: outcome.clone(),
                    },
                    DomainEvent::OperationRecoveryObserved {
                        id: operation.clone(),
                        evidence: *evidence,
                    },
                ],
                effects: Vec::new(),
                result: StructuralResult::Pending {
                    operation: operation.clone(),
                },
                completed_command: Some(previous.command_id.clone()),
            }
        }
        StructuralCommand::Replace { .. } => replacement::decide(model, command)?,
        StructuralCommand::PrepareForward { .. } | StructuralCommand::FinishForward { .. } => {
            forward::decide(model, command)?
        }
        StructuralCommand::PrepareAssembly { .. } => assembly::decide(model, command)?,
        StructuralCommand::RecordCapture {
            surface,
            kind,
            activation,
            content_generation,
            snapshot_schema,
            data,
        } => {
            let current = model
                .surfaces
                .get(surface)
                .ok_or_else(|| Rejection("capture target no longer exists".into()))?;
            if current.kind != *kind
                || current.activation.map(|activation| activation.generation) != *activation
            {
                return Err(Rejection(
                    "capture belongs to an earlier kind instance".into(),
                ));
            }
            StructuralDecision {
                events: vec![DomainEvent::SurfaceDataRecorded {
                    id: *surface,
                    activation_generation: *activation,
                    content_generation: *content_generation,
                    snapshot_schema: *snapshot_schema,
                    data: *data,
                }],
                effects: Vec::new(),
                result: StructuralResult::Updated,
                completed_command: None,
            }
        }
        StructuralCommand::Close { .. } | StructuralCommand::FinishRetirement { .. } => {
            retirement::decide(model, command)?
        }
        StructuralCommand::OpenEngine { .. } | StructuralCommand::RetireEngine { .. } => {
            bootstrap::decide(model, command)?
        }
        StructuralCommand::PrepareCreation { .. }
        | StructuralCommand::FinishCreation { .. }
        | StructuralCommand::FinishCleanup { .. }
        | StructuralCommand::CancelUnstartedCreation { .. }
        | StructuralCommand::RejectInstallation { .. }
        | StructuralCommand::MarkPreparationUncertain { .. } => creation::decide(model, command)?,
        _ => metadata::decide(model, command)?,
    };
    let mut candidate = model.clone();
    let after = model.applied.revision.unwrap_or(0);
    let batch_id = model
        .applied
        .batch
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| Rejection("journal batch range exhausted".into()))?;
    let events = decision
        .events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let revision = after
                .checked_add(index as u64 + 1)
                .ok_or_else(|| Rejection("structure revision range exhausted".into()))?;
            Ok(crate::RecordedEvent {
                revision,
                event: event.clone(),
            })
        })
        .collect::<Result<Vec<_>, Rejection>>()?;
    crate::evolve(&mut candidate, &crate::DomainBatch { batch_id, events })
        .map_err(|e| Rejection(e.to_string()))?;
    Ok(decision)
}
