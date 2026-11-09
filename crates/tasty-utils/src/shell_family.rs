//! 실행 파일의 basename으로 bash/zsh/기타를 구분한다. tasty-settings의 셸 통합 설정에 사용한다.
//! Other는 bashrc/zshenv 자동 주입을 적용하지 않는 계열이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellFamily {
    Bash,
    Zsh,
    Other,
}

impl ShellFamily {
    /// 대소문자와 .exe 접미사를 무시하고 basename을 비교한다.
    /// symlink는 해석하지 않으므로 bash를 가리키는 /bin/sh도 Other다.
    pub fn detect(shell_path: &str) -> Self {
        let stem = std::path::Path::new(shell_path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let stem = stem.strip_suffix(".exe").unwrap_or(&stem);
        match stem {
            "bash" => Self::Bash,
            "zsh" => Self::Zsh,
            _ => Self::Other,
        }
    }
}

/// Git for Windows의 셸인지 파일 배치로 판별한다. 프로세스를 띄우지 않아 프레임마다 불러도 된다.
///
/// 파일 이름이 `bash.exe`이고 `<루트>\bin` 또는 `<루트>\usr\bin`에 있으며, 그 루트에 `cmd\git.exe`와
/// `usr\bin\msys-2.0.dll`이 있어야 한다. 루트의 위치는 보지 않으므로 다른 곳에 설치했거나 Portable이어도
/// 통과한다. `cmd\git.exe`는 MSYS2에 없고, 위치 조건이 `System32\bash.exe`(WSL)를 거른다. 이름 조건은
/// [`ShellFamily::detect`]가 `Bash`를 내는 조건에 포함되므로 통과한 셸은 항상 bash 통합을 받는다.
/// 레지스트리는 Portable을 모르고 `mingw64` 폴더는 판마다 달라서(`ucrt64`, `clangarm64`) 쓰지 않는다.
pub fn is_git_for_windows_bash(path: &std::path::Path) -> bool {
    fn named(path: &std::path::Path, name: &str) -> bool {
        path.file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|f| f.eq_ignore_ascii_case(name))
    }
    fn is_git_root(root: &std::path::Path) -> bool {
        root.join("cmd").join("git.exe").is_file()
            && root.join("usr").join("bin").join("msys-2.0.dll").is_file()
    }
    if !path.is_absolute() || !named(path, "bash.exe") || !path.is_file() {
        return false;
    }
    let Some(above_bin) = path
        .parent()
        .filter(|bin| named(bin, "bin"))
        .and_then(|bin| bin.parent())
    else {
        return false;
    };
    // 루트 자체의 이름이 usr일 수도 있으므로 `usr\bin`이어도 두 자리를 모두 본다.
    is_git_root(above_bin)
        || (named(above_bin, "usr") && above_bin.parent().is_some_and(is_git_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_bash_by_basename() {
        assert_eq!(ShellFamily::detect("/bin/bash"), ShellFamily::Bash);
        assert_eq!(
            ShellFamily::detect("C:/Program Files/Git/bin/bash.exe"),
            ShellFamily::Bash
        );
        assert_eq!(ShellFamily::detect("BASH.EXE"), ShellFamily::Bash);
    }

    #[test]
    fn detects_zsh_by_basename() {
        assert_eq!(ShellFamily::detect("/bin/zsh"), ShellFamily::Zsh);
        assert_eq!(ShellFamily::detect("/usr/local/bin/zsh"), ShellFamily::Zsh);
    }

    #[test]
    fn other_shells_and_empty_fall_back_to_other() {
        assert_eq!(ShellFamily::detect("/bin/fish"), ShellFamily::Other);
        assert_eq!(ShellFamily::detect("/usr/bin/sh"), ShellFamily::Other);
        assert_eq!(ShellFamily::detect(""), ShellFamily::Other);
        assert_eq!(ShellFamily::detect("cmd.exe"), ShellFamily::Other);
    }

    #[test]
    fn symlinked_or_sh_compat_bash_is_not_detected() {
        assert_eq!(ShellFamily::detect("/bin/sh"), ShellFamily::Other);
    }

    /// 빈 파일로 배치를 만든다. 배치 판별은 파일 내용을 읽지 않아 어느 OS에서나 같은 결론을 낸다.
    fn layout(root: &std::path::Path, files: &[&str]) {
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create fixture dir");
            std::fs::write(&path, "").expect("write fixture");
        }
    }

    const GIT_FOR_WINDOWS: &[&str] = &[
        "bin/bash.exe",
        "usr/bin/bash.exe",
        "cmd/git.exe",
        "usr/bin/msys-2.0.dll",
        "git-bash.exe",
    ];

    #[test]
    fn a_git_for_windows_layout_is_found_wherever_it_is_installed() {
        let dir = tempfile::tempdir().expect("tempdir");
        for root in ["Program Files/Git", "other place/Tools/PG", "usr"] {
            let root = dir.path().join(root);
            layout(&root, GIT_FOR_WINDOWS);
            let bin_bash = root.join("bin/bash.exe");
            assert!(is_git_for_windows_bash(&bin_bash), "{bin_bash:?}");
            let usr_bash = root.join("usr/bin/bash.exe");
            assert!(is_git_for_windows_bash(&usr_bash), "{usr_bash:?}");
            assert_eq!(
                ShellFamily::detect(&bin_bash.to_string_lossy()),
                ShellFamily::Bash
            );
        }
    }

    #[test]
    fn names_differing_in_case_still_match_the_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        layout(
            dir.path(),
            &["BIN/BASH.EXE", "cmd/git.exe", "usr/bin/msys-2.0.dll"],
        );
        assert!(is_git_for_windows_bash(&dir.path().join("BIN/BASH.EXE")));
    }

    #[test]
    fn other_files_in_a_git_layout_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        layout(dir.path(), GIT_FOR_WINDOWS);
        layout(
            dir.path(),
            &["my-bash.exe", "bin/my-bash.exe", "bin/zsh.exe", "bash.exe"],
        );
        for file in [
            "git-bash.exe",
            "my-bash.exe",
            "bin/my-bash.exe",
            "bin/zsh.exe",
            "bash.exe",
        ] {
            assert!(!is_git_for_windows_bash(&dir.path().join(file)), "{file}");
        }
    }

    #[test]
    fn layouts_without_the_git_markers_are_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        // MSYS2: usr\bin\bash.exe와 런타임은 있지만 cmd\git.exe가 없다.
        let msys2 = dir.path().join("msys64");
        layout(&msys2, &["usr/bin/bash.exe", "usr/bin/msys-2.0.dll"]);
        assert!(!is_git_for_windows_bash(&msys2.join("usr/bin/bash.exe")));
        // MSYS2에 git 패키지를 깔아도 cmd\git.exe는 생기지 않는다.
        layout(&msys2, &["usr/bin/git.exe", "ucrt64/bin/git.exe"]);
        assert!(!is_git_for_windows_bash(&msys2.join("usr/bin/bash.exe")));
        // 복사본: bin\bash.exe만 있다.
        let copy = dir.path().join("fake");
        layout(&copy, &["bin/bash.exe"]);
        assert!(!is_git_for_windows_bash(&copy.join("bin/bash.exe")));
        // 표식이 하나만 있어도 거부한다.
        let half = dir.path().join("half");
        layout(&half, &["bin/bash.exe", "cmd/git.exe"]);
        assert!(!is_git_for_windows_bash(&half.join("bin/bash.exe")));
    }

    /// System32\bash.exe(WSL)는 부모 폴더 이름에서 걸러진다. 옆에 표식이 있어도 마찬가지다.
    #[test]
    fn a_bash_outside_a_bin_folder_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let system32 = dir.path().join("Windows/System32");
        layout(
            &system32,
            &["bash.exe", "cmd/git.exe", "usr/bin/msys-2.0.dll"],
        );
        assert!(!is_git_for_windows_bash(&system32.join("bash.exe")));
    }

    #[test]
    fn a_folder_or_a_relative_path_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        layout(dir.path(), &["cmd/git.exe", "usr/bin/msys-2.0.dll"]);
        let folder = dir.path().join("bin/bash.exe");
        std::fs::create_dir_all(&folder).expect("create folder");
        assert!(!is_git_for_windows_bash(&folder));
        assert!(!is_git_for_windows_bash(std::path::Path::new(
            "bin/bash.exe"
        )));
    }
}
