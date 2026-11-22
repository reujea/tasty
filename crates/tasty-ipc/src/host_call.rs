//! 메인 스레드 밖에서 호스트 IPC 큐에 명령을 넣고 동기 응답을 기다린다.
//! IpcWaker로 루프를 깨우며 서버와 같은 바이트·주입 개수 제한을 적용한다.
//!
//! dispatch는 응답 대기 상한을 명령 기한으로도 싣는다. 시작 전에 만료되면
//! Expired(미실행), 시작한 뒤 대기가 끝나면 Timeout(결과 불명)이다.
//! dispatch_even_if_abandoned는 기한 없이 보내 호출자가 떠나도 나중에 실행할 수 있다.
//! dispatch_keeping_late_answer는 Timeout 대신 응답 통로(LateAnswer)를 넘겨 늦은 응답을 받게 한다.

use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::Value;

use crate::admission::{CommandAdmission, Origin, Refusal};
use crate::protocol::{
    ERR_EXPIRED_BEFORE_RUN, ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN, JsonRpcRequest, JsonRpcResponse,
};
use crate::server::{IpcCommand, IpcWaker, Withdraw};

/// 주입 실패 사유. nothing_ran이 true인 경우만 실행되지 않았음이 확실하다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectError {
    /// 큐 입장 장부가 거절했다 — 명령은 큐에 안 들어갔고 실행되지 않았다.
    Refused(Refusal),
    /// 큐 송신 실패(메인 루프가 큐를 버렸다) — 실행되지 않았다.
    Send(String),
    /// JSON-RPC 오류 응답. 실제 실행 여부는 오류 사유에 따라 다르다.
    Rpc {
        /// JSON-RPC 에러 코드.
        code: i32,
        /// 에러 문구.
        message: String,
    },
    /// 응답 대기 상한이 지났다 — **결과 불명**. 기한을 실은 주입([`HostIpcInjector::dispatch`])
    /// 이면 명령은 이미 시작됐고 handler 에서 계속 갈 수 있다. 기한 없는 주입이면 큐에 남아
    /// 나중에 실행될 수도 있다.
    Timeout(Duration),
    /// 응답 대기 상한이 명령이 **큐에서 기다리는 동안** 지났다 — 명령은 실행되지 않았고 앞으로도
    /// 안 된다. 기한을 실은 주입만 이 갈래를 낸다.
    Expired(Duration),
    /// 응답 통로가 답 없이 버려졌다.
    Disconnected,
}

impl InjectError {
    /// 이 실패에서 요청이 **확실히 실행되지 않았는가**. 참이면 그대로 다시 걸어도 두 번째
    /// 효과가 남지 않는다.
    pub fn nothing_ran(&self) -> bool {
        matches!(
            self,
            InjectError::Refused(_) | InjectError::Send(_) | InjectError::Expired(_)
        )
    }
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InjectError::Refused(r) => {
                write!(
                    f,
                    "host_dispatch refused before queueing (nothing ran): {r}"
                )
            }
            InjectError::Send(e) => write!(f, "inject IpcCommand: {e}"),
            InjectError::Rpc { code, message } => write!(f, "rpc error {code}: {message}"),
            InjectError::Timeout(t) => write!(f, "host_dispatch timeout after {t:?}"),
            InjectError::Expired(t) => write!(
                f,
                "host_dispatch timeout after {t:?} while still queued (nothing ran)"
            ),
            InjectError::Disconnected => {
                write!(f, "host_dispatch response channel disconnected")
            }
        }
    }
}

impl std::error::Error for InjectError {}

/// Host→plugin 동기 IPC 디스패처. `Clone` — Arc 기반이라 thread 간 자유 공유.
#[derive(Clone)]
pub struct HostIpcInjector {
    sender: mpsc::Sender<IpcCommand>,
    waker: IpcWaker,
    /// 서버와 공유하는 입장 장부. 서버 없이 만든 시험 채널에서는 None일 수 있다.
    admission: Option<Arc<CommandAdmission>>,
}

impl HostIpcInjector {
    pub fn new(sender: mpsc::Sender<IpcCommand>, waker: IpcWaker) -> Self {
        Self {
            sender,
            waker,
            admission: None,
        }
    }

    /// 큐 입장 장부를 붙인다. 붙은 뒤로 주입은 그 장부의 주입 깊이·바이트 판정을 받는다.
    pub fn with_admission(mut self, admission: Arc<CommandAdmission>) -> Self {
        self.admission = Some(admission);
        self
    }

    /// 큐 통계 조회에 쓰는 공용 입장 장부. 서버 없이 조립했으면 None이다.
    pub fn admission(&self) -> Option<&Arc<CommandAdmission>> {
        self.admission.as_ref()
    }

    /// 대기 상한을 명령 기한으로 싣는다. 시작 전 만료는 Expired, 시작 후 만료는 Timeout이다.
    pub fn dispatch(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, InjectError> {
        self.dispatch_inner(method, params, timeout, true)
    }

    /// [`Self::dispatch`] 와 같지만, 시작한 뒤 상한이 지나면 응답 통로를 [`LateAnswer`] 로 남긴다.
    /// 다시 보낼 수 없는(비멱등) 요청의 늦은 응답을 중복 실행 없이 받는 데 쓴다.
    pub fn dispatch_keeping_late_answer(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LateDispatch> {
        self.send_and_wait(method, params, timeout, true)
            .into_late()
    }

    /// 호출자가 대기를 끝내도 나중에 실행할 명령을 기한 없이 넣는다.
    /// 이미 ACK한 이벤트를 반영하는 훅 등에 쓰며 대기 만료는 항상 Timeout이다.
    pub fn dispatch_even_if_abandoned(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, InjectError> {
        self.dispatch_inner(method, params, timeout, false)
    }

    fn dispatch_inner(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        with_deadline: bool,
    ) -> Result<Value, InjectError> {
        self.send_and_wait(method, params, timeout, with_deadline)
            .into_inject()
    }

    fn send_and_wait(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        with_deadline: bool,
    ) -> Waited {
        match self.send(method, params, with_deadline.then(|| deadline_ms(timeout))) {
            Ok(sent) => sent.wait(timeout, with_deadline),
            Err(e) => Waited::Done(Err(e)),
        }
    }

    /// 명령을 큐에 넣고 루프를 깨운다. 응답 대기는 [`Sent::wait`] 가 따로 한다 — 시험은 그 사이에
    /// 명령을 꺼내 시작시켜 대기 상한이 시작 뒤에야 흐르게 한다.
    fn send(
        &self,
        method: &str,
        params: Value,
        response_timeout_ms: Option<u64>,
    ) -> Result<Sent, InjectError> {
        let (resp_tx, resp_rx) = mpsc::sync_channel::<JsonRpcResponse>(1);
        let req = JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            id: Some(Value::from(1u64)),
            params,
            session_token: None,
        };
        let mut cmd = IpcCommand::new(req, resp_tx);
        if let Some(admission) = &self.admission
            && let Err(r) = cmd.admit(admission, Origin::Injected)
        {
            return Err(InjectError::Refused(r));
        }
        // Queued-progress ownership stays on cmd: send failure/drop/dequeue or withdrawal ends it.
        // The waiter/late answer keeps flight ownership, not a main-loop queue obligation.
        // 시작/철회를 판정하고 in-flight 수명을 유지하도록 대기 중 lifecycle을 보유한다.
        let lifecycle = cmd.lifecycle();
        let request_seq = cmd.request_seq();
        if let Err(e) = self.sender.send(cmd) {
            return Err(InjectError::Send(e.to_string()));
        }
        lifecycle.mark_enqueued();
        (self.waker)();
        Ok(Sent {
            resp_rx,
            lifecycle,
            request_seq,
        })
    }
}

/// 큐에 들어간 주입. 응답 통로와 실행 상태 사본을 쥔다.
struct Sent {
    resp_rx: mpsc::Receiver<JsonRpcResponse>,
    lifecycle: crate::server::LifecycleHandle,
    request_seq: crate::server::RequestSeq,
}

impl Sent {
    fn wait(self, timeout: Duration, with_deadline: bool) -> Waited {
        let Sent {
            resp_rx,
            lifecycle,
            request_seq,
        } = self;
        // 실제 응답 또는 기한 만료의 결과를 기록한다. 기한 없는 대기 만료는 이후 실행될 수 있어 기록하지 않는다.
        Waited::Done(match resp_rx.recv_timeout(timeout) {
            Ok(resp) => {
                lifecycle.record_answer(&resp);
                answer(resp, timeout, with_deadline)
            }
            // 기한이 있으면 소켓 경로(`await_dispatch_response`)와 같은 판정이다 — 아직 큐에
            // 있었으면 꺼내질 때 실행되지 않게 막는다. 기한이 없으면 명령은 큐에 남는다.
            Err(mpsc::RecvTimeoutError::Timeout) if with_deadline => {
                tracing::debug!(target: "tasty::ipc_wait", request_seq = request_seq.get(), timeout_ms = timeout.as_millis(), "host IPC response wait bound reached; queued withdrawal and started outcome remain distinct");
                match lifecycle.withdraw() {
                    Withdraw::NotRun => {
                        lifecycle.record_error_code(ERR_EXPIRED_BEFORE_RUN);
                        Err(InjectError::Expired(timeout))
                    }
                    Withdraw::Started => {
                        // 호출자의 상한은 지났으므로 답이 나간 방식은 결과 불명으로 적는다.
                        lifecycle.record_error_code(ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN);
                        return Waited::StartedPastBound(LateAnswer {
                            rx: resp_rx,
                            _lifecycle: lifecycle,
                            timeout,
                        });
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Err(InjectError::Timeout(timeout)),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(InjectError::Disconnected),
        })
    }
}

/// 대기의 끝. 시작한 명령이 상한을 넘기면 응답 통로를 그대로 넘긴다.
enum Waited {
    Done(Result<Value, InjectError>),
    StartedPastBound(LateAnswer),
}

impl Waited {
    /// [`HostIpcInjector::dispatch`] 의 갈래 — 시작 뒤 만료는 Timeout 이다.
    fn into_inject(self) -> Result<Value, InjectError> {
        match self {
            Waited::Done(r) => r,
            Waited::StartedPastBound(late) => Err(InjectError::Timeout(late.timeout)),
        }
    }

    /// [`HostIpcInjector::dispatch_keeping_late_answer`] 의 갈래 — 시작 뒤 만료는 응답 통로다.
    fn into_late(self) -> Result<Value, LateDispatch> {
        match self {
            Waited::Done(r) => r.map_err(LateDispatch::Failed),
            Waited::StartedPastBound(late) => Err(LateDispatch::Pending(late)),
        }
    }
}

/// [`HostIpcInjector::dispatch_keeping_late_answer`] 의 실패.
#[derive(Debug)]
pub enum LateDispatch {
    /// 응답 대기 상한 안에 끝난 실패. 갈래는 [`HostIpcInjector::dispatch`] 와 같다.
    Failed(InjectError),
    /// 명령은 시작됐고 상한이 지났다. 응답은 아직 올 수 있다.
    Pending(LateAnswer),
}

/// 시작한 뒤 상한을 넘긴 명령의 응답 통로. 들고 있는 동안 in-flight 몫도 유지된다.
#[derive(Debug)]
pub struct LateAnswer {
    rx: mpsc::Receiver<JsonRpcResponse>,
    _lifecycle: crate::server::LifecycleHandle,
    timeout: Duration,
}

impl LateAnswer {
    /// 처음 걸었던 응답 대기 상한.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 기다리지 않고 응답을 확인한다. 아직 없으면 None 이다. 답 없이 통로가 닫혔으면
    /// [`InjectError::Disconnected`] 다(실행 결과 불명).
    pub fn try_take(&self) -> Option<Result<Value, InjectError>> {
        match self.rx.try_recv() {
            Ok(resp) => Some(answer(resp, self.timeout, true)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(InjectError::Disconnected)),
        }
    }
}

/// 대기 상한을 봉투의 밀리초로 옮긴다. `0` 은 봉투 규약상 "기한 없음" 이라 1 ms 밑의 상한도 1 로
/// 싣는다 — 기한을 실으려던 호출이 기한 없는 명령이 되면 안 된다.
fn deadline_ms(timeout: Duration) -> u64 {
    u64::try_from(timeout.as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}

/// 받은 답을 호출자의 갈래로 옮긴다. 꺼낸 쪽이 기한 경과를 먼저 보면 `-32067` 을 답으로 보내므로
/// (`ipc_round::claim_or_answer`), 기한을 실은 주입에서 그 코드는 [`InjectError::Expired`] 다 —
/// 기다리는 쪽이 먼저 물러났을 때와 같은 사실이다.
fn answer(
    resp: JsonRpcResponse,
    timeout: Duration,
    with_deadline: bool,
) -> Result<Value, InjectError> {
    match resp.error {
        Some(err) if with_deadline && err.code == ERR_EXPIRED_BEFORE_RUN => {
            Err(InjectError::Expired(timeout))
        }
        Some(err) => Err(InjectError::Rpc {
            code: err.code,
            message: err.message,
        }),
        None => Ok(resp.result.unwrap_or(Value::Null)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;
    use crate::server::Claim;

    fn noop_waker() -> IpcWaker {
        Arc::new(|| {})
    }

    #[test]
    fn dispatch_sends_command_and_returns_response() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());

        let h = thread::spawn(move || {
            let cmd = rx.recv().expect("recv cmd");
            assert_eq!(cmd.request.method, "echo.method");
            let resp = JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(Value::Null),
                serde_json::json!({"ok": true}),
            );
            cmd.response_tx.send(resp).expect("send resp");
        });

        let v = injector
            .dispatch("echo.method", serde_json::json!({}), Duration::from_secs(2))
            .expect("dispatch ok");
        assert_eq!(v, serde_json::json!({"ok": true}));
        h.join().unwrap();
    }

    #[test]
    fn dispatch_returns_err_on_rpc_error() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());

        let (cell_tx, cell_rx) = mpsc::channel();
        let h = thread::spawn(move || {
            let cmd = rx.recv().expect("recv cmd");
            cell_tx.send(cmd.outcome_cell()).expect("cell");
            let resp = JsonRpcResponse::error(
                cmd.request.id.clone().unwrap_or(Value::Null),
                -32601,
                "Method not found",
            );
            cmd.response_tx.send(resp).expect("send resp");
        });

        let err = injector
            .dispatch("missing", Value::Null, Duration::from_secs(2))
            .expect_err("should be err");
        assert!(err.to_string().contains("-32601"));
        assert!(!err.nothing_ran(), "an rpc error means the handler ran");
        h.join().unwrap();
        assert_eq!(
            cell_rx.recv().expect("cell").get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error { code: -32601 }),
            "the waiter records the answer it received"
        );
    }

    fn stats() -> Arc<crate::dispatch::DispatchStats> {
        Arc::new(crate::dispatch::DispatchStats::default())
    }

    #[test]
    fn an_injection_abandoned_in_the_queue_is_expired_and_not_run_later() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let err = injector
            .dispatch("surface.send", Value::Null, Duration::from_millis(30))
            .expect_err("nobody answers");
        assert_eq!(err, InjectError::Expired(Duration::from_millis(30)));
        assert!(err.nothing_ran(), "it was still queued");

        let stale = rx.try_recv().expect("the command is still queued");
        assert_eq!(
            stale.request.response_timeout_ms,
            Some(30),
            "the deadline is the caller's own wait"
        );
        assert_eq!(
            stale.claim(&stats()),
            Claim::Withdrawn,
            "taken after the caller gave up, it must not run"
        );
        assert_eq!(
            stale.outcome_cell().get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error {
                code: ERR_EXPIRED_BEFORE_RUN
            }),
            "the slow-request row reads the code the socket path would have sent"
        );
    }

    /// 보내기와 기다리기 사이에 같은 스레드에서 명령을 꺼내 시작시킨다. 요청에 기한을 싣지 않으므로
    /// claim 은 큐 기한을 재지 않고, 응답 대기 상한은 claim 뒤에 흐르므로 만료 순간 명령은 반드시
    /// 시작된 상태다. 기다리는 쪽의 철회·결과 불명 처리(`with_deadline`)는 켜 둔다.
    fn send_started(
        injector: &HostIpcInjector,
        rx: &mpsc::Receiver<IpcCommand>,
    ) -> (Sent, IpcCommand) {
        let sent = injector
            .send("noop", Value::Null, None)
            .expect("the injection is queued");
        let cmd = rx.try_recv().expect("the command reaches the queue");
        assert_eq!(cmd.claim(&stats()), Claim::Run);
        (sent, cmd)
    }

    #[test]
    fn an_injection_that_started_before_its_bound_is_an_unknown_outcome() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let (sent, cmd) = send_started(&injector, &rx);
        // 답하지 않은 채 상한을 넘긴다 — 굳은 handler 다.
        let err = sent
            .wait(Duration::from_millis(50), true)
            .into_inject()
            .expect_err("should time out");
        assert_eq!(err, InjectError::Timeout(Duration::from_millis(50)));
        assert!(
            !err.nothing_ran(),
            "a started command's timeout is an unknown outcome, not a refusal"
        );
        assert_eq!(
            cmd.outcome_cell().get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error {
                code: ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN
            })
        );
    }

    #[test]
    fn a_public_dispatch_started_past_its_bound_is_a_timeout() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let (held_tx, held_rx) = mpsc::channel::<IpcCommand>();
        let rx = std::sync::Mutex::new(rx);
        // 보낸 스레드가 응답 대기 전에 부르는 waker 안에서 꺼내 시작시킨다. 남는 전제는 요청에
        // 실린 큐 기한(1초)이 명령 생성부터 이 claim 까지의 몇 줄 사이에 지나지 않는 것이다.
        let waker: IpcWaker = Arc::new(move || {
            let cmd = rx
                .lock()
                .expect("waker lock")
                .try_recv()
                .expect("sent before the wake");
            assert_eq!(cmd.claim(&stats()), Claim::Run);
            held_tx.send(cmd).expect("the test holds the receiver");
        });
        let injector = HostIpcInjector::new(tx, waker);
        let err = injector
            .dispatch("noop", Value::Null, Duration::from_secs(1))
            .expect_err("should time out");
        assert_eq!(err, InjectError::Timeout(Duration::from_secs(1)));
        assert!(!err.nothing_ran());
        let cmd = held_rx.try_recv().expect("the started command is held");
        assert_eq!(
            cmd.request.response_timeout_ms,
            Some(1000),
            "the public call carries its bound as the deadline"
        );
        assert_eq!(
            cmd.outcome_cell().get(),
            Some(tasty_telemetry::slow_requests::HostOutcome::Error {
                code: ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN
            })
        );
    }

    #[test]
    fn a_started_injection_past_its_bound_can_still_deliver_its_late_answer() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let (sent, cmd) = send_started(&injector, &rx);
        let late = match sent.wait(Duration::from_millis(50), true).into_late() {
            Err(LateDispatch::Pending(late)) => late,
            other => panic!("expected a pending late answer, got {other:?}"),
        };
        assert_eq!(late.timeout(), Duration::from_millis(50));
        assert!(late.try_take().is_none(), "nothing answered yet");
        let resp = JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(Value::Null),
            serde_json::json!({"surface_id": 7}),
        );
        cmd.response_tx.send(resp).expect("send resp");
        assert_eq!(
            late.try_take(),
            Some(Ok(serde_json::json!({"surface_id": 7}))),
            "the answer that came after the bound is not lost"
        );
    }

    #[test]
    fn a_late_answer_whose_channel_closes_unanswered_is_disconnected() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let (sent, cmd) = send_started(&injector, &rx);
        let Err(LateDispatch::Pending(late)) =
            sent.wait(Duration::from_millis(50), true).into_late()
        else {
            panic!("expected a pending late answer");
        };
        drop(cmd);
        assert_eq!(late.try_take(), Some(Err(InjectError::Disconnected)));
    }

    #[test]
    fn keeping_the_late_answer_still_reports_a_queued_expiry_as_nothing_ran() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let res =
            injector.dispatch_keeping_late_answer("noop", Value::Null, Duration::from_millis(20));
        match res {
            Err(LateDispatch::Failed(e)) => {
                assert_eq!(e, InjectError::Expired(Duration::from_millis(20)));
                assert!(e.nothing_ran());
            }
            other => panic!("expected an expiry, got {other:?}"),
        }
        let stale = rx.recv().expect("the command was queued");
        assert_eq!(
            stale.request.response_timeout_ms,
            Some(20),
            "keeping the late answer still carries the deadline"
        );
        assert_eq!(stale.claim(&stats()), Claim::Withdrawn);
    }

    #[test]
    fn an_injection_even_if_abandoned_carries_no_deadline_and_runs_later() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let err = injector
            .dispatch_even_if_abandoned(
                "agent.task_set_result",
                Value::Null,
                Duration::from_millis(10),
            )
            .expect_err("nobody answers");
        assert_eq!(err, InjectError::Timeout(Duration::from_millis(10)));
        assert!(!err.nothing_ran());
        let stale = rx.try_recv().expect("still queued");
        assert!(
            stale.queue_progress_pending(),
            "abandonment without deadline does not cancel queued work"
        );
        assert_eq!(stale.request.response_timeout_ms, None);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(stale.claim(&stats()), Claim::Run, "it still runs");
        assert!(
            !stale.queue_progress_pending(),
            "async completion is not a queued obligation"
        );
    }

    #[test]
    fn the_takers_not_run_answer_is_expired_only_for_a_deadline() {
        let t = Duration::from_millis(7);
        let not_run = || crate::server::expired_before_run_response(Value::from(1u64), t);
        assert_eq!(answer(not_run(), t, true), Err(InjectError::Expired(t)));
        assert!(matches!(
            answer(not_run(), t, false),
            Err(InjectError::Rpc {
                code: ERR_EXPIRED_BEFORE_RUN,
                ..
            })
        ));
    }

    #[test]
    fn a_sub_millisecond_bound_still_carries_a_deadline() {
        assert_eq!(deadline_ms(Duration::from_micros(10)), 1);
        assert_eq!(deadline_ms(Duration::from_secs(5)), 5000);
    }

    fn ledger(depth: usize) -> Arc<CommandAdmission> {
        CommandAdmission::new(crate::admission::QueueLimits {
            queued_bytes: crate::admission::QUEUED_BYTES_LIMIT,
            injected_depth: depth,
        })
    }

    #[test]
    fn a_stalled_queue_refuses_injection_past_the_depth_limit() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker()).with_admission(ledger(2));
        for _ in 0..2 {
            let err = injector
                .dispatch("noop", Value::Null, Duration::from_millis(10))
                .expect_err("nobody answers");
            assert_eq!(err, InjectError::Expired(Duration::from_millis(10)));
        }
        let err = injector
            .dispatch("noop", Value::Null, Duration::from_millis(10))
            .expect_err("third one is refused");
        assert!(
            matches!(
                err,
                InjectError::Refused(Refusal::InjectedDepth {
                    queued: 2,
                    limit: 2
                })
            ),
            "{err:?}"
        );
        assert!(err.nothing_ran());
        assert_eq!(
            rx.try_iter().count(),
            2,
            "the refused command never reached the queue"
        );
    }

    #[test]
    fn draining_the_queue_lets_injection_back_in() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker()).with_admission(ledger(1));
        let first = injector.dispatch("noop", Value::Null, Duration::from_millis(10));
        assert!(matches!(first, Err(InjectError::Expired(_))), "{first:?}");
        let refused = injector.dispatch("noop", Value::Null, Duration::from_millis(10));
        assert!(
            matches!(refused, Err(InjectError::Refused(_))),
            "{refused:?}"
        );

        let mut stale = rx
            .try_recv()
            .expect("the timed-out command is still queued");
        stale.mark_dequeued();
        drop(stale);

        let again = injector.dispatch("noop", Value::Null, Duration::from_millis(10));
        assert!(matches!(again, Err(InjectError::Expired(_))), "{again:?}");
    }

    #[test]
    fn the_legacy_error_strings_are_kept() {
        assert_eq!(
            InjectError::Timeout(Duration::from_millis(5)).to_string(),
            "host_dispatch timeout after 5ms"
        );
        assert_eq!(
            InjectError::Expired(Duration::from_millis(5)).to_string(),
            "host_dispatch timeout after 5ms while still queued (nothing ran)"
        );
        assert_eq!(
            InjectError::Disconnected.to_string(),
            "host_dispatch response channel disconnected"
        );
        assert_eq!(
            InjectError::Rpc {
                code: -1,
                message: "m".into()
            }
            .to_string(),
            "rpc error -1: m"
        );
    }

    #[test]
    fn each_injection_gets_its_own_request_seq_while_the_rpc_id_stays_one() {
        let (tx, rx) = mpsc::channel::<IpcCommand>();
        let injector = HostIpcInjector::new(tx, noop_waker());
        let h = thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let cmd = rx.recv().expect("recv cmd");
                seen.push((cmd.request_seq(), cmd.request.id.clone()));
                let resp = JsonRpcResponse::success(Value::from(1u64), serde_json::json!({}));
                cmd.response_tx.send(resp).expect("send resp");
            }
            seen
        });
        for _ in 0..2 {
            injector
                .dispatch("echo.method", serde_json::json!({}), Duration::from_secs(2))
                .expect("dispatch ok");
        }
        let seen = h.join().unwrap();
        assert_eq!(seen[0].1, Some(Value::from(1u64)));
        assert_eq!(seen[0].1, seen[1].1, "the rpc id does not tell them apart");
        assert!(seen[0].0 < seen[1].0, "{:?}", seen);
    }
}
