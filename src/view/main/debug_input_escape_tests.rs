//! debug 키 주입의 수식키 없는 Esc 는 실제 키처럼 창의 Esc 판정을 거친다.

use super::{PushedKey, inject_key};
use crate::adapters::ui::popup::body_keys::is_escape_press;
use crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID;
use crate::adapters::ui::popup::defs;
use crate::adapters::ui::popup::window_input_test_support::{WindowInput, layer_frame};

/// 키 주인 명령 팔레트 하나를 창 범위에 열고 그린 장면.
fn palette_scene() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
    egui::Context,
) {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(
                COMMAND_PALETTE_POPUP_ID,
                crate::model::popup_kind::PopupScope::Window,
            )
            .centered()
            .focused(),
            true,
        )
        .expect("명령 팔레트");
    let ctx = egui::Context::default();
    {
        let engine = session.borrow_mut();
        layer_frame(&ctx, &mut state, &engine.read(), Vec::new());
    }
    assert!(state.popups.is_open(COMMAND_PALETTE_POPUP_ID));
    (state, session, ctx)
}

/// T-F6-1: Given 키 주인 명령 팔레트, When debug 주입으로 수식키 없는 Esc 를 넣고 다음 패스를 그리면, Then 창이 그
/// 누름을 처리해 egui 입력에서 빼고 팔레트가 닫힌다.
#[test]
fn an_injected_escape_closes_the_focused_palette() {
    let (mut state, mut session, ctx) = palette_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let mut input = WindowInput::default();
    assert_eq!(
        inject_key(&mut state, &read, &mut input, "Escape", true, None),
        Some(PushedKey::Key)
    );
    let events = std::mem::take(input.queue_mut());
    assert!(
        !events.iter().any(is_escape_press),
        "창이 처리한 누름은 egui 입력에서 빠진다: {events:?}"
    );
    layer_frame(&ctx, &mut state, &read, events);
    assert!(
        !state.popups.is_open(COMMAND_PALETTE_POPUP_ID),
        "주입한 Esc 로 팔레트가 닫힌다"
    );
}

/// T-F6-2: Given 키 주인 명령 팔레트, When debug 주입으로 Shift+Esc 를 넣으면, Then 실제 키처럼 창의 Esc 판정 대상이
/// 아니라 그 누름이 egui 입력에 남고 팔레트는 열린 채다.
#[test]
fn an_injected_shift_escape_stays_in_the_egui_input() {
    let (mut state, mut session, ctx) = palette_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let mut input = WindowInput::default();
    assert_eq!(
        inject_key(
            &mut state,
            &read,
            &mut input,
            "Escape",
            true,
            Some(egui::Modifiers::SHIFT)
        ),
        Some(PushedKey::Key)
    );
    let events = std::mem::take(input.queue_mut());
    assert!(events.iter().any(is_escape_press), "{events:?}");
    layer_frame(&ctx, &mut state, &read, events);
    assert!(state.popups.is_open(COMMAND_PALETTE_POPUP_ID));
}

/// T-G3-1: Given 키 주인 명령 팔레트와 창이 실물 Win·Super 를 누른 것으로 보는 상태(비-macOS), When debug 주입으로
/// 수식키 없는 Esc 를 넣으면, Then 실제 Super+Esc 처럼 창의 Esc 판정 대상이 아니라 그 누름이 egui 입력에 남고 팔레트는
/// 열린 채다. egui 수식키는 비-macOS 의 Win·Super 를 담지 않으므로 창은 그 상태를 따로 읽는다.
#[cfg(not(target_os = "macos"))]
#[test]
fn an_injected_escape_with_the_real_super_held_is_not_a_bare_escape() {
    let (mut state, mut session, ctx) = palette_scene();
    let engine = session.borrow_mut();
    let read = engine.read();
    let mut input = WindowInput::default();
    input.hold_super(true);
    assert_eq!(
        inject_key(&mut state, &read, &mut input, "Escape", true, None),
        Some(PushedKey::Key)
    );
    let events = std::mem::take(input.queue_mut());
    assert!(
        events.iter().any(is_escape_press),
        "Super 를 쥔 Esc 는 창이 처리하지 않는다: {events:?}"
    );
    layer_frame(&ctx, &mut state, &read, events);
    assert!(
        state.popups.is_open(COMMAND_PALETTE_POPUP_ID),
        "Super+Esc 로는 팔레트가 닫히지 않는다"
    );
}

/// M-E0b-6 F: Given 팝업이 없는 터미널 포커스와 수식키 없는 Esc 에 묶은 host 단축키(`toggle_sidebar`), When debug
/// 주입으로 수식키 없는 Esc 를 넣으면, Then 그 Esc 의 수신자는 경로의 단축키 단계지만 debug 입구는 egui 전용이라 단계
/// 11 적용만 거치고 단축키·끝 목적지 단계는 돌지 않는다. 대기 요청이 없고 그 누름은 egui 입력에 남는다.
#[test]
fn a_debug_escape_runs_no_host_shortcut() {
    crate::i18n::init("en");
    let (mut state, mut session) = crate::state::tests::test_state();
    {
        let engine = session.borrow_mut();
        assert!(
            engine
                .runtime
                .settings
                .keybindings
                .set_field("toggle_sidebar", "escape")
        );
    }
    let engine = session.borrow_mut();
    let read = engine.read();
    drop(state.take_pending_intents());
    let egui_surface =
        crate::adapters::ui::input::key_nodes::focused_surface_takes_egui(&state, &read);
    let decision =
        crate::adapters::ui::input::key_nodes::bare_escape_decision(&state, &read, egui_surface);
    assert!(
        matches!(
            decision,
            crate::adapters::ui::input::key_nodes::KeyDecision::Path(routed)
                if matches!(
                    routed.receiver,
                    crate::adapters::ui::input::key_route::KeyReceiver::Shortcut { .. }
                )
        ),
        "Esc 의 수신자는 단축키 단계다: {decision:?}"
    );
    let mut input = WindowInput::default();
    assert_eq!(
        inject_key(&mut state, &read, &mut input, "Escape", true, None),
        Some(PushedKey::Key)
    );
    assert!(
        state.take_pending_intents().is_empty(),
        "debug 입구는 host 단축키를 돌리지 않는다"
    );
    assert!(
        input.queue_mut().iter().any(is_escape_press),
        "그 누름은 egui 입력에 남는다"
    );
}
