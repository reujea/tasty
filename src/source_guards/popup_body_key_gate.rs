//! host 팝업 본문이 키 입력 문(`body_keys::with_body_keys`) 안에서만 그려지는지 확인한다. 문은 이번 프레임 키
//! 주인이 아닌 본문을 그리는 동안 키·글자·붙여넣기·복사·잘라내기·IME 사건과 눌린 키 상태(`keys_down`)를 egui
//! 입력에서 뺀다. `events` 와 원래 사건 목록 `raw.events` 둘 다에서 뺀다. 그래서 본문이 egui 입력을 직접
//! 읽든(`key_pressed`·`key_down`·`consume_key`·`events`·`raw.events`) 위젯 키 포커스로 읽든 키 주인 플래그를
//! 따른다. 수식키 상태(`modifiers`)는 가리지 않는다.
//!
//! 규칙의 구조를 세 가지로 본다.
//! 1. 본문 호출 자리는 하나다. draw.rs 에서 본문 콜백 `content_fn` 이 나오는 곳은 매개변수 선언, `draw` 에서
//!    `draw_routed` 로 넘기는 곳, 문 클로저 안의 호출 하나뿐이다. 문 클로저는 본문 호출 하나만 담는다. 문의 키 플래그는
//!    이번 프레임 키 주인 값에서 나온다.
//! 2. 팝업 정의의 본문(`PopupDef::draw_fn`)은 팝업 층이 `draw_routed` 의 둘째 인자로 넘기는 본문 콜백 클로저
//!    리터럴 안에서만 부른다.
//! 3. egui 키 입력을 직접 읽는 파일은 아래 목록 중 하나다. 팝업 본문(문 안에서만 그려진다), 문·키 주인 버튼, 팝업
//!    밖(이유와 함께), 공용 위젯(부른 쪽의 문을 따른다)으로 나눈다. 새 파일이 키를 읽으면 분류를 정해야 통과한다.
//!
//! 키 읽기와 `draw_fn` 은 줄바꿈·공백으로 나눠 쓴 형태(`key_pressed\n(`, `Event::\nKey`, `def\n.draw_fn`)도 같은
//! 형태로 맞춘 뒤 본다(`joined`). 키 읽기 메서드를 값으로 담는 경로(`egui::InputState::num_presses`)도 읽기로 본다.
//! 정의 본문은 부르는 자리뿐 아니라 꺼내 쓰는 자리(`let f = def.draw_fn;`, 전달, 구조 분해. 이름을 바꾼
//! `PopupDef { draw_fn: body, .. }` 패턴 포함. 중첩 구조체·튜플·튜플 구조체 패턴, 뒤에 매개변수가 더 있는 클로저
//! 매개변수, match 갈래 가드 앞 패턴도 바깥 패턴까지 거슬러 올라가 잡는다)도 팝업 층 하나에만 있어야 한다.
//!
//! 잡지 못하는 것:
//! - 본문 모듈의 함수를 팝업 밖에서 이름으로 직접 부르는 경로(3의 목록은 파일 단위다).
//! - 위젯이 팝업 밖에서 키를 읽는 것.
//! - egui 입력이 아닌 경로(창 상태에 쌓아 둔 키 등)로 키를 읽는 것.
//! - 매크로가 만든 키 읽기.
//! - `use` 별칭으로 이름을 바꾼 사건 이름(`use egui::Event::Key as K` 등).
//! - 키 읽기를 다른 이름의 함수로 감싸 부르는 것(감싼 함수가 든 파일은 3에서 잡힌다).
//! - 구조체 리터럴 뒤에 같은 깊이의 `|` 가 이어지는 식(`f(PopupDef { draw_fn: g }, |x| x)`)은 클로저 매개변수
//!   패턴으로 잘못 보아 걸린다(놓치지 않고 과하게 잡는 쪽이다).
//! - 본문 콜백 클로저 안에서 정의 본문을 부르기 전에 다른 일을 하는 것(호출 자리만 본다).

use std::collections::BTreeSet;

use super::{matching_delim, repo_root, strip_comments};

const HOME: &str = "src/adapters/ui/popup/draw.rs";
const FRAME: &str = "src/adapters/ui/popup/frame.rs";

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    strip_comments(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n"),
    )
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `(`·`::`·`.` 앞뒤 공백(줄바꿈 포함)을 지운다. 낱말 사이 공백은 남겨 낱말 경계 판정이 그대로다.
fn joined(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(code.len());
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_whitespace() {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let end = (at..chars.len())
            .find(|&i| !chars[i].is_whitespace())
            .unwrap_or(chars.len());
        let prev = out.chars().next_back();
        let next = chars.get(end).copied();
        if !(matches!(prev, Some(':' | '.')) || matches!(next, Some('(' | ':' | '.'))) {
            out.extend(&chars[at..end]);
        }
        at = end;
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `word` 가 단어로 나오는 위치마다 그 뒤 꼬리(공백 제거).
fn word_tails(src: &str, word: &str) -> Vec<String> {
    src.match_indices(word)
        .filter(|(at, _)| {
            let before = src[..*at].chars().next_back();
            let after = src[at + word.len()..].chars().next();
            !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
        })
        .map(|(at, _)| squeeze(&src[at..(at + 160).min(src.len())]))
        .collect()
}

/// 문 클로저 안의 본문 호출. 문 표현식 전체를 공백 없이 비교한다.
const GATED_CALLS: [&str; 2] = [
    "with_body_keys(ctx,takes_keys,||{content_fn(&mutcx,ui)})",
    "with_body_keys(ctx,takes_keys,||content_fn(&mutcx,ui))",
];

#[test]
fn the_popup_body_is_called_once_inside_the_key_gate() {
    let src = read(HOME);
    let flat = squeeze(&src);
    let mut gated = 0;
    for tail in word_tails(&src, "content_fn") {
        if tail.starts_with("content_fn:&mutdynFnMut(") || tail.starts_with("content_fn,draw_ctx,")
        {
            continue;
        }
        assert!(
            tail.starts_with("content_fn(&mutcx,ui)"),
            "{HOME} 에서 본문 콜백을 문 밖에서 쓴다(별칭·전달·다른 호출): {tail}"
        );
        gated += 1;
    }
    assert_eq!(
        gated, 1,
        "{HOME} 의 본문 호출은 하나여야 한다. 둘 이상이면 문을 거치지 않는 갈래가 생긴다"
    );
    assert!(
        GATED_CALLS.iter().any(|c| flat.matches(c).count() == 1),
        "{HOME} 의 본문 호출이 문 클로저 하나 안에 있지 않다(문 클로저는 본문 호출만 담는다)"
    );
    assert_eq!(
        flat.matches("with_body_keys(").count(),
        1,
        "{HOME} 의 키 입력 문은 본문 호출 자리 하나다"
    );
    // 문의 키 플래그는 이번 프레임 키 주인 하나에서 나온다.
    for fact in [
        "letframe_keys=self.take_frame_keys();",
        "letkeys_at_start:Option<PopupInstanceId>=frame_keys.host;",
        "letframe_has_key_owner=frame_keys.held();",
        "focused:keys_at_start==Some(popup.instance),",
        "lettakes_keys=cx.focused||(!frame_has_key_owner&&!popup.arrange.takes_focus);",
    ] {
        assert_eq!(
            flat.matches(fact).count(),
            1,
            "{HOME} 의 키 플래그가 이번 프레임 키 주인 값에서 나오지 않는다: `{fact}`"
        );
    }
}

#[test]
fn popup_definition_bodies_run_only_inside_the_body_callback() {
    let frame = read(FRAME);
    let call = "(def.draw_fn)(ui, state, engine, cx)";
    assert_eq!(
        frame.matches(call).count(),
        1,
        "{FRAME} 의 팝업 본문 호출 `{call}` 이 하나가 아니다"
    );
    let at = frame.find(call).expect("본문 호출");
    // 본문 호출은 draw_routed 의 둘째 인자인 클로저 리터럴의 본문 안에 있다. 인자를 만드는 블록(`{ 호출; 콜백 }`)이나
    // 다른 인자 안의 호출은 콜백 밖이다.
    let routed = frame
        .find("popups.draw_routed(")
        .expect("팝업 층의 draw_routed 호출");
    let args = routed + "popups.draw_routed(".len();
    let head = "ctx,&mut|cx,ui|{";
    let mut seen = String::new();
    let mut body_open = None;
    for (i, c) in frame[args..].char_indices() {
        if c.is_whitespace() {
            continue;
        }
        seen.push(c);
        if !head.starts_with(&seen) {
            break;
        }
        if seen == head {
            body_open = Some(args + i);
            break;
        }
    }
    let body_open = body_open.unwrap_or_else(|| {
        panic!(
            "{FRAME} 의 draw_routed 둘째 인자가 본문 콜백 클로저 `&mut |cx, ui| {{ … }}` 가 아니다"
        )
    });
    let body_close = matching_delim(&frame, body_open).expect("본문 콜백 끝");
    assert!(
        body_open < at && at < body_close,
        "{FRAME} 의 팝업 본문 호출이 draw_routed 에 넘기는 본문 콜백 클로저 안에 있지 않다"
    );

    // 팝업 정의 본문은 다른 곳에서 부르거나 꺼내 쓰지 않는다. `draw_fn` 낱말은 필드 선언·초기화(`draw_fn:`)로만
    // 나온다. 팝업 층에서는 위 호출 하나만 읽는다. 전체화면 무대는 별도 정의(`FullscreenStageDef::draw_fn`)다.
    let mut reads_elsewhere = Vec::new();
    for (rel, text) in tasty_doc_guards::source_text::rust_sources(&repo_root(), &["src"]) {
        let rel = rel.to_string_lossy().into_owned();
        if rel == "src/adapters/ui/fullscreen.rs" || rel.starts_with("src/source_guards/") {
            continue;
        }
        let code = joined(&tasty_doc_guards::source_text::mask_non_code_aligned(
            text.as_str(),
        ));
        let uses = draw_fn_uses(&code);
        let allowed = usize::from(rel == FRAME);
        if uses.len() != allowed {
            reads_elsewhere.push(format!("{rel}: {uses:?}"));
        }
    }
    assert!(
        reads_elsewhere.is_empty(),
        "팝업 층의 본문 호출 하나 밖에서 정의 본문(`draw_fn`)을 읽는다(호출·별칭·전달·구조 분해): {reads_elsewhere:?}"
    );
}

/// `draw_fn` 낱말이 필드 선언·초기화(`draw_fn:`)가 아닌 자리. 이름을 바꾼 구조 분해(`PopupDef { draw_fn: body, .. }`
/// 패턴)는 읽는 자리로 센다. 각 자리의 앞뒤 몇 글자.
fn draw_fn_uses(code: &str) -> Vec<String> {
    code.match_indices("draw_fn")
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back();
            let after = &code[at + "draw_fn".len()..];
            if before.is_some_and(is_ident) || after.chars().next().is_some_and(is_ident) {
                return false;
            }
            let rest = after.trim_start();
            let field = rest.starts_with(':') && !rest.starts_with("::");
            !field || in_pattern(code, *at)
        })
        .map(|(at, _)| {
            let start = code[..at].char_indices().rev().nth(8).map_or(0, |(i, _)| i);
            let end = code[at..]
                .char_indices()
                .nth(16)
                .map_or(code.len(), |(i, _)| at + i);
            code[start..end].to_string()
        })
        .collect()
}

/// `at` 을 담은 가장 안쪽 `{ … }` 가 패턴(구조 분해)의 일부인지. 그 묶음부터 바깥 괄호 묶음으로 거슬러 올라가며 닫는
/// 괄호 뒤를 본다.
/// - `=`(let·if let·match 갈래 `=>`), `:`(매개변수 타입), `|`(클로저 매개변수 끝·갈래 대안), `if`(match 갈래 가드)면
///   패턴이다.
/// - 닫는 `)`·`]`·`}` 면 바깥 묶음으로 올라간다(중첩 구조체·튜플·튜플 구조체·배열 패턴).
/// - `,` 면 같은 깊이에서 뒤를 훑어 `|` 가 바깥 묶음 끝·`;` 보다 먼저 나오면 클로저 매개변수로 본다. 아니면 바깥
///   묶음으로 올라간다(뒤에 다른 요소·필드가 이어지는 패턴).
/// - 그 밖(`;`·`.`·`?`·낱말·끝)이면 식이다. 구조체 리터럴·필드 선언 뒤에는 이 갈래가 온다.
fn in_pattern(code: &str, at: usize) -> bool {
    let mut depth = 0usize;
    let mut open = None;
    for (i, c) in code[..at].char_indices().rev() {
        match c {
            '}' => depth += 1,
            '{' if depth == 0 => {
                open = Some(i);
                break;
            }
            '{' => depth -= 1,
            _ => {}
        }
    }
    let Some(mut open) = open else {
        return false;
    };
    loop {
        let Some(close) = matching_delim(code, open) else {
            return false;
        };
        let rest = code[close + 1..].trim_start();
        if (rest.starts_with('=') && !rest.starts_with("=="))
            || (rest.starts_with(':') && !rest.starts_with("::"))
            || (rest.starts_with('|') && !rest.starts_with("||"))
            || (rest.starts_with("if") && !rest[2..].chars().next().is_some_and(is_ident))
        {
            return true;
        }
        if rest.starts_with(',') {
            if closure_params_end_follows(code, code.len() - rest.len() + 1) {
                return true;
            }
        } else if !rest.starts_with([')', ']', '}']) {
            return false;
        }
        let Some(outer) = enclosing_open(code, open) else {
            return false;
        };
        open = outer;
    }
}

/// `from` 부터 같은 괄호 깊이를 훑어 `|`(클로저 매개변수 끝)가 바깥 묶음의 닫는 괄호나 `;` 보다 먼저 나오는지.
fn closure_params_end_follows(code: &str, from: usize) -> bool {
    let mut i = from;
    while let Some(c) = code[i..].chars().next() {
        match c {
            '(' | '[' | '{' => match matching_delim(code, i) {
                Some(end) => i = end + 1,
                None => return false,
            },
            ')' | ']' | '}' | ';' => return false,
            '|' => return !code[i + 1..].starts_with('|'),
            _ => i += c.len_utf8(),
        }
    }
    false
}

/// `pos` 를 담은 가장 안쪽 괄호 묶음(`(`·`[`·`{`)의 여는 자리.
fn enclosing_open(code: &str, pos: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in code[..pos].char_indices().rev() {
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth == 0 => return Some(i),
            '(' | '[' | '{' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// 팝업 정의의 본문 모듈. 문 안에서만 그려진다.
const POPUP_BODIES: &[&str] = &[
    "src/adapters/ui/info_modal.rs",
    "src/adapters/ui/mouse_capture_menu.rs",
    "src/adapters/ui/popup/approval.rs",
    "src/adapters/ui/popup/command_palette.rs",
    "src/adapters/ui/popup/confirm_close_remote.rs",
    "src/adapters/ui/popup/confirm_delete_category.rs",
    "src/adapters/ui/popup/confirm_force_detach_workspace.rs",
    "src/adapters/ui/popup/convert.rs",
    "src/adapters/ui/popup/dag_list.rs",
    "src/adapters/ui/popup/file_handler_picker.rs",
    "src/adapters/ui/popup/file_picker.rs",
    "src/adapters/ui/popup/file_picker/folder.rs",
    "src/adapters/ui/popup/port_scanner.rs",
    "src/adapters/ui/popup/preset_apply.rs",
    "src/adapters/ui/popup/rail_category.rs",
    "src/adapters/ui/popup/remote_attach.rs",
    "src/adapters/ui/popup/remote_attach/nav.rs",
    "src/adapters/ui/popup/remote_tool.rs",
    "src/adapters/ui/popup/script_confirm.rs",
    "src/adapters/ui/popup/transfer.rs",
    "src/adapters/ui/search_bar.rs",
    "src/adapters/ui/tools_menu.rs",
    "src/adapters/ui/tutorial/topic_popup.rs",
];

/// 문 자체와 이번 프레임 키 주인 값(`takes_keys`)을 받는 접힌 보기 버튼.
const GATE_AND_OWNER_BUTTONS: &[&str] = &[
    "src/adapters/ui/popup/body_keys.rs",
    "src/adapters/ui/popup/display.rs",
];

/// 팝업 본문이 아닌 곳. 그 화면의 입력 규칙을 따른다.
const OUTSIDE_POPUPS: &[(&str, &str)] = &[
    (
        "src/adapters/ui/surface/explorer.rs",
        "explorer surface 의 바로 찾기",
    ),
    ("src/adapters/ui/preset/toolbar.rs", "프리셋 편집 창"),
    (
        "src/adapters/ui/preset/surface_settings.rs",
        "프리셋 편집 창",
    ),
];

/// 공용 위젯. 팝업 본문 안에서 그리면 그 본문의 문을 따르고, surface·창에서 그리면 그쪽 입력 규칙을 따른다.
const SHARED_WIDGETS: &str = "crates/tasty-ui-widgets/src/";

const KEY_READS: [&str; 14] = [
    "key_pressed(",
    "key_released(",
    "key_down(",
    "consume_key(",
    "count_and_consume_key(",
    "consume_shortcut(",
    "num_presses(",
    "Event::Key",
    "Event::Text(",
    "Event::Paste(",
    "Event::Copy",
    "Event::Cut",
    "Event::Ime(",
    "keys_down",
];

/// 코드(문자열·주석을 가린 뒤)가 egui 키 입력을 직접 읽는지. 키 읽기 메서드를 값으로 담는 경로
/// (`let read = egui::InputState::num_presses;`)도 읽기로 본다. 메서드 값은 언제나 경로(`::이름`)로 쓴다.
fn reads_keys(code: &str) -> bool {
    let code = joined(&without_inline_tests(code));
    KEY_READS.iter().any(|r| {
        reads(&code, r)
            || r.strip_suffix('(')
                .is_some_and(|name| path_ends_with(&code, name))
    })
}

/// `read` 가 낱말로 나오는지. `Event::Key` 가 `WindowEvent::KeyboardInput` 에 걸리지 않게 앞뒤를 본다. 앞 글자가
/// 낱말 글자면 다른 이름의 꼬리라 보지 않는다(`count_and_consume_key(` 는 따로 둔다).
fn reads(code: &str, read: &str) -> bool {
    code.match_indices(read).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + read.len()..].chars().next();
        !before.is_some_and(is_ident) && (read.ends_with('(') || !after.is_some_and(is_ident))
    })
}

/// 경로가 `::name` 으로 끝나는 자리가 있는지(뒤에 낱말 글자가 이어지지 않는다).
fn path_ends_with(code: &str, name: &str) -> bool {
    let path = format!("::{name}");
    code.match_indices(&path)
        .any(|(at, _)| !code[at + path.len()..].chars().next().is_some_and(is_ident))
}

/// 인라인 시험 모듈(`#[cfg(test)] mod 이름 { … }`)을 뺀 본문.
fn without_inline_tests(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    while let Some(at) = rest.find("#[cfg(test)]") {
        let after = &rest[at + "#[cfg(test)]".len()..];
        let head = after.trim_start();
        let Some(name_end) = head
            .strip_prefix("mod ")
            .and_then(|h| h.find(|c: char| !is_ident(c)).map(|n| n + 4))
        else {
            out.push_str(&rest[..at + "#[cfg(test)]".len()]);
            rest = after;
            continue;
        };
        let skipped = after.len() - head.len();
        let body = &head[name_end..];
        let open_rel = body.len() - body.trim_start().len();
        if !body.trim_start().starts_with('{') {
            out.push_str(&rest[..at + "#[cfg(test)]".len()]);
            rest = after;
            continue;
        }
        let open = at + "#[cfg(test)]".len() + skipped + name_end + open_rel;
        let close = matching_delim(rest, open).expect("시험 모듈 끝");
        out.push_str(&rest[..at]);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn every_direct_key_read_is_classified() {
    let found: BTreeSet<String> = tasty_doc_guards::source_text::rust_sources(
        &repo_root(),
        &["src/adapters/ui", "crates/tasty-ui-widgets/src"],
    )
    .into_iter()
    .filter(|(rel, _)| {
        let rel = rel.to_string_lossy();
        let name = rel.rsplit('/').next().unwrap_or_default();
        let test_file = name.ends_with("_tests.rs")
            || name == "tests.rs"
            || rel.contains("/tests/")
            || rel.contains("virtual_tests/");
        !test_file
    })
    .filter(|(_, text)| {
        // 문자열·주석 안의 괄호와 낱말은 가린다.
        reads_keys(&tasty_doc_guards::source_text::mask_non_code_aligned(text))
    })
    .map(|(rel, _)| rel.to_string_lossy().into_owned())
    .collect();

    let listed: BTreeSet<String> = POPUP_BODIES
        .iter()
        .chain(GATE_AND_OWNER_BUTTONS)
        .copied()
        .chain(OUTSIDE_POPUPS.iter().map(|(f, _)| *f))
        .map(str::to_string)
        .collect();
    let unclassified: Vec<&String> = found
        .iter()
        .filter(|f| !listed.contains(*f) && !f.starts_with(SHARED_WIDGETS))
        .collect();
    assert!(
        unclassified.is_empty(),
        "egui 키 입력을 직접 읽는 새 파일이다. 팝업 본문이면 문 안에서만 그려지는지 확인하고 POPUP_BODIES 에, 아니면 이유와 함께 OUTSIDE_POPUPS 에 넣는다: {unclassified:?}"
    );
    let stale: Vec<&String> = listed.iter().filter(|f| !found.contains(*f)).collect();
    assert!(
        stale.is_empty(),
        "목록의 파일이 더는 키를 직접 읽지 않는다. 목록에서 뺀다: {stale:?}"
    );
}

/// 키 읽기 판정이 각 형태와 나눠 쓴 형태를 잡고, 다른 이름의 꼬리와 낱말 사이 공백에는 걸리지 않는다.
#[test]
fn the_key_read_scan_catches_each_form() {
    for code in [
        "i.consume_shortcut(&s)",
        "i.count_and_consume_key(m, k)",
        "i.num_presses(k)",
        "matches!(e, egui::Event::Copy)",
        "matches!(e, egui::Event::Cut)",
        "i.key_pressed\n    (k)",
        "i\n    .consume_key(m, k)",
        "egui::Event::\n    Key { .. }",
        "egui::Event ::Paste(p)",
        "let read = egui::InputState::num_presses;",
        "let down = egui::InputState::key_down;",
        "i.keys_down.contains(&k)",
    ] {
        assert!(reads_keys(code), "잡지 못했다: {code}");
    }
    for code in [
        "WindowEvent::KeyboardInput { .. }",
        "my_key_pressed(k)",
        "let key_pressed = 1;",
        "return key_pressed_count",
    ] {
        assert!(!reads_keys(code), "키 읽기가 아닌데 걸렸다: {code}");
    }
    assert_eq!(draw_fn_uses("PopupDef { draw_fn: f, keys }").len(), 0);
    assert_eq!(
        draw_fn_uses("let d = PopupDef { draw_fn: f, keys };").len(),
        0
    );
    assert_eq!(draw_fn_uses("defs.push(PopupDef { draw_fn: f });").len(), 0);
    assert_eq!(
        draw_fn_uses("vec![PopupDef { draw_fn: f }, other]").len(),
        0
    );
    assert_eq!(draw_fn_uses("pub draw_fn: DrawFn,").len(), 0);
    assert_eq!(
        draw_fn_uses(
            "match def { E::A(PopupDef { draw_fn: b, .. }, _) | E::B(PopupDef { draw_fn: b, .. }) => b }"
        )
        .len(),
        2,
        "갈래 대안의 두 패턴"
    );
    for code in [
        "let w = Wrapper { def: PopupDef { draw_fn: f }, x };",
        "let w = Pair(PopupDef { draw_fn: f }, y);",
        "if on { PopupDef { draw_fn: f } } else { other }",
        "defs.iter().map(|d| PopupDef { draw_fn: d.f }).collect()",
        "match k { K::A => PopupDef { draw_fn: f }, K::B => g }",
    ] {
        assert_eq!(draw_fn_uses(code).len(), 0, "리터럴인데 걸렸다: {code}");
    }
    for code in [
        "let f = def.draw_fn;",
        "let f = def\n    .draw_fn;",
        "let PopupDef { draw_fn, .. } = def;",
        "let PopupDef { draw_fn: body, .. } = def;",
        "if let Some(PopupDef { draw_fn: body, .. }) = def {",
        "match def { PopupDef { draw_fn: body, .. } => body }",
        "|PopupDef { draw_fn: body, .. }| body",
        "let (PopupDef { draw_fn: b, .. }, x) = pair;",
        "let [PopupDef { draw_fn: b, .. }, _] = defs;",
        "let Some((PopupDef { draw_fn: b, .. }, _)) = found else { return };",
        "let Pair(PopupDef { draw_fn: b, .. }, y) = pair;",
        "let Wrapper { def: PopupDef { draw_fn: b, .. } } = w;",
        "let Wrapper { def: PopupDef { draw_fn: b, .. }, x } = w;",
        "|PopupDef { draw_fn: b, .. }, x| b",
        "match def { PopupDef { draw_fn: body, .. } if enabled => body, _ => fallback }",
        "(def.draw_fn)(ui)",
        "run(def.draw_fn)",
    ] {
        assert_eq!(draw_fn_uses(code).len(), 1, "잡지 못했다: {code}");
    }
}
