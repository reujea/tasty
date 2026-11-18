//! 큐로 차례를 기다리는 팝업은 다음 항목을 그 항목을 넣은 요청의 출처로 연다.
//! 사용자 출처가 아닌 항목은 앞 항목이 사용자 출처였어도 포커스 없이 passive 로 열린다.

use super::{INFO_MODAL_ID, InfoModal, on_close_info_modal};
use crate::adapters::ui::popup::transfer::{
    TRANSFER_ERROR_POPUP_ID, TransferError, on_close_transfer_error,
};
use crate::adapters::ui::popup::{PopupPayload, PopupScope};
use crate::intent::{IntentOrigin, OpenRequest, UiIntent};
use crate::runtime::engine_access::EngineMut;
use crate::state::{Queued, RequestContext};

fn modal(body: &str) -> InfoModal {
    InfoModal {
        title: "Notice".to_string(),
        body: body.to_string(),
        extra_buttons: Vec::new(),
        emphasis: false,
    }
}

fn failure(name: &str) -> TransferError {
    TransferError {
        scope: PopupScope::Window,
        name: name.to_string(),
        reason: "refused".to_string(),
        retry: None,
        kind: crate::adapters::ui::popup::transfer::TransferErrorKind::Upload,
    }
}

fn origin(by_user: bool) -> IntentOrigin {
    if by_user {
        IntentOrigin::User {
            source: crate::core::origin::UserSource::Menu("test"),
        }
    } else {
        IntentOrigin::Agent {
            source: crate::intent::AgentSource::Ipc,
        }
    }
}

/// `id` 를 `by_user` 출처로 `payload` 와 함께 열고 닫은 뒤 닫기 훅을 실행한다.
fn open_then_close(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    id: &'static str,
    by_user: bool,
    payload: PopupPayload,
    hook: crate::adapters::ui::popup::PopupCloseHook,
) {
    let open = UiIntent::OpenPopup(
        OpenRequest::new(id, PopupScope::Window)
            .centered()
            .focused()
            .with_payload(payload),
    );
    let open = if by_user {
        open.from_user_menu("test")
    } else {
        open.from_agent_ipc()
    };
    crate::intent::popup::handle(state, engine, &open);
    assert!(state.popups.is_open(id));
    assert_eq!(state.popups.is_passive(id), !by_user);
    crate::intent::popup::handle(
        state,
        engine,
        &UiIntent::ClosePopup { id }.from_user_menu("test"),
    );
    let ctx = egui::Context::default();
    for closed in state.popups.take_closed_queue() {
        hook(&ctx, state, &engine.read(), &closed);
    }
}

fn info_payload(items: std::collections::VecDeque<Queued<InfoModal>>) -> PopupPayload {
    PopupPayload::InfoModal(super::InfoModalQueue {
        items,
        body_height: None,
    })
}

/// 열린 안내 인스턴스의 맨 앞 본문.
fn front_body(state: &RequestContext) -> Option<String> {
    state
        .popups
        .topmost_payload(INFO_MODAL_ID)
        .and_then(super::InfoModalQueue::of)
        .and_then(|q| q.items.front())
        .map(|m| m.body.clone())
}

/// 첫 안내를 그 출처로 열어 닫으면 둘째 안내가 자기 출처로 열린다.
fn info_modal_next_uses_its_own_origin(first_by_user: bool, next_by_user: bool) {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let items = [("a", first_by_user), ("b", next_by_user)]
        .into_iter()
        .map(|(body, by_user)| Queued {
            item: modal(body),
            origin: origin(by_user),
        })
        .collect();
    open_then_close(
        &mut state,
        &mut engine,
        INFO_MODAL_ID,
        first_by_user,
        info_payload(items),
        on_close_info_modal,
    );
    assert_eq!(front_body(&state).as_deref(), Some("b"));
    assert!(state.popups.is_open(INFO_MODAL_ID), "다음 안내가 열린다");
    assert_eq!(state.popups.is_passive(INFO_MODAL_ID), !next_by_user);
    assert_eq!(state.popups.is_focused(INFO_MODAL_ID), next_by_user);
}

/// Given 사용자 안내 뒤에 에이전트 안내가 큐에 있음, When 사용자 안내를 닫으면,
/// Then 에이전트 안내는 포커스 없이 passive 로 열린다.
#[test]
fn an_agent_info_modal_after_a_user_one_opens_without_focus() {
    info_modal_next_uses_its_own_origin(true, false);
}

/// Given 에이전트 안내 뒤에 사용자 안내가 큐에 있음, When 에이전트 안내를 닫으면, Then 사용자 안내가 포커스를 받는다.
#[test]
fn a_user_info_modal_after_an_agent_one_takes_focus() {
    info_modal_next_uses_its_own_origin(false, true);
}

/// 같은 출처끼리의 큐도 그 출처를 지킨다.
#[test]
fn a_same_origin_info_modal_queue_keeps_its_origin() {
    info_modal_next_uses_its_own_origin(true, true);
    info_modal_next_uses_its_own_origin(false, false);
}

fn transfer_error_next_uses_its_own_origin(first_by_user: bool, next_by_user: bool) {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    for (name, by_user) in [("a.txt", first_by_user), ("b.txt", next_by_user)] {
        state.dialogs.transfer_error.push_back(Queued {
            item: failure(name),
            origin: origin(by_user),
        });
    }
    open_then_close(
        &mut state,
        &mut engine,
        TRANSFER_ERROR_POPUP_ID,
        first_by_user,
        PopupPayload::None,
        on_close_transfer_error,
    );
    assert_eq!(state.dialogs.transfer_error.front().unwrap().name, "b.txt");
    assert!(state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
    assert_eq!(
        state.popups.is_passive(TRANSFER_ERROR_POPUP_ID),
        !next_by_user
    );
}

/// Given 사용자 실패 뒤에 에이전트 실패가 큐에 있음, When 앞 실패를 닫으면, Then 다음 실패는 passive 로 열린다.
/// 반대 순서와 같은 출처끼리도 다음 실패는 자기 출처로 열린다.
#[test]
fn the_next_transfer_error_opens_with_its_own_origin() {
    transfer_error_next_uses_its_own_origin(true, false);
    transfer_error_next_uses_its_own_origin(false, true);
    transfer_error_next_uses_its_own_origin(true, true);
    transfer_error_next_uses_its_own_origin(false, false);
}

/// 사용자 출처로 `id` 를 `payload` 와 함께 연다.
fn open_by_user(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    id: &'static str,
    payload: PopupPayload,
) {
    let open = UiIntent::OpenPopup(
        OpenRequest::new(id, PopupScope::Window)
            .centered()
            .focused()
            .with_payload(payload),
    )
    .from_user_menu("test");
    crate::intent::popup::handle(state, engine, &open);
}

/// 큐 `first` 를 열어 닫고, 닫기 훅이 돌기 전에 같은 범위로 `next` 를 연 뒤 옛 훅을 돌린다. 닫기 훅은
/// 프레임 끝에 돌므로 이 순서가 실제로 생긴다. 새로 연 인스턴스가 그대로 남아야 한다.
fn close_open_then_hook(
    id: &'static str,
    first: PopupPayload,
    next: PopupPayload,
    hook: crate::adapters::ui::popup::PopupCloseHook,
) -> RequestContext {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    open_by_user(&mut state, &mut engine, id, first);
    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup { id }.from_user_menu("test"),
    );
    open_by_user(&mut state, &mut engine, id, next);
    let opened = state
        .popups
        .instance_for(id, &PopupScope::Window)
        .expect("the new request opens its own instance");
    let ctx = egui::Context::default();
    for closed in state.popups.take_closed_queue() {
        hook(&ctx, &mut state, &engine.read(), &closed);
    }
    assert_eq!(
        state.popups.instance_for(id, &PopupScope::Window),
        Some(opened),
        "the instance opened in between keeps its place and focus"
    );
    assert_eq!(state.popups.instances_of(id).count(), 1);
    assert!(state.popups.is_focused(id));
    state
}

/// Given 안내 [a, b]를 닫았고 닫기 훅 전에 새 안내 c가 열림, When 옛 닫기 훅이 돌면, Then 열린 c 인스턴스의
/// 차례가 b, c다. 남은 안내가 먼저 들어온 것이라 앞에 온다.
#[test]
fn a_late_info_modal_close_hook_keeps_the_newly_opened_modal() {
    let user = |body: &str| Queued {
        item: modal(body),
        origin: origin(true),
    };
    let state = close_open_then_hook(
        INFO_MODAL_ID,
        info_payload([user("a"), user("b")].into_iter().collect()),
        info_payload([user("c")].into_iter().collect()),
        on_close_info_modal,
    );
    let bodies: Vec<String> = state
        .popups
        .topmost_payload(INFO_MODAL_ID)
        .and_then(super::InfoModalQueue::of)
        .map(|q| q.items.iter().map(|m| m.body.clone()).collect())
        .unwrap_or_default();
    assert_eq!(bodies, vec!["b", "c"]);
}

fn queued_bodies(state: &RequestContext) -> Vec<String> {
    state
        .popups
        .topmost_payload(INFO_MODAL_ID)
        .and_then(super::InfoModalQueue::of)
        .map(|q| q.items.iter().map(|m| m.body.clone()).collect())
        .unwrap_or_default()
}

/// Given 데이터 없이 연 안내 인스턴스(debug 열기), When 실제 안내를 열면, Then 그 안내가 인스턴스 큐에
/// 들어가고 인스턴스 id·위치·포커스는 그대로다.
#[test]
fn a_real_info_modal_fills_an_instance_opened_without_data() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    open_by_user(&mut state, &mut engine, INFO_MODAL_ID, PopupPayload::None);
    let opened = state
        .popups
        .instance_for(INFO_MODAL_ID, &PopupScope::Window)
        .expect("the data-less request opens an instance");
    let pos = state
        .popups
        .instances_of(INFO_MODAL_ID)
        .map(|p| p.pos)
        .next();

    let user = Queued {
        item: modal("a"),
        origin: origin(true),
    };
    open_by_user(
        &mut state,
        &mut engine,
        INFO_MODAL_ID,
        info_payload([user].into_iter().collect()),
    );

    assert_eq!(
        state
            .popups
            .instance_for(INFO_MODAL_ID, &PopupScope::Window),
        Some(opened)
    );
    assert_eq!(state.popups.instances_of(INFO_MODAL_ID).count(), 1);
    assert_eq!(
        state
            .popups
            .instances_of(INFO_MODAL_ID)
            .map(|p| p.pos)
            .next(),
        pos
    );
    assert!(state.popups.is_focused(INFO_MODAL_ID));
    assert_eq!(queued_bodies(&state), vec!["a"]);
}

/// Given 안내 [a, b]를 닫았고 닫기 훅 전에 데이터 없는 안내 인스턴스가 열림, When 옛 닫기 훅이 돌면, Then
/// 남은 b가 그 인스턴스의 큐가 되고 인스턴스는 그대로다.
#[test]
fn a_late_info_modal_close_hook_fills_a_data_less_instance() {
    let user = |body: &str| Queued {
        item: modal(body),
        origin: origin(true),
    };
    let state = close_open_then_hook(
        INFO_MODAL_ID,
        info_payload([user("a"), user("b")].into_iter().collect()),
        PopupPayload::None,
        on_close_info_modal,
    );
    assert_eq!(queued_bodies(&state), vec!["b"]);
}

/// Given 데이터 없이 연 안내 인스턴스, When 안내 큐가 아닌 데이터(팔레트 입력)를 실은 같은 팝업의 열기가 오면,
/// Then 데이터는 없는 그대로이고 인스턴스도 그대로다.
#[test]
fn an_instance_opened_without_data_rejects_another_kind_of_data() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    open_by_user(&mut state, &mut engine, INFO_MODAL_ID, PopupPayload::None);
    let opened = state
        .popups
        .instance_for(INFO_MODAL_ID, &PopupScope::Window)
        .expect("the data-less request opens an instance");

    open_by_user(
        &mut state,
        &mut engine,
        INFO_MODAL_ID,
        PopupPayload::CommandPalette(Default::default()),
    );

    assert_eq!(
        state
            .popups
            .instance_for(INFO_MODAL_ID, &PopupScope::Window),
        Some(opened)
    );
    assert_eq!(state.popups.instances_of(INFO_MODAL_ID).count(), 1);
    assert!(matches!(
        state.popups.topmost_payload(INFO_MODAL_ID),
        Some(PopupPayload::None)
    ));
}

/// 세 큐 팝업은 `Queue` 를 선언한다. 범위가 달라도 창에 인스턴스 하나로 차례를 보인다.
#[test]
fn the_three_queue_popups_declare_the_queue_policy() {
    use crate::adapters::ui::popup::approval::APPROVAL_POPUP_ID;
    use tasty_plugin_protocol::popup_decl::PopupInstances;
    for id in [INFO_MODAL_ID, TRANSFER_ERROR_POPUP_ID, APPROVAL_POPUP_ID] {
        let def = crate::adapters::ui::popup::defs::all_defs()
            .iter()
            .find(|d| d.id == id)
            .expect("declared");
        assert_eq!(def.decl.instances, PopupInstances::Queue, "{id}");
    }
}

type DrawFn = fn(
    &mut egui::Ui,
    &mut crate::state::MainViewState,
    &crate::runtime::engine_read::EngineRead<'_>,
    &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> crate::adapters::ui::popup::PopupAction;

fn key_event(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// `draw` 를 `focused` 문맥으로 `key` 를 누른 한 프레임 그리고 동작이 닫기인지 돌려준다.
fn closes_on_key(
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    id: &'static str,
    draw: DrawFn,
    payload: &mut PopupPayload,
    focused: bool,
    key: egui::Key,
) -> bool {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        events: vec![key_event(key)],
        ..Default::default()
    };
    let mut action = crate::adapters::ui::popup::PopupAction::None;
    drop(ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut cx = crate::adapters::ui::popup::PopupCx {
                id,
                instance: crate::adapters::ui::popup::PopupInstanceId::NONE,
                focused,
                payload: &mut *payload,
            };
            action = draw(ui, state, engine, &mut cx);
        });
    }));
    !matches!(action, crate::adapters::ui::popup::PopupAction::None)
}

/// Given passive로 열린(포커스 없는) 안내·실패 팝업, When 다른 곳의 포커스에서 Enter·Escape를 누르면,
/// Then 항목은 닫히지 않는다. 포커스를 가지면 같은 키로 닫힌다.
#[test]
fn a_passive_queue_popup_ignores_enter_and_escape() {
    crate::i18n::init("en");
    let (mut state, session) = crate::state::tests::test_state();
    state.dialogs.transfer_error.push_back(Queued {
        item: failure("f"),
        origin: origin(false),
    });
    let engine = session.read();
    let popups: [(&'static str, DrawFn, PopupPayload); 2] = [
        (
            INFO_MODAL_ID,
            super::draw_info_modal,
            info_payload(
                [Queued {
                    item: modal("a"),
                    origin: origin(false),
                }]
                .into(),
            ),
        ),
        (
            TRANSFER_ERROR_POPUP_ID,
            crate::adapters::ui::popup::transfer::draw_transfer_error,
            PopupPayload::None,
        ),
    ];
    for (id, draw, mut payload) in popups {
        for key in [egui::Key::Enter, egui::Key::Escape] {
            assert!(
                !closes_on_key(&mut state, &engine, id, draw, &mut payload, false, key),
                "{id} {key:?}"
            );
            assert!(
                closes_on_key(&mut state, &engine, id, draw, &mut payload, true, key),
                "{id} {key:?} focused"
            );
        }
    }
    assert_eq!(state.dialogs.transfer_error.len(), 1);
}

/// Given 안내가 열려 있을 때, When 같은 프레임에 안내 둘이 더 요청되면, Then 새 인스턴스를 만들지 않고 열린
/// 인스턴스의 차례 뒤에 그 순서대로 잇는다. 위치·포커스는 그대로다.
#[test]
fn info_modals_shown_while_one_is_open_wait_in_its_queue() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    for def in crate::adapters::ui::popup::defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    for body in ["a", "b", "c"] {
        super::show_info_modal(&mut state, modal(body));
    }
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
    }
    assert_eq!(state.popups.instances_of(INFO_MODAL_ID).count(), 1);
    let bodies: Vec<String> = state
        .popups
        .topmost_payload(INFO_MODAL_ID)
        .and_then(super::InfoModalQueue::of)
        .map(|q| q.items.iter().map(|m| m.body.clone()).collect())
        .unwrap_or_default();
    assert_eq!(bodies, ["a", "b", "c"]);
}
