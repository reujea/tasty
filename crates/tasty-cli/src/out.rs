//! CLI 클라이언트 stdout 쓰기의 단일 경로 — `println!` / `print!` 대체.
//!
//! `println!` 은 stdout 쓰기 실패를 panic 으로 승격한다(`failed printing to stdout`).
//! 읽는 쪽이 파이프를 먼저 닫으면(`tasty list tree | head -1`, `| true`) EPIPE 가
//! 돌아오고, 그 panic 이 종료 코드 101 + 가짜 crash report 로 이어진다. Rust 런타임이
//! SIGPIPE 를 `SIG_IGN` 으로 두므로 프로세스가 스스로 처리해야 하고, Windows 에는
//! SIGPIPE 자체가 없어 `ErrorKind::BrokenPipe` 만 온다 — 그래서 쓰기를 `Result` 로
//! 받는다(근거: `docs/adr/0101-cli-stdout-broken-pipe-exit-zero.md`).
//!
//! - tasty-cli 는 stdout 에 [`outln!`] / [`out!`] 로만 쓴다(정책은
//!   `docs/dev-guide/error-handling.md` "stdout 쓰기", 집행은
//!   `tests/cli_stdout_broken_pipe.rs` 의 소스 스캔).
//! - `BrokenPipe` 는 [`StdoutClosed`] 로 구분돼 호출 스택을 타고 올라오고, CLI 진입점
//!   (`run_client` / `print_augmented_help` / `print_command_tree` /
//!   `try_run_plugin_cli`)이 [`quiet_if_stdout_closed`] 로 **종료 코드 0** 으로 접는다.
//!   더 쓸 곳이 없어졌을 뿐 명령이 실패한 것이 아니다.
//! - 그 외 stdout 오류(EIO / ENOSPC 등)는 일반 에러로 전파돼 `Error: …` + 종료 코드 1.
//! - stderr 도 같은 이유로 `eprintln!` 을 쓰지 않는다 — [`errln!`] 을 쓴다. stderr 는 CLI 의
//!   마지막 보고 채널이라 그 쓰기의 실패는 알릴 곳이 없으므로 **버리고**, 명령의 종료 코드는
//!   그대로 둔다(`docs/adr/0513-cli-stderr-broken-pipe-keeps-the-exit-code.md`).
//! - host(GUI / headless) 는 stdout 에 쓰지 않으므로 이 모듈과 무관하다 — `Routed::Gui`
//!   갈래의 동작은 바뀌지 않는다.

use std::fmt;
use std::io::{self, Write};

/// 읽는 쪽이 stdout 파이프를 닫았다(EPIPE / `ErrorKind::BrokenPipe`).
///
/// 실패가 아니라 "더 이상 출력이 필요 없다" 는 신호다. 호출 스택을 `?` 로 타고 올라와
/// CLI 진입점에서 [`quiet_if_stdout_closed`] 가 종료 코드 0 으로 접는다.
#[derive(Debug, thiserror::Error)]
#[error("stdout closed by reader (broken pipe)")]
pub struct StdoutClosed;

/// stdout 쓰기 오류를 분류한다 — `BrokenPipe` 만 [`StdoutClosed`] 로, 나머지는 문맥을
/// 붙인 일반 에러로.
fn classify(err: io::Error) -> anyhow::Error {
    if err.kind() == io::ErrorKind::BrokenPipe {
        StdoutClosed.into()
    } else {
        anyhow::Error::new(err).context(tasty_i18n::t("cli.out.write_failed").to_string())
    }
}

/// stdout 에 직접 쓰는 외부 코드(clap `print_help` 등)의 `io::Result` 를 같은 규칙으로
/// 분류한다.
pub fn from_io(result: io::Result<()>) -> anyhow::Result<()> {
    result.map_err(classify)
}

/// `println!` 대체 — `args` 뒤에 개행. [`outln!`] 이 이 함수를 부른다.
pub fn line(args: fmt::Arguments<'_>) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_fmt(args)
        .and_then(|()| stdout.write_all(b"\n"))
        .map_err(classify)
}

/// `print!` 대체 — 개행 없이 쓴다. [`out!`] 이 이 함수를 부른다. 버퍼링은 std 와
/// 같다(LineWriter: 개행 또는 [`flush`] 시점에 실제 write 가 일어난다).
pub fn text(args: fmt::Arguments<'_>) -> anyhow::Result<()> {
    io::stdout().lock().write_fmt(args).map_err(classify)
}

/// stdout flush — `std::io::stdout().flush()` 대체. 개행 없는 [`text`] 출력을 밀어낼 때
/// 쓴다. 파이프 생존 프로브가 아니다: 버퍼가 비어 있으면 write(2) 를 내지 않아 EPIPE 를
/// 감지하지 못한다 — 읽는 쪽 닫힘은 다음 실제 write([`line`]/[`text`]+flush)에서 잡힌다.
pub fn flush() -> anyhow::Result<()> {
    io::stdout().lock().flush().map_err(classify)
}

/// `eprintln!` 대체 — stderr 에 `args` 뒤 개행. [`errln!`] 이 이 함수를 부른다.
///
/// `eprintln!` 은 stderr 쓰기 실패를 panic 으로 승격한다(`failed printing to stderr`) — 읽는
/// 쪽이 먼저 닫힌 `2>&1 | head` 에서 그 panic 이 crash report 가 된다. stderr 는 CLI 가 실패를
/// 알리는 마지막 채널이라 그 쓰기가 실패하면 **더 알릴 곳이 없다.** 그래서 결과를 버리고,
/// 실패의 신호는 호출자가 이어서 내는 종료 코드가 나른다.
pub fn err_line(args: fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let written = stderr
        .write_fmt(args)
        .and_then(|()| stderr.write_all(b"\n"));
    // stderr 가 닫혔다 — 이 실패를 보고할 채널이 남아 있지 않다. 종료 코드는 호출자가 정한다.
    drop(written);
}

/// `err` 가 stdout 닫힘([`StdoutClosed`])에서 비롯됐는가. `context()` 로 감싸인 경우도
/// 원인 체인을 따라 찾는다.
pub fn is_stdout_closed(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|e| e.downcast_ref::<StdoutClosed>().is_some())
}

/// CLI 진입점의 경계 처리 — stdout 닫힘은 조용한 성공(종료 코드 0), 나머지는 그대로.
pub fn quiet_if_stdout_closed(result: anyhow::Result<()>) -> anyhow::Result<()> {
    match result {
        Err(e) if is_stdout_closed(&e) => Ok(()),
        other => other,
    }
}

/// `println!` 대체. `outln!()?` / `outln!("{}", v)?` — 값은 `anyhow::Result<()>`.
macro_rules! outln {
    () => {
        $crate::out::line(::std::format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::out::line(::std::format_args!($($arg)*))
    };
}

/// `print!` 대체(개행 없음). 값은 `anyhow::Result<()>`.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::out::text(::std::format_args!($($arg)*))
    };
}

/// `eprintln!` 대체. 값이 없다 — stderr 쓰기 실패는 [`err_line`] 이 버린다.
macro_rules! errln {
    () => {
        $crate::out::err_line(::std::format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::out::err_line(::std::format_args!($($arg)*))
    };
}

pub(crate) use {errln, out, outln};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_pipe_is_classified_as_stdout_closed() {
        let err = classify(io::Error::from(io::ErrorKind::BrokenPipe));
        assert!(is_stdout_closed(&err));
        assert!(quiet_if_stdout_closed(Err(err)).is_ok());
    }

    #[test]
    fn stdout_closed_is_found_through_context_layers() {
        let err = classify(io::Error::from(io::ErrorKind::BrokenPipe))
            .context("outer")
            .context("outermost");
        assert!(is_stdout_closed(&err));
    }

    #[test]
    fn other_io_errors_stay_errors() {
        let err = classify(io::Error::other("disk full"));
        assert!(!is_stdout_closed(&err));
        assert!(quiet_if_stdout_closed(Err(err)).is_err());
        assert!(quiet_if_stdout_closed(Err(anyhow::anyhow!("unrelated"))).is_err());
        assert!(quiet_if_stdout_closed(Ok(())).is_ok());
    }

    #[test]
    fn from_io_maps_broken_pipe_only() {
        assert!(is_stdout_closed(
            &from_io(Err(io::Error::from(io::ErrorKind::BrokenPipe))).unwrap_err()
        ));
        assert!(!is_stdout_closed(
            &from_io(Err(io::Error::from(io::ErrorKind::PermissionDenied))).unwrap_err()
        ));
        assert!(from_io(Ok(())).is_ok());
    }
}
