//! 권한 상승 요청은 사용자(GUI·Local caller)만 종결한다. `Approval` 권한을 가진 agent 도
//! 다른 agent 의 요청을 승인·거절·취소하거나, 다른 agent 를 대상으로 요청을 만들 수 없다.
//! plugin 호출자는 토큰 없이 plugin_callable 여부만 검사받으며, 역시 요청을 승인할 수 없다.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::json;
use tasty_approval::{ApprovalId, ApprovalState, Requester, Severity};
use tasty_ipc::caller::{CallerContext, SessionToken};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::session::SessionStore;
use tasty_memory::{MemoryConfig, MemoryStore};

use crate::adapters::ipc::handler::approval::publish_capability_elevation_at;
use crate::adapters::ipc::handler::deferred::tests::{
    make_request, run_request_as, writer_core_with,
};
use crate::app::services::AppServices;
use crate::plugin::manifest::Permission;

/// 권한을 얻으려는 agent.
const TARGET: &str = "child:target";
/// `Approval` 권한으로 대신 승인하려는 다른 agent.
const OTHER: &str = "child:other";
const PERMISSION: &str = "fs.write";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
}

fn core() -> AppServices {
    let memory = Arc::new(Mutex::new(
        MemoryStore::open_in_memory_with_config(MemoryConfig::default()).expect("store"),
    ));
    writer_core_with(memory, usize::MAX)
}

fn issue_session(core: &AppServices, agent_id: &str) -> SessionToken {
    core.with_memory(|mem| {
        SessionStore::new(mem, tasty_memory::HOST_OWNER).issue(
            agent_id,
            None,
            Vec::new(),
            None,
            now_ms(),
        )
    })
    .expect("issue session")
    .0
}

fn has_permission(core: &AppServices, token: &SessionToken) -> bool {
    crate::ipc::handler::gate::session_permissions_waiting(core, &token.to_string())
        .expect("session must resolve to the agent")
        .iter()
        .any(|p| p == PERMISSION)
}

fn approval_holder() -> HashSet<Permission> {
    [Permission::Approval].into_iter().collect()
}

fn agent(id: &str) -> CallerContext {
    CallerContext::Agent {
        agent_id: id.into(),
        permissions: Arc::new(approval_holder()),
    }
}

fn plugin() -> CallerContext {
    CallerContext::Plugin {
        plugin_id: "com.example.approver".into(),
    }
}

fn call(
    core: &mut AppServices,
    caller: &CallerContext,
    method: &str,
    params: serde_json::Value,
) -> JsonRpcResponse {
    run_request_as(core, &make_request(1, method, params, None), caller).0
}

/// `plugin.request_permission`은 App 경계가 직접 핸들러를 부르므로 그 핸들러를 같은 방식으로 부른다.
fn request_permission(
    core: &mut AppServices,
    caller: &CallerContext,
    params: serde_json::Value,
) -> JsonRpcResponse {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let mut scope = crate::ipc::request_scope::RequestScope::capture(
        &mut state,
        engine.core,
        #[cfg(feature = "gui")]
        None,
    );
    crate::adapters::ipc::handler::session::handle_request_permission(
        core,
        &mut scope,
        &mut engine,
        caller,
        json!(1),
        &params,
    )
}

/// host 가 권한 거부로 발행한 TARGET 의 권한 상승 요청(경로 B의 시작).
fn target_elevation(core: &mut AppServices) -> ApprovalId {
    publish_capability_elevation_at(core, None, TARGET, "file.write", PERMISSION, None)
        .expect("elevation request")
        .request
        .id
}

fn assert_user_response_required(response: &JsonRpcResponse) {
    let error = response.error.as_ref().expect("must be rejected");
    assert_eq!(error.code, -32019, "{}", error.message);
    assert!(
        error.message.starts_with("user_response_required:"),
        "{}",
        error.message
    );
}

fn is_pending(core: &AppServices, id: &ApprovalId) -> bool {
    matches!(
        core.approval_store.get(id).expect("record").state,
        ApprovalState::Pending
    )
}

/// 경로 A: agent 는 다른 agent 를 대상으로 권한 상승을 요청하지 못한다.
#[test]
fn an_agent_cannot_request_elevation_for_another_agent() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let response = request_permission(
        &mut core,
        &agent(OTHER),
        json!({"agent_id": TARGET, "permission": PERMISSION}),
    );
    let error = response.error.expect("other target must be rejected");
    assert_eq!(error.code, -32602, "{}", error.message);
    assert!(
        core.approval_store.list().is_empty(),
        "nothing is requested"
    );
}

/// agent 는 자기 자신을 대상으로는 요청할 수 있다(생략과 명시 모두).
#[test]
fn an_agent_can_request_elevation_for_itself() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    for params in [
        json!({"permission": PERMISSION}),
        json!({"agent_id": OTHER, "permission": PERMISSION}),
    ] {
        let response = request_permission(&mut core, &agent(OTHER), params);
        assert!(response.error.is_none(), "{response:?}");
    }
    let records = core.approval_store.list();
    assert_eq!(
        records.len(),
        1,
        "the second call reuses the pending request"
    );
    let elevation = records[0]
        .request
        .host_elevation
        .as_ref()
        .expect("host elevation");
    assert_eq!(elevation.agent_id, OTHER);
}

/// 사용자(Local caller)는 다른 agent 를 대상으로 요청을 만들 수 있다.
#[test]
fn the_user_can_request_elevation_for_any_agent() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let response = request_permission(
        &mut core,
        &CallerContext::local(),
        json!({"agent_id": TARGET, "permission": PERMISSION}),
    );
    assert!(response.error.is_none(), "{response:?}");
}

/// 경로 A·B: 다른 agent 가 IPC 로 승인해도 권한이 붙지 않고 요청은 사용자를 기다린다.
#[test]
fn another_agent_cannot_approve_an_elevation() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let token = issue_session(&core, TARGET);
    let id = target_elevation(&mut core);

    for choice in ["approve", "approve_permanently"] {
        let response = call(
            &mut core,
            &agent(OTHER),
            "approval.respond",
            json!({"id": id, "choice": choice}),
        );
        assert_user_response_required(&response);
    }

    assert!(!has_permission(&core, &token));
    assert!(is_pending(&core, &id));
}

/// plugin 도 사용자를 대신해 권한 상승을 승인하지 못한다.
#[test]
fn a_plugin_cannot_approve_an_elevation() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let token = issue_session(&core, TARGET);
    let id = target_elevation(&mut core);

    let response = call(
        &mut core,
        &plugin(),
        "approval.respond",
        json!({"id": id, "choice": "approve"}),
    );

    assert_user_response_required(&response);
    assert!(!has_permission(&core, &token));
    assert!(is_pending(&core, &id));
}

/// 거절·취소도 사용자만 한다. agent 가 사용자의 판단을 앞질러 요청을 닫지 못한다.
#[test]
fn another_agent_cannot_deny_or_cancel_an_elevation() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let id = target_elevation(&mut core);

    let denied = call(
        &mut core,
        &agent(OTHER),
        "approval.respond",
        json!({"id": id, "choice": "deny"}),
    );
    assert_user_response_required(&denied);
    let cancelled = call(
        &mut core,
        &agent(OTHER),
        "approval.cancel",
        json!({"id": id}),
    );
    assert_user_response_required(&cancelled);

    assert!(is_pending(&core, &id));
}

/// 사용자(Local caller)의 IPC 응답은 그대로 권한을 붙인다.
#[test]
fn the_user_still_approves_an_elevation_over_ipc() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let token = issue_session(&core, TARGET);
    let id = target_elevation(&mut core);

    let response = call(
        &mut core,
        &CallerContext::local(),
        "approval.respond",
        json!({"id": id, "choice": "approve"}),
    );

    assert!(response.error.is_none(), "{response:?}");
    assert!(has_permission(&core, &token));
}

/// 권한 상승이 아닌 일반 승인 요청은 지금처럼 다른 agent 가 응답·취소할 수 있다.
#[test]
fn agents_still_answer_ordinary_approvals() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let mut core = core();
    let ordinary = |core: &mut AppServices| {
        let request = tasty_approval::ApprovalRequest {
            id: ApprovalId::generate(),
            requester: Requester::Plugin { id: TARGET.into() },
            workspace_id: None,
            surface_id: None,
            title: "deploy?".into(),
            body: None,
            choices: Vec::new(),
            default_choice: None,
            timeout_ms: None,
            severity: Severity::Warn,
            created_at: 0,
            metadata: serde_json::Value::Null,
            host_elevation: None,
        };
        let requested = core.request_approval(request).expect("request");
        requested.record.detach(core.memory_access());
        requested.change.record.request.id
    };

    let id = ordinary(&mut core);
    let response = call(
        &mut core,
        &agent(OTHER),
        "approval.respond",
        json!({"id": id, "choice": "approve"}),
    );
    assert!(response.error.is_none(), "{response:?}");

    let id = ordinary(&mut core);
    let response = call(
        &mut core,
        &agent(OTHER),
        "approval.cancel",
        json!({"id": id}),
    );
    assert!(response.error.is_none(), "{response:?}");
}
