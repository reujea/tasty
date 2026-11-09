//! `tasty plugin ...` + extension + tool subcommand 정의.

use clap::Subcommand;

use super::passkey::PasskeyCommands;
use super::remote_profile::RemoteProfileCommands;

#[derive(Subcommand)]
pub enum PluginCommands {
    /// List installed plugins (id, version, enabled, running).
    List,
    /// Show full manifest, permissions, commands, and runtime state for a plugin.
    Show {
        /// Plugin id.
        id: String,
    },
    /// Install a plugin from a directory containing tasty-plugin.toml.
    ///
    /// Two more things happen, and neither is asked about: every permission the
    /// manifest declares is granted immediately, and the plugin is enabled and
    /// started unless a disabled mark for that id is already on file. Inspect
    /// `tasty-plugin.toml` first, or check afterwards with
    /// `plugin permissions <id>` and `plugin revoke <id> <permission>`.
    ///
    /// A plugin whose signature Tasty cannot check against a trusted key is
    /// unverified. It is not installed unless `--allow-unverified` is given, and
    /// the refusal explains why. The choice is remembered until the manifest
    /// changes or the plugin is removed.
    Install {
        /// Path to plugin directory (must contain tasty-plugin.toml).
        path: String,
        /// Install even if the plugin is unverified. It runs with your account's
        /// access; use this only if you trust where it came from.
        #[arg(long)]
        allow_unverified: bool,
    },
    /// Remove an installed plugin by id.
    ///
    /// The whole plugin directory is deleted, so anything the plugin kept inside
    /// it goes with it. For a built-in, removal is also recorded so the next boot
    /// does not put it back — bringing it back is
    /// `plugin upgrade-builtins --restore-removed <id>`, and it returns enabled.
    ///
    /// A running plugin is asked first whether it is busy; if it is, nothing is
    /// removed. Pass --ignore-busy to remove it anyway.
    Remove {
        /// Plugin id (e.g. com.example.explorer).
        id: String,
        /// Remove the plugin even if it says it is busy.
        #[arg(long = "ignore-busy")]
        ignore_busy: bool,
    },
    /// Re-sync all built-in plugins from the bundle.
    ///
    /// The manifest version picks the branch and content decides inside it: a
    /// higher bundle version rewrites the whole directory, the same version
    /// rewrites only what differs, and an installed version higher than the bundle
    /// is skipped without reading content — `--force` is for that last branch.
    /// Permissions newly declared by a built-in that changed are granted in the
    /// same call.
    UpgradeBuiltins {
        /// Overwrite even when installed version >= bundle version.
        #[arg(long)]
        force: bool,
        /// Restore specific builtins from `removed_builtins` so they become
        /// eligible for auto-install in the same call. Repeat for multiple ids.
        #[arg(long = "restore-removed", value_name = "ID")]
        restore_removed: Vec<String>,
        /// Restore ALL builtins from `removed_builtins`. Overrides
        /// `--restore-removed` when both are given.
        #[arg(long = "restore-removed-all")]
        restore_all: bool,
        /// Restart running builtin processes whose binary is replaced — graceful
        /// swap. Also unblocks Windows sharing violation on in-place overwrite.
        /// Default off (conservative — briefly closes affected plugin surfaces).
        #[arg(long = "restart-running")]
        restart_running: bool,
        /// With --restart-running, stop a plugin even if it says it is busy (for
        /// example an agent turn in a chat tab). Without this, a busy plugin
        /// makes the command refuse with the reason.
        #[arg(long = "ignore-busy")]
        ignore_busy: bool,
    },
    /// Enable a disabled plugin and start it.
    ///
    /// Waits until the plugin connects, fails to start or is cancelled, then
    /// exits with a non-zero status unless it connected. The reply carries the
    /// start generation and its final state. While waiting it does not take
    /// --response-timeout-ms; with --no-wait it sends the request once.
    Enable {
        /// Plugin id.
        id: String,
        /// Print the accepted start and return without waiting for the result.
        #[arg(long)]
        no_wait: bool,
    },
    /// Disable a plugin (graceful shutdown if running).
    ///
    /// The mark is written to disk, so it survives a restart and a later
    /// `plugin install` of the same id will not turn it back on. This is not an
    /// uninstall: the directory stays and the plugin keeps its command namespace,
    /// so `tasty <namespace> ...` answers `plugin ... is not running` rather than
    /// reporting an unknown method. What does go away is everything the plugin
    /// contributed — file detectors and handlers, hook handlers, completion
    /// strategies, settings pages — so a file it used to open now takes another
    /// route.
    ///
    /// A running plugin is asked first whether it is busy (for example an agent
    /// turn in a chat tab); if it is, the command refuses with the reason and
    /// nothing changes. Pass --ignore-busy to stop it anyway.
    Disable {
        /// Plugin id.
        id: String,
        /// Stop the plugin even if it says it is busy.
        #[arg(long = "ignore-busy")]
        ignore_busy: bool,
    },
    /// Print the contents of a plugin's log file.
    Logs {
        /// Plugin id.
        id: String,
        /// Tail and follow new output (Ctrl-C to stop).
        #[arg(long)]
        follow: bool,
    },
    /// Diagnose a plugin's manifest — list contributed detectors / handlers and
    /// flag rule kinds the current host does not understand.
    Doctor {
        /// Plugin id (e.g. com.example.foo).
        id: String,
    },
    /// Show a plugin's manifest permissions and currently granted set.
    Permissions {
        /// Plugin id.
        id: String,
    },
    /// Grant a permission to a plugin (must be declared in its manifest).
    Grant {
        /// Plugin id.
        id: String,
        /// Permission token (e.g. fs.read, surface.write).
        permission: String,
    },
    /// Revoke a previously-granted permission from a plugin.
    Revoke {
        /// Plugin id.
        id: String,
        /// Permission token.
        permission: String,
    },
    /// Grant a temporary permission to an active agent session.
    /// Permission lives until TTL expiry or explicit revoke; base permissions
    /// (issued at session.issue) are unaffected.
    GrantAgentPermission {
        /// Agent id (e.g. `claude:child-1`).
        #[arg(long)]
        agent: String,
        /// Permission token (e.g. fs.write, surface.write).
        #[arg(long)]
        permission: String,
        /// Time-to-live in seconds. Omit for indefinite (until revoke).
        #[arg(long)]
        ttl: Option<u64>,
    },
    /// Revoke a previously-granted temporary permission from an agent session.
    /// Does not affect base permissions assigned at issue time.
    RevokeAgentPermission {
        /// Agent id.
        #[arg(long)]
        agent: String,
        /// Permission token.
        #[arg(long)]
        permission: String,
    },
    /// List base + temporary permissions for active agent sessions.
    ListAgentPermissions {
        /// Filter to a specific agent. Omit to list all active sessions.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Manually publish a capability elevation approval. Useful for operators
    /// pre-granting a permission before the agent's first call. The popup is
    /// the same one shown by automatic elevation on permission_denied.
    RequestPermission {
        /// Agent id to grant on approval.
        #[arg(long)]
        agent: String,
        /// Permission token (e.g. fs.write).
        #[arg(long)]
        permission: String,
        /// Reason shown to the user in the popup body.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Inspect plugin extensions (extends-blocks).
    Extension {
        #[command(subcommand)]
        command: ExtensionCommands,
    },
    /// Query the IPC audit log. Returns filtered records with allow/deny decisions.
    AuditQuery {
        /// Filter by caller kind: local | internal | plugin | agent.
        #[arg(long)]
        caller_kind: Option<String>,
        /// Filter by caller id (plugin id or agent id).
        #[arg(long)]
        caller_id: Option<String>,
        /// Filter to methods starting with this prefix (e.g. `surface.`).
        #[arg(long)]
        method_prefix: Option<String>,
        /// Filter by decision: allow | deny.
        #[arg(long)]
        decision: Option<String>,
        /// Lower bound on timestamp (unix ms, inclusive).
        #[arg(long)]
        since_ms: Option<u64>,
        /// Upper bound on timestamp (unix ms, exclusive).
        #[arg(long)]
        until_ms: Option<u64>,
        /// Cap on number of records returned.
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Aggregate audit records into totals + top callers/methods.
    AuditSummary {
        /// Filter by caller kind: local | internal | plugin | agent.
        #[arg(long)]
        caller_kind: Option<String>,
        /// Filter by caller id.
        #[arg(long)]
        caller_id: Option<String>,
        /// Filter to methods starting with this prefix.
        #[arg(long)]
        method_prefix: Option<String>,
        /// Filter by decision: allow | deny.
        #[arg(long)]
        decision: Option<String>,
        /// Lower bound on timestamp (unix ms, inclusive).
        #[arg(long)]
        since_ms: Option<u64>,
        /// Upper bound on timestamp (unix ms, exclusive).
        #[arg(long)]
        until_ms: Option<u64>,
        /// Top-N cap for by_caller / by_method lists. Default: 10.
        #[arg(long)]
        top_n: Option<u64>,
    },
    /// Delete audit records older than `before_ms`, or all if omitted.
    AuditClear {
        /// Delete records with ts < before_ms. Omit to clear everything.
        #[arg(long)]
        before_ms: Option<u64>,
    },
    /// Tail audit records as they arrive. Polls every `interval_ms` ms until
    /// Ctrl-C. Filters are the same as `audit-query`.
    AuditFollow {
        /// Filter by caller kind: local | internal | plugin | agent.
        #[arg(long)]
        caller_kind: Option<String>,
        /// Filter by caller id.
        #[arg(long)]
        caller_id: Option<String>,
        /// Filter to methods starting with this prefix.
        #[arg(long)]
        method_prefix: Option<String>,
        /// Filter by decision: allow | deny.
        #[arg(long)]
        decision: Option<String>,
        /// Per-poll cap on record batch size.
        #[arg(long, default_value_t = 100)]
        batch: u64,
        /// Polling interval in milliseconds.
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
    },
}

#[derive(Subcommand)]
pub enum ExtensionCommands {
    /// List all extensions and their current state.
    List,
}

#[derive(Subcommand)]
// reason: clap 파싱 1회용 enum — Box 화는 derive(Subcommand) 와 충돌하고 런타임 이득 없음.
#[allow(clippy::large_enum_variant)]
pub enum ToolCommands {
    /// Open an interactive ssh session using a saved ssh profile (identity and
    /// port are injected automatically). Local, no IPC.
    Ssh {
        /// Name of the ssh profile to connect with.
        profile: String,
        /// Command to run once on the remote host (interactive shell if omitted),
        /// e.g. `tasty tool ssh gb10 --command hostname`.
        #[arg(long = "command", num_args = 1.., allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Manage remote connection profiles (ssh and tasty-attach kinds). Local
    /// file, no IPC.
    RemoteProfile {
        #[command(subcommand)]
        command: RemoteProfileCommands,
    },
    /// Attach to a remote surface/workspace using a tasty-attach profile.
    /// `--list` only lists the profiles. Local.
    Attach {
        /// Name of the tasty-attach profile to attach with (omit it together
        /// with `--list` to only list profiles).
        name: Option<String>,
        /// Remote surface id to attach to.
        surface: Option<u32>,
        /// Remote workspace id to attach to (mutually exclusive with surface).
        #[arg(long)]
        workspace: Option<u32>,
        /// Input to send once right after attaching, non-interactively (escapes
        /// decoded).
        #[arg(long)]
        send: Option<String>,
        /// Surface id that receives --send in workspace mode.
        #[arg(long)]
        send_to: Option<u32>,
        /// Mirror-dump collection time in ms (built-in default if omitted).
        #[arg(long)]
        dump_after: Option<u64>,
        /// Raw stdin/stdout bridge (surface only).
        #[arg(long)]
        raw: bool,
        /// Disable automatic reconnection when the connection drops.
        #[arg(long)]
        no_reconnect: bool,
        /// Only list tasty-attach profiles and exit (no attach).
        #[arg(long)]
        list: bool,
    },
    /// Passkeys (`~/.tasty/passkeys.toml`): the credential store that profiles
    /// reference by name. list/show never reveal values. Local file, no IPC.
    Passkey {
        #[command(subcommand)]
        command: PasskeyCommands,
    },
}
