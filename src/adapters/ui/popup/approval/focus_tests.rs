//! 에이전트 요청으로 연 승인 팝업이 키보드 포커스를 가져가지 않고, 사용자가 누른 뒤에만
//! 숫자 키로 답하는지 egui 프레임과 Intent 처리로 확인한다. OS 창은 만들지 않는다.

use super::*;
use crate::intent::{Intent, UiIntent};
use crate::model::{PhysicalPx, PhysicalRect};
use tasty_approval::{ApprovalChoice, ApprovalId, ApprovalRequest, ApprovalState, Requester};

fn record(id: &str) -> ApprovalRecord {
    ApprovalRecord {
        request: ApprovalRequest {
            id: ApprovalId(id.into()),
            requester: Requester::Plugin { id: "test".into() },
            workspace_id: None,
            surface_id: None,
            title: "Test approval".into(),
            body: None,
            choices: vec![ApprovalChoice::approve(), ApprovalChoice::deny()],
            default_choice: None,
            timeout_ms: None,
            severity: Severity::Info,
            created_at: 1,
            metadata: serde_json::Value::Null,
            host_elevation: None,
        },
        state: ApprovalState::Pending,
        history: vec![],
    }
}

fn screen() -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1920.0),
        height: PhysicalPx(1080.0),
    }
}

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events,
        ..Default::default()
    }
}

fn key(k: egui::Key) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn press(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }
}

fn frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) {
    drop(ctx.run(input, |ctx| {
        crate::adapters::ui::draw_popups(ctx, state, &engine.read(), &[], screen(), 1.0);
    }));
}

/// 팝업 Intent만 처리하고 나머지(응답·알림)는 돌려준다.
fn drain_popup_intents(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> Vec<crate::intent::DispatchedIntent> {
    let mut rest = Vec::new();
    for d in state.take_pending_intents() {
        if matches!(
            d.body,
            Intent::Ui(UiIntent::OpenPopup(_) | UiIntent::ClosePopup { .. })
        ) {
            crate::intent::popup::handle(state, engine, &d);
        } else {
            rest.push(d);
        }
    }
    rest
}

fn responded(intents: &[crate::intent::DispatchedIntent]) -> Option<String> {
    intents.iter().find_map(|d| match &d.body {
        Intent::RespondApproval { choice, .. } => Some(choice.clone()),
        _ => None,
    })
}

#[test]
fn queued_approval_opens_without_taking_keyboard_focus() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let core = crate::core::CoreState::new_base();
    enqueue_approval(&mut state, &core, &record("req-1"));
    drain_popup_intents(&mut state, &mut engine);

    assert!(state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(!state.popups.is_focused(APPROVAL_POPUP_ID));
    assert!(
        !state.popups.has_focused(),
        "terminal keys must keep flowing"
    );
    assert!(
        !state.has_egui_overlay_open(),
        "a passive popup must not hide every native webview"
    );
    // 겹치는 WebView만 키보드 입력이 없을 때 숨기도록 영역을 넘긴다.
    let ctx = egui::Context::default();
    frame(&ctx, raw(vec![]), &mut state, &mut engine);
    let rects = state.popups.visible_passive_rects();
    assert_eq!(rects.len(), 1);
    assert_eq!(
        Some(rects[0]),
        state
            .popups
            .open_geometry(APPROVAL_POPUP_ID)
            .map(|(_, r)| r)
    );
}

#[test]
fn digit_on_an_unfocused_popup_does_not_answer() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let core = crate::core::CoreState::new_base();
    enqueue_approval(&mut state, &core, &record("req-1"));
    drain_popup_intents(&mut state, &mut engine);
    let ctx = egui::Context::default();
    frame(&ctx, raw(vec![]), &mut state, &mut engine);

    // explorer·dag_graph처럼 egui 입력을 받는 surface에 포커스가 있으면 숫자 키가 egui에 온다.
    frame(
        &ctx,
        raw(vec![key(egui::Key::Num1)]),
        &mut state,
        &mut engine,
    );

    assert_eq!(responded(&state.take_pending_intents()), None);
    assert_eq!(state.approval_inbox.len(), 1);
}

#[test]
fn clicking_the_popup_gives_focus_and_then_digit_answers() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let core = crate::core::CoreState::new_base();
    enqueue_approval(&mut state, &core, &record("req-1"));
    drain_popup_intents(&mut state, &mut engine);
    let ctx = egui::Context::default();
    frame(&ctx, raw(vec![]), &mut state, &mut engine);
    let (_, rect) = state
        .popups
        .open_geometry(APPROVAL_POPUP_ID)
        .expect("approval popup is open");

    // 제목 표시줄은 코멘트 칸이 아니므로 입력칸 포커스 없이 팝업만 포커스를 받는다.
    let title = egui::pos2(rect.center().x, rect.min.y + 4.0);
    frame(&ctx, raw(vec![press(title)]), &mut state, &mut engine);
    assert!(state.popups.is_focused(APPROVAL_POPUP_ID));
    assert!(state.has_egui_overlay_open());
    assert!(state.popups.visible_passive_rects().is_empty());
    drain_popup_intents(&mut state, &mut engine);

    frame(
        &ctx,
        raw(vec![key(egui::Key::Num2)]),
        &mut state,
        &mut engine,
    );
    assert_eq!(
        responded(&state.take_pending_intents()).as_deref(),
        Some("deny")
    );
}

#[test]
fn next_request_reopens_without_inheriting_the_users_focus() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let core = crate::core::CoreState::new_base();
    enqueue_approval(&mut state, &core, &record("req-1"));
    enqueue_approval(&mut state, &core, &record("req-2"));
    drain_popup_intents(&mut state, &mut engine);
    // 사용자가 첫 요청에 답하려고 팝업에 포커스를 준 상태.
    state.popups.set_focused(APPROVAL_POPUP_ID, true);
    let mut answered = record("req-1");
    answered.state = ApprovalState::Cancelled { at: 2 };
    state.approval_inbox.update(answered);

    let ctx = egui::Context::default();
    frame(&ctx, raw(vec![]), &mut state, &mut engine);
    drain_popup_intents(&mut state, &mut engine);

    assert_eq!(state.approval_inbox.len(), 1);
    assert!(state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(!state.popups.is_focused(APPROVAL_POPUP_ID));
    assert!(!state.popups.has_focused());
}

/// Given 코멘트를 쓰던 승인 req-1이 보임, When 두 번째 승인 req-2가 들어와 팝업을 다시 열면, Then 열린
/// 인스턴스는 req-1과 그 코멘트를 그대로 보이고 req-2는 inbox에서 차례를 기다린다.
#[test]
fn a_second_approval_keeps_the_shown_one_and_its_comment() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let core = crate::core::CoreState::new_base();
    enqueue_approval(&mut state, &core, &record("req-1"));
    drain_popup_intents(&mut state, &mut engine);
    let iid = state
        .popups
        .instance_for(
            APPROVAL_POPUP_ID,
            &crate::adapters::ui::popup::PopupScope::Window,
        )
        .expect("approval popup is open");
    if let Some(p) = state.popups.instance_mut(iid)
        && let PopupPayload::Approval(shown) = &mut p.payload
    {
        shown.comment = "draft".to_string();
    }

    enqueue_approval(&mut state, &core, &record("req-2"));
    drain_popup_intents(&mut state, &mut engine);

    assert_eq!(state.popups.instances_of(APPROVAL_POPUP_ID).count(), 1);
    let shown = state
        .popups
        .topmost_payload(APPROVAL_POPUP_ID)
        .and_then(ShownApproval::of)
        .expect("the instance keeps its approval");
    assert_eq!(shown.id, ApprovalId("req-1".into()));
    assert_eq!(shown.comment, "draft");
    assert_eq!(state.approval_inbox.len(), 2);
}
