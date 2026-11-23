//! Windows 자식을 KILL_ON_JOB_CLOSE Job Object에 등록한다.
//! 등록된 프로세스는 호스트의 마지막 job 핸들이 닫힐 때 OS가 종료한다.
//! 생성·등록 실패는 경고하고 호스트 실행을 계속하므로 모든 자식의 정리를 보장하지는 않는다.
//! 비 Windows에서는 아무 작업도 하지 않는다. 해당 플랫폼의 PTY·자식 종료 처리는 호출자 책임이다.
//! 예외로 [`spawn_bound_to_host`]는 Linux에서 PDEATHSIG로 자식을 호스트 수명에 묶는다.
//! [`terminate_process_group`]는 그룹 리더로 띄운 자식과 그 그룹을 정상 종료 요청부터 끝낸다.
//! [`defer_termination_signals`]는 소유한 자식을 정리하는 동안 터미널 신호가 정리를 끊지 않게 한다.

use std::sync::OnceLock;

pub use imp::JobObject;

/// 프로세스 전역 호스트 job. [`init_host_reaper`] 로 부팅 시 1회 채운다.
///
/// `OnceLock<Option<JobObject>>` — 바깥 `None` = 미초기화(테스트/CLI client),
/// 안쪽 `None` = job 생성 실패로 결박 비활성. 어느 경우든 [`adopt_pid`] 는 no-op.
static HOST_JOB: OnceLock<Option<JobObject>> = OnceLock::new();

/// 호스트 프로세스(터미널을 spawn 하는 쪽)에서 **1회** 호출한다. plugin/terminal
/// spawn 보다 먼저 부팅 초기에 호출해야 한다. 중복 호출은 무시된다.
///
/// job 생성에 실패해도 앱은 계속 진행하며 결박만 비활성화된다.
pub fn init_host_reaper() {
    let job = match JobObject::new() {
        Ok(job) => Some(job),
        Err(e) => {
            tracing::warn!(
                "host reaper init failed; child shells may orphan on abnormal exit: {e}"
            );
            None
        }
    };
    if HOST_JOB.set(job).is_err() {
        tracing::warn!("init_host_reaper called more than once; ignoring");
    }
}

/// 자식 프로세스(PID)를 전역 호스트 job 에 결박한다. best-effort — 미초기화이거나
/// job 비활성이거나 `pid` 가 `None` 이면 조용히 no-op. 비-Windows 에서도 no-op.
pub fn adopt_pid(pid: Option<u32>) {
    let Some(pid) = pid else {
        return;
    };
    let Some(Some(job)) = HOST_JOB.get() else {
        return;
    };
    if let Err(e) = job.assign_pid(pid) {
        tracing::warn!("reaper adopt pid {pid} failed: {e}");
    }
}

/// 호스트가 비정상 종료해도 남지 않아야 하는 자식(SSH 터널 등)을 실행한다.
///
/// Linux는 부모가 죽으면 자식이 SIGTERM을 받도록 PDEATHSIG를 설정한다. PDEATHSIG는 fork한
/// 스레드의 종료에 반응하므로 짧게 사는 워커 대신 프로세스 수명 동안 남는 spawner 스레드에서
/// 실행한다. Windows는 [`init_host_reaper`]로 만든 호스트 job에 등록한다(초기화하지 않은
/// CLI 등에서는 등록하지 않는다). 다른 OS는 결속하지 않고 그대로 실행한다.
pub fn spawn_bound_to_host(cmd: std::process::Command) -> std::io::Result<std::process::Child> {
    bound::spawn(cmd)
}

/// 자기 프로세스 그룹의 리더로 띄운 자식과 그 그룹 전체를 끝내고 자식을 회수한다.
///
/// Unix는 그룹에 SIGTERM을 보내고 `grace` 안에 자식이 끝나지 않으면 SIGKILL을 보낸다. 자식이
/// 먼저 끝나도 그룹에 남은 프로세스는 SIGKILL로 정리한다. npm 실행 스크립트처럼 실제 프로그램을
/// 다시 띄우는 래퍼는 신호를 한 번만 전달하므로 래퍼에만 신호를 보내면 실제 프로그램이 고아로
/// 남을 수 있기 때문이다. 호출자는 `CommandExt::process_group(0)`으로 자식을 띄워야 한다. 그룹이
/// 하나라도 남아 있는 동안 그 그룹 ID는 다른 프로세스에 재사용되지 않는다. 그래서 자식이 이미
/// 끝났어도 회수하지 않은 채(좀비가 PID와 그룹 ID를 붙잡는다) 그룹 신호를 보낸 뒤에 회수한다.
/// 호출자가 이 함수 전에 자식을 회수했다면 남은 그룹 프로세스가 그 ID를 붙잡고 있어야 한다.
/// Windows는 신호가 없어 자식만 바로 끝낸다. 하위 프로세스까지 끝내려면 [`JobObject`]에 등록한다.
pub fn terminate_process_group(
    child: &mut std::process::Child,
    grace: std::time::Duration,
) -> std::io::Result<std::process::ExitStatus> {
    #[cfg(unix)]
    {
        let group = libc::pid_t::try_from(child.id()).map_err(std::io::Error::other)?;
        if !exited_unreaped(child)? {
            signal_group(group, libc::SIGTERM);
            let deadline = std::time::Instant::now() + grace;
            while std::time::Instant::now() < deadline && !exited_unreaped(child)? {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        signal_group(group, libc::SIGKILL);
        child.wait()
    }
    #[cfg(not(unix))]
    {
        // 이유: 신호가 없는 OS는 기다리지 않고 바로 끝내므로 기한을 쓰지 않는다.
        let _ = grace;
        match child.kill() {
            Ok(()) => {}
            // 그 사이에 끝났으면 회수만 한다.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(e) => return Err(e),
        }
        child.wait()
    }
}

/// [`defer_termination_signals`] 뒤에 받은 신호. Unix가 아니면 항상 비어 있다.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeferredSignals {
    /// SIGINT(터미널의 Ctrl+C).
    pub interrupt: bool,
    /// SIGHUP(터미널 닫힘).
    pub hangup: bool,
    /// SIGTERM.
    pub terminate: bool,
}

impl DeferredSignals {
    pub fn any(self) -> bool {
        self.interrupt || self.hangup || self.terminate
    }

    /// 종료 요청(hangup·terminate)이 있으면 같은 신호를 아직 회수하지 않은 `child`에 한 번
    /// 전달하고 true를 돌려준다. 회수하지 않은 자식의 PID는 다른 프로세스에 재사용되지 않는다.
    /// 신호가 없는 OS는 자식을 바로 끝낸다.
    pub fn forward_termination(self, child: &mut std::process::Child) -> std::io::Result<bool> {
        if !(self.hangup || self.terminate) || child.try_wait()?.is_some() {
            return Ok(false);
        }
        #[cfg(unix)]
        {
            let pid = libc::pid_t::try_from(child.id()).map_err(std::io::Error::other)?;
            let signal = if self.hangup {
                libc::SIGHUP
            } else {
                libc::SIGTERM
            };
            // SAFETY: pid는 회수하지 않은 직접 자식이며 정수 인자만 넘긴다.
            if unsafe { libc::kill(pid, signal) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        }
        #[cfg(not(unix))]
        child.kill()?;
        Ok(true)
    }
}

/// SIGINT·SIGHUP·SIGTERM이 이 프로세스를 끝내지 않고 기록만 하게 한다. 기록은
/// [`take_deferred_signals`]로 읽는다.
///
/// 터미널 안에서 소유한 자식(서버 그룹 등)을 띄우는 프로세스용이다. 자식 TUI가 끝나 터미널이 일반
/// 모드로 돌아온 뒤 도착한 Ctrl+C나 hangup이 정리 도중 이 프로세스를 끝내면 그룹 정리를 건너뛴다.
/// 처리기를 둔 신호는 exec 때 기본 처리로 돌아가므로 뒤에 띄우는 자식의 신호 처리는 바뀌지 않는다.
/// 이미 무시(SIG_IGN)하도록 받은 신호는 그대로 둔다(백그라운드 작업 등). Unix가 아니면 아무 일도
/// 하지 않는다.
pub fn defer_termination_signals() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        deferred::install()
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// 마지막 호출 뒤 받은 신호를 돌려주고 기록을 비운다.
pub fn take_deferred_signals() -> DeferredSignals {
    #[cfg(unix)]
    {
        deferred::take()
    }
    #[cfg(not(unix))]
    {
        DeferredSignals::default()
    }
}

#[cfg(unix)]
mod deferred {
    use std::sync::atomic::{AtomicU32, Ordering};

    const INTERRUPT: u32 = 1;
    const HANGUP: u32 = 2;
    const TERMINATE: u32 = 4;

    static PENDING: AtomicU32 = AtomicU32::new(0);

    fn bit(signal: libc::c_int) -> u32 {
        match signal {
            libc::SIGINT => INTERRUPT,
            libc::SIGHUP => HANGUP,
            libc::SIGTERM => TERMINATE,
            _ => 0,
        }
    }

    // 신호 처리기 안에서는 lock-free 원자 연산만 한다.
    extern "C" fn record(signal: libc::c_int) {
        PENDING.fetch_or(bit(signal), Ordering::SeqCst);
    }

    pub(super) fn install() -> std::io::Result<()> {
        for signal in [libc::SIGINT, libc::SIGHUP, libc::SIGTERM] {
            // SAFETY: sigaction은 POD이며 all-zero가 유효한 초기 상태다.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = record as extern "C" fn(libc::c_int) as libc::sighandler_t;
            // 느린 시스템 호출이 EINTR로 끝나지 않게 한다.
            action.sa_flags = libc::SA_RESTART;
            // SAFETY: 유효한 sigaction 구조체의 마스크를 비운다.
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            // SAFETY: 이전 값을 받을 구조체도 all-zero가 유효하다.
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            // SAFETY: 두 포인터 모두 이 함수의 유효한 지역 변수를 가리킨다.
            if unsafe { libc::sigaction(signal, &action, &mut previous) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if previous.sa_sigaction == libc::SIG_IGN {
                // SAFETY: 위에서 읽은 이전 처리를 그대로 되돌린다.
                if unsafe { libc::sigaction(signal, &previous, std::ptr::null_mut()) } != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(())
    }

    pub(super) fn take() -> super::DeferredSignals {
        let pending = PENDING.swap(0, Ordering::SeqCst);
        super::DeferredSignals {
            interrupt: pending & INTERRUPT != 0,
            hangup: pending & HANGUP != 0,
            terminate: pending & TERMINATE != 0,
        }
    }
}

/// 자식이 끝났는지 회수하지 않고 확인한다(WNOWAIT). 이미 회수된 자식(ECHILD)은 끝난 것으로 본다.
#[cfg(unix)]
fn exited_unreaped(child: &std::process::Child) -> std::io::Result<bool> {
    let pid = libc::id_t::try_from(child.id()).map_err(std::io::Error::other)?;
    loop {
        // SAFETY: siginfo_t는 POD이며 all-zero가 유효하다. 상태 변화가 없으면 si_signo가 0으로 남는다.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: info는 유효한 지역 변수이고 나머지는 정수 인자다.
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if rc == 0 {
            return Ok(info.si_signo != 0);
        }
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EINTR) => continue,
            Some(libc::ECHILD) => return Ok(true),
            _ => return Err(error),
        }
    }
}

/// 남은 프로세스가 없는 그룹(ESRCH)은 정상으로 본다.
#[cfg(unix)]
fn signal_group(group: libc::pid_t, signal: libc::c_int) {
    // SAFETY: group은 호출자가 그룹 리더로 띄운 직접 자식의 ID이며, 정수 인자만 넘긴다.
    if unsafe { libc::kill(-group, signal) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!(group, signal, %error, "could not signal the process group");
        }
    }
}

#[cfg(target_os = "linux")]
mod bound {
    use std::io;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::OnceLock;
    use std::sync::mpsc::{self, Sender, SyncSender};

    type SpawnJob = (Command, SyncSender<io::Result<Child>>);

    /// 스레드를 만들지 못하면 None이며, 호출자 스레드에서 실행한다.
    fn spawner() -> Option<&'static Sender<SpawnJob>> {
        static TX: OnceLock<Option<Sender<SpawnJob>>> = OnceLock::new();
        TX.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<SpawnJob>();
            let spawned = std::thread::Builder::new()
                .name("bound-spawner".into())
                .spawn(move || {
                    while let Ok((mut cmd, reply)) = rx.recv() {
                        deliver(&reply, cmd.spawn());
                    }
                });
            match spawned {
                Ok(_) => Some(tx),
                Err(e) => {
                    tracing::warn!(
                        "bound-spawner thread creation failed; PDEATHSIG follows the caller thread: {e}"
                    );
                    None
                }
            }
        })
        .as_ref()
    }

    // 요청자가 떠나 전달하지 못한 자식은 Child drop만으로 회수되지 않으므로 직접 끝낸다.
    fn deliver(reply: &SyncSender<io::Result<Child>>, result: io::Result<Child>) {
        let Err(mpsc::SendError(Ok(mut child))) = reply.send(result) else {
            return;
        };
        let pid = child.id();
        if let Err(error) = child.kill() {
            tracing::warn!(pid, %error, "could not stop an unclaimed bound child");
        }
        if let Err(error) = child.wait() {
            tracing::warn!(pid, %error, "could not reap an unclaimed bound child");
        }
    }

    pub(super) fn spawn(mut cmd: Command) -> io::Result<Child> {
        let parent = std::process::id();
        // SAFETY: pre_exec 클로저는 fork 뒤 exec 전 자식에서 실행된다. 아래 함수는
        // async-signal-safe인 prctl·getppid·_exit만 호출하고 할당하지 않는다.
        unsafe {
            cmd.pre_exec(move || exit_with_parent(parent));
        }
        let Some(tx) = spawner() else {
            return cmd.spawn();
        };
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        if tx.send((cmd, reply_tx)).is_err() {
            return Err(io::Error::other("bound-spawner thread is gone"));
        }
        reply_rx
            .recv()
            .map_err(|_| io::Error::other("bound-spawner thread dropped its reply"))?
    }

    fn exit_with_parent(parent: u32) -> io::Result<()> {
        // SAFETY: fork 뒤 자식에서 인자만 정수인 prctl을 호출한다.
        let set = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) };
        if set != 0 {
            return Err(io::Error::last_os_error());
        }
        // 설정 전에 부모가 이미 끝났으면 신호가 오지 않는다. 부모가 subreaper(systemd --user 등)로
        // 바뀌므로 1과 비교하지 않고 fork 전 PID와 비교한다.
        // SAFETY: getppid는 실패하지 않는 async-signal-safe 호출이다.
        let now = unsafe { libc::getppid() };
        if u32::try_from(now).ok() != Some(parent) {
            // SAFETY: exec 전 자식을 즉시 끝낸다. 부모의 atexit·버퍼를 실행하지 않는다.
            unsafe { libc::_exit(0) };
        }
        Ok(())
    }
}

#[cfg(windows)]
mod bound {
    pub(super) fn spawn(mut cmd: std::process::Command) -> std::io::Result<std::process::Child> {
        let child = cmd.spawn()?;
        super::adopt_pid(Some(child.id()));
        Ok(child)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod bound {
    pub(super) fn spawn(mut cmd: std::process::Command) -> std::io::Result<std::process::Child> {
        cmd.spawn()
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::ptr;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// Windows Job Object 기반 결박 primitive. `job` 은 `OwnedHandle` 로 보관되어,
    /// 본 구조체가 drop 되면 `CloseHandle` 되고 `KILL_ON_JOB_CLOSE` 로 job 내 전
    /// 프로세스가 종료된다.
    pub struct JobObject {
        job: OwnedHandle,
    }

    impl JobObject {
        /// Job 을 생성하고 `KILL_ON_JOB_CLOSE` 를 설정한다.
        pub fn new() -> io::Result<Self> {
            let Self { job } = Self::new_detached()?;

            // SAFETY: JOBOBJECT_EXTENDED_LIMIT_INFORMATION 은 POD 이며 all-zero 가
            // 유효한 초기 상태다.
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            // SAFETY: job 핸들은 유효하고, info 는 정보 클래스에 맞는 크기의 유효 포인터다.
            let ok = unsafe {
                SetInformationJobObject(
                    job.as_raw_handle() as HANDLE,
                    JobObjectExtendedLimitInformation,
                    ptr::addr_of!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { job })
        }

        /// 닫혀도 안의 프로세스를 끝내지 않는 job 을 만든다. 호스트보다 오래 살아야 하는 자식을
        /// 묶어 두었다가 [`Self::terminate`] 로 한꺼번에 끝낼 때 쓴다.
        pub fn new_detached() -> io::Result<Self> {
            // SAFETY: lpJobAttributes/lpName 둘 다 NULL → 기본 보안 속성의 익명 job.
            let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: raw 는 CreateJobObjectW 가 막 반환한 유효·단독 소유 핸들이며,
            // OwnedHandle 이 소유권을 가져가 drop 시 CloseHandle 한다.
            let job = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
            Ok(Self { job })
        }

        /// job 의 모든 프로세스를 끝낸다.
        pub fn terminate(&self) -> io::Result<()> {
            // SAFETY: job 핸들은 자기 소유의 유효한 핸들이다.
            let ok = unsafe { TerminateJobObject(self.job.as_raw_handle() as HANDLE, 1) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        /// 이미 열린 프로세스 핸들을 job 에 assign 한다.
        ///
        /// # Safety
        /// `process_handle` 은 `PROCESS_SET_QUOTA | PROCESS_TERMINATE` 권한을 가진
        /// 유효한 열린 프로세스 핸들이어야 한다(호출자 보장). 무효 핸들이면
        /// `AssignProcessToJobObject` 가 실패를 반환할 뿐이지만 계약상 유효를 요구한다.
        pub unsafe fn assign_handle(&self, process_handle: HANDLE) -> io::Result<()> {
            // SAFETY: job 핸들은 유효(자기 소유), process_handle 은 호출자 보장 유효.
            let ok = unsafe {
                AssignProcessToJobObject(self.job.as_raw_handle() as HANDLE, process_handle)
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        /// PID 로 프로세스를 열어 job 에 assign 한다. portable-pty 처럼 `std::process::Child`
        /// 핸들을 직접 못 얻는 자식(터미널 셸)용 경로.
        pub fn assign_pid(&self, pid: u32) -> io::Result<()> {
            // SAFETY: OpenProcess 는 실패 시 NULL 을 반환하며 아래에서 검사한다.
            let raw = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: raw 는 OpenProcess 가 막 반환한 유효·단독 소유 핸들. OwnedHandle 이
            // 소유권을 가져가 이 함수 종료 시 CloseHandle 한다(assign 은 job 이 자체 참조를
            // 잡으므로 이후 핸들을 닫아도 무방).
            let owned = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
            // SAFETY: owned 는 방금 OpenProcess 로 PROCESS_SET_QUOTA | PROCESS_TERMINATE
            // 권한으로 연 유효 핸들이므로 assign_handle 의 안전 계약을 충족한다.
            unsafe { self.assign_handle(owned.as_raw_handle() as HANDLE) }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;

    /// 비 Windows에서는 OS 결속을 수행하지 않는다. 호출 인터페이스만 동일하게 제공한다.
    pub struct JobObject;

    impl JobObject {
        pub fn new() -> io::Result<Self> {
            Ok(Self)
        }

        pub fn new_detached() -> io::Result<Self> {
            Ok(Self)
        }

        pub fn assign_pid(&self, _pid: u32) -> io::Result<()> {
            Ok(())
        }

        pub fn terminate(&self) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_object_new_succeeds() {
        // 테스트 러너를 job에 등록하지 않는다. 여기서는 빈 job의 생성·해제만 확인한다.
        let job = JobObject::new();
        assert!(job.is_ok(), "JobObject::new failed: {:?}", job.err());
    }

    #[test]
    fn adopt_pid_none_is_noop() {
        adopt_pid(None);
    }

    #[cfg(unix)]
    fn spawn_group_leader(script: &str) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh")
            .args(["-c", script])
            .process_group(0)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.take().unwrap()),
            &mut line,
        )
        .unwrap();
        assert_eq!(line, "ready\n");
        child
    }

    /// 정상 종료 요청을 받은 리더가 스스로 정리하고 끝날 시간을 준다.
    #[cfg(unix)]
    #[test]
    fn a_group_leader_exits_on_sigterm_within_the_grace() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("cleaned");
        let mut child = spawn_group_leader(&format!(
            "trap 'echo done > \"{}\"; exit 0' TERM; echo ready; while :; do sleep 0.05; done",
            marker.display()
        ));
        let status =
            terminate_process_group(&mut child, std::time::Duration::from_secs(5)).unwrap();
        assert!(status.success(), "{status:?}");
        assert!(marker.exists(), "SIGTERM 처리기가 실행돼야 한다");
    }

    /// 신호를 한 번만 전달하는 래퍼와 정상 종료가 느린 실제 프로그램을 흉내 낸다. 기한 뒤
    /// 그룹 전체를 끝내므로 실제 프로그램 역할의 손자도 남지 않는다.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_slow_grandchild_does_not_outlive_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("inner.pid");
        let inner_script = dir.path().join("inner.sh");
        // 손자는 신호 처리를 바꾼 뒤에 PID를 쓴다. 리더는 그 파일을 본 뒤 준비됐다고 알린다.
        std::fs::write(
            &inner_script,
            "trap '' TERM\necho $$ > \"$1\"\nwhile :; do sleep 0.05; done\n",
        )
        .unwrap();
        let mut child = spawn_group_leader(&format!(
            "trap '' TERM; sh \"{script}\" \"{pid}\" & \
             while [ ! -s \"{pid}\" ]; do sleep 0.01; done; echo ready; wait",
            script = inner_script.display(),
            pid = pid_file.display()
        ));
        let inner = std::fs::read_to_string(&pid_file).unwrap();
        let proc_dir = std::path::PathBuf::from(format!("/proc/{}", inner.trim()));
        assert!(proc_dir.exists());
        let started = std::time::Instant::now();
        let status =
            terminate_process_group(&mut child, std::time::Duration::from_millis(300)).unwrap();
        assert!(!status.success(), "{status:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while proc_dir.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!proc_dir.exists(), "그룹의 손자 {} 가 남았다", inner.trim());
        // 이미 끝난 자식은 회수 결과만 돌려준다.
        assert_eq!(
            terminate_process_group(&mut child, std::time::Duration::ZERO).unwrap(),
            status
        );
    }

    /// 기록하도록 바꾼 신호는 프로세스를 끝내지 않고 한 번만 읽힌다. 이 시험 프로세스의 처리기를
    /// 바꾸므로 이 신호들을 쓰는 다른 시험을 이 크레이트에 두지 않는다.
    #[cfg(unix)]
    #[test]
    fn deferred_signals_are_recorded_once_instead_of_ending_the_process() {
        defer_termination_signals().unwrap();
        take_deferred_signals();
        for signal in [libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
            // SAFETY: 처리기를 설치한 신호를 자기 프로세스에 보낸다.
            assert_eq!(unsafe { libc::raise(signal) }, 0);
        }
        let got = take_deferred_signals();
        assert_eq!(
            got,
            DeferredSignals {
                interrupt: true,
                hangup: true,
                terminate: true
            }
        );
        assert!(!take_deferred_signals().any());
    }

    /// 리더가 먼저 끝나도 회수하기 전에 그룹 신호를 보내 남은 하위 프로세스를 끝낸다. 종료 확인은
    /// 리더를 회수하지 않는다.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_exited_leaders_group_is_signalled_before_the_leader_is_reaped() {
        use std::os::unix::process::CommandExt;
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("inner.pid");
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 20 & echo $! > \"$1\"", "leader"])
            .arg(&pid_file)
            .process_group(0)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // 거짓을 읽은 뒤 기한이 지났으면 한 번 더 읽고 판정한다(검사 스레드 지연으로 거짓 실패하지 않게).
        while !exited_unreaped(&child).unwrap() {
            if std::time::Instant::now() >= deadline {
                assert!(exited_unreaped(&child).unwrap(), "리더가 끝나지 않았다");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let leader = std::path::PathBuf::from(format!("/proc/{}", child.id()));
        assert!(leader.exists(), "확인만으로 리더를 회수하면 안 된다");
        let inner = std::fs::read_to_string(&pid_file).unwrap();
        let proc_dir = std::path::PathBuf::from(format!("/proc/{}", inner.trim()));
        assert!(proc_dir.exists());
        let status =
            terminate_process_group(&mut child, std::time::Duration::from_secs(5)).unwrap();
        assert!(status.success(), "{status:?}");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while proc_dir.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !proc_dir.exists(),
            "하위 프로세스 {} 가 남았다",
            inner.trim()
        );
    }

    #[test]
    fn adopt_pid_without_init_is_noop() {
        // 초기화하지 않은 전역 job에는 실제 프로세스를 등록하지 않는다.
        adopt_pid(Some(0xFFFF_FFFF));
    }
}
