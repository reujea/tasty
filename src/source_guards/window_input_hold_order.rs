//! 보류 검사가 창 입력 맨 앞 한 자리인지 소스에서 확인한다. 보류 규칙(줄 서기·정착 줄·재생 순서)은
//! `gfx::gpu::edit_keys` 시험이 상태로 잰다. 그 검사를 부르는 자리(App 메인 창 입구, 별도 창 입구)는 App·창 view 를
//! 시험에서 만들 수 없어 이 검사가 고정한다. 계약: `docs/architecture/input-layer.md`의 "보류 줄".

use super::{fn_body, repo_root, strip_comments};

fn body(rel: &str, signature: &str) -> String {
    let src = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, signature)
        .unwrap_or_else(|| panic!("{rel}에서 `{signature}` 본문을 읽지 못했다"));
    strip_comments(&body).split_whitespace().collect()
}

fn at(body: &str, needle: &str, what: &str) -> usize {
    let needle: String = needle.split_whitespace().collect();
    body.find(&needle)
        .unwrap_or_else(|| panic!("{what}: `{needle}` 이 없다"))
}

/// 메인 창: 보류 검사가 출발 노드(`settle_window_key`)와 plugin 단축키 단계보다 앞이다. 그래서 정착되지 않은 앞
/// 입력 뒤의 plugin 단축키는 그 패스 뒤에 그때의 노드로 정한다.
#[test]
fn a_plugin_shortcut_behind_unsettled_input_waits_for_its_pass() {
    let route = body("src/app/edit_key_replay.rs", "fn route_input_steps(");
    let hold = at(
        &route,
        "if self.queue_behind_plugin_edit(id, &event, at) || self.hold_window_input(id, &event) { return; }",
        "맨 앞 보류 검사",
    );
    let settle = at(
        &route,
        "let settled = self.settle_window_key(id, &event);",
        "출발 노드",
    );
    let plugin = at(&route, "self.try_plugin_shortcut(id, ke)", "plugin 단축키");
    assert!(
        hold < settle && settle < plugin,
        "보류 검사가 맨 앞이 아니다"
    );
}

/// 메인 창의 줄 하나: plugin 편집 보류 줄과 창의 보류 줄(편집·정착)을 같은 자리에서 본다. 창의 줄은 정착 조건
/// 3(그리지 않은 넘김 Esc)을 함께 읽는다.
#[test]
fn escape_letter_and_plugin_shortcut_keep_arrival_order() {
    let route = body("src/app/edit_key_replay.rs", "fn route_input_steps(");
    at(
        &route,
        "self.queue_behind_plugin_edit(id, &event, at) || self.hold_window_input(id, &event)",
        "두 줄",
    );
    let hold = body("src/app/edit_key_replay.rs", "fn hold_window_input(");
    at(
        &hold,
        "let handed = main.state.popups.has_undrawn_handed_escape();
         main.base.gpu.hold_window_input(&main.base.winit, event, handed)",
        "창의 줄",
    );
}

/// 별도 창(설정·Preset·Plugins): 보류 검사가 단계 순서(`run_key_event`)보다 앞이다.
#[test]
fn a_separate_window_key_after_unsettled_input_waits() {
    let entry = body(
        "src/view/judged_event.rs",
        "pub(crate) fn judged_window_event<",
    );
    let hold = at(
        &entry,
        "if judged.is_none() && hold_window_input(view, &event, input) { return ViewAction::None; }",
        "별도 창 보류 검사",
    );
    let steps = at(&entry, "run_key_event(&mut steps, event)", "단계 순서");
    assert!(hold < steps, "별도 창 보류 검사가 단계 순서보다 뒤다");
    let check = body("src/view/judged_event.rs", "fn hold_window_input<");
    at(
        &check,
        "let handed = popups.has_undrawn_handed_escape();
         let base = view.base_mut();
         base.gpu.hold_window_input(&base.winit, event, handed)",
        "별도 창의 줄",
    );
}

/// debug 주입: 보류 줄 검사가 주입 입구 맨 앞 한 자리이고 winit 사건과 같은 두 줄(plugin 편집 보류 줄, 창의 보류 줄)을
/// 같은 순서로 본다. 줄 순서와 재생 규칙은 `gfx::gpu::edit_keys` 의 `a_debug_input_takes_its_place_in_the_hold_line` 이
/// 상태로 잰다. 줄 선 주입은 재생 때 같은 입구로 다시 간다.
#[test]
fn a_debug_injection_takes_the_window_hold_lines_first() {
    let entry = body("src/app/ipc/debug_input_entry.rs", "fn enter_debug_input(");
    let plugin = at(
        &entry,
        "let Some(input) = self.plugin_edit_holds.queue_debug(id, input) else { return Some(DebugEntered::Held); };",
        "plugin 편집 보류 줄",
    );
    let window = at(
        &entry,
        "let Some(input) = w.base.gpu.hold_debug_input(&w.base.winit, input) else {",
        "메인 창의 줄",
    );
    let run = at(
        &entry,
        "w.enter_debug_input(&engine.read(), input)",
        "메인 창 주입",
    );
    assert!(
        plugin < window && window < run,
        "보류 검사가 주입 앞 한 자리가 아니다"
    );
    let separate = body(
        "src/view/window_input.rs",
        "pub(crate) fn enter_window_debug<",
    );
    let hold = at(
        &separate,
        "let Some(input) = base.gpu.hold_debug_input(&base.winit, input) else {",
        "별도 창의 줄",
    );
    let key = at(
        &separate,
        "inject_window_key(&ctx, popups, gpu, key, pressed, modifiers)",
        "별도 창 키",
    );
    assert!(hold < key, "별도 창 보류 검사가 주입보다 뒤다");
    let replay = body("src/app/edit_key_replay.rs", "fn replay_edit_steps(");
    at(
        &replay,
        "ReplayStep::Debug(input) => { let _ = self.enter_debug_input(id, input); }",
        "메인 창 재생",
    );
    let window_replay = body("src/view/mod.rs", "fn replay_edit_keys<");
    at(
        &window_replay,
        "Replayed::Debug(input) => { let _ = window_input::enter_window_debug(view, input); ViewAction::None }",
        "별도 창 재생",
    );
}
