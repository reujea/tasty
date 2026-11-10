//! 설치하지 못한 mirror 의 실패가 실제 `fail_pending_mirror` 를 지나 행 상태가 되는 것을 잰다.
//! `App` 을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop` 는 프로세스에 하나뿐이라 새 프로세스에서 혼자 돈다.

use super::*;
use crate::app::attach_client::tests::test_session;
use crate::model::RemoteBinding;
use std::collections::HashMap;
use tasty_remote::client_session::SessionState;
use tasty_remote::refusal::RowNoticeKind;
use winit::event_loop::EventLoop;
use winit::platform::x11::EventLoopBuilderExtX11;

const ROW: u32 = 7;
const CHAIN: &str = "workspace attach rejected: mirror_workspace";

fn app_on(event_loop: &EventLoop<crate::AppEvent>) -> App {
    crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy())
}

fn event_loop() -> EventLoop<crate::AppEvent> {
    EventLoop::<crate::AppEvent>::with_user_event()
        .with_x11()
        .with_any_thread(true)
        .build()
        .unwrap()
}

fn binding() -> RemoteBinding {
    RemoteBinding {
        profile: "rv".to_string(),
        remote_workspace: 3,
    }
}

fn target() -> PendingMirrorInstall {
    let (_, engine) = crate::state::tests::test_state();
    PendingMirrorInstall {
        engine: engine.id,
        row: Some(ROW),
        window: None,
        view: None,
        selection: None,
        activate: false,
        binding: Some(binding()),
        row_token: None,
        reconnect: None,
        resync: false,
        run: None,
        user: false,
        manual: None,
        profile: Some("rv".to_string()),
        remote_workspace: Some(3),
    }
}

/// Given 저장 행의 첫 연결, When 서버가 mirror 체인으로 거절하면, Then 행 안내는 MirrorChain 이다.
#[test]
fn a_first_connect_refused_as_a_chain_notes_mirror_chain() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_first_connect_refused_as_a_chain_notes_mirror_chain",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut app = app_on(&event_loop);
    let target = target();
    app.fail_pending_mirror(ConnectionTicket(1), &target, CHAIN.to_string());
    let notice = app
        .remote
        .refusals
        .notice(ROW)
        .expect("a notice for the row");
    assert_eq!(notice.kind, RowNoticeKind::MirrorChain);
}

/// Given 재연결 중인 저장 행, When 재연결이 mirror 체인으로 거절되면, Then 백오프 없이 바로 끊긴 행이 된다.
#[test]
fn a_reconnect_refused_as_a_chain_disconnects_at_once() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_reconnect_refused_as_a_chain_disconnects_at_once",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut app = app_on(&event_loop);
    let mut session = test_session(ROW, HashMap::new());
    session.state.saved_row = Some(ROW);
    session.state.phase = SessionState::Reconnecting;
    let epoch = session.transport.frame_tx.epoch();
    app.remote.sessions.push(session);
    let target = PendingMirrorInstall {
        reconnect: Some((ROW, epoch)),
        ..target()
    };
    app.fail_pending_mirror(ConnectionTicket(1), &target, CHAIN.to_string());
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    let slot = app.remote.reconnect.get(&ROW).expect("a reconnect slot");
    assert!(slot.given_up, "the chain refusal ends the reconnect");
    assert_eq!(slot.attempts, 0, "no backoff attempt is counted");
}
