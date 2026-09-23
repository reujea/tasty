<!-- source-hash: 286a80a0f4a4 -->
# Working with the terminal

Copy the output you need, search terminal history, and open file paths as you work. You can also select text without a mouse and receive notifications when work finishes.

The shortcuts below use the default **Tasty** preset. On macOS, read `Alt` as `Cmd`.

## Shell

Set under **Settings** > **Terminal**.

- **Shell** — the shell to run. On Windows, Tasty looks for the bash from Git for Windows and, if it is missing, tells you to specify the path yourself.
- **Startup command** — one line entered at the start of every new terminal.
- **Inherit working directory** — starts a new terminal in the current terminal's directory (on by default).
- **Confirm close running process** — asks for confirmation when closing a terminal in which a program is running.

bash (4.4 or later) and zsh get shell integration with no extra setup. The Tab name follows the current directory, the git branch appears in the status bar, and you can attach hooks to command completion ([Hooks · notifications](../agents/hooks-notifications.md)). Other shells such as fish need to be set up by hand, and until then the **Shell integration not detected** banner may appear.

When the program exits and the shell ends, that Surface closes automatically.

## Arrow keys with modifiers

Terminal programs can distinguish arrow keys pressed with `Shift`, `Ctrl`, or `Alt` from plain arrow keys. **For these combinations on macOS, use the physical `Option` key, not `Cmd`.** For example, press `Option+↑` when a program displays `⌥ + ↑`. The program inside the terminal determines the action.

Modified arrow keys are forwarded with **Option as Meta** either on or off. That setting only controls whether Option+character input on macOS produces a composed character or a Meta key sequence. Keys consumed by a Tasty shortcut, copy mode, or an overlay that captures keyboard input are not also sent to the program.

## Copy · paste

| Action | Shortcut |
|---------|--------|
| Copy | `Ctrl+C` · `Alt+C` · `Ctrl+Shift+C` |
| Paste | `Ctrl+V` · `Alt+V` · `Ctrl+Shift+V` |
| Enter copy mode | `Ctrl+Shift+Space` |

- `Ctrl+C` copies when there is a selection; otherwise it interrupts the program as usual.
- A `Ctrl+C` within 0.5 seconds after a paste is ignored — this prevents losing input by hitting the key next to `Ctrl+V` by mistake. A toast tells you when it is ignored.
- If the clipboard has no text but has an image, it is saved as a PNG file and the path is pasted. Use this to hand a screenshot to an agent.
- Lines wrapped because of the window width are joined into one line when copied.

### Selecting

Drag (characters) · double-click (word) · triple-click (line). Selection extends into the scrollback. Right-click after selecting and a menu appears with **Copy** · **Copy Without Line Breaks** · **Copy Terminal ID**. If the selected text is an actual file · folder path, **Open File** / **Open Folder** also appear.

### Copy mode (vi)

Move around the screen and the scrollback and copy without the mouse. Enter with `Ctrl+Shift+Space`. You cannot enter it while a program that uses the whole screen, such as vim, is running.

| Key | Action |
|----|------|
| `h` `j` `k` `l` | Move. Prefix a number to repeat |
| `w` `b` `e` / `W` `B` `E` | Move by word |
| `0` `^` `$` | Start of line / first character / end of line |
| `gg` `G` / `H` `M` `L` | Top / bottom / top · middle · bottom of the screen |
| `v` / `V` | Start character / line selection |
| `y` | Copy and leave |
| `/` `?` `n` `N` | Search down · up, next · previous match |
| `"+` `"*` | Choose the target clipboard for the next copy |
| `q<char>` … `q` / `@<char>` | Record / replay a macro |
| `Esc` | Clear the selection. Leaves if there is no selection |

### Clipboard access by programs (OSC 52)

A program inside the terminal (tmux, remote vim, and so on) can always **write** to the clipboard. **Reading** requires turning on **Settings** > **Terminal** > **Allow clipboard read (OSC 52)**. It is off by default; be aware that once on, a program on the far side of SSH can also take the clipboard contents.

### Writing to the clipboard from a script

A script or agent can put text on the clipboard directly.

```sh
tasty clipboard set-text "text to copy"
```

## Search

`Ctrl+F` or `Alt+F`, or the search icon on the tab strip. A search bar appears at the top center of the Surface.

- `Enter` / `Shift+Enter` / `↑` `↓` — next / previous match. The match count is shown as `3/42` and the screen follows.
- Toggles on the right — **Match case** · **Regular expression** · **Match whole word**.
- `Esc` or `×` while the cursor is in the search bar — close. Even with the search bar open, if the cursor is in the terminal, keystrokes go to the terminal as usual. Press `Ctrl+F` again to move between the search bar and the terminal.

## Opening links and paths

Hold `Ctrl` and hover a URL or path and a blue underline appears; click to open it in the browser or the associated program. Relative paths such as `src/main.rs` also work, but only those that actually exist relative to the current directory become links.

Right-click an underlined link to open a link menu.

- **Select** — selects the whole link text.
- **Copy** — copies the link text as shown on screen.
- **Open With…** — instead of opening right away, shows a list to choose which program opens it. `http(s)://` addresses can be chosen too; picking the HTML viewer, for example, opens the page in a Tasty tab instead of the browser. A path in a remotely attached terminal cannot be opened on this computer, so the list is empty.

Even while a program that uses the mouse (vim · tmux and so on) is running, right-clicking a link shows this menu instead of going to that program. Right-clicking anywhere else works as before.

Under **Settings** > **Terminal** > **Link click modifier**, choose `Ctrl`, `Alt`, or **Disable left-click opening**. The last option prevents left-clicks from opening links. You can still hover a link without a modifier and use its right-click menu. Popups and banners can block link highlighting.

How files are opened (Markdown · image · Explorer) is in [Opening files](files.md). Dropping a file onto the terminal (**Drop to open**) opens it by the same rules.

## Scrolling

- Use the mouse wheel, or `PageUp` / `PageDown` (one screen at a time), to view the scrollback. Typing returns to the bottom.
- Inside a program that uses the whole screen, such as vim · less · htop, the wheel and `PageUp` / `PageDown` are passed to the program.
- The number of lines kept is **Settings** > **Terminal** > **Scrollback lines** (default 10,000, maximum 100,000). `clear` also empties the scrollback.
- The scrollback continues after a restart — [Workspaces](workspaces.md#restoring-after-a-restart).

Increase and decrease the font size with `Ctrl+=` / `Ctrl+-` and reset it with `Ctrl+0`. This applies only to the terminal font.

## Programs that capture the mouse

When a program that uses the mouse directly, such as vim (`:set mouse=a`) · htop · Claude Code, is running, clicks and drags go to that program. A banner appears on the first click.

> **Mouse input captured** — This app is capturing the mouse. Use Shift+drag to select text, Shift+Right-click for the tasty menu.

- `Shift+drag` — select text. `Shift+double-click` selects a word.
- `Shift+right-click` — Tasty's copy menu.
- The banner closes by itself when the program ends. You can also close it right away with `×`.
- In **More options**, which appears when you hover the banner, choose **Turn off this notification for** or **Disable mouse capture for** that program. When disabled, clicks over that program act as selection as usual and only the wheel goes to the program.

The same items are under **Settings** > **Terminal** > **Mouse Capture** — the **Show mouse-capture hint** toggle, the **Disable mouse capture for these programs** list, and the **Suppress the capture hint banner for these programs** list. Names match partially and take wildcards such as `ht*`.

## Running · attention marks

- The **green dot** on a Tab and the dot on a sidebar card — a program other than the shell is producing output. It disappears when the program stops and waits for input.
- A **yellow border** on a Surface — the program is waiting for a response (an agent asked a question). A **blue border** — a command finished or a notification arrived. Click that Surface and it disappears. Tab names and sidebar badges use the same colors.

## Notifications

Terminal bells (`\a`) and notification sequences sent by programs (OSC 9 · 99 · 777) are collected and shown together.

- `Ctrl+Shift+I` — the **Notifications** panel. A newest-first list with a **Jump** button and **Mark all read**. Opening it marks everything as read. The fullscreen button on the title bar shows it at a large size ([Fullscreen](panes-tabs-splits.md#fullscreen)).
- If the window is in the background, an OS notification is shown as well.
- **Settings** > **Notifications** — **Notifications enabled** · **Sound** · **Coalesce interval (ms)** (the interval within which consecutive notifications from the same place are merged into one, default 500).
- To turn off only the bell, **Settings** > **Terminal** > **Show bell notification**.

To send a notification directly from a script, `tasty notify "build done" --title build`.

## Platform notes

- **macOS** — turn on **Settings** > **Terminal** > **Use Option as Meta** and combinations such as `Option+F` are sent as the readline · Emacs Meta key instead of special characters (off by default).
- **Windows** — the terminal may stop responding after waking from sleep. Tasty tries to wake it and, if it still appears stuck, tells you with a notification. In that case open a new terminal.
- If programs that flash the whole screen (readline's visible bell and so on) bother you, turn off **Settings** > **Terminal** > **Reverse-screen flash (DECSCNM)**.

## Other

- Colors and fonts are in [Themes](../customize/themes.md) and **Settings** > **Appearance**. The D2Coding font is bundled, so you do not need to install one separately.
- The Sixel · Kitty image protocols are not supported. View images with the [image Surface](files.md#image).
- How a script or agent sends commands to this terminal and reads its output is in [tasty CLI](../agents/cli.md). The number in the status bar is that terminal's ID.
