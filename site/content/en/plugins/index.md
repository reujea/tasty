<!-- source-hash: 3c10954c5e09 -->
# Plugins

Use plugins for tools such as Markdown and image viewers or AI agent integrations. Explore the bundled plugins, add new ones, and manage which tools run and what permissions they have.

## What a plugin is

Features such as the Markdown viewer, the image viewer and the Claude Code integration are provided not by the Tasty core but by **plugins**. A plugin runs as a separate process, declares what it adds (Surface kinds · tool menu items · `tasty` subcommands · settings pages · file handlers) and which **permissions** it needs, and Tasty accepts its requests only within the granted permissions.

- Plugins are installed in `~/.tasty/plugins/<id>/`, with logs in `~/.tasty/plugins-logs/<id>.log`.
- The bundled plugins are installed automatically on first launch. After that they can be disabled or removed exactly like plugins you installed yourself. A bundled plugin you removed is not reinstalled on the next launch.
- When a plugin is disabled, the Surface kinds · commands · menu items it added disappear with it. This takes effect right away when you disable or remove it while Tasty is running: opening a new Surface of such a kind fails with the reason that the plugin providing it is turned off. Surfaces you already opened stay as they are and pick up again when you enable the plugin.

## Bundled plugins

| Plugin | id | What it does | Where you use it |
|---------|-----|---------|---------|
| **Markdown Viewer** | `com.tasty.markdown` | A Markdown Surface that renders `.md` files. Re-reads the file automatically when it changes | Right-click the tab strip > **New Markdown...**, open a `.md` in the explorer, `tasty markdown reload` · `recent` |
| **Image** | `com.tasty.image` | An image viewer and simple paint tool. Steps to the next · previous image in the same folder and saves as PNG | **New Image**, opening an image file, `tasty image open` · `save` · `export` · `next` · `prev` · `paste` · `list` |
| **HTML Viewer** | `com.tasty.html` | A Surface that shows HTML files and URLs in an embedded webview | **New HTML...**, opening an `.html`, `tasty html open` |
| **Clipboard Viewer** | `com.tasty.clipboard-viewer` | A popup that shows what is currently on the clipboard, classified as text · file · image · HTML. Keeps no history | **Tools** > **Clipboard Viewer**, `Ctrl+Shift+H` |
| **Git Viewer** | `com.tasty.git-viewer` | A read-only popup showing the status · log · diff of the repository in the current directory. With several worktrees, pick one on the left | **Tools** > **Git**, keybinding assigned by you |
| **Claude Code** | `com.tasty.claude` | Multi-agent commands that launch Claude Code inside Tasty, spawn child instances, send them messages and get notified on completion | `tasty claude launch` · `spawn` · `tell` … — [Working with Claude · Codex](../agents/claude-codex.md) |
| **Codex** | `com.tasty.codex` | Does the same as above for the Codex CLI | `tasty codex launch` · `spawn` · `tell` … — same page |

How to use each Surface kind is in [Opening files](../using/files.md). There are also demo · experimental plugins that ship only in development builds; they are not included in the distribution.

### Settings added by plugins

In the **Settings** window, plugin pages appear in two places.

- **Appearance** > **Markdown** — The font override for the Markdown Surface only.
- **Appearance** > **HTML** — **Default zoom** (%) · **Color scheme** (follow theme / light / dark) · **Allow remote content** (off by default — blocks external http/https resources) · **Sandbox scripts** (on by default).
- **Plugins** > **Claude Code** — **Spawn child warning threshold** and so on.
- **Plugins** > **Codex** — **Spawn child warning threshold** · **Default approval policy** · **Default sandbox mode**.

The **Configure** button in the plugin window goes to these pages too.

## The plugin window

Press the **Plugins** button at the very bottom of the sidebar. It has three tabs.

### Installed

The **Installed** tab. Pick one from the list on the left and the details appear on the right. **Filter installed…** above the list filters by name.

- **Status** — **Enabled** / **Disabled**, **Running**. If a plugin is enabled but fails to run, a red marker appears with the notice **Failed to connect. Check the plugin's configuration in Settings.**
- The enable toggle — Turning it off cleans up the process; turning it on starts it again.
- **Permissions** — The list of permissions this plugin has been granted. Read-only here; it cannot be changed.
- **Commands** — Keybinding commands added by the plugin. Change the keys under **Settings** > **Keybindings** > **Plugins**.
- **Log** · **Install path** · **Open folder**.
- **Configure** — Goes to the plugin's page in the settings window.
- **Uninstall** — Deletes the install folder. For a plugin with the **built-in** badge, one more warning appears saying it will not be installed automatically on the next launch either.

### Attention

The **Attention** tab. Plugins whose registration was rejected or that failed to run are collected here with the reason.

| Shown | Meaning | What to do |
|------|----|------|
| **Signature not trusted** | Signed with a key not in the trust list | Confirm the source, compare with **Copy fingerprint**, and if you trust it, **Re-approve** |
| **Signature invalid** | No signature, or verification failed | Get a correct package from the distributor |
| **Permissions changed** | An update changed the required permissions | Read the **newly requested** list and **Re-approve** |
| **Runtime error** | Enabled but failed while running | Check the **Log** |

### Add plugin

The **Add plugin** tab.

1. In **Plugin folder path**, enter a folder containing `tasty-plugin.toml`, or pick one with **Find plugin folder…**.
2. Press **Verify** and **Plugin information** previews the name · version · description and the **required permissions**.
3. Press **Add**.
4. If the plugin is not signed with a verified key, the **Unknown source plugin** confirmation appears. If you check the fingerprint and proceed, that key is recorded in the trust list and you are not asked again. A plugin without its signing key file (`tasty-plugin.toml.pub`) cannot be registered, so ask the distributor for it.

Installing grants the permissions the plugin requests. Review the permission list in the preview before adding it.

## Permissions

A plugin declares the permissions it needs in advance, and Tasty rejects any request without a granted permission. Common names and their meanings:

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

Permissions granted to the bundled plugins:

| Plugin | Permissions |
|---------|------|
| Markdown Viewer | `surface.read` `surface.write` `fs.read` `file_handler.define` `file_handler.handle:markdown` `ui.settings_page` `ui.popup` |
| Image | `surface.read` `surface.write` `clipboard.read` `fs.read` `fs.write` `file_handler.define` `file_handler.handle:image` |
| HTML Viewer | `surface.read` `surface.write` `file_handler.define` `file_handler.handle:html` `ui.settings_page` |
| Clipboard Viewer | `clipboard.read` `ui.popup` `ui.tool_item` |
| Git Viewer | `ui.popup` `ui.tool_item` `fs.read` |
| Claude Code | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.read` `fs.write` `notification` `telemetry` `agent` `ui.settings_page` `completion_strategy.define` `memory.read` `ipc.invoke:codex` |
| Codex | `surface.read` `surface.write` `terminal.spawn` `terminal.write` `terminal.read` `fs.write` `notification` `ui.settings_page` `completion_strategy.define` |

Removing or restoring individual permissions is done only from the CLI (below).

## The `tasty plugin` command

Use it from a terminal while Tasty is running. The output is JSON.

| Command | What it does |
|------|---------|
| `tasty plugin list` | The id · version · enabled · running state of installed plugins |
| `tasty plugin show <id>` | The full manifest · permissions · commands · running state |
| `tasty plugin install <folder>` | Installs from a folder containing `tasty-plugin.toml`. Grants the manifest permissions as they are |
| `tasty plugin remove <id>` | Removes it |
| `tasty plugin enable <id>` · `disable <id>` | Enables · disables it |
| `tasty plugin logs <id> [--follow]` | Prints the log. `--follow` keeps showing new lines (stop with `Ctrl+C`) |
| `tasty plugin permissions <id>` | The permissions the manifest requires and those actually granted |
| `tasty plugin grant <id> <permission>` · `revoke <id> <permission>` | Grants · revokes one permission. Only permissions declared in the manifest can be granted |
| `tasty plugin doctor <id>` | Diagnoses the manifest — whether it has rules this version of Tasty does not understand |
| `tasty plugin upgrade-builtins [--force] [--restore-removed <id>]` | Realigns the bundled plugins with the bundled version. `--restore-removed` brings back a bundled plugin you removed |

`enable` and `disable` require an installed plugin ID. An ID that is not installed returns an error and leaves settings unchanged. Use `tasty plugin list` to find installed IDs.

`disable` returns without waiting for the plugin to exit. A plugin that does not respond is force-stopped in the background after at most 2 seconds. If you `enable` it in the meantime, `enable` waits for the previous process to exit and then starts the plugin right away, so it can take up to 2 seconds. When `enable` returns, the plugin is already running and ready to use.

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

- [Opening files](../using/files.md) — How to use the Markdown · image · HTML Surfaces.
- [Working with Claude · Codex](../agents/claude-codex.md) — The Claude Code · Codex plugins.
- [Settings](../customize/settings.md) — Where the plugin settings pages are.
