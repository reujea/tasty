//! Tasty CLI — clap subcommand surface + request/run dispatch.
//!
//! 클라이언트 IPC 연결(`IpcConnection` / `StreamConnection`)은 서버·프레이밍과
//! 같은 곳에 있다 — `tasty_ipc::client`.
//!
//! 본 바이너리 src/adapters/cli/ 의 전 내용을 흡수했다.

// 이유: 테스트 본문의 `let _ =` 는 정책이 사유를 요구하지 않는 자리라
// `clippy::let_underscore_must_use` 명부에 섞이면 안 된다 — 그 명부는 프로덕션에서
// 값을 버리는 자리의 목록이고, 테스트가 늘 때마다 숫자만 흔들리면 새 프로덕션
// 자리가 그 안에 묻힌다(docs/dev-guide/error-handling.md). `cfg_attr(test, ..)` 라
// 라이브러리 타깃의 판정은 그대로다 — 프로덕션 자리는 여전히 명부에 오른다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

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
pub mod help_i18n;
pub mod hook_failure;
pub mod local;
pub mod out;
pub mod plugin;
pub mod port_file;
pub mod request;
mod rpc_error;
pub mod run;

use clap::{Parser, Subcommand};

// 원격 인스턴스 조회/생성 코어는 `tasty-remote` 크레이트로 분리됐다. 내부
// `crate::remote_browse::` / `crate::remote_create::` 경로를 유지하기 위한 재수출.
pub use tasty_remote::browse as remote_browse;
pub use tasty_remote::create as remote_create;

// SSH 위임 계층은 `tasty-ssh` 크레이트로 분리됐다. 내부 `crate::ssh::` 경로를
// 유지하기 위한 재수출 (`docs/dev-guide/build.md` §크레이트 분리 가이드).
pub use tasty_ssh as ssh;

pub use commands::*;
pub use help::{format_parse_error, print_augmented_help, print_command_tree};
// 번역이 적용된 clap 트리. 프로덕션은 `Cli::command()` 대신 이것을 쓴다 — 근거는
// `help_i18n::command`. 배경 설명이 한국어라 `///` 가 아니라 `//` 다: 이 파일은
// `no_hardcoded_ui_strings` 의 clap 도움말 스캔 뿌리라, 게이트 밖 `///` 의 CJK 는
// 부착 대상과 무관하게 걸린다.
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

    /// Bound, in milliseconds, on how long a single-request command waits for its reply.
    /// When it runs out the instance answers -32061 if the request had started: the
    /// outcome is unknown and the request may still run. If it was still queued the
    /// answer is -32067 and nothing ran. 0 or no flag waits without a bound. The instance
    /// must declare ipc.response-timeout, or nothing is sent. Checking that shares the same
    /// bound: if the check does not finish in time the answer is -32067 and nothing was
    /// sent. Commands that loop or stream refuse the flag
    #[arg(long, value_name = "MS")]
    pub response_timeout_ms: Option<u64>,

    /// Force GUI launch even inside a tasty terminal
    #[arg(long)]
    pub launch: bool,

    /// Run as headless terminal emulator (no GUI, IPC + PTY + plugin only).
    /// Only a no-default-features build is headless. A default-features (GUI)
    /// build does NOT embed headless mode: this flag logs a warning and boots
    /// the GUI anyway, so a window opens on whatever DISPLAY it inherits.
    #[arg(long, default_value_t = false)]
    pub headless: bool,

    /// Show all commands in a tree (use with -h)
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
    /// Create a new resource (window, workspace, tab, split)
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
        #[arg(long)]
        target_surface: Option<String>,
        /// Target pane: numeric pane ID (only for --level pane)
        #[arg(long)]
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
        /// Working directory (for terminal type)
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
    /// Manage child terminals (spawn/tell/children/kill/…) — host-internalized
    /// agent child-terminal management (ADR-0040).
    Terminal {
        #[command(subcommand)]
        command: TerminalCommands,
    },
    /// Manage headless PTYs (spawn/write/read/wait/kill/list) — background PTYs with
    /// no Surface/tab. Separate namespace from `terminal` (ADR-0050 pty primitive).
    Pty {
        #[command(subcommand)]
        command: PtyCommands,
    },
    /// Check if a surface is currently typing (received key input within 5 seconds)
    IsTyping {
        /// Surface ID (default: focused)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Spawn the PTY of a deferred surface (no-op if already running)
    ///
    /// Send commands auto-wake the target; use this only to start the PTY without sending input.
    Wake {
        /// Surface ID (default: focused)
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
    /// Global settings — get/set the remote-transfer storage folder and size cap
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
    /// Agent collaboration primitives (task DAG; barrier/semaphore/lease/reducer/rate-limit follow)
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// File handler — reload the user configuration (`~/.tasty/file-handlers.toml`)
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
    /// Completion strategy — task Custom-dispatch completion judge registry (list).
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

    // 원격 attach 는 `tasty remote attach` 네임스페이스로 파싱된다.
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

    // `--profile` / `--into-gui` 등 원격 부분집합 플래그가 remote 네임스페이스에 있다.
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

    // `remote new-workspace` — 원격 mutate 1건(생성). 출력 id 를 `remote attach
    // --workspace <id>` 로 넘기는 CLI 복합 경로의 앞단.
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
        // 기본값은 `remote workspaces` 와 동일해야 한다(같은 포트 발견 체인).
        assert_eq!(remote_tasty, "tasty");
        assert_eq!(remote_port_mode, "auto");
    }

    // loopback e2e 형태(`--ssh 127.0.0.1:<port>`)와 생성 옵션 전부 생략도 파싱된다.
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

    // top-level `tasty attach` 는 release 표면에서 완전히 제거되었다.
    #[test]
    fn top_level_attach_removed() {
        assert!(Cli::try_parse_from(["tasty", "attach", "5"]).is_err());
    }

    // remote attach 는 `--force-detach`(원격 클라이언트 attach 락 강제해제)를 갖는다.
    #[test]
    fn remote_attach_force_detach_parses() {
        let cli =
            Cli::try_parse_from(["tasty", "remote", "attach", "5", "--force-detach"]).unwrap();
        let Some(Commands::Remote {
            command: RemoteCommands::Attach { force_detach, .. },
        }) = cli.command
        else {
            panic!("expected remote attach");
        };
        assert!(force_detach);
    }

    // remote attach 의 런타임 가드: `--ssh` 와 `--force-detach` 는 상호배타.
    #[test]
    fn remote_attach_ssh_force_detach_rejected() {
        let cli = Cli::try_parse_from([
            "tasty",
            "remote",
            "attach",
            "5",
            "--ssh",
            "h",
            "--force-detach",
        ])
        .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.ssh_force_detach_exclusive")),
            "unexpected error: {err}"
        );
    }

    // `tasty remote check --ssh user@host` 가 Check 변형으로 파싱된다(기본값 포함).
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
        // attach 와 동일한 기본값.
        assert_eq!(remote_tasty, "tasty");
        assert_eq!(remote_port_mode, "auto");
    }

    // `remote check --profile <name>` + 발견 모드 오버라이드가 파싱된다.
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

    // 런타임 가드: `remote check` 의 `--ssh` 와 `--profile` 는 상호배타.
    #[test]
    fn remote_check_ssh_profile_rejected() {
        let cli = Cli::try_parse_from(["tasty", "remote", "check", "--ssh", "h", "--profile", "p"])
            .unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.ssh_profile_exclusive")),
            "unexpected error: {err}"
        );
    }

    // 런타임 가드: 대상(`--ssh`/`--profile`) 없이 `remote check` → 명확한 거부.
    #[test]
    fn remote_check_no_target_rejected() {
        let cli = Cli::try_parse_from(["tasty", "remote", "check"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.remote_check_target_required")),
            "unexpected error: {err}"
        );
    }

    // `tasty remote workspaces --ssh user@host` 가 Workspaces 변형으로 파싱된다.
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

    // 런타임 가드: `remote workspaces` 의 `--ssh` 와 `--profile` 는 상호배타.
    #[test]
    fn remote_workspaces_ssh_profile_rejected() {
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

    // 런타임 가드: 대상(`--ssh`/`--profile`) 없이 `remote workspaces` → 명확한 거부.
    #[test]
    fn remote_workspaces_no_target_rejected() {
        let cli = Cli::try_parse_from(["tasty", "remote", "workspaces"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string().contains("원격 대상이 필요합니다"),
            "unexpected error: {err}"
        );
    }

    // 존재하지 않는 프로필 `remote check --profile nope` → 프로필 미발견 거부.
    //
    // 이 메시지는 i18n 키를 거치므로 원문 리터럴로 매칭할 수 없다. 렌더 결과를
    // **같은 키로 만들어** 비교한다 — 이러면 i18n 초기화 여부와 무관하게(미초기화
    // 프로세스에서는 `t_fmt` 가 키를 그대로 돌려준다) 성립하고, 다른 키를 쓰도록
    // 바뀌면 실패한다.
    #[test]
    fn remote_check_unknown_profile_rejected() {
        let cli =
            Cli::try_parse_from(["tasty", "remote", "check", "--profile", "__nope__"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        let expected = tasty_i18n::t_fmt("cli.remote_profile.not_found_list_hint", "__nope__");
        assert!(
            err.to_string().contains(&expected),
            "unexpected error: {err}"
        );
    }

    // 로컬 loopback attach 는 debug 빌드 `tasty debug attach` 로만 파싱된다.
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

    // debug 로컬 attach 에는 ssh/profile 같은 원격 플래그가 없다.
    #[cfg(debug_assertions)]
    #[test]
    fn debug_attach_has_no_ssh() {
        assert!(Cli::try_parse_from(["tasty", "debug", "attach", "5", "--ssh", "h"]).is_err());
    }

    // remote attach 의 런타임 가드: 원격 대상(--ssh/--profile) 없이는 거부된다
    // (로컬 attach 로 폴백하지 않는다 — 로컬은 debug 빌드 전용).
    #[test]
    fn remote_attach_without_target_rejected() {
        let cli = Cli::try_parse_from(["tasty", "remote", "attach", "5"]).unwrap();
        let err = run::run_client(cli.command.unwrap(), None).unwrap_err();
        assert!(
            err.to_string()
                .contains(tasty_i18n::t("cli.dispatch.remote_attach_target_required")),
            "unexpected error: {err}"
        );
    }

    // remote attach 의 런타임 가드: surface 와 --workspace 는 상호배타.
    #[test]
    fn remote_attach_surface_workspace_exclusive() {
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

    // remote attach 의 런타임 가드: --ssh 와 --profile 은 상호배타.
    #[test]
    fn remote_attach_ssh_profile_exclusive() {
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

    /// `--id` 는 주인 창을 짚는다 — `--from` 과 달리 포커스에 안 걸린다.
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
        // kind 미지정 = kind 무관 해제. 호스트가 "필터 없음" 으로 읽어야 하므로
        // 문자열 기본값을 실어 보내지 않는다.
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
        // --sequence 는 JSON 문자열 → 배열 Value 로 파싱돼 전달.
        assert_eq!(r.params["sequence"][0]["method"], "notification.create");
    }

    /// `--method` 생략은 빈 배열이 아니라 null 이다 — 빈 배열은 서버가 거절하고, null 이면
    /// 서버 기본값(POST)이 선다(도움말 "Defaults to POST").
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
        // auth 미지정 → null (서버가 무인증으로 취급).
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
        // --body 는 JSON 문자열 → Value 로 파싱돼 치환 컨텍스트로 전달.
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
        // 안 준 필드는 null — 서버가 "패치 안 함" 으로 읽는다.
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
        // `.ok()` 로 떨어뜨리면 오타 하나가 "아무것도 안 고침" 이 되고 성공으로 나간다.
        // 원문을 그대로 실어 보내면 서버 스키마가 그 자리에서 거부한다.
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
        // body/headers/query 미지정 → null (서버가 부재로 취급).
        assert!(r.params["body"].is_null());
        assert!(r.params["headers"].is_null());
        assert!(r.params["query"].is_null());
    }
}
