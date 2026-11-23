//! 공유 훅 핸들러를 ID로 조회·편집·재로드·실행한다. 목록은 비활성 항목도 포함한다.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum HookHandlerCommands {
    /// List every registered hook handler (host/plugin/user, incl. disabled).
    List,
    /// Show one handler by id, including the full action body.
    ///
    /// `list` only counts the steps of an IpcSequence; this prints the calls
    /// themselves. The `action` object it prints is exactly what `upsert
    /// --action` takes, so a sequence can be read, edited and sent back.
    Get {
        /// Hook handler id to show (e.g. `host/notify`, `user/wh-...`).
        #[arg(long)]
        id: String,
    },
    /// Create or edit a user handler in place, without opening the TOML by hand.
    ///
    /// Fields that are not given are left as they are — this patches the
    /// handler rather than replacing it, so the id, and every binding that
    /// refers to it, survive the edit. At least one field must be given.
    /// The change is written to `~/.tasty/hook-handlers.toml` right away.
    ///
    /// An IpcSequence that a webhook was already bound to keeps firing what it
    /// captured at registration time; re-register the webhook to pick this up.
    Upsert {
        /// Hook handler id as `<owner>/<short-name>` (e.g. `user/my-handler`).
        #[arg(long)]
        id: String,
        /// Trigger source gate: `hook`, `webhook` or `any`.
        #[arg(long)]
        source: Option<String>,
        /// Ordering priority (lower runs first).
        #[arg(long)]
        priority: Option<i32>,
        /// Translation key for the display name.
        #[arg(long)]
        display_name_key: Option<String>,
        /// Disable (`true`) or enable (`false`) the handler.
        #[arg(long)]
        disabled: Option<bool>,
        /// Whole action as JSON, in the shape `get` prints:
        /// `{"kind":"ipc_sequence","calls":[...]}` or
        /// `{"kind":"shell_command","command":"...","args":[...]}`.
        #[arg(long, conflicts_with = "calls")]
        action: Option<String>,
        /// Calls for an IpcSequence, e.g. [{"method":"system.info","params":{}}].
        /// Uses the same shape as webhook register --sequence.
        #[arg(long)]
        calls: Option<String>,
    },
    /// Remove the user-origin part of a handler. Host/plugin defaults stay.
    ///
    /// If host or a plugin planted the same id, that default becomes visible
    /// again; the answer says so in `still_present`.
    Remove {
        /// Hook handler id to remove (e.g. `user/my-handler`).
        #[arg(long)]
        id: String,
    },
    /// Reload `~/.tasty/hook-handlers.toml` (user source only; host/plugin unaffected).
    Reload,
    /// Manually fire a registered hook handler by id (test / automation entry point).
    ///
    /// For an IpcSequence handler, `--body`/`--header`/`--query` fill its
    /// `${body.x}`/`${header.x}`/`${query.x}` value slots. Prints the
    /// acceptance (with a `run` number) on one line, then waits for the final
    /// result and prints it on a second line: `state` is `succeeded`,
    /// `partial`, `failed` or `unknown`, `result` says what happened and
    /// `steps` lists each step. A shell command handler succeeds once its
    /// process starts; `exit_code` is filled once it exits and is not judged.
    /// Exits with code 1 unless the final state is `succeeded`. If the queue
    /// that runs the handler is already full (shell commands share one
    /// executor queue with surface and global hooks) or its worker thread
    /// cannot be started, the dispatch is refused with an error and nothing
    /// runs.
    Dispatch {
        /// Hook handler id to fire (e.g. `host/notify`, `user/wh-...`).
        #[arg(long)]
        id: String,
        /// Substitution body as a JSON value (object/array/scalar). Optional.
        #[arg(long)]
        body: Option<String>,
        /// Header substitution values as a JSON object `{"X-Sig":"abc"}`. Optional.
        #[arg(long)]
        header: Option<String>,
        /// Query substitution values as a JSON object `{"token":"t"}`. Optional.
        #[arg(long)]
        query: Option<String>,
        /// Print only the acceptance and return without waiting. Look the
        /// result up later with `tasty hook-handler result --run <N>`.
        #[arg(long)]
        no_wait: bool,
    },
    /// Show the progress or final result of a dispatch by its `run` number.
    ///
    /// `done` is false while it is still running. The last 128 runs, counted
    /// together with file handler requests, are kept; an older or unknown run
    /// is an error.
    Result {
        /// The `run` number from the dispatch acceptance.
        #[arg(long)]
        run: u64,
    },
}
