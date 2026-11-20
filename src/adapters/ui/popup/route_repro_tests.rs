//! 사용자가 보고한 팝업 범위 결함 두 개의 재현. 아직 고치지 않은 결함은 해제할 단계를 적어 무시해 둔다.
//! 장면: 좌우 surface L·R, R 범위의 plugin 팝업(markdown 경로 팝업과 같은 선언).

use std::sync::Arc;

use egui::{Context, Event, Pos2};
use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupSizeHint};

use super::{SURFACE_L, SURFACE_R, layout, rect, scene, surface_rect_l, to_egui};
use crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::frame::draw_popup_layer;
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
    state.press_snapshot = state.press_snapshot_after_cut.take();
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

/// host 팝업만 그리는 한 프레임. 메인 창처럼 누름은 `frame_press`로 찍고 프레임 끝에 스냅샷을 지우며, 배치한
/// 팝업 노드를 다음 누름의 포인터 대상으로 남긴다.
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
    state.press_snapshot = state.press_snapshot_after_cut.take();
    let result: crate::adapters::ui::popup::PopupDrawResult = result.unwrap();
    // 메인 창처럼 다음 누름의 포인터 대상은 이 프레임에 배치한 팝업 노드다(`frame::draw_popup_layer`).
    state.host_popup_hittest = result.hit_rects.clone();
    state.host_popup_covers = result.covers.clone();
    result
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
            .is_some_and(|p| p.open && state.popups.is_instance_focused(iid))
    );
    let second = host_frame(&ctx, &mut state, &engine.read(), vec![press(outside)]);
    assert_eq!(second.closed, [HOST_POPUP]);
    assert!(
        crate::adapters::ui::popup::route::path_of_focus(&state, &engine.read())
            .contains(&PopupScope::Surface(SURFACE_L)),
        "엔진 포커스는 L에 남는다"
    );
}

/// 결함 2: R 팝업의 자식 파일 피커가 떠 있어도 포커스가 L 이면 키는 L 로 간다. 창의 키 게이트도 열리지 않는다.
#[test]
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
        state.focus_node(&engine.read()),
        crate::state::focus_node::FocusNode::Surface(Some(SURFACE_L)),
        "포커스는 L 인데 키가 오버레이로 간다"
    );
    assert!(
        !state.window_keys_to_overlay(),
        "창의 키 게이트가 L 의 키를 오버레이로 돌린다"
    );
    let picker = state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .map(|p| p.instance);
    assert_ne!(
        state.popups.host_key_owner().map(|p| p.instance),
        picker,
        "포커스 경로에 없는 R 피커가 Escape 를 받는다"
    );
}

/// 창의 한 프레임: 탐색기 찾기 입력란(팝업 층보다 먼저 그리는 L 화면) → host 팝업 층 → plugin 팝업. 입력란이 받은
/// 사건과, 입력란이 포커스를 가졌으면 그 위젯 id 를 돌려준다.
fn window_frame_with_find(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    events: Vec<Event>,
    query: &mut String,
    focus: bool,
) -> (tasty_ui_widgets::ExplorerFindEvents, Option<egui::Id>) {
    let input = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let th = crate::theme::theme();
    let mut out = tasty_ui_widgets::ExplorerFindEvents::default();
    let mut field = None;
    crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, mgr.popup_instances());
    drop(ctx.run(input, |ctx| {
        let take = state.surfaces_take_keys();
        crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || {
            egui::Area::new(egui::Id::new("explorer_find_focus_node_test"))
                .fixed_pos(surface_rect_l().min)
                .show(ctx, |ui| {
                    ui.set_width(300.0);
                    out = tasty_ui_widgets::explorer_find_bar(
                        ui,
                        &th,
                        tasty_ui_widgets::ExplorerFindBar {
                            query,
                            subfolders: None,
                            status: tasty_ui_widgets::ExplorerFindStatus::Text(""),
                            labels: tasty_ui_widgets::ExplorerFindLabels {
                                placeholder: "",
                                subfolders: "",
                                close: "",
                            },
                            focus,
                        },
                    );
                    if out.field_focused {
                        field = ui.ctx().memory(|m| m.focused());
                    }
                });
        });
        draw_popup_layer(ctx, state, engine, &layout());
        draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            Some(&layout()),
        );
    }));
    state.press_snapshot = state.press_snapshot_after_cut.take();
    (out, field)
}

/// 완료 조건 장면. Given R 에 markdown file-open 같은 plugin 팝업과 포커스를 가진 검색창(host)이 있을 때, When 한
/// 이벤트 묶음에서 L 의 탐색기 찾기 입력란을 누르고 글자를 치면, Then 창의 누름 처리(winit 이 누름 스냅샷을 찍는
/// 자리)가 포커스 노드를 L 로 옮겨 그 글자는 창의 키 게이트를 지나 L 입력란에 들어간다. R 의 두 팝업은 닫히지 않고
/// 포커스만 잃는다.
#[test]
fn a_press_on_the_left_field_moves_the_focus_node_and_the_next_letter_goes_there() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_R);
    let engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    // file-open 을 먼저 열고 그 위에 검색창을 연다(검색창이 위라 포커스 노드다).
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let search = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(SURFACE_R))
                .focused(),
            true,
        )
        .unwrap();
    let ctx = Context::default();
    let mut query = String::new();
    // 처음 여는 찾기 바처럼 입력란에 egui 키 포커스를 준다.
    let (_, field) = window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        Vec::new(),
        &mut query,
        true,
    );
    let field = field
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("찾기 입력란이 그려진다");
    window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        Vec::new(),
        &mut query,
        false,
    );
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(search)),
        "누르기 전 포커스 노드는 R 의 검색창이다"
    );

    // 이벤트 묶음 [L 누름, 글자]. 창은 누름 스냅샷을 찍으며 포커스 노드를 옮기고(`mouse::press_snapshot_now`),
    // click-to-activate 로 엔진 포커스를 L 로 옮긴 뒤, 같은 묶음의 글자를 창의 키 게이트로 처리한다.
    let at = field.center();
    assert!(surface_rect_l().contains(at));
    let snapshot = state.take_press_snapshot(&engine.read(), at, egui::PointerButton::Primary);
    state.press_snapshot = Some(snapshot);
    {
        let read = engine.read();
        state
            .navigation
            .select_pane(read.workspace_at(0).unwrap(), super::PANE_L);
    }
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    assert_eq!(
        state.window_feeds_egui(true, true),
        Some(false),
        "글자는 L 입력란(egui 화면)으로 가고 팝업 키 주인은 없다"
    );
    assert!(!state.window_holds_terminal_keys());
    let (after, _) = window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(at), press(at), Event::Text("a".into())],
        &mut query,
        false,
    );
    assert!(after.field_focused, "L 입력란이 포커스를 가진다");
    assert_eq!(query, "a", "글자가 L 입력란에 들어간다");
    assert!(
        state.popups.instance(search).is_some_and(|p| p.open),
        "R 검색창은 닫히지 않는다"
    );
    assert!(!state.popups.is_instance_focused(search));
    assert_eq!(closes_of_popup(&state), [], "R plugin 팝업은 닫히지 않는다");
    assert_eq!(state.plugin_popup_key_target(), None);
}

/// Given R 의 검색창이 포커스 노드이고 창이 이번 그리기 키를 이미 R 에 묶었을 때(앞선 R 키), When 같은 이벤트 묶음에서
/// L 찾기 입력란을 누르고 글자를 치면, Then 창은 누름 자리에서 egui 입력을 끊어(`MainView::cut_key_frame_before_press`
/// 와 같은 순서) 앞선 키는 R 이 묶인 그리기에, 누름과 글자는 다음 그리기에 넣는다. 글자는 창의 키 게이트를 지나 L
/// 입력란에 들어가고 터미널 게이트도 막히지 않는다.
#[test]
fn a_letter_after_a_press_follows_the_press_even_when_an_earlier_key_was_bound() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_R);
    let engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let search = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(SURFACE_R))
                .focused(),
            true,
        )
        .unwrap();
    let ctx = Context::default();
    let mut query = String::new();
    let (_, field) = window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        Vec::new(),
        &mut query,
        true,
    );
    let field = field
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("찾기 입력란이 그려진다");
    window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        Vec::new(),
        &mut query,
        false,
    );
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(search))
    );

    // 묶음 [R 키, L 누름, 글자]. 창은 R 키를 받으며 키 주인(검색창)을 묶는다.
    let mut pending = Vec::new();
    assert_eq!(state.window_feeds_egui(false, true), Some(true));
    pending.push(Event::Text("x".into()));
    // 누름을 egui 입력에 넣기 전에 끊는다.
    let at = field.center();
    let cut = state
        .popups
        .cut_key_frame(pending.len())
        .then_some(pending.len());
    assert_eq!(cut, Some(1), "앞선 결속이 있으니 누름 자리에서 끊는다");
    pending.extend([Event::PointerMoved(at), press(at)]);
    let snapshot = state.take_press_snapshot(&engine.read(), at, egui::PointerButton::Primary);
    state.keep_press_snapshot(snapshot);
    {
        let read = engine.read();
        state
            .navigation
            .select_pane(read.workspace_at(0).unwrap(), super::PANE_L);
    }
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.window_feeds_egui(true, true),
        Some(false),
        "누름 뒤 글자에는 팝업 키 주인이 없다"
    );
    assert!(
        !state.window_holds_terminal_keys(),
        "누름 뒤 글자는 터미널 게이트에서도 막히지 않는다"
    );
    pending.push(Event::Text("a".into()));

    // 그리기 둘: 끊은 앞(R 키)과 끊은 뒤(누름, 글자).
    let rest = pending.split_off(cut.unwrap());
    let (first, _) = window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        pending,
        &mut query,
        false,
    );
    assert!(!first.field_focused);
    assert_eq!(query, "", "R 에 묶인 키는 L 입력란에 들어가지 않는다");
    assert!(
        state.press_snapshot.is_some(),
        "끊은 뒤 누름의 스냅샷은 다음 그리기 몫으로 남는다"
    );
    let (after, _) = window_frame_with_find(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        rest,
        &mut query,
        false,
    );
    assert!(after.field_focused, "L 입력란이 포커스를 가진다");
    assert_eq!(query, "a", "누름 뒤 글자는 L 입력란에 들어간다");
    assert!(state.popups.instance(search).is_some_and(|p| p.open));
    assert_eq!(closes_of_popup(&state), []);
}

/// Given 엔진 포커스가 L 이고 R 에 plugin 팝업(바깥 클릭에 닫힘)만 있을 때, When R 범위 안·팝업 밖을 처음 누르면,
/// Then 닫히지 않고 그 plugin 팝업이 포커스 노드가 된다. When 같은 자리를 다시 누르면, Then 닫힘 요청이다.
#[test]
fn the_first_press_into_a_scope_with_only_a_plugin_popup_focuses_it() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    let mut mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let ctx = Context::default();
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    let placed = state.plugin_popup_hittest[0].rect;
    let r = super::surface_rect_r();
    let outside = egui::pos2(r.center().x, r.max.y - 4.0);
    assert!(r.contains(outside) && !placed.contains(outside));
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    plugin_frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(outside), press(outside)],
    );
    assert_eq!(closes_of_popup(&state), []);
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(POPUP))
    );
    for id in std::mem::take(&mut state.plugin_popup_focus_bumps) {
        mgr.touch_popup_instance_z(id);
    }
    state.press_snapshot = state.press_snapshot_after_cut.take();
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

/// N-B. Given 엔진 포커스가 L 이고 L 을 덮는 host 팝업이 passive 로(에이전트가) 열렸을 때, When 창이 포커스를
/// 정규화하면, Then 덮인 L 은 포커스 노드가 될 수 없어 그 passive 팝업이 포커스 노드이고 키를 받는다. passive 표시는
/// 남는다.
#[test]
fn a_covering_passive_popup_is_the_focus_node() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    let iid = open_host_popup(&mut state, SURFACE_L, false);
    let ctx = Context::default();
    host_frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert_eq!(
        state.popups.focus(),
        None,
        "passive 로 열면 포커스 위치를 쓰지 않는다"
    );
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(iid))
    );
    assert!(state.popups.instance_is_visible_passive(iid));
    assert!(
        state.window_holds_terminal_keys(),
        "키는 터미널로 가지 않는다"
    );
}

/// Given L 에 포커스를 가진 검색창(덮지 않음)이 있고 엔진 포커스도 L 일 때, When 창이 L 의 터미널 본문(검색창 밖)
/// 누름을 처리하면, Then 엔진 포커스는 그대로여도 포커스 노드는 L surface 라 다음 키는 터미널로 간다. 검색창은 열린
/// 채 포커스만 잃는다.
#[test]
fn a_press_on_the_surface_under_its_own_popup_takes_the_focus_from_the_popup() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let search = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(SURFACE_L))
                .focused(),
            true,
        )
        .unwrap();
    let ctx = Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        draw_popup_layer(ctx, &mut state, &engine.read(), &layout());
    }));
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(search))
    );
    let body = egui::pos2(surface_rect_l().center().x, surface_rect_l().max.y - 10.0);
    assert!(!state.popup_owns_point(body), "누른 곳은 검색창 밖이다");
    state.take_press_snapshot(&engine.read(), body, egui::PointerButton::Primary);
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    assert!(
        !state.window_holds_terminal_keys(),
        "다음 키는 터미널로 간다"
    );
    assert!(state.popups.instance(search).is_some_and(|p| p.open));
}

/// Given R 에 포커스를 가진 이름 바꾸기 입력 대화상자가 열려 있을 때, When L 터미널을 누르면, Then 포커스 노드가 L 로
/// 옮겨 이름 바꾸기는 열린 채 키를 가져가지 않는다. 창의 터미널 게이트와 키·IME 오버레이 게이트가 모두 열려 L 의 글자와
/// IME 조합이 터미널로 간다.
#[test]
fn a_rename_dialog_that_lost_the_focus_node_does_not_hold_the_terminal_keys() {
    use crate::adapters::ui::rename_popup::RENAME_POPUP_ID;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_R);
    let engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let rename = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(RENAME_POPUP_ID, PopupScope::Surface(SURFACE_R))
                .focused(),
            true,
        )
        .unwrap();
    assert!(state.input_dialog_owns_keys());
    assert!(state.window_holds_terminal_keys());

    let at = surface_rect_l().center();
    let snapshot = state.take_press_snapshot(&engine.read(), at, egui::PointerButton::Primary);
    state.keep_press_snapshot(snapshot);
    {
        let read = engine.read();
        state
            .navigation
            .select_pane(read.workspace_at(0).unwrap(), super::PANE_L);
    }
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    assert!(state.popups.instance(rename).is_some_and(|p| p.open));
    assert!(
        state.has_input_dialog_open(),
        "이름 바꾸기는 열린 채 남는다"
    );
    assert!(!state.input_dialog_owns_keys());
    assert_eq!(
        state.window_feeds_egui(false, true),
        None,
        "L 터미널의 키·IME 는 egui 로 가지 않는다"
    );
    assert!(
        !state.window_keys_to_overlay(),
        "IME 오버레이 게이트가 열린다"
    );
    assert!(
        !state.window_holds_terminal_keys(),
        "L 의 글자는 터미널로 간다"
    );
}

/// J7. Given L 의 HTML 주소 띠가 키보드 포커스를 가진 포커스 노드일 때, When 사용자가 팝업을 포커스를 주며 열면,
/// Then 포커스 노드는 그 팝업 하나이고 주소 띠는 포커스를 놓는다(다음 그리기에서 편집을 끝낸다). 주소 띠 키 재해석은
/// 포커스 노드가 주소 띠일 때만 돌므로 팝업 키를 바꾸지 않는다.
#[test]
fn a_popup_taking_the_focus_node_releases_the_html_address_band() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(super::PANE_L);
    let engine = session.borrow_mut();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state.html_address.set_focused_for_test(SURFACE_L);
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::HtmlAddress(SURFACE_L)
    );
    let search = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(SURFACE_L))
                .focused(),
            true,
        )
        .unwrap();
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(search))
    );
    assert!(
        !state.html_address.has_keyboard_focus(),
        "주소 띠는 포커스를 놓는다"
    );
}
