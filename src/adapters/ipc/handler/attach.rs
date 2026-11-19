//! attach 점유의 획득·해제·조회. 대상은 ID로 지정하며 사용자 포커스를 바꾸지 않는다.
//! 별도 토큰 대신 SSH와 loopback 연결을 신뢰한다. client_id는 stream.open에서 발급한다.

#[cfg(feature = "gui")]
use super::params;
use crate::runtime::engine_access::EngineMut;
use serde_json::json;

use crate::runtime::engine_read::EngineRead;
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;

fn require_client_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    super::params::require_u32(params, "client_id", id)
}

/// `attach.acquire` { surface_id, client_id } → 배타 lock 획득(동시 attach 거부).
pub(crate) fn handle_acquire(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client_id = match require_client_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // 실재하는(또는 deferred) 터미널 surface 만 점유 대상. 없는 id 점유 방지.
    if !engine.runtime.terminals.contains(surface_id)
        && !engine.as_ref().is_surface_deferred(surface_id)
    {
        return JsonRpcResponse::invalid_params(
            id,
            format!("Surface {surface_id} not found or not attachable"),
        );
    }
    match engine.live.occupancy.acquire(surface_id, client_id) {
        Ok(lock) => JsonRpcResponse::success(
            id,
            json!({
                "attached": true,
                "surface_id": surface_id,
                "holder": lock.holder,
                "granted_seq": lock.granted_seq,
            }),
        ),
        Err(crate::core::attach::AttachError::AlreadyAttached { holder }) => {
            JsonRpcResponse::error(id, -32020, format!("already attached by client {holder}"))
        }
        Err(e) => JsonRpcResponse::error(id, -32020, format!("{e:?}")),
    }
}

/// `attach.release` { surface_id, client_id } → 정상 해제(holder 본인).
pub(crate) fn handle_release(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let client_id = match require_client_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match engine.live.occupancy.release(surface_id, client_id) {
        Ok(()) => {
            JsonRpcResponse::success(id, json!({ "released": true, "surface_id": surface_id }))
        }
        Err(e) => JsonRpcResponse::error(id, -32021, format!("{e:?}")),
    }
}

/// `attach.force_detach` { surface_id } → 점유 테두리의 강제 끊기 버튼과 같은 `force_detach`로 hard
/// 점유를 풀고 holder에게 `force_detached`를 보낸다. 대상이 workspace 점유의 멤버면 그 workspace 점유
/// 전체를 풀고 응답에 `workspace_id`를 싣는다. soft 점유는 건드리지 않는다. 점유가 없으면
/// `force_detached: false`다.
pub(crate) fn handle_force_detach(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let detached = engine.force_detach(surface_id);
    let mut result = forced_detach_result(detached);
    result["surface_id"] = json!(surface_id);
    if let Some(workspace_id) = detached.and_then(|d| d.workspace_id) {
        result["workspace_id"] = json!(workspace_id);
    }
    JsonRpcResponse::success(id, result)
}

/// `attach.force_detach_workspace` { workspace_id } → 사이드바 확인 팝업과 같은 엔진 경로로
/// workspace 점유와 멤버 제한을 풀고 holder에게 `force_detached`를 보낸다.
pub(crate) fn handle_force_detach_workspace(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let workspace_id = match super::params::require_u32(params, "workspace_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut result = forced_detach_result(engine.force_detach_workspace(workspace_id));
    result["workspace_id"] = json!(workspace_id);
    JsonRpcResponse::success(id, result)
}

/// 두 메서드의 공통 응답. `notified`는 알릴 holder 연결이 있었고 그 큐에 `force_detached` 통지를
/// 넣었는지다. 넣었으면 서버가 그 통지를 쓴 뒤 연결을 닫는다. 점유 해제는 되돌리지 않으므로 오류가
/// 아니라 이 필드로 알린다. 점유나 연결이 없으면 false다. 통지를 넣지 못했으면 서버가 곧바로 연결을
/// 닫아 holder는 통지 없이 끊김만 보며, 연결 끊김으로 보고 다시 붙을 수 있다.
fn forced_detach_result(
    detached: Option<crate::runtime::engine_access::ForcedDetach>,
) -> serde_json::Value {
    json!({
        "force_detached": detached.is_some(),
        "holder": detached.map(|d| d.holder),
        "notified": detached.is_some_and(|d| d.notified),
    })
}

/// attach 대상 포트가 이 인스턴스의 IPC 포트면 처리를 시작하기 전에 거절 응답을 만든다.
/// 자기 자신을 mirror하면 그 mirror가 원본 workspace를 점유해 로컬 입력이 막힌다.
/// 연결 직전의 `connect_unless_self` 검사는 비동기로 해석한 엔드포인트에 계속 적용한다.
#[cfg(feature = "gui")]
pub(crate) fn refuse_own_port(
    own_port: Option<u16>,
    port: u16,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    (own_port == Some(port)).then(|| {
        JsonRpcResponse::invalid_params(
            id,
            format!(
                "attach target port {port} is this instance's own IPC port; attaching to itself is refused"
            ),
        )
    })
}

/// `attach.into_gui`의 대상 port와 원격 workspace를 읽고 자기 포트를 거절한다.
/// GUI App 층만 사용한다. 헤드리스에는 mirror 큐를 처리할 쪽이 없어 engine 처리기에 arm을 두지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn into_gui_target(
    own_port: Option<u16>,
    id: &serde_json::Value,
    params: &serde_json::Value,
) -> Result<(u16, u32), JsonRpcResponse> {
    let port = match params::opt_int::<u64>(params, "port", id)? {
        Some(v) if v <= u16::MAX as u64 => v as u16,
        _ => {
            return Err(JsonRpcResponse::invalid_params(
                id.clone(),
                "Missing/invalid 'port' parameter",
            ));
        }
    };
    let workspace = super::params::require_u32(params, "workspace", id)?;
    if let Some(refused) = refuse_own_port(own_port, port, id.clone()) {
        return Err(refused);
    }
    Ok((port, workspace))
}

/// `attach.into_gui`의 접수 응답.
#[cfg(feature = "gui")]
pub(crate) fn into_gui_queued(id: serde_json::Value, port: u16, workspace: u32) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({ "queued": true, "port": port, "workspace": workspace }),
    )
}

/// surface와 workspace 점유 목록을 함께 반환한다.
pub(crate) fn handle_list(engine: &EngineRead<'_>, id: serde_json::Value) -> JsonRpcResponse {
    let arr: Vec<_> = engine
        .live
        .occupancy
        .locks_snapshot()
        .into_iter()
        .map(|(sid, l)| {
            json!({
                "surface_id": sid,
                "holder": l.holder,
                "granted_seq": l.granted_seq,
            })
        })
        .collect();
    let workspaces: Vec<_> = engine
        .live
        .occupancy
        .workspaces_snapshot()
        .into_iter()
        .map(|(ws, l)| {
            json!({
                "workspace_id": ws,
                "holder": l.holder,
                "granted_seq": l.granted_seq,
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "attached": arr, "workspaces": workspaces }))
}
