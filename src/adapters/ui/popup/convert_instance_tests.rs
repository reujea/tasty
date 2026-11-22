//! 변환 팝업이 surface마다 인스턴스를 따로 열고 대상·선택을 인스턴스에 두는지 확인한다.

use super::{CONVERT_POPUP_ID, ConvertPayload, open_request};
use crate::adapters::ui::popup::{PopupCloseReason, PopupInstanceId, PopupPayload};
use crate::intent::UiIntent;
use crate::state::RequestContext;

fn reopen(
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    surface_id: u32,
) {
    let mut engine = session.borrow_mut();
    crate::intent::popup::handle(
        state,
        &mut engine,
        &UiIntent::ReopenPopup(open_request(surface_id)).from_user_menu("test"),
    );
}

/// 열린 변환 인스턴스의 (인스턴스 id, 대상 데이터)를 연 순서대로 모은다.
fn opened(state: &RequestContext) -> Vec<(PopupInstanceId, ConvertPayload)> {
    let mut found: Vec<_> = state
        .popups
        .instances_of(CONVERT_POPUP_ID)
        .map(|p| match &p.payload {
            PopupPayload::Convert(c) => (p.instance, c.clone()),
            other => panic!("convert instance without its data: {other:?}"),
        })
        .collect();
    found.sort_by_key(|(iid, _)| *iid);
    found
}

fn target(surface_id: u32, selected: Option<usize>) -> ConvertPayload {
    ConvertPayload {
        surface_id,
        selected,
    }
}

fn select(state: &mut RequestContext, iid: PopupInstanceId, selected: usize) {
    let p = state.popups.instance_mut(iid).expect("open instance");
    let PopupPayload::Convert(c) = &mut p.payload else {
        panic!("convert instance without its data");
    };
    c.selected = Some(selected);
}

/// 왼쪽·오른쪽 surface에서 연 변환 팝업은 각자 자기 surface를 대상으로 한다.
#[test]
fn each_surface_opens_its_own_convert_instance() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);

    let found = opened(&state);
    assert_eq!(
        found.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        vec![target(1, None), target(2, None)]
    );
}

/// 같은 surface에서 다시 열면 그 인스턴스의 선택만 처음으로 돌아가고 다른 surface의 선택은 남는다.
#[test]
fn reopening_on_one_surface_resets_only_its_own_selection() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);
    let found = opened(&state);
    let (left, right) = (found[0].0, found[1].0);
    select(&mut state, left, 2);
    select(&mut state, right, 1);

    reopen(&mut state, &mut session, 1);

    assert_eq!(
        opened(&state),
        vec![(left, target(1, None)), (right, target(2, Some(1)))]
    );
}

/// 한 surface의 변환 팝업을 닫아도 다른 surface의 인스턴스와 대상은 그대로다.
#[test]
fn closing_one_surface_instance_keeps_the_other_target() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);
    let found = opened(&state);
    let (left, right) = (found[0].0, found[1].0);
    select(&mut state, right, 1);

    state.popups.close_instance(left, PopupCloseReason::Escape);

    assert_eq!(opened(&state), vec![(right, target(2, Some(1)))]);
}

/// 두 surface의 변환 인스턴스를 한 프레임에 그리며 키 하나를 넣는다. 인스턴스마다 그리기 결과를 돌려준다.
fn draw_with_key(key: egui::Key, focused: [bool; 2]) -> [&'static str; 2] {
    let (mut state, session) = crate::state::tests::two_pane_fixture();
    let engine = session.read();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events: vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut acted = ["none"; 2];
    drop(egui::Context::default().run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (i, sid) in [101, 201].into_iter().enumerate() {
                let mut payload = PopupPayload::Convert(target(sid, Some(0)));
                let mut cx = crate::adapters::ui::popup::PopupCx {
                    id: CONVERT_POPUP_ID,
                    instance: PopupInstanceId::NONE,
                    focused: focused[i],
                    keys: crate::adapters::ui::popup::keys::PopupKeys::new(
                        ctx,
                        focused[i],
                        crate::adapters::ui::popup::PopupKeyboard::Keys,
                        &crate::adapters::ui::input::edit_key::EditBindings::default(),
                    ),
                    payload: &mut payload,
                };
                let action = super::draw_convert_popup(ui, &mut state, &engine, &mut cx);
                acted[i] = match action {
                    crate::adapters::ui::PopupAction::None => "none",
                    crate::adapters::ui::PopupAction::CloseBy(PopupCloseReason::Escape) => "escape",
                    _ => "acted",
                };
            }
        });
    }));
    acted
}

/// 인스턴스마다 키에 반응했는지.
fn press_with_focus(key: egui::Key, focused: [bool; 2]) -> [bool; 2] {
    draw_with_key(key, focused).map(|a| a != "none")
}

/// 첫 항목의 글자 단축키.
fn first_shortcut_key() -> egui::Key {
    let (state, session) = crate::state::tests::two_pane_fixture();
    let ch = super::listed_kinds(&state, &session.read(), Some(101))
        .into_iter()
        .find_map(|item| item.shortcut)
        .expect("a convert item with a letter shortcut");
    egui::Key::from_name(&ch.to_string()).expect("letter key")
}

/// Given 두 surface에 변환 팝업이 보이고 한쪽만 포커스를 가졌을 때, When Enter·글자 단축키를 누르면,
/// Then 포커스를 가진 인스턴스만 반응한다. Esc 는 본문이 읽지 않는다(아래 시험).
#[test]
fn only_the_focused_convert_instance_reads_keys() {
    for key in [egui::Key::Enter, first_shortcut_key()] {
        assert_eq!(
            press_with_focus(key, [false, true]),
            [false, true],
            "{key:?}"
        );
        assert_eq!(
            press_with_focus(key, [true, false]),
            [true, false],
            "{key:?}"
        );
    }
}

/// When 포커스를 가진 변환 팝업에서 Escape를 누르면, Then 두 본문 모두 반응하지 않는다. 본문은 Esc 를 읽지 않는다. 닫기는 팝업 층 중앙 단계가 선언대로 정한다(T-4a3-25).
#[test]
fn the_convert_body_leaves_escape_to_the_popup_layer() {
    assert_eq!(
        draw_with_key(egui::Key::Escape, [true, false]),
        ["none", "none"]
    );
}
