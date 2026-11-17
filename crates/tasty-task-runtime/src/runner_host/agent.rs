//! agent task 실행. provider 플러그인의 spawn·tell·state 를 재사용하고, 턴의 귀속과 결과는
//! [`crate::agent_turns`] 의 표로 판단한다. 사용자 프로세스를 끝내지 않고, 세션은 회차에 묶지
//! 못한 늦은 spawn 세션만 닫는다([`super::agent_spawn`]).

use serde_json::json;
use tasty_agent::runner::{DispatchHandle, PollOutcome};
use tasty_agent::task::FailureCode;
use tasty_agent::task::agent::{self, AgentLink, AgentSession};
use tasty_agent::{Task, TaskCommand, TaskStore};
use tasty_ipc::host_call::{InjectError, LateAnswer};
use tasty_memory::HOST_OWNER;

use super::late_answer::DISPATCH_OUTCOME_UNKNOWN;
use super::{HostExecutor, INJECTOR_UNINIT_MSG, LateCall, PluginCallError, now_ms};
use crate::agent_turns::{TurnBinding, TurnPoll, decide};

/// 회차 토큰. 회차 id 와 달리 예측할 수 없다.
fn new_attempt_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl HostExecutor {
    /// 새 세션은 spawn 으로 지시를 보내고 바로 회차에 묶는다. 기존 세션은 handle 에 지시를 담아
    /// 두고 poll 에서 세션이 비었을 때 보낸다.
    pub(super) fn dispatch_agent(&mut self, task: &Task) -> Result<DispatchHandle, String> {
        let TaskCommand::Agent {
            provider,
            workspace_id,
            session,
            instruction,
            timeout_ms,
        } = &task.command
        else {
            return Err("dispatch_agent: not an agent task".into());
        };
        let unavailable = |detail: String| FailureCode::AgentUnavailable.message(detail);
        // 생성 뒤 플러그인이 비활성화·제거됐을 수 있어 실행 직전에 다시 판정한다.
        agent::check_provider(provider).map_err(unavailable)?;
        let contract = task
            .contract
            .as_ref()
            .ok_or_else(|| unavailable(agent::AGENT_NEEDS_CONTRACT.into()))?;
        // dispatch 는 Running 전이 직전이다. 이 실행이 만들 회차가 턴의 회차다.
        let attempt_id = super::dispatch_attempt(task)
            .ok_or_else(|| unavailable(agent::AGENT_NEEDS_CONTRACT.into()))?;
        let needs_submission = agent::needs_submission(contract, &task.command);
        let attempt_token = new_attempt_token();
        let mut text = task
            .input_snapshot
            .as_ref()
            .and_then(|s| s.execution.instruction.clone())
            .unwrap_or_else(|| instruction.clone());
        if needs_submission {
            let schema = serde_json::to_value(contract.output_schema(&task.command))
                .map_err(|e| unavailable(e.to_string()))?;
            text.push_str("\n\n");
            text.push_str(&agent::submission_instructions(
                task.workspace_id,
                &task.id,
                &attempt_id,
                &attempt_token,
                &schema,
            ));
        }
        if let Some(addr) =
            super::report::report_address(task, tasty_agent::task::report::ReportSource::Agent)
        {
            text.push_str("\n\n");
            text.push_str(&agent::report_line(&addr.to_env_value()));
        }
        text.push_str("\n\n");
        text.push_str(&agent::attempt_marker_line(&attempt_token));
        let deadline_ms = timeout_ms.map(|t| now_ms().saturating_add(t));
        // 회차 기록·제출·턴 표는 task 자신의 workspace 를 쓴다. 명령의 workspace_id 는 새 세션이
        // 뜨는 곳일 뿐이다.
        let handle = |surface_id: u32, pending: Option<String>| DispatchHandle::AgentTurn {
            workspace_id: task.workspace_id,
            task_id: task.id.clone(),
            attempt_id: attempt_id.clone(),
            provider: provider.clone(),
            surface_id,
            needs_submission,
            deadline_ms,
            pending_instruction: pending,
            attempt_token: attempt_token.clone(),
        };
        match session {
            AgentSession::Existing { surface_id } => Ok(handle(*surface_id, Some(text))),
            AgentSession::New {
                parent_surface,
                cwd,
            } => {
                let mut params = json!({
                    "surface": parent_surface,
                    "workspace": workspace_id.to_string(),
                    "prompt": text,
                });
                if let Some(cwd) = cwd {
                    params["cwd"] = json!(cwd);
                }
                let method = format!("{provider}.spawn");
                let inj = self
                    .ctx
                    .host_ipc
                    .get()
                    .ok_or_else(|| unavailable(format!("{method}: {INJECTOR_UNINIT_MSG}")))?
                    .clone();
                let binding = TurnBinding::new(
                    task.workspace_id,
                    task.id.clone(),
                    attempt_id.clone(),
                    provider.clone(),
                    attempt_token.clone(),
                    true,
                );
                Ok(self.start_agent_spawn(task, &inj, method, params, binding, handle(0, None)))
            }
        }
    }

    pub(super) fn poll_agent(&mut self, handle: &DispatchHandle) -> PollOutcome {
        let DispatchHandle::AgentTurn {
            workspace_id,
            task_id,
            attempt_id,
            provider,
            surface_id,
            needs_submission,
            deadline_ms,
            pending_instruction,
            ..
        } = handle
        else {
            return PollOutcome::Failed("poll_agent: not an agent handle".into());
        };
        let now = now_ms();
        if let Some(outcome) = self.poll_late_tell(handle, now) {
            return outcome;
        }
        let state = self.provider_state(provider, *surface_id);
        let binding = match self.ctx.agent_turns.find(*workspace_id, task_id) {
            Some((_, b)) if b.attempt == *attempt_id => b,
            Some(_) => return PollOutcome::Failed("agent turn bound to another attempt".into()),
            None => {
                let Some(text) = pending_instruction else {
                    // 묶음이 사라졌다(재시작 등). 이 회차의 턴을 귀속할 수 없다.
                    return PollOutcome::Failed(
                        FailureCode::AgentUnavailable.message("the turn binding was lost"),
                    );
                };
                return self.send_pending(handle, text, state.as_deref(), now);
            }
        };
        match decide(
            &binding,
            *surface_id,
            *needs_submission,
            state.as_deref(),
            now,
            *deadline_ms,
        ) {
            TurnPoll::Active { awaiting_input } => {
                if let Some(since) =
                    self.ctx
                        .agent_turns
                        .note_awaiting(*workspace_id, task_id, awaiting_input, now)
                {
                    self.record_link(
                        *workspace_id,
                        task_id,
                        attempt_id,
                        provider,
                        *surface_id,
                        since,
                    );
                }
                PollOutcome::Active
            }
            TurnPoll::Done(r) => PollOutcome::Done(r),
            TurnPoll::Failed(e) => PollOutcome::Failed(e),
        }
    }

    /// 기존 세션이 비었으면 회차에 묶고 지시를 보낸다. 다른 회차가 묶었거나 세션이 바쁘면
    /// 다음 poll 에서 다시 본다.
    fn send_pending(
        &mut self,
        handle: &DispatchHandle,
        text: &str,
        state: Option<&str>,
        now: u64,
    ) -> PollOutcome {
        let DispatchHandle::AgentTurn {
            workspace_id,
            task_id,
            attempt_id,
            provider,
            surface_id,
            deadline_ms,
            attempt_token,
            ..
        } = handle
        else {
            return PollOutcome::Active;
        };
        match state {
            Some("exited") => {
                return PollOutcome::Failed(FailureCode::AgentExited.message(format!(
                    "surface {surface_id} exited before the instruction was sent"
                )));
            }
            Some("idle") => {}
            // 사용자의 턴이 진행 중이거나 입력을 기다린다. 끼어들지 않는다.
            _ => {
                if deadline_ms.is_some_and(|d| now >= d) {
                    return PollOutcome::Failed(
                        FailureCode::TimedOut.message("the session never became idle"),
                    );
                }
                return PollOutcome::Active;
            }
        }
        let binding = TurnBinding::new(
            *workspace_id,
            task_id.clone(),
            attempt_id.clone(),
            provider.clone(),
            attempt_token.clone(),
            false,
        );
        if self.ctx.agent_turns.bind(*surface_id, binding).is_err() {
            return PollOutcome::Active;
        }
        let method = format!("{provider}.tell");
        let sent = self.ctx.dispatch_plugin_keeping_late_answer(
            &method,
            json!({ "surface_id": surface_id, "message": text }),
        );
        match sent {
            Ok(_) => {}
            // 시작했지만 상한 안에 답이 없다. 지시가 이미 전달됐을 수 있어 다시 보내지 않고 늦은
            // 응답을 기다린다. 묶음은 남겨 그 사이 시작한 이 회차의 턴을 받는다.
            Err(LateCall::Pending(late)) => {
                self.park_late_tell(handle, &method, late);
                return PollOutcome::Active;
            }
            // 큐에서 기다리다 만료된 지시는 전달되지 않았다. 묶음을 풀고 다음 poll 에서 다시 본다.
            Err(LateCall::Failed(PluginCallError::Inject(e @ InjectError::Expired(_)))) => {
                self.ctx.agent_turns.release(*workspace_id, task_id);
                if deadline_ms.is_some_and(|d| now >= d) {
                    return PollOutcome::Failed(
                        FailureCode::TimedOut.message(format!("{method}: {e}")),
                    );
                }
                tracing::warn!("agent task {task_id}: {method}: {e}; sending it again later");
                return PollOutcome::Active;
            }
            Err(LateCall::Failed(e)) => {
                self.ctx.agent_turns.release(*workspace_id, task_id);
                return PollOutcome::Failed(
                    FailureCode::AgentUnavailable.message(format!("{method}: {e}")),
                );
            }
        }
        self.held_turns.insert(task_id.clone(), *workspace_id);
        self.note_told(handle, now);
        PollOutcome::Active
    }

    /// 지시를 보낸 회차의 세션 연결을 기록한다.
    fn note_told(&self, handle: &DispatchHandle, now: u64) {
        let DispatchHandle::AgentTurn {
            workspace_id,
            task_id,
            attempt_id,
            provider,
            surface_id,
            ..
        } = handle
        else {
            return;
        };
        if let Some(since) = self
            .ctx
            .agent_turns
            .note_awaiting(*workspace_id, task_id, false, now)
        {
            self.record_link(
                *workspace_id,
                task_id,
                attempt_id,
                provider,
                *surface_id,
                since,
            );
        }
    }

    /// 상한을 넘긴 tell 의 응답 통로를 보관한다. 저장 handle 은 응답을 기다리는 요청으로 바꿔
    /// 재시작 뒤에는 다시 보내지 않고 결과 불명으로 끝나게 한다.
    fn park_late_tell(&mut self, handle: &DispatchHandle, method: &str, late: LateAnswer) {
        let DispatchHandle::AgentTurn {
            workspace_id,
            task_id,
            attempt_id,
            ..
        } = handle
        else {
            return;
        };
        tracing::warn!(
            "agent task {task_id}: {method} started but did not answer within {:?}; \
             waiting for its late answer instead of sending it again",
            late.timeout()
        );
        self.held_turns.insert(task_id.clone(), *workspace_id);
        self.late.tells.insert(task_id.clone(), late);
        let waiting = DispatchHandle::AwaitDispatchAnswer {
            workspace_id: *workspace_id,
            task_id: task_id.clone(),
            ipc_method: method.to_string(),
        };
        self.persist_handle(*workspace_id, task_id, &waiting, Some(attempt_id));
    }

    /// 늦은 tell 응답을 기다리는 중이면 그 결과. 기다리는 tell 이 없거나 전달됐으면 `None` 이고
    /// 턴 판단을 이어 간다.
    fn poll_late_tell(&mut self, handle: &DispatchHandle, now: u64) -> Option<PollOutcome> {
        let DispatchHandle::AgentTurn {
            workspace_id,
            task_id,
            attempt_id,
            provider,
            deadline_ms,
            ..
        } = handle
        else {
            return None;
        };
        let method = format!("{provider}.tell");
        let Some(answer) = self.late.tells.get(task_id)?.try_take() else {
            if deadline_ms.is_some_and(|d| now >= d) {
                return Some(PollOutcome::Failed(
                    FailureCode::TimedOut
                        .message(format!("{method} did not answer before the deadline")),
                ));
            }
            return Some(PollOutcome::Active);
        };
        self.late.tells.remove(task_id);
        match answer {
            Ok(_) => {
                tracing::info!("agent task {task_id}: {method} answered late");
                self.persist_handle(*workspace_id, task_id, handle, Some(attempt_id));
                self.note_told(handle, now);
                None
            }
            Err(InjectError::Disconnected) => Some(PollOutcome::Lost(format!(
                "{DISPATCH_OUTCOME_UNKNOWN}: '{method}' started but its answer channel closed \
                 without an answer; check whether the session received the instruction before \
                 retrying"
            ))),
            Err(e) => {
                self.ctx.agent_turns.release(*workspace_id, task_id);
                Some(PollOutcome::Failed(
                    FailureCode::AgentUnavailable.message(format!("{method}: {e}")),
                ))
            }
        }
    }

    /// `<provider>.state` 의 상태. 조회하지 못하면 `None` 이다(턴 판단은 보고로 한다).
    fn provider_state(&self, provider: &str, surface_id: u32) -> Option<String> {
        let method = format!("{provider}.state");
        match self
            .ctx
            .dispatch_plugin(&method, json!({ "surface_id": surface_id }))
        {
            Ok(v) => v.get("state").and_then(|s| s.as_str()).map(str::to_string),
            Err(e) => {
                tracing::debug!("agent task {method} s{surface_id}: {e}");
                None
            }
        }
    }

    fn record_link(
        &self,
        workspace_id: u32,
        task_id: &tasty_agent::TaskId,
        attempt_id: &str,
        provider: &str,
        surface_id: u32,
        awaiting_input_since: Option<u64>,
    ) {
        let link = AgentLink {
            provider: provider.to_string(),
            surface_id,
            awaiting_input_since,
        };
        let seq = self.ctx.agent_seq.clone();
        let (id, attempt) = (task_id.clone(), attempt_id.to_string());
        let res = self.ctx.with_memory(move |mem| {
            TaskStore::new(mem, HOST_OWNER, seq.as_ref()).set_agent_link(
                workspace_id,
                &id,
                &attempt,
                link.clone(),
            )
        });
        if let Err(e) = res {
            tracing::warn!("agent task {task_id}: record session link: {e}");
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
