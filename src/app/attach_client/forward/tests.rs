use super::*;

use crate::ipc::stream::{InheritCwd, StructuralOp};
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn an_agent_close_is_forwarded_with_the_agent_origin() {
    let origin_on_wire = |user_triggered| {
        let payload = structural_op_payload(
            3,
            tasty_ipc::stream::StructuralOp::CloseSurface { surface_id: 9 },
            user_triggered,
        );
        let v: Value = serde_json::from_slice(&payload).expect("json");
        assert_eq!(v["event"], "structural_op");
        v["origin"].clone()
    };
    assert_eq!(origin_on_wire(false), "agent");
    assert_eq!(origin_on_wire(true), "user");
}

#[test]
fn remote_structural_op_maps_the_move_target_to_its_remote_id() {
    // remote → local. 로컬 ID가 다른 원격 surface ID와 겹치는 배치다.
    let map: HashMap<u32, u32> = [(40, 3), (41, 4), (3, 9)].into_iter().collect();
    let local = StructuralOp::MoveSurface {
        source_surface_id: 4,
        target_surface_id: 3,
    };
    let wire = remote_structural_op(&local, 41, &map).expect("target mapped");
    assert_eq!(
        wire,
        StructuralOp::MoveSurface {
            source_surface_id: 41,
            target_surface_id: 40,
        },
        "target 도 원격 ID 로 보내야 한다 — 로컬 3 을 그대로 보내면 원격 surface 3 을 가리킨다"
    );
}

#[test]
fn remote_structural_op_refuses_a_move_whose_target_is_not_mirrored() {
    let map: HashMap<u32, u32> = [(41, 4)].into_iter().collect();
    let local = StructuralOp::MoveSurface {
        source_surface_id: 4,
        target_surface_id: 7,
    };
    assert!(
        remote_structural_op(&local, 41, &map)
            .expect_err("unmapped target")
            .contains("surface 7 is outside the original mirror")
    );
}

#[test]
fn remote_structural_op_only_swaps_the_anchor_for_other_ops() {
    let local = StructuralOp::CloseSurface { surface_id: 4 };
    assert_eq!(
        remote_structural_op(&local, 41, &HashMap::new()),
        Ok(StructuralOp::CloseSurface { surface_id: 41 })
    );
}

fn new_tab(inherit_cwd: Option<InheritCwd>) -> StructuralOp {
    StructuralOp::NewTab {
        anchor_surface_id: 4,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd,
    }
}

/// client가 고른 상속 원본도 원격 ID로 보낸다. 로컬 3을 그대로 보내면 원격 surface 3의 cwd를 잇는다.
#[test]
fn remote_structural_op_maps_the_inherit_cwd_source_to_its_remote_id() {
    let map: HashMap<u32, u32> = [(40, 3), (41, 4), (3, 9)].into_iter().collect();
    assert_eq!(
        remote_structural_op(&new_tab(Some(InheritCwd::Surface(3))), 41, &map),
        Ok(StructuralOp::NewTab {
            anchor_surface_id: 41,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: Some(InheritCwd::Surface(40)),
        })
    );
    for kept in [None, Some(InheritCwd::NoInherit)] {
        assert_eq!(
            remote_structural_op(&new_tab(kept.clone()), 41, &map)
                .expect("nothing to map")
                .inherit_cwd(),
            kept.as_ref()
        );
    }
}

/// 상속 원본의 원격 매핑이 없으면 원본을 빼고 보내지 않고 거절한다. 빼면 서버가 자기 선택으로 cwd를 고른다.
#[test]
fn remote_structural_op_refuses_an_unmapped_inherit_cwd_source() {
    let map: HashMap<u32, u32> = [(41, 4)].into_iter().collect();
    let refusal = remote_structural_op(&new_tab(Some(InheritCwd::Surface(7))), 41, &map)
        .expect_err("unmapped source");
    assert!(
        refusal.contains("inherit_cwd surface 7 is outside the original mirror"),
        "{refusal}"
    );
}
