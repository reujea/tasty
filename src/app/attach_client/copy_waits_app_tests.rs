//! 복사 실패가 실제 회차 함수(`fail_copies_of_ending_sessions`·`settle_attach_copies`)를 지나 토스트 범위가
//! 되는 것을 잰다. 끝나는 세션의 목록이 실패 surface와 이어지는지가 대상이다.
//! `App` 을 만드는 데 X11 디스플레이가 필요하고 winit `EventLoop` 는 프로세스에 하나뿐이라 새 프로세스에서
//! 혼자 돈다. 메인 창은 GPU 가 있어야 만들어지므로 같은 안내를 받는 parked engine 에서 본다.

use crate::adapters::ui::ToastScope;
use crate::app::App;
use crate::app::attach_client::tests::test_session;
use std::collections::HashMap;
use std::time::Instant;
use tasty_core::{DomainEvent, SplitSpec, SurfaceSpec};

/// 한 탭에 나란히 보이는 두 mirror surface. 첫째는 끝나는 세션, 둘째는 살아 있는 세션의 것이다.
const ENDING: u32 = 71_001;
const LIVING: u32 = 71_002;
/// 본문을 받는 중인 OSC 52 쓰기.
const WRITING: &[u8] = b"\x1b]52;c;aGk";

fn app_on(event_loop: &winit::event_loop::EventLoop<crate::AppEvent>) -> App {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    let surface = |id| SurfaceSpec {
        id,
        kind: "terminal".into(),
        data: None,
    };
    let model = crate::state::tests::test_model(vec![
        DomainEvent::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        DomainEvent::WorkspaceCreated {
            id: ENDING,
            name: "Workspace 1".into(),
            category: 0,
            index: 0,
            pane: ENDING,
        },
        DomainEvent::TabCreated {
            id: ENDING,
            pane: ENDING,
            index: 0,
            name: "Terminal".into(),
            surface: surface(ENDING),
        },
        DomainEvent::SurfaceSplit {
            target: ENDING,
            surface: surface(LIVING),
            split: SplitSpec {
                direction: crate::model::SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ]);
    let (state, session) = crate::state::tests::test_state_from_model(model);
    app.engines.park_for_test(state, session);
    app.remote
        .sessions
        .push(test_session(ENDING, HashMap::from([(1, ENDING)])));
    app.remote
        .sessions
        .push(test_session(ENDING, HashMap::from([(2, LIVING)])));
    for surface in [ENDING, LIVING] {
        app.attach_copies.feed(surface, WRITING, Instant::now());
    }
    app
}

fn scopes(app: &mut App) -> Vec<ToastScope> {
    app.engines_mut()
        .parked_sessions_with_ids()
        .flat_map(|(_, state, _)| state.toasts.scopes())
        .collect()
}

/// Given 두 surface가 다 보이고 끝나는 세션은 없다, When 출력 회차의 종료 정산과 일반 정산이 실패를 알린다,
/// Then 두 실패 모두 기존 결과 범위 규칙대로 surface 범위다.
#[test]
fn failures_without_an_ending_session_keep_the_result_scope() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "failures_without_an_ending_session_keep_the_result_scope",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = crate::x11_test_loop::x11_event_loop();
    let mut app = app_on(&event_loop);
    // 재동기화가 본문을 받던 쓰기를 끊었다(begin_resync와 같은 정산).
    app.attach_copies.break_stream(ENDING);
    app.fail_copies_of_ending_sessions(&[]);
    app.remote.sessions[1].state.resync_pending = Some(1);
    app.settle_attach_copies();
    assert_eq!(
        scopes(&mut app),
        vec![ToastScope::Surface(ENDING), ToastScope::Surface(LIVING)]
    );
}

/// Given 한 세션이 끝나고 다른 세션은 재동기화로 쓰기를 잃었다, When 같은 회차에 끝나는 세션을 정산한다,
/// Then 끝나는 세션의 실패만 창 범위이고 살아 있는 세션의 실패는 surface 범위다.
#[test]
fn only_the_ending_session_failure_goes_to_the_window() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "only_the_ending_session_failure_goes_to_the_window",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = crate::x11_test_loop::x11_event_loop();
    let mut app = app_on(&event_loop);
    app.remote.sessions[1].state.resync_pending = Some(1);
    app.attach_copies.break_stream(LIVING);
    app.fail_copies_of_ending_sessions(&[0]);
    assert_eq!(
        scopes(&mut app),
        vec![ToastScope::Surface(LIVING), ToastScope::Window]
    );
}
