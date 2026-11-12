//! `tree` 응답의 busy 필드를 단계(workspace → pane → tab → surface)에 맞춰 붙인다.
//!
//! tab id와 surface id는 서로 다른 번호 공간이라 숫자가 자주 겹친다. 그래서 키 모양으로
//! 리프를 추측하지 않고, surface 리프는 나뉘지 않은 tab의 `surface` 객체와 `layout`의
//! `Leaf` 노드 두 곳으로만 정한다. 집계 정의는 `docs/design/policies/busy-indicator.md`의
//! "집계 — OR"이다.

use serde_json::{Value, json};

/// workspace 하나의 트리 JSON에 busy 필드를 붙인다.
///
/// - surface 리프: `busy`
/// - tab: `busy_count`(그 tab의 surface 중 busy 수)와 `busy`(`busy_count > 0`)
/// - pane: `busy_count`(그 pane의 모든 tab `busy_count` 합, 선택되지 않은 tab 포함)
///
/// workspace의 `busy_count`는 호출자가 모델에서 계산해 넣으므로 건드리지 않는다.
pub(crate) fn annotate_tree_busy(workspace: &mut Value, is_busy: &dyn Fn(u32) -> bool) {
    let Some(panes) = workspace.get_mut("panes").and_then(Value::as_array_mut) else {
        return;
    };
    for pane in panes {
        let mut pane_count: u64 = 0;
        if let Some(tabs) = pane.get_mut("tabs").and_then(Value::as_array_mut) {
            for tab in tabs {
                let tab_count = tab
                    .get_mut("surface")
                    .map_or(0, |surface| annotate_tab_content(surface, is_busy));
                if let Some(obj) = tab.as_object_mut() {
                    obj.insert("busy_count".into(), json!(tab_count));
                    obj.insert("busy".into(), json!(tab_count > 0));
                }
                pane_count += tab_count;
            }
        }
        if let Some(obj) = pane.as_object_mut() {
            obj.insert("busy_count".into(), json!(pane_count));
        }
    }
}

/// tab의 `surface` 값에 리프 `busy`를 붙이고 그 tab의 busy surface 수를 돌려준다.
fn annotate_tab_content(content: &mut Value, is_busy: &dyn Fn(u32) -> bool) -> u64 {
    if content.get("type").and_then(Value::as_str) == Some("SplitLayout") {
        // `surfaces`는 모델의 `Tab::all_surface_ids()`다. layout이 아직 초기화되지 않아
        // 리프가 없을 때도 tab 수는 이 목록으로 센다.
        let count = content
            .get("surfaces")
            .and_then(Value::as_array)
            .map_or(0, |ids| {
                ids.iter()
                    .filter_map(surface_id)
                    .filter(|&id| is_busy(id))
                    .count() as u64
            });
        if let Some(layout) = content.get_mut("layout") {
            annotate_layout(layout, is_busy);
        }
        count
    } else {
        let busy = content.get("id").and_then(surface_id).is_some_and(is_busy);
        if let Some(obj) = content.as_object_mut() {
            obj.insert("busy".into(), json!(busy));
        }
        u64::from(busy)
    }
}

fn annotate_layout(node: &mut Value, is_busy: &dyn Fn(u32) -> bool) {
    match node.get("type").and_then(Value::as_str) {
        Some("Leaf") => {
            let busy = node.get("id").and_then(surface_id).is_some_and(is_busy);
            if let Some(obj) = node.as_object_mut() {
                obj.insert("busy".into(), json!(busy));
            }
        }
        Some("Split") => {
            for key in ["first", "second"] {
                if let Some(child) = node.get_mut(key) {
                    annotate_layout(child, is_busy);
                }
            }
        }
        _ => {}
    }
}

fn surface_id(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|id| u32::try_from(id).ok())
}

#[cfg(test)]
mod tests {
    use super::annotate_tree_busy;
    use serde_json::{Value, json};

    /// pane 513 = 나뉜 tab 513[surface 513 | surface 514], pane 514 = tab 514(surface 515).
    /// tab id와 surface id가 일부러 겹친다.
    fn overlapping_ids_tree() -> Value {
        json!({
            "id": 513, "name": "ws", "active": true, "busy_count": 1,
            "panes": [
                {"id": 513, "focused": true, "tabs": [
                    {"id": 513, "name": "a", "active": true, "surface": {
                        "type": "SplitLayout", "focused_surface": 513, "surfaces": [513, 514],
                        "layout": {"type": "Split", "direction": "horizontal", "ratio": 0.5,
                            "focus_second": true,
                            "first": {"type": "Leaf", "id": 513, "kind": "terminal"},
                            "second": {"type": "Leaf", "id": 514, "kind": "terminal"}}
                    }}
                ]},
                {"id": 514, "focused": false, "tabs": [
                    {"id": 514, "name": "b", "active": true,
                     "surface": {"id": 515, "type": "Terminal", "pty_ready": true}}
                ]}
            ]
        })
    }

    fn busy_in(set: &'static [u32]) -> impl Fn(u32) -> bool {
        move |id| set.contains(&id)
    }

    #[test]
    fn busy_follows_surfaces_not_tab_ids() {
        let mut ws = overlapping_ids_tree();
        annotate_tree_busy(&mut ws, &busy_in(&[514]));

        let p1 = &ws["panes"][0];
        let t1 = &p1["tabs"][0];
        let layout = &t1["surface"]["layout"];
        assert_eq!(layout["first"]["busy"], json!(false));
        assert_eq!(layout["second"]["busy"], json!(true));
        assert!(layout.get("busy").is_none(), "Split 노드에는 busy가 없다");
        assert!(t1["surface"].get("busy").is_none());
        assert!(t1["surface"].get("busy_count").is_none());
        assert_eq!(t1["busy_count"], json!(1));
        assert_eq!(t1["busy"], json!(true));
        assert_eq!(p1["busy_count"], json!(1));

        let p2 = &ws["panes"][1];
        let t2 = &p2["tabs"][0];
        assert_eq!(t2["surface"]["busy"], json!(false));
        assert_eq!(t2["busy_count"], json!(0));
        assert_eq!(
            t2["busy"],
            json!(false),
            "tab id 514가 busy surface id와 겹친다"
        );
        assert_eq!(p2["busy_count"], json!(0));

        assert_eq!(
            ws["busy_count"],
            json!(1),
            "workspace 값은 호출자 값 그대로"
        );
    }

    #[test]
    fn hidden_tab_counts_toward_its_pane() {
        let mut ws = json!({"id": 1, "busy_count": 1, "panes": [
            {"id": 1, "focused": true, "tabs": [
                {"id": 1, "active": true, "surface": {"id": 10, "type": "Terminal"}},
                {"id": 2, "active": false, "surface": {"id": 11, "type": "Terminal"}}
            ]}
        ]});
        annotate_tree_busy(&mut ws, &busy_in(&[11]));
        let pane = &ws["panes"][0];
        assert_eq!(pane["tabs"][0]["busy_count"], json!(0));
        assert_eq!(pane["tabs"][1]["busy_count"], json!(1));
        assert_eq!(pane["tabs"][1]["surface"]["busy"], json!(true));
        assert_eq!(pane["busy_count"], json!(1));
    }

    #[test]
    fn nothing_busy_gives_zero_everywhere() {
        let mut ws = overlapping_ids_tree();
        annotate_tree_busy(&mut ws, &busy_in(&[]));
        let t1 = &ws["panes"][0]["tabs"][0];
        assert_eq!(t1["surface"]["layout"]["first"]["busy"], json!(false));
        assert_eq!(t1["surface"]["layout"]["second"]["busy"], json!(false));
        for pane in ws["panes"].as_array().expect("panes") {
            assert_eq!(pane["busy_count"], json!(0));
            for tab in pane["tabs"].as_array().expect("tabs") {
                assert_eq!(tab["busy_count"], json!(0));
                assert_eq!(tab["busy"], json!(false));
            }
        }
    }

    #[test]
    fn pending_surface_is_a_leaf_and_not_busy() {
        let mut ws = json!({"id": 1, "busy_count": 0, "panes": [
            {"id": 1, "focused": true, "tabs": [
                {"id": 1, "active": true, "surface": {"id": 7, "type": "Pending"}}
            ]}
        ]});
        annotate_tree_busy(&mut ws, &busy_in(&[]));
        let tab = &ws["panes"][0]["tabs"][0];
        assert_eq!(tab["surface"]["busy"], json!(false));
        assert_eq!(tab["busy_count"], json!(0));
    }

    #[test]
    fn split_without_layout_counts_from_the_surface_list() {
        let mut ws = json!({"id": 1, "busy_count": 1, "panes": [
            {"id": 1, "focused": true, "tabs": [
                {"id": 1, "active": true, "surface": {
                    "type": "SplitLayout", "focused_surface": 3, "surfaces": [3, 4]}}
            ]}
        ]});
        annotate_tree_busy(&mut ws, &busy_in(&[4]));
        let tab = &ws["panes"][0]["tabs"][0];
        assert_eq!(tab["busy_count"], json!(1));
        assert_eq!(ws["panes"][0]["busy_count"], json!(1));
    }

    /// 엔진이 만든 실제 트리에서도 리프·tab·pane·workspace 값이 같은 surface 상태를 따른다.
    #[test]
    fn engine_tree_reports_the_busy_surface_at_every_level() {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let sid = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        engine.live.busy_surfaces.insert(sid);
        let active = state.active_workspace_index(&engine);
        let tree = super::super::build_engine_tree(&state.navigation, active, &engine.read());

        let ws = &tree[0];
        let pane = &ws["panes"][0];
        let tab = &pane["tabs"][0];
        assert_eq!(tab["surface"]["id"], json!(sid));
        assert_eq!(tab["surface"]["busy"], json!(true));
        assert_eq!(tab["busy_count"], json!(1));
        assert_eq!(tab["busy"], json!(true));
        assert_eq!(pane["busy_count"], json!(1));
        assert_eq!(ws["busy_count"], json!(1));
    }
}
