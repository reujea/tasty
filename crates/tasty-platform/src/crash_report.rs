//! Panic reporting and host logs are shared by GUI, headless and CLI startup.
//! Only winit stall reports and the GUI error-loop detector require the GUI feature.

use std::backtrace::Backtrace;
use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Instant, SystemTime};

use tracing_subscriber::EnvFilter;

use tasty_utils::path::tasty_home;

/// 보고서에 쓸 본체 버전. 이 크레이트의 CARGO_PKG_VERSION과 달라 init 호출자가 전달한다.
static APP_VERSION: OnceLock<&'static str> = OnceLock::new();

/// init 전에는 unknown을 반환한다.
fn app_version() -> &'static str {
    APP_VERSION.get().copied().unwrap_or("unknown")
}

/// crash · hang 보고서가 공유하는 머리(제목 · 시각 · 버전 · OS)를 쓴다.
fn write_report_header(out: &mut impl Write, title: &str, timestamp: &str) {
    writeln!(out, "=== {title} ===").ok();
    writeln!(
        out,
        "Timestamp: {}",
        timestamp.replace('T', " ").replace('-', ":")
    )
    .ok();
    writeln!(out, "Version: {}", app_version()).ok();
    writeln!(
        out,
        "OS: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
    .ok();
    writeln!(out).ok();
}

/// Return the crash report directory: `~/.tasty/crash-reports/`
fn crash_report_dir() -> Option<PathBuf> {
    tasty_home().map(|dir| dir.join("crash-reports"))
}

/// Format a `SystemTime` as `YYYY-MM-DDTHH-MM-SS` without external crates.
fn format_timestamp(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let days = days + 719_468;
    let era = days / 146_097;
    let doe = days % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// 프로세스 하나가 초마다 새로 만드는 패닉 보고서 수의 상한.
/// 치명 연쇄는 Linux와 macOS에서 3건(원인 · 소멸자 · 런타임 abort), x86_64-pc-windows-msvc에서 2건(원인 · 소멸자)으로 관측됐다.
/// 상한은 OS와 무관하게, 3건 연쇄와 직전에 회복된 패닉 1건이 같은 초에 들어가는 값이다.
/// 회복되는 패닉이 폭주할 때 파일이 늘어나는 속도를 제한할 뿐 디스크 사용량의 상한은 아니다.
/// 넘친 패닉은 stderr에만 남는다.
/// 프로세스 총량 상한은 두지 않는다. 앞선 파일이 뒤의 치명 패닉 자리를 빼앗으면 안 된다.
const REPORT_FILES_PER_SECOND: u32 = 4;

/// 이미 있는 이름을 피해 다음 이름을 시도하는 횟수. 이름에 PID와 순번이 있어 충돌은
/// PID 재사용이나 시계 역행 때만 생기므로 작아도 된다.
const REPORT_NAME_ATTEMPTS: u32 = 100;

static PANIC_LOG: PanicLog = PanicLog::new();

/// 보고서마다 옮겨 적는 패닉의 위치와 메시지.
#[derive(Clone)]
struct PanicSummary {
    location: Option<String>,
    message: String,
}

impl PanicSummary {
    fn from_info(info: &panic::PanicHookInfo<'_>) -> Self {
        let payload = info.payload();
        let message = if let Some(msg) = payload.downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = payload.downcast_ref::<String>() {
            msg.clone()
        } else {
            "<unknown>".to_string()
        };
        Self {
            location: info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line())),
            message,
        }
    }
}

struct FirstPanic {
    summary: PanicSummary,
    at: Instant,
}

/// 프로세스의 패닉 순서와 파일 생성률. 패닉 훅에서 쓰므로 파일 I/O 동안 잠금을 잡지 않는다.
struct PanicLog {
    first: OnceLock<FirstPanic>,
    /// 지금까지 나눠 준 마지막 순번. 1번은 `first`를 채운 호출의 몫이다.
    last_sequence: AtomicU32,
    /// 상위 32비트는 첫 패닉 뒤 지난 초, 하위 32비트는 그 초에 허가한 파일 수.
    rate: AtomicU64,
}

impl PanicLog {
    const fn new() -> Self {
        Self {
            first: OnceLock::new(),
            last_sequence: AtomicU32::new(1),
            rate: AtomicU64::new(0),
        }
    }

    /// 순번을 받고 첫 패닉을 확정한다. backtrace 수집과 파일 열기보다 먼저 불러, 그 작업이
    /// 오래 걸려도 나중에 진입한 패닉이 첫 패닉으로 기록되지 않게 한다.
    fn register(&self, summary: &PanicSummary) -> (u32, &FirstPanic) {
        let mut won = false;
        let first = self.first.get_or_init(|| {
            won = true;
            FirstPanic {
                summary: summary.clone(),
                at: Instant::now(),
            }
        });
        let sequence = if won {
            1
        } else {
            self.last_sequence
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1)
        };
        (sequence, first)
    }

    /// 시각이 앞선 호출이 늦게 도착하면 현재 초의 몫을 쓴다. 시계 대신 첫 패닉 뒤의 경과
    /// 시간을 받으므로 시계 역행의 영향을 받지 않는다.
    fn admit_file(&self, second: u64) -> bool {
        let second = second.min(u64::from(u32::MAX));
        self.rate
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |state| {
                if second > state >> 32 {
                    Some((second << 32) | 1)
                } else if (state & u64::from(u32::MAX)) < u64::from(REPORT_FILES_PER_SECOND) {
                    Some(state + 1)
                } else {
                    None
                }
            })
            .is_ok()
    }
}

/// 보고서 하나의 내용. 시각과 PID를 값으로 받아 시험에서 이름 충돌을 강제할 수 있다.
struct PanicReport<'a> {
    timestamp: &'a str,
    pid: u32,
    sequence: u32,
    /// 이 패닉이 프로세스의 첫 패닉이면 None이다.
    first: Option<&'a PanicSummary>,
    panic: &'a PanicSummary,
    display: &'a dyn Display,
    backtrace: &'a dyn Display,
}

/// 같은 시각 · PID 안에서 이름순이 순번순이 되도록 순번을 u32 최대 자릿수로 맞춘다.
fn report_file_name(timestamp: &str, pid: u32, sequence: u32, attempt: u32) -> String {
    if attempt == 0 {
        format!("crash-{timestamp}-p{pid}-{sequence:010}.log")
    } else {
        format!("crash-{timestamp}-p{pid}-{sequence:010}-{attempt}.log")
    }
}

/// 이미 있는 보고서를 비우지 않도록 언제나 새 파일로 연다.
fn create_report_file(dir: &Path, report: &PanicReport<'_>) -> Option<(fs::File, PathBuf)> {
    for attempt in 0..REPORT_NAME_ATTEMPTS {
        let name = report_file_name(report.timestamp, report.pid, report.sequence, attempt);
        let path = dir.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Some((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

/// 보고서 파일을 쓰고 경로를 돌려준다. 버퍼 없이 바로 써서 심볼 해석이 오래 걸리거나 그 사이
/// 프로세스가 abort해도 앞의 위치와 메시지는 디스크에 남는다.
fn write_panic_report(dir: &Path, report: &PanicReport<'_>) -> Option<PathBuf> {
    fs::create_dir_all(dir).ok()?;
    let (mut file, path) = create_report_file(dir, report)?;

    write_report_header(&mut file, "Tasty Crash Report", report.timestamp);

    writeln!(file, "=== Process ===").ok();
    writeln!(file, "PID: {}", report.pid).ok();
    writeln!(file, "Panic sequence: {}", report.sequence).ok();
    if let Some(first) = report.first {
        if let Some(location) = &first.location {
            writeln!(file, "First panic location: {location}").ok();
        }
        writeln!(file, "First panic message: {}", first.message).ok();
    }
    writeln!(file).ok();

    writeln!(file, "=== Panic ===").ok();
    if let Some(location) = &report.panic.location {
        writeln!(file, "Location: {location}").ok();
    }
    writeln!(file, "Message: {}", report.panic.message).ok();
    writeln!(file, "Display: {}", report.display).ok();
    writeln!(file).ok();

    writeln!(file, "=== Backtrace ===").ok();
    writeln!(file, "{}", report.backtrace).ok();

    Some(path)
}

/// 패닉 하나를 파일에 기록하고 경로를 돌려준다. 생성률 제한에 걸렸거나 쓸 수 없으면 None이고,
/// 그 패닉은 호출자의 stderr 출력에만 남는다.
fn record_panic(
    info: &panic::PanicHookInfo<'_>,
    backtrace: &Backtrace,
    summary: &PanicSummary,
    sequence: u32,
    first: &FirstPanic,
) -> Option<PathBuf> {
    let admitted = PANIC_LOG.admit_file(first.at.elapsed().as_secs());
    if sequence != 1 && !admitted {
        return None;
    }
    let dir = crash_report_dir()?;
    let timestamp = format_timestamp(SystemTime::now());
    let report = PanicReport {
        timestamp: &timestamp,
        pid: std::process::id(),
        sequence,
        first: (sequence != 1).then_some(&first.summary),
        panic: summary,
        display: info,
        backtrace,
    };
    write_panic_report(&dir, &report)
}

/// 이벤트 루프 무응답 보고서를 별도 hang 파일로 남긴다.
/// 호스트 재시작이 공유 로그를 덮어써도 보고서는 유지된다.
#[cfg(feature = "gui")]
pub fn write_hang_report(site: &str, phase: &str, stuck_ms: u64) -> Option<PathBuf> {
    let dir = crash_report_dir()?;
    fs::create_dir_all(&dir).ok()?;

    let timestamp = format_timestamp(SystemTime::now());
    let path = dir.join(format!("hang-{timestamp}.log"));
    let mut file = fs::File::create(&path).ok()?;

    write_report_header(&mut file, "Tasty Hang Report", &timestamp);

    writeln!(file, "=== Stall ===").ok();
    writeln!(file, "Callback: {site}").ok();
    writeln!(file, "Render phase: {phase}").ok();
    writeln!(file, "Stuck for: {stuck_ms} ms").ok();
    writeln!(file).ok();
    writeln!(
        file,
        "The event loop made no observed progress within the watchdog threshold.\n\
         Main-loop input and IPC dispatch may be delayed while it is blocked.\n\
         The render phase identifies the last recorded region; it does not establish the cause.\n\
         This watchdog records the stall and does not cancel the blocked operation."
    )
    .ok();

    Some(path)
}

/// panic hook과 stderr·파일 로그 레이어를 설치한다. app_version은 본체 버전이다.
/// 파일은 여기서 열지 않으며 GUI/headless 호스트가 enable_host_file_log를 호출한 뒤에만 기록한다.
/// 그전 로그는 파일에 남지 않으므로 필요한 초기 진단은 stderr 또는 별도 보고서로 확인한다.
pub fn init(app_version: &'static str) {
    set_app_version(app_version);

    panic::set_hook(Box::new(|info| {
        let summary = PanicSummary::from_info(info);
        let (sequence, first) = PANIC_LOG.register(&summary);
        let backtrace = Backtrace::force_capture();

        // 파일을 먼저 쓴다. 아래 출력이 실패해 패닉하면 프로세스가 바로 abort한다.
        if let Some(path) = record_panic(info, &backtrace, &summary, sequence, first) {
            eprintln!("Tasty crashed! Report saved to: {}", path.display());
        }

        eprintln!("panic: {info}");
        eprintln!("{backtrace}");
    }));

    init_tracing();
}

/// 본체 버전을 한 번 저장한다. 이후 호출은 처음 값을 유지한다.
fn set_app_version(version: &'static str) {
    if APP_VERSION.set(version).is_err() {
        tracing::debug!(
            ignored = version,
            kept = app_version(),
            "crash report version already set; keeping the first value"
        );
    }
}

fn make_env_filter() -> EnvFilter {
    EnvFilter::try_from_env("TASTY_LOG").unwrap_or_else(|_| {
        EnvFilter::new("warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off")
    })
}

/// 파일 로그 싱크. host 프로세스가 [`enable_host_file_log`] 로 열어 넣기 전까지 비어
/// 있고, 그동안 파일 레이어가 만든 출력은 버려진다.
static LOG_FILE: OnceLock<Mutex<fs::File>> = OnceLock::new();

/// 파일 레이어의 writer 팩토리. `LOG_FILE` 이 채워진 프로세스에서만 실제로 쓴다.
struct HostLogWriter;

/// 이벤트 한 건을 쓰는 동안 파일 락을 잡는다 — `Mutex<File>` 의 기본 `MakeWriter` 구현과
/// 같은 원자성(한 줄이 다른 줄 사이에 끼어들지 않는다). 파일이 없으면 조용히 버린다.
enum HostLogSink {
    File(MutexGuard<'static, fs::File>),
    Discard,
}

impl io::Write for HostLogSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.write(buf),
            Self::Discard => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::Discard => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for HostLogWriter {
    type Writer = HostLogSink;

    fn make_writer(&'a self) -> Self::Writer {
        match LOG_FILE.get().map(Mutex::lock) {
            Some(Ok(guard)) => HostLogSink::File(guard),
            // 아직 안 열렸거나(= CLI 프로세스), 쓰는 도중 다른 스레드가 panic 해 poison
            // 된 경우. 로그 한 줄 때문에 여기서 다시 panic 하지 않는다.
            Some(Err(_)) | None => HostLogSink::Discard,
        }
    }
}

/// 파일 로그 파일명. dev 와 release 는 필터가 달라 파일도 나눈다.
fn log_file_name() -> &'static str {
    if cfg!(debug_assertions) {
        "debug-dev.log"
    } else {
        "debug.log"
    }
}

/// stderr의 TASTY_LOG와 독립된 파일 필터. debug 빌드는 debug, release는 warn 이상이다.
fn file_env_filter() -> EnvFilter {
    if cfg!(debug_assertions) {
        EnvFilter::new("debug,wgpu_hal=warn,wgpu_core=warn,naga=warn")
    } else {
        EnvFilter::new("warn,wgpu_hal=warn,wgpu_core=warn,naga=warn")
    }
}

/// stderr와 파일 레이어를 설치한다. 파일 출력은 enable_host_file_log 이후에만 시작한다.
fn init_tracing() {
    use tracing_subscriber::Layer as _;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // stdout은 CLI 결과용이므로 진단을 stderr에 보낸다.
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(make_env_filter());
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(HostLogWriter)
        .with_ansi(false)
        .with_filter(file_env_filter());

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();
}

/// GUI/headless 호스트의 파일 로그를 연다. CLI는 호출하지 않아야 호스트 로그를 덮어쓰지 않는다.
/// 순차 재호출은 파일을 열기 전에 거절한다. 열기에 실패하면 stderr만 사용한다.
/// 동시 호출을 직렬화하는 API는 아니므로 호스트 시작 경로에서 한 번 호출한다.
pub fn enable_host_file_log() {
    if let Some(reason) = install_host_log_file() {
        tracing::warn!("{reason}");
    }
}

/// 파일을 열어 [`LOG_FILE`] 에 넣는다. 로그로 남길 사유가 있으면 문자열로 돌려준다 —
/// `open_host_log_file` 과 같은 방식으로, warn 호출을 호출자 한 곳에 모은다.
fn install_host_log_file() -> Option<String> {
    // 파일을 열기 전에 판정한다 — 순서를 뒤집으면 재호출이 파일을 잘라먹는다(위 참조).
    if LOG_FILE.get().is_some() {
        return Some("file logging already enabled; ignoring repeat call".to_string());
    }
    let file = match open_host_log_file() {
        Ok(file) => file,
        Err(reason) => return Some(format!("file logging disabled: {reason}")),
    };
    if LOG_FILE.set(Mutex::new(file)).is_err() {
        // 위 검사와 이 사이에 다른 스레드가 먼저 넣은 경우(정상 부팅 경로에는 없지만
        // `pub` 이라 배제할 수 없다). 우리가 연 핸들은 그대로 버려진다.
        return Some("file logging enabled concurrently; dropping this handle".to_string());
    }
    None
}

/// 로그 파일을 연다(truncate). 실패 사유는 문자열로 올려보내 로그를 호출자 한 곳에서만
/// 남긴다 — 실패해도 stderr-only 로 도는 것이 정상 폴백이라 에러 타입까진 필요 없다.
fn open_host_log_file() -> Result<fs::File, String> {
    let dir = tasty_home().ok_or_else(|| "could not resolve tasty home".to_string())?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create log dir {}: {e}", dir.display()))?;
    let path = dir.join(log_file_name());
    fs::File::create(&path).map_err(|e| format!("cannot open log file {}: {e}", path.display()))
}

// =============================================================================
// Debug-only: error loop detection
// =============================================================================

#[cfg(all(feature = "gui", debug_assertions))]
pub mod error_loop {
    use std::sync::atomic::AtomicBool;
    use std::sync::{LazyLock, Mutex};
    use std::time::Instant;

    const WINDOW_SECS: u64 = 1;
    const THRESHOLD: usize = 100;

    /// 카운터·시각·문자열만 담는 락의 poison을 복구하고 한 번 보고한다.
    /// 복구를 포기하면 오류 반복 감지가 꺼진다. 임계 패닉은 락을 놓은 뒤 발생한다.
    static DETECTOR_POISONED: AtomicBool = AtomicBool::new(false);
    const DETECTOR_WHAT: &str = "error-loop detector";

    struct Inner {
        count: usize,
        window_start: Instant,
        last_msg: String,
    }

    pub struct ErrorLoopDetector {
        inner: Mutex<Inner>,
    }

    impl Default for ErrorLoopDetector {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ErrorLoopDetector {
        pub fn new() -> Self {
            Self {
                inner: Mutex::new(Inner {
                    count: 0,
                    window_start: Instant::now(),
                    last_msg: String::new(),
                }),
            }
        }

        /// 같은 메시지가 WINDOW_SECS 안에 THRESHOLD회에 도달하면 패닉으로 보고서를 남긴다.
        pub fn record(&self, msg: &str) {
            let mut inner = tasty_utils::poison::recover_mutex(
                self.inner.lock(),
                DETECTOR_WHAT,
                &DETECTOR_POISONED,
            );

            let now = Instant::now();
            let elapsed = now.duration_since(inner.window_start).as_secs();

            if elapsed >= WINDOW_SECS || inner.last_msg != msg {
                inner.count = 1;
                inner.window_start = now;
                inner.last_msg = msg.to_string();
                return;
            }

            inner.count += 1;

            if inner.count >= THRESHOLD {
                let count = inner.count;
                let last_msg = inner.last_msg.clone();
                drop(inner);
                panic!(
                    "Error loop detected! The following error repeated {count} times in {WINDOW_SECS}s:\n{last_msg}"
                );
            }
        }
    }

    /// Global error loop detector instance.
    static DETECTOR: LazyLock<ErrorLoopDetector> = LazyLock::new(ErrorLoopDetector::new);

    /// 반복 오류를 기록한다. 같은 메시지가 감지 창 안에서 임계 횟수에 도달하면 패닉한다.
    pub fn record_error(msg: &str) {
        DETECTOR.record(msg);
    }
}

#[cfg(all(feature = "gui", debug_assertions))]
pub use error_loop::record_error;

/// Record an error for loop detection (debug builds only, no-op in release).
#[cfg(all(feature = "gui", not(debug_assertions)))]
#[inline(always)]
pub fn record_error(_msg: &str) {}

#[cfg(test)]
mod panic_chain_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험이 넣는 버전. `APP_VERSION` 은 프로세스 전역 `OnceLock` 이라 한 번만 들어가므로,
    /// 이 모듈의 모든 시험이 **같은 값**을 넣는다 — 어느 시험이 먼저 돌든 결과가 같다.
    /// 이 크레이트의 버전(`CARGO_PKG_VERSION`)과 일부러 다른 값이다.
    const TEST_VERSION: &str = "9.8.7-crash-report-test";

    fn with_test_version() {
        set_app_version(TEST_VERSION);
        assert_eq!(
            app_version(),
            TEST_VERSION,
            "이 모듈 밖에서 다른 값이 먼저 들어갔다 — 전역 OnceLock 을 쓰는 시험이 늘었다"
        );
    }

    /// 머리의 `Version:` 은 [`init`] 이 받은 본체 버전이다 — 이 크레이트 자신의 버전이
    /// 아니다. crash · hang 두 보고서가 이 함수를 공유하므로 headless 조합에서도 잰다.
    #[test]
    fn the_report_header_carries_the_version_given_to_init() {
        with_test_version();
        let mut out = Vec::new();
        write_report_header(&mut out, "Tasty Crash Report", "2026-09-21T00-00-00");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(
            text.lines()
                .any(|l| l == format!("Version: {TEST_VERSION}")),
            "머리에 넘긴 버전이 없다:\n{text}"
        );
        assert_ne!(TEST_VERSION, env!("CARGO_PKG_VERSION"));
    }

    fn summary(location: &str, message: &str) -> PanicSummary {
        PanicSummary {
            location: Some(location.to_string()),
            message: message.to_string(),
        }
    }

    const FIXED_TIME: &str = "2026-11-09T01-02-03";
    const FIXED_PID: u32 = 4242;

    fn write_fixed(dir: &Path, sequence: u32, panic: &PanicSummary) -> Option<PathBuf> {
        write_panic_report(
            dir,
            &PanicReport {
                timestamp: FIXED_TIME,
                pid: FIXED_PID,
                sequence,
                first: None,
                panic,
                display: &"display text",
                backtrace: &"   0: frame",
            },
        )
    }

    /// 같은 시각 · PID · 순번이 다시 나오는 경우다(PID 재사용, 시계 역행). 초 경계에 기대지 않는다.
    #[test]
    fn a_name_collision_leaves_the_existing_report_byte_for_byte() {
        let dir = tempfile::tempdir().expect("temp dir");
        let earlier_panic = summary("a.rs:1", "earlier");
        let earlier = write_fixed(dir.path(), 1, &earlier_panic).expect("first report");
        let earlier_bytes = fs::read(&earlier).expect("read first report");

        let later_panic = summary("b.rs:2", "later");
        let later = write_fixed(dir.path(), 1, &later_panic).expect("second report");

        assert_ne!(earlier, later);
        assert_eq!(fs::read(&earlier).expect("reread"), earlier_bytes);
        let later_text = fs::read_to_string(&later).expect("read second report");
        assert!(later_text.contains("Message: later"), "{later_text}");
    }

    #[test]
    fn running_out_of_names_writes_nothing_and_keeps_every_report() {
        let dir = tempfile::tempdir().expect("temp dir");
        for attempt in 0..REPORT_NAME_ATTEMPTS {
            let name = report_file_name(FIXED_TIME, FIXED_PID, 1, attempt);
            fs::write(dir.path().join(name), "kept").expect("seed report");
        }
        let panic = summary("a.rs:1", "no room");
        assert_eq!(write_fixed(dir.path(), 1, &panic), None);
        for entry in fs::read_dir(dir.path()).expect("read dir") {
            let path = entry.expect("entry").path();
            assert_eq!(fs::read_to_string(path).expect("read"), "kept");
        }
    }

    #[test]
    fn report_names_keep_the_pattern_and_sort_by_sequence() {
        let sequences = [1, 2, 9, 10, 99, 100, 1000, 65_536, u32::MAX];
        let names: Vec<String> = sequences
            .iter()
            .map(|sequence| report_file_name(FIXED_TIME, FIXED_PID, *sequence, 0))
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);

        // 같은 초에 패닉한 프로세스들의 파일을 PID로 구별한다.
        assert!(
            names[0].contains(&format!("-p{FIXED_PID}-")),
            "{}",
            names[0]
        );
        assert_ne!(names[0], report_file_name(FIXED_TIME, FIXED_PID + 1, 1, 0));

        let collided = report_file_name(FIXED_TIME, FIXED_PID, 1, 3);
        for name in names.iter().chain([&collided]) {
            assert!(name.starts_with("crash-"), "{name}");
            assert!(name.ends_with(".log"), "{name}");
            assert!(
                !name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']),
                "{name}"
            );
        }
    }

    /// 패닉이 1번인 프로세스의 보고서는 원래 절을 유지하고 뒤 패닉 안내가 없다.
    #[test]
    fn a_single_panic_report_has_every_section_and_no_first_panic_lines() {
        with_test_version();
        let dir = tempfile::tempdir().expect("temp dir");
        let panic = summary("src/a.rs:7", "only panic");
        let path = write_fixed(dir.path(), 1, &panic).expect("report");
        let text = fs::read_to_string(path).expect("read report");
        for line in [
            "=== Tasty Crash Report ===",
            "Timestamp: 2026:11:09 01:02:03",
            "PID: 4242",
            "Panic sequence: 1",
            "=== Panic ===",
            "Location: src/a.rs:7",
            "Message: only panic",
            "Display: display text",
            "=== Backtrace ===",
            "   0: frame",
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "{line} is missing:\n{text}"
            );
        }
        assert!(text.lines().any(|l| l.starts_with("Version: ")), "{text}");
        assert!(text.lines().any(|l| l.starts_with("OS: ")), "{text}");
        assert!(!text.contains("First panic"), "{text}");
    }

    #[test]
    fn the_file_rate_admits_a_fatal_chain_and_resets_each_second() {
        let log = PanicLog::new();
        for _ in 0..3 {
            assert!(log.admit_file(0), "a fatal chain has to fit in one second");
        }
        let limit = REPORT_FILES_PER_SECOND - 3;
        for _ in 0..limit {
            assert!(log.admit_file(0));
        }
        assert!(!log.admit_file(0));
        assert!(log.admit_file(1), "the next second starts a new count");
        // 앞선 초를 들고 늦게 도착한 호출은 현재 초의 몫을 쓴다.
        for _ in 1..REPORT_FILES_PER_SECOND {
            assert!(log.admit_file(0));
        }
        assert!(!log.admit_file(1));
        assert!(
            log.admit_file(u64::MAX),
            "a huge elapsed time must not wrap"
        );
    }

    #[test]
    fn registering_from_many_threads_gives_one_first_panic_and_unique_sequences() {
        let log = PanicLog::new();
        let start = std::sync::Barrier::new(16);
        let results: Vec<(u32, String)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|i| {
                    let (log, start) = (&log, &start);
                    scope.spawn(move || {
                        start.wait();
                        let own = summary("a.rs:1", &format!("panic-{i}"));
                        let (sequence, first) = log.register(&own);
                        (sequence, first.summary.message.clone())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("thread"))
                .collect()
        });

        let mut sequences: Vec<u32> = results.iter().map(|(sequence, _)| *sequence).collect();
        sequences.sort_unstable();
        assert_eq!(sequences, (1..=16).collect::<Vec<u32>>());
        assert!(results.iter().all(|(_, first)| *first == results[0].1));
    }

    /// hang 보고서 파일을 격리 홈에 실제로 쓰고 머리를 읽는다.
    #[cfg(feature = "gui")]
    #[test]
    fn a_hang_report_written_under_tasty_home_carries_the_version_given_to_init() {
        with_test_version();
        let home = tasty_test_support::TastyHomeGuard::new();
        let path =
            write_hang_report("redraw", "present", 5000).expect("hang report 가 써져야 한다");
        assert!(path.starts_with(home.path().join("crash-reports")));
        let text = fs::read_to_string(&path).expect("read hang report");
        assert!(text.starts_with("=== Tasty Hang Report ==="));
        assert!(
            text.lines()
                .any(|l| l == format!("Version: {TEST_VERSION}")),
            "hang 보고서 머리에 넘긴 버전이 없다:\n{text}"
        );
    }
}
