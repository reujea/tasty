//! 업로드 실패 팝업의 Retry·Dismiss 처리.

use super::*;
use crate::model::popup_kind::PopupScope;

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

/// 살아 있는 surface의 Retry는 업로드를 다시 넣고 마지막 항목이면 팝업을 닫는다.
#[test]
fn retry_for_a_live_surface_queues_the_upload() {
    let (mut state, mut session, sid) = fixture();
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Surface(sid), Some(sid)));
    let engine = session.borrow_mut();
    let action = retry_head_error(&mut state, &engine.read());
    assert!(matches!(action, PopupAction::Close));
    assert!(state.dialogs.transfer_error.is_empty());
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

/// S1 실패를 닫거나 다시 시도한 뒤 대기 중인 W2 실패는 W2 범위로 보인다.
fn the_next_failure_moves_to_its_scope(
    press: impl FnOnce(&mut crate::state::RequestContext, &EngineRead<'_>) -> PopupAction,
) {
    let (mut state, mut session, sid) = fixture();
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Surface(sid), Some(sid)));
    state
        .dialogs
        .transfer_error
        .push_back(failure(PopupScope::Workspace(8), None));
    state
        .popups
        .open_centered_focused(TRANSFER_ERROR_POPUP_ID, PopupScope::Surface(sid));
    let action = {
        let engine = session.borrow_mut();
        press(&mut state, &engine.read())
    };
    assert!(matches!(action, PopupAction::None));
    crate::intent::popup::apply_pending(&mut state, &mut session);
    assert_eq!(state.dialogs.transfer_error.len(), 1);
    assert_eq!(
        state.popups.open_scope(TRANSFER_ERROR_POPUP_ID),
        Some(&PopupScope::Workspace(8))
    );
}

#[test]
fn dismiss_shows_the_next_failure_in_its_own_scope() {
    the_next_failure_moves_to_its_scope(|state, _| dismiss_head_error(state));
}

#[test]
fn retry_shows_the_next_failure_in_its_own_scope() {
    the_next_failure_moves_to_its_scope(retry_head_error);
}

/// 다음 실패의 범위가 같으면 열린 팝업이 그대로 다음 항목을 그린다.
#[test]
fn the_next_failure_in_the_same_scope_keeps_the_popup() {
    let (mut state, mut session, sid) = fixture();
    for _ in 0..2 {
        state
            .dialogs
            .transfer_error
            .push_back(failure(PopupScope::Surface(sid), None));
    }
    state
        .popups
        .open_centered_focused(TRANSFER_ERROR_POPUP_ID, PopupScope::Surface(sid));
    let before = state.popups.max_open_z_seq();
    assert!(matches!(dismiss_head_error(&mut state), PopupAction::None));
    crate::intent::popup::apply_pending(&mut state, &mut session);
    assert_eq!(
        state.popups.open_scope(TRANSFER_ERROR_POPUP_ID),
        Some(&PopupScope::Surface(sid))
    );
    assert_eq!(state.popups.max_open_z_seq(), before);
}
