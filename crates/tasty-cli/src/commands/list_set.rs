//! `tasty list` / `tasty set` subcommand 정의.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ListCommands {
    /// List workspaces
    Workspaces,
    /// List all windows
    Windows,
    /// Show tree view of workspaces, panes, and tabs
    Tree,
    /// List surfaces (terminals) across all workspaces
    Surfaces,
    /// List surface kinds currently registered by the host.
    ///
    /// Includes builtins and started plugins. Manifest declarations alone
    /// are not registration.
    SurfaceKinds,
    /// List panes across all workspaces
    Panes,
    /// List tabs in a pane
    Tabs {
        /// Pane ID (required)
        #[arg(long)]
        pane: u32,
    },
    /// Show system info
    Info,
    /// Show GPU resource counts (wgpu report + per-window renderer stats)
    GpuStats,
    /// Show IPC pressure and diagnostic statistics since this instance started.
    ///
    /// The blocks measure different requests and must not be subtracted:
    /// queue_before_gate includes requests later rejected by permissions;
    /// handler_after_gate covers handled requests; plugin_round_trip covers
    /// answered plugin calls; db covers successful database commits.
    /// A request may pass the gate and receive an answer before handler timing.
    ///
    /// connections counts TCP connections, including attach and mesh streams.
    /// live is current usage, limit is the enforced ceiling, and
    /// refused_saturated counts connections rejected before they made requests.
    /// Cumulative counts do not decrease; live and derived means can decrease.
    /// accept_waits equals accepted + refused_saturated. The
    /// accept_wait_bound_us_sum/max/mean fields bound time in the OS accept
    /// queue using elapsed time since that queue was last observed empty.
    /// They are bounds, not measured waits. The empty-queue sleep is 100 ms.
    ///
    /// queue_before_gate, handler_after_gate, and plugin_round_trip also have
    /// histograms. bounds_us contains the limits; counts has one extra overflow
    /// bucket and is not cumulative. Counts sum to the observation count.
    /// Quantiles are not calculated. db and connections have no histogram.
    /// queue_before_gate.waits is the denominator of wait_us_mean and the sum
    /// of wait_us_hist.counts. commands updates at the end of a dispatch round,
    /// so it can lag waits during the current round, including this query.
    ///
    /// db_pragmas compares requested and actual settings for memory_db and
    /// state_db. degraded reports a setting mismatch or, for memory_db, an
    /// in-memory fallback after file-open failure. init_failure records that
    /// failure or is null. The fallback database remains usable. state_db is
    /// null when unopened, including in headless instances.
    ///
    /// stream_push counts attach/mesh frames: frames_dropped records full-queue
    /// drops on live connections; clients_lagged_out records disconnected slow
    /// clients. backlog is current queued frames across live connections, and
    /// sink_capacity is the limit for one connection.
    ///
    /// queue_admission reports current bytes, commands, injected commands, peak
    /// bytes, and the limit_bytes/limit_injected_depth ceilings. refused_bytes
    /// and refused_depth count requests rejected before entering the queue;
    /// they appear in no other block. The block is null without an IPC server.
    /// queue_dispatch records rounds, command/time budget stops,
    /// expired_before_run, commands started, and current/peak in_flight requests.
    /// A started request remains in_flight until both the command and its
    /// response waiter release the shared lifecycle state.
    ///
    /// keyed_requests counts each keyed request once: executed, replayed,
    /// conflicted (nothing ran), discarded (answer was not retained), or
    /// in_flight (joined an existing request). This in_flight is a cumulative
    /// decision count, unlike the current count in queue_dispatch.
    ///
    /// slow_requests retains requests whose queue wait + host time + plugin
    /// waits meet threshold_us (100 ms), oldest first, up to capacity. Rows
    /// contain request_seq (neither JSON-RPC id nor trace id), method, caller,
    /// queue_wait_us, host_us, and plugin_hops. Each hop includes its plugin,
    /// host_request_id, wait, and outcome. admitted counts all retained rows;
    /// admitted minus rows shown counts evictions. The query excludes itself.
    /// A row's host outcome is ok/error and error_code is its JSON-RPC error,
    /// distinguishing expiry (-32067), refusal (-32001), and success. Both are
    /// null before an answer, for example while waiting for a plugin.
    ///
    /// gate_refusals.judged counts requests reaching permission checks, including
    /// Local and plugin callers. Gates run in order: permission_denied (-32001),
    /// cap_blocked (-32007), then throttled (-32010). A request is counted in at
    /// most one refusal slot. Permission refusal can mean missing permission,
    /// an unknown method, or a method forbidden to that caller; a grant only
    /// addresses the first case. Local callers without a token pass this gate.
    /// Invalid/expired/revoked session tokens are rejected before judged, while
    /// -32001 errors after the gate count as passed. Malformed idempotency keys
    /// are not counted here. These totals reset at restart; the per-bucket
    /// throttled_count in agent rate-limit-status persists and is a different count.
    ///
    /// An average with no observations is null, not zero.
    Pressure,
    /// List notifications
    Notifications,
    /// List registered timers and what is currently waking this instance
    Timers,
    /// List hooks
    Hooks {
        /// Filter by surface ID
        #[arg(long)]
        surface: Option<u32>,
    },
    /// List all global hooks
    GlobalHooks,
    /// Show the resolved global theme snapshot (colors, font sizes, ui scale).
    Theme,
    /// List the recently opened files of one surface kind.
    Recent {
        /// Surface kind to query (e.g. `markdown`). The host does not know the
        /// kind names — the caller supplies one.
        #[arg(long)]
        kind: String,
    },
    /// Show queue status (count + preview of pending messages)
    Queue {
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum SetCommands {
    /// Set a hook on a surface
    Hook {
        /// Surface ID to hook (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
        /// Event type: process-exit, bell, notification, output-match:PATTERN, idle-timeout:SECS
        #[arg(long)]
        event: String,
        /// Inline shell command to execute when the event fires (backward-compat;
        /// wrapped as an anonymous hook handler). Mutually exclusive with --handler.
        #[arg(long)]
        command: Option<String>,
        /// Shared hook-handler id to bind (e.g. host/my-handler). The handler must
        /// accept the hook trigger source. Mutually exclusive with --command.
        #[arg(long)]
        handler: Option<String>,
        /// Remove the hook after it fires once
        #[arg(long)]
        once: bool,
    },
    /// Set a read mark on a surface
    Mark {
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Update workspace name, subtitle, description, or category
    Workspace {
        /// Workspace ID (required)
        #[arg(long)]
        id: u32,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New subtitle
        #[arg(long)]
        subtitle: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
        /// Move this workspace to another category (name or id).
        #[arg(long)]
        category: Option<String>,
    },
    /// Set the working directory a remote surface reports to the host.
    Cwd {
        /// Target surface id.
        #[arg(long)]
        surface: u32,
        /// Directory path. Omit to clear it.
        #[arg(long)]
        path: Option<String>,
    },
    /// Set the URL of a webview-kind surface.
    Url {
        /// Target surface id.
        #[arg(long)]
        surface: u32,
        /// URL to load.
        #[arg(long)]
        url: String,
        /// Name the host shows for the page while the WebView is hidden. Omit to show the URL.
        #[arg(long)]
        label: Option<String>,
    },
    /// Set a global hook — fires on a schedule, not bound to any surface
    GlobalHook {
        /// Condition: interval:SECS, once:SECS
        #[arg(long)]
        condition: String,
        /// Shell command to execute when the condition fires
        #[arg(long)]
        command: String,
        /// Optional human-readable label
        #[arg(long)]
        label: Option<String>,
    },
}
