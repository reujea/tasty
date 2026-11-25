//! T-12a-2: 더블탭 후보는 포커스 키 경로의 level로 매칭 전에 거른다. 창(winit·GPU)을 띄우지 않으므로 창이 부르는
//! 결정 함수(`focus_path_levels` → `double_tap_actions`)를 지금 상태로 부른다. `handle_double_tap_shortcut`은 이 결과의
//! 첫 동작부터 실행을 시도한다(`toggle_settings`는 `OpenSettings` 요청). 설정 열기 요청 래치는 실제 키로는 한
//! 프레임이라 `settings_open_requested`를 세워 만든다.

use super::*;
use crate::adapters::ui::input::key_path::focus_path_levels;
use crate::adapters::ui::input::shortcuts::double_tap::double_tap_actions;
use crate::settings::KeybindingSettings;

const DT: &str = "shift+shift";

/// `toggle_settings`(engine), `toggle_notifications`(Window), `new_tab`(Pane)을 같은 더블탭에 묶은 설정.
fn kb() -> KeybindingSettings {
    let mut kb = KeybindingSettings::default();
    for field in ["toggle_settings", "toggle_notifications", "new_tab"] {
        assert!(kb.set_field(field, DT));
    }
    kb
}

/// 지금 상태에서 이 더블탭이 고르는 동작.
fn actions(
    state: &crate::state::RequestContext,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) -> Vec<&'static str> {
    double_tap_actions(&kb(), focus_path_levels(state, read), DT)
}

fn open_focused(state: &mut crate::state::RequestContext, id: PopupId) -> PopupInstanceId {
    let iid = state
        .popups
        .open_centered_focused(id, PopupScope::Window)
        .expect("열린다");
    assert_eq!(state.popup_focus(), Some(focus::PopupNode::Host(iid)));
    iid
}

/// [펼친 팝업 포커스 + Window 동작 → 안 돎]
#[test]
fn an_expanded_popup_blocks_a_window_double_tap() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    assert!(!state.popups.instance(iid).unwrap().look.covers_scope);
    state.popups.instance_mut(iid).unwrap().display = display::PopupDisplay::Expanded {
        restore: egui::Pos2::ZERO,
    };
    assert_eq!(state.popup_focus(), Some(focus::PopupNode::Host(iid)));
    assert_eq!(actions(&state, &read), ["toggle_settings"]);
}

/// [덮는 팝업(command_palette) 포커스 + 덮인 범위 level 동작(new_tab) → 안 돎]과
/// [같은 덮음 + engine level 동작(toggle_settings) → 돎]
#[test]
fn a_covering_popup_blocks_a_covered_double_tap_and_keeps_an_engine_one() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "command_palette");
    assert!(state.popups.instance(iid).unwrap().covers());
    let got = actions(&state, &read);
    assert!(!got.contains(&"new_tab"), "{got:?}");
    assert!(!got.contains(&"toggle_notifications"), "{got:?}");
    assert_eq!(got.first(), Some(&"toggle_settings"));
}

/// [덮지 않는 팝업 포커스 + Window 동작 → 돎]. 붙은 범위(Window) 아래 level(new_tab, Pane)은 경로에 없다.
#[test]
fn an_uncovering_popup_keeps_a_window_double_tap() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    assert!(!state.popups.instance(iid).unwrap().covers());
    assert_eq!(
        actions(&state, &read),
        ["toggle_settings", "toggle_notifications"]
    );
}

/// [덮지 않는 팝업 포커스 + Window 동작 → 돎] 대조: 경로 밖 level 의 후보가 없는 설정이라 거르기와 무관하게
/// 같은 목록이다.
#[test]
fn an_uncovering_popup_keeps_a_window_double_tap_without_an_off_path_candidate() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    let iid = open_focused(&mut state, "notifications");
    assert!(!state.popups.instance(iid).unwrap().covers());
    let mut kb = KeybindingSettings::default();
    for field in ["toggle_settings", "toggle_notifications"] {
        assert!(kb.set_field(field, DT));
    }
    assert_eq!(
        double_tap_actions(&kb, focus_path_levels(&state, &read), DT),
        ["toggle_settings", "toggle_notifications"]
    );
}

/// [래치 없음 + toggle_settings 더블탭 → 요청을 냄](대조)
#[test]
fn without_the_latch_a_settings_double_tap_requests_settings() {
    let (state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    assert!(!state.has_settings_open_request());
    assert_eq!(
        actions(&state, &read),
        ["toggle_settings", "toggle_notifications", "new_tab"]
    );
}

/// 래치 중 경로 level 은 거르지 않는다(`focus_path_levels` 가 `None`): 필터만 보면 toggle_settings·new_tab 더블탭이
/// 남는다. 제품에서 래치 중 누름은 단계 11 맨 앞(`end_key_at_settings_latch`)에서 끝나 더블탭 단계에 가지 않는다.
#[test]
fn under_the_latch_nothing_is_filtered() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    state.settings_open_requested = true;
    assert_eq!(
        actions(&state, &read),
        ["toggle_settings", "toggle_notifications", "new_tab"]
    );
}

/// 래치 + 덮는 팝업(command_palette) 포커스에서도 필터는 new_tab 을 거르지 않는다(래치가 없으면 위 장면대로 거른다).
/// 제품에서는 래치 노드가 그 누름을 끝낸다.
#[test]
fn under_the_latch_a_covered_double_tap_still_runs() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_focused(&mut state, "command_palette");
    state.settings_open_requested = true;
    assert!(actions(&state, &read).contains(&"new_tab"));
}

/// 래치 + 같은 덮음에서 필터 결과의 첫 동작은 toggle_settings 다. 제품에서는 래치 노드가 그 누름을 끝낸다.
#[test]
fn under_the_latch_a_covering_popup_keeps_the_settings_double_tap() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_focused(&mut state, "command_palette");
    state.settings_open_requested = true;
    assert_eq!(actions(&state, &read).first(), Some(&"toggle_settings"));
}
