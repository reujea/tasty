//! 공식 플러그인 잠금 파일(`official-plugins.lock.toml`)의 가드.
//!
//! 1. 완비: 형식을 읽을 수 있고, 모든 플러그인(`bundle = false` 포함)에 5플랫폼 산출물이 있으며
//!    sha256·파일 이름·URL 형태가 맞다.
//! 2. 계약 호환: 잠금 파일이 가리키는 릴리스가 빌드에 쓴 계약 값이 현재 tasty와 같다.
//!    (a) host_api_version, (b) protocol 크레이트 버전, (c) manifest_version,
//!    (d) Cargo.lock의 epaint 버전, (e) `HEAD:vendor/epaint` 트리 id.
//! 3. 집합: 호스트 `BUILTINS`의 id 집합(두 cfg 판 모두)과 `OFFICIAL_PLUGIN_IDS`가 잠금 파일 전체 id에
//!    tasty에 남는 mesh-demo를 더한 집합과 같다.
//! 4. 배포 집합: 배포 패키지 스크립트(`scripts/lib/bundled-plugins.sh`, `scripts/build-windows.ps1`)는
//!    `fetch-official-plugins.sh`를 `--all` 없이 불러 잠금 파일의 `bundle = true` 항목만 받고, 개발 번들
//!    (`scripts/stage-dev-plugins.sh`의 기본 경로)은 `--all`로 모두 받는다. 잠금 항목의 `bundle`과 매니페스트의
//!    `bundle`이 같은지는 `official_plugin_fixtures.rs`가 픽스처 매니페스트로 본다.
//!
//! 한계: 계약 대조는 버전 값만 본다. 버전을 올리지 않은 protocol schema 변경은 잡지 못한다.
//! tasty 릴리스 사이의 develop에서는 protocol 변경이 `CHANGELOG.md`의 `[Unreleased]`에만 쌓이고
//! 버전은 그대로이므로 이 가드는 그 변경을 원리적으로 보지 못한다. 실제로 잠금 릴리스의 tasty_rev
//! 뒤 편집 결과 회신(Hello `capabilities`, `EditOutcome`)이 버전 0.2.16 그대로 들어왔다. 추가 변형이라
//! 옛 플러그인과 호환되지만, 호환이 아닌 변경도 같은 방식으로 지나갈 수 있다. 트리 id 비교(엄격안)를
//! 쓰지 않은 이유와 대신 지키는 규칙은 docs/dev-guide/plugin-packaging.md의 "계약 호환 가드" 절이다.

use std::collections::BTreeSet;

use tasty_doc_guards::official_plugins::{
    LOCK_FILE, Lock, REFRESH_HINT, check, contract_mismatches, parse_lock, read_host_contract,
};
use tasty_doc_guards::repo_root;

/// 공식 플러그인 레포로 옮기지 않고 tasty에 남는 시범 플러그인.
const STAYS_IN_TASTY: &str = "com.tasty.mesh-demo";

fn lock() -> Lock {
    let path = repo_root().join(LOCK_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", path.display()));
    parse_lock(&text)
        .unwrap_or_else(|errors| panic!("{LOCK_FILE}를 읽지 못했다:\n  {}", errors.join("\n  ")))
}

fn locked_ids(lock: &Lock) -> BTreeSet<String> {
    lock.plugins.iter().map(|p| p.id.clone()).collect()
}

/// `const <name>` 정의마다 그 블록(다음 `];`까지)의 `"com.tasty.…"` 문자열 집합을 돌려준다.
/// 줄 주석은 지운다.
fn id_sets_in_const(src: &str, name: &str) -> Vec<BTreeSet<String>> {
    let needle = format!("const {name}:");
    let mut sets = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(&needle) {
        let block = &rest[at..];
        let end = block
            .find("\n];")
            .unwrap_or_else(|| panic!("{name} 블록의 끝을 못 찾았다"));
        let mut ids = BTreeSet::new();
        for line in block[..end].lines() {
            let code = line.split("//").next().unwrap_or("");
            let mut parts = code.split('"');
            parts.next();
            while let Some(lit) = parts.next() {
                if lit.starts_with("com.tasty.") {
                    ids.insert(lit.to_string());
                }
                parts.next();
            }
        }
        sets.push(ids);
        rest = &block[end..];
    }
    sets
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel}를 읽지 못했다: {e}"))
}

/// 스크립트 원문에서 `fetch-official-plugins.sh`를 실행하는 줄. 주석 줄과 Windows 스크립트의 안내 문자열
/// (`Write-Error`)은 실행이 아니라 뺀다.
fn fetch_calls(rel: &str) -> Vec<String> {
    fetch_calls_in(&read(rel))
}

/// 호출 줄의 인자 토큰. 줄 끝 주석(공백 뒤 `#`부터)은 인자가 아니다.
fn call_args(call: &str) -> Vec<&str> {
    // 공백·탭 뒤의 첫 `#`부터가 주석이다. 둘 중 먼저 나오는 것을 쓴다.
    let comment = call
        .char_indices()
        .zip(call.chars().skip(1))
        .find(|((_, c), next)| c.is_whitespace() && *next == '#')
        .map(|((i, _), _)| i);
    let code = comment.map_or(call, |i| &call[..i]);
    code.split_whitespace().collect()
}

/// 개발 스테이징이 쓰는 실제 호출 꼴: `bash "$root/scripts/fetch-official-plugins.sh" ...`.
fn is_staging_fetch_call(args: &[&str]) -> bool {
    matches!(args, ["bash", script, ..] if script.trim_matches('"').ends_with("/scripts/fetch-official-plugins.sh"))
}

/// fetch 스크립트 뒤의 독립 옵션. `scripts/fetch-official-plugins.sh`의 파서처럼 값을 받는 옵션은
/// 다음 토큰을 값으로 소비하므로 `--cache --all`의 `--all`은 옵션이 아니다.
fn fetch_flags<'a>(args: &[&'a str]) -> Vec<&'a str> {
    const TAKES_VALUE: [&str; 4] = ["--platform", "--out", "--lock", "--cache"];
    let Some(script) = args
        .iter()
        .position(|a| a.trim_matches('"').ends_with("fetch-official-plugins.sh"))
    else {
        return Vec::new();
    };
    let mut flags = Vec::new();
    let mut rest = args[script + 1..].iter();
    while let Some(a) = rest.next() {
        if TAKES_VALUE.contains(a) {
            rest.next();
        } else {
            flags.push(*a);
        }
    }
    flags
}

/// `scripts/stage-dev-plugins.sh`의 기본 분기(`--from-checkout`이 아닐 때) 본문. 맨 앞 열의
/// `if [[ "$from_checkout" -eq 1 ]]; then` 뒤 맨 앞 열 `else`부터 맨 앞 열 `fi`까지다.
fn default_staging_branch(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "if [[ \"$from_checkout\" -eq 1 ]]; then")?;
    let open = start + lines[start..].iter().position(|l| *l == "else")?;
    let close = open + lines[open..].iter().position(|l| *l == "fi")?;
    Some(lines[open + 1..close].join("\n"))
}

fn fetch_calls_in(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.contains("fetch-official-plugins.sh"))
        .filter(|l| !l.starts_with('#') && !l.starts_with("Write-Error"))
        .map(String::from)
        .collect()
}

#[test]
fn the_lock_file_is_complete() {
    let lock = lock();
    let problems = check(&lock);
    assert!(
        problems.is_empty(),
        "{LOCK_FILE}가 완비되지 않았다. 손으로 고치지 말고 official-plugins generate로 다시 만든다:\n  {}",
        problems.join("\n  ")
    );
    assert!(!lock.plugins.is_empty(), "{LOCK_FILE}에 플러그인이 없다");
}

#[test]
fn the_lock_matches_the_host_contract() {
    let lock = lock();
    let host = read_host_contract(&repo_root()).unwrap_or_else(|e| panic!("{e}"));
    let mismatches = contract_mismatches(&lock.release, &host);
    assert!(
        mismatches.is_empty(),
        "잠금 파일의 릴리스 {} {}(tasty_rev {})는 현재 tasty와 계약이 다르다:\n  {}\n{REFRESH_HINT}",
        lock.release.repository,
        lock.release.tag,
        lock.release.tasty_rev,
        mismatches.join("\n  ")
    );
}

#[test]
fn builtins_are_the_locked_plugins_plus_mesh_demo() {
    let lock = lock();
    let mut want = locked_ids(&lock);
    want.insert(STAYS_IN_TASTY.to_string());

    let builtin = read("crates/tasty-host-plugin/src/builtin.rs");
    let sets = id_sets_in_const(&builtin, "BUILTINS");
    assert_eq!(
        sets.len(),
        2,
        "BUILTINS 정의가 두 cfg 판(windows, not windows)이 아니다. 판독 전제를 확인한다."
    );
    for (i, set) in sets.iter().enumerate() {
        assert_eq!(
            set,
            &want,
            "BUILTINS {}번째 정의의 id 집합이 잠금 파일 id + {STAYS_IN_TASTY}와 다르다",
            i + 1
        );
    }

    let official = read("crates/tasty-host-plugin/src/official.rs");
    let sets = id_sets_in_const(&official, "OFFICIAL_PLUGIN_IDS");
    assert_eq!(sets.len(), 1, "OFFICIAL_PLUGIN_IDS 정의가 하나가 아니다");
    assert_eq!(
        sets[0], want,
        "OFFICIAL_PLUGIN_IDS가 잠금 파일 id + {STAYS_IN_TASTY}와 다르다"
    );
}

/// 배포 패키지는 잠금 파일의 `bundle = true` 항목만 싣고 개발 번들은 모두 싣는다. 받을 집합은
/// `fetch-official-plugins.sh`가 잠금 파일에서 고르므로 호출 옵션 `--all` 유무가 배포 집합을 정한다.
#[test]
fn the_package_scripts_fetch_only_bundled_entries_and_dev_staging_fetches_all() {
    for rel in [
        "scripts/lib/bundled-plugins.sh",
        "scripts/build-windows.ps1",
    ] {
        let calls = fetch_calls(rel);
        assert!(!calls.is_empty(), "{rel}에서 fetch 호출을 찾지 못했다");
        for call in &calls {
            assert!(
                !call_args(call).contains(&"--all"),
                "{rel}의 배포 fetch가 --all 로 bundle = false 항목까지 싣는다: {call}"
            );
        }
    }
    let branch = default_staging_branch(&read("scripts/stage-dev-plugins.sh")).expect(
        "scripts/stage-dev-plugins.sh 에서 --from-checkout 이 아닌 기본 분기를 찾지 못했다",
    );
    let staging = fetch_calls_in(&branch);
    assert_eq!(
        staging.len(),
        1,
        "기본 분기의 fetch 호출이 하나가 아니다: {staging:?}"
    );
    let args = call_args(&staging[0]);
    assert!(
        is_staging_fetch_call(&args),
        "scripts/stage-dev-plugins.sh 기본 분기의 fetch 줄이 bash 로 스크립트를 부르는 호출이 아니다: {}",
        staging[0]
    );
    assert!(
        fetch_flags(&args).contains(&"--all"),
        "scripts/stage-dev-plugins.sh 기본 분기가 --all 로 받지 않는다: {}",
        staging[0]
    );
}

/// 주석에만 남은 `--all`은 인자가 아니고, 기본 분기 밖의 호출은 기본 분기로 세지 않는다.
#[test]
fn the_staging_check_reads_only_real_arguments_of_the_default_branch() {
    assert!(!call_args("bash fetch-official-plugins.sh --out x # --all").contains(&"--all"));
    assert!(call_args("bash fetch-official-plugins.sh --out x --all # 모두").contains(&"--all"));
    assert!(
        !call_args("bash fetch-official-plugins.sh --out \"$stage\"\t# --all # 설명")
            .contains(&"--all"),
        "탭 뒤 주석의 --all 을 인자로 읽었다"
    );
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\" --all";
    assert!(is_staging_fetch_call(&call_args(real)));
    assert!(fetch_flags(&call_args(real)).contains(&"--all"));
    assert!(
        !fetch_flags(&call_args(&real.replacen("--all", "--cache --all", 1))).contains(&"--all"),
        "값 자리의 --all 을 옵션으로 읽었다"
    );
    assert!(
        !is_staging_fetch_call(&call_args(&real.replacen("bash", ":", 1))),
        "무동작 명령을 fetch 호출로 보았다"
    );
    let text = "if [[ \"$from_checkout\" -eq 1 ]]; then\n    bash scripts/fetch-official-plugins.sh --all\nelse\n    bash scripts/fetch-official-plugins.sh --out y\nfi\n";
    let branch = default_staging_branch(text).expect("기본 분기");
    assert_eq!(
        fetch_calls_in(&branch),
        ["bash scripts/fetch-official-plugins.sh --out y"],
        "다른 분기의 호출을 기본 분기로 읽었다"
    );
}

#[test]
fn the_fetch_call_reader_skips_comments_and_messages() {
    let text = "# fetch-official-plugins.sh --all 은 설명이다\n\
                Write-Error \"Install Git to run scripts/fetch-official-plugins.sh.\"\n\
                    bash scripts/fetch-official-plugins.sh --platform x --out y --all\n\
                echo done\n";
    assert_eq!(
        fetch_calls_in(text),
        ["bash scripts/fetch-official-plugins.sh --platform x --out y --all"]
    );
}

#[test]
fn the_const_reader_sees_every_literal_and_skips_comments() {
    let src = "const X: &[S] = &[\n    S { id: \"com.tasty.a\" },\n    // S { id: \"com.tasty.gone\" },\n    S { id: \"com.tasty.b\", other: \"x\" },\n];\nconst X: &[S] = &[\n    S { id: \"com.tasty.c\" },\n];\n";
    let sets = id_sets_in_const(src, "X");
    assert_eq!(sets.len(), 2);
    assert_eq!(
        sets[0],
        ["com.tasty.a", "com.tasty.b"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(sets[1].len(), 1);
}
