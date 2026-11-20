//! 출력 회차의 관측·설치·처리가 강제 끊김을 다루는 순서를 실제 `App` 으로 잰다.
//! 새 프로세스에서 혼자 돈다(`test_rig`).

use super::force_detach_result_tests::{
    ROW_B, assert_stopped, first_connect_target, prepare_an_automatic_reconnect,
    push_other_row_session_unflagged, reconnecting_app,
};
use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use crate::app::App;
use crate::app::attach_client::round_intake::FORCE_DETACHED_BEFORE_INSTALL;
use std::io::Read;
use std::time::{Duration, Instant};
use tasty_remote::client_session::SessionState;
use tasty_remote::pending_connection::ConnectionTicket;
use tasty_remote::transport_end::EndReason;

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

const FORCE_NOTICE: &[u8] = br#"{"event":"force_detached","reason":"force_detach_workspace"}"#;

/// handshake 를 마친 뒤 서버의 강제 끊김 통지를 보내는 상대. 소켓은 돌려준다.
fn forced_peer() -> (u16, std::sync::mpsc::Receiver<std::net::TcpStream>) {
    use std::io::{BufRead, BufReader};
    use tasty_ipc::stream::{self, StreamTag};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(socket.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        for control in [
            serde_json::json!({"ok": true, "proto": 1, "client_id": 7}),
            serde_json::json!({"event": "attached_workspace", "name": "remote", "tree": null, "surfaces": []}),
        ] {
            stream::write_frame(
                &mut socket,
                StreamTag::Control,
                &serde_json::to_vec(&control).unwrap(),
            )
            .unwrap();
        }
        stream::write_frame(&mut socket, StreamTag::Control, FORCE_NOTICE).unwrap();
        tx.send(socket).unwrap();
    });
    (port, rx)
}

fn marked(app: &App) -> bool {
    app.services
        .registries
        .remote_force_detached
        .read(|marks| marks.contains(&binding()))
}

fn session_of(app: &App, row: u32) -> &tasty_remote::client_session::AttachClientSession {
    app.remote
        .sessions
        .iter()
        .find(|session| session.state.saved_row == Some(row))
        .expect("the row session")
}

fn ticket_of(app: &App, row: u32) -> Option<ConnectionTicket> {
    app.state
        .mirror_attempts
        .connection_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(row))
        .map(|(ticket, _)| ticket)
}

fn run_json(run: u64) -> serde_json::Value {
    crate::request_runs::lookup(run, None).expect("the run")
}

/// 행 `ROW_B` 의 재연결 시도를 강제로 끊기는 상대로 큐에 넣는다. run 과 합류 run 도 단다.
fn queue_a_forced_reconnect_of_the_other_row(
    app: &mut App,
) -> (std::sync::mpsc::Receiver<std::net::TcpStream>, u64, u64) {
    push_other_row_session_unflagged(app, SessionState::Reconnecting);
    app.ensure_row_token(ROW_B);
    let index = app
        .remote
        .sessions
        .iter()
        .position(|session| session.state.saved_row == Some(ROW_B))
        .unwrap();
    let run = crate::request_runs::begin("remote.connect", None, serde_json::json!({}));
    let joined = crate::request_runs::begin("remote.connect", None, serde_json::json!({}));
    let mut target = app
        .mirror_install_target(None, Some(ROW_B), Some(index), false)
        .unwrap();
    target.run = Some(crate::request_runs::RunGuard::new(run, "dropped"));
    app.row_waits.join_run(ROW_B, Some(joined));
    let (port, peer) = forced_peer();
    app.queue_mirror_connection(target, port, 3, None).unwrap();
    app.remote.active.insert(ROW_B);
    (peer, run, joined)
}

/// 준비된 `row` 연결의 전송이 강제 끊김으로 확정될 때까지 결과를 거둔다.
fn collect_until_forced(app: &mut App, row: u32) {
    let ticket = ticket_of(app, row).expect("the queued connection");
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        app.remote.collect_connections();
        if app
            .remote
            .prepared_connection(ticket)
            .is_some_and(|prepared| prepared.transport.end.settled() == Some(EndReason::Forced))
        {
            return;
        }
        assert!(Instant::now() < until, "the connection never got forced");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// G1 로 끝난 B 와 그 때문에 버려진 A 를 확인한다. run 과 합류 run 은 같은 실패로 끝난다(S14).
fn assert_b_ended_by_g1_and_a_dropped(app: &mut App, run: u64, joined: u64) {
    assert_stopped(app);
    assert!(marked(app), "the observation marked the target");
    assert_eq!(
        session_of(app, ROW_B).state.phase,
        SessionState::Disconnected
    );
    assert!(!app.remote.active.contains(&ROW_B));
    for id in [run, joined] {
        let json = run_json(id);
        assert_eq!(json["state"], "failed", "{json}");
        assert_eq!(json["result"], FORCE_DETACHED_BEFORE_INSTALL, "{json}");
    }
    // 옛 run 결과는 이후 명시적 연결로 바뀌지 않는다.
    app.connect_saved_row(ROW_B, true);
    for id in [run, joined] {
        assert_eq!(run_json(id)["result"], FORCE_DETACHED_BEFORE_INSTALL);
    }
}

/// S6(a)·S14: Given 행 A 의 자동 재연결이 설치만 남았고 같은 대상의 행 B 의 재연결 연결이 준비된 채
/// 강제 끊김으로 확정됐다, When 회차를 돌리면, Then 관측이 B 를 G1 으로 끝내고(run·합류 run 실패, 행은
/// 끊긴 행) 표지를 써서 A 는 설치 검사가 버린다. 시험 장치에 View 가 없어 막히지 않은 쪽의 설치 성공과
/// 세션 교체는 재지 않는다.
#[test]
fn a_ready_forced_connection_ends_before_install_and_drops_the_other_attempt() {
    alone!("a_ready_forced_connection_ends_before_install_and_drops_the_other_attempt");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    let (_peer, run, joined) = queue_a_forced_reconnect_of_the_other_row(&mut app);
    collect_until_forced(&mut app, ROW_B);

    app.apply_attach_client_output();

    assert_b_ended_by_g1_and_a_dropped(&mut app, run, joined);
}

/// S6(b): Given B 의 결과가 아직 결과 채널에 있다(회차 시작 때 Connecting), When 회차를 돌리면, Then
/// 관측의 첫 단계가 결과를 거둬 Ready 와 같이 G1 으로 끝낸다. 결과 채널의 전송이 강제 끊김으로 확정될
/// 때까지는 상대가 통지를 쓴 뒤 기다려 만든다(전송을 거두지 않고는 확정을 볼 수 없다). 덜 기다렸으면
/// 이 시험은 통과하지 않고 실패한다.
#[test]
fn a_forced_connection_still_in_the_result_channel_ends_before_install() {
    alone!("a_forced_connection_still_in_the_result_channel_ends_before_install");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    let (peer, run, joined) = queue_a_forced_reconnect_of_the_other_row(&mut app);
    let _socket_b = peer.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let ticket = ticket_of(&app, ROW_B).unwrap();
    assert!(
        app.remote.prepared_connection(ticket).is_none(),
        "not collected before the round"
    );

    app.apply_attach_client_output();

    assert_b_ended_by_g1_and_a_dropped(&mut app, run, joined);
}

/// S8: Given B 의 준비된 연결이 강제 끊김으로 확정됐지만 행 토큰이 바뀌어 낡은 시도다, When 회차를
/// 돌리면, Then 표지를 쓰지 않고 G1 이 아닌 낡음 폐기로 끝나며 A 는 버려지지 않는다.
#[test]
fn a_stale_forced_connection_writes_no_mark() {
    alone!("a_stale_forced_connection_writes_no_mark");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    let (_peer, run, _joined) = queue_a_forced_reconnect_of_the_other_row(&mut app);
    collect_until_forced(&mut app, ROW_B);
    rotate_row_token(&mut app, ROW_B);

    app.apply_attach_client_output();

    assert!(!marked(&app), "a stale attempt is no evidence");
    assert!(ticket_of(&app, ROW_B).is_none(), "discarded");
    assert_ne!(run_json(run)["result"], FORCE_DETACHED_BEFORE_INSTALL);
    assert!(app.remote.active.contains(&ROW), "A goes on");
    assert_ne!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
}

fn rotate_row_token(app: &mut App, row: u32) {
    for session in app.engines.all_sessions_mut() {
        if session.core_state.has_workspace(row) {
            session
                .remote
                .row_tokens
                .insert(row, std::sync::Arc::new(()));
        }
    }
}

/// 서버 쪽 소켓이 회수 대기(250ms)보다 먼저 EOF 를 받는지.
fn assert_closed_at_once(socket: &mut std::net::TcpStream) {
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let started = Instant::now();
    let mut buf = [0u8; 4096];
    loop {
        match socket.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                panic!("the socket stayed open: {error}")
            }
            Err(_) => break,
        }
    }
    assert!(started.elapsed() < Duration::from_millis(250));
}

/// 행 A 의 자동 재연결을 준비하고, 관측 뒤에 그 전송을 강제 끊김으로 확정한다.
fn settle_a_after_the_observation(
    app: &mut App,
) -> crate::app::attach_client::round_intake::RoundIntake {
    let intake = app.observe_round();
    let ticket = ticket_of(app, ROW).unwrap();
    let prepared = app.remote.prepared_connection(ticket).unwrap();
    prepared.transport.end.raise(EndReason::Forced);
    assert!(prepared.transport.end.settle());
    intake
}

/// S12(a): Given 사용자가 끊어 막힌 행 A 의 자동 재연결이 설치만 남았다, When 관측 뒤 A 의 전송이 강제
/// 끊김으로 확정되고 같은 회차의 설치 검사가 A 를 버리면, Then 표지는 이번 회차에도 다음 회차에도 서지
/// 않고 서버 쪽 소켓은 바로 닫힌다.
#[test]
fn a_connection_dropped_after_the_observation_writes_no_mark_and_closes_at_once() {
    alone!("a_connection_dropped_after_the_observation_writes_no_mark_and_closes_at_once");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let mut socket = prepare_an_automatic_reconnect(&mut app);
    app.remote.user_disconnected.insert(ROW);
    let intake = settle_a_after_the_observation(&mut app);

    app.finish_round(intake);

    assert!(ticket_of(&app, ROW).is_none(), "dropped");
    assert!(!marked(&app));
    assert_closed_at_once(&mut socket);
    app.apply_attach_client_output();
    assert!(!marked(&app), "nor next round");
}

/// S12(c): Given 행 A 의 자동 재연결이 설치만 남았다, When 관측 뒤 A 의 전송이 강제 끊김으로 확정되고
/// 행 토큰이 바뀌어 같은 회차에 낡음으로 폐기되면, Then 표지는 서지 않고 소켓은 바로 닫힌다.
#[test]
fn a_connection_discarded_as_stale_after_the_observation_writes_no_mark() {
    alone!("a_connection_discarded_as_stale_after_the_observation_writes_no_mark");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let mut socket = prepare_an_automatic_reconnect(&mut app);
    let intake = settle_a_after_the_observation(&mut app);
    rotate_row_token(&mut app, ROW);

    app.finish_round(intake);

    assert!(ticket_of(&app, ROW).is_none(), "discarded");
    assert!(!marked(&app));
    assert_closed_at_once(&mut socket);
    app.apply_attach_client_output();
    assert!(!marked(&app), "nor next round");
}

/// r4 given_up: Given 연결된 행 A 와 같은 대상의 연결된 행 B, When 한 회차에 A 는 평범하게 끊기고 B 는
/// 강제로 끊긴 뒤 재연결 스케줄러가 돌면, Then A 는 실제 재연결 진입(`enter_reconnecting`)의 슬롯을 거쳐
/// 끊긴 행이 되고 슬롯은 given_up 이다.
#[test]
fn a_reconnecting_row_of_a_force_detached_target_gives_up_its_real_slot() {
    alone!("a_reconnecting_row_of_a_force_detached_target_gives_up_its_real_slot");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.ensure_row_token(ROW);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    crate::app::attach_client::tests::settle_transport_end(
        &app.remote.sessions[0],
        EndReason::Lost,
    );
    crate::app::attach_client::tests::settle_transport_end(
        session_of(&app, ROW_B),
        EndReason::Forced,
    );

    app.apply_attach_client_output();
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Reconnecting
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(false),
        "the real reconnect slot"
    );
    assert!(marked(&app));

    app.maybe_trigger_reconnect(Some(ROW), None);

    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_eq!(
        app.remote.reconnect.get(&ROW).map(|slot| slot.given_up),
        Some(true)
    );
}

/// U8-a: Given 명시 연결로 설치된 행 A 가 연결된 동안 같은 대상의 행 B 가 강제로 끊겨 표지가 섰다,
/// When A 가 손실 통지로 재동기화를 시작한 상태(`resync_pending`, Detach 보냄)에서 서버 EOF 가 오면,
/// Then 재attach 하지 않고 끊긴 행으로 두며 새 시도를 등록하지 않는다. 이어서 A 를 명시 재연결하면 그
/// 시도는 재동기화가 아닌 일반 재연결이다. 시험 장치에는 창 View 가 없어 출력 이벤트가 적용되지 않으므로
/// 손실 통지 대신 `begin_resync` 가 남기는 상태를 직접 둔다.
#[test]
fn a_loss_on_an_explicitly_installed_row_under_the_mark_stops() {
    alone!("a_loss_on_an_explicitly_installed_row_under_the_mark_stops");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.ensure_row_token(ROW);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    crate::app::attach_client::tests::settle_transport_end(
        session_of(&app, ROW_B),
        EndReason::Forced,
    );
    app.apply_attach_client_output();
    assert!(app.row_blocked(ROW), "B's force detach blocks A");

    app.remote.sessions[0].state.resync_pending = Some(1);
    crate::app::attach_client::tests::settle_transport_end(
        &app.remote.sessions[0],
        EndReason::Lost,
    );
    app.apply_attach_client_output();

    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert!(app.state.mirror_attempts.connection_snapshot().is_empty());
    assert!(app.state.mirror_attempts.endpoint_snapshot().is_empty());
    assert_eq!(app.remote.sessions[0].state.resync_pending, None);

    app.connect_saved_row(ROW, true);
    let resync = app
        .state
        .mirror_attempts
        .endpoint_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(ROW))
        .map(|(_, target)| target.resync);
    assert_eq!(resync, Some(false), "an ordinary reconnect");
}

/// S10: 출처(자동·명시·팝업) × 행 상태(표지·보류·풀린 행·없음)의 자동 시도 판정.
#[test]
fn the_one_predicate_blocks_only_automatic_attempts_of_blocked_rows() {
    alone!("the_one_predicate_blocks_only_automatic_attempts_of_blocked_rows");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    push_other_row_session_unflagged(&mut app, SessionState::Disconnected);
    let sources = |app: &App| {
        let auto = first_connect_target(app, false);
        let explicit = first_connect_target(app, true);
        let mut user = first_connect_target(app, false);
        user.user = true;
        [
            app.auto_attempt_blocked(&auto),
            app.auto_attempt_blocked(&explicit),
            app.auto_attempt_blocked(&user),
        ]
    };
    assert_eq!(sources(&app), [false, false, false], "none");
    app.mark_force_detached_row(ROW_B);
    assert_eq!(sources(&app), [true, false, false], "mark");
    app.forget_force_detached_row(ROW);
    assert!(!marked(&app));
    assert_eq!(sources(&app), [false, false, false], "released row");
    app.forget_force_detached_row(ROW_B);
    app.remote.held.get_mut(&binding()).unwrap().remove(&ROW);
    assert_eq!(sources(&app), [true, false, false], "hold only");
}

/// S11: 보류는 확정 뒤 명시 연결로 풀린 행만 놓고 같은 대상의 다른 행·나중에 편입된 행은 막는다.
/// 새 표지가 서면 풀린 행도 다시 막히고, 마지막 행을 없애면 보류가 풀린다. 기동 복원(첫 poll 전 명시
/// 연결 포함)은 `row_disconnect_app_tests` 의 재시작 시험이 잰다.
#[test]
fn the_hold_covers_every_row_of_the_target_but_the_released_one() {
    alone!("the_hold_covers_every_row_of_the_target_but_the_released_one");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, None);
    push_other_row_session_unflagged(&mut app, SessionState::Disconnected);
    app.mark_force_detached_row(ROW_B);
    app.forget_force_detached_row(ROW);
    assert!(!app.row_blocked(ROW), "the explicit connect released A");
    assert!(app.row_blocked(ROW_B), "B stays held");

    const ROW_C: u32 = 9_300;
    let mut row_c = crate::model::Workspace::new_with_terminal_marker(
        ROW_C,
        "remote c".to_string(),
        9_301,
        9_302,
        9_303,
    );
    row_c.mirror = true;
    row_c.remote_row = Some(binding());
    let engine = app.saved_remote_row_engine(ROW).unwrap();
    app.engines
        .get_mut(engine)
        .unwrap()
        .push_mirror_workspace(row_c);
    assert!(app.row_blocked(ROW_C), "a row added later is held");

    app.mark_force_detached_row(ROW_B);
    assert!(
        app.row_blocked(ROW),
        "a new mark blocks the released row again"
    );

    // 행을 없앤 뒤의 정리(`remove_unattached_row` 가 부르는 것)는 같은 대상의 행이 남으면 보류를 남긴다.
    for row in [ROW, ROW_B] {
        app.engines
            .get_mut(engine)
            .unwrap()
            .remove_mirror_workspace(row);
        app.forget_removed_row_mark(&binding());
        assert!(app.row_blocked(ROW_C), "a row of the target is left");
    }
    app.engines
        .get_mut(engine)
        .unwrap()
        .remove_mirror_workspace(ROW_C);
    app.forget_removed_row_mark(&binding());
    assert!(
        !app.remote.held.contains_key(&binding()),
        "the last row released the hold"
    );
    assert!(!marked(&app));
}

/// 연결된 행 A 가 손실 통지로 재동기화를 시작한 뒤(`resync_pending`, Detach 보냄) 옛 전송이 `reason`
/// 으로 확정된 상태. 재동기화 연결은 `port` 로 간다.
fn resyncing_app(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    reason: EndReason,
    port: u16,
) -> App {
    let mut app = app_with_saved_row(event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.remote.sessions[0].state.bulk_port = port;
    app.remote.sessions[0].state.resync_pending = Some(1);
    app.ensure_row_token(ROW);
    crate::app::attach_client::tests::settle_transport_end(&app.remote.sessions[0], reason);
    app
}

fn assert_no_attempt(app: &App) {
    assert!(app.state.mirror_attempts.connection_snapshot().is_empty());
    assert!(app.state.mirror_attempts.endpoint_snapshot().is_empty());
}

/// U7-a (a): Given 재동기화 Detach 뒤 옛 소켓의 강제 통지가 먼저 와 `Forced`로 확정됐다, When 회차를
/// 돌리면, Then 표지를 쓰고 새 handshake 없이 끊긴 행이 되며 재동기화 상태가 비워진다. 전송 쪽 순서는
/// `transport::tests::exits::a_notice_after_the_resync_detach_is_still_settled_as_forced` 가 잰다.
#[test]
fn a_force_detach_after_the_resync_detach_stops_the_row() {
    alone!("a_force_detach_after_the_resync_detach_stops_the_row");
    let event_loop = event_loop();
    let mut app = resyncing_app(&event_loop, EndReason::Forced, 1);

    app.apply_attach_client_output();

    assert!(marked(&app));
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_no_attempt(&app);
    assert_eq!(app.remote.sessions[0].state.resync_pending, None);
}

/// U7-a (b): Given 손실 끝으로 재동기화가 실제 `resync_session` 경로로 새 연결을 큐에 넣었다, When 그
/// 연결이 설치 전에 강제로 끊긴 뒤 회차를 돌리면, Then G1 으로 끝나 표지를 쓰고 행은 끊긴 행이 되며
/// 재동기화 상태가 비워진다.
#[test]
fn a_resync_connection_forced_before_install_stops_the_row() {
    alone!("a_resync_connection_forced_before_install_stops_the_row");
    let event_loop = event_loop();
    let (port, _peer) = forced_peer();
    let mut app = resyncing_app(&event_loop, EndReason::Lost, port);
    app.apply_attach_client_output();
    let (_, target) = app
        .state
        .mirror_attempts
        .connection_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(ROW))
        .expect("the resync queued a connection");
    assert!(target.resync, "the resync path");
    collect_until_forced(&mut app, ROW);

    app.apply_attach_client_output();

    assert!(marked(&app));
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Disconnected
    );
    assert_no_attempt(&app);
    let state = &app.remote.sessions[0].state;
    assert_eq!(state.resync_pending, None);
    assert!(!state.resync_awaiting_window);
}

/// U7-a 이어하기 실패: Given 재동기화 연결이 handshake 전에 실패했다, When 회차를 돌리면, Then 저장 행은
/// 재연결 중이 되고(처리 단계가 아니라 실패 처리가 스스로 끝낸다) 표지는 없다.
#[test]
fn a_failed_resync_connection_reconnects_the_saved_row() {
    alone!("a_failed_resync_connection_reconnects_the_saved_row");
    let event_loop = event_loop();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut app = resyncing_app(&event_loop, EndReason::Lost, port);
    app.apply_attach_client_output();
    let ticket = ticket_of(&app, ROW).expect("the resync queued a connection");
    let until = Instant::now() + Duration::from_secs(10);
    while app.remote.connection_error(ticket).is_none() {
        assert!(Instant::now() < until, "the connection never failed");
        std::thread::sleep(Duration::from_millis(10));
        app.remote.collect_connections();
    }

    app.apply_attach_client_output();

    assert!(!marked(&app));
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Reconnecting
    );
}

/// U1: Given 연결된 행의 전송에 사유만 올랐다(큐 거절·ID 실패, 쓰기 스레드가 reader 를 기다리는 중),
/// When 회차를 돌리면, Then 확정 전이라 끝으로 보지 않는다. 확정한 뒤의 회차에 재연결로 들어간다.
#[test]
fn a_raised_but_unsettled_end_waits_for_the_settle() {
    alone!("a_raised_but_unsettled_end_waits_for_the_settle");
    let event_loop = event_loop();
    let mut app = app_with_saved_row(&event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.ensure_row_token(ROW);
    let end = app.remote.sessions[0].transport.end.clone();
    end.raise(EndReason::Lost);

    app.apply_attach_client_output();
    assert_eq!(app.remote.sessions[0].state.phase, SessionState::Connected);
    assert!(app.remote.sessions[0].state.observed_end.is_none());

    assert!(end.settle());
    app.apply_attach_client_output();
    assert_eq!(
        app.remote.sessions[0].state.phase,
        SessionState::Reconnecting
    );
}
