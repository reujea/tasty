//! 닫은 항목 복원 cascade 는 **사용자 발화일 때만** 포커스 포인터를 옮긴다.
//!
//! 복원은 오늘 사용자 단축키에서만 발화하지만, 그 사실은 호출 자리의 약속이지 이 cascade 의
//! 판정이 아니었다. 복원이 IPC 로 열리면 에이전트 행동이 사용자의 활성 워크스페이스를 옮기게
//! 되므로(원칙 2.1 ① · 2.3), cascade 가 origin 을 직접 본다. 이 시험이 그 판정을 고정한다.

use super::cascade_closed_item_restored;
use crate::core::intent::RestoredKind;
use crate::intent::{AgentSource, IntentOrigin, UserSource};

fn restored_workspace_index(origin: &IntentOrigin) -> usize {
    let (mut state, mut engine) = crate::state::tests::test_state();
    assert_eq!(state.active_workspace, 0);
    cascade_closed_item_restored(
        &mut state,
        &mut engine,
        origin,
        RestoredKind::Workspace { new_ws_index: 1 },
    );
    state.active_workspace
}

#[test]
fn a_user_restore_moves_the_active_workspace_to_the_restored_one() {
    let user = IntentOrigin::User {
        source: UserSource::Shortcut("restore_closed"),
    };
    assert_eq!(restored_workspace_index(&user), 1);
}

#[test]
fn a_restore_the_user_did_not_raise_leaves_the_active_workspace_alone() {
    let agent = IntentOrigin::Agent {
        source: AgentSource::Ipc,
    };
    assert_eq!(restored_workspace_index(&agent), 0, "agent");
    assert_eq!(restored_workspace_index(&IntentOrigin::System), 0, "system");
}
