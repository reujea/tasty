//! 시험용 모의 host IPC. 명령을 보낸 스레드가 응답 대기 전에 부르는 waker 안에서, 정해 둔 순서대로
//! 명령을 처리한다.
//!
//! 응답 대기 상한은 waker 가 돌아온 뒤에 흐르므로 claim 과 즉시 응답이 그 상한과 경쟁하지 않는다.
//! 늦은 응답과 채널 종료는 시험 본체가 첫 dispatch·poll 이 반환한 뒤에 한다. 남는 전제는 큐 기한이
//! 명령을 만든 시각부터 흐르므로(`IpcCommand::claim`), 같은 스레드의 명령 생성부터 waker 까지의
//! 몇 줄이 그 기한(5초)보다 짧다는 것이다.

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

fn reply(cmd: IpcCommand, result: Value) {
    cmd.response_tx
        .send(JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(Value::Null),
            result,
        ))
        .expect("send resp");
}

/// `answer_state_idle` 이면 `.state` 로 끝나는 명령은 단계를 쓰지 않고 idle 로 바로 답한다.
/// 단계가 다 떨어진 뒤 온 명령은 보낸 스레드에서 실패한다.
pub(crate) fn install(ctx: &RunnerContext, steps: Vec<Step>, answer_state_idle: bool) -> Scripted {
    let (tx, cmds) = mpsc::channel::<IpcCommand>();
    let (fwd_tx, fwd_rx) = mpsc::channel::<IpcCommand>();
    let steps = Arc::new(Mutex::new(VecDeque::from(steps)));
    let cmds = Mutex::new(cmds);
    let script = steps.clone();
    let waker = Arc::new(move || {
        let cmds = cmds.lock().expect("commands lock");
        while let Ok(cmd) = cmds.try_recv() {
            if answer_state_idle && cmd.request.method.ends_with(".state") {
                assert_eq!(claim(&cmd), Claim::Run);
                reply(cmd, json!({ "state": "idle" }));
                continue;
            }
            let step = script.lock().expect("steps lock").pop_front();
            match step {
                Some(Step::Leave) => fwd_tx.send(cmd).expect("the test holds the receiver"),
                Some(Step::Hold) => {
                    assert_eq!(claim(&cmd), Claim::Run);
                    fwd_tx.send(cmd).expect("the test holds the receiver");
                }
                Some(Step::Reply(answer)) => {
                    assert_eq!(claim(&cmd), Claim::Run);
                    let result = answer(&cmd);
                    reply(cmd, result);
                }
                None => panic!("unexpected command {}", cmd.request.method),
            }
        }
    });
    ctx.host_ipc
        .set(HostIpcInjector::new(tx, waker))
        .ok()
        .expect("set once");
    Scripted { rx: fwd_rx, steps }
}
