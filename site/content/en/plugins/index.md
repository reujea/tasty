<!-- source-hash: 44378f729d23 -->
# Plugins

Plugins provide Markdown, image, and HTML viewers, along with Claude Code and Codex integrations. Manage bundled plugins or install new ones and review the permissions they need.

## What a plugin is

Features such as the Markdown viewer, the image viewer and the Claude Code integration are provided not by the Tasty core but by **plugins**. A plugin runs as a separate process, declares what it adds (Surface kinds · tool menu items · `tasty` subcommands · settings pages · file handlers) and which **permissions** it needs, and Tasty accepts its requests only within the granted permissions.

- Plugins are installed in `~/.tasty/plugins/<id>/`, with logs in `~/.tasty/plugins-logs/<id>.log`.
- The bundled plugins are installed automatically on first launch. After that they can be disabled or removed exactly like plugins you installed yourself. A bundled plugin you removed is not reinstalled on the next launch.
- When a plugin is disabled, the Surface kinds · commands · menu items it added disappear with it. This takes effect right away when you disable or remove it while Tasty is running: opening a new Surface of such a kind fails with the reason that the plugin providing it is turned off or has not reconnected yet. Surfaces you already opened stay as they are and pick up again when you enable the plugin, but their scroll position and any unsaved content are lost.

When you enable or disable a plugin or change its permissions, Tasty confirms the save before applying the runtime change. Installation and trust choices also report their result after saving. A failed or unconfirmed save is not reported as success. An enable saved during shutdown takes effect on the next launch; shutdown does not start a new plugin process. A change whose result remains unconfirmed at the shutdown deadline is not reported as successful or automatically replayed.

## Bundled plugins

| Plugin | id | What it does | Where you use it |
|---------|-----|---------|---------|
| **Markdown Viewer** | `com.tasty.markdown` | A Markdown Surface that renders `.md` files. Re-reads the file automatically when it changes | Right-click the tab strip > **New Markdown...**, open a `.md` in the explorer, `tasty markdown reload` · `recent` |
| **Image** | `com.tasty.image` | An image viewer and simple paint tool. Steps to the next · previous image in the same folder and saves as PNG | **New Image**, opening an image file, `tasty image open` · `save` · `export` · `next` · `prev` · `paste` · `list` |
| **HTML Viewer** | `com.tasty.html` | A Surface that shows HTML and SVG files and URLs in an embedded webview | **New HTML...**, opening an `.html` or `.svg`, `tasty html open` |
| **Clipboard Viewer** | `com.tasty.clipboard-viewer` | A popup that shows what is currently on the clipboard, classified as text · file · image · HTML. Keeps no history | **Tools** > **Clipboard Viewer**, `Ctrl+Shift+H` |
| **Git Viewer** | `com.tasty.git-viewer` | A read-only popup showing the status · log · diff of the repository in the current directory. With several worktrees, pick one on the left | **Tools** > **Git**, keybinding assigned by you |
| **Claude Code** | `com.tasty.claude` | Multi-agent commands that launch Claude Code inside Tasty, spawn child instances, send them messages and get notified on completion | `tasty claude launch` · `spawn` · `tell` … — [Working with Claude and Codex](../agents/claude-codex.md) |
| **Codex** | `com.tasty.codex` | Does the same as above for the Codex CLI | `tasty codex launch` · `spawn` · `tell` … — same page |

How to use each Surface kind is in [Managing and viewing files](../using/files.md). There are also demo · experimental plugins that ship only in development builds; they are not included in the distribution.

### Settings added by plugins

In the **Settings** window, plugin pages appear in two places.

- **Appearance** > **Markdown** — Font settings applied only to Markdown Surfaces · **Allow remote content** (off by default — blocks external http/https resources the document loads).
- **Appearance** > **HTML** — **Default zoom** (%) · **Color scheme** (follow theme / light / dark) · **Allow remote content** (off by default — blocks external http/https resources) · **Sandbox scripts** (on by default).
- **Plugins** > **Claude Code** — **Spawn child warning threshold** and so on.
- **Plugins** > **Codex** — **Spawn child warning threshold** · **Default approval policy** · **Default sandbox mode**.

A plugin that adds a Surface drawn as a web page, even one you installed yourself, applies the zoom, color scheme, allow remote content, and sandbox scripts values from its own settings page to that Surface, the same way HTML does. Values you have not changed follow the defaults the plugin sets, and on a Surface that supports the zoom shortcuts, the shortcuts change that plugin's zoom value.

The **Configure** button in the plugin window goes to these pages too.

In the light theme, the trusted verdict uses dark green text and a shield. It describes the signature verdict; it does not certify code review or runtime safety.

## The plugin window

Press the **Plugins** button at the very bottom of the sidebar. It has three tabs.

### Installed

The **Installed** tab. Pick one from the list on the left and the details appear on the right. **Filter installed…** above the list filters by name.

- The line under the name at the top of the details shows the authors and the plugin id. If the plugin has a homepage, it appears at the end of the same line. An `http://` or `https://` address is an underlined link that opens in your browser (if the browser can't be started, a notice says it couldn't be opened); any other value is shown as plain text.
- Rows in the list are marked **Disabled** or **Running**. If a plugin is enabled but fails to run, a red marker appears with the notice **Failed to connect. Check the plugin's configuration in Settings.**
- **Permissions** — The list of permissions this plugin has been granted. Read-only here; it cannot be changed.
- **Commands** — Keybinding commands added by the plugin, one command and its shortcut per line. On macOS the shortcut shows the keys you actually press, in the order Ctrl · Option · Shift · Command, and Option, Shift and Command follow the modifier display style (word or symbol) under **Settings** > **General**. Change the keys under **Settings** > **Keybindings** > **Plugins**.
- **Install path** · **Log** — The last section of the details. A long path wraps so it is shown in full, and you can drag to select and copy it. **Open folder** on the right of the heading row opens the install folder in your file manager. If the file manager can't be started, the plugin window shows a notice that the folder couldn't be opened.
- The bar under the details stays visible while you scroll the details.
  - The switch on the left — The label next to it shows **Enabled** / **Disabled**. Clicking the label also toggles it. Turning it off cleans up the process; turning it on starts it again.
  - **Configure** — Goes to the plugin's page in the settings window.
  - **Uninstall** — The bar turns into a confirmation in place, with **Cancel** and **Uninstall** buttons. Pressing **Uninstall** deletes the install folder; the plugin's settings stay until you delete them. For a **built-in** plugin, the note says it won't be installed again on the next launch. When the confirmation appears, keyboard focus is on **Cancel**, so Enter cancels right away; Esc also cancels, even after you switch to another tab. Selecting another plugin cancels the confirmation and brings back the normal bar.

### Attention

The **Attention** tab. Plugins whose registration was rejected or that failed to run are collected here with the reason. As for installed plugins, the top of the details shows the name, authors and id, plus the homepage link when it is an `http://` or `https://` address, with the description below. For a plugin whose signature is invalid, the manifest can't be trusted, so its description and homepage are not shown; **Description hidden — the signature is invalid.** appears in place of the description.

| Shown | Meaning | What to do |
|------|----|------|
| **Signature not trusted** | Signed with a key not in the trust list | The details show an **Unverified plugin** warning. Confirm the source, then copy the fingerprint with the **Copy fingerprint** button in the Signature section and compare. If you trust it, press **Register anyway** |
| **Signature invalid** | No signature, or verification failed. The detail shows the cause (such as a missing signature file) on one line | Getting a correct package from the distributor is safest. If you trust the source, you can use it with **Register anyway** |
| **Permissions changed** | An update changed the required permissions | Read the **newly requested** list and **Re-approve** |
| **Runtime error** | Enabled but failed while running | Check the **Log** |

### Add plugin

The **Add plugin** tab.

1. In the **Plugin folder** field, enter a folder containing `tasty-plugin.toml`, or pick one with **Find folder…**. The Tasty folder picker confirms the selected subfolder, or the current folder when no row is selected. Double-click enters a folder; Enter confirms it. Cancelling keeps the field unchanged.
2. Choosing a folder validates it immediately. If you typed a path, press **Verify**. The manifest card appears right under the field: name · version · id · authors · description · **required permissions** · surface kinds · source path, plus the homepage when the plugin has one. An `http://` or `https://` address is an underlined link that opens in your browser (if the browser can't be started, a notice says it couldn't be opened); any other value is shown as plain text. With several authors, the first one is followed by `+N`; hover to see them all. Editing the path clears the card, so press **Verify** again. If the folder has no `tasty-plugin.toml` or the file can't be parsed, a **Can't read tasty-plugin.toml** box takes the card's place with the reader's error on the line below. If the file reads but breaks a manifest rule (for example, the listed binary is not in the folder), a **tasty-plugin.toml is not valid** box appears in the same place with what is wrong on the line below. Fix the path and press **Verify** again.
3. The box under the card tells you how the signature was judged: **Signed by a trusted publisher**, **Permissions changed**, or **Unverified plugin**. For an unverified plugin the box gives the cause (a key not in the trust list, a missing public key file, no signature, or a failed signature check) and warns that the plugin runs with your account's access and that its permissions only limit what it can ask Tasty to do. When the judgment involves a key, the box shows the fingerprint, which you can copy with its button. A long fingerprint shows only its first and last 8 bytes with `…` in between; hovering or copying gives the full value.
4. The left of the bottom bar shows how many permissions the plugin will get. Press **Add plugin**. For changed permissions the same button reads **Trust & add**; pressing it records the new permission set in the trust list. Permissions only limit what the plugin can ask Tasty to do; they don't isolate the plugin program from your files, network or other programs. For an unverified plugin the button reads **Add anyway** (**Trust key & add anyway** for a key not in the trust list), and **Cancel** has focus at first, so Enter or Esc goes back without installing. Press it only if you trust where the plugin came from. The choice is remembered until the manifest changes or the plugin is removed; once installed, the plugin has no other restriction beyond its mark in the list. While adding, the button reads **Adding…**.
5. If the plugin is already installed, the button stays disabled and **Already installed** is shown on its left.

Installing grants the permissions the plugin requests. Review the permission list in the preview before adding it.

## Permissions

A plugin declares the permissions it needs in advance. Tasty rejects requests to the host when the required permission has not been granted.

Permissions define which features a plugin can request from Tasty. They do not isolate every file or network operation performed by the plugin process. Check the source and description of a plugin before installing it.

Common names and their meanings:

| Permission | What it allows |
|------|-------------|
| `surface.read` · `surface.write` | Reading the Surface list · state, creating · changing Surfaces |
| `fs.read` · `fs.write` | Reading · writing files |
| `clipboard.read` · `clipboard.write` | Reading · writing the clipboard |
| `terminal.spawn` · `terminal.write` · `terminal.read` | Creating terminals, sending key input, reading output |
| `notification` | Showing notifications |
| `process.spawn` · `network` | Running external processes, network |
| `ui.tool_item` · `ui.popup` · `ui.settings_page` | Adding tool menu items, popups, settings pages |
| `file_handler.define` · `file_handler.handle:<kind>` | Defining file-kind detection rules, being the one that opens files of that kind |
| `memory.read` · `memory.write` · `memory.secret` | Access to the agent memory store |
| `agent` · `approval` · `telemetry` | Agent collaboration · approval gates · telemetry |
| `agent.turn_report` | Reporting the start and end of an agent task turn only (opens no other agent collaboration feature) |
| `completion_strategy.define` | Defining how the task runner decides that a child agent has finished |
| `ipc.invoke:<prefix>` | Calling commands another plugin provides under `<prefix>` (for example `ipc.invoke:codex` for the Codex plugin's commands) |

Permissions granted to the bundled plugins:

| Plugin | Permissions |
|---------|------|
| Markdown Viewer | `surface.read` `surface.write` `fs.read` `file_handler.define` `file_handler.handle:markdown` `ui.settings_page` `ui.popup` |
| Image | `surface.read` `surface.write` `clipboard.read` `fs.read` `fs.write` `file_handler.define` `file_handler.handle:image` |
| HTML Viewer | `surface.read` `surface.write` `file_handler.define` `file_handler.handle:html` `file_handler.handle:svg` `ui.settings_page` |
| Clipboard Viewer | `clipboard.read` `ui.popup` `ui.tool_item` |
| Git Viewer | `ui.popup` `ui.tool_item` `fs.read` |
| Claude Code | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.read` `fs.write` `notification` `telemetry` `agent` `agent.turn_report` `ui.settings_page` `completion_strategy.define` `memory.read` `ipc.invoke:codex` |
| Codex | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.write` `notification` `ui.settings_page` `completion_strategy.define` `agent.turn_report` |

Removing or restoring individual permissions is done only from the CLI (below).

## The `tasty plugin` command

Use it from a terminal while Tasty is running. The output is JSON.

| Command | What it does |
|------|---------|
| `tasty plugin list` | The id · version · enabled · running state of installed plugins, whether one was turned off automatically, and its last start failure |
| `tasty plugin show <id>` | The full manifest · permissions · commands · running state |
| `tasty plugin install <folder> [--allow-unverified]` | Installs from a folder containing `tasty-plugin.toml`. Grants the manifest permissions as they are. An unverified plugin is installed only with `--allow-unverified` |
| `tasty plugin remove <id>` | Removes it |
| `tasty plugin enable <id> [--no-wait]` · `disable <id>` | Enables · disables it. `enable` waits until the plugin connects (up to 15 seconds) and fails with the cause if it does not start. If it is still starting after 3 seconds, it shows a progress line once |
| `tasty plugin logs <id> [--follow]` | Prints the log. `--follow` keeps showing new lines (stop with `Ctrl+C`) |
| `tasty plugin permissions <id>` | The permissions the manifest requires and those actually granted |
| `tasty plugin grant <id> <permission>` · `revoke <id> <permission>` | Grants · revokes one permission. Only permissions declared in the manifest can be granted |
| `tasty plugin doctor <id>` | Diagnoses the manifest — whether it has rules this version of Tasty does not understand |
| `tasty plugin upgrade-builtins [--force] [--restore-removed <id>] [--restore-removed-all] [--restart-running]` | Realigns the bundled plugins with the bundled version. `--restore-removed` brings back a bundled plugin you removed (can be repeated), and `--restore-removed-all` brings back every bundled plugin you removed. `--restart-running` restarts plugins whose binary was replaced (that plugin's Surfaces close briefly) |
| `tasty plugin extension list` | Extensions that plugins add to other plugins, and their state |
| `tasty plugin grant-agent-permission --agent <id> --permission <permission> [--ttl <secs>]` · `revoke-agent-permission` | Grants a temporary permission to a running agent session, or revokes it. Without `--ttl` it lasts until revoked |
| `tasty plugin list-agent-permissions [--agent <id>]` | Base and temporary permissions of agent sessions |
| `tasty plugin request-permission --agent <id> --permission <permission> [--reason <reason>]` | Shows the approval popup that asks whether to give an agent a permission, ahead of time. When an agent runs it, the agent can only name itself, and only the user can approve or deny this popup |
| `tasty plugin audit-query` · `audit-summary` · `audit-follow` · `audit-clear` | Queries · summarizes · follows live · deletes the allow/deny records of requests Tasty received (CLI, plugins, agents) |

`install` checks the signature before copying the folder. For an unverified plugin it warns about the cause and the risk, installs nothing, and shows the `tasty plugin install --allow-unverified <folder>` command to use if you trust the source. When the signature check refuses the install, it tells you the actual reason (no signature, a key you have not trusted, a public key file that is missing or does not match, and so on) and leaves nothing behind in the plugins folder, the permissions or the trust list. Fix the problem and run the same command again to install. If the plugin is registered but then fails to start, the installed folder stays; check `tasty plugin logs <id>` for the cause.

When a plugin does not start, a warning is left in the notification list. If it fails several times in a short time and Tasty turns it off automatically, a separate error notification is left and the plugin is not started until you turn it on again. Repeated failures of the same plugin that you did not turn on again are grouped into one. Each time you turn it on yourself, that result gets its own notification. The notification body and `tasty plugin logs <id>` show the cause.

`enable` and `disable` require an installed plugin ID. An ID that is not installed returns an error and leaves settings unchanged. Use `tasty plugin list` to find installed IDs.

`disable` starts shutdown, normally in the background. If the plugin has not exited after a 2-second grace period, Tasty attempts to force-stop it. Those 2 seconds are not a limit on the entire operation; waiting for the OS to finish the process can take longer. An `enable` during shutdown waits for the previous process to finish before starting a new one. `tasty plugin enable` then waits (up to 15 seconds) for the new plugin to connect and reports the result. Pass `--no-wait` to return right away. In that case, and when an agent calls IPC directly, the reply only says the start was accepted, so check the result with `tasty plugin show <id>`. If startup or connection fails, check the notification and the logs.

While closing a Surface of a stuck plugin takes time, other commands and input are still handled as usual. If you `disable` that plugin while the close is waiting, the close succeeds as soon as the process exits. If you quit Tasty in the meantime, quitting waits for that close to finish (up to 5 seconds).

```sh
tasty plugin list
tasty plugin permissions com.tasty.git-viewer
tasty plugin disable com.tasty.clipboard-viewer
tasty plugin logs com.tasty.markdown --follow
```

Example output of `tasty plugin permissions com.tasty.git-viewer`:

```json
{
  "granted": ["ui.tool_item", "fs.read", "ui.popup"],
  "id": "com.tasty.git-viewer",
  "manifest": ["ui.popup", "ui.tool_item", "fs.read"]
}
```

## Troubleshooting

| Symptom | What to check |
|------|-----------|
| Markdown · image · HTML files do not open inside the terminal | Whether that plugin is **Enabled** in the plugin window. When it is off, the Surface kind itself does not exist |
| **Clipboard Viewer** · **Git** are missing from the Tools menu | The two plugins are disabled or have landed in **Attention** |
| A plugin's status is red | The **Log** button or `tasty plugin logs <id>` |
| I want to bring back a bundled plugin I removed | `tasty plugin upgrade-builtins --restore-removed <id>` |
| A plugin landed in **Attention** after an update | Its required permissions changed. Read the list and **Re-approve** |

<a id="what-to-read-next"></a>

## Keep exploring

- [Managing and viewing files](../using/files.md) — How to use the Markdown · image · HTML Surfaces.
- [Working with Claude and Codex](../agents/claude-codex.md) — The Claude Code · Codex plugins.
- [Settings](../customize/settings.md) — Where the plugin settings pages are.
