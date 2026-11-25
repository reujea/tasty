//! 이 디렉터리의 소스 검사에 경로 필터 없는 push 실행 경로가 있는지 확인한다(docs/dev-guide/guard-verification.md).
//! 워크플로 이름 대신 패키지 전체 호출이나 개별 테스트 선택을 읽는다.
//! 다른 검사가 이동 대상으로 안내하는 디렉터리와도 일치해야 한다.
//! 워크플로 조건식의 실제 실행 가능성은 계산하지 않고 workflow_triggers의 분류를 사용한다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tasty_doc_guards::workflow_triggers::filter_free_coverage;

/// 필터 없는 실행을 확인할 디렉터리. 다른 검사의 FILTER_FREE_DIR와 일치해야 한다.
const GUARD_DIR: &str = "crates/tasty-doc-guards/tests";

const PACKAGE: &str = "tasty-doc-guards";

/// 2026-09-05 실측 17개를 기준으로 둔 하한 12다. 2026-09-07에는 53개로 늘어 여유가 커졌다.
/// 여유 안의 일부 누락은 검출하지 못한다. 실제 삭제·이동과 수집 실패를 구별해 기준을 갱신한다.
const MIN_GUARDED: usize = 12;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

/// 파일 읽기 표지는 있고 프로세스 실행 표지는 없는 소스를 분류한다. 실제 동작을 추적하지는 않는다.
fn guarded_targets(root: &Path) -> BTreeSet<String> {
    let dir = root.join(GUARD_DIR);
    let mut out = BTreeSet::new();
    for e in std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
    {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&p) else {
            continue;
        };
        let reads = src.contains("CARGO_MANIFEST_DIR")
            || src.contains("repo_root()")
            || src.contains("read_to_string")
            || src.contains("read_dir");
        let spawns = [
            "Command::new",
            "spawn_diag",
            "TASTY_E2E_BIN",
            "CARGO_BIN_EXE",
        ]
        .iter()
        .any(|m| src.contains(m));
        if reads && !spawns {
            out.insert(p.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    out
}

#[test]
fn a_filter_free_job_runs_this_package_whole() {
    let root = repo_root();
    let coverage = filter_free_coverage(&root.join(".github/workflows")).unwrap_or_else(|bad| {
        panic!(
            "판독하지 못한 워크플로가 있다 — 판정 불가는 통과가 아니다. 인라인 표기(`on: [push]`)나 \
                 지원하지 않는 잡·스텝 표기면 지원하는 표기로 바꾼다: {bad:?}"
        )
    });

    // 개별 타깃 선택 없이 패키지 전체만 실행하는 구성도 허용하므로 합친 결과가 비었는지 확인한다.
    assert!(
        !coverage.named.is_empty() || !coverage.packages.is_empty() || coverage.whole_workspace,
        "필터 없는 채널을 하나도 못 읽었다 — 판독이 깨졌거나 채널이 전부 사라졌다"
    );

    let guarded = guarded_targets(&root);
    println!(
        "[필터 없는 채널] 순수 스캔 가드 {} · 하한 {MIN_GUARDED}",
        guarded.len()
    );
    assert!(
        guarded.len() >= MIN_GUARDED,
        "{GUARD_DIR}의 소스 검사를 {}개만 수집했다(하한 {MIN_GUARDED}). 실제 파일과 분류 조건을 확인한다.",
        guarded.len()
    );

    let uncovered: Vec<&String> = guarded
        .iter()
        .filter(|stem| !coverage.covers(stem, PACKAGE))
        .collect();
    assert!(
        uncovered.is_empty(),
        "{GUARD_DIR}의 다음 검사를 포함하는 필터 없는 push 호출을 찾지 못했다. {PACKAGE} 전체나 해당 타깃을 경로 필터 없는 워크플로에서 실행하도록 구성한다.\n읽은 타깃 {:?}·패키지 {:?}:\n  {}",
        coverage.named,
        coverage.packages,
        uncovered
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// 이동 안내의 대상과 필터 없는 실행을 검증하는 대상이 다르면 안내를 따라도 검사되지 않을 수 있다.
#[test]
fn the_move_target_and_the_guarded_channel_are_the_same_directory() {
    let root = repo_root();
    let path = root
        .join(GUARD_DIR)
        .join("filtered_guards_are_not_totally_blind.rs");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "이동 안내가 있는 검사 파일을 읽지 못했다: {} — {e}. 두 검사의 역할과 대상을 함께 확인한다.",
            path.display()
        )
    });
    let decl = format!("const FILTER_FREE_DIR: &str = \"{GUARD_DIR}\";");
    assert!(
        src.contains(&decl),
        "filtered_guards_are_not_totally_blind의 FILTER_FREE_DIR가 {GUARD_DIR}와 다르다. 이동 안내와 실행 검사의 대상을 맞춘다."
    );
}

/// 수집 누락과 잘못된 분류를 구별하도록 읽기·프로세스 실행 표지 조합을 따로 확인한다.
#[test]
fn the_guarded_floor_sees_a_collapsed_collection() {
    let root = std::env::temp_dir().join(format!(
        "tasty-guardedfloor-{}-{}",
        std::process::id(),
        line!()
    ));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(GUARD_DIR);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리를 못 만들었다");

    assert_eq!(
        guarded_targets(&root).len(),
        0,
        "빈 디렉토리에서 0 이 아니면 이 수집기는 입력을 안 보는 것이다"
    );

    std::fs::write(dir.join("inert.rs"), "fn main() {}\n").expect("쓰기 실패");
    assert_eq!(
        guarded_targets(&root).len(),
        0,
        "읽기 표지가 없는 소스를 검사 대상으로 분류했다"
    );

    std::fs::write(
        dir.join("spawner.rs"),
        "fn main() { let _ = read_to_string(\"x\"); Command::new(\"y\"); }\n",
    )
    .expect("쓰기 실패");
    assert_eq!(
        guarded_targets(&root).len(),
        0,
        "프로세스 실행 표지가 있는 소스를 순수 소스 검사로 분류했다"
    );

    std::fs::write(
        dir.join("pure.rs"),
        "fn main() { let _ = read_to_string(\"x\"); }\n",
    )
    .expect("쓰기 실패");
    assert_eq!(
        guarded_targets(&root).len(),
        1,
        "파일 읽기 표지만 있는 소스를 검사 대상으로 분류하지 못했다"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
