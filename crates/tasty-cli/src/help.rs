//! `-a/--all` 트리 인쇄, 파싱 에러 포맷, `print_augmented_help` 등 도움말 헬퍼.

use anyhow::Result;

use super::dynamic;
use crate::out::outln;

struct ArgInfo {
    /// 값 이름. `value_name`이 없으면 clap처럼 인자 id를 대문자로 쓴다.
    name: String,
    flag: Option<String>,
    help: String,
    required: bool,
    /// 값 없는 플래그(SetTrue·Count 등)는 `<NAME>`을 붙이지 않는다.
    takes_value: bool,
    /// 여러 값(가변 위치 인자·반복 옵션)을 받으면 `...`을 붙인다.
    multiple: bool,
}

impl ArgInfo {
    fn value(&self) -> String {
        let dots = if self.multiple { "..." } else { "" };
        format!("<{}>{}", self.name, dots)
    }

    /// 인자 하나의 사용법: `<NAME>`, `--flag <NAME>`, `--flag`, `<NAME>...`.
    fn usage(&self) -> String {
        match &self.flag {
            None => self.value(),
            Some(f) if self.takes_value => format!("{} {}", f, self.value()),
            Some(f) => f.clone(),
        }
    }

    /// Compact form: required as is, optional in brackets.
    fn compact(&self) -> String {
        if self.required {
            self.usage()
        } else {
            format!("[{}]", self.usage())
        }
    }

    /// Detail form for error messages: `  --flag <NAME>   Help text`
    fn detail(&self) -> String {
        if self.flag.is_none() {
            format!("  {}          {}", self.compact(), self.help)
        } else if self.required {
            format!("  {}   {}", self.usage(), self.help)
        } else {
            format!("  {} {}", self.compact(), self.help)
        }
    }
}

/// clap이 추가하는 help/version 인자. 사용자가 선언한 인자가 아니다.
fn is_builtin_arg(a: &clap::Arg) -> bool {
    matches!(
        a.get_action(),
        clap::ArgAction::Help
            | clap::ArgAction::HelpShort
            | clap::ArgAction::HelpLong
            | clap::ArgAction::Version
    ) || a.get_id() == "help"
        || a.get_id() == "version"
}

/// 파서의 인자 선언에서 사용법을 읽는다. 값 유무와 개수는 action·num_args를 따른다.
fn arg_info(a: &clap::Arg) -> ArgInfo {
    let takes_value = a.get_action().takes_values();
    let many_values = a.get_num_args().is_some_and(|r| r.max_values() > 1);
    let appends = matches!(a.get_action(), clap::ArgAction::Append);
    ArgInfo {
        name: a
            .get_value_names()
            .and_then(|names| names.first())
            .map(|n| n.to_string())
            .unwrap_or_else(|| a.get_id().to_string().to_uppercase()),
        flag: a
            .get_long()
            .map(|l| format!("--{}", l))
            .or_else(|| a.get_short().map(|s| format!("-{}", s))),
        help: a.get_help().map(|s| s.to_string()).unwrap_or_default(),
        required: a.is_required_set(),
        takes_value,
        multiple: takes_value && (many_values || appends),
    }
}

/// Extract visible arguments from a clap Command (filtering out help/version).
fn visible_args(cmd: &clap::Command) -> Vec<ArgInfo> {
    cmd.get_arguments()
        .filter(|a| !is_builtin_arg(a) && !a.is_hide_set())
        .map(arg_info)
        .collect()
}

/// Extract visible subcommands (filtering out "help").
fn visible_subcommands(cmd: &clap::Command) -> Vec<&clap::Command> {
    cmd.get_subcommands()
        .filter(|s| s.get_name() != "help")
        .collect()
}

/// Compact usage string: `<TEXT> [--surface <SURFACE>]`
fn format_args(cmd: &clap::Command) -> String {
    visible_args(cmd)
        .iter()
        .map(|a| a.compact())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve the deepest matched command from raw CLI args.
/// 루트 옵션과 그 값, `help <경로>`의 `help`를 건너뛰어 실제 하위 명령 문맥을 찾는다.
fn resolve_command_path(args: &[String]) -> (clap::Command, String) {
    let root = match tasty_host_plugin::plugin_root() {
        Some(root) => dynamic::build_augmented_cli(&dynamic::discover_plugin_clis(&root)),
        None => crate::help_i18n::command(),
    };
    // `help` 접두는 clap이 help 하위 명령을 만들기 전의 트리로 판별한다.
    let tokens = crate::help_route::command_path_tokens(&root, args).to_vec();
    let mut current = root;
    // 인자 action·num_args 기본값이 채워진 상태로 사용법을 읽는다.
    current.build();
    let mut matched_path: Vec<String> = Vec::new();

    for arg in &tokens {
        if arg.starts_with('-') {
            break;
        }
        let found = current
            .get_subcommands()
            .find(|s| s.get_name() == arg.as_str() || s.get_all_aliases().any(|a| a == arg));
        if let Some(sub) = found {
            matched_path.push(sub.get_name().to_string());
            current = sub.clone();
        } else {
            break;
        }
    }

    let path = if matched_path.is_empty() {
        "tasty".to_string()
    } else {
        format!("tasty {}", matched_path.join(" "))
    };
    (current, path)
}

/// Print the host and plugin command tree with usage details.
/// The caller supplies the application version; this library has its own version.
/// A closed stdout pipe is treated as normal completion.
pub fn print_command_tree(version: &str) -> Result<()> {
    crate::out::quiet_if_stdout_closed(print_command_tree_inner(version))
}

fn print_command_tree_inner(version: &str) -> Result<()> {
    let entries = match tasty_host_plugin::plugin_root() {
        Some(root) => dynamic::discover_plugin_clis(&root),
        None => Vec::new(),
    };
    let mut cmd = if entries.is_empty() {
        crate::help_i18n::command()
    } else {
        dynamic::build_augmented_cli(&entries)
    };
    // 인자 action·num_args 기본값이 채워진 상태로 읽는다.
    cmd.build();
    outln!("{} {}", cmd.get_name(), version)?;
    outln!(
        "{}",
        cmd.get_about().map(|s| s.to_string()).unwrap_or_default()
    )?;
    outln!()?;
    for line in tree_lines(&cmd) {
        outln!("{}", line)?;
    }
    outln!()?;
    outln!("{}", tasty_i18n::t("cli.help_frame.tree_details"))?;
    Ok(())
}

/// 모든 깊이의 canonical 명령을 경로 순서대로 그린다. 인자는 개별 도움말로 안내한다.
/// clap이 만드는 help 하위 명령은 반복해 펼치지 않고, 별칭은 같은 줄에 표시한다.
fn tree_lines(root: &clap::Command) -> Vec<String> {
    fn walk(cmd: &clap::Command, indent: &str, out: &mut Vec<String>) {
        let subs = visible_subcommands(cmd);
        let count = subs.len();
        for (i, sub) in subs.iter().enumerate() {
            let is_last = i + 1 == count;
            let branch = if is_last { "└── " } else { "├── " };
            let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
            let aliases: Vec<&str> = sub.get_visible_aliases().collect();
            let name = if aliases.is_empty() {
                sub.get_name().to_string()
            } else {
                format!(
                    "{} [{}]",
                    sub.get_name(),
                    tasty_i18n::t_fmt("cli.help_frame.aliases", &aliases.join(", "))
                )
            };
            out.push(format!("{indent}{branch}{name} — {about}"));
            let child_indent = format!("{indent}{}", if is_last { "    " } else { "│   " });
            walk(sub, &child_indent, out);
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// Format a contextual error message for a failed parse and exit.
/// `args` is the argv the parser received; it locates the command context.
pub fn format_parse_error(err: clap::Error, args: &[String]) -> ! {
    use clap::error::ErrorKind;

    match err.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        | ErrorKind::DisplayVersion => err.exit(),
        _ => {
            let (current, cmd_path) = resolve_command_path(args);
            let children = visible_subcommands(&current);

            crate::out::errln!("{}", crate::help_error::render(&err));

            if !children.is_empty() {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.help_frame.available", &cmd_path)
                );
                for sub in &children {
                    let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
                    let args = format_args(sub);
                    if args.is_empty() {
                        crate::out::errln!("  {} {:16} {}", cmd_path, sub.get_name(), about);
                    } else {
                        crate::out::errln!(
                            "  {} {} {} — {}",
                            cmd_path,
                            sub.get_name(),
                            args,
                            about
                        );
                    }
                }
            } else {
                let args = visible_args(&current);
                let required: Vec<_> = args.iter().filter(|a| a.required).collect();
                let optional: Vec<_> = args.iter().filter(|a| !a.required).collect();

                if !required.is_empty() {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt("cli.help_frame.required", &cmd_path)
                    );
                    for arg in &required {
                        crate::out::errln!("{}", arg.detail());
                    }
                }
                if !optional.is_empty() {
                    crate::out::errln!("{}", tasty_i18n::t("cli.help_frame.optional"));
                    for arg in &optional {
                        crate::out::errln!("{}", arg.detail());
                    }
                }
            }
            crate::out::errln!();
            crate::out::errln!("{}", tasty_i18n::t_fmt("cli.help_frame.details", &cmd_path));
        }
    }
    std::process::exit(2);
}

/// 플러그인을 포함한 도움말을 출력한다. discovery 실패 시 정적 도움말은 유지한다.
/// stdout의 BrokenPipe는 정상 종료로 처리한다.
pub fn print_augmented_help() -> Result<()> {
    crate::out::quiet_if_stdout_closed(print_augmented_help_inner())
}

fn print_augmented_help_inner() -> Result<()> {
    let entries = match tasty_host_plugin::plugin_root() {
        Some(root) => dynamic::discover_plugin_clis(&root),
        None => Vec::new(),
    };
    let mut cmd = if entries.is_empty() {
        crate::help_i18n::command()
    } else {
        dynamic::build_augmented_cli(&entries)
    };
    crate::out::from_io(cmd.print_help())?;
    outln!()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, ArgAction, Command};

    fn built(mut cmd: Command) -> Command {
        cmd.build();
        cmd
    }

    /// clap 트리에서 직접 모은 canonical 경로. help 하위 명령은 제외한다.
    fn canonical_paths(cmd: &Command, prefix: &str, out: &mut Vec<String>) {
        for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
            let path = format!("{prefix}{}", sub.get_name());
            out.push(path.clone());
            canonical_paths(sub, &format!("{path} "), out);
        }
    }

    /// 트리 출력의 들여쓰기(4칸 단위)로 경로를 복원한다.
    fn paths_from_tree(lines: &[String]) -> Vec<String> {
        let mut stack: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for line in lines {
            let (head, _) = line.split_once(" — ").unwrap();
            let chars: Vec<char> = head.chars().collect();
            let depth = chars.iter().take_while(|c| matches!(c, '│' | ' ')).count() / 4;
            let name: String = chars[depth * 4 + 4..].iter().collect();
            let name = name.split(" [").next().unwrap().to_string();
            stack.truncate(depth);
            stack.push(name);
            out.push(stack.join(" "));
        }
        out
    }

    #[test]
    fn tree_lists_every_canonical_path_at_any_depth_once() {
        let deep = Command::new("root").subcommand(
            Command::new("a").about("A").subcommand(
                Command::new("b").about("B").subcommand(
                    Command::new("c")
                        .about("C")
                        .visible_alias("cc")
                        .subcommand(Command::new("d").about("D")),
                ),
            ),
        );
        for root in [built(deep), built(crate::help_i18n::command())] {
            let lines = tree_lines(&root);
            let mut expected = Vec::new();
            canonical_paths(&root, "", &mut expected);
            assert_eq!(paths_from_tree(&lines), expected);
        }
        let lines = tree_lines(&built(crate::help_i18n::command()));
        let paths = paths_from_tree(&lines);
        for leaf in ["memory goal set", "memory goal get", "memory goal clear"] {
            assert!(paths.iter().any(|p| p == leaf), "{leaf}");
        }
    }

    #[test]
    fn aliases_are_shown_on_the_canonical_line_not_as_extra_commands() {
        let root = built(
            Command::new("root").subcommand(Command::new("state").about("S").visible_alias("wait")),
        );
        let lines = tree_lines(&root);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("state") && lines[0].contains("wait"),
            "{lines:?}"
        );
    }

    #[test]
    fn usage_reflects_value_presence_and_count() {
        let cmd = built(
            Command::new("x")
                .arg(Arg::new("flag").long("flag").action(ArgAction::SetTrue))
                .arg(Arg::new("name").long("name").required(true))
                .arg(Arg::new("pos").required(true))
                .arg(Arg::new("rest").num_args(1..).trailing_var_arg(true)),
        );
        let usages: Vec<String> = visible_args(&cmd).iter().map(ArgInfo::compact).collect();
        assert_eq!(
            usages,
            ["[--flag]", "--name <NAME>", "<POS>", "[<REST>...]"]
        );

        let root = built(crate::help_i18n::command());
        let find = |path: &[&str]| {
            let mut c = &root;
            for p in path {
                c = c.find_subcommand(p).unwrap();
            }
            c.clone()
        };
        let read = format_args(&find(&["read", "since-mark"]));
        assert!(read.contains("[--strip-ansi]"), "{read}");
        assert!(!read.contains("STRIP_ANSI"), "{read}");
        let spawn = format_args(&find(&["pty", "spawn"]));
        assert!(spawn.contains("[<COMMAND>...]"), "{spawn}");
    }

    /// 모든 명령의 모든 옵션에 대해, 도움말이 말한 값 유무대로 argv를 만들면 파서가 받는다.
    #[test]
    fn every_option_usage_is_accepted_by_the_parser() {
        fn walk(cmd: &Command, path: &mut Vec<String>, checked: &mut usize) {
            for arg in cmd
                .get_arguments()
                .filter(|a| !is_builtin_arg(a) && !a.is_positional())
            {
                let info = arg_info(arg);
                let mut argv: Vec<String> = std::iter::once("tasty".to_string())
                    .chain(path.iter().cloned())
                    .collect();
                argv.push(info.flag.clone().unwrap());
                if info.takes_value {
                    let value = arg
                        .get_possible_values()
                        .first()
                        .map(|v| v.get_name().to_string())
                        .unwrap_or_else(|| "1".to_string());
                    argv.push(value);
                }
                // 값 유무가 틀리면 파서는 알 수 없는 인자나 값 개수 오류를 낸다.
                // 다른 필수 인자 누락·충돌은 이 옵션의 표기와 무관하다.
                use clap::error::ErrorKind;
                if let Err(err) = crate::help_i18n::command().try_get_matches_from(&argv) {
                    assert!(
                        matches!(
                            err.kind(),
                            ErrorKind::MissingRequiredArgument
                                | ErrorKind::MissingSubcommand
                                | ErrorKind::ArgumentConflict
                                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                        ),
                        "{argv:?} ({}): {:?} {err}",
                        info.usage(),
                        err.kind()
                    );
                }
                *checked += 1;
            }
            for sub in cmd.get_subcommands().filter(|s| s.get_name() != "help") {
                path.push(sub.get_name().to_string());
                walk(sub, path, checked);
                path.pop();
            }
        }
        let root = built(crate::help_i18n::command());
        let mut checked = 0;
        for sub in root.get_subcommands().filter(|s| s.get_name() != "help") {
            walk(sub, &mut vec![sub.get_name().to_string()], &mut checked);
        }
        assert!(checked > 100, "{checked}");
    }
}
