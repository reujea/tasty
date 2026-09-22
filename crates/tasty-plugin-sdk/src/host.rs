//! Plugin → 호스트 동기 IPC 호출 헬퍼.
//!
//! SDK 메시지 루프가 메인(receiver) + worker(dispatcher)로 갈라져 있을 때만
//! 의미가 있다. worker가 [`HostHandle::call`]을 호출하면 IpcCall event가
//! 송신되고, 메인 스레드가 호스트로부터 받은 `ipc.result`를 oneshot channel로
//! 결과를 push한다. worker는 그 결과가 올 때까지 block.

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::Value;
use tasty_plugin_protocol::PluginEvent;
#[cfg(any(unix, windows))]
use tasty_plugin_protocol::{METHOD_HOST_SHARED_BUFFER_CREATE, SharedBufferCreateResult};

use crate::error::PluginError;
use crate::handle_channel::HandleClient;
use crate::shared_buffer::SharedBuffer;

/// 호스트 호출 결과. 워크스페이스 plugin이 `match` 분기에 쓰지 않고 `?`로만
/// 흘려보내는 경우가 대부분이라, 표준 [`PluginError`]로 합쳤다.
pub(crate) type HostCallResult = Result<Value, PluginError>;
pub(crate) type PendingCalls = Arc<Mutex<HashMap<u64, mpsc::Sender<HostCallResult>>>>;

/// 두 pending 맵의 poison 을 각각 첫 1 회만 보고한다 — poison 은 sticky 라 이후 모든
/// 호출이 같은 경로를 탄다.
///
/// 임계구역은 `HashMap` 의 insert/remove 뿐이라 패닉이 나도 불변식이 성립한다. 방침표는
/// plugin 프로세스 하나가 죽는 범위라면 패닉을 허용하지만, **복구가 가능한 자료구조에서
/// 프로세스를 버리는 것은 재시작보다 비싸다** — `runtime.rs` 의 `fd_pending` 도 같은
/// 이유로 복구를 택했다. 소켓 쓰기(writer)만 데이터를 신뢰할 수 없어 패닉을 유지한다.
///
/// 조용히 버리면 `deliver_ipc_result` 가 응답을 배달하지 못해 **이후 모든 host call 이
/// timeout** 이 되고, 정리 경로에서 버리면 죽은 항목이 맵에 누적된다.
static PENDING_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FD_PENDING_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

const PENDING_WHAT: &str = "plugin host call pending map";
const FD_PENDING_WHAT: &str = "plugin shared-buffer fd pending map";

/// `host.shared_buffer.create` RPC와 동일한 call_id로 매칭되는 fd waiter 맵.
/// 보조 채널 reader가 `HandleAttach`를 받으면 여기 등록된 mpsc로 fd를 push한다.
#[cfg(unix)]
pub(crate) type SharedBufferFdPending = Arc<Mutex<HashMap<u64, mpsc::Sender<std::os::fd::RawFd>>>>;
#[cfg(windows)]
pub(crate) type SharedBufferFdPending = Arc<Mutex<HashMap<u64, mpsc::Sender<u64>>>>;

/// 옛 이름 호환을 위한 alias. 신규 코드는 [`PluginError`] 사용.
#[deprecated(note = "PluginError로 통합됨. 새 코드는 PluginError 사용.")]
pub type HostCallError = PluginError;

/// Plugin 코드가 호스트 IPC 메서드를 동기로 호출하는 진입점.
///
/// `Clone` 가능 — plugin이 자기 스레드로 옮겨도 동작한다. 내부의 writer는
/// `Arc<Mutex<TcpStream>>`로 보호되어 동시 호출 안전.
#[derive(Clone)]
pub struct HostHandle {
    writer: Arc<Mutex<TcpStream>>,
    pending: PendingCalls,
    next_call_id: Arc<AtomicU64>,
    /// host call의 최대 대기 시간. 기본 60초. 호출 전에 변경 가능.
    pub timeout: Duration,
    /// 보조 핸들 채널의 writer. 없으면 shared buffer 기능 비활성.
    handle_writer: Option<Arc<Mutex<HandleClient>>>,
    /// `host.shared_buffer.create` RPC에 대한 fd 도착 알림 맵.
    shared_buffer_fd_pending: SharedBufferFdPending,
    /// self-invoke 큐 sender. [`Self::self_invoke`] 문서 참고. `run()`이 worker
    /// 큐를 만든 직후에만 채워지므로, 그 전에 만들어진 `HostHandle`(예: `spawn_handle_reader`
    /// 내부)에는 없다 — `run()`이 `with_self_invoke`로 마지막에 부착한다.
    self_invoke_tx: Option<mpsc::Sender<crate::runtime::WorkerItem>>,
    /// 호스트가 이 plugin 에게 보내려다 큐 포화로 버린 요청의 **누적** 수.
    /// [`PluginRequest::dropped_requests`](tasty_plugin_protocol::PluginRequest::dropped_requests)
    /// 가 실어 온 값을 SDK 가 여기 더한다. `Arc` 라 clone 한 핸들끼리 같은 값을 본다 —
    /// 자체 background thread 에서 부하를 줄일지 판단할 때 읽는다.
    dropped_by_host: Arc<AtomicU64>,
    /// 위 누적분 중 아직 plugin 콜백에 안 넘긴 몫. worker 가 dispatch 직전에 비운다.
    dropped_unreported: Arc<AtomicU64>,
    /// `ipc.result` 를 읽어 줄 메인 recv 루프가 끝났다(shutdown · 연결 종료 · 수신 에러).
    /// 켜진 뒤의 [`Self::call`] 은 기다리지 않고 [`PluginError::HostClosed`] 로 돌아온다 —
    /// [`Self::fail_pending_calls`] 문서 참고.
    host_gone: Arc<AtomicBool>,
}

impl HostHandle {
    pub(crate) fn new(writer: Arc<Mutex<TcpStream>>, pending: PendingCalls) -> Self {
        Self {
            writer,
            pending,
            next_call_id: Arc::new(AtomicU64::new(1)),
            timeout: Duration::from_secs(60),
            handle_writer: None,
            shared_buffer_fd_pending: Arc::new(Mutex::new(HashMap::new())),
            self_invoke_tx: None,
            dropped_by_host: Arc::new(AtomicU64::new(0)),
            dropped_unreported: Arc::new(AtomicU64::new(0)),
            host_gone: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 메인 recv 루프가 끝났을 때 부른다 — 대기 중인 host call 을 전부
    /// [`PluginError::HostClosed`] 로 깨우고, 이후의 call 도 기다리지 않게 한다.
    ///
    /// 루프가 끝나면 `ipc.result` 를 읽을 스레드가 없다. 그런데 worker 는 shutdown 앞에
    /// 쌓인 요청을 계속 처리하므로, 그중 하나가 host 를 부르면 결과가 배달될 길 없이
    /// [`Self::timeout`](기본 60 s)까지 선다. 그 동안 `run` 이 worker join 에서 서고,
    /// 호스트는 shutdown 유예(2 s) 뒤 프로세스를 강제 종료한다. plugin 입장에서 그 call 의
    /// 뜻은 "호스트가 떠났다" 이므로 그 에러로 즉시 돌려준다.
    ///
    /// 순서가 경합을 닫는다: 여기서는 플래그를 먼저 켜고 맵을 비우고, [`Self::call`] 은
    /// 맵에 넣은 **뒤에** 플래그를 본다. 맵 락이 둘을 줄 세우므로, 비우기 전에 넣은 call 은
    /// 여기서 깨고 비운 뒤에 넣은 call 은 자기 확인에서 플래그를 본다.
    pub(crate) fn fail_pending_calls(&self) {
        self.host_gone.store(true, Ordering::SeqCst);
        let drained: Vec<(u64, mpsc::Sender<HostCallResult>)> = tasty_utils::poison::recover_mutex(
            self.pending.lock(),
            PENDING_WHAT,
            &PENDING_POISONED,
        )
        .drain()
        .collect();
        for (call_id, tx) in drained {
            // 호출자가 timeout 으로 이미 rx 를 drop 했을 수 있다 — 깨울 대상이 없다.
            if tx.send(Err(PluginError::HostClosed)).is_err() {
                tracing::trace!("host call#{call_id} already gone when the host left");
            }
        }
    }

    /// 호스트가 이 plugin 에게 보내려다 **버린** 요청의 누적 수.
    ///
    /// 호스트 → plugin 큐는 유한하고 포화 시 거절이라(ADR-0315), 밀리는 plugin 은
    /// 자기에게 오던 요청이 사라지는 것을 원리적으로 알 수 없었다. 이 값이 0 이 아니면
    /// **이 plugin 이 소비를 못 따라가고 있다** — 자체 작업량을 줄이거나(polling 간격,
    /// 렌더 빈도) 사용자에게 알릴 판단 근거다. 단조 증가하고 절대 줄지 않는다.
    pub fn dropped_by_host(&self) -> u64 {
        self.dropped_by_host.load(Ordering::Relaxed)
    }

    /// 호스트 요청이 싣고 온 드롭 수를 누적한다. 0 이면 아무것도 안 한다.
    pub(crate) fn record_dropped_by_host(&self, dropped: u64) {
        if dropped == 0 {
            return;
        }
        self.dropped_by_host.fetch_add(dropped, Ordering::Relaxed);
        self.dropped_unreported
            .fetch_add(dropped, Ordering::Relaxed);
    }

    /// 아직 plugin 콜백에 안 넘긴 몫을 가져오고 0 으로 만든다.
    pub(crate) fn take_unreported_drops(&self) -> u64 {
        self.dropped_unreported.swap(0, Ordering::Relaxed)
    }

    /// 보조 핸들 채널을 등록한다. runtime이 [`HandleClient::connect`] 성공 시 호출.
    pub(crate) fn with_handle_channel(
        mut self,
        handle_writer: Arc<Mutex<HandleClient>>,
        shared_buffer_fd_pending: SharedBufferFdPending,
    ) -> Self {
        self.handle_writer = Some(handle_writer);
        self.shared_buffer_fd_pending = shared_buffer_fd_pending;
        self
    }

    /// self-invoke 큐를 등록한다. `run()`이 worker 채널을 만든 직후 호출.
    pub(crate) fn with_self_invoke(mut self, tx: mpsc::Sender<crate::runtime::WorkerItem>) -> Self {
        self.self_invoke_tx = Some(tx);
        self
    }

    /// plugin 자신의 백그라운드 스레드가 자기 네임스페이스 IPC 메서드(예: 이
    /// plugin이 등록한 `"markdown.reload"`)를 트리거한다.
    ///
    /// [`Self::call`]과 달리 host를 왕복하지 않는다 — 호스트 dispatcher
    /// (`plugin_ipc.rs`)는 caller가 네임스페이스 owner 자신이면 forward하지 않고
    /// host-native dispatch로 통과시키는 trampoline 정책이라, plugin 자신의
    /// 네임스페이스 메서드를 `call()`로 부르면 항상 `-32601 Method not found`로
    /// 실패한다. 이 메서드는 worker 큐에 직접 enqueue해 `&mut plugin`을 쥔 단일
    /// worker 스레드에서 `handle_ipc_method`를 바로 실행시킨다 — read/재생성이
    /// 여전히 같은 함수·같은 스레드로 수렴하므로 레이스가 없다.
    ///
    /// fire-and-forget이다 — host가 요청한 적 없는 call이라 돌려줄 응답 대상이
    /// 없다. 실패는 worker 스레드가 `tracing::warn!`으로 로그만 남긴다.
    pub fn self_invoke(&self, method: impl Into<String>, params: Value) -> Result<(), PluginError> {
        let tx = self
            .self_invoke_tx
            .as_ref()
            .ok_or(PluginError::SelfInvokeUnavailable)?;
        tx.send(crate::runtime::WorkerItem::SelfInvoke {
            method: method.into(),
            params,
        })
        .map_err(|_| PluginError::SelfInvokeUnavailable)
    }

    /// 호스트에 비동기 알림([`PluginEvent`])을 보낸다. 응답을 기다리지 않는다
    /// (fire-and-forget). egui-mesh surface 가 mesh 를 commit 한 뒤
    /// [`PluginEvent::PaintFrame`] 를 보내는 등, plugin 능동 알림 경로에 쓴다.
    pub fn notify(&self, event: &PluginEvent) -> Result<(), PluginError> {
        let payload = serde_json::json!({ "event": event });
        let line = serde_json::to_string(&payload)?;
        let mut w = self
            .writer
            .lock()
            .map_err(|_| PluginError::LockPoisoned("host writer"))?;
        writeln!(*w, "{line}")?;
        w.flush()?;
        Ok(())
    }

    /// 호스트 IPC 메서드를 동기로 호출한다. 응답까지 [`Self::timeout`]만큼 block.
    pub fn call(&self, method: impl Into<String>, params: Value) -> Result<Value, PluginError> {
        let method_str = method.into();
        let call_id = self.next_call_id.fetch_add(1, Ordering::Relaxed);
        self.call_with_id(call_id, method_str, params)
    }

    fn call_with_id(
        &self,
        call_id: u64,
        method: String,
        params: Value,
    ) -> Result<Value, PluginError> {
        let (tx, rx) = mpsc::channel::<HostCallResult>();
        {
            let mut p = self
                .pending
                .lock()
                .map_err(|_| PluginError::LockPoisoned("host pending"))?;
            p.insert(call_id, tx);
        }
        // 넣은 뒤에 본다 — 순서의 이유는 `fail_pending_calls` 문서.
        if self.host_gone.load(Ordering::SeqCst) {
            tasty_utils::poison::recover_mutex(
                self.pending.lock(),
                PENDING_WHAT,
                &PENDING_POISONED,
            )
            .remove(&call_id);
            return Err(PluginError::HostClosed);
        }
        let event = PluginEvent::IpcCall {
            call_id,
            method: method.clone(),
            params,
        };
        let payload = serde_json::json!({ "event": event });
        let line = serde_json::to_string(&payload)?;
        {
            let mut w = self
                .writer
                .lock()
                .map_err(|_| PluginError::LockPoisoned("host writer"))?;
            writeln!(*w, "{line}")?;
            w.flush()?;
        }
        match rx.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(_) => {
                tasty_utils::poison::recover_mutex(
                    self.pending.lock(),
                    PENDING_WHAT,
                    &PENDING_POISONED,
                )
                .remove(&call_id);
                Err(PluginError::HostCallTimeout {
                    method,
                    timeout: self.timeout,
                })
            }
        }
    }

    /// 새 공유 메모리 buffer를 호스트에 요청한다. 메인 채널로 메타데이터 RPC가 가고,
    /// 보조 채널로 fd/HANDLE이 동행해서 도착한다 — 둘 다 모인 시점에 [`SharedBuffer`]를
    /// 반환.
    ///
    /// `size`는 plugin이 실제로 쓸 수 있는 사용자 영역의 크기다. 호스트에는 footer
    /// ([`tasty_shm::footer::SIZE`])를 더한 OS 영역 크기로 요청이 나간다.
    ///
    /// 보조 채널이 활성화되지 않은 plugin이면 [`PluginError::HandleChannelUnavailable`].
    #[cfg(unix)]
    pub fn create_shared_buffer(&self, size: usize) -> Result<SharedBuffer, PluginError> {
        let handle_writer = self
            .handle_writer
            .clone()
            .ok_or(PluginError::HandleChannelUnavailable)?;

        let call_id = self.next_call_id.fetch_add(1, Ordering::Relaxed);

        // fd 도착 알림 채널을 *RPC 송신 전에* 등록 — race 방지.
        let (fd_tx, fd_rx) = mpsc::channel::<std::os::fd::RawFd>();
        {
            let mut m = self
                .shared_buffer_fd_pending
                .lock()
                .map_err(|_| PluginError::LockPoisoned("shared_buffer_fd_pending"))?;
            m.insert(call_id, fd_tx);
        }

        // OS 영역 = footer + user area. plugin 사용자에게는 footer가 보이지 않음.
        let total_size = size
            .checked_add(tasty_shm::footer::SIZE)
            .ok_or(PluginError::Shm("size overflow with footer".into()))?;

        // RPC 송신. 결과로 {id, size}가 회신된다.
        let rpc_result = self.call_with_id(
            call_id,
            METHOD_HOST_SHARED_BUFFER_CREATE.to_string(),
            serde_json::json!({ "size": total_size as u64 }),
        );
        let parsed: SharedBufferCreateResult = match rpc_result {
            Ok(v) => serde_json::from_value(v)?,
            Err(e) => {
                tasty_utils::poison::recover_mutex(
                    self.shared_buffer_fd_pending.lock(),
                    FD_PENDING_WHAT,
                    &FD_PENDING_POISONED,
                )
                .remove(&call_id);
                return Err(e);
            }
        };

        // 보조 채널로 도착하는 fd 대기. RPC 응답보다 먼저 와 있을 수도, 뒤에 올 수도 있음.
        let fd = match fd_rx.recv_timeout(self.timeout) {
            Ok(fd) => fd,
            Err(_) => {
                tasty_utils::poison::recover_mutex(
                    self.shared_buffer_fd_pending.lock(),
                    FD_PENDING_WHAT,
                    &FD_PENDING_POISONED,
                )
                .remove(&call_id);
                return Err(PluginError::HostCallTimeout {
                    method: format!("{} (handle attach)", METHOD_HOST_SHARED_BUFFER_CREATE),
                    timeout: self.timeout,
                });
            }
        };

        // 받은 fd를 tasty_shm::receive로 매핑.
        let payload = tasty_shm::ReceivedPayload::Fd {
            fd,
            size: parsed.size as usize,
        };
        // SAFETY: fd는 보조 채널 reader 스레드(spawn_handle_reader)가 SCM_RIGHTS
        // recvmsg로 커널로부터 이 프로세스에 방금 dup 받아, call_id로 매칭해 넘겨준
        // 값 — 다른 곳에서 소유되지 않았고 아직 receive되지 않았음을 mpsc 단일
        // 소비(위 fd_rx.recv_timeout)로 보장한다.
        let mem =
            unsafe { tasty_shm::receive(payload) }.map_err(|e| PluginError::Shm(e.to_string()))?;

        Ok(SharedBuffer::new(parsed.id, mem, handle_writer))
    }

    /// Windows: Unix 판과 같은 모양이되 핸들을 fd 대신 in-band HANDLE u64 로 받는다.
    /// host 가 `DuplicateHandle` 로 우리 프로세스 테이블에 복제한 파일 매핑 핸들을
    /// 보조 채널 라인으로 받아 `tasty_shm::receive(Handle)` 로 매핑한다.
    ///
    /// **하나로 묶지 않는다.** 두 판이 받는 것이 fd 와 HANDLE 로 다르고 그 차이가 몸통
    /// 전체에 퍼져 있어 공유할 알맹이가 없다 — `#[cfg]` 로 갈리는 것이 필연이다.
    /// 여기서 "같은 모양" 은 지켜야 할 계약이 아니라 읽는 사람을 위한 유비다.
    #[cfg(windows)]
    pub fn create_shared_buffer(&self, size: usize) -> Result<SharedBuffer, PluginError> {
        let handle_writer = self
            .handle_writer
            .clone()
            .ok_or(PluginError::HandleChannelUnavailable)?;

        let call_id = self.next_call_id.fetch_add(1, Ordering::Relaxed);

        // 핸들 도착 알림 채널을 *RPC 송신 전에* 등록 — race 방지.
        let (handle_tx, handle_rx) = mpsc::channel::<u64>();
        {
            let mut m = self
                .shared_buffer_fd_pending
                .lock()
                .map_err(|_| PluginError::LockPoisoned("shared_buffer_fd_pending"))?;
            m.insert(call_id, handle_tx);
        }

        let total_size = size
            .checked_add(tasty_shm::footer::SIZE)
            .ok_or(PluginError::Shm("size overflow with footer".into()))?;

        let rpc_result = self.call_with_id(
            call_id,
            METHOD_HOST_SHARED_BUFFER_CREATE.to_string(),
            serde_json::json!({ "size": total_size as u64 }),
        );
        let parsed: SharedBufferCreateResult = match rpc_result {
            Ok(v) => serde_json::from_value(v)?,
            Err(e) => {
                tasty_utils::poison::recover_mutex(
                    self.shared_buffer_fd_pending.lock(),
                    FD_PENDING_WHAT,
                    &FD_PENDING_POISONED,
                )
                .remove(&call_id);
                return Err(e);
            }
        };

        let handle = match handle_rx.recv_timeout(self.timeout) {
            Ok(h) => h,
            Err(_) => {
                tasty_utils::poison::recover_mutex(
                    self.shared_buffer_fd_pending.lock(),
                    FD_PENDING_WHAT,
                    &FD_PENDING_POISONED,
                )
                .remove(&call_id);
                return Err(PluginError::HostCallTimeout {
                    method: format!("{} (handle attach)", METHOD_HOST_SHARED_BUFFER_CREATE),
                    timeout: self.timeout,
                });
            }
        };

        let payload = tasty_shm::ReceivedPayload::Handle {
            handle,
            size: parsed.size as usize,
        };
        // SAFETY: handle은 host가 DuplicateHandle로 이 프로세스 핸들 테이블에 방금
        // 복제해, 보조 채널 reader 스레드가 call_id로 매칭해 넘겨준 값 — 다른
        // 곳에서 소유되지 않았고 아직 receive되지 않았음을 mpsc 단일 소비(위
        // handle_rx.recv_timeout)로 보장한다.
        let mem =
            unsafe { tasty_shm::receive(payload) }.map_err(|e| PluginError::Shm(e.to_string()))?;

        Ok(SharedBuffer::new(parsed.id, mem, handle_writer))
    }
}

/// 메인 recv 스레드가 호스트로부터 받은 `ipc.result` 요청을 처리한다.
/// `result`/`error`를 매칭되는 oneshot sender로 전달.
///
/// 메서드 이름은 pending map에 저장하지 않으므로 에러 표시에는 `call#<id>`를
/// 쓴다. 호출자가 [`HostHandle::call`]의 반환 에러를 보면 timeout variant에는
/// method가 들어있다 (HostCall variant는 호스트 에러 응답일 때만 사용).
pub(crate) fn deliver_ipc_result(
    pending: &PendingCalls,
    call_id: u64,
    result: Option<Value>,
    error: Option<String>,
    error_code: Option<i32>,
) {
    let sender =
        tasty_utils::poison::recover_mutex(pending.lock(), PENDING_WHAT, &PENDING_POISONED)
            .remove(&call_id);
    if let Some(tx) = sender {
        let out = match error {
            Some(message) => Err(PluginError::HostCall {
                method: format!("call#{call_id}"),
                message,
                code: error_code,
            }),
            None => Ok(result.unwrap_or(Value::Null)),
        };
        // 호출자가 timeout 등으로 이미 rx를 drop했을 수 있음 — 무시 가능.
        if let Err(e) = tx.send(out) {
            tracing::trace!("host call#{call_id} result dropped (waiter gone): {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테스트 전용: 더미 stream으로 HostHandle 생성. 실제로 call하면 write
    /// 실패하므로 호출은 하지 말 것. timeout 동작 검증 등 internal state 테스트용.
    fn dummy_handle() -> HostHandle {
        // TcpStream::connect로 실제 소켓을 안 만들고 가짜를 만들려면 listener
        // 페어가 필요 — 여기서는 PendingCalls만 검증한다.
        let pending: PendingCalls = Arc::new(Mutex::new(HashMap::new()));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind localhost");
        let port = listener.local_addr().unwrap().port();
        let accept_handle = std::thread::spawn(move || {
            let _accepted = listener.accept();
        });
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        accept_handle.join().unwrap();
        HostHandle::new(Arc::new(Mutex::new(stream)), pending)
    }

    #[test]
    fn deliver_unblocks_waiter() {
        let handle = dummy_handle();
        handle.pending.lock().unwrap().insert(7, {
            let (tx, _rx) = mpsc::channel::<HostCallResult>();
            tx
        });
        // 모르는 call_id는 조용히 무시.
        deliver_ipc_result(&handle.pending, 999, Some(Value::Null), None, None);
        assert!(handle.pending.lock().unwrap().contains_key(&7));

        // 알려진 call_id는 pending에서 제거.
        deliver_ipc_result(&handle.pending, 7, Some(Value::Null), None, None);
        assert!(!handle.pending.lock().unwrap().contains_key(&7));
    }

    #[test]
    fn deliver_passes_error_string() {
        let handle = dummy_handle();
        let (tx, rx) = mpsc::channel::<HostCallResult>();
        handle.pending.lock().unwrap().insert(5, tx);
        deliver_ipc_result(&handle.pending, 5, None, Some("denied".into()), None);
        let err = rx.recv().unwrap().unwrap_err();
        match err {
            PluginError::HostCall { message, code, .. } => {
                assert_eq!(message, "denied");
                assert_eq!(code, None, "호스트가 코드를 안 주면 None 이다");
            }
            other => panic!("expected HostCall variant, got {other:?}"),
        }
    }

    /// 호스트가 준 코드가 waiter 까지 온다 — 그리고 **표시 문구는 안 바뀐다**.
    ///
    /// 문구를 읽는 소비자가 이미 있다(agent-stream 의 "그런 surface 는 없다" 판정).
    /// 코드를 더하는 변경이 그 판정을 깨면 안 되므로 두 축을 한 자리에서 못 박는다.
    #[test]
    fn deliver_carries_the_host_error_code_without_changing_the_message() {
        let handle = dummy_handle();
        let (tx, rx) = mpsc::channel::<HostCallResult>();
        handle.pending.lock().unwrap().insert(21, tx);
        deliver_ipc_result(
            &handle.pending,
            21,
            None,
            Some("no live surface 999 (named by 'terminal.parent')".into()),
            Some(-32602),
        );
        let err = rx.recv().unwrap().unwrap_err();
        assert_eq!(
            err.to_string(),
            "host call 'call#21' failed: no live surface 999 (named by 'terminal.parent')",
            "표시 문구에 코드가 새어 들어갔다 — 문구를 읽는 판정이 깨진다"
        );
        match err {
            PluginError::HostCall { code, .. } => assert_eq!(code, Some(-32602)),
            other => panic!("expected HostCall variant, got {other:?}"),
        }
    }

    #[test]
    fn deliver_passes_result_value() {
        let handle = dummy_handle();
        let (tx, rx) = mpsc::channel::<HostCallResult>();
        handle.pending.lock().unwrap().insert(11, tx);
        deliver_ipc_result(
            &handle.pending,
            11,
            Some(serde_json::json!({"ok": 1})),
            None,
            None,
        );
        let v = rx.recv().unwrap().unwrap();
        assert_eq!(v, serde_json::json!({"ok": 1}));
    }
}
