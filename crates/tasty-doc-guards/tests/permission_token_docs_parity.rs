//! Permission::as_token의 모든 Agent 권한 토큰이 권한 문서에 있는지 확인한다.
//! 함수의 match 분기에서 토큰을 읽으며 scope가 필요한 권한은 접두만 비교한다.
//! 문서에는 IPC 메서드 이름도 섞여 있어 문서에만 있는 항목은 검사하지 않는다.

use std::path::PathBuf;

use tasty_doc_guards::manifest_text::TokenForm;

const SOURCE: &str = "crates/tasty-plugin-manifest/src/types.rs";

/// 토큰이 전부 등장해야 하는 문서. 토큰별 개방 범위 표가 있다.
const DOCS: &[&str] = &["docs/dev-guide/plugin-permissions.md"];

fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// 공용 permission_tokens 파서를 쓴다. scoped 권한은 문서의 자리표시자를 허용하려고 콜론까지 비교한다.
fn tokens_from_source(src: &str) -> Vec<String> {
    tasty_doc_guards::manifest_text::permission_tokens(src)
        .into_values()
        .map(|form| match form {
            TokenForm::Literal(t) | TokenForm::Prefixed(t) => t,
        })
        .collect()
}

#[test]
fn as_token_arms_are_parsed() {
    let tokens = tokens_from_source(&read(SOURCE));
    assert!(
        tokens.len() >= 20,
        "as_token parsing looks broken — got {} tokens: {tokens:?}",
        tokens.len()
    );
    assert!(
        tokens.iter().any(|t| t == "network"),
        "expected the `network` token among {tokens:?}"
    );
    assert!(
        tokens.iter().any(|t| t == "ipc.invoke:"),
        "expected the scoped `ipc.invoke:` prefix among {tokens:?}"
    );
}

/// 일반 토큰은 닫는 백틱까지 맞아야 긴 메서드 이름이 대신 통과하지 않는다.
/// scoped 토큰은 뒤에 자리표시자를 쓸 수 있어 콜론 뒤를 비교하지 않는다.
fn doc_mentions(text: &str, token: &str) -> bool {
    if token.ends_with(':') {
        text.contains(&format!("`{token}"))
    } else {
        text.contains(&format!("`{token}`"))
    }
}

#[test]
fn a_method_name_sharing_the_prefix_does_not_satisfy_a_token() {
    assert!(!doc_mentions(
        "호출: `notification.create` · `notification.list`",
        "notification"
    ));
    assert!(doc_mentions("| `notification` | ... |", "notification"));
    assert!(doc_mentions(
        "`ipc.invoke:<prefix>`(다른 플러그인 namespace 호출)",
        "ipc.invoke:"
    ));
}

/// 각 문서 경로를 DOCS와 독립적으로 적어 검사 대상의 누락·오타를 찾는다.
/// 대상 문서를 늘리면 대조 입력도 추가하도록 개수를 함께 비교한다.
#[test]
fn every_doc_on_the_roster_is_named_by_a_literal_of_its_own() {
    let cases: [(&str, &str); 1] = [(
        "docs/dev-guide/plugin-permissions.md",
        "토큰별 개방 범위 표",
    )];
    let tokens = tokens_from_source(&read(SOURCE));
    for (path, role) in cases {
        assert!(
            DOCS.contains(&path),
            "검사 대상에 `{path}`({role})가 없다. 삭제·이동·경로 오타를 확인한다."
        );
        let text = read(path);
        assert!(
            tokens.iter().any(|t| doc_mentions(&text, t)),
            "`{path}`({role})에서 권한 토큰을 하나도 찾지 못했다. 문서 이동과 표기를 확인한다."
        );
    }
    assert_eq!(
        DOCS.len(),
        cases.len(),
        "검사 대상 문서는 {}개인데 대조 입력은 {}개다. 대상이 늘었다면 cases에도 경로를 추가한다.",
        DOCS.len(),
        cases.len()
    );
}

#[test]
fn every_permission_token_appears_in_the_docs() {
    let tokens = tokens_from_source(&read(SOURCE));
    for doc in DOCS {
        let text = read(doc);
        let missing: Vec<&String> = tokens.iter().filter(|t| !doc_mentions(&text, t)).collect();
        assert!(
            missing.is_empty(),
            "{doc}: permission tokens missing from the doc: {missing:?}\nAdd the tokens from Permission::as_token in {SOURCE}, using backticks."
        );
    }
}
