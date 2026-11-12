use serde_json::json;

use super::params;
use crate::runtime::engine_access::EngineRef;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::stream::InheritCwd;

/// create/update params 의 `category` 를 카테고리 id 로 해석한다.
/// - 필드 없음 → `Ok(None)` (호출자가 normal 기본값 유지).
/// - 숫자(id) 또는 문자열(이름/id 토큰) → 존재하면 `Ok(Some(id))`.
/// - 주어졌으나 해석 불가 → `Err(메시지)`.
pub(crate) fn resolve_category_param(
    engine: &crate::core::CoreState,
    params: &serde_json::Value,
) -> Result<Option<crate::model::WorkspaceCategoryId>, String> {
    let Some(token) = params::read_id_or_name(params, "category")? else {
        return Ok(None);
    };
    match engine.resolve_category(&token) {
        Some(id) => Ok(Some(id)),
        None => Err(format!("unknown category: {token}")),
    }
}

/// 저장된 원격 행의 연결 대상. 로컬 workspace와 에이전트 mirror는 null이다.
pub(crate) fn remote_row_to_json(
    binding: &Option<crate::model::RemoteBinding>,
) -> serde_json::Value {
    match binding {
        Some(binding) => json!({
            "profile": binding.profile,
            "remote_workspace": binding.remote_workspace,
        }),
        None => serde_json::Value::Null,
    }
}

/// 저장된 원격 행의 연결 상태. 저장된 행이 아니면 null이다.
fn remote_state(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    ws: &crate::model::Workspace,
) -> serde_json::Value {
    if ws.remote_row.is_none() {
        return serde_json::Value::Null;
    }
    // 헤드리스는 원격 행을 연결하지 않아 상태가 실리지 않는다.
    match engine.remote.row_states.get(&ws.id) {
        Some(state) => json!(state),
        None => json!("not_connected"),
    }
}

pub fn handle_workspace_list(
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let workspaces: Vec<_> = engine
        .workspaces()
        .into_iter()
        .enumerate()
        .map(|(i, ws)| {
            let sids = ws.all_surface_ids();
            json!({
                "id": ws.id,
                "name": ws.name,
                "subtitle": ws.subtitle,
                "description": ws.description,
                "active": i == window.active_workspace_index(engine),
                "pane_count": ws.pane_layout().all_pane_ids().len(),
                "busy_count": engine.read().busy_count(&sids),
                // 저장된 원격 행의 연결 대상. 에이전트 mirror는 mirror만 true다.
                "remote_row": remote_row_to_json(&ws.remote_row),
                "remote_state": remote_state(engine, ws),
                "mirror": ws.mirror,
                "category": ws.category,
                "category_name": engine.category_name(ws.category),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!(workspaces))
}

/// surface_id를 지정하면 그 대상의 cwd를 쓴다. 생략한 경우만 창의 포커스 surface를 따른다.
fn inherit_cwd_for_create(
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &EngineRef<'_>,
    named_surface: Option<u32>,
) -> Option<std::path::PathBuf> {
    match named_surface {
        Some(surface_id) => window.resolve_inherit_cwd_from_surface(engine, surface_id),
        None => window.resolve_inherit_cwd(engine),
    }
}

/// 첫 surface 생성의 cwd를 정한다. 명시값은 kind와 관계없이 create에 넘긴다(explorer는 시작 폴더로 쓴다).
/// 명시값이 없을 때 상속하는 것은 terminal뿐이다. 원격 mirror의 경로는 로컬 workspace에 상속하지 않는다.
/// 잘못된 surface_id는 포커스 대상으로 대체하지 않고 거절한다.
pub(crate) fn resolve_create_cwd(
    params: &serde_json::Value,
    kind: &str,
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &EngineRef<'_>,
    id: &serde_json::Value,
) -> Result<Option<std::path::PathBuf>, JsonRpcResponse> {
    let explicit_cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);
    if let Some(p) = &explicit_cwd
        && !p.is_dir()
    {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("cwd does not exist: {}", p.display()),
        ));
    }
    let named_surface = params::opt_int::<u32>(params, "surface_id", id)?;
    let source = named_inherit_source(params, engine, id)?;
    Ok(match explicit_cwd {
        Some(cwd) => Some(cwd),
        None if kind == "terminal" => match source {
            None => inherit_cwd_for_create(window, engine, named_surface),
            Some(InheritCwd::Surface(surface)) => engine.local_surface_cwd(surface),
            Some(_) => None,
        },
        None => None,
    })
}

/// `inherit_cwd` 인자. 원격 client 가 자기 포커스로 고른 이 인스턴스의 터미널(`{"surface": N}`) 또는 상속 안 함
/// (`"none"`)이다. 생략하면 None 이고 기존 규칙(`surface_id` 또는 창의 포커스, 이 인스턴스의 설정)을 쓴다.
/// 원본을 고른 요청자는 이미 자기 설정으로 판단했으므로 이 인스턴스의 `inherit_cwd` 설정으로 막지 않는다.
/// 원본은 실재하는 terminal 이어야 한다. 아니면 포커스로 대체하지 않고 거절한다. 모르는 값도 거절한다.
fn named_inherit_source(
    params: &serde_json::Value,
    engine: &EngineRef<'_>,
    id: &serde_json::Value,
) -> Result<Option<InheritCwd>, JsonRpcResponse> {
    let Some(value) = params.get("inherit_cwd").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let refuse = |message: String| Err(JsonRpcResponse::invalid_params(id.clone(), message));
    match serde_json::from_value::<InheritCwd>(value.clone()) {
        Ok(InheritCwd::NoInherit) => Ok(Some(InheritCwd::NoInherit)),
        Ok(InheritCwd::Surface(surface)) => {
            match engine.find_surface_by_id(surface).map(|found| found.kind()) {
                Some("terminal") => Ok(Some(InheritCwd::Surface(surface))),
                Some(kind) => refuse(format!(
                    "inherit_cwd surface {surface} is a {kind} surface, not a terminal"
                )),
                None => refuse(tasty_utils::target::unowned_target_message(
                    "surface",
                    u64::from(surface),
                    "workspace.create.inherit_cwd",
                )),
            }
        }
        Ok(InheritCwd::Unknown(_)) | Err(_) => {
            refuse(format!("unsupported inherit_cwd value {value}"))
        }
    }
}

pub(crate) fn last_workspace_refusal() -> &'static str {
    if cfg!(feature = "gui") {
        "Refusing to close the last workspace — closing the window instead is a separate \
         decision; use 'window.close' explicitly if that is what you want"
    } else {
        "Refusing to close the last workspace — the last workspace of a headless instance \
         cannot be closed (there is no window to close instead)"
    }
}
