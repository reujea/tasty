//! 하위 명령이 많은 명령의 도움말을 목적별 그룹으로 나눠 표시한다.
//!
//! clap은 하위 명령 그룹 제목을 지원하지 않으므로 그룹 목록을 도움말 템플릿에 직접 적는다.
//! 명령 집합·파싱·실행 경로는 바꾸지 않고 표시 순서와 제목만 정한다.
//! 분류는 명시적인 표(host)와 매니페스트 선언(plugin)으로 정하며 설명 문구로 추측하지 않는다.

use clap::Command;
use tasty_i18n::t;

/// 루트 도움말의 그룹 순서. plugin이 선언할 수 있는 값은 `PLUGIN_SELECTABLE`이다.
pub const ROOT_GROUPS: &[&str] = &[
    "basic",
    "agents",
    "background",
    "automation",
    "files",
    "settings",
    "plugins",
    "other",
];

/// plugin 매니페스트의 `help_group`에 쓸 수 있는 값. 다른 값이나 생략은 `plugins`다.
pub const PLUGIN_SELECTABLE: &[&str] = &[
    "basic",
    "agents",
    "background",
    "automation",
    "files",
    "settings",
];

/// 내부 연동 명령 그룹. 직접 실행할 수 있지만 일반 사용 흐름에서 추천하지 않으며 항상 마지막이다.
pub const INTERNAL: &str = "internal";

/// host 최상위 명령의 그룹. 새 명령은 여기에 추가해야 하며, 빠지면 `other`로 표시되고
/// `every_host_command_has_a_root_group` 시험이 실패한다.
const HOST_ROOT: &[(&str, &str)] = &[
    ("new", "basic"),
    ("close", "basic"),
    ("list", "basic"),
    ("set", "basic"),
    ("unset", "basic"),
    ("move", "basic"),
    ("split", "basic"),
    ("send", "basic"),
    ("read", "basic"),
    ("is-typing", "basic"),
    ("wake", "basic"),
    ("screenshot", "basic"),
    ("surface", "basic"),
    ("surface-meta", "basic"),
    ("terminal", "agents"),
    ("agent", "agents"),
    ("memory", "agents"),
    ("session", "agents"),
    ("approval", "agents"),
    ("pty", "background"),
    ("remote", "background"),
    ("tool", "background"),
    ("output", "automation"),
    ("events", "automation"),
    ("hook-handler", "automation"),
    ("webhook", "automation"),
    ("completion-strategy", "automation"),
    ("telemetry", "automation"),
    ("notify", "automation"),
    ("notification", "automation"),
    ("file-handler", "files"),
    ("clipboard", "files"),
    ("plugin", "settings"),
    ("settings", "settings"),
    ("preset", "settings"),
    ("workspace-category", "settings"),
    ("port", "settings"),
    // debug 빌드 전용이고 사용자 조작을 재현하므로 설정·관리가 아니라 기타 명령에 둔다.
    ("debug", "other"),
];

/// `tasty agent` 하위 명령의 그룹 순서.
pub const AGENT_GROUPS: &[&str] = &[
    "agent_tasks",
    "agent_sync",
    "agent_lease",
    "agent_rate_limit",
];

/// `tasty agent` 하위 명령의 그룹. 이름의 접두가 아니라 명시 표로 정한다.
const HOST_AGENT: &[(&str, &str)] = &[
    ("task-create", "agent_tasks"),
    ("task-graph-submit", "agent_tasks"),
    ("task-list", "agent_tasks"),
    ("task-get", "agent_tasks"),
    ("task-await", "agent_tasks"),
    ("task-cancel", "agent_tasks"),
    ("task-retry", "agent_tasks"),
    ("task-graph", "agent_tasks"),
    ("dag-list", "agent_tasks"),
    ("dag-get", "agent_tasks"),
    ("dag-report", "agent_tasks"),
    ("report", "agent_tasks"),
    ("task-run", "agent_tasks"),
    ("task-set-result", "agent_tasks"),
    ("task-submit", "agent_tasks"),
    ("task-delete", "agent_tasks"),
    ("task-purge", "agent_tasks"),
    ("task-reduce", "agent_tasks"),
    ("barrier-create", "agent_sync"),
    ("barrier-signal", "agent_sync"),
    ("barrier-await", "agent_sync"),
    ("barrier-state", "agent_sync"),
    ("barrier-list", "agent_sync"),
    ("barrier-delete", "agent_sync"),
    ("semaphore-create", "agent_sync"),
    ("semaphore-set-permits", "agent_sync"),
    ("semaphore-acquire", "agent_sync"),
    ("semaphore-release", "agent_sync"),
    ("semaphore-list", "agent_sync"),
    ("semaphore-delete", "agent_sync"),
    ("lease-acquire", "agent_lease"),
    ("lease-release", "agent_lease"),
    ("lease-list", "agent_lease"),
    ("rate-limit-set", "agent_rate_limit"),
    ("rate-limit-list", "agent_rate_limit"),
    ("rate-limit-remove", "agent_rate_limit"),
    ("rate-limit-status", "agent_rate_limit"),
];

/// 그룹 제목·시작 안내가 사용하는 번역 키 전체. 도움말 slots 수집 밖의 문구라
/// `help_i18n_catalog_parity`가 이 목록으로 내장 언어 카탈로그를 따로 대조한다.
pub fn text_keys() -> Vec<String> {
    let groups = ROOT_GROUPS
        .iter()
        .chain(AGENT_GROUPS)
        .chain(&[INTERNAL, "plugin_commands"])
        .map(|g| format!("cli.help_group.{g}"));
    let start = START_LINES
        .iter()
        .map(|(_, key)| key.to_string())
        .chain(["cli.help_start.title".to_string()]);
    groups.chain(start).collect()
}

/// host 최상위 명령의 그룹. 표에 없으면 `other`.
pub fn host_root_group(name: &str) -> &'static str {
    lookup(HOST_ROOT, name).unwrap_or("other")
}

/// plugin 최상위 명령의 그룹. 선언이 없거나 모르는 값이면 `plugins`.
pub fn plugin_root_group(declared: Option<&str>) -> &'static str {
    declared
        .and_then(|d| PLUGIN_SELECTABLE.iter().find(|g| **g == d).copied())
        .unwrap_or("plugins")
}

fn lookup(table: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == name).map(|(_, g)| *g)
}

/// host 정적 트리(번역 뒤)에 루트·agent 그룹을 적용한다. plugin 명령은 없다.
pub(crate) fn apply_host(cmd: Command) -> Command {
    let cmd = apply_root(cmd, &[]);
    cmd.mut_subcommand("agent", |agent| {
        let order = AGENT_GROUPS.to_vec();
        apply(
            agent,
            &order,
            |name| lookup(HOST_AGENT, name).unwrap_or(AGENT_GROUPS[0]),
            "",
        )
    })
}

/// 루트 도움말을 그룹 목록과 시작 안내로 표시한다. `plugins`는 (이름, 그룹)이다.
pub(crate) fn apply_root(cmd: Command, plugins: &[(String, &'static str)]) -> Command {
    let order = ROOT_GROUPS.to_vec();
    apply(
        cmd,
        &order,
        |name| {
            plugins
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, g)| *g)
                .unwrap_or_else(|| host_root_group(name))
        },
        &start_block(),
    )
}

/// plugin 최상위 명령에서 내부 연동 하위 명령을 마지막 그룹으로 분리한다.
/// 내부 명령이 없으면 도움말 형식을 바꾸지 않는다.
pub(crate) fn apply_plugin_internal(cmd: Command, internal: &[String]) -> Command {
    if internal.is_empty() {
        return cmd;
    }
    let order = vec!["plugin_commands", INTERNAL];
    apply(
        cmd,
        &order,
        |name| {
            if internal.iter().any(|i| i == name) {
                INTERNAL
            } else {
                "plugin_commands"
            }
        },
        "",
    )
}

/// 시작 안내의 예시와 설명 키. 예시는 `start_examples_parse` 시험이 실제 파서로 확인한다.
const START_LINES: [(&str, &str); 5] = [
    ("tasty list surfaces", "cli.help_start.list"),
    (
        "tasty send text \"echo hi\\r\" --surface <ID>",
        "cli.help_start.send",
    ),
    ("tasty read screen --surface <ID>", "cli.help_start.read"),
    ("tasty help <COMMAND>", "cli.help_start.help"),
    ("tasty -a", "cli.help_start.tree"),
];

/// 시작 안내: 대상 찾기 → 보내기·읽기 → 도움말 찾기. 실제 대상 없이 복사해 실행하지 않도록
/// 대상 자리는 `<ID>`로 둔다.
fn start_block() -> String {
    let lines = START_LINES;
    let width = lines
        .iter()
        .map(|(c, _)| c.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = format!("{}\n", t("cli.help_start.title"));
    for (example, key) in lines {
        out.push_str(&format!("  {example:<width$}  {}\n", t(key)));
    }
    out
}

/// 하위 명령을 그룹 순서대로 나열한다. 빈 그룹은 표시하지 않는다.
/// clap이 만드는 help 하위 명령은 목록에서 빼고 시작 안내·사용법 줄로 안내한다.
/// 목록은 템플릿 해석을 거치지 않는 before-help 자리에 넣어 설명의 중괄호를 그대로 보인다.
fn apply<'a>(
    cmd: Command,
    order: &[&'a str],
    group_of: impl Fn(&str) -> &'a str,
    start: &str,
) -> Command {
    let subs: Vec<(String, String, Vec<String>)> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .map(|s| {
            (
                s.get_name().to_string(),
                s.get_about().map(|a| a.to_string()).unwrap_or_default(),
                s.get_visible_aliases().map(str::to_string).collect(),
            )
        })
        .collect();
    let width = subs
        .iter()
        .map(|(n, _, _)| n.chars().count())
        .max()
        .unwrap_or(0);
    let mut blocks: Vec<String> = Vec::new();
    if !start.is_empty() {
        blocks.push(start.trim_end().to_string());
    }
    for group in order {
        let members: Vec<_> = subs
            .iter()
            .filter(|(name, _, _)| group_of(name) == *group)
            .collect();
        if members.is_empty() {
            continue;
        }
        // 키 리터럴 형태를 유지한다. i18n_key_parity의 사용처 판독이 이 템플릿으로 그룹 제목 키를 찾는다.
        let mut block = t(&format!("cli.help_group.{group}")).to_string();
        for (name, about, aliases) in members {
            block.push_str(&format!("\n  {name:<width$}  {about}"));
            if !aliases.is_empty() {
                block.push_str(&format!(
                    " [{}]",
                    tasty_i18n::t_fmt("cli.help_frame.aliases", &aliases.join(", "))
                ));
            }
        }
        blocks.push(block);
    }
    let mut template = format!(
        "{{about-with-newline}}\n{} {{usage}}\n\n{{before-help}}",
        t("cli.help_frame.usage")
    );
    if cmd.get_positionals().any(|a| !a.is_hide_set()) {
        template.push_str(&format!(
            "{}\n{{positionals}}\n\n",
            t("cli.help_frame.arguments")
        ));
    }
    // 루트·agent·plugin 명령에는 항상 clap의 -h/--help가 있으므로 옵션 절을 둔다.
    template.push_str(&format!("{}\n{{options}}\n", t("cli.help_frame.options")));
    template.push_str("{after-help}");
    cmd.before_help(blocks.join("\n\n")).help_template(template)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_command_has_a_root_group() {
        let root = crate::help_i18n::command();
        let unclassified: Vec<_> = root
            .get_subcommands()
            .map(|s| s.get_name())
            .filter(|n| *n != "help" && lookup(HOST_ROOT, n).is_none())
            .collect();
        assert!(unclassified.is_empty(), "{unclassified:?}");
        // 표의 이름은 실제 명령이어야 한다. debug는 release 빌드에 없다.
        for (name, group) in HOST_ROOT {
            assert!(ROOT_GROUPS.contains(group), "{name}: {group}");
            if *name != "debug" || cfg!(debug_assertions) {
                assert!(root.find_subcommand(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn every_agent_command_has_exactly_one_group() {
        let root = crate::help_i18n::command();
        let agent = root.find_subcommand("agent").unwrap();
        let names: Vec<_> = agent
            .get_subcommands()
            .map(|s| s.get_name())
            .filter(|n| *n != "help")
            .collect();
        let mut table: Vec<_> = HOST_AGENT.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort();
        table.sort();
        assert_eq!(sorted, table);
        for (_, group) in HOST_AGENT {
            assert!(AGENT_GROUPS.contains(group));
        }
    }

    /// 시작 안내의 예시는 자리값을 채우면 실제 파서가 받는다. 큰따옴표 안은 한 인자다.
    #[test]
    fn start_examples_parse() {
        for (example, key) in START_LINES {
            let filled = example.replace("<ID>", "7").replace("<COMMAND>", "new");
            let mut argv = Vec::new();
            for (i, part) in filled.split('"').enumerate() {
                if i % 2 == 1 {
                    argv.push(part.to_string());
                } else {
                    argv.extend(part.split_whitespace().map(str::to_string));
                }
            }
            let result = crate::help_i18n::command().try_get_matches_from(&argv);
            match result {
                Ok(_) => {}
                Err(e) => assert!(
                    matches!(e.kind(), clap::error::ErrorKind::DisplayHelp),
                    "{example}: {e}"
                ),
            }
            assert_ne!(t(key), key, "{key}");
        }
    }

    #[test]
    fn plugin_groups_fall_back_to_the_plugin_group() {
        assert_eq!(plugin_root_group(Some("agents")), "agents");
        assert_eq!(plugin_root_group(Some("future-group")), "plugins");
        assert_eq!(plugin_root_group(Some("internal")), "plugins");
        assert_eq!(plugin_root_group(None), "plugins");
    }

    /// 그룹 목록에 나온 명령 집합 = clap 트리의 명령 집합(help 제외). 누락·중복이 없다.
    #[test]
    fn grouped_help_lists_each_command_exactly_once() {
        let mut root = crate::help_i18n::command();
        let rendered = root.render_help().to_string();
        let mut expected: Vec<String> = root
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .filter(|n| n != "help")
            .collect();
        let mut listed: Vec<String> = rendered
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| !w.starts_with('-') && *w != "tasty")
            .map(str::to_string)
            .collect();
        expected.sort();
        listed.sort();
        assert_eq!(listed, expected, "{rendered}");
    }

    #[test]
    fn braces_in_descriptions_do_not_become_template_tags() {
        let cmd =
            Command::new("x").subcommand(Command::new("a").about("uses {usage} and {\"k\":1}"));
        let order = vec!["plugin_commands"];
        let mut cmd = apply(cmd, &order, |_| "plugin_commands", "");
        let text = cmd.render_help().to_string();
        assert!(text.contains("uses {usage} and {\"k\":1}"), "{text}");
    }
}
