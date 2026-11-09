//! Plugin → 호스트 동기 IPC 호출 헬퍼.
//!
//! SDK 메시지 루프가 메인(receiver) + worker(dispatcher)로 갈라져 있을 때만
//! 의미가 있다. worker가 [`HostHandle::call`]을 호출하면 IpcCall event가
//! 송신되고, 메인 스레드가 호스트로부터 받은 `ipc.result`를 oneshot channel로
//! 결과를 push한다. worker는 그 결과가 올 때까지 block.

use std::collections::HashMap;
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

/// 호스트 호출의 결과 타입.
pub(crate) type HostCallResult = Result<Value, PluginError>;
pub(crate) type PendingCalls = Arc<Mutex<HashMap<u64, mpsc::Sender<HostCallResult>>>>;

/// 두 pending 맵의 poison을 각각 처음 한 번 보고한다.
/// 맵 정리와 응답 전달은 복구한 잠금으로 계속 처리한다. 소켓 writer의 poison은
/// 메시지가 일부만 쓰였을 수 있어 이와 다르게 처리한다.
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
    /// 송신 후 응답 채널을 기다리는 시간. 기본 60초이며 잠금·송신 시간은 포함하지 않는다.
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
    /// worker 가 지금 답할 `ipc.invoke` — 처리기가 미루면 그 답을 가져간다(`deferred`).
    reply_slot: crate::deferred::ReplySlot,
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
            reply_slot: Arc::default(),
        }
    }

    /// 수신 루프 종료 시 대기 중인 호스트 호출에 HostClosed를 전달한다.
    /// 먼저 종료 플래그를 켜고 pending 맵을 비운다. 새 호출은 맵에 등록한 뒤
    /// 종료 플래그를 검사하므로 등록 시점이 겹쳐도 결과를 기다리지 않게 한다.
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

    pub(crate) fn reply_slot(&self) -> &crate::deferred::ReplySlot {
        &self.reply_slot
    }

    pub(crate) fn defer_reply(&self) -> Option<crate::deferred::DeferredReply> {
        crate::deferred::take(&self.reply_slot, &self.writer)
    }

    /// 호스트가 큐 포화로 버렸다고 이 SDK에 알려 온 요청의 누적 수.
    /// 현재 부하뿐 아니라 앞서 발생한 포화도 포함한다.
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

    /// 자기 IPC 메서드를 호스트를 거치지 않고 플러그인 워커 큐에 넣는다.
    /// HostHandle::call은 자기 네임스페이스 요청을 호스트 구현으로 전달하므로
    /// 플러그인 내부 함수를 실행하려면 이 메서드를 사용한다.
    /// 반환값은 큐에 넣은 결과이며, 실행 실패는 워커가 로그에 남긴다.
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
        tasty_plugin_protocol::write_line(&mut *w, &line)?;
        Ok(())
    }

    /// 호스트 IPC를 호출한다. 송신 뒤 응답 채널 대기에 timeout을 적용한다.
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
            tasty_plugin_protocol::write_line(&mut *w, &line)?;
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

    /// Windows 공유 메모리 생성. 호스트가 DuplicateHandle로 복제한 HANDLE을
    /// 보조 채널에서 받아 tasty_shm::receive로 매핑한다.
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

    /// 테스트용 HostHandle. 상대 소켓은 바로 닫으므로 실제 응답은 받을 수 없다.
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

    /// 오류 코드가 전달되면서 기존 Display 문자열도 유지되는지 확인한다.
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
            "오류 코드를 추가해도 기존 Display 문자열은 유지해야 한다"
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
