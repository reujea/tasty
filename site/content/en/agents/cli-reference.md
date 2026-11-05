<!-- source-hash: 6734368d217b -->
# CLI command reference

Find commands beyond the introduction, including `read since-mark`, surface state, message queues, and shared memory, organized by purpose. Use `tasty help` for the full list of arguments and options.

## Finding commands

```sh
tasty help                    # commands grouped by purpose, with a first-task walkthrough
tasty -a                      # every command path with a short description
tasty help read since-mark    # the long description of one command (= tasty read since-mark --help)
```

`-h` shows short descriptions; `--help` and `tasty help <command>` show the long ones. Options that pick a target are named after the kind of target, such as `--surface`, `--pane` and `--workspace`. Older forms (`--workspace-id`, `--target-surface` and so on) are still accepted as aliases and are listed in the help.

## When several agents read the same terminal: read from a position you hold

There is only one mark per Surface, so when several agents watch the same terminal, one agent's `set mark` moves the others' reading window. To stay out of each other's way, **each reader saves its own position**.

```sh
tasty read since-mark --surface 42 --strip-ansi --max-bytes 65536
# keep next_cursor and stream from the reply and pass them to the next call
tasty read since-mark --surface 42 --strip-ansi --cursor 81920 --stream 1a2b-7
```

- The reply (JSON) carries `next_cursor` (where to read next), `stream` (a token for the terminal that position belongs to) and `skipped` (how many bytes left the buffer before you could read them). **Always continue from `next_cursor`** — counting from the length of `text` drifts by however much colour code was stripped.
- Tasty does not store a read position per reader. Each request uses the position you send, without changing another reader’s position or the shared mark.
- Use `--cursor` only together with `--stream`. If the Surface was closed and another opened under the same number, or its terminal was restarted, the old position is refused with an error instead of being applied — read once without a position to start over.
- A non-zero `skipped` is what disappeared before you read it. The output buffer keeps only the most recent 1 MiB.
- `--max-bytes` lowers how much one read returns. Continue from `next_cursor` for the rest. `0` does not mean "no limit" — it is treated as 1 byte. To read without a limit, leave `--max-bytes` out.
- An older Tasty that does not know these arguments is **not sent the request**. The command then writes one line, `{"error":{"kind":"unsupported_capability",…,"sent":false}}`, to stderr and exits with code 1. Nothing was sent, so after updating Tasty you can simply call it again.

## Looking into a surface

```sh
tasty surface cursor-position --surface 42     # which row and column the cursor sits at
tasty surface foreground-process --surface 42  # what is running in front (is_shell true means the shell, so idle)
tasty surface mouse-tracking --surface 42      # whether the program grabbed the mouse, and whether tasty honours it
tasty surface locate --surface 42              # the pane it belongs to, and whether it still exists
tasty surface respawn-terminal --surface 42    # restart the shell, keeping the surface in place
tasty surface fire-hook --surface 42 --event process-exit    # fire a hook yourself
tasty surface fire-hook --surface 42 --event idle-timeout:300 # some events carry a number
```

`respawn-terminal` replies only after the previous shell is confirmed to have exited, even though the new shell already appears.
If that exit is not confirmed within 5 seconds, the new shell stays usable but the reply is a recovery-required error (`-32068`).
After that error, sending the request again with the same idempotency key (`idempotency_key` on the IPC request) returns the success result, because the shell replacement already finished.
When a command that creates a terminal, workspace, or tab fails while starting the shell and it is unknown what actually ran, it replies with a recovery-required error (`-32068`) instead of waiting. Sending it again with the same idempotency key (`idempotency_key` on the IPC request) does not run it again and returns the same error.
When the shell fails before its process is created, for example because the configured shell file is not executable, or the path has no file and no other shell was found to use instead, the command replies with an ordinary error. Sending it again with the same idempotency key returns the same failure; after fixing the shell setting, a new idempotency key runs it again.

## Granting permissions to a child agent

```sh
tasty session issue --agent-id build-bot --permission surface.read --permission terminal.write
tasty session list
tasty session revoke --token <token>
```

When a child uses the issued token as `TASTY_SESSION_TOKEN`, Tasty checks its requests against the permissions on that token.
A session token tells Tasty which agent a request comes from and keeps agents from making mistakes; it is not a security boundary. A command run without a token is handled as the user's command.

Commands that a plugin adds (`tasty markdown recent`, `tasty codex spawn` and so on) need a permission too.
For `tasty <command> …`, add `--permission ipc.invoke:<command>` — `tasty markdown …` needs
`ipc.invoke:markdown`, and a `-` in the command name becomes `_`. Without it the call is refused and a permission approval request goes to a person.
A Claude started with `tasty claude spawn` already holds the permissions for `tasty claude …` and `tasty codex …`.

## Sending notifications

Tell a person when a long task has finished. The notification collects in the notification panel and shows as a Surface border and a sidebar badge. It is not sent to the OS notification center.

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

Notes can only be attached to an open Surface. A closed Surface ID is rejected with an error, and closing a Surface removes its notes as well.

## Passing messages between Surfaces (queue)

A queue that passes messages between Surfaces without touching terminal input.

```sh
tasty send queue --to 42 "Tests done, please check the results"
tasty list queue --surface 42            # pending count and preview
tasty read queue --surface 42            # pop all queued messages
tasty read queue --surface 42 --peek     # look without popping
tasty read queue --surface 42 --clear    # empty everything
```

<a id="running-child-terminals-like-agents"></a>

## Running tasks in several terminals

Open additional terminals in a workspace to run commands, send input, and track the work. You can use this with **any program**, including Claude and Codex. Their [dedicated commands](claude-codex.md) also provide session management.

```sh
tasty terminal spawn --workspace build --command "cargo watch -x test" --cwd ~/proj --role worker
tasty terminal children                        # children under me
tasty terminal tell "y" --surface 57           # send input to a child (line breaks kept, submitted automatically)
tasty terminal broadcast $'git pull\r' --role worker # to every child with the same role (submits only with a real trailing CR)
tasty terminal kill --child 1                   # kill a child by index
```

`spawn` replies when terminal preparation completes; it does not wait for the agent to finish its work. Agent state delivery and parent setup follow [Claude & Codex integration](claude-codex.md). Starting an ordinary program does not guarantee delivery of agent results.
There is no separate command to wait on. Tag children with `--role` to address them together with `broadcast`.

The child index reservation is saved before the new terminal is prepared. If that save fails, Tasty reports an error without creating the terminal or sending the command. Damaged or unreadable relationship data is not overwritten.

`adopt` saves index allocation and relationship registration together. `adopt`, `release` and `set-state` reply after checking the save result. An unconfirmed result is not reported as success, and the same reservation is not automatically replayed. If the target closes or changes during the save, Tasty reports that the live change could not be applied. Input from `spawn --command` is also sent only after the child relationship is saved.

If `terminal spawn` fails during setup, it sends no command and removes the terminal created by that call. Existing terminals remain open.

With `--command`, `spawn` types the command after the new terminal appears, sends Enter, and then replies. If the terminal is replaced by another shell or held by a remote attach in between, Enter is not sent and the reply is a recovery-required error (`-32068`); the created terminal stays. Keys typed into that terminal between the command and Enter can join the end of the command and run with it, as with `terminal tell`.

A child's parent is usually the terminal that ran the command. The dedicated Claude and Codex commands choose the parent for you, so you do not need to set it. If your script uses something outside Tasty as the parent (for example, an agent conversation running in another process), pass `--parent-external <key>` instead of `--surface`. The key is a string you choose (no whitespace, at most 256 bytes), and the same key groups children under the same parent. `spawn`, `children`, `kill`, `respawn`, `broadcast` and `release` accept this option. When several windows are open, also pass `--workspace`.

```sh
tasty terminal spawn --workspace build --parent-external review-bot --command "make lint"
tasty terminal children --parent-external review-bot --workspace build
tasty terminal release --parent-external review-bot --workspace build --child 0
```

## Headless PTY

Run a program on a real PTY (pseudo-terminal) with no tab and no screen. Use it when a command needs a TTY but you drive it from a script without taking up screen space. Feed input and read the screen by the id that `spawn` returns.

```sh
tasty pty spawn --cwd ~/proj -- python3         # start a command on a PTY and get its id
tasty pty write --id 2147483648 $'print(1+1)\n' # send to stdin (a newline submits)
tasty pty read --id 2147483648 --lines 20      # the last 20 lines on the screen right now
tasty pty state --id 2147483648                 # whether it has exited, right away (does not wait)
tasty pty list                                  # PTYs that are up
tasty pty kill --id 2147483648                   # stop it
```

Checking the run state and moving a running task into a tab are covered in [Bringing background tasks into a tab](background-tasks.md).

## Memory shared between agents

A key-value store where several agents in the same Tasty exchange values. Pick a scope (global · surface · workspace · window · account) to store under, and optionally let entries expire (TTL) or guard against overwrites (CAS).

```sh
tasty memory put --workspace 7 --key build.status --value running --ttl 600
tasty memory get --workspace 7 --key build.status
tasty memory list --workspace 7 --prefix build.
tasty memory delete --workspace 7 --key build.status
```

Switch scope with `--global` · `--surface 3` · `--window 42` · `--account me`. A value that parses as JSON is stored as JSON, otherwise as a string.

A key whose `--ttl` has passed is the same as a missing key. Any agent may store a new value under it, and its version starts again at 1. Expired values do not count toward the size limits, and Tasty removes them from disk every 10 minutes.

Storing in a `--surface` scope works only for an open Surface. A closed Surface ID is rejected with an error. `tasty memory import` follows the same rule: if the file has even one entry in the scope of a closed Surface, nothing is imported and the command fails with an error. Closing a Surface removes the values in its scope as well. Values left in the scope of a Surface closed earlier can be found with `tasty memory list --surface <ID>` and removed with `tasty memory delete`.

If Tasty cannot open its memory file (`~/.tasty/memory.db`) when it starts — the file is damaged or not accessible — Tasty does not stop; it keeps running on **temporary memory**. Values stored in that state are gone after Tasty restarts. This also applies to everything else kept in the same file: agent tasks and coordination tools such as semaphores, approvals, surface metadata, telemetry, and sessions. While it lasts, every such store result also carries `"durable": false`, and `db_pragmas.memory_db` in `tasty list pressure` shows `degraded: true` with the cause (`init_failure`). A warning notification with the cause and this effect is also left in the notification list.

## Pulling signals out of the output (observers)

Observers find and record paths, URLs, exit codes, and prompt boundaries in terminal output. Scripts can use these records without a person watching the screen.

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

Without `--agent`, `record` uses the caller ID identified by Tasty. An agent using a session token uses that session’s ID. A local call without a token is recorded under the shell’s `TASTY_AGENT_ID`, or as `_host` when it is unset or not up to 64 letters, digits, `_` and `-`. The value is only a name the caller gives itself and does not change any permission. To insert several values at once with their order preserved, use `tasty telemetry record-batch`.

## Other queries and settings

Things an agent reaches for occasionally. `tasty <command> --help` lists them all.

```sh
tasty list pressure                    # where the time went while answering requests (how to read it: response delays and connections)
tasty list theme                       # the theme snapshot in effect (colors, font sizes, UI scale)
tasty list recent --kind markdown      # files recently opened as that kind
tasty set cwd --surface 42 --path /tmp # change the working directory a remote surface reports
tasty set url --surface 42 --url URL   # change the address of a webview surface (--label NAME: the name shown while the page is hidden)
tasty surface html-script --surface 42 # script and remote content detection, allowance and notice state of an HTML surface (read only)
tasty surface html-script-allow --surface 42 --url <url> --fingerprint <hex>  # allow the scripts of the queried document and reload it; refused if the document changed; focus stays
tasty surface html-remote-allow --surface 42 --url <url> --fingerprint <hex>  # allow remote content (network stylesheets, images, fonts, frames and scripts) for the queried document only and reload it; separate from the script allowance; focus stays
tasty surface html-nav --surface 42    # the address of the document an HTML surface shows and whether it can go back or forward (read only)
tasty surface html-back --surface 42   # go back in an HTML surface (html-forward: forward, html-reload: reload); focus stays; check the result with html-nav
tasty hook-handler dispatch --id user/my-handler  # run a hook handler directly and wait for its final result
tasty hook-handler result --run 7       # look up a result again by the run number dispatch printed
tasty file-handler dispatch PATH       # open a file the same way a double-click in the explorer does (waits for the result)
tasty file-handler result --run 7       # look up a result again by the run number dispatch printed
tasty file-handler reload              # read the file handler settings file again
tasty file-handler detectors           # see how the file format detectors are merged right now
```

`set cwd` and `set url` only apply to a remote surface and a webview surface respectively. Using them on a regular terminal surface returns an unsupported-target error.

`hook-handler dispatch` prints two lines. The first is the reply accepting the request and contains a `run` number. The second is the final result after waiting. If the result's `state` is `succeeded`, the command exits with status 0; if it is `partial`, `failed` or `unknown`, or the run did not finish while waiting (`running`), it exits with status 1. If Tasty does not answer the result query, the command still stops at the wait limit, and the second line then carries `"timed_out": true`. The failed steps and their reasons are in `result` and `steps`. A request refused before it was accepted (an unknown handler, a disabled handler, a full queue and so on) prints an error without the first line and exits with status 1. With `--no-wait` it prints only the first line and exits with status 0; look the result up later with `hook-handler result --run <number>`. While waiting for the result the command sends several requests, so it does not take the root option `--response-timeout-ms`; with it, the command sends nothing and exits with a usage error (status 2). With `--no-wait` there is only one request, and the option applies to it. The session token (`TASTY_SESSION_TOKEN`) and agent ID (`TASTY_AGENT_ID`) set in the shell are also carried by the requests that ask for the result.

`file-handler dispatch` accepts file paths only. Passing a web address such as `https://…` returns an error. A file opened with this command is added as a new tab in the background; the tab the user was looking at stays selected. If no handler can open the file's format, the **Open file with…** chooser does not appear on the user's screen and the file is not opened. Like `hook-handler dispatch`, the command prints two lines. The first is the reply accepting the request (with a `run` number), and the second is the final result. If the file was opened, `state` is `succeeded`, the new tab is given in `surface_id`, and the command exits with status 0. If no handler can open it or the handler fails, `state` is `failed`, `result` says why, and the command exits with status 1. When the file is opened next to a tab of a remote workspace, the command waits until the remote Tasty has actually opened it. If the remote cannot open it, `state` is `failed` with the reason the remote gave. If the connection drops before the remote answers, whether the file was opened is not known, so `state` is `unknown` and the command exits with status 1. It also exits with status 1 when no result arrives within 30 seconds. With `--no-wait` it prints only the first line; look the result up later with `file-handler result --run <number>`. While waiting for the result it does not take the root option `--response-timeout-ms` (status 2). A headless build, which runs Tasty on a server without a GUI, cannot open files, so instead of reporting the request as accepted it returns an error saying this build does not support it.

The `file-handler reload` response has a `rejected` list. It holds the `id` and the reason (`reason`) of each entry from the settings file that is not applied right now, and it is empty when everything applied. There are three reasons.

- `missing_owner_prefix` — the `id` has no owner part such as `user/` in front of it (write it as `user/name`). The entry is dropped.
- `missing_detector_or_action` — an entry you made yourself (`user/…`) is missing either which files it handles (`detector`) or what to do (`action`). The entry is dropped.
- `target_not_contributed` — the entry changes a built-in or plugin handler, but that handler is not there right now. Either the plugin is off or the `id` is wrong. The entry is kept, and it applies as soon as the plugin is on. If it is still listed after you turn the plugin on, check the `id`.

`file-handler detectors` shows, for each file format detector, the values in effect now (display name, icon, whether it is on, rules) together with what each source contributed (`contributions`). A source is `host` (built in), `plugin:<id>` or `user` (the settings file). Use it to check whether a value from your settings file took effect — `contributions` are listed in the order they were installed, so look at the values in effect above them to see which one applied.

The workspace count and active index in `list info` describe the queried window. The returned workspace IDs identify its scope. Use `list workspaces` for the global inventory and `list windows` for each window’s state.

## Keep exploring

- [Driving terminals with the tasty CLI](cli.md) — The first-time order and the basic pattern.
- [CLI response delays and connection diagnostics](../help/cli-diagnostics.md) — Reading `tasty list pressure` and what to do for each error code.
- [Bringing background tasks into a tab](background-tasks.md) · [Claude · Codex](claude-codex.md) · [Task workflows](tasks.md) · [Hooks, notifications, and webhooks](hooks-notifications.md)
