//! crossplatform-check의 경로 필터가 입력 전체를 제외하는 소스 검사를 찾는다.
//! 별도의 필터 없는 호출이 해당 타깃·패키지를 포함하면 제외되지 않는 것으로 본다.
//! 입력 일부만 필터에 걸리는 검사와 워크스페이스 의존 때문에 이동이 어려운 검사는 이유를 등록한다.
//! 독립된 소스 검사 크레이트를 사용하는 이유는 docs/dev-guide/guard-verification.md에서 설명한다.
//!
//! 판정은 소스의 읽기·프로세스 실행 표지와 경로 리터럴, use문을 바탕으로 한다.
//! 실제 호출 그래프·동적 경로·링크 의존을 모두 알아내지는 못한다.
//! 워크플로 실행 여부도 공유 파서의 모델에 따른다. 실제 push 묶음과 실행 기록은 검사하지 않는다.
//! 경로 일부만 제외된 검사는 문서만 바꾼 push에서 실행되지 않을 수 있다.
//! 디렉터리 읽기 실패와 성공 뒤의 일부 수집 누락은 별개 문제이며 이 검사가 둘을 보장하지는 않는다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tasty_doc_guards::workflow_triggers::filter_free_coverage;

/// 입력 일부가 경로 필터에 걸리지만 현재 위치를 유지하는 검사와 근거.
const PARTIALLY_FILTERED: &[(&str, &str)] = &[(
    "tests/cli_method_table_parity.rs",
    "코드 입력과 문서를 함께 대조한다. 문서만 바꾼 push에서는 생략될 수 있고 다음 소스 push에서 문서 오류를 검출한다.",
)];

/// 2026-09-05 실측 51개를 기준으로 둔 하한 45다. 이후 검사 추가로 여유가 커졌다.
/// 일부 누락은 통과할 수 있으므로 미달 시 실제 파일 감소와 분류 오류를 구별한다.
const MIN_SCANNED: usize = 45;

/// 문서를 읽으며 워크스페이스 크레이트를 use하는 검사와 이동 제약.
/// 제품 크레이트를 링크하면 의존 없는 doc-guards로 그대로 옮길 수 없다.
/// 소스 판독으로 대체할 경우 실제 런타임 값과 별도 대조가 필요하다.
/// 이동 전 워크플로·훅의 (패키지, 테스트 타깃) 호출도 함께 확인한다.
/// 명부는 실제 분류와 양방향으로 대조한다.
const DEP_BEARING: &[(&str, &str)] = &[(
    "tests/cli_method_table_parity.rs",
    "tasty_ipc 의 METHOD_TABLE·method_meta·DEBUG_METHODS 를 런타임 값으로 읽는다. \
     앞의 둘은 tasty_doc_guards::method_table 판독으로 대체되지만 DEBUG_METHODS \
     판독기는 아직 없다 — 그것이 이 가드의 이동 선행 작업이다.",
)];

const WORKFLOW: &str = ".github/workflows/crossplatform-check.yml";

/// 필터 없는 실행을 확보할 이동 대상. 이 경로 자체를 검사에서 면제하지는 않는다.
const FILTER_FREE_DIR: &str = "crates/tasty-doc-guards/tests";

/// 필터를 읽지 못한 경우와 빈 필터를 같게 취급하지 않도록 실패시킨다.
fn ignore_globs(root: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(root.join(WORKFLOW))
        .unwrap_or_else(|e| panic!("read {WORKFLOW}: {e}"));
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("paths-ignore:") {
            inside = true;
            continue;
        }
        if inside {
            if let Some(rest) = t.strip_prefix("- ") {
                out.push(rest.trim_matches('\'').trim_matches('"').to_string());
            } else if !t.is_empty() {
                break;
            }
        }
    }
    assert!(
        !out.is_empty(),
        "{WORKFLOW}에서 paths-ignore를 읽지 못했다. 필터 삭제와 형식 변경을 구별해 검사 전제를 확인한다."
    );
    out
}

/// 이 검사에서 쓰는 경로 접두어·확장자 glob 형태만 판독한다.
fn is_ignored(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|g| match g.strip_suffix("/**") {
        Some(prefix) => path.starts_with(prefix) && path[prefix.len()..].starts_with('/'),
        None => match g.strip_prefix("**/*") {
            Some(suffix) => path.ends_with(suffix),
            None => path == g,
        },
    })
}

/// 경로 리터럴로 읽을 저장소 최상위 접두어.
const TOP_DIRS: &[&str] = &[
    "docs/", "src/", "crates/", "scripts/", ".github/", "tests/", "assets/", "lang/",
];

fn path_literals(root: &Path, src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, part) in src.split('"').enumerate() {
        if i % 2 == 0 || part.contains('\n') {
            continue;
        }
        if TOP_DIRS.iter().any(|d| part.starts_with(d)) {
            out.insert(part.to_string());
            continue;
        }
        // 디렉터리 접두어가 없는 루트 파일은 실제로 존재할 때만 포함해 가상 예시와 구별한다.
        if !part.contains('/') && part.contains('.') && root.join(part).is_file() {
            out.insert(part.to_string());
        }
    }
    out
}

/// 파일 읽기 표지가 있고 프로세스 실행 표지가 없는 소스를 분류한다. 실제 호출을 분석하지는 않는다.
fn is_pure_source_scan(src: &str) -> bool {
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
    reads && !spawns
}

/// 매니페스트가 있는 디렉터리 이름의 하이픈을 밑줄로 바꿔 크레이트 식별자로 사용한다.
fn workspace_crate_idents(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root.join("crates")) else {
        panic!("read crates/: 워크스페이스 크레이트 목록을 못 읽었다");
    };
    for e in entries.flatten() {
        if e.path().join("Cargo.toml").is_file() {
            out.insert(e.file_name().to_string_lossy().replace('-', "_"));
        }
    }
    out
}

/// use문에서 찾은 워크스페이스 크레이트. doc-guards 자체는 이동을 막는 의존으로 세지 않는다.
fn linked_crates(src: &str, idents: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("use ") else {
            continue;
        };
        let head: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if head != "tasty_doc_guards" && idents.contains(&head) {
            out.insert(head);
        }
    }
    out
}

fn integration_targets(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![root.join("tests")];
    if let Ok(entries) = std::fs::read_dir(root.join("crates")) {
        for e in entries.flatten() {
            dirs.push(e.path().join("tests"));
        }
    }
    for d in dirs {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 상위 매니페스트에서 패키지 이름을 읽는다. 디렉터리명이 패키지명과 같다고 가정하지 않는다.
fn package_of(root: &Path, target: &Path) -> String {
    let mut dir = target.parent();
    while let Some(d) = dir {
        let manifest = d.join("Cargo.toml");
        if manifest.is_file()
            && let Ok(text) = std::fs::read_to_string(&manifest)
        {
            for line in text.lines() {
                if let Some(rest) = line.trim().strip_prefix("name") {
                    let rest = rest.trim_start();
                    if let Some(v) = rest.strip_prefix('=') {
                        return v.trim().trim_matches('"').to_string();
                    }
                }
            }
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    String::new()
}

fn rel(p: &Path, root: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn no_filtered_scan_guard_reads_only_ignored_paths() {
    let root = tasty_doc_guards::repo_root();
    let globs = ignore_globs(&root);
    let covered = filter_free_coverage(&root.join(".github/workflows")).unwrap_or_else(|bad| {
        panic!("`on:` 을 못 읽은 워크플로가 있다 — 판정 불가는 통과가 아니다: {bad:?}")
    });
    // 타깃 목록이나 패키지 목록 중 한쪽만 비는 구성도 가능하다.
    assert!(
        !covered.named.is_empty() || !covered.packages.is_empty() || covered.whole_workspace,
        "필터 없는 실행 경로를 읽지 못했다. 실제 호출과 파서를 확인한 뒤 이동 여부를 판단한다."
    );

    let idents = workspace_crate_idents(&root);
    let mut scanned = 0usize;
    let mut blind: Vec<String> = Vec::new();
    let mut partial: BTreeSet<String> = BTreeSet::new();
    let mut dep_bearing: BTreeSet<String> = BTreeSet::new();

    for file in integration_targets(&root) {
        let r = rel(&file, &root);
        let src = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("read {r}: {e}"));
        if !is_pure_source_scan(&src) {
            continue;
        }
        scanned += 1;
        let paths = path_literals(&root, &src);
        if paths.is_empty() {
            continue;
        }
        if paths
            .iter()
            .any(|p| p.starts_with("docs/") || p.ends_with(".md"))
            && !linked_crates(&src, &idents).is_empty()
        {
            dep_bearing.insert(r.clone());
        }
        let ignored: Vec<&String> = paths.iter().filter(|p| is_ignored(p, &globs)).collect();
        if ignored.is_empty() {
            continue;
        }
        // 디렉터리 위치로 면제하지 않고 필터 없는 타깃·패키지 호출이 있는지 확인한다.
        let stem = file
            .file_stem()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default();
        let pkg = package_of(&root, &file);
        if covered.covers(&stem, &pkg) {
            continue;
        }
        if ignored.len() == paths.len() {
            blind.push(format!(
                "  {r} — 읽는 경로가 전부 무시 대상이다: {}",
                ignored
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else {
            partial.insert(r);
        }
    }

    println!("[필터 뒤 스캔 가드] {scanned} · 하한 {MIN_SCANNED}");
    assert!(
        scanned >= MIN_SCANNED,
        "소스 검사를 {scanned}개만 수집했다. 수집 범위와 분류를 확인한다."
    );

    assert!(
        blind.is_empty(),
        "다음 검사의 경로 리터럴은 모두 {WORKFLOW}의 paths-ignore에 속하고 별도 실행도 찾지 못했다:\n{}\n입력만 바꾼 push에서도 검사하도록 {FILTER_FREE_DIR}로 옮기거나 필터 없는 호출을 구성한다(docs/dev-guide/guard-verification.md). 런타임 상수를 소스 판독으로 대체한다면 실제 값과의 대조 검사도 필요하다.",
        blind.join("\n")
    );

    let declared: BTreeSet<String> = PARTIALLY_FILTERED
        .iter()
        .map(|(p, _)| (*p).to_string())
        .collect();
    let added: Vec<&String> = partial.difference(&declared).collect();
    let stale: Vec<&String> = declared.difference(&partial).collect();
    assert!(
        added.is_empty(),
        "입력의 일부가 무시 대상인 가드가 새로 생겼다. 옮길지 남길지는 판단이 필요하다 — \
         남기기로 했으면 `PARTIALLY_FILTERED` 에 **사유와 함께** 등재해라:\n  {}",
        added
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    let dep_declared: BTreeSet<String> =
        DEP_BEARING.iter().map(|(p, _)| (*p).to_string()).collect();
    let dep_added: Vec<&String> = dep_bearing.difference(&dep_declared).collect();
    let dep_stale: Vec<&String> = dep_declared.difference(&dep_bearing).collect();
    assert!(
        dep_added.is_empty(),
        "문서를 읽고 워크스페이스 크레이트를 사용하는 검사가 추가됐다:\n  {}\n의존 없는 {FILTER_FREE_DIR}로 옮기려면 소스 판독과 런타임 값 대조로 분리하거나, 현재 위치가 필요한 이유를 DEP_BEARING에 등록한다.",
        dep_added
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        dep_stale.is_empty(),
        "DEP_BEARING의 분류와 실제 소스가 다르다. 의존을 제거했다면 항목도 지운다:\n  {}",
        dep_stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    assert!(
        stale.is_empty(),
        "PARTIALLY_FILTERED의 분류와 실제 입력이 다르다. 이동하거나 입력을 바꿨다면 명부를 갱신한다:\n  {}",
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// 수집 누락은 수를 줄이고 잘못된 분류는 수를 늘릴 수도 있어 두 방향을 각각 검증한다.
#[test]
fn the_scanned_floor_sees_both_ways_it_can_die() {
    let root = std::env::temp_dir().join(format!(
        "tasty-scannedfloor-{}-{}",
        std::process::id(),
        line!()
    ));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).expect("임시 디렉토리를 못 만들었다");
    assert_eq!(
        integration_targets(&root).len(),
        0,
        "빈 뿌리에서 0 이 아니면 이 순회는 입력을 안 보는 것이다"
    );

    std::fs::write(root.join("tests/a.rs"), "").expect("쓰기 실패");
    assert_eq!(
        integration_targets(&root).len(),
        1,
        "합성 타깃을 수집하지 못했다"
    );

    assert!(
        is_pure_source_scan("let s = std::fs::read_to_string(p);"),
        "레포를 읽는 소스를 놓치면 이 수가 실제보다 작아진다"
    );
    assert!(
        !is_pure_source_scan("Command::new(\"tasty\"); read_to_string(p);"),
        "프로세스 실행 표지가 있는 소스를 순수 소스 검사로 분류했다"
    );
    assert!(
        !is_pure_source_scan("fn main() {}"),
        "아무것도 안 읽는 소스를 세면 모수의 뜻이 달라진다"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
