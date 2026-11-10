//! 생성기 출력과 커밋된 토큰 상수·Theme 접근자 파일을 비교한다.
//! CI에서는 check-headless 잡에서 실행하며 기본 조합의 `--lib --bins`에는 포함되지 않는다.
//! 토큰이나 생성기를 바꿨으면 커밋 전에 `cargo test -p tasty-design-tokens`를 실행한다.

use tasty_design_tokens::{DTCG_JSON, dtcg};

/// 커밋된 생성물. 생성기 출력 순서(`Generated::files`)와 같은 순서.
const COMMITTED: &[(&str, &str)] = &[
    ("mod.rs", include_str!("../src/generated/mod.rs")),
    (
        "primitive.rs",
        include_str!("../src/generated/primitive.rs"),
    ),
    ("semantic.rs", include_str!("../src/generated/semantic.rs")),
    (
        "component.rs",
        include_str!("../src/generated/component.rs"),
    ),
];

/// 커밋된 `tasty-type-appearance` 산출물. `Generated::type_appearance_files` 와 같은 순서.
const COMMITTED_TYPE_APPEARANCE: &[(&str, &str)] = &[
    (
        "semantic_color_generated.rs",
        include_str!("../../tasty-type-appearance/src/semantic_color_generated.rs"),
    ),
    (
        "generated_component.rs",
        include_str!("../../tasty-type-appearance/src/generated_component.rs"),
    ),
    (
        "generated_tracking.rs",
        include_str!("../../tasty-type-appearance/src/generated_tracking.rs"),
    ),
];

/// 토큰 수 스냅샷. 원격 토큰을 갱신할 때 추가·삭제된 항목을 확인한 뒤 함께 갱신한다.
#[test]
fn token_census_matches_design_export() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    assert_eq!(
        set.tier_count(dtcg::Tier::Primitive),
        146,
        "primitive census drift"
    );
    assert_eq!(
        set.tier_count(dtcg::Tier::Semantic),
        249,
        "semantic census drift"
    );
    assert_eq!(
        set.tier_count(dtcg::Tier::Component),
        1118,
        "component census drift"
    );
    assert_eq!(set.len(), 1513, "total census drift");
}

/// in-memory 재생성 결과가 커밋된 생성물 텍스트와 완전히 일치해야 한다.
#[test]
fn committed_generated_files_are_fresh() {
    let set = dtcg::parse(DTCG_JSON).expect("vendor json must parse");
    let generated = dtcg::generate(&set);
    assert_eq!(
        generated.files.len(),
        COMMITTED.len(),
        "생성 파일 목록이 바뀜 — 테스트의 COMMITTED 목록도 갱신할 것"
    );
    for ((name, fresh), (committed_name, committed)) in generated.files.iter().zip(COMMITTED) {
        assert_eq!(name, committed_name, "생성 파일 순서/이름 불일치");
        assert_fresh(&format!("src/generated/{name}"), fresh, committed);
    }

    assert_eq!(
        generated.type_appearance_files.len(),
        COMMITTED_TYPE_APPEARANCE.len(),
        "type-appearance 산출 파일 목록이 바뀜 — 테스트의 COMMITTED_TYPE_APPEARANCE 목록도 갱신할 것"
    );
    for ((name, fresh), (committed_name, committed)) in generated
        .type_appearance_files
        .iter()
        .zip(COMMITTED_TYPE_APPEARANCE)
    {
        assert_eq!(name, committed_name, "생성 파일 순서/이름 불일치");
        assert_fresh(
            &format!("../tasty-type-appearance/src/{name}"),
            fresh,
            committed,
        );
    }
}

/// 첫 불일치 행을 보고한다. Windows 체크아웃의 CRLF는 비교 전에 LF로 통일한다.
fn assert_fresh(label: &str, fresh: &str, committed: &str) {
    let fresh = fresh.replace("\r\n", "\n");
    let committed = committed.replace("\r\n", "\n");
    if fresh != committed {
        let first_diff = fresh
            .lines()
            .zip(committed.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1);
        let detail = match first_diff {
            Some(line) => format!("첫 불일치: {line}행"),
            None => "행 내용은 모두 일치 — 행 수/말미 개행 차이".to_string(),
        };
        panic!(
            "{label} 이 vendor json 과 어긋남 ({detail}, \
             재생성 {}줄 vs 커밋 {}줄) — `cargo run -p tasty-design-tokens --bin generate` \
             실행 후 결과를 커밋할 것",
            fresh.lines().count(),
            committed.lines().count(),
        );
    }
}
