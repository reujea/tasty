//! 0.7.0의 METHOD_TABLE 기준 목록에서 메서드가 사라지지 않았는지 확인한다.
//! 호환성 정책은 docs/dev-guide/api-conventions.md를 따른다.
//! 기준 파일은 메서드 표를 소유하는 tasty-ipc 크레이트에 둔다.

use std::collections::HashSet;

use tasty_ipc::method_meta::METHOD_TABLE;

const BASELINE_0_7: &str = include_str!("../crates/tasty-ipc/fixtures/method_baseline_0_7.txt");

/// 0.7.0 기준 목록은 고정돼 있으므로 하한 대신 정확한 수를 확인한다. 2026-09-05 주석·빈 줄 제외 191개를 측정했고,
/// 플러그인 권한 승인 메서드 3개를 의도해 제거한 뒤 188개다(기준 파일 머리의 제거 사유).
const BASELINE_METHOD_COUNT: usize = 188;

/// 빈 목록이면 아래 비교가 통과할 수 있어 추출 시점에 고정 개수를 확인한다.
fn baseline_methods() -> Vec<&'static str> {
    let methods: Vec<&'static str> = BASELINE_0_7
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    assert_eq!(
        methods.len(),
        BASELINE_METHOD_COUNT,
        "기준 파일의 메서드 수가 {}개로 0.7.0의 {BASELINE_METHOD_COUNT}개와 다르다. 파일·파싱 오류를 확인하고 의도한 제거가 아니면 기준 목록을 고치지 않는다.",
        methods.len()
    );
    methods
}

#[test]
fn all_baseline_methods_still_registered() {
    let current: HashSet<&str> = METHOD_TABLE.iter().map(|(name, _)| *name).collect();
    let missing: Vec<&str> = baseline_methods()
        .into_iter()
        .filter(|name| !current.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "0.7.0 기준 메서드가 METHOD_TABLE에서 사라졌다: {missing:?}. 의도한 제거라면 CHANGELOG에 (BREAK)로 적고 tasty-ipc의 기준 파일과 BASELINE_METHOD_COUNT를 함께 고친다(api-conventions 안정성 정책)."
    );
}

#[test]
fn baseline_file_is_sorted_and_unique() {
    let methods = baseline_methods();
    let mut sorted = methods.clone();
    sorted.sort_unstable();
    assert_eq!(
        methods, sorted,
        "crates/tasty-ipc/fixtures/method_baseline_0_7.txt 는 sort 된 상태여야 한다"
    );
    let dedup: HashSet<&str> = methods.iter().copied().collect();
    assert_eq!(
        methods.len(),
        dedup.len(),
        "crates/tasty-ipc/fixtures/method_baseline_0_7.txt 에 중복 메서드 존재"
    );
}
