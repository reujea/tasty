//! 1급 Agent task 의 실행 대상과 회차별 세션 연결.
//!
//! 호스트가 provider 중립 계약을 소유한다. provider 의 플러그인은 기존 spawn·tell·state 와
//! 턴 종료 보고로 실행과 관측을 맡고, 결과 검증과 종결은 호스트가 한다.

use serde::{Deserialize, Serialize};

/// provider 가 턴의 시작·종료와 최종 답변을 호스트에 보고하는 데 필요한 권한.
pub const TURN_REPORT_PERMISSION: &str = "agent.turn_report";

/// 결과 수집을 지원하는 provider 인지 판정한다. provider 는 IPC namespace 이름이며, 그 namespace 를
/// 소유한 설치 플러그인이 활성이고 `agent.turn_report` 를 허가받았으면 자격이 있다. 플러그인
/// 프로세스가 아직 준비되지 않았어도 자격은 같다(첫 호출이 프로세스를 띄운다).
/// 자격이 없으면 원인(소유자 없음·비활성·권한 없음·표 미발행)을 구분한 문구를 돌려준다.
pub fn check_provider(provider: &str) -> Result<(), String> {
    use tasty_utils::plugin_namespaces::{Lookup, lookup};
    let reason = match lookup(provider) {
        Lookup::Owner(owner) if !owner.enabled => format!(
            "plugin '{}' that owns the '{provider}' namespace is disabled",
            owner.plugin_id
        ),
        Lookup::Owner(owner) if !owner.permissions.contains(TURN_REPORT_PERMISSION) => format!(
            "plugin '{}' that owns the '{provider}' namespace does not have the \
             {TURN_REPORT_PERMISSION} permission, so it cannot report turns",
            owner.plugin_id
        ),
        Lookup::Owner(_) => return Ok(()),
        Lookup::NoOwner => format!("no installed plugin owns the '{provider}' namespace"),
        Lookup::NotPublished => "plugin information is not loaded yet".to_string(),
    };
    Err(format!(
        "agent provider '{provider}' does not support result collection: {reason}"
    ))
}

/// v1 생성 경로가 agent task 를 거절하는 문구.
pub const AGENT_NEEDS_CONTRACT: &str =
    "agent tasks need contract_version 2; submit them with agent.task_graph_submit";

/// agent 가 일할 세션.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSession {
    /// provider 의 spawn 으로 새 세션을 연다. 새 surface 는 `parent_surface` 의 자식이 된다.
    New {
        parent_surface: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// 이미 떠 있는 세션(surface)에 지시를 보낸다.
    Existing { surface_id: u32 },
}

/// v2 회차가 연결된 agent 세션과 입력 대기 상태.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLink {
    pub provider: String,
    pub surface_id: u32,
    /// 세션이 사용자 입력(승인 등)을 기다리기 시작한 시각. 기다리지 않으면 없다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awaiting_input_since: Option<u64>,
}

/// agent 출력 확정에 쓰는 실행 보고(`TaskResult.output`)의 필드.
pub mod report {
    /// 턴의 최종 답변(string). provider 가 보고한 마지막 assistant 메시지다.
    pub const FINAL_ANSWER: &str = "final_answer";
    /// 같은 회차에 명시 제출한 구조화 결과(wire 형식).
    pub const SUBMITTED: &str = "submitted";
    pub const PROVIDER: &str = "provider";
    pub const SURFACE_ID: &str = "surface_id";
    /// 결과 없이 끝난 회차에서 `final_answer` 를 [`super::ANSWER_EXCERPT_CHARS`] 로 자른 경우 true.
    pub const FINAL_ANSWER_TRUNCATED: &str = "final_answer_truncated";
}

/// 결과 없이 끝난 회차의 기록에 남기는 마지막 답의 최대 문자 수. 진단용 발췌라 출력 크기
/// 상한(256KiB)보다 훨씬 작게 둔다.
pub const ANSWER_EXCERPT_CHARS: usize = 2000;

/// 마지막 답의 앞부분 발췌와 잘렸는지 여부.
pub fn answer_excerpt(answer: &str) -> (String, bool) {
    match answer.char_indices().nth(ANSWER_EXCERPT_CHARS) {
        Some((cut, _)) => (answer[..cut].to_string(), true),
        None => (answer.to_string(), false),
    }
}

/// 생성 시 agent command 를 검사한다.
pub(crate) fn check_command(
    provider: &str,
    instruction: &str,
    timeout_ms: Option<u64>,
) -> Result<(), String> {
    check_provider(provider)?;
    if instruction.trim().is_empty() {
        return Err("agent instruction must not be empty".into());
    }
    if timeout_ms == Some(0) {
        return Err("agent timeout_ms must be positive".into());
    }
    Ok(())
}

/// 출력 타입이 명시 제출을 요구하는가. string 출력(기본)은 최종 답변으로 채울 수 있고,
/// 그 밖의 타입은 같은 회차의 제출이 있어야 한다. 후처리가 있으면 후처리가 출력을 만든다.
pub fn needs_submission(contract: &super::TaskContract, command: &super::TaskCommand) -> bool {
    if contract.postprocess.is_some() {
        return false;
    }
    let schema = contract.output_schema(command);
    !matches!(
        contract.defs().resolve(&schema).map(|r| r.kind.clone()),
        Ok(super::types::TypeKind::String { .. })
    )
}

/// 실행 보고에서 출력 후보와 그 출처를 고른다. 명시 제출이 최종 답변보다 우선하고, 제출이
/// 필요한 출력이면 최종 답변은 후보가 아니다(기록용 발췌일 뿐이다).
pub(crate) fn candidate(
    report: Option<&serde_json::Value>,
    needs_submission: bool,
) -> Option<(serde_json::Value, &'static str)> {
    let report = report?;
    if let Some(v) = report.get(report::SUBMITTED) {
        return Some((v.clone(), "agent.submitted"));
    }
    if needs_submission {
        return None;
    }
    report
        .get(report::FINAL_ANSWER)
        .filter(|v| v.is_string())
        .map(|v| (v.clone(), "agent.final_answer"))
}

/// 지시 끝에 붙이는 회차 표지의 앞부분. provider 플러그인이 프롬프트에서 같은 앞부분으로 토큰을
/// 찾는다. 계약 값은 `tasty_plugin_protocol::task_turn::ATTEMPT_MARKER_PREFIX`이고, 이 크레이트는
/// 계층 규칙상 protocol에 의존하지 않아 같은 값을 둔다. 루트 패키지의 시험이 두 값을 대조한다.
pub const ATTEMPT_MARKER_PREFIX: &str = "[tasty-task-attempt:";

/// 지시의 마지막 줄. 세션이 받은 프롬프트가 이 회차의 지시인지 가리는 데 쓴다.
pub fn attempt_marker_line(attempt_token: &str) -> String {
    format!("Tasty task attempt marker (no action needed): {ATTEMPT_MARKER_PREFIX}{attempt_token}]")
}

/// 입력 블록의 머리말. 세션이 지시와 입력을 구별할 수 있게 고정 문구로 둔다.
pub const INPUT_BLOCK_HEADER: &str = "Task input (JSON):";

/// 세션에 보낼 지시. 입력이 있으면 지시 뒤에 JSON 블록으로 붙인다. 값은 다시 해석하지 않는다.
pub fn compose_instruction(instruction: &str, input: Option<&serde_json::Value>) -> String {
    match input {
        Some(v) => {
            let block = serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
            format!("{instruction}\n\n{INPUT_BLOCK_HEADER}\n```json\n{block}\n```")
        }
        None => instruction.to_string(),
    }
}

/// agent task 회차의 세부 단계(`executing`·`awaiting_input`). 후처리 단계는
/// [`super::Task::postprocess_phase`] 가 맡는다. agent task 가 Running 이 아니면 없다.
pub fn phase(task: &super::Task) -> Option<&'static str> {
    if !matches!(task.command, super::TaskCommand::Agent { .. })
        || !matches!(task.state, super::TaskState::Running)
    {
        return None;
    }
    let awaiting = task
        .attempt
        .as_ref()
        .and_then(|a| a.agent.as_ref())
        .is_some_and(|l| l.awaiting_input_since.is_some());
    Some(if awaiting {
        "awaiting_input"
    } else {
        "executing"
    })
}

/// 회차 지시문에 붙이는 report 안내 한 줄. 사용법 전문은 붙이지 않고 그것을 출력하는 명령과
/// 이 회차의 주소만 준다.
pub fn report_line(address: &str) -> String {
    format!(
        "If you need to record notes in this task's report, run `tasty agent report usage` to read how. Report address: {address}"
    )
}

/// string 이 아닌 출력을 요구하는 회차에 붙이는 제출 안내. 세션은 이 명령으로 결과를 낸다.
pub fn submission_instructions(
    workspace_id: u32,
    task_id: &str,
    attempt_id: &str,
    attempt_token: &str,
    output_schema: &serde_json::Value,
) -> String {
    let schema = serde_json::to_string(output_schema).unwrap_or_default();
    format!(
        "When you have finished, submit your result before ending your turn:\n\
         tasty agent task-submit --workspace {workspace_id} --id '{task_id}' --attempt-id '{attempt_id}' --token '{attempt_token}' --output '<JSON>'\n\
         The result must be JSON of this Tasty type: {schema}\n\
         If the command reports a type error, fix the JSON and submit again."
    )
}
