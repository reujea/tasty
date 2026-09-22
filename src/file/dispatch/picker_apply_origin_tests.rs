use super::tests::build_test_core;
use super::{apply_file_picker_result, apply_identify_result};
use crate::core::intent::DomainIntent;
use crate::file::dispatch::{
    DispatchTarget, FileDispatchOrigin, execute_handler_action, open_surface_tab,
};
use crate::file::format::{DetectorId, FileTarget};
use crate::file::handler::{FileHandler, HandlerAction, HandlerId, HandlerOwner};
use crate::state::FileHandlerPickerResult;

fn handler(action: HandlerAction) -> FileHandler {
    FileHandler {
        id: HandlerId::new("host/origin-test"),
        detector: DetectorId::new("origin-test"),
        priority: 0,
        owner: HandlerOwner::Host,
        action,
        display_name_i18n_key: None,
        disabled: false,
    }
}

#[test]
fn delayed_picker_selection_uses_origin_pane_after_active_workspace_changes() {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let sid = engine.workspaces[0].all_surface_ids()[0];
    let pane = engine.find_pane_for_surface(sid).unwrap();
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.file_handler.as_ref(),
        "com.example.origin",
        &[
            serde_json::json!({"id":"open", "detector":"origin-test", "priority":0,
            "action":{"kind":"open_surface", "surface_kind":"empty", "param_key":"file"}}),
        ],
    );
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/unknown"),
        None,
        Some(sid),
        FileDispatchOrigin::Agent,
        false,
    );
    let picker = state.dialogs.file_handler_picker.take().unwrap();
    core.apply(
        &mut engine,
        DomainIntent::CreateWorkspace {
            cwd: None,
            kind: "terminal".into(),
            surface_params: serde_json::json!({}),
            name: None,
            subtitle: None,
            description: None,
            category: None,
        },
    )
    .unwrap();
    state.active_workspace = 1;
    let before = engine.workspaces[0].all_surface_ids();
    let active_tab = engine.find_pane_by_id(pane).unwrap().active_tab;
    let focused_surface = state.focused_surface_id(&engine);
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        picker.target,
        FileHandlerPickerResult::Selected(HandlerId::new("com.example.origin/open")),
        picker.origin_surface_id,
        picker.dispatch_origin,
        picker.ignore_size_limit,
    );
    let added = engine.workspaces[0]
        .all_surface_ids()
        .into_iter()
        .find(|id| !before.contains(id))
        .unwrap();
    assert_eq!(engine.find_pane_for_surface(added), Some(pane));
    assert_eq!(state.active_workspace, 1);
    assert_eq!(engine.find_pane_by_id(pane).unwrap().active_tab, active_tab);
    assert_eq!(state.focused_surface_id(&engine), focused_surface);
    assert!(state.pending_intents.is_empty());
}

#[test]
fn a_dead_origin_cannot_execute_any_action_or_enqueue_a_new_tab() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    for action in [
        HandlerAction::OpenSurface {
            surface_kind: "empty".into(),
            param_key: "file".into(),
        },
        HandlerAction::Ipc {
            method: "example.open".into(),
            owner_plugin_id: "example".into(),
        },
        HandlerAction::System,
    ] {
        assert!(!execute_handler_action(
            &mut core,
            &mut state,
            &mut engine,
            &handler(action),
            &DispatchTarget::File(FileTarget::new("/missing")),
            Some(u32::MAX),
            FileDispatchOrigin::Agent,
            false
        ));
        assert!(state.pending_intents.is_empty());
        assert!(state.pending_handler_ipc.is_empty());
    }
    assert!(!open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "empty",
        serde_json::json!({}),
        Some(u32::MAX),
        FileDispatchOrigin::Agent
    ));
    assert!(state.pending_intents.is_empty());
}

#[test]
fn no_origin_retains_the_user_new_tab_path_and_failed_creation_is_not_success() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let sid = engine.workspaces[0].all_surface_ids()[0];
    assert!(!open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "missing-kind",
        serde_json::json!({}),
        Some(sid),
        FileDispatchOrigin::User
    ));
    assert!(state.pending_intents.is_empty());
    assert!(open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "empty",
        serde_json::json!({}),
        None,
        FileDispatchOrigin::User
    ));
    assert_eq!(state.pending_intents.len(), 1);
}

#[test]
fn identify_and_picker_keep_origin_and_cancel_or_disappearance_do_not_dispatch() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let sid = engine.workspaces[0].all_surface_ids()[0];
    let pane = engine.find_pane_for_surface(sid).unwrap();
    let target = FileTarget::new("/missing/unknown");
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target.clone(),
        None,
        Some(sid),
        FileDispatchOrigin::Agent,
        true,
    );
    let picker = state.dialogs.file_handler_picker.take().unwrap();
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert!(picker.ignore_size_limit);
    let recent_before = engine.file_handler_recent.list().len();
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        picker.target,
        FileHandlerPickerResult::Cancelled,
        picker.origin_surface_id,
        picker.dispatch_origin,
        picker.ignore_size_limit,
    );
    assert!(state.pending_intents.is_empty());
    assert_eq!(engine.file_handler_recent.list().len(), recent_before);

    // Preserve the pane while removing the explicitly named origin.
    core.apply(
        &mut engine,
        DomainIntent::CreateTab {
            pane_id: pane,
            cwd: None,
            kind: "empty".into(),
            name: None,
            surface_params: serde_json::json!({}),
            activate: true,
        },
    )
    .unwrap();
    core.apply(
        &mut engine,
        DomainIntent::CloseSurface {
            surface_id: sid,
            save_snapshot: false,
        },
    )
    .unwrap();
    assert!(!engine.has_surface(sid));
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target.clone(),
        None,
        Some(sid),
        FileDispatchOrigin::Agent,
        false,
    );
    assert!(state.dialogs.file_handler_picker.is_none());
    let h = engine
        .file_handler
        .all_handlers()
        .into_iter()
        .next()
        .unwrap();
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        DispatchTarget::File(target),
        FileHandlerPickerResult::Selected(h.id),
        Some(sid),
        FileDispatchOrigin::Agent,
        false,
    );
    assert!(state.pending_intents.is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    assert_eq!(engine.file_handler_recent.list().len(), recent_before);
}

/// ADR-0279 의 축 — **에이전트** 가 명시 origin 으로 연 결과는 선택하지 않는다. 비동기
/// 완료가 사용자가 보고 있던 탭을 갈아치우면 안 되기 때문이다. 사용자 경로는 반대이고
/// 그것은 [`a_user_origin_selects_its_result_tab`] 이 고정한다(ADR-0302).
#[test]
fn agent_origin_preserves_the_selected_tab_even_when_origin_is_inactive() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let origin = engine.workspaces[0].all_surface_ids()[0];
    let pane_id = engine.find_pane_for_surface(origin).unwrap();
    // Select the new tab so the saved selection differs from the origin.
    core.apply(
        &mut engine,
        DomainIntent::CreateTab {
            pane_id,
            cwd: None,
            kind: "empty".into(),
            name: None,
            surface_params: serde_json::json!({}),
            activate: true,
        },
    )
    .unwrap();
    assert_eq!(engine.find_pane_by_id(pane_id).unwrap().active_tab, 1);
    let focused_surface = state.focused_surface_id(&engine);
    assert_ne!(focused_surface, Some(origin));
    for kind in ["empty", "terminal", "missing-kind"] {
        let before = engine.find_pane_by_id(pane_id).unwrap();
        let selected_id = before.tabs[before.active_tab].id;
        let count = before.tabs.len();
        let succeeded = open_surface_tab(
            &mut core,
            &mut state,
            &mut engine,
            kind,
            serde_json::json!({}),
            Some(origin),
            FileDispatchOrigin::Agent,
        );
        assert_eq!(succeeded, kind != "missing-kind");
        let after = engine.find_pane_by_id(pane_id).unwrap();
        assert_eq!(after.tabs.len(), count + usize::from(succeeded));
        assert_eq!(after.tabs[after.active_tab].id, selected_id, "{kind}");
        assert_eq!(state.focused_surface_id(&engine), focused_surface, "{kind}");
        assert!(state.pending_intents.is_empty());
    }
}

/// ADR-0302 — 사용자가 자기 손으로 연 결과는 **선택된다.** explorer 더블클릭이 이 경로이고,
/// 같은 pane 에 붙는다는 라우팅 계약(ADR-0279)은 그대로다. 두 단정이 함께 있어야 한다 —
/// 선택만 보면 후보 B(origin 을 버려 focused pane 으로 보내기)도 통과한다.
#[test]
fn a_user_origin_selects_its_result_tab() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let origin = engine.workspaces[0].all_surface_ids()[0];
    let pane_id = engine.find_pane_for_surface(origin).unwrap();
    let before = engine.find_pane_by_id(pane_id).unwrap();
    let selected_before = before.tabs[before.active_tab].id;
    let count = before.tabs.len();

    assert!(open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "empty",
        serde_json::json!({}),
        Some(origin),
        FileDispatchOrigin::User,
    ));

    let after = engine.find_pane_by_id(pane_id).unwrap();
    assert_eq!(
        after.tabs.len(),
        count + 1,
        "origin 의 pane 에 하나 늘어야 한다"
    );
    assert_ne!(
        after.tabs[after.active_tab].id, selected_before,
        "사용자가 연 결과는 선택돼야 한다"
    );
    assert_eq!(
        after.tabs[after.active_tab].id,
        after.tabs[after.tabs.len() - 1].id,
        "선택은 방금 append 된 탭이어야 한다"
    );
    // 라우팅은 안 바뀐다 — `Intent::NewTab` 으로 위임되지 않았다(그쪽은 focused pane 을 고른다).
    assert!(state.pending_intents.is_empty());
}
