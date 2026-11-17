//! debug host_popup 은 같은 선언의 인스턴스를 id 로 구분해 보고하고 하나만 닫을 수 있다.

use super::{handle_debug_host_popup_close, handle_debug_host_popup_list};
use crate::adapters::ui::popup::convert::{CONVERT_POPUP_ID, open_request};
use serde_json::json;

fn convert_instances(state: &crate::state::RequestContext) -> Vec<(u64, u64)> {
    let listed = handle_debug_host_popup_list(state, json!(1));
    let popups = listed.result.expect("list")["popups"].clone();
    let convert = popups
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == CONVERT_POPUP_ID)
        .expect("convert listed")
        .clone();
    convert["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            assert_eq!(i["scope_kind"], "surface");
            (
                i["instance"].as_u64().unwrap(),
                i["scope_target"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// Given 두 surface 에 변환 팝업이 따로 열림, Then 목록이 인스턴스 둘을 서로 다른 id 와 대상으로 보고한다.
/// When 한 인스턴스 id 로 닫으면, Then 그 인스턴스만 닫힌다. 없는 id 는 -32602 다.
#[test]
fn host_popup_list_reports_each_instance_and_close_takes_one() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let ids = session
        .borrow_mut()
        .workspace_at(0)
        .unwrap()
        .all_surface_ids();
    let (left, right) = (ids[0], ids[1]);
    crate::intent::popup::open_for_test(&mut state, &open_request(left), true);
    crate::intent::popup::open_for_test(&mut state, &open_request(right), true);

    let listed = convert_instances(&state);
    let targets: Vec<u64> = listed.iter().map(|(_, t)| *t).collect();
    assert_eq!(targets, [u64::from(left), u64::from(right)]);
    assert_ne!(listed[0].0, listed[1].0);

    let (left_instance, _) = listed[0];
    let closed = handle_debug_host_popup_close(
        &mut state,
        json!(2),
        &json!({ "popup_id": CONVERT_POPUP_ID, "instance": left_instance }),
    );
    assert_eq!(closed.result.expect("closed")["instance"], left_instance);
    crate::intent::popup::apply_pending(&mut state, &mut session);
    assert_eq!(convert_instances(&state), [listed[1]]);

    let missing = handle_debug_host_popup_close(
        &mut state,
        json!(3),
        &json!({ "popup_id": CONVERT_POPUP_ID, "instance": left_instance }),
    );
    assert_eq!(missing.error.map(|e| e.code), Some(-32602));
}
