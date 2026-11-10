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
        anchor: Some(5),
        window: None,
        view: None,
        selection: None,
        activate: false,
        mapping: None,
        mapping_token: None,
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
