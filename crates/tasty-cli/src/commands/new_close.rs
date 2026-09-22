//! `tasty new` / `tasty close` subcommand 정의.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum NewCommands {
    /// Create a new window
    Window,
    /// Create a new workspace
    Workspace {
        /// Name for the new workspace
        #[arg(long)]
        name: Option<String>,
        /// Working directory for the new workspace
        #[arg(long)]
        cwd: Option<String>,
        /// Surface type: terminal (default), markdown, explorer, html, image, dag_graph
        #[arg(long, default_value = "terminal")]
        r#type: String,
        /// File path (for markdown/image type)
        #[arg(long)]
        file: Option<String>,
        /// Directory path (for explorer type)
        #[arg(long)]
        path: Option<String>,
        /// URL (for html type)
        #[arg(long)]
        url: Option<String>,
        /// Map this workspace to a saved SSH profile (a remote machine). When the
        /// workspace is activated, the host attaches automatically (SSH tunnel +
        /// workspace mirror).
        #[arg(long)]
        ssh_profile: Option<String>,
        /// Map to a one-off inline SSH target without a saved profile, e.g.
        /// --ssh user@host. Mutually exclusive with `--ssh-profile`.
        #[arg(long)]
        ssh: Option<String>,
        /// Workspace id on the mapped remote tasty to attach to.
        #[arg(long)]
        remote_workspace: Option<u32>,
        /// Category to place the workspace in (name or id). Defaults to normal.
        #[arg(long)]
        category: Option<String>,
        /// Create the workspace in the window that holds this surface ID, and inherit the working directory from that surface when --cwd is omitted. Without it the workspace goes to the window the user is looking at.
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Create a new tab in the specified pane
    Tab {
        /// Target pane ID (required)
        #[arg(long)]
        pane: u32,
        /// Surface type: terminal (default), markdown, explorer, html, image, dag_graph
        #[arg(long, default_value = "terminal")]
        r#type: String,
        /// Working directory (for terminal type)
        #[arg(long)]
        cwd: Option<String>,
        /// File path (for markdown type)
        #[arg(long)]
        file: Option<String>,
        /// Directory path (for explorer type)
        #[arg(long)]
        path: Option<String>,
        /// URL (for html type)
        #[arg(long)]
        url: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CloseCommands {
    /// Close a specific tab by its ID
    Tab {
        /// Target tab ID (required)
        #[arg(long)]
        tab: u32,
    },
    /// Close the specified pane (unsplit)
    Pane {
        /// Target pane ID (required)
        #[arg(long)]
        pane: u32,
    },
    /// Close the specified surface within a tab
    Surface {
        /// Target surface ID (required)
        #[arg(long)]
        surface: u32,
    },
    /// Close a whole workspace by its ID, with every pane, tab and surface in it.
    ///
    /// This cannot be undone: every terminal in the workspace is killed, the
    /// workspace does not enter the user's reopen history, and its scrollback is
    /// not kept. Only closes the user made by hand can be reopened.
    ///
    /// The target is always given explicitly with --id (see `tasty list workspaces`);
    /// it is never inferred from what is on screen. Closing a workspace the user is
    /// not looking at leaves their current workspace on screen. A workspace holding
    /// your own surface is refused, as is a workspace mirroring a remote attach
    /// session and the last remaining workspace - closing the window is a separate
    /// decision, see `tasty close window`.
    Workspace {
        /// Target workspace ID (required)
        #[arg(long)]
        id: u32,
    },
    /// Close a window by its ID.
    ///
    /// The target is always given explicitly with --id (see `tasty list windows`).
    /// The last remaining main window is refused - quitting is a user action.
    Window {
        /// Target window ID (required)
        #[arg(long)]
        id: u64,
    },
    /// Close the calling surface itself (uses TASTY_SURFACE_ID)
    #[command(name = "self")]
    CloseSelf,
}
