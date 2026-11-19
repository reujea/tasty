//! egui 프레임을 실행해 사용자 활성화 기록이 생기고 닫힌 팝업의 기록이 지워지는지 검사한다.
//! 등록·판정 함수만 직접 검사하는 시험과 달리 draw_plugin_popups를 거친다.

use std::sync::Arc;

use egui::{Context, Event, Pos2, Rect, Vec2};
use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupSizeHint};

use super::draw_plugin_popups;
use crate::plugin::PluginManager;
use crate::plugin::manifest::PopupAnchor;

const PLUGIN: &str = "com.example.popup";
const POPUP: u64 = 7;

fn manager() -> PluginManager {
    PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    )
}

fn manager_with_popup() -> PluginManager {
    let mut mgr = manager();
    mgr.insert_popup_instance_for_test(
        POPUP,
        tasty_host_plugin::PopupInstance {
            plugin_id: PLUGIN.into(),
            popup_id: "file-open".into(),
            contribute: PopupContribute {
                id: "file-open".into(),
                trigger: tasty_plugin_manifest::PopupTrigger::Ipc,
                size_hint: Some(PopupSizeHint {
                    width: 400,
                    height: 200,
                }),
                anchor: PopupAnchor::ScreenCenter,
                scope: PopupScopeDecl::Window,
                dismiss_on_outside_click: true,
                rendering: PopupRendering::EguiMesh,
                layout: Default::default(),
            },
            z_seq: 1,
            scope_surface: None,
        },
    );
    mgr
}

fn frame(
    ctx: &Context,
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &PluginManager,
    events: Vec<Event>,
) -> Option<Rect> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        events,
        ..Default::default()
    };
    let _full_output = ctx.run(input, |ctx| {
        draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            None,
        );
    });
    state.plugin_popup_hittest.first().map(|o| o.rect)
}

fn press(pos: Pos2) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }
}

#[test]
fn a_press_inside_the_popup_records_it_and_a_hover_does_not() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    // 창 범위 팝업은 이 창에 결속해야 그려진다.
    state.plugin_window_popups.insert(POPUP);
    let engine = engine_session.borrow_mut();
    let mgr = manager_with_popup();
    let ctx = Context::default();

    let shell = frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new()).expect("popup 이 놓인다");
    let inside = shell.center();

    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(inside)],
    );
    assert!(
        state.plugin_popup_user_activated.is_empty(),
        "지나가는 포인터는 활성화가 아니다: {:?}",
        state.plugin_popup_user_activated
    );

    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(inside), press(inside)],
    );
    assert_eq!(
        state
            .plugin_popup_user_activated
            .get(&POPUP)
            .map(String::as_str),
        Some(PLUGIN),
        "팝업 내부의 버튼 누름이 활성화로 기록되지 않았다"
    );
}

#[test]
fn a_closed_popup_loses_its_record() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    // 창 범위 팝업은 이 창에 결속해야 그려진다.
    state.plugin_window_popups.insert(POPUP);
    let engine = engine_session.borrow_mut();
    let mgr = manager_with_popup();
    let ctx = Context::default();

    let shell = frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new()).expect("popup 이 놓인다");
    let inside = shell.center();
    frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![Event::PointerMoved(inside), press(inside)],
    );
    assert!(state.plugin_popup_user_activated.contains_key(&POPUP));

    // 실제 닫기 처리 대신 인스턴스를 지워 다음 렌더 프레임이 보는 상태를 만든다.
    let mgr = manager();
    frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    assert!(
        state.plugin_popup_user_activated.is_empty(),
        "닫힌 팝업의 활성화 기록이 남았다: {:?}",
        state.plugin_popup_user_activated
    );
}
