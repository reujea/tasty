<!-- source-hash: 39ed8f66f8af -->
# Troubleshooting

Find help by symptom for installation, input, display, restoration, and notification issues. For CLI connection failures or slow responses, also see connection diagnostics. If the problem remains, report it with your environment details and logs.

## Install · first launch

- **Windows: a "Windows protected your PC" warning appears** — the binary is not code-signed. Click **More info → Run anyway**.
- **macOS: it will not open, saying "unidentified developer"** — that is Gatekeeper. Allow it in **System Settings > Privacy & Security**, or run `xattr -dr com.apple.quarantine /Applications/Tasty.app` in a terminal.
- **Linux: the AppImage does not run** — it lacks the execute bit or FUSE is missing. Run `chmod +x Tasty-*.AppImage` first; if it still fails, start it with `./Tasty-*.AppImage --appimage-extract-and-run`.
- **Linux: it does not start, with `GLIBC_2.39 not found`** — your distribution is older than the build baseline (Ubuntu 24.04), for example Ubuntu 20.04 · Debian 11. There is no build for older distributions.
- **Linux `.tar.gz`: it exits saying a library is missing** — `tasty` lists the file names of the missing shared libraries and exits. Use the search command it prints for your distribution (`apt-file search` · `dnf provides` · `zypper what-provides` · `pacman -F`) to find the package that provides each library, then install it. To have the required libraries installed automatically, use the `.deb` / `.rpm` instead.
- **It exits right away and the log says "No GPU adapter found"** — Tasty could not find a usable GPU adapter (Vulkan / DirectX 12 / Metal). This error is not shown on screen; it is written only to `~/.tasty/debug.log` and to the terminal output (when launched from a terminal). Install or update the driver. On Linux, Tasty uses Vulkan when `libvulkan1` / `vulkan-loader` is present, OpenGL when it is not, and software rendering if that fails too. To run on a server or VM without a graphical desktop, use a [headless build](../getting-started/install.md#headless-build).
- **Windows: "Git Bash not found"** — Tasty uses Git Bash as the shell on Windows. Install Git for Windows, or set the bash path yourself in **Settings** > **Terminal** > **Shell**. On the first run a **Choose a shell** screen appears instead of a terminal. Enter the path to bash.exe there and press **Use this shell**, or press **Quit**, install Git for Windows, and start Tasty again.
- **A different shell opens instead of the one I configured** — If there is no file at the configured shell path (on Windows, also when the file is not bash or zsh), Tasty opens terminals with another shell it finds (Git Bash on Windows, your login shell or `$SHELL` on macOS and Linux). Tasty does not change the shell value in your settings file, so once you fix the path or reinstall that shell, new terminals use it again. The substitute shell is recorded in `~/.tasty/debug.log`, and the **Terminal** tab in **Settings** shows a notice ("Git Bash not found" on Windows, "No shell found at the configured path" on macOS and Linux).
- **macOS and Linux: the Choose a shell screen appears at start, or Settings shows a shell error** — The configured shell path is a folder or a file without permission to run. Tasty does not switch to another shell in this case; it shows the cause instead. On the first run, the check line of the **Choose a shell** screen says "This path is a folder, not a program" or "This file has no permission to run". While Tasty is running, the **Terminal** tab in **Settings** shows the same cause and new terminals cannot open. Set the path to a shell program, or make the file executable with `chmod +x`. On macOS and Linux, shells other than bash and zsh, such as fish, work as long as the file can be run.
- **A "Could not start the terminal" screen appears right after starting** — Tasty could not open the first terminal. The "Details" part of the message gives the cause. Fix the cause and start Tasty again. If a new window fails for the same reason while Tasty is running, the **Could not open a new window** notice shows the cause, and the windows already open keep working.
- **Notifications show "Recent files and tutorial progress are not being saved"** — Tasty could not open `~/.tasty/state.db`, which holds the recent-files list and tutorial progress. You can keep using Tasty, and terminals and layout restore are not affected, but the previously saved recent list and tutorial progress cannot be loaded, and changes made in this session are not saved. Folders you visit in Explorer during this session can still show up in that window's recent list while Tasty runs, but they are not kept for the next run. The notification body gives the cause. "The database is locked" means another Tasty is already running. "corrupted" / "schema version mismatch" means you can quit Tasty, back up `~/.tasty/state.db`, and delete it to start fresh from the next run. Saving resumes only after you fix the cause and start Tasty again.
- **A "Tasty could not start" screen appears and no terminal opens** — an internal step failed while Tasty was preparing its first window. The "Details" part gives the cause. Press **Quit**, Esc or Enter to close it and Tasty exits. Start it again, and if it keeps happening, check that the data folder (`TASTY_HOME`, or `~/.tasty` by default) is usable and look at `~/.tasty/debug.log`. Where Tasty cannot show a window, the same message appears in a message box (on Linux without zenity, as a desktop notification).
- **You typed `tasty` inside a Tasty terminal but no new window appeared** — run with no arguments inside Tasty, it shows the help instead of opening a new window. For a new window use `tasty new window` (it leaves the focus on the window you were looking at).

The install procedure itself is in [Install](../getting-started/install.md).

## macOS permission prompts

**Symptom** — a permission prompt appears mid-task and stalls what you were doing. It happens the first time a command inside the terminal (or an agent inside it) touches a new folder.

**Cause** — when a command run inside the terminal reads a file, macOS attributes that access to Tasty (Terminal.app · iTerm2 behave the same). macOS only asks at the moment of actual access, so unless you grant the permissions up front, the prompt lands in the middle of your work.

**Fix — grant them up front, in one go.** In **Settings** > **General** > **Permissions**, click **Request all permissions**: it asks one at a time, in the order Downloads · Documents · Desktop folders → (if connected) external · network volumes → screen recording. The next prompt only appears once you answer the previous one, and the window works normally while they are up. Items you have already allowed or denied are not asked again, so pressing the button more than once is harmless. Press it again after mounting a new volume and only that volume is asked about.

Tasty does **not** raise these prompts automatically at startup. That keeps a first launch from throwing a stack of unexplained prompts at you. Instead, if any permission is still missing, a notice at startup leads you to this screen.

**How to answer**

| Permission | If you do not allow it | To change it later |
|---|---|---|
| Folder access (Downloads · Documents · Desktop · volumes) | Commands that read or write that folder may fail | System Settings > Privacy & Security > Files and Folders |
| Screen recording | The `Ctrl+Alt+S` screenshot-to-clipboard feature only shows a "Screen recording permission is required" notice. Once denied, it is not asked again | System Settings > Privacy & Security > Screen & System Audio Recording |

You can see the current state in the same tab (only shown on macOS). Full Disk Access is not part of that button because no app can request it. Use **Open System Settings** in the Full Disk Access row to open System Settings and add Tasty yourself. Folder permissions cannot be checked automatically, so they read "Cannot check automatically".

- **A "Some permissions are not granted" notice appeared** — it appears at every start while Tasty does not seem to have Full Disk Access or screen recording. The **Settings** > **General** > **Permissions** tab shows which ones and what state they are in. **Open permission settings** opens Tasty's own Permissions screen. Tasty cannot request Full Disk Access itself, so from there use **Open System Settings** in the Full Disk Access row to open System Settings and add Tasty to the list yourself. Granting it makes the file access prompts (other apps' data · Downloads · Documents · Desktop · volumes) go away, and the notice stops appearing from the next start. Controlling other apps (Automation) · screen recording are separate permissions, though, and remain. There is no setting to disable this notice. Tasty checks the current state at each startup.
- **I granted everything, but folder prompts still appear** — the startup notice only looks at permissions it can check. Folder permissions (Downloads · Documents · Desktop · volumes) are left out because accessing a folder to check can open a permission prompt. Granting Full Disk Access covers those folders too.
- **The notice keeps coming back even though I granted it** — downloaded releases and builds you make yourself are both ad-hoc signed, so after an update or a rebuild macOS sees a different app. Tasty can then stay switched on in the Full Disk Access list in System Settings while that entry no longer applies. If you have started Tasty with the permission granted before, the notice recognises this case and tells you to remove Tasty from the list and add it again. If the app is unchanged and the permission was turned off, it tells you to turn it back on, or to add Tasty again if it has disappeared from the list (for example after `tccutil reset`). The Full Disk Access row in **Settings** > **General** > **Permissions** keeps the "Not granted" status and adds one line under it with the same fix (remove and add it again, since the entry may look on but no longer applies, or turn it back on, and add it again with + if it is no longer in the list). If you build Tasty yourself, create the "Tasty Dev" certificate once with `./scripts/macos-codesign-identity.sh --create` and use it to sign later builds. This does not guarantee that permissions will persist; check their state in System Settings too. Right after switching, also remove the old Tasty entry from the list and add the new one.
- **The Full Disk Access status shows "Unknown"** — macOS offers no API to ask whether an app has this permission, so the status is an estimate. When the file used for the estimate does not exist on your macOS version, there is nothing to judge from, so it reads Unknown and this permission alone does not raise the startup notice (a missing screen recording permission still does). No feature is blocked by this value.
- **"Tasty would like to access data from other apps" keeps appearing for every app folder** — paths like `~/Library/Application Support/<app>` are asked per app, so **Request all permissions** cannot cover them up front. Granting Full Disk Access as above makes them go away.
- **"wants to control another app" appears when you use `osascript`** — the Automation permission must be approved per target app, and Full Disk Access does not cover it. There is nothing Tasty can do in advance.

## Korean input stops after restarting the input method on Linux X11

When Tasty detects that the XIM input method server has exited or been replaced, it stops sending
requests to the old server and reconnects to the new one. Local input may be used until the connection
recovers, so Korean composition may be temporarily unavailable. Check that ibus is running again and
that its Korean engine is selected.

This does not prevent every freeze: the server may exit during connection negotiation or just after
a liveness check, or remain alive without answering. If the whole window stays frozen, preserve the
hang report described below when reporting the problem. This applies to X11, not the input paths on
Wayland, macOS or Windows.

## The `tasty` command cannot connect

- **`No running tasty instance found (port file not found at …)`** — there is no port file at the path checked by the CLI. If the path in the message is not `~/.tasty/tasty.port`, the command is looking at a different home directory (`TASTY_HOME`). The message follows your configured language (`general.language`, English by default), so it is worded differently if you set another one. A wrong argument (broken JSON, a `--cwd` folder that does not exist, and so on) is reported before this message, so if you see this message the argument checks performed before connecting passed.
- **The port file exists but it cannot connect** — it may be left over from an earlier run, or the CLI may be looking at another instance’s path. Check the path in the error and `TASTY_HOME`, then confirm that the Tasty instance using that file has stopped. Only delete the file if it belongs to that stopped instance. Do not delete it solely because a process-name search found nothing. Tasty may leave this file on exit if it was replaced after startup or another process is saving it.
- **Startup reports a port-file save error** — check that your account can read and write the file and its parent folder. Tasty keeps an original it cannot read. Replacing the complete file also requires write access to the parent folder.
- **It connects, but replies are slow or end with a line such as `Error (-32061)` or `Error (-32065)`** — see [CLI response delays and connection diagnostics](cli-diagnostics.md) for what to do for each error code and how to read `tasty list pressure`.


- **`tasty: command not found`** — inside a terminal that Tasty opened it is on the PATH automatically, and a `.deb` · `.rpm` · `.msi` install puts it on the PATH for other terminal apps too. With any other install method you have to add it yourself. The path for each install method is in [Install location](../getting-started/install.md#install-locations).

## A button or shortcut says the action could not be completed

When a user action fails, such as changing the layout, moving a divider, resetting categories, or starting a remote connection, one failure notice appears. If the divider position could not be committed, it returns to its previous position. A category reset failure means the settings were not changed. Check the current state before trying again. If it keeps failing, report the problem with the records from the same time in `~/.tasty/debug.log`. Agent command failures are returned through command responses or logs instead of on-screen toasts.

## A notice says a link or folder couldn't be opened

Terminal links, files opened with the system default app, and the homepage and install folder in the plugin window are handed to the operating system's default program. If that program can't be started, a notice appears.

- **Linux** — Check that `xdg-open` is installed, or that the `BROWSER` environment variable points to a browser that can run. Tasty looks for them in the `PATH` of the environment it was started from.
- **macOS · Windows** — Check in the system settings that a default app is associated with that address or file type.

If nothing happens and no notice appears, the default program did start but could not open it. Check that program.

## A command cannot open the file picker

- **A script or an agent that tries to open the file picker over IPC gets a `-32016` error** — this is expected. The file picker is opened by you from **Tools** > **Open File…** in the sidebar, or by a plugin (for example when you press **Browse…** in the Markdown open-file popup). A command that opened it would take the input focus away from where you were typing, and the picked path would not come back to that command anyway. Scripts and agents should pass the file path directly.

<a id="i-do-not-know-which-port-my-dev-server-came-up-on"></a>

## Finding the port of a development server

Open **Listening ports…** from the **Tools** menu in the sidebar. It shows the TCP ports opened by processes started from Tasty terminals, together with the port · process · Workspace · Tab.

- By default only the LISTEN state is shown. If the list is empty and "No ports match the state filter" is displayed, ports in other states may still exist — use the **State** button on the right of the filter row to turn on other states and click **Apply**.
- To include processes outside Tasty, turn on **Show all (system-wide)**.
- Click a row to select it and use **Copy address** to put `host:port` on the clipboard.
- Add a port to favorites with the star icon and it always stays at the top, surviving restarts (`~/.tasty/port-favorites.toml`). Favorites changed in another window show up right away.

## The window freezes or crashes

- **The window does not respond to clicks · key input · the CLI at all** — when it freezes for more than 5 seconds, `~/.tasty/crash-reports/hang-*.log` is written automatically. A `Render phase` of `acquire` / `submit` / `present` records where rendering was stalled. A GPU driver issue is one possibility, so check for driver updates. Tasty does not recover on its own, so force-quit it and start it again.
- **It exited suddenly** — look at `~/.tasty/crash-reports/crash-*.log`. Attach this file when you report the problem.

### A notice says saving stopped while Tasty was running

If the **Tasty stopped and needs a restart** notice appears in the top right of the window and one error is left in Notifications, saving the workspace · pane · tab layout to `~/.tasty/structure/journal.db` failed. To protect the saved layout, Tasty stops applying further changes.

- In this state, key input and clicks do not reach the terminal or any other part of the window, and `tasty` commands are rejected. **Continue** or × on the notice only closes the notice.
- Quit Tasty with the window's close button and start it again. In this state it quits right away without asking again.
- The cause is written at the bottom of the notice and also in `~/.tasty/debug.log`. If the same notice comes back after a restart, check the free space and permissions of the disk that holds the store, and attach the log when you report the problem.

## My settings or window layout look like they were reset

If your layout does not return, first check **Restore layout on startup** and the notifications. If Tasty says it could not open the store, check the location and cause shown in the notice. A temporary layout started while the original store is unreadable is not retained for the next launch. This restriction does not apply to project files you edit in the terminal.

- **The settings went back to their defaults** — if `~/.tasty/config.toml` cannot be parsed as TOML, Tasty starts from the defaults. Your original file is not deleted. It is left where it is, and moved next to it as `config.toml.bak` the moment settings are saved over it. Fix that file and rename it back to `config.toml` and your settings come back as they were. Which line failed is written to `~/.tasty/debug.log`.
- **A notice says the saved workspace could not be recovered** — Accumulated journal size alone does not block terminal startup. If restore cannot continue, Tasty preserves the original and opens an empty workspace for this run. Terminal input and new tabs remain available, but this temporary layout is not kept for the next run. Check free disk space and data folder permissions, then restart. Actual disk write failures are reported as failures; the notification gives the cause.
- **A saved window layout was not restored** — first check that **Restore layout on startup** under **Settings** > **General** is turned on. The Workspace · Pane · Tab arrangement is stored in `~/.tasty/structure/journal.db`. Files such as `01.json` under `~/.tasty/layouts/`, written by earlier versions, are read only once, when they are first carried over into the new store; after that, editing or deleting them does not change the restored layout. If Tasty cannot open the store, it starts with a new default workspace and shows an error notification together with a **Couldn't restore your last session** notice in the top-right corner of the window. The notice gives the cause and the location of the store, the notification holds the full error, and the same text is written to `~/.tasty/debug.log`. The store file is not changed, moved or deleted. This run's layout records (workspaces, tabs and what Tasty needs to restore them) are lost when Tasty quits (files changed in your terminals are not affected), and the next start reads the store again; once read permission is back, your original layout is restored. Close the notice with **Continue** or ×; the terminal keeps taking input while it is shown. If another Tasty is using the same data folder, Tasty does not start and shows the cause instead. Include the log when you report the problem. <!-- TODO verify: whether there is a user procedure to recover the layout from a damaged journal.db -->
- **"An old layout file could not be imported"** — a file written by an earlier version, such as `~/.tasty/layouts/01.json`, could not be read as a layout. That window starts with a new default workspace and the file is kept. If Tasty has no permission to read the file, an error notification and a notice in the top-right corner of the window appear. Once the window has started with a new layout, the file is not imported again even if you fix it.
- **"Some terminal contents were not restored"** — while importing a file from an earlier version, the saved contents file of some terminals was missing or unreadable. The layout is restored, and only those terminals start without their earlier output.
- **"Selected tabs could not be restored"** — the workspace and tab arrangement was restored, but the record of the last selected workspace and tabs could not be read. Tasty starts with the first workspace and tab selected. The unreadable record is kept at the location named in the notification (usually `~/.tasty/structure/damaged/`) and is never deleted automatically.
- **"Part of the saved layout could not be restored"** — Tasty could not read the saved records of one window. Only that window is not restored and starts with a new layout; other windows are restored as usual. The unreadable records are kept, not deleted. The notification gives the layout number and the cause.
- **"Restored from an earlier save point"** — the latest save point could not be read, so Tasty rebuilt the layout from an earlier save point and the records after it. Every later record is applied again, so the layout is unchanged.
- **A tab opened as an empty terminal with "A tab could not be restored"** — Tasty could not read what was saved for that tab (working folder, earlier output, restore command). Other tabs and workspaces are restored as usual, and that tab starts fresh with your current shell settings. The unreadable original stays in the store; it is not deleted. The notification names the workspace and tab and gives the cause.
- **"Layout is no longer being saved"** — a surface capture request collecting the current layout and terminal contents exceeded the 64 MiB limit. The limit applies to one request saving all surfaces in the same running engine; it is not a disk space limit for the entire store or an individual workspace. Tasty skips the whole request rather than saving part of it. The last saved state is kept, but later changes will not be restored at the next start. Clear the scrollback of terminals with a lot of output. Tasty tries again when the layout changes or the history gets smaller, and resumes saving once the request fits within the limit. For the same cause, the notification appears only once until a save succeeds again.
- **"Saving keeps failing"** — saving the layout or the selected tabs failed 3 times in a row without a success. One or two passing failures are retried at the next save and are not reported. Tasty keeps retrying, but until a save succeeds, changes since the last save will not be restored at the next start. Fix the cause shown in the notification (low disk space, permissions and so on) and the next save returns to normal. The full cause is also written to `~/.tasty/debug.log`.
- **A notification said saving is blocked until you move or delete the `.bak` files** — Tasty tried to move your original settings file (`config.toml`) aside but all nine slots (`.bak` through `.bak.9`) are already taken, so there is nowhere to put it. It stops saving rather than delete your file, and it stays that way for the whole session. Move or delete the backups you no longer need, then start Tasty again.
- **A notification said the settings file could not be read** — the file is there but Tasty could **not read** it (a permission problem or a disk error). In that case Tasty leaves the file alone and does not save over it, so the defaults on screen never replace your real settings. Fix the permissions or move the file aside, then start Tasty again.

  ```sh
  ls -l ~/.tasty/config.toml
  ```

## Running Tasty again

- **Launching from the app menu or a launcher brings the open window forward instead of a new window** — only one Tasty runs per data folder (`~/.tasty/`, or the folder set with `TASTY_HOME`). A new launch does not start a second process; it is handed to the running Tasty. Windows hidden to the tray or minimized come back, and the window you used last comes forward. Whether it actually comes to the front is up to the OS, so in some environments the taskbar entry only flashes. On Wayland, if the desktop does not support window activation requests, a new window opens instead of the open one.
- **Running `tasty` in a terminal opens just one new window** — running it with no arguments from a terminal, script, or agent outside Tasty is the same as `tasty new window`. Inside a terminal that Tasty opened, it shows the help instead of a new window. The focused window and hidden windows stay as they are. On Windows, launching from the terminal you are using (the one in front) passes on the permission to come forward, so the open window may come forward as it does from the app menu.
- **A "Tasty is already running." box appears** — the launch could not be handed to the running Tasty and the running Tasty does not respond. When the request did arrive but is handled late, no box appears and a window opens a moment later. Usually the box means the running Tasty was still starting up, or on Linux the session bus could not be reached. The cause is written to `launch.log` in the data folder (`~/.tasty/launch.log`). Each launch adds several lines, one per step. On Linux without `zenity`, a desktop notification appears instead of the box.
- **Starting Tasty right after force-quitting it takes a moment** — it waits up to two seconds for the leftover lock to be released before starting.
- **You need two separate Tastys** — start one with a different `TASTY_HOME`.

  ```sh
  TASTY_HOME=~/tasty-second tasty
  ```

## Notifications do not arrive · there are too many

- **Notifications do not appear in the OS notification center** — Tasty currently does not send notifications to the OS notification center. Even when the window is in the background, notifications are shown inside the app: the **Notifications** panel, the Surface border, and the sidebar badge. The panel opens with `Ctrl+Shift+I`. If notifications do not appear inside the app either, check that **Notifications enabled** under **Settings** > **General** > **Notifications** is turned on.
- **A notification for every bell (`\a`) is noisy** — turn off **Settings** > **Terminal** > **Show bell notification**. In `config.toml`, that is `bell_notification = false` under `[general]`. Bell hooks still fire.
- **There is no sound** — **Settings** > **General** > **Notifications** > **Sound** is off by default. Even when it is on, consecutive notifications from the same source within the merge interval are combined into one, so the sound plays only once.

The full list of settings is in [Hooks, notifications, and webhooks](../agents/hooks-notifications.md#settings).

## Claude · Codex hooks do not work

**Symptom** — completion notifications from child agents do not arrive. The Surface border · sidebar badge does not light up even after the agent finishes its response. Restoring a Tab or restarting Tasty does not resume the same session. `tasty claude reboot` fails with `claude-session-id meta not set` (with the Korean UI, "claude-session-id 메타데이터 미설정"). The state shown by `tasty claude children` differs from reality.

**Cause** — the hooks are not installed, or an old hook command is still sitting in the settings file after you updated Tasty.

**Fix** — reinstall the hooks. Running it several times does not create duplicates.

```sh
tasty claude install    # ~/.claude/settings.json
tasty codex install     # ~/.codex/config.toml
```

If that still does not help, look at `~/.tasty/hook-failures.log` and `tasty plugin logs com.tasty.claude --follow`. Details are in [Working with Claude and Codex](../agents/claude-codex.md#troubleshooting).

Fixed an issue on Windows and macOS where Codex sessions in Tasty terminals disconnected immediately after connecting.

### The parent agent does not receive a child result

Child attention indicators and parent delivery are separate. For a Claude Code parent, re-run install and check the notification mod marker and delivery in a new session. Check Monitor only when the mod is unavailable; do not enable both. See [connection and recovery](../agents/claude-codex.md).

### Creating a semaphore or barrier fails with "invalid char"

The name contains a character that is not allowed, such as an uppercase letter or a space. The position in the message is counted in the name you typed, starting from 0. Rename it using only lowercase letters, digits, `.`, `_`, and `-`. See [Concurrency limits and signals](../agents/tasks.md#concurrency-limits-and-signals).

## A plugin has stopped

- **`tasty plugin list` shows enabled but not running** — a plugin that fails to run 3 times within 10 seconds is stopped automatically. Check the cause with `tasty plugin logs <id>`, then start it again with `tasty plugin enable <id>`.
- **Right after `plugin enable` it shows running, then a moment later it does not** — `plugin enable` returns without waiting for the plugin to connect to Tasty. A plugin you just turned on therefore shows as running before it connects, and requests sent in the meantime are delivered once it connects. If it does not connect within 10 seconds, that counts as a failed run and it is no longer running. Check the cause with `tasty plugin logs <id>`.
- **A bundled plugin is broken** — copy it again from the bundle with `tasty plugin upgrade-builtins --force`. Plugin data (bookmarks · profiles and so on) is kept.
- **macOS: built-in plugins do not run** — the Gatekeeper quarantine flag left on the app may have been carried over to the plugin copies and blocked them. Tasty does not remove this flag by itself. Check the cause with `tasty plugin logs <id>`; to remove it yourself, run `xattr -dr com.apple.quarantine /Applications/Tasty.app ~/.tasty/plugins` in a terminal and restart Tasty.

<a id="files-to-check-first"></a>

## Log and settings file locations

All of them live under `~/.tasty/` (on Windows, `%USERPROFILE%\.tasty\`).

| File | Contents |
|---|---|
| `config.toml` | Settings. Values saved from the settings window end up here |
| `tasty.port` | The IPC port number of the running Tasty. Created when Tasty starts |
| `debug.log` | Warning-and-above log of the previous run. Cleared when Tasty starts again |
| `crash-reports/crash-*.log` | Crash reports — version · OS · location · message · backtrace |
| `crash-reports/hang-*.log` | Written automatically when the window freezes for more than 5 seconds |
| `launch.log` | Records and results of launching Tasty again while one is already running |
| `hook-failures.log` | Records of Claude / Codex hooks that failed to reach Tasty |
| `plugins-logs/<plugin id>.log` | Per-plugin logs. Also viewable with `tasty plugin logs <id>` |
| `state.db` | Data the app manages on its own, such as recent files |

If you need more detailed logs, run Tasty from a terminal with the log level raised. The variable is `TASTY_LOG`, not `RUST_LOG`.

```sh
TASTY_LOG=debug tasty 2> tasty.log
```

## Reporting a problem

Include the steps to reproduce, Tasty version, OS and architecture, and relevant error messages in a [GitHub issue](https://github.com/zilhak/tasty/issues). Before attaching logs, check paths and output for personal information.

- The output of `tasty --version`, plus your OS · version
- Steps to reproduce
- The matching `crash-*.log` / `hang-*.log` from `~/.tasty/crash-reports/`
- `~/.tasty/debug.log` from right after the symptom (it is cleared on the next start, so copy it first)
