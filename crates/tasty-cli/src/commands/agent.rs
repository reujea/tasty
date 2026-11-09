//! `tasty agent ...` subcommand 정의.

use clap::{Subcommand, ValueEnum};

/// `agent task-run` action.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TaskRunAction {
    Start,
    Stop,
    Status,
}

impl TaskRunAction {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskRunAction::Start => "start",
            TaskRunAction::Stop => "stop",
            TaskRunAction::Status => "status",
        }
    }
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Create a task in a workspace without starting it.
    ///
    /// Start the workspace runner explicitly with `task-run --action start`;
    /// host startup does not start it for you.
    /// Without a runner, ready tasks remain queued and task-await can time out.
    TaskCreate {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Display name.
        #[arg(long)]
        name: String,
        /// TaskCommand as JSON (e.g. `{"kind":"run","command":["cargo","build"]}`).
        /// `workspace_id` is auto-filled from `--workspace` if omitted; if present
        /// and different, `--workspace` wins and a warning is returned.
        /// Accepts inline JSON or `@path/to/file.json`.
        #[arg(long)]
        command: String,
        /// Comma-separated dependency task IDs.
        #[arg(long, value_delimiter = ',')]
        depends_on: Vec<String>,
        /// On-failure policy: abort | continue_downstream | fallback:<task_id> (default: abort).
        /// Which task you set this on matters: abort/continue_downstream must be
        /// set on the dependent (downstream) task; fallback must be set on the
        /// task that may itself fail (upstream) — setting fallback on a
        /// downstream task has no effect on skip cascades from a failed dependency.
        #[arg(long, default_value = "abort")]
        on_failure: String,
        /// Metadata JSON (free-form, attached to the task).
        #[arg(long)]
        metadata: Option<String>,
        /// Shorthand for `metadata.semaphore.name` — caps how many tasks tagged
        /// with the same semaphore name run concurrently (the rest wait as
        /// `ready` until a permit frees up). Merges into `--metadata` if both
        /// are given; conflicts if `--metadata` already sets `semaphore`.
        #[arg(long)]
        concurrency_limit: Option<String>,
        /// Keep this fallback candidate waiting until a later task-create links
        /// it with --on-failure fallback:<this-id>. This prevents the runner from
        /// executing it between the two calls. It remains waiting if no main task
        /// ever references it.
        #[arg(long, default_value_t = false)]
        reserved_for_fallback: bool,
    },
    /// Submit a typed (contract_version 2) task graph in one call.
    ///
    /// The whole graph is validated first: ids, references, input bindings,
    /// types, mappings, fallbacks and cycles. A graph that fails validation
    /// stores nothing; the error names the task and a JSON pointer into the
    /// submitted graph. A valid graph is stored and activated together, so no
    /// task of it runs before every task exists. Run and agent commands without
    /// a `workspace_id`, including inline fallback commands, get `--workspace`.
    /// Other command kinds have no such field and are sent unchanged.
    TaskGraphSubmit {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Graph JSON (`{"contract_version":2,"types":{..},"tasks":[..]}`).
        /// Accepts inline JSON or `@path/to/file.json`.
        #[arg(long)]
        graph: String,
        /// Validate only. Nothing is stored or run.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// List tasks in a workspace.
    TaskList {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Filter by state (comma-separated: waiting,ready,running,succeeded,failed,cancelled,skipped,unknown).
        /// With several states, every task matching any of them is returned
        /// (`--state waiting,ready,running` = tasks that have not finished yet).
        #[arg(long = "state", value_delimiter = ',')]
        state: Vec<String>,
        /// Print the IPC response as JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Fetch a single task.
    TaskGet {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id.
        #[arg(long)]
        id: String,
        /// Print the IPC response as JSON instead of text, including
        /// `typed_result.raw` and `input_snapshot`.
        #[arg(long)]
        json: bool,
    },
    /// Wait for a task to reach a terminal state (blocking). Check the reply's `outcome`: exit status 0 does not mean the task succeeded.
    ///
    /// Without `--timeout-ms` it waits up to 10 minutes (provisional default);
    /// `--timeout-ms 0` waits indefinitely. The reply's `outcome` is `terminal`
    /// (with `state`, for example succeeded or failed), `timed_out`, or `not_found`.
    /// All three exit with status 0, so read `outcome` first and then `state`.
    /// The root `--response-timeout-ms` only bounds how long the CLI waits for the
    /// server's reply; it is not the task's timeout.
    TaskAwait {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id to wait on.
        #[arg(long)]
        id: String,
        /// Give up waiting after this many milliseconds. `0` = wait indefinitely.
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Cancel a task. Downstream is cascaded according to on_failure.
    TaskCancel {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id to cancel.
        #[arg(long)]
        id: String,
    },
    /// Retry a Failed/Cancelled/Skipped/Unknown task.
    TaskRetry {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id to retry.
        #[arg(long)]
        id: String,
        /// Also reset downstream Skipped/Failed tasks back to Waiting. Refused for typed
        /// (contract_version 2) tasks.
        #[arg(long, default_value_t = false)]
        reset_downstream: bool,
    },
    /// Output the task DAG as a parent ID table, JSON or Graphviz dot.
    TaskGraph {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Format: table | json | dot. Default: table (parent ID table). The table lists every edge source as a parent; use json or dot to see edge kinds.
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// List every registered DAG. A DAG is derived, not stored: tasks tagged
    /// with the same `metadata.dag` string form one explicit group, and the
    /// rest are grouped by weak connectivity (`depends_on` / `Fallback.task` /
    /// `Reduce.inputs` / `metadata.fallback_of` / v2 `bindings` / v2
    /// `transitions`). Omit `--workspace` to scan every live workspace.
    DagList {
        /// Restrict to one workspace. Omitted = every live workspace.
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: Option<u32>,
        /// Also include each DAG's task id list (`task_ids`).
        #[arg(long, default_value_t = false)]
        include_tasks: bool,
    },
    /// Output one DAG (as listed by `dag-list`) as a parent ID table, JSON or Graphviz dot —
    /// the same node/edge shape as `task-graph`, restricted to that DAG.
    DagGet {
        /// DAG id from `dag-list` (`d:<metadata.dag>` or `c:<root task id>`).
        #[arg(long)]
        id: String,
        /// Restrict the lookup to one workspace. Explicit ids are user-chosen,
        /// so two workspaces can carry the same one; pass this to disambiguate.
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: Option<u32>,
        /// Format: table | json | dot. Default: table (parent ID table). The table lists every edge source as a parent; use json or dot to see edge kinds.
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Output a DAG's report: for each task, the automatic part projected from its
    /// stored state and the custom notes of every attempt. Standard output and
    /// error are left out unless `--include-raw` is given.
    DagReport {
        /// DAG id from `dag-list` (`d:<metadata.dag>` or `c:<root task id>`).
        dag: String,
        /// Restrict the lookup to one workspace. Omitted = every live workspace.
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: Option<u32>,
        /// Only this task.
        #[arg(long)]
        task: Option<String>,
        /// Only this attempt of `--task`. An attempt closed by a retry shows the
        /// automatic part saved once at that retry (null for an attempt closed before
        /// this was saved).
        #[arg(long, requires = "task")]
        attempt: Option<u32>,
        /// Include the stored standard output and error.
        #[arg(long, default_value_t = false)]
        include_raw: bool,
    },
    /// Write custom notes to the report of the task attempt you are running.
    Report {
        #[command(subcommand)]
        command: ReportCommands,
    },
    /// Start, stop, or inspect a workspace runner. Starting it twice is idempotent.
    /// The host restores saved state at startup but leaves runners stopped.
    ///
    /// Stopping a runner stops completion checks, not its dispatched processes.
    /// Their tasks remain Running. The exception is a running postprocess: it is
    /// ended and recorded as cancelled, and its task fails when the runner starts
    /// again. Starting again restores handles and checks whether processes are
    /// alive or have a saved exit result.
    TaskRun {
        /// Workspace id whose runner is being controlled (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// start | stop | status. Default: status.
        #[arg(long, value_enum, default_value_t = TaskRunAction::Status)]
        action: TaskRunAction,
    },
    /// Manually report a task's terminal result (succeeded | failed).
    /// For signalling completion of external/manual tasks that the runner
    /// thread did not dispatch.
    TaskSetResult {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id whose result is being reported.
        #[arg(long)]
        id: String,
        /// Terminal state: `succeeded` | `failed`.
        #[arg(long)]
        state: String,
        /// Optional output JSON. Accepts inline JSON or `@path/to/file.json`.
        #[arg(long)]
        output: Option<String>,
        /// Optional error message (recommended for state=failed).
        #[arg(long)]
        error: Option<String>,
        /// Optional process exit code.
        #[arg(long)]
        exit_code: Option<i32>,
        /// Optional attempt id of a typed task run; a report for another attempt is rejected.
        #[arg(long)]
        attempt_id: Option<String>,
    },
    /// Submit the structured result of an agent task's current attempt. The value is
    /// checked against the task's output type now and becomes the result when the
    /// agent's turn ends; a successful submission does not finish the task. The same
    /// value may be submitted again; a different value for the same attempt is rejected.
    TaskSubmit {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Agent task id.
        #[arg(long)]
        id: String,
        /// Attempt id the instruction named; a submission for another attempt is rejected.
        #[arg(long)]
        attempt_id: String,
        /// Attempt token the instruction named; a submission with another token is rejected.
        #[arg(long)]
        token: String,
        /// Result JSON. Accepts inline JSON or `@path/to/file.json`.
        #[arg(long)]
        output: String,
    },
    /// Delete a task. Rejected by default if other tasks still reference it
    /// (depends_on / on_failure.fallback.task / reduce.inputs) — the
    /// referencing task IDs are returned. `Running` tasks are always
    /// rejected regardless of `--cascade`/`--force` (cancel first).
    TaskDelete {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Task id to delete.
        #[arg(long)]
        id: String,
        /// Also delete every task that transitively references this one.
        #[arg(long, default_value_t = false)]
        cascade: bool,
        /// Skip the reference check only — state constraints (Running) still apply.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Bulk-delete tasks matching state/age filters. At least one of
    /// `--states`/`--older-than-ms` is required. Only tasks that are safe
    /// to delete (not Running, not referenced from outside the matched set)
    /// are actually removed; the rest are reported as retained.
    TaskPurge {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Comma-separated state names (waiting|ready|running|succeeded|failed|cancelled|skipped|unknown).
        #[arg(long, value_delimiter = ',')]
        states: Vec<String>,
        /// Only tasks whose age (created_at, or finished_at for terminal
        /// tasks) exceeds this many milliseconds are candidates.
        #[arg(long)]
        older_than_ms: Option<u64>,
        /// Compute and print the plan without deleting anything.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Create a barrier that closes after N signals.
    ///
    /// Expiry is checked on the next barrier-signal, barrier-state, or
    /// barrier-list call, not by a timer.
    /// A timed-out barrier rejects signals and remains until barrier-delete.
    BarrierCreate {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Barrier name (unique within the workspace).
        #[arg(long)]
        name: String,
        /// Required signal count (must be >= 1).
        #[arg(long)]
        count_required: u32,
        /// Optional timeout in milliseconds, measured from creation time. Omit
        /// for a barrier that waits forever.
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Signal a barrier (count_signaled++).
    BarrierSignal {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Barrier name to signal.
        #[arg(long)]
        name: String,
    },
    /// Read a barrier's current state (older name of barrier-state). Returns immediately; it does not wait.
    BarrierAwait {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Barrier name to poll.
        #[arg(long)]
        name: String,
    },
    /// Read a barrier's current state. Returns immediately; it does not wait for the barrier to close.
    BarrierState {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Barrier name to read.
        #[arg(long)]
        name: String,
    },
    /// Create a semaphore with N permits.
    SemaphoreCreate {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Semaphore name (unique within the workspace).
        #[arg(long)]
        name: String,
        /// Permit count (must be >= 1).
        #[arg(long)]
        permits: u32,
    },
    /// Change a semaphore's permit count in place. Growing takes effect at
    /// once; shrinking drains — current holders are never revoked, new
    /// acquires are refused until the holder count falls under the new limit.
    SemaphoreSetPermits {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Semaphore name to resize.
        #[arg(long)]
        name: String,
        /// New permit count (must be >= 1).
        #[arg(long)]
        permits: u32,
    },
    /// Acquire 1 permit. Idempotent for the same holder.
    ///
    /// A full semaphore is not an error: the call succeeds and reports
    /// `acquired: false` along with the current holders, so read that field
    /// rather than the exit code. There is no waiting mode — retrying is the
    /// caller's job.
    SemaphoreAcquire {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Semaphore name to acquire a permit from.
        #[arg(long)]
        name: String,
        /// Holder id (must be non-empty).
        #[arg(long)]
        holder: String,
        /// Expire this permit after this many milliseconds. Re-acquiring with
        /// the same holder renews it; acquire/list calls reclaim expired permits.
        /// Omit for no expiry.
        #[arg(long)]
        ttl_ms: Option<u64>,
    },
    /// Release a permit. No-op if holder isn't currently holding.
    SemaphoreRelease {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Semaphore name the permit belongs to.
        #[arg(long)]
        name: String,
        /// Holder id that currently holds the permit.
        #[arg(long)]
        holder: String,
    },
    /// List all barriers in a workspace.
    BarrierList {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
    },
    /// Delete a barrier (no-op if missing).
    BarrierDelete {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Barrier name to delete.
        #[arg(long)]
        name: String,
    },
    /// List all semaphores in a workspace.
    SemaphoreList {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
    },
    /// Delete a semaphore (no-op if missing).
    SemaphoreDelete {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Semaphore name to delete.
        #[arg(long)]
        name: String,
    },
    /// Acquire a lease on an arbitrary resource (advisory, cooperative).
    ///
    /// Whether you got it is not the exit code. `--mode fail` turns a conflict
    /// into an error, but `--mode block` succeeds and reports `acquired: false`
    /// with the current holder — read that field. Nothing enforces the lease
    /// either: it is a marker the participants agree to read.
    ///
    /// Without `--ttl-ms` the lease never expires and only `lease-release` with
    /// the same holder id frees it, so a holder that dies keeps the resource. The
    /// restart cleanup does not cover that case — it only releases leases a
    /// running task holds under its own task id.
    LeaseAcquire {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Resource identifier (e.g. `file:/path`, `workspace:foo`).
        #[arg(long)]
        resource: String,
        /// Holder id (must be non-empty).
        #[arg(long)]
        holder: String,
        /// Time-to-live in milliseconds. Omit for a lease that never expires.
        /// Expiry is lazy — an expired lease is evicted by the next `lease-acquire`
        /// or `lease-list`, not at the moment it lapses.
        #[arg(long)]
        ttl_ms: Option<u64>,
        /// Conflict mode: fail | block (default: fail). `block` does not wait —
        /// it returns at once with `acquired: false`, and retrying is the caller's
        /// job.
        #[arg(long, default_value = "fail")]
        mode: String,
    },
    /// Release a lease. Only the current holder can release.
    LeaseRelease {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Resource identifier the lease was acquired on.
        #[arg(long)]
        resource: String,
        /// Holder id that currently holds the lease.
        #[arg(long)]
        holder: String,
    },
    /// List all leases in a workspace (expired leases auto-evicted).
    LeaseList {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
    },
    /// Combine results from N tasks into one value (strategies: first_success | all | merge_json | concat_text | custom:<cmd>).
    TaskReduce {
        /// Workspace id (focus-independent — required).
        #[arg(long = "workspace", visible_alias = "workspace-id")]
        workspace_id: u32,
        /// Comma-separated input task IDs (in reduce order).
        #[arg(long, value_delimiter = ',')]
        inputs: Vec<String>,
        /// Strategy: `first_success` | `all` | `merge_json` | `concat_text` | `custom:<command>`.
        #[arg(long)]
        strategy: String,
        /// JSON Pointer (e.g. `/stdout/text`) extracted from each input's `output`
        /// before reducing — recommended when reducing `Run` task results, whose
        /// `output` is `{pid,stdout:{text,...},stderr:{...}}` rather than a plain
        /// value. Inputs missing the path are treated as `null` and reported in
        /// the response's `warnings` (the rest of the reduce still proceeds).
        #[arg(long)]
        extract_path: Option<String>,
    },
    /// Configure a rate limit bucket for (agent, metric) — `limit` tokens per `per_ms` ms.
    ///
    /// One metric is actually spent from: `ipc_calls`. A bucket stored under any
    /// other name is listed by `rate-limit-list` and charged by nothing, so it
    /// throttles nothing.
    ///
    /// The `ipc_calls` bucket is charged one token per IPC call, and a call over
    /// budget comes back as `-32010 throttled` with a Deny in the audit log. Not
    /// every call is charged: a caller with no session token — the plain CLI —
    /// and the host's own calls are exempt, as are `telemetry.*`,
    /// `agent.rate_limit_*` and `system.info`. The last two are what let a
    /// throttled agent read and lift its own limit.
    RateLimitSet {
        /// Agent id.
        #[arg(long)]
        agent: String,
        /// Metric name. `ipc_calls` is the only one anything spends from.
        #[arg(long)]
        metric: String,
        /// Token refill amount per window.
        #[arg(long)]
        limit: u32,
        /// Window length in milliseconds (e.g. `60000` for per-minute).
        #[arg(long)]
        per_ms: u64,
        /// Bucket capacity (defaults to `limit`).
        #[arg(long)]
        burst: Option<u32>,
    },
    /// List all configured rate limit buckets (with current token state).
    RateLimitList,
    /// Remove a rate limit bucket by id.
    RateLimitRemove {
        /// Bucket id, as reported by `rate-limit-list`.
        #[arg(long)]
        id: String,
    },
    /// Inspect rate limit state, optionally filtered by agent and/or metric.
    RateLimitStatus {
        /// Only report buckets belonging to this agent id.
        #[arg(long)]
        agent: Option<String>,
        /// Only report buckets for this metric name.
        #[arg(long)]
        metric: Option<String>,
    },
}

/// `tasty agent report ...`.
#[derive(Subcommand)]
pub enum ReportCommands {
    /// Append one text note to the attempt's report block. Without `--address` the
    /// address comes from `TASTY_TASK_REPORT`, which the task runner gives to the
    /// processes it starts. Notes over the limits are cut or counted, not refused.
    Append {
        /// Report address (`TASTY_TASK_REPORT` value or the one in an agent instruction).
        #[arg(long)]
        address: Option<String>,
        /// The note. Plain text; one call is one entry.
        text: String,
    },
    /// Print how to write report notes.
    Usage,
}
