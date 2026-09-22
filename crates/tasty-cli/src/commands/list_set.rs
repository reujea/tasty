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
    /// List the surface kinds this instance has actually registered.
    ///
    /// This is the runtime fact, not a manifest declaration: a kind shows up here
    /// only if the host registered it, so `--type <kind>` works for exactly the
    /// kinds listed. Host builtins and plugin-provided kinds both appear.
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
    /// Show request pressure since this instance started: where the time went
    /// while answering requests.
    ///
    /// The answer is one block per population, and the populations differ on
    /// purpose. `queue_before_gate` is measured before the permission gate, so
    /// it also counts requests that were later rejected. `handler_after_gate`
    /// is measured after it, so it counts only requests that ran.
    /// `plugin_round_trip` is how long this instance waited for a plugin to
    /// answer, counting only the requests that were answered at all. `db` is
    /// how long writes took to settle on disk, counting only commits that
    /// succeeded. Do not subtract one block from another: a request can pass
    /// the gate and still be answered before the handler block sees it.
    ///
    /// `connections` is the fifth block and it is not time, it is seats: every
    /// TCP connection attached to this port, including attach and mesh streams
    /// that never send a request. Of the counts and current values here only
    /// `live` goes down (the accept wait mean below is derived, so it can fall
    /// too), `limit` is the ceiling the server enforces and travels with the
    /// values so the two can be read together, and `refused_saturated` counts
    /// connections turned away at that ceiling — before they became requests,
    /// so they appear in none of the four blocks above.
    ///
    /// `connections` also carries one duration, and it is a bound rather than a
    /// measurement: for every connection the accept loop took (`accept_waits`,
    /// which equals `accepted` plus `refused_saturated`), the time since the
    /// accept queue was last seen empty, which the connection's wait in the
    /// operating system's queue can never exceed (`accept_wait_bound_us_sum`,
    /// `accept_wait_bound_us_max`, `accept_wait_bound_us_mean`). The loop
    /// sleeps 100 ms when the queue is empty, so a client that opens a new
    /// connection for every call can lose up to that much before its request is
    /// read, and this is the only block where that time shows.
    ///
    /// Three of those four — `queue_before_gate`, `handler_after_gate` and
    /// `plugin_round_trip` — also carry a `*_hist` with the distribution of
    /// the same observations, because an average and a maximum cannot tell
    /// "everything is slightly slow" from "most are fast and a few are not".
    /// `bounds_us` and `counts` travel together; `counts` is one entry longer
    /// and is not cumulative, so the entries sum to the observation count and
    /// the last one means only that the top bound was passed. Quantiles are
    /// not computed here. `db` is a duration too but has no distribution, and
    /// `connections` has none because its seats are not durations and its one
    /// duration is only a bound.
    ///
    /// `db_pragmas` is the sixth block and it is not a running total at all:
    /// for each SQLite database (`memory_db`, `state_db`) it shows the
    /// connection settings that were requested when the database was opened
    /// next to the values read back from it, because a request such as
    /// journal_mode=WAL can be silently refused. `degraded` is true when any
    /// setting did not take, and for `memory_db` also when the file could not
    /// be opened and the instance runs on an in-memory fallback, in which case
    /// `init_failure` names the cause (it is null otherwise); the database is
    /// still in use. `state_db` is null
    /// when that database is not open in this process, which is always the
    /// case for a headless instance.
    ///
    /// `stream_push` is the seventh block and it counts frames this instance
    /// pushed rather than requests it answered: frames sent to attach and mesh
    /// stream connections. `frames_dropped` counts frames thrown away because a
    /// connection's queue was full while the connection stayed up, and
    /// `clients_lagged_out` counts connections cut for falling too far behind;
    /// both only go up. `backlog` is how many frames are queued right now and
    /// not yet written, summed over the live connections, and `sink_capacity`
    /// is the queue ceiling of one connection to read it against.
    ///
    /// `queue_admission` and `queue_dispatch` are the two ends of the command
    /// queue. `queue_admission` is what is in the queue right now (bytes,
    /// commands, and how many of those this instance injected itself) with the
    /// peak byte count, the requests turned away at the byte ceiling
    /// (`refused_bytes`) or the injected depth ceiling (`refused_depth`), and
    /// the two ceilings (`limit_bytes`, `limit_injected_depth`) to read them
    /// against; a turned-away request never entered the queue, so it appears
    /// in no other block. It is null when this process has no IPC server.
    /// `queue_dispatch` is what was taken out: rounds and how many stopped at
    /// the command or time budget, commands whose deadline passed while they
    /// waited (`expired_before_run`), commands started, and `in_flight`, the
    /// requests running now whose caller is still waiting, with its peak.
    ///
    /// `keyed_requests` counts only requests that carried an idempotency key,
    /// one slot per decision: `executed` (a new key, it ran), `replayed` (the
    /// same request again, the kept answer was returned), `conflicted` (a
    /// different request under the same key, nothing ran), `discarded` (it
    /// ran but its answer was thrown away) and `in_flight` (the same request
    /// was still running and this one joined it). Each request is counted
    /// once. This `in_flight` is not the one in `queue_dispatch`.
    ///
    /// `slow_requests` is the eleventh block and it lists single slow requests
    /// instead of totals: each request whose queue wait, host time and plugin
    /// waits add up to `threshold_us` (100 ms) or more gets one row, oldest
    /// first, up to `capacity` rows. A row carries `request_seq`, a number this
    /// instance gives every request (not the JSON-RPC id and not an event trace
    /// id), the `method`, the `caller`, `queue_wait_us`, `host_us` and, when
    /// the request was forwarded to a plugin, `plugin_hops` with the plugin,
    /// the `host_request_id` that plugin received as its request id, the wait
    /// and the outcome. `admitted` counts every row ever kept, so the rows
    /// pushed out are `admitted` minus the rows shown. A query for this answer
    /// is never kept itself.
    ///
    /// The `host` part of a slow row also carries the answer the caller
    /// actually got: `outcome` is `ok` or `error`, the same words a plugin hop
    /// uses, and `error_code` is the JSON-RPC error code when it is an error,
    /// so a request that expired in the queue before it ran (-32067), a refused
    /// one (-32001) and a normal answer can be told apart from the ring alone.
    /// Both are null while no answer has gone out yet, for example while a
    /// forwarded request is still waiting for its plugin.
    ///
    /// `queue_before_gate.waits` counts the commands whose queue wait was
    /// recorded, at the moment each was taken out; it is the denominator of
    /// `wait_us_mean` and equals the sum of `wait_us_hist.counts`. `commands`
    /// rises only when a round of taking commands out ends, so within one
    /// answer it can trail `waits` by the round still running, which includes
    /// the query itself.
    ///
    /// `gate_refusals` is the twelfth block and counts requests the admission
    /// gate **turned away**: `judged` is every request that reached the gate
    /// (requests from this CLI and calls plugins make into this instance
    /// included), and each gate has its own slot — `permission_denied`
    /// (permission, -32001), `cap_blocked` (telemetry cap, -32007) and
    /// `throttled` (rate limit, -32010). The gates run in that order and a
    /// request turned away by one never reaches the next, so a request is
    /// counted in at most one slot. The three call for different fixes: ask for
    /// the permission, lift the cap, or wait. `permission_denied` counts only
    /// the -32001 answers the permission gate gives: every refusal there is
    /// -32001, but not every -32001 is one. Besides a missing permission, that
    /// gate also turns away calls a plugin or agent makes to a method that does
    /// not exist or to a method it is not allowed to call. Asking for a
    /// permission does not fix those two; calling a different method does.
    /// Calls from this CLI without a session token are never turned away at
    /// this gate, so they do not land here even when the method does not exist.
    /// Two kinds of -32001 never land here either: a rejected session token
    /// (malformed, unknown, expired or revoked, such as a stale
    /// TASTY_SESSION_TOKEN), which is answered before the gate so `judged` does
    /// not count it, and a -32001 returned after the request passed the gate
    /// (such as a refused grant), which counts as passed. The numbers are
    /// totals since this instance started and go back to 0 on restart. The
    /// `throttled_count` reported by `tasty agent rate-limit-status` is a
    /// per-bucket total that survives restarts, so it is not the same number.
    /// A request turned away for a malformed idempotency key is not counted
    /// here.
    ///
    /// An average with nothing behind it comes back as null, not zero.
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
        /// Surface ID (default: focused)
        #[arg(long)]
        surface: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum SetCommands {
    /// Set a hook on a surface
    Hook {
        /// Surface ID to hook (default: focused)
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
        /// Surface ID (default: focused terminal)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Update workspace name, subtitle, description, or SSH attach mapping
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
        /// Map this workspace to a saved SSH profile (a remote machine).
        #[arg(long)]
        ssh_profile: Option<String>,
        /// Map to a one-off inline SSH target, e.g. --ssh user@host.
        #[arg(long)]
        ssh: Option<String>,
        /// Workspace id on the mapped remote tasty to attach to.
        #[arg(long)]
        remote_workspace: Option<u32>,
        /// Clear the existing SSH attach mapping.
        #[arg(long)]
        clear_mapping: bool,
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
