//! `tasty approval ...` subcommand 정의 — Approval + Summary.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ApprovalCommands {
    /// Request a new approval. Prints the new id on success.
    ///
    /// Nothing expires on its own. `--timeout-ms` is counted inside
    /// `approval await`, and that call is the only thing that moves a request out
    /// of `pending` on time — with no waiter the request stays pending however
    /// long the timeout was, and `approval list --state pending` keeps showing it.
    Request {
        /// Title shown in the popup / notification.
        #[arg(long)]
        title: String,
        /// Optional body (multi-line description).
        #[arg(long)]
        body: Option<String>,
        /// Comma-separated `key:label[:destructive]` triples (e.g. `approve:Approve,deny:Deny:1`).
        /// Defaults to `approve / deny` if omitted.
        #[arg(long)]
        choices: Option<String>,
        /// Choice key recorded when the request times out. The outcome is
        /// `timed_out` carrying this key, credited to the system — not
        /// `responded`, so `approval history --decision` does not match it.
        #[arg(long)]
        default_choice: Option<String>,
        /// Timeout in milliseconds. Caller may override at `await` time.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Severity: info | warn | danger. Default: info.
        #[arg(long)]
        severity: Option<String>,
        /// Workspace id binding (defaults to the workspace of --surface-id, then the active workspace).
        #[arg(long)]
        workspace_id: Option<u32>,
        /// Surface id binding (optional).
        #[arg(long)]
        surface_id: Option<u32>,
        /// Free-form JSON metadata (string passed verbatim; must parse).
        #[arg(long)]
        metadata: Option<String>,
    },
    /// Submit a response.
    Respond {
        /// Approval id (`req_...`).
        #[arg(long)]
        id: String,
        /// Choice key (must match one of the request's `choices`).
        #[arg(long)]
        choice: String,
        /// Optional comment.
        #[arg(long)]
        comment: Option<String>,
    },
    /// Cancel a pending approval (terminal).
    Cancel {
        /// Approval id (`req_...`) to cancel.
        #[arg(long)]
        id: String,
    },
    /// Block until response / timeout / cancel. Prints the outcome as JSON.
    Await {
        /// Approval id (`req_...`) to wait on.
        #[arg(long)]
        id: String,
        /// Override request's `timeout_ms`. 0 = use request's value (or wait forever).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Show a single approval record.
    Get {
        /// Approval id (`req_...`) to show.
        #[arg(long)]
        id: String,
    },
    /// List approvals. Optional state filter and workspace filter.
    List {
        /// Filter by state: pending | responded | timed_out | cancelled | terminal.
        #[arg(long)]
        state: Option<String>,
        /// Filter by workspace id.
        #[arg(long)]
        workspace_id: Option<u32>,
    },
    /// Set or read the workspace markdown summary (manual write/read only).
    Summary {
        #[command(subcommand)]
        command: ApprovalSummaryCommands,
    },
    /// Persistent history query (sources from memory store).
    History {
        /// Only entries with memory `updated_at >= since` (unix ms).
        #[arg(long)]
        since: Option<i64>,
        /// Only entries with memory `updated_at < until` (unix ms).
        #[arg(long)]
        until: Option<i64>,
        /// Filter by workspace id.
        #[arg(long)]
        workspace_id: Option<u32>,
        /// Filter by requester id (plugin/agent id or "user").
        #[arg(long)]
        requester_id: Option<String>,
        /// Filter by chosen decision key (only matches Responded).
        #[arg(long)]
        decision: Option<String>,
        /// Filter by state: pending | responded | timed_out | cancelled | terminal.
        #[arg(long)]
        state: Option<String>,
        /// Maximum entries returned (after sort, newest-first).
        #[arg(long)]
        limit: Option<u64>,
    },
}

#[derive(Subcommand)]
pub enum ApprovalSummaryCommands {
    /// Overwrite the workspace summary with `content` (or `@file`).
    Set {
        /// Workspace whose summary is overwritten (focus-independent — required).
        #[arg(long)]
        workspace_id: u32,
        /// Inline content, or `@path` to read from a file.
        #[arg(long)]
        content: String,
    },
    /// Print the workspace summary.
    Get {
        /// Workspace whose summary is printed (focus-independent — required).
        #[arg(long)]
        workspace_id: u32,
    },
}
