//! 한 프레임의 키 주인은 하나다. 창의 Esc 판정은 그리기보다 먼저 돌며 정한 대상을 그 프레임 키 주인으로 묶고,
//! host 본문과 plugin 팝업 렌더가 그 값을 쓴다. 장면: L 범위 피커 H(맨 위), R 범위 plugin 팝업 P(가운데), R 범위
//! 피커 G(맨 아래). 창의 한 프레임처럼 결속 범위 동기화 → host 팝업 층 → plugin 팝업 순서로 그린다.

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
pub(super) const POPUP: u64 = 9;

pub(super) fn plugin_popup(
    z_seq: u64,
    escape: tasty_plugin_protocol::popup_decl::PopupEscape,
) -> tasty_host_plugin::PopupInstance {
    let layout = tasty_plugin_manifest::PopupLayout {
        escape,
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

/// G(R 피커) → P(R plugin 팝업) → H(L 피커) 순서로 열어 순번이 H > P > G 다. 엔진 포커스는 L 이라 L 을 덮는 H 가
/// 포커스 노드다(피커는 plugin 이 열어 passive 지만 덮는 노드다).
fn open_scene() -> Scene {
    open_scene_with(tasty_plugin_protocol::popup_decl::PopupEscape::Close)
}

/// [`open_scene`]과 같고, P 의 Esc 선언이 `escape` 다.
fn open_scene_with(escape: tasty_plugin_protocol::popup_decl::PopupEscape) -> Scene {
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
            plugin_popup(tasty_host_plugin::next_popup_z_seq(), escape),
        );
        let h = open_picker(&mut state, &mut engine, SURFACE_L);
        // 피커의 필요 크기는 반쪽 surface 보다 커서 접힌다. 본문을 그리도록 범위 안에 드는 크기로 고정한다.
        for iid in [h, g] {
            let p = state.popups.instance_mut(iid).unwrap();
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

/// 사용자가 P 를 누른 것처럼 P 를 창의 포커스 노드로 한다.
pub(super) fn focus_p(state: &mut RequestContext) {
    state
        .popups
        .set_focus(Some(crate::adapters::ui::popup::focus::PopupNode::Plugin(
            POPUP,
        )));
}

fn is_open(state: &RequestContext, iid: PopupInstanceId) -> bool {
    state.popups.instance(iid).is_some_and(|p| p.open)
}

fn plugin_escaped(state: &RequestContext) -> bool {
    state.plugin_popup_closes.iter().any(|(id, _)| *id == POPUP)
}

/// 창의 Esc 처리(`MainViewState::window_escape`)를 거쳐 그 Esc 를 실은 프레임을 그린다. 창은 그리기보다 먼저 돌므로
/// 직전 렌더의 범위·후보로 대상을 정하고, 그 프레임 키 주인은 창이 정한 대상이다. 창이 처리했으면 참이다.
fn escape_through_the_window(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    layout: &LayoutContext,
) -> bool {
    let mut events = escape();
    let consumed = state.window_escape(
        &mut events,
        Some(0),
        ctx.input(|i| i.key_down(egui::Key::Escape)),
    );
    frame(ctx, state, engine, mgr, layout, events);
    consumed
}

/// Given H > P > G 이고 직전 렌더에 H 가 보였을 때, When H 의 범위(L)가 레이아웃에서 빠진 뒤 다시 그리기 전에 Esc 가
/// 오면, Then 창은 키 주인 H 의 포커스를 풀고 그 Esc 를 H 에 묶는다. 그 프레임 키 주인도 H 라 숨은 H 외에 P·G 는
/// 그 Esc 를 받지 않는다.
#[test]
fn a_scope_hidden_before_the_redraw_keeps_the_escape_on_the_window_target() {
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

    assert!(
        escape_through_the_window(&ctx, &mut state, &engine.read(), &mgr, &layout_without_l()),
        "창이 H 의 포커스를 푼다"
    );
    assert!(!state.popups.is_instance_focused(h), "창의 대상은 H 다");
    assert!(!plugin_escaped(&state), "P 는 받지 않는다");
    assert!(
        is_open(&state, g) && !state.popups.is_instance_focused(g),
        "G 는 받지 않는다"
    );
}

/// Given 직전 렌더에 H 가 숨고 사용자가 누른 P 가 포커스 노드일 때, When H 의 범위가 다시 보이는 프레임에 Esc 가 오면, Then 창이 고른
/// P 가 그 프레임 키 주인이라 P 하나만 받는다. 다시 보인 H 와 G 는 받지 않는다.
#[test]
fn a_scope_shown_again_before_the_redraw_keeps_the_escape_on_the_window_target() {
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
    focus_p(&mut state);
    assert!(is_open(&state, h) && is_open(&state, g) && !plugin_escaped(&state));

    assert!(
        escape_through_the_window(&ctx, &mut state, &engine.read(), &mgr, &layout()),
        "plugin 키 주인 갈래: 창이 처리하고 그 Esc 를 P 의 렌더에 넘긴다"
    );
    assert!(plugin_escaped(&state), "창이 고른 P 가 받는다");
    assert!(
        is_open(&state, h) && !state.popups.is_instance_focused(h),
        "다시 보인 H 는 받지 않는다"
    );
    assert!(
        is_open(&state, g) && !state.popups.is_instance_focused(g),
        "G 는 받지 않는다"
    );
}

fn escape_with(modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Given 키 주인이 피커 H 일 때, When 창이 수식키 없는 Esc 로 H 의 포커스를 풀고 그 Esc 를 H 본문에 넘기면, Then 넘긴
/// 사건은 원래 수식키(없음)를 지니고 H 를 닫는다. Ctrl+Esc 는 창 Esc 가 아니라 창이 처리하지 않고 그 사건이 egui 큐에
/// 남는다.
#[test]
fn the_escape_handed_to_the_owner_keeps_its_modifiers() {
    {
        use crate::adapters::ui::popup::window_input_test_support::WindowInput;
        let Scene {
            mut state,
            mut session,
            mgr,
            h,
            g: _,
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
        let mut input = WindowInput::default();
        assert!(
            !input.escape_with(
                &ctx,
                &mut state,
                &engine.read(),
                true,
                egui::Modifiers::CTRL
            ),
            "Ctrl+Esc 는 창이 처리하지 않는다"
        );
        assert!(input.escape_fed());
        assert_eq!(
            input.queue_mut().as_slice(),
            [escape_with(egui::Modifiers::CTRL)],
            "그 사건이 egui 큐에 남는다"
        );
        assert!(is_open(&state, h));
    }
    let Scene {
        mut state,
        mut session,
        mgr,
        h,
        g: _,
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
    assert!(is_open(&state, h));
    let mut events = vec![escape_with(egui::Modifiers::NONE)];
    assert!(
        state.window_escape(&mut events, Some(0), false),
        "창이 H 의 포커스를 푼다"
    );
    assert!(events.is_empty(), "창은 그 Esc 를 egui 입력에서 뺀다");
    frame(&ctx, &mut state, &engine.read(), &mgr, &layout(), events);
    assert!(
        !is_open(&state, h),
        "H 본문이 받은 수식키 없는 Esc 로 닫힌다"
    );
}

/// 탐색기 찾기 입력란을 팝업 층보다 먼저 그리는 창의 한 프레임(`egui_bridge::run_egui_frame` 순서: 화면은
/// `surfaces_take_keys` 문 안 → host 팝업 층 → plugin 팝업 렌더).
#[allow(clippy::too_many_arguments)]
fn frame_with_find(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    layout: &LayoutContext,
    events: Vec<Event>,
    query: &mut String,
    focus: bool,
) -> tasty_ui_widgets::ExplorerFindEvents {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        )),
        events,
        ..Default::default()
    };
    let th = crate::theme::theme();
    let mut out = tasty_ui_widgets::ExplorerFindEvents::default();
    drop(ctx.run(input, |ctx| {
        let take = state.surfaces_take_keys();
        crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || {
            egui::Area::new(egui::Id::new("explorer_find_test"))
                .fixed_pos(egui::pos2(0.0, 650.0))
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
                });
        });
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
    out
}

/// Given 탐색기 찾기 입력란에 egui 키 포커스와 검색어가 있고 plugin 팝업 P 가 키 주인일 때, When 창이 Esc 를 받아 P
/// 의 렌더가 그 Esc 로 P 를 닫으면, Then 찾기 입력란은 포커스·검색어·열림이 그대로다. 창은 그 Esc 를 egui 입력에서
/// 빼고 P 의 렌더 동안만 되넣으므로 egui 의 패스 시작 처리(키 포커스 해제)가 그 Esc 를 보지 않는다.
#[test]
fn an_escape_that_closes_a_plugin_owner_does_not_reach_a_surface_field() {
    let Scene {
        mut state,
        mut session,
        mgr,
        h: _,
        g: _,
    } = open_scene();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let mut query = String::from("abc");
    // L 이 보이지 않아 H 가 숨고 사용자가 누른 P 가 키 주인이다. 처음 여는 찾기 바처럼 입력란에 egui 키 포커스를
    // 준다.
    let draw = |state: &mut RequestContext, query: &mut String, events, focus| {
        frame_with_find(
            &ctx,
            state,
            &engine.read(),
            &mgr,
            &layout_without_l(),
            events,
            query,
            focus,
        )
    };
    draw(&mut state, &mut query, Vec::new(), true);
    focus_p(&mut state);
    let before = draw(&mut state, &mut query, Vec::new(), false);
    assert!(before.field_focused, "찾기 입력란이 키 포커스를 가진다");
    assert!(!plugin_escaped(&state));

    let mut events = escape();
    assert!(
        state.window_escape(&mut events, Some(0), false),
        "plugin 키 주인 갈래"
    );
    assert!(events.is_empty(), "창은 그 Esc 를 egui 입력에서 뺀다");
    let after = draw(&mut state, &mut query, events, false);
    assert!(plugin_escaped(&state), "P 의 렌더가 그 Esc 로 P 를 닫는다");
    assert!(!after.escape, "찾기 입력란은 Esc 를 보지 않는다");
    assert!(after.field_focused, "찾기 입력란의 포커스가 그대로다");
    assert!(!after.close, "찾기 바를 닫지 않는다");
    assert_eq!(query, "abc", "검색어가 그대로다");
}

fn escape_release() -> Event {
    Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 이번 프레임 plugin 렌더가 팝업에 보낸 Escape 사건(눌림, 반복).
fn sent_escape_keys(state: &mut RequestContext) -> Vec<(bool, bool)> {
    use crate::app::frame_effect::{DrawEffect, FrameEffect};
    use crate::app::plugin_display::PluginDisplayRequest;
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|d| match d.body {
            crate::intent::Intent::Frame(FrameEffect::Draw(DrawEffect::PluginDisplay(
                PluginDisplayRequest::Popup { params, .. },
            ))) => Some(params.raw_input.events),
            _ => None,
        })
        .flatten()
        .filter_map(|e| match e {
            tasty_plugin_protocol::protocol::RawInputEventWire::Key {
                key,
                pressed,
                repeat,
                ..
            } if key == "Escape" => Some((pressed, repeat)),
            _ => None,
        })
        .collect()
}

/// 창이 Esc 누름을 처리한 뒤 같은 프레임에 그 뗌이 온다(뗌은 창이 처리하지 않아 egui 입력에 남는다).
fn quick_escape(ctx: &Context, state: &mut RequestContext) -> Vec<Event> {
    let mut events = escape();
    assert!(
        state.window_escape(
            &mut events,
            Some(0),
            ctx.input(|i| i.key_down(egui::Key::Escape))
        ),
        "plugin 키 주인 갈래"
    );
    events.push(escape_release());
    events
}

/// Given 키 주인이 plugin 팝업 P 일 때, When 한 프레임에 Esc 누름과 뗌이 함께 오면(창은 누름을 처리해 P 에 넘긴다),
/// Then P 에는 egui 가 처리했을 때처럼 누름 → 뗌 순서로 가고 다음 프레임 눌린 키 상태에 Escape 가 없다.
#[test]
fn a_quick_escape_reaches_the_plugin_owner_as_press_then_release() {
    let Scene {
        mut state,
        mut session,
        mgr,
        h: _,
        g: _,
    } = open_scene();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let draw = |state: &mut RequestContext, events| {
        frame(
            &ctx,
            state,
            &engine.read(),
            &mgr,
            &layout_without_l(),
            events,
        )
    };
    draw(&mut state, Vec::new());
    focus_p(&mut state);
    state.take_pending_intents();
    let events = quick_escape(&ctx, &mut state);
    draw(&mut state, events);
    assert_eq!(
        sent_escape_keys(&mut state),
        vec![(true, false), (false, false)],
        "P 는 누름 → 뗌 순서로 받는다"
    );
    assert!(!ctx.input(|i| i.key_down(egui::Key::Escape)));
    draw(&mut state, Vec::new());
    assert!(
        !ctx.input(|i| i.key_down(egui::Key::Escape)),
        "다음 프레임에 Escape 가 눌린 채로 남지 않는다"
    );
}

/// 프레임 뒤 앱이 렌더 요청을 처리하는 단계(`App::dispatch_plugin_popup_events`)를 같은 순서로 밟는다. popup 닫기
/// 요청은 자식 피커 정리(`cancel_child_file_picker`)를 거쳐 돌려준다. 매니저에 popup 닫기를 적용하는 호출은 앱 처리
/// 파일 밖에서 부르지 않으므로(doc guard `plugin_popup_close_chokepoint`) 부른 쪽이 그 목록을 단정한다. banner 닫기와
/// popup 순서 올림은 매니저에 적용한다.
fn dispatch_like_app(
    state: &mut RequestContext,
    mgr: &mut PluginManager,
) -> Vec<(u64, tasty_plugin_protocol::PopupCloseReason)> {
    let closes = std::mem::take(&mut state.plugin_popup_closes);
    crate::app::dispatch::plugin_popup_events::cancel_child_file_picker(state, &closes);
    for (id, reason) in std::mem::take(&mut state.plugin_banner_closes) {
        mgr.close_banner_instance(id, reason);
    }
    for id in std::mem::take(&mut state.plugin_popup_focus_bumps) {
        mgr.touch_popup_instance_z(id);
    }
    closes
}

/// Given 키 주인이 Esc 선언 `Unfocus`(Esc 로 닫히지 않음)인 plugin 팝업 P 일 때, When Esc 를 누른 채 두 프레임에 걸쳐
/// 누름이 오면, Then 첫 프레임에 창이 넘긴 새 누름을 P 가 받고(창을 거치지 않고 egui 가 처리한 결과와 같다) 그 입력을
/// 보낸 뒤 P 의 포커스가 풀린다. P 는 닫는 요청 없이 열린 채다. 둘째 프레임의 누름은 P 가 키 주인이 아니라 받지 않는다.
#[test]
fn a_held_escape_reaches_an_unfocus_plugin_owner_once_and_releases_it() {
    let Scene {
        mut state,
        mut session,
        mut mgr,
        h: _,
        g: _,
    } = open_scene_with(tasty_plugin_protocol::popup_decl::PopupEscape::Unfocus);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let draw = |state: &mut RequestContext, mgr: &PluginManager, events| {
        frame(
            &ctx,
            state,
            &engine.read(),
            mgr,
            &layout_without_l(),
            events,
        )
    };
    draw(&mut state, &mgr, Vec::new());
    focus_p(&mut state);
    assert!(dispatch_like_app(&mut state, &mut mgr).is_empty());
    state.take_pending_intents();

    let mut events = escape();
    assert!(
        state.window_escape(
            &mut events,
            Some(0),
            ctx.input(|i| i.key_down(egui::Key::Escape))
        ),
        "plugin 키 주인 갈래"
    );
    draw(&mut state, &mgr, events);
    assert_eq!(
        sent_escape_keys(&mut state),
        vec![(true, false)],
        "P 가 받은 Esc(눌림, 반복)"
    );
    let mut plain_seen = Vec::new();
    drop(Context::default().run(
        egui::RawInput {
            events: escape(),
            ..Default::default()
        },
        |ctx| {
            ctx.input(|i| {
                for e in &i.events {
                    if let Event::Key {
                        pressed, repeat, ..
                    } = e
                    {
                        plain_seen.push((*pressed, *repeat));
                    }
                }
            })
        },
    ));
    assert_eq!(
        plain_seen,
        vec![(true, false)],
        "창을 거치지 않은 egui 결과와 같다"
    );
    let closes = dispatch_like_app(&mut state, &mut mgr);
    assert!(
        closes.iter().all(|(id, _)| *id != POPUP),
        "Unfocus 선언 P 는 Esc 로 닫히지 않는다: {closes:?}"
    );
    assert!(
        mgr.popup_instances().any(|(id, _)| id == POPUP),
        "P 가 열린 채다"
    );
    assert_ne!(
        state.popups.focus(),
        Some(crate::adapters::ui::popup::focus::PopupNode::Plugin(POPUP)),
        "입력을 보낸 뒤 P 의 포커스가 풀린다"
    );

    let mut events = escape();
    let handled = state.window_escape(
        &mut events,
        Some(0),
        ctx.input(|i| i.key_down(egui::Key::Escape)),
    );
    draw(&mut state, &mgr, events);
    assert!(
        sent_escape_keys(&mut state).is_empty(),
        "둘째 누름은 P 가 받지 않는다(창 처리 {handled})"
    );
}

/// 창의 egui 패스 하나(`GpuState::run_main_pass`): 다음 몫의 입력을 꺼내 그리고 그 몫을 닫는다.
fn window_pass(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    input: &mut crate::adapters::ui::popup::window_input_test_support::WindowInput,
) {
    let events = input.take_pass(state, ctx);
    frame(ctx, state, engine, mgr, &layout_without_l(), events);
    state.close_input_pass(ctx);
}

/// Given plugin 팝업 P 가 포커스 노드일 때, When 한 묶음에 [P 에 넘긴 Esc 누름, 팝업 밖 누름, egui 로 가지 않는 Esc
/// 뗌]이 오면, Then 그 뗌은 P 의 누름을 그린 패스 뒤에 egui 의 Escape 눌림을 푼다. 다시 P 에 포커스를 주고 누른 Esc 는
/// 반복이 아니다. 뗌을 받은 즉시 풀면 앞 패스가 넘긴 누름이 눌림을 다시 남겨 다음 누름이 반복이 된다.
#[test]
fn an_unfed_escape_release_after_a_press_releases_after_the_escape_it_follows() {
    use crate::adapters::ui::popup::window_input_test_support::WindowInput;
    let Scene {
        mut state,
        mut session,
        mgr,
        h: _,
        g: _,
    } = open_scene_with(tasty_plugin_protocol::popup_decl::PopupEscape::Unfocus);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    window_pass(&ctx, &mut state, &engine.read(), &mgr, &mut input);
    focus_p(&mut state);
    state.take_pending_intents();

    assert!(
        input.escape(&ctx, &mut state, &engine.read(), true),
        "창이 P 에 넘긴다"
    );
    // L 은 이 배치에서 보이지 않아 어느 팝업 노드도 덮지 않는다.
    let outside = super::surface_rect_l().center();
    assert!(!state.popup_owns_point(outside));
    input.press(&mut state, &engine.read(), outside);
    assert_eq!(
        state.popup_focus(),
        None,
        "누름이 팝업 밖으로 포커스를 옮긴다"
    );
    assert!(!input.escape(&ctx, &mut state, &engine.read(), false));
    while !input.is_empty() {
        window_pass(&ctx, &mut state, &engine.read(), &mgr, &mut input);
    }
    assert_eq!(
        sent_escape_keys(&mut state),
        vec![(true, false)],
        "P 는 넘긴 누름을 받는다"
    );
    assert!(
        !ctx.input(|i| i.key_down(egui::Key::Escape)),
        "뗌이 실린 몫을 닫은 뒤 Escape 가 눌린 채 남지 않는다"
    );

    focus_p(&mut state);
    assert!(input.escape(&ctx, &mut state, &engine.read(), true));
    window_pass(&ctx, &mut state, &engine.read(), &mgr, &mut input);
    assert_eq!(
        sent_escape_keys(&mut state),
        vec![(true, false)],
        "다음 Esc 는 반복이 아니다"
    );
}

/// plugin 팝업 P(R 범위)만 연 장면. 엔진 포커스는 L 이고 팝업 포커스는 없다.
pub(super) fn plugin_only_scene() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    PluginManager,
) {
    crate::i18n::init("en");
    let (mut state, session) = scene(super::PANE_L);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.insert_popup_instance_for_test(
        POPUP,
        plugin_popup(
            tasty_host_plugin::next_popup_z_seq(),
            tasty_plugin_protocol::popup_decl::PopupEscape::Close,
        ),
    );
    (state, session, mgr)
}

/// plugin 렌더 입력: Esc 가 아닌 키 누름 하나와 IME 사건 하나. 포인터 누름은 넣지 않는다(누름 스냅샷이 포커스를
/// 옮기지 않게).
fn render_input() -> Vec<Event> {
    vec![
        Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        Event::Ime(egui::ImeEvent::Preedit("ㄱ".into())),
    ]
}

/// 렌더 입력으로 한 프레임을 그린다. `with_host`면 host 팝업 층을 먼저 그린다. `before_render`는 plugin 렌더 직전
/// 상태를 단정한다.
fn render_frame(
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    with_host: bool,
    before_render: &dyn Fn(&RequestContext),
) {
    render_frame_on(
        &Context::default(),
        state,
        engine,
        mgr,
        with_host,
        before_render,
    );
}

/// [`render_frame`]와 같고 `ctx` 에 그린다(여러 프레임에 걸친 egui 상태를 보는 시험).
pub(super) fn render_frame_on(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    with_host: bool,
    before_render: &dyn Fn(&RequestContext),
) {
    let layout = layout();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        )),
        events: render_input(),
        ..Default::default()
    };
    drop(ctx.run(input, |ctx| {
        crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, mgr.popup_instances());
        if with_host {
            draw_popup_layer(ctx, state, engine, &layout);
        }
        before_render(state);
        draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            Some(&layout),
        );
    }));
}

/// 이번 프레임 plugin 렌더가 P 에 보낸 요청의 입력. 요청이 없으면 `None`이다.
pub(super) fn sent_input(
    state: &mut RequestContext,
) -> Option<tasty_plugin_protocol::protocol::RawInputWire> {
    use crate::app::frame_effect::{DrawEffect, FrameEffect};
    use crate::app::plugin_display::PluginDisplayRequest;
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|d| match d.body {
            crate::intent::Intent::Frame(FrameEffect::Draw(DrawEffect::PluginDisplay(
                PluginDisplayRequest::Popup { params, .. },
            ))) if params.instance_id == POPUP => Some(params.raw_input),
            _ => None,
        })
        .last()
}

/// 요청 입력에 Esc 가 아닌 키 사건과 IME 사건이 있는지.
pub(super) fn key_and_ime(input: &tasty_plugin_protocol::protocol::RawInputWire) -> (bool, bool) {
    use tasty_plugin_protocol::protocol::RawInputEventWire as W;
    (
        input
            .events
            .iter()
            .any(|e| matches!(e, W::Key { key, .. } if key != "Escape")),
        input.events.iter().any(|e| matches!(e, W::Ime { .. })),
    )
}

/// 팝업이 아닌 포커스 노드(엔진 포커스 surface)에서 키를 받은 몫을 누름으로 끊어 닫는다. 닫힌 몫은 `NoPopup`으로
/// 묶이고 뒤 몫은 결속이 없다. 그 뒤 포커스를 P 로 옮긴다(끊기는 다시 부르지 않는다).
fn close_a_no_popup_segment_then_focus_p(state: &mut RequestContext, core: &tasty_core::CoreState) {
    assert!(
        state.popup_focus().is_none(),
        "끊는 몫의 포커스 노드는 팝업이 아니다"
    );
    let typed = vec![Event::Key {
        key: egui::Key::B,
        physical_key: Some(egui::Key::B),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    assert_eq!(
        state.cut_key_frame(
            core,
            &typed,
            crate::adapters::ui::popup::frame_keys::Incoming::Press
        ),
        1,
        "키가 든 몫을 누름이 한 번 끊는다"
    );
    assert_eq!(
        state.popups.frame_binding(),
        Some(None),
        "닫힌 몫은 NoPopup 이다"
    );
    focus_p(state);
}

fn plugin_owner() -> Option<crate::adapters::ui::popup::occlusion::PopupKeyOwner> {
    Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(POPUP))
}

/// Given plugin 팝업 P 가 보이고 팝업 키 주인이 없을 때, When host 팝업 층과 plugin 렌더를 그리면, Then P 의 요청은
/// 키 포커스가 없고 키·IME 사건을 싣지 않는다.
#[test]
fn a_plugin_popup_takes_no_keys_in_a_frame_without_a_key_owner() {
    let (mut state, mut session, mgr) = plugin_only_scene();
    let engine = session.borrow_mut();
    render_frame(&mut state, &engine.read(), &mgr, true, &|state| {
        assert_eq!(
            state.frame_key_owner,
            Some(None),
            "전제(무주인): 결정은 키 주인 없음"
        );
        assert_eq!(
            state.popups.frame_binding(),
            None,
            "전제(무주인): 몫 결속 없음"
        );
        assert_eq!(
            state.popup_key_owner(),
            None,
            "전제(무주인): live 키 주인 없음"
        );
    });
    let input = sent_input(&mut state).expect("P 의 Popup 요청이 없다");
    assert!(!input.focused, "(무주인) wire.raw_input.focused 가 참이다");
    assert_eq!(
        key_and_ime(&input),
        (false, false),
        "키 주인 없는 프레임의 P 는 키·IME 를 받지 않는다"
    );
}

/// Given 창이 host 피커 H 를 이 프레임 키 주인으로 묶었고 plugin 팝업 P 가 함께 보일 때, When 그리면, Then 결정은
/// Host 이고 P 의 요청은 키 포커스가 없고 키·IME 사건을 싣지 않는다.
#[test]
fn a_bound_host_render_gives_the_plugin_popup_no_keys() {
    let Scene {
        mut state,
        mut session,
        mgr,
        h,
        ..
    } = open_scene();
    let engine = session.borrow_mut();
    state
        .popups
        .bind_input_key_owner(crate::adapters::ui::popup::focus::PopupNode::Host(h));
    render_frame(&mut state, &engine.read(), &mgr, true, &|state| {
        assert_eq!(
            state.frame_key_owner,
            Some(Some(
                crate::adapters::ui::popup::occlusion::PopupKeyOwner::Host
            )),
            "전제(결속 Host): 결정은 Host"
        );
        assert_eq!(
            state.popups.frame_binding(),
            None,
            "전제(결속 Host): 몫 결속은 꺼냈다"
        );
    });
    let input = sent_input(&mut state).expect("P 의 Popup 요청이 없다");
    assert!(
        !input.focused,
        "(결속 Host) wire.raw_input.focused 가 참이다"
    );
    assert_eq!(
        key_and_ime(&input),
        (false, false),
        "Host 결정 프레임의 P 는 키·IME 를 받지 않는다"
    );
}

/// Given 팝업이 아닌 노드의 몫(`NoPopup`)을 host 그리기가 이 프레임 결정으로 정했고 그 뒤 포커스가 P 로 옮겼을 때,
/// When plugin 렌더를 그리면, Then 결정 `Some(None)`을 따라 P 는 키를 받지 않는다(대체 경로로 떨어지지 않는다).
#[test]
fn a_decided_no_popup_keeps_the_fallback_plugin_from_keys() {
    let (mut state, mut session, mgr) = plugin_only_scene();
    let engine = session.borrow_mut();
    close_a_no_popup_segment_then_focus_p(&mut state, engine.read().core);
    render_frame(&mut state, &engine.read(), &mgr, true, &|state| {
        assert_eq!(
            state.frame_key_owner,
            Some(None),
            "전제(가): 결정 Some(None)"
        );
        assert_eq!(
            state.popups.frame_binding(),
            None,
            "전제(가): 뒤 몫 결속 없음"
        );
        assert_eq!(
            state.popup_key_owner(),
            plugin_owner(),
            "전제(가): live 키 주인은 P"
        );
    });
    let input = sent_input(&mut state).expect("P 의 Popup 요청이 없다");
    assert!(!input.focused, "(가) wire.raw_input.focused 가 참이다");
    assert_eq!(
        key_and_ime(&input),
        (false, false),
        "결정 NoPopup 프레임의 P 는 키·IME 를 받지 않는다"
    );
}

/// Given 팝업이 아닌 노드의 몫(`NoPopup`) 결속이 남았고 결정이 없으며 포커스가 P 일 때, When host 그리기 없이 plugin
/// 렌더만 그리면, Then 몫 결속을 따라 P 는 키를 받지 않는다(live 판정으로 떨어지지 않는다).
#[test]
fn a_no_popup_binding_keeps_the_fallback_plugin_from_keys() {
    let (mut state, mut session, mgr) = plugin_only_scene();
    let engine = session.borrow_mut();
    close_a_no_popup_segment_then_focus_p(&mut state, engine.read().core);
    render_frame(&mut state, &engine.read(), &mgr, false, &|state| {
        assert_eq!(state.frame_key_owner, None, "전제(나): 결정 없음");
        assert_eq!(
            state.popups.frame_binding(),
            Some(None),
            "전제(나): 몫 결속 NoPopup"
        );
        assert_eq!(
            state.popup_key_owner(),
            plugin_owner(),
            "전제(나): live 키 주인은 P"
        );
    });
    let input = sent_input(&mut state).expect("P 의 Popup 요청이 없다");
    assert!(!input.focused, "(나) wire.raw_input.focused 가 참이다");
    assert_eq!(
        key_and_ime(&input),
        (false, false),
        "NoPopup 결속 몫의 P 는 키·IME 를 받지 않는다"
    );
}

/// Given 결정도 몫 결속도 없고 포커스가 P 일 때, When plugin 렌더만 그리면, Then live 판정으로 P 가 키를 받는다.
#[test]
fn with_no_decision_or_binding_the_live_plugin_takes_keys() {
    let (mut state, mut session, mgr) = plugin_only_scene();
    let engine = session.borrow_mut();
    focus_p(&mut state);
    render_frame(&mut state, &engine.read(), &mgr, false, &|state| {
        assert_eq!(state.frame_key_owner, None, "전제(다): 결정 없음");
        assert_eq!(state.popups.frame_binding(), None, "전제(다): 몫 결속 없음");
        assert_eq!(
            state.popup_key_owner(),
            plugin_owner(),
            "전제(다): live 키 주인은 P"
        );
    });
    let input = sent_input(&mut state).expect("P 의 Popup 요청이 없다");
    assert!(input.focused, "(다) wire.raw_input.focused 가 거짓이다");
    assert_eq!(
        key_and_ime(&input),
        (true, true),
        "live 키 주인 P 는 키·IME 를 받는다"
    );
}

/// 결정 전 준비. 상태, 코어, 장면의 H·G 를 받는다.
type Setup =
    Box<dyn Fn(&mut RequestContext, &tasty_core::CoreState, PopupInstanceId, PopupInstanceId)>;

/// 장면 H(L 피커)·G(R 피커)·P(R plugin)에서 `setup` 뒤 이 프레임 키를 정하고, host 쪽 키 대상(`FrameKeys.host`)과
/// plugin 쪽 대상(`frame_plugin_key_target`)을 돌려준다.
fn decide_after(
    setup: &Setup,
) -> (
    PopupInstanceId,
    PopupInstanceId,
    Option<PopupInstanceId>,
    Option<u64>,
) {
    let Scene {
        mut state,
        mut session,
        mgr,
        h,
        g,
    } = open_scene();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let layout = layout();
    let mut keys = (None, None);
    drop(ctx.run(egui::RawInput::default(), |ctx| {
        crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(
            &mut state,
            mgr.popup_instances(),
        );
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        setup(&mut state, engine.read().core, h, g);
        state.decide_frame_key_owner(Some(&layout));
        let host = state.popups.take_frame_keys().host;
        let decided = state.frame_key_owner.take();
        keys = (host, state.frame_plugin_key_target(decided));
    }));
    (h, g, keys.0, keys.1)
}

/// Given 이 프레임 결정이 (결속 Host / 결속 Plugin / NoPopup 몫 / 결속 없음의 live host·plugin) 일 때, When 공통 판정
/// 함수로 각 후보를 판정하면, Then 경우마다 정해진 노드 하나만(NoPopup 이면 아무도) 이 프레임 키를 받는다. host 는
/// `FrameKeys.host`, plugin 은 `frame_plugin_key_target`의 값을 판정 함수에 넣는다.
#[test]
fn each_frame_decision_gives_its_keys_to_one_node_only() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::adapters::ui::popup::frame_keys::receives_frame_keys as judge;
    // (경우, 결정 전 준비, H·G·P 가 받는지)
    let cases: Vec<(&str, Setup, [bool; 3])> = vec![
        (
            "결속 Host",
            Box::new(|state, _, h, _| state.popups.bind_input_key_owner(PopupNode::Host(h))),
            [true, false, false],
        ),
        (
            "결속 Plugin",
            Box::new(|state, _, _, _| state.popups.bind_input_key_owner(PopupNode::Plugin(POPUP))),
            [false, false, true],
        ),
        (
            "NoPopup",
            Box::new(|state, core, _, _| {
                state.popups.set_focus(None);
                assert!(
                    state.popup_focus().is_none(),
                    "NoPopup: 포커스 노드는 팝업이 아니다"
                );
                let typed = vec![Event::Key {
                    key: egui::Key::B,
                    physical_key: Some(egui::Key::B),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }];
                assert_eq!(
                    state.cut_key_frame(
                        core,
                        &typed,
                        crate::adapters::ui::popup::frame_keys::Incoming::Press
                    ),
                    1,
                    "NoPopup: 키가 든 몫을 누름이 끊는다"
                );
            }),
            [false, false, false],
        ),
        (
            "live host",
            Box::new(|state, _, _, g| state.popups.set_focus(Some(PopupNode::Host(g)))),
            [false, true, false],
        ),
        (
            "live plugin",
            Box::new(|state, _, _, _| focus_p(state)),
            [false, false, true],
        ),
    ];
    for (case, setup, [h_takes, g_takes, p_takes]) in cases {
        let (h, g, host, plugin) = decide_after(&setup);
        assert_eq!(
            judge(host, h),
            h_takes,
            "{case}: H 판정 (host 대상 {host:?})"
        );
        assert_eq!(
            judge(host, g),
            g_takes,
            "{case}: G 판정 (host 대상 {host:?})"
        );
        assert_eq!(
            judge(plugin, POPUP),
            p_takes,
            "{case}: P 판정 (plugin 대상 {plugin:?})"
        );
    }
}

/// 두 번째 plugin 팝업. P 와 같은 선언으로 R 에 연다.
const OTHER_POPUP: u64 = 10;

/// 이번 프레임 plugin 렌더가 보낸 모든 plugin 팝업 Popup 요청의 (인스턴스, 입력).
fn sent_inputs(
    state: &mut RequestContext,
) -> Vec<(u64, tasty_plugin_protocol::protocol::RawInputWire)> {
    use crate::app::frame_effect::{DrawEffect, FrameEffect};
    use crate::app::plugin_display::PluginDisplayRequest;
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|d| match d.body {
            crate::intent::Intent::Frame(FrameEffect::Draw(DrawEffect::PluginDisplay(
                PluginDisplayRequest::Popup { params, .. },
            ))) => Some((params.instance_id, params.raw_input)),
            _ => None,
        })
        .collect()
}

/// Given 키 주인인 plugin 팝업 P 와 키 주인이 아닌 plugin 팝업 Q 에 각각 보류 편집 요청이 줄 서 있을 때, When
/// `draw_plugin_popups`로 그리면, Then 각 요청은 자기 인스턴스의 `Popup` 요청 입력 끝에 붙고 다른 인스턴스에는 붙지
/// 않는다. 붙이기는 키 주인과 관계없이 인스턴스로 고른다.
#[test]
fn a_held_edit_request_rides_on_its_own_popup_request() {
    use tasty_plugin_protocol::protocol::RawInputEventWire as W;
    let (mut state, mut session, mut mgr) = plugin_only_scene();
    mgr.insert_popup_instance_for_test(
        OTHER_POPUP,
        plugin_popup(
            tasty_host_plugin::next_popup_z_seq(),
            tasty_plugin_protocol::popup_decl::PopupEscape::Close,
        ),
    );
    let engine = session.borrow_mut();
    focus_p(&mut state);
    let mine = W::PasteText {
        text: "for P".into(),
    };
    state.plugin_popup_edit_requests.push((POPUP, mine.clone()));
    state.plugin_popup_edit_requests.push((OTHER_POPUP, W::Cut));
    render_frame(&mut state, &engine.read(), &mgr, false, &|state| {
        assert_eq!(
            state.popup_key_owner(),
            plugin_owner(),
            "전제: live 키 주인은 P"
        );
    });
    let sent = sent_inputs(&mut state);
    let input_of = |id: u64| {
        sent.iter()
            .rev()
            .find(|(iid, _)| *iid == id)
            .map(|(_, input)| input.clone())
    };
    let p = input_of(POPUP).expect("P 의 Popup 요청이 없다");
    assert!(p.focused, "전제: P 가 키 포커스를 가진다");
    assert_eq!(
        p.events.last(),
        Some(&mine),
        "P 의 보류 편집 요청이 P 요청 입력 끝에 붙는다"
    );
    assert!(
        !p.events.contains(&W::Cut),
        "Q 의 편집 요청이 P 요청에 붙었다"
    );
    let q = input_of(OTHER_POPUP).expect("Q 의 Popup 요청이 없다");
    assert!(!q.focused, "전제: Q 는 키 포커스가 없다");
    assert_eq!(
        q.events.last(),
        Some(&W::Cut),
        "키 주인이 아닌 Q 의 편집 요청도 Q 요청 입력 끝에 붙는다"
    );
    assert!(
        !q.events.contains(&mine),
        "P 의 편집 요청이 Q 요청에 붙었다"
    );
    assert!(
        state.plugin_popup_edit_requests.is_empty(),
        "붙인 요청은 대기열에서 빠진다"
    );
}

/// 캐럿을 든 P 의 최근 프레임.
fn frame_with_caret(
    ime: &tasty_plugin_protocol::ImeCursorWire,
) -> tasty_host_plugin::EguiMeshFrame {
    tasty_host_plugin::EguiMeshFrame {
        plugin_id: PLUGIN.into(),
        buffer_id: tasty_plugin_protocol::SharedBufferId(1),
        generation: 1,
        frame_seq: 1,
        full_textures: true,
        byte_len: 0,
        ime_cursor: Some(*ime),
        banner_body_cut: None,
    }
}

/// P 에 캐럿 든 프레임을 두고 한 프레임 그린 뒤 (저장된 캐럿 영역, P 의 창 영역)을 돌려준다. `focus`면 P 를
/// 포커스 노드로 한다.
fn caret_after_render(
    focus: bool,
    ime: &tasty_plugin_protocol::ImeCursorWire,
) -> (
    Option<crate::model::PhysicalRect>,
    Option<crate::model::PhysicalRect>,
) {
    let (mut state, mut session, mut mgr) = plugin_only_scene();
    mgr.insert_popup_mesh_frame_for_test(POPUP, frame_with_caret(ime));
    let engine = session.borrow_mut();
    if focus {
        focus_p(&mut state);
    }
    render_frame(&mut state, &engine.read(), &mgr, !focus, &|state| {
        assert_eq!(
            state.popup_key_owner().is_some(),
            focus,
            "전제: live 키 주인이 P 인지"
        );
    });
    let region = state
        .plugin_mesh_popup_regions
        .iter()
        .find(|(id, _)| *id == POPUP)
        .map(|(_, r)| *r);
    (state.plugin_popup_ime_cursor_area, region)
}

/// 비교용 네 변(물리 px).
fn edges(r: crate::model::PhysicalRect) -> (f32, f32, f32, f32) {
    (r.x.value(), r.y.value(), r.width.value(), r.height.value())
}

/// Given P 의 최근 프레임이 IME 캐럿을 들고 있을 때, When `draw_plugin_popups`로 그리면, Then 키 포커스를 가진 P 는
/// 그 캐럿을 P 의 창 영역 기준으로 `plugin_popup_ime_cursor_area`에 저장하고, 키 포커스가 없는 P 는 저장하지 않는다.
#[test]
fn only_the_focused_plugin_popup_stores_its_ime_caret() {
    use tasty_plugin_protocol::protocol::RectWire;
    let ime = tasty_plugin_protocol::ImeCursorWire {
        rect: RectWire {
            x: 4.0,
            y: 6.0,
            width: 120.0,
            height: 20.0,
        },
        cursor_rect: RectWire {
            x: 30.0,
            y: 8.0,
            width: 1.0,
            height: 16.0,
        },
    };
    let (caret, region) = caret_after_render(true, &ime);
    let region = region.expect("P 를 그렸다");
    assert_eq!(
        caret.map(edges),
        Some(edges(crate::plugin_bridge::mesh_ime_cursor_area(
            region, &ime, 1.0
        ))),
        "포커스 P 의 캐럿이 창 좌표로 저장된다"
    );

    let (caret, region) = caret_after_render(false, &ime);
    assert!(region.is_some(), "포커스 없는 P 도 그렸다");
    assert_eq!(
        caret.map(edges),
        None,
        "포커스 없는 P 는 캐럿을 저장하지 않는다"
    );
}
