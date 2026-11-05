<!-- source-hash: c3bcf52a4676 -->
# Driving terminals with the tasty CLI

Use the `tasty` CLI to create and control terminals in a running Tasty instance. Check the connection and target IDs, send a command, then use `read since-mark` to read output after a saved mark. Scripts and AI agents can use the same commands.

Follow this page in order the first time. Check the connection, find target IDs, create a working terminal, send a command, read its output, and clean up. The remaining commands, such as reading from a saved position, metadata, queues and memory, are in the [CLI command reference](cli-reference.md). Response delays, connections, and what to do for each error code are in [CLI response delays and connection diagnostics](../help/cli-diagnostics.md).

## Prerequisites

- Tasty must be running. The CLI connects to the running instance through the port written in `~/.tasty/tasty.port`.
- Inside a terminal that Tasty opened, `tasty` is already on the PATH. To use it from outside (another terminal app), add the directory containing the Tasty executable to your PATH. The path for each install method is listed in [Install](../getting-started/install.md#install-locations).
- A shell that Tasty opened carries the `TASTY_SURFACE_ID` environment variable. Most commands that omit `--surface` use this value (`tasty new workspace` · `tasty output observe start` do not), so you do not need to type an ID when driving your own terminal.
- When the Codex plugin is installed, inside a command that Codex runs (an environment with `CODEX_THREAD_ID`), that shell’s `TASTY_SURFACE_ID` may belong to another terminal, so commands added by plugins (for example `tasty codex …` and `tasty claude …`) do not fill `--surface` from it. Pass `--surface` when a target is needed. If the `CODEX_THREAD_ID` value is not valid, `tasty codex …` and `tasty claude …` do not run, while other plugin commands run as usual.
- If you run Tasty from a terminal of another Tasty, the shells and hook commands the new Tasty starts do not inherit the outer Tasty’s session token (`TASTY_SESSION_TOKEN`) or agent ID (`TASTY_AGENT_ID`). `tasty` commands run there reach the new Tasty as a regular user.

```sh
echo $TASTY_SURFACE_ID     # e.g. 42
tasty list info            # version and the queried window’s Workspace count — connection check
```

## Terms and IDs

Tasty's screen is nested as **Workspace > Pane > Tab > Surface**. A Surface displays content such as a terminal or a Markdown document. A command with an explicit target ID uses that target regardless of which window has focus.

```sh
tasty list tree            # whole hierarchy as a tree
tasty list workspaces      # Workspace list
tasty list surfaces        # Surface (terminal) list — across all Workspaces
tasty list panes           # Pane list
tasty list tabs --pane 3   # Tabs of a specific Pane
tasty list surface-kinds   # Surface kinds this instance actually registered
```

`list tree` also shows the split structure. The focused Surface is marked `*focus`.

```
└─ vertical (L|R) 60:40
   ├─ surface:396 (terminal)
   └─ horizontal (T|B) 50:50
      ├─ surface:417 (terminal) *focus
      └─ surface:418 (markdown)
```

`list surface-kinds` shows the surface kinds you can currently create with `--type <kind>`. A kind will be absent if its plugin is not running or the current build does not support it. Disabling or removing a plugin drops its kinds right away, and a request to create one of them fails with an error saying the plugin that provides it is turned off or has not reconnected yet. Each row shows the rendering method and the plugin or built-in feature that provides it.

Each row of `list workspaces` has the form `name (id:N) (pane count)`. The active Workspace is marked `*`, and a remote mirror is marked `[mirror]` ([Working remotely](../remote/attach.md)).

## First walkthrough: run a command in a working terminal

Do not send commands to a terminal the person is using. Create a separate working tab and use its ID. The `3` and `57` below are placeholders; use the values from the replies of the previous commands.

```sh
tasty list panes                               # find a Pane ID for the new tab (e.g. 3)
tasty new tab --pane 3 --cwd ~/proj            # the reply's surface_id is the new terminal (e.g. 57)
tasty set mark --surface 57                    # mark the current output position
tasty read last-command --surface 57           # note ended_at before sending
tasty send text "ls\r" --surface 57            # send a command (\r is Enter)
tasty read last-command --surface 57           # a changed ended_at means the command finished
tasty read since-mark --surface 57 --strip-ansi # read the output after the mark
tasty close surface --surface 57               # clean up the terminal you made
```

- A tab made with `new tab` does not change the tab the person was looking at. Use the reply's `surface_id`, not `active_tab`.
- Do not decide that a command finished by waiting a fixed time or by seeing empty output. In bash and zsh, shell integration records each command's end, so compare `ended_at` in the `read last-command` reply before and after sending, and check `exit_code`. In a shell without integration, check with `read screen` that the prompt is back.
- To see every argument of one command, run `tasty help <command>`, for example `tasty help read since-mark`.

## Frequently used commands

| What you want | Command |
|---|---|
| Show the hierarchy | `tasty list tree` |
| Surface list | `tasty list surfaces` |
| Create a working tab | `tasty new tab --pane ID` |
| Send text (including Enter) | `tasty send text "ls\r" --surface ID` |
| Send a key | `tasty send key enter --surface ID` |
| Set a mark | `tasty set mark --surface ID` |
| Read since the mark | `tasty read since-mark --surface ID --strip-ansi` |
| Exit record of the last command | `tasty read last-command --surface ID` |
| Read the screen | `tasty read screen --surface ID --lines N` |
| Close a Surface | `tasty close surface --surface ID` |
| Notification | `tasty notify "body" --title "title"` |
| Screenshot | `tasty screenshot --path out.png [--surface ID] [--window ID]` |
| Help | `tasty help`, `tasty help <command>` (= `tasty <command> --help`), `tasty -a` (every command path with a short description; arguments are in each command’s `--help`) |

`tasty help` groups commands by purpose, such as basic operations, agent collaboration, and background and remote, and starts with a short first-task walkthrough. `-h` shows short descriptions; `--help` and `tasty help <command>` show the long ones. The last "Internal integration" group in the Claude and Codex help lists commands that hooks call, so you do not type them yourself.

## Basic pattern: mark → send → read since mark

Set a marker before sending a command, then read the output after it. Other output from the same terminal may be included too.

1. `tasty set mark` — leave a marker at the current output position.
2. `tasty send text "command\r"` — send the text. `\r` is Enter.
3. Once the command has finished, `tasty read since-mark --strip-ansi` — read only the output that appeared after the marker.

```sh
tasty set mark --surface 42
tasty send text "cargo test 2>&1 | tail -20\r" --surface 42
# check that it finished with read last-command or read screen, as in the first walkthrough below
tasty read since-mark --surface 42 --strip-ansi
```

- `send text` interprets the `\r` `\n` `\t` `\\` `\0` escapes. Write them as-is inside shell quotes.
- Add `--strip-ansi` to get plain text with colour and other control sequences removed. Always add it when you are going to parse the output.
- A mark stays until you call `set mark` again. `read since-mark` does not move the mark, so reading several times returns the same range.

When you are not sure whether the command has finished, read the screen and check whether the prompt is back.

```sh
tasty read screen --surface 42 --lines 5     # bottom 5 lines of the screen (reaches into scrollback if needed)
tasty is-typing --surface 42                  # whether a person pressed a key or pasted in the last 5 seconds
```

By default, `read screen` excludes dimmed autocomplete suggestions (for example Claude Code's grey suggestion text). Use `--show-dim` to include them. This applies to every dimmed character, not only suggestions. If `dim_omitted_rows` in the response is greater than 0, some rows changed because dimmed text was left out, so read again with `--show-dim` and compare.

On a mirror terminal opened through a remote connection (attach), `read screen`, `read since-mark` and `set mark` read the original terminal on the remote computer. One response may carry less output, so continue with `next_cursor` from the response. If the remote Tasty does not support this or the connection is down, the command answers with an error; update the remote Tasty to the same version or read again after it reconnects.

If `--lines N` returns fewer lines than requested, check `scrollback_len` in the response. A value of `0` means there is no more scrollback to read, which can happen when you first open a full-screen app (TUI). If scrollback remains but the requested lines are missing, check the output query. `alt_screen` tells you whether a full-screen app is in use.

## Sending keys

Keys other than Enter are sent with `send key`.

```sh
tasty send key enter --surface 42
tasty send key ctrl+c --surface 42
tasty send key escape --surface 42
tasty send key up --surface 42
```

Key names: `enter` `tab` `escape` (or `esc`) `backspace` `delete` `insert` `up` `down` `left` `right` `home` `end` `pageup` `pagedown` `f1`~`f12`. Join combinations with `+`, as in `ctrl+c` `alt+x`. `shift+` is accepted but ignored.

## With shell integration: reading per command

Tasty sets up shell integration for bash / zsh automatically, so you can query the commands that ran and their exit codes on a per-command basis.

```sh
tasty read commands --surface 42       # list of recorded commands
tasty read last-command --surface 42   # last command (command string and exit code)
tasty read command-at --surface 42 --index -1   # first from the end (0-based, negative counts from the end)
```

Other shells such as fish return an empty list unless you install shell integration yourself.

## Creating and closing terminals

```sh
tasty new workspace --name build --cwd ~/proj          # new Workspace
tasty new workspace --surface 42                       # new Workspace in the window that holds Surface 42 (working directory from Surface 42 too)
tasty new window                                        # new window (the reply carries window_id)
tasty split --level surface --surface this --direction vertical  # split my Surface left/right
tasty split --level pane --pane 3 --direction horizontal          # split a Pane
tasty new tab --pane 3 --cwd ~/proj                     # new Tab in a Pane
tasty close surface --surface 99                        # close a Surface
tasty close tab --tab 12
tasty close workspace --workspace 3                     # a whole Workspace, tabs and surfaces included
tasty close window --window 1                           # close a window
tasty close self                                        # close this very Surface
```

The older forms `split --target-surface` · `--target-pane` and `close workspace|window --id` are still accepted. Giving both forms at once is refused as the same option given twice.

`--surface this` means yourself (`TASTY_SURFACE_ID`). You can also create non-terminal surfaces, for example `--type markdown --file README.md` ([Managing and viewing files](../using/files.md)). The `--cwd` of `new workspace` · `new tab` · `split` works for an Explorer (`--type explorer`) too: without `--path` it opens that folder as the root. With both, `--path` wins. An Explorer does not inherit another surface's working directory when `--cwd` is omitted; it opens your home folder.

A tab opened with `tasty new tab` does not change the tab the person was looking at, whatever its kind. The new tab is added at the end of the pane and stays in the background until the person picks it. The reply's `active_tab` is the tab currently selected in that pane, not the new one, so use the reply's `surface_id` to work with the new tab. A tab the person opens by shortcut or menu is selected right away.

A window opened with `tasty new window` does not take the focus from the window the person was looking at. So a following command with no target (`tasty new workspace` and so on) lands in the window they were looking at, not the new one.

To create a workspace in the new window, give a Surface ID from that window with `tasty new workspace --surface <ID>` — `tasty list windows` shows the `workspace_ids` of each window, and `tasty list surfaces` shows the `workspace_id` of each Surface. If no window holds the Surface you give, the command ends with an error instead of landing in another window. Without `--cwd`, the new workspace takes its working directory from the Surface you give (not from the one the person is looking at in that window).

The new window appears behind the window the person was looking at and does not take keyboard input (macOS · Windows). On Linux, X11 asks the window manager to do the same, but whether it does is up to the window manager, and on Wayland the compositor decides. Either way, the window that untargeted commands go to does not change.

The last remaining workspace and the last remaining window cannot be closed. Closing a workspace
never takes the window down with it; it is refused instead, so reach for `tasty close window` when
that is what you mean (a [headless build](../getting-started/install.md#headless-build) has no window to close, so its
last workspace simply cannot be closed). A target holding your own terminal is refused too - use `tasty close self`
there. A workspace mirroring a remote connection is refused as well - end that connection instead.
The other way round, a workspace holding **a terminal someone is using over a remote connection**
is refused too - it closes once that person lets go of it.
Closing a workspace you are not looking at leaves the one on screen where it was.

**Closing a workspace cannot be undone.** Every terminal running inside it ends, it does not come
back from "recently closed", and its scrollback is gone. Only what a person closed by hand can be
restored. Check with `tasty list workspaces` before you close.

## Not sending while a person is typing

```sh
tasty send text "make test\r" --surface 42 --wait-idle
```

`--wait-idle` decides and sends in one step. Checking with `tasty is-typing` first leaves a gap in
which the person may start typing; this flag closes it. When they are typing nothing is sent and you
get `"sent": false` with the reason.

## Troubleshooting

- **Cannot connect** — check that Tasty is running and that the `~/.tasty/tasty.port` file exists. The file may be left over from an earlier run, or the CLI may be checking another instance’s path ([Troubleshooting](../help/troubleshooting.md)).
- **Calling without `--surface` is rejected** — in a shell without `TASTY_SURFACE_ID` (outside Tasty) there is no target Surface, so the command ends in an error. Tasty never guesses the focused one: the same command gives the same result no matter which window is in front. Always write `--surface` in scripts.
- **`read since-mark` is empty** — either the output finished before you set the mark, or the command has not finished yet. Check the current state with `read screen`.
- **Not sure which window `screenshot` captures** — automatic selection counts **main (terminal) windows only**. With one main window open, omitting `--window` captures it; with several, `--window` is required (it never picks whichever window happens to be focused). Windows that `list windows` does not show, such as the settings window, are not counted: `--window` stays optional while the settings window is up, and capturing the settings window itself means naming its ID with `--window`.

If replies are slow or a command ends with an `Error (…)` line, see [CLI response delays and connection diagnostics](../help/cli-diagnostics.md) for what to do for each error code.

## Finding other commands

Commands this introduction does not cover are on pages organised by purpose.

- <a id="when-several-agents-read-the-same-terminal-read-from-a-position-you-hold"></a>[When several agents read the same terminal: read from a position you hold](cli-reference.md#when-several-agents-read-the-same-terminal-read-from-a-position-you-hold)
- <a id="looking-into-a-surface"></a>[Looking into a surface](cli-reference.md#looking-into-a-surface)
- <a id="granting-permissions-to-a-child-agent"></a>[Granting permissions to a child agent](cli-reference.md#granting-permissions-to-a-child-agent)
- <a id="sending-notifications"></a>[Sending notifications](cli-reference.md#sending-notifications)
- <a id="leaving-notes-on-a-surface-metadata"></a>[Leaving notes on a Surface (metadata)](cli-reference.md#leaving-notes-on-a-surface-metadata)
- <a id="passing-messages-between-surfaces-queue"></a>[Passing messages between Surfaces (queue)](cli-reference.md#passing-messages-between-surfaces-queue)
- <a id="running-child-terminals-like-agents"></a><a id="running-tasks-in-several-terminals"></a>[Running tasks in several terminals](cli-reference.md#running-tasks-in-several-terminals)
- <a id="headless-pty"></a>[Headless PTY](cli-reference.md#headless-pty)
- <a id="memory-shared-between-agents"></a>[Memory shared between agents](cli-reference.md#memory-shared-between-agents)
- <a id="pulling-signals-out-of-the-output-observers"></a>[Pulling signals out of the output (observers)](cli-reference.md#pulling-signals-out-of-the-output-observers)
- <a id="measuring-agent-activity"></a>[Measuring agent activity](cli-reference.md#measuring-agent-activity)
- <a id="other-queries-and-settings"></a>[Other queries and settings](cli-reference.md#other-queries-and-settings)
- <a id="bounding-how-long-to-wait-for-a-reply"></a>[Bounding how long to wait for a reply](../help/cli-diagnostics.md#bounding-how-long-to-wait-for-a-reply)
- <a id="response-delays-and-connection-status"></a><a id="connection-counts-and-limits-connections"></a><a id="timing-distributions"></a><a id="database-settings-db_pragmas"></a><a id="screen-delivery-stream_push"></a><a id="request-queues-queue_admission-queue_dispatch"></a><a id="duplicate-requests-keyed_requests"></a><a id="individual-slow-requests-slow_requests"></a><a id="refusal-reasons-gate_refusals"></a>[Response delays and connection status](../help/cli-diagnostics.md#response-delays-and-connection-status)

<a id="what-to-read-next"></a>

## Keep exploring

- [Claude · Codex](claude-codex.md) — Spawning child agents and receiving completion notifications.
- [Task workflows](tasks.md) — Tying several pieces of work together by dependency.
- [Hooks, notifications, and webhooks](hooks-notifications.md) — Running commands automatically on an event.
- [CLI command reference](cli-reference.md) — Finding commands beyond the introduction by purpose.
- [CLI response delays and connection diagnostics](../help/cli-diagnostics.md) — Response delays, connection counts, and what to do for each error code.
