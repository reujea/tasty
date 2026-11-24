//! 숨김 보류 로드의 완료를 다시 그리기로 잇는 회차 끝 호출을 확인한다. 깨움 판정(`take_settled`)과 기록
//! (`record_pending`)의 동작은 `app::webview_sync::reveal_wake` 의 시험이 잰다. 그 판정을 이벤트 루프가
//! 부르지 않거나 깨움이 소유 창에 다시 그리기를 요청하지 않으면 그 시험은 보지 못하므로 두 자리의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출을 두되 그 앞에서 돌아가는 새 분기, OS 콜백이 이 회차 뒤에 오는 이벤트 루프.

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

/// `pos` 를 감싸는 중괄호 블록의 깊이와, 가장 안쪽 블록을 연 `{` 앞의 머리(직전 문장 경계 뒤부터)를 돌려준다.
/// 문자열·문자 리터럴 안의 중괄호는 세지 않는다. `body` 는 공백을 지운 함수 본문이라 함수 자신의 블록이 깊이 1이다.
fn enclosing_block(body: &str, pos: usize) -> (usize, String) {
    let bytes = body.as_bytes();
    let mut opens: Vec<usize> = Vec::new();
    let mut in_str = false;
    let mut i = 0;
    while i < pos {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'\'' && i + 2 < pos && bytes[i + 2] == b'\'' {
            i += 2;
        } else if c == b'{' {
            opens.push(i);
        } else if c == b'}' {
            opens.pop();
        }
        i += 1;
    }
    let head = opens.last().map_or_else(String::new, |&open| {
        let start = body[..open].rfind([';', '{', '}']).map_or(0, |at| at + 1);
        body[start..open].to_string()
    });
    (opens.len(), head)
}

/// 이벤트 루프 회차 끝(`about_to_wait`)은 native webview 이벤트를 처리한 뒤, 미룬 다시 그리기를 실행하기 전에
/// 깨움을 부른다. 앞이면 이번 회차의 GTK 완료 신호를 놓치고, 뒤면 요청한 다시 그리기가 이번 회차에 예약되지 않는다.
/// 호출은 본문의 최상위 문장이어야 한다. 조건문·블록 안으로 옮기면 그 조건에 따라 깨움이 빠진다.
#[test]
fn the_loop_end_wakes_settled_reveals_between_the_webview_pump_and_the_repaints() {
    let src = read("src/app/event_handler.rs");
    let body = squeeze(&fn_body(&src, "fn about_to_wait(").expect("about_to_wait"));
    let pump = at(
        &body,
        "self.pump_webview_key_events();",
        "webview 이벤트 처리",
    );
    let wake = at(
        &body,
        "self.redraw_settled_webview_reveals();",
        "숨김 보류 깨움",
    );
    let repaint = at(
        &body,
        "self.drive_deferred_repaints(event_loop);",
        "미룬 다시 그리기",
    );
    assert!(pump < wake && wake < repaint, "깨움 호출의 순서가 바뀌었다");
    let (depth, head) = enclosing_block(&body, wake);
    assert_eq!(
        depth, 1,
        "깨움 호출이 `{head}` 블록 안에 있다 — 본문의 최상위 문장이어야 한다"
    );
}

/// 깨움은 판정이 참인 창에 다시 그리기를 요청한다. 요청은 판정의 참 분기 바로 안에 조건 없이 놓인다.
#[test]
fn a_settled_reveal_marks_its_own_window_dirty() {
    let src = read("src/app/webview_sync/reveal_wake.rs");
    let body = squeeze(&fn_body(&src, "fn redraw_settled_webview_reveals(").expect("wake"));
    let mark = at(&body, "main.mark_dirty();", "소유 창 다시 그리기");
    let (_, head) = enclosing_block(&body, mark);
    assert_eq!(
        head, "ifwake_window(&mutmain.webview_reveal_pending,&main.webviews)",
        "다시 그리기 요청이 깨움 판정의 참 분기 바로 안에 있지 않다"
    );
}

/// 블록 판정 도우미가 최상위·조건 블록·리터럴 안 중괄호를 구분하는지 본다.
#[test]
fn the_block_helper_tells_top_level_from_nested_statements() {
    let body = "{a();ifx{b();}letf=\"{\";c(\'{\');d();}";
    assert_eq!(
        enclosing_block(body, body.find("a()").unwrap()),
        (1, String::new())
    );
    assert_eq!(
        enclosing_block(body, body.find("b()").unwrap()),
        (2, "ifx".to_string())
    );
    assert_eq!(enclosing_block(body, body.find("d()").unwrap()).0, 1);
}
