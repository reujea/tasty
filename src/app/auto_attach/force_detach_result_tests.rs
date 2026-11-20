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

const OUTSIDE_ROUND: crate::app::attach_client::saved_row::DropSite =
    crate::app::attach_client::saved_row::DropSite::OutsideRound;

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

/// 행 A 의 연결이 준비돼 설치만 남은 뒤 같은 대상의 행 B 를 강제로 끊고 출력 회차 하나를 돈다.
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

    app.apply_attach_client_output();
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

/// 진입점이 띄운 endpoint worker 의 결과를 받아, 같은 시도 토큰으로 준비된 상대 포트를 가리키는 결과로
/// 바꿔 결과 소비(`drain_auto_attach_results` → `apply_auto_attach_outcome`)에 넣는다. 시험 장치에는
/// 프로필이 없어 worker 는 실패를 보고하고, 실패 처리는 행을 끊긴 행으로 만들 수 있어 버림과 구분되지
/// 않는다. 그래서 결과만 바꾸고 소비 경로는 실제 함수를 그대로 지난다.
fn deliver_a_ready_endpoint(app: &mut App) -> std::sync::mpsc::Receiver<std::net::TcpStream> {
    let (port, peer) = ready_peer();
    let mut outcome = app
        .remote
        .rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the endpoint worker reports");
    assert_eq!(outcome.row, Some(ROW));
    outcome.result = Ok((None, port));
    app.remote.tx.send(outcome).unwrap();
    app.drain_auto_attach_results();
    peer
}

/// 결과 소비가 큐에 넣은 행 `ROW` 의 연결이 준비되기를 기다려 설치 검사(`poll_pending_mirror_installs`)
/// 를 지나게 하고, 버려지지 않았는지 본다. 시험 장치에 View 가 없어 설치를 끝까지 마치는지는 재지
/// 않는다. 큐에 든 연결의 출처는 돌려준다.
fn assert_kept_through_the_install_check(
    app: &mut App,
    peer: std::sync::mpsc::Receiver<std::net::TcpStream>,
) -> bool {
    let (ticket, target) = app
        .state
        .mirror_attempts
        .connection_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(ROW))
        .expect("the result queued a connection");
    let until = Instant::now() + Duration::from_secs(10);
    while app.remote.prepared_connection(ticket).is_none() {
        assert!(Instant::now() < until, "the connection never got ready");
        std::thread::sleep(Duration::from_millis(10));
        app.remote.collect_connections();
    }
    let _socket = peer.recv_timeout(Duration::from_secs(5)).unwrap();
    app.run_install_stage();
    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(false)
    );
    assert!(app.remote.active.contains(&ROW), "the attempt goes on");
    target.explicit
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

/// 행 `ROW` 의 첫 연결 대상. 시험 장치에는 View 가 없어 `start_first_connect` 가 대상을 만들지
/// 못한다(`mirror_install_target`). 그래서 진입점이 정한 출처로 같은 모양의 대상을 만든다.
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
/// Then 보이게 된 행의 진입점이 정한 출처(자동)의 시도는 버리고 진행 중 표지를 지워 연결 중으로 보이지
/// 않는다. 명시적 연결의 시도는 그대로 둔다. 첫 연결은 시도 등록과 결과 전달까지 가지 못하므로
/// 결과 소비·설치 경로가 부르는 판정(`drop_attempt_for_force_detached_target`)을 직접 잰다.
#[test]
fn a_first_connect_result_is_dropped_unless_the_connect_was_explicit() {
    alone!("a_first_connect_result_is_dropped_unless_the_connect_was_explicit");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    let (row, explicit) = app
        .visible_row_connect(Some(ROW), None)
        .expect("a shown row starts its first connect");
    assert_eq!(row, ROW);
    app.ensure_row_token(ROW);
    app.remote.active.insert(ROW);
    force_detach_the_other_row(&mut app);

    let user_chosen = first_connect_target(&app, true);
    assert!(!app.drop_attempt_for_force_detached_target(&user_chosen, OUTSIDE_ROUND));
    assert!(
        app.remote.active.contains(&ROW),
        "the explicit attempt goes on"
    );

    let shown = first_connect_target(&app, explicit);
    assert!(app.drop_attempt_for_force_detached_target(&shown, OUTSIDE_ROUND));
    assert!(!app.remote.active.contains(&ROW));
    assert_ne!(app.row_state_name(ROW), "connecting");
}

/// Given 끊긴 저장 행, When 그 행으로 다시 옮겨 재연결을 시작한 뒤 같은 대상의 행 B 가 강제로
/// 끊기고 그 시도의 endpoint 결과가 도착하면, Then 시도는 비명시적이고 결과 소비가 버려 연결을 큐에
/// 넣지 않는다.
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
    force_detach_the_other_row(&mut app);
    let _peer = deliver_a_ready_endpoint(&mut app);
    assert_stopped(&app);
}

/// Given 끊긴 저장 행, When 연결 버튼처럼 명시적 연결(`connect_saved_row(.., true)`)로 재연결을 시작한
/// 뒤 같은 대상의 행 B 가 강제로 끊기고 endpoint 결과가 도착하면, Then 등록된 시도는 명시적이고 결과
/// 소비가 연결을 큐에 넣으며(출처 유지) 설치 검사도 버리지 않는다.
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
    force_detach_the_other_row(&mut app);
    let peer = deliver_a_ready_endpoint(&mut app);
    assert!(assert_kept_through_the_install_check(&mut app, peer));
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
    app.run_install_stage();
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
    let peer = deliver_a_ready_endpoint(&mut app);
    assert!(assert_kept_through_the_install_check(&mut app, peer));
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
    let peer = deliver_a_ready_endpoint(&mut app);
    assert!(assert_kept_through_the_install_check(&mut app, peer));
}

/// 같은 대상의 행 `ROW_B` 와 그 세션을 둔다. 표지는 저장하지 않는다.
fn push_other_row_session(app: &mut App, phase: SessionState) {
    push_other_row_session_unflagged(app, phase);
    flag_the_other_row_force_detached(app);
}

/// reader 가 강제 끊김 통지를 처리한 것처럼 행 `ROW_B` 세션 전송의 끝을 `Forced`로 확정한다.
fn flag_the_other_row_force_detached(app: &App) {
    let session_b = app
        .remote
        .sessions
        .iter()
        .find(|session| session.state.saved_row == Some(ROW_B))
        .expect("the session of row B");
    crate::app::attach_client::tests::settle_transport_end(
        session_b,
        tasty_remote::transport_end::EndReason::Forced,
    );
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

    app.run_install_stage();

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

/// Given 같은 출력 회차에 행 A 의 재연결 연결이 설치만 남았고 같은 대상의 행 B 세션의 전송이 강제
/// 끊김으로 확정됐다(표지는 아직 저장 전), When 그 회차를 처리하면(`apply_attach_client_output`, 관측이
/// 설치보다 먼저), Then 관측 단계가 표지를 저장하고 A 는 설치되지 않고 끊긴 행이 된다.
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
        "the observation of B saved the mark"
    );
}

/// Given 행 A 의 자동 재연결이 설치만 남았고 같은 대상의 행 B 세션은 연결돼 있다, When 출력 회차의
/// 관측 단계가 끝난 뒤에 B 의 전송이 강제 끊김으로 확정되고 그 회차의 나머지를 처리하면, Then 그
/// 회차의 설치 검사는 A 를 버리지 않고 처리도 B 를 그대로 둔다. 다음 회차의 관측이 B 의 끝을 기록해
/// 표지를 저장하고 처리가 B 를 멈춘다. A 의 설치 뒤에 B 가 끊긴 것과 같은 순서다. 시험 장치에 View 가
/// 없어 A 의 설치를 끝까지 마치는지는 재지 않고, 버리지 않았다는 것만 본다.
#[test]
fn a_force_detach_flagged_after_the_round_observation_ends_the_other_row_next_round() {
    alone!("a_force_detach_flagged_after_the_round_observation_ends_the_other_row_next_round");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    let intake = app.observe_round();
    flag_the_other_row_force_detached(&app);

    app.finish_round(intake);

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

/// Given 행 A 의 자동 재연결이 endpoint 를 지나 연결 대기 단계에 있다, When 명시적 연결이 그 시도에
/// 합류한 뒤 같은 대상의 행 B 가 강제로 끊기고 설치를 poll 하면, Then 연결 대기 대상도 명시적으로
/// 올라가 설치 검사가 버리지 않는다.
#[test]
fn an_explicit_connect_joining_at_the_connection_stage_keeps_it() {
    alone!("an_explicit_connect_joining_at_the_connection_stage_keeps_it");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    let explicit_of_the_connection = |app: &App| {
        app.state
            .mirror_attempts
            .connection_snapshot()
            .into_iter()
            .find(|(_, target)| target.row == Some(ROW))
            .map(|(_, target)| target.explicit)
    };
    assert_eq!(explicit_of_the_connection(&app), Some(false));

    app.connect_saved_row(ROW, true);
    assert_eq!(
        explicit_of_the_connection(&app),
        Some(true),
        "the waiting connection is explicit"
    );
    force_detach_the_other_row(&mut app);
    app.run_install_stage();

    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert!(app.remote.active.contains(&ROW), "the attempt goes on");
}
