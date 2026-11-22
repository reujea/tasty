//! `scripts/sync-official-plugin-fixtures.sh`의 갱신·대조를 격리한 작업 루트에서 확인한다.
//! 작업 루트에 스크립트·스캐너·합성 잠금 파일을 복사하고, `scripts/fetch-official-plugins.sh` 자리에
//! 합성 산출물을 내는 스텁을 둔다. 스크립트는 늘 자기 루트의 fetch를 부르므로 운영 경로와 같은 갱신 경로를 지난다.
//! Bash를 쓰는 Unix 전용 시험이다. Windows에서는 cfg로 빠지므로 컴파일 성공을 실행 통과로 보지 않는다.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tasty_doc_guards::floored_walk::{CountedOn, Descend, Floor, walk_with_floor};

const FIXTURES: &str = "crates/tasty-test-support/fixtures/official-plugins";

/// 합성 픽스처 순회의 하한. 폴더가 있으면 합성 플러그인마다 매니페스트와 lang 파일이 있다.
const SYNTHETIC_FIXTURE_FLOOR: Floor = Floor {
    min: 1,
    measured: 5,
    measured_on: "2026-11-21",
    counted_on: CountedOn::SyntheticTree,
    why_this_gap: "합성 산출물은 alpha의 매니페스트·en·ko와 beta의 매니페스트·en으로 5파일이다. 일부 시험은 파일을 바꾸거나 지운 뒤 다시 세므로 하나 이상이면 수집이 동작한 것으로 본다.",
};

const LOCK: &str = "[release]\ntag = \"v9.9.9\"\n\n\
                    [[plugin]]\nid = \"com.tasty.alpha\"\nversion = \"1.0.0\"\nbundle = true\n\n\
                    [[plugin]]\nid = \"com.tasty.beta\"\nversion = \"2.0.0\"\nbundle = false\n";

/// 받은 인자와 전용 캐시의 파일 목록을 기록하고, payload 폴더를 출력 폴더로 복사한다.
/// `mutate-lock` 표지가 있으면 받는 도중 잠금 파일을 고친다.
const FETCH_STUB: &str = r##"#!/usr/bin/env bash
root="$(cd "$(dirname "$0")/.." && pwd)"
echo "ARGS $*" >> "$root/fetch.log"
out=""; cache=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) out="$2"; shift 2 ;;
        --cache) cache="$2"; shift 2 ;;
        *) shift ;;
    esac
done
if [[ -n "$cache" ]]; then
    for f in "$cache"/*; do [[ -e "$f" ]] && echo "CACHED $(basename "$f")" >> "$root/fetch.log"; done
fi
cp -R "$root/payload/." "$out/"
if [[ -f "$root/mutate-lock" ]]; then
    echo "# changed while fetching" >> "$root/official-plugins.lock.toml"
fi
"##;

struct Root {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Root {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("임시 작업 루트");
        let path = dir.path().to_path_buf();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        fs::create_dir_all(path.join("scripts/lib")).expect("scripts 폴더");
        for rel in [
            "scripts/sync-official-plugin-fixtures.sh",
            "scripts/lib/toml-scan.awk",
            "scripts/lib/run-lock.sh",
        ] {
            fs::copy(repo.join(rel), path.join(rel)).unwrap_or_else(|e| panic!("{rel} 복사: {e}"));
        }
        tasty_test_support::write_executable(
            &path.join("scripts/fetch-official-plugins.sh"),
            FETCH_STUB,
        )
        .expect("fetch 스텁");
        let root = Self { _dir: dir, path };
        root.write("official-plugins.lock.toml", LOCK);
        root.write(
            "payload/com.tasty.alpha/tasty-plugin.toml",
            "id = \"com.tasty.alpha\"\nversion = \"1.0.0\"\n",
        );
        root.write("payload/com.tasty.alpha/lang/en.toml", "a = \"b\"\n");
        root.write("payload/com.tasty.alpha/lang/ko.toml", "a = \"나\"\n");
        root.write(
            "payload/com.tasty.beta/tasty-plugin.toml",
            "id = \"com.tasty.beta\"\nversion = \"2.0.0\"\nbundle = false\n",
        );
        root.write("payload/com.tasty.beta/lang/en.toml", "c = \"d\"\n");
        root
    }

    fn write(&self, rel: &str, body: &str) {
        let p = self.path.join(rel);
        fs::create_dir_all(p.parent().expect("부모 경로")).expect("폴더 생성");
        fs::write(&p, body).unwrap_or_else(|e| panic!("{rel} 쓰기: {e}"));
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        self.run_with_env(args, &[])
    }

    fn run_with_env(&self, args: &[&str], envs: &[(&str, std::ffi::OsString)]) -> (i32, String) {
        let out = Command::new("bash")
            .arg(self.path.join("scripts/sync-official-plugin-fixtures.sh"))
            .args(args)
            .current_dir(&self.path)
            .env_remove("CARGO_TARGET_DIR")
            .envs(envs.iter().map(|(k, v)| (*k, v)))
            .output()
            .expect("sync 스크립트 실행");
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    fn fetch_log(&self) -> String {
        fs::read_to_string(self.path.join("fetch.log")).unwrap_or_default()
    }

    /// 픽스처 트리의 (상대 경로, 바이트) 목록. 없으면 빈 목록이다.
    fn fixtures(&self) -> Vec<(String, Vec<u8>)> {
        let base = self.path.join(FIXTURES);
        if !base.is_dir() {
            return Vec::new();
        }
        walk_with_floor(
            &base,
            &base,
            &SYNTHETIC_FIXTURE_FLOOR,
            Descend::Everything,
            &|_| true,
        )
        .unwrap_or_else(|why| panic!("{why}"))
        .into_iter()
        .map(|w| {
            let bytes = fs::read(&w.path).expect("픽스처 읽기");
            (w.rel, bytes)
        })
        .collect()
    }

    fn update_ok(&self) {
        let (code, text) = self.run(&["--update"]);
        assert_eq!(code, 0, "--update 가 실패했다:\n{text}");
    }
}

#[test]
fn update_writes_the_artifact_files_and_check_accepts_them() {
    let root = Root::new();
    root.update_ok();
    let files: Vec<String> = root.fixtures().into_iter().map(|(rel, _)| rel).collect();
    assert_eq!(
        files,
        [
            "com.tasty.alpha/lang/en.toml",
            "com.tasty.alpha/lang/ko.toml",
            "com.tasty.alpha/tasty-plugin.toml",
            "com.tasty.beta/lang/en.toml",
            "com.tasty.beta/tasty-plugin.toml",
        ],
        "픽스처 파일 집합이 산출물의 매니페스트·lang 과 다르다"
    );
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 0, "갓 갱신한 픽스처를 --check 가 거부했다:\n{text}");
    let log = root.fetch_log();
    for want in ["--platform ", "--all", "--lock ", "--out ", "--cache "] {
        assert!(
            log.contains(want),
            "fetch 에 {want} 를 넘기지 않았다:\n{log}"
        );
    }
    assert!(
        log.contains("/target/official-plugin-fixtures-sync/run."),
        "잠금 사본·출력·캐시가 target 아래 실행 폴더에 있지 않다:\n{log}"
    );
}

#[test]
fn crlf_artifacts_are_rejected_and_the_fixtures_kept() {
    let root = Root::new();
    root.update_ok();
    let before = root.fixtures();
    root.write("payload/com.tasty.alpha/lang/en.toml", "a = \"b\"\r\n");
    let (code, text) = root.run(&["--update"]);
    assert_eq!(code, 1, "CR 이 든 산출물을 받아들였다:\n{text}");
    assert!(text.contains("CR bytes"), "CR 진단이 없다:\n{text}");
    assert_eq!(
        root.fixtures(),
        before,
        "실패한 갱신이 기존 픽스처를 바꿨다"
    );
}

#[test]
fn check_finds_a_changed_byte_and_a_stale_fixture() {
    let root = Root::new();
    root.update_ok();
    root.write(
        &format!("{FIXTURES}/com.tasty.alpha/lang/en.toml"),
        "a = \"x\"\n",
    );
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "바뀐 픽스처 바이트를 놓쳤다:\n{text}");
    assert!(
        text.contains("com.tasty.alpha/lang/en.toml"),
        "어느 파일인지 말하지 않는다:\n{text}"
    );

    root.update_ok();
    root.write(
        &format!("{FIXTURES}/com.tasty.stale/tasty-plugin.toml"),
        "id = \"x\"\n",
    );
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "잠금 파일에 없는 픽스처를 놓쳤다:\n{text}");
    assert!(
        text.contains("com.tasty.stale"),
        "옛 픽스처를 말하지 않는다:\n{text}"
    );
    root.update_ok();
    assert!(
        !root.path.join(FIXTURES).join("com.tasty.stale").exists(),
        "--update 가 잠금 파일에 없는 픽스처를 남겼다"
    );

    // 점으로 시작하는 폴더도 픽스처 폴더 집합에 든다(오프라인 가드와 같은 집합).
    fs::create_dir_all(root.path.join(FIXTURES).join(".stale")).expect(".stale 폴더");
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "점으로 시작하는 옛 폴더를 놓쳤다:\n{text}");
    assert!(
        text.contains("not in the lock file: .stale"),
        "점 폴더를 말하지 않는다:\n{text}"
    );
    fs::remove_dir(root.path.join(FIXTURES).join(".stale")).expect(".stale 지우기");

    // 폴더를 가리키는 링크는 링크 너머를 읽지 않고 오류다.
    let elsewhere = root.path.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("링크 대상");
    std::os::unix::fs::symlink(
        &elsewhere,
        root.path.join(FIXTURES).join("com.tasty.linked"),
    )
    .expect("링크 만들기");
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "픽스처 루트의 폴더 링크를 놓쳤다:\n{text}");
    assert!(
        text.contains("symbolic link in the fixture root: com.tasty.linked"),
        "링크를 말하지 않는다:\n{text}"
    );
    fs::remove_file(root.path.join(FIXTURES).join("com.tasty.linked")).expect("링크 지우기");

    // 잠금 id 자리가 같은 내용을 가리키는 링크여도 따라가 대조하지 않는다.
    let fixtures = root.path.join(FIXTURES);
    let real_alpha = root.path.join("real-alpha");
    fs::rename(fixtures.join("com.tasty.alpha"), &real_alpha).expect("alpha 옮기기");
    std::os::unix::fs::symlink(&real_alpha, fixtures.join("com.tasty.alpha")).expect("alpha 링크");
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "잠금 id 자리의 링크를 따라가 대조했다:\n{text}");
    assert!(
        text.contains("symbolic link in the fixture root: com.tasty.alpha"),
        "잠금 id 자리의 링크를 말하지 않는다:\n{text}"
    );
    fs::remove_file(fixtures.join("com.tasty.alpha")).expect("alpha 링크 지우기");
    fs::rename(&real_alpha, fixtures.join("com.tasty.alpha")).expect("alpha 되돌리기");

    // 픽스처 루트 자체가 링크여도 따라가지 않는다. 링크 너머에 CR 파일을 두어, CR 검사가 링크를 따라
    // 읽기 전에 멈추는지도 본다.
    let real_root = root.path.join("real-fixtures");
    fs::rename(&fixtures, &real_root).expect("루트 옮기기");
    std::os::unix::fs::symlink(&real_root, &fixtures).expect("루트 링크");
    fs::write(real_root.join("com.tasty.alpha/cr.txt"), "a\r\n").expect("CR 파일");
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "링크인 픽스처 루트를 따라가 대조했다:\n{text}");
    assert!(
        text.contains("the fixture root is a symbolic link") && !text.contains("CR bytes"),
        "CR 검사보다 먼저 루트 링크를 말하지 않는다:\n{text}"
    );
}

/// 잠금 파일은 fetch 전에 checker와 같은 조건으로 읽는다. 배포 파서는 `[[plugin.artifact]]` 를 앞선
/// `[[plugin]]` 에 붙여 읽고, id는 공식 id 꼴이며 겹치지 않아야 한다. 어기면 두 모드 모두 받기 전에 멈춘다.
#[test]
fn a_lock_file_the_release_tools_reject_stops_before_fetching() {
    let dup = LOCK.replace("com.tasty.beta", "com.tasty.alpha");
    for (what, lock, why) in [
        (
            "앞선 [[plugin]] 없는 artifact",
            format!("[[plugin.artifact]]\nplatform = \"x\"\n{LOCK}"),
            "[[plugin.artifact]] before any [[plugin]] table",
        ),
        (
            "공식 id 꼴이 아닌 id",
            LOCK.replace("com.tasty.beta", "com.example.demo"),
            "id is not an official plugin id: com.example.demo",
        ),
        ("중복 id", dup, "duplicate id com.tasty.alpha"),
        (
            "두 번 나온 [release]",
            format!("{LOCK}\n[release]\ntag = \"v1.0.0\"\n"),
            "[release] table appears twice",
        ),
    ] {
        let root = Root::new();
        root.write("official-plugins.lock.toml", &lock);
        for mode in ["--update", "--check"] {
            let (code, text) = root.run(&[mode]);
            assert_eq!(code, 2, "{what} {mode}: 잠금 파일을 받았다:\n{text}");
            assert!(
                text.contains(why),
                "{what} {mode}: 이유를 말하지 않는다:\n{text}"
            );
        }
        assert!(
            root.fetch_log().is_empty(),
            "{what}: 잘못된 잠금 파일로 fetch 를 불렀다"
        );
    }
}

#[test]
fn a_lock_file_changed_while_fetching_fails_both_modes() {
    let root = Root::new();
    root.update_ok();
    let before = root.fixtures();
    root.write("mutate-lock", "");
    let (code, text) = root.run(&["--update"]);
    assert_eq!(code, 1, "받는 동안 바뀐 잠금 파일로 갱신했다:\n{text}");
    assert_eq!(
        root.fixtures(),
        before,
        "실패한 갱신이 기존 픽스처를 바꿨다"
    );
    root.write("official-plugins.lock.toml", LOCK);
    let (code, text) = root.run(&["--check"]);
    assert_eq!(
        code, 1,
        "받는 동안 바뀐 잠금 파일로 대조를 통과시켰다:\n{text}"
    );
    assert!(
        text.contains("changed while"),
        "잠금 파일 변경을 말하지 않는다:\n{text}"
    );
}

#[test]
fn a_candidate_failing_the_offline_checks_keeps_the_fixtures() {
    let root = Root::new();
    root.update_ok();
    let before = root.fixtures();

    root.write(
        "payload/com.tasty.beta/tasty-plugin.toml",
        "id = \"com.tasty.beta\"\nversion = \"2.0.0\"\n",
    );
    let (code, text) = root.run(&["--update"]);
    assert_eq!(code, 1, "잠금 파일과 bundle 이 다른 후보를 들였다:\n{text}");
    assert!(
        text.contains("bundle"),
        "bundle 차이를 말하지 않는다:\n{text}"
    );
    assert_eq!(
        root.fixtures(),
        before,
        "실패한 갱신이 기존 픽스처를 바꿨다"
    );

    root.write(
        "payload/com.tasty.beta/tasty-plugin.toml",
        "id = \"com.tasty.beta\"\nversion = \"2.0.0\"\nbundle = false\n",
    );
    fs::remove_file(root.path.join("payload/com.tasty.alpha/lang/en.toml")).expect("en 삭제");
    let (code, text) = root.run(&["--update"]);
    assert_eq!(code, 1, "lang/en.toml 이 없는 후보를 들였다:\n{text}");
    assert!(
        text.contains("lang/en.toml"),
        "빠진 파일을 말하지 않는다:\n{text}"
    );
    assert_eq!(
        root.fixtures(),
        before,
        "실패한 갱신이 기존 픽스처를 바꿨다"
    );
    root.write("payload/com.tasty.alpha/lang/en.toml", "a = \"b\"\n");

    // 오프라인 가드와 같은 판독: 읽지 못한 키는 뒤의 bundle 과 관계없이 실패, version 은 여러 줄 문자열 밖의 값.
    for (what, beta) in [
        (
            "읽지 못한 키 뒤의 bundle",
            "\"\\u0066uture\" = \"x\"\nid = \"com.tasty.beta\"\nversion = \"2.0.0\"\nbundle = false\n",
        ),
        (
            "bundle 뒤의 읽지 못한 키",
            "id = \"com.tasty.beta\"\nversion = \"2.0.0\"\nbundle = false\n\"\\u0066uture\" = \"x\"\n",
        ),
        (
            "중복 version, 잠금 값이 마지막",
            "id = \"com.tasty.beta\"\nversion = \"2.0.9\"\nversion = \"2.0.0\"\nbundle = false\n",
        ),
        (
            "여러 줄 문자열 안의 가짜 version",
            "id = \"com.tasty.beta\"\nnote = \"\"\"\nversion = \"2.0.0\"\n\"\"\"\nversion = \"2.0.9\"\nbundle = false\n",
        ),
    ] {
        root.write("payload/com.tasty.beta/tasty-plugin.toml", beta);
        let (code, text) = root.run(&["--update"]);
        assert_eq!(code, 1, "{what}: 후보를 들였다:\n{text}");
        assert_eq!(
            root.fixtures(),
            before,
            "{what}: 실패한 갱신이 기존 픽스처를 바꿨다"
        );
    }
}

/// 잠금 값이 `true`인 alpha에서 `bundle`을 표로 만든다. 표를 기본값 `true`로 읽으면 값 대조가 통과하므로
/// 판독 오류로 실패하는지 본다. 점이 든 따옴표 조각은 그 조각 하나가 첫 키다.
#[test]
fn a_bundle_table_fails_even_where_the_lock_value_is_true() {
    let root = Root::new();
    root.update_ok();
    let before = root.fixtures();
    let head = "id = \"com.tasty.alpha\"\nversion = \"1.0.0\"\n";
    for (what, tail) in [
        ("점 구분 키", "bundle.enabled = false\n"),
        (
            "뒤쪽 표",
            "[entry]\ntype = \"process\"\n[bundle]\nenabled = false\n",
        ),
        (
            "점이 든 따옴표 조각의 표",
            "[bundle.\"x.y\"]\nenabled = false\n",
        ),
        (
            "이스케이프로 쓴 표 이름",
            "[\"bu\\u006edle\"]\nenabled = false\n",
        ),
    ] {
        root.write(
            "payload/com.tasty.alpha/tasty-plugin.toml",
            &format!("{head}{tail}"),
        );
        let (code, text) = root.run(&["--update"]);
        assert_eq!(code, 1, "{what}: 표가 된 bundle 을 들였다:\n{text}");
        assert!(
            text.contains("com.tasty.alpha: tasty-plugin.toml has a top-level key it cannot read"),
            "{what}: 판독 오류로 실패하지 않았다:\n{text}"
        );
        assert_eq!(
            root.fixtures(),
            before,
            "{what}: 실패한 갱신이 기존 픽스처를 바꿨다"
        );
    }
    for (what, tail) in [
        (
            "따옴표 조각 bundle.other",
            "\"bundle.other\".enabled = false\n",
        ),
        ("빈 인용 키 표", "[\"\"]\nenabled = false\n"),
    ] {
        root.write(
            "payload/com.tasty.alpha/tasty-plugin.toml",
            &format!("{head}{tail}"),
        );
        let (code, text) = root.run(&["--update"]);
        assert_eq!(
            code, 0,
            "{what}: bundle 이 아닌 표를 bundle 표로 보았다:\n{text}"
        );
    }
}

/// 기록된 fetch 인자에서 `opt` 다음 값을 읽는다.
fn arg_after(log: &str, opt: &str) -> String {
    log.split_whitespace()
        .skip_while(|w| *w != opt)
        .nth(1)
        .unwrap_or_else(|| panic!("fetch 에 {opt} 를 넘기지 않았다:\n{log}"))
        .to_owned()
}

#[test]
fn shared_cache_seeds_a_private_copy_and_is_off_by_default() {
    let root = Root::new();
    root.update_ok();
    // 스크립트가 고른 플랫폼을 첫 실행의 인자에서 읽어 그 자리의 기본 캐시를 채운다.
    let platform = arg_after(&root.fetch_log(), "--platform");
    let shared = format!("target/official-plugins/v9.9.9/{platform}");
    root.write(&format!("{shared}/seeded.tar.gz"), "x");
    root.write(&format!("{shared}/partial.tar.gz.part"), "x");
    // 실행마다 기록을 비워 그 실행이 받은 인자와 캐시만 본다.
    let fresh_log = |args: &[&str]| -> String {
        let log = root.path.join("fetch.log");
        if log.exists() {
            fs::remove_file(&log).expect("fetch 기록 지우기");
        }
        let (code, text) = root.run(args);
        assert_eq!(code, 0, "{args:?} 대조가 실패했다:\n{text}");
        root.fetch_log()
    };

    let off = fresh_log(&["--check"]);
    assert!(
        !off.contains("CACHED seeded.tar.gz"),
        "--shared-cache 없이 기본 캐시를 썼다:\n{off}"
    );
    let on = fresh_log(&["--check", "--shared-cache"]);
    assert!(
        on.contains("CACHED seeded.tar.gz"),
        "기본 캐시 파일을 전용 캐시로 옮기지 않았다:\n{on}"
    );
    assert!(
        !on.contains("CACHED partial"),
        "받다 만 .part 파일까지 옮겼다:\n{on}"
    );
    let cache = arg_after(&on, "--cache");
    assert!(
        !Path::new(&cache).starts_with(root.path.join("target/official-plugins")),
        "fetch 에 기본 캐시를 직접 넘겼다: {cache}"
    );
    // 다 받은 같은 이름의 파일은 옮긴다. 위 부재 확인이 이름 때문에 통과한 것이 아니다.
    root.write(&format!("{shared}/partial.tar.gz"), "x");
    let done = fresh_log(&["--check", "--shared-cache"]);
    assert!(
        done.contains("CACHED partial"),
        "다 받은 캐시 파일을 옮기지 않았다:\n{done}"
    );
}

/// 플러그인 폴더 안의 링크는 파일 목록에서 빠지므로 대조 전에 거부한다. 폴더를 나열하지 못하면
/// 부분 목록으로 대조하지 않고 실패한다.
#[test]
fn links_and_unlistable_folders_inside_a_fixture_fail_the_check() {
    use std::os::unix::fs::PermissionsExt;
    let root = Root::new();
    root.update_ok();
    let alpha = root.path.join(FIXTURES).join("com.tasty.alpha");

    let target = root.path.join("extra.toml");
    fs::write(&target, "e = \"f\"\n").expect("링크 대상");
    std::os::unix::fs::symlink(&target, alpha.join("lang/extra.toml")).expect("링크 만들기");
    let (code, text) = root.run(&["--check"]);
    assert_eq!(code, 1, "플러그인 폴더 안의 링크를 놓쳤다:\n{text}");
    assert!(
        text.contains("symbolic link inside the fixture: com.tasty.alpha"),
        "폴더 안의 링크를 말하지 않는다:\n{text}"
    );
    fs::remove_file(alpha.join("lang/extra.toml")).expect("링크 지우기");
    root.update_ok();

    // root 는 권한을 무시해 폴더를 읽으므로 나열 실패를 재현할 수 없다.
    // SAFETY: geteuid 는 인자 없이 현재 프로세스의 유효 사용자 ID 만 읽는다.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let locked = alpha.join("lang/locked");
    fs::create_dir_all(&locked).expect("읽기 막을 폴더");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("권한 막기");
    let (code, text) = root.run(&["--check"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("권한 되돌리기");
    assert_eq!(code, 1, "나열하지 못한 폴더로 대조를 통과했다:\n{text}");
    assert!(
        text.contains("could not list") && text.contains("com.tasty.alpha"),
        "나열 실패를 말하지 않는다:\n{text}"
    );
}

/// 받은 쪽(`find lang -type f`)이나 픽스처 쪽(`find . -type f`)의 파일 목록 명령만 실패시키는 `find` 스텁.
/// 다른 호출(링크 탐색 등)은 진짜 `find`로 넘긴다.
const FIND_STUB: &str = r##"#!/usr/bin/env bash
if [[ "${2:-} ${3:-}" == "-type f" ]]; then
    if [[ "$FIND_FAIL" == want && "$1" == lang ]] || [[ "$FIND_FAIL" == have && "$1" == . ]]; then
        echo "find stub: refused $*" >&2
        exit 1
    fi
fi
exec "$REAL_FIND" "$@"
"##;

/// 두 파일 목록 명령의 실패를 각각 따로 본다. 앞선 링크 탐색은 통과하므로 그 검사가 덮지 않는다.
#[test]
fn each_file_listing_failure_fails_the_check_on_its_own() {
    let root = Root::new();
    root.update_ok();
    let real_find = std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .map(|d| d.join("find"))
        .find(|p| {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .expect("PATH 에 실행 가능한 find 가 없다");
    let bin = root.path.join("stub-bin");
    fs::create_dir_all(&bin).expect("스텁 폴더");
    tasty_test_support::write_executable(&bin.join("find"), FIND_STUB).expect("find 스텁");
    let mut path = std::ffi::OsString::from(&bin);
    path.push(":");
    path.push(std::env::var_os("PATH").expect("PATH"));

    let (code, text) = root.run_with_env(
        &["--check"],
        &[
            ("PATH", path.clone()),
            ("REAL_FIND", real_find.clone().into()),
            ("FIND_FAIL", "none".into()),
        ],
    );
    assert_eq!(
        code, 0,
        "스텁이 실패시키지 않을 때 대조가 통과하지 않는다:\n{text}"
    );

    for (side, said) in [
        (
            "want",
            "could not list the fetched files of com.tasty.alpha",
        ),
        (
            "have",
            "could not list the fixture files of com.tasty.alpha",
        ),
    ] {
        let (code, text) = root.run_with_env(
            &["--check"],
            &[
                ("PATH", path.clone()),
                ("REAL_FIND", real_find.clone().into()),
                ("FIND_FAIL", side.into()),
            ],
        );
        assert_eq!(
            code, 1,
            "{side} 쪽 파일 목록 실패로 대조를 통과했다:\n{text}"
        );
        assert!(
            text.contains(said),
            "{side} 쪽 나열 실패를 말하지 않는다:\n{text}"
        );
    }
}

/// 다른 실행이 동시 실행 잠금을 쥐고 있으면 두 모드 모두 받기 전에 기존 두 줄로 거부하고 종료 2다.
/// 남의 잠금은 그대로 둔다.
#[test]
fn a_held_run_lock_refuses_both_modes_before_fetching() {
    let root = Root::new();
    let rel = "target/official-plugin-fixtures-sync.lock";
    root.write(rel, "other\n");
    let lock = root.path.join(rel);
    for mode in ["--update", "--check"] {
        let (code, text) = root.run(&[mode]);
        assert_eq!(code, 2, "{mode}: 잠금이 있는데 진행했다:\n{text}");
        for line in [
            format!(
                "Error: another run is syncing the fixtures ({} exists).",
                lock.display()
            ),
            format!(
                "  Wait for it to finish and run this again. If no run is in progress, remove {}.",
                lock.display()
            ),
        ] {
            assert!(
                text.lines().any(|l| l == line),
                "{mode}: 안내 줄이 없다: {line}\n{text}"
            );
        }
        assert_eq!(
            fs::read_to_string(&lock).expect("잠금 읽기"),
            "other\n",
            "{mode}: 남의 잠금을 바꿨다"
        );
    }
    assert!(root.fetch_log().is_empty(), "잠금이 있는데 fetch 를 불렀다");
}
