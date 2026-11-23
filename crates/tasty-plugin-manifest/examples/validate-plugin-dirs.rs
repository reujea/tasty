//! 플러그인 폴더의 `tasty-plugin.toml`을 현재 호스트의 파서와 검증기([`Manifest::load`])로 읽는다.
//! 공식 플러그인 릴리스에서 받은 매니페스트가 이 tasty와 맞는지 받는 단계에서 확인하는 데 쓴다
//! (`scripts/fetch-official-plugins.sh`). 감지기·핸들러 본문처럼 호스트 타입이 필요한 검사는
//! 하지 않는다. 그 검사는 호스트가 플러그인을 등록할 때 한다.
//!
//! ```text
//! cargo run -p tasty-plugin-manifest --example validate-plugin-dirs -- [--tasty-version <X.Y.Z>] <plugin dir>...
//! ```
//!
//! `--tasty-version`을 주면 매니페스트의 `min_tasty_version`을 그 본체 버전과도 비교한다.
//! 배포 패키징은 패키징하는 본체 버전을 넘겨 그 판이 읽지 못할 플러그인을 받는 단계에서 막는다.
//!
//! 표준 출력에 폴더마다 `valid\t<id>\t<version>\t<폴더>`, 표준 오류에 `invalid\t<폴더>\t<이유>`를 쓴다.
//! 이 줄들은 로그가 아니라 호출 스크립트가 읽는 판정 출력이라 `tracing`을 쓰지 않는다.
//! 종료코드: 0은 모두 통과, 1은 하나 이상 실패, 2는 인자 없음·잘못된 `--tasty-version`·출력 실패.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use tasty_plugin_manifest::Manifest;

/// 출력에 실패하면 판정을 알릴 수 없으므로 종료코드 2로 끝낸다.
fn run(tasty_version: Option<&std::ffi::OsStr>, dirs: &[PathBuf]) -> std::io::Result<ExitCode> {
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    if let Some(version) = tasty_version {
        let set = tasty_plugin_manifest::set_host_tasty_version(&version.to_string_lossy());
        if let Err(e) = set {
            writeln!(err, "{e}")?;
            return Ok(ExitCode::from(2));
        }
    }
    if dirs.is_empty() {
        writeln!(
            err,
            "usage: validate-plugin-dirs [--tasty-version <X.Y.Z>] <plugin dir>..."
        )?;
        return Ok(ExitCode::from(2));
    }
    let mut failed = 0usize;
    for dir in dirs {
        match Manifest::load(dir).and_then(|m| m.require_host_tasty().map(|()| m)) {
            Ok(m) => writeln!(out, "valid\t{}\t{}\t{}", m.id, m.version, dir.display())?,
            Err(e) => {
                writeln!(err, "invalid\t{}\t{e:#}", dir.display())?;
                failed += 1;
            }
        }
    }
    if failed == 0 {
        return Ok(ExitCode::SUCCESS);
    }
    writeln!(err, "{failed} of {} manifest(s) failed", dirs.len())?;
    Ok(ExitCode::from(1))
}

fn main() -> ExitCode {
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let mut tasty_version = None;
    if args.first().is_some_and(|a| a == "--tasty-version") {
        tasty_version = Some(args.get(1).cloned().unwrap_or_default());
        args.drain(..2.min(args.len()));
    }
    let dirs: Vec<PathBuf> = args.into_iter().map(PathBuf::from).collect();
    run(tasty_version.as_deref(), &dirs).unwrap_or(ExitCode::from(2))
}
