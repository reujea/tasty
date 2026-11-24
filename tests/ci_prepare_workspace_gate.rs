//! CI 작업 트리 준비 스크립트(scripts/ci-prepare-workspace.sh)의 판정과 정리를 임시 git 저장소로 고정한다.
//! 판정 경계는 df 없이 `--decide`로 재고, 본 실행은 임계값 0(유지)과 아주 큰 값(삭제)으로 df 값과 무관하게 만든다.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use tasty_doc_guards::git_env::command;

const SCRIPT: &str = "scripts/ci-prepare-workspace.sh";
const MARKER: &str = "target/.ci-cache-day";

fn script_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SCRIPT)
}

fn decide(args: &[&str]) -> (i32, String) {
    let out = command("bash")
        .arg(script_src())
        .arg("--decide")
        .args(args)
        .output()
        .expect("bash 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    )
}

#[test]
fn decide_wipes_at_or_below_the_threshold_and_keeps_above_it() {
    let today = "2026-11-23";
    assert_eq!(
        decide(&[today, today, "1", "99", "100"]),
        (0, "wipe low-disk".into())
    );
    assert_eq!(
        decide(&[today, today, "1", "100", "100"]),
        (0, "wipe low-disk".into())
    );
    assert_eq!(
        decide(&[today, today, "1", "101", "100"]),
        (0, "keep cached".into())
    );
}

#[test]
fn decide_wipes_once_a_day_and_when_the_marker_is_missing() {
    let today = "2026-11-23";
    assert_eq!(
        decide(&[today, "2026-11-22", "1", "999", "100"]),
        (0, "wipe first-of-day".into())
    );
    assert_eq!(
        decide(&[today, "-", "1", "999", "100"]),
        (0, "wipe no-marker".into())
    );
    assert_eq!(
        decide(&[today, "-", "0", "1", "100"]),
        (0, "none target-absent".into())
    );
    assert_eq!(
        decide(&[today, "2026-11-22", "0", "1", "100"]).1,
        "none target-absent"
    );
}

#[test]
fn decide_rejects_malformed_numbers() {
    assert_eq!(decide(&["2026-11-23", "-", "1", "x", "100"]).0, 2);
    assert_eq!(decide(&["2026-11-23", "-", "1"]).0, 2);
    assert_eq!(decide(&["2026-11-23", "-", "2", "1", "100"]).0, 2);
    assert_eq!(
        decide(&["2026-11-23", "-", "1", "1234567890123456789", "100"]).0,
        2
    );
}

/// 앞자리 0은 10진수로 읽는다. 8진수로 읽으면 0101(65)이 0100(64)보다 커지지 않거나 090에서 산술 오류가 난다.
#[test]
fn decide_reads_leading_zeros_as_decimal() {
    let today = "2026-11-23";
    assert_eq!(
        decide(&[today, today, "1", "0100", "099"]),
        (0, "keep cached".into())
    );
    assert_eq!(
        decide(&[today, today, "1", "090", "0100"]),
        (0, "wipe low-disk".into())
    );
}

struct Repo {
    dir: tempfile::TempDir,
    root: PathBuf,
}

fn git(root: &Path, args: &[&str]) {
    let st = command("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git 실행");
    assert!(st.success(), "git {args:?} 실패");
}

fn repo() -> Repo {
    let dir = tempfile::tempdir().expect("임시 디렉토리");
    let root = dir.path().join("r");
    fs::create_dir_all(root.join("scripts")).expect("scripts");
    fs::copy(script_src(), root.join(SCRIPT)).expect("스크립트 복사");
    fs::write(root.join(".gitignore"), "target/\n*.sig\n").expect(".gitignore");
    fs::write(root.join("tracked.txt"), "committed\n").expect("추적 파일");
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    );
    Repo { dir, root }
}

fn put(root: &Path, rel: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().expect("부모")).expect("폴더");
    fs::write(p, "x").expect("파일");
}

fn run(root: &Path, min_free_gb: &str) -> (i32, String) {
    run_with(root, min_free_gb, None)
}

/// `df_line`이 있으면 PATH 앞에 그 한 줄을 데이터 행으로 내는 가짜 df를 둔다.
fn run_with(root: &Path, min_free_gb: &str, df_line: Option<&str>) -> (i32, String) {
    let mut cmd = command("bash");
    cmd.current_dir(root)
        .arg(SCRIPT)
        .env("TASTY_CI_CACHE_MIN_FREE_GB", min_free_gb)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    let fake_bin = root.parent().expect("부모").join("fake-bin");
    if let Some(line) = df_line {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(&fake_bin).expect("fake-bin");
        let df = fake_bin.join("df");
        fs::write(
            &df,
            format!(
                "#!/bin/sh\nprintf '%s\\n' 'Filesystem 1024-blocks Used Available Capacity Mounted on' '{line}'\n"
            ),
        )
        .expect("가짜 df");
        fs::set_permissions(&df, fs::Permissions::from_mode(0o755)).expect("권한");
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{path}", fake_bin.display()));
    }
    let out = cmd.output().expect("bash 실행");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

/// 자정을 넘기면 오늘이 바뀌므로 스크립트와 같은 명령으로 읽는다.
fn today() -> String {
    let out = command("date").arg("+%F").output().expect("date");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn marker(root: &Path) -> Option<String> {
    fs::read_to_string(root.join(MARKER))
        .ok()
        .map(|s| s.trim().to_string())
}

/// 표지가 오늘이고 여유가 충분하면 루트 target의 cargo 산출물은 남고, 그 밖의 미추적·무시 파일과
/// target 안의 cargo 밖 상태(번들·잠금·스테이징·스냅숏·대조 폴더)는 지워진다.
#[test]
fn a_cached_run_keeps_cargo_outputs_and_clears_everything_else() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, MARKER);
    fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
    let kept = [
        "target/debug/deps/libfoo.rlib",
        "target/release/deps/bar",
        "target/official-plugins/v1/linux-x64/a.tar.gz",
    ];
    let cleared = [
        "target/debug/builtin-plugins/old-id/tasty-plugin.toml",
        "target/debug/builtin-plugins.lock",
        "target/debug/builtin-plugins.lock.123.456",
        "target/debug/.official-plugins-stage.ab12/x",
        "target/debug/.mesh-demo-stage.cd34/x",
        "target/debug/test-bundle-links/0123/x",
        "target/x86_64-pc-windows-gnu/debug/builtin-plugins/id/x",
        "target/debug/tasty-plugin-mesh-demo",
        "target/x86_64-pc-windows-gnu/debug/tasty-plugin-mesh-demo.exe",
        "target/official-plugin-fixtures-sync/x",
        "target/official-plugin-fixtures-sync.lock",
        "crates/removed-crate/tasty-plugin.toml.sig",
        "crates/kept/target/debug/x",
        "dist/pkg.tar.gz",
        "untracked.txt",
    ];
    for p in kept.iter().chain(cleared.iter()) {
        put(root, p);
    }
    fs::write(root.join("tracked.txt"), "modified by an earlier job\n").expect("수정");

    let (code, log) = run(root, "0");
    // 실행 도중 날짜가 바뀌었으면 판정 대상이 아니므로 다시 준비해 한 번 더 잰다.
    // 재시도는 시계로만 정한다. 스크립트가 남긴 표지로 정하면 표지를 잃는 회귀가 끝없는 재귀가 된다.
    if today() != day {
        return a_cached_run_keeps_cargo_outputs_and_clears_everything_else();
    }
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=keep reason=cached"), "{log}");
    for p in kept {
        assert!(root.join(p).exists(), "남아야 할 {p}가 지워졌다\n{log}");
    }
    for p in cleared {
        assert!(!root.join(p).exists(), "지워야 할 {p}가 남았다\n{log}");
    }
    assert_eq!(
        fs::read_to_string(root.join("tracked.txt")).expect("추적 파일"),
        "committed\n",
        "이전 잡이 바꾼 추적 파일이 되돌아가지 않았다"
    );
    assert_eq!(marker(root).as_deref(), Some(day.as_str()));
}

#[test]
fn a_stale_marker_wipes_target_and_records_today() {
    let r = repo();
    let root = &r.root;
    put(root, "target/debug/deps/libfoo.rlib");
    fs::write(root.join(MARKER), "2000-01-01\n").expect("표지");
    let (code, log) = run(root, "0");
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=wipe reason=first-of-day"), "{log}");
    assert!(!root.join("target/debug").exists(), "{log}");
    assert_eq!(marker(root), Some(today()));
}

#[test]
fn a_missing_marker_wipes_target() {
    let r = repo();
    let root = &r.root;
    put(root, "target/debug/deps/libfoo.rlib");
    let (code, log) = run(root, "0");
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=wipe reason=no-marker"), "{log}");
    assert!(!root.join("target/debug").exists(), "{log}");
    assert_eq!(marker(root), Some(today()));
}

/// release·build-check의 checkout이 target을 통째로 지운 뒤의 상태다. 다음 잡은 표지를 새로 쓴다.
#[test]
fn an_absent_target_starts_a_new_day_marker() {
    let r = repo();
    let root = &r.root;
    let (code, log) = run(root, "0");
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=none reason=target-absent"), "{log}");
    assert_eq!(marker(root), Some(today()));
}

#[test]
fn low_free_space_wipes_target() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, "target/debug/deps/libfoo.rlib");
    fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
    // 어떤 볼륨의 여유도 이 값(GB)을 넘지 않는다.
    let (code, log) = run(root, "999999999");
    if today() != day {
        return low_free_space_wipes_target();
    }
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=wipe reason=low-disk"), "{log}");
    assert!(!root.join("target/debug").exists(), "{log}");
}

#[test]
fn a_failed_wipe_removes_the_marker_and_fails() {
    use std::os::unix::fs::PermissionsExt;
    let r = repo();
    let root = &r.root;
    put(root, "target/locked/file");
    fs::write(root.join(MARKER), "2000-01-01\n").expect("표지");
    let locked = root.join("target/locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("권한");
    let probe = locked.join("probe");
    if fs::write(&probe, "x").is_ok() {
        // 권한을 무시하는 사용자(root)라 삭제 실패를 만들 수 없다.
        fs::remove_file(&probe).expect("probe 정리");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("권한 복원");
        panic!("쓰기 금지 폴더에 쓸 수 있어 삭제 실패를 재현하지 못했다(root로 실행 중인지 확인)");
    }
    let (code, log) = run(root, "0");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("권한 복원");
    assert_eq!(code, 1, "{log}");
    // 표지를 먼저 지우므로 덜 지운 target에는 표지가 없다. 다음 잡은 no-marker로 다시 지운다.
    assert_eq!(marker(root), None, "{log}");
    assert!(log.contains("The day marker was removed"), "{log}");
}

#[test]
fn a_malformed_threshold_is_a_usage_error() {
    let r = repo();
    let (code, log) = run(&r.root, "150G");
    assert_eq!(code, 2, "{log}");
}

/// target이 링크면 작업 트리 밖을 지울 수 있으므로 손대지 않고 멈춘다.
#[test]
fn a_symlinked_target_is_refused() {
    let r = repo();
    let root = &r.root;
    let outside = r.dir.path().join("outside");
    put(&outside, "debug/deps/libfoo.rlib");
    fs::write(outside.join(".ci-cache-day"), "2000-01-01\n").expect("표지");
    std::os::unix::fs::symlink(&outside, root.join("target")).expect("링크");
    let (code, log) = run(root, "0");
    assert_eq!(code, 2, "{log}");
    assert!(outside.join("debug/deps/libfoo.rlib").exists(), "{log}");
    assert_eq!(
        fs::read_to_string(outside.join(".ci-cache-day")).expect("표지"),
        "2000-01-01\n",
        "{log}"
    );
}

/// 남긴 target 안의 링크된 프로필 폴더는 따라가지 않는다.
#[test]
fn a_cached_run_does_not_follow_symlinked_profile_dirs() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, MARKER);
    fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
    let outside = r.dir.path().join("outside-profile");
    put(&outside, "builtin-plugins/id/x");
    put(&outside, "nested/builtin-plugins/id/x");
    std::os::unix::fs::symlink(&outside, root.join("target/linked")).expect("링크");
    let (code, log) = run(root, "0");
    if today() != day {
        return a_cached_run_does_not_follow_symlinked_profile_dirs();
    }
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=keep reason=cached"), "{log}");
    assert!(outside.join("builtin-plugins/id/x").exists(), "{log}");
    assert!(
        outside.join("nested/builtin-plugins/id/x").exists(),
        "{log}"
    );
}

/// df 데이터 행의 파일시스템·마운트 이름에 공백이 있어도 여유 칸을 읽는다.
/// 칸 번호로 읽으면 사용량(1)을 여유로 읽어 임계 1GB 아래로 판정한다.
#[test]
fn free_space_is_read_from_the_available_column_even_with_spaces_in_names() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, "target/debug/deps/libfoo.rlib");
    fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
    let (code, log) = run_with(
        root,
        "1",
        Some("my fs 999999999999 1 999999999999 1% /mnt/a b"),
    );
    if today() != day {
        return free_space_is_read_from_the_available_column_even_with_spaces_in_names();
    }
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=keep reason=cached"), "{log}");
    assert!(log.contains("free_before_gb=1023999"), "{log}");
}

/// 여유가 음수로 보고되면(예약 블록을 넘겨 쓴 볼륨) 여유 없음으로 보고 지운다.
#[test]
fn a_negative_available_value_wipes_target() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, "target/debug/deps/libfoo.rlib");
    fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
    let (code, log) = run_with(root, "0", Some("/dev/x 1000 1008 -8 100% /"));
    if today() != day {
        return a_negative_available_value_wipes_target();
    }
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=wipe reason=low-disk"), "{log}");
    assert!(log.contains("free_before_gb=0"), "{log}");
    assert!(!root.join("target/debug").exists(), "{log}");
}

/// 링크인 표지는 오늘 날짜를 담고 있어도 표지로 인정하지 않는다. 링크 대상은 건드리지 않는다.
#[test]
fn a_symlinked_marker_is_not_trusted() {
    let r = repo();
    let root = &r.root;
    let day = today();
    put(root, "target/debug/deps/libfoo.rlib");
    let outside = r.dir.path().join("outside-marker");
    fs::write(&outside, format!("{day}\n")).expect("바깥 표지");
    std::os::unix::fs::symlink(&outside, root.join(MARKER)).expect("링크");
    let (code, log) = run(root, "0");
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("action=wipe reason=no-marker"), "{log}");
    assert_eq!(
        fs::read_to_string(&outside).expect("바깥 표지"),
        format!("{day}\n"),
        "{log}"
    );
    assert_eq!(marker(root), Some(today()), "{log}");
}

/// 임계값은 바이트로 비교한다. 1GB(10^9 바이트)에 여유 976563KiB(1000000512바이트)는 임계보다 커서 남기고,
/// 976562KiB(999999488바이트)는 이하라 지운다. GB를 KiB로 올림해 비교하면 앞의 경우도 지운다.
#[test]
fn the_threshold_compares_bytes_at_an_odd_gigabyte() {
    for (avail, want) in [
        ("976563", "action=keep reason=cached"),
        ("976562", "action=wipe reason=low-disk"),
    ] {
        let r = repo();
        let root = &r.root;
        let day = today();
        put(root, "target/debug/deps/libfoo.rlib");
        fs::write(root.join(MARKER), format!("{day}\n")).expect("표지");
        let line = format!("/dev/x 9999999 1 {avail} 1% /");
        let (code, log) = run_with(root, "1", Some(&line));
        if today() != day {
            return the_threshold_compares_bytes_at_an_odd_gigabyte();
        }
        assert_eq!(code, 0, "{log}");
        assert!(log.contains(want), "여유 {avail}KiB, 임계 1GB\n{log}");
    }
}
