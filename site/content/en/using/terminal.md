<!-- source-hash: c98a458a1e4e -->
# Working with the terminal

Copy the output you need, search terminal history, and open file paths as you work. You can also select text without a mouse and receive notifications when work finishes.

The shortcuts below use the default **Tasty** preset. On macOS, read `Alt` as `Cmd`.

## Shell

Set under **Settings** > **Terminal**.

- **Shell** — the shell to run. On Windows, Tasty looks for the bash from Git for Windows and, if it is missing, tells you to specify the path yourself.
- **Startup command** — one line entered at the start of every new terminal.
- **Inherit working directory** — starts a new terminal in the current terminal's directory (on by default).
- **Confirm close running process** — asks for confirmation when closing a terminal in which a program is running.

bash (4.4 or later) and zsh get shell integration with no extra setup. The Tab name follows the current directory, the git branch appears in the status bar, and you can attach hooks to command completion ([Hooks · notifications](../agents/hooks-notifications.md)). Other shells such as fish need to be set up by hand, and until then the **Shell integration not detected** banner may appear. It does not appear on terminals you view by attaching to a remote Tasty. It closes on its own after 10 seconds, and waits while the pointer is over it.

When the program exits and the shell ends, that Surface closes automatically.

## Arrow keys with modifiers

Terminal programs can distinguish arrow keys pressed with `Shift`, `Ctrl`, or `Alt` from plain arrow keys. **For these combinations on macOS, use the physical `Option` key, not `Cmd`.** For example, press `Option+↑` when a program displays `⌥ + ↑`. The program inside the terminal determines the action.

Modified arrow keys are forwarded with **Option as Meta** either on or off. That setting only controls whether Option+character input on macOS produces a composed character or a Meta key sequence. Keys consumed by a Tasty shortcut, copy mode, or an overlay that captures keyboard input are not also sent to the program.

## Shift+Enter per application

In **Settings → Terminal → Input**, add an executable name (such as `claude`) and toggle **Send a newline with Shift+Enter**. When enabled, Shift+Enter sends a newline character (LF). Turning it off or removing the rule restores the platform's default key encoding. Save settings to apply your changes.

The Claude Code plugin adds the newline rule for `claude` when first activated after installation. Existing installations also receive it on their first activation after updating. Existing choices and any later edits or deletions are preserved.

Executable names must match exactly, ignoring case and `.exe`. Detecting an application change can take up to one second. If only a launcher such as `node` is detected, add a rule for that executable name. These rules do not affect pasted text or plain Enter.

With the default key encoding, Codex CLI for Windows uses `Shift+Enter` or `Ctrl+J` to insert a newline and `Enter` to send.

## Copy · paste

| Action | Shortcut |
|---------|--------|
| Copy | `Ctrl+C` · `Alt+C` · `Ctrl+Shift+C` |
| Paste | `Ctrl+V` · `Alt+V` · `Ctrl+Shift+V` |
| Enter copy mode | `Ctrl+Shift+Space` |

- `Ctrl+C` copies when there is a selection; otherwise it interrupts the program as usual.
- A `Ctrl+C` within 0.5 seconds after a paste is ignored — this prevents losing input by hitting the key next to `Ctrl+V` by mistake. A toast tells you when it is ignored.
- If the clipboard has no text but has an image, it is saved as a PNG file and the path is pasted. Use this to hand a screenshot to an agent. Saving happens in the background without freezing the window, and the path goes into the terminal you pasted into once the file is saved. Anything you type in the meantime may arrive before the path. If you close that terminal before then, the path is not pasted and the file is deleted.
- Lines wrapped because of the window width are joined into one line when copied.
- When you copy from a right-click menu, the "Copied" notice also shows the copy shortcut on its right. Copying a path from the Explorer right-click menu shows the copy-path shortcut. It does not appear when you copied with the shortcut or left the shortcut empty.

### Screenshot to clipboard

Press `Ctrl+Alt+S` and pick an area of the screen: the PNG file is saved under `~/.tasty/screenshots/` and both its path and the image go to the clipboard. Pasting into a terminal gives the path; pasting into an app that accepts pictures, such as an image editor, gives the image. Pressed in a remote mirror workspace, the file is saved on the remote computer and goes to the remote clipboard.

- **macOS**: picks the area with `screencapture`. Screen recording permission is required.
- **Linux**: on Wayland it uses `grim`·`slurp`, or `gnome-screenshot` if they are missing. On X11 it uses the installed tool in the order `gnome-screenshot` → `scrot` → ImageMagick `import`. If one tool fails, the next one is tried.
- **Windows**: captures the whole screen across all monitors, without area selection.
- If the image cannot be put on the clipboard (the picture cannot be read, or on Linux it is larger than one X11 transfer allows, and so on), only the path goes in.
- If you cancel the area selection with Esc, no other tool opens and the clipboard is left as is. The same applies when the tool ends without leaving a file.
- The result appears as a notification in the window where you pressed the shortcut. Copying both the image and the path shows a success; copying only the path shows a warning; a missing or failing tool, or a failed save or copy, shows the reason. Cancelling with Esc shows nothing, and a tool that ends without a file (some tools end this way when cancelled) only says "No screenshot was taken." If you move to another window while picking the area, the notification still appears in the first window, and not at all if that window was closed.

### Selecting

Drag (characters) · double-click (word) · triple-click (line). Selection extends into the scrollback. Right-click after selecting and a menu appears with **Copy** · **Copy Without Line Breaks** · **Copy Terminal ID**. If the selected text is an actual file · folder path, **Open File** / **Open Folder** also appear.

While typing a command in a shell, click once on the input line to move the cursor there. The cursor does not move in programs other than a shell, while you are scrolled back, or in programs that capture the mouse. To turn this off, put `click_to_move_cursor = false` under `[general]` in `config.toml` ([Settings](../customize/settings.md)).

### Copy mode (vi)

Move around the screen and the scrollback and copy without the mouse. Click the terminal, then press `Ctrl+Shift+Space`. On macOS this is the `Control` key, not `Cmd`. If you changed the shortcut, press your own key.

On entry, a notice in the lower-right corner of the terminal lists the keys to move, select, copy, and leave, and a copy cursor appears on the first cell of the top line on screen. Keys you press in copy mode are not sent to the shell or program.

If Tasty cannot enter copy mode, a notice tells you why.

- You cannot enter it while a program that uses the terminal's **alternate screen**, such as vim or less, is running. Maximizing the window or switching to OS full screen does not matter. Leave the program and try again.
- You cannot enter it on a surface that is not a terminal, such as the explorer. You also cannot enter it while a markdown or HTML surface takes keyboard input, but then the notice may be hidden behind that surface's page.
- If you are already in copy mode, Tasty tells you so instead of entering again.

While a modal or popup takes keyboard input, the shortcut goes there, so neither copy mode nor a notice appears. Click the terminal and press it again.

For example, run `printf '\033[2J\033[HABCDE\n'` in a shell to leave `ABCDE` at the top of the screen. Enter copy mode and check that the cursor sits on `A`. Release `Ctrl` and `Shift`, then press `v` `l` `l` `y` in turn: `ABC` is copied to the clipboard and copy mode ends.

| Key | Action |
|----|------|
| `h` `j` `k` `l` | Move. Prefix a number to repeat |
| `w` `b` `e` / `W` `B` `E` | Move by word |
| `0` `^` `$` | Start of line / first character / end of line |
| `gg` `G` / `H` `M` `L` | Top / bottom / top · middle · bottom of the screen |
| `v` / `V` | Start character / line selection |
| `y` | Copy and leave on success; keep the selection on failure. Without a selection, leave without copying |
| `/` `?` `n` `N` | Search down · up, next · previous match |
| `"+` `"*` | Choose the target clipboard for the next copy |
| `q<char>` … `q` / `@<char>` | Record / replay a macro |
| `Esc` | Clear the selection. Leaves if there is no selection. With a selection, the first `Esc` clears it and the second leaves |

Copy notices appear only after the clipboard accepts the write. If access is unavailable or a transfer fails, Tasty shows a notice and keeps your selection. With a selection, a failed Ctrl+C copy does not interrupt the program. A failed paste sends no input; an empty clipboard does nothing.

On Linux, terminal copy and paste use X11 (through XWayland in a Wayland session). Native Wayland clipboard access is not supported. If X11 access is unavailable, Tasty tells you so.

### Clipboard access by programs (OSC 52)

A program inside the terminal (tmux, remote vim, and so on) can request a **write** to the clipboard. Tasty reports success only when the write succeeds and reports unavailable access or failure otherwise. **Reading** requires turning on **Settings** > **Terminal** > **Allow clipboard read (OSC 52)**. It is off by default; be aware that once on, a program on the far side of SSH can also take the clipboard contents.

### Writing to the clipboard from a script

A script or agent can put text on the clipboard directly.

```sh
tasty clipboard set-text "text to copy"
```

## Search

`Ctrl+F` or `Alt+F`, or the search icon on the tab strip. A search bar appears at the top center of the Surface.

In a normal terminal, search covers the scrollback and the screen. Inside a program that uses an alternate screen, such as vim or less, it searches only the current screen. You can search earlier shell output after leaving the program.

- `Enter` / `Shift+Enter` / `↑` `↓` — next / previous match. The match count is shown as `3/42` and the screen follows.
- Matches are painted light yellow and the current match deeper yellow. Text inside the current match is drawn in the theme's match ink whatever colour the program chose, so it stays readable in every theme. Other matches keep their own text colour.
- Toggles on the right — **Match case** · **Regular expression** · **Match whole word**.
- `Esc` or `×` while the cursor is in the search bar — close. Even with the search bar open, if the cursor is in the terminal, keystrokes go to the terminal as usual. Press `Ctrl+F` again to move between the search bar and the terminal.
- The search bar opens on the Surface where the cursor is. If it is open in another tab or on a neighboring split Surface, that search is cleared and the bar moves to the current Surface.

The query field keeps its own background and border when unfocused. An invalid regular expression turns its border red; the error remains visible when you return focus to the terminal.

## Opening links and paths

Hold `Ctrl` and hover a URL or path and a blue underline appears; click to open it in the browser or the associated program. If the browser or program can't be started, a notice that it couldn't be opened appears at the bottom right of that terminal. Relative paths such as `src/main.rs` also work, but only those that actually exist relative to the current directory become links.

Right-click an underlined link to open a link menu.

- **Select** — selects the whole link text.
- **Copy** — copies the link text as shown on screen. If you assign a key to **Copy link** in the keybinding settings, the "Copied" notice shows that key on its right, and pressing it over a link copies without the menu. It has no default key.
- **Open With…** — instead of opening right away, shows a list to choose which program opens it. `http(s)://` addresses can be chosen too; picking the HTML viewer, for example, opens the page in a Tasty tab instead of the browser. A path in a remotely attached terminal cannot be opened on this computer, so the list is empty.

Even while a program that uses the mouse (vim · tmux and so on) is running, right-clicking a link shows this menu instead of going to that program. Right-clicking anywhere else works as before.

Under **Settings** > **Terminal** > **Link click modifier**, choose `Ctrl`, `Alt`, or **Disable left-click opening**. The last option prevents left-clicks from opening links. You can still hover a link without a modifier and use its right-click menu. Popups and banners can block link highlighting.

How files are opened (Markdown · image · Explorer) is in [Managing and viewing files](files.md). Dropping a file onto the terminal (**Drop to open**) opens it by the same rules.

## Scrolling

- Use the mouse wheel, or `PageUp` / `PageDown` (one screen at a time), to view the scrollback. Typing returns to the bottom.
- Inside a program that uses the whole screen, such as vim · less · htop, the wheel and `PageUp` / `PageDown` are passed to the program.
- The number of lines kept is **Settings** > **Terminal** > **Scrollback lines** (default 10,000, maximum 100,000). `clear` also empties the scrollback.
- The scrollback continues after a restart — [Workspaces](workspaces.md#restoring-after-a-restart).

Increase and decrease the font size with `Ctrl+=` / `Ctrl+-` and reset it with `Ctrl+0`. It applies to the focused Surface — a terminal, Explorer or Markdown.

## Programs that capture the mouse

When a program that uses the mouse directly, such as vim (`:set mouse=a`) · htop · Claude Code, is running, clicks and drags go to that program. A banner appears on the first click.

> **Mouse input captured** — This app is capturing the mouse. Use Shift+drag to select text, Shift+Right-click for the tasty menu.

- `Shift+drag` — select text. `Shift+double-click` selects a word.
- `Shift+right-click` — Tasty's copy menu.
- The banner closes by itself when the program ends. You can also close it right away with `×`.
- In **More options**, which appears when you hover the banner, choose **Don't show again for {program}** or **Let Tasty handle clicks in {program}**. When Tasty handles the clicks, clicks over that program act as selection as usual and only the wheel goes to the program. When Tasty can't tell which program is running, such as in a terminal you view by attaching to a remote Tasty, both items are dimmed and can't be chosen. A note at the top of the menu says why.

The same items are under **Settings** > **Terminal** > **Mouse Capture** — the **Show mouse-capture hint** toggle, the **Let Tasty handle clicks in these programs** list, and the **Don't show the capture notice for these programs** list. Names match partially and take wildcards such as `ht*`.

## Running · attention marks

- The **green dot** on a Tab and the dot on a sidebar card — a program other than the shell is producing output. It disappears when the program stops and waits for input. Output that only changes the terminal title does not count as output.
- A **yellow border** on a Surface — the program is waiting for a response (an agent asked a question). A **blue border** — a command finished or a notification arrived. Click that Surface and it disappears. Tab names and sidebar badges use the same colors.

## Notifications

Terminal bells (`\a`) and notification sequences sent by programs (OSC 9 · 99 · 777) are collected and shown together.

- `Ctrl+Shift+I` — the **Notifications** panel. A newest-first list with a **Jump** button and **Mark all read**. Opening it marks everything as read. The fullscreen button on the title bar shows it at a large size ([Fullscreen](panes-tabs-splits.md#fullscreen)).
- Notifications are not sent to the OS notification center. Even when the window is in the background, check them in this panel, on the Surface border, and with the sidebar badge.
- **Settings** > **General** > **Notifications** — **Notifications enabled** · **Sound** · **Coalesce interval (ms)** (the interval within which consecutive notifications from the same place are merged into one, default 500).
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
