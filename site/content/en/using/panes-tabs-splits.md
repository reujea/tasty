<!-- source-hash: c96d2989776a -->
# Panes · Tabs · splits

Keep an agent, your code, and the logs side by side. Divide the window with panes and surfaces, switch tasks with tabs, and save layouts you use often as presets. [A first look](../getting-started/first-look.md) explains how the parts fit together.

The shortcuts below use the default **Tasty** preset. On macOS, read `Alt` as `Cmd` and `Ctrl` as `Control`.

## Two levels of splitting — choose first

| | Pane split | Surface split |
|--|-----------|-------------|
| What it divides | The Workspace | The inside of one Tab |
| When you switch Tabs | Other Panes stay as they are | The whole split changes together |
| Tab strip | One per Pane | Shared by one Tab |
| Side by side / top and bottom | `Alt+E` / `Alt+Shift+E` | `Alt+D` / `Alt+Shift+D` |
| Focus next / previous | `Ctrl+]` / `Ctrl+[` | `Alt+]` / `Alt+[` |
| Close | `Ctrl+Shift+W` | `Alt+W` |

- To keep an agent fixed on one side while moving between several Tabs on the other, split the **Pane**.
- To keep the log · editor belonging to one task side by side and switch them as a whole Tab, split the **Surface**.

The terminal in the new cell starts in the working directory of the original cell (**Settings** > **Terminal** > **Inherit working directory**). Drag the border between cells to change the ratio.

The split icon on the right of the tab strip divides that Pane side by side. To split into a different kind of Surface, use the CLI.

```sh
tasty split --level pane --target-pane 1 --direction horizontal
tasty split --level surface --target-surface 5 --type markdown --file notes.md
tasty split --level surface --target-surface 5 --type explorer --path ~/proj
```

## Tabs

### Creating · closing

- The `+` button or `Alt+T` — a new terminal Tab.
- Right-click `+` — **Create tab from preset...** / **Create pane from preset...**.
- Right-click an empty area of the tab strip — **New Terminal** · **New Markdown...** · **New Explorer** · **New HTML...** · **New Image**. Each kind is described in [Opening files](files.md).
- The Tab's `×`, right-click the Tab > **Close Tab**, or `Ctrl+W`. The last remaining Tab does not close — instead `Ctrl+W` closes the Pane.

```sh
tasty list panes
tasty new tab --pane 1 --cwd ~/proj
tasty new tab --pane 1 --type html --url https://example.com
tasty list tabs --pane 1
tasty close tab --tab 7
```

### Switching · order

- Click, or `Ctrl+1` to `Ctrl+9`, `Ctrl+0` (the 10th). Hold `Ctrl` and numbers appear on the Tabs.
- `Ctrl+L` / `Ctrl+H` — next / previous Tab. The **Next tab** / **Previous tab** shortcuts are empty by default so they do not collide with the OS.
- Drag to reorder. Right-click a Tab > **Move Left** / **Move Right** also works.
- When Tabs overflow, left and right arrows appear. Switching with a shortcut scrolls automatically so the active Tab is visible.

### Names and marks

A Tab's name is decided in this order of priority — a name you set yourself > the window title set by the program > the shell's current directory.

- Right-click a Tab > **Rename Tab**, or `F2`. Once set, it does not change when you move directories.
- **Green dot** — a program other than the shell is producing output. It disappears while waiting at a prompt or when the program is idle.
- **Yellow name** — the program is waiting for input. **Blue name** — a job finished. Click that Surface and it returns to its normal color.

Tab width and font size have no entry in the settings window; change them with `tab_width` · `tab_font_size` under `[appearance]` in `~/.tasty/config.toml`.

### Tabs not visible after a restart

When the layout is restored, the shells of Tabs not visible on screen are not launched right away; they start when you first open that Tab. They appear in `tasty list surfaces` as `pty_ready: false`; to launch one ahead of time without opening it, `tasty wake --surface <ID>`.

## Moving Surfaces

Moves a live Surface to another Tab · Pane · Workspace. For a terminal, the running program and the scrollback go with it.

1. Right-click an empty area of the Surface to move > **Cut**.
2. Right-click an empty area of the destination Surface > **Move Here**.

The destination Surface is closed and the moved Surface takes its place. Create an empty terminal first and use it as the destination. Over a program that captures the mouse (vim and so on), use `Shift+right-click`.

## Changing the kind

Press `Alt+'` and the **Surface Type** popup appears. It changes the current Surface, in place, to **Terminal** · **Explorer** · **Markdown** · **HTML...** · **Image** · **DAG**. Choose with the arrow keys and `Enter`, or with the first letter. The button in the middle of an empty Surface is the same popup.

- Changing to Explorer opens the terminal's current directory as the root.
- Markdown · HTML ask for a file path · URL.
- The previous content is lost. Going terminal → Explorer → terminal starts a new shell session.
- While the popup is open only **that one Surface** dims. Whatever is running in the pane beside it, the sidebar and the status bar all stay readable.

**Convert to Markdown** · **Convert to Explorer** are shortcuts that change the kind directly without the popup; they are empty by default.

## Fullscreen

Press the fullscreen button on the title bar of the notification window (tooltip **Show fullscreen**) and the Tasty window covers the monitor, showing only the notification list at a large size. Leave with `Esc` or the exit button at the top right. When you leave, the window size and the terminal grid are exactly as they were before entering. Fullscreen view is currently available for the notification list.

## Saving layouts — presets

Save a layout you use often as a preset and pull it out when creating a new Workspace · Tab · Pane. What is saved is the split structure and each cell's kind · working directory · startup command · file path · URL.

### Saving

- Workspace — right-click a sidebar card > **Save as workspace preset**.
- Tab — right-click a Tab > **Save as tab preset**.
- Pane — right-click an empty area of the tab strip > **Save as pane preset**.

The file is created at `~/.tasty/presets/{workspace,tab,pane}/<name>.toml`. If the name already exists, a number such as `-2` is appended.

### Editing

Open **Tools** > **Presets** in the sidebar and the **Layout Presets** window appears.

- Choose **Workspace** / **Tab** / **Pane** with the tabs at the top, and pick a preset from the list on the left.
- A structure preview is shown on the right. The toolbar has **Rename** · **Duplicate** · **Delete**.
- Press **Edit** to change the structure directly on the preview — click the strip that appears when you hover a cell's edge to split in that direction, click the middle of a cell to select it and remove it with the trash handle at its top right, and add and remove Tabs with `+` / `×`. The same split · close shortcuts as in the main window also work on the selected cell.
- Structure changes are written to the file immediately, with no save button, and **saved automatically** is shown. Leave with **Done**.
- A cell's kind · working directory · startup command are changed on the **Surface settings** screen. Press the gear handle of the selected cell, or double-click the cell, and the whole right-hand area becomes that cell's settings screen. Every field is visible however small the cell is.
  - Once you change a value, **unsaved** appears at the top, and nothing is saved until you press **OK** at the bottom. **Cancel** or `Esc` discards the changes. Pressing `Enter` in a field also confirms.
  - While the settings screen is open, the list on the left and the tabs at the top are dimmed and cannot be clicked. After OK or Cancel you are back on the preview, with that cell still selected.
- **New preset** creates an empty preset with a single terminal.

### Applying

- Right-click the **New Workspace** button > **Create workspace from preset...**.
- Right-click the tab strip `+` > **Create tab from preset...** / **Create pane from preset...**.
- The three shortcuts such as **Apply workspace preset** are empty by default. Once assigned, they open a selection popup.

If a terminal cell has a startup command, one line is entered as soon as the shell starts.

```sh
tasty preset list --kind workspace
tasty preset capture --kind workspace --source-id 2 --name dev
tasty preset apply --kind workspace --name dev
tasty preset apply --kind tab --name logs --target-pane 1
```

Applying from the CLI does not move focus. Automatic restore on restart and restoring closed items are covered in [Workspaces](workspaces.md#restoring-after-a-restart).
