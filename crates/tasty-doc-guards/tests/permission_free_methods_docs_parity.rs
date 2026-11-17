//! plugin_callable 이면서 요구 토큰이 0개인 메서드, 곧 Agent 게이트가 토큰 없이 통과시키는 메서드와
//! plugin-permissions 문서의 목록을 양방향으로 대조한다. plugin_only 메서드도 이 집합에 들지만 Agent 는
//! 외부 호출에서 -32016 으로 거절되므로 문서가 따로 적는다.
//! 권한별 표에는 토큰을 요구하지 않는 메서드가 빠질 수 있어 별도 목록이 필요하다.
//! Plugin 호출자는 토큰을 보유하지 않고 메서드의 plugin_callable 여부만 검사받으므로 이 목록과 무관하다.
//! METHOD_TABLE은 공용 텍스트 파서로 읽으며, 런타임 값과의 일치는 tests/method_table_readings_agree.rs에서 확인한다.
//! 문서의 분류 사유가 맞는지는 판단하지 않는다.

use std::collections::BTreeSet;

const DOC: &str = "docs/dev-guide/plugin-permissions.md";

/// 표를 찾는 기준. 문서에 표가 여럿이라 헤더 행으로 특정한다.
const TABLE_HEADER: &str = "| 군 | 메서드 | 왜 토큰을 요구하지 않나 |";

/// 백틱 안의 이름만 정확히 대조해 메서드 이름의 접미사 오타도 찾는다.
fn code_spans(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn doc_text() -> String {
    let path = tasty_doc_guards::repo_root().join(DOC);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {DOC}: {e}"))
}

/// `METHOD_TABLE` 판독 — 메서드 → (plugin 이 부를 수 있으면) 요구 variant 목록.
fn method_table() -> std::collections::BTreeMap<String, Option<Vec<String>>> {
    let path = tasty_doc_guards::repo_root().join("crates/tasty-ipc/src/method_meta.rs");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    tasty_doc_guards::method_table(&src)
}

/// 표의 메서드 열에 적힌 메서드 이름을 **적힌 순서 그대로**(중복 포함) 모은다.
fn doc_methods(text: &str) -> Vec<String> {
    let after_header = text
        .split_once(TABLE_HEADER)
        .unwrap_or_else(|| panic!("{DOC}: `{TABLE_HEADER}` 표 헤더를 찾지 못했다"))
        .1;
    after_header
        .lines()
        .skip(1) // 헤더 바로 다음 줄은 `|---|---|---|` 구분선
        .take_while(|l| l.trim_start().starts_with('|'))
        .flat_map(|line| {
            let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
            if cells.len() != 3 {
                panic!("{DOC}: 군 표 행의 열 수가 3이 아니다: {line}");
            }
            if cells[0].trim().starts_with("---") {
                return Vec::new();
            }
            let methods = code_spans(cells[1]);
            assert!(
                !methods.is_empty(),
                "{DOC}: 군 표 행의 메서드 열이 비었다: {line}"
            );
            methods
        })
        .collect()
}

/// `plugin_callable` 이면서 요구 권한이 0개인 메서드 — 이 가드가 지키는 집합.
fn permission_free_methods() -> BTreeSet<String> {
    method_table()
        .into_iter()
        .filter_map(|(name, required)| match required {
            // `None` = plugin_callable 이 아니라 Local 만 부른다. `Some(v)` 에서 v 가 비면 토큰 0개로 열려 있다.
            Some(v) if v.is_empty() => Some(name),
            _ => None,
        })
        .collect()
}

#[test]
fn every_permission_free_method_is_documented() {
    let listed: BTreeSet<String> = doc_methods(&doc_text()).into_iter().collect();

    let missing: Vec<String> = permission_free_methods()
        .into_iter()
        .filter(|m| !listed.contains(m))
        .collect();
    assert!(
        missing.is_empty(),
        "Agent 게이트가 토큰 0개로 통과시키는 plugin_callable 메서드인데 {DOC} 에 없는 메서드: {missing:?}\n\
         `method_meta.rs` 에 `plugin(&[])` 을 추가했으면 그 문서의 군 표에도 적어라 — \
         어느 군인지(근거가 무엇인지)까지 골라야 한다."
    );
}

#[test]
fn every_documented_method_is_permission_free() {
    let actual = permission_free_methods();

    let stray: Vec<String> = doc_methods(&doc_text())
        .into_iter()
        .filter(|m| !actual.contains(m.as_str()))
        .collect();
    assert!(
        stray.is_empty(),
        "{DOC} 의 군 표에 있으나 요구 권한 0개가 아닌(또는 없는) 메서드: {stray:?}\n\
         권한을 붙였거나 메서드를 없앴으면 표에서도 빼라."
    );
}

/// 표와 별개로 본문에 적힌 개수도 대조한다.
#[test]
fn the_prose_count_matches_the_table() {
    let text = doc_text();
    let n = permission_free_methods().len();
    let expected = format!("`required` 가 빈 {n}개다");
    assert!(
        text.contains(&expected),
        "{DOC}: 본문의 개수 표기가 실제({n}개)와 다르다 — \"{expected}\" 를 찾지 못했다"
    );
}

/// 양방향 집합 비교로는 같은 메서드의 중복 등록을 찾지 못하므로 별도로 확인한다.
#[test]
fn the_doc_list_has_no_duplicates() {
    let listed = doc_methods(&doc_text());
    let unique: BTreeSet<&String> = listed.iter().collect();
    assert_eq!(
        listed.len(),
        unique.len(),
        "{DOC} 의 군 표에 같은 메서드가 두 번 나온다: {listed:?}"
    );
    assert!(!listed.is_empty(), "{DOC}: 군 표가 비어 있다");
}
