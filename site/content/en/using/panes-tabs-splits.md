<!-- source-hash: b8f9a7869da6 -->
# Panes, tabs, and splits

Panes create areas whose tabs switch independently. Surface splits divide the content you view together within one tab. Choose either level to build the layout you need.

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
tasty split --level pane --pane 1 --direction horizontal
tasty split --level surface --surface 5 --type markdown --file notes.md
tasty split --level surface --surface 5 --type explorer --path ~/proj
```

## Tabs

### Creating · closing

- The `+` button or `Alt+T` — a new terminal Tab.
- Right-click `+` — **Create tab from preset...** / **Create pane from preset...**.
- Right-click an empty area of the tab strip — **New Terminal** · **New Markdown...** · **New HTML...** · **New Image** create a new Tab. **New HTML...** opens a Tab in that Pane once you pick an HTML or SVG file in the file chooser. The same menu also has **Save as pane preset**, **Create tab from preset...** and **Create pane from preset...**. Each kind is described in [Managing and viewing files](files.md).
- The Tab's `×`, right-click the Tab > **Close Tab**, or `Ctrl+W`. The last remaining Tab of a Pane does not close. To close the whole Pane, use Close Pane (`Ctrl+Shift+W`).

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
- **Green dot** — a program other than the shell is producing output. It disappears while waiting at a prompt or when the program is idle. Output that only changes the terminal title does not count, so the dot also disappears for a program that blinks its title while it waits for input (for example, Codex waiting on a question).
- **Yellow name** — the program is waiting for input. **Blue name** — a job finished. Click that Surface and it returns to its normal color.

Tab width and font size have no entry in the settings window; change them with `tab_width` · `tab_font_size` under `[appearance]` in `~/.tasty/config.toml`.

### Tabs not visible after a restart

When the layout is restored, the shells of Tabs not visible on screen are not launched right away; they start when you first open that Tab. They appear in `tasty list surfaces` as `pty_ready: false`; to launch one ahead of time without opening it, `tasty wake --surface <ID>`.

## Moving Surfaces

Moves a live Surface to another Tab · Pane · Workspace. For a terminal, the running program and the scrollback go with it.

1. Right-click an empty area of the Surface to move > **Move Surface**. A notice that the Surface was cut appears briefly over it.
2. Right-click an empty area of the destination Surface > **Move Surface Here**. This item appears only while a Surface is cut.

Only one cut item is remembered. Cutting something else replaces the earlier one.

The destination Surface is closed and the moved Surface takes its place. Create an empty terminal first and use it as the destination. Over a program that captures the mouse (vim and so on), use `Shift+right-click`.

## Moving Tabs

Moves one Tab, together with its splits, to another Pane or Workspace. For a terminal, the running program and the scrollback go with it.

1. Right-click the Tab to move > **Move Tab**. A notice that the Tab was cut appears briefly over its Pane.
2. Right-click the destination Tab > **Move Tab Here**. This item appears only while another Tab is cut.

The destination Tab is closed together with its Surfaces and the moved Tab takes its place. If the moved Tab was the last Tab of its Pane, that Pane closes; if it was in the last Pane of the Workspace, the Workspace closes. Tabs in a remote Workspace (one connected with attach) cannot be moved. A cut item can only be moved within the same window.

## Moving Panes

Moves one Pane, with all of its Tabs, to another split position or another Workspace. For a terminal, the running program and the scrollback go with it.

1. Right-click any Tab of the Pane to move > **Move Pane**. A notice that the Pane was cut appears briefly over it.
2. Right-click a Tab of the destination Pane > **Move Pane Here**. This item appears only while another Pane is cut.

The destination Pane is closed together with its Tabs, and the moved Pane takes its place at the same size. The neighboring Pane grows to fill the original spot. If the moved Pane was the last Pane of its Workspace, that Workspace closes. Panes in a remote Workspace cannot be moved, and a cut item can only be moved within the same window.

## Spotting what is cut

A cut Surface, Tab or Pane gets a pink dotted border on its inside. A Surface gets it around its own area, a Tab around its title cell, and a Pane around the whole Pane including its tab row. Every other border is a solid line, so the dots set it apart. On a Surface that also has a notification border, the original border shows between the dots.

When the cut item is not on screen, a move icon appears on whatever holds it.

- In a Tab that is not showing: after that Tab's title.
- In another Workspace: on that Workspace's row in the sidebar. With the sidebar collapsed, at the bottom-left of its avatar.

When a Pane has so many Tabs that its tab row scrolls and the cut Tab, or the Tab holding the cut Surface when that Tab is not the active one, is scrolled out of view, the tab row arrow on that side turns pink. When the cut Surface is in the active Tab, its outline is already on screen, so the arrow does not change. Once the Tab is fully inside the tab row, the arrow returns to its usual color and the mark on the Tab shows. Pressing the pink arrow still scrolls one step at a time.

The mark goes away when the move is done, when you cut something else, or when the cut item is closed. It stays while you switch Tabs or Workspaces, and it does not block clicks.

## Changing the kind

Press `Alt+'` and the **Surface Type** popup appears. It changes the current Surface, in place, to **Terminal** · **Explorer** · **Markdown** · **HTML...** · **Image** · **DAG**. Choose with the arrow keys and `Enter`, or with the letter shown at the right end of the row. A row without a letter is chosen with the arrow keys or a click. The current kind is not in the list. The button in the middle of an empty Surface is the same popup.

- Changing to Explorer opens the terminal's current directory as the root.
- Markdown · HTML ask for a file path · URL.
- The previous content is lost. Going terminal → Explorer → terminal starts a new shell session.
- While the popup is open only **that one Surface** dims. Whatever is running in the pane beside it, the sidebar and the status bar all stay readable.

**Convert to Markdown** · **Convert to Explorer** are shortcuts that change the kind directly without the popup; they are empty by default.

## Fullscreen

Press the fullscreen button on the title bar of the notification window (tooltip **Show fullscreen**) and the Tasty window covers the monitor, showing only the notification list at a large size. Leave with `Esc` or the exit button at the top right. When you leave, the window size and the terminal grid are exactly as they were before entering. Fullscreen view is currently available for the notification list.

## Saving layouts — presets

Save a layout you use often as a preset and pull it out when creating a new Workspace · Tab · Pane. What is saved is the split structure and each cell's kind · working directory · startup command · file path · URL.

A preset applies a layout and startup settings to new work. It does not clone running processes. See [workspace restoration](workspaces.md#restoring-after-a-restart) for reopening the layout and history of previous work.

### Saving

- Workspace — right-click a sidebar card > **Save as workspace preset**.
- Tab — right-click a Tab > **Save as tab preset**.
- Pane — right-click an empty area of the tab strip > **Save as pane preset**.

The file is created at `~/.tasty/presets/{workspace,tab,pane}/<name>.toml`. If the name already exists, a number such as `-2` is appended.

Saving reports an error rather than overwriting a file changed in another editor since it was loaded, or a damaged file that could not be read. Check the external changes, then reopen the app to load them.

If renaming saves the new file but cannot remove the previous one, it reports an error and both names may remain. Check the list and files before choosing which name to keep.

While a preset is being saved, renamed, or deleted, editor input is temporarily disabled. You can continue editing when the file operation finishes. If saving fails, an error is shown and your settings draft and selection are retained. If only part of a batch was saved, those saved changes remain.

### Editing

Open **Tools** > **Presets** in the sidebar and the **Layout Presets** window appears.

- Choose **Workspace** / **Tab** / **Pane** with the tabs at the top, and pick a preset from the list on the left.
- A structure preview is shown on the right. The toolbar has **Rename** · **Duplicate** · **Delete**.
- Press **Edit** to change the structure directly on the preview — click the strip that appears when you hover a cell's edge to split in that direction, click the middle of a cell to select it and remove it with the trash handle at its top right, and add and remove Tabs with `+` / `×`. The same split · close shortcuts as in the main window also work on the selected cell.
- Structure changes request an automatic save, with no save button. The **automatic saving** label describes this behavior. Leave with **Done**.
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
