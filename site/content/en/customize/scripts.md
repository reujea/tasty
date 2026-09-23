<!-- source-hash: c9b51f4f0d28 -->
# Lua scripts

Turn repeated tasks into Lua scripts. Run them with a shortcut, or set them to run when a window, workspace, or tab opens or closes.

Register each script and connect it to a shortcut or event. Unregistered files do not run automatically at startup.

## Registering

Press **Add script** in **Settings** > **Misc** > **Scripts**.

- **File** — The path to a `.lua` file. You can also pick it with **Browse…**. You may put it anywhere, but the convention is `~/.tasty/scripts/`.
- **Display name** — The name shown in the list and in the keybinding screen. Leave it empty and the file name is used.

Registering adds one row to the list, showing the keybinding attached to it. If there is none yet it says **Unbound**. You can rename or remove the script from the row. Removing it also releases the keybinding attached to it.

## Running from a keybinding

Assign a combination for each registered script in **Settings** > **Keybindings** > **Run Scripts**. Press the combination you assigned and the script runs right there.

## Auto-running on an event

Pick an event with **Add trigger…** on the **Auto-run** line below the script's row. You can attach several, and each attached event shows as a chip that you click to detach.

| Event | When |
|---|---|
| `tasty.startup.post` | Right after Tasty has finished coming up |
| `window.create.post` · `window.delete.post` | When a window is created or closed |
| `workspace.create.post` · `workspace.delete.post` | When a Workspace is created or closed |
| `workspace.change.post` | When a Workspace name or description is changed directly in the window |
| `tab.create.post` · `tab.delete.post` | When a Tab is created or closed |
| `tab.change.post` | When a Tab name is changed directly in the window |
| `pane.create.post` · `pane.delete.post` | When a Pane is created or closed |
| `surface.create.post` · `surface.delete.post` | When a Surface is created or closed |

The two rename events fire **only when the change was made directly in the window**. A name changed through the CLI does not fire them.

An auto-run script that causes another event through the CLI can start a chain. To prevent repeated execution, Tasty briefly holds off new automatic runs while a script runs and just after it finishes.

## What a script can do

Use the following Tasty functions in your scripts.

| Function | What it does |
|---|---|
| `tasty.tree()` | Reads the window · Workspace · Tab · Surface structure as a table. It is a read-only copy |
| `tasty.run_cli(args)` | Runs a `tasty` command. Pass a single string or a table of strings |
| `tasty.log(msg)` · `tasty.warn(msg)` | Writes to the log |
| `tasty.on(event, cb)` | Registers a function to be called when an event fires. It can only observe; it cannot change what Tasty does |

Actually operating Tasty is mostly done with `tasty.run_cli`. Whether you create a Workspace or send a notification, anything you can do with the [CLI](../agents/cli.md) works here too.

```lua
-- When a new Workspace is created, attach a log window on the right.
local tree = tasty.tree()
tasty.log("workspaces: " .. tostring(#tree))
tasty.run_cli({ "notify", "New Workspace", "--title", "script" })
```

File I/O and running external commands from the Lua standard library are available as they are. Scripts run on your computer without a separate sandbox.

## When the file changes

The file's hash is recorded at registration time and compared against the current file on every run. Edit the script in an editor and a **changed** badge appears in the list.

- **Running from a keybinding** — A confirmation dialog appears. Approve it and the new contents run and the hash is updated.
- **Auto-run** — It is blocked and does not run. This is to stop a script that changed while nobody was looking from running. Check the badge in the list and run it by hand once to approve it, and it runs again.

Other files pulled in with `require` are not checked.

## Execution limits

- There is a memory ceiling, and going over it aborts just that run.
- A script that never finishes, such as an infinite loop, is aborted after a while. Only the script thread is aborted; Tasty keeps running.
- Only text source is accepted. Precompiled bytecode is rejected.
- The debug library and the arbitrary-code-loading functions are blocked.

## When something goes wrong

To see what is not running and why, raise the log level.

```sh
TASTY_LOG=tasty_lua=debug tasty
```

An auto-run that was blocked, or a chain that was suppressed, is also recorded here as a warning.

<a id="what-to-read-next"></a>

## Keep exploring

- [Keybindings](keybindings.md) — Assigning combinations and presets
- [Settings](settings.md) — The whole settings window and where the settings file lives
- [Driving Tasty from the CLI](../agents/cli.md) — The commands you can call with `tasty.run_cli`
