mod admission;
mod assembly;
mod binding;
mod capture;
#[cfg(test)]
mod credit_tests;
mod effects;
mod group;
mod manifest_rebase;
#[cfg(test)]
mod manifest_rebase_tests;
mod metrics;
#[cfg(all(test, unix))]
mod preservation_tests;
mod recovery;
mod resolve;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use tasty_event_store::CommandLookup;

use super::decider::{ResolvedCommand, StructureDecider};
use super::{Admission, Completion, JournalError, QUEUE_CAPACITY, ResultValue, Work, identity};
use crate::runtime::command_executor::{Executor, Request as ExecuteRequest};
use crate::runtime::journal;

struct Pending {
    disk_credit: u64,
    admission: Admission,
    followers: Vec<u64>,
    reservations: Vec<tasty_event_store::IdRange>,
    inputs: Vec<tasty_core::DataRef>,
}

const MAX_ADMISSION_IDS: u64 = 16_384;
/// 복원하지 못한 surface payload의 보유자. 자동으로 풀지 않으므로 원본은 저장소에 남는다.
pub(crate) const RESTORE_FAILURE_HOLDER: &str = "restore-failure";
impl Pending {
    fn check_ids(&self, additional: u64) -> Result<(), String> {
        let held = self
            .reservations
            .iter()
            .map(tasty_event_store::IdRange::len)
            .sum::<u64>();
        if held
            .checked_add(additional)
            .is_none_or(|total| total > MAX_ADMISSION_IDS)
        {
            return Err("admission ID reservation capacity exhausted".into());
        }
        Ok(())
    }
}

type Acknowledgements = mpsc::Receiver<(u64, Result<(), String>)>;

#[allow(clippy::too_many_arguments)] // reason: 스레드 생성 1회에 소유권째로 넘기는 자원 묶음이다
pub(super) fn run(
    home: PathBuf,
    writer_lock: Option<tasty_event_store::WriterLock>,
    readers: Arc<crate::runtime::journal_payload::PayloadReaders>,
    observed: Arc<super::JournalMetrics>,
    requests: mpsc::Receiver<super::QueuedRequest>,
    completions: mpsc::SyncSender<Completion>,
    acknowledgements: Acknowledgements,
    closed: Arc<AtomicBool>,
    skip_final_checkpoint: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
    #[cfg(test)] fail_next_publication: Arc<AtomicBool>,
    #[cfg(test)] muted: Arc<AtomicBool>,
    #[cfg(all(test, unix))] installs_sent: Arc<std::sync::Mutex<Vec<tasty_core::OperationId>>>,
) {
    let _observation_scope = super::observability::WorkerScope(observed.clone());
    let send = completion_sender(
        completions,
        wake,
        #[cfg(test)]
        muted,
        #[cfg(all(test, unix))]
        installs_sent,
    );
    let Some((executor, mut published)) =
        open_published_executor(&home, writer_lock, &acknowledgements, &send, &observed)
    else {
        return;
    };
    metrics::total(&executor, "opened");
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    // Admissions waiting for a preparation credit, in arrival order. They count toward QUEUE_CAPACITY.
    let mut waiting: VecDeque<(u64, Admission)> = VecDeque::new();
    let mut halted: Option<String> = None;
    let mut requests = group::Requests::new(requests);
    while let Some(queued) = requests.next() {
        observe_admissions(&observed, &pending, &waiting);
        if closed.load(Ordering::Acquire) {
            break;
        }
        let allow_resolve = executor.decider().unpreserved().is_empty();
        if halted.is_none() && group::eligible(&queued.request.work, allow_resolve) {
            let ready = requests.collect(queued, allow_resolve);
            if !group::process(
                &executor,
                &home,
                ready,
                &mut pending,
                &mut waiting,
                &mut published,
                &acknowledgements,
                &mut halted,
                &send,
                #[cfg(test)]
                &fail_next_publication,
            ) {
                return;
            }
            continue;
        }
        let observation = queued.observation;
        observation.start();
        let measurement =
            metrics::Measurement::begin(&executor, queued.submitted, &queued.request.work);
        let request = queued.request;
        drop(queued._bytes);
        let pending_before = pending.len();
        let waiting_before = waiting.len();
        let request = match admission::ordered(
            request,
            &pending,
            &mut waiting,
            halted.is_some(),
            &send,
            &observation,
        ) {
            admission::Order::Ready(request) => request,
            admission::Order::Deferred => {
                observation.finish(super::observability::Outcome::waiting(
                    waiting_before,
                    waiting.len(),
                ));
                observe_admissions(&observed, &pending, &waiting);
                continue;
            }
            admission::Order::Disconnected => return,
        };
        let was_halted = halted.is_some();
        let mut predecessor = capture_predecessor(&executor, &request.work, &mut halted);
        let mut followers = if matches!(request.work, Work::Resolve { .. } | Work::CancelAdmission)
        {
            pending
                .get(&request.ticket)
                .map(|p| p.followers.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let checkpoint_requested = match &request.work {
            Work::Capture { .. } => true,
            #[cfg(any(feature = "gui", test))]
            Work::RetireEngine(_) => true,
            _ => false,
        };
        let mut release_admission = matches!(
            request.work,
            Work::Resolve { .. } | Work::CancelAdmission | Work::Capture { .. }
        );
        let is_admit = matches!(request.work, Work::Admit(_));
        let mut deferred = None;
        let mut result = match &halted {
            Some(reason) => Err(JournalError::Halted(reason.clone())),
            None => handle(
                &executor,
                &home,
                &mut pending,
                request.ticket,
                request.work,
                &mut deferred,
            ),
        };
        observation.execution_finished();
        if let Some(admission) = deferred {
            let waiting_before = waiting.len();
            if !wait_for_credit(
                &pending,
                &mut waiting,
                request.ticket,
                admission,
                &send,
                &observation,
            ) {
                return;
            }
            observation.finish(super::observability::Outcome::waiting(
                waiting_before,
                waiting.len(),
            ));
            observe_admissions(&observed, &pending, &waiting);
            continue;
        }
        release_finished_admission(
            &executor,
            &mut pending,
            request.ticket,
            result.is_err() && !is_admit,
            &mut followers,
            &mut release_admission,
        );
        observe_admissions(&observed, &pending, &waiting);
        measurement.durable(&executor, request.ticket, result.is_ok());
        if halted.is_none()
            && let Err(error) = publish(
                &executor,
                &observed,
                &mut published,
                &mut predecessor,
                &acknowledgements,
                match &result {
                    Ok(ResultValue::Bound(bound)) => Some(bound.binding.clone()),
                    _ => None,
                },
                None,
                &send,
                #[cfg(test)]
                &fail_next_publication,
            )
        {
            observed.publication_error();
            halted = Some(error.clone());
            result = Err(JournalError::Halted(error));
        }
        if !was_halted && halted.is_some() {
            release_halted_admissions(&executor, &mut pending);
        }
        if checkpoint_requested && halted.is_none() && result.is_ok() {
            checkpoint_published(&executor, published, &readers);
        }
        if !was_halted
            && let Some(reason) = &halted
            && !send(Completion::Halted(reason.clone()))
        {
            return;
        }
        observation.finish(super::observability::Outcome::handled(result.is_ok()));
        measurement.finished(&executor, request.ticket, result.is_ok());
        for ticket in std::iter::once(request.ticket).chain(followers) {
            if !send(Completion::Finished {
                ticket,
                result: result.clone(),
            }) {
                return;
            }
        }
        let credit_released = pending.len() < pending_before;
        if !process_waiting(
            &executor,
            &home,
            &mut pending,
            &mut waiting,
            halted.as_deref(),
            credit_released,
            &send,
        ) {
            return;
        }
        observe_admissions(&observed, &pending, &waiting);
    }
    if halted.is_none() && !skip_final_checkpoint.load(Ordering::Acquire) {
        checkpoint_published(&executor, published, &readers);
    }
    observed.admissions(0, waiting.len(), 0);
    metrics::total(&executor, "stopped");
    metrics::bounded_snapshot(&observed);
}

fn capture_predecessor(
    executor: &Executor<StructureDecider>,
    work: &Work,
    halted: &mut Option<String>,
) -> Option<tasty_core::StructureModels> {
    let publishes = match work {
        Work::ReconcilePreparation { .. }
        | Work::ReconcileRetirement { .. }
        | Work::Capture { .. }
        | Work::RetirementFinished { .. }
        | Work::OpenEngine { .. }
        | Work::Resolve { .. }
        | Work::Prepared { .. }
        | Work::CleanupFinished { .. }
        | Work::InstallationRejected { .. }
        | Work::PreparationUncertain { .. }
        | Work::ClaimPreparation { .. } => true,
        #[cfg(any(feature = "gui", test))]
        Work::RetireEngine(_) => true,
        #[cfg(feature = "gui")]
        Work::ForwardFinished { .. } => true,
        _ => false,
    };
    if publishes && halted.is_none() {
        match executor.with_state(|models| binding::publication_predecessor(models, work)) {
            Ok(models) => Some(models),
            Err(error) => {
                *halted = Some(error.to_string());
                None
            }
        }
    } else {
        None
    }
}

/// Complete halted waiters or retry them in arrival order after a credit is released.
fn process_waiting(
    executor: &Executor<StructureDecider>,
    home: &std::path::Path,
    pending: &mut HashMap<u64, Pending>,
    waiting: &mut VecDeque<(u64, Admission)>,
    halted: Option<&str>,
    credit_released: bool,
    send: &impl Fn(Completion) -> bool,
) -> bool {
    if let Some(reason) = halted {
        for (ticket, _) in waiting.drain(..) {
            if !send(Completion::Finished {
                ticket,
                result: Err(JournalError::Halted(reason.to_owned())),
            }) {
                return false;
            }
        }
    } else if credit_released {
        while let Some((ticket, admission)) = waiting.pop_front() {
            let mut deferred = None;
            let result = handle(
                executor,
                home,
                pending,
                ticket,
                Work::Admit(admission),
                &mut deferred,
            );
            if let Some(admission) = deferred {
                waiting.push_front((ticket, admission));
                break;
            }
            if !send(Completion::Finished { ticket, result }) {
                return false;
            }
        }
    }
    true
}

/// Queue an admission until a credit returns. The wait shares the admission count limit, so a
/// full queue still answers immediately. Returns false when the App side has gone away.
fn wait_for_credit(
    pending: &HashMap<u64, Pending>,
    waiting: &mut VecDeque<(u64, Admission)>,
    ticket: u64,
    admission: Admission,
    send: &impl Fn(Completion) -> bool,
    observation: &super::observability::Observation,
) -> bool {
    let admitted: usize = pending.values().map(|p| 1 + p.followers.len()).sum();
    if admitted + waiting.len() >= QUEUE_CAPACITY {
        // The rejection is determined; Finished delivery may block or disconnect.
        observation.finish(super::observability::Outcome::Error);
        return send(Completion::Finished {
            ticket,
            result: Err(JournalError::QueueFull(
                "journal admission capacity exhausted",
            )),
        });
    }
    waiting.push_back((ticket, admission));
    true
}

/// Maintenance cannot change an already committed command response. The leaf atomically replaces
/// snapshot/live pins and preserves the previous checkpoint on failure; retry at the next capture,
/// retirement or graceful worker stop, never by waking an unbounded maintenance loop.
fn checkpoint_published(
    executor: &Executor<StructureDecider>,
    published: Option<u64>,
    readers: &crate::runtime::journal_payload::PayloadReaders,
) {
    match executor.with_state(|models| models.batch) {
        Ok(cut) if cut == published => {
            checkpoint_current(executor, readers);
        }
        Ok(_) => {
            tracing::warn!("structure checkpoint skipped: committed cut has not been published")
        }
        Err(error) => {
            tracing::warn!(%error,"structure checkpoint skipped: canonical state unavailable")
        }
    }
}

/// `deferred` receives an admission that must wait for a credit; its returned value is unused.
fn handle(
    executor: &Executor<StructureDecider>,
    home: &std::path::Path,
    pending: &mut HashMap<u64, Pending>,
    ticket: u64,
    work: Work,
    deferred: &mut Option<Admission>,
) -> Result<ResultValue, JournalError> {
    if drops_references(&work) {
        retry_unpreserved(executor).map_err(JournalError::Internal)?;
    }
    match work {
        Work::Admit(admission) => admission::admit(executor, pending, ticket, admission, deferred),
        work => handle_work(executor, home, pending, ticket, work).map_err(JournalError::Internal),
    }
}

/// 확정되면 surface의 자료 참조를 바꾸거나 없앨 수 있는 작업.
fn drops_references(work: &Work) -> bool {
    match work {
        Work::Capture { .. } | Work::CaptureClosed { .. } | Work::Resolve { .. } => true,
        #[cfg(any(feature = "gui", test))]
        Work::RetireEngine(_) => true,
        _ => false,
    }
}

/// 보존 pin을 걸지 못한 복원 실패 원본에 pin을 다시 건다. 남으면 `Err`다. 이 원본의 참조를 바꾸는
/// 작업보다 먼저 불러, 보존이 확정되기 전에 원 참조가 사라져 다음 실행의 GC가 원본을 지우지 않게 한다.
pub(super) fn retry_unpreserved(executor: &Executor<StructureDecider>) -> Result<(), String> {
    let remaining = executor.decider().unpreserved();
    if remaining.is_empty() {
        return Ok(());
    }
    let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
    let epoch = inner.epoch;
    for payload in remaining {
        match inner.store.pin_payload(
            epoch,
            tasty_event_store::PayloadRef(payload),
            RESTORE_FAILURE_HOLDER,
        ) {
            Ok(()) | Err(tasty_event_store::StoreError::PayloadMissing(_)) => {
                executor.decider().forget_unpreserved(payload)
            }
            Err(error) => {
                return Err(format!(
                    "restore failure payload {payload} is not preserved yet: {error}"
                ));
            }
        }
    }
    Ok(())
}

fn handle_work(
    executor: &Executor<StructureDecider>,
    home: &std::path::Path,
    pending: &mut HashMap<u64, Pending>,
    ticket: u64,
    work: Work,
) -> Result<ResultValue, String> {
    match work {
        Work::CapturePreset { draft } => capture::preset(executor, draft)
            .map(|(preset, base_name)| ResultValue::CapturedPreset { preset, base_name }),
        Work::ReconcilePreparation {
            lease,
            evidence,
            discarded,
            view,
        } => recovery::reconcile_preparation(executor, lease, evidence, discarded, view),
        Work::ReconcileRetirement { lease, evidence } => {
            recovery::reconcile_retirement(executor, lease, evidence)
        }
        Work::ReadCommand(command_id) => {
            executor
                .with_state(|_| ())
                .map_err(|error| error.to_string())?;
            let inner = executor.inner.lock().map_err(|error| error.to_string())?;
            let record = inner
                .store
                .command(&command_id)
                .map_err(|error| error.to_string())?
                .ok_or("original command missing")?;
            Ok(recovery::command_result(&inner.state, record, false))
        }
        #[cfg(any(feature = "gui", test))]
        Work::RetireEngine(binding) => binding::retire(executor, home, ticket, binding),
        Work::OpenEngine {
            selection,
            normal_category_name,
            surface_floor,
        } => binding::open_after_floor(
            executor,
            home,
            ticket,
            selection,
            normal_category_name,
            &surface_floor,
            pending.values().map(|entry| entry.disk_credit).sum(),
        ),
        Work::Admit(_) => unreachable!("admission is handled at the typed boundary"),
        Work::Resolve { changes, response } => {
            let admitted = pending
                .remove(&ticket)
                .ok_or("journal request was not admitted")?;
            let request = executor
                .with_state(|models| resolve::request(&admitted, changes, response, models))
                .map_err(|error| error.to_string())??;
            executor
                .execute(&request)
                .map(ResultValue::Executed)
                .map_err(|error| error.to_string())
        }
        Work::PrepareSubtree { binding, draft } => {
            let admitted = pending
                .get_mut(&ticket)
                .ok_or("preset input has no admitted owner")?;
            assembly::preset(executor, admitted, ticket, binding, draft)
        }
        Work::PrepareUndo {
            binding,
            target_pane,
            scope,
            shell,
        } => {
            let admitted = pending
                .get_mut(&ticket)
                .ok_or("undo input has no admitted owner")?;
            assembly::undo(
                executor,
                admitted,
                ticket,
                binding,
                target_pane,
                scope,
                shell,
            )
        }
        #[cfg(feature = "gui")]
        Work::ClaimForward { stream, operation } => {
            effects::claim_forward(executor, &stream, &operation)
        }
        #[cfg(feature = "gui")]
        Work::ForwardFinished { lease, outcome } => effects::forwarded(executor, lease, outcome),
        Work::Capture {
            binding,
            mut surfaces,
            restore,
            drop_uncaptured_scrollback,
        } => {
            // 복원 명령을 읽지 못하면 앞 capture를 남기고 dirty 상태로 다시 시도한다.
            restore.fill(&mut surfaces)?;
            capture::persist(
                executor,
                ticket,
                binding,
                surfaces,
                drop_uncaptured_scrollback,
            )
        }
        Work::CaptureClosed {
            view,
            binding,
            target,
            display_name,
            mut surfaces,
            restore,
        } => {
            // 닫기는 막지 않는다. 복원 명령을 읽지 못하면 그 값 없이 되돌리기 자료를 남긴다.
            if let Err(error) = restore.fill(&mut surfaces) {
                tracing::warn!("close capture kept no restore command: {error}");
            }
            let admitted = pending
                .get_mut(&ticket)
                .ok_or("close capture has no admitted request")?;
            let (input, undo) = capture::closed(
                executor,
                ticket,
                binding,
                target,
                display_name,
                surfaces,
                view,
            )?;
            admitted.inputs.push(input);
            if let Some(capture) = &undo {
                admitted.inputs.extend(capture.data_refs());
            }
            Ok(ResultValue::ClosedCaptured { input, undo })
        }
        Work::ReserveExecutionIds { binding, kinds } => {
            executor
                .with_state(|_| ())
                .map_err(|error| error.to_string())?;
            let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
            let epoch = inner.epoch;
            if binding.journal_id != inner.store.journal_id()
                || binding.runtime_epoch != epoch.0
                || inner
                    .state
                    .streams
                    .get(&binding.stream)
                    .is_none_or(|model| {
                        model.engine_retired || model.engine_incarnation != binding.incarnation
                    })
            {
                return Err("execution reservation belongs to a retired engine".into());
            }
            if kinds.len() > 4 {
                return Err("invalid execution reservation kind count".into());
            }
            let mut ranges = Vec::new();
            for (kind, count) in kinds {
                if kind == tasty_core::IdKind::Category || count == 0 {
                    return Err("execution reservation has invalid kind/count".into());
                }
                let max = if kind == tasty_core::IdKind::Surface {
                    0x7fff_ffff
                } else {
                    u32::MAX
                };
                ranges.push(
                    inner
                        .store
                        .reserve_ids(epoch, kind.label(), u64::from(count), u64::from(max))
                        .map_err(|error| error.to_string())?,
                );
            }
            Ok(ResultValue::ExecutionIds { binding, ranges })
        }
        Work::Reserve(kinds) => {
            if !pending.contains_key(&ticket) {
                return Err("ID reservation requires an admitted request".into());
            }
            if kinds.len() > 5 || kinds.iter().any(|(_, count)| *count == 0 || *count > 4096) {
                return Err("invalid structure ID reservation size".into());
            }
            pending
                .get(&ticket)
                .ok_or("ID reservation requires admission")?
                .check_ids(kinds.iter().map(|(_, count)| u64::from(*count)).sum())?;
            let mut inner = executor.inner.lock().map_err(|e| e.to_string())?;
            let epoch = inner.epoch;
            let mut ranges = Vec::new();
            for (kind, count) in kinds {
                let max = if kind == tasty_core::IdKind::Surface {
                    0x7fff_ffff
                } else {
                    u32::MAX
                };
                let range = inner
                    .store
                    .reserve_ids(epoch, kind.label(), u64::from(count), u64::from(max))
                    .map_err(|e| e.to_string())?;
                pending
                    .get_mut(&ticket)
                    .expect("admitted request exists")
                    .reservations
                    .push(range.clone());
                ranges.push(range);
            }
            Ok(ResultValue::Reserved(ranges))
        }
        #[cfg(feature = "gui")]
        Work::SaveView(view) => capture::save_view(executor, home, &view),
        Work::ReadPayload(reference) => read_payload(executor, reference),
        Work::PreserveFailedPayload(reference) => preserve_failed_payload(executor, reference),
        #[cfg(test)]
        Work::ReadEngine(stream) => executor
            .with_state(|models| ResultValue::Engine(models.stream(&stream)))
            .map_err(|e| e.to_string()),
        Work::PutPayload(bytes) => {
            let admitted = pending
                .get_mut(&ticket)
                .ok_or("payload has no admitted owner")?;
            let inner = &mut *executor.inner.lock().map_err(|error| error.to_string())?;
            let epoch = inner.epoch;
            let reference = inner
                .store
                .put_admission_payload_pinned(
                    epoch,
                    &bytes,
                    &format!("admission/{}/{ticket}", epoch.0),
                )
                .map_err(|error| error.to_string())?;
            let reference = tasty_core::DataRef(reference.0);
            admitted.inputs.push(reference);
            Ok(ResultValue::InputStored(reference))
        }
        Work::PutPreparation(input) => {
            let admitted = pending
                .get_mut(&ticket)
                .ok_or("preparation input requires admission")?;
            let bytes = serde_json::to_vec(&input).map_err(|error| error.to_string())?;
            let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
            let epoch = inner.epoch;
            let reference = inner
                .store
                .put_admission_payload_pinned(
                    epoch,
                    &bytes,
                    &format!("admission/{}/{ticket}", epoch.0),
                )
                .map_err(|error| error.to_string())?;
            let reference = tasty_core::DataRef(reference.0);
            admitted.inputs.push(reference);
            Ok(ResultValue::InputStored(reference))
        }
        Work::ClaimRetirement { stream, operation } => {
            effects::claim_retirement(executor, &stream, &operation)
        }
        Work::RetirementFinished { lease, outcome } => effects::retired(executor, lease, outcome),
        Work::ClaimPreparation { stream, operation } => {
            effects::claim(executor, &stream, &operation)
        }
        Work::Prepared { lease, result } => effects::prepared(executor, lease, result),
        Work::InstallationRejected { lease, reason } => effects::rejected(executor, lease, reason),
        Work::PreparationUncertain { lease, reason } => effects::uncertain(executor, lease, reason),
        Work::CleanupFinished { lease, view } => effects::cleaned(executor, lease, view),
        Work::CancelAdmission => {
            if pending.remove(&ticket).is_none() {
                for p in pending.values_mut() {
                    p.followers.retain(|f| *f != ticket);
                }
            }
            Ok(ResultValue::Cancelled)
        }
    }
}

/// 행이 없거나 checksum이 맞지 않는 payload는 그 surface의 손실로 돌려준다. 저장소는 계속 쓴다.
fn read_payload(
    executor: &Executor<StructureDecider>,
    reference: tasty_core::DataRef,
) -> Result<ResultValue, String> {
    let inner = executor.inner.lock().map_err(|error| error.to_string())?;
    match inner
        .store
        .read_payload(tasty_event_store::PayloadRef(reference.0))
    {
        Ok(bytes) => Ok(ResultValue::Payload { reference, bytes }),
        Err(
            error @ (tasty_event_store::StoreError::PayloadMissing(_)
            | tasty_event_store::StoreError::PayloadCorrupt(_)),
        ) => Ok(ResultValue::PayloadUnavailable {
            reason: error.to_string(),
        }),
        Err(error) => Err(error.to_string()),
    }
}

/// 행이 없는 payload는 보존할 바이트가 없다. 있는 행에 pin을 걸지 못하면 기억해 두고 checkpoint가
/// 다시 시도한다. 그 전까지 checkpoint는 압축·GC를 하지 않는다.
fn preserve_failed_payload(
    executor: &Executor<StructureDecider>,
    reference: tasty_core::DataRef,
) -> Result<ResultValue, String> {
    let mut inner = executor.inner.lock().map_err(|error| error.to_string())?;
    let epoch = inner.epoch;
    match inner.store.pin_payload(
        epoch,
        tasty_event_store::PayloadRef(reference.0),
        RESTORE_FAILURE_HOLDER,
    ) {
        Ok(()) => Ok(ResultValue::PayloadPreserved),
        Err(error @ tasty_event_store::StoreError::PayloadMissing(_)) => Err(error.to_string()),
        Err(error) => {
            executor.decider().remember_unpreserved(reference.0);
            Err(format!("{error}; retrying before the next checkpoint"))
        }
    }
}

#[allow(clippy::too_many_arguments)] // reason: 원 요청의 binding과 확정 cut을 기존 publication·ACK 자원과 함께 전달한다
fn publish(
    executor: &Executor<StructureDecider>,
    observed: &super::JournalMetrics,
    published: &mut Option<u64>,
    predecessor: &mut Option<tasty_core::StructureModels>,
    acks: &Acknowledgements,
    engine_binding: Option<super::EngineBinding>,
    through: Option<u64>,
    send: &impl Fn(Completion) -> bool,
    #[cfg(test)] fail_next_publication: &AtomicBool,
) -> Result<(), String> {
    let durable = executor.with_state(|state| state.batch).map_err(|e| {
        observed.publication_unknown();
        e.to_string()
    })?;
    observed.publication(durable, *published);
    loop {
        if through.is_some_and(|cut| published.is_some_and(|current| current >= cut)) {
            return Ok(());
        }
        let next = {
            let inner = executor.inner.lock().map_err(|e| e.to_string())?;
            inner
                .store
                .read_batches_after(*published, 1)
                .map_err(|e| e.to_string())?
                .pop()
        };
        let Some(batch) = next else {
            return if through.is_some() {
                Err("publication target batch is missing".into())
            } else {
                Ok(())
            };
        };
        if through.is_some_and(|cut| batch.cut.batch_id > cut) {
            return Err("publication skipped its logical request cut".into());
        }
        #[cfg(test)]
        if fail_next_publication.swap(false, Ordering::AcqRel) {
            return Err("injected failure after commit before publication".into());
        }
        let decoded = journal::stream_batch(&batch).map_err(|e| e.to_string())?;
        let mut previous = predecessor
            .take()
            .ok_or("committed batch has no live predecessor")?;
        let before = decoded
            .streams
            .keys()
            .map(|stream| (stream.clone(), previous.stream(stream)))
            .collect();
        tasty_core::evolve_streams(&mut previous, &decoded).map_err(|error| error.to_string())?;
        *predecessor = Some(previous);
        #[cfg(test)]
        let commit_statistics = executor
            .inner
            .lock()
            .map_err(|error| error.to_string())?
            .store
            .commit_statistics();
        if !send(Completion::Publish {
            #[cfg(test)]
            commit_statistics,
            engine_binding: engine_binding.clone(),
            batch: decoded,
            before,
        }) {
            return Err("application projection disconnected".into());
        }
        acknowledge(acks, batch.cut.batch_id)?;
        *published = Some(batch.cut.batch_id);
        observed.publication(durable, *published);
    }
}

fn acknowledge(acks: &Acknowledgements, expected: u64) -> Result<(), String> {
    let (acknowledged, applied) = acks
        .recv()
        .map_err(|_| "application projection disconnected")?;
    if acknowledged != expected {
        return Err("application acknowledged a different journal cut".into());
    }
    applied.map_err(|e| format!("committed projection halted: {e}"))
}

fn release_finished_admission(
    executor: &Executor<StructureDecider>,
    pending: &mut HashMap<u64, Pending>,
    ticket: u64,
    failed: bool,
    followers: &mut Vec<u64>,
    release_admission: &mut bool,
) {
    // A failed preparation is terminal for this unresolved admission. Release its credit
    // and notify joined callers even when App never sends a later CancelAdmission.
    if failed && let Some(abandoned) = pending.remove(&ticket) {
        for follower in abandoned.followers {
            if !followers.contains(&follower) {
                followers.push(follower);
            }
        }
        *release_admission = true;
    }
    if *release_admission {
        let mut inner = executor.inner.lock().expect("worker executor lock");
        let epoch = inner.epoch;
        if let Err(error) = inner.store.release_payload_holder(
            epoch,
            &format!("admission/{}/{ticket}", epoch.0, ticket = ticket),
        ) {
            // A leaked pin is conservative; it is reclaimed by a future fenced writer.
            tracing::warn!("admission payload pin release failed: {error}");
        }
    }
}

fn release_halted_admissions(
    executor: &Executor<StructureDecider>,
    pending: &mut HashMap<u64, Pending>,
) {
    // Publication failure ends all unresolved admissions. Their credits must not survive
    // the terminal transport failure; committed operations already have durable pins.
    let abandoned: Vec<_> = pending.drain().map(|(ticket, _)| ticket).collect();
    let mut inner = executor.inner.lock().expect("worker executor lock");
    let epoch = inner.epoch;
    for ticket in abandoned {
        if let Err(error) = inner
            .store
            .release_payload_holder(epoch, &format!("admission/{}/{ticket}", epoch.0))
        {
            tracing::warn!(%error,"halted admission pin remains for fenced-writer cleanup");
        }
    }
}

fn open_published_executor(
    home: &std::path::Path,
    writer_lock: Option<tasty_event_store::WriterLock>,
    acknowledgements: &Acknowledgements,
    send: &impl Fn(Completion) -> bool,
    observed: &Arc<super::JournalMetrics>,
) -> Option<(Executor<StructureDecider>, Option<u64>)> {
    let opened = identity::open(home, writer_lock).and_then(|store| {
        observed.attach_store(store.commit_metrics_reader());
        Executor::open(StructureDecider::default(), store).map_err(|error| match &error {
            crate::runtime::command_executor::ExecError::Store(store_error)
                if matches!(**store_error, tasty_event_store::StoreError::WriterLocked) =>
            {
                super::StartupFailure::HomeInUse(error.to_string())
            }
            _ => super::StartupFailure::Other(error.to_string()),
        })
    });
    let executor = match opened {
        Ok(executor) => executor,
        Err(error) => {
            send(Completion::StartupFailed(error));
            return None;
        }
    };
    observed.attach_scalar_retries(executor.scalar_retry_reader());
    {
        let mut inner = executor.inner.lock().expect("new executor lock");
        let epoch = inner.epoch;
        if let Err(error) = inner.store.release_abandoned_admission_holders(epoch) {
            send(Completion::StartupFailed(error.to_string().into()));
            return None;
        }
    }
    if let Err(error) = recovery::recover(&executor) {
        send(Completion::StartupFailed(error.into()));
        return None;
    }
    let published = {
        let inner = executor.inner.lock().expect("new executor lock");
        let cut = inner.state.batch;
        observed.publication(cut, None);
        if !send(Completion::Ready {
            #[cfg(test)]
            journal_id: inner.store.journal_id().to_owned(),
            runtime_epoch: inner.epoch.0,
            cut,
            bootstrap: inner.state.clone(),
            damage: executor.decider().report(),
        }) {
            return None;
        }
        cut
    };
    if let Err(error) = acknowledge(acknowledgements, published.unwrap_or(0)) {
        observed.publication_error();
        send(Completion::StartupFailed(error.into()));
        return None;
    }
    observed.publication(published, published);
    Some((executor, published))
}

fn checkpoint_current(
    executor: &Executor<StructureDecider>,
    readers: &crate::runtime::journal_payload::PayloadReaders,
) {
    if let Err(error) = capture::checkpoint(executor, readers) {
        tracing::warn!(%error,"structure checkpoint failed; retaining previous checkpoint");
    }
}

fn observe_admissions(
    observed: &super::JournalMetrics,
    pending: &HashMap<u64, Pending>,
    waiting: &VecDeque<(u64, Admission)>,
) {
    observed.admissions(
        pending.values().map(|p| 1 + p.followers.len()).sum(),
        waiting.len(),
        pending
            .values()
            .map(|p| p.admission.original_digest.len())
            .sum::<usize>()
            + waiting
                .iter()
                .map(|(_, a)| a.original_digest.len())
                .sum::<usize>(),
    );
}

fn completion_sender(
    completions: mpsc::SyncSender<Completion>,
    wake: Arc<dyn Fn() + Send + Sync>,
    #[cfg(test)] muted: Arc<AtomicBool>,
    #[cfg(all(test, unix))] installs_sent: Arc<std::sync::Mutex<Vec<tasty_core::OperationId>>>,
) -> impl Fn(Completion) -> bool {
    move |completion| {
        #[cfg(test)]
        if muted.load(Ordering::Acquire) {
            drop(completion);
            return true;
        }
        #[cfg(all(test, unix))]
        let installs = installations(&completion);
        if completions.send(completion).is_err() {
            return false;
        }
        #[cfg(all(test, unix))]
        if !installs.is_empty() {
            *installs_sent.lock().unwrap() = installs;
        }
        wake();
        true
    }
}

/// 설치 이벤트가 든 Publish의 설치 operation들.
#[cfg(all(test, unix))]
fn installations(completion: &Completion) -> Vec<tasty_core::OperationId> {
    let Completion::Publish { batch, .. } = completion else {
        return Vec::new();
    };
    batch
        .streams
        .values()
        .flatten()
        .filter_map(|recorded| match &recorded.event {
            tasty_core::DomainEvent::OperationAwaitingCleanup {
                id,
                cleanup: tasty_core::CleanupPlan::InstallPrepared { .. },
                ..
            } => Some(id.clone()),
            _ => None,
        })
        .collect()
}
