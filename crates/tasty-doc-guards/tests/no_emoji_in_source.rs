//! Rust 소스와 플러그인 매니페스트의 이모지를 검사한다. 아이콘은 디자인 SVG를 사용한다.
//! 자동 실행 경로는 docs/dev-guide/ci-gates.md에 있다.
//!
//! 검사 범위는 U+1F000..=1FAFF와 국기 문자 U+1F1E6..=1F1FF다.
//! 화살표·명령 키 등 이 밖의 텍스트 기호는 검사하지 않는다.
//! 이모지가 검증 입력인 파일은 예외로 둔다. 설명에는 글리프 대신 코드포인트를 적어
//! 이 파일 자체가 위반으로 검출되지 않게 한다. crates의 tests도 검사하지만 루트 tests는 제외한다.

use std::path::{Path, PathBuf};

/// 스캔에서 제외할 파일(repo-relative) — 이모지가 테스트의 본질이라 제거하면 검증이 무의미해지는 곳.
/// - `tasty-memory/src/scope.rs`: "이모지 키 거부" 단위테스트(`U+1F600` 이 입력).
/// - `tasty-terminal/src/disk_scrollback.rs`: 터미널 셀 이모지 렌더 테스트(`U+1F980`).
/// - `tasty-terminal/tests/scrollback_capture.rs`: 이모지 폭/ZWJ/flag/skin-tone 캡처 테스트.
const ALLOWLIST_FILES: &[&str] = &[
    "crates/tasty-memory/src/scope.rs",
    "crates/tasty-terminal/src/disk_scrollback.rs",
    "crates/tasty-terminal/tests/scrollback_capture.rs",
];

const PRUNE_DIRS: &[&str] = &["target", "dist", ".worktree", ".git", "node_modules"];

/// 로컬 경로를 문서 인용으로 오인하지 않도록 순회용 이름을 조립한다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

fn is_forbidden_emoji(cp: u32) -> bool {
    (0x1F000..=0x1FAFF).contains(&cp) || (0x1F1E6..=0x1F1FF).contains(&cp)
}

/// 스캔 대상 파일인지 — repo-relative 경로 기준.
/// - 파일명이 `tasty-plugin.toml` 이면 대상(어디에 있든).
/// - `.rs` 이면서 `src/` · `crates/*/src/` · `crates/*/tests/` 아래면 대상.
fn is_scan_target(rel: &str) -> bool {
    if rel.rsplit('/').next() == Some("tasty-plugin.toml") {
        return true;
    }
    if !rel.ends_with(".rs") {
        return false;
    }
    if rel.starts_with("src/") {
        return true;
    }
    if let Some(rest) = rel.strip_prefix("crates/") {
        let mut parts = rest.splitn(2, '/');
        let _name = parts.next();
        if let Some(after) = parts.next() {
            return after.starts_with("src/") || after.starts_with("tests/");
        }
    }
    false
}

fn gather(path: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        let rel = rel_of(path, root);
        if is_scan_target(&rel) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // 이름이 다른 CARGO_TARGET_DIR도 캐시 표식으로 제외한다.
            // 공식 플러그인 픽스처는 공식 플러그인 레포의 원문이라 그 레포의 검사 몫이다.
            if is_pruned(name)
                || tasty_doc_guards::is_build_cache_dir(&p)
                || tasty_doc_guards::is_official_plugin_fixtures_dir(&p)
            {
                continue;
            }
        }
        gather(&p, root, out);
    }
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 빈 순회가 통과하지 않게 하는 하한이다(docs/dev-guide/guard-verification.md).
/// 2026-09-05에 gather와 is_scan_target으로 1120파일을 수집했다.
/// 파일 정리를 허용할 여유를 두므로 하한 통과가 수집의 완전성을 보장하지는 않는다.
const MIN_SCANNED_FILES: usize = 700;

fn scan_is_credible(found: usize) -> bool {
    found >= MIN_SCANNED_FILES
}

#[test]
fn the_scan_refuses_to_report_zero_from_an_empty_walk() {
    assert!(!scan_is_credible(0), "빈 스캔을 믿을 만하다고 판정했다");
    assert!(!scan_is_credible(MIN_SCANNED_FILES - 1));
    assert!(scan_is_credible(MIN_SCANNED_FILES));
}

#[test]
fn no_emoji_in_source() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, root, &mut files);
    assert!(
        scan_is_credible(files.len()),
        "검사 대상을 {}개만 수집했다(하한 {MIN_SCANNED_FILES}). is_scan_target과 제외 디렉터리를 확인한다. Git의 추적 Rust 파일 목록과 비교하되 매니페스트 포함·제외 범위가 다른 점을 고려한다. 실제 대상이 줄었다면 값과 측정 근거를 함께 갱신한다.",
        files.len()
    );

    let mut violations = Vec::new();
    for file in files {
        let rel = rel_of(&file, root);
        if ALLOWLIST_FILES.contains(&rel.as_str()) {
            continue;
        }
        let contents = std::fs::read_to_string(&file).expect("소스 파일 read 실패");
        for (i, line) in contents.lines().enumerate() {
            for ch in line.chars() {
                let cp = ch as u32;
                if is_forbidden_emoji(cp) {
                    violations.push(format!("  {}:{} — U+{:04X} `{}`", rel, i + 1, cp, ch));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "소스(.rs / tasty-plugin.toml)에 픽토그래픽 이모지(U+1F000–1FAFF / 국기 U+1F1E6–1F1FF)가 \
         재유입됨 — 아이콘은 디자인 SVG 라인아이콘(`icons::*`)에서 쓰고, 매니페스트 icon 이모지· \
         주석 이모지는 제거할 것. 테스트 입력이 본질인 파일은 ALLOWLIST_FILES 에 추가:\n{}",
        violations.join("\n")
    );
}

/// 예외 경로의 존재만 확인한다. 해당 예외가 아직 필요한지는 별도 검토해야 한다.
#[test]
fn allowlist_files_point_at_paths_that_exist() {
    let root = &tasty_doc_guards::repo_root();
    let missing = tasty_doc_guards::missing_referents(root, ALLOWLIST_FILES.iter().copied());
    assert!(
        missing.is_empty(),
        "면제가 없는 경로를 가리킨다 — 옮겼으면 항목도 옮기고, 사라졌으면 항목을 지워라: {missing:?}"
    );
}

#[test]
fn official_plugin_fixtures_are_not_scanned() {
    let dir = tasty_doc_guards::temp_scratch::Scratch::new("no-emoji-fixtures");
    let root = dir.path();
    let kept = tasty_doc_guards::seed_official_plugin_fixture_probe(
        root,
        "tasty-plugin.toml",
        "crates/tasty-plugin-probe",
        "id = \"com.tasty.probe\"\n",
    );
    let mut out = Vec::new();
    gather(root, root, &mut out);
    let rels: Vec<String> = out.iter().map(|p| rel_of(p, root)).collect();
    assert_eq!(
        rels,
        vec![kept],
        "픽스처나 갱신 후보를 수집했거나 일반 매니페스트를 놓쳤다"
    );
}
