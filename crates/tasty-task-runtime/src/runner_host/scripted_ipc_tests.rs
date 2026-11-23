//! 시험용 모의 host IPC. 명령을 보낸 쪽이 응답 대기 전에 부르는 waker 안에서, 정해 둔 순서나
//! 응답자 함수대로 명령을 처리한다.
//!
//! waker 는 공유 명령 큐를 잠그고 비우므로, 송신이 겹치면 먼저 잠금을 잡은 송신 스레드가 다른 송신의
//! 명령까지 처리한다. 어느 쪽이든 처리 주체는 송신 경로의 스레드이고, 응답 대기 상한은 자기 waker 가
//! 돌아온 뒤에 흐르므로 claim 과 즉시 응답이 그 상한과 경쟁하지 않는다. 늦은 응답과 채널 종료는 시험
//! 본체가 첫 dispatch·poll 이 반환한 뒤에 한다. 남는 전제는 큐 기한이 명령을 만든 시각부터 흐르므로,
//! 명령 생성부터 waker 안의 잠금·`try_recv`·단계 선택을 거쳐 `IpcCommand::claim` 안의 큐 경과
//! 판정까지가 그 기한(일반 호출 5초)보다 짧다는 것이다.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};

use serde_json::{Value, json};
use tasty_ipc::dispatch::DispatchStats;
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::server::{Claim, IpcCommand};

use super::RunnerContext;

/// 명령 하나를 처리하는 방식.
pub(crate) enum Step {
    /// claim 하지 않고 시험에 넘긴다 — 큐 대기 중 만료를 재현한다.
    Leave,
    /// 시작시키고 답하지 않은 채 넘긴다. 시험이 쥐고 있는 동안 응답 송신자가 살아 있다.
    Hold,
    /// 시작시키고 그 자리에서 이 값으로 답한다. 함수 안에서 요청을 단언할 수 있다.
    Reply(Box<dyn Fn(&IpcCommand) -> Value + Send>),
    /// 시작시키고 그 자리에서 이 RPC 오류로 답한다. 쓰는 시험(poll_interval_tests)이 unix 전용이다.
    #[cfg(unix)]
    Error(i32, String),
    /// 시작 전에 철회하고 시작 전 만료 오류로 답한다. 호스트가 큐에서 만료된 명령을 철회한 뒤 실행 없이
    /// 보내는 답이다.
    #[cfg(unix)]
    ExpiredBeforeRun,
}

impl Step {
    /// 늘 같은 값으로 답하는 [`Step::Reply`].
    pub(crate) fn answer(value: Value) -> Self {
        Self::Reply(Box::new(move |_| value.clone()))
    }
}

/// [`install`] 이 붙인 모의 IPC. `rx` 로 Leave·Hold 한 명령을 받는다.
pub(crate) struct Scripted {
    pub(crate) rx: mpsc::Receiver<IpcCommand>,
    steps: Arc<Mutex<VecDeque<Step>>>,
}

impl Scripted {
    /// 예정한 명령이 모두 왔는지 확인한다. 재전송·후속 조회가 빠지면 단계가 남는다.
    pub(crate) fn assert_all_steps_used(&self) {
        let left = self.steps.lock().expect("steps lock").len();
        assert_eq!(left, 0, "{left} scripted command(s) never arrived");
    }
}

fn claim(cmd: &IpcCommand) -> Claim {
    cmd.claim(&Arc::new(DispatchStats::default()))
}

/// `cmd` 에 성공 응답을 보낸다. 시험 본체가 Hold 한 명령에 답할 때도 쓴다.
pub(crate) fn reply(cmd: IpcCommand, result: Value) {
    cmd.response_tx
        .send(JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(Value::Null),
            result,
        ))
        .expect("send resp");
}

#[cfg(unix)]
fn respond_with(cmd: IpcCommand, code: i32, message: &str) {
    cmd.response_tx
        .send(JsonRpcResponse::error(
            cmd.request.id.clone().unwrap_or(Value::Null),
            code,
            message,
        ))
        .expect("send error resp");
}

/// `answer_state_idle` 이면 `.state` 로 끝나는 명령은 단계를 쓰지 않고 idle 로 바로 답한다.
/// 그 밖의 명령은 단계를 하나씩 쓰며, 단계가 다 떨어진 뒤 온 명령은 보낸 스레드에서 실패한다.
/// `answer_state_idle` 이면 `.state` 는 단계 큐를 보기 전에 답하므로 단계가 떨어진 뒤에도 실패하지
/// 않는다. 꺼져 있으면 `.state` 도 단계를 쓴다.
pub(crate) fn install(ctx: &RunnerContext, steps: Vec<Step>, answer_state_idle: bool) -> Scripted {
    let steps = Arc::new(Mutex::new(VecDeque::from(steps)));
    let script = steps.clone();
    let rx = attach(ctx, move |cmd| {
        if answer_state_idle && cmd.request.method.ends_with(".state") {
            return Step::answer(json!({ "state": "idle" }));
        }
        let step = script.lock().expect("steps lock").pop_front();
        step.unwrap_or_else(|| panic!("unexpected command {}", cmd.request.method))
    });
    Scripted { rx, steps }
}

/// 명령마다 `respond` 가 고른 단계로 처리한다. 명령 수가 정해지지 않은 모의(상태 반복 조회, 메서드별
/// 응답, 호출 기록)에 쓴다. 정해 둔 단계 큐가 없으므로 `assert_all_steps_used` 는 늘 통과한다.
pub(crate) fn install_responder(
    ctx: &RunnerContext,
    respond: impl Fn(&IpcCommand) -> Step + Send + 'static,
) -> Scripted {
    Scripted {
        rx: attach(ctx, respond),
        steps: Arc::new(Mutex::new(VecDeque::new())),
    }
}

/// `IpcWaker` 는 `Sync` 여야 하므로 `respond` 를 명령 수신자와 같은 잠금 안에 두어 호출을 직렬화한다.
fn attach(
    ctx: &RunnerContext,
    respond: impl Fn(&IpcCommand) -> Step + Send + 'static,
) -> mpsc::Receiver<IpcCommand> {
    let (tx, cmds) = mpsc::channel::<IpcCommand>();
    let (fwd_tx, fwd_rx) = mpsc::channel::<IpcCommand>();
    let inbox = Mutex::new((cmds, respond));
    let waker = Arc::new(move || {
        let inbox = inbox.lock().expect("commands lock");
        let (cmds, respond) = &*inbox;
        for cmd in cmds.try_iter() {
            match respond(&cmd) {
                Step::Leave => fwd_tx.send(cmd).expect("the test holds the receiver"),
                Step::Hold => {
                    assert_eq!(claim(&cmd), Claim::Run);
                    fwd_tx.send(cmd).expect("the test holds the receiver");
                }
                Step::Reply(answer) => {
                    assert_eq!(claim(&cmd), Claim::Run);
                    let result = answer(&cmd);
                    reply(cmd, result);
                }
                #[cfg(unix)]
                Step::Error(code, message) => {
                    assert_eq!(claim(&cmd), Claim::Run);
                    respond_with(cmd, code, &message);
                }
                #[cfg(unix)]
                Step::ExpiredBeforeRun => {
                    use tasty_ipc::server::Withdraw;
                    assert_eq!(cmd.lifecycle().withdraw(), Withdraw::NotRun);
                    respond_with(
                        cmd,
                        tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN,
                        "expired before run",
                    );
                }
            }
        }
    });
    ctx.host_ipc
        .set(HostIpcInjector::new(tx, waker))
        .ok()
        .expect("set once");
    fwd_rx
}
