<!-- source-hash: f372da6410bb -->
# Driving terminals with the tasty CLI

Use the `tasty` CLI to create terminals, send commands, and read results. Control a running Tasty from a script, or let an AI agent set up the terminals it needs.

Start by listing terminals, then try sending a command and reading its output. Agents such as Claude Code and Codex use the same commands.

## Prerequisites

- Tasty must be running. The CLI connects to the running instance through the port written in `~/.tasty/tasty.port`.
- Inside a terminal that Tasty opened, `tasty` is already on the PATH. To use it from outside (another terminal app), add the directory containing the Tasty executable to your PATH. The path for each install method is listed in [Install](../getting-started/install.md#install-locations).
- A shell that Tasty opened carries the `TASTY_SURFACE_ID` environment variable. Commands that omit `--surface` use this value, so you do not need to type an ID when driving your own terminal.

```sh
echo $TASTY_SURFACE_ID     # e.g. 42
tasty list info            # version and the queried window’s Workspace count — connection check
```

## Terms and IDs

Tasty's screen is nested as **Workspace > Pane > Tab > Surface**. A Surface is one terminal. Every target in the CLI is addressed directly by this ID — the result is the same no matter which window has focus.

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

`list surface-kinds` shows the surface kinds you can currently create with `--type <kind>`. A kind will be absent if its plugin is not running or the current build does not support it. Each row shows the rendering method and the plugin or built-in feature that provides it.

Each row of `list workspaces` has the form `name (id:N) (pane count)`. The active Workspace is marked `*`, and a remote mirror is marked `[mirror]` ([Working remotely](../remote/attach.md)).

## Basic pattern: mark → send → read since mark

This is the standard procedure for extracting just the result of a single command.

1. `tasty set mark` — leave a marker at the current output position.
2. `tasty send text "command\r"` — send the text. `\r` is Enter.
3. Wait a moment, then `tasty read since-mark --strip-ansi` — read only the output that appeared after the marker.

```sh
tasty set mark --surface 42
tasty send text "cargo test 2>&1 | tail -20\r" --surface 42
sleep 5
tasty read since-mark --surface 42 --strip-ansi
```

- `send text` interprets the `\r` `\n` `\t` `\\` `\0` escapes. Write them as-is inside shell quotes.
- Add `--strip-ansi` to get plain text with colour and other control sequences removed. Always add it when you are going to parse the output.
- A mark stays until you call `set mark` again. `read since-mark` does not move the mark, so reading several times returns the same range.

When you are not sure whether the command has finished, read the screen and check whether the prompt is back.

```sh
tasty read screen --surface 42 --lines 5     # bottom 5 lines of the screen (reaches into scrollback if needed)
tasty is-typing --surface 42                  # whether a person pressed a key in the last 5 seconds
```

### When several agents read the same terminal: read from a position you hold

There is only one mark per Surface, so when several agents watch the same terminal, one agent's `set mark` moves the others' reading window. To stay out of each other's way, **each reader holds its own position**.

```sh
tasty read since-mark --surface 42 --strip-ansi --max-bytes 65536
# keep next_cursor and stream from the reply and pass them to the next call
tasty read since-mark --surface 42 --strip-ansi --cursor 81920 --stream 1a2b-7
```

- The reply (JSON) carries `next_cursor` (where to read next), `stream` (a token for the terminal that position belongs to) and `skipped` (how many bytes left the buffer before you could read them). **Always continue from `next_cursor`** — counting from the length of `text` drifts by however much colour code was stripped.
- Tasty remembers nothing for a read that gives a position, so any number of readers never push each other. The mark does not move either.
- Use `--cursor` only together with `--stream`. If the Surface was closed and another opened under the same number, or its terminal was restarted, the old position is refused with an error instead of being applied — read once without a position to start over.
- A non-zero `skipped` is what disappeared before you read it. The output buffer keeps only the most recent 1 MiB.
- `--max-bytes` lowers how much one read returns. Continue from `next_cursor` for the rest. `0` does not mean "no limit" — it is treated as 1 byte. To read without a limit, leave `--max-bytes` out.
- An older Tasty that does not know these arguments is **not sent the request**. The command then writes one line, `{"error":{"kind":"unsupported_capability",…,"sent":false}}`, to stderr and exits with code 1. Nothing was sent, so after updating Tasty you can simply call it again.

### Bounding how long to wait for a reply

```sh
tasty --response-timeout-ms 5000 read screen --surface 42
```

Put `--response-timeout-ms` **before** the command. If no reply comes within that time, the command ends with `Error (-32061): …`. That error means **the outcome is unknown** — the request may keep running inside Tasty, so if the command changes something, check the state before sending it again. If the time runs out while the request is still waiting its turn inside Tasty, the command ends with `Error (-32067): …` instead. Then **nothing ran**, so you can send it again as is. Without the flag, or with `0`, there is no bound. It only works for commands that send a single request; commands that ask repeatedly, like `events follow`, or open a connection, like remote attach, refuse the flag with exit code 2 and send nothing. An older Tasty that does not understand the bound is not sent the request, with the same `sent:false` refusal as above. That check counts against the same time: if Tasty is stuck and even the check does not finish in time, the request is not sent and the command ends with `Error (-32067): …`, so you can send it again as is.

By default, `read screen` excludes dimmed autocomplete suggestions (for example Claude Code's grey suggestion text). Use `--show-dim` to include them.

If `--lines N` returns fewer lines than requested, check `scrollback_len` in the response. A value of `0` means there is no more scrollback to read, which can happen when you first open a full-screen app (TUI). If scrollback remains but the requested lines are missing, check the output query. `alt_screen` tells you whether a full-screen app is in use.

## Sending keys

Keys other than Enter are sent with `send key`.

```sh
tasty send key enter --surface 42
tasty send key ctrl+c --surface 42
tasty send key escape --surface 42
tasty send key up --surface 42
```

Key names: `enter` `tab` `escape` (or `esc`) `backspace` `delete` `insert` `up` `down` `left` `right` `home` `end` `pageup` `pagedown` `f1`~`f12`. Join combinations with `+`, as in `ctrl+c` `alt+x` `ctrl+shift+c`.

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
tasty split --level surface --target-surface this --direction vertical   # split my Surface left/right
tasty split --level pane --target-pane 3 --direction horizontal          # split a Pane
tasty new tab --pane 3 --cwd ~/proj                     # new Tab in a Pane
tasty close surface --surface 99                        # close a Surface
tasty close tab --tab 12
tasty close workspace --id 3                            # a whole Workspace, tabs and surfaces included
tasty close window --id 1                               # close a window
tasty close self                                        # close this very Surface
```

`--target-surface this` means yourself (`TASTY_SURFACE_ID`). You can also create non-terminal surfaces, for example `--type markdown --file README.md` ([Opening files](../using/files.md)).

A tab opened with `tasty new tab` does not change the tab the person was looking at, whatever its kind. The new tab is added at the end of the pane and stays in the background until the person picks it. The reply's `active_tab` is the tab currently selected in that pane, not the new one, so use the reply's `surface_id` to work with the new tab. A tab the person opens by shortcut or menu is selected right away.

A window opened with `tasty new window` does not take the focus from the window the person was looking at. So a following command with no target (`tasty new workspace` and so on) lands in the window they were looking at, not the new one. To create a workspace in the new window, give a Surface ID from that window with `tasty new workspace --surface <ID>` — `tasty list windows` shows the `workspace_ids` of each window, and `tasty list surfaces` shows the `workspace_id` of each Surface. If no window holds the Surface you give, the command ends with an error instead of landing in another window. Without `--cwd`, the new workspace takes its working directory from the Surface you give (not from the one the person is looking at in that window). The new window appears behind the window the person was looking at and does not take keyboard input (macOS · Windows). On Linux, X11 asks the window manager to do the same, but whether it does is up to the window manager, and on Wayland the compositor decides. Either way, the window that untargeted commands go to does not change.

The last remaining workspace and the last remaining window cannot be closed. Closing a workspace
never takes the window down with it; it is refused instead, so reach for `tasty close window` when
that is what you mean (an instance started with `tasty --headless` has no window to close, so its
last workspace simply cannot be closed). A target holding your own terminal is refused too - use `tasty close self`
there. A workspace mirroring a remote connection is refused as well - end that connection instead.
The other way round, a workspace holding **a terminal someone is using over a remote connection**
is refused too - it closes once that person lets go of it.
Closing a workspace you are not looking at leaves the one on screen where it was.

**Closing a workspace cannot be undone.** Every terminal running inside it ends, it does not come
back from "recently closed", and its scrollback is gone. Only what a person closed by hand can be
restored. Check with `tasty list workspaces` before you close.

## Looking into a surface

```sh
tasty surface cursor-position --surface 42     # which row and column the cursor sits at
tasty surface foreground-process --surface 42  # what is running in front (a shell means idle)
tasty surface mouse-tracking --surface 42      # whether the program grabbed the mouse, and whether tasty honours it
tasty surface locate --surface 42              # the pane it belongs to, and whether it still exists
tasty surface respawn-terminal --surface 42    # restart the shell, keeping the surface in place
tasty surface fire-hook --surface 42 --event process-exit    # fire a hook yourself
tasty surface fire-hook --surface 42 --event idle-timeout:300 # some events carry a number
```

## Not sending while a person is typing

```sh
tasty send text "make test\r" --surface 42 --wait-idle
```

`--wait-idle` decides and sends in one step. Checking with `tasty is-typing` first leaves a gap in
which the person may start typing; this flag closes it. When they are typing nothing is sent and you
get `"sent": false` with the reason.

## Granting permissions to a child agent

```sh
tasty session issue --agent-id build-bot --permission surface.read --permission terminal.write
tasty session list
tasty session revoke --token <token>
```

A child holding the issued token in `TASTY_SESSION_TOKEN` may use exactly the permissions named on it.

Commands that a plugin adds (`tasty markdown recent`, `tasty codex spawn` and so on) need a permission too.
For `tasty <command> …`, add `--permission ipc.invoke:<command>` — `tasty markdown …` needs
`ipc.invoke:markdown`, and a `-` in the command name becomes `_`. Without it the call is refused and a permission approval request goes to a person.
A Claude started with `tasty claude spawn` already holds the permissions for `tasty claude …` and `tasty codex …`.

## Sending notifications

Tell a person when a long task has finished. The notification goes to the notification panel and to the OS notification system.

```sh
tasty notify "Build finished" --title "cargo"
tasty list notifications
```

For notification behaviour in detail and automatic execution (hooks), see [Hooks, notifications and webhooks](hooks-notifications.md).

## Leaving notes on a Surface (metadata)

You can attach key-value pairs to each Surface. Use them when several agents label their roles or exchange state.

```sh
tasty surface-meta set --key role --value builder --surface 42
tasty surface-meta get --key role --surface 42
tasty surface-meta list --surface 42
tasty surface-meta unset --key role --surface 42
```

## Passing messages between Surfaces (queue)

A queue that passes messages between Surfaces without touching terminal input.

```sh
tasty send queue --to 42 "Tests done, please check the results"
tasty list queue --surface 42            # pending count and preview
tasty read queue --surface 42            # pop the oldest message
tasty read queue --surface 42 --peek     # look without popping
tasty read queue --surface 42 --clear    # empty everything
```

<a id="running-child-terminals-like-agents"></a>

## Running tasks in several terminals

Open additional terminals in a workspace to run commands, send input, and track the work. You can use this with **any program**, including Claude and Codex. Their [dedicated commands](claude-codex.md) also provide session management.

```sh
tasty terminal spawn --workspace build --command "cargo watch -x test\r" --cwd ~/proj --role worker
tasty terminal children                        # children under me
tasty terminal tell "y\r" --surface 57         # send input to a child (line breaks kept, submitted automatically)
tasty terminal broadcast "git pull\r" --role worker   # to every child with the same role
tasty terminal kill --child 1                   # kill a child by index
```

`spawn` creates a terminal and returns immediately. Agent state delivery and parent setup follow [Claude & Codex integration](claude-codex.md). Starting an ordinary program does not guarantee delivery of agent results.
There is no separate command to wait on. Tag children with `--role` to address them together with `broadcast`.

If `terminal spawn` fails during setup, it sends no command and removes the terminal created by that call. Existing terminals remain open.

## Headless PTY

Run a program on a real PTY (pseudo-terminal) with no tab and no screen. Use it when a command needs a TTY but you drive it from a script without taking up screen space. Feed input and read the screen by the id that `spawn` returns.

```sh
tasty pty spawn --cwd ~/proj -- python3         # start a command on a PTY and get its id
tasty pty write --id 3 $'print(1+1)\n'           # send to stdin (a newline submits)
tasty pty read --id 3 --lines 20                # the last 20 lines on the screen right now
tasty pty list                                  # PTYs that are up
tasty pty kill --id 3                            # stop it
```

## Memory shared between agents

A key-value store where several agents in the same Tasty exchange values. Pick a scope (global · surface · workspace · window · account) to store under, and optionally let entries expire (TTL) or guard against overwrites (CAS).

```sh
tasty memory put --workspace 7 --key build/status --value running --ttl 600
tasty memory get --workspace 7 --key build/status
tasty memory list --workspace 7 --prefix build/
tasty memory delete --workspace 7 --key build/status
```

Switch scope with `--global` · `--surface 3` · `--window 42` · `--account me`. A value that parses as JSON is stored as JSON, otherwise as a string.

If Tasty cannot open its memory file (`~/.tasty/memory.db`) when it starts — the file is damaged or not accessible — Tasty does not stop; it keeps running on **temporary memory**. Values stored in that state are gone after Tasty restarts. This also applies to everything else kept in the same file: agent tasks and coordination tools such as semaphores, approvals, surface metadata, telemetry, and sessions. While it lasts, every such store result also carries `"durable": false`, and `db_pragmas.memory_db` in `tasty list pressure` shows `degraded: true` with the cause (`init_failure`). No notice appears on screen.

## Pulling signals out of the output (observers)

Watch the output as it scrolls past and collect only the **structured signals** — paths · URLs · exit codes · prompt boundaries. Use it so a script can react to those signals without a person watching the screen.

```sh
tasty output observe start --surface 42 --parsers exit_code,url --sink file
tasty output observe list                        # observers running now
tasty output observe info --observer 1           # one observer's state and collected count
tasty output observe stop --observer 1
```

`--sink memory` collects into an in-memory ring buffer, `--sink file` into a file. Leave `--parsers` empty and the default parsers (paths · URLs · prompt boundaries · exit codes) are all on.

## Measuring agent activity

Several agents record their own activity as numbers (token counts · call counts and so on), and you look at it as sums · time series · top rankings. Use it to review activity and usage across your agents.

```sh
tasty telemetry record --metric tokens --value 1200 --tags '{"model":"opus"}'
tasty telemetry summary --metric tokens           # sum and count
tasty telemetry top --by agent --metric tokens    # top by agent
tasty telemetry timeseries --metric tokens --window 1h
```

`record` attributes the call to the caller automatically (`TASTY_AGENT_ID`). To insert several values at once with their order preserved, use `tasty telemetry record-batch`.

## Other queries and settings

Things an agent reaches for occasionally. `tasty <command> --help` lists them all.

```sh
tasty list pressure                    # where the time went while answering requests
tasty list theme                       # the theme snapshot in effect (colors, font sizes, UI scale)
tasty list recent --kind markdown      # files recently opened as that kind
tasty set cwd --surface 42 --path /tmp # change the working directory a remote surface reports
tasty set url --surface 42 --url URL   # change the address of a webview surface
tasty file-handler dispatch PATH       # open a file the same way a double-click in the explorer does
tasty file-handler reload              # read the file handler settings file again
```

`set cwd` and `set url` only apply to a remote surface and a webview surface respectively. Using them on a regular terminal surface returns an unsupported-target error.

`file-handler dispatch` accepts file paths only. Passing a web address such as `https://…` returns an error. A headless build, which runs Tasty on a server without a GUI, cannot open files, so instead of reporting the request as accepted it returns an error saying this build does not support it.

The `file-handler reload` response has a `rejected` list. It holds the `id` and the reason (`reason`) of each entry from the settings file that is not applied right now, and it is empty when everything applied. There are three reasons.

- `missing_owner_prefix` — the `id` has no owner part such as `user/` in front of it (write it as `user/name`). The entry is dropped.
- `missing_detector_or_action` — an entry you made yourself (`user/…`) is missing either which files it handles (`detector`) or what to do (`action`). The entry is dropped.
- `target_not_contributed` — the entry changes a built-in or plugin handler, but that handler is not there right now. Either the plugin is off or the `id` is wrong. The entry is kept, and it applies as soon as the plugin is on. If it is still listed after you turn the plugin on, check the `id`.

The workspace count and active index in `list info` describe the queried window. The returned workspace IDs identify its scope. Use `list workspaces` for the global inventory and `list windows` for each window’s state.

Use `list pressure` when responses feel slow and you need to tell why. The answer comes in eleven blocks that **count different things**: `queue_before_gate` is how long commands waited in the queue, so it also counts requests that were rejected afterwards, `handler_after_gate` counts only the ones that actually ran, `plugin_round_trip` is how long Tasty waited for a plugin to answer (counting only the requests that were answered), and `db` is how long it took for what was written to settle on disk. A large wait means the instance is backed up, a large handler time means the command itself is heavy, a large round trip means the time was spent inside the plugin, and a large db time means the disk is slow. Do not subtract one from the other to get a rejection count — it does not work that way. The numbers are totals since this instance started, and an average with nothing behind it comes back as `null`.

The fifth block, `connections`, counts **seats rather than time**. The other four all answer "how long did it take"; this one answers "is there room to connect". Nothing has to be slow for it to matter — once the number of simultaneous connections is full, a new one is refused before it ever gets an answer. `live` is how many are attached right now and is the only count or current value in this block that goes down (the accept wait mean is derived, so it can fall too); `live_max` is the highest it has been since startup, `limit` is the cap Tasty enforces, and `accepted` and `refused_saturated` are the running totals of connections let in and turned away at that cap. If `live` sits at `limit`, or `refused_saturated` is climbing, nothing is slow — **there is no room**, and the fix is to drop connections you are not using (a long-lived attach, for instance). What is counted here is connections, not requests, so a connection that never sends a request still takes a seat. This block also carries one time — `accept_wait_bound_us_max` and `accept_wait_bound_us_mean` (with their count `accept_waits`) are the **longest** a new connection may have waited before Tasty took it in. Tasty checks for new connections at most every 100 ms, so a tool that connects anew for every command (such as the `tasty` CLI) can wait that long before its request is read, and that time shows up in no other block. It is a bound that the real wait cannot exceed, not the exact wait, so the real wait is usually shorter.

The three blocks that measure time also carry a **distribution** next to the mean and the max (`wait_us_hist`, `us_hist`). "Everything is a little slow" and "most calls are fast but a few spike" can produce the same mean and the same max, and they call for opposite work — the first is a capacity problem, the second is a hunt for what those few calls were. A distribution comes as the list of bucket upper bounds (`bounds_us`, eleven of them from 10 µs to 1 s) and the count in each bucket (`counts`). `counts` is one entry longer because **the last bucket holds everything past 1 s**; it has no upper bound, so how far past is answered by `us_max` in the same block. The buckets do not overlap, so adding them all up gives the number of observations. In `queue_before_gate` that number also comes as `waits` (the commands whose wait was measured), and the average wait is divided by it. The `commands` value in the same block goes up only when a batch of commands taken out together finishes, so at the moment you ask it can be smaller than `waits` by the batch being handled right now. No percentiles such as p99 are provided — that is a value only knowable to bucket resolution, and handing it over as a single number would claim precision that is not there. No other block carries a distribution.

The sixth block, `db_pragmas`, is **neither time nor seats — it is the settings of the two databases Tasty uses**. For each one it shows the setting that was requested when the database was opened (`requested`) next to the value that actually took (`effective`), because a request can be refused silently and asking for a setting does not mean it is in effect. `memory_db` and `state_db` get one entry each, and `degraded` is `true` when at least one setting did not take. The database is still in use; it may just be slower or less safe across a crash. A `null` `state_db` means that database is not open in this instance, which is always the case for an instance running without windows. This block is not a running total; it is fixed when the database opens. `init_failure` under `memory_db` carries the cause (`cause`) and the error text (`error`) when Tasty could not open its memory file at start and runs on temporary memory instead, and is `null` otherwise. In that case `degraded` is `true` even if every setting took.

The seventh block, `stream_push`, counts **what Tasty pushed rather than requests it answered**: the pieces of screen data sent over long-lived connections such as a remote attach. `frames_dropped` is how many pieces were thrown away because the receiving side could not keep up while the connection stayed open, and `clients_lagged_out` is how many connections were cut for falling too far behind — both are totals since startup and only go up. `backlog` is how many pieces are waiting to be sent right now and can go down, and `sink_capacity` is how many one connection can hold. A climbing `frames_dropped` means some attach missed part of the screen; Tasty's attach clients re-attach on their own when told so and fetch the screen again.

The eighth and ninth blocks, `queue_admission` and `queue_dispatch`, are **the two ends of the queue commands line up in**. `queue_admission` is what is in the queue right now (`queued_bytes` in bytes, `queued_commands`, and `queued_injected`, the ones Tasty put there itself), the highest byte count since startup (`peak_bytes`), and how many requests were **turned away before they got in** because the queue was full — `refused_bytes` at the byte cap, `refused_depth` at the cap on Tasty's own internal commands. The caps to read them against come along (`limit_bytes`, `limit_injected_depth`). A turned-away request never entered the queue, so it shows up in no other block; if requests come back with a "queue is full" error, look here. `queue_dispatch` is **the side that takes commands out**: how many times it did (`rounds`) and how many of those stopped because they used up their share of commands or time (`rounds_stopped_by_count`, `rounds_stopped_by_time`), commands not run because the wait the caller allowed ran out while they queued (`expired_before_run`), commands started (`started`), and the requests running right now whose caller is still waiting (`in_flight`, which can go down) with its highest value (`in_flight_max`). A climbing `rounds_stopped_by_time` or `expired_before_run` means the side taking commands out is falling behind.

The tenth block, `keyed_requests`, counts **only requests sent with an idempotency key (`idempotency_key`)**. Each slot is one way Tasty handled a request under a key: `executed` is a new key, so it ran; `replayed` is the same request again, answered with the earlier answer without running it; `conflicted` is different content under the same key, so nothing ran; `discarded` means it ran but its answer was thrown away; `in_flight` means the same request was still running and this one waited for its result. Each request is counted once. A `replayed` that is large next to `executed` means retries are piling up. This `in_flight` is a different value from the one in `queue_dispatch`.

The eleventh block, `slow_requests`, shows **single slow requests instead of totals**. Each request whose time waiting in the queue, time being handled by Tasty and time waiting for a plugin add up to `threshold_us` (100 ms) or more gets one row, oldest first, up to `capacity` (32) rows — when it overflows the oldest rows are pushed out, and `admitted` counts every row kept since the instance started, so the difference from the rows shown is how many were pushed out. A row carries `request_seq` (a number Tasty gives every request — not the request's `id`), `host` (the command name `method`, the sender `caller`, the queue wait `queue_wait_us`, the handling time `host_us`, and the answer the caller got: `outcome` `ok`/`error` with the error code `error_code`, both `null` while no answer has gone out yet), `plugin_hops` when the request was forwarded to a plugin (the plugin, the request number that plugin received `host_request_id`, how long Tasty waited and the outcome `ok`/`error`/`expired`/`cancelled`), and the sum `total_us`. Where the distributions above tell you how many requests took over 100 ms, this block tells you **where the time of that one request went** — a large wait in `plugin_hops` means the request was inside the plugin, and the plugin's log has the same request id as `host_request_id`. Tasty's own log puts `id=` and `request_seq=` on the same line when a plugin answers with an error or never answers. Request contents and tokens are never kept, and the query for this answer is not kept itself. It lives in memory only, so it is empty after a restart and the numbers start again from 1.

`list info` also answers what this Tasty can do, under `capabilities`. Each entry pairs a name with a version, and it tells you what the version string alone cannot — the same version can do different things depending on how it was built. Ignore any name you do not recognise.

## Frequently used commands

| What you want | Command |
|---|---|
| Show the hierarchy | `tasty list tree` |
| Surface list | `tasty list surfaces` |
| Surface kinds you can create | `tasty list surface-kinds` |
| Send text (including Enter) | `tasty send text "ls\r" --surface ID` |
| Send a key | `tasty send key enter --surface ID` |
| Set a mark | `tasty set mark --surface ID` |
| Read since the mark | `tasty read since-mark --surface ID --strip-ansi` |
| Continue reading from a held position | `tasty read since-mark --surface ID --cursor N --stream S` |
| Bound the reply wait | `tasty --response-timeout-ms MS <command>` |
| Read the screen | `tasty read screen --surface ID --lines N` |
| Notification | `tasty notify "body" --title "title"` |
| Screenshot | `tasty screenshot --path out.png [--surface ID] [--window ID]` |
| Help | `tasty --help`, `tasty <command> --help`, `tasty -a -h` (full tree) |

## Troubleshooting

- **Cannot connect** — check that Tasty is running and that the `~/.tasty/tasty.port` file exists. If the file is there but the connection fails, the previous instance exited abnormally ([Troubleshooting](../help/troubleshooting.md)).
- **Calling without `--surface` is rejected** — in a shell without `TASTY_SURFACE_ID` (outside Tasty) there is no target Surface, so the command ends in an error. Tasty never guesses the focused one: the same command gives the same result no matter which window is in front. Always write `--surface` in scripts.
- **`read since-mark` is empty** — either the output finished before you set the mark, or the command has not finished yet. Check the current state with `read screen`.
- **An error line containing `"sent":false`** — the Tasty you are connected to is an older version that does not know that feature (reading from a position, bounding the reply wait, and so on). The request was not sent. The name under `capability` says what is missing.
- **A `data: {…}` line follows the `Error (…)` line** — Tasty sent a classification of the failure along with it (for example `storage_failure` when a memory write fails, or `reason` for a refusal). Everything after `data: ` is one line of JSON, so a script can branch on that value instead of the first line. The first line, `Error (code): message`, is the same whether or not this line is present. `tasty events follow` and `tasty plugin audit-follow` add this line too; their first line keeps its shape, `Error: Error (code): message`, with one extra `Error: ` in front.
- **The command ends with `Error (-32065): …`** — Tasty has a backlog of requests to handle and did not take this one. The request did not run, so call it again as is after a short pause.
- **The command ends with `Error (-32066): …`** — nothing was sent within 20 seconds of connecting, so Tasty closed the connection. Nothing ran. If your tool opens the socket itself, send the request right after connecting. A connection that has sent one request is not closed however long it pauses between requests.
- **Not sure which window `screenshot` captures** — automatic selection counts **main (terminal) windows only**. With one main window open, omitting `--window` captures it; with several, `--window` is required (it never picks whichever window happens to be focused). Windows that `list windows` does not show, such as the settings window, are not counted: `--window` stays optional while the settings window is up, and capturing the settings window itself means naming its ID with `--window`.

<a id="what-to-read-next"></a>

## Keep exploring

- [Claude · Codex](claude-codex.md) — Spawning child agents and receiving completion notifications.
- [Task workflows](tasks.md) — Tying several pieces of work together by dependency.
- [Hooks · notifications · webhooks](hooks-notifications.md) — Running commands automatically on an event.

For agents using session tokens, call limits apply to plugin commands and combined list queries as well as ordinary commands. Each admitted request is counted once; requests over the limit return an error without running. The existing exemption for local CLI calls without a token remains.

A plugin namespace call starts only its enabled owner and any active extension needed for matching IPC hooks. It does not enable a disabled plugin or start unrelated plugins. An unknown namespace starts none; a misspelled method inside a known namespace may start its owner before returning an error.
