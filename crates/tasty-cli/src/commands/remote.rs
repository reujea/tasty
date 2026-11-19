//! 원격 SSH attach 명령. 터널·스트림·재연결은 local::attach에서 처리한다.
//! 로컬 검증용 attach는 debug 빌드의 debug attach 명령으로 분리한다.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// Attach to a remote surface/workspace over SSH (one-shot).
    ///
    /// Opens an `ssh -L` tunnel and runs the attach session through it; the
    /// remote target comes from `--ssh` or `--profile`.
    Attach {
        /// Target surface id (given explicitly, never inferred from focus).
        /// Mutually exclusive with `--workspace`.
        surface: Option<u32>,
        /// Target workspace id; mirrors every terminal in it as a tree.
        /// Mutually exclusive with the `surface` positional. Non-terminal
        /// surfaces are shown as placeholders.
        #[arg(long)]
        workspace: Option<u32>,
        /// Mirror-dump mode: collect output for N ms after attaching, print the
        /// mirrored screen to stdout, then exit (automated verification without
        /// a GUI). In workspace mode each surface is printed as its own section.
        #[arg(long)]
        dump_after: Option<u64>,
        /// Input to send once right after attaching (escapes decoded: \n \r \t
        /// \xNN). For non-interactive verification.
        #[arg(long)]
        send: Option<String>,
        /// Remote surface id that receives the `--send` input in workspace mode.
        #[arg(long)]
        send_to: Option<u32>,
        /// Raw bridge mode: stdin/stdout passthrough (detach with Ctrl+\).
        #[arg(long)]
        raw: bool,
        /// Remote SSH target, e.g. --ssh user@host or --ssh gx10. Mutually
        /// exclusive with `--profile`.
        #[arg(long)]
        ssh: Option<String>,
        /// Attach using a saved tasty-attach profile name. The profile is
        /// resolved from `~/.tasty/remote-profiles.toml` to supply
        /// user/port/identity/extra options. Mutually exclusive with `--ssh`.
        /// The profile's values replace `--remote-tasty` and
        /// `--remote-port-mode`.
        #[arg(long)]
        profile: Option<String>,
        /// Path to the tasty binary on the remote host, used by the subcommand
        /// step of port discovery (`ssh host <path> port`). Defaults to "tasty"
        /// (assumed to be on the remote PATH).
        #[arg(long, default_value = "tasty")]
        remote_tasty: String,
        /// Remote port discovery mode: auto (default) | subcommand | file-unix |
        /// file-windows.
        #[arg(long, default_value = "auto")]
        remote_port_mode: String,
        /// Disable automatic reconnection (by default the session reconnects
        /// with backoff when SSH drops).
        #[arg(long)]
        no_reconnect: bool,
        /// Ask the local GUI to become the client and rebuild the remote
        /// workspace as a mirror, without moving focus. With `--ssh` or
        /// `--profile` the GUI opens the tunnel itself (`remote.attach` IPC) and
        /// the command waits for the connection result. With `--target-port` it
        /// connects to a port you already forwarded (`attach.into_gui` IPC) and
        /// `--ssh`/`--profile` are not used. Requires `--workspace`, or
        /// `--new-workspace` with `--ssh`/`--profile`.
        #[arg(long)]
        into_gui: bool,
        /// Loopback port of the remote tasty server for `--into-gui` (the port
        /// the GUI connects to).
        #[arg(long)]
        target_port: Option<u16>,
        /// With `--into-gui`, create a workspace on the remote and mirror it.
        /// The reply carries the new remote workspace id. Mutually exclusive
        /// with `--workspace` and `--target-port`.
        #[arg(long, requires = "into_gui", conflicts_with_all = ["surface", "workspace", "target_port"])]
        new_workspace: bool,
        /// Name of the workspace made by `--new-workspace` (remote default if
        /// omitted).
        #[arg(long, requires = "new_workspace", conflicts_with_all = ["surface", "workspace", "target_port"])]
        name: Option<String>,
        /// Working directory of the workspace made by `--new-workspace`, as a
        /// path on the remote filesystem.
        #[arg(long, requires = "new_workspace", conflicts_with_all = ["surface", "workspace", "target_port"])]
        cwd: Option<String>,
    },
    /// Check whether a remote tasty instance is alive over SSH.
    ///
    /// Find the remote port using `tasty port` or the port file, open an `ssh -L`
    /// tunnel, and send a `system.info` request. A successful response confirms that
    /// the instance is reachable. A connection failure, timeout, invalid response,
    /// or error response means the check failed; it does not by itself show that
    /// the server stopped. Argument names match `remote attach`.
    Check {
        /// Remote SSH target, e.g. --ssh user@host or --ssh gx10. Mutually
        /// exclusive with `--profile`.
        #[arg(long)]
        ssh: Option<String>,
        /// Check using a saved tasty-attach profile name. The profile is
        /// resolved from `~/.tasty/remote-profiles.toml` to supply
        /// user/port/identity/extra options. Mutually exclusive with `--ssh`.
        /// The profile's values replace `--remote-tasty` and
        /// `--remote-port-mode`.
        #[arg(long)]
        profile: Option<String>,
        /// Path to the tasty binary on the remote host, used by the subcommand
        /// step of port discovery (`ssh host <path> port`). Defaults to "tasty"
        /// (assumed to be on the remote PATH).
        #[arg(long, default_value = "tasty")]
        remote_tasty: String,
        /// Remote port discovery mode: auto (default) | subcommand | file-unix |
        /// file-windows.
        #[arg(long, default_value = "auto")]
        remote_port_mode: String,
    },
    /// List the workspaces of a remote tasty instance over SSH.
    ///
    /// Connects to the ssh target or attach profile, fetches the remote
    /// `workspace.list` and `attach.list`, and merges them: each workspace is
    /// reported with its id, name, pane count, busy flag, and whether another
    /// client has it attached. Use this to discover the workspace id that
    /// `remote attach --workspace` expects. Read-only; the local user's state
    /// (focus etc.) is untouched. `--ssh 127.0.0.1:<port>` connects directly
    /// over loopback without a tunnel (local end-to-end checks).
    ///
    /// The local IPC method `remote.workspaces` exposes the same capability.
    Workspaces {
        /// Remote SSH target, e.g. --ssh user@host, --ssh gx10, or
        /// --ssh 127.0.0.1:45123. Mutually exclusive with `--profile`.
        #[arg(long)]
        ssh: Option<String>,
        /// List using a saved tasty-attach profile name. Mutually exclusive
        /// with `--ssh`. The profile's values replace `--remote-tasty` and
        /// `--remote-port-mode`.
        #[arg(long)]
        profile: Option<String>,
        /// Path to the tasty binary on the remote host (used by the subcommand
        /// step of port discovery). Defaults to "tasty".
        #[arg(long, default_value = "tasty")]
        remote_tasty: String,
        /// Remote port discovery mode: auto (default) | subcommand | file-unix |
        /// file-windows.
        #[arg(long, default_value = "auto")]
        remote_port_mode: String,
        /// Print a JSON array instead of human-readable text (for scripts and
        /// UI consumers).
        #[arg(long)]
        json: bool,
    },
    /// Create a new workspace on a remote tasty instance over SSH.
    ///
    /// The write-side counterpart of `remote workspaces`. Pass the printed id
    /// to `tasty remote attach --workspace <id>` to create a workspace remotely
    /// and attach to it in one go. The remote's active workspace does not
    /// change. To also mirror it in the local GUI, use
    /// `tasty remote attach --into-gui --new-workspace`.
    NewWorkspace {
        /// Remote SSH target, e.g. --ssh user@host, --ssh gx10, or
        /// --ssh 127.0.0.1:45123. Mutually exclusive with `--profile`.
        #[arg(long)]
        ssh: Option<String>,
        /// Create using a saved tasty-attach profile name. Mutually exclusive
        /// with `--ssh`. The profile's values replace `--remote-tasty` and
        /// `--remote-port-mode`.
        #[arg(long)]
        profile: Option<String>,
        /// Path to the tasty binary on the remote host (used by the subcommand
        /// step of port discovery). Defaults to "tasty".
        #[arg(long, default_value = "tasty")]
        remote_tasty: String,
        /// Remote port discovery mode: auto (default) | subcommand | file-unix |
        /// file-windows.
        #[arg(long, default_value = "auto")]
        remote_port_mode: String,
        /// Name of the new workspace (remote default if omitted).
        #[arg(long)]
        name: Option<String>,
        /// Working directory of the new workspace, as a path on the remote
        /// filesystem. The remote checks that it exists and rejects the request
        /// with `cwd does not exist` otherwise.
        #[arg(long)]
        cwd: Option<String>,
        /// Print a JSON object instead of human-readable text (for scripts and
        /// agents).
        #[arg(long)]
        json: bool,
    },
    /// List recent attach attempts refused because they targeted this instance.
    ///
    /// Refusals decided after the endpoint is resolved are listed here: a
    /// saved remote workspace row whose attach profile points at this
    /// instance, and a `remote.attach` to an existing workspace that reaches
    /// this instance over SSH by host name or LAN IP (that reply is `attaching`
    /// and the refusal follows asynchronously). A loopback target on the own
    /// port is refused immediately with an error instead. Each entry carries
    /// the attempt number (the `attempt` in a `remote.attach` reply), the local
    /// workspace of the saved row (null for a manual request), the remote
    /// workspace id, the port, and whether it was decided over SSH. Up to 32 of
    /// the newest refusals are kept until the instance exits. Other connection
    /// failures are not listed. Read-only.
    Refusals,
    /// Detach a mirror from the local GUI and leave the remote workspace running.
    ///
    /// Does what the mirror card's "Disconnect Workspace" menu item does: the
    /// local mirror workspace, its attach session, and its reconnect attempts
    /// are cleared, while the remote workspace and its processes keep running.
    /// Attach again later with `tasty remote attach --into-gui --workspace
    /// <remote id>`. A saved remote workspace row stays in the list; connect it
    /// again with `tasty remote connect --workspace <local id>`. The reply
    /// carries that remote id. A saved row that has no connection or attempt
    /// to cancel (already disconnected, failed, or not connected yet) still
    /// succeeds with `disconnected: false` and `already_disconnected: true`,
    /// and a notice goes to stderr. Fails with `data.reason`
    /// `workspace_not_found` or `not_a_mirror` if the workspace does not exist
    /// or is neither a saved row nor a mirror with an attach session; an agent
    /// mirror is gone once disconnected. Use `tasty close surface` only when
    /// the remote workspace should close too.
    Disconnect {
        /// Local id of the mirror workspace (the `[mirror]` entry in
        /// `tasty list workspaces`), not the remote id.
        #[arg(long)]
        workspace: u32,
    },
    /// Connect a saved remote workspace row, including one the user disconnected.
    ///
    /// Clears the user's disconnect on that row and connects it without moving
    /// focus. Prints the accepted reply, then waits for the final result and
    /// prints it as a second line. An already connected row succeeds at once,
    /// and a row that is connecting joins that attempt. Exits with 1 if the
    /// result is not a success or does not arrive within 90 seconds. Fails at
    /// once if the workspace is not a saved remote workspace row, if its remote
    /// workspace is gone, or if its window is closed.
    Connect {
        /// Local id of the saved remote workspace row (its entry in
        /// `tasty list workspaces`), not the remote id.
        #[arg(long)]
        workspace: u32,
    },
    /// Force-detach remote clients that occupy this instance.
    ///
    /// Does what the occupancy border's "Force detach" button and the sidebar's
    /// force-detach confirmation do: the occupancy is released and the server tries
    /// to queue a force-detached notice on the holder's connection. A holder that
    /// handles the notice ends its mirror without reconnecting, and a saved remote
    /// workspace row on that client stays disconnected, even after that client
    /// restarts, until someone connects it explicitly. `--workspace` calls
    /// `attach.force_detach_workspace` and `--surface` calls `attach.force_detach`;
    /// each prints that reply, with `force_detached: false` when nothing held the
    /// target. A surface that belongs to an occupied workspace releases that whole
    /// workspace, and the reply names it in `workspace_id`. `notified: true` means
    /// the notice was put in the holder connection's send queue; it is not a
    /// delivery guarantee, because a failed or timed out earlier write ends the
    /// connection before the notice is written. `notified: false` has three cases:
    /// nothing held the target, so there is no connection to close; the occupancy
    /// was released but its connection was already gone; or the send queue was
    /// full, so the server closes that connection without the notice once any write
    /// in progress ends or reaches its write timeout. A holder that did not get the
    /// notice may reconnect as after any lost connection. `--all` reads
    /// `attach.list`, detaches every occupied workspace, then every surface still
    /// occupied, and prints `{"workspaces": [...], "surfaces": [...]}` with one
    /// reply per call. A target that fails keeps its id and an `error` in its item,
    /// the others are still detached, and the command exits with 1. If the list
    /// cannot be read again after the workspaces, the error goes in `list_error`,
    /// the results so far are printed, and the command exits with 1. The IPC method
    /// `attach.list` lists this instance's occupancies, including surfaces held on
    /// their own; to leave a mirror on this side, use `tasty remote disconnect`.
    #[command(group(
        clap::ArgGroup::new("detach_target")
            .required(true)
            .args(["workspace", "surface", "all"])
    ))]
    Detach {
        /// Id of the occupied workspace on this instance.
        #[arg(long)]
        workspace: Option<u32>,
        /// Id of the occupied surface on this instance.
        #[arg(long)]
        surface: Option<u32>,
        /// Detach every occupancy listed by `attach.list`.
        #[arg(long)]
        all: bool,
    },
}

impl RemoteCommands {
    // GUI가 터널을 직접 여는 mirror 요청인지. `--target-port`가 있으면 호출자가 연 터널을 쓰는
    // `attach.into_gui`이고, 대상도 포트도 없으면 그 메서드의 인자 오류로 답한다.
    pub(crate) fn gui_opens_tunnel(&self) -> bool {
        matches!(
            self,
            Self::Attach {
                into_gui: true,
                target_port: None,
                ssh,
                profile,
                new_workspace,
                ..
            } if ssh.is_some() || profile.is_some() || *new_workspace
        )
    }
}
