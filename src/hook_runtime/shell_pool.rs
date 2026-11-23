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
            let tx =
                spawn_worker(self).map_err(|e| ShellNotQueued::WorkerUnavailable(e.to_string()))?;
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

    /// 작업을 맡은 슬롯 수. 시험이 슬롯 반납을 관측할 때 쓴다.
    #[cfg(all(test, unix))]
    fn running(&self) -> usize {
        self.lock().running
    }

    /// 대기열의 작업 수. 시험이 미실행을 관측할 때 쓴다.
    #[cfg(all(test, unix))]
    fn waiting(&self) -> usize {
        self.lock().waiting.len()
    }
}

/// 첫 작업을 받을 채널과 함께 실행기 스레드를 만든다. 호출자가 슬롯 수를 센다.
fn spawn_worker(pool: &Arc<ShellPool>) -> std::io::Result<mpsc::Sender<ShellJob>> {
    let (tx, rx) = mpsc::channel::<ShellJob>();
    let pool = Arc::clone(pool);
    std::thread::Builder::new()
        .name("hook-shell".into())
        .spawn(move || work(&pool, rx))?;
    Ok(tx)
}

/// 실행기 스레드가 슬롯을 반납하지 못한 채 풀려 나가면(unwind) 대신 반납하고,
/// 대기열이 남아 있으면 그 작업을 맡을 스레드를 새로 세운다.
struct SlotGuard<'a> {
    pool: &'a Arc<ShellPool>,
    held: bool,
}

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        if !self.held {
            return;
        }
        tracing::error!("hook shell thread is unwinding; returning its slot");
        let mut state = self.pool.lock();
        let Some(mut job) = state.waiting.pop_front() else {
            state.running -= 1;
            return;
        };
        match spawn_worker(self.pool) {
            Ok(tx) => {
                if let Err(mpsc::SendError(mut job)) = tx.send(job) {
                    state.running -= 1;
                    (job.report)(ShellEvent::NotStarted(
                        "the hook shell thread stopped before the job".into(),
                    ));
                }
            }
            Err(e) => {
                state.running -= 1;
                tracing::error!("hook shell thread could not be replaced: {e}");
                (job.report)(ShellEvent::NotStarted(e.to_string()));
            }
        }
    }
}

fn work(pool: &Arc<ShellPool>, first: mpsc::Receiver<ShellJob>) {
    let mut guard = SlotGuard { pool, held: true };
    let mut next = first.recv().ok();
    loop {
        if let Some(job) = next.take() {
            run_caught(job);
        }
        let mut state = pool.lock();
        match state.waiting.pop_front() {
            Some(job) => next = Some(job),
            None => {
                state.running -= 1;
                guard.held = false;
                return;
            }
        }
    }
}

/// 작업 하나의 패닉(빌더·보고 콜백)이 슬롯을 잃게 하지 않는다. 이미 띄운 자식은
/// [`ChildGuard`]가 끝내고 회수한 뒤 같은 스레드가 다음 작업으로 넘어간다.
fn run_caught(job: ShellJob) {
    let label = job.label.clone();
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(job))).is_err() {
        tracing::error!("hook command '{label}' panicked; its slot moves to the next job");
    }
}

/// 패닉으로 풀려 나가는 동안 자식이 남지 않게 끝내고 회수한다.
struct ChildGuard<'a> {
    child: Option<std::process::Child>,
    label: &'a str,
}

impl Drop for ChildGuard<'_> {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Err(e) = child.kill() {
            tracing::warn!("hook command '{}' could not be killed: {e}", self.label);
        }
        match child.wait() {
            Ok(_) => tracing::error!(
                "hook command '{}' was killed because its job panicked",
                self.label
            ),
            Err(e) => tracing::error!(
                "hook command '{}' was not reaped after its job panicked: {e}",
                self.label
            ),
        }
    }
}

fn run(mut job: ShellJob) {
    let mut command = (job.build)();
    match tasty_utils::process::hide_console(&mut command).spawn() {
        Ok(child) => {
            let mut guard = ChildGuard {
                child: Some(child),
                label: &job.label,
            };
            (job.report)(ShellEvent::Started);
            let Some(child) = guard.child.as_mut() else {
                return;
            };
            let event = settle(&job.label, child);
            guard.child = None;
            (job.report)(event);
        }
        Err(e) => {
            tracing::warn!("hook command '{}' spawn failed: {e}", job.label);
            (job.report)(ShellEvent::NotStarted(e.to_string()));
        }
    }
}

/// 종료를 기다리고 끝낼 수 있는 자식. 시험이 대기 실패를 흉내 낼 때 바꿔 끼운다.
trait Reap {
    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus>;
    fn kill(&mut self) -> std::io::Result<()>;
}

impl Reap for std::process::Child {
    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        std::process::Child::wait(self)
    }
    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
}

/// 자식의 종료를 확인한다. 기다리지 못하면 끝내기를 시도한 뒤 한 번 더 기다리고,
/// 그래도 확인하지 못하면 [`ShellEvent::WaitFailed`]다. 어느 경우든 슬롯은 다음 작업으로 넘어간다.
fn settle(label: &str, child: &mut impl Reap) -> ShellEvent {
    let first = match child.wait() {
        Ok(status) => {
            tracing::debug!("hook command '{label}' exited with {:?}", status.code());
            return ShellEvent::Exited(status.code());
        }
        Err(e) => e,
    };
    tracing::warn!("hook command '{label}' wait failed: {first}; killing it");
    if let Err(e) = child.kill() {
        tracing::warn!("hook command '{label}' could not be killed: {e}");
    }
    match child.wait() {
        Ok(status) => ShellEvent::Exited(status.code()),
        Err(e) => {
            tracing::error!(
                "hook command '{label}' exit could not be confirmed: {e}; its slot moves on"
            );
            ShellEvent::WaitFailed(e.to_string())
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
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// 시험 작업이 보고한 (작업 이름, 결과 종류).
    type Report = (String, &'static str);

    fn kind(event: &ShellEvent) -> &'static str {
        match event {
            ShellEvent::Started => "started",
            ShellEvent::NotStarted(_) => "not_started",
            ShellEvent::Exited(_) => "exited",
            ShellEvent::WaitFailed(_) => "wait_failed",
        }
    }

    fn reporter(name: &str, tx: mpsc::Sender<Report>) -> impl FnMut(ShellEvent) + Send + 'static {
        let name = name.to_string();
        move |event| {
            let _ignored = tx.send((name.clone(), kind(&event)));
        }
    }

    /// 해제 파일이 생길 때까지 도는 자식. 시작할 때 살아 있는 자식 수를 `peaks`에 남기고
    /// 시작 순서를 `order`에 남긴다. 시험이 실패해 임시 폴더가 지워지거나 60초가 지나면 스스로 끝난다.
    fn held_job(dir: &Path, name: &str, tx: mpsc::Sender<Report>) -> ShellJob {
        let script = format!(
            "touch alive/{name}; ls alive | wc -l >> peaks; echo {name} >> order; i=0; \
             while [ ! -e release ] && [ -d alive ] && [ $i -lt 6000 ]; do sleep 0.01; i=$((i+1)); done; \
             rm -f alive/{name}"
        );
        let dir = dir.to_path_buf();
        ShellJob::new(
            name,
            move || {
                let mut c = std::process::Command::new("sh");
                c.args(["-c", &script]).current_dir(dir);
                c
            },
            reporter(name, tx),
        )
    }

    /// 시험이 끝나거나 실패해도 붙잡은 자식을 놓아준 뒤 임시 폴더를 지운다.
    struct Held(Option<tempfile::TempDir>);

    impl Drop for Held {
        fn drop(&mut self) {
            if let Some(tmp) = self.0.take() {
                if let Err(e) = std::fs::write(tmp.path().join("release"), "") {
                    tracing::warn!("could not release the held children: {e}");
                }
                drop(tmp);
            }
        }
    }

    fn setup() -> (Held, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().to_path_buf();
        std::fs::create_dir(dir.join("alive")).expect("alive dir");
        (Held(Some(tmp)), dir)
    }

    fn release(dir: &Path) {
        std::fs::write(dir.join("release"), "").expect("release");
    }

    /// 파일에 `n`줄이 생길 때까지 기다리고 정확히 `n`줄인지 단언한다.
    fn wait_lines(path: &Path, n: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let lines: Vec<String> = std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect();
            if lines.len() >= n || Instant::now() >= deadline {
                assert_eq!(lines.len(), n, "{}: {lines:?}", path.display());
                return lines;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// 모든 슬롯이 반납될 때까지 기다린다. 반납은 마지막 보고 뒤에 일어나므로, 이 뒤에는 더 올 보고가 없다.
    fn wait_idle(pool: &Arc<ShellPool>, within: Duration) {
        let deadline = Instant::now() + within;
        while pool.running() > 0 {
            assert!(
                Instant::now() < deadline,
                "{} slots still held after {within:?}",
                pool.running()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// 슬롯 반납 뒤 채널에 남은 보고를 작업별로 모은다.
    fn reports(rx: &mpsc::Receiver<Report>) -> BTreeMap<String, Vec<&'static str>> {
        let mut by_job: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
        for (name, kind) in rx.try_iter() {
            by_job.entry(name).or_default().push(kind);
        }
        by_job
    }

    fn started_and_exited(names: &[&str]) -> BTreeMap<String, Vec<&'static str>> {
        names
            .iter()
            .map(|n| (n.to_string(), vec!["started", "exited"]))
            .collect()
    }

    /// 슬롯 2·대기 3에 붙잡힌 자식 5개를 넣고 10개를 더 넣으면, 정확히 10개가 거절되고 실행되지 않는다.
    /// 두 슬롯이 붙잡힌 동안 나머지 셋은 대기열에 남아 있다. 살아 있는 자식은 어느 순간에도 2개를 넘지
    /// 않고, 해제하면 접수한 5개가 각각 한 번 시작하고 끝난다.
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
        wait_lines(&dir.join("order"), 2);
        assert_eq!(
            (pool.running(), pool.waiting()),
            (2, 3),
            "while both slots are held the other three stay queued"
        );
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(
            reports(&rx),
            started_and_exited(&["a0", "a1", "a2", "a3", "a4"]),
            "each accepted job starts and exits once and no refused job runs"
        );
        let mut order = wait_lines(&dir.join("order"), 5);
        order.sort();
        assert_eq!(order, ["a0", "a1", "a2", "a3", "a4"]);
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
    }

    /// 슬롯 안에서 넣으면 모두 바로 실행되고 거절은 없다.
    #[test]
    fn jobs_within_the_slots_all_run_without_refusal() {
        let (_tmp, dir) = setup();
        release(&dir);
        let pool = ShellPool::new(4, 0);
        let (tx, rx) = mpsc::channel();
        for i in 0..4 {
            pool.submit(|| held_job(&dir, &format!("a{i}"), tx.clone()))
                .expect("accepted");
        }
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&["a0", "a1", "a2", "a3"]));
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
        wait_lines(&dir.join("order"), 1);
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&names));
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
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&["held"]));
    }

    /// Command 빌더가 패닉해도 슬롯이 돌아오고 같은 실행기가 다음 작업을 실행한다.
    #[test]
    fn a_job_whose_builder_panics_returns_its_slot() {
        let (_tmp, dir) = setup();
        release(&dir);
        let pool = ShellPool::new(1, 1);
        let (tx, rx) = mpsc::channel();
        let threads = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&threads);
        pool.submit(|| {
            ShellJob::new(
                "boom",
                move || {
                    seen.lock().unwrap().push(std::thread::current().id());
                    panic!("test builder panic")
                },
                reporter("boom", tx.clone()),
            )
        })
        .expect("accepted");
        let seen = Arc::clone(&threads);
        pool.submit(|| {
            let mut job = held_job(&dir, "after", tx.clone());
            let mut forward = std::mem::replace(&mut job.report, Box::new(|_| {}));
            job.report = Box::new(move |event| {
                seen.lock().unwrap().push(std::thread::current().id());
                forward(event);
            });
            job
        })
        .expect("accepted");
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&["after"]));
        let threads = threads.lock().unwrap();
        assert_eq!(threads.len(), 3, "{threads:?}");
        assert!(
            threads.iter().all(|t| *t == threads[0]),
            "the queued job ran on a replacement thread: {threads:?}"
        );
    }

    /// Started 보고가 패닉하면 이미 띄운 자식을 끝내고 회수한 뒤 슬롯을 돌려준다.
    /// 자식은 30초를 자므로, 끝내지 않았다면 슬롯이 10초 안에 돌아오지 않거나 그 pid가 살아 있다.
    #[test]
    fn a_job_whose_report_panics_after_start_leaves_no_child() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 0);
        let pid_file = dir.join("pid");
        let script = format!("echo $$ > {}; exec sleep 30", pid_file.display());
        pool.submit(|| {
            ShellJob::new(
                "panicky",
                move || {
                    let mut c = std::process::Command::new("sh");
                    c.args(["-c", &script]);
                    c
                },
                |event| {
                    if let ShellEvent::Started = event {
                        panic!("test report panic");
                    }
                },
            )
        })
        .expect("accepted");
        wait_idle(&pool, Duration::from_secs(10));
        // 자식이 pid를 쓰기 전에 끝났으면 파일이 없다. 썼다면 그 프로세스는 없어야 한다.
        let deadline = Instant::now() + Duration::from_secs(2);
        while !pid_file.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Ok(pid) = std::fs::read_to_string(&pid_file) {
            let pid = pid.trim();
            if !pid.is_empty() {
                let alive = std::process::Command::new("kill")
                    .args(["-0", pid])
                    .status()
                    .expect("kill -0")
                    .success();
                assert!(!alive, "the child {pid} outlived its panicked job");
            }
        }
    }

    /// 종료를 기다리지 못하는 자식을 흉내 낸다. 대기 결과를 차례로 돌려준다.
    struct FakeChild {
        waits: VecDeque<std::io::Result<std::process::ExitStatus>>,
        killed: bool,
    }

    impl Reap for FakeChild {
        fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
            self.waits
                .pop_front()
                .unwrap_or_else(|| Err(std::io::Error::other("no more waits")))
        }
        fn kill(&mut self) -> std::io::Result<()> {
            self.killed = true;
            Ok(())
        }
    }

    fn exit_ok() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    /// 대기에 실패하면 끝내기를 시도하고 다시 기다린다. 그래도 실패하면 종료 확인 실패로 보고하고,
    /// 두 번째 대기가 성공하면 그 종료로 보고한다.
    #[test]
    fn a_failed_wait_kills_and_reports_an_unconfirmed_exit() {
        let mut stuck = FakeChild {
            waits: VecDeque::from([
                Err(std::io::Error::other("first")),
                Err(std::io::Error::other("second")),
            ]),
            killed: false,
        };
        assert_eq!(
            settle("stuck", &mut stuck),
            ShellEvent::WaitFailed("second".into())
        );
        assert!(stuck.killed);
        let mut recovered = FakeChild {
            waits: VecDeque::from([Err(std::io::Error::other("first")), Ok(exit_ok())]),
            killed: false,
        };
        assert_eq!(
            settle("recovered", &mut recovered),
            ShellEvent::Exited(Some(0))
        );
        assert!(recovered.killed);
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
        wait_lines(&dir.join("order"), 1);
        let mark = |name: &str| format!("touch {}", dir.join(name).display());
        super::super::global::submit_command_in(&pool, &mark("global"));
        assert_eq!(pool.waiting(), 1, "the global hook takes the queue place");
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
        assert_eq!(pool.waiting(), 1, "a refused job entered the queue");
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&["held"]));
        assert!(
            dir.join("global").exists(),
            "the queued global hook did not run"
        );
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
