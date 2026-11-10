//! Freeze legacy workspace inputs after admission; local facts and mirror display have distinct owners.
use super::display::{DisplayAction, DisplayContinuation};
use super::*;
use crate::ipc::handler::{params, workspace as legacy};
use tasty_core::StructuralCommand as C;

pub(super) struct Resolved {
    pub changes: Vec<C>,
    pub response: ResponsePlan,
    pub display: Option<DisplayContinuation>,
}

pub(super) fn resolve(
    request: &JsonRpcRequest,
    session: &EngineSession,
    stream: &str,
) -> Result<Resolved, JsonRpcResponse> {
    match request.method.as_str() {
        "workspace.update" | "intent.workspace-rename" => update(request, session, stream),
        "workspace.move" => reorder(request, session),
        "intent.remote-row" => Ok(remote_row(request, session)),
        _ => unreachable!("workspace command family"),
    }
}

fn update(
    request: &JsonRpcRequest,
    session: &EngineSession,
    stream: &str,
) -> Result<Resolved, JsonRpcResponse> {
    let core = &session.core_state;
    let input = &request.params;
    let id = serde_json::Value::Null;
    let bad = |message| JsonRpcResponse::invalid_params(id.clone(), message);
    let named = params::optional_u32(input, "id", &id)?;
    let workspace_id = if let Some(named) = named {
        named
    } else if let Some(index) = params::opt_int::<u64>(input, "index", &id)? {
        core.workspace_at(index as usize)
            .ok_or_else(|| {
                bad(format!(
                    "Workspace index {index} out of range (0..{})",
                    core.workspaces().len()
                ))
            })?
            .id
    } else {
        return Err(bad("Missing required 'id' or 'index' parameter".into()));
    };
    // Validate auxiliary inputs before resolving the mutation, just like the public handler.
    let category = legacy::resolve_category_param(core, input).map_err(&bad)?;
    let index = core
        .find_workspace_index_for_id(workspace_id)
        .ok_or_else(|| bad(format!("Workspace id {workspace_id} not found")))?;
    let workspace = core.workspace_at(index).expect("resolved workspace");
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let subtitle = input
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let description = input
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    if workspace.mirror {
        // 저장된 원격 행은 이름·설명·카테고리를 journal에도 남긴다. 화면 값은 아래 continuation이 바꾼다.
        let changes = saved_row(core, &display_order(core), workspace)
            .map(|row| C::SetRemoteRow {
                id: workspace_id,
                row: tasty_core::RemoteRow {
                    name: name.clone().unwrap_or_else(|| row.name.clone()),
                    subtitle: subtitle.clone().unwrap_or_else(|| row.subtitle.clone()),
                    description: description
                        .clone()
                        .unwrap_or_else(|| row.description.clone()),
                    category: category.unwrap_or(row.category),
                    ..row
                },
            })
            .into_iter()
            .collect();
        let token = core
            .mirror_projection_token(workspace_id)
            .expect("mirror projection has a lifetime");
        let response = ResponsePlan::Fixed(JsonRpcResponse::success(
            id,
            serde_json::json!({
                "id": workspace_id, "index": index,
                "name": name.as_ref().unwrap_or(&workspace.name),
                "subtitle": subtitle.as_ref().unwrap_or(&workspace.subtitle),
                "description": description.as_ref().unwrap_or(&workspace.description),
                "category": category.unwrap_or(workspace.category),
            }),
        ));
        return Ok(Resolved {
            changes,
            response,
            display: Some(DisplayContinuation {
                engine: session.id,
                mirrors: vec![(workspace_id, token)],
                action: DisplayAction::Workspace {
                    id: workspace_id,
                    user_direct: request.method == "intent.workspace-rename"
                        && input["user_direct"].as_bool().unwrap_or(false),
                    name,
                    subtitle,
                    description,
                    category,
                },
            }),
        });
    }
    let display = Some(DisplayContinuation {
        engine: session.id,
        mirrors: Vec::new(),
        action: DisplayAction::Workspace {
            id: workspace_id,
            user_direct: request.method == "intent.workspace-rename"
                && input["user_direct"].as_bool().unwrap_or(false),
            name: name.clone(),
            subtitle: subtitle.clone(),
            description: description.clone(),
            category: None,
        },
    });
    let mut changes = vec![C::UpdateWorkspaceMeta {
        workspace_id,
        name,
        subtitle,
        description,
    }];
    if let Some(category) = category {
        changes.push(C::SetWorkspaceCategory {
            workspace_id,
            category,
        });
    }
    Ok(Resolved {
        changes,
        response: ResponsePlan::WorkspaceUpdated {
            stream: stream.into(),
            id: workspace_id,
            display_index: index,
        },
        display,
    })
}

fn reorder(request: &JsonRpcRequest, session: &EngineSession) -> Result<Resolved, JsonRpcResponse> {
    let core = &session.core_state;
    let input = &request.params;
    let id = serde_json::Value::Null;
    let bad = |message| JsonRpcResponse::invalid_params(id.clone(), message);
    let named = params::opt_int::<u64>(input, "id", &id)?;
    let from = params::opt_int::<u64>(input, "from_index", &id)?;
    let from = match (named, from) {
        (Some(_), Some(_)) => {
            return Err(bad(
                "give either 'id' (the workspace to move) or 'from_index', not both".into(),
            ));
        }
        (Some(named), None) => core
            .find_workspace_index_for_id(named as u32)
            .ok_or_else(|| bad(format!("no workspace {named}")))?,
        (None, Some(from)) => from as usize,
        (None, None) => return Err(bad("Missing 'id' or 'from_index' parameter".into())),
    };
    let to = params::opt_int::<u64>(input, "to_index", &id)?
        .ok_or_else(|| bad("Missing 'to_index' parameter".into()))? as usize;
    let mut order: Vec<_> = core
        .workspaces()
        .iter()
        .map(|workspace| workspace.id)
        .collect();
    let moved = from != to && from < order.len() && to < order.len();
    let mut changes = Vec::new();
    let display = if moved {
        let source = order.remove(from);
        order.insert(to, source);
        if core
            .local_workspaces()
            .iter()
            .any(|workspace| workspace.id == source)
        {
            let to_index = order
                .iter()
                .filter(|id| {
                    core.local_workspaces()
                        .iter()
                        .any(|workspace| workspace.id == **id)
                })
                .position(|id| *id == source)
                .expect("source survives");
            changes.push(C::MoveWorkspace {
                workspace_id: source,
                to_index,
            });
        }
        // 저장된 원격 행은 새 화면 순서의 자리를 저장한다. 바뀌지 않은 행은 core가 이벤트를 만들지 않는다.
        changes.extend(
            core.mirror_workspaces()
                .iter()
                .filter_map(|workspace| remote_row_command(core, &order, workspace)),
        );
        Some(DisplayContinuation {
            engine: session.id,
            mirrors: core
                .mirror_workspaces()
                .iter()
                .map(|workspace| {
                    (
                        workspace.id,
                        core.mirror_projection_token(workspace.id)
                            .expect("mirror lifetime"),
                    )
                })
                .collect(),
            action: DisplayAction::Order(order),
        })
    } else {
        None
    };
    Ok(Resolved {
        changes,
        response: ResponsePlan::Fixed(JsonRpcResponse::success(
            id,
            serde_json::json!({"moved":moved}),
        )),
        display,
    })
}

/// 저장된 원격 행의 현재 값을 저장 명령으로 만든다. 자리는 합친 화면 순서 `order`에서 읽는다.
pub(super) fn remote_row_command(
    core: &crate::core::CoreState,
    order: &[u32],
    workspace: &crate::model::Workspace,
) -> Option<C> {
    Some(C::SetRemoteRow {
        id: workspace.id,
        row: saved_row(core, order, workspace)?,
    })
}

fn display_order(core: &crate::core::CoreState) -> Vec<u32> {
    core.workspaces()
        .iter()
        .map(|workspace| workspace.id)
        .collect()
}

/// 화면 순서에서 행의 자리: 바로 앞 로컬 workspace와, 앞에 있는 저장 행의 수.
fn row_place(core: &crate::core::CoreState, order: &[u32], id: u32) -> (Option<u32>, usize) {
    let before = &order[..order.iter().position(|w| *w == id).unwrap_or(order.len())];
    let local = |w: &u32| core.local_workspaces().iter().any(|l| l.id == *w);
    let saved = |w: &u32| {
        core.mirror_workspaces()
            .iter()
            .any(|m| m.id == *w && m.remote_row.is_some())
    };
    let after = before.iter().rev().find(|w| local(w)).copied();
    (after, before.iter().filter(|w| saved(w)).count())
}

fn saved_row(
    core: &crate::core::CoreState,
    order: &[u32],
    workspace: &crate::model::Workspace,
) -> Option<tasty_core::RemoteRow> {
    let (after, rank) = row_place(core, order, workspace.id);
    Some(tasty_core::RemoteRow {
        name: workspace.name.clone(),
        subtitle: workspace.subtitle.clone(),
        description: workspace.description.clone(),
        category: workspace.category,
        after,
        rank,
        binding: workspace.remote_row.clone()?,
    })
}

fn remote_row(request: &JsonRpcRequest, session: &EngineSession) -> Resolved {
    let core = &session.core_state;
    let id = request.params["id"].as_u64().unwrap_or(u64::MAX) as u32;
    let changes = if request.params["remove"].as_bool().unwrap_or(false) {
        vec![C::RemoveRemoteRow { id }]
    } else {
        core.find_workspace_index_for_id(id)
            .and_then(|index| {
                remote_row_command(core, &display_order(core), core.workspace_at(index)?)
            })
            .into_iter()
            .collect()
    };
    Resolved {
        changes,
        response: ResponsePlan::Fixed(JsonRpcResponse::success(
            serde_json::Value::Null,
            serde_json::json!({"id": id}),
        )),
        display: None,
    }
}
