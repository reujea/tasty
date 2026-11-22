//! 펼친 plugin 부모 팝업이 연 피커를 Esc 가 아닌 접기 버튼으로 접은 뒤의 키 주인. 접힌 묶음에서 키를 받는 것은 보이는
//! 보기 버튼(plugin 부모)이다. 숨은 피커는 덮는 노드가 아니므로 정규화가 포커스를 그 피커로 되돌리지 않는다.

use super::handle_trigger;
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::defs;
use crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::occlusion::PopupKeyOwner;
use crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes;
use crate::runtime::engine_session::EngineSession;
use crate::state::RequestContext;
use crate::state::tests::test_state;
use serde_json::json;
use tasty_host_plugin::manager::PopupInstance;
use tasty_ipc::caller::CallerContext;

const PLUGIN: &str = "com.example.picker";
const PARENT: u64 = 7;

/// 좁은 surface 에 결속된 plugin 부모(창 위로 펼침)와 그 부모가 연 피커(함께 펼침).
struct Scene {
    state: RequestContext,
    session: EngineSession,
    mgr: crate::plugin::PluginManager,
    layout: LayoutContext,
    ctx: egui::Context,
}

impl Scene {
    fn new() -> Self {
        let (mut state, mut session) = test_state();
        let sid = state
            .focused_surface_id(&session.borrow_mut().read())
            .expect("surface");
        let mut inst = PopupInstance {
            plugin_id: PLUGIN.into(),
            popup_id: "open".into(),
            contribute: serde_json::from_value(json!({
                "id": "open", "trigger": {"kind": "event", "event_key": "test.open"},
                "scope": "surface"
            }))
            .unwrap(),
            z_seq: 1,
            scope_surface: Some(sid),
        };
        inst.contribute.size_hint = Some(tasty_plugin_manifest::PopupSizeHint {
            width: 800,
            height: 600,
        });
        let mut mgr = crate::plugin::PluginManager::with_registries(
            std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
            std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
            std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        mgr.insert_popup_instance_for_test(PARENT, inst);
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
        let mut scene = Self {
            state,
            session,
            mgr,
            layout,
            ctx: egui::Context::default(),
        };
        scene.state.popups.expand_plugin(PARENT);
        scene.frame();
        {
            let mut engine = scene.session.borrow_mut();
            let caller = CallerContext::Plugin {
                plugin_id: PLUGIN.into(),
            };
            let response = handle_trigger(
                &mut scene.state,
                &mut engine,
                &caller,
                json!(1),
                &json!({"owner_popup_instance": PARENT}),
            );
            assert!(response.result.is_some(), "{response:?}");
            for intent in scene.state.take_pending_intents() {
                crate::intent::popup::handle(&mut scene.state, &mut engine, &intent);
            }
        }
        scene.frame();
        assert!(scene.picker().0, "피커는 부모와 함께 펼친 채 열린다");
        scene
    }

    fn raw(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        }
    }

    /// 제품 순서의 한 패스: host 팝업 층(실제 정의 본문) 뒤 plugin 팝업 렌더.
    fn frame(&mut self) {
        let engine = self.session.borrow_mut();
        let (state, mgr, layout) = (&mut self.state, &self.mgr, &self.layout);
        drop(self.ctx.run(Self::raw(Vec::new()), |ctx| {
            crate::adapters::ui::popup::frame::draw_popup_layer(ctx, state, &engine.read(), layout);
            sync_plugin_popup_scopes(state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
                Some(layout),
            );
        }));
        self.state.close_input_pass(&self.ctx);
        self.state.press_snapshot = None;
    }

    /// `pos` 의 주 버튼 누름 한 패스. 누름을 그리기 전에 적용하고(`frame_press`) host 팝업을 누름과 함께 그린다.
    fn press(&mut self, pos: egui::Pos2) {
        let engine = self.session.borrow_mut();
        let (state, mgr, layout) = (&mut self.state, &self.mgr, &self.layout);
        let events = vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        drop(self.ctx.run(Self::raw(events), |ctx| {
            sync_plugin_popup_scopes(state, mgr.popup_instances());
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                &engine.read(),
                Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
                Some(layout),
            );
            let press = crate::adapters::ui::popup::frame::frame_press(
                ctx,
                state,
                &engine.read(),
                Some(layout),
            );
            let mut popups =
                std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
            popups.draw_routed(
                ctx,
                &mut |_, _| crate::adapters::ui::PopupAction::None,
                Some(layout),
                &crate::adapters::ui::input::edit_key::EditBindings::default(),
                &[],
                press.as_ref(),
            );
            state.popups = popups;
        }));
        self.state.press_snapshot = None;
    }

    /// 피커의 (펼침, 접힘).
    fn picker(&mut self) -> (bool, bool) {
        let p = self.state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap();
        (p.expanded(), p.collapsed())
    }

    /// 접힌 묶음을 몇 패스 그린 뒤 피커가 열린 채 접혔고 키 주인이 보기 버튼(plugin 부모)인지 본다.
    fn assert_folded_with_the_button_keyed(&mut self, how: &str) {
        for _ in 0..3 {
            self.frame();
        }
        assert!(
            self.state.popups.is_open(FILE_PICKER_POPUP_ID),
            "{how}: 피커는 열린 채다"
        );
        let (expanded, collapsed) = self.picker();
        assert!(!expanded && collapsed, "{how}: 피커는 부모와 함께 접힌다");
        assert_eq!(
            self.state.popup_key_owner(),
            Some(PopupKeyOwner::Plugin(PARENT)),
            "{how}: 접힌 묶음의 키 주인은 보이는 보기 버튼(plugin 부모)이다"
        );
    }
}

/// T-G5-1: Given 펼친 plugin 부모와 그 부모가 연 피커(피커가 포커스 노드), When plugin 부모의 제목줄 접기 버튼이 부르는
/// 접기(`refold_plugin`, popup_render 의 접기 버튼 처리)로 묶음을 접으면, Then 피커는 열린 채 접히고 키 주인은 보이는
/// 보기 버튼인 plugin 부모다.
#[test]
fn folding_the_plugin_parent_hands_the_keys_to_its_view_button() {
    let mut scene = Scene::new();
    scene.state.popups.set_focused(FILE_PICKER_POPUP_ID, true);
    scene.state.popups.refold_plugin(PARENT);
    scene.assert_folded_with_the_button_keyed("plugin 부모 접기");
}

/// T-G5-2: Given 펼친 plugin 부모와 그 부모가 연 피커, When 피커 제목줄의 host 접기 버튼을 누르면(누름 패스가
/// 포커스를 피커에 준다), Then 묶음째 접히고 키 주인은 보이는 보기 버튼인 plugin 부모다.
#[test]
fn the_picker_collapse_button_hands_the_keys_to_the_parent_view_button() {
    let mut scene = Scene::new();
    let button = scene
        .state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .unwrap()
        .collapse_btn_rect()
        .expect("펼친 피커는 접기 버튼이 있다");
    scene.press(button.center());
    assert!(
        !scene.state.popups.plugin_expanded().contains(&PARENT),
        "피커 접기 버튼은 plugin 부모도 접는다"
    );
    scene.assert_folded_with_the_button_keyed("피커 접기 버튼");
}
