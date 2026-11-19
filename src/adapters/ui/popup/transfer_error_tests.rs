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

/// 실패 `errors`를 차례로 가진 인스턴스 데이터.
fn queue_of(errors: impl IntoIterator<Item = TransferError>) -> PopupPayload {
    PopupPayload::TransferError(TransferErrorQueue {
        items: errors.into_iter().map(crate::state::Queued::from).collect(),
    })
}

fn queue_items(
    payload: &PopupPayload,
) -> &std::collections::VecDeque<crate::state::Queued<TransferError>> {
    &TransferErrorQueue::of(payload)
        .expect("failure queue")
        .items
}

/// 결속할 surface가 없는 Retry는 항목과 재시도 데이터를 그대로 두고 아무것도 실행하지 않는다.
#[test]
fn retry_for_a_gone_surface_keeps_the_failure() {
    let (mut state, mut session, _sid) = fixture();
    let mut payload = queue_of([failure(PopupScope::Workspace(8), Some(GONE_SURFACE))]);
    let engine = session.borrow_mut();
    let action = retry_head_error(&mut state, &engine.read(), &mut payload);
    assert!(matches!(action, PopupAction::None));
    assert_eq!(queue_items(&payload).len(), 1);
    assert!(queue_items(&payload)[0].retry.is_some());
    assert!(state.take_pending_intents().is_empty());
}

/// 살아 있는 surface의 Retry는 업로드를 다시 넣고 팝업을 닫는다. 항목은 닫기 훅이 꺼낸다.
#[test]
fn retry_for_a_live_surface_queues_the_upload() {
    let (mut state, mut session, sid) = fixture();
    let mut payload = queue_of([failure(PopupScope::Surface(sid), Some(sid))]);
    let engine = session.borrow_mut();
    let action = retry_head_error(&mut state, &engine.read(), &mut payload);
    assert!(matches!(action, PopupAction::Close));
    assert!(queue_items(&payload)[0].retry.is_none());
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

type Press =
    fn(&mut crate::state::RequestContext, &EngineRead<'_>, &mut PopupPayload) -> PopupAction;

/// S1 실패 둘이 쌓인 팝업에서 앞 실패를 닫거나 다시 시도하면 팝업이 닫히고, 다음 실패가 같은 범위로 다시
/// 열린다. 다른 범위의 실패 팝업은 그대로다.
fn the_next_failure_stays_in_the_scope(press: Press) {
    let (mut state, mut session, sid) = fixture();
    let scope = PopupScope::Surface(sid);
    let other = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(TRANSFER_ERROR_POPUP_ID, PopupScope::Workspace(8))
                .centered()
                .with_payload(queue_of([failure(PopupScope::Workspace(8), None)])),
            true,
        )
        .expect("other scope opens");
    let iid = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(TRANSFER_ERROR_POPUP_ID, scope.clone())
                .centered()
                .focused()
                .with_payload(queue_of([
                    failure(scope.clone(), Some(sid)),
                    failure(scope.clone(), None),
                ])),
            true,
        )
        .expect("opens");
    assert_ne!(iid, other);
    let mut payload = std::mem::take(&mut state.popups.instance_mut(iid).expect("open").payload);
    let action = {
        let engine = session.borrow_mut();
        press(&mut state, &engine.read(), &mut payload)
    };
    assert!(matches!(action, PopupAction::Close));
    state.popups.instance_mut(iid).expect("open").payload = payload;
    close_with_hook(&mut state, &mut session, iid);

    let next = state
        .popups
        .instance_for(TRANSFER_ERROR_POPUP_ID, &scope)
        .expect("the next failure reopens in the same scope");
    let next = state.popups.instance(next).expect("open");
    assert_eq!(queue_items(&next.payload).len(), 1);
    assert!(queue_items(&next.payload)[0].retry.is_none());
    assert!(
        state.popups.instance(other).is_some(),
        "the other scope is untouched"
    );
}

/// 본문의 Enter(Dismiss)를 그린 프레임에 누른다.
fn dismiss(
    state: &mut crate::state::RequestContext,
    engine: &EngineRead<'_>,
    payload: &mut PopupPayload,
) -> PopupAction {
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
            let mut cx = crate::adapters::ui::popup::PopupCx {
                id: TRANSFER_ERROR_POPUP_ID,
                instance: crate::adapters::ui::popup::PopupInstanceId::NONE,
                focused: true,
                payload: &mut *payload,
            };
            action = draw_transfer_error(ui, state, engine, &mut cx);
        });
    });
    action
}

#[test]
fn dismiss_shows_the_next_failure_in_the_same_scope() {
    the_next_failure_stays_in_the_scope(dismiss);
}

#[test]
fn retry_shows_the_next_failure_in_the_same_scope() {
    the_next_failure_stays_in_the_scope(retry_head_error);
}
