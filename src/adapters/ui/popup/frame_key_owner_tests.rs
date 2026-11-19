//! 한 프레임의 키 주인은 하나다. 창의 Esc 판정은 그리기보다 먼저 돌며 정한 대상을 그 프레임 키 주인으로 묶고,
//! host 본문과 plugin 팝업 렌더가 그 값을 쓴다. 장면: L 범위 피커 H(맨 위, 포커스), R 범위 plugin 팝업 P(가운데),
//! R 범위 피커 G(맨 아래, 포커스). 창의 한 프레임처럼 결속 범위 동기화 → host 팝업 층 → plugin 팝업 순서로 그린다.

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

fn plugin_popup(
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

/// G(R 피커) → P(R plugin 팝업) → H(L 피커) 순서로 열어 순번이 H > P > G 다. H 와 G 는 각자 범위에서 포커스를
/// 가진다.
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
    let consumed = state.window_escape(&mut events, ctx.input(|i| i.key_down(egui::Key::Escape)));
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
    assert!(
        !state.popups.instance(h).unwrap().focused,
        "창의 대상은 H 다"
    );
    assert!(!plugin_escaped(&state), "P 는 받지 않는다");
    assert!(
        is_open(&state, g) && state.popups.instance(g).unwrap().focused,
        "G 는 받지 않는다"
    );
}

/// Given 직전 렌더에 H 가 숨어 키 주인이 P 일 때, When H 의 범위가 다시 보이는 프레임에 Esc 가 오면, Then 창이 고른
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
    assert!(is_open(&state, h) && is_open(&state, g) && !plugin_escaped(&state));

    assert!(
        escape_through_the_window(&ctx, &mut state, &engine.read(), &mgr, &layout()),
        "plugin 키 주인 갈래: 창이 처리하고 그 Esc 를 P 의 렌더에 넘긴다"
    );
    assert!(plugin_escaped(&state), "창이 고른 P 가 받는다");
    assert!(
        is_open(&state, h) && state.popups.instance(h).unwrap().focused,
        "다시 보인 H 는 받지 않는다"
    );
    assert!(
        is_open(&state, g) && state.popups.instance(g).unwrap().focused,
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

/// Given 키 주인이 피커 H 일 때, When 창이 Esc 로 H 의 포커스를 풀고 그 Esc 를 H 본문에 넘기면, Then 넘긴 사건은
/// 원래 수식키를 지닌다. Ctrl+Esc 는 피커 취소(수식키 없는 Esc)와 맞지 않아 H 가 열린 채이고, 일반 Esc 는 H 를
/// 닫는다.
#[test]
fn the_escape_handed_to_the_owner_keeps_its_modifiers() {
    for (modifiers, cancels) in [
        (egui::Modifiers::CTRL, false),
        (egui::Modifiers::NONE, true),
    ] {
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
        let mut events = vec![escape_with(modifiers)];
        assert!(
            state.window_escape(&mut events, false),
            "창이 H 의 포커스를 푼다"
        );
        assert!(events.is_empty(), "창은 그 Esc 를 egui 입력에서 뺀다");
        frame(&ctx, &mut state, &engine.read(), &mgr, &layout(), events);
        assert_eq!(
            !is_open(&state, h),
            cancels,
            "H 본문이 받은 Esc 의 수식키: {modifiers:?}"
        );
    }
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
    // L 이 보이지 않아 H 가 숨고 P 가 키 주인이다. 처음 여는 찾기 바처럼 입력란에 egui 키 포커스를 준다.
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
    let before = draw(&mut state, &mut query, Vec::new(), false);
    assert!(before.field_focused, "찾기 입력란이 키 포커스를 가진다");
    assert!(!plugin_escaped(&state));

    let mut events = escape();
    assert!(
        state.window_escape(&mut events, false),
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
        state.window_escape(&mut events, ctx.input(|i| i.key_down(egui::Key::Escape))),
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
/// 누름이 오면(창이 두 번 처리해 넘긴다), Then 프레임마다 앱 처리 단계를 거친 뒤에도 P 는 닫는 요청 없이 열린 채
/// 다음 프레임을 받는다. P 는 첫 프레임에 새 누름, 둘째 프레임에 반복 누름을 받는다. 창을 거치지 않고 egui 가
/// 그대로 처리한 프레임별 결과와 같다.
#[test]
fn a_held_escape_reaches_the_plugin_owner_as_a_repeat() {
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
    assert!(dispatch_like_app(&mut state, &mut mgr).is_empty());
    state.take_pending_intents();

    let plain = Context::default();
    for (n, expected) in [vec![(true, false)], vec![(true, true)]]
        .into_iter()
        .enumerate()
    {
        let mut events = escape();
        assert!(
            state.window_escape(&mut events, ctx.input(|i| i.key_down(egui::Key::Escape))),
            "plugin 키 주인 갈래"
        );
        draw(&mut state, &mgr, events);
        assert_eq!(
            sent_escape_keys(&mut state),
            expected,
            "{n}번째 프레임에 P 가 받은 Esc(눌림, 반복)"
        );
        let closes = dispatch_like_app(&mut state, &mut mgr);
        assert!(
            closes.iter().all(|(id, _)| *id != POPUP),
            "Unfocus 선언 P 는 Esc 로 닫히지 않는다: {closes:?}"
        );
        assert!(
            mgr.popup_instances().any(|(id, _)| id == POPUP),
            "P 가 열린 채 다음 프레임을 받는다"
        );

        let mut plain_seen = Vec::new();
        drop(plain.run(
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
            plain_seen, expected,
            "{n}번째 프레임: 창을 거치지 않은 egui 결과와 같다"
        );
    }
}
