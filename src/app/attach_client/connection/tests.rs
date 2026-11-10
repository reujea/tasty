use super::*;
use crate::app::attach_client::tests::supply_ids;
use crate::app::engine_registry::EngineRegistry;
use crate::app::window_access::EngineScanMut;
use crate::model::Workspace;
use crate::runtime::engine_session::EngineId;
use std::collections::HashMap;
use tasty_terminal::Terminal;

/// 공용 정리 본문을 검사한다. 두 호출 경로의 연결 여부까지 검증하는 시험은 아니다.
#[test]
fn remove_mirror_workspace_clears_terminal_busy_and_mesh() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ws_id = 9_000u32;
    let (pane_id, tab_id, local_surface) = (9_001u32, 9_002u32, 9_003u32);
    let remote_surface = 42u32;

    let mut mirror_ws = Workspace::new_with_terminal_marker(
        ws_id,
        "mirror".to_string(),
        pane_id,
        tab_id,
        local_surface,
    );
    mirror_ws.mirror = true;
    engine.push_mirror_workspace(mirror_ws);
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    engine.set_mirror_surface_busy(local_surface, true);
    engine.set_mirror_surface_cwd(local_surface, Some("/srv/remote".to_string()));
    engine.set_mirror_surface_foreground(local_surface, Some("claude".to_string()));
    engine
        .remote
        .attach_mesh_frames
        .update(local_surface, vec![1, 2, 3], 0, 0, true);
    state.set_active_workspace_index(&engine, engine.workspaces().len() - 1);

    let remote_to_local = HashMap::from([(remote_surface, local_surface)]);
    assert!(remove_mirror_workspace_from_engine(
        &mut engine,
        &mut state,
        ws_id,
        &remote_to_local
    ));

    assert!(!engine.has_workspace(ws_id), "mirror 워크스페이스 행 제거");
    assert!(
        !engine.runtime.terminals.contains(local_surface),
        "mirror 터미널 제거"
    );
    assert!(
        !engine.is_surface_busy(local_surface),
        "mirror busy 엔트리 제거"
    );
    assert!(
        engine
            .remote
            .attach_mesh_frames
            .get(local_surface)
            .is_none(),
        "mesh 프레임 캐시 제거"
    );
    assert!(
        engine.remote.mirror_surface_cwd.is_empty(),
        "mirror cwd 엔트리 제거"
    );
    assert_eq!(
        engine.mirror_surface_foreground(local_surface),
        None,
        "mirror 전경 이름 제거"
    );
    assert_eq!(
        state.active_workspace_index(&engine),
        engine.workspaces().len() - 1,
        "제거로 out-of-range 가 된 active_workspace 클램프"
    );
}

#[test]
fn remove_mirror_workspace_leaves_unrelated_engine_untouched() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let local_surface = 9_003u32;
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    let before = engine.workspaces().len();

    let remote_to_local = HashMap::from([(42u32, local_surface)]);
    assert!(!remove_mirror_workspace_from_engine(
        &mut engine,
        &mut state,
        9_000,
        &remote_to_local
    ));
    assert_eq!(engine.workspaces().len(), before);
    assert!(engine.runtime.terminals.contains(local_surface));
}

/// mirror가 두 번째 parked engine에 있어야 첫 항목만 검사하는 오류를 잡을 수 있다.
#[test]
fn cleanup_scans_all_parked_engines_for_the_mirror_workspace() {
    let ws_id = 9_000u32;
    let (pane_id, tab_id, local_surface) = (9_001u32, 9_002u32, 9_003u32);
    let remote_to_local = HashMap::from([(42u32, local_surface)]);

    let mut reg = EngineRegistry::default();
    let ids: Vec<EngineId> = (0..2)
        .map(|_| {
            let (state, engine_session) = crate::state::tests::test_state();
            reg.park_for_test(state, engine_session)
        })
        .collect();
    let untouched_ws_count = reg.get(ids[0]).expect("engine").workspaces().len();

    let mut mirror_ws = Workspace::new_with_terminal_marker(
        ws_id,
        "mirror".to_string(),
        pane_id,
        tab_id,
        local_surface,
    );
    mirror_ws.mirror = true;
    {
        let (state, mut engine) = reg.parked_session_mut(ids[1]).expect("parked 항목");
        engine.push_mirror_workspace(mirror_ws);
        engine
            .runtime
            .terminals
            .insert(local_surface, Terminal::new_detached(80, 24), None);
        engine.set_mirror_surface_busy(local_surface, true);
        engine
            .remote
            .attach_mesh_frames
            .update(local_surface, vec![1, 2, 3], 0, 0, true);
        state.set_active_workspace_index(engine.core, engine.workspaces().len() - 1);
    }

    assert!(remove_mirror_workspace_from_parked(
        EngineScanMut::from_fields(&mut HashMap::new(), &mut reg),
        ws_id,
        &remote_to_local
    ));

    let (state, engine) = reg.parked_session_mut(ids[1]).expect("parked 항목");
    assert!(!engine.has_workspace(ws_id), "mirror 워크스페이스 행 제거");
    assert!(
        !engine.runtime.terminals.contains(local_surface),
        "mirror 터미널 제거"
    );
    assert!(
        !engine.is_surface_busy(local_surface),
        "mirror busy 엔트리 제거"
    );
    assert!(
        engine
            .remote
            .attach_mesh_frames
            .get(local_surface)
            .is_none(),
        "mesh 프레임 캐시 제거"
    );
    assert_eq!(
        state.active_workspace_index(engine.core),
        engine.workspaces().len() - 1
    );
    assert_eq!(
        reg.get(ids[0]).expect("engine").workspaces().len(),
        untouched_ws_count,
        "무관한 parked engine 은 건드리지 않는다"
    );
}

#[test]
fn cleanup_parked_scan_reports_false_when_absent() {
    let mut reg = EngineRegistry::default();
    for _ in 0..2 {
        let (state, engine_session) = crate::state::tests::test_state();
        reg.park_for_test(state, engine_session);
    }
    let remote_to_local = HashMap::from([(42u32, 9_003u32)]);
    assert!(!remove_mirror_workspace_from_parked(
        EngineScanMut::from_fields(&mut HashMap::new(), &mut reg),
        9_000,
        &remote_to_local
    ));
}

#[test]
fn removing_a_mirror_workspace_forgets_the_occupancy_of_its_surfaces() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let parent = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let ws_id = 9000;
    let local = 9003;
    let mut workspace =
        Workspace::new_with_terminal_marker(ws_id, "mirror".into(), 9001, 9002, local);
    workspace.mirror = true;
    engine.push_mirror_workspace(workspace);
    engine.occupy_soft(local, parent, None).expect("soft 점유");

    let map = HashMap::from([(99u32, local)]);
    assert!(remove_mirror_workspace_from_engine(
        &mut engine,
        &mut state,
        ws_id,
        &map
    ));
    assert!(
        engine.live.occupancy.occupancy_of(local).is_none(),
        "정리된 mirror surface 의 soft 점유가 남았다"
    );
}

/// 재연결 설치의 View 마무리는 그 mirror 의 탐색기를 다시 읽게 한다. 옛 연결로 보낸 목록 요청은 응답이
/// 오지 않으므로, 다시 읽지 않으면 그 폴더는 시간 초과 오류에 머문다.
#[test]
fn finishing_a_reconnect_rereads_that_mirrors_explorers() {
    let (mut state, _engine) = crate::state::tests::test_state();
    let panel = tasty_model::ExplorerPanel::new(41, std::path::PathBuf::from("/remote/project"));
    let other = tasty_model::ExplorerPanel::new(42, std::path::PathBuf::from("/remote/other"));
    // 옛 연결로 보낸 요청이 응답을 기다리는 중이다.
    assert_eq!(
        state
            .explorer_views
            .get_or_init(&panel, Some(7))
            .drain_outbox()
            .len(),
        1
    );
    assert_eq!(
        state
            .explorer_views
            .get_or_init(&other, Some(8))
            .drain_outbox()
            .len(),
        1
    );
    assert!(
        state
            .explorer_views
            .get_or_init(&panel, Some(7))
            .drain_outbox()
            .is_empty()
    );

    finish_reconnected_view(&mut state, 7);
    assert_eq!(
        state
            .explorer_views
            .get_or_init(&panel, Some(7))
            .drain_outbox()
            .len(),
        1,
        "the reconnected mirror asks for its folder again"
    );
    assert!(
        state
            .explorer_views
            .get_or_init(&other, Some(8))
            .drain_outbox()
            .is_empty(),
        "another mirror keeps its request"
    );
}
