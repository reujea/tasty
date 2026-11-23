//! 재동기화 연결 시도가 설치 단계에서 낡음으로 버려졌을 때 옛 세션이 실제 `App` 의 처리 단계에서 끝나는지 잰다.
//! 저장 행은 재연결 중이 되고 에이전트 mirror 는 정리된다. 같은 묶음의 서버 닫기 회신, 이벤트 유예, 그 행의
//! 다른 시도와 재연결 구간을 지키는지도 본다. 새 프로세스에서 혼자 돈다(`test_rig`).

use super::force_detach_result_tests::{prepare_an_automatic_reconnect, reconnecting_app};
use super::force_detach_round_tests::{resyncing_app, rotate_row_token, ticket_of};
use super::test_rig::{ROW, binding, event_loop};
use super::unknown_close_notice_tests::{AGENT_WS, closed_port, resyncing_agent_mirror};
use crate::app::App;
use crate::app::attach_client::pending::PendingMirrorInstall;
use std::time::{Duration, Instant};
use tasty_remote::client_session::{MirrorEvent, SessionState, WorkspaceClose};
use tasty_remote::outbound::ReconnectSlot;
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

fn row_session(app: &App) -> Option<&tasty_remote::client_session::AttachClientSession> {
    app.remote
        .sessions
        .iter()
        .find(|session| session.state.saved_row == Some(ROW))
}

/// 첫 회차가 옛 전송을 가리키는 재동기화 연결을 큐에 넣었는지 확인하고 그 시도를 돌려준다.
fn queued_resync(app: &App, workspace: u32) -> (ConnectionTicket, PendingMirrorInstall) {
    let epoch = app
        .remote
        .sessions
        .iter()
        .find(|session| session.state.local_workspace == workspace)
        .expect("the session")
        .transport
        .frame_tx
        .epoch();
    app.state
        .mirror_attempts
        .connection_snapshot()
        .into_iter()
        .find(|(_, target)| {
            target.resync
                && target
                    .reconnect
                    .as_ref()
                    .is_some_and(|(ws, old)| *ws == workspace && old.same(&epoch))
        })
        .expect("the resync queued a connection for the old transport")
}

fn ticket_registered(app: &App, ticket: ConnectionTicket) -> bool {
    app.state
        .mirror_attempts
        .connection_snapshot()
        .iter()
        .any(|(registered, _)| *registered == ticket)
}

/// 손실로 재동기화를 시작한 저장 행. 첫 회차가 재동기화 연결을 큐에 넣는다.
fn resync_queued_row(
    event_loop: &winit::event_loop::EventLoop<crate::AppEvent>,
) -> (App, ConnectionTicket) {
    let mut app = resyncing_app(event_loop, EndReason::Lost, closed_port());
    app.apply_attach_client_output();
    let (ticket, _) = queued_resync(&app, ROW);
    (app, ticket)
}

/// 행 engine 을 park 한다. 창 id 만 있는 시험 장치에서는 parked engine 에만 이벤트가 적용된다.
fn park_the_row(app: &mut App) {
    let (state, _) = crate::state::tests::test_state();
    app.engines
        .park(winit::window::WindowId::from(u64::MAX - 2), state)
        .expect("the row window is parked");
}

fn hold_output(app: &mut App, event: MirrorEvent) {
    let index = app
        .remote
        .sessions
        .iter()
        .position(|session| session.state.saved_row == Some(ROW))
        .unwrap();
    assert!(app.remote.sessions[index].transport.output.push(event));
}

/// 설치 단계가 남긴 낡음 기록.
fn recorded(state: &tasty_remote::client_session::ClientSessionState) -> bool {
    state
        .observed_end
        .as_ref()
        .is_some_and(|end| end.resync_discarded)
}

fn assert_reconnecting_with_a_slot(app: &App) {
    let state = &row_session(app).expect("the row session").state;
    assert_eq!(state.phase, SessionState::Reconnecting);
    assert_eq!(state.resync_pending, None);
    assert!(!recorded(state));
    assert!(app.remote.reconnect.contains_key(&ROW), "a reconnect slot");
}

/// 완료 조건 1: Given 재동기화 시도가 큐에 있는 저장 행, When 그 시도가 설치 단계에서 낡음으로 버려지면, Then
/// 같은 회차에 재동기화가 끝나 행은 재연결 중이고 재연결 슬롯이 있다.
#[test]
fn a_discarded_resync_of_a_saved_row_reconnects_it() {
    alone!("a_discarded_resync_of_a_saved_row_reconnects_it");
    let event_loop = event_loop();
    let (mut app, ticket) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);

    app.apply_attach_client_output();

    assert!(!ticket_registered(&app, ticket), "discarded");
    assert_reconnecting_with_a_slot(&app);
    assert!(!app.remote.active.contains(&ROW));
}

/// 완료 조건 2: Given 재동기화 시도가 큐에 있는 에이전트 mirror, When 그 workspace 가 다른 engine 으로 옮겨
/// 시도가 낡음으로 버려지면, Then 세션이 빠지고 mirror workspace 가 정리된다.
#[test]
fn a_discarded_resync_of_an_agent_mirror_cleans_it_up() {
    alone!("a_discarded_resync_of_an_agent_mirror_cleans_it_up");
    let event_loop = event_loop();
    let (mut app, first) = resyncing_agent_mirror(&event_loop, closed_port());
    app.apply_attach_client_output();
    let (ticket, target) = queued_resync(&app, AGENT_WS);
    assert_eq!(target.engine, first, "the attempt targets the first engine");
    let (state, mut other) = crate::state::tests::test_state();
    let moved = app
        .engines
        .get_mut(first)
        .unwrap()
        .remove_mirror_workspace(AGENT_WS)
        .expect("the mirror workspace");
    other.borrow_mut().push_mirror_workspace(moved);
    app.engines.park_for_test(state, other);

    app.apply_attach_client_output();

    assert!(!ticket_registered(&app, ticket), "discarded");
    assert!(
        app.remote
            .sessions
            .iter()
            .all(|session| session.state.local_workspace != AGENT_WS),
        "the session ended"
    );
    assert!(
        app.engines
            .all_sessions()
            .all(|engine| !engine.core_state.has_workspace(AGENT_WS)),
        "the mirror workspace was cleaned up"
    );
}

/// 완료 조건 3: Given 재동기화가 아닌 자동 재연결 시도, When 낡음으로 버려지면, Then 세션 단계와 슬롯은 그대로다.
#[test]
fn a_discarded_plain_reconnect_keeps_the_session_and_slot() {
    alone!("a_discarded_plain_reconnect_keeps_the_session_and_slot");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    assert!(ticket_of(&app, ROW).is_some());
    let started = app.remote.reconnect[&ROW].started;
    rotate_row_token(&mut app, ROW);

    app.apply_attach_client_output();

    assert!(ticket_of(&app, ROW).is_none(), "discarded");
    let state = &row_session(&app).unwrap().state;
    assert_eq!(state.phase, SessionState::Reconnecting);
    assert!(!recorded(state));
    assert_eq!(app.remote.reconnect[&ROW].started, started, "the same slot");
}

/// 완료 조건 4(같은 회차): Given 낡음으로 버려지는 재동기화 세션의 같은 묶음에 서버 닫기 성공 회신이 있다, When
/// 처리하면, Then 회신이 적용되어 서버 닫기로 정리되고 재연결하지 않는다.
#[test]
fn a_close_reply_in_the_discard_round_closes_the_row() {
    alone!("a_close_reply_in_the_discard_round_closes_the_row");
    let event_loop = event_loop();
    let (mut app, ticket) = resync_queued_row(&event_loop);
    park_the_row(&mut app);
    rotate_row_token(&mut app, ROW);
    app.remote.sessions[0].state.workspace_close = WorkspaceClose::Unanswered(3);
    hold_output(&mut app, MirrorEvent::StructuralSucceeded(3, None, 0));

    app.apply_attach_client_output();

    assert!(!ticket_registered(&app, ticket));
    assert!(row_session(&app).is_none(), "the server close ended it");
    assert!(app.saved_row_binding(ROW).is_none(), "and removed the row");
    assert!(!app.remote.reconnect.contains_key(&ROW), "no reconnect");
}

/// Given 낡음으로 버린 회차에 닫기 회신일 수 있는 이벤트가 적용되지 못하고 남았다, When 그 회차와 이벤트가 적용되는
/// 다음 회차를 돌리면, Then 첫 회차는 끝내지 않고 기록만 남긴다. 회신이 아니면 다음 회차에 재연결 중이 된다.
#[test]
fn a_discard_waits_for_held_events_then_reconnects() {
    alone!("a_discard_waits_for_held_events_then_reconnects");
    let event_loop = event_loop();
    let (mut app, ticket) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);
    hold_output(&mut app, MirrorEvent::StructuralSucceeded(99, None, 0));

    app.apply_attach_client_output();

    assert!(!ticket_registered(&app, ticket), "discarded");
    let state = &row_session(&app).unwrap().state;
    assert_eq!(state.phase, SessionState::Connected, "waits for the events");
    assert!(recorded(state), "the record stays");
    assert!(state.resync_pending.is_some());

    park_the_row(&mut app);
    app.apply_attach_client_output();

    assert_reconnecting_with_a_slot(&app);
}

/// 완료 조건 4(다음 회차): Given 낡음으로 버린 회차에 서버 닫기 성공 회신이 적용되지 못하고 남았다, When 그 회신이
/// 적용되는 다음 회차를 돌리면, Then 서버 닫기로 정리되고 재연결하지 않는다.
#[test]
fn a_held_close_reply_after_the_discard_closes_the_row() {
    alone!("a_held_close_reply_after_the_discard_closes_the_row");
    let event_loop = event_loop();
    let (mut app, _) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);
    app.remote.sessions[0].state.workspace_close = WorkspaceClose::Unanswered(3);
    hold_output(&mut app, MirrorEvent::StructuralSucceeded(3, None, 0));

    app.apply_attach_client_output();
    assert_eq!(
        row_session(&app).unwrap().state.phase,
        SessionState::Connected
    );

    park_the_row(&mut app);
    app.apply_attach_client_output();

    assert!(row_session(&app).is_none(), "the server close ended it");
    assert!(app.saved_row_binding(ROW).is_none());
    assert!(!app.remote.reconnect.contains_key(&ROW), "no reconnect");
}

/// Given 낡음 기록이 이벤트 유예로 남은 저장 행, When 사용자가 그 행을 끊은 뒤 회차가 돌면, Then 끊긴 행으로 남고
/// 재연결 슬롯은 포기 상태다.
#[test]
fn a_user_disconnect_ends_a_held_discard() {
    alone!("a_user_disconnect_ends_a_held_discard");
    let event_loop = event_loop();
    let (mut app, _) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);
    hold_output(&mut app, MirrorEvent::StructuralSucceeded(99, None, 0));
    app.apply_attach_client_output();
    assert!(recorded(&row_session(&app).unwrap().state));

    app.disconnect_mirror_workspace(ROW);
    park_the_row(&mut app);
    app.apply_attach_client_output();

    let state = &row_session(&app).unwrap().state;
    assert_eq!(state.phase, SessionState::Disconnected);
    assert!(!recorded(state));
    assert!(app.remote.reconnect[&ROW].given_up, "still given up");
}

/// 그 행의 유효한 다른 endpoint 시도를 실제 시도 토큰으로 등록한다. 표적은 지금 행 토큰·binding 을 쓰는 같은
/// 세션의 재연결이다.
fn register_a_live_endpoint(
    app: &mut App,
    old: &PendingMirrorInstall,
) -> tasty_remote::outbound::AttemptToken {
    let engine = app.saved_remote_row_engine(ROW).unwrap();
    let (row_binding, row_token) = {
        let owner = app.engines.get(engine).unwrap();
        let binding = owner
            .find_workspace_index_for_id(ROW)
            .and_then(|index| owner.workspace_at(index))
            .and_then(|workspace| workspace.remote_row.clone());
        let token = owner
            .remote
            .row_tokens
            .get(&ROW)
            .map(std::sync::Arc::downgrade);
        (binding, token)
    };
    let target = PendingMirrorInstall {
        resync: false,
        binding: row_binding.clone(),
        row_token,
        ..old.clone()
    };
    assert!(app.mirror_install_target_is_current(&target));
    let token = app.remote.begin_attempt(Some(ROW), row_binding).unwrap();
    app.state
        .mirror_attempts
        .register_endpoint(token.clone(), target);
    token
}

fn endpoint_registered(app: &App, token: &tasty_remote::outbound::AttemptToken) -> bool {
    app.state
        .mirror_attempts
        .endpoint_snapshot()
        .iter()
        .any(|(registered, _)| registered == token)
}

/// 완료 조건 5: Given 행 토큰이 바뀌어 그 행에 유효한 다른 시도가 있고 진행 표지·슬롯은 없다, When 낡은 재동기화
/// 세션을 끝내면, Then 그 시도는 취소되지 않고 진행 표지와 재연결 슬롯이 갖춰져 스케줄러도 새 시도를 시작하지 않는다.
#[test]
fn a_discard_keeps_the_rows_live_attempt() {
    alone!("a_discard_keeps_the_rows_live_attempt");
    let event_loop = event_loop();
    let (mut app, ticket) = resync_queued_row(&event_loop);
    let (_, old) = queued_resync(&app, ROW);
    rotate_row_token(&mut app, ROW);
    let token = register_a_live_endpoint(&mut app, &old);
    assert!(!app.remote.active.contains(&ROW));
    assert!(!app.remote.reconnect.contains_key(&ROW));

    app.apply_attach_client_output();

    assert!(
        !ticket_registered(&app, ticket),
        "the stale resync was discarded"
    );
    assert_reconnecting_with_a_slot(&app);
    assert!(
        app.remote.active.contains(&ROW),
        "the live attempt holds the row"
    );
    app.maybe_trigger_reconnect(None, None);
    assert!(token.is_active(), "the scheduler did not replace it");
    assert!(endpoint_registered(&app, &token));
}

/// 완료 조건 5(행 토큰 그대로): Given 행 binding 이 바뀌어 재동기화 시도가 낡았고 새 binding 의 유효한 시도가 진행
/// 표지와 슬롯을 가졌다, When 이벤트 유예로 완료가 다음 회차로 미뤄지고 그 회차에 끝나면, Then 두 회차 내내 그
/// 시도와 진행 표지, 슬롯이 그대로다.
#[test]
fn a_discard_by_a_binding_change_keeps_the_rows_attempt_and_slot() {
    alone!("a_discard_by_a_binding_change_keeps_the_rows_attempt_and_slot");
    let event_loop = event_loop();
    let (mut app, ticket) = resync_queued_row(&event_loop);
    let (_, old) = queued_resync(&app, ROW);
    let engine = app.saved_remote_row_engine(ROW).unwrap();
    app.engines
        .get_mut(engine)
        .unwrap()
        .mirror_workspace_mut(ROW)
        .unwrap()
        .remote_row = Some(crate::model::RemoteBinding {
        remote_workspace: binding().remote_workspace + 1,
        ..binding()
    });
    let token = register_a_live_endpoint(&mut app, &old);
    app.remote.active.insert(ROW);
    let mut slot = ReconnectSlot::new();
    slot.attempts = 2;
    let (started, next) = (slot.started, slot.next_attempt);
    app.remote.reconnect.insert(ROW, slot);
    hold_output(&mut app, MirrorEvent::StructuralSucceeded(99, None, 0));

    app.apply_attach_client_output();

    assert!(
        !ticket_registered(&app, ticket),
        "the stale resync was discarded"
    );
    assert_eq!(
        row_session(&app).unwrap().state.phase,
        SessionState::Connected
    );
    assert!(
        app.remote.active.contains(&ROW),
        "the install stage kept the mark"
    );

    park_the_row(&mut app);
    app.apply_attach_client_output();

    assert_reconnecting_with_a_slot(&app);
    assert!(app.remote.active.contains(&ROW));
    assert!(token.is_active());
    assert!(endpoint_registered(&app, &token));
    let slot = &app.remote.reconnect[&ROW];
    assert_eq!(
        (slot.started, slot.next_attempt, slot.attempts),
        (started, next, 2)
    );
}

/// 대조: Given 그 행에 살아 있는 시도 없이 진행 표지만 남았다, When 낡은 재동기화 세션을 끝내면, Then 표지를 거두고
/// 새 재연결 구간을 연다.
#[test]
fn a_discard_drops_an_orphaned_mark() {
    alone!("a_discard_drops_an_orphaned_mark");
    let event_loop = event_loop();
    let (mut app, _) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);
    app.remote.active.insert(ROW);

    app.apply_attach_client_output();

    assert_reconnecting_with_a_slot(&app);
    assert!(
        !app.remote.active.contains(&ROW),
        "the orphaned mark is gone"
    );
}

/// 대조: Given 살아 있는 시도는 없고 다른 시도의 실패가 정한 슬롯(백오프)이 있다, When 낡은 재동기화 세션을 끝내면,
/// Then 그 슬롯을 그대로 이어 간다.
#[test]
fn a_discard_keeps_the_backoff_of_a_failed_attempt() {
    alone!("a_discard_keeps_the_backoff_of_a_failed_attempt");
    let event_loop = event_loop();
    let (mut app, _) = resync_queued_row(&event_loop);
    rotate_row_token(&mut app, ROW);
    let mut slot = ReconnectSlot::new();
    slot.attempts = 2;
    slot.next_attempt = Instant::now() + Duration::from_secs(30);
    let (started, next) = (slot.started, slot.next_attempt);
    app.remote.reconnect.insert(ROW, slot);

    app.apply_attach_client_output();

    assert_reconnecting_with_a_slot(&app);
    assert!(!app.remote.active.contains(&ROW));
    let slot = &app.remote.reconnect[&ROW];
    assert_eq!(
        (slot.started, slot.next_attempt, slot.attempts),
        (started, next, 2)
    );
}
