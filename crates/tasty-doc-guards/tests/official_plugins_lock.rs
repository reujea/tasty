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

#[cfg(unix)]
use tasty_doc_guards::floored_walk::{CountedOn, Floor, Pick, walk_dirs_with_floor};
use tasty_doc_guards::official_plugins::{
    LOCK_FILE, Lock, REFRESH_HINT, check, contract_mismatches, parse_lock, read_host_contract,
};
#[cfg(unix)]
use tasty_doc_guards::plugin_manifest_scan::{scan_text, top_level};
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

/// 배포 받기는 `bundle = true` 항목이 없으면 실제 fetch가 종료 1로 실패한다. generate는 이런 잠금도 만든다.
fn assert_some_entries_are_bundled() {
    assert!(
        !bundled_ids().is_empty(),
        "{LOCK_FILE}에 bundle = true 항목이 없다. 배포 패키지의 fetch(--all 없음)는 받을 것이 없어 실패한다"
    );
}

/// 잠금 파일에서 배포 패키지에 싣는(`bundle = true`) id.
fn bundled_ids() -> Vec<String> {
    lock()
        .plugins
        .iter()
        .filter(|p| p.bundle)
        .map(|p| p.id.clone())
        .collect()
}

/// `crates/` 바로 아래 디렉터리 순회의 하한. 하한은 방문한 디렉터리 수에 적용한다.
#[cfg(unix)]
const CRATES_DIR_FLOOR: Floor = Floor {
    min: 45,
    measured: 54,
    measured_on: "2026-11-21",
    counted_on: CountedOn::Tree(
        "8d9f31b1fd — git ls-tree -d --name-only HEAD crates/ 로 센 crates 바로 아래 디렉터리 54개.",
    ),
    why_this_gap: "crates 바로 아래 디렉터리만 방문하고 그 아래로는 내려가지 않는다. 크레이트 몇 개를 합치거나 옮겨도 통과하도록 9개 여유를 둔다. 그보다 줄면 루트를 잘못 잡았거나 순회가 실패한 것이므로 재측정한다.",
};

/// workspace 플러그인 크레이트 가운데 매니페스트가 있고 최상위 `bundle = false`를 적지 않은 것의 id.
/// 배포 스크립트는 잠금 파일의 `bundle = true` 산출물만 싣고 workspace 플러그인은 싣지 않는다.
#[cfg(unix)]
fn workspace_ids_claiming_to_ship(root: &std::path::Path) -> BTreeSet<String> {
    let crates = walk_dirs_with_floor(&root.join("crates"), root, &CRATES_DIR_FLOOR, &|_| {
        Pick::TakeAndStop
    })
    .unwrap_or_else(|why| panic!("{why}"));
    let mut ids = BTreeSet::new();
    for krate in crates {
        let name = krate
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.starts_with("tasty-plugin-") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(krate.path.join("tasty-plugin.toml")) else {
            continue;
        };
        if let Some(id) =
            shipping_id(&text).unwrap_or_else(|why| panic!("{name}/tasty-plugin.toml: {why}"))
        {
            ids.insert(id);
        }
    }
    ids
}

/// 매니페스트가 배포 대상이라고 선언하면(최상위 `bundle`이 없거나 true) 그 id를 돌려준다.
/// sync 스크립트와 같은 TOML 스캐너로 표 구조를 따라 읽으므로 표 머리 아래의 `bundle`은 최상위 값이 아니다.
#[cfg(unix)]
fn shipping_id(manifest: &str) -> Result<Option<String>, String> {
    let top = scan_text(manifest).and_then(|scan| top_level(&scan))?;
    if !top.bundle {
        return Ok(None);
    }
    top.id
        .map(Some)
        .ok_or_else(|| "최상위 id가 없다".to_string())
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
    /// `--lock`을 넘겼다. 기본 파일을 명시해도 이 가드는 옵션 자체를 거부한다.
    lock: bool,
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
        lock: false,
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
                    "--lock" => parsed.lock = true,
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

/// 기본 잠금 파일로 받기까지 가는 호출: 도움말·모르는 인자·값 빠진 옵션 없이 `--platform`·`--out`에 값이 있고,
/// `--lock`을 넘기지 않는다(이 가드는 기본 잠금만 읽으므로 기본 파일을 명시한 `--lock`도 거부한다).
fn really_fetches(p: &FetchArgs) -> bool {
    !p.help && !p.invalid && p.platform.is_some() && p.out && !p.lock
}

/// 임시 루트에 `scripts/fetch-official-plugins.sh` 스텁을 두고 대상 코드를 실제 bash로 실행해, 스텁이 받은
/// argv를 호출마다 돌려준다. 셸 문법(따옴표, 리다이렉션, 명령 경계, 줄 연속)은 bash가 판정한다.
/// 대상은 운영 스크립트와 같은 `set -euo pipefail`로 돌고(source와 함수 호출은 운영처럼 따로 선 문장이다),
/// 대상이 실패하면 기록과 관계없이 시험이 실패한다. 스텁은 호출마다 `mktemp`로 고유한 파일을 잡아 인자를
/// NUL로 끝맺어 적으므로 동시 호출, 빈 인자, 제어 문자가 그대로 남는다.
/// 동시 호출은 같은 `--out`(산출물과 표지 `.fetched`)을 함께 쓰므로 그 산출물 단계는 호출 사이에 잠금으로 차례로
/// 돈다. 잠금을 얻지 못하거나 놓지 못하면 하네스가 실패한다.
/// 대상은 네 번 돈다. 성공 실행은 스텁이 `--out`에 픽스처의 플러그인 트리(`<id>/tasty-plugin.toml` 등)와 표지를
/// 남기고 종료 0이며, 대상이 0으로 끝나고 분기 뒤 하네스 검사까지 도달해야 한다. 실패 주입 세 번은 같은 기록을
/// 남긴 뒤 `early1`(산출물 없이 종료 1), `late1`(산출물을 남기고 종료 1), `late23`(산출물을 남기고 종료 23)이다.
/// 실패 주입에서 대상의 원래 종료값이 0이면 fetch 실패가 운영 흐름을 멈추지 않는다는 뜻이므로(`&`로 띄운 호출,
/// `|| true`, `|| exit 0`, 인자 없는 `wait` 등 표기와 관계없이) 실패로 본다.
/// 하네스 검사에서 `.fetched`가 없으면 하네스 루트에 `.missing-fetched` 표지를 남기고 1로 끝난다. `package`는 루트로
/// 들어간 뒤의 `$PWD`를 넘기므로 헬퍼가 작업 폴더를 바꿔도 표지 경로가 절대 경로다. `staging`은 `mktemp -d` 결과를
/// 그대로 넘기므로 `TMPDIR`이 상대 경로면 표지 경로도 상대 경로다(그때는 스텁 경로 등 하네스 전체가 이미 깨지는
/// 기존 한계다).
/// 실패 주입 실행에 이 표지가 있으면 대상이 fetch 실패에서 멈추지 않고 하네스 검사까지 간 것이므로 종료값이 1이어도
/// 실패로 본다. 기다리지 않은 백그라운드 호출은 검사와 스텁의 `.fetched` 생성 중 어느 쪽이 먼저냐에 따라 종료값이
/// 0이나 1이 되는데, 두 경우 모두 이 판정이나 위의 종료값 0 판정에 걸리므로 실행 순서와 관계없이 거부된다.
/// 실패를 삼킨 뒤 산출물을 지우는 호출도 같은 표지를 남긴다. 표지를 쓰지 못하면 `.reached` 없이 0으로 끝내
/// `0-unreached`로 거부한다(종료값 1로 정상 전파와 섞이지 않게 한다).
/// 하네스는 stdout·stderr를 한 수집 파이프에, fd 9를 스텁의 제어 파이프에 잇고 두 파이프가 모두 닫힐 때까지(그 fd 중
/// 하나라도 물려받은 자손이 모두 끝날 때까지) 기다린 뒤 기록을 모으므로, 일부 fd를 돌린 채 늦게 시작한 호출도 남는다.
/// 스텁의 산출물 단계가 실패하면 그 호출은 그 값으로 끝나고, `copy-error` 파일과 fd 9 표지로 알려 하네스가 실패한다
/// (인자 없는 `wait`이 스텁의 종료값을 삼키고 다른 호출의 `.fetched`가 남아도 드러난다). 수집기 자체가 실패해도
/// 하네스가 실패한다.
/// 스텁 산출물은 실제 fetch처럼 `--all`이면 잠금의 모든 항목, 아니면 `bundle = true` 항목만이다.
/// 이 가드는 정직한 실수(배포 호출의 `--all`, 실패 무시, 백그라운드 실행, 검사 전 종료)를 잡는다. 가드를 일부러
/// 피하도록 쓴 대상(1·23 밖의 특정 종료값이나 하네스만의 산출물 차이를 보고 실패를 삼키기, 세 fd를 모두 닫거나 돌리고
/// 늦게 시작하는 자손)은 범위 밖이며 잡지 못한다. 백그라운드로 띄운 뒤 하네스 검사에 닿기 전에 대상이 스스로
/// 산출물을 읽다 실패하는 꼴(직접 `.fetched`를 검사하거나 없는 폴더를 `find`하다 pipefail로 멈추기)도 종료값이
/// 실행 순서에 달려 있어 잡지 못할 수 있다.
/// 임시 루트는 bash의 `mktemp -d`로 만들고 끝나면 지운다.
/// - `package`: `scripts/lib/bundled-plugins.sh`(원문을 넘기면 그 원문)를 source하고 `fetch_bundled_plugins <platform>`을
///   부른다.
/// - `staging`: 넘긴 원문(스테이징 기본 분기)을 `root`·`platform`·`stage` 변수와 함께 실행한다.
#[cfg(unix)]
fn fetch_argvs(mode: &str, platform: &str, branch: &str) -> Vec<Vec<String>> {
    fetch_run(mode, platform, branch).unwrap_or_else(|why| panic!("{why}"))
}

/// `fetch_argvs`와 같고, 대상이 운영 셸 옵션에서 실패하거나, fetch 실패에도 성공하거나, 실패 주입 실행에서 하네스
/// 검사까지 가서 `.fetched`가 없으면 `Err`를 돌려준다.
#[cfg(unix)]
fn fetch_run(mode: &str, platform: &str, branch: &str) -> Result<Vec<Vec<String>>, String> {
    let all: Vec<String> = locked_ids(&lock()).into_iter().collect();
    fetch_run_with(mode, platform, branch, &all, &bundled_ids())
}

/// `fetch_run`과 같고, 스텁이 `--all`로 받을 때와 없이 받을 때 산출물로 둘 id를 잠금 파일 대신 `all`·`bundled`로
/// 정한다. 픽스처에 없는 id는 빈 폴더로 만든다.
#[cfg(unix)]
fn fetch_run_with(
    mode: &str,
    platform: &str,
    branch: &str,
    all: &[String],
    bundled: &[String],
) -> Result<Vec<Vec<String>>, String> {
    let (rc, calls) = fetch_collect(mode, platform, branch, "", all, bundled);
    if rc != "0" {
        return Err(format!(
            "{mode} ({platform}) 대상이 운영 셸 옵션에서 실패했다(RC={rc}): {calls:?}"
        ));
    }
    // 실제 fetch는 산출물 전후 어디서든 종료 1로 실패한다. 특정 종료값만 삼키는 꼴도 거부하도록 세 가지로 돈다.
    for fail in ["early1", "late1", "late23"] {
        let (rc, failed) = fetch_collect(mode, platform, branch, fail, all, bundled);
        if rc == "0" || rc.starts_with("0-") {
            return Err(format!(
                "{mode} ({platform}) 대상이 fetch 실패({fail})에도 성공했다(실패를 전파하지 않는 호출): {failed:?}"
            ));
        }
        if rc.ends_with("-missing-fetched") {
            return Err(format!(
                "{mode} ({platform}) 실패 주입({fail}) 실행에서 대상이 하네스 검사까지 진행했고 .fetched가 없었다(RC={rc}): {failed:?}"
            ));
        }
    }
    Ok(calls)
}

/// 하네스를 한 번 돌려 대상의 종료 상태와 스텁이 받은 argv를 돌려준다. `fail`이 `early1`이면 스텁이 기록만 하고
/// 산출물 없이 종료 1, `late1`·`late23`이면 산출물 표지까지 남긴 뒤 종료 1·23이다(실제 fetch는 일부 산출물을 만든 뒤에도
/// 종료 1로 실패한다). 대상이 0으로 끝났고 `.reached`가 없으면(중간의 `exit 0`, 하네스 검사의 표지 쓰기 실패 등)
/// RC는 원래 종료값을 앞에 둔 `0-unreached`다. 하네스 검사에서 `.fetched`가 없어 `.missing-fetched` 표지가 남았으면
/// RC 끝에 `-missing-fetched`를 붙인다(예: `1-missing-fetched`).
/// 하네스 자체가 실패하면(잠금·산출물·수집기 오류) 패닉한다.
#[cfg(unix)]
fn fetch_collect(
    mode: &str,
    platform: &str,
    branch: &str,
    fail: &str,
    all: &[String],
    bundled: &[String],
) -> (String, Vec<Vec<String>>) {
    let out = fetch_harness(mode, platform, branch, fail, all, bundled);
    assert!(
        out.status.success(),
        "bash 하네스가 실패했다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse_harness(out.stdout)
}

/// `fetch_collect`의 하네스 실행만 한다. `fail`의 `copy<N>`·`copy<N> <K>`는 N번째로 산출물 단계에 들어온 호출의 복사를
/// 종료값 1·K로 실패시킨다(하네스 자체가 그 실패로 끝나는지 보는 시험 전용). `mode`가 `stub`이면 대상 없이 스텁을
/// 직접 한 번 부르고 그 종료값만 RC로 낸다.
#[cfg(unix)]
fn fetch_harness(
    mode: &str,
    platform: &str,
    branch: &str,
    fail: &str,
    all: &[String],
    bundled: &[String],
) -> std::process::Output {
    const HARNESS: &str = r#"set -eu
repo="$1"; mode="$2"; platform="$3"; branch="$4"; fail="$5"; bundled="$6"; all="$7"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/scripts/lib" "$root/calls"
# 스텁은 잠금 문제와 산출물 단계 실패를 이 파일들에 덧붙인다. 미리 만들어 두면 루트에 새 항목을 못 만드는 경우에도
# 기록이 남는다. 산출물 실패는 파일에 쓰지 못해도 드러나도록 fd 9(제어 파이프)에도 표지로 알린다.
: > "$root/lock-error"
: > "$root/copy-error"
marker=@@tasty-stub-copy-error@@
collector_marker=@@tasty-collector-failed@@
if [ -n "$fail" ]; then printf '%s' "$fail" > "$root/fail"; fi
# 실제 fetch처럼 `--all`이면 잠금의 모든 항목, 아니면 `bundle = true` 항목만 산출물로 둔다.
fixtures="$repo/crates/tasty-test-support/fixtures/official-plugins"
tree_of() {
    mkdir -p "$1"
    shift
    for id in "$@"; do
        if [ -d "$fixtures/$id" ]; then cp -R "$fixtures/$id" "$tree/"; else mkdir -p "$tree/$id"; fi
    done
}
tree="$root/tree-all"; tree_of "$tree" $all
tree="$root/tree-bundle"; tree_of "$tree" $bundled
printf '#!/usr/bin/env bash\nmarker=%q\n' "$marker" > "$root/scripts/fetch-official-plugins.sh"
cat >> "$root/scripts/fetch-official-plugins.sh" <<'STUB'
top="$(cd "$(dirname "$0")/.." && pwd)"
record="$(mktemp "$top/calls/call.XXXXXX")"
printf '%s\0' "$@" > "$record"
# 산출물은 실제 fetch의 인자 판독(값 옵션은 다음 인자를 소비, 모르는 인자에서 멈춤)으로 정한다.
out=""
tree=tree-bundle
while [ $# -gt 0 ]; do
    case "$1" in
        --platform|--lock|--cache) [ $# -ge 2 ] || break; shift 2 ;;
        --out) [ $# -ge 2 ] || break; out="$2"; shift 2 ;;
        --all) tree=tree-all; shift ;;
        *) break ;;
    esac
done
if [ "$(cat "$top/fail" 2>/dev/null)" = early1 ]; then exit 1; fi
# 실제 fetch는 고른 항목이 하나도 없으면 산출물을 만들기 전에 종료 1이다.
if [ -z "$(ls -A "$top/$tree")" ]; then exit 1; fi
if [ -n "$out" ]; then
    # 같은 --out에 함께 복사하면 GNU cp가 상대가 먼저 만든 항목에서 File exists로 실패하므로 산출물 단계만
    # 호출 사이에 차례로 돈다. flock은 macOS에 기본으로 없고, 일부 mkdir 구현(Ubuntu 26.04 기본 uutils)은 동시에
    # 같은 디렉터리를 만들 때 진 쪽도 0으로 끝나 mkdir 잠금이 배타적이지 않다. 그래서 bash 자체의 noclobber
    # 리다이렉션(없는 파일을 O_EXCL로 연다)으로 잠금 파일을 만든다. 운영 스크립트의 scripts/lib/run-lock.sh와 같은
    # 원리이고, 이 스텁은 기다리며 다시 시도한다. set -C는 뒤의 덮어쓰기에 번지지 않게 서브셸에
    # 둔다. 잠금 문제는 인자 없는 wait가 삼키지 않도록 lock-error에 적고 하네스가 수집 뒤 실패로 낸다.
    lock="$top/out.lock"; waited=0
    until ( set -C; : > "$lock" ) 2>/dev/null; do
        waited=$((waited + 1))
        if [ "$waited" -ge 500 ]; then echo "산출물 잠금을 얻지 못했다: $lock" >> "$top/lock-error"; exit 97; fi
        sleep 0.01
    done
    # 잠금 안에서 산출물 단계에 들어온 순번을 센다. fail이 "copy<N> <K>"면 N번째 호출의 복사가 종료값 K(없으면 1)로
    # 실패한다.
    entered=$(( $(cat "$top/out.entered" 2>/dev/null || echo 0) + 1 ))
    echo "$entered" > "$top/out.entered"
    f_mode="" f_code=""
    read -r f_mode f_code 2>/dev/null < "$top/fail" || :
    fail_copy() { return "${f_code:-1}"; }
    copy=cp
    if [ "$f_mode" = "copy$entered" ]; then copy=fail_copy; fi
    made=0
    { mkdir -p "$out" && "$copy" -R "$top/$tree/." "$out/" && : > "$out/.fetched"; } || made=$?
    rm "$lock" || echo "산출물 잠금을 놓지 못했다: $lock" >> "$top/lock-error"
    # 산출물 실패는 이 호출의 종료값이다. 인자 없는 wait이 그 값을 삼킬 수 있으므로 하네스에도 알린다. 두 통지는
    # 한쪽이 신호(SIGXFSZ 등)로 끝나도 다른 쪽과 종료가 돌도록 각각 서브셸에 둔다.
    if [ "$made" -ne 0 ]; then
        ( echo "산출물 단계가 실패했다(rc=$made): $out" >> "$top/copy-error" ) 2>/dev/null || :
        ( printf '\n%s\n' "$marker" >&9 ) 2>/dev/null || :
        exit "$made"
    fi
fi
case "$(cat "$top/fail" 2>/dev/null)" in late1) exit 1 ;; late23) exit 23 ;; esac
STUB
chmod +x "$root/scripts/fetch-official-plugins.sh"
rc=0
cst=0
# 대상의 stdout·stderr는 cat 파이프로, fd 9는 awk 제어 파이프로 보낸다. awk는 EOF까지 읽고 스텁 표지가 있으면 3,
# 안쪽 cat이 실패했다는 표지가 있으면 4로 끝난다. 표지는 줄 끝으로 판정한다(bash printf는 앞 줄바꿈과 표지를 따로
# 쓰므로 그 사이 다른 출력이 끼면 표지가 그 뒤에 붙는다).
collect() {
    set +e
    ( "$@" 9>&4 4>&- 2>&1 | cat > /dev/null 4>&-
      inner="${PIPESTATUS[0]} ${PIPESTATUS[1]}"
      if [ "${inner#* }" != 0 ]; then printf '\n%s\n' "$collector_marker" >&4; fi
      exit "${inner% *}" ) 4>&1 | awk -v m="$marker" -v c="$collector_marker" '
        substr($0, length($0) - length(c) + 1) == c { bad = 1 }
        substr($0, length($0) - length(m) + 1) == m { n++ }
        END { exit (bad ? 4 : (n ? 3 : 0)) }'
    st="${PIPESTATUS[0]} ${PIPESTATUS[1]}"
    set -e
    rc=${st% *}
    cst=${st#* }
}
run_package() {
    (cd "$root" && root="$PWD" bash -euo pipefail -c '. scripts/lib/bundled-plugins.sh
fetch_bundled_plugins "$1"
[ -f target/bundled-plugins/.fetched ] || { : > "$root/.missing-fetched" || exit 0; exit 1; }
: > .reached' run "$platform")
}
run_staging() {
    (cd "$root" && root="$root" platform="$platform" stage="$root/stage" bash -euo pipefail -c "$branch
[ -f \"\$stage/.fetched\" ] || { : > \"\$root/.missing-fetched\" || exit 0; exit 1; }
: > \"\$root/.reached\"")
}
case "$mode" in
    package)
        if [ -n "$branch" ]; then
            printf '%s\n' "$branch" > "$root/scripts/lib/bundled-plugins.sh"
        else
            cp "$repo/scripts/lib/bundled-plugins.sh" "$root/scripts/lib/"
        fi
        collect run_package
        ;;
    staging)
        collect run_staging
        ;;
    stub)
        # 대상 없이 스텁을 직접 한 번 부른다. 스텁 자신의 종료값만 RC로 내고 기록 판정은 하지 않는다.
        set +e
        bash "$root/scripts/fetch-official-plugins.sh" --platform "$platform" --out "$root/stage" --all 9> /dev/null > /dev/null 2>&1
        rc=$?
        set -e
        printf 'RC=%s\n' "$rc"
        exit 0
        ;;
esac
if [ "$rc" = 0 ] && ! [ -e "$root/.reached" ]; then rc=0-unreached; fi
if [ -f "$root/.missing-fetched" ]; then rc="$rc-missing-fetched"; fi
if [ -s "$root/lock-error" ]; then cat "$root/lock-error" >&2; exit 97; fi
if [ "$cst" != 0 ] && [ "$cst" != 3 ]; then echo "수집기가 실패했다($cst)" >&2; exit 98; fi
if [ -s "$root/copy-error" ] || [ "$cst" = 3 ]; then
    if [ -s "$root/copy-error" ]; then cat "$root/copy-error" >&2; else echo "스텁 산출물 단계가 실패했다(fd 9 표지)" >&2; fi
    exit 98
fi
printf 'RC=%s\n' "$rc"
for f in "$root/calls"/*; do
    [ -f "$f" ] || continue
    printf 'CALL\n'
    od -An -v -tx1 "$f" | tr -d ' \n'
    printf '\n'
done
"#;
    std::process::Command::new("bash")
        .args(["-c", HARNESS, "harness"])
        .arg(repo_root())
        .args([
            mode,
            platform,
            branch,
            fail,
            &bundled.join(" "),
            &all.join(" "),
        ])
        .output()
        .expect("bash 하네스를 실행하지 못했다")
}

/// 하네스 표준출력을 대상의 종료 상태와 스텁 호출 기록으로 나눈다.
#[cfg(unix)]
fn parse_harness(stdout: Vec<u8>) -> (String, Vec<Vec<String>>) {
    let text = String::from_utf8(stdout).expect("하네스 출력이 UTF-8이 아니다");
    let mut lines = text.lines();
    let rc = lines
        .next()
        .and_then(|l| l.strip_prefix("RC="))
        .unwrap_or_else(|| panic!("하네스 출력 형식이 다르다:\n{text}"))
        .to_string();
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
    (rc, calls)
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
#[cfg(unix)]
fn default_staging_branch(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "if [[ \"$from_checkout\" -eq 1 ]]; then")?;
    let open = start + lines[start..].iter().position(|l| *l == "else")?;
    let close = open + lines[open..].iter().position(|l| *l == "fi")?;
    Some(lines[open + 1..close].join("\n"))
}

/// 배포 패키지가 싣는 집합은 잠금 파일의 `bundle = true` 항목이다. workspace 플러그인(mesh-demo)이
/// `bundle = false`를 잃거나 true로 바꾸면 배포 대상이라고 선언하지만 패키지에는 들어가지 않으므로 실패한다.
/// 스캐너가 awk로 돌므로 Unix에서만 실행한다. 매니페스트 원문은 OS마다 같아 Linux 실행으로 덮인다.
#[cfg(unix)]
#[test]
fn the_shipped_set_is_the_bundled_lock_entries() {
    let bundled: BTreeSet<String> = bundled_ids().into_iter().collect();
    let mut shipped = bundled.clone();
    shipped.extend(workspace_ids_claiming_to_ship(&repo_root()));
    assert_eq!(
        shipped, bundled,
        "workspace 플러그인(crates/tasty-plugin-*/tasty-plugin.toml 중 bundle = false가 아닌 것)이 배포 대상이라고 \
         선언한다. 배포 스크립트는 잠금 파일의 bundle = true 산출물만 싣는다"
    );
}

/// 배포 선언 판독은 줄 모양이 아니라 TOML 구조를 따른다.
#[cfg(unix)]
#[test]
fn the_shipping_reader_follows_the_toml_structure() {
    assert_eq!(
        shipping_id("id = \"com.tasty.x\"\n\"bundle\" = false\n"),
        Ok(None),
        "따옴표 키 \"bundle\" = false 를 배포 제외로 읽지 않았다"
    );
    assert_eq!(
        shipping_id("id = \"com.tasty.x\"\n\n[entry]\nbundle = false\n"),
        Ok(Some("com.tasty.x".to_string())),
        "[entry] 아래의 bundle = false 를 최상위 배포 제외로 읽었다"
    );
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
    assert_some_entries_are_bundled();
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
            "{platform}: scripts/lib/bundled-plugins.sh 의 배포 fetch 가 그 플랫폼으로 --all·--lock 없이 받지 않는다: {:?}",
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
            "{platform}: scripts/stage-dev-plugins.sh 기본 분기가 그 플랫폼으로 --lock 없이 --all 받지 않는다: {:?}",
            staging[0]
        );
    }
}

#[test]
fn the_windows_package_fetches_only_bundled_entries() {
    assert_some_entries_are_bundled();
    let argv = windows_fetch_argv();
    let p = parse_fetch(&argv);
    assert!(
        really_fetches(&p) && !p.all,
        "scripts/build-windows.ps1 의 배포 fetch 가 --all·--lock 없이 --platform·--out 으로 받지 않는다: {argv:?}"
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
    // 함께 띄운 두 호출은 각자 기록된다(하나가 다른 하나를 덮지 않는다). 그래도 실패를 전파하지 않으므로 거부한다.
    let plain = real.replacen(" --all", "", 1);
    let locked: Vec<String> = locked_ids(&lock()).into_iter().collect();
    let together = format!("{real} &\n{plain} &\nwait");
    let (rc, calls) = fetch_collect(
        "staging",
        "linux-x64",
        &together,
        "",
        &locked,
        &bundled_ids(),
    );
    assert_eq!(
        rc, "0",
        "함께 띄운 두 호출의 성공 실행이 실패했다: {calls:?}"
    );
    assert_eq!(
        calls.len(),
        2,
        "동시 호출 두 건을 모두 기록하지 않았다: {calls:?}"
    );
    assert_eq!(
        calls.iter().filter(|c| parse_fetch(c).all).count(),
        1,
        "동시 호출 중 --all 호출이 하나가 아니다: {calls:?}"
    );
    // fetch 실패를 전파하지 않는 꼴은 띄운 시점이나 표준 입력과 관계없이 실패다.
    for (what, branch) in [
        ("함께 띄우고 기다린 두 호출", together.clone()),
        (
            "띄운 뒤 끝날 때까지 쉰 호출",
            format!("{real} &\nsleep 0.6"),
        ),
        (
            "표준 입력을 넘겨 띄운 호출",
            format!("exec 3<&0\n{real} <&3 &\nwait"),
        ),
        ("실패를 삼킨 호출", format!("{real} || true")),
        (
            "실패를 삼키고 산출물을 지운 호출",
            format!("{real} || rm -rf \"$stage\""),
        ),
        // 아래 둘은 성공 실행에서 하네스 검사까지 가서 RC 0을 얻고, 실패 주입 중에서는 각각 late1·late23에서만 하네스
        // 검사까지 가므로 그 실패 주입의 표지 판정을 따로 지킨다.
        (
            "산출물을 남긴 종료 1만 삼키고 산출물을 지운 호출",
            format!(
                "{real} || {{ s=$?; if [ \"$s\" -eq 1 ] && [ -f \"$stage/.fetched\" ]; then rm -rf \"$stage\"; else exit \"$s\"; fi; }}"
            ),
        ),
        (
            "산출물을 남긴 종료 23만 삼키고 산출물을 지운 호출",
            format!(
                "{real} || {{ s=$?; if [ \"$s\" -eq 23 ] && [ -f \"$stage/.fetched\" ]; then rm -rf \"$stage\"; else exit \"$s\"; fi; }}"
            ),
        ),
        (
            "종료 1만 삼키는 호출",
            format!("{real} &\npid=$!\nwait \"$pid\" || [ \"$?\" -eq 1 ]"),
        ),
        ("실패 때만 끝내는 호출", format!("{real} || exit 0")),
        (
            "실제 산출물이 있으면 실패를 삼키는 호출",
            // 잠금의 첫 항목(픽스처에 매니페스트가 있다)으로 고정해 잠금 구성과 관계없이 매니페스트를 본다.
            format!(
                "{real} || [ -f \"$stage/{}/tasty-plugin.toml\" ]",
                locked[0]
            ),
        ),
        (
            "검사 전에 끝내는 분기",
            format!("{real} &\nwait \"$!\"\nexit 0"),
        ),
    ] {
        assert!(
            fetch_run("staging", "linux-x64", &branch).is_err(),
            "{what}: fetch 실패를 전파하지 않는 분기를 통과시켰다"
        );
    }
    // 대상이 끝난 뒤 시작하는 호출도 fd 9 파이프가 닫힐 때까지 기다려 모은다.
    for (what, branch) in [
        (
            "정상 호출 뒤 늦게 띄운 호출",
            format!("{real}\n(sleep 0.5; {plain}) &"),
        ),
        (
            "손자로 늦게 띄운 호출",
            format!("{real}\n( (sleep 0.5; {plain}) & )"),
        ),
        (
            "출력을 모두 돌리고 늦게 띄운 호출",
            format!("{real}\n(sleep 0.4; {plain}) > /dev/null 2>&1 &"),
        ),
        (
            "fd 9만 돌리고 늦게 띄운 호출",
            format!("{real}\n(sleep 0.3; {plain}) 9> /dev/null &"),
        ),
    ] {
        let (_, calls) =
            fetch_collect("staging", "linux-x64", &branch, "", &locked, &bundled_ids());
        assert_eq!(
            calls.len(),
            2,
            "{what}: 늦게 시작한 호출을 모으지 못했다: {calls:?}"
        );
    }
}

/// 스텁의 산출물 단계 실패는 그 호출의 종료값이다. 대상이 인자 없는 `wait`으로 그 값을 삼키고 다른 호출이
/// `.fetched`를 남겨도 하네스가 실패로 낸다.
#[cfg(unix)]
#[test]
fn a_stub_copy_failure_fails_the_harness() {
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\" --all";
    let plain = real.replacen(" --all", "", 1);
    let locked: Vec<String> = locked_ids(&lock()).into_iter().collect();
    let together = format!("{real} &\n{plain} &\nwait");
    for (what, branch, fail) in [
        (
            "함께 띄운 두 호출 중 뒤의 복사 실패",
            together.as_str(),
            "copy2",
        ),
        ("앞에서 부른 호출의 복사 실패", real, "copy1"),
    ] {
        let out = fetch_harness(
            "staging",
            "linux-x64",
            branch,
            fail,
            &locked,
            &bundled_ids(),
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{what}: 하네스가 성공했다: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains("산출물 단계가 실패했다"),
            "{what}: 산출물 실패를 알리지 않았다: {stderr}"
        );
    }
    // 스텁을 직접 부르면 주입한 복사 실패의 종료값이 그대로 나온다(통지만 남기고 계속 가지 않는다).
    for (fail, want) in [("", "0"), ("copy1", "1"), ("copy1 37", "37")] {
        let out = fetch_harness("stub", "linux-x64", "", fail, &locked, &bundled_ids());
        assert!(
            out.status.success(),
            "스텁 직접 호출 하네스가 실패했다: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let (rc, _) = parse_harness(out.stdout);
        assert_eq!(rc, want, "복사 실패 {fail:?}의 스텁 종료값");
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
    // 스텁 산출물은 실제 fetch처럼 --all 유무에 따라 다르다. 실제 잠금의 구성에 기대지 않도록 배포 항목과 개발 전용
    // 항목을 하나씩 가진 합성 집합으로 잰다. 배포 집합에 없는 항목을 요구하면 실패한다.
    let bundled = vec!["com.tasty.synthetic-bundled".to_string()];
    let dev_only = "com.tasty.synthetic-dev".to_string();
    let all = vec![bundled[0].clone(), dev_only.clone()];
    // 헬퍼 함수가 fetch를 마친 뒤 그 항목의 폴더를 요구하도록 감싼다.
    let requires = |id: &str| {
        format!(
            "{helper}\neval \"orig_$(declare -f fetch_bundled_plugins)\"\nfetch_bundled_plugins() {{ orig_fetch_bundled_plugins \"$@\"; [ -d \"$BUNDLED_PLUGINS_DIR/{id}\" ]; }}"
        )
    };
    assert!(
        fetch_run_with("package", "linux-x64", &requires(&dev_only), &all, &bundled).is_err(),
        "배포 산출물에 개발 전용 {dev_only}가 있다고 보았다"
    );
    assert!(
        fetch_run_with(
            "package",
            "linux-x64",
            &requires(&bundled[0]),
            &all,
            &bundled
        )
        .is_ok(),
        "배포 산출물에 {}가 없다고 보았다",
        bundled[0]
    );
    // 값 옵션이 소비한 --all은 전체 받기가 아니므로 산출물도 배포 집합이다.
    let real = "bash \"$root/scripts/fetch-official-plugins.sh\" --platform \"$platform\" --out \"$stage\"";
    assert!(
        fetch_run_with(
            "staging",
            "linux-x64",
            &format!("{real} --cache --all\n[ ! -d \"$stage/{dev_only}\" ]"),
            &all,
            &bundled
        )
        .is_ok(),
        "--cache 값으로 소비된 --all 에 개발 전용 {dev_only}를 내놓았다"
    );
    assert!(
        fetch_run_with(
            "staging",
            "linux-x64",
            &format!("{real} --all\n[ -d \"$stage/{dev_only}\" ]"),
            &all,
            &bundled
        )
        .is_ok(),
        "--all 에 개발 전용 {dev_only}를 내놓지 않았다"
    );
    // 배포 항목이 하나도 없으면 실제 fetch처럼 배포 받기는 실패하고, --all은 모든 항목을 내놓는다.
    assert!(
        fetch_run_with("package", "linux-x64", &helper, &all, &[]).is_err(),
        "배포 항목이 없는 받기를 성공으로 보았다"
    );
    let all_present: String = all
        .iter()
        .map(|id| format!("\n[ -d \"$stage/{id}\" ]"))
        .collect();
    assert!(
        fetch_run_with(
            "staging",
            "linux-x64",
            &format!("{real} --all{all_present}"),
            &all,
            &[]
        )
        .is_ok(),
        "배포 항목이 없을 때 --all 이 모든 항목을 내놓지 않았다"
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
        "--platform p --out o --lock target/official-plugins.lock.toml",
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
