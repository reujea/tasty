<!-- source-hash: 81c53eca203a -->
# Hooks, notifications, and webhooks

Hooks run actions in response to Tasty events, notifications show items to check in the app, and webhooks receive HTTP requests from external services. Choose the path that fits your task.

- Configure a hook to react to an event.
- Check notifications in the notification panel and surface indicators.
- Receive external requests through webhooks. An acknowledgement does not mean execution has completed.
- Use an approval request when you need a person’s decision and response.

## Surface hooks

Shell-exit hooks also run in headless instances. Closing a surface removes its remaining hook registrations, including hooks without `--once`.

Run a shell command when an event occurs on a specific Surface (terminal).

```sh
tasty set hook --surface 42 --event process-exit --command "tasty notify 'Shell exited'"
tasty set hook --surface 42 --event 'output-match:error\[E\d+\]' --command 'tasty notify "$TASTY_HOOK_MATCHED_TEXT"'
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
| `claude-stop-failure` | When a Claude Code turn ends on an API error (server overload, rate limit, authentication failure, and so on). It usually comes with `claude-idle`; when background work is still running, only this one comes |
| `claude-error` | When a known error message appears in Claude Code's screen output |
| `claude-error-stalled` | When the turn hasn't ended but Claude Code's output hasn't changed for a while, so it looks stuck (comes even without an earlier error) |

- Add `--once` and the hook is removed automatically after firing once. By default it stays.
- Hook commands run in the background and do not block terminal input.
- `command-completed` never fires without shell integration. A guidance banner appears once on local Surfaces that lack shell integration and closes on its own after 10 seconds.

Event information reaches the command through environment variables.

| Variable | Value |
|---|---|
| `TASTY_HOOK_EVENT` | The event string you registered (`bell`, `output-match:…`, etc.) |
| `TASTY_HOOK_SURFACE_ID` | ID of the Surface where the event occurred |
| `TASTY_HOOK_MATCHED_TEXT` | The whole line that actually matched in `output-match` |
| `TASTY_HOOK_EXIT_CODE` | Exit code for `command-completed` |
| `TASTY_HOOK_IDLE_ELAPSED_SECS` | Seconds since the last output for `idle-timeout` |
| `TASTY_HOOK_CUSTOM_EVENT` | The name of a plugin event (`claude-idle` and so on) |
| `TASTY_HOOK_SOURCE` | `hook` when a hook ran it, `dispatch` when you ran it directly with `tasty hook-handler dispatch` |

Apart from `TASTY_HOOK_EVENT`, `TASTY_HOOK_SOURCE` and `TASTY_HOOK_SURFACE_ID`, the variables are built from the key names of the event information. Letters and digits in a key become uppercase, every other character becomes `_`, and the result follows `TASTY_HOOK_` (`matched_text` → `TASTY_HOOK_MATCHED_TEXT`). When you run a hook handler that runs a shell command with `tasty hook-handler dispatch --body '{"build-id":7}'`, the top-level keys of the `--body` object come in by the same rule (`TASTY_HOOK_BUILD_ID=7`). Strings come in as they are, other values as JSON text, and each value is cut at 4096 bytes. When two keys produce the same name, the first value is used.

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
tasty hook-handler dispatch --id user/my-handler            # run directly to test
tasty hook-handler reload                                   # re-read ~/.tasty/hook-handlers.toml
```

`dispatch` first prints a line saying the run was accepted (with its `run` number), then waits until it finishes and prints the result line. The result's `state` is `succeeded` when every step worked, `partial` when only some did, `failed` when none did, and `unknown` when no answer came back so the outcome is not known; `result` and `steps` say which step failed and why. A shell command handler counts as succeeded once its process starts, and its exit code is shown for reference only once it ends. Unless the result is `succeeded`, the command exits with code 1. To skip waiting, add `--no-wait` and check later with `tasty hook-handler result --run <number>`.

You can also **change or create** a handler from the command line. Anything you leave out
stays as it was, so a handler can keep its name while what it does changes (every hook
pointing at it follows along).

```sh
# change only what it does — read it with get first, then hand the same shape back
tasty hook-handler upsert --id user/my-handler \
  --calls '[{"method":"notification.create","params":{"body":"Build finished"}}]'

# switch it off for a while, or back on
tasty hook-handler upsert --id user/my-handler --disabled true

# delete one you made
tasty hook-handler remove --id user/my-handler
```

The change applies to the current run first, and the command finishes after the save result for `~/.tasty/hook-handlers.toml` arrives. A failed save reports an error while keeping the change in the current run. One thing does not follow: **a webhook address you already created keeps doing what it did** — it holds on to the actions it was given at the time, so register it again to pick up the new ones.

User handlers are added and edited in the **Settings** › **Handler** › **Hook Handlers** tab. Save applies the changes to the current run first and writes `~/.tasty/hook-handlers.toml` in the background. A failed save appears in a toast and a notification; the changes may be lost on the next start. You can also edit the file directly and apply it with `tasty hook-handler reload`. Reload is rejected while a save is pending, so try again after it finishes. A read or format error keeps the previous settings. A startup load failure logs the file name and the type of problem without including the file contents.

Every row shows who registered it — `host` for Tasty itself, the plugin's own name for a plugin, and `you` for the ones you made. Only the rows you can delete carry a trash button; the rest carry a padlock, because Tasty and its plugins register their handlers again on every start. A handler that chains several internal actions shows that chain on one line, and **Edit** at the end of the line turns it into text you can change in place. Write one action per line: the method name, then optional JSON params (lines starting with `#` are skipped and are not kept). If a line is wrong, the first problem and its line number appear under the field and **Apply** stays disabled until you fix it. Applied changes reach the file when you press **Save** at the bottom of the window. Inside the field, Ctrl+Enter (Cmd+Enter on macOS) applies and Esc cancels — change both under **Apply in a code field** and **Cancel in a code field** in the [keybinding](../customize/keybindings.md) settings. Editing a handler that Tasty or a plugin registered saves it as your own setting under the same name. Such a handler **no longer follows** later changes that Tasty or the plugin makes to its default. Such a row carries an **edited** mark next to its origin, and **Revert** on its second line takes it back to the default. Revert applies when you press **Save**; until then the row shows **reverts on save** and the same button reads **Undo**. While it waits, the on/off switch is locked at the default and **Edit** is disabled — press **Undo** first to change them. From a terminal, `tasty hook-handler remove --id <id>` does the same (only your setting is removed and the original handler applies again). If the chain holds an action whose name cannot be written as text, the row shows **Edit with CLI** and a copy button instead — the copied `tasty hook-handler get --id <id>` shows the handler, and `tasty hook-handler upsert` above changes it.

```toml
[[handler]]
id = "user/notify-fail"
source = "hook"          # hook | webhook | any — which triggers may use this handler
[handler.action]
kind = "shell_command"
command = "tasty"
args = ["notify", "Command failed", "--title", "hook"]
```

With `kind = "ipc_sequence"` it runs a list of Tasty-internal actions (`calls = [{ method = "...", params = {} }]`) instead of a shell. TOML has no null, so when params need a null value, write them as a JSON string instead of `params`, as in `params_json = '{"a":null}'`. Saving with `tasty hook-handler upsert` writes this form for you.

When a hook runs by itself and fails, the failure is left in the notification list. If it did not run at all, because its handler is gone or its command could not start, that is an **error**. If only some steps of an action list failed, or a step gave no answer in time so its result is unknown, that is a **warning**. A surface hook's notification is attached to the surface the hook is on. A shell command ending with a non-zero exit code is not counted as a failure. When the same failure repeats quickly, it is left once every 10 seconds.

## Notifications

### Sending

```sh
tasty notify "Build finished"                    # the title defaults to "Notification"
tasty notify "3 tests failed" --title "cargo test" --level error --surface 42
tasty list notifications
```

`list notifications` returns the 50 most recently created notifications across all windows. IDs are unique across windows within one run. When nearby notifications merge into an existing entry, that entry keeps its position. The on-screen panel shows its own window’s notifications. Counts and explicit read commands can cover all windows.


Notification sequences sent by terminal programs (OSC 9 / 99 / 777) and bells are collected as the same kind of notification.

Choose `--level error` for a critical failure, `--level warning` for a non-critical problem, or `--level info` for information. Omitted levels default to `info`. Different levels never merge; new content merged into a read entry makes it unread again.

```sh
tasty notification counts
tasty notification counts --workspace 12
tasty notification read 7
tasty notification read --all --workspace 12
```

Counts cover all retained unread notifications, beyond the latest 50 returned by the list. Use the notification ID printed by `list notifications` to mark one read. `read --all` without a workspace filter covers every window. Reading the last unread notification from a Surface clears its attention unless the Surface is hard-occupied. Notification counts and Surface counts remain separate.

Use `notify --workspace 12` for an alert without a Surface. This does not add a Surface attention mark. With an explicit workspace, the Surface ID from your environment is not added automatically.

### Where they appear

- **Sidebar notification summary** — just above Tools, badges show unread errors (red), warnings (yellow), and information (blue). Zero counts are hidden and counts above 99 appear as `99+`. The expanded sidebar also shows the latest unread title. Click the summary to open the notification panel. In the light theme, warning badges use a pale yellow background with dark numbers. Warning icons in the list use darker ink that remains the same after you mark them as read.
- **Notification panel** — open it from the summary or with `Ctrl+Shift+I`. The newest-first list shows level, title, body, source, and elapsed time. Opening or closing the panel keeps unread notifications unchanged. Click a row or **More** to expand the full text and mark that notification as read. Use the check button for one notification or **Mark all read** for that window. **Jump** takes you to the source Surface or Workspace. A closed source keeps its notification history, with Jump disabled. The title bar lets you switch to fullscreen. New notifications do not open the panel automatically or scroll the list back to the top. Your reading position also stays in place when new content is merged into an existing notification and makes its body longer.
- **Surface border** — a blue border on the Surface where the notification occurred. It disappears when you focus that Surface while the Tasty window is active. While you are in another app it stays, and it disappears when you return to the window.
- **Sidebar badge** — a count badge on the row of any Workspace that has a Surface needing attention.
- **Sound** — when enabled in settings, one system beep per notification.

Notifications are not sent to the OS notification center (system notifications). To notice them while you use another app, turn on the sound.

### Settings

The **Settings** › **General** › **Notifications** tab, or `~/.tasty/config.toml`:

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
      --choices "approve:Run,deny:Abort:1" --timeout-ms 600000 | jq -r .id)
tasty approval await --id "$ID"            # wait until a response arrives, print the result as JSON
```

- A popup appears and a notification goes out as well. The user answers by clicking a popup button, pressing number keys `1`~`9` in the popup (in choice order), or with `tasty approval respond --id <ID> --choice approve`.
- The popup does not take keyboard focus. Whatever you were typing into a terminal or a document keeps going there. To pick with a number key, first click the popup's title or an empty part of it. Buttons can be clicked right away. While you are typing into a Markdown or HTML document, that document can cover the part of the popup it overlaps. The notification that goes out with the request shows it as well.
- Digits typed while the comment field is focused go into the comment and do not pick a choice. To pick with a number key after writing a comment, leave the comment field with Tab or click an empty part of the popup.
- `--severity info` sends only a notification with no popup; `warn`/`danger` show a popup plus a notification. `danger` must be answered by the user directly.
- The popup does not close on Esc (to prevent bypassing). Query with `tasty approval list` / `get` / `history`.
- `approval await` is for the user only. An agent connected with a session token (`TASTY_SESSION_TOKEN`) cannot call it; check the state with `tasty approval get --id "$ID"` instead (this needs the `approval` permission).
- `--metadata` cannot use `capability_elevation` as its `kind`. That value is reserved for agent permission requests, so the request is rejected. Ask for an agent permission with `tasty plugin request-permission` instead.
- A request is bound to a workspace: the one given by `--workspace`, otherwise the workspace of the terminal given by `--surface`, otherwise the workspace you are looking at (the older `--workspace-id` and `--surface-id` are still accepted). An agent is safest passing its own terminal with `--surface "$TASTY_SURFACE_ID"` — the request then lands in the same place even when the user is looking at another workspace.

## Webhooks (outside → Tasty)

Let CI or other services send an HTTP request to trigger an action inside Tasty. Tasty opens one port for webhooks and issues a URL that is difficult to guess for each webhook.

Webhooks use a URL that is difficult to guess and an optional fixed token (`--auth-*`). **HMAC signature verification is not supported**, so signature headers from external services are not checked. **Without authentication, anyone who can reach the URL can request the action.** Include the authentication options you need when registering a webhook.

### Port and calls from other computers

```sh
tasty webhook port                  # address and port of the running listener, and how the port was chosen
tasty webhook port 28500            # use this port from the next start
tasty webhook port --unset          # remove the saved port so the next start picks a free one
tasty webhook allow-external on     # also accept calls from other computers from the next start (off to undo)
tasty webhook allow-external        # show the saved value and the address the running listener actually opened
tasty --webhook-port 28500          # start with this port for this run only
```

- **If you do not choose a port**, Tasty tries `28429` and counts up, using the first free port (up to 64 ports, through `28492`). Webhooks work even when another Tasty already uses `28429`. If all are taken, Tasty leaves a warning in the notification list and runs without webhooks.
- **If you chose a port** (`--webhook-port` or `tasty webhook port <N>`; with both, `--webhook-port` wins), Tasty uses only that port. If another program holds it, Tasty does not move to another port and **starts without webhooks**. Terminals work as usual, and one warning in the notification list gives the port, the cause and how to fix it. Webhook URLs (including ones saved with `--persistent`) do not answer meanwhile, but the registrations stay, so after you fix the port and restart the same registrations answer again. Their URLs use the port opened in that run, so check the actual addresses with `tasty webhook list`. `tasty webhook port` shows `bound: false` and, under `unavailable`, the port it could not open and why. Start with another port, or, for a saved port, run `tasty webhook port --unset` (remove) or `tasty webhook port <free port>` (replace) and start again. These two commands edit `~/.tasty/webhooks.toml` directly when Tasty is not running and say so. When Tasty is running, they apply from the next start.
- `tasty webhook config --port <N>` is the earlier name and is deprecated. It still works exactly like `tasty webhook port <N>`.
- If the port moved off `28429` and you have webhooks saved with `--persistent`, Tasty leaves a warning in the notification list. The saved webhooks' URLs change port too, so check the new URLs with `tasty webhook list`. Choose a port if the URLs must not change.
- If Tasty is already running and you launch it again with a different `--webhook-port`, the launch is not handed to the running Tasty; it explains why and ends. With the same port it opens a new window in the running Tasty as usual.
- A `port = 28429` that an earlier version wrote automatically is removed on the first start, so the port is chosen automatically. Any other port you set stays.
- **By default the webhook server accepts calls from this computer (`127.0.0.1`) only.** If other computers or CI must call it directly, turn on **Settings → General → Accept webhook calls from other computers** or run `tasty webhook allow-external on`, then restart Tasty. Writing `allow_external = true` under `[webhook]` in `~/.tasty/config.toml` does the same. When on, it accepts connections on all IPv4 network interfaces (`0.0.0.0`), so if the host firewall allows it, devices on the same network, such as an office LAN or public Wi-Fi, can reach it without router forwarding. Check firewall and authentication settings. Use a reverse proxy for HTTPS.

### Registering

```sh
# attach to a registered handler
tasty webhook register --method POST --handler host/webhook-notify --persistent

# define the action inline — pull values from the body with ${body.x}
tasty webhook register --method POST \
  --sequence '[{"method":"notification.create","params":{"title":"CI","body":"${body.status}"}}]' \
  --ttl-secs 3600 \
  --auth-location header --auth-key X-Token --auth-token s3cret
```

Registering prints a URL of the form `http://127.0.0.1:<port>/<16-character id>`. The port is the one this run actually opened. If you turned on calls from other computers, replace the host part with the real address when calling from outside — that host is printed so you can `curl` it right there. Use the returned path unchanged; do not add a `/webhook/` prefix.

| Option | Meaning |
|---|---|
| `--method <M>` | Allowed HTTP method (repeatable, default POST) |
| `--handler <id>` or `--sequence <json>` | Exactly one of the two is required |
| `--persistent` | Kept across restarts (by default it disappears on restart). If you also set `--auth-token`, that token is stored in plain text in `~/.tasty/webhooks.toml` |
| `--ttl-secs <secs>` / `--count <N>` | Time limit / **accepted** call count limit (one of the two). A rejected token (`401`) and a method that is not allowed (`405`) do not spend the count |
| `--auth-location query\|bearer\|body\|header` + `--auth-token` (+ `--auth-key`) | Optional authentication. No auth if not set |

When you query or change the port in a running instance, the result arrives after file processing completes. If saving fails, the original stays intact and the command returns an error. The running listener address stays the same.

Persistent webhook registration and removal return after the file save finishes. If saving fails, the previous registration stays. Other Tasty work can continue while the save is pending.

Port settings and persistent registrations share one file and preserve each other's contents. If the file cannot be read or parsed, saving is refused and the original stays intact. Comments and whitespace from manual edits may change on the next save.

Omit both `--ttl-secs` and `--count` for unlimited calls. `--persistent` can be combined with either limit; restarting restores registrations whose time or call allowance has not expired.

Check the available handler ids with `tasty hook-handler list`. `--sequence` is a JSON list of Tasty-internal actions (send a notification, send text, etc.) written in order. If it is not valid JSON, nothing is registered and the command ends with an error that says why (exit code 1).

### Responses and management

The calling service receives a status code and a fixed response message. The response does not include the action's result.

**`200` means "accepted", not "done".** The answer is chosen before the action starts, so the caller gets `200` even when the action fails outright. In a multi-step `--sequence` a failing step does not stop the steps after it, so it can end half applied. What failed is left in Tasty's notification list (an error when no step succeeded, a warning when only some failed). If your CI decides retries from the status code, do not count `200` as success.

| Code | Meaning |
|---|---|
| 200 `received` | Accepted (the action runs in the background) |
| 401 `unauthorized` | Authentication failed (repeat it and you are blocked with 429) |
| 404 | Unknown URL |
| 405 | Method not allowed |
| 410 | Time or count limit expired |
| 413 | The body exceeded the per-request size cap (1 MiB by default) |
| 429 | The same source failed (`401`, `404`, `405` or `413`) 20 or more times in 10 seconds and is blocked for 60 seconds |
| 503 `unavailable` | The request could not be accepted or its save result could not be confirmed. No action runs |

The body size cap is checked before authentication. The same cap applies to chunked bodies containing invalid UTF-8 bytes. An oversized request receives `413 payload too large` even without a token, and no action runs. This cap limits the body accepted for processing. A 413 response carries `Connection: close`, and the server closes the connection without receiving the rest of the body. A blocked request (`429`) is answered the same way. The connection cannot carry another request, so open a new one. If the client was still sending the body, the connection may end with a reset. The cap does not bound total memory use or simultaneous connections.

The block lives in memory only — restart Tasty and the remaining block time is gone, so that source starts over.

When a failed save of a count or expiration change is confirmed, Tasty leaves a warning and continues acceptance and execution as before. The unsaved change may be missing after a restart. If admission is refused or the save outcome is unknown, Tasty returns `503` and runs no action. After an unknown outcome, further changes are refused until Tasty restarts and restores the saved registrations.

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
