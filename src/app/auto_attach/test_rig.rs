//! 저장된 원격 행을 가진 `App` 을 만든다. 행의 상태 전이를 실제 `App` 메서드로 잰다.
//! `App` 을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop` 는 프로세스에 하나뿐이라,
//! 이 도구를 쓰는 시험은 `crate::test_support::rerun_alone` 으로 새 프로세스에서 혼자 돈다.

use crate::app::App;
use crate::model::RemoteBinding;
use std::collections::HashMap;
use tasty_remote::client_session::SessionState;
use winit::event_loop::EventLoop;
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::WindowId;

pub(crate) const ROW: u32 = 9_000;

pub(crate) fn event_loop() -> EventLoop<crate::AppEvent> {
    EventLoop::<crate::AppEvent>::with_user_event()
        .with_x11()
        .with_any_thread(true)
        .build()
        .unwrap()
}

pub(crate) fn binding() -> RemoteBinding {
    RemoteBinding {
        profile: "rv".to_string(),
        remote_workspace: 3,
    }
}

/// 창이 있는 engine 에 저장된 원격 행 `ROW` 하나를 둔 `App`. `phase` 가 있으면 그 단계의 행 세션도 둔다.
pub(crate) fn app_with_saved_row(
    event_loop: &EventLoop<crate::AppEvent>,
    phase: Option<SessionState>,
) -> App {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    if let Some(pending) = app.engines.pending_id() {
        app.engines
            .attach_window(WindowId::from(u64::MAX - 1), pending);
    }
    let (_, session) = crate::state::tests::test_state();
    let mut row = crate::model::Workspace::new_with_terminal_marker(
        ROW,
        "remote".to_string(),
        9_001,
        9_002,
        9_003,
    );
    row.mirror = true;
    row.remote_row = Some(binding());
    let mut session = session;
    session.borrow_mut().push_mirror_workspace(row);
    let id = app
        .engines
        .insert_pending(session)
        .unwrap_or_else(|_| panic!("a pending engine is left"));
    app.engines.attach_window(WindowId::from(u64::MAX - 2), id);
    if let Some(phase) = phase {
        let mut session = crate::app::attach_client::tests::test_session(ROW, HashMap::new());
        session.state.saved_row = Some(ROW);
        session.state.phase = phase;
        app.remote.sessions.push(session);
    }
    app
}
