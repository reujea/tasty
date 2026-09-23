<!-- source-hash: 2201271214e1 -->
# Workspaces

Give each project or task its own workspace. Keep its terminals and layout together, then use names and categories to find it easily. If the window structure is new to you, start with [A first look](../getting-started/first-look.md).

## Creating

| How | What it does |
|------|------|
| The **New Workspace** button at the bottom of the sidebar | Creates a Workspace with a single terminal |
| `Alt+N` (macOS `Cmd+N`) | Same |
| Right-click the **New Workspace** button > **Create workspace from preset...** | Creates one from a saved layout preset ([Saving layouts](panes-tabs-splits.md#saving-layouts--presets)) |
| Right-click an empty area of the sidebar > **Add remote workspace** | Mirrors tasty on another machine ([Working remotely](../remote/attach.md)) |

The terminal in a new Workspace starts in the working directory of the current terminal (**Settings** > **Terminal** > **Inherit working directory**, on by default).

To start with a non-terminal Surface or to set the name at creation, use the CLI.

```sh
tasty new workspace --name build --cwd ~/proj
tasty new workspace --type explorer --path ~/proj
tasty new workspace --type markdown --file README.md
```

## Name · subtitle

The default name is a sequence number such as `Workspace 1`. Right-click the sidebar card to change it.

- **Rename** — shortcut `F3`.
- **Change Subtitle** — the small text under the card. Shortcut `F4`.

```sh
tasty set workspace --id 3 --name api --subtitle "port 8080"
tasty list workspaces
```

## Switching

- Click a sidebar card.
- `Alt+1` to `Alt+9` (macOS `Cmd+1` and so on). Hold `Alt` and a number keycap appears next to each card showing which number it is.
- `Alt+J` / `Alt+K` — next / previous Workspace. Wraps around at either end.
- The **Jump** button in the notification panel — goes to the Workspace that sent the notification.

The modifier and number keys are changed in the **Workspace switch modifier** item under **Settings** > **Keybindings**.

Each Workspace remembers its own focused Pane. Switch away and back and the cursor is where you left it. Switching scrolls the sidebar list automatically so the active card is visible.

## Reordering

- Drag a card.
- Right-click a card > **Move Up** / **Move Down**.
- `tasty move workspace --id 5 --to 0` — names the Workspace by **ID**, so it moves in whichever window owns it even when several are open. `tasty list workspaces` shows the IDs.
- `tasty move workspace --from 2 --to 0` (counting from 0) — a position **inside one window**, so with several windows open it moves in the one you are looking at.

## Marks on a card

| Mark | Meaning |
|------|----|
| Dot left of the name | A program is running inside that Workspace (the count is shown alongside, as in `● 3`) |
| Number badge right of the name | Number of Surfaces that need attention. Yellow means waiting for input, blue means a job finished. Click that Surface and it disappears |
| **REMOTE** pill | A Workspace mirroring a remote tasty |
| Lavender border around the dot left of the name | Occupied remotely by another user |

When the sidebar is collapsed (**Collapse**, `Ctrl+B`) the cards become square icons and the marks shrink with them. The dot at the top right shows only the most urgent one, without a count — yellow is waiting for input, blue is a finished job, green is running. The **REMOTE** pill turns into a sky-blue chip at the bottom right. When another user holds the Workspace remotely, the lavender border wraps **the whole square icon** instead of the dot.

## Closing and restoring

- Right-click a card > **Close Workspace**, or `Alt+Shift+W`.
- `Ctrl+W` is **Close active** — if only one Tab is left it closes the Pane, and if only one Pane is left it closes the Workspace.

A workspace holding a terminal someone is using over a remote attach will not close; you
get a notice instead. Someone is working in that terminal right now, and closing it would
end their session without warning. To take it back, press **Force detach** on the occupancy
badge, then close again. If you would rather not switch to that Workspace, right-click its
sidebar card instead — a **Force detach** item appears only while it is occupied, and it
goes through a confirmation first.

Closed Workspaces · Panes · Tabs · Surfaces are restored with `Ctrl+Shift+T` (**Restore closed**), most recently closed first. A terminal's scrollback comes back with it. The shell is started fresh, though, so a program that was running does not come back. This list lives only in memory and is gone when Tasty exits.

## Grouping into categories (folders)

When you have many Workspaces, turn on **Settings** > **General** > **Workspace categories (folders)**. The sidebar changes into collapsible sections.

- The default section **Workspaces** is always at the top and cannot be renamed or deleted.
- Click a section header — collapse / expand. The **Collapse/expand all categories** shortcut is empty by default, so assign it yourself.
- Right-click a header — **Add workspace** · **Create workspace from preset...** · **Add remote workspace** · **Rename category** · **Delete category** · **New category**.
- Right-click an empty area — **New category** · **Add remote workspace**.
- Right-click a Workspace card — **Move to category**. Dragging it to another section also works.
- Deleting a category does not delete the Workspaces in it; they return to the **Workspaces** section.

With categories on, the **New Workspace** button under the sidebar disappears and you create Workspaces from the header menu. `Alt+N` creates one in the category the current Workspace belongs to.

Switching shortcuts also gain a category level.

| Shortcut | What it does |
|--------|------|
| `Ctrl+Shift+1` to `Ctrl+Shift+0` | Go to the nth category. Expands it if collapsed and goes to the Workspace last used in that category. Hold `Ctrl+Shift` and numbers appear on the headers |
| `Ctrl+Shift+J` / `Ctrl+Shift+K` | Next / previous category |
| `Alt+1` to `Alt+9` | The nth Workspace **within** the current category |
| `Alt+J` / `Alt+K` | Next / previous within the current category. To cross the boundary into the next category, turn on **Settings** > **General** > **Next/prev workspace crosses categories** |

Turning categories off again merges all Workspaces into the single **Workspaces** section.

```sh
tasty workspace-category list
tasty workspace-category create --name work
tasty new workspace --name api --category work
tasty set workspace --id 3 --category work
```

## Restoring after a restart

When you run Tasty again, the Workspace · Pane · Tab · Surface layout of the previous window comes back as it was. The related settings are under **Settings** > **General**.

- **Restore layout on startup** (on by default) — when off, the saved copy is deleted when the window closes.
- **Restore surface content on restart** (on by default) — also preserves each terminal's scrollback. Scroll up and the previous session's output is there. Turning it off deletes all stored scrollback files.

What is not restored — programs that were running, environment variables, popups that were open. The shell starts fresh in the same directory. Programs that leave a session-resume command, such as Claude Code, are run again automatically ([Working with Claude · Codex](../agents/claude-codex.md)).

The save file is `~/.tasty/layouts/01.json`. With several windows open, each window gets its own `02.json`, `03.json`, and the next time you open a new window it takes over the empty slots in order. Only one window appears on restart; the remaining slots are restored when you open a new window (`Alt+Shift+N`).

To reuse a layout that is not a restore target, save it as a preset — [Saving layouts](panes-tabs-splits.md#saving-layouts--presets).
