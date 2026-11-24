//! egui 패스 전에 디코드하는 plugin 팝업 본문은 합성할 팝업과 같은 집합이다. 접힌 팝업과 배치되지 않은 팝업은
//! 합성 영역을 받지 않으므로 디코드하지 않는다. 디코드한 팝업을 같은 프레임 합성이 지우면 delta 프레임마다 전체
//! 재전송을 요청해 매 프레임 다시 그리기 때문이다.

use egui::Context;
use tasty_plugin_manifest::PopupSizeHint;

use super::frame_keys::{POPUP, plugin_only_scene, plugin_popup, render_frame_on};
use crate::plugin_bridge::popup_scope::bodies_to_decode;

/// surface R 보다 커서 보기 버튼으로 접히는 팝업.
const FOLDED: u64 = 10;
/// 이 창 배치에 없는 surface 범위라 배치되지 않는 팝업.
const UNPLACED: u64 = 11;

/// Given 펼친 팝업 P, 접힌 팝업, 배치되지 않은 팝업이 열려 있을 때, When 두 프레임을 그리면, Then 각 프레임 전에
/// 디코드하는 본문은 P 뿐이고 그 프레임의 합성 영역에 모두 든다. 합성이 디코드한 자원을 지우지 않으므로 전체
/// 재전송 요청과 다시 그리기가 생기지 않는다.
#[test]
fn only_the_composited_popups_are_decoded_before_the_pass() {
    let (mut state, mut session, mut mgr) = plugin_only_scene();
    let close = tasty_plugin_protocol::popup_decl::PopupEscape::Close;
    let mut folded = plugin_popup(tasty_host_plugin::next_popup_z_seq(), close);
    folded.contribute.size_hint = Some(PopupSizeHint {
        width: 2000,
        height: 2000,
    });
    mgr.insert_popup_instance_for_test(FOLDED, folded);
    let mut unplaced = plugin_popup(tasty_host_plugin::next_popup_z_seq(), close);
    unplaced.scope_surface = Some(999);
    mgr.insert_popup_instance_for_test(UNPLACED, unplaced);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let mut decoded_per_frame = Vec::new();
    for _ in 0..3 {
        let decode = bodies_to_decode(
            &state.plugin_mesh_popup_regions,
            mgr.popup_instances().map(|(id, _)| id),
        );
        render_frame_on(&ctx, &mut state, &engine.read(), &mgr, false, &|_| {});
        let composited: Vec<u64> = state
            .plugin_mesh_popup_regions
            .iter()
            .map(|r| r.0)
            .collect();
        assert!(
            decode.iter().all(|id| composited.contains(id)),
            "디코드한 본문 {decode:?} 은 이번 프레임 합성 영역 {composited:?} 에 들어야 한다"
        );
        assert_eq!(composited, vec![POPUP], "합성 영역은 펼친 P 뿐이다");
        decoded_per_frame.push(decode);
    }
    assert!(
        state
            .plugin_popup_key_candidates
            .iter()
            .any(|(id, _)| *id == FOLDED),
        "전제: 접힌 팝업도 배치는 된다(보기 버튼)"
    );
    assert_eq!(
        decoded_per_frame,
        vec![vec![], vec![POPUP], vec![POPUP]],
        "첫 프레임은 직전 합성 영역이 없고, 그 뒤로는 P 만 디코드한다"
    );
}
