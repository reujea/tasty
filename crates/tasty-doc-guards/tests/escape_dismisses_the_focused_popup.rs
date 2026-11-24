//! Escape 적용(단계 11)에서 설정 열기 요청 래치 수신자와 사건 시점 수신자(단계 4) 분기의 순서를 확인한다.
//! plugin 팝업 수신자의 Esc는 펼쳤든 아니든 그 렌더에 넘긴다(렌더가 선언과 표시 상태로 닫기·다시 접기를 정한다).
//! 설정 열기 요청 래치 분기는 단계 4 수신자가 래치 노드일 때 처리하므로 수신자 분기보다 먼저 있어야 한다. 알림 목록은
//! 특례 갈래가 없다. 범위 노드에 붙은 비덮음 팝업으로 사건 시점 수신자가 된다(`key_route::route_escape`).
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
/// 창의 Esc 적용(`MainViewState::apply_escape_decision` → `escape_outcome`, 단계 11). keyboard.rs 의 Escape 처리가 이 적용만 부른다.
const ESCAPE_GATE: &str = "src/state/escape_gate.rs";

#[test]
fn the_escape_consumers_keep_their_order() {
    let keyboard = read(KEYBOARD);
    let escape_key = between(
        &keyboard,
        "fn apply_escape_key_event(",
        "fn try_consume_shortcut_key(",
    );
    only_at(
        escape_key,
        ".apply_escape_key(",
        "Escape 처리의 단계 11 적용 호출",
    );
    let src = read(ESCAPE_GATE);
    let window = between(&src, "fn apply_escape_decision(", "fn escape_outcome(");
    only_at(
        window,
        "self.escape_outcome(decision)",
        "창 Esc 처리의 판정 호출",
    );
    // 판정 함수 하나만 있는 파일이다. 함수 시작부터 순서를 비교한다.
    let start = only_at(&src, "fn escape_outcome(", "창 Esc 판정 함수");
    let body = &src[start..];
    let settings = only_at(
        body,
        "if crate::adapters::ui::input::key_nodes::settings_latch_receives(decision) {",
        "Escape 의 설정 열기 요청 래치 수신자 분기",
    );
    assert!(
        !body.contains("notifications"),
        "창 Esc 판정에 알림 목록 갈래가 있다. 알림은 붙은 팝업으로 단계 4 수신자가 된다."
    );
    let receiver = only_at(body, "match routed.receiver {", "사건 시점 수신자 분기");
    let plugin_owner = only_at(
        body,
        "KeyReceiver::PluginPopup(_) => EscapeOutcome::Consumed {",
        "Escape 의 plugin 팝업 수신자 분기",
    );
    let general = only_at(
        body,
        "KeyReceiver::Popup(instance) => {",
        "Escape 의 host 팝업 수신자 분기",
    );
    assert!(
        body[plugin_owner..general]
            .trim_start_matches(|c: char| c != '{')
            .split('}')
            .next()
            .is_some_and(|branch| branch.contains("hand_to_owner: true")),
        "plugin 팝업 수신자 분기가 그 Esc 를 렌더에 넘기지 않는다. 펼친 plugin 팝업의 다시 접기와 선언 처리는 렌더가 한다."
    );
    assert!(
        settings < receiver,
        "수신자 분기가 설정 열기 요청 분기보다 앞에 있다. 래치 노드가 받은 Esc 는 수신자 분기 전에 요청을 취소한다."
    );
}

#[test]
fn the_escape_path_runs_before_the_overlay_gate() {
    let src = read(KEYBOARD);
    let escape_call = only_at(
        &src,
        "if self.apply_escape_key_event(event) {",
        "Escape 소비 호출",
    );
    let gate = only_at(
        &src,
        "let overlay_open = self.state.window_key_holds_terminal(self.key_decision);",
        "오버레이 게이트",
    );
    assert!(
        escape_call < gate,
        "Escape 처리가 오버레이 검사 뒤에 있어 팝업의 키 입력 차단에 걸릴 수 있다."
    );
}

/// host 팝업 수신자의 Esc 는 바깥 누름 정책이 아니라 선언 `escape` 를 따른다. Ignore 팝업은 사건 시점 수신자가 되지
/// 않고(`route_escape` 가 펼치지 않은 Ignore 팝업을 건너뛰어 부모 경로로 올린다), Close·Unfocus 는 사건 시점에 포커스를
/// 풀고 그 Esc 를 넘긴다(닫기는 팝업 층 중앙 단계가 내용 결과를 본 뒤 정한다).
#[test]
fn escape_follows_the_declaration() {
    let src = read(ESCAPE_GATE);
    let start = only_at(&src, "fn escape_outcome(", "창 Esc 판정 함수");
    let gate = &src[start..];
    for gone in [
        "close_on_outside_click",
        "focused_dismissal_target",
        "PopupEscape::Ignore",
    ] {
        assert!(
            !gate.contains(gone),
            "창 Esc 판정이 `{gone}` 를 읽는다. 수신자는 단계 4 가 정하고 닫기는 선언 escape 를 따른다."
        );
    }
    let host = only_at(
        gate,
        "KeyReceiver::Popup(instance) => {",
        "host 팝업 수신자 분기",
    );
    let body = &gate[host..];
    let release = only_at(
        body,
        "self.popups.set_instance_focused(instance, false);",
        "사건 시점 포커스 해제",
    );
    assert!(
        body[release..].contains("hand_to_owner: true"),
        "Close·Unfocus 갈래가 포커스를 푼 뒤 Esc 를 넘기지 않는다"
    );
    let route = read("src/adapters/ui/input/key_route.rs");
    only_at(
        &route,
        "if p.escape != PopupEscape::Ignore || p.fold == PopupFold::Expanded {",
        "Esc 경로가 펼치지 않은 Ignore 팝업을 건너뛰는 조건",
    );
}
