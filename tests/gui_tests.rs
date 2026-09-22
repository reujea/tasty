//! GUI integration tests for tasty.
//!
//! All tests share a single tasty GUI instance. Each test creates its own
//! workspace(s) for isolation — tasty's architecture guarantees that independent
//! workspaces don't interfere with each other.
//!
//! Run with: cargo test --test gui_tests -- --ignored --test-threads=1
//! (single-threaded because only one window can have OS keyboard focus at a time)
//!
//! These tests are ignored by default since they require a display and
//! take focus of the desktop.
//!
//! ## 공허한 초록 — 이 파일 전수 조사 (2026-09-06)
//!
//! 이 스위트의 초록은 **자극이 통했다는 뜻이 아니다.** 자극 경로(enigo 키보드 ·
//! `debug.inject_window_mouse` · IPC)가 무효과여도, 검사가 전부 "아무 일도 안 일어났다"
//! 형태면 시험은 통과한다. 실측된 사건이 있다 — 주입이 죽은 슬롯의 발사에서 형제 둘이
//! 반대 색이었고(하나는 `got: ""` 로 빨강, 하나는 통과), 그 통과는 정책 준수가 아니라
//! 같은 사망의 다른 얼굴이었다.
//!
//! **가르는 축은 단정의 문법이 아니다.** `assert!(!x.contains(..))` 는 부정형이지만
//! 앞에 자극이 통해야 지나가는 줄이 있으면 공허하지 않고, `assert_eq!(focus, 그대로)` 는
//! 긍정형이지만 내용은 무변화라 자극이 죽어도 통과한다. 물어야 할 것은 하나다 —
//! **자극이 무효과일 때 이 시험이 통과하는가.**
//!
//! 그 축으로 33 건을 전수로 세면:
//!
//! - **26 건**(`test_*` 키보드 계열)은 `wait_for_ui(.., 바뀐 값)` 이 상한 안에 안 오면
//!   패닉한다 — 자극이 죽으면 빨개진다. 자기 안에 비영 대조를 갖고 있다.
//! - **5 건**(마우스 주입 계열 중 `click_to_activate_moves_focus` ·
//!   `drag_creates_local_selection` · `right_click_*` 둘 · `drag_motion_carries_the_pressed_button`)
//!   은 존재를 요구하는 단정(`present == true` · `contains(..)`)을 앞에 둔다. 같음.
//! - **2 건**이 공허했다: `hover_motion_never_reaches_a_non_focused_surface` 와
//!   `test_ime_preedit_cleared_on_popup_focus_shortcut`. 둘 다 이제 자기 안에 비영
//!   대조를 갖는다(각 시험의 `★★ 비영 대조` 주석).
//!
//! **한 건은 대조가 이 시험 밖에 있다.** `test_keyboard_not_sent_to_terminal_when_settings_open`
//! 의 단정은 `!output.contains(..)` 하나인데, 타이핑 경로가 살아 있다는 증거는 형제
//! `test_keyboard_sent_to_terminal_when_no_overlay` 가 낸다. 같은 바이너리에서 함께 도니
//! 경로가 죽으면 형제가 빨개져 드러나긴 한다 — 다만 **이 시험 하나만 뽑아 돌리면 그
//! 보증이 없다.** 안으로 옮기지 않은 이유는 그러면 같은 국면을 두 번 태우게 돼서다.
//!
//! ## 설정 창을 기다리는 기준
//!
//! 이 파일의 설정 계열 시험은 `UiState::settings_modal_is_up()`
//! (= `modal_open` + `active_modal_kind == Settings`)으로 기다린다.
//! 예전 기준이던 `settings_open_requested` 로는 **원리적으로 못 기다린다** — 그 값은
//! 열기 요청 래치이고, 여기서 쓰는 `Ctrl+,` 는 `AppEvent` 직행 경로라 그 필드를 아예 안
//! 건드린다. 실측(고치기 전, 같은 Xvfb): `test_settings_open_ctrl_comma` 가 3 초 타임아웃,
//! 그때 덤프는 `settings_open_requested: false` 인데 `modal_open: true` 였다 —
//! 창은 떠 있었고 기다린 값만 없었다.
//!
//! ## 설정 창을 닫는 손
//!
//! **`toggle_settings` 바인딩(기본 `Ctrl+,`)이 연 창을 닫는다.** `Escape` 는 아직 안
//! 닫는다 — `SettingsView::handle_event`(`src/view/settings.rs`)에 그 분기가 없다.
//! 실재하는 다른 닫기 경로는 창 닫기 요청(`WindowEvent::CloseRequested`)과 egui 액션인데,
//! WM 없는 Xvfb 에는 앞의 것을 보낼 손이 없다 — 창은 `WM_DELETE_WINDOW` 를 광고하지만
//! `xdotool windowclose` 는 그것을 안 쓰고 `XDestroyWindow` 를 불러 winit 이 패닉하고,
//! `wmctrl -i -c` 는 WM 이 없으면 아무도 처리하지 않는다.
//!
//! 그래서 시험의 정리 단계는 `GuiTestInstance::close_active_modal()`
//! (= `debug.modal.close_request`)로 한다. 무엇이 닫고 무엇이 안 닫는지는
//! `test_the_toggle_binding_closes_the_settings_modal_and_escape_does_not` 이 고정한다 —
//! 그것은 옳음의 단정이 아니라 **현재 동작의 기록**이고, 동작이 바뀌면 그 시험이 알려준다.

mod gui_common;

use enigo::Key;
use gui_common::shared;
use serde_json::json;
use std::time::{Duration, Instant};

/// Maximum acceptable latency for UI operations (milliseconds).
/// This includes ~340ms of intentional sleep in input simulation helpers
/// (focus + key press/release timing), so the effective UI response budget
/// is roughly MAX_UI_RESPONSE_MS - 340ms.
const MAX_UI_RESPONSE_MS: u128 = 1000;

/// Helper: measure how long a UI condition takes to become true after an action.
fn measure_ui_latency<F, C>(
    inst: &mut gui_common::GuiTestInstance,
    action_name: &str,
    action: F,
    condition: C,
) -> Duration
where
    F: FnOnce(&mut gui_common::GuiTestInstance),
    C: Fn(&gui_common::UiState) -> bool,
{
    let start = Instant::now();
    action(inst);
    inst.wait_for_ui(action_name, Duration::from_secs(5), &condition);
    start.elapsed()
}

// ============================================================
// Settings Window Tests
// ============================================================

#[test]
#[ignore]
fn test_settings_open_ctrl_comma() {
    let mut inst = shared();

    // Verify settings is initially closed
    let state = inst.ui_state();
    assert!(
        !state.settings_modal_is_up(),
        "settings should be closed initially"
    );

    // Press Ctrl+, to open settings
    inst.press_ctrl(Key::Unicode(','));

    let state = inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });
    assert!(
        state.settings_modal_is_up(),
        "settings should be open after Ctrl+,"
    );

    // Cleanup: 키보드로는 못 닫는다 — 아래 두 시험이 그 사실을 값으로 고정한다.
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

/// 닫기 **요청**으로 닫힌다 — 사용자가 창 닫기 버튼을 누르는 경로.
///
/// 예전 이름은 `test_settings_close_ctrl_comma` 였는데 그 이름이 담은 기대가 거짓이다
/// (아래 형제 시험이 그것을 잰다). 이 시험이 재는 것은 **실재하는 유일한 닫기 경로**다.
#[test]
#[ignore]
fn test_settings_closes_on_close_request() {
    let mut inst = shared();

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    inst.close_active_modal();

    let state = inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
    assert!(
        !state.settings_modal_is_up(),
        "close request 를 받은 모달은 사라져야 한다"
    );
    assert!(
        state.active_modal_kind.is_none(),
        "종류도 함께 지워져야 한다 — 남으면 다음 시험이 자기가 안 연 창을 본다. 지금: {:?}",
        state.active_modal_kind
    );
}

/// 설정 모달을 **키보드로 닫는 길**과, 아직 닫지 않는 키를 함께 고정한다.
///
/// · `Ctrl+,`(= `toggle_settings` 바인딩)는 연 창을 **닫는다.** 필드 이름이 토글을
///   약속하는데 여는 쪽만 구현돼 있었고, 그래서 키보드로 이 창을 벗어나는 길이 없었다.
///   닫는 경로는 창 닫기 버튼과 같은 `ViewAction::Close` 라 저장/취소 cascade 가 그대로
///   흐른다.
/// · `Escape` 는 **아직 안 닫는다.** 팝업은 Escape 로 닫히므로 비대칭이 남아 있지만,
///   그쪽은 텍스트 입력 중 Escape 가 편집 취소인지 창 닫기인지, 바인딩 녹화 중에는
///   무엇인지 같은 판단이 먼저다. 여기서는 **지금 무엇이 참인지만** 적는다.
///
/// 키를 상수로 안 쓰고 바인딩으로 누르는 이유: 무엇으로 닫히는가는
/// `KeybindingSettings` 가 정한다. 사용자가 그 바인딩을 바꾸면 닫는 키도 함께 바뀐다.
///
/// 동작이 바뀌면 이 시험이 빨개진다 — 그때 이 시험을 함께 바꿔라. 그것이 이 시험의
/// 목적이다: 바뀐 것이 조용히 지나가지 않게 한다.
///
/// 이 기대를 담은 시험이 원래 넷 있었고 **한 번도 검증된 적이 없었다.** 열기 관측
/// 채널이 어긋나 있어서(열기부터 타임아웃) 닫기까지 도달한 적이 없었기 때문이다.
#[test]
#[ignore]
fn test_the_toggle_binding_closes_the_settings_modal_and_escape_does_not() {
    let mut inst = shared();

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    inst.press_key(Key::Escape);
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        inst.ui_state().settings_modal_is_up(),
        "Escape 로 설정 모달이 닫혔다 — 동작이 바뀐 것이다. 이 시험과 그 근거 주석을 \
         함께 갱신해라(닫혔다는 것 자체는 결함이 아닐 수 있다)"
    );

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

#[test]
#[ignore]
fn test_settings_open_speed() {
    let mut inst = shared();

    let elapsed = measure_ui_latency(
        &mut inst,
        "settings open speed",
        |i| i.press_ctrl(Key::Unicode(',')),
        |s| s.settings_modal_is_up(),
    );

    println!("Settings open latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Settings open took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup — 여기서는 debug 경로로 닫는다. 키보드 토글이 닫는다는 것은 형제 시험이
    // 따로 고정하므로, 이 시험이 그것까지 태우면 두 물음이 한 국면에 섞인다.
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

// ============================================================
// Notification Panel Tests
// ============================================================

#[test]
#[ignore]
fn test_notification_panel_toggle() {
    let mut inst = shared();

    let state = inst.ui_state();
    assert!(
        !state.notification_panel_open,
        "notification panel should be closed initially"
    );

    // Ctrl+Shift+I to open
    inst.press_ctrl_shift(Key::Unicode('i'));

    let state = inst.wait_for_ui("notification panel open", Duration::from_secs(3), |s| {
        s.notification_panel_open
    });
    assert!(state.notification_panel_open);

    // Ctrl+Shift+I to close
    inst.press_ctrl_shift(Key::Unicode('i'));

    let state = inst.wait_for_ui("notification panel close", Duration::from_secs(3), |s| {
        !s.notification_panel_open
    });
    assert!(!state.notification_panel_open);
}

#[test]
#[ignore]
fn test_notification_panel_close_escape() {
    let mut inst = shared();

    // Open notification panel
    inst.press_ctrl_shift(Key::Unicode('i'));
    inst.wait_for_ui("notification open", Duration::from_secs(3), |s| {
        s.notification_panel_open
    });

    // Close with Escape
    inst.press_key(Key::Escape);

    let state = inst.wait_for_ui(
        "notification panel close via escape",
        Duration::from_secs(3),
        |s| !s.notification_panel_open,
    );
    assert!(!state.notification_panel_open);
}

#[test]
#[ignore]
fn test_notification_panel_speed() {
    let mut inst = shared();

    let elapsed = measure_ui_latency(
        &mut inst,
        "notification panel open speed",
        |i| i.press_ctrl_shift(Key::Unicode('i')),
        |s| s.notification_panel_open,
    );

    println!("Notification panel open latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Notification panel open took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_ctrl_shift(Key::Unicode('i'));
    inst.wait_for_ui("notification closed", Duration::from_secs(3), |s| {
        !s.notification_panel_open
    });
}

// ============================================================
// Workspace Tests
// Uses IPC to create workspaces, keyboard to interact.
// ============================================================

#[test]
#[ignore]
fn test_new_workspace_ctrl_shift_n() {
    let mut inst = shared();
    let initial_count = inst.ui_state().workspace_count;

    // Alt+N to create new workspace (preset: new_workspace)
    inst.press_alt(Key::Unicode('n'));

    let state = inst.wait_for_ui("workspace count increased", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count + 1
    });
    assert_eq!(state.workspace_count, initial_count + 1);

    // Cleanup: close the workspace we created (Alt+Shift+W)
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("workspace closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count
    });
}

#[test]
#[ignore]
fn test_workspace_switch_alt_number() {
    let mut inst = shared();

    // Create a fresh workspace for this test
    let initial_count = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("new ws", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count + 1
    });

    // Should be on the new workspace (last index)
    let state = inst.ui_state();
    let new_ws_idx = state.active_workspace;

    // Switch to first workspace (Alt+1)
    inst.press_alt(Key::Unicode('1'));
    inst.wait_for_ui("switch to ws 0", Duration::from_secs(3), |s| {
        s.active_workspace == 0
    });

    // Switch back
    let target = new_ws_idx + 1; // Alt+N is 1-based
    let key = char::from_digit(target as u32, 10).unwrap();
    inst.press_alt(Key::Unicode(key));
    inst.wait_for_ui("switch back", Duration::from_secs(3), |s| {
        s.active_workspace == new_ws_idx
    });

    // Cleanup: close the test workspace
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count
    });
}

#[test]
#[ignore]
fn test_workspace_creation_speed() {
    let mut inst = shared();
    let initial_count = inst.ui_state().workspace_count;

    let elapsed = measure_ui_latency(
        &mut inst,
        "workspace creation speed",
        |i| i.press_alt(Key::Unicode('n')),
        |s| s.workspace_count == initial_count + 1,
    );

    println!("Workspace creation latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Workspace creation took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count
    });
}

// ============================================================
// Tab Tests
// ============================================================

#[test]
#[ignore]
fn test_new_tab_ctrl_shift_t() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let state = inst.ui_state();
    assert_eq!(state.tab_count, 1, "new workspace should start with 1 tab");

    // Ctrl+Shift+T to create new tab
    inst.press_ctrl_shift(Key::Unicode('t'));
    let state = inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);
    assert_eq!(state.tab_count, 2);

    // Cleanup: close the test workspace
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_close_tab_ctrl_w() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Create a second tab
    inst.press_ctrl_shift(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    // Ctrl+W to close the active tab
    inst.press_ctrl(Key::Unicode('w'));
    let state = inst.wait_for_ui("1 tab", Duration::from_secs(3), |s| s.tab_count == 1);
    assert_eq!(state.tab_count, 1);

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_tab_creation_speed() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let elapsed = measure_ui_latency(
        &mut inst,
        "tab creation speed",
        |i| i.press_ctrl_shift(Key::Unicode('t')),
        |s| s.tab_count == 2,
    );

    println!("Tab creation latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Tab creation took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ============================================================
// Pane Split Tests
// ============================================================

#[test]
#[ignore]
fn test_pane_split_vertical_ctrl_shift_e() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    assert_eq!(inst.ui_state().pane_count, 1);

    // Alt+E for vertical pane split (preset: split_pane_vertical)
    inst.press_alt(Key::Unicode('e'));
    let state = inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);
    assert_eq!(state.pane_count, 2);

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_pane_split_horizontal_ctrl_shift_o() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Alt+Shift+E for horizontal pane split (preset: split_pane_horizontal)
    inst.press_alt_shift(Key::Unicode('e'));
    let state = inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);
    assert_eq!(state.pane_count, 2);

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_close_pane_ctrl_shift_w() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Split first
    inst.press_alt(Key::Unicode('e'));
    inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);

    // Close the active pane
    inst.press_ctrl_shift(Key::Unicode('w'));
    let state = inst.wait_for_ui("1 pane", Duration::from_secs(3), |s| s.pane_count == 1);
    assert_eq!(state.pane_count, 1);

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_pane_split_speed() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let elapsed = measure_ui_latency(
        &mut inst,
        "pane split speed",
        |i| i.press_alt(Key::Unicode('e')),
        |s| s.pane_count == 2,
    );

    println!("Pane split latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Pane split took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ============================================================
// Keyboard Routing Tests
// ============================================================

#[test]
#[ignore]
fn test_keyboard_not_sent_to_terminal_when_settings_open() {
    let mut inst = shared();

    // ★ mark/read 는 `surface_id` 가 **필수**다(`handler/surface/mark.rs` 의
    // `require_surface_id` — 포커스 기반 기본값이 없다, 원칙 3). 생략하면 격리 여부와
    // 무관하게 `-32602 missing 'surface_id'` 로 첫 줄에서 죽는다.
    // `first_surface_id()` 는 쓰지 않는다 — `surface.list` 의 **첫** 항목이라 대상이
    // 활성 터미널이라는 보장이 없다(마우스 축에서 실측으로 밟은 함정이다).
    // settings 를 **열기 전에** 잡는다: 오버레이가 뜬 뒤에는 포커스 개념이 달라진다.
    let sid = inst
        .debug_focused_surface()
        .expect("mark 를 찍을 포커스된 surface");

    // Set a mark so we can check terminal output
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    // Open settings
    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings open", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    // Type some text — should NOT reach the terminal.
    // ★ **본창에** 넣는다. 이 시험이 재는 것은 "모달이 떠 있는 동안 본창이
    // `KeyboardInput` 을 끊는가"(`src/view/main.rs` 의 모달 분기)이므로, 키가 본창에
    // 도착해야 성립한다 — 모달이 받아 버리면 재는 것이 그 게이트가 아니라 "모달이
    // 포커스를 가져갔다" 가 되고, 두 초록은 같은 모양이다.
    inst.type_text_into_main_window("hello_should_not_appear");
    std::thread::sleep(Duration::from_millis(500));

    // Check terminal did not receive the text
    let result = inst.call(
        "surface.read_since_mark",
        serde_json::json!({ "surface_id": sid, "strip_ansi": true }),
    );
    let output = result["text"].as_str().unwrap_or("");
    assert!(
        !output.contains("hello_should_not_appear"),
        "Terminal should NOT receive keyboard input when settings is open. Got: {}",
        output,
    );

    // Cleanup: close settings — Escape 로는 안 닫힌다(형제 시험이 그 사실을 고정한다).
    // 여기서 Escape 를 쓰면 창이 남고, **다음 시험이 그 창을 물려받는다.**
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });

    // ★ 비영 대조 — 위 단정은 `!contains(..)` 하나뿐이라, 타이핑이 **아무 데도** 안 가면
    // 그것만으로도 초록이 된다. 그 초록은 "settings 가 막았다" 가 아니라 "아무 일도 안
    // 일어났다" 다. 그래서 같은 회차·같은 인스턴스에서 **경로가 살아 있음**을 보인다.
    // 형제 `test_keyboard_sent_to_terminal_when_no_overlay` 가 이 형태를 갖고 있는데,
    // 그 증거가 다른 시험에 있으면 이 시험을 단독으로 돌릴 때는 아무 보장이 없다.
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));
    inst.type_text("echo overlay_control_marker");
    inst.press_key(Key::Return);
    std::thread::sleep(Duration::from_millis(1000));
    let control = inst.call(
        "surface.read_since_mark",
        serde_json::json!({ "surface_id": sid, "strip_ansi": true }),
    );
    let control = control["text"].as_str().unwrap_or("");
    assert!(
        control.contains("overlay_control_marker"),
        "비영 대조 실패 — settings 를 닫은 뒤에도 타이핑이 터미널에 안 닿는다. \
         그러면 위 초록은 settings 가 막았다는 증거가 아니다. Got: {control}"
    );
}

#[test]
#[ignore]
fn test_keyboard_sent_to_terminal_when_no_overlay() {
    let mut inst = shared();

    // Create a test workspace so we have a clean terminal
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Wait for shell to be ready
    std::thread::sleep(Duration::from_millis(500));

    // ★ mark/read 의 `surface_id` 는 필수다(위 형제 시험의 주석 참조). 여기서는 잡는
    // **시점**이 중요하다 — workspace 를 만든 **뒤**에 잡아야 그 새 터미널을 가리킨다.
    // 앞에서 잡으면 직전 workspace 의 터미널이라 이 시험이 재려는 대상이 아니고,
    // `first_surface_id()` 도 같은 이유로 쓸 수 없다(`surface.list` 의 첫 항목이다).
    let sid = inst
        .debug_focused_surface()
        .expect("새 workspace 의 포커스된 surface");

    // Set mark
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    // Type some text
    inst.type_text("echo gui_test_marker");
    inst.press_key(Key::Return);

    // Wait for the output to appear
    std::thread::sleep(Duration::from_millis(1000));

    let result = inst.call(
        "surface.read_since_mark",
        serde_json::json!({ "surface_id": sid, "strip_ansi": true }),
    );
    let output = result["text"].as_str().unwrap_or("");
    assert!(
        output.contains("gui_test_marker"),
        "Terminal should receive keyboard input when no overlay is open. Got: {}",
        output,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ============================================================
// Settings Window Interaction Tests
// ============================================================

#[test]
#[ignore]
fn test_settings_window_is_interactive() {
    let mut inst = shared();

    // 여닫기를 빠르게 반복해도 창이 계속 살아나는가. 닫기는 요청 경로로 한다 —
    // `Ctrl+,` 는 토글이 아니라서 두 번째 누름이 아무 일도 안 한다.
    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    std::thread::sleep(Duration::from_millis(300));

    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });

    // Open again
    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up again", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    // Verify still responsive
    let state = inst.ui_state();
    assert!(
        state.settings_modal_is_up(),
        "settings should still be open after the open/close round trip"
    );

    // Cleanup
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

// ============================================================
// Combined Workflow Tests
// ============================================================

#[test]
#[ignore]
fn test_full_workflow_workspace_pane_tab() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Start: 1 pane, 1 tab in new workspace
    let state = inst.ui_state();
    assert_eq!(state.pane_count, 1);
    assert_eq!(state.tab_count, 1);

    // Create new tab
    inst.press_ctrl_shift(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    // Split pane
    inst.press_alt(Key::Unicode('e'));
    inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);

    // Create another workspace
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws+1", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 2
    });

    // Switch back to previous workspace
    let prev_ws_key = char::from_digit((initial_ws + 1) as u32, 10).unwrap();
    inst.press_alt(Key::Unicode(prev_ws_key));
    inst.wait_for_ui("switch back", Duration::from_secs(3), |s| {
        s.active_workspace == initial_ws
    });

    // Verify workspace still has 2 panes
    let state = inst.ui_state();
    assert_eq!(state.pane_count, 2, "workspace should still have 2 panes");

    // Close pane
    inst.press_ctrl_shift(Key::Unicode('w'));
    inst.wait_for_ui("1 pane", Duration::from_secs(3), |s| s.pane_count == 1);

    // Close tab
    inst.press_ctrl(Key::Unicode('w'));
    inst.wait_for_ui("1 tab", Duration::from_secs(3), |s| s.tab_count == 1);

    // Cleanup: close both test workspaces
    // Close current workspace
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws-1", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });
    // Close the other test workspace (now active)
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws restored", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ============================================================
// Performance / Speed Tests
// ============================================================

#[test]
#[ignore]
fn test_settings_open_speed_repeated() {
    let mut inst = shared();

    // ★ **열기 지연만** 담는다. 예전 이름은 `..._toggle_speed_repeated` 였고 여닫기를 한
    // 통에 섞었는데, 두 가지가 틀렸다: `Ctrl+,` 는 토글이 아니라 두 번째 누름이 아무
    // 일도 안 했고(그래서 닫기 쪽은 실제로 잰 적이 없다), 지금 닫는 손은 IPC 라 재는
    // 것이 사용자 지연이 아니다. 성질이 다른 값을 한 통에 넣으면 평균도 최댓값도
    // 무엇의 값인지 말하지 못한다.
    let mut latencies = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        inst.press_ctrl(Key::Unicode(','));
        inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
            s.settings_modal_is_up()
        });
        latencies.push(start.elapsed());

        inst.close_active_modal();
        inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
            !s.settings_modal_is_up()
        });
    }

    let avg_ms = latencies.iter().map(|d| d.as_millis()).sum::<u128>() / latencies.len() as u128;
    let max_ms = latencies.iter().map(|d| d.as_millis()).max().unwrap_or(0);

    println!(
        "Settings open: avg={}ms, max={}ms over {} iterations",
        avg_ms,
        max_ms,
        latencies.len()
    );

    assert!(
        max_ms < MAX_UI_RESPONSE_MS,
        "Settings open max latency {}ms exceeds {}ms limit",
        max_ms,
        MAX_UI_RESPONSE_MS,
    );
}

#[test]
#[ignore]
fn test_workspace_switch_speed() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let ws1_key = char::from_digit(initial_ws as u32 + 1, 10).unwrap();
    let ws0_key = char::from_digit(initial_ws as u32, 10).unwrap_or('1');

    let mut latencies = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        inst.press_alt(Key::Unicode(ws0_key));
        inst.wait_for_ui("ws 0", Duration::from_secs(3), |s| {
            s.active_workspace == (initial_ws.saturating_sub(1))
        });
        latencies.push(start.elapsed());

        let start = Instant::now();
        inst.press_alt(Key::Unicode(ws1_key));
        inst.wait_for_ui("ws 1", Duration::from_secs(3), |s| {
            s.active_workspace == initial_ws
        });
        latencies.push(start.elapsed());
    }

    let avg_ms = latencies.iter().map(|d| d.as_millis()).sum::<u128>() / latencies.len() as u128;
    let max_ms = latencies.iter().map(|d| d.as_millis()).max().unwrap_or(0);

    println!(
        "Workspace switch: avg={}ms, max={}ms over {} iterations",
        avg_ms,
        max_ms,
        latencies.len()
    );

    assert!(
        max_ms < MAX_UI_RESPONSE_MS,
        "Workspace switch max latency {}ms exceeds {}ms limit",
        max_ms,
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_tab_switch_speed() {
    let mut inst = shared();

    // Create a test workspace
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Create a second tab
    inst.press_ctrl_shift(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    // ★ 고정 sleep 이 아니라 **관측**을 잰다. sleep 을 재면 그 값은 늘 sleep 길이라
    // Ctrl+Tab 이 아무 일도 안 해도 상한 아래로 통과한다 — 형제
    // `test_workspace_switch_speed` 가 처음부터 옳은 형태(`wait_for_ui`)를 갖고 있다.
    // 관측 축은 `active_tab` 이다: `tab_count` 는 전환해도 안 변해서 못 쓴다.
    let mut latencies = Vec::new();
    for _ in 0..5 {
        let from = inst.ui_state().active_tab;
        let start = Instant::now();
        inst.press_ctrl(Key::Tab);
        inst.wait_for_ui("tab forward", Duration::from_secs(3), move |s| {
            s.active_tab != from
        });
        latencies.push(start.elapsed());

        let from = inst.ui_state().active_tab;
        let start = Instant::now();
        inst.press_ctrl_shift(Key::Tab);
        inst.wait_for_ui("tab back", Duration::from_secs(3), move |s| {
            s.active_tab != from
        });
        latencies.push(start.elapsed());
    }

    let avg_ms = latencies.iter().map(|d| d.as_millis()).sum::<u128>() / latencies.len() as u128;
    let max_ms = latencies.iter().map(|d| d.as_millis()).max().unwrap_or(0);

    println!(
        "Tab switch: avg={}ms, max={}ms over {} iterations",
        avg_ms,
        max_ms,
        latencies.len()
    );

    assert!(
        max_ms < MAX_UI_RESPONSE_MS,
        "Tab switch max latency {}ms exceeds {}ms limit",
        max_ms,
        MAX_UI_RESPONSE_MS,
    );

    // Cleanup
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ============================================================
// IME composition + shortcut flush/clear (hybrid: IPC preedit + enigo shortcut)
// ============================================================
//
// handle_keyboard_input 6단계의 flush/clear 분기를 검출한다. 진짜 OS IME 조합
// 이벤트열은 enigo(SendInput/KEYEVENTF_UNICODE)가 OS IME 를 우회하므로 자동 재현
// 불가하지만, "조합 중(preedit 존재) 상태에서 단축키를 누른" 상황은 하이브리드로
// 재현된다: preedit 는 IPC(surface.ime_preedit)로 윈도우 state 에 직접 세팅하고,
// 단축키만 enigo 실입력으로 handle_keyboard_input 을 태운다.
//
// 분기 조건은 `handle_shortcut` 소비 직후 `popups.has_focused()`.
//   flush = preedit.text 를 PTY 로 확정 전송 (팝업 포커스 없음).
//   clear = preedit 폐기, PTY 미전송 (팝업이 포커스를 가짐).
// dispatch_intent 는 큐잉(지연 적용)이라, intent 로 여는 팝업(command palette/
// notifications)은 이 체크 시점에 아직 focused 가 아니다 → flush 로 떨어진다.
// 동기적으로 has_focused()==true 가 되는 유일한 경로는 `find` 가 "이미 열려 있으나
// 비포커스인" search_bar 를 set_focused 로 재포커스하는 경우다. 아래 clear 테스트가
// 정확히 그 상태(ctrl+f 로 열고 → 터미널 클릭으로 unfocus)를 구성한다.

/// flush 경로: 조합 중 팝업을 열지 않는 단축키(sidebar collapse)를 누르면
/// preedit 이 PTY 로 확정 전송된다.
#[test]
#[ignore]
fn test_ime_preedit_flushed_on_non_popup_shortcut() {
    let mut inst = shared();
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });
    std::thread::sleep(Duration::from_millis(500)); // shell ready

    // 포커스된 터미널에 preedit "한" 을 IPC 로 세팅 (응답에 surface_id 포함).
    let pre = inst.call("surface.ime_preedit", serde_json::json!({ "text": "한" }));
    let sid = pre["surface_id"]
        .as_u64()
        .expect("preedit response surface_id");
    assert_eq!(pre["preedit_active"], serde_json::json!(true));

    // preedit 은 화면에 에코되지 않으므로, mark 이후 read 는 단축키가 보낸 것만 잡는다.
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    // 팝업을 열지 않는 소비형 단축키(ctrl+b = toggle_sidebar_collapse) → flush.
    inst.press_ctrl(Key::Unicode('b'));
    std::thread::sleep(Duration::from_millis(500));

    let out = inst.call(
        "surface.read_since_mark",
        serde_json::json!({ "surface_id": sid, "strip_ansi": true }),
    );
    let text = out["text"].as_str().unwrap_or("");
    assert!(
        text.contains("한"),
        "flush: 조합 중 문자는 단축키 시 PTY 로 확정 전송돼야 한다. Got: {text:?}"
    );
    let status = inst.call("surface.ime_status", serde_json::json!({}));
    assert_eq!(
        status["has_preedit"],
        serde_json::json!(false),
        "flush 후 preedit 은 비워져야 한다"
    );

    // Cleanup: 사이드바 원복 + 워크스페이스 닫기.
    inst.press_ctrl(Key::Unicode('b'));
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

/// clear 경로: 조합 중 "팝업을 포커스시키는" 단축키를 누르면 preedit 이 폐기되고
/// PTY 로 전송되지 않는다. 동기 재포커스가 일어나도록 search_bar 를 열고 → 터미널
/// 클릭으로 unfocus 한 뒤(닫히지 않음) → find 를 다시 눌러 set_focused 를 태운다.
#[test]
#[ignore]
fn test_ime_preedit_cleared_on_popup_focus_shortcut() {
    let mut inst = shared();
    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });
    std::thread::sleep(Duration::from_millis(500));

    // search_bar 를 연다(포커스됨). 이어 터미널을 클릭해 unfocus (열린 채 유지 —
    // search_bar 는 close_on_outside_click=false, sticky_focus=false).
    inst.press_ctrl(Key::Unicode('f'));
    std::thread::sleep(Duration::from_millis(300));
    let (w, h) = inst.client_size();
    inst.click_at(w / 2, h * 3 / 4); // 상단 앵커 search_bar 를 피해 터미널 영역 클릭
    std::thread::sleep(Duration::from_millis(200));

    // ★★ **비영 대조 ① — 전제를 잰다.** 아래 단정 둘은 "PTY 로 안 갔다" 와 "preedit 이
    // 없다" 로 **둘 다 부정**이라, 이 시험이 재현하려는 국면이 애초에 서지 않아도 통과한다.
    // search_bar 가 안 열렸으면 재포커스가 없고 clear 경로는 돌지도 않는다.
    let popups = inst.call("debug.host_popup.list", serde_json::json!({}));
    let search_open = popups["popups"].as_array().is_some_and(|list| {
        list.iter().any(|p| {
            p["id"] == serde_json::json!("search_bar") && p["open"] == serde_json::json!(true)
        })
    });
    assert!(
        search_open,
        "비영 대조 실패 — search_bar 가 열리지 않았다. clear 경로가 돌지 않았으므로 \
         아래 초록은 '전송을 막았다' 가 아니라 '보낼 것이 없었다' 다: {popups}"
    );
    // ★ 전제의 나머지 절반(search_bar 가 **비포커스**인가)에는 채널이 없다 —
    // `debug.host_popup.list` 는 `open`·`z_seq`·`rect` 만 내고 포커스는 안 낸다.
    // 없는 것을 있는 척하지 않고 여기 적어 둔다. 그 절반이 어긋나면 이 시험은 여전히
    // 조용히 다른 국면을 재게 되고, 그때 드러날 자리는 이 파일이 아니다.

    // 이제 터미널 포커스 + search_bar 열림·비포커스. preedit + mark 세팅.
    let pre = inst.call("surface.ime_preedit", serde_json::json!({ "text": "한" }));
    let sid = pre["surface_id"]
        .as_u64()
        .expect("preedit response surface_id");
    // 비영 대조 ② — 조합이 실제로 섰는가. 형제 시험 `..._flushed_...` 는 이 단정을
    // 갖고 있는데 이쪽만 빠져 있었다. 안 섰으면 "PTY 로 안 갔다" 는 당연한 참이다.
    assert_eq!(
        pre["preedit_active"],
        serde_json::json!(true),
        "비영 대조 실패 — preedit 이 서지도 않았다. 아래 초록은 clear 경로의 증거가 아니다"
    );
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    // find 재입력 → 열린 search_bar 를 동기 재포커스 → has_focused()==true → clear.
    inst.press_ctrl(Key::Unicode('f'));
    std::thread::sleep(Duration::from_millis(500));

    let out = inst.call(
        "surface.read_since_mark",
        serde_json::json!({ "surface_id": sid, "strip_ansi": true }),
    );
    let text = out["text"].as_str().unwrap_or("");
    assert!(
        !text.contains("한"),
        "clear: 조합 중 문자는 팝업 포커스 단축키 시 PTY 로 가면 안 된다. Got: {text:?}"
    );
    let status = inst.call("surface.ime_status", serde_json::json!({}));
    assert_eq!(status["has_preedit"], serde_json::json!(false));

    // Cleanup: search_bar 닫기 + 워크스페이스 닫기.
    inst.press_key(Key::Escape);
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// ═══════════════════ mouse-routing injection net (구 mouse_routing_tests.rs 병합 — 테스트 다이어트) ═══════════════════
// Mouse-routing injection regression net for `handle_mouse_input`.
//
// `handle_mouse_input` 의 좌표/라우팅 결정 수학은 이미 순수 함수(단위테스트)로
// 격리돼 있다. 이 net 이 잡는 것은 순수테스트가 못 잡는 부분 — "블록→메서드
// 추출이 분기 순서·가드·early-return 을 보존했는가" 하는 stateful 라우팅이다.
//
// 실제 데스크톱 마우스를 뺏지 않고 IPC(`debug.inject_window_mouse`)로 winit
// 레벨 포인터 이벤트를 주입해 실제 `handle_mouse_input` 을 헤드리스 구동하고,
// read-only debug IPC(`debug.selection`/`debug.pending_menu`/`debug.focused_surface`)
// 로 라우팅 결과를 단언한다 (원칙 1·3: 사용자 입력 재현은 debug 격리).
//
// 대상은 focused 테스트 윈도우의 **active workspace** surface 다 — 주입 좌표가
// 보이는 레이아웃에 닿아야 `surface_rect_by_id` 가 해소되기 때문. IPC 로 만든
// workspace 는 active 전환이 없으므로(포커스 독립) 여기서는 쓰지 않는다.
//
// Run with: cargo test --test gui_tests -- --ignored --test-threads=1
// (display 필요, single-thread — 한 윈도우만 OS 포커스를 가질 수 있으므로.)

/// 주입 후 GUI 상태가 정착할 시간. inject IPC 자체는 동기지만 여유를 둔다.
fn settle() {
    std::thread::sleep(Duration::from_millis(150));
}

/// (a) click-to-activate: 비활성 surface 를 좌클릭(press)하면 포커스가 그 surface
/// 로 전환된다. surface-level split 으로 active workspace 에 2 번째 surface 를 만든
/// 뒤(IPC split 은 focus 미이동), 비활성 surface 중앙을 press+release 한다.
#[test]
#[ignore]
fn click_to_activate_moves_focus() {
    let inst = shared();

    let focused_before = inst
        .debug_focused_surface()
        .expect("an initially focused surface");

    // active workspace 에 2 번째 surface 생성 (같은 pane 내 surface split).
    let res = inst.call(
        "split",
        json!({
            "level": "surface",
            "direction": "vertical",
            "target_surface": focused_before,
        }),
    );
    let new_sid = res["new_surface_id"]
        .as_u64()
        .expect("split should return new_surface_id");
    settle();

    // IPC split 은 focus 를 옮기지 않는다 — 새 surface 는 비활성.
    assert_ne!(
        inst.debug_focused_surface(),
        Some(new_sid),
        "IPC split must not move focus (focus independence)"
    );

    // 비활성 surface 중앙을 좌클릭 → click-to-activate 가 포커스를 전환.
    inst.inject_mouse(new_sid, 0.5, 0.5, "press", 0);
    inst.inject_mouse(new_sid, 0.5, 0.5, "release", 0);
    settle();

    assert_eq!(
        inst.debug_focused_surface(),
        Some(new_sid),
        "click-to-activate should move focus to the clicked surface"
    );

    // cleanup: 생성한 surface 정리.
    inst.call("surface.close", json!({ "surface_id": new_sid }));
    settle();
}

/// (b) 로컬 드래그 선택: 트래킹 OFF 터미널에서 press→move→move→release 하면
/// 로컬 텍스트 선택이 생긴다 (start≠end, 드래그 종료 후 dragging=false).
#[test]
#[ignore]
fn drag_creates_local_selection() {
    let inst = shared();

    let sid = inst
        .debug_focused_surface()
        .expect("a focused terminal surface");

    inst.inject_mouse(sid, 0.3, 0.3, "press", 0);
    inst.inject_mouse(sid, 0.5, 0.4, "move", 0);
    inst.inject_mouse(sid, 0.7, 0.6, "move", 0);
    inst.inject_mouse(sid, 0.7, 0.6, "release", 0);
    settle();

    let sel = inst.debug_selection();
    assert_eq!(sel["present"], json!(true), "selection should be present");
    assert_eq!(
        sel["empty"],
        json!(false),
        "drag selection should not be empty"
    );
    assert_eq!(
        sel["dragging"],
        json!(false),
        "dragging must be false after release"
    );
    assert_eq!(
        sel["surface_id"].as_u64(),
        Some(sid),
        "selection surface_id should match the injected surface"
    );
    // 드래그가 실제로 범위를 만들었는지 — start != end.
    assert_ne!(
        (sel["start"]["col"].clone(), sel["start"]["row"].clone()),
        (sel["end"]["col"].clone(), sel["end"]["row"].clone()),
        "selection start and end should differ"
    );
}

/// (c) 우클릭 컨텍스트 메뉴: 트래킹 OFF 터미널을 우클릭(press)하면 tasty
/// 터미널 컨텍스트 메뉴가 대기 상태로 세워진다 (kind=TerminalSurface).
#[test]
#[ignore]
fn right_click_opens_terminal_menu() {
    let inst = shared();

    let sid = inst
        .debug_focused_surface()
        .expect("a focused terminal surface");

    inst.inject_mouse(sid, 0.5, 0.5, "press", 2);
    // ★ release 까지 줘야 한다. Linux 는 컨텍스트 메뉴를 **`Released` 에서** 연다 —
    // 버튼이 눌린 채로 GTK `popup_at_rect` 를 부르면 팝업을 realize/map 하지 않고
    // 조용히 no-op 하기 때문이다(`src/view/main/mouse.rs` 의 `terminal_menu_open_state`).
    // 그래서 press 만 주는 것은 제품이 옳은데도 실패하는 자극이다.
    // macOS/Windows 는 press 에서 열지만, 그쪽에서도 release 를 더 주는 것은 무해하다
    // (메뉴는 press 에서 이미 서고 release 는 그 뒤에 온다).
    inst.inject_mouse(sid, 0.5, 0.5, "release", 2);
    settle();

    let menu = inst.debug_pending_menu();
    assert_eq!(
        menu["present"],
        json!(true),
        "a context menu should be pending"
    );
    assert_eq!(
        menu["kind"],
        json!("TerminalSurface"),
        "right-click on a terminal should open the TerminalSurface menu"
    );
    assert_eq!(
        menu["surface_id"].as_u64(),
        Some(sid),
        "menu surface_id should match the injected surface"
    );
}

/// (d) explorer 우클릭은 표면 어디서든 explorer 메뉴가 뜨고, generic surface
/// fallback("터미널 ID 복사")이 새지 않는다. 그리드 콘텐츠뿐 아니라 chrome
/// (툴바/내부 탭바/상태줄/빈 사이드바)까지 `draw_explorer` 의 표면 전체 catch-all 이
/// Empty 메뉴로 흡수하는 회귀를 잡는다(불가침 원칙 §1·§2). 좌표는 surface 상대
/// 정규화라 창 크기와 무관. egui 경로 주입(`inject_egui_mouse`)으로 위젯
/// `secondary_clicked` 라우팅을 그대로 탄다.
#[test]
#[ignore]
fn right_click_explorer_never_falls_back_to_surface_menu() {
    let inst = shared();

    // active workspace 의 pane 에 grid explorer 를 만들어 활성 탭으로 렌더시킨다.
    let pane_id = inst.first_pane_id();
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
    let mut params = json!({ "pane_id": pane_id, "type": "explorer", "view_mode": "grid" });
    if let Ok(h) = home {
        params["path"] = json!(h);
    }
    let created = inst.call("tab.create", params);
    let sid = created["surface_id"]
        .as_u64()
        .expect("tab.create should return the explorer surface_id");
    // 에이전트가 만든 탭은 사용자 선택을 안 바꾸므로(ADR-0502) 사용자의 탭 전환을 재현해
    // 렌더시킨다. 새 탭은 뒤에 붙으므로 마지막 인덱스다.
    let last = created["tab_count"]
        .as_u64()
        .expect("tab.create should return tab_count")
        - 1;
    inst.call("debug.switch_tab", json!({ "index": last }));
    settle();

    // surface 상대 좌표(fx,fy ∈ [0,1]): 이전에 surface fallback 이 새던 chrome 영역들 +
    // 콘텐츠 그리드. 전부 explorer 메뉴여야 한다.
    let spots = [
        (0.5_f32, 0.30_f32, "content grid"),
        (0.5, 0.02, "internal tab bar / toolbar (top)"),
        (0.5, 0.99, "status line (bottom)"),
        (0.02, 0.60, "left sidebar empty area"),
    ];
    for (fx, fy, label) in spots {
        inst.inject_egui_mouse(sid, fx, fy, "move", 2);
        inst.inject_egui_mouse(sid, fx, fy, "press", 2);
        inst.inject_egui_mouse(sid, fx, fy, "release", 2);
        settle();
        let menu = inst.debug_pending_menu();
        assert_eq!(
            menu["present"],
            json!(true),
            "explorer right-click at {label} should set a pending menu"
        );
        assert_eq!(
            menu["kind"],
            json!("Explorer"),
            "explorer right-click at {label} must open the Explorer menu, \
             never the generic Surface fallback"
        );
        assert_eq!(
            menu["surface_id"].as_u64(),
            Some(sid),
            "menu surface_id at {label} should match the explorer surface"
        );
    }
}

/// 트래킹을 **이 모드 하나로** 맞추고 수신 바이트를 화면에 노출시킨다(`cat -v` 로 ESC 가
/// `^[` 로 보인다). 셸 프롬프트가 자리를 잡을 시간을 준 뒤 mark 를 찍는 것은 호출자 몫이다.
///
/// ★ 켜기 전에 셋을 **끈다.** 1000/1002/1003 은 서로를 대체하지 않는 **독립 레지스터**이고
/// 실효 레벨은 켜진 것 중 **가장 넓은 것**이다(`MouseTrackingRegisters::effective`).
/// 끄지 않으면 1003 뒤에 1002 를 요청해도 AllMotion 이 남아, "1002 는 버튼 없는 hover 를
/// 보고하지 않는다" 같은 단정이 **제품이 옳은데도** 실패한다. 이름이 `enable_` 이라
/// 호출자는 "이 모드가 된다" 로 읽는다 — 그 이름이 참이 되게 헬퍼가 맞춘다.
/// 호출자가 다섯이라 자리마다 끄는 줄을 넣는 것이 아니라 여기서 한 번 한다.
#[cfg(test)]
fn enable_mouse_tracking(inst: &gui_common::GuiTestInstance, sid: u64, mode: &str) {
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": format!(
            "printf '\\e[?1000l\\e[?1002l\\e[?1003l\\e[?{mode}h\\e[?1006h'; cat -v\n"
        ) }),
    );
    std::thread::sleep(Duration::from_millis(400));
}

/// mark 이후 그 surface 가 화면에 뱉은 텍스트.
#[cfg(test)]
fn read_since_mark(inst: &gui_common::GuiTestInstance, sid: u64) -> String {
    let res = inst.call(
        "surface.read_since_mark",
        json!({ "surface_id": sid, "strip_ansi": true }),
    );
    res["text"].as_str().unwrap_or("").to_string()
}

/// (f) 1003(AnyEventMouse)은 버튼 없는 hover 를 셀 단위로 보고한다(`ESC[<35;col;rowM`).
/// 1002 는 같은 이동에 아무것도 내보내지 않는다 — 두 모드가 드래그에서만 같고 hover
/// 에서 갈린다는 계약을 고정한다.
#[test]
#[ignore]
fn hover_motion_reported_only_for_mode_1003() {
    let inst = shared();
    inst.focus();
    let sid = inst.first_surface_id();

    enable_mouse_tracking(&inst, sid, "1003");
    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    // 버튼 없이 가로지른다 — 여러 셀을 넘으므로 보고가 여러 번 나가야 한다.
    for fx in [0.30_f32, 0.45, 0.60, 0.75] {
        inst.inject_mouse(sid, fx, 0.5, "move", 0);
    }
    settle();
    let out = read_since_mark(&inst, sid);
    assert!(
        out.contains("^[[<35;"),
        "1003 hover should emit cb=35 motion reports, got: {out:?}"
    );

    // 같은 셀 안에서의 미세 이동은 dedup 으로 삼켜진다.
    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    inst.inject_mouse(sid, 0.75, 0.5, "move", 0);
    settle();
    assert!(
        !read_since_mark(&inst, sid).contains("^[[<35;"),
        "same-cell motion must be deduped"
    );

    // 1002 로 바꾸면 hover 는 사라진다(드래그 motion 만 남는다).
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{3}" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    enable_mouse_tracking(&inst, sid, "1002");
    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    for fx in [0.30_f32, 0.45, 0.60] {
        inst.inject_mouse(sid, fx, 0.5, "move", 0);
    }
    settle();
    let out = read_since_mark(&inst, sid);
    assert!(
        !out.contains("^[[<35;"),
        "1002 must not report button-less hover, got: {out:?}"
    );
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{3}" }),
    );
}

/// (g) 확정 정책(ADR-0081): hover 는 focused surface 에만 간다. 비포커스 surface 위를
/// 지나가도 바이트가 나가지 않고 **포커스도 바뀌지 않는다**.
#[test]
#[ignore]
fn hover_motion_never_reaches_a_non_focused_surface() {
    let inst = shared();
    inst.focus();
    let focused = inst
        .debug_focused_surface()
        .expect("an initially focused surface");

    let res = inst.call(
        "split",
        json!({ "level": "surface", "direction": "vertical", "target_surface": focused }),
    );
    let other = res["new_surface_id"].as_u64().expect("new_surface_id");
    settle();

    // ★★ **비영 대조.** 이 시험의 단정은 둘 다 "아무 일도 안 일어났다" 형태다 — 바이트가
    // 안 나왔다(부정)와 포커스가 안 바뀌었다(형태는 긍정이지만 내용은 무변화). 주입이
    // 무효과면 둘 다 그대로 통과하므로, 그 초록은 정책 준수가 아니라 **자극 부재**다.
    // 실측 2026-09-06: 주입이 죽은 슬롯의 gui 발사에서 이 시험이 통과한 소수에 들어 있었고,
    // 같은 원인으로 형제 `hover_motion_reported_only_for_mode_1003` 은 빨갰다.
    // 그래서 **같은 주입이 살아 있다는 것을 이 시험 안에서 먼저 보인다.**
    enable_mouse_tracking(&inst, focused, "1003");
    inst.call("surface.set_mark", json!({ "surface_id": focused }));
    for fx in [0.30_f32, 0.50, 0.70] {
        inst.inject_mouse(focused, fx, 0.5, "move", 0);
    }
    settle();
    let control = read_since_mark(&inst, focused);
    assert!(
        control.contains("^[[<35;"),
        "비영 대조 실패 — 포커스된 surface 에서조차 hover 보고가 안 나온다. 주입 경로가 \
         죽었다는 뜻이고, 그러면 아래 '비포커스로 안 샌다' 는 정책의 증거가 못 된다. \
         got: {control:?}"
    );
    // 대조가 끝났으면 그 surface 의 모드를 되돌린다 — 공유 인스턴스라 켜둔 채로 나가면
    // 뒤 시험이 남의 상태를 물려받는다. Ctrl-C 는 `cat -v` 만 죽이고 모드는 안 되돌린다.
    inst.call(
        "surface.send",
        json!({ "surface_id": focused, "text": "\u{3}" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    inst.call(
        "surface.send",
        json!({ "surface_id": focused, "text": "printf '\\e[?1003l\\e[?1006l'\n" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    inst.call("surface.set_mark", json!({ "surface_id": focused }));

    // 비활성 surface 에서 1003 을 켠다 — 앱은 트래킹 중이라고 믿는 상태.
    enable_mouse_tracking(&inst, other, "1003");
    inst.call("surface.set_mark", json!({ "surface_id": other }));
    for fx in [0.30_f32, 0.50, 0.70] {
        inst.inject_mouse(other, fx, 0.5, "move", 0);
    }
    settle();

    assert!(
        !read_since_mark(&inst, other).contains("^[[<35;"),
        "hover must not leak into a non-focused surface"
    );
    assert_eq!(
        inst.debug_focused_surface(),
        Some(focused),
        "hover must not move focus"
    );

    inst.call(
        "surface.send",
        json!({ "surface_id": other, "text": "\u{3}" }),
    );
    inst.call("surface.close", json!({ "surface_id": other }));
}

/// (h) 우/미들 드래그도 중간 motion 을 보고하고, cb 의 버튼 비트가 실제 버튼을 담는다.
/// 좌버튼은 기존 값(0/32/0)을 유지해야 한다(회귀).
#[test]
#[ignore]
fn drag_motion_carries_the_pressed_button() {
    let inst = shared();
    inst.focus();
    let sid = inst.first_surface_id();

    enable_mouse_tracking(&inst, sid, "1002");
    for (button, press, motion) in [
        (2_u8, "^[[<2;", "^[[<34;"),
        (1, "^[[<1;", "^[[<33;"),
        (0, "^[[<0;", "^[[<32;"),
    ] {
        inst.call("surface.set_mark", json!({ "surface_id": sid }));
        inst.inject_mouse(sid, 0.30, 0.5, "press", button);
        inst.inject_mouse(sid, 0.45, 0.5, "move", button);
        inst.inject_mouse(sid, 0.60, 0.5, "move", button);
        inst.inject_mouse(sid, 0.60, 0.5, "release", button);
        settle();
        let out = read_since_mark(&inst, sid);
        assert!(
            out.contains(press),
            "button {button} press should be reported, got: {out:?}"
        );
        assert!(
            out.contains(motion),
            "button {button} drag should report motion with its own bits, got: {out:?}"
        );
    }

    // 트래킹 OFF 에서는 우 드래그가 PTY 로 새지 않고 로컬 선택도 건드리지 않는다.
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{3}" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "printf '\\e[?1002l\\e[?1003l'; cat -v\n" }),
    );
    std::thread::sleep(Duration::from_millis(400));
    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    inst.inject_mouse(sid, 0.30, 0.5, "press", 2);
    inst.inject_mouse(sid, 0.50, 0.5, "move", 2);
    inst.inject_mouse(sid, 0.50, 0.5, "release", 2);
    settle();
    assert!(
        !read_since_mark(&inst, sid).contains("^[[<"),
        "tracking off must not report any mouse bytes"
    );
    assert_eq!(
        inst.debug_selection()["present"],
        json!(false),
        "a right drag must not start a local text selection"
    );
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{3}" }),
    );
}

/// 명령 팔레트 붙여넣기도 사용자 입력으로 기록된다 — `surface.is_typing` 이 사람 있음을 말한다.
///
/// 이 경로는 키가 surface 에 닿지 않는다(팔레트는 IPC 로 열고, 쿼리와 Enter 는 egui 층에
/// 주입한다). 그래서 기록은 `run_paste` 자신이 하는 것밖에 없다 — 키보드 Ctrl+V 는 수식키
/// 키다운이 이미 기록해서 이 구멍을 못 잰다.
///
/// ★★ 비영 대조: 붙여넣기 전에 `idle_seconds` 가 3 초를 넘기를 기다린다. 기록이 안 되면
/// 그 값이 그대로 커지므로 아래 `< 3.0` 단정이 빨개진다. 붙여넣기가 실제로 입력창에
/// 들어갔다는 것은 화면의 표지로 따로 단정한다.
#[test]
#[ignore]
fn test_palette_paste_records_user_typing() {
    let inst = shared();
    let sid = inst
        .debug_focused_surface()
        .expect("a focused surface to paste into");
    let is_typing = || inst.call("surface.is_typing", json!({ "surface_id": sid }));

    // 앞 시험의 키 입력이 남긴 기록이 식기를 기다린다(기록이 없으면 -1 이다).
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let idle = is_typing()["idle_seconds"].as_f64().unwrap();
        if !(0.0..=3.0).contains(&idle) {
            break;
        }
        assert!(Instant::now() < deadline, "idle never exceeded 3s: {idle}");
        std::thread::sleep(Duration::from_millis(200));
    }

    let mark = "PALETTE_PASTE_MARK";
    inst.call("clipboard.set_text", json!({ "text": mark }));
    inst.call(
        "debug.host_popup.open",
        json!({ "popup_id": "command_palette" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    inst.call("debug.inject_egui_text", json!({ "text": "paste" }));
    std::thread::sleep(Duration::from_millis(300));
    inst.call("debug.inject_egui_key", json!({ "key": "Enter" }));

    let deadline = Instant::now() + Duration::from_secs(3);
    let screen = loop {
        let text = inst.call("surface.screen_text", json!({ "surface_id": sid }))["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if text.contains(mark) || Instant::now() >= deadline {
            break text;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        screen.contains(mark),
        "palette paste did not reach the input: {screen:?}"
    );

    let after = is_typing();
    let idle = after["idle_seconds"].as_f64().unwrap();
    assert!(
        (0.0..3.0).contains(&idle),
        "palette paste must record user input, got idle_seconds={idle}"
    );
    assert_eq!(after["typing"], json!(true), "got {after}");

    // 정리: 붙여넣은 표지를 입력줄에서 지운다(Ctrl+U).
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{15}" }),
    );
}
