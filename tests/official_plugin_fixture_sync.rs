//! `scripts/sync-official-plugin-fixtures.sh`의 갱신·대조를 격리한 작업 루트에서 확인한다.
//! 작업 루트에 스크립트·스캐너·합성 잠금 파일을 복사하고, `scripts/fetch-official-plugins.sh` 자리에
//! 합성 산출물을 내는 스텁을 둔다. 스크립트는 늘 자기 루트의 fetch를 부르므로 운영 경로와 같은 갱신 경로를 지난다.
//! Bash를 쓰는 Unix 전용 시험이다. Windows에서는 cfg로 빠지므로 컴파일 성공을 실행 통과로 보지 않는다.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURES: &str = "crates/tasty-test-support/fixtures/official-plugins";

const LOCK: &str = "[release]\ntag = \"v9.9.9\"\n\n\
                    [[plugin]]\nid = \"com.tasty.alpha\"\nversion = \"1.0.0\"\n\n\
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
        ] {
            fs::copy(repo.join(rel), path.join(rel)).unwrap_or_else(|e| panic!("{rel} 복사: {e}"));
        }
        tasty_test_support::write_executable(
            &path.join("scripts/fetch-official-plugins.sh"),
            FETCH_STUB.to_owned(),
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
        let out = Command::new("bash")
            .arg(self.path.join("scripts/sync-official-plugin-fixtures.sh"))
            .args(args)
            .current_dir(&self.path)
            .env_remove("CARGO_TARGET_DIR")
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
        let mut out = Vec::new();
        collect(&base, &base, &mut out);
        out.sort();
        out
    }

    fn update_ok(&self) {
        let (code, text) = self.run(&["--update"]);
        assert_eq!(code, 0, "--update 가 실패했다:\n{text}");
    }
}

fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let p = entry.expect("폴더 항목").path();
        if p.is_dir() {
            collect(&p, base, out);
        } else {
            let rel = p
                .strip_prefix(base)
                .expect("상대 경로")
                .to_string_lossy()
                .into_owned();
            out.push((rel, fs::read(&p).expect("픽스처 읽기")));
        }
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
}

#[test]
fn shared_cache_seeds_a_private_copy_and_is_off_by_default() {
    let root = Root::new();
    root.update_ok();
    let first = root.fetch_log();
    assert!(
        !first.contains("CACHED"),
        "--shared-cache 없이 기본 캐시를 썼다:\n{first}"
    );
    // 스크립트가 고른 플랫폼을 첫 실행의 인자에서 읽어 그 자리의 기본 캐시를 채운다.
    let platform = first
        .split_whitespace()
        .skip_while(|w| *w != "--platform")
        .nth(1)
        .unwrap_or_else(|| panic!("fetch 에 --platform 을 넘기지 않았다:\n{first}"))
        .to_owned();
    root.write(
        &format!("target/official-plugins/v9.9.9/{platform}/seeded.tar.gz"),
        "x",
    );
    root.write(
        &format!("target/official-plugins/v9.9.9/{platform}/partial.tar.gz.part"),
        "x",
    );
    let (code, text) = root.run(&["--check", "--shared-cache"]);
    assert_eq!(code, 0, "--shared-cache 대조가 실패했다:\n{text}");
    let log = root.fetch_log();
    assert!(
        log.contains("CACHED seeded.tar.gz"),
        "기본 캐시 파일을 전용 캐시로 옮기지 않았다:\n{log}"
    );
    assert!(
        !log.contains("CACHED partial"),
        "받다 만 .part 파일까지 옮겼다:\n{log}"
    );
    assert!(
        !log.contains("/target/official-plugins/v9.9.9/"),
        "fetch 에 기본 캐시를 직접 넘겼다:\n{log}"
    );
}
