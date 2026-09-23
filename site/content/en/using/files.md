<!-- source-hash: f5c3454989df -->
# Opening files

Read a README or check an image beside your terminal. Tasty can open an Explorer, Markdown documents, images, and HTML pages, with a Git viewer for reviewing changes.

## Ways to open

| Where | How |
|--------|--------|
| Sidebar **Tools** > **Open File…** | Tasty's own file chooser. It opens in the folder of the terminal or Explorer you are looking at, and in a remote Workspace it shows remote files |
| Drag a file onto the window | Drop when **Drop to open** appears |
| Explorer Surface | Double-click a file |
| Terminal | `Ctrl+click`, or select a path and right-click > **Open File** ([Working with the terminal](terminal.md#opening-links-and-paths)) |
| Right-click an empty area of the tab strip | **New Markdown...** · **New Explorer** · **New HTML...** · **New Image** |
| `Alt+'` | Changes the current Surface to another kind ([Changing the kind](panes-tabs-splits.md#changing-the-kind)) |
| CLI | `tasty new tab --pane 1 --type markdown --file README.md` and so on |

The file extension decides which Surface opens it — `.md` is Markdown, image files are image, `.html` is HTML, and a folder is Explorer. When nothing settles which one opens it, the **Open file with…** window appears so you can choose, and your choice is kept under **Recent**. Extension mappings and handlers are changed in the **Settings** > **Handler** tab.

That window shows the file path and the format it recognised at the top, and below it one row per handler you can pick — an icon, a name, and where it came from (**built-in** · **you** · **plugin**). Anything you used before is gathered under **Recent** with when you used it — just now, minutes ago, hours ago, yesterday, then days ago, and **past a week it becomes a date** like `2026-09-13` (by then what you notice is when it was, not how many days back). Leave the window open across midnight and the wording follows. One click selects, a double click opens straight away. The choice is used **once** — nothing is registered for the format, and the same window appears next time you open that file. To settle on a handler, use the **Settings** > **Handler** tab. Close the window with `Esc` or **Cancel**.

A Surface opened from a file is split · moved · closed · restored on restart just like any other Surface. The Tab name becomes the file name.

In the file chooser, long file names end with `…` so they do not overlap the size and modification date. This only changes the display; selecting or opening an entry still uses its full name.

In the list, a single click selects the row, file or folder; a double click enters a folder or opens a file. Pressing **Open** with a folder selected enters that folder — the way down by keyboard. When choosing where to save, a folder cannot be the target, so selecting one leaves the name field as it is and shows a grey line below saying so. When saving, double-clicking a file only fills in its name and does not save — this leaves you the chance to read the overwrite warning.

A file you open yourself, by any of the ways above, opens in a new tab and **switches to it.** When an AI agent opens one for you, the tab is added and the tab you were on stays selected, so the screen you are working on does not change under you.

When an opening request specifies its source surface, switching windows while it is pending keeps the result in a new tab in that surface’s pane. Opening a file from the Explorer while you are looking at another pane still puts it in the Explorer’s pane. Closing the source does not open the file in another window.

## Explorer

The file manager built into Tasty. Change a terminal with `Alt+'` > **Explorer** and it opens that terminal's current directory as the root. `tasty new tab --pane 1 --type explorer --path ~/proj` also opens one.

### Screen

- **Left** — the **Files** tree (fixed at the root) and **Favorites** under it.
- **Top** — **Back** · **Forward** · **Up** · **Refresh**, the address bar, and the view switch (**Grid** · **List** · **Detail**).
- **Right** — the items in the current folder. `..` at the top goes to the parent folder. In Detail view, click the **Name** · **Size** · **Modified** · **Type** column headers to sort.
- You can keep several **New tab**s inside a Surface and view folders separately. These are separate from the Pane's Tabs.

Click the address bar to type a path directly; recently visited folders appear as autocompletion. Go with `Enter` or **Go**. The left tree stays fixed at the root, but the right list can go anywhere.

### Shortcuts (when the Explorer has focus)

| Action | Shortcut |
|---------|--------|
| Refresh | `F5` |
| Go to parent folder | `Alt+↑` |
| Select all | `Ctrl+A` · `Alt+A` |
| Copy path | `Alt+Shift+C` |
| Copy / cut / paste | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` (the same bindings as the terminal) |

Click to select, `Ctrl+click` to add, `Shift+click` to select a range.

### Right-click menu

- **Copy Path** — with several selected, they are joined with line breaks.
- **Copy** · **Cut** · **Paste** · **Paste (into)** — if the name already exists, `(copy)` is appended.
- **Move to Trash** — sends to the OS trash without confirmation. To undo, use the trash.
- **Rename**.
- **Open in System** — opens the folder in the OS file manager.
- **Open in New Tab** — opens one more Explorer Tab in the Pane with that folder as the root.
- **Set as Root** — moves the root of the left tree.
- **Add to Favorites** — gives it a name and puts it in the list at the bottom left. Favorites are shared by all Explorers and saved in `~/.tasty/explorer-favorites.toml`.

The last chosen view mode is remembered and applied to new Explorers too. The Explorer font is set separately in the **Explorer** item under **Settings** > **Appearance**. An Explorer in a remote Workspace is browse-only, and items that change files do not appear.

## Markdown

Renders `.md` files. When the file changes, it redraws automatically within 1 second.

- Tables · checkboxes · footnotes · code highlighting · `mermaid` diagrams · `$…$` math · `> [!NOTE]` callouts (including Obsidian-style `[!tip]-` folding) · frontmatter at the top hidden.
- If there are headings, a collapsible **Table of contents** is attached above the body. Clicking an entry moves down to that heading within the document you are already reading — and clicking the same entry again moves there again.
- Hover a code block for the **Copy code** button.
- `Ctrl+F` (macOS `Cmd+F`) while the document has focus — **Find in document**.
- Type another file path in the address bar at the top to move to it. Recently opened files appear as autocompletion. Files opened in other windows of the same Tasty instance are included, with up to 10 entries in most recently opened order.
- Links in the document — other Markdown · files open in a new Tab in the same Pane, and on Linux Tasty switches to it (on macOS the new Tab is only added behind the current one), and `http(s)://` goes to the browser. Relative paths are relative to the folder the document is in. Links that point inside the same document, such as `#heading`, and footnote numbers and their back arrows only move to that spot.
- Files over 1MB are asked about once with **Open large file?**.

**New Markdown...** or `Alt+'` > **Markdown** opens the **Open Markdown File** window. Type a path or choose one with **Browse…**. Browse starts in the current folder of the terminal you are looking at. The window appears centered over that surface, and it hides while you switch to another workspace or tab and comes back as you left it when you return. The file chooser opened by Browse hides and returns with it, keeping your selected files and current folder. While hidden, it does not block clicks or keyboard input on the other screen.

```sh
tasty markdown recent
tasty markdown reload --surface 5
```

## Image

View PNG, JPEG, and other images, or add drawings to them. **New Image** starts with an empty canvas. When the file changes outside Tasty, it is re-read automatically within 1 second — except **while you are editing**, where it is deferred until you leave edit mode (so the picture underneath your strokes does not change).

- Toolbar — **Previous image** / **Next image** (within the same folder), **Refresh**, **Edit**, **New image**, zoom **Fit** / `+` / `-`.
- Press **Edit** to choose **Brush** · **Color** and draw on top. Undo and redo with `Ctrl+Z` / `Ctrl+Shift+Z`. **Save** writes a PNG.
- `tasty image paste --surface <ID>` pastes the clipboard image as a floating selection.
- `tasty image list` shows every open image **across all windows**.
- `tasty image reload --surface <ID>` re-reads the file right now instead of waiting.

```sh
tasty image list
tasty image open --surface 5 shot.png
tasty image export --surface 5 out.png
```

## HTML

Shows a local HTML file or a URL in the OS web view. Press **New HTML...** and type a URL or file path in the **Open HTML** window.

- While loading, **Loading…** is shown; on failure, **Failed to load** and the URL.
- The **HTML** item under **Settings** > **Appearance** — **Default zoom** · **Color scheme** (**Follow theme** / light / dark) · **Allow remote content** · **Sandbox scripts**. Remote content and scripts are blocked by default. It is meant for viewing previews built locally, so to open external sites you need to turn **Allow remote content** on and **Sandbox scripts** off.

```sh
tasty html open --surface 5 ./dist/index.html
tasty new tab --pane 1 --type html --url http://localhost:3000
```

## Git view

Press **Tools** > **Git** in the sidebar and a window appears showing the repository of the current terminal directory, read-only.

- **Changes** (status) · **Commits** (log) · **Diff** when you click a file.
- Pick another worktree from the **Worktrees** list on the left to view relative to it. The actual checkout does not change.
- **Refresh** picks up external changes. There are no writes such as commit · staging.
- The **Open Git Viewer** shortcut is assigned under **Settings** > **Keybindings** > **Plugins** (no default).

The branch name in the status bar is always shown, independently of this window ([A first look](../getting-started/first-look.md#status-bar)).

## Empty Surface

A slot whose file was closed or whose kind has not been chosen yet remains **Empty**, with a kind-selection button in the middle. Pressing it is the same as the [Changing the kind](panes-tabs-splits.md#changing-the-kind) popup.

The Explorer is built into Tasty; Markdown · image · HTML · Git view are bundled plugins. If you disable a plugin, you cannot open that kind — [Plugins](../plugins/index.md).
