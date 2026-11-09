//! `tasty surface` subcommand 정의 — surface 대상 액션(completion 등).

use clap::Subcommand;

#[derive(Subcommand)]
pub enum SurfaceCommands {
    /// Signal that a surface has completed its work (or needs input), raising
    /// attention highlight.
    ///
    /// The target surface is always given explicitly with --surface; it is
    /// never inferred from focus.
    Completion {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
        /// Attention kind: "completion" (default) or "needs_input". Any other
        /// value is treated as "completion".
        #[arg(long)]
        kind: Option<String>,
    },
    /// Mark a surface as busy or idle, lighting the tab and sidebar dot.
    ///
    /// Use it from a long-running program inside a terminal whose foreground
    /// process alone does not show that it is working, such as an agent
    /// waiting on a network reply. A terminal mark clears itself when the
    /// program in front at the time of the mark leaves the foreground.
    /// Without --surface it marks the surface it runs in (TASTY_SURFACE_ID).
    SetBusy {
        /// Surface ID (default: TASTY_SURFACE_ID)
        #[arg(long)]
        surface: Option<u32>,
        /// true to mark busy, false to clear the mark
        #[arg(long, action = clap::ArgAction::Set)]
        busy: bool,
    },
    /// Print where the cursor sits in a terminal surface, as column and row.
    ///
    /// The target surface is always given explicitly with --surface; it is
    /// never inferred from focus.
    CursorPosition {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Print whether the program inside a terminal surface has grabbed the
    /// mouse, and at which level.
    ///
    /// Use it before sending mouse sequences or expecting drag selection to
    /// work: when tracking is on, the program consumes mouse input; when it is
    /// off, mouse sequences would land on screen as stray characters. The
    /// target surface is always given explicitly with --surface.
    MouseTracking {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Print the process currently in the foreground of a terminal surface.
    ///
    /// Use it to tell whether a shell is idle or is running something. The
    /// target surface is always given explicitly with --surface.
    ForegroundProcess {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Print which pane a surface belongs to, and whether it is still in the
    /// tree at all.
    ///
    /// Use it to check that a surface you created or are waiting on has not
    /// been closed. The target surface is always given explicitly with
    /// --surface.
    Locate {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Print the script and remote content state of an html surface: what
    /// was detected in the current document, whether scripts may run and
    /// remote content may load, and the notice states.
    ///
    /// Read-only: the query never shows the banner and never counts as the
    /// user viewing the document. The target surface is always given
    /// explicitly with --surface. Not available in a headless build.
    HtmlScript {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Allow the scripts of the document an html surface shows, as the
    /// banner button does, and reload it.
    ///
    /// Pass the url and fingerprint printed by html-script so a document
    /// replaced after that query is refused instead of allowed. The result
    /// field is "allowed" or "already_allowed". A refusal names its reason:
    /// document_changed, loading, no_document or no_fingerprint. Focus,
    /// selection and the active tab stay as they are. The target surface is
    /// always given explicitly with --surface. Not available in a headless
    /// build.
    HtmlScriptAllow {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
        /// Document url from html-script; refuse if the document differs
        #[arg(long)]
        url: Option<String>,
        /// Document fingerprint from html-script; refuse if it differs
        #[arg(long)]
        fingerprint: Option<String>,
    },
    /// Allow remote content (network stylesheets, images, fonts, frames and
    /// scripts) for the document an html surface shows, as the remote notice
    /// button does, and reload it.
    ///
    /// The allowance covers this document only and is separate from the
    /// script allowance. Pass the url and fingerprint printed by html-script
    /// so a document replaced after that query is refused instead of allowed.
    /// The result field is "allowed" or "already_allowed". A refusal names
    /// its reason: unsupported_view, globally_allowed, document_changed,
    /// loading, no_document or no_fingerprint. Focus, selection, the active
    /// tab and the notice stay as they are. The target surface is always
    /// given explicitly with --surface. Not available in a headless build.
    HtmlRemoteAllow {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
        /// Document url from html-script; refuse if the document differs
        #[arg(long)]
        url: Option<String>,
        /// Document fingerprint from html-script; refuse if it differs
        #[arg(long)]
        fingerprint: Option<String>,
    },
    /// Print the address of the document an html surface shows and whether
    /// it can go back or forward in its history.
    ///
    /// The address is the last committed one: the final address after
    /// redirects, with a #fragment move applied, and the previous document
    /// while a load is in flight or after it failed. Read-only. The target
    /// surface is always given explicitly with --surface. Not available in a
    /// headless build.
    HtmlNav {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Go back one entry in the history of an html surface.
    ///
    /// The step is applied on the next redraw as a load the host started, so
    /// a script banner waits until the user selects the surface. Nothing
    /// happens when there is nowhere to go; check the result with html-nav.
    /// Focus and selection stay as they are. The target surface is always
    /// given explicitly with --surface. Not available in a headless build.
    HtmlBack {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Go forward one entry in the history of an html surface.
    ///
    /// The step is applied on the next redraw as a load the host started, so
    /// a script banner waits until the user selects the surface. Nothing
    /// happens when there is nowhere to go; check the result with html-nav.
    /// Focus and selection stay as they are. The target surface is always
    /// given explicitly with --surface. Not available in a headless build.
    HtmlForward {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Reload the document an html surface shows.
    ///
    /// The reload is applied on the next redraw as a load the host started, so
    /// a script banner waits until the user selects the surface. Nothing
    /// happens when the surface has no document; check the result with
    /// html-nav. Focus and selection stay as they are. The target surface is
    /// always given explicitly with --surface. Not available in a headless
    /// build.
    HtmlReload {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Restart the shell of a terminal surface in place, keeping the surface,
    /// its tab and its position.
    ///
    /// The target surface is always given explicitly with --surface.
    RespawnTerminal {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Fire a surface hook by name, as if that event had just happened.
    ///
    /// The target surface is always given explicitly with --surface.
    FireHook {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
        /// Hook event name to fire (required)
        #[arg(long)]
        event: String,
    },
    /// Inspect or clear the attention highlight of a surface.
    ///
    /// The reverse of `completion`. The other two clear paths (real render
    /// focus and reading the notification) are GUI-local events, so this is
    /// the only way to clear attention on a headless instance.
    Attention {
        #[command(subcommand)]
        command: SurfaceAttentionCommands,
    },
}

#[derive(Subcommand)]
pub enum SurfaceAttentionCommands {
    /// Show the attention kind recorded for a surface: "completion",
    /// "needs_input", or null when the surface has none.
    ///
    /// The target surface is always given explicitly with --surface; it is
    /// never inferred from focus. Allowed while the surface is occupied by a
    /// remote attach session (this is a read-only query).
    Get {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Clear the attention highlight of a surface.
    ///
    /// The target surface is always given explicitly with --surface; it is
    /// never inferred from focus. Clearing a surface that has no attention
    /// succeeds and reports "cleared": false. Rejected for a surface that is
    /// hard-occupied by a remote attach session, and for a mirror surface of
    /// a remote workspace: in both cases the attention belongs to the
    /// instance that owns the surface.
    Clear {
        /// Surface ID (required)
        #[arg(long)]
        surface: u32,
        /// Only clear when the recorded kind matches: "completion" or
        /// "needs_input". Omit to clear regardless of kind. Any other value
        /// is rejected.
        #[arg(long)]
        kind: Option<String>,
    },
}
