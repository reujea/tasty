//! 버튼에 붙는 egui 팝오버는 `tasty_egui_theme::with_popover_frame`(또는 안쪽 둘레를 받는
//! `with_popover_frame_ring`) 클로저 안에서 열어야 메뉴 틀
//! (menu-bg · menu-border · menu-radius)로 그려진다. 전역 스타일은 tooltip 토큰을 담으므로 감싸지 않은
//! 팝오버는 tooltip 틀로 그려진다(docs/design/systems/theme.md).
//!
//! 주석을 가린 본체·크레이트 소스에서 세 진입 호출을 찾고, 바로 앞 코드가 래퍼 호출의 `|ui| {` 클로저
//! 시작인지 텍스트로 확인한다. 래퍼를 다른 함수로 한 번 더 감싼 호출, `Frame::popup`을 직접 그리는 경로,
//! 진입 호출을 `use`로 줄여 부른 형태는 보지 않는다.
//!
//! 열린 ComboBox 목록의 행은 공용 옵션 행(`menu_option` · `menu_option_value`)으로 그린다. egui 기본
//! 선택 행(`selectable_value` · `selectable_label`)은 현재 값을 accent 채움으로 칠하므로 디자인의 선택
//! 표현(글자 + 오른쪽 체크, 채움 없음)과 다르다. `show_ui` 클로저 본문을 중괄호 짝으로 잘라 확인한다.
//! remote_tool 폼은 ComboBox 를 도우미 `combo(..)` 로 그리고 목록 행은 호출자가 마지막 클로저 인자로 넘긴다. 그
//! 파일을 문자열·문자 리터럴과 주석을 바이트 정렬로 가린 뒤, `combo(` 호출마다 괄호 짝으로 인자 범위를 자르고
//! 깊이 0 의 쉼표로 나눈 마지막 최상위 인자가 `|이름| { .. }` 클로저인지 본다. 그 본문에 egui 기본 선택 행이 없고,
//! 공용 옵션 행 함수를 정확한 이름으로 부르며 첫 인자로 그 클로저의 매개변수를 넘기는지 본다. 다른 파일의 도우미나
//! 클로저를 변수로 넘긴 호출은 보지 않는다.

use super::rust_sources;

const ENTRIES: &[&str] = &[
    "egui::popup_below_widget(",
    "egui::popup::popup_above_or_below_widget(",
    "egui::ComboBox::from_id_salt(",
];
/// 기본 둘레 래퍼와 둘레를 받는 래퍼. 어느 쪽이든 메뉴 틀을 정한다.
const WRAPPERS: &[&str] = &["with_popover_frame(", "with_popover_frame_ring("];
const GUARD_DIR: &str = "src/source_guards/";
/// 현재 감싼 호출부는 13곳이다. 순회가 비거나 진입 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
const MIN_SITES: usize = 13;

/// `at`의 진입 호출이 래퍼 호출의 클로저 본문 첫 식인지 본다.
fn opened_inside_the_wrapper(code: &str, at: usize) -> bool {
    let Some(head) = code[..at].trim_end().strip_suffix('{') else {
        return false;
    };
    let Some(head) = head.trim_end().strip_suffix("|ui|") else {
        return false;
    };
    WRAPPERS.iter().any(|wrapper| {
        head.rfind(wrapper)
            .is_some_and(|w| !head[w..].contains([';', '{', '}']))
    })
}

/// 감싸지 않은 진입 호출의 줄 번호와 검사한 진입 호출 수.
fn judge(src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_comments_aligned(src);
    let mut bare = Vec::new();
    let mut seen = 0;
    for entry in ENTRIES {
        for (at, _) in code.match_indices(entry) {
            seen += 1;
            if !opened_inside_the_wrapper(&code, at) {
                bare.push(code[..at].matches('\n').count() + 1);
            }
        }
    }
    (bare, seen)
}

#[test]
fn every_button_popover_opens_inside_the_menu_frame_wrapper() {
    let mut bare = Vec::new();
    let mut seen = 0;
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.starts_with(GUARD_DIR) {
            continue;
        }
        let (lines, n) = judge(&src);
        seen += n;
        bare.extend(lines.into_iter().map(|l| format!("{rel}:{l}")));
    }
    assert!(
        seen >= MIN_SITES,
        "팝오버 진입 호출을 {seen}곳만 찾았다(하한 {MIN_SITES}). 순회 범위와 진입 문자열을 확인한다."
    );
    assert!(
        bare.is_empty(),
        "`with_popover_frame` 밖에서 연 팝오버가 있다. 감싸지 않으면 메뉴 틀 대신 tooltip 틀로 그려진다:\n{}",
        bare.join("\n")
    );
}

#[test]
fn the_judge_tells_a_wrapped_popover_from_a_bare_one() {
    let wrapped = "tasty_egui_theme::with_popover_frame(ui, &th, |ui| {\n    egui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {});\n});";
    assert_eq!(judge(wrapped), (vec![], 1));
    let bare = "let x = 1;\negui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {});";
    assert_eq!(judge(bare), (vec![2], 1));
    // 래퍼 클로저 안이라도 앞에 다른 문장이 있으면 진입 호출이 래퍼의 첫 식이 아니다.
    let later = "with_popover_frame(ui, th, |ui| {\n    let y = 2;\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(later), (vec![3], 1));
    // 다른 함수의 `|ui| {` 클로저는 래퍼로 치지 않는다.
    let other = "with_popover_frame(ui, th, |ui| {});\nui.horizontal(|ui| {\n    egui::popup::popup_above_or_below_widget(ui, id, &r, a, b, |ui| {});\n});";
    assert_eq!(judge(other), (vec![3], 1));
    // 둘레를 받는 래퍼도 래퍼다. 이름만 비슷한 다른 함수는 래퍼가 아니다.
    let ring = "tasty_egui_theme::with_popover_frame_ring(ui, th, egui::Margin::ZERO, |ui| {\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(ring), (vec![], 1));
    let lookalike = "with_popover_frame_like(ui, th, |ui| {\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(lookalike), (vec![2], 1));
    // 주석 안의 진입 문자열은 세지 않는다.
    assert_eq!(judge("// egui::popup_below_widget(\n"), (vec![], 0));
}

/// egui 기본 선택 행. 열린 ComboBox 목록 안에서는 쓰지 않는다.
const EGUI_SELECTABLE: &[&str] = &["selectable_value(", "selectable_label("];

/// `open` 위치의 `{` 와 짝이 되는 `}` 까지의 본문.
fn brace_body(code: &str, open: usize) -> &str {
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &code[open..open + i];
                }
            }
            _ => {}
        }
    }
    &code[open..]
}

/// egui 기본 선택 행을 쓰는 ComboBox 의 줄 번호와 검사한 ComboBox 수.
fn combo_rows(src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_comments_aligned(src);
    let mut offenders = Vec::new();
    let mut seen = 0;
    for (at, _) in code.match_indices("egui::ComboBox::from_id_salt(") {
        let Some(rel) = code[at..].find(".show_ui(ui, |ui| {") else {
            continue;
        };
        seen += 1;
        let open = at + rel + ".show_ui(ui, |ui| ".len();
        if EGUI_SELECTABLE
            .iter()
            .any(|e| brace_body(&code, open).contains(e))
        {
            offenders.push(code[..at].matches('\n').count() + 1);
        }
    }
    (offenders, seen)
}

/// 목록 행을 호출자 클로저로 받는 ComboBox 도우미를 쓰는 파일과 그 호출 문자열.
const HELPER_COMBO_FILE: &str = "src/adapters/ui/popup/remote_tool.rs";
const HELPER_COMBO_CALL: &str = "combo(";
/// 공용 옵션 행 함수.
const SHARED_ROWS: &[&str] = &["menu_option", "menu_option_value"];

/// `open` 위치의 `(` 와 짝이 되는 `)` 의 위치. `code` 는 리터럴·주석을 가린 텍스트다.
fn paren_end(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 인자 범위 `args`(리터럴·주석을 가린 텍스트)의 마지막 최상위 인자. 괄호·대괄호·중괄호 깊이 0 의 쉼표로 나누고
/// 끝의 빈 인자(뒤따르는 쉼표)는 건너뛴다. 비었는지는 같은 바이트 구간의 `kept`(주석만 가리고 리터럴은 남긴 텍스트)로
/// 본다. 가린 문자·문자열 리터럴 인자는 공백이지만 빈 인자가 아니다. 돌려주는 값은 그 인자의 시작 위치(범위 안
/// 상대)와 앞뒤 공백을 뺀 가린 텍스트다.
fn last_top_level_arg<'a>(args: &'a str, kept: &str) -> Option<(usize, &'a str)> {
    let mut depth = 0isize;
    let mut start = 0;
    let mut last = None;
    for (i, c) in args.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                last = Some((start, &args[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = &args[start..];
    let (at, arg) = if kept[start..].trim().is_empty() {
        last?
    } else {
        (start, tail)
    };
    let lead = arg.len() - arg.trim_start().len();
    Some((at + lead, arg.trim()))
}

/// 마지막 최상위 인자가 `|이름| { .. }` 클로저 하나이면 그 매개변수 이름과 본문(중괄호 안). `kept` 는
/// `last_top_level_arg` 와 같다.
fn last_closure<'a>(args: &'a str, kept: &str) -> Option<(&'a str, &'a str)> {
    let (_, arg) = last_top_level_arg(args, kept)?;
    let rest = arg.strip_prefix('|')?;
    let name_len = rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len());
    if name_len == 0 {
        return None;
    }
    let body = rest[name_len..]
        .strip_prefix('|')?
        .trim_start()
        .strip_prefix('{')?
        .strip_suffix('}')?;
    Some((&rest[..name_len], body))
}

/// 본문(리터럴·주석을 가린 텍스트)이 공용 옵션 행 함수를 정확한 이름으로 부르고 첫 인자로 `param` 을 넘기는지.
fn calls_shared_row(body: &str, param: &str) -> bool {
    SHARED_ROWS.iter().any(|row| {
        body.match_indices(&format!("{row}(")).any(|(at, _)| {
            if body[..at].chars().next_back().is_some_and(is_ident) {
                return false;
            }
            let arg = body[at + row.len() + 1..].trim_start();
            arg.strip_prefix(param)
                .is_some_and(|r| r.trim_start().starts_with(','))
        })
    })
}

/// 도우미 `combo(` 호출 중 마지막 최상위 인자(목록 행 클로저)가 egui 기본 선택 행을 쓰거나 공용 옵션 행을 그
/// 클로저의 `Ui` 로 부르지 않는 호출의 줄 번호와 검사한 호출 수. 판정은 모두 문자열·문자 리터럴과 주석을 바이트
/// 정렬로 가린 텍스트에서 한다. 앞 글자가 식별자·`.` 이면 다른 이름이고, `fn combo(` 는 정의다. 마지막 인자가
/// 클로저가 아닌 호출도 위반이다.
fn helper_combo_rows(src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let kept = tasty_doc_guards::source_text::mask_comments_aligned(src);
    let mut offenders = Vec::new();
    let mut seen = 0;
    for (at, _) in code.match_indices(HELPER_COMBO_CALL) {
        let before = code[..at].chars().next_back();
        if before.is_some_and(|c| is_ident(c) || c == '.') || code[..at].ends_with("fn ") {
            continue;
        }
        seen += 1;
        let open = at + HELPER_COMBO_CALL.len() - 1;
        let line = code[..at].matches('\n').count() + 1;
        let Some(close) = paren_end(&code, open) else {
            offenders.push(line);
            continue;
        };
        let args = &code[open + 1..close];
        let ok = last_closure(args, &kept[open + 1..close]).is_some_and(|(param, body)| {
            !EGUI_SELECTABLE.iter().any(|e| body.contains(e)) && calls_shared_row(body, param)
        });
        if !ok {
            offenders.push(line);
        }
    }
    (offenders, seen)
}

#[test]
fn every_combo_helper_list_draws_shared_option_rows() {
    let src = rust_sources()
        .into_iter()
        .find(|(rel, _)| rel.to_string_lossy().replace('\\', "/") == HELPER_COMBO_FILE)
        .map(|(_, src)| src)
        .unwrap_or_else(|| panic!("{HELPER_COMBO_FILE} 를 순회에서 찾지 못했다"));
    let (offenders, seen) = helper_combo_rows(&src);
    // 현재 도우미 호출은 5곳이다. 순회가 비거나 호출 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
    assert!(
        seen >= 5,
        "{HELPER_COMBO_FILE} 의 combo( 호출을 {seen}곳만 찾았다(하한 5). 호출 문자열을 확인한다."
    );
    assert!(
        offenders.is_empty(),
        "combo( 의 목록 행이 egui 기본 선택 행을 쓰거나 공용 menu_option · menu_option_value 를 쓰지 않는다:\n{}",
        offenders
            .iter()
            .map(|l| format!("{HELPER_COMBO_FILE}:{l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_helper_combo_judge_reads_the_row_closure() {
    let shared = "fn combo(ui: &mut Ui, items: impl FnOnce(&mut Ui)) {}\ncombo(ui, pen, \"a\", sel, None, |ui| {\n    menu_option_value(ui, th, &mut v, 1, \"x\");\n});\nui.selectable_value(&mut w, 2, \"y\");";
    assert_eq!(helper_combo_rows(shared), (vec![], 1));
    let egui_row = "\ncombo(ui, pen, \"a\", sel, None, |ui| {\n    ui.selectable_value(&mut v, 1, \"x\");\n    menu_option(ui, th, false, \"y\");\n});";
    assert_eq!(helper_combo_rows(egui_row), (vec![2], 1));
    let no_shared = "combo(ui, pen, \"a\", sel, None, |ui| {\n    ui.label(\"x\");\n});\nreport_combo(x);\nself.combo(y);";
    assert_eq!(helper_combo_rows(no_shared), (vec![1], 1));
    // 행 클로저 매개변수 이름이 달라도 그 호출의 마지막 클로저를 본다. 뒤 호출의 공용 행으로 통과하지 않는다.
    let renamed = "combo(ui, pen, \"a\", sel, None, |rows| {\n    rows.selectable_value(&mut v, 1, \"x\");\n});\nform_row(ui, th, t(\"b\"), |ui| {\n    menu_option_value(ui, th, &mut w, 2, \"y\");\n});";
    assert_eq!(helper_combo_rows(renamed), (vec![1], 1));
    // 공용 행은 정확한 이름의 호출이고 첫 인자가 그 클로저의 매개변수다.
    let lookalike = "combo(ui, pen, \"a\", sel, None, |ui| {\n    menu_option_valuex(ui, th, &mut v, 1, \"x\");\n});";
    assert_eq!(helper_combo_rows(lookalike), (vec![1], 1));
    let outer_ui = "combo(ui, pen, \"a(\", sel, None, |rows| {\n    menu_option_value(ui, th, &mut v, 1, \"x\");\n});";
    assert_eq!(helper_combo_rows(outer_ui), (vec![1], 1));
    let rows_ok = "combo(ui, pen, \"a\", sel, None, |rows| {\n    tasty_ui_widgets::menu_option_value(rows, th, &mut v, 1, \"x\");\n},);";
    assert_eq!(helper_combo_rows(rows_ok), (vec![], 1));
    // 마지막 인자가 클로저가 아니면 위반이다.
    let trailing =
        "combo(ui, pen, \"a\", sel, |ui| {\n    menu_option(ui, th, false, \"x\");\n}, None);";
    assert_eq!(helper_combo_rows(trailing), (vec![1], 1));
    // 문자열 안의 공용 행 이름은 호출이 아니다.
    let in_string =
        "combo(ui, pen, \"a\", sel, None, |rows| {\n    rows.label(\"menu_option(rows,\");\n});";
    assert_eq!(helper_combo_rows(in_string), (vec![1], 1));
    // 행 클로저 안의 중첩 클로저는 마지막 인자가 아니다. 바깥 클로저 본문 전체를 본다.
    let nested = "combo(ui, pen, \"a\", sel, None, |rows| {\n    rows.horizontal(|inner| {\n        inner.add_space(1.0);\n    });\n    menu_option_value(rows, th, &mut v, 1, \"x\");\n});";
    assert_eq!(helper_combo_rows(nested), (vec![], 1));
    // 문자열 안의 짝 없는 중괄호는 인자 범위를 넘기지 않는다. 뒤 호출은 따로 판정한다.
    let open_brace = "combo(ui, pen, \"a\", sel, None, |rows| {\n    rows.label(\"{\");\n    menu_option(rows, th, false, \"x\");\n});\ncombo(ui, pen, \"b\", sel, None, |ui| {\n    ui.selectable_value(&mut v, 1, \"x\");\n});";
    assert_eq!(helper_combo_rows(open_brace), (vec![5], 2));
    // 가린 리터럴 마지막 인자는 끝 쉼표가 아니다. 마지막 인자가 클로저가 아니므로 위반이다.
    let literal_tail = "combo(ui, pen, \"a\", sel, |rows| {\n    menu_option(rows, th, false, \"x\");\n}, \"tail\");";
    assert_eq!(helper_combo_rows(literal_tail), (vec![1], 1));
    let char_tail =
        "combo(ui, pen, \"a\", sel, |rows| {\n    menu_option(rows, th, false, \"x\");\n}, 'x');";
    assert_eq!(helper_combo_rows(char_tail), (vec![1], 1));
}

#[test]
fn every_open_combo_box_list_draws_shared_option_rows() {
    let mut offenders = Vec::new();
    let mut seen = 0;
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.starts_with(GUARD_DIR) {
            continue;
        }
        let (lines, n) = combo_rows(&src);
        seen += n;
        offenders.extend(lines.into_iter().map(|l| format!("{rel}:{l}")));
    }
    // 현재 ComboBox 는 3곳이다(시험 하네스 1곳 포함). 순회가 비거나 진입 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
    assert!(
        seen >= 3,
        "ComboBox 를 {seen}곳만 찾았다(하한 3). 순회 범위와 진입 문자열을 확인한다."
    );
    assert!(
        offenders.is_empty(),
        "열린 ComboBox 목록이 egui 기본 선택 행을 쓴다. 공용 menu_option · menu_option_value 로 그린다:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_combo_judge_tells_shared_rows_from_egui_selectable_rows() {
    let shared = "egui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {\n    menu_option_value(ui, th, &mut v, 1, \"x\");\n});\nui.selectable_value(&mut w, 2, \"y\");";
    assert_eq!(combo_rows(shared), (vec![], 1));
    let egui_row = "\negui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {\n    if ui.selectable_label(true, \"x\").clicked() {}\n});";
    assert_eq!(combo_rows(egui_row), (vec![2], 1));
}
