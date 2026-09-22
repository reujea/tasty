//! CLI 클라이언트가 stdout 파이프 조기 종료(EPIPE)를 만나도 panic(종료 코드 101)·
//! 가짜 crash report 없이 **종료 코드 0** 으로 조용히 끝나는지 검증한다.
//! 정책 근거 `docs/adr/0101-cli-stdout-broken-pipe-exit-zero.md`, 구현
//! `crates/tasty-cli/src/out.rs`.
//!
//! host 없이 출력이 나오는 로컬 명령만 써서 CLI 클라이언트 갈래를 각각 덮는다:
//! - `-a` → `print_command_tree` (`Routed::AlreadyHandled`)
//! - 루트 `--help` → `print_augmented_help` (`Routed::AlreadyHandled`)
//! - `tool remote-profile list` → `run_client` (`Routed::Subcommand`)
//! - 서브커맨드 `--help` → clap `Error::exit` (자체적으로 EPIPE 를 삼키는 별개 경로 —
//!   회귀 가드로 함께 둔다)
//!
//! `TASTY_HOME` 을 tempdir 로 격리하므로 crash report 가 생기면 그 안의
//! `crash-reports/` 에 남는다 — 사용자 홈은 건드리지 않는다.
//!
//! stderr 도 같은 모양으로 잰다 — 읽는 쪽이 stderr 를 먼저 닫아도(`2>&1 | head`) panic ·
//! crash report 가 없고, 종료 코드는 stderr 를 열어 둔 대조군과 **같다**(stderr 가 닫혔다고
//! 실패가 성공이 되지 않는다 — `docs/adr/0513-cli-stderr-broken-pipe-keeps-the-exit-code.md`).
//!
//! 소스 스캔 둘: tasty-cli 가 `println!`/`print!` 나 `eprintln!` 으로 되돌아가면 같은 panic
//! 이 재발하므로, 쓰기는 `out.rs` 의 `outln!`/`out!`/`errln!` 로만 하도록 여기서 강제한다.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

fn tasty(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tasty"));
    cmd.env("TASTY_HOME", home)
        // 부모가 tasty 안에서 돌면 상속된다 — `Routed::AugmentedHelp` 로 새지 않게 제거.
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

/// stdout 을 파이프로 열고 읽는 쪽을 **즉시 닫은 뒤** 자식을 기다린다 — `| true` 와
/// 같은 조건. 자식이 첫 write 를 하기 전에 read end 가 닫히므로 EPIPE 가 결정적으로
/// 발생한다(`| head -c 1` 처럼 읽는 쪽이 늦게 닫히는 경합이 없다).
fn run_with_closed_stdout(mut cmd: Command) -> (ExitStatus, String) {
    let mut child = cmd.stdout(Stdio::piped()).spawn().expect("spawn tasty");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait tasty");
    (
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 대조군 — stdout 을 열어 둔 정상 실행. 같은 명령이 실제로 stdout 에 쓴다는 것을
/// 확인해야 위 EPIPE 케이스가 "출력이 없어서 통과" 한 것이 아님이 보장된다.
fn run_with_open_stdout(mut cmd: Command) -> (ExitStatus, String) {
    let output = cmd.stdout(Stdio::piped()).output().expect("run tasty");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn crash_report_count(home: &Path) -> usize {
    std::fs::read_dir(home.join("crash-reports"))
        .map(|d| d.count())
        .unwrap_or(0)
}

fn assert_quiet_exit_zero(label: &str, home: &Path, status: ExitStatus, stderr: &str) {
    assert_eq!(
        status.code(),
        Some(0),
        "{label}: 종료 코드가 0 이 아님 ({status}); stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("Tasty crashed") && !stderr.contains("panicked"),
        "{label}: panic 흔적이 stderr 에 있음:\n{stderr}"
    );
    assert!(
        !stderr.to_ascii_lowercase().contains("broken pipe"),
        "{label}: EPIPE 가 에러로 노출됨:\n{stderr}"
    );
    assert_eq!(
        crash_report_count(home),
        0,
        "{label}: crash report 가 생성됨 ({})",
        home.join("crash-reports").display()
    );
}

/// 라벨 + 인자 → 두 번 실행한다: (1) stdout 열림(대조군, 출력이 실제로 있어야 함),
/// (2) stdout 닫힘(검증 대상).
fn check_command(label: &str, args: &[&str]) {
    let home = tempfile::tempdir().expect("tempdir");
    let home = home.path();

    let (status, stdout) = run_with_open_stdout({
        let mut c = tasty(home);
        c.args(args);
        c
    });
    assert_eq!(
        status.code(),
        Some(0),
        "{label}: 대조군(stdout 열림) 종료 코드"
    );
    assert!(
        !stdout.trim().is_empty(),
        "{label}: 대조군이 stdout 에 아무것도 쓰지 않음 — EPIPE 시나리오가 성립하지 않는다"
    );

    let (status, stderr) = run_with_closed_stdout({
        let mut c = tasty(home);
        c.args(args);
        c
    });
    assert_quiet_exit_zero(label, home, status, &stderr);
}

/// `Routed::Subcommand` → `run_client` 갈래. host 없이 출력이 나오는 로컬 명령이며,
/// 프로필이 없어도 "없음" 한 줄을 stdout 에 쓴다.
#[test]
fn subcommand_output_with_closed_stdout_exits_zero() {
    check_command(
        "tool remote-profile list",
        &["tool", "remote-profile", "list"],
    );
}

/// `Routed::AlreadyHandled` → `print_command_tree` 갈래(`parse_or_route` 안에서 출력).
#[test]
fn command_tree_with_closed_stdout_exits_zero() {
    check_command("-a", &["-a"]);
}

/// `Routed::AlreadyHandled` → `print_augmented_help` 갈래. clap `print_help` 의
/// `io::Result`(Broken pipe) 도 같은 규칙으로 접힌다.
#[test]
fn root_help_with_closed_stdout_exits_zero() {
    check_command("--help", &["--help"]);
}

/// 서브커맨드 `--help` 는 clap `Error::exit` 가 stdout 에 직접 쓰고 EPIPE 를 스스로
/// 삼킨다(코드 0). tasty 코드 밖의 경로라 동작이 바뀌면 여기서 드러난다.
#[test]
fn subcommand_help_with_closed_stdout_exits_zero() {
    check_command("list --help", &["list", "--help"]);
}

/// stderr 를 파이프로 열고 읽는 쪽을 **즉시 닫은 뒤** 자식을 기다린다. stdout 은 버린다.
fn run_with_closed_stderr(home: &Path, args: &[&str]) -> ExitStatus {
    let mut cmd = tasty(home);
    cmd.args(args).stdout(Stdio::null());
    let mut child = cmd.spawn().expect("spawn tasty");
    drop(child.stderr.take());
    child.wait().expect("wait tasty")
}

/// 라벨 + 인자 → 두 번 실행한다: (1) stderr 열림(대조군 — stderr 에 실제로 쓰고 0 이 아닌
/// 코드로 끝나야 한다), (2) stderr 닫힘(검증 대상 — 같은 코드, panic · crash report 없음).
fn check_stderr_command(label: &str, args: &[&str]) {
    let home = tempfile::tempdir().expect("tempdir");
    let home = home.path();

    let control = tasty(home)
        .args(args)
        .stdout(Stdio::null())
        .output()
        .expect("run tasty");
    let control_stderr = String::from_utf8_lossy(&control.stderr);
    assert!(
        !control_stderr.trim().is_empty(),
        "{label}: 대조군이 stderr 에 아무것도 쓰지 않음 — EPIPE 시나리오가 성립하지 않는다"
    );
    assert_ne!(
        control.status.code(),
        Some(0),
        "{label}: 대조군이 실패 경로가 아니다"
    );

    let status = run_with_closed_stderr(home, args);
    assert_eq!(
        status.code(),
        control.status.code(),
        "{label}: stderr 가 닫히자 종료 코드가 바뀜 ({status}) — panic 이면 101 · abort 면 신호"
    );
    assert_eq!(
        crash_report_count(home),
        0,
        "{label}: crash report 가 생성됨 ({})",
        home.join("crash-reports").display()
    );
}

/// clap 파싱 오류 — tasty-cli 의 `format_parse_error` 가 stderr 에 여러 줄을 쓴다.
#[test]
fn parse_error_with_closed_stderr_keeps_its_exit_code() {
    check_stderr_command("unknown subcommand", &["nosuchcmd"]);
    check_stderr_command(
        "missing required argument",
        &["read", "since-mark", "--surface", "1", "--cursor", "5"],
    );
}

/// host 에 닿지 못한 실패(포트 파일 없음) — 오류가 `main` 까지 올라가 std 가 stderr 에 쓴다.
#[test]
fn unreachable_host_with_closed_stderr_keeps_its_exit_code() {
    check_stderr_command("no port file", &["list", "surfaces"]);
}

/// tasty-cli 의 stdout 쓰기는 `out.rs` 의 `outln!`/`out!` 로만 한다 — `println!`/`print!`
/// 가 돌아오면 EPIPE panic 이 재발한다. 주석·문자열 안의 언급은 대상이 아니다.
#[test]
fn cli_crate_has_no_direct_stdout_print() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/tasty-cli/src");
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "tasty-cli 소스를 찾지 못함: {}",
        root.display()
    );

    let mut offenders = Vec::new();
    for path in files {
        if path.file_name().is_some_and(|n| n == "out.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for (i, line) in src.lines().enumerate() {
            let code = strip_comment_and_strings(line);
            if invokes_macro(&code, "println") || invokes_macro(&code, "print") {
                offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "tasty-cli 에 std 의 stdout 출력 매크로가 있음 — `crate::out::{{outln, out}}` 을 쓴다:\n{}",
        offenders.join("\n")
    );
}

/// tasty-cli 의 stderr 쓰기는 `out.rs` 의 `errln!` 로만 한다 — `eprintln!` 이 돌아오면 stderr
/// 가 닫혔을 때 panic 이 재발한다. 주석·문자열 안의 언급은 대상이 아니다.
#[test]
fn cli_crate_has_no_direct_stderr_print() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/tasty-cli/src");
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "tasty-cli 소스를 찾지 못함: {}",
        root.display()
    );

    let mut offenders = Vec::new();
    for path in files {
        if path.file_name().is_some_and(|n| n == "out.rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for (i, line) in src.lines().enumerate() {
            let code = strip_comment_and_strings(line);
            if invokes_macro(&code, "eprintln") || invokes_macro(&code, "eprint") {
                offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "tasty-cli 에 std 의 stderr 출력 매크로가 있음 — `crate::out::errln` 을 쓴다:\n{}",
        offenders.join("\n")
    );
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `//` 이후와 문자열 리터럴 내용을 지운다 — 주석·문구에 적힌 `println!` 을 오탐하지 않게.
fn strip_comment_and_strings(line: &str) -> String {
    let code = line.split("//").next().unwrap_or("");
    let mut out = String::new();
    let mut in_str = false;
    let mut esc = false;
    for c in code.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// `name!` 매크로 호출이 식별자 경계에서 시작하는지 — `eprintln!` 안의 `println` 은
/// 제외. 매크로 이름과 `!` 를 나눠 받는 것은 이 테스트 소스 자체가 pre-commit C.11
/// (`println!` 리터럴 검사)에 걸리지 않게 하기 위해서다.
// 매크로 호출 판정은 사본을 두지 않는다 — 같은 술어를
// `crates/tasty-doc-guards/tests/host_writes_nothing_to_stdout.rs` 도 쓴다(그쪽 좌변은
// host 트리다). 한쪽만 고쳐지면 두 트리의 답이 갈린다.
use tasty_doc_guards::source_text::invokes_macro;
