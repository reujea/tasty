//! Plugin 부트스트랩 + 메시지 루프.
//!
//! `run(plugin)`은 두 스레드 구조로 동작한다:
//!
//! - **메인 스레드 (receiver)**: 호스트로부터 NDJSON 한 줄씩 받는다. 받은 메시지가
//!   `ipc.result`면 매칭되는 `HostHandle::call` 대기자에게 결과를 전달. 그 외 모든
//!   `PluginRequest`는 worker queue로 enqueue.
//! - **worker 스레드 (dispatcher)**: queue에서 request를 pop해 plugin에 dispatch.
//!   dispatch 안에서 [`HostHandle::call`]을 통해 호스트를 동기 호출하면 메인이
//!   계속 recv 가능하므로 deadlock 없이 결과가 회신된다.
//!
//! shutdown 요청은 메인이 즉시 ack 보내고, 메인 루프를 빠져나오면 worker 큐에
//! [`WorkerItem::Stop`] 을 넣는다 — worker 는 그 앞에 쌓인 요청까지 처리한 뒤 멈춘다.
//! 그 요청들이 부르는 host call 은 결과를 읽어 줄 루프가 없으므로 기다리지 않고
//! [`crate::PluginError::HostClosed`] 를 받는다([`HostHandle::fail_pending_calls`]).
//! 큐가 닫히기를 기다리지 않는 이유는 `Stop` 의 문서를 본다.

use std::collections::HashMap;
use std::io::BufRead;
use std::net::TcpStream;
use std::sync::{Arc, Mutex, mpsc};

use anyhow::Result;
use serde_json::Value;

#[cfg(any(unix, windows))]
use tasty_plugin_protocol::HandleChannelMessage;
use tasty_plugin_protocol::{
    BannerClosedParams, BannerOpenParams, BannerSetContextParams, EventDispatchParams,
    IpcCallResult, IpcInvokeParams, METHOD_BANNER_CLOSED, METHOD_BANNER_OPEN,
    METHOD_BANNER_SET_CONTEXT, METHOD_COMMAND_INVOKE, METHOD_EVENT_DISPATCH, METHOD_IPC_INVOKE,
    METHOD_IPC_RESULT, METHOD_PING, METHOD_POPUP_CLOSED, METHOD_POPUP_OPEN,
    METHOD_POPUP_SET_CONTEXT, METHOD_SHUTDOWN, METHOD_SURFACE_CREATE, METHOD_SURFACE_DESTROY,
    METHOD_SURFACE_RESTORE, METHOD_SURFACE_SET_CONTEXT, METHOD_SURFACE_SNAPSHOT,
    METHOD_WEBVIEW_NAVIGATION_ATTEMPT, PluginEvent, PluginRequest, PluginResponse,
    PopupClosedParams, PopupOpenParams, PopupSetContextParams, SurfaceSetContextParams,
    WebviewNavigationAttemptParams,
};

use crate::connection::Connection;
use crate::env::PluginEnv;
use crate::handle_channel::HandleClient;
use crate::host::{HostHandle, PendingCalls, SharedBufferFdPending, deliver_ipc_result};
use crate::plugin::{
    CommandInvokeCtx, IpcMethodCtx, Plugin, SurfaceCreateCtx, SurfaceRestoreCtx, SurfaceSnapshotCtx,
};

/// worker 큐 항목. 호스트발 요청과 plugin 자신의 self-invoke를 함께 다룬다 —
/// [`crate::host::HostHandle::self_invoke`] 문서 참고.
pub(crate) enum WorkerItem {
    /// 호스트로부터 받은 실제 요청. 처리 후 반드시 host로 응답을 보낸다.
    Host(PluginRequest),
    /// plugin 자신의 백그라운드 스레드가 자기 네임스페이스 IPC 메서드를 worker
    /// 스레드(= `&mut plugin` 을 쥔 유일한 스레드)에서 실행시키기 위한 요청.
    /// host가 보낸 적 없는 call이라 응답을 돌려줄 곳이 없다 — fire-and-forget.
    SelfInvoke { method: String, params: Value },
    /// 메인 recv 루프가 끝났다(shutdown 요청 · 호스트 연결 종료 · 수신 에러) — worker 는
    /// 이 항목 앞에 쌓인 것까지 처리하고 멈춘다.
    ///
    /// sender 를 전부 drop 해 큐를 닫는 것으로는 worker 가 안 멈춘다: self-invoke 용
    /// sender 클론을 [`HostHandle`] 이 들고 있고, 그 `HostHandle` 은 `run` 자신과 worker 가
    /// 쥐며 `on_start` 로 plugin 에게도 넘어가 plugin 의 백그라운드 스레드에 남는다. 큐가
    /// 영영 안 닫히면 `run` 이 worker join 에서 서고, 프로세스가 안 끝나 호스트가
    /// shutdown 유예(2 s) 뒤 강제 종료한다.
    Stop,
}

/// dispatch 내부에서만 쓰이는 에러. JSON-RPC 에러 코드를 보존한다.
pub(crate) struct DispatchError {
    pub message: String,
    pub code: Option<i32>,
}

impl DispatchError {
    fn from_anyhow(e: anyhow::Error) -> Self {
        Self {
            message: e.to_string(),
            code: None,
        }
    }

    fn with_code(message: String, code: i32) -> Self {
        Self {
            message,
            code: Some(code),
        }
    }
}

pub fn run<P: Plugin>(plugin: P) -> Result<()> {
    let env = PluginEnv::load()?;
    run_with_env(plugin, &env)
}

/// [`run`] 의 본체 — 환경변수 대신 받은 [`PluginEnv`] 로 돈다. 프로세스 전역인
/// 환경변수를 안 건드리고 가짜 호스트에 붙여 생명주기를 시험하려고 떼어 둔 것이다.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: plugin 부트스트랩(connect/handshake/스레드 기동) 순차 설정 + 단일 while 메인 recv 루프 안의 host 라인 파싱·특수 메서드(ipc.result/shutdown) 분기. 루프 진입부터 worker join 까지가 한 함수의 생명주기라 쪼개면 reader/writer/pending/req_tx 전달만 늘어남.
fn run_with_env<P: Plugin>(plugin: P, env: &PluginEnv) -> Result<()> {
    // macOS: 부모(tasty) 사망 감시 watchdog 시작. PDEATHSIG 등가물이 없어 자식
    // 측에서 부모 PID 변화를 폴링해 self-exit 한다. Windows(Job)/Linux(PDEATHSIG)
    // 는 호스트 측 메커니즘이 처리하므로 다른 OS 에선 no-op.
    spawn_parent_death_watchdog();
    // connect + AuthMessage 송신 + AuthAck 5s 대기.
    // 호스트가 토큰을 거부하면 PluginError::HandshakeRejected가 즉시 올라온다.
    let conn = Connection::connect_and_authenticate(env)?;
    let (writer_stream, mut reader) = conn.into_parts();
    let writer = Arc::new(Mutex::new(writer_stream));

    let handle_client = connect_handle_channel(env);

    // hello event 송신.
    let hello = PluginEvent::Hello {
        plugin_id: plugin.id().to_string(),
        version: plugin.version().to_string(),
    };
    send_event(&writer, &hello)?;

    tracing::info!(
        "plugin '{}' v{} connected to host",
        plugin.id(),
        plugin.version()
    );

    let pending: PendingCalls = Arc::new(Mutex::new(HashMap::new()));
    let shared_buffer_fd_pending: SharedBufferFdPending = Arc::new(Mutex::new(HashMap::new()));
    let host = HostHandle::new(writer.clone(), pending.clone());

    // 보조 채널이 살아 있으면 reader thread 띄우고 HostHandle에 writer 연결.
    let (host, _handle_reader_thread) =
        spawn_handle_reader(host, handle_client, &shared_buffer_fd_pending)?;

    let (req_tx, req_rx) = mpsc::channel::<WorkerItem>();
    // self-invoke 채널을 HostHandle에 실어 보낸다 — 이 시점 이후 host를 클론받는
    // 모든 plugin 코드(특히 on_start 로 넘어가는 host)가 self_invoke()를 쓸 수 있다.
    let host = host.with_self_invoke(req_tx.clone());
    let worker_writer = writer.clone();
    let worker_host = host.clone();
    let worker_handle = std::thread::Builder::new()
        .name("plugin-worker".into())
        .spawn(move || worker_loop(plugin, req_rx, worker_writer, worker_host))?;

    // 메인 recv loop. 줄 버퍼는 루프 밖에 둔다 — read timeout 이 줄 중간에 걸리면
    // `read_until` 은 이미 소비한 앞부분을 버퍼에 남긴 채 에러를 돌려주므로, 다음 회가 그
    // 뒤에 이어 붙여야 줄이 온전하다. 매 회 새 버퍼를 만들면 앞부분이 사라져 요청 하나가
    // 통째로 깨진다. `String` 이 아니라 바이트인 이유: `read_line` 은 문자 중간에서 끊긴
    // 조각을 UTF-8 검사로 **지워** 버린다.
    let mut line_buf: Vec<u8> = Vec::new();
    loop {
        match reader.read_until(b'\n', &mut line_buf) {
            Ok(0) => break, // host closed
            Ok(_) => {
                let line = std::mem::take(&mut line_buf);
                let trim = line.trim_ascii();
                if trim.is_empty() {
                    continue;
                }
                let req: PluginRequest = match serde_json::from_slice(trim) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("unparseable host line: {e}");
                        continue;
                    }
                };
                // 호스트가 포화로 버린 수는 **모든** 요청이 실어 올 수 있다 —
                // worker 로 안 가는 `ipc.result`·`shutdown` 도 포함이다. 그래서
                // 분기보다 먼저 기록한다.
                if req.dropped_requests > 0 {
                    tracing::warn!(
                        "host dropped {} request(s) to this plugin before this one — \
                         this plugin is not draining its queue fast enough",
                        req.dropped_requests
                    );
                    host.record_dropped_by_host(req.dropped_requests);
                }
                if req.method == METHOD_IPC_RESULT {
                    handle_ipc_result_request(&req, &pending, &writer);
                    continue;
                }
                if req.method == METHOD_SHUTDOWN {
                    tracing::info!("plugin received shutdown");
                    let resp = PluginResponse {
                        id: req.id,
                        result: Some(Value::Null),
                        error: None,
                        error_code: None,
                    };
                    if let Err(e) = send_response(&writer, &resp) {
                        tracing::trace!("shutdown ack send failed (host closing): {e}");
                    }
                    break;
                }
                if req_tx.send(WorkerItem::Host(req)).is_err() {
                    break;
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                tracing::warn!("plugin recv error: {e}");
                break;
            }
        }
    }
    // 이제 `ipc.result` 를 읽을 스레드가 없다 — host 를 기다리는 worker 를 깨운다.
    host.fail_pending_calls();
    // 큐를 닫는 것으로는 worker 가 안 멈춘다 — `WorkerItem::Stop` 문서 참고. send 가
    // 실패하면 worker 가 이미 끝나 receiver 가 없는 것이라 join 이 곧바로 돌아온다.
    if req_tx.send(WorkerItem::Stop).is_err() {
        tracing::debug!("plugin worker already exited before stop");
    }
    drop(req_tx);
    if let Err(e) = worker_handle.join() {
        tracing::warn!("plugin worker thread panicked: {e:?}");
    }
    Ok(())
}

/// 보조 핸들 채널이 활성화돼 있으면 connect 한다. 실패는 fatal 이 아니라 warn 만 —
/// 보조 채널을 안 쓰는 plugin 이라면 그대로 동작해야 한다 (shared buffer 기능만 비활성).
fn connect_handle_channel(env: &PluginEnv) -> Option<HandleClient> {
    // endpoint 가 없으면 이 plugin 은 보조 채널을 안 쓴다 — connect 시도 자체를 안 한다.
    env.handle_endpoint.as_ref()?;
    match HandleClient::connect(env) {
        Ok(c) => {
            tracing::info!("plugin handle channel connected");
            Some(c)
        }
        Err(e) => {
            tracing::warn!("plugin handle channel connect failed: {e}");
            None
        }
    }
}

/// 보조 채널이 살아 있으면 reader thread 를 띄우고 `HostHandle` 에 writer 를 연결한다.
/// `host` 를 값으로 받아 `with_handle_channel` 로 재구성한 뒤 되돌려준다(소유권 왕복).
fn spawn_handle_reader(
    mut host: HostHandle,
    handle_client: Option<HandleClient>,
    shared_buffer_fd_pending: &SharedBufferFdPending,
) -> Result<(HostHandle, Option<std::thread::JoinHandle<()>>)> {
    let thread: Option<std::thread::JoinHandle<()>> = match handle_client {
        #[cfg(any(unix, windows))]
        Some(client) => match client.reader() {
            Ok(reader) => {
                let handle_writer = Arc::new(Mutex::new(client));
                host = host
                    .with_handle_channel(handle_writer.clone(), shared_buffer_fd_pending.clone());
                let fd_pending_clone = shared_buffer_fd_pending.clone();
                let writer_clone = handle_writer.clone();
                let handle = std::thread::Builder::new()
                    .name("plugin-handle-reader".into())
                    .spawn(move || {
                        handle_reader_loop(reader, fd_pending_clone, writer_clone);
                    })?;
                Some(handle)
            }
            Err(e) => {
                tracing::warn!("plugin handle channel reader split failed: {e}");
                None
            }
        },
        // 보조 채널을 지원하지 않는 exotic 타깃 — client 는 drop 된다.
        #[cfg(not(any(unix, windows)))]
        Some(_client) => None,
        None => None,
    };
    Ok((host, thread))
}

/// macOS 전용: 부모(tasty) 프로세스 사망을 감시해 self-exit 하는 watchdog 스레드.
///
/// macOS 는 Linux 의 `PR_SET_PDEATHSIG` 등가물이 없어 호스트 측에서 자식 수명을
/// 커널 레벨로 결박할 수 없다. 대신 자식이 직접 부모 PID 를 폴링한다 — 호스트가
/// 주입한 `TASTY_HOST_PID`(없으면 시작 시점의 `getppid`)를 기준으로, 부모가 죽어
/// 재부모화되면 `getppid` 값이 달라지므로 그때 프로세스를 종료한다.
///
/// 폴링 주기는 500ms — 종료 지연 수백 ms 는 명세상 허용. 즉시성이 문제되면
/// kqueue `EVFILT_PROC`/`NOTE_EXIT` 로 교체 검토.
#[cfg(target_os = "macos")]
fn spawn_parent_death_watchdog() {
    // SAFETY: getppid 는 부작용 없는 async-signal-safe 호출이다.
    let baseline_ppid = unsafe { libc::getppid() };
    let host_pid: i32 = std::env::var("TASTY_HOST_PID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(baseline_ppid);
    let spawned = std::thread::Builder::new()
        .name("plugin-parent-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                // SAFETY: getppid 는 부작용 없는 안전한 호출이다.
                let ppid = unsafe { libc::getppid() };
                if ppid != host_pid {
                    tracing::info!(
                        "parent host process gone (ppid {ppid} != {host_pid}) — plugin self-exit"
                    );
                    std::process::exit(0);
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("parent-death watchdog thread spawn failed: {e}");
    }
}

/// 비-macOS: 수명 결박은 호스트 측(Windows Job / Linux PDEATHSIG)이 처리하므로 no-op.
#[cfg(not(target_os = "macos"))]
fn spawn_parent_death_watchdog() {}

/// 보조 채널의 reader thread loop. host가 보낸 `HandleAttach`의 버퍼 핸들(Unix=fd /
/// Windows=HANDLE u64)을 fd_pending에 매칭해 `HostHandle::create_shared_buffer` 대기자
/// 에게 push하고, ping을 받으면 pong을 회신한다. 연결이 닫히면 조용히 종료.
#[cfg(any(unix, windows))]
#[allow(clippy::cognitive_complexity)] // complexity-exempt: 보조 채널 reader 루프 — recv_message 성공/실패 + 메시지 종류(HandleAttach/Ping/Pong/Dirty) 평면 match 나열, 중첩 얕은 디스패치.
fn handle_reader_loop(
    mut reader: crate::handle_channel::HandleClientReader,
    fd_pending: SharedBufferFdPending,
    writer: Arc<Mutex<HandleClient>>,
) {
    loop {
        match reader.recv_message() {
            Ok((msg, aux)) => match msg {
                HandleChannelMessage::HandleAttach { request_id, .. } => {
                    deliver_buffer_handle(&fd_pending, request_id, aux);
                }
                HandleChannelMessage::Ping { seq } => {
                    let pong = HandleChannelMessage::Pong { seq };
                    // poison 이면 보내지 않는다 — 이 임계구역도 소켓 쓰기라
                    // 반쯤 쓰인 메시지 위에 이어 쓰면 안 된다(`send_event` 주석 참조).
                    // 다만 **조용히** 건너뛰지는 않는다: pong 이 끊기면 호스트가 채널을
                    // 죽은 것으로 보는데, 그 원인이 어디에도 안 남으면 추적이 불가능하다.
                    match writer.lock() {
                        Ok(mut w) => {
                            if let Err(e) = w.send_message(&pong) {
                                tracing::warn!("handle channel: pong send failed: {e}");
                            }
                        }
                        Err(_) => tracing::error!(
                            "handle channel: writer lock poisoned — skipping pong; the host will \
                             see this channel go quiet"
                        ),
                    }
                }
                HandleChannelMessage::Pong { .. } => {
                    // plugin이 ping을 안 보내므로 도착할 일이 없지만 와도 무해.
                }
                HandleChannelMessage::Dirty { .. } => {
                    tracing::warn!("handle channel: plugin received Dirty (unexpected)");
                }
            },
            Err(e) => {
                tracing::debug!("handle channel reader exiting: {e}");
                break;
            }
        }
    }
}

/// Unix: `HandleAttach` 동행 fd 를 매칭 waiter 에게 push. 미매칭이면 leak 방지 close.
#[cfg(unix)]
#[allow(clippy::cognitive_complexity)] // complexity-exempt: sender 유무·send 성공 여부 조합이 전부 fd leak 방지 close 경로 — 쪼개면 unsafe 블록 위치만 함수 경계로 흩어짐.
fn deliver_buffer_handle(
    fd_pending: &SharedBufferFdPending,
    request_id: u64,
    aux: Option<std::os::fd::RawFd>,
) {
    let Some(fd) = aux else {
        tracing::warn!("handle channel: HandleAttach without fd (request_id={request_id})");
        return;
    };
    // poison 을 `.ok()` 로 버리면 대기 중인 waiter 를 못 찾아 "미매칭" 으로 흘러가고,
    // 그 waiter 는 영영 fd 를 못 받는다. 이 맵은 `HashMap<u64, Sender<_>>` 뿐이고
    // 임계구역은 `remove` 하나라 패닉이 나도 불변식이 성립한다 — 복구가 맞다.
    let sender = fd_pending
        .lock()
        .unwrap_or_else(|poisoned| {
            tracing::error!(
                "handle channel: fd_pending mutex poisoned — a thread panicked while holding it; \
                 recovering so the pending waiter still gets its handle"
            );
            poisoned.into_inner()
        })
        .remove(&request_id);
    match sender {
        Some(tx) => {
            if tx.send(fd).is_err() {
                tracing::warn!(
                    "handle channel: orphan fd for request_id={request_id} (waiter dropped)"
                );
                // SAFETY: 방금 SCM_RIGHTS로 받은 valid fd — waiter 소멸 시 leak 방지 close.
                unsafe { libc::close(fd) };
            }
        }
        None => {
            tracing::warn!("handle channel: unsolicited HandleAttach (request_id={request_id})");
            // SAFETY: 위와 동일.
            unsafe { libc::close(fd) };
        }
    }
}

/// Windows: `HandleAttach` in-band HANDLE u64 를 매칭 waiter 에게 push. 미매칭이면
/// 우리 핸들 테이블에 복제돼 있는 핸들을 `CloseHandle` 로 회수(leak 방지).
#[cfg(windows)]
fn deliver_buffer_handle(fd_pending: &SharedBufferFdPending, request_id: u64, aux: Option<u64>) {
    let Some(handle) = aux else {
        tracing::warn!("handle channel: HandleAttach without handle (request_id={request_id})");
        return;
    };
    // poison 을 `.ok()` 로 버리면 대기 중인 waiter 를 못 찾아 "미매칭" 으로 흘러가고,
    // 그 waiter 는 영영 fd 를 못 받는다. 이 맵은 `HashMap<u64, Sender<_>>` 뿐이고
    // 임계구역은 `remove` 하나라 패닉이 나도 불변식이 성립한다 — 복구가 맞다.
    let sender = fd_pending
        .lock()
        .unwrap_or_else(|poisoned| {
            tracing::error!(
                "handle channel: fd_pending mutex poisoned — a thread panicked while holding it; \
                 recovering so the pending waiter still gets its handle"
            );
            poisoned.into_inner()
        })
        .remove(&request_id);
    match sender {
        Some(tx) => {
            if tx.send(handle).is_err() {
                tracing::warn!(
                    "handle channel: orphan handle for request_id={request_id} (waiter dropped)"
                );
                close_orphan_handle(handle);
            }
        }
        None => {
            tracing::warn!("handle channel: unsolicited HandleAttach (request_id={request_id})");
            close_orphan_handle(handle);
        }
    }
}

/// Windows: 미수령 파일 매핑 핸들을 닫아 leak 을 막는다. DuplicateHandle 로 우리
/// 프로세스 테이블에 이미 복제돼 있으므로 CloseHandle 대상이다.
#[cfg(windows)]
fn close_orphan_handle(handle: u64) {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    // SAFETY: handle 은 host 가 DuplicateHandle 로 우리 테이블에 넣은 유효 핸들.
    unsafe {
        CloseHandle(handle as HANDLE);
    }
}

fn worker_loop<P: Plugin>(
    mut plugin: P,
    req_rx: mpsc::Receiver<WorkerItem>,
    writer: Arc<Mutex<TcpStream>>,
    host: HostHandle,
) {
    // dispatch가 시작되기 전에 plugin에 1회 시작 알림. plugin이 여기서 자체
    // background thread를 spawn하면 host call이 안전하게 동작한다 (메인 recv
    // 루프가 이미 동작 중이므로 ipc.result delivery 가능).
    let bus = crate::bus::BusHandle::new(writer.clone(), plugin.id().to_string());
    plugin.on_start(host.clone(), bus);
    for item in req_rx.iter() {
        match item {
            WorkerItem::Host(req) => {
                // 드롭 통지는 dispatch 직전에 한 번 — reader 스레드에서 바로 부르면
                // `&mut plugin` 을 두 스레드가 잡는다.
                let dropped = host.take_unreported_drops();
                if dropped > 0 {
                    plugin.on_host_dropped_requests(dropped);
                }
                let result = dispatch(&mut plugin, &req.method, &req.params, &host);
                let resp = build_response(req.id, result);
                if let Err(e) = send_response(&writer, &resp) {
                    tracing::warn!("plugin worker send_response failed: {e}");
                    break;
                }
            }
            WorkerItem::SelfInvoke { method, params } => {
                let caller_plugin_id = Some(plugin.id().to_string());
                if let Err(e) = plugin.handle_ipc_method(IpcMethodCtx {
                    method: method.clone(),
                    params,
                    caller_plugin_id,
                    host: host.clone(),
                }) {
                    tracing::warn!("plugin self-invoke '{method}' failed: {}", e.message);
                }
            }
            WorkerItem::Stop => break,
        }
    }
}

fn handle_ipc_result_request(
    req: &PluginRequest,
    pending: &PendingCalls,
    writer: &Arc<Mutex<TcpStream>>,
) {
    match serde_json::from_value::<IpcCallResult>(req.params.clone()) {
        Ok(parsed) => {
            deliver_ipc_result(
                pending,
                parsed.call_id,
                parsed.result,
                parsed.error,
                parsed.error_code,
            );
        }
        Err(e) => {
            tracing::warn!("ipc.result parse error: {e}");
        }
    }
    // 호스트는 응답을 기다리지 않지만, JSON-RPC 의미상 ack 응답을 보낸다.
    let ack = PluginResponse {
        id: req.id,
        result: Some(Value::Null),
        error: None,
        error_code: None,
    };
    if let Err(e) = send_response(writer, &ack) {
        tracing::trace!("ipc.result ack send failed: {e}");
    }
}

/// **poison 시 패닉을 유지하는 자리다.** 임계구역이 소켓에 줄을 쓰고 flush 하므로,
/// 락을 든 채 죽은 스레드는 **줄이 끊긴 채로** 남겨 뒀을 수 있다. 그 위에 이어 쓰면
/// 프로토콜의 "한 메시지 = 한 줄" 불변식이 깨져 호스트 파서가 어긋난다 — 여기서는
/// 데이터를 신뢰할 수 없으므로 복구가 오답이다. 폭발 반경도 이 plugin 프로세스 하나로
/// 한정된다(plugin 스레드 spawn 정책과 같은 근거,
/// [`error-handling.md`](../../../docs/dev-guide/error-handling.md) "락 poison").
pub(crate) fn send_event(writer: &Arc<Mutex<TcpStream>>, event: &PluginEvent) -> Result<()> {
    let payload = serde_json::json!({ "event": event });
    let line = serde_json::to_string(&payload)?;
    let mut w = writer.lock().expect("writer lock");
    tasty_plugin_protocol::write_line(&mut *w, &line)?;
    Ok(())
}

/// poison 시 패닉을 유지하는 이유는 [`send_event`] 와 같다 — 반쯤 쓰인 줄 위에
/// 이어 쓰지 않는다.
pub(crate) fn send_response(
    writer: &Arc<Mutex<TcpStream>>,
    response: &PluginResponse,
) -> Result<()> {
    let line = serde_json::to_string(response)?;
    let mut w = writer.lock().expect("writer lock");
    tasty_plugin_protocol::write_line(&mut *w, &line)?;
    Ok(())
}

pub(crate) fn build_response(id: u64, result: Result<Value, DispatchError>) -> PluginResponse {
    match result {
        Ok(v) => PluginResponse {
            id,
            result: Some(v),
            error: None,
            error_code: None,
        },
        Err(e) => PluginResponse {
            id,
            result: None,
            error: Some(e.message),
            error_code: e.code,
        },
    }
}

pub(crate) fn dispatch<P: Plugin>(
    plugin: &mut P,
    method: &str,
    params: &Value,
    host: &HostHandle,
) -> Result<Value, DispatchError> {
    match method {
        METHOD_PING => Ok(serde_json::json!({"pong": true})),
        METHOD_SURFACE_CREATE => {
            let surface_id = require_surface_id(params).map_err(DispatchError::from_anyhow)?;
            let kind = params
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cwd = params
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from);
            let result = plugin.create_surface(SurfaceCreateCtx {
                surface_id,
                kind,
                cwd,
                params: params.clone(),
            });
            serde_json::to_value(result).map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_SURFACE_RESTORE => {
            let surface_id = require_surface_id(params).map_err(DispatchError::from_anyhow)?;
            let kind = params
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data = params.get("data").cloned().unwrap_or(Value::Null);
            let result = plugin.restore_surface(SurfaceRestoreCtx {
                surface_id,
                kind,
                data,
            });
            serde_json::to_value(result).map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_SURFACE_SNAPSHOT => {
            let surface_id = require_surface_id(params).map_err(DispatchError::from_anyhow)?;
            let data = plugin.snapshot_surface(SurfaceSnapshotCtx { surface_id });
            Ok(serde_json::json!({"data": data}))
        }
        METHOD_SURFACE_DESTROY => {
            let surface_id = require_surface_id(params).map_err(DispatchError::from_anyhow)?;
            plugin.destroy_surface(surface_id);
            Ok(Value::Null)
        }
        METHOD_SURFACE_SET_CONTEXT => {
            let parsed: SurfaceSetContextParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(
                        format!("invalid surface.set_context params: {e}"),
                        -32602,
                    )
                })?;
            plugin.paint_surface(crate::plugin::SurfaceSetContextCtx {
                params: parsed,
                host: host.clone(),
            });
            // fire-and-forget — mesh 는 PaintFrame 알림으로 비동기 회신. null ack.
            Ok(Value::Null)
        }
        METHOD_COMMAND_INVOKE => {
            let surface_id = require_surface_id(params).map_err(DispatchError::from_anyhow)?;
            let command_id = params
                .get("command_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| DispatchError {
                    message: "command.invoke params missing 'command_id'".into(),
                    code: None,
                })?
                .to_string();
            let result = plugin.handle_command(CommandInvokeCtx {
                surface_id,
                command_id,
            });
            serde_json::to_value(result).map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_IPC_INVOKE => {
            let parsed: IpcInvokeParams = serde_json::from_value(params.clone()).map_err(|e| {
                DispatchError::with_code(format!("invalid ipc.invoke params: {e}"), -32602)
            })?;
            match plugin.handle_ipc_method(IpcMethodCtx {
                method: parsed.method,
                params: parsed.params,
                caller_plugin_id: parsed.caller_plugin_id,
                host: host.clone(),
            }) {
                Ok(value) => Ok(value),
                Err(err) => Err(DispatchError::with_code(err.message, err.code)),
            }
        }
        tasty_plugin_protocol::METHOD_EXTENSION_INVOKE_HOOK => {
            let parsed: tasty_plugin_protocol::ExtensionHookInvokeParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(
                        format!("invalid extension.invoke_hook params: {e}"),
                        -32602,
                    )
                })?;
            let outcome = plugin.handle_extension_hook(crate::plugin::ExtensionHookCtx {
                kind: parsed.kind,
                phase: parsed.phase,
                mode: parsed.mode,
                target: parsed.target,
                payload: parsed.payload,
                host: host.clone(),
            });
            serde_json::to_value(outcome.into_proto())
                .map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_EVENT_DISPATCH => {
            let parsed: EventDispatchParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(format!("invalid event.dispatch params: {e}"), -32602)
                })?;
            plugin.on_event(crate::plugin::EventDispatchCtx {
                sub_id: parsed.sub_id,
                envelope: parsed.envelope,
            });
            // 호스트는 이 응답을 기다리지 않지만, 받으면 그 dispatch 의 재발화 hop 하한
            // 기록을 지운다. on_event 가 돌아온 뒤에 응답하는 이 순서가 그 하한의 전제다 —
            // 응답 전에 이 plugin 이 publish 한 사건은 호스트가 hop 을 올린다(ADR-0406).
            // 응답 값 자체는 쓰지 않으므로 null.
            Ok(Value::Null)
        }
        METHOD_WEBVIEW_NAVIGATION_ATTEMPT => {
            let parsed: WebviewNavigationAttemptParams = serde_json::from_value(params.clone())
                .map_err(|e| {
                    DispatchError::with_code(
                        format!("invalid webview.navigation_attempt params: {e}"),
                        -32602,
                    )
                })?;
            plugin.on_webview_navigation_attempt(crate::plugin::WebviewNavigationAttemptCtx {
                surface_id: parsed.surface_id,
                url: parsed.url,
            });
            // 호스트는 응답을 무시한다(fire-and-forget). null 반환.
            Ok(Value::Null)
        }
        METHOD_POPUP_OPEN => {
            let parsed: PopupOpenParams = serde_json::from_value(params.clone()).map_err(|e| {
                DispatchError::with_code(format!("invalid popup.open params: {e}"), -32602)
            })?;
            let result = plugin.open_popup(crate::plugin::PopupOpenCtx {
                popup_id: parsed.popup_id,
                instance_id: parsed.instance_id,
                context: parsed.context,
            });
            serde_json::to_value(result).map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_POPUP_CLOSED => {
            let parsed: PopupClosedParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(format!("invalid popup.closed params: {e}"), -32602)
                })?;
            plugin.on_popup_closed(crate::plugin::PopupClosedCtx {
                instance_id: parsed.instance_id,
                reason: parsed.reason,
            });
            Ok(Value::Null)
        }
        METHOD_POPUP_SET_CONTEXT => {
            let parsed: PopupSetContextParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(
                        format!("invalid popup.set_context params: {e}"),
                        -32602,
                    )
                })?;
            plugin.paint_popup(crate::plugin::PopupSetContextCtx {
                params: parsed,
                host: host.clone(),
            });
            // fire-and-forget — mesh 는 PopupPaintFrame 알림으로 비동기 회신. null ack.
            Ok(Value::Null)
        }
        METHOD_BANNER_OPEN => {
            let parsed: BannerOpenParams = serde_json::from_value(params.clone()).map_err(|e| {
                DispatchError::with_code(format!("invalid banner.open params: {e}"), -32602)
            })?;
            plugin.open_banner(crate::plugin::BannerOpenCtx {
                banner_id: parsed.banner_id,
                instance_id: parsed.instance_id,
                context: parsed.context,
            });
            // banner 는 초기 tree 가 없다(egui-mesh 전용) — 빈 결과.
            serde_json::to_value(tasty_plugin_protocol::BannerOpenResult::default())
                .map_err(|e| DispatchError::from_anyhow(e.into()))
        }
        METHOD_BANNER_CLOSED => {
            let parsed: BannerClosedParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(format!("invalid banner.closed params: {e}"), -32602)
                })?;
            plugin.on_banner_closed(crate::plugin::BannerClosedCtx {
                instance_id: parsed.instance_id,
                reason: parsed.reason,
            });
            Ok(Value::Null)
        }
        METHOD_BANNER_SET_CONTEXT => {
            let parsed: BannerSetContextParams =
                serde_json::from_value(params.clone()).map_err(|e| {
                    DispatchError::with_code(
                        format!("invalid banner.set_context params: {e}"),
                        -32602,
                    )
                })?;
            plugin.paint_banner(crate::plugin::BannerSetContextCtx {
                params: parsed,
                host: host.clone(),
            });
            // fire-and-forget — mesh 는 BannerPaintFrame 알림으로 비동기 회신. null ack.
            Ok(Value::Null)
        }
        other => Err(DispatchError::with_code(
            format!("plugin does not handle method '{other}'"),
            -32601,
        )),
    }
}

fn require_surface_id(params: &Value) -> Result<u32> {
    params
        .get("surface_id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| anyhow::anyhow!("missing 'surface_id' parameter"))
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
