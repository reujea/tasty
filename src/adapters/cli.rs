//! 부팅에 필요한 CLI 진입점만 명시적으로 재수출한다.
//! GUI·IPC·앱 상태에서 CLI 전체를 사용하지 않도록 glob 재수출은 피한다.
//! 양쪽에 필요한 기능은 tasty-ssh·tasty-remote·tasty_ipc::client에서 공유한다.

pub use tasty_cli::{
    Cli, Commands, Envelope, format_parse_error, help_route, localized_command,
    print_augmented_help, print_command_tree, process_args_or_exit, run_client_with,
    try_run_plugin_cli,
};
