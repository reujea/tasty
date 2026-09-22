//! 적용 실패 신호의 origin 분기 — 사용자 발화의 실패만 toast 를 내고, 에이전트 발화의
//! 실패는 로그로 끝난다(identity 원칙 1, `docs/design/systems/toast.md` "트리거 정책").

use super::*;
use crate::core::MirrorStructuralBlocked;

fn fixture() -> (
    crate::core::Core,
    crate::state::AppState,
    crate::core::CoreState,
) {
    let (state, engine) = crate::state::tests::test_state();
    (
        crate::ipc::handler::cli_entry_tests::test_core(),
        state,
        engine,
    )
}

fn blocked(forwarded: bool) -> anyhow::Error {
    anyhow::Error::new(MirrorStructuralBlocked {
        workspace_index: 0,
        forwarded,
    })
}

fn agent() -> IntentOrigin {
    IntentOrigin::Agent {
        source: AgentSource::Ipc,
    }
}

#[cfg(feature = "gui")]
fn user() -> IntentOrigin {
    IntentOrigin::User {
        source: UserSource::Shortcut("test"),
    }
}

/// forward 할 수 없는 차단(`forwarded=false`)은 사용자 발화에서만 차단 toast 를 낸다.
/// `report_apply_error` 의 `origin.is_user()` 가드를 지우는 변이에서 실패해야 한다.
#[cfg(feature = "gui")]
#[test]
fn an_unforwardable_block_toasts_only_for_the_user() {
    let (_core, mut state, mut engine) = fixture();
    report_apply_error(&mut state, &mut engine, &agent(), "t", &blocked(false));
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 발화의 차단은 toast 를 안 낸다"
    );

    report_apply_error(&mut state, &mut engine, &user(), "t", &blocked(false));
    assert_eq!(
        state.toasts.len(),
        1,
        "사용자 발화의 차단은 종전대로 toast 를 낸다"
    );
}

/// IPC(`markdown.navigate`)가 mirror 워크스페이스의 surface 를 바꾸면 op 는 원격으로
/// forward 되고, 그 op 는 실패 회신이 toast 가 아니라 로그로 가도록 표시된다. 같은 op 를
/// 사용자가 발화하면 표시되지 않는다(원격 실패 toast 는 종전대로).
#[test]
fn an_agent_forward_is_marked_for_a_silent_failure() {
    let (mut core, mut state, mut engine) = fixture();
    let surface_id = *state
        .active_workspace(&engine)
        .all_surface_ids()
        .first()
        .expect("fixture surface");
    engine.workspaces[0].mirror = true;
    let file = tempfile::NamedTempFile::new().expect("tmp file");

    let req = crate::ipc::protocol::JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        method: "markdown.navigate".to_string(),
        params: serde_json::json!({
            "surface_id": surface_id,
            "path": file.path().to_string_lossy(),
        }),
        id: Some(serde_json::json!(1)),
        session_token: None,
    };
    let resp = crate::ipc::handler::handle_with_caller(
        &mut core,
        &mut state,
        &mut engine,
        &req,
        &crate::ipc::caller::CallerContext::Local,
    );
    assert!(resp.error.is_none(), "navigate: {:?}", resp.error);
    crate::intent::headless::drain_pending_intents(&mut core, &mut state, &mut engine);

    assert_eq!(
        engine.pending_structural_forward.len(),
        1,
        "convert 는 forward 된다"
    );
    assert!(
        engine.pending_structural_forward[0].silent_failure,
        "에이전트 발화 op 는 실패 회신을 로그로 보낸다"
    );
    #[cfg(feature = "gui")]
    assert_eq!(state.toasts.len(), 0);

    // 같은 op 를 사용자 발화로 — 표시되지 않는다.
    engine.pending_structural_forward.clear();
    crate::intent::surface::handle(
        &mut core,
        &mut state,
        &mut engine,
        &Intent::ConvertSurface {
            surface_id,
            target: ConvertTarget::Terminal,
        }
        .from_user_menu("test"),
    );
    assert_eq!(engine.pending_structural_forward.len(), 1);
    assert!(!engine.pending_structural_forward[0].silent_failure);
}
