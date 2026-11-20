//! 강제 끊김이 실제 이벤트 적용·이벤트 루프·실패 처리를 거치는 경로를 실제 `App` 으로 잰다.
//! 새 프로세스에서 혼자 돈다(`test_rig`).

use super::force_detach_result_tests::{
    ROW_B, prepare_an_automatic_reconnect, push_other_row_session_unflagged, reconnecting_app,
};
use super::force_detach_round_tests::{
    PeerGates, assert_b_ended_by_g1_and_a_dropped, marked,
    queue_a_gated_forced_reconnect_of_the_other_row, run_json, session_of, ticket_of,
    wait_for_and_lower_the_attach_wake,
};
use super::test_rig::{ROW, app_with_saved_row, binding, event_loop};
use crate::app::App;
use std::time::{Duration, Instant};
use tasty_remote::client_session::{MirrorEvent, SessionState, WorkspaceClose};
use tasty_remote::transport_end::EndReason;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;

macro_rules! alone {
    ($name:literal) => {
        if crate::test_support::rerun_alone(module_path!(), $name) {
            return;
        }
        let _home = crate::test_support::IsolatedHome::new();
    };
}

/// 사용자 이벤트와 `about_to_wait` 의 미룬 스트림 회차만 `App` 에 넘기는 이벤트 루프 처리기. 주기 확인
/// (`about_to_wait` 의 타이머가 부르는 `poll_attach_views`)은 넘기지 않는다. 그래서 회차는 깨움 문이 열린
/// 때만 `AttachClientData` 나 미룬 회차로 열린다. `run` 이 어느 자리에서 실패로 끝났는지 기록한다.
struct UserEventsOnly<'a> {
    app: &'a mut App,
    run: u64,
    attach_data_with_wake: Vec<bool>,
    ended_at: Option<&'static str>,
}

impl UserEventsOnly<'_> {
    fn note_end(&mut self, site: &'static str) {
        if self.ended_at.is_none() && run_json(self.run)["state"] == "failed" {
            self.ended_at = Some(site);
        }
    }
}

impl ApplicationHandler<crate::AppEvent> for UserEventsOnly<'_> {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.app.run_deferred_stream_rounds();
        self.note_end("about_to_wait");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: crate::AppEvent) {
        match event {
            crate::AppEvent::AttachClientData => {
                let armed = self.app.attach_client_wake.is_pending();
                self.attach_data_with_wake.push(armed);
                self.app.user_event(event_loop, event);
                self.note_end("AttachClientData");
            }
            other => self.app.user_event(event_loop, other),
        }
    }
}

/// 이벤트 루프에 쌓인 이벤트를 `App` 에 넘기지 않고 버린다.
struct Discard;

impl ApplicationHandler<crate::AppEvent> for Discard {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: crate::AppEvent) {}
}

/// 깨움 문을 내리고 지금까지 쌓인 사용자 이벤트를 버린다. 그 뒤 문은 닫혀 있어야 한다.
fn quiesce(event_loop: &mut winit::event_loop::EventLoop<crate::AppEvent>, app: &App) {
    use winit::platform::pump_events::EventLoopExtPumpEvents;
    app.attach_client_wake.open_round();
    for _ in 0..3 {
        event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut Discard);
    }
    assert!(!app.attach_client_wake.is_pending(), "nothing woke since");
}

/// S9: Given 행 A 의 자동 재연결이 설치만 남았고 같은 대상의 행 B 의 재연결 연결 결과가 채널에 있으며,
/// 그 두 결과의 깨움과 사용자 이벤트는 비웠다(상대는 강제 끊김 통지를 아직 보내지 않음), When 상대가
/// 통지를 보내 B 의 reader 가 강제 끊김을 확정하고 실제 이벤트 루프가 사용자 이벤트를 배달하면, Then 그
/// reader 깨움이 깨움 문을 연 채 `AttachClientData` 로 배달되고, 그 이벤트의 처리(제품 진입
/// `run_attach_client_round(Event)`)가 연 회차가 B 를 G1 으로 끝내고 A 를 버린다. 주기 확인 경로는 돌리지
/// 않는다. reader 가 깨우기 전에 사유를 확정하는 순서는 전송 시험 S2 가 잰다. 이 시험은 깨움 콜백 안에서
/// 사유를 읽지 않는다.
#[test]
fn the_reader_wake_opens_the_round_through_the_event_loop() {
    alone!("the_reader_wake_opens_the_round_through_the_event_loop");
    use winit::platform::pump_events::EventLoopExtPumpEvents;
    let mut event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    quiesce(&mut event_loop, &app);
    let (handshake, handshake_held) = std::sync::mpsc::channel();
    let (notice, notice_held) = std::sync::mpsc::channel();
    let (peer, run, joined) = queue_a_gated_forced_reconnect_of_the_other_row(
        &mut app,
        PeerGates {
            handshake: Some(handshake_held),
            notice: Some(notice_held),
        },
    );
    handshake.send(()).unwrap();
    wait_for_and_lower_the_attach_wake(&app, "B's connection result");
    quiesce(&mut event_loop, &app);
    assert_ne!(run_json(run)["state"], "failed", "no round ran yet");

    notice.send(()).unwrap();
    let _socket_b = peer.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut handler = UserEventsOnly {
        app: &mut app,
        run,
        attach_data_with_wake: Vec::new(),
        ended_at: None,
    };
    let until = Instant::now() + Duration::from_secs(10);
    while handler.ended_at.is_none() {
        assert!(
            Instant::now() < until,
            "no round ended B through the event loop"
        );
        event_loop.pump_app_events(Some(Duration::from_millis(20)), &mut handler);
    }
    let delivered = std::mem::take(&mut handler.attach_data_with_wake);
    let ended_at = handler.ended_at;
    assert_eq!(
        delivered.first(),
        Some(&true),
        "the reader wake arrived as AttachClientData with the gate armed: {delivered:?}"
    );
    assert_eq!(
        ended_at,
        Some("AttachClientData"),
        "the event itself opened the round"
    );

    assert_b_ended_by_g1_and_a_dropped(&mut app, run, joined);
}

/// G2-a: Given 같은 대상의 행 A 는 자동 재연결이 설치만 남았고 행 B 는 닫기 회신을 기다리며 연결돼 있다,
/// When 한 회차에 B 의 강제 끊김(관측)과 닫기 성공 회신(이벤트 적용)이 함께 오면, Then B 는 서버 닫기로
/// 끝나고, 관측이 쓴 표지로 A 는 같은 회차에 버려지며 표지는 남는다. 시험 장치의 창에는 화면이 없어
/// 창을 park 해 B 의 이벤트를 실제 적용 경로로 적용한다.
#[test]
fn a_close_reply_with_a_force_detach_ends_b_while_a_is_dropped_and_the_mark_stays() {
    alone!("a_close_reply_with_a_force_detach_ends_b_while_a_is_dropped_and_the_mark_stays");
    let event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    push_other_row_session_unflagged(&mut app, SessionState::Connected);
    let (state, _) = crate::state::tests::test_state();
    app.engines
        .park(winit::window::WindowId::from(u64::MAX - 2), state)
        .expect("the rows' window is parked");
    {
        let b = app
            .remote
            .sessions
            .iter_mut()
            .find(|session| session.state.saved_row == Some(ROW_B))
            .unwrap();
        b.state.workspace_close = WorkspaceClose::Pending(3);
        assert!(
            b.transport
                .output
                .push(MirrorEvent::StructuralSucceeded(3, None, 0))
        );
    }
    crate::app::attach_client::tests::settle_transport_end(
        session_of(&app, ROW_B),
        EndReason::Forced,
    );

    app.apply_attach_client_output();

    assert!(
        app.remote
            .sessions
            .iter()
            .all(|session| session.state.saved_row != Some(ROW_B)),
        "B ended by the server close"
    );
    assert!(
        ticket_of(&app, ROW).is_none(),
        "A dropped in the same round"
    );
    assert!(marked(&app), "the mark written by the observation stays");
}

/// U8 이어서: Given 표지 아래에서 연결된 행 A 가 손실 뒤 멈췄다, When A 를 명시 재연결했는데 연결 대상을
/// 해석하지 못하면, Then 그 시도는 재동기화가 아닌 일반 재연결의 실패로 끝나 진행 중 표지를 지우고 재연결
/// 실패 처리를 탄다. 시험의 격리 홈에는 프로필이 없어 해석이 실패한다. 이 실패는 영구 거절이 아니므로 그
/// 처리는 시도 수를 올리고 백오프를 한 단계 늘린 뒤 다음 시도를 앞으로 예약한다.
#[test]
fn an_explicit_reconnect_after_a_stopped_loss_fails_as_an_ordinary_reconnect() {
    alone!("an_explicit_reconnect_after_a_stopped_loss_fails_as_an_ordinary_reconnect");
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

    app.connect_saved_row(ROW, true);
    // 명시 연결은 슬롯을 새로 둔다. 기준은 그 뒤, 결과를 거두기 전에 잰다.
    let (before, backoff_before) = app
        .remote
        .reconnect
        .get(&ROW)
        .map(|slot| (slot.attempts, slot.backoff.current()))
        .unwrap_or_default();
    let resync = app
        .state
        .mirror_attempts
        .endpoint_snapshot()
        .into_iter()
        .find(|(_, target)| target.row == Some(ROW))
        .map(|(_, target)| target.resync);
    assert_eq!(resync, Some(false), "an ordinary reconnect");
    let until = Instant::now() + Duration::from_secs(10);
    while !app.state.mirror_attempts.endpoint_snapshot().is_empty() {
        assert!(Instant::now() < until, "the endpoint attempt never ended");
        std::thread::sleep(Duration::from_millis(10));
        app.drain_auto_attach_results();
    }

    assert!(!app.remote.active.contains(&ROW), "no attempt in progress");
    assert!(app.state.mirror_attempts.connection_snapshot().is_empty());
    let state = &app.remote.sessions[0].state;
    assert_ne!(state.phase, SessionState::Connected);
    assert_eq!(state.resync_pending, None);
    let checked = Instant::now();
    let slot = app.remote.reconnect.get(&ROW).expect("the reconnect slot");
    assert!(
        !slot.given_up,
        "a missing profile is not a permanent refusal"
    );
    assert_eq!(slot.attempts, before + 1, "one failed attempt counted");
    assert!(
        slot.backoff.current() > backoff_before,
        "the backoff advanced: {:?} (before {backoff_before:?})",
        slot.backoff.current()
    );
    assert!(
        slot.next_attempt > checked,
        "the next attempt is scheduled ahead"
    );
    assert!(!marked(&app), "the explicit connect erased the mark");
}
