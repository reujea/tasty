//! 창이 키를 egui·터미널로 보내는 두 게이트가 시험이 밟는 판정 함수를 그대로 쓰는지 확인한다. 팝업 시험
//! (`adapters::ui::popup::window_key_gate_tests`)은 창 없이 같은 함수(`window_feeds_egui`, `window_escape`,
//! `window_holds_terminal_keys`)로 키를 보낸다. 호출부가 그 함수를 거치지 않고 키를 보내면 시험이 보지 못하므로
//! 호출부의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출부 앞에 같은 키를 다른 길로 egui·PTY 에 보내는 코드를 더하는 것(첫 게이트만 본다).

use super::{fn_body, repo_root, strip_comments};

fn read(rel: &str) -> String {
    strip_comments(
        &std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n"),
    )
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn at(body: &str, needle: &str, what: &str) -> usize {
    assert_eq!(
        body.matches(needle).count(),
        1,
        "{what}: `{needle}` 이 하나가 아니다"
    );
    body.find(needle).unwrap_or_default()
}

/// egui 전달 게이트: `feed_egui_keyboard` 는 egui 로 보내기 전에 `window_feeds_egui` 의 결과로만 돌아간다.
#[test]
fn the_egui_feed_goes_through_the_window_feed_gate() {
    let src = read("src/view/main.rs");
    let body = squeeze(&fn_body(&src, "fn feed_egui_keyboard(").expect("feed_egui_keyboard"));
    let gate = at(
        &body,
        "letSome(bound)=self.state.window_feeds_egui(egui_surface,binds)else{return(false,false);};",
        "egui 전달 게이트",
    );
    let feed = at(
        &body,
        "self.base.gpu.handle_egui_event(",
        "egui 로 보내는 호출",
    );
    assert!(gate < feed, "egui 로 보낸 뒤에 게이트를 본다");
    at(
        &body,
        "letbinds=matches!(event,WindowEvent::KeyboardInput{..}|WindowEvent::Ime(_));",
        "키 주인을 묶는 키 종류",
    );
    assert_eq!(
        body.matches("return(false,false)").count(),
        1,
        "egui 전달 게이트 말고 보내지 않고 돌아가는 갈래가 생겼다. 시험이 보는 게이트가 하나가 아니다"
    );
}

/// 터미널 게이트: `handle_keyboard_input` 은 Esc 를 창의 Esc 처리로 보낸 뒤 `window_holds_terminal_keys` 가 참이면
/// 터미널로 보내지 않는다.
#[test]
fn the_terminal_goes_through_the_window_hold_gate() {
    let src = read("src/view/main/keyboard.rs");
    let body = squeeze(&fn_body(&src, "fn handle_keyboard_input(").expect("handle_keyboard_input"));
    let escape = at(
        &body,
        "ifself.try_consume_escape_key(event){return;}",
        "Esc 처리",
    );
    let hold = at(
        &body,
        "letoverlay_open=self.state.window_holds_terminal_keys();",
        "터미널 게이트",
    );
    let stop = at(&body, "ifoverlay_open{return;}", "터미널 게이트의 멈춤");
    let pty = at(
        &body,
        "FocusedSurfaceType::Terminal=>self.forward_key_to_terminal(engine,event)",
        "터미널로 보내는 호출",
    );
    assert!(
        escape < hold && hold < stop && stop < pty,
        "Esc 처리 → 터미널 게이트 → 터미널 순서가 아니다"
    );
    let consume =
        squeeze(&fn_body(&src, "fn try_consume_escape_key(").expect("try_consume_escape_key"));
    at(
        &consume,
        ".window_escape(self.base.gpu.egui_input_events_mut())",
        "창의 Esc 처리가 이번 egui 입력에서 Esc 를 뺀다",
    );
}
