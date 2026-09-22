//! Plugin 자식 프로세스 + 호스트와의 양방향 채널.
//!
//! `PluginProcess::spawn(...)`는:
//! 1. 토큰 생성
//! 2. 자식 프로세스 spawn (env로 host port + token + plugin id 전달, stdout/stderr는 로그 파일)
//! 3. listener에서 token 매칭된 connection 수신 (timeout 10s)
//! 4. 송신/수신 스레드 가동 → mpsc 채널로 호스트 메인 루프에 노출
//!
//! plugin이 응답할 때마다 `last_pong`이 갱신된다. 헬스체크는 `since_last_pong()` 비교.

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
#[cfg(test)]
use channel_bytes::ChannelLimits;
use channel_bytes::{
    Admission, ChannelLedger, Direction, MeteredReceiver, MeteredSender, Refusal, TrySendRefusal,
    metered_channel,
};
use tasty_plugin_manifest::{HOST_API_VERSION, PluginPackage};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

// plugin 프로세스 상태를 지키는 내부 락들의 poison 보고 플래그(각 첫 1 회만).
//
// 셋 다 임계구역이 자료구조/값 조작뿐이라(타임스탬프 · 상태 enum · dirty 맵) 락을 든 채
// 죽은 스레드가 불변식을 깨지 않는다 — 복구가 맞다. 조용히 삼키면: pong 갱신이 유실돼
// plugin 이 죽은 것으로 오판돼 kill 되고(last_pong), aux 채널이 있는데 없는 것으로
// 보이며(handle_state), 프레임 dirty 영역이 통째로 사라진다(dirty_rects). 자유 함수에서도
// 쓰므로 모듈 static 으로 둔다.
static LAST_PONG_POISONED: AtomicBool = AtomicBool::new(false);
const LAST_PONG_WHAT: &str = "plugin last-pong timestamp";
static HANDLE_STATE_POISONED: AtomicBool = AtomicBool::new(false);
const HANDLE_STATE_WHAT: &str = "plugin aux handle stream state";
static DIRTY_RECTS_POISONED: AtomicBool = AtomicBool::new(false);
const DIRTY_RECTS_WHAT: &str = "plugin dirty-rects map";

// HandleStream 락은 위 셋과 반대다 — 임계구역이 소켓에 프레임을 쓰므로 락을 든 채 죽은
// 스레드가 반쪽 프레임을 남길 수 있다. poison 을 `into_inner` 로 복구해 이어 쓰면 프레이밍이
// 깨진다(= `writer` 가 FORBIDDEN_LOCKS 에 있는 것과 같은 이유). 그래서 복구하지 않고 이
// 연산을 건너뛰되, 조용히는 아니다 — 첫 1 회 보고한다(aux_reader_loop 의 pong 과 같은 판단).
static WITH_HANDLE_STREAM_POISONED: AtomicBool = AtomicBool::new(false);

/// 보조 채널 stream을 mailbox에서 가져올 때 첫 호출 한도. plugin SDK가 HandleClient::connect
/// 완료 → 호스트 accept thread가 stream을 우편함에 채울 때까지 ms 단위 정도면 충분하지만,
/// startup 직후 호출 가능성을 고려해 500ms 여유.
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
    /// 보조 채널 미지원 (handle_listener bind 실패 / Windows stub) 또는 reader
    /// 분리 실패. 향후 호출이 항상 None을 반환하도록 sticky.
    Unavailable,
}

/// 호스트↔plugin 세 채널의 용량.
///
/// 셋 다 무제한 `mpsc::channel` 이었다. 무제한 큐는 소비자가 멈추면 생산자의 속도만큼
/// 메모리를 먹고, 그 자리가 셋이라 한 plugin 이 멈추면 세 방향으로 자란다. 여기서
/// 고치는 것은 **유한하게 만드는 것**이고 조이는 것이 아니다.
///
/// 이 수들은 **파생이 아니다.** 프로토콜에 한 프레임당 메시지 수의 상한이 없고
/// (`pending_requests` 도 `HashMap` 이라 상한이 없다) 관측된 분포도 없다. 고른 근거는
/// 두 가지뿐이다 — (1) 호스트 pump 는 매 프레임 세 큐를 **끝까지** 비우므로 한 프레임
/// 분량의 버스트를 여러 번 담을 수 있으면 정상 사용은 절대 상한에 안 닿는다,
/// (2) 셋의 곱이 작다: 채널 3 × 1024 × 번들 plugin 9 = 27,648 개의 메시지 슬롯이고
/// 메시지 하나가 수 KB 라도 수십 MB 다. 상한에 닿는지 재는 법은 ADR-0315 에 있다.
pub(crate) const REQUEST_QUEUE_CAPACITY: usize = 1024;
/// plugin → 호스트 응답 큐 용량. 근거는 [`REQUEST_QUEUE_CAPACITY`] 와 같다.
pub(crate) const RESPONSE_QUEUE_CAPACITY: usize = 1024;
/// plugin → 호스트 이벤트 큐 용량. 근거는 [`REQUEST_QUEUE_CAPACITY`] 와 같다.
pub(crate) const EVENT_QUEUE_CAPACITY: usize = 1024;

/// 호스트 → plugin 요청을 큐에 못 넣은 이유.
///
/// 무제한 채널일 때는 실패 이유가 하나뿐이었다(수신단 소멸). 유한해지면서 **포화**가
/// 생겼고, 둘은 성질이 다르다 — 소멸은 영구이고 포화는 일시적이다. 호출부가 로그에서
/// 둘을 가를 수 있어야 "plugin 이 죽었다" 와 "plugin 이 밀리고 있다" 를 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSendError {
    /// 큐가 찼다 — writer 스레드가 소켓에 못 밀어 넣고 있다. 요청은 버려진다.
    Full,
    /// 바이트 상한 — 이 큐의 누적이나 모든 plugin 채널의 합계가 넘친다
    /// ([`channel_bytes`], ADR-0360). 요청은 버려진다. 개수 포화(`Full`)와 가르는 이유는
    /// 처방이 다르기 때문이다 — 개수는 plugin 이 안 읽는 것이고, 합계는 **다른** plugin
    /// 이 자리를 먹은 것일 수 있다.
    OverBytes(Refusal),
    /// writer 스레드가 끝났다 — 프로세스 종료 또는 소켓 끊김.
    Disconnected,
    /// 요청을 줄로 직렬화하지 못했다. `serde_json::Value` 를 담은 요청이라 실제로는 안
    /// 나지만, 났을 때 "끊겼다" 로 보고하면 거짓이 된다.
    Encode,
}

impl std::fmt::Display for RequestSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(
                f,
                "request queue full ({REQUEST_QUEUE_CAPACITY}) — plugin is not draining"
            ),
            Self::OverBytes(Refusal::Queue) => write!(
                f,
                "request queue over its byte budget — plugin is not draining"
            ),
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

/// 요청 한 건을 큐에 넣되 **절대 블록하지 않는다.**
///
/// 블록하지 않는 것이 정책의 핵심이다. 이 함수를 부르는 12 자리는 거의 전부 호스트
/// main thread 의 pump 안이고(`PluginManager::pump` → `apply_collected_events` ·
/// `drain_host_cmds`), 거기서 블록하면 큐가 찼다는 이유로 **프레임이 통째로 멈춘다.**
/// 그래서 포화는 대기가 아니라 거절로 처리하고, 거절은 호출부가 이미 들고 있던
/// "보내기 실패" 갈래로 흘려보낸다.
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
    pub plugin_id: String,
    child: Option<Child>,
    /// 비공개인 것이 정책 강제의 전부다 — `pub` 이면 형제 모듈이 `.send()` 로
    /// 블로킹 송신을 되살릴 수 있고, 그 자리는 컴파일러가 안 잡는다. 송신은
    /// [`PluginProcess::try_send_request`] 하나로만 들어간다.
    ///
    /// 싣는 것은 요청이 아니라 **이미 직렬화한 줄**이다. 바이트 상한은 넣기 전에 크기를
    /// 알아야 판정할 수 있고, 크기를 알려면 직렬화해야 한다 — 그래서 직렬화를 writer
    /// 스레드에서 송신 자리로 옮겼다. 두 번 직렬화하지 않으려고 그 결과를 그대로 싣는다.
    req_tx: MeteredSender<String>,
    /// 포화로 **버린** 요청 수 가운데 아직 plugin 에게 안 알린 몫. 다음으로 큐에
    /// 실제로 들어가는 요청이 이 값을 싣고 그만큼 뺀다
    /// ([`PluginRequest::dropped_requests`]). 송신은 호스트 main thread 한 곳에서만
    /// 일어나지만(pump), 이 필드는 `&self` 메서드에서 갱신되므로 원자값이다.
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
}

#[cfg(test)]
impl PluginProcess {
    /// 송신 큐의 수신단을 살려 둔 stub — 호스트가 plugin 에 **무엇을 보냈는지**를
    /// 재는 자리. [`PluginProcess::stub_for_test`] 는 rx 를 즉시 버려서 모든 송신이
    /// `Disconnected` 로 떨어지므로, 보낸 내용을 단정할 수 없다.
    pub(crate) fn stub_with_request_rx(plugin_id: &str) -> (Self, RequestTap) {
        Self::stub_with_request_rx_capacity(plugin_id, REQUEST_QUEUE_CAPACITY)
    }

    /// 위와 같되 큐 용량을 고른다 — 포화를 재려면 1024 건을 쓸 수 없다.
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

    /// 위와 같되 바이트 장부를 고른다 — 바이트 상한을 재려면 작은 상한이 필요하다.
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

    /// 실제 자식을 든 stub — 회수 경로(`manager::retire`)가 자식이 빠질 때까지 무엇을
    /// 하는지 재는 자리. 송신 큐는 [`Self::stub_for_test`] 와 같이 끊겨 있어 shutdown
    /// 요청은 안 닿는다 — 자식은 스스로 끝나거나 deadline 뒤 kill 된다.
    pub(crate) fn stub_with_child(plugin_id: &str, child: Child) -> Self {
        let mut proc = Self::stub_for_test(plugin_id);
        proc.child = Some(child);
        proc
    }

    /// 마지막 pong 을 `by` 만큼 과거로 민다 — 무응답 재시작을 60 초 기다리지 않고 재려고.
    pub(crate) fn backdate_pong_for_test(&self, by: Duration) {
        let mut last = self.last_pong.lock().expect("fresh mutex");
        *last = Instant::now()
            .checked_sub(by)
            .expect("the clock goes back far enough");
    }

    /// 단위 테스트 전용 stub. child/last_pong 등 외부에서 접근 불가능한 필드를
    /// 합리적인 기본값으로 채운다. 송수신 채널은 dangling이라 실제로 사용하면 안 된다.
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
            plugin_id: plugin_id.into(),
            child: None,
            req_tx,
            dropped_requests: AtomicU64::new(0),
            resp_rx,
            event_rx,
            last_pong: Arc::new(Mutex::new(Instant::now())),
            handle_state: Mutex::new(HandleStreamState::Unavailable),
            dirty_rects: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// 시험이 호스트가 **무엇을 보냈는지** 읽는 자리. 큐에는 직렬화된 줄이 들어 있으므로
/// 꺼낼 때 요청으로 되돌린다 — 되돌린 값이 곧 plugin 이 소켓에서 읽을 값이다.
#[cfg(test)]
pub(crate) struct RequestTap(MeteredReceiver<String>);

#[cfg(test)]
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
        // spawn 보다 먼저 — plugin 의 인증이 등록보다 앞서면 거절된다(`register_connection`).
        let expected = listener.register_connection(&token);

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

        // spawn 은 reaper 를 경유한다 — Linux 는 PDEATHSIG 가 fork 한 스레드 수명에
        // 결박되므로(단명 부트 워커에서 직접 spawn 하면 그 스레드 종료 시 plugin
        // 전원 SIGKILL) 영속 spawner 스레드에서 fork 해야 한다. 타 OS 는 직접 spawn.
        let child = reaper.spawn_bound(cmd).map_err(|e| {
            anyhow::anyhow!(
                "failed to spawn plugin '{}' ({}): {}",
                package.manifest.id,
                entry_path.display(),
                e
            )
        })?;

        // spawn 직후 자식이 살아있는 시점에 Job 에 assign(Windows). 실패해도
        // 플러그인 기능은 정상이며 수명 결박만 누락되므로 warn 후 진행한다.
        if let Err(e) = reaper.adopt(&child) {
            tracing::warn!(
                "plugin '{}' lifetime adopt failed — process not bound to host lifetime: {e}",
                package.manifest.id
            );
        }

        let stream = match expected.wait(HANDSHAKE_TIMEOUT) {
            Some(s) => s,
            None => {
                anyhow::bail!(
                    "plugin '{}' did not connect within {}s — log: {}",
                    package.manifest.id,
                    HANDSHAKE_TIMEOUT.as_secs(),
                    log_path.display()
                );
            }
        };

        // 보조 채널은 별도 mailbox로 받는다 — blocking하지 않는다. plugin이 connect하면
        // listener accept thread가 stream을 receiver로 넣어 둠. shared buffer 사용 시점에
        // 비로소 try_recv로 가져온다. plugin이 영영 connect 안 해도 startup 지연 0.

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

        let writer = stream.try_clone()?;
        spawn_tx_thread(&package.manifest.id, writer, req_rx)?;
        spawn_rx_thread(
            &package.manifest.id,
            stream,
            waker,
            last_pong.clone(),
            resp_tx,
            event_tx,
        )?;

        let initial_state = match handle_stream_rx {
            Some(rx) => HandleStreamState::Pending(rx),
            None => HandleStreamState::Unavailable,
        };
        Ok(Self {
            plugin_id: package.manifest.id.clone(),
            child: Some(child),
            req_tx,
            dropped_requests: AtomicU64::new(0),
            resp_rx,
            event_rx,
            last_pong,
            handle_state: Mutex::new(initial_state),
            dirty_rects: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// 보조 핸들 채널 stream을 첫 호출 시 materialize한 뒤 closure로 노출한다.
    ///
    /// 첫 호출은 mailbox에서 짧은 timeout(`HANDLE_STREAM_MATERIALIZE_TIMEOUT`)으로
    /// 대기하며, 성공하면 reader 스레드를 함께 spawn해 dirty 수신을 시작한다.
    /// 이후 호출은 캐시된 stream을 lock한 뒤 closure에 넘긴다. 보조 채널이
    /// 활성화되지 않은 plugin이면 `None`.
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
                         a thread panicked mid-frame and continuing would corrupt framing"
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

    /// 호스트 → plugin 요청을 큐에 넣는다. **블록하지 않는다** — 근거는
    /// [`try_send_request`] 의 doc.
    ///
    /// 포화로 버린 수를 **여기서** 싣는다. 별도 통지를 만들면 그 통지도 같은(찬) 큐를
    /// 써야 해서 자기모순이므로, 다음으로 실제 들어가는 요청에 얹는다
    /// ([`PluginRequest::dropped_requests`]). 실린 만큼만 빼므로, load 와 send 사이에
    /// 늘어난 몫은 그 다음 요청이 싣는다 — 누락도 중복도 없다.
    ///
    /// 바이트 상한도 여기서 판정한다([`channel_bytes`], ADR-0360) — 그래서 직렬화도
    /// 여기서 한다. 바이트로 버린 것도 개수로 버린 것과 같이 plugin 에게 알린다: plugin
    /// 입장에서는 둘 다 "오던 요청이 사라졌다" 다.
    pub fn try_send_request(&self, req: PluginRequest) -> Result<(), RequestSendError> {
        self.try_send_request_as(req, Admission::Data)
    }

    /// [`Self::try_send_request`] 에 갈래를 준 형태. 제어(ping · shutdown)는 합계 상한을
    /// 면제한다 — 근거는 [`Admission`].
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
                    // 송신은 호스트 main thread 한 곳뿐이라(pump) 이 뺄셈이 되감기지
                    // 않는다 — 실린 값 말고는 아무도 안 뺀다.
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

    /// shutdown 요청만 보내고 **대기하지 않고** 즉시 반환한다. 반환된
    /// [`PendingShutdown`] 이 자식 소유권을 가져가므로(`child.take()`), 남은
    /// `PluginProcess` 가 이 자리에서 drop 돼도 [`PluginProcess::drop`] 의 즉시
    /// kill 이 graceful 대기를 앞지르지 않는다.
    ///
    /// 요청 전송과 대기를 분리해 두면 호출자가 여러 plugin 의 요청을 먼저 전부
    /// 뿌린 뒤 대기 구간만 겹칠 수 있다 — 총 소요가 Σ(개별 대기) 가 아니라
    /// max(개별 대기) 로 수렴한다.
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

    /// 요청 전송 + 종료 대기를 한 번에 하는 블로킹 형태 — 단건 경로(plugin
    /// disable / 재시작 / swap)용. 반환 시점에 자식은 회수(exit 관측 또는
    /// kill+wait 완료)돼 있다.
    ///
    /// 반환값은 종료 계측(`S4a plugin_shutdown_one`)의 `reason` 필드용이다 —
    /// 어느 plugin 이 graceful 시간 안에 못 빠졌는지 가리려면 소요 ms 만으로는
    /// 부족하고 사유가 함께 있어야 한다.
    pub fn shutdown(self, timeout: Duration) -> ShutdownOutcome {
        self.begin_shutdown(Instant::now() + timeout).wait()
    }
}

/// 자식 종료를 관측하는 폴링 간격. `try_wait` 는 논블로킹이라 간격이 그대로
/// 관측 해상도가 된다 — 짧게 하면 종료 감지가 빨라지지만 대기 스레드의 busy
/// 비율이 오른다.
pub const CHILD_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// [`PluginProcess::begin_shutdown`] 이 반환하는 종료 대기 핸들.
///
/// shutdown 요청 전송은 이미 끝났고 남은 것은 자식 종료 관측뿐이다. `poll` 은
/// 논블로킹이라 여러 핸들을 번갈아 폴링하면 대기 구간이 서로 겹친다.
pub struct PendingShutdown {
    plugin_id: String,
    /// 아직 회수하지 않은 자식. 종료를 관측했거나 kill 을 마친 순간 `None` 이 된다.
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

    /// 논블로킹 폴링. 자식이 아직 살아 있고 deadline 전이면 `None`.
    /// `Some` 을 한 번 반환한 시점에 자식은 회수 완료 상태다.
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
            // try_wait 자체가 실패하면 더 기다려도 관측할 방법이 없다 — 기존
            // `wait_for_child_exit` 와 같이 즉시 kill 경로로 보낸다.
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

    /// kill 실패는 이미 죽은 프로세스(`ESRCH`)거나 OS 권한 문제이며, 어느 쪽이든
    /// 호스트가 추가로 할 수 있는 일이 없으므로 trace 로만 흔적을 남긴다.
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
        // 폴링을 끝내기 전에 핸들이 버려진 경우에만 남아 있다 — 좀비를 만들지
        // 않기 위해 여기서 회수한다.
        if self.child.is_some() {
            self.force_kill();
        }
    }
}

/// 여러 plugin 의 종료 대기를 **겹쳐서** 진행하는 집합 핸들.
///
/// 생성 시점에 이미 모든 대상에 shutdown 요청이 나가 있어야 한다
/// ([`PluginProcess::begin_shutdown`]). 이후 `poll` 을 반복 호출하면 각 자식의
/// 대기가 서로 독립적으로 진행되므로 전체 소요는 개별 deadline 의 max 로
/// 수렴한다.
///
/// 스레드를 쓰지 않는 것은 의도다 — 호출자가 프레임 루프 안에서 논블로킹으로
/// 돌릴 수 있어야 종료 화면 같은 것을 그리면서 대기할 수 있다.
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

    /// 논블로킹 — 이번 라운드에 종료가 관측된 plugin 들의 결과만 반환한다.
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

    /// 전부 회수될 때까지 블로킹. 반환 시점에 잔존 자식은 없다.
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
    /// deadline 초과 → `child.kill()` 로 강제 종료했다.
    Killed,
    /// 회수할 자식 핸들이 없었다 (이미 이관/종료됨).
    NoChild,
}

impl ShutdownOutcome {
    /// 로그 필드용 표기 — **맨 소문자 토큰**(`[a-z][a-z0-9_]*`)이고 닫힌 집합이다.
    /// 부팅 계측의 `reason = satisfied|deadline` 이 같은 모양을 쓴다.
    ///
    /// ★ 그 짝은 **저장소 전역 관례가 아니다.** 같은 이름의 필드가 여러 곳에 있고 값의
    /// 모양이 서로 다르다 — `agent-stream` 은 `stream:` 을 앞에 붙인 이름공간 토큰
    /// (`turn_end{reason=stream:turn_timeout}`)을 쓰고, `hook-failures.log` 의 `reason`
    /// 은 애초에 **산문**이라 언어까지 갈린다(`docs/adr/0164-…`). 그래서 "reason 은 늘
    /// 맨 토큰" 으로 일반화한 관측자는 그런 자리에서 조용히 0 을 센다.
    ///
    /// ★ 위는 **본보기이지 명부가 아니다** — 수를 안 적는 이유가 그것이다. 갈래를 세어
    /// 적으면 넷째가 생기는 순간 그 수가 조용히 거짓이 되고, 그 수를 지키는 것은 없다.
    ///
    /// 이쪽 절반(값 셋의 모양·구별)은 아래 단정이 잡는다. 반대쪽 절반(부팅의 인라인
    /// 리터럴)은 **아무것도 안 잡는다** — 크레이트가 갈려 부를 수도, 타입으로 묶을 수도
    /// 없어서 이 문장은 주석에 머문다. 부팅 쪽 표기가 바뀌면 여기는 조용히 낡는다.
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

/// `PluginProcess::spawn` 의 `Command` 조립 스텝 — entry/args/필수 env 설정 후
/// 보조 채널 endpoint 를 알려주고 mailbox 를 등록한다. mailbox 등록은 *child
/// spawn 전*에 일어나야 SDK 가 빠르게 connect 해도 accept thread 가 매핑할
/// sender 를 찾을 수 있다. `log_file`/`log_clone` 은 stdout/stderr 로 소비된다.
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
    // Windows GUI 서브시스템 호스트가 콘솔 서브시스템 플러그인 바이너리를
    // spawn 할 때 빈 콘솔 창이 뜨는 것을 막는다 (비-Windows 에서는 no-op).
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

/// 활성 로케일 env 주입. 이 크레이트는 `tasty-i18n` 에 의존하지 않는다 — 활성 언어는
/// host 본 바이너리가 부팅 시(`src/boot/locale.rs`, 스레드 생성 전 단일 스레드 구간)
/// 자기 프로세스 env 에 set 한 `TASTY_LOCALE` / `TASTY_LOCALE_FONT` 를 그대로 자식에
/// propagate 한다. `Command` 는 host env 를 상속하므로 두 값은 명시하지 않아도
/// 흘러가지만, 계약을 코드에 드러내고(host 본 바이너리 밖 — 테스트 · 다른 호스트 — 에서
/// 쓰일 때의 `en` 폴백) 빈 폰트 값이 자식에 남지 않게 여기서 확정한다. 값은 spawn 시점에
/// 고정된다 — 근거 `docs/adr/0103-plugin-locale-via-host-process-env.md`.
fn inject_locale_env(cmd: &mut Command) {
    let (locale, font) = locale_env_for_child(
        std::env::var_os("TASTY_LOCALE"),
        std::env::var_os("TASTY_LOCALE_FONT"),
    );
    cmd.env("TASTY_LOCALE", locale);
    match font {
        Some(font) => {
            cmd.env("TASTY_LOCALE_FONT", font);
        }
        None => {
            cmd.env_remove("TASTY_LOCALE_FONT");
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

/// plugin별 격리 디렉터리 env 주입. 디렉터리 생성은 호스트가 미리 보장한다 —
/// plugin이 fs.write 권한 없이도 자기 영역만은 자유롭게 쓸 수 있도록.
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
    // host 가 부팅 시 확정한 데이터 루트를 자식에 정보성으로 내려준다
    // (completion-log 경로 판별용). **`TASTY_HOME` 이 아니라
    // `TASTY_PARENT_HOME`** 으로 주입한다 — `TASTY_HOME` 은 tasty_home()
    // (self-determination, override 전용)의 1순위라, 정보성 값을 그 이름으로
    // 주입하면 자식이 그걸 자기 데이터 루트 override 로 오인한다(release 안에서
    // debug 실행 시 격리 붕괴). notify_log_path() 가 `TASTY_PARENT_HOME` 을
    // 최우선으로 보므로 writer(plugin)/reader(conductor) 경로는 계속 일치한다.
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

/// `line` 의 길이가 곧 그 메시지가 큐에 들고 있는 바이트다 — 디코드한 값의 크기를 다시
/// 재지 않는다. 받은 줄이 plugin 이 실제로 보낸 양이고, 상한이 묶으려는 것도 그것이다.
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
// dispatch 루프 — HandleAttach arm 의 fd 소유권 정리만 플랫폼별 cfg 분기다.
// `aux: Option<RawFd>` (unix) / `Option<u64>` (windows) 로 타입이 cfg 에 따라
// 달라서 arm 을 별 함수로 뽑으려면 cfg 게이트를 그대로 복제해야 하고, fd
// close 책임 소재가 흐려질 위험이 이득보다 크다 — 이 자리에 두는 편이 더 안전.
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
                // poison 이면 보내지 않는다 — 임계구역이 소켓 쓰기라 반쯤 쓰인 메시지
                // 위에 이어 쓰면 프레이밍이 깨진다(plugin SDK 쪽 pong 과 같은 판단).
                // 다만 **조용히** 건너뛰지 않는다: pong 이 끊기면 상대가 이 채널을 죽은
                // 것으로 보는데, 그 원인이 어디에도 안 남으면 추적이 불가능하다.
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
    // 단순한 의사 랜덤 — 단계 07에서 강화 가능 (rand 크레이트 등).
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

    /// 포화로 버린 수는 **다음으로 실제 큐에 들어가는 요청**이 싣는다.
    ///
    /// 별도 통지 메시지를 만들 수 없다는 것이 요점이다 — 그 통지도 같은 큐를 써야
    /// 하는데 그 큐가 찼기 때문에 통지가 생긴 것이다. 그래서 plugin 이 하나라도
    /// 소비해 자리가 난 순간, 그때까지 버린 수가 합쳐져 실린다.
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

    /// 장부에 오르는 바이트는 **소켓에 나가는 줄의 길이**(개행 포함)다 — 추정이 아니다.
    /// 그리고 writer 가 꺼내면 장부에서 내려간다.
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

    /// 바이트 상한으로 버린 요청도 개수 포화와 **같이** plugin 에게 알린다 — plugin 입장에서
    /// 둘은 같은 사건(오던 요청이 사라졌다)이다. 호출부 로그에서는 둘이 갈린다.
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

    /// 합계가 **다른 plugin** 으로 찬 동안에도 제어(ping · shutdown)는 들어간다 — 같은
    /// 순간 일반 요청은 합계로 거절된다. 면제가 없으면 건강한 plugin 이 남의 포화 때문에
    /// ping 을 못 받아 무응답 재시작되고, shutdown 을 못 받아 graceful 없이 kill 된다.
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

        // 면제는 판정만 건너뛴다 — 들어간 제어 요청도 합계에 **센다.** 안 세면 writer 가
        // 꺼낼 때 올리지 않은 몫을 빼서 다른 큐의 몫을 깎는다(차감이 포화 뺄셈이라 조용하다).
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

    /// 제어가 면제받는 것은 **합계뿐**이다 — 큐 상한은 그대로 받는다. 그 큐를 채운 것은
    /// 그 plugin 자신이고, 안 읽는 plugin 의 ping 이 막혀 무응답으로 재시작되는 것이
    /// healthcheck 의 뜻이다.
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

    /// 자리가 없는 큐에 한 건을 넣어 보고 **그 판정을 다른 스레드에서 받아 온다.**
    ///
    /// 포화 송신을 본 스레드에서 직접 부르면 안 된다. 정책이 블로킹 `send` 로
    /// 되돌아갔을 때 그 호출은 영영 안 돌아오고, 그러면 시험이 **빨개지는 대신
    /// 멈춘다** — 멈춘 시험은 실패보다 나쁘다(스위트 전체가 서고 원인도 안 보인다).
    /// 판정을 timeout 으로 받으면 같은 회귀가 실패 한 줄로 나온다.
    fn verdict_off_thread(
        tx: &mpsc::SyncSender<PluginRequest>,
        id: u64,
    ) -> Result<(), RequestSendError> {
        let tx = tx.clone();
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            // 의도적 무시: 본 스레드가 timeout 으로 이미 포기했으면 수신단이 사라져
            // 이 send 가 실패하는데, 그 경우는 아래 `expect` 가 이미 시험을 빨갛게
            // 만든 뒤다 — 여기서 또 보고할 것이 없다.
            let _ = done_tx.send(try_send_request(&tx, a_request(id)));
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("포화 송신이 안 돌아왔다 — 정책이 기다리고 있다(블로킹)")
    }

    // 큐가 차면 **기다리지 않고 거절한다.** 이 시험의 요점은 반환값만이 아니라
    // **돌아온다는 것** 자체다 — 이 정책을 부르는 12 자리는 거의 전부 호스트 main
    // thread 의 pump 안이고(`PluginManager::pump` → `apply_collected_events` ·
    // `drain_host_cmds`), 거기서 기다리면 큐가 찼다는 이유로 프레임이 통째로 멈춘다.
    #[test]
    fn a_full_queue_is_refused_instead_of_awaited() {
        let (tx, _rx) = mpsc::sync_channel::<PluginRequest>(1);
        try_send_request(&tx, a_request(1)).expect("첫 건은 자리에 들어간다");
        assert_eq!(verdict_off_thread(&tx, 2), Err(RequestSendError::Full));
    }

    // 포화와 소멸은 **다른 사건**이다. 무제한 채널일 때는 실패 이유가 소멸 하나뿐이라
    // 호출부가 가를 필요가 없었는데, 유한해지면서 일시적 실패가 생겼다. 둘이 같은
    // 값으로 뭉개지면 로그에서 "밀리는 중" 과 "죽었다" 를 못 가른다.
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

    // 용량이 실제로 걸려 있다. `sync_channel(0)` 은 rendezvous 라 첫 건부터 거절되고,
    // 무제한으로 되돌리면 이 자리가 컴파일부터 안 된다 — 상수가 **쓰인다**는 것을
    // 그 둘 사이의 값으로 고정한다.
    #[test]
    fn the_request_queue_holds_exactly_its_capacity() {
        // 컴파일 시점에 본다 — 런타임 `assert!` 은 상수 비교라 clippy 가 잡고,
        // 잡히는 쪽이 맞다: 0 이면 rendezvous 라 아래 루프가 한 건도 못 넣는다.
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

    // plugin → 호스트 방향은 **거절이 아니라 대기**다. 응답을 버리면 그 요청이 영영
    // 답을 못 받고(호스트는 deadline 으로만 회수한다 — ADR-0311), 이벤트를 버리면
    // 등록·수명 전이가 조용히 빠진다. 여기 sender 는 reader 스레드 하나뿐이라 블록해도
    // 호스트 프레임이 안 멈추고, 멈추는 것은 소켓 읽기 — 그것이 plugin 에 거는
    // backpressure 다.
    //
    // 재는 자리는 생산 경로 그 자체(`handle_incoming_response`)다. 로컬 채널로
    // `sync_channel` 의 성질을 재면 std 를 재는 것이지 이 코드를 재는 것이 아니다.
    //
    // ★ 모수를 크게 잡는 이유: 용량 1 짜리 큐에 **한 건만** 흘려 보내면 보내는 쪽과
    // 받는 쪽의 순서가 안 정해져서, 버리는 구현(`try_send`)이어도 수신자가 먼저
    // 비워 둔 순간에 걸리면 통과한다 — 실제로 그 형태로 짰다가 변이가 **살아남았다.**
    // 한 건이 아니라 용량의 여러 배를 연속으로 흘리면 버리는 구현은 가득 찬 순간을
    // 반드시 만난다. 이 시험의 방향은 안전하다: 블로킹 구현은 절대 안 잃으므로
    // **거짓 빨강이 없고**, 부하가 어떻든 한쪽으로만 틀릴 수 있다.
    const OVERFLOW_ROUNDS: u64 = 1000;

    /// 개수를 재는 두 시험이 바이트 장부의 판정에 먼저 걸리지 않게 넉넉한 장부를 쓴다 — 이
    /// 시험들이 재는 것은 **개수** 포화의 답이다(바이트 쪽은 `channel_bytes` 의 시험).
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

    /// 종료 계측의 `reason` 값은 **맨 소문자 토큰**이고 서로 구별된다.
    ///
    /// 이 로그는 사람이 아니라 기계가 읽는다 — 값에 공백·대문자·구분자가 섞이면 그것을
    /// 세던 관측자가 **실패가 아니라 0** 을 낸다(안 보인다). 그래서 모양을 단정으로 박는다.
    /// 값 자체는 `docs/architecture/shutdown-sequence.md` 가 인용한다(`reason="killed"`).
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
                "맨 소문자 토큰이어야 한다(공백·대문자·구분자 금지): {v}"
            );
            assert!(!seen.contains(&v), "두 결말이 같은 표기를 쓴다: {v}");
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
mod launch;
