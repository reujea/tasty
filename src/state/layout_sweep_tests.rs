//! 레이아웃 스윕의 그리기 판정: 활성 workspace의 그리는 탭만 `drawn`이고, 창이 그리지 않으면 모두 숨는다.
use super::tests::{add_mirror_test_workspace, test_state};
use crate::app::frame_effect::{FrameEffect, GeometryEffect, LayoutTarget};
use crate::model::{PhysicalPx, PhysicalRect};

fn sweep(
    state: &mut super::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Vec<LayoutTarget> {
    let rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };
    state.take_pending_intents();
    state.resize_all(engine, rect, 8.0, 16.0, 1.0);
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::Frame(FrameEffect::Geometry(GeometryEffect::Resize {
                targets,
            })) => Some(targets),
            _ => None,
        })
        .flatten()
        .map(|(_, target)| target)
        .collect()
}

/// Given 활성 workspace와 숨은 workspace에 터미널이 하나씩, When 스윕하면, Then 활성 쪽만 `drawn`이고
/// 그 surface만 `swept_drawn`에 든다. When 창이 그리지 않으면, Then 모든 target이 숨는다.
#[test]
fn sweep_marks_only_the_drawn_tab_of_the_active_workspace() {
    let (mut state, mut session) = test_state();
    let mut engine = session.borrow_mut();
    let local = *engine
        .workspace_at(0)
        .expect("workspace")
        .all_surface_ids()
        .first()
        .expect("surface");
    let hidden = add_mirror_test_workspace(&mut state, &mut engine);
    state.set_active_workspace_index(&engine.read(), 0);

    let targets = sweep(&mut state, &engine.read());
    let drawn = |sid: u32| {
        targets
            .iter()
            .find(|t| t.surface_id == sid)
            .map(|t| t.drawn)
    };
    assert_eq!(drawn(local), Some(true));
    assert_eq!(drawn(hidden), Some(false));
    assert!(state.swept_drawn.contains(&local));
    assert!(!state.swept_drawn.contains(&hidden));
    assert!(state.draws_surface(&engine.read(), local));
    assert!(!state.draws_surface(&engine.read(), hidden));

    state.window_drawn = false;
    let targets = sweep(&mut state, &engine.read());
    assert!(
        targets.iter().all(|t| !t.drawn),
        "그리지 않는 창의 target은 모두 숨는다"
    );
    assert!(state.swept_drawn.is_empty());
}

/// Given 스윕 뒤 다른 workspace로 바뀐 프레임, When 그 프레임이 새 workspace의 터미널을 그리면, Then
/// 스윕이 처리하지 않은 터미널이라 다음 redraw가 필요하다. 다시 스윕하면 필요 없다.
#[test]
fn drawing_an_unswept_terminal_asks_for_another_sweep() {
    let (mut state, mut session) = test_state();
    let mut engine = session.borrow_mut();
    let hidden = add_mirror_test_workspace(&mut state, &mut engine);
    state.set_active_workspace_index(&engine.read(), 0);
    sweep(&mut state, &engine.read());

    // egui가 같은 프레임에서 workspace를 바꾼 경우.
    state.set_active_workspace_index(&engine.read(), engine.workspaces().len() - 1);
    let rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };
    let read = engine.read();
    let regions = state.surface_regions(&read, rect, 1.0);
    assert!(
        regions
            .iter()
            .flat_map(|(_, _, r)| r)
            .any(|r| r.id == hidden)
    );
    assert!(state.draws_unswept_terminal(&read, &regions));
    drop(regions);
    drop(read);

    sweep(&mut state, &engine.read());
    let read = engine.read();
    let regions = state.surface_regions(&read, rect, 1.0);
    assert!(!state.draws_unswept_terminal(&read, &regions));
}
