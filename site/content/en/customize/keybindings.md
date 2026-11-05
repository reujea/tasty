<!-- source-hash: a0637f308534 -->
# Keybindings

Use Tasty with shortcuts that feel familiar. Choose a preset or assign your preferred keys to actions you use often. Open **Settings** > **Keybindings** to get started.

## Notation

Tasty stores keybindings by the **physical position of the key**. That is why the same setting is pressed differently on the three OSes.

| Name written in settings | Windows · Linux | macOS |
|-------------------|-----------------|-------|
| `ctrl` | Ctrl | Control (⌃) |
| `alt` | Alt | **Command (⌘)** |
| `shift` | Shift | Shift (⇧) |
| `option` | Win (Windows) · Super (Linux) | Option (⌥) |

⌘ on macOS sits in the same place as Alt on Windows · Linux, so the setting value `alt+t` is pressed as `Alt+T` on Windows and `Cmd+T` on macOS. Likewise the Win key on Windows and the Super key on Linux sit where Option is on macOS, so recording `Win+K`, `Super+K`, or `⌥+K` stores `option+k`, and it works with the key in that place on all three OSes. The settings screen on Windows and Linux shows this key as `Win` and `Super`. The tables below fill in both columns by this rule.

Combinations Windows reserves, such as `Win+L` and `Win+D`, and the Super combinations GNOME and KDE use (Super alone, many Super+letter keys) are taken by the operating system first and never reach Tasty. They can't be recorded, and they don't work even if you write them in the settings file.

The settings file uses only four names: `ctrl`, `alt`, `option`, and `shift`. Key names such as `cmd`, `super`, `win`, and `meta` are not recognized.

**Non-Latin keyboards are matched by position too.** Even on a non-Latin layout such as Russian or Greek, keybindings are recognized by the Latin position on the keycap — on a Russian layout, pressing the `H` position (the key that types `Р` in Russian) together with modifiers still triggers the `Ctrl+Shift+H` shortcut. You do not need to re-record shortcuts after switching layouts. The same holds over Markdown and web preview surfaces.

To show the settings screen's notation as the `⌘` `⌥` `⇧` symbols on macOS, change **Cmd key display** · **Option key display** · **Shift key display** under **Settings** > **General** > **Display**. The stored values stay the same.

## Default keybindings (Tasty preset)

On a fresh install the **Tasty** preset is applied. When an action has several combinations, all of them work.

### Workspaces · categories

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| New Workspace | `Alt+N` | `Cmd+N` |
| Close Workspace | `Alt+Shift+W` | `Cmd+Shift+W` |
| Rename Workspace · change subtitle | `F3` · `F4` | `F3` · `F4` |
| Go to Workspace n | `Alt+1` ~ `Alt+9` | `Cmd+1` ~ `Cmd+9` |
| Next · previous Workspace | `Alt+J` · `Alt+K` | `Cmd+J` · `Cmd+K` |
| Go to category n | `Ctrl+Shift+1` ~ `Ctrl+Shift+0` | `Ctrl+Shift+1` ~ `Ctrl+Shift+0` |
| Next · previous category | `Ctrl+Shift+J` · `Ctrl+Shift+K` | `Ctrl+Shift+J` · `Ctrl+Shift+K` |

Category keybindings work only when **Settings** > **General** > **Workspace categories (folders)** is turned on.

### Panes · Tabs · Surfaces

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| Split Pane (left/right / top/bottom) | `Alt+E` / `Alt+Shift+E` | `Cmd+E` / `Cmd+Shift+E` |
| Next · previous Pane | `Ctrl+]` · `Ctrl+[` | `Ctrl+]` · `Ctrl+[` |
| Close Pane | `Ctrl+Shift+W` | `Ctrl+Shift+W` |
| New Tab | `Alt+T` | `Cmd+T` |
| Go to Tab n | `Ctrl+1` ~ `Ctrl+0` | `Ctrl+1` ~ `Ctrl+0` |
| Next · previous Tab | `Ctrl+L` · `Ctrl+H` | `Ctrl+L` · `Ctrl+H` |
| Rename Tab | `F2` | `F2` |
| Close active item (Tab → Pane → Workspace, in that order) | `Ctrl+W` | `Ctrl+W` |
| Restore closed item | `Ctrl+Shift+T` | `Ctrl+Shift+T` |
| Split Surface (left/right / top/bottom) | `Alt+D` / `Alt+Shift+D` | `Cmd+D` / `Cmd+Shift+D` |
| Next · previous Surface | `Alt+]` · `Alt+[` | `Cmd+]` · `Cmd+[` |
| Close Surface | `Alt+W` | `Cmd+W` |
| Convert Surface type (terminal ↔ Markdown, etc.) | `Alt+'` | `Cmd+'` |
| Exit fullscreen stage | `Esc` | `Esc` |

### Terminal · clipboard · explorer

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| Copy | `Ctrl+C` · `Alt+C` · `Ctrl+Shift+C` | `Ctrl+C` · `Cmd+C` · `Ctrl+Shift+C` |
| Paste | `Ctrl+V` · `Alt+V` · `Ctrl+Shift+V` | `Ctrl+V` · `Cmd+V` · `Ctrl+Shift+V` |
| Search (terminal · Markdown document) | `Ctrl+F` · `Alt+F` | `Ctrl+F` · `Cmd+F` |
| vi copy mode | `Ctrl+Shift+Space` | `Ctrl+Shift+Space` |
| Screenshot to clipboard | `Ctrl+Alt+S` | `Ctrl+Cmd+S` |
| Copy path (explorer) | `Alt+Shift+C` | `Cmd+Shift+C` |
| Copy link (link under the mouse) | None | None |
| Cut (explorer · web page · address bar) | `Ctrl+X` · `Alt+X` | `Ctrl+X` · `Cmd+X` |
| Select all (explorer · web page · address bar) | `Ctrl+A` · `Alt+A` | `Ctrl+A` · `Cmd+A` |
| Refresh (the surface you are looking at) | `F5` | `F5` |
| Refresh · go to parent folder (explorer) | `F5` · `Alt+↑` | `F5` · `Cmd+↑` |
| Back · forward (explorer) | `Alt+←` · `Alt+→` | `Cmd+←` · `Cmd+→` |
| Go to the address bar (explorer · HTML · Markdown) | `Alt+L` | `Cmd+L` |
| Open the selected item · clear the selection (explorer) | `Enter` · `Esc` | `Enter` · `Esc` |
| Move between items · extend the selection (explorer) | Arrow keys · `Home` · `End` · `PageUp` · `PageDown` · the same with `Shift` | Same |
| New folder · new file (explorer) | `F7` · `Shift+F4` | `F7` · `Shift+F4` (hold `Fn` with them by default) |
| Rename · move to trash (explorer) | `F2` · `Delete` · `Alt+Backspace` | `F2` · `Delete` · `Cmd+Backspace` |
| Undo (explorer) | `Ctrl+Z` · `Alt+Z` | `Ctrl+Z` · `Cmd+Z` |
| Undo (web page · address bar) | `Ctrl+Z` · `Alt+Z` | `Ctrl+Z` · `Cmd+Z` |
| Redo (explorer) | `Ctrl+Shift+Z` · `Ctrl+Y` · `Alt+Shift+Z` | `Ctrl+Shift+Z` · `Ctrl+Y` · `Cmd+Shift+Z` |
| Redo (web page · address bar) | `Ctrl+Shift+Z` · `Ctrl+Y` · `Alt+Shift+Z` | `Ctrl+Shift+Z` · `Ctrl+Y` · `Cmd+Shift+Z` |
| Show or hide hidden files (explorer) | `Ctrl+Shift+.` · `Alt+Shift+.` | `Ctrl+Shift+.` · `Cmd+Shift+.` |
| Zoom in · zoom out · reset zoom | `Ctrl+=` · `Ctrl+-` · `Ctrl+0` (`Alt` also works) | `Ctrl+=` · `Ctrl+-` · `Ctrl+0` (`Cmd` also works) |

**Refresh** asks the surface you are looking at to refresh. Markdown re-reads its file (or the original on the remote), the explorer re-reads its folder, and a terminal does nothing. Even when it shares a key with the explorer's own Refresh, the folder is read once. Over a Markdown or HTML page, the only keys without modifiers it takes are function keys such as `F5`; if you assign a letter such as `R`, that page keeps it as typing. If the Refresh key is also assigned to an editing key such as Copy or Paste, a web page, the input fields in it and the HTML address field run only that edit and do not refresh. While you type in the HTML address field, **Focus the address bar** also comes before Refresh on a shared key. While Refresh has `F5`, programs inside a terminal (mc, htop and so on) do not receive `F5`. To use `F5` in a terminal program, clear this Refresh key or change it under **Settings** > **Keybindings** > **Surface**.

`Ctrl+C` copies when there is selected text; otherwise it interrupts the running program as usual.

The copy, cut, paste and select-all keys also work in the body of Markdown and HTML views, in their input fields, and in the HTML address bar. They act only where the focus is. In an input field, select all picks just that field's text; in the body it picks the whole document. Read-only fields and the body do not change on cut or paste. On Windows and Linux, editing keys that web views and input fields already know, such as `Ctrl+C` and `Ctrl+V`, keep working even if you remove them here. A key you add here, such as `Alt+C`, runs the same edit once. Copy or cut with nothing selected leaves the clipboard as it was. Undo and redo in web pages can be changed separately from the explorer; on Mac, removing them also turns off `Cmd+Z` in web pages. If a plugin command uses the same key, the edit comes first in web pages. Word movement uses the default keys of the web view and the input field. On Linux, if too many edit requests pile up while a page is not responding, the edit keys can stop working in that view. Typing still works; close the view and open it again to get the edit keys back.

On Windows, while a Markdown or HTML view has keyboard focus, a shortcut assigned to a letter, digit or symbol key pressed without `Ctrl` or `Alt` (with or without `Shift`) is not received, and the key is typed into the view. For shortcuts that should also work in web views, use a combination with `Ctrl` or `Alt`, or a non-character key such as `F5`.

**Copy link** has no default key. Once you assign one, hover a terminal link and press it to copy the link text right away. It does not appear in the command palette.

### Window · tools

| Action | Windows · Linux | macOS |
|---------|-----------------|-------|
| New window | `Alt+Shift+N` | `Cmd+Shift+N` |
| Open/close Settings | `Ctrl+,` | `Ctrl+,` |
| Open/close notifications | `Ctrl+Shift+I` | `Ctrl+Shift+I` |
| Open/close DAG list | `Ctrl+Shift+G` | `Ctrl+Shift+G` |
| Hide / collapse sidebar | `Ctrl+Shift+B` / `Ctrl+B` | `Ctrl+Shift+B` / `Ctrl+B` |
| Command palette | `Ctrl+Shift+P` · `Alt+Shift+P` | `Ctrl+Shift+P` · `Cmd+Shift+P` |
| Clipboard Viewer (plugin) | `Ctrl+Shift+H` | `Ctrl+Shift+H` |

### Code fields

These work only while a multi-line code field, such as the hook handler sequence editor, has focus. They never reach terminals or other shortcuts, so the same keys may also be bound elsewhere.

| Action | Windows · Linux | macOS |
|--------|-----------------|-------|
| Apply in a code field | `Ctrl+Enter` · `Alt+Enter` | `Ctrl+Enter` · `Cmd+Enter` |
| Cancel in a code field | `Esc` | `Esc` |

### Actions with an empty default

The following have no default combination, either to prevent accidents or because they clash with OS shortcuts. Assign one yourself if you need it.

- **Quit** · **Immediate quit** · **Minimize to background** (the Mac preset includes `Cmd+Q` · `Cmd+M`, the Linux preset `Ctrl+Q`)
- Free combinations for **Next tab** · **Previous tab** (left empty because `Ctrl+Tab` clashes with the OS — the numbered switching `Ctrl+L` · `Ctrl+H` works)
- **Open Markdown** · **Open Explorer** · **Convert to Markdown** · **Convert to Explorer**
- The Explorer's **Preview panel** toggle · **Properties**
- **Apply workspace · tab · pane preset**, **Collapse/expand all categories**
- **Minimize window** · **Maximize/Zoom window** · **Close window** — on macOS, the items in the menu bar's **Window** menu show the first combination of each action and run on it. If you leave an action empty, its menu item has no shortcut either.
- The entries in the sidebar **Tools** menu — **Open port scanner** · **Open remote tools** · **Open preset window** · **Open tutorial** · **Open file picker**
- Open Git Viewer (plugin)

## Presets

**Settings** > **Keybindings** > **Preset** offers four. A preset is only a recommended set and is not tied to the OS — you can use the Mac preset on Windows.

| Preset | Based on | Differences from the Tasty preset |
|--------|------|------------------------|
| **Tasty** (default) | Its own | The tables above. Copy · paste · zoom bundle the conventions of all three OSes |
| **Mac** | iTerm2 · Terminal.app | `Cmd`-centred. Settings `Cmd+,` · notifications `Cmd+Shift+I` · DAG list `Cmd+Shift+G` · sidebar `Cmd+Shift+B` / `Cmd+B` · quit `Cmd+Q` · background `Cmd+M` · close Pane `Cmd+Shift+W`. Actions that bundle several combinations keep only the `Cmd` one — copy `Cmd+C` · paste `Cmd+V` · cut `Cmd+X` · select all `Cmd+A` · search `Cmd+F` · zoom `Cmd+=` · `Cmd+-` · `Cmd+0` · command palette `Cmd+Shift+P` · apply in a code field `Cmd+Enter`, and in the Explorer move to trash `Cmd+Backspace` · undo `Cmd+Z` · redo `Cmd+Shift+Z` · hidden files `Cmd+Shift+.`. **Close active item** and **Close Workspace** are empty |
| **Windows** | Windows Terminal | Pane split `Alt+Shift+E` / `Alt+Shift+D`, Surface split `Alt+D` / `Alt+E` · new window `Ctrl+Shift+N`. Actions that bundle several combinations keep only the `Ctrl` side and drop the `Alt` ones — copy `Ctrl+C` · paste `Ctrl+V` · cut `Ctrl+X` · select all `Ctrl+A` · search `Ctrl+F` · zoom `Ctrl+=` · `Ctrl+-` · `Ctrl+0` · command palette `Ctrl+Shift+P` · apply in a code field `Ctrl+Enter`, and in the Explorer move to trash `Delete` · undo `Ctrl+Z` · redo `Ctrl+Shift+Z` · `Ctrl+Y` · hidden files `Ctrl+Shift+.` |
| **Linux** | GNOME Terminal | Same as the Windows preset, but copy/paste/cut `Ctrl+Shift+C` / `Ctrl+Shift+V` / `Ctrl+Shift+X` · quit `Ctrl+Q` |

To apply one:

1. Click a preset row in the **Settings** > **Keybindings** > **Preset** list. The preset in use is marked **Active**.
2. In the detail view, check the rows that change in the three-column table of **Action** / **Current** / preset.
3. Press **Apply** at the top right. Nothing is saved to the file yet.
4. Press **Save** at the bottom of the window.

Applying a preset resets every keybinding you changed by hand back to the preset value.

## Moving to another computer — Import / Export

**Import / Export**, at the very bottom of the list on the left of **Settings** > **Keybindings** (below the divider), moves your whole keybinding configuration as a single file. General keybindings, number-switch rules, script keybindings and plugin keybindings all go in.

To export:

1. Press **Export…**.
2. Pick a folder, set the name in the **File name** box at the bottom and press **Save**. The default name is `tasty-keybindings-<date>.toml`. Clicking an existing file in the list puts its name in the box; if that name already exists, a warning appears and the button changes to **Overwrite**.
3. Once written, a notification shows the file path. Edits you haven't saved yet are included.

While the export is pending, the **Export…** button keeps its label and width and is disabled. A progress indicator and **Exporting…** appear beside it. You cannot start the same export again, but you can keep editing other settings. Closing the settings window or changing the keybinding bundle suppresses feedback from the earlier request; an accepted file write still finishes.

If the file can't be written (for example, the folder is read-only), **The export wasn't written** appears inside the **Export** row. Use **Try again** to write to the same place, or **Choose another location…** to pick somewhere else. While that notice is up, the **Export…** button is disabled. The middle sentence of the notice says which of three things went wrong — the folder is read-only, you don't have permission to write there, or the disk is full. If it was none of those, it says the write didn't finish and adds whatever the operating system reported on a small grey line below (truncated if long; hover it for the full text).

If the result cannot be confirmed, **Export result unknown** asks you to check the destination before trying again. The app does not automatically repeat the write. **Try again** exports your current draft. Replacement uses a temporary file in the same folder and preserves supported existing access permissions; it does not preserve every file attribute or promise survival of a power failure.

To import:

1. Press **Import…** and open the file you brought over. Nothing is applied yet — a comparison view opens.
2. Current and imported values sit side by side in four groups: **General bindings** · **Quick switch (by group)** · **Script bindings** · **Plugin overrides**. At first only rows that change are shown; **Show all N** at the top right shows everything.
3. Untick the rows you don't want. The tick on a group title turns the whole group on or off.
4. Press **Apply** at the top right. Nothing is saved to the file yet.
5. Press **Save** at the bottom of the window. **Cancel** discards the imported changes too.

Good to know:

- Quick-switch settings are imported as separate Tab, Workspace, and Category groups. Each group’s modifier and numbered keys are applied together.
- Plugin keybindings that exist only on this computer are not removed just because the file lacks them. Conversely, keybindings for plugins not installed on this computer are not imported, and the comparison view says so in one line.
- If parts of the file had to be skipped (a file written by a newer tasty, actions this version doesn't know), they are listed one per line in a **Read with warnings** box above the comparison. With four or more, three show and the rest open with **Show N more**.
- If an imported shortcut is one the OS may take first, such as `Win+L`, `Win+D`, `Win+E`, `Win+R`, `Win+I` or `Win+Tab` on Windows, the same box adds a line like "Windows may use Win+L itself, so Tasty might not receive it." It is only a warning; the import still goes ahead. On Linux this applies to a shortcut that is `Super` alone.
- If you pick something that isn't a keybinding file, you get **This file can't be read as keybindings** instead of the comparison, and nothing changes. Use **Choose another file** to try again.

### Importing a file made on another OS

A keybinding file imports as is on any OS. A keybinding made with `Option` on a Mac works with `Win` on Windows and `Super` on Linux, and the other way round. The **Explorer drag flip modifier** (at the bottom of **Settings** > **Keybindings** > **General**) defaults to `Option` on a Mac and `Ctrl` on Windows and Linux, so after importing a file exported on a Mac you hold `Win` or `Super` while dragging on Windows or Linux to swap move and copy.

## Changing one keybinding

1. Open **Settings** (`Ctrl+,`) > **Keybindings**. The sub-tabs on the left are divided by what the action targets — **General** · **Workspace** · **Pane** · **Tab** · **Surface** · **Clipboard** · **Zoom** · **Explorer** · **Run Scripts** · **Preset** · **Plugins** · **Import / Export**.
2. Click the key button of the action you want to change; it turns into **Press key combination...**. Press the combination you want.
3. To add another combination to the same action, press **Add binding**.
4. Pressing `Esc` while recording empties that slot.
5. If the combination is already used by another action, the **Shortcut already in use** popup appears. Choosing **Overwrite** removes the combination from the existing action and moves it to this one.
6. Press **Save**. **Cancel** discards everything.

Recording rules:

- Typing keys such as letters · digits · space register only together with at least one modifier (Ctrl/Alt/Shift). `W` alone is ignored.
- Keys such as `F1`~`F24` · `Tab` · `Enter` register without a modifier.
- A double tap, pressing a modifier twice quickly (such as `Shift` twice), records only for actions such as opening, closing, splitting and moving between workspaces, tabs and surfaces, opening Settings or notifications, and quitting. In a slot that a double tap does not run (the command palette, scripts, plugin commands, number switching and so on), the value stays unchanged and a notice appears. Double taps already written in the settings file that do not run are kept and listed as a warning at the bottom of the screen. On macOS `alt+alt` is ⌘ twice; ⌥ twice is not used as a double tap.
- `Esc` is reserved for "empty the slot" and cannot be recorded as a keybinding. To restore **Exit fullscreen stage** or **Cancel in a code field**, whose default is `Esc`, reapply a preset.
- When a plugin keybinding (the **Plugins** sub-tab) overlaps a core keybinding, **the plugin's runs first**.

## Changing a plugin keybinding

In the **Plugins** sub-tab, pick a plugin from the **Plugin:** drop-down at the top. Each of its commands gets one line.

- **Inherit** in the mode drop-down follows the key of one Tasty action (for example `clipboard.paste`). The key it follows right now is shown under the line.
- **Custom** records the key the same way as the other subtabs. Click the key button and press the combination; use `+` to add another one. Pressing `Esc` while recording removes that key, and removing every key leaves the command without a shortcut. Changing the plugin or the mode, or pressing **Reset**, while recording cancels the recording. With many keys or a narrow window, the key buttons and **Reset** continue on the next line, and every key stays visible.
- If a hand-edited settings file holds a key that can never match any input (for example `ctrl+shft+h`), that key's button shows the text as written with an error border, and **Unrecognized key:** appears under the row. That key will not run the command.
- If the key uses an OS key name such as `cmd+k`, `super+k`, `win+k` or `meta+k`, the row instead says "Write Win, Super and Option as `option`, and Cmd as `alt`." Settings files only take key-position names so that they mean the same thing on every OS.
- If a key from the settings file is a combo the OS may take first, such as `Win+L` on Windows, the row warns "Windows may use Win+L itself, so Tasty might not receive it." Saving still works.
- **None** leaves the command without a keybinding.
- A command with an unsaved change shows a dot after its name. **Reset** clears your setting and goes back to the plugin's default; it cannot be pressed when there is no setting of yours.
- Changes apply only when you close the window with **Save**; Cancel or closing the window discards them.

## Changing the numbered-switching rule

"Go to n" · "next/previous" for Tabs · Workspaces · categories are grouped under a **one modifier + one key** rule. Change it at the bottom of the **Tab** sub-tab and the bottom of the **Workspace** sub-tab.

- The **Tab switch modifier** · **Workspace switch modifier** · **Category switch modifier** dropdowns — defaults `Ctrl` · `Alt` · `Ctrl+Shift`. Changing one changes all numbered keys in that group at once (10 for tabs · 9 for Workspaces · 10 for categories).
- The slot buttons **Tab 1:** ~ **Tab 10:** and so on — press a single key without a modifier (**Press a key (no modifier)...**). For example, changing slot 1 to `Q` makes `Ctrl+Q` go to Tab 1, and the number badge shown on the Tab while the modifier is held changes to `Q` as well.
- **Next tab:** · **Previous tab:** and so on — likewise a single key. Defaults are vi-style `L`/`H` (Tabs), `J`/`K` (Workspaces · categories).
- Choosing **Custom** in the dropdown abandons the rule and records a completely different combination for each slot (such as `Ctrl+Alt+1`). No number badge is shown in this mode. Going back to the rule mode resets that group to its defaults.

<a id="keybindings-do-nothing-while-a-popup-has-focus"></a>

## Keybindings while typing in a popup

While a popup that takes input — the search bar, the file picker, the command palette — **has
focus, keybindings are paused.** This prevents a shortcut such as `Alt+W` from closing a surface while you type into the search bar. If such a popup is merely open and does not
have focus, keybindings work as usual.

To return to terminal controls, use either of these actions.

- **`Esc`** — releases the focused popup. Popups of the kind that close when you click outside
  them close as well; the others stay open and merely lose focus. Other popups that are open
  but not focused are left alone.
- **Click outside the popup** — same result as `Esc`.

While the settings window or the notification panel is open, `Esc` closes that one first.

## Two things that save you from memorising keybindings

- **Modifier key hints** — Hold `Ctrl` · `Alt` · `Shift` and the like for 0.5 seconds or longer (Shift alone: 1.2 seconds) and a list of keybindings starting with that combination appears below the sidebar. It disappears when you let go. On Windows and Linux, `Win` and `Super` show the list only when at least one keybinding uses that key. Turn it off with **Settings** > **General** > **Accessibility** > **Show modifier key hints**; the panel can be dragged around or resized.
- **Command palette** — `Ctrl+Shift+P` or the palette shortcut keycap on the right of the status bar. Type an action's name and run it with `Enter`. The actions in the Keybindings tab and the global commands of active plugins are searchable. Left out are the command palette itself, **Exit fullscreen stage**, **Copy link** (its target is the link under the mouse), the code field keys, and the Explorer keys for moving between items, extending the selection, opening and clearing the selection (the Explorer's rename and move to trash are included). Script keybindings don't appear in the palette either. The query field keeps a distinct background and border when unfocused, and shows a focus border while you type.

## Editing the settings file directly

Keybindings are stored in the `[keybindings]` section of `~/.tasty/config.toml`. The values are OS-independent names, so the file can be moved to another OS and used as-is.

```toml
[keybindings]
new_tab = ["alt+t"]
copy = ["ctrl+c", "alt+c", "ctrl+shift+c"]
quit = []                      # empty = no keybinding
tab_switch_modifier = "ctrl"   # combinations such as "ctrl+shift" also work
tab_switch_slot_keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
tab_switch_next_key = "l"
tab_switch_prev_key = "h"
```

The `+` key itself is written as `ctrl++` or `ctrl+plus`, `-` as `ctrl+-` or `ctrl+minus`, and `=` as `ctrl+=` or `ctrl+equals`. For editing the file in general see [Settings](settings.md).

<a id="what-to-read-next"></a>

## Keep exploring

- [Settings](settings.md) — The settings window structure and `config.toml`.
- [Panes, tabs, and splits](../using/panes-tabs-splits.md) — What the split · move actions in the tables above actually do.
- [Working in the terminal](../using/terminal.md) — Copy mode · search · mouse capture.
- [Lua scripts](scripts.md) — How to register the scripts that show up in the **Run Scripts** subtab.
