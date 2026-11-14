//! `scripts/sign-bundle.sh --dir` 의 계약을 셸 시험(`scripts/tests/sign-bundle-dir.sh`)으로 실행한다.
//! 받은 공식 플러그인 산출물은 서명 없이 오고 tasty 패키징이 다시 서명한다. 그래서 대상 폴더에
//! 서명이 이미 있으면 아무것도 쓰지 않고 실패해야 한다. 정상·빈 폴더·키 없음·기존 서명을 나눈다.
//! OpenSSL과 Bash가 필요하다. Windows에서는 Git과 함께 설치된 Bash와 그 OpenSSL을 쓴다.
//! 셸 시험이 쓰는 키는 일회용 폴더에서 만들어 지우므로 저장소의 키와 무관하다.

use std::path::{Path, PathBuf};
use std::process::Command;
use tasty_doc_guards::repo_root;

/// Windows의 시스템 bash.exe는 배포판 없는 WSL 실행기일 수 있어 Git의 Bash를 쓴다.
fn bash() -> PathBuf {
    if cfg!(windows) {
        let shell = Command::new("git")
            .args(["var", "GIT_SHELL_PATH"])
            .output()
            .expect("git var GIT_SHELL_PATH");
        assert!(shell.status.success(), "git var GIT_SHELL_PATH failed");
        Path::new(String::from_utf8_lossy(&shell.stdout).trim())
            .parent()
            .expect("GIT_SHELL_PATH has a directory")
            .join("bash.exe")
    } else {
        PathBuf::from("bash")
    }
}

/// Git의 Bash를 비대화형으로 띄우면 PATH에 coreutils·openssl 디렉터리가 없다. 그 디렉터리를 앞에 둔다.
fn bash_command() -> Command {
    let bash = bash();
    #[cfg(windows)]
    {
        let dir = bash.parent().expect("bash has a directory").to_path_buf();
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(&rest)))
            .expect("PATH entries join");
        let mut command = Command::new(bash);
        command.env("PATH", path);
        command
    }
    #[cfg(not(windows))]
    Command::new(bash)
}

fn shell_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn sign_bundle_dir_signs_unsigned_stages_and_refuses_the_rest() {
    let scratch = tasty_doc_guards::temp_scratch::Scratch::new("sign-bundle-dir");
    let output = bash_command()
        .arg(shell_path(
            &repo_root().join("scripts/tests/sign-bundle-dir.sh"),
        ))
        .arg(shell_path(&repo_root()))
        .arg(shell_path(scratch.path()))
        .output()
        .expect("run sign-bundle --dir shell test");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
