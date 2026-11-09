//! 플러그인 자식 프로세스와 양방향 채널.
//! spawn은 자식을 실행한 뒤 연결 대기를 별도 스레드에 맡긴다.
//! 연결 전 요청은 큐에 쌓이며, 연결 뒤 송수신 스레드가 처리한다.
//! 수신 시각을 last_pong에 기록해 무응답 여부를 판단한다.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use tasty_plugin_protocol::{HandleChannelMessage, PixelRect, SharedBufferId};

use crate::handle_channel::{HandleListener, HandleStream, HandleStreamReader};
use crate::listener::HostListener;
use crate::protocol::{PluginEvent, PluginRequest, PluginResponse};
#[cfg(any(test, feature = "test-support"))]
use channel_bytes::ChannelLimits;
use channel_bytes::{
    Admission, ChannelLedger, Direction, MeteredReceiver, MeteredSender, Refusal, TrySendRefusal,
    metered_channel,
};
use tasty_plugin_manifest::{HOST_API_VERSION, PluginPackage};

// 내부 상태 잠금은 poison을 처음 한 번 알리고 기존 값을 재사용한다.
// 복구 자체가 패닉 전의 부분 변경을 되돌리는 것은 아니다.
static LAST_PONG_POISONED: AtomicBool = AtomicBool::new(false);
const LAST_PONG_WHAT: &str = "plugin last-pong timestamp";
static HANDLE_STATE_POISONED: AtomicBool = AtomicBool::new(false);
const HANDLE_STATE_WHAT: &str = "plugin aux handle stream state";
static DIRTY_RECTS_POISONED: AtomicBool = AtomicBool::new(false);
const DIRTY_RECTS_WHAT: &str = "plugin dirty-rects map";

// 보조 채널 쓰기 중 패닉이 나면 프레임 일부만 전송됐을 수 있다.
// 이 잠금은 복구해 계속 쓰지 않고 연산을 생략하며 처음 한 번 경고한다.
static WITH_HANDLE_STREAM_POISONED: AtomicBool = AtomicBool::new(false);

/// 보조 채널의 첫 사용 시 연결을 기다리는 한도. 시작 직후 호출을 고려해 500ms를 둔다.
const HANDLE_STREAM_MATERIALIZE_TIMEOUT: Duration = Duration::from_millis(500);

/// 보조 핸들 채널 상태 머신. spawn 시점에 Pending(rx) 또는 Unavailable로 초기화되고,
/// 첫 사용 시 Pending → Ready(stream)으로 전이. Ready 전이 시 reader 스레드도 함께
/// 시작되어 plugin이 보내는 Dirty 메시지를 수신한다.
enum HandleStreamState {
    /// 아직 plugin이 보조 채널에 connect하지 않음. mailbox에서 try-recv 대기.
    Pending(mpsc::Receiver<HandleStream>),
    /// 한 번 materialize 완료. write 핸들은 Arc로 공유 — reader 스레드가 Pong을
    /// 응답할 때도 같은 stream을 쓴다.
    Ready(Arc<Mutex<HandleStream>>),
    /// 보조 채널을 사용할 수 없거나 reader 분리에 실패했다. 이후 호출도 None을 반환한다.
    Unavailable,
}

/// 호스트→플러그인 요청 큐의 개수 상한. 응답·이벤트 큐도 같은 용량을 쓴다.
/// 관측된 정상 부하에서 계산한 값은 아니므로, 포화 발생 여부를 계속 확인해야 한다.
/// 직렬화 바이트의 상한은 channel_bytes가 별도로 적용한다.
pub(crate) const REQUEST_QUEUE_CAPACITY: usize = 1024;
/// plugin → 호스트 응답 큐 용량. 근거는 [`REQUEST_QUEUE_CAPACITY`] 와 같다.
pub(crate) const RESPONSE_QUEUE_CAPACITY: usize = 1024;
/// plugin → 호스트 이벤트 큐 용량. 근거는 [`REQUEST_QUEUE_CAPACITY`] 와 같다.
pub(crate) const EVENT_QUEUE_CAPACITY: usize = 1024;

/// 요청을 큐에 넣지 못한 이유. 일시적인 포화와 수신단 종료를 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSendError {
    /// 큐가 가득 차 요청을 버렸다.
    Full,
    /// 이 큐 또는 전체 플러그인 채널의 바이트 상한을 넘어 요청을 버렸다.
    OverBytes(Refusal),
    /// 송신 큐의 수신단이 종료됐다.
    Disconnected,
    /// 요청을 JSON 한 줄로 직렬화하지 못했다.
    Encode,
}

impl std::fmt::Display for RequestSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(
                f,
                "request queue full ({REQUEST_QUEUE_CAPACITY}) — request rejected"
            ),
            Self::OverBytes(Refusal::Queue) => {
                write!(f, "request queue over its byte budget — request rejected")
            }
            Self::OverBytes(Refusal::Total) => write!(
                f,
                "plugin channels over their total byte budget — the host is holding too much \
                 for all plugins together"
            ),
            Self::OverBytes(Refusal::Closed) | Self::Disconnected => {
                write!(f, "writer thread gone")
            }
            Self::Encode => write!(f, "request could not be encoded"),
        }
    }
}

/// 큐가 찼을 때 기다리지 않고 거절한다. 메인 스레드가 송신 때문에 멈추지 않게 한다.
pub(crate) fn try_send_request<T>(
    tx: &mpsc::SyncSender<T>,
    req: T,
) -> Result<(), RequestSendError> {
    match tx.try_send(req) {
        Ok(()) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => Err(RequestSendError::Full),
        Err(mpsc::TrySendError::Disconnected(_)) => Err(RequestSendError::Disconnected),
    }
}

pub struct PluginProcess {
    /// Async replies must identify this owner rather than a later process with the same plugin ID.
    reply_binding: Arc<()>,
    pub plugin_id: String,
    child: Option<Child>,
    /// 직렬화한 요청 줄. 바이트 상한을 검사한 결과를 writer가 그대로 쓴다.
    /// 비공개로 두어 다른 모듈이 blocking send를 직접 호출하지 못하게 한다.
    req_tx: MeteredSender<String>,
    /// 포화로 버렸지만 아직 알리지 못한 요청 수. 다음으로 큐에 들어가는 요청에 싣고 뺀다.
    dropped_requests: AtomicU64,
    pub resp_rx: MeteredReceiver<PluginResponse>,
    pub event_rx: MeteredReceiver<PluginEvent>,
    last_pong: Arc<Mutex<Instant>>,
    /// 보조 핸들 채널 상태. 첫 사용 시 Pending → Ready 전이하며 reader 스레드 시작.
    handle_state: Mutex<HandleStreamState>,
    /// reader 스레드가 누적하는 dirty rect. `Some(rect)`는 union된 영역, `None`은
    /// "전체 갱신" sticky flag. 호스트 main loop이 frame 합성 시 `take_dirty_rects`로
    /// drain한다.
    dirty_rects: Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>>,
    /// 연결 대기 결과는 매니저가 pump에서 한 번 가져간다.
    connect: Arc<connect::ConnectSlot>,
    /// 연결 완료 시각부터 요청 deadline을 계산한다. None이면 아직 연결 중이다.
    connected_at: Option<Instant>,
}

#[cfg(test)]
impl PluginProcess {
    /// 실제 자식을 회수하는 시험용 stub. 송신 큐는 끊겨 있어 shutdown 요청은 전달되지 않는다.
    pub(crate) fn stub_with_child(plugin_id: &str, child: Child) -> Self {
        let mut proc = Self::stub_for_test(plugin_id);
        proc.child = Some(child);
        proc
    }

    /// 연결 전 요청의 deadline을 시험하도록 연결 대기 상태로 바꾼다.
    pub(crate) fn mark_connecting_for_test(&mut self) {
        self.connected_at = None;
    }

    /// 실제 대기 없이 무응답 상태를 시험하도록 마지막 수신 시각을 과거로 옮긴다.
    pub(crate) fn backdate_pong_for_test(&self, by: Duration) {
        let mut last = self.last_pong.lock().expect("fresh mutex");
        *last = Instant::now()
            .checked_sub(by)
            .expect("the clock goes back far enough");
    }
}

/// 매니저 밖(본체 시험)에서도 namespace 소유자를 흉내 내도록 `test-support` 기능에서 연다.
#[cfg(any(test, feature = "test-support"))]
impl PluginProcess {
    /// 시험에서 보낸 요청을 읽을 수 있도록 수신단을 유지하는 stub.
    pub(crate) fn stub_with_request_rx(plugin_id: &str) -> (Self, RequestTap) {
        Self::stub_with_request_rx_capacity(plugin_id, REQUEST_QUEUE_CAPACITY)
    }

    /// 큐 용량을 지정해 포화를 시험하는 stub.
    pub(crate) fn stub_with_request_rx_capacity(
        plugin_id: &str,
        capacity: usize,
    ) -> (Self, RequestTap) {
        Self::stub_with_request_rx_in(
            plugin_id,
            capacity,
            &ChannelLedger::new(ChannelLimits::default()),
        )
    }

    /// 바이트 상한을 지정해 시험하는 stub.
    pub(crate) fn stub_with_request_rx_in(
        plugin_id: &str,
        capacity: usize,
        ledger: &Arc<ChannelLedger>,
    ) -> (Self, RequestTap) {
        let (req_tx, req_rx) =
            metered_channel(capacity, ledger.open_queue(plugin_id, Direction::Request));
        let mut proc = Self::stub_for_test(plugin_id);
        proc.req_tx = req_tx;
        (proc, RequestTap(req_rx))
    }

    /// 자식이 없고 송수신 채널이 끊겨 있는 단위 테스트용 stub.
    pub(crate) fn stub_for_test(plugin_id: &str) -> Self {
        let ledger = ChannelLedger::new(ChannelLimits::default());
        let (req_tx, _req_rx) = metered_channel(
            REQUEST_QUEUE_CAPACITY,
            ledger.open_queue(plugin_id, Direction::Request),
        );
        let (_resp_tx, resp_rx) = metered_channel(
            RESPONSE_QUEUE_CAPACITY,
            ledger.open_queue(plugin_id, Direction::Response),
        );
        let (_event_tx, event_rx) = metered_channel(
            EVENT_QUEUE_CAPACITY,
            ledger.open_queue(plugin_id, Direction::Event),
        );
        Self {
            reply_binding: Arc::new(()),
            plugin_id: plugin_id.into(),
            child: None,
            req_tx,
            dropped_requests: AtomicU64::new(0),
            resp_rx,
            event_rx,
            last_pong: Arc::new(Mutex::new(Instant::now())),
            handle_state: Mutex::new(HandleStreamState::Unavailable),
            dirty_rects: Arc::new(Mutex::new(HashMap::new())),
            connect: connect::ConnectSlot::reported(),
            connected_at: Some(Instant::now()),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// 자식 실행 직후 호스트를 지연시켜, 빠른 연결도 인증 정보 등록 뒤에 처리되는지 시험한다.
    pub(crate) static AFTER_CHILD_SPAWN_DELAY: std::cell::Cell<Duration> =
        const { std::cell::Cell::new(Duration::ZERO) };
}

/// 송신 큐의 JSON 줄을 요청으로 읽는 시험용 수신단.
#[cfg(any(test, feature = "test-support"))]
pub(crate) struct RequestTap(MeteredReceiver<String>);

#[cfg(any(test, feature = "test-support"))]
impl RequestTap {
    pub(crate) fn try_recv(&self) -> Result<PluginRequest, mpsc::TryRecvError> {
        let line = self.0.try_recv()?;
        Ok(serde_json::from_str(&line).expect("호스트가 보낸 줄은 요청으로 읽혀야 한다"))
    }
}

impl PluginProcess {
    pub fn spawn(
        package: &PluginPackage,
        listener: &HostListener,
        handle_listener: Option<&HandleListener>,
        log_dir: &Path,
        waker: tasty_terminal::waker_factory::SharedWakerFactory,
        reaper: &crate::reaper::PluginReaper,
        ledger: &Arc<ChannelLedger>,
    ) -> anyhow::Result<Self> {
        let token = generate_token();
        std::fs::create_dir_all(log_dir).ok();
        let log_path = log_dir.join(format!("{}.log", sanitize_id(&package.manifest.id)));
        let log_file = std::fs::File::create(&log_path)?;
        let log_clone = log_file.try_clone()?;
        let entry_path = package.entry_command_path();

        let (mut cmd, handle_stream_rx) = build_plugin_command(
            package,
            listener,
            handle_listener,
            reaper,
            &token,
            log_file,
            log_clone,
        )?;
        inject_plugin_data_env(&mut cmd, package, &log_path)?;

        // 빠르게 연결하는 플러그인도 인증할 수 있도록 자식 실행 전에 등록한다.
        let pending = listener.register(&token);

        // Linux PDEATHSIG는 fork한 스레드의 수명에 연결되므로 영속 spawner에서 실행한다.
        let child = reaper.spawn_bound(cmd).map_err(|e| {
            anyhow::anyhow!(
                "failed to spawn plugin '{}' ({}): {}",
                package.manifest.id,
                entry_path.display(),
                e
            )
        })?;
        let spawned_at = Instant::now();
        #[cfg(test)]
        std::thread::sleep(AFTER_CHILD_SPAWN_DELAY.with(std::cell::Cell::get));

        // Windows Job Object 등록 실패는 경고하고 실행을 계속한다.
        if let Err(e) = reaper.adopt(&child) {
            tracing::warn!(
                "plugin '{}' lifetime adopt failed — process not bound to host lifetime: {e}",
                package.manifest.id
            );
        }

        // 보조 채널 연결은 기다리지 않고, shared buffer를 처음 사용할 때 가져온다.

        let last_pong = Arc::new(Mutex::new(Instant::now()));
        let id = &package.manifest.id;
        let (req_tx, req_rx) = metered_channel::<String>(
            REQUEST_QUEUE_CAPACITY,
            ledger.open_queue(id, Direction::Request),
        );
        let (resp_tx, resp_rx) = metered_channel::<PluginResponse>(
            RESPONSE_QUEUE_CAPACITY,
            ledger.open_queue(id, Direction::Response),
        );
        let (event_tx, event_rx) = metered_channel::<PluginEvent>(
            EVENT_QUEUE_CAPACITY,
            ledger.open_queue(id, Direction::Event),
        );

        // 연결은 별도 스레드에서 기다린다. 그동안 요청은 송신 큐에 쌓인다.
        let connect = connect::start(connect::ConnectJob {
            plugin_id: id.clone(),
            log_path,
            pending,
            req_rx,
            resp_tx,
            event_tx,
            last_pong: last_pong.clone(),
            waker,
            spawned_at,
        });

        let initial_state = match handle_stream_rx {
            Some(rx) => HandleStreamState::Pending(rx),
            None => HandleStreamState::Unavailable,
        };
        Ok(Self {
            reply_binding: Arc::new(()),
            plugin_id: package.manifest.id.clone(),
            child: Some(child),
            req_tx,
            dropped_requests: AtomicU64::new(0),
            resp_rx,
            event_rx,
            last_pong,
            handle_state: Mutex::new(initial_state),
            dirty_rects: Arc::new(Mutex::new(HashMap::new())),
            connect,
            connected_at: None,
        })
    }

    /// 완료된 연결 결과를 한 번만 가져간다. 대기 중이거나 이미 가져갔으면 None.
    pub(crate) fn take_connect_outcome(&self) -> Option<connect::ConnectOutcome> {
        self.connect.take()
    }

    /// 연결 성사 시각. 아직 연결 중이면 `None`.
    pub(crate) fn connected_at(&self) -> Option<Instant> {
        self.connected_at
    }

    /// 매니저가 연결 성공 결과를 처리했을 때 호출한다.
    pub(crate) fn mark_connected(&mut self, at: Instant) {
        self.connected_at = Some(at);
    }

    /// deadline까지 연결 결과를 기다리되 결과는 꺼내지 않는다. 부팅 워커 등에서 사용한다.
    pub(crate) fn wait_connect_settled(&self, deadline: Instant) {
        self.connect.wait_settled(deadline);
    }

    /// 보조 채널의 첫 사용 시 짧게 연결을 기다리고 reader 스레드를 시작한다.
    /// 이후에는 저장한 stream을 잠가 closure에 전달한다. 사용할 수 없으면 None.
    pub fn with_handle_stream<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut HandleStream) -> R,
    {
        let arc = self.ensure_handle_stream()?;
        let mut g = match arc.lock() {
            Ok(g) => g,
            Err(_) => {
                if !WITH_HANDLE_STREAM_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::error!(
                        "plugin aux handle stream lock poisoned — skipping this shared-buffer op; \
                         a frame may have been partially written before the panic"
                    );
                }
                return None;
            }
        };
        Some(f(&mut g))
    }

    fn ensure_handle_stream(&self) -> Option<Arc<Mutex<HandleStream>>> {
        let mut state = tasty_utils::poison::recover_mutex(
            self.handle_state.lock(),
            HANDLE_STATE_WHAT,
            &HANDLE_STATE_POISONED,
        );
        match &*state {
            HandleStreamState::Ready(arc) => return Some(arc.clone()),
            HandleStreamState::Unavailable => return None,
            HandleStreamState::Pending(_) => {}
        }
        let rx = match std::mem::replace(&mut *state, HandleStreamState::Unavailable) {
            HandleStreamState::Pending(rx) => rx,
            other => {
                *state = other;
                return None;
            }
        };
        match self.materialize_handle_stream(rx) {
            Ok(arc) => {
                *state = HandleStreamState::Ready(arc.clone());
                Some(arc)
            }
            // 재시도 가능(timeout) — rx 를 Pending 으로 되돌려 다음 호출이 이어받는다.
            Err(Some(rx)) => {
                *state = HandleStreamState::Pending(rx);
                None
            }
            // 영구 불가(disconnected / reader split 실패 / reader thread spawn 실패)
            // — state 는 이미 Unavailable 로 replace 되어 있다.
            Err(None) => None,
        }
    }

    /// Pending mailbox 에서 stream 을 꺼내 reader 스레드까지 띄운다.
    /// `Err(Some(rx))` 는 재시도 가능(timeout) — caller 가 rx 를 Pending 으로
    /// 되돌린다. `Err(None)` 은 영구 불가.
    fn materialize_handle_stream(
        &self,
        rx: mpsc::Receiver<HandleStream>,
    ) -> Result<Arc<Mutex<HandleStream>>, Option<mpsc::Receiver<HandleStream>>> {
        let stream = match rx.recv_timeout(HANDLE_STREAM_MATERIALIZE_TIMEOUT) {
            Ok(s) => s,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    "plugin '{}' handle stream not yet available",
                    self.plugin_id
                );
                return Err(Some(rx));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // accept thread가 종료됨. 영구적으로 사용 불가.
                return Err(None);
            }
        };
        let reader = match stream.reader() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "plugin '{}' handle stream reader split failed: {e}",
                    self.plugin_id
                );
                return Err(None);
            }
        };
        let arc = Arc::new(Mutex::new(stream));
        if !self.spawn_aux_reader_thread(arc.clone(), reader) {
            return Err(None);
        }
        Ok(arc)
    }

    /// aux 채널 reader 스레드를 띄운다. thread spawn 실패 시 false(caller 는
    /// 영구 Unavailable 로 처리).
    fn spawn_aux_reader_thread(
        &self,
        writer: Arc<Mutex<HandleStream>>,
        reader: HandleStreamReader,
    ) -> bool {
        let dirty = self.dirty_rects.clone();
        let plugin_id = self.plugin_id.clone();
        if let Err(e) = std::thread::Builder::new()
            .name(format!("plugin-aux-rx-{}", sanitize_id(&plugin_id)))
            .spawn(move || aux_reader_loop(reader, dirty, writer, plugin_id))
        {
            tracing::warn!(
                "plugin '{}' aux reader thread spawn failed: {e}",
                self.plugin_id
            );
            return false;
        }
        true
    }

    /// reader 스레드가 누적한 dirty rect를 drain. 호스트 main loop이 frame 합성 직전에
    /// 호출. 반환된 map의 value가 `None`이면 "전체 갱신".
    pub fn take_dirty_rects(&self) -> HashMap<SharedBufferId, Option<PixelRect>> {
        let mut guard = tasty_utils::poison::recover_mutex(
            self.dirty_rects.lock(),
            DIRTY_RECTS_WHAT,
            &DIRTY_RECTS_POISONED,
        );
        std::mem::take(&mut *guard)
    }

    /// 자식 프로세스의 OS PID. Windows의 `DuplicateHandle` 대상 식별에 필요.
    /// `shutdown` 이후나 stub 인스턴스에서는 `None`.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// A non-owning token for replies delayed by a host commit or another asynchronous boundary.
    pub fn reply_binding(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.reply_binding)
    }

    /// 요청을 직렬화하고 개수·바이트 상한 안에서 큐에 넣는다. 포화 시 기다리지 않는다.
    /// 앞서 버린 요청 수는 다음으로 큐에 들어가는 요청에 함께 보낸다.
    /// 별도 알림을 보내면 그 알림도 가득 찬 큐에 넣어야 하기 때문이다.
    pub fn try_send_request(&self, req: PluginRequest) -> Result<(), RequestSendError> {
        self.try_send_request_as(req, Admission::Data)
    }

    /// ping·shutdown은 전체 바이트 상한만 면제한다. 개별 큐의 상한은 유지한다.
    fn try_send_request_as(
        &self,
        mut req: PluginRequest,
        admission: Admission,
    ) -> Result<(), RequestSendError> {
        let carried = self.dropped_requests.load(Ordering::Relaxed);
        req.dropped_requests = carried;
        let line = match serde_json::to_string(&req) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("plugin '{}' request encode error: {e}", self.plugin_id);
                return Err(RequestSendError::Encode);
            }
        };
        // 줄 끝 개행까지 소켓에 나가는 바이트다.
        let bytes = line.len() + 1;
        match self.req_tx.try_send(line, bytes, admission) {
            Ok(()) => {
                if carried > 0 {
                    // 단일 송신자가 실제로 실어 보낸 수만 뺀다.
                    self.dropped_requests.fetch_sub(carried, Ordering::Relaxed);
                }
                Ok(())
            }
            Err(TrySendRefusal::Full) => {
                self.dropped_requests.fetch_add(1, Ordering::Relaxed);
                Err(RequestSendError::Full)
            }
            Err(TrySendRefusal::Bytes(r)) => {
                self.dropped_requests.fetch_add(1, Ordering::Relaxed);
                Err(RequestSendError::OverBytes(r))
            }
            Err(TrySendRefusal::Disconnected) => Err(RequestSendError::Disconnected),
        }
    }

    /// 아직 plugin 에게 안 알린 누적 드롭 수. 시험과 진단용.
    #[cfg(test)]
    pub(crate) fn unreported_drops(&self) -> u64 {
        self.dropped_requests.load(Ordering::Relaxed)
    }

    pub fn ping(&self, next_id: u64) {
        if let Err(e) = self.try_send_request_as(
            PluginRequest::new("ping", serde_json::json!({}), next_id),
            Admission::Control,
        ) {
            tracing::warn!("plugin '{}' ping send failed: {e}", self.plugin_id);
        }
    }

    pub fn since_last_pong(&self) -> Duration {
        tasty_utils::poison::recover_mutex(
            self.last_pong.lock(),
            LAST_PONG_WHAT,
            &LAST_PONG_POISONED,
        )
        .elapsed()
    }

    /// 종료를 요청하고 자식 핸들을 PendingShutdown으로 옮긴다. 여기서는 기다리지 않는다.
    /// 여러 플러그인에 먼저 요청한 뒤 공유 deadline으로 기다릴 수 있다.
    pub fn begin_shutdown(mut self, deadline: Instant) -> PendingShutdown {
        if let Err(e) = self.try_send_request_as(
            PluginRequest::new("shutdown", serde_json::json!({}), u64::MAX),
            Admission::Control,
        ) {
            tracing::warn!("plugin '{}' shutdown send failed: {e}", self.plugin_id);
        }
        PendingShutdown {
            plugin_id: std::mem::take(&mut self.plugin_id),
            child: self.child.take(),
            deadline,
            started: Instant::now(),
        }
    }

    /// 연결에 실패한 자식은 종료 요청 없이 바로 강제 종료하도록 핸들을 반환한다.
    /// 실제 kill과 회수는 핸들을 받은 쪽에서 처리한다.
    pub(crate) fn abandon(mut self) -> PendingShutdown {
        let now = Instant::now();
        PendingShutdown {
            plugin_id: std::mem::take(&mut self.plugin_id),
            child: self.child.take(),
            deadline: now,
            started: now,
        }
    }

    /// 종료 요청과 대기를 함께 수행한다. 결과는 종료 로그의 reason 값으로 사용한다.
    pub fn shutdown(self, timeout: Duration) -> ShutdownOutcome {
        self.begin_shutdown(Instant::now() + timeout).wait()
    }
}

/// 자식 종료 확인 주기. 짧을수록 빨리 확인하지만 대기 스레드의 호출 횟수가 늘어난다.
pub const CHILD_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 종료 요청 이후 자식의 상태를 확인하고 deadline 뒤 강제 종료를 시도하는 핸들.
pub struct PendingShutdown {
    plugin_id: String,
    /// 아직 정리하지 않은 자식 핸들.
    child: Option<Child>,
    /// graceful 종료를 기다려 주는 한계 시각. 초과하면 force kill.
    deadline: Instant,
    started: Instant,
}

impl PendingShutdown {
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// `begin_shutdown` 이후 경과 — 계측의 개별 plugin 소요(`S4a`)용.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// deadline 전에는 try_wait로 확인하고 살아 있으면 None을 반환한다.
    /// deadline을 넘었거나 확인에 실패하면 kill과 wait를 호출하므로 이 경로는 기다릴 수 있다.
    pub fn poll(&mut self) -> Option<ShutdownOutcome> {
        let Some(child) = self.child.as_mut() else {
            return Some(ShutdownOutcome::NoChild);
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                self.child = None;
                Some(ShutdownOutcome::Graceful)
            }
            Ok(None) if Instant::now() <= self.deadline => None,
            // deadline 초과. 강제 종료로 넘어간다.
            Ok(None) => Some(self.force_kill()),
            // 종료 상태를 읽지 못하면 강제 종료를 시도한다.
            Err(e) => {
                tracing::trace!("plugin child try_wait failed: {e}");
                Some(self.force_kill())
            }
        }
    }

    /// 자식이 회수될 때까지 블로킹. 단건 경로용.
    pub fn wait(mut self) -> ShutdownOutcome {
        loop {
            if let Some(outcome) = self.poll() {
                return outcome;
            }
            std::thread::sleep(CHILD_EXIT_POLL_INTERVAL);
        }
    }

    /// 강제 종료와 회수를 시도한다. OS 호출 실패는 trace로 기록한다.
    fn force_kill(&mut self) -> ShutdownOutcome {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.kill() {
                tracing::trace!("plugin child kill failed (already exited?): {e}");
            }
            if let Err(e) = child.wait() {
                tracing::trace!("plugin child wait failed: {e}");
            }
        }
        ShutdownOutcome::Killed
    }
}

impl Drop for PendingShutdown {
    fn drop(&mut self) {
        // 폴링이 끝나기 전에 핸들을 버려도 남은 자식의 종료·회수를 시도한다.
        if self.child.is_some() {
            self.force_kill();
        }
    }
}

/// 여러 자식의 정상 종료 대기를 겹쳐 처리한다. 생성 전에 종료 요청을 모두 보내야 한다.
/// deadline 이후의 kill·wait는 블로킹할 수 있어 전체 소요 시간을 보장하지 않는다.
pub struct ShutdownBatch {
    pending: Vec<PendingShutdown>,
    total: usize,
    started: Instant,
}

/// plugin 한 개의 종료 결과 — 계측 로그 한 줄에 필요한 값 묶음.
pub struct ShutdownReport {
    pub plugin_id: String,
    pub elapsed: Duration,
    pub outcome: ShutdownOutcome,
}

impl ShutdownBatch {
    pub fn new(pending: Vec<PendingShutdown>) -> Self {
        let total = pending.len();
        Self {
            pending,
            total,
            started: Instant::now(),
        }
    }

    /// 대기 시작 시점의 대상 수 (완료된 것 포함).
    pub fn total(&self) -> usize {
        self.total
    }

    /// batch 생성 이후 경과 — 계측의 합계(`S4`)용.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn is_done(&self) -> bool {
        self.pending.is_empty()
    }

    /// 이번 순회에서 종료 처리를 마친 플러그인의 결과를 반환한다.
    pub fn poll(&mut self) -> Vec<ShutdownReport> {
        let mut done = Vec::new();
        self.pending.retain_mut(|p| match p.poll() {
            Some(outcome) => {
                done.push(ShutdownReport {
                    plugin_id: p.plugin_id.clone(),
                    elapsed: p.elapsed(),
                    outcome,
                });
                false
            }
            None => true,
        });
        done
    }

    /// 모든 종료 핸들의 처리가 끝날 때까지 기다린다.
    pub fn wait(&mut self) -> Vec<ShutdownReport> {
        let mut all = Vec::new();
        loop {
            all.extend(self.poll());
            if self.is_done() {
                return all;
            }
            std::thread::sleep(CHILD_EXIT_POLL_INTERVAL);
        }
    }
}

/// `PluginProcess::shutdown` 의 결말 — 종료 계측의 `reason` 필드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// 자식이 deadline 안에 스스로 종료했다.
    Graceful,
    /// 강제 종료·회수 경로를 실행했다. OS 호출 실패는 별도 로그에 남는다.
    Killed,
    /// 회수할 자식 핸들이 없었다 (이미 이관/종료됨).
    NoChild,
}

impl ShutdownOutcome {
    /// 종료 로그용 고정 값. 다른 로그의 reason 필드도 같은 형식이라고 가정하지 않는다.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Graceful => "graceful",
            Self::Killed => "killed",
            Self::NoChild => "no_child",
        }
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.kill() {
                tracing::trace!("plugin child kill on drop failed: {e}");
            }
            if let Err(e) = child.wait() {
                tracing::trace!("plugin child wait on drop failed: {e}");
            }
        }
    }
}

/// 실행 명령과 환경 변수를 준비한다. 보조 채널은 빠른 연결을 받을 수 있도록
/// 자식 실행 전에 mailbox를 등록한다. 두 로그 파일은 stdout/stderr로 넘긴다.
fn build_plugin_command(
    package: &PluginPackage,
    listener: &HostListener,
    handle_listener: Option<&HandleListener>,
    reaper: &crate::reaper::PluginReaper,
    token: &str,
    log_file: std::fs::File,
    log_clone: std::fs::File,
) -> io::Result<(Command, Option<mpsc::Receiver<HandleStream>>)> {
    let mut cmd = launch::command(package)?;
    // Windows 에서 콘솔 서브시스템 플러그인 바이너리를 콘솔 없이 실행한다. GUI 서브시스템 호스트에서는
    // 빈 콘솔 창이 뜨지 않고, 콘솔 서브시스템 헤드리스 호스트에서도 자식이 호스트 콘솔을 함께 쓰지 않는다.
    // 자식 stdio 는 로그 파일로 보낸다. 비-Windows 에서는 no-op.
    tasty_utils::process::hide_console(&mut cmd);
    // 플러그인 수명을 호스트에 결박: spawn *전* 준비(Linux PDEATHSIG pre_exec /
    // macOS TASTY_HOST_PID env 주입). Windows assign 은 spawn *후*(adopt).
    reaper.prepare(&mut cmd);
    inject_locale_env(&mut cmd);
    cmd.args(package.entry_args())
        .env("TASTY_PLUGIN_ID", &package.manifest.id)
        .env("TASTY_HOST_API_VERSION", HOST_API_VERSION)
        .env("TASTY_HOST_IPC_PORT", listener.port().to_string())
        .env("TASTY_PLUGIN_TOKEN", token)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_clone));

    let handle_stream_rx = if let Some(hl) = handle_listener {
        cmd.env("TASTY_PLUGIN_HANDLE_ENDPOINT", hl.endpoint());
        Some(hl.register_token(token))
    } else {
        None
    };
    Ok((cmd, handle_stream_rx))
}

/// 호스트의 언어와 폰트 경로를 자식 환경에 전달한다.
/// 빈 언어는 en으로, 빈 폰트 경로는 환경 변수 제거로 처리한다.
fn inject_locale_env(cmd: &mut Command) {
    set_locale_env(
        cmd,
        std::env::var_os("TASTY_LOCALE"),
        std::env::var_os("TASTY_LOCALE_FONT"),
        std::env::var_os("TASTY_LOCALE_FONT_FACE"),
    );
}

fn set_locale_env(
    cmd: &mut Command,
    locale: Option<OsString>,
    font: Option<OsString>,
    face: Option<OsString>,
) {
    let (locale, font) = locale_env_for_child(locale, font);
    cmd.env("TASTY_LOCALE", locale);
    // 번호는 경로와 한 쌍이다. 경로가 없으면 상속된 번호도 지운다.
    let face = font.as_ref().and(face);
    match font {
        Some(font) => {
            cmd.env("TASTY_LOCALE_FONT", font);
        }
        None => {
            cmd.env_remove("TASTY_LOCALE_FONT");
        }
    }
    match face {
        Some(face) => {
            cmd.env("TASTY_LOCALE_FONT_FACE", face);
        }
        None => {
            cmd.env_remove("TASTY_LOCALE_FONT_FACE");
        }
    }
}

/// host env 의 로케일 값을 자식에 넘길 형태로 정리한다 — `TASTY_LOCALE` 은 항상
/// (미설정 · 빈 값이면 `en`), `TASTY_LOCALE_FONT` 는 비어 있지 않을 때만.
fn locale_env_for_child(
    locale: Option<OsString>,
    font: Option<OsString>,
) -> (OsString, Option<OsString>) {
    let locale = locale
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| OsString::from("en"));
    (locale, font.filter(|v| !v.is_empty()))
}

/// 플러그인별 데이터·설정 경로를 전달하고 필요한 디렉터리 생성을 시도한다.
fn inject_plugin_data_env(
    cmd: &mut Command,
    package: &PluginPackage,
    log_path: &Path,
) -> io::Result<()> {
    let Some(home) = tasty_utils::path::tasty_home() else {
        return Ok(());
    };
    // Resolve in the host before the child changes CWD, as tasty-settings does
    // for shell integration. No canonicalization: the home need not exist yet.
    let home = std::path::absolute(home)?;
    let data_dir = home.join("plugin-data").join(&package.manifest.id);
    let config_path = home
        .join("plugin-config")
        .join(format!("{}.toml", &package.manifest.id));
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        tracing::warn!("plugin data dir {} create failed: {e}", data_dir.display());
    }
    if let Some(parent) = config_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("plugin config dir {} create failed: {e}", parent.display());
    }
    cmd.env("TASTY_PLUGIN_DATA_DIR", &data_dir);
    cmd.env("TASTY_PLUGIN_CONFIG_PATH", &config_path);
    cmd.env("TASTY_PLUGIN_LOG_PATH", log_path);
    // 부모 데이터 루트는 TASTY_PARENT_HOME으로 전달한다.
    // TASTY_HOME에 넣으면 자식이 자기 데이터 루트의 override로 해석한다.
    cmd.env("TASTY_PARENT_HOME", &home);
    Ok(())
}

/// 송신 스레드 — `req_rx` 로 들어오는 줄(이미 직렬화됨)을 NDJSON 한 줄씩 `writer` 에 기록.
/// 꺼내는 순간 그 줄의 바이트가 장부에서 내려간다.
fn spawn_tx_thread(
    plugin_id: &str,
    mut writer: std::net::TcpStream,
    req_rx: MeteredReceiver<String>,
) -> io::Result<()> {
    let plugin_id_tx = plugin_id.to_string();
    std::thread::Builder::new()
        .name(format!("plugin-tx-{}", sanitize_id(&plugin_id_tx)))
        .spawn(move || {
            while let Ok(line) = req_rx.recv() {
                // 본문과 개행을 한 번에 — `write_line` 문서의 전송 지연 이유.
                if tasty_plugin_protocol::write_line(&mut writer, &line).is_err() {
                    break;
                }
            }
        })?;
    Ok(())
}

/// 수신 스레드 — `stream` 에서 한 줄씩 읽어 `handle_incoming_line` 으로 분류.
fn spawn_rx_thread(
    plugin_id: &str,
    stream: std::net::TcpStream,
    waker: tasty_terminal::waker_factory::SharedWakerFactory,
    last_pong: Arc<Mutex<Instant>>,
    resp_tx: MeteredSender<PluginResponse>,
    event_tx: MeteredSender<PluginEvent>,
) -> io::Result<()> {
    let plugin_id_rx = plugin_id.to_string();
    std::thread::Builder::new()
        .name(format!("plugin-rx-{}", sanitize_id(&plugin_id_rx)))
        .spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let trim = line.trim();
                if trim.is_empty() {
                    continue;
                }
                // 바이트 상한에 막혀 서기 전에 호스트를 깨운다 — 자리를 만드는 것은 pump 다.
                let wake = || waker.make_default_waker()();
                handle_incoming_line(trim, &resp_tx, &event_tx, &last_pong, &plugin_id_rx, wake);
                waker.make_default_waker()();
            }
        })?;
    Ok(())
}

/// 큐의 바이트 사용량은 받은 JSON 줄의 길이로 센다. 디코드된 값의 메모리 크기는 아니다.
fn handle_incoming_line(
    line: &str,
    resp_tx: &MeteredSender<PluginResponse>,
    event_tx: &MeteredSender<PluginEvent>,
    last_pong: &Arc<Mutex<Instant>>,
    plugin_id: &str,
    before_wait: impl FnMut(),
) {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("plugin '{plugin_id}' sent unparseable line: {e}");
            return;
        }
    };
    let bytes = line.len() + 1;
    if v.get("id").and_then(|x| x.as_u64()).is_some() {
        handle_incoming_response(v, bytes, resp_tx, last_pong, plugin_id, before_wait);
        return;
    }
    if let Some(ev_value) = v.get("event") {
        handle_incoming_event(ev_value.clone(), bytes, event_tx, plugin_id, before_wait);
    }
}

fn handle_incoming_response(
    v: serde_json::Value,
    bytes: usize,
    resp_tx: &MeteredSender<PluginResponse>,
    last_pong: &Arc<Mutex<Instant>>,
    plugin_id: &str,
    before_wait: impl FnMut(),
) {
    match serde_json::from_value::<PluginResponse>(v) {
        Ok(resp) => {
            *tasty_utils::poison::recover_mutex(
                last_pong.lock(),
                LAST_PONG_WHAT,
                &LAST_PONG_POISONED,
            ) = Instant::now();
            if resp_tx.send_waiting(resp, bytes, before_wait).is_err() {
                tracing::trace!("plugin response forward dropped (consumer exited)");
            }
        }
        Err(e) => {
            tracing::warn!("plugin '{plugin_id}' response decode error: {e}");
        }
    }
}

fn handle_incoming_event(
    ev_value: serde_json::Value,
    bytes: usize,
    event_tx: &MeteredSender<PluginEvent>,
    plugin_id: &str,
    before_wait: impl FnMut(),
) {
    match serde_json::from_value::<PluginEvent>(ev_value) {
        Ok(ev) => {
            if event_tx.send_waiting(ev, bytes, before_wait).is_err() {
                tracing::trace!("plugin event forward dropped (consumer exited)");
            }
        }
        Err(e) => {
            tracing::warn!("plugin '{plugin_id}' event decode error: {e}");
        }
    }
}

/// 보조 채널 reader 스레드의 메시지 처리 루프.
///
/// - `Dirty`: `dirty_rects`에 union(coalesce)해 누적.
/// - `Ping`: 동일 `seq`로 `Pong` 응답.
/// - `Pong`: 호스트는 ping을 보내지 않으므로 무시(트레이스 로그만).
/// - `HandleAttach`: plugin→host로 오는 일은 없어야 함. 받으면 fd 즉시 close 후 경고.
///
/// EOF가 도착하면 (plugin 종료/재시작 또는 정상 shutdown) 조용히 종료.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: 4-arm 평면 메시지
// 메시지별 얕은 dispatch이며 플랫폼별 fd 소유권 처리를 같은 함수에 둔다.
fn aux_reader_loop(
    mut reader: HandleStreamReader,
    dirty: Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>>,
    writer: Arc<Mutex<HandleStream>>,
    plugin_id: String,
) {
    loop {
        match reader.recv_message() {
            Ok((HandleChannelMessage::Dirty { id, rect }, _)) => {
                merge_dirty(&dirty, id, rect);
            }
            Ok((HandleChannelMessage::Ping { seq }, _)) => {
                // 패닉으로 프레임 일부만 쓰였을 수 있으므로 poison이면 전송을 생략하고 알린다.
                match writer.lock() {
                    Ok(mut w) => {
                        if let Err(e) = w.send_message(&HandleChannelMessage::Pong { seq }) {
                            tracing::warn!("plugin '{plugin_id}' aux Pong send failed: {e}");
                        }
                    }
                    Err(_) => tracing::error!(
                        "plugin '{plugin_id}' aux writer lock poisoned — skipping pong; the \
                         plugin will see this channel go quiet"
                    ),
                }
            }
            Ok((HandleChannelMessage::Pong { .. }, _)) => {
                // 호스트가 Ping을 보내지 않으므로 정상 시나리오에서는 도착하지 않는다.
            }
            Ok((HandleChannelMessage::HandleAttach { .. }, aux)) => {
                tracing::warn!("plugin '{plugin_id}' sent unexpected HandleAttach on aux channel");
                #[cfg(unix)]
                if let Some(fd) = aux {
                    // SAFETY: 동행 fd가 있다면 우리가 SCM_RIGHTS로 받은 새 fd 소유권.
                    // 사용처가 없으므로 leak 방지를 위해 close.
                    unsafe { libc::close(fd) };
                }
                // Windows: aux는 in-band HANDLE u64 값일 뿐, 우리 프로세스 핸들 테이블에
                // 복제된 게 아니므로 CloseHandle 대상이 아니다(plugin→host 로 HandleAttach 가
                // 오는 것 자체가 비정상). 그냥 버린다.
                #[cfg(windows)]
                let _ = aux;
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                tracing::warn!("plugin '{plugin_id}' aux channel reader error: {e}");
                break;
            }
        }
    }
}

/// 한 buffer의 dirty 상태를 incoming rect와 union한다. value가 `None`이면 "전체 갱신"
/// sticky flag — 더 이상 좁히지 않는다.
fn merge_dirty(
    map: &Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>>,
    id: SharedBufferId,
    incoming: Option<PixelRect>,
) {
    let mut m =
        tasty_utils::poison::recover_mutex(map.lock(), DIRTY_RECTS_WHAT, &DIRTY_RECTS_POISONED);
    match (m.get(&id).copied(), incoming) {
        (Some(None), _) => {} // 이미 full — 무시.
        (_, None) => {
            m.insert(id, None);
        }
        (None, Some(r)) => {
            m.insert(id, Some(r));
        }
        (Some(Some(existing)), Some(r)) => {
            m.insert(id, Some(union_rect(existing, r)));
        }
    }
}

/// 두 정수 rect의 bounding union. tasty-plugin-protocol의 PixelRect는 (x, y, w, h)이고
/// w/h=0은 invalid 취급이지만 reader는 wire 그대로 union한다 (필터링은 호출자).
fn union_rect(a: PixelRect, b: PixelRect) -> PixelRect {
    let x1 = a.x.min(b.x);
    let y1 = a.y.min(b.y);
    let x2 = a.x.saturating_add(a.w).max(b.x.saturating_add(b.w));
    let y2 = a.y.saturating_add(a.h).max(b.y.saturating_add(b.h));
    PixelRect {
        x: x1,
        y: y1,
        w: x2.saturating_sub(x1),
        h: y2.saturating_sub(y1),
    }
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn generate_token() -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // 시각에서 계산한 토큰이며 암호학적 난수는 아니다.
    let a = (nanos as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let b = ((nanos >> 64) as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_request(id: u64) -> PluginRequest {
        PluginRequest::new("noop", serde_json::json!({}), id)
    }

    /// 포화로 버린 요청 수를 다음으로 큐에 들어가는 요청에 합쳐 보낸다.
    #[test]
    fn the_next_delivered_request_carries_what_saturation_dropped() {
        let (proc, rx) = PluginProcess::stub_with_request_rx_capacity("com.example.slow", 1);
        proc.try_send_request(a_request(1))
            .expect("첫 건은 자리에 들어간다");
        for id in 2..=4 {
            assert_eq!(
                proc.try_send_request(a_request(id)),
                Err(RequestSendError::Full)
            );
        }
        assert_eq!(proc.unreported_drops(), 3);

        let first = rx.try_recv().expect("첫 건은 큐에 있다");
        assert_eq!(first.dropped_requests, 0, "포화 전에 들어간 요청은 0 이다");

        proc.try_send_request(a_request(5))
            .expect("소비했으니 자리가 났다");
        let next = rx.try_recv().expect("다음 건이 들어갔다");
        assert_eq!(next.dropped_requests, 3, "버린 수가 다음 요청에 안 실렸다");
        assert_eq!(proc.unreported_drops(), 0, "실은 만큼 빠져야 한다");
    }

    /// 개행을 포함한 JSON 줄 길이를 집계하고 writer가 꺼낼 때 뺀다.
    #[test]
    fn the_ledger_counts_the_wire_bytes_of_a_queued_request() {
        let ledger = ChannelLedger::new(ChannelLimits::default());
        let (proc, rx) = PluginProcess::stub_with_request_rx_in("com.example.x", 16, &ledger);
        let req = PluginRequest::new("noop", serde_json::json!({ "k": "v" }), 7);
        let wire = serde_json::to_string(&req).unwrap().len() + 1;

        proc.try_send_request(req).unwrap();
        let snap = ledger.snapshot();
        assert_eq!(snap.total_bytes, wire);
        assert_eq!(snap.queues[0].queued_bytes, wire);
        assert_eq!(snap.queues[0].direction, Direction::Request);

        rx.try_recv().unwrap();
        assert_eq!(ledger.snapshot().total_bytes, 0, "꺼낸 줄이 장부에 남았다");
    }

    /// 바이트 상한으로 버린 요청도 플러그인에 알리며 로그에서는 개수 포화와 구분한다.
    #[test]
    fn a_request_over_the_byte_budget_is_dropped_and_reported_like_a_full_queue() {
        let ledger = ChannelLedger::new(ChannelLimits {
            queue_bytes: 64,
            total_bytes: 1 << 20,
        });
        let (proc, rx) = PluginProcess::stub_with_request_rx_in("com.example.big", 16, &ledger);
        let big = PluginRequest::new("noop", serde_json::json!({ "pad": "x".repeat(200) }), 1);
        proc.try_send_request(big)
            .expect("빈 큐는 상한보다 큰 한 건을 받는다");
        let refused = proc.try_send_request(a_request(2));
        assert_eq!(refused, Err(RequestSendError::OverBytes(Refusal::Queue)));
        assert_ne!(
            refused.unwrap_err().to_string(),
            RequestSendError::Full.to_string(),
            "바이트 포화와 개수 포화가 로그에서 안 갈린다"
        );
        assert_eq!(proc.unreported_drops(), 1);

        rx.try_recv().unwrap();
        proc.try_send_request(a_request(3)).unwrap();
        let next = rx.try_recv().unwrap();
        assert_eq!(
            next.dropped_requests, 1,
            "바이트로 버린 수가 다음 요청에 안 실렸다"
        );
    }

    /// 다른 플러그인이 전체 바이트 상한을 채워도 ping·shutdown은 보낼 수 있어야 한다.
    #[test]
    fn control_requests_pass_a_total_filled_by_another_plugin() {
        let ledger = ChannelLedger::new(ChannelLimits {
            queue_bytes: 1 << 20,
            total_bytes: 400,
        });
        let (hog, _hog_rx) = PluginProcess::stub_with_request_rx_in("com.example.hog", 16, &ledger);
        hog.try_send_request(PluginRequest::new(
            "noop",
            serde_json::json!({ "pad": "x".repeat(500) }),
            1,
        ))
        .expect("빈 큐는 한 건을 받는다");
        let before = ledger.snapshot().total_bytes;
        assert!(before > 400, "합계가 안 찼다");

        let (proc, rx) = PluginProcess::stub_with_request_rx_in("com.example.ok", 16, &ledger);
        proc.try_send_request(a_request(1))
            .expect("빈 큐는 한 건을 받는다");
        assert_eq!(
            proc.try_send_request(a_request(2)),
            Err(RequestSendError::OverBytes(Refusal::Total)),
            "전제: 비어 있지 않은 큐의 일반 요청은 합계로 거절된다"
        );

        proc.ping(3);
        let _pending = proc.begin_shutdown(Instant::now());
        // 꺼내기 **전에** 잰다 — 꺼내면 그 몫이 풀린다.
        let after = ledger.snapshot().total_bytes;
        let received: Vec<PluginRequest> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let methods: Vec<&str> = received.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(
            methods,
            ["noop", "ping", "shutdown"],
            "합계가 찬 동안 제어 요청이 거절됐다"
        );
        assert_eq!(ledger.snapshot().refused_over_total, 1);

        // 상한 판정을 면제한 제어 요청도 바이트 합계에는 넣고 꺼낼 때 뺀다.
        let wire: usize = received
            .iter()
            .map(|r| serde_json::to_string(r).unwrap().len() + 1)
            .sum();
        assert_eq!(
            after - before,
            wire,
            "들어간 세 줄(noop · ping · shutdown)의 wire 바이트만큼 합계가 늘지 않았다"
        );
    }

    /// 제어 요청도 개별 큐의 상한은 적용받는다.
    #[test]
    fn control_requests_still_obey_the_queue_byte_limit() {
        let ledger = ChannelLedger::new(ChannelLimits {
            queue_bytes: 64,
            total_bytes: 1 << 20,
        });
        let (proc, _rx) = PluginProcess::stub_with_request_rx_in("com.example.full", 16, &ledger);
        proc.try_send_request(PluginRequest::new(
            "noop",
            serde_json::json!({ "pad": "x".repeat(200) }),
            1,
        ))
        .expect("빈 큐는 한 건을 받는다");
        for (method, id) in [("ping", 2), ("shutdown", u64::MAX)] {
            assert_eq!(
                proc.try_send_request_as(
                    PluginRequest::new(method, serde_json::json!({}), id),
                    Admission::Control,
                ),
                Err(RequestSendError::OverBytes(Refusal::Queue)),
                "큐 상한을 넘은 큐에 제어 요청 '{method}' 이 들어갔다"
            );
        }
        assert_eq!(ledger.snapshot().refused_over_queue, 2);
    }

    /// 포화 송신은 별도 스레드에서 시도하고 제한 시간 안에 반환하는지 확인한다.
    /// blocking send로 바뀌어도 시험 전체가 멈추지 않도록 한다.
    fn verdict_off_thread(
        tx: &mpsc::SyncSender<PluginRequest>,
        id: u64,
    ) -> Result<(), RequestSendError> {
        let tx = tx.clone();
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            // 의도적 무시: timeout으로 시험이 실패한 뒤 수신단이 사라졌으면 회신할 필요가 없다.
            let _ = done_tx.send(try_send_request(&tx, a_request(id)));
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("포화 송신이 제한 시간 안에 반환하지 않았다")
    }

    // 큐가 차면 기다리지 않고 거절해야 한다.
    #[test]
    fn a_full_queue_is_refused_instead_of_awaited() {
        let (tx, _rx) = mpsc::sync_channel::<PluginRequest>(1);
        try_send_request(&tx, a_request(1)).expect("첫 건은 자리에 들어간다");
        assert_eq!(verdict_off_thread(&tx, 2), Err(RequestSendError::Full));
    }

    // 일시적인 큐 포화와 수신단 종료를 다른 오류로 알려야 한다.
    #[test]
    fn saturation_is_told_apart_from_a_dead_writer() {
        let (tx, rx) = mpsc::sync_channel::<PluginRequest>(1);
        try_send_request(&tx, a_request(1)).unwrap();
        assert_eq!(verdict_off_thread(&tx, 2), Err(RequestSendError::Full));

        drop(rx);
        assert_eq!(
            try_send_request(&tx, a_request(3)),
            Err(RequestSendError::Disconnected)
        );
    }

    // 선언한 용량까지 받아들이고 그다음 요청은 거절해야 한다.
    #[test]
    fn the_request_queue_holds_exactly_its_capacity() {
        // 용량 0인 rendezvous 채널은 첫 요청도 보관하지 못하므로 제외한다.
        const { assert!(REQUEST_QUEUE_CAPACITY > 0) };
        let (tx, _rx) = mpsc::sync_channel::<PluginRequest>(REQUEST_QUEUE_CAPACITY);
        for i in 0..REQUEST_QUEUE_CAPACITY {
            try_send_request(&tx, a_request(i as u64))
                .unwrap_or_else(|e| panic!("{i} 번째가 용량 안인데 거절됐다: {e}"));
        }
        assert_eq!(
            verdict_off_thread(&tx, u64::MAX),
            Err(RequestSendError::Full),
            "용량을 넘겨도 계속 받는다 — 상한이 안 걸렸다"
        );
    }

    // 플러그인 응답·이벤트는 큐가 차도 버리지 않고 reader 스레드에서 기다린다.
    // 실제 수신 처리 함수를 거쳐 순서와 개수를 확인한다.
    // 큐 용량보다 많은 메시지를 보내 포화가 일어날 가능성을 높인다.
    // 다만 스레드 실행 순서를 강제하지 않으므로 매번 포화를 보장하는 시험은 아니다.
    const OVERFLOW_ROUNDS: u64 = 1000;

    /// 메시지 개수 제한을 시험할 때 바이트 제한에 먼저 걸리지 않도록 여유를 둔다.
    fn roomy_queue<T>(direction: Direction) -> (MeteredSender<T>, MeteredReceiver<T>) {
        let ledger = ChannelLedger::new(ChannelLimits::default());
        metered_channel(1, ledger.open_queue("com.example.x", direction))
    }

    #[test]
    fn responses_past_the_capacity_are_delayed_not_dropped() {
        let (tx, rx) = roomy_queue::<PluginResponse>(Direction::Response);
        let last_pong = Arc::new(Mutex::new(Instant::now()));
        let writer = std::thread::spawn(move || {
            for id in 0..OVERFLOW_ROUNDS {
                handle_incoming_response(
                    serde_json::json!({ "id": id, "result": {} }),
                    16,
                    &tx,
                    &last_pong,
                    "com.example.x",
                    || {},
                );
            }
        });

        for expected in 0..OVERFLOW_ROUNDS {
            let got = rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|e| panic!("{expected} 번째 응답이 안 왔다({e}) — 버려졌다"));
            assert_eq!(got.id, expected, "응답이 빠져 순번이 밀렸다");
        }
        writer.join().unwrap();
    }

    #[test]
    fn events_past_the_capacity_are_delayed_not_dropped() {
        let (tx, rx) = roomy_queue::<PluginEvent>(Direction::Event);
        let writer = std::thread::spawn(move || {
            for id in 0..OVERFLOW_ROUNDS {
                handle_incoming_event(
                    serde_json::json!({ "kind": "surface_invalidated", "surface_id": id }),
                    16,
                    &tx,
                    "com.example.x",
                    || {},
                );
            }
        });

        for expected in 0..OVERFLOW_ROUNDS {
            let ev = rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|e| panic!("{expected} 번째 이벤트가 안 왔다({e}) — 버려졌다"));
            match ev {
                PluginEvent::SurfaceInvalidated { surface_id } => {
                    assert_eq!(
                        u64::from(surface_id),
                        expected,
                        "이벤트가 빠져 순번이 밀렸다"
                    );
                }
                other => panic!("예상 밖 이벤트: {other:?}"),
            }
        }
        writer.join().unwrap();
    }

    /// 종료 로그의 reason이 서로 다르고 소문자·숫자·밑줄 형식을 따르는지 확인한다.
    #[test]
    fn shutdown_reasons_are_distinct_bare_lowercase_tokens() {
        let all = [
            ShutdownOutcome::Graceful,
            ShutdownOutcome::Killed,
            ShutdownOutcome::NoChild,
        ];
        let mut seen: Vec<&str> = Vec::new();
        for o in all {
            let v = o.as_str();
            assert!(!v.is_empty(), "빈 표기: {o:?}");
            assert!(
                v.starts_with(|c: char| c.is_ascii_lowercase()),
                "소문자로 시작해야 한다: {v}"
            );
            assert!(
                v.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "소문자·숫자·밑줄만 허용한다: {v}"
            );
            assert!(
                !seen.contains(&v),
                "다른 종료 결과는 다른 값이어야 한다: {v}"
            );
            seen.push(v);
        }
    }

    #[test]
    fn token_is_32_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn locale_env_falls_back_to_en_and_drops_empty_font() {
        assert_eq!(
            locale_env_for_child(None, None),
            (OsString::from("en"), None)
        );
        assert_eq!(
            locale_env_for_child(Some(OsString::new()), Some(OsString::new())),
            (OsString::from("en"), None)
        );
    }

    #[test]
    fn locale_env_propagates_host_values() {
        assert_eq!(
            locale_env_for_child(
                Some(OsString::from("ko")),
                Some(OsString::from("/x/lang/ko/fonts/a.ttf"))
            ),
            (
                OsString::from("ko"),
                Some(OsString::from("/x/lang/ko/fonts/a.ttf"))
            )
        );
    }

    /// 명령에 적힌 값. 바깥 `None`은 손대지 않아 상속되는 것이고 안쪽 `None`은 지운 것이다.
    fn locale_env_in(cmd: &Command, name: &str) -> Option<Option<OsString>> {
        cmd.get_envs()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.map(OsString::from))
    }

    #[test]
    fn the_font_face_number_reaches_the_child_with_its_path() {
        let mut cmd = Command::new("plugin");
        set_locale_env(
            &mut cmd,
            Some(OsString::from("zh-Hans")),
            Some(OsString::from("/x/noto.ttc")),
            Some(OsString::from("2")),
        );
        assert_eq!(
            locale_env_in(&cmd, "TASTY_LOCALE_FONT"),
            Some(Some(OsString::from("/x/noto.ttc")))
        );
        assert_eq!(
            locale_env_in(&cmd, "TASTY_LOCALE_FONT_FACE"),
            Some(Some(OsString::from("2")))
        );
    }

    #[test]
    fn no_font_path_also_removes_an_inherited_face_number() {
        let mut cmd = Command::new("plugin");
        set_locale_env(&mut cmd, None, None, Some(OsString::from("2")));
        assert_eq!(locale_env_in(&cmd, "TASTY_LOCALE_FONT"), Some(None));
        assert_eq!(locale_env_in(&cmd, "TASTY_LOCALE_FONT_FACE"), Some(None));
    }

    #[test]
    fn a_font_path_without_a_number_removes_an_inherited_face_number() {
        let mut cmd = Command::new("plugin");
        set_locale_env(&mut cmd, None, Some(OsString::from("/x/a.ttf")), None);
        assert_eq!(locale_env_in(&cmd, "TASTY_LOCALE_FONT_FACE"), Some(None));
    }

    #[test]
    fn sanitize_strips_special() {
        assert_eq!(sanitize_id("com.foo/bar:baz"), "com.foo_bar_baz");
        assert_eq!(sanitize_id("com.example-x"), "com.example-x");
    }

    #[test]
    fn union_rect_combines_bbox() {
        let a = PixelRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let b = PixelRect {
            x: 5,
            y: 5,
            w: 10,
            h: 10,
        };
        let u = union_rect(a, b);
        assert_eq!(
            u,
            PixelRect {
                x: 0,
                y: 0,
                w: 15,
                h: 15
            }
        );
    }

    #[test]
    fn union_rect_disjoint_gives_outer_bbox() {
        let a = PixelRect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let b = PixelRect {
            x: 10,
            y: 10,
            w: 5,
            h: 5,
        };
        let u = union_rect(a, b);
        assert_eq!(
            u,
            PixelRect {
                x: 0,
                y: 0,
                w: 15,
                h: 15
            }
        );
    }

    #[test]
    fn merge_dirty_full_is_sticky() {
        let map: Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let id = SharedBufferId(1);
        merge_dirty(&map, id, None);
        // 이후 Some이 와도 None 유지.
        merge_dirty(
            &map,
            id,
            Some(PixelRect {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            }),
        );
        assert_eq!(map.lock().unwrap().get(&id).copied(), Some(None));
    }

    #[test]
    fn merge_dirty_some_unions_with_existing() {
        let map: Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let id = SharedBufferId(2);
        merge_dirty(
            &map,
            id,
            Some(PixelRect {
                x: 0,
                y: 0,
                w: 5,
                h: 5,
            }),
        );
        merge_dirty(
            &map,
            id,
            Some(PixelRect {
                x: 10,
                y: 10,
                w: 5,
                h: 5,
            }),
        );
        let got = map.lock().unwrap().get(&id).copied().flatten();
        assert_eq!(
            got,
            Some(PixelRect {
                x: 0,
                y: 0,
                w: 15,
                h: 15
            })
        );
    }

    #[test]
    fn merge_dirty_some_then_full_becomes_full() {
        let map: Arc<Mutex<HashMap<SharedBufferId, Option<PixelRect>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let id = SharedBufferId(3);
        merge_dirty(
            &map,
            id,
            Some(PixelRect {
                x: 0,
                y: 0,
                w: 5,
                h: 5,
            }),
        );
        merge_dirty(&map, id, None);
        assert_eq!(map.lock().unwrap().get(&id).copied(), Some(None));
    }
}

/// 종료 대기 겹침 검증. 실제 plugin SDK 없이 "종료가 늦는 자식" 을 직접
/// spawn 해서, 대기가 직렬이 아니라 겹치는지를 시간으로 관측한다.
#[cfg(test)]
#[cfg(any(unix, windows))]
mod shutdown_tests {
    use super::*;

    /// `long=true` 면 deadline 을 넘길 만큼 오래 사는 자식, false 면 즉시 종료.
    /// tasty 는 Unix/Windows 만 지원하므로 두 분기로 충분하다.
    fn spawn_test_child(long: bool) -> Child {
        #[cfg(unix)]
        {
            Command::new("sleep")
                .arg(if long { "30" } else { "0" })
                .spawn()
                .expect("test child spawn")
        }
        #[cfg(windows)]
        {
            Command::new("cmd")
                .args([
                    "/C",
                    if long {
                        "ping -n 30 127.0.0.1 > nul"
                    } else {
                        "exit 0"
                    },
                ])
                .spawn()
                .expect("test child spawn")
        }
    }

    fn pending(plugin_id: &str, child: Child, deadline: Instant) -> PendingShutdown {
        PendingShutdown {
            plugin_id: plugin_id.to_string(),
            child: Some(child),
            deadline,
            started: Instant::now(),
        }
    }

    /// 응답 없는 자식 3개의 deadline 이 겹쳐야 한다 — 직렬이면 3×300ms 이상
    /// 걸린다. 개별 타임아웃 의미론(각자 deadline 까지 기다린 뒤 force kill)은
    /// `reason = killed` 로 유지되는지 함께 본다.
    #[test]
    fn shutdown_batch_waits_concurrently() {
        let children: Vec<Child> = (0..3).map(|_| spawn_test_child(true)).collect();
        let deadline = Instant::now() + Duration::from_millis(300);
        let handles = children
            .into_iter()
            .enumerate()
            .map(|(i, c)| pending(&format!("slow-{i}"), c, deadline))
            .collect();

        let mut batch = ShutdownBatch::new(handles);
        assert_eq!(batch.total(), 3);

        let t = Instant::now();
        let reports = batch.wait();
        let elapsed = t.elapsed();

        assert!(batch.is_done());
        assert_eq!(reports.len(), 3);
        assert!(
            reports.iter().all(|r| r.outcome == ShutdownOutcome::Killed),
            "deadline 을 넘긴 자식은 force kill 이어야 한다"
        );
        assert!(
            elapsed < Duration::from_millis(700),
            "대기가 겹치지 않았다 (직렬이면 900ms 이상): {elapsed:?}"
        );
    }

    /// 스스로 종료하는 자식은 deadline 을 소모하지 않고 graceful 로 빠진다.
    #[test]
    fn shutdown_batch_reports_graceful_exit() {
        let children: Vec<Child> = (0..2).map(|_| spawn_test_child(false)).collect();
        let deadline = Instant::now() + Duration::from_secs(2);
        let handles = children
            .into_iter()
            .enumerate()
            .map(|(i, c)| pending(&format!("quick-{i}"), c, deadline))
            .collect();

        let mut batch = ShutdownBatch::new(handles);
        let t = Instant::now();
        let reports = batch.wait();

        assert_eq!(reports.len(), 2);
        assert!(
            reports
                .iter()
                .all(|r| r.outcome == ShutdownOutcome::Graceful),
            "스스로 종료한 자식은 graceful 이어야 한다"
        );
        assert!(
            t.elapsed() < Duration::from_secs(1),
            "graceful 종료가 deadline 을 기다렸다: {:?}",
            t.elapsed()
        );
    }

    /// 폴링을 끝내지 않고 핸들을 버려도 자식은 회수돼야 한다 (좀비 방지).
    #[test]
    fn dropping_pending_shutdown_reaps_child() {
        let child = spawn_test_child(true);
        #[cfg(unix)]
        let pid = child.id();
        let handle = pending("dropped", child, Instant::now() + Duration::from_secs(60));
        drop(handle);

        // kill + wait 이 Drop 안에서 끝나므로, 반환 시점에 자식은 이미 회수됐다.
        #[cfg(unix)]
        {
            // 이미 wait 했으므로 같은 pid 로 다시 신호를 보내면 실패해야 한다
            // (좀비로 남아 있다면 신호가 성공한다).
            // SAFETY: signal 0 은 프로세스를 건드리지 않고 존재 여부만 조회하는
            // POSIX 표준 용법이다. 인자는 정수 두 개뿐이라 포인터 유효성 요건이 없다.
            let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
            assert!(!alive, "자식 {pid} 이 회수되지 않았다");
        }
    }
}

#[cfg(test)]
mod tests_parent_home;

pub mod channel_bytes;
pub(crate) mod connect;
mod launch;
