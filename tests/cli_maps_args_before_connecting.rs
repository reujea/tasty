//! CLI 는 인자 → 요청 매핑(서버 없이 끝나는 검증)을 **연결보다 먼저** 한다.
//!
//! 그래서 실행 중인 인스턴스가 없어도 잘못된 인자는 그 인자의 원인으로 끝난다 — "실행 중인
//! 인스턴스가 없다" 가 원인을 가리지 않는다. 구현 `crates/tasty-cli/src/run.rs` 의
//! `run_client_inner`, 구조 설명 `docs/dev-guide/cli-structure.md`.
//!
//! `TASTY_HOME` 을 빈 tempdir 로 격리해 포트 파일이 없는 상태(= 인스턴스 없음)를 만든다.
//! 로케일은 격리 홈의 기본값을 따르므로 번역되는 문장이 아니라 **번역되지 않는 조각**
//! (인자 이름 · 포트 파일 경로의 파일 이름)으로 판정한다.

use std::path::Path;
use std::process::{Command, Stdio};

/// 인스턴스 없음 문구가 반드시 싣는 조각 — 찾아본 포트 파일 경로의 파일 이름이다.
const NO_INSTANCE_MARK: &str = "tasty.port";

fn run(home: &Path, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tasty"))
        .args(args)
        .env("TASTY_HOME", home)
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .expect("run tasty");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 대조군 — 인자가 멀쩡하면 같은 격리 홈에서 인스턴스 없음으로 끝난다. 이것이 서야 아래
/// 두 시험의 "인스턴스 없음 문구가 없다" 가 격리 실패가 아니라 순서의 결과다.
#[test]
fn valid_arguments_without_an_instance_still_end_in_no_instance() {
    let home = tempfile::tempdir().expect("tempdir");
    for args in [
        &["webhook", "register", "--sequence", "[]"][..],
        &["new", "workspace", "--cwd", "."][..],
    ] {
        let (code, stderr) = run(home.path(), args);
        assert!(stderr.contains(NO_INSTANCE_MARK), "{args:?}: {stderr}");
        assert_ne!(code, Some(0), "{args:?}");
    }
}

/// 깨진 `--sequence` JSON 은 인스턴스가 없어도 파싱 오류로 끝난다.
#[test]
fn a_broken_json_argument_is_reported_before_connecting() {
    let home = tempfile::tempdir().expect("tempdir");
    let (code, stderr) = run(
        home.path(),
        &["webhook", "register", "--sequence", "{broken"],
    );
    assert!(stderr.contains("--sequence"), "{stderr}");
    assert!(!stderr.contains(NO_INSTANCE_MARK), "{stderr}");
    assert_eq!(code, Some(1), "{stderr}");
}

/// 없는 `--cwd` 디렉토리는 인스턴스가 없어도 그 경로의 오류로 끝난다.
#[test]
fn a_missing_cwd_is_reported_before_connecting() {
    let home = tempfile::tempdir().expect("tempdir");
    let missing = home.path().join("no-such-dir");
    let missing = missing.to_str().expect("utf-8 path");
    let (code, stderr) = run(home.path(), &["new", "workspace", "--cwd", missing]);
    assert!(stderr.contains("--cwd"), "{stderr}");
    assert!(!stderr.contains(NO_INSTANCE_MARK), "{stderr}");
    assert_eq!(code, Some(1), "{stderr}");
}
