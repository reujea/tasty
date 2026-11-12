//! 원시 `egui::TextEdit::hint_text`에는 색을 명시한 글자를 넘긴다.
//!
//! 본체 전역 스타일은 `visuals.override_text_color = theme.text`다(`tasty_egui_theme::theme_visuals`).
//! egui는 색을 지정하지 않은 `RichText`의 색을 정할 때 `override_text_color`를 placeholder 색보다
//! 먼저 쓰므로, `&str`·`String`을 그대로 넘긴 hint는 본문색으로 그려져 이미 입력된 값처럼 보인다.
//! 그래서 hint 인자는 다음 중 하나여야 한다(docs/design/systems/theme.md "입력 전 안내").
//!
//! - `tasty_egui_theme::hint_text(theme, ..)` 호출(placeholder 토큰)
//! - `.color(`를 붙인 `RichText`
//! - 아래 명부에 이유와 함께 올린 지역 변수
//!
//! 주석·리터럴을 가린 본체·크레이트 소스에서 `.hint_text(` 호출의 괄호 짝 인자를 텍스트로 본다.
//! 헬퍼를 `use`로 줄여 부른 형태, 색 없는 `RichText`를 담은 다른 지역 변수, 인자를 만드는 함수
//! 호출은 명부에 없으면 위반으로 보고한다. 변수 안의 실제 색은 보지 않는다.

use super::rust_sources;

const ENTRY: &str = ".hint_text(";
const HELPER: &str = "tasty_egui_theme::hint_text(";
const GUARD_DIR: &str = "src/source_guards/";

/// 색을 정한 `RichText`를 바로 위에서 만들어 담는 지역 변수(파일, 변수 이름).
const COLORED_LOCALS: &[(&str, &str)] = &[
    // 활성은 헬퍼, 비활성은 state-disabled-fg를 명시한 RichText다.
    ("crates/tasty-ui-widgets/src/input.rs", "hint"),
    // 활성은 헬퍼, 비활성은 ink를 명시한 RichText에 글꼴·줄 높이를 더한다.
    ("crates/tasty-ui-widgets/src/code_area.rs", "hint"),
];

/// 현재 `.hint_text(` 호출은 13곳이다. 순회가 비거나 진입 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
const MIN_SITES: usize = 13;

/// `open`(여는 괄호 바로 뒤)부터 짝이 되는 닫는 괄호 앞까지.
fn paren_arg(code: &str, open: usize) -> &str {
    let mut depth = 1usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
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

fn colored(rel: &str, arg: &str) -> bool {
    let arg = arg.trim();
    arg.starts_with(HELPER)
        || arg.contains(".color(")
        || COLORED_LOCALS
            .iter()
            .any(|(file, local)| *file == rel && arg == *local)
}

/// 색 없는 hint를 넘긴 줄 번호와 검사한 호출 수.
fn judge(rel: &str, src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let mut bare = Vec::new();
    let mut seen = 0;
    for (at, _) in code.match_indices(ENTRY) {
        seen += 1;
        let arg = paren_arg(&code, at + ENTRY.len());
        if !colored(rel, arg) {
            bare.push(code[..at].matches('\n').count() + 1);
        }
    }
    (bare, seen)
}

#[test]
fn every_text_edit_hint_names_its_color() {
    let mut bare = Vec::new();
    let mut seen = 0;
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.starts_with(GUARD_DIR) {
            continue;
        }
        let (lines, n) = judge(&rel, &src);
        seen += n;
        bare.extend(lines.into_iter().map(|l| format!("{rel}:{l}")));
    }
    assert!(
        seen >= MIN_SITES,
        "`.hint_text(` 호출을 {seen}곳만 찾았다(하한 {MIN_SITES}). 순회 범위와 진입 문자열을 확인한다."
    );
    assert!(
        bare.is_empty(),
        "색을 정하지 않은 hint를 TextEdit에 넘겼다. 전역 override_text_color 때문에 본문색으로 그려진다. \
         tasty_egui_theme::hint_text(theme, ..)로 감싼다:\n{}",
        bare.join("\n")
    );
}

#[test]
fn the_judge_tells_a_colored_hint_from_a_bare_one() {
    let f = "src/x.rs";
    let helper = "TextEdit::singleline(v).hint_text(tasty_egui_theme::hint_text(th, \"key\"))";
    assert_eq!(judge(f, helper), (vec![], 1));
    let colored = "x\n.hint_text(egui::RichText::new(p).color(c))";
    assert_eq!(judge(f, colored), (vec![], 1));
    // 리터럴, String, 번역 호출은 색이 없다.
    assert_eq!(judge(f, "a\n.hint_text(\"key\")"), (vec![2], 1));
    assert_eq!(
        judge(f, ".hint_text(props.comment_hint.clone())"),
        (vec![1], 1)
    );
    assert_eq!(judge(f, ".hint_text(t(\"a.b\"))"), (vec![1], 1));
    // 헬퍼를 줄여 부르면 이름만 같은 다른 함수와 가릴 수 없어 위반으로 본다.
    assert_eq!(judge(f, ".hint_text(hint_text(th, \"k\"))"), (vec![1], 1));
    // 명부의 지역 변수는 그 파일에서만 통과한다.
    let local = ".hint_text(hint)";
    assert_eq!(
        judge("crates/tasty-ui-widgets/src/input.rs", local),
        (vec![], 1)
    );
    assert_eq!(judge(f, local), (vec![1], 1));
    // 주석과 문자열 안의 진입 문자열은 세지 않는다.
    assert_eq!(
        judge(f, "// .hint_text(\"a\")\nlet s = \".hint_text(\";"),
        (vec![], 0)
    );
}
