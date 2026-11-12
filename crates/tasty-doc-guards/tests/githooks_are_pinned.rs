//! 저장소의 Git 훅 파일·검사 ID·문서 표가 일치하는지 확인한다.
//! 개수만 같고 이름이 바뀐 경우도 찾도록 파일 목록을 대조한다.
//! 개발 환경의 core.hooksPath 설정이나 훅 설치 여부는 검사하지 않는다.

use std::collections::BTreeSet;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::repo_root;

/// 저장소가 제공할 훅 파일 목록(2026-09-20 확인).
const REQUIRED_HOOKS: &[&str] = &["pre-commit", "pre-merge-commit", "pre-push"];

/// 개별 누락은 이름 목록으로 찾고 순회 하한은 빈 수집을 구별한다.
const LIVENESS: Floor = Floor {
    min: 1,
    measured: 3,
    measured_on: "2026-09-20",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "742b0dbf7 — `git ls-files .githooks/` 가 셋이고 순회도 셋이다. \
         이 디렉토리는 평평하고(하위 디렉토리 0) 확장자가 없다.",
    ),
    why_this_gap: "개별 파일 누락은 이름 목록과 대조한다. 하한은 파일을 하나도 수집하지 못한 경우를 구별한다.",
};

fn hooks_on_disk() -> BTreeSet<String> {
    let dir = repo_root().join(".githooks");
    assert!(
        dir.is_dir(),
        ".githooks가 디렉터리가 아니다. 훅 파일 목록을 읽기 전에 경로를 확인한다."
    );

    walk_with_floor(&dir, &dir, &LIVENESS, Descend::Everything, &|_| true)
        .unwrap_or_else(|why| panic!("{why}"))
        .into_iter()
        .map(|w| w.rel)
        .collect()
}

#[test]
fn every_named_hook_is_still_in_the_tree() {
    let on_disk = hooks_on_disk();
    let missing: Vec<&str> = REQUIRED_HOOKS
        .iter()
        .copied()
        .filter(|name| !on_disk.contains(*name))
        .collect();

    assert!(
        missing.is_empty(),
        ".githooks에서 빠진 훅: {missing:?}. 의도적으로 삭제했다면 훅 명부와 docs/dev-guide/git-hooks.md를 함께 갱신한다."
    );
}

#[test]
fn the_directory_holds_nothing_the_roster_does_not_name() {
    let named: BTreeSet<&str> = REQUIRED_HOOKS.iter().copied().collect();
    let extra: Vec<String> = hooks_on_disk()
        .into_iter()
        .filter(|name| !named.contains(name.as_str()))
        .collect();

    assert!(
        extra.is_empty(),
        "미등록 훅 파일: {extra:?}. 훅 명부와 docs/dev-guide/git-hooks.md에 이름과 역할을 추가한다."
    );
}

#[test]
fn each_hook_is_a_bash_script_with_content() {
    for name in REQUIRED_HOOKS {
        let path = repo_root().join(".githooks").join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("훅 `{name}` 을 못 읽었다: {e}"));

        // 빈 파일도 존재하므로 최소 내용과 실행 셸을 확인한다.
        assert!(
            text.lines().count() > 5,
            "훅 {name}이 {}줄뿐이다. 내용이 누락됐는지 확인한다.",
            text.lines().count()
        );
        assert!(
            text.starts_with("#!/usr/bin/env bash"),
            "훅 `{name}` 의 첫 줄이 bash shebang 이 아니다. \
             git 은 이 파일을 직접 실행하므로 shebang 이 곧 실행 셸이다."
        );
    }
}

/// pre-merge-commit은 파일 전체가 하나의 검사라 내부 ID가 없다.
/// 충돌한 merge는 git commit으로 마무리하므로 M.1은 pre-commit에서 구현한다.
const HOOK_STEPS: &[(&str, &[&str])] = &[
    (
        "pre-commit",
        &[
            "A.1", "A.2", "A.3", "C.6", "C.8", "C.9", "C.11", "C.12", "M.1", "P.1", "T.1", "W.1",
            "W.2",
        ],
    ),
    ("pre-merge-commit", &[]),
    ("pre-push", &["B.4", "B.6", "B.7", "B.8", "B.9", "B.10"]),
];

/// 표의 첫 열에 검사 ID를 적는 운영 문서.
const HOOK_DOC: &str = "docs/dev-guide/git-hooks.md";

/// 산문의 ID 언급과 구분하도록 구분선 바로 아래의 구역 제목에서만 ID를 읽는다.
fn banner_ids(text: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut prev = "";
    for line in text.lines() {
        let is_rule = prev.starts_with("# ─") && prev.trim_end().ends_with('─');
        if is_rule && let Some(id) = banner_id_of(line) {
            ids.insert(id);
        }
        prev = line;
    }
    ids
}

fn banner_id_of(line: &str) -> Option<String> {
    let rest = line.strip_prefix("# ")?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '.')
        .collect();
    let (family, number) = id.split_once('.')?;
    let ok = family.len() == 1
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
        && rest[id.len()..].starts_with(|c: char| c.is_whitespace());
    ok.then_some(id)
}

#[test]
fn the_banners_in_each_hook_match_the_roster() {
    for (name, declared) in HOOK_STEPS {
        let path = repo_root().join(".githooks").join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("훅 `{name}` 을 못 읽었다: {e}"));

        let found = banner_ids(&text);
        let want: BTreeSet<String> = declared.iter().map(|s| s.to_string()).collect();

        assert_eq!(
            found,
            want,
            "훅 {name}의 검사 ID와 명부가 다르다.\n훅에만 있음: {:?}\n명부에만 있음: {:?}\n훅·명부·{HOOK_DOC}의 표를 함께 갱신한다.",
            found.difference(&want).collect::<Vec<_>>(),
            want.difference(&found).collect::<Vec<_>>(),
        );
    }
}

#[test]
fn the_hook_doc_tables_list_exactly_the_declared_steps() {
    let path = repo_root().join(HOOK_DOC);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("`{HOOK_DOC}` 을 못 읽었다: {e}"));

    // 문서 절 배치와 무관하게 표 첫 열의 ID 전체를 대조한다.
    let documented: BTreeSet<String> = text
        .lines()
        .filter_map(|line| {
            let cell = line.strip_prefix("| ")?.split('|').next()?.trim();
            banner_id_of(&format!("# {cell} "))
        })
        .collect();

    let declared: BTreeSet<String> = HOOK_STEPS
        .iter()
        .flat_map(|(_, ids)| ids.iter().map(|s| s.to_string()))
        .collect();

    assert_eq!(
        documented,
        declared,
        "{HOOK_DOC}의 표와 훅 검사 ID가 다르다.\n문서에만 있음: {:?}\n훅에만 있음: {:?}\n누락되거나 오래된 설명을 갱신한다.",
        documented.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&documented).collect::<Vec<_>>(),
    );
}

#[test]
fn the_pre_commit_run_list_and_its_definitions_still_agree() {
    let text = std::fs::read_to_string(repo_root().join(".githooks/pre-commit"))
        .unwrap_or_else(|e| panic!("pre-commit 을 못 읽었다: {e}"));

    let defined = text
        .lines()
        .filter(|l| l.starts_with("check_") && l.ends_with("() {"))
        .count();

    let listed = text
        .lines()
        .skip_while(|l| !l.starts_with("CHECKS=("))
        .skip(1)
        .take_while(|l| !l.starts_with(')'))
        .filter(|l| l.trim().starts_with("check_"))
        .count();

    // 로컬 훅 설치 여부와 무관하게 자동 검사에서도 정의·실행 목록 개수를 확인한다.
    assert!(
        defined > 0 && listed > 0,
        "pre-commit 검사 정의 {defined}개·실행 목록 {listed}개를 읽었다. 빈 결과라면 CHECKS 배열과 함수 선언 형태가 바뀌었는지 확인한다."
    );
    assert_eq!(
        defined, listed,
        "pre-commit의 검사 정의 {defined}개와 실행 목록 {listed}개가 다르다. 실행 목록 누락이나 오래된 항목을 확인한다."
    );

    // 구현한 검사 함수 수를 하한으로 사용해 훅 명부와 문서 표를 함께 비우는 경우를 막는다.
    let declared_here = HOOK_STEPS
        .iter()
        .find(|(name, _)| *name == "pre-commit")
        .map(|(_, ids)| ids.len())
        .unwrap_or(0);
    assert!(
        declared_here >= defined,
        "pre-commit 검사 함수는 {defined}개인데 ID 명부는 {declared_here}개뿐이다. 검사별 ID와 문서 표를 확인한다."
    );
}

// Run the real hook with fake Git/Cargo commands so failures do not build or push anything.
fn run_pre_push_fixture(
    refs: &str,
    version_rc: i32,
    cargo_rc: i32,
) -> (bool, String, String, String) {
    run_pre_push_fixture_with(
        refs,
        version_rc,
        cargo_rc,
        "test source_guards::fixture ... ok",
    )
}

const SOURCE_GUARD_TITLE: &str = "cargo test --lib --locked --no-fail-fast -- source_guards";

fn run_pre_push_fixture_with(
    refs: &str,
    version_rc: i32,
    cargo_rc: i32,
    source_guard_output: &str,
) -> (bool, String, String, String) {
    use std::io::Write;
    use std::process::Stdio;
    use tasty_doc_guards::temp_scratch::Scratch;

    let scratch = Scratch::new("pre-push-hook");
    // Include a space to verify that log and command paths remain quoted.
    let root = scratch.path().join("hook fixture");
    std::fs::create_dir(&root).unwrap();
    let env_file = root.join("commands.sh");
    std::fs::write(
        &env_file,
        r#"
git() {
    case "$*" in
        'rev-parse --show-toplevel') printf '%s\n' "$HOOK_FIXTURE" ;;
        'rev-parse --local-env-vars') command git rev-parse --local-env-vars ;;
        'rev-parse --git-path hook-logs') printf '%s/logs\n' "$HOOK_FIXTURE" ;;
        cat-file*) return 0 ;;
        *) return 99 ;;
    esac
}
bash() {
    printf '%s\n' "$*" >> "$HOOK_FIXTURE/calls"
    case "$1" in
        *check-plugin-version-bump.sh) return "$HOOK_VERSION_RC" ;;
        *) return 99 ;;
    esac
}
cargo() {
    printf 'cargo %s\n' "$*" >> "$HOOK_FIXTURE/calls"
    printf 'first diagnostic\n'
    printf '%s\n' "$HOOK_SOURCE_GUARD_OUTPUT"
    for ((i=0; i<70; i++)); do printf 'detail %s\n' "$i"; done
    return "$HOOK_CARGO_RC"
}
"#,
    )
    .unwrap();
    let mut child = hook_bash_command()
        .arg(hook_path(&repo_root().join(".githooks/pre-push")))
        .args(["origin", "unused"])
        .env("BASH_ENV", hook_path(&env_file))
        .env("HOOK_FIXTURE", hook_path(&root))
        .env("HOOK_VERSION_RC", version_rc.to_string())
        .env("HOOK_CARGO_RC", cargo_rc.to_string())
        .env("HOOK_SOURCE_GUARD_OUTPUT", source_guard_output)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Git hooks require Bash");
    // 훅이 입력을 읽기 전에 실패로 끝나면 쓰기가 파이프 끊김으로 끝난다. 그때는 아래 검사가
    // 훅의 stdout·stderr 로 실패 이유를 보여 준다.
    let written = child.stdin.take().unwrap().write_all(refs.as_bytes());
    if let Err(e) = written {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        root.join("logs").is_dir(),
        "hook did not create logs: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // 빈 refs 시나리오는 로그 파일을 남기지 않으므로 픽스처 루트를 순회하고 logs/ 아래만 모은다.
    let fixture_floor = Floor {
        min: 1,
        measured: 1,
        measured_on: "2026-09-28",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "합성 트리라 이 시험이 직접 쓴 commands.sh 하나가 항상 있다. \
                       빈 refs 에서는 그것만 남고, 검사를 돌린 시나리오는 calls 와 로그 파일이 더해진다.",
    };
    let fixture_files =
        walk_with_floor(&root, &root, &fixture_floor, Descend::Everything, &|_| true)
            .unwrap_or_else(|why| panic!("{why}"));
    let mut logs = String::new();
    for file in fixture_files.iter().filter(|w| w.rel.starts_with("logs/")) {
        logs.push_str(&std::fs::read_to_string(&file.path).unwrap());
    }
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        std::fs::read_to_string(root.join("calls")).unwrap_or_default(),
        logs,
    )
}

#[test]
fn pre_push_reports_failures_and_retains_complete_logs() {
    let refs = "refs/heads/main aaaaaaaa refs/heads/main bbbbbbbb\n";
    let (success, output, calls, logs) = run_pre_push_fixture(refs, 0, 0);
    assert!(success, "{output}");
    // Clippy covers the development targets once; release and headless remain separate.
    let expected_calls = [
        "scripts/check-plugin-version-bump.sh --range bbbbbbbb aaaaaaaa",
        "cargo clippy --workspace --all-targets -- -D clippy::correctness",
        "cargo check --workspace --release --locked",
        "cargo check --no-default-features",
        "cargo test -p tasty-doc-guards",
        "cargo test --lib --locked --no-fail-fast -- source_guards",
    ];
    assert_eq!(calls.lines().collect::<Vec<_>>(), expected_calls);
    for title in [
        "플러그인 버전",
        "cargo clippy --workspace --all-targets -- -D clippy::correctness",
        "cargo check --workspace --release --locked",
        "cargo check --no-default-features",
        "cargo test -p tasty-doc-guards",
        SOURCE_GUARD_TITLE,
    ] {
        assert!(output.contains(&format!("시작: {title}")), "{output}");
        assert!(logs.contains(&format!("통과: {title} (")), "{logs}");
    }
    assert!(!output.contains("[B."), "{output}");
    let (success, output, calls, logs) = run_pre_push_fixture(refs, 0, 101);
    assert!(!success, "{output}");
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        expected_calls,
        "later checks must still run"
    );
    assert!(output.contains("101"));
    assert!(
        !output.contains("first diagnostic"),
        "screen output is shortened"
    );
    assert!(
        logs.contains("first diagnostic"),
        "full diagnostics must remain in the logs"
    );
    assert!(logs.contains("실패: cargo test -p tasty-doc-guards"));
    // 시험 줄이 있어도 cargo의 실패가 이긴다.
    assert!(
        logs.contains(&format!("실패: {SOURCE_GUARD_TITLE}")),
        "{logs}"
    );
}

/// 이름 필터는 부분 일치라 passed 수만으로는 모듈이 사라진 경우를 구분할 수 없다.
#[test]
fn pre_push_fails_when_no_source_guard_test_passed() {
    let refs = "refs/heads/main aaaaaaaa refs/heads/main bbbbbbbb\n";
    for output_of_cargo in [
        "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 5024 filtered out",
        "running 1 test\ntest other::source_guards_like ... ok\ntest result: ok. 1 passed; 0 failed",
        "running 1 test\ntest source_guards::ignored ... ignored\ntest result: ok. 0 passed; 0 failed",
    ] {
        let (success, output, calls, logs) = run_pre_push_fixture_with(refs, 0, 0, output_of_cargo);
        assert!(!success, "{output_of_cargo}: {output}");
        assert!(calls.contains("-- source_guards"), "{calls}");
        assert!(
            logs.contains(&format!("실패: {SOURCE_GUARD_TITLE}")),
            "{logs}"
        );
        assert!(
            logs.contains("통과: cargo test -p tasty-doc-guards"),
            "{logs}"
        );
    }
}

#[test]
fn pre_push_stops_before_builds_when_commit_checks_fail() {
    let refs = "refs/heads/main aaaaaaaa refs/heads/main bbbbbbbb\n";
    for version in [1, 2] {
        let (success, output, calls, _) = run_pre_push_fixture(refs, version, 0);
        assert!(!success, "{output}");
        assert_eq!(calls.lines().count(), 1, "{calls}");
        assert!(!calls.contains("cargo"), "{calls}");
    }
}

#[test]
fn pre_push_handles_new_deleted_multiple_and_empty_refs() {
    let null = "0000000000000000000000000000000000000000";
    let refs = format!(
        "refs/heads/new aaaaaaaa refs/heads/new {null}\n(delete) {null} refs/heads/old bbbbbbbb\nrefs/heads/main cccccccc refs/heads/main dddddddd\n"
    );
    let (success, output, calls, _) = run_pre_push_fixture(&refs, 0, 0);
    assert!(success, "{output}");
    assert_eq!(calls.matches("--range").count(), 1);
    assert!(calls.contains("--range dddddddd cccccccc"));
    assert!(!calls.contains("bbbbbbbb"));
    let (success, output, calls, _) = run_pre_push_fixture("", 1, 101);
    assert!(success, "{output}");
    assert!(calls.is_empty(), "{calls}");
}

#[test]
fn pre_push_checks_do_not_inherit_the_pushing_repository_environment() {
    let scratch = tasty_doc_guards::temp_scratch::Scratch::new("pre-push-git-env");
    let output = hook_bash_command()
        .arg(hook_path(
            &repo_root().join("scripts/tests/pre-push-git-environment.sh"),
        ))
        .arg(hook_path(&repo_root().join(".githooks/pre-push")))
        .arg(hook_path(scratch.path()))
        .output()
        .expect("run disposable hook repository fixture");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 역병합·승격 merge 커밋만 허용하는 판정을 일회용 저장소의 실제 `git merge` 로 확인한다.
/// 허용 범위: docs/dev-guide/git-hooks.md#merge-커밋-판정
#[test]
fn merge_commits_are_allowed_only_for_back_merges_and_promotions() {
    let scratch = tasty_doc_guards::temp_scratch::Scratch::new("merge-policy");
    let output = hook_bash_command()
        .arg(hook_path(
            &repo_root().join("scripts/tests/merge-policy.sh"),
        ))
        .arg(hook_path(&repo_root()))
        .arg(hook_path(scratch.path()))
        .output()
        .expect("run disposable merge repository fixture");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn hook_bash() -> std::path::PathBuf {
    use std::process::Command;
    // On Windows, the system bash.exe may be a WSL launcher without a distribution.
    // Use the Bash installed alongside Git's hook shell instead.
    if cfg!(windows) {
        let shell = Command::new("git")
            .args(["var", "GIT_SHELL_PATH"])
            .output()
            .unwrap();
        assert!(shell.status.success());
        std::path::Path::new(String::from_utf8_lossy(&shell.stdout).trim())
            .parent()
            .unwrap()
            .join("bash.exe")
    } else {
        std::path::PathBuf::from("bash")
    }
}

/// hook 을 실행할 Bash. Windows 에서 Git 의 `usr\bin\bash.exe` 를 비대화형으로 띄우면 PATH 에 자기
/// 디렉터리가 없어 `mkdir` 같은 coreutils 를 찾지 못한다. Git 이 hook 을 실행할 때처럼 그
/// 디렉터리를 PATH 앞에 둔다.
fn hook_bash_command() -> std::process::Command {
    let bash = hook_bash();
    #[cfg(windows)]
    {
        let dir = bash.parent().expect("bash has a directory").to_path_buf();
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(&rest)))
            .expect("PATH entries join");
        let mut command = std::process::Command::new(bash);
        command.env("PATH", path);
        command
    }
    #[cfg(not(windows))]
    std::process::Command::new(bash)
}

fn hook_path(path: &std::path::Path) -> String {
    tasty_doc_guards::floored_walk::normalized_rel(path, std::path::Path::new(""))
}
