//! agent task 의 결과 제출과 provider 턴 보고. 둘 다 회차에 결과를 제안할 뿐이고, 검증과
//! 종결은 러너가 턴 표를 읽어 완료 보고로 한다.

use serde_json::Value;
use tasty_agent::task::contract::{FailureStage, TaskFailure};
use tasty_agent::{AgentError, SubmissionRejection, TaskCommand, TaskId, TaskState, TaskStore};
use tasty_memory::HOST_OWNER;

use crate::agent_turns::{ReportOutcome, SubmitOutcome, TurnEvent};
use crate::{TaskScope, TaskService};

/// 제출한 쪽. 세션 토큰을 가진 agent 는 자기 세션의 회차에만 낼 수 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Submitter {
    /// 사용자 CLI 또는 호출 검사를 통과한 플러그인.
    Trusted,
    /// 세션 토큰으로 확인한 agent 세션. 값은 그 세션의 surface 다.
    Session(u32),
}

/// 제출이 가리키는 회차. 토큰은 지시에 실린 회차 토큰이고, 다른 회차·다른 호출자의 실수
/// 제출을 막는다(보안 경계가 아니다).
#[derive(Debug, Clone, Copy)]
pub struct AttemptRef<'a> {
    pub id: &'a str,
    pub token: &'a str,
}

impl TaskService {
    /// agent task 회차에 결과를 제출한다. 출력 타입으로 바로 검증하고, 받은 값은 턴이 끝날 때
    /// 러너가 결과로 확정한다. 제출 성공은 task 의 성공이 아니다.
    pub fn agent_submit_result(
        &self,
        scope: &TaskScope,
        workspace_id: u32,
        task_id: &TaskId,
        attempt: AttemptRef<'_>,
        output: &Value,
        submitter: Submitter,
    ) -> Result<SubmitOutcome, AgentError> {
        let seq = scope.agent_seq().clone();
        let id = task_id.clone();
        let task = self.with_memory(move |mem| {
            TaskStore::new(mem, HOST_OWNER, seq.as_ref()).get(workspace_id, &id)
        })?;
        self.agent_submit_checked(task, workspace_id, task_id, attempt, output, submitter)
    }

    /// 저장소에서 읽은 task 로 제출을 판정하고 턴에 넣는다. 저장소는 읽지 않는다. IPC 는 task 를
    /// writer 요청으로 읽은 뒤 메인 루프에서 이 함수를 부른다.
    pub fn agent_submit_checked(
        &self,
        task: Option<tasty_agent::Task>,
        workspace_id: u32,
        task_id: &TaskId,
        attempt: AttemptRef<'_>,
        output: &Value,
        submitter: Submitter,
    ) -> Result<SubmitOutcome, AgentError> {
        let attempt_id = attempt.id;
        let task = task.ok_or_else(|| AgentError::TaskNotFound(task_id.clone()))?;
        let current = task.attempt.as_ref().map(|a| a.id.clone());
        let reject = |reason, current_attempt_id: Option<String>| AgentError::SubmissionRejected {
            task_id: task_id.clone(),
            attempt_id: attempt_id.to_string(),
            current_attempt_id,
            reason,
        };
        let contract = match (&task.command, &task.contract, &task.state) {
            (TaskCommand::Agent { .. }, Some(c), TaskState::Running) => c,
            _ => return Err(reject(SubmissionRejection::NotRunning, current)),
        };
        if current.as_deref() != Some(attempt_id) {
            return Err(reject(SubmissionRejection::StaleAttempt, current));
        }
        let Some((surface, _)) = self.agent_turns().find(workspace_id, task_id) else {
            return Err(reject(SubmissionRejection::NotRunning, current));
        };
        if matches!(submitter, Submitter::Session(s) if s != surface) {
            return Err(reject(SubmissionRejection::NotTheSession, current));
        }
        let schema = contract.output_schema(&task.command);
        let typed = contract
            .defs()
            .validate_typed(&schema, output)
            .map_err(|e| {
                AgentError::TypeContract(Box::new(TaskFailure::typed(
                    FailureStage::OutputValidation,
                    Some(task_id.clone()),
                    e,
                )))
            })?;
        self.agent_turns()
            .submit(
                workspace_id,
                task_id,
                attempt_id,
                attempt.token,
                typed.to_wire(),
            )
            .map_err(|(reason, current)| reject(reason, current))
    }

    /// `provider` 플러그인이 보고한 턴의 시작·종료를 그 surface 에 묶인 회차에 적용한다.
    pub fn agent_turn_report(
        &self,
        surface: u32,
        provider: &str,
        event: TurnEvent,
    ) -> ReportOutcome {
        self.agent_turns().report(surface, provider, event)
    }
}
