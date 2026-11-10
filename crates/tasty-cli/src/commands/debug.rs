//! `tasty debug ...` subcommand 정의 — Debug + Tool + Popup + Extension + EventBus.

#![cfg(debug_assertions)]

use clap::Subcommand;

#[derive(Subcommand)]
pub enum DebugCommands {
    /// Local loopback attach to a surface/workspace and mirror it (debug builds
    /// only — local self-attach simulates user-driven mirroring, so it is
    /// isolated from the release surface per the agent/user action separation
    /// policy). Remote attach is `tasty remote attach` (release).
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
        /// mirrored screen to stdout, then exit.
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
        /// Forcibly detach an occupied surface/workspace (server-side; does not
        /// attach).
        #[arg(long)]
        force_detach: bool,
    },
    /// Show debug info from the running tasty instance
    Info,
    /// Artificially block the next frame's GPU present to reproduce an event
    /// loop stall (for verifying the stall watchdog; debug builds only).
    GpuStall {
        /// How long to block, in milliseconds
        #[arg(long, default_value_t = 8000)]
        ms: u64,
    },
    /// Enable IME composition mode
    ImeEnable,
    /// Disable IME composition mode and clear preedit
    ImeDisable,
    /// Send IME preedit (composition) text
    ImePreedit {
        /// Composition text (e.g. an in-progress Hangul syllable such as U+D558)
        #[arg()]
        text: String,
        /// Cursor position within composition
        #[arg(long)]
        cursor: Option<u64>,
    },
    /// Commit IME composition text (finalize and send to terminal)
    ImeCommit {
        /// Finalized text to commit (e.g. the completed Hangul syllable U+D55C)
        #[arg()]
        text: String,
    },
    /// Show current IME status
    ImeStatus,
    /// Get cell info at a specific position
    CellInfo {
        /// Row (0-indexed)
        #[arg(long)]
        row: u64,
        /// Column (0-indexed)
        #[arg(long)]
        col: u64,
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Get all cell attributes for a specific row
    ScreenAttrs {
        /// Row (0-indexed)
        #[arg(long)]
        row: u64,
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Get the resolved RGBA bg/fg the renderer would push to the GPU for a cell
    GlyphColor {
        /// Row (0-indexed)
        #[arg(long)]
        row: u64,
        /// Column (0-indexed)
        #[arg(long)]
        col: u64,
        /// Surface ID (default: $TASTY_SURFACE_ID, the calling surface)
        #[arg(long)]
        surface: Option<u32>,
        /// Background mode: "focused" or "unfocused" (default: focused)
        #[arg(long, default_value = "focused")]
        bg_mode: String,
    },
    /// Switch the input source (for example, the Korean IME). macOS GUI builds only.
    ///
    /// The command is listed everywhere; platforms without TISSelectInputSource
    /// return -32015 instead of an unknown-command error.
    SwitchInputSource {
        /// Input source ID (e.g. "com.apple.inputmethod.Korean.2SetKorean")
        #[arg()]
        source_id: String,
    },
    /// Send a physical key code through CGEvent and the OS IME. macOS GUI builds only.
    ///
    /// Platforms without CGEventPost return -32015.
    RawKey {
        /// macOS virtual key code (e.g. 7=KeyX, 35=KeyP, 49=Space)
        #[arg()]
        keycode: u16,
    },
    /// Event Bus inspection and injection (debug builds only)
    #[command(subcommand)]
    EventBus(EventBusCommands),
    /// Extension hook inspection and manual invocation (debug builds only)
    #[command(subcommand)]
    Extension(ExtensionDebugCommands),
    /// Tool menu inspection and invocation (debug builds only)
    #[command(subcommand)]
    Tool(ToolDebugCommands),
    /// Plugin popup inspection and open/close (debug builds only)
    #[command(subcommand)]
    Popup(PopupDebugCommands),
    /// Host built-in popup inspection and direct open/close (debug builds only).
    /// Forces a host popup (tools_menu / port_scanner / command_palette /
    /// remote_tool ...) open without the user-click path, for visual verification.
    #[command(subcommand)]
    HostPopup(HostPopupDebugCommands),
    /// Fullscreen stage inspection and direct enter/exit (debug builds only).
    /// Puts a stage on a window without the popup-titlebar click path, and dumps
    /// the window's fullscreen state, for visual verification of the stage.
    #[command(subcommand)]
    Fullscreen(FullscreenDebugCommands),
    /// Fire a toast in a chosen scope and target (debug builds only). Toasts only
    /// follow user actions in release; this places one for measuring its position
    Toast {
        /// Toast text
        #[arg()]
        message: String,
        /// Scope: window, workspace, pane or surface
        #[arg(long, default_value = "window")]
        scope: String,
        /// Workspace, pane or surface id for the non-window scopes
        #[arg(long)]
        target_id: Option<u32>,
        /// Target window, main or auxiliary (Settings, Preset, Plugins). May be omitted only when exactly one main window is open
        #[arg(long)]
        window_id: Option<u64>,
        /// Kind: info, success, warning or error
        #[arg(long, default_value = "info")]
        kind: String,
        /// Shortcut keycap shown after the text; repeat for each key
        #[arg(long)]
        hint: Vec<String>,
    },
    /// Banner inspection and direct fire/close (debug builds only).
    /// Fires a banner without the user-action path (banners only fire from user
    /// actions in release), for visual verification of the overlay.
    #[command(subcommand)]
    Banner(BannerDebugCommands),
    /// Modifier-hint overlay hold injection + render-state dump (debug builds only).
    /// Sets the overlay's held modifier combo without a real key hold and dumps
    /// the recomputed render state (narrowed sections / delay / visibility).
    #[command(subcommand)]
    ModifierHint(ModifierHintDebugCommands),
    /// Settings modal force-open (debug builds only).
    /// Opens the settings modal without the user-action path (shortcut / button
    /// click), for visual verification of the settings UI against the design.
    #[command(subcommand)]
    Settings(SettingsDebugCommands),
    /// Active-modal close **request** (debug builds only).
    /// Reproduces the user pressing the window close button on whichever modal
    /// is up. Release `window.close` covers main views only — closing a modal is
    /// a user action, not agent work.
    #[command(subcommand)]
    Modal(ModalDebugCommands),
    /// Arbitrary Lua injection into the host worker (debug builds only).
    /// Runs source in the isolated Lua worker (deadline-guarded).
    /// Release builds have no such path — the user-input-only rule applies there.
    #[command(subcommand)]
    Lua(LuaDebugCommands),
    /// VTE sequence simulator — identical to the standalone `tasty-tui-sim`
    /// binary, run from inside the current surface (emits raw VTE to stdout, no
    /// IPC). No subcommand = interactive REPL. Use `sim flood` for a heavy
    /// full-screen redraw stress load. Debug builds only.
    Sim {
        #[command(subcommand)]
        cmd: Option<tasty_tui_simulator::Commands>,
    },
    /// Dump the focused surface id (debug builds only). Observation only — the
    /// focused view is read, never changed.
    FocusedSurface,
    /// List every window, including auxiliary ones such as Settings, Preset,
    /// Plugins and the quit dialog (debug builds only). Gives the window id
    /// that `--window-id` of the egui inject commands and `debug toast` take.
    /// Release `window list` covers main windows only.
    Windows,
    /// Dump the on-screen rect of a surface in the active tab, in physical pixels (debug builds
    /// only). Works for non-terminal surfaces too; null when the surface is not on screen.
    SurfaceRect {
        /// Surface id.
        #[arg(long)]
        surface: u32,
    },
    /// Dump the current selection of the focused window (debug builds only).
    Selection,
    /// Dump the pending native menu action of the focused window (debug builds only).
    PendingMenu,
    /// Answer the next native context menu of the given kind in the focused window without the
    /// OS popup (debug builds only). Picks an item by id or label, or closes the menu. Menus of
    /// other kinds open as usual. A right click such as `debug inject egui-mouse --button 2`
    /// still opens the menu, and `debug inject window-mouse --button 2` opens terminal menus.
    MenuAnswer {
        /// Menu kind the answer applies to, as `debug pending-menu` names it (Tab, Pane,
        /// TerminalSurface, ...).
        #[arg(long)]
        menu: String,
        /// Menu item id.
        #[arg(long, conflicts_with_all = ["label", "dismiss"])]
        item: Option<u32>,
        /// Item label as shown in the menu.
        #[arg(long, conflicts_with = "dismiss")]
        label: Option<String>,
        /// Close the menu without picking an item.
        #[arg(long)]
        dismiss: bool,
    },
    /// Dump the pending move slot of the focused window (debug builds only).
    PendingMove,
    /// Dump the UI state tree of the focused window (debug builds only).
    UiState,
    /// Switch the active workspace by index — reproduces the user's workspace tab
    /// click, so it is debug-only (user action, not agent action).
    SwitchWorkspace {
        /// Workspace index (0-based).
        #[arg(long)]
        index: u64,
    },
    /// Switch the active tab of the focused pane by index — reproduces the user's
    /// tab click. Debug builds only.
    SwitchTab {
        /// Tab index within the focused pane (0-based).
        #[arg(long)]
        index: u64,
    },
    /// Close a workspace by index — reproduces the workspace context menu's
    /// "Close workspace", which touches the closed-items undo stack. Debug only.
    CloseWorkspace {
        /// Workspace index (0-based).
        #[arg(long)]
        index: u64,
    },
    /// Feed raw bytes straight into a surface's terminal parser, bypassing the
    /// PTY. Debug builds only — it fabricates output the shell never produced.
    FeedBytes {
        /// Target surface id (defaults to the current surface).
        #[arg(long)]
        surface: Option<u32>,
        /// Text to feed (UTF-8). Mutually exclusive with `--bytes`.
        #[arg(long)]
        text: Option<String>,
        /// Bytes to feed as an even-length hex string (e.g. `1b5b3141`).
        #[arg(long, conflicts_with = "text")]
        bytes: Option<String>,
    },
    /// Input injection — reproduces user keyboard/mouse input. Debug builds only
    /// (input reproduction has no release surface).
    #[command(subcommand)]
    Inject(InjectDebugCommands),
    /// Plugin-contributed banner force-open/close, bypassing the owning plugin.
    /// Debug builds only — `debug banner` is the host built-in banner instead.
    #[command(subcommand)]
    PluginBanner(PluginBannerDebugCommands),
    /// Reproduce a user navigation on an html surface: open an address as if
    /// typed into the address bar, or go back, forward, reload or stop.
    /// Debug builds only — it stands in for user navigation.
    WebviewHistory {
        /// Target html surface id (defaults to the current surface).
        #[arg(long)]
        surface: Option<u32>,
        /// One of open, back, forward, reload, stop.
        #[arg(long, value_parser = ["open", "back", "forward", "reload", "stop"])]
        action: String,
        /// Address for open (file://, http:// or https://).
        #[arg(long, required_if_eq("action", "open"))]
        url: Option<String>,
    },
    /// Show the native WebView overlay mode of the last frame and the popup
    /// boxes it covers, or force the backend support report (off restores it).
    /// Debug builds only — forcing a mode the backend lacks reproduces the
    /// fallback to hiding pages.
    WebviewOverlay {
        /// One of off, hide, opaque, transparent. Omit to only read the state.
        #[arg(long, value_parser = ["off", "hide", "opaque", "transparent"])]
        force: Option<String>,
    },
    /// Open a streaming channel and verify the server→client push path: send N
    /// data frames and expect each one echoed back (debug builds only).
    StreamEcho {
        /// Payload sent in each frame (an index suffix `#N` is appended).
        #[arg(long, default_value = "hello")]
        payload: String,
        /// Number of frames to send and expect echoed back.
        #[arg(long, default_value_t = 1)]
        count: u32,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum ToolDebugCommands {
    /// List all tool menu items in display order
    List,
    /// Invoke a tool menu item by key
    Invoke {
        /// Tool item key (`<plugin_id>/<tool_id>`, e.g. `com.tasty.clipboard-viewer/open-viewer`)
        #[arg(long)]
        key: String,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum PopupDebugCommands {
    /// List all popup contributes + currently open instances
    List,
    /// Open a plugin popup instance
    Open {
        /// Plugin id (e.g. "com.example.popper")
        #[arg(long)]
        plugin_id: String,
        /// Popup id within the plugin
        #[arg(long)]
        popup_id: String,
        /// Optional context JSON to send as the popup.open payload
        #[arg(long)]
        context: Option<String>,
    },
    /// Close a popup instance by id
    Close {
        /// Popup instance id returned by `popup open`
        #[arg(long)]
        instance_id: u64,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum HostPopupDebugCommands {
    /// List all host built-in popups (id + title key)
    List,
    /// Force-open a host popup by id, centered on the focused window
    Open {
        /// Host popup id (e.g. "tools_menu", "port_scanner", "command_palette")
        #[arg(long)]
        popup_id: String,
        /// Open it scoped to the active workspace instead of the window, so the
        /// scope visibility gate can be exercised (e.g. "dag_list").
        #[arg(long)]
        workspace_scope: bool,
        /// Open it scoped to the focused surface instead of the window, so the
        /// surface scrim and the surface boundary inset can be exercised.
        #[arg(long)]
        surface_scope: bool,
    },
    /// Close a host popup by id
    Close {
        /// Host popup id
        #[arg(long)]
        popup_id: String,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum FullscreenDebugCommands {
    /// List all registered fullscreen stages (id + title key)
    List,
    /// Put a stage on a window. A different stage already up is replaced; the
    /// same stage is a no-op.
    Open {
        /// Stage id (e.g. "blank", "notifications")
        #[arg(long)]
        stage_id: String,
        /// Target window. Omit only when exactly one window is open — with
        /// several, the call errors instead of guessing the focused one.
        #[arg(long)]
        window_id: Option<u64>,
    },
    /// Take the active stage off a window
    Close {
        /// Target window (see `open --window-id`)
        #[arg(long)]
        window_id: Option<u64>,
    },
    /// Dump the active stage id plus the window's fullscreen state (OS
    /// fullscreen / maximized / inner size / covered monitor)
    State {
        /// Target window (see `open --window-id`)
        #[arg(long)]
        window_id: Option<u64>,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum ModifierHintDebugCommands {
    /// Inject a modifier hold (no flags = release). `--elapsed-ms` backdates the
    /// hold timer to instantly pass the reveal-delay gate.
    Hold {
        /// Hold the ctrl axis.
        #[arg(long)]
        ctrl: bool,
        /// Hold the `alt` token — the physical key differs by platform: Command on macOS, Alt elsewhere.
        #[arg(long)]
        alt: bool,
        /// Hold the `option` token — the physical key differs by platform: Option on macOS, Win on Windows, Super elsewhere.
        #[arg(long)]
        option: bool,
        /// Hold the shift axis.
        #[arg(long)]
        shift: bool,
        /// Backdate the hold timer by this many ms (skip the reveal delay).
        #[arg(long)]
        elapsed_ms: Option<u64>,
    },
    /// Dump the overlay render state (held / delay / alpha / visible / sections / header).
    State,
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum BannerDebugCommands {
    /// List built-in banner definitions + current shown/queued state
    List,
    /// Fire a banner by id into a scope
    Show {
        /// Banner id (e.g. "mouse-capture")
        #[arg(long)]
        banner_id: String,
        /// Target scope token: view | workspace:<i> | pane:<id> | tab:<pane>:<i> | surface:<id>
        #[arg(long)]
        scope: String,
    },
    /// Close a banner by id (promotes the queue head if it was shown)
    Close {
        /// Banner id
        #[arg(long)]
        banner_id: String,
    },
    /// Force the remaining countdown of a shown TTL banner in a scope
    SetCountdown {
        /// Target scope token (same format as `show --scope`)
        #[arg(long)]
        scope: String,
        /// New remaining seconds
        #[arg(long)]
        seconds: u32,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum LuaDebugCommands {
    /// Inject arbitrary Lua source and run it in the host worker (fire-and-forget).
    /// Effects are observable via logs (e.g. `tasty.log`); deadline-exceeding
    /// sources are aborted by the worker.
    Eval {
        /// Lua source to execute. e.g. 'tasty.log("hi from debug")'
        #[arg()]
        source: String,
    },
    /// Read Lua source from a file and inject it (same worker path as `eval`).
    EvalFile {
        /// Path to a `.lua` file to read and execute.
        #[arg()]
        path: String,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum ModalDebugCommands {
    /// Send a close request to the active modal (no-op when none is up; the
    /// response distinguishes the two via `closed`).
    CloseRequest,
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum SettingsDebugCommands {
    /// Force-open the settings modal, optionally on a specific tab/subtab. If it is
    /// already open, switch its tab/subtab.
    Open {
        /// Initial L1 tab: general | terminal | appearance | keybindings |
        /// file_handler | misc | plugins (default: general).
        #[arg(long)]
        tab: Option<String>,
        /// Initial L2 section within the chosen tab (depends on --tab). e.g.
        /// appearance: theme|colors|general|display|tasty|terminal;
        /// keybindings: general|workspace|pane|tab|surface|clipboard|zoom|
        /// image|preset|plugins; general: general|notifications|accessibility;
        /// terminal: general|input|mouse_capture|tui|performance; file_handler: extension_mapping|
        /// detectors|handlers; misc: scripts|task_pipeline. Unknown keys keep the tab default.
        #[arg(long)]
        subtab: Option<String>,
    },
    /// Close the settings modal without saving (like Cancel). Other modals stay open.
    Close,
    /// Apply a (partial) settings patch at runtime: the JSON object is
    /// deep-merged onto the live settings, then dispatched as UpdateSettings
    /// (same path as saving the settings modal — persists to config.toml).
    Apply {
        /// Settings patch as a JSON object (partial allowed). e.g.
        /// '{"general":{"workspace_categories_enabled":false}}'
        #[arg(long, conflicts_with = "file")]
        json: Option<String>,
        /// Read the JSON patch from a file instead of --json.
        #[arg(long)]
        file: Option<String>,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum ExtensionDebugCommands {
    /// Fire an extension hook manually (sends extension.invoke_hook to the
    /// specified extension and returns the response).
    InvokeHook {
        /// Extension plugin id (must be installed and running).
        #[arg(long)]
        extension_id: String,
        /// Hook kind: "event" or "ipc".
        #[arg(long)]
        kind: String,
        /// Hook phase: "pre" or "post".
        #[arg(long)]
        phase: String,
        /// Hook mode: "transform", "filter", or "observe".
        #[arg(long)]
        mode: String,
        /// Target: event key (e.g. "foo.bar") or IPC method (e.g. "codex.spawn").
        #[arg(long)]
        target: String,
        /// JSON payload to pass as the hook input (default: `{}`).
        #[arg(long, default_value = "{}")]
        payload: String,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum EventBusCommands {
    /// List plugins subscribing to the given event key
    ListSubscribers {
        /// Event key (e.g. "surface.closed")
        #[arg()]
        key: String,
    },
    /// Publish an arbitrary event from the host side
    Publish {
        /// Event key
        #[arg()]
        key: String,
        /// JSON payload (default: `{}`)
        #[arg(long, default_value = "{}")]
        payload: String,
        /// Event scope: "system" (default) or "surface"
        #[arg(long, default_value = "system")]
        scope: String,
    },
    /// Print recent envelopes with the given trace_id
    Trace {
        /// trace_id (e.g. "h2a")
        #[arg()]
        trace_id: String,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum InjectDebugCommands {
    /// Inject key bytes into a surface's input path.
    Key {
        /// Target surface id (defaults to the current surface).
        #[arg(long)]
        surface: Option<u32>,
        /// Text to send (UTF-8). Mutually exclusive with `--bytes`.
        #[arg(long)]
        text: Option<String>,
        /// Bytes as an even-length hex string (e.g. `1b5b41` for Up).
        #[arg(long, conflicts_with = "text")]
        bytes: Option<String>,
    },
    /// Inject a terminal-cell mouse event (row/column addressed).
    Mouse {
        /// Target surface id (defaults to the current surface).
        #[arg(long)]
        surface: Option<u32>,
        /// Cell row (0-based).
        #[arg(long)]
        row: u32,
        /// Cell column (0-based).
        #[arg(long)]
        col: u32,
        /// One of `move`, `press`, `release`.
        #[arg(long, default_value = "move")]
        event_type: String,
        /// 0 = left, 1 = middle, 2 = right.
        #[arg(long, default_value_t = 0)]
        button: u64,
    },
    /// Inject a pointer event into the window mesh, addressed by normalized
    /// surface-local coordinates.
    WindowMouse {
        /// Target surface id.
        #[arg(long)]
        surface: Option<u32>,
        /// Surface-local x in [0,1].
        #[arg(long, default_value_t = 0.5)]
        fx: f64,
        /// Surface-local y in [0,1].
        #[arg(long, default_value_t = 0.5)]
        fy: f64,
        /// One of `move`, `press`, `release`, `scroll`.
        #[arg(long, default_value = "move")]
        event_type: String,
        /// 0 = left, 1 = middle, 2 = right.
        #[arg(long, default_value_t = 0)]
        button: u64,
        /// Scroll unit: `line` (wheel notches) or `point` (physical pixels at this level).
        /// `page` has no winit delta and is refused.
        #[arg(long, default_value = "line", value_parser = ["line", "point"])]
        unit: String,
        /// Horizontal scroll delta (with `--event-type scroll`).
        #[arg(long, default_value_t = 0.0)]
        scroll_dx: f64,
        /// Vertical scroll delta (with `--event-type scroll`).
        #[arg(long, default_value_t = 0.0)]
        scroll_dy: f64,
    },
    /// Inject a pointer event into the egui layer (chrome: tab bar, sidebar,
    /// popups) rather than the terminal mesh.
    EguiMouse {
        /// Target surface id.
        #[arg(long)]
        surface: Option<u32>,
        /// Surface-local x in [0,1].
        #[arg(long, default_value_t = 0.5)]
        fx: f64,
        /// Surface-local y in [0,1].
        #[arg(long, default_value_t = 0.5)]
        fy: f64,
        /// One of `move`, `press`, `release`, `scroll`.
        #[arg(long, default_value = "move")]
        event_type: String,
        /// 0 = left, 1 = middle, 2 = right.
        #[arg(long, default_value_t = 0)]
        button: u64,
        /// Scroll unit: `line` (wheel notches), `point` (logical points), or `page`.
        #[arg(long, default_value = "line", value_parser = ["line", "point", "page"])]
        unit: String,
        /// Horizontal scroll delta (with `--event-type scroll`).
        #[arg(long, default_value_t = 0.0)]
        scroll_dx: f64,
        /// Vertical scroll delta (with `--event-type scroll`).
        #[arg(long, default_value_t = 0.0)]
        scroll_dy: f64,
        /// Modifier keys egui treats as held from this event on (comma-separated
        /// `shift`, `ctrl`, `alt`, `command`, or `none` to release; omit to keep them).
        #[arg(long, value_delimiter = ',')]
        modifiers: Option<Vec<String>>,
        /// Target window, main or auxiliary (Settings, Preset, Plugins). Then fx/fy are relative to that whole window and --surface is refused
        #[arg(long, conflicts_with = "surface")]
        window_id: Option<u64>,
    },
    /// Inject a key event into the egui layer.
    EguiKey {
        /// egui key name (e.g. `Enter`, `Escape`, `ArrowDown`).
        #[arg(long)]
        key: String,
        /// Press (default) or release with `--pressed false`. A bare `--pressed` means true.
        #[arg(
            long,
            default_value_t = true,
            num_args = 0..=1,
            default_missing_value = "true",
            action = clap::ArgAction::Set
        )]
        pressed: bool,
        /// Target window, main or auxiliary (Settings, Preset, Plugins). Omit to send to the focused main window
        #[arg(long)]
        window_id: Option<u64>,
    },
    /// Type characters into a focused egui text field in a GUI build without
    /// physical keyboard input. Enter, Tab, and Backspace use egui-key instead.
    EguiText {
        /// Characters to type. Control characters are refused, since the real
        /// input path never carries them and the text field drops them.
        #[arg(long)]
        text: String,
        /// Target window, main or auxiliary (Settings, Preset, Plugins). Omit to type into the focused main window
        #[arg(long)]
        window_id: Option<u64>,
    },
}

#[cfg(debug_assertions)]
#[derive(Subcommand)]
pub enum PluginBannerDebugCommands {
    /// Force-open a plugin-contributed banner on a surface, skipping the
    /// ownership check that a real plugin caller would go through.
    Open {
        /// Banner id declared by the owning plugin.
        #[arg(long)]
        banner_id: String,
        /// Target surface id (defaults to the current surface).
        #[arg(long)]
        surface: Option<u32>,
    },
    /// Close a plugin banner instance by id.
    Close {
        /// Banner instance id returned by `plugin-banner open`.
        #[arg(long)]
        instance_id: u64,
    },
}
