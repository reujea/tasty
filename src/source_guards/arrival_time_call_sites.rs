//! 더블탭·수식키 힌트 판정이 사건의 도착 시각을 읽는지 소스에서 확인한다. 검출기는 넘겨받은 시각으로 판정하고
//! (`double_tap` · `modifier_hint_overlay` 시험), 보류 줄은 줄 선 사건의 도착 시각을 재생까지 든다(`edit_key` ·
//! `gfx::gpu::edit_keys` · `app::edit_key_replay` 시험). 그 사이의 제품 호출자(메인 창·설정 창 view)는 시험에서 만들 수
//! 없어 이 검사가 그 자리를 고정한다. 호출자가 처리 시각(`Instant::now()`)을 넘기면 보류 뒤 재생한 사건이 처리 시각으로
//! 판정된다. 계약: `docs/design/policies/key-mapping.md`의 "보류와 재생".

use super::{fn_body, repo_root, strip_comments};

fn body(rel: &str, signature: &str) -> String {
    let src = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, signature)
        .unwrap_or_else(|| panic!("{rel}에서 `{signature}` 본문을 읽지 못했다"));
    strip_comments(&body).split_whitespace().collect()
}

fn has(body: &str, needle: &str, what: &str) {
    let needle: String = needle.split_whitespace().collect();
    assert!(body.contains(&needle), "{what}: `{needle}` 이 없다");
}

/// 메인 창은 누름·뗌을 그 사건의 도착 시각으로 더블탭 검출기에 준다.
#[test]
fn a_held_main_double_tap_is_judged_by_its_arrival_interval() {
    let feed = body("src/view/main/keyboard.rs", "fn handle_keyboard_input(");
    has(
        &feed,
        "let arrived = self.base.gpu.event_arrival();",
        "메인 더블탭 도착 시각",
    );
    has(
        &feed,
        "key_ends_at_latch( &mut self.double_tap, &event.logical_key, pressed, arrived,",
        "메인 더블탭 공급",
    );
}

/// 메인 창의 힌트 최초 홀드는 수식키 사건의 도착 시각에서 시작한다.
#[test]
fn a_held_modifier_hint_uses_its_arrival_time() {
    let event = body("src/view/main.rs", "fn after_egui_feed(");
    has(
        &event,
        ".update_hold(ctrl, alt, option, shift, self.base.gpu.event_arrival(),)",
        "힌트 최초 홀드",
    );
}

/// 단축키 뒤 힌트 재설정은 단축키 사건의 도착 시각으로 잰다. 더블탭 성공과 단축키 후처리가 같은 함수를 쓴다.
#[test]
fn a_held_shortcut_resets_the_hint_timer_at_its_arrival() {
    let reset = body(
        "src/view/main/keyboard.rs",
        "fn reset_modifier_hint_reveal_timer(",
    );
    has(
        &reset,
        "let arrived = self.base.gpu.event_arrival();
         self.state.modifier_hint.reset_reveal_timer_if_not_shown(&theme, arrived);",
        "힌트 재설정",
    );
    let after = body("src/view/main/keyboard.rs", "fn after_shortcut_consumed(");
    has(
        &after,
        "self.reset_modifier_hint_reveal_timer();",
        "단축키 후처리",
    );
}

/// 설정 창의 두 번째 검출기도 그 창의 사건 도착 시각을 쓴다.
#[test]
fn a_held_settings_double_tap_is_judged_by_its_arrival_interval() {
    let judged = body("src/view/settings.rs", "fn own_event(");
    has(
        &judged,
        "self.double_tap.on_key_event( &event.logical_key, event.state == ElementState::Pressed, self.base.gpu.event_arrival(), );",
        "설정 창 더블탭 공급",
    );
}

/// 도착 시각은 App 이 사건마다 창에 적는다. 새 입력은 지금, 줄 섰다 재생하는 입력은 처음 도착한 때이고, 처리가 끝나면
/// 앞 값으로 되돌린다. 별도 창의 자체 재생도 같다.
#[test]
fn each_event_runs_with_its_arrival_time() {
    let fresh = body("src/app/edit_key_replay.rs", "fn route_window_input(");
    has(
        &fresh,
        "self.route_window_input_at(id, event, Instant::now());",
        "새 입력",
    );
    let at = body("src/app/edit_key_replay.rs", "fn route_window_input_at(");
    has(
        &at,
        "let before = self.replace_event_arrival(id, Some(at));
         self.route_input_steps(id, event, at);
         self.replace_event_arrival(id, before);",
        "창 입력의 도착 시각",
    );
    let replay = body("src/app/edit_key_replay.rs", "fn replay_edit_steps(");
    has(
        &replay,
        "ReplayStep::Input(event, at) => self.route_window_input_at(id, event, at),",
        "plugin 보류 재생",
    );
    let window = body("src/view/mod.rs", "fn replay_edit_keys<");
    has(
        &window,
        "let before = view.base_mut().gpu.replace_event_arrival(at);
         let action = handle(view, event, judged);
         view.base_mut().gpu.replace_event_arrival(before);",
        "별도 창 재생",
    );
}
