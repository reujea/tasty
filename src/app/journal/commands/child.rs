//! Child terminal creation fixes its relation before entering the ordinary resource lifecycle.
use super::*;

pub(super) struct Spawn {
    request: create::Request,
    receipt: crate::runtime::child_terminal::Ticket,
}
impl Spawn {
    pub(super) fn weight(&self) -> usize {
        self.request.weight()
    }
}

pub(super) fn resolve_spawn(
    request: &JsonRpcRequest,
    session: &mut EngineSession,
    view: &crate::runtime::journal_product::CompletionView,
    pending: &mut Option<Spawn>,
    caller_plugin: Option<&str>,
) -> Result<Option<create::Request>, JsonRpcResponse> {
    if let Some(spawn) = pending.as_ref() {
        let Some(result) = spawn.receipt.poll() else {
            return Ok(None);
        };
        let index = result
            .map_err(|reason| JsonRpcResponse::internal_error(serde_json::Value::Null, reason))?
            .ok_or_else(|| {
                JsonRpcResponse::internal_error(
                    serde_json::Value::Null,
                    "child reservation returned no index",
                )
            })?;
        // 원격 holder 문맥의 spawn 이면 이 번호를 그 call 에 귀속한다. 같은 출처의 call 이 겹쳐도 섞이지 않는다.
        if let Some(context) = request
            .params
            .get("attach_context")
            .and_then(|v| v.as_str())
            && request
                .params
                .get("parent_external")
                .and_then(|v| v.as_str())
                .is_some_and(tasty_ipc::agent_child::is_attach_parent)
        {
            crate::remote::agent_children::record_spawn_slot(context, index);
        }
        let mut spawn = pending.take().unwrap();
        spawn
            .request
            .input
            .as_mut()
            .expect("retained child input")
            .child
            .as_mut()
            .expect("retained child recipe")
            .index = index;
        spawn.request.plan.tab_name = format!("child{index}");
        spawn.request.plan.explicit_name = Some(spawn.request.plan.tab_name.clone());
        if let create::Shape::Child { index: stored, .. } = &mut spawn.request.shape {
            *stored = index;
        }
        return Ok(Some(spawn.request));
    }
    use crate::ipc::handler::params;
    let id = serde_json::Value::Null;
    let bad = |reason: String| JsonRpcResponse::invalid_params(id.clone(), reason);
    let input = &request.params;
    let parent = spawn_parent(input, &id)?;
    let workspace = input
        .get("workspace")
        .and_then(|value| value.as_str())
        .ok_or_else(|| bad("missing workspace".into()))?;
    let core = &session.core_state;
    let workspace = workspace
        .parse::<u32>()
        .ok()
        .and_then(|id| {
            core.workspaces()
                .into_iter()
                .find(|workspace| workspace.id == id)
        })
        .or_else(|| {
            core.workspaces()
                .into_iter()
                .find(|candidate| candidate.name == workspace)
        })
        .ok_or_else(|| {
            bad(format!(
                "workspace '{workspace}' not found — pass its numeric workspace id"
            ))
        })?;
    let pane = params::optional_u32(input, "pane", &id)?
        .or_else(|| workspace.pane_layout().all_pane_ids().into_iter().next())
        .ok_or_else(|| bad("workspace has no panes".into()))?;
    let pane_value = core
        .find_pane_by_id(pane)
        .ok_or_else(|| bad(format!("Pane {pane} not found")))?;
    let target = core
        .find_workspace_index_for_pane(pane)
        .and_then(|index| core.workspace_at(index))
        .ok_or_else(|| bad("pane workspace disappeared".into()))?;
    // 원격 holder 출처의 부모 키는 그 holder 의 call 을 처리하는 plugin 만 쓸 수 있다. 통과하면 그 holder 가
    // 점유한 workspace 의 hard 점유 거절만 건너뛴다. mirror 거절은 그대로다.
    let attach_parent = match &parent {
        crate::core::child_parent::ParentKey::External(key)
            if tasty_ipc::agent_child::is_attach_parent(key) =>
        {
            Some(key.clone())
        }
        _ => None,
    };
    match &attach_parent {
        Some(key) if !target.mirror => crate::remote::agent_children::spawn_allowed(
            &session.live.occupancy,
            caller_plugin,
            key,
            input.get("attach_context").and_then(|v| v.as_str()),
            target.id,
        )
        .map_err(bad)?,
        _ => {
            if let Some(response) =
                crate::ipc::handler::spawn_target_guard(&session.as_ref(), pane, &id)
            {
                return Err(response);
            }
        }
    }
    let workspace = target.id;
    let tab_index = pane_value.tabs.len();
    let cwd = optional_str(input, "cwd");
    // 원격 holder 의 경로는 그 client 에서 바꾸지 않고 오므로, 상대 경로를 이 프로세스 cwd 로 풀지 않는다.
    if attach_parent.is_some()
        && let Some(path) = &cwd
        && !std::path::Path::new(path).is_absolute()
    {
        return Err(bad(format!(
            "cwd of a remote attach child must be an absolute path on this machine: {path}"
        )));
    }
    if let Some(path) = &cwd
        && !std::path::Path::new(path).is_dir()
    {
        return Err(bad(format!("cwd does not exist: {path}")));
    }
    // 원격 holder 의 자식은 client 가 고른 원본(host 가 확인해 넣은 예약 인자)을 잇는다. 없을 때만 이 서버의
    // View 선택과 설정을 쓴다(fallback). 로컬 호출자의 같은 이름 인자는 쓰지 않는다.
    let source = match &attach_parent {
        Some(_) => remote_cwd_source(input, pane_value, &id)?,
        None => super::create_spec::CwdSource::View,
    };
    let launch_cwd = launch_cwd(&session.as_ref(), view, pane, cwd.as_deref(), source);
    let mut result =
        create::Request::base(session, "terminal", &serde_json::json!({}), launch_cwd)?;
    let receipt = session
        .runtime
        .child_terminals
        .reserve(&parent)
        .map_err(|reason| JsonRpcResponse::internal_error(id.clone(), reason))?;
    let index = 0;
    let command = optional_str(input, "command");
    let recipe = crate::runtime::journal_product::ChildRecipe {
        parent,
        index,
        workspace,
        runtime_epoch: result.binding.runtime_epoch,
        cwd,
        role: optional_str(input, "role"),
        nickname: optional_str(input, "nickname"),
        has_command: command.is_some(),
        replacing: false,
    };
    result
        .input
        .as_mut()
        .ok_or_else(|| bad("child preparation input missing".into()))?
        .child = Some(recipe);
    result.plan.destination = tasty_core::CreationDestination::Tab {
        pane,
        tab: 0,
        index: tab_index,
    };
    // This private plan receives its child name only after the reservation receipt.
    result.shape = create::Shape::Child { index, workspace };
    result.activate = false;
    result.one_shot_input = command.map(|body| crate::app::terminal_input::tell_payload(&body));
    *pending = Some(Spawn {
        request: result,
        receipt,
    });
    Ok(None)
}

/// 숫자 `parent`(별칭 `surface`) 또는 `parent_external` 중 하나. 숫자 부모의 해석은 이전과 같다.
fn spawn_parent(
    input: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<crate::core::child_parent::ParentKey, JsonRpcResponse> {
    use crate::core::child_parent::ParentKey;
    use crate::ipc::handler::params;
    match params::optional_external_parent(input, id)? {
        Some(external) => {
            for key in ["parent", "surface"] {
                if input.get(key).is_some_and(|value| !value.is_null()) {
                    return Err(params::both_parents(key, id));
                }
            }
            Ok(ParentKey::External(external))
        }
        None => Ok(ParentKey::Surface(
            params::require_u32(input, "parent", id)
                .or_else(|_| params::require_u32(input, "surface", id))?,
        )),
    }
}

fn optional_str(input: &serde_json::Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

pub(super) fn resolve_respawn(
    request: &JsonRpcRequest,
    session: &EngineSession,
) -> Result<create::Request, JsonRpcResponse> {
    let id = serde_json::Value::Null;
    let input = &request.params;
    if input
        .get("parent_external")
        .and_then(|v| v.as_str())
        .is_some_and(tasty_ipc::agent_child::is_attach_parent)
    {
        return Err(JsonRpcResponse::invalid_params(
            id,
            "remote attach children cannot be respawned on the remote instance",
        ));
    }
    let parent = crate::ipc::handler::params::optional_parent(input, "surface", &id)?
        .or_else(|| {
            session
                .runtime
                .child_terminals
                .single_parent(&session.core_state.live_surface_ids())
                .map(crate::core::child_parent::ParentKey::Surface)
        })
        .ok_or_else(|| {
            JsonRpcResponse::invalid_params(
                id.clone(),
                "missing 'surface' parameter (0 or >1 parents — specify --surface)",
            )
        })?;
    let index = crate::ipc::handler::params::require_u32(input, "child", &id)?;
    let entry = session
        .runtime
        .child_terminals
        .find_child(&parent, index)
        .ok_or_else(|| {
            let under = match &parent {
                crate::core::child_parent::ParentKey::Surface(parent) => {
                    format!("surface {parent}")
                }
                crate::core::child_parent::ParentKey::External(parent) => {
                    format!("parent_external {parent}")
                }
            };
            JsonRpcResponse::invalid_params(
                id.clone(),
                format!("child {index} not found under {under}"),
            )
        })?;
    let cwd = optional_str(input, "cwd").ok_or_else(|| {
        JsonRpcResponse::invalid_params(id.clone(), "structural child respawn requires cwd")
    })?;
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("cwd does not exist: {cwd}"),
        ));
    }
    let spec = super::create_spec::Spec {
        destination: super::create_spec::Destination::Convert {
            surface: entry.child_surface_id,
            respawn: true,
        },
        kind: "terminal".into(),
        cwd: Some(cwd.clone().into()),
        params: serde_json::json!({}),
    };
    let mut result = create::Request::from_spec(spec, session)?;
    let workspace = session
        .core_state
        .find_workspace_index_for_surface(entry.child_surface_id)
        .and_then(|(index, _)| session.core_state.workspace_at(index))
        .ok_or_else(|| JsonRpcResponse::invalid_params(id, "child surface disappeared"))?
        .id;
    let command = optional_str(input, "command");
    result.input.as_mut().expect("fresh preparation").child =
        Some(crate::runtime::journal_product::ChildRecipe {
            parent,
            index,
            workspace,
            runtime_epoch: result.binding.runtime_epoch,
            cwd: Some(cwd),
            role: optional_str(input, "role").or_else(|| entry.role.clone()),
            nickname: optional_str(input, "nickname").or_else(|| entry.nickname.clone()),
            has_command: command.is_some(),
            replacing: true,
        });
    result.shape = create::Shape::ChildRespawn { index };
    result.one_shot_input = command;
    Ok(result)
}

/// 자식 터미널의 시작 폴더. 명시 cwd, 원격 holder 가 고른 원본, 이 서버 View 의 선택 순서로 정한다.
/// 이 서버의 `inherit_cwd` 설정은 View 선택에만 적용한다. 원본을 고른 요청자는 이미 자기 설정으로 판단했다.
pub(super) fn launch_cwd(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    view: &crate::runtime::journal_product::CompletionView,
    pane: u32,
    explicit: Option<&str>,
    source: super::create_spec::CwdSource,
) -> Option<std::path::PathBuf> {
    use super::create_spec::CwdSource;
    if let Some(path) = explicit {
        return Some(std::path::PathBuf::from(path));
    }
    let surface = match source {
        CwdSource::Surface(surface) => surface,
        CwdSource::NoInherit => return None,
        CwdSource::View if engine.runtime.settings.general.inherit_cwd => {
            super::create_spec::selected_in_view(engine.core, view, pane)?
        }
        CwdSource::View => return None,
    };
    engine.local_surface_cwd(surface)
}

/// 원격 holder 자식의 cwd 상속 원본. 원본 surface 는 자식을 만들 pane 에 있어야 한다. 서버 연결 처리가 이미
/// 확인했지만 plugin 을 거쳐 온 값이라 여기서 다시 확인한다. 모르는 값은 거절한다.
pub(super) fn remote_cwd_source(
    input: &serde_json::Value,
    pane: &crate::model::Pane,
    id: &serde_json::Value,
) -> Result<super::create_spec::CwdSource, JsonRpcResponse> {
    use super::create_spec::CwdSource;
    use tasty_ipc::stream::InheritCwd;
    let Some(value) = input
        .get(tasty_ipc::agent_child::INHERIT_CWD_PARAM)
        .filter(|value| !value.is_null())
    else {
        return Ok(CwdSource::View);
    };
    let bad = |message: String| JsonRpcResponse::invalid_params(id.clone(), message);
    match serde_json::from_value::<InheritCwd>(value.clone()) {
        Ok(InheritCwd::NoInherit) => Ok(CwdSource::NoInherit),
        Ok(InheritCwd::Surface(surface))
            if pane.tabs.iter().any(|tab| tab.contains_surface(surface)) =>
        {
            Ok(CwdSource::Surface(surface))
        }
        Ok(InheritCwd::Surface(surface)) => Err(bad(format!(
            "inherit_cwd surface {surface} is not in pane {}",
            pane.id
        ))),
        Ok(InheritCwd::Unknown(_)) | Err(_) => {
            Err(bad(format!("unsupported inherit_cwd value {value}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::spawn_parent;
    use crate::core::child_parent::ParentKey;
    use serde_json::json;

    fn message(input: serde_json::Value) -> String {
        spawn_parent(&input, &json!(null))
            .expect_err("refused")
            .error
            .expect("error")
            .message
    }

    #[test]
    fn a_spawn_takes_one_numeric_or_one_external_parent() {
        let id = json!(null);
        assert_eq!(
            spawn_parent(&json!({ "parent": 7, "workspace": "w" }), &id).ok(),
            Some(ParentKey::Surface(7))
        );
        assert_eq!(
            spawn_parent(&json!({ "surface": 8 }), &id).ok(),
            Some(ParentKey::Surface(8))
        );
        assert_eq!(
            spawn_parent(&json!({ "parent_external": "codex:t" }), &id).ok(),
            Some(ParentKey::External("codex:t".into()))
        );
        assert_eq!(
            message(json!({ "parent": 7, "parent_external": "codex:t" })),
            "pass either 'parent' or 'parent_external', not both"
        );
        assert_eq!(
            message(json!({ "surface": 7, "parent_external": "codex:t" })),
            "pass either 'surface' or 'parent_external', not both"
        );
        assert_eq!(message(json!({})), "missing 'surface'");
        assert!(message(json!({ "parent_external": "a b" })).contains("whitespace"));
        assert!(message(json!({ "parent_external": 3 })).contains("must be a string"));
    }

    /// 숫자 부모로 기록된 이전 journal 레코드도 그대로 읽힌다.
    #[test]
    fn a_child_recipe_with_a_numeric_parent_still_reads() {
        let recipe: crate::runtime::journal_product::ChildRecipe = serde_json::from_value(json!({
            "parent": 513, "index": 2, "workspace": 1, "runtime_epoch": 4,
            "cwd": null, "role": "worker", "nickname": null, "has_command": true
        }))
        .expect("legacy recipe");
        assert_eq!(recipe.parent, ParentKey::Surface(513));
        assert!(!recipe.replacing);
        let external = crate::runtime::journal_product::ChildRecipe {
            parent: ParentKey::External("codex:t".into()),
            ..recipe
        };
        let value = serde_json::to_value(&external).unwrap();
        assert_eq!(value["parent"], json!("codex:t"));
        let back: crate::runtime::journal_product::ChildRecipe =
            serde_json::from_value(value).unwrap();
        assert_eq!(back.parent, external.parent);
    }
}
