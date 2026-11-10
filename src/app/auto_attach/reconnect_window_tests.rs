//! 재연결 시간 창이 진행 중인 시도에도 적용되는지 실제 `App` 으로 잰다. 새 프로세스에서 혼자 돈다(`test_rig`).

use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use std::time::{Duration, Instant};
use tasty_remote::client_session::SessionState;
use tasty_remote::outbound::ReconnectSlot;
use tasty_remote::pending_connection::ConnectionTicket;

/// Given 재연결 중이며 endpoint 해석과 설치를 기다리는 연결이 진행 중인 행, When 재연결 시간이 지나면,
/// Then 깨울 시각에 그 끝이 들어 있고, 그때 두 시도를 취소하고 끊긴 행이 되며 늦게 온 결과는 낡은 것이다.
#[test]
fn an_expired_window_ends_the_attempt_in_progress() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "an_expired_window_ends_the_attempt_in_progress",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Reconnecting));
    app.ensure_row_token(ROW);
    let window = app.reconnect_window();
    let mut slot = ReconnectSlot::new();
    slot.started = Instant::now() - window - Duration::from_secs(1);
    let deadline = slot.started + window;
    app.remote.reconnect.insert(ROW, slot);
    let token = app
        .remote
        .begin_attempt(Some(ROW), Some(binding()))
        .unwrap();
    let target = app
        .mirror_install_target(None, Some(ROW), Some(0), false)
        .unwrap();
    assert!(app.mirror_install_target_is_current(&target));
    app.state
        .mirror_attempts
        .register_endpoint(token.clone(), target.clone());
    app.state
        .mirror_attempts
        .register_connection(ConnectionTicket(1), target.clone());
    app.remote.active.insert(ROW);

    assert!(
        app.reconnect_wakeups().contains(&(ROW, deadline)),
        "the window end wakes the loop while the attempt runs"
    );
    app.maybe_trigger_reconnect(None, None);

    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert!(!app.remote.active.contains(&ROW));
    assert!(app.state.mirror_attempts.endpoint_snapshot().is_empty());
    assert!(app.state.mirror_attempts.connection_snapshot().is_empty());
    assert!(!token.is_active(), "the endpoint attempt is cancelled");
    assert!(
        !app.mirror_install_target_is_current(&target),
        "a late success of the cancelled attempt is stale"
    );
}
