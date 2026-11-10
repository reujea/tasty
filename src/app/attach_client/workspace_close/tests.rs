use super::*;
use crate::app::attach_client::pending::PendingMirrorInstall;
use crate::app::attach_client::tests::test_session;
use std::collections::HashMap;
use tasty_remote::outbound::Remote;
use tasty_remote::pending_connection::ConnectionTicket;

fn mirror(
    engine: crate::runtime::engine_session::EngineId,
    reconnect: Option<(u32, tasty_remote::connection::ConnectionEpoch)>,
) -> PendingMirrorInstall {
    PendingMirrorInstall {
        engine,
        row: Some(5),
        window: None,
        view: None,
        selection: None,
        activate: false,
        binding: None,
        row_token: None,
        reconnect,
        resync: false,
        run: None,
        user: false,
        manual: None,
        profile: None,
        remote_workspace: None,
    }
}

/// 연결이 살아 있고 서버가 기능을 알린 mirror에서만 닫기를 보낸다. 기다리는 닫기나 닫힌 연결은 다시 보내지 않는다.
#[test]
fn a_close_is_sent_only_over_a_live_capable_idle_connection() {
    let mut session = test_session(1, HashMap::from([(10, 100)]));
    assert_eq!(refusal_of(&session), None);

    session.transport.workspace_close = false;
    assert_eq!(refusal_of(&session), Some(CloseRefusal::Unsupported));
    session.transport.workspace_close = true;

    session.state.workspace_close = WorkspaceClose::Pending(4);
    assert_eq!(refusal_of(&session), Some(CloseRefusal::Pending));
    assert!(close_result_unknown(&session));
    session.state.workspace_close = WorkspaceClose::Closed;
    assert_eq!(refusal_of(&session), Some(CloseRefusal::Pending));
    assert!(!close_result_unknown(&session));
    session.state.workspace_close = WorkspaceClose::Idle;

    session.state.phase = SessionState::Reconnecting;
    assert_eq!(refusal_of(&session), Some(CloseRefusal::Reconnecting));
    session.state.phase = SessionState::Connected;
    session.transport.frame_tx.retire();
    assert_eq!(refusal_of(&session), Some(CloseRefusal::Reconnecting));
}

/// 기다리던 op의 성공 회신만 닫힘으로 바꾼다. 다른 op의 회신이나 실패는 mirror를 닫지 않는다.
#[test]
fn only_the_awaited_success_marks_the_mirror_closed() {
    let mut close = WorkspaceClose::Pending(3);
    close.settle(2, true);
    assert_eq!(close, WorkspaceClose::Pending(3));
    close.settle(3, false);
    assert_eq!(close, WorkspaceClose::Idle);

    let mut close = WorkspaceClose::Pending(3);
    close.settle(3, true);
    assert_eq!(close, WorkspaceClose::Closed);

    let mut idle = WorkspaceClose::Idle;
    idle.settle(3, true);
    assert_eq!(idle, WorkspaceClose::Idle);
}

/// 연결 끊기는 그 mirror의 재연결 endpoint·connection 시도만 취소하고 다른 mirror의 시도는 남긴다.
#[test]
fn disconnect_cancels_only_that_mirrors_reconnect_attempts() {
    let (_, engine) = crate::state::tests::test_state();
    let mut remote = Remote::new();
    let mut attempts = MirrorAttempts::default();
    let (sender, _queue) = tasty_remote::connection::channel();
    let (other_sender, _other_queue) = tasty_remote::connection::channel();

    let own = remote.begin_attempt(Some(5), None).unwrap();
    attempts.register_endpoint(own.clone(), mirror(engine.id, Some((20, sender.epoch()))));
    let other = remote.begin_attempt(Some(6), None).unwrap();
    attempts.register_endpoint(
        other.clone(),
        mirror(engine.id, Some((21, other_sender.epoch()))),
    );
    attempts.register_connection(
        ConnectionTicket(1),
        mirror(engine.id, Some((20, sender.epoch()))),
    );
    attempts.register_connection(
        ConnectionTicket(2),
        mirror(engine.id, Some((21, other_sender.epoch()))),
    );

    assert_eq!(cancel_mirror_attempts(&mut attempts, &mut remote, 20), 2);
    assert!(!own.is_active());
    assert!(other.is_active());
    assert!(!attempts.installing_epoch(20, &sender.epoch()));
    assert!(attempts.installing_epoch(21, &other_sender.epoch()));
    let endpoints = attempts.endpoint_snapshot();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(
        endpoints[0].1.reconnect.as_ref().map(|(id, _)| *id),
        Some(21)
    );

    assert_eq!(cancel_mirror_attempts(&mut attempts, &mut remote, 20), 0);
    remote.finish_attempt(&other);
}

fn saved_row_workspace(id: u32, saved: bool) -> crate::model::Workspace {
    let mut workspace = crate::model::Workspace::new_with_terminal_marker(
        id,
        "remote".into(),
        id + 1,
        id + 2,
        id + 3,
    );
    workspace.mirror = true;
    if saved {
        workspace.remote_row = Some(crate::model::RemoteBinding {
            profile: "gx10".into(),
            remote_workspace: 4,
        });
    }
    workspace
}

/// 저장된 원격 행을 비우는 사용자 닫기만 확인 대상이다. 마지막 탭·pane·surface 닫기는 그 한 줄을 더한다.
#[test]
fn closing_what_empties_a_saved_row_asks_and_a_partial_close_does_not() {
    use crate::app::command::DomainIntent as D;
    let (_, mut session) = crate::state::tests::test_state();
    session
        .core_state
        .push_mirror_workspace(saved_row_workspace(900, true));
    session
        .core_state
        .push_mirror_workspace(saved_row_workspace(950, false));
    let core = &session.core_state;
    let ask = |intent: D| saved_row_close(core, &intent);
    let closed = |last| Some(SavedRowClose { row: 900, last });
    assert_eq!(ask(D::CloseWorkspace { workspace_id: 900 }), closed(false));
    assert_eq!(ask(D::ClosePane { pane_id: 901 }), closed(true));
    assert_eq!(ask(D::CloseTab { tab_id: 902 }), closed(true));
    assert_eq!(
        ask(D::CloseSurface {
            surface_id: 903,
            presentation: None,
        }),
        closed(true)
    );
    // 에이전트 mirror와 로컬 workspace는 묻지 않는다.
    assert_eq!(ask(D::CloseWorkspace { workspace_id: 950 }), None);
    let local = core.local_workspaces()[0].id;
    assert_eq!(
        ask(D::CloseWorkspace {
            workspace_id: local
        }),
        None
    );
}

/// 탭이 둘인 저장 행에서 한 탭을 닫는 것은 workspace를 비우지 않아 묻지 않는다.
#[test]
fn closing_one_of_two_tabs_of_a_saved_row_does_not_ask() {
    use crate::app::command::DomainIntent as D;
    let (_, mut session) = crate::state::tests::test_state();
    let mut workspace = saved_row_workspace(900, true);
    let extra = crate::model::Pane::new_with_terminal_marker(1, 912, 913);
    let tab = extra.tabs.into_iter().next().expect("one tab");
    workspace
        .pane_layout_mut()
        .find_pane_mut(901)
        .expect("the row's pane")
        .tabs
        .push(tab);
    session.core_state.push_mirror_workspace(workspace);
    assert_eq!(
        saved_row_close(&session.core_state, &D::CloseTab { tab_id: 902 }),
        None
    );
}
