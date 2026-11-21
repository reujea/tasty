//! rpm 폴더 소유의 생성기(`scripts/lib/rpm_dir_assets.py`)와 판정 함수(`scripts/lib/rpm-dirs.sh`)를
//! 합성 입력으로 확인한다.
//! - 생성기는 Cargo.toml의 assets를 그대로 옮기고, 소유 뿌리 아래 파일의 조상 폴더만 폴더 항목으로 덧붙인다.
//!   빈 스테이징 폴더는 넣지 않고, 폴더 dest에는 끝 슬래시가 없다. 출력은 tomllib으로 다시 읽어 대조한다.
//! - 판정 함수는 rpm 헤더 목록 형식("권한<TAB>경로")을 입력으로 받는다. 실제 rpm 헤더와의 일치는
//!   `scripts/build-linux.sh`가 만든 rpm에서 판정한다.
//! - Python 3.11 이상(tomllib)이 필요하다. 없으면 실패한다.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;
use tasty_doc_guards::temp_scratch::Scratch;

fn repo(rel: &str) -> PathBuf {
    tasty_doc_guards::repo_root().join(rel)
}

fn python() -> PathBuf {
    for dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
        let p = Path::new(dir).join("python3");
        if p.exists() {
            return p;
        }
    }
    panic!("이 시험에는 python3(3.11 이상)가 필요하다");
}

/// 생성기를 실행한다. (종료 코드, stderr)
fn generate(cwd: &Path, manifest: &str) -> (i32, String) {
    std::fs::write(cwd.join("Cargo.toml"), manifest).expect("Cargo.toml");
    std::fs::write(cwd.join("dir-entry"), "").expect("dir-entry");
    let out = Command::new(python())
        .arg("-I")
        .arg(repo("scripts/lib/rpm_dir_assets.py"))
        .args([
            "Cargo.toml",
            "dir-entry",
            "out.toml",
            "/usr/lib/tasty",
            "/usr/share/licenses/tasty",
        ])
        .current_dir(cwd)
        .output()
        .expect("python3 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 출력 TOML을 tomllib으로 읽어 항목마다 키=값을 정렬해 한 줄로 낸다.
fn read_back(cwd: &Path) -> Vec<String> {
    let out = Command::new(python())
        .arg("-I")
        .arg("-c")
        .arg(
            "import sys, tomllib\n\
             d = tomllib.load(open(sys.argv[1], 'rb'))\n\
             for a in d['assets']:\n\
             \x20   print(' '.join(f'{k}={a[k]!r}' for k in sorted(a)))\n",
        )
        .arg(cwd.join("out.toml"))
        .output()
        .expect("python3 실행");
    assert!(
        out.status.success(),
        "출력 TOML을 tomllib이 읽지 못했다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn mkfile(root: &Path, rel: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(p, "x").expect("write");
}

const MANIFEST: &str = "[package.metadata.generate-rpm]\n\
assets = [\n\
    { source = \"bin/tasty\", dest = \"/usr/bin/tasty\", mode = \"755\" },\n\
    { source = \"LICENSE\", dest = \"/usr/share/licenses/tasty/LICENSE\", mode = \"644\", doc = true },\n\
    { source = \"LICENSES/*\", dest = \"/usr/share/licenses/tasty/\", mode = \"644\" },\n\
    { source = \"plugins/**/*\", dest = \"/usr/lib/tasty/plugins/\", config = \"noreplace\" },\n\
]\n";

#[test]
fn the_generator_keeps_the_assets_and_adds_the_ancestor_directories() {
    let dir = Scratch::new("rpm-dirs-gen");
    let root = dir.path();
    mkfile(root, "LICENSES/a.txt");
    mkfile(root, "plugins/com.a/tasty-plugin.toml");
    mkfile(root, "plugins/com.a/lang/en.toml");
    mkfile(root, "plugins/com.b/.hidden/x");
    std::fs::create_dir_all(root.join("plugins/com.empty/sub")).expect("빈 폴더");
    let (rc, err) = generate(root, MANIFEST);
    assert_eq!(rc, 0, "{err}");
    let got = read_back(root);
    let dir_entry = |d: &str| format!("dest='{d}' mode='40755' source='dir-entry'");
    let want = vec![
        "dest='/usr/bin/tasty' mode='755' source='bin/tasty'".to_owned(),
        "dest='/usr/share/licenses/tasty/LICENSE' doc=True mode='644' source='LICENSE'".to_owned(),
        "dest='/usr/share/licenses/tasty/' mode='644' source='LICENSES/*'".to_owned(),
        "config='noreplace' dest='/usr/lib/tasty/plugins/' source='plugins/**/*'".to_owned(),
        dir_entry("/usr/lib/tasty"),
        dir_entry("/usr/lib/tasty/plugins"),
        dir_entry("/usr/lib/tasty/plugins/com.a"),
        dir_entry("/usr/lib/tasty/plugins/com.a/lang"),
        dir_entry("/usr/lib/tasty/plugins/com.b"),
        dir_entry("/usr/lib/tasty/plugins/com.b/.hidden"),
        dir_entry("/usr/share/licenses/tasty"),
    ];
    assert_eq!(
        got, want,
        "빈 폴더(com.empty)는 넣지 않고 /usr/bin은 소유하지 않는다"
    );
}

#[test]
fn the_generator_escapes_control_characters_for_toml() {
    let dir = Scratch::new("rpm-dirs-escape");
    let manifest = "[package.metadata.generate-rpm]\n\
assets = [ { source = \"a\\u007Fb\\\"c\\\\d\\te\", dest = \"/usr/bin/x\" } ]\n";
    let (rc, err) = generate(dir.path(), manifest);
    assert_eq!(rc, 0, "{err}");
    assert_eq!(
        read_back(dir.path()),
        vec!["dest='/usr/bin/x' source='a\\x7fb\"c\\\\d\\te'".to_owned()]
    );
}

#[test]
fn the_generator_rejects_unsupported_values_and_globs() {
    for (what, asset) in [
        (
            "정수 값",
            "{ source = \"a\", dest = \"/usr/bin/a\", mode = 755 }",
        ),
        (
            "지원하지 않는 glob",
            "{ source = \"p/*.txt\", dest = \"/usr/lib/tasty/\" }",
        ),
        (
            "glob인데 dest가 폴더가 아님",
            "{ source = \"p/*\", dest = \"/usr/lib/tasty/x\" }",
        ),
    ] {
        let dir = Scratch::new("rpm-dirs-reject");
        let manifest = format!("[package.metadata.generate-rpm]\nassets = [ {asset} ]\n");
        let (rc, err) = generate(dir.path(), &manifest);
        assert_eq!(rc, 1, "{what}: {err}");
        assert!(err.contains("Error: rpm dir assets:"), "{what}: {err}");
        assert!(
            !dir.path().join("out.toml").exists(),
            "{what}: 출력이 남았다"
        );
    }
}

/// rpm-dirs.sh를 source한 뒤 `body`를 실행한다. (종료 코드, stdout, stderr)
fn run_sh(body: &str) -> (i32, String, String) {
    let out = Command::new("/bin/bash")
        .env("LC_ALL", "C")
        .arg("-c")
        .arg(format!(". \"$1\"\n{body}"))
        .arg("rpm-dirs-test")
        .arg(repo("scripts/lib/rpm-dirs.sh"))
        .output()
        .expect("bash 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const D: &str = "drwxr-xr-x";
const F: &str = "-rw-r--r--";

fn entries(rows: &[(&str, &str)]) -> String {
    rows.iter()
        .map(|(perm, path)| format!("{perm}\t{path}\n"))
        .collect()
}

fn good() -> Vec<(&'static str, &'static str)> {
    vec![
        ("-rwxr-xr-x", "/usr/bin/tasty"),
        (D, "/usr/lib/tasty"),
        (D, "/usr/lib/tasty/plugins"),
        (D, "/usr/lib/tasty/plugins/com.a"),
        (F, "/usr/lib/tasty/plugins/com.a/tasty-plugin.toml"),
        (D, "/usr/share/licenses/tasty"),
        (F, "/usr/share/licenses/tasty/LICENSE"),
    ]
}

fn verify(rows: &[(&str, &str)]) -> (i32, String) {
    let body = format!(
        "verify_rpm_dir_ownership \"$(printf '%s' '{}')\" t.rpm \"${{RPM_OWNED_ROOTS[@]}}\"",
        entries(rows)
    );
    let (rc, _, err) = run_sh(&body);
    (rc, err)
}

#[test]
fn matching_directory_entries_pass() {
    let (rc, err) = verify(&good());
    assert_eq!(rc, 0, "{err}");
}

#[test]
fn a_missing_directory_entry_is_rejected() {
    let rows: Vec<_> = good()
        .into_iter()
        .filter(|(_, p)| *p != "/usr/lib/tasty/plugins")
        .collect();
    let (rc, err) = verify(&rows);
    assert_eq!(rc, 1);
    assert!(
        err.contains("directory not owned by the package: /usr/lib/tasty/plugins"),
        "{err}"
    );
}

#[test]
fn a_directory_outside_the_owned_roots_is_rejected() {
    let mut rows = good();
    rows.push((D, "/usr/bin"));
    let (rc, err) = verify(&rows);
    assert_eq!(rc, 1);
    assert!(
        err.contains("directory entry outside the owned roots: /usr/bin"),
        "{err}"
    );
}

#[test]
fn an_owned_directory_without_files_is_rejected() {
    let mut rows = good();
    rows.push((D, "/usr/lib/tasty/plugins/com.empty"));
    let (rc, err) = verify(&rows);
    assert_eq!(rc, 1);
    assert!(
        err.contains("owned directory with no packaged file: /usr/lib/tasty/plugins/com.empty"),
        "{err}"
    );
}

#[test]
fn a_directory_with_another_mode_is_rejected() {
    let rows: Vec<_> = good()
        .into_iter()
        .map(|(perm, p)| {
            if p == "/usr/lib/tasty/plugins/com.a" {
                ("drwx------", p)
            } else {
                (perm, p)
            }
        })
        .collect();
    let (rc, err) = verify(&rows);
    assert_eq!(rc, 1);
    assert!(
        err.contains("directory /usr/lib/tasty/plugins/com.a has mode drwx------"),
        "{err}"
    );
}

#[test]
fn regular_files_are_listed_and_other_entry_types_are_rejected() {
    let body = format!(
        "rpm_regular_files \"$(printf '%s' '{}')\" t.rpm",
        entries(&good())
    );
    let (rc, out, err) = run_sh(&body);
    assert_eq!(rc, 0, "{err}");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        vec![
            "/usr/bin/tasty",
            "/usr/lib/tasty/plugins/com.a/tasty-plugin.toml",
            "/usr/share/licenses/tasty/LICENSE"
        ]
    );

    let mut rows = good();
    rows.push(("lrwxrwxrwx", "/usr/lib/tasty/plugins/com.a/link"));
    let body = format!(
        "rpm_regular_files \"$(printf '%s' '{}')\" t.rpm",
        entries(&rows)
    );
    let (rc, _, err) = run_sh(&body);
    assert_eq!(rc, 1);
    assert!(
        err.contains(
            "unexpected rpm file entry type in t.rpm: lrwxrwxrwx /usr/lib/tasty/plugins/com.a/link"
        ),
        "{err}"
    );
}

#[test]
fn build_linux_writes_the_directory_entries_before_generating_the_rpm() {
    let src = std::fs::read_to_string(repo("scripts/build-linux.sh")).expect("build-linux.sh");
    let check = "if [[ \"$PROFILE\" != \"debug\" ]]; then\n    check_rpm_dir_tools || exit 1\nfi\n";
    let check_at = src.find(check).expect("Python 사전 검사 블록");
    let build_at = src.find("cargo build $CARGO_FLAGS").expect("cargo build");
    assert!(check_at < build_at, "Python 검사는 본체 빌드 전에 돈다");
    let write = "    write_rpm_assets_overwrite Cargo.toml \"$RPM_ASSETS_TOML\" || exit 1\n    cargo generate-rpm ";
    assert!(src.contains(write), "폴더 항목 표를 만든 직후 rpm을 만든다");
    assert!(
        src.contains("verify_rpm_dir_ownership \"$RPM_ENTRIES\" \"$RPM_FILE\" \"${RPM_OWNED_ROOTS[@]}\" || exit 1"),
        "만든 rpm의 폴더 소유를 확인한다"
    );
    assert!(
        !src.contains("rpm -qpl"),
        "rpm 목록은 폴더 줄이 섞이지 않게 헤더 목록의 일반 파일만 쓴다"
    );
}
