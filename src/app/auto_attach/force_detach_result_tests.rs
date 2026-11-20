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

/// 행 `ROW` 에 등록된 endpoint 시도의 대상.
fn registered_attempt(app: &App) -> PendingMirrorInstall {
    app.state
        .mirror_attempts
        .endpoint_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(ROW))
        .map(|(_, target)| target)
        .expect("an attempt for the row is registered")
}

/// 시험 장치에는 View 가 없어 첫 연결 대상은 `mirror_install_target_is_current` 를 통과하지 못한다.
/// 그래서 실제 진입점이 등록한 대상에 대해 결과 소비·설치 경로가 부르는 판정을 잰다.
fn drop_after_marking_the_other_row(app: &mut App) -> bool {
    let target = registered_attempt(app);
    force_detach_the_other_row(app);
    app.drop_attempt_for_force_detached_target(&target)
}

/// Given 세션 없는 저장 행, When 그 행이 보이게 된 프레임(edge, 재시작 뒤 첫 프레임 포함)과 그 뒤
/// 프레임에 `maybe_connect_visible_row` 가 첫 연결을 정하면, Then 출처는 자동(explicit=false)이다.
/// 시험 장치에는 View 가 없어 첫 연결은 시도 등록까지 가지 못하므로(`mirror_install_target`) 진입점이
/// `connect_saved_row` 에 넘기는 값을 잰다. 등록된 시도의 값은 아래 재연결 시험이 잰다.
#[test]
fn a_first_connect_for_a_shown_row_is_requested_as_automatic() {
    alone!("a_first_connect_for_a_shown_row_is_requested_as_automatic");
    let event_loop = event_loop();
    let app = app_with_saved_row(&event_loop, None);
    assert_eq!(app.visible_row_connect(Some(ROW), None), Some((ROW, false)));
    assert_eq!(
        app.visible_row_connect(Some(ROW), Some(ROW_B)),
        Some((ROW, false))
    );
    assert_eq!(
        app.visible_row_connect(Some(ROW), Some(ROW)),
        Some((ROW, false))
    );
}

/// Given 끊긴 저장 행, When 그 행으로 다시 옮겨 재연결을 시작한 뒤 같은 대상의 행 B 가 강제로
/// 끊기면, Then 시도는 비명시적이고 그 결과는 버려진다.
#[test]
fn a_reconnect_started_by_reopening_the_row_is_automatic_and_dropped() {
    alone!("a_reconnect_started_by_reopening_the_row_is_automatic_and_dropped");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Disconnected));
    for session in &mut app.remote.sessions {
        session.state.remote_workspace = binding().remote_workspace;
    }
    app.maybe_connect_visible_row(Some(ROW), Some(ROW_B));
    assert!(app.remote.active.contains(&ROW), "the reconnect started");
    assert!(!registered_attempt(&app).explicit);
    assert!(drop_after_marking_the_other_row(&mut app));
    assert_stopped(&app);
}

/// Given 끊긴 저장 행, When 연결 버튼처럼 명시적 연결(`connect_saved_row(.., true)`)로 재연결을 시작한
/// 뒤 같은 대상의 행 B 가 강제로 끊기면, Then 등록된 시도는 명시적이고 결과를 버리지 않는다.
#[test]
fn a_reconnect_started_by_an_explicit_connect_is_kept() {
    alone!("a_reconnect_started_by_an_explicit_connect_is_kept");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Disconnected));
    for session in &mut app.remote.sessions {
        session.state.remote_workspace = binding().remote_workspace;
    }
    app.connect_saved_row(ROW, true);
    assert!(registered_attempt(&app).explicit);
    assert!(!drop_after_marking_the_other_row(&mut app));
    assert!(
        app.remote.active.contains(&ROW),
        "the explicit attempt goes on"
    );
}

/// Given 명시적 연결로 재연결 중인 행 A, When 같은 대상의 행 B 가 강제로 끊긴 뒤 재연결 스케줄러가
/// 돌고 A 의 연결이 준비돼 설치를 poll 하면, Then 스케줄러는 A 의 시도를 거두지 않고 설치 검사도
/// 버리지 않는다. 시험 장치에 View 가 없어 설치를 끝까지 마치는지는 재지 않는다.
#[test]
fn the_scheduler_keeps_an_explicit_reconnect_after_the_target_was_force_detached() {
    alone!("the_scheduler_keeps_an_explicit_reconnect_after_the_target_was_force_detached");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Disconnected));
    for session in &mut app.remote.sessions {
        session.state.remote_workspace = binding().remote_workspace;
    }
    app.connect_saved_row(ROW, true);
    let target = registered_attempt(&app);
    assert!(target.explicit);
    force_detach_the_other_row(&mut app);

    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(
        app.remote.active.contains(&ROW),
        "the attempt is not abandoned"
    );
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Reconnecting
    );
    assert!(registered_attempt(&app).explicit);

    let (port, peer) = ready_peer();
    app.queue_mirror_connection(target, port, 3, None).unwrap();
    let (ticket, _) = app.state.mirror_attempts.connection_snapshot()[0].clone();
    let until = Instant::now() + Duration::from_secs(10);
    while app.remote.prepared_connection(ticket).is_none() {
        assert!(Instant::now() < until, "the connection never got ready");
        std::thread::sleep(Duration::from_millis(10));
        app.remote.collect_connections();
    }
    let _socket = peer.recv_timeout(Duration::from_secs(5)).unwrap();
    app.poll_pending_mirror_installs();
    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert!(app.remote.active.contains(&ROW));
}

/// Given 스케줄러가 시작한 자동 재연결이 진행 중인 행 A, When 명시적 연결이 그 시도에 합류한 뒤 같은
/// 대상의 행 B 가 강제로 끊기면, Then 합류한 시도는 명시적이 되어 스케줄러와 결과 검사가 남긴다.
#[test]
fn an_explicit_connect_joining_an_automatic_reconnect_keeps_it() {
    alone!("an_explicit_connect_joining_an_automatic_reconnect_keeps_it");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(
        app.remote.active.contains(&ROW),
        "the scheduler started an attempt"
    );
    assert!(!registered_attempt(&app).explicit);

    app.connect_saved_row(ROW, true);
    assert!(
        registered_attempt(&app).explicit,
        "the joined attempt is explicit"
    );
    force_detach_the_other_row(&mut app);
    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(
        app.remote.active.contains(&ROW),
        "the attempt is not abandoned"
    );
    assert!(!app.drop_attempt_for_force_detached_target(&registered_attempt(&app)));
}

/// `remote.connect` 요청을 실제 IPC 진입점으로 보내고 접수 응답을 받는다.
fn request_remote_connect(app: &mut App, row: u32) -> serde_json::Value {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: crate::ipc::protocol::JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "remote.connect", "params": { "workspace_id": row },
    }))
    .unwrap();
    let cmd = crate::ipc::server::IpcCommand::new(request, tx);
    let caller = crate::ipc::caller::CallerContext::local();
    app.connect_saved_row_by_request(&cmd, &caller, row);
    serde_json::to_value(rx.try_recv().expect("an immediate response")).unwrap()
}

/// Given 재연결 구간의 자동 재시도가 진행 중, When `remote.connect` 가 그 시도에 합류한 뒤 같은 대상의
/// 다른 행이 강제로 끊기면, Then 합류한 시도는 명시적 출처라 재시도 판정이 거두지 않고 결과도 버리지 않는다.
#[test]
fn a_remote_connect_joining_an_automatic_reconnect_keeps_it() {
    alone!("a_remote_connect_joining_an_automatic_reconnect_keeps_it");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(!registered_attempt(&app).explicit);

    let response = request_remote_connect(&mut app, ROW);
    assert!(response["error"].is_null(), "{response}");
    assert!(
        registered_attempt(&app).explicit,
        "the joined attempt is explicit"
    );
    force_detach_the_other_row(&mut app);
    app.maybe_trigger_reconnect(Some(ROW), None);
    assert!(
        app.remote.active.contains(&ROW),
        "the attempt is not abandoned"
    );
    assert!(!app.drop_attempt_for_force_detached_target(&registered_attempt(&app)));
}

/// 같은 대상의 행 `ROW_B` 와 그 세션을 둔다. 표지는 저장하지 않는다.
fn push_other_row_session(app: &mut App, phase: SessionState) {
    push_other_row_session_unflagged(app, phase);
    flag_the_other_row_force_detached(app);
}

/// reader 가 강제 끊김 통지를 처리한 것처럼 행 `ROW_B` 세션 전송의 `force_detached`·`disconnected` 를
/// 그 순서로 세운다.
fn flag_the_other_row_force_detached(app: &App) {
    let session_b = app
        .remote
        .sessions
        .iter()
        .find(|session| session.state.saved_row == Some(ROW_B))
        .expect("the session of row B");
    session_b
        .transport
        .force_detached
        .store(true, std::sync::atomic::Ordering::SeqCst);
    session_b
        .transport
        .disconnected
        .store(true, std::sync::atomic::Ordering::SeqCst);
}

fn push_other_row_session_unflagged(app: &mut App, phase: SessionState) {
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
    let mut session_b =
        crate::app::attach_client::tests::test_session(ROW_B, std::collections::HashMap::new());
    session_b.state.saved_row = Some(ROW_B);
    session_b.state.remote_workspace = binding().remote_workspace;
    session_b.state.phase = phase;
    app.remote.sessions.push(session_b);
}

/// 재연결 중인 행 A 의 자동 연결을 준비해 설치만 남긴다.
fn prepare_an_automatic_reconnect(app: &mut App) -> std::net::TcpStream {
    let (port, peer) = ready_peer();
    let target = app
        .mirror_install_target(None, Some(ROW), Some(0), false)
        .unwrap();
    app.queue_mirror_connection(target, port, 3, None).unwrap();
    app.remote.active.insert(ROW);
    let (ticket, _) = app.state.mirror_attempts.connection_snapshot()[0].clone();
    let until = Instant::now() + Duration::from_secs(10);
    while app.remote.prepared_connection(ticket).is_none() {
        assert!(Instant::now() < until, "the connection never got ready");
        std::thread::sleep(Duration::from_millis(10));
        app.remote.collect_connections();
    }
    peer.recv_timeout(Duration::from_secs(5)).unwrap()
}

/// Given 행 B 가 강제로 끊겨 끝 분류를 마치고(세션은 Disconnected, 전송 표지는 남음) 저장 표지가 생긴
/// 뒤 사용자가 같은 대상의 행 A 를 명시적으로 연결해 표지를 지웠다, When A 가 평범하게 끊겨 시작한
/// 자동 재연결의 연결이 준비되고 설치를 poll 하면, Then B 의 옛 전송 표지 때문에 버리지 않는다.
#[test]
fn an_old_force_detach_of_another_row_does_not_drop_a_later_automatic_reconnect() {
    alone!("an_old_force_detach_of_another_row_does_not_drop_a_later_automatic_reconnect");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    push_other_row_session(&mut app, SessionState::Disconnected);
    app.mark_force_detached_row(ROW_B);
    assert!(app.row_user_disconnected(ROW));
    // 연결 버튼이 부르는 표지 지우기(`connect_saved_row_with_run` 의 명시적 분기).
    app.forget_force_detached_row(ROW);
    assert!(
        !app.row_user_disconnected(ROW),
        "the explicit connect cleared the mark"
    );
    let _socket = prepare_an_automatic_reconnect(&mut app);

    app.poll_pending_mirror_installs();

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
        "the automatic attempt goes on"
    );
}

/// Given 같은 출력 회차에 행 A 의 재연결 연결이 설치만 남았고 같은 대상의 행 B 세션의 reader 가 강제
/// 끊김 통지를 처리해 전송 표지만 섰다(표지는 아직 저장 전), When 그 회차를 처리하면
/// (`apply_attach_client_output`, 설치가 B 의 끝 분류보다 먼저), Then A 는 설치되지 않고 끊긴 행이 되며
/// B 의 끝 분류가 표지를 저장한다.
#[test]
fn an_install_in_the_round_that_ends_the_other_row_by_force_detach_is_dropped() {
    alone!("an_install_in_the_round_that_ends_the_other_row_by_force_detach_is_dropped");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    push_other_row_session(&mut app, SessionState::Connected);
    assert!(!app.row_user_disconnected(ROW), "no mark is saved yet");

    app.apply_attach_client_output();

    assert_stopped(&app);
    assert!(
        app.row_user_disconnected(ROW),
        "the end of B saved the mark"
    );
}

/// Given 행 A 의 자동 재연결이 설치만 남았고 같은 대상의 행 B 세션은 연결돼 있다, When 출력 회차가
/// 관측값을 찍은 뒤에 B 의 reader 가 강제 끊김 통지를 처리하고(전송 표지가 섬) 그 회차를 처리하면,
/// Then 그 회차의 설치 검사는 A 를 버리지 않고 끝 분류도 B 를 그대로 둔다. 다음 회차에 B 가 강제
/// 끊김으로 분류되어 표지가 저장된다. A 의 설치 뒤에 B 가 끊긴 것과 같은 순서다. 시험 장치에 View 가
/// 없어 A 의 설치를 끝까지 마치는지는 재지 않고, 버리지 않았다는 것만 본다.
#[test]
fn a_force_detach_flagged_after_the_round_observation_ends_the_other_row_next_round() {
    alone!("a_force_detach_flagged_after_the_round_observation_ends_the_other_row_next_round");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    let observation = crate::app::attach_client::round_observation::RoundObservation::observe(
        &app.remote.sessions,
    );
    flag_the_other_row_force_detached(&app);

    app.apply_attach_client_output_observed(observation);

    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected,
        "the install check kept A"
    );
    assert!(app.remote.active.contains(&ROW), "A's attempt goes on");
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(false)
    );
    let phase_b = |app: &App| {
        app.remote
            .sessions
            .iter()
            .find(|session| session.state.saved_row == Some(ROW_B))
            .map(|session| session.state.phase)
    };
    assert_eq!(
        phase_b(&app),
        Some(SessionState::Connected),
        "B ends next round"
    );
    assert!(
        !app.row_user_disconnected(ROW),
        "no mark is saved this round"
    );

    app.apply_attach_client_output();

    assert_eq!(phase_b(&app), Some(SessionState::Disconnected));
    assert!(
        app.row_user_disconnected(ROW),
        "the next round saved the mark"
    );
}
