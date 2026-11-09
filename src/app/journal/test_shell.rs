//! journal 시험이 기본 터미널로 띄우는 셸.
//!
//! 시험은 셸 출력을 보지 않고 터미널 자원이 만들어지는지만 본다. Unix 는 아무것도 출력하지 않고
//! 기다리는 `sleep` 으로 바꾼다. Windows 는 Git Bash 만 실행하므로 탐지한 Git Bash 를 띄운다.

pub(super) fn use_quiet_shell(settings: &mut crate::settings::Settings) {
    #[cfg(unix)]
    {
        settings.general.shell = "/bin/sh".into();
        settings.general.startup_command = "exec sleep 60".into();
    }
    #[cfg(windows)]
    {
        settings.general.shell = crate::settings::GeneralSettings::detect_bash()
            .expect("Windows tests need Git Bash at a default install path");
    }
}
