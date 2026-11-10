//! 원격에서 닫기 확인은 Esc·Cancel 로만 닫히고 아무 일도 일어나지 않는다. 바깥 클릭은 무시한다.

use super::*;
use tasty_type_geometry::length::PhysicalPx;
use tasty_type_geometry::rect::PhysicalRect;

const ROW: u32 = 900_002;
const AT: egui::Pos2 = egui::pos2(500.0, 500.0);

fn screen() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    }
}

fn with(event: egui::Event) -> egui::RawInput {
    let mut raw = screen();
    raw.events.push(event);
    raw
}

fn key(key: egui::Key) -> egui::RawInput {
    with(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    })
}

fn frame(
    ctx: &egui::Context,
    raw: egui::RawInput,
    state: &mut crate::state::RequestContext,
    session: &crate::runtime::engine_session::EngineSession,
) {
    let rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1920.0),
        height: PhysicalPx(1080.0),
    };
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, &session.read(), &[], rect, 1.0);
    }));
}

/// 저장된 원격 행 하나와 그 행의 원격 닫기 확인이 열린 상태.
fn opened() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut row =
        crate::model::Workspace::new_with_terminal_marker(ROW, "row".into(), ROW, ROW, ROW);
    row.mirror = true;
    row.remote_row = Some(crate::model::RemoteBinding {
        profile: "gx10".into(),
        remote_workspace: 4,
    });
    session.borrow_mut().core.push_mirror_workspace(row);
    state.dialogs.pending_close_remote = Some(PendingCloseRemote {
        close: SavedRowClose {
            row: ROW,
            last: false,
        },
        intent: DomainIntent::CloseWorkspace { workspace_id: ROW },
        measured_h: None,
        focused: false,
    });
    state
        .popups
        .open_at_focused(CONFIRM_CLOSE_REMOTE_POPUP_ID, AT);
    (state, session)
}

fn nothing_sent(state: &mut crate::state::RequestContext) -> bool {
    state
        .take_pending_intents()
        .iter()
        .all(|intent| !is_confirmed(&intent.origin))
}

/// Given 열린 원격 닫기 확인, When 바깥을 누르면, Then 확인은 그대로 열려 있고 대기도 남는다.
#[test]
fn an_outside_click_does_not_close_the_close_remote_confirmation() {
    let (mut state, session) = opened();
    let ctx = egui::Context::default();
    frame(&ctx, screen(), &mut state, &session);
    frame(
        &ctx,
        with(egui::Event::PointerButton {
            pos: egui::pos2(20.0, 20.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }),
        &mut state,
        &session,
    );
    assert!(state.popups.is_open(CONFIRM_CLOSE_REMOTE_POPUP_ID));
    assert!(state.dialogs.pending_close_remote.is_some());
    assert!(nothing_sent(&mut state));
}

/// Given 열린 원격 닫기 확인, When Esc 를 누르면, Then 닫히고 담아 둔 닫기 요청은 보내지 않는다.
#[test]
fn escape_closes_the_close_remote_confirmation_and_sends_nothing() {
    let (mut state, session) = opened();
    let ctx = egui::Context::default();
    frame(&ctx, screen(), &mut state, &session);
    frame(&ctx, key(egui::Key::Escape), &mut state, &session);
    assert!(!state.popups.is_open(CONFIRM_CLOSE_REMOTE_POPUP_ID));
    assert!(state.dialogs.pending_close_remote.is_none());
    assert!(nothing_sent(&mut state));
}

/// Given 기본 포커스가 Cancel 인 원격 닫기 확인, When Enter 로 Cancel 을 누르면, Then 닫히고 아무 요청도
/// 보내지 않는다.
#[test]
fn cancel_closes_the_close_remote_confirmation_and_sends_nothing() {
    let (mut state, session) = opened();
    let ctx = egui::Context::default();
    frame(&ctx, screen(), &mut state, &session);
    frame(&ctx, screen(), &mut state, &session);
    assert!(state.dialogs.pending_close_remote.as_ref().unwrap().focused);
    frame(&ctx, key(egui::Key::Enter), &mut state, &session);
    assert!(!state.popups.is_open(CONFIRM_CLOSE_REMOTE_POPUP_ID));
    assert!(state.dialogs.pending_close_remote.is_none());
    assert!(nothing_sent(&mut state));
}
