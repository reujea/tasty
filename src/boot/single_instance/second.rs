//! release 두 번째 프로세스. 같은 데이터 홈의 Tasty가 이미 실행 중일 때 창·GPU·이벤트 루프 없이
//! 실행 중인 Tasty에 요청만 넘기고 종료한다.
//!
//! - 사용자 실행 증거가 있으면 OS 활성화 경로를 쓴다(Linux D-Bus `Activate`, Windows 등록 창 메시지).
//! - 증거가 없으면 `tasty new window`와 같은 `window.create`로 새 View 하나를 요청한다.
//! - 실행 중인 쪽이 아직 부팅 중이면 상한 안에서 기다리고, 그 사이 잠금이 풀리면 평소처럼 부팅한다.
//! - 넘기다 연결이 끊기거나 D-Bus 이름 소유자가 사라지면 인스턴스 기록과 잠금을 다시 확인한다. 실행 중이던
//!   쪽이 끝나 잠금을 얻으면 평소처럼 부팅한다.
//! - 요청을 썼지만 기한 안에 확인을 못 받았고 실행 중인 쪽이 살아 있으면 경고만 남기고 성공으로 끝낸다.
//!   늦게라도 그쪽이 처리한다.
//! - 요청을 넘기지 못했고 잠금도 얻지 못하면 Tasty 창 없이 OS 메시지 상자로 알리고 종료한다.
//! - `--webhook-port`로 실행했는데 실행 중인 쪽의 웹훅 포트와 다르면 넘기지 않고 같은 방법으로 알린 뒤
//!   종료 코드 1로 끝낸다. 같으면 평소처럼 넘긴다. 실행 중인 쪽의 포트를 조회하지 못하면 평소처럼 넘긴다.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use super::evidence::{self, LaunchEvidence};
use super::instance_file::{self, InstanceRecord, InstanceState};
use super::launch_log::LaunchLog;

/// 실행 중인 Tasty가 인스턴스 파일에 포트를 쓸 때까지 기다리는 상한.
const BOOT_WAIT: Duration = Duration::from_secs(20);
/// `window.create` 요청 전체(연결·재시도 포함)의 상한.
const IPC_DEADLINE: Duration = Duration::from_secs(10);
/// D-Bus 이름 소유자가 나타나기를 기다리는 상한과 메서드 응답 상한.
#[cfg(target_os = "linux")]
const OWNER_WAIT: Duration = Duration::from_secs(5);
#[cfg(target_os = "linux")]
const DBUS_METHOD_TIMEOUT: Duration = Duration::from_secs(5);
/// 대체 동작의 종료 코드. 요청을 넘기지 못했다.
const FALLBACK_EXIT: u8 = 1;
/// 넘김 실패 뒤 다시 기다려 넘기는 최대 횟수. 실행 중이던 쪽이 끝나고 다른 인스턴스가 뜬 경우만 다시 돈다.
const MAX_ROUNDS: u32 = 3;

pub(crate) enum HeldOutcome {
    Exit(ExitCode),
    /// 기다리는 동안 실행 중이던 Tasty가 끝나 잠금을 얻었다. 평소처럼 부팅한다.
    BootNormally(tasty_event_store::WriterLock),
}

/// 실행 중인 Tasty에 넘길 경로.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    /// 증거 없음: `window.create`로 새 View 하나.
    NewView,
    /// Linux 증거 있음: D-Bus `Activate`.
    DbusActivate,
    /// Windows 포그라운드 권한 있음: 등록 창 메시지(창이 없으면 `window.create` 후 포그라운드 요청).
    WindowsActivate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Linux,
    Windows,
    MacOs,
}

impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// 증거로 경로를 정한다. macOS의 Finder·Dock·`open` 실행은 LaunchServices가 두 번째 프로세스를
/// 만들지 않으므로, 두 번째 프로세스가 있으면 바이너리를 직접 실행한 것으로 보고 증거 없음이다.
pub(crate) fn route(
    os: Platform,
    evidence: &LaunchEvidence,
    windows_foreground_granted: bool,
) -> Route {
    match os {
        Platform::Linux if evidence.is_present() => Route::DbusActivate,
        Platform::Windows if windows_foreground_granted => Route::WindowsActivate,
        _ => Route::NewView,
    }
}

fn display_backend() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        crate::boot::display_backend::DisplayEnv::current().expected_backend_name()
    }
}

/// 같은 홈의 writer 잠금이 다른 프로세스에 있을 때 release가 부른다.
pub(crate) fn handle_held(home: &Path, webhook_port: Option<u16>) -> HeldOutcome {
    let started = Instant::now();
    let log = LaunchLog::new(Some(home));
    let canonical = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let evidence = evidence::take();
    log.line(&format!(
        "second launch: version={} home={} os={} backend={} evidence={:?}",
        env!("CARGO_PKG_VERSION"),
        canonical.display(),
        std::env::consts::OS,
        display_backend(),
        evidence.kinds()
    ));
    let code = match hand_over(home, &evidence, webhook_port, &log, started) {
        Handover::Delivered => ExitCode::SUCCESS,
        Handover::Unconfirmed(reason) => {
            log.line(&format!(
                "warning: the request was written but not confirmed ({reason}); \
                 the running instance is alive, so no notice is shown"
            ));
            ExitCode::SUCCESS
        }
        Handover::BootNormally(lock) => {
            log.line(&format!(
                "the running instance ended ({} ms); booting normally",
                started.elapsed().as_millis()
            ));
            return HeldOutcome::BootNormally(lock);
        }
        Handover::GiveUp(reason) => give_up(&log, &evidence, &reason),
        Handover::WebhookPortDiffers { wanted, running } => {
            refuse_webhook_port(&log, wanted, running)
        }
    };
    log.line(&format!(
        "second launch done in {} ms",
        started.elapsed().as_millis()
    ));
    HeldOutcome::Exit(code)
}

/// 넘김의 결과. 화면에 아무것도 띄우지 않고 정하기만 한다.
#[derive(Debug)]
enum Handover {
    Delivered,
    /// 요청을 썼지만 확인을 못 받았고, 실행 중인 쪽은 아직 살아 있다.
    Unconfirmed(String),
    /// 실행 중이던 쪽이 끝나 잠금을 얻었다.
    BootNormally(tasty_event_store::WriterLock),
    /// 넘기지 못했고 잠금도 얻지 못했다.
    GiveUp(String),
    /// 실행 인자의 웹훅 포트가 실행 중인 쪽과 다르다. 넘기지 않는다.
    WebhookPortDiffers {
        wanted: u16,
        running: Option<u16>,
    },
}

/// 넘기다 실패한 이유. `written`은 실행 중인 쪽에 연결된 뒤 실패해 요청이 닿았을 수 있다는 뜻이다.
#[derive(Debug)]
struct Undelivered {
    written: bool,
    reason: String,
}

impl Undelivered {
    fn lost(reason: impl Into<String>) -> Self {
        Self {
            written: false,
            reason: reason.into(),
        }
    }
}

/// 기다림 → 넘김 → 실패하면 기록과 잠금 재확인. 재확인은 `wait_for_instance`와 같은 규칙이다.
fn hand_over(
    home: &Path,
    evidence: &LaunchEvidence,
    webhook_port: Option<u16>,
    log: &LaunchLog,
    started: Instant,
) -> Handover {
    let mut round = 0u32;
    loop {
        round += 1;
        let record = match wait_for_instance(home, log) {
            Waited::Live(record) => record,
            Waited::LockReleased(lock) => return Handover::BootNormally(lock),
            Waited::TimedOut(reason) => {
                return Handover::GiveUp(format!("instance not ready: {reason}"));
            }
        };
        log.line(&format!(
            "instance live: pid={} start_time={} port={:?} after {} ms",
            record.pid,
            record.start_time,
            record.port,
            started.elapsed().as_millis()
        ));
        if let Some(wanted) = webhook_port {
            match running_webhook_port(&record) {
                Ok(running) if running == Some(wanted) => {
                    log.line(&format!(
                        "webhook port {wanted} matches the running instance"
                    ));
                }
                Ok(running) => return Handover::WebhookPortDiffers { wanted, running },
                // 묻지 못했다는 이유로 실행을 막지 않는다. 포트가 다를 수 있으니 경고로 남긴다.
                Err(reason) => {
                    let message = format!(
                        "warning: could not ask the running instance for its webhook port ({reason}); handing over without checking --webhook-port {wanted}"
                    );
                    tracing::warn!("{message}");
                    log.line(&message);
                }
            }
        }
        let failure = match deliver(home, &record, evidence, log) {
            Ok(()) => return Handover::Delivered,
            Err(failure) => failure,
        };
        if failure.written && is_running(home, &record) {
            return Handover::Unconfirmed(failure.reason);
        }
        log.line(&format!(
            "not delivered: {}; checking the instance record and the writer lock again",
            failure.reason
        ));
        let database = crate::runtime::journal_product::journal_database_path(home);
        match tasty_event_store::preempt_writer_lock(&database) {
            Ok(tasty_event_store::WriterPreempt::Acquired(lock)) => {
                return Handover::BootNormally(lock);
            }
            Ok(tasty_event_store::WriterPreempt::Held) => {}
            Err(e) => log.line(&format!("writer lock check failed: {e}")),
        }
        // 같은 인스턴스가 살아 있는데 넘기지 못했으면 더 기다려도 소용없다.
        if is_running(home, &record) || round >= MAX_ROUNDS {
            return Handover::GiveUp(failure.reason);
        }
    }
}

/// 기록이 여전히 같은 프로세스(PID·시작 시각)를 살아 있는 인스턴스로 가리키는지.
fn is_running(home: &Path, record: &InstanceRecord) -> bool {
    matches!(
        instance_file::read_state(home),
        InstanceState::Live(now) if now.pid == record.pid && now.start_time == record.start_time
    )
}

fn deliver(
    home: &Path,
    record: &InstanceRecord,
    evidence: &LaunchEvidence,
    log: &LaunchLog,
) -> Result<(), Undelivered> {
    let granted = windows_grant(record.pid, log);
    let chosen = route(Platform::current(), evidence, granted);
    log.line(&format!("route: {chosen:?}"));
    match chosen {
        Route::NewView => request_new_view(home, record, log)
            .map(|window_id| log.line(&format!("new view requested: window_id={window_id}"))),
        Route::DbusActivate => dbus_activate(home, record, evidence, log),
        Route::WindowsActivate => windows_activate(home, record, log),
    }
}

enum Waited {
    Live(InstanceRecord),
    LockReleased(tasty_event_store::WriterLock),
    TimedOut(String),
}

/// 포트가 있는 살아 있는 기록이 나올 때까지 기다린다. 잠금 시도는 writer 잠금의 재시도 구간을 쓰므로
/// 그 자체가 대기 간격이 된다.
fn wait_for_instance(home: &Path, log: &LaunchLog) -> Waited {
    let deadline = Instant::now() + BOOT_WAIT;
    let database = crate::runtime::journal_product::journal_database_path(home);
    let mut last_reason = String::new();
    loop {
        match instance_file::read_state(home) {
            InstanceState::Live(record) => return Waited::Live(record),
            InstanceState::Booting(reason) => {
                let text = format!("{reason:?}");
                if text != last_reason {
                    log.line(&format!("waiting for the running instance: {text}"));
                    last_reason = text;
                }
            }
        }
        if Instant::now() >= deadline {
            return Waited::TimedOut(format!("{last_reason} after {} ms", BOOT_WAIT.as_millis()));
        }
        match tasty_event_store::preempt_writer_lock(&database) {
            Ok(tasty_event_store::WriterPreempt::Acquired(lock)) => {
                return Waited::LockReleased(lock);
            }
            Ok(tasty_event_store::WriterPreempt::Held) => {}
            Err(e) => {
                log.line(&format!("writer lock check failed: {e}"));
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// `window.create`를 멱등 키와 함께 보낸다. 응답이 유실되면 같은 키로, 명시적으로 거절되면 새 키로 다시 보낸다.
/// 연결 오류가 나면 실행 중인 쪽이 아직 살아 있는지 확인하고, 끝났으면 기한을 기다리지 않고 돌아온다.
fn request_new_view(
    home: &Path,
    record: &InstanceRecord,
    log: &LaunchLog,
) -> Result<u64, Undelivered> {
    request_new_view_within(home, record, log, IPC_DEADLINE, &WallClock)
}

/// 재시도 루프가 마감을 판단할 시각과 재시도 사이의 대기. 시험은 가짜 시계를 넣어 기계 부하와 무관하게
/// 마감 동작을 판정한다.
trait RetryClock {
    fn now(&self) -> Instant;
    fn pause(&self, duration: Duration);
}

struct WallClock;

impl RetryClock for WallClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn request_new_view_within(
    home: &Path,
    record: &InstanceRecord,
    log: &LaunchLog,
    limit: Duration,
    clock: &dyn RetryClock,
) -> Result<u64, Undelivered> {
    let Some(port) = record.port else {
        return Err(Undelivered::lost("instance record has no port"));
    };
    let deadline = clock.now() + limit;
    let mut key = new_key();
    let mut attempt = 0u32;
    let mut written = false;
    loop {
        attempt += 1;
        let left = deadline.saturating_duration_since(clock.now());
        if left.is_zero() {
            return Err(Undelivered {
                written,
                reason: format!(
                    "window.create gave no result within {} ms ({attempt} attempts)",
                    limit.as_millis()
                ),
            });
        }
        let transport_error = match send_window_create(port, &key, left) {
            Ok(value) => {
                return value
                    .get("window_id")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| Undelivered {
                        written: true,
                        reason: format!("window.create returned no window_id: {value}"),
                    });
            }
            Err(SendError::Refused { code, message }) => {
                log.line(&format!(
                    "window.create refused (attempt {attempt}): code={code} message={message}"
                ));
                key = new_key();
                None
            }
            Err(SendError::Unreachable(e)) => Some(e),
            Err(SendError::Transport(e)) => {
                written = true;
                Some(e)
            }
        };
        if let Some(e) = transport_error {
            log.line(&format!(
                "window.create transport error (attempt {attempt}): {e}"
            ));
            if !is_running(home, record) {
                return Err(Undelivered {
                    written,
                    reason: format!("the running instance ended: {e}"),
                });
            }
        }
        clock.pause(Duration::from_millis(200).min(left));
    }
}

enum SendError {
    Refused {
        code: i32,
        message: String,
    },
    /// 연결이나 기능 확인 단계에서 실패했다. `window.create`는 보내지 않았다.
    Unreachable(String),
    /// `window.create`를 보낸 뒤 실패했다. 요청이 닿았을 수 있다.
    Transport(String),
}

fn new_key() -> String {
    format!(
        "single-instance-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    )
}

fn send_window_create(
    port: u16,
    key: &str,
    left: Duration,
) -> Result<serde_json::Value, SendError> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let transport = |e: &dyn std::fmt::Display| SendError::Transport(e.to_string());
    let stream = std::net::TcpStream::connect_timeout(&address, left)
        .map_err(|e| SendError::Unreachable(e.to_string()))?;
    stream
        .set_read_timeout(Some(left))
        .map_err(|e| SendError::Unreachable(e.to_string()))?;
    stream
        .set_write_timeout(Some(left))
        .map_err(|e| SendError::Unreachable(e.to_string()))?;
    let unreachable = |e: &dyn std::fmt::Display| SendError::Unreachable(e.to_string());
    let mut connection =
        tasty_ipc::client::IpcConnection::new(stream).map_err(|e| unreachable(&e))?;
    // `send_idempotent`는 먼저 기능 확인 요청을 보내고 응답을 기다린다. 멈춘 인스턴스는 여기서 막히므로
    // 따로 먼저 불러 "본 요청을 아직 보내지 않음"과 "보낸 뒤 실패"를 가른다. 결과는 연결에 저장된다.
    connection
        .require_capability(
            tasty_ipc::client::IDEMPOTENCY_CAPABILITY,
            tasty_ipc::client::IDEMPOTENCY_CAPABILITY_VERSION,
            None,
        )
        .map_err(|e| unreachable(&e))?;
    let request = tasty_ipc::protocol::JsonRpcRequest {
        caller_agent_id: None,
        jsonrpc: "2.0".to_string(),
        method: "window.create".to_string(),
        params: serde_json::json!({}),
        id: Some(serde_json::json!(1)),
        session_token: None,
        response_timeout_ms: None,
        idempotency_key: None,
    };
    connection
        .send_idempotent(&request, key)
        .map_err(
            |e| match e.downcast_ref::<tasty_ipc::client::JsonRpcCallError>() {
                Some(call) => SendError::Refused {
                    code: call.code,
                    message: call.message.clone(),
                },
                None => transport(&e),
            },
        )
}

#[cfg(target_os = "linux")]
fn dbus_activate(
    home: &Path,
    record: &InstanceRecord,
    evidence: &LaunchEvidence,
    log: &LaunchLog,
) -> Result<(), Undelivered> {
    let name = super::dbus::bus_name(home);
    let connection = super::dbus::connect(DBUS_METHOD_TIMEOUT)
        .map_err(|e| Undelivered::lost(format!("session bus: {e}")))?;
    let deadline = Instant::now() + OWNER_WAIT;
    loop {
        match super::dbus::name_has_owner(&connection, &name) {
            Ok(true) => break,
            Ok(false) if Instant::now() < deadline && is_running(home, record) => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Ok(false) => {
                return Err(Undelivered::lost(format!("D-Bus name {name} has no owner")));
            }
            Err(e) => {
                return Err(Undelivered::lost(format!("NameHasOwner({name}): {e}")));
            }
        }
    }
    match super::dbus::activate(&connection, &name, evidence) {
        Ok(()) => {
            log.line(&format!("D-Bus Activate sent to {name}"));
            Ok(())
        }
        // 메서드 호출은 소유자에게 보낸 뒤 응답을 기다리다 실패할 수 있다.
        Err(e) => Err(Undelivered {
            written: true,
            reason: format!("Activate on {name}: {e}"),
        }),
    }
}

#[cfg(not(target_os = "linux"))]
fn dbus_activate(
    _home: &Path,
    _record: &InstanceRecord,
    _evidence: &LaunchEvidence,
    _log: &LaunchLog,
) -> Result<(), Undelivered> {
    Err(Undelivered::lost("D-Bus activation is only used on Linux"))
}

/// 넘기지 못했다. X11 증거가 있으면 실행기의 대기 표시를 직접 끝낸 뒤 대체 동작으로 간다.
#[cfg(target_os = "linux")]
fn give_up(log: &LaunchLog, evidence: &LaunchEvidence, reason: &str) -> ExitCode {
    log.line(&format!("activation failed: {reason}"));
    if let Some(id) = &evidence.x11_startup_id {
        match crate::platform::window_activation::x11_startup_complete(id) {
            Ok(()) => log.line("sent X11 startup-notification remove"),
            Err(e) => log.line(&format!("X11 startup-notification remove failed: {e}")),
        }
    }
    fallback(log, reason)
}

#[cfg(not(target_os = "linux"))]
fn give_up(log: &LaunchLog, _evidence: &LaunchEvidence, reason: &str) -> ExitCode {
    fallback(log, reason)
}

#[cfg(windows)]
fn windows_grant(pid: u32, log: &LaunchLog) -> bool {
    match crate::platform::single_instance_windows::allow_set_foreground(pid) {
        Ok(()) => {
            log.line(&format!("AllowSetForegroundWindow({pid}) succeeded"));
            true
        }
        Err(e) => {
            log.line(&format!("AllowSetForegroundWindow({pid}) refused: {e}"));
            false
        }
    }
}

#[cfg(not(windows))]
fn windows_grant(_pid: u32, _log: &LaunchLog) -> bool {
    false
}

#[cfg(windows)]
fn windows_activate(
    home: &Path,
    record: &InstanceRecord,
    log: &LaunchLog,
) -> Result<(), Undelivered> {
    use crate::platform::single_instance_windows as win;
    let find = |log: &LaunchLog| match win::enumerate_top_level() {
        Ok(windows) => {
            let found = win::pick_main_view(&windows, record.pid);
            log.line(&format!(
                "enumerated {} top-level windows; main view of pid {}: {}",
                windows.len(),
                record.pid,
                found.is_some()
            ));
            found
        }
        Err(e) => {
            log.line(&format!("window enumeration failed: {e}"));
            None
        }
    };
    let post = |hwnd: isize, log: &LaunchLog| match win::post_activate(hwnd) {
        Ok(()) => {
            log.line("posted the activate message");
            Ok(())
        }
        Err(e) => Err(Undelivered::lost(format!("post activate: {e}"))),
    };
    if let Some(hwnd) = find(log) {
        return post(hwnd, log);
    }
    // 창이 없으면 연속 실행이 각자 창을 만들지 않도록 차례로 처리하고 다시 확인한다.
    let _turn =
        activation_turn(home).map_err(|e| Undelivered::lost(format!("activation lock: {e}")))?;
    if let Some(hwnd) = find(log) {
        return post(hwnd, log);
    }
    let window_id = request_new_view(home, record, log)?;
    let granted = win::set_foreground(window_id);
    log.line(&format!(
        "new view {window_id} requested; SetForegroundWindow returned {granted}"
    ));
    Ok(())
}

#[cfg(not(windows))]
fn windows_activate(
    _home: &Path,
    _record: &InstanceRecord,
    _log: &LaunchLog,
) -> Result<(), Undelivered> {
    Err(Undelivered::lost(
        "window messages are only used on Windows",
    ))
}

/// 증거 있음 + 창 없음 처리를 차례로 하기 위한 홈의 활성화 잠금.
#[cfg(windows)]
fn activation_turn(home: &Path) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("activation.lock"))?;
    let deadline = Instant::now() + IPC_DEADLINE;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "activation lock is busy",
                ));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// 실행 중인 쪽의 리스너 포트를 `webhook.config` 조회로 묻는다. 리스너가 없으면 `Ok(None)`이다.
fn running_webhook_port(record: &InstanceRecord) -> Result<Option<u16>, String> {
    let port = record.port.ok_or("instance record has no port")?;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let stream =
        std::net::TcpStream::connect_timeout(&address, IPC_DEADLINE).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(IPC_DEADLINE))
        .map_err(|e| e.to_string())?;
    let mut connection =
        tasty_ipc::client::IpcConnection::new(stream).map_err(|e| e.to_string())?;
    let request = tasty_ipc::protocol::JsonRpcRequest {
        caller_agent_id: None,
        jsonrpc: "2.0".to_string(),
        method: "webhook.config".to_string(),
        params: serde_json::json!({}),
        id: Some(serde_json::json!(1)),
        session_token: None,
        response_timeout_ms: None,
        idempotency_key: None,
    };
    let status = connection.send(&request).map_err(|e| e.to_string())?;
    Ok(status
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .and_then(|p| u16::try_from(p).ok()))
}

/// `--webhook-port`가 실행 중인 쪽과 달라 넘기지 않고 끝낸다. launch.log(표준 오류 사본 포함)와 메시지 상자로 알린다.
fn refuse_webhook_port(log: &LaunchLog, wanted: u16, running: Option<u16>) -> ExitCode {
    crate::boot::locale::init();
    let title = crate::i18n::t("boot.webhook_port.title").to_string();
    let body = match running {
        Some(running) => crate::i18n::t_fmt2(
            "boot.webhook_port.differs",
            &running.to_string(),
            &wanted.to_string(),
        ),
        None => crate::i18n::t_fmt("boot.webhook_port.differs_none", &wanted.to_string()),
    };
    // launch.log 줄은 표준 오류에도 같이 나간다.
    log.line(&format!(
        "refused: --webhook-port {wanted} differs from the running instance ({running:?}): {body}"
    ));
    let shown = show_notice(&title, &body);
    log.line(&format!("notice: {shown}; exit code {FALLBACK_EXIT}"));
    ExitCode::from(FALLBACK_EXIT)
}

/// Tasty 창 없이 원인을 알리고 끝낸다. 문구는 하나이고 상세는 launch.log에 있다.
fn fallback(log: &LaunchLog, reason: &str) -> ExitCode {
    log.line(&format!("fallback: {reason}"));
    crate::boot::locale::init();
    let title = crate::i18n::t("app.name").to_string();
    let body = crate::i18n::t("boot.already_running.body").to_string();
    let shown = show_notice(&title, &body);
    log.line(&format!("notice: {shown}; exit code {FALLBACK_EXIT}"));
    ExitCode::from(FALLBACK_EXIT)
}

/// 메시지 상자를 띄운다. Linux 메시지 상자는 외부 프로그램 zenity에 기대므로 없으면 데스크톱 알림으로 바꾼다.
/// 창을 만들 수 없는 부팅 실패도 이 경로로 원인을 보인다. 반환값은 실제로 쓴 표시 수단이다.
pub(crate) fn show_notice(title: &str, body: &str) -> String {
    #[cfg(target_os = "linux")]
    {
        if !program_on_path("zenity") {
            return match super::dbus::notify(title, body) {
                Ok(()) => "desktop notification (zenity not found)".to_string(),
                Err(e) => format!("not shown: zenity not found and notification failed: {e}"),
            };
        }
    }
    let result = rfd::MessageDialog::new()
        .set_title(title)
        .set_description(body)
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    format!("message box ({result:?})")
}

#[cfg(target_os = "linux")]
fn program_on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(name);
            std::fs::metadata(&candidate).is_ok_and(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(wayland: Option<&str>, x11: Option<&str>) -> LaunchEvidence {
        LaunchEvidence::from_values(wayland.map(str::to_string), x11.map(str::to_string))
    }

    #[test]
    fn linux_routes_by_the_launcher_evidence() {
        assert_eq!(
            route(Platform::Linux, &with(None, None), false),
            Route::NewView
        );
        assert_eq!(
            route(Platform::Linux, &with(Some("t"), None), false),
            Route::DbusActivate
        );
        assert_eq!(
            route(Platform::Linux, &with(None, Some("id")), false),
            Route::DbusActivate
        );
    }

    #[test]
    fn windows_routes_by_the_foreground_grant_only() {
        assert_eq!(
            route(Platform::Windows, &with(None, None), true),
            Route::WindowsActivate
        );
        assert_eq!(
            route(Platform::Windows, &with(None, Some("id")), false),
            Route::NewView
        );
    }

    #[test]
    fn a_direct_second_process_on_macos_never_activates() {
        assert_eq!(
            route(Platform::MacOs, &with(Some("t"), Some("id")), true),
            Route::NewView
        );
    }

    #[test]
    fn a_released_lock_while_waiting_boots_normally() {
        let home = tempfile::tempdir().unwrap();
        // 인스턴스 파일도 잠금 소유자도 없다 → 첫 잠금 시도에서 얻는다.
        let log = LaunchLog::new(Some(home.path()));
        assert!(matches!(
            wait_for_instance(home.path(), &log),
            Waited::LockReleased(_)
        ));
        let text = std::fs::read_to_string(super::super::launch_log::path(home.path())).unwrap();
        assert!(
            text.contains("waiting for the running instance: NoFile"),
            "{text}"
        );
    }

    #[test]
    fn a_live_record_is_returned_without_touching_the_lock() {
        let home = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let record = InstanceRecord {
            pid,
            start_time: instance_file::process_start_time(pid).unwrap(),
            port: Some(9),
        };
        std::fs::write(
            instance_file::path(home.path()),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let log = LaunchLog::new(Some(home.path()));
        assert!(matches!(wait_for_instance(home.path(), &log), Waited::Live(r) if r == record));
    }

    /// 이 프로세스(살아 있음)를 가리키는 기록을 홈에 쓴다.
    fn publish_self(home: &Path, port: u16) -> InstanceRecord {
        let pid = std::process::id();
        let record = InstanceRecord {
            pid,
            start_time: instance_file::process_start_time(pid).unwrap(),
            port: Some(port),
        };
        std::fs::write(
            instance_file::path(home),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        record
    }

    /// 시험이 끝낼 때까지 살아 있는 자식. Windows 에는 `sleep` 이 없어 `ping` 으로 기다린다.
    fn long_lived_child() -> std::process::Child {
        #[cfg(unix)]
        let mut command = std::process::Command::new("sleep");
        #[cfg(unix)]
        command.arg("30");
        #[cfg(windows)]
        let mut command = std::process::Command::new("ping");
        #[cfg(windows)]
        command
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null());
        command.spawn().unwrap()
    }

    /// 재시도 사이 대기만큼만 흐르는 시계. 마감 판정이 기계 부하에 흔들리지 않는다. 마감을 넘겨 기다리거나
    /// 남은 시간 없이 다시 기다리면 마감을 무시한 것이므로 그 자리에서 실패한다(무한 재시도 대신).
    struct FakeClock {
        base: Instant,
        elapsed: std::cell::Cell<Duration>,
        limit: Duration,
    }

    impl FakeClock {
        fn new(limit: Duration) -> Self {
            Self {
                base: Instant::now(),
                elapsed: std::cell::Cell::new(Duration::ZERO),
                limit,
            }
        }
    }

    impl RetryClock for FakeClock {
        fn now(&self) -> Instant {
            self.base + self.elapsed.get()
        }

        fn pause(&self, duration: Duration) {
            let elapsed = self.elapsed.get() + duration;
            assert!(
                !duration.is_zero() && elapsed <= self.limit,
                "retried past the deadline: paused {duration:?} at {:?} of {:?}",
                self.elapsed.get(),
                self.limit
            );
            self.elapsed.set(elapsed);
        }
    }

    fn closed_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    #[test]
    fn an_unreachable_running_instance_ends_at_the_deadline_as_not_written() {
        // 기록의 프로세스는 살아 있는데 포트가 아무도 듣지 않는다: 기한까지 재시도하고 "닿지 않음"으로 끝난다.
        let home = tempfile::tempdir().unwrap();
        let record = publish_self(home.path(), closed_port());
        let log = LaunchLog::new(Some(home.path()));
        // 기한은 가짜 시계로 잰다. 재시도 간격 200ms로 800ms를 채우면 다섯 번째 시도에서 남은 시간이 0이다.
        let limit = Duration::from_millis(800);
        let clock = FakeClock::new(limit);
        let error = request_new_view_within(home.path(), &record, &log, limit, &clock).unwrap_err();
        assert!(
            error.reason.contains("window.create gave no result"),
            "{error:?}"
        );
        assert!(error.reason.contains("(5 attempts)"), "{error:?}");
        assert!(!error.written);
        assert_eq!(clock.elapsed.get(), limit);
        let text = std::fs::read_to_string(super::super::launch_log::path(home.path())).unwrap();
        assert!(text.contains("transport error"), "{text}");
    }

    #[test]
    fn a_running_instance_that_dies_mid_request_hands_the_launch_back_to_boot() {
        // 리뷰 재현의 시험판: 실행 중인 A가 잠금을 쥐고 요청을 받아 둔 채(멈춤) 강제 종료된다.
        // B는 기한(10초)을 기다리지 않고 기록·잠금을 다시 확인해 스스로 부팅해야 한다(유실 0).
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("structure")).unwrap();
        let database = crate::runtime::journal_product::journal_database_path(home.path());
        let Ok(tasty_event_store::WriterPreempt::Acquired(lock)) =
            tasty_event_store::preempt_writer_lock(&database)
        else {
            panic!("the test home lock must be free");
        };
        let mut a = long_lived_child();
        // 연결은 받지만(backlog) 응답하지 않는 멈춘 A의 포트.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let record = InstanceRecord {
            pid: a.id(),
            start_time: instance_file::process_start_time(a.id()).unwrap(),
            port: Some(listener.local_addr().unwrap().port()),
        };
        std::fs::write(
            instance_file::path(home.path()),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        // 고정 지연 대신 B의 연결을 받은 뒤 A를 죽인다. 부하로 B가 늦어도 "요청을 받아 둔 채 죽음" 순서가
        // 지켜진다.
        let killer = std::thread::spawn(move || {
            let (held, _) = listener.accept().unwrap();
            a.kill().unwrap();
            a.wait().unwrap();
            drop(held);
            drop(listener);
            drop(lock);
        });
        let log = LaunchLog::new(Some(home.path()));
        let started = Instant::now();
        let outcome = hand_over(home.path(), &LaunchEvidence::default(), None, &log, started);
        killer.join().unwrap();
        let text = std::fs::read_to_string(super::super::launch_log::path(home.path())).unwrap();
        assert!(
            matches!(outcome, Handover::BootNormally(_)),
            "{outcome:?}\n{text}"
        );
        // 기한(10초)까지 기다렸다면 사유가 "gave no result within"이다. 벽시계 대신 끝난 사유로 판정한다.
        assert!(text.contains("instance live"), "{text}");
        assert!(
            text.contains("not delivered: the running instance ended"),
            "{text}"
        );
        assert!(!text.contains("gave no result"), "{text}");
    }

    #[test]
    fn a_live_but_silent_instance_never_receives_the_request() {
        // 연결은 되지만 기능 확인에도 답하지 않는 인스턴스(멈춘 A): window.create는 보내지 않았으므로
        // "썼다"로 보지 않는다. 그렇게 보면 성공으로 끝나 실행 요청이 사라진다.
        let home = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let record = publish_self(home.path(), listener.local_addr().unwrap().port());
        let log = LaunchLog::new(Some(home.path()));
        let error = request_new_view_within(
            home.path(),
            &record,
            &log,
            Duration::from_millis(600),
            &WallClock,
        )
        .unwrap_err();
        assert!(!error.written, "{error:?}");
        assert!(is_running(home.path(), &record));
        drop(listener);
    }

    #[test]
    fn a_request_sent_to_a_live_instance_that_never_answers_is_written() {
        // 기능 확인에는 답하고 window.create는 받기만 한 채 답하지 않는 인스턴스(이벤트 루프가 늦은 A).
        // 요청이 닿았으므로 "썼다"이고, A가 살아 있으면 상자 없이 성공으로 끝낼 근거가 된다.
        // 서버는 요청을 읽은 뒤 답 없이 연결을 닫는다. 응답 대기를 실제 시간으로 채우지 않으므로 기한은
        // 가짜 시계로 재고, 첫 시도의 소켓 대기(= 기한 2초)는 기능 확인 응답이 부하로 늦어도 넉넉하다.
        // 시도마다 실행 기록을 디스크에 동기화하므로 기한을 더 늘리면 시험 시간이 그만큼 길어진다.
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let home = tempfile::tempdir().unwrap();
        let record = publish_self(home.path(), listener.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut probe = String::new();
            reader.read_line(&mut probe).unwrap();
            assert!(probe.contains("system.info"), "{probe}");
            writer
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"capabilities\":\
                      [{\"name\":\"ipc.idempotency-key\",\"version\":99}]}}\n",
                )
                .unwrap();
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            request
        });
        let log = LaunchLog::new(Some(home.path()));
        let limit = Duration::from_secs(2);
        let clock = FakeClock::new(limit);
        let error = request_new_view_within(home.path(), &record, &log, limit, &clock).unwrap_err();
        assert!(error.written, "{error:?}");
        let request = server.join().unwrap();
        assert!(request.contains("window.create"), "{request}");
    }
}
