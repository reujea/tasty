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
