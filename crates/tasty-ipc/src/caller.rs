//! Local·Plugin·Agent 호출자를 구분하고 요청 권한을 확인한다.
//! Local은 이 권한 검사를 통과하며, 다른 호출자는 method_meta와 부여된 권한을 대조한다.
//! 호스트는 조기 응답 전에 검사해야 한다. 라우팅 순서는 every_routing_entry_gates_before_it_answers가 확인한다.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::method_meta::method_meta;
use tasty_plugin_manifest::Permission;

/// 32바이트 난수를 64자 hex로 인코딩한 세션 토큰.
/// 자식이 TASTY_SESSION_TOKEN을 요청에 실으면 SessionStore가 신원을 검증한다.
/// agent_id 문자열만으로는 호출자를 인증하지 않는다.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct SessionToken(String);

impl SessionToken {
    /// 32바이트 OS random → hex 64 chars.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let mut s = String::with_capacity(64);
        for b in bytes {
            use std::fmt::Write;
            write!(&mut s, "{b:02x}").unwrap();
        }
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// [`SessionToken`] 파싱 에러 — hex 64 chars 형식 위반.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSessionToken;

impl fmt::Display for InvalidSessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid session token (expected 64 hex chars)")
    }
}

impl std::error::Error for InvalidSessionToken {}

impl std::str::FromStr for SessionToken {
    type Err = InvalidSessionToken;

    /// 외부 입력(envelope, env) 으로부터 token 을 받는다. hex 64 chars 검증.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(InvalidSessionToken);
        }
        Ok(Self(s.to_ascii_lowercase()))
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 토큰 자체는 비밀이므로 prefix 만 노출.
        write!(f, "SessionToken({}…)", &self.0[..8])
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// IPC 명령을 누가 보냈는가.
#[derive(Debug, Clone)]
pub enum CallerContext {
    /// CLI/네트워크 IPC 클라이언트(사용자) — 모든 메서드 자동 허용.
    /// TCP 포트는 로컬 기기 액세스를 전제로 하므로 별도 권한 검사 없음.
    Local {
        /// 호출자가 요청 봉투의 `caller_agent_id` 로 밝힌 agent ID. 검증할 수 없는 자기 신고라
        /// 텔레메트리·감사·헤드리스 PTY 소유자 표시에만 쓰고 권한·memory owner·rate limit
        /// 판단에는 쓰지 않는다(docs/dev-guide/plugin-permissions.md). 형식이 틀렸거나 없으면 None 이다.
        claimed_agent_id: Option<tasty_telemetry::AgentId>,
    },
    /// 외부 plugin process가 호출. 매니페스트의 `permissions`만 허용.
    Plugin {
        plugin_id: String,
        /// 매니페스트에 선언되고 사용자가 grant한 권한. 런타임에 in-place로
        /// 갱신되지 않고 전체를 새 Arc로 교체하므로, 동시 호출 안전.
        permissions: Arc<HashSet<Permission>>,
    },
    /// claude.spawn 등으로 호스트가 띄운 child Claude / 외부 agent.
    /// SessionToken 으로 신원 검증한 뒤에만 이 variant 가 만들어진다.
    Agent {
        /// 'child:1', 'claude:abc' 등 호스트-부여 식별자.
        agent_id: String,
        /// 부모로부터 상속된 권한 + grant 로 보강된 권한.
        permissions: Arc<HashSet<Permission>>,
    },
}

#[derive(Debug)]
pub enum CallerError {
    UnknownMethod(String),
    NotPluginCallable {
        caller_label: String,
        method: String,
    },
    MissingPermission {
        caller_label: String,
        method: String,
        permission: Permission,
    },
}

impl std::fmt::Display for CallerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallerError::UnknownMethod(m) => write!(f, "unknown ipc method: {m}"),
            CallerError::NotPluginCallable {
                caller_label,
                method,
            } => {
                write!(f, "method '{method}' is not callable from '{caller_label}'")
            }
            CallerError::MissingPermission {
                caller_label,
                method,
                permission,
            } => write!(
                f,
                "'{caller_label}' missing permission '{}' for method '{method}'",
                permission.as_token()
            ),
        }
    }
}

impl std::error::Error for CallerError {}

impl CallerContext {
    /// 자기 신고 agent ID 가 없는 Local 호출자(사용자·호스트 내부 경로).
    pub const fn local() -> Self {
        CallerContext::Local {
            claimed_agent_id: None,
        }
    }

    /// 봉투의 `caller_agent_id` 로 Local 호출자를 만든다. 텔레메트리 agent ID 형식
    /// (`[a-zA-Z0-9_-]`, 64자 이하)이 아니면 버리고 `_host` 로 기록되게 한다. 요청은 거절하지
    /// 않는다 — 표시용 값 때문에 사용자의 명령이 실패하면 안 된다.
    pub fn local_claiming(claimed: Option<&str>) -> Self {
        let claimed_agent_id =
            claimed.and_then(|raw| match tasty_telemetry::validate_agent_id(raw) {
                Ok(()) => Some(tasty_telemetry::AgentId::new(raw)),
                Err(e) => {
                    tracing::warn!("ignoring caller_agent_id: {e}");
                    None
                }
            });
        CallerContext::Local { claimed_agent_id }
    }

    /// 호출하려는 메서드가 caller에게 허용되는지 확인.
    pub fn ensure_allowed(&self, method: &str) -> Result<(), CallerError> {
        match self {
            CallerContext::Local { .. } => Ok(()),
            CallerContext::Plugin {
                plugin_id,
                permissions,
            } => check_permissions(plugin_id, Some(plugin_id), permissions, method),
            CallerContext::Agent {
                agent_id,
                permissions,
                ..
            } => check_permissions(agent_id, None, permissions, method),
        }
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self, CallerContext::Plugin { .. })
    }

    /// 권한 집합. Local은 제한이 없어 None이다.
    pub fn permissions(&self) -> Option<&Arc<HashSet<Permission>>> {
        match self {
            CallerContext::Plugin { permissions, .. }
            | CallerContext::Agent { permissions, .. } => Some(permissions),
            _ => None,
        }
    }

    /// memory.db `owner` 값 도출.
    pub fn owner(&self) -> &str {
        match self {
            CallerContext::Local { .. } => tasty_memory::HOST_OWNER,
            CallerContext::Plugin { plugin_id, .. } => plugin_id.as_str(),
            CallerContext::Agent { agent_id, .. } => agent_id.as_str(),
        }
    }

    /// 텔레메트리에 쓸 호출자 ID. Agent는 세션에서 확인한 에이전트 ID, Plugin은 매니페스트 ID를
    /// 허용 문자로 변환한 값, Local은 봉투로 밝힌 자기 신고 ID, 없으면 _host다. 호스트 프로세스
    /// 자신의 `TASTY_AGENT_ID` 는 읽지 않는다. 그 값은 이 Tasty 를 띄운 바깥 인스턴스에서 상속한
    /// 것이라 호출자와 관계가 없다. Local 의 값은 위조할 수 있으므로 표시·집계에만 쓴다.
    pub fn agent_id(&self) -> tasty_telemetry::AgentId {
        match self {
            CallerContext::Local { claimed_agent_id } => claimed_agent_id
                .clone()
                .unwrap_or_else(tasty_telemetry::AgentId::host),
            CallerContext::Plugin { plugin_id, .. } => {
                tasty_telemetry::AgentId::from_plugin_id(plugin_id)
            }
            CallerContext::Agent { agent_id, .. } => {
                tasty_telemetry::AgentId::new(agent_id.clone())
            }
        }
    }
}

/// `caller_plugin_id` 는 caller 가 **plugin 프로세스일 때만** 준다. agent 의 `agent_id` 는
/// 발급자가 고른 문자열이라 plugin id 와 같은 값일 수 있고, 그 값으로 소유자 면제를 받으면
/// 토큰을 발급할 수 있는 누구나 임의 namespace 를 여는 셈이 된다.
fn check_permissions(
    caller_label: &str,
    caller_plugin_id: Option<&str>,
    permissions: &Arc<HashSet<Permission>>,
    method: &str,
) -> Result<(), CallerError> {
    let meta = method_meta(method).ok_or_else(|| CallerError::UnknownMethod(method.to_string()))?;
    if !meta.plugin_callable {
        return Err(CallerError::NotPluginCallable {
            caller_label: caller_label.to_string(),
            method: method.to_string(),
        });
    }
    for needed in meta.required {
        if !permissions.contains(needed) {
            return Err(CallerError::MissingPermission {
                caller_label: caller_label.to_string(),
                method: method.to_string(),
                permission: needed.clone(),
            });
        }
    }
    if meta.namespace_forward {
        // 표에 없는 namespace 메서드는 ipc.invoke:<prefix>가 필요하다.
        // 소유 플러그인의 자기 namespace 호출만 면제하며, 같은 ID의 Agent는 면제하지 않는다.
        let prefix = method.split('.').next().unwrap_or("");
        let is_owner =
            caller_plugin_id.is_some_and(|id| crate::method_meta::plugin_owns_prefix(id, prefix));
        let needed = Permission::IpcInvoke(prefix.to_string());
        if !is_owner && !permissions.contains(&needed) {
            return Err(CallerError::MissingPermission {
                caller_label: caller_label.to_string(),
                method: method.to_string(),
                permission: needed,
            });
        }
    }
    Ok(())
}

/// 봉투가 정한 호출자. 세션 토큰이 있으면 세션 저장소에서 판정해야 한다.
pub enum EnvelopeCaller<'r> {
    /// 토큰이 없다. 봉투의 `caller_agent_id`는 표시용으로만 쓴다.
    Local(CallerContext),
    /// 형식이 맞는 토큰. [`caller_from_session`]에 판정 결과를 넘긴다.
    Session(&'r str),
}

/// envelope 의 `session_token` 필드를 보고 caller 를 결정한다.
///
/// - `session_token` 가 None → `CallerContext::Local`
/// - 형식이 잘못된 토큰(64-char hex 아님) → `Err(deny)`
/// - 유효 형식 → 세션 판정이 필요하다. 저장소 판정은 호출자가 하고 결과를 [`caller_from_session`]에 넘긴다.
pub fn envelope_caller(
    request: &crate::protocol::JsonRpcRequest,
) -> Result<EnvelopeCaller<'_>, crate::protocol::JsonRpcResponse> {
    let Some(token_str) = request.session_token.as_deref() else {
        return Ok(EnvelopeCaller::Local(CallerContext::local_claiming(
            request.caller_agent_id.as_deref(),
        )));
    };
    // 형식 위반 검증을 먼저 (trait 의 NotFound 와 invalid_format 구분 보존).
    if token_str.parse::<SessionToken>().is_err() {
        return Err(session_deny(
            request,
            "invalid session_token format (expect 64 hex chars)",
        ));
    }
    Ok(EnvelopeCaller::Session(token_str))
}

fn session_deny(
    request: &crate::protocol::JsonRpcRequest,
    msg: &str,
) -> crate::protocol::JsonRpcResponse {
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    crate::protocol::JsonRpcResponse::error(id, -32001, format!("permission_denied: {msg}"))
}

/// 세션 판정 결과로 호출자를 정한다. 없음/만료/철회는 `Err(deny)` 이며 Local 로 대신하지 않는다.
pub fn caller_from_session(
    request: &crate::protocol::JsonRpcRequest,
    resolution: crate::SessionResolution,
) -> Result<CallerContext, crate::protocol::JsonRpcResponse> {
    use std::collections::HashSet;
    use std::sync::Arc;

    use crate::SessionResolution;

    match resolution {
        SessionResolution::NotFound => Err(session_deny(
            request,
            "session_token unknown/expired/revoked",
        )),
        SessionResolution::Agent {
            agent_id,
            permissions,
        } => {
            let perms: HashSet<tasty_plugin_manifest::Permission> = permissions
                .iter()
                .filter_map(|t| tasty_plugin_manifest::Permission::from_token(t))
                .collect();
            Ok(CallerContext::Agent {
                agent_id,
                permissions: Arc::new(perms),
            })
        }
        SessionResolution::Plugin {
            plugin_id,
            permissions,
        } => {
            // Plugin caller 케이스는 현재 본 바이너리 store 가 발급하지 않으나
            // trait 호환을 위해 처리. agent_id 로 plugin_id 사용.
            let perms: HashSet<tasty_plugin_manifest::Permission> = permissions
                .iter()
                .filter_map(|t| tasty_plugin_manifest::Permission::from_token(t))
                .collect();
            Ok(CallerContext::Agent {
                agent_id: plugin_id,
                permissions: Arc::new(perms),
            })
        }
    }
}

/// [`envelope_caller`]와 [`caller_from_session`]을 이어 동기 판정한다. 세션 판정을 기다려도 되는
/// 호출자만 쓴다. 호스트의 요청 게이트는 판정을 writer에 넣고 결과를 거둔 뒤 이어간다.
pub fn resolve_caller_from_envelope(
    host: &dyn crate::IpcHostFacade,
    request: &crate::protocol::JsonRpcRequest,
) -> Result<CallerContext, crate::protocol::JsonRpcResponse> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let token_str = match envelope_caller(request)? {
        EnvelopeCaller::Local(caller) => return Ok(caller),
        EnvelopeCaller::Session(token) => token,
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    caller_from_session(request, host.session_resolve(token_str, now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 봉투의 `caller_agent_id` 는 토큰이 없을 때만 Local 의 표시 ID 가 되고, 세션 토큰이 있으면
    /// 세션이 정한 ID 가 우선한다. 형식이 틀린 값은 버리되 요청은 거절하지 않는다.
    #[test]
    fn the_envelope_claim_names_a_local_caller_but_never_a_session() {
        struct OneSession;
        impl crate::IpcHostFacade for OneSession {
            fn session_resolve(&self, _token: &str, _now_ms: u64) -> crate::SessionResolution {
                crate::SessionResolution::Agent {
                    agent_id: "child_1".into(),
                    permissions: vec![],
                }
            }
            fn record_audit(
                &self,
                _caller: crate::AuditCallerMarker,
                _method: &str,
                _decision: crate::AuditDecision,
                _reason: Option<&str>,
                _workspace_id: Option<u32>,
                _seq: u64,
                _ts_ms: u64,
            ) {
            }
        }
        let envelope = |token: Option<&str>, claim: Option<&str>| {
            serde_json::from_value::<crate::protocol::JsonRpcRequest>(serde_json::json!({
                "jsonrpc": "2.0", "method": "system.info", "id": 1,
                "session_token": token, "caller_agent_id": claim,
            }))
            .expect("envelope")
        };
        let resolve = |token: Option<&str>, claim: Option<&str>| {
            resolve_caller_from_envelope(&OneSession, &envelope(token, claim))
                .expect("resolved")
                .agent_id()
        };
        assert_eq!(resolve(None, Some("agent_a")).as_str(), "agent_a");
        assert!(resolve(None, None).is_host());
        assert!(resolve(None, Some("")).is_host());
        assert!(resolve(None, Some("bad id!")).is_host());
        assert!(resolve(None, Some(&"a".repeat(65))).is_host());
        let token = "ab".repeat(32);
        assert_eq!(resolve(Some(&token), Some("agent_a")).as_str(), "child_1");
    }

    /// 호스트 프로세스가 바깥 인스턴스의 `TASTY_AGENT_ID` 를 상속했어도 Local 호출자는 `_host` 다.
    /// 이 시험만 `TASTY_AGENT_ID` 를 둔 자식 프로세스로 다시 실행한다.
    #[test]
    fn local_caller_ignores_the_host_inherited_agent_id() {
        const CHILD: &str = "TASTY_TEST_INHERITED_AGENT_ID_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // 자식의 결과 줄이 부모 시험의 출력에 섞이지 않게 캡처하고, 실패할 때만 보여 준다.
            let output = std::process::Command::new(std::env::current_exe().expect("exe"))
                .args([
                    "--exact",
                    "caller::tests::local_caller_ignores_the_host_inherited_agent_id",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .env("TASTY_AGENT_ID", "outer_agent")
                .output()
                .expect("child");
            assert!(
                output.status.success(),
                "child run failed: {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert_eq!(
            std::env::var("TASTY_AGENT_ID").expect("inherited"),
            "outer_agent"
        );
        assert!(CallerContext::local().agent_id().is_host());
    }

    fn plugin_with(perms: &[Permission]) -> CallerContext {
        CallerContext::Plugin {
            plugin_id: "com.example.test".into(),
            permissions: Arc::new(perms.iter().cloned().collect()),
        }
    }

    fn agent_with(perms: &[Permission]) -> CallerContext {
        CallerContext::Agent {
            agent_id: "child:1".into(),
            permissions: Arc::new(perms.iter().cloned().collect()),
        }
    }

    #[test]
    fn local_passes_all_known_methods() {
        let c = CallerContext::local();
        assert!(c.ensure_allowed("surface.list").is_ok());
        assert!(c.ensure_allowed("debug.inject_key").is_ok());
        assert!(c.ensure_allowed("plugin.enable").is_ok());
    }

    #[test]
    fn local_passes_unknown_methods_too() {
        // Local은 method_meta 검사 없이 통과 — 알려지지 않은 메서드도 라우터의
        // method_not_found가 처리하도록 위임.
        assert!(
            CallerContext::local()
                .ensure_allowed("not.a.real.method")
                .is_ok()
        );
    }

    #[test]
    fn plugin_missing_permission_denied() {
        let c = plugin_with(&[Permission::SurfaceRead]);
        let err = c.ensure_allowed("tab.create").unwrap_err();
        assert!(matches!(
            err,
            CallerError::MissingPermission {
                permission: Permission::SurfaceWrite,
                ..
            }
        ));
    }

    /// 권한이 하나도 없는 호출자도 mirror attach 와 그 거절 조회를 게이트에서 거절당하지 않는다.
    #[test]
    fn remote_attach_passes_the_gate_without_any_permission() {
        for caller in [plugin_with(&[]), agent_with(&[])] {
            for method in ["remote.attach", "remote.refusals"] {
                assert!(caller.ensure_allowed(method).is_ok(), "{method}");
            }
        }
    }

    #[test]
    fn plugin_missing_clipboard_write_denied_for_clipboard_set_text() {
        let c = plugin_with(&[Permission::SurfaceRead]);
        let err = c.ensure_allowed("clipboard.set_text").unwrap_err();
        assert!(matches!(
            err,
            CallerError::MissingPermission {
                permission: Permission::ClipboardWrite,
                ..
            }
        ));
    }

    #[test]
    fn plugin_with_permission_passes() {
        let c = plugin_with(&[Permission::SurfaceWrite]);
        assert!(c.ensure_allowed("tab.create").is_ok());
    }

    #[test]
    fn plugin_cannot_call_local_only_method() {
        let c = plugin_with(&[Permission::SurfaceWrite]);
        let err = c.ensure_allowed("plugin.enable").unwrap_err();
        assert!(matches!(err, CallerError::NotPluginCallable { .. }));
    }

    #[test]
    fn agent_with_permission_passes() {
        let c = agent_with(&[Permission::SurfaceWrite]);
        assert!(c.ensure_allowed("tab.create").is_ok());
    }

    #[test]
    fn agent_missing_permission_denied() {
        let c = agent_with(&[Permission::SurfaceRead]);
        let err = c.ensure_allowed("tab.create").unwrap_err();
        assert!(matches!(
            err,
            CallerError::MissingPermission {
                permission: Permission::SurfaceWrite,
                ..
            }
        ));
    }

    #[test]
    fn agent_cannot_call_local_only_method() {
        let c = agent_with(&[Permission::SurfaceWrite]);
        let err = c.ensure_allowed("plugin.enable").unwrap_err();
        assert!(matches!(err, CallerError::NotPluginCallable { .. }));
    }

    #[test]
    fn session_token_generate_is_64_hex() {
        let t = SessionToken::generate();
        assert_eq!(t.as_str().len(), 64);
        assert!(t.as_str().bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn session_token_from_str_validates_length_and_charset() {
        assert!("".parse::<SessionToken>().is_err());
        assert!("too short".parse::<SessionToken>().is_err());
        let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(valid.parse::<SessionToken>().unwrap().as_str(), valid);
        let invalid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdez";
        assert!(invalid.parse::<SessionToken>().is_err());
    }

    #[test]
    fn session_token_from_str_normalizes_case() {
        let upper = "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";
        assert_eq!(
            upper.parse::<SessionToken>().unwrap().as_str(),
            upper.to_ascii_lowercase()
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    fn plugin_cannot_call_debug_method_even_with_all_perms() {
        let all: Vec<Permission> = [
            Permission::SurfaceRead,
            Permission::SurfaceWrite,
            Permission::TerminalRead,
            Permission::TerminalWrite,
            Permission::Notification,
        ]
        .into_iter()
        .collect();
        let c = plugin_with(&all);
        let err = c.ensure_allowed("debug.inject_key").unwrap_err();
        assert!(matches!(err, CallerError::NotPluginCallable { .. }));
    }

    /// release 빌드에서는 debug 메서드가 method_meta에 아예 등록되지 않는다.
    /// 따라서 plugin이 호출하면 `NotPluginCallable`이 아니라 `UnknownMethod`로
    /// 떨어진다 — 호출 거부라는 결과는 같지만 메시지가 다르다.
    #[test]
    #[cfg(not(debug_assertions))]
    fn plugin_call_to_debug_method_is_unknown_in_release() {
        let c = plugin_with(&[]);
        let err = c.ensure_allowed("debug.inject_key").unwrap_err();
        assert!(matches!(err, CallerError::UnknownMethod(_)));
    }

    /// 원격 사용자를 끊는 공개 메서드는 없으므로 attach 제어를 부를 수 있는 plugin 도 부를 수 없다.
    /// debug 재현 메서드는 local 전용이라 debug 빌드에서도 plugin 에 닫혀 있다.
    #[test]
    fn plugin_cannot_call_force_detach() {
        let c = plugin_with(&[Permission::SurfaceRead, Permission::SurfaceWrite]);
        assert!(c.ensure_allowed("attach.release").is_ok());
        for m in ["attach.force_detach", "attach.force_detach_workspace"] {
            let err = c.ensure_allowed(m).unwrap_err();
            assert!(matches!(err, CallerError::UnknownMethod(_)), "{m}: {err}");
        }
        for m in [
            "debug.attach.force_detach",
            "debug.attach.force_detach_workspace",
        ] {
            assert!(c.ensure_allowed(m).is_err(), "{m}");
        }
    }

    #[test]
    fn plugin_unknown_method_errors() {
        let c = plugin_with(&[]);
        let err = c.ensure_allowed("not.a.real.method").unwrap_err();
        assert!(matches!(err, CallerError::UnknownMethod(_)));
    }

    #[test]
    fn plugin_ipc_invoke_permission_does_not_unlock_static_methods() {
        // ipc.invoke 권한은 namespace forward 경로에서만 의미가 있고,
        // 정적 method_meta 권한 검사를 우회하지 않는다.
        let c = plugin_with(&[Permission::IpcInvoke("codex".into())]);
        let err = c.ensure_allowed("tab.create").unwrap_err();
        assert!(matches!(
            err,
            CallerError::MissingPermission {
                permission: Permission::SurfaceWrite,
                ..
            }
        ));
    }
}
