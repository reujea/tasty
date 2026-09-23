<!-- source-hash: b7395825d68d -->
<a id="remote-attach"></a>

# Working remotely (attach)

Connect to work running on another computer and continue it in your own Tasty window. SSH connects you to a remote Tasty workspace, where you can view its terminal layout and output and operate it. This connection is called **attach**, and the remote workspace shown locally is a **mirror**.

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
| `tasty-attach` | The attach spec — the connection (a reference to an ssh profile, or entered directly) plus the remote tasty path · port discovery mode | attach and remote queries use **only this kind** |

Trying to attach directly with an `ssh` profile is refused. Always create one `tasty-attach` profile.

### From the GUI

1. The tools button in the sidebar → **Tools** menu → **Remote connections…**.
2. **Remote profiles** tab → **+ Add profile**. Enter the name · host · user · port · shell and save. If you leave the shell as `auto`, saving connects once over SSH to detect the shell.
   - Hosts already in `~/.ssh/config` can be created directly from the **From ssh config** section below the profile list with **Add profile** on the right of the host. Only the alias is stored, so changes to the ssh config are followed as they are. A host you already created shows **in profiles** instead of the button. The list is read once when the popup opens, so after editing the ssh config, reopen the popup.
   - Register key files in the **Passkey** tab first, then pick them from the profile's Passkey dropdown. Secret values are referenced only by path in `~/.tasty/passkeys.toml`.
3. **Attach** tab → **+ Add attach**. Enter a name and, under **Connection**, choose **SSH profile** (referencing the one created above) or **Direct (inline)**.
   - **Remote tasty** group: **Executable** (the remote tasty path, default `tasty`) · **Port mode** · **Port file**. If tasty is not on the remote PATH, enter the full path in Executable or set the port mode to `file-unix`.

### From the CLI

```sh
# ssh profile
tasty tool remote-profile add-ssh --name gx10 --host 10.0.0.5 --user me --port 22 --identity ~/.ssh/id_ed25519
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
4. A mirror Workspace with the **REMOTE** tag appears in the sidebar and focus moves to it.

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

- A Workspace attach mirrors the terminals inside it, including the split structure. Image Surfaces show their content as well, and clicks and key presses reach the remote. The explorer can only be browsed — renaming, deleting and opening files are blocked. Markdown Surfaces show the remote document in this computer's theme, and the refresh button at the document's top right fetches the latest content again. When the document changes on the remote, the content isn't replaced on its own — only the refresh button changes color (if the remote Tasty is a headless build, the color doesn't change, so press it yourself to check). While the connection is lost and waiting to reconnect, the document shows a notice that the connection was lost instead of the old content, and once it reconnects it fetches the latest content on its own. Relative image paths and file links inside a remote document don't open, and the address bar can't open another file. HTML Surfaces still only take up their place; their content is not shown yet.
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
- A mirror attached without a mapping disappears from the window when the connection drops. If it was the only Workspace left in that window, the window stays open and a new terminal Workspace takes its place.

## What you can do inside a mirror

- Keyboard input · mouse go straight to the remote terminal. The remote re-lays out to match the size of your Pane.
- Splits, new Tabs, closing · moving Tabs, Surface conversion, and reopening a closed item are **executed on the remote** and the result is reflected in the mirror. Creating a Surface of a type the remote does not have fails with a toast. When an agent requests a split, new Tab, close, move, or conversion (including a markdown navigation or an image open), or a file open without specifying where to open it, and that fails on the remote, it is only logged, with no toast.
- **Reopening a closed item** (`Ctrl+Shift+T` by default) inside a mirror brings back a Tab that was closed on the remote — that Tab is a remote terminal, so your input goes to the remote too. Its earlier scrollback is fully visible only in the remote window (a mirror starts from the visible screen).
  - A Tab that an agent closed from the CLI inside a mirror is not brought back — not even by reopening from the remote window. Reopening only covers what a person closed by hand.
  - A reopen only brings back what was closed **in that remote Workspace**. If the remote has nothing to reopen you just get an informational toast and nothing happens — a Tab you closed on your own side is never created inside the mirror instead. Go back to your own Workspace and press the same key to reopen yours.
- A Tab that closes on the remote because its shell ended (`exit`) disappears from the mirror right away too.
- The remote terminal's completion · input-needed indicators (border · badge) also arrive in the mirror as they are.
- If part of what the remote was sending is lost, a "Part of the remote screen was lost" toast appears and the mirror re-attaches on its own to fetch the screen again (a mirror attached without a profile does the same). After it re-attaches, the same screen may appear once more in the scrollback. If this happens during a clipboard image upload, the upload ends as failed and you can try again — it is not resent automatically, because the remote may already have saved it.
- Pasting a clipboard image uploads it to the remote and inputs the **remote path**. Text paste works as usual.
- You cannot create child agents in a mirror Workspace with `tasty claude spawn` and the like. Launch them directly on the remote instance.
- Closing the last terminal of a mirror removes the remote Workspace itself and disconnects.

## Releasing the occupation (on the remote side)

When a Workspace in your Tasty has been attached from elsewhere and become read-only, a **Force detach** notice appears on the Surface. That notice only shows while you are looking at that Workspace, so you can also detach straight from the sidebar — right-click the occupied Workspace card > **Force detach**, and a confirmation appears in the middle of the screen; confirming releases it on the spot. You do not have to switch to that Workspace. You can also detach from the CLI.

```sh
tasty remote attach --force-detach --workspace 3    # release the occupation of this instance's Workspace 3
tasty remote attach --force-detach 57               # Surface 57
```

While the other side's user is attached, attempting a split · new Tab · `spawn` locally in that Workspace is refused with an "occupied" error. Use another Workspace, or force-detach and then proceed.

## Where files received from the remote are stored

Files transferred over the attach channel are stored in `~/.tasty/transfers/`, with a folder cap of 500 MiB. Change it under **Settings** > **General** > **Remote transfer**, or with the CLI.

```sh
tasty settings get-remote-transfer
tasty settings set-remote-transfer --dir ~/Downloads/tasty --max-mb 2000
```

## Troubleshooting

| Symptom | What to check |
|---|---|
| `kind='ssh'` refused | You passed an ssh profile to `--profile`. Create a `tasty-attach` profile and specify that |
| Remote tasty not found | Is Tasty running on the remote? Verify the port with `tasty tool remote-profile detect --name <n>`. If it is not on PATH, specify the executable path or `--port-mode file-unix` |
| Timeout | Host reachability · firewall. ssh profile `--option ConnectTimeout=<seconds>` |
| SSH connection failed | Authentication · host key. First check that `tasty tool ssh <ssh profile>` connects |
| Workspace attach refused | One of the terminals inside it is already occupied by another client. Force-detach on the remote |
| Screen flickers briefly when first attaching | Normal behavior while the remote re-lays out to your Pane size |
