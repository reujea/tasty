//! 사용자가 보고한 팝업 범위 결함 두 개의 재현. 아직 고치지 않은 결함은 해제할 단계를 적어 무시해 둔다.
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
            layout: Default::default(),
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
    // egui_bridge 처럼 그리기 전에 결속 범위를 다시 만든다.
    crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, mgr.popup_instances());
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

/// Given R 범위 plugin 팝업(바깥 클릭에 닫힘) 위에 L 범위 host 팝업이 키를 가질 때, When R 범위 안·팝업 밖을
/// 처음 누르면, Then 닫힘 요청 없이 앞으로 가져온다. When 메인 루프가 그 요청을 처리한 실제 상태에서 같은 자리를
/// 다시 누르면(스냅샷을 새로 찍는다), Then plugin 팝업이 포커스 노드라 닫힘 요청이다.
#[test]
fn the_first_press_into_the_right_scope_does_not_close_its_plugin_popup() {
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    let mut mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    open_host_popup(&mut state, SURFACE_L, true);
    let ctx = Context::default();
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    let placed = state.plugin_popup_hittest[0].rect;
    let r = super::surface_rect_r();
    let outside = egui::pos2(r.center().x, r.max.y - 4.0);
    assert!(r.contains(outside) && !placed.contains(outside));
    assert_eq!(
        state.popup_key_owner(),
        Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Host)
    );
    plugin_frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(outside), press(outside)],
    );
    assert_eq!(closes_of_popup(&state), []);
    assert!(state.plugin_popup_focus_bumps.contains(&POPUP));
    // 메인 루프: 앞으로 가져오기를 처리하고 프레임 끝에 스냅샷을 지운다.
    for id in std::mem::take(&mut state.plugin_popup_focus_bumps) {
        mgr.touch_popup_instance_z(id);
    }
    state.press_snapshot = None;
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    plugin_frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(outside), press(outside)],
    );
    assert_eq!(
        closes_of_popup(&state),
        [tasty_plugin_protocol::PopupCloseReason::OutsideClick]
    );
}

const HOST_POPUP: crate::adapters::ui::popup::PopupId = "route_repro_cover";

/// 덮고 바깥 클릭에 닫히는 host 팝업을 `surface` 범위에 연다.
fn open_host_popup(
    state: &mut RequestContext,
    surface: u32,
    focused: bool,
) -> crate::adapters::ui::popup::PopupInstanceId {
    if state.popups.entries_of(HOST_POPUP).next().is_none() {
        let mut p = crate::adapters::ui::popup::PopupState::new(
            HOST_POPUP,
            "cover".to_string(),
            egui::vec2(120.0, 60.0),
            PopupScope::Window,
        )
        .with_instances(crate::adapters::ui::popup::PopupInstances::PerScopeTarget);
        p.look = p.look.covering(true);
        p.declared_look = p.look;
        p.close_on_outside_click = true;
        state.popups.register(p);
    }
    let at = if surface == SURFACE_L {
        surface_rect_l().min + egui::vec2(10.0, 10.0)
    } else {
        super::surface_rect_r().min + egui::vec2(10.0, 10.0)
    };
    let mut req = crate::intent::OpenRequest::new(HOST_POPUP, PopupScope::Surface(surface)).at(at);
    if focused {
        req = req.focused();
    }
    state.popups.open_request(&req, focused).unwrap()
}

/// host 팝업만 그리는 한 프레임. 메인 창처럼 누름은 `frame_press`로 찍고 프레임 끝에 스냅샷을 지운다.
fn host_frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    events: Vec<Event>,
) -> crate::adapters::ui::popup::PopupDrawResult {
    let input = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let mut result = None;
    drop(ctx.run(input, |ctx| {
        let press =
            crate::adapters::ui::popup::frame::frame_press(ctx, state, engine, Some(&layout()));
        let mut popups =
            std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
        result =
            Some(popups.draw_routed(ctx, &mut |_, _| {}, Some(&layout()), &[], press.as_ref()));
        state.popups = popups;
    }));
    state.press_snapshot = None;
    result.unwrap()
}

/// 결함 C1: 엔진 포커스가 L에 남은 채 R의 덮는 팝업(바깥 클릭에 닫힘)이 포커스 없이 열려 있을 때, When R 범위
/// 안·상자 밖을 처음 누르면, Then 닫히지 않고 팝업이 포커스를 받는다. When 같은 자리를 다시 누르면(실제 상태로
/// 스냅샷을 새로 찍는다), Then 그 팝업이 포커스 노드라 닫힌다. 엔진 포커스는 그동안 L이다.
#[test]
fn a_second_press_in_the_covered_scope_closes_the_popup_that_took_focus() {
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    let iid = open_host_popup(&mut state, SURFACE_R, false);
    let ctx = Context::default();
    let r = super::surface_rect_r();
    let outside = egui::pos2(r.center().x, r.max.y - 4.0);
    host_frame(
        &ctx,
        &mut state,
        &engine.read(),
        vec![Event::PointerMoved(outside)],
    );
    let first = host_frame(&ctx, &mut state, &engine.read(), vec![press(outside)]);
    assert!(first.closed.is_empty());
    assert!(
        state
            .popups
            .instance(iid)
            .is_some_and(|p| p.open && p.focused)
    );
    let second = host_frame(&ctx, &mut state, &engine.read(), vec![press(outside)]);
    assert_eq!(second.closed, [HOST_POPUP]);
    assert!(
        crate::adapters::ui::popup::route::path_of_focus(&state, &engine.read())
            .contains(&PopupScope::Surface(SURFACE_L)),
        "엔진 포커스는 L에 남는다"
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
    // 창의 그리기 전 단계처럼 열린 plugin 팝업의 결속 범위를 먼저 기록한다.
    let inst = instance(0);
    crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(
        &mut state,
        [(POPUP, &inst)].into_iter(),
    );
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: PLUGIN.into(),
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
    let picker = state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .map(|p| p.instance);
    assert_ne!(
        state.popup_escape_owner, picker,
        "포커스 경로에 없는 R 피커가 Escape 를 받는다"
    );
}
