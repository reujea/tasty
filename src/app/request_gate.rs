//! 요청 게이트의 저장 결과를 기다리는 요청.
//!
//! 메인 루프는 writer 결과를 기다리지 않고 회차마다 맨 앞 요청만 확인한다. 맨 앞 요청이 끝나기 전에는
//! 뒤 요청을 시작하지 않으므로 IPC 명령과 plugin 호출이 접수 순서대로 실행된다. 앞 요청이 바꾼 상태를
//! 뒤 요청의 게이트가 본다(예: 세션 철회 뒤의 같은 토큰 요청).
//!
//! 예외가 하나 있다 — plugin 에 바쁜지 묻고 답을 기다리는 요청은 건너뛴다
//! ([`crate::ipc::handler::gate::GateProgress::waiting_on_plugin`]). 그 답은 plugin 프로세스가
//! 보내는 것이라 **뒤 요청이 바꾸는 호스트 상태에 기대지 않는다**. 반대로 건너뛰지 않으면 두 가지가
//! 깨진다. (1) 답하지 않는 plugin 하나가 `PROBE_TIMEOUT` 동안 모든 IPC 와 plugin 호출을 멈춘다.
//! (2) `lifecycle_busy` 처리기가 답하기 전에 호스트를 부르면 그 호출이 이 줄 뒤에 서서 영원히
//! 진행되지 못하고, 기한이 지나 "바쁘지 않음"으로 판정된다 — 호스트 상태를 보고 busy 를 정하는
//! plugin 에서는 질문이 항상 무력해진다.
//!
//! 건너뛴 요청은 줄에서 자리를 잃지 않는다. 답이 오거나 기한이 지나면 그 자리에서 다시 진행한다.

use std::collections::VecDeque;

use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::services::AppServices;
use crate::ipc::handler::gate::GateProgress;
use crate::ipc::protocol::JsonRpcRequest;
use crate::ipc::server::IpcCommand;

/// 게이트를 진행 중인 요청과 그 진행 상태.
pub(crate) enum Gated {
    Ipc {
        cmd: IpcCommand,
        progress: GateProgress,
    },
    Plugin {
        call: PendingPluginCall,
        request: JsonRpcRequest,
        progress: GateProgress,
    },
}

impl Gated {
    fn progress_mut(&mut self) -> &mut GateProgress {
        match self {
            Self::Ipc { progress, .. } | Self::Plugin { progress, .. } => progress,
        }
    }
}

#[derive(Default)]
pub(crate) struct GateQueue {
    waiting: VecDeque<Gated>,
}

impl GateQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// 새 요청은 뒤에 둔다. 맨 앞에서 꺼내 진행하다 다시 기다리게 된 요청은 앞자리로 돌려놓는다.
    pub(crate) fn park(&mut self, gated: Gated, resumed: bool) {
        if resumed {
            self.waiting.push_front(gated);
        } else {
            self.waiting.push_back(gated);
        }
    }

    /// 맨 앞 요청이 기다리던 결과가 나왔거나 아직 시작하지 않은 요청이면 꺼낸다.
    ///
    /// plugin 의 busy 답을 기다리는 요청은 건너뛰고 그 뒤를 본다(모듈 문서의 예외). 그래야
    /// 답하지 않는 plugin 이 IPC 전체를 멈추지 않고, `lifecycle_busy` 처리기가 호스트를 불러도
    /// 그 호출이 질문 뒤에 갇히지 않는다.
    pub(crate) fn pop_ready(&mut self, core: &AppServices) -> Option<Gated> {
        for index in 0..self.waiting.len() {
            let gated = self.waiting.get_mut(index).expect("index < len");
            if gated.progress_mut().poll(core) {
                return self.waiting.remove(index);
            }
            if !gated.progress_mut().waiting_on_plugin() {
                // busy 질문이 아닌 대기는 종전대로 뒤를 막는다 — 뒤 요청이 앞 요청의 결과를 본다.
                return None;
            }
        }
        None
    }

    /// 종료가 시작되면 기다리던 요청에 종료 응답을 보내고 실행하지 않는다. 정리 중인 engine을
    /// 건드리지 않기 위해서다. 이미 넣은 게이트 요청(세션 정리·rate limit 소비)은 확정될 수 있다.
    pub(crate) fn reject_all(
        &mut self,
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> usize {
        let rejected = self.waiting.len();
        for gated in self.waiting.drain(..) {
            match gated {
                Gated::Ipc { cmd, .. } => {
                    let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
                    crate::ipc::server::send_response(
                        &cmd.response_tx,
                        crate::ipc::handler::gate::shutting_down(id),
                    );
                }
                Gated::Plugin { call, .. } => {
                    if let Some(manager) = plugins.as_deref_mut() {
                        manager.send_plugin_call_result(
                            &call,
                            None,
                            Some("host is shutting down".to_string()),
                            Some(-32000),
                        );
                    }
                }
            }
        }
        rejected
    }
}
