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
        permissions: std::sync::Arc::new(Default::default()),
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
    assert!(state.dialogs.file_picker.is_none(), "picker opened");
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

/// owner 가 좁은 surface 에 있어 창 범위로 옮겨 그려져도 피커는 owner 의 결속 범위를 쓴다.
/// 매 프레임 배치 결과는 범위를 정하지 않는다.
#[test]
fn a_narrow_owner_drawn_in_the_window_still_gives_its_surface_scope() {
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
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    };
    {
        let engine = session.borrow_mut();
        drop(ctx.run(raw, |ctx| {
            sync_plugin_popup_scopes(&mut state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                &mut state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(&mgr)),
                Some(&layout),
            );
        }));
    }
    let placed = state
        .plugin_popup_hittest
        .first()
        .expect("owner drawn")
        .rect;
    assert!(
        !narrow.contains_rect(placed),
        "the owner must be drawn outside its narrow surface: {placed:?}"
    );
    let mut engine = session.borrow_mut();
    let caller = CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
        permissions: std::sync::Arc::new(Default::default()),
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

fn assert_visible_gates(state: &RequestContext, user_focused: bool) {
    assert_eq!(state.popups.has_focused(), user_focused);
    assert_eq!(RequestContext::keyboard_overlay_open(state), user_focused);
    assert_eq!(state.has_egui_overlay_open(), user_focused);
    assert_eq!(state.popups.is_passive(FILE_PICKER_POPUP_ID), !user_focused);
}

/// 숨겨져도 포커스 의도는 남지만 입력 차단은 해제되어야 한다.
fn assert_hidden_gates(state: &mut RequestContext, user_focused: bool) {
    assert!(state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert_eq!(
        state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().focused,
        user_focused
    );
    assert!(!state.popups.has_focused());
    assert!(!state.popups.is_focused(FILE_PICKER_POPUP_ID));
    assert!(state.popups.focused_dismissal_target().is_none());
    assert!(!RequestContext::keyboard_overlay_open(state));
    assert!(
        !state.has_egui_overlay_open(),
        "hidden child must not hide native WebViews"
    );
    assert!(!state.popup_hovered);
    assert!(state.host_popup_hittest.is_empty());
    assert!(state.popup_layers.is_empty());
    assert!(state.popup_escape_owner.is_none());
    assert!(state.popups.take_closed_queue().is_empty());
}

fn assert_work_preserved(state: &RequestContext, dir: &str, request: u64) {
    let data = state.dialogs.file_picker.as_ref().unwrap();
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
fn passive_hidden_child_stays_nonfocusing_when_restored() {
    assert_hidden_child_round_trip(false);
}

fn assert_hidden_child_round_trip(user_focused: bool) {
    let (mut state, mut engine_session) = trigger(Some(7));
    let engine = engine_session.borrow_mut();
    // 범위 대상이 사라진 팝업은 그리기 전에 닫히므로 엔진에 있는 surface를 쓴다.
    let sid = state.focused_surface_id(&engine).expect("surface");
    assert_eq!(picker_scope(&mut state), PopupScope::Surface(sid));
    let data = state.dialogs.file_picker.as_mut().unwrap();
    data.selected = vec!["draft.md".into()];
    data.load = crate::state::FpLoadState::Loading {
        request_id: 123,
        sent_at: std::time::Instant::now(),
    };
    let request = data.requester.as_ref().unwrap().request_id;
    let dir = data.current_dir.clone();
    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(400.0, 360.0));
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
    // Plugin 출처는 자동 포커스를 갖지 않는다. 포커스 보존 시나리오는 실제 클릭부터 시작한다.
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
    assert_eq!(
        popup.size,
        rect.shrink(inset).size(),
        "existing clamp follows inherited surface bounds"
    );
    assert!(rect.contains_rect(egui::Rect::from_min_size(popup.pos, popup.size)));
    let z = state.host_popup_hittest[0].z_seq;
    layout.surface_rects.clear();
    layout.active_workspace = 1;
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
    assert_hidden_gates(&mut state, user_focused);
    assert_work_preserved(&state, &dir, request);
    layout.surface_rects.push((sid, rect));
    layout.active_workspace = 0;
    draw(&mut state, &layout, vec![]);
    assert_visible_gates(&state, user_focused);
    assert_eq!(state.host_popup_hittest[0].z_seq, z);
    assert_eq!(
        state.dialogs.file_picker.as_ref().unwrap().selected,
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
    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(crate::state::FilePickerResult::Cancelled)
    ));
    assert_eq!(state.popups.take_closed_queue(), [FILE_PICKER_POPUP_ID]);
    crate::app::dispatch::plugin_popup_events::cancel_child_file_picker(
        &mut state,
        &[(7, tasty_plugin_protocol::PopupCloseReason::PluginRequest)],
    );
    assert!(state.popups.take_closed_queue().is_empty());
}

/// R 이 owner 없이 요청한 피커는 `Surface(R)`이고 R 안에 놓인다. 포커스가 L 인 동안 누른 Escape 는
/// 피커를 닫지 않아야 한다. Escape 주인은 범위와 무관하게 z 최상단 팝업이라 이 시험은 실패한다.
/// Escape 주인 판정이 포커스가 있는 범위를 보게 되면 풀린다.
#[test]
#[ignore = "the escape owner is the topmost popup whatever its scope; this passes once the escape owner check reads the focused scope"]
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
        permissions: std::sync::Arc::new(Default::default()),
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
    assert!(state.dialogs.file_picker.as_ref().unwrap().result.is_none());
    // winit 쪽 Escape 처리도 피커를 닫을 대상으로 고르지 않는다.
    assert!(state.popups.focused_dismissal_target().is_none());
    let popup = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap();
    let placed = egui::Rect::from_min_size(popup.pos, popup.size);
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
        permissions: std::sync::Arc::new(Default::default()),
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
