#![cfg(test)]
//! 셸 PTY 생성. model은 생성 정보만 보관하고 실제 `Terminal`과 waker는 호스트가 만든다.

use tasty_terminal::{Pty, Terminal, TerminalConfig, Waker};

/// 새 셸 터미널의 실행 옵션.
pub(crate) struct ShellSpawnOpts<'a> {
    pub cols: usize,
    pub rows: usize,
    pub shell: Option<&'a str>,
    pub shell_args: &'a [&'a str],
    pub waker: Waker,
    pub working_dir: Option<&'a std::path::Path>,
    /// 자식 셸에 추가로 심을 환경변수(docs/features/terminal-output/index.md#명령-인덱싱-osc-133,
    /// `ShellConfig::envs_ref` 참고).
    pub extra_env: &'a [(&'a str, &'a str)],
}

/// 셸 PTY를 만든다. 호출자는 반환된 Terminal을 store에 넣은 뒤 트리에 marker를 붙인다.
/// 그래야 layout이 store에 없는 터미널을 가리키는 순간이 생기지 않는다.
pub(crate) fn spawn_shell_terminal(
    surface_id: u32,
    spawn: ShellSpawnOpts<'_>,
) -> anyhow::Result<(Terminal, Pty)> {
    Ok(tasty_terminal::spawn_terminal(
        TerminalConfig {
            cols: spawn.cols,
            rows: spawn.rows,
            shell: spawn.shell,
            args: spawn.shell_args,
            surface_id,
            working_dir: spawn.working_dir,
            initial_input: None,
            extra_env: spawn.extra_env,
        },
        spawn.waker,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn noop_waker() -> Waker {
        Arc::new(|| {})
    }

    #[test]
    fn spawn_shell_terminal_with_a_missing_shell_returns_err_not_panic() {
        // 잘못된 셸 경로는 복구 가능한 오류로 반환해야 한다.
        let bogus = "/nonexistent/definitely/not/a/real/shell-xyzzy";
        let result = spawn_shell_terminal(
            1,
            ShellSpawnOpts {
                cols: 80,
                rows: 24,
                shell: Some(bogus),
                shell_args: &[],
                waker: noop_waker(),
                working_dir: None,
                extra_env: &[],
            },
        );
        assert!(
            result.is_err(),
            "a missing shell path must return Err, not Ok or panic"
        );
    }

    /// `ShellConfig`가 빈 값(Git Bash 없음)이나 허용하지 않는 값을 내도 다른 셸이 뜨지 않는다.
    #[cfg(windows)]
    #[test]
    fn a_windows_shell_config_never_starts_another_shell() {
        for shell in ["", "cmd.exe", "C:/Windows/System32/cmd.exe"] {
            let config = crate::core::state::ShellConfig {
                shell: shell.to_string(),
                args: Vec::new(),
                envs: Vec::new(),
            };
            let result = spawn_shell_terminal(
                1,
                ShellSpawnOpts {
                    cols: 80,
                    rows: 24,
                    shell: config.shell_ref(),
                    shell_args: &[],
                    waker: noop_waker(),
                    working_dir: None,
                    extra_env: &[],
                },
            );
            assert!(result.is_err(), "{shell:?}");
        }
    }
}
