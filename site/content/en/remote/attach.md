<!-- source-hash: f447d6aef045 -->
<a id="remote-attach"></a>

# Working remotely (attach)

Connect over SSH to a workspace in Tasty running on another computer. The remote workspace displayed locally is called a mirror. While connected, the mirror holds the workspace; users on the remote side can view it but cannot type into it.

## Concepts

- attach brings in and shows a Workspace from a **remote Tasty that is already running**. It does not open a new remote shell. Tasty must be running on the remote machine (either the GUI or a windowless [headless build](../getting-started/install.md#headless-build)).
- The remote Workspace is held **exclusively** by the side that mirrors it. Meanwhile the user on the remote side can see that terminal but cannot type into it (read-only). It comes back when the connection is closed.
- A mirror Workspace carries a sky-blue **REMOTE** tag in the sidebar. It disappears on restart and is not saved.
- The remote Tasty does not need to be on PATH. It is enough that the port file (`~/.tasty/tasty.port`) can be read.

## 1. Create a profile

There are two kinds of profile. Both are stored in `~/.tasty/remote-profiles.toml`.

| Kind | Holds | Used for |
|---|---|---|
| `ssh` | **Connection details only** — host · user · port · identity | `tasty tool ssh` connections; referenced by attach profiles |
| `tasty-attach` | Attach connection settings (a reference to an ssh profile, or entered directly) plus the remote tasty path · port discovery mode | attach and remote queries use **only this kind** |

Trying to attach directly with an `ssh` profile is refused. Always create one `tasty-attach` profile.

### From the GUI

1. The tools button in the sidebar → **Tools** menu → **Remote connections…**.
2. **Remote profiles** tab → **Add profile**. Enter the name · host · user · port · shell and save. If you leave the shell as `auto`, saving connects once over SSH to detect the shell.
   - Hosts already in `~/.ssh/config` can be created directly from the **From ssh config** section below the profile list with **Add profile** on the right of the host. Only the alias is stored, so changes to the ssh config are followed as they are. A host you already created shows **in profiles** instead of the button. The list is read once when the popup opens, so after editing the ssh config, reopen the popup.
   - Register key files in the **Passkey** tab first, then pick them from the profile's Passkey dropdown. Secret values are referenced only by path in `~/.tasty/passkeys.toml`.
3. **Attach** tab → **Add attach**. Enter a name and, under **Connection**, choose **SSH profile** (referencing the one created above) or **Direct (inline)**.
   - **Remote tasty** group: **Executable** (the remote tasty path, default `tasty`) · **Port mode** · **Port file**. If tasty is not on the remote PATH, enter the full path in Executable or set the port mode to `file-unix`.

The Save button is temporarily disabled while saving. If saving fails, your input and the error stay visible so you can edit and retry. Canceling or closing the remote connections popup leaves an already accepted save running; its later completion does not overwrite a reopened form. A new Passkey with an existing name is rejected without replacing the existing credential.

When editing a Passkey, typing and then clearing the value prevents a delayed preload from filling it again. Canceling and reopening the editor accepts only the new edit’s preload. If the credential cannot be loaded, the current editor shows an error.

### From the CLI

CLI profile and Passkey mutations apply their changes to the latest list under a file lock. Corrupt or unreadable lists are preserved. SSH detection for `add-ssh` starts after the save succeeds and cannot overwrite a profile changed by another command during detection.

An inline Passkey file, the credential list and a profile reference are saved in that order, without a transaction across files. If an error reports that an earlier stage was saved, check the list and reference state. Removing a list entry may succeed even if cleanup of its managed file fails. User files registered as `path` are never deleted. Windows inline storage requires PowerShell ACL setup and verification; failure prevents the secret payload from being written.

Fixed an issue where passkeys could not be saved in Windows PowerShell 7. If setting file access permissions (ACLs) fails, the error message now includes the cause.


```sh
# ssh profile
tasty tool remote-profile add-ssh --name gx10 --host 10.0.0.5 --user me --port 22 --identity ~/.ssh/id_ed25519
tasty tool remote-profile add-ssh --name win --host 10.0.0.9 --shell powershell --label "Windows build PC"   # set the shell and display name
tasty tool remote-profile list-local                      # list ~/.ssh/config aliases
tasty tool remote-profile import --from devbox --name devbox   # alias → ssh profile

# tasty-attach profile (references the ssh profile)
tasty tool remote-profile add-attach --name gx10-attach --ssh-ref gx10 \
  --remote-tasty /home/me/tasty/target/release/tasty --port-mode auto

# inspect · edit · remove
tasty tool remote-profile list [--kind ssh|tasty-attach]
tasty tool remote-profile show --name gx10-attach
tasty tool remote-profile edit --name gx10-attach --port-mode file-unix
tasty tool remote-profile detect --name gx10-attach       # actually connect to the remote and verify the port
tasty tool remote-profile remove --name gx10-attach
```

Port modes:

| Value | Behavior |
|---|---|
| `auto` (default) | Tries `subcommand` → `file-unix` → `file-windows` in that order |
| `subcommand` | Runs `<executable> port` on the remote to obtain the port |
| `file-unix` | Reads `~/.tasty/tasty.port` on the remote (no tasty execution) |
| `file-windows` | Reads the port file of a Windows remote |

If you pass `--port-file <path>`, that file is read with the highest priority.

To read the IPC port of the instance currently running on the remote machine, run `tasty port` on that machine — this is what port mode `subcommand` calls on the remote.

<a id="2-check-that-the-remote-is-alive"></a>

## 2. Check the remote connection

```sh
tasty remote check --profile gx10-attach       # alive: gx10 (port 41234, version …, N workspaces)
tasty remote workspaces --profile gx10-attach  # list of remote Workspaces (id · name · whether occupied)
```

`remote check` only reports alive once it has found the port and actually received a response. If the program has stopped but its port file remains, the result is `dead`. The failure cause is reported as one of four kinds: SSH connection failure / remote instance not running / response could not be parsed / timeout.

Connection attempts do not wait forever — 10 seconds for the SSH connection, 20 seconds per step, 45 seconds total. On a slow link, raise it yourself with something like `--option ConnectTimeout=30` on the ssh profile.

## 3. Attach from the GUI

1. In the sidebar, **right-click the New workspace (+) button** or right-click the empty background → **Add remote workspace**. (If categories are on, it is also in the category header's right-click menu.)
2. Pick a profile under **Attach profiles** on the left. The list of remote Workspaces appears on the right (if there is no response within 20 seconds it stops and shows **Retry**).
3. Pick a Workspace and **Connect**. One that someone else is already attached to is shown as **in use** and cannot be picked.
   - Picking the first row, **New workspace**, creates a Workspace on the remote with a default name and then attaches to it (**Create & connect**). A Workspace created this way stays on the remote.
   - Once the list appears, move between rows with `↑` / `↓` and press `Enter` to connect.
4. A mirror Workspace with the **REMOTE** tag appears in the sidebar and focus moves to it. If connecting or preparing the mirror fails, a failure toast with the reason appears in that window. Nothing is shown if you have closed the window.

## 4. Attach from the CLI

```sh
tasty tool attach --list                                   # list tasty-attach profiles
tasty tool attach gx10-attach --workspace 3                # mirror the whole Workspace (run in a terminal)
tasty tool attach gx10-attach 57                           # a single Surface only
tasty tool attach gx10-attach 57 --raw                     # wire my terminal directly to the remote Surface (exit with Ctrl+\)
tasty remote attach --profile gx10-attach --workspace 3    # long form of the same thing as tool attach
tasty remote attach --ssh me@10.0.0.5 --workspace 3        # one-off, without a profile
tasty remote new-workspace --profile gx10-attach --name build --cwd /home/me/proj   # create a Workspace on the remote
```

Common options:

| Option | Used with | Effect |
|---|---|---|
| `--send <input>` | `tool attach`, `remote attach` | Sends input once right after attaching (escapes `\n` `\r` `\t` `\xNN` are decoded) |
| `--send-to <surface>` | `tool attach`, `remote attach` | In a Workspace attach, the remote Surface ID that receives `--send` |
| `--dump-after <ms>` | `tool attach`, `remote attach` | Collects output for the given time, prints the mirrored screen and exits (for checks without a GUI) |
| `--remote-tasty <path>` | `remote attach·check·workspaces·new-workspace` | Path to the remote tasty binary when you don't use a profile (default `tasty`). With `--profile`, the profile's value is used |
| `--remote-port-mode <mode>` | Same as above | Port discovery mode (`auto`, `subcommand`, `file-unix`, `file-windows`) |
| `--json` | `remote workspaces`, `remote new-workspace` | Prints the result as JSON |

- A Workspace attach mirrors the terminals inside it, including the split structure.
- Surfaces that a plugin draws itself, such as Image Surfaces, show their content as well, and clicks and key presses reach the remote. This includes Surfaces from plugins you installed yourself.
- In the explorer you can change files on the remote computer (since 0.12.2). New folder, new file, rename, move to trash, and copy, cut, paste and drag-and-drop within the same connection are carried out by the remote Tasty, and the progress display, name-conflict popup and result card are the same as in a local explorer. The remote Tasty must also be 0.12.2 or later. The following limits apply.
  - Remote operations are not added to the undo history. To revert one, use **Undo** on its result card.
  - Pasting and drag-and-drop between local and remote, or between two different remotes, are refused.
  - If the remote Tasty is an older version, the menu items and toolbar buttons that change files are greyed out, and their tooltip (or a notice, for a shortcut) asks you to update Tasty on that computer.
  - **Open in system**, Show in file manager, **Add to Favorites** and **Open in New Tab** aren't available in a remote explorer.
  - When you create a new folder or file or rename an item on the remote, the name you typed stays in place, greyed, with a spinner next to it until the remote answers. You can't edit or cancel it meanwhile. On success the new item is selected or the rename popup closes; on failure you can edit the same name again.
  - If the connection drops during an operation, the remote stops at the item it was on. The explorer on this side keeps a warning card saying the result is unknown until you close it, and reads the folder again once the connection is back. The card has no Retry or Undo button.
- In the explorer, double-clicking a markdown file opens a markdown tab for it on the remote computer, and the tab appears in this window too; a file at the same path on this computer is not read. If the type's default handler can't be used remotely, only the handlers that can are offered; a type with no such handler only shows a notice.
- Markdown Surfaces show the remote document in this computer's theme, and the **Reload** button left of the address bar fetches the latest content again. When the document changes on the remote, the content isn't replaced on its own — the Reload button fills with the accent color instead (if the remote Tasty is a headless build, the color doesn't change, so press it yourself to check). While the connection is lost and waiting to reconnect, the document shows a notice that the connection was lost instead of the old content, the address bar and its buttons are all disabled, and the buttons say **Disconnected from the remote**. Once it reconnects it fetches the latest content on its own. Relative image paths and file links inside a remote document don't open.
- Type a path on the remote computer into a remote Markdown address bar and press `Enter` or **Go** to switch the same Surface to that remote file. This computer's recent files don't appear in the autocompletion. `Esc` puts the current document's remote path back. If it can't be opened, the path you typed stays and the reason appears under the address bar. If the file doesn't exist on the remote, it says **Can't open … on the remote: file not found.**; if the remote refuses for another reason, the remote's own reason is shown as sent, in English. If the connection is lost before the remote answers, it says **Connection lost before the remote answered. Reconnect and try again.** — whether the remote changed anything is unknown then, so check after reconnecting. If there's no answer after 30 seconds, **Still waiting for the remote…** appears under the address bar, and a late answer from the remote still applies. Typing again or pressing `Esc` clears these lines, and a failure that arrives after that doesn't bring them back. If the file opens late, the document still switches to it.
- The folder button left of a remote Markdown address bar (**Open a remote file…**) opens the file chooser in the remote document's folder on the remote computer. The remote file you pick opens in the same Surface, and if the remote can't open it, a toast tells you.
- While you're viewing a mirror, when the **Open Markdown File** popup converts a Surface to markdown or opens a new tab, the path you type in the popup or pick with **Browse…** opens as a file on the remote computer. A relative path is taken from the remote terminal's current folder. A file opened this way is not added to this computer's recent files list.
- On Linux, Windows, and macOS, you can view the body of a remote HTML file. Choose **Allow** or **Decline** before receiving a large file. Scripts follow your local HTML settings; allowing scripts displays the body already received. The address is read-only, and **Reload** receives the latest body. A completed body remains visible after disconnection, with Reload disabled. Connections to older versions may still show an empty area. If the remote version also supports it, referenced files such as CSS and images under the same folder are received too. Large files are presented one at a time. At the 2MB document limit, choose **Continue** or **Stop**. Large-file and document questions do not disappear however long you wait. If the remote transfer expired in the meantime, **Allow** or **Continue** requests that file again, and asks once more if its size has changed. Continue also receives the remaining files; Reload also retries failed files. The count of declined or failed files remains until Reload. Moving to a child HTML file checks it as a new document. When the page cancels a file request, its receive wait ends while other requests for the same file remain active. On Linux X11, Windows, and macOS 27 or later, HTTP and HTTPS addresses also open when both versions support them. Requests run on the remote computer, so submitting a form or using a page button can change the remote service. On Windows and macOS, however, WebRTC connections from a page whose scripts are allowed can leave this computer directly. Tasty removes the page's WebRTC features to reduce this but does not guarantee that every path is closed. Same-origin requests are allowed by default; requests to other origins follow your local HTML remote-content setting. Browser security policies still apply. Existing receive consent applies at 100000 encoded bytes per response and 2000000 encoded bytes per document. Allowing scripts reuses the received body without resubmitting a form. An explicit Reload sends the original request again and keeps that document’s native cookies. Disconnection stops scripts; reconnecting or replacing the remote source does not copy cookies or script permission. If private browser storage preparation briefly overlaps, it waits for a bounded time; storage limits and safety-check failures still appear as errors. On macOS, a remote address such as `localhost` or `127.0.0.1` opens under a separate name such as `localhost.tasty.localhost`. The address bar shows the original address, but page scripts read the separate name, and images or requests inside the document that name the remote `localhost` with a full address do not load. Relative paths and following links still work. Native Wayland on Linux and macOS 11–26 do not support remote web-address rendering and do not connect from this computer as a fallback. The minimum supported macOS version remains 11.
- `--raw` works only at the Surface level.
- Unless you pass `--no-reconnect`, it automatically tries to reconnect when SSH drops.
- If a slow link makes you miss part of the screen the remote was sending, the remote says so and the attach re-attaches on its own to fetch the screen again, printing a one-line notice in your terminal. An attach that prints the screen and exits re-attaches up to three times per run; if it still misses something, it prints the result and then warns that part of the screen may be missing. `--raw` re-attaches with no limit and redraws the screen from scratch. Input given with `--send` is not sent again.

To bring it up as a mirror Workspace inside a running Tasty window, use "Set up automatic attach on a Workspace" below.

## 5. Set up automatic attach on a Workspace

If you map a remote target onto a local Workspace, every time you switch to that Workspace Tasty sets up the SSH tunnel and attaches the mirror on its own.

```sh
tasty new workspace --name gx10-dev --ssh-profile gx10-attach --remote-workspace 3
tasty set workspace --id 5 --ssh-profile gx10-attach --remote-workspace 3
tasty set workspace --id 5 --ssh me@10.0.0.5 --remote-workspace 3      # without a profile
tasty set workspace --id 5 --clear-mapping                              # remove the mapping
```

- `--remote-workspace` is the remote Workspace **ID**. Look it up with `tasty remote workspaces`.
- A mapped mirror keeps the Workspace and its scrollback as they are when the connection drops and reconnects in the background (retrying with intervals growing from 0.5 seconds up to 30 seconds; at 30-second intervals when the remote is occupied by someone else). After 20 failures it stops and notifies you with a toast — leaving that Workspace and coming back triggers one more attempt immediately.
- If the first connection fails (a profile, port or SSH problem), it retries with intervals growing from 0.5 seconds up to 30 seconds. Switching back to that Workspace or changing the mapping triggers one more attempt immediately.
- If the mapped target is this same Tasty, it does not connect. A mapping set with `--ssh` is not retried until you change the mapping, and a profile mapping is checked once more when you switch back to that Workspace. `tasty remote refusals` lists the refused attempts.
- If a mapping cannot connect (the target is this same Tasty, the SSH profile does not exist, or the endpoint cannot be prepared), a warning mark appears on that Workspace's row in the sidebar, after the name and to the left of the notification count badges. When the sidebar is collapsed, a small warning mark sits on the top-left corner of the Workspace icon. Hover it to see the target and the reason. Opening that Workspace shows a banner at the top with the same details, and **Remove mapping** clears the mapping right there. × closes the banner for this visit only; it comes back when you leave the Workspace and return. The mark goes away when you change or remove the mapping, or when the connection succeeds.
- A mirror attached without a mapping disappears from the window when the connection drops. If it was the only Workspace left in that window, the window stays open and a new terminal Workspace takes its place.

## What you can do inside a mirror

- Keyboard input · mouse go straight to the remote terminal. The remote re-lays out to match the size of your Pane.
- If the remote does not answer a new size, Tasty asks once more. If there is still no answer (about 10 seconds), a banner saying **Couldn't sync the terminal size with the remote** appears at the top of the Workspace with the affected tab names. When several tabs share a name, as split tabs do, the name appears once with a count, such as `Shell ×2`. **Retry** (**Retry all** for several) asks again, and × only closes the banner. If the remote accepts that size later, the banner goes away on its own. A remote running an older Tasty without this feature never shows the banner.
- Splits, new Tabs, closing · reordering Tabs, Surface conversion, and reopening a closed item are **executed on the remote** and the result is reflected in the mirror. Creating a Surface of a type the remote does not have fails with a toast. When an agent requests a split, new Tab, close, move, or conversion (including a markdown navigation or an image open), or a file open without specifying where to open it, and that fails on the remote, it is only logged, with no toast.
- **Move Surface** from the Surface context menu is executed on the remote only within the same mirror Workspace. Moving between a mirror and another Workspace is blocked.
- Moving with **Move Tab** · **Move Pane** from the Tab context menu is not available in a mirror. If the item to move or the destination is a mirror, a toast says it was blocked and nothing changes.
- **Reopening a closed item** (`Ctrl+Shift+T` by default) inside a mirror brings back a Tab that was closed on the remote — that Tab is a remote terminal, so your input goes to the remote too. Its earlier scrollback is fully visible only in the remote window (a mirror starts from the visible screen).
  - A Tab that an agent closed from the CLI inside a mirror is not brought back — not even by reopening from the remote window. Reopening only covers what a person closed by hand.
  - A reopen only brings back what was closed **in that remote Workspace**. If the remote has nothing to reopen you just get an informational toast and nothing happens — a Tab you closed on your own side is never created inside the mirror instead. Go back to your own Workspace and press the same key to reopen yours.
- A Tab that closes on the remote because its shell ended (`exit`) disappears from the mirror right away too.
- The remote terminal's completion · input-needed indicators (border · badge) also arrive in the mirror as they are.
- If part of what the remote was sending is lost, a "Part of the remote screen was lost" toast appears and the mirror re-attaches on its own to fetch the screen again (a mirror attached without a profile does the same). After it re-attaches, the same screen may appear once more in the scrollback. If this happens during a clipboard image upload, the upload ends as failed and you can try again — it is not resent automatically, because the remote may already have saved it.
- Pasting a clipboard image uploads it to the remote and inputs the **remote path**. Text paste works as usual.
- Pressing [Screenshot to clipboard](../using/terminal.md#screenshot-to-clipboard) (`Ctrl+Alt+S`) inside a mirror captures the screen on your computer, saves the file on the remote, and puts both the remote path and the image on the **remote clipboard**. Your own clipboard does not change. If the remote Tasty is an older version that cannot copy images, only the path goes in and the toast says so. If the file cannot be sent because the remote connection is gone, the toast says it was saved only on your computer, and if the connection drops before the remote answers, it says the copy was not confirmed.
- Pass a mirror Workspace number to `--workspace` and the child of `tasty claude spawn` or `tasty codex spawn` runs on the remote computer, while you manage it as a child inside that mirror. See [Claude · Codex](../agents/claude-codex.md#creating-children-in-an-attached-remote-workspace) for how to use it.
  - While the connection is down, that child's state shows as `unknown`. Once you reconnect to the same remote Tasty, a completion that happened in the meantime is delivered once.
  - If the remote Tasty restarts, completions of children created before the restart are not delivered. On a remote running an older Tasty, no child is created and you get an error.
- Restarting the shell of a mirror surface (`tasty surface respawn-terminal`, `tasty terminal respawn`) or attaching a background PTY as a tab inside a mirror (`tasty pty attach-surface`) is also refused, because it would put a local shell where the remote terminal is. Run them directly on the remote instance.
- Applying a Tab or Pane preset to a mirror Workspace (the preset popup, `tasty preset apply`) is refused for the same reason. A Workspace preset creates a new Workspace, so it still works.
- Closing the last terminal of a mirror removes the remote Workspace itself and disconnects.

## Closing and disconnecting a mirror

**Disconnect Workspace** closes your mirror and leaves the remote work in place. **Close Workspace** closes the workspace on the remote computer. Choose Disconnect to leave the remote work running.

Right-clicking a mirror card shows two items together. Your own regular Workspace cards only have **Close Workspace**.

- **Close Workspace** — actually closes that Workspace on the remote computer. All its tabs, panes and terminals, and the programs running in them, end on the remote side. Only after the remote answers that it closed does your mirror disappear, with **Closed the workspace on the remote Tasty.** If the remote refuses, the mirror stays and a failure notice appears. The closed Workspace can be brought back with Restore closed in the remote window (running programs do not come back).
  - Closing the Workspace with a shortcut does the same. Closing a mirror's last tab, pane or terminal with a shortcut, which falls back to closing the Workspace, also closes the remote Workspace. To keep the remote running, use **Disconnect Workspace**.
  - If it was the remote's last Workspace, the remote window does not close; a new terminal Workspace opens in its place.
  - While the connection is down or reconnecting, the close is not sent and only a notice appears. It is not sent either when the remote Tasty is an older version without this feature. In that case update the remote, or use **Disconnect Workspace**.
  - If the connection ends before the remote answers, you are told that whether it closed is unknown. The Workspace may still be running on the remote, so reconnect and check.
  - When you close a mapped mirror on the remote, the same mapping does not reconnect automatically. The mapping is not deleted; point it at another remote Workspace or clear it yourself.
- **Disconnect Workspace** — closes only your mirror and ends the connection. The remote Workspace and its programs keep running, and the remote side can type again. It works while reconnecting too, and then also stops the reconnect attempts. If it was the last Workspace, the window does not close and a new terminal Workspace opens.
  - A mapped mirror keeps its mapping but does not attach again right away. Go to another Workspace and come back to the mapped one to reconnect. For a mirror attached without a mapping, attach again with **Add remote workspace** to see the same Workspace and output.
  - A drop caused by the network still reconnects automatically as before. Only a disconnect you chose stops it.

These two items differ from **Force detach** in the remote window. Force detach removes someone else's connection on the remote computer; Disconnect closes the mirror you attached, from your side.

## Releasing the occupation (on the remote side)

When a Workspace in your Tasty has been attached from elsewhere and become read-only, a **Force detach** notice appears on the Surface. That notice only shows while you are looking at that Workspace, so you can also detach straight from the sidebar — right-click the occupied Workspace card > **Force detach**, and a confirmation appears in the middle of the screen; confirming releases it on the spot. You do not have to switch to that Workspace.

There is no CLI command that disconnects someone else's attach. Detaching is done from the window so that an agent on the server does not cut off a person working remotely by mistake. On a server without a GUI, such as a windowless Tasty, ending the SSH session the remote side came in through closes the connection and releases the occupation. If the remote side reconnects automatically, it may attach again.

While the other side's user is attached, attempting a split · new Tab · `spawn` locally in that Workspace, applying a Tab or Pane preset from the CLI (`tasty preset apply`), or attaching a background PTY as a Tab (`tasty pty attach-surface`) is refused with an "occupied" error. Use another Workspace, or force-detach and then proceed.

## Where files received from the remote are stored

Files transferred over the attach channel are stored in `~/.tasty/transfers/`, with a folder cap of 500 MiB. Change it under **Settings** > **General** > **Remote transfer**, or with the CLI.

```sh
tasty settings get-remote-transfer
tasty settings set-remote-transfer --dir ~/Downloads/tasty --max-mb 2000
```

When you pass the `--raw` output of a CLI attach to another command and that command exits first, the attach also ends normally.
Other write errors, such as a permission problem on the output device, are reported as a failure.

## While waiting for a response

When you create a new tab or split in a mirror, Tasty shows its place from the next frame. You cannot send input to that new area until the server confirms it, but you can select an existing Tab or area and keep working. If you select somewhere else while waiting, a late completion does not move your selection back. A failed request removes only its own temporary area.

After 200 ms without a response, spinners appear in the Tab and sidebar, and **Waiting for response** with the elapsed time appears at the top-right of the Workspace. With several requests, it shows their count and the longest wait. Hover over it to see the longest wait for each request kind on its own line. Closing, moving, reopening, converting, and folder, document, or Git queries also show a wait without blocking the screen or input. Native WebViews stay visible and use the Tab and sidebar indicators.

The spinner appears only in place of the Tab icon, the sidebar status dot, or the avatar in the collapsed sidebar; Tab titles and Workspace names do not change. Hover over a waiting Tab or Workspace to see **Attaching…** under its name.

For ordinary input, a server that supports input acknowledgments confirms that preceding input has reached the app's processing boundary. This does not mean a shell command or program has finished. Normal input without output and long-running commands do not keep the indicator on; older servers are not asked for unsupported input acknowledgments. If the connection drops, the existing reconnection notice takes over.

## Troubleshooting

| Symptom | What to check |
|---|---|
| `kind='ssh'` refused | You passed an ssh profile to `--profile`. Create a `tasty-attach` profile and specify that |
| Remote tasty not found | Is Tasty running on the remote? Verify the port with `tasty tool remote-profile detect --name <n>`. If it is not on PATH, specify the executable path or `--port-mode file-unix` |
| Timeout | Check whether you can reach the host and whether the firewall permits the connection. ssh profile `--option ConnectTimeout=<seconds>` |
| SSH connection failed | Authentication · host key. First check that `tasty tool ssh <ssh profile>` connects |
| A notice or error that the profile points to this Tasty itself | The profile's host (including a host name or LAN IP) and port file, or the port given with `--into-gui --target-port` (including an SSH tunnel you opened yourself), point to the Tasty you are using. You cannot attach to itself. To attach to another Tasty on the same computer, give that instance's port file |
| An error that `--into-gui` is not supported in this build | The local Tasty that received the command is a windowless [headless build](../getting-started/install.md#headless-build). It has no window to show the mirror in, so it does not accept the request. Send it to a Tasty running with its GUI |
| An `ssh` process remains after Tasty was force-quit | On macOS the remote attach tunnel is cleaned up only on a normal exit (Quit from the menu or closing the window). End a leftover `ssh -N -L` process yourself after a force quit or a crash. On Linux and Windows it ends together with Tasty |
| Workspace attach refused | One of the terminals inside it is already occupied by another client. Force-detach on the remote |
| Screen flickers briefly when first attaching | Normal behavior while the remote re-lays out to your Pane size |
