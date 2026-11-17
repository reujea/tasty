//! Escape 처리에서 설정 열기 요청·알림·포커스된 팝업의 순서를 확인한다.
//! 앞의 두 분기는 포커스와 무관하게 처리하므로 일반 팝업 처리보다 먼저 있어야 한다.
//! settings_open_requested는 열기 요청 상태다. 이미 열린 설정 모달을 Escape로 닫는 경로를 뜻하지 않는다.
//! 팝업이 키 입력을 막기 전에 Escape를 처리하도록 오버레이 검사보다 앞에 있어야 한다.
//! 여기서는 호출 위치·순서만 확인하며 대상 선택 동작은 adapters::ui::popup의 단위 테스트가 확인한다.

fn read(rel: &str) -> String {
    let p = tasty_doc_guards::repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn only_at(hay: &str, needle: &str, what: &str) -> usize {
    let n = hay.matches(needle).count();
    assert_eq!(n, 1, "{what}: `{needle}` 이 {n} 곳이다 (1 곳이어야 한다)");
    hay.find(needle).unwrap()
}

fn between<'a>(hay: &'a str, start: &str, end: &str) -> &'a str {
    let a = only_at(hay, start, "범위 시작 표지");
    let b = only_at(hay, end, "범위 끝 표지");
    assert!(
        b > a,
        "범위 끝 표지가 시작보다 앞에 있다 — 함수 순서가 바뀌었다"
    );
    &hay[a..b]
}

const KEYBOARD: &str = "src/view/main/keyboard.rs";

#[test]
fn the_three_escape_consumers_keep_their_order() {
    let src = read(KEYBOARD);
    // 다른 함수의 같은 조건문을 섞지 않도록 Escape 처리 함수 범위에서 순서를 비교한다.
    let body = between(
        &src,
        "fn try_consume_escape_key(",
        "fn try_consume_shortcut_key(",
    );
    let settings = only_at(
        body,
        "if self.state.settings_open_requested {",
        "Escape 의 settings 분기",
    );
    let notifications = only_at(
        body,
        "if self.state.popups.is_open(\"notifications\") {",
        "Escape 의 notifications 분기",
    );
    let general = only_at(
        body,
        "if let Some((_, instance, closes)) = self.state.popups.focused_dismissal_target() {",
        "Escape 의 포커스된 popup 분기",
    );

    assert!(
        settings < notifications,
        "settings 분기가 notifications 보다 뒤에 있다 — 앞의 둘은 포커스와 무관하게 먹으므로 \
         순서가 바뀌면 동작이 바뀐다."
    );
    assert!(
        notifications < general,
        "일반 팝업 처리가 설정 열기 요청·알림 특례보다 앞에 있다. 두 특례가 포커스와 무관하게 먼저 처리되도록 순서를 유지한다."
    );
}

#[test]
fn the_escape_path_runs_before_the_overlay_gate() {
    let src = read(KEYBOARD);
    let escape_call = only_at(
        &src,
        "if self.try_consume_escape_key(event) {",
        "Escape 소비 호출",
    );
    let gate = only_at(
        &src,
        "let overlay_open = self.state.keyboard_overlay_open();",
        "오버레이 게이트",
    );
    assert!(
        escape_call < gate,
        "Escape 처리가 오버레이 검사 뒤에 있어 팝업의 키 입력 차단에 걸릴 수 있다."
    );
}

/// 팝업의 닫기 여부를 바깥 클릭 정책과 맞춘다. 닫지 않는 팝업도 포커스는 해제해야 한다.
#[test]
fn escape_closes_only_what_an_outside_click_would_close() {
    let src = read(KEYBOARD);
    let start = only_at(
        &src,
        "if let Some((_, instance, closes)) = self.state.popups.focused_dismissal_target() {",
        "Escape 의 포커스된 popup 분기",
    );
    let body = &src[start..start + 400];
    assert!(
        body.contains("set_instance_focused(instance, false)"),
        "팝업 포커스 해제가 없어 오버레이 입력 차단에서 벗어날 수 없다: {body}"
    );
    assert!(
        body.contains("if closes {"),
        "팝업 닫기가 close_on_outside_click 조건을 따르지 않는다: {body}"
    );
}
