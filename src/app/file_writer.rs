//! 사용자 설정 파일 전담 writer와 설정 저장의 표시·확정 revision.
//!
//! 메인 루프는 설정을 메모리에 적용한 뒤 저장 요청만 넣고 다음 일을 한다. `file-writer` 스레드가
//! 파일을 교체하고, 메인 루프가 회차마다 결과를 거두어 원 요청에 한 번 돌려준다. 접수·수집·예산·종료
//! 규칙은 `tasty_utils::batch_writer`가 정한다. 파일 사이에는 transaction이 없어 요청마다 따로 끝난다.
//! 계약은 docs/design/systems/storage-contract.md의 "파일 실행기" 절에 있다.

pub(crate) mod command;
mod measurements;
#[cfg(test)]
pub(crate) mod settle;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tasty_settings::{Settings, SettingsStorage};
use tasty_utils::batch_writer::{
    self, Clock, Completion, Execution, Executor, Handle, Limits, RequestReceipt, Settled,
    WakeLatch, Writer,
};

use crate::app::settings_change::SettingsDone;
use crate::intent::IntentOrigin;

/// config.toml 저장의 snapshot 키. 같은 묶음 안에서는 마지막 설정만 쓴다.
const SETTINGS_KEY: &str = "file:config.toml";

/// writer에 넣는 요청. 요청은 소유한 값만 담는다.
pub(crate) enum FileRequest {
    Command(command::Command),
    Snapshot(crate::core::file_snapshot::Snapshot),
    /// 메모리에 적용한 설정 전체. `revision`은 표시 revision이다.
    Settings {
        revision: u64,
        settings: Box<Settings>,
    },
}

impl batch_writer::Request for FileRequest {
    /// 동적 문자열과 목록도 큐 예산에 포함한다. 바이트는 보관하거나 로그에 남기지 않는다.
    fn bytes(&self) -> usize {
        match self {
            Self::Command(command) => command.bytes,
            Self::Settings { settings, .. } => settings_bytes(settings),
            Self::Snapshot(snapshot) => snapshot.bytes.len(),
        }
    }

    fn snapshot_key(&self) -> Option<&str> {
        match self {
            Self::Command(_) => None,
            Self::Settings { .. } => Some(SETTINGS_KEY),
            Self::Snapshot(snapshot) => Some(snapshot.key),
        }
    }
}

fn settings_bytes(settings: &Settings) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(std::mem::size_of::<Settings>());
    match serde_json::to_writer(&mut count, settings) {
        Ok(()) => count.0,
        Err(_) => usize::MAX,
    }
}

/// 요청 하나의 성공 결과.
#[derive(Debug)]
pub(crate) enum FileOutput {
    Command(command::Output),
    Snapshot,
    /// 이 revision의 설정으로 파일을 교체했다.
    Settings {
        revision: u64,
    },
}

/// 저장하지 못한 이유. 경로와 OS 오류 문구를 싣는다. 파일 내용은 싣지 않는다.
#[derive(Debug, Clone)]
pub(crate) struct FileError(Arc<str>);

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 설정·snapshot 교체 시도 계측. 여러 파일을 다루는 ordered command는 이 수에 넣지 않는다.
#[derive(Debug, Default)]
pub(crate) struct FileWriteStats {
    replaced: AtomicU64,
    failed: AtomicU64,
    unknown: AtomicU64,
    operations: Arc<tasty_utils::file_replace::FileOperationMetrics>,
    commands: measurements::Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileWriteCounts {
    pub(crate) replaced: u64,
    pub(crate) failed: u64,
    pub(crate) unknown: u64,
    pub(crate) operations: tasty_utils::file_replace::FileOperationSnapshot,
    pub(crate) commands: measurements::CommandCounts,
}

impl FileWriteStats {
    pub(crate) fn snapshot(&self) -> FileWriteCounts {
        FileWriteCounts {
            replaced: self.replaced.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            unknown: self.unknown.load(Ordering::Relaxed),
            operations: self.operations.snapshot(),
            commands: self.commands.snapshot(),
        }
    }
}

/// 파일 실행기. 설정 저장소 port를 worker 스레드에서 부른다.
pub(crate) struct FileExecutor {
    settings: Arc<dyn SettingsStorage>,
    stats: Arc<FileWriteStats>,
}

impl FileExecutor {
    fn run_one(&self, request: &mut FileRequest) -> Result<FileOutput, FileError> {
        match request {
            FileRequest::Command(command) => command
                .execute()
                .map(FileOutput::Command)
                .map_err(|e| FileError(e.message.into())),
            FileRequest::Snapshot(snapshot) => snapshot
                .write()
                .map(|()| FileOutput::Snapshot)
                .map_err(|e| FileError(e.into())),
            FileRequest::Settings { revision, settings } => self
                .settings
                .save(settings)
                .map(|()| FileOutput::Settings {
                    revision: *revision,
                })
                .map_err(|e| FileError(format!("{e:#}").into())),
        }
    }
}

impl Executor for FileExecutor {
    type Request = FileRequest;
    type Output = FileOutput;
    type Error = FileError;

    fn output_bytes(output: &FileOutput) -> usize {
        match output {
            FileOutput::Command(output) => output.bytes,
            _ => 0,
        }
    }

    /// 요청마다 따로 교체한다. 앞 요청의 교체는 뒤 요청의 실패와 관계없이 남는다.
    fn execute(&mut self, batch: &mut [FileRequest]) -> Execution<FileOutput, FileError> {
        let settled = batch
            .iter_mut()
            .map(|request| {
                let snapshot = !matches!(request, FileRequest::Command(_));
                let _observation =
                    tasty_utils::file_replace::observe_operations(self.stats.operations.clone());
                match catch_unwind(AssertUnwindSafe(|| self.run_one(request))) {
                    Ok(Ok(output)) => {
                        if let FileOutput::Command(command) = &output {
                            self.stats.commands.record(command.disposition);
                        }
                        if snapshot {
                            self.stats.replaced.fetch_add(1, Ordering::Relaxed);
                        }
                        Settled::Done(Ok(output))
                    }
                    Ok(Err(error)) => {
                        if snapshot {
                            self.stats.failed.fetch_add(1, Ordering::Relaxed);
                        } else {
                            self.stats
                                .commands
                                .record(Some(command::CommandDisposition {
                                    outcome: command::CommandOutcome::KnownFailure,
                                    partial: false,
                                }));
                        }
                        tracing::warn!(%error, "file save failed");
                        Settled::Failed(error)
                    }
                    // 교체 도중의 panic이라 파일이 바뀌었는지 알 수 없다.
                    Err(_) => {
                        if snapshot {
                            self.stats.unknown.fetch_add(1, Ordering::Relaxed);
                        } else {
                            self.stats
                                .commands
                                .record(Some(command::CommandDisposition {
                                    outcome: command::CommandOutcome::Unknown,
                                    partial: false,
                                }));
                        }
                        Settled::Unknown(FileError("file save panicked".into()))
                    }
                }
            })
            .collect();
        Execution::Finished(settled)
    }
}

/// AppServices가 소유하는 writer.
pub(crate) struct FileWriterParts {
    pub(crate) writer: Writer<FileExecutor>,
    pub(crate) wake: Arc<WakeLatch>,
    pub(crate) stats: Arc<FileWriteStats>,
    end_reported: bool,
}

impl FileWriterParts {
    /// 메인 루프 회차마다 결과를 거두기 전에 부른다. 종료를 요청하지 않았는데 worker가 끝났으면 한 번 알린다.
    pub(crate) fn poll(&mut self) {
        self.wake.acknowledge();
        if !self.end_reported && self.writer.worker_ended_unexpectedly() {
            self.end_reported = true;
            tracing::error!("file writer ended without a shutdown request");
        }
    }
}

/// `file-writer` 스레드를 시작한다. `wake`는 묶음이 끝날 때 메인 루프를 깨운다.
pub(crate) fn start(
    settings: Arc<dyn SettingsStorage>,
    limits: Limits,
    clock: Arc<dyn Clock>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> std::io::Result<FileWriterParts> {
    let stats = Arc::new(FileWriteStats::default());
    let wake = WakeLatch::new(wake);
    let writer = Writer::spawn(
        "file-writer",
        FileExecutor {
            settings,
            stats: stats.clone(),
        },
        limits,
        clock,
        wake.notifier(),
    )?;
    Ok(FileWriterParts {
        writer,
        wake,
        stats,
        end_reported: false,
    })
}

/// 저장 결과를 받을 쪽. 내부 저장은 출처 없이 완료 콜백만 받을 수 있다.
pub(crate) struct SaveReply {
    pub(crate) origin: Option<IntentOrigin>,
    pub(crate) done: Option<SettingsDone>,
    /// 이 요청이 적용한 설정. 응답이 싣는다.
    pub(crate) settings: Box<Settings>,
}

/// 끝난 저장 하나.
pub(crate) struct SettledSave {
    pub(crate) reply: SaveReply,
    pub(crate) saved: Result<(), String>,
}

struct PendingSave {
    revision: u64,
    receipt: RequestReceipt<FileExecutor>,
    reply: SaveReply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettingsSaveSnapshot {
    pub(crate) visible: u64,
    pub(crate) committed: u64,
    pub(crate) pending: usize,
    pub(crate) dirty_age_us: Option<u64>,
}

/// 설정 저장의 표시·확정 revision과 결과를 기다리는 요청.
///
/// 표시 revision은 메모리에 적용할 때마다 오르고, 확정 revision은 그 revision의 설정으로 파일 교체가
/// 성공했을 때만 오른다. 오래된 요청의 성공은 더 새 표시 revision을 확정하지 않는다. 확정하지 못한
/// 최신 설정은 종료 때 한 번 더 저장을 시도한다.
pub(crate) struct SettingsSaves {
    /// writer 큐. 없으면(시험 구성) 접수 때 호출 스레드에서 바로 저장한다.
    queue: Option<Handle<FileExecutor>>,
    storage: Arc<dyn SettingsStorage>,
    visible: u64,
    committed: u64,
    latest: Option<Box<Settings>>,
    pending: Vec<PendingSave>,
    settled: Vec<SettledSave>,
    retried_at_shutdown: bool,
    clock: Arc<dyn Clock>,
    dirty_since: Option<Instant>,
}

impl SettingsSaves {
    pub(crate) fn new(
        queue: Option<Handle<FileExecutor>>,
        storage: Arc<dyn SettingsStorage>,
    ) -> Self {
        Self::with_clock(queue, storage, Arc::new(batch_writer::SystemClock))
    }

    fn with_clock(
        queue: Option<Handle<FileExecutor>>,
        storage: Arc<dyn SettingsStorage>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            queue,
            storage,
            visible: 0,
            committed: 0,
            latest: None,
            pending: Vec::new(),
            settled: Vec::new(),
            retried_at_shutdown: false,
            clock,
            dirty_since: None,
        }
    }

    /// Cached revision metadata only; reading does not collect receipts or access storage.
    pub(crate) fn snapshot(&self) -> SettingsSaveSnapshot {
        SettingsSaveSnapshot {
            visible: self.visible,
            committed: self.committed,
            pending: self.pending.len(),
            dirty_age_us: self.dirty_since.map(|since| {
                self.clock
                    .now()
                    .saturating_duration_since(since)
                    .as_micros()
                    .try_into()
                    .unwrap_or(u64::MAX)
            }),
        }
    }

    /// 메모리에 적용한 설정의 저장을 넣는다. 기다리지 않는다. 접수가 거절되면 저장 실패로 끝나고
    /// 설정은 확정하지 못한 채 남는다.
    pub(crate) fn submit(
        &mut self,
        settings: Settings,
        origin: Option<IntentOrigin>,
        done: Option<SettingsDone>,
    ) {
        self.visible += 1;
        self.dirty_since.get_or_insert_with(|| self.clock.now());
        let revision = self.visible;
        let settings = Box::new(settings);
        self.latest = Some(settings.clone());
        let reply = SaveReply {
            origin,
            done,
            settings: settings.clone(),
        };
        self.enqueue(revision, settings, reply);
    }

    fn enqueue(&mut self, revision: u64, settings: Box<Settings>, reply: SaveReply) {
        let Some(queue) = &self.queue else {
            let saved = self.storage.save(&settings).map_err(|e| format!("{e:#}"));
            if saved.is_ok() {
                self.confirm(revision);
            }
            self.settled.push(SettledSave { reply, saved });
            return;
        };
        match queue.submit(FileRequest::Settings { revision, settings }) {
            Ok(receipt) => self.pending.push(PendingSave {
                revision,
                receipt,
                reply,
            }),
            Err(error) => {
                tracing::warn!(%error, revision, "settings save was not accepted");
                self.settled.push(SettledSave {
                    reply,
                    saved: Err(error.to_string()),
                });
            }
        }
    }

    /// 끝난 저장을 꺼낸다. 기다리지 않는다.
    pub(crate) fn reap(&mut self) -> Vec<SettledSave> {
        self.collect_completed();
        std::mem::take(&mut self.settled)
    }

    pub(crate) fn collect_completed(&mut self) {
        let mut index = 0;
        while index < self.pending.len() {
            let Some(completion) = self.pending[index].receipt.try_take() else {
                index += 1;
                continue;
            };
            let pending = self.pending.swap_remove(index);
            if let Completion::Done(Ok(FileOutput::Settings { revision })) = &completion {
                self.confirm(*revision);
            }
            let saved = saved_of(&completion);
            if let Err(error) = &saved {
                tracing::warn!(revision = pending.revision, %error, "settings were not saved");
            }
            self.settled.push(SettledSave {
                reply: pending.reply,
                saved,
            });
        }
    }

    fn confirm(&mut self, revision: u64) {
        self.committed = self.committed.max(revision);
        if self.committed >= self.visible {
            self.dirty_since = None;
        }
    }

    /// 표시했지만 확정하지 못한 revision이 있으면 `(표시, 확정)`.
    pub(crate) fn unsaved(&self) -> Option<(u64, u64)> {
        (self.visible > self.committed).then_some((self.visible, self.committed))
    }

    /// Most recently applied/submitted settings, even while its replacement is pending or failed.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn latest(&self) -> Option<&Settings> {
        self.latest.as_deref()
    }

    /// 결과를 기다리는 저장 수.
    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }

    /// 종료 전에 한 번 부른다. 최신 표시 설정을 저장하는 요청이 없고 확정되지도 않았으면 다시 넣는다.
    /// 앞서 실패한 요청에는 다시 답하지 않는다.
    pub(crate) fn retry_latest_before_shutdown(&mut self) {
        if std::mem::replace(&mut self.retried_at_shutdown, true) {
            return;
        }
        let covered = self.pending.iter().any(|p| p.revision == self.visible);
        if covered || self.unsaved().is_none() {
            return;
        }
        let Some(latest) = self.latest.clone() else {
            return;
        };
        tracing::info!(
            revision = self.visible,
            "saving unsaved settings before exit"
        );
        let reply = SaveReply {
            origin: None,
            done: None,
            settings: latest.clone(),
        };
        self.enqueue(self.visible, latest, reply);
    }
}

pub(super) fn saved_of(completion: &Completion<FileOutput, FileError>) -> Result<(), String> {
    saved_of_erased(&completion.erase())
}

fn saved_of_erased(completion: &Completion<(), FileError>) -> Result<(), String> {
    match completion {
        Completion::Done(Ok(())) => Ok(()),
        Completion::Superseded { outcome, .. } => saved_of_erased(outcome),
        Completion::Done(Err(e)) | Completion::Failed(e) => Err(e.to_string()),
        Completion::Unknown(e) => Err(format!("save outcome unknown: {e}")),
        Completion::Lost => Err("file writer ended before reporting a result".into()),
    }
}

#[cfg(test)]
mod tests;
