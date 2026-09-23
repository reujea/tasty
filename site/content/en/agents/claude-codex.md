<!-- source-hash: 347d1d2567bb -->
# Working with Claude and Codex

Connect Claude Code and Codex CLI to share work across several agents. One agent can launch others and receive their results, so implementation, testing, and review can run alongside each other.

Install Claude Code and Codex CLI separately. Tasty manages launching, placement, and the connection to the agent that delegated the work. Set up both the hooks and the parent’s receiving channel. Completion lands in the same completion log whatever the parent is, and a Claude Code parent uses Monitor to subscribe to that log. Follow [Receiving completion notifications](#4-receiving-completion-notifications).

## 1. Install the hooks (once)

For Tasty to know an agent's state (working / idle / needs input / exited), each CLI's hook configuration must contain a Tasty entry.

```sh
tasty claude install    # add the Tasty entry to hooks in ~/.claude/settings.json
tasty codex install     # add the Tasty entry to [hooks] in ~/.codex/config.toml
```

- Hooks you added yourself are preserved as they are. Running it several times does not create duplicates.
- **Run it again after updating Tasty.** The hook command string is baked into the settings file, so a reinstall is needed to pick up the new format.
- The hook that reports a turn Claude Code ended on an API error (server overload, rate limit, authentication failure, and so on) as idle also arrives only with a reinstall. Until then such a child keeps looking "working".
- If Codex shows a `hook returned invalid ... JSON output` error, update and run `tasty codex install` again. It sets things up so Tasty's status reporting does not mix into Codex's hook response.
- These hooks do not run when you use Claude Code outside Tasty.
- To remove: `tasty claude uninstall` / `tasty codex uninstall`.

Select the actual Codex home or profile when it differs from the default. Use one location
option. Other hooks and model settings are preserved.

```sh
tasty codex install --codex-home /absolute/codex-home
# To select a profile file directly:
tasty codex install --config-file /absolute/codex-home/work.config.toml
```

When the installed hooks run successfully in the current session, Tasty receives state and session information.

- When an agent finishes a response or asks a question, an **attention border** lights up on that Surface and a badge appears on the Workspace in the sidebar (waiting for a question takes priority, in yellow).
- When you close and restore a Tab, or restart Tasty, the same session resumes (`claude -r` / `codex resume`).
- Receiving a child’s state and results also requires [parent setup](#4-receiving-completion-notifications). Completion lands in the completion log as one line, and a Claude Code parent uses Monitor to subscribe to it.

## 2. Launching

```sh
tasty claude launch --workspace myproj --directory ~/proj --task "Fix the tests"
tasty codex launch --workspace review --directory ~/proj
```

This creates a new Workspace and runs the CLI in its terminal. If you omit `--workspace`, the name is `claude` / `codex`.

For Codex you can attach approval and sandbox policies with `--approval untrusted|on-request|never`, `--sandbox read-only|workspace-write|danger-full-access`, and `--full-auto` (see "Codex approval policy" below).

For Claude Code you can set the permission mode with `--permission-mode` (see "Claude permission mode" below).

<a id="3-driving-child-agents-spawn--tell"></a>

## 3. Delegating work to another agent (spawn / tell)

From a Claude Code session, use the following command to launch another agent. The agent that delegates the work is called the parent, and the new agent is called the child. The new agent opens in a tab within a pane of the chosen workspace, and Tasty records this relationship.

```sh
tasty claude spawn --workspace workers --cwd ~/proj --role tester --nickname t1 \
  --prompt "Run cargo test and report the cause of any failures"
tasty codex spawn --workspace workers --cwd ~/proj --sandbox read-only \
  --prompt "Review the diff that was just committed"
```

| Option | Meaning |
|---|---|
| `--workspace <ID or name>` | Required. The Workspace the child goes into |
| `--pane <ID>` | A specific Pane of the Workspace (default: the first Pane) |
| `--cwd <path>` | The child's working directory |
| `--role <label>` | Role label. Used to pick recipients with `broadcast --role` |
| `--nickname <name>` | Name shown on the Tab |
| `--prompt <text>` | First instruction sent right after launch |
| `--surface <ID>` | Parent Surface (default: yourself) |

`spawn` **returns immediately**. There is no separate wait command. With the child’s hooks and [receiving setup](#4-receiving-completion-notifications) in place, the child’s reported idle state is delivered to the parent. Idle does not establish task success.

It is safer for the parent to put children in a **different Workspace** than its own. You cannot spawn into a remote mirror Workspace.

Afterwards, send further instructions to the child or inspect its state.

```sh
tasty claude tell "Run clippy this time too" --surface 57       # multi-line allowed, submits automatically
tasty claude children                                       # child list (index, surface, state)
tasty claude state --surface 57                             # idle / needs_input / active / exited
tasty claude broadcast "Report your progress\r" --role tester   # send to all of a role at once (\r submits)
tasty claude kill --child 0                                 # terminate by index
tasty claude respawn --child 0 --prompt "Start over"          # restart in the same place
tasty claude parent --surface 57                            # the parent of this child
```

`tasty codex …` has the same subcommands (`tell` / `children` / `state` / `broadcast` / `kill` / `respawn` / `parent`).

When there are too many children, a warning is attached to the spawn response. Change the threshold at **Settings** › **Plugins** › **Claude Code** / **Codex** › **Spawn child warning threshold** (Codex default 6).

## 4. Receiving completion notifications

Completion lands in the same log file as one line per event, whatever the parent is, and for both Claude and Codex children. Input requests,
interruptions, errors, and process exits are state notifications too; they do not establish task success.
When a Claude child ended its turn on an API error, the end of the completion line says so and names the error kind (for example `overloaded`).

### Receiving through Claude Code's Monitor

When the parent is Claude Code, subscribe to the completion log with Monitor.

```text
Monitor({ command: "tail -n0 -F \"$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log\"", persistent: true })
```

Log messages follow the app language. The file is truncated at 256 KiB. When a claude child and a
codex child write to the same log, **a line is either there whole or not there at all** - two lines
never interleave and a line is never cut in the middle. **Restarting tasty clears
the completion log the previous run left behind** - lines written before a restart cannot be read
back afterwards. You can read the file without Monitor, but file reading alone guarantees neither
automatic resumption nor permanent retention.

What the log keeps is **the tasty instance you are running right now**, and the size limit is
**bytes only**. There is no time limit and no limit on the number of files - the completion log of
a tab you closed stays where it is until the next restart. At 256 KiB the file is **emptied whole
rather than trimmed back to the last 256 KiB**, so anything you had not read yet goes with it.
What disappears here is whole lines too - a line is never cut in the middle. How much was thrown
away is written to the log at that moment, so you can trace afterwards why a notification never
arrived. If you read the completion log yourself and pick up where you left off, also look at the
`<number>.log.meta` file next to it. Its `retention_start` is the total number of bytes emptied so
far. Compare your read position with that value to learn how much disappeared unread while you
were away. A Monitor subscription does not need this file.

**Do not run two copies of tasty against the same data folder.** The one that starts later wipes
the completion-log folder as it boots, which takes the live log the earlier one was writing to.
Give each copy its own data folder with `TASTY_HOME` if you need both.

### Resuming Claude automatically after an API error

When a temporary API error such as a server overload ends Claude's turn, Tasty can send a "please continue" message for you after a short wait so the work carries on. **It is off by default.** Turn it on at **Settings** › **Plugins** › **Claude Code**.

- **Resume automatically after a temporary API error (server overload)** — on/off.
- **Seconds to wait before resuming** — 10 seconds by default. Any value from 1 second to one day (86400 seconds) works, and a value outside that range is moved to the nearest end.
- **Consecutive automatic resumes before giving up** — 5 by default. When failures in a row reach this number, Tasty stops sending and shows one notification.
- Errors that would fail the same way again — rate limits, authentication failures, billing problems — are not resumed.
- If you type something yourself while it waits, nothing is sent. If you pressed any key or pasted anything into that surface after the turn began (there may be a half-written message in the input box), it does not resume.
- The number of automatic resumes is in the surface meta `claude-auto-resume-count`.
- This needs the hooks installed by `tasty claude install`. Run it again after updating Tasty.

## 5. Codex approval policy

`tasty codex spawn/launch/respawn/reboot` accept Codex's approval and sandbox policies as flags.

`launch`/`spawn`/`respawn` bypass shell `codex` aliases and functions in sh/bash/zsh and Windows Git Bash, running Codex installed on `PATH`. Specify approval and sandbox options through the flags below or global settings. Alias/function bypass for `reboot` applies on Linux/macOS. Windows `reboot` keeps its existing launch behavior and does not yet bypass aliases or functions.

- **Approval**: `--approval untrusted|on-request|never`. If you pass nothing, it runs with **`never`** — to prevent automation from getting stuck forever at an approval prompt. Specify `untrusted` / `on-request` only when a person is beside it to approve.
- **Sandbox**: `--sandbox read-only|workspace-write|danger-full-access`. If not given, the Codex default. `read-only` suits children used for review and cross-checking.
- `--full-auto`: bypasses both approval and sandbox. Cannot be combined with `--approval`/`--sandbox`.
- The global defaults are **Default approval policy** / **Default sandbox mode** at **Settings** › **Plugins** › **Codex**. Per-call flags take precedence.

In environments where nested sandboxes are not possible, such as containers, if specifying `--sandbox` fails with something like `RTM_NEWADDR: Operation not permitted`, use `--full-auto`. This hint is also attached to the completion notification.

## 6. Claude permission mode

`tasty claude launch/spawn/respawn/reboot/child-profile` accept the child's permission mode as a flag.

- `--permission-mode acceptEdits|auto|bypassPermissions|manual|dontAsk|plan` — passed straight through to Claude Code.
- **If you pass nothing, no flag is added at all.** The child starts with the Claude Code settings you already use. Unlike Codex it does not quietly become "never ask" — Claude Code has no separate sandbox axis, so making it stop asking is the same as letting it run unrestricted.
- If a child pausing for approval would break an unattended run, name the mode you want on that call. A paused child’s state is also delivered through the configured [receiving channel](#4-receiving-completion-notifications).
- The global default is **Default permission mode for child sessions** at **Settings** › **Plugins** › **Claude Code**. It defaults to **Inherit** (no flag), and per-call flags take precedence.
- If the settings JSON behind `--profile` / `--profile-file` sets `permissions.defaultMode`, it cannot be combined with `--permission-mode` — the two decide the same thing, so you get an error asking you to pick one.
- A mode given to `reboot` / `child-profile` applies **to that restart only**. It is not carried over when the tab is restored later.

<a id="6-restarting-a-session-reboot"></a>

## 7. Restarting a session (reboot)

After changing hooks or settings, relaunch the agent with the same session.

```sh
tasty claude reboot --surface 57 --delay 5
tasty codex reboot --surface 58
```

After the specified delay, Tasty stops the process and resumes the same session. When an agent calls this on **itself**, it should make this its last action of the turn, since stopping the process interrupts any remaining response. Restarting a child does not interrupt the parent's response.

<a id="7-claude-session-profiles-and-the-stop-gate"></a>

## 8. Claude session profiles and the Stop gate

Claude Code reads hooks only once, at startup. To attach extra hooks and permissions to a specific session only, register a profile and pass `--profile` at launch.

```sh
tasty claude profile-register strict --file ./strict.json   # register a settings JSON under a name
tasty claude profile-list
tasty claude spawn --workspace w --profile strict           # applies to this child only
tasty claude reboot --profile strict                        # carried over to later restarts too
tasty claude child-profile --child 0 --profile strict       # attach persistently to a child
```

Use a **Stop gate** to have an agent review a checklist before finishing its response. Enable the built-in `continue-checklist` gate and connect it to the session.

```sh
tasty claude checklist-enable                               # turn the gate on (checklist-disable turns it off)
tasty claude spawn --workspace w --profile continue-checklist
```

- If the agent puts `[[TASTY-CHECKLIST-DONE]]` at the end of its response it passes; otherwise it receives the checklist again. When the round limit (default 3) is reached it passes automatically.
- The limit is **Default gate round limit** at **Settings** › **Plugins** › **Claude Code**.
- To create your own gate: `tasty claude gate-register <name> --body-file <file> [--sentinel <string>] [--rounds N]`. The body must contain the sentinel string. Check with `gate-list` / `gate-show`.

## Troubleshooting

- **No completion notification arrives** — check the child CLI’s hook installation and execution, then [receiving setup](#4-receiving-completion-notifications). If the parent is Claude Code, check that Monitor subscribes to the completion log. Hook delivery failures are recorded in `~/.tasty/hook-failures.log`. Plugin logs: `tasty plugin logs com.tasty.claude --follow`.
- **`reboot` fails with "claude-session-id meta not set"** — the session-start hook failed to record the session ID. Set it directly with `tasty surface-meta set --key claude-session-id --value <session ID>`.
- **The child is not spawned and you get an "occupied" error** — the target Workspace is being attached from a remote, or is a mirror. Use another Workspace.
- **No notifications when launched from the app icon on macOS** — Tasty calls `tasty` again when it writes notifications, but Tasty adds its own executable path to PATH automatically, so this is normally not a problem. If it still fails, look at `hook-failures.log`.

<a id="what-to-read-next"></a>

## Keep exploring

- [Task workflows](tasks.md) — Running spawn and tell as one dependency graph.
- [Hooks · notifications · webhooks](hooks-notifications.md) — Completion notices and approval gates.

Reinstalling the Codex integration also registers SessionEnd. When the current execution ends, the completion wait for that task ends with it.
