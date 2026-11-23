//! Nonblocking application handle for the data-home journal worker.
//!
//! SQLite and canonical models stay on the worker. A committed batch must be acknowledged by
//! the application projection before a successful response or another mutation is released.

mod binding;
mod decider;
mod error;
mod identity;
mod kind;
pub(crate) mod observability;
mod preparation;
mod response;
#[cfg(test)]
pub(crate) mod stall_budget;
#[cfg(test)]
pub(crate) mod thread_probe;
pub(crate) mod view_record;
mod worker;

pub(crate) use error::JournalError;
pub(crate) use identity::journal_database_path;
pub(crate) use observability::{JournalMetrics, JournalMetricsSnapshot};

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use tasty_core::{IdKind, JournalModel, StreamBatch, StructureModels};
use tasty_event_store::{CommandKey, CommandRecord, IdRange};

use super::command_executor::Executed;
pub(crate) use binding::{BindingIssue, BoundEngine, EngineBinding, EngineSelection};
pub(crate) use decider::StreamCommand;
pub(crate) use preparation::{
    AdoptRecipe, ChildRecipe, ClaimedPreparation, ClaimedRetirement, EffectLease, PreparationInput,
    ShellRecipe,
};
pub(crate) use response::{CompletionView, ResponsePlan, ResponseProgress};

const QUEUE_CAPACITY: usize = 64;
/// Maximum encoded input retained for one journal command.
pub(crate) const MAX_COMMAND_INPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum serialized preset result and label admitted by capture.
pub(crate) const MAX_PRESET_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const MAX_QUEUED_BYTES: usize = tasty_ipc::admission::QUEUED_BYTES_LIMIT;

#[derive(Debug, Clone)]
pub(crate) struct Admission {
    pub(crate) key: Option<CommandKey>,
    pub(crate) original_digest: Vec<u8>,
    pub(crate) actor: String,
    pub(crate) origin: String,
    pub(crate) causation_id: Option<String>,
}

/// engine을 열 때 memory.db에 남은 surface scope의 가장 큰 ID. 새 surface ID가 남은 metadata와
/// 겹치지 않게 그 위에서 시작한다. journal worker가 memory writer 순서로 읽으므로 메인 루프는
/// 저장소를 기다리지 않는다.
#[derive(Clone)]
pub(crate) enum SurfaceFloor {
    Lookup(tasty_memory::writer::MemoryAccess),
    #[cfg(test)]
    Fixed(u32),
}

impl std::fmt::Debug for SurfaceFloor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SurfaceFloor").finish()
    }
}

impl SurfaceFloor {
    pub(crate) fn of(session: &crate::runtime::engine_session::EngineSession) -> Self {
        Self::Lookup(session.runtime.memory_access.clone())
    }

    /// worker 스레드에서 writer 결과를 기다려 읽는다. 읽지 못하면 engine을 열지 않는다.
    pub(crate) fn read(&self) -> Result<u32, String> {
        let access = match self {
            Self::Lookup(access) => access,
            #[cfg(test)]
            Self::Fixed(floor) => return Ok(*floor),
        };
        let scopes = access
            .run_and_wait(
                tasty_memory::writer::MemoryRequest::read(0, |memory| memory.scopes()),
                std::time::Duration::from_secs(30),
            )
            .map_err(|why| format!("cannot establish existing surface ID floor: {why:?}"))?
            .into_value::<Vec<String>>()
            .ok_or("surface ID floor lookup returned another value")?;
        Ok(scopes
            .iter()
            .filter_map(|scope| match tasty_memory::Scope::parse(scope) {
                Ok(tasty_memory::Scope::Surface(id))
                    if crate::runtime::terminal_store::is_surface_id_space(id) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0))
    }
}

#[derive(Debug)]
pub(crate) enum Work {
    ReconcilePreparation {
        lease: EffectLease,
        evidence: Vec<u8>,
        discarded: Option<String>,
        view: CompletionView,
    },
    ReconcileRetirement {
        lease: EffectLease,
        evidence: Vec<u8>,
    },
    #[cfg(feature = "gui")]
    ClaimForward {
        stream: String,
        operation: tasty_core::OperationId,
    },
    #[cfg(feature = "gui")]
    ForwardFinished {
        lease: EffectLease,
        outcome: tasty_core::OperationOutcome,
    },
    PrepareSubtree {
        binding: EngineBinding,
        draft: crate::runtime::preset_plan::AssemblyDraft,
    },
    PrepareUndo {
        binding: EngineBinding,
        target_pane: Option<u32>,
        scope: Option<u32>,
        shell: ShellRecipe,
    },
    ReserveExecutionIds {
        binding: EngineBinding,
        kinds: Vec<(IdKind, u32)>,
    },
    CaptureClosed {
        view: CompletionView,
        binding: EngineBinding,
        target: tasty_core::CloseTarget,
        display_name: Option<String>,
        surfaces: Vec<crate::runtime::surface_capture::CapturedSurface>,
        restore: crate::runtime::surface_capture::RestoreLookup,
    },
    Capture {
        binding: EngineBinding,
        surfaces: Vec<crate::runtime::surface_capture::CapturedSurface>,
        restore: crate::runtime::surface_capture::RestoreLookup,
        /// 내용 복원이 꺼져 있다. 이번 capture에 없는 terminal(지연 placeholder 등)의
        /// 기존 payload에서도 scrollback을 뺀 새 payload를 기록한다.
        drop_uncaptured_scrollback: bool,
    },
    CapturePreset {
        draft: crate::intent::preset_capture::PresetCaptureDraft,
    },
    #[cfg(any(feature = "gui", test))]
    RetireEngine(EngineBinding),
    OpenEngine {
        selection: EngineSelection,
        normal_category_name: String,
        surface_floor: SurfaceFloor,
    },
    /// Current permissions are checked by App before this lookup. Targets are not resolved yet.
    Admit(Admission),
    Resolve {
        changes: Vec<StreamCommand>,
        response: Option<ResponsePlan>,
    },
    Reserve(Vec<(IdKind, u32)>),
    #[cfg(feature = "gui")]
    SaveView(view_record::StoredView),
    #[cfg(test)]
    ReadEngine(String),
    ReadCommand(String),
    ReadPayload(tasty_core::DataRef),
    /// 복원하지 못한 surface의 원본 payload를 다음 capture가 참조를 바꾼 뒤에도 남긴다.
    PreserveFailedPayload(tasty_core::DataRef),
    PutPreparation(PreparationInput),
    PutPayload(Vec<u8>),
    ClaimRetirement {
        stream: String,
        operation: tasty_core::OperationId,
    },
    RetirementFinished {
        lease: EffectLease,
        outcome: tasty_core::OperationOutcome,
    },
    ClaimPreparation {
        stream: String,
        operation: tasty_core::OperationId,
    },
    Prepared {
        lease: EffectLease,
        result: tasty_core::PreparationResult,
    },
    InstallationRejected {
        lease: EffectLease,
        reason: String,
    },
    CleanupFinished {
        lease: EffectLease,
        view: CompletionView,
    },
    PreparationUncertain {
        lease: EffectLease,
        reason: String,
    },
    CancelAdmission,
}

#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) ticket: u64,
    pub(crate) work: Work,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: Vec 에 모으지 않고 completions 채널(sync_channel, 용량 64)로만 흐른다 — 상한이 64 라 낭비가 묶이고 Box 는 completion 마다 할당을 더한다
pub(crate) enum ResultValue {
    CapturedPreset {
        preset: crate::intent::ClonedPreset,
        base_name: String,
    },
    RecoveryRequired {
        command_id: String,
        reason: String,
        replay: bool,
    },
    #[cfg(feature = "gui")]
    ForwardClaimed {
        lease: EffectLease,
        payload: Vec<u8>,
    },
    AssemblyResolved {
        stream: String,
        input: Option<tasty_core::DataRef>,
        plan: Option<tasty_core::CreationAssembly>,
    },
    #[cfg(feature = "gui")]
    ViewSaved,
    /// 손상 범위가 남아 있어 View를 저장하지 않았다. 저장은 domain snapshot을 함께 쓰므로 미룬다.
    #[cfg(feature = "gui")]
    ViewDeferred,
    Bound(BoundEngine),
    /// engine을 열기 전 확인에 실패해 아무것도 기록하지 않았다.
    OpeningRefused(String),
    NeedsResolution,
    JoinedAdmission {
        #[cfg(test)]
        leader_ticket: u64,
    },
    Stored(CommandRecord),
    Command(CommandRecord),
    Executed(Executed),
    Reserved(Vec<IdRange>),
    ExecutionIds {
        binding: EngineBinding,
        ranges: Vec<IdRange>,
    },
    #[cfg(test)]
    Engine(JournalModel),
    Payload {
        reference: tasty_core::DataRef,
        bytes: Vec<u8>,
    },
    /// payload 행이 없거나 checksum이 맞지 않는다. 저장소 자체는 계속 쓸 수 있다.
    PayloadUnavailable {
        reason: String,
    },
    PayloadPreserved,
    InputStored(tasty_core::DataRef),
    ClosedCaptured {
        input: tasty_core::DataRef,
        undo: Option<tasty_core::UndoCapture>,
    },
    Claimed(ClaimedPreparation),
    RetirementClaimed(ClaimedRetirement),
    Cancelled,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // reason: Vec 에 모으지 않고 completions 채널(sync_channel, 용량 64)로만 흐른다 — 상한이 64 라 낭비가 묶이고 Box 는 completion 마다 할당을 더한다
pub(crate) enum Completion {
    Ready {
        #[cfg(test)]
        journal_id: String,
        runtime_epoch: u64,
        cut: Option<u64>,
        bootstrap: StructureModels,
        /// 적재하지 못해 `bootstrap`에서 빠진 stream과 건너뛴 snapshot.
        damage: crate::runtime::journal::LoadReport,
    },
    StartupFailed(StartupFailure),
    /// Canonical/publication cut is no longer available. Every App reader and writer must halt.
    Halted(String),
    /// All affected local engines must apply this as one publication before acknowledging.
    Publish {
        #[cfg(test)]
        commit_statistics: tasty_event_store::CommitStatistics,
        /// Explicit bootstrap destination, allocated on the worker before this batch.
        engine_binding: Option<EngineBinding>,
        batch: StreamBatch,
        /// One-use predecessor for live preflight; App discards it after publication.
        before: std::collections::BTreeMap<String, JournalModel>,
    },
    Finished {
        ticket: u64,
        result: Result<ResultValue, JournalError>,
    },
}

/// 저널을 열지 못한 이유. 부팅 화면이 오류 문자열을 해석하지 않고 종류로 문구를 고른다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupFailure {
    /// 다른 프로세스가 같은 데이터 홈의 저널을 쓰거나 초기화하고 있다.
    HomeInUse(String),
    Other(String),
}

impl StartupFailure {
    #[cfg(any(test, feature = "gui"))]
    pub(crate) fn is_home_in_use(&self) -> bool {
        matches!(self, Self::HomeInUse(_))
    }
}

impl std::fmt::Display for StartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HomeInUse(detail) | Self::Other(detail) => f.write_str(detail),
        }
    }
}

impl From<String> for StartupFailure {
    fn from(detail: String) -> Self {
        Self::Other(detail)
    }
}

impl From<&str> for StartupFailure {
    fn from(detail: &str) -> Self {
        Self::Other(detail.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubmitError {
    Busy,
    TooLarge,
    Stopped,
}

struct QueuedRequest {
    submitted: std::time::Instant,
    observation: observability::Observation,
    request: Request,
    _bytes: QueuedBytes,
}

struct QueuedBytes {
    count: usize,
    held: Arc<AtomicUsize>,
}

impl Drop for QueuedBytes {
    fn drop(&mut self) {
        self.held.fetch_sub(self.count, Ordering::AcqRel);
    }
}

pub(crate) struct JournalWorker {
    readers: Arc<super::journal_payload::PayloadReaders>,
    metrics: Arc<JournalMetrics>,
    requests: Option<mpsc::SyncSender<QueuedRequest>>,
    queued_bytes: Arc<AtomicUsize>,
    completions: Option<mpsc::Receiver<Completion>>,
    acknowledgements: Option<mpsc::SyncSender<(u64, Result<(), String>)>>,
    closed: Arc<AtomicBool>,
    #[cfg(feature = "gui")]
    skip_final_checkpoint: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    shutdown_timer: tasty_utils::batch_writer::ShutdownTimer,
    #[cfg(test)]
    pub(crate) fail_next_publication: Arc<AtomicBool>,
    /// 시험에서 응답하지 않는 worker를 흉내 낸다. 켜진 동안의 완료는 App에 보내지 않고 버린다.
    #[cfg(all(test, feature = "gui"))]
    pub(crate) mute_replies: Arc<AtomicBool>,
    /// 시험이 설치 Publish의 전송을 확인한다. worker가 설치 이벤트가 든 Publish를 App 채널에 보내는
    /// 데 성공할 때마다 그 batch의 설치 operation들로 바뀐다. 전송 성공만 말하고 아직 받지 않았다는
    /// 뜻은 아니다.
    #[cfg(all(test, unix))]
    pub(crate) installs_sent: Arc<std::sync::Mutex<Vec<tasty_core::OperationId>>>,
    /// 시험의 정체 감지가 worker 스레드의 상태를 읽는다.
    #[cfg(test)]
    pub(crate) thread_probe: thread_probe::ThreadProbe,
}

impl JournalWorker {
    /// Filesystem work begins on the new thread, including opening and recovering the database.
    #[cfg(test)]
    pub(crate) fn spawn(home: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        Self::spawn_with(home, wake, None)
    }

    /// `writer_lock`는 부팅 첫머리에서 이 홈의 저널에 대해 선점한 잠금이다. 있으면 worker가
    /// 저장소를 열기 전부터 그 잠금을 쥐고 writer가 된다. 없으면 worker 안에서 잠근다.
    pub(crate) fn spawn_with(
        home: PathBuf,
        wake: Arc<dyn Fn() + Send + Sync>,
        writer_lock: Option<tasty_event_store::WriterLock>,
    ) -> Result<Self, String> {
        let (requests, incoming) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (outgoing, completions) = mpsc::sync_channel(QUEUE_CAPACITY);
        // A separate channel leaves room for the publication barrier even when admission is full.
        let (acknowledgements, acks) = mpsc::sync_channel(1);
        let closed = Arc::new(AtomicBool::new(false));
        let stopped = closed.clone();
        let skip_final_checkpoint = Arc::new(AtomicBool::new(false));
        let worker_skip_checkpoint = skip_final_checkpoint.clone();
        let metrics = Arc::new(JournalMetrics::default());
        let worker_metrics = metrics.clone();
        let readers = Arc::new(super::journal_payload::PayloadReaders::default());
        let worker_readers = readers.clone();
        #[cfg(test)]
        let fail_next_publication = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let publication_fault = fail_next_publication.clone();
        #[cfg(test)]
        let mute_replies = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let muted = Arc::clone(&mute_replies);
        #[cfg(all(test, unix))]
        let installs_sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        #[cfg(all(test, unix))]
        let sent_installs = Arc::clone(&installs_sent);
        #[cfg(test)]
        let thread_probe = thread_probe::ThreadProbe::default();
        #[cfg(test)]
        let bound_probe = thread_probe.clone();
        let thread = std::thread::Builder::new()
            .name("structure-journal".into())
            .spawn(move || {
                #[cfg(test)]
                bound_probe.bind_current();
                worker::run(
                    home,
                    writer_lock,
                    worker_readers,
                    worker_metrics,
                    incoming,
                    outgoing,
                    acks,
                    stopped,
                    worker_skip_checkpoint,
                    wake,
                    #[cfg(test)]
                    publication_fault,
                    #[cfg(test)]
                    muted,
                    #[cfg(all(test, unix))]
                    sent_installs,
                );
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            readers,
            metrics,
            requests: Some(requests),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            completions: Some(completions),
            acknowledgements: Some(acknowledgements),
            closed,
            #[cfg(feature = "gui")]
            skip_final_checkpoint,
            thread: Some(thread),
            shutdown_timer: Default::default(),
            #[cfg(test)]
            fail_next_publication,
            #[cfg(all(test, feature = "gui"))]
            mute_replies,
            #[cfg(all(test, unix))]
            installs_sent,
            #[cfg(test)]
            thread_probe,
        })
    }

    /// Retain references synchronously with draft freezing, before another projection can ACK.
    /// The storage worker holds the same readers lock while transferring snapshot pins and GC.
    pub(crate) fn retain_payloads(
        &self,
        refs: Vec<tasty_core::DataRef>,
    ) -> Result<super::journal_payload::PayloadReadLease, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("structure journal stopped".into());
        }
        self.readers.lease(refs)
    }

    /// Nonconsuming cached telemetry, independent of the worker's Store/Executor locks.
    pub(crate) fn metrics_handle(&self) -> Arc<JournalMetrics> {
        self.metrics.clone()
    }
    pub(crate) fn metrics_snapshot(&self) -> JournalMetricsSnapshot {
        self.metrics_handle().snapshot()
    }

    pub(crate) fn submit(&self, request: Request) -> Result<(), SubmitError> {
        self.submit_owned(request).map_err(|(error, _)| error)
    }

    pub(crate) fn submit_owned(&self, request: Request) -> Result<(), (SubmitError, Request)> {
        if self.closed.load(Ordering::Acquire) {
            self.metrics.reject(None, &SubmitError::Stopped);
            return Err((SubmitError::Stopped, request));
        }
        let bytes = request_size(&request.work);
        if bytes > MAX_QUEUED_BYTES || request_payload_too_large(&request.work) {
            self.metrics.reject(None, &SubmitError::TooLarge);
            return Err((SubmitError::TooLarge, request));
        }
        let Some(requests) = self.requests.as_ref() else {
            self.metrics.reject(None, &SubmitError::Stopped);
            return Err((SubmitError::Stopped, request));
        };
        if self
            .queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                held.checked_add(bytes)
                    .filter(|next| *next <= MAX_QUEUED_BYTES)
            })
            .is_err()
        {
            self.metrics.reject(None, &SubmitError::Busy);
            return Err((SubmitError::Busy, request));
        }
        let observation = self.metrics.prepare(bytes);
        let observation_id = observation.id();
        let queued = QueuedRequest {
            observation,
            submitted: std::time::Instant::now(),
            request,
            _bytes: QueuedBytes {
                count: bytes,
                held: self.queued_bytes.clone(),
            },
        };
        match requests.try_send(queued) {
            Ok(()) => {
                self.metrics.accept(observation_id);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(queued)) => {
                self.metrics
                    .reject(Some(observation_id), &SubmitError::Busy);
                Err((SubmitError::Busy, queued.request))
            }
            Err(mpsc::TrySendError::Disconnected(queued)) => {
                self.metrics
                    .reject(Some(observation_id), &SubmitError::Stopped);
                Err((SubmitError::Stopped, queued.request))
            }
        }
    }

    pub(crate) fn try_recv(&self) -> Result<Completion, mpsc::TryRecvError> {
        self.completions
            .as_ref()
            .ok_or(mpsc::TryRecvError::Disconnected)?
            .try_recv()
    }

    pub(crate) fn acknowledge(
        &self,
        batch: u64,
        applied: Result<(), String>,
    ) -> Result<(), SubmitError> {
        match self
            .acknowledgements
            .as_ref()
            .ok_or(SubmitError::Stopped)?
            .try_send((batch, applied))
        {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(SubmitError::Busy),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(SubmitError::Stopped),
        }
    }

    /// Boot recovery preserves the failed source instead of publishing shutdown maintenance.
    #[cfg(feature = "gui")]
    pub(crate) fn abort_without_checkpoint(&mut self) {
        self.skip_final_checkpoint.store(true, Ordering::Release);
        self.stop();
    }

    /// Disconnecting both outbound paths unblocks a worker waiting for publication or completion.
    pub(crate) fn stop(&mut self) {
        if tracing::enabled!(target: "tasty::journal_metrics", tracing::Level::DEBUG) {
            tracing::debug!(target: "tasty::journal_metrics", snapshot=?self.metrics_snapshot(), "journal cached snapshot before stop");
        }
        self.shutdown_timer.start(std::time::Instant::now());
        self.closed.store(true, Ordering::Release);
        self.requests.take();
        self.acknowledgements.take();
        self.completions.take();
    }
}

impl Drop for JournalWorker {
    fn drop(&mut self) {
        self.stop();
        // 시험이 멈춘 worker로 판정했다. join하면 실패 대신 그 자리에서 멈춘다.
        #[cfg(test)]
        if self.thread_probe.is_abandoned() {
            drop(self.thread.take());
            return;
        }
        if let Some(thread) = self.thread.take() {
            let started = std::time::Instant::now();
            let panicked = thread.join().is_err();
            let now = std::time::Instant::now();
            self.shutdown_timer.joined(now);
            tracing::info!(target: "tasty::shutdown", panicked,
                stop_to_join_us = ?self.shutdown_timer.snapshot(now).map(|t| t.elapsed.as_micros()),
                join_wait_us = now.saturating_duration_since(started).as_micros(),
                "journal_actual_join");
            if panicked {
                tracing::error!("structure journal worker panicked during shutdown");
            }
        }
    }
}

// Internal resolution may carry both a fixed decision input and its frozen response. Its total
// bytes share the queue budget; the public request limit applies to the admitted user payload.
fn request_payload_too_large(work: &Work) -> bool {
    match work {
        Work::Admit(header) => header.original_digest.len() > MAX_COMMAND_INPUT_BYTES,
        Work::PutPreparation(input) => {
            json_len_within(&input.params, MAX_COMMAND_INPUT_BYTES).is_none()
        }
        _ => false,
    }
}

/// JSON 직렬화 길이를 버퍼 없이 센다. `limit`를 넘으면 그 자리에서 멈추고 `None`을 준다.
///
/// `request_size`는 App 스레드에서 매 제출마다 불린다. 버릴 직렬화 버퍼를 만들지 않고,
/// 상한을 넘은 작업은 상한까지만 센다. 직렬화 오류도 `None`이다.
fn json_len_within(value: &impl serde::Serialize, limit: usize) -> Option<usize> {
    struct Counter {
        written: usize,
        limit: usize,
    }
    // 오류 생성은 상한을 넘는 한 번뿐이다. 본문에 두면 이스케이프 문자마다 불리는 쓰기가
    // 인라인되지 않아 측정에서 세 배 가까이 느려졌다.
    #[cold]
    #[inline(never)]
    fn exceeded() -> std::io::Error {
        std::io::Error::other("JSON length limit exceeded")
    }
    impl std::io::Write for Counter {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.write_all(buf)?;
            Ok(buf.len())
        }
        #[inline]
        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            self.written += buf.len();
            if self.written > self.limit {
                return Err(exceeded());
            }
            Ok(())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { written: 0, limit };
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.written)
}

/// 큐 예산으로 잴 JSON 길이. 예산을 넘으면 예산 + 1을 준다. 호출자는 예산과 비교만 하므로
/// 정확한 초과량은 필요 없고, 뒤따르는 덧셈이 넘치지 않는다.
fn json_len(value: &impl serde::Serialize) -> usize {
    json_len_within(value, MAX_QUEUED_BYTES).unwrap_or(MAX_QUEUED_BYTES + 1)
}

pub(crate) fn request_size(work: &Work) -> usize {
    match work {
        Work::ReconcilePreparation {
            lease,
            evidence,
            discarded,
            view,
        } => json_len(&(lease, view))
            .saturating_add(evidence.capacity())
            .saturating_add(discarded.as_ref().map_or(0, String::capacity)),
        Work::CapturePreset { draft } => draft.weight(),
        Work::ReconcileRetirement { lease, evidence } => {
            json_len(lease).saturating_add(evidence.capacity())
        }
        Work::PrepareSubtree { binding, draft } => json_len(&(binding, draft)),
        Work::PrepareUndo {
            binding,
            target_pane,
            scope,
            shell,
        } => json_len(&(binding, target_pane, scope, shell)),
        Work::ReserveExecutionIds { binding, kinds } => {
            binding.stream.len()
                + binding.journal_id.len()
                + kinds.capacity() * std::mem::size_of::<(IdKind, u32)>()
        }
        Work::CaptureClosed {
            view,
            binding,
            surfaces,
            display_name,
            ..
        } => surfaces.iter().fold(
            json_len(view).saturating_add(binding.stream.len())
                + binding.journal_id.len()
                + display_name.as_ref().map_or(0, String::len)
                + 96,
            |sum, surface| sum.saturating_add(surface.weight()),
        ),
        Work::Capture {
            binding, surfaces, ..
        } => surfaces.iter().fold(
            binding.stream.len() + binding.journal_id.len() + 96,
            |sum, surface| sum.saturating_add(surface.weight()),
        ),
        #[cfg(any(feature = "gui", test))]
        Work::RetireEngine(binding) => binding.stream.len() + binding.journal_id.len() + 64,
        #[cfg(feature = "gui")]
        Work::SaveView(view) => json_len(view),
        Work::OpenEngine {
            selection,
            normal_category_name,
            surface_floor: _,
        } => {
            let serialized = json_len(&(selection, normal_category_name)).saturating_add(16);
            match selection {
                EngineSelection::ImportedSlot { source } => serialized.max(
                    source
                        .weight()
                        .saturating_add(normal_category_name.len())
                        .saturating_add(64),
                ),
                _ => serialized,
            }
        }
        Work::Admit(header) => header
            .original_digest
            .len()
            .saturating_add(header.actor.len())
            .saturating_add(header.origin.len())
            .saturating_add(header.causation_id.as_ref().map_or(0, String::len))
            .saturating_add(header.key.as_ref().map_or(0, |key| {
                key.caller_scope
                    .len()
                    .saturating_add(key.idempotency_key.len())
            })),
        Work::Resolve { changes, response } => json_len(&(changes, response)),
        Work::Reserve(kinds) => kinds
            .len()
            .saturating_mul(std::mem::size_of::<(IdKind, u32)>()),
        #[cfg(test)]
        Work::ReadEngine(stream) => stream.len(),
        Work::ReadCommand(stream) => stream.len(),
        Work::ReadPayload(_) | Work::PreserveFailedPayload(_) => {
            std::mem::size_of::<tasty_core::DataRef>()
        }
        Work::PutPayload(bytes) => bytes.len(),
        Work::PutPreparation(input) => json_len(input),
        #[cfg(feature = "gui")]
        Work::ClaimForward { stream, operation } => stream.len().saturating_add(operation.0.len()),
        Work::ClaimPreparation { stream, operation }
        | Work::ClaimRetirement { stream, operation } => {
            stream.len().saturating_add(operation.0.len())
        }
        Work::Prepared { lease, result } => json_len(&(lease, result)),
        Work::InstallationRejected { lease, reason }
        | Work::PreparationUncertain { lease, reason } => json_len(&(lease, reason)),
        Work::CleanupFinished { lease, view } => json_len(&(lease, view)),
        #[cfg(feature = "gui")]
        Work::ForwardFinished { lease, outcome } => json_len(&(lease, outcome)),
        Work::RetirementFinished { lease, outcome } => json_len(&(lease, outcome)),
        Work::CancelAdmission => 0,
    }
}

#[cfg(test)]
mod tests;
