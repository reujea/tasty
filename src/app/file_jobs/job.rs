//! 탐색기 파일 작업 하나를 항목 단위로 실행한다. 진행·취소·이름 충돌 질문·항목별 결과·되돌리기 기록을
//! App 과 나누는 상태는 [`Shared`] 하나에 둔다. 기존 항목은 사용자가 Replace 를 고른 파일에서만 바뀐다.

#[path = "count.rs"]
mod count;

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tasty_utils::poison::{recover_mutex, recover_poisoned};

use super::ops::{drop_nested, remove_path, rename_noreplace, unique_dest};
use leftover::Leftover;

/// 바이트 총량을 아직 모르거나 셀 수 없는 작업(휴지통)의 표시.
pub(crate) const UNKNOWN_BYTES: u64 = u64::MAX;
/// 진행 표시를 다시 그리라고 깨우는 최소 간격. 바이트마다 이벤트를 보내지 않는다.
const WAKE_INTERVAL: Duration = Duration::from_millis(100);
/// 답을 기다리는 동안 취소 여부를 다시 보는 간격.
const ANSWER_POLL: Duration = Duration::from_millis(100);
/// 파일 복사 한 번에 읽는 크기. 이 단위마다 취소와 진행을 확인한다.
const COPY_CHUNK: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpKind {
    Copy,
    Move,
    Trash,
    /// 끝난 복사·이동을 되돌린다.
    Undo,
    /// 원본이 남은 이동 항목을 사본과 비교한 뒤 원본만 지운다.
    RemoveOriginals,
}

/// 이름 충돌에 대한 사용자의 답.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Choice {
    KeepBoth,
    Skip,
    /// 파일끼리 충돌할 때만 받는다. 폴더가 낀 충돌에서는 Keep both 로 처리한다.
    Replace,
    CancelRest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Answer {
    pub choice: Choice,
    /// 남은 충돌에도 같은 답을 쓴다. Replace 는 폴더가 낀 충돌에 적용하지 않고 다시 묻는다.
    pub apply_all: bool,
}

/// 충돌한 두 항목의 비교 정보.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct EntryFacts {
    pub is_dir: bool,
    /// 파일의 바이트 수. 폴더는 None.
    pub size: Option<u64>,
    /// 폴더의 바로 아래 항목 수. 읽지 못했거나 파일이면 None.
    pub items: Option<usize>,
    pub modified: Option<SystemTime>,
}

impl EntryFacts {
    fn read(path: &Path) -> Option<Self> {
        let meta = path.symlink_metadata().ok()?;
        let is_dir = meta.is_dir();
        Some(Self {
            is_dir,
            size: (!is_dir).then_some(meta.len()),
            items: is_dir
                .then(|| std::fs::read_dir(path).ok().map(Iterator::count))
                .flatten(),
            modified: meta.modified().ok(),
        })
    }
}

/// 이름 충돌 질문. 작업은 답을 받을 때까지 멈춘다.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Ask {
    pub name: String,
    pub folder: PathBuf,
    pub existing: EntryFacts,
    pub incoming: EntryFacts,
    /// 폴더가 낀 충돌. Replace 를 내놓지 않는다.
    pub folder_conflict: bool,
    /// 이 질문 뒤에 남은 충돌 수(작업 시작 때 센 값). 0 이면 "모두 적용" 을 내놓지 않는다.
    pub remaining: usize,
}

/// 충돌을 누가 정하는가.
enum Asker {
    /// App 이 사용자에게 묻고 [`Shared::answer`] 로 돌려준다.
    Interactive,
    /// 묻지 않고 항상 이 답을 쓴다.
    Fixed(Choice),
}

/// File-level transfer progress is separate from top-level result and history counts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileProgress {
    pub done: usize,
    pub total: usize,
    pub counting: bool,
}

/// 진행 표시가 읽는 값.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Snapshot {
    #[serde(default)]
    pub files: Option<FileProgress>,
    pub items_done: usize,
    pub items_total: usize,
    pub bytes_done: u64,
    /// [`UNKNOWN_BYTES`] 이면 바이트 표시가 없으며 전송 막대는 파일 수를 쓴다.
    pub bytes_total: u64,
    pub current: String,
    /// 원본 지우기 작업이 비교를 마치고 지우는 단계다.
    pub removing: bool,
}

/// worker 와 App 이 함께 보는 작업 상태.
pub(crate) struct Shared {
    transfer: AtomicBool,
    counting: AtomicBool,
    files_total: AtomicUsize,
    files_done: AtomicUsize,
    cancel: AtomicBool,
    items_total: AtomicUsize,
    items_done: AtomicUsize,
    bytes_total: AtomicU64,
    bytes_done: AtomicU64,
    current: Mutex<String>,
    removing: AtomicBool,
    asker: Asker,
    ask: Mutex<Option<Ask>>,
    answer: Mutex<Option<Answer>>,
    answered: Condvar,
    started: Instant,
    last_wake_ms: AtomicU64,
    wake: Box<dyn Fn() + Send + Sync>,
    #[cfg(test)]
    write_failure_after: Mutex<Option<(u64, i32)>>,
    /// 시험의 휴지통 대역이 이 이유로 거절한다.
    #[cfg(test)]
    trash_refusal: Mutex<Option<String>>,
    /// 시험의 휴지통 대역이 항목을 처리한 뒤 작업을 취소한다.
    #[cfg(test)]
    cancel_after_trash: AtomicBool,
}

impl Shared {
    pub(crate) fn interactive(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with(Asker::Interactive, Box::new(wake))
    }
    /// 묻지 않는 작업. 충돌은 항상 `choice` 로 처리한다.
    pub(crate) fn fixed(choice: Choice) -> Self {
        Self::with(Asker::Fixed(choice), Box::new(|| {}))
    }
    fn with(asker: Asker, wake: Box<dyn Fn() + Send + Sync>) -> Self {
        Self {
            transfer: AtomicBool::new(false),
            counting: AtomicBool::new(false),
            files_total: AtomicUsize::new(0),
            files_done: AtomicUsize::new(0),
            cancel: AtomicBool::new(false),
            items_total: AtomicUsize::new(0),
            items_done: AtomicUsize::new(0),
            bytes_total: AtomicU64::new(UNKNOWN_BYTES),
            bytes_done: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            removing: AtomicBool::new(false),
            asker,
            ask: Mutex::new(None),
            answer: Mutex::new(None),
            answered: Condvar::new(),
            started: Instant::now(),
            last_wake_ms: AtomicU64::new(0),
            wake,
            #[cfg(test)]
            write_failure_after: Mutex::new(None),
            #[cfg(test)]
            trash_refusal: Mutex::new(None),
            #[cfg(test)]
            cancel_after_trash: AtomicBool::new(false),
        }
    }
    /// 이 작업의 휴지통 호출을 `reason` 으로 거절한다.
    #[cfg(test)]
    pub(crate) fn refuse_trash(&self, reason: &str) {
        *lock(&self.trash_refusal) = Some(reason.to_owned());
    }
    /// 아직 끝나지 않은 항목을 멈춘다. 답을 기다리던 작업도 깨운다.
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        self.answered.notify_all();
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            files: self.transfer.load(Ordering::Acquire).then(|| FileProgress {
                counting: self.counting.load(Ordering::Acquire),
                total: self.files_total.load(Ordering::Acquire),
                done: self.files_done.load(Ordering::Acquire),
            }),
            items_done: self.items_done.load(Ordering::Acquire),
            items_total: self.items_total.load(Ordering::Acquire),
            bytes_done: self.bytes_done.load(Ordering::Acquire),
            bytes_total: self.bytes_total.load(Ordering::Acquire),
            current: lock(&self.current).clone(),
            removing: self.removing.load(Ordering::Acquire),
        }
    }
    /// 답을 기다리는 충돌. 없으면 None.
    pub(crate) fn pending_ask(&self) -> Option<Ask> {
        lock(&self.ask).clone()
    }
    /// 기다리는 질문에 답한다. 질문이 없으면 버린다.
    pub(crate) fn answer(&self, answer: Answer) {
        if lock(&self.ask).is_none() {
            return;
        }
        *lock(&self.answer) = Some(answer);
        self.answered.notify_all();
    }
    /// 원격 서버가 보낸 진행·질문을 이 상태에 옮긴다. 원격 작업의 진행 표시와 충돌 popup 이
    /// 로컬 작업과 같은 값을 읽게 한다. 새 질문이 오면 이전 답은 버린다.
    #[cfg(feature = "gui")]
    pub(crate) fn mirror(&self, snap: &Snapshot, ask: Option<Ask>) {
        self.transfer.store(snap.files.is_some(), Ordering::Release);
        if let Some(files) = &snap.files {
            self.counting.store(files.counting, Ordering::Release);
            self.files_total.store(files.total, Ordering::Release);
            self.files_done.store(files.done, Ordering::Release);
        }
        self.items_total.store(snap.items_total, Ordering::Release);
        self.items_done.store(snap.items_done, Ordering::Release);
        self.bytes_total.store(snap.bytes_total, Ordering::Release);
        self.bytes_done.store(snap.bytes_done, Ordering::Release);
        *lock(&self.current) = snap.current.clone();
        self.removing.store(snap.removing, Ordering::Release);
        let mut pending = lock(&self.ask);
        if ask.is_some() && *pending != ask {
            *lock(&self.answer) = None;
        }
        *pending = ask;
    }
    /// 사용자가 원격 질문에 준 답을 꺼낸다. 꺼내면 질문도 거둔다. 서버의 다음 진행 통지가 상태를 맞춘다.
    #[cfg(feature = "gui")]
    pub(crate) fn take_answer(&self) -> Option<Answer> {
        let answer = lock(&self.answer).take()?;
        *lock(&self.ask) = None;
        Some(answer)
    }
    fn wake_now(&self) {
        self.last_wake_ms
            .store(self.elapsed_ms(), Ordering::Release);
        (self.wake)();
    }
    fn wake_throttled(&self) {
        let now = self.elapsed_ms();
        let last = self.last_wake_ms.load(Ordering::Acquire);
        if now.saturating_sub(last) >= WAKE_INTERVAL.as_millis() as u64 {
            self.wake_now();
        }
    }
    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn set_current(&self, path: &Path) {
        *lock(&self.current) = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
    }
    fn set_removing(&self, removing: bool) {
        self.removing.store(removing, Ordering::Release);
    }
    fn add_bytes(&self, n: u64) {
        self.bytes_done.fetch_add(n, Ordering::AcqRel);
        self.wake_throttled();
    }
    fn finish_item(&self, bytes_done: u64) {
        self.items_done.fetch_add(1, Ordering::AcqRel);
        self.bytes_done.store(bytes_done, Ordering::Release);
        self.wake_throttled();
    }
    /// 충돌을 묻고 답을 기다린다. 기다리는 동안 취소되면 CancelRest 다.
    fn decide(&self, ask: Ask) -> Answer {
        let choice = match self.asker {
            Asker::Fixed(choice) => {
                return Answer {
                    choice,
                    apply_all: true,
                };
            }
            Asker::Interactive => {
                *lock(&self.answer) = None;
                *lock(&self.ask) = Some(ask);
                self.wake_now();
                let mut answer = lock(&self.answer);
                loop {
                    if let Some(answer) = answer.take() {
                        break answer;
                    }
                    if self.cancelled() {
                        break Answer {
                            choice: Choice::CancelRest,
                            apply_all: true,
                        };
                    }
                    answer = self
                        .answered
                        .wait_timeout(answer, ANSWER_POLL)
                        .map(|(guard, _)| guard)
                        .unwrap_or_else(|poisoned| {
                            recover_poisoned(poisoned, "explorer file job answer", &POISON_REPORTED)
                                .0
                        });
                }
            }
        };
        *lock(&self.ask) = None;
        self.wake_now();
        choice
    }
}

/// 작업 스레드가 패닉해도 질문·답 상태는 단순 값이라 그대로 읽어도 된다. 처음 한 번만 보고한다.
static POISON_REPORTED: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    recover_mutex(m.lock(), "explorer file job state", &POISON_REPORTED)
}

/// 항목이 실패한 이유. 화면 문구는 App 이 번역한다.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
    /// OS 오류 문구(번역하지 않는다).
    Os(String),
    /// 폴더를 자기 안으로 옮기거나 복사하려 했다.
    IntoItself,
    /// 다른 디스크로 옮기며 복사는 됐지만 원본을 다 지우지 못했다. 원본은 전부 또는 일부 남고,
    /// 사본은 하나뿐인 온전한 사본일 수 있어 되돌리기에서도 건드리지 않는다.
    SourceNotRemoved(String),
    /// 되돌리려는 자리에 지금 다른 항목이 있다.
    NewerThere,
    /// 되돌릴 항목이 이제 없다.
    Gone,
    /// Replace 로 덮어쓴 파일은 되돌릴 수 없다.
    Replaced,
    /// 작업 뒤 사본의 수정 시각이 바뀌었다. 사용자가 고친 사본으로 보고 남긴다.
    /// 남은 원본을 다시 지울 때는 원본 파일이 바뀌어 남긴 경우에도 쓴다.
    ChangedSince,
    /// 옮긴 뒤 목적지 항목의 수정 시각이 바뀌었다. 옮긴 것과 다른 항목일 수 있어 그 자리에 둔다.
    ChangedAfterMove,
    /// 남은 원본을 다시 지우려는데 사본이 없거나 다른 종류로 바뀌었다. 원본을 남긴다.
    CopyMissing,
    /// 폴더 원본을 다시 지우며 사본에 없는 항목(이동 뒤 넣거나 바꾼 것)을 이 수만큼 남겼다.
    /// 사본은 온전하고, 남은 원본은 다시 시도할 수 있다.
    KeptNotInCopy(usize),
    /// 남은 원본 다시 지우기를 취소해 이 원본이 전부 또는 일부 남았다. 다시 시도할 수 있다.
    RemoveCancelled,
    /// 사본 경로가 링크·마운트·하드링크로 원본과 같은 파일을 가리켜 원본을 지우지 않았다.
    SameAsCopy,
}

impl Reason {
    /// 사본은 온전하고 원본이 남은 이동 항목인가. 이런 항목은 다시 옮기지 않고 원본 삭제만 다시 한다.
    pub(crate) fn leaves_original(&self) -> bool {
        matches!(
            self,
            Self::SourceNotRemoved(_)
                | Self::KeptNotInCopy(_)
                | Self::RemoveCancelled
                | Self::SameAsCopy
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Failure {
    /// 실패한 원래 항목의 경로.
    pub path: PathBuf,
    pub reason: Reason,
}

/// 끝난 항목을 되돌리는 방법.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UndoStep {
    /// 새로 만든 사본과 공개 직후의 수정 시각. 되돌리면 휴지통으로 보내되,
    /// 수정 시각이 달라졌으면 남긴다. 폴더는 자기 수정 시각만 보므로 안쪽 파일을 고친 것은 모른다.
    Created(PathBuf, Option<SystemTime>),
    /// 새로 만든 빈 폴더·빈 파일과 만든 직후의 수정 시각. 되돌리면 휴지통으로 보내되, 비어 있고
    /// 수정 시각이 그대로일 때만 보낸다([`super::named`]).
    Made(PathBuf, Option<SystemTime>),
    /// 옮긴 항목과 옮긴 직후 목적지의 수정 시각. 되돌리면 원래 자리로 옮긴다(그 자리가 비어 있고,
    /// 목적지의 수정 시각이 그대로일 때만). 폴더는 자기 수정 시각만 본다.
    Moved {
        from: PathBuf,
        to: PathBuf,
        made: Option<SystemTime>,
    },
    /// Replace 로 기존 파일을 바꿨다. 되돌릴 수 없다.
    Replaced(PathBuf),
}

/// 작업 결과.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Report {
    pub kind: OpKind,
    /// 복사·이동의 목적지 폴더.
    pub dest: Option<PathBuf>,
    pub total: usize,
    /// 끝난 항목 수(원본 미삭제처럼 사본이 생긴 항목 포함).
    pub done: usize,
    pub undo: Vec<UndoStep>,
    pub failed: Vec<Failure>,
    pub skipped: Vec<PathBuf>,
    pub cancelled: bool,
    /// 휴지통이 받지 않아 아무것도 지우지 못했다.
    pub trash_unavailable: bool,
    /// 사본은 공개했지만 원본을 다 지우지 못한 이동 항목. Retry 가 원본 삭제만 다시 한다.
    pub leftovers: Vec<Leftover>,
    /// 취소로 하지 않은 원래 경로. 취소 때 멈춘 항목도 준비 중 사본을 지웠으므로 여기 든다.
    pub unprocessed: Vec<PathBuf>,
    /// 지우지 못하고 남은 준비용 임시 폴더와 그 OS 오류. 항목 결과와 따로 두며 다시 시도하지 않는다.
    #[serde(default)]
    pub staging_left: Vec<Failure>,
    /// Retry 가 위 목록 대신 그대로 다시 보낼 원래 요청. 원격 서버가 목록을 줄여 보낸 결과에는 빠진 항목이
    /// 있으므로 client 가 보냈던 목록을 쓴다. 연결로 보내지 않는다. 원격 결과 enum 의 변형 크기를
    /// 고르게 두려고 상자에 담는다.
    #[serde(skip)]
    pub resend: Option<Box<Resend>>,
}

/// 줄어든 원격 결과의 Retry 가 다시 보낼, client 가 처음 보낸 경로와 남은 원본.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Resend {
    pub paths: Vec<PathBuf>,
    pub leftovers: Vec<Leftover>,
}

impl UndoStep {
    /// 방금 공개한 사본의 되돌리기 단계.
    fn created(path: PathBuf) -> Self {
        let modified = modified_at(&path);
        Self::Created(path, modified)
    }
    /// 방금 `to` 로 옮긴 항목. 지금 `to` 의 수정 시각을 함께 적는다.
    pub(crate) fn moved(from: PathBuf, to: PathBuf) -> Self {
        let made = modified_at(&to);
        Self::Moved { from, to, made }
    }
}

pub(crate) fn modified_at(path: &Path) -> Option<SystemTime> {
    path.symlink_metadata().and_then(|m| m.modified()).ok()
}

impl Report {
    fn new(kind: OpKind, dest: Option<PathBuf>, total: usize) -> Self {
        Self {
            kind,
            dest,
            total,
            done: 0,
            undo: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
            cancelled: false,
            trash_unavailable: false,
            leftovers: Vec::new(),
            unprocessed: Vec::new(),
            staging_left: Vec::new(),
            resend: None,
        }
    }
    /// 같은 작업으로 다시 보낼 원래 경로: 건너뛴 항목, 원본 미삭제를 뺀 실패 항목, 취소로 하지 않은 항목.
    /// 원본 미삭제 항목은 [`Report::leftovers`] 로 원본 삭제만 다시 한다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn retryable(&self) -> Vec<PathBuf> {
        if let Some(resend) = &self.resend {
            return resend.paths.clone();
        }
        self.failed
            .iter()
            .filter(|f| !f.reason.leaves_original())
            .map(|f| f.path.clone())
            .chain(self.skipped.iter().cloned())
            .chain(self.unprocessed.iter().cloned())
            .collect()
    }
    /// Retry 가 원본 삭제만 다시 할 항목.
    #[cfg(feature = "gui")]
    pub(crate) fn retry_leftovers(&self) -> Vec<Leftover> {
        match &self.resend {
            Some(resend) => resend.leftovers.clone(),
            None => self.leftovers.clone(),
        }
    }
}

/// 작업 하나가 끝난 항목.
enum Outcome {
    Done(UndoStep),
    /// 같은 폴더로 옮기기처럼 바뀐 것이 없다.
    Unchanged,
    Skipped,
    Cancelled,
}

fn cancelled_error() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// 원본을 `dest_dir` 로 복사하거나 옮긴다. 원본마다 결과를 따로 남기고 다음 원본으로 간다.
/// 고른 폴더가 이미 나르는 안쪽 항목과 겹친 경로는 빼고 센다([`drop_nested`]).
pub(crate) fn run_transfer(
    shared: &Shared,
    sources: &[PathBuf],
    dest_dir: &Path,
    cut: bool,
) -> Report {
    let sources = &drop_nested(sources)[..];
    let kind = if cut { OpKind::Move } else { OpKind::Copy };
    let mut report = Report::new(kind, Some(dest_dir.to_path_buf()), sources.len());
    shared.items_total.store(sources.len(), Ordering::Release);
    let canonical_dest = match dest_dir.canonicalize() {
        Ok(dest) => dest,
        Err(e) => {
            report.failed = sources
                .iter()
                .map(|p| Failure {
                    path: p.clone(),
                    reason: Reason::Os(e.to_string()),
                })
                .collect();
            return report;
        }
    };
    shared.transfer.store(true, Ordering::Release);
    shared.counting.store(true, Ordering::Release);
    shared.wake_now();
    let mut found = 0;
    let mut sizes = Vec::with_capacity(sources.len());
    for source in sources {
        match count::measure(shared, source, &mut found) {
            Some(size) => sizes.push(size),
            None => {
                shared.counting.store(false, Ordering::Release);
                report.cancelled = true;
                report.unprocessed = sources.to_vec();
                return report;
            }
        }
    }
    shared.bytes_total.store(
        sizes
            .iter()
            .try_fold(0u64, |a, b| a.checked_add(b.bytes?))
            .unwrap_or(UNKNOWN_BYTES),
        Ordering::Release,
    );
    shared.files_total.store(found, Ordering::Release);
    shared.counting.store(false, Ordering::Release);
    shared.wake_now();
    let mut conflicts_left = sources
        .iter()
        .filter(|s| conflicts(s, &canonical_dest, cut))
        .count();
    let mut sticky: Option<Choice> = None;
    let mut staging_left = Vec::new();
    let mut base = 0u64;
    let mut files_base = 0usize;
    for (index, (source, size)) in sources.iter().zip(sizes).enumerate() {
        if shared.cancelled() {
            report.cancelled = true;
            report.unprocessed = sources[index..].to_vec();
            break;
        }
        shared.set_current(size.first_file.as_deref().unwrap_or(source));
        let mut item = Item {
            shared,
            dest: &canonical_dest,
            shown_dest: dest_dir,
            cut,
            sticky: &mut sticky,
            conflicts_left: &mut conflicts_left,
            staging_left: &mut staging_left,
            base,
        };
        // 멈추는 것은 취소뿐이다. 멈춘 항목은 사본을 지웠으므로 남은 항목과 함께 하지 않은 것으로 둔다.
        if record(&mut report, shared, source, item.run(source)).is_break() {
            report.unprocessed = sources[index..].to_vec();
            break;
        }
        base = base.saturating_add(size.bytes.unwrap_or(0));
        files_base = files_base.saturating_add(size.files);
        shared.files_done.store(files_base, Ordering::Release);
        shared.finish_item(base);
    }
    report.staging_left = staging_left;
    report
}

/// 항목 하나의 결과를 보고에 더한다. 작업을 멈춰야 하면 Break.
fn record(
    report: &mut Report,
    shared: &Shared,
    source: &Path,
    result: Result<Outcome, ItemError>,
) -> std::ops::ControlFlow<()> {
    match result {
        Ok(Outcome::Done(step)) => {
            report.done += 1;
            report.undo.push(step);
        }
        Ok(Outcome::Unchanged) => report.done += 1,
        Ok(Outcome::Skipped) => report.skipped.push(source.to_path_buf()),
        Ok(Outcome::Cancelled) => {
            report.cancelled = true;
            return std::ops::ControlFlow::Break(());
        }
        // 원본이 일부만 지워졌을 수 있어 사본이 하나뿐인 온전한 자료다. 되돌리기 단계로 남기지 않는다.
        Err(ItemError::SourceNotRemoved { error, leftover }) => {
            report.done += 1;
            report.failed.push(Failure {
                path: source.to_path_buf(),
                reason: Reason::SourceNotRemoved(error),
            });
            report.leftovers.push(leftover);
        }
        Err(ItemError::Failed(reason)) => {
            if shared.cancelled() {
                report.cancelled = true;
                return std::ops::ControlFlow::Break(());
            }
            report.failed.push(Failure {
                path: source.to_path_buf(),
                reason,
            });
        }
    }
    std::ops::ControlFlow::Continue(())
}

/// 원본의 이름이 목적지에 이미 있어 물어야 하는가.
fn conflicts(source: &Path, dest: &Path, cut: bool) -> bool {
    let Some(name) = source.file_name() else {
        return false;
    };
    if cut && same_parent(source, dest) {
        return false;
    }
    dest.join(name).symlink_metadata().is_ok()
}

fn same_parent(source: &Path, dest: &Path) -> bool {
    source
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .is_some_and(|p| p == dest)
}

enum ItemError {
    Failed(Reason),
    SourceNotRemoved { error: String, leftover: Leftover },
}

impl From<io::Error> for ItemError {
    fn from(e: io::Error) -> Self {
        Self::Failed(Reason::Os(e.to_string()))
    }
}

/// 이름을 정한 뒤 공개하는 방법.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Publish {
    /// 같은 이름이 생기면 Keep both 이름으로 바꾼다. 기존 항목을 바꾸지 않는다.
    NoReplace,
    /// 사용자가 고른 파일 덮어쓰기.
    Replace,
}

struct Item<'a> {
    shared: &'a Shared,
    dest: &'a Path,
    /// 사용자가 연 목적지 경로. 질문에는 링크를 풀지 않은 이 경로를 보인다.
    shown_dest: &'a Path,
    cut: bool,
    sticky: &'a mut Option<Choice>,
    conflicts_left: &'a mut usize,
    staging_left: &'a mut Vec<Failure>,
    /// 앞서 끝난 항목들의 바이트 합. 이 항목의 복사량을 더해 진행을 맞춘다.
    base: u64,
}

impl Item<'_> {
    fn run(&mut self, source: &Path) -> Result<Outcome, ItemError> {
        let name = source
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
            .to_owned();
        let meta = source.symlink_metadata()?;
        if meta.is_dir() && self.dest.starts_with(source.canonicalize()?) {
            return Err(ItemError::Failed(Reason::IntoItself));
        }
        if self.cut && same_parent(source, self.dest) {
            return Ok(Outcome::Unchanged);
        }
        let mut target = self.dest.join(&name);
        let mut publish = Publish::NoReplace;
        if let Some(existing) = EntryFacts::read(&target) {
            let folder_conflict = existing.is_dir || meta.is_dir();
            *self.conflicts_left = self.conflicts_left.saturating_sub(1);
            let choice = match *self.sticky {
                Some(Choice::Replace) if folder_conflict => self.ask(&name, existing, source, true),
                Some(choice) => choice,
                None => self.ask(&name, existing, source, folder_conflict),
            };
            match choice {
                Choice::Skip => return Ok(Outcome::Skipped),
                Choice::CancelRest => {
                    self.shared.cancel();
                    return Ok(Outcome::Cancelled);
                }
                Choice::Replace if !folder_conflict => publish = Publish::Replace,
                Choice::Replace | Choice::KeepBoth => {
                    target = unique_dest(self.dest, &name.to_string_lossy());
                }
            }
        }
        if self.cut
            && let Some(outcome) = self.rename(source, &mut target, publish)?
        {
            return Ok(outcome);
        }
        self.copy(source, &meta, target, publish)
    }

    fn ask(
        &mut self,
        name: &std::ffi::OsStr,
        existing: EntryFacts,
        source: &Path,
        folder_conflict: bool,
    ) -> Choice {
        let incoming = EntryFacts::read(source).unwrap_or(EntryFacts {
            is_dir: false,
            size: None,
            items: None,
            modified: None,
        });
        let answer = self.shared.decide(Ask {
            name: name.to_string_lossy().into_owned(),
            folder: self.shown_dest.to_path_buf(),
            existing,
            incoming,
            folder_conflict,
            remaining: *self.conflicts_left,
        });
        let choice = if folder_conflict && answer.choice == Choice::Replace {
            Choice::KeepBoth
        } else {
            answer.choice
        };
        if answer.apply_all {
            *self.sticky = Some(answer.choice);
        }
        choice
    }

    /// 같은 디스크면 이름만 바꿔 옮긴다. 다른 디스크면 None 을 돌려 복사로 넘긴다.
    fn rename(
        &self,
        source: &Path,
        target: &mut PathBuf,
        publish: Publish,
    ) -> Result<Option<Outcome>, ItemError> {
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        loop {
            let replace = publish == Publish::Replace && !is_dir(target);
            let result = if replace {
                std::fs::rename(source, &*target)
            } else {
                rename_noreplace(source, target)
            };
            match result {
                Ok(()) => {
                    let step = if replace {
                        UndoStep::Replaced(target.clone())
                    } else {
                        UndoStep::moved(source.to_path_buf(), target.clone())
                    };
                    return Ok(Some(Outcome::Done(step)));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    *target = unique_dest(self.dest, &name);
                }
                Err(e) if e.kind() == io::ErrorKind::CrossesDevices => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// 목적지 폴더 안의 숨은 임시 폴더에서 사본을 만든 뒤 공개한다. 취소·실패하면 임시 폴더째 지운다.
    fn copy(
        &mut self,
        source: &Path,
        meta: &std::fs::Metadata,
        target: PathBuf,
        publish: Publish,
    ) -> Result<Outcome, ItemError> {
        let staging = tempfile::Builder::new()
            .prefix(".tasty-copy-")
            .tempdir_in(self.dest)?;
        let result = self.copy_staged(staging.path(), source, meta, target, publish);
        close_staging(staging, self.staging_left);
        result
    }

    fn copy_staged(
        &self,
        staging: &Path,
        source: &Path,
        meta: &std::fs::Metadata,
        mut target: PathBuf,
        publish: Publish,
    ) -> Result<Outcome, ItemError> {
        let staged = staging.join("entry");
        let mut copied = 0u64;
        let mut modes = FolderModes::default();
        if let Err(e) = copy_tree(
            self.shared,
            source,
            meta,
            &staged,
            (&mut copied, self.base),
            &mut modes,
        ) {
            if self.shared.cancelled() {
                return Ok(Outcome::Cancelled);
            }
            return Err(e.into());
        }
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replaced = loop {
            let replace = publish == Publish::Replace && !is_dir(&target);
            let result = if replace {
                std::fs::rename(&staged, &target)
            } else {
                rename_noreplace(&staged, &target)
            };
            match result {
                Ok(()) => {
                    modes.apply(&staged, &target);
                    break replace;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    target = unique_dest(self.dest, &name);
                }
                Err(e) => return Err(e.into()),
            }
        };
        let leftover = self.cut.then(|| Leftover::before_remove(source, &target));
        let step = if replaced {
            UndoStep::Replaced(target.clone())
        } else if self.cut {
            UndoStep::moved(source.to_path_buf(), target.clone())
        } else {
            UndoStep::created(target)
        };
        if let Some(leftover) = leftover
            && let Err(e) = remove_path(source)
        {
            return Err(ItemError::SourceNotRemoved {
                error: e.to_string(),
                leftover,
            });
        }
        Ok(Outcome::Done(step))
    }
}

/// 임시 폴더를 지운다. drop 은 정리 오류를 버리므로 직접 닫고, 실패하면 경고 로그와 결과에 남긴다.
fn close_staging(staging: tempfile::TempDir, left: &mut Vec<Failure>) {
    let path = staging.path().to_path_buf();
    if let Err(e) = staging.close() {
        tracing::warn!(path = %path.display(), error = %e, "Explorer could not remove a staging folder");
        left.push(Failure {
            path,
            reason: Reason::Os(e.to_string()),
        });
    }
}

fn is_dir(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.is_dir())
}

/// 새 경로에 사본을 만든다. 원본 링크를 따라가지 않고 기존 트리에 합치지 않는다.
fn copy_tree(
    shared: &Shared,
    src: &Path,
    meta: &std::fs::Metadata,
    dst: &Path,
    (copied, base): (&mut u64, u64),
    modes: &mut FolderModes,
) -> io::Result<()> {
    if shared.cancelled() {
        return Err(cancelled_error());
    }
    if !meta.is_dir() {
        shared.set_current(src);
    }
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(src)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, dst)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::{FileTypeExt, symlink_dir, symlink_file};
            if meta.file_type().is_symlink_dir() {
                symlink_dir(target, dst)?;
            } else {
                symlink_file(target, dst)?;
            }
        }
    } else if meta.is_dir() {
        std::fs::create_dir(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let meta = path.symlink_metadata()?;
            copy_tree(
                shared,
                &path,
                &meta,
                &dst.join(entry.file_name()),
                (copied, base),
                modes,
            )?;
        }
        modes.push(dst, meta);
    } else if meta.is_file() {
        let mut input = std::fs::File::open(src)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dst)?;
        let mut buf = vec![0u8; COPY_CHUNK];
        loop {
            if shared.cancelled() {
                return Err(cancelled_error());
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            #[cfg(test)]
            if let Some((after, code)) = *lock(&shared.write_failure_after)
                && shared.bytes_done.load(Ordering::Acquire) >= after
            {
                return Err(io::Error::from_raw_os_error(code));
            }
            output.write_all(&buf[..n])?;
            *copied = copied.saturating_add(n as u64);
            shared.add_bytes(n as u64);
        }
        keep_file_meta(&output, dst, meta)?;
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot copy a special file",
        ));
    }
    if !meta.is_dir() {
        shared.files_done.fetch_add(1, Ordering::AcqRel);
        shared.wake_throttled();
    }
    // 항목 경계에서 진행 바이트를 앞선 항목 합 + 이 항목 복사량으로 맞춘다.
    shared
        .bytes_done
        .store(base.saturating_add(*copied), Ordering::Release);
    Ok(())
}

/// 항목을 하나씩 OS 휴지통으로 보낸다. 휴지통이 받지 않아도 영구 삭제로 대신하지 않는다.
/// 고른 폴더와 함께 가는 안쪽 항목은 따로 보내지 않는다.
pub(crate) fn run_trash(shared: &Shared, paths: &[PathBuf]) -> Report {
    let paths = &drop_nested(paths)[..];
    let mut report = Report::new(OpKind::Trash, None, paths.len());
    shared.items_total.store(paths.len(), Ordering::Release);
    let mut refused = 0usize;
    for (index, path) in paths.iter().enumerate() {
        if shared.cancelled() {
            report.cancelled = true;
            report.unprocessed = paths[index..].to_vec();
            break;
        }
        shared.set_current(path);
        match os_trash::send(shared, path) {
            Ok(()) => report.done += 1,
            Err(e) => {
                if trash_refused(&e, path) {
                    refused += 1;
                }
                report.failed.push(Failure {
                    path: path.clone(),
                    reason: Reason::Os(e.to_string()),
                });
            }
        }
        shared.finish_item(0);
    }
    report.trash_unavailable =
        report.done == 0 && !report.failed.is_empty() && refused == report.failed.len();
    report
}

/// 항목은 읽을 수 있는데 휴지통이 받지 않았는가. 없는 항목·권한 문제는 휴지통 탓이 아니다.
fn trash_refused(error: &trash::Error, path: &Path) -> bool {
    !matches!(
        error,
        trash::Error::CouldNotAccess { .. }
            | trash::Error::TargetedRoot
            | trash::Error::CanonicalizePath { .. }
            | trash::Error::ConvertOsString { .. }
    ) && path.symlink_metadata().is_ok()
}

/// 끝난 복사·이동을 뒤에서부터 되돌린다. 되돌릴 자리에 다른 항목이 있으면 그 항목은 남긴다.
pub(crate) fn run_undo(shared: &Shared, steps: &[UndoStep]) -> Report {
    let mut report = Report::new(OpKind::Undo, None, steps.len());
    shared.items_total.store(steps.len(), Ordering::Release);
    for step in steps.iter().rev() {
        if shared.cancelled() {
            report.cancelled = true;
            break;
        }
        let (path, result) = match step {
            UndoStep::Created(path, _) | UndoStep::Made(path, _) => {
                shared.set_current(path);
                let result = match super::named::blocked(step) {
                    Some(reason) => Err(reason),
                    None => os_trash::send(shared, path).map_err(|e| Reason::Os(e.to_string())),
                };
                (path, result)
            }
            UndoStep::Moved { from, to, made } => {
                shared.set_current(to);
                (from, move_back(to, from, *made, &mut report.staging_left))
            }
            UndoStep::Replaced(path) => (path, Err(Reason::Replaced)),
        };
        match result {
            Ok(()) => report.done += 1,
            Err(reason) => report.failed.push(Failure {
                path: path.clone(),
                reason,
            }),
        }
        shared.finish_item(0);
    }
    report
}

/// `to` 를 정확히 `from` 자리로 옮긴다. 그 자리에 무엇이 있거나, `to` 가 옮긴 뒤 바뀌었으면
/// 옮기지 않는다.
fn move_back(
    to: &Path,
    from: &Path,
    made: Option<SystemTime>,
    staging_left: &mut Vec<Failure>,
) -> Result<(), Reason> {
    if to.symlink_metadata().is_err() {
        return Err(Reason::Gone);
    }
    if super::named::taken(from, to) {
        return Err(Reason::NewerThere);
    }
    if modified_at(to) != made {
        return Err(Reason::ChangedAfterMove);
    }
    match rename_noreplace(to, from) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(Reason::NewerThere),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {}
        Err(e) => return Err(Reason::Os(e.to_string())),
    }
    let parent = from
        .parent()
        .ok_or_else(|| Reason::Os("no parent".into()))?;
    let staging = tempfile::Builder::new()
        .prefix(".tasty-copy-")
        .tempdir_in(parent)
        .map_err(|e| Reason::Os(e.to_string()))?;
    let result = move_back_staged(staging.path(), to, from);
    close_staging(staging, staging_left);
    result
}

fn move_back_staged(staging: &Path, to: &Path, from: &Path) -> Result<(), Reason> {
    let staged = staging.join("entry");
    let meta = to
        .symlink_metadata()
        .map_err(|e| Reason::Os(e.to_string()))?;
    let quiet = Shared::fixed(Choice::KeepBoth);
    let mut modes = FolderModes::default();
    copy_tree(&quiet, to, &meta, &staged, (&mut 0, 0), &mut modes)
        .map_err(|e| Reason::Os(e.to_string()))?;
    match rename_noreplace(&staged, from) {
        Ok(()) => modes.apply(&staged, from),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(Reason::NewerThere),
        Err(e) => return Err(Reason::Os(e.to_string())),
    }
    remove_path(to).map_err(|e| Reason::SourceNotRemoved(e.to_string()))
}

#[path = "copy_meta.rs"]
mod copy_meta;
#[path = "os_trash.rs"]
mod os_trash;
use copy_meta::{FolderModes, keep_file_meta};
#[path = "leftover.rs"]
pub(crate) mod leftover;

#[cfg(test)]
#[path = "job_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "storage_failure_tests.rs"]
mod storage_failure_tests;

#[cfg(test)]
#[path = "os_trash_tests.rs"]
mod os_trash_tests;
