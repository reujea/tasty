//! Escape 처리에서 설정 열기 요청·알림·plugin 키 주인·포커스된 팝업의 순서를 확인한다.
//! plugin 키 주인의 Esc는 펼쳤든 아니든 그 렌더에 넘긴다(렌더가 선언과 표시 상태로 닫기·다시 접기를 정한다). 아래
//! host 팝업을 먼저 닫지 않도록 일반 팝업 처리보다 먼저 있어야 한다.
//! 설정 열기 요청 분기는 포커스와 무관하게, 알림 분기는 포커스 노드가 다른 팝업이 아닐 때 처리하므로 일반 팝업
//! 처리보다 먼저 있어야 한다.
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
/// 창의 Esc 판정(`MainViewState::window_escape` → `escape_gate`). keyboard.rs 의 Escape 처리가 이 판정만 부른다.
const ESCAPE_GATE: &str = "src/state/escape_gate.rs";

#[test]
fn the_three_escape_consumers_keep_their_order() {
    let keyboard = read(KEYBOARD);
    let escape_key = between(
        &keyboard,
        "fn try_consume_escape_key(",
        "fn try_consume_shortcut_key(",
    );
    only_at(
        escape_key,
        ".window_escape(",
        "Escape 처리의 창 Esc 판정 호출",
    );
    let src = read(ESCAPE_GATE);
    let window = between(&src, "fn window_escape(", "fn escape_gate(");
    only_at(window, "self.escape_gate()", "창 Esc 처리의 판정 호출");
    // 판정 함수 하나만 있는 파일이다. 함수 시작부터 순서를 비교한다.
    let start = only_at(&src, "fn escape_gate(", "창 Esc 판정 함수");
    let body = &src[start..];
    let settings = only_at(
        body,
        "if self.settings_open_requested {",
        "Escape 의 settings 분기",
    );
    let notifications = only_at(
        body,
        "if self.notifications_take_escape(live) {",
        "Escape 의 notifications 분기",
    );
    let general = only_at(
        body,
        "if let Some((instance, escape)) = self.popups.host_key_owner_escape() {",
        "Escape 의 host 키 주인 분기",
    );
    let plugin_owner = only_at(
        body,
        "Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(_))",
        "Escape 의 plugin 키 주인 분기",
    );
    assert!(
        plugin_owner < general,
        "plugin 키 주인 분기가 일반 팝업 처리보다 뒤에 있다. plugin 팝업이 받을 Esc 로 아래 host 팝업이 닫힌다."
    );
    assert!(
        body[plugin_owner..general]
            .trim_start_matches(|c: char| c != '{')
            .split('}')
            .next()
            .is_some_and(|branch| branch.contains("hand_to_owner: true")),
        "plugin 키 주인 분기가 그 Esc 를 렌더에 넘기지 않는다. 펼친 plugin 팝업의 다시 접기와 선언 처리는 렌더가 한다."
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
        "let overlay_open = self.state.window_holds_terminal_keys();",
        "오버레이 게이트",
    );
    assert!(
        escape_call < gate,
        "Escape 처리가 오버레이 검사 뒤에 있어 팝업의 키 입력 차단에 걸릴 수 있다."
    );
}

/// host 키 주인의 Esc 는 바깥 누름 정책이 아니라 선언 `escape` 를 따른다. Ignore 는 넘기지 않고, Close·Unfocus 는
/// 사건 시점에 포커스를 풀고 그 Esc 를 넘긴다(닫기는 팝업 층 중앙 단계가 내용 결과를 본 뒤 정한다).
#[test]
fn escape_follows_the_declaration() {
    let src = read(ESCAPE_GATE);
    let start = only_at(&src, "fn escape_gate(", "창 Esc 판정 함수");
    let gate = &src[start..];
    for gone in ["close_on_outside_click", "focused_dismissal_target"] {
        assert!(
            !gate.contains(gone),
            "창 Esc 판정이 `{gone}` 를 읽는다. 닫기는 선언 escape 를 따른다."
        );
    }
    let host = only_at(
        gate,
        "if let Some((instance, escape)) = self.popups.host_key_owner_escape() {",
        "Escape 의 host 키 주인 분기",
    );
    let body = &gate[host..];
    let ignore = only_at(body, "if escape == PopupEscape::Ignore {", "Ignore 갈래");
    let release = only_at(
        body,
        "self.popups.set_instance_focused(instance, false);",
        "사건 시점 포커스 해제",
    );
    let ignore_body = &body[ignore..release];
    assert!(
        ignore_body.contains("hand_to_owner: false"),
        "Ignore 갈래가 Esc 를 넘긴다: {ignore_body}"
    );
    assert!(
        body[release..].contains("hand_to_owner: true"),
        "Close·Unfocus 갈래가 포커스를 푼 뒤 Esc 를 넘기지 않는다"
    );
}
