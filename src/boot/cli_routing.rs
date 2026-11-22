//! CLI 인자를 해석해 명령 실행·도움말·호스트 시작을 선택한다.

use crate::cli;

pub(crate) enum Routed {
    /// 이미 도움말을 출력했거나 플러그인 명령을 실행했다.
    AlreadyHandled,
    /// 명령, 선택한 port-file, 요청 timeout 설정.
    Subcommand(cli::Commands, Option<String>, cli::Envelope),
    AugmentedHelp,
    /// 호스트 실행. GUI·헤드리스 선택은 호출자가 빌드 feature로 판단한다.
    Gui(cli::Cli),
}

pub(crate) fn parse_or_route() -> anyhow::Result<Routed> {
    use clap::FromArgMatches;

    // 플러그인 인자 오류·매니페스트 경고도 번역을 사용하므로 라우팅 전에 초기화한다.
    super::locale::init();
    // UTF-8이 아닌 인자는 도움말·명령 해석·호스트 시작보다 먼저 사용 오류로 끝낸다.
    // 번역 초기화 뒤라 설정한 언어로 알린다.
    let args = cli::process_args_or_exit();

    // tasty-cli 라이브러리 버전 대신 실행 바이너리의 버전을 표시한다.
    let cmd = cli::localized_command().version(env!("CARGO_PKG_VERSION"));

    // 전체 트리는 루트 옵션 위치의 -a/--all만 요청으로 본다. 옵션 값이나 하위 명령의 데이터는 제외한다.
    if cli::help_route::scan_root(&cmd, &args).all {
        cli::print_command_tree(env!("CARGO_PKG_VERSION"))?;
        return Ok(Routed::AlreadyHandled);
    }
    let args = cli::help_route::normalize_bare_help(&cmd, &args);

    // 호스트 명령을 먼저 해석한다. 정적 트리가 모르는 이름이나 도움말 요청이면
    // 플러그인 명령을 포함한 트리로 다시 해석해 help·-h·--help가 같은 명령 집합을 보이게 한다.
    let cli = match cmd.try_get_matches_from(&args) {
        Ok(matches) => match cli::Cli::from_arg_matches(&matches) {
            Ok(cli) => cli,
            Err(err) => cli::format_parse_error(err, &args),
        },
        Err(err) => {
            if cli::help_route::needs_plugin_tree(err.kind())
                && let Some(result) = cli::try_run_plugin_cli(&args)
            {
                result?;
                return Ok(Routed::AlreadyHandled);
            }
            cli::format_parse_error(err, &args);
        }
    };

    if let Some(command) = cli.command {
        let envelope = cli::Envelope {
            response_timeout_ms: cli.response_timeout_ms,
        };
        return Ok(Routed::Subcommand(command, cli.port_file, envelope));
    }
    // 보낼 IPC 요청이 없는 도움말·호스트 시작에서는 요청 timeout 옵션을 거절한다.
    cli::Envelope {
        response_timeout_ms: cli.response_timeout_ms,
    }
    .refuse_if_set();
    #[cfg(debug_assertions)]
    let forced_launch = cli.launch;
    #[cfg(not(debug_assertions))]
    let forced_launch = false;
    if !forced_launch && std::env::var("TASTY_SURFACE_ID").is_ok() {
        return Ok(Routed::AugmentedHelp);
    }
    Ok(Routed::Gui(cli))
}
