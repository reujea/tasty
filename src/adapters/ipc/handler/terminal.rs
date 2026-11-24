//! 자식 터미널의 생성·입력·조회·종료를 관리한다. 에이전트별 명령 구성과 훅은 플러그인이 맡는다.
//! 구조 생성·닫기는 App의 journal admission에서 처리하며 여기에는 입력·관계 서비스가 남는다.
//! 부모·자식 관계와 soft 점유를 함께 관리한다(docs/dev-guide/attach-behavior.md).

mod fixed_input;
use crate::runtime::engine_access::{EngineMut, EngineRef};
use fixed_input::respawn_options;
pub(crate) use fixed_input::{input_response, prepare_input};
use serde_json::{Value, json};

use crate::core::state::child_liveness::ChildLiveness;
use crate::runtime::child_terminal::{ChildEntry, ParentKey};
use tasty_ipc::protocol::JsonRpcResponse;

use super::params::{optional_parent, require_u32};

fn optional_str(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn require_str(params: &Value, key: &str, id: &Value) -> Result<String, JsonRpcResponse> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), format!("missing '{key}'")))
}

/// 부모를 사람이 읽는 형태로 쓴다. Surface 부모 문구는 이전 u32 부모와 같다.
fn parent_label(parent: &ParentKey) -> String {
    match parent {
        ParentKey::Surface(id) => format!("surface {id}"),
        ParentKey::External(key) => format!("parent_external {key}"),
    }
}

/// child는 surface ID가 아니라 부모별 index다. 잘못 지정했으면 올바른 인자를 안내한다.
/// 두 번호가 겹칠 수 있으므로 surface ID로 자동 해석하지 않는다.
fn child_not_found_message(
    reg: &crate::runtime::child_terminal::ChildTerminalRegistry,
    parent: &ParentKey,
    given: u32,
) -> String {
    let base = format!("child {given} not found under {}", parent_label(parent));

    if let Some(owner) = reg.parent_of_child(given)
        && let Some(index) = reg
            .list_children(&owner)
            .iter()
            .find(|c| c.child_surface_id == given)
            .map(|c| c.index)
    {
        return if &owner == parent {
            format!(
                "{base}. {given} is a child_surface_id, not a child index — use `--child {index}`"
            )
        } else {
            let target = match &owner {
                ParentKey::Surface(owner) => format!("--surface {owner}"),
                ParentKey::External(owner) => format!("--parent-external {owner}"),
            };
            format!(
                "{base}. {given} is a child_surface_id under a different parent \
                 — use `{target} --child {index}`"
            )
        };
    }

    let mut indices: Vec<u32> = reg.list_children(parent).iter().map(|c| c.index).collect();
    if indices.is_empty() {
        return format!(
            "{base} (no children registered under {})",
            parent_label(parent)
        );
    }
    indices.sort_unstable();
    let count = indices.len();
    format!(
        "{base} (valid child indices: {}; {count} children)",
        format_index_ranges(&indices)
    )
}

/// 정렬된 index를 연속 구간으로 압축하되 빠진 번호를 포함하지 않는다.
fn format_index_ranges(sorted: &[u32]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        });
        i += 1;
    }
    parts.join(", ")
}

/// `--surface`(parent) 또는 `parent_external` 명시가 없으면 유일 Surface parent 로 폴백한다.
/// External 부모는 폴백 후보가 아니다. 0 또는 2+ 면 에러(호출자 명시 요구).
fn resolve_parent(
    engine: &EngineRef<'_>,
    params: &Value,
    id: &Value,
) -> Result<ParentKey, JsonRpcResponse> {
    if let Some(p) = optional_parent(params, "surface", id)? {
        return Ok(p);
    }
    engine
        .runtime
        .child_terminals
        .single_parent(&engine.core.live_surface_ids())
        .map(ParentKey::Surface)
        .ok_or_else(|| {
            JsonRpcResponse::invalid_params(
                id.clone(),
                "missing 'surface' parameter (0 or >1 parents — specify --surface)".to_string(),
            )
        })
}

/// 본문 제출 거절을 기존 오류 코드와 문구로 바꾼다.
fn submit_rejected_response(
    id: Value,
    surface_id: u32,
    rejected: crate::runtime::pending_submit::SubmitRejected,
) -> JsonRpcResponse {
    use crate::runtime::pending_submit::SubmitRejected;
    match rejected {
        SubmitRejected::QueueFull => JsonRpcResponse::error(
            id,
            tasty_ipc::protocol::ERR_COMMAND_QUEUE_FULL,
            "terminal submission queue is full",
        ),
        SubmitRejected::NotFound => {
            JsonRpcResponse::invalid_params(id, format!("Surface {surface_id} not found"))
        }
        SubmitRejected::HardOccupied => JsonRpcResponse::invalid_params(
            id,
            format!(
                "Surface {surface_id} is attached elsewhere (hard-occupied) — release the \
                 attach or target a different surface"
            ),
        ),
    }
}

fn decode_tell<'a>(params: &'a Value, id: &Value) -> Result<(u32, &'a str), JsonRpcResponse> {
    let surface = require_u32(params, "surface", id)?;
    let text = params
        .get("text")
        .and_then(|value| value.as_str())
        .ok_or_else(|| JsonRpcResponse::invalid_params(id.clone(), "missing 'text'"))?;
    Ok((surface, text))
}

pub(crate) fn handle_tell(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let (surface_id, text) = match decode_tell(params, &id) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let result = crate::app::terminal_input::tell(&mut engine.terminal_input(), surface_id, text);
    input_response(&engine.as_ref(), id, result)
}

pub(crate) fn handle_children(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let live = engine.live_surface_ids();
    // 부모를 생략하면 호스트가 고르므로 호출자가 알 수 있도록 항목마다 싣는다.
    let parent_value = match &parent {
        ParentKey::Surface(surface) => json!(surface),
        ParentKey::External(key) => json!(key),
    };
    let children: Vec<Value> = engine
        .runtime
        .child_terminals
        .list_children(&parent)
        .iter()
        .map(|c| {
            let mut item =
                liveness_fields(engine.child_liveness_with_live(c.child_surface_id, &live));
            item.insert("index".into(), json!(c.index));
            item.insert("surface_id".into(), json!(c.child_surface_id));
            item.insert("role".into(), json!(c.role));
            item.insert("nickname".into(), json!(c.nickname));
            item.insert("cwd".into(), json!(c.cwd));
            item.insert("parent".into(), parent_value.clone());
            item.insert(
                "last_report_at".into(),
                json!(
                    engine
                        .runtime
                        .child_terminals
                        .last_state_report_at(c.child_surface_id)
                ),
            );
            remote_child_fields(c.child_surface_id, &mut item);
            Value::Object(item)
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "children": children }))
}

/// attach mirror 의 원격 자식이면 `remote`·`liveness` 를 싣는다. holder 연결이 끊긴 동안에는 마지막 원격
/// 보고를 확정처럼 보이지 않게 state 를 `unknown` 으로 덮는다. 원격 자식이 아니면 그대로 둔다.
fn remote_child_fields(surface: u32, fields: &mut serde_json::Map<String, Value>) {
    #[cfg(feature = "gui")]
    if let Some(connected) = crate::remote::agent_children::remote_child_connected(surface) {
        remote_connection_fields(connected, fields);
    }
    #[cfg(not(feature = "gui"))]
    let _ = (surface, fields); // 헤드리스에는 attach client 가 없어 원격 자식도 없다.
}

/// 끊긴 동안에는 mirror 에 남은 원격 전경 이름 등 마지막 관측으로 낸 판정을 덮는다.
#[cfg(feature = "gui")]
fn remote_connection_fields(connected: bool, fields: &mut serde_json::Map<String, Value>) {
    fields.insert("remote".into(), json!(true));
    if connected {
        fields.insert("liveness".into(), json!("connected"));
    } else {
        fields.insert("liveness".into(), json!("unknown"));
        fields.insert("state".into(), json!("unknown"));
        fields.insert("evidence".into(), json!("remote_disconnected"));
        fields.insert("confidence".into(), json!("unobserved"));
    }
}

/// children/state 응답이 같은 state/evidence/confidence 필드를 사용한다.
/// 값의 조합은 docs/features/child-terminal/index.md의 판정 표를 따른다.
fn liveness_fields(liveness: ChildLiveness) -> serde_json::Map<String, Value> {
    let mut fields = serde_json::Map::new();
    fields.insert("state".into(), json!(liveness.state.as_str()));
    fields.insert("evidence".into(), json!(liveness.evidence.as_str()));
    fields.insert("confidence".into(), json!(liveness.confidence.as_str()));
    fields
}

/// 명시한 자식 surface를 목록 조회와 같은 child_liveness 함수로 판정한다.
/// 관계가 사라져도 surface가 살아 있을 수 있어 실제 트리와 PTY 상태를 따로 확인한다.
pub(crate) fn handle_state(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let surface_id = match require_u32(params, "surface", &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let mut out = liveness_fields(engine.child_liveness(surface_id));
    out.insert("surface_id".into(), json!(surface_id));
    remote_child_fields(surface_id, &mut out);
    JsonRpcResponse::success(id, Value::Object(out))
}

pub(crate) fn handle_parent(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let child_surface = match require_u32(params, "surface", &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match engine
        .runtime
        .child_terminals
        .parent_of_child(child_surface)
    {
        Some(parent) => JsonRpcResponse::success(
            id,
            json!({
                "parent_surface_id": parent.surface(),
                "parent_external": parent.external(),
                "status": "active",
            }),
        ),
        None => JsonRpcResponse::success(
            id,
            json!({
                "parent_surface_id": Value::Null,
                "parent_external": Value::Null,
                "status": "none",
            }),
        ),
    }
}

pub(crate) fn handle_release(
    core: &crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let child_index = match require_u32(params, "child", &id) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Some(removed) = engine
        .runtime
        .child_terminals
        .remove_child(&parent, child_index)
    else {
        return JsonRpcResponse::invalid_params(
            id,
            child_not_found_message(&engine.runtime.child_terminals, &parent, child_index),
        );
    };
    let receipt = match engine
        .runtime
        .child_terminals
        .release_with_effect(parent, removed.child_surface_id)
    {
        Ok(ticket) => ticket,
        Err(error) => return JsonRpcResponse::internal_error(id, error),
    };

    finish_mutation(
        core,
        id,
        receipt,
        json!({
            "released_surface_id": removed.child_surface_id,
            "child_index": removed.index,
        }),
    )
}

/// 기존 surface를 자식으로 등록한다. 임의의 기존 대상이므로 soft 점유를 먼저 확보한다.
/// 실패하면 관계를 추가하지 않는다. 새 surface를 만드는 spawn은 관계 등록 후 점유하며
/// 기존 surface에 자식 관계와 soft 점유를 연결한다. 구조 생성은 수행하지 않는다.
pub(crate) fn handle_adopt(
    core: &crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(ParentKey::Surface(p)) => p,
        Ok(ParentKey::External(_)) => {
            return JsonRpcResponse::invalid_params(
                id,
                "terminal.adopt does not accept 'parent_external'",
            );
        }
        Err(e) => return e,
    };
    let target = match require_u32(params, "target", &id) {
        Ok(t) => t,
        Err(e) => return e,
    };

    if engine.find_surface_by_id(target).is_none() {
        return JsonRpcResponse::invalid_params(id, format!("surface {target} not found"));
    }
    if parent == target {
        return JsonRpcResponse::invalid_params(
            id,
            "cannot adopt a surface as its own child".to_string(),
        );
    }
    if let Some(existing_parent) = engine.runtime.child_terminals.parent_of_child(target) {
        return JsonRpcResponse::invalid_params(
            id,
            format!(
                "surface {target} is already a child of {existing_parent} \
                 (release it first — see 'tasty terminal release')"
            ),
        );
    }
    // soft 점유 함수는 hard 점유를 검사하지 않으므로 여기서 먼저 거절한다.
    if engine.live.occupancy.is_hard_occupied(target) {
        return JsonRpcResponse::invalid_params(
            id,
            format!("surface {target} is hard-occupied (remote attach) — cannot adopt"),
        );
    }

    let role = optional_str(params, "role");
    let nickname = optional_str(params, "nickname");
    let cwd = optional_str(params, "cwd");
    // Check the existing owner without changing its label or releasing its lock
    // if the following relationship commit fails.
    if let Some(occupancy) = engine.live.occupancy.occupancy_of(target)
        && occupancy.parent != Some(ParentKey::Surface(parent))
    {
        return JsonRpcResponse::error(id, -32020, "occupy_soft failed: another owner");
    }
    let generation = engine.runtime.terminals.generation(target);
    let receipt = match engine.runtime.child_terminals.adopt_with_effect(
        parent.into(),
        ChildEntry {
            child_surface_id: target,
            index: 0,
            cwd,
            role,
            nickname,
        },
        generation,
    ) {
        Ok(ticket) => ticket,
        Err(error) => return JsonRpcResponse::internal_error(id, error),
    };
    core.reply_with_file(
        id.clone(),
        move || receipt.poll(),
        move |_, result| match result {
            Ok(Some(index)) => JsonRpcResponse::success(
                id,
                json!({ "child_surface_id": target, "child_index": index }),
            ),
            Ok(None) => JsonRpcResponse::internal_error(id, "child adopt returned no index"),
            Err(error) => JsonRpcResponse::internal_error(id, error),
        },
    )
}

pub(crate) fn handle_respawn(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let child_index = match require_u32(params, "child", &id) {
        Ok(c) => c,
        Err(e) => return e,
    };
    if engine
        .runtime
        .child_terminals
        .find_child(&parent, child_index)
        .is_none()
    {
        return JsonRpcResponse::invalid_params(
            id,
            child_not_found_message(&engine.runtime.child_terminals, &parent, child_index),
        );
    }
    // cwd가 바뀌면 PTY를 교체해야 하므로 구조 admission이 맡는다.
    if optional_str(params, "cwd").is_some() {
        return JsonRpcResponse::internal_error(
            id,
            "child resource respawn requires committed structure admission",
        );
    }
    let options = respawn_options(params);
    let result = crate::app::terminal_input::respawn(
        &mut engine.terminal_input(),
        &parent,
        child_index,
        &options,
    );
    input_response(&engine.as_ref(), id, result)
}

pub(crate) fn handle_broadcast(
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let text = match require_str(params, "text", &id) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let role_filter = optional_str(params, "role");

    let targets: Vec<u32> = engine
        .runtime
        .child_terminals
        .list_children(&parent)
        .iter()
        .filter(|c| match &role_filter {
            Some(r) => c.role.as_deref() == Some(r.as_str()),
            None => true,
        })
        .map(|c| c.child_surface_id)
        .collect();
    let result =
        crate::app::terminal_input::broadcast(&mut engine.terminal_input(), targets, &text);
    input_response(&engine.as_ref(), id, result)
}

/// 훅은 idle/needs_input/active만 보고할 수 있다.
/// exited/stale은 호스트가 트리·PTY에서 판정하는 출력 상태이므로 입력으로 받지 않는다.
pub(crate) fn handle_set_state(
    core: &crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let surface_id = match require_u32(params, "surface", &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let new_state = match require_str(params, "state", &id) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if !matches!(new_state.as_str(), "idle" | "active" | "needs_input") {
        return JsonRpcResponse::invalid_params(
            id,
            format!("unknown state '{new_state}' (supported: idle, needs_input, active)"),
        );
    }
    match new_state.as_str() {
        "idle" => engine.runtime.child_terminals.set_idle(surface_id, true),
        "active" => engine.runtime.child_terminals.set_idle(surface_id, false),
        "needs_input" => engine
            .runtime
            .child_terminals
            .set_needs_input(surface_id, true),
        other => {
            return JsonRpcResponse::invalid_params(
                id,
                format!("unknown state '{other}' (supported: idle, needs_input, active)"),
            );
        }
    }
    let receipt = match engine.runtime.child_terminals.submit() {
        Ok(ticket) => ticket,
        Err(error) => return JsonRpcResponse::internal_error(id, error),
    };
    crate::remote::agent_children::relay(
        &engine.as_ref(),
        surface_id,
        tasty_ipc::agent_child::Notice::State {
            state: new_state.clone(),
        },
    );
    finish_mutation(
        core,
        id,
        receipt,
        json!({ "ok": true, "surface_id": surface_id, "state": new_state }),
    )
}

/// nickname 한도. `parent_external` 키와 같은 바이트 상한을 쓴다.
const NICKNAME_MAX_BYTES: usize = 256;

/// `nickname` 문자열 또는 `clear: true` 중 정확히 하나를 읽는다. `Some`은 새 이름, `None`은 지우기다.
/// 빈 문자열은 지우기로 읽지 않는다. 비어 있는 셸 변수가 이름을 조용히 지우지 않게 한다.
fn decode_nickname(params: &Value, id: &Value) -> Result<Option<String>, JsonRpcResponse> {
    let invalid = |msg: String| JsonRpcResponse::invalid_params(id.clone(), msg);
    let clear = match params.get("clear") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(clear)) => *clear,
        Some(_) => return Err(invalid("'clear' must be a boolean".into())),
    };
    let nickname = match params.get("nickname") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(name.clone()),
        Some(_) => return Err(invalid("'nickname' must be a string".into())),
    };
    match (nickname, clear) {
        (Some(_), true) => Err(invalid(
            "pass either 'nickname' or 'clear', not both".into(),
        )),
        (None, true) => Ok(None),
        (None, false) => Err(invalid(
            "missing 'nickname' (use 'clear: true' to remove it)".into(),
        )),
        (Some(name), false) => {
            if name.is_empty() {
                return Err(invalid(
                    "'nickname' must not be empty (use 'clear: true' to remove it)".into(),
                ));
            }
            if name.len() > NICKNAME_MAX_BYTES {
                return Err(invalid(format!(
                    "'nickname' is longer than {NICKNAME_MAX_BYTES} bytes"
                )));
            }
            if name.chars().any(char::is_control) {
                return Err(invalid(
                    "'nickname' must not contain control characters".into(),
                ));
            }
            Ok(Some(name))
        }
    }
}

/// 숫자 부모 없이 `workspace`로 창을 고른 요청은 그 workspace가 이 창에 있어야 한다.
/// 라우터는 찾지 못한 `workspace`를 포커스 창으로 보낸다(`terminal.*` 공통 동작). 그 창에 같은 External 키와
/// 번호의 자식이 있으면 명시하지 않은 창의 자식을 고치게 되므로, 이 메서드는 여기서 거절한다.
fn require_named_workspace_here(
    engine: &EngineRef<'_>,
    params: &Value,
    id: &Value,
) -> Result<(), JsonRpcResponse> {
    if params.get("surface").is_some_and(|v| !v.is_null()) {
        return Ok(());
    }
    let target = match params.get("workspace") {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::String(target)) => target,
        Some(_) => {
            return Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "'workspace' must be a string",
            ));
        }
    };
    // 라우터(find_main_with_workspace_target)와 같이 숫자면 workspace ID, 아니면 이름으로 맞춘다.
    let numeric = target.parse::<u32>().ok();
    let here = engine
        .core
        .workspaces()
        .into_iter()
        .any(|ws| Some(ws.id) == numeric || ws.name == *target);
    if here {
        return Ok(());
    }
    Err(JsonRpcResponse::invalid_params(
        id.clone(),
        format!(
            "workspace '{target}' is not open in the window that received this request — \
             nothing was changed; check the name with `tasty list workspaces`"
        ),
    ))
}

/// 실행 중인 child의 nickname을 바꾸거나 지운다. 이 인스턴스의 registry만 바꾸며 PTY·점유는 건드리지 않는다.
/// 저장 명령이 끝난 뒤 답한다. 저장이 접수되지 않거나 실패하면 오류로 답한다.
pub(crate) fn handle_set_nickname(
    core: &crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    engine.reconcile_child_terminals();
    if let Err(e) = require_named_workspace_here(&engine.as_ref(), params, &id) {
        return e;
    }
    let parent = match resolve_parent(&engine.as_ref(), params, &id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let child_index = match require_u32(params, "child", &id) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let nickname = match decode_nickname(params, &id) {
        Ok(n) => n,
        Err(e) => return e,
    };
    let Some(entry) = engine
        .runtime
        .child_terminals
        .find_child(&parent, child_index)
    else {
        return JsonRpcResponse::invalid_params(
            id,
            child_not_found_message(&engine.runtime.child_terminals, &parent, child_index),
        );
    };
    // 항목을 고치면 이 engine 이 그 자식을 소유한 것으로 기록한다. 다른 창의 자식을 소유하면 이 창의
    // reconcile 이 그 관계를 닫힌 자식으로 지우므로, 이 창에 열린 자식만 받는다.
    let surface_id = entry.child_surface_id;
    if !engine.live_surface_ids().contains(&surface_id) {
        return JsonRpcResponse::invalid_params(
            id,
            format!(
                "child {child_index} under {} is surface {surface_id}, which is not open in \
                 this window — its nickname can only be changed from the window that holds it",
                parent_label(&parent)
            ),
        );
    }
    // 확인한 자식(surface)으로 대상을 고정한다. 확인과 수정 사이에 앞선 저장 결과가 들어와 같은 번호가
    // 다른 자식을 가리키면, 그 자식을 이 창 소유로 기록하며 고치지 않고 거절한다.
    let receipt = match engine.runtime.child_terminals.set_nickname(
        &parent,
        child_index,
        surface_id,
        nickname.clone(),
    ) {
        Ok(ticket) => ticket,
        Err(crate::runtime::child_terminal::NicknameRefused::ChildChanged) => {
            return JsonRpcResponse::internal_error(
                id,
                format!(
                    "child {child_index} under {} changed while renaming (it was surface \
                     {surface_id}) — nothing was changed; list the children and retry",
                    parent_label(&parent)
                ),
            );
        }
        Err(crate::runtime::child_terminal::NicknameRefused::NotAdmitted(error)) => {
            return JsonRpcResponse::internal_error(id, error);
        }
    };
    let parent_value = match &parent {
        ParentKey::Surface(surface) => json!(surface),
        ParentKey::External(key) => json!(key),
    };
    finish_mutation(
        core,
        id,
        receipt,
        json!({
            "parent": parent_value,
            "child_index": child_index,
            "surface_id": surface_id,
            "nickname": nickname,
        }),
    )
}

fn finish_mutation(
    core: &crate::app::services::AppServices,
    id: Value,
    receipt: crate::runtime::child_terminal::Ticket,
    value: Value,
) -> JsonRpcResponse {
    core.reply_with_file(
        id.clone(),
        move || receipt.poll(),
        move |_, result| match result {
            Ok(_) => JsonRpcResponse::success(id, value),
            Err(error) => JsonRpcResponse::internal_error(id, error),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::attach::OccupancyTier;

    fn wait_mutation(
        core: &crate::app::services::AppServices,
        engine: &mut EngineMut<'_>,
        immediate: JsonRpcResponse,
    ) -> JsonRpcResponse {
        let Some(deferred) = core.memory_replies().take_parked() else {
            return immediate;
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        core.memory_replies()
            .push(deferred, super::super::deferred::ReplyTo::Ipc(sender));
        // holds_by 로 바꾸지 않는다. 응답을 꺼내는 poll_child_registry_effects 가 child cleanup 제출을 함께 해
        // 기한 뒤 관측 읽기에서 부를 수 없고, 새 작업 없이 끝난 결과만 거두는 경로가 따로 없다.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            engine.poll_child_registry_effects();
            core.poll_memory_replies(None);
            if let Ok(response) = receiver.try_recv() {
                return response;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "child caller reply missing"
            );
            std::thread::yield_now();
        }
    }
    fn handle_adopt(engine: &mut EngineMut<'_>, id: Value, params: &Value) -> JsonRpcResponse {
        let core = super::super::cli_entry_tests::test_core();
        let response = super::handle_adopt(&core, engine, id, params);
        wait_mutation(&core, engine, response)
    }
    fn handle_release(engine: &mut EngineMut<'_>, id: Value, params: &Value) -> JsonRpcResponse {
        let core = super::super::cli_entry_tests::test_core();
        let response = super::handle_release(&core, engine, id, params);
        wait_mutation(&core, engine, response)
    }
    fn handle_set_state(engine: &mut EngineMut<'_>, id: Value, params: &Value) -> JsonRpcResponse {
        let core = super::super::cli_entry_tests::test_core();
        let response = super::handle_set_state(&core, engine, id, params);
        wait_mutation(&core, engine, response)
    }
    fn handle_set_nickname(
        engine: &mut EngineMut<'_>,
        id: Value,
        params: &Value,
    ) -> JsonRpcResponse {
        let core = super::super::cli_entry_tests::test_core();
        let response = super::handle_set_nickname(&core, engine, id, params);
        wait_mutation(&core, engine, response)
    }
    fn engine() -> crate::runtime::engine_session::EngineSession {
        engine_with_extra(&[])
    }
    fn engine_with_extra(ids: &[u32]) -> crate::runtime::engine_session::EngineSession {
        use tasty_core::{DomainEvent as E, SurfaceSpec};
        let mut events = vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "workspace".into(),
                category: 0,
                index: 0,
                pane: 1,
            },
        ];
        for (index, id) in std::iter::once(1).chain(ids.iter().copied()).enumerate() {
            events.push(E::TabCreated {
                id,
                pane: 1,
                index,
                name: "terminal".into(),
                surface: SurfaceSpec {
                    id,
                    kind: "terminal".into(),
                    data: None,
                },
            });
        }
        let mut owner =
            crate::state::tests::test_state_from_model(crate::state::tests::test_model(events)).1;
        for id in ids {
            drop(owner.runtime.terminals.remove(*id));
        }
        owner
    }

    fn child(sid: u32, index: u32) -> ChildEntry {
        ChildEntry {
            child_surface_id: sid,
            index,
            cwd: None,
            role: None,
            nickname: None,
        }
    }

    fn ok(resp: JsonRpcResponse) -> Value {
        resp.result.expect("expected success result")
    }

    #[test]
    fn index_ranges_compress_contiguous_runs() {
        assert_eq!(format_index_ranges(&[0, 1, 2, 3]), "0-3");
        assert_eq!(format_index_ranges(&[7]), "7");
        assert_eq!(format_index_ranges(&[0, 1, 2, 5, 8, 9]), "0-2, 5, 8-9");
        assert_eq!(format_index_ranges(&[0, 57]), "0, 57");
    }

    #[test]
    fn child_not_found_points_at_index_when_given_a_surface_id() {
        let mut reg = crate::runtime::child_terminal::ChildTerminalRegistry::default();
        reg.register_child(3157, child(3204, 31));
        let msg = child_not_found_message(&reg, &3157.into(), 3204);
        assert!(msg.contains("child_surface_id, not a child index"), "{msg}");
        assert!(msg.contains("`--child 31`"), "{msg}");
    }

    #[test]
    fn child_not_found_points_at_other_parent() {
        let mut reg = crate::runtime::child_terminal::ChildTerminalRegistry::default();
        reg.register_child(9000, child(3204, 4));
        let msg = child_not_found_message(&reg, &3157.into(), 3204);
        assert!(msg.contains("under a different parent"), "{msg}");
        assert!(msg.contains("`--surface 9000 --child 4`"), "{msg}");
    }

    #[test]
    fn child_not_found_lists_valid_indices() {
        let mut reg = crate::runtime::child_terminal::ChildTerminalRegistry::default();
        for i in 0..3 {
            reg.register_child(3157, child(100 + i, i));
        }
        let msg = child_not_found_message(&reg, &3157.into(), 999);
        assert!(msg.contains("valid child indices: 0-2"), "{msg}");
        assert!(msg.contains("3 children"), "{msg}");
    }

    #[test]
    fn child_not_found_reports_empty_parent() {
        let reg = crate::runtime::child_terminal::ChildTerminalRegistry::default();
        let msg = child_not_found_message(&reg, &3157.into(), 0);
        assert!(msg.contains("no children registered"), "{msg}");
    }

    #[test]
    fn spawn_kill_occupancy_wiring() {
        // 점유와 관계 등록만 검사한다. 탭 생성·입력 핸들러 전체 시험은 아니다.
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let c = 5000u32;

        let idx = e.runtime.child_terminals.next_index_for(parent);
        e.runtime
            .child_terminals
            .register_child(parent, child(c, idx));
        e.occupy_soft(c, parent, Some("worker".into())).unwrap();
        let occ = e
            .live
            .occupancy
            .occupancy_of(c)
            .expect("soft occupancy present");
        assert_eq!(occ.tier, OccupancyTier::Soft);
        assert_eq!(occ.parent, Some(parent.into()));
        assert!(!e.live.occupancy.is_hard_occupied(c));

        assert!(
            e.runtime
                .child_terminals
                .remove_child(&parent.into(), idx)
                .is_some()
        );
        e.release_soft_occupancy(c, parent).unwrap();
        assert!(e.live.occupancy.occupancy_of(c).is_none());
    }

    #[test]
    fn adopt_registers_existing_surface_without_new_tab() {
        let mut e_session = engine_with_extra(&[6101]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = 6101u32; // 이미 존재하는(=spawn 아닌) surface

        let resp = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "target": target }),
        );
        assert!(resp.error.is_none());

        let occ = e
            .live
            .occupancy
            .occupancy_of(target)
            .expect("soft occupancy present");
        assert_eq!(occ.tier, OccupancyTier::Soft);
        assert_eq!(occ.parent, Some(parent.into()));
        assert_eq!(
            e.runtime.child_terminals.parent_of_child(target),
            Some(parent.into())
        );
    }

    #[test]
    fn adopt_rejects_already_registered_child() {
        let mut e_session = engine_with_extra(&[6102]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = 6102u32;

        let _ = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "target": target }),
        );
        let resp2 = handle_adopt(
            &mut e,
            json!(2),
            &json!({ "surface": parent, "target": target }),
        );
        assert!(resp2.error.is_some()); // 중복 등록 거부
    }

    #[test]
    fn adopt_rejects_nonexistent_surface() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let resp = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "target": 999999u32 }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn adopt_rejects_self_adoption() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let resp = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "target": parent }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn adopt_rejects_hard_occupied_target_and_leaves_registry_unchanged() {
        let mut e_session = engine_with_extra(&[6103]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = 6103u32;
        e.live
            .occupancy
            .acquire(target, /* hard occupancy client id */ 1)
            .unwrap();

        let resp = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "target": target }),
        );
        assert!(resp.error.is_some());
        assert!(e.runtime.child_terminals.parent_of_child(target).is_none());
        assert!(e.live.occupancy.is_hard_occupied(target)); // 기존 hard 점유는 건드리지 않음
    }

    #[test]
    fn release_clears_registry_and_occupancy_but_keeps_surface() {
        let mut e_session = engine_with_extra(&[5701]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let c = 5701u32;

        let idx = e.runtime.child_terminals.next_index_for(parent);
        e.runtime
            .child_terminals
            .register_child(parent, child(c, idx));
        e.occupy_soft(c, parent, Some("worker".into())).unwrap();

        let resp = handle_release(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": idx }),
        );
        assert!(resp.error.is_none());

        assert!(
            e.runtime
                .child_terminals
                .find_child(&parent.into(), idx)
                .is_none()
        );
        assert!(e.live.occupancy.occupancy_of(c).is_none());
    }

    #[test]
    fn release_rejects_unregistered_child_index() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let resp = handle_release(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": 999u32 }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn release_does_not_touch_unrelated_hard_occupancy() {
        let mut e_session = engine_with_extra(&[5702]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let c = 5702u32;
        let idx = e.runtime.child_terminals.next_index_for(parent);
        e.runtime
            .child_terminals
            .register_child(parent, child(c, idx));
        e.occupy_soft(c, parent, None).unwrap();
        let other = 5703u32;
        e.live.occupancy.acquire(other, 1).unwrap();

        let resp = handle_release(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": idx }),
        );
        assert!(resp.error.is_none());
        assert!(e.live.occupancy.occupancy_of(c).is_none()); // soft 는 정상 해제
        assert!(e.live.occupancy.is_hard_occupied(other)); // 무관한 hard 점유는 그대로
    }

    // 이 시험은 단일 engine의 부모 선택만 확인한다. 창 간 모호성은 App::find_request_owner에서 거절한다.
    #[test]
    fn resolve_parent_omitted_surface_succeeds_with_single_parent() {
        let mut e_session = engine_with_extra(&[59010]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let idx = e.runtime.child_terminals.next_index_for(parent);
        e.runtime
            .child_terminals
            .register_child(parent, child(59010, idx));

        let resp = handle_release(&mut e, json!(1), &json!({ "child": idx }));
        assert!(resp.error.is_none(), "{:?}", resp.error);
    }

    #[test]
    fn resolve_parent_omitted_surface_errors_with_multiple_parents_in_one_engine() {
        let mut e_session = engine_with_extra(&[59030, 59040]);
        let mut e = e_session.borrow_mut();
        let parent1 = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let parent2 = 59020u32;
        let idx1 = e.runtime.child_terminals.next_index_for(parent1);
        e.runtime
            .child_terminals
            .register_child(parent1, child(59030, idx1));
        let idx2 = e.runtime.child_terminals.next_index_for(parent2);
        e.runtime
            .child_terminals
            .register_child(parent2, child(59040, idx2));

        let resp = handle_release(&mut e, json!(1), &json!({ "child": idx1 }));
        assert!(resp.error.is_some());
    }

    #[test]
    fn children_reconcile_prunes_dead_child() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.runtime
            .child_terminals
            .register_child(parent, child(99999, 0));
        let resp = handle_children(&mut e, json!(1), &json!({ "surface": parent }));
        let n = ok(resp)["children"].as_array().unwrap().len();
        assert_eq!(n, 0);
    }

    #[test]
    fn children_item_names_its_parent_and_last_report() {
        let mut e_session = engine_with_extra(&[5902]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.runtime
            .child_terminals
            .register_child(parent, child(5902, 0));
        let reported = e.runtime.child_terminals.last_state_report_at(5902);
        assert!(reported.is_some(), "등록이 보고 시각을 찍는다");

        // 부모를 생략해 호스트가 고른 경우에도 그 부모가 응답에 있어야 한다.
        for params in [json!({ "surface": parent }), json!({})] {
            let resp = ok(handle_children(&mut e, json!(1), &params));
            let item = &resp["children"].as_array().expect("children")[0];
            assert_eq!(item["parent"], json!(parent), "{params}");
            assert_eq!(item["last_report_at"], json!(reported), "{params}");
        }
    }

    #[test]
    fn children_of_an_external_parent_name_its_key() {
        let mut e_session = engine_with_extra(&[5903]);
        let mut e = e_session.borrow_mut();
        let key = ParentKey::External("codex:thread-1".to_string());
        e.runtime
            .child_terminals
            .register_child(key, child(5903, 0));
        let resp = ok(handle_children(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:thread-1" }),
        ));
        let item = &resp["children"].as_array().expect("children")[0];
        assert_eq!(item["parent"], json!("codex:thread-1"));
    }

    #[test]
    fn children_item_carries_evidence_and_confidence() {
        let mut e_session = engine_with_extra(&[5901]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = 5901u32;
        e.runtime
            .child_terminals
            .register_child(parent, child(target, 0));

        let resp = ok(handle_children(
            &mut e,
            json!(1),
            &json!({ "surface": parent }),
        ));
        let item = &resp["children"].as_array().expect("children")[0];
        for key in ["state", "evidence", "confidence"] {
            assert!(
                item.get(key).and_then(|v| v.as_str()).is_some(),
                "children 항목에 '{key}' 가 문자열로 실려야 한다: {item}"
            );
        }
    }

    #[test]
    fn children_and_state_report_identical_liveness_fields() {
        let mut e_session = engine_with_extra(&[5902]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = 5902u32;
        e.runtime
            .child_terminals
            .register_child(parent, child(target, 0));

        let list = ok(handle_children(
            &mut e,
            json!(1),
            &json!({ "surface": parent }),
        ));
        let item = list["children"].as_array().expect("children")[0].clone();
        let single = ok(handle_state(
            &mut e,
            json!(2),
            &json!({ "surface": target }),
        ));

        for key in ["state", "evidence", "confidence"] {
            assert_eq!(
                item[key], single[key],
                "'{key}' 가 목록과 단건에서 갈렸다: list={item}, single={single}"
            );
        }
    }

    /// 원격 자식의 mirror: 첫 이름을 받기 전에는 셸 근거를 쓰지 않고, 셸 이름을 받으면 로컬처럼
    /// 셸 복귀로 판정한다. 연결이 끊긴 동안에는 남은 이름으로 낸 판정을 unknown 으로 덮는다.
    #[cfg(feature = "gui")]
    #[test]
    fn a_mirror_child_judges_the_shell_by_the_pushed_name_only_while_connected() {
        use crate::core::state::child_liveness::{ChildState, ChildStateEvidence};
        let mut e_session = crate::state::tests::test_mirror_state().1;
        let mut e = e_session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];

        let before = e.child_liveness(sid);
        assert_ne!(
            before.evidence,
            ChildStateEvidence::ForegroundIsShell,
            "시작 직후"
        );

        e.set_mirror_surface_foreground(sid, Some("claude".into()));
        assert_ne!(e.child_liveness(sid).state, ChildState::Stale);

        e.set_mirror_surface_foreground(sid, Some("zsh".into()));
        let back = e.child_liveness(sid);
        assert_eq!(back.state, ChildState::Stale);
        assert_eq!(back.evidence, ChildStateEvidence::ForegroundIsShell);

        let mut fields = liveness_fields(back);
        remote_connection_fields(false, &mut fields);
        assert_eq!(fields["state"], json!("unknown"));
        assert_eq!(fields["evidence"], json!("remote_disconnected"));
    }

    #[test]
    fn parent_lookup_unregistered_is_none() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let resp = handle_parent(&mut e, json!(1), &json!({ "surface": 424242 }));
        assert_eq!(ok(resp)["status"], "none");
    }

    #[test]
    fn set_state_rejects_unknown() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        let resp = handle_set_state(
            &mut e,
            json!(1),
            &json!({ "surface": 5000, "state": "bogus" }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn set_state_updates_registry() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.runtime.child_terminals.register_child(7, child(5000, 0));
        let _ = handle_set_state(
            &mut e,
            json!(1),
            &json!({ "surface": 5000, "state": "idle" }),
        );
        assert_eq!(e.runtime.child_terminals.state_of(5000), "idle");
        let _ = handle_set_state(
            &mut e,
            json!(1),
            &json!({ "surface": 5000, "state": "needs_input" }),
        );
        assert_eq!(e.runtime.child_terminals.state_of(5000), "needs_input");
        let _ = handle_set_state(
            &mut e,
            json!(1),
            &json!({ "surface": 5000, "state": "active" }),
        );
        assert_eq!(e.runtime.child_terminals.state_of(5000), "active");
    }

    // needs_input 뒤 idle만 보고되어도 이전 입력 대기 상태를 지워야 한다.
    #[test]
    fn set_state_idle_clears_a_pending_needs_input() {
        let mut e_session = engine();
        let mut e = e_session.borrow_mut();
        e.runtime.child_terminals.register_child(7, child(5001, 0));
        fn push_child_state(e: &mut EngineMut<'_>, state: &str) {
            let resp = handle_set_state(e, json!(1), &json!({ "surface": 5001, "state": state }));
            assert!(resp.error.is_none(), "{state} 주입이 거부됐다");
        }
        push_child_state(&mut e, "needs_input");
        assert_eq!(e.runtime.child_terminals.state_of(5001), "needs_input");
        push_child_state(&mut e, "idle");
        assert_eq!(e.runtime.child_terminals.state_of(5001), "idle");
    }

    /// tell의 거절 순서와 wake 횟수를 관측한다. 본문은 기본 surface의 detached 터미널 sink로 본다.
    fn tell_fixture() -> (
        crate::runtime::engine_session::EngineSession,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) {
        let mut session = engine();
        let wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let e = session.borrow_mut();
            let counter = wakes.clone();
            e.runtime.waker = std::sync::Arc::new(move || {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
            e.runtime
                .terminals
                .get_mut(1)
                .expect("default terminal")
                .set_input_sink(tx);
        }
        (session, wakes, rx)
    }

    fn tell(e: &mut EngineMut<'_>, surface: u32) -> JsonRpcResponse {
        handle_tell(e, json!(1), &json!({ "surface": surface, "text": "hi" }))
    }

    #[test]
    fn tell_refuses_a_full_queue_before_writing_the_body() {
        let (mut session, wakes, rx) = tell_fixture();
        let mut e = session.borrow_mut();
        let generation = e.runtime.terminals.generation(1).expect("generation");
        for _ in 0..crate::runtime::pending_submit::MAX_PENDING {
            let ack = tasty_terminal::Terminal::new_detached(4, 2).send_key_with_ack("x");
            e.runtime
                .pending_submits
                .push(crate::runtime::pending_submit::PendingSubmit::new(
                    1, generation, ack,
                ));
        }

        let err = tell(&mut e, 1).error.expect("full queue must refuse");
        assert_eq!(err.code, tasty_ipc::protocol::ERR_COMMAND_QUEUE_FULL);
        assert_eq!(err.message, "terminal submission queue is full");
        assert_eq!(
            e.runtime.pending_submits.len(),
            crate::runtime::pending_submit::MAX_PENDING
        );
        assert_eq!(wakes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(rx.try_iter().next().is_none());
    }

    #[test]
    fn tell_reports_a_missing_surface_without_queueing() {
        let (mut session, wakes, _rx) = tell_fixture();
        let mut e = session.borrow_mut();

        let err = tell(&mut e, 424_242).error.expect("missing surface");
        assert_eq!(err.code, -32602);
        assert_eq!(err.message, "Surface 424242 not found");
        assert!(e.runtime.pending_submits.is_empty());
        assert_eq!(wakes.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn tell_refuses_a_hard_occupied_surface_without_writing_or_queueing() {
        let (mut session, wakes, rx) = tell_fixture();
        let mut e = session.borrow_mut();
        e.live.occupancy.acquire(1, 1).expect("hard occupy");

        let err = tell(&mut e, 1).error.expect("hard-occupied surface");
        assert_eq!(err.code, -32602);
        assert_eq!(
            err.message,
            "Surface 1 is attached elsewhere (hard-occupied) — release the attach or target a \
             different surface"
        );
        assert!(e.runtime.pending_submits.is_empty());
        assert_eq!(wakes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(rx.try_iter().next().is_none());
    }

    #[test]
    fn tell_writes_the_body_queues_one_submit_and_wakes_once() {
        let (mut session, wakes, rx) = tell_fixture();
        let mut e = session.borrow_mut();
        let generation = e.runtime.terminals.generation(1).expect("generation");

        let ok = tell(&mut e, 1).result.expect("tell succeeds");
        assert_eq!(ok, json!({ "sent": true, "surface_id": 1 }));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![b"hi".to_vec()]);
        assert_eq!(e.runtime.pending_submits.len(), 1);
        assert_eq!(e.runtime.pending_submits[0].surface(), 1);
        assert!(e.runtime.terminals.matches_generation(1, generation));
        assert_eq!(wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    fn external(key: &str) -> ParentKey {
        ParentKey::External(key.into())
    }

    /// External 부모 둘에 자식을 하나씩 단다. spawn 설치처럼 관계와 soft 점유를 함께 기록한다.
    fn two_external_parents(e: &mut EngineMut<'_>) {
        for (key, sid) in [("codex:a", 6101), ("codex:b", 6102)] {
            let parent = external(key);
            let index = e.runtime.child_terminals.reserve_index(&parent).unwrap();
            e.occupy_soft(sid, parent.clone(), Some("worker".into()))
                .unwrap();
            e.runtime
                .child_terminals
                .register_child(parent, child(sid, index));
        }
    }

    #[test]
    fn external_parents_list_report_and_release_only_their_own_children() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);

        let listed = ok(handle_children(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:a" }),
        ));
        let ids: Vec<u64> = listed["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["surface_id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [6101]);

        assert_eq!(
            ok(handle_parent(&mut e, json!(1), &json!({ "surface": 6101 }))),
            json!({ "parent_surface_id": null, "parent_external": "codex:a", "status": "active" })
        );

        let wrong = handle_release(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:b", "child": 1 }),
        );
        let message = wrong.error.expect("b has no child 1").message;
        assert!(
            message.starts_with("child 1 not found under parent_external codex:b"),
            "{message}"
        );

        let released = ok(handle_release(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:a", "child": 0 }),
        ));
        assert_eq!(released["released_surface_id"], json!(6101));
        assert!(e.live.occupancy.occupancy_of(6101).is_none());
        assert!(
            e.runtime
                .child_terminals
                .list_children(&external("codex:a"))
                .is_empty()
        );
        assert_eq!(
            e.runtime
                .child_terminals
                .reserve_index(&external("codex:a")),
            Ok(0),
            "last child released restarts the numbering"
        );
        assert_eq!(
            e.live.occupancy.occupancy_of(6102).unwrap().parent,
            Some(external("codex:b"))
        );
        assert_eq!(
            e.runtime
                .child_terminals
                .list_children(&external("codex:b"))
                .len(),
            1
        );
    }

    /// 다른 External 부모의 child_surface_id를 넘기면 CLI 옵션 표기로 올바른 인자를 안내한다.
    #[test]
    fn a_child_of_another_external_parent_points_at_the_cli_option() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);
        let msg = child_not_found_message(&e.runtime.child_terminals, &external("codex:a"), 6102);
        assert!(
            msg.ends_with("use `--parent-external codex:b --child 0`"),
            "{msg}"
        );
    }

    #[test]
    fn a_surface_and_an_external_parent_together_are_refused() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);
        for resp in [
            handle_children(
                &mut e,
                json!(1),
                &json!({ "surface": 1, "parent_external": "codex:a" }),
            ),
            handle_release(
                &mut e,
                json!(1),
                &json!({ "surface": 1, "parent_external": "codex:a", "child": 0 }),
            ),
        ] {
            let err = resp.error.expect("both parents");
            assert_eq!(err.code, -32602);
            assert_eq!(
                err.message,
                "pass either 'surface' or 'parent_external', not both"
            );
        }
        let err = handle_children(&mut e, json!(1), &json!({ "parent_external": "" }))
            .error
            .expect("empty key");
        assert_eq!(err.message, "'parent_external' must not be empty");
        assert_eq!(
            e.runtime
                .child_terminals
                .list_children(&external("codex:a"))
                .len(),
            1,
            "a refused request changes nothing"
        );
    }

    #[test]
    fn the_omitted_parent_fallback_skips_external_parents() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);
        let err = handle_release(&mut e, json!(1), &json!({ "child": 0 }))
            .error
            .expect("no surface parent to fall back to");
        assert_eq!(
            err.message,
            "missing 'surface' parameter (0 or >1 parents — specify --surface)"
        );
    }

    #[test]
    fn adopt_refuses_an_external_parent() {
        let mut e_session = engine_with_extra(&[6101]);
        let mut e = e_session.borrow_mut();
        let err = handle_adopt(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:a", "target": 6101 }),
        )
        .error
        .expect("adopt is surface only");
        assert_eq!(
            err.message,
            "terminal.adopt does not accept 'parent_external'"
        );
        assert!(e.runtime.child_terminals.parent_of_child(6101).is_none());
        assert!(e.live.occupancy.occupancy_of(6101).is_none());
    }

    #[test]
    fn a_broadcast_fixes_only_the_external_parents_children() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);
        let request: crate::ipc::protocol::JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "method": "terminal.broadcast",
            "params": { "parent_external": "codex:b", "text": "hi\r" },
            "id": 1,
        }))
        .unwrap();
        let fixed = prepare_input(&e.as_ref(), &request)
            .ok()
            .flatten()
            .expect("broadcast is fixed");
        assert_eq!(fixed.targets(), [6102]);
    }

    #[test]
    fn a_new_prompt_clears_idle_on_an_external_child() {
        let mut e_session = engine_with_extra(&[6101, 6102]);
        let mut e = e_session.borrow_mut();
        two_external_parents(&mut e);
        e.runtime.child_terminals.set_idle(6101, true);
        assert!(e.runtime.child_terminals.clear_idle_for_new_prompt(6101));
        assert_eq!(e.runtime.child_terminals.state_of(6101), "active");
    }

    /// 직접 respawn의 전송 실패는 단계마다 기존 코드와 문구를 그대로 낸다.
    #[test]
    fn a_direct_respawn_maps_each_send_failure_to_its_own_code() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut e_session = engine_with_extra(&[6101]);
        let mut e = e_session.borrow_mut();
        let parent = external("codex:r");
        for (sid, index) in [(1, 0), (6101, 1)] {
            e.runtime
                .child_terminals
                .register_child(parent.clone(), child(sid, index));
        }

        let missing = handle_respawn(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:r", "child": 1 }),
        )
        .error
        .expect("no terminal");
        assert_eq!(
            (missing.code, missing.message.as_str()),
            (-32603, "No terminal found")
        );

        e.live.occupancy.acquire(1, 1).expect("hard occupy");
        let held = handle_respawn(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:r", "child": 0, "command": "make" }),
        )
        .error
        .expect("hard occupied");
        assert_eq!(held.code, -32602);
        assert_eq!(
            held.message,
            "Surface 1 is hard-occupied (remote attach holds it) — server-local input is \
             blocked while attached"
        );

        let cwd = handle_respawn(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:r", "child": 0, "cwd": "/tmp" }),
        )
        .error
        .expect("cwd needs admission");
        assert_eq!(cwd.code, -32603);
        assert_eq!(
            cwd.message,
            "child resource respawn requires committed structure admission"
        );
    }

    /// 직접 broadcast는 본문과 끝 CR을 따로 쓰고, 보낼 수 없는 child도 대상 수에 센다.
    #[test]
    fn a_direct_broadcast_writes_body_then_cr_and_counts_every_target() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut e_session = engine_with_extra(&[6101]);
        let mut e = e_session.borrow_mut();
        let parent = external("codex:b");
        for (sid, index) in [(1, 0), (6101, 1)] {
            e.runtime
                .child_terminals
                .register_child(parent.clone(), child(sid, index));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        e.runtime
            .terminals
            .get_mut(1)
            .expect("terminal")
            .set_input_sink(tx);
        e.runtime.child_terminals.set_idle(1, true);
        e.runtime.child_terminals.set_idle(6101, true);
        let response = ok(handle_broadcast(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:b", "text": "go\r" }),
        ));
        assert_eq!(response, json!({ "sent_count": 2, "children": [1, 6101] }));
        let writes: Vec<Vec<u8>> = rx.try_iter().collect();
        assert_eq!(writes, [b"go".to_vec(), b"\r".to_vec()]);
        assert_eq!(e.runtime.child_terminals.state_of(1), "active");
        assert_eq!(e.runtime.child_terminals.state_of(6101), "idle");
    }

    /// 바꾼 nickname이 같은 registry를 읽는 `terminal.children` 항목에 나오고, 지우면 null로 돌아간다.
    #[test]
    fn set_nickname_updates_the_registry_and_children_output() {
        let mut e_session = engine_with_extra(&[5911]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let mut entry = child(5911, 0);
        entry.nickname = Some("old".into());
        entry.role = Some("worker".into());
        e.runtime.child_terminals.register_child(parent, entry);

        let resp = ok(handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": 0, "nickname": "lane a" }),
        ));
        assert_eq!(
            resp,
            json!({ "parent": parent, "child_index": 0, "surface_id": 5911, "nickname": "lane a" })
        );
        let list = ok(handle_children(
            &mut e,
            json!(2),
            &json!({ "surface": parent }),
        ));
        let item = &list["children"][0];
        assert_eq!(item["nickname"], json!("lane a"));
        assert_eq!(item["role"], json!("worker"), "다른 필드는 그대로다");

        let cleared = ok(handle_set_nickname(
            &mut e,
            json!(3),
            &json!({ "child": 0, "clear": true }),
        ));
        assert_eq!(cleared["nickname"], Value::Null);
        let list = ok(handle_children(
            &mut e,
            json!(4),
            &json!({ "surface": parent }),
        ));
        assert_eq!(list["children"][0]["nickname"], Value::Null);
    }

    #[test]
    fn set_nickname_names_an_external_parent_by_its_key() {
        let mut e_session = engine_with_extra(&[5912]);
        let mut e = e_session.borrow_mut();
        e.runtime
            .child_terminals
            .register_child(external("codex:n"), child(5912, 0));
        let resp = ok(handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:n", "child": 0, "nickname": "x" }),
        ));
        assert_eq!(resp["parent"], json!("codex:n"));
        let list = ok(handle_children(
            &mut e,
            json!(2),
            &json!({ "parent_external": "codex:n" }),
        ));
        assert_eq!(list["children"][0]["nickname"], json!("x"));
    }

    /// 라우터가 찾지 못한 workspace 요청은 포커스 창으로 온다. 그 창에 같은 External 키·번호의 자식이
    /// 열려 있어도 고치지 않는다. 이 창의 workspace 이름이나 ID를 주면 그대로 고친다.
    #[test]
    fn set_nickname_refuses_a_workspace_this_window_does_not_hold() {
        let mut e_session = engine_with_extra(&[5918]);
        let mut e = e_session.borrow_mut();
        let mut entry = child(5918, 0);
        entry.nickname = Some("kept".into());
        e.runtime
            .child_terminals
            .register_child(external("codex:f"), entry);
        for workspace in [json!("elsewhere"), json!("77")] {
            let resp = handle_set_nickname(
                &mut e,
                json!(1),
                &json!({ "parent_external": "codex:f", "workspace": workspace,
                         "child": 0, "nickname": "x" }),
            );
            let err = resp.error.expect("unresolved workspace must be refused");
            assert_eq!(err.code, -32602, "{}", err.message);
            assert!(err.message.contains("is not open"), "{}", err.message);
            let kept = e
                .runtime
                .child_terminals
                .find_child(&external("codex:f"), 0)
                .expect("child stays");
            assert_eq!(kept.nickname.as_deref(), Some("kept"), "{workspace}");
        }
        let bad = handle_set_nickname(
            &mut e,
            json!(2),
            &json!({ "parent_external": "codex:f", "workspace": 1, "child": 0, "nickname": "x" }),
        );
        assert_eq!(bad.error.expect("non-string workspace").code, -32602);
        for (workspace, name) in [("workspace", "by-name"), ("1", "by-id")] {
            let resp = ok(handle_set_nickname(
                &mut e,
                json!(3),
                &json!({ "parent_external": "codex:f", "workspace": workspace,
                         "child": 0, "nickname": name }),
            ));
            assert_eq!(resp["nickname"], json!(name));
        }
    }

    /// 없는 child는 유효한 번호를 안내하는 -32602이고 registry는 그대로다.
    #[test]
    fn set_nickname_rejects_an_unknown_child() {
        let mut e_session = engine_with_extra(&[5913]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.runtime
            .child_terminals
            .register_child(parent, child(5913, 0));
        let resp = handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": 7, "nickname": "x" }),
        );
        let err = resp.error.expect("unknown child must fail");
        assert_eq!(err.code, -32602);
        assert!(
            err.message.contains("valid child indices: 0"),
            "{}",
            err.message
        );
        let kept = e.runtime.child_terminals.find_child(&parent.into(), 0);
        assert_eq!(kept.expect("child kept").nickname, None);
    }

    #[test]
    fn set_nickname_rejects_bad_nickname_arguments_without_changing_the_child() {
        let mut e_session = engine_with_extra(&[5914]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let mut entry = child(5914, 0);
        entry.nickname = Some("keep".into());
        e.runtime.child_terminals.register_child(parent, entry);
        let long = "n".repeat(NICKNAME_MAX_BYTES + 1);
        for (params, needle) in [
            (json!({ "child": 0 }), "missing 'nickname'"),
            (json!({ "child": 0, "nickname": "" }), "must not be empty"),
            (
                json!({ "child": 0, "nickname": "a\nb" }),
                "control characters",
            ),
            (
                json!({ "child": 0, "nickname": long }),
                "longer than 256 bytes",
            ),
            (json!({ "child": 0, "nickname": 3 }), "must be a string"),
            (
                json!({ "child": 0, "nickname": "x", "clear": true }),
                "not both",
            ),
            (json!({ "child": 0, "clear": "yes" }), "must be a boolean"),
            (json!({ "child": 0, "clear": false }), "missing 'nickname'"),
            (
                json!({ "surface": parent, "parent_external": "codex:n", "child": 0, "nickname": "x" }),
                "not both",
            ),
            (json!({ "nickname": "x" }), "missing 'child'"),
        ] {
            let resp = handle_set_nickname(&mut e, json!(1), &params);
            let err = resp.error.unwrap_or_else(|| panic!("{params} must fail"));
            assert_eq!(err.code, -32602, "{params}");
            assert!(err.message.contains(needle), "{params}: {}", err.message);
        }
        let kept = e.runtime.child_terminals.find_child(&parent.into(), 0);
        assert_eq!(kept.expect("child kept").nickname.as_deref(), Some("keep"));
        let at_limit = "n".repeat(NICKNAME_MAX_BYTES);
        assert!(
            handle_set_nickname(
                &mut e,
                json!(2),
                &json!({ "child": 0, "nickname": at_limit })
            )
            .error
            .is_none(),
            "256 byte까지는 받는다"
        );
    }

    /// 선택 인자의 null은 생략이고 `clear: false`도 생략과 같다(API 규약 "잘못된 인자는 거절한다").
    #[test]
    fn set_nickname_reads_null_and_false_as_omitted() {
        let mut e_session = engine_with_extra(&[5916]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.runtime
            .child_terminals
            .register_child(parent, child(5916, 0));
        let set = ok(handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "child": 0, "nickname": "a", "clear": false }),
        ));
        assert_eq!(set["nickname"], json!("a"));
        let cleared = ok(handle_set_nickname(
            &mut e,
            json!(2),
            &json!({ "child": 0, "nickname": null, "clear": true }),
        ));
        assert_eq!(cleared["nickname"], Value::Null);
        let err = handle_set_nickname(
            &mut e,
            json!(3),
            &json!({ "child": 0, "nickname": null, "clear": null }),
        )
        .error
        .expect("nothing to set");
        assert!(
            err.message.contains("missing 'nickname'"),
            "{}",
            err.message
        );
    }

    /// 저장 명령이 접수되지 않으면 성공으로 답하지 않는다.
    #[test]
    fn set_nickname_reports_a_refused_save_as_an_error() {
        let mut e_session = engine_with_extra(&[5915]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        // 파일 writer가 붙지 않은 registry는 저장 명령을 접수하지 않는다(초기 읽기 대기로 거절).
        e.runtime.child_terminals =
            crate::runtime::child_terminal::ChildTerminalRegistry::default();
        e.runtime
            .child_terminals
            .register_child(parent, child(5915, 0));
        let resp = handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": 0, "nickname": "x" }),
        );
        let err = resp.error.expect("refused save must fail");
        assert_eq!(err.code, -32603, "{}", err.message);
        assert!(err.message.contains("child registry"), "{}", err.message);
        let kept = e.runtime.child_terminals.find_child(&parent.into(), 0);
        assert_eq!(
            kept.expect("child kept").nickname,
            None,
            "오류로 답한 값이 조회에 남지 않는다"
        );
    }

    /// 접수된 저장이 쓰기 오류로 끝나면 -32603이다. 성공 응답은 값이 파일에 쓰인 뒤에 온다.
    #[cfg(unix)]
    #[test]
    fn set_nickname_answers_a_failed_write_with_an_error_and_success_after_the_write() {
        use std::os::unix::fs::PermissionsExt;
        let mut e_session = engine_with_extra(&[5919]);
        let mut e = e_session.borrow_mut();
        let parent = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let path = tasty_utils::path::tasty_home()
            .expect("isolated test home")
            .join("child-terminals-write-error.json");
        let hub = crate::runtime::child_terminal::Hub::new(Some(path.clone()));
        hub.test_attach_writer();
        e.runtime.child_terminals =
            crate::runtime::child_terminal::ChildTerminalRegistry::from_hub(&hub);
        e.runtime
            .child_terminals
            .register_child(parent, child(5919, 0));
        let first = ok(handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "surface": parent, "child": 0, "nickname": "first" }),
        ));
        assert_eq!(first["nickname"], json!("first"));
        let on_disk = std::fs::read_to_string(&path).expect("registry file");
        assert!(on_disk.contains("\"first\""), "{on_disk}");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
        let resp = handle_set_nickname(
            &mut e,
            json!(2),
            &json!({ "surface": parent, "child": 0, "nickname": "second" }),
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let err = resp.error.expect("a failed write must not answer success");
        assert_eq!(err.code, -32603, "{}", err.message);
        assert!(err.message.contains("child registry"), "{}", err.message);
        let on_disk = std::fs::read_to_string(&path).expect("registry file");
        assert!(!on_disk.contains("\"second\""), "{on_disk}");
    }

    /// 다른 창이 등록한 자식은 이 창에서 고치지 않는다. 고치면 이 창이 그 자식을 소유한 것으로 기록해
    /// 다음 reconcile 이 다른 창의 관계를 지운다.
    #[test]
    fn set_nickname_refuses_a_child_open_in_another_window() {
        let mut e_session = engine_with_extra(&[]);
        let mut e = e_session.borrow_mut();
        // 두 창이 같은 파일을 공유하는 모양을 만들려고 격리 홈 아래 별도 파일의 hub 하나에 client 둘을 붙인다.
        let path = tasty_utils::path::tasty_home()
            .map(|home| home.join("child-terminals-two-windows.json"));
        assert!(path.is_some(), "isolated test home");
        let hub = crate::runtime::child_terminal::Hub::new(path);
        hub.test_attach_writer();
        let mut other = crate::runtime::child_terminal::ChildTerminalRegistry::from_hub(&hub);
        other.register_child(external("codex:w"), child(7777, 0));
        let ticket = other.submit().expect("other window save admitted");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let saved = loop {
            if let Some(result) = ticket.poll() {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "other window save missing"
            );
            std::thread::yield_now();
        };
        assert!(saved.is_ok(), "{saved:?}");
        // 이미 떠 있는 client 는 다른 client 의 저장을 자기 저장 결과로만 받는다. 저장 뒤에 붙인 client 는
        // 그 파일 상태로 시작하므로, 다른 창이 기록한 자식을 이 창이 보는 모양이 된다.
        e.runtime.child_terminals =
            crate::runtime::child_terminal::ChildTerminalRegistry::from_hub(&hub);
        let resp = handle_set_nickname(
            &mut e,
            json!(1),
            &json!({ "parent_external": "codex:w", "child": 0, "nickname": "x" }),
        );
        let err = resp.error.expect("another window's child must be refused");
        assert_eq!(err.code, -32602);
        assert!(
            err.message.contains("not open in this window"),
            "{}",
            err.message
        );
        e.reconcile_child_terminals();
        let kept = e
            .runtime
            .child_terminals
            .find_child(&external("codex:w"), 0);
        assert_eq!(kept.expect("other window's child kept").nickname, None);
    }
}
