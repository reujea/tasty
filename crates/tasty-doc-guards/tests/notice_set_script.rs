//! `scripts/lib/notice-set.sh`의 판정을 합성 고지 세트로 확인한다.
//! - `verify_notice_listing`은 목록의 각 줄을 경로 하나로 보고 앞의 `./` 하나만 뗀 뒤
//!   `prefix+경로`와 줄 전체를 비교한다. 다른 폴더 아래에 같은 이름이 있어도 통과하지 않는다.
//! - `LICENSES/`는 일반 파일만 담는다. 폴더·링크가 있으면 목록·스테이징이 실패한다.
//!   점으로 시작하는 항목도 열거해 거부한다. PowerShell(`Get-NoticeSetFiles`)도 같은 규칙이다.
//!   이 시험은 PowerShell을 실행하지 않는다. 그 함수가 규칙의 요소(`-Force`, 점 이름·폴더·링크
//!   거부)를 갖췄는지는 `notice_set_powershell_source`가 소스에서 확인한다.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use tasty_doc_guards::temp_scratch::Scratch;

fn script() -> PathBuf {
    tasty_doc_guards::repo_root().join("scripts/lib/notice-set.sh")
}

/// LICENSE, THIRD_PARTY_LICENSES.md, LICENSES/a.txt, LICENSES/b.txt를 가진 합성 저장소.
fn notice_repo(what: &str) -> Scratch {
    let dir = Scratch::new(what);
    let root = dir.path();
    std::fs::write(root.join("LICENSE"), "own\n").expect("LICENSE");
    std::fs::write(root.join("THIRD_PARTY_LICENSES.md"), "inventory\n").expect("inventory");
    std::fs::create_dir(root.join("LICENSES")).expect("LICENSES");
    std::fs::write(root.join("LICENSES/a.txt"), "a\n").expect("a");
    std::fs::write(root.join("LICENSES/b.txt"), "b\n").expect("b");
    dir
}

/// `root`를 cwd로 notice-set.sh를 source한 뒤 `body`를 실행한다. (종료 코드, stdout, stderr).
fn run(root: &Path, body: &str) -> (i32, String, String) {
    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(". \"$1\"\n{body}"))
        .arg("notice-set-test")
        .arg(script())
        .current_dir(root)
        .output()
        .expect("bash 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn listing_rc(root: &Path, listing: &str, prefix: &str, flat: bool) -> i32 {
    let flat = if flat { " flat" } else { "" };
    let body = format!(
        "L=$(cat <<'LISTING'\n{listing}\nLISTING\n)\nverify_notice_listing \"$L\" '{prefix}' t{flat}"
    );
    run(root, &body).0
}

#[test]
fn a_set_nested_under_another_folder_is_rejected() {
    let repo = notice_repo("notice-stale");
    let listing = "pkg/\npkg/stale/pkg/LICENSE\npkg/stale/pkg/THIRD_PARTY_LICENSES.md\n\
                   pkg/stale/pkg/LICENSES/a.txt\npkg/stale/pkg/LICENSES/b.txt";
    assert_eq!(listing_rc(repo.path(), listing, "pkg/", false), 1);
}

#[test]
fn a_tar_listing_with_or_without_a_leading_dot_passes() {
    let repo = notice_repo("notice-tar");
    let plain = "pkg/\npkg/tasty\npkg/LICENSE\npkg/THIRD_PARTY_LICENSES.md\npkg/LICENSES/\n\
                 pkg/LICENSES/a.txt\npkg/LICENSES/b.txt";
    assert_eq!(listing_rc(repo.path(), plain, "pkg/", false), 0);
    let dotted = "./\n./pkg/LICENSE\n./pkg/THIRD_PARTY_LICENSES.md\n./pkg/LICENSES/a.txt\n\
                  ./pkg/LICENSES/b.txt";
    assert_eq!(listing_rc(repo.path(), dotted, "pkg/", false), 0);
}

#[test]
fn a_missing_text_is_rejected() {
    let repo = notice_repo("notice-missing");
    let listing = "pkg/LICENSE\npkg/THIRD_PARTY_LICENSES.md\npkg/LICENSES/a.txt";
    assert_eq!(listing_rc(repo.path(), listing, "pkg/", false), 1);
}

#[test]
fn a_deb_payload_listing_passes() {
    // dpkg-deb --fsys-tarfile | tar -t 의 형식이다. dpkg-deb -c 처럼 권한·크기가 앞에 붙은
    // 줄은 경로 목록이 아니라서 받지 않는다.
    let repo = notice_repo("notice-deb");
    let listing = "./\n./usr/\n./usr/share/doc/tasty/\n./usr/share/doc/tasty/LICENSE\n\
                   ./usr/share/doc/tasty/THIRD_PARTY_LICENSES.md\n./usr/share/doc/tasty/LICENSES/\n\
                   ./usr/share/doc/tasty/LICENSES/a.txt\n./usr/share/doc/tasty/LICENSES/b.txt";
    assert_eq!(
        listing_rc(repo.path(), listing, "usr/share/doc/tasty/", false),
        0
    );
    let long = "-rw-r--r-- root/root 4 2026-11-14 00:00 ./usr/share/doc/tasty/LICENSE\n\
                -rw-r--r-- root/root 4 2026-11-14 00:00 ./usr/share/doc/tasty/THIRD_PARTY_LICENSES.md\n\
                -rw-r--r-- root/root 2 2026-11-14 00:00 ./usr/share/doc/tasty/LICENSES/a.txt\n\
                -rw-r--r-- root/root 2 2026-11-14 00:00 ./usr/share/doc/tasty/LICENSES/b.txt";
    assert_eq!(
        listing_rc(repo.path(), long, "usr/share/doc/tasty/", false),
        1
    );
}

#[test]
fn an_rpm_flat_listing_passes_only_in_its_own_folder() {
    let repo = notice_repo("notice-rpm");
    let listing = "/usr/bin/tasty\n/usr/share/licenses/tasty/LICENSE\n\
                   /usr/share/licenses/tasty/THIRD_PARTY_LICENSES.md\n\
                   /usr/share/licenses/tasty/a.txt\n/usr/share/licenses/tasty/b.txt";
    assert_eq!(
        listing_rc(repo.path(), listing, "/usr/share/licenses/tasty/", true),
        0
    );
    let elsewhere = "/usr/share/licenses/tasty/LICENSE\n\
                     /usr/share/licenses/tasty/THIRD_PARTY_LICENSES.md\n\
                     /opt/x/a.txt\n/opt/x/b.txt";
    assert_eq!(
        listing_rc(repo.path(), elsewhere, "/usr/share/licenses/tasty/", true),
        1
    );
}

#[test]
fn a_folder_in_licenses_fails_listing_and_staging() {
    let repo = notice_repo("notice-subdir");
    std::fs::create_dir(repo.path().join("LICENSES/sub")).expect("sub");
    std::fs::write(repo.path().join("LICENSES/sub/New-MIT.txt"), "n\n").expect("new");
    let (rc, _, err) = run(repo.path(), "notice_set_files");
    assert_eq!(
        rc, 1,
        "하위 폴더를 조용히 건너뛰면 그 본문이 배포물에서 빠진다"
    );
    assert!(err.contains("LICENSES/sub"), "stderr: {err}");
    let dest = repo.path().join("out");
    let (rc, _, _) = run(repo.path(), "stage_notice out");
    assert_eq!(rc, 1);
    assert!(
        !dest.exists(),
        "실패한 스테이징이 부분 트리를 남기면 안 된다"
    );
}

#[test]
fn a_link_in_licenses_is_rejected() {
    let repo = notice_repo("notice-link");
    std::os::unix::fs::symlink("a.txt", repo.path().join("LICENSES/c.txt")).expect("link");
    let (rc, _, err) = run(repo.path(), "notice_set_files");
    assert_eq!(rc, 1);
    assert!(err.contains("LICENSES/c.txt"), "stderr: {err}");
}

/// 점으로 시작하는 항목도 열거해 같은 규칙을 적용한다. 숨김 폴더·링크가 열거에서 빠지면
/// 거부 분기를 우회하고, 숨김 파일은 라이선스 본문이 아니므로 받지 않는다.
#[test]
fn hidden_entries_in_licenses_are_rejected() {
    let cases: [(&str, fn(&Path)); 3] = [
        (".sub", |root| {
            std::fs::create_dir(root.join("LICENSES/.sub")).expect("hidden dir");
            std::fs::write(root.join("LICENSES/.sub/New-MIT.txt"), "n\n").expect("new");
        }),
        (".link", |root| {
            std::os::unix::fs::symlink("a.txt", root.join("LICENSES/.link")).expect("link");
        }),
        (".DS_Store", |root| {
            std::fs::write(root.join("LICENSES/.DS_Store"), "x").expect("dot file");
        }),
    ];
    for (name, make) in cases {
        let repo = notice_repo("notice-hidden");
        make(repo.path());
        let (rc, out, err) = run(repo.path(), "notice_set_files");
        assert_eq!(rc, 1, "LICENSES/{name} 를 받으면 안 된다. stdout: {out}");
        assert!(err.contains(&format!("LICENSES/{name}")), "stderr: {err}");
    }
}

/// 숨김 항목을 열거하려고 켠 셸 옵션이 호출자 셸에 남지 않는다.
#[test]
fn listing_keeps_the_callers_glob_options() {
    let repo = notice_repo("notice-shopt");
    let (rc, out, err) = run(
        repo.path(),
        "notice_set_files >/dev/null; shopt -q dotglob && echo dotglob-on || echo dotglob-off\n\
         shopt -s dotglob; notice_set_files >/dev/null; shopt -q dotglob && echo dotglob-on || echo dotglob-off",
    );
    assert_eq!(rc, 0, "stderr: {err}");
    assert_eq!(out, "dotglob-off\ndotglob-on\n");
}

#[test]
fn a_flat_set_is_listed_staged_and_verified() {
    let repo = notice_repo("notice-flat");
    let (rc, out, _) = run(repo.path(), "notice_set_files");
    assert_eq!(rc, 0);
    assert_eq!(
        out,
        "LICENSE\nTHIRD_PARTY_LICENSES.md\nLICENSES/a.txt\nLICENSES/b.txt\n"
    );
    let (rc, _, err) = run(
        repo.path(),
        "stage_notice out && verify_notice_tree out out",
    );
    assert_eq!(rc, 0, "stderr: {err}");
}
