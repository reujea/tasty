use serde_json::json;

use super::params::{self, p_try};
use crate::model::{WorkspaceAttachMapping, WorkspaceAttachTarget};
use tasty_ipc::protocol::JsonRpcResponse;

/// 단계 7 — workspace.create/update params 에서 SSH attach 매핑을 파싱한다.
/// `attach_profile`(저장 프로필) 우선, 없으면 `attach_ssh`(1회성 인라인).
/// 둘 다 없으면 None(매핑 없음).
/// `Result` 를 반환하는 이유: 잘못된 `attach_remote_workspace` 를 `None` 으로 만들면
/// **매핑 없음**과 구별되지 않아, 사용자가 지정한 원격 워크스페이스 대신 매핑 없이
/// 조용히 진행된다.
fn parse_attach_mapping(
    params: &serde_json::Value,
) -> Result<Option<WorkspaceAttachMapping>, String> {
    let remote_workspace = params::read_int::<u32>(params, "attach_remote_workspace")?;
    if let Some(name) = params
        .get("attach_profile")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Ok(Some(WorkspaceAttachMapping {
            target: WorkspaceAttachTarget::Profile {
                name: name.to_string(),
            },
            remote_workspace,
        }));
    }
    if let Some(host) = params
        .get("attach_ssh")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Ok(Some(WorkspaceAttachMapping {
            target: WorkspaceAttachTarget::Inline {
                host: host.to_string(),
                remote_tasty: None,
                port_mode: None,
                port_file: None,
            },
            remote_workspace,
        }));
    }
    Ok(None)
}

/// `attach_ssh` host 가 self(loopback) 대상(`127.0.0.1:PORT`/`localhost:PORT`/
/// `[::1]:PORT`)인지 판정한다. release 빌드의 self-attach 매핑 차단에 쓴다(원칙 1 ②).
/// (`src/app/auto_attach.rs::parse_loopback_port` 의 판정과 동일 규칙 — 그쪽은
/// 공유 실행단 헬퍼라 보존하고, 여기선 입력단 거부용 술어만 둔다.)
#[cfg(not(debug_assertions))]
fn is_loopback_attach_host(host: &str) -> bool {
    let h = if host.strip_prefix("[::1]:").is_some() {
        "::1"
    } else if let Some((h, _port)) = host.rsplit_once(':') {
        h
    } else {
        return false;
    };
    matches!(h, "127.0.0.1" | "localhost" | "::1")
}

/// release 빌드에서 self(loopback) attach 매핑을 입력단에서 거부한다(원칙 1 ②).
/// 로컬 self-mirror 는 사용자 입력 재현 성격이라 debug 빌드 `tasty debug attach`
/// 전용 — workspace 매핑(`--ssh 127.0.0.1:PORT`)으로 우회 self-attach 를 막는다.
#[cfg(not(debug_assertions))]
fn reject_loopback_attach(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    let host = params
        .get("attach_ssh")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    if is_loopback_attach_host(host) {
        return Some(JsonRpcResponse::invalid_params(
            id.clone(),
            "loopback(self) attach 매핑은 release 빌드에서 지원되지 않습니다 \
             (로컬 self-attach 는 debug 빌드 `tasty debug attach` 전용).",
        ));
    }
    None
}

/// create/update params 의 `category` 를 카테고리 id 로 해석한다.
/// - 필드 없음 → `Ok(None)` (호출자가 normal 기본값 유지).
/// - 숫자(id) 또는 문자열(이름/id 토큰) → 존재하면 `Ok(Some(id))`.
/// - 주어졌으나 해석 불가 → `Err(메시지)`.
fn resolve_category_param(
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

/// 매핑을 JSON 으로 노출(workspace.list 의 read — 원칙 3 read 허용).
fn mapping_to_json(mapping: &Option<WorkspaceAttachMapping>) -> serde_json::Value {
    match mapping {
        Some(m) => serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    }
}

pub fn handle_workspace_list(
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let workspaces: Vec<_> = engine
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let sids = ws.all_surface_ids();
            json!({
                "id": ws.id,
                "name": ws.name,
                "subtitle": ws.subtitle,
                "description": ws.description,
                "active": i == window.active_workspace_index(),
                "pane_count": ws.pane_layout().all_pane_ids().len(),
                "busy_count": engine.busy_count(&sids),
                "attach_mapping": mapping_to_json(&ws.attach_mapping),
                // 지금 원격을 attach 한 client mirror 인지. `attach_mapping`(활성화 시
                // attach 할 매핑)과는 다른 축이라 그걸로 유추할 수 없다. GUI 사이드바만
                // 알던 정보를 에이전트 조회 경로에도 노출한다(원칙 2).
                "mirror": ws.mirror,
                "category": ws.category,
                "category_name": engine.category_name(ws.category),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!(workspaces))
}

/// `cwd` 를 생략한 `workspace.create` 의 상속 원본.
///
/// `surface_id` 를 지목했으면 **그 surface** 의 cwd 다 — 라우터가 그 키로 주인 창을 고른
/// 뒤라 이 창(engine)에 있다. 지목이 없을 때만 이 창의 포커스 surface 로 떨어진다(기존 동작).
/// 지목했는데 창의 포커스를 보면 결과가 사용자가 그 창에서 무엇을 보고 있는지에 좌우된다
/// (원칙 3). 같은 키가 창과 원본을 함께 정하는 이유는
/// `docs/adr/0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md`.
fn inherit_cwd_for_create(
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::core::CoreState,
    named_surface: Option<u32>,
) -> Option<std::path::PathBuf> {
    match named_surface {
        Some(surface_id) => window.resolve_inherit_cwd_from_surface(engine, surface_id),
        None => window.resolve_inherit_cwd(engine),
    }
}

/// `workspace.create` 의 params 에서 새 워크스페이스에 줄 cwd 를 정한다.
///
/// terminal 의 cwd inherit 은 호출자가 미리 결정해 payload 로 넘긴다 (Core 는
/// focus state 모름). 그 외 kind 는 cwd 미사용. 새 워크스페이스는 로컬이므로 원본이
/// mirror surface 면 inherit 은 `None`(= 홈)이고, 명시 `cwd` 는 그대로 존중한다
/// (`docs/architecture/invariants/surface-cwd.md` §3-2).
/// 창을 지목한 `surface_id` 가 상속 원본이다. 숫자가 아닌 값은 거절한다 — 무시하면
/// 라우팅도 못 짚은 채 포커스 surface 로 조용히 떨어진다.
fn resolve_create_cwd(
    params: &serde_json::Value,
    kind: &str,
    window: &dyn crate::ipc::window_port::IpcWindow,
    engine: &crate::core::CoreState,
    id: &serde_json::Value,
) -> Result<Option<std::path::PathBuf>, JsonRpcResponse> {
    let explicit_cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);
    // CLI 가 absolute path 로 정규화해 보낸다는 contract — 2 차 방어로 호스트도
    // 디렉토리 존재 검증. plugin 의 직접 IPC 경로도 함께 보호.
    if let Some(p) = &explicit_cwd
        && !p.is_dir()
    {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("cwd does not exist: {}", p.display()),
        ));
    }
    let named_surface = params::opt_int::<u32>(params, "surface_id", id)?;
    Ok(if kind == "terminal" {
        explicit_cwd.or_else(|| inherit_cwd_for_create(window, engine, named_surface))
    } else {
        None
    })
}

pub fn handle_workspace_create(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    // release: self(loopback) attach 매핑 입력단 거부(원칙 1 ②).
    #[cfg(not(debug_assertions))]
    if let Some(resp) = reject_loopback_attach(params, &id) {
        return resp;
    }
    let kind = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("terminal");
    let resolved_cwd = p_try!(resolve_create_cwd(params, kind, window, engine, &id));

    // 필수 파라미터 검증 (registry create 함수도 검사하지만, 명확한 에러 메시지를 위해
    // 선검증). registry 의 required_params(preset_fields.required)로 generic 검증.
    if let Some(def) = engine.surface_registry.get_live(kind)
        && let Some(missing) = def.first_missing_required_param(params)
    {
        return JsonRpcResponse::invalid_params(
            id,
            format!("Missing '{missing}' parameter for {kind} type"),
        );
    }

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let subtitle = params
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let description = params
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let intent = crate::core::intent::DomainIntent::CreateWorkspace {
        cwd: resolved_cwd,
        kind: kind.to_string(),
        surface_params: params.clone(),
        name,
        subtitle,
        description,
        // IPC 는 생성 후 resolve_category_param 으로 소속을 별도 지정하므로 여기선 None.
        category: None,
    };

    let events = match core.apply(engine, intent) {
        Ok(events) => events,
        Err(e) => return JsonRpcResponse::internal_error(id, e.to_string()),
    };

    // events 안에 정확히 하나의 WorkspaceCreated 가 들어있다.
    let Some(crate::core::intent::CoreEvent::WorkspaceCreated {
        id: workspace_id,
        index,
        surface_id,
        renamed_name,
        renamed_subtitle,
        renamed_description,
    }) = events.into_iter().next()
    else {
        return JsonRpcResponse::internal_error(
            id,
            "Core::apply returned no WorkspaceCreated event",
        );
    };

    // cascade: host event 발화 (rename 필드 있을 때). IPC 는 Agent origin 이므로
    // active 전환은 하지 않는다 (`cascade_workspace_created` 가 origin 보고 분기).
    let agent_origin = crate::intent::IntentOrigin::Agent {
        source: crate::intent::AgentSource::Ipc,
    };
    window.cascade_workspace_created(
        engine,
        &agent_origin,
        0,
        crate::app::dispatch_domain::WorkspaceCreatedCascade {
            workspace_id,
            index,
            surface_id,
            renamed_name: renamed_name.clone(),
            renamed_subtitle: renamed_subtitle.clone(),
            renamed_description: renamed_description.clone(),
        },
    );

    // S-WSCAT — 카테고리 소속 설정(있으면). 미지정이면 normal(생성자 기본값) 유지.
    // 카테고리 변경은 사용자 active 에 닿지 않는다(원칙 1·3). attach_mapping 과 동형으로
    // 직접 set + dirty.
    match resolve_category_param(engine, params) {
        Ok(Some(cat_id)) => {
            engine.workspaces[index].set_category(cat_id);
            engine.mark_layout_dirty();
        }
        Ok(None) => {}
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    }

    // 단계 7 — SSH attach 매핑 설정(있으면). layout.json 영속을 위해 dirty 표시.
    if let Some(mapping) = match parse_attach_mapping(params) {
        Ok(v) => v,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    } {
        engine.workspaces[index].set_attach_mapping(Some(mapping));
        engine.mark_layout_dirty();
    }

    let ws = &engine.workspaces[index];
    JsonRpcResponse::success(
        id,
        json!({
            "id": ws.id,
            "name": ws.name,
            "subtitle": ws.subtitle,
            "description": ws.description,
            "index": index,
            "surface_id": surface_id,
            "attach_mapping": mapping_to_json(&ws.attach_mapping),
            "category": ws.category,
        }),
    )
}

pub fn handle_workspace_update(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    // release: self(loopback) attach 매핑 입력단 거부(원칙 1 ②).
    #[cfg(not(debug_assertions))]
    if let Some(resp) = reject_loopback_attach(params, &id) {
        return resp;
    }
    // workspace_id resolve — `id` 우선, 없으면 `index` 로 lookup.
    // `id` 가 **왔는데 잘못된** 경우 `index` 로 흘러내리지 않는다 — 흘러내리면 오타
    // 하나가 엉뚱한 워크스페이스를 성공적으로 가리킨다.
    let id_param = match super::params::optional_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let workspace_id = if let Some(ws_id) = id_param {
        ws_id
    } else if let Some(i) = p_try!(params::opt_int::<u64>(params, "index", &id)) {
        let idx = i as usize;
        if idx >= engine.workspaces.len() {
            return JsonRpcResponse::invalid_params(
                id,
                format!(
                    "Workspace index {} out of range (0..{})",
                    idx,
                    engine.workspaces.len()
                ),
            );
        }
        engine.workspaces[idx].id
    } else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' or 'index' parameter");
    };

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let subtitle = params
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let description = params
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let intent = crate::core::intent::DomainIntent::UpdateWorkspaceMeta {
        workspace_id,
        name,
        subtitle,
        description,
    };

    let events = match core.apply(engine, intent) {
        Ok(events) => events,
        Err(e) => return JsonRpcResponse::invalid_params(id, e.to_string()),
    };

    let Some(crate::core::intent::CoreEvent::WorkspaceMetaUpdated {
        workspace_id,
        index,
        name,
        subtitle,
        description,
    }) = events.into_iter().next()
    else {
        return JsonRpcResponse::internal_error(
            id,
            "Core::apply returned no WorkspaceMetaUpdated event",
        );
    };

    window.cascade_workspace_meta_updated(workspace_id, name, subtitle, description);

    // S-WSCAT — 카테고리 소속 변경(있으면). 사용자 active 불변(원칙 1·3).
    match resolve_category_param(engine, params) {
        Ok(Some(cat_id)) => {
            if let Err(e) = engine.set_workspace_category(workspace_id, cat_id) {
                return JsonRpcResponse::invalid_params(id, e.to_string());
            }
            engine.mark_layout_dirty();
        }
        Ok(None) => {}
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    }

    // 단계 7 — SSH attach 매핑 갱신/해제. `attach_clear` 가 우선(해제), 아니면 파싱한
    // 매핑이 있으면 설정. 어느 쪽이든 layout.json 영속을 위해 dirty 표시.
    let clear = params
        .get("attach_clear")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if clear {
        engine.workspaces[index].set_attach_mapping(None);
        engine.mark_layout_dirty();
    } else if let Some(mapping) = match parse_attach_mapping(params) {
        Ok(v) => v,
        Err(msg) => return JsonRpcResponse::invalid_params(id, msg),
    } {
        engine.workspaces[index].set_attach_mapping(Some(mapping));
        engine.mark_layout_dirty();
    }

    let ws = &engine.workspaces[index];
    JsonRpcResponse::success(
        id,
        json!({
            "id": ws.id,
            "name": ws.name,
            "subtitle": ws.subtitle,
            "description": ws.description,
            "index": index,
            "attach_mapping": mapping_to_json(&ws.attach_mapping),
            "category": ws.category,
        }),
    )
}

/// `workspace.close` — 워크스페이스를 통째로 닫는다. 대상은 **id 로 직접 지정**하며
/// (`index` 도 받지만 `workspace.update` 와 같은 보조 경로다) 활성 상태에 의존하지 않는다.
///
/// 사용자 상태를 건드리지 않기 위해 두 가지를 지킨다.
///
/// 1. **닫은 항목 히스토리에 쌓지 않는다** — `save_snapshot = false`. 되돌리기 스택은
///    사용자가 자기 손으로 닫은 것만 담는다(원칙 1).
/// 2. **포커스를 옮기지 않는다** — `close_workspace_at` 이 제거 직후
///    `fix_workspace_pointers_after_removal` 로 인덱스 활성 포인터를 대상 기준으로
///    보정한다. 활성 워크스페이스 자신을 닫을 때만 이동한다.
///
/// 거절은 넷이다(대상 해석 실패 제외) — caller 자신의 surface 가 든 워크스페이스 ·
/// mirror 워크스페이스 · **원격 attach 가 하드 점유 중인 surface 를 든 워크스페이스** ·
/// 마지막 하나 남은 워크스페이스. 근거는 [ADR-0120](../../../../docs/adr/0120-agent-workspace-close-boundaries.md).
///
/// 마지막 워크스페이스는 거부한다. GUI 는 그 경우 창까지 닫지만, 창을 없애는 것은
/// 별개의 결정이라 에이전트에게는 `window.close` 라는 명시적 수단을 따로 준다 —
/// 워크스페이스 하나를 닫으라는 요청이 창 종료로 번지지 않게 한다.
pub fn handle_workspace_close(
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let id_param = match super::params::optional_u32(params, "id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let ws_idx = if let Some(ws_id) = id_param {
        match engine.workspaces.iter().position(|w| w.id == ws_id) {
            Some(i) => i,
            None => {
                return JsonRpcResponse::invalid_params(id, format!("Workspace {ws_id} not found"));
            }
        }
    } else if let Some(i) = p_try!(params::opt_int::<u64>(params, "index", &id)) {
        let idx = i as usize;
        if idx >= engine.workspaces.len() {
            return JsonRpcResponse::invalid_params(
                id,
                format!(
                    "Workspace index {} out of range (0..{})",
                    idx,
                    engine.workspaces.len()
                ),
            );
        }
        idx
    } else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'id' or 'index' parameter");
    };

    // 자기 자신 닫기 보호 — pane.close / tab.close 와 같은 규칙. 대상 워크스페이스에
    // caller 의 surface 가 들어 있으면 이 요청은 자기 터미널을 죽인다.
    if let Some(caller) = super::caller_surface_id(params)
        && engine
            .find_workspace_index_for_surface(caller)
            .map(|(i, _)| i)
            == Some(ws_idx)
    {
        return JsonRpcResponse::invalid_params(
            id,
            "Cannot close a workspace that contains your own surface. Move elsewhere first, \
             or use 'tasty close self' to close just your surface.",
        );
    }

    // mirror 워크스페이스는 원격을 attach 해 들고 있는 그림자다. 거두는 절차가
    // 따로 있고(`app::attach_client` 의 `remove_mirror_workspace_from_engine` —
    // mirror 터미널 · busy · attention · mesh 프레임까지 함께 걷는다) 이 경로는
    // 그중 아무것도 하지 않는다. 같은 이유로 `surface.attention.clear` 도 mirror
    // surface 를 거절한다. detach 는 attach 세션 쪽 수단을 쓴다.
    if engine.workspaces[ws_idx].mirror {
        return JsonRpcResponse::invalid_params(
            id,
            "Workspace is a mirror of a remote attach session — detach it from that session \
             instead of closing it here",
        );
    }

    // 원격 attach 가 **하드 점유** 중인 surface 가 하나라도 들어 있으면 거절한다.
    // 점유 중에는 그 surface 를 holder 세션이 소유하고 원격 사용자가 지금 그
    // 터미널을 쓰고 있다 — 여기서 닫으면 남의 작업이 예고 없이 죽는다. 같은 이유로
    // `surface.attention.clear` 도 하드 점유 surface 를 거절한다(ADR-0120 ④).
    // mirror 검사와 별개다: mirror 는 "이 인스턴스가 원격을 비추는 그림자", 하드
    // 점유는 "이 인스턴스의 surface 를 원격 클라이언트가 잡고 있는 상태" 다.
    if let Some(occupied) = engine.workspaces[ws_idx]
        .all_surface_ids()
        .into_iter()
        .find(|sid| engine.attach.is_hard_occupied(*sid))
    {
        return JsonRpcResponse::invalid_params(
            id,
            format!(
                "Workspace holds surface {occupied}, which is occupied by a remote attach \
                 session (hard-occupied) — someone is working in that terminal right now. \
                 Release it from the attaching instance first."
            ),
        );
    }

    if engine.workspaces.len() == 1 {
        return JsonRpcResponse::invalid_params(id, last_workspace_refusal());
    }

    let workspace_id = engine.workspaces[ws_idx].id;
    // `ws_idx` 는 위에서 검증됐고 그 사이 워크스페이스가 제거되지 않으므로 `closed` 는
    // 참일 수밖에 없다. 그래도 상수 `true` 를 싣지 않고 반환값을 그대로 싣는다 — 그
    // 불변이 언젠가 깨지면 응답이 조용히 성공을 주장하는 대신 사실을 말한다.
    let closed =
        window.close_workspace_at(engine, ws_idx, crate::state::WorkspaceCloseOrigin::Agent);
    JsonRpcResponse::success(id, json!({ "closed": closed, "id": workspace_id }))
}

/// 마지막 워크스페이스 거절 문구. gui 는 창을 닫는 별개의 수단(`window.close`)을 권한다.
/// 헤드리스에는 그 수단이 없다 — `window.close` 가 그 조합에서 `-32017`(gated out)이라, 권하면
/// 실행할 수 없는 처방이 된다. 그래서 헤드리스는 닫을 수 없다는 사실만 말한다. 에러 코드는
/// 두 조합 모두 `-32602` 그대로다.
fn last_workspace_refusal() -> &'static str {
    if cfg!(feature = "gui") {
        "Refusing to close the last workspace — closing the window instead is a separate \
         decision; use 'window.close' explicitly if that is what you want"
    } else {
        "Refusing to close the last workspace — the last workspace of a headless instance \
         cannot be closed (there is no window to close instead)"
    }
}

/// 워크스페이스 순서 이동.
///
/// 대상은 **`id` 로 지목한다** — workspace id 는 engine 을 건너 유일해서 라우팅이 주인
/// 창을 짚는다. `from_index` 는 창 안의 위치라 창이 정해지지 않으므로, 그 형태로 부르면
/// 포커스된 창에 떨어진다(`docs/design/policies/focus.md`). 종전 호출을 깨지 않으려고
/// 남겨 두었고 둘을 함께 주면 거절한다 — 어긋났을 때 조용히 한쪽을 고르지 않는다.
/// `to_index` 는 지목된 창 **안에서의** 목적지라 창이 정해진 뒤에는 뜻이 분명하다.
pub fn handle_workspace_move(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let named = p_try!(params::opt_int::<u64>(params, "id", &id));
    let from_index = p_try!(params::opt_int::<u64>(params, "from_index", &id));
    let from = match (named, from_index) {
        (Some(_), Some(_)) => {
            return JsonRpcResponse::invalid_params(
                id,
                "give either 'id' (the workspace to move) or 'from_index', not both",
            );
        }
        (Some(ws_id), None) => match engine.find_workspace_index_for_id(ws_id as u32) {
            Some(i) => i,
            // 라우팅이 주인 창으로 보냈으므로 여기 없으면 그 id 가 죽은 것이다.
            None => {
                return JsonRpcResponse::invalid_params(id, format!("no workspace {ws_id}"));
            }
        },
        (None, Some(f)) => f as usize,
        (None, None) => {
            return JsonRpcResponse::invalid_params(id, "Missing 'id' or 'from_index' parameter");
        }
    };
    let to = match p_try!(params::opt_int::<u64>(params, "to_index", &id)) {
        Some(t) => t as usize,
        None => return JsonRpcResponse::invalid_params(id, "Missing 'to_index' parameter"),
    };

    let intent = crate::core::intent::DomainIntent::MoveWorkspace {
        from_index: from,
        to_index: to,
    };
    let events = match core.apply(engine, intent) {
        Ok(events) => events,
        Err(e) => return JsonRpcResponse::internal_error(id, e.to_string()),
    };

    let moved = matches!(
        events.first(),
        Some(crate::core::intent::CoreEvent::WorkspaceMoved { moved: true, .. })
    );
    if moved {
        window.fix_workspace_pointers_after_move(from, to);
    }
    JsonRpcResponse::success(id, json!({ "moved": moved }))
}

// workspace.select removed: focus is user-only (shortcuts/clicks).

#[cfg(test)]
mod close_tests {
    use super::*;
    use serde_json::json;

    /// 워크스페이스를 하나 더 만들고 그 인덱스를 돌려준다.
    fn add_workspace(engine: &mut crate::core::CoreState) -> u32 {
        let event = crate::core::apply_create_workspace_inner(
            engine,
            crate::core::WorkspaceCreationParams::terminal(),
        )
        .unwrap();
        let crate::core::intent::CoreEvent::WorkspaceCreated { index, .. } = event else {
            panic!("expected WorkspaceCreated");
        };
        engine.workspaces[index].id
    }

    #[test]
    fn closing_the_last_workspace_is_refused() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        assert_eq!(engine.workspaces.len(), 1);
        let only = engine.workspaces[0].id;

        let res = handle_workspace_close(&mut state, &mut engine, json!(1), &json!({ "id": only }));

        let err = res.error.expect("마지막 워크스페이스는 거절해야 한다");
        assert_eq!(
            err.code, -32602,
            "코드는 두 조합 모두 invalid_params 그대로"
        );
        // gui 는 창을 닫는 별개의 수단을 권하고, 헤드리스는 그 수단(`window.close`)이
        // gated out 이라 권하지 않는다 — 실행할 수 없는 처방을 싣지 않는다.
        if cfg!(feature = "gui") {
            assert!(
                err.message.contains("use 'window.close' explicitly"),
                "{}",
                err.message
            );
        } else {
            assert!(
                err.message
                    .contains("the last workspace of a headless instance cannot be closed"),
                "{}",
                err.message
            );
            assert!(!err.message.contains("window.close"), "{}", err.message);
        }
        assert_eq!(
            engine.workspaces.len(),
            1,
            "거절이면 아무것도 닫히지 않는다"
        );
    }

    /// 원격 attach 가 하드 점유한 surface 가 든 워크스페이스는 거절한다.
    ///
    /// 점유 중에는 그 터미널을 원격 사용자가 실제로 쓰고 있다. 이 검사가 없으면
    /// 에이전트가 id 를 훑다가 남의 작업 세션을 예고 없이 죽인다 — 되돌릴 수도 없다.
    /// (ADR-0120 ④. 같은 판단의 선례는 `surface.attention.clear`.)
    #[test]
    fn closing_a_workspace_a_remote_session_occupies_is_refused() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let target = add_workspace(&mut engine);
        let ws_idx = engine
            .workspaces
            .iter()
            .position(|w| w.id == target)
            .expect("방금 만든 워크스페이스가 있어야 한다");
        let occupied = engine.workspaces[ws_idx]
            .all_surface_ids()
            .first()
            .copied()
            .expect("워크스페이스에 surface 가 있어야 한다");
        engine
            .attach
            .acquire(occupied, 1)
            .expect("하드 점유를 잡을 수 있어야 한다");

        let res =
            handle_workspace_close(&mut state, &mut engine, json!(1), &json!({ "id": target }));

        let err = res
            .error
            .expect("하드 점유 중인 워크스페이스는 거절해야 한다");
        assert!(
            err.message.contains("hard-occupied"),
            "거절 사유가 점유임을 알려야 한다: {}",
            err.message
        );
        assert_eq!(
            engine.workspaces.len(),
            2,
            "거절이면 아무것도 닫히지 않는다"
        );
        assert!(
            engine.attach.is_hard_occupied(occupied),
            "거절 경로가 점유 상태를 건드리면 안 된다"
        );
    }

    #[test]
    fn closing_the_workspace_holding_your_own_surface_is_refused() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        add_workspace(&mut engine);
        // caller 자신의 surface 가 든 워크스페이스를 대상으로 지정한다.
        let target = engine.workspaces[0].id;
        let caller = engine.workspaces[0]
            .all_surface_ids()
            .first()
            .copied()
            .expect("워크스페이스에 surface 가 있어야 한다");

        let res = handle_workspace_close(
            &mut state,
            &mut engine,
            json!(1),
            &json!({ "id": target, "caller_surface_id": caller }),
        );

        assert!(
            res.error.is_some(),
            "자기 surface 가 든 대상은 거절해야 한다"
        );
        assert_eq!(engine.workspaces.len(), 2);
    }

    /// 성공 경로 — 이 lane 이 주장하는 것 전부를 한 자리에서 고정한다.
    ///
    /// 거절 테스트만 있으면 `close_workspace_at` 호출 줄에 **도달조차 하지 않아**,
    /// 대상 해석(원칙 3) · 활성 포인터 보정(ADR-0113) · 되돌리기 스택 미기록
    /// (원칙 1) · 응답 계약이 전부 무방비로 남는다. 실제로 그 네 축의 변이가
    /// 전부 생존했다.
    #[test]
    fn closing_a_workspace_the_user_is_not_looking_at_leaves_the_view_alone() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let target_id = add_workspace(&mut engine);
        add_workspace(&mut engine);
        assert_eq!(engine.workspaces.len(), 3);

        // 사용자는 **마지막** 워크스페이스를 보고 있고, 에이전트는 **중간** 것을 닫는다.
        state.active_workspace = 2;
        let viewing_id = engine.workspaces[2].id;
        let target_idx = 1;
        assert_eq!(engine.workspaces[target_idx].id, target_id);
        assert_ne!(
            target_idx as u32, target_id,
            "인덱스와 id 가 같으면 응답이 어느 쪽을 실었는지 구분할 수 없다"
        );

        let res = handle_workspace_close(
            &mut state,
            &mut engine,
            json!(1),
            &json!({ "id": target_id }),
        );

        assert!(res.error.is_none(), "성공해야 한다: {:?}", res.error);
        let result = res.result.expect("success 응답에는 result 가 있다");
        assert_eq!(result["closed"], true);
        assert_eq!(
            result["id"], target_id,
            "응답은 인덱스가 아니라 대상 워크스페이스 id 를 돌려줘야 한다"
        );

        // 원칙 3 — 지정한 대상만 사라진다. 활성 워크스페이스를 닫지 않는다.
        assert_eq!(engine.workspaces.len(), 2);
        assert!(
            engine.workspaces.iter().all(|w| w.id != target_id),
            "대상이 아직 남아 있다"
        );

        // ADR-0113 — 사용자가 보던 워크스페이스가 그대로여야 한다. 앞쪽이 빠지면
        // 뒤가 한 칸 당겨지므로 인덱스는 2 에서 1 로 내려가되 **가리키는 대상은
        // 같아야** 한다.
        assert_eq!(
            engine.workspaces[state.active_workspace].id, viewing_id,
            "앞쪽 워크스페이스를 닫았는데 사용자 시야가 다른 워크스페이스로 옮겨갔다"
        );

        // 원칙 1 — 되돌리기 스택에 쌓지 않는다.
        assert!(
            engine.closed_items.is_empty(),
            "에이전트가 닫은 것이 사용자의 되돌리기 스택에 들어갔다"
        );

        // 원칙 1 — plugin 에도 에이전트 close 로 나가야 한다.
        let events = state.take_pending_lifecycle_events();
        assert!(
            !events.is_empty(),
            "닫힌 surface 의 lifecycle 이벤트가 없다"
        );
        assert!(
            events.iter().all(|e| !e.is_user_close),
            "에이전트가 닫았는데 plugin 에는 사용자 close 로 나간다"
        );
    }

    /// mirror 워크스페이스는 거둘 절차가 따로 있어 이 경로로 닫지 않는다.
    #[test]
    fn closing_a_mirror_workspace_is_refused() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let mirror_id = add_workspace(&mut engine);
        let mirror_idx = engine
            .workspaces
            .iter()
            .position(|w| w.id == mirror_id)
            .expect("방금 만든 워크스페이스");
        engine.workspaces[mirror_idx].mirror = true;

        let res = handle_workspace_close(
            &mut state,
            &mut engine,
            json!(1),
            &json!({ "id": mirror_id }),
        );

        assert!(res.error.is_some(), "mirror 워크스페이스는 거절해야 한다");
        assert_eq!(
            engine.workspaces.len(),
            2,
            "거절이면 아무것도 닫히지 않는다"
        );
    }

    #[test]
    fn closing_an_unknown_workspace_id_is_refused() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        add_workspace(&mut engine);

        let res =
            handle_workspace_close(&mut state, &mut engine, json!(1), &json!({ "id": 999_999 }));

        assert!(res.error.is_some());
        assert_eq!(engine.workspaces.len(), 2);
    }
}

#[cfg(test)]
mod create_cwd_tests {
    use super::*;
    use serde_json::json;

    /// explorer 탭을 열고 그 surface id 와 root 를 돌려준다. 새 탭이 포커스를 받으므로
    /// 마지막에 연 것이 이 창의 포커스 surface 다.
    fn open_explorer(
        state: &mut crate::state::AppState,
        engine: &mut crate::core::CoreState,
        rel: &str,
    ) -> (u32, std::path::PathBuf) {
        let root = crate::test_support::abs_path(rel);
        let (_tab, sid) = state
            .add_kind_tab(
                engine,
                "explorer",
                &json!({ "path": root.to_string_lossy() }),
            )
            .expect("add explorer tab");
        (sid, root)
    }

    /// `surface_id` 를 지목하면 창의 포커스가 아니라 **그 surface** 에서 상속한다(원칙 3).
    /// 포커스를 다른 surface 에 두어, 둘이 같으면 통과하는 구현을 가른다.
    #[test]
    fn a_named_surface_is_the_inherit_source_not_the_focus() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let (named, named_root) = open_explorer(&mut state, &mut engine, "named/proj");
        let (focused, focused_root) = open_explorer(&mut state, &mut engine, "focused/proj");
        assert_eq!(state.focused_surface_id(&engine), Some(focused));
        assert_ne!(named_root, focused_root);

        assert_eq!(
            inherit_cwd_for_create(&state, &engine, Some(named)),
            Some(named_root),
        );
    }

    /// 지목이 없으면 기존 동작 그대로 이 창의 포커스 surface 에서 상속한다.
    #[test]
    fn without_a_named_surface_the_focus_is_the_inherit_source() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let (_named, _) = open_explorer(&mut state, &mut engine, "named/proj");
        let (_focused, focused_root) = open_explorer(&mut state, &mut engine, "focused/proj");

        assert_eq!(
            inherit_cwd_for_create(&state, &engine, None),
            Some(focused_root),
        );
    }

    /// `surface_id` 가 숫자가 아니면 포커스로 떨어지지 않고 거절한다 — 아무것도 안 만든다.
    #[test]
    fn a_malformed_surface_id_is_rejected_not_ignored() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let mut core = crate::ipc::handler::cli_entry_tests::test_core();
        let before = engine.workspaces.len();

        let res = handle_workspace_create(
            &mut core,
            &mut state,
            &mut engine,
            json!(1),
            &json!({ "surface_id": "7" }),
        );

        let err = res.error.expect("숫자가 아닌 surface_id 는 거절해야 한다");
        assert_eq!(err.code, -32602, "{}", err.message);
        assert!(err.message.contains("surface_id"), "{}", err.message);
        assert_eq!(engine.workspaces.len(), before);
    }

    /// 핸들러가 params 에서 읽은 `surface_id` 가 상속 원본으로 **넘어가는지**를 params 로
    /// 잰다. 헬퍼만 부르는 위 시험은 이 배선을 안 본다 — 핸들러가 `None` 을 넘기거나 키
    /// 읽기를 지워도 초록이었다. 포커스를 다른 surface 에 두어 그 결함을 가른다.
    #[test]
    fn the_params_surface_id_reaches_the_inherit_source() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let (named, named_root) = open_explorer(&mut state, &mut engine, "named/proj");
        let (_focused, focused_root) = open_explorer(&mut state, &mut engine, "focused/proj");
        assert_ne!(named_root, focused_root);

        let got = resolve_create_cwd(
            &json!({ "surface_id": named }),
            "terminal",
            &state,
            &engine,
            &json!(1),
        )
        .expect("정상 params");
        assert_eq!(got, Some(named_root));

        let got = resolve_create_cwd(&json!({}), "terminal", &state, &engine, &json!(1))
            .expect("정상 params");
        assert_eq!(got, Some(focused_root));
    }

    /// 명시 `cwd` 는 지목한 surface 보다 앞서고, terminal 이 아닌 kind 는 cwd 를 안 쓴다.
    #[test]
    fn explicit_cwd_wins_and_non_terminal_kinds_take_no_cwd() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let (named, _) = open_explorer(&mut state, &mut engine, "named/proj");
        // 실재하는 디렉토리면 된다 — 핸들러가 명시 `cwd` 의 존재를 검사한다.
        let explicit = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let params = json!({ "surface_id": named, "cwd": explicit.to_string_lossy() });

        let got = resolve_create_cwd(&params, "terminal", &state, &engine, &json!(1))
            .expect("정상 params");
        assert_eq!(got, Some(explicit));

        let got = resolve_create_cwd(
            &json!({ "surface_id": named }),
            "explorer",
            &state,
            &engine,
            &json!(1),
        )
        .expect("정상 params");
        assert_eq!(got, None);
    }

    /// 핸들러를 끝까지 불러 새 터미널을 **실제 PTY 로** 띄우고, 그 셸의 cwd 를 조회해
    /// 핸들러가 계산한 값과 대조한다. 위 시험들은 cwd 를 **무엇으로 계산하는가**만 재서,
    /// 생성 intent 에 `cwd: None` 을 실어도 전부 초록이었다 — 이 시험은 그 값이 생성
    /// payload 에 **실리는가**를 잰다. 명시 `cwd` 와 지목 surface 상속 두 갈래를 다 본다.
    ///
    /// 셸은 `/bin/sh` 로 고정한다 — 사용자 rc 파일이 `cd` 하면 조회값이 흔들린다.
    /// 셸 프로세스의 cwd 를 읽는 수단(`get_cwd_of_pid`)이 linux·macos 에만 있어 그 둘로 한정한다.
    ///
    /// **Windows 에는 이 채널이 없다.** 거기서 `get_cwd_of_pid` 는 항상 `None` 이고
    /// (`crates/tasty-terminal/src/cwd.rs`), `Terminal::get_cwd` 는 OSC 7 캐시 다음에 그것을
    /// 보므로 OSC 7 을 안 내는 셸의 cwd 는 조회되지 않는다. 누락 지점
    /// (`cwd: resolved_cwd,`)은 플랫폼 공통 코드라 linux 채널이 같은 줄을 본다. Windows 에서
    /// 이 사실을 재려면 PTY 가 아니라 `DomainIntent::CreateWorkspace` 를 가로채 그 `cwd`
    /// 필드를 단언하는 수준의 시험이 필요하다.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_resolved_cwd_reaches_the_new_terminals_shell() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let mut core = crate::ipc::handler::cli_entry_tests::test_core();
        engine.settings.general.shell = "/bin/sh".to_string();

        let explicit_dir = tempfile::tempdir().expect("tmp");
        let explicit = explicit_dir.path().canonicalize().expect("canonical");
        let inherit_dir = tempfile::tempdir().expect("tmp");
        let inherited = inherit_dir.path().canonicalize().expect("canonical");
        let (_tab, named) = state
            .add_kind_tab(
                &mut engine,
                "explorer",
                &json!({ "path": inherited.to_string_lossy() }),
            )
            .expect("add explorer tab");

        for (params, want) in [
            (json!({ "cwd": explicit.to_string_lossy() }), &explicit),
            (json!({ "surface_id": named }), &inherited),
        ] {
            let res =
                handle_workspace_create(&mut core, &mut state, &mut engine, json!(1), &params);
            let result = res
                .result
                .unwrap_or_else(|| panic!("{params}: {:?}", res.error));
            let sid = result["surface_id"].as_u64().expect("surface_id") as u32;
            assert_eq!(
                engine.local_surface_cwd(sid).as_ref(),
                Some(want),
                "{params}: 새 터미널의 셸이 계산된 cwd 에서 뜨지 않았다",
            );
        }
    }

    /// 지목한 surface 가 있어도 `inherit_cwd` 를 끈 설정은 그대로 존중한다.
    #[test]
    fn a_named_surface_respects_inherit_cwd_off() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let (named, _) = open_explorer(&mut state, &mut engine, "named/proj");
        engine.settings.general.inherit_cwd = false;

        assert_eq!(inherit_cwd_for_create(&state, &engine, Some(named)), None,);
    }
}
