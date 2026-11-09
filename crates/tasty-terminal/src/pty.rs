//! OS PTY 연결. 자식 프로세스 생성, raw read/write, OS resize 적용, 종료 관측·회수를 맡는다.
//!
//! Pty는 VT 해석과 grid를 모른다. 읽은 raw byte는 [`PtyOutput`] 계약으로 넘기고,
//! 쓸 byte는 [`OutputSink`] 채널로 받는다. reader worker가 받은 byte를 바로
//! Terminal에 ingest하더라도 그 상태와 lock은 Terminal 쪽 소유다.

use crate::ResourceGeneration;
use crate::binding::ConnectionLease;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

use crate::TerminalConfig;
use crate::sink::{OutputSink, WriteProgress, lock_write_progress, new_write_progress};
use crate::spawn_error::SpawnError;

/// `Pty::drop` 누적 소요(ns) — [`pty_drop_totals`] 참조.
static PTY_DROP_NANOS: AtomicU64 = AtomicU64::new(0);
/// `Pty::drop` 누적 횟수 — [`pty_drop_totals`] 참조.
static PTY_DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// PTY Drop의 누적 시간과 횟수. 특정 종료 구간의 비용은 호출 전후 차이로 계산한다.
pub fn pty_drop_totals() -> (Duration, u64) {
    (
        Duration::from_nanos(PTY_DROP_NANOS.load(Ordering::Relaxed)),
        PTY_DROP_COUNT.load(Ordering::Relaxed),
    )
}

/// Minimum interval between child-alive `try_wait` syscalls.
pub(crate) const ALIVE_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// Initial post-EOF wake interval, doubled up to ALIVE_CHECK_INTERVAL until exit is handled.
pub(crate) const EOF_REWAKE_FIRST: Duration = Duration::from_millis(10);

/// Throttle interval for OS resize notifications.
const RESIZE_THROTTLE: Duration = Duration::from_millis(100);

/// reader worker가 읽은 raw byte를 받는 쪽의 계약.
pub(crate) trait PtyOutput: Send + 'static {
    /// 읽은 청크를 처리한다. 받는 쪽이 사라졌으면 false를 반환해 reader를 멈춘다.
    fn on_bytes(&self, data: &[u8]) -> bool;
    /// EOF/읽기 오류를 알린다. 이 호출 전에 [`PtyState::reader_eof`]가 설정된다.
    fn on_eof(&self);
    /// 종료 관측을 재촉하도록 받는 쪽을 깨운다.
    fn wake(&self);
    /// 받는 쪽이 아직 살아 있는지. EOF 뒤 재촉 반복의 종료 조건이다.
    fn is_attached(&self) -> bool;
}

/// Compatibility outcome from the child owner. Inspect PtyPhase to distinguish reap from wait failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyExit {
    pub code: Option<i32>,
    pub success: bool,
}

/// Standalone-only metadata. Adoption removes it without changing the child or generation.
#[derive(Debug)]
pub struct StandalonePty {
    pub owner_agent_id: String,
    pub cwd: Option<String>,
    pub command: Vec<String>,
    pub created_at: Instant,
    pub last_activity: Instant,
    /// Caller-chosen limit; without it Tasty never ends the running process for idleness.
    pub idle_timeout: Option<Duration>,
    /// When the host first saw the exit; starts the exited-entry retention.
    pub exited_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyPhase {
    Running,
    TerminationRequested,
    Signalled,
    Reaped,
    WaitFailed,
}

#[derive(Debug, Clone)]
pub struct PtyObservation {
    pub phase: PtyPhase,
    pub exit: Option<PtyExit>,
}

type ExitCell = Arc<Mutex<PtyObservation>>;
static EXIT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
fn observe(cell: &ExitCell) -> std::sync::MutexGuard<'_, PtyObservation> {
    tasty_utils::poison::recover_mutex(cell.lock(), "PTY child lifecycle", &EXIT_POISON_REPORTED)
}
fn publish_exit(cell: &ExitCell, status: portable_pty::ExitStatus) {
    let mut observation = observe(cell);
    observation.phase = PtyPhase::Reaped;
    observation.exit = Some(PtyExit {
        code: Some(status.exit_code() as i32),
        success: status.success(),
    });
}

fn publish_wait_failure(cell: &ExitCell) {
    let mut observation = observe(cell);
    observation.phase = PtyPhase::WaitFailed;
    // Keep the existing standalone wait wire outcome without claiming an observed/reaped exit.
    observation.exit = Some(PtyExit {
        code: None,
        success: false,
    });
}

/// Runtime state for one OS resource; it never contains VT grid/content state.
pub struct PtyState {
    pub(crate) pending_resize: Option<(usize, usize)>,
    pub(crate) last_resize_flush: Instant,
    pub(crate) last_alive_check: Instant,
    pub(crate) exit_observed: bool,
    pub(crate) reader_eof: Arc<AtomicBool>,
    pub(crate) exit_settled: Arc<AtomicBool>,
    connection: ConnectionLease,
    standalone: Option<StandalonePty>,
    exit: ExitCell,
}

impl PtyState {
    fn new() -> Self {
        Self {
            pending_resize: None,
            last_resize_flush: Instant::now(),
            last_alive_check: Instant::now() - ALIVE_CHECK_INTERVAL,
            exit_observed: false,
            reader_eof: Arc::new(AtomicBool::new(false)),
            exit_settled: Arc::new(AtomicBool::new(false)),
            connection: ConnectionLease::new(),
            standalone: None,
            exit: Arc::new(Mutex::new(PtyObservation {
                phase: PtyPhase::Running,
                exit: None,
            })),
        }
    }
    pub fn generation(&self) -> ResourceGeneration {
        self.connection.generation()
    }
    pub fn standalone(&self) -> Option<&StandalonePty> {
        self.standalone.as_ref()
    }
    pub fn exit(&self) -> Option<PtyExit> {
        observe(&self.exit).exit.clone()
    }
    pub fn observation(&self) -> PtyObservation {
        observe(&self.exit).clone()
    }
}

/// Read-only completion receipt for an exact retired owner; it cannot signal or borrow another Pty.
pub struct PtyRetirement {
    generation: ResourceGeneration,
    exit: ExitCell,
}

impl PtyRetirement {
    pub fn generation(&self) -> ResourceGeneration {
        self.generation
    }
    pub fn observation(&self) -> PtyObservation {
        observe(&self.exit).clone()
    }
}

/// spawn 직후 아직 worker를 시작하지 않은 PTY 읽기 끝.
pub(crate) struct PtyReader(Box<dyn Read + Send>);
type PreparedIo = (Box<dyn Write + Send>, Box<dyn Read + Send>);

/// OS PTY와 자식 프로세스의 소유자.
pub struct Pty {
    pub(crate) state: PtyState,
    _writer_thread: Option<thread::JoinHandle<()>>,
    writer_wake: Option<mpsc::Sender<Vec<u8>>>,
    /// Drop의 계측 구간 안에서 master를 take해 해제한다. 살아 있는 동안은 Some이다.
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    /// 일반 surface·standalone·adopt 모두 이 핸들 하나로 관측·종료·회수한다.
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    /// raw read worker. [`Pty::start_reader`] 전에는 None이다.
    _reader_thread: Option<thread::JoinHandle<()>>,
}

impl Pty {
    /// PTY를 열고 자식을 실행한 뒤 writer worker를 시작한다. 반환한 sink가 writer로 가는
    /// 유일한 입력 연결이며, 모든 sink가 사라지면 writer가 끝난다. reader는 호출자가
    /// 받는 쪽을 준비한 뒤 [`start_reader`](Self::start_reader)로 시작한다.
    pub(crate) fn spawn(
        config: &TerminalConfig<'_>,
    ) -> std::result::Result<(Self, PtyReader, OutputSink), SpawnError> {
        Self::spawn_with_setup(config, |pty| {
            let master = pty.master.as_ref().expect("spawned master");
            Ok((master.take_writer()?, master.try_clone_reader()?))
        })
    }

    fn spawn_with_setup(
        config: &TerminalConfig<'_>,
        prepare: impl FnOnce(&Pty) -> Result<PreparedIo>,
    ) -> std::result::Result<(Self, PtyReader, OutputSink), SpawnError> {
        let pair = NativePtySystem::default()
            .openpty(pty_size(config.cols, config.rows))
            .map_err(SpawnError::before_child)?;

        let shell = match config.shell {
            Some(s) if !s.is_empty() => s.to_string(),
            #[cfg(not(windows))]
            _ => default_shell(),
            // COMSPEC(cmd.exe)로 조용히 빠지지 않는다.
            #[cfg(windows)]
            _ => {
                return Err(SpawnError::before_child(anyhow::anyhow!(
                    tasty_i18n::t("terminal_error.git_bash_not_found").to_string()
                )));
            }
        };
        // Windows는 Git Bash만 실행한다. 모든 셸 실행이 여기를 지나므로 어떤 경로로 온 값도 다른 셸을 띄우지 않는다.
        #[cfg(windows)]
        if !tasty_utils::shell_family::is_git_for_windows_bash(std::path::Path::new(&shell)) {
            // 설정 판정(`ShellPathStatus`)과 같은 순서로 없는 경로를 먼저 가른다.
            let key = if std::path::Path::new(&shell).exists() {
                "terminal_error.shell_not_git_bash"
            } else {
                "terminal_error.shell_missing_windows"
            };
            return Err(SpawnError::before_child(anyhow::anyhow!(
                tasty_i18n::t_fmt(key, &shell)
            )));
        }
        // 디렉터리는 portable-pty의 실행 파일 확인(access X_OK)을 통과해 fork 뒤 exec에서 조용히 실패한다.
        // 그 전에 원인과 함께 거절한다.
        #[cfg(unix)]
        if std::path::Path::new(&shell).is_dir() {
            return Err(SpawnError::before_child(anyhow::anyhow!(
                "Unable to spawn {shell} because it is a directory, not an executable"
            )));
        }
        let cmd = build_shell_command(
            &shell,
            config.args,
            config.surface_id,
            config.working_dir,
            config.extra_env,
        );

        // An error here leaves no child; see the stage boundary in spawn_error.
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(SpawnError::before_child)?;
        drop(pair.slave);

        // From this point every fallible setup operation is protected by Pty::drop.
        tasty_reaper::adopt_pid(child.process_id());
        let mut pty = Self {
            state: PtyState::new(),
            _writer_thread: None,
            writer_wake: None,
            master: Some(pair.master),
            child: Some(child),
            _reader_thread: None,
        };
        let (mut pty_writer, pty_reader) = prepare(&pty).map_err(SpawnError::after_child)?;

        // writer 스레드가 시작되기 전에 초기 입력을 쓴다. 자식은 이미 실행 중이므로
        // 셸의 tcflush/TCSAFLUSH 등으로 입력이 사라질 수 있다.
        if let Some(input) = config.initial_input
            && !input.is_empty()
        {
            if let Err(e) = pty_writer.write_all(input.as_bytes()) {
                tracing::warn!("initial_input write_all failed: {e}");
            } else if let Err(e) = pty_writer.flush() {
                tracing::warn!("initial_input flush failed: {e}");
            }
        }

        let write_progress = new_write_progress();
        let progress_for_writer = Arc::clone(&write_progress);
        let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>();
        let connection = pty.connection();
        let writer_thread = thread::Builder::new()
            .name("pty-writer".into())
            .spawn(move || {
                run_writer_loop(pty_writer, write_rx, progress_for_writer, Some(connection));
            })
            .map_err(SpawnError::after_child)?;
        pty.writer_wake = Some(write_tx.clone());
        pty._writer_thread = Some(writer_thread);
        let sink = OutputSink::with_progress(write_tx, write_progress).bind(pty.connection());

        Ok((pty, PtyReader(pty_reader), sink))
    }

    /// raw read worker를 시작한다. 청크마다 `output.on_bytes`를 호출하고 EOF 뒤에는
    /// 종료를 관측하거나 자원이 무효화되거나 받는 쪽이 사라질 때까지 간격을 늘려 깨운다.
    pub(crate) fn start_reader(
        &mut self,
        reader: PtyReader,
        output: impl PtyOutput,
    ) -> std::io::Result<()> {
        let mut reader = reader.0;
        let eof = Arc::clone(&self.state.reader_eof);
        let settled = Arc::clone(&self.state.exit_settled);
        self._reader_thread = Some(thread::Builder::new().name("pty-reader".into()).spawn(
            move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if !output.on_bytes(&buf[..n]) {
                                return;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // PTY EOF/error: the next observation checks immediately (bypassing the throttle).
                eof.store(true, Ordering::Release);
                output.on_eof();
                // EOF can precede a waitable child exit. Keep waking with increasing intervals
                // until the exit is observed, the connection is retired, or the receiver is gone.
                let mut gap = EOF_REWAKE_FIRST;
                loop {
                    output.wake();
                    thread::sleep(gap);
                    if settled.load(Ordering::Acquire) || !output.is_attached() {
                        break;
                    }
                    gap = (gap * 2).min(ALIVE_CHECK_INTERVAL);
                }
            },
        )?);
        Ok(())
    }

    /// grid 변경 뒤 OS resize를 예약한다. 실제 적용은 flush 경로에서 한다.
    pub fn schedule_resize(
        &mut self,
        generation: ResourceGeneration,
        cols: usize,
        rows: usize,
    ) -> bool {
        if generation != self.generation() || !self.state.connection.is_active() {
            return false;
        }
        self.state.pending_resize = Some((cols, rows));
        true
    }

    pub fn has_pending_resize(&self) -> bool {
        self.state.pending_resize.is_some()
    }

    /// 예약된 OS resize를 throttle에 맞춰 적용한다. 적용했으면 true, 예약이 없거나
    /// throttle에 걸렸으면 false다(throttle이면 예약은 유지된다).
    pub fn flush_resize(&mut self) -> bool {
        if self.state.pending_resize.is_none() {
            return false;
        }
        if self.state.last_resize_flush.elapsed() < RESIZE_THROTTLE {
            return false;
        }
        self.force_flush_resize();
        true
    }

    /// throttle과 무관하게 예약된 OS resize를 적용한다.
    pub fn force_flush_resize(&mut self) {
        if !self.state.connection.is_active() {
            self.state.pending_resize = None;
            return;
        }
        if let Some((cols, rows)) = self.state.pending_resize.take() {
            if let Err(e) = self.apply_os_resize(cols, rows) {
                tracing::warn!("PTY resize failed: {e}");
            }
            self.state.last_resize_flush = Instant::now();
        }
    }

    /// OS PTY에 크기를 바로 알린다. master가 없으면 아무 일도 하지 않는다.
    pub fn apply_os_resize(&self, cols: usize, rows: usize) -> Result<()> {
        anyhow::ensure!(
            self.state.connection.is_active(),
            "PTY connection is retired"
        );
        if let Some(master) = self.master.as_ref() {
            master.resize(pty_size(cols, rows))?;
        }
        Ok(())
    }

    /// 자식 종료를 관측한다. 이번 호출에서 처음 종료를 확인했으면 true다.
    /// `try_wait`는 ALIVE_CHECK_INTERVAL마다 한 번이며 reader EOF 뒤에는 즉시 확인한다.
    pub fn observe_exit(&mut self) -> bool {
        if self.state.exit_observed {
            return false;
        }
        let reader_gone = self.state.reader_eof.load(Ordering::Acquire);
        if !reader_gone && self.state.last_alive_check.elapsed() < ALIVE_CHECK_INTERVAL {
            return false;
        }
        self.state.last_alive_check = Instant::now();
        if self.check_alive() {
            return false;
        }
        self.state.exit_observed = true;
        self.state.exit_settled.store(true, Ordering::Release);
        true
    }

    /// OS PTY에 현재 적용된 크기 `(cols, rows)`.
    #[cfg(test)]
    pub(crate) fn os_size(&self) -> Option<(usize, usize)> {
        let size = self.master.as_ref()?.get_size().ok()?;
        Some((usize::from(size.cols), usize::from(size.rows)))
    }

    /// Start ordinary nonblocking retirement and retain only its observable completion.
    pub fn retire(self) -> PtyRetirement {
        let receipt = PtyRetirement {
            generation: self.generation(),
            exit: Arc::clone(&self.state.exit),
        };
        drop(self);
        receipt
    }

    pub fn state(&self) -> &PtyState {
        &self.state
    }
    pub fn generation(&self) -> ResourceGeneration {
        self.state.generation()
    }
    pub(crate) fn connection(&self) -> ConnectionLease {
        self.state.connection.clone()
    }
    pub fn set_standalone(&mut self, metadata: StandalonePty) {
        self.state.standalone = Some(metadata);
    }
    /// Returns the standalone metadata so a failed adoption can return the owner unchanged.
    pub fn adopt(&mut self) -> Option<StandalonePty> {
        self.state.standalone.take()
    }
    pub fn standalone_mut(&mut self) -> Option<&mut StandalonePty> {
        self.state.standalone.as_mut()
    }
    pub fn touch(&mut self, now: Instant) {
        if let Some(metadata) = &mut self.state.standalone {
            metadata.last_activity = now;
        }
    }
    pub fn process_id(&self) -> Option<u32> {
        if self.state.exit().is_some() {
            return None;
        }
        self.child.as_ref()?.process_id()
    }
    pub fn foreground_process_info(
        &self,
    ) -> Option<crate::foreground_process::ForegroundProcessInfo> {
        crate::foreground_process::get_foreground_process(self.process_id()?)
    }

    pub fn is_alive(&mut self) -> bool {
        self.check_alive()
    }
    pub fn check_alive(&mut self) -> bool {
        match self.state.observation().phase {
            PtyPhase::Reaped => return false,
            PtyPhase::WaitFailed => return true,
            _ => {}
        }
        match self.child.as_mut().map(|child| child.try_wait()) {
            Some(Ok(Some(status))) => {
                publish_exit(&self.state.exit, status);
                false
            }
            Some(Err(error)) => {
                tracing::warn!("PTY child try_wait failed: {error}");
                publish_wait_failure(&self.state.exit);
                self.state.exit_settled.store(true, Ordering::Release);
                true
            }
            _ => true,
        }
    }
}

impl Drop for Pty {
    /// 정상 Drop에서는 자식 종료를 시도한다. Windows의 비정상 종료 처리는
    /// Job Object에 등록된 자식에 한해 tasty_reaper가 맡는다.
    fn drop(&mut self) {
        self.state.connection.revoke();
        self.state.exit_settled.store(true, Ordering::Release);
        self.state.pending_resize = None;
        if let Some(tx) = self.writer_wake.take()
            && tx.send(Vec::new()).is_err()
        {
            tracing::trace!("PTY writer already stopped before connection retirement");
        }
        let t_drop = Instant::now();
        if let Some(child) = self.child.take()
            && self.state.exit().is_none()
        {
            retire_child(child, Arc::clone(&self.state.exit));
        }
        // master 해제를 계측 구간 안으로 끌어들인다(위 필드 주석 참조).
        drop(self.master.take());
        PTY_DROP_NANOS.fetch_add(
            u64::try_from(t_drop.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        PTY_DROP_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Confirm the waitable handle still owns an unreaped child before any PID-based signal.
fn retire_child(mut child: Box<dyn portable_pty::Child + Send + Sync>, cell: ExitCell) {
    match child.try_wait() {
        Ok(Some(status)) => {
            publish_exit(&cell, status);
            return;
        }
        Err(error) => {
            tracing::warn!("PTY retirement cannot confirm child ownership: {error}");
            publish_wait_failure(&cell);
            return;
        }
        Ok(None) => {}
    }
    observe(&cell).phase = PtyPhase::TerminationRequested;
    // The waitable child handle is moved, never cloned or reduced to a PID.
    // Unix signals synchronously; bounded grace and final wait run off the event loop.
    #[cfg(unix)]
    let signalled = match child.process_id() {
        // SAFETY: this sole owner has not reaped its child, so this PID cannot be reused.
        Some(pid) => unsafe { libc::kill(pid as i32, libc::SIGHUP) == 0 },
        None => child.kill().is_ok(),
    };
    #[cfg(not(unix))]
    let signalled = child.kill().is_ok();
    if signalled {
        observe(&cell).phase = PtyPhase::Signalled;
    }
    thread::spawn(move || {
        if observe_grace_period(child.as_mut(), &cell) {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = child.process_id() {
            // SAFETY: the sole waitable owner just observed this child alive.
            if unsafe { libc::kill(pid as i32, libc::SIGKILL) } != 0 {
                tracing::trace!("PTY final kill failed: {}", std::io::Error::last_os_error());
            }
        }
        #[cfg(not(unix))]
        if let Err(error) = child.kill() {
            tracing::trace!("PTY final kill failed: {error}");
        }
        match child.wait() {
            Ok(status) => publish_exit(&cell, status),
            Err(error) => {
                tracing::warn!("PTY child wait failed: {error}");
                publish_wait_failure(&cell);
            }
        }
    });
}

/// True means exit or a wait failure was recorded; neither permits a later PID signal.
fn observe_grace_period(child: &mut dyn portable_pty::Child, cell: &ExitCell) -> bool {
    for _ in 0..40 {
        match child.try_wait() {
            Ok(Some(status)) => {
                publish_exit(cell, status);
                return true;
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                tracing::warn!("PTY reap check failed: {error}");
                publish_wait_failure(cell);
                return true;
            }
        }
    }
    false
}

fn pty_size(cols: usize, rows: usize) -> PtySize {
    PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// PTY writer 스레드 본체 — 큐에 들어오는 write 를 순서대로 PTY 에
/// write_all+flush 하고, 각 성공마다 `progress` 카운터를 올린다. 호스트 runtime 은 본문 write 의
/// `WriteAck` 를 제출 예약에 담고(`write_then_submit`), poll 이 이 flush 완료나 제한 시간을 확인한 뒤
/// 별도 write 로 `\r` 를 보낸다(`poll_input_submissions`).
pub(crate) fn run_writer_loop(
    mut pty_writer: Box<dyn Write + Send>,
    write_rx: mpsc::Receiver<Vec<u8>>,
    progress: WriteProgress,
    connection: Option<ConnectionLease>,
) {
    while let Ok(data) = write_rx.recv() {
        if connection.as_ref().is_some_and(|lease| !lease.is_active()) {
            break;
        }
        if pty_writer.write_all(&data).is_err() {
            break;
        }
        if pty_writer.flush().is_err() {
            break;
        }
        let (count, cvar) = &*progress;
        *lock_write_progress(count) += 1;
        cvar.notify_all();
    }
}

#[cfg(not(windows))]
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// `inherited` 중 제거 대상 키(`tasty_utils::process::is_stripped_inherited_env`)를 자식 환경에서 지운다.
fn strip_inherited_env(
    cmd: &mut CommandBuilder,
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) {
    for key in tasty_utils::process::env_keys_to_strip(inherited) {
        cmd.env_remove(&key);
    }
}

use tasty_utils::process::OUTER_IDENTITY_ENV;

/// 바깥 인스턴스의 신원 변수([`OUTER_IDENTITY_ENV`])를 자식 환경에서 지운다. 셸 설정의
/// 환경변수보다 먼저 적용해 사용자가 설정에 넣은 값은 남기고, 그 뒤 이 셸의
/// `TASTY_SURFACE_ID`·`TASTY_PARENT_HOME` 을 넣는다.
fn drop_outer_identity_env(cmd: &mut CommandBuilder) {
    for key in OUTER_IDENTITY_ENV {
        cmd.env_remove(key);
    }
}

/// 자식 셸의 인자·환경변수·작업 디렉터리를 구성한다.
fn build_shell_command(
    shell: &str,
    args: &[&str],
    surface_id: u32,
    working_dir: Option<&std::path::Path>,
    extra_env: &[(&str, &str)],
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(shell);
    // Launch as interactive login shell so .zshrc/.bashrc and themes are loaded —
    // *unless* `args` already carries an explicit `--rcfile`-based bash startup
    // (`GeneralSettings::shell_args_for` on `runtime_shell`): bash's `--rcfile` is
    // silently ignored for login shells (`bash(1)`: it only applies to
    // interactive *non-login* shells), so login mode must be dropped in that
    // case — the args themselves append `-i` to keep the shell interactive.
    #[cfg(not(windows))]
    if !args.contains(&"--rcfile") {
        cmd.arg("-li");
    }
    for arg in args {
        if !arg.is_empty() {
            cmd.arg(arg);
        }
    }
    drop_outer_identity_env(&mut cmd);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("TASTY_SURFACE_ID", surface_id.to_string());
    // 자식이 부모의 데이터 경로를 찾도록 정보용 TASTY_PARENT_HOME을 전달한다.
    // TASTY_HOME으로 주입하면 자식 Tasty의 debug/release별 경로 선택을 덮어쓰게 된다.
    if let Some(home) = tasty_utils::path::tasty_home() {
        cmd.env("TASTY_PARENT_HOME", &home);
    }

    strip_inherited_env(&mut cmd, std::env::vars_os());

    // Add tasty's own binary directory to PATH so `tasty` CLI works inside the
    // terminal. hook_runtime::trigger::spawn_shell 와 동일한 보강을 공유
    // 헬퍼로 적용해 두 경로의 동작을 일치시킨다(패키징된 macOS `.app` 의
    // 최소 PATH 에서 `tasty` self 호출 해결).
    if let Some(new_path) = tasty_utils::process::path_prepending_self_dir(std::env::var_os("PATH"))
    {
        cmd.env("PATH", new_path);
    }

    if let Some(dir) = working_dir {
        cmd.cwd(dir);
    }
    cmd
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// VT 해석 없이 raw byte와 EOF만 기록하는 받는 쪽.
    #[derive(Clone, Default)]
    struct Recorder {
        bytes: Arc<Mutex<Vec<u8>>>,
        eof: Arc<AtomicBool>,
        wakes: Arc<AtomicU64>,
        detached: Arc<AtomicBool>,
    }

    impl PtyOutput for Recorder {
        fn on_bytes(&self, data: &[u8]) -> bool {
            self.bytes.lock().expect("recorder").extend_from_slice(data);
            true
        }
        fn on_eof(&self) {
            self.eof.store(true, Ordering::Release);
        }
        fn wake(&self) {
            self.wakes.fetch_add(1, Ordering::Relaxed);
        }
        fn is_attached(&self) -> bool {
            !self.detached.load(Ordering::Acquire)
        }
    }

    fn spawn_sh(args: &[&str]) -> (Pty, PtyReader, OutputSink) {
        Pty::spawn(&TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args,
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        })
        .expect("pty spawn")
    }

    /// 활성 플러그인이 선언한 세션 키와 CMUX_* 는 지우고 TASTY_*·일반 키는 남긴다.
    #[test]
    fn claude_session_and_cmux_env_are_stripped_but_tasty_env_is_kept() {
        use tasty_test_support::strip_env_keys::{
            KEPT, STRIPPED, inherited, install_bundled_plugin_rules,
        };
        install_bundled_plugin_rules();
        let mut cmd = CommandBuilder::new("/bin/sh");
        for (key, value) in inherited() {
            cmd.env(key, value);
        }
        strip_inherited_env(&mut cmd, inherited());
        for key in STRIPPED {
            assert_eq!(cmd.get_env(key), None, "{key} 는 지워져야 한다");
        }
        for key in KEPT {
            assert!(cmd.get_env(key).is_some(), "{key} 는 남아야 한다");
        }
    }

    /// 바깥 인스턴스의 신원 변수는 지우고 이 셸의 surface 를 넣는다. 셸 설정에 넣은 값은 남긴다.
    #[test]
    fn outer_session_token_and_agent_id_are_dropped_but_settings_values_win() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        for key in OUTER_IDENTITY_ENV {
            cmd.env(key, "outer");
        }
        drop_outer_identity_env(&mut cmd);
        for key in OUTER_IDENTITY_ENV {
            assert_eq!(cmd.get_env(key), None, "{key} 는 지워져야 한다");
        }

        let cmd = build_shell_command(
            "/bin/sh",
            &[],
            7,
            None,
            &[("TASTY_AGENT_ID", "from-settings")],
        );
        assert_eq!(
            cmd.get_env("TASTY_AGENT_ID"),
            Some(std::ffi::OsStr::new("from-settings"))
        );
        assert_eq!(cmd.get_env("TASTY_SESSION_TOKEN"), None);
        assert_eq!(
            cmd.get_env("TASTY_SURFACE_ID"),
            Some(std::ffi::OsStr::new("7"))
        );
    }

    fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !cond() {
            assert!(
                Instant::now() < deadline,
                "{what}: 10초 안에 일어나지 않았다"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Terminal 없이도 raw 출력·EOF·종료 관측이 계약대로 전달된다.
    #[test]
    fn raw_output_eof_and_exit_reach_a_non_terminal_receiver() {
        let (mut pty, reader, _sink) = spawn_sh(&["-c", "printf tasty-pty-raw"]);
        let rec = Recorder::default();
        pty.start_reader(reader, rec.clone()).expect("reader start");

        wait_until("EOF", || rec.eof.load(Ordering::Acquire));
        assert!(pty.state.reader_eof.load(Ordering::Acquire));
        let out = String::from_utf8_lossy(&rec.bytes.lock().expect("recorder")).into_owned();
        assert!(out.contains("tasty-pty-raw"), "raw 출력: {out:?}");

        wait_until("exit", || pty.observe_exit());
        assert!(pty.state.exit_settled.load(Ordering::Acquire));
        assert!(!pty.observe_exit(), "종료 관측은 한 번만 보고한다");
    }

    /// sink로 보낸 byte가 자식의 stdin에 도달하고 writer 완료 카운터가 오른다.
    #[test]
    fn sink_bytes_reach_the_child_and_advance_progress() {
        let (mut pty, reader, sink) = spawn_sh(&["-c", "read line; printf \"got:%s\" \"$line\""]);
        let rec = Recorder::default();
        pty.start_reader(reader, rec.clone()).expect("reader start");

        sink.send(b"ping\r".to_vec()).expect("writer alive");
        wait_until("writer flush", || {
            *lock_write_progress(&sink.progress().0) >= 1
        });
        wait_until("echo", || {
            String::from_utf8_lossy(&rec.bytes.lock().expect("recorder")).contains("got:ping")
        });
    }

    /// Standalone and surface-owned children retain the same kill/wait owner.
    #[test]
    fn dropping_the_pty_signals_and_reaps_its_owned_child() {
        let (mut pty, reader, _sink) = spawn_sh(&["-c", "exec sleep 60"]);
        pty.start_reader(reader, Recorder::default())
            .expect("reader start");
        let completion = Arc::clone(&pty.state.exit);
        assert_eq!(observe(&completion).phase, PtyPhase::Running);
        drop(pty);
        wait_until("owned child reaped", || {
            observe(&completion).phase == PtyPhase::Reaped
        });
        assert!(observe(&completion).exit.is_some());
    }

    /// SIGHUP 을 무시하는 자식은 유예 뒤 SIGKILL 로 끝내고 회수한다. 마스터를 닫을 때 커널이 보내는
    /// SIGHUP 도 무시하므로, 이 경로가 없으면 자식은 `sleep 60` 을 마치고 스스로 정상 종료한다.
    /// 그래서 짧은 기한 없이 종료 결과로 판정하고, 회수 결과가 기록되지 않는 결함만 120초 상한으로 잡는다.
    #[test]
    fn retiring_a_child_that_ignores_hangup_kills_and_reaps_it() {
        // 무시한 신호는 exec 뒤에도 무시된 채 남는다. 표지를 본 뒤에 retire 해야 trap 이 걸린 상태다.
        let (mut pty, reader, _sink) =
            spawn_sh(&["-c", "trap '' HUP; printf hup-ignored; exec sleep 60"]);
        let rec = Recorder::default();
        pty.start_reader(reader, rec.clone()).expect("reader start");
        wait_until("hangup trap installed", || {
            String::from_utf8_lossy(&rec.bytes.lock().expect("recorder")).contains("hup-ignored")
        });
        let retired = pty.retire();
        // 상한은 자식의 `sleep 60` 보다 길다. 최종 kill 이 빠지면 자식이 먼저 끝나 아래 단언이 잡고,
        // 회수 결과가 기록되지 않으면 이 상한이 잡는다. 유예(200ms)와 거리가 멀어 부하로 넘지 않는다.
        let limit = Instant::now() + Duration::from_secs(120);
        while !matches!(
            retired.observation().phase,
            PtyPhase::Reaped | PtyPhase::WaitFailed
        ) {
            assert!(
                Instant::now() < limit,
                "retire did not record an exit within 120s"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let observed = retired.observation();
        assert_eq!(observed.phase, PtyPhase::Reaped);
        let exit = observed.exit.expect("a reaped child has an exit");
        assert!(
            !exit.success,
            "the child must end by the final kill, not by finishing its sleep: {exit:?}"
        );
    }

    /// Fail after the real child exists, at both reader/writer preparation boundaries.
    #[test]
    fn setup_failures_after_spawn_are_reaped_by_the_early_owner() {
        for writer_was_acquired in [false, true] {
            let mut completion = None;
            let mut child_pid = None;
            let result = Pty::spawn_with_setup(
                &TerminalConfig {
                    cols: 80,
                    rows: 24,
                    shell: Some("/bin/sh"),
                    args: &["-c", "exec sleep 60"],
                    surface_id: 0,
                    working_dir: None,
                    initial_input: None,
                    extra_env: &[],
                },
                |pty| {
                    child_pid = pty.process_id();
                    completion = Some(Arc::clone(&pty.state.exit));
                    if writer_was_acquired {
                        let writer = pty.master.as_ref().unwrap().take_writer()?;
                        drop(writer);
                    }
                    anyhow::bail!("injected I/O handle preparation failure");
                },
            );
            let error = result.err().expect("injected setup failure");
            assert_eq!(error.stage(), crate::SpawnStage::AfterChild);
            assert!(
                child_pid.is_some(),
                "the injected error must follow an actual spawn"
            );
            let completion = completion.expect("owner existed before fallible setup");
            wait_until("failed setup child reaped", || {
                observe(&completion).phase == PtyPhase::Reaped
            });
        }
    }

    fn spawn_shell(shell: &str) -> Result<(Pty, PtyReader, OutputSink), SpawnError> {
        Pty::spawn(&TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some(shell),
            args: &[],
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        })
    }

    /// The executable check fails before fork.
    #[test]
    fn a_missing_shell_fails_before_any_child() {
        let error = spawn_shell("/nonexistent/tasty-test-shell")
            .err()
            .expect("a missing shell cannot start");
        assert_eq!(error.stage(), crate::SpawnStage::BeforeChild);
        assert!(error.to_string().contains("tasty-test-shell"), "{error}");
    }

    /// A directory would pass the executable check and fail exec silently after fork, so it is
    /// refused before any child with the cause in the message.
    #[test]
    fn a_directory_shell_fails_before_any_child_with_the_cause() {
        let error = spawn_shell("/").err().expect("a directory cannot start");
        assert_eq!(error.stage(), crate::SpawnStage::BeforeChild);
        assert!(error.to_string().contains("is a directory"), "{error}");
    }

    /// An exec failure after fork is not reported: portable-pty's pre_exec closes every descriptor
    /// above stderr, including the pipe std uses to report an exec error, so the spawn succeeds and
    /// the child exits on its own. An executable script whose interpreter does not exist fails
    /// exactly there.
    #[test]
    fn an_exec_failure_after_fork_is_a_started_child_that_exits() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let script = dir.path().join("broken-shell");
        std::fs::write(&script, "#!/nonexistent/tasty-interpreter\n").expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");
        let spawned = spawn_shell(&script.to_string_lossy());
        let (mut pty, _reader, _sink) = spawned.expect("the exec error is not reported");
        wait_until("child that failed exec exits", || !pty.check_alive());
        let exit = pty.state.exit().expect("observed exit");
        assert!(!exit.success, "{exit:?}");
    }

    #[test]
    fn old_resize_generation_cannot_resize_the_new_physical_pty() {
        let (mut old, _reader, _sink) = spawn_sh(&["-c", "exec sleep 60"]);
        let old_generation = old.generation();
        assert!(old.schedule_resize(old_generation, 99, 31));
        old.state.connection.revoke();
        old.force_flush_resize();
        assert!(!old.has_pending_resize());
        assert_eq!(old.os_size(), Some((80, 24)));
        let (mut replacement, _reader, _sink) = spawn_sh(&["-c", "exec sleep 60"]);
        assert_ne!(old_generation, replacement.generation());
        assert!(!replacement.schedule_resize(old_generation, 99, 31));
        replacement.force_flush_resize();
        assert_eq!(replacement.os_size(), Some((80, 24)));
    }

    #[derive(Debug)]
    struct LostChild;
    impl portable_pty::ChildKiller for LostChild {
        fn kill(&mut self) -> std::io::Result<()> {
            panic!("unknown ownership must never kill")
        }
        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            panic!("must not clone a killer")
        }
    }
    impl portable_pty::Child for LostChild {
        fn try_wait(&mut self) -> std::io::Result<Option<portable_pty::ExitStatus>> {
            Err(std::io::Error::from_raw_os_error(libc::ECHILD))
        }
        fn wait(&mut self) -> std::io::Result<portable_pty::ExitStatus> {
            panic!("unknown ownership must not enter a new wait")
        }
        fn process_id(&self) -> Option<u32> {
            panic!("PID must not be consulted after ECHILD")
        }
    }

    #[test]
    fn retirement_wait_failure_is_reported_without_targeting_a_pid() {
        let state = PtyState::new();
        let observed = Arc::clone(&state.exit);
        retire_child(Box::new(LostChild), Arc::clone(&observed));
        assert_eq!(observe(&observed).phase, PtyPhase::WaitFailed);
        assert_eq!(
            state.exit(),
            Some(PtyExit {
                code: None,
                success: false
            }),
            "legacy wait failure response must terminate polling without claiming reap"
        );
    }
}

#[cfg(all(test, windows))]
mod windows_shell_tests {
    use crate::{SpawnError, TerminalConfig, spawn_terminal};
    use std::sync::Arc;

    fn spawn(shell: Option<&str>) -> Result<(crate::Terminal, crate::Pty), SpawnError> {
        spawn_terminal(
            TerminalConfig {
                cols: 80,
                rows: 24,
                shell,
                args: &[],
                surface_id: 0,
                working_dir: None,
                initial_input: None,
                extra_env: &[],
            },
            Arc::new(|| {}),
        )
    }

    /// 허용하지 않는 셸과 빈 값은 COMSPEC 없이 자식을 만들기 전에 원인과 함께 거절한다.
    #[test]
    fn only_git_bash_reaches_the_spawn() {
        let comspec = std::env::var("COMSPEC").expect("COMSPEC");
        let no_shell = tasty_i18n::t("terminal_error.git_bash_not_found").to_string();
        let cases = [
            (
                Some(comspec.as_str()),
                tasty_i18n::t_fmt("terminal_error.shell_not_git_bash", &comspec),
            ),
            (
                Some("cmd.exe"),
                tasty_i18n::t_fmt("terminal_error.shell_missing_windows", "cmd.exe"),
            ),
            (Some(""), no_shell.clone()),
            (None, no_shell),
        ];
        for (shell, expected) in cases {
            let Err(error) = spawn(shell) else {
                panic!("{shell:?} must not start");
            };
            assert!(error.is_before_child(), "{shell:?}: {error}");
            assert_eq!(error.to_string(), expected, "{shell:?}");
        }
    }
}
