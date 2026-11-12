//! SSH 터널의 기동·회수와 재시도 간격.

use super::*;

/// 빈 로컬 포트와 점유 중인 리스너를 반환한다. 리스너를 놓은 뒤 ssh가 다시 바인드할
/// 때까지는 다른 프로세스가 포트를 차지할 수 있다.
pub(super) fn reserve_local_port() -> Result<(std::net::TcpListener, u16)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// ssh -L 자식의 수명을 관리한다. Drop에서 터널 자식을 종료·회수한다.
/// 원격 Tasty 프로세스는 터널과 별도로 실행된다.
pub struct SshTunnel {
    child: Child,
    /// 로컬 끝점 포트 — client 가 `127.0.0.1:local_port` 로 붙는다.
    pub local_port: u16,
    /// 포워드 대상 원격 포트.
    pub remote_port: u16,
    /// ssh stderr에 채널 연결 실패 줄이 나왔다([`crate::listener`]).
    channel_connect_failed: Arc<AtomicBool>,
    /// stderr를 넘기는 스레드. 준비 전 종료로 실패할 때만 끝나기를 기다린다.
    stderr_relay: Option<std::thread::JoinHandle<()>>,
}

/// 준비 전에 끝난 ssh의 마지막 stderr를 넘기는 스레드를 기다리는 상한. ssh가 끝나면 파이프가
/// 닫혀 스레드도 곧 끝난다. 파이프를 물려받은 다른 프로세스(ProxyCommand 등)가 남아 있으면
/// 닫히지 않으므로 상한을 둔다.
const STDERR_DRAIN_LIMIT: Duration = Duration::from_millis(500);

impl SshTunnel {
    /// `ssh -L 127.0.0.1:local:127.0.0.1:remote -N <dest>` 백그라운드 spawn 후
    /// 로컬 끝점이 LISTEN 상태가 될 때까지 폴링한다(ready-probe).
    pub fn establish(
        ssh: &Path,
        target: &SshTarget,
        remote_port: u16,
        verify: bool,
    ) -> Result<Self> {
        Self::establish_relaying_to(ssh, target, remote_port, verify, std::io::stderr)
    }

    /// [`Self::establish`]와 같고 ssh stderr를 `sink`가 만든 곳으로 넘긴다.
    fn establish_relaying_to<W: std::io::Write + 'static>(
        ssh: &Path,
        target: &SshTarget,
        remote_port: u16,
        verify: bool,
        sink: impl FnOnce() -> W + Send + 'static,
    ) -> Result<Self> {
        // ssh가 바인드하도록 예약을 해제한다. 그 사이 다른 프로세스가 차지할 수 있다.
        let (reservation, local_port) = reserve_local_port()?;
        drop(reservation);
        // loopback에만 바인드한다. 같은 호스트의 다른 사용자 접근까지 막지는 않는다.
        let forward = format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}");

        let mut args: Vec<String> = Vec::new();
        push_common_opts(&mut args, target, verify);
        // 포워드 실패 시 즉시 종료 → ready-probe 가 자식 사망으로 감지.
        args.push("-o".into());
        args.push("ExitOnForwardFailure=yes".into());
        args.push("-N".into()); // 원격 명령 없이 터널만
        args.push("-L".into());
        args.push(forward);
        args.push(target.destination.clone());

        let mut cmd = Command::new(ssh);
        // stderr는 인증·오류를 보이도록 그대로 사용자 stderr로 넘긴다(첫 연결 host key/passphrase 질문은
        // ssh가 터미널에 직접 쓴다). 넘기는 동안 리스너 없음 판정에 쓸 채널 연결 실패 줄을 표시한다.
        cmd.args(&args).stdin(Stdio::null()).stderr(Stdio::piped());
        // 호스트 GUI(windows subsystem, 콘솔 없음)가 in-process 로 이 함수를 호출하므로
        // CREATE_NO_WINDOW 를 걸지 않으면 Windows 가 ssh.exe 용 새 콘솔 창을 띄운다.
        tasty_utils::process::hide_console(&mut cmd);
        // 호스트가 SIGTERM 등으로 끝나도 터널이 고아로 남지 않게 호스트 수명에 묶는다.
        let mut child = tasty_reaper::spawn_bound_to_host(cmd)
            .map_err(|e| anyhow::anyhow!("ssh 터널 spawn 실패({}): {e}", ssh.display()))?;
        let channel_connect_failed = Arc::new(AtomicBool::new(false));
        let stderr_relay = child.stderr.take().and_then(|stderr| {
            let seen = Arc::clone(&channel_connect_failed);
            // ssh가 끝나 stderr가 닫히면 스레드도 끝난다. Drop은 기다리지 않는다.
            std::thread::Builder::new()
                .name("ssh-tunnel-stderr".into())
                .spawn(move || crate::listener::relay_stderr(stderr, sink(), &seen))
                .map_err(|e| tracing::warn!(%e, "ssh tunnel stderr relay thread could not start"))
                .ok()
        });

        let mut tunnel = SshTunnel {
            child,
            local_port,
            remote_port,
            channel_connect_failed,
            stderr_relay,
        };
        tunnel.wait_ready()?;
        Ok(tunnel)
    }

    /// ssh가 준비 전에 끝났을 때 마지막 stderr(인증 거절 등 실패 원인)가 오류보다 먼저 사용자에게
    /// 가도록 넘기는 스레드를 [`STDERR_DRAIN_LIMIT`]까지 기다린다.
    fn drain_stderr_relay(&mut self) {
        let Some(relay) = self.stderr_relay.take() else {
            return;
        };
        let deadline = Instant::now() + STDERR_DRAIN_LIMIT;
        while !relay.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !relay.is_finished() {
            tracing::debug!("ssh tunnel stderr relay still open after the tunnel exited");
            return;
        }
        if relay.join().is_err() {
            tracing::warn!("ssh tunnel stderr relay thread panicked");
        }
    }

    /// 로컬 연결을 폴링하며 반복 사이에 5초 기한과 자식 종료를 확인한다.
    /// 개별 TcpStream::connect에는 별도 timeout을 설정하지 않는다.
    fn wait_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.drain_stderr_relay();
                bail!(
                    "ssh 터널이 ready 전에 종료(status={status:?}) — 인증/포워드 실패 가능. \
                     (Windows 는 시스템 OpenSSH 풀경로/agent 확인)"
                );
            }
            if TcpStream::connect(("127.0.0.1", self.local_port)).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!("ssh 터널 ready 타임아웃(127.0.0.1:{})", self.local_port);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Terminate this exact tunnel and observe its child's wait result. Hosts call this on
    /// their retirement worker; the synchronous Drop contract used by CLI remains unchanged.
    pub fn terminate_and_reap(&mut self) -> std::io::Result<std::process::ExitStatus> {
        if let Err(error) = self.child.kill() {
            tracing::debug!(%error,"SSH tunnel kill returned before wait");
        }
        self.child.wait()
    }

    /// 이 터널의 리스너 없음 판정 핸들. 터널을 다른 곳으로 넘긴 뒤에도 실패를 분류할 때 쓴다.
    pub fn listener_verdict(&self) -> crate::ListenerVerdict {
        crate::ListenerVerdict::new(self.remote_port, Arc::clone(&self.channel_connect_failed))
    }

    /// 터널을 통한 요청이 `failure`로 실패한 원인이 원격 포트의 리스너 없음인지
    /// ([`crate::ListenerVerdict::remote_listener_absent`]).
    pub fn remote_listener_absent(&self, failure: &anyhow::Error) -> bool {
        self.listener_verdict().remote_listener_absent(failure)
    }

    /// 리스너 없음이면 `failure` 위에 [`crate::RemoteListenerAbsent`]를 맥락으로 붙이고, 아니면 그대로 돌려준다.
    pub fn explain_failure(&self, failure: anyhow::Error) -> anyhow::Error {
        self.listener_verdict().explain_failure(failure)
    }

    /// 터널 자식 ssh 가 살아있는지(끊김 감지 — 프로세스 레벨).
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// `SshTunnel::drop` 누적 소요(ns) — [`tunnel_drop_totals`] 참조.
pub(super) static TUNNEL_DROP_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
/// `SshTunnel::drop` 누적 횟수 — [`tunnel_drop_totals`] 참조.
pub(super) static TUNNEL_DROP_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// 지금까지 SshTunnel::drop에서 쓴 총 시간과 횟수.
/// 평시 터널 해제도 포함되므로 특정 종료 구간을 측정하려면 전후 차이를 사용한다.
pub fn tunnel_drop_totals() -> (Duration, u64) {
    use std::sync::atomic::Ordering;
    (
        Duration::from_nanos(TUNNEL_DROP_NANOS.load(Ordering::Relaxed)),
        TUNNEL_DROP_COUNT.load(Ordering::Relaxed),
    )
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;

        let t_drop = Instant::now();
        kill_and_reap(&mut self.child);
        TUNNEL_DROP_NANOS.fetch_add(
            u64::try_from(t_drop.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        TUNNEL_DROP_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// 재연결 대기 시간을 두 배씩 늘리고 max로 제한한다. 성공하면 reset으로 min으로 돌아간다.
pub struct Backoff {
    pub(super) cur: Duration,
    min: Duration,
    pub(super) max: Duration,
}

impl Backoff {
    /// 초기 500ms, 최대 30초의 대기 간격을 만든다.
    pub fn new() -> Self {
        Self {
            cur: Duration::from_millis(500),
            min: Duration::from_millis(500),
            max: Duration::from_secs(30),
        }
    }

    /// 현재 백오프만큼 sleep 한 뒤 다음 간격을 2 배(상한 max)로 늘린다.
    pub fn sleep(&mut self) {
        std::thread::sleep(self.cur);
        self.advance();
    }

    /// 현재 대기 간격을 조회한다(sleep 하지 않음) — GUI 메인 루프처럼 스레드를
    /// 블록할 수 없는 논블로킹 스케줄러가 "다음 재시도 시각"을 계산하는 데 쓴다.
    pub fn current(&self) -> Duration {
        self.cur
    }

    /// sleep 없이 다음 간격으로 넘어간다(2 배, 상한 max) — 논블로킹 스케줄러 전용.
    /// `sleep()`은 이 메서드 위에 blocking sleep 을 얹은 것과 동일하다.
    pub fn advance(&mut self) {
        self.cur = (self.cur * 2).min(self.max);
    }

    /// 연결 성공 시 백오프를 min 으로 되돌린다.
    pub fn reset(&mut self) {
        self.cur = self.min;
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// 쓸 때마다 늦게 받는 stderr 대상. 넘기는 스레드가 ssh 종료보다 늦게 끝나게 한다.
    struct SlowSink(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SlowSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(Duration::from_millis(100));
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 준비 전에 끝난 ssh가 마지막에 쓴 stderr 줄은 오류를 돌려주기 전에 모두 넘어간다.
    #[test]
    fn stderr_written_before_an_early_exit_is_relayed_before_the_error() {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let ssh =
            std::env::temp_dir().join(format!("tasty-ssh-early-exit-{}-{seq}", std::process::id()));
        crate::write_test_script(
            &ssh,
            "#!/bin/sh\necho 'first line' >&2\nsleep 0.05\necho 'Permission denied (publickey).' >&2\nexit 255\n",
        );
        let relayed = Arc::new(Mutex::new(Vec::new()));
        let sink = SlowSink(Arc::clone(&relayed));
        let result = SshTunnel::establish_relaying_to(
            &ssh,
            &SshTarget::parse("fake-host"),
            45123,
            false,
            move || sink,
        );
        if let Err(e) = std::fs::remove_file(&ssh) {
            tracing::warn!("fake ssh cleanup failed: {e}");
        }
        let error = result
            .err()
            .expect("the fake ssh exits before the tunnel is ready");
        assert!(format!("{error:#}").contains("ready"), "{error:#}");
        let relayed = String::from_utf8(
            relayed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        )
        .unwrap();
        assert_eq!(relayed, "first line\nPermission denied (publickey).\n");
    }
}
