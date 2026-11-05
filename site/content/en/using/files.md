<!-- source-hash: a6ba67217392 -->
# Managing and viewing files

Find and manage files in the Explorer, and view Markdown, images, and HTML beside the terminal. Inspect changes in the Git viewer. The viewers and Git tools require their respective plugins to be enabled.

## Ways to open

| Where | How |
|--------|--------|
| Sidebar **Tools** > **Open File…** | Tasty's own file chooser. It opens in the folder of the terminal or Explorer you are looking at, and in a remote Workspace it shows remote files; a remote file you pick opens in that remote Workspace |
| Drag a file onto the window | Drop when **Drop to open** appears. Dropped on an Explorer cell, the file is copied into that folder instead of opened ([Drag and drop](#drag-and-drop)) |
| Explorer Surface | Double-click a file |
| Terminal | `Ctrl+click`, or select a path and right-click > **Open File** ([Working with the terminal](terminal.md#opening-links-and-paths)) |
| Right-click an empty area of the tab strip | **New Markdown...** · **New Explorer** · **New HTML...** · **New Image** |
| `Alt+'` | Changes the current Surface to another kind ([Changing the kind](panes-tabs-splits.md#changing-the-kind)) |
| CLI | `tasty new tab --pane 1 --type markdown --file README.md` and so on |

The file extension decides which Surface opens it — `.md` is Markdown, image files are image, `.html` and `.svg` are HTML, and a folder is Explorer. When nothing settles which one opens it, the **Open file with…** window appears so you can choose, and your choice is kept under **Recent**. Extension mappings and handlers are changed in the **Settings** > **Handler** tab.

File type and handler settings take effect when you press **Save** and are saved to the file in the background. If saving fails, a warning and notification explain why, and the changes last only until Tasty exits. An externally edited or damaged original is kept; check the file and restart Tasty before changing those settings again. If these settings cannot be loaded on startup, the log names the file and the type of problem without including the file contents.

The **Open file with…** window lists the file path, detected format, and available handlers. Each handler has an icon, a name, and its origin (**built-in** · **you** · **plugin**).

**Recent** shows previously used handlers and when you used them: just now, minutes ago, hours ago, yesterday, or days ago. After a week, it shows a date such as `2026-09-13`. The display updates if the window stays open across midnight.

One click selects a handler; a double-click opens the file. This choice applies **only this time**. It does not register a default for the file type, so the chooser can appear again for the same file. Set a default under **Settings** > **Handler**. Press `Esc` or **Cancel** to close the chooser.

A Surface opened from a file is split · moved · closed · restored on restart just like any other Surface. The Tab name becomes the file name.

Drag the file chooser by its top row, the one with the title and ✕. Closing it with ✕, **Cancel**, or `Esc` opens nothing.

In the file chooser, the list has a row of column names for name, size and modification date, and the selected row carries a coloured bar on its left edge. Long file names end with `…` so they do not overlap the size and modification date. This only changes the display; selecting or opening an entry still uses its full name.

When the chooser was opened for certain file types only (for example, importing key bindings shows only `.toml`), a box to the right of the name field lists those extensions, such as `*.toml`. A long list ends with `…`; hover over it to see the whole list. The box only tells you what is shown — you cannot click it to change the filter. A chooser opened without a file type has no such box.

In the list, a single click selects the row, file or folder; a double click enters a folder or opens a file. Pressing **Open** with a folder selected enters that folder — the way down by keyboard. When choosing where to save, a folder cannot be the target, so selecting one leaves the name field as it is and shows a grey line below saying so. When saving, double-clicking a file only fills in its name and does not save — this leaves you the chance to read the overwrite warning.

A file you open yourself, by any of the ways above, opens in a new tab and **switches to it.** When an AI agent opens one for you, the tab is added and the tab you were on stays selected, so the screen you are working on does not change under you.

When an opening request specifies its source surface, switching windows while it is pending keeps the result in a new tab in that surface’s pane. Opening a file from the Explorer while you are looking at another pane still puts it in the Explorer’s pane. Closing the source does not open the file in another window.

If a file selection is already in progress, another request is declined with a notice. Finish or cancel the existing selection before opening another chooser.

## Explorer

The file manager built into Tasty. Change a terminal with `Alt+'` > **Explorer** and it opens that terminal's current directory as the root. `tasty new tab --pane 1 --type explorer --path ~/proj` also opens one. Passing `--cwd ~/proj` instead of `--path` opens that folder as the root too.

### Screen

- **Left** — the **Files** tree (fixed at the root) and **Favorites** under it.
- **Top** — **Back** · **Forward** · **Up** · **Refresh**, the address bar, the **New folder** · **New file** buttons, the **Find**, **Preview panel** and hidden files (eye) toggles, and the view switch (**Grid** · **List** · **Detail**). When the Explorer cell is narrower than 440px, the command buttons fold into one **More** (`…`) button that shows the same commands as a menu. In that menu the find and preview rows say what they do: **Find** ↔ **Close find**, **Show preview** ↔ **Hide preview**, **Show hidden files** ↔ **Hide hidden files**. The hidden files toggle looks pressed while hidden files are shown, and its tooltip names the action with its shortcut, such as **Show hidden files (Ctrl+Shift+.)**.
- **Right** — the items in the current folder. In Grid view, local image files (1 MiB or smaller) show a small thumbnail instead of an icon. `..` at the top goes to the parent folder. In Detail view, click the **Name** · **Size** · **Date modified** · **Type** column headers to sort. The Type column names the kind in words, such as "Markdown", "PDF document", "Archive" or "PNG image" ("RS file" for anything else), and a link says what it points to, such as "Link to Markdown". Properties and the preview header use the same words, and sorting by Type keeps each kind together, so archives such as .gz and .zip sit side by side.
- You can keep several **New tab**s inside a Surface and view folders separately. These are separate from the Pane's Tabs.

When you return to an internal tab, it keeps your selected items, list and Files tree scroll positions, and the Find query and subfolder search results. Two tabs showing the same folder remember these separately. Reads and searches that finish while you are in another tab update only the tab that started them. Closing a tab discards this state. After restarting the app, folders, views and sorting are restored, while selection and Find start empty.

A remote Explorer also keeps its internal tabs and browsing locations when the Workspace updates on the same connection and the server folder is unchanged. If the server changes that folder or a new connection is established, Explorer starts with a fresh tab at the folder reported by the server.

When you save a layout with an Explorer as a preset and apply it, each tab inside the Explorer opens the folder it was showing, with the same view and sort, and the Pane's tab is titled after the folder the active tab was opened on (its root). If you set a working directory for it in the preset editor, it opens that one folder instead of the saved tabs. If a saved folder no longer exists, that tab shows the read error message.

Dragging a split line to shrink an Explorer stops at the height that still leaves the toolbar, the status line and about two rows of the list (180px). Splitting an Explorer, or splitting to make a new one, moves the split line so the Explorer keeps 180px. A cell of another kind takes the height that is left, but a split never makes it thinner than a tab strip and one row (56px). When there is no room for two Explorers to keep 180px, or for the other cell to keep 56px, nothing is split: the split menu and shortcut briefly show "Not enough room to split this pane", and `tasty split` reports the reason as an error. When the cell is so low that the left sidebar is shorter than 240px, Favorites hide and only the Files tree shows. Resizing the window can take the cell below that height. When the content area is lower than 120px, the empty folder, permission denied and read error messages turn into a single line of icon, title and buttons, and hovering the title shows the details.

If a folder can't be read (the path is gone, it is not a folder, or a remote read failed), the list area shows **Can't read this folder** with the reason the operating system gave. **Retry** reads the same folder again and **Go up** goes one folder up. If the folders above were removed too, it goes to the nearest folder above that still exists. A missing permission shows **Permission denied** instead.

When you paste, rename or move items to the trash in an Explorer, every other Explorer showing a changed folder (even in another window) re-reads its list and left tree. If the folder you were viewing was deleted or renamed, the read error message appears in place and the Explorer does not move to another folder on its own. When another program creates, deletes or renames items in a folder, the Explorer in the focused tab of the Tasty window you are using notices within 2 seconds and re-reads its list and the folders you expanded in the tree, keeping your selection and scroll position. An Explorer in another tab, or one left open while you use another app, checks the moment you return to that tab or to the Tasty window. Changes to a file's content, size or permissions alone, and remote (attach) Explorers, are not picked up automatically, so press **Refresh** (`F5`). To have size and modified-time changes of files in the open folder picked up automatically too, turn on **Settings** > **General** > **Explorer: refresh file details**. The size and date columns, the preview and thumbnails then update within a few seconds, but every check reads all items in that folder, so leave it off on slow drives or in very large folders. It is off by default, and files inside expanded tree folders are not checked even when it is on. If you expand a great many tree folders, they are not all checked at once but in turns over a few checks, so changes deep in the tree can show up a little later. Refresh also re-reads the folders you expanded in the tree. While a local Explorer is showing a folder list, it does not empty the list while it reads, so your selection and scroll position stay. In a remote (attach) Explorer, from **Retry** on an error message, or while a folder is still loading, it empties the list and reads it again from the start. When a re-read list no longer has an item you selected, that item leaves the selection too.

Click the address bar to type a path directly; recently visited folders appear as autocompletion. Click a suggestion, or choose one with the arrow keys and press `Enter`, to go to that folder. Go with `Enter` or **Go**. The left tree stays fixed at the root, but the right list can go anywhere. If you type the path of a file, the file is not opened: Explorer goes to the folder that holds it and selects it. A remote Explorer does the same.

Names whose accents or Hangul letters were stored as separate characters are composed for display. When you start editing an address that was displayed this way, the whole original path is selected. Copy and navigation still use the exact original name, even when two files look the same.

- A relative path starts from the folder you are looking at. `..` goes one level up.
- `~` and `~/folder` are under your home folder.
- On Windows, a drive alone such as `C:` goes to the top of that drive, and network paths such as `\\server\share` work too.
- If the path does not exist, the Explorer stays where it is and a notice tells you why. A link whose target is gone is treated like a file: the Explorer goes to the folder that holds the link and selects it.
- In an Explorer in a remote Workspace, the path is looked up on the remote computer. A folder that exists only there opens, and if the remote computer can't read it, the reason shows in the list area. `~` is the remote computer's home folder, and `.` and `..` are resolved on the remote path. If Tasty hasn't learned the remote home yet right after connecting, it asks you to type the full remote path instead of `~`. While Tasty checks whether a remote path is a file, the address bar keeps the path you typed, and if the check takes a while a spinner appears at its end. Press `Esc` then to stop the check and put the current folder back in the address bar.

On Linux X11, pressing a list navigation, open, clear selection, rename, or trash key during IME composition handles the composition first. The same press does not move or open files. Pressing the key again after composition ends works normally. In the Find field, the Esc that ends composition also leaves the query intact. The next Esc works normally. In the popup for renaming an Explorer entry, the `Enter` that ends composition commits the text and keeps the popup open. Press `Enter` again or click **Save** to rename the entry. Other Find keys and the address bar keep their own input handling.

On native Linux Wayland, a key that ends Hangul composition can also trigger a list action or clear the Find query. Confirm a rename from the name field with `Enter` or **Save**, or cancel with `Esc`. This works from the first window without switching to another window and back.

### New folder · New file

Start from **New folder** · **New file** on the toolbar, the right-click menu on empty space, or the right-click menu on a folder (which creates inside that folder). A name row opens at the top of the list. Started from a folder's menu, it opens right under that folder instead (in Grid, in the cell right after it), indented, with **in {folder}** after the field and the folder highlighted; the list you are looking at stays where it is. The name row starts with "New folder" fully selected, or "untitled.txt" selected up to the extension. If the name is taken, it starts with a numbered name such as "New folder 2".

- `Enter` creates it and `Esc` cancels. Nothing is written to disk when you cancel.
- Clicking outside the row creates it when the name is valid and cancels when it is empty or invalid.
- An empty name (including one with only spaces), a character that can't be used (`/`, and on Windows also `\ : * ? " < > |`), a name that starts or ends with a space, a name Windows reserves (`CON`, `NUL`, …), or on Windows a name ending in a dot shows the reason under the field right away. A name that already exists is reported when you press `Enter` (on macOS and Windows, a name that differs only in letter case counts as existing); the row stays open with your text. Nothing is overwritten.
- The new item moves to its sorted place and is selected. If you moved to another folder or selected something else in the meantime, the selection does not change.
- In a folder you can't write to, both buttons are dimmed and hovering shows **Can't write to this folder**. Creating inside such a folder from its right-click menu shows the same notice instead of opening the name row (on Windows, the error appears when the item is created). They work in Explorers in remote workspaces too. If the remote computer runs an older Tasty, these buttons and menu items don't appear.

With the Explorer focused, `F7` makes a new folder and `Shift+F4` a new file. On macOS the F keys are media keys by default, so hold `Fn` with them. Change the keys in Settings > Keybindings > **Explorer**.

### Find · search subfolders

The **Find** toggle on the toolbar (the **More** menu in a narrow cell), or `Ctrl+F` while the Explorer has focus, opens a find bar under the toolbar. `Ctrl+F` is the same shortcut as terminal search; in the Explorer it opens this bar.

- As you type, only the items in the current folder whose names contain the text stay. Case is ignored and the matching part is highlighted. The right end shows how many are shown out of the total, such as "2 of 6". When no name matches, the list area says "No names match “…”". The `..` row that goes up a folder stays while you filter.
- Items that are filtered out leave the selection. Select all picks only the items shown.
- Turn on **Subfolders** to search everything under the current folder. Results fill in as they are found, and **Stop** halts the search while keeping what was found. The Detail view shows a **Folder** column with each result's location relative to the current folder instead of the type; in the List and Grid views, hover an item to see it. Selecting a result also shows its location on the status line. Folders that can't be read are skipped and reported as, for example, "1 folder skipped"; hover it to see which. A search stops once it reaches 5,000 results and shows "5,000+ found · stopped". The results stay; type more to narrow the search. Folder links are not followed. When you rename, move or delete a result, from this Explorer or another, the search runs again so the results stay current.
- In the find field, `Esc` once clears the text and once more closes the bar. ×, or moving to another folder (Back, Up, the address bar, opening a folder), closes it too.
- While you type in the find field, type to find and the Explorer shortcuts are off.
- Enter `ext:rs` in the same field to search by extension. `ext:rs ext:md` finds rs or md files; `note ext:rs` also requires note in the name. Folders are excluded, and only the last extension counts. Case is ignored, and `EXT:.RS` works too. Use letters, digits, underscores or hyphens for an extension. Empty `ext:` and malformed forms such as `ext:*` or `ext:rs,md` are searched as literal name text; there is no wildcard or quoting syntax. With valid extension conditions, the remaining name words are joined with single spaces and matched as one substring. Only the name condition is highlighted.
- While viewing search results, New folder, New file and ordinary Paste from the toolbar or empty-area menu use the folder where the search started. Matching new items reappear in the results. A folder-specific menu or **Paste into** still uses the folder you explicitly chose.
- Explorers in remote workspaces filter the current folder only; they don't search subfolders.

### Preview panel

Press the **Preview panel** button in the toolbar to open a panel to the right of the list that shows the one selected file. Text files show their first 64 KB in a monospace font; images are fitted to the panel without being enlarged past their own size. Other file types and folders show **No preview for this file type**, and files over 1 MiB, as well as images too big to decode (such as more than 16384 pixels on a side), show **Too large to preview**. An image over the pixel limit also shows its actual size. With several files selected, the panel says how many and asks you to pick one. Drag the panel's left edge to set its width between 200 and 460 px. Each Explorer remembers whether the panel is on and its width, across restarts and in presets you save. When the Explorer cell is too narrow to keep the list beside it, the panel hides while the button stays on. When less than 120 px remains below the header, only the file name, kind and size stay visible; the body and the line below the header are hidden. Explorers in remote workspaces have no preview. The outline around the keyboard’s current item stays inside the file list, clear of the preview panel.

### Hidden files

Files and folders whose names start with a dot (`.`), along with items marked hidden on Windows (the Hidden attribute) or on macOS (`chflags hidden`), are left out of the list at first, and the status line below says how many, such as **2 hidden**. Press `Ctrl+Shift+.` (`Cmd+Shift+.` on macOS) or choose **Show hidden files** in the **More** menu to show them; hidden items then show their name and icon in a dimmer text color. Do the same to hide them again. Each Explorer remembers the setting across restarts and in presets you save. The folder tree on the left follows the same setting. Subfolder searches follow this setting too. With hidden items off, hidden folders are not entered. Changing the setting restarts the same search. An Explorer in a remote workspace also follows the remote computer's hidden attribute or flag. If the remote computer runs an older Tasty, only the leading dot counts.

### Properties

Choose **Properties** at the bottom of the right-click menu to open a popup inside the Explorer cell. Files and folders show kind, size, modified and created dates, path and permissions. **Path** is the item's own absolute path; use the button beside it to copy it. For a folder, a spinner turns while the items and size are counted in the background. If part of the count cannot be read, **Some folders couldn't be read** appears below the size. Closing the popup stops the count. Times are in this computer's time zone; choose Minutes, Seconds or Seconds and UTC offset under **Time format in Properties** in **Settings** > **General**.

When the popup shows one item, the **Open** button at the bottom opens it the way Enter does in the list: a file follows your handler settings and a folder opens in that Explorer. The popup stays open. For a broken link the button is greyed out.

For one file or folder on this computer, **Edit** next to **Permissions** changes its permissions. On macOS and Linux, tick read, write and execute for the owner, group and others; on Windows only the **Read-only** attribute changes. Nothing changes until you press **Apply**, which changes that item only, not the items inside a folder, and keeps special bits such as setuid, setgid and sticky. If the change fails, the box shows why and the real permissions are read again. In a narrow Explorer cell the box never gets cut off: the role names move to their own line above the checkboxes, and when it is narrower still, read, write and execute become labelled checkboxes, one per line. If you don't own the item (and Tasty isn't running as an administrator), a note says only the owner can change permissions instead of the Edit button. Links, several items and remote items can't have their permissions changed here.

A link shows its target, the target's kind, size and modified date, and the link's own path. The target also has a copy button. A missing target is shown as **Broken link**, with **Target not found** below the target. A note below the fields says that deleting or renaming the link doesn't change its target. Other read errors, such as permission errors, show the reason and a **Retry** button.

Several selected items show the count, kinds, total size and common location, and right-clicking an empty area shows the current folder. If some of the chosen items cannot be read, only the readable ones are counted and **Some items couldn't be read** appears below the kinds; if none can be read, the Retry screen above appears. Choosing a folder together with items inside it counts those items once, and hard links to the same file add its size once. In a remote Explorer only the kind, size, modified date and path from the list are shown; the remote list has no folder sizes, so a total that includes a folder shows —.

Every bound command in the context menu shows its first shortcut on the right, including **Undo** and **Redo**. Changing or clearing the bindings in Settings updates the menu too.

### Shortcuts (when the Explorer has focus)

| Action | Shortcut |
|---------|--------|
| Refresh | `F5` |
| Go to parent folder | `Alt+↑` (macOS `Cmd+↑`) |
| Back / forward | `Alt+←` / `Alt+→` (macOS `Cmd+←` / `Cmd+→`) |
| Go to the address bar (selects the whole path) | `Alt+L` (macOS `Cmd+L`) |
| Select all | `Ctrl+A` · `Alt+A` (macOS `Cmd+A`) |
| Copy path | `Alt+Shift+C` (macOS `Cmd+Shift+C`) |
| Find | `Ctrl+F` · `Alt+F` |
| Show or hide hidden files | `Ctrl+Shift+.` · `Alt+Shift+.` (macOS `Cmd+Shift+.`) |
| Copy / cut / paste | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` (the same bindings as the terminal) |
| Move between items | Arrow keys · `Home` · `End` · `PageUp` · `PageDown` (left and right only in Grid view) |
| Extend the selection | The same keys with `Shift` |
| Open (opens every file when only files are selected, enters the folder when one folder is among them, does nothing with two or more folders) | `Enter` |
| Clear the selection | `Esc` |
| New folder / new file | `F7` / `Shift+F4` (on macOS hold `Fn` with them by default) |
| Rename (one item selected) | `F2` |
| Move to Trash | `Delete` · `Alt+Backspace` (`Cmd+Backspace` on macOS) |
| Undo / redo | `Ctrl+Z` · `Alt+Z` / `Ctrl+Shift+Z` · `Ctrl+Y` · `Alt+Shift+Z` (macOS `Cmd+Z` / `Cmd+Shift+Z`) |
| Toggle the preview panel · Properties | No default — set them in **Settings** > **Keybindings** > **Explorer** |

Click to select, `Ctrl+click` to add, `Shift+click` to select a range. When you move with the keys, the current item gets an outline; clicking the list hides it until the next key. Drag from an empty part of the list to select every item the rectangle touches, and hold `Ctrl` or `Shift` as you start to add to the current selection. While dragging, move near the top or bottom edge of the list to scroll that way and keep selecting. It speeds up as you near the edge, and past the list it keeps scrolling at full speed (20 rows a second). `Shift+click` selects from the last item you clicked to the one you click, and `Ctrl+Shift+click` adds that range to the current selection. On macOS use `Cmd` instead of `Ctrl`.

### Type to find

With the list focused, type the first letters of a file name and the first item starting with those letters is selected and scrolled into view. This works the same in Grid, List and Detail view. Pressing the same letter again steps through the candidates, wrapping to the first after the last one. Typing different letters in a row turns it into a prefix search, such as `co` then `con`. Case is ignored, and pausing for about a second starts a new search. If no name matches, the selection stays where it was.

While you are editing the address bar or a popup is open, the letters go there instead. If you have several Explorers open, only the focused one responds.

If you have bound a shortcut to a single letter with no modifier, that letter acts only as the shortcut and is not used here. The default settings have no such shortcut.

### Right-click menu

Renaming refuses to overwrite an existing entry and reports the failure. Copying preserves symbolic links as links, keeps each file's permissions and modified time, and keeps each folder's permissions, including read-only folders. Owners, extended attributes and ACLs are not copied. You cannot paste a folder into itself or one of its descendants through another path. Failed file operations display an error notification.

A link to a folder opens like a folder, and the address bar keeps the link's path. Renaming a link or moving it to the trash affects only the link, not the folder it points to. Opening a link whose target is gone shows a notice that the target is missing. In the list, a link has a small link icon after its name, and a link whose target is gone shows a link icon in the warning colour in place of its icon; hover over it to see the missing target path. A remote Explorer shows links the same way and shows the same notice, but hovering doesn't show the target path. If the remote Tasty is a version that doesn't send link states, links aren't told apart and look like ordinary items.

On native Linux Wayland, right-click menus and the toolbar **More** menu are not supported. Use the visible toolbar buttons and Explorer shortcuts. For actions without a default shortcut, such as **Properties**, assign a key under **Settings** > **Keybindings** > **Explorer**.

- **Open** — first when you right-click one subfolder search result. Like a double-click, it goes into a folder and opens a file. It doesn't appear in a remote Explorer.
- **Show in enclosing folder** — first when you right-click one link, and right after **Open** for one subfolder search result. Opens the folder that really holds the item (for a link, the folder that holds its target) under the same path you were browsing, selects it and scrolls it into view. It doesn't appear for a link whose target is gone or in a remote Explorer.
- **New folder** · **New file** — first in the empty-space menu, and in the file group of a folder's menu. See "New folder · New file" above.
- **Copy Path** — with several selected, the item reads **Copy Paths** and joins the paths with line breaks.
- **Undo** · **Redo** — in the empty-space menu, named after the job they act on, such as "Undo moving 3 items" or "Undo rename". When there is no job to undo or redo, both rows stay greyed out before **Properties**. Each Explorer remembers its last 10 jobs (copies, moves, renames, new folders and new files). A copy or move that only partly went through is remembered for the items that made it. Undoing a job lets you redo it, and a new job clears what could be redone. Redo runs the same job again: a copy or move asks again if a name is taken, and a rename or new item fails instead of overwriting when the name is taken. Undoing a rename restores the old name (unless something else now has that name or the item was edited since), and undoing a new folder or file sends it to the trash (unless you put something in it or edited it since). When the latest step can no longer be undone (for example its copies were deleted, or a moved file was edited after the move), the row is greyed out and hovering it says why (on Windows, the same reason appears as the row label). Redo is greyed out the same way when something at the original place was removed or changed after the undo, so a redo never moves a file you did not undo. While a file job from that Explorer is still waiting or running, both rows are greyed out and the shortcuts only show a notice, so an undo never skips past the job you just started. Moving to the trash is not in this history; restore from the trash instead.
- **Copy** · **Cut** · **Paste** · **Paste (into)** — if the name already exists, you are asked what to do ([Progress and results](#progress-and-results)). Every Tasty window shares the copied or cut list, so you can paste it in another window. The list also goes on the OS clipboard, so you can paste it into another file manager, and files you copy or cut in a file manager paste into the explorer. When the two differ, whichever you copied last is pasted, and if you copied text in another app after that, the explorer has nothing to paste. A cut is removed from the clipboard once it has been moved. Files copied in a remote explorer are not put on the OS clipboard, and a clipboard holding items that are not files on this computer (such as paths on another machine or web addresses) is not pasted. If another app holds the clipboard but does not respond, the explorer treats that as the later copy, pastes nothing, and tells you so. Files copied in the macOS Finder always paste as a copy. Linux uses the X11 clipboard, through XWayland on Wayland. If the system clipboard cannot be updated, Tasty tells you that the files are stored only inside this Tasty instance. Pasting that internal list also tells you when the system clipboard could not be checked. If there is no usable internal list, Paste explains that clipboard access is unavailable. Opening the menu alone shows no notice. Native Wayland clipboard access is not supported.
- **Move to Trash** — sends to the OS trash without confirmation. To undo, use the trash. If that drive has no trash, nothing is deleted and you are told so.
- **Rename**. The new name follows the same rules as a new folder or file (empty, characters that can't be used, leading or trailing spaces, reserved names); if it breaks one, the reason appears under the field and it isn't saved.
- **Open in system** — opens chosen files in their default app and folders in the OS file manager. With several items chosen, it opens each one.
- **Show in Finder** (macOS) · **Show in File Explorer** (Windows) · **Show in File Manager** (Linux) — with one item chosen, opens the folder that holds it with the item selected. On Linux, if the file manager can't select items, only the folder opens.
- **Open in New Tab** — opens one more Explorer Tab in the Pane with that folder as the root.
- **Set as Root** — moves the root of the left tree.
- **Add to Favorites** — appears when you right-click a single folder or an empty area. It gives the folder a name and puts it in the list at the bottom left. Favorites are shared by all Explorers and saved in `~/.tasty/explorer-favorites.toml`. Favorites added or removed in another window show up right away. File saving runs in the background so you can keep working. If saving fails, a warning appears and the list stays available for this session. Changes made to the favorites file outside Tasty are preserved; check the file and restart Tasty to load them. Right-clicking a favorite at the bottom left offers **Open in New Tab** · **Set as Root** · **Remove from Favorites** (a remote Explorer leaves out Open in New Tab). Removing a favorite leaves the folder itself alone.
- **Properties** — opens the [Properties](#properties) popup above. It also appears in remote Explorers.

The last chosen view mode is remembered and applied to new Explorers too. The Explorer font is set separately in the **Explorer** item under **Settings** > **Appearance**. An Explorer in a remote Workspace can also create folders and files, rename, move to the trash, copy, cut, paste and drag. The remote computer's Tasty does the work, and the progress line, name conflict question and result card look the same as for local files. The trash is the remote computer's trash. Remote operations don't go into the **Undo** list, so use the result card's **Undo**. The progress line and its waiting list start with the remote computer's name, because each remote connection runs one operation at a time. In a narrow cell, the name and byte count shorten to leave the queued tag and cancel button clear; hover the host glyph to read the full name even when its text is hidden, or the shortened byte count to read its full value. The waiting list fits inside the cell too. In very narrow cells, the queue hides operation icons to make room for the cancel and remove buttons. If a complete button cannot fit, it is hidden too; widen the cell to use it. If the remote computer runs an older Tasty, the items and buttons that change files are greyed out, and their tooltip (or a notice, when you use a shortcut) says to update Tasty on that computer. If the connection drops during an operation, the remote computer stops it, but a notice says Tasty can't tell how far it got, so check the folder after you reconnect. Operations still waiting for that connection don't run, and a notice says how many. Open in system, Show in file manager and Open in New Tab don't appear in a remote Explorer. **Add to Favorites** is shown in the menu, but choosing it only shows a notice that it isn't supported yet, and nothing is added.

You can't paste or drag files between your computer and a remote one, or between two remote computers: Paste does not appear in the menu, and the paste shortcut only shows a notice. Tasty never copies a file with the same path on the other computer instead. Double-clicking a markdown file opens a tab for it on the remote computer; if the type's default handler can't be used remotely, only the handlers that can are offered, and a type with no such handler only shows a notice.

### Drag and drop

Drag items onto a folder row or cell, a folder in the left tree, or a favorite to put them in that folder. Dropped on a file or on empty space, they go to the folder the Explorer is showing. You can drag into another Explorer next to it too. While dragging an item, move toward the top or bottom edge of the list or the left tree to scroll automatically. Scrolling accelerates toward the edge and stays at 20 rows per second outside the area. With Explorers stacked vertically, only the area under the pointer scrolls; outside all areas, the nearest one scrolls.

- On the same disk the drop **moves**; across disks it **copies**. Hold `Ctrl` (macOS `Option`) while dropping to do the opposite. Change that key with **Explorer drag flip modifier** at the bottom of **Settings** > **Keybindings** > **General** (the same value as `explorer_drag_flip_modifier` under `[keybindings]` in `config.toml`).
- While dragging, the label by the pointer says **Move to** or **Copy to** and the folder. Where you can't drop, it says **Can't drop** and why (into itself, already in this folder, different computer, this remote can't change files, no write access). A folder reached through a symbolic link counts as the folder the link points to.
- Inside a remote Explorer, a drag **moves** only between folders the remote computer reports as the same disk, and **copies** otherwise.
- Hover over a closed tree folder for 800 ms to expand it without moving the pointer again. The drop-target ring stays visible while you wait. Press `Esc` to cancel the drag.
- Files dragged in from your OS file manager and dropped on an Explorer cell are **copied** into its folder (never moved). A remote Explorer refuses them, because they are on a different computer. On Linux, incoming file drags require X11/XWayland. On X11 the whole-window hint stays while you drag, but dropping on an Explorer cell still copies.

On Windows, macOS, and Linux, drag local Explorer items outside the window to **copy** them to another app. Linux supports X11 and native Wayland. File icons and pointer feedback come from the operating system. Originals remain in place, and external copies do not enter Tasty's undo history. Returning to the source window does not also trigger an internal move. Remote items cannot be exported. On X11, the Explorer **Clear selection** binding (by default `Esc`) cancels the drag. On Windows, macOS, and Wayland, the operating system handles drag cancellation.

The Windows and macOS implementations have been checked by compilation; their behavior on real machines has not yet been measured. Other windows in the same Tasty process cannot receive it. A separately running Tasty instance may appear as an external receiver, but dragging files between Tasty windows is not a supported feature. A start or communication error shows a notice; starting a drag is never reported as a completed copy.

### Progress and results

Paste, drag and drop, and Move to Trash run one at a time, in order. Copy and move first count the files, showing **Counting items to copy** or **Counting items to move** and updating how many have been found, without a progress bar. Once counted, progress includes files inside folders and names the file being processed. The bar follows bytes, or the file count when all files are empty or the total size is unknown. Result counts still refer to the items originally requested. If you picked a folder together with items inside it (for example from a subfolder search), the inner items go along with the folder and aren't counted separately. Items reached through a link are still handled on their own, even when the link is picked too.  The status line of the Explorer that started the job shows a progress bar, the current file and the size; **×** stops it. Stopping keeps finished items and skips the rest. Jobs waiting their turn show a **+1 queued** tag; click it to see the queue and remove waiting jobs. If too many jobs are waiting, or you picked too many items for one job, the job isn't taken and a warning card in that Explorer says so; when the queue is full, **Show queue** on the card opens the list.

When a name already exists, a question opens in that Explorer if it has focus. If you were looking elsewhere, click **Show** in its status line.

- **Keep both** (`Enter`) — the default. The new item gets `(copy)`; the existing one is left alone.
- **Skip** · **Replace** (files only) · **Cancel the rest** (`Esc`).
- Folders with the same name are never merged; for folders you can only Skip or Keep both.
- With more conflicts left, turn on **Do this for the other conflicts** to answer them all at once.

When it finishes, a result card appears at the bottom right of the Explorer. Cards for finished jobs fade after a moment; cards with failed or skipped items stay until you close them and list those paths. The card of a job you stopped with **×** also stays until you close it when items were left undone. If the hidden temporary folder used while copying (its name starts with `.tasty-copy-`) can't be removed, the card turns into a warning card, lists that path with "Temporary folder couldn't be removed" and the reason, and stays until you close it; fix the cause and delete that folder yourself.

- **Retry** — runs only the failed or skipped items and the items left undone when you stopped the job. Finished items are not run again. For a move that left originals behind, it only deletes those originals again.
- **Copy paths** — copies the card's paths to the clipboard.
- **Undo** — shown while the card is up, after a copy or move that fully finished. Moved items go back (unless something else now sits there), and copies go to the trash. A copy you edited after the job is kept. Files you replaced can't be restored.

If a move across disks can't delete all of the original (for example a subfolder you can't write to), a warning card says the original is still there, in whole or in part, and stays until you close it. The copy is then the complete data, so there is no Undo. Once whatever blocked the delete is fixed, **Retry** deletes only what is left of the originals. If the copy has gone or been replaced since, or you edited the original file, the original is kept and the card says why. If the copy's path leads back to the original itself (for example through a link), nothing is deleted. For a folder, only items whose content is the same in the copy are deleted (large files show progress while they are compared, and you can cancel; the originals still left then stay on the warning card so you can retry later); anything you added to or changed in the original folder after the move is kept, and the card says how many. The status line says **Checking** while it compares and **Removing originals** while it deletes, and when every original is gone a card briefly says how many were removed. One Retry gives one result card, even when it both deletes originals and moves items again.

## Markdown

Renders `.md` files. When the file changes, it redraws automatically within 1 second.

- Tables · checkboxes · footnotes · code highlighting · `mermaid` diagrams · `$…$` math · `> [!NOTE]` callouts (including Obsidian-style `[!tip]-` folding) · frontmatter at the top hidden.
- If there are headings, a collapsible **Table of contents** is attached above the body. Clicking an entry moves down to that heading within the document you are already reading — and clicking the same entry again moves there again.
- Hover a code block for the **Copy code** button.
- The **Find** key from the [Keyboard shortcuts](../customize/keybindings.md) settings (default `Ctrl+F` · `Alt+F`, macOS `Ctrl+F` · `Cmd+F`) while the document has focus — **Find in document**. Changing the Find key applies to open documents right away.
- Select all and copy act only on what has focus. Select all in the document picks only the document text, without the address bar or find box text; in the address bar or find box it picks only that field's text.
- Type another file path in the address bar at the top and press `Enter` or **Go** to move to it. Recently opened files appear as autocompletion. Files opened in other windows of the same Tasty instance are included, with up to 10 entries in most recently opened order. When the Surface is narrow the Go button hides, but `Enter` still moves.
- The folder button left of the address bar (**Open a file…**) opens the file chooser in the current document's folder. The file you pick opens in the same Surface, without a new tab. Cancelling leaves the document as it was. If a file chooser is already open, a new one doesn't open, so finish or close that one first. On macOS, pressing the folder button with the keyboard can leave the file chooser hidden behind the document, so click it with the mouse. Whether it appears right away after a mouse click has not been confirmed on a real Mac yet. The address bar of a Markdown document in a remote Workspace is described in [Working remotely](../remote/attach.md).
- Links in the document — other Markdown · files open in a new Tab in the same Pane, and on Linux Tasty switches to it (on macOS it is meant to switch only for links you click with the mouse, but this has not been confirmed on a real Mac yet; a link opened with the keyboard only adds the new Tab behind the current one), and `http(s)://` goes to the browser. Relative paths are relative to the folder the document is in. If no handler can open a linked file, the **Open file with…** window appears so you can choose (in a remote Workspace it lists only handlers that can open the file on the remote, and a notice appears when there are none). Links that point inside the same document, such as `#heading`, and footnote numbers and their back arrows only move to that spot.
- Press the **Refresh** key (default `F5`) to re-read and redraw only the document you are looking at. It also works while you are typing in the address bar or the find box. Because the document is drawn again, what you were typing in the address bar, the open find box and its query, and the selection are cleared; the scroll position is restored when possible. A document in a remote Workspace fetches the original from the remote again; while disconnected, the disconnected state stays as it is. Large files go through the same confirmation described below.
- Files over 1 MiB are asked about once with **Open large file?**. The prompt appears in the middle of that Markdown surface, or in the middle of the window when the surface is narrower than the prompt. A file that was small when opened is asked about the same way before it is re-read once it grows past 1 MiB, and its previous content stays visible until you answer. If you cancel, the document area shows **This file is larger than 1 MiB and has not been loaded**, with an **Open file…** button below it that asks again. A file you cancelled is not re-read automatically when it changes; `tasty markdown reload --surface <ID>` also asks again. Once you choose **Open**, that file is not asked about again.

**New Markdown...** or `Alt+'` > **Markdown** opens the **Open Markdown File** window. Type a path or choose one with **Browse…**. Browse starts in the current folder of the terminal you are looking at, and a relative path you type is opened from that folder too. The window appears centered over that surface, and it hides while you switch to another workspace or tab and comes back as you left it when you return. The file chooser opened by Browse hides and returns with it, keeping your selected files and current folder. While hidden, it does not block clicks or keyboard input on the other screen.

```sh
tasty markdown recent
tasty markdown reload --surface 5
```

## Image

View PNG, JPEG, and other images, or add drawings to them. **New Image** starts with an empty canvas. Even when you create it while an image is open, it is separate from that file, so **Save** asks where to save it. Once you save it to a new path, the tab is renamed to that file, and that file is what opens after a restart. When the file changes outside Tasty, it is re-read automatically within 1 second — except **while you are editing**, where it is deferred until you leave edit mode (so the picture underneath your strokes does not change).

- Toolbar — **Previous image** / **Next image** (within the same folder; the tab is renamed to the file you move to. They do nothing while you are editing — save or cancel first; `tasty image next` and `prev` return an error then), **Refresh**, **Edit**, **New image**, zoom **Fit** / `+` / `-`.
- Press **Edit** to choose **Brush** · **Color** and draw on top. Undo and redo with `Ctrl+Z` / `Ctrl+Shift+Z`. **Save** writes a PNG. When you save edits to a file that is not a PNG, such as a JPEG, the original stays as it is: a PNG with the same name is created in the same folder and the tab moves to it. If that PNG already exists, Tasty does not overwrite it and asks where to save instead. The prompt then says the file already exists, and the field holds a free name such as `name-1.png` with the name part selected.
- The folder button in **Save As** opens the Tasty file picker. Choose a PNG or type a name and confirm to fill in the save path. An existing filename shows an overwrite warning. The file is written when you press **Save** in the image’s **Save As** form; cancelling Browse keeps the path you had entered.
- When a file cannot be read, the canvas says why — **Image not found** (with the path), **Permission denied**, **Can't open this image** (damaged or unsupported, with the decoder message), or **Image is too large to open** (an image over 16384px on a side, or one that needs more than 512 MiB to decode, is not opened, and its real size is shown). Except for a too large image, **Retry** reads it again. The file is also re-read automatically when it comes back or changes.
- Pressing the paste shortcut (`Ctrl+V` by default) on an image pastes the clipboard image as a floating selection. Nothing happens when the clipboard holds no image. `tasty image paste --surface <ID>` does the same.
- `tasty image list` shows every open image **across all windows**.
- `tasty image reload --surface <ID>` re-reads the file right now instead of waiting.

```sh
tasty image list
tasty image open --surface 5 shot.png
tasty image export --surface 5 out.png
```

## HTML

When working folder inheritance is enabled, new tabs and splits created from an HTML file tab use that file’s folder. Tabs showing web addresses keep their existing working folder.

You can also use `tasty html open` to change an HTML tab’s requested address on a headless server. A headless server does not render the page, and HTML content is not yet displayed over a remote connection.

Shows a local HTML file or a URL in the OS web view. Right-click an empty area of the tab strip > **New HTML...** opens the file chooser in that Pane's current folder, showing only folders and `.html` · `.htm` · `.svg` files. Pick a file to open an HTML Tab in that Pane; cancel and no Tab is created. In a remote Workspace the file chooser does not open, and a notice says the file can't be opened yet.

- While loading, **Loading…** is shown; on failure, **Failed to load** and the URL.
- While a menu or popup is open, HTML and Markdown pages stay visible on Linux. When the desktop composites windows (a compositor is running), the popup appears over the page as usual, with its rounded corners, shadow and the dimming behind it. Without compositing, or if the window manager cannot keep the popup in place, only the popup's own area is covered by a square-cornered box. This can happen with a full-screen or partly off-screen window. On Windows and macOS, the page stays visible and the popup uses the same square-cornered box. Clicking or scrolling the page during that time counts as a click outside the popup and does not reach the page, and key presses don't go to the page either. On Linux when the X server lacks the window shape extensions (SHAPE and XFixes), the pages are hidden for a moment and only an empty panel shows while a popup is open. When a notice card appears, only the pages it overlaps are hidden until the card goes away; other pages stay visible. A page you are typing into is not hidden, so a notice card over it stays covered.
- The **HTML** item under **Settings** > **Appearance** — **Default zoom** · **Color scheme** (**Follow theme** / light / dark) · **Allow remote content** · **Sandbox scripts**. Remote content and scripts are blocked by default. It is meant for viewing previews built locally, so to open external sites you need to turn **Allow remote content** on and **Sandbox scripts** off.

### Address bar

The top of an HTML view has an address bar: the **Back** · **Forward** · **Reload** buttons and the address field. A local file shows as its file path (your home folder as `~`), a web document as its URL.

- Type a file path or a `file://` · `http://` · `https://` address in the field and press `Enter` to open it. There is no separate Go button.
- A relative path is resolved against the current document: the folder of a local file, or the address of a web document. In an empty HTML tab it uses the tab's working folder; without any base, Tasty does not open it and tells you why.
- A file that does not exist is not opened, and **File not found** appears under the field. The page you were viewing and what you typed stay; the notice goes away on your next keystroke or `Esc`.
- `Esc` while editing cancels your input and returns to the current document's address. While you type in the address field, keys go neither to the page nor to the terminal or Tasty shortcuts.
- In the address field, `Tab` and `Shift+Tab` move in turn between the address field and whichever of **Back**, **Forward** and **Reload** can be pressed, and the focused button shows an outline. `Enter` or `Space` presses that button, and `Esc` leaves. Moving to a button returns what you typed to the current address, and coming back to the address field selects the whole address. If the pressed button dims or turns into the loading indicator, the button loses focus.
- `Alt+L` (macOS `Cmd+L`) goes to the address field of the HTML view that has the focus, with the whole address selected. It works even while you are typing inside the page. Change the key under **Settings** > **Keybindings** > **Surface** at **Focus the address bar**; the Explorer's address bar uses the same key.
- While a page loads, a spinner takes the Reload button's place; if it fails to load, the field keeps the address you tried to open.
- Back is dimmed at the start of the history and Forward at its end. When the HTML view is narrower than 440px, the Forward button is hidden.
- An empty HTML tab shows a hint for the address field with its shortcut. After a restart, a document opened from the address bar returns to the address the tab was created with (an empty tab stays empty), as does an address an agent opened with `tasty html open`.
- HTML tabs in a remote workspace have no address bar.

### Documents with blocked remote content

With **Allow remote content** off, viewing a local document that uses stylesheets, images, fonts and the like from the network shows a **Remote content in this document is blocked** notice under the address bar. Remote content is not loaded, so the page may look unstyled. Like the script notice, it does not cover the page; it pushes the page down by its height.

- **Allow remote content** — reloads this document once with its remote content allowed. Navigating to another document, changing the file's contents, or restarting Tasty blocks it again. It is separate from allowing scripts. It can't be pressed while the document is loading.
- When you close the notice with **×** or allow remote content, a globe icon stays at the right end of the address field. A blocked document shows a crossed-out globe; press it to show the notice again (pressing it does not allow anything). An allowed document shows a globe, and hovering it only says that remote content is allowed.
- A document with both remote content and scripts blocked shows two separate notices, remote content first, then scripts.
- A document opened by an agent shows the notice when a person looks at that tab. An agent can allow the remote content of a queried document with `tasty surface html-remote-allow`.
- With the **Allow remote content** setting on, every document loads remote content and there is no notice or icon. HTML tabs in a remote workspace have no such notice.

### Documents with blocked scripts

With **Sandbox scripts** on, viewing a document that contains scripts shows a **Scripts in this document are blocked** notice above the page. The notice does not cover the page; it pushes the page down by its height. Its text is never cut off: when a narrow view wraps it onto more lines, the notice grows and the page moves down with it. When the HTML view is narrow, the button moves to the line below the description.

- **Allow for this document** — turns scripts on for this document only and reloads it once. Navigating to another document or restarting Tasty blocks them again. The tab of an allowed document gets a script file icon.
- While the view is loading a new document, **Allow for this document** can't be pressed. Hovering it shows **Available when the document finishes loading**. When a document you opened yourself (a link, going back, or reloading) has scripts, the button comes back; when it has none, the notice goes away. A document an agent opened shows the notice when you look at that tab again. For a very large file, the button can stay off for a moment after the page appears while Tasty checks the whole file. The check does not pause other terminals or the screen. When you reload a document you allowed scripts for, the new document appears after the check finishes.
- **×** at the top right of the notice — closes the notice without allowing. A lock icon stays after the tab title; press the lock to show the notice again. Pressing the lock does not switch tabs.
- A document opened by an agent, or reopened by session restore, shows the notice when a person looks at that tab. An agent can also query the state and then allow that document's scripts with `tasty surface html-script-allow`. It is refused when the document changed after the query or is still loading. Allowing keeps the tab you were looking at and the focus as they are.
- While **Failed to load** is shown because the document could not be loaded, the notice and the tab's lock or script icon are removed. When a reloaded document opens, they are decided again for that document.

```sh
tasty html open --surface 5 ./dist/index.html
tasty new tab --pane 1 --type html --url http://localhost:3000
tasty surface html-script --surface 5   # script detection, allowance and notice state
tasty surface html-script-allow --surface 5 --url <url> --fingerprint <hex>   # allow the scripts of the queried document
tasty surface html-remote-allow --surface 5 --url <url> --fingerprint <hex>   # allow the remote content of the queried document
```

## Git view

Press **Tools** > **Git** in the sidebar and a window appears showing the repository of the current terminal directory, read-only.

- **Changes** (status) · **Commits** (log) · **Diff** when you click a file.
- Pick another worktree from the **Worktrees** list on the left to view relative to it. The actual checkout does not change.
- **Refresh** picks up external changes. There are no writes such as commit · staging.
- The **Open Git Viewer** shortcut is assigned under **Settings** > **Keybindings** > **Plugins** (no default).

The status bar can show the current repository’s branch independently of this window ([A first look](../getting-started/first-look.md#status-bar)).

## Empty Surface

A slot whose file was closed or whose kind has not been chosen yet remains **Empty**, with a kind-selection button in the middle. Pressing it is the same as the [Changing the kind](panes-tabs-splits.md#changing-the-kind) popup.

The Explorer is built into Tasty; Markdown · image · HTML · Git view are bundled plugins. If you disable a plugin, you cannot open that kind — [Plugins](../plugins/index.md).
