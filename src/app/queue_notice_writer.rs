//! 큐 도착 알림 줄을 이벤트 루프 밖에서 notify 로그에 쓴다.
//!
//! IPC 핸들러는 이벤트 루프에서 실행되므로 파일 I/O(잠금 대기 포함)를 직접 하지 않고 이 writer에
//! 줄을 넘긴다. 전용 스레드는 첫 요청 때 만든다. 경로는 호스트의 데이터 루트(`tasty_home()`)로
//! 정하고 호스트 환경의 TASTY_PARENT_HOME은 읽지 않는다.
//!
//! 종료가 시작되면 아직 쓰지 않은 줄은 버리고 버린 수를 로그에 남긴다. 다음 부팅이 notify
//! 디렉터리를 비우므로 남은 줄을 기다려 쓸 이유가 없다.

use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use tasty_utils::notify::NoticeType;

/// 시험 구성이 모은 (받는 surface, 표지 붙은 줄).
#[cfg(test)]
pub(crate) type RecordedLines = Arc<Mutex<Vec<(u32, String)>>>;

struct Job {
    surface: u32,
    line: String,
}

#[derive(Default)]
struct Counters {
    /// 넘겨받았지만 아직 끝나지 않은 줄(쓰는 중인 줄 포함).
    queued: usize,
    /// 지금 쓰는 줄이 있는지. 종료 때 버린 수에서 뺀다.
    writing: bool,
    closing: bool,
}

#[derive(Default)]
struct Shared {
    counters: Mutex<Counters>,
    settled: Condvar,
    /// counters 잠금 poison을 처음 한 번만 보고한다.
    poison_reported: std::sync::atomic::AtomicBool,
    /// 시험이 쓰기 직전에 worker를 멈춰 핸들러가 쓰기를 기다리지 않음을 확인한다.
    #[cfg(test)]
    gate: Mutex<()>,
}

impl Shared {
    /// 계수만 담은 잠금이라 poison 뒤에도 값을 그대로 쓴다. 처음 한 번 보고한다.
    fn counters(&self) -> std::sync::MutexGuard<'_, Counters> {
        tasty_utils::poison::recover_mutex(
            self.counters.lock(),
            "queue notice counters",
            &self.poison_reported,
        )
    }
}

enum Sink {
    Thread {
        home: Option<PathBuf>,
        tx: OnceLock<Sender<Job>>,
    },
    #[cfg(test)]
    Recording(RecordedLines),
}

pub(crate) struct QueueNoticeWriter {
    sink: Sink,
    shared: Arc<Shared>,
}

impl QueueNoticeWriter {
    /// `home`은 호스트의 데이터 루트다. None이면 쓰지 않고 경고만 남긴다.
    pub(crate) fn new(home: Option<PathBuf>) -> Self {
        Self {
            sink: Sink::Thread {
                home,
                tx: OnceLock::new(),
            },
            shared: Arc::default(),
        }
    }

    /// 시험용: 파일 대신 표지 붙은 줄을 모은다.
    #[cfg(test)]
    pub(crate) fn recording() -> (Self, RecordedLines) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                sink: Sink::Recording(lines.clone()),
                shared: Arc::default(),
            },
            lines,
        )
    }

    /// `surface`의 notify 로그에 `[queue]` 줄을 쓰도록 넘긴다. 쓰기를 기다리지 않는다.
    pub(crate) fn submit(&self, surface: u32, line: String) {
        let (home, tx) = match &self.sink {
            #[cfg(test)]
            Sink::Recording(lines) => {
                let line = tasty_utils::notify::notice_line(NoticeType::Queue, &line);
                lines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((surface, line));
                return;
            }
            Sink::Thread { home, tx } => (home, tx),
        };
        let Some(home) = home.clone() else {
            tracing::warn!("queue notice for surface {surface} dropped: tasty_home() unavailable");
            return;
        };
        {
            let mut c = self.lock();
            if c.closing {
                tracing::debug!(
                    "queue notice for surface {surface} dropped: host is shutting down"
                );
                return;
            }
            c.queued += 1;
        }
        let Some(tx) = self.sender(tx, home) else {
            self.finish_one();
            return;
        };
        if let Err(e) = tx.send(Job { surface, line }) {
            tracing::warn!("queue notice for surface {surface} dropped: writer thread ended: {e}");
            self.finish_one();
        }
    }

    /// 종료를 시작한다. 아직 쓰지 않은 줄은 버리고 그 수를 로그에 남긴다. 버린 수를 돌려준다.
    pub(crate) fn begin_shutdown(&self) -> usize {
        let mut c = self.lock();
        c.closing = true;
        let discarded = c.queued - usize::from(c.writing);
        if discarded > 0 {
            tracing::info!(
                target: "tasty::shutdown",
                discarded,
                "queue notice lines not written at shutdown; the next boot clears the notify directory"
            );
        }
        discarded
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Counters> {
        self.shared.counters()
    }

    fn finish_one(&self) {
        let mut c = self.lock();
        c.queued = c.queued.saturating_sub(1);
        self.shared.settled.notify_all();
    }

    fn sender<'a>(&self, tx: &'a OnceLock<Sender<Job>>, home: PathBuf) -> Option<&'a Sender<Job>> {
        if let Some(tx) = tx.get() {
            return Some(tx);
        }
        let (new_tx, rx) = channel::<Job>();
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new()
            .name("queue-notice".into())
            .spawn(move || run(home, rx, shared));
        if let Err(e) = spawned {
            tracing::warn!("queue notice writer thread failed to start: {e}");
            return None;
        }
        // 이벤트 루프 한 스레드만 부르므로 경합하지 않는다. 경합해도 먼저 넣은 쪽을 쓴다.
        Some(tx.get_or_init(|| new_tx))
    }

    /// 시험용: 넘긴 줄이 모두 끝날 때까지 기다린다.
    #[cfg(test)]
    fn wait_settled(&self) {
        let mut c = self.lock();
        while c.queued > 0 {
            c = self
                .shared
                .settled
                .wait(c)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
}

fn run(home: PathBuf, rx: std::sync::mpsc::Receiver<Job>, shared: Arc<Shared>) {
    while let Ok(job) = rx.recv() {
        #[cfg(test)]
        drop(shared.gate.lock().unwrap_or_else(|e| e.into_inner()));
        {
            let mut c = shared.counters();
            if c.closing {
                c.queued = c.queued.saturating_sub(1);
                shared.settled.notify_all();
                continue;
            }
            c.writing = true;
        }
        if let Err(e) = tasty_utils::notify::append_notice_under(
            &home,
            job.surface,
            NoticeType::Queue,
            &job.line,
        ) {
            tracing::warn!(
                "queue notice for surface {} not written under {}: {e}",
                job.surface,
                home.display()
            );
        }
        let mut c = shared.counters();
        c.writing = false;
        c.queued = c.queued.saturating_sub(1);
        shared.settled.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(home: &std::path::Path, surface: u32) -> Option<String> {
        std::fs::read_to_string(tasty_utils::notify::notify_log_path_under(home, surface)).ok()
    }

    #[test]
    fn a_submitted_line_lands_in_the_given_home_with_the_queue_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let w = QueueNoticeWriter::new(Some(tmp.path().to_path_buf()));
        w.submit(7, "surface 7: 1".into());
        w.wait_settled();
        assert_eq!(
            read(tmp.path(), 7).as_deref(),
            Some("[queue] surface 7: 1\n")
        );
    }

    /// 쓰기가 막혀 있어도 submit은 바로 돌아온다. 핸들러(이벤트 루프)가 파일 I/O를 기다리지 않는다.
    #[test]
    fn submit_returns_before_the_write_happens() {
        let tmp = tempfile::tempdir().unwrap();
        let w = QueueNoticeWriter::new(Some(tmp.path().to_path_buf()));
        let gate = w.shared.gate.lock().unwrap();
        w.submit(7, "surface 7: 1".into());
        assert_eq!(read(tmp.path(), 7), None);
        drop(gate);
        w.wait_settled();
        assert!(read(tmp.path(), 7).is_some());
    }

    #[test]
    fn shutdown_discards_unwritten_lines_and_counts_them() {
        let tmp = tempfile::tempdir().unwrap();
        let w = QueueNoticeWriter::new(Some(tmp.path().to_path_buf()));
        let gate = w.shared.gate.lock().unwrap();
        w.submit(7, "a".into());
        w.submit(7, "b".into());
        assert_eq!(w.begin_shutdown(), 2);
        drop(gate);
        w.wait_settled();
        assert_eq!(read(tmp.path(), 7), None);
        w.submit(7, "c".into());
        w.wait_settled();
        assert_eq!(read(tmp.path(), 7), None);
    }

    #[test]
    fn without_a_home_nothing_is_written_or_counted() {
        let w = QueueNoticeWriter::new(None);
        w.submit(7, "a".into());
        assert_eq!(w.begin_shutdown(), 0);
    }
}
