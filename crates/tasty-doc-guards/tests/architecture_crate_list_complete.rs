//! 아키텍처 문서에 실제 크레이트가 모두 나오는지, 적힌 개수가 맞는지 확인한다.
//! 문서에만 있는 이름은 검사하지 않는다. 바이너리 이름도 설명에 나올 수 있기 때문이다.
//! 계층 소속과 의존 방향은 architecture_layer_order_holds에서 별도로 검사한다.

use std::path::PathBuf;

const DOC: &str = "docs/architecture/index.md";
const CRATES_DIR: &str = "crates";
const SECTION_HEADER: &str = "## 워크스페이스 크레이트 (";

fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// 매니페스트가 있는 crates/ 하위 디렉터리를 센다.
fn crate_dir_names() -> Vec<String> {
    tasty_doc_guards::crate_layers::crate_dir_names(&root())
}

fn section_count(doc: &str) -> usize {
    let line = doc
        .lines()
        .find(|line| line.starts_with(SECTION_HEADER))
        .unwrap_or_else(|| panic!("{DOC} must contain a `{SECTION_HEADER}N)` header"));
    let digits = line[SECTION_HEADER.len()..].trim_end_matches(')');
    digits
        .parse()
        .unwrap_or_else(|e| panic!("{DOC}: crate count in `{line}` is not a number: {e}"))
}

#[test]
fn every_crate_directory_is_listed() {
    let doc = read(DOC);
    let names = crate_dir_names();
    let missing: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !doc.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "{DOC} does not list these crates (write each as `name` in the workspace crate section): {missing:?}"
    );
}

#[test]
fn stated_crate_count_matches_directories() {
    let doc = read(DOC);
    let actual = crate_dir_names().len();
    let stated = section_count(&doc);
    assert_eq!(
        stated, actual,
        "{DOC}: `{SECTION_HEADER}{stated})` but {CRATES_DIR}/ holds {actual} crates"
    );
    let overview = format!("{actual} 개 크레이트(`crates/*`)");
    assert!(
        doc.contains(&overview),
        "{DOC}: the overview sentence must say `{overview}` ({CRATES_DIR}/ holds {actual} crates)"
    );
}
