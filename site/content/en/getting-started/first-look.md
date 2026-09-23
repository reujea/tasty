<!-- source-hash: 608378c88d68 -->
# A first look

Start by looking at the sidebar and work area. Learn how workspaces organize your projects and how panes, tabs, and surfaces fit together, then arrange them to suit your work.

## Learn directly on the screen

Open **Tools** > **Tutorial** in the sidebar for screen structure, splitting practice, and finding commands and shortcuts. Learn how each pane has its own tab bar and each tab has its own surface layout.

Splitting practice creates a separate workspace. Click the work area, then use the displayed shortcut or menu. Once the action is confirmed, choose **Next**. The practice workspace stays open afterward; keep using it or close it yourself.

Choose **Topics** or press `Esc` to stop, and **Resume** to continue later. Completion and progress survive restarting the app. Resuming splitting practice creates a new practice workspace. Shortcut hints follow your current settings.

## Window layout

<!-- tasty-diagram: window-anatomy -->
```
┌─────────────────────────────────────────────────┐
│ Title bar                                       │
├──────────┬──────────────────────────────────────┤
│          │ Tab strip                            │
│ Sidebar  ├──────────────────────────────────────┤
│          │ Work area (Pane · Tab · Surface)     │
│          ├──────────────────────────────────────┤
│          │ Status bar                           │
└──────────┴──────────────────────────────────────┘
```

- **Title bar** — Drag to move the window, double-click to maximize. On Linux · Windows, Tasty draws the minimize · maximize · close buttons itself; on macOS the traffic-light buttons on the left are used as they are.
- **Sidebar** — The Workspace list and the entry points for tools · plugins · settings.
- **Work area** — Where the actual work happens. It is made up of the four terms below.
- **Status bar** — The state of the current terminal and quick buttons.

## The four terms — Workspace · Pane · Tab · Surface

Tasty's screen nests in this order. If it is confusing at first, open **Tools** > **Tutorial…** in the sidebar and follow the guidance drawn directly on the screen.

| Term | Meaning | Similar concept |
|------|----|------------|
| **Workspace** | The largest unit, holding all Tabs · Panes · Surfaces. You switch between them in the sidebar | tmux session, iTerm2 window |
| **Pane** | A physical division of a Workspace. Each Pane has **its own tab strip** | A Tasty-specific concept with no counterpart in tmux · iTerm2 |
| **Tab** | One tab inside a Pane | tmux window, iTerm2 tab |
| **Surface** | The area where a terminal · Markdown · explorer is actually drawn. One Tab can be split into several | tmux · iTerm2 pane |

What sets Tasty apart is that **splitting happens on two levels**.

- **Pane split** — Divides the Workspace left/right or top/bottom. Switching Tabs in one Pane leaves the other Panes untouched. Example: the left Pane is dedicated to Claude Code, the right Pane holds several log · build Tabs.
- **Surface split** — Divides the inside of a single Tab. Switching Tabs swaps the whole split with it.

For detailed operation see [Panes · Tabs · splits](../using/panes-tabs-splits.md).

## Sidebar

Three areas, top to bottom.

- **Header** — The `tasty.` wordmark and the **Collapse** button. Collapsed, it becomes a narrow rail showing only icons.
- **WORKSPACES** list — Click a card to switch, drag to reorder. Add with the **New workspace** button at the bottom (right-click to create from a preset). The number badge on the right of a card is how many Surfaces in that Workspace need your attention.
- **Bottom buttons** — **Tools** (command palette, listening ports, remote connections, presets, tutorial, open file, items added by plugins), **Plugins** (the install · enable management window), **Settings** (the settings window).

When you have many Workspaces, turn on **Settings** > **General** > **Workspace categories (folders)** to group them like folders. Details in [Workspaces](../using/workspaces.md).

## Tab strip

At the top of each Pane.

- Each Tab shows a type icon (terminal · Markdown, etc.) and a name. The name follows the shell's current directory and can be pinned with right-click > **Rename Tab**.
- A **green dot** appears while a program is producing output, and the name changes colour when it waits for input or finishes ([Names and marks](../using/panes-tabs-splits.md#names-and-marks)).
- `+` is a new Tab. Right-click to create from a preset. The split · search icons next to it split this Pane or search inside the current Surface.
- When Tabs overflow, left/right arrows appear. Drag to reorder.
- Right-click an empty spot to create a non-terminal Surface with **New Terminal** · **New Markdown...** · **New Explorer** · **New HTML...** · **New Image**. Each type is described in [Opening files](../using/files.md).

The last remaining Tab cannot be closed.

## Status bar

The single line at the bottom of the work area. The left side shows information about the currently focused Surface, the right side holds buttons.

- **git branch** — A branch glyph and the branch name when the shell's current directory is a git repository. Reflected with a delay of up to 1 second. Outside a repository the item is **not there at all**. When you are on a specific commit (detached HEAD) you get `@` and a short commit id instead of a name.
- **Surface ID** — Like `s3·p1`: the Surface number and the Pane holding it. The first number is what you use to target this terminal with the `tasty` CLI ([tasty CLI](../agents/cli.md)).
- **Shell** — The name of the foreground program.
- **Grid size** — `columns×rows`.
- **Palette shortcut** — Drawn as keycaps. Click to open the command palette; the keys shown follow your settings. Clear the shortcut and the item goes away too.
- **Theme icon** — Each click toggles between the light theme (latte) and the dark theme (mocha). The icon shows which one you are on — a sun for the light theme.

**An item with no value disappears rather than leaving a gap.** As the window narrows, the grid size goes first, then the shell, the Surface ID, the palette shortcut and finally the branch name; the theme icon stays to the end.

## Shortcuts to know first

These are the values of the default preset (**Tasty**). In the notation, `Alt` is `Cmd(⌘)` on macOS and `Ctrl` is `Control(⌃)`. How to change them and the full table are in [Keybindings](../customize/keybindings.md).

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| New Workspace | `Alt+N` | `Cmd+N` |
| New Tab | `Alt+T` | `Cmd+T` |
| Close current Tab · Surface | `Ctrl+W` | `Ctrl+W` |
| Restore closed item | `Ctrl+Shift+T` | `Ctrl+Shift+T` |
| Pane split (left/right / top/bottom) | `Alt+E` / `Alt+Shift+E` | `Cmd+E` / `Cmd+Shift+E` |
| Surface split (left/right / top/bottom) | `Alt+D` / `Alt+Shift+D` | `Cmd+D` / `Cmd+Shift+D` |
| Next · previous Pane | `Ctrl+]` / `Ctrl+[` | `Ctrl+]` / `Ctrl+[` |
| Next · previous Surface | `Alt+]` / `Alt+[` | `Cmd+]` / `Cmd+[` |
| Go to Tab n | `Ctrl+1` ~ `Ctrl+0` | `Ctrl+1` ~ `Ctrl+0` |
| Go to Workspace n | `Alt+1` ~ `Alt+9` | `Cmd+1` ~ `Cmd+9` |
| Collapse / hide sidebar | `Ctrl+B` / `Ctrl+Shift+B` | `Ctrl+B` / `Ctrl+Shift+B` |
| Notification panel | `Ctrl+Shift+I` | `Ctrl+Shift+I` |
| Command palette | `Ctrl+Shift+P` | `Ctrl+Shift+P` |
| Settings | `Ctrl+,` | `Ctrl+,` |
| New window | `Alt+Shift+N` | `Cmd+Shift+N` |

If you want iTerm2-style `⌘`-centric combinations on macOS, pick the **Mac** preset under **Settings** > **Keybindings**.

## When you close the window

Pressing the window's close button asks whether to **Quit** or **Minimize to background**. If you choose background, Tasty goes into the system tray (menu bar) and comes back via **Show Window** in the tray icon's menu. To stop being asked every time, change **Settings** > **General** > **Close behavior**.

Once you confirm quitting, web pages and Markdown documents in all windows are hidden so the progress screen can be shown while shutdown is pending. If you cancel the confirmation or return after minimizing to the background, you can continue using the documents.

When you launch Tasty again, the Workspace · Pane · Tab arrangement of the last window is restored as it was (**Settings** > **General** > **Restore layout on startup**, on by default). A [headless build](install.md#headless-build) of Tasty, which runs without a window, neither saves nor restores the arrangement, whatever this setting says — its workspaces last only for that run. If the setting is on, it logs a one-line warning about this at startup.

<a id="what-to-read-next"></a>

## Keep exploring

- [Workspaces](../using/workspaces.md) — Names · categories · switching · closing.
- [Working in the terminal](../using/terminal.md) — Copy/paste, search, mouse capture.
- [tasty CLI](../agents/cli.md) — Send commands to a terminal and read its output using the Surface ID from the status bar.
