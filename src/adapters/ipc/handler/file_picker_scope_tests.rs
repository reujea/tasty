use super::handle_trigger;
use crate::adapters::ui::popup::{PopupScope, file_picker::FILE_PICKER_POPUP_ID};
use crate::adapters::ui::{
    LayoutContext,
    popup::{defs, frame::draw_popup_layer},
};
use crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes;
use crate::state::RequestContext;
use crate::state::tests::test_state;
use serde_json::json;
use tasty_host_plugin::manager::PopupInstance;
use tasty_ipc::caller::CallerContext;

const PLUGIN: &str = "com.example.picker";

fn parent(scope: &str, target: Option<u32>) -> PopupInstance {
    PopupInstance {
        plugin_id: PLUGIN.into(),
        popup_id: "open".into(),
        contribute: serde_json::from_value(json!({
            "id": "open", "trigger": {"kind": "event", "event_key": "test.open"},
            "scope": scope
        }))
        .unwrap(),
        z_seq: 1,
        scope_surface: target,
    }
}

/// 그리기 전 단계처럼 열린 plugin 팝업을 기록한 뒤 `file_picker.trigger`를 보낸다.
/// `make`는 포커스 surface id 로 인스턴스와 요청 인자를 만든다.
fn request(
    make: impl FnOnce(u32) -> (Vec<(u64, PopupInstance)>, serde_json::Value),
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    tasty_ipc::protocol::JsonRpcResponse,
) {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let sid = state.focused_surface_id(&engine.read()).expect("surface");
    let (instances, params) = make(sid);
    sync_plugin_popup_scopes(&mut state, instances.iter().map(|(id, inst)| (*id, inst)));
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = handle_trigger(&mut state, &mut engine, &caller, json!(1), &params);
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    (state, engine_session, response)
}

/// owner 7 이 있는 피커를 연다. owner 는 엔진에 있는 surface 에 결속된 surface 선언 팝업이다.
fn trigger(
    owner: Option<u64>,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let (state, session, response) = request(|sid| {
        (
            vec![(7, parent("surface", Some(sid)))],
            json!({"owner_popup_instance": owner, "origin_surface_id": sid}),
        )
    });
    assert!(response.result.is_some(), "{response:?}");
    (state, session)
}

fn picker_scope(state: &mut RequestContext) -> PopupScope {
    state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .unwrap()
        .scope
        .clone()
}

fn refused(response: &tasty_ipc::protocol::JsonRpcResponse, state: &RequestContext) {
    assert_eq!(response.error.as_ref().map(|e| e.code), Some(-32602));
    assert!(
        crate::adapters::ui::popup::file_picker::instances::latest(state).is_none(),
        "picker opened"
    );
    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
}

/// owner 팝업이 있으면 그 팝업의 선언과 결속 대상으로 범위를 정한다. 대상 없는 surface 선언은 열지 않는다.
#[test]
fn an_owner_gives_its_declared_and_bound_scope() {
    for (decl, target, expected) in [
        ("surface", Some(17), Some(PopupScope::Surface(17))),
        ("surface", None, None),
        ("window", Some(17), Some(PopupScope::Window)),
        ("window", None, Some(PopupScope::Window)),
    ] {
        let (mut state, _, response) = request(|_| {
            (
                vec![(7, parent(decl, target))],
                json!({"owner_popup_instance": 7}),
            )
        });
        match expected {
            Some(scope) => assert_eq!(picker_scope(&mut state), scope, "{decl} {target:?}"),
            None => refused(&response, &state),
        }
    }
}

/// owner 가 없으면 피커는 요청한 surface 범위로 열린다(작용 대상 범위 규칙).
#[test]
fn an_ownerless_request_opens_in_its_origin_surface() {
    let mut origin = 0;
    let (mut state, _, response) = request(|sid| {
        origin = sid;
        (vec![], json!({"origin_surface_id": sid}))
    });
    assert!(response.result.is_some(), "{response:?}");
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(origin));
}

/// owner·origin 이 모두 없거나 가리키는 대상이 없으면 창 범위로 바꾸지 않고 -32602 로 거절한다.
#[test]
fn a_request_without_a_live_target_is_refused() {
    let other = || {
        let mut inst = parent("window", None);
        inst.plugin_id = "com.example.other".into();
        inst
    };
    for case in 0..4 {
        let (state, _, response) = request(|_| match case {
            0 => (vec![], json!({})),
            1 => (vec![], json!({"origin_surface_id": 9999})),
            2 => (vec![], json!({"owner_popup_instance": 7})),
            _ => (vec![(7, other())], json!({"owner_popup_instance": 7})),
        });
        refused(&response, &state);
    }
}

/// owner 가 좁은 surface 에서 창 위로 펼쳐 그려져도 피커는 owner 의 결속 범위를 쓴다.
/// 매 프레임 배치 결과는 범위를 정하지 않는다.
#[test]
fn a_narrow_owner_expanded_over_the_window_still_gives_its_surface_scope() {
    let (mut state, mut session) = test_state();
    let sid = state
        .focused_surface_id(&session.borrow_mut().read())
        .expect("surface");
    let mut inst = parent("surface", Some(sid));
    inst.contribute.size_hint = Some(tasty_plugin_manifest::PopupSizeHint {
        width: 800,
        height: 600,
    });
    let mut mgr = crate::plugin::PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.insert_popup_instance_for_test(7, inst);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let narrow = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 120.0));
    let layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects: vec![(sid, narrow)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    let ctx = egui::Context::default();
    let mut draw = |state: &mut RequestContext| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };
        let engine = session.borrow_mut();
        drop(ctx.run(raw, |ctx| {
            sync_plugin_popup_scopes(state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(&mgr)),
                Some(&layout),
            );
        }));
        state
            .plugin_popup_hittest
            .first()
            .expect("owner drawn")
            .rect
    };
    // 접힌 owner는 좁은 surface 안의 보기 버튼만 그리고 콘텐츠 영역을 합성하지 않는다.
    let collapsed = draw(&mut state);
    assert!(
        narrow.contains_rect(collapsed),
        "the collapsed owner stays in its surface: {collapsed:?}"
    );
    assert!(state.plugin_mesh_popup_regions.is_empty());
    state.popups.expand_plugin(7);
    let placed = draw(&mut state);
    assert!(
        !narrow.contains_rect(placed),
        "the owner must be drawn outside its narrow surface: {placed:?}"
    );
    assert_eq!(state.plugin_mesh_popup_regions.len(), 1);
    let mut engine = session.borrow_mut();
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &json!({"owner_popup_instance": 7}),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    sync_plugin_popup_scopes(&mut state, mgr.popup_instances());
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
}

/// Given 좁은 surface 에서 창 위로 펼친 plugin 부모 팝업, When 찾아보기로 피커를 열면, Then 피커도 펼친 상태다.
/// When 피커에서 Esc 를 하면, Then 부모와 함께 접힌다. When 다시 펼친 부모를 접기 버튼으로 접으면, Then 피커도
/// 접히고(부모 버튼이 대표한다) 키 후보에서 빠진다.
#[test]
fn opening_the_picker_from_an_expanded_parent_popup_opens_it_expanded() {
    let (mut state, mut session) = test_state();
    let sid = state
        .focused_surface_id(&session.borrow_mut().read())
        .expect("surface");
    let mut inst = parent("surface", Some(sid));
    inst.contribute.size_hint = Some(tasty_plugin_manifest::PopupSizeHint {
        width: 800,
        height: 600,
    });
    let mut mgr = crate::plugin::PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.insert_popup_instance_for_test(7, inst);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let narrow = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 120.0));
    let layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects: vec![(sid, narrow)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    let ctx = egui::Context::default();
    // plugin 렌더 뒤 host 팝업을 그린다. host 판정은 직전 plugin 렌더가 적은 부모 표시를 읽는다.
    let frame = |state: &mut RequestContext,
                 session: &mut crate::runtime::engine_session::EngineSession| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };
        let engine = session.borrow_mut();
        drop(ctx.run(raw, |ctx| {
            sync_plugin_popup_scopes(state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(&mgr)),
                Some(&layout),
            );
            let mut popups =
                std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
            popups.draw_routed(ctx, &mut |_, _| {}, Some(&layout), &[], None);
            state.popups = popups;
        }));
        state.press_snapshot = None;
    };
    state.popups.expand_plugin(7);
    frame(&mut state, &mut session);
    {
        let mut engine = session.borrow_mut();
        let caller = CallerContext::Plugin {
            plugin_id: PLUGIN.into(),
        };
        let response = handle_trigger(
            &mut state,
            &mut engine,
            &caller,
            json!(1),
            &json!({"owner_popup_instance": 7}),
        );
        assert!(response.result.is_some(), "{response:?}");
        for intent in state.take_pending_intents() {
            crate::intent::popup::handle(&mut state, &mut engine, &intent);
        }
    }
    frame(&mut state, &mut session);
    let picker = |state: &mut RequestContext| {
        let p = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap();
        (p.expanded(), p.collapsed())
    };
    assert!(
        picker(&mut state).0,
        "the picker opens expanded with its parent"
    );

    // plugin 호출로 연 피커는 passive 다. 사용자가 누르면 포커스를 받아 키 주인이 된다. Esc 는 피커와 함께
    // 펼친 부모도 접는다.
    state.popups.set_focused(FILE_PICKER_POPUP_ID, true);
    assert!(state.refold_key_owner());
    assert!(!state.popups.plugin_expanded().contains(&7));
    frame(&mut state, &mut session);
    frame(&mut state, &mut session);
    let (expanded, collapsed) = picker(&mut state);
    assert!(!expanded && collapsed, "the picker folds with its parent");
    assert_eq!(
        state.popup_key_owner(),
        Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(7)),
        "a picker its parent's button stands for takes no keys"
    );

    // 부모를 다시 펼치면 피커도 펼친다. 부모의 접기 버튼(같은 처리)으로 접으면 피커도 접힌다.
    state.popups.expand_plugin(7);
    frame(&mut state, &mut session);
    frame(&mut state, &mut session);
    assert!(
        picker(&mut state).0,
        "the picker follows its parent out again"
    );
    state.popups.refold_plugin(7);
    frame(&mut state, &mut session);
    frame(&mut state, &mut session);
    let (expanded, collapsed) = picker(&mut state);
    assert!(!expanded && collapsed);

    // 다시 펼치고 피커 제목줄의 접기 버튼(host 접기 버튼 경로)을 누르면 plugin 부모와 피커가 함께 접히고, 다음
    // 프레임에도 접힌 채다. 접기는 그 한 번에 끝나 나중 Esc 가 꺼낼 남은 요청이 없다.
    state.popups.expand_plugin(7);
    frame(&mut state, &mut session);
    frame(&mut state, &mut session);
    assert!(picker(&mut state).0);
    let button = state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .unwrap()
        .collapse_btn_rect()
        .expect("an expanded picker has a collapse button");
    let press_frame = |state: &mut RequestContext,
                       session: &mut crate::runtime::engine_session::EngineSession,
                       pos: egui::Pos2| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let engine = session.borrow_mut();
        drop(ctx.run(raw, |ctx| {
            sync_plugin_popup_scopes(state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(&mgr)),
                Some(&layout),
            );
            let press = crate::adapters::ui::popup::frame::frame_press(
                ctx,
                state,
                &engine.read(),
                Some(&layout),
            );
            let mut popups =
                std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
            popups.draw_routed(ctx, &mut |_, _| {}, Some(&layout), &[], press.as_ref());
            state.popups = popups;
        }));
        state.press_snapshot = None;
    };
    press_frame(&mut state, &mut session, button.center());
    assert!(
        !state.popups.plugin_expanded().contains(&7),
        "the collapse button of the picker folds its plugin parent too"
    );
    frame(&mut state, &mut session);
    frame(&mut state, &mut session);
    let (expanded, collapsed) = picker(&mut state);
    assert!(
        !expanded && collapsed,
        "the picker stays folded on the next frames"
    );
}

/// 다른 owner 의 결속 범위나 이전 자식이 남긴 범위를 따라가지 않는다.
#[test]
fn only_the_requesting_owner_moves_the_picker() {
    let (mut state, _) = trigger(Some(7));
    let sid = match picker_scope(&mut state) {
        PopupScope::Surface(sid) => sid,
        other => panic!("owner scope expected, got {other:?}"),
    };
    let mut foreign = parent("window", None);
    foreign.plugin_id = "com.example.other".into();
    sync_plugin_popup_scopes(&mut state, [(7, &foreign)].into_iter());
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
    let unrelated = parent("window", None);
    sync_plugin_popup_scopes(&mut state, [(8, &unrelated)].into_iter());
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
}

/// 포커스된 surface 를 덮는 피커는 passive 여도 포커스 노드다(덮인 노드는 포커스 노드가 될 수 없다). passive
/// 표시는 사용자가 누르기 전까지 남는다.
fn assert_visible_gates(state: &RequestContext, user_focused: bool) {
    assert!(state.popups.has_focused());
    assert!(RequestContext::keyboard_overlay_open(state));
    // WebView 를 가리는지는 포커스가 아니라 passive 로 정한다(passive 팝업은 그 영역만 잘라 낸다).
    assert_eq!(state.has_egui_overlay_open(), user_focused);
    assert_eq!(state.popups.is_passive(FILE_PICKER_POPUP_ID), !user_focused);
}

/// 숨은 피커는 열린 채 작업을 보존하지만 포커스 노드가 아니고 입력 차단도 풀린다.
fn assert_hidden_gates(state: &mut RequestContext) {
    assert!(state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert!(
        !state
            .popups
            .get_mut(FILE_PICKER_POPUP_ID)
            .map(|p| p.instance)
            .is_some_and(|iid| state.popups.is_instance_focused(iid))
    );
    assert!(!state.popups.has_focused());
    assert!(!state.popups.is_focused(FILE_PICKER_POPUP_ID));
    assert!(state.popups.focused_dismissal_target().is_none());
    assert!(!RequestContext::keyboard_overlay_open(state));
    assert!(
        !state.has_egui_overlay_open(),
        "hidden child must not hide native WebViews"
    );
    assert!(state.host_popup_hittest.is_empty());
    assert!(state.popup_layers.is_empty());
    assert!(!matches!(
        state.frame_key_owner,
        Some(Some(
            crate::adapters::ui::popup::occlusion::PopupKeyOwner::Host
        ))
    ));
    assert!(state.popups.take_closed_ids().is_empty());
}

fn assert_work_preserved(state: &RequestContext, dir: &str, request: u64) {
    let data = crate::adapters::ui::popup::file_picker::instances::latest(state).unwrap();
    assert_eq!(data.selected, ["draft.md"]);
    assert_eq!(data.current_dir, dir);
    assert_eq!(data.requester.as_ref().unwrap().request_id, request);
    assert!(matches!(
        data.load,
        crate::state::FpLoadState::Loading {
            request_id: 123,
            ..
        }
    ));
    assert!(data.result.is_none());
}

#[test]
fn hidden_child_keeps_selection_and_request_without_paint_hit_or_keyboard_gate() {
    assert_hidden_child_round_trip(true);
}

#[test]
fn passive_hidden_child_stays_passive_when_restored() {
    assert_hidden_child_round_trip(false);
}

fn assert_hidden_child_round_trip(user_focused: bool) {
    let (mut state, mut engine_session) = trigger(Some(7));
    let engine = engine_session.borrow_mut();
    // 범위 대상이 사라진 팝업은 그리기 전에 닫히므로 엔진에 있는 surface를 쓴다.
    let sid = state.focused_surface_id(&engine).expect("surface");
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
    let data = crate::adapters::ui::popup::file_picker::instances::current_mut(&mut state).unwrap();
    data.selected = vec!["draft.md".into()];
    data.load = crate::state::FpLoadState::Loading {
        request_id: 123,
        sent_at: std::time::Instant::now(),
    };
    let request = data.requester.as_ref().unwrap().request_id;
    let dir = data.current_dir.clone();
    let ctx = egui::Context::default();
    // 피커(640×480)가 surface 안쪽 경계에 들어가는 크기다.
    let rect = egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(680.0, 560.0));
    let mut layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects: vec![(sid, rect)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    let draw = |state: &mut RequestContext, layout: &LayoutContext, events| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        drop(ctx.run(raw, |ctx| {
            draw_popup_layer(ctx, state, &engine.read(), layout)
        }));
    };
    draw(&mut state, &layout, vec![]);
    // Plugin 출처는 passive 다. 덮는 피커라 포커스 노드는 피커다. 사용자 누름은 passive 를 푼다.
    assert_visible_gates(&state, false);
    if user_focused {
        for pressed in [true, false] {
            draw(
                &mut state,
                &layout,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    assert_visible_gates(&state, user_focused);
    let popup = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap();
    // surface 범위 팝업은 테두리에서 spacing_sm만큼 안쪽을 경계로 쓴다.
    let inset = crate::theme::theme().spacing_sm.value();
    assert!(!popup.collapsed(), "the picker fits the inherited surface");
    assert!(rect.shrink(inset).contains_rect(popup.popup_rect()));
    let z = state.host_popup_hittest[0].z_seq;
    layout.surface_rects.clear();
    layout.active_workspace = 1;
    // 숨긴 화면을 한 번 그린 뒤 누른다. 누름의 포인터 대상은 사용자가 본 직전 프레임의 배치다.
    draw(&mut state, &layout, vec![]);
    draw(
        &mut state,
        &layout,
        vec![
            egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_hidden_gates(&mut state);
    assert_work_preserved(&state, &dir, request);
    layout.surface_rects.push((sid, rect));
    layout.active_workspace = 0;
    draw(&mut state, &layout, vec![]);
    assert_visible_gates(&state, user_focused);
    assert_eq!(state.host_popup_hittest[0].z_seq, z);
    assert_eq!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state)
            .unwrap()
            .selected,
        ["draft.md"]
    );
}

#[test]
fn closing_a_hidden_parent_closes_the_shell_without_waiting_for_draw() {
    let (mut state, _) = trigger(Some(7));
    crate::app::dispatch::plugin_popup_events::cancel_child_file_picker(
        &mut state,
        &[(7, tasty_plugin_protocol::PopupCloseReason::PluginRequest)],
    );
    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    let closed = state.popups.take_closed_queue();
    assert_eq!(
        closed.iter().map(|c| c.id).collect::<Vec<_>>(),
        [FILE_PICKER_POPUP_ID]
    );
    assert!(matches!(
        crate::adapters::ui::popup::file_picker::instances::picker_of(&closed[0].payload)
            .unwrap()
            .result,
        Some(crate::state::FilePickerResult::Cancelled)
    ));
    crate::app::dispatch::plugin_popup_events::cancel_child_file_picker(
        &mut state,
        &[(7, tasty_plugin_protocol::PopupCloseReason::PluginRequest)],
    );
    assert!(state.popups.take_closed_ids().is_empty());
}

/// R 이 owner 없이 요청한 피커는 `Surface(R)`이고 R 안에 놓인다. 포커스가 L 인 동안 누른 Escape 는
/// 피커를 닫지 않아야 한다.
#[test]
fn escape_on_the_left_surface_leaves_a_right_ownerless_picker_open() {
    escape_on_left_with_right_picker(true);
}

/// 같은 장면에서 Escape 없이 범위와 위치만 본다.
#[test]
fn an_ownerless_right_picker_is_placed_in_the_right_surface() {
    escape_on_left_with_right_picker(false);
}

fn escape_on_left_with_right_picker(press_escape: bool) {
    const L: u32 = 201;
    const R: u32 = 101;
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    assert_eq!(state.focused_surface_id(&engine.read()), Some(L));
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &json!({"origin_surface_id": R}),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(R));

    let left = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(450.0, 700.0));
    let right = egui::Rect::from_min_size(egui::pos2(450.0, 0.0), egui::vec2(450.0, 700.0));
    let layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![],
        surface_rects: vec![(L, left), (R, right)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    // 창의 키 경로가 읽는 판정만 잰다. L 은 egui 화면이 아니고 포커스 노드가 L 이라 Esc 는 egui 로 가지 않고
    // (`window_feeds_egui`), 창 Esc 판정이 소비하지 않으며(`window_escape`), 터미널 게이트가 막지 않는다
    // (`window_holds_terminal_keys`). L PTY 에 실제로 쓰는 것(`view::main::keyboard`)은 GPU 창이 있어야 불러 볼 수
    // 있어 이 시험이 관측하지 않는다. 아래 그리기는 같은 Esc 가 egui 프레임에 실려도 R 피커가 닫히지 않는지 따로 본다.
    if press_escape {
        state.normalize_focus(&engine.read());
        assert_eq!(
            state.focus_node(&engine.read()),
            crate::state::focus_node::FocusNode::Surface(Some(L))
        );
        assert_eq!(
            state.window_feeds_egui(false, true),
            None,
            "Esc 가 egui 로 간다"
        );
        let mut pending = Vec::new();
        assert!(
            !state.window_escape(&mut pending, false),
            "창 Esc 판정이 L 의 Esc 를 처리한다"
        );
        assert!(
            !state.window_holds_terminal_keys(),
            "L 의 Esc 가 터미널로 가지 않는다"
        );
        assert!(state.pending_intents.is_empty(), "Esc 가 닫기 요청을 냈다");
    }
    let ctx = egui::Context::default();
    let escape = press_escape.then(|| egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    for events in [vec![], escape.into_iter().collect()] {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        };
        drop(ctx.run(raw, |ctx| {
            draw_popup_layer(ctx, &mut state, &engine.read(), &layout)
        }));
    }
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert!(state.popups.is_open(FILE_PICKER_POPUP_ID), "picker closed");
    assert!(
        crate::adapters::ui::popup::file_picker::instances::current(&mut state)
            .unwrap()
            .result
            .is_none()
    );
    // winit 쪽 Escape 처리도 피커를 닫을 대상으로 고르지 않는다.
    assert!(state.popups.focused_dismissal_target().is_none());
    // R이 피커보다 좁아 보기 버튼으로 접힌다. 그린 상자가 R 안이다.
    let popup = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap();
    assert!(popup.collapsed());
    let placed = popup.box_rect().expect("own show button");
    assert!(right.contains_rect(placed), "picker left R: {placed:?}");
}

/// owner 팝업을 연 직후, 그리기 전에 온 요청도 owner 의 결속 범위로 연다.
/// 요청 직전에 manager 인스턴스로 범위를 다시 만들기 때문이다.
#[test]
fn a_trigger_before_the_first_draw_finds_a_just_opened_owner() {
    let (mut state, mut session) = test_state();
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let sid = state.focused_surface_id(&engine.read()).expect("surface");
    let mut mgr = crate::plugin::PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.insert_popup_instance_for_test(7, parent("surface", Some(sid)));
    assert!(
        state.plugin_popup_bound_scopes.is_empty(),
        "no frame drawn yet"
    );
    crate::plugin_bridge::popup_scope::refresh_for_request(
        &mut state,
        "file_picker.trigger",
        mgr.popup_instances(),
    );
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &json!({"owner_popup_instance": 7}),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
}

/// 다른 요청은 그리기 단계가 만든 결속 범위를 바꾸지 않는다.
#[test]
fn other_requests_leave_the_bound_scopes_alone() {
    let (mut state, _session) = test_state();
    let inst = parent("window", None);
    crate::plugin_bridge::popup_scope::refresh_for_request(
        &mut state,
        "surface.list",
        [(7, &inst)].into_iter(),
    );
    assert!(state.plugin_popup_bound_scopes.is_empty());
}

/// J5(실제 피커). Given 알림 목록이 열린 위에 plugin 이 Window 범위 파일 피커를 열었을 때, When 창이 Esc 를 받으면,
/// Then 덮는 피커가 포커스 노드라 Esc 는 피커 본문(`owns_escape`, 열린 하위 메뉴 없음)이 읽어 피커만 닫고 덮인 알림
/// 목록은 닫지 않는다. 글자는 저장 모드 이름 칸만 읽으므로 여기서는 창 게이트가 피커에 키를 묶는지로 잰다.
#[test]
fn a_window_picker_takes_the_first_escape_before_an_open_notification_list() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    crate::i18n::init("en");
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .unwrap();
    let instances = [(7, parent("window", None))];
    sync_plugin_popup_scopes(&mut state, instances.iter().map(|(id, inst)| (*id, inst)));
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &json!({"owner_popup_instance": 7}),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert_eq!(picker_scope(&mut state), PopupScope::Window);
    let picker = state
        .popups
        .entries_of(FILE_PICKER_POPUP_ID)
        .next()
        .map(|p| p.instance)
        .expect("피커가 열린다");

    let sid = state.focused_surface_id(&engine.read()).expect("surface");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 700.0));
    let layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![],
        surface_rects: vec![(sid, screen)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    let ctx = egui::Context::default();
    let layer_frame = |state: &mut RequestContext,
                       engine: &crate::runtime::engine_read::EngineRead<'_>,
                       events: Vec<egui::Event>| {
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        drop(ctx.run(raw, |ctx| draw_popup_layer(ctx, state, engine, &layout)));
    };
    layer_frame(&mut state, &engine.read(), Vec::new());
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(picker)),
        "덮는 피커가 포커스 노드다"
    );
    assert_eq!(
        state.window_feeds_egui(false, true),
        Some(true),
        "창은 키를 egui 로 보내고 피커에 묶는다"
    );

    let escape = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut pending = vec![escape];
    state.window_escape(&mut pending, false);
    layer_frame(&mut state, &engine.read(), pending);
    let closes_notifications = state.pending_intents.iter().any(|i| {
        matches!(
            &i.body,
            crate::intent::Intent::Ui(crate::intent::UiIntent::ClosePopup {
                id: "notifications"
            })
        )
    });
    assert!(!closes_notifications, "알림 목록 닫기 요청이 없다");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert!(
        state.popups.instance(picker).is_none_or(|p| !p.open),
        "피커 본문이 Esc 를 읽어 닫힌다"
    );
    assert!(
        state.popups.is_open("notifications"),
        "덮인 알림 목록은 그 Esc 로 닫히지 않는다"
    );
}

/// J5(실제 피커, 글자). Given 알림 목록이 열린 위에 plugin 이 Window 범위 저장 모드 파일 피커를 열었을 때, When 창의
/// 키 경로로 Tab 을 보내 이름 칸에 egui 키 포커스를 주고 글자를 보내면, Then 덮는 피커가 포커스 노드라 그 글자는 피커
/// 본문의 이름 칸에 들어간다(덮인 알림 목록이 받지 않는다).
#[test]
fn a_window_save_picker_body_receives_the_letters_over_an_open_notification_list() {
    use crate::adapters::ui::popup::window_input_test_support::WindowInput;
    crate::i18n::init("en");
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .unwrap();
    let instances = [(7, parent("window", None))];
    sync_plugin_popup_scopes(&mut state, instances.iter().map(|(id, inst)| (*id, inst)));
    let dir = tempfile::tempdir().unwrap();
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &CallerContext::Plugin {
            plugin_id: PLUGIN.into(),
        },
        json!(1),
        &json!({"owner_popup_instance": 7, "mode": "save", "location": "local", "default_name": "", "start_dir": dir.path()}),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert_eq!(picker_scope(&mut state), PopupScope::Window);

    let sid = state.focused_surface_id(&engine.read()).expect("surface");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 700.0));
    let layout = LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![],
        surface_rects: vec![(sid, screen)],
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    };
    let ctx = egui::Context::default();
    let mut input = WindowInput::default();
    // 창의 egui 패스 하나(`GpuState::run_main_pass`).
    let pass = |state: &mut RequestContext,
                engine: &crate::runtime::engine_read::EngineRead<'_>,
                input: &mut WindowInput| {
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events: input.take_pass(state, &ctx),
            ..Default::default()
        };
        drop(ctx.run(raw, |ctx| draw_popup_layer(ctx, state, engine, &layout)));
        state.close_input_pass(&ctx);
    };
    pass(&mut state, &engine.read(), &mut input);
    state.normalize_focus(&engine.read());
    let tab = egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let in_text_field = |ctx: &egui::Context| {
        ctx.memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
    };
    for _ in 0..20 {
        if in_text_field(&ctx) {
            break;
        }
        assert_eq!(
            input.key(&mut state, &engine.read(), false, tab.clone()),
            Some(true),
            "창은 키를 egui 로 보내고 피커에 묶는다"
        );
        pass(&mut state, &engine.read(), &mut input);
    }
    assert!(
        in_text_field(&ctx),
        "Tab 이 피커의 이름 칸에 키 포커스를 준다"
    );

    for letter in ["a", "b"] {
        assert_eq!(
            input.key(
                &mut state,
                &engine.read(),
                false,
                egui::Event::Text(letter.into())
            ),
            Some(true)
        );
    }
    while !input.is_empty() {
        pass(&mut state, &engine.read(), &mut input);
    }
    let picker = crate::adapters::ui::popup::file_picker::instances::current(&mut state)
        .expect("피커가 열려 있다");
    assert_eq!(
        picker.save_name.as_deref(),
        Some("ab"),
        "글자는 피커 본문의 이름 칸에 들어간다"
    );
    assert!(state.popups.is_open("notifications"));
}
