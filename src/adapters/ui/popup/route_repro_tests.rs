//! 사용자가 보고한 팝업 범위 결함 두 개의 재현. 고치기 전에는 실패하므로 해제할 단계를 적어 무시해 둔다.
//! 장면: 좌우 surface L·R, R 범위의 plugin 팝업(markdown 경로 팝업과 같은 선언).

use std::sync::Arc;

use egui::{Context, Event, Pos2};
use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupSizeHint};

use super::{SURFACE_L, SURFACE_R, layout, rect, scene, surface_rect_l, to_egui};
use crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::route::{KeyTarget, keyboard_target};
use crate::adapters::ui::popup::{PopupScope, defs};
use crate::model::LogicalPx;
use crate::plugin::PluginManager;
use crate::plugin::manifest::PopupAnchor;
use crate::plugin_bridge::popup_render::draw_plugin_popups;
use crate::state::RequestContext;

const PLUGIN: &str = "com.example.markdown";
const POPUP: u64 = 7;

fn screen() -> egui::Rect {
    to_egui(rect(
        LogicalPx(0.0),
        LogicalPx(0.0),
        LogicalPx(900.0),
        LogicalPx(700.0),
    ))
}

fn instance(z_seq: u64) -> tasty_host_plugin::PopupInstance {
    tasty_host_plugin::PopupInstance {
        plugin_id: PLUGIN.into(),
        popup_id: "file-open".into(),
        contribute: PopupContribute {
            id: "file-open".into(),
            trigger: tasty_plugin_manifest::PopupTrigger::Ipc,
            // R 안에 들어가는 크기라 창 범위 대체 배치가 일어나지 않는다.
            size_hint: Some(PopupSizeHint {
                width: 300,
                height: 150,
            }),
            anchor: PopupAnchor::ScreenCenter,
            scope: PopupScopeDecl::Surface,
            dismiss_on_outside_click: true,
            rendering: PopupRendering::EguiMesh,
        },
        z_seq,
        scope_surface: Some(SURFACE_R),
    }
}

fn manager_with_r_popup(z_seq: u64) -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.insert_popup_instance_for_test(POPUP, instance(z_seq));
    mgr
}

fn plugin_frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    events: Vec<Event>,
) {
    let input = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    drop(ctx.run(input, |ctx| {
        draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            Some(&layout()),
        );
    }));
}

fn press(pos: Pos2) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }
}

fn closes_of_popup(state: &RequestContext) -> Vec<tasty_plugin_protocol::PopupCloseReason> {
    state
        .plugin_popup_closes
        .iter()
        .filter(|(id, _)| *id == POPUP)
        .map(|(_, reason)| *reason)
        .collect()
}

/// 결함 1: R 범위 팝업이 L 을 누른 것에 닫힌다. L 은 그 팝업 범위 밖이라 바깥 클릭이 아니다.
#[test]
#[ignore = "defect 1 reproduction: fixed by the plugin pointer step of the popup scope refactor"]
fn a_press_on_the_left_surface_does_not_close_a_popup_scoped_to_the_right() {
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let ctx = Context::default();

    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    let placed = state
        .plugin_popup_hittest
        .first()
        .map(|o| o.rect)
        .expect("R 범위 팝업이 놓인다");
    assert!(
        super::surface_rect_r().contains_rect(placed),
        "R 안에 놓여야 재현 장면이다: {placed:?}"
    );
    assert_eq!(
        state.plugin_popup_placed_scopes.get(&POPUP),
        Some(&PopupScope::Surface(SURFACE_R))
    );

    let on_left = surface_rect_l().center();
    plugin_frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(on_left), press(on_left)],
    );
    assert_eq!(
        closes_of_popup(&state),
        [],
        "L 을 누른 것이 R 범위 팝업의 닫힘 요청이 됐다"
    );
}

/// 결함 2: R 팝업의 자식 파일 피커가 떠 있으면 포커스가 L 이어도 키가 L 로 가지 않는다.
#[test]
#[ignore = "defect 2 reproduction: fixed by the focus target and key owner step of the popup scope refactor"]
fn keys_reach_the_focused_left_surface_while_a_right_picker_is_open() {
    let (mut state, mut session) = scene(super::PANE_L);
    let mut engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let ctx = Context::default();
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    assert!(state.plugin_popup_open, "R 팝업이 보이는 상태여야 한다");

    // markdown 의 찾아보기처럼 R 팝업을 owner 로 가진 피커를 연다.
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
        permissions: Arc::new(Default::default()),
    };
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        &mut state,
        &mut engine,
        &caller,
        serde_json::json!(1),
        &serde_json::json!({"owner_popup_instance": POPUP}),
    );
    assert!(
        response.result.is_some(),
        "피커 요청이 거절됐다: {response:?}"
    );
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    let inst = instance(0);
    crate::plugin_bridge::popup_scope::inherit_file_picker_scope(
        &mut state,
        [(POPUP, &inst)].into_iter(),
    );
    assert_eq!(
        state
            .popups
            .get_mut(FILE_PICKER_POPUP_ID)
            .map(|p| p.scope.clone()),
        Some(PopupScope::Surface(SURFACE_R))
    );

    // 창의 한 프레임처럼 host 팝업 층과 plugin 팝업을 차례로 그린다.
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        draw_popup_layer(ctx, &mut state, &engine.read(), &layout());
    }));
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    assert_eq!(state.focused_surface_id(&engine.read()), Some(SURFACE_L));

    assert_eq!(
        keyboard_target(&state, &engine.read()),
        Some(KeyTarget::Surface(SURFACE_L)),
        "포커스는 L 인데 키가 오버레이로 간다"
    );
    assert_ne!(
        state.popup_escape_owner,
        Some(FILE_PICKER_POPUP_ID),
        "포커스 경로에 없는 R 피커가 Escape 를 받는다"
    );
}
