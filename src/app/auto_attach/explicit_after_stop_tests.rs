//! 미룬 멈춤(`stop_after_events`) 동안 받은 명시적 연결이 그 멈춤 뒤에도 이어지는지 실제 `App` 으로 잰다.
//! 새 프로세스에서 혼자 돈다(`test_rig`).
//!
//! 시험 장치에는 창 id 만 있고 MainView 가 없다. 그래서 세션 출력에 넣은 이벤트는 적용되지 않고 남아
//! (`has_pending`) 처리 단계가 정한 멈춤을 미룬다. 출력을 비우면 다음 회차가 멈춤을 적용한다. 창 id 가
//! 있어 `remote.connect` 는 `window_parked` 로 거절되지 않는다. `remote.connect` 는 저장 행 요청 처리기
//! (`connect_saved_row_by_request`)로 부르며 IPC 디스패치의 게이트는 재지 않는다.

use super::force_detach_result_tests::{
    ROW_B, prepare_an_automatic_reconnect, push_other_row_session_unflagged, reconnecting_app,
};
use super::force_detach_round_tests::{
    PeerGates, collect_until_forced, forced_peer, forced_peer_after, resyncing_app, run_json,
    ticket_of,
};
use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use crate::app::App;
use crate::app::attach_client::round_intake::FORCE_DETACHED_BEFORE_INSTALL;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tasty_remote::client_session::{DeferredStop, MirrorEvent, SessionState, WorkspaceClose};
use tasty_remote::transport_end::EndReason;

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

fn row_index(app: &App) -> usize {
    app.remote
        .sessions
        .iter()
        .position(|session| session.state.saved_row == Some(ROW))
        .expect("the row session")
}

fn phase(app: &App) -> SessionState {
    app.remote.sessions[row_index(app)].state.phase
}

/// 행 세션의 멈춤과 그 연결 의사. 멈춤이 없으면 None.
fn stop(app: &App) -> Option<bool> {
    app.remote.sessions[row_index(app)]
        .state
        .stop_after_events
        .as_ref()
        .map(|stop| stop.connect_after)
}

/// 이벤트 적용 단계가 적용하지 못하는 이벤트를 행 세션 출력에 넣어 처리 단계가 멈춤을 미루게 한다.
fn hold_output(app: &mut App, event: MirrorEvent) {
    let index = row_index(app);
    assert!(app.remote.sessions[index].transport.output.push(event));
}

fn release_output(app: &mut App) {
    let index = row_index(app);
    app.remote.sessions[index].transport.output.drain();
    assert!(!app.remote.sessions[index].transport.output.has_pending());
}

/// 행의 endpoint 시도마다 (명시적 출처, 재동기화 여부).
fn endpoint_attempts(app: &App) -> Vec<(bool, bool)> {
    app.state
        .mirror_attempts
        .endpoint_snapshot()
        .into_iter()
        .filter(|(_, target)| target.row == Some(ROW))
        .map(|(_, target)| (target.explicit, target.resync))
        .collect()
}

fn assert_no_attempt(app: &App) {
    assert!(endpoint_attempts(app).is_empty(), "no endpoint attempt");
    assert!(
        ticket_of(app, ROW).is_none(),
        "no connection is queued for the row"
    );
}

fn slot_given_up(app: &App) -> Option<bool> {
    app.remote.reconnect.get(&ROW).map(|slot| slot.given_up)
}

fn slot_started(app: &App) -> Instant {
    app.remote.reconnect.get(&ROW).expect("the slot").started
}

/// 행 세션의 지금 전송에 연결 의사 없는 새 멈춤을 정한다.
fn decide_a_stop_on_the_current_transport(app: &mut App) {
    let index = row_index(app);
    let session = &mut app.remote.sessions[index];
    session.state.stop_after_events = Some(DeferredStop::new(session.transport.frame_tx.epoch()));
}

fn run_state(run: u64) -> String {
    run_json(run)["state"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// `remote.connect` 를 저장 행 요청 처리기로 보내고 접수 응답의 run 을 돌려준다.
fn request_remote_connect(app: &mut App) -> u64 {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let request: crate::ipc::protocol::JsonRpcRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "remote.connect", "params": { "workspace_id": ROW },
    }))
    .unwrap();
    let cmd = crate::ipc::server::IpcCommand::new(request, tx);
    let caller = crate::ipc::caller::CallerContext::local();
    app.connect_saved_row_by_request(&cmd, &caller, ROW);
    let response = serde_json::to_value(rx.try_recv().expect("an immediate response")).unwrap();
    assert!(response["error"].is_null(), "{response}");
    response["result"]["run"].as_u64().expect("the run")
}

/// 행의 진행 중 endpoint 시도가 실제 결과(시험 장치의 프로필 `rv` 없음 → 일반 실패)로 끝날 때까지 거둔다.
fn wait_for_the_endpoint_result(app: &mut App) {
    let until = Instant::now() + Duration::from_secs(10);
    while !endpoint_attempts(app).is_empty() {
        assert!(Instant::now() < until, "the endpoint attempt never ended");
        std::thread::sleep(Duration::from_millis(10));
        app.drain_auto_attach_results();
    }
}

/// 행의 재연결 슬롯이 재연결 시간을 넘긴 것으로 둔다. 전역 재연결 시간은 그대로다.
fn expire_the_slot(app: &mut App) {
    let window = app.reconnect_window();
    app.remote
        .reconnect
        .get_mut(&ROW)
        .expect("the slot")
        .started = Instant::now() - window - Duration::from_secs(1);
}

/// 같은 대상의 행 B 표지로 막힌 연결된 행 A 가 손실 재동기화를 시작한 뒤 옛 전송이 Lost 로 끝났고, 출력에
/// `event` 가 남아 처리 단계가 막힌 Resync 를 지금 전송의 멈춤으로 미룬 상태.
fn blocked_resync_deferred(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
    event: MirrorEvent,
    close: WorkspaceClose,
) -> App {
    let mut app = app_with_saved_row(event_loop, Some(SessionState::Connected));
    app.remote.sessions[0].state.remote_workspace = binding().remote_workspace;
    app.ensure_row_token(ROW);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    app.mark_force_detached_row(ROW_B);
    assert!(app.row_blocked(ROW), "B's mark blocks A");
    {
        let state = &mut app.remote.sessions[0].state;
        state.resync_pending = Some(1);
        state.resync_awaiting_window = false;
        state.workspace_close = close;
    }
    crate::app::attach_client::tests::settle_transport_end(
        &app.remote.sessions[0],
        EndReason::Lost,
    );
    hold_output(&mut app, event);

    app.apply_attach_client_output();

    assert_eq!(stop(&app), Some(false), "the blocked resync became a stop");
    assert_eq!(phase(&app), SessionState::Connected, "and it waits");
    assert_eq!(app.row_state_name(ROW), "reconnecting");
    app
}

fn held() -> MirrorEvent {
    MirrorEvent::StructuralSucceeded(99, None, 0)
}

/// 재연결 중인 행 A 의 자동 연결이 설치 전에 강제 끊김으로 확정됐고(G1) 출력이 남아 멈춤을 미룬 상태.
fn g1_deferred(event_loop: &winit::event_loop::EventLoop<crate::AppEvent>) -> App {
    let mut app = reconnecting_app(event_loop);
    // 상대 소켓은 이 회차가 끝날 때까지 연다. 먼저 닫으면 전송이 손실로 끝나 확정 순서가 갈린다.
    let socket = prepare_an_automatic_reconnect(&mut app);
    let ticket = ticket_of(&app, ROW).unwrap();
    let prepared = app.remote.prepared_connection(ticket).unwrap();
    prepared.transport.end.raise(EndReason::Forced);
    assert!(prepared.transport.end.settle());
    hold_output(&mut app, held());

    app.apply_attach_client_output();

    assert!(ticket_of(&app, ROW).is_none(), "G1 ended the attempt");
    assert_eq!(stop(&app), Some(false));
    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert!(!app.remote.active.contains(&ROW));
    drop(socket);
    app
}

/// 완료 조건 1: Given 막힌 저장 행의 Resync 가 지금 전송의 멈춤으로 미뤄졌다, When 그 사이 `remote.connect`
/// 가 오고 다음 회차가 멈춤을 적용하면, Then 행은 새 명시적 재연결 시도를 가지며 합류한 run 은 진행 중이다.
#[test]
fn an_explicit_connect_during_a_deferred_blocked_resync_starts_after_the_stop() {
    alone!("an_explicit_connect_during_a_deferred_blocked_resync_starts_after_the_stop");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);

    let run = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true), "the connect left its intent");
    assert!(!app.row_blocked(ROW), "the connect cleared the marks");
    assert_no_attempt(&app);

    release_output(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert_eq!(stop(&app), None);
    assert_eq!(
        endpoint_attempts(&app),
        vec![(true, false)],
        "an explicit ordinary reconnect"
    );
    assert!(app.remote.active.contains(&ROW));
    assert_eq!(slot_given_up(&app), Some(false));
    assert_eq!(run_state(run), "running", "{}", run_json(run));
}

/// Given 의사를 남긴 뒤 멈춤을 적용하기 전에 같은 대상의 다른 행이 다시 표지를 세웠다, When 멈춤을 적용하면,
/// Then 새 시도는 명시적 출처로 시작되고 다시 선 표지는 지워지지 않는다.
#[test]
fn a_mark_raised_again_before_the_stop_stays_and_the_explicit_attempt_starts() {
    alone!("a_mark_raised_again_before_the_stop_stays_and_the_explicit_attempt_starts");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);
    request_remote_connect(&mut app);
    app.mark_force_detached_row(ROW_B);
    assert!(app.row_blocked(ROW));

    release_output(&mut app);
    app.apply_attach_client_output();

    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert!(app.remote.active.contains(&ROW));
    assert!(
        app.row_blocked(ROW),
        "the stop step does not clear the mark"
    );
}

/// 완료 조건 2: Given 같은 상황에서 명시적 연결이 없다, When 다음 회차가 멈춤을 적용하면, Then 지금처럼
/// 끊긴 행이고 새 시도는 없다.
#[test]
fn a_deferred_blocked_resync_without_an_explicit_connect_stops_the_row() {
    alone!("a_deferred_blocked_resync_without_an_explicit_connect_stops_the_row");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);

    release_output(&mut app);
    app.apply_attach_client_output();

    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_no_attempt(&app);
    assert!(!app.remote.active.contains(&ROW));
    assert_eq!(slot_given_up(&app), Some(true));
}

/// 완료 조건 3·8: Given G1 로 재연결 중인 세션의 멈춤이 미뤄졌다, When 그 사이 명시적 연결(시도의 run1, 합류
/// run2)이 오고 멈춤이 시도 설치보다 먼저 적용되면, Then 명시적 시도·진행 표지·재연결 슬롯이 남고 두 run 은
/// 진행 중이다. 시도의 실제 결과(일반 실패)가 run1 을 끝내고, 재연결 시간이 지나 끊긴 행이 되면 run2 도
/// 끝난다. 어느 run 도 멈춤 때문에 끝나지 않는다.
#[test]
fn a_g1_stop_applied_first_keeps_the_explicit_attempt_and_its_runs() {
    alone!("a_g1_stop_applied_first_keeps_the_explicit_attempt_and_its_runs");
    let event_loop = event_loop();
    let mut app = g1_deferred(&event_loop);
    let run1 = request_remote_connect(&mut app);
    let run2 = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    let started = slot_started(&app);

    release_output(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(stop(&app), None, "the stop was applied");
    assert_eq!(phase(&app), SessionState::Reconnecting, "not a stopped row");
    assert!(app.remote.active.contains(&ROW), "the progress mark stays");
    assert_eq!(slot_given_up(&app), Some(false), "the slot is not given up");
    assert_eq!(slot_started(&app), started, "the same reconnect window");
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert_eq!(run_state(run1), "running", "{}", run_json(run1));
    assert_eq!(run_state(run2), "running", "{}", run_json(run2));

    wait_for_the_endpoint_result(&mut app);
    app.advance_row_waits();
    let ended = run_json(run1);
    assert_eq!(ended["state"], "failed", "{ended}");
    assert_ne!(ended["result"], FORCE_DETACHED_BEFORE_INSTALL, "{ended}");
    assert_eq!(slot_given_up(&app), Some(false), "an ordinary failure");
    assert_eq!(run_state(run2), "running", "{}", run_json(run2));

    expire_the_slot(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();
    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_eq!(run_state(run2), "failed", "{}", run_json(run2));
}

/// Given G1 멈춤을 미룬 동안 명시적 연결이 의사를 남겼다, When 멈춤을 적용하기 전에 재연결 시간이 지나 행이
/// 끊긴 행이 되면, Then 시도의 run 은 만료로 취소되고 합류 run 은 실패하며 의사는 버려진다. 이어 멈춤을
/// 적용해도 새 연결을 시작하지 않는다.
#[test]
fn an_expiry_before_the_stop_drops_the_intent() {
    alone!("an_expiry_before_the_stop_drops_the_intent");
    let event_loop = event_loop();
    let mut app = g1_deferred(&event_loop);
    let run1 = request_remote_connect(&mut app);
    let run2 = request_remote_connect(&mut app);

    expire_the_slot(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(phase(&app), SessionState::Disconnected);
    let cancelled = run_json(run1);
    assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
    assert_eq!(
        cancelled["result"], "the reconnect window ran out",
        "{cancelled}"
    );
    assert_eq!(run_state(run2), "failed", "{}", run_json(run2));
    assert_eq!(stop(&app), Some(false), "the stop stays without the intent");

    release_output(&mut app);
    app.apply_attach_client_output();
    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_no_attempt(&app);
}

/// Given G1 멈춤을 미룬 동안 명시적 시도가 일반 실패로 끝났고 같은 행의 자동 재연결 연결이 handshake 응답을
/// 기다린다, When 멈춤을 적용하면, Then 그 회차 안에서 자동 연결을 거두고 새 명시적 시도를 시작한다. 다음
/// 회차에도 새 시도의 진행 표지와 슬롯이 남고 합류 run 은 진행 중이다.
#[test]
fn the_stop_takes_back_a_waiting_automatic_connection_before_the_explicit_restart() {
    alone!("the_stop_takes_back_a_waiting_automatic_connection_before_the_explicit_restart");
    let event_loop = event_loop();
    let mut app = g1_deferred(&event_loop);
    let run1 = request_remote_connect(&mut app);
    let run2 = request_remote_connect(&mut app);
    wait_for_the_endpoint_result(&mut app);
    assert_eq!(run_state(run1), "failed", "{}", run_json(run1));
    assert!(!app.remote.active.contains(&ROW));

    let (gate, gate_rx) = std::sync::mpsc::channel();
    let (port, _peer) = forced_peer_after(PeerGates {
        handshake: Some(gate_rx),
        notice: None,
    });
    let index = row_index(&app);
    let target = app
        .mirror_install_target(None, Some(ROW), Some(index), false)
        .unwrap();
    app.queue_mirror_connection(target, port, 3, None).unwrap();
    app.remote.active.insert(ROW);
    let automatic = ticket_of(&app, ROW).expect("the automatic connection");
    release_output(&mut app);

    let intake = app.observe_round();
    let (_, target) = intake
        .tickets
        .iter()
        .find(|(ticket, _)| *ticket == automatic)
        .expect("the observation took the automatic connection");
    assert!(!target.explicit && !target.user, "an automatic attempt");
    assert!(app.mirror_install_target_is_current(target));
    assert!(app.remote.prepared_connection(automatic).is_none());
    assert!(app.remote.connection_error(automatic).is_none());
    app.finish_round(intake);

    assert!(
        app.state
            .mirror_attempts
            .connection_snapshot()
            .iter()
            .all(|(ticket, _)| *ticket != automatic),
        "the stop step took the automatic connection back"
    );
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert!(app.remote.active.contains(&ROW));

    app.apply_attach_client_output();
    app.advance_row_waits();
    assert!(app.remote.active.contains(&ROW));
    assert_eq!(slot_given_up(&app), Some(false));
    assert_eq!(run_state(run2), "running", "{}", run_json(run2));
    // 거둔 연결이라 handshake 를 보내지 않는다. 보내는 쪽을 버려 상대를 끝낸다.
    drop(gate);
}

/// 완료 조건 4: Given G1 멈춤에 명시적 연결 의사가 남았다, When 재연결 설치가 새 전송을 넘기면
/// (`adopt_reconnected_transport`, 성공 설치가 부르는 자리), Then 처리 단계 전에 새 전송과 멈춤 없음이 보이고
/// 다음 두 회차는 멈춤을 적용하지 않는다. 행은 재연결 중이고 명시적 시도·진행 표지·같은 슬롯이 남으며 새
/// 시도는 없다. 이어 그 새 전송에 의사 없는 멈춤을 정해 적용하면 옛 의사로 시도를 이어 두지 않고 끊긴 행이 된다.
///
/// 재지 않는 것: 성공 설치의 나머지(Connected 전이, `active`·시도 정리)와 그 뒤 연결 유지. 설치는 mirror 를
/// 창의 MainView 에서 바꾸는데 시험 장치에는 MainView 가 없어 App 경로의 성공 설치를 끝까지 돌릴 수 없다.
#[test]
fn an_install_before_the_stop_ends_the_stop_and_its_intent() {
    alone!("an_install_before_the_stop_ends_the_stop_and_its_intent");
    let event_loop = event_loop();
    let mut app = g1_deferred(&event_loop);
    request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    let started = slot_started(&app);
    let index = row_index(&app);
    let old = app.remote.sessions[index].transport.frame_tx.epoch();
    let fresh = crate::app::attach_client::tests::test_session(ROW, HashMap::new()).transport;

    crate::app::attach_client::adopt_reconnected_transport(
        &mut app.remote.sessions[index],
        tasty_remote::client_session::InstallGuard::new(fresh),
    );

    let session = &app.remote.sessions[index];
    let epoch = session.transport.frame_tx.epoch();
    assert!(!epoch.same(&old), "a new transport");
    assert!(session.state.stop_after_events.is_none());
    for round in 0..2 {
        app.apply_attach_client_output();
        app.advance_row_waits();
        assert_eq!(stop(&app), None, "round {round}");
        assert_eq!(phase(&app), SessionState::Reconnecting, "round {round}");
        assert!(app.remote.active.contains(&ROW), "round {round}");
        assert_eq!(slot_given_up(&app), Some(false), "round {round}");
        assert_eq!(slot_started(&app), started, "round {round}: the same slot");
        assert_eq!(
            endpoint_attempts(&app),
            vec![(true, false)],
            "round {round}: the explicit attempt only"
        );
        let index = row_index(&app);
        assert!(
            app.remote.sessions[index]
                .transport
                .frame_tx
                .epoch()
                .same(&epoch),
            "round {round}: the adopted transport stays"
        );
    }

    decide_a_stop_on_the_current_transport(&mut app);
    app.apply_attach_client_output();
    assert_eq!(stop(&app), None);
    assert_eq!(
        phase(&app),
        SessionState::Disconnected,
        "the new stop carries no intent"
    );
    assert_eq!(slot_given_up(&app), Some(true));
}

/// 완료 조건 5 (run 없음): Given D4 로 멈춤이 기록되고 `active` 가 남았다, When run 없는 명시적 연결
/// (`SavedRowAction::Connect`)이 오면, Then 멈춤 적용 뒤 새 명시적 연결이 시작된다.
#[test]
fn a_d4_stop_then_a_runless_explicit_connect_starts_a_new_connection() {
    alone!("a_d4_stop_then_a_runless_explicit_connect_starts_a_new_connection");
    let event_loop = event_loop();
    let mut app = d4_deferred(&event_loop);

    app.apply_saved_row_action(ROW, crate::intent::SavedRowAction::Connect);
    assert_eq!(stop(&app), Some(true));
    release_output(&mut app);
    app.apply_attach_client_output();

    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert!(app.remote.active.contains(&ROW));
    assert!(!app.row_blocked(ROW));
}

/// 완료 조건 5 (run 있음): Given D4 로 멈춤이 기록되고 `active` 가 남았다, When `remote.connect` 가 진행
/// 표지에 합류하면, Then 멈춤 적용 뒤 새 연결이 시작되고 run 은 진행 중으로 남았다가 그 연결의 결과로 끝난다.
#[test]
fn a_d4_stop_then_a_remote_connect_ends_its_run_with_the_new_connection() {
    alone!("a_d4_stop_then_a_remote_connect_ends_its_run_with_the_new_connection");
    let event_loop = event_loop();
    let mut app = d4_deferred(&event_loop);

    let run = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    release_output(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert_eq!(run_state(run), "running", "{}", run_json(run));
    wait_for_the_endpoint_result(&mut app);
    app.advance_row_waits();
    assert_eq!(run_state(run), "running", "the reconnect window goes on");
    expire_the_slot(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();
    assert_eq!(run_state(run), "failed", "{}", run_json(run));
}

/// 사용자가 끊어 막힌 재연결 중인 행 A 의 자동 연결이 설치만 남았고 출력이 남아, 설치 단계(D4)가 정한
/// 멈춤이 미뤄졌다. 행의 진행 표지는 남는다.
fn d4_deferred(event_loop: &winit::event_loop::EventLoop<crate::AppEvent>) -> App {
    let mut app = reconnecting_app(event_loop);
    // 상대 소켓은 이 회차가 끝날 때까지 연다. 먼저 닫으면 전송이 손실로 끝나 확정 순서가 갈린다.
    let socket = prepare_an_automatic_reconnect(&mut app);
    app.remote.user_disconnected.insert(ROW);
    hold_output(&mut app, held());

    app.apply_attach_client_output();

    assert!(ticket_of(&app, ROW).is_none(), "D4 dropped the attempt");
    assert_eq!(stop(&app), Some(false));
    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert!(app.remote.active.contains(&ROW), "the progress mark stays");
    drop(socket);
    app
}

/// 행의 engine 을 park 한다. 창 id 만 있는 장치의 정리 경로는 parked engine 에서 행을 지운다.
fn park_the_rows(app: &mut App) {
    let (state, _) = crate::state::tests::test_state();
    app.engines
        .park(winit::window::WindowId::from(u64::MAX - 2), state)
        .expect("the rows' window is parked");
}

/// 완료 조건 6: Given 미룬 멈춤과 명시적 연결 의사가 있는 회차에 서버 닫기 성공 회신이 함께 오면, When
/// 처리하면, Then 서버 닫기로 정리되고 새 연결은 시작되지 않는다.
#[test]
fn a_close_reply_in_the_stop_round_wins_over_the_intent() {
    alone!("a_close_reply_in_the_stop_round_wins_over_the_intent");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(
        &event_loop,
        MirrorEvent::StructuralSucceeded(3, None, 0),
        WorkspaceClose::Pending(3),
    );
    let run = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    park_the_rows(&mut app);

    app.apply_attach_client_output();
    app.advance_row_waits();

    assert!(
        app.remote
            .sessions
            .iter()
            .all(|session| session.state.saved_row != Some(ROW)),
        "the server close ended the session"
    );
    assert!(app.saved_row_binding(ROW).is_none(), "and removed the row");
    assert_no_attempt(&app);
    let ended = run_json(run);
    assert_eq!(ended["state"], "failed", "{ended}");
    assert_eq!(
        ended["result"], "the saved remote workspace row was closed",
        "{ended}"
    );
}

/// 행 세션의 전송을 새 것으로 바꾸고 그 전송에 새 멈춤을 정한다.
fn decide_a_stop_on_a_new_transport(app: &mut App) {
    let index = row_index(app);
    let fresh = crate::app::attach_client::tests::test_session(ROW, HashMap::new()).transport;
    let session = &mut app.remote.sessions[index];
    session.transport = fresh;
    session.state.stop_after_events = Some(DeferredStop::new(session.transport.frame_tx.epoch()));
}

/// 완료 조건 7 (사용자 끊기): Given 의사가 남은 멈춤이 사용자 끊기로 사라졌다, When 그 뒤 다음 전송에 새
/// 멈춤이 정해져 적용되면, Then 옛 의사로 연결을 시작하지 않는다.
#[test]
fn a_user_disconnect_ends_the_intent_with_its_stop() {
    alone!("a_user_disconnect_ends_the_intent_with_its_stop");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);
    request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));

    app.disconnect_mirror_workspace(ROW);
    assert_eq!(stop(&app), None, "the stop ended");
    decide_a_stop_on_a_new_transport(&mut app);
    app.apply_attach_client_output();

    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_no_attempt(&app);
}

/// Given 끊긴 행 세션에 의사를 실은 지금 전송의 멈춤이 남았다, When 사용자가 그 행을 끊으면
/// (`AlreadyDisconnected`), Then 멈춤과 그 전송은 그대로 두고 의사만 버린다. 다음 회차는 새 연결을 시작하지
/// 않는다.
#[test]
fn a_disconnect_of_a_stopped_row_drops_only_the_intent() {
    alone!("a_disconnect_of_a_stopped_row_drops_only_the_intent");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);
    release_output(&mut app);
    app.apply_attach_client_output();
    assert_eq!(phase(&app), SessionState::Disconnected);
    let index = row_index(&app);
    let epoch = app.remote.sessions[index].transport.frame_tx.epoch();
    let mut planted = DeferredStop::new(epoch.clone());
    planted.connect_after = true;
    app.remote.sessions[index].state.stop_after_events = Some(planted);

    app.disconnect_mirror_workspace(ROW);

    let left = app.remote.sessions[index]
        .state
        .stop_after_events
        .as_ref()
        .expect("the stop stays");
    assert!(left.is_for(&epoch), "on the same transport");
    assert!(!left.connect_after, "without the intent");
    app.apply_attach_client_output();
    assert_no_attempt(&app);
}

/// 완료 조건 7 (로컬 닫기): Given 의사가 남은 멈춤이 로컬 닫기로 세션과 함께 사라졌다, When 같은 id 의 저장
/// 행과 새 전송에 새 멈춤이 정해져 적용되면, Then 옛 의사로 연결을 시작하지 않는다.
#[test]
fn a_local_close_ends_the_intent_with_the_session() {
    alone!("a_local_close_ends_the_intent_with_the_session");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);
    request_remote_connect(&mut app);
    let engine = app.saved_remote_row_engine(ROW).expect("the row engine");
    park_the_rows(&mut app);

    app.apply_saved_row_action(ROW, crate::intent::SavedRowAction::CloseLocally);
    assert!(app.saved_row_binding(ROW).is_none(), "the row is gone");
    app.apply_attach_client_output();
    assert_no_attempt(&app);

    let mut row = crate::model::Workspace::new_with_terminal_marker(
        ROW,
        "remote".to_string(),
        9_001,
        9_002,
        9_003,
    );
    row.mirror = true;
    row.remote_row = Some(binding());
    app.engines
        .get_mut(engine)
        .expect("the parked engine")
        .push_mirror_workspace(row);
    let mut session = crate::app::attach_client::tests::test_session(ROW, HashMap::new());
    session.state.saved_row = Some(ROW);
    session.state.remote_workspace = binding().remote_workspace;
    session.state.stop_after_events = Some(DeferredStop::new(session.transport.frame_tx.epoch()));
    app.remote.sessions.push(session);
    app.apply_attach_client_output();

    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_no_attempt(&app);
}

/// 완료 조건 7 (재연결 설치 결과): Given 의사가 남은 멈춤이 있는 채 세션 전송만 새 것으로 바뀌었다, When
/// 회차를 돌리면, Then 처리 단계가 옛 전송의 멈춤을 버리고 연결을 시작하지 않으며, 세션은 새 전송으로 다음
/// 회차까지 연결된 채 남는다. 이어 새 전송에 새 멈춤을 정해 적용해도 옛 의사로 연결하지 않고 끊긴 행이 된다.
///
/// 전송은 직접 바꾼다. 이 시험이 재는 것은 옛 epoch 의 멈춤을 버리는 처리 단계다. 실제 재연결 설치는 MainView
/// 가 없어 돌리지 않는다(위 완료 조건 4 시험).
#[test]
fn a_stop_of_an_older_transport_never_uses_its_intent() {
    alone!("a_stop_of_an_older_transport_never_uses_its_intent");
    let event_loop = event_loop();
    let mut app = blocked_resync_deferred(&event_loop, held(), WorkspaceClose::Idle);
    request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    let index = row_index(&app);
    let fresh = crate::app::attach_client::tests::test_session(ROW, HashMap::new()).transport;
    let epoch = fresh.frame_tx.epoch();
    app.remote.sessions[index].transport = fresh;

    for round in 0..2 {
        app.apply_attach_client_output();
        assert_eq!(
            stop(&app),
            None,
            "round {round}: the older stop was dropped"
        );
        assert_eq!(phase(&app), SessionState::Connected, "round {round}");
        assert!(!app.remote.active.contains(&ROW), "round {round}");
        assert_no_attempt(&app);
        let index = row_index(&app);
        assert!(
            app.remote.sessions[index]
                .transport
                .frame_tx
                .epoch()
                .same(&epoch),
            "round {round}: the new transport stays"
        );
    }

    decide_a_stop_on_the_current_transport(&mut app);
    app.apply_attach_client_output();
    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_no_attempt(&app);
}

/// U1: Given G1 멈춤을 미룬 동안 재연결 시간이 지나 행이 끊긴 행이 됐다(멈춤은 남음), When `remote.connect`
/// 가 새 재연결 구간을 시작하고 합류 run 이 붙은 뒤 멈춤이 적용되면, Then 새 시도·진행 표지·새 슬롯이 남고
/// 두 run 은 진행 중이다.
#[test]
fn an_explicit_connect_after_the_window_ran_out_survives_the_stop() {
    alone!("an_explicit_connect_after_the_window_ran_out_survives_the_stop");
    let event_loop = event_loop();
    let mut app = g1_deferred(&event_loop);
    expire_the_slot(&mut app);
    app.apply_attach_client_output();
    assert_eq!(phase(&app), SessionState::Disconnected);
    assert_eq!(stop(&app), Some(false));
    let old_started = app.remote.reconnect.get(&ROW).unwrap().started;

    let run1 = request_remote_connect(&mut app);
    let run2 = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    release_output(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert!(app.remote.active.contains(&ROW));
    let slot = app.remote.reconnect.get(&ROW).expect("the new slot");
    assert!(!slot.given_up);
    assert_ne!(slot.started, old_started, "a new reconnect window");
    assert_eq!(run_state(run1), "running", "{}", run_json(run1));
    assert_eq!(run_state(run2), "running", "{}", run_json(run2));
}

/// Given 막히지 않은 손실의 재동기화 연결이 설치 전에 강제로 끊겨(G1) 연결된 세션의 멈춤이 미뤄졌다, When
/// `remote.connect` 가 오면(Connected·링크 끝 분기), Then 멈춤 적용 뒤 일반 재연결의 명시적 시도가 시작되고
/// run 은 진행 중이다.
#[test]
fn an_explicit_connect_after_a_forced_resync_connection_starts_after_the_stop() {
    alone!("an_explicit_connect_after_a_forced_resync_connection_starts_after_the_stop");
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
    hold_output(&mut app, held());

    app.apply_attach_client_output();
    assert_eq!(stop(&app), Some(false), "G1 decided a stop");
    assert_eq!(phase(&app), SessionState::Connected, "and it waits");
    assert!(!app.remote.active.contains(&ROW));
    assert_eq!(app.row_state_name(ROW), "reconnecting");

    let run = request_remote_connect(&mut app);
    assert_eq!(stop(&app), Some(true));
    release_output(&mut app);
    app.apply_attach_client_output();
    app.advance_row_waits();

    assert_eq!(phase(&app), SessionState::Reconnecting);
    assert_eq!(endpoint_attempts(&app), vec![(true, false)]);
    assert!(app.remote.active.contains(&ROW));
    assert_eq!(run_state(run), "running", "{}", run_json(run));
}
