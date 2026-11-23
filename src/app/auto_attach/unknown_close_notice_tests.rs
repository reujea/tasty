//! 닫기 회신을 기다리던 mirror 의 결과 불명 안내가 요청마다 한 번인지 실제 `App` 으로 잰다. 처리 단계와 사용자
//! 끊기가 같은 기록을 본다. 시험 장치에는 창 View 가 없어 토스트 대신 안내 경로의 호출을 센다
//! (`workspace_close::notice_log`). 새 프로세스에서 혼자 돈다(`test_rig`).

use super::force_detach_round_tests::{resyncing_app, ticket_of};
use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use crate::app::App;
use crate::app::attach_client::workspace_close::notice_log;
use std::collections::HashMap;
use tasty_remote::client_session::{SessionState, WorkspaceClose};
use tasty_remote::transport_end::EndReason;

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

const UNKNOWN: &str = "attach.toast.workspace_close_unknown";

/// 저장되지 않은 mirror 의 로컬 workspace.
pub(super) const AGENT_WS: u32 = 9_500;

/// 아무도 받지 않는 포트. 그리로 가는 연결은 handshake 전에 실패한다.
pub(super) fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn unknown_notices(workspace: u32) -> usize {
    notice_log::count(workspace, UNKNOWN)
}

fn session_index(app: &App, workspace: u32) -> Option<usize> {
    app.remote
        .sessions
        .iter()
        .position(|session| session.state.local_workspace == workspace)
}

/// 창 없는 engine 에 저장되지 않은 mirror workspace `AGENT_WS` 를 두고, 그 세션이 손실 통지로 재동기화를
/// 시작한 뒤(`resync_pending`, Detach 보냄) 옛 전송이 손실로 확정된 `App`. 재동기화 연결은 `port` 로 간다.
/// 돌려주는 값은 mirror workspace 를 가진 parked engine 이다.
pub(super) fn resyncing_agent_mirror(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    port: u16,
) -> (App, crate::runtime::engine_session::EngineId) {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    if let Some(pending) = app.engines.pending_id() {
        app.engines
            .attach_window(winit::window::WindowId::from(u64::MAX - 1), pending);
    }
    let (state, mut engine) = crate::state::tests::test_state();
    let mut mirror = crate::model::Workspace::new_with_terminal_marker(
        AGENT_WS,
        "agent".to_string(),
        9_501,
        9_502,
        9_503,
    );
    mirror.mirror = true;
    engine.borrow_mut().push_mirror_workspace(mirror);
    let parked = app.engines.park_for_test(state, engine);
    let mut session = crate::app::attach_client::tests::test_session(AGENT_WS, HashMap::new());
    session.state.bulk_port = port;
    session.state.resync_pending = Some(1);
    session.state.resync_awaiting_window = false;
    crate::app::attach_client::tests::settle_transport_end(&session, EndReason::Lost);
    app.remote.sessions.push(session);
    (app, parked)
}

/// Given 닫기 회신을 기다리던 연결된 저장 행, When 연결이 끊겨 재연결 중이 된 뒤 사용자가 그 행을 끊으면,
/// Then 결과 불명은 처리 단계에서 한 번만 알리고 닫기 대기는 끝나며 행은 끊긴 행이다.
#[test]
fn a_reconnecting_row_disconnect_does_not_repeat_the_unknown_close() {
    alone!("a_reconnecting_row_disconnect_does_not_repeat_the_unknown_close");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.remote.sessions[0].state.workspace_close = WorkspaceClose::Pending(3);
    app.ensure_row_token(ROW);
    crate::app::attach_client::tests::settle_transport_end(
        &app.remote.sessions[0],
        EndReason::Lost,
    );

    app.apply_attach_client_output();
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Reconnecting
    );
    assert_eq!(unknown_notices(ROW), 1, "told when the link was lost");

    app.disconnect_mirror_workspace(ROW);

    assert_eq!(unknown_notices(ROW), 1, "not told again");
    let state = &app.remote.sessions[0].state;
    assert_eq!(state.workspace_close, WorkspaceClose::Idle);
    assert_eq!(state.phase, SessionState::Disconnected);
}

/// Given 닫기 회신을 기다리던 저장 행이 손실 통지로 재동기화 연결을 기다린다(Connected), When 사용자가 그 행을
/// 끊으면, Then 결과 불명은 처리 단계에서 한 번만 알린다.
#[test]
fn a_resyncing_row_disconnect_does_not_repeat_the_unknown_close() {
    alone!("a_resyncing_row_disconnect_does_not_repeat_the_unknown_close");
    let event_loop = event_loop();
    let mut app = resyncing_app(&event_loop, EndReason::Lost, closed_port());
    app.remote.sessions[0].state.workspace_close = WorkspaceClose::Pending(3);

    app.apply_attach_client_output();
    assert!(ticket_of(&app, ROW).is_some(), "the resync queued");
    assert_eq!(app.remote.sessions[0].state.phase, SessionState::Connected);
    assert_eq!(unknown_notices(ROW), 1);

    app.disconnect_mirror_workspace(ROW);

    assert_eq!(unknown_notices(ROW), 1, "not told again");
    let state = &app.remote.sessions[0].state;
    assert_eq!(state.workspace_close, WorkspaceClose::Idle);
    assert_eq!(state.phase, SessionState::Disconnected);
}

/// Given 닫기 회신을 기다리던 저장되지 않은 mirror 가 재동기화 연결을 기다린다, When 사용자가 끊으면, Then 결과
/// 불명은 처리 단계에서 한 번만 알리고 세션은 정리된다.
#[test]
fn a_resyncing_agent_mirror_disconnect_does_not_repeat_the_unknown_close() {
    alone!("a_resyncing_agent_mirror_disconnect_does_not_repeat_the_unknown_close");
    let event_loop = event_loop();
    let (mut app, _) = resyncing_agent_mirror(&event_loop, closed_port());
    let index = session_index(&app, AGENT_WS).unwrap();
    app.remote.sessions[index].state.workspace_close = WorkspaceClose::Pending(3);

    app.apply_attach_client_output();
    assert!(
        app.state
            .mirror_attempts
            .connection_snapshot()
            .iter()
            .any(|(_, target)| target.resync
                && target.reconnect.as_ref().map(|(ws, _)| *ws) == Some(AGENT_WS)),
        "the resync queued"
    );
    assert_eq!(unknown_notices(AGENT_WS), 1);

    app.disconnect_mirror_workspace(AGENT_WS);

    assert_eq!(unknown_notices(AGENT_WS), 1, "not told again");
    assert!(session_index(&app, AGENT_WS).is_none());
}

/// Given 닫기 회신을 기다리던 연결된 저장 행이고 아직 알리지 않았다, When 사용자가 끊으면, Then 그 자리에서 한 번
/// 알리고 다음 회차는 다시 알리지 않는다.
#[test]
fn a_connected_row_disconnect_tells_the_unknown_close_once() {
    alone!("a_connected_row_disconnect_tells_the_unknown_close_once");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.workspace_close = WorkspaceClose::Pending(3);
    app.ensure_row_token(ROW);

    app.disconnect_mirror_workspace(ROW);
    assert_eq!(unknown_notices(ROW), 1);
    assert_eq!(
        app.remote.sessions[0].state.workspace_close,
        WorkspaceClose::Idle
    );

    app.apply_attach_client_output();
    assert_eq!(unknown_notices(ROW), 1, "the next round does not repeat it");
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
}
