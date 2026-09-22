//! `Permission` enum 의 모든 토큰이 권한 문서 두 곳에 등장하는지 검증한다.
//!
//! 매니페스트 작성자가 읽는 정본은 문서이고 강제되는 실체는 enum 이라, 문서에 없는
//! 토큰은 **선언할 수 있는데 아무도 모르는 권한**이 된다. 토큰은 두 문서에 나뉘어
//! 실리는데(개방 범위 표 · 개념 나열) 한쪽만 갱신해도 컴파일은 통과하므로, 수동
//! 대조로는 반복해서 놓친다.
//!
//! 토큰의 단일 출처는 `Permission::as_token` 이다. exhaustive match 라 variant 를
//! 늘리면 팔이 강제로 추가되고, 그 팔의 문자열 리터럴이 곧 전체 토큰 집합이다.
//! 여기서는 그 함수 본문을 소스에서 읽어 토큰을 뽑는다 — enum 을 런타임에 열거할
//! 방법(`strum` 류)이 없고, 있더라도 scoped variant 는 대표값을 지어내야 한다.
//!
//! 역방향(문서에만 있고 enum 에 없는 토큰)은 검사하지 않는다. 문서의 백틱 코드에는
//! `surface.read` 같은 토큰과 `surface.list` 같은 IPC 메서드명이 섞여 있어, 구조적으로
//! 구분할 방법이 없으면 오탐만 늘어난다.
//!
//! 선례: `crates/tasty-doc-guards/tests/architecture_crate_list_complete.rs` · `crates/tasty-doc-guards/tests/plugin_manifest_version_parity.rs`.

use std::path::PathBuf;

use tasty_doc_guards::manifest_text::TokenForm;

/// 토큰 문자열의 단일 출처.
const SOURCE: &str = "crates/tasty-plugin-manifest/src/types.rs";

/// 토큰이 전부 등장해야 하는 문서. 앞은 토큰별 개방 범위 표, 뒤는 개념 나열.
const DOCS: &[&str] = &[
    "docs/dev-guide/plugin-permissions.md",
    "docs/concepts/plugins.md",
];

/// 레포 루트 — 이 크레이트가 `crates/` 아래 살아서 `CARGO_MANIFEST_DIR` 이 레포 루트가
/// 아니다. 해석과 검증을 [`tasty_doc_guards::repo_root`] 한 곳에 모은다(ADR-0138).
fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `as_token` 본문의 match 팔에서 토큰을 뽑는다 — 판독은 `manifest_text::permission_tokens`
/// 한 벌이다(같은 함수를 읽는 판독기가 둘이면 한쪽만 고쳐져 갈린다).
///
/// - `Self::SurfaceRead => "surface.read".into(),` → `surface.read` (그대로 검색)
/// - `Self::IpcInvoke(prefix) => format!("ipc.invoke:{prefix}"),` → `ipc.invoke:`
///   (scope 값은 매니페스트가 정하므로 문서에는 `ipc.invoke:<prefix>` 처럼 적힌다 —
///   `:` 까지만 대조한다)
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
    // 파서가 조용히 빈 목록을 돌려주면 이 테스트 전체가 무력해진다.
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

/// 문서가 이 토큰을 **그 토큰으로서** 적었는가.
///
/// scope 없는 토큰은 `` `surface.read` `` 처럼 닫는 백틱까지 정확히 일치해야 한다.
/// 여는 백틱 + prefix 만 보면 `` `surface.read_since_mark` `` 같은 **IPC 메서드명**이
/// 토큰 행을 대신 만족시켜, 토큰을 통째로 지워도 통과하는 무력한 가드가 된다.
///
/// scoped 토큰(`ipc.invoke:` 처럼 `:` 로 끝남)만 예외다 — scope 값은 매니페스트가
/// 정하므로 문서에는 `ipc.invoke:<prefix>` 같은 자리표시자와 함께 적힌다.
fn doc_mentions(text: &str, token: &str) -> bool {
    if token.ends_with(':') {
        text.contains(&format!("`{token}"))
    } else {
        text.contains(&format!("`{token}`"))
    }
}

#[test]
fn a_method_name_sharing_the_prefix_does_not_satisfy_a_token() {
    // `notification` 토큰 행을 지우고 `notification.create` 만 남긴 문서는 통과하면 안 된다.
    assert!(!doc_mentions(
        "호출: `notification.create` · `notification.list`",
        "notification"
    ));
    assert!(doc_mentions("| `notification` | ... |", "notification"));
    // scoped 는 자리표시자가 뒤따라도 인정한다.
    assert!(doc_mentions(
        "`ipc.invoke:<prefix>`(다른 플러그인 namespace 호출)",
        "ipc.invoke:"
    ));
}

/// 명부의 문서마다, 그 경로를 **손으로 적은 조각**으로 붙든다.
///
/// 실측 2026-09-06 (트리 9c1419aa2): `DOCS` 에서 첫 원소를 지우고
/// `cargo test -p tasty-doc-guards` 를 돌리면 rc=0 이었다 — 검사 대상 둘 중 하나가
/// 통째로 빠졌는데 조용했다. 명부는 **빼는 쪽이 느슨해지는** 극성이라, 원소가 줄면
/// 순회가 덜 돌 뿐 아무 단정도 안 걸린다.
///
/// 조각을 `DOCS` 에서 만들지 않고 손으로 적는다. 명부를 순회해 조각을 지으면 오타 난
/// 항목(`plugin-permission.md`)도 자기 자신과는 맞아 통과한다 — 그러면 이 테스트가
/// 명부의 사본이 될 뿐 명부를 검사하지 않는다. 조각당 문서 하나만 담는다.
///
/// 등호로 크기를 함께 붙든다. 문서를 **더하는** 방향은 검사가 늘어 더 엄격하지만,
/// 조각 없이 늘면 그 새 항목이 다시 아무에게도 안 지켜진다.
#[test]
fn every_doc_on_the_roster_is_named_by_a_literal_of_its_own() {
    let cases: [(&str, &str); 2] = [
        (
            "docs/dev-guide/plugin-permissions.md",
            "토큰별 개방 범위 표",
        ),
        ("docs/concepts/plugins.md", "개념 나열"),
    ];
    let tokens = tokens_from_source(&read(SOURCE));
    for (path, role) in cases {
        assert!(
            DOCS.contains(&path),
            "`{path}`({role})가 명부에 없다 — 지워졌거나 철자가 틀렸다. 그 문서는 이제 \
             대조되지 않으므로, 거기서 토큰이 사라져도 아무도 모른다"
        );
        // 명부에 있기만 하고 그 문서가 토큰을 하나도 안 실으면 그 항목은 죽은 자리다.
        // 판정은 흉내 내지 않고 이 파일의 술어를 그대로 부른다.
        let text = read(path);
        assert!(
            tokens.iter().any(|t| doc_mentions(&text, t)),
            "`{path}`({role})가 명부에 있는데 토큰을 하나도 안 싣는다 — 문서가 옮겨졌거나 \
             표기가 바뀌었다. 이 상태의 초록은 '전부 실렸다' 가 아니라 '아무것도 안 봤다' 다"
        );
    }
    assert_eq!(
        DOCS.len(),
        cases.len(),
        "명부가 {} 개인데 조각은 {} 개다 — 문서를 더했으면 위 `cases` 에도 그 경로를 \
         리터럴로 적어라. 안 적으면 새 항목은 다시 아무에게도 안 지켜진다",
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
            "{doc}: permission tokens missing from the doc: {missing:?}\n\
             `Permission::as_token` in {SOURCE} is the source of truth — add each token \
             (in backticks) to the doc, or the permission stays undiscoverable to plugin authors."
        );
    }
}
