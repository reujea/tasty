//! 복원 **입구**가 발화자의 origin 을 cascade 까지 그대로 넘기는지 고정한다.
//!
//! cascade 자신의 판정(사용자면 옮기고 아니면 그대로)은 `app/dispatch_domain_restore_tests.rs`
//! 가 origin 을 직접 주입해 잰다. 그 시험은 입구가 무엇을 넘기는지는 안 본다 — 입구가
//! origin 을 `System` 으로 바꿔 넘기면 사용자 단축키 복원이 포커스를 안 옮기는 회귀가
//! 조용히 통과한다. 그래서 여기서는 `handle` 에 실제 발화 형태의 intent 를 넣는다.

use super::handle;
use crate::intent::Intent;
use crate::state::WorkspaceCloseOrigin;

/// 워크스페이스 둘 중 뒤쪽을 사용자가 닫아 되돌리기 스택에 올린 뒤, 첫째를 보는 상태에서
/// `intent` 로 복원하고 활성 워크스페이스 인덱스를 돌려준다.
fn active_after_restoring_a_closed_workspace(intent: crate::intent::DispatchedIntent) -> usize {
    let (mut state, mut engine) = crate::state::tests::test_state();
    let event = crate::core::apply_create_workspace_inner(
        &mut engine,
        crate::core::WorkspaceCreationParams::terminal(),
    )
    .expect("second workspace");
    let crate::core::intent::CoreEvent::WorkspaceCreated { index, .. } = event else {
        panic!("apply_create_workspace_inner 가 WorkspaceCreated 외 반환");
    };
    assert_eq!(index, 1);
    assert!(state.close_workspace_at(&mut engine, 1, WorkspaceCloseOrigin::User));
    state.active_workspace = 0;
    assert_eq!(engine.workspaces.len(), 1);
    assert_eq!(
        engine.closed_items.len(),
        1,
        "사용자 close 는 스택에 올린다"
    );

    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core();
    handle(&mut core, &mut state, &mut engine, &intent);
    assert_eq!(
        engine.workspaces.len(),
        2,
        "복원이 워크스페이스를 되살려야 한다"
    );
    assert_eq!(engine.closed_items.len(), 0);
    state.active_workspace
}

#[test]
fn a_shortcut_restore_moves_the_active_workspace_to_the_restored_one() {
    let intent = Intent::RestoreClosedItem.from_user_shortcut("restore_closed");
    assert_eq!(active_after_restoring_a_closed_workspace(intent), 1);
}

#[test]
fn an_agent_restore_leaves_the_active_workspace_alone() {
    let intent = Intent::RestoreClosedItem.from_agent_ipc();
    assert_eq!(active_after_restoring_a_closed_workspace(intent), 0);
}
