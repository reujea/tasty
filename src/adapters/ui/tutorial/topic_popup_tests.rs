//! 튜토리얼 목록 팝업의 ↓·Enter 는 수식키 없는 누름만 받는다.

use super::*;
use crate::adapters::ui::popup::window_input_test_support::{
    Held, OwnerBody, draw_owner_body, held_press,
};

/// 키 주인 목록을 `held` 를 붙인 `key` 한 프레임으로 그린다. 고른 줄과 그리기 결과를 돌려준다.
fn topics_frame(key: egui::Key, held: Option<Held>) -> (usize, Option<usize>, PopupAction) {
    crate::i18n::init("en");
    let (mut state, session) = crate::state::tests::test_state();
    let engine = session.read();
    // 진행 기록 파일을 읽지 않도록 목록을 이미 불러온 것으로 둔다.
    state.tutorial.catalog_loaded = true;
    let ctx = egui::Context::default();
    let mut payload = crate::adapters::ui::popup::PopupPayload::None;
    let press = held_press(&ctx, key, held);
    let action = draw_owner_body(
        &ctx,
        &mut state,
        &engine,
        OwnerBody {
            id: TUTORIAL_TOPICS_POPUP_ID,
            draw: draw_tutorial_topics_popup,
            payload: &mut payload,
            keyboard: crate::adapters::ui::popup::PopupKeyboard::Keys,
        },
        vec![press],
    );
    (
        state.tutorial.popup_selected,
        state.tutorial.pending_start,
        action,
    )
}

/// T-9a: Given 키 주인 튜토리얼 목록, When 수식키 없는 ↓ 를 누르면, Then 다음 줄을 고른다.
#[test]
fn bare_down_moves_the_topic_selection() {
    assert_eq!(topics_frame(egui::Key::ArrowDown, None).0, 1);
}

/// T-9a: Given 키 주인 튜토리얼 목록, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+↓ 를 누르면, Then 줄이 그대로다.
#[test]
fn modified_down_is_ignored_by_the_topics() {
    for held in Held::ALL {
        assert_eq!(
            topics_frame(egui::Key::ArrowDown, Some(held)).0,
            0,
            "{held:?}"
        );
    }
}

/// T-9a: Given 키 주인 튜토리얼 목록, When 수식키 없는 Enter 를 누르면, Then 고른 주제를 시작하고 닫는다.
#[test]
fn bare_enter_starts_the_topic() {
    let (_, start, action) = topics_frame(egui::Key::Enter, None);
    assert_eq!(start, Some(0));
    assert!(matches!(action, PopupAction::Close));
}

/// T-9a: Given 키 주인 튜토리얼 목록, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+Enter 를 누르면, Then 시작하지 않는다.
#[test]
fn modified_enter_is_ignored_by_the_topics() {
    for held in Held::ALL {
        let (_, start, action) = topics_frame(egui::Key::Enter, Some(held));
        assert_eq!(start, None, "{held:?}");
        assert!(matches!(action, PopupAction::None), "{held:?}");
    }
}
