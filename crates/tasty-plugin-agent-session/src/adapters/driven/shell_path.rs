//! 에이전트 CLI(`claude`·`pi`)를 찾을 PATH — 사용자의 **대화형 로그인 셸**에서 받는다.
//!
//! tasty 를 앱 아이콘·런처 스크립트로 띄우면 plugin 이 물려받는 PATH 에 CLI 설치 위치가 없을 수
//! 있다(실측 2026-11-02: `~/.local/bin` 이 빠져 `claude` 를 못 띄웠다 — os error 2). v1 supervisor
//! 데몬 기동(`daemon_launch.rs`)과 같은 방법이다.
//!
//! 셸을 띄우는 값은 plugin 프로세스당 한 번만 치른다 — [`warm`] 으로 기동 때 미리 받아 둔다.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
#[cfg(all(unix, not(test)))]
use std::time::Duration;

/// 로그인 셸에서 PATH 를 받는 상한 — 넘기면 이 프로세스의 PATH 를 쓴다.
#[cfg(all(unix, not(test)))]
const SHELL_PATH_WAIT: Duration = Duration::from_secs(5);
#[cfg(all(unix, not(test)))]
const SHELL_PATH_POLL: Duration = Duration::from_millis(50);
#[cfg(unix)]
const PATH_MARKER: &str = "__TASTY_AGENT_PATH__=";

static SPAWN_PATH: OnceLock<Option<OsString>> = OnceLock::new();

/// 기동 때 불러 첫 세션이 셸을 기다리지 않게 한다.
pub fn warm() {
    std::thread::spawn(|| {
        get();
    });
}

/// 자식이 쓸 PATH — 로그인 셸에서 받고, 못 받으면 이 프로세스의 PATH 다.
pub fn get() -> Option<&'static OsStr> {
    SPAWN_PATH
        .get_or_init(|| login_shell_path().or_else(|| std::env::var_os("PATH")))
        .as_deref()
}

/// 에이전트 CLI 를 띄울 명령 — 이름은 PATH 에서 찾은 절대 경로로 바꾸고, 그 PATH 를 자식에 물린다.
/// 프로필 `env` 의 `PATH`(`profile_path`)가 있으면 그것이 이긴다.
pub fn command(program: &str, profile_path: Option<&String>) -> std::process::Command {
    let path = match profile_path {
        Some(p) => Some(OsStr::new(p.as_str())),
        None => get(),
    };
    let mut cmd = std::process::Command::new(resolve(program, path));
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    cmd
}

/// 경로 구분자가 없는 이름이면 `path` 에서 찾아 절대 경로로 바꾼다. 못 찾으면 그대로 둔다 —
/// 그러면 spawn 이 원래 오류(`No such file or directory`)를 낸다.
pub fn resolve(program: &str, path: Option<&OsStr>) -> PathBuf {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(program);
    }
    let name = format!("{program}{}", std::env::consts::EXE_SUFFIX);
    path.and_then(|p| {
        std::env::split_paths(p)
            .map(|dir| dir.join(&name))
            .find(|c| is_executable(c))
    })
    .unwrap_or_else(|| PathBuf::from(program))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// 시험은 사용자 셸에 기대지 않는다 — 느리고 기계마다 다르다.
#[cfg(any(not(unix), test))]
fn login_shell_path() -> Option<OsString> {
    None
}

/// 사용자의 대화형 로그인 셸이 쓰는 PATH. rc 파일이 찍는 군말과 가르려고 표지를 붙여 읽는다.
#[cfg(all(unix, not(test)))]
fn login_shell_path() -> Option<OsString> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;
    let shell = std::env::var_os("SHELL").filter(|s| !s.is_empty())?;
    let mut child = Command::new(&shell)
        .arg("-ilc")
        .arg(format!("printf '\\n{PATH_MARKER}%s\\n' \"$PATH\""))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| tracing::warn!("로그인 셸에서 PATH 를 못 받는다: {e}"))
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        stdout.read_to_string(&mut s).map(|_| s)
    });
    let deadline = Instant::now() + SHELL_PATH_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(SHELL_PATH_POLL),
            Ok(None) | Err(_) => {
                tracing::warn!("로그인 셸이 PATH 를 안 내놓는다 — plugin 의 PATH 를 쓴다");
                if let Err(e) = child.kill() {
                    tracing::warn!("로그인 셸을 못 끝냈다: {e}");
                }
                if let Err(e) = child.wait() {
                    tracing::warn!("로그인 셸을 못 거뒀다: {e}");
                }
                return None;
            }
        }
    }
    let out = reader.join().ok()?.ok()?;
    parse_marked_path(&out).map(OsString::from)
}

#[cfg(unix)]
fn parse_marked_path(out: &str) -> Option<&str> {
    out.lines()
        .rev()
        .find_map(|l| l.strip_prefix(PATH_MARKER))
        .filter(|p| !p.is_empty())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("tasty-shell-path-{tag}-{}-{n}", std::process::id()));
        // 지난 실행이 남긴 폴더만 치운다 — 없으면 실패하는 것이 정상이다.
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_marked_path_is_read_past_rc_noise() {
        let out = format!("banner\n{PATH_MARKER}/a:/b\n");
        assert_eq!(parse_marked_path(&out), Some("/a:/b"));
        assert_eq!(parse_marked_path("no marker"), None);
    }

    /// 앱으로 띄운 PATH 에 없던 디렉터리라도, 받은 PATH 에 있으면 절대 경로로 찾는다.
    #[test]
    fn a_bare_name_resolves_through_the_given_path() {
        use std::os::unix::fs::PermissionsExt;
        let empty = scratch("empty");
        let bin = scratch("bin");
        let exe = bin.join("fakeagent");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths([&empty, &bin]).unwrap();
        assert_eq!(resolve("fakeagent", Some(&path)), exe);
    }

    /// 실행 권한 없는 같은 이름 파일은 건너뛰고, 못 찾으면 이름을 그대로 둔다.
    #[test]
    fn unresolvable_names_and_paths_stay_as_given() {
        let d = scratch("noexec");
        std::fs::write(d.join("fakeagent"), "").unwrap();
        let path = d.clone().into_os_string();
        assert_eq!(
            resolve("fakeagent", Some(&path)),
            PathBuf::from("fakeagent")
        );
        assert_eq!(resolve("fakeagent", None), PathBuf::from("fakeagent"));
        assert_eq!(
            resolve("/abs/claude", Some(&path)),
            PathBuf::from("/abs/claude")
        );
    }
}
