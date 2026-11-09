//! Tasty의 명령 선언·요청 변환·클라이언트 실행. IPC 연결은 tasty_ipc::client를 사용한다.

// 이유: 테스트는 let _ = 사유 주석 정책의 대상이 아니며 제품 lint는 유지한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

pub mod caller_context;
pub mod commands;
mod contract;
pub mod cwd_resolve;
pub mod dispatch;
pub mod dynamic;
pub mod events;
pub mod format;
pub mod help;
mod help_error;
mod help_frame;
pub mod help_groups;
pub mod help_i18n;
pub mod help_route;
pub mod hook_failure;
mod install_warning;
pub mod local;
pub mod out;
pub mod plugin;
mod plugin_enable_wait;
pub mod port_file;
pub mod request;
mod rpc_error;
pub mod run;

use clap::{Parser, Subcommand};

pub use tasty_remote::browse as remote_browse;
pub use tasty_remote::create as remote_create;

pub use tasty_ssh as ssh;

pub use commands::*;
pub use help::{format_parse_error, print_augmented_help, print_command_tree};
// 실제 파싱에는 번역된 명령 트리를 사용한다.
pub use contract::Envelope;
pub use help_i18n::command as localized_command;
pub use run::{run_client, run_client_with, try_run_plugin_cli};

#[derive(Parser)]
#[command(
    name = "tasty",
    about = "GPU-accelerated terminal emulator for AI coding agents",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Custom port file path (for test isolation)
    #[arg(long)]
    pub port_file: Option<String>,

    /// Webhook listener port for this run, overriding `tasty webhook port`.
    /// If it cannot be opened, Tasty runs without the listener and leaves a warning notification.
    ///
    /// Tasty does not move to another port in that case. Without this option and without a
    /// saved port, Tasty tries 28429 and the next 63 ports and takes the first free one.
    #[arg(long, value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..))]
    pub webhook_port: Option<u16>,

    /// Server response wait limit for one request, in milliseconds. On -32061 the
    /// outcome is unknown and the request may still run; 0 or no flag means no limit.
    ///
    /// If the limit expires after processing starts, the server returns -32061. If it
    /// expires while queued, the server returns -32067 and the request did not run.
    /// The instance must declare ipc.response-timeout; otherwise the original request is not sent.
    /// Time spent checking support is subtracted from the request's remaining budget.
    /// If the check times out or uses up that budget, the result is -32067 and the original
    /// request is not sent. The check also sets a timeout for each socket read.
    /// This option is not an absolute deadline covering connection setup, sending, and
    /// repeated reads. Commands that loop or stream refuse the flag
    #[arg(long, value_name = "MS")]
    pub response_timeout_ms: Option<u64>,

    /// Force GUI launch even inside a tasty terminal
    // debug 검증 전용이다. release는 같은 홈에 프로세스를 하나만 두므로 다시 실행해도 새 GUI가 뜨지 않는다.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub launch: bool,

    /// Run headless (no GUI). Only a no-default-features build is headless; a GUI
    /// build warns and still opens a window.
    ///
    /// Headless mode runs IPC, PTYs and plugins only. A default-features (GUI) build
    /// does NOT embed headless mode: this flag logs a warning and boots the GUI anyway,
    /// so a window opens on whatever DISPLAY it inherits.
    #[arg(long, default_value_t = false)]
    pub headless: bool,

    /// Show all commands in a tree. Only valid before any command; see each command's --help for its arguments
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Enable input simulation IPC (debug builds only).
    /// Required for debug.inject_mouse, debug.inject_key, etc.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub enable_input_simulation: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new resource (window, workspace, tab)
    New {
        #[command(subcommand)]
        command: NewCommands,
    },
    /// Close a resource (tab, pane, surface, workspace, window)
    Close {
        #[command(subcommand)]
        command: CloseCommands,
    },
    /// List/query resources (workspaces, windows, tree, surfaces, panes, info, hooks, etc.)
    List {
        #[command(subcommand)]
        command: ListCommands,
    },
    /// Set/update resources (hook, mark, workspace, global-hook)
    Set {
        #[command(subcommand)]
        command: SetCommands,
    },
    /// Move/reorder a resource (tab, workspace)
    Move {
        #[command(subcommand)]
        command: MoveCommands,
    },
    /// Split a pane group or surface
    Split {
        /// Split level: pane (upper layout) or surface (lower layout)
        #[arg(long)]
        level: String,
        /// Target surface: numeric surface ID, "this" (TASTY_SURFACE_ID), or nickname
        #[arg(long = "surface", visible_alias = "target-surface")]
        target_surface: Option<String>,
        /// Target pane: numeric pane ID (only for --level pane)
        #[arg(long = "pane", visible_alias = "target-pane")]
        target_pane: Option<u32>,
        /// Split direction: vertical (default) or horizontal
        #[arg(long, default_value = "vertical")]
        direction: String,
        /// Surface type: terminal (default), markdown, explorer, html, image, dag_graph
        #[arg(long, default_value = "terminal")]
        r#type: String,
        /// Metadata JSON to set on the new surface (e.g. '{"nickname":"build"}')
        #[arg(long)]
        meta: Option<String>,
        /// Working directory: where a terminal starts, and the root of an explorer when --path is omitted
        #[arg(long)]
        cwd: Option<String>,
        /// File path (for markdown/image type)
        #[arg(long)]
        file: Option<String>,
        /// Directory path (for explorer type)
        #[arg(long)]
        path: Option<String>,
        /// URL (for html type)
        #[arg(long)]
        url: Option<String>,
    },
    /// Remote (SSH) attach to a surface/workspace on another host — `remote attach`
    Remote {
        #[command(subcommand)]
        command: RemoteCommands,
    },
    /// Send text, key, or queue message
    Send {
        #[command(subcommand)]
        command: SendCommands,
    },
    /// Read from surface or queue
    Read {
        #[command(subcommand)]
        command: ReadCommands,
    },
    /// Create a notification
    Notify {
        /// Notification body
        body: String,
        /// Optional notification title. When omitted the host fills its
        /// localized default (`notification.default_title`) — the CLI does not
        /// carry an English default of its own.
        #[arg(long)]
        title: Option<String>,
        /// Notification level
        #[arg(long, value_parser = ["error", "warning", "info"], default_value = "info")]
        level: String,
        /// Source surface ID
        #[arg(long)]
        surface: Option<u32>,
        /// Source workspace ID
        #[arg(long)]
        workspace: Option<u32>,
    },
    /// Query and acknowledge notifications
    Notification {
        #[command(subcommand)]
        command: NotificationCommands,
    },
    /// Remove resources (hook, global-hook)
    Unset {
        #[command(subcommand)]
        command: UnsetCommands,
    },
    /// Manage per-surface metadata
    SurfaceMeta {
        #[command(subcommand)]
        command: SurfaceMetaCommands,
    },
    /// Surface actions (completion signal, queries, …)
    Surface {
        #[command(subcommand)]
        command: SurfaceCommands,
    },
    /// Agent session tokens (issue, revoke, list)
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
    /// Manage child terminals (spawn, tell, children, kill).
    Terminal {
        #[command(subcommand)]
        command: TerminalCommands,
    },
    /// Manage headless PTYs (spawn/write/read/state/kill/list) — background PTYs with
    /// no Surface/tab. Separate namespace from `terminal`.
    Pty {
        #[command(subcommand)]
        command: PtyCommands,
    },
    /// Check if a surface is currently typing (received key input or a paste within 5 seconds)
    IsTyping {
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Spawn the PTY of a deferred surface (no-op if already running)
    ///
    /// Send commands auto-wake the target; use this only to start the PTY without sending input.
    Wake {
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Debug and diagnostic commands (IME simulation, raw key input, etc.) — debug builds only
    #[cfg(debug_assertions)]
    Debug {
        #[command(subcommand)]
        command: DebugCommands,
    },
    /// Internal tools (SSH connection profiles, passkeys, etc.)
    Tool {
        #[command(subcommand)]
        command: ToolCommands,
    },
    /// Manage plugins (list, install, remove, enable, disable, logs)
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },
    /// Agent memory store (~/.tasty/memory.db) — persistent key-value
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    /// Global settings — terminal input rules and remote-transfer storage
    Settings {
        #[command(subcommand)]
        command: SettingsCommands,
    },
    /// Structured output observer (parsers + sink fan-out)
    Output {
        #[command(subcommand)]
        command: OutputCommands,
    },
    /// Human-handoff approval gates (request/respond/await/cancel/list/get)
    ///
    /// The live gate does not outlive the host. `request`, `respond`, `await`,
    /// `cancel`, `list` and `get` all read one in-memory store that starts empty
    /// on every boot, so a request that was pending when the host stopped is not
    /// there afterwards — `list` will not show it and `await` on its id answers
    /// "not found". What survives is the written record, which `history` reads
    /// from the memory store. Treat `history` as the log and `list` as what is
    /// answerable right now.
    Approval {
        #[command(subcommand)]
        command: ApprovalCommands,
    },
    /// Read the host event feed by position (fetch, follow)
    ///
    /// The feed lives in memory only: a restart clears it and starts positions
    /// over, so every answer carries an `epoch` that says which generation a
    /// position belongs to. A position that has scrolled out of the ring is not
    /// quietly replaced by the oldest one — the answer says how many were
    /// skipped.
    Events {
        #[command(subcommand)]
        command: EventsCommands,
    },
    /// Agent telemetry (record metrics, summary, timeseries, top-N)
    ///
    /// Two sources feed it: what an agent reports with `telemetry record`, and
    /// one `ipc_calls` event the host writes per IPC call an agent makes, tagged
    /// with the method name (host calls, `telemetry.*` and throttled calls are not
    /// counted). Queries read the raw events and nothing else — there is no rollup
    /// — and the store is trimmed to the most recent 20,000, so `--since` cannot
    /// reliably reach past that. Anomaly records are trimmed separately: the most
    /// recent 5,000, and at most 50 hours.
    Telemetry {
        #[command(subcommand)]
        command: TelemetryCommands,
    },
    /// Agent tasks, barriers, semaphores, leases, reducers, and rate limits.
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// File handler — reload the user configuration (`~/.tasty/file-handlers.toml`),
    /// list the merged file format detectors, or dispatch a path
    FileHandler {
        #[command(subcommand)]
        command: FileHandlerCommands,
    },
    /// Clipboard — operate on the local clipboard
    Clipboard {
        #[command(subcommand)]
        command: ClipboardCommands,
    },
    /// Hook handler — shared hook/webhook handler registry (list / get / upsert / remove / reload / dispatch).
    HookHandler {
        #[command(subcommand)]
        command: HookHandlerCommands,
    },
    /// List registered task completion strategies.
    CompletionStrategy {
        #[command(subcommand)]
        command: CompletionStrategyCommands,
    },
    /// Manage layout presets (workspace / tab / pane).
    Preset {
        #[command(subcommand)]
        command: PresetCommands,
    },
    /// Manage workspace categories (sidebar folders) — list/create/rename/delete/move.
    WorkspaceCategory {
        #[command(subcommand)]
        command: WorkspaceCategoryCommands,
    },
    /// Manage inbound webhooks (register/list/info/unregister) — HTTP → IpcSequence.
    Webhook {
        #[command(subcommand)]
        command: WebhookCommands,
    },
    /// Print this instance's IPC port to stdout (first step of the auto remote
    /// port-discovery chain, `ssh host tasty port`). Reads the port file only — no IPC.
    Port,
    /// Capture a screenshot to a PNG file (focus-independent).
    ///
    /// With `--surface`, captures that terminal surface via an offscreen render at
    /// its own grid size — works for background tabs/workspaces and never changes
    /// focus or the visible frame. Otherwise captures a whole window: `--window`
    /// selects it by ID (see `list windows`); if omitted and only one window is
    /// open, that one is used.
    ///
    /// An explicit `--window` may name a window that `list windows` does not show,
    /// such as the settings or plugins modal. Only the automatic (no `--window`)
    /// choice is limited to main windows, and it never falls back to whichever
    /// window happens to be focused.
    Screenshot {
        /// Output PNG path.
        #[arg(long)]
        path: String,
        /// Terminal surface ID to capture (offscreen; focus-independent).
        #[arg(long)]
        surface: Option<u32>,
        /// Window ID to capture (whole tasty frame). Required when multiple windows are
        /// open. May name a modal window that `list windows` does not enumerate.
        #[arg(long)]
        window: Option<u64>,
    },
}

#[cfg(test)]
mod attach_surface_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn remote_attach_parses() {
        let cli =
            Cli::try_parse_from(["tasty", "remote", "attach", "5", "--ssh", "user@host"]).unwrap();
        let Some(Commands::Remote {
            command: RemoteCommands::Attach { surface, ssh, .. },
        }) = cli.command
        else {
            panic!("expected remote attach");
        };
        assert_eq!(surface, Some(5));
        assert_eq!(ssh.as_deref(), Some("user@host"));
    }

    #[test]
    fn remote_attach_into_gui_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "attach",
            "--profile",
            "gx10",
            "--workspace",
            "2",
            "--into-gui",
            "--target-port",
            "45123",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::Attach {
                    profile,
                    workspace,
                    into_gui,
                    target_port,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected remote attach");
        };
        assert_eq!(profile.as_deref(), Some("gx10"));
        assert_eq!(workspace, Some(2));
        assert!(into_gui);
        assert_eq!(target_port, Some(45123));
    }

    #[test]
    fn remote_attach_into_gui_new_workspace_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "attach",
            "--into-gui",
            "--new-workspace",
            "--profile",
            "gx10",
            "--name",
            "build",
            "--cwd",
            "/srv/app",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::Attach {
                    new_workspace,
                    name,
                    cwd,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected remote attach");
        };
        assert!(new_workspace);
        assert_eq!(name.as_deref(), Some("build"));
        assert_eq!(cwd.as_deref(), Some("/srv/app"));
    }

    #[test]
    fn remote_attach_new_workspace_rejects_what_it_replaces() {
        let attach = ["tasty", "remote", "attach", "--ssh", "h"];
        for extra in [
            &["--new-workspace"][..],
            &["--into-gui", "--new-workspace", "--workspace", "1"][..],
            &["--into-gui", "--new-workspace", "--target-port", "1"][..],
            &["--into-gui", "--new-workspace", "7"][..],
            &["--into-gui", "--workspace", "1", "--name", "n"][..],
            &["--into-gui", "--workspace", "1", "--cwd", "/x"][..],
        ] {
            let args: Vec<&str> = attach.iter().chain(extra).copied().collect();
            assert!(Cli::try_parse_from(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn remote_new_workspace_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "new-workspace",
            "--profile",
            "gx10",
            "--name",
            "build",
            "--cwd",
            "/srv/app",
            "--json",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::NewWorkspace {
                    profile,
                    ssh,
                    name,
                    cwd,
                    json,
                    remote_tasty,
                    remote_port_mode,
                },
        }) = cli.command
        else {
            panic!("expected remote new-workspace");
        };
        assert_eq!(profile.as_deref(), Some("gx10"));
        assert_eq!(ssh, None);
        assert_eq!(name.as_deref(), Some("build"));
        assert_eq!(cwd.as_deref(), Some("/srv/app"));
        assert!(json);
        assert_eq!(remote_tasty, "tasty");
        assert_eq!(remote_port_mode, "auto");
    }

    #[test]
    fn remote_new_workspace_loopback_minimal_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "new-workspace",
            "--ssh",
            "127.0.0.1:45123",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::NewWorkspace {
                    ssh,
                    profile,
                    name,
                    cwd,
                    json,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected remote new-workspace");
        };
        assert_eq!(ssh.as_deref(), Some("127.0.0.1:45123"));
        assert_eq!(profile, None);
        assert_eq!(name, None);
        assert_eq!(cwd, None);
        assert!(!json);
    }

    #[test]
    fn top_level_attach_removed() {
        assert!(Cli::try_parse_from(["tasty", "attach", "5"]).is_err());
    }

    // 원격 사용자를 끊는 release CLI는 없다. 로컬 사용자는 GUI로 끊는다.
    #[test]
    fn remote_attach_has_no_force_detach() {
        for args in [
            &["tasty", "remote", "attach", "5", "--force-detach"][..],
            &[
                "tasty",
                "remote",
                "attach",
                "--workspace",
                "3",
                "--force-detach",
            ][..],
        ] {
            let err = match Cli::try_parse_from(args) {
                Ok(_) => panic!("{args:?} parsed"),
                Err(e) => e,
            };
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::UnknownArgument,
                "{args:?}"
            );
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_attach_force_detach_uses_debug_methods() {
        for (args, method, key, value) in [
            (
                &["tasty", "debug", "attach", "5", "--force-detach"][..],
                "debug.attach.force_detach",
                "surface_id",
                5,
            ),
            (
                &[
                    "tasty",
                    "debug",
                    "attach",
                    "--workspace",
                    "3",
                    "--force-detach",
                ][..],
                "debug.attach.force_detach_workspace",
                "workspace_id",
                3,
            ),
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            let command = cli.command.unwrap();
            assert!(matches!(
                command.dispatch().unwrap(),
                crate::dispatch::Dispatch::Rpc
            ));
            let req = crate::request::command_to_request(&command);
            assert_eq!(req.method, method);
            assert_eq!(req.params[key], value);
        }
    }

    #[test]
    fn remote_check_parses() {
        let cli = Cli::try_parse_from(["tasty", "remote", "check", "--ssh", "user@host"]).unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::Check {
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                },
        }) = cli.command
        else {
            panic!("expected remote check");
        };
        assert_eq!(ssh.as_deref(), Some("user@host"));
        assert_eq!(profile, None);
        assert_eq!(remote_tasty, "tasty");
        assert_eq!(remote_port_mode, "auto");
    }

    #[test]
    fn remote_check_profile_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "check",
            "--profile",
            "gx10",
            "--remote-port-mode",
            "file-unix",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::Check {
                    profile,
                    remote_port_mode,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected remote check");
        };
        assert_eq!(profile.as_deref(), Some("gx10"));
        assert_eq!(remote_port_mode, "file-unix");
    }

    #[test]
    fn remote_check_ssh_profile_rejected() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from(["tasty", "remote", "check", "--ssh", "h", "--profile", "p"])
            .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.ssh_profile_exclusive")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_check_no_target_rejected() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from(["tasty", "remote", "check"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.remote_check_target_required")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_workspaces_parses() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "workspaces",
            "--ssh",
            "user@host",
            "--json",
        ])
        .unwrap();
        let Some(Commands::Remote {
            command:
                RemoteCommands::Workspaces {
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                    json,
                },
        }) = cli.command
        else {
            panic!("expected remote workspaces");
        };
        assert_eq!(ssh.as_deref(), Some("user@host"));
        assert_eq!(profile, None);
        assert_eq!(remote_tasty, "tasty");
        assert_eq!(remote_port_mode, "auto");
        assert!(json);
    }

    #[test]
    fn remote_workspaces_ssh_profile_rejected() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "workspaces",
            "--ssh",
            "h",
            "--profile",
            "p",
        ])
        .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.ssh_profile_exclusive")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_workspaces_no_target_rejected() {
        let cli = Cli::try_parse_from(["tasty", "remote", "workspaces"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string().contains("원격 대상이 필요합니다"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_check_unknown_profile_rejected() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli =
            Cli::try_parse_from(["tasty", "remote", "check", "--profile", "__nope__"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        let expected = tasty_i18n::t_fmt("cli.remote_profile.not_found_list_hint", "__nope__");
        assert!(
            err.to_string().contains(&expected),
            "unexpected error: {err}"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_attach_parses() {
        let cli = Cli::try_parse_from(["tasty", "debug", "attach", "5", "--raw"]).unwrap();
        let Some(Commands::Debug {
            command: DebugCommands::Attach { surface, raw, .. },
        }) = cli.command
        else {
            panic!("expected debug attach");
        };
        assert_eq!(surface, Some(5));
        assert!(raw);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_attach_has_no_ssh() {
        assert!(Cli::try_parse_from(["tasty", "debug", "attach", "5", "--ssh", "h"]).is_err());
    }

    #[test]
    fn remote_attach_without_target_rejected() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from(["tasty", "remote", "attach", "5"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.remote_attach_target_required")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_attach_surface_workspace_exclusive() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "attach",
            "5",
            "--workspace",
            "2",
            "--ssh",
            "h",
        ])
        .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.surface_workspace_exclusive")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remote_attach_ssh_profile_exclusive() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "attach",
            "5",
            "--ssh",
            "h",
            "--profile",
            "p",
        ])
        .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.ssh_profile_exclusive")),
            "unexpected error: {err}"
        );
    }
}

#[cfg(test)]
mod workspace_category_tests {
    use super::*;
    use crate::request::command_to_request;
    use clap::Parser;

    fn req(args: &[&str]) -> tasty_ipc::protocol::JsonRpcRequest {
        let cli = Cli::try_parse_from(args).unwrap();
        command_to_request(&cli.command.unwrap())
    }

    #[test]
    fn category_list_maps_to_ipc() {
        let r = req(&["tasty", "workspace-category", "list"]);
        assert_eq!(r.method, "workspace_category.list");
    }

    #[test]
    fn category_create_maps_to_ipc() {
        let r = req(&["tasty", "workspace-category", "create", "--name", "Work"]);
        assert_eq!(r.method, "workspace_category.create");
        assert_eq!(r.params["name"], "Work");
    }

    #[test]
    fn category_rename_delete_move_map() {
        let r = req(&[
            "tasty",
            "workspace-category",
            "rename",
            "--id",
            "3",
            "--name",
            "Play",
        ]);
        assert_eq!(r.method, "workspace_category.rename");
        assert_eq!(r.params["id"], 3);
        assert_eq!(r.params["name"], "Play");

        let r = req(&["tasty", "workspace-category", "delete", "--id", "3"]);
        assert_eq!(r.method, "workspace_category.delete");
        assert_eq!(r.params["id"], 3);

        let r = req(&[
            "tasty",
            "workspace-category",
            "move",
            "--from",
            "2",
            "--to",
            "1",
        ]);
        assert_eq!(r.method, "workspace_category.move");
        assert_eq!(r.params["from_index"], 2);
        assert_eq!(r.params["to_index"], 1);
        assert!(
            r.params.get("id").is_none(),
            "안 준 키를 null 로 실으면 안 된다"
        );
    }

    #[test]
    fn move_by_id_sends_id_and_omits_the_index() {
        let r = req(&[
            "tasty",
            "workspace-category",
            "move",
            "--id",
            "3",
            "--to",
            "1",
        ]);
        assert_eq!(r.method, "workspace_category.move");
        assert_eq!(r.params["id"], 3);
        assert_eq!(r.params["to_index"], 1);
        assert!(r.params.get("from_index").is_none());

        let r = req(&["tasty", "move", "workspace", "--id", "7", "--to", "0"]);
        assert_eq!(r.method, "workspace.move");
        assert_eq!(r.params["id"], 7);
        assert_eq!(r.params["to_index"], 0);
        assert!(r.params.get("from_index").is_none());
    }

    #[test]
    fn new_workspace_carries_category() {
        let r = req(&[
            "tasty",
            "new",
            "workspace",
            "--name",
            "x",
            "--category",
            "Work",
        ]);
        assert_eq!(r.method, "workspace.create");
        assert_eq!(r.params["category"], "Work");
    }

    #[test]
    fn set_workspace_carries_category() {
        let r = req(&["tasty", "set", "workspace", "--id", "2", "--category", "5"]);
        assert_eq!(r.method, "workspace.update");
        assert_eq!(r.params["category"], "5");
    }

    #[test]
    fn surface_completion_maps_to_ipc() {
        let r = req(&["tasty", "surface", "completion", "--surface", "42"]);
        assert_eq!(r.method, "surface.completion");
        assert_eq!(r.params["surface_id"], 42);
        assert!(r.params["kind"].is_null());
    }

    #[test]
    fn surface_completion_carries_kind() {
        let r = req(&[
            "tasty",
            "surface",
            "completion",
            "--surface",
            "42",
            "--kind",
            "needs_input",
        ]);
        assert_eq!(r.method, "surface.completion");
        assert_eq!(r.params["kind"], "needs_input");
    }

    #[test]
    fn surface_html_script_maps_to_ipc() {
        let r = req(&["tasty", "surface", "html-script", "--surface", "42"]);
        assert_eq!(r.method, "surface.html_script");
        assert_eq!(r.params["surface_id"], 42);
    }

    #[test]
    fn surface_html_remote_allow_maps_to_ipc() {
        let r = req(&["tasty", "surface", "html-remote-allow", "--surface", "42"]);
        assert_eq!(r.method, "surface.html_remote_allow");
        assert_eq!(r.params, serde_json::json!({ "surface_id": 42 }));
        let r = req(&[
            "tasty",
            "surface",
            "html-remote-allow",
            "--surface",
            "42",
            "--url",
            "file:///a.html",
            "--fingerprint",
            "ab",
        ]);
        assert_eq!(
            r.params,
            serde_json::json!({ "surface_id": 42, "url": "file:///a.html", "fingerprint": "ab" })
        );
    }

    #[test]
    fn surface_html_script_allow_maps_to_ipc() {
        let r = req(&["tasty", "surface", "html-script-allow", "--surface", "42"]);
        assert_eq!(r.method, "surface.html_script_allow");
        assert_eq!(r.params, serde_json::json!({ "surface_id": 42 }));
        let r = req(&[
            "tasty",
            "surface",
            "html-script-allow",
            "--surface",
            "42",
            "--url",
            "file:///a.html",
            "--fingerprint",
            "ab",
        ]);
        assert_eq!(r.params["url"], "file:///a.html");
        assert_eq!(r.params["fingerprint"], "ab");
    }

    #[test]
    fn surface_html_nav_maps_to_ipc() {
        let r = req(&["tasty", "surface", "html-nav", "--surface", "42"]);
        assert_eq!(r.method, "surface.html_nav");
        assert_eq!(r.params["surface_id"], 42);
    }

    #[test]
    fn surface_html_history_steps_map_to_ipc() {
        for (cmd, method) in [
            ("html-back", "surface.html_back"),
            ("html-forward", "surface.html_forward"),
            ("html-reload", "surface.html_reload"),
        ] {
            let r = req(&["tasty", "surface", cmd, "--surface", "42"]);
            assert_eq!(r.method, method);
            assert_eq!(r.params["surface_id"], 42);
            assert_eq!(r.params.as_object().map(|o| o.len()), Some(1), "{cmd}");
        }
    }

    #[test]
    fn surface_html_history_steps_require_a_surface() {
        for cmd in ["html-back", "html-forward", "html-reload"] {
            assert!(
                Cli::try_parse_from(["tasty", "surface", cmd]).is_err(),
                "{cmd} parsed without --surface"
            );
        }
    }

    #[test]
    fn surface_attention_get_maps_to_ipc() {
        let r = req(&["tasty", "surface", "attention", "get", "--surface", "42"]);
        assert_eq!(r.method, "surface.attention.get");
        assert_eq!(r.params["surface_id"], 42);
    }

    #[test]
    fn surface_attention_clear_maps_to_ipc() {
        let r = req(&["tasty", "surface", "attention", "clear", "--surface", "42"]);
        assert_eq!(r.method, "surface.attention.clear");
        assert_eq!(r.params["surface_id"], 42);
        assert!(r.params["kind"].is_null());
    }

    #[test]
    fn surface_attention_clear_carries_kind_filter() {
        let r = req(&[
            "tasty",
            "surface",
            "attention",
            "clear",
            "--surface",
            "42",
            "--kind",
            "needs_input",
        ]);
        assert_eq!(r.method, "surface.attention.clear");
        assert_eq!(r.params["kind"], "needs_input");
    }

    #[test]
    fn graph_commands_default_to_the_table_and_ask_the_host_for_json() {
        let cases: [(&[&str], &str); 2] = [
            (
                &["tasty", "agent", "task-graph", "--workspace", "1"],
                "agent.task_graph",
            ),
            (
                &["tasty", "agent", "dag-get", "--id", "c:a"],
                "agent.dag_get",
            ),
        ];
        for (args, method) in cases {
            let r = req(args);
            assert_eq!(r.method, method);
            assert_eq!(r.params["format"], "json", "table is rendered by the CLI");
            for format in ["json", "dot"] {
                let mut explicit = args.to_vec();
                explicit.extend(["--format", format]);
                assert_eq!(req(&explicit).params["format"], format);
            }
        }
    }

    #[test]
    fn task_graph_submit_fills_run_workspace_and_dry_run_only_validates() {
        let graph = r#"{"contract_version":2,"tasks":[
            {"id":"a","command":{"kind":"run","command":["true"]}},
            {"id":"b","command":{"kind":"custom","ipc_method":"system.ping"}}]}"#;
        let r = req(&[
            "tasty",
            "agent",
            "task-graph-submit",
            "--workspace-id",
            "3",
            "--graph",
            graph,
        ]);
        assert_eq!(r.method, "agent.task_graph_submit");
        assert_eq!(r.params["workspace_id"], 3);
        assert_eq!(r.params["graph"]["tasks"][0]["command"]["workspace_id"], 3);
        let r = req(&[
            "tasty",
            "agent",
            "task-graph-submit",
            "--workspace-id",
            "3",
            "--graph",
            graph,
            "--dry-run",
        ]);
        assert_eq!(r.method, "agent.task_graph_validate");
    }

    /// workspace_id 는 그 필드가 있는 run·agent command(inline fallback 포함)에만 넣는다.
    #[test]
    fn task_graph_submit_fills_workspace_only_where_the_command_has_it() {
        let graph = r#"{"contract_version":2,"tasks":[
            {"id":"r","command":{"kind":"run","command":["true"]}},
            {"id":"a","command":{"kind":"agent","provider":"claude","instruction":"i","session":{"kind":"existing","surface_id":1}}},
            {"id":"w","command":{"kind":"wait_barrier","name":"b"}},
            {"id":"d","command":{"kind":"reduce","inputs":["r"],"strategy":{"kind":"all"}}},
            {"id":"c","command":{"kind":"custom","ipc_method":"system.ping"},
             "on_failure":{"kind":"fallback","inline":{"name":"f","command":{"kind":"run","command":["true"]}}}}]}"#;
        let r = req(&[
            "tasty",
            "agent",
            "task-graph-submit",
            "--workspace-id",
            "3",
            "--graph",
            graph,
        ]);
        let tasks = &r.params["graph"]["tasks"];
        assert_eq!(tasks[0]["command"]["workspace_id"], 3);
        assert_eq!(tasks[1]["command"]["workspace_id"], 3);
        for i in 2..5 {
            assert!(
                tasks[i]["command"].get("workspace_id").is_none(),
                "{}",
                tasks[i]["command"]
            );
        }
        assert_eq!(
            tasks[4]["on_failure"]["inline"]["command"]["workspace_id"],
            3
        );
    }

    #[test]
    fn task_create_concurrency_limit_sets_semaphore_metadata() {
        let r = req(&[
            "tasty",
            "agent",
            "task-create",
            "--workspace-id",
            "1",
            "--name",
            "t",
            "--command",
            r#"{"kind":"run","command":["true"]}"#,
            "--concurrency-limit",
            "cap2",
        ]);
        assert_eq!(r.method, "agent.task_create");
        assert_eq!(r.params["metadata"]["semaphore"]["name"], "cap2");
    }

    #[test]
    fn task_create_without_concurrency_limit_has_no_metadata() {
        let r = req(&[
            "tasty",
            "agent",
            "task-create",
            "--workspace-id",
            "1",
            "--name",
            "t",
            "--command",
            r#"{"kind":"run","command":["true"]}"#,
        ]);
        assert!(r.params.get("metadata").is_none());
    }

    #[test]
    fn task_create_concurrency_limit_merges_with_existing_metadata() {
        let r = req(&[
            "tasty",
            "agent",
            "task-create",
            "--workspace-id",
            "1",
            "--name",
            "t",
            "--command",
            r#"{"kind":"run","command":["true"]}"#,
            "--metadata",
            r#"{"foo":"bar"}"#,
            "--concurrency-limit",
            "cap2",
        ]);
        assert_eq!(r.params["metadata"]["foo"], "bar");
        assert_eq!(r.params["metadata"]["semaphore"]["name"], "cap2");
    }

    #[test]
    fn task_create_reserved_for_fallback_flag_maps_to_ipc_param() {
        let r = req(&[
            "tasty",
            "agent",
            "task-create",
            "--workspace-id",
            "1",
            "--name",
            "t",
            "--command",
            r#"{"kind":"run","command":["true"]}"#,
            "--reserved-for-fallback",
        ]);
        assert_eq!(r.params["reserved_for_fallback"], true);
    }

    #[test]
    fn task_create_without_reserved_for_fallback_omits_param() {
        let r = req(&[
            "tasty",
            "agent",
            "task-create",
            "--workspace-id",
            "1",
            "--name",
            "t",
            "--command",
            r#"{"kind":"run","command":["true"]}"#,
        ]);
        assert!(r.params.get("reserved_for_fallback").is_none());
    }

    #[test]
    fn webhook_register_inline_sequence_maps_to_ipc() {
        let r = req(&[
            "tasty",
            "webhook",
            "register",
            "--method",
            "POST",
            "--sequence",
            r#"[{"method":"notification.create","params":{"body":"${body.m}"}}]"#,
        ]);
        assert_eq!(r.method, "webhook.register");
        assert_eq!(r.params["methods"][0], "POST");
        assert_eq!(r.params["sequence"][0]["method"], "notification.create");
    }

    #[test]
    fn webhook_register_without_method_leaves_the_default_to_the_server() {
        let r = req(&["tasty", "webhook", "register", "--handler", "host/notify"]);
        assert_eq!(r.method, "webhook.register");
        assert!(r.params["methods"].is_null(), "{:?}", r.params["methods"]);
    }

    #[test]
    fn webhook_register_handler_maps_to_ipc() {
        let r = req(&["tasty", "webhook", "register", "--handler", "host/notify"]);
        assert_eq!(r.method, "webhook.register");
        assert_eq!(r.params["handler"], "host/notify");
    }

    #[test]
    fn webhook_register_auth_flags_map_to_auth_object() {
        let r = req(&[
            "tasty",
            "webhook",
            "register",
            "--handler",
            "host/notify",
            "--auth-location",
            "query",
            "--auth-key",
            "token",
            "--auth-token",
            "s3cret",
        ]);
        assert_eq!(r.method, "webhook.register");
        assert_eq!(r.params["auth"]["location"], "query");
        assert_eq!(r.params["auth"]["key"], "token");
        assert_eq!(r.params["auth"]["token"], "s3cret");
    }

    #[test]
    fn webhook_register_without_auth_flags_omits_auth() {
        let r = req(&["tasty", "webhook", "register", "--handler", "host/notify"]);
        assert!(r.params["auth"].is_null());
    }

    #[test]
    fn webhook_list_info_unregister_map() {
        let r = req(&["tasty", "webhook", "list"]);
        assert_eq!(r.method, "webhook.list");

        let r = req(&["tasty", "webhook", "info", "--id", "abc123"]);
        assert_eq!(r.method, "webhook.info");
        assert_eq!(r.params["id"], "abc123");

        let r = req(&["tasty", "webhook", "unregister", "--id", "abc123"]);
        assert_eq!(r.method, "webhook.unregister");
        assert_eq!(r.params["id"], "abc123");
    }

    #[test]
    fn file_handler_detectors_maps_to_the_read_method() {
        let r = req(&["tasty", "file-handler", "detectors"]);
        assert_eq!(r.method, "file_handler.detectors");
        assert_eq!(r.params, serde_json::json!({}));
    }

    #[test]
    fn hook_handler_list_reload_map() {
        let r = req(&["tasty", "hook-handler", "list"]);
        assert_eq!(r.method, "hook_handler.list");

        let r = req(&["tasty", "hook-handler", "reload"]);
        assert_eq!(r.method, "hook_handler.reload");
    }

    #[test]
    fn hook_handler_dispatch_maps_id_and_body() {
        let r = req(&[
            "tasty",
            "hook-handler",
            "dispatch",
            "--id",
            "host/notify",
            "--body",
            r#"{"message":"hi"}"#,
        ]);
        assert_eq!(r.method, "hook_handler.dispatch");
        assert_eq!(r.params["id"], "host/notify");
        assert_eq!(r.params["body"]["message"], "hi");
    }

    #[test]
    fn hook_handler_get_and_remove_map_id() {
        let r = req(&["tasty", "hook-handler", "get", "--id", "user/x"]);
        assert_eq!(r.method, "hook_handler.get");
        assert_eq!(r.params["id"], "user/x");

        let r = req(&["tasty", "hook-handler", "remove", "--id", "user/x"]);
        assert_eq!(r.method, "hook_handler.remove");
        assert_eq!(r.params["id"], "user/x");
    }

    #[test]
    fn hook_handler_upsert_wraps_calls_shorthand() {
        let r = req(&[
            "tasty",
            "hook-handler",
            "upsert",
            "--id",
            "user/x",
            "--calls",
            r#"[{"method":"window.focus","params":{}}]"#,
        ]);
        assert_eq!(r.method, "hook_handler.upsert");
        assert_eq!(r.params["action"]["kind"], "ipc_sequence");
        assert_eq!(r.params["action"]["calls"][0]["method"], "window.focus");
        assert!(r.params["source"].is_null());
        assert!(r.params["priority"].is_null());
    }

    #[test]
    fn hook_handler_upsert_passes_action_through() {
        let r = req(&[
            "tasty",
            "hook-handler",
            "upsert",
            "--id",
            "user/x",
            "--source",
            "hook",
            "--disabled",
            "true",
            "--action",
            r#"{"kind":"shell_command","command":"echo hi"}"#,
        ]);
        assert_eq!(r.params["action"]["kind"], "shell_command");
        assert_eq!(r.params["source"], "hook");
        assert_eq!(r.params["disabled"], true);
    }

    #[test]
    fn hook_handler_upsert_keeps_malformed_json_instead_of_dropping_it() {
        let r = req(&[
            "tasty",
            "hook-handler",
            "upsert",
            "--id",
            "user/x",
            "--calls",
            "[{oops",
        ]);
        assert_eq!(r.params["action"]["calls"], "[{oops");
    }

    #[test]
    fn hook_handler_dispatch_without_context_omits_it() {
        let r = req(&["tasty", "hook-handler", "dispatch", "--id", "user/x"]);
        assert_eq!(r.method, "hook_handler.dispatch");
        assert!(r.params["body"].is_null());
        assert!(r.params["headers"].is_null());
        assert!(r.params["query"].is_null());
    }
}
