//! 같은 연결 대상의 다른 행이 강제로 끊긴 뒤 도착한 시도 결과를 실제 `App` 으로 잰다.
//! 새 프로세스에서 혼자 돈다(`test_rig`).

use super::reconnect_window_tests::ready_peer;
use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use crate::app::App;
use crate::app::attach_client::pending::PendingMirrorInstall;
use std::time::{Duration, Instant};
use tasty_remote::client_session::SessionState;
use tasty_remote::outbound::{AutoAttachOutcome, ReconnectSlot};

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

/// 같은 연결 대상을 가리키는 두 번째 저장 행.
const ROW_B: u32 = 9_200;

/// 행 `ROW` 와 같은 대상의 행 `ROW_B` 를 같은 창에 두고, `ROW_B` 를 강제로 끊긴 행으로 저장한다.
fn force_detach_the_other_row(app: &mut App) {
    let mut row_b = crate::model::Workspace::new_with_terminal_marker(
        ROW_B,
        "remote b".to_string(),
        9_201,
        9_202,
        9_203,
    );
    row_b.mirror = true;
    row_b.remote_row = Some(binding());
    let engine = app.saved_remote_row_engine(ROW).expect("the row engine");
    app.engines
        .get_mut(engine)
        .expect("the row engine")
        .push_mirror_workspace(row_b);
    app.mark_force_detached_row(ROW_B);
    assert!(app.row_user_disconnected(ROW), "the mark covers the target");
}

/// 재연결 중인 행. 시험 장치의 행 세션 원격 id 를 실제 저장 행처럼 binding 의 것으로 맞춘다.
fn reconnecting_app(event_loop: &winit::event_loop::EventLoop<crate::AppEvent>) -> App {
    let mut app = app_with_saved_row(event_loop, Some(SessionState::Reconnecting));
    for session in &mut app.remote.sessions {
        session.state.remote_workspace = binding().remote_workspace;
    }
    app.ensure_row_token(ROW);
    app.remote.reconnect.insert(ROW, ReconnectSlot::new());
    app
}

fn assert_stopped(app: &App) {
    assert!(
        app.state.mirror_attempts.connection_snapshot().is_empty(),
        "no connection is queued or installed"
    );
    assert!(!app.remote.active.contains(&ROW));
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(true)
    );
}

/// Given 재연결 중인 행 A 의 endpoint 시도가 진행 중, When 같은 대상의 행 B 가 강제로 끊긴 뒤 A 의 결과가
/// 도착하면, Then 그 결과로 연결을 시작하지 않고 A 는 끊긴 행이 된다.
#[test]
fn an_endpoint_result_after_the_target_was_force_detached_starts_no_connection() {
    alone!("an_endpoint_result_after_the_target_was_force_detached_starts_no_connection");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let attempt = app
        .remote
        .begin_attempt(Some(ROW), Some(binding()))
        .unwrap();
    let target = app
        .mirror_install_target(None, Some(ROW), Some(0), false)
        .unwrap();
    app.state
        .mirror_attempts
        .register_endpoint(attempt.clone(), target);
    app.remote.active.insert(ROW);
    force_detach_the_other_row(&mut app);
    // 연결을 받기만 하는 상대. 결과를 버리지 않으면 이 포트로 연결이 큐에 든다.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    app.remote
        .tx
        .send(AutoAttachOutcome {
            attempt,
            row: Some(ROW),
            remote_ws: binding().remote_workspace,
            result: Ok((None, port)),
            is_reconnect: true,
        })
        .unwrap();

    app.drain_auto_attach_results();

    assert_stopped(&app);
    assert!(app.row_user_disconnected(ROW), "the mark stays");
}

/// 행 A 의 연결이 준비돼 설치만 남은 뒤 같은 대상의 행 B 를 강제로 끊고 설치를 poll 한다.
fn poll_an_install_prepared_before_the_mark(explicit: bool) -> App {
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let (port, peer) = ready_peer();
    let target = app
        .mirror_install_target(None, Some(ROW), Some(0), false)
        .unwrap()
        .explicit_connect(explicit);
    app.queue_mirror_connection(target, port, 3, None).unwrap();
    app.remote.active.insert(ROW);
    let (ticket, _) = app.state.mirror_attempts.connection_snapshot()[0].clone();
    let until = Instant::now() + Duration::from_secs(10);
    while app.remote.prepared_connection(ticket).is_none() {
        assert!(Instant::now() < until, "the connection never got ready");
        std::thread::sleep(Duration::from_millis(10));
        app.remote.collect_connections();
    }
    let _socket = peer.recv_timeout(Duration::from_secs(5)).unwrap();
    force_detach_the_other_row(&mut app);

    app.poll_pending_mirror_installs();
    app
}

/// Given 재연결 중인 행 A 의 연결이 설치만 남았다, When 같은 대상의 행 B 가 강제로 끊긴 뒤 설치를 poll
/// 하면, Then 설치하지 않고 A 는 끊긴 행이 된다.
#[test]
fn a_prepared_connection_is_not_installed_after_the_target_was_force_detached() {
    alone!("a_prepared_connection_is_not_installed_after_the_target_was_force_detached");
    let app = poll_an_install_prepared_before_the_mark(false);
    assert_stopped(&app);
}

/// Given 명시적 연결로 시작한 행 A 의 연결이 설치만 남았다, When 같은 대상의 행 B 가 강제로 끊긴 뒤
/// 설치를 poll 하면, Then 사용자가 고른 연결이라 버리지 않고 설치 단계로 넘긴다. 시험 장치는 View 가
/// 없어 설치를 끝까지 마치는지는 재지 않는다. 버리지 않았다는 것(행이 멈추지 않고 시도가 남음)만 본다.
#[test]
fn an_explicit_connection_is_not_dropped_after_the_target_was_force_detached() {
    alone!("an_explicit_connection_is_not_dropped_after_the_target_was_force_detached");
    let app = poll_an_install_prepared_before_the_mark(true);
    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(false)
    );
    assert!(
        app.remote.active.contains(&ROW),
        "the explicit attempt goes on"
    );
}

/// 행 `ROW` 의 첫 연결 대상. 시험에는 View 가 없어 첫 연결 대상은 `mirror_install_target_is_current`
/// 를 통과하지 못한다. 그래서 첫 연결은 결과 소비·설치 경로가 부르는 판정을 직접 잰다.
fn first_connect_target(app: &App, explicit: bool) -> PendingMirrorInstall {
    let engine = app.saved_remote_row_engine(ROW).expect("the row engine");
    let row_token = app
        .engines
        .get(engine)
        .and_then(|session| session.remote.row_tokens.get(&ROW))
        .map(std::sync::Arc::downgrade);
    PendingMirrorInstall {
        engine,
        window: None,
        view: None,
        selection: None,
        activate: false,
        row: Some(ROW),
        binding: Some(binding()),
        row_token,
        reconnect: None,
        resync: false,
        run: None,
        user: false,
        explicit,
        manual: None,
        profile: Some("rv".to_string()),
        remote_workspace: Some(3),
    }
}

/// Given 세션 없이 첫 연결 중인 행 A, When 같은 대상의 행 B 가 강제로 끊긴 뒤 A 의 결과를 판정하면,
/// Then 비명시적 시도는 버리고 진행 중 표지를 지운다. 명시적 연결로 시작한 시도는 그대로 둔다.
#[test]
fn a_first_connect_result_is_dropped_unless_the_connect_was_explicit() {
    alone!("a_first_connect_result_is_dropped_unless_the_connect_was_explicit");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    app.ensure_row_token(ROW);
    app.remote.active.insert(ROW);
    force_detach_the_other_row(&mut app);

    let explicit = first_connect_target(&app, true);
    assert!(!app.drop_attempt_for_force_detached_target(&explicit));
    assert!(
        app.remote.active.contains(&ROW),
        "the explicit attempt goes on"
    );

    let automatic = first_connect_target(&app, false);
    assert!(app.drop_attempt_for_force_detached_target(&automatic));
    assert!(!app.remote.active.contains(&ROW));
    assert_ne!(app.row_state_name(ROW), "connecting");
}
