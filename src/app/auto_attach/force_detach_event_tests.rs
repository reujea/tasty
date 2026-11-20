//! 강제 끊김이 실제 이벤트 적용·이벤트 루프·실패 처리를 거치는 경로를 실제 `App` 으로 잰다.
//! 새 프로세스에서 혼자 돈다(`test_rig`).

use super::force_detach_result_tests::{
    ROW_B, prepare_an_automatic_reconnect, push_other_row_session_unflagged, reconnecting_app,
};
use super::force_detach_round_tests::{
    assert_b_ended_by_g1_and_a_dropped, marked, queue_a_forced_reconnect_of_the_other_row,
    run_json, session_of, ticket_of,
};
use super::test_rig::{ROW, event_loop};
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

/// 사용자 이벤트만 `App` 에 넘기는 이벤트 루프 처리기. 주기 확인(`about_to_wait` 의 타이머가 부르는
/// `poll_attach_views`)은 넘기지 않으므로, 회차는 `AttachClientData` 로만 열린다. 배달 순간에 깨움 문이
/// 열려 있었는지(깨움이 왔는지)를 기록한다.
struct UserEventsOnly<'a> {
    app: &'a mut App,
    attach_data_with_wake: Vec<bool>,
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

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: crate::AppEvent) {
        if matches!(event, crate::AppEvent::AttachClientData) {
            self.attach_data_with_wake
                .push(self.app.attach_client_wake.is_pending());
        }
        self.app.user_event(event_loop, event);
    }
}

/// S9: Given 행 A 의 자동 재연결이 설치만 남았고 같은 대상의 행 B 의 재연결 연결을 강제 끊김 통지를 보내는
/// 상대로 큐에 넣었다, When 시험이 결과를 직접 거두지 않고 실제 이벤트 루프의 사용자 이벤트만 배달하면,
/// Then reader 의 깨움이 깨움 문을 거쳐 `AttachClientData` 로 배달되고 그 이벤트가 연 회차가 B 를 G1 으로
/// 끝내고 A 를 버린다. 주기 확인 경로(`attach_poll`)는 배달하지 않는다.
#[test]
fn the_reader_wake_opens_the_round_through_the_event_loop() {
    alone!("the_reader_wake_opens_the_round_through_the_event_loop");
    use winit::platform::pump_events::EventLoopExtPumpEvents;
    let mut event_loop = event_loop();
    let mut app = reconnecting_app(&event_loop);
    let _socket = prepare_an_automatic_reconnect(&mut app);
    let (_peer, run, joined) = queue_a_forced_reconnect_of_the_other_row(&mut app);

    let mut handler = UserEventsOnly {
        app: &mut app,
        attach_data_with_wake: Vec::new(),
    };
    let until = Instant::now() + Duration::from_secs(10);
    while run_json(run)["state"] != "failed" {
        assert!(
            Instant::now() < until,
            "no round ended B through the event loop"
        );
        event_loop.pump_app_events(Some(Duration::from_millis(20)), &mut handler);
    }
    let delivered = std::mem::take(&mut handler.attach_data_with_wake);
    assert!(
        delivered.contains(&true),
        "an AttachClientData arrived with the wake gate armed: {delivered:?}"
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
