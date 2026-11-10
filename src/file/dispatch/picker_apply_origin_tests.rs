use super::tests::build_test_core;
use super::{apply_file_picker_result, apply_identify_result};
use crate::app::command::DomainIntent;
use crate::file::dispatch::{
    DispatchTarget, FileDispatchOrigin, execute_handler_action, open_surface_tab,
};
use crate::file::format::{DetectorId, FileTarget};
use crate::file::handler::{FileHandler, HandlerAction, HandlerId, HandlerOwner};
use crate::state::FileHandlerPickerResult;

fn fixture() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let mut events = vec![E::CategoryCreated {
        id: 0,
        name: "normal".into(),
        index: 0,
    }];
    for id in 1..=2 {
        events.push(E::WorkspaceCreated {
            id,
            name: format!("workspace {id}"),
            category: 0,
            index: (id - 1) as usize,
            pane: id,
        });
        events.push(E::TabCreated {
            id,
            pane: id,
            index: 0,
            name: "terminal".into(),
            surface: SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        });
    }
    events.push(E::TabCreated {
        id: 3,
        pane: 1,
        index: 1,
        name: "extra".into(),
        surface: SurfaceSpec {
            id: 3,
            kind: "empty".into(),
            data: None,
        },
    });
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(events))
}

fn take_creation(state: &mut crate::state::RequestContext, pane: u32, activate: bool) {
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].origin.is_user(), activate);
    assert!(
        matches!(&intents[0].body, crate::intent::Intent::Domain(DomainIntent::CreateTab { pane_id, activate: selected, .. }) if *pane_id == pane && *selected == activate)
    );
}

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

/// 식별 결과가 picker를 열면 요청 출처를 그대로 실은 OpenPopup 하나만 큐에 남는다.
fn take_picker_open_request(state: &mut crate::state::RequestContext, agent: bool) {
    let pending = state.take_pending_intents();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        &pending[0].body,
        crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup { id, .. })
            if *id == crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID
    ));
    assert_eq!(pending[0].origin.is_agent(), agent);
}

#[test]
fn delayed_picker_selection_uses_origin_pane_after_active_workspace_changes() {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let pane = engine.find_pane_for_surface(sid).unwrap();
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
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
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    let picker = state.dialogs.file_handler_picker.take().unwrap();
    take_picker_open_request(&mut state, false);
    state.set_active_workspace_index(&engine, 1);
    let before = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids();
    let active_tab = state
        .navigation
        .tab_index(engine.find_pane_by_id(pane).unwrap());
    let focused_surface = state.focused_surface_id(&engine);
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        picker.target,
        FileHandlerPickerResult::Selected(HandlerId::new("com.example.origin/open")),
        picker.origin_surface_id,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: picker.dispatch_origin,
            ignore_size_limit: picker.ignore_size_limit,
            run: None,
        },
    );
    take_creation(&mut state, pane, true);
    assert_eq!(engine.workspace_at(0).unwrap().all_surface_ids(), before);
    assert_eq!(
        state
            .navigation
            .tab_index(engine.find_pane_by_id(pane).unwrap()),
        active_tab
    );
    assert_eq!(state.active_workspace_index(&engine), 1);
    assert_eq!(state.focused_surface_id(&engine), focused_surface);
}

#[test]
fn a_dead_origin_cannot_execute_any_action_or_enqueue_a_new_tab() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
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
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: FileDispatchOrigin::Agent,
                ignore_size_limit: false,
                run: None,
            }
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
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::Agent,
            ignore_size_limit: false,
            run: None,
        }
    ));
    assert!(state.pending_intents.is_empty());
}

#[test]
fn no_origin_queues_the_user_new_tab_request() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    assert!(open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "empty",
        serde_json::json!({}),
        None,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        }
    ));
    let pending = state.take_pending_intents();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].origin.is_user());
    assert!(
        matches!(&pending[0].body, crate::intent::Intent::NewTab { kind: Some(kind), params }
        if kind == "empty" && *params == serde_json::json!({}))
    );
}

#[test]
fn identify_and_picker_keep_origin_and_cancel_or_missing_target_do_not_dispatch() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let target = FileTarget::new("/missing/unknown");
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target.clone(),
        None,
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: true,
            run: None,
        },
    );
    let picker = state.dialogs.file_handler_picker.take().unwrap();
    take_picker_open_request(&mut state, false);
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert!(picker.ignore_size_limit);
    let recent_before = state.file_handler_recent.list().len();
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        picker.target,
        FileHandlerPickerResult::Cancelled,
        picker.origin_surface_id,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: picker.dispatch_origin,
            ignore_size_limit: picker.ignore_size_limit,
            run: None,
        },
    );
    assert!(state.pending_intents.is_empty());
    assert_eq!(state.file_handler_recent.list().len(), recent_before);

    // A missing explicit origin must not fall back to the focused pane.
    let sid = u32::MAX;
    assert!(!engine.has_surface(sid));
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target.clone(),
        None,
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(state.dialogs.file_handler_picker.is_none());
    let h = engine
        .runtime
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
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::Agent,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(state.pending_intents.is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    assert_eq!(state.file_handler_recent.list().len(), recent_before);
}

/// 에이전트가 연 결과는 원래 사용자 선택을 유지해야 한다.
#[test]
fn agent_origin_preserves_the_selected_tab_even_when_origin_is_inactive() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let origin = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let pane_id = engine.find_pane_for_surface(origin).unwrap();
    state.goto_tab_in_pane(&engine, 1);
    let focused_surface = state.focused_surface_id(&engine);
    assert_ne!(focused_surface, Some(origin));
    for kind in ["empty", "terminal"] {
        let before = engine.find_pane_by_id(pane_id).unwrap();
        let selected_id = before.tabs[state.navigation.tab_index(before)].id;
        let count = before.tabs.len();
        let succeeded = open_surface_tab(
            &mut core,
            &mut state,
            &mut engine,
            kind,
            serde_json::json!({}),
            Some(origin),
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: FileDispatchOrigin::Agent,
                ignore_size_limit: false,
                run: None,
            },
        );
        assert!(succeeded);
        take_creation(&mut state, pane_id, false);
        let after = engine.find_pane_by_id(pane_id).unwrap();
        assert_eq!(after.tabs.len(), count);
        assert_eq!(
            after.tabs[state.navigation.tab_index(after)].id,
            selected_id,
            "{kind}"
        );
        assert_eq!(state.focused_surface_id(&engine), focused_surface, "{kind}");
        assert!(state.pending_intents.is_empty());
    }
}

/// 사용자 결과는 선택돼야 하며 origin의 pane에 추가돼야 한다. 선택만 보면 잘못된 pane을 놓친다.
#[test]
fn a_user_origin_requests_activation_in_the_original_pane() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let origin = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let pane_id = engine.find_pane_for_surface(origin).unwrap();
    let before = engine.find_pane_by_id(pane_id).unwrap();
    let selected_before = before.tabs[state.navigation.tab_index(before)].id;
    let count = before.tabs.len();

    assert!(open_surface_tab(
        &mut core,
        &mut state,
        &mut engine,
        "empty",
        serde_json::json!({}),
        Some(origin),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        }
    ));

    take_creation(&mut state, pane_id, true);
    let after = engine.find_pane_by_id(pane_id).unwrap();
    assert_eq!(after.tabs.len(), count);
    assert_eq!(
        after.tabs[state.navigation.tab_index(after)].id,
        selected_before
    );
}

#[test]
fn user_dispatch_and_remote_placeholder_open_the_picker_as_user_requests() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/unknown"),
        None,
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    take_picker_open_request(&mut state, false);

    crate::file::dispatch::open_remote_placeholder_picker(
        &mut state,
        FileTarget::new("/remote/a.md"),
        "gx10".into(),
    );
    take_picker_open_request(&mut state, false);
}

/// 매칭 핸들러가 없어도 에이전트 요청은 사용자 화면에 picker를 띄우지 않고 아무것도 실행하지 않는다.
#[test]
fn agent_dispatch_without_a_matching_handler_opens_no_picker() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let recent_before = state.file_handler_recent.list().len();
    for origin_surface_id in [Some(sid), None] {
        for detector in [None, Some(DetectorId::new("no-such-detector"))] {
            apply_identify_result(
                &mut core,
                &mut state,
                &mut engine,
                FileTarget::new("/unknown"),
                detector,
                origin_surface_id,
                crate::file::dispatch::FileDispatchPolicy {
                    dispatch_origin: FileDispatchOrigin::Agent,
                    ignore_size_limit: false,
                    run: None,
                },
            );
            assert!(state.dialogs.file_handler_picker.is_none());
            assert!(state.pending_intents.is_empty());
            assert!(state.pending_handler_ipc.is_empty());
        }
    }
    assert_eq!(state.file_handler_recent.list().len(), recent_before);
}

/// 사용자 입력을 증명하지 못한 plugin 중계 요청은 사용자 클릭일 수 있어 무매칭이면 fallback picker를 연다.
/// 확정해도 출처는 그대로라 결과 탭으로 선택을 옮기지 않는다.
#[test]
fn an_unverified_plugin_dispatch_without_a_matching_handler_opens_the_fallback_picker() {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = fixture();
    let mut engine = engine_session.borrow_mut();
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
        "com.example.picker",
        &[
            serde_json::json!({"id": "open", "detector": "picker-test", "priority": 0,
            "action": {"kind": "open_surface", "surface_kind": "empty", "param_key": "file"}}),
        ],
    );
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    for detector in [None, Some(DetectorId::new("no-such-detector"))] {
        apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            FileTarget::new("/unknown"),
            detector,
            Some(sid),
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: FileDispatchOrigin::PluginUnverified,
                ignore_size_limit: false,
                run: None,
            },
        );
        let picker = state
            .dialogs
            .file_handler_picker
            .take()
            .expect("unverified plugin dispatch opens the fallback picker");
        assert!(picker.candidates_are_fallback);
        assert_eq!(picker.dispatch_origin, FileDispatchOrigin::PluginUnverified);
        assert_eq!(picker.origin_surface_id, Some(sid));
        take_picker_open_request(&mut state, true);
    }

    let pane_id = engine.find_pane_for_surface(sid).unwrap();
    let before = engine.find_pane_by_id(pane_id).unwrap();
    let selected_id = before.tabs[state.navigation.tab_index(before)].id;
    let count = before.tabs.len();
    let picked = engine
        .runtime
        .file_handler
        .all_handlers()
        .into_iter()
        .find(|h| {
            matches!(&h.action, HandlerAction::OpenSurface { surface_kind, .. }
                if surface_kind == "empty")
        })
        .expect("installed open_surface handler");
    apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        DispatchTarget::File(FileTarget::new("/unknown")),
        FileHandlerPickerResult::Selected(picked.id),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::PluginUnverified,
            ignore_size_limit: false,
            run: None,
        },
    );
    take_creation(&mut state, pane_id, false);
    let after = engine.find_pane_by_id(pane_id).unwrap();
    assert_eq!(after.tabs.len(), count, "생성은 App에 요청한다");
    assert_eq!(
        after.tabs[state.navigation.tab_index(after)].id,
        selected_id,
        "증명하지 못한 요청의 결과는 사용자 선택을 옮기지 않는다"
    );
}
