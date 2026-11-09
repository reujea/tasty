//! 설정한 셸 경로의 판정. 런타임 대체, 부팅 판정, 설정 창 경고, 첫 실행 셸 설정 화면이 같은 판정을 쓴다.
//!
//! macOS·Linux는 셸 이름을 보지 않는다. fish처럼 bash·zsh가 아닌 셸도 실행할 수 있으면 쓴다.
//! 경로가 디렉터리이거나 실행 권한이 없으면 원인을 따로 알린다. 이 두 경우는 다른 셸로 대신하지
//! 않는다 — 사용자가 고른 경로가 왜 실패하는지 보여 주고 고치게 한다.
//! Windows는 Git Bash만 쓴다([`tasty_utils::shell_family::is_git_for_windows_bash`]). 값이 비었을 때만 탐지한
//! Git Bash를 쓰고, 그 밖의 값은 대신하지 않는다 — 저장한 셸이 왜 안 되는지 설정 화면에서 보여 주고 고치게 한다.

use std::path::Path;

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
    /// `path`를 판정한다.
    pub fn of(path: &str) -> Self {
        if path.is_empty() {
            return Self::Empty;
        }
        let path = Path::new(path);
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
        use super::ShellPathStatus;
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
        #[test]
        fn only_an_empty_value_falls_back() {
            for value in [
                "cmd.exe",
                "pwsh",
                "nu",
                "/bin/zsh",
                r"C:\nonexistent\Git\bin\bash.exe",
            ] {
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
