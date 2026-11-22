//! UTF-8이 아닌 인자가 패닉 없이 위치만 알리는 사용 오류(종료 코드 2)로 끝나는지 확인한다.
//! 포트 파일 없는 격리 홈에서 실행해 인스턴스 없음 오류보다 먼저 거절되는지도 본다.
//! 깨진 바이트열은 Unix의 `OsStr::from_bytes`로만 만들 수 있어 이 타깃은 Unix 전용이다.
//! Windows의 짝 없는 서로게이트는 `crates/tasty-cli/src/argv.rs`의 단위 시험이 덮는다.

#![cfg(unix)]

#[path = "spawn_diag/mod.rs"]
mod spawn_diag;

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const MARK: &str = "NONUTF8MARK";
const TAIL: &str = "TAIL";
const NO_INSTANCE_MARK: &str = "tasty.port";

/// 표지 문자열 사이에 UTF-8이 아닌 바이트(`C2` 뒤에 이어지는 바이트 없음)를 둔다.
/// 진단이 인자 원문을 어떤 형태로든(원본·이스케이프·치환) 옮기면 표지가 드러난다.
fn broken() -> OsString {
    let mut bytes = MARK.as_bytes().to_vec();
    bytes.push(0xC2);
    bytes.extend_from_slice(TAIL.as_bytes());
    OsStr::from_bytes(&bytes).to_os_string()
}

fn home(language: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("config.toml"),
        format!("[general]\nlanguage='{language}'\n"),
    )
    .expect("write config");
    dir
}

fn command(home: &Path) -> Command {
    let mut cmd = Command::new(spawn_diag::instance_bin());
    cmd.env("TASTY_HOME", home)
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .stdin(Stdio::null());
    cmd
}

fn run(home: &Path, args: &[OsString]) -> Output {
    command(home).args(args).output().expect("run tasty")
}

fn lang_value(language: &str, key: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang/{language}.toml"));
    let table: toml::Table = std::fs::read_to_string(path)
        .expect("read lang file")
        .parse()
        .expect("parse lang file");
    table["cli"]["argv"][key]
        .as_str()
        .expect("string value")
        .to_string()
}

fn args(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(OsString::from).collect()
}

/// 종료 코드 2, 기대 문구 한 줄, 원문·패닉·크래시 보고서·연결 시도 흔적 없음.
fn assert_rejected(home: &Path, out: &Output, expected: &str) {
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let raw = &out.stderr;
    assert!(!raw.contains(&0xC2), "raw byte leaked: {out:?}");
    let stderr = std::str::from_utf8(raw).expect("stderr is UTF-8");
    assert_eq!(stderr, format!("{expected}\n"), "{stderr}");
    for leaked in [MARK, TAIL, "\\x", "\u{FFFD}", "panicked", "unwrap"] {
        assert!(!stderr.contains(leaked), "{leaked}: {stderr}");
    }
    assert!(!stderr.contains(NO_INSTANCE_MARK), "{stderr}");
    let reports = home.join("crash-reports");
    let count = match std::fs::read_dir(&reports) {
        Ok(dir) => dir.count(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => 0,
        Err(err) => panic!("read {}: {err}", reports.display()),
    };
    assert_eq!(count, 0, "crash reports in {}", reports.display());
}

#[test]
fn a_non_utf8_argument_is_reported_by_position_with_exit_code_2() {
    let dir = home("en");
    let mut argv = args(&["send", "queue", "--to", "1"]);
    argv.push(broken());
    let out = run(dir.path(), &argv);
    let expected = lang_value("en", "not_utf8").replace("{}", "5");
    assert_rejected(dir.path(), &out, &expected);
}

#[test]
fn the_first_of_several_non_utf8_arguments_is_reported() {
    let dir = home("en");
    let mut argv = args(&["send", "text"]);
    argv.push(broken());
    argv.push(broken());
    let out = run(dir.path(), &argv);
    let expected = lang_value("en", "not_utf8").replace("{}", "3");
    assert_rejected(dir.path(), &out, &expected);
}

/// `--all`은 clap 해석 전에 전체 트리를 출력하는 단축 경로다. 인자 검사가 그보다 먼저다.
#[test]
fn the_check_runs_before_the_full_tree_shortcut() {
    let dir = home("en");
    let mut argv = args(&["--all"]);
    argv.push(broken());
    let out = run(dir.path(), &argv);
    let expected = lang_value("en", "not_utf8").replace("{}", "2");
    assert_rejected(dir.path(), &out, &expected);
}

#[test]
fn a_non_utf8_program_name_has_its_own_message() {
    let dir = home("en");
    let out = command(dir.path())
        .arg0(broken())
        .args(["send", "queue", "--to", "1", "x"])
        .output()
        .expect("run tasty");
    let expected = lang_value("en", "program_not_utf8");
    assert_rejected(dir.path(), &out, &expected);
}

/// 번역 초기화 전의 문구는 내장 영어다. 영어가 아닌 언어로 나오면 검사가 초기화 뒤에 있다.
#[test]
fn the_message_follows_the_configured_language() {
    let dir = home("ko");
    let mut argv = args(&["send", "queue", "--to", "1"]);
    argv.push(broken());
    let out = run(dir.path(), &argv);
    let expected = lang_value("ko", "not_utf8").replace("{}", "5");
    assert_ne!(expected, lang_value("en", "not_utf8").replace("{}", "5"));
    assert_rejected(dir.path(), &out, &expected);
}

/// 같은 격리 환경에서 UTF-8 비 ASCII 인자는 그대로 해석되어 인스턴스 없음 오류로 끝난다.
#[test]
fn valid_non_ascii_arguments_still_reach_the_connection() {
    let dir = home("en");
    let out = run(dir.path(), &args(&["send", "queue", "--to", "1", "·"]));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(NO_INSTANCE_MARK), "{stderr}");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
