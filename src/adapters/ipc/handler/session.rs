//! `session.*` IPC 핸들러 — 자식 agent 에게 발급하는 [`SessionToken`] 관리.
//!
//! 호스트가 띄운 child 프로세스(예: `claude.spawn`)는 시작 시
//! 환경변수 `TASTY_SESSION_TOKEN` 으로 토큰을 받아 모든 IPC envelope 에 첨부한다.
//! 호스트는 [`crate::ipc::session::SessionStore`] 로 검증해
//! [`CallerContext::Agent`] 로 분기 — agent_id 위조 방지의 핵심.
//!
//! 권한 모델:
//! - `session.issue` 는 `AgentManage` 필요. 호출자(부모) 가 자식에게 권한을
//!   넘긴다. 호스트는 자식에게 **자신이 가진 권한의 부분집합만** 발급할 수
//!   있다 (escalation 방지). Local caller 는 무제한.
//! - `session.revoke` 는 `AgentManage` 필요. 임의 토큰을 무효화 — 부모-자식
//!   확인은 하지 않는다 (어차피 AgentManage 가 있으면 새 token 을 만들 수
//!   있으므로 추가 게이트가 의미 없음).
//! - `session.list` 는 host 전용 (`local_only`). 모든 활성 세션을 본다.

use super::params::{self, p_try};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::plugin::manifest::Permission;
use tasty_ipc::caller::CallerContext;
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::session::SessionError;

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

/// caller 가 자식 토큰에 `p` 를 넣을 수 있는가 — 자기 권한 셋에 있는 것만 넘긴다.
///
/// 예외 하나: plugin 은 **자기가 점유한 namespace** 의 `ipc.invoke:<prefix>` 를 쥐지 않고도
/// 넘길 수 있다. 자기 namespace 호출은 게이트가 토큰 없이 통과시키므로 그 토큰은 plugin
/// 자신에게 쓸모가 없고, 그렇다고 넘기지 못하면 자식 agent 가 발급자에게 돌아오는 호출
/// (예: `claude.hook`)이 막힌다. 이 면제는 plugin 프로세스 caller 에만 선다 — agent 는
/// 이미 받은 토큰 안에서만 넘긴다(ADR-0271).
///
/// `owns_prefix(plugin_id, prefix)` 는 운영에서 `tasty_ipc::method_meta::plugin_owns_prefix`
/// 다. 인자로 받는 것은 소유 표가 프로세스 전역이라 테스트가 그것을 바꿔 끼울 수 없어서다.
fn caller_may_grant(
    caller: &CallerContext,
    p: &Permission,
    owns_prefix: impl Fn(&str, &str) -> bool,
) -> bool {
    let Some(own) = caller.permissions() else {
        return true;
    };
    if own.contains(p) {
        return true;
    }
    match (caller, p) {
        (CallerContext::Plugin { plugin_id, .. }, Permission::IpcInvoke(prefix)) => {
            owns_prefix(plugin_id, prefix)
        }
        _ => false,
    }
}

/// `session.issue` — 자식 agent 에게 새 SessionToken 발급.
///
/// params:
/// - `agent_id` (str, 필수) — 자식의 호스트-부여 식별자. 비어 있으면 거부.
/// - `permissions` (array<str>, 옵션) — 자식에게 부여할 권한 토큰. caller 의
///   권한 셋에 포함되지 않은 토큰이 있으면 거부 (escalation 방지). Local
///   caller 는 모든 토큰 허용.
/// - `ttl_ms` (u64, 옵션) — 토큰 수명. 없으면 자식 프로세스 종료/`session.revoke`
///   까지 유효.
///
/// 응답: `{ token, agent_id, expires_at_ms? }`.
pub fn handle_issue(
    core: &crate::core::Core,
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

    // 권한 토큰을 Permission 으로 매핑하고, 알 수 없는 토큰을 거부.
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

    // Escalation 방지: caller 가 가진 권한의 부분집합만 발급 가능.
    // Local/Internal 은 무제한. Plugin/Agent 는 자기 권한 셋을 기준으로 검사.
    for p in &perms {
        if !caller_may_grant(caller, p, tasty_ipc::method_meta::plugin_owns_prefix) {
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
        CallerContext::Local => None,
        CallerContext::Plugin { plugin_id, .. } => Some(plugin_id.clone()),
        CallerContext::Agent { agent_id, .. } => Some(agent_id.clone()),
    };

    let now = now_ms();
    let result = core.session_issue(agent_id.clone(), parent.clone(), perms.clone(), ttl_ms, now);
    match result {
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
    }
}

/// `session.revoke` — 주어진 토큰 무효화.
///
/// params: `{ token: str }`. 응답: `{ revoked: bool }` (없으면 false).
pub fn handle_revoke(core: &crate::core::Core, id: Value, params: &Value) -> JsonRpcResponse {
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
    let result = core.session_revoke(&token);
    match result {
        Ok(revoked) => {
            crate::adapters::ipc::handler::memory::written(core, id, json!({ "revoked": revoked }))
        }
        Err(e) => session_err_to_response(id, e),
    }
}

/// `session.list` — 활성 세션 목록 (host 전용, 디버깅/감사용).
pub fn handle_list(core: &crate::core::Core, id: Value) -> JsonRpcResponse {
    let now = now_ms();
    let result = core.session_list(now);
    match result {
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
    }
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
    core: &crate::core::Core,
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
    let result = core.session_grant_permission_for_agent(&agent_id, &permission, ttl_ms, now);
    match result {
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
            SessionError::InvalidArgument(format!("no active session for agent_id '{agent_id}'")),
        ),
        Err(e) => session_err_to_response(id, e),
    }
}

/// `plugin.revoke_agent_permission` — 활성 agent 에서 임시 권한 회수.
///
/// params: `{ agent_id, permission }`. base permission 은 건드리지 않는다.
/// 응답: `{ agent_id, permission, removed }`.
#[cfg(feature = "gui")]
pub fn handle_revoke_agent_permission(
    core: &crate::core::Core,
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
    let result = core.session_revoke_permission_for_agent(&agent_id, &permission, now);
    match result {
        Ok(removed) => JsonRpcResponse::success(
            id,
            json!({
                "agent_id": agent_id,
                "permission": permission,
                "removed": removed,
            }),
        ),
        Err(e) => session_err_to_response(id, e),
    }
}

/// `plugin.list_agent_permissions` — 활성 agent 의 base + temp permission 조회.
///
/// params:
/// - `agent_id` (str, 옵션) — 지정 시 해당 agent 만, 없으면 모든 활성 세션.
///
/// 응답: `{ agents: [{ agent_id, parent, base_permissions, temp_grants: [{permission, expires_at_ms?}] }] }`.
pub fn handle_list_agent_permissions(
    core: &crate::core::Core,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let target_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let now = now_ms();
    let result = core.session_list(now);
    match result {
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
    }
}

/// `plugin.request_permission` 핸들러. agent 가 자기 권한 부족을
/// 미리 알고 capability_elevation approval 을 자체 발행할 entry point.
///
/// params:
/// - `agent_id` (str) — 대상. Agent caller 면 미지정 시 caller 자신의 agent_id
///   로 기본값. Plugin/Local caller 는 필수 (운영자가 대신 발행).
/// - `permission` (str, 필수) — 요청 권한 토큰.
/// - `reason` (str, 옵션) — 사용자에게 보여줄 사유.
///
/// 응답: `{ approval_id }`. dedupe 로직 (같은 agent+permission Pending 재사용)
/// 은 publish_capability_elevation 안에서 처리.
pub fn handle_request_permission(
    core: &mut crate::core::Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut crate::core::CoreState,
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

// 옛 handler validation 단위 테스트 모듈은 제거했다.
// 핸들러 시그니처에 `&Core` 가 들어가면서 mock 비용이 크고, 핵심인 영속 통합은
// `crate::ipc::session::tests` 가 SessionStore 직접 호출로 이미 검증한다.

#[cfg(test)]
mod grant_tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;

    const OWNER: &str = "com.test.owner";

    fn owns(plugin_id: &str, prefix: &str) -> bool {
        plugin_id == OWNER && prefix == "own"
    }

    fn with(kind: &str, id: &str, perms: &[Permission]) -> CallerContext {
        let permissions: Arc<HashSet<Permission>> = Arc::new(perms.iter().cloned().collect());
        match kind {
            "plugin" => CallerContext::Plugin {
                plugin_id: id.into(),
                permissions,
            },
            _ => CallerContext::Agent {
                agent_id: id.into(),
                permissions,
            },
        }
    }

    #[test]
    fn a_plugin_may_hand_its_own_namespace_token_to_a_child_without_holding_it() {
        let own = Permission::IpcInvoke("own".into());
        assert!(caller_may_grant(&with("plugin", OWNER, &[]), &own, owns));
    }

    #[test]
    fn the_owner_exemption_does_not_cover_other_namespaces_or_other_permissions() {
        let owner = with("plugin", OWNER, &[]);
        assert!(!caller_may_grant(
            &owner,
            &Permission::IpcInvoke("other".into()),
            owns
        ));
        assert!(!caller_may_grant(&owner, &Permission::SurfaceWrite, owns));
        // 남의 namespace 는 쥐고 있어야 넘긴다.
        let holder = with("plugin", OWNER, &[Permission::IpcInvoke("other".into())]);
        assert!(caller_may_grant(
            &holder,
            &Permission::IpcInvoke("other".into()),
            owns
        ));
    }

    #[test]
    fn an_agent_named_after_the_owner_does_not_get_the_owner_exemption() {
        let own = Permission::IpcInvoke("own".into());
        assert!(!caller_may_grant(&with("agent", OWNER, &[]), &own, owns));
        assert!(!caller_may_grant(
            &with("plugin", "com.test.stranger", &[]),
            &own,
            owns
        ));
        assert!(caller_may_grant(&CallerContext::Local, &own, owns));
    }
}
