//! 자식 에이전트의 세션 토큰을 발급·조회·무효화한다.
//! 자식은 TASTY_SESSION_TOKEN을 IPC에 보내고 호스트가 SessionStore로 검증한다.
//! issue/revoke에는 AgentManage 권한이 필요하다. Agent 호출자의 발급은 자기 권한 범위로 제한하며
//! Local과 Plugin은 제한하지 않는다. revoke는 부모 관계를 따로 검사하지 않는다. list는 Local 전용이다.

use super::params::{self, p_try};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::plugin::manifest::Permission;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::session::{SessionError, SessionStore};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn session_err_to_response(id: Value, err: SessionError) -> JsonRpcResponse {
    match err {
        SessionError::InvalidArgument(_) => JsonRpcResponse::invalid_params(id, err.to_string()),
        SessionError::Memory(_) | SessionError::Serde(_) => {
            JsonRpcResponse::error(id, -32603, err.to_string())
        }
    }
}

/// Agent 호출자는 가진 권한만 자식에게 넘길 수 있다. Local과 설치된 플러그인은 권한으로 막지
/// 않으므로 어떤 토큰이든 넘길 수 있다(docs/dev-guide/plugin-permissions.md).
fn caller_may_grant(caller: &CallerContext, p: &Permission) -> bool {
    caller.permissions().is_none_or(|own| own.contains(p))
}

/// `session.issue` — 자식 agent 에게 새 SessionToken 발급.
///
/// params:
/// - `agent_id` (str, 필수) — 자식의 호스트-부여 식별자. 비어 있으면 거부.
/// - `permissions` (array<str>, 옵션) — 자식에게 부여할 권한 토큰. caller 의
///   권한 셋에 포함되지 않은 토큰이 있으면 거부 (escalation 방지). Local·Plugin
///   caller 는 모든 토큰 허용.
/// - `ttl_ms` (u64, 옵션) — 토큰 수명. 없으면 자식 프로세스 종료/`session.revoke`
///   까지 유효.
///
/// 응답: `{ token, agent_id, expires_at_ms? }`.
pub fn handle_issue(
    core: &crate::app::services::AppServices,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_id = match params.get("agent_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return JsonRpcResponse::invalid_params(id, "Missing or empty 'agent_id'");
        }
    };
    let perm_tokens: Vec<String> = match params.get("permissions") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                match v.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => {
                        return JsonRpcResponse::invalid_params(
                            id,
                            "'permissions' must be an array of strings",
                        );
                    }
                }
            }
            out
        }
        Some(Value::Null) | None => Vec::new(),
        _ => {
            return JsonRpcResponse::invalid_params(
                id,
                "'permissions' must be an array of strings",
            );
        }
    };
    let ttl_ms = p_try!(params::opt_int::<u64>(params, "ttl_ms", &id));

    let mut perms: Vec<Permission> = Vec::with_capacity(perm_tokens.len());
    for t in &perm_tokens {
        match Permission::from_token(t) {
            Some(p) => perms.push(p),
            None => {
                return JsonRpcResponse::invalid_params(
                    id,
                    format!("unknown permission token: {t}"),
                );
            }
        }
    }

    for p in &perms {
        if !caller_may_grant(caller, p) {
            return JsonRpcResponse::error(
                id,
                -32001,
                format!(
                    "caller cannot grant permission '{}' (not in own permissions)",
                    p.as_token()
                ),
            );
        }
    }

    let parent = match caller {
        CallerContext::Local { .. } => None,
        CallerContext::Plugin { plugin_id, .. } => Some(plugin_id.clone()),
        CallerContext::Agent { agent_id, .. } => Some(agent_id.clone()),
    };

    let now = now_ms();
    reply_session(
        core,
        id,
        params,
        move |store| store.issue(agent_id.clone(), parent.clone(), perms.clone(), ttl_ms, now),
        |core, id, result| match result {
            Ok((token, session)) => crate::adapters::ipc::handler::memory::written(
                core,
                id,
                json!({
                    "token": token.as_str(),
                    "agent_id": session.agent_id,
                    "parent": session.parent,
                    "expires_at_ms": session.expires_at_ms,
                }),
            ),
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// `session.revoke` — 주어진 토큰 무효화.
///
/// params: `{ token: str }`. 응답: `{ revoked: bool }` (없으면 false).
pub fn handle_revoke(
    core: &crate::app::services::AppServices,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let token_str = match params.get("token").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return JsonRpcResponse::invalid_params(id, "Missing 'token'");
        }
    };
    let token = match token_str.parse::<tasty_ipc::caller::SessionToken>() {
        Ok(t) => t,
        Err(_) => {
            return JsonRpcResponse::invalid_params(id, "Invalid 'token' (must be 64 hex chars)");
        }
    };
    reply_session(
        core,
        id,
        params,
        move |store| store.revoke(&token),
        |core, id, result| match result {
            Ok(revoked) => crate::adapters::ipc::handler::memory::written(
                core,
                id,
                json!({ "revoked": revoked }),
            ),
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// `session.list` — 활성 세션 목록 (host 전용, 디버깅/감사용).
pub fn handle_list(core: &crate::app::services::AppServices, id: Value) -> JsonRpcResponse {
    let now = now_ms();
    reply_session(
        core,
        id,
        &Value::Null,
        move |store| store.list(now),
        |_, id, result| match result {
            Ok(sessions) => {
                let arr: Vec<Value> = sessions
                    .into_iter()
                    .map(|s| {
                        json!({
                            "agent_id": s.agent_id,
                            "parent": s.parent,
                            "permissions": s.permissions,
                            "temp_grants": s.temp_grants.iter().map(|g| json!({
                                "permission": g.permission,
                                "expires_at_ms": g.expires_at_ms,
                            })).collect::<Vec<_>>(),
                            "created_at_ms": s.created_at_ms,
                            "expires_at_ms": s.expires_at_ms,
                        })
                    })
                    .collect();
                JsonRpcResponse::success(id, json!({ "sessions": arr }))
            }
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// `plugin.grant_agent_permission` — 활성 agent 에게 임시 권한 grant.
///
/// params:
/// - `agent_id` (str, 필수) — 대상 agent.
/// - `permission` (str, 필수) — 권한 토큰. 알 수 없는 토큰이면 거부.
/// - `ttl_secs` (u64, 옵션) — 만료까지 초. 없으면 무기한 (revoke 까지 유효).
///
/// 동일 token 이 base 에 이미 있으면 noop. 중복 grant 는 만료 시점을 갱신.
/// 응답: `{ agent_id, permission, added, expires_at_ms? }`.
#[cfg(feature = "gui")]
pub fn handle_grant_agent_permission(
    core: &crate::app::services::AppServices,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_id = match params.get("agent_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return JsonRpcResponse::invalid_params(id, "Missing or empty 'agent_id'");
        }
    };
    let permission = match params.get("permission").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return JsonRpcResponse::invalid_params(id, "Missing or empty 'permission'");
        }
    };
    if Permission::from_token(&permission).is_none() {
        return JsonRpcResponse::invalid_params(
            id,
            format!("unknown permission token: {permission}"),
        );
    }
    let ttl_ms =
        p_try!(params::opt_int::<u64>(params, "ttl_secs", &id)).map(|s| s.saturating_mul(1000));
    let now = now_ms();
    let (agent, perm) = (agent_id.clone(), permission.clone());
    reply_session(
        core,
        id,
        params,
        move |store| {
            let Some((token, _)) = store.find_by_agent_id(&agent, now)? else {
                return Ok(None);
            };
            let added = store.grant_permission(&token, &perm, ttl_ms, now)?;
            Ok(Some((added, ttl_ms.map(|t| now.saturating_add(t)))))
        },
        move |_, id, result| match result {
            Ok(Some((added, expires_at))) => JsonRpcResponse::success(
                id,
                json!({
                    "agent_id": agent_id,
                    "permission": permission,
                    "added": added,
                    "expires_at_ms": expires_at,
                }),
            ),
            Ok(None) => session_err_to_response(
                id,
                SessionError::InvalidArgument(format!(
                    "no active session for agent_id '{agent_id}'"
                )),
            ),
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// `plugin.revoke_agent_permission` — 활성 agent 에서 임시 권한 회수.
///
/// params: `{ agent_id, permission }`. base permission 은 건드리지 않는다.
/// 응답: `{ agent_id, permission, removed }`.
#[cfg(feature = "gui")]
pub fn handle_revoke_agent_permission(
    core: &crate::app::services::AppServices,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_id = match params.get("agent_id").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing or empty 'agent_id'"),
    };
    let permission = match params.get("permission").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing or empty 'permission'"),
    };
    let now = now_ms();
    let (agent, perm) = (agent_id.clone(), permission.clone());
    reply_session(
        core,
        id,
        params,
        move |store| {
            let Some((token, _)) = store.find_by_agent_id(&agent, now)? else {
                return Ok(false);
            };
            store.revoke_permission(&token, &perm, now)
        },
        move |_, id, result| match result {
            Ok(removed) => JsonRpcResponse::success(
                id,
                json!({
                    "agent_id": agent_id,
                    "permission": permission,
                    "removed": removed,
                }),
            ),
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// `plugin.list_agent_permissions` — 활성 agent 의 base + temp permission 조회.
///
/// params:
/// - `agent_id` (str, 옵션) — 지정 시 해당 agent 만, 없으면 모든 활성 세션.
///
/// 응답: `{ agents: [{ agent_id, parent, base_permissions, temp_grants: [{permission, expires_at_ms?}] }] }`.
pub fn handle_list_agent_permissions(
    core: &crate::app::services::AppServices,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let target_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let now = now_ms();
    reply_session(
        core,
        id,
        params,
        move |store| store.list(now),
        move |_, id, result| match result {
            Ok(sessions) => {
                let arr: Vec<Value> = sessions
                    .into_iter()
                    .filter(|s| match &target_id {
                        Some(want) => &s.agent_id == want,
                        None => true,
                    })
                    .map(|s| {
                        json!({
                            "agent_id": s.agent_id,
                            "parent": s.parent,
                            "base_permissions": s.permissions,
                            "temp_grants": s.temp_grants.iter().map(|g| json!({
                                "permission": g.permission,
                                "expires_at_ms": g.expires_at_ms,
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                JsonRpcResponse::success(id, json!({ "agents": arr }))
            }
            Err(e) => session_err_to_response(id, e),
        },
    )
}

/// 세션 저장소 연산 하나를 memory writer 요청으로 넣고 그 결과 뒤에 응답한다.
fn reply_session<T: Send + 'static>(
    core: &crate::app::services::AppServices,
    id: Value,
    params: &Value,
    mut body: impl FnMut(&mut SessionStore<'_>) -> Result<T, SessionError> + Send + 'static,
    respond: impl FnOnce(
        &crate::app::services::AppServices,
        Value,
        Result<T, SessionError>,
    ) -> JsonRpcResponse
    + Send
    + 'static,
) -> JsonRpcResponse {
    core.reply_store(
        id,
        crate::adapters::ipc::handler::deferred::params_bytes(params),
        move |mem| body(&mut SessionStore::new(mem, tasty_memory::HOST_OWNER)),
        respond,
    )
}

/// `plugin.request_permission` 핸들러. agent 가 자기 권한 부족을
/// 미리 알고 capability_elevation approval 을 자체 발행할 entry point.
///
/// params:
/// - `agent_id` (str) — 대상. Agent caller 면 미지정 시 caller 자신의 agent_id
///   로 기본값이며 자기 자신 외의 값은 거절한다. Plugin/Local caller 는 필수 (운영자가 대신 발행).
/// - `permission` (str, 필수) — 요청 권한 토큰.
/// - `reason` (str, 옵션) — 사용자에게 보여줄 사유.
///
/// 응답: `{ approval_id }`. dedupe 로직 (같은 agent+permission Pending 재사용)
/// 은 publish_capability_elevation 안에서 처리.
pub fn handle_request_permission(
    core: &mut crate::app::services::AppServices,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    caller: &CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| match caller {
            CallerContext::Agent { agent_id, .. } => Some(agent_id.clone()),
            _ => None,
        });
    let agent_id = match agent_id {
        Some(s) => s,
        None => {
            return JsonRpcResponse::invalid_params(
                id,
                "Missing 'agent_id' (required for non-agent callers)",
            );
        }
    };
    // agent 는 자기 권한만 요청한다. 다른 agent 를 대상으로 한 요청은 사용자 승인 하나로 그 agent 에게
    // 권한을 붙이는 경로가 된다.
    if let CallerContext::Agent {
        agent_id: caller_id,
        ..
    } = caller
        && *caller_id != agent_id
    {
        return JsonRpcResponse::invalid_params(
            id,
            format!(
                "agent callers can only request permissions for themselves: 'agent_id' must be omitted or '{caller_id}'"
            ),
        );
    }
    let permission = match params.get("permission").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JsonRpcResponse::invalid_params(id, "Missing or empty 'permission'"),
    };
    if Permission::from_token(&permission).is_none() {
        return JsonRpcResponse::invalid_params(
            id,
            format!("unknown permission token: {permission}"),
        );
    }
    let reason = params
        .get("reason")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let method = params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("(self-request)");
    match crate::ipc::handler::approval::publish_capability_elevation(
        core,
        window,
        engine,
        &agent_id,
        method,
        &permission,
        reason,
    ) {
        Some(rec) => JsonRpcResponse::success(
            id,
            json!({
                "approval_id": rec.request.id,
                "agent_id": agent_id,
                "permission": permission,
            }),
        ),
        None => JsonRpcResponse::error(id, -32603, "elevation publish failed"),
    }
}

#[cfg(test)]
mod grant_tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;

    fn agent(id: &str, perms: &[Permission]) -> CallerContext {
        let permissions: Arc<HashSet<Permission>> = Arc::new(perms.iter().cloned().collect());
        CallerContext::Agent {
            agent_id: id.into(),
            permissions,
        }
    }

    /// 플러그인은 권한으로 막지 않으므로 어떤 토큰이든 자식에게 넘긴다.
    #[test]
    fn a_plugin_may_hand_any_token_to_a_child() {
        let plugin = CallerContext::Plugin {
            plugin_id: "com.test.owner".into(),
        };
        for p in [
            Permission::IpcInvoke("own".into()),
            Permission::IpcInvoke("other".into()),
            Permission::SurfaceWrite,
        ] {
            assert!(caller_may_grant(&plugin, &p), "{}", p.as_token());
        }
        assert!(caller_may_grant(
            &CallerContext::local(),
            &Permission::SurfaceWrite
        ));
    }

    /// Agent 는 쥔 권한만 넘긴다. 플러그인 ID 와 같은 이름이어도 예외가 없다.
    #[test]
    fn an_agent_hands_only_what_it_holds() {
        let own = Permission::IpcInvoke("own".into());
        assert!(!caller_may_grant(&agent("com.test.owner", &[]), &own));
        assert!(caller_may_grant(
            &agent("child:1", std::slice::from_ref(&own)),
            &own
        ));
        assert!(!caller_may_grant(
            &agent("child:1", &[own]),
            &Permission::SurfaceWrite
        ));
    }
}
