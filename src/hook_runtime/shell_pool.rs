//! 셸 명령 훅(surface 훅 셸·수동 셸 dispatch·전역 훅)이 함께 쓰는 실행기.
//! 작업을 맡는 슬롯은 최대 [`RUNNING_LIMIT`]개이고, 슬롯이 모두 찼을 때만 [`WAITING_LIMIT`]개까지
//! 대기열에 넣는다. 그보다 많으면 실행하지 않고 호출자가 알림이나 오류로 알린다.
//! 슬롯 하나는 실행기 스레드 하나가 자식을 띄워 종료까지 기다린다. 대기열이 비면 그 스레드는 끝난다.
//!
//! 보장: 살아 있는 직접 자식은 최대 [`RUNNING_LIMIT`]개이고, 대기열에서 꺼내는 순서는 접수 순서다.
//! 여러 스레드가 꺼낸 뒤 실제 프로세스가 시작·종료되는 순서는 보장하지 않는다.

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// 작업을 맡은 슬롯 수. 대체 스레드를 세우지 못한 직후가 아니면, 대기열이 비어 있지 않을 때 상한과 같다.
    running: usize,
}

pub(crate) struct ShellPool {
    state: Mutex<PoolState>,
    running_limit: usize,
    waiting_limit: usize,
    /// 시험이 실행기 스레드 생성 실패를 흉내 낼 횟수.
    #[cfg(test)]
    fail_spawns: AtomicUsize,
    /// 시험이 작업을 마친 실행기 스레드를 한 번 풀려 나가게(unwind) 한다.
    #[cfg(test)]
    unwind_after_job: AtomicBool,
}

static POISONED: AtomicBool = AtomicBool::new(false);

impl ShellPool {
    pub(crate) fn new(running_limit: usize, waiting_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(PoolState::default()),
            running_limit,
            waiting_limit,
            #[cfg(test)]
            fail_spawns: AtomicUsize::new(0),
            #[cfg(test)]
            unwind_after_job: AtomicBool::new(false),
        })
    }

    fn lock(&self) -> MutexGuard<'_, PoolState> {
        crate::poison::recover_mutex(self.state.lock(), "hook shell pool", &POISONED)
    }

    /// 접수가 확정된 뒤에만 `make`를 부른다. 거절하면 `make`는 불리지 않는다.
    /// 슬롯이 남으면 새 실행기 스레드에 넘기고, 모두 찼으면 대기열에 넣는다.
    /// 슬롯이 남았는데 대기열에 작업이 있으면(대체 스레드를 세우지 못한 뒤) 새 스레드는
    /// 대기열 맨 앞을 맡고 새 작업은 대기열 끝에 선다. 그래서 접수 순서가 유지된다.
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
            // 대기열을 건드리기 전에 만든다. `make`가 패닉해도 대기 작업은 그대로 남고,
            // 새 스레드는 첫 작업을 받지 못한 채 대기열을 맡는다.
            let new = make();
            let job = match state.waiting.pop_front() {
                Some(front) => {
                    state.waiting.push_back(new);
                    front
                }
                None => new,
            };
            if tx.send(job).is_err() {
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
    pub(super) fn running(&self) -> usize {
        self.lock().running
    }

    /// 대기열의 작업 수. 시험이 미실행을 관측할 때 쓴다.
    #[cfg(all(test, unix))]
    pub(super) fn waiting(&self) -> usize {
        self.lock().waiting.len()
    }
}

/// 첫 작업을 받을 채널과 함께 실행기 스레드를 만든다. 호출자가 슬롯 수를 센다.
fn spawn_worker(pool: &Arc<ShellPool>) -> std::io::Result<mpsc::Sender<ShellJob>> {
    #[cfg(test)]
    if pool
        .fail_spawns
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
    {
        return Err(std::io::Error::other("test spawn failure"));
    }
    let (tx, rx) = mpsc::channel::<ShellJob>();
    let pool = Arc::clone(pool);
    std::thread::Builder::new()
        .name("hook-shell".into())
        .spawn(move || work(&pool, rx))?;
    Ok(tx)
}

/// 실행기 스레드가 슬롯을 반납하지 못한 채 풀려 나가면(unwind) 대신 반납하고,
/// 대기열이 남아 있으면 그 작업을 맡을 스레드를 새로 세운다.
/// 세우지 못하면 슬롯을 줄이고 작업을 대기열 맨 앞에 되돌린다. 다른 슬롯이 남아 있으면 그 스레드가
/// 맡는다. 남은 슬롯이 없으면 대기열 전부를 시작하지 못한 것으로 보고한다.
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
        let Some(job) = state.waiting.pop_front() else {
            state.running -= 1;
            return;
        };
        let error = match spawn_worker(self.pool) {
            Ok(tx) => match tx.send(job) {
                Ok(()) => return,
                Err(mpsc::SendError(job)) => {
                    state.waiting.push_front(job);
                    "the replacement hook shell thread stopped before its job".to_string()
                }
            },
            Err(e) => {
                state.waiting.push_front(job);
                e.to_string()
            }
        };
        tracing::error!("hook shell thread could not be replaced: {error}");
        state.running -= 1;
        if state.running > 0 {
            return;
        }
        let orphans: Vec<ShellJob> = state.waiting.drain(..).collect();
        drop(state);
        for job in orphans {
            report_caught(job, ShellEvent::NotStarted(error.clone()));
        }
    }
}

/// 풀 잠금 밖에서 보고한다. 보고 콜백의 패닉은 여기서 멈춘다(풀려 나가는 중의 이중 패닉 방지).
fn report_caught(mut job: ShellJob, event: ShellEvent) {
    let label = job.label.clone();
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (job.report)(event))).is_err() {
        tracing::error!("hook command '{label}' report panicked");
    }
}

fn work(pool: &Arc<ShellPool>, first: mpsc::Receiver<ShellJob>) {
    let mut guard = SlotGuard { pool, held: true };
    let mut next = first.recv().ok();
    loop {
        if let Some(job) = next.take() {
            run_caught(job);
        }
        #[cfg(test)]
        if pool.unwind_after_job.swap(false, Ordering::SeqCst) {
            panic!("test hook shell thread unwind");
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

/// 종료를 기다릴 수 있는 자식. 시험이 대기 실패를 흉내 낼 때 바꿔 끼운다.
trait Reap {
    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus>;
    #[cfg(windows)]
    fn kill(&mut self) -> std::io::Result<()>;
}

impl Reap for std::process::Child {
    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        std::process::Child::wait(self)
    }
    #[cfg(windows)]
    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
}

/// 자식의 종료를 확인한다. 확인하지 못하면 [`ShellEvent::WaitFailed`]이고 슬롯은 다음 작업으로 넘어간다.
/// Unix에서는 대기 실패 뒤 끝내기를 시도하지 않는다. 그 pid가 이미 회수돼 다른 프로세스에 다시 쓰였을 수
/// 있기 때문이다. Windows는 프로세스 핸들을 쥐고 있어 끝내기를 시도하고 한 번 더 기다린다.
fn settle(label: &str, child: &mut impl Reap) -> ShellEvent {
    let first = match child.wait() {
        Ok(status) => {
            tracing::debug!("hook command '{label}' exited with {:?}", status.code());
            return ShellEvent::Exited(status.code());
        }
        Err(e) => e,
    };
    #[cfg(windows)]
    let first = {
        tracing::warn!("hook command '{label}' wait failed: {first}; killing it");
        if let Err(e) = child.kill() {
            tracing::warn!("hook command '{label}' could not be killed: {e}");
        }
        match child.wait() {
            Ok(status) => return ShellEvent::Exited(status.code()),
            Err(e) => e,
        }
    };
    tracing::error!(
        "hook command '{label}' exit could not be confirmed: {first}; its slot moves on"
    );
    ShellEvent::WaitFailed(first.to_string())
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
        held_job_with(dir, name, reporter(name, tx))
    }

    fn held_job_with(
        dir: &Path,
        name: &str,
        report: impl FnMut(ShellEvent) + Send + 'static,
    ) -> ShellJob {
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
            report,
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

    /// 열릴 때까지 빌더를 붙잡는 문. 두 번째 작업이 대기열에 선 뒤 첫 작업을 진행시킬 때 쓴다.
    fn gate() -> (mpsc::Sender<()>, mpsc::Receiver<()>) {
        mpsc::channel()
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting until {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// 보고마다 그 콜백을 부른 스레드를 `threads`에 남기고 원래 보고로 넘긴다.
    fn record_thread(
        mut job: ShellJob,
        threads: &Arc<Mutex<Vec<std::thread::ThreadId>>>,
    ) -> ShellJob {
        let seen = Arc::clone(threads);
        let mut forward = std::mem::replace(&mut job.report, Box::new(|_| {}));
        job.report = Box::new(move |event| {
            seen.lock().unwrap().push(std::thread::current().id());
            forward(event);
        });
        job
    }

    /// Command 빌더가 패닉해도 슬롯이 돌아오고, 같은 실행기 스레드가 대기열의 다음 작업을 실행한다.
    /// 빌더는 두 번째 작업이 대기열에 선 것을 확인한 뒤에야 패닉한다.
    #[test]
    fn a_job_whose_builder_panics_returns_its_slot() {
        let (_tmp, dir) = setup();
        release(&dir);
        let pool = ShellPool::new(1, 1);
        let (tx, rx) = mpsc::channel();
        let threads = Arc::new(Mutex::new(Vec::new()));
        let (open, wait_open) = gate();
        let seen = Arc::clone(&threads);
        pool.submit(|| {
            ShellJob::new(
                "boom",
                move || {
                    seen.lock().unwrap().push(std::thread::current().id());
                    wait_open.recv().expect("gate");
                    panic!("test builder panic")
                },
                reporter("boom", tx.clone()),
            )
        })
        .expect("accepted");
        pool.submit(|| record_thread(held_job(&dir, "after", tx.clone()), &threads))
            .expect("accepted");
        assert_eq!(
            pool.waiting(),
            1,
            "the second job must wait behind the first"
        );
        open.send(()).expect("open the gate");
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(reports(&rx), started_and_exited(&["after"]));
        let threads = threads.lock().unwrap();
        assert_eq!(threads.len(), 3, "{threads:?}");
        assert!(
            threads.iter().all(|t| *t == threads[0]),
            "the queued job ran on a replacement thread: {threads:?}"
        );
    }

    /// 전역 훅 작업이 빌더 패닉이나 시작 실패로 끝나도 실행 토큰을 놓는다. 놓지 않으면 그 훅은 다시 제출되지 않는다.
    #[test]
    fn a_global_hook_job_releases_its_run_token_when_it_panics_or_fails_to_start() {
        let pool = ShellPool::new(1, 0);
        let builders: [Box<dyn FnOnce() -> std::process::Command + Send>; 2] = [
            Box::new(|| panic!("test builder panic")),
            Box::new(|| std::process::Command::new("/nonexistent/tasty-test-no-such-program")),
        ];
        for build in builders {
            let token = Arc::new(());
            let mut job = super::super::global::global_job("test", Some(Arc::clone(&token)));
            job.build = build;
            pool.submit(|| job).expect("accepted");
            let deadline = Instant::now() + Duration::from_secs(20);
            assert!(
                tasty_utils::test_wait::holds_by(deadline, || Arc::strong_count(&token) == 1
                    && pool.running() == 0),
                "the ended job kept its run token ({} holders) or its slot ({} running)",
                Arc::strong_count(&token),
                pool.running()
            );
        }
    }

    /// Started 보고가 패닉하면 이미 띄운 자식을 끝내고 회수한 뒤 슬롯을 돌려준다.
    /// 자식은 30초를 자므로, 끝내지 않았다면 슬롯이 10초 안에 돌아오지 않거나 그 pid가 살아 있다.
    #[test]
    fn a_job_whose_report_panics_after_start_leaves_no_child() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 0);
        let pid_file = dir.join("pid");
        let script = format!("echo $$ > {}; exec sleep 30", pid_file.display());
        let started = Arc::new(AtomicBool::new(false));
        let saw_start = Arc::clone(&started);
        pool.submit(|| {
            ShellJob::new(
                "panicky",
                move || {
                    let mut c = std::process::Command::new("sh");
                    c.args(["-c", &script]);
                    c
                },
                move |event| {
                    if let ShellEvent::Started = event {
                        saw_start.store(true, Ordering::SeqCst);
                        panic!("test report panic");
                    }
                },
            )
        })
        .expect("accepted");
        wait_idle(&pool, Duration::from_secs(10));
        assert!(
            started.load(Ordering::SeqCst),
            "the job never reported Started"
        );
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

    /// 실행기 스레드가 풀려 나가고 대체 스레드를 세우지 못해 슬롯이 하나도 남지 않으면,
    /// 대기열의 작업 전부를 시작하지 못한 것으로 보고한다. 한 보고가 패닉해도 나머지는 보고된다.
    #[test]
    fn when_no_slot_is_left_every_queued_job_is_reported_not_started() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(1, 3);
        let (tx, rx) = mpsc::channel();
        pool.submit(|| held_job(&dir, "held", tx.clone()))
            .expect("accepted");
        wait_lines(&dir.join("order"), 1);
        let panicking = {
            let mut forward = reporter("q0", tx.clone());
            move |event: ShellEvent| {
                let not_started = matches!(event, ShellEvent::NotStarted(_));
                forward(event);
                if not_started {
                    panic!("test report panic while unwinding");
                }
            }
        };
        pool.submit(|| held_job_with(&dir, "q0", panicking))
            .expect("accepted");
        for name in ["q1", "q2"] {
            pool.submit(|| held_job(&dir, name, tx.clone()))
                .expect("accepted");
        }
        pool.fail_spawns.store(1, Ordering::SeqCst);
        pool.unwind_after_job.store(true, Ordering::SeqCst);
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(pool.waiting(), 0);
        // 슬롯이 0이 된 뒤 잠금 밖에서 보고하므로, 반납을 본 뒤에도 보고 다섯 건을 기다린다.
        let mut got: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
        for _ in 0..5 {
            let (name, kind) = rx
                .recv_timeout(Duration::from_secs(20))
                .expect("a queued job was never reported");
            got.entry(name).or_default().push(kind);
        }
        for (name, kind) in rx.try_iter() {
            got.entry(name).or_default().push(kind);
        }
        let mut expected = started_and_exited(&["held"]);
        for name in ["q0", "q1", "q2"] {
            expected.insert(name.to_string(), vec!["not_started"]);
        }
        assert_eq!(got, expected);
        assert_eq!(wait_lines(&dir.join("order"), 1), ["held"]);
    }

    /// 대체 스레드를 세우지 못해도 다른 슬롯이 남아 있으면 대기 작업은 버려지지 않는다.
    /// 그 뒤 새 작업이 빈 슬롯을 얻어도 대기열의 작업이 먼저 시작한다.
    #[test]
    fn a_job_left_by_a_failed_replacement_still_runs_before_newer_jobs() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(2, 3);
        let (tx, rx) = mpsc::channel();
        pool.submit(|| held_job(&dir, "held", tx.clone()))
            .expect("accepted");
        let (open, wait_open) = gate();
        pool.submit(|| {
            ShellJob::new(
                "quick",
                move || {
                    wait_open.recv().expect("gate");
                    std::process::Command::new("true")
                },
                reporter("quick", tx.clone()),
            )
        })
        .expect("accepted");
        pool.submit(|| held_job(&dir, "queued", tx.clone()))
            .expect("accepted");
        assert_eq!((pool.running(), pool.waiting()), (2, 1));
        pool.fail_spawns.store(1, Ordering::SeqCst);
        pool.unwind_after_job.store(true, Ordering::SeqCst);
        open.send(()).expect("open the gate");
        wait_until("the unwound slot is returned", || pool.running() == 1);
        assert_eq!(pool.waiting(), 1, "the queued job was dropped");
        pool.submit(|| held_job(&dir, "newer", tx.clone()))
            .expect("accepted");
        wait_lines(&dir.join("order"), 2);
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(
            reports(&rx),
            started_and_exited(&["held", "newer", "queued", "quick"])
        );
        assert_eq!(
            wait_lines(&dir.join("order"), 3),
            ["held", "queued", "newer"]
        );
    }

    /// 빈 슬롯을 얻은 요청의 작업 생성이 패닉해도 대기열에 남아 있던 작업은 잃지 않고 실행된다.
    #[test]
    fn a_panicking_submit_into_a_free_slot_keeps_the_queued_job() {
        let (_tmp, dir) = setup();
        let pool = ShellPool::new(2, 3);
        let (tx, rx) = mpsc::channel();
        pool.submit(|| held_job(&dir, "held", tx.clone()))
            .expect("accepted");
        let (open, wait_open) = gate();
        pool.submit(|| {
            ShellJob::new(
                "quick",
                move || {
                    wait_open.recv().expect("gate");
                    std::process::Command::new("true")
                },
                reporter("quick", tx.clone()),
            )
        })
        .expect("accepted");
        pool.submit(|| held_job(&dir, "queued", tx.clone()))
            .expect("accepted");
        pool.fail_spawns.store(1, Ordering::SeqCst);
        pool.unwind_after_job.store(true, Ordering::SeqCst);
        open.send(()).expect("open the gate");
        wait_until("the unwound slot is returned", || pool.running() == 1);
        assert_eq!(pool.waiting(), 1);
        let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pool.submit(|| panic!("test make panic"))
        }));
        assert!(submitted.is_err(), "the make closure did not panic");
        wait_lines(&dir.join("order"), 2);
        release(&dir);
        wait_idle(&pool, Duration::from_secs(20));
        assert_eq!(
            reports(&rx),
            started_and_exited(&["held", "queued", "quick"])
        );
    }

    /// 종료를 기다리지 못하는 자식을 흉내 낸다.
    struct FakeChild;

    impl Reap for FakeChild {
        fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
            Err(std::io::Error::other("wait failed"))
        }
    }

    /// Unix에서는 대기에 실패하면 끝내기를 시도하지 않고(Reap에 kill이 없다) 종료 확인 실패로 보고한다.
    #[test]
    fn a_failed_wait_reports_an_unconfirmed_exit() {
        assert_eq!(
            settle("stuck", &mut FakeChild),
            ShellEvent::WaitFailed("wait failed".into())
        );
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
        super::super::global::submit_command_in(&pool, &mark("global"), None);
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
