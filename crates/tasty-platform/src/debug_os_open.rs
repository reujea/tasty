//! debug 격리: OS 열기를 실행하지 않고 기록만 한다 (`TASTY_DEBUG_OS_OPEN_LOG`).
//!
//! tasty 가 OS 기본 핸들러(브라우저 · 파일 관리자 · 시스템 설정)에 무언가를 넘기는 자리는
//! 격리 `TASTY_HOME` 과 전용 디스플레이로도 격리되지 않는다 — 브라우저는 이미 떠 있는
//! 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓 · LaunchServices)을 가져서,
//! 검증용 인스턴스가 연 것이 **사용자의 브라우저 탭**으로 나타난다. 그래서 이 변수가
//! 있으면 host 의 OS 열기 자리가 프로세스를 띄우지 않고 이 파일에 한 줄을 붙인다.
//! 무엇이 열리려 했는지는 그 줄로 판정한다.
//!
//! 변수가 **있기만 하면** 억제한다(fail closed) — 값이 비었거나 파일에 못 쓰면 경고만
//! 남기고 여전히 열지 않는다. 억제가 기록 실패에 달려 있으면 기록 경로의 오타 하나가
//! 사용자 브라우저를 다시 연다.
//!
//! 모듈 선언에 `#[cfg(debug_assertions)]` 가 붙어 release 에는 없다. 절차와 근거는
//! `docs/dev-guide/debug-ipc.md` 와 `docs/adr/0511-os-open-is-recorded-not-launched-under-a-debug-switch.md`.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

/// 이 변수가 있으면 OS 열기를 기록만 한다. 값은 기록 파일 경로다.
pub const ENV: &str = "TASTY_DEBUG_OS_OPEN_LOG";

/// OS 열기를 가로챘으면 `true` — 호출부는 그때 아무것도 띄우지 않고 성공으로 돌아간다.
///
/// `via` 는 어느 자리가 열려 했는지(`open_uri` · `open_path` …), `target` 은 넘기려던 값이다.
/// 기록 줄은 `<via>\t<target>\n` 이다.
pub fn intercepted(via: &str, target: &str) -> bool {
    record(std::env::var_os(ENV), via, target)
}

/// [`intercepted`] 의 판정만 떼어낸 것 — 환경변수를 건드리지 않고 시험하려고 둔다
/// (`set_var` 는 프로세스 전역이고 시험은 병렬로 돈다).
fn record(dest: Option<OsString>, via: &str, target: &str) -> bool {
    let Some(dest) = dest else {
        return false;
    };
    if dest.is_empty() {
        tracing::warn!(
            via,
            target,
            "{ENV} is empty; OS open suppressed without a record"
        );
        return true;
    }
    let path = Path::new(&dest);
    let appended = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{via}\t{target}"));
    if let Err(err) = appended {
        tracing::warn!(%err, via, target, path = %path.display(), "OS open suppressed but not recorded");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_variable_lets_the_open_through() {
        assert!(!record(None, "open_uri", "https://example.com"));
    }

    #[test]
    fn present_variable_records_one_line_per_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("os-open.log");
        assert!(record(
            Some(log.clone().into_os_string()),
            "open_uri",
            "file:///tmp/a"
        ));
        assert!(record(
            Some(log.clone().into_os_string()),
            "open_path",
            "/tmp/b"
        ));
        let text = std::fs::read_to_string(&log).expect("log written");
        assert_eq!(text, "open_uri\tfile:///tmp/a\nopen_path\t/tmp/b\n");
    }

    /// 기록 실패는 억제를 풀지 않는다 — 풀면 경로 오타 하나가 사용자 브라우저를 연다.
    #[test]
    fn empty_or_unwritable_destination_still_suppresses() {
        assert!(record(Some(OsString::new()), "open_uri", "https://x"));
        let dir = tempfile::tempdir().expect("tempdir");
        let unwritable = dir.path().join("missing-dir").join("os-open.log");
        assert!(record(
            Some(unwritable.clone().into_os_string()),
            "open_uri",
            "https://x"
        ));
        assert!(!unwritable.exists());
    }
}
