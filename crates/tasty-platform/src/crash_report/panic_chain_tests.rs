//! 실제 패닉 훅을 설치한 자식 프로세스가 남긴 보고서를 판정한다.
//!
//! 패닉 훅은 프로세스 전역이고 소멸자 연쇄는 프로세스를 abort시키므로, 자기 시험 바이너리를
//! `panic_chain_child_entry` 하나만 돌도록 다시 실행한다. 이름에 초 단위 시각이 들어가므로
//! 충돌은 시각에 달려 있다. 그래서 시나리오마다 새 홈으로 여러 번 반복한다.

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const ENV_SCENARIO: &str = "TASTY_CRASH_REPORT_TEST_SCENARIO";
const ENV_WORK: &str = "TASTY_CRASH_REPORT_TEST_WORK";
const ENV_ROLE: &str = "TASTY_CRASH_REPORT_TEST_ROLE";
const CHILD_TIMEOUT: Duration = Duration::from_secs(120);
const ROUNDS: usize = 3;

const ROOT: &str = "root-cause-panic";
const SECONDARY: &str = "secondary-destructor-panic";
/// 소멸자가 해제 중에 패닉하면 언어 런타임이 내는 되돌릴 수 없는 패닉이다.
#[cfg(not(windows))]
const CLEANUP: &str = "panic in a destructor during cleanup";

/// 소멸자 연쇄에서 훅이 받는 패닉 메시지.
/// x86_64-pc-windows-msvc CI에서는 CLEANUP의 패닉 줄과 보고서가 없었고, Linux와 macOS CI에서는 있었다.
#[cfg(windows)]
const DESTRUCTOR_CHAIN: &[&str] = &[ROOT, SECONDARY];
#[cfg(not(windows))]
const DESTRUCTOR_CHAIN: &[&str] = &[ROOT, SECONDARY, CLEANUP];

struct PanicsOnDrop;

impl Drop for PanicsOnDrop {
    fn drop(&mut self) {
        panic!("{SECONDARY}");
    }
}

/// 자식 진입점. 부모로 돌 때는 아무것도 하지 않는다.
#[test]
fn panic_chain_child_entry() {
    let Ok(scenario) = std::env::var(ENV_SCENARIO) else {
        return;
    };
    let work = PathBuf::from(std::env::var_os(ENV_WORK).expect("child needs its work directory"));
    let role = std::env::var(ENV_ROLE).unwrap_or_default();
    super::init("9.8.7-crash-report-test");

    match scenario.as_str() {
        "destructor-chain" => {
            let _armed = PanicsOnDrop;
            reach(&work, &role, "armed");
            panic!("{ROOT}");
        }
        "two-threads" => {
            let barrier = Arc::new(Barrier::new(2));
            let threads: Vec<_> = (0..2)
                .map(|i| {
                    let barrier = Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        panic!("thread-panic-{i}");
                    })
                })
                .collect();
            for thread in threads {
                assert!(thread.join().is_err(), "the thread was meant to panic");
            }
            reach(&work, &role, "joined");
        }
        "recovered-then-panic" => {
            let caught = panic::catch_unwind(|| panic!("recovered-panic"));
            assert!(caught.is_err());
            reach(&work, &role, "recovered");
            panic!("later-panic");
        }
        "process-panic" => {
            reach(&work, &role, "ready");
            let deadline = Instant::now() + CHILD_TIMEOUT;
            while !work.join("go").exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("process-panic-{role}");
        }
        "storm" => {
            let start = Instant::now();
            let mut count: u64 = 0;
            while count < 40 || start.elapsed() < Duration::from_millis(1200) {
                let index = count;
                let caught = panic::catch_unwind(AssertUnwindSafe(|| {
                    panic!("storm-panic-{index}");
                }));
                assert!(caught.is_err());
                count += 1;
            }
            let note = format!("{count} {}", start.elapsed().as_secs());
            reach(&work, &role, &note);
        }
        "unwritable" => {
            let caught = panic::catch_unwind(|| panic!("unwritable-panic"));
            assert!(caught.is_err());
            reach(&work, &role, "survived");
        }
        other => panic!("unknown scenario {other}"),
    }
}

fn marker(work: &Path, role: &str) -> PathBuf {
    work.join(format!("reached-{role}"))
}

fn reach(work: &Path, role: &str, note: &str) {
    fs::write(marker(work, role), note).expect("write reach marker");
}

/// 자식이 쓰는 격리 홈과 표시 파일을 담는 폴더.
struct Round {
    _temp: tempfile::TempDir,
    work: PathBuf,
    home: PathBuf,
}

impl Round {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let work = temp.path().to_path_buf();
        let home = work.join("home");
        fs::create_dir_all(&home).expect("create home");
        Self {
            _temp: temp,
            work,
            home,
        }
    }

    fn stderr_path(&self, role: &str) -> PathBuf {
        self.work.join(format!("child-{role}.err"))
    }

    fn stderr(&self, role: &str) -> String {
        fs::read_to_string(self.stderr_path(role)).unwrap_or_default()
    }

    fn spawn(&self, scenario: &str, role: &str) -> Child {
        let module = module_path!()
            .split_once("::")
            .expect("module path has a crate prefix")
            .1;
        let out = fs::File::create(self.stderr_path(role)).expect("child log");
        Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                &format!("{module}::panic_chain_child_entry"),
                "--test-threads=1",
                "--nocapture",
            ])
            .env(ENV_SCENARIO, scenario)
            .env(ENV_WORK, &self.work)
            .env(ENV_ROLE, role)
            .env("TASTY_HOME", &self.home)
            // libtest 의 진행 줄이 stdout 에 나와 패닉 줄 앞에 붙지 않게 stderr 만 모은다.
            .stdout(Stdio::null())
            .stderr(out)
            .spawn()
            .expect("spawn child")
    }

    fn reports(&self) -> Vec<Report> {
        let Ok(entries) = fs::read_dir(self.home.join("crash-reports")) else {
            return Vec::new();
        };
        let mut reports: Vec<Report> = entries
            .map(|entry| entry.expect("read report entry"))
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                (name.starts_with("crash-") && name.ends_with(".log")).then(|| Report {
                    text: fs::read_to_string(entry.path()).expect("read report"),
                    name,
                })
            })
            .collect();
        reports.sort_by(|a, b| a.name.cmp(&b.name));
        reports
    }

    fn reached(&self, role: &str, status: ExitStatus) -> String {
        fs::read_to_string(marker(&self.work, role)).unwrap_or_else(|_| {
            panic!(
                "child {role:?} did not reach its point: {status:?}\n{}",
                self.stderr(role)
            )
        })
    }
}

struct Report {
    name: String,
    text: String,
}

impl Report {
    fn field(&self, key: &str) -> Option<&str> {
        self.text
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
    }

    fn message(&self) -> &str {
        self.field("Message").unwrap_or("<no message line>")
    }

    fn count(&self, line: &str) -> usize {
        self.text.lines().filter(|l| *l == line).count()
    }
}

fn wait(mut child: Child) -> ExitStatus {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("wait child") {
            return status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stuck child");
            panic!("child did not finish within {CHILD_TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn messages(reports: &[Report]) -> Vec<&str> {
    reports.iter().map(Report::message).collect()
}

/// 어느 보고서에도 없는 메시지는 `LOST:` 실패로 알려, 형식 판정의 실패와 구별한다.
fn assert_kept(reports: &[Report], expected: &[&str], round: usize) {
    let found = messages(reports);
    for message in expected {
        assert!(
            found.contains(message),
            "LOST: round {round} has no report with {message:?}; reports hold {found:?}"
        );
    }
}

fn panic_lines(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|line| line.starts_with("panic: "))
        .count()
}

fn assert_one_panic_each(reports: &[Report]) {
    for report in reports {
        assert_eq!(report.count("=== Panic ==="), 1, "{}", report.text);
        assert_eq!(report.count("=== Backtrace ==="), 1, "{}", report.text);
        let message_lines = report
            .text
            .lines()
            .filter(|line| line.starts_with("Message: "))
            .count();
        assert_eq!(message_lines, 1, "reports are mixed:\n{}", report.text);
    }
}

/// 첫 보고서를 뺀 모든 보고서가 같은 첫 패닉을 가리키는지 본다.
fn assert_names_first_panic(reports: &[Report], pid: Option<u32>) {
    let first = reports
        .iter()
        .find(|r| r.field("Panic sequence") == Some("1"))
        .expect("one report is the first panic");
    assert_eq!(first.field("First panic message"), None, "{}", first.text);
    assert_eq!(first.field("First panic location"), None, "{}", first.text);

    for report in reports {
        let reported_pid = report.field("PID").expect("report has a PID");
        assert_eq!(reported_pid, first.field("PID").expect("first has a PID"));
        if let Some(pid) = pid {
            assert_eq!(reported_pid, pid.to_string());
        }
        if std::ptr::eq(report, first) {
            continue;
        }
        assert_eq!(
            report.field("First panic message"),
            first.field("Message"),
            "{}",
            report.text
        );
        assert_eq!(
            report.field("First panic location"),
            first.field("Location"),
            "{}",
            report.text
        );
    }
}

#[test]
fn a_destructor_chain_keeps_every_panic_and_each_report_names_the_first() {
    for round in 0..ROUNDS {
        let run = Round::new();
        let child = run.spawn("destructor-chain", "");
        let pid = child.id();
        let status = wait(child);
        assert_eq!(run.reached("", status), "armed");
        assert!(!status.success(), "the chain was meant to abort");
        assert_eq!(
            panic_lines(&run.stderr("")),
            DESTRUCTOR_CHAIN.len(),
            "{}",
            run.stderr("")
        );

        let reports = run.reports();
        assert_kept(&reports, DESTRUCTOR_CHAIN, round);
        assert_eq!(messages(&reports), DESTRUCTOR_CHAIN);
        assert_eq!(
            reports[0].field("Location").map(|l| l.contains(file!())),
            Some(true),
            "{}",
            reports[0].text
        );
        let sequences: Vec<_> = reports
            .iter()
            .map(|r| r.field("Panic sequence").expect("sequence"))
            .collect();
        let expected_sequences: Vec<String> = (1..=DESTRUCTOR_CHAIN.len())
            .map(|sequence| sequence.to_string())
            .collect();
        assert_eq!(sequences, expected_sequences);
        assert_names_first_panic(&reports, Some(pid));
        assert_one_panic_each(&reports);
    }
}

#[test]
fn two_threads_panicking_together_write_separate_reports() {
    for round in 0..ROUNDS {
        let run = Round::new();
        let status = wait(run.spawn("two-threads", ""));
        assert_eq!(run.reached("", status), "joined");
        assert!(status.success(), "{}", run.stderr(""));

        let reports = run.reports();
        assert_kept(&reports, &["thread-panic-0", "thread-panic-1"], round);
        assert_eq!(reports.len(), 2);
        assert_one_panic_each(&reports);
        assert_names_first_panic(&reports, None);
    }
}

#[test]
fn a_recovered_panic_survives_a_later_panic() {
    for round in 0..ROUNDS {
        let run = Round::new();
        let status = wait(run.spawn("recovered-then-panic", ""));
        assert_eq!(run.reached("", status), "recovered");
        assert!(!status.success());

        let reports = run.reports();
        assert_kept(&reports, &["recovered-panic", "later-panic"], round);
        assert_eq!(messages(&reports), ["recovered-panic", "later-panic"]);
        assert_names_first_panic(&reports, None);
    }
}

#[test]
fn two_processes_panicking_together_both_keep_their_reports() {
    for round in 0..ROUNDS {
        let run = Round::new();
        let a = run.spawn("process-panic", "a");
        let b = run.spawn("process-panic", "b");
        let deadline = Instant::now() + CHILD_TIMEOUT;
        while !(marker(&run.work, "a").exists() && marker(&run.work, "b").exists()) {
            assert!(Instant::now() < deadline, "children did not get ready");
            std::thread::sleep(Duration::from_millis(1));
        }
        fs::write(run.work.join("go"), "").expect("release children");
        let (a_status, b_status) = (wait(a), wait(b));
        assert!(!a_status.success());
        assert!(!b_status.success());

        let reports = run.reports();
        assert_kept(&reports, &["process-panic-a", "process-panic-b"], round);
        assert_eq!(reports.len(), 2);
        assert_one_panic_each(&reports);
    }
}

#[test]
fn a_following_process_leaves_the_earlier_report_untouched() {
    for round in 0..ROUNDS {
        let run = Round::new();
        fs::write(run.work.join("go"), "").expect("children need no release here");
        let status = wait(run.spawn("process-panic", "a"));
        assert!(!status.success());
        let earlier = run.reports();
        assert_eq!(earlier.len(), 1);

        let status = wait(run.spawn("process-panic", "b"));
        assert!(!status.success());
        let reports = run.reports();
        assert_kept(&reports, &["process-panic-a", "process-panic-b"], round);
        let kept = reports
            .iter()
            .find(|r| r.name == earlier[0].name)
            .expect("the earlier report is still there");
        assert_eq!(kept.text, earlier[0].text);
    }
}

#[test]
fn a_recovered_panic_storm_keeps_the_first_panic_and_a_bounded_file_rate() {
    let run = Round::new();
    let status = wait(run.spawn("storm", ""));
    let note = run.reached("", status);
    assert!(status.success());
    let (count, seconds) = note.split_once(' ').expect("count and seconds");
    let count: usize = count.parse().expect("panic count");
    let seconds: usize = seconds.parse().expect("whole seconds");

    assert_eq!(panic_lines(&run.stderr("")), count);
    let reports = run.reports();
    assert_kept(&reports, &["storm-panic-0"], 0);
    let limit = super::REPORT_FILES_PER_SECOND as usize;
    let saved = run.stderr("").matches("Report saved to").count();
    assert_eq!(saved, reports.len());
    assert!(
        reports.len() <= seconds * limit + limit,
        "{} files from {count} panics in {seconds} whole seconds",
        reports.len()
    );
}

#[test]
fn an_unwritable_report_folder_still_prints_the_panic_to_stderr() {
    let run = Round::new();
    let blocker = run.home.join("crash-reports");
    fs::write(&blocker, "not a directory").expect("block the report folder");

    let status = wait(run.spawn("unwritable", ""));
    assert_eq!(run.reached("", status), "survived");
    assert!(status.success(), "{}", run.stderr(""));

    let stderr = run.stderr("");
    assert_eq!(panic_lines(&stderr), 1, "{stderr}");
    assert!(stderr.contains("unwritable-panic"), "{stderr}");
    assert!(!stderr.contains("Report saved"), "{stderr}");
    assert!(
        stderr
            .lines()
            .any(|line| line.trim_start().starts_with("0:")),
        "the backtrace is missing:\n{stderr}"
    );
    assert_eq!(
        fs::read_to_string(&blocker).expect("read blocker"),
        "not a directory"
    );
}
