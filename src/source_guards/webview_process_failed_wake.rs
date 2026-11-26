//! Windows WebView2 `ProcessFailed` 처리기가 끝난 process를 기록한 뒤 창에 그리기를 요청하는지 확인한다.
//! 다시 싣기와 Failed 표시는 다음 동기화에서 일어나고, Windows에는 WebView 폴링이 없다. 요청이 빠지면 입력이
//! 올 때까지 WebView2 오류 페이지가 남는다. 이 처리기는 Windows 빌드에서만 컴파일되므로 모양을 여기서 고정해
//! 모든 OS의 시험에서 잰다.
//!
//! 이 가드는 코드를 고치다 생기는 실수를 잡는다: 요청을 지우기, 요청을 조건문·클로저 안으로 옮기기, 상태를
//! 바꾼 뒤 요청 전에 돌아가는 분기(`return`·`?`) 더하기. 일부러 꾸민 우회는 막으려 하지 않는다. 구조는
//! 주석·리터럴을 가린 공용 사본(`tasty_doc_guards::match_arms::Source`)에서 읽는다.
//!
//! 잡지 못하는 것: 매크로로 감싼 호출, 같은 이름의 다른 함수로 바꿔 끼운 요청, WebView2가 처리기를 부르지
//! 않거나 요청이 프레임을 만들지 않는 경우(Windows 실행으로 확인한다).

use tasty_doc_guards::match_arms::{Source, matching_close};

use super::repo_root;

const HANDLER: &str = "ProcessFailedEventHandler::create(";
const STATE: &str = "nav_failed.set(NavState::Failed);";
const WAKE: &str = "RedrawWindow(";
const WAKE_CALL: &str = "RedrawWindow(Some(parent),None,None,RDW_INTERNALPAINT)";

fn compact(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `text`의 `ProcessFailed` 처리기 본문에서 깨움 요청의 모양이 어긋난 곳. 맞으면 `None`이다.
fn wake_problem(text: &str) -> Option<String> {
    let source = Source::new(text);
    let code = source.code.as_str();
    if code.matches(HANDLER).count() != 1 {
        return Some(format!("`{HANDLER}` 이 하나가 아니다"));
    }
    let create = code.find(HANDLER).unwrap_or_default();
    // 처리기 클로저 본문은 `create(` 뒤 첫 `{`부터다(`Box::new(move |sender, args| -> … {`).
    let Some(open) = code[create..].find('{').map(|k| create + k) else {
        return Some("처리기 클로저 본문이 없다".to_string());
    };
    let Some(close) = matching_close(code, open) else {
        return Some("처리기 클로저 본문이 닫히지 않는다".to_string());
    };
    let body = &code[open..=close];
    if body.matches(WAKE).count() != 1 {
        return Some(format!("처리기 본문의 `{WAKE}` 호출이 하나가 아니다"));
    }
    if body.matches(STATE).count() != 1 {
        return Some(format!("처리기 본문의 `{STATE}` 이 하나가 아니다"));
    }
    let wake = open + body.find(WAKE).unwrap_or_default();
    let state = open + body.find(STATE).unwrap_or_default();
    // 인자 안의 `Some(parent)` 까지 읽도록 호출 끝은 짝 괄호로 찾는다.
    let call = paren_close(code, wake + WAKE.len() - 1).map_or("", |end| &code[wake..=end]);
    if compact(call) != WAKE_CALL {
        return Some(format!("깨움 호출이 `{WAKE_CALL}` 이 아니다"));
    }
    if wake < state {
        return Some("깨움 호출이 Failed 기록보다 앞에 있다".to_string());
    }
    let between = &code[state..wake];
    if between.contains("return") || between.contains('?') {
        return Some("Failed 기록과 깨움 호출 사이에 돌아가는 분기가 있다".to_string());
    }
    // 깨움은 처리기 본문의 최상위 문장 `if !RedrawWindow(…)` 의 조건이다. 감싼 블록이 더 있으면 조건 안이다.
    let depth = (open..wake)
        .filter(|&i| code.as_bytes()[i] == b'{')
        .filter(|&i| matching_close(code, i).is_some_and(|c| c > wake))
        .count();
    let head_start = code[open..wake]
        .rfind([';', '{', '}'])
        .map_or(open, |k| open + k + 1);
    let head = compact(&code[head_start..wake]);
    if depth != 1 || head != "if!" {
        return Some(format!(
            "깨움 호출이 처리기 본문의 최상위 `if !` 조건이 아니다(깊이 {depth}, 앞 `{head}`)"
        ));
    }
    None
}

/// `open` 의 `(` 와 짝인 `)` 의 자리.
fn paren_close(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in code.bytes().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn the_windows_process_failed_handler_requests_a_frame_after_recording_the_end() {
    let rel = "src/host_api/webview/windows.rs";
    let text = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    if let Some(problem) = wake_problem(&text) {
        panic!("{rel}: {problem}");
    }
}

/// 판정이 맞는 모양만 받고, 요청이 빠지거나 조건·이른 반환 뒤로 밀린 모양을 거절하는지 본다.
#[test]
fn the_check_refuses_a_missing_conditional_or_skipped_request() {
    let shape = |middle: &str, wake: &str| {
        format!(
            "let h = ProcessFailedEventHandler::create(Box::new(\n\
             move |sender, args| -> windows::core::Result<()> {{\n\
             let Some(args) = args else {{ return Ok(()) }};\n\
             nav_failed.set(NavState::Failed);\n\
             {middle}\n\
             {wake}\n\
             Ok(())\n\
             }},\n\
             ));\n"
        )
    };
    let good = "if !RedrawWindow(Some(parent), None, None, RDW_INTERNALPAINT).as_bool() {\n\
         tracing::warn!(\"{ RedrawWindow( }\");\n\
         }";
    assert_eq!(
        wake_problem(&shape("// return early?", good)),
        None,
        "주석·리터럴 속 글자는 세지 않는다"
    );
    let refused = [
        ("요청 없음", shape("", "")),
        (
            "조건 블록 안의 요청",
            shape("if renderer {", &format!("{good}\n}}")),
        ),
        (
            "조건이 붙은 요청",
            shape("", &good.replacen("if !", "if renderer && !", 1)),
        ),
        (
            "블록 안의 요청",
            shape(
                "",
                &good
                    .replacen("if !RedrawWindow(", "if !unsafe { RedrawWindow(", 1)
                    .replacen(".as_bool()", " }.as_bool()", 1),
            ),
        ),
        (
            "요청 전 이른 반환",
            shape("if renderer { return Ok(()); }", good),
        ),
        ("요청 전 `?`", shape("gate()?;", good)),
        (
            "다른 창에 요청",
            shape("", &good.replacen("Some(parent)", "Some(child)", 1)),
        ),
    ];
    for (what, text) in refused {
        assert!(wake_problem(&text).is_some(), "{what}을 받았다");
    }
}
