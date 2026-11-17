//! 생성 대기 초안이 선택된 동안 기존 대상으로 넘어가던 내장 명령.

use crate::state::tests::{select_pending_draft, test_mirror_state};

/// 초안 선택 중 `close_pane` 은 초안을 닫을 대상으로 보지 않고, 활성 workspace 닫기로 넘어가지도 않는다.
#[test]
fn close_pane_does_not_close_the_workspace_while_a_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    crate::shortcuts::close_pane_action(&mut state, &read);
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.entries.len(), 1);
}

/// 초안 선택 중 `close_active` 도 탭·pane·workspace 어느 것도 닫지 않는다.
#[test]
fn close_active_does_not_close_the_workspace_while_a_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    crate::shortcuts::close_active_action(&mut state, &read);
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.entries.len(), 1);
}

/// 초안이 없으면 `close_pane` 은 지금처럼 닫기 요청을 보낸다. 위 두 시험이 아무 요청도 없는 경로만
/// 재지 않도록 대조한다.
#[test]
fn close_pane_without_a_draft_still_requests_a_close() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    crate::shortcuts::close_pane_action(&mut state, &read);
    crate::shortcuts::close_active_action(&mut state, &read);
    assert!(!state.take_pending_intents().is_empty());
}
