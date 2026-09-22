//! popup 입력 forward 자리가 사용자 활성화를 **기록하고 걷는지** 고정한다.
//!
//! `file_handler.dispatch` 입구 시험은 활성화 기록을 직접 채워 시작하고, `is_user_activation`
//! 시험은 판정 함수만 본다. 둘 다 그 판정을 렌더 루프가 실제로 부르는지는 안 본다 — 루프가
//! 기록을 안 세우면 사용자가 popup 에서 연 파일이 에이전트로 떨어지고, 닫힌 popup 의 기록을 안
//! 걷으면 plugin 이 사람이 더는 보지 않는 popup 으로 사용자 행동을 주장한다. 그래서 여기서는
//! `draw_plugin_popups` 를 egui frame 으로 돌려 `AppState::plugin_popup_user_activated` 를 본다
//! (ADR-0526).
//!
//! 이 모듈은 gui feature 뒤라 헤드리스 조합(`--no-default-features`)에는 없고, 자동 실행은
//! 기본 조합의 `cargo test --workspace --lib --bins` 스텝이 맡는다. 손으로 돌리는 명령:
//! `cargo test -p tasty --lib --locked -- popup_render::activation_tests`.

use std::sync::Arc;

use egui::{Context, Event, Pos2, Rect, Vec2};
use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupSizeHint};

use super::draw_plugin_popups;
use crate::plugin::PluginManager;
use crate::plugin::manifest::PopupAnchor;

const PLUGIN: &str = "com.example.popup";
const POPUP: u64 = 7;

fn manager() -> PluginManager {
    // `PluginManager::new` 는 그 크레이트 안 전용(`#[cfg(test)]`)이라 공개 생성자를 쓴다.
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
            },
            z_seq: 1,
            scope_surface: None,
        },
    );
    mgr
}

/// 한 frame 을 돌린다. 돌려주는 값은 그 frame 에 놓인 popup 셸 rect 다(없으면 `None`).
fn frame(
    ctx: &Context,
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
    mgr: &PluginManager,
    events: Vec<Event>,
) -> Option<Rect> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        events,
        ..Default::default()
    };
    let _full_output = ctx.run(input, |ctx| {
        draw_plugin_popups(ctx, state, engine, Some(mgr), None);
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

/// 포인터가 지나가기만 하면 기록이 안 서고, popup 안을 누르면 그 popup 의 소유 plugin 으로 선다.
#[test]
fn a_press_inside_the_popup_records_it_and_a_hover_does_not() {
    let (mut state, mut engine) = crate::state::tests::test_state();
    let mgr = manager_with_popup();
    let ctx = Context::default();

    let shell = frame(&ctx, &mut state, &mut engine, &mgr, Vec::new()).expect("popup 이 놓인다");
    let inside = shell.center();

    frame(
        &ctx,
        &mut state,
        &mut engine,
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
        &mut engine,
        &mgr,
        vec![Event::PointerMoved(inside), press(inside)],
    );
    assert_eq!(
        state
            .plugin_popup_user_activated
            .get(&POPUP)
            .map(String::as_str),
        Some(PLUGIN),
        "popup 안의 누름이 기록되지 않았다"
    );
}

/// 닫힌 popup 의 기록은 다음 frame 에 걷힌다 — 남으면 사람이 안 보는 popup 이 근거가 된다.
#[test]
fn a_closed_popup_loses_its_record() {
    let (mut state, mut engine) = crate::state::tests::test_state();
    let mgr = manager_with_popup();
    let ctx = Context::default();

    let shell = frame(&ctx, &mut state, &mut engine, &mgr, Vec::new()).expect("popup 이 놓인다");
    let inside = shell.center();
    frame(
        &ctx,
        &mut state,
        &mut engine,
        &mgr,
        vec![Event::PointerMoved(inside), press(inside)],
    );
    assert!(state.plugin_popup_user_activated.contains_key(&POPUP));

    // 닫힘은 인스턴스가 매니저에서 사라진 상태로 표현한다. 실제 닫기(`close_popup_instance`)는
    // 앱의 drain 한 곳에서만 부르게 가드가 막고 있고, 렌더 루프가 보는 것은 그 결과뿐이다.
    let mgr = manager();
    frame(&ctx, &mut state, &mut engine, &mgr, Vec::new());
    assert!(
        state.plugin_popup_user_activated.is_empty(),
        "닫힌 popup 의 기록이 남았다: {:?}",
        state.plugin_popup_user_activated
    );
}
