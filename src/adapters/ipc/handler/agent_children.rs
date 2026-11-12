//! `attach.agent_*`·`attach.child_*`: plugin 이 attach holder 연결로 원격 자식 에이전트를 다루는 host 호출.
//! 상태와 전송은 [`crate::remote::agent_children`] 가 맡고, 여기서는 호출자·대상을 확인한다.

#[cfg(feature = "gui")]
use std::time::Instant;

use serde_json::{Value, json};
use tasty_ipc::protocol::JsonRpcResponse;

#[cfg(feature = "gui")]
use crate::core::child_parent::ParentKey;
use crate::remote::agent_children as agents;
#[cfg(feature = "gui")]
use crate::remote::agent_children::client::{CALL_REPLY_LIMIT, Call, CallState, Outgoing};
use crate::runtime::engine_access::EngineMut;

pub(crate) fn handle_agent_method(
    engine: &mut EngineMut<'_>,
    caller: &crate::ipc::caller::CallerContext,
    id: Value,
    method: &str,
    params: &Value,
) -> JsonRpcResponse {
    let result = match method {
        #[cfg(feature = "gui")]
        "attach.agent_target" => target(engine, params),
        #[cfg(feature = "gui")]
        "attach.agent_commit" => commit(caller, params),
        // 헤드리스에는 attach client 가 없어 원격 mirror 도 없다.
        #[cfg(not(feature = "gui"))]
        "attach.agent_target" => Ok(json!({ "remote": false })),
        #[cfg(not(feature = "gui"))]
        "attach.agent_commit" => Err(NO_MIRRORS.into()),
        "attach.child_notice" => notice(caller, params),
        "attach.child_notices" => Ok(notices()),
        _ => Err(format!("unknown method {method}")),
    };
    match result {
        Ok(value) => JsonRpcResponse::success(id, value),
        Err(message) => JsonRpcResponse::invalid_params(id, message),
    }
}

#[cfg(not(feature = "gui"))]
const NO_MIRRORS: &str = "this instance has no remote attach mirrors";

/// `attach.agent_call`. 응답은 원격 결과가 올 때까지 미룬다.
pub(crate) fn handle_call(
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            unused_variables,
            reason = "only the attach client answers through deferred replies"
        )
    )]
    core: &crate::app::services::AppServices,
    #[cfg_attr(
        not(feature = "gui"),
        expect(unused_variables, reason = "a headless instance has no mirrors")
    )]
    engine: &mut EngineMut<'_>,
    #[cfg_attr(
        not(feature = "gui"),
        expect(unused_variables, reason = "a headless instance has no mirrors")
    )]
    caller: &crate::ipc::caller::CallerContext,
    id: Value,
    #[cfg_attr(
        not(feature = "gui"),
        expect(unused_variables, reason = "a headless instance has no mirrors")
    )]
    params: &Value,
) -> JsonRpcResponse {
    #[cfg(feature = "gui")]
    return call(core, engine, caller, id, params);
    #[cfg(not(feature = "gui"))]
    JsonRpcResponse::invalid_params(id, NO_MIRRORS)
}

#[cfg(feature = "gui")]
fn opt_u32(params: &Value, key: &str) -> Option<u32> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
}

#[cfg(feature = "gui")]
fn plugin_of(caller: &crate::ipc::caller::CallerContext) -> Result<&str, String> {
    match caller {
        crate::ipc::caller::CallerContext::Plugin { plugin_id, .. } => Ok(plugin_id),
        _ => Err("only plugins can use remote attach agent calls".into()),
    }
}

#[cfg(feature = "gui")]
/// 호출 plugin 의 유일한 IPC namespace. 다른 namespace 를 원격 실행하지 못하게 한다.
fn namespace_of(plugin: &str) -> Result<String, String> {
    let table = tasty_utils::plugin_namespaces::snapshot().unwrap_or_default();
    let mut owned = table
        .iter()
        .filter(|(_, owner)| owner.plugin_id == plugin)
        .map(|(prefix, _)| prefix.clone());
    match (owned.next(), owned.next()) {
        (Some(prefix), None) => Ok(prefix),
        _ => Err(format!("{plugin} must own exactly one IPC namespace")),
    }
}

#[cfg(feature = "gui")]
/// mirror workspace 와 그 안의 기준 surface. surface·pane·workspace 순으로 찾는다.
fn mirror_anchor(engine: &EngineMut<'_>, params: &Value) -> Result<Option<(u32, u32)>, String> {
    let core = &engine.core;
    let pane = opt_u32(params, "pane")
        .or_else(|| opt_u32(params, "surface_id").and_then(|s| core.find_pane_for_surface(s)));
    let workspace = match pane {
        Some(pane) => core
            .find_workspace_index_for_pane(pane)
            .and_then(|i| core.workspace_at(i)),
        None => {
            let ws = opt_u32(params, "workspace").ok_or("missing workspace, pane or surface_id")?;
            core.find_workspace_index_for_id(ws)
                .and_then(|i| core.workspace_at(i))
        }
    }
    .ok_or("target not found")?;
    if !workspace.mirror {
        return Ok(None);
    }
    let anchor = match opt_u32(params, "surface_id") {
        Some(surface) => Some(surface),
        None => {
            let pane = pane.or_else(|| workspace.pane_layout().all_pane_ids().into_iter().next());
            pane.and_then(|p| workspace.pane_layout().find_pane(p))
                .and_then(|p| p.tabs.first())
                .and_then(|t| t.first_surface_id())
        }
    }
    .ok_or("the mirror target has no surface")?;
    Ok(Some((workspace.id, anchor)))
}

#[cfg(feature = "gui")]
fn target(engine: &EngineMut<'_>, params: &Value) -> Result<Value, String> {
    let Some((workspace, anchor)) = mirror_anchor(engine, params)? else {
        return Ok(json!({ "remote": false }));
    };
    let client = agents::client();
    let capable = client.sessions.get(&workspace).is_some_and(|s| s.capable);
    let child = opt_u32(params, "surface_id").is_some_and(|s| client.children.contains_key(&s));
    Ok(json!({
        "remote": true,
        "capable": capable,
        "workspace": workspace,
        "anchor_surface_id": anchor,
        "child": child,
    }))
}

#[cfg(feature = "gui")]
fn parse_parent(params: &Value) -> Result<Option<ParentKey>, String> {
    let Some(parent) = params.get("parent").filter(|p| !p.is_null()) else {
        return Ok(None);
    };
    if let Some(surface) = opt_u32(parent, "surface") {
        return Ok(Some(ParentKey::Surface(surface)));
    }
    let external = parent
        .get("external")
        .and_then(Value::as_str)
        .ok_or("parent must be {surface} or {external}")?;
    crate::core::child_parent::validate_external_parent(external)?;
    if tasty_ipc::agent_child::is_attach_parent(external) {
        return Err("a remote attach parent key cannot be used as a client parent".into());
    }
    Ok(Some(ParentKey::External(external.to_owned())))
}

#[cfg(feature = "gui")]
fn call(
    core: &crate::app::services::AppServices,
    engine: &mut EngineMut<'_>,
    caller: &crate::ipc::caller::CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let prepared = (|| -> Result<u64, String> {
        let plugin = plugin_of(caller)?;
        let namespace = namespace_of(plugin)?;
        let method = params
            .get("method")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty() && !m.contains('.'))
            .ok_or("missing 'method' (a name without the namespace prefix)")?
            .to_owned();
        let anchor = opt_u32(params, "anchor_surface_id").ok_or("missing 'anchor_surface_id'")?;
        let Some((workspace, anchor)) = mirror_anchor(engine, &json!({ "surface_id": anchor }))?
        else {
            return Err(format!("surface {anchor} is not in a remote attach mirror"));
        };
        let mut client = agents::client();
        if !client.sessions.get(&workspace).is_some_and(|s| s.capable) {
            return Err("the remote instance does not support remote agent children".into());
        }
        let target = match opt_u32(params, "target_surface_id") {
            Some(local) => {
                let child = client
                    .children
                    .get(&local)
                    .filter(|c| {
                        c.workspace == workspace && c.plugin == plugin && c.gone_at.is_none()
                    })
                    .ok_or_else(|| format!("surface {local} is not a remote child of {plugin}"))?;
                Some(child.remote)
            }
            None => None,
        };
        let parent = parse_parent(params)?;
        let call_id = client.next_call_id();
        let str_field = |k: &str| params.get(k).and_then(Value::as_str).map(str::to_owned);
        client.calls.insert(
            call_id,
            Call {
                workspace,
                plugin: plugin.to_owned(),
                namespace: namespace.clone(),
                parent,
                role: str_field("role"),
                nickname: str_field("nickname"),
                started: Instant::now(),
                sent: false,
                state: CallState::Waiting,
                outcome: None,
                spawned: Vec::new(),
                adopted: Vec::new(),
                unregistered: Vec::new(),
                replied: false,
                committed: false,
                late: false,
                late_sent: false,
                late_retry_at: None,
                wire: None,
            },
        );
        client.outgoing.push((
            workspace,
            Outgoing::Call {
                call_id,
                namespace,
                method,
                anchor,
                target,
                params: params.get("params").cloned().unwrap_or(Value::Null),
                inherit: engine.runtime.settings.general.inherit_cwd,
                source: None,
            },
        ));
        Ok(call_id)
    })();
    let call_id = match prepared {
        Ok(call_id) => call_id,
        Err(message) => return JsonRpcResponse::invalid_params(id, message),
    };
    (engine.runtime.waker)();
    let reply_id = id.clone();
    core.reply_with_file(
        id,
        move || agents::call_reply(call_id, reply_id.clone(), Instant::now(), CALL_REPLY_LIMIT),
        |_, response| response,
    )
}

#[cfg(feature = "gui")]
fn commit(caller: &crate::ipc::caller::CallerContext, params: &Value) -> Result<Value, String> {
    let plugin = plugin_of(caller)?;
    let call_id = params
        .get("call_id")
        .and_then(Value::as_u64)
        .ok_or("missing 'call_id'")?;
    let remote = agents::client().commit(call_id, plugin)?;
    Ok(json!({ "committed": remote.len() }))
}

fn notice(caller: &crate::ipc::caller::CallerContext, params: &Value) -> Result<Value, String> {
    use crate::remote::agent_children::PluginNoticeStatus as S;
    Ok(match agents::plugin_notice(caller, params)? {
        S::Pending => json!({ "status": "pending" }),
        S::Delivered => json!({ "status": "delivered" }),
        S::Rejected(reason) => json!({ "status": "rejected", "reason": reason }),
    })
}

fn notices() -> Value {
    let out = agents::pending_summary();
    #[cfg(feature = "gui")]
    let out = {
        let mut out = out;
        out["client"] = agents::client_summary();
        out
    };
    out
}
