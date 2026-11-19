//! 윈도우(engine)마다 사본을 두면 저장할 때 다른 윈도우의 변경을 지우는 프로세스 공용 목록.
//! 원본은 [`SharedList`] 하나이고 engine은 그리기용 [`Replica`]만 가진다. 변경은 원본에만 하고
//! 저장도 원본 전체로 한다. 사본은 원본 리비전이 바뀌면 다시 복사한다.

#[cfg(test)]
mod dirty_age_tests;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tasty_utils::batch_writer::{Clock, SystemClock};

use super::file_snapshot::{Document, PendingSave, Snapshot, Submit};

/// 파일 하나에 통째로 저장하는 목록. `load`는 읽기 실패를 빈 목록으로 처리한다.
pub(crate) trait PersistedList:
    Clone + Default + serde::Serialize + serde::de::DeserializeOwned
{
    /// 경고 로그에 쓰는 이름.
    const NAME: &'static str;
    fn config_path() -> Option<std::path::PathBuf>;
}

pub(crate) struct SharedList<T> {
    inner: Mutex<SharedState<T>>,
    clock: Arc<dyn Clock>,
}
impl<T: Default> Default for SharedList<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}

#[derive(Default)]
struct SharedState<T> {
    revision: u64,
    list: T,
    committed: u64,
    document: Option<Arc<Document>>,
    sink: Option<Submit>,
    pending: Vec<(u64, Box<dyn PendingSave>)>,
    errors: Vec<String>,
    retried: bool,
    shutdown_reported: bool,
    dirty_since: Option<Instant>,
}

impl<T: PersistedList> SharedList<T> {
    pub(crate) fn load() -> Self {
        let Some(path) = T::config_path() else {
            return Self::from(T::default());
        };
        Self::load_at(path)
    }

    pub(crate) fn load_at(path: std::path::PathBuf) -> Self {
        Self::load_at_with_clock(path, Arc::new(SystemClock))
    }
    pub(crate) fn load_at_with_clock(path: std::path::PathBuf, clock: Arc<dyn Clock>) -> Self {
        let (list, document) = Document::load::<T>(path);
        let mut source = Self::from(list);
        source.clock = clock;
        source.lock().document = Some(document);
        source
    }

    fn lock(&self) -> MutexGuard<'_, SharedState<T>> {
        // 변경은 목록 연산과 snapshot 접수뿐이라 중간에 멈춰도 목록 자체는 쓸 수 있다.
        self.inner.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("{}: lock was poisoned; using the last list", T::NAME);
            poisoned.into_inner()
        })
    }

    /// 원본 변경과 revision·snapshot 접수 순서만 잠금 안에서 정한다. 파일 I/O는 worker가 한다.
    pub(crate) fn update(&self, change: impl FnOnce(&mut T)) {
        let mut state = self.lock();
        change(&mut state.list);
        state.revision += 1;
        state.dirty_since.get_or_insert_with(|| self.clock.now());
        Self::enqueue(&mut state);
    }

    pub(crate) fn install_sink(&self, sink: Submit) {
        self.lock().sink = Some(sink);
    }

    fn enqueue(state: &mut SharedState<T>) {
        let Some(document) = state.document.clone() else {
            state
                .errors
                .push("favorites file path is unavailable".into());
            return;
        };
        let saved = (|| {
            let bytes = toml::to_string_pretty(&state.list)
                .map_err(|e| e.to_string())?
                .into_bytes();
            let sink = state.sink.as_ref().ok_or("file writer is unavailable")?;
            sink(Snapshot {
                key: T::NAME,
                document,
                bytes,
            })
        })();
        match saved {
            Ok(receipt) => state.pending.push((state.revision, receipt)),
            Err(error) => state.errors.push(error),
        }
    }

    /// Receipt를 기다리지 않고 완료된 revision만 확정한다. 오류는 한 번만 반환한다.
    pub(crate) fn poll(&self) -> Vec<String> {
        let mut state = self.lock();
        let mut index = 0;
        while index < state.pending.len() {
            let Some(result) = state.pending[index].1.poll() else {
                index += 1;
                continue;
            };
            let (revision, _) = state.pending.swap_remove(index);
            match result {
                Ok(()) => state.committed = state.committed.max(revision),
                Err(error) => state.errors.push(error),
            }
        }
        if state.committed >= state.revision {
            state.dirty_since = None;
        }
        std::mem::take(&mut state.errors)
    }

    pub(crate) fn retry_latest_before_shutdown(&self) {
        let mut state = self.lock();
        if std::mem::replace(&mut state.retried, true) {
            return;
        }
        if state.revision > state.committed
            && !state.pending.iter().any(|(r, _)| *r == state.revision)
        {
            Self::enqueue(&mut state);
        }
    }

    pub(crate) fn finish_shutdown(&self) -> Option<(u64, u64)> {
        let mut state = self.lock();
        if std::mem::replace(&mut state.shutdown_reported, true) {
            return None;
        }
        (state.revision > state.committed).then_some((state.revision, state.committed))
    }

    pub(crate) fn save_status(&self) -> (u64, u64, usize) {
        let state = self.lock();
        (state.revision, state.committed, state.pending.len())
    }
    pub(crate) fn dirty_age_us(&self) -> Option<u64> {
        self.lock().dirty_since.map(|since| {
            u64::try_from(
                self.clock
                    .now()
                    .saturating_duration_since(since)
                    .as_micros(),
            )
            .unwrap_or(u64::MAX)
        })
    }

    pub(crate) fn revision(&self) -> u64 {
        self.lock().revision
    }

    /// 사본을 만들지 않고 원본 목록을 읽는다.
    pub(crate) fn read<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.lock().list)
    }

    /// 현재 리비전과 목록 사본.
    pub(crate) fn copy(&self) -> (u64, T) {
        let state = self.lock();
        (state.revision, state.list.clone())
    }

    /// `known` 리비전 뒤에 바뀌었으면 현재 리비전과 목록 사본을 돌려준다.
    fn copy_if_newer(&self, known: u64) -> Option<(u64, T)> {
        let state = self.lock();
        (known != state.revision).then(|| (state.revision, state.list.clone()))
    }
}

impl<T: Default> From<T> for SharedList<T> {
    fn from(list: T) -> Self {
        Self {
            clock: Arc::new(SystemClock),
            inner: Mutex::new(SharedState {
                list,
                ..Default::default()
            }),
        }
    }
}

/// engine이 그리기에 쓰는 사본. 읽기만 `Deref`로 열고 변경은 [`Replica::change`]로 원본에 한다.
pub(crate) struct Replica<T> {
    list: T,
    revision: u64,
    source: Arc<SharedList<T>>,
}

impl<T: PersistedList> Replica<T> {
    pub(crate) fn new(source: Arc<SharedList<T>>) -> Self {
        let (revision, list) = source.copy();
        Self {
            list,
            revision,
            source,
        }
    }

    /// 원본을 바꾸고 이 사본을 맞춘다. 다른 engine의 사본은 다음 그리기 전에 맞춘다.
    pub(crate) fn change(&mut self, change: impl FnOnce(&mut T)) {
        self.source.update(change);
        self.sync();
    }

    /// 원본이 이 사본 뒤에 바뀌었으면 다시 복사하고 true를 돌려준다.
    pub(crate) fn sync(&mut self) -> bool {
        let Some((revision, list)) = self.source.copy_if_newer(self.revision) else {
            return false;
        };
        self.list = list;
        self.revision = revision;
        true
    }
}

impl<T> std::ops::Deref for Replica<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.list
    }
}
