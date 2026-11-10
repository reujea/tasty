//! 워크플로의 자동·수동 전용 잡 이름을 각각 명부와 대조한다.
//! 개수가 같아도 분류나 이름이 바뀌면 검출한다. 파일 누락은 해당 잡의 누락으로 드러나므로
//! 순회 하한은 파일을 하나도 읽지 못한 경우만 확인한다.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::workflow_triggers::{automatic_job_names, job_headers};

/// automatic_job_names가 자동으로 분류한 잡 목록(실측 2026-09-08, 12bc0f4b2).
/// 트리거·경로 필터나 if 조건의 실제 참·거짓은 평가하지 않는다.
const AUTOMATIC: &[(&str, &str)] = &[
    ("build-check.yml", "build-linux-arm64"),
    ("build-check.yml", "build-linux-x64"),
    ("build-check.yml", "build-macos"),
    ("build-check.yml", "build-windows"),
    ("build-check.yml", "build-windows-arm64"),
    ("complexity-check.yml", "check-file-size"),
    ("crossplatform-check-macos.yml", "check-macos"),
    ("crossplatform-check.yml", "check-android"),
    ("crossplatform-check.yml", "check-headless"),
    ("crossplatform-check.yml", "check-release"),
    ("crossplatform-check.yml", "check-windows"),
    ("crossplatform-check.yml", "check-windows-arm64"),
    ("doc-guards.yml", "doc-guards"),
    ("format-check.yml", "fmt"),
    ("plugin-version-check.yml", "version-bump"),
    ("release.yml", "create-release"),
    ("release.yml", "publish-release"),
    ("script-gates.yml", "script-gates"),
    ("supply-chain-check.yml", "cargo-deny"),
    ("test.yml", "semver-guards"),
];

/// 같은 측정에서 수동 전용으로 분류된 잡이다.
/// automatic_job_bodies는 workflow_dispatch 조건을 포함하는지만 본다.
/// 따라서 OR 조건으로 태그 push에서도 실행되는 release.yml의 다섯 빌드 잡도 여기에 포함된다.
/// 해당 파일에 cargo test가 없어 테스트 실행 경로 집계에는 영향이 없다.
const MANUAL_ONLY: &[(&str, &str)] = &[
    ("release.yml", "build-linux-arm64"),
    ("release.yml", "build-linux-x64"),
    ("release.yml", "build-macos"),
    ("release.yml", "build-windows"),
    ("release.yml", "build-windows-arm64"),
    ("test.yml", "test-linux-x64"),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("레포 루트")
        .to_path_buf()
}

/// 파일별 잡 목록으로 누락을 찾으므로 순회 하한은 빈 결과만 막는다.
const LIVENESS: Floor = Floor {
    min: 1,
    measured: 11,
    measured_on: "2026-09-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "9d1b15669 — git ls-tree -r --name-only로 .github/workflows/의 yml 파일 11개를 재확인했다.",
    ),
    why_this_gap: "파일 누락은 두 잡 명부의 차집합에서 검출한다. 순회 하한은 파일을 하나도 읽지 못한 경우만 막는다.",
};

/// 잡 하나의 출처 — (워크플로 파일 상대 경로, 잡 이름).
type JobCell = (String, String);

fn measured() -> (BTreeSet<JobCell>, BTreeSet<JobCell>) {
    let dir = repo_root().join(".github/workflows");
    let walked = walk_with_floor(&dir, &dir, &LIVENESS, Descend::Everything, &|w| {
        w.rel.ends_with(".yml")
    })
    .unwrap_or_else(|why| panic!("{why}"));

    let (mut auto, mut manual) = (BTreeSet::new(), BTreeSet::new());
    for w in walked {
        // 읽기 실패와 잡 삭제를 구분하기 위해 오류를 무시하지 않는다.
        let text = std::fs::read_to_string(&w.path)
            .unwrap_or_else(|e| panic!("워크플로 {} 를 못 읽었다: {e}", w.rel));
        let a: BTreeSet<String> = automatic_job_names(&text).into_iter().collect();
        for name in job_headers(&text) {
            let cell = (w.rel.clone(), name.clone());
            if a.contains(&name) {
                auto.insert(cell);
            } else {
                manual.insert(cell);
            }
        }
    }
    (auto, manual)
}

fn pinned(rows: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    rows.iter()
        .map(|(f, j)| ((*f).to_string(), (*j).to_string()))
        .collect()
}

fn show(set: &BTreeSet<(String, String)>) -> String {
    if set.is_empty() {
        return "  (없음)".to_string();
    }
    set.iter()
        .map(|(f, j)| format!("  {f} / {j}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_roster_of_jobs_that_run_on_every_push_is_pinned() {
    let (auto, manual) = measured();
    let want_auto = pinned(AUTOMATIC);
    let want_manual = pinned(MANUAL_ONLY);

    // 이름이 같고 분류만 바뀐 잡을 먼저 찾는다.
    let crossed_out: Vec<_> = want_auto.intersection(&manual).cloned().collect();
    let crossed_in: Vec<_> = want_manual.intersection(&auto).cloned().collect();
    assert!(
        crossed_out.is_empty() && crossed_in.is_empty(),
        "잡의 자동·수동 전용 분류가 달라졌다. 조건식을 직접 확인한 뒤 실제 실행 경로와 명부를 함께 갱신한다.\n[자동 → 수동 전용]\n{}\n[수동 전용 → 자동]\n{}",
        show(&crossed_out.iter().cloned().collect()),
        show(&crossed_in.iter().cloned().collect()),
    );

    let gone: BTreeSet<_> = want_auto.difference(&auto).cloned().collect();
    let added: BTreeSet<_> = auto.difference(&want_auto).cloned().collect();
    assert!(
        gone.is_empty() && added.is_empty(),
        "자동 잡 명부가 달라졌다(등록 {} · 현재 {}).\n[삭제] 대체 실행 경로가 있는지 확인한다. 없다면 docs/dev-guide/ci-gates.md에 검사되지 않는 조합을 적고 명부를 갱신한다:\n{}\n[추가] 명부에 등록하고 실행할 검사와 기존 잡과의 중복 여부를 커밋에 설명한다:\n{}",
        want_auto.len(),
        auto.len(),
        show(&gone),
        show(&added),
    );

    let m_gone: BTreeSet<_> = want_manual.difference(&manual).cloned().collect();
    let m_added: BTreeSet<_> = manual.difference(&want_manual).cloned().collect();
    assert!(
        m_gone.is_empty() && m_added.is_empty(),
        "수동 전용 잡 명부가 달라졌다(등록 {} · 현재 {}). 조건식을 확인하고 명부를 갱신한다.\n[삭제]\n{}\n[추가]\n{}",
        want_manual.len(),
        manual.len(),
        show(&m_gone),
        show(&m_added),
    );
}

/// 명부까지 비우면 빈 집합끼리의 대조가 통과하므로 상수 목록도 확인한다.
#[test]
fn the_pinned_roster_is_not_empty() {
    assert!(
        !AUTOMATIC.is_empty() && !MANUAL_ONLY.is_empty(),
        "잡 명부가 비어 있다(자동 {} · 수동 전용 {}). 빈 명부는 수집 실패를 검출할 수 없다.",
        AUTOMATIC.len(),
        MANUAL_ONLY.len(),
    );
}
