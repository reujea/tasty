//! 셸 명령 훅(surface 훅 셸·수동 셸 dispatch·전역 훅)이 함께 쓰는 실행기.
//! 작업을 맡는 슬롯은 최대 [`RUNNING_LIMIT`]개이고, 슬롯이 모두 찼을 때만 [`WAITING_LIMIT`]개까지
//! 대기열에 넣는다. 그보다 많으면 실행하지 않고 호출자가 알림이나 오류로 알린다.
//! 슬롯 하나는 실행기 스레드 하나가 자식을 띄워 종료까지 기다린다. 대기열이 비면 그 스레드는 끝난다.
//!
//! 보장: 살아 있는 직접 자식은 최대 [`RUNNING_LIMIT`]개이고, 대기열에서 꺼내는 순서는 접수 순서다.
//! 여러 스레드가 꺼낸 뒤 실제 프로세스가 시작·종료되는 순서는 보장하지 않는다.

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::hook_handler::exec::ShellEvent;

/// 동시에 작업을 맡는 슬롯 수. 근거는 [훅 문서](../../docs/features/hooks/index.md#셸-명령-실행-상한)에 있다.
pub(crate) const RUNNING_LIMIT: usize = 32;
/// 슬롯이 모두 찼을 때 기다릴 수 있는 작업 수. IpcSequence 대기 상한과 같은 값이다.
pub(crate) const WAITING_LIMIT: usize = tasty_ipc::admission::INJECTED_DEPTH_LIMIT;

/// 실행기에 넣는 작업. Command는 실행 직전에 만들어 그때의 상속 환경 제거 규칙을 쓴다.
pub(crate) struct ShellJob {
    label: String,
    build: Box<dyn FnOnce() -> std::process::Command + Send>,
    report: Box<dyn FnMut(ShellEvent) + Send>,
}

impl ShellJob {
    pub(crate) fn new(
        label: impl Into<String>,
        build: impl FnOnce() -> std::process::Command + Send + 'static,
        report: impl FnMut(ShellEvent) + Send + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            build: Box::new(build),
            report: Box::new(report),
        }
    }
}

/// 실행기가 작업을 받지 않은 이유. 문구는 로그와 `-32603` 응답에 쓰는 영어다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellNotQueued {
    Full,
    WorkerUnavailable(String),
}

impl std::fmt::Display for ShellNotQueued {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(
                f,
                "{RUNNING_LIMIT} hook commands are running and {WAITING_LIMIT} are already waiting"
            ),
            Self::WorkerUnavailable(e) => write!(f, "the hook command thread could not start: {e}"),
        }
    }
}

#[derive(Default)]
struct PoolState {
    waiting: VecDeque<ShellJob>,
    /// 작업을 맡은 슬롯 수. 대기열이 비어 있지 않으면 항상 상한과 같다.
    running: usize,
}

pub(crate) struct ShellPool {
    state: Mutex<PoolState>,
    running_limit: usize,
    waiting_limit: usize,
}

static POISONED: AtomicBool = AtomicBool::new(false);

impl ShellPool {
    pub(crate) fn new(running_limit: usize, waiting_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PoolState::default()),
            running_limit,
            waiting_limit,
        })
    }

    fn lock(&self) -> MutexGuard<'_, PoolState> {
        crate::poison::recover_mutex(self.state.lock(), "hook shell pool", &POISONED)
    }

    /// 접수가 확정된 뒤에만 `make`를 부른다. 거절하면 `make`는 불리지 않는다.
    /// 슬롯이 남으면 새 실행기 스레드에 넘기고, 모두 찼으면 대기열에 넣는다.
    pub(crate) fn submit(
        self: &Arc<Self>,
        make: impl FnOnce() -> ShellJob,
    ) -> Result<(), ShellNotQueued> {
        let mut state = self.lock();
        if state.running < self.running_limit {
            // 스레드를 먼저 만들어, 만들지 못하면 작업을 만들지 않고 거절한다.
            let (tx, rx) = mpsc::channel::<ShellJob>();
            let pool = Arc::clone(self);
            std::thread::Builder::new()
                .name("hook-shell".into())
                .spawn(move || work(&pool, rx))
                .map_err(|e| ShellNotQueued::WorkerUnavailable(e.to_string()))?;
            state.running += 1;
            if tx.send(make()).is_err() {
                tracing::error!("hook shell thread stopped before its first job");
            }
            return Ok(());
        }
        if state.waiting.len() >= self.waiting_limit {
            return Err(ShellNotQueued::Full);
        }
        state.waiting.push_back(make());
        Ok(())
    }
}

fn work(pool: &Arc<ShellPool>, first: mpsc::Receiver<ShellJob>) {
    let mut next = first.recv().ok();
    loop {
        if let Some(job) = next.take() {
            run(job);
        }
        let mut state = pool.lock();
        match state.waiting.pop_front() {
            Some(job) => next = Some(job),
            None => {
                state.running -= 1;
                return;
            }
        }
    }
}

fn run(mut job: ShellJob) {
    let mut command = (job.build)();
    match tasty_utils::process::hide_console(&mut command).spawn() {
        Ok(mut child) => {
            (job.report)(ShellEvent::Started);
            let code = match child.wait() {
                Ok(status) => status.code(),
                Err(e) => {
                    tracing::warn!("hook command '{}' wait failed: {e}", job.label);
                    None
                }
            };
            tracing::debug!("hook command '{}' exited with {code:?}", job.label);
            (job.report)(ShellEvent::Exited(code));
        }
        Err(e) => {
            tracing::warn!("hook command '{}' spawn failed: {e}", job.label);
            (job.report)(ShellEvent::NotStarted(e.to_string()));
        }
    }
}

/// 제품의 세 경로가 함께 쓰는 실행기.
pub(crate) fn shared() -> &'static Arc<ShellPool> {
    static SHARED: OnceLock<Arc<ShellPool>> = OnceLock::new();
    SHARED.get_or_init(|| ShellPool::new(RUNNING_LIMIT, WAITING_LIMIT))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// 해제 파일이 생길 때까지 도는 자식. 시작할 때 살아 있는 자식 수를 `peaks`에 남기고
    /// 시작 순서를 `order`에 남긴다.
    fn held_job(dir: &Path, name: &str, done: mpsc::Sender<String>) -> ShellJob {
        let script = format!(
            "touch alive/{name}; ls alive | wc -l >> peaks; echo {name} >> order; \
             while [ ! -e release ]; do sleep 0.01; done; rm alive/{name}"
        );
        let dir = dir.to_path_buf();
        let name = name.to_string();
        ShellJob::new(
            name.clone(),
            move || {
                let mut c = std::process::Command::new("sh");
                c.args(["-c", &script]).current_dir(dir);
                c
            },
            move |event| {
                if let ShellEvent::Exited(_) | ShellEvent::NotStarted(_) = event {
                    let _ignored = done.send(name.clone());
                }
            },
        )
    }

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().to_path_buf();
        std::fs::create_dir(dir.join("alive")).expect("alive dir");
        (tmp, dir)
    }

    fn wait_lines(path: &Path, n: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let lines: Vec<String> = std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect();
            if lines.len() >= n || Instant::now() >= deadline {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn collect(rx: &mpsc::Receiver<String>, n: usize) -> Vec<String> {
        (0..n)
            .map(|_| rx.recv_timeout(Duration::from_secs(20)).expect("job ends"))
            .collect()
    }

    /// 슬롯 2·대기 3에 붙잡힌 자식 5개를 넣고 10개를 더 넣으면, 정확히 10개가 거절되고 실행되지 않는다.
    /// 살아 있는 자식은 어느 순간에도 2개를 넘지 않고, 해제하면 접수한 5개가 한 번씩 실행된다.
    #[test]
    fn jobs_beyond_the_slots_wait_and_jobs_beyond_the_queue_are_refused() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(2, 3);
        let (tx, rx) = mpsc::channel();
        for i in 0..5 {
            pool.submit(|| held_job(&dir, &format!("a{i}"), tx.clone()))
                .expect("accepted");
        }
        let mut refused = 0;
        for i in 0..10 {
            if pool.submit(|| held_job(&dir, &format!("x{i}"), tx.clone()))
                == Err(ShellNotQueued::Full)
            {
                refused += 1;
            }
        }
        assert_eq!(refused, 10, "the ten jobs past the queue must be refused");
        assert_eq!(
            wait_lines(&dir.join("order"), 2).len(),
            2,
            "two slots start"
        );
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            wait_lines(&dir.join("order"), 0).len(),
            2,
            "a queued job started while both slots were held"
        );
        std::fs::write(dir.join("release"), "").expect("release");
        let mut ended = collect(&rx, 5);
        ended.sort();
        assert_eq!(ended, ["a0", "a1", "a2", "a3", "a4"]);
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "a refused job ran"
        );
        let peaks = wait_lines(&dir.join("peaks"), 5);
        let max: usize = peaks
            .iter()
            .map(|l| l.trim().parse().unwrap())
            .max()
            .unwrap();
        assert!(
            max <= 2,
            "{max} children were alive at once (peaks {peaks:?})"
        );
        assert_eq!(
            pool.lock().running,
            0,
            "slots are returned once the queue is empty"
        );
    }

    /// 슬롯 안에서 넣으면 모두 바로 실행되고 거절은 없다.
    #[test]
    fn jobs_within_the_slots_all_run_without_refusal() {
        let (_tmp, dir) = setup();
        std::fs::write(dir.join("release"), "").expect("release");
        let pool = ShellPool::new(4, 0);
        let (tx, rx) = mpsc::channel();
        for i in 0..4 {
            pool.submit(|| held_job(&dir, &format!("a{i}"), tx.clone()))
                .expect("accepted");
        }
        assert_eq!(collect(&rx, 4).len(), 4);
    }

    /// 슬롯 하나면 꺼낸 순서가 곧 시작 순서라, 대기열이 접수 순서로 꺼내는지 볼 수 있다.
    #[test]
    fn the_queue_hands_out_jobs_in_the_order_they_arrived() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 8);
        let (tx, rx) = mpsc::channel();
        let names = ["q0", "q1", "q2", "q3", "q4"];
        for name in names {
            pool.submit(|| held_job(&dir, name, tx.clone()))
                .expect("accepted");
        }
        assert_eq!(wait_lines(&dir.join("order"), 1).len(), 1);
        std::fs::write(dir.join("release"), "").expect("release");
        collect(&rx, names.len());
        assert_eq!(wait_lines(&dir.join("order"), names.len()), names);
    }

    /// 거절하면 작업(수동 dispatch의 run 기록 포함)을 만들지 않고, 접수하면 한 번 만든다.
    #[test]
    fn a_refused_submit_never_builds_its_job() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 0);
        let (tx, rx) = mpsc::channel();
        let made = Arc::new(AtomicUsize::new(0));
        let make = |name: &str| {
            let made = Arc::clone(&made);
            let tx = tx.clone();
            let dir = dir.clone();
            let name = name.to_string();
            move || {
                made.fetch_add(1, Ordering::SeqCst);
                held_job(&dir, &name, tx)
            }
        };
        pool.submit(make("held")).expect("accepted");
        assert_eq!(made.load(Ordering::SeqCst), 1);
        assert_eq!(pool.submit(make("refused")), Err(ShellNotQueued::Full));
        assert_eq!(made.load(Ordering::SeqCst), 1, "a refused job was built");
        std::fs::write(dir.join("release"), "").expect("release");
        assert_eq!(collect(&rx, 1), ["held"]);
    }

    /// surface 훅 셸·전역 훅·수동 dispatch가 한 실행기의 슬롯과 대기열을 함께 쓴다.
    /// 슬롯 1을 붙잡고 전역 훅이 대기열 1을 채우면, surface 훅과 dispatch는 실행되지 않는다.
    #[test]
    fn the_three_hook_paths_share_one_pool() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 1);
        let (tx, rx) = mpsc::channel();
        pool.submit(|| held_job(&dir, "held", tx.clone()))
            .expect("accepted");
        assert_eq!(wait_lines(&dir.join("order"), 1).len(), 1);
        let mark = |name: &str| format!("touch {}", dir.join(name).display());
        super::super::global::submit_command_in(&pool, &mark("global"));
        super::super::trigger::submit_hook_shell(
            &pool,
            mark("surface"),
            Vec::new(),
            Vec::new(),
            crate::hook_runtime::notice::AutoOrigin::SurfaceHook { surface_id: 0 },
        );
        let dispatched = super::super::runs::submit_shell(&pool, "test/dispatch", {
            let path = dir.join("dispatch");
            move || {
                let mut c = std::process::Command::new("touch");
                c.arg(path);
                c
            }
        });
        assert_eq!(dispatched, Err(ShellNotQueued::Full));
        std::fs::write(dir.join("release"), "").expect("release");
        assert_eq!(collect(&rx, 1), ["held"]);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !dir.join("global").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            dir.join("global").exists(),
            "the queued global hook did not run"
        );
        while pool.lock().running > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!dir.join("surface").exists(), "a refused surface hook ran");
        assert!(!dir.join("dispatch").exists(), "a refused dispatch ran");
    }

    #[test]
    fn the_shared_pool_uses_the_documented_limits() {
        assert_eq!(shared().running_limit, RUNNING_LIMIT);
        assert_eq!(shared().waiting_limit, WAITING_LIMIT);
        assert_eq!((RUNNING_LIMIT, WAITING_LIMIT), (32, 256));
    }
}
