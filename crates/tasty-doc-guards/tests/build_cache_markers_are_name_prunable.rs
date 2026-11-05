//! 저장소 안의 빌드 캐시가 이름으로 제외되는 경로 밖에 있는지 확인한다.
//! 일부 순회는 is_build_cache_dir를 호출하지 않으므로 임의 이름의 캐시를 소스로 읽을 수 있다.
//! 그 순회들이 공통으로 제외하는 target·.git 밖의 캐시는 사유와 함께 등록해야 한다.
//! 점 디렉터리도 검사한다. 이름만 보는 다른 순회가 그 안의 캐시를 읽을 수 있기 때문이다.
//! 로컬 파일시스템을 검사하므로 같은 커밋이어도 clone·worktree와 빌드 상태에 따라 결과가 다르다.
//! 심볼릭 링크는 따라가지 않아 저장소 밖을 가리키는 작업 디렉터리도 검사되지 않는다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::path::Path;
use tasty_doc_guards::floored_walk::{
    Descend, Floor, Pick, Walked, walk_dirs_with_floor, walk_with_floor,
};

/// 이름으로만 제외하는 순회들의 공통 제외 이름(2026-09-06 확인).
/// 다른 순회의 제외 규칙이 바뀌면 이 목록도 다시 확인해야 한다.
const COMMON_PRUNED: &[&str] = &["target", ".git"];

/// 공통 제외 경로 밖에 두는 캐시 이름과 그 이유·처리 근거.
/// 로컬 빌드에 따라 존재 여부가 달라지므로 미존재를 오류로 보지는 않는다.
/// 표식 판독을 하지 않는 순회까지 안전하다고 보장하는 목록은 아니다.
const KNOWN_OUTSIDE: &[(&str, &str)] = &[(
    "target-e2e-headless",
    "문서의 E2E 전용 CARGO_TARGET_DIR이다(docs/dev-guide/e2e-tests.md). ensure_dev_bundle이 실행 파일 디렉터리의 조부모를 워크스페이스로 사용하므로 target/ 아래에 한 단계 더 넣으면 번들 plugin을 스테이징하지 못한다. Cargo의 CACHEDIR.TAG가 있어 is_build_cache_dir를 호출하는 순회에서는 이름에 관계없이 제외된다.",
)];

/// 표식 수는 빌드 전이나 외부 target 경로를 사용할 때 0일 수 있어 순회한 디렉터리 수에 하한을 둔다.
/// Pick에는 수집과 하위 순회를 모두 건너뛰는 선택지가 없어, 순회를 줄이는 변이는 표식 목록도 바꾼다.
/// 따라서 이 하한만 실패시키는 독립 대조는 없다. 해당 선택지가 다른 이유로 추가되면 보완한다.
const MARKER_FLOOR: Floor = Floor {
    min: 339,
    measured: 369,
    measured_on: "2026-09-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "b134d28e3 — 이 좌표는 이력 재작성(1998055b1) 이전이라 지금 main 에서 도달 불가다. 당시 측정값은 있으나 그 해시는 현재 이력의 조상이 아니다",
    ),
    why_this_gap: "빌드 캐시를 만날 때 하위 순회를 멈추며, 로컬 디렉터리 상태에 따라 순회 수가 달라진다. 기준값 369는 2026-09-08의 추적 경로에서 디렉터리를 센 값이다(git ls-tree -r --name-only <rev>). 당시 작업 트리에서는 390개였다. 하한 339의 여유 30은 당시 크레이트 하나가 없어질 때 줄어드는 최대 디렉터리 수 10의 세 배로 정했다. 옛 측정 SHA는 현재 이력에서 도달할 수 없다. 빈 디렉터리나 미추적 폴더 때문에 로컬 순회 수를 clean clone의 기준값으로 사용하면 안 된다.",
};

/// 통합 타깃 순회의 하한.
const TARGET_FLOOR: Floor = Floor {
    min: 122,
    // 루트 패키지와 크레이트의 통합 테스트 타깃 수를 합산한다.
    measured: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.measured
        + tasty_doc_guards::floored_walk::populations::CRATE_TEST_TARGETS.measured,
    measured_on: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.measured_on,
    counted_on: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.counted_on,
    why_this_gap: "루트와 크레이트의 통합 테스트 타깃 수를 합산한다. cc2e5e72e에서 합 172(46+126), 하한 122의 여유는 50이다. fdca139c0..91ca7d37d의 558커밋에서 루트 타깃의 최대 감소 16, 크레이트 타깃의 감소 0을 관측해, 같은 감소가 세 번 겹치는 48을 허용하도록 정했다. 실제 수집 범위는 루트·크레이트 수집 검사와 경로 깊이의 합성 검사로 확인한다.",
};

/// `CACHEDIR.TAG` 규격의 서명 줄. 여기서는 **판정이 아니라 입력**으로만 쓴다 —
/// 판정은 언제나 [`tasty_doc_guards::is_build_cache_dir`] 가 한다.
const CACHEDIR_LINE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55\n";

/// 캐시 표식 판독을 호출하지 않는 통합 테스트 수를 진단에 표시한다.
/// 원문에서 read_dir와 is_build_cache_dir의 포함 여부만 확인하며, 위반 판정에는 쓰지 않는다.
fn scanners_without_property_check(root: &Path) -> Option<usize> {
    let Ok(targets) = walk_with_floor(
        root,
        root,
        &TARGET_FLOOR,
        Descend::SkipBuildCaches,
        &is_integration_target,
    ) else {
        // 수집 실패를 0개로 표시하면 표식 판독이 불필요한 것으로 오해할 수 있어 None으로 구분한다.
        return None;
    };
    Some(
        targets
            .iter()
            .filter(|t| {
                std::fs::read_to_string(&t.path).is_ok_and(|src| {
                    src.contains("read_dir") && !src.contains("is_build_cache_dir")
                })
            })
            .count(),
    )
}

/// 정규화된 경로로 루트·크레이트 tests/ 바로 아래의 .rs 파일을 구분한다.
fn is_integration_target(found: &Walked) -> bool {
    if !found.rel.ends_with(".rs") {
        return false;
    }
    let parts: Vec<&str> = found.rel.split('/').collect();
    matches!(parts.as_slice(), ["tests", _] | ["crates", _, "tests", _])
}

/// 경로 성분 중 하나라도 [`KNOWN_OUTSIDE`] 에 등재된 이름인가.
fn is_known_outside(rel: &str) -> bool {
    rel.split('/')
        .any(|c| KNOWN_OUTSIDE.iter().any(|(name, _)| *name == c))
}

/// 경로 성분 중 하나라도 공통 제외 이름이면, 이름만 보는 가드도 여기 도달하지 못한다.
fn is_covered_by_name_pruning(rel: &str) -> bool {
    rel.split('/').any(|c| COMMON_PRUNED.contains(&c))
}

/// 이름에 관계없이 캐시 표식을 찾는다. 심볼릭 링크는 따라가지 않는다.
/// 표식을 찾으면 캐시 내부를 소스로 읽지 않도록 하위 순회를 중단한다.
fn find_markers(root: &Path, rel_base: &Path, floor: &Floor) -> Result<Vec<String>, String> {
    walk_dirs_with_floor(root, rel_base, floor, &|found| {
        if tasty_doc_guards::is_build_cache_dir(&found.path) {
            Pick::TakeAndStop
        } else {
            Pick::Skip
        }
    })
    .map(|dirs| dirs.into_iter().map(|d| d.rel).collect())
}

#[test]
fn every_build_cache_marker_sits_under_a_name_that_every_scanner_prunes() {
    let root = tasty_doc_guards::repo_root();
    let markers = find_markers(&root, &root, &MARKER_FLOOR).unwrap_or_else(|why| panic!("{why}"));

    let outside: Vec<&String> = markers
        .iter()
        .filter(|rel| !is_covered_by_name_pruning(rel) && !is_known_outside(rel))
        .collect();

    let blind = match scanners_without_property_check(&root) {
        Some(n) => format!("{n} 개"),
        None => "셀 수 없었다 — 그 순회가 하한에 걸렸다".to_string(),
    };
    assert!(
        outside.is_empty(),
        "빌드 캐시 표식이 공통 제외 경로({COMMON_PRUNED:?}) 밖에 있고 KNOWN_OUTSIDE에도 없다:\n{}\n이 검사는 현재 파일시스템을 읽으므로 캐시가 없는 CI에서는 재현되지 않을 수 있다. 표식 판독을 호출하지 않는 통합 타깃은 {blind}다. 캐시를 소스로 읽으면 순회 지연이나 오탐이 생길 수 있다.\n1. target/ 아래로 옮길 수 있는지 확인한다.\n2. 해당 경로를 읽는 순회가 is_build_cache_dir를 호출하는지 확인한다.\n3. 옮길 수 없다면 CACHEDIR.TAG와 제외 근거를 확인하고 KNOWN_OUTSIDE에 이유를 적는다.\n각 PRUNE_DIRS에 이름을 복제하기보다 공용 표식 판독을 사용한다.",
        outside
            .iter()
            .map(|r| format!("  {r}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_scan_separates_a_marker_outside_the_pruned_names_from_one_inside() {
    let base = std::env::temp_dir().join(format!(
        "tasty-cachetag-probe-{}-{}",
        std::process::id(),
        line!()
    ));
    // 이유: 이전 실행의 임시 경로가 없어도 된다. 생성 성공은 아래에서 따로 확인한다.
    let _ = std::fs::remove_dir_all(&base);

    let outside = base.join("tools").join("cache");
    let inside = base.join("target").join("nested");
    let excused = base.join(KNOWN_OUTSIDE[0].0).join("debug");
    for dir in [&outside, &inside, &excused] {
        std::fs::create_dir_all(dir).expect("프로브 디렉토리를 만들지 못했다");
        std::fs::write(dir.join("CACHEDIR.TAG"), CACHEDIR_LINE).expect("표식을 쓰지 못했다");
    }

    for dir in [&outside, &inside, &excused] {
        assert!(
            tasty_doc_guards::is_build_cache_dir(dir),
            "프로브가 적용되지 않았다: {}",
            dir.display()
        );
    }

    // 실제 검사와 같은 순회를 호출하되 작은 합성 트리에 맞는 하한을 쓴다.
    let probe_floor = Floor {
        min: 3,
        measured: 7,
        measured_on: "2026-09-06",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "심은 트리라 수가 고정이다. 하한을 실측보다 낮춰 두는 것은 이 대조가                        트리 모양의 사소한 변경에 깨지지 않게 하려는 것뿐이다.",
    };
    let found = find_markers(&base, &base, &probe_floor)
        .unwrap_or_else(|why| panic!("대조 트리 순회가 하한에 걸렸다: {why}"));

    let mut expected = vec![
        format!("{}/debug", KNOWN_OUTSIDE[0].0),
        "target/nested".to_string(),
        "tools/cache".to_string(),
    ];
    expected.sort();
    assert_eq!(
        found, expected,
        "합성 트리에 만든 캐시 표식 세 개를 모두 수집해야 한다"
    );

    let reported: Vec<&String> = found
        .iter()
        .filter(|rel| !is_covered_by_name_pruning(rel) && !is_known_outside(rel))
        .collect();
    assert_eq!(
        reported,
        vec![&"tools/cache".to_string()],
        "공통 제외 경로와 KNOWN_OUTSIDE 항목을 제외하고 tools/cache만 위반이어야 한다"
    );

    // 이유: 정리는 판정 뒤라 여기서 실패해도 이 테스트의 결론이 안 바뀐다.
    std::fs::remove_dir_all(&base).ok();
}

/// 경로를 제외하는 이유와 표식 처리 근거를 적었는지 최소 길이로 확인한다. 내용의 타당성은 사람이 검토한다.
#[test]
fn every_excused_directory_carries_a_reason_not_just_a_name() {
    for (name, why) in KNOWN_OUTSIDE {
        let words = why.split_whitespace().count();
        assert!(
            words >= 20,
            "KNOWN_OUTSIDE의 {name} 사유가 {words}단어뿐이다. 공통 제외 경로로 옮길 수 없는 이유와 캐시 표식 처리 근거를 설명한다. 근거가 없다면 기준을 낮추지 말고 경로를 옮긴다."
        );
    }
}

/// 실제 순회가 루트와 크레이트 tests/를 모두 수집하는지 확인한다.
/// 깊이 제한은 수집된 결과로 재검사하면 항상 참이므로 별도 합성 입력에서 판정한다.
#[test]
fn the_target_walk_sees_both_roots() {
    let root = tasty_doc_guards::repo_root();
    let targets = walk_with_floor(
        &root,
        &root,
        &TARGET_FLOOR,
        Descend::SkipBuildCaches,
        &is_integration_target,
    )
    .unwrap_or_else(|why| panic!("{why}"));

    assert!(
        targets.iter().any(|t| t.rel.starts_with("tests/")),
        "루트 패키지의 tests/에서 통합 타깃을 수집하지 못했다"
    );
    assert!(
        targets.iter().any(|t| t.rel.starts_with("crates/")),
        "크레이트의 tests/에서 통합 타깃을 수집하지 못했다"
    );
}

/// tests/ 바로 아래 .rs만 통합 타깃으로 고르는지 판정 함수를 합성 경로로 직접 확인한다.
#[test]
fn the_target_predicate_takes_only_the_flat_layer() {
    let probe = |rel: &str| Walked {
        path: std::path::PathBuf::from(rel),
        rel: rel.to_string(),
    };
    assert!(is_integration_target(&probe("tests/layering.rs")));
    assert!(is_integration_target(&probe(
        "crates/tasty-doc-guards/tests/x.rs"
    )));
    assert!(
        !is_integration_target(&probe("tests/common/mod.rs")),
        "`tests/` 하위 디렉토리는 공유 헬퍼지 타깃이 아니다"
    );
    assert!(
        !is_integration_target(&probe("crates/a/tests/sub/x.rs")),
        "크레이트 쪽도 마찬가지다"
    );
    assert!(!is_integration_target(&probe("src/main.rs")));
    assert!(
        !is_integration_target(&probe("tests/fixtures/a.md")),
        "`.rs` 가 아닌 것을 타깃으로 센다"
    );
}
