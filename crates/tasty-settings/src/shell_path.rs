//! 설정한 셸 경로의 해석과 판정. 런타임 대체, 부팅 판정, 설정 창 경고, 첫 실행 셸 설정 화면, spawn이
//! 같은 해석 결과를 쓴다.
//!
//! 값은 경로 해석 유틸([`tasty_utils::path_resolve`])로 푼다. 구분자가 없는 이름(`zsh`)은 자식 셸이 받을
//! PATH([`tasty_utils::process::shell_child_path`])에서 찾고, 구분자가 있는 상대 값(`./sh`, `rel/sh`)은
//! Tasty를 실행한 폴더 기준이다. 판정한 절대 경로를 그대로 실행에 넘기므로 판정과 실행이 같은 파일을 본다.
//!
//! macOS·Linux는 셸 이름을 보지 않는다. fish처럼 bash·zsh가 아닌 셸도 실행할 수 있으면 쓴다.
//! 경로가 디렉터리이거나 실행 권한이 없으면 원인을 따로 알린다. 이 두 경우는 다른 셸로 대신하지
//! 않는다 — 사용자가 고른 경로가 왜 실패하는지 보여 주고 고치게 한다.
//! Windows는 Git Bash만 쓴다([`tasty_utils::shell_family::is_git_for_windows_bash`]). 값이 비었을 때만 탐지한
//! Git Bash를 쓰고, 그 밖의 값은 대신하지 않는다 — 저장한 셸이 왜 안 되는지 설정 화면에서 보여 주고 고치게 한다.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tasty_utils::path_resolve::{locate_command_in, resolve_command};

/// 셸 설정값을 해석한 결과. `path`는 판정에 쓴 절대 경로이며 실행에도 이 값을 넘긴다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedShell {
    pub status: ShellPathStatus,
    /// 해석하지 못했으면(빈 값, PATH에 없는 이름, 풀 수 없는 경로, 문자열로 손실 없이 표현할 수 없는 경로) `None`.
    pub path: Option<PathBuf>,
}

impl ResolvedShell {
    /// `raw`를 자식 셸이 받을 PATH로 해석·판정한다.
    pub fn of(raw: &str) -> Self {
        let search_path = tasty_utils::process::shell_child_path(&[]).unwrap_or_default();
        Self::with_search_path(raw, &search_path)
    }

    /// PATH를 호출자가 준다. 실행 폴더는 경로 해석 유틸의 고정값이다.
    pub fn with_search_path(raw: &str, search_path: &OsStr) -> Self {
        let missing = Self {
            status: ShellPathStatus::Missing,
            path: None,
        };
        if raw.is_empty() {
            return Self {
                status: ShellPathStatus::Empty,
                path: None,
            };
        }
        let Ok(target) = resolve_command(raw) else {
            return missing;
        };
        let Ok(path) = locate_command_in(&target, search_path) else {
            return missing;
        };
        // 실행은 문자열 셸 경로로 넘어간다. 손실 없이 문자열로 표현할 수 없는 경로(비 UTF-8 실행 폴더 등)는
        // 판정한 파일과 실행할 파일이 달라지므로 해석하지 못한 값으로 본다.
        if path.to_str().is_none() {
            return missing;
        }
        Self {
            status: ShellPathStatus::judge(&path),
            path: Some(path),
        }
    }
}

/// 부팅할 때 쓸 셸의 판정([`crate::GeneralSettings::boot_shell_choice`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootShell {
    /// 설정한 셸을 그대로 쓴다.
    Configured,
    /// 설정한 셸을 대신해 탐지한 셸을 쓴다.
    Fallback(String),
    /// 쓸 셸이 없다. 첫 실행 셸 설정 화면을 띄운다.
    Unusable,
}

/// 셸 경로 판정 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellPathStatus {
    /// 값이 비었다. 실행할 때 탐지한 셸을 쓴다.
    Empty,
    /// 경로에 아무것도 없다. macOS·Linux는 실행할 때 탐지한 셸로 대신한다. Windows는 대신하지 않고 실행에 실패한다.
    Missing,
    /// Windows 전용. 경로에 무언가 있지만 Git Bash가 아니다. 대신하지 않고 실행에 실패한다.
    NotShell,
    /// macOS·Linux 전용. 경로가 디렉터리다. 대신하지 않고 실행에 실패한다.
    Directory,
    /// macOS·Linux 전용. 파일에 실행 권한이 없다. 대신하지 않고 실행에 실패한다.
    NotExecutable,
    /// 쓸 수 있는 셸이다.
    Usable,
}

impl ShellPathStatus {
    /// 설정값 `path`를 해석해 판정한다([`ResolvedShell::of`]).
    pub fn of(path: &str) -> Self {
        ResolvedShell::of(path).status
    }

    /// 해석이 끝난 절대 경로를 판정한다.
    fn judge(path: &Path) -> Self {
        if !path.exists() {
            return Self::Missing;
        }
        #[cfg(windows)]
        {
            if tasty_utils::shell_family::is_git_for_windows_bash(path) {
                Self::Usable
            } else {
                Self::NotShell
            }
        }
        #[cfg(not(windows))]
        {
            if path.is_dir() {
                return Self::Directory;
            }
            if executable(path) {
                Self::Usable
            } else {
                Self::NotExecutable
            }
        }
    }

    /// 실행할 때 탐지한 셸로 대신하는 판정인지. 실행 불가(디렉터리·권한 없음)는 대신하지 않고,
    /// Windows는 빈 값만 대신한다.
    pub fn falls_back(self) -> bool {
        #[cfg(windows)]
        {
            matches!(self, Self::Empty)
        }
        #[cfg(not(windows))]
        {
            matches!(self, Self::Empty | Self::Missing | Self::NotShell)
        }
    }
}

/// 이 프로세스가 `path`를 실행할 수 있는지. portable-pty가 실행 전에 쓰는 검사와 같은 `access(X_OK)`다.
#[cfg(not(windows))]
fn executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c_path는 이 호출 동안 살아 있는 NUL 종료 문자열이다. access는 경로를 읽기만 한다.
    unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(test)]
mod tests {
    use super::ShellPathStatus;

    #[test]
    fn an_empty_value_falls_back_and_a_missing_path_is_reported() {
        assert_eq!(ShellPathStatus::of(""), ShellPathStatus::Empty);
        assert_eq!(
            ShellPathStatus::of("/nonexistent/tasty-shell-path/fish"),
            ShellPathStatus::Missing
        );
        assert!(ShellPathStatus::Empty.falls_back());
    }

    #[test]
    fn unusable_but_present_paths_do_not_fall_back() {
        assert!(!ShellPathStatus::Directory.falls_back());
        assert!(!ShellPathStatus::NotExecutable.falls_back());
        assert!(!ShellPathStatus::Usable.falls_back());
    }

    #[cfg(not(windows))]
    mod unix {
        use super::super::ResolvedShell;
        use super::ShellPathStatus;
        use std::ffi::OsStr;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn a_missing_path_falls_back() {
            assert!(ShellPathStatus::Missing.falls_back());
            assert!(ShellPathStatus::NotShell.falls_back());
        }

        fn file_with_mode(dir: &std::path::Path, name: &str, mode: u32) -> String {
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\n").expect("write fixture");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .expect("chmod fixture");
            path.to_string_lossy().into_owned()
        }

        /// 이름에 bash·zsh가 없어도 실행할 수 있으면 쓴다.
        #[test]
        fn an_executable_named_fish_is_usable() {
            let dir = tempfile::tempdir().expect("tempdir");
            let fish = file_with_mode(dir.path(), "fish", 0o755);
            assert_eq!(ShellPathStatus::of(&fish), ShellPathStatus::Usable);
        }

        #[test]
        fn a_directory_is_reported_as_a_directory() {
            let dir = tempfile::tempdir().expect("tempdir");
            assert_eq!(
                ShellPathStatus::of(&dir.path().to_string_lossy()),
                ShellPathStatus::Directory
            );
        }

        /// 실행 폴더를 이 스레드에서만 바꾸고 끝나면 되돌린다.
        struct Launch(tempfile::TempDir);

        impl Launch {
            fn new() -> Self {
                let dir = tempfile::tempdir().expect("tempdir");
                tasty_utils::path_resolve::push_launch_dir_override(
                    tasty_utils::path_resolve::LaunchDir::Known(dir.path().to_path_buf()),
                );
                Launch(dir)
            }

            fn path(&self) -> &std::path::Path {
                self.0.path()
            }
        }

        impl Drop for Launch {
            fn drop(&mut self) {
                tasty_utils::path_resolve::pop_launch_dir_override();
            }
        }

        fn exec(path: &std::path::Path) {
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, "#!/bin/sh\n").expect("write fixture");
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod fixture");
        }

        /// `./sh`·`../x/sh`·`rel/sh`는 실행 폴더 기준 절대 경로로 판정하고 그 경로를 실행에 넘긴다.
        #[test]
        fn relative_values_are_judged_at_the_launch_dir() {
            let launch = Launch::new();
            exec(&launch.path().join("sh"));
            exec(&launch.path().join("rel/sh"));
            let base = launch.path().file_name().expect("name").to_string_lossy();
            for (raw, expected) in [
                ("./sh".to_string(), launch.path().join("sh")),
                ("rel/sh".to_string(), launch.path().join("rel/sh")),
                (
                    format!("../{base}/sh"),
                    launch.path().join(format!("../{base}/sh")),
                ),
            ] {
                let resolved = ResolvedShell::with_search_path(&raw, OsStr::new("/nonexistent"));
                assert_eq!(resolved.status, ShellPathStatus::Usable, "{raw}");
                assert_eq!(resolved.path.as_deref(), Some(expected.as_path()), "{raw}");
            }
        }

        /// 이름만 쓴 값은 PATH에서 찾는다. 실행 폴더의 같은 이름 파일은 보지 않는다.
        #[test]
        fn a_bare_name_is_judged_by_the_file_found_on_path() {
            let launch = Launch::new();
            let bin = launch.path().join("bin");
            exec(&launch.path().join("zsh"));
            exec(&bin.join("zsh"));
            let resolved = ResolvedShell::with_search_path("zsh", bin.as_os_str());
            assert_eq!(resolved.status, ShellPathStatus::Usable);
            assert_eq!(resolved.path, Some(bin.join("zsh")));

            let missing = ResolvedShell::with_search_path("zsh", OsStr::new("/nonexistent"));
            assert_eq!(missing.status, ShellPathStatus::Missing);
            assert_eq!(missing.path, None);
            assert!(missing.status.falls_back());
        }

        /// PATH의 상대 항목은 터미널 작업 폴더가 아니라 실행 폴더 기준이다.
        #[test]
        fn a_relative_path_entry_is_anchored_at_the_launch_dir() {
            let launch = Launch::new();
            exec(&launch.path().join("rel/sh"));
            let resolved = ResolvedShell::with_search_path("sh", OsStr::new("rel"));
            assert_eq!(resolved.path, Some(launch.path().join("rel/sh")));
            assert_eq!(resolved.status, ShellPathStatus::Usable);
        }

        /// 문자열로 손실 없이 표현할 수 없는 해석 결과는 실행할 경로와 달라지므로 해석하지 못한 값으로 본다.
        /// macOS APFS는 UTF-8이 아닌 이름의 폴더를 만들지 못하므로 Linux에서만 돈다.
        #[cfg(target_os = "linux")]
        #[test]
        fn a_path_under_a_non_utf8_launch_dir_is_not_usable() {
            use std::os::unix::ffi::OsStrExt;
            let root = tempfile::tempdir().expect("tempdir");
            let launch = root.path().join(OsStr::from_bytes(b"\xff-launch"));
            exec(&launch.join("sh"));
            tasty_utils::path_resolve::push_launch_dir_override(
                tasty_utils::path_resolve::LaunchDir::Known(launch.clone()),
            );
            let resolved = ResolvedShell::with_search_path("./sh", OsStr::new(""));
            tasty_utils::path_resolve::pop_launch_dir_override();
            assert_eq!(resolved.status, ShellPathStatus::Missing);
            assert_eq!(resolved.path, None);
        }

        /// 실행 폴더 기준으로 디렉터리이거나 권한 없는 상대 값은 대신하지 않는 판정이다.
        #[test]
        fn unusable_relative_values_keep_their_cause() {
            let launch = Launch::new();
            std::fs::create_dir(launch.path().join("shdir")).expect("dir");
            file_with_mode(launch.path(), "plain", 0o644);
            let dir = ResolvedShell::with_search_path("./shdir", OsStr::new(""));
            assert_eq!(dir.status, ShellPathStatus::Directory);
            let plain = ResolvedShell::with_search_path("./plain", OsStr::new(""));
            assert_eq!(plain.status, ShellPathStatus::NotExecutable);
            let gone = ResolvedShell::with_search_path("./gone", OsStr::new(""));
            assert_eq!(gone.status, ShellPathStatus::Missing);
            assert_eq!(gone.path, Some(launch.path().join("gone")));
        }

        #[test]
        fn a_file_without_execute_permission_is_not_executable() {
            let dir = tempfile::tempdir().expect("tempdir");
            let plain = file_with_mode(dir.path(), "zsh", 0o644);
            assert_eq!(ShellPathStatus::of(&plain), ShellPathStatus::NotExecutable);
        }
    }

    /// Windows는 Git Bash 배치만 허용한다. 이 시험은 Windows 빌드에서만 돈다.
    #[cfg(windows)]
    mod windows {
        use super::ShellPathStatus;
        use tasty_utils::shell_family::ShellFamily;

        fn files(root: &std::path::Path, names: &[&str]) {
            for name in names {
                let path = root.join(name);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
                std::fs::write(&path, "").expect("write fixture");
            }
        }

        fn of(path: &std::path::Path) -> ShellPathStatus {
            ShellPathStatus::of(&path.to_string_lossy())
        }

        #[test]
        fn only_a_git_for_windows_layout_is_usable() {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path().join("other place").join("PG");
            files(
                &root,
                &[
                    "bin/bash.exe",
                    "usr/bin/bash.exe",
                    "cmd/git.exe",
                    "usr/bin/msys-2.0.dll",
                    "git-bash.exe",
                ],
            );
            assert_eq!(of(&root.join("bin/bash.exe")), ShellPathStatus::Usable);
            assert_eq!(of(&root.join(r"bin\bash.exe")), ShellPathStatus::Usable);
            assert_eq!(of(&root.join("usr/bin/bash.exe")), ShellPathStatus::Usable);
            assert_eq!(of(&root.join("git-bash.exe")), ShellPathStatus::NotShell);

            let msys2 = dir.path().join("msys64");
            files(
                &msys2,
                &[
                    "usr/bin/bash.exe",
                    "usr/bin/zsh.exe",
                    "usr/bin/msys-2.0.dll",
                ],
            );
            assert_eq!(
                of(&msys2.join("usr/bin/bash.exe")),
                ShellPathStatus::NotShell
            );
            assert_eq!(
                of(&msys2.join("usr/bin/zsh.exe")),
                ShellPathStatus::NotShell
            );

            let system32 = dir.path().join("System32");
            files(
                &system32,
                &[
                    "bash.exe",
                    "cmd.exe",
                    "powershell.exe",
                    "pwsh.exe",
                    "nu.exe",
                ],
            );
            for name in [
                "bash.exe",
                "cmd.exe",
                "powershell.exe",
                "pwsh.exe",
                "nu.exe",
            ] {
                assert_eq!(
                    of(&system32.join(name)),
                    ShellPathStatus::NotShell,
                    "{name}"
                );
            }

            let folder = dir.path().join("folder").join("bash.exe");
            std::fs::create_dir_all(&folder).expect("fixture dir");
            assert_eq!(of(&folder), ShellPathStatus::NotShell);
        }

        /// 판정이 통과한 셸은 bash 통합을 받는다. `git-bash.exe`처럼 판정과 통합이 어긋나지 않는다.
        #[test]
        fn a_usable_shell_always_gets_the_bash_integration() {
            let dir = tempfile::tempdir().expect("tempdir");
            files(
                dir.path(),
                &["BIN/BASH.EXE", "cmd/git.exe", "usr/bin/msys-2.0.dll"],
            );
            let shell = dir.path().join("BIN/BASH.EXE");
            assert_eq!(of(&shell), ShellPathStatus::Usable);
            assert_eq!(
                ShellFamily::detect(&shell.to_string_lossy()),
                ShellFamily::Bash
            );
        }

        /// 이름만 있는 값과 없는 경로, 거부한 파일은 어느 것도 다른 셸로 대신하지 않는다.
        /// 이름만 있는 값은 PATH에서 찾으므로 찾으면 `NotShell`, 못 찾으면 `Missing`이다.
        #[test]
        fn only_an_empty_value_falls_back() {
            for value in ["cmd.exe", "pwsh", "nu"] {
                let status = ShellPathStatus::of(value);
                assert!(
                    matches!(status, ShellPathStatus::Missing | ShellPathStatus::NotShell),
                    "{value}: {status:?}"
                );
                assert!(!status.falls_back(), "{value}");
            }
            for value in ["/bin/zsh", r"C:\nonexistent\Git\bin\bash.exe"] {
                let status = ShellPathStatus::of(value);
                assert_eq!(status, ShellPathStatus::Missing, "{value}");
                assert!(!status.falls_back(), "{value}");
            }
            assert!(ShellPathStatus::Empty.falls_back());
            assert!(!ShellPathStatus::Missing.falls_back());
            assert!(!ShellPathStatus::NotShell.falls_back());
        }
    }
}
