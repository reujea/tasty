//! App-owned outbound connection lifetime. No View/engine relationship is duplicated here.
use super::client_session::AttachClientSession;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tasty_ssh::{Backoff, SshTunnel};
/// 실패한 재연결의 다음 시도 시각과 누적 횟수. 첫 시도는 슬롯 없이 즉시 수행한다.
pub struct ReconnectSlot {
    pub backoff: Backoff,
    pub next_attempt: Instant,
    pub attempts: u32,
    /// 이 재연결 구간이 시작된 시각. 재연결 시간 상한은 여기서부터 센다.
    pub started: Instant,
    /// 슬롯을 지우면 첫 시도로 취급하므로 중단 상태를 별도로 보존한다.
    pub given_up: bool,
}

/// AppEvent에 넣을 수 없는 터널 핸들을 별도 결과 채널로 전달한다.
pub struct AutoAttachOutcome {
    pub attempt: AttemptToken,
    /// 연결할 저장된 원격 행 ID. 행 없는 수동 요청은 None이며 중복 방지 집합에 넣지 않는다.
    pub row: Option<u32>,
    pub remote_ws: u32,
    pub result: anyhow::Result<(Option<SshTunnel>, u16)>,
    /// 신규 mirror 생성 대신 기존 세션 재연결과 실패 시 백오프 갱신을 수행할지 구별한다.
    pub is_reconnect: bool,
}

pub struct Remote {
    pub(crate) browsers: crate::browser::Browsers,
    retirements: Vec<crate::transport::RetirementReceipt>,
    attempt_threads: Vec<std::thread::JoinHandle<()>>,
    retirement_failed: bool,
    pub(crate) shutdown_started: Option<Instant>,
    pub(crate) pending_connections: std::collections::HashMap<
        super::pending_connection::ConnectionTicket,
        super::pending_connection::PendingConnection,
    >,
    pub(crate) next_connection: u64,
    pub(crate) connection_tx:
        std::sync::mpsc::SyncSender<super::pending_connection::ConnectionOutcome>,
    pub(crate) connection_rx:
        std::sync::mpsc::Receiver<super::pending_connection::ConnectionOutcome>,
    attempts: Vec<AttemptRecord>,
    pub sessions: Vec<AttachClientSession>,
    pub active: std::collections::HashSet<u32>,
    pub last_active_ws: Option<u32>,
    pub pending_reactivation: std::collections::HashSet<u32>,
    pub reconnect: std::collections::HashMap<u32, ReconnectSlot>,
    pub refusals: crate::refusal::Refusals,
    /// 이 실행 중 사용자가 연결을 끊은 원격 행. 명시적 연결 전까지 자동으로 붙지 않는다. 저장하지 않는다.
    pub user_disconnected: std::collections::HashSet<u32>,
    /// 서버가 강제로 끊은 연결 대상의 실행 중 보류. 값은 이 실행에서 명시적 연결로 풀린 행이다. 대상의
    /// 표지를 쓸 때와 기동 때 걸고, 대상의 마지막 행을 없애면 지운다. 저장하지 않는다.
    pub held: std::collections::HashMap<tasty_model::RemoteBinding, std::collections::HashSet<u32>>,
    next_attempt_id: u64,
    pub tx: std::sync::mpsc::Sender<AutoAttachOutcome>,
    pub rx: std::sync::mpsc::Receiver<AutoAttachOutcome>,
}
impl Default for Remote {
    fn default() -> Self {
        Self::new()
    }
}
impl Remote {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (connection_tx, connection_rx) = std::sync::mpsc::sync_channel(8);
        Self {
            browsers: Default::default(),
            pending_connections: Default::default(),
            next_connection: 1,
            connection_tx,
            connection_rx,
            retirements: Vec::new(),
            attempt_threads: Vec::new(),
            retirement_failed: false,
            shutdown_started: None,
            attempts: Vec::new(),
            sessions: Vec::new(),
            active: Default::default(),
            last_active_ws: None,
            pending_reactivation: Default::default(),
            reconnect: Default::default(),
            user_disconnected: Default::default(),
            held: Default::default(),
            refusals: Default::default(),
            next_attempt_id: 1,
            tx,
            rx,
        }
    }
}

/// 취소된 시도의 연결 단계가 돌려주는 오류 문구.
pub const ATTEMPT_RETIRED: &str = "connection attempt retired";

/// 시도가 있고 이미 비활성이면 취소 오류를, 아니면 원 오류를 돌려준다. 취소는 등록 소켓 양쪽을 닫으므로 그 뒤의
/// open 은 실패하고, 소켓 설정은 OS 에 따라 실패한다(macOS 는 양쪽을 닫은 소켓의 옵션 설정을 `EINVAL` 로
/// 거절하고 Linux 는 받아들인다). 판정은 오류의 원인을 보지 않고 시도의 활성 여부만 읽어 취소로 보고한다.
pub fn retired_or(
    attempt: Option<&AttemptToken>,
    error: impl Into<anyhow::Error>,
) -> anyhow::Error {
    if attempt.is_some_and(|attempt| !attempt.is_active()) {
        anyhow::anyhow!(ATTEMPT_RETIRED)
    } else {
        error.into()
    }
}

#[derive(Clone)]
pub struct AttemptToken(std::sync::Arc<AttemptState>);
struct AttemptState {
    id: u64,
    ssh: std::sync::Mutex<Vec<tasty_ssh::SshCancel>>,
    active: std::sync::atomic::AtomicBool,
    sockets: std::sync::Mutex<Vec<std::net::TcpStream>>,
    /// 시험 전용: 이 시도가 취소로 닫은 등록 소켓의 수. 닫는 함수 안에서 양쪽 닫기가 성공했을 때만 더한다.
    #[cfg(any(test, feature = "test-support"))]
    closed_sockets: std::sync::atomic::AtomicUsize,
}
impl PartialEq for AttemptToken {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}
impl Eq for AttemptToken {}
impl std::hash::Hash for AttemptToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&(std::sync::Arc::as_ptr(&self.0) as usize), state);
    }
}
impl AttemptState {
    /// Cancellation can race EOF or a previous shutdown of another clone of this socket.
    /// Failure does not establish a join: callers still retain and poll the worker receipt.
    fn shutdown_cancelled_socket(&self, socket: &std::net::TcpStream) {
        let shut = tasty_utils::socket_wake::shutdown_both_and_wake(socket);
        #[cfg(any(test, feature = "test-support"))]
        if shut.is_ok() {
            self.closed_sockets
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        match shut {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotConnected => {
                tracing::debug!("remote cancellation socket already disconnected: {error}");
            }
            Err(error) => {
                tracing::warn!(
                    "remote cancellation socket shutdown failed; waiting for worker exit: {error}"
                );
            }
        }
    }
}

// These lists own cancellation controls, not a stream writer or protocol frame state.
static ATTEMPT_SSH_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static ATTEMPT_SOCKETS_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

impl AttemptToken {
    /// 이 인스턴스 안에서 시도마다 다른 번호. 거절 기록과 IPC 응답이 같은 값을 쓴다.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn is_active(&self) -> bool {
        self.0.active.load(std::sync::atomic::Ordering::Acquire)
    }
    /// 시험 전용: 이 시도가 취소로 닫는 데 성공한 등록 소켓의 수. 닫기 실패는 세지 않는다.
    #[cfg(any(test, feature = "test-support"))]
    pub fn closed_socket_count(&self) -> usize {
        self.0
            .closed_sockets
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(crate) fn cancel(&self) {
        self.0
            .active
            .store(false, std::sync::atomic::Ordering::Release);
        {
            let mut ssh = tasty_utils::poison::recover_mutex(
                self.0.ssh.lock(),
                "remote attempt SSH cancellation controls",
                &ATTEMPT_SSH_POISON_REPORTED,
            );
            for cancel in ssh.drain(..) {
                cancel.request_cancel();
            }
        }
        {
            let mut sockets = tasty_utils::poison::recover_mutex(
                self.0.sockets.lock(),
                "remote attempt socket cancellation controls",
                &ATTEMPT_SOCKETS_POISON_REPORTED,
            );
            for socket in sockets.drain(..) {
                self.0.shutdown_cancelled_socket(&socket);
            }
        }
    }
    pub fn register_ssh(&self, cancel: tasty_ssh::SshCancel) -> Result<(), String> {
        let mut handles = self
            .0
            .ssh
            .lock()
            .map_err(|_| "SSH cancellation state poisoned".to_owned())?;
        if !self.is_active() {
            cancel.request_cancel();
            return Err("remote attempt retired".into());
        }
        handles.push(cancel);
        Ok(())
    }
    fn complete(&self) {
        self.0
            .active
            .store(false, std::sync::atomic::Ordering::Release);
        let mut sockets = tasty_utils::poison::recover_mutex(
            self.0.sockets.lock(),
            "remote attempt socket cancellation controls",
            &ATTEMPT_SOCKETS_POISON_REPORTED,
        );
        sockets.clear();
    }
    /// Register the exact connecting socket before the first blocking handshake read/write.
    pub fn register_socket(&self, socket: &std::net::TcpStream) -> std::io::Result<()> {
        let control = socket.try_clone()?;
        let mut sockets = self
            .0
            .sockets
            .lock()
            .map_err(|_| std::io::Error::other("connection cancellation state poisoned"))?;
        if !self.is_active() {
            self.0.shutdown_cancelled_socket(&control);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                ATTEMPT_RETIRED,
            ));
        }
        sockets.push(control);
        Ok(())
    }
    fn same(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
pub struct AttemptRecord {
    pub token: AttemptToken,
    pub row: Option<u32>,
    pub binding: Option<tasty_model::RemoteBinding>,
}
impl Remote {
    pub fn spawn_attempt(
        &mut self,
        token: AttemptToken,
        work: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        self.reap_attempts();
        let failure = if self.shutdown_started.is_some() {
            Some("remote is shutting down")
        } else if !token.is_active() {
            Some("remote attempt is retired")
        } else if self.attempt_threads.len() >= 64 {
            Some("remote endpoint worker capacity exhausted")
        } else {
            None
        };
        if let Some(reason) = failure {
            token.cancel();
            self.attempts.retain(|attempt| !attempt.token.same(&token));
            return Err(reason.into());
        }
        match std::thread::Builder::new()
            .name("remote-endpoint".into())
            .spawn(work)
        {
            Ok(thread) => {
                self.attempt_threads.push(thread);
                Ok(())
            }
            Err(error) => {
                token.cancel();
                self.attempts.retain(|attempt| !attempt.token.same(&token));
                Err(error.to_string())
            }
        }
    }
    pub fn reap_attempts(&mut self) {
        let mut running = Vec::new();
        for thread in std::mem::take(&mut self.attempt_threads) {
            if thread.is_finished() {
                if thread.join().is_err() {
                    self.retirement_failed = true;
                    tracing::error!("remote endpoint worker panicked");
                }
            } else {
                running.push(thread);
            }
        }
        self.attempt_threads = running;
    }
    pub fn track_workers(&mut self, receipt: crate::transport::RetirementReceipt) {
        self.retirement_failed |= self.retirements.iter().any(|receipt| receipt.failed());
        self.retirements.retain(|receipt| !receipt.is_done());
        self.retirements.push(receipt);
    }
    pub fn retire_tunnel(&mut self, tunnel: Option<SshTunnel>) {
        if let Some(tunnel) = tunnel {
            self.track_workers(crate::transport::retire_tunnel(tunnel));
        }
    }
    pub fn discard_endpoint_outcome(&mut self, outcome: AutoAttachOutcome) {
        if let Ok((tunnel, _)) = outcome.result {
            self.retire_tunnel(tunnel);
        }
    }
    pub fn begin_shutdown(&mut self) {
        if self.shutdown_started.is_some() {
            return;
        }
        self.shutdown_started = Some(Instant::now());
        for attempt in &self.attempts {
            attempt.token.cancel();
        }
        self.shutdown_browsers();
        self.sessions.clear();
        self.pending_connections.clear();
        self.collect_connections();
        let threads = std::mem::take(&mut self.attempt_threads);
        if !threads.is_empty() {
            self.track_workers(crate::transport::join_attempt_workers(threads));
        }
        while let Ok(outcome) = self.rx.try_recv() {
            self.discard_endpoint_outcome(outcome);
        }
    }
    /// Joining and deadline expiry are distinct observations; callers must not call timeout success.
    pub fn shutdown_observation(&mut self) -> ShutdownObservation {
        self.collect_connections();
        self.shutdown_browsers();
        while let Ok(outcome) = self.rx.try_recv() {
            self.discard_endpoint_outcome(outcome);
        }
        if self.retirements.iter().all(|receipt| receipt.is_done()) {
            // Acquire of every joined receipt follows the worker's final send. Drain once more
            // after that observation so Joined also releases the final owned SSH tunnel outcome.
            while let Ok(outcome) = self.rx.try_recv() {
                self.discard_endpoint_outcome(outcome);
            }
            self.collect_connections();
            self.shutdown_browsers();
            if self.retirements.iter().any(|receipt| !receipt.is_done()) {
                return ShutdownObservation::Waiting;
            }
            if self.retirement_failed || self.retirements.iter().any(|receipt| receipt.failed()) {
                ShutdownObservation::WorkerFailed
            } else {
                ShutdownObservation::Joined
            }
        } else if self
            .shutdown_started
            .is_some_and(|start| start.elapsed() >= std::time::Duration::from_secs(5))
        {
            ShutdownObservation::TimedOut
        } else {
            ShutdownObservation::Waiting
        }
    }
    pub fn begin_attempt(
        &mut self,
        row: Option<u32>,
        binding: Option<tasty_model::RemoteBinding>,
    ) -> Result<AttemptToken, &'static str> {
        if self.shutdown_started.is_some() {
            return Err("remote is shutting down");
        }
        self.retirement_failed |= self.retirements.iter().any(|receipt| receipt.failed());
        self.retirements.retain(|receipt| !receipt.is_done());
        if self.retirements.len() >= 128 {
            return Err("remote resource retirement capacity exhausted");
        }
        if let Some(row) = row {
            self.attempts.retain(|attempt| {
                if attempt.row == Some(row) {
                    attempt.token.cancel();
                    false
                } else {
                    true
                }
            });
        }
        if self.attempts.len() >= 64 {
            return Err("remote connection attempt queue is full");
        }
        let id = self.next_attempt_id;
        self.next_attempt_id += 1;
        let token = AttemptToken(std::sync::Arc::new(AttemptState {
            id,
            ssh: std::sync::Mutex::new(Vec::new()),
            active: std::sync::atomic::AtomicBool::new(true),
            sockets: std::sync::Mutex::new(Vec::new()),
            #[cfg(any(test, feature = "test-support"))]
            closed_sockets: Default::default(),
        }));
        self.attempts.push(AttemptRecord {
            token: token.clone(),
            row,
            binding,
        });
        Ok(token)
    }
    pub fn cancel_attempt(&mut self, token: &AttemptToken) {
        token.cancel();
        self.attempts.retain(|attempt| !attempt.token.same(token));
    }
    pub fn finish_attempt(&mut self, token: &AttemptToken) -> Option<AttemptRecord> {
        let index = self
            .attempts
            .iter()
            .position(|attempt| attempt.token.same(token))?;
        let record = self.attempts.remove(index);
        let active = record.token.is_active();
        record.token.complete();
        active.then_some(record)
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.begin_shutdown();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownObservation {
    Waiting,
    Joined,
    WorkerFailed,
    TimedOut,
}

impl ReconnectSlot {
    pub fn new() -> Self {
        Self {
            backoff: Backoff::new(),
            next_attempt: Instant::now(),
            attempts: 0,
            started: Instant::now(),
            given_up: false,
        }
    }
}
impl Default for ReconnectSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// A full result queue retains its owned value only while the original attempt is live.
/// Cancellation releases a blocked producer so shutdown can observe its actual join.
pub(crate) fn send_attempt_result<T>(
    sender: &std::sync::mpsc::SyncSender<T>,
    token: &AttemptToken,
    mut value: T,
) {
    loop {
        if !token.is_active() {
            return;
        }
        match sender.try_send(value) {
            Ok(()) => return,
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return,
            Err(std::sync::mpsc::TrySendError::Full(pending)) => value = pending,
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
