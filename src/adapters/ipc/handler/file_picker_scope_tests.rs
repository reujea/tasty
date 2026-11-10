use super::handle_trigger;
use crate::adapters::ui::popup::{PopupScope, file_picker::FILE_PICKER_POPUP_ID};
use crate::adapters::ui::{
    LayoutContext,
    popup::{defs, frame::draw_popup_layer},
};
use crate::plugin_bridge::popup_scope::inherit_file_picker_scope;
use crate::state::RequestContext;
use crate::state::tests::test_state;
use serde_json::json;
use tasty_host_plugin::manager::PopupInstance;
use tasty_ipc::caller::CallerContext;

fn parent(scope: &str, target: Option<u32>) -> PopupInstance {
    PopupInstance {
        plugin_id: "com.example.picker".into(),
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

fn trigger(
    owner: Option<u64>,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let caller = CallerContext::Plugin {
        plugin_id: "com.example.picker".into(),
        permissions: std::sync::Arc::new(Default::default()),
    };
    let response = handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        json!(1),
        &json!({"owner_popup_instance": owner}),
    );
    assert!(response.result.is_some());
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    (state, engine_session)
}

#[test]
fn trigger_inherits_declaration_and_target_not_just_target() {
    for (decl, target, expected) in [
        ("surface", Some(17), PopupScope::Surface(17)),
        ("surface", None, PopupScope::Window),
        ("window", Some(17), PopupScope::Window),
        ("window", None, PopupScope::Window),
    ] {
        let (mut state, _) = trigger(Some(7));
        let inst = parent(decl, target);
        inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
        assert_eq!(
            state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope,
            expected
        );
    }
}

#[test]
fn the_child_follows_the_scope_its_parent_was_actually_placed_in() {
    // 좁은 surface 라 창 범위로 연 부모, 그리고 surface 에 그대로 놓인 부모.
    for (placed, expected) in [
        (Some(PopupScope::Window), PopupScope::Window),
        (Some(PopupScope::Surface(17)), PopupScope::Surface(17)),
        (None, PopupScope::Surface(17)),
    ] {
        let (mut state, _) = trigger(Some(7));
        if let Some(scope) = placed {
            state.plugin_popup_placed_scopes.insert(7, scope);
        }
        let inst = parent("surface", Some(17));
        inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
        assert_eq!(
            state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope,
            expected
        );
    }
}

#[test]
fn a_placed_scope_of_another_instance_is_not_inherited() {
    let (mut state, _) = trigger(Some(7));
    state
        .plugin_popup_placed_scopes
        .insert(8, PopupScope::Window);
    let inst = parent("surface", Some(17));
    inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
    assert_eq!(
        state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope,
        PopupScope::Surface(17)
    );
}

#[test]
fn ownerless_and_foreign_owner_cannot_inherit_another_plugins_scope() {
    for owner in [None, Some(8), Some(7)] {
        let (mut state, _) = trigger(owner);
        let mut inst = parent("surface", Some(17));
        if owner == Some(7) {
            inst.plugin_id = "com.example.other".into();
        }
        // A prior child must not leave its scope on the singleton host picker.
        state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope = PopupScope::Surface(99);
        inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
        assert_eq!(
            state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().scope,
            PopupScope::Window
        );
    }
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
    let inst = parent("surface", Some(sid));
    inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
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
    let inst = parent("surface", Some(17));
    inherit_file_picker_scope(&mut state, [(7, &inst)].into_iter());
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
