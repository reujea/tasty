<!-- source-hash: 4597e04d37b7 -->
# Settings

Configure Tasty’s shell, fonts, language, and other preferences. Use the settings window or edit `~/.tasty/config.toml` directly. Keybindings and themes have their own guides.

## Find settings by purpose

- [Shell and input](#terminal)
- [Appearance and fonts](#appearance)
- [Keybindings](keybindings.md)
- [Themes](themes.md)
- [Plugins](../plugins/index.md)
- [Automation](scripts.md)

## Opening the settings window

Press the **Settings** button at the very bottom of the sidebar, or press `Ctrl+,`.
Press the same shortcut again to close the settings window.

<!-- tasty-diagram: settings-window -->
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ [General] [Terminal] [Appearance] [Keybindings] [Handler] [Misc] [Plugins]  │  top tabs
├────────────────────┬────────────────────────────────────────────────────────┤
│ Filter sections…   │                                                        │
│ ▸ General          │   Settings entries of the selected section             │
│   Notifications    │                                                        │
│   Accessibility    │                                                        │
├────────────────────┴────────────────────────────────────────────────────────┤
│                                                      [ Cancel ]  [ Save ]   │
└─────────────────────────────────────────────────────────────────────────────┘
```

- Seven top tabs: **General** · **Terminal** · **Appearance** · **Keybindings** · **Handler** · **Misc** · **Plugins**.
- The list on the left holds that tab's sections. Typing into **Filter sections…** above it filters the list. Switching tabs clears the filter text.
- **Save** applies your changes and starts saving them to the file. You can keep working while the file is written. **Cancel** discards your edits. There is no close button in the header.
- Settings an agent changes by command while the settings window is open (input rules, remote transfer, accepting webhooks from other computers, and so on) stay as they are when you press **Save**. Saving only writes what you changed in this window. If both sides changed the same item, this window's value wins.
- If saved settings cannot be applied, an error notice appears and the settings stay as they were. If they were applied on screen but could not be written to `config.toml`, a warning notice appears instead; those changes are lost when Tasty restarts.
- If another program changes `config.toml` after Tasty reads it, Tasty reports a save failure and preserves that file. Restart Tasty to load the changes made outside the app.
- Startup corrections and the confirmed shell path are saved together. The loading screen keeps updating while the save completes. During normal use, new windows use the latest settings applied in this run even if a save is still pending.
- Changes to **Language** take effect after you save and restart Tasty. Performance settings apply to terminals you open after saving; terminals that are already open don't change right away.
- Numeric entries are **typed**, never dragged. Values are not adjusted while you type; they are brought into the allowed range when you leave the field or press `Enter`. Type something outside the range and the field's border takes the warning colour, with one line under it naming the range and the value that will actually be saved.
- Whole-number fields widen to show the entire value, and grow with each character as you type. The unit sits after the field. A value in the settings file that is larger than the range is shown as written; it is not changed when it is read.
- If the settings window is so narrow that an entry's control no longer fits beside its name, that entry moves the control onto the line below the name. Widen the window and it moves back beside it.

## Entries by tab

### General

| Section | Entries |
|------|------|
| **General** | **Restore layout on startup** · **Restore surface content on restart** (terminal scrollback) · **Workspace categories (folders)** · **Next/prev workspace crosses categories** · **Explorer: refresh file details** (off by default — when on, size and modified-time changes of files in the open folder show up within a few seconds too, but every check reads all items of that folder; see [Files](../using/files.md)) · **Close behavior** (Ask / Minimize to background / Quit) · **Wheel scroll distance** (10-200pt, default 50) - how far one wheel notch scrolls, applied the same everywhere in the window · **Language** (English / 한국어 / 日本語 plus any [language pack](#adding-a-language-language-packs) you install) · **Time format in Properties** (Minutes / Seconds / Seconds and UTC offset, default Minutes — used for Modified, Created and Target modified in Explorer Properties, in this computer's time zone; an open Properties popup follows right away) · **Accept webhook calls from other computers** (off by default, applies from the next start — [webhooks](../agents/hooks-notifications.md)) |
| **Notifications** | **Notifications enabled** · **Sound** · **Coalesce interval (ms)** |
| **Accessibility** | **Reduced motion** (turns off toast and overlay fades, the loading spinner's rotation, and the modal shake) · **Show modifier key hints** |
| **Overlay** | **Toast duration** (1~10 seconds) |
| **Remote transfer** | **Save folder** (default `~/.tasty/transfers/`) · **Maximum size** (MiB) — where files received from a remote Workspace are saved |
| **Display** (macOS only) | **Cmd key display** · **Option key display** · **Shift key display** — choose text or symbols for keybinding notation |
| **Permissions** (macOS only) | **Full Disk Access** · **Screen recording** status, **Request all permissions** (asks for the folders and screen recording in one go), and **Open System Settings** in the Full Disk Access row |

Turn off **Restore surface content on restart** and earlier terminal content no longer comes back from the next start. Content that was already saved is not restored either, while the tab layout and working directories still are. Reopening a closed tab (`Ctrl+Shift+T`) also leaves out terminal content while the setting is off.

**Browse…** beside the **Remote transfer** save folder opens the Tasty folder picker inside Settings. **Select folder** fills the field with the selected subfolder, or the current folder when nothing is selected. Press **Save** in Settings to apply it. Cancel or Esc leaves the folder value unchanged.

### Terminal

| Section | Entries |
|------|------|
| **General** | **Shell** · **Startup command** · **Scrollback lines** (default 10000) · **Confirm close running process** · **Inherit working directory** · **Reverse-screen flash (DECSCNM)** · **Show bell notification** · **Link click modifier** (Ctrl / Alt / Disable left-click opening) · macOS: **Use Option as Meta** · Windows: **Shell mode** |
| **Input** | Per-app **Send a newline with Shift+Enter** rules — add an executable name (for example `claude`) and turn it on, and `Shift+Enter` sends a newline (LF) while that app takes the input. Turned off, or with no rule, it uses the platform's default encoding |
| **Mouse Capture** | **Show mouse-capture hint** · **Let Tasty handle clicks in these programs** · **Don't show the capture notice for these programs** — process names or patterns such as `ht*` |
| **TUI** | **Allow clipboard read (OSC 52)** — off by default. When on, programs inside the terminal can read the clipboard |
| **Performance** | **Targeted PTY polling** · **Scrollback disk swap** (moves lines past the scrollback line count to disk) · **Lines kept on disk** (default 10000, -1 = unlimited — separate from the scrollback line count; the oldest lines past it are dropped). The unit **lines** follows the field, and at -1 **no limit** takes its place. Type a value outside the range and "Use -1 or a number from 0 to 100000000." appears under the field. As the note at the top of the tab says, all three apply to terminals you open after saving, and terminals that are already open don't change right away |

Set **Link click modifier** to **Disable left-click opening** to prevent left-clicks from opening links. You can still hover a link without a modifier and use its right-click menu. Popups and banners can block link highlighting. The meaning of the mouse capture entries is in [Working in the terminal](../using/terminal.md).

### Appearance

| Section | Entries |
|------|------|
| **Theme** | The theme card list — [Themes](themes.md) |
| **Colors** | Override the current theme's colours entry by entry — [Themes](themes.md) |
| **General** | **Default Font Settings**: **Font family** · **Custom font file** · **Font size** (default 14) · **Line height** · **Font DPI scaling** (Auto / Fixed). Plus **Ligatures** · **Background opacity** |
| **Display** | **UI Scale** — Small / Medium / Large |
| **Tasty** | Button, sidebar, and tab colours — **Accent** · **Sidebar background** · **Active tab indicator** (Underline / Fill / Dot) |
| **Terminal** | The terminal Surface's **Focused background** · **Unfocused background** and font override |
| **Explorer** | Explorer-only font override |
| **Markdown** · **HTML** | Pages added by plugins — the Markdown font override and **Allow remote content**, and the HTML viewer's **Default zoom** · **Color scheme** · **Allow remote content** · **Sandbox scripts** |

For fonts, the defaults under **General** apply to terminal · Markdown · explorer all at once, and turning off **Use default** in a section lets only that type use different values. With **Font DPI scaling** set to **Auto**, the physical size of the text stays the same across monitors; with **Fixed**, the pixel size stays the same, so text gets smaller on high-resolution monitors.

Open the **Font family** list and type part of a name in the search field at the top to filter it; the search text clears when the list closes. **monospace (default)** always stays at the top whatever you type, so you can return to the default font at any time. If the settings file names a font that isn't installed on this computer, that name is shown as is and left unchanged.

### Keybindings

Covered in [Keybindings](keybindings.md).

### Handler

The tables that decide how to "identify what kind of file it is" when opening a file, and "what to open it with". Markdown · images · HTML open on their own with the defaults, so you rarely need to touch this.

- **File Extension Mapping** — Priority when several detectors claim the same extension.
- **File Detectors** — Rules that identify a file's type by extension · path pattern. The list shows detectors in the order they are tried. You can add your own rules; adding with an id that already exists adds your rules to that detector. Remove deletes only what you added, and built-in · plugin rules stay. Changes apply on Save, and until then Undo on each row cancels them.
- **File Handlers** — Which Surface opens an identified type, or whether to hand it to the OS default app. The list is in run order: lower priority first, and on a tie user handlers, then plugin handlers, then built-in ones.
- **Hook Handlers** — Shell commands and internal action lists to run when a hook · webhook event arrives. [Hooks, notifications, and webhooks](../agents/hooks-notifications.md).

These tables are stored not in `config.toml` but in `~/.tasty/file-handlers.toml` and `~/.tasty/hook-handlers.toml`.

### Misc

- **Scripts** — Register Lua scripts to run from a keybinding or an event ([Lua scripts](scripts.md)). Enter the file path (for example `~/.tasty/scripts/my-script.lua`) and a display name; attach the keybinding under **Keybindings** > **Run Scripts**. If the file changes after registration, a **changed** marker appears and you are asked to confirm on the next run.
- **Task pipeline** — Size limits for the notes kept in a task graph's report. A note longer than the **Note size limit** is cut, and notes past the **Attempt report limit** are not stored, only counted. The first value must always be smaller than the second ([Task report](../agents/tasks.md)).
- **Anomaly detection** — Change notification switches and thresholds for memory growth, repeated calls and call bursts. These are heuristic signals that normal work can trigger. Repeated calls counts the same call, rather than response latency. Turning notifications off keeps detection, recording and numeric editing active. Saved values apply from the next check. Out-of-range numbers are clamped on Enter or when leaving the field; Cancel discards the draft.
- On Windows a **Tastyrc** section is added.

### Plugins

Settings pages added by plugins are collected here. In a default install there are **Claude Code** and **Codex** pages — [Plugins](../plugins/index.md).

## Adding a language (language packs)

Tasty ships with three languages: English, 한국어 and 日本語. Any other language comes
from a **language pack** you place yourself; it then appears in the language list
alongside the built-in ones.

### Writing a pack

Create a folder named after the language code under `~/.tasty/lang/`, and put a
`pack.toml` inside it. For French that is `~/.tasty/lang/fr/pack.toml`.

```toml
[meta]
name = "Français"          # name shown in the language list (without it, the folder name is shown)

[font]                     # required — use exactly one of the four below
builtin = true             # the built-in font already covers the characters you use
# file = "fonts/x.ttf"     # a font file shipped inside the pack folder
# family = "Noto Sans"     # a font family installed on the machine
# candidates = ["fonts/x.ttf", "Noto Sans"]   # tried in order, first hit wins

[button]                   # from here on, the text you are translating
ok = "OK"
cancel = "Annuler"
```

- **`[font]` cannot be left out.** Text without a font choice can render as □, so
  the person writing the pack has to say which font it expects.
- **The declared font applies across the whole UI.** The font named by `file` ·
  `family` · `candidates` is added behind the built-in font, so the language's
  letters show up in the settings and popups and on plugin screens (the clipboard
  viewer, Markdown, and so on). `builtin = true` means "the built-in font is enough — add
  nothing."
- **You do not have to translate everything.** Anything you leave out shows in
  English. Leaving a value empty (`""`) also counts as "not translated" and shows
  in English — you never end up with a blank label.
- Entry names (`[button] ok` and so on) follow the same layout as the language
  files that ship with Tasty. Copying one of those is the quickest start.
- Text containing `{}` has a value filled in at runtime. **Keep the same number of
  them** — drop one and that value disappears, add one and a literal `{}` shows up
  on screen.

### Command-line help is translated too

`tasty --help` and the other terminal help screens come out in the language you
picked. In a language pack, that text lives under `[cli.help]`.

```toml
[cli.help._root]
about = "Émulateur de terminal accéléré par GPU"

[cli.help._root.new]
about = "Créer une ressource"
```

- Here too, **you do not have to translate everything.** Commands you leave out
  keep their English description and simply appear alongside the translated ones.
- Usage, command and option headings, default-value hints and argument errors also follow
  your selected language. This includes `tasty help help` and plugin `help help` pages.
  Default-value hints also appear for plugin arguments without descriptions.
  Command names, options and accepted values stay unchanged.
  Details from an underlying parser and the server's original error text may remain English.
- Plugin command descriptions and argument help can also be changed in the plugin translation
  files below. For example, put `[codex.cli]` and `desc = "My Codex commands"` in
  `~/.tasty/lang/plugins/com.tasty.codex/en.toml` to change the English Codex help title.
  Older plugins without translation keys retain their original descriptions.

### Selecting it

The pack appears in Settings › **General** › **Language**. Select it, press
**Save**, then restart Tasty.

### Changing plugin text

You can override plugin text without editing its installation folder. For Korean,
create `~/.tasty/lang/plugins/com.tasty.clipboard-viewer/ko.toml` with, for example:

```toml
[clipboard_viewer.command]
open_viewer = "내 클립보드 열기"

[clipboard_viewer.popup]
title = "내 클립보드"
```

The plugin's tool entry and internal UI read the same file. The folder name is the
plugin ID; use the corresponding ID and translation entry names for another plugin.
For a new `fr` language pack, use
`~/.tasty/lang/fr/plugins/com.tasty.clipboard-viewer.toml` instead. Create
`fr/pack.toml` and select that language first.

Loading proceeds from installed English to the installed selected language, then
to your file. Blank or missing entries keep the preceding text. Invalid TOML and
files over 2 MiB are ignored with a warning. Restart Tasty after editing. Your file
survives plugin upgrades. Do not duplicate the same plugin entries in the host's
`ko.toml` or `pack.toml`.

When you launch Tasty with a relative `TASTY_HOME`, plugins read user translations relative to the directory where you launched Tasty.
Standard bundled plugins also start from this home, and entries missing from your translations use the installed translations.

### Changing only some text of a built-in language

Place a **single file** instead of a folder — `~/.tasty/lang/ko.toml`, for example.
It overrides just the entries you write, and needs no `[font]`. This works for
English, 한국어 and 日本語 only; a new language placed this way does not show up in
the list.

Leaving a value empty (`""`) counts as "not changed" here too, just as in a pack —
the text that language ships with stays on screen, and you never get a blank spot.

### When it does not work

- **It does not appear in the list** — check that the folder is named after the
  language code, that the file inside is called exactly `pack.toml`, and that
  `[font]` is present. Close and reopen the settings window (the list is read once
  when the window opens). A `pack.toml` may not exceed **2 MiB** — the built-in
  language files are under 400 KiB, so a normal translation never runs into this.
- **English shows up with a warning** — the pack for the selected language was not
  found, or the file is invalid. The warning names the path that was looked for.
  **Your setting is kept**, so fixing the pack and restarting brings that language
  back.
- **Text renders as □ with a font warning** — the pack and its text loaded, but the
  font named by `[font]` could not be found or opened. The text still shows; only
  the font is missing. Check the `file` path (relative to the pack folder) or the
  `family` name and restart.
- The full reason is written as one line to `~/.tasty/debug.log`.

## The settings file `~/.tasty/config.toml`

General settings are stored in this file. Handler tables, theme files, and other items described above use their separate paths. Missing keys are read as defaults, so you only need to write the ones you need.

```toml
[general]
language = "ko"                  # "en" | "ko" | "ja"
close_behavior = "ask"           # "ask" | "minimize" | "quit"
wheel_line_scroll = 50.0         # how far one wheel notch scrolls (pt), 10-200
restore_layout = true
restore_surface_content = true
scrollback_lines = 10000
inherit_cwd = true
confirm_close_running = true
link_click_modifier = "ctrl"     # "ctrl" | "alt" | "none"
click_to_move_cursor = true      # clicking the shell input line moves the cursor there
allow_clipboard_read = false
bell_notification = true
workspace_categories_enabled = false
explorer_refresh_file_details = false  # Explorer also picks up size/modified-time changes in the open folder
explorer_props_time_format = "minutes"  # "minutes" | "seconds" | "seconds_offset" — time format in Explorer Properties
mouse_capture_blacklist = ["htop"]
shell = ""                       # empty = auto-detect
startup_command = ""

[appearance]
theme = "mocha"                  # ~/.tasty/themes/<id>.toml
ui_scale = "medium"              # "small" | "medium" | "large"; also sets the sidebar width
ligatures = true
background_opacity = 1.0
active_tab_indicator = "underline"   # "underline" | "fill" | "dot"

[appearance.default_font]
font_family = ""
font_size = 14.0
line_height = 1.0
font_scale_mode = "auto"         # "auto" | "fixed"

[appearance.terminal_font]       # leave empty to follow default_font
font_size = 15.0

[notification]
enabled = true
sound = false
coalesce_ms = 500

[accessibility]
reduced_motion = false

[modifier_hint]
enabled = true

[overlay]
toast_duration_ms = 2000

[performance]
targeted_pty_polling = true
scrollback_disk_swap = false
scrollback_disk_lines = 10000     # lines kept on disk, -1 = unlimited

[remote_transfer]
dir = ""                         # empty = ~/.tasty/transfers/
max_mb = 500

[webhook]
allow_external = false           # true = also accept webhook calls from other computers (from the next start)

[task_pipeline]
report_append_bytes = 1024       # note size limit, 64-65536
report_block_bytes = 16384       # attempt report limit, 128-131072, larger than the note size limit

[anomaly_detection]
rss_surge_notify = true
rss_surge_samples = 5            # 3~20
rss_surge_min_mib = 256          # 0~65536 MiB
slow_loop_notify = false
slow_loop_window_secs = 300      # 30~3600
slow_loop_threshold = 20         # 2~10000
call_burst_notify = true
call_burst_window_secs = 60      # 1~600
call_burst_threshold = 1000      # 10~100000

[keybindings]
new_tab = ["alt+t"]              # the rest is in keybindings.md
```

Anomaly settings are edited in this file. A disabled notification still leaves a detection record. SlowLoop is recorded without notifications by default because normal polling can trigger it; it does not measure response time. RSS growth requires the latest sample to reach the minimum size. An ongoing RSS growth trend or SlowLoop episode is recorded once; re-enabling notifications does not replay earlier records. Live threshold changes keep observation state. Reducing a window or sample count discards history on the next check, and increasing it cannot recover that history. Invalid file values reset this whole section to its defaults; invalid live changes are rejected.

- `[terminal_input]` stores per-app Shift+Enter behavior. Configure it under **Terminal → Input**. For example, a `[[terminal_input.rules]]` entry with `app = "claude"` and `shift_enter_newline = true` sends a newline to Claude Code. `initialized_defaults` records which plugin defaults have been applied and does not need manual editing.
- `[appearance]` also stores the full set of theme colours (`theme_base`) and the values overridden in the Colors tab (`theme_overrides`). Using the settings window is safer than editing these by hand.
- Values from plugin settings pages go into a section keyed by the plugin id, such as `[plugin_settings."com.tasty.html"]`.
- Tasty does not watch this file while running. Hand-edited values are read on the next start. If a save detects an external change, it refuses to overwrite the file and preserves it. Quit Tasty before editing the file directly.

Only the remote transfer entries can also be read and written from the CLI.

```sh
tasty settings get-remote-transfer
tasty settings set-remote-transfer --dir ~/incoming --max-mb 1000
```

Commands that change settings (`set-remote-transfer`, `set-input-rule`, `tasty webhook allow-external`, and so on) print their result only after the setting is applied and saved to `config.toml`. If it could not be saved, the command ends with an error instead of success, and the value applies to this run only.

## What is in the `~/.tasty` folder

`~/.tasty/config.toml` stores settings. Layouts and plugin data are managed separately. Do not edit or delete the layout store to troubleshoot restoration.

| Path | Contents |
|------|------|
| `config.toml` | The settings above |
| `themes/` | Theme files — [Themes](themes.md) |
| `lang/` | Language packs and language overrides — [Adding a language](#adding-a-language-language-packs) |
| `plugins/` · `plugins-logs/` | Installed plugins and their logs — [Plugins](../plugins/index.md) |
| `file-handlers.toml` · `hook-handlers.toml` | User entries from the Handler tab |
| `remote-profiles.toml` | Remote connection profiles — [Working remotely](../remote/attach.md) |
| `scripts/` | The conventional place for Lua scripts — [Lua scripts](scripts.md) |
| `transfers/` | Default save folder for files received from a remote |
| `tasty.port` · `debug.log` | The running instance's port, and the warning-and-above log — [Troubleshooting](../help/troubleshooting.md) |

<a id="what-to-read-next"></a>

## Keep exploring

- [Keybindings](keybindings.md) — The default table · presets · recording.
- [Themes](themes.md) — Switching themes · overriding colours · making your own.
- [Plugins](../plugins/index.md) — Plugin settings pages and the management window.
