use super::*;
use crate::app::attach_client::projection::build_mirror_workspace;
use crate::app::attach_client::tests::test_ids;
use crate::ipc::stream::{SplitAxis, StructuralOp};
use std::collections::{HashMap, HashSet};
use tasty_remote::client_session::{MirrorStructureIds, PendingOpFocus};

/// pane B의 두 번째 탭에 있는 surface를 원격 ID로 되찾는다.
#[test]
fn capture_focused_remote_finds_remote_id_of_locally_focused_surface() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let ids = test_ids();
    let mut map = HashMap::new();
    map.insert(1u32, 50u32); // pane A 의 surface
    map.insert(2u32, 51u32); // pane B, tab1 의 surface
    map.insert(3u32, 52u32); // pane B, tab2 의 surface — 사용자가 보고 있는 곳
    let mut term = HashSet::new();
    term.insert(50u32);
    term.insert(51u32);
    term.insert(52u32);
    let tree = serde_json::json!({
        "id": 9, "name": "remote", "focused_pane": 11,
        "panes": [],
        "pane_layout": {
            "type": "Split", "direction": "horizontal", "ratio": 0.5,
            "first": { "type": "Leaf", "id": 10, "tabs": [
                { "id": 100, "name": "Shell", "active": true, "focused_surface": 1,
                  "layout": { "type": "Leaf", "id": 1, "kind": "terminal" } }
            ] },
            "second": { "type": "Leaf", "id": 11, "tabs": [
                { "id": 110, "name": "Shell", "active": false, "focused_surface": 2,
                  "layout": { "type": "Leaf", "id": 2, "kind": "terminal" } },
                { "id": 111, "name": "Shell", "active": true, "focused_surface": 3,
                  "layout": { "type": "Leaf", "id": 3, "kind": "terminal" } }
            ] }
        }
    });
    let ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        99,
        "remote",
        &tree,
        &ids,
        &map,
        &term,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    )
    .expect("fixture construction");
    assert_eq!(
        capture_focused_remote(&navigation, &ws, &map),
        Some(3),
        "focused_pane=pane B, active_tab=tab2(remote 3) 를 정확히 되짚어야 한다"
    );
}

#[test]
fn set_focus_to_surface_updates_pane_tab_surface_or_reports_false() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let ids = test_ids();
    let mut map = HashMap::new();
    map.insert(1u32, 50u32);
    map.insert(2u32, 51u32);
    let mut term = HashSet::new();
    term.insert(50u32);
    term.insert(51u32);
    let tree = serde_json::json!({
        "id": 9, "name": "remote", "focused_pane": 10,
        "panes": [],
        "pane_layout": {
            "type": "Split", "direction": "horizontal", "ratio": 0.5,
            "first": { "type": "Leaf", "id": 10, "tabs": [
                { "id": 100, "name": "Shell", "active": true, "focused_surface": 1,
                  "layout": { "type": "Leaf", "id": 1, "kind": "terminal" } }
            ] },
            "second": { "type": "Leaf", "id": 11, "tabs": [
                { "id": 110, "name": "Shell", "active": true, "focused_surface": 2,
                  "layout": { "type": "Leaf", "id": 2, "kind": "terminal" } }
            ] }
        }
    });
    let ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        99,
        "remote",
        &tree,
        &ids,
        &map,
        &term,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    )
    .expect("fixture construction");
    let local_b = *map.get(&2).unwrap();

    assert!(set_focus_to_surface(&mut navigation, &ws, local_b));
    let (pane_b_id, tab_b_id) = find_pane_and_tab_for_surface(&ws, local_b).expect("pane B exists");
    assert_eq!(navigation.pane_id(&ws).unwrap(), pane_b_id);
    let pane_b = ws.pane_layout().find_pane(pane_b_id).unwrap();
    assert_eq!(pane_b.tabs[navigation.tab_index(pane_b)].id, tab_b_id);
    assert_eq!(
        navigation
            .surface_id(&pane_b.tabs[navigation.tab_index(pane_b)])
            .unwrap(),
        local_b
    );

    assert!(
        !set_focus_to_surface(&mut navigation, &ws, 12345),
        "존재하지 않는 surface 는 false"
    );
}

#[test]
fn pending_op_focus_for_new_tab_and_split_is_new_resource() {
    let map = HashMap::new();
    for op in [
        StructuralOp::NewTab {
            anchor_surface_id: 1,
            surface_kind: "terminal".to_string(),
            params: serde_json::Value::Null,
            inherit_cwd: None,
        },
        StructuralOp::SplitSurface {
            surface_id: 1,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::Value::Null,
            inherit_cwd: None,
        },
        StructuralOp::SplitPane {
            anchor_surface_id: 1,
            direction: SplitAxis::Vertical,
            surface_kind: "terminal".to_string(),
            params: serde_json::Value::Null,
            inherit_cwd: None,
        },
        StructuralOp::RestoreClosedItem {
            anchor_surface_id: 1,
        },
    ] {
        assert!(matches!(
            pending_op_focus_for(&op, &[], &map),
            Some(PendingOpFocus::NewResource)
        ));
    }
}

#[test]
fn pending_op_focus_for_close_translates_candidates_or_none() {
    let mut map = HashMap::new();
    map.insert(7u32, 70u32); // remote 7 -> local 70
    map.insert(8u32, 71u32); // remote 8 -> local 71

    let op = StructuralOp::CloseSurface { surface_id: 1 };
    match pending_op_focus_for(&op, &[70, 71], &map) {
        Some(PendingOpFocus::Close { candidates }) => {
            assert_eq!(candidates, vec![7, 8]);
        }
        other => panic!("expected Close{{candidates}}, got {other:?}"),
    }

    assert!(pending_op_focus_for(&op, &[999], &map).is_none());
    assert!(pending_op_focus_for(&op, &[], &map).is_none());
}

#[test]
fn pending_op_focus_for_non_target_ops_is_none() {
    let mut map = HashMap::new();
    map.insert(7u32, 70u32);
    let op = StructuralOp::MoveTab {
        anchor_surface_id: 1,
        from_index: 0,
        to_index: 1,
    };
    assert!(pending_op_focus_for(&op, &[70], &map).is_none());
}

/// 원격 포커스와 다른 로컬 포커스를 기억해 구조 재구성 뒤 복원한다.
#[test]
fn focus_restore_keeps_client_on_pane_b_after_structural_delta_from_pane_a() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let ids = test_ids();

    let mut map = HashMap::new();
    map.insert(1u32, 50u32);
    map.insert(2u32, 51u32);
    map.insert(3u32, 52u32);
    let mut term = HashSet::new();
    term.insert(50u32);
    term.insert(51u32);
    term.insert(52u32);
    let before_tree = serde_json::json!({
        "id": 9, "name": "remote", "focused_pane": 10,
        "panes": [],
        "pane_layout": {
            "type": "Split", "direction": "horizontal", "ratio": 0.5,
            "first": { "type": "Leaf", "id": 10, "tabs": [
                { "id": 100, "name": "Shell", "active": true, "focused_surface": 1,
                  "layout": { "type": "Leaf", "id": 1, "kind": "terminal" } }
            ] },
            "second": { "type": "Leaf", "id": 11, "tabs": [
                { "id": 110, "name": "Shell", "active": false, "focused_surface": 2,
                  "layout": { "type": "Leaf", "id": 2, "kind": "terminal" } },
                { "id": 111, "name": "Shell", "active": true, "focused_surface": 3,
                  "layout": { "type": "Leaf", "id": 3, "kind": "terminal" } }
            ] }
        }
    });
    let mut before_ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        99,
        "remote",
        &before_tree,
        &ids,
        &map,
        &term,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    )
    .expect("fixture construction");

    let pane_b_surface3_local = *map.get(&3).unwrap();
    let (pane_b_id, tab_id) = find_pane_and_tab_for_surface(&before_ws, pane_b_surface3_local)
        .expect("pane B tab2 surface must exist");
    navigation.select_pane(&before_ws, pane_b_id);
    let pane_b = before_ws
        .pane_layout_mut()
        .find_pane_mut(pane_b_id)
        .expect("pane B exists");
    let tab_index = pane_b
        .tabs
        .iter()
        .position(|t| t.id == tab_id)
        .expect("tab exists");
    navigation.goto_tab(pane_b, tab_index);
    navigation.select_surface(&pane_b.tabs[tab_index], pane_b_surface3_local);

    let old_focused_remote = capture_focused_remote(&navigation, &before_ws, &map);
    assert_eq!(old_focused_remote, Some(3));

    let mut after_map = map.clone();
    after_map.insert(4u32, 53u32);
    term.insert(53u32);
    let after_tree = serde_json::json!({
        "id": 9, "name": "remote", "focused_pane": 10,
        "panes": [],
        "pane_layout": {
            "type": "Split", "direction": "horizontal", "ratio": 0.5,
            "first": { "type": "Leaf", "id": 10, "tabs": [
                { "id": 100, "name": "Shell", "active": true, "focused_surface": 1,
                  "layout": { "type": "Leaf", "id": 1, "kind": "terminal" } },
                { "id": 101, "name": "Shell", "active": false, "focused_surface": 4,
                  "layout": { "type": "Leaf", "id": 4, "kind": "terminal" } }
            ] },
            "second": { "type": "Leaf", "id": 11, "tabs": [
                { "id": 110, "name": "Shell", "active": false, "focused_surface": 2,
                  "layout": { "type": "Leaf", "id": 2, "kind": "terminal" } },
                { "id": 111, "name": "Shell", "active": true, "focused_surface": 3,
                  "layout": { "type": "Leaf", "id": 3, "kind": "terminal" } }
            ] }
        }
    });
    let mut after_ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        99,
        "remote",
        &after_tree,
        &ids,
        &after_map,
        &term,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    )
    .expect("fixture construction");

    // Stable local pane/tab IDs preserve the live selection during rebuild.
    assert_eq!(navigation.pane_id(&after_ws), Some(pane_b_id));
    assert_eq!(
        capture_focused_remote(&navigation, &after_ws, &after_map),
        Some(3)
    );

    restore_focus_after_delta(
        &mut navigation,
        &mut after_ws,
        old_focused_remote,
        &after_map,
    );

    let pane_b_surface3_local = *after_map.get(&3).unwrap();
    let (pane_b_id, tab_id) = find_pane_and_tab_for_surface(&after_ws, pane_b_surface3_local)
        .expect("pane B tab2 surface must exist in rebuilt tree");
    assert_eq!(
        navigation.pane_id(&after_ws).unwrap(),
        pane_b_id,
        "복원 후 focus 는 pane A 가 아니라 사용자가 실제로 보던 pane B 에 있어야 한다"
    );
    let pane_b = after_ws
        .pane_layout()
        .find_pane(pane_b_id)
        .expect("pane B exists");
    assert_eq!(
        pane_b.tabs[navigation.tab_index(pane_b)].id,
        tab_id,
        "pane B 의 active_tab 도 사용자가 보던 두 번째 탭이어야 한다"
    );
    assert_eq!(
        navigation
            .surface_id(&pane_b.tabs[navigation.tab_index(pane_b)])
            .unwrap(),
        pane_b_surface3_local,
        "그 탭의 focused_surface 도 정확히 그 surface 를 가리켜야 한다"
    );
}

#[test]
fn focus_restore_is_noop_when_captured_surface_no_longer_exists() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let ids = test_ids();
    let mut map = HashMap::new();
    map.insert(1u32, 50u32);
    map.insert(2u32, 51u32);
    let mut term = HashSet::new();
    term.insert(50u32);
    term.insert(51u32);
    let tree = serde_json::json!({
        "id": 9, "name": "remote", "focused_pane": 10,
        "panes": [],
        "pane_layout": {
            "type": "Split", "direction": "horizontal", "ratio": 0.5,
            "first": { "type": "Leaf", "id": 10, "tabs": [
                { "id": 100, "name": "Shell", "active": true, "focused_surface": 1,
                  "layout": { "type": "Leaf", "id": 1, "kind": "terminal" } }
            ] },
            "second": { "type": "Leaf", "id": 11, "tabs": [
                { "id": 110, "name": "Shell", "active": true, "focused_surface": 2,
                  "layout": { "type": "Leaf", "id": 2, "kind": "terminal" } }
            ] }
        }
    });
    let mut ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        99,
        "remote",
        &tree,
        &ids,
        &map,
        &term,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    )
    .expect("fixture construction");
    let untouched_focused_pane = navigation.pane_id(&ws).unwrap();

    restore_focus_after_delta(&mut navigation, &mut ws, Some(3), &map);

    assert_eq!(
        navigation.pane_id(&ws).unwrap(),
        untouched_focused_pane,
        "캡처된 surface 가 없으면 원격이 보낸 focused_pane 그대로 둬야 한다"
    );
}
