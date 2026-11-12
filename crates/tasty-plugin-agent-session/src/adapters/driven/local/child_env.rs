//! 자식 프로세스의 환경 — 물려받은 것 중 뺄 것, 역할 지침, 프로필 `env`.

use std::ffi::OsStr;
use std::process::Command;

use crate::adapters::driven::store::agents::AgentProfile;

/// 물려받은 Claude Code 세션 표지·비밀 — `tasty-plugin-claude` 매니페스트 `contributes.strip_inherited_env` 와
/// 같은 목록이다(시험이 대조한다). 호스트는 그 선언을 터미널·훅·작업 자식에만 적용하고, 이 plugin 이 직접
/// 띄우는 자식은 여기서 지운다. `CLAUDE_CODE_` 접두사 전체는 지우지 않는다 — 그 이름공간의 대부분은 사용자가
/// 넣는 설정(`CLAUDE_CODE_OAUTH_TOKEN` 등)이라 지우면 자식의 인증·설정이 빠진다.
const CLAUDE_SESSION_NAMES: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_PLUGIN_ROOT",
    "CLAUDE_PLUGIN_DATA",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_ENV_FILE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_INVOKED_SKILLS",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
];
const CLAUDE_SESSION_PREFIXES: &[&str] = &["CLAUDE_PLUGIN_OPTION_"];
/// `AI_AGENT` 는 여러 도구가 쓰는 이름이다 — Claude Code 가 넣은 값만 지운다.
const AI_AGENT_CLAUDE_VALUES: &[&str] = &["claude-code_", "claude-code/"];

/// 자식에 넘기지 않는 환경 — 넘기면 자식이 자신을 바깥 세션의 자식으로 알거나(`TASTY_*` 는 hook 이 tasty 를
/// 부모로 안다) 바깥 세션의 비밀이 샌다.
pub(super) fn stripped_env(key: &str, value: &OsStr) -> bool {
    key.starts_with("TASTY_")
        || CLAUDE_SESSION_NAMES.contains(&key)
        || CLAUDE_SESSION_PREFIXES.iter().any(|p| key.starts_with(p))
        || (key == "AI_AGENT"
            && value
                .to_str()
                .is_some_and(|v| AI_AGENT_CLAUDE_VALUES.iter().any(|p| v.starts_with(p))))
}

/// 자식 환경을 채운다 — 뺄 것을 빼고, 프로필 `env` 를 얹는다.
pub(super) fn apply(cmd: &mut Command, profile: &AgentProfile, has_roles: bool) {
    for (k, v) in std::env::vars_os() {
        if k.to_str().is_some_and(|k| stripped_env(k, &v)) {
            cmd.env_remove(&k);
        }
    }
    if profile.kind == "claude" && has_roles {
        cmd.env(
            crate::adapters::driven::store::roles::CLAUDE_ADD_DIR_GUIDE_ENV,
            "1",
        );
    }
    cmd.envs(&profile.env);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// claude plugin 매니페스트의 `[contributes.strip_inherited_env]` 절에서 `key = [...]` 의 따옴표 값만 읽는다.
    fn declared(key: &str) -> Vec<String> {
        let toml = include_str!("../../../../../tasty-plugin-claude/tasty-plugin.toml");
        let section = toml
            .split("[contributes.strip_inherited_env]")
            .nth(1)
            .expect("절이 있다");
        let section = section.split("\n[").next().unwrap_or(section);
        let body = section
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let start = body
            .find(&format!("{key} = "))
            .unwrap_or_else(|| panic!("{key} 가 있다"));
        let rest = &body[start..];
        let end = if key == "values" {
            rest.find("}]").expect("닫힌다")
        } else {
            rest.find(']').expect("닫힌다")
        };
        rest[..end]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_strip_list_matches_the_claude_plugin_manifest() {
        let names: Vec<String> = CLAUDE_SESSION_NAMES.iter().map(|s| s.to_string()).collect();
        assert_eq!(declared("names"), names);
        let prefixes: Vec<String> = CLAUDE_SESSION_PREFIXES
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(declared("prefixes"), prefixes);
        let mut values = vec!["AI_AGENT".to_string()];
        values.extend(AI_AGENT_CLAUDE_VALUES.iter().map(|s| s.to_string()));
        assert_eq!(declared("values"), values);
    }

    #[test]
    fn user_settings_and_other_agents_values_survive() {
        let v = OsStr::new("x");
        for k in [
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_GIT_BASH_PATH",
            "CLAUDE_CONFIG_DIR",
            "HOME",
            "PATH",
        ] {
            assert!(!stripped_env(k, v), "{k}");
        }
        assert!(stripped_env(
            "AI_AGENT",
            OsStr::new("claude-code_2.1.291_bash")
        ));
        assert!(stripped_env("AI_AGENT", OsStr::new("claude-code/2.1")));
        assert!(!stripped_env("AI_AGENT", OsStr::new("codex")));
        assert!(stripped_env("CLAUDE_PLUGIN_OPTION_FOO", v));
    }
}
