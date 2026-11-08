//! 플러그인 팝업의 자식 피커 정리와 Escape 대상 선택을 검사한다.

use super::tests::test_state;
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::PopupManager;
use crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::{PopupScope, defs};
use crate::app::dispatch::plugin_popup_events::cancel_child_file_picker;
use crate::state::{
    FilePickerData, FilePickerRequester, FilePickerResult, FpLoadState, RequestContext,
};
use tasty_plugin_protocol::PopupCloseReason;

const OWNER_IID: u64 = 7;
const PORT_SCANNER_ID: &str = "port_scanner";

/// owner가 없으면 플러그인이 아닌 도구 메뉴에서 연 피커를 구성한다.
fn state_with_picker(owner: Option<u64>) -> RequestContext {
    let (mut state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    state.dialogs.file_picker = Some(FilePickerData {
        save_name: None,
        request_id: crate::state::FilePickerRequestId(1),
        mirror_ws_id: None,
        remote_host: None,
        current_dir: "/tmp".to_string(),
        load: FpLoadState::Empty,
        local_query: None,
        entries: Vec::new(),
        selected: Vec::new(),
        result: None,
        requester: owner.map(|iid| FilePickerRequester {
            plugin_id: "com.example.any".to_string(),
            request_id: 1,
            owner_popup_instance: Some(iid),
            origin_surface_id: None,
        }),
        filters: Vec::new(),
    });
    state
}

#[test]
fn owner_instance_is_reported_as_having_an_open_child() {
    let state = state_with_picker(Some(OWNER_IID));
    assert!(state.plugin_popup_has_open_child(OWNER_IID));
    assert!(!state.plugin_popup_has_open_child(OWNER_IID + 1));
}

#[test]
fn tools_menu_picker_is_nobodys_child() {
    let state = state_with_picker(None);
    assert!(!state.plugin_popup_has_open_child(OWNER_IID));
}

#[test]
fn no_picker_open_means_no_child() {
    let (state, mut _engine_session) = test_state();
    let _engine = _engine_session.borrow_mut();
    assert!(!state.plugin_popup_has_open_child(OWNER_IID));
}

#[test]
fn closing_the_owner_cancels_its_child_picker() {
    let mut state = state_with_picker(Some(OWNER_IID));
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::OutsideClick)]);
    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(FilePickerResult::Cancelled)
    ));
}

// 닫기 사유별 직접 호출을 검사한다. 각 진입점이 이 함수를 호출하는지는
// crates/tasty-doc-guards/tests/plugin_popup_close_chokepoint.rs가 소스로 검사한다.
#[test]
fn cascade_cleanup_is_independent_of_the_close_reason() {
    for reason in [
        PopupCloseReason::Escape,
        PopupCloseReason::OutsideClick,
        PopupCloseReason::PluginRequest,
        PopupCloseReason::HostShutdown,
    ] {
        let mut state = state_with_picker(Some(OWNER_IID));
        cancel_child_file_picker(&mut state, &[(OWNER_IID, reason)]);
        assert!(
            state.dialogs.file_picker.as_ref().unwrap().result.is_some(),
            "close reason {reason:?} should still clean up the child"
        );
    }
}

#[test]
fn closing_an_unrelated_plugin_popup_leaves_the_picker_alone() {
    let mut state = state_with_picker(Some(OWNER_IID));
    cancel_child_file_picker(&mut state, &[(OWNER_IID + 1, PopupCloseReason::Escape)]);
    assert!(state.dialogs.file_picker.as_ref().unwrap().result.is_none());
}

#[test]
fn cascade_cleanup_does_not_overwrite_a_settled_result() {
    let mut state = state_with_picker(Some(OWNER_IID));
    state.dialogs.file_picker.as_mut().unwrap().result = Some(FilePickerResult::Confirmed {
        paths: vec!["/tmp/a.md".to_string()],
    });
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::Escape)]);
    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(FilePickerResult::Confirmed { .. })
    ));
}

// 취소 정리에서는 부모 링크를 지워 정상 정리가 결과 유실 경고로 보고되지 않게 한다.
#[test]
fn successful_cascade_cleanup_severs_the_owner_link() {
    let mut state = state_with_picker(Some(OWNER_IID));
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::PluginRequest)]);
    let req = state
        .dialogs
        .file_picker
        .as_ref()
        .unwrap()
        .requester
        .as_ref()
        .unwrap();
    assert_eq!(req.owner_popup_instance, None);
    assert_eq!(req.request_id, 1);
    assert_eq!(req.plugin_id, "com.example.any");
}

// 이미 확정된 결과는 부모 링크를 남겨 수신 팝업 부재를 알릴 수 있어야 한다.
#[test]
fn a_settled_result_keeps_the_owner_link_for_the_loss_warning() {
    let mut state = state_with_picker(Some(OWNER_IID));
    state.dialogs.file_picker.as_mut().unwrap().result = Some(FilePickerResult::Confirmed {
        paths: vec!["/tmp/a.md".to_string()],
    });
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::PluginRequest)]);
    assert_eq!(
        state
            .dialogs
            .file_picker
            .as_ref()
            .unwrap()
            .requester
            .as_ref()
            .unwrap()
            .owner_popup_instance,
        Some(OWNER_IID)
    );
}

#[test]
fn severed_link_stops_reporting_an_open_child() {
    let mut state = state_with_picker(Some(OWNER_IID));
    assert!(state.plugin_popup_has_open_child(OWNER_IID));
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::Escape)]);
    assert!(!state.plugin_popup_has_open_child(OWNER_IID));
}

#[test]
fn tools_menu_picker_survives_plugin_popup_closes() {
    let mut state = state_with_picker(None);
    cancel_child_file_picker(&mut state, &[(OWNER_IID, PopupCloseReason::Escape)]);
    assert!(state.dialogs.file_picker.as_ref().unwrap().result.is_none());
}

fn registered_manager() -> PopupManager {
    let mut mgr = PopupManager::new();
    for def in defs::all_defs() {
        mgr.register_def(def, 1.0);
    }
    mgr
}

fn layout_ctx(active_workspace_id: u32) -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id,
        pane_rects: Vec::new(),
        surface_rects: Vec::new(),
        active_tabs: Vec::new(),
        active_tab_ids: Vec::new(),
    }
}

#[test]
fn topmost_visible_open_picks_the_highest_z() {
    let mut mgr = registered_manager();
    mgr.open(FILE_PICKER_POPUP_ID);
    mgr.open(PORT_SCANNER_ID);
    let (id, _) = mgr
        .topmost_visible_open(None)
        .expect("two popups are open, one must be topmost");
    assert_eq!(id, PORT_SCANNER_ID, "later open wins");

    mgr.close(PORT_SCANNER_ID);
    let (id, _) = mgr.topmost_visible_open(None).unwrap();
    assert_eq!(id, FILE_PICKER_POPUP_ID);
}

#[test]
fn topmost_visible_open_is_none_without_open_popups() {
    let mgr = registered_manager();
    assert!(mgr.topmost_visible_open(None).is_none());
}

#[test]
fn scope_hidden_popup_is_not_the_escape_candidate() {
    let mut mgr = registered_manager();
    mgr.open(FILE_PICKER_POPUP_ID);
    mgr.open_with_scope(PORT_SCANNER_ID, PopupScope::Workspace(9));
    let ctx = layout_ctx(0);
    let (id, _) = mgr.topmost_visible_open(Some(&ctx)).unwrap();
    assert_eq!(id, FILE_PICKER_POPUP_ID);
    let ctx = layout_ctx(9);
    let (id, _) = mgr.topmost_visible_open(Some(&ctx)).unwrap();
    assert_eq!(id, PORT_SCANNER_ID);
}
