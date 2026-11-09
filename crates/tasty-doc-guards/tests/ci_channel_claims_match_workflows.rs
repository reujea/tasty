//! 문서·소스 주석의 CI 실행 주장을 워크플로의 테스트 명령과 대조한다.
//! 기본·헤드리스 feature 조합을 구분하고 타깃·패키지 제한, required-features, --skip을 반영한다.
//! CI 구성은 docs/dev-guide/ci-gates.md에서 설명한다.
//!
//! 이 검사는 정해진 문자열 표지와 문단 범위를 사용한다.
//! - 표지가 줄바꿈으로 나뉘거나 대상 이름·명령이 없으면 찾지 못할 수 있다.
//! - 한 문장에 서로 다른 검사의 주장이 섞이면 각 주장과 대상을 구별하지 못한다.
//! - 워크플로·패키지 이름만 든 부재 주장과 다른 문단의 제목에서만 워크플로를 든 목록은 판정하지 않는다.
//! - 스크립트 형제 목록 검사는 전체 목록이라고 명시한 문단만 본다. 단독 게이트 설명은 제외한다.
//! - clippy deny·타입 검사·pre-commit 등 워크플로 밖의 강제 수단과 커밋·PR·티켓은 검사하지 않는다.
//! - 조합 한정 표현이 등록된 표지에 없으면 정확한 문장도 오류로 보고할 수 있다.
//!
//! 자동 잡 분류는 workflow_triggers의 문자열 판독을 사용하며 조건식을 계산하지 않는다.
//! libtest의 --skip 부분일치와 Cargo feature 해석은 모델의 전제로 사용한다.
//! Cargo 자체를 재귀 호출해 이 전제를 검증하지는 않는다.
//! 일부 타깃 검사는 제한 없는 자동 호출이 있으면 조기 반환한다. 통과 수만으로 검사 범위를 판단하면 안 된다.
//!
//! 이 파일의 주석도 검사 입력이다. 예시에서 실제 이름과 주장 표지를 함께 쓰면 실제 주장으로 판정될 수 있다.
//! 합성 입력은 각 파서의 통과·실패를 확인하며, 저장소 전체 판정의 타당성을 대신하지 않는다.
//! 경로 비교에는 normalized_rel을 사용해 Windows 구분자를 정규화한다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, normalized_rel, walk_with_floor};
use tasty_doc_guards::temp_scratch::Scratch;
use tasty_doc_guards::workflow_triggers::automatic_job_bodies;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

/// 수집에서 제외할 디렉터리. 항목을 추가하면 검사 범위가 줄어든다.
const SKIP_DIRS: &[&str] = &["target", ".git", "_site", "node_modules"];

/// 텍스트로 읽을 확장자.
const TEXT_EXTS: &[&str] = &["rs", "md", "toml", "yml", "yaml", "sh"];

/// 같은 설명에 자동 실행 부재를 밝힌 표지. 파일 경로별 예외 대신 문맥으로 판단한다.
const ABSENCE_MARKERS: &[&str] = &[
    "수동 전용",
    "수동 실행",
    "수동 트리거",
    "자동 채널 없음",
    "자동 채널이 없다",
    "자동 채널은 아니다",
    "자동으로 돌지 않는다",
    "자동으로 도는 채널은 없다",
    "그 채널도 수동",
    "workflow_dispatch",
    // 컴파일과 실행을 구별한 부재 표현도 허용한다.
    "실행 채널이 없",
    "실행 채널 없음",
    // 특정 경로에서만 실행된다는 말도 그 밖의 경로를 부정하는 주장이다.
    "에서만 일어난다",
    "실행은 수동",
];

/// 명령 인용과 같은 설명 범위에서 CI 실행 주장으로 읽는 표지.
const CI_MARKERS: &[&str] = &[
    "(CI)",
    "CI 강제",
    "CI 채널",
    "CI channel",
    "CI 의",
    "CI 에서",
    "Linux CI",
    "test.yml",
    ".github/workflows",
];

/// 명령 없이도 실행을 주장하는 표지. 워크플로 파일의 단순 참조는 포함하지 않는다.
const ENFORCE_MARKERS: &[&str] = &[
    "CI 강제",
    "CI 에서 강제",
    "CI 가 강제",
    "CI 로 강제",
    "CI 가 잡",
    "CI 에서 잡",
    "CI 가 막",
    "CI 에서 막",
    "CI 가 차단",
    "CI 에서 차단",
    "CI fail",
    "CI 가 fail",
    "CI 에서 fail",
    "(CI)",
];

/// 테스트 실행과 컴파일·정적 검사를 구별한다.
const COMPILE_CLAIM_MARKERS: &[&str] = &["컴파일", "clippy", "빌드", "compile"];

/// 자동 실행을 긍정하는 문맥의 표지. 부재 주장과 다른 실행 경로를 함께 설명할 때 사용한다.
const AUTOMATIC_CHANNEL_MARKERS: &[&str] = &[
    "--lib --bins",
    "자동으로 돈다",
    "자동으로 돌린다",
    "crossplatform-check",
];

/// 자동 실행을 직접 긍정하는 표현. 단순 워크플로·명령 참조와 구별한다.
const AFFIRMATIVE_RUN_MARKERS: &[&str] = &["자동으로 돈다", "자동으로 돌린다", "✅ 자동"];

/// 같은 설명이 자동 실행도 주장하면 부재 표지만으로 면제하지 않는다.
/// 반대 방향 검사의 시작점인 weak_absence_offsets에서는 긍정 표현이 있어도 부재 주장을 수집한다.
fn absence_exempts(scope: &str) -> bool {
    ABSENCE_MARKERS.iter().any(|m| scope.contains(m))
        && !AFFIRMATIVE_RUN_MARKERS.iter().any(|m| scope.contains(m))
}

/// 추출 실패로 lib 테스트 이름이 거의 남지 않는 경우를 찾는 하한.
/// 2026-09-06 실측 5130에 비해 하한 100은 낮아, 점진적인 누락을 검출하지는 못한다.
const MIN_LIB_TESTS: usize = 100;

/// 문서 수집이 크게 줄었는지 확인하는 하한.
const MIN_SCANNED_FILES: usize = 400;
/// 통합 타깃 인용 수의 하한. 제한 없는 자동 호출이 있으면 검사 전에 반환해 적용되지 않는다.
/// the_enforcement_arm_is_dormant_only_while_an_unnarrowed_automatic_job_exists에서 반환 조건을 확인한다.
const MIN_TEST_CITATIONS: usize = 40;

/// 명령이 --lib·--bins·--test로 검사 대상을 제한하는지 확인한다.
fn is_narrowed(tail: &str) -> bool {
    logical_command(tail)
        .split_whitespace()
        .any(|w| w.starts_with("--lib") || w.starts_with("--bins") || w.starts_with("--test"))
}

/// 명령 시작부터 줄 끝 백슬래시로 이어진 행만 합친다. 이어지지 않은 다음 스텝은 포함하지 않는다.
fn logical_command(tail: &str) -> String {
    let mut out = String::new();
    for line in tail.lines() {
        let trimmed = line.trim_end();
        let continues = trimmed.ends_with('\\');
        out.push_str(trimmed.trim_end_matches('\\'));
        out.push(' ');
        if !continues {
            break;
        }
    }
    out
}

/// 텍스트에서 "전체 스위트를 CI 가 돌린다" 는 주장의 바이트 오프셋들.
fn claim_offsets(text: &str, path: &str) -> Vec<usize> {
    const NEEDLE: &str = "cargo test --workspace";
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(NEEDLE) {
        let at = from + rel;
        from = at + NEEDLE.len();
        if is_narrowed(&text[from..]) {
            continue;
        }
        // 주장 판정은 산문만 읽고 Rust 리터럴·식별자는 제외한다.
        if !is_prose_line(text, at, path) {
            continue;
        }
        // 다른 문단의 표지가 섞이지 않도록 같은 설명 범위에서 찾는다.
        let scope = claim_scope(text, at);
        if !CI_MARKERS.iter().any(|m| scope.contains(m)) {
            continue;
        }
        // 자동 실행을 함께 주장하는 모순된 설명은 부재 표지로 면제하지 않는다.
        if absence_exempts(scope) {
            continue;
        }
        found.push(at);
    }
    found
}

/// 바이트 오프셋 → 1-기준 줄 번호.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].lines().count().max(1)
}

/// 워크플로 수집 실패를 실행 경로의 부재로 오해하지 않도록 하한을 둔다.
const WORKFLOW_FLOOR: Floor = Floor {
    min: 7,
    measured: 11,
    measured_on: "2026-09-07",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::NEVER_COUNTED,
    why_this_gap: "기준값 11과 하한 7의 차이 4는 소수 워크플로의 통폐합을 허용하되 절반 이하만 수집하는 오류를 찾기 위한 여유다.",
};

/// 호출자가 만든 합성 디렉터리에도 쓰는 하한. 저장소 규모와 무관하게 빈 수집만 막는다.
const CALLER_SUPPLIED_FLOOR: Floor = Floor {
    min: 1,
    measured: 1,
    measured_on: "2026-09-07",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::NEVER_COUNTED,
    why_this_gap: "호출자가 만든 디렉터리의 크기는 일정하지 않으므로 빈 수집만 막는 하한 1을 쓴다. 저장소 전체 수집에는 별도 하한을 적용한다.",
};

/// GitHub Actions가 읽는 .yml과 .yaml을 모두 수집한다.
fn is_workflow_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yml") | Some("yaml")
    )
}

/// 워크플로 디렉터리의 파일을 공유 순회로 수집한다.
fn workflow_files(workflows: &Path, floor: &Floor) -> Vec<Walked> {
    // 하한은 전체 수집 파일에 적용한다. 파일은 있지만 워크플로가 없는 합성 입력도 허용해야 한다.
    walk_with_floor(workflows, workflows, floor, Descend::Everything, &|_| true)
        .unwrap_or_else(|why| panic!("{why}"))
        .into_iter()
        .filter(|w| is_workflow_file(&w.path))
        .collect()
}

fn automatic_job_bodies_of_dir(workflows: &Path, floor: &Floor) -> Vec<String> {
    let mut bodies = Vec::new();
    for w in workflow_files(workflows, floor) {
        let text = std::fs::read_to_string(&w.path).unwrap_or_default();
        // jobs: 이전의 push:·pull_request:·schedule: 문자열로 자동 트리거를 판독한다.
        let head: String = text
            .lines()
            .take_while(|l| !l.starts_with("jobs:"))
            .collect();
        if !(head.contains("push:") || head.contains("pull_request:") || head.contains("schedule:"))
        {
            continue;
        }
        // 잡 분할과 수동 전용 분류는 workflow_triggers의 공유 함수를 사용한다.
        bodies.extend(automatic_job_bodies(&text));
    }
    bodies
}

/// 한 잡에 제한 없는 workspace 테스트 호출이 있는지 확인한다.
fn a_job_body_runs_the_full_suite(body: &str) -> bool {
    cargo_test_tails(body)
        .iter()
        .any(|tail| a_full_suite_invocation(tail))
}

/// cargo_test_tails가 추출한 테스트 인자에서 workspace 전체 호출인지 확인한다.
fn a_full_suite_invocation(tail: &str) -> bool {
    tail.contains("--workspace") && !is_narrowed(tail)
}

/// 기본 feature 조합의 전체 호출만 인정한다. 헤드리스 호출로 기본 조합의 주장을 정당화하지 않는다.
fn ci_actually_runs_the_full_suite(root: &Path) -> bool {
    automatic_test_invocations(root)
        .iter()
        .any(|(combo, tail)| *combo == Combo::Default && a_full_suite_invocation(tail))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        panic!("디렉토리를 읽지 못했다: {}", dir.display());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') && name != ".github" {
                continue;
            }
            collect_files(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| TEXT_EXTS.contains(&e))
        {
            out.push(path);
        }
    }
}

/// run: 명령만 읽는다. 스텝 이름에 적힌 명령은 포함하지 않는다.
/// 접힘 스칼라(>)와 일반 스칼라는 행을 합치고, 리터럴 스칼라(|)는 백슬래시 연속행만 합친다.
fn run_commands(body: &str) -> Vec<String> {
    let indent_of = |l: &str| l.len() - l.trim_start().len();
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let bare = line.trim_start();
        let bare = bare.strip_prefix("- ").unwrap_or(bare);
        let Some(rest) = bare.strip_prefix("run:") else {
            i += 1;
            continue;
        };
        let base = indent_of(line);
        let head = rest.trim();
        // 블록 스칼라 표시가 없으면 같은 줄부터 명령이 시작된다.
        let literal = head.starts_with('|');
        let block = literal || head.starts_with('>');
        let mut collected: Vec<String> = Vec::new();
        if !block && !head.is_empty() {
            collected.push(head.to_string());
        }
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            if l.trim().is_empty() {
                collected.push(String::new());
                i += 1;
                continue;
            }
            if indent_of(l) <= base {
                break;
            }
            collected.push(l.trim().to_string());
            i += 1;
        }
        let mut cur = String::new();
        for l in &collected {
            if l.is_empty() {
                if !cur.trim().is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                continue;
            }
            let continues = !literal || l.ends_with('\\');
            cur.push_str(l.trim_end_matches('\\'));
            cur.push(' ');
            if !continues {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur);
        }
    }
    out
}

/// `run:` 안의 `cargo test` 호출들 — 명령 이름 뒤의 인자 꼬리로 돌려준다.
fn cargo_test_tails(body: &str) -> Vec<String> {
    run_commands(body)
        .iter()
        .filter_map(|cmd| cmd.split_once("cargo test").map(|(_, t)| t.to_string()))
        .collect()
}

/// feature 조합에 따라 테스트 실행 범위가 다르므로 따로 집계한다.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Combo {
    /// 기본 feature 조합 — 문서가 인용하는 `cargo test --workspace` 가 이것이다.
    Default,
    /// `--no-default-features`.
    Headless,
}

impl Combo {
    fn label(self) -> &'static str {
        match self {
            Combo::Default => "기본 조합",
            Combo::Headless => "헤드리스 조합",
        }
    }
}

/// 자동 잡의 `cargo test` 호출들 — (조합, 인자 꼬리).
fn automatic_test_invocations(root: &Path) -> Vec<(Combo, String)> {
    let mut out = Vec::new();
    // 합성 저장소도 받으므로 고정된 저장소 파일 수를 하한으로 쓰지 않는다.
    for body in automatic_job_bodies_of_dir(&root.join(".github/workflows"), &CALLER_SUPPLIED_FLOOR)
    {
        for tail in cargo_test_tails(&body) {
            let combo = if tail.contains("--no-default-features") {
                Combo::Headless
            } else {
                Combo::Default
            };
            out.push((combo, tail));
        }
    }
    out
}

/// 타깃 플래그뿐 아니라 패키지 선택도 실행 범위 제한으로 본다.
fn tail_is_narrowed(tail: &str) -> bool {
    tail.split_whitespace().any(|w| {
        w.starts_with("--lib")
            || w.starts_with("--bins")
            || w == "--test"
            || w == "-p"
            || w == "--package"
    })
}

fn integration_tests_run_automatically(root: &Path) -> Option<std::collections::BTreeSet<String>> {
    let mut named = std::collections::BTreeSet::new();
    {
        for (_combo, tail) in automatic_test_invocations(root) {
            let words: Vec<&str> = tail.split_whitespace().collect();
            if !tail_is_narrowed(&tail) {
                return None;
            }
            for pair in words.windows(2) {
                if pair[0] == "--test" {
                    named.insert(pair[1].to_string());
                }
            }
        }
    }
    Some(named)
}

/// 명시적 feature 선택은 두 조합만 다루는 모델로 해석할 수 없어 별도로 검출한다.
const UNMODELLED_TEST_FLAGS: &[&str] = &["--features", "--all-features"];

/// 실행 가능한 조합을 명시해 부재 주장을 한정한 표현.
const COMBO_QUALIFIED_MARKERS: &[&str] = &[
    "check-headless",
    "헤드리스 조합에서만",
    "헤드리스에서만",
    "기본 조합에는",
    "기본 조합 전용",
    "조합에서만",
    // 구체적 명령을 언급한 것만으로 한정 여부를 알 수 없어 명시적 표현을 등록한다.
    "조합 하나",
];

/// `key = "value"` 한 줄.
fn toml_string(block: &str, key: &str) -> Option<String> {
    for line in block.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        return Some(rest.trim().trim_matches('"').to_string());
    }
    None
}

/// `key = ["a", "b"]` 한 줄.
fn toml_array(block: &str, key: &str) -> Vec<String> {
    for line in block.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim().trim_start_matches('[').trim_end_matches(']');
        return rest
            .split(',')
            .map(|w| w.trim().trim_matches('"').to_string())
            .filter(|w| !w.is_empty())
            .collect();
    }
    Vec::new()
}

/// 워크스페이스의 매니페스트 경로들 — 루트 + `crates/*`.
fn manifests(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("Cargo.toml")];
    if let Ok(entries) = std::fs::read_dir(root.join("crates")) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("Cargo.toml"))
            .filter(|m| m.is_file())
            .collect();
        found.sort();
        out.extend(found);
    }
    out
}

/// 명시적 test 타깃의 required-features와 패키지 default feature를 읽어 조합별 빌드 여부를 판단한다.
fn test_target_features(
    root: &Path,
) -> std::collections::BTreeMap<String, (Vec<String>, Vec<String>)> {
    let mut out = std::collections::BTreeMap::new();
    for manifest in manifests(root) {
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let defaults = text
            .split_once("\n[features]")
            .map(|(_, rest)| toml_array(rest.split("\n[").next().unwrap_or(rest), "default"))
            .unwrap_or_default();
        for block in text.split("[[test]]").skip(1) {
            let block = block.split("\n[").next().unwrap_or(block);
            let required = toml_array(block, "required-features");
            if required.is_empty() {
                continue;
            }
            if let Some(name) = toml_string(block, "name") {
                out.insert(name, (required, defaults.clone()));
            }
        }
    }
    out
}

/// `--skip` 인자로 지목된 이름들.
fn skip_names(tail: &str) -> Vec<String> {
    let words: Vec<&str> = tail.split_whitespace().collect();
    words
        .windows(2)
        .filter(|p| p[0] == "--skip")
        .map(|p| p[1].to_string())
        .collect()
}

/// -- 뒤의 테스트 이름 필터와 --exact를 추출한다.
/// 호출부는 타깃 이름의 등장 여부만 보지 않고 필터가 해당 타깃의 테스트를 모두 포함하는지 확인한다.
fn positive_filters(tail: &str) -> (Vec<String>, bool) {
    let words: Vec<&str> = tail.split_whitespace().collect();
    let Some(sep) = words.iter().position(|w| *w == "--") else {
        return (Vec::new(), false);
    };
    let after = &words[sep + 1..];
    let exact = after.contains(&"--exact");
    let mut filters = Vec::new();
    let mut i = 0;
    while i < after.len() {
        let w = after[i];
        // 값을 하나 먹는 harness 플래그는 그 값까지 건너뛴다.
        if w == "--skip" || w == "--test-threads" || w == "--logfile" || w == "--format" {
            i += 2;
            continue;
        }
        if w.starts_with('-') {
            i += 1;
            continue;
        }
        filters.push(w.to_string());
        i += 1;
    }
    (filters, exact)
}

/// 한 파일에서 `#[test]` 가 붙은 함수 이름들.
fn test_fn_names(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut names = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() != "#[test]" {
            continue;
        }
        for next in lines.iter().skip(i + 1).take(4) {
            let t = next.trim_start();
            let t = t.strip_prefix("async ").unwrap_or(t);
            if let Some(rest) = t.strip_prefix("fn ")
                && let Some(name) = rest.split(['(', '<']).next()
                && !name.is_empty()
            {
                names.push(name.to_string());
                break;
            }
        }
    }
    names
}

/// #[test] 함수 이름과 뒤따르는 #[ignore] 여부를 읽는다. 일반 실행에서 제외되는 테스트를 구분한다.
fn test_fns_with_ignore(text: &str) -> Vec<(String, bool)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() != "#[test]" {
            continue;
        }
        let mut ignored = false;
        for next in lines.iter().skip(i + 1).take(4) {
            let t = next.trim_start();
            if t.starts_with("#[ignore") {
                ignored = true;
                continue;
            }
            let t = t.strip_prefix("async ").unwrap_or(t);
            if let Some(rest) = t.strip_prefix("fn ")
                && let Some(name) = rest.split(['(', '<']).next()
                && !name.is_empty()
            {
                out.push((name.to_string(), ignored));
                break;
            }
        }
    }
    out
}

/// 여러 줄에 나뉜 문구를 찾기 위해 연속 공백을 한 칸으로 합친다.
fn unwrapped(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "N 통과" 꼴 — 숫자와 통과/passed 가 붙어 있는 자리.
fn states_a_pass_count(flat: &str) -> bool {
    let bytes: Vec<char> = flat.chars().collect();
    for marker in ["통과", "passed"] {
        let mut from = 0;
        while let Some(pos) = flat[from..].find(marker) {
            let at = from + pos;
            from = at + marker.len();
            let head = flat[..at].chars().count();
            let lo = head.saturating_sub(6);
            if bytes[lo..head].iter().any(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// GUI 스위트의 통과 수를 적은 절을 찾아 1부터 시작하는 줄 번호를 반환한다.
/// 통과 수는 다음 제목 전까지만 찾고, 이를 한정하는 설명은 하위 절까지 포함해 찾는다.
/// 옆 절의 설명으로 면제하지는 않는다.
fn gui_pass_counts_missing_marker(text: &str, marker: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    // 첫 제목 앞의 머리말은 하위 절을 포함하지 않도록 가장 깊은 수준으로 둔다.
    let mut heads: Vec<(usize, usize)> = vec![(0, usize::MAX)];
    let mut fence = false;
    for (i, ln) in lines.iter().enumerate() {
        if ln.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        // 마크다운은 들여쓰기 3칸까지를 heading 으로 보고 4칸부터는 코드 블록으로 본다.
        let indent = ln.len() - ln.trim_start_matches(' ').len();
        let body = &ln[indent..];
        let hashes = body.chars().take_while(|c| *c == '#').count();
        if indent <= 3 && (1..=6).contains(&hashes) && body.chars().nth(hashes) == Some(' ') {
            heads.push((i, hashes));
        }
    }

    let mut out = Vec::new();
    for (k, &(start, level)) in heads.iter().enumerate() {
        let own_end = heads.get(k + 1).map_or(lines.len(), |&(i, _)| i);
        let own = unwrapped(&lines[start..own_end].join("\n"));
        if !own.contains("gui_tests") || !states_a_pass_count(&own) {
            continue;
        }
        let scope_end = heads[k + 1..]
            .iter()
            .find(|&&(_, l)| l <= level)
            .map_or(lines.len(), |&(i, _)| i);
        let scope = unwrapped(&lines[start..scope_end].join("\n"));
        if !scope.contains(marker) {
            out.push(start + 1);
        }
    }
    out
}

/// 통합 테스트 타깃 이름 -> 그 소스 경로.
fn integration_target_path(root: &Path, name: &str) -> Option<PathBuf> {
    let direct = root.join("tests").join(format!("{name}.rs"));
    if direct.is_file() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(root.join("crates")).ok()?.flatten() {
        let path = entry.path().join("tests").join(format!("{name}.rs"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// 패키지 선택 인자를 읽는다. 한 패키지의 실행을 전체 워크스페이스 실행으로 세면 안 된다.
fn packages_named(tail: &str) -> Vec<String> {
    let logical = logical_command(tail);
    let words: Vec<&str> = logical.split_whitespace().collect();
    let mut out: Vec<String> = words
        .windows(2)
        .filter(|w| w[0] == "-p" || w[0] == "--package")
        .map(|w| w[1].to_string())
        .collect();
    out.extend(
        words
            .iter()
            .filter_map(|w| {
                w.strip_prefix("--package=")
                    .or_else(|| w.strip_prefix("-p="))
            })
            .map(str::to_string),
    );
    out
}

/// 매니페스트의 `[package] name`.
fn package_name(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let block = text.split("[package]").nth(1)?;
    let block = block.split("\n[").next()?;
    toml_string(block, "name")
}

/// 타깃 경로에서 소유 패키지를 찾되, 이름은 디렉터리명이 아닌 Cargo.toml에서 읽는다.
fn owning_package(root: &Path, source: &Path) -> Option<String> {
    let rel = source.strip_prefix(root).ok()?;
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string());
    match parts.next()?.as_str() {
        "tests" => package_name(&root.join("Cargo.toml")),
        "crates" => {
            let dir = parts.next()?;
            package_name(&root.join("crates").join(dir).join("Cargo.toml"))
        }
        _ => None,
    }
}

/// 타깃의 요구 feature와 호출의 타깃·패키지·이름 필터를 적용해 실행 가능한 조합을 구한다.
fn integration_target_channels(
    root: &Path,
    target: &str,
    invocations: &[(Combo, String)],
    features: &std::collections::BTreeMap<String, (Vec<String>, Vec<String>)>,
) -> std::collections::BTreeSet<Combo> {
    let mut out = std::collections::BTreeSet::new();
    // 문서의 자리표시자를 실제 타깃으로 오해하지 않도록 파일 존재를 먼저 확인한다.
    let Some(source) = integration_target_path(root, target) else {
        return out;
    };
    let owner = owning_package(root, &source);
    for (combo, tail) in invocations {
        // `-p` 로 좁힌 호출은 그 패키지의 타깃만 돌린다.
        let named_packages = packages_named(tail);
        if !named_packages.is_empty() && !owner.as_ref().is_some_and(|o| named_packages.contains(o))
        {
            continue;
        }
        if let Some((required, defaults)) = features.get(target) {
            let enabled: &[String] = match combo {
                Combo::Default => defaults,
                Combo::Headless => &[],
            };
            if !required.iter().all(|r| enabled.contains(r)) {
                continue;
            }
        }
        let words: Vec<&str> = tail.split_whitespace().collect();
        let named: Vec<&str> = words
            .windows(2)
            .filter(|p| p[0] == "--test")
            .map(|p| p[1])
            .collect();
        if named.is_empty() {
            // `--lib`/`--bins` 로 좁힌 호출은 통합 타깃을 하나도 만들지 않는다.
            if words.iter().any(|w| *w == "--lib" || *w == "--bins") {
                continue;
            }
        } else if !named.contains(&target) {
            continue;
        }
        let skips = skip_names(tail);
        if !skips.is_empty()
            && let Ok(text) = std::fs::read_to_string(&source)
        {
            let names = test_fn_names(&text);
            if !names.is_empty() && names.iter().all(|n| skips.iter().any(|s| n.contains(s))) {
                continue;
            }
        }
        // 양성 필터가 타깃을 **좁히면** 그 호출은 타깃 전체의 채널이 아니다(위 doc).
        let (filters, exact) = positive_filters(tail);
        if !filters.is_empty()
            && let Ok(text) = std::fs::read_to_string(&source)
        {
            let names = test_fn_names(&text);
            let covers_all = !names.is_empty()
                && names.iter().all(|n| {
                    filters
                        .iter()
                        .any(|f| if exact { n == f } else { n.contains(f) })
                });
            if !covers_all {
                continue;
            }
        }
        out.insert(*combo);
    }
    out
}

/// 자동 잡의 호출이 본체 lib 테스트를 포함하는지 워크플로에서 확인한다.
fn lib_tests_run_automatically(root: &Path) -> bool {
    let main = package_name(&root.join("Cargo.toml"));
    automatic_test_invocations(root).iter().any(|(_, tail)| {
        // `-p <다른 패키지>` 로 좁힌 잡은 본체의 lib 유닛을 안 돌린다.
        let named_packages = packages_named(tail);
        if !named_packages.is_empty() && !main.as_ref().is_some_and(|m| named_packages.contains(m))
        {
            return false;
        }
        let words: Vec<&str> = tail.split_whitespace().collect();
        if words.contains(&"--lib") {
            return true;
        }
        // `--test`/`--bins` 로만 좁힌 호출은 lib 유닛 테스트를 돌리지 않는다.
        !words.iter().any(|w| *w == "--test" || *w == "--bins")
    })
}

/// 통합 테스트의 실행 경로가 없다고 주장한 설명만 검사한다.
fn overstated_absence(
    text: &str,
    path: &str,
    channels_of: &dyn Fn(&str) -> std::collections::BTreeSet<Combo>,
    class_channels: &std::collections::BTreeSet<Combo>,
) -> Vec<(usize, String)> {
    // 주장 파일의 경로로 면제하지 않는다. 인용된 tests/X.rs 타깃의 실행 경로를 확인한다.
    let mut found = Vec::new();
    for at in absence_offsets(text) {
        if !is_prose_line(text, at, path) {
            continue;
        }
        let scope = claim_scope(text, at);
        // 여러 타깃을 인용한 설명도 한 건으로 보고한다.
        // 문장의 실제 주어와 덧붙인 참조는 구별할 수 없어 같은 범위의 실행 타깃을 함께 알린다.
        let mut running: Vec<(String, Vec<&str>)> = Vec::new();
        let mut cited = cited_tests(scope);
        // 다른 타깃을 지목하지 않은 모듈 주석은 명시적인 자기 지칭이 있을 때만 파일 자신에 대한 설명으로 본다.
        let in_module_doc = text[text[..at].rfind('\n').map_or(0, |i| i + 1)..]
            .trim_start()
            .starts_with("//!");
        if cited.is_empty()
            && in_module_doc
            && speaks_of_itself(scope)
            && !is_quoted(text, scope, at)
            && let Some(own) = integration_test_name(path)
        {
            cited.push((at, own.to_string()));
        }
        for (_, target) in cited {
            let channels = channels_of(&target);
            if channels.is_empty() {
                continue;
            }
            let labels: Vec<&str> = channels.iter().map(|c| c.label()).collect();
            if running.iter().all(|(name, _)| *name != target) {
                running.push((target, labels));
            }
        }

        // tests/*.rs 같은 부류 인용은 전체 통합 타깃의 실행 조합을 합쳐 판단한다.
        if running.is_empty() && !class_channels.is_empty() && cites_the_test_class(scope) {
            let labels: Vec<&str> = class_channels.iter().map(|c| c.label()).collect();
            running.push(("*".to_string(), labels));
        }
        if running.is_empty() {
            continue;
        }
        let both = running.iter().any(|(_, labels)| labels.len() >= 2);
        if !both && COMBO_QUALIFIED_MARKERS.iter().any(|m| scope.contains(m)) {
            continue;
        }
        let listed: Vec<String> = running
            .iter()
            .map(|(name, labels)| format!("tests/{name}.rs({})", labels.join(" · ")))
            .collect();
        found.push((
            at,
            if both {
                format!("{} — 두 조합 모두에서 돈다", listed.join(", "))
            } else {
                format!("{} — 조합을 한정해서 적어라", listed.join(", "))
            },
        ));
    }
    found.sort();
    found.dedup();
    found
}

/// 모듈 주석에서 다른 검사나 전제의 설명을 파일 자신에 대한 주장으로 오해하지 않도록 자기 지칭을 확인한다.
fn speaks_of_itself(scope: &str) -> bool {
    ["이 테스트", "이 가드", "이 파일", "this test", "This test"]
        .iter()
        .any(|m| scope.contains(m))
}

/// 주장 대신 예시로 인용한 문구인지 확인한다. at은 text 기준, scope는 해당 부분 문자열이다.
fn is_quoted(text: &str, scope: &str, at: usize) -> bool {
    let base = scope.as_ptr() as usize - text.as_ptr() as usize;
    if at < base {
        return false;
    }
    let rel = (at - base).min(scope.len());
    scope
        .char_indices()
        .take_while(|(i, _)| *i < rel)
        .filter(|(_, c)| *c == '"' || *c == '\u{201c}' || *c == '\u{201d}')
        .count()
        % 2
        == 1
}

/// 경로가 통합 테스트면 그 테스트 이름 — `tests/X.rs` 와 `crates/<c>/tests/X.rs` 둘 다.
fn integration_test_name(rel: &str) -> Option<&str> {
    let after = rel.rsplit_once("tests/")?.1;
    if after.contains('/') {
        return None;
    }
    after.strip_suffix(".rs")
}

/// tests/*.rs 같은 와일드카드 경로를 통합 테스트 부류의 인용으로 읽는다.
fn cites_the_test_class(scope: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = scope[from..].find("tests/") {
        let at = from + rel;
        from = at + "tests/".len();
        let rest = &scope[from..];
        let end = rest
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '*'))
            .map_or(rest.len(), |(i, _)| i);
        if rest[..end].contains('*') && rest[end..].starts_with(".rs") {
            return true;
        }
    }
    false
}

/// 텍스트가 지목하는 통합 테스트 인용 지점 — (오프셋, 이름).
fn cited_tests(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find("tests/") {
        let at = from + rel;
        from = at + "tests/".len();
        let rest = &text[from..];
        let end = rest
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(rest.len(), |(i, _)| i);
        if rest[end..].starts_with(".rs") {
            found.push((at, rest[..end].to_string()));
        }
    }
    found
}

/// src/ 안에서 #[test] 함수 이름을 수집한다.
fn lib_test_names(root: &Path) -> std::collections::BTreeSet<String> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    let mut names = std::collections::BTreeSet::new();
    for file in files {
        let rel = normalized_rel(&file, root);
        if !rel.ends_with(".rs") || !(rel.starts_with("src/") || rel.contains("/src/")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        names.extend(test_fn_names(&text));
    }
    names
}

/// 부재 표지가 놓인 오프셋들.
fn absence_offsets(text: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for marker in ABSENCE_MARKERS {
        let mut from = 0;
        while let Some(rel) = text[from..].find(marker) {
            let at = from + rel;
            from = at + marker.len();
            found.push(at);
        }
    }
    found.sort_unstable();
    found
}

/// `text` 안에서 `name` 이 **낱말로** 등장하는 오프셋들.
fn word_offsets(text: &str, name: &str) -> Vec<usize> {
    let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        if boundary(before) && boundary(after) {
            found.push(at);
        }
    }
    found
}

/// 오류를 수정할 사람이 판정 범위를 알 수 있도록 진단에 함께 표시한다.
const SCOPE_NOTE: &str = "\n\n  [범위] 표는 한 행, 목록은 이어진 줄을 포함한 한 항목, 산문은 빈 줄 사이 한 문단, Rust는 연속된 주석 블록을 읽는다. 같은 범위의 다른 문장에 있는 표지도 함께 판정한다.";

/// 같은 범위의 인용이 실제 주어인지 알 수 없다는 한계와 수정 방법을 함께 알린다.
const SUBJECT_NOTE: &str = "\n  [주어] 같은 범위의 인용이 실제 문장의 주어인지 덧붙인 참조인지 구별하지 못한다. 실행 주장의 대상이 맞다면 내용을 사실에 맞게 고친다. 무관한 참조일 때만 별도 항목으로 분리한다.";

/// 과거 시점을 나타내는 표지와 설명. 검출을 면제하지 않고 진단 방법만 바꾼다.
/// 표지가 없다고 반드시 현재에 대한 주장인 것은 아니다.
const TIME_MARKERS: &[(&str, &str)] = &[
    (
        "한 날의 상태",
        "제목 관용구 — 그 절 전체가 배선하던 날의 기록이라는 선언이다",
    ),
    ("당시", "과거의 한 때를 가리키는 부사"),
    ("그때", "과거의 한 때를 가리키는 부사"),
    (
        "한때",
        "이제는 아니라는 것을 문장 자신이 이미 말한다 — 고치면 그 대비가 사라진다",
    ),
    (
        "실측",
        "잰 값의 기록 — 잰 날에 대해 참이고 지금에 대해 참일 의무가 없다",
    ),
    (
        "도입 시점",
        "무엇이 들어오던 때를 명시하는 말 — 결정의 근거로 남는 값이다",
    ),
    (
        "결정 시점",
        "무엇을 정하던 때를 명시하는 말 — ADR 본문이 이 모양이다",
    ),
];

/// 시점 낱말 없이 날짜만 적은 경우도 YYYY-MM-DD 모양으로 찾는다.
fn iso_date_in(scope: &str) -> Option<String> {
    let b = scope.as_bytes();
    if b.len() < 10 {
        return None;
    }
    for i in 0..=b.len() - 10 {
        let w = &b[i..i + 10];
        let d = |k: usize| w[k].is_ascii_digit();
        if (0..4).all(d) && w[4] == b'-' && (5..7).all(d) && w[7] == b'-' && (8..10).all(d) {
            return Some(String::from_utf8_lossy(w).into_owned());
        }
    }
    None
}

/// 이 범위가 때를 못박고 있으면 그 **표지**를 낸다.
fn time_pin(scope: &str) -> Option<String> {
    for (marker, _) in TIME_MARKERS {
        if scope.contains(marker) {
            return Some((*marker).to_string());
        }
    }
    iso_date_in(scope)
}

/// 진단 항목 바로 옆에 시점 표지를 표시한다.
fn with_time_mark(entry: String, scope: &str) -> String {
    match time_pin(scope) {
        Some(marker) => {
            format!(
                "{entry}\n      [시점 표지 \"{marker}\"] 과거 기록일 수 있으므로 아래 [시점] 안내를 확인한다."
            )
        }
        None => entry,
    }
}

/// 과거 사실을 현재 구성에 맞춰 바꾸지 않도록 시점 표지가 있는 진단에 안내를 붙인다.
const TIME_NOTE: &str = "\n  [시점] 과거 기록이라면 당시 사실을 현재 값으로 바꾸지 않는다. 시점을 분명히 적고 현재 구성은 안내 문서로 연결한다. 시점 표지가 없는 문장도 과거 기록일 수 있으므로 먼저 문맥을 확인한다. 새 표지를 발견하면 TIME_MARKERS에 사유와 함께 추가한다.";

/// 표는 한 행, 목록은 이어진 줄을 포함한 한 항목, 산문은 빈 줄 사이 문단을 읽는다.
/// Rust 문서 주석은 빈 주석 줄을 경계로 한다. 검출과 면제에 같은 범위를 쓴다.
fn claim_scope(text: &str, at: usize) -> &str {
    let line_start = |i: usize| text[..i].rfind('\n').map_or(0, |j| j + 1);
    let line_end = |i: usize| text[i..].find('\n').map_or(text.len(), |j| i + j);
    let kind = |start: usize| -> u8 {
        let line = text[start..line_end(start)].trim_start();
        if line.starts_with('|') {
            0 // 표 행 — 혼자 선다
        } else if line.starts_with("//!") || line.starts_with("///") {
            if line.trim_end().len() <= 3 { 2 } else { 1 } // 주석 / 빈 주석(경계)
        } else if line.trim().is_empty() {
            2 // 빈 줄 — 경계
        } else {
            3 // 산문
        }
    };

    let starts_item = |start: usize| {
        let line = text[start..line_end(start)].trim_start();
        line.starts_with("- ")
            || line.starts_with("* ")
            || line.split_once(". ").is_some_and(|(head, _)| {
                !head.is_empty() && head.bytes().all(|b| b.is_ascii_digit())
            })
    };

    let mut lo = line_start(at);
    let mut hi = line_end(at);
    let here = kind(lo);
    if here == 0 || here == 2 {
        return &text[lo..hi];
    }
    while lo > 0 && !starts_item(lo) {
        let prev = line_start(lo - 1);
        if kind(prev) != here {
            break;
        }
        lo = prev;
    }
    while hi < text.len() {
        let next = hi + 1;
        if next >= text.len() || kind(next) != here || starts_item(next) {
            break;
        }
        hi = line_end(next);
    }
    &text[lo..hi]
}

/// Rust 파일은 해당 줄이 //로 시작할 때만 산문으로 읽는다. 완전한 Rust 렉싱은 하지 않는다.
fn is_prose_line(text: &str, at: usize, rel: &str) -> bool {
    if !rel.ends_with(".rs") {
        return true;
    }
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    text[start..end].trim_start().starts_with("//")
}

/// 한 파일의 실행 주장 위반 위치와 인용 수. 합성 문자열로도 판정을 확인할 수 있게 순회와 분리한다.
fn enforcement_violations(
    text: &str,
    path: &str,
    automatic: &std::collections::BTreeSet<String>,
) -> (Vec<usize>, usize) {
    let mut candidates = cited_tests(text);
    let cited = candidates.len();
    // 통합 타깃은 자기 경로 없이 실행 표지만 적은 경우도 자기 인용으로 센다.
    if let Some(own) = integration_test_name(path) {
        for marker in ENFORCE_MARKERS {
            let mut from = 0;
            while let Some(off) = text[from..].find(marker) {
                let at = from + off;
                from = at + marker.len();
                candidates.push((at, own.to_string()));
            }
        }
    }

    let mut found = Vec::new();
    for (at, name) in candidates {
        if automatic.contains(&name) {
            continue;
        }
        if !is_prose_line(text, at, path) {
            continue;
        }
        // 다른 행·항목의 표지로 판정하지 않도록 범위를 제한한다.
        let scope = claim_scope(text, at);
        if !ENFORCE_MARKERS.iter().any(|m| scope.contains(m)) {
            continue;
        }
        if absence_exempts(scope) {
            continue;
        }
        // 컴파일·정적 검사 주장은 실행 주장과 구분해 제외한다.
        if COMPILE_CLAIM_MARKERS.iter().any(|m| scope.contains(m)) {
            continue;
        }
        found.push(at);
    }
    found.sort_unstable();
    found.dedup();
    (found, cited)
}

/// 한 파일에서 **lib 테스트를 두고 부재를 적은 자리** — 역방향 판정기.
fn weak_absence_offsets(
    text: &str,
    path: &str,
    lib_tests: &std::collections::BTreeSet<String>,
) -> Vec<usize> {
    // 이 파일이 직접 정의하는 테스트만 제외한다. 같은 파일의 다른 테스트 인용은 검사한다.
    let defined_here: std::collections::BTreeSet<String> =
        test_fn_names(text).into_iter().collect();
    let mut found = Vec::new();
    // 전체 테스트 이름을 대조하기 전에 부재 표지가 있는 위치로 좁힌다.
    for at in absence_offsets(text) {
        if !is_prose_line(text, at, path) {
            continue;
        }
        let scope = claim_scope(text, at);
        if AUTOMATIC_CHANNEL_MARKERS.iter().any(|m| scope.contains(m)) {
            continue;
        }
        if lib_tests
            .iter()
            .any(|n| !defined_here.contains(n) && !word_offsets(scope, n).is_empty())
        {
            found.push(at);
        }
    }
    found
}

/// 문서가 "CI 가 전체 스위트를 돌린다" 고 말하면, 실제로 그런지 워크플로와 대조한다.
#[test]
fn no_file_claims_ci_runs_the_full_suite_while_it_does_not() {
    let root = repo_root();
    if ci_actually_runs_the_full_suite(&root) {
        // 기본 조합의 전체 호출이 있으면 이 부재 검사를 생략한다.
        return;
    }

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "스캔 대상 파일이 하나도 없다 — 수집이 깨졌다"
    );

    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for at in claim_offsets(&text, &rel_str) {
            let entry = format!("{rel_str}:{}", line_of(&text, at));
            violations.push(with_time_mark(entry, claim_scope(&text, at)));
        }
    }

    assert!(
        violations.is_empty(),
        "기본 조합의 전체 스위트 자동 호출을 찾지 못했지만 아래 설명은 CI 실행을 주장한다. docs/dev-guide/ci-gates.md와 워크플로를 대조해 실행 주장을 바로잡는다:\n  {}{SCOPE_NOTE}{SUBJECT_NOTE}{TIME_NOTE}",
        violations.join("\n  ")
    );
}

/// 실행 가능한 조합이 없는 통합 타깃을 CI 검사로 소개하는지 확인한다.
/// 이름 열거 검사와 달리 제한 없는 호출에서도 --skip으로 타깃 전체가 빠진 경우를 찾는다.
#[test]
fn no_file_claims_ci_enforces_an_integration_target_no_automatic_job_runs() {
    let root = repo_root();
    let invocations = automatic_test_invocations(&root);
    let features = test_target_features(&root);

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    assert!(
        files.len() >= MIN_SCANNED_FILES,
        "수집한 파일이 {}개로 하한 {MIN_SCANNED_FILES}보다 적다. 하한을 낮추기 전에 수집 범위를 확인한다.",
        files.len()
    );

    // 실행 가능한 조합이 있는 통합 타깃을 모아 실행 주장과 대조한다.
    let mut running = std::collections::BTreeSet::new();
    let mut dark = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        // 자기 인용을 판정하는 크레이트 통합 타깃도 포함한다.
        if !rel_str.ends_with(".rs") || !rel_str.contains("tests/") {
            continue;
        }
        let Some(name) = integration_test_name(&rel_str) else {
            continue;
        };
        if integration_target_channels(&root, name, &invocations, &features).is_empty() {
            if !dark.contains(&name.to_string()) {
                dark.push(name.to_string());
            }
        } else {
            running.insert(name.to_string());
        }
    }

    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let (found, _) = enforcement_violations(&text, &rel_str, &running);
        for at in found {
            let entry = format!("{rel_str}:{}", line_of(&text, at));
            violations.push(with_time_mark(entry, claim_scope(&text, at)));
        }
    }
    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "실행 가능한 자동 조합이 없는 통합 타깃을 CI 검사로 소개한 설명이다. 해당 타깃: {dark:?}. 실행 주장을 수정하거나 실제 자동 실행을 구성한다. 실행 경로는 docs/dev-guide/ci-gates.md에서 확인한다:\n  {}{SCOPE_NOTE}{SUBJECT_NOTE}{TIME_NOTE}",
        violations.join("\n  ")
    );
}

/// 문서가 어떤 통합 테스트를 자동 집행 장치로 부르면, 자동 잡이 실제로 그 이름을
/// 돌리는지 워크플로에서 읽어 대조한다.
#[test]
fn no_file_claims_ci_enforces_an_integration_test_it_does_not_run() {
    let root = repo_root();
    let Some(automatic) = integration_tests_run_automatically(&root) else {
        // 타깃·패키지 제한 없는 호출이 있으면 이름 열거 검사는 생략한다.
        return;
    };

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    assert!(
        files.len() >= MIN_SCANNED_FILES,
        "수집한 파일이 {}개로 하한 {MIN_SCANNED_FILES}보다 적다. SKIP_DIRS·TEXT_EXTS 변경과 실제 파일 삭제를 구별한다. 수집 오류를 고치고 실제 대상이 줄었을 때만 하한을 다시 정한다.",
        files.len()
    );

    let mut citations = 0usize;
    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };

        let (found, cited) = enforcement_violations(&text, &rel_str, &automatic);
        citations += cited;
        for at in found {
            let entry = format!("{rel_str}:{}", line_of(&text, at));
            violations.push(with_time_mark(entry, claim_scope(&text, at)));
        }
    }

    assert!(
        citations >= MIN_TEST_CITATIONS,
        "통합 타깃 인용을 {citations}개만 찾았다(하한 {MIN_TEST_CITATIONS}). 추출 범위를 확인하고 integration_tests_run_automatically가 Some을 반환한 이유를 확인한다."
    );

    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "자동 호출에 명시된 통합 타깃 {automatic:?} 밖의 테스트를 CI 검사로 소개한 설명이다. 테스트 자체의 설명은 유지하고 실행 주장을 docs/dev-guide/ci-gates.md와 맞춘다:\n  {}{SCOPE_NOTE}{SUBJECT_NOTE}{TIME_NOTE}",
        violations.join("\n  ")
    );
}

/// 자동 lib 호출이 있는데 lib 테스트의 실행 경로가 없다고 적은 설명을 찾는다.
#[test]
fn no_file_denies_the_automatic_channel_a_lib_test_actually_has() {
    let root = repo_root();
    if !lib_tests_run_automatically(&root) {
        // 자동 lib 호출이 없으면 부재 주장을 오류로 보지 않는다.
        return;
    }
    let lib_tests = lib_test_names(&root);
    assert!(
        lib_tests.len() >= MIN_LIB_TESTS,
        "lib 테스트 이름을 {}개만 찾았다(하한 {MIN_LIB_TESTS}). 실제 테스트가 줄었는지 확인한다. 소스 수는 그대로인데 추출 결과만 줄었다면 lib_test_names를 고치고 하한은 유지한다.",
        lib_tests.len()
    );

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for at in weak_absence_offsets(&text, &rel_str, &lib_tests) {
            let entry = format!("{rel_str}:{}", line_of(&text, at));
            violations.push(with_time_mark(entry, claim_scope(&text, at)));
        }
    }

    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "자동 lib 호출이 있는데 아래 설명은 lib 테스트의 실행 경로가 없다고 적고 있다. docs/dev-guide/ci-gates.md와 대조한다:\n  {}{SCOPE_NOTE}{SUBJECT_NOTE}{TIME_NOTE}",
        violations.join("\n  ")
    );
}

/// 통합 타깃의 자동 실행 조합 수와 부재 주장을 대조한다.
/// 조합이 하나면 한정 표현을 요구하고, 둘 모두 있으면 부재 주장을 오류로 본다.
#[test]
fn no_file_denies_a_channel_an_integration_test_actually_has() {
    let root = repo_root();
    let invocations = automatic_test_invocations(&root);
    let unmodelled: Vec<String> = invocations
        .iter()
        .flat_map(|(_, tail)| {
            UNMODELLED_TEST_FLAGS
                .iter()
                .filter(|f| tail.split_whitespace().any(|w| w == **f))
                .map(|f| format!("{f} in `cargo test{tail}`"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        unmodelled.is_empty(),
        "자동 호출에 명시적 feature 선택이 있다. 기본·헤드리스 두 조합만 처리하는 모델을 먼저 보완해야 한다:\n  {}",
        unmodelled.join("\n  ")
    );

    let features = test_target_features(&root);
    let channels_of =
        |target: &str| integration_target_channels(&root, target, &invocations, &features);

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    assert!(
        files.len() >= MIN_SCANNED_FILES,
        "수집한 파일이 {}개로 하한 {MIN_SCANNED_FILES}보다 적다. SKIP_DIRS·TEXT_EXTS와 실제 파일 삭제를 확인한다. 수집 오류를 고치고 실제 대상이 줄었을 때만 하한을 다시 정한다.",
        files.len()
    );

    // tests/*.rs 부류는 루트 패키지의 통합 타깃으로 해석한다.
    // 크레이트별 패키지 호출까지 합치면 루트 타깃의 실행 범위를 과장하게 된다.
    let mut class_channels = std::collections::BTreeSet::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        if !rel_str.ends_with(".rs") || !rel_str.starts_with("tests/") {
            continue;
        }
        if let Some(name) = integration_test_name(&rel_str) {
            class_channels.extend(channels_of(name));
        }
    }

    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for (at, why) in overstated_absence(&text, &rel_str, &channels_of, &class_channels) {
            let entry = format!("{rel_str}:{} — {why}", line_of(&text, at));
            violations.push(with_time_mark(entry, claim_scope(&text, at)));
        }
    }
    violations.sort();
    violations.dedup();
    assert!(
        violations.is_empty(),
        "아래는 통합 테스트를 두고 자동 채널의 부재를 적었지만, 그 테스트가 실제로는 \
         자동으로 도는 자리다. 조합 정본은 `docs/dev-guide/ci-gates.md`:\n  {}{SCOPE_NOTE}{SUBJECT_NOTE}{TIME_NOTE}",
        violations.join("\n  ")
    );
}

// GUI 테스트의 실행 경로, ignore 상태, 통과 수의 해석을 각각 검사한다.

/// multi_window_owner_routing을 이름으로 선택하는 자동 호출과 ignore 부재를 확인한다.
/// 잡 분할의 정확성은 workflow_triggers 테스트에서 별도로 검증한다.
#[test]
fn the_gui_layer_a_display_revives_is_exactly_the_one_named_test() {
    const THE_ONE: &str = "multi_window_owner_routing";
    let root = repo_root();

    let e2e = integration_target_path(&root, "e2e_tests").expect("tests/e2e_tests.rs 가 없다");
    let e2e_text = std::fs::read_to_string(&e2e).expect("e2e_tests.rs 를 읽지 못했다");
    let e2e_fns = test_fns_with_ignore(&e2e_text);
    assert!(
        e2e_fns.len() > 10,
        "e2e_tests에서 테스트를 {}개만 추출했다. 수집 범위를 확인한다.",
        e2e_fns.len()
    );
    let one = e2e_fns
        .iter()
        .find(|(n, _)| n == THE_ONE)
        .expect("multi_window_owner_routing을 찾지 못했다. 이름이 바뀌었다면 대상과 자동 호출을 함께 확인한다.");
    assert!(
        !one.1,
        "{THE_ONE}에 #[ignore]가 붙어 이름을 선택하는 일반 호출로는 실행되지 않는다."
    );

    let invocations = automatic_test_invocations(&root);
    assert!(
        !invocations.is_empty(),
        "자동 잡의 cargo test 호출을 찾지 못했다. 수집 범위와 명령 판독을 확인한다."
    );
    let selected = invocations.iter().any(|(_, tail)| {
        let (filters, exact) = positive_filters(tail);
        filters.iter().any(|f| {
            if exact {
                f == THE_ONE
            } else {
                THE_ONE.contains(f.as_str())
            }
        })
    });
    assert!(
        selected,
        "multi_window_owner_routing을 이름으로 선택해 실행하는 자동 호출이 없다. 워크플로의 해당 스텝을 확인한다."
    );
}

/// markdown WebKitGTK 시험을 이름으로 고르는 자동 호출이 실제 시험 이름과 맞는지 확인한다.
/// 지목 실행은 이름이 어긋나면 `0 passed` 로 통과하므로, 시험 이름과 호출을 여기서 함께 대조한다.
/// GTK 는 한 프로세스에서 한 스레드만 초기화하므로 파일마다 시험을 하나 두고 각각 따로 지목한다.
#[test]
fn the_webview_layout_step_names_the_one_layout_test() {
    for leaf in ["webview_layout_tests", "bed_focus_webkit_tests"] {
        assert_one_named_webkit_test(leaf);
    }
}

fn assert_one_named_webkit_test(leaf: &str) {
    const PACKAGE: &str = "tasty-plugin-markdown";
    const PARENT: &str = "crates/tasty-plugin-markdown/src/render.rs";
    let module = format!("render::{leaf}");
    let file = format!("crates/tasty-plugin-markdown/src/render/{leaf}.rs");
    let root = repo_root();

    let parent = std::fs::read_to_string(root.join(PARENT)).expect("render.rs 를 읽지 못했다");
    assert!(
        parent.contains(&format!("mod {leaf};")),
        "{PARENT} 가 {leaf} 모듈을 선언하지 않는다. 모듈 경로 {module} 가 바뀌었다면 이 검사와 워크플로를 함께 고친다."
    );
    let text = std::fs::read_to_string(root.join(&file))
        .unwrap_or_else(|e| panic!("{file} 를 읽지 못했다: {e}"));
    let fns = test_fns_with_ignore(&text);
    assert_eq!(
        fns.len(),
        1,
        "{file} 의 시험이 {}개다({fns:?}). 지목 실행은 한 이름만 고르므로 시험을 늘리면 워크플로 호출과 이 검사를 함께 바꾼다.",
        fns.len()
    );
    let (name, ignored) = &fns[0];
    assert!(
        *ignored,
        "{name} 에 #[ignore] 가 없다. Xvfb 안에서 도는 차단 단계인 --lib --bins 단계에서도 실행된다."
    );
    let full = format!("{module}::{name}");

    let invocations = automatic_test_invocations(&root);
    assert!(
        !invocations.is_empty(),
        "자동 잡의 cargo test 호출을 찾지 못했다. 수집 범위와 명령 판독을 확인한다."
    );
    let selected = invocations.iter().any(|(_, tail)| {
        let words: Vec<&str> = tail.split_whitespace().collect();
        let package = words.windows(2).any(|w| w[0] == "-p" && w[1] == PACKAGE);
        let (filters, exact) = positive_filters(tail);
        package && exact && words.contains(&"--ignored") && filters.contains(&full)
    });
    assert!(
        selected,
        "`-p {PACKAGE} … -- --ignored --exact {full}` 로 이 시험을 고르는 자동 호출이 없다. 시험 이름이 바뀌었다면 워크플로의 webview layout 단계를 함께 고친다."
    );
}

/// gui_tests 전체가 #[ignore] 상태인지 확인한다. 디스플레이만 있어서는 일반 실행에 포함되지 않는다.
#[test]
fn the_gui_suite_needs_a_flag_not_a_display() {
    let root = repo_root();
    let gui = integration_target_path(&root, "gui_tests").expect("tests/gui_tests.rs 가 없다");
    let gui_text = std::fs::read_to_string(&gui).expect("gui_tests.rs 를 읽지 못했다");
    let gui_fns = test_fns_with_ignore(&gui_text);
    assert!(
        gui_fns.len() > 10,
        "gui_tests에서 시험을 {}건만 추출했다. 함수 판독을 확인한다.",
        gui_fns.len()
    );
    let running: Vec<&String> = gui_fns
        .iter()
        .filter(|(_, ig)| !ig)
        .map(|(n, _)| n)
        .collect();
    assert!(
        running.is_empty(),
        "gui_tests에 #[ignore]가 없는 테스트가 있다: {running:?}. 전체가 일반 실행에서 제외된다는 설명과 실행 경로를 다시 확인한다."
    );
}

/// 초기화 클로저의 swap(true와 첫 lock 이후의 into_inner() 문자열을 찾는다.
/// SpawnOnceLatch 같은 헬퍼를 해석하지 못하므로 실제 재시도나 락 오염 여부를 입증하지 않는다.
/// 현재 GUI 하네스는 SpawnOnceLatch로 재시도를 막지만 이 함수는 이를 인식하지 못한다.
/// 구조를 찾지 못하면 None으로 반환하고 호출부가 실패시킨다.
fn gui_amplifiers_live(src: &str) -> Option<(bool, bool)> {
    let at = src.find("static SHARED_INSTANCE")?;
    let tail = &src[at..];
    let init = tail.find("get_or_init")?;
    let open = init + tail[init..].find('{')?;
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in tail[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let closure = &tail[open..=close?];
    let a_live = !closure.contains("swap(true");

    let lock = tail.find(".lock()")?;
    let window = &tail[lock..(lock + 160).min(tail.len())];
    let b_live = !window.contains("into_inner()");
    Some((a_live, b_live))
}

/// GUI 통과 수를 적은 절에 결과를 해석할 때의 한계를 함께 설명하도록 요구한다.
/// gui_amplifiers_live의 텍스트 판독은 실제 실패 재시도 여부를 증명하지 못한다.
/// 하네스 구조가 바뀌면 검사 모델과 문서 근거를 함께 재검토해야 한다.
#[test]
fn the_gui_ignored_layer_has_no_single_value() {
    const MARKER: &str = "단일 값이 없다";
    const CLAIM_DOC: &str = "docs/dev-guide/ci-gates.md";
    let root = repo_root();

    let common = root.join("tests/gui_common/mod.rs");
    let common_text = std::fs::read_to_string(&common).expect("tests/gui_common/mod.rs 가 없다");
    let (a_live, b_live) = gui_amplifiers_live(&common_text).expect(
        "GUI 공유 인스턴스 구조를 찾지 못해 판정할 수 없다. gui_amplifiers_live와 실제 하네스를 대조한다.",
    );
    assert!(
        a_live || b_live,
        "GUI 하네스의 텍스트 표지 판정이 달라졌다. 실제 재시도·락 처리와 gui_amplifiers_live의 한계를 확인하고 '{MARKER}' 설명의 근거를 다시 검토한다."
    );

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    let mut scanned = 0usize;
    let mut carries_marker = false;
    let mut violations = Vec::new();
    for file in &files {
        let rel_str = normalized_rel(file, &root);
        if !rel_str.ends_with(".md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        if !unwrapped(&text).contains("gui_tests") {
            continue;
        }
        scanned += 1;
        if rel_str == CLAIM_DOC {
            carries_marker = unwrapped(&text).contains(MARKER);
        }
        for line in gui_pass_counts_missing_marker(&text, MARKER) {
            violations.push(format!("{rel_str}:{line}"));
        }
    }
    assert!(
        scanned > 0,
        "gui_tests를 언급하는 문서를 찾지 못했다. 문서 수집 범위를 확인한다."
    );
    assert!(
        carries_marker,
        "{CLAIM_DOC}에 결과의 한계 표지 '{MARKER}'가 없다. 현재 하네스 동작과 설명을 함께 확인한다."
    );
    assert!(
        violations.is_empty(),
        "GUI 스위트 통과 수를 적은 절에 '{MARKER}'라는 한계 설명이 없다. 실행 방식에 따라 결과가 달라질 수 있으므로 같은 절이나 하위 절에 설명을 추가한다:\n  {}",
        violations.join("\n  ")
    );
}

/// 자동 호출의 ignored 관련 플래그와 문서의 부재 설명이 모순되는지 확인한다.
/// 수동으로 분류된 잡은 제외한다. 실행 경로가 생겼을 때 부재 표지만 지우고 새 설명을 쓰지 않아도 통과한다.
#[test]
fn the_gui_suite_channel_claim_points_the_same_way_as_the_workflows() {
    const ABSENCE_CLAIM: &str = "`gui_tests` 에 자동 채널이 없다";
    const CLAIM_DOC: &str = "docs/dev-guide/ci-gates.md";
    let root = repo_root();

    let bodies = automatic_job_bodies_of_dir(&root.join(".github/workflows"), &WORKFLOW_FLOOR);
    assert!(
        bodies.len() >= 8,
        "자동 잡을 {}개만 읽었다. 수집이 누락되면 ignored 실행 플래그의 부재를 잘못 판단할 수 있다.",
        bodies.len()
    );

    // 설명 주석에 적힌 플래그를 명령으로 읽지 않도록 # 뒤를 제거한다.
    let firing: Vec<String> = bodies
        .iter()
        .flat_map(|b| run_commands(b))
        .map(|c| {
            c.lines()
                .map(|l| match l.find('#') {
                    Some(at) => &l[..at],
                    None => l,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        // --include-ignored는 --ignored를 부분문자열로 포함하지 않아 따로 확인한다.
        .filter(|c| c.contains("--ignored") || c.contains("--include-ignored"))
        .filter(|c| can_reach_gui_tests(c))
        .collect();

    let doc_text = std::fs::read_to_string(root.join(CLAIM_DOC))
        .unwrap_or_else(|e| panic!("{CLAIM_DOC} 를 읽지 못했다: {e}"));
    assert!(
        doc_text.contains("gui_tests"),
        "{CLAIM_DOC}에서 gui_tests 언급을 찾지 못했다. 문서 경로와 검사 범위를 확인한다."
    );
    let says_absent = unwrapped(&doc_text).contains(ABSENCE_CLAIM);

    if firing.is_empty() {
        assert!(
            says_absent,
            "자동 잡에 ignored 실행 플래그가 없지만 {CLAIM_DOC}에 '{ABSENCE_CLAIM}' 표지가 없다. 실제 실행 경로를 확인하고 설명을 복원한다. 표현만 바꿨다면 검사 상수도 함께 갱신한다."
        );
    } else {
        assert!(
            !says_absent,
            "자동 잡에 ignored 실행 플래그가 있다:\n  {}\n{CLAIM_DOC}의 '{ABSENCE_CLAIM}' 설명과 대조해 실제 실행 타깃·트리거·러너를 확인하고 문서를 고친다.",
            firing.join("\n  ")
        );
    }
}

/// 루트 패키지의 `gui_tests` 타깃을 실행할 수 있는 호출인지 본다. 다른 패키지만 지목하거나
/// 다른 통합 타깃만 지목한 호출은 ignored 플래그가 있어도 이 스위트를 돌리지 않는다.
fn can_reach_gui_tests(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let value_of = |flag: &str| -> Vec<&str> {
        words
            .windows(2)
            .filter(|w| w[0] == flag)
            .map(|w| w[1])
            .collect()
    };
    let packages = value_of("-p");
    if !words.contains(&"--workspace") && !packages.is_empty() && !packages.contains(&"tasty") {
        return false;
    }
    let targets = value_of("--test");
    targets.is_empty() || targets.contains(&"gui_tests")
}

#[test]
fn only_calls_that_can_build_gui_tests_count_for_its_channel() {
    assert!(!can_reach_gui_tests(
        "cargo test -p tasty-plugin-markdown --locked -- --ignored --exact x"
    ));
    assert!(!can_reach_gui_tests(
        "cargo test --workspace --locked --test e2e_tests -- --ignored"
    ));
    assert!(can_reach_gui_tests(
        "cargo test --workspace --locked --test gui_tests -- --ignored"
    ));
    assert!(can_reach_gui_tests(
        "cargo test --workspace --locked -- --ignored"
    ));
    assert!(can_reach_gui_tests(
        "cargo test -p tasty --locked -- --include-ignored"
    ));
}

/// 이름 열거 검사를 생략할 때 근거가 되는 제한 없는 자동 호출이 있는지 별도로 확인한다.
#[test]
fn the_self_silencing_axis_names_what_silenced_it() {
    let root = repo_root();
    if integration_tests_run_automatically(&root).is_some() {
        return;
    }

    let invocations = automatic_test_invocations(&root);
    assert!(
        !invocations.is_empty(),
        "자동 잡에서 cargo test 호출을 읽지 못했다. 워크플로 판독을 확인한다."
    );
    let unnarrowed: Vec<&String> = invocations
        .iter()
        .filter(|(_, tail)| !tail_is_narrowed(tail))
        .map(|(_, tail)| tail)
        .collect();
    assert!(
        !unnarrowed.is_empty(),
        "integration_tests_run_automatically가 None을 반환했지만 근거가 되는 제한 없는 자동 호출이 없다."
    );

    // 같은 범위 제한 판정만 반복하지 않고 --workspace도 요구한다. 패키지 선택 없는 cargo test는 루트 패키지만 실행한다.
    let not_whole: Vec<String> = unnarrowed
        .iter()
        .filter(|tail| !tail.split_whitespace().any(|w| w == "--workspace"))
        .map(|tail| format!("cargo test{tail}"))
        .collect();
    assert!(
        not_whole.is_empty(),
        "검사 생략의 근거가 된 호출에 --workspace가 없다. 범위 제한 판독과 루트 패키지 호출의 해석을 확인한다:\n  {}",
        not_whole.join("\n  ")
    );
}

/// 실행을 말하는 항목만 검사한다. 검사 대상·한계 설명에는 채널명을 반복하지 않는다.
/// 문단·목록 항목·표 행의 경계는 다른 CI 검사와 같은 `claim_scope`를 사용한다.
/// 실행 표현의 문자열 목록만 인식하므로 모든 자연어 주장을 판독하는 것은 아니다.
fn theme_channel_claims(text: &str, target: &str, channels: &[String]) -> (usize, Vec<usize>) {
    let mut claims = 0;
    let mut violations = Vec::new();
    for at in word_offsets(text, target) {
        let scope = claim_scope(text, at);
        if !["자동", "CI", "실행", "돈다", "돌린다", "--lib --bins"]
            .iter()
            .any(|marker| scope.contains(marker))
        {
            continue;
        }
        claims += 1;
        let named: Vec<&str> = scope
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || "-_.".contains(ch)))
            .filter(|word| word.ends_with(".yml") || word.ends_with(".yaml"))
            .collect();
        if named.is_empty() || named.iter().any(|name| !channels.iter().any(|c| c == name)) {
            violations.push(at);
        }
    }
    (claims, violations)
}

/// 워크플로 이름별로 자동 호출을 읽고 기존 타깃·패키지·조합 판정에 넘긴다.
/// 문서에 적힌 이름이나 특정 워크플로 파일명을 정답 목록으로 복사하지 않는다.
fn theme_target_workflows(root: &Path, target: &str) -> Vec<String> {
    let features = test_target_features(root);
    let mut channels = Vec::new();
    for workflow in workflow_files(&root.join(".github/workflows"), &WORKFLOW_FLOOR) {
        let text = std::fs::read_to_string(&workflow.path).expect("워크플로를 읽지 못했다");
        let head: String = text
            .lines()
            .take_while(|line| !line.starts_with("jobs:"))
            .collect();
        if !["push:", "pull_request:", "schedule:"]
            .iter()
            .any(|trigger| head.contains(trigger))
        {
            continue;
        }
        let invocations: Vec<_> = automatic_job_bodies(&text)
            .iter()
            .flat_map(|body| cargo_test_tails(body))
            .map(|tail| {
                let combo = if tail.contains("--no-default-features") {
                    Combo::Headless
                } else {
                    Combo::Default
                };
                (combo, tail)
            })
            .collect();
        if !integration_target_channels(root, target, &invocations, &features).is_empty() {
            channels.push(
                workflow
                    .path
                    .file_name()
                    .expect("워크플로 파일명")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    channels
}

#[test]
fn the_theme_table_keeps_the_two_channels_apart() {
    let root = repo_root();
    let path = root.join("docs/design/systems/theme.md");
    let text = std::fs::read_to_string(&path).expect("theme.md 를 읽지 못했다");
    let target = "design_token_adherence";
    assert!(
        !word_offsets(&text, target).is_empty(),
        "theme 가드 인용을 하나도 읽지 못했다"
    );
    let channels = theme_target_workflows(&root, target);
    let (claims, violations) = theme_channel_claims(&text, target, &channels);
    assert!(
        claims > 0,
        "theme의 실제 실행 설명을 하나도 검사하지 못했다"
    );
    assert!(
        violations.is_empty(),
        "{} — 실행을 주장한 항목에 해당 타깃을 돌리는 워크플로를 적어야 한다. 실제 채널: {channels:?}, 위반 행: {:?}{SCOPE_NOTE}",
        path.display(),
        violations
            .iter()
            .map(|at| line_of(&text, *at))
            .collect::<Vec<_>>()
    );
}

#[test]
fn theme_scope_descriptions_do_not_need_repeated_channels() {
    let text =
        "- `design_token_adherence.rs`는 색 리터럴을 검사한다. 변수 의미는 추적하지 않는다.\n";
    assert_eq!(
        theme_channel_claims(text, "design_token_adherence", &[]),
        (0, vec![])
    );
}

#[test]
fn theme_run_claims_require_the_right_channel_even_beside_limits() {
    let channels = vec!["doc-guards.yml".to_string()];
    for claim in [
        "CI가 항상 실행한다",
        "자동으로 돈다",
        "헤드리스 조합에서만 실행한다",
        "`wrong.yml`이 자동으로 돌린다",
    ] {
        let text =
            format!("`design_token_adherence.rs`는 변수 의미를 추적하지 못하지만 {claim}.\n");
        let (checked, violations) =
            theme_channel_claims(&text, "design_token_adherence", &channels);
        assert_eq!(checked, 1, "{claim}");
        assert_eq!(violations.len(), 1, "{claim}");
    }
    let text = "`design_token_adherence.rs`는 `doc-guards.yml`이 실행한다.\n";
    assert_eq!(
        theme_channel_claims(text, "design_token_adherence", &channels),
        (1, vec![])
    );
    assert_eq!(
        theme_channel_claims(text, "design_token_adherence", &[])
            .1
            .len(),
        1
    );
}

#[test]
fn theme_claims_do_not_borrow_channels_from_other_paragraphs_or_rows() {
    let channels = vec!["doc-guards.yml".to_string()];
    for text in [
        "`doc-guards.yml`이 자동으로 돌린다.\n\n`design_token_adherence.rs`는 CI가 실행한다.\n",
        "- `doc-guards.yml`이 자동으로 돌린다.\n- `design_token_adherence.rs`는 CI가 실행한다.\n",
        "| 다른 검사 | `doc-guards.yml`이 자동으로 돌린다 |\n| `design_token_adherence.rs` | CI가 실행한다 |\n",
    ] {
        assert_eq!(
            theme_channel_claims(text, "design_token_adherence", &channels)
                .1
                .len(),
            1
        );
    }
    let text = "`doc-guards.yml`이 자동으로 돌린다.\n\n`design_token_adherence.rs`는 원시 색을 검사한다.\n";
    assert_eq!(
        theme_channel_claims(text, "design_token_adherence", &channels),
        (0, vec![])
    );
}

#[test]
fn theme_former_lib_row_is_a_fixture_not_a_required_doc_row() {
    let lib = "ui_font_size_tokens_are_integers_at_every_zoom";
    let text = format!(
        "| `design_token_adherence.rs` | `doc-guards.yml`이 자동으로 돌린다 |\n| `{lib}` | `crossplatform-check.yml`이 자동으로 돌린다 |\n"
    );
    let integration_channels = vec!["doc-guards.yml".to_string()];
    let lib_channels = vec!["crossplatform-check.yml".to_string()];
    assert_eq!(
        theme_channel_claims(&text, "design_token_adherence", &integration_channels),
        (1, vec![])
    );
    assert_eq!(theme_channel_claims(&text, lib, &lib_channels), (1, vec![]));
    assert_eq!(
        theme_channel_claims(&text, lib, &integration_channels)
            .1
            .len(),
        1
    );
}

// 합성 주장 표지는 조각으로 조립한다. 주석에 실제 표지를 그대로 쓰면 이 파일의 검사 입력이 된다.
fn enforce() -> String {
    format!("CI 가 {}", "강제한다")
}

/// 자동 잡이 이름으로 지목하는 통합 테스트가 하나도 없는 상태.
fn no_named_tests() -> std::collections::BTreeSet<String> {
    std::collections::BTreeSet::new()
}

fn named(names: &[&str]) -> std::collections::BTreeSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn a_target_with_no_automatic_combo_is_caught_as_an_enforcer() {
    let text = format!("`tests/gui_tests.rs` 를 {}.\n", enforce());
    let (found, cited) = enforcement_violations(&text, "docs/x.md", &named(&["e2e_tests"]));
    assert_eq!(cited, 1, "지목을 못 읽었다");
    assert_eq!(
        found.len(),
        1,
        "자동 조합이 0 인 타깃을 집행 장치로 부른 서술을 안 잡았다"
    );
}

#[test]
fn a_target_with_a_combo_is_not_caught() {
    let text = format!("`tests/e2e_tests.rs` 를 {}.\n", enforce());
    let (found, cited) = enforcement_violations(&text, "docs/x.md", &named(&["e2e_tests"]));
    assert_eq!(cited, 1, "지목을 못 읽었다");
    assert!(
        found.is_empty(),
        "자동으로 도는 타깃을 두고 참인 집행 서술을 고발했다 — {found:?}"
    );
}

/// 이 판정은 Markdown만 읽으므로 Rust 합성 입력에서는 표지를 그대로 쓸 수 있다.
const NO_SINGLE_VALUE: &str = "단일 값이 없다";

#[test]
fn a_pass_count_in_an_unrelated_section_is_not_a_gui_claim() {
    let text = concat!(
        "## 어느 바이너리를 띄우는가\n\n스위트 단위 10 / 11 통과.\n\n",
        "## 시나리오 하나에 테스트 하나\n\n`gui_tests` 는 전수 무시다.\n"
    );
    assert!(
        gui_pass_counts_missing_marker(text, NO_SINGLE_VALUE).is_empty(),
        "무관한 두 절이 서로를 위반으로 만들었다"
    );
}

#[test]
fn a_pass_count_beside_gui_tests_without_the_marker_is_caught() {
    let text = "# 머리\n\n## 남은 칸\n\n`gui_tests` 는 11 통과다.\n";
    assert_eq!(
        gui_pass_counts_missing_marker(text, NO_SINGLE_VALUE),
        vec![3],
        "같은 절에서 수만 적은 자리를 못 잡았다"
    );
}

#[test]
fn a_marker_in_a_child_section_qualifies_the_parent() {
    let text = format!(
        "## 남은 칸\n\n`gui_tests` 는 11 통과다.\n\n### 왜 그 수가 흔들리나\n\n이 칸에는 {NO_SINGLE_VALUE}.\n"
    );
    assert!(
        gui_pass_counts_missing_marker(&text, NO_SINGLE_VALUE).is_empty(),
        "하위 절의 단정이 부모 절을 못 덮었다"
    );
}

#[test]
fn a_marker_in_a_sibling_section_does_not_exempt() {
    let text = format!(
        "## 남은 칸\n\n`gui_tests` 는 11 통과다.\n\n## 다른 칸\n\n이 칸에는 {NO_SINGLE_VALUE}.\n"
    );
    assert_eq!(
        gui_pass_counts_missing_marker(&text, NO_SINGLE_VALUE),
        vec![1],
        "옆 절의 단정이 면제로 작동했다"
    );
}

#[test]
fn a_heading_inside_a_fence_does_not_split_a_section() {
    let text = format!(
        "## 남은 칸\n\n```sh\n# gui_tests 를 이렇게 돈다\n```\n\n`gui_tests` 는 11 통과이고 이 칸에는 {NO_SINGLE_VALUE}.\n"
    );
    assert!(
        gui_pass_counts_missing_marker(&text, NO_SINGLE_VALUE).is_empty(),
        "코드 펜스 안의 `#` 주석을 heading 으로 읽어 절을 갈랐다"
    );
}

#[test]
fn an_absence_marker_in_a_neighbouring_table_row_does_not_exempt() {
    let text = format!(
        "| 포맷 | `tests/a_guard.rs` 가 강제한다 — 자동 채널이 없다 |\n| 린트 | `tests/b_guard.rs` 를 {} |\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert_eq!(found.len(), 1, "옆 행의 부재 표지가 면제로 작동했다");
    assert_eq!(line_of(&text, found[0]), 2);
}

#[test]
fn an_absence_marker_in_a_neighbouring_list_item_does_not_exempt() {
    let text = format!(
        "- `tests/a_guard.rs` 가 강제한다 — 자동 채널이 없다.\n- `tests/b_guard.rs` 를 {}.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert_eq!(found.len(), 1, "옆 항목의 부재 표지가 면제로 작동했다");
    assert_eq!(line_of(&text, found[0]), 2);
}

#[test]
fn a_compile_marker_in_another_paragraph_does_not_exempt() {
    let text = format!(
        "자동 잡의 clippy 는 통합 테스트를 컴파일한다.\n\n`tests/b_guard.rs` 를 {}.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert_eq!(found.len(), 1, "앞 문단의 컴파일 언급이 면제로 작동했다");
}

#[test]
fn a_compile_claim_in_the_same_sentence_is_true_and_exempt() {
    let text = format!(
        "`tests/b_guard.rs` 를 {} — 다만 그것은 컴파일 검사다.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert!(found.is_empty(), "참인 컴파일 주장을 위반으로 짚었다");
}

#[test]
fn a_wrapped_sentence_keeps_its_absence_marker_in_scope() {
    let text = format!(
        "`tests/b_guard.rs` 를 {} — 다만 그 잡은 수동\n전용이라 실행 채널이 없다.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert!(found.is_empty(), "접힌 문장이 반토막 나 오탐이 났다");
}

#[test]
fn a_doc_comment_block_is_one_scope_but_a_blank_comment_line_splits_it() {
    let joined = format!(
        "//! `tests/b_guard.rs` 를 {} — 그 잡은 수동\n//! 전용이라 실행 채널이 없다.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&joined, "src/some_module.rs", &no_named_tests());
    assert!(found.is_empty(), "이어진 주석 블록이 갈라졌다");

    let split = format!(
        "//! 그 잡은 수동 전용이라 실행 채널이 없다.\n//!\n//! `tests/b_guard.rs` 를 {}.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&split, "src/some_module.rs", &no_named_tests());
    assert_eq!(
        found.len(),
        1,
        "빈 주석 줄 너머의 부재 표지가 면제로 작동했다"
    );
}

#[test]
fn the_named_enumeration_exempts_only_the_names_the_workflow_lists() {
    let text = format!("`tests/api_baseline_0_7.rs` 를 {}.\n", enforce());
    let (found, _) = enforcement_violations(&text, "docs/x.md", &named(&["api_baseline_0_7"]));
    assert!(found.is_empty(), "열거된 이름을 위반으로 짚었다");

    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert_eq!(found.len(), 1, "열거가 사라졌는데 판정이 따라오지 않았다");
}

#[test]
fn a_code_literal_is_not_a_claim() {
    let text = format!(
        "    let needle = \"`tests/b_guard.rs` 를 {}\";\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&text, "src/some_module.rs", &no_named_tests());
    assert!(found.is_empty(), "코드 리터럴을 서술로 읽었다");
}

#[test]
fn a_scope_that_asserts_both_automatic_and_absent_is_not_exempt() {
    let contradiction = "| 테스트 | `cargo test --workspace --locked` | 이 조합은 CI 가 자동으로 돌린다 — `test.yml` 의 전체 스위트는 `workflow_dispatch` 전용이다 |\n";
    assert_eq!(
        claim_offsets(contradiction, "docs/x.md").len(),
        1,
        "한 서술 안의 모순을 준수로 읽었다"
    );

    let precise = "| 테스트 | `cargo test --workspace --locked` | 이 조합 그대로는 자동 채널 없음 — `test.yml` 의 전체 스위트는 `workflow_dispatch` 전용이다 |\n";
    assert!(
        claim_offsets(precise, "docs/x.md").is_empty(),
        "정확히 쓴 행을 위반으로 짚었다"
    );
}

#[test]
fn a_reference_to_an_automatic_job_is_not_an_assertion_that_it_runs() {
    let referring = "| lint | `cargo test --workspace --locked` | 자동 채널 없음 — `crossplatform-check` 의 Windows 잡이 `--lib --bins` 를 배선했다. `test.yml` 참조 |\n";
    assert!(
        claim_offsets(referring, "docs/x.md").is_empty(),
        "참조를 자동 실행 주장으로 읽었다"
    );
}

#[test]
fn the_contradiction_rule_applies_to_the_enforcement_axis_too() {
    let contradiction = format!(
        "`tests/b_guard.rs` 를 {} — 자동으로 돌린다. 그래도 그 잡은 수동 전용이다.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&contradiction, "docs/x.md", &no_named_tests());
    assert_eq!(found.len(), 1, "집행 축에서 모순이 면제로 작동했다");

    let precise = format!(
        "`tests/b_guard.rs` 를 {} — 다만 그 잡은 수동 전용이다.\n",
        enforce()
    );
    let (found, _) = enforcement_violations(&precise, "docs/x.md", &no_named_tests());
    assert!(found.is_empty(), "정확히 쓴 서술을 위반으로 짚었다");
}

#[test]
fn the_automatic_channel_marker_exempts_only_inside_the_same_row() {
    let libs = named(&["some_lib_test"]);

    let same_row = "| lib | 있다 — `--lib --bins` 가 돈다 | `some_lib_test` 자동 채널이 없다 |\n";
    assert!(
        weak_absence_offsets(same_row, "docs/x.md", &libs).is_empty(),
        "자동 채널을 함께 적은 행을 약한 서술로 짚었다"
    );

    let other_row =
        "| lib | 있다 — `--lib --bins` 가 돈다 |\n| 통합 | `some_lib_test` 는 자동 채널이 없다 |\n";
    assert_eq!(
        weak_absence_offsets(other_row, "docs/x.md", &libs).len(),
        1,
        "옆 행의 자동 채널 표지가 면제로 작동했다"
    );
}

// 의도적으로 판정하지 않는 입력도 고정해, 범위를 바꿀 때 기존 한계가 드러나게 한다.

#[test]
fn narrowing_is_seen_however_many_flags_precede_it() {
    assert!(is_narrowed(" --workspace --lib --bins --locked\n"));
    assert!(is_narrowed(
        " --locked --no-fail-fast --frozen --offline --lib --bins\n"
    ));
    assert!(is_narrowed(
        " --locked --no-default-features --test api_baseline_0_7\n"
    ));
    assert!(!is_narrowed(" --locked --no-fail-fast\n"));
    assert!(!is_narrowed(
        " --locked\n      - name: other\n        run: cargo test --lib\n"
    ));
}

#[test]
fn narrowing_is_seen_on_a_continuation_line() {
    assert!(is_narrowed(
        " --workspace --locked \\\n      --lib --bins\n"
    ));
    assert!(is_narrowed(
        " --workspace \\\n      --locked \\\n      --no-fail-fast \\\n      --test api_baseline_0_7\n"
    ));
    assert!(!is_narrowed(
        " --workspace \\\n      --locked\n      --lib --bins\n"
    ));
}

#[test]
fn the_full_suite_judgement_uses_the_same_narrowing_rule() {
    assert!(!a_job_body_runs_the_full_suite(
        "        run: cargo test --workspace --locked --no-fail-fast --frozen --offline --lib --bins\n"
    ));
    assert!(!a_job_body_runs_the_full_suite(
        "        run: cargo test --workspace --locked --no-fail-fast --frozen --test api_baseline_0_7\n"
    ));
    assert!(!a_job_body_runs_the_full_suite(
        "        run: |\n          cargo test --workspace --locked \\\n            --lib --bins\n"
    ));
    assert!(a_job_body_runs_the_full_suite(
        "        run: cargo test --workspace --no-default-features --locked --no-fail-fast\n"
    ));
    // 이 함수는 타깃 제한만 판정하므로 --skip은 별도로 처리한다.
    assert!(a_job_body_runs_the_full_suite(
        "        run: |\n          cargo test --workspace --locked --no-fail-fast -- \\\n            --skip all_e2e_tests\n"
    ));
    assert!(!a_job_body_runs_the_full_suite(
        "        run: cargo clippy --workspace --all-targets\n"
    ));
}

fn workflow_dir(name: &str, files: &[(&str, &str)]) -> Scratch {
    let probe = Scratch::new(&format!("ci-guard-{name}"));
    let dir = probe.path();
    std::fs::create_dir_all(dir).expect("합성 워크플로 디렉토리를 만들지 못했다");
    for (file, body) in files {
        std::fs::write(dir.join(file), body).expect("합성 워크플로를 쓰지 못했다");
    }
    probe
}

/// 실제 저장소에 의존하지 않도록 타깃 파일과 워크플로를 함께 만든다.
fn fake_repo(name: &str, targets: &[(&str, &str)], workflows: &[(&str, &str)]) -> Scratch {
    let probe = Scratch::new(&format!("ci-repo-{name}"));
    let dir = probe.path();
    std::fs::create_dir_all(dir.join("tests")).expect("합성 tests/ 를 만들지 못했다");
    std::fs::create_dir_all(dir.join(".github/workflows")).expect("합성 워크플로를 만들지 못했다");
    for (file, body) in targets {
        std::fs::write(dir.join("tests").join(file), body).expect("합성 타깃을 쓰지 못했다");
    }
    for (file, body) in workflows {
        std::fs::write(dir.join(".github/workflows").join(file), body)
            .expect("합성 워크플로를 쓰지 못했다");
    }
    probe
}

#[test]
fn a_module_doc_speaks_for_its_own_target() {
    let headless = |_: &str| combos(&[Combo::Headless]);
    let none = |_: &str| combos(&[]);

    let denying = "//! 이 테스트가 그 집행 채널이다 — 그 잡은 수동 전용이다.\n";
    assert_eq!(
        overstated_absence(denying, "tests/alpha.rs", &headless, &combos(&[])).len(),
        1,
        "자기 채널을 부정하는 모듈 doc 을 못 잡았다"
    );

    let other = "//! 그 빌드 잡은 수동 전용이라 이 실패를 자동으로 잡지 못한다.\n";
    assert!(
        overstated_absence(other, "tests/alpha.rs", &headless, &combos(&[])).is_empty(),
        "주어가 남인 서술을 이 타깃의 부재 주장으로 읽었다"
    );

    let quoted = "//! 이 가드는 \"그 잡은 수동 전용이다\" 같은 서술을 잡는다.\n";
    assert!(
        overstated_absence(quoted, "tests/alpha.rs", &headless, &combos(&[])).is_empty(),
        "인용을 주장으로 읽었다"
    );

    assert!(
        overstated_absence(denying, "tests/alpha.rs", &none, &combos(&[])).is_empty(),
        "채널이 없는 타깃의 참인 부재 주장을 고발했다"
    );
}

#[test]
fn an_example_command_inside_an_explanation_is_not_a_claim() {
    let explaining = concat!(
        "const EXPECTED: &[(&str, usize)] = &[(\"crossplatform-check.yml\", 2), (\"test.yml\", 3)];\n",
        "\n",
        "/// 평탄화하는 이유: 줄 끝 `\\` 이음이 한 명령을 여러 줄에 나눈다.\n",
        "/// 줄 단위로 보면 `cargo test --workspace` 에서 끊겨 뒤 플래그를 놓친다.\n",
    );
    assert!(
        claim_offsets(explaining, "src/x.rs").is_empty(),
        "설명문 안의 인용을 주장으로 고발했다 — 표지가 그 서술 밖(위 상수)에 있는데도 걸렸다"
    );

    let claiming = "/// 이 가드는 `cargo test --workspace` 로 CI 에서 자동으로 강제된다.\n";
    assert_eq!(
        claim_offsets(claiming, "src/x.rs").len(),
        1,
        "표지가 같은 서술 안에 있는 진짜 주장을 놓쳤다 — 축이 잠잠해졌다"
    );

    let claiming_in_scope =
        "/// `test.yml` 이 `cargo test --workspace` 로 이 가드를 자동 강제한다.\n";
    assert_eq!(claim_offsets(claiming_in_scope, "src/x.rs").len(), 1);
}

/// 패키지 선택이 다른 패키지에 영향을 주지 않는지 비교하려면 두 패키지가 필요하다.
fn fake_two_package_repo(name: &str) -> Scratch {
    let probe = fake_repo(name, &[("root_side.rs", &one_test("a"))], &[]);
    let dir = probe.path();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"rootpkg\"\n")
        .expect("루트 매니페스트를 쓰지 못했다");
    let sub = dir.join("crates/subpkg");
    std::fs::create_dir_all(sub.join("tests")).expect("합성 크레이트를 만들지 못했다");
    std::fs::write(sub.join("Cargo.toml"), "[package]\nname = \"subpkg\"\n")
        .expect("크레이트 매니페스트를 쓰지 못했다");
    std::fs::write(sub.join("tests/sub_side.rs"), one_test("b"))
        .expect("크레이트 타깃을 쓰지 못했다");
    probe
}

#[test]
fn a_package_narrowed_job_does_not_reach_another_package() {
    let dir_probe = fake_two_package_repo("pkgnarrow");
    let dir = dir_probe.path();
    let invocations = vec![(
        Combo::Default,
        " -p subpkg --locked --no-fail-fast\n".to_string(),
    )];
    let features = std::collections::BTreeMap::new();

    assert_eq!(
        integration_target_channels(dir, "sub_side", &invocations, &features),
        combos(&[Combo::Default]),
        "지목된 패키지 자신의 타깃은 채널을 받아야 한다"
    );
    assert_eq!(
        integration_target_channels(dir, "root_side", &invocations, &features),
        combos(&[]),
        "`-p subpkg` 가 루트 패키지의 타깃에까지 채널을 줬다"
    );

    let wide = vec![(Combo::Default, " --workspace --locked\n".to_string())];
    assert_eq!(
        integration_target_channels(dir, "root_side", &wide, &features),
        combos(&[Combo::Default])
    );
}

#[test]
fn a_package_narrowed_job_is_not_a_lib_channel() {
    let dir_probe = fake_two_package_repo("pkglib");
    let dir = dir_probe.path();
    std::fs::write(
        dir.join(".github/workflows/only-sub.yml"),
        "on:\n  push:\n    branches: [main]\njobs:\n  j:\n    steps:\n      - run: cargo test -p subpkg --locked\n",
    )
    .expect("합성 워크플로를 쓰지 못했다");
    assert!(!lib_tests_run_automatically(dir));

    std::fs::write(
        dir.join(".github/workflows/only-sub.yml"),
        "on:\n  push:\n    branches: [main]\njobs:\n  j:\n    steps:\n      - run: cargo test -p rootpkg --locked\n",
    )
    .expect("합성 워크플로를 쓰지 못했다");
    assert!(
        lib_tests_run_automatically(dir),
        "본체 패키지를 지목한 잡은 lib 채널이 맞다 — 이 대조가 없으면 위 단언은 언제나 참이다"
    );
}

fn one_test(name: &str) -> String {
    format!("#[test]\nfn {name}() {{}}\n")
}

fn combos(list: &[Combo]) -> std::collections::BTreeSet<Combo> {
    list.iter().copied().collect()
}

#[test]
fn a_step_name_is_not_a_command() {
    let body = "  j:\n    steps:\n      - name: cargo test (unit)\n        shell: pwsh\n        run: cargo test --workspace --lib --bins --locked\n";
    let tails = cargo_test_tails(body);
    assert_eq!(tails.len(), 1, "스텝 이름을 명령으로 읽었다: {tails:?}");
    assert!(is_narrowed(&tails[0]), "좁힘을 못 봤다: {tails:?}");
}

#[test]
fn a_folded_scalar_is_one_command() {
    let body = "  j:\n    steps:\n      - name: t\n        run: >\n          cargo test --locked --no-default-features\n          --test alpha\n          --test beta\n";
    let tails = cargo_test_tails(body);
    assert_eq!(tails.len(), 1, "접힌 명령을 쪼갰다: {tails:?}");
    assert!(
        is_narrowed(&tails[0]),
        "접힌 줄에 놓인 --test 를 못 봤다: {tails:?}"
    );
    // 블록 지시자는 cargo 앞에 있으므로 인자만 보지 않고 명령 전체도 확인한다.
    let commands = run_commands(body);
    assert_eq!(commands.len(), 1, "접힌 블록을 쪼갰다: {commands:?}");
    assert!(
        commands[0].trim_start().starts_with("cargo"),
        "블록 지시자를 명령 내용으로 읽었다: {commands:?}"
    );
}

#[test]
fn a_literal_scalar_keeps_the_shell_continuation_rule() {
    let body = "  j:\n    steps:\n      - name: t\n        run: |\n          cargo test --workspace --locked -- \\\n            --skip only_this\n          cargo test --lib\n";
    let tails = cargo_test_tails(body);
    assert_eq!(
        tails.len(),
        2,
        "리터럴 블록의 두 명령을 하나로 붙였다: {tails:?}"
    );
    assert!(
        !is_narrowed(&tails[0]),
        "다음 줄의 --lib 를 첫 명령의 좁힘으로 읽었다: {tails:?}"
    );
    assert_eq!(skip_names(&tails[0]), vec!["only_this".to_string()]);
}

#[test]
fn a_required_feature_keeps_a_target_off_the_headless_channel() {
    let root_probe = fake_repo(
        "reqfeat",
        &[("guarded.rs", &one_test("t"))],
        &[("w.yml", AUTOMATIC_FULL)],
    );
    let root = root_probe.path();
    let invocations = vec![(
        Combo::Headless,
        " --workspace --no-default-features".to_string(),
    )];
    let mut features = std::collections::BTreeMap::new();
    features.insert(
        "guarded".to_string(),
        (vec!["gui".to_string()], vec!["gui".to_string()]),
    );
    assert!(
        integration_target_channels(root, "guarded", &invocations, &features).is_empty(),
        "gui 를 요구하는 타깃이 헤드리스 조합에서 돈다고 판정했다"
    );
    let default_only = vec![(Combo::Default, " --workspace".to_string())];
    assert_eq!(
        integration_target_channels(root, "guarded", &default_only, &features),
        combos(&[Combo::Default]),
        "기본 조합에서는 그 타깃이 만들어지는데 채널이 없다고 판정했다"
    );
}

#[test]
fn a_skip_that_covers_every_test_removes_the_targets_channel() {
    let root_probe = fake_repo(
        "skips",
        &[
            ("whole.rs", &one_test("all_of_it")),
            (
                "partial.rs",
                &format!("{}{}", one_test("all_of_it"), one_test("survivor")),
            ),
        ],
        &[("w.yml", AUTOMATIC_FULL)],
    );
    let root = root_probe.path();
    let invocations = vec![(
        Combo::Headless,
        " --workspace --no-default-features -- --skip all_of_it".to_string(),
    )];
    let features = std::collections::BTreeMap::new();
    assert!(
        integration_target_channels(root, "whole", &invocations, &features).is_empty(),
        "모든 테스트가 skip 된 타깃을 실행 채널로 셌다"
    );
    assert_eq!(
        integration_target_channels(root, "partial", &invocations, &features),
        combos(&[Combo::Headless]),
        "일부만 skip 된 타깃의 채널을 지웠다"
    );
}

#[test]
fn a_positive_filter_that_narrows_the_target_is_not_its_channel() {
    let root_probe = fake_repo(
        "filters",
        &[
            (
                "narrowed.rs",
                &format!("{}{}", one_test("chosen_one"), one_test("left_out")),
            ),
            ("whole.rs", &one_test("chosen_one")),
        ],
        &[("w.yml", AUTOMATIC_FULL)],
    );
    let root = root_probe.path();
    let invocations = vec![(
        Combo::Headless,
        " --workspace --no-default-features -- chosen_one --exact".to_string(),
    )];
    let features = std::collections::BTreeMap::new();
    assert!(
        integration_target_channels(root, "narrowed", &invocations, &features).is_empty(),
        "한 건만 고른 호출을 타깃 전체의 실행 채널로 셌다"
    );
    assert_eq!(
        integration_target_channels(root, "whole", &invocations, &features),
        combos(&[Combo::Headless]),
        "필터가 그 타깃의 모든 테스트를 덮는데 채널을 지웠다"
    );
    let loose = vec![(
        Combo::Headless,
        " --workspace --no-default-features -- chosen".to_string(),
    )];
    assert!(
        integration_target_channels(root, "narrowed", &loose, &features).is_empty(),
        "부분일치 필터도 좁히는 것은 마찬가지다"
    );
}

#[test]
fn a_named_target_gets_the_channel_only_for_itself() {
    let root_probe = fake_repo(
        "named",
        &[("alpha.rs", &one_test("a")), ("beta.rs", &one_test("b"))],
        &[("w.yml", AUTOMATIC_FULL)],
    );
    let root = root_probe.path();
    let invocations = vec![(
        Combo::Headless,
        " --locked --no-default-features --test alpha".to_string(),
    )];
    let features = std::collections::BTreeMap::new();
    assert_eq!(
        integration_target_channels(root, "alpha", &invocations, &features),
        combos(&[Combo::Headless])
    );
    assert!(
        integration_target_channels(root, "beta", &invocations, &features).is_empty(),
        "지목되지 않은 타깃에 채널을 줬다"
    );
    assert!(
        integration_target_channels(root, "nonexistent", &invocations, &features).is_empty(),
        "존재하지 않는 타깃에 채널을 줬다"
    );
}

#[test]
fn the_lib_axis_reads_the_workflow_instead_of_assuming() {
    let with_lib_probe = fake_repo(
        "libyes",
        &[],
        &[(
            "w.yml",
            "on:\n  push:\n    branches: [main]\njobs:\n  a:\n    steps:\n      - name: t\n        run: cargo test --workspace --lib --bins --locked\n",
        )],
    );
    let with_lib = with_lib_probe.path();
    assert!(lib_tests_run_automatically(with_lib));
    let named_only_probe = fake_repo(
        "libno",
        &[],
        &[(
            "w.yml",
            "on:\n  push:\n    branches: [main]\njobs:\n  a:\n    steps:\n      - name: t\n        run: cargo test --locked --test alpha\n",
        )],
    );
    let named_only = named_only_probe.path();
    assert!(
        !lib_tests_run_automatically(named_only),
        "`--test` 로만 좁힌 자동 잡을 lib 채널로 읽었다"
    );
}

#[test]
fn an_absence_claim_about_a_running_integration_test_needs_the_combination() {
    let bare = "`tests/alpha.rs` 가 강제한다 — 자동 채널이 없다.\n";
    let violations = overstated_absence(
        bare,
        "docs/x.md",
        &|_| combos(&[Combo::Headless]),
        &combos(&[]),
    );
    assert_eq!(violations.len(), 1, "도는 테스트의 부재 주장을 놓쳤다");

    let qualified = "`tests/alpha.rs` 가 강제한다 — 기본 조합에는 자동 채널이 없다(자동 실행은 헤드리스에서만).\n";
    assert!(
        overstated_absence(
            qualified,
            "docs/x.md",
            &|_| combos(&[Combo::Headless]),
            &combos(&[])
        )
        .is_empty(),
        "조합을 한정해 정확히 쓴 문장을 위반으로 짚었다"
    );
}

#[test]
fn a_test_with_no_channel_keeps_its_absence_claim() {
    let text = "`tests/alpha.rs` 는 자동 채널이 없다.\n";
    assert!(
        overstated_absence(text, "docs/x.md", &|_| combos(&[]), &combos(&[])).is_empty(),
        "채널이 0 인데 위반으로 짚었다"
    );
}

#[test]
fn two_channels_cannot_be_qualified_away() {
    let text = "`tests/alpha.rs` 는 기본 조합에는 자동 채널이 없다(헤드리스에서만 돈다).\n";
    let violations = overstated_absence(
        text,
        "docs/x.md",
        &|_| combos(&[Combo::Default, Combo::Headless]),
        &combos(&[]),
    );
    assert_eq!(
        violations.len(),
        1,
        "두 조합에서 도는데 한정 표지 하나로 면제됐다"
    );
}

#[test]
fn an_exclusive_job_claim_is_an_absence_claim() {
    let only = format!("{}일어난다", "에서만 ");

    let overstated = format!("`tests/alpha.rs` 가 강제한다 — 자동 실행은 그 잡{only}.\n");
    let violations = overstated_absence(
        &overstated,
        "docs/x.md",
        &|_| combos(&[Combo::Default, Combo::Headless]),
        &combos(&[]),
    );
    assert_eq!(
        violations.len(),
        1,
        "두 조합에서 도는데 배타 주장이 통과했다: {violations:?}"
    );

    let exact = format!("`tests/alpha.rs` 가 강제한다 — 자동 실행은 `check-headless` 잡{only}.\n");
    assert!(
        overstated_absence(
            &exact,
            "docs/x.md",
            &|_| combos(&[Combo::Headless]),
            &combos(&[])
        )
        .is_empty(),
        "조합을 함께 적은 배타 주장을 위반으로 짚었다"
    );

    assert_eq!(
        overstated_absence(
            &exact,
            "docs/x.md",
            &|_| combos(&[Combo::Default, Combo::Headless]),
            &combos(&[])
        )
        .len(),
        1,
        "조합이 둘로 늘었는데 옛 한정 표지가 계속 면제했다"
    );
}

#[test]
fn a_statement_that_asserts_no_absence_is_not_judged() {
    let text = "`tests/alpha.rs` 의 채널은 여기서 단정하지 않는다 — 정본은 ci-gates 다.\n";
    assert!(
        overstated_absence(
            text,
            "docs/x.md",
            &|_| combos(&[Combo::Headless]),
            &combos(&[])
        )
        .is_empty(),
        "부재를 주장하지 않은 문장을 짚었다"
    );
}

#[test]
fn a_source_file_is_judged_because_it_cannot_define_an_integration_target() {
    let text = "// `tests/alpha.rs` 는 자동 채널이 없다.\n";
    assert_eq!(
        overstated_absence(
            text,
            "src/x.rs",
            &|_| combos(&[Combo::Headless]),
            &combos(&[])
        )
        .len(),
        1,
        "src/ 를 통째로 면제해 모듈 doc 의 채널 주장을 놓쳤다"
    );
}

#[test]
fn a_class_citation_is_judged_against_the_whole_class() {
    let text = "`tests/*.rs` 는 자동 채널이 없다.\n";
    assert_eq!(
        overstated_absence(
            text,
            "docs/x.md",
            &|_| combos(&[]),
            &combos(&[Combo::Headless])
        )
        .len(),
        1,
        "부류를 지목한 부재 주장이 판정에서 빠졌다"
    );

    assert!(
        overstated_absence(text, "docs/x.md", &|_| combos(&[]), &combos(&[])).is_empty(),
        "채널이 없는데 부재 주장을 고발했다"
    );
}

const AUTOMATIC_FULL: &str = "on:\n  push:\n    branches: [main]\njobs:\n  a:\n    steps:\n      - name: t\n        run: cargo test --workspace --locked\n";

#[test]
fn a_workflow_is_read_under_either_extension() {
    for (name, file) in [("yml", "ci.yml"), ("yaml", "ci.yaml")] {
        let dir_probe = workflow_dir(name, &[(file, AUTOMATIC_FULL)]);
        let dir = dir_probe.path();
        let bodies = automatic_job_bodies_of_dir(dir, &CALLER_SUPPLIED_FLOOR);
        assert_eq!(bodies.len(), 1, "{file} 을 워크플로로 읽지 않았다");
        assert!(bodies[0].contains("cargo test --workspace"));
    }

    let dir_probe = workflow_dir("other", &[("notes.md", AUTOMATIC_FULL)]);
    let dir = dir_probe.path();
    assert!(
        automatic_job_bodies_of_dir(dir, &CALLER_SUPPLIED_FLOOR).is_empty(),
        "워크플로가 아닌 파일을 잡 본문으로 읽었다"
    );
}

#[test]
fn widening_the_extension_does_not_widen_the_trigger_rule() {
    let manual = "on:\n  workflow_dispatch:\njobs:\n  a:\n    steps:\n      - name: t\n        run: cargo test --workspace --locked\n";
    for (name, file) in [("m-yml", "manual.yml"), ("m-yaml", "manual.yaml")] {
        let dir_probe = workflow_dir(name, &[(file, manual)]);
        let dir = dir_probe.path();
        assert!(
            automatic_job_bodies_of_dir(dir, &CALLER_SUPPLIED_FLOOR).is_empty(),
            "{file}: 수동 전용 워크플로가 자동 잡으로 들어왔다"
        );
    }
}

#[test]
fn a_claim_that_names_no_test_is_deliberately_not_judged() {
    let text = format!("이 규칙은 {}.\n", enforce());
    let (found, _) = enforcement_violations(&text, "docs/x.md", &no_named_tests());
    assert!(
        found.is_empty(),
        "대상이 특정되지 않은 서술은 판정하지 않는다 — 짚을 좌표가 없다"
    );
}

#[test]
fn only_the_file_that_defines_the_name_is_exempt() {
    let libs = named(&["some_lib_test"]);

    let defines = "//! `some_lib_test` 는 자동 채널이 없다.\n#[test]\nfn some_lib_test() {}\n";
    assert!(
        weak_absence_offsets(defines, "crates/x/src/lib.rs", &libs).is_empty(),
        "정의 파일의 이름 등장을 서술로 읽었다"
    );

    let refers = "//! `some_lib_test` 는 자동 채널이 없다.\n";
    assert_eq!(
        weak_absence_offsets(refers, "crates/x/src/lib.rs", &libs).len(),
        1,
        "남의 이름을 부른 모듈 doc 이 경로 면제로 빠졌다"
    );
}

/// 기준 목록에서 입력을 생성하면 항목 삭제를 놓치므로 합성 표지는 독립된 리터럴로 둔다.
#[test]
fn each_affirmative_run_marker_is_pinned_by_a_literal() {
    for fragment in ["자동으로 돈다", "자동으로 돌린다", "✅ 자동"] {
        assert!(
            AFFIRMATIVE_RUN_MARKERS.contains(&fragment),
            "AFFIRMATIVE_RUN_MARKERS에서 {fragment:?}가 빠졌다. 이 항목을 지우면 부재 표지의 면제 범위가 넓어진다. 표지를 폐기하려면 이를 사용하는 설명을 먼저 확인하고 근거를 남긴다."
        );
    }
}

/// 제한 없는 자동 호출의 --skip 증가로 이름 열거 검사가 놓치는 범위가 넓어지지 않도록 상한을 둔다.
const MAX_SKIPPED_IN_UNNARROWED: usize = 1;

#[test]
fn the_enforcement_arm_is_dormant_only_while_an_unnarrowed_automatic_job_exists() {
    let root = repo_root();
    let dormant = integration_tests_run_automatically(&root).is_none();

    let unnarrowed: Vec<(Combo, String)> = automatic_test_invocations(&root)
        .into_iter()
        .filter(|(_, tail)| !tail_is_narrowed(tail))
        .collect();

    if !dormant {
        assert!(
            unnarrowed.is_empty(),
            "이름 열거 검사를 실행하지만 제한 없는 자동 호출도 있다. 두 판정을 대조한다."
        );
        return;
    }

    assert!(
        !unnarrowed.is_empty(),
        "이름 열거 검사를 생략했지만 제한 없는 자동 호출이 없다. integration_tests_run_automatically가 None을 반환한 이유를 확인한다."
    );

    let mut skipped: Vec<String> = Vec::new();
    for (_, tail) in &unnarrowed {
        let words: Vec<&str> = tail.split_whitespace().collect();
        for pair in words.windows(2) {
            if pair[0] == "--skip" {
                skipped.push(pair[1].to_string());
            }
        }
    }
    skipped.sort();
    skipped.dedup();

    assert!(
        skipped.len() <= MAX_SKIPPED_IN_UNNARROWED,
        "제한 없는 자동 호출이 --skip으로 제외하는 이름이 {}개다(상한 {MAX_SKIPPED_IN_UNNARROWED}): {skipped:?}\n상한을 높이기 전에 새 제외가 조합의 제약인지 불안정한 테스트 때문인지 확인한다. 다른 자동 조합에서 실행된다면 ci-gates.md에 해당 경로를 적는다.",
        skipped.len()
    );
}

/// 워크플로 원문에서 scripts/check-*.sh 이름을 수집한다. 주석·step 이름과 실제 명령을 구별하지 않는다.
fn gate_scripts_of_workflow(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    // check-로 시작하는 셸 스크립트만 게이트로 본다. build- 스크립트는 대상이 아니다.
    while let Some(at) = rest.find("scripts/check-") {
        let tail = &rest[at + "scripts/".len()..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
            .unwrap_or(tail.len());
        let name = &tail[..end];
        if name.ends_with(".sh") && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
        rest = &rest[at + "scripts/".len()..];
    }
    out.sort();
    out
}

/// 합성 디렉터리도 검사할 수 있도록 파일 수 하한을 호출자가 넘긴다.
fn gate_scripts_by_workflow(workflows: &Path, floor: &Floor) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for w in workflow_files(workflows, floor) {
        let text = std::fs::read_to_string(&w.path).unwrap_or_default();
        let scripts = gate_scripts_of_workflow(&text);
        if scripts.is_empty() {
            continue;
        }
        out.push((w.rel.clone(), scripts));
    }
    out.sort();
    out
}

#[test]
fn the_gate_script_index_dies_when_extraction_dies() {
    const CALLS_GATES: &str = "on:\n  push:\n    branches: [main]\njobs:\n  a:\n    steps:\n      - run: bash scripts/check-alpha.sh\n      - run: bash scripts/check-beta.sh\n";
    let dir_probe = workflow_dir("gate-index-live", &[("alpha.yml", CALLS_GATES)]);
    let dir = dir_probe.path();
    let index = gate_scripts_by_workflow(dir, &CALLER_SUPPLIED_FLOOR);
    assert_eq!(
        index,
        vec![(
            "alpha.yml".to_string(),
            vec!["check-alpha.sh".to_string(), "check-beta.sh".to_string()]
        )],
        "합성 워크플로에서 게이트 두 개를 추출하지 못했다"
    );

    const CALLS_NO_GATE: &str = "on:\n  push:\n    branches: [main]\njobs:\n  a:\n    steps:\n      - run: bash scripts/build-alpha.sh\n      - run: bash tools/check-beta.sh\n";
    let dir_probe = workflow_dir("gate-index-dead", &[("beta.yml", CALLS_NO_GATE)]);
    let dir = dir_probe.path();
    assert!(
        gate_scripts_by_workflow(dir, &CALLER_SUPPLIED_FLOOR).is_empty(),
        "게이트 스크립트가 아닌 명령을 게이트로 분류했다"
    );
}

/// 파일의 자기 이름은 게이트 목록에서 제외한다.
fn own_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

#[derive(Debug)]
struct GateListAudit {
    mentions: usize,
    named_any: usize,
    none_named: usize,
    judged: usize,
    violations: Vec<String>,
    misattributed: Vec<String>,
}

/// 실제 문서와 합성 트리가 같은 순회·문장 분류·목록 대조를 사용한다.
fn gate_list_audit(root: &Path, floor: &Floor) -> GateListAudit {
    let index = gate_scripts_by_workflow(&root.join(".github/workflows"), floor);
    assert!(
        !index.is_empty(),
        "게이트 스크립트를 호출하는 워크플로를 찾지 못했다. 문서 판정 전에 워크플로 수집·명령 추출을 확인한다."
    );

    let mut files = Vec::new();
    collect_files(root, &mut files);
    files.sort();

    let mut all_scripts: Vec<String> = index.iter().flat_map(|(_, s)| s.clone()).collect();
    all_scripts.sort();
    all_scripts.dedup();

    let mut violations = Vec::new();
    let mut misattributed: Vec<String> = Vec::new();
    let mut judged = 0usize;
    let mut mentions = 0usize; // 워크플로 이름을 든 산문 자리 (전수)
    let mut named_any = 0usize; // 그중 그 워크플로의 게이트를 하나라도 든 자리
    // 각 분류를 따로 세고 합계를 대조한다. 뺄셈으로 만들면 집계 누락을 찾지 못한다.
    let mut none_named = 0usize; // 그중 그 워크플로의 게이트를 하나도 안 든 자리
    for path in &files {
        // Windows 구분자가 경로 비교에 영향을 주지 않도록 정규화한다.
        let rel = normalized_rel(path, root);
        // 워크플로 파일은 문서 주장이 아니라 실행 목록의 출처이므로 제외한다.
        if rel.starts_with(".github/workflows/") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (workflow, scripts) in &index {
            let mut from = 0usize;
            while let Some(at) = text[from..].find(workflow.as_str()) {
                let at = from + at;
                from = at + workflow.len();
                if !is_prose_line(&text, at, &rel) {
                    continue;
                }
                mentions += 1;
                let scope = claim_scope(&text, at);

                // 같은 문단의 참조만으로 스크립트가 어느 워크플로에 속하는지 단정할 수 없다.
                // 잘못된 연결은 필드가 대응하는 표 행에서만 판정한다. 누락 판정은 아래에서 별도로 한다.
                if scope.trim_start().starts_with('|') {
                    let claimed: Vec<&(String, Vec<String>)> = index
                        .iter()
                        .filter(|(w, _)| scope.contains(w.as_str()))
                        .collect();
                    for script in &all_scripts {
                        if !scope.contains(script.as_str()) || script.as_str() == own_name(&rel) {
                            continue;
                        }
                        if claimed.iter().any(|(_, ss)| ss.contains(script)) {
                            continue;
                        }
                        let line = line_of(&text, at);
                        let msg = with_time_mark(
                            format!(
                                "  {rel}:{line} — `{script}` 를 들었는데 이 자리가 든 워크플로 {:?} 중 어느 것도 그것을 안 부른다",
                                claimed.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>()
                            ),
                            scope,
                        );
                        if !misattributed.contains(&msg) {
                            misattributed.push(msg);
                        }
                    }
                }

                // 자기 이름을 소개한 스크립트에 형제 게이트 목록까지 요구하지 않는다.
                let named: Vec<&String> = scripts
                    .iter()
                    .filter(|s| s.as_str() != own_name(&rel) && scope.contains(s.as_str()))
                    .collect();
                if named.is_empty() {
                    none_named += 1;
                    continue; // 실행 목록이 아니므로 제외한다
                }
                named_any += 1;
                // 전체 목록 주장은 워크플로가 스크립트들보다 먼저 나오거나 스크립트를 둘 이상 열거한 경우로 한정한다.
                // 게이트 하나의 실행 경로를 설명하는 표 행에 형제 목록을 요구하면 중복 설명이 생긴다.
                let wf_at = scope.find(workflow.as_str()).unwrap_or(0);
                let workflow_is_subject = named
                    .iter()
                    .filter_map(|s| scope.find(s.as_str()))
                    .all(|s| wf_at < s);
                if named.len() < 2 && !workflow_is_subject {
                    continue;
                }
                judged += 1;
                let missing: Vec<&str> = scripts
                    .iter()
                    .filter(|s| !scope.contains(s.as_str()))
                    .map(|s| s.as_str())
                    .collect();
                if !missing.is_empty() {
                    violations.push(with_time_mark(
                        format!(
                            "  {rel}:{} — `{workflow}` 을 들면서 {:?} 는 적고 {:?} 가 빠졌다",
                            line_of(&text, at),
                            named.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                            missing
                        ),
                        scope,
                    ));
                }
            }
        }
    }

    GateListAudit {
        mentions,
        named_any,
        none_named,
        judged,
        violations,
        misattributed,
    }
}

#[test]
fn no_file_lists_only_some_of_the_gates_a_workflow_runs() {
    let GateListAudit {
        mentions,
        named_any,
        none_named,
        judged,
        violations,
        misattributed,
    } = gate_list_audit(&repo_root(), &WORKFLOW_FLOOR);
    println!(
        "FUNNEL 워크플로 이름을 든 자리 {mentions} → 게이트를 하나라도 든 자리 \
         {named_any} → 자칭 두 모양까지 통과해 판정된 자리 {judged} (스크립트를 하나도 \
         안 든 자리 {none_named})"
    );
    assert_eq!(
        mentions,
        named_any + none_named,
        "워크플로 언급의 분류별 합계가 맞지 않는다. 게이트를 포함한 경우와 포함하지 않은 경우의 집계를 확인한다."
    );
    // 전체 목록 주장은 없어도 된다. 워크플로·게이트 언급 수집과 합성 목록 판정은 별도로 확인한다.
    assert!(mentions > 0, "워크플로 이름을 든 문서를 하나도 찾지 못했다");
    assert!(
        named_any > 0,
        "워크플로와 게이트를 함께 든 문서를 하나도 찾지 못했다"
    );
    // 단독 게이트 설명이 전체 목록으로 분류되지 않는지도 실제 문서에서 확인한다.
    assert!(
        judged < named_any,
        "게이트를 언급한 {named_any}곳 모두가 전체 목록으로 판정됐다({judged}). 단독 게이트 설명을 제외하는 named.len() >= 2 || workflow_is_subject 조건을 확인한다. 문서의 모든 행에 형제 이름을 덧붙이지 않는다. 실제 문서 구조가 바뀌었다면 근거를 확인한 뒤 검사를 갱신한다."
    );
    assert!(
        violations.is_empty(),
        "워크플로의 전체 게이트 목록에서 누락이 있다. 판정 {judged}곳 중 {}곳:\n{}\n실제 워크플로를 기준으로 문서 목록을 고친다. 이 가드에 목록 사본을 추가하지 않는다.{TIME_NOTE}",
        violations.len(),
        violations.join("\n")
    );
    assert!(
        misattributed.is_empty(),
        "표에서 게이트를 호출하지 않는 워크플로에 연결했다. {}곳:\n{}\n실제 워크플로 파일을 확인하고 실행 경로를 바로잡는다.{TIME_NOTE}",
        misattributed.len(),
        misattributed.join("\n")
    );
}

#[test]
fn gate_list_audit_checks_complete_partial_single_and_empty_claims() {
    const ALPHA: &str = "jobs:\n  a:\n    steps:\n      - run: bash scripts/check-one.sh\n      - run: bash scripts/check-two.sh\n      - run: bash scripts/check-three.sh\n";
    const BETA: &str = "jobs:\n  b:\n    steps:\n      - run: bash scripts/check-other.sh\n";
    let cases = [
        (
            "complete",
            "alpha.yml runs check-one.sh, check-two.sh and check-three.sh.\n",
            1,
            1,
            0,
            1,
            0,
            0,
        ),
        (
            "missing",
            "alpha.yml runs check-one.sh.\n",
            1,
            1,
            0,
            1,
            1,
            0,
        ),
        (
            "two-before-workflow",
            "check-one.sh and check-two.sh run in alpha.yml.\n",
            1,
            1,
            0,
            1,
            1,
            0,
        ),
        (
            "single-gate",
            "| rule | check-one.sh | alpha.yml |\n",
            1,
            1,
            0,
            0,
            0,
            0,
        ),
        (
            "no-gates",
            "alpha.yml has a manual trigger.\n",
            1,
            0,
            1,
            0,
            0,
            0,
        ),
        ("empty", "No workflow claim.\n", 0, 0, 0, 0, 0, 0),
        (
            "wrong-channel",
            "| rule | check-other.sh | alpha.yml |\n",
            1,
            0,
            1,
            0,
            0,
            1,
        ),
    ];
    for (name, text, mentions, named, none, judged, missing, wrong) in cases {
        let probe = fake_repo(name, &[], &[("alpha.yml", ALPHA), ("beta.yml", BETA)]);
        std::fs::write(probe.path().join("README.md"), text).expect("합성 설명을 쓰지 못했다");
        let got = gate_list_audit(probe.path(), &CALLER_SUPPLIED_FLOOR);
        assert_eq!(
            (
                got.mentions,
                got.named_any,
                got.none_named,
                got.judged,
                got.violations.len(),
                got.misattributed.len()
            ),
            (mentions, named, none, judged, missing, wrong),
            "{name}: {got:?}"
        );
        if missing > 0 {
            assert!(
                got.violations[0].contains("check-three.sh"),
                "빠진 형제 이름이 없다"
            );
        }
        if wrong > 0 {
            assert!(
                got.misattributed[0].contains("check-other.sh"),
                "잘못 연결한 이름이 없다"
            );
        }
    }
}

/// 이 검사는 표지 명부의 판독을 확인한다. 표지 목록 자체의 완전성을 보장하지는 않는다.
#[test]
fn the_time_marker_reader_answers_both_yes_and_no() {
    let (first_marker, _) = TIME_MARKERS
        .first()
        .expect("시점 표지 명부가 비었다 — 이 대조가 설 자리가 없다");

    let pinned = format!("| 어떤 축 | 어떤 게이트 | {first_marker} 였다 |");
    assert_eq!(
        time_pin(&pinned).as_deref(),
        Some(*first_marker),
        "명부의 표지를 못 알아봤다"
    );

    assert_eq!(
        time_pin("배선했다 (2026-09-05) 그 값은 38 이었다").as_deref(),
        Some("2026-09-05"),
        "날짜 모양을 못 잡았다 — 낱말 없이 날짜만 단 실측이 이 레포에 흔하다"
    );

    assert_eq!(
        time_pin("| 파일 SLOC | check-file-size.sh | complexity-check.yml |"),
        None,
        "때를 안 못박은 자리를 시점 서술로 셌다 — 그러면 진짜 위반이 다른 처방을 달고 나간다"
    );

    assert_eq!(
        time_pin("버전 1234-56"),
        None,
        "날짜가 아닌 것을 날짜로 읽었다"
    );
    assert_eq!(iso_date_in("짧다"), None, "10 글자 미만에서 색인이 넘쳤다");

    let marked = with_time_mark("docs/x.md:12".to_string(), &pinned);
    assert!(
        marked.starts_with("docs/x.md:12") && marked.contains("[시점 표지"),
        "표지가 짚힌 줄에 안 붙었다: {marked}"
    );
    assert_eq!(
        with_time_mark("docs/x.md:12".to_string(), "그냥 지금 서술"),
        "docs/x.md:12",
        "표지가 없는 줄에 군더더기가 붙었다"
    );
}

#[test]
fn every_time_marker_carries_its_own_reason() {
    for (marker, why) in TIME_MARKERS {
        assert!(!marker.trim().is_empty(), "빈 표지가 명부에 있다");
        assert!(
            why.chars().count() >= 10,
            "표지 {marker:?} 의 사유가 너무 짧다 — 왜 이것이 시점 표지인지 적어라: {why:?}"
        );
    }
    let mut seen: Vec<&str> = TIME_MARKERS.iter().map(|(m, _)| *m).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(before, seen.len(), "같은 표지가 명부에 두 번 있다");
}

/// swallowable_verification_steps가 분류한 스텝 수(2026-09-08, b134d28e3).
/// 앞선 스텝이 있고 cargo test/clippy를 호출하며 if·continue-on-error가 없는 스텝이다.
/// YAML 주석을 제외한 사본에서 센다. 실제 실행 결과를 수집한 수는 아니다.
const SWALLOWABLE_STEPS: usize = 6;

/// if가 있는 검증 스텝 수(2026-10-07, check-headless의 webview 레이아웃 관측 단계를 더해 7).
/// 조건식을 계산하지 않아 앞선 실패 뒤 실행을 보장하지 않는다.
/// 두 분류를 함께 대조하지만 수만으로 스텝 삭제와 조건 변경을 구별할 수는 없다.
const PROTECTED_STEPS: usize = 7;

fn workflow_texts() -> Vec<(String, String)> {
    let root = repo_root();
    workflow_files(&root.join(".github/workflows"), &WORKFLOW_FLOOR)
        .into_iter()
        .map(|w| {
            let name = w
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(&w.path).unwrap_or_default();
            (name, text)
        })
        .collect()
}

/// 앞선 스텝이 있는 검증 스텝 중 if·continue-on-error가 없는 항목 수의 변화를 확인한다.
#[test]
fn the_verification_steps_a_dead_neighbour_can_swallow_are_pinned() {
    let texts = workflow_texts();
    assert!(
        texts.len() >= 8,
        "워크플로를 {}개만 읽었다. 스텝 수를 판정하기 전에 수집 범위를 확인한다.",
        texts.len()
    );

    let mut found: Vec<String> = Vec::new();
    for (name, text) in &texts {
        for s in tasty_doc_guards::workflow_triggers::swallowable_verification_steps(text) {
            found.push(format!(
                "{name}:{} {} #{} {}",
                s.line, s.job, s.ordinal, s.name
            ));
        }
    }
    found.sort();

    assert!(
        found.len() <= SWALLOWABLE_STEPS,
        "앞선 스텝이 있고 if·continue-on-error가 없는 검증 스텝이 {}개로 상한 {SWALLOWABLE_STEPS}을 넘었다.\n    {}\n앞선 실패 뒤에도 실행할 검증인지 판단하고 필요한 실행 조건을 설정한다. 줄어든 것은 실패가 아니다.",
        found.len(),
        found.join("\n    ")
    );
}

/// if가 있는 검증 스텝 수의 변화를 확인한다. 조건식의 의미는 사람이 검토한다.
#[test]
fn the_steps_that_were_lifted_out_of_the_failure_chain_stay_lifted() {
    let texts = workflow_texts();
    let mut protected: Vec<String> = Vec::new();
    for (name, text) in &texts {
        for s in tasty_doc_guards::workflow_triggers::protected_verification_steps(text) {
            protected.push(format!("{name}:{} {} {}", s.line, s.job, s.name));
        }
    }
    protected.sort();
    assert_eq!(
        protected.len(),
        PROTECTED_STEPS,
        "if가 있는 검증 스텝이 {}개다(기준 {PROTECTED_STEPS}).\n    {}\n스텝 추가·삭제와 조건식을 확인하고 SWALLOWABLE_STEPS와 함께 비교한다. if가 있다는 것만으로 앞선 실패 뒤 실행이 보장되지는 않는다.",
        protected.len(),
        protected.join("\n    ")
    );
}
