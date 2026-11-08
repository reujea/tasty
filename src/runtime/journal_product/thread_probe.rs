//! 시험이 저널 worker 스레드가 지금 일하는 중인지 읽는다.
//!
//! 정체 감지는 worker가 잠들어 있는 시간만 세야 한다. worker가 실행 중이거나 디스크 I/O를
//! 기다리는 시간까지 세면 같은 디스크를 쓰는 다른 프로세스의 fsync 지연이 교착처럼 보인다.
//! 스레드 상태는 Linux의 `/proc/<pid>/task/<tid>/stat`에서만 읽는다. 다른 OS에서는 항상
//! 모름으로 답하고, 호출자는 쉰 시간을 모두 정체로 센다.
//!
//! 다른 프로세스를 볼 때는 그 프로세스의 모든 스레드를 그때마다 다시 읽는다. 저널 일을 맡은 스레드는
//! 리더가 아닐 수 있고 probe를 만든 뒤에 시작할 수도 있다. 읽는 사이 사라진 스레드는 일하지 않는
//! 것으로 본다.
//!
//! join이 돌아온 직후에도 커널은 그 스레드의 종료 경로를 아직 실행하고 있을 수 있다. 그동안
//! 상태는 R로 읽히지만 flags에 PF_EXITING이 서 있으므로 일하는 중으로 보지 않는다. 같은 TID가
//! 다른 스레드에 다시 쓰이면 시작 시각이 달라지므로 그 스레드도 일하는 중으로 보지 않는다.
//!
//! 정체로 판정한 시험은 worker를 버린다고 표시한다. 멈춘 worker를 Drop에서 join하면 시험이
//! 실패로 끝나지 않고 그 자리에서 멈추기 때문이다.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// 커널 `PF_EXITING`: 스레드가 종료 경로에 들어갔다.
const PF_EXITING: u64 = 0x4;

/// worker 스레드의 상태 파일 경로와 버림 표시. worker가 시작하며 경로와 시작 시각을 한 번 기록한다.
#[derive(Clone, Default)]
pub(crate) struct ThreadProbe {
    stat: Arc<OnceLock<Bound>>,
    abandoned: Arc<AtomicBool>,
}

impl ThreadProbe {
    /// 지금 스레드를 이 probe가 볼 스레드로 기록한다. worker 스레드 첫머리에서 부른다.
    pub(crate) fn bind_current(&self) {
        #[cfg(target_os = "linux")]
        match std::fs::read_link("/proc/thread-self") {
            Ok(link) => {
                let path = PathBuf::from("/proc").join(link).join("stat");
                let Some(start) = start_of(&path) else {
                    return;
                };
                if self
                    .stat
                    .set(Bound {
                        path,
                        start,
                        threads: None,
                    })
                    .is_err()
                {
                    tracing::warn!("journal worker thread probe was bound twice");
                }
            }
            Err(error) => tracing::warn!("cannot locate the journal worker thread: {error}"),
        }
    }

    /// worker가 실행 중(R)이거나 끊을 수 없는 대기(D, 대개 디스크 I/O)에 있으면 true.
    /// 잠든 상태, 끝났거나 끝나는 중인 스레드, TID를 물려받은 다른 스레드, 상태를 읽을 수 없는
    /// OS는 false다.
    pub(crate) fn is_working(&self) -> bool {
        let Some(bound) = self.stat.get() else {
            return false;
        };
        let Ok(stat) = std::fs::read_to_string(&bound.path) else {
            return false;
        };
        match &bound.threads {
            None => works(&stat, &bound.start),
            Some(threads) => process_works(&stat, &bound.start, thread_stats(threads)),
        }
    }
}

impl ThreadProbe {
    /// PTY 자식처럼 이 프로세스 밖의 프로세스를 볼 probe. 스레드 하나라도 일하면 일하는 중이다.
    /// 상태는 Linux에서만 읽는다. 지금 읽을 수 없는(이미 끝난) 프로세스는 일하지 않는 것으로 본다.
    /// 시작 시각을 함께 기록하므로 회수 뒤 같은 PID를 물려받은 다른 프로세스를 일하는 중으로 읽지
    /// 않는다.
    #[cfg(unix)]
    pub(crate) fn process(pid: u32) -> Self {
        let probe = Self::default();
        if cfg!(target_os = "linux") {
            let path = PathBuf::from(format!("/proc/{pid}/stat"));
            if let Some(start) = start_of(&path) {
                // 방금 만든 probe라 이미 묶였을 수 없다.
                probe.stat.get_or_init(|| Bound {
                    path,
                    start,
                    threads: Some(PathBuf::from(format!("/proc/{pid}/task"))),
                });
            }
        }
        probe
    }

    /// 시험이 이 worker를 정체로 판정했다. 이후 Drop은 worker를 join하지 않는다.
    pub(crate) fn abandon(&self) {
        self.abandoned.store(true, Ordering::Release);
    }

    pub(crate) fn is_abandoned(&self) -> bool {
        self.abandoned.load(Ordering::Acquire)
    }
}

/// 기록한 스레드·프로세스의 상태 파일과 시작 시각(`stat` 22번째 필드).
struct Bound {
    path: PathBuf,
    start: String,
    /// 프로세스를 볼 때 그 스레드들이 있는 `task` 폴더.
    threads: Option<PathBuf>,
}

/// `stat`에서 읽는 필드. 3번째 state, 9번째 flags, 22번째 starttime.
struct Fields {
    state: char,
    flags: u64,
    start: String,
}

/// 둘째 필드(comm)는 괄호와 공백을 품을 수 있어 마지막 `)` 뒤에서 읽는다.
fn fields_of(stat: &str) -> Option<Fields> {
    let (_, rest) = stat.rsplit_once(')')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    Some(Fields {
        state: fields.first()?.chars().next()?,
        flags: fields.get(6)?.parse().ok()?,
        start: (*fields.get(19)?).to_owned(),
    })
}

/// `path`의 스레드·프로세스 시작 시각. 읽거나 해석하지 못하면 경고를 남기고 None.
#[cfg(unix)]
fn start_of(path: &std::path::Path) -> Option<String> {
    match std::fs::read_to_string(path).map(|stat| fields_of(&stat)) {
        Ok(Some(fields)) => Some(fields.start),
        Ok(None) => {
            tracing::warn!("cannot parse the probed stat {}", path.display());
            None
        }
        Err(error) => {
            tracing::warn!("cannot read the probed stat {}: {error}", path.display());
            None
        }
    }
}

impl Fields {
    fn busy(&self) -> bool {
        self.flags & PF_EXITING == 0 && matches!(self.state, 'R' | 'D')
    }
}

/// `start`에 시작한 스레드가 이 `stat`에서 일하는 중인지.
fn works(stat: &str, start: &str) -> bool {
    fields_of(stat).is_some_and(|f| f.start == start && f.busy())
}

/// `start`에 시작한 프로세스의 스레드 가운데 일하는 것이 있는지. 스레드마다 시작 시각이 다르므로
/// 시작 시각은 리더의 `stat`에서만 비교한다. 리더가 먼저 끝나 좀비여도 다른 스레드는 일할 수 있다.
fn process_works(leader: &str, start: &str, mut threads: impl Iterator<Item = String>) -> bool {
    fields_of(leader).is_some_and(|f| f.start == start)
        && threads.any(|stat| fields_of(&stat).is_some_and(|f| f.busy()))
}

/// `task` 폴더 아래 스레드의 `stat`들. 읽는 사이 끝난 스레드는 건너뛴다.
fn thread_stats(threads: &std::path::Path) -> impl Iterator<Item = String> {
    std::fs::read_dir(threads)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|thread| std::fs::read_to_string(thread.path().join("stat")).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실제 `stat` 한 줄의 꼴. state, flags(9번째), starttime(22번째)만 바꿔 넣는다.
    fn stat(comm: &str, state: char, flags: u64, start: u64) -> String {
        format!("12 ({comm}) {state} 1 12 12 0 -1 {flags} 0 0 0 0 0 0 0 0 20 0 1 0 {start} 0 0")
    }

    #[test]
    fn the_fields_are_read_after_a_comm_that_holds_parentheses() {
        let f = fields_of(&stat("a) b", 'D', 64, 777)).expect("parsed");
        assert_eq!((f.state, f.flags, f.start.as_str()), ('D', 64, "777"));
        assert!(fields_of(&stat("structure-journ", 'S', 0, 1)).is_some());
        assert!(fields_of("garbage").is_none());
        assert!(fields_of("12 (x) R 1").is_none());
    }

    #[test]
    fn only_the_bound_live_thread_running_or_in_io_is_working() {
        assert!(works(&stat("w", 'R', 64, 5), "5"));
        assert!(works(&stat("w", 'D', 64, 5), "5"));
        assert!(!works(&stat("w", 'S', 64, 5), "5"));
        // join 직후 종료 경로를 실행 중인 같은 스레드.
        assert!(!works(&stat("w", 'R', 64 | PF_EXITING, 5), "5"));
        // 같은 TID를 물려받은 다른 스레드.
        assert!(!works(&stat("w", 'R', 64, 9), "5"));
    }

    #[test]
    fn a_process_works_only_through_a_live_busy_thread_of_the_same_start() {
        let leader = stat("w", 'S', 64, 5);
        let threads = |list: &[(char, u64)]| {
            list.iter()
                .map(|&(state, flags)| stat("w", state, flags, 900))
                .collect::<Vec<_>>()
                .into_iter()
        };
        // 일하는 스레드는 리더가 아니어도 된다.
        assert!(process_works(
            &leader,
            "5",
            threads(&[('S', 64), ('R', 64)])
        ));
        assert!(process_works(&leader, "5", threads(&[('D', 64)])));
        // 리더가 먼저 끝나도 남은 스레드가 일한다.
        assert!(process_works(
            &stat("w", 'Z', 64, 5),
            "5",
            threads(&[('R', 64)])
        ));
        assert!(!process_works(
            &leader,
            "5",
            threads(&[('S', 64), ('S', 64)])
        ));
        assert!(!process_works(&leader, "5", threads(&[])));
        assert!(!process_works(
            &leader,
            "5",
            threads(&[('R', 64 | PF_EXITING)])
        ));
        // 같은 PID를 물려받은 다른 프로세스.
        assert!(!process_works(&leader, "9", threads(&[('R', 64)])));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_thread_spinning_reads_as_working_and_a_parked_one_does_not() {
        let probe = ThreadProbe::default();
        let bound = probe.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spin = stop.clone();
        let (park_tx, park_rx) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            bound.bind_current();
            while !spin.load(std::sync::atomic::Ordering::Relaxed) {
                std::hint::spin_loop();
            }
            // 잠든 상태(S)로 머문다.
            park_rx.recv().ok();
        });
        while probe.stat.get().is_none() {
            std::thread::yield_now();
        }
        assert!(probe.is_working(), "spinning thread is R");
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while probe.is_working() {
            assert!(
                std::time::Instant::now() < deadline,
                "parked thread never read as sleeping"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        drop(park_tx);
        handle.join().unwrap();
        assert!(!probe.is_working(), "an exited thread is not working");
    }

    /// 부모가 stdin으로 시키는 대로 리더가 아닌 스레드를 돌리거나 재우는 자식.
    #[cfg(target_os = "linux")]
    const THREADS_CHILD_ENV: &str = "TASTY_THREAD_PROBE_CHILD";

    #[cfg(target_os = "linux")]
    #[test]
    fn threads_child_helper() {
        use std::io::{BufRead, Write};
        if std::env::var_os(THREADS_CHILD_ENV).is_none() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (_park_tx, park_rx) = std::sync::mpsc::channel::<()>();
        let mut park_rx = Some(park_rx);
        let say = |word: &str| {
            let mut out = std::io::stdout();
            writeln!(out, "{word}").unwrap();
            out.flush().unwrap();
        };
        say("ready");
        for line in std::io::stdin().lock().lines() {
            match line.unwrap().as_str() {
                "spin" => {
                    let spin = stop.clone();
                    let parked = park_rx.take().unwrap();
                    std::thread::spawn(move || {
                        while !spin.load(Ordering::Relaxed) {
                            std::hint::spin_loop();
                        }
                        parked.recv().ok();
                    });
                    say("spinning");
                }
                "park" => {
                    stop.store(true, Ordering::Relaxed);
                    say("parked");
                }
                other => panic!("unknown command {other}"),
            }
        }
    }

    #[cfg(target_os = "linux")]
    struct ThreadsChild {
        child: std::process::Child,
        stdin: Option<std::process::ChildStdin>,
        stdout: std::io::BufReader<std::process::ChildStdout>,
    }

    #[cfg(target_os = "linux")]
    impl ThreadsChild {
        fn spawn() -> Self {
            use std::process::{Command, Stdio};
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::journal_product::thread_probe::tests::threads_child_helper",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(THREADS_CHILD_ENV, "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdin = child.stdin.take();
            let stdout = std::io::BufReader::new(child.stdout.take().unwrap());
            Self {
                child,
                stdin,
                stdout,
            }
        }

        /// libtest가 같은 줄 앞에 시험 이름을 찍을 수 있어 줄 끝으로 맞춘다.
        fn expect(&mut self, word: &str) {
            use std::io::BufRead;
            let mut line = String::new();
            loop {
                line.clear();
                let read = self.stdout.read_line(&mut line).unwrap();
                assert_ne!(read, 0, "the child ended before saying {word}");
                if line.trim_end().ends_with(word) {
                    return;
                }
            }
        }

        fn order(&mut self, command: &str, answer: &str) {
            use std::io::Write;
            writeln!(self.stdin.as_mut().unwrap(), "{command}").unwrap();
            self.expect(answer);
        }

        fn end(mut self) {
            drop(self.stdin.take());
            assert!(self.child.wait().unwrap().success());
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for ThreadsChild {
        fn drop(&mut self) {
            if matches!(self.child.try_wait(), Ok(None)) {
                drop(self.child.kill());
                drop(self.child.wait());
            }
        }
    }

    /// 조건이 설 때까지 기다린다. 시간은 실패할 때의 상한일 뿐이다.
    #[cfg(target_os = "linux")]
    fn until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "never: {what}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_works_while_a_thread_started_after_the_probe_runs_and_not_once_all_sleep() {
        let mut child = ThreadsChild::spawn();
        let probe = ThreadProbe::process(child.child.id());
        child.expect("ready");
        until("all threads asleep", || !probe.is_working());
        child.order("spin", "spinning");
        until("the new thread reads as working", || probe.is_working());
        child.order("park", "parked");
        until("all threads asleep again", || !probe.is_working());
        child.end();
        assert!(!probe.is_working(), "a reaped process is not working");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_works_while_a_thread_started_before_the_probe_runs() {
        let mut child = ThreadsChild::spawn();
        child.expect("ready");
        child.order("spin", "spinning");
        let probe = ThreadProbe::process(child.child.id());
        until("the running thread reads as working", || probe.is_working());
        child.order("park", "parked");
        until("all threads asleep", || !probe.is_working());
        child.end();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_busy_child_process_reads_as_working_and_a_sleeping_or_reaped_one_does_not() {
        let mut busy = std::process::Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .spawn()
            .unwrap();
        let mut sleeping = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let busy_probe = ThreadProbe::process(busy.id());
        let sleeping_probe = ThreadProbe::process(sleeping.id());
        // 바쁜 자식은 CPU를 기다리는 동안에도 R이다. 잠든 자식은 곧 S가 된다.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !busy_probe.is_working() || sleeping_probe.is_working() {
            assert!(
                std::time::Instant::now() < deadline,
                "busy child never read as working or sleeping child never read as idle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        busy.kill().unwrap();
        busy.wait().unwrap();
        sleeping.kill().unwrap();
        sleeping.wait().unwrap();
        assert!(!busy_probe.is_working(), "a reaped process is not working");
    }
}
