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

/// [설정 열기 요청 래치 + toggle_settings 더블탭 → 요청을 냄](전후 같음)과 [래치 + 덮는 팝업 없음 + new_tab
/// 더블탭 → 돎](전후 같음)
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

/// [래치 + 덮는 팝업(command_palette) 포커스 + new_tab 더블탭 → 돎](전후 같음. 래치가 없으면 위 장면대로 안 돎)
#[test]
fn under_the_latch_a_covered_double_tap_still_runs() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_focused(&mut state, "command_palette");
    state.settings_open_requested = true;
    assert!(actions(&state, &read).contains(&"new_tab"));
}

/// [래치 + 같은 덮음 + toggle_settings 더블탭 → 요청을 냄](전후 같음)
#[test]
fn under_the_latch_a_covering_popup_keeps_the_settings_double_tap() {
    let (mut state, mut session) = crate::state::tests::test_state();
    let engine = session.borrow_mut();
    let read = engine.read();
    open_focused(&mut state, "command_palette");
    state.settings_open_requested = true;
    assert_eq!(actions(&state, &read).first(), Some(&"toggle_settings"));
}
