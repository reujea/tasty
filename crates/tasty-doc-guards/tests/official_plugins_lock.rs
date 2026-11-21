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
//!    (`scripts/stage-dev-plugins.sh`의 기본 경로)은 `--all`로 모두 받는다. Bash 스크립트는 스텁 fetch를 둔
//!    임시 루트에서 실제 bash로 실행해 받은 argv를 보고(Unix), PowerShell 줄은 원문을 읽는다. 잠금 항목의 `bundle`과 매니페스트의
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

/// fetch 스크립트가 받은 인자(argv)를 `scripts/fetch-official-plugins.sh`의 파서처럼 순서대로 한 번 읽은 결과.
struct FetchArgs {
    all: bool,
    /// `-h`·`--help`를 만나 사용법만 내고 끝난다.
    help: bool,
    /// 모르는 인자나 값이 빠진 옵션을 만나 종료 2로 끝난다.
    invalid: bool,
    platform: bool,
    out: bool,
}

/// 값을 받는 옵션은 다음 인자를 값으로 소비하므로 `--cache --all`의 `--all`은 옵션이 아니다.
/// `--help`나 모르는 인자를 만나면 실제 파서처럼 거기서 멈춘다. 입력은 셸이 이미 나눈 argv다.
fn parse_fetch(argv: &[String]) -> FetchArgs {
    let mut parsed = FetchArgs {
        all: false,
        help: false,
        invalid: false,
        platform: false,
        out: false,
    };
    let mut rest = argv.iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--platform" | "--out" | "--lock" | "--cache" => {
                let Some(value) = rest.next() else {
                    parsed.invalid = true;
                    break;
                };
                match a.as_str() {
                    "--platform" => parsed.platform = !value.is_empty(),
                    "--out" => parsed.out = !value.is_empty(),
                    _ => {}
                }
            }
            "--all" => parsed.all = true,
            "-h" | "--help" => {
                parsed.help = true;
                break;
            }
            _ => {
                parsed.invalid = true;
                break;
            }
        }
    }
    parsed
}

/// 받기까지 가는 호출: 도움말·모르는 인자·값 빠진 옵션 없이 `--platform`·`--out`에 값이 있다.
fn really_fetches(p: &FetchArgs) -> bool {
    !p.help && !p.invalid && p.platform && p.out
}

/// 임시 루트에 `scripts/fetch-official-plugins.sh` 스텁을 두고 대상 코드를 실제 bash로 실행해, 스텁이 받은
/// argv를 호출마다 돌려준다. 셸 문법(따옴표, 리다이렉션, 명령 경계, 줄 연속)은 bash가 판정한다.
/// 임시 루트는 bash의 `mktemp -d`로 만들고 끝나면 지운다.
/// - `package`: `scripts/lib/bundled-plugins.sh`를 source하고 `fetch_bundled_plugins`를 부른다.
/// - `staging`: 넘긴 원문(스테이징 기본 분기)을 `root`·`platform`·`stage` 변수와 함께 실행한다. 백그라운드로 띄운
///   명령도 기록되도록 끝에서 기다린다.
#[cfg(unix)]
fn fetch_argvs(mode: &str, branch: &str) -> Vec<Vec<String>> {
    const HARNESS: &str = r#"set -eu
repo="$1"; mode="$2"; branch="$3"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/scripts/lib"
cat > "$root/scripts/fetch-official-plugins.sh" <<'STUB'
#!/usr/bin/env bash
log="$(cd "$(dirname "$0")/.." && pwd)/fetch.log"
for a in "$@"; do printf '%s' "$a" >> "$log"; done
printf '' >> "$log"
while [ $# -gt 0 ]; do
    if [ "$1" = --out ] && [ $# -ge 2 ]; then mkdir -p "$2"; fi
    shift
done
STUB
chmod +x "$root/scripts/fetch-official-plugins.sh"
case "$mode" in
    package)
        cp "$repo/scripts/lib/bundled-plugins.sh" "$root/scripts/lib/"
        (cd "$root" && . scripts/lib/bundled-plugins.sh && fetch_bundled_plugins linux-x64) > /dev/null 2>&1 || true
        ;;
    staging)
        (cd "$root" && root="$root" platform=linux-x64 stage="$root/stage" bash -c "$branch
wait") > /dev/null 2>&1 || true
        ;;
esac
if [ -f "$root/fetch.log" ]; then cat "$root/fetch.log"; fi
"#;
    let out = std::process::Command::new("bash")
        .args(["-c", HARNESS, "harness"])
        .arg(repo_root())
        .args([mode, branch])
        .output()
        .expect("bash 하네스를 실행하지 못했다");
    assert!(
        out.status.success(),
        "bash 하네스가 실패했다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("스텁 기록이 UTF-8이 아니다")
        .split('\u{1e}')
        .filter(|call| !call.is_empty())
        .map(|call| {
            call.split('\u{1f}')
                .filter(|a| !a.is_empty())
                .map(String::from)
                .collect()
        })
        .collect()
}

/// `scripts/build-windows.ps1`의 fetch 호출 줄. PowerShell은 이 시험 환경에서 실행하지 않으므로 원문을 읽는다.
/// 따옴표·백틱·줄 연속이 없는 한 줄이어야 공백으로 나눈 단어가 PowerShell의 인자와 같다. 그 밖의 꼴은
/// 판정하지 않고 실패한다. 주석 줄과 안내 명령(`Write-Error`·`Write-Host`)의 문자열은 호출이 아니다.
fn windows_fetch_argv() -> Vec<String> {
    const CALL: &str = "& $Bash ./scripts/fetch-official-plugins.sh";
    let text = read("scripts/build-windows.ps1");
    let calls: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("fetch-official-plugins.sh"))
        .filter(|l| {
            !l.starts_with('#') && !l.starts_with("Write-Error") && !l.starts_with("Write-Host")
        })
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "scripts/build-windows.ps1 의 fetch 줄이 하나가 아니다: {calls:?}"
    );
    let call = calls[0];
    assert!(
        call.starts_with(CALL) && !call.contains(['"', '\'', '`', '#', ';', '|']),
        "scripts/build-windows.ps1 의 fetch 줄이 `{CALL} <인자>` 꼴(따옴표·백틱·주석·명령 구분 없음)이 아니다: {call}"
    );
    call[CALL.len()..]
        .split_whitespace()
        .map(String::from)
        .collect()
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
/// Bash 스크립트는 실제 bash로 실행해 스텁이 받은 argv를 본다(Unix에서만 돈다).
#[cfg(unix)]
#[test]
fn the_package_scripts_fetch_only_bundled_entries_and_dev_staging_fetches_all() {
    let package = fetch_argvs("package", "");
    assert_eq!(
        package.len(),
        1,
        "fetch_bundled_plugins 의 fetch 호출이 하나가 아니다: {package:?}"
    );
    let p = parse_fetch(&package[0]);
    assert!(
        really_fetches(&p) && !p.all,
        "scripts/lib/bundled-plugins.sh 의 배포 fetch 가 --all 없이 --platform·--out 으로 받지 않는다: {:?}",
        package[0]
    );
    let branch = default_staging_branch(&read("scripts/stage-dev-plugins.sh")).expect(
        "scripts/stage-dev-plugins.sh 에서 --from-checkout 이 아닌 기본 분기를 찾지 못했다",
    );
    let staging = fetch_argvs("staging", &branch);
    assert_eq!(
        staging.len(),
        1,
        "기본 분기의 fetch 호출이 하나가 아니다: {staging:?}"
    );
    let p = parse_fetch(&staging[0]);
    assert!(
        really_fetches(&p) && p.all,
        "scripts/stage-dev-plugins.sh 기본 분기가 --platform·--out·--all 로 받지 않는다: {:?}",
        staging[0]
    );
}

#[test]
fn the_windows_package_fetches_only_bundled_entries() {
    let argv = windows_fetch_argv();
    let p = parse_fetch(&argv);
    assert!(
        really_fetches(&p) && !p.all,
        "scripts/build-windows.ps1 의 배포 fetch 가 --all 없이 --platform·--out 으로 받지 않는다: {argv:?}"
    );
}

/// 하네스가 셸 판정을 bash에 맡기는지: 명령 경계·따옴표·리다이렉션 변이를 bash가 실제로 넘기는 argv로 본다.
#[cfg(unix)]
#[test]
fn the_staging_harness_reads_what_bash_passes() {
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\" --all";
    let one = |branch: &str| {
        let calls = fetch_argvs("staging", branch);
        assert_eq!(
            calls.len(),
            1,
            "fetch 호출이 하나가 아니다 ({branch}): {calls:?}"
        );
        parse_fetch(&calls[0])
    };
    assert!(one(real).all && really_fetches(&one(real)));
    for (what, branch) in [
        ("뒤 명령의 --all", real.replacen(" --all", " & : --all", 1)),
        (
            "값 자리의 --all",
            real.replacen(" --all", " --cache --all", 1),
        ),
        (
            "따옴표 안 공백이 든 값",
            real.replacen(" --all", " --cache \"x --all\"", 1),
        ),
        ("주석", real.replacen(" --all", " # --all", 1)),
    ] {
        assert!(!one(&branch).all, "{what}: --all 로 읽었다");
    }
    assert!(
        !really_fetches(&one(&real.replacen(" --all", " '2>&1' --all", 1))),
        "따옴표로 넘긴 '2>&1' 인자를 받기로 보았다"
    );
    assert!(
        one(&real.replacen(" --all", " 2>&1 --all", 1)).all,
        "리다이렉션 뒤의 --all 을 놓쳤다"
    );
    assert!(
        one(&real.replacen(" --all", " \\\n    --all", 1)).all,
        "줄 연속으로 넘긴 --all 을 놓쳤다"
    );
    let calls = fetch_argvs(
        "staging",
        &format!("printf '%s\\n' 'Run scripts/fetch-official-plugins.sh'\n{real}"),
    );
    assert_eq!(calls.len(), 1, "안내문을 fetch 호출로 셌다: {calls:?}");
}

#[test]
fn the_fetch_argument_reader_follows_the_fetch_parser() {
    let argv = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    assert!(really_fetches(&parse_fetch(&argv(
        "--platform p --out o --all"
    ))));
    assert!(parse_fetch(&argv("--platform p --out o --all")).all);
    assert!(!parse_fetch(&argv("--platform p --out o --cache --all")).all);
    for bad in [
        "--cache --platform p --out o",
        "--platform p --out o --help --all",
        "--platform p --out o 2>&1",
        "--platform p --out",
        "--platform p",
    ] {
        assert!(
            !really_fetches(&parse_fetch(&argv(bad))),
            "{bad}: 받기로 보았다"
        );
    }
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
