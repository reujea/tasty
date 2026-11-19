//! 한 프레임의 키 주인은 하나다. host 팝업 그리기가 이번 프레임 범위 판정 뒤 정한 값을 host 본문과 plugin 팝업
//! 렌더가 함께 쓴다. 장면: L 범위 피커 H(맨 위, 포커스), R 범위 plugin 팝업 P(가운데), R 범위 피커 G(맨 아래,
//! 포커스). 창의 한 프레임처럼 결속 범위 동기화 → host 팝업 층 → plugin 팝업 순서로 그린다.

use std::sync::Arc;

use egui::{Context, Event};
use serde_json::json;
use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupSizeHint};

use super::{SURFACE_L, SURFACE_R, layout, scene};
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::file_picker::instances;
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::{PopupInstanceId, PopupScope, defs};
use crate::plugin::PluginManager;
use crate::plugin_bridge::popup_render::draw_plugin_popups;
use crate::state::RequestContext;

const PLUGIN: &str = "com.example.keys";
const POPUP: u64 = 9;

fn plugin_popup(z_seq: u64) -> tasty_host_plugin::PopupInstance {
    let layout = tasty_plugin_manifest::PopupLayout {
        escape: tasty_plugin_protocol::popup_decl::PopupEscape::Close,
        ..Default::default()
    };
    tasty_host_plugin::PopupInstance {
        plugin_id: PLUGIN.into(),
        popup_id: "keys".into(),
        contribute: PopupContribute {
            id: "keys".into(),
            trigger: tasty_plugin_manifest::PopupTrigger::Ipc,
            size_hint: Some(PopupSizeHint {
                width: 200,
                height: 100,
            }),
            anchor: crate::plugin::manifest::PopupAnchor::ScreenCenter,
            scope: PopupScopeDecl::Surface,
            dismiss_on_outside_click: false,
            rendering: PopupRendering::EguiMesh,
            layout,
        },
        z_seq,
        scope_surface: Some(SURFACE_R),
    }
}

/// L 이 보이지 않는 배치. R 만 남는다.
fn layout_without_l() -> LayoutContext {
    let mut l = layout();
    l.surface_rects.retain(|(sid, _)| *sid != SURFACE_L);
    l.active_tab_ids.retain(|(_, sid)| *sid != SURFACE_L);
    l.surface_owners.retain(|o| o.surface != SURFACE_L);
    l
}

fn open_picker(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
) -> PopupInstanceId {
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
    };
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        state,
        engine,
        &caller,
        json!(1),
        &json!({ "origin_surface_id": sid, "location": "local", "mode": "folder" }),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(state, engine, &intent);
    }
    instances::open_pickers(state)
        .map(|(iid, _)| iid)
        .find(|iid| {
            state.popups.instance(*iid).map(|p| &p.scope) == Some(&PopupScope::Surface(sid))
        })
        .expect("picker opened")
}

/// 창의 한 프레임. `events`가 키 입력이다.
fn frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    layout: &LayoutContext,
    events: Vec<Event>,
) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        )),
        events,
        ..Default::default()
    };
    drop(ctx.run(input, |ctx| {
        crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, mgr.popup_instances());
        draw_popup_layer(ctx, state, engine, layout);
        draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            Some(layout),
        );
    }));
    state.press_snapshot = None;
}

fn escape() -> Vec<Event> {
    vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

struct Scene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    mgr: PluginManager,
    h: PopupInstanceId,
    g: PopupInstanceId,
}

/// G(R 피커) → P(R plugin 팝업) → H(L 피커) 순서로 열어 순번이 H > P > G 다. H 와 G 는 각자 범위에서 포커스를
/// 가진다.
fn open_scene() -> Scene {
    crate::i18n::init("en");
    let (mut state, mut session) = scene(super::PANE_L);
    let (mgr, h, g) = {
        let mut engine = session.borrow_mut();
        for def in defs::all_defs() {
            state.popups.register_def(def, 1.0);
        }
        let g = open_picker(&mut state, &mut engine, SURFACE_R);
        let mut mgr = PluginManager::with_registries(
            Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
            Arc::new(crate::file::format::FileFormatRegistry::new()),
            Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        mgr.insert_popup_instance_for_test(
            POPUP,
            plugin_popup(tasty_host_plugin::next_popup_z_seq()),
        );
        let h = open_picker(&mut state, &mut engine, SURFACE_L);
        // 피커의 필요 크기는 반쪽 surface 보다 커서 접힌다. 본문을 그리도록 범위 안에 드는 크기로 고정한다.
        for iid in [h, g] {
            let p = state.popups.instance_mut(iid).unwrap();
            p.focused = true;
            p.size_user_overridden = true;
            p.set_needed_size(egui::vec2(240.0, 200.0));
        }
        (mgr, h, g)
    };
    Scene {
        state,
        session,
        mgr,
        h,
        g,
    }
}

fn is_open(state: &RequestContext, iid: PopupInstanceId) -> bool {
    state.popups.instance(iid).is_some_and(|p| p.open)
}

fn plugin_escaped(state: &RequestContext) -> bool {
    state.plugin_popup_closes.iter().any(|(id, _)| *id == POPUP)
}

/// When H 의 범위(L)가 이번 프레임에 숨으면, Then 그 프레임의 Esc 는 보이는 맨 위 P 하나만 받는다. 직전 프레임
/// 보임으로 정하면 host 쪽은 G 가, plugin 쪽은 P 가 함께 받았다.
#[test]
fn a_scope_hidden_this_frame_gives_the_keys_to_one_popup_only() {
    let Scene {
        mut state,
        mut session,
        mgr,
        h,
        g,
    } = open_scene();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        &layout(),
        Vec::new(),
    );
    assert!(is_open(&state, h) && is_open(&state, g) && !plugin_escaped(&state));

    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        &layout_without_l(),
        escape(),
    );
    assert!(plugin_escaped(&state), "P 가 키 주인이다");
    assert!(is_open(&state, g), "G 는 키를 받지 않는다");
    assert!(is_open(&state, h), "숨은 H 는 키를 받지 않는다");
}

/// When 숨었던 H 의 범위가 이번 프레임에 다시 보이면, Then 그 프레임의 Esc 는 H 가 받는다. 직전 프레임 보임으로
/// 정하면 host 쪽은 plugin 이 주인이라 놓고, plugin 쪽은 그린 H 가 주인이라 놓아 아무도 받지 않았다.
#[test]
fn a_scope_shown_again_this_frame_does_not_drop_the_keys() {
    let Scene {
        mut state,
        mut session,
        mgr,
        h,
        g,
    } = open_scene();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    // 본문을 한 번 그린 뒤 숨긴다. 처음 그리는 피커 본문은 그 프레임에 입력란 포커스를 잡는다.
    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        &layout(),
        Vec::new(),
    );
    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        &layout_without_l(),
        Vec::new(),
    );
    assert!(is_open(&state, h) && is_open(&state, g) && !plugin_escaped(&state));

    frame(&ctx, &mut state, &engine.read(), &mgr, &layout(), escape());
    assert!(!is_open(&state, h), "H 가 키 주인이다");
    assert!(
        is_open(&state, g) && !plugin_escaped(&state),
        "다른 팝업은 받지 않는다"
    );
}
