//! 숨김 보류 로드의 완료를 다시 그리기로 잇는 회차 끝 호출을 확인한다. 깨움 판정(`take_settled`)과 기록
//! (`record_pending`)의 동작은 `app::webview_sync::reveal_wake` 의 시험이 잰다. 그 판정을 이벤트 루프가
//! 부르지 않거나 깨움이 소유 창에 다시 그리기를 요청하지 않으면 그 시험은 보지 못하므로 두 자리의 모양을 여기서 고정한다.
//!
//! 이 가드는 코드를 고치다 생기는 실수를 잡는다: 호출을 조건문·블록·클로저 안으로 옮기기, 속성(`#[cfg]`)을
//! 붙이기, 회차 끝의 순서 바꾸기, 다시 그리기 요청을 조건부로 만들기. 일부러 꾸민 우회는 막으려 하지 않는다.
//! 구조는 주석·리터럴을 가린 공용 사본(`tasty_doc_guards::match_arms::Source`)에서 읽는다.
//!
//! 잡지 못하는 것: 매크로로 감싼 호출(매크로가 호출을 조건에 넣어도 문장 모양은 같다), 호출을 두되 그 앞에서
//! 돌아가는 새 분기, 같은 이름의 다른 메서드로 바꿔 끼운 본문, OS 콜백이 이 회차 뒤에 오는 이벤트 루프.

use std::ops::Range;

use tasty_doc_guards::match_arms::{Source, matching_close};

use super::repo_root;

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

/// 이름이 `name` 인 함수의 본문 구간(여는 `{` 부터 닫는 `}` 까지). 정의가 하나가 아니면 실패한다.
fn body_of(source: &Source<'_>, name: &str) -> Range<usize> {
    let bodies = source.fn_bodies(name);
    assert_eq!(bodies.len(), 1, "`fn {name}` 본문이 하나가 아니다");
    bodies[0].clone()
}

/// 본문의 코드 사본에서 `needle` 의 자리. 하나가 아니면 실패한다. 주석·리터럴 안의 글자는 세지 않는다.
fn at(source: &Source<'_>, body: &Range<usize>, needle: &str, what: &str) -> usize {
    let code = source.code_slice(body);
    assert_eq!(
        code.matches(needle).count(),
        1,
        "{what}: `{needle}` 이 하나가 아니다"
    );
    body.start + code.find(needle).unwrap_or_default()
}

/// `pos` 를 감싸는 중괄호 블록의 깊이와 가장 안쪽 블록을 연 `{` 앞의 머리(직전 문장 경계 뒤부터,
/// 공백을 지운 코드 사본). 함수 자신의 블록이 깊이 1이다.
fn enclosing_block(source: &Source<'_>, body: &Range<usize>, pos: usize) -> (usize, String) {
    let code = source.code.as_str();
    let opens: Vec<usize> = (body.start..pos)
        .filter(|&i| code.as_bytes()[i] == b'{')
        .filter(|&i| matching_close(code, i).is_some_and(|close| close > pos))
        .collect();
    let head = opens.last().map_or_else(String::new, |&open| {
        let start = code[body.start..open]
            .rfind([';', '{', '}'])
            .map_or(body.start, |k| body.start + k + 1);
        code[start..open]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    });
    (opens.len(), head)
}

/// `pos` 의 호출이 홀로 선 문장인지 본다. 앞의 공백·주석을 건너뛴 글자가 문장 경계(`;` `{` `}`)여야 한다.
/// `]` 이면 속성이 붙은 문장이고, 다른 글자면 식(클로저·대입 등)의 일부다.
fn statement_start_problem(source: &Source<'_>, pos: usize) -> Option<String> {
    let before = source.code[..pos].trim_end();
    match before.chars().next_back() {
        Some(';' | '{' | '}') => None,
        Some(']') => Some("속성이 붙어 있다".to_string()),
        Some(c) => Some(format!("`{c}` 뒤에 이어진 식의 일부다")),
        None => Some("앞에 문장 경계가 없다".to_string()),
    }
}

/// 이벤트 루프 회차 끝(`about_to_wait`)은 native webview 이벤트를 처리한 뒤, 미룬 다시 그리기를 실행하기 전에
/// 깨움을 부른다. 앞이면 이번 회차의 GTK 완료 신호를 놓치고, 뒤면 요청한 다시 그리기가 이번 회차에 예약되지 않는다.
/// 호출은 본문의 최상위에 홀로 선 문장이어야 한다. 조건문·블록·클로저 안이나 속성 아래면 깨움이 빠질 수 있다.
#[test]
fn the_loop_end_wakes_settled_reveals_between_the_webview_pump_and_the_repaints() {
    let src = read("src/app/event_handler.rs");
    let source = Source::new(&src);
    let body = body_of(&source, "about_to_wait");
    let pump = at(
        &source,
        &body,
        "self.pump_webview_key_events();",
        "webview 이벤트 처리",
    );
    let wake = at(
        &source,
        &body,
        "self.redraw_settled_webview_reveals();",
        "숨김 보류 깨움",
    );
    let repaint = at(
        &source,
        &body,
        "self.drive_deferred_repaints(event_loop);",
        "미룬 다시 그리기",
    );
    assert!(pump < wake && wake < repaint, "깨움 호출의 순서가 바뀌었다");
    let (depth, head) = enclosing_block(&source, &body, wake);
    assert_eq!(
        depth, 1,
        "깨움 호출이 `{head}` 블록 안에 있다 — 본문의 최상위 문장이어야 한다"
    );
    if let Some(problem) = statement_start_problem(&source, wake) {
        panic!("깨움 호출이 홀로 선 문장이 아니다: {problem}");
    }
}

/// 깨움은 판정이 참인 창에 다시 그리기를 요청한다. 요청은 판정의 참 분기 바로 안에 조건 없이 홀로 선 문장이다.
#[test]
fn a_settled_reveal_marks_its_own_window_dirty() {
    let src = read("src/app/webview_sync/reveal_wake.rs");
    let source = Source::new(&src);
    let body = body_of(&source, "redraw_settled_webview_reveals");
    let mark = at(&source, &body, "main.mark_dirty();", "소유 창 다시 그리기");
    let (_, head) = enclosing_block(&source, &body, mark);
    assert_eq!(
        head, "ifwake_window(&mutmain.webview_reveal_pending,&main.webviews)",
        "다시 그리기 요청이 깨움 판정의 참 분기 바로 안에 있지 않다"
    );
    if let Some(problem) = statement_start_problem(&source, mark) {
        panic!("다시 그리기 요청이 홀로 선 문장이 아니다: {problem}");
    }
}

/// 도우미가 리터럴 속 중괄호·따옴표를 구조로 세지 않고, 최상위·조건 블록·클로저·속성을 가르는지 본다.
#[test]
fn the_helpers_tell_top_level_statements_from_nested_or_attached_ones() {
    let text = r####"fn probe() {
    a();
    let _q = '"';
    let _r = r"\";
    let _h = r#"{"#;
    if x { b(); }
    let _c = || c();
    #[cfg(any())]
    d();
    e(); // }
}
"####;
    let source = Source::new(text);
    let body = body_of(&source, "probe");
    let pos = |needle: &str| at(&source, &body, needle, needle);
    assert_eq!(
        enclosing_block(&source, &body, pos("a();")),
        (1, String::new())
    );
    assert_eq!(
        enclosing_block(&source, &body, pos("b();")),
        (2, "ifx".to_string())
    );
    assert_eq!(enclosing_block(&source, &body, pos("e();")).0, 1);
    assert_eq!(statement_start_problem(&source, pos("a();")), None);
    assert_eq!(statement_start_problem(&source, pos("b();")), None);
    assert_eq!(statement_start_problem(&source, pos("e();")), None);
    assert!(statement_start_problem(&source, pos("c();")).is_some());
    assert!(statement_start_problem(&source, pos("d();")).is_some());
}
