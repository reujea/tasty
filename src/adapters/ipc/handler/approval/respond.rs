//! `approval` IPC: respond 도메인.

use super::*;

pub fn handle_respond(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let req_id = match params.get("id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => ApprovalId(s.to_string()),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'id'"),
    };
    let choice = match params.get("choice").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing 'choice'"),
    };
    let comment = params
        .get("comment")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let by = responder_from_caller(caller);

    match core.respond_approval(engine, &req_id, choice.clone(), by, comment) {
        Ok(change) => {
            persist_record(core, &change.record);
            // capability_elevation 이 approve* 로 응답되면 대상
            // agent 에 임시 grant 를 적용한다. 실패해도 응답 자체는 유지
            // (grant 가 실패해도 agent 는 retry 시 다시 elevation 을 받게 됨).
            apply_elevation_grant_if_any(core, &change.record, &choice);
            crate::adapters::ipc::handler::memory::written(core, id, record_to_json(&change.record))
        }
        Err(e) => map_error(id, e),
    }
}
