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
    platform: Option<String>,
    out: bool,
}

/// 값을 받는 옵션은 다음 인자를 값으로 소비하므로 `--cache --all`의 `--all`은 옵션이 아니다.
/// `--help`나 모르는 인자를 만나면 실제 파서처럼 거기서 멈춘다. 입력은 셸이 이미 나눈 argv다.
fn parse_fetch(argv: &[String]) -> FetchArgs {
    let mut parsed = FetchArgs {
        all: false,
        help: false,
        invalid: false,
        platform: None,
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
                    "--platform" => parsed.platform = Some(value.clone()).filter(|v| !v.is_empty()),
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
    !p.help && !p.invalid && p.platform.is_some() && p.out
}

/// 임시 루트에 `scripts/fetch-official-plugins.sh` 스텁을 두고 대상 코드를 실제 bash로 실행해, 스텁이 받은
/// argv를 호출마다 돌려준다. 셸 문법(따옴표, 리다이렉션, 명령 경계, 줄 연속)은 bash가 판정한다.
/// 대상은 운영 스크립트와 같은 `set -euo pipefail`로 돌고(source와 함수 호출은 운영처럼 따로 선 문장이다),
/// 대상이 실패하면 기록과 관계없이 시험이 실패한다. 스텁은 호출마다 `mktemp`로 고유한 파일을 잡아 인자를
/// NUL로 끝맺어 적으므로 동시 호출, 빈 인자, 제어 문자가 그대로 남는다. `&`로 띄운 fetch는 종료 상태가
/// `set -e`에 닿지 않으므로 시점과 관계없이 실패로 본다. job control이 없는 bash는 비동기 명령의 표준 입력을
/// /dev/null로 바꾸므로, 스텁은 표준 입력이 하네스가 준 파일인지로 이를 가린다. 하네스는 대상의 출력 파이프가
/// 닫힐 때까지(그 파이프를 물려받은 자손이 모두 끝날 때까지) 기다린 뒤 기록을 모으므로 늦게 시작한 호출도 남는다.
/// 임시 루트는 bash의 `mktemp -d`로 만들고 끝나면 지운다.
/// - `package`: `scripts/lib/bundled-plugins.sh`(원문을 넘기면 그 원문)를 source하고 `fetch_bundled_plugins <platform>`을
///   부른다.
/// - `staging`: 넘긴 원문(스테이징 기본 분기)을 `root`·`platform`·`stage` 변수와 함께 실행한다.
#[cfg(unix)]
fn fetch_argvs(mode: &str, platform: &str, branch: &str) -> Vec<Vec<String>> {
    fetch_run(mode, platform, branch).unwrap_or_else(|why| panic!("{why}"))
}

/// `fetch_argvs`와 같고, 대상이 운영 셸 옵션에서 실패하면 `Err`를 돌려준다.
#[cfg(unix)]
fn fetch_run(mode: &str, platform: &str, branch: &str) -> Result<Vec<Vec<String>>, String> {
    const HARNESS: &str = r#"set -eu
repo="$1"; mode="$2"; platform="$3"; branch="$4"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/scripts/lib" "$root/calls" "$root/bg"
: > "$root/stdin"
cat > "$root/scripts/fetch-official-plugins.sh" <<'STUB'
#!/usr/bin/env bash
top="$(cd "$(dirname "$0")/.." && pwd)"
record="$(mktemp "$top/calls/call.XXXXXX")"
printf '%s\0' "$@" > "$record"
# job control이 없는 bash는 `&`로 띄운 명령의 표준 입력을 /dev/null로 바꾼다.
if ! [ /dev/stdin -ef "$top/stdin" ]; then : > "$top/bg/$(basename "$record")"; fi
out=""
while [ $# -gt 0 ]; do
    if [ "$1" = --out ] && [ $# -ge 2 ]; then out="$2"; fi
    shift
done
if [ -n "$out" ]; then mkdir -p "$out" && : > "$out/.fetched"; fi
STUB
chmod +x "$root/scripts/fetch-official-plugins.sh"
rc=0
case "$mode" in
    package)
        if [ -n "$branch" ]; then
            printf '%s\n' "$branch" > "$root/scripts/lib/bundled-plugins.sh"
        else
            cp "$repo/scripts/lib/bundled-plugins.sh" "$root/scripts/lib/"
        fi
        (cd "$root" && bash -euo pipefail -c '. scripts/lib/bundled-plugins.sh
fetch_bundled_plugins "$1"
[ -f target/bundled-plugins/.fetched ]' run "$platform") < "$root/stdin" 2>&1 | cat > /dev/null
        rc=${PIPESTATUS[0]}
        ;;
    staging)
        (cd "$root" && root="$root" platform="$platform" stage="$root/stage" bash -euo pipefail -c "$branch
[ -f \"\$stage/.fetched\" ]") < "$root/stdin" 2>&1 | cat > /dev/null
        rc=${PIPESTATUS[0]}
        ;;
esac
if [ -n "$(ls -A "$root/bg")" ]; then rc=background; fi
printf 'RC=%s\n' "$rc"
for f in "$root/calls"/*; do
    [ -f "$f" ] || continue
    printf 'CALL\n'
    od -An -v -tx1 "$f" | tr -d ' \n'
    printf '\n'
done
"#;
    let out = std::process::Command::new("bash")
        .args(["-c", HARNESS, "harness"])
        .arg(repo_root())
        .args([mode, platform, branch])
        .output()
        .expect("bash 하네스를 실행하지 못했다");
    assert!(
        out.status.success(),
        "bash 하네스가 실패했다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("하네스 출력이 UTF-8이 아니다");
    let mut lines = text.lines();
    if lines.next() != Some("RC=0") {
        return Err(format!(
            "{mode} ({platform}) 대상이 운영 셸 옵션에서 실패했다:\n{text}"
        ));
    }
    let mut calls = Vec::new();
    while let Some(marker) = lines.next() {
        assert_eq!(marker, "CALL", "하네스 출력 형식이 다르다:\n{text}");
        let hex = lines.next().unwrap_or("");
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("16진 기록"))
            .collect();
        let argv = String::from_utf8(bytes).expect("인자가 UTF-8이 아니다");
        // 인자마다 NUL로 끝맺으므로 마지막 조각만 빈 문자열이다. 그 앞의 빈 조각은 실제 빈 인자다.
        let mut args: Vec<String> = argv.split('\0').map(String::from).collect();
        assert_eq!(
            args.pop().as_deref(),
            Some(""),
            "NUL로 끝나지 않은 기록이다"
        );
        calls.push(args);
    }
    Ok(calls)
}

/// `scripts/build-windows.ps1`의 fetch 호출 줄. PowerShell은 이 시험 환경에서 실행하지 않으므로 원문을 읽는다.
/// 따옴표·백틱·줄 연속이 없는 한 줄이어야 공백으로 나눈 단어가 PowerShell의 인자와 같다. 그 밖의 꼴은
/// 판정하지 않고 실패한다. 주석 줄과 안내 명령(`Write-Error`·`Write-Host`)의 문자열은 호출이 아니다.
fn windows_fetch_argv() -> Vec<String> {
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
    windows_call_args(call).unwrap_or_else(|| {
        panic!(
            "scripts/build-windows.ps1 의 fetch 줄이 `{WINDOWS_CALL} <인자>` 꼴(앞 세 단어 일치, 따옴표·백틱·주석·명령 구분·호출 연산자 뒤 `&` 없음)이 아니다: {call}"
        )
    })
}

const WINDOWS_CALL: &str = "& $Bash ./scripts/fetch-official-plugins.sh";

/// PowerShell fetch 줄 하나에서 스크립트 경로 뒤 인자를 읽는다. 판독할 수 있는 꼴이 아니면 `None`이다.
fn windows_call_args(call: &str) -> Option<Vec<String>> {
    let words: Vec<&str> = call.split_whitespace().collect();
    let head: Vec<&str> = WINDOWS_CALL.split(' ').collect();
    let readable = words.starts_with(&head)
        && !call.contains(['"', '\'', '`', '#', ';', '|'])
        && !words[1..].iter().any(|w| w.contains('&'));
    readable.then(|| words[head.len()..].iter().map(|w| w.to_string()).collect())
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
    // 배포 헬퍼는 build-linux.sh(linux-x64·linux-arm64)와 build-macos-dmg.sh(macos-arm64)가 부른다.
    for platform in ["linux-x64", "linux-arm64", "macos-arm64"] {
        let package = fetch_argvs("package", platform, "");
        assert_eq!(
            package.len(),
            1,
            "{platform}: fetch_bundled_plugins 의 fetch 호출이 하나가 아니다: {package:?}"
        );
        let p = parse_fetch(&package[0]);
        assert!(
            really_fetches(&p) && !p.all && p.platform.as_deref() == Some(platform),
            "{platform}: scripts/lib/bundled-plugins.sh 의 배포 fetch 가 그 플랫폼으로 --all 없이 받지 않는다: {:?}",
            package[0]
        );
    }
    let branch = default_staging_branch(&read("scripts/stage-dev-plugins.sh")).expect(
        "scripts/stage-dev-plugins.sh 에서 --from-checkout 이 아닌 기본 분기를 찾지 못했다",
    );
    for platform in [
        "linux-x64",
        "linux-arm64",
        "macos-arm64",
        "windows-x64",
        "windows-arm64",
    ] {
        let staging = fetch_argvs("staging", platform, &branch);
        assert_eq!(
            staging.len(),
            1,
            "{platform}: 기본 분기의 fetch 호출이 하나가 아니다: {staging:?}"
        );
        let p = parse_fetch(&staging[0]);
        assert!(
            really_fetches(&p) && p.all && p.platform.as_deref() == Some(platform),
            "{platform}: scripts/stage-dev-plugins.sh 기본 분기가 그 플랫폼으로 --all 받지 않는다: {:?}",
            staging[0]
        );
    }
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

#[test]
fn the_windows_call_reader_rejects_command_separators() {
    let real = "& $Bash ./scripts/fetch-official-plugins.sh --platform $PluginPlatform --out target/bundled-plugins";
    assert!(windows_call_args(real).is_some(), "정상 호출 꼴을 거부했다");
    for bad in [
        real.replacen(" target/bundled-plugins", " &&", 1),
        format!("{real} && Write-Host done"),
        format!("{real} & other"),
        real.replacen(".sh --platform", ".sh--platform", 1),
        format!("{real}; other"),
    ] {
        assert!(
            windows_call_args(&bad).is_none(),
            "판독할 수 없는 꼴을 읽었다: {bad}"
        );
    }
}

/// 하네스가 셸 판정을 bash에 맡기는지: 명령 경계·따옴표·리다이렉션 변이를 bash가 실제로 넘기는 argv로 본다.
#[cfg(unix)]
#[test]
fn the_staging_harness_reads_what_bash_passes() {
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\" --all";
    let one = |branch: &str| {
        let calls = fetch_argvs("staging", "linux-x64", branch);
        assert_eq!(
            calls.len(),
            1,
            "fetch 호출이 하나가 아니다 ({branch}): {calls:?}"
        );
        parse_fetch(&calls[0])
    };
    assert!(one(real).all && really_fetches(&one(real)));
    for (what, branch) in [
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
    for (what, branch) in [
        ("빈 인자", format!("{real} \"\"")),
        (
            "제어 문자가 붙은 --all",
            real.replacen(" --all", " $'--all\\x1f'", 1),
        ),
    ] {
        assert!(!really_fetches(&one(&branch)), "{what}: 받기로 보았다");
    }
    for (what, branch) in [
        ("fetch 앞의 실패", format!("false\n{real}")),
        ("fetch 뒤의 실패", format!("{real}\nexit 1")),
        (
            "백그라운드로 띄운 fetch",
            real.replacen(" --all", " & : --all", 1),
        ),
        ("백그라운드로 띄운 정상 fetch", format!("{real} &")),
    ] {
        assert!(
            fetch_run("staging", "linux-x64", &branch).is_err(),
            "{what}: 운영 셸 옵션에서 실패하는 분기를 통과시켰다"
        );
    }
    assert!(
        one(&real.replacen(" --all", " \\\n    --all", 1)).all,
        "줄 연속으로 넘긴 --all 을 놓쳤다"
    );
    let calls = fetch_argvs(
        "staging",
        "linux-x64",
        &format!("printf '%s\\n' 'Run scripts/fetch-official-plugins.sh'\n{real}"),
    );
    assert_eq!(calls.len(), 1, "안내문을 fetch 호출로 셌다: {calls:?}");
    // `&`로 띄운 fetch는 끝나는 시점과 관계없이 실패다. 대상이 끝난 뒤 시작하는 호출도 기다려 본다.
    let plain = real.replacen(" --all", "", 1);
    for (what, branch) in [
        (
            "함께 띄우고 기다린 두 호출",
            format!("{real} &\n{plain} &\nwait"),
        ),
        (
            "띄운 뒤 끝날 때까지 쉰 호출",
            format!("{real} &\nsleep 0.6"),
        ),
        (
            "정상 호출 뒤 늦게 띄운 호출",
            format!("{real}\n(sleep 0.5; {plain}) &"),
        ),
        (
            "손자로 늦게 띄운 호출",
            format!("{real}\n( (sleep 0.5; {plain}) & )"),
        ),
    ] {
        assert!(
            fetch_run("staging", "linux-x64", &branch).is_err(),
            "{what}: 백그라운드 fetch를 통과시켰다"
        );
    }
}

/// 배포 하네스는 헬퍼를 운영처럼 따로 선 문장으로 source한다. 헬퍼 최상위의 실패는 fetch 전에 멈춘다.
#[cfg(unix)]
#[test]
fn the_package_harness_stops_on_a_failing_helper() {
    let helper = read("scripts/lib/bundled-plugins.sh");
    assert!(
        fetch_run("package", "linux-x64", &helper).is_ok(),
        "원문 헬퍼를 넘긴 배포 하네스가 실패했다"
    );
    assert!(
        fetch_run("package", "linux-x64", &format!("false\n{helper}")).is_err(),
        "헬퍼 최상위의 실패를 통과시켰다"
    );
}

/// 기본 분기 추출기는 `--from-checkout` 분기의 호출을 기본 분기 판정에 넣지 않는다.
#[cfg(unix)]
#[test]
fn the_default_branch_reader_ignores_the_checkout_branch() {
    let real = "    bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\"";
    let text = format!("if [[ \"$from_checkout\" -eq 1 ]]; then\n{real} --all\nelse\n{real}\nfi\n");
    let branch = default_staging_branch(&text).expect("기본 분기를 찾지 못했다");
    let calls = fetch_argvs("staging", "linux-x64", &branch);
    assert_eq!(
        calls.len(),
        1,
        "기본 분기의 fetch 호출이 하나가 아니다: {calls:?}"
    );
    assert!(
        !parse_fetch(&calls[0]).all,
        "--from-checkout 분기의 --all 을 기본 분기 판정에 썼다"
    );
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
