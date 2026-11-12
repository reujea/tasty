---@meta
--- LuaLS definitions for registered Tasty scripts.
--- Add this directory to workspace.library in .luarc.json for completion and type checking.
--- Example: { "workspace.library": ["<TASTY_REPO>/crates/tasty-lua/meta"] }

---@class tasty
tasty = {}

--- Register a callback for a host event. Multiple callbacks per event are
--- supported (fired in registration order). Errors in one callback are logged
--- and do not abort sibling callbacks. Tasty hooks are *observe-only* — the
--- callback's return value is ignored and cannot cancel/alter host behavior.
---@param event TastyEvent     -- canonical event name (see EventMatrix below)
---@param callback fun(ctx: table): nil
function tasty.on(event, callback) end

--- Log at INFO level via tracing.
---@param msg string
function tasty.log(msg) end

--- Log at WARN level via tracing.
---@param msg string
function tasty.warn(msg) end

--- Ask the main thread to spawn the Tasty CLI without waiting for completion. Stdio is nulled.
---@param args string[]
function tasty.run_cli(args) end

--- Return a copy of the workspace tree the main thread published last. The main
--- thread publishes it once per event loop pass, so a change requested with
--- tasty.run_cli is not guaranteed to appear in the next call. Each entry is
--- one workspace with the same fields as IPC `list tree`. The workspaces of
--- each session (engine) are appended in turn: sessions with a window first, in
--- no fixed order, then parked sessions kept without a window (last window
--- closed, window minimized on macOS) in the order they were kept. Changing the
--- returned table does not change Tasty.
---
--- Focused surface of a session: the workspace with `active`, then its pane with
--- `focused`, then that pane's tab with `active`, then `tab.surface.focused_surface`
--- when `tab.surface.type == "SplitLayout"`, otherwise `tab.surface.id`.
---
--- Limit: `active` and `focused` are decided inside each session (a window or
--- a parked session) and no field names the window or session, so with several
--- sessions more than one workspace can have `active = true`. The tree cannot
--- tell which window has the OS focus or which session is on screen; the lookup
--- above can return a surface of another window, or of a parked session that is
--- not shown when every window is closed or minimized on macOS.
---@return TastyTreeWorkspace[]
function tasty.tree() end

-- ─────────────────────────────────────────────────────────────────────────────
-- Event names
-- ─────────────────────────────────────────────────────────────────────────────

---@alias TastyEvent
---| "tasty.startup.post"
---| "window.create.post"
---| "window.delete.post"
---| "workspace.create.post"
---| "workspace.delete.post"
---| "workspace.change.post"
---| "tab.create.post"
---| "tab.delete.post"
---| "tab.change.post"
---| "pane.create.post"
---| "pane.delete.post"
---| "surface.create.post"
---| "surface.delete.post"

-- ─────────────────────────────────────────────────────────────────────────────
-- Payload shapes
-- (Field names mirror tasty-plugin-protocol payloads. Optional fields use ?.)
-- ─────────────────────────────────────────────────────────────────────────────

---@class WindowCreated
---@field window_id integer
---@field kind string
---@field modality "modeless"|"modal"

---@class WindowClosed
---@field window_id integer
---@field reason string

---@class WorkspaceCreated
---@field workspace_id integer
---@field window_id integer
---@field name string

---@class WorkspaceClosed
---@field workspace_id integer
---@field reason string

---@class WorkspaceRenamed
---@field workspace_id integer
---@field name string|nil
---@field subtitle string|nil
---@field description string|nil

---@class TabCreated
---@field tab_id integer
---@field pane_id integer
---@field workspace_id integer
---@field kind string

---@class TabClosed
---@field tab_id integer
---@field pane_id integer
---@field reason string

---@class TabRenamed
---@field tab_id integer
---@field title string

---@class PaneCreated
---@field pane_id integer
---@field parent_pane_group integer|nil
---@field workspace_id integer

---@class PaneClosed
---@field pane_id integer
---@field reason string

---@class SurfaceCreatedBy
---@field kind "user"|"agent"
---@field source_plugin string|nil

---@class SurfaceCreated
---@field surface_id integer
---@field kind string
---@field tab_id integer
---@field pane_id integer
---@field workspace_id integer
---@field created_by SurfaceCreatedBy

---@class SurfaceClosed
---@field surface_id integer
---@field kind string
---@field reason string

-- ─────────────────────────────────────────────────────────────────────────────
-- tasty.tree() shapes
-- (Same fields as IPC `list tree`.)
-- ─────────────────────────────────────────────────────────────────────────────

---@class TastyTreeWorkspace
---@field id integer
---@field name string
---@field active boolean           -- the selected workspace of its session (window or parked session)
---@field busy_count integer       -- busy surfaces in this workspace
---@field panes TastyTreePane[]

---@class TastyTreePane
---@field id integer
---@field focused boolean          -- the focused pane of its workspace
---@field busy_count integer       -- busy surfaces in every tab of this pane, selected or not
---@field tabs TastyTreeTab[]

---@class TastyTreeTab
---@field id integer
---@field name string
---@field active boolean           -- the selected tab of its pane
---@field busy_count integer       -- busy surfaces in this tab
---@field busy boolean             -- true when any surface in this tab is busy
---@field surface TastyTreeSplitLayout|TastyTreeSurface

--- Content of a tab whose surfaces are split.
---@class TastyTreeSplitLayout
---@field type "SplitLayout"
---@field focused_surface integer  -- the focused surface of this tab
---@field surfaces integer[]       -- every surface id in the tab
---@field layout TastyTreeLayoutNode

---@class TastyTreeLayoutNode
---@field type "Leaf"|"Split"
---@field id integer|nil           -- Leaf only
---@field kind string|nil          -- Leaf only
---@field busy boolean|nil         -- Leaf only
---@field direction "horizontal"|"vertical"|nil -- Split only
---@field ratio number|nil         -- Split only
---@field focus_second boolean|nil -- Split only; the hint set when the split was made or restored, not the current focus (see TastyTreeSplitLayout.focused_surface)
---@field first TastyTreeLayoutNode|nil  -- Split only
---@field second TastyTreeLayoutNode|nil -- Split only

--- Content of a tab with one surface. Every surface has `id` and `type`; the
--- other fields depend on the surface type. A JSON null arrives as the mlua
--- null value (a light userdata), not as nil.
---@class TastyTreeSurface
---@field id integer
---@field type "Terminal"|"Empty"|"Pending"|"DAG"|"Explorer"|"Remote"|"AttachMesh"|"EguiMesh"|string
---@field busy boolean             -- false for the Pending stand-in
---@field kind string|nil          -- every type except Terminal and the {id, type = "Pending"} stand-in for a surface the engine cannot find
---@field pty_ready boolean|nil    -- Terminal (true) and restoring Pending (false)
---@field cols integer|nil         -- Terminal; always 0 in this tree
---@field rows integer|nil         -- Terminal; always 0 in this tree
---@field ready boolean|nil        -- Pending waiting for its plugin; always false
---@field pending_reason "plugin_not_loaded"|nil -- Pending waiting for its plugin
---@field surface_id integer|nil   -- Pending still being restored at startup; same as id
---@field restore_error string|lightuserdata|nil -- Pending still being restored at startup; null unless recovery is blocked
---@field direction "lr"|"td"|nil  -- DAG
---@field dag_id string|nil        -- DAG; only when bound to a DAG
---@field workspace_id integer|nil -- DAG; only when bound to a workspace
---@field plugin_id string|nil     -- Remote, AttachMesh, EguiMesh
---@field display_name string|nil  -- Remote, AttachMesh, EguiMesh
