//! 매니페스트로 clap 명령 트리를 만든다. 요청 조립은 super::request가 담당한다.

use std::collections::HashSet;
use std::path::Path;

use clap::{Arg, ArgAction, Command};

use tasty_plugin_manifest::{CliArg, CliArgGroup, CliArgType, CliCommandDecl, Manifest};

use super::PluginCliEntry;
use crate::caller_context::CallerContextSource;

/// 설치된 플러그인의 CLI 선언과 호출자 문맥 선언.
#[derive(Debug, Default)]
pub struct DiscoveredPlugins {
    pub clis: Vec<PluginCliEntry>,
    /// 플러그인 ID 순, 같은 플러그인 안에서는 선언 순.
    pub caller_contexts: Vec<CallerContextSource>,
}

/// `~/.tasty/plugins/*`의 CLI 선언만 돌려준다.
pub fn discover_plugin_clis(plugins_root: &Path) -> Vec<PluginCliEntry> {
    discover_plugins(plugins_root).clis
}

/// `~/.tasty/plugins/*` 스캔. 파싱 실패한 매니페스트는 stderr에 경고만 찍고 스킵.
/// 비활성화한 플러그인도 읽는다. 명령이 그대로 보이고, 호출자 문맥을 받은 다른 플러그인은
/// 해당 provider가 꺼져 있다고 오류를 내야 하기 때문이다(다른 surface로 대신하지 않는다).
pub fn discover_plugins(plugins_root: &Path) -> DiscoveredPlugins {
    let mut out = DiscoveredPlugins::default();
    let Ok(read_dir) = std::fs::read_dir(plugins_root) else {
        return out;
    };
    let mut dirs: Vec<_> = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir() && dir.join("tasty-plugin.toml").exists())
        .collect();
    dirs.sort();
    let mut manifests = Vec::new();
    for dir in dirs {
        match Manifest::load(&dir) {
            Ok(manifest) => manifests.push((dir, manifest)),
            Err(e) => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt2(
                        "cli.plugin_cli.manifest_skipped",
                        &dir.display().to_string(),
                        &e.to_string()
                    )
                );
            }
        }
    }
    manifests.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    for (dir, manifest) in manifests {
        let user_lang = tasty_utils::path::tasty_home().map(|home| home.join("lang"));
        let strings = tasty_i18n::plugin_catalog::load(
            &dir.join(&manifest.lang_dir),
            tasty_i18n::current_language(),
            &manifest.id,
            user_lang.as_deref(),
        );
        let contributes = manifest.contributes;
        let mut strict: Vec<String> = contributes
            .caller_context
            .iter()
            .map(|decl| decl.provider.clone())
            .chain(contributes.requires_caller_context.iter().cloned())
            .collect();
        strict.sort();
        strict.dedup();
        let remote_methods: Vec<String> = match contributes.ipc_namespace.as_slice() {
            [ns] => contributes
                .remote_attach
                .methods
                .iter()
                .map(|m| format!("{}.{m}", ns.prefix))
                .collect(),
            _ => Vec::new(),
        };
        for mut cli in contributes.cli {
            localize_decl(&mut cli, &strings);
            out.clis.push(PluginCliEntry {
                cli,
                strict_caller_contexts: strict.clone(),
                remote_methods: remote_methods.clone(),
            });
        }
        out.caller_contexts
            .extend(
                contributes
                    .caller_context
                    .into_iter()
                    .map(|decl| CallerContextSource {
                        plugin_id: manifest.id.clone(),
                        decl,
                    }),
            );
    }
    out
}

/// 호스트 정적 `Cli`에 plugin 서브커맨드를 추가한 `clap::Command`. `--help` 출력
/// 통합과 동적 파싱에 공통 사용.
pub fn build_augmented_cli(entries: &[PluginCliEntry]) -> Command {
    let mut cmd = crate::help_i18n::command();
    let host = host_command_names(&cmd);
    let mut groups = Vec::new();
    for entry in entries {
        if host.contains(&entry.cli.name) {
            crate::out::errln!(
                "{}",
                tasty_i18n::t_fmt("cli.plugin_cli.name_shadows_host_command", &entry.cli.name)
            );
            continue;
        }
        cmd = cmd.subcommand(build_cli_subcommand(&entry.cli));
        groups.push((
            entry.cli.name.clone(),
            crate::help_groups::plugin_root_group(entry.cli.help_group.as_deref()),
        ));
    }
    crate::help_groups::apply_root(cmd, &groups)
}

/// 정적 명령과 alias를 clap 트리에서 읽는다.
pub(super) fn host_command_names(cmd: &Command) -> HashSet<String> {
    // clap이 build 때 만드는 help 하위 명령의 이름도 예약돼 있다. en 트리는 아직 build 전이라
    // 목록에 없으므로 항상 넣어 모든 언어에서 같은 경고·거부 경로를 타게 한다.
    let mut names = HashSet::from(["help".to_string()]);
    for sub in cmd.get_subcommands() {
        names.insert(sub.get_name().to_string());
        for alias in sub.get_all_aliases() {
            names.insert(alias.to_string());
        }
    }
    names
}

/// clap의 static 문자열 요구에 맞춰 프로세스 종료까지 보관한다.
/// 반복 호출하면 계속 누적되므로 CLI의 일회성 구성에만 사용한다.
fn leak_static(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn build_cli_subcommand(decl: &CliCommandDecl) -> Command {
    // arg_required_else_help: subcommand 누락 시 에러 메시지 대신 풀 도움말을 출력 —
    // 호스트의 derive 기반 CLI(tasty claude 등)와 동일한 UX.
    let mut top = Command::new(leak_static(&decl.name))
        .subcommand_required(true)
        .arg_required_else_help(true);
    if let Some(desc) = decl.description.as_deref().filter(|s| !s.is_empty()) {
        top = top.about(leak_static(desc));
    }
    for sub in &decl.subcommands {
        let mut sc = Command::new(leak_static(&sub.name));
        if let Some(desc) = sub.description.as_deref().filter(|s| !s.is_empty()) {
            sc = sc.about(leak_static(desc));
        }
        if let Some(group) = decl.arg_groups.get(&sub.args) {
            sc = apply_arg_group(sc, group);
        }
        top = top.subcommand(sc);
    }
    let internal: Vec<String> = decl
        .subcommands
        .iter()
        .filter(|s| s.internal)
        .map(|s| s.name.clone())
        .collect();
    crate::help_groups::apply_plugin_internal(crate::help_i18n::localize_frame(top), &internal)
}

fn apply_arg_group(mut cmd: Command, group: &CliArgGroup) -> Command {
    for (idx, arg) in group.positional.iter().enumerate() {
        cmd = cmd.arg(build_arg(arg, Some(idx + 1)));
    }
    for arg in &group.flags {
        cmd = cmd.arg(build_arg(arg, None));
    }
    cmd
}

fn build_arg(arg: &CliArg, positional_index: Option<usize>) -> Arg {
    let mut a = Arg::new(leak_static(&arg.name));
    if let Some(i) = positional_index {
        a = a.index(i);
    } else if let Some(flag) = &arg.flag {
        a = a.long(leak_static(flag.trim_start_matches('-')));
    }
    a = a.required(arg.required);
    a = match arg.ty {
        CliArgType::Bool => a.action(ArgAction::SetTrue),
        // 반복 횟수를 잃지 않도록 Append로 받고 extract_value에서 중복을 거절한다.
        _ if arg.reject_repeat => a.action(ArgAction::Append),
        _ => a.action(ArgAction::Set),
    };
    // 음수 값을 공백으로 띄워 줄 수 있게 한다(`--secs -1`). 없으면 clap 이 `-1` 을 플래그로 보고 거절한다.
    if arg.ty == CliArgType::I64 {
        a = a.allow_negative_numbers(true);
    }
    if let Some(default) = &arg.default {
        let s = match default {
            toml::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        a = a.default_value(leak_static(&s));
    }
    if let Some(help) = arg.help.as_deref().filter(|s| !s.is_empty()) {
        a = a.help(leak_static(help));
    }
    a
}

/// Keep legacy manifest text when a key is absent; do not consult host namespaces.
fn localize_decl(decl: &mut CliCommandDecl, strings: &std::collections::HashMap<String, String>) {
    fn replace(
        value: &mut Option<String>,
        key: &Option<String>,
        strings: &std::collections::HashMap<String, String>,
    ) {
        if let Some(translated) = key
            .as_ref()
            .and_then(|key| strings.get(key))
            .filter(|v| !v.trim().is_empty())
        {
            *value = Some(translated.clone());
        }
    }
    replace(&mut decl.description, &decl.description_i18n_key, strings);
    for sub in &mut decl.subcommands {
        replace(&mut sub.description, &sub.description_i18n_key, strings);
    }
    for group in decl.arg_groups.values_mut() {
        for arg in group.positional.iter_mut().chain(&mut group.flags) {
            replace(&mut arg.help, &arg.help_i18n_key, strings);
        }
    }
}
