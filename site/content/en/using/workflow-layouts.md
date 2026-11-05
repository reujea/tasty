<!-- source-hash: 87d534abcfbd -->
# Build and reuse a working layout

Keep implementation and tests side by side. Save a layout as a preset so you can use it again for your next task.

<!-- tasty-diagram: workflow-layout -->
```
Left pane: implementation | Right pane: tests
Switch each pane's tabs independently.
```

## 1. Divide your workspace

1. Open the project's Workspace and change to its directory in the terminal.
2. Press `Alt+E` to split the Pane horizontally. This uses the default Tasty bindings. If you changed them, check [keybindings](../customize/keybindings.md).
3. Work on the implementation on one side and run your project's test command on the other.

Each Pane has independent tabs, so you can leave test output visible while checking documentation on the other side. To switch several views together as one tab, use a [Surface split](panes-tabs-splits.md).

## 2. Keep documentation close

Right-click an empty part of the tab strip in the Pane where you want the document, choose **New Markdown...**, and open the project's README. Select the terminal tab in that Pane to return to your shell. The other Pane's test view stays in place.

You can also inspect images and HTML output with the viewers described in [Managing and viewing files](files.md). The layout keeps execution and results close together; it does not replace a file editor or every browser feature.

## 3. Save the layout as a preset

1. Right-click the Workspace in the sidebar.
2. Choose **Save as workspace preset**.
3. Open **Tools > Presets** and find the saved Workspace preset.
4. Choose **Edit** to check each area's kind, working directory, and startup command. Structure changes save automatically; area settings save only when you press **OK** ([Editing](panes-tabs-splits.md#editing)).

Startup commands run when new terminals open. Leave them empty where you do not need automatic execution. Adjust paths and commands before using a layout for another project.

**Browse…** beside a directory or file field opens the Tasty picker inside the preset editor. For directories, confirm a selected subfolder or the current folder; for files, choose a listed file. The result fills only that field. Press **OK** to save the settings. Esc in the picker closes only the picker, leaving your settings draft open.

## 4. Reuse it for a new task

Right-click **New Workspace** and choose **Create workspace from preset...**. Select your preset, then check that each terminal starts in the intended directory.

Presets reuse layout and startup settings. They do not clone running processes. [Restoring layouts and output](workspaces.md) when you reopen the app is a separate feature.

<a id="use-the-same-environment-on-every-os"></a>

## Using a layout on another OS

Workspaces, panes, and tabs follow the same layout model on Windows, macOS, and Linux. Set file paths, shell commands, and installed tools for each environment.

## Work alongside an agent

Connect [Claude Code or Codex CLI](../agents/claude-codex.md) on one side and continue your own work on the other. An agent creating a background tab does not change your view's focus, selection, or scroll position.

To run something without a tab and inspect it later, see [bringing a background task into a tab](../agents/background-tasks.md).
