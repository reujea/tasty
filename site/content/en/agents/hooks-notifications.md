<!-- source-hash: ee3f4c9096c6 -->
<!-- source-hash: 566aeb51a6d8 -->
<!-- source-hash: 4d29d4555043 -->
<!-- source-hash: 889e7471180f -->
# Hooks, notifications and webhooks

Get a notification when a build finishes, or run a command when a message appears in the logs. **Hooks** run commands in response to events, and **notifications** let you know when to check back. Use **webhooks** to send requests to Tasty from an external service.

## Surface hooks

Shell-exit hooks also run in headless instances. Closing a surface removes its remaining hook registrations, including hooks without `--once`.

Run a shell command when an event occurs on a specific Surface (terminal).

```sh
tasty set hook --surface 42 --event process-exit --command "tasty notify 'Shell exited'"
tasty set hook --surface 42 --event 'output-match:error\[E\d+\]' --command "tasty notify \"$TASTY_HOOK_MATCHED_TEXT\""
tasty set hook --surface 42 --event idle-timeout:30 --command "tasty notify 'No output for 30 seconds'" --once
tasty list hooks [--surface 42]
tasty unset hook --hook <HOOK_ID>
```

| Event | When |
|---|---|
| `process-exit` | When the shell process exits |
| `bell` | When a bell (`\a`) is received |
| `notification` | When a terminal notification sequence (OSC 9/99/777) is received |
| `output-match:<regex>` | When a **completed line** of output matches the regex |
| `idle-timeout:<secs>` | When there has been no output for N seconds (1-second granularity, starts waiting again when new output arrives) |
| `command-completed` | When one command finishes inside the shell (requires bash/zsh shell integration) |
| `command-completed:<code>` | Only commands that finished with that exit code (e.g. `command-completed:1` = failed commands) |
| `claude-idle` / `needs-input` / `codex-idle` | Events emitted by the Claude Code / Codex plugins ([Claude and Codex](claude-codex.md)) |

- Add `--once` and the hook is removed automatically after firing once. By default it stays.
- Hook commands run in the background and do not block terminal input.
- `command-completed` never fires without shell integration. A guidance banner appears once on Surfaces that lack shell integration.

Event information reaches the command through environment variables.

| Variable | Value |
|---|---|
| `TASTY_HOOK_EVENT` | The event string you registered (`bell`, `output-match:…`, etc.) |
| `TASTY_HOOK_SURFACE_ID` | ID of the Surface where the event occurred |
| `TASTY_HOOK_MATCHED_TEXT` | The whole line that actually matched in `output-match` |
| `TASTY_HOOK_EXIT_CODE` | Exit code for `command-completed` |
| `TASTY_HOOK_IDLE_ELAPSED_SECS` | Seconds since the last output for `idle-timeout` |

## Global hooks

Run on a time condition, independent of any Surface.

```sh
tasty set global-hook --condition interval:60 --command "df -h > ~/disk.log" --label "Disk log"
tasty set global-hook --condition once:300 --command "tasty notify '5 minutes passed'"
tasty list global-hooks
tasty unset global-hook --hook <HOOK_ID>
```

| Condition | Meaning |
|---|---|
| `interval:<secs>` | Repeat every N seconds |
| `once:<secs>` | Run once after N seconds, then delete |

## Hook handlers (reusable actions)

Instead of `--command`, you can attach a pre-registered **hook handler** by name. The same handler is shared by several hooks and webhooks.

```sh
tasty hook-handler list                                     # registered handlers (host / plugin / user)
tasty set hook --surface 42 --event bell --handler user/my-handler
tasty hook-handler get --id user/my-handler                 # one handler in full, including what it does
tasty hook-handler dispatch --id user/my-handler            # fire by hand to test
tasty hook-handler reload                                   # re-read ~/.tasty/hook-handlers.toml
```

You can also **change or create** a handler from the command line. Anything you leave out
stays as it was, so a handler can keep its name while what it does changes (every hook
pointing at it follows along).

```sh
# change only what it does — read it with get first, then hand the same shape back
tasty hook-handler upsert --id user/my-handler \
  --calls '[{"method":"notification.create","params":{"message":"Build finished"}}]'

# switch it off for a while, or back on
tasty hook-handler upsert --id user/my-handler --disabled true

# delete one you made
tasty hook-handler remove --id user/my-handler
```

The change is saved to `~/.tasty/hook-handlers.toml` right away. One thing does not follow: **a webhook address you already created keeps doing what it did** — it holds on to the actions it was given at the time, so register it again to pick up the new ones.

User handlers are added and edited in the **Settings** › **Handlers** › **Hook Handlers** tab. Saving writes them to `~/.tasty/hook-handlers.toml`, and you can also write the file directly (apply with `tasty hook-handler reload`).

Every row shows who planted it — `host` for Tasty itself, the plugin's own name for a plugin, and `you` for the ones you made. Only the rows you can delete carry a trash button; the rest carry a padlock, because Tasty and its plugins plant their handlers again on every start. A handler that chains several internal actions shows that chain on one line, and the tab cannot change it — use `tasty hook-handler upsert` above, or edit the file and run `tasty hook-handler reload`.

```toml
[[handler]]
id = "user/notify-fail"
source = "hook"          # hook | webhook | any — which triggers may use this handler
[handler.action]
kind = "shell_command"
command = "tasty"
args = ["notify", "Command failed", "--title", "hook"]
```

With `kind = "ipc_sequence"` it runs a list of Tasty-internal actions (`calls = [{ method = "...", params = {} }]`) instead of a shell.

## Notifications

### Sending

```sh
tasty notify "Build finished"                    # the title defaults to "Notification"
tasty notify "3 tests failed" --title "cargo test"
tasty list notifications
```

`list notifications` returns the 50 most recently created notifications across all windows. IDs are unique across windows within one run. When nearby notifications merge into an existing entry, that entry keeps its position. The on-screen panel and read state remain local to each window.


Notification sequences sent by terminal programs (OSC 9 / 99 / 777) and bells are collected as the same kind of notification.

### Where they appear

- **Notification panel** — open it with `Ctrl+Shift+I` (macOS `Cmd+Shift+I`). The newest-first list shows Workspace, title, body, and elapsed time, and **Jump** takes you to that Workspace. Opening it marks everything read; there is also a **Mark all read** button.
- **Surface border** — a blue border on the Surface where the notification occurred. It disappears when you focus that Surface.
- **Sidebar badge** — a count badge on the row of any Workspace that has a Surface needing attention.
- **OS notification** — a system notification when the Tasty window is inactive (limited to once per second).
- **Sound** — when enabled in settings, one system beep per notification.

### Settings

The **Settings** › **Notifications** tab, or `~/.tasty/config.toml`:

```toml
[notification]
enabled = true        # enable notifications
sound = false         # sound
coalesce_ms = 500     # notification coalescing interval (ms) — merges consecutive notifications from the same source into one

[general]
bell_notification = true   # show bell notifications (turning it off suppresses only the bell toast; bell hooks still fire)
```

### Waiting for a human decision (approval)

Send an approval request when a task needs the user's decision before it can proceed. The agent can then wait for the response.

```sh
ID=$(tasty approval request --title "Run the prod DB migration?" --severity danger \
      --choices "approve:Run,deny:Abort:1" --timeout-ms 600000)
tasty approval await --id "$ID"            # wait until a response arrives, print the result as JSON
```

- A popup appears and a notification goes out as well. The user answers by clicking a popup button, pressing number keys `1`~`9` in the popup (in choice order), or with `tasty approval respond --id <ID> --choice approve`.
- `--severity info` sends only a notification with no popup; `warn`/`danger` show a popup plus a notification. `danger` must be answered by the user directly.
- The popup does not close on Esc (to prevent bypassing). Query with `tasty approval list` / `get` / `history`.

## Webhooks (outside → Tasty)

Let CI or other services send an HTTP request to trigger an action inside Tasty. Tasty opens one designated port and issues an unguessable URL for each webhook.

Webhooks use an unguessable URL and an optional fixed token (`--auth-*`). **HMAC signature verification is not supported**, so signature headers from external services are not checked. **Without authentication, anyone who can reach the URL can request the action.** Include the authentication options you need when registering a webhook.

### Port settings

```sh
tasty webhook config                # current port and whether it is bound
tasty webhook config --port 28429   # change the port — applied after restart
```

- The settings file is `~/.tasty/webhooks.toml`. On first run, `28429` is written as the default.
- If the port is empty or the bind fails, Tasty keeps the configured port and displays a warning. Check the port setting and restart Tasty.
- **The listener binds every network interface.** Without opening any forwarding it is already reachable from the same network (an office LAN, public Wi-Fi). Router forwarding and the firewall are what you open to let *the internet* in; they are not what keeps it closed until then. Leave HTTPS to a reverse proxy in front.

### Registering

```sh
# attach to a registered handler
tasty webhook register --method POST --handler host/notify --persistent

# define the action inline — pull values from the body with ${body.x}
tasty webhook register --method POST \
  --sequence '[{"method":"notification.create","params":{"title":"CI","body":"${body.status}"}}]' \
  --ttl-secs 3600 \
  --auth-location header --auth-key X-Token --auth-token s3cret
```

Registering prints a URL of the form `http://127.0.0.1:28429/<16-character id>`. When calling from outside, replace the host part with the real address — that host is printed so you can `curl` it right there, and is not the address the listener binds. Use the returned path unchanged; do not add a `/webhook/` prefix.

| Option | Meaning |
|---|---|
| `--method <M>` | Allowed HTTP method (repeatable, default POST) |
| `--handler <id>` or `--sequence <json>` | Exactly one of the two is required |
| `--persistent` | Kept across restarts (by default it disappears on restart). If you also set `--auth-token`, that token is stored in plain text in `~/.tasty/webhooks.toml` |
| `--ttl-secs <secs>` / `--count <N>` | Time limit / **accepted** call count limit (one of the two). A rejected token (`401`) and a method that is not allowed (`405`) do not spend the count |
| `--auth-location query\|bearer\|body\|header` + `--auth-token` (+ `--auth-key`) | Optional authentication. No auth if not set |

Omit both `--ttl-secs` and `--count` for unlimited calls. `--persistent` can be combined with either limit; restarting restores registrations whose time or call allowance has not expired.

Check the available handler ids with `tasty hook-handler list`. `--sequence` is a JSON list of Tasty-internal actions (send a notification, send text, etc.) written in order. If it is not valid JSON, nothing is registered and the command ends with an error that says why (exit code 1).

### Responses and management

The calling service receives a status code and a fixed response message. The response does not include the action's result.

**`200` means "accepted", not "done".** The answer is chosen before the action starts, so the caller gets `200` even when the action fails outright. In a multi-step `--sequence` a failing step does not stop the steps after it, so it can end half applied, and what failed appears only in Tasty's own log. If your CI decides retries from the status code, do not count `200` as success.

| Code | Meaning |
|---|---|
| 200 `received` | Accepted (the action runs in the background) |
| 401 `unauthorized` | Authentication failed (repeat it and you are blocked with 429) |
| 404 | Unknown URL |
| 405 | Method not allowed |
| 410 | Time or count limit expired |
| 413 | The body exceeded the per-request size cap (1 MiB by default) |
| 429 | The same source failed (`401`, `404`, `405` or `413`) 20 or more times in 10 seconds and is blocked for 60 seconds |

The body size cap is checked before authentication. The same cap applies to chunked bodies containing invalid UTF-8 bytes. An oversized request receives `413 payload too large` even without a token, and no action runs. This cap limits the body accepted for processing. A 413 response carries `Connection: close`, and the server closes the connection without receiving the rest of the body. A blocked request (`429`) is answered the same way. The connection cannot carry another request, so open a new one. If the client was still sending the body, the connection may end with a reset. The cap does not bound total memory use or simultaneous connections.

The block lives in memory only — restart Tasty and the remaining block time is gone, so that source starts over.

```sh
tasty webhook list
tasty webhook info --id <ID>
tasty webhook unregister --id <ID>
tasty webhook sweep                 # clean up expired webhooks in one go
```

Webhooks cannot run shell commands directly (Tasty-internal actions only). If you need a shell, chain them: webhook → notification → Surface hook.

## Combined example: notify when a long build finishes

```sh
tasty set hook --surface 42 --event command-completed --once \
  --command 'if [ "$TASTY_HOOK_EXIT_CODE" = 0 ]; then tasty notify "Build succeeded" --title build; else tasty notify "Build failed ($TASTY_HOOK_EXIT_CODE)" --title build; fi'
tasty send text "cargo build --release\r" --surface 42
```

`command-completed` fires for every exit code; append a single integer, as in `command-completed:0`, and it fires only for that code. The actual exit code reaches the hook command through the `TASTY_HOOK_EXIT_CODE` environment variable.
