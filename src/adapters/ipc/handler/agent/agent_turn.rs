//! agent task 회차에 결과를 제안하는 두 경로. 명시 제출(`agent.task_submit_result`)과 provider
//! 턴 보고(`agent.task_turn_report`). 둘 다 task 를 끝내지 않는다(종결은 러너가 한다).

use serde_json::{Value, json};
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_task_runtime::agent_task::{AttemptRef, Submitter};
use tasty_task_runtime::agent_turns::{
    ReportOutcome, StartPrompt, SubmitOutcome, TurnEnd, TurnEvent,
};

use crate::app::services::AppServices;
use crate::runtime::engine_access::EngineMut;

use super::{reply_task, task_id_param, workspace_id_param};

fn required_str<'a>(params: &'a Value, key: &str, id: &Value) -> Result<&'a str, JsonRpcResponse> {
    params.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
        JsonRpcResponse::invalid_params(id.clone(), format!("Missing required '{key}'"))
    })
}

/// 시작 보고의 프롬프트 정보. `prompt_seen` 이 없으면 provider 가 프롬프트를 받지 못한 것이다.
fn start_prompt(params: &Value) -> StartPrompt {
    if params.get("prompt_seen").and_then(Value::as_bool) != Some(true) {
        tracing::debug!("agent turn start without a prompt; the attempt marker is not checked");
        return StartPrompt::Unknown;
    }
    StartPrompt::Seen(
        params
            .get("attempt_marker")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    )
}

/// 세션 토큰의 agent id(`<provider>_s<surface>`)에서 세션 surface 를 읽는다.
fn session_surface(agent_id: &str) -> Option<u32> {
    let (_, surface) = agent_id.rsplit_once("_s")?;
    surface.parse().ok()
}

/// 호출자가 회차에 쓸 수 있는 범위. 세션 토큰의 agent 는 자기 세션의 회차로 묶이고, 사용자
/// CLI·플러그인은 묶이지 않는다. 세션이 아닌 agent id 면 그 id 를 돌려준다.
pub(super) fn session_writer(caller: &CallerContext) -> Result<Submitter, &str> {
    match caller {
        CallerContext::Agent { agent_id, .. } => session_surface(agent_id)
            .map(Submitter::Session)
            .ok_or(agent_id.as_str()),
        CallerContext::Local { .. } | CallerContext::Plugin { .. } => Ok(Submitter::Trusted),
    }
}

/// 같은 회차의 구조화 결과를 제출한다. 출력 타입으로 검증하고 받은 값은 턴이 끝날 때 결과가
/// 된다. 응답 `final: false` 는 task 가 아직 끝나지 않았다는 뜻이다.
pub fn task_submit(
    core: &AppServices,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let workspace_id = match workspace_id_param(params, &id) {
        Ok(w) => w,
        Err(e) => return e,
    };
    let task_id = match task_id_param(params, &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let attempt_id = match required_str(params, "attempt_id", &id) {
        Ok(a) => a.to_string(),
        Err(e) => return e,
    };
    let token = match required_str(params, "token", &id) {
        Ok(t) => t.to_string(),
        Err(e) => return e,
    };
    let Some(output) = params.get("output") else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'output'");
    };
    let submitter = match session_writer(caller) {
        Ok(s) => s,
        Err(agent_id) => {
            return JsonRpcResponse::error(
                id,
                -32001,
                format!("agent '{agent_id}' is not a session that can submit a task result"),
            );
        }
    };
    let scope = engine.task_scope.clone();
    let output = output.clone();
    let task_id_for_check = task_id.clone();
    reply_task(
        core,
        id,
        params,
        move |tx| tx.task_get(&scope, workspace_id, &task_id),
        // 턴 상태는 메모리에 있어 메인 루프에서 판정한다. 읽은 task 는 writer 순서 안의 값이다.
        move |core, id, task| match core.tasks.agent_submit_checked(
            task,
            workspace_id,
            &task_id_for_check,
            AttemptRef {
                id: &attempt_id,
                token: &token,
            },
            &output,
            submitter,
        ) {
            Ok(outcome) => JsonRpcResponse::success(
                id,
                json!({
                    "accepted": true,
                    "duplicate": outcome == SubmitOutcome::Duplicate,
                    "final": false,
                }),
            ),
            Err(error) => super::agent_err_to_response(id, error),
        },
    )
}

/// provider 플러그인의 턴 시작·종료 보고. 호출한 플러그인이 `provider` namespace 를 소유해야
/// 한다. surface 에 묶인 회차가 없으면 아무것도 하지 않는다.
pub fn task_turn_report(
    core: &AppServices,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let provider = match required_str(params, "provider", &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let owns = matches!(caller, CallerContext::Plugin { plugin_id, .. }
        if tasty_ipc::method_meta::plugin_owns_prefix(plugin_id, provider));
    if !owns {
        return JsonRpcResponse::error(
            id,
            -32001,
            format!("only the plugin that owns the '{provider}' namespace reports its turns"),
        );
    }
    let surface = match super::super::params::require_u32(params, "surface_id", &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let event = match required_str(params, "event", &id) {
        Ok("turn_started") => TurnEvent::Started(start_prompt(params)),
        Ok("turn_ended") => {
            let text = |k: &str| params.get(k).and_then(|v| v.as_str()).map(str::to_string);
            TurnEvent::Ended(match text("error") {
                Some(e) => TurnEnd::Error(e),
                None => TurnEnd::Answer(text("final_answer")),
            })
        }
        Ok(other) => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("invalid 'event': {other} (expected turn_started or turn_ended)"),
            );
        }
        Err(e) => return e,
    };
    let kind = match event {
        TurnEvent::Started(_) => "turn_started",
        TurnEvent::Ended(_) => "turn_ended",
    };
    let body = match core.tasks.agent_turn_report(surface, provider, event) {
        ReportOutcome::Unbound => json!({ "bound": false }),
        ReportOutcome::Applied { task, attempt } => {
            // 성공한 보고는 응답 말고는 흔적이 없다. 실제 실행 진단에서 보고 횟수와 귀속을 셀 수 있게 남긴다.
            tracing::debug!(
                "agent turn report applied: {provider} surface {surface} {kind} -> task {task} attempt {attempt}"
            );
            json!({ "bound": true, "applied": true, "task_id": task, "attempt_id": attempt })
        }
        ReportOutcome::Ignored(why) => json!({ "bound": true, "applied": false, "reason": why }),
    };
    JsonRpcResponse::success(id, body)
}

#[cfg(test)]
mod tests {
    use super::{StartPrompt, session_surface, start_prompt};
    use serde_json::json;

    #[test]
    fn a_start_report_says_whether_its_prompt_carried_a_marker() {
        assert_eq!(start_prompt(&json!({})), StartPrompt::Unknown);
        assert_eq!(
            start_prompt(&json!({ "prompt_seen": true, "attempt_marker": null })),
            StartPrompt::Seen(None)
        );
        assert_eq!(
            start_prompt(&json!({ "prompt_seen": true, "attempt_marker": "ab" })),
            StartPrompt::Seen(Some("ab".into()))
        );
    }

    /// 작업 실행기가 붙이는 회차 표지와 플러그인 계약의 표지가 같다. `tasty-agent`는 계층 규칙상
    /// protocol에 의존하지 않아 값을 따로 두므로, 둘을 다 의존하는 여기서 대조한다.
    #[test]
    fn the_host_marker_prefix_is_the_protocol_contract() {
        assert_eq!(
            tasty_agent::task::agent::ATTEMPT_MARKER_PREFIX,
            tasty_plugin_protocol::task_turn::ATTEMPT_MARKER_PREFIX
        );
    }

    #[test]
    fn the_session_surface_comes_from_the_issued_agent_id() {
        assert_eq!(session_surface("claude_s12"), Some(12));
        assert_eq!(session_surface("child:1"), None);
        assert_eq!(session_surface("claude_sx"), None);
    }
}
