//! A live request continuation. The engine owns the unpublished materialization, never this router.
mod preparing;
mod settle;
mod uncertain;
use super::*;
use crate::runtime::effect_runner::{self, ExecutionBinding, Installed};
use crate::runtime::journal_product::{Admission, PreparationInput, ShellRecipe, StreamCommand};
use tasty_core::{
    CreationDestination, CreationPlan, IdKind, OperationId, PreparationResult, StructuralCommand,
    StructuralResult, SurfaceSpec,
};
use uncertain::{Uncertain, UncertainOwner};

/// Process-local observation barrier only. Ready is emitted after the full publication ACK;
/// Failed denies this join and does not classify the durable operation as Failed or Cancelled.
/// Recovery/cancellation must consult OperationOutcome and the retained effect obligation.
#[derive(Clone)]
pub(crate) enum ActivationOutcome {
    Ready {
        activation: Option<u64>,
        physical: Option<tasty_terminal::ResourceGeneration>,
    },
    Failed(String),
}
pub(crate) type ActivationReceipt = std::sync::Arc<std::sync::OnceLock<ActivationOutcome>>;

pub(super) struct Creation {
    pub(super) ticket: u64,
    binding: crate::runtime::journal_product::EngineBinding,
    input: PreparationInput,
    fixed_plan: Option<CreationPlan>,
    requested_restore: Option<(u32, Option<u64>)>,
    receipt: ActivationReceipt,
    restore_retry: Option<crate::runtime::surface_restorer::RestoreInput>,
    restore_failure: Option<String>,
    stage: Stage,
    public: bool,
    /// An engine's first workspace has no caller; its failure leaves the engine without one.
    first_workspace: bool,
    assembly_member: bool,
    transfers_existing: bool,
    one_shot_input: Option<String>,
    /// The public command whose stored result this creation completes. Its reply waits until the
    /// replaced owner is retired, as it did before publication was separated from that receipt.
    origin_command: Option<String>,
    queued: Option<Work>,
    /// One ticket carries every step, so a reply is matched to the only submitted request.
    in_flight: Option<InFlight>,
}

/// Upper bound for observing the retirement of a replaced owner after publication.
const SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// The submitted request whose reply this ticket awaits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InFlight {
    Preparation,
    Uncertainty,
    Step,
}

impl InFlight {
    fn of(work: &Work) -> Self {
        match work {
            Work::Prepared { .. } => Self::Preparation,
            Work::PreparationUncertain { .. } => Self::Uncertainty,
            _ => Self::Step,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Publication {
    Pending,
    Applied,
}

enum Stage {
    Admit,
    Reserve,
    Input(Vec<tasty_event_store::IdRange>),
    Commit,
    Claim,
    Preparing(preparing::Preparing),
    Prepared {
        operation: OperationId,
        acknowledged: bool,
    },
    Installing {
        installed: Installed,
        answered: bool,
    },
    Uncertain(Uncertain),
    Finish {
        installed: Installed,
        publication: Publication,
    },
    AwaitPublication(Installed),
    Published(Installed),
    /// The structure is published; the child CR and the replaced owner's receipts remain.
    Settling {
        installed: Installed,
        deadline: std::time::Instant,
        input_failure: Option<String>,
    },
    /// `unconfirmed` keeps why the input or the replaced owner's retirement did not finish.
    Settled {
        unconfirmed: Option<String>,
    },
    Rejected {
        discard_committed: bool,
        lease: crate::runtime::journal_product::EffectLease,
        reason: String,
        retirement: Option<tasty_terminal::PtyRetirement>,
        answered: bool,
        started: std::time::Instant,
    },
    Failed(String),
    Transition,
}

impl Stage {
    fn is_published(&self) -> bool {
        matches!(
            self,
            Self::Finish {
                publication: Publication::Applied,
                ..
            } | Self::Published(_)
        )
    }
    fn mark_published(&mut self) {
        *self = match std::mem::replace(self, Self::Transition) {
            Self::Finish { installed, .. } => Self::Finish {
                installed,
                publication: Publication::Applied,
            },
            Self::AwaitPublication(installed) => Self::Published(installed),
            other => other,
        };
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::Admit => "Admit",
            Self::Reserve => "Reserve",
            Self::Input(_) => "Input",
            Self::Commit => "Commit",
            Self::Claim => "Claim",
            Self::Preparing(_) => "Preparing",
            Self::Prepared { .. } => "Prepared",
            Self::Installing { .. } => "Installing",
            Self::Uncertain(_) => "Uncertain",
            Self::Finish { .. } => "Finish",
            Self::AwaitPublication(_) => "AwaitPublication",
            Self::Published(_) => "Published",
            Self::Settling { .. } => "Settling",
            Self::Settled { .. } => "Settled",
            Self::Rejected { .. } => "Rejected",
            Self::Failed(_) => "Failed",
            Self::Transition => "Transition",
        }
    }
}

impl Creation {
    pub(super) fn set_one_shot_input(&mut self, input: Option<String>) {
        self.one_shot_input = input;
    }
    pub(super) fn committed(
        ticket: u64,
        binding: crate::runtime::journal_product::EngineBinding,
        worker: &JournalWorker,
        operation: OperationId,
    ) -> Result<Self, String> {
        let mut creation = Self {
            ticket,
            binding,
            public: true,
            first_workspace: false,
            assembly_member: false,
            transfers_existing: false,
            one_shot_input: None,
            origin_command: None,
            stage: Stage::Claim,
            fixed_plan: None,
            requested_restore: None,
            receipt: Default::default(),
            restore_retry: None,
            restore_failure: None,
            queued: None,
            in_flight: None,
            input: PreparationInput {
                adopt: None,
                child: None,
                kind: String::new(),
                cwd: None,
                params: serde_json::Value::Null,
                restore: None,
                discard_capture: false,
                shell: None,
            },
        };
        creation.submit(
            worker,
            Work::ClaimPreparation {
                stream: creation.binding.stream.clone(),
                operation,
            },
        )?;
        Ok(creation)
    }

    pub(super) fn default_workspace(
        ticket: u64,
        session: &EngineSession,
        worker: &JournalWorker,
    ) -> Result<Self, String> {
        let shell = crate::core::state::ShellConfig::from_settings(&session.runtime.settings);
        let binding = session
            .journal_binding
            .clone()
            .ok_or("default workspace has no engine binding")?;
        worker
            .submit(Request {
                ticket,
                work: Work::Admit(Admission {
                    key: None,
                    original_digest: b"bootstrap.default_workspace".to_vec(),
                    actor: "system".into(),
                    origin: "bootstrap".into(),
                    causation_id: None,
                }),
            })
            .map_err(|error| format!("default workspace admission: {error:?}"))?;
        Ok(Self {
            ticket,
            binding,
            stage: Stage::Admit,
            public: false,
            first_workspace: true,
            assembly_member: false,
            transfers_existing: false,
            one_shot_input: None,
            origin_command: None,
            queued: None,
            in_flight: Some(InFlight::Step),
            fixed_plan: None,
            requested_restore: None,
            receipt: Default::default(),
            restore_retry: None,
            restore_failure: None,
            input: PreparationInput {
                adopt: None,
                child: None,
                kind: "terminal".into(),
                cwd: None,
                params: serde_json::json!({}),
                restore: None,
                discard_capture: false,
                shell: Some(ShellRecipe {
                    executable: shell.shell,
                    arguments: shell.args,
                    environment: shell.envs,
                    cols: session.runtime.default_cols,
                    rows: session.runtime.default_rows,
                    scrollback_lines: session.runtime.settings.general.scrollback_lines,
                    disk_scrollback: session.runtime.settings.performance.scrollback_disk_swap,
                    disk_scrollback_lines: session
                        .runtime
                        .settings
                        .performance
                        .scrollback_disk_cap(),
                    startup_command: session.runtime.settings.general.startup_command.clone(),
                    restore_command: None,
                }),
            },
        })
    }

    pub(super) fn restore(
        ticket: u64,
        session: &EngineSession,
        _worker: &JournalWorker,
        request: crate::runtime::surface_restorer::RestoreInput,
    ) -> Result<Self, String> {
        let binding = session
            .journal_binding
            .clone()
            .ok_or("restoring engine has no binding")?;
        let admission = Work::Admit(Admission {
            key: None,
            original_digest: serde_json::to_vec(&(
                "restore",
                &binding.stream,
                request.surface_id,
                &request.plan,
            ))
            .map_err(|error| error.to_string())?,
            actor: "system".into(),
            origin: "selected-restore".into(),
            causation_id: None,
        });
        Ok(Self {
            ticket,
            binding,
            restore_retry: Some(request.clone()),
            restore_failure: None,
            input: request.input,
            requested_restore: match &request.plan.destination {
                CreationDestination::Restore {
                    surface,
                    previous_activation,
                } => Some((*surface, *previous_activation)),
                _ => None,
            },
            fixed_plan: Some(request.plan),
            receipt: Default::default(),
            stage: Stage::Admit,
            public: false,
            first_workspace: false,
            assembly_member: false,
            transfers_existing: false,
            one_shot_input: None,
            origin_command: None,
            queued: Some(admission),
            in_flight: None,
        })
    }

    fn submit(&mut self, _worker: &JournalWorker, work: Work) -> Result<(), String> {
        self.enqueue(work)
    }

    /// Queued work is sent by `flush` only after the in-flight reply of this ticket arrives.
    fn enqueue(&mut self, work: Work) -> Result<(), String> {
        if self.queued.is_some() {
            return Err("materialization already has a pending worker step".into());
        }
        self.queued = Some(work);
        Ok(())
    }

    /// Returns false while a request is in flight or the worker queue is full.
    fn flush(&mut self, worker: &JournalWorker) -> Result<bool, String> {
        if self.in_flight.is_some() {
            return Ok(false);
        }
        let Some(work) = self.queued.take() else {
            return Ok(true);
        };
        let in_flight = InFlight::of(&work);
        match worker.submit_owned(Request {
            ticket: self.ticket,
            work,
        }) {
            Ok(()) => {
                self.in_flight = Some(in_flight);
                Ok(true)
            }
            Err((crate::runtime::journal_product::SubmitError::Busy, request)) => {
                self.queued = Some(request.work);
                Ok(false)
            }
            Err((error, _)) => Err(format!("materialization submission: {error:?}")),
        }
    }

    pub(super) fn answered(
        &mut self,
        worker: &JournalWorker,
        session: &mut EngineSession,
        spawner: &effect_runner::TerminalSpawner,
        value: ResultValue,
    ) -> Result<bool, String> {
        let value = match value {
            ResultValue::Stored(record) if !matches!(self.stage, Stage::Admit) => {
                ResultValue::Executed(crate::runtime::command_executor::Executed {
                    command_id: record.command_id,
                    status: record.status,
                    response: record.response,
                    source: crate::runtime::command_executor::Source::Stored,
                })
            }
            other => other,
        };
        let in_flight = self.in_flight.take();
        let preparation = in_flight == Some(InFlight::Preparation);
        let stage = std::mem::replace(&mut self.stage, Stage::Transition);
        self.stage = match (stage, value) {
            (Stage::Admit, ResultValue::NeedsResolution) if self.fixed_plan.is_some() => {
                self.submit(worker, Work::PutPreparation(self.input.clone()))?;
                Stage::Input(Vec::new())
            }
            (Stage::Admit, ResultValue::NeedsResolution) => {
                self.submit(
                    worker,
                    Work::Reserve(vec![
                        (IdKind::Workspace, 1),
                        (IdKind::Pane, 1),
                        (IdKind::Tab, 1),
                        (IdKind::Surface, 1),
                    ]),
                )?;
                Stage::Reserve
            }
            (Stage::Reserve, ResultValue::Reserved(ids)) => {
                self.submit(worker, Work::PutPreparation(self.input.clone()))?;
                Stage::Input(ids)
            }
            (Stage::Input(ids), ResultValue::InputStored(input)) => {
                let id = |kind: IdKind| -> Result<u32, String> {
                    let range = ids
                        .iter()
                        .find(|range| range.kind == kind.label())
                        .ok_or("missing reserved identity")?;
                    u32::try_from(range.start).map_err(|error| error.to_string())
                };
                let plan = if let Some(plan) = &self.fixed_plan {
                    plan.clone()
                } else {
                    CreationPlan {
                        destination: CreationDestination::Workspace {
                            workspace: id(IdKind::Workspace)?,
                            pane: id(IdKind::Pane)?,
                            tab: id(IdKind::Tab)?,
                            name: "Workspace 1".into(),
                            category: 0,
                            subtitle: String::new(),
                            description: String::new(),
                        },
                        surface: SurfaceSpec {
                            id: id(IdKind::Surface)?,
                            kind: "terminal".into(),
                            data: None,
                        },
                        tab_name: "Shell".into(),
                        explicit_name: None,
                    }
                };
                self.submit(
                    worker,
                    Work::Resolve {
                        changes: vec![StreamCommand {
                            stream: self.binding.stream.clone(),
                            command: StructuralCommand::PrepareCreation {
                                operation: OperationId(String::new()),
                                command_id: String::new(),
                                input,
                                plan,
                            },
                        }],
                        response: None,
                    },
                )?;
                Stage::Commit
            }
            (Stage::Commit, ResultValue::Executed(executed)) => {
                let progress: Vec<StructuralResult> = serde_json::from_slice(
                    executed
                        .response
                        .as_deref()
                        .ok_or("creation progress missing")?,
                )
                .map_err(|error| error.to_string())?;
                let [StructuralResult::Pending { operation }] = &progress[..] else {
                    return Err("creation returned no pending operation".into());
                };
                self.submit(
                    worker,
                    Work::ClaimPreparation {
                        stream: self.binding.stream.clone(),
                        operation: operation.clone(),
                    },
                )?;
                Stage::Claim
            }
            (Stage::Claim, ResultValue::Claimed(claimed)) => {
                self.requested_restore = match &claimed.plan.destination {
                    CreationDestination::Restore {
                        surface,
                        previous_activation,
                    } => Some((*surface, *previous_activation)),
                    _ => None,
                };
                self.transfers_existing = claimed.input.adopt.is_some();
                self.assembly_member = matches!(
                    claimed.plan.destination,
                    CreationDestination::Assembly { .. }
                );
                self.begin_preparation(worker, session, spawner, claimed)?
            }
            (Stage::Prepared { operation, .. }, ResultValue::Executed(_)) if preparation => {
                Stage::Prepared {
                    operation,
                    acknowledged: true,
                }
            }
            (Stage::Installing { installed, .. }, ResultValue::Executed(_)) if preparation => {
                Stage::Installing {
                    installed,
                    answered: true,
                }
            }
            // Publication can fail while the preparation reply is in flight; that reply only
            // releases the queued uncertainty record and must not acknowledge it.
            (Stage::Uncertain(uncertain), ResultValue::Executed(_) | ResultValue::Stored(_))
                if preparation =>
            {
                Stage::Uncertain(uncertain)
            }
            (
                Stage::Uncertain(mut uncertain),
                ResultValue::Executed(_) | ResultValue::Stored(_),
            ) if in_flight == Some(InFlight::Uncertainty) => {
                uncertain.acknowledge();
                let leaves = uncertain.has_no_evidence();
                self.stage = Stage::Uncertain(uncertain);
                if leaves {
                    // Nothing can reconcile it; polling again would only repeat the same check.
                    self.leave_uncertain(session);
                }
                return Ok(false);
            }
            (
                Stage::Finish {
                    installed,
                    publication: Publication::Pending,
                },
                ResultValue::Executed(_),
            ) if self.assembly_member && in_flight == Some(InFlight::Step) => {
                Stage::AwaitPublication(installed)
            }
            (Stage::Finish { installed, .. }, ResultValue::Executed(_))
                if in_flight == Some(InFlight::Step) =>
            {
                match Self::settle(installed) {
                    Some(stage) => stage,
                    None => return Ok(true),
                }
            }
            (
                Stage::Rejected {
                    lease,
                    reason,
                    retirement,
                    started,
                    discard_committed,
                    ..
                },
                ResultValue::Executed(_),
            ) if preparation => Stage::Rejected {
                discard_committed,
                lease,
                reason,
                retirement,
                answered: true,
                started,
            },
            // The first workspace's failure ends here; its engine stays ready without one.
            (Stage::Failed(reason), _) if self.public || self.first_workspace => {
                // OnceLock rejection means the original observer already has its terminal result.
                self.receipt
                    .get_or_init(|| ActivationOutcome::Failed(reason));
                return Ok(true);
            }
            (Stage::Claim, ResultValue::Executed(_)) if self.public => return Ok(true),
            (Stage::Failed(reason), _) if self.restore_retry.is_some() => {
                self.restore_failure = Some(reason.clone());
                // OnceLock rejection means the original observer already has its terminal result.
                self.receipt
                    .get_or_init(|| ActivationOutcome::Failed(reason));
                return Ok(true);
            }
            (Stage::Failed(reason), _) => return Err(reason),
            (stage, value) => {
                return Err(format!(
                    "materialization completion arrived outside its request phase (stage={}, result={}, in_flight={in_flight:?})",
                    stage.kind(),
                    value.kind()
                ));
            }
        };
        Ok(false)
    }

    pub(super) fn authorize_installation(
        &mut self,
        session: &mut EngineSession,
        events: &[tasty_core::RecordedEvent],
    ) -> Result<Option<effect_runner::Installation>, String> {
        let Stage::Prepared {
            operation,
            acknowledged,
        } = &self.stage
        else {
            return Ok(None);
        };
        let preparation_acked = *acknowledged;
        if events.iter().any(|recorded|matches!(&recorded.event,tasty_core::DomainEvent::OperationAwaitingCleanup {id,cleanup:tasty_core::CleanupPlan::DiscardPrepared {..},..} if id==operation)) {
            let candidate=session.pending_materializations.remove(operation).ok_or("discarded assembly member lost its private owner")?;
            let lease=candidate.lease.clone();let retirement=match self.discard_candidate(session,candidate)? {Some(receipt)=>receipt,None=>return Ok(None)};
            self.stage=Stage::Rejected {discard_committed:true,lease,reason:"assembly cancelled before installation".into(),retirement,answered:preparation_acked,started:std::time::Instant::now()};
            return Ok(None);
        }
        if !events.iter().any(|recorded|matches!(&recorded.event,
            tasty_core::DomainEvent::OperationAwaitingCleanup { id, cleanup:tasty_core::CleanupPlan::InstallPrepared { .. }, .. } if id==operation)) { return Ok(None); }
        let operation = operation.clone();
        let result = session
            .pending_materializations
            .get_mut(&operation)
            .ok_or("prepared operation lost its resource owner")?
            .begin_installation(&session.runtime.surface_registry);
        match result {
            Ok(installation) => Ok(Some(installation)),
            Err(error) => {
                let candidate = session
                    .pending_materializations
                    .remove(&operation)
                    .expect("checked candidate");
                let lease = candidate.lease.clone();
                let retirement = match self.discard_candidate(session, candidate)? {
                    Some(receipt) => receipt,
                    None => return Ok(None),
                };
                self.stage = Stage::Rejected {
                    discard_committed: false,
                    lease,
                    reason: error.to_string(),
                    retirement,
                    answered: preparation_acked,
                    started: std::time::Instant::now(),
                };
                Ok(None)
            }
        }
    }

    fn discard_candidate(
        &mut self,
        session: &mut EngineSession,
        candidate: effect_runner::PreparedMaterialization,
    ) -> Result<Option<Option<tasty_terminal::PtyRetirement>>, String> {
        match candidate.discard(&mut session.borrow_mut()) {
            Ok(receipt) => Ok(Some(receipt)),
            Err((candidate, reason)) => {
                let lease = candidate.lease.clone();
                uncertain::log_uncertain(&lease, "discard", &reason);
                // The stage keeps the owner even if the record cannot be queued. The session's
                // pending candidates do not, so the engine release never waits for it.
                self.stage = Stage::Uncertain(Uncertain::pending(
                    reason.clone(),
                    UncertainOwner::Unreturned(Box::new(candidate)),
                ));
                self.enqueue(Work::PreparationUncertain { lease, reason })?;
                Ok(None)
            }
        }
    }

    pub(super) fn leaf_for_publication(
        &mut self,
        session: &mut EngineSession,
        events: &[tasty_core::RecordedEvent],
    ) -> Result<Option<effect_runner::PreparedLeaf>, String> {
        let installed = match &self.stage {
            Stage::Finish { installed, .. } | Stage::AwaitPublication(installed) => installed,
            _ => return Ok(None),
        };
        let surface = session
            .pending_materializations
            .get(&installed.lease.operation)
            .and_then(|prepared| prepared.surface_id())
            .ok_or("installed leaf has no original candidate")?;
        if !events.iter().any(|recorded|matches!(&recorded.event,
            tasty_core::DomainEvent::SurfaceActivationChanged {id,activation,..} if *id==surface && activation.generation==installed.lease.resource_generation)) { return Ok(None); }
        let prepared = session
            .pending_materializations
            .remove(&installed.lease.operation)
            .ok_or("installed operation lost its kind owner")?;
        let leaf = prepared
            .into_leaf(installed)
            .map_err(|error| error.to_string())?;
        self.stage.mark_published();
        Ok(Some(leaf))
    }

    /// Why an engine's first workspace ended without being published, once it has ended.
    #[cfg(feature = "gui")]
    pub(super) fn first_workspace_failure(&self) -> Option<String> {
        match self.receipt.get() {
            Some(ActivationOutcome::Failed(reason)) if self.first_workspace => Some(reason.clone()),
            _ => None,
        }
    }

    pub(super) fn failed_restore(
        &mut self,
        session: &mut EngineSession,
    ) -> Option<crate::runtime::surface_restorer::RestoreInput> {
        let reason = self.restore_failure.take()?;
        let request = self.restore_retry.take()?;
        let placeholder = session
            .runtime
            .surfaces
            .get_mut(&request.surface_id)?
            .as_any_mut()
            .downcast_mut::<crate::runtime::surface_restorer::JournalPlaceholder>()?;
        placeholder.attempts = placeholder.attempts.saturating_add(1);
        placeholder.failure = Some(reason);
        // Retain the old content/seed, but stop automatic retries at the legacy terminal cap.
        (placeholder.attempts < crate::runtime::surface_restorer::MAX_ACTIVATION_ATTEMPTS)
            .then_some(request)
    }
    pub(super) fn join_restore(
        &self,
        surface: u32,
        activation: Option<u64>,
    ) -> Option<ActivationReceipt> {
        (self.requested_restore == Some((surface, activation))).then(|| self.receipt.clone())
    }
    pub(super) fn acknowledge_publication(&self, session: &EngineSession) {
        if !self.stage.is_published() {
            return;
        }
        let Some((surface, _)) = self.requested_restore else {
            return;
        };
        let activation = session
            .core_state
            .find_surface_by_id(surface)
            .and_then(|surface| surface.activation_generation);
        let physical = session.runtime.terminals.generation(surface);
        // Publication can be observed again; preserve the first exact activation receipt.
        self.receipt.get_or_init(|| ActivationOutcome::Ready {
            activation,
            physical,
        });
    }
    pub(super) fn retain_for_release(
        mut self,
        release: &mut crate::runtime::resource_retirement::EngineRelease,
    ) {
        match std::mem::replace(&mut self.stage, Stage::Transition) {
            Stage::Installing { installed, .. }
            | Stage::Finish { installed, .. }
            | Stage::AwaitPublication(installed)
            | Stage::Published(installed)
            | Stage::Settling { installed, .. } => release.retain_installation(installed),
            Stage::Rejected {
                retirement: Some(receipt),
                ..
            } => release.retain_pty(receipt),
            Stage::Uncertain(uncertain) => uncertain.retain_for_release(release),
            Stage::Preparing(preparing) => preparing.retain_for_release(release),
            _ => {}
        }
    }

    fn installed(&mut self, installed: Installed) {
        let preparation_acked = matches!(
            self.stage,
            Stage::Prepared {
                acknowledged: true,
                ..
            }
        );
        self.stage = Stage::Installing {
            installed,
            answered: preparation_acked,
        };
    }

    #[cfg(feature = "gui")]
    pub(super) fn input_generation(
        &self,
        surface: u32,
    ) -> Option<Option<tasty_terminal::ResourceGeneration>> {
        match &self.stage {
            Stage::Installing { installed, .. }
            | Stage::Finish { installed, .. }
            | Stage::AwaitPublication(installed)
            | Stage::Published(installed)
                if installed.surface_id == surface =>
            {
                Some(installed.previous_resource)
            }
            _ => None,
        }
    }

    /// Only an installed owner that the published structure does not yet describe pauses
    /// observation. Published, settling, rejected and withdrawn owners leave the live state
    /// consistent with the structure, so their receipts are observed without a pause.
    pub(super) fn pauses_observation(&self) -> bool {
        (self.transfers_existing
            && matches!(self.stage, Stage::Prepared { .. } | Stage::Rejected { .. }))
            || matches!(
                self.stage,
                Stage::Installing { .. }
                    | Stage::Finish {
                        publication: Publication::Pending,
                        ..
                    }
                    | Stage::AwaitPublication(_)
            )
            || matches!(&self.stage, Stage::Uncertain(uncertain) if uncertain.holds_unpublished_installation())
    }

    pub(super) fn needs_cleanup_poll(&self) -> bool {
        matches!(
            self.stage,
            Stage::Installing { answered: true, .. }
                | Stage::Rejected { answered: true, .. }
                | Stage::Settling { .. }
                | Stage::Preparing(preparing::Preparing::Discarding { .. })
        )
    }

    pub(super) fn poll_cleanup(
        &mut self,
        worker: &JournalWorker,
        view: crate::runtime::journal_product::CompletionView,
        session: &mut EngineSession,
    ) -> Result<(), String> {
        if !self.flush(worker)? {
            return Ok(());
        }
        if matches!(self.stage, Stage::Uncertain(_)) {
            self.poll_reconciliation(worker, view, session)?;
            return Ok(());
        }
        if let Stage::Rejected {
            lease,
            reason,
            retirement,
            answered: true,
            started,
            discard_committed,
        } = &self.stage
        {
            let phase = retirement
                .as_ref()
                .map(|receipt| receipt.observation().phase);
            let complete = matches!(phase, None | Some(tasty_terminal::PtyPhase::Reaped));
            if !complete
                && (phase == Some(tasty_terminal::PtyPhase::WaitFailed)
                    || started.elapsed() >= std::time::Duration::from_secs(5))
            {
                let reason = "private preparation owner could not confirm reap before its deadline"
                    .to_owned();
                uncertain::log_uncertain(lease, "rejected", &reason);
                self.submit(
                    worker,
                    Work::PreparationUncertain {
                        lease: lease.clone(),
                        reason: reason.clone(),
                    },
                )?;
                self.retain_uncertain(reason);
                self.flush(worker)?;
                return Ok(());
            }
            if complete {
                let reason = reason.clone();
                self.submit(
                    worker,
                    if *discard_committed {
                        Work::CleanupFinished {
                            lease: lease.clone(),
                            view,
                        }
                    } else {
                        Work::InstallationRejected {
                            lease: lease.clone(),
                            reason: reason.clone(),
                        }
                    },
                )?;
                self.stage = Stage::Failed(reason);
                self.flush(worker)?;
            }
            return Ok(());
        }
        if self.poll_settling(session) || self.poll_preparing(worker, session)? {
            return Ok(());
        }
        // The child body is already written; its CR follows publication in Settling.
        let ready = matches!(self.stage, Stage::Installing { answered: true, .. });
        if ready && let Stage::Installing { installed, .. } = &self.stage {
            self.submit(
                worker,
                Work::CleanupFinished {
                    lease: installed.lease.clone(),
                    view,
                },
            )?;
            let Stage::Installing { installed, .. } =
                std::mem::replace(&mut self.stage, Stage::Transition)
            else {
                unreachable!("checked installation phase")
            };
            self.stage = Stage::Finish {
                installed,
                publication: Publication::Pending,
            };
            self.flush(worker)?;
        }
        Ok(())
    }
}

impl Drop for Creation {
    fn drop(&mut self) {
        let reason = match &self.stage {
            Stage::Failed(reason) => reason.clone(),
            Stage::Uncertain(uncertain) => uncertain.reason.clone(),
            _ => "activation owner ended without a published resource receipt".into(),
        };
        // Drop must not overwrite an already published Ready or previously reported failure.
        self.receipt
            .get_or_init(|| ActivationOutcome::Failed(reason));
    }
}

#[cfg(test)]
mod tests;
