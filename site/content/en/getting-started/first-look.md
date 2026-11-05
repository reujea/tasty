<!-- source-hash: a8a66164ab60 -->
# A first look

Choose a workspace in the sidebar and use its terminals in the work area. Each pane has a tab bar, and each tab contains surfaces for terminals or documents.

## Learn directly on the screen

Open **Tools** > **Tutorial…** in the sidebar to explore the window structure and practise splitting.

Splitting practice creates a separate workspace. Click the work area, then use the displayed shortcut or menu. Once the action is confirmed, choose **Next**. The practice workspace stays open afterward; keep using it or close it yourself.

Choose **Topics** or press `Esc` to stop, and **Resume** to continue later. Completion and progress survive restarting the app. Resuming splitting practice creates a new practice workspace. Shortcut hints follow your current settings.

## The four terms — Workspace · Pane · Tab · Surface

These parts of the screen fit inside one another in this order. If it is confusing at first, open **Tools** > **Tutorial…** in the sidebar and follow the guidance drawn directly on the screen.

| Term | Meaning | Similar concept |
|------|----|------------|
| **Workspace** | The largest unit, holding all Tabs · Panes · Surfaces. You switch between them in the sidebar | tmux session, iTerm2 window |
| **Pane** | A physical division of a Workspace. Each Pane has **its own tab strip** | A Tasty-specific concept with no counterpart in tmux · iTerm2 |
| **Tab** | One tab inside a Pane | tmux window, iTerm2 tab |
| **Surface** | The area where a terminal · Markdown · explorer is actually drawn. One Tab can be split into several | tmux · iTerm2 pane |

What sets Tasty apart is that **splitting happens on two levels**.

- **Pane split** — Divides the Workspace left/right or top/bottom. Switching Tabs in one Pane leaves the other Panes untouched. Example: the left Pane is dedicated to Claude Code, the right Pane holds several log · build Tabs.
- **Surface split** — Divides the inside of a single Tab. Switching Tabs swaps the whole split with it.

For detailed operation see [Panes, tabs, and splits](../using/panes-tabs-splits.md).

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
- **Work area** — Where you arrange terminals and documents, using the four components below.
- **Status bar** — The state of the current terminal and quick buttons.

## Sidebar

Three areas, top to bottom.

- **Header** — The `tasty.` wordmark and the **Collapse** button. Collapsed, it becomes a narrow rail showing only icons.
- **WORKSPACES** list — Click a card to switch, drag to reorder. Add with the **New workspace** button at the bottom. Right-click the button for **Create workspace from preset...** and **Add remote workspace**. There are two kinds of number badge on the right of a card: a yellow badge counts the Surfaces waiting for input, and a blue badge counts the Surfaces whose work finished or that received a notification. When both are shown, the yellow badge is on the left.
- **Bottom buttons** — **Tools** (**Command palette…** · **Listening ports…** · **Remote connections…** · **Presets** · **Tutorial…** · **Task DAGs** · **Open File…** in that order, followed by items added by plugins below a separator), **Plugins** (the install · enable management window), **Settings** (the settings window).

When you have many Workspaces, turn on **Settings** > **General** > **Workspace categories (folders)** to group them like folders. Details in [Workspaces](../using/workspaces.md).

## Tab strip

At the top of each Pane.

- Each Tab shows a type icon (terminal · Markdown, etc.) and a name. The name is chosen in this order: a name you set → the terminal title set by the program → the name of the shell's current directory → the default name. Once you set a name with right-click > **Rename Tab**, it no longer changes.
- A **green dot** appears while a program is producing output, and the name changes colour when it waits for input or finishes ([Names and marks](../using/panes-tabs-splits.md#names-and-marks)).
- `+` is a new Tab. Right-click to create from a preset. The split · search icons next to it split this Pane or search inside the current Surface.
- When Tabs overflow, left/right arrows appear. Drag to reorder.
- Right-click an empty spot to create a new Tab with **New Terminal** · **New Markdown...** · **New HTML...** · **New Image**. **New HTML...** opens a Tab in that Pane once you pick an HTML or SVG file in the file chooser. The same menu also has **Save as pane preset**, **Create tab from preset...** and **Create pane from preset...**. Each type is described in [Managing and viewing files](../using/files.md).

The last remaining Tab of a Pane cannot be closed with Close Tab. Closing a Surface is different: closing the last Surface in a Tab closes that Tab, and if it was the last Tab of its Pane the Pane closes too, and if that was the last Pane of the Workspace the Workspace closes as well.

<a id="status-bar"></a>

<details>
<summary>Status bar</summary>

The single line at the bottom of the work area. The left side shows information about the currently focused Surface, the right side holds buttons.

- **git branch** — A branch glyph and the branch name when the shell's current directory is a git repository. Reflected with a delay of up to 1 second. Outside a repository the item is **not there at all**. When you are on a specific commit (detached HEAD) you get `@` and a short commit id instead of a name.
- **Surface ID** — Like `s3·p1`: the Surface number and the Pane holding it. The first number is what you use to target this terminal with the `tasty` CLI ([tasty CLI](../agents/cli.md)).
- **Shell** — The name of the foreground program.
- **Grid size** — `columns×rows`.
- **Palette shortcut** — Drawn as keycaps. Click to open the command palette; the keys shown follow your settings. Clear the shortcut and the item goes away too.
- **Theme icon** — Each click toggles between the light theme (latte) and the dark theme (mocha). The icon shows which one you are on — a sun for the light theme.

**An item with no value disappears rather than leaving a gap.** As the window narrows, the grid size goes first, then the shell, the Surface ID, the palette shortcut and finally the branch name; the theme icon stays to the end.

</details>

## Shortcuts to know first

These are the values of the default preset (**Tasty**). In the notation, `Alt` is `Cmd(⌘)` on macOS and `Ctrl` is `Control(⌃)`. How to change them and the full table are in [Keybindings](../customize/keybindings.md).

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| New Workspace | `Alt+N` | `Cmd+N` |
| New Tab | `Alt+T` | `Cmd+T` |
| Pane split (left/right / top/bottom) | `Alt+E` / `Alt+Shift+E` | `Cmd+E` / `Cmd+Shift+E` |
| Surface split (left/right / top/bottom) | `Alt+D` / `Alt+Shift+D` | `Cmd+D` / `Cmd+Shift+D` |
| Command palette | `Ctrl+Shift+P` | `Ctrl+Shift+P` |
| Settings | `Ctrl+,` | `Ctrl+,` |

If you want iTerm2-style `⌘`-centric combinations on macOS, pick the **Mac** preset under **Settings** > **Keybindings**.

## When you close the window

With several windows open, the close button closes only that window. On the last remaining window, the close button asks whether to **Quit** or **Minimize**. If you choose Minimize, Tasty goes into the system tray (menu bar) and comes back via **Show Window** in the tray icon's menu. To stop being asked every time, change **Settings** > **General** > **Close behavior**.

Once you confirm quitting, web pages and Markdown documents in all windows are hidden so the progress screen can be shown while shutdown is pending. If you cancel the confirmation or return after minimizing to the background, you can continue using the documents.

Tasty can [restore](../using/workspaces.md#restoring-after-a-restart) layouts and terminal history when it starts again. This differs from restoring running processes and depends on your settings and the program’s session resume support.

<a id="what-to-read-next"></a>

## Keep exploring

- [Workspaces](../using/workspaces.md) — Names · categories · switching · closing.
- [Working in the terminal](../using/terminal.md) — Copy/paste, search, mouse capture.
- [tasty CLI](../agents/cli.md) — Send commands to a terminal and read its output using the Surface ID from the status bar.
