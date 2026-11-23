//! GUI 인스턴스를 공유하는 수동 통합 시험이다. 화면과 키보드 포커스를 사용하므로 기본 실행에서는 제외한다.
//! 격리된 디스플레이에서 cargo test --test gui_tests -- --ignored --test-threads=1로 실행한다.
//! 변화가 없음을 검사할 때는 같은 입력 경로가 실제로 동작하는 대조도 확인한다.
//! 설정 창은 열기 요청 래치 대신 실제 모달 상태와 종류로 기다린다.

mod gui_common;

use enigo::Key;
use gui_common::shared;
use serde_json::json;
use std::time::{Duration, Instant};

/// 입력 헬퍼의 포커스·키 간격 sleep도 측정 시간에 포함한다.
const MAX_UI_RESPONSE_MS: u128 = 1000;

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

#[test]
#[ignore]
fn test_settings_open_ctrl_comma() {
    let mut inst = shared();

    let state = inst.ui_state();
    assert!(
        !state.settings_modal_is_up(),
        "settings should be closed initially"
    );

    inst.press_ctrl(Key::Unicode(','));

    let state = inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });
    assert!(
        state.settings_modal_is_up(),
        "settings should be open after Ctrl+,"
    );

    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

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

/// 설정 토글 바인딩으로 닫히고 Escape로는 닫히지 않는 현재 동작을 확인한다.
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

    // 열기 지연만 측정하므로 정리는 별도 닫기 요청으로 한다.
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

#[test]
#[ignore]
fn test_notification_panel_toggle() {
    let mut inst = shared();

    let state = inst.ui_state();
    assert!(
        !state.notification_panel_open,
        "notification panel should be closed initially"
    );

    inst.press_ctrl_shift(Key::Unicode('i'));

    let state = inst.wait_for_ui("notification panel open", Duration::from_secs(3), |s| {
        s.notification_panel_open
    });
    assert!(state.notification_panel_open);

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

    inst.press_ctrl_shift(Key::Unicode('i'));
    inst.wait_for_ui("notification open", Duration::from_secs(3), |s| {
        s.notification_panel_open
    });

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

    inst.press_ctrl_shift(Key::Unicode('i'));
    inst.wait_for_ui("notification closed", Duration::from_secs(3), |s| {
        !s.notification_panel_open
    });
}

#[test]
#[ignore]
fn test_new_workspace_ctrl_shift_n() {
    let mut inst = shared();
    let initial_count = inst.ui_state().workspace_count;

    inst.press_alt(Key::Unicode('n'));

    let state = inst.wait_for_ui("workspace count increased", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count + 1
    });
    assert_eq!(state.workspace_count, initial_count + 1);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("workspace closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count
    });
}

/// Select an exact fixture workspace outside shortcut latency measurements.
fn select_workspace_fixture(inst: &gui_common::GuiTestInstance, index: usize) {
    assert!(
        index < inst.ui_state().workspace_count,
        "fixture workspace must exist"
    );
    inst.call("debug.switch_workspace", json!({"index":index}));
    inst.wait_for_ui("fixture workspace selected", Duration::from_secs(3), |s| {
        s.active_workspace == index
    });
}

#[test]
#[ignore]
fn test_workspace_switch_alt_number() {
    let mut inst = shared();

    let initial_count = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("new ws", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count + 1
    });

    let state = inst.ui_state();
    let new_ws_idx = state.active_workspace;

    inst.press_alt(Key::Unicode('1'));
    inst.wait_for_ui("switch to ws 0", Duration::from_secs(3), |s| {
        s.active_workspace == 0
    });

    // Number shortcuts address the first nine entries, not every accumulated workspace.
    inst.press_alt(Key::Unicode('2'));
    inst.wait_for_ui("switch to ws 1", Duration::from_secs(3), |s| {
        s.active_workspace == 1
    });
    select_workspace_fixture(&inst, new_ws_idx);

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

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_count
    });
}

#[test]
#[ignore]
fn test_new_tab_alt_t() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let state = inst.ui_state();
    assert_eq!(state.tab_count, 1, "new workspace should start with 1 tab");

    inst.press_alt(Key::Unicode('t'));
    let state = inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);
    assert_eq!(state.tab_count, 2);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_close_tab_ctrl_w() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    inst.press_alt(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    inst.press_ctrl(Key::Unicode('w'));
    let state = inst.wait_for_ui("1 tab", Duration::from_secs(3), |s| s.tab_count == 1);
    assert_eq!(state.tab_count, 1);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_tab_creation_speed() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let elapsed = measure_ui_latency(
        &mut inst,
        "tab creation speed",
        |i| i.press_alt(Key::Unicode('t')),
        |s| s.tab_count == 2,
    );

    println!("Tab creation latency: {}ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < MAX_UI_RESPONSE_MS,
        "Tab creation took {}ms, exceeds {}ms limit",
        elapsed.as_millis(),
        MAX_UI_RESPONSE_MS,
    );

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_pane_split_vertical_ctrl_shift_e() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    assert_eq!(inst.ui_state().pane_count, 1);

    inst.press_alt(Key::Unicode('e'));
    let state = inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);
    assert_eq!(state.pane_count, 2);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_pane_split_horizontal_ctrl_shift_o() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    inst.press_alt_shift(Key::Unicode('e'));
    let state = inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);
    assert_eq!(state.pane_count, 2);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_close_pane_ctrl_shift_w() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    inst.press_alt(Key::Unicode('e'));
    inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);

    inst.press_ctrl_shift(Key::Unicode('w'));
    let state = inst.wait_for_ui("1 pane", Duration::from_secs(3), |s| s.pane_count == 1);
    assert_eq!(state.pane_count, 1);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_pane_split_speed() {
    let mut inst = shared();

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

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_keyboard_not_sent_to_terminal_when_settings_open() {
    let mut inst = shared();

    // 목록의 첫 서피스가 활성 터미널이라는 보장이 없어 설정을 열기 전에 포커스된 ID를 얻는다.
    let sid = inst
        .debug_focused_surface()
        .expect("mark 를 찍을 포커스된 surface");

    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings open", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    // 모달이 입력을 받는 경우와 구별하도록 본창에 직접 입력해 본창의 차단을 확인한다.
    inst.type_text_into_main_window("hello_should_not_appear");
    std::thread::sleep(Duration::from_millis(500));

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

    // Escape로는 설정이 닫히지 않으므로 닫기 요청으로 정리한다.
    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });

    // 입력이 전혀 도달하지 않아도 부정 단정은 통과할 수 있어 모달을 닫은 뒤 같은 경로로 출력도 확인한다.
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
        "설정을 닫은 뒤에도 입력을 확인하지 못했다. 앞의 미전송 결과만으로 모달의 입력 차단을 입증할 수 없다: {control}"
    );
}

#[test]
#[ignore]
fn test_keyboard_sent_to_terminal_when_no_overlay() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    std::thread::sleep(Duration::from_millis(500));

    // 새 워크스페이스를 만든 뒤 포커스된 ID를 얻어 이전 터미널에 입력하지 않도록 한다.
    let sid = inst
        .debug_focused_surface()
        .expect("새 workspace 의 포커스된 surface");

    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

    inst.type_text("echo gui_test_marker");
    inst.press_key(Key::Return);

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

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_settings_window_is_interactive() {
    let mut inst = shared();

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    std::thread::sleep(Duration::from_millis(300));

    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });

    inst.press_ctrl(Key::Unicode(','));
    inst.wait_for_ui("settings modal is up again", Duration::from_secs(3), |s| {
        s.settings_modal_is_up()
    });

    let state = inst.ui_state();
    assert!(
        state.settings_modal_is_up(),
        "settings should still be open after the open/close round trip"
    );

    inst.close_active_modal();
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
}

#[test]
#[ignore]
fn test_full_workflow_workspace_pane_tab() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    let state = inst.ui_state();
    assert_eq!(state.pane_count, 1);
    assert_eq!(state.tab_count, 1);

    inst.press_alt(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    inst.press_alt(Key::Unicode('e'));
    inst.wait_for_ui("2 panes", Duration::from_secs(3), |s| s.pane_count == 2);

    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws+1", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 2
    });

    select_workspace_fixture(&inst, initial_ws);

    let state = inst.ui_state();
    assert_eq!(state.pane_count, 2, "workspace should still have 2 panes");

    inst.press_ctrl_shift(Key::Unicode('w'));
    inst.wait_for_ui("1 pane", Duration::from_secs(3), |s| s.pane_count == 1);

    inst.press_ctrl(Key::Unicode('w'));
    inst.wait_for_ui("1 tab", Duration::from_secs(3), |s| s.tab_count == 1);

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws-1", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws restored", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_settings_open_speed_repeated() {
    let mut inst = shared();

    // 키보드로 여는 시간만 측정한다. 닫기는 IPC로 처리하므로 같은 지연 측정에 섞지 않는다.
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

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    // Measure real Alt+1/Alt+2 transitions even after other shared tests left >9 workspaces.
    select_workspace_fixture(&inst, 1);

    let mut latencies = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        inst.press_alt(Key::Unicode('1'));
        inst.wait_for_ui("ws 0", Duration::from_secs(3), |s| s.active_workspace == 0);
        latencies.push(start.elapsed());

        let start = Instant::now();
        inst.press_alt(Key::Unicode('2'));
        inst.wait_for_ui("ws 1", Duration::from_secs(3), |s| s.active_workspace == 1);
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

    select_workspace_fixture(&inst, initial_ws);
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

#[test]
#[ignore]
fn test_tab_switch_speed() {
    let mut inst = shared();

    let initial_ws = inst.ui_state().workspace_count;
    inst.press_alt(Key::Unicode('n'));
    inst.wait_for_ui("ws created", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws + 1
    });

    inst.press_alt(Key::Unicode('t'));
    inst.wait_for_ui("2 tabs", Duration::from_secs(3), |s| s.tab_count == 2);

    // 기본 quick-switch는 Ctrl+L/H다. 탭 수 대신 active_tab의 실제 변화를 기다린다.
    let mut latencies = Vec::new();
    for _ in 0..5 {
        let from = inst.ui_state().active_tab;
        let start = Instant::now();
        inst.press_ctrl(Key::Unicode('l'));
        inst.wait_for_ui("tab forward", Duration::from_secs(3), move |s| {
            s.active_tab != from
        });
        latencies.push(start.elapsed());

        let from = inst.ui_state().active_tab;
        let start = Instant::now();
        inst.press_ctrl(Key::Unicode('h'));
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

    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// 실제 OS IME 이벤트 대신 IPC로 preedit를 설정하고 단축키는 실제 키 입력으로 보낸다.
// 팝업 포커스로 인한 clear는 열린 search_bar를 비활성화한 뒤 다시 포커스하는 상태로 재현한다.

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

    let pre = inst.call("surface.ime_preedit", serde_json::json!({ "text": "한" }));
    let sid = pre["surface_id"]
        .as_u64()
        .expect("preedit response surface_id");
    assert_eq!(pre["preedit_active"], serde_json::json!(true));

    // preedit 자체는 화면에 에코되지 않아 mark 이후 출력으로 단축키의 전송을 확인한다.
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

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

    inst.press_ctrl(Key::Unicode('b'));
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

/// search_bar를 닫지 않고 포커스만 해제한 뒤 다시 find를 눌러 preedit 폐기를 검사한다.
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

    inst.press_ctrl(Key::Unicode('f'));
    std::thread::sleep(Duration::from_millis(300));
    let (w, h) = inst.client_size();
    inst.click_at(w / 2, h * 3 / 4); // 상단 앵커 search_bar 를 피해 터미널 영역 클릭
    std::thread::sleep(Duration::from_millis(200));

    // 팝업이 열리지 않아도 뒤의 부정 단정이 통과할 수 있어 먼저 열림 상태를 확인한다.
    let popups = inst.call("debug.host_popup.list", serde_json::json!({}));
    let search_open = popups["popups"].as_array().is_some_and(|list| {
        list.iter().any(|p| {
            p["id"] == serde_json::json!("search_bar") && p["open"] == serde_json::json!(true)
        })
    });
    assert!(
        search_open,
        "search_bar가 열리지 않아 preedit 폐기 시나리오의 전제를 만족하지 못했다: {popups}"
    );
    // A refocus shortcut is meaningful only after the existing popup actually lost focus.
    inst.wait_for_ui("search popup unfocused", Duration::from_secs(3), |s| {
        !s.gate_terms.host_popup_focused && !s.keyboard_shortcuts_gated
    });

    let pre = inst.call("surface.ime_preedit", serde_json::json!({ "text": "한" }));
    let sid = pre["surface_id"]
        .as_u64()
        .expect("preedit response surface_id");
    // preedit가 실제로 생기지 않아도 미전송 단정은 통과하므로 설정 결과를 확인한다.
    assert_eq!(
        pre["preedit_active"],
        serde_json::json!(true),
        "preedit 설정이 확인되지 않아 폐기 동작을 검사할 수 없다"
    );
    inst.call("surface.set_mark", serde_json::json!({ "surface_id": sid }));

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

    inst.press_key(Key::Escape);
    inst.press_alt_shift(Key::Unicode('w'));
    inst.wait_for_ui("ws closed", Duration::from_secs(3), |s| {
        s.workspace_count == initial_ws
    });
}

// 포인터는 debug IPC로 winit·egui 입력 경로에 주입한다. GUI 인스턴스와 디스플레이는 필요하다.
// 레이아웃 좌표가 있는 활성 워크스페이스를 사용한다. IPC로 만든 워크스페이스는 자동 활성화되지 않는다.

/// 주입 후 상태 반영을 기다리는 고정 간격이다. 반영 완료를 직접 동기화하지는 않는다.
fn settle() {
    std::thread::sleep(Duration::from_millis(150));
}

#[test]
#[ignore]
fn click_to_activate_moves_focus() {
    let inst = shared();

    let focused_before = inst
        .debug_focused_surface()
        .expect("an initially focused surface");

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

    assert_ne!(
        inst.debug_focused_surface(),
        Some(new_sid),
        "IPC split must not move focus (focus independence)"
    );

    inst.inject_mouse(new_sid, 0.5, 0.5, "press", 0);
    inst.inject_mouse(new_sid, 0.5, 0.5, "release", 0);
    settle();

    assert_eq!(
        inst.debug_focused_surface(),
        Some(new_sid),
        "click-to-activate should move focus to the clicked surface"
    );

    inst.call("surface.close", json!({ "surface_id": new_sid }));
    settle();
}

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
    assert_ne!(
        (sel["start"]["col"].clone(), sel["start"]["row"].clone()),
        (sel["end"]["col"].clone(), sel["end"]["row"].clone()),
        "selection start and end should differ"
    );
}

#[test]
#[ignore]
fn right_click_opens_terminal_menu() {
    let inst = shared();

    let sid = mouse_test_terminal(&inst);

    inst.inject_mouse(sid, 0.5, 0.5, "press", 2);
    // Linux에서는 컨텍스트 메뉴를 release에서 열므로 press와 release를 모두 보낸다.
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

/// 터미널 메뉴의 "이동" 항목 id. 선택 결과를 `debug.pending_move`로 관찰한다.
const TERMINAL_MENU_MOVE_ITEM: u64 = 10;
/// 링크 메뉴의 "선택" 항목 id. 선택 결과를 `debug.selection`으로 관찰한다.
const LINK_MENU_SELECT_ITEM: u64 = 1;

/// 사용자 우클릭처럼 winit 경로로 press와 release를 보낸다. Linux는 release에서 메뉴를 연다.
fn terminal_right_click(inst: &gui_common::GuiTestInstance, sid: u64, fx: f32, fy: f32) {
    inst.inject_mouse(sid, fx, fy, "move", 0);
    inst.inject_mouse(sid, fx, fy, "press", 2);
    inst.inject_mouse(sid, fx, fy, "release", 2);
    settle();
}

#[test]
#[ignore]
fn menu_answer_applies_to_terminal_right_click() {
    let inst = shared();
    let sid = mouse_test_terminal(&inst);

    // 하네스는 포획 env를 켠다. 같은 종류의 응답은 포획보다 먼저 쓰인다.
    let armed = inst.call(
        "debug.native_menu.answer",
        json!({ "menu": "TerminalSurface", "item": TERMINAL_MENU_MOVE_ITEM }),
    );
    assert_eq!(armed["armed"], json!(true), "answer must be armed: {armed}");
    terminal_right_click(&inst, sid, 0.5, 0.5);

    let moving = inst.call("debug.pending_move", json!({}));
    assert_eq!(
        (moving["kind"].clone(), moving["id"].as_u64()),
        (json!("surface"), Some(sid)),
        "the answered Move item must run its selection handler: {moving}"
    );
    let menu = inst.debug_pending_menu();
    assert_eq!(
        menu["present"],
        json!(false),
        "an answered menu must not stay captured: {menu}"
    );

    // 응답은 한 번만 쓰인다. 두 번째 우클릭은 포획 경로로 간다.
    terminal_right_click(&inst, sid, 0.5, 0.5);
    let menu = inst.debug_pending_menu();
    assert_eq!(
        menu["kind"],
        json!("TerminalSurface"),
        "a second right-click without an answer must be captured: {menu}"
    );

    // 대상 surface를 닫으면 이동 대기 슬롯도 비워져 다음 시험에 남지 않는다.
    inst.call("surface.close", json!({ "surface_id": sid }));
    settle();
}

#[test]
#[ignore]
fn menu_answer_of_another_kind_survives_terminal_right_click() {
    let inst = shared();
    let sid = mouse_test_terminal(&inst);

    inst.call(
        "debug.native_menu.answer",
        json!({ "menu": "Tab", "dismiss": true }),
    );
    terminal_right_click(&inst, sid, 0.5, 0.5);

    let menu = inst.debug_pending_menu();
    assert_eq!(
        (menu["kind"].clone(), menu["surface_id"].as_u64()),
        (json!("TerminalSurface"), Some(sid)),
        "a terminal menu must be captured while only a Tab answer waits: {menu}"
    );
    // 새 응답이 덮어쓴 응답의 종류로 Tab 응답이 남아 있었는지 확인한다.
    let armed = inst.call(
        "debug.native_menu.answer",
        json!({ "menu": "TerminalSurface", "dismiss": true }),
    );
    assert_eq!(
        armed["replaced"],
        json!("Tab"),
        "the Tab answer must survive a terminal right-click: {armed}"
    );

    // 덮어쓴 닫기 응답은 다음 터미널 우클릭이 소비한다. 공유 인스턴스의 다음 시험에 응답이 남지 않는다.
    terminal_right_click(&inst, sid, 0.5, 0.5);
    let menu = inst.debug_pending_menu();
    assert_eq!(
        menu["present"],
        json!(false),
        "the dismiss answer must close the terminal menu instead of capturing it: {menu}"
    );
    inst.call("surface.close", json!({ "surface_id": sid }));
    settle();
}

#[test]
#[ignore]
fn menu_answer_applies_to_terminal_link_right_click() {
    let inst = shared();
    let sid = mouse_test_terminal(&inst);

    // 모든 행을 같은 URL로 채워 창 크기와 관계없이 가운데 행 앞쪽이 링크가 되게 한다.
    // 입력 줄에 URL을 그대로 쓰면 zsh의 URL 인용 기능이 뒤의 `;`를 이스케이프한다.
    inst.call(
        "surface.send",
        json!({
            "surface_id": sid,
            "text": "clear; for i in $(seq 200); do printf 'https%s//example.com/t323-link-menu\\n' :; done\r",
        }),
    );
    let screen = || {
        inst.call("surface.screen_text", json!({ "surface_id": sid }))["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(100), |_| screen()
            .matches("t323-link-menu")
            .count()
            >= 10),
        "link lines never appeared: {:?}",
        screen()
    );
    // 링크 hover는 기본 Ctrl 수식 키를 요구한다. 수식 키 주입 대신 조건을 끈다.
    inst.call(
        "debug.settings.apply",
        json!({ "settings": { "general": { "link_click_modifier": "none" } } }),
    );
    settle();

    inst.call(
        "debug.native_menu.answer",
        json!({ "menu": "TerminalLink", "item": LINK_MENU_SELECT_ITEM }),
    );
    terminal_right_click(&inst, sid, 0.05, 0.5);

    let sel = inst.debug_selection();
    assert_eq!(
        (sel["present"].clone(), sel["surface_id"].as_u64()),
        (json!(true), Some(sid)),
        "the answered Select item must select the link: {sel}"
    );
    let menu = inst.debug_pending_menu();
    assert_eq!(
        menu["present"],
        json!(false),
        "an answered link menu must not stay captured: {menu}"
    );

    // 격리 홈의 기본값으로 되돌린다.
    inst.call(
        "debug.settings.apply",
        json!({ "settings": { "general": { "link_click_modifier": "ctrl" } } }),
    );
    inst.call("surface.close", json!({ "surface_id": sid }));
    settle();
}

#[test]
#[ignore]
fn right_click_explorer_never_falls_back_to_surface_menu() {
    let inst = shared();

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
    // IPC로 만든 탭은 자동 선택되지 않으므로 사용자 전환을 재현해 렌더링한다.
    let last = created["tab_count"]
        .as_u64()
        .expect("tab.create should return tab_count")
        - 1;
    inst.call("debug.switch_tab", json!({ "index": last }));
    settle();

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

/// 공유 앱의 이전 시험이 남긴 선택 탭·cat·마우스 모드를 새 terminal로 분리한다.
fn mouse_test_terminal(inst: &gui_common::GuiTestInstance) -> u64 {
    inst.focus();
    let focused = inst
        .debug_focused_surface()
        .expect("focused surface before mouse fixture");
    let surfaces = inst.call("surface.list", json!({}));
    let pane = surfaces
        .as_array()
        .expect("surface list")
        .iter()
        .find(|surface| surface["id"].as_u64() == Some(focused))
        .and_then(|surface| surface["pane_id"].as_u64())
        .expect("focused surface pane");
    let created = inst.call("tab.create", json!({"pane_id":pane,"type":"terminal"}));
    let sid = created["surface_id"].as_u64().expect("created terminal");
    let index = created["tab_count"].as_u64().expect("created tab count") - 1;
    inst.call("debug.switch_tab", json!({"index":index}));
    settle();
    assert_eq!(
        inst.debug_focused_surface(),
        Some(sid),
        "mouse fixture must select its terminal"
    );
    sid
}

/// 1000·1002·1003은 독립 플래그라 먼저 모두 끄고 원하는 모드 하나를 켠다.
/// cat -v로 수신 바이트를 관측하며 mark는 호출자가 출력 준비 후 설정한다.
#[cfg(test)]
fn enable_mouse_tracking(inst: &gui_common::GuiTestInstance, sid: u64, mode: &str) {
    let enable = if mode == "off" {
        String::new()
    } else {
        format!("\\e[?{mode}h")
    };
    inst.call("surface.set_mark", json!({"surface_id":sid}));
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": format!(
            "printf '\\e[?1000l\\e[?1002l\\e[?1003l{enable}\\e[?1006h%s%s\\n' '__TASTY_MOUSE_' '{mode}_READY__'; cat -v\n"
        ) }),
    );
    std::thread::sleep(Duration::from_millis(400));
    let output = read_since_mark(inst, sid);
    assert!(
        output.contains(&format!("__TASTY_MOUSE_{mode}_READY__")),
        "mouse mode setup must execute in the shell, not merely echo into an old cat: {output:?}"
    );
}

#[cfg(test)]
fn read_since_mark(inst: &gui_common::GuiTestInstance, sid: u64) -> String {
    let res = inst.call(
        "surface.read_since_mark",
        json!({ "surface_id": sid, "strip_ansi": true }),
    );
    res["text"].as_str().unwrap_or("").to_string()
}

#[test]
#[ignore]
fn hover_motion_reported_only_for_mode_1003() {
    let inst = shared();
    let sid = mouse_test_terminal(&inst);

    enable_mouse_tracking(&inst, sid, "1003");
    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    for fx in [0.30_f32, 0.45, 0.60, 0.75] {
        inst.inject_mouse(sid, fx, 0.5, "move", 0);
    }
    settle();
    let out = read_since_mark(&inst, sid);
    assert!(
        out.contains("^[[<35;"),
        "1003 hover should emit cb=35 motion reports, got: {out:?}"
    );

    inst.call("surface.set_mark", json!({ "surface_id": sid }));
    inst.inject_mouse(sid, 0.75, 0.5, "move", 0);
    settle();
    assert!(
        !read_since_mark(&inst, sid).contains("^[[<35;"),
        "same-cell motion must be deduped"
    );

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

#[test]
#[ignore]
fn hover_motion_never_reaches_a_non_focused_surface() {
    let inst = shared();
    let focused = mouse_test_terminal(&inst);

    let res = inst.call(
        "split",
        json!({ "level": "surface", "direction": "vertical", "target_surface": focused }),
    );
    let other = res["new_surface_id"].as_u64().expect("new_surface_id");
    settle();

    // 비포커스 대상에서 변화가 없음을 검사하기 전에 포커스된 대상의 hover 출력으로 주입 동작을 확인한다.
    enable_mouse_tracking(&inst, focused, "1003");
    inst.call("surface.set_mark", json!({ "surface_id": focused }));
    for fx in [0.30_f32, 0.50, 0.70] {
        inst.inject_mouse(focused, fx, 0.5, "move", 0);
    }
    settle();
    let control = read_since_mark(&inst, focused);
    assert!(
        control.contains("^[[<35;"),
        "포커스된 서피스의 hover 보고를 찾지 못해 주입 경로의 동작을 확인할 수 없다: {control:?}"
    );
    // 공유 인스턴스에 모드를 남기지 않는다. Ctrl+C는 cat만 끝내고 추적 모드를 복원하지 않는다.
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

#[test]
#[ignore]
fn drag_motion_carries_the_pressed_button() {
    let inst = shared();
    let sid = mouse_test_terminal(&inst);

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

    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{3}" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    enable_mouse_tracking(&inst, sid, "off");
    // A prior tracking gesture need not erase an existing local selection.
    // Establish the empty-selection precondition using the ordinary local click path.
    inst.inject_mouse(sid, 0.30, 0.5, "press", 0);
    inst.inject_mouse(sid, 0.30, 0.5, "release", 0);
    settle();
    assert_eq!(
        inst.debug_selection()["present"],
        json!(false),
        "local click must clear the selection before testing a right drag"
    );
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

/// 팔레트 붙여넣기는 터미널 키 입력을 거치지 않으므로 붙여넣기 처리 자체가 사용자 입력을 기록해야 한다.
/// 이전 입력 기록의 유효 시간이 지난 뒤 실행하고 실제 출력과 idle_seconds 감소를 함께 확인한다.
#[test]
#[ignore]
fn test_palette_paste_records_user_typing() {
    let inst = shared();
    let sid = inst
        .debug_focused_surface()
        .expect("a focused surface to paste into");
    let is_typing = || inst.call("surface.is_typing", json!({ "surface_id": sid }));

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut idle = 0.0;
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(200), |_| {
            idle = is_typing()["idle_seconds"].as_f64().unwrap();
            !(0.0..=3.0).contains(&idle)
        }),
        "idle never exceeded 3s: {idle}"
    );

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
    let mut screen = String::new();
    tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(100), |_| {
        screen = inst.call("surface.screen_text", json!({ "surface_id": sid }))["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        screen.contains(mark)
    });
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

    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{15}" }),
    );
}

/// 창 종류별 window_id 를 debug.window.list 에서 찾는다. 설정 창은 열기 요청 뒤 예약되므로 기다린다.
fn debug_window_id(inst: &gui_common::GuiTestInstance, kind: &str) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut list = serde_json::Value::Null;
    let mut found = None;
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(100), |_| {
            list = inst.call("debug.window.list", json!({}));
            found = list["windows"]
                .as_array()
                .and_then(|ws| ws.iter().find(|w| w["kind"] == kind))
                .and_then(|w| w["window_id"].as_u64());
            found.is_some()
        }),
        "no {kind} window: {list}"
    );
    found.expect("held implies a window id")
}

/// 창 하나에 붙여넣은 글자가 클립보드 값 그대로인지 Tasty 의 기존 경로만으로 읽는다.
/// 팔레트 쿼리 칸에 command+V 로 붙인 뒤 클립보드를 다른 값으로 덮고, 쿼리를 command+A·C 로
/// 복사해 클립보드로 되돌린다. 그 클립보드를 팔레트의 Paste 명령으로 터미널 입력줄에 붙여
/// 화면 글자에서 찾는다. `window_id` 가 없으면 포커스 입구, 있으면 창 지정 입구로 키를 넣는다.
fn assert_paste_carries_the_clipboard_text(
    inst: &gui_common::GuiTestInstance,
    window_id: Option<u64>,
    mark: &str,
) {
    let sid = inst
        .debug_focused_surface()
        .expect("a focused surface to read the pasted text");
    let key = |key: &str, pressed: bool, modifiers: Option<&[&str]>| -> String {
        let mut params = json!({ "key": key, "pressed": pressed });
        if let Some(m) = modifiers {
            params["modifiers"] = json!(m);
        }
        if let Some(wid) = window_id {
            params["window_id"] = json!(wid);
        }
        let r = inst.call("debug.inject_egui_key", params.clone());
        assert_eq!(r["injected"], json!(true), "{params} -> {r}");
        r["pushed"].as_str().unwrap_or_default().to_string()
    };
    let settle = || std::thread::sleep(Duration::from_millis(300));

    inst.call("clipboard.set_text", json!({ "text": mark }));
    inst.call(
        "debug.host_popup.open",
        json!({ "popup_id": "command_palette" }),
    );
    settle();
    assert_eq!(key("V", true, Some(&["command"])), "paste");
    key("V", false, None);
    settle();
    inst.call(
        "clipboard.set_text",
        json!({ "text": "EGUI_KEY_CLIPBOARD_OVERWRITTEN" }),
    );
    key("A", true, None);
    key("A", false, None);
    assert_eq!(key("C", true, None), "copy");
    key("C", false, Some(&[]));
    settle();
    key("Escape", true, None);
    key("Escape", false, None);
    settle();

    inst.call(
        "debug.host_popup.open",
        json!({ "popup_id": "command_palette" }),
    );
    settle();
    inst.call("debug.inject_egui_text", json!({ "text": "paste" }));
    settle();
    inst.call("debug.inject_egui_key", json!({ "key": "Enter" }));
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut screen = String::new();
    tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(100), |_| {
        screen = inst.call("surface.screen_text", json!({ "surface_id": sid }))["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        screen.contains(mark)
    });
    inst.call(
        "surface.send",
        json!({ "surface_id": sid, "text": "\u{15}" }),
    );
    assert!(
        screen.contains(mark),
        "pasted query (window {window_id:?}) did not carry {mark:?}: {screen:?}"
    );
}

/// debug.inject_egui_key 의 두 입구(포커스된 메인 창, window_id)가 실제 창의 egui 상태를 쓴다.
/// 유지 수식 키는 그 창의 GpuState 에 남고 다른 창으로 새지 않으며, 붙여넣기 글자는 그 창의
/// egui-winit 클립보드에서 읽는다. 수식 키는 응답의 pushed 로, 붙여넣은 글자는 두 입구 각각에서
/// 팔레트 쿼리를 되돌려 읽어 판정한다.
/// 보조 창을 받는 debug.inject_egui_clipboard 도 같은 설정 창으로 확인한다.
#[test]
#[ignore]
fn test_egui_key_injection_uses_each_windows_own_egui_state() {
    let inst = shared();
    let pushed = |params: serde_json::Value| -> String {
        let r = inst.call("debug.inject_egui_key", params.clone());
        assert_eq!(r["injected"], json!(true), "{params} -> {r}");
        r["pushed"].as_str().unwrap_or_default().to_string()
    };
    inst.call(
        "clipboard.set_text",
        json!({ "text": "EGUI_KEY_PASTE_MARK" }),
    );

    // 포커스 입구: command 를 걸면 C 는 복사, 생략한 V 는 유지된 command 로 붙여넣기다.
    assert_eq!(
        pushed(json!({ "key": "C", "modifiers": ["command"] })),
        "copy"
    );
    assert_eq!(pushed(json!({ "key": "C", "pressed": false })), "key");
    assert_eq!(pushed(json!({ "key": "V" })), "paste");
    assert_eq!(pushed(json!({ "key": "V", "pressed": false })), "key");
    assert_eq!(pushed(json!({ "key": "C", "modifiers": [] })), "key");
    assert_eq!(pushed(json!({ "key": "C", "pressed": false })), "key");
    // 포커스 입구의 붙여넣기 글자가 클립보드 값 그대로다.
    assert_paste_carries_the_clipboard_text(&inst, None, "EGUI_FOCUSED_PASTE_MARK");

    inst.call("debug.settings.open", json!({ "tab": "general" }));
    let main = debug_window_id(&inst, "main");
    let settings = debug_window_id(&inst, "settings");

    // 창 지정 입구: 메인 창에 건 command 는 설정 창으로 새지 않는다.
    assert_eq!(
        pushed(json!({ "window_id": main, "key": "C", "modifiers": ["command"] })),
        "copy"
    );
    assert_eq!(pushed(json!({ "window_id": settings, "key": "C" })), "key");
    assert_eq!(
        pushed(json!({ "window_id": settings, "key": "V", "modifiers": ["command"] })),
        "paste"
    );
    assert_eq!(pushed(json!({ "window_id": settings, "key": "C" })), "copy");
    assert_eq!(pushed(json!({ "window_id": main, "key": "X" })), "cut");

    let clip = inst.call(
        "debug.inject_egui_clipboard",
        json!({ "op": "copy", "window_id": settings }),
    );
    assert_eq!(clip, json!({ "injected": true, "window_id": settings }));

    for wid in [main, settings] {
        pushed(json!({ "window_id": wid, "key": "C", "pressed": false, "modifiers": [] }));
    }
    inst.call("debug.settings.close", json!({}));
    inst.wait_for_ui("settings modal is gone", Duration::from_secs(3), |s| {
        !s.settings_modal_is_up()
    });
    // 창 지정 입구의 붙여넣기 글자도 클립보드 값 그대로다.
    assert_paste_carries_the_clipboard_text(&inst, Some(main), "EGUI_WINDOW_PASTE_MARK");
    // 메인 창 붙여넣기가 터미널 입력줄에 닿았을 수 있어 다음 시험을 위해 줄을 비운다.
    if let Some(sid) = inst.debug_focused_surface() {
        inst.call(
            "surface.send",
            json!({ "surface_id": sid, "text": "\u{15}" }),
        );
    }
}
