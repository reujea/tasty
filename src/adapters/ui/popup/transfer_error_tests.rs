//! 업로드 실패 팝업의 Retry·Dismiss 처리.

use super::*;
use crate::model::popup_kind::PopupScope;

const GONE_SURFACE: u32 = 9_999;

fn failure(scope: PopupScope, retry_surface: Option<u32>) -> TransferError {
    TransferError {
        scope,
        name: "a.png".into(),
        reason: "boom".into(),
        retry: retry_surface.map(|surface_id| crate::core::PendingImageUpload {
            origin_view: std::sync::Weak::new(),
            mirror_ws_id: 8,
            surface_id,
            bracketed: false,
            file_name: "a.png".into(),
            png_bytes: vec![1, 2, 3],
        }),
        kind: TransferErrorKind::Upload,
    }
}

fn fixture() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
) {
    let (state, mut session) = crate::state::tests::test_state();
    let sid = state
        .focused_surface_id(&session.borrow_mut().read())
        .expect("surface");
    (state, session, sid)
}

/// 그린 Retry 버튼이 누를 수 있는지. 버튼이 없으면 None.
fn drawn_retry_clickable(error: &TransferError, engine: &EngineRead<'_>) -> Option<bool> {
    let ctx = egui::Context::default();
    let th = theme::theme();
    let mut drawn = None;
    let _output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            drawn = draw_error_footer(ui, &th, retry_button_state(error, engine))
                .retry_response
                .map(|r| r.sense.senses_click());
        });
    });
    drawn
}

/// 업로드한 surface가 닫혀 workspace에서 알린 실패는 Retry를 비활성으로 그린다.
/// 업로드한 surface가 살아 있는 실패는 활성, 재시도 데이터가 없는 실패는 버튼이 없다.
#[test]
fn a_failure_whose_surface_is_gone_draws_retry_disabled() {
    let (_state, mut session, sid) = fixture();
    let engine = session.borrow_mut();
    let read = engine.read();
    let gone = failure(PopupScope::Workspace(8), Some(GONE_SURFACE));
    let live = failure(PopupScope::Surface(sid), Some(sid));
    let rejected = failure(PopupScope::Surface(sid), None);
    assert_eq!(drawn_retry_clickable(&gone, &read), Some(false));
    assert_eq!(drawn_retry_clickable(&live, &read), Some(true));
    assert_eq!(drawn_retry_clickable(&rejected, &read), None);
}

/// 결속할 surface가 없는 Retry는 항목과 재시도 데이터를 그대로 두고 아무것도 실행하지 않는다.
#[test]
fn retry_for_a_gone_surface_keeps_the_failure() {
    let (mut state, mut session, _sid) = fixture();
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Workspace(8), Some(GONE_SURFACE)).into());
    let engine = session.borrow_mut();
    let action = retry_head_error(&mut state, &engine.read());
    assert!(matches!(action, PopupAction::None));
    assert_eq!(state.dialogs.transfer_error.len(), 1);
    assert!(state.dialogs.transfer_error[0].retry.is_some());
    assert!(state.take_pending_intents().is_empty());
}

/// 살아 있는 surface의 Retry는 업로드를 다시 넣고 팝업을 닫는다. 항목은 닫기 훅이 꺼낸다.
#[test]
fn retry_for_a_live_surface_queues_the_upload() {
    let (mut state, mut session, sid) = fixture();
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Surface(sid), Some(sid)).into());
    let engine = session.borrow_mut();
    let action = retry_head_error(&mut state, &engine.read());
    assert!(matches!(action, PopupAction::Close));
    assert!(state.dialogs.transfer_error[0].retry.is_none());
    let intents = state.take_pending_intents();
    assert!(
        matches!(
            &intents[..],
            [crate::intent::DispatchedIntent {
                body: crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ImageUpload { .. }
                ),
                ..
            }]
        ),
        "{intents:?}"
    );
}

/// 버튼이 돌려준 닫기를 관리자에 반영하고 닫기 훅을 실행한다.
fn close_with_hook(
    state: &mut crate::state::RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    iid: crate::adapters::ui::popup::PopupInstanceId,
) {
    state
        .popups
        .close_instance(iid, crate::adapters::ui::popup::PopupCloseReason::Content);
    let ctx = egui::Context::default();
    let engine = session.borrow_mut();
    for closed in state.popups.take_closed_queue() {
        on_close_transfer_error(&ctx, state, &engine.read(), &closed);
    }
}

/// S1 실패를 닫거나 다시 시도하면 팝업이 닫히고, 대기 중인 다음 실패가 자기 범위로 열린다.
fn the_next_failure_moves_to_its_scope(
    next_scope: PopupScope,
    press: impl FnOnce(&mut crate::state::RequestContext, &EngineRead<'_>) -> PopupAction,
) {
    let (mut state, mut session, sid) = fixture();
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Surface(sid), Some(sid)).into());
    state
        .dialogs
        .transfer_error
        .push_back(failure(next_scope.clone(), None).into());
    let iid = state
        .popups
        .open_centered_focused(TRANSFER_ERROR_POPUP_ID, PopupScope::Surface(sid))
        .unwrap();
    let action = {
        let engine = session.borrow_mut();
        press(&mut state, &engine.read())
    };
    assert!(matches!(action, PopupAction::Close));
    close_with_hook(&mut state, &mut session, iid);
    assert_eq!(state.dialogs.transfer_error.len(), 1);
    assert_eq!(
        state.popups.open_scope(TRANSFER_ERROR_POPUP_ID),
        Some(&next_scope)
    );
}

/// 본문의 Enter(Dismiss)를 그린 프레임에 누른다.
fn dismiss(state: &mut crate::state::RequestContext, engine: &EngineRead<'_>) -> PopupAction {
    let mut action = PopupAction::None;
    let ctx = egui::Context::default();
    let mut raw = egui::RawInput::default();
    raw.events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let _output = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut cx = crate::adapters::ui::popup::PopupCx::test(TRANSFER_ERROR_POPUP_ID);
            cx.focused = true;
            action = draw_transfer_error(ui, state, engine, &mut cx);
        });
    });
    action
}

#[test]
fn dismiss_shows_the_next_failure_in_its_own_scope() {
    the_next_failure_moves_to_its_scope(PopupScope::Workspace(8), dismiss);
}

#[test]
fn retry_shows_the_next_failure_in_its_own_scope() {
    the_next_failure_moves_to_its_scope(PopupScope::Workspace(8), retry_head_error);
}
