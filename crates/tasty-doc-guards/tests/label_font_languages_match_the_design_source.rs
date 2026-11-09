//! `font-label`의 언어 규칙은 디자인 정본 vendor/tokens/base.css의 `:lang()` 규칙이고, 앱은 같은 집합을
//! tasty-ui-widgets의 상수로 옮겨 둔다. 내장 언어 목록의 부분 집합이라 언어 목록 대조에 넣을 수 없어 여기서
//! 두 사본을 비교한다. 정본 경로는 TASTY_PAGE_DIR로 받는다(tasty_doc_guards::design_source).

use std::collections::BTreeSet;
use tasty_doc_guards::design_source;

const APP_FILE: &str = "crates/tasty-ui-widgets/src/label_font.rs";
const APP_CONST: &str = "LABEL_UI_SANS_LANGUAGES: &[&str] = &[";
const SOURCE_FILE: &str = "vendor/tokens/base.css";
const TOKEN: &str = "--tasty-font-label:";

fn app_set() -> BTreeSet<String> {
    let path = tasty_doc_guards::repo_root().join(APP_FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{APP_FILE}: {e}"));
    let (_, rest) = text
        .split_once(APP_CONST)
        .unwrap_or_else(|| panic!("{APP_FILE}에 {APP_CONST}가 없다"));
    let (items, _) = rest.split_once(']').expect("상수 배열이 닫힌다");
    items
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// `value`를 읽는 규칙의 언어 집합. 줄 안의 `:lang(..)`만 읽는다.
fn source_set(css: &str, value: &str) -> BTreeSet<String> {
    css.lines()
        .filter(|line| line.contains(TOKEN) && line.contains(value))
        .flat_map(|line| {
            line.split(":lang(")
                .skip(1)
                .filter_map(|part| part.split_once(')').map(|(code, _)| code.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn label_font_languages_match_the_design_source() {
    let css = design_source::read(SOURCE_FILE);
    let sans = source_set(&css, "var(--tasty-font-ui)");
    let mono = source_set(&css, "var(--tasty-font-mono)");
    assert!(
        !sans.is_empty() && !mono.is_empty(),
        "{SOURCE_FILE}에서 {TOKEN} 규칙을 읽지 못했다. CSS 구조와 파서를 확인한다."
    );
    assert!(
        sans.is_disjoint(&mono),
        "{SOURCE_FILE}이 같은 언어를 두 글꼴로 정한다: sans {sans:?}, mono {mono:?}"
    );
    assert_eq!(
        app_set(),
        sans,
        "{APP_FILE}의 언어 집합이 정본 {SOURCE_FILE}과 다르다"
    );
}
